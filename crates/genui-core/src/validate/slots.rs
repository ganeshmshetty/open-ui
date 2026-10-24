//! Retained slot positions and prefix counts. Source positions stay fixed even
//! while a waiting reference contributes zero rendered children.
use super::*;
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct SlotKey {
    pub parent: NodeId,
    pub name: String,
}
#[derive(Clone)]
pub(super) struct SlotItem {
    pub source: std::sync::Arc<Expr>,
    pub ctx: Context,
    pub path: NodePath,
    pub output: crate::SharedVector<NodeId>,
    pub diagnostics: Vec<Diagnostic>,
    pub reads: BTreeSet<String>,
    pub planned: bool,
}
#[derive(Clone, Default)]
pub(crate) struct PrefixCounts {
    len: usize,
    tree: crate::SharedMap<usize, usize>,
    values: crate::SharedMap<usize, usize>,
}
impl PrefixCounts {
    pub(crate) fn empty(len: usize) -> Self {
        Self {
            len,
            ..Self::default()
        }
    }
    pub(super) fn new(counts: &[usize]) -> Self {
        let mut result = Self {
            len: counts.len(),
            ..Self::default()
        };
        for (i, n) in counts.iter().enumerate() {
            result.set(i, *n);
        }
        result
    }
    pub fn prefix(&self, end: usize) -> usize {
        let mut index = end.min(self.len);
        let mut sum = 0;
        while index > 0 {
            sum += self.tree.get(&index).copied().unwrap_or(0);
            index -= 1 << index.trailing_zeros();
        }
        sum
    }
    pub fn set(&mut self, index: usize, value: usize) {
        let old = self.values.get(&index).copied().unwrap_or(0);
        if old == value {
            return;
        }
        self.values.insert(index, value);
        let mut i = index + 1;
        while i <= self.len {
            let current = self.tree.get(&i).copied().unwrap_or(0);
            self.tree.insert(i, current.saturating_sub(old) + value);
            i += 1 << i.trailing_zeros();
        }
    }
}
#[derive(Clone)]
pub(super) struct SlotCheck {
    pub parent_path: NodePath,
    pub arg: crate::r#lib::ArgDef,
    pub depth: usize,
    pub place: String,
    pub items: crate::SharedMap<usize, SlotItem>,
    pub counts: PrefixCounts,
}
impl Validator<'_> {
    pub(super) fn record_slot(&mut self, key: SlotKey, check: SlotCheck) {
        if let Some(old) = self.slots.get(&key) {
            for (i, item) in &old.items {
                for name in &item.reads {
                    if let Some(users) = self.slot_readers.get_mut(name) {
                        users.remove(&(key.clone(), *i));
                    }
                }
            }
        }
        for (i, item) in &check.items {
            for name in &item.reads {
                if let Some(users) = self.slot_readers.get_mut(name) {
                    users.insert((key.clone(), *i));
                } else {
                    self.slot_readers
                        .insert(name.clone(), std::iter::once((key.clone(), *i)).collect());
                }
            }
        }
        self.slots.insert(key, check);
    }
}
#[derive(Clone)]
pub(crate) struct Graft {
    pub parent: String,
    pub slot: String,
    pub index: usize,
    pub children: crate::SharedVector<NodeId>,
    pub remove: Option<String>,
    pub order: Vec<(String, usize, usize)>,
}
pub(crate) struct StructuralAttempt {
    pub result: Option<(Validation, ValidationCache, Vec<Graft>)>,
    pub work: crate::MaintenanceWork,
}
/// Resolve previously empty static entries. Retained positions supply insertion
/// indices without scanning preceding siblings. More general structural and
/// allocation changes remain explicit I7 work, rather than accepted shortcuts.
pub(crate) fn resolve_waiting(
    cached: &ValidationCache,
    base: &Validation,
    changes: &[Statement],
    dirty: &BTreeSet<String>,
    sealed_states: Option<&crate::SharedMap<String, StateDef>>,
    library: &Library,
    options: &Options,
) -> StructuralAttempt {
    if cached.tasks.is_some() {
        return StructuralAttempt {
            result: None,
            work: Default::default(),
        };
    }
    let mut work = crate::MaintenanceWork::default();
    if changes.is_empty()
        || (cached.ambiguous_states && sealed_states.is_none())
        || cached.allocation_limited
        || changes.iter().any(|s| s.kind != StatementKind::Bind)
    {
        return StructuralAttempt { result: None, work };
    }
    let mut v = Validator::new(&Program::default(), Vec::new(), library, options);
    v.table = cached.table.clone();
    v.state_sources = cached.state_sources.clone();
    v.kinds = cached.kinds.clone();
    v.cache = cached.checked.clone();
    v.action_sources = cached.action_sources.clone();
    v.reachable = cached.reachable.clone();
    v.instances = cached.instances.clone();
    v.readers = cached.readers.clone();
    v.value_diagnostics = cached.value_diagnostics.clone();
    v.slots = cached.slots.clone();
    v.slot_readers = cached.slot_readers.clone();
    v.ir = base.ir.clone();
    v.sealed_states = sealed_states;
    v.ids = cached.ids.clone();
    let mut diagnostics = cached.diagnostics.clone();
    let mut entries = BTreeSet::new();
    for name in dirty {
        if cached
            .kinds
            .get(name)
            .is_some_and(|kind| *kind != Kind::Node)
        {
            return StructuralAttempt { result: None, work };
        }
        if cached
            .readers
            .get(name)
            .is_some_and(|readers| !readers.is_empty())
        {
            return StructuralAttempt { result: None, work };
        }
        if let Some(users) = cached.slot_readers.get(name) {
            entries.extend(users.iter().cloned());
        }
    }
    if entries.is_empty() {
        return StructuralAttempt { result: None, work };
    }
    for s in changes {
        work.statements_checked += 1;
        let mut deps = BTreeSet::new();
        crate::session::dependencies(&s.value, &mut deps);
        // Reachability/type/alias changes need the general task-maintenance path.
        if deps.iter().any(|name| !name.starts_with('$')) || v.classify(&s.value) != Kind::Node {
            return StructuralAttempt { result: None, work };
        }
        v.table.insert(s.name.clone(), s.clone());
        v.kinds.insert(s.name.clone(), Kind::Node);
        v.ir.templates.insert(s.name.clone(), s.value.clone());
    }
    let mut grafts = Vec::new();
    for (key, index) in entries {
        let Some(check) = v.slots.get(&key).cloned() else {
            return StructuralAttempt { result: None, work };
        };
        let Some(item) = check.items.get(&index).cloned() else {
            return StructuralAttempt { result: None, work };
        };
        if item.planned || !item.output.is_empty() || !base.ir.nodes.contains_key(&key.parent) {
            return StructuralAttempt { result: None, work };
        }
        v.active.clear();
        // Ancestors are context, not work on unrelated descendants.
        let mut ancestor = Some(check.parent_path.0.as_str());
        while let Some(path) = ancestor {
            let id = v.ids.allocate(&NodePath(path.into()));
            if let Some(node) = base.ir.nodes.get(&id) {
                v.active.insert(node.origin.clone());
            }
            ancestor = path.rsplit_once('/').map(|(head, _)| head);
        }
        v.allocated = v.ir.nodes.len();
        let start = v.diagnostics.len();
        let ids = v.walk_expr(
            &item.source,
            &item.ctx,
            item.path.clone(),
            check.depth,
            Some(&check.place),
        );
        if !v.pending_plans.is_empty()
            || v.diagnostics[start..]
                .iter()
                .any(|d| matches!(d.code, Code::Nodes | Code::IrDepth))
        {
            work += v.work;
            return StructuralAttempt { result: None, work };
        }
        let only = match check.arg.ty.slot() {
            Some(crate::r#lib::Accepts::Only(allowed)) => Some(allowed),
            _ => None,
        };
        let mut accepted = Vec::new();
        for id in ids {
            let allowed = only.is_none_or(|allowed| {
                v.ir.nodes
                    .get(&id)
                    .and_then(|node| library.components().get(node.kind.0 as usize))
                    .is_some_and(|def| allowed.contains(&def.name))
            });
            if allowed {
                accepted.push(id);
            } else {
                let mut at = item.ctx.clone();
                at.path = v.ir.nodes.get(&id).map(|n| n.path.clone());
                v.emit(Code::SlotKind, &at);
                v.remove_subtree(id);
            }
        }
        let output: crate::SharedVector<_> = accepted.into();
        let offset = check.counts.prefix(index);
        for d in &item.diagnostics {
            diagnostics.remove_raw(d);
            work.diagnostics_examined += 1;
        }
        let Some(parent) = v.ir.nodes.get_mut(&key.parent) else {
            work += v.work;
            return StructuralAttempt { result: None, work };
        };
        let Some(slot) = parent.slots.iter_mut().find(|slot| slot.name == key.name) else {
            work += v.work;
            return StructuralAttempt { result: None, work };
        };
        let mut suffix = slot.children.split_off(offset);
        slot.children.append(output.iter().copied().collect());
        slot.children.append(std::mem::take(&mut suffix));
        let mut next = check;
        next.counts.set(index, output.len());
        let order = item.ctx.order.clone();
        let mut item = item;
        item.output = output.clone();
        item.diagnostics = v.diagnostics[start..].to_vec();
        next.items.insert(index, item);
        v.slots.insert(key.clone(), next);
        grafts.push(Graft {
            parent: parent.path.0.clone(),
            slot: key.name,
            index: offset,
            children: output,
            remove: None,
            order,
        });
    }
    work += v.work;
    work.diagnostics_examined += v.diagnostics.len() as u64;
    diagnostics.extend_raw(v.diagnostics);
    let cache = ValidationCache {
        table: v.table,
        state_sources: v.state_sources,
        query_sources: cached.query_sources.clone(),
        query_diagnostics: cached.query_diagnostics.clone(),
        orphan_diagnostics: cached.orphan_diagnostics.clone(),
        query_budget: cached.query_budget.clone(),
        kinds: v.kinds,
        checked: v.cache,
        action_sources: v.action_sources,
        reachable: v.reachable,
        instances: v.instances,
        readers: v.readers,
        value_diagnostics: v.value_diagnostics,
        diagnostics: diagnostics.clone(),
        ids: v.ids,
        slots: v.slots,
        slot_readers: v.slot_readers,
        ambiguous_states: cached.ambiguous_states,
        allocation_limited: cached.allocation_limited,
        tasks: None,
        state_bytes: cached.state_bytes,
        state_budget: cached.state_budget.clone(),
        state_candidates: cached.state_candidates.clone(),
        state_limited: cached.state_limited,
    };
    StructuralAttempt {
        result: Some((
            Validation {
                ir: v.ir,
                diagnostics,
            },
            cache,
            grafts,
        )),
        work,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefix_counts_are_persistent_and_include_zero_width_entries() {
        let counts = PrefixCounts::new(&[0, 2, 0, 1, 3]);
        let mut next = counts.clone();
        next.set(0, 1);
        next.set(3, 0);
        assert_eq!(
            (0..=5).map(|i| counts.prefix(i)).collect::<Vec<_>>(),
            vec![0, 0, 2, 2, 3, 6]
        );
        assert_eq!(
            (0..=5).map(|i| next.prefix(i)).collect::<Vec<_>>(),
            vec![0, 1, 3, 3, 3, 6]
        );
    }
}

#[cfg(test)]
mod structural_tests {
    use super::*;
    use crate::{
        ir::Role,
        r#lib::{ComponentDef, GroupDef},
    };

    fn library() -> Library {
        Library::builder("test", "0.1.0")
            .group(GroupDef::new("G", 0, "`Leaf` and `Box`."))
            .component(
                ComponentDef::new("Leaf", Role::Paragraph)
                    .group("G")
                    .doc("A leaf.")
                    .req("text", T::text())
                    .example("Leaf(\"ok\")"),
            )
            .component(
                ComponentDef::new("Box", Role::Group)
                    .group("G")
                    .doc("A box.")
                    .req("children", T::nodes())
                    .example("Box([])"),
            )
            .build()
            .unwrap()
    }

    fn parse(text: &str) -> Program {
        crate::syntax::parse_batch(text.as_bytes(), crate::limits::SyntaxLimits::default()).program
    }

    fn base(lib: &Library, opt: &Options) -> (Validation, ValidationCache) {
        let program = parse("root=Box([])");
        let (validation, _, cache) = Validator::new(&program, Vec::new(), lib, opt).run_measured();
        (validation, cache)
    }

    fn waiting(
        lib: &Library,
        opt: &Options,
    ) -> (Validation, ValidationCache, SlotKey, crate::ir::NodeId) {
        let (validation, mut cache) = base(lib, opt);
        let root = validation.ir.root.expect("root box");
        let key = SlotKey {
            parent: root,
            name: "children".into(),
        };
        let arg = lib
            .component("Box")
            .and_then(|(_, def)| def.args.iter().find(|a| a.name == "children").cloned())
            .expect("children arg");
        let path = NodePath("root/children:0".into());
        let mut item = SlotItem {
            source: std::sync::Arc::new(Expr::Name { name: "w".into() }),
            ctx: Context::at("root", Some(NodePath::root())),
            path: path.clone(),
            output: Default::default(),
            diagnostics: Vec::new(),
            reads: ["w".to_string()].into_iter().collect(),
            planned: false,
        };
        item.ctx.order.push(("children".into(), 0, 0));
        cache.slots.insert(
            key.clone(),
            SlotCheck {
                parent_path: NodePath::root(),
                arg,
                depth: 2,
                place: "children".into(),
                items: [(0, item)].into_iter().collect(),
                counts: PrefixCounts::new(&[0]),
            },
        );
        cache
            .slot_readers
            .insert("w".into(), [(key.clone(), 0)].into_iter().collect());
        cache.kinds.insert("w".into(), Kind::Node);
        (validation, cache, key, root)
    }

    fn change(text: &str) -> Statement {
        parse(text).statements.into_iter().next().unwrap()
    }

    fn run(
        lib: &Library,
        opt: &Options,
        cache: &ValidationCache,
        base: &Validation,
        changes: &[Statement],
        dirty: &BTreeSet<String>,
        sealed: Option<&crate::SharedMap<String, StateDef>>,
    ) -> StructuralAttempt {
        resolve_waiting(cache, base, changes, dirty, sealed, lib, opt)
    }

    #[test]
    fn record_slot_tracks_and_replaces_item_readers() {
        let lib = library();
        let opt = Options::default();
        let (validation, cache, key, _) = waiting(&lib, &opt);
        let mut v = Validator::new(&Program::default(), Vec::new(), &lib, &opt);
        v.ir = validation.ir.clone();
        v.record_slot(key.clone(), cache.slots.get(&key).unwrap().clone());
        assert!(
            v.slot_readers
                .get("w")
                .is_some_and(|s| s.contains(&(key.clone(), 0)))
        );
        assert!(v.slots.contains_key(&key));

        let mut old = cache.slots.get(&key).unwrap().clone();
        old.items.clear();
        v.record_slot(key.clone(), old);
        assert!(
            !v.slot_readers
                .get("w")
                .is_some_and(|s| s.contains(&(key.clone(), 0)))
        );
    }

    #[test]
    fn resolves_a_waiting_node_into_its_retained_position() {
        let lib = library();
        let opt = Options::default();
        let (base, cache, key, root) = waiting(&lib, &opt);
        let changes = [change("w=Leaf(\"x\")")];
        let dirty = ["w".to_string()].into_iter().collect();
        let attempt = run(&lib, &opt, &cache, &base, &changes, &dirty, None);
        assert!(
            attempt.result.is_some(),
            "waiting reference resolves locally"
        );
        assert!(attempt.work.statements_checked > 0);
        let (validation, next, grafts) = attempt.result.unwrap();
        assert_eq!(grafts.len(), 1);
        assert_eq!(grafts[0].parent, "root");
        assert_eq!(grafts[0].slot, "children");
        assert_eq!(grafts[0].index, 0);
        assert_eq!(grafts[0].children.len(), 1);
        let parent = validation.ir.nodes.get(&root).unwrap();
        let slot = parent.slots.iter().find(|s| s.name == key.name).unwrap();
        assert_eq!(
            slot.children.iter().copied().collect::<Vec<_>>(),
            grafts[0].children.iter().copied().collect::<Vec<_>>()
        );
        let item = next.slots.get(&key).unwrap().items.get(&0).unwrap();
        assert_eq!(item.output.len(), 1);
        assert_eq!(next.slots.get(&key).unwrap().counts.prefix(0), 0);
        assert_eq!(next.slots.get(&key).unwrap().counts.prefix(1), 1);
    }

    #[test]
    fn resolve_waiting_bails_on_general_or_unsafe_changes() {
        let lib = library();
        let opt = Options::default();
        let (base, cache, _, _) = waiting(&lib, &opt);
        let dirty = ["w".to_string()].into_iter().collect();
        assert!(
            run(&lib, &opt, &cache, &base, &[], &dirty, None)
                .result
                .is_none(),
            "empty changes are not a local slot resolution"
        );
        assert!(
            run(&lib, &opt, &cache, &base, &[change("$s=1")], &dirty, None)
                .result
                .is_none(),
            "non-bind statements are not local slot resolutions"
        );
        let mut non_node = change("w=1");
        non_node.name = "w".into();
        assert!(
            run(&lib, &opt, &cache, &base, &[non_node], &dirty, None)
                .result
                .is_none()
        );
        let mut depending = change("w=Leaf(t)");
        depending.name = "w".into();
        assert!(
            run(&lib, &opt, &cache, &base, &[depending], &dirty, None)
                .result
                .is_none(),
            "a value dependency needs the general path"
        );
        let mut state_dep = change("w=Leaf($s)");
        state_dep.name = "w".into();
        let mut sealed = base.ir.states.clone();
        sealed.insert(
            "s".into(),
            StateDef {
                default: Value::from("x"),
                ty: T::text(),
            },
        );
        assert!(
            run(
                &lib,
                &opt,
                &cache,
                &base,
                &[state_dep],
                &dirty,
                Some(&sealed)
            )
            .result
            .is_some(),
            "state dependencies do not need the general path"
        );
    }

    #[test]
    fn resolve_waiting_honours_readers_allocation_and_ambiguous_state_flags() {
        let lib = library();
        let opt = Options::default();
        let changes = [change("w=Leaf(\"x\")")];
        let dirty = ["w".to_string()].into_iter().collect();

        let (base, mut cache, _, _) = waiting(&lib, &opt);
        cache.readers.insert("w".into(), Default::default());
        assert!(
            run(&lib, &opt, &cache, &base, &changes, &dirty, None)
                .result
                .is_some(),
            "an empty reader set does not block a waiting slot"
        );

        let (base, mut cache, _, _) = waiting(&lib, &opt);
        cache.allocation_limited = true;
        assert!(
            run(&lib, &opt, &cache, &base, &changes, &dirty, None)
                .result
                .is_none(),
            "allocation-limited caches use the general path"
        );

        let (base, mut cache, _, _) = waiting(&lib, &opt);
        cache.ambiguous_states = true;
        let sealed = base.ir.states.clone();
        assert!(
            run(&lib, &opt, &cache, &base, &changes, &dirty, Some(&sealed))
                .result
                .is_some(),
            "sealed state types allow local slot resolution"
        );

        let (base, mut cache, _, _) = waiting(&lib, &opt);
        cache
            .readers
            .insert("w".into(), [(crate::ir::NodeId(42))].into_iter().collect());
        assert!(
            run(&lib, &opt, &cache, &base, &changes, &dirty, None)
                .result
                .is_none(),
            "instances that read the waiting name need the general path"
        );
    }

    #[test]
    fn resolve_waiting_rejects_planned_or_occupied_positions() {
        let lib = library();
        let opt = Options::default();
        let dirty = ["w".to_string()].into_iter().collect();
        let changes = [change("w=Leaf(\"x\")")];
        for mutate in ["planned", "output", "missing-parent"] {
            let (base, mut cache, key, _) = waiting(&lib, &opt);
            let mut check = cache.slots.get(&key).unwrap().clone();
            let mut item = check.items.get(&0).unwrap().clone();
            match mutate {
                "planned" => item.planned = true,
                "output" => item.output = [crate::ir::NodeId(9999)].into_iter().collect(),
                _ => {
                    let mut damaged = cache.clone();
                    damaged.slots.clear();
                    assert!(
                        run(&lib, &opt, &damaged, &base, &changes, &dirty, None)
                            .result
                            .is_none()
                    );
                    continue;
                }
            }
            check.items.insert(0, item);
            cache.slots.insert(key.clone(), check);
            assert!(
                run(&lib, &opt, &cache, &base, &changes, &dirty, None)
                    .result
                    .is_none(),
                "{mutate} position is not resolvable"
            );
        }
    }
}
