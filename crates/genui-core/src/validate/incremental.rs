//! Retained local checks. Structural/allocation changes currently retain the
//! batch-equivalent fallback; that remaining work is tracked as an I7 blocker.
use super::*;

pub(crate) struct Invalidation<'a> {
    pub dirty: &'a BTreeSet<String>,
    pub reverse: &'a BTreeMap<String, BTreeSet<String>>,
}
pub(crate) struct Attempt {
    pub result: Option<(Validation, ValidationCache, Vec<Graft>)>,
    pub work: crate::MaintenanceWork,
}
fn failed(v: Validator<'_>, mut work: crate::MaintenanceWork) -> Attempt {
    work += v.work;
    work.diagnostics_examined += v.diagnostics.len() as u64;
    Attempt { result: None, work }
}
impl Validator<'_> {
    pub(super) fn record_instance(&mut self, id: NodeId, mut instance: InstanceCheck) {
        instance.reads.insert(format!("@{}", instance.ctx.stmt));
        if let Some(old) = self.instances.get(&id) {
            for name in &old.reads {
                if let Some(ids) = self.readers.get_mut(name) {
                    ids.remove(&id);
                }
            }
        }
        for name in &instance.reads {
            if let Some(ids) = self.readers.get_mut(name) {
                ids.insert(id);
            } else {
                self.readers
                    .insert(name.clone(), std::iter::once(id).collect());
            }
        }
        self.instances.insert(id, instance);
    }
}
/// Recheck changed scalar definitions and their actual prop consumers, without
/// visiting containing slots or unchanged siblings. An unsuccessful attempt's
/// work is still charged before the general path runs.
pub(crate) fn validate_props(
    cached: &ValidationCache,
    base: &Validation,
    changes: &[Statement],
    invalidation: Invalidation<'_>,
    sealed_states: Option<&crate::SharedMap<String, StateDef>>,
    library: &Library,
    options: &Options,
) -> Attempt {
    if cached.tasks.is_some() {
        return Attempt {
            result: None,
            work: Default::default(),
        };
    }
    let Invalidation { dirty, reverse } = invalidation;
    let mut work = crate::MaintenanceWork::default();
    if (cached.ambiguous_states
        && sealed_states.is_none()
        && changes.iter().any(|s| s.kind == StatementKind::Bind))
        || changes.is_empty()
        || changes.iter().any(|s| {
            !matches!(
                s.kind,
                StatementKind::Bind | StatementKind::State | StatementKind::Query
            )
        })
    {
        return Attempt { result: None, work };
    }
    let mut v = Validator::new(&Program::default(), Vec::new(), library, options);
    v.table = cached.table.clone();
    v.state_sources = cached.state_sources.clone();
    v.query_sources = cached.query_sources.clone();
    v.query_diagnostics = cached.query_diagnostics.clone();
    v.orphan_diagnostics = cached.orphan_diagnostics.clone();
    v.query_budget = cached.query_budget.clone();
    v.kinds = cached.kinds.clone();
    v.cache = cached.checked.clone();
    v.action_sources = cached.action_sources.clone();
    v.reachable = cached.reachable.clone();
    v.slots = cached.slots.clone();
    v.slot_readers = cached.slot_readers.clone();
    v.instances = cached.instances.clone();
    v.readers = cached.readers.clone();
    v.value_diagnostics = cached.value_diagnostics.clone();
    v.sealed_states = sealed_states;
    v.ir = base.ir.clone();
    v.ids = cached.ids.clone();
    v.state_bytes = cached.state_bytes;
    v.state_budget = cached.state_budget.clone();
    v.state_candidates = cached.state_candidates.clone();
    let mut state_limited = cached.state_limited;
    let mut diagnostics = cached.diagnostics.clone();
    let mut targets = BTreeSet::new();
    let mut grafts = Vec::new();
    let mut changed_slots = BTreeSet::new();
    let mut scalar_sources = BTreeSet::new();
    let mut additional_dirty = BTreeSet::new();
    for s in changes {
        work.statements_checked += 1;
        if s.kind == StatementKind::State {
            if cached.ambiguous_states || cached.state_sources.contains_key(&s.name) {
                return failed(v, work);
            }
            let Some(mut value) = Value::constant_recovered(&s.value) else {
                return failed(v, work);
            };
            v.cap_value(&mut value, &Context::at(&s.name, None));
            if value == Value::Null || matches!(&value,Value::List(items) if items.is_empty()) {
                return failed(v, work);
            }
            let size = serde_json::to_vec(&value).map_or(usize::MAX, |bytes| bytes.len());
            v.state_budget.insert(s.name.clone(), size);
            v.state_candidates.insert(
                s.name.clone(),
                StateDef {
                    ty: T::infer(&value),
                    default: value,
                },
            );
            v.state_sources.insert(s.name.clone(), s.clone());
            continue;
        }
        if s.kind == StatementKind::Query {
            if cached.query_sources.contains_key(&s.name) {
                return failed(v, work);
            }
            v.query_sources.insert(s.name.clone(), s.clone());
            v.query_budget.insert(s.name.clone(), 1);
            v.table.insert(s.name.clone(), s.clone());
            v.kinds.insert(s.name.clone(), Kind::Value);
            continue;
        }
        let kind = v.classify(&s.value);
        if kind == Kind::Value && s.name != "root" {
            if let Some(old) = cached.table.get(&s.name) {
                let mut before = BTreeSet::new();
                let mut after = BTreeSet::new();
                crate::session::dependencies(&old.value, &mut before);
                crate::session::dependencies(&s.value, &mut after);
                if before != after {
                    return failed(v, work);
                }
            }
            scalar_sources.insert(s.name.clone());
        } else if kind == Kind::Node {
            if let Some(old) = cached.table.get(&s.name) {
                let mut before = BTreeSet::new();
                let mut after = BTreeSet::new();
                crate::session::dependencies(&old.value, &mut before);
                crate::session::dependencies(&s.value, &mut after);
                if before != after {
                    return failed(v, work);
                }
            }
            let Expr::Call { name, .. } = &s.value else {
                return failed(v, work);
            };
            let Some((_, def)) = library.component(name) else {
                return failed(v, work);
            };
            if def.args.iter().any(|arg| {
                sealed_states.is_none()
                    && cached.ambiguous_states
                    && matches!(arg.ty.non_null(), T::Binding { .. })
            }) {
                return failed(v, work);
            }
            let Some(ids) = cached.readers.get(&format!("@{}", s.name)) else {
                return failed(v, work);
            };
            targets.extend(ids.iter().copied());
            v.ir.templates.insert(s.name.clone(), s.value.clone());
        } else if kind == Kind::Action && s.name != "root" {
            if let Some(old) = cached.table.get(&s.name) {
                let mut before = BTreeSet::new();
                let mut after = BTreeSet::new();
                crate::session::dependencies(&old.value, &mut before);
                crate::session::dependencies(&s.value, &mut after);
                if before != after {
                    return failed(v, work);
                }
            }
            scalar_sources.insert(s.name.clone());
        } else {
            return failed(v, work);
        }
        if cached.kinds.get(&s.name).is_some_and(|old| *old != kind) {
            return failed(v, work);
        }
        v.table.insert(s.name.clone(), s.clone());
        v.kinds.insert(s.name.clone(), kind);
    }
    if changes.iter().any(|s| s.kind == StatementKind::State) {
        let (bytes, rejected, changed) = v
            .state_budget
            .reconcile(v.limits.max_state_bytes, &mut work.statements_checked);
        v.state_bytes = bytes;
        state_limited = rejected > 0;
        for (name, accepted) in changed {
            let old = Diagnostic::new(Code::StateSize, Some(name.clone()), None);
            diagnostics.remove_raw(&old);
            work.diagnostics_examined += 1;
            if accepted {
                if let Some(value) = v.state_candidates.get(&name) {
                    if value.default == Value::Null
                        || matches!(&value.default, Value::List(items) if items.is_empty())
                    {
                        return failed(v, work);
                    }
                    v.ir.states.insert(name.clone(), value.clone());
                }
            } else {
                v.ir.states.remove(&name);
                v.emit(Code::StateSize, &Context::at(&name, None));
            }
            let mut pending = vec![format!("${name}")];
            while let Some(name) = pending.pop() {
                if additional_dirty.insert(name.clone()) {
                    pending.extend(reverse.get(&name).into_iter().flatten().cloned());
                }
            }
        }
    }
    let queries: Vec<_> = changes
        .iter()
        .filter(|s| s.kind == StatementKind::Query)
        .cloned()
        .collect();
    let additions: Vec<_> = changes
        .iter()
        .filter(|s| s.kind != StatementKind::State && !cached.table.contains_key(&s.name))
        .cloned()
        .collect();
    v.extend_reachable(&additions, &mut diagnostics);
    if !queries.is_empty() {
        let (_, _, changed) = v
            .query_budget
            .reconcile(v.limits.max_queries, &mut work.statements_checked);
        for (name, accepted) in changed {
            if let Some(old) = v.query_diagnostics.remove(&name) {
                for diagnostic in old {
                    diagnostics.remove_raw(&diagnostic);
                    work.diagnostics_examined += 1;
                }
            }
            let start = v.diagnostics.len();
            if accepted {
                if let Some(source) = v.query_sources.get(&name).cloned() {
                    v.query_header(&source);
                }
                scalar_sources.insert(name.clone());
            } else {
                v.table.remove(&name);
                v.kinds.remove(&name);
                v.cache.remove(&name);
                v.ir.values.remove(&name);
                v.ir.queries.remove(&name);
                if let Some(old) = v.value_diagnostics.remove(&name) {
                    for diagnostic in old {
                        diagnostics.remove_raw(&diagnostic);
                        work.diagnostics_examined += 1;
                    }
                }
                if let Some(old) = v.orphan_diagnostics.remove(&name) {
                    for diagnostic in old {
                        diagnostics.remove_raw(&diagnostic);
                        work.diagnostics_examined += 1;
                    }
                }
                v.emit(Code::TooManyQueries, &Context::at(&name, None));
            }
            v.query_diagnostics
                .insert(name.clone(), v.diagnostics[start..].to_vec());
            let mut pending = vec![name];
            while let Some(name) = pending.pop() {
                if additional_dirty.insert(name.clone()) {
                    pending.extend(reverse.get(&name).into_iter().flatten().cloned());
                }
            }
        }
    }
    // Dirty-set traversal is bounded by reverse dependencies, not the program table.
    for name in dirty.iter().chain(additional_dirty.iter()) {
        if let Some(source) = v.table.get(name)
            && source.kind == StatementKind::Bind
            && v.kinds
                .get(name)
                .is_some_and(|kind| *kind != v.classify(&source.value))
        {
            return failed(v, work);
        }
        if let Some(ids) = cached.readers.get(name) {
            targets.extend(ids.iter().copied());
        }
        match v.kinds.get(name) {
            Some(Kind::Value) => {
                if name == "root" {
                    return failed(v, work);
                }
                scalar_sources.insert(name.clone());
            }
            Some(Kind::Action) => {
                scalar_sources.insert(name.clone());
            }
            Some(Kind::Node)
                if !v.reachable.contains(name)
                    || (cached
                        .readers
                        .get(&format!("@{name}"))
                        .is_none_or(|ids| ids.is_empty())
                        && !v
                            .table
                            .get(name)
                            .is_some_and(|source| matches!(source.value, Expr::Name { .. }))) =>
            {
                return failed(v, work);
            }
            _ => {}
        }
    }
    let mut completed = BTreeSet::new();
    let mut visiting = BTreeSet::new();
    for source in &scalar_sources {
        let mut stack = vec![(source.clone(), false)];
        while let Some((name, finish)) = stack.pop() {
            if completed.contains(&name) {
                continue;
            }
            let Some(s) = v.table.get(&name).cloned() else {
                continue;
            };
            if !finish {
                if !visiting.insert(name.clone()) {
                    return failed(v, work);
                }
                stack.push((name.clone(), true));
                let mut deps = BTreeSet::new();
                names(&s.value, &mut deps);
                for dep in deps {
                    if scalar_sources.contains(&dep) {
                        stack.push((dep, false));
                    }
                }
                continue;
            }
            visiting.remove(&name);
            completed.insert(name.clone());
            work.statements_checked += 1;
            if let Some(old) = cached.value_diagnostics.get(&name) {
                work.diagnostics_examined += old.len() as u64;
                for d in old {
                    diagnostics.remove_raw(d);
                }
            }
            let before = v.diagnostics.len();
            let kind = v.kinds.get(&name).copied().unwrap_or(Kind::Value);
            let checked = if s.kind == StatementKind::Query {
                v.query_checked(&name, &Context::at(&name, None))
            } else if kind == Kind::Action {
                let (valid, source) = v.normalize_action(&s.value, &Context::at(&name, None));
                v.action_sources.insert(name.clone(), source);
                if valid {
                    Checked::dynamic(T::Action)
                } else {
                    Checked::invalid()
                }
            } else {
                v.check_value(&s.value, &Context::at(&name, None))
            };
            let own = v.diagnostics[before..].to_vec();
            v.value_diagnostics.insert(name.clone(), own);
            v.ir.values.insert(
                name.clone(),
                ValueEntry {
                    kind,
                    source_kind: s.kind,
                    expression: s.value,
                    value: checked.value.clone().filter(|_| checked.valid),
                },
            );
            v.cache.insert(name.clone(), checked);
        }
    }
    for s in changes {
        if s.kind != StatementKind::State
            && !cached.table.contains_key(&s.name)
            && v.table.contains_key(&s.name)
            && !v.reachable.contains(&s.name)
        {
            let start = v.diagnostics.len();
            v.emit(Code::Orphan, &Context::at(&s.name, None));
            v.orphan_diagnostics
                .insert(s.name.clone(), v.diagnostics[start..].to_vec());
        }
    }
    let mut targets: Vec<_> = targets.into_iter().collect();
    targets.sort_by_key(|id| {
        cached
            .instances
            .get(id)
            .map(|check| check.ctx.order.clone())
    });
    for id in targets {
        let Some(instance) = cached.instances.get(&id).cloned() else {
            return failed(v, work);
        };
        let old = base.ir.nodes.get(&id);
        let changed_source = changes.iter().find(|s| {
            s.kind == StatementKind::Bind
                && s.name == instance.ctx.stmt
                && v.kinds.get(&s.name) == Some(&Kind::Node)
        });
        let source = changed_source.map_or(instance.source.clone(), |s| {
            std::sync::Arc::new(s.value.clone())
        });
        let Expr::Call { name, args } = source.as_ref() else {
            return failed(v, work);
        };
        let Some((kind, def)) = library.component(name) else {
            return failed(v, work);
        };
        if old.is_some_and(|old| kind != old.kind)
            || (old.is_none() && def.args.iter().any(|arg| arg.ty.slot().is_some()))
        {
            return failed(v, work);
        }
        work.instances_examined += 1;
        for d in &instance.diagnostics {
            diagnostics.remove_raw(d);
            work.diagnostics_examined += 1;
        }
        v.retained_slots = old.map(|old| old.slots.clone());
        v.retained_instance = changed_source.is_none().then_some(instance.clone());
        let kept = v.component(
            &source,
            (kind, def),
            args,
            &instance.ctx,
            instance.path.clone(),
            instance.depth,
        );
        v.retained_instance = None;
        v.retained_slots = None;
        if kept != Some(id) || old.is_none() {
            if def.args.iter().any(|arg| arg.ty.slot().is_some()) {
                return failed(v, work);
            }
            if instance.path.0 == "root" {
                if kept.is_none() {
                    v.ir.nodes.remove(&id);
                }
                v.ir.root = kept;
            } else {
                let Some((parent_path, _)) = instance.path.0.rsplit_once('/') else {
                    return failed(v, work);
                };
                let Some((slot, index, yielded)) = instance.ctx.order.last() else {
                    return failed(v, work);
                };
                if *yielded != 0 {
                    return failed(v, work);
                }
                let parent_id = v.ids.allocate(&NodePath(parent_path.into()));
                let key = SlotKey {
                    parent: parent_id,
                    name: slot.clone(),
                };
                let Some(check) = v.slots.get(&key).cloned() else {
                    return failed(v, work);
                };
                let Some(mut item) = check.items.get(index).cloned() else {
                    return failed(v, work);
                };
                if cached.allocation_limited
                    || item.path != instance.path
                    || item.planned
                    || item.output.len() > 1
                    || item.output.get(0).is_some_and(|old| *old != id)
                {
                    return failed(v, work);
                }
                let offset = check.counts.prefix(*index);
                if v.ir.nodes.len() > v.limits.max_nodes || instance.depth > v.limits.max_ir_depth {
                    return failed(v, work);
                }
                let output: crate::SharedVector<_> = kept.into_iter().collect();
                let Some(parent) = v.ir.nodes.get_mut(&parent_id) else {
                    return failed(v, work);
                };
                let Some(parent_slot) = parent.slots.iter_mut().find(|s| s.name == *slot) else {
                    return failed(v, work);
                };
                if !item.output.is_empty() {
                    parent_slot.children.remove(offset);
                }
                if kept.is_some() {
                    parent_slot.children.insert(offset, id);
                }
                if kept.is_none() {
                    v.ir.nodes.remove(&id);
                }
                let mut check = check;
                check.counts.set(*index, output.len());
                item.output = output.clone();
                item.diagnostics = v
                    .instances
                    .get(&id)
                    .map_or_else(Vec::new, |check| check.diagnostics.clone());
                check.items.insert(*index, item);
                changed_slots.insert(key.clone());
                v.slots.insert(key, check);
                grafts.push(Graft {
                    parent: parent_path.into(),
                    slot: slot.clone(),
                    index: offset,
                    children: output,
                    remove: old.map(|_| instance.path.0.clone()),
                    order: instance.ctx.order.clone(),
                });
            }
        } else if old.is_none_or(|old| {
            v.ir.nodes
                .get(&id)
                .is_none_or(|node| node.slots != old.slots)
        }) {
            return failed(v, work);
        }
    }
    for key in changed_slots {
        let Some(check) = v.slots.get(&key) else {
            return failed(v, work);
        };
        let count =
            v.ir.nodes
                .get(&key.parent)
                .and_then(|parent| parent.slots.iter().find(|slot| slot.name == key.name))
                .map_or(0, |slot| slot.children.len());
        if check.arg.min_items.is_some_and(|min| count < min) {
            return failed(v, work);
        }
    }
    work += v.work;
    work.diagnostics_examined += v.diagnostics.len() as u64;
    diagnostics.extend_raw(v.diagnostics);
    let cache = ValidationCache {
        table: v.table,
        state_sources: v.state_sources,
        query_sources: v.query_sources,
        query_diagnostics: v.query_diagnostics,
        orphan_diagnostics: v.orphan_diagnostics,
        query_budget: v.query_budget,
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
        state_bytes: v.state_bytes,
        state_budget: v.state_budget,
        state_candidates: v.state_candidates,
        state_limited,
    };
    Attempt {
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
    use crate::{
        ir::NodeId,
        ir::Role,
        r#lib::{ActionDef, ComponentDef, GroupDef},
    };

    fn library() -> Library {
        Library::builder("test", "0.1.0")
            .group(GroupDef::new("G", 0, "`Leaf`, `Box`, `Control`."))
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
            .component(
                ComponentDef::new("Control", Role::Button)
                    .group("G")
                    .doc("A control.")
                    .opt_none("on_click", T::action())
                    .example("Control()"),
            )
            .action(ActionDef {
                name: "notify".into(),
                params: vec![T::text()],
                doc: "Notify.".into(),
            })
            .build()
            .unwrap()
    }

    fn options() -> Options {
        let mut options = Options::default();
        options.tools.insert("tickets.list".into());
        options
    }

    fn program(text: &str) -> Program {
        crate::syntax::parse_batch(text.as_bytes(), crate::limits::SyntaxLimits::default()).program
    }

    fn prepare(text: &str, lib: &Library, opt: &Options) -> (Validation, ValidationCache) {
        let (validation, _, cache) =
            Validator::new(&program(text), Vec::new(), lib, opt).run_measured();
        (validation, cache)
    }

    fn statement(parsed: &Program, name: &str) -> Statement {
        parsed
            .statements
            .iter()
            .find(|s| s.name == name)
            .cloned()
            .unwrap_or_else(|| panic!("missing statement {name}"))
    }

    fn key(name: &str, kind: StatementKind) -> String {
        if kind == StatementKind::State {
            format!("${name}")
        } else {
            name.into()
        }
    }

    fn invalidation(
        parsed: &Program,
        names: &[&str],
    ) -> (BTreeSet<String>, BTreeMap<String, BTreeSet<String>>) {
        let mut reverse: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for s in &parsed.statements {
            let mut deps = BTreeSet::new();
            crate::session::dependencies(&s.value, &mut deps);
            for dep in deps {
                reverse.entry(dep).or_default().insert(key(&s.name, s.kind));
            }
        }
        let mut dirty = BTreeSet::new();
        let mut pending: Vec<String> = names.iter().map(|n| n.to_string()).collect();
        while let Some(name) = pending.pop() {
            if dirty.insert(name.clone()) {
                pending.extend(reverse.get(&name).into_iter().flatten().cloned());
            }
        }
        (dirty, reverse)
    }

    fn apply(
        base: &str,
        next: &str,
        changed: &[&str],
        lib: &Library,
        opt: &Options,
        sealed: Option<&crate::SharedMap<String, StateDef>>,
    ) -> Attempt {
        let (validation, cache) = prepare(base, lib, opt);
        let parsed = program(next);
        let changes: Vec<_> = changed
            .iter()
            .map(|name| statement(&parsed, name))
            .collect();
        let (dirty, reverse) = invalidation(&parsed, changed);
        validate_props(
            &cache,
            &validation,
            &changes,
            Invalidation {
                dirty: &dirty,
                reverse: &reverse,
            },
            sealed,
            lib,
            opt,
        )
    }

    fn diag_keys(validation: &Validation) -> Vec<(Code, String)> {
        let mut keys: Vec<_> = validation
            .diagnostics
            .iter()
            .map(|d| (d.code, d.stmt.clone().unwrap_or_default()))
            .collect();
        keys.sort();
        keys
    }

    fn assert_matches_batch(
        attempt: Attempt,
        next: &str,
        lib: &Library,
        opt: &Options,
    ) -> ValidationCache {
        let (checked, cache, _grafts) = match attempt.result {
            Some(result) => result,
            None => panic!("retained path rejected for\n{next}"),
        };
        let batch = validate(next.as_bytes(), lib, opt);
        assert_eq!(
            checked.ir.canonical_tree(lib),
            batch.ir.canonical_tree(lib),
            "retained IR differs from batch"
        );
        assert_eq!(diag_keys(&checked), diag_keys(&batch));
        assert!(attempt.work.statements_checked >= 1);
        cache
    }

    const BASE: &str = "$status = \"open\"\n\
$count = 1\n\
t = \"hello\"\n\
n = 3\n\
a = [say(\"hi\")]\n\
u = \"orphan\"\n\
query q0 = tool(\"tickets.list\", {status: $status}, default: [])\n\
root = Box([Leaf(t), Control(on_click: a)])\n";

    #[test]
    fn scalar_value_edits_recheck_only_consumers() {
        let lib = library();
        let opt = options();
        let next = BASE.replace("t = \"hello\"", "t = \"world\"");
        let attempt = apply(BASE, &next, &["t"], &lib, &opt, None);
        assert!(attempt.work.instances_examined > 0);
        assert!(
            attempt.work.statements_checked >= 2,
            "the changed source and its consumer are both checked"
        );
        assert_matches_batch(attempt, &next, &lib, &opt);

        let invalid_base = "t = 1\nroot = Leaf(t)\n";
        let next = invalid_base.replace("t = 1", "t = \"fixed\"");
        let attempt = apply(invalid_base, &next, &["t"], &lib, &opt, None);
        assert!(
            attempt.work.diagnostics_examined > 0,
            "old value diagnostics are withdrawn"
        );
        assert_matches_batch(attempt, &next, &lib, &opt);

        let next = BASE.replace("n = 3", "n = 4");
        assert_matches_batch(
            apply(BASE, &next, &["n"], &lib, &opt, None),
            &next,
            &lib,
            &opt,
        );
    }

    #[test]
    fn dependency_changes_fall_back_to_batch() {
        let lib = library();
        let opt = options();
        let next = BASE.replace("t = \"hello\"", "t = n");
        assert!(
            apply(BASE, &next, &["t"], &lib, &opt, None)
                .result
                .is_none()
        );
        let next = BASE.replace(
            "root = Box([Leaf(t), Control(on_click: a)])",
            "root = Box([Leaf(t), Control(on_click: a), Leaf(n)])",
        );
        assert!(
            apply(BASE, &next, &["root"], &lib, &opt, None)
                .result
                .is_none()
        );
    }

    #[test]
    fn node_and_action_edits_use_retained_instances() {
        let lib = library();
        let opt = options();
        let node_base = "t = \"hello\"\nroot = Leaf(upper(t))\n";
        let next = node_base.replace("upper(t)", "lower(t)");
        assert_matches_batch(
            apply(node_base, &next, &["root"], &lib, &opt, None),
            &next,
            &lib,
            &opt,
        );
        let next = BASE.replace("a = [say(\"hi\")]", "a = [say(\"bye\")]");
        assert_matches_batch(
            apply(BASE, &next, &["a"], &lib, &opt, None),
            &next,
            &lib,
            &opt,
        );
        let next = BASE.replace("a = [say(\"hi\")]", "a = [open(\"https://example.com\")]");
        assert_matches_batch(
            apply(BASE, &next, &["a"], &lib, &opt, None),
            &next,
            &lib,
            &opt,
        );
        let next = BASE.replace("t = \"hello\"", "t = say(\"hi\")");
        assert!(
            apply(BASE, &next, &["t"], &lib, &opt, None)
                .result
                .is_none(),
            "kind changes use the general path"
        );
    }

    #[test]
    fn new_state_declarations_reconcile_budget() {
        let lib = library();
        let opt = options();
        let next = format!("{BASE}$new = 5\n");
        let attempt = apply(BASE, &next, &["new"], &lib, &opt, None);
        assert!(!attempt.result.as_ref().unwrap().1.state_limited);
        assert!(attempt.work.diagnostics_examined >= 1);
        assert_matches_batch(attempt, &next, &lib, &opt);

        let next = format!("{BASE}$new = null\n");
        assert!(
            apply(BASE, &next, &["new"], &lib, &opt, None)
                .result
                .is_none()
        );
        let next = format!("{BASE}$new = []\n");
        assert!(
            apply(BASE, &next, &["new"], &lib, &opt, None)
                .result
                .is_none()
        );
        let next = BASE.replace("$count = 1", "$count = 2");
        let attempt = apply(BASE, &next, &["count"], &lib, &opt, None);
        assert!(attempt.result.is_none(), "existing state edits use batch");
        assert!(attempt.work.statements_checked >= 1);
    }

    #[test]
    fn state_budget_rejection_retains_the_reachable_states() {
        let lib = library();
        let mut opt = options();
        opt.limits.max_state_bytes = 8;
        let base = BASE.replace("$count = 1\n", "$count = 1\n");
        let next = format!(
            "{base}$z1 = \"012345678901234567890123456789\"\n$z2 = \"012345678901234567890123456789\"\n"
        );
        let attempt = apply(&base, &next, &["z1", "z2"], &lib, &opt, None);
        assert!(attempt.work.diagnostics_examined >= 2);
        let cache = assert_matches_batch(attempt, &next, &lib, &opt);
        assert!(cache.state_limited);

        let next = format!("{base}$z1 = 1\n");
        let attempt = apply(&base, &next, &["z1"], &lib, &opt, None);
        let cache = assert_matches_batch(attempt, &next, &lib, &opt);
        assert!(!cache.state_limited);
    }

    #[test]
    fn new_queries_reconcile_budget_and_existing_queries_fall_back() {
        let lib = library();
        let mut opt = options();
        opt.limits.max_queries = 2;
        let next =
            format!("{BASE}query q1 = tool(\"tickets.list\", {{status: $status}}, default: [])\n");
        assert_matches_batch(
            apply(BASE, &next, &["q1"], &lib, &opt, None),
            &next,
            &lib,
            &opt,
        );
        let next = BASE.replace("{status: $status}", "{status: $count}");
        assert!(
            apply(BASE, &next, &["q0"], &lib, &opt, None)
                .result
                .is_none()
        );

        opt.limits.max_queries = 1;
        let next =
            format!("{BASE}query q1 = tool(\"tickets.list\", {{status: $status}}, default: [])\n");
        assert_matches_batch(
            apply(BASE, &next, &["q1"], &lib, &opt, None),
            &next,
            &lib,
            &opt,
        );
    }

    #[test]
    fn orphan_additions_and_edits_keep_one_diagnostic() {
        let lib = library();
        let opt = options();
        let next = format!("{BASE}u2 = \"new\"\n");
        assert_matches_batch(
            apply(BASE, &next, &["u2"], &lib, &opt, None),
            &next,
            &lib,
            &opt,
        );
        let next = BASE.replace("u = \"orphan\"", "u = \"changed\"");
        let attempt = apply(BASE, &next, &["u"], &lib, &opt, None);
        assert_matches_batch(attempt, &next, &lib, &opt);

        let next = format!("{BASE}w = Leaf(\"new\")\n");
        assert!(
            apply(BASE, &next, &["w"], &lib, &opt, None)
                .result
                .is_none(),
            "new unreachable nodes need the general path"
        );
    }

    #[test]
    fn bind_only_changes_do_not_run_query_maintenance() {
        let lib = library();
        let mut opt = options();
        opt.limits.max_queries = 0;
        let base = "$count = 1\nroot = Leaf(\"x\")\n";
        let next = base.replace("$count = 1", "$n = 1");
        // A scalar bind edit with no queries must not be mistaken for a query.
        let next = next.replace("$n = 1", "$count = 1\nn = 2");
        let attempt = apply(base, &next, &["n"], &lib, &opt, None);
        assert_matches_batch(attempt, &next, &lib, &opt);
    }

    #[test]
    fn ambiguous_states_use_sealed_types_or_the_general_path() {
        let lib = library();
        let opt = options();
        let base = "$s = null\n$count = 1\nt = \"hello\"\nroot = Leaf(t)\n";
        let (_, cache) = prepare(base, &lib, &opt);
        assert!(cache.ambiguous_states);
        let next = base.replace("t = \"hello\"", "t = \"world\"");
        let parsed = program(&next);
        let changes = vec![statement(&parsed, "t")];
        let (dirty, reverse) = invalidation(&parsed, &["t"]);
        let (validation, cache_ref) = prepare(base, &lib, &opt);
        let sealed = validation.ir.states.clone();
        let attempt = validate_props(
            &cache_ref,
            &validation,
            &changes,
            Invalidation {
                dirty: &dirty,
                reverse: &reverse,
            },
            Some(&sealed),
            &lib,
            &opt,
        );
        assert_matches_batch(attempt, &next, &lib, &opt);

        let next = format!("{base}$new = 5\n");
        let parsed = program(&next);
        let changes = vec![statement(&parsed, "new")];
        let (dirty, reverse) = invalidation(&parsed, &["new"]);
        let (validation, cache_ref) = prepare(base, &lib, &opt);
        let attempt = validate_props(
            &cache_ref,
            &validation,
            &changes,
            Invalidation {
                dirty: &dirty,
                reverse: &reverse,
            },
            None,
            &lib,
            &opt,
        );
        assert!(
            attempt.result.is_none(),
            "ambiguous states without sealed types leave inference to batch"
        );
    }

    #[test]
    fn rejected_queries_withdraw_old_diagnostics_and_charge_work() {
        let lib = library();
        let mut opt = options();
        opt.limits.max_queries = 1;
        let (validation, mut cache) = prepare(BASE, &lib, &opt);
        let old = Diagnostic::new(Code::UnknownTool, Some("q1".into()), None);
        cache
            .query_diagnostics
            .insert("q1".into(), vec![old.clone()]);
        cache
            .value_diagnostics
            .insert("q1".into(), vec![old.clone()]);
        cache
            .orphan_diagnostics
            .insert("q1".into(), vec![old.clone()]);
        let next =
            format!("{BASE}query q1 = tool(\"tickets.list\", {{status: $status}}, default: [])\n");
        let parsed = program(&next);
        let changes = vec![statement(&parsed, "q1")];
        let (dirty, reverse) = invalidation(&parsed, &["q1"]);
        let attempt = validate_props(
            &cache,
            &validation,
            &changes,
            Invalidation {
                dirty: &dirty,
                reverse: &reverse,
            },
            None,
            &lib,
            &opt,
        );
        let (checked, cache, _) = attempt.result.expect("rejected query is still local");
        assert!(
            !checked
                .diagnostics
                .iter()
                .any(|d| d.code == Code::UnknownTool)
        );
        assert_eq!(
            checked
                .diagnostics
                .iter()
                .filter(|d| d.code == Code::TooManyQueries)
                .count(),
            1
        );
        assert!(attempt.work.diagnostics_examined >= 3);
        assert!(!cache.value_diagnostics.contains_key("q1"));
        assert!(!cache.orphan_diagnostics.contains_key("q1"));
    }

    #[test]
    fn record_instance_replaces_old_reader_edges() {
        let lib = library();
        let opt = Options::default();
        let mut v = Validator::new(&Program::default(), Vec::new(), &lib, &opt);
        let id = NodeId(7);
        let instance = |reads: &[&str]| InstanceCheck {
            source: std::sync::Arc::new(Expr::null()),
            ctx: Context::at("root", None),
            path: NodePath::root(),
            depth: 1,
            bound: Default::default(),
            reads: reads.iter().map(|s| s.to_string()).collect(),
            diagnostics: Vec::new(),
        };
        v.record_instance(id, instance(&["t"]));
        assert!(v.readers.get("t").is_some_and(|ids| ids.contains(&id)));
        assert!(v.readers.get("@root").is_some_and(|ids| ids.contains(&id)));
        v.record_instance(id, instance(&["n"]));
        assert!(!v.readers.get("t").is_some_and(|ids| ids.contains(&id)));
        assert!(v.readers.get("n").is_some_and(|ids| ids.contains(&id)));
    }
}
