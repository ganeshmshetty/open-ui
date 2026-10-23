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
