//! Canonical source tasks. Batch and live share static checking; retained source
//! ownership separates a definition's diagnostics from its instances and users.
use super::*;

impl Validator<'_> {
    pub(super) fn check_sources(&mut self, sources: &BTreeSet<String>) {
        let mut edges = BTreeMap::<String, Vec<String>>::new();
        let mut reverse = BTreeMap::<String, Vec<String>>::new();
        for name in sources {
            self.work.statements_checked += 1;
            let Some(source) = self.table.get_shared(name) else {
                continue;
            };
            if self.kinds.get(name) == Some(&Kind::Node) {
                continue;
            }
            let mut dependencies = BTreeSet::new();
            names(&source.value, &mut dependencies);
            let dependencies: Vec<_> = dependencies
                .into_iter()
                .filter(|dependency| {
                    sources.contains(dependency)
                        && self.table.contains_key(dependency)
                        && self.kinds.get(dependency) != Some(&Kind::Node)
                })
                .collect();
            for dependency in &dependencies {
                reverse
                    .entry(dependency.clone())
                    .or_default()
                    .push(name.clone());
            }
            edges.insert(name.clone(), dependencies);
        }
        // Explicit postorder and reverse walks avoid Rust recursion on long
        // alias chains and give every static SCC one stable diagnostic owner.
        let mut visited = BTreeSet::new();
        let mut order = Vec::new();
        for root in edges.keys() {
            let mut stack = vec![(root.clone(), false)];
            while let Some((name, finish)) = stack.pop() {
                if finish {
                    order.push(name);
                    continue;
                }
                if !visited.insert(name.clone()) {
                    continue;
                }
                self.work.statements_checked += 1;
                stack.push((name.clone(), true));
                stack.extend(
                    edges
                        .get(&name)
                        .into_iter()
                        .flatten()
                        .rev()
                        .cloned()
                        .map(|dependency| (dependency, false)),
                );
            }
        }
        let mut assigned = BTreeSet::new();
        let mut cyclic = BTreeMap::<String, String>::new();
        for root in order.iter().rev() {
            if assigned.contains(root) {
                continue;
            }
            let mut component = BTreeSet::new();
            let mut stack = vec![root.clone()];
            while let Some(name) = stack.pop() {
                if !assigned.insert(name.clone()) {
                    continue;
                }
                self.work.statements_checked += 1;
                component.insert(name.clone());
                stack.extend(reverse.get(&name).into_iter().flatten().cloned());
            }
            let is_cycle = component.len() > 1
                || component
                    .first()
                    .is_some_and(|name| edges.get(name).is_some_and(|edges| edges.contains(name)));
            if is_cycle && let Some(owner) = component.first().cloned() {
                for name in component {
                    self.cache.insert(name.clone(), Checked::invalid());
                    cyclic.insert(name, owner.clone());
                }
            }
        }
        for name in order {
            self.work.statements_checked += 1;
            let Some(source) = self.table.get_shared(&name) else {
                continue;
            };
            let context = Context::at(&name, None);
            let kind = self.kinds.get(&name).copied().unwrap_or(Kind::Value);
            let start = self.diagnostics.len();
            let checked = if let Some(owner) = cyclic.get(&name) {
                if owner == &name {
                    self.emit(Code::Cycle, &context);
                }
                if kind == Kind::Action {
                    self.action_sources.insert(name.clone(), Expr::null());
                }
                Checked::invalid()
            } else if source.kind == StatementKind::Query {
                self.query_checked(&name, &context)
            } else if kind == Kind::Action {
                let (valid, normalized) = self.normalize_action(&source.value, &context);
                self.action_sources.insert(name.clone(), normalized);
                if valid {
                    Checked::dynamic(T::Action)
                } else {
                    Checked::invalid()
                }
            } else {
                self.check_value(&source.value, &context)
            };
            self.value_diagnostics
                .insert(name.clone(), self.diagnostics[start..].to_vec());
            self.ir.values.insert(
                name.clone(),
                ValueEntry {
                    kind,
                    source_kind: source.kind,
                    expression: source.value.clone(),
                    value: checked.value.clone().filter(|_| checked.valid),
                },
            );
            self.cache.insert(name, checked);
        }
    }
}

impl Validator<'_> {
    fn stage(&mut self, key: String, diagnostics: Vec<Diagnostic>) {
        if let Some(machine) = &mut self.tasks {
            self.work.diagnostics_examined += machine.stage(key, diagnostics) as u64;
        }
    }
    fn changed_names(
        names: impl IntoIterator<Item = String>,
        reverse: &BTreeMap<String, BTreeSet<String>>,
        dirty: &mut BTreeSet<String>,
    ) {
        let mut queue: Vec<_> = names.into_iter().collect();
        while let Some(name) = queue.pop() {
            if dirty.insert(name.clone()) {
                queue.extend(reverse.get(&name).into_iter().flatten().cloned());
            }
        }
    }
    pub(super) fn run_live(
        mut self,
        changes: tasks::Changes<'_>,
    ) -> (Validation, crate::MaintenanceWork, ValidationCache) {
        let mut dirty = changes.dirty.clone();
        let mut raw = self
            .tasks
            .as_ref()
            .map_or_else(Default::default, |machine| machine.sources.clone());
        let mut states_changed = false;
        let mut queries_changed = false;
        let mut ordinary = Vec::new();
        self.reachable.insert("root".into());
        for source in changes.sources {
            self.work.statements_checked += 1;
            if source.kind == StatementKind::State {
                if self.state_sources.contains_key(&source.name) {
                    continue;
                }
                self.state_sources
                    .insert(source.name.clone(), source.clone());
                let context = Context::at(&source.name, None);
                let start = self.diagnostics.len();
                if let Some(mut value) = Value::constant_recovered(&source.value) {
                    self.cap_value(&mut value, &context);
                    let weight = serde_json::to_vec(&value).map_or(usize::MAX, |bytes| bytes.len());
                    if (value == Value::Null
                        || matches!(&value,Value::List(items)if items.is_empty()))
                        && let Some(machine) = &mut self.tasks
                    {
                        machine.ambiguous.insert(source.name.clone());
                    }
                    self.state_candidates.insert(
                        source.name.clone(),
                        StateDef {
                            ty: T::infer(&value),
                            default: value,
                        },
                    );
                    self.state_budget.insert(source.name.clone(), weight);
                    states_changed = true;
                } else {
                    self.emit(Code::ConstDefault, &context);
                }
                self.stage(
                    format!("state:{}", source.name),
                    self.diagnostics[start..].to_vec(),
                );
            } else {
                if raw.contains_key(&source.name) {
                    continue;
                }
                raw.insert(source.name.clone(), source.clone());
                self.table.insert(source.name.clone(), source.clone());
                self.kinds.insert(source.name.clone(), Kind::Value);
                if source.kind == StatementKind::Query {
                    self.query_sources
                        .insert(source.name.clone(), source.clone());
                    self.query_budget.insert(source.name.clone(), 1);
                    queries_changed = true;
                }
                ordinary.push(source.clone());
            }
        }
        if let Some(machine) = &mut self.tasks {
            machine.sources = raw.clone();
        }
        // Source tables only grow. A draft is a persistent overlay of the sealed
        // root; dropping it selects that root rather than deleting through history.
        let mut reachable_queue = Vec::new();
        for source in &ordinary {
            if self.reachable.contains(&source.name) {
                let mut dependencies = BTreeSet::new();
                names(&source.value, &mut dependencies);
                reachable_queue.extend(dependencies);
            }
        }
        while let Some(name) = reachable_queue.pop() {
            self.work.statements_checked += 1;
            if self.reachable.contains(&name) {
                continue;
            }
            self.reachable.insert(name.clone());
            self.stage(format!("orphan:{name}"), Vec::new());
            self.orphan_diagnostics.remove(&name);
            if let Some(source) = raw.get_shared(&name) {
                let mut dependencies = BTreeSet::new();
                names(&source.value, &mut dependencies);
                reachable_queue.extend(dependencies);
            }
        }
        let mut kinds: BTreeSet<_> = dirty
            .iter()
            .filter(|name| raw.contains_key(*name))
            .cloned()
            .collect();
        kinds.extend(ordinary.iter().map(|source| source.name.clone()));
        while let Some(name) = kinds.pop_first() {
            let Some(source) = raw.get_shared(&name) else {
                continue;
            };
            let kind = if source.kind == StatementKind::Query {
                Kind::Value
            } else {
                self.classify(&source.value)
            };
            if self.kinds.get(&name) != Some(&kind) {
                self.kinds.insert(name.clone(), kind);
                kinds.extend(
                    changes
                        .reverse
                        .get(&name)
                        .into_iter()
                        .flatten()
                        .filter(|name| raw.contains_key(*name))
                        .cloned(),
                );
            }
            if kind == Kind::Node {
                if !self.ir.templates.contains_key(&name) {
                    self.ir.templates.insert(name.clone(), source.value.clone());
                }
                self.ir.values.remove(&name);
                self.cache.remove(&name);
                self.action_sources.remove(&name);
                self.value_diagnostics.remove(&name);
                self.stage(format!("value:{name}"), Vec::new());
            }
        }
        if states_changed {
            let (bytes, _, accepted) = self.state_budget.reconcile(
                self.limits.max_state_bytes,
                &mut self.work.statements_checked,
            );
            self.state_bytes = bytes;
            for (name, accepted) in accepted {
                if accepted {
                    if let Some(candidate) = self.state_candidates.get(&name) {
                        self.ir.states.insert(name.clone(), candidate.clone());
                    }
                    self.stage(format!("state-limit:{name}"), Vec::new());
                } else {
                    self.ir.states.remove(&name);
                    self.stage(
                        format!("state-limit:{name}"),
                        vec![Diagnostic::new(Code::StateSize, Some(name.clone()), None)],
                    );
                }
                Self::changed_names([format!("${name}")], changes.reverse, &mut dirty);
            }
        }
        if let Some(states) = self.sealed_states {
            self.ir.states = states.clone();
        } else if let Some(machine) = &self.tasks
            && !machine.ambiguous.is_empty()
        {
            let (bindings, inferred, checked) = binding::refresh(
                &machine.bindings,
                &self.table,
                &self.kinds,
                self.lib,
                &dirty,
                &machine.changed_sources,
            );
            self.work.statements_checked += checked;
            let before = self.ir.states.clone();
            if let Some(machine) = &mut self.tasks {
                machine.bindings = bindings;
                for name in &machine.ambiguous {
                    // The inferred type is derived, not authoritative: a later
                    // earlier binding must be able to retract an earlier choice.
                    let Some(base) = self.state_candidates.get(name) else {
                        continue;
                    };
                    let chosen = inferred.iter().find(|(state, _)| state == name).and_then(
                        |(_, ty)| match (&base.default, ty) {
                            // Match batch inference exactly: a null default only
                            // widens `Any`; an empty list only widens `List<Any>`
                            // to a bound list type.
                            (Value::Null, ty) if matches!(base.ty, T::Any) => Some(ty.clone()),
                            (Value::List(items), ty)
                                if items.is_empty()
                                    && matches!(&base.ty, T::List { of } if matches!(of.as_ref(), T::Any))
                                    && matches!(ty, T::List { .. }) =>
                            {
                                Some(ty.clone())
                            }
                            _ => None,
                        },
                    );
                    let next = chosen.unwrap_or_else(|| base.ty.clone());
                    if let Some(state) = self.ir.states.get_mut(name)
                        && state.ty != next
                    {
                        state.ty = next;
                    }
                }
            }
            let changed = before
                .changed_keys(&self.ir.states)
                .into_iter()
                .filter(|name| before.get(name) != self.ir.states.get(name))
                .map(|name| format!("${name}"));
            Self::changed_names(changed, changes.reverse, &mut dirty);
        }
        if queries_changed {
            let (_, _, accepted) = self
                .query_budget
                .reconcile(self.limits.max_queries, &mut self.work.statements_checked);
            for (name, accepted) in accepted {
                let start = self.diagnostics.len();
                if accepted {
                    if let Some(source) = self.query_sources.get_shared(&name) {
                        self.table.insert(name.clone(), source.as_ref().clone());
                        self.kinds.insert(name.clone(), Kind::Value);
                        self.query_header(&source);
                    }
                } else {
                    self.table.remove(&name);
                    self.kinds.remove(&name);
                    self.cache.remove(&name);
                    self.ir.values.remove(&name);
                    self.ir.queries.remove(&name);
                    self.value_diagnostics.remove(&name);
                    self.orphan_diagnostics.remove(&name);
                    self.stage(format!("value:{name}"), Vec::new());
                    self.stage(format!("orphan:{name}"), Vec::new());
                    self.emit(Code::TooManyQueries, &Context::at(&name, None));
                }
                let diagnostics = self.diagnostics[start..].to_vec();
                self.query_diagnostics
                    .insert(name.clone(), diagnostics.clone());
                self.stage(format!("query:{name}"), diagnostics);
                Self::changed_names([name], changes.reverse, &mut dirty);
            }
        }
        let scalar: BTreeSet<_> = dirty
            .iter()
            .filter(|name| {
                self.table.contains_key(*name) && self.kinds.get(*name) != Some(&Kind::Node)
            })
            .cloned()
            .collect();
        self.check_sources(&scalar);
        for name in &scalar {
            self.stage(
                format!("value:{name}"),
                self.value_diagnostics
                    .get(name)
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        for name in dirty
            .iter()
            .filter(|name| self.table.contains_key(*name))
            .cloned()
            .collect::<Vec<_>>()
        {
            if self.reachable.contains(&name) {
                continue;
            }
            self.work.statements_checked += 1;
            let Some(source) = self.table.get_shared(&name) else {
                continue;
            };
            let context = Context::at(&name, None);
            let start = self.diagnostics.len();
            if self.kinds.get(&name) == Some(&Kind::Node) {
                let ir = std::mem::take(&mut self.ir);
                let plans = std::mem::take(&mut self.pending_plans);
                let allocated = self.allocated;
                self.ir.states = ir.states.clone();
                self.ir.queries = ir.queries.clone();
                self.checking_orphan = true;
                self.checking_template = true;
                self.walk_expr(&source.value, &context, NodePath::root(), 1, None);
                self.checking_orphan = false;
                self.checking_template = false;
                self.ir = ir;
                self.pending_plans = plans;
                self.allocated = allocated;
            }
            self.emit(Code::Orphan, &context);
            let diagnostics = self.diagnostics[start..].to_vec();
            self.orphan_diagnostics
                .insert(name.clone(), diagnostics.clone());
            self.stage(format!("orphan:{name}"), diagnostics);
        }
        if let Some(machine) = &mut self.tasks {
            machine.invalidate_sources(&dirty, changes.reverse);
            machine.metadata(&self.ir, &self.cache, &self.action_sources, changes.reverse);
        }
        if let Some(root) = self.table.get_shared("root") {
            self.stage("root".into(), Vec::new());
            self.active.insert("root".into());
            let roots = if self.cache.get("root").is_some_and(|checked| !checked.valid) {
                if let Some(machine) = &mut self.tasks {
                    machine.discard_root(&mut self.ir);
                }
                Default::default()
            } else {
                self.walk_expr(
                    &root.value,
                    &Context::at("root", Some(NodePath::root())),
                    NodePath::root(),
                    1,
                    None,
                )
            };
            self.active.remove("root");
            self.ir.root = roots.first().copied();
            self.ir.root_plan = self.pending_plans.pop_back();
        } else {
            if let Some(machine) = &mut self.tasks {
                machine.discard_root(&mut self.ir);
            }
            self.ir.root = None;
            self.ir.root_plan = None;
            self.stage(
                "root".into(),
                vec![Diagnostic::new(Code::NoRoot, None, None)],
            );
        }
        let mut diagnostics = crate::Diagnostics::default();
        if let Some(machine) = &mut self.tasks {
            let (nodes, work) = machine.finish();
            self.ir.nodes = nodes;
            self.work.instances_examined += work;
            diagnostics = machine.diagnostics();
            self.work.diagnostics_examined += machine.diagnostic_work;
            machine.retain_ir(&self.ir);
        }
        let ambiguous_states = self
            .tasks
            .as_ref()
            .is_some_and(|machine| !machine.ambiguous.is_empty());
        let state_limited = diagnostics.contains_code(Code::StateSize);
        let allocation_limited =
            diagnostics.contains_code(Code::Nodes) || diagnostics.contains_code(Code::IrDepth);
        let cache = ValidationCache {
            table: self.table,
            state_sources: self.state_sources,
            query_sources: self.query_sources,
            query_diagnostics: self.query_diagnostics,
            orphan_diagnostics: self.orphan_diagnostics,
            query_budget: self.query_budget,
            kinds: self.kinds,
            checked: self.cache,
            action_sources: self.action_sources,
            reachable: self.reachable,
            instances: self.instances,
            readers: self.readers,
            value_diagnostics: self.value_diagnostics,
            diagnostics: diagnostics.clone(),
            ids: self.ids,
            slots: self.slots,
            slot_readers: self.slot_readers,
            ambiguous_states,
            allocation_limited,
            tasks: self.tasks,
            state_bytes: self.state_bytes,
            state_budget: self.state_budget,
            state_candidates: self.state_candidates,
            state_limited,
        };
        (
            Validation {
                ir: self.ir,
                diagnostics,
            },
            self.work,
            cache,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::r#lib::{ComponentDef, GroupDef, Role};

    fn library() -> Library {
        Library::builder("source", "0.1.0")
            .group(GroupDef::new("G", 0, "Source fixtures."))
            .component(
                ComponentDef::new("Leaf", Role::Paragraph)
                    .group("G")
                    .doc("A leaf.")
                    .req("text", T::text())
                    .example("Leaf(\"ok\")"),
            )
            .build()
            .unwrap()
    }
    fn validator(source: &str) -> Validator<'static> {
        let lib = Box::leak(Box::new(library()));
        let options = Box::leak(Box::new(Options::default()));
        let parsed = parse_batch(source.as_bytes(), options.limits.clone().syntax);
        let mut v = Validator::new(&parsed.program, parsed.diagnostics, lib, options);
        v.work.statements_checked = 0;
        v
    }
    fn sources(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn check_sources_counts_every_phase() {
        let mut v = validator("a=b\nb=1\nc=a\n");
        v.check_sources(&sources(&["a", "b", "c"]));
        assert_eq!(v.work.statements_checked, 12);
    }

    #[test]
    fn check_sources_ignores_dependencies_outside_the_dirty_set() {
        let mut v = validator("a=b\nb=a\n");
        v.check_sources(&sources(&["a"]));
        assert_eq!(v.work.statements_checked, 4);
        assert!(!v.ir.values.contains_key("b"));
        assert!(v.diagnostics.is_empty());
    }

    #[test]
    fn static_cycles_have_one_stable_alphabetic_owner() {
        let mut v = validator("a=b\nb=a\n");
        v.check_sources(&sources(&["a", "b"]));
        assert_eq!(v.diagnostics.len(), 1);
        assert_eq!(v.diagnostics[0].code, Code::Cycle);
        assert_eq!(v.diagnostics[0].stmt.as_deref(), Some("a"));
        assert!(v.cache.get("a").is_some_and(|c| !c.valid));
        assert!(v.cache.get("b").is_some_and(|c| !c.valid));
    }

    #[test]
    fn cyclic_action_owners_still_retain_an_action_source() {
        let mut v = validator("a=b\nb=a\n");
        v.kinds.insert("a".into(), Kind::Action);
        v.kinds.insert("b".into(), Kind::Value);
        v.check_sources(&sources(&["a", "b"]));
        assert!(v.action_sources.contains_key("a"));
        assert!(!v.action_sources.contains_key("b"));
    }

    #[test]
    fn cyclic_value_owners_never_retain_an_action_source() {
        let mut v = validator("a=b\nb=a\n");
        v.kinds.insert("a".into(), Kind::Value);
        v.kinds.insert("b".into(), Kind::Action);
        v.check_sources(&sources(&["a", "b"]));
        assert!(!v.action_sources.contains_key("a"));
        assert!(v.action_sources.contains_key("b"));
        assert_eq!(v.action_sources.len(), 1);
    }
}
