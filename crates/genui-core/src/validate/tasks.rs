//! Retained semantic tasks and their independently maintained visible projection.
use super::*;
use std::ops::Bound::{Excluded, Included};

#[derive(Clone, Default)]
pub(super) struct Graph {
    pub(super) nodes: crate::SharedMap<NodeId, Node>,
    paths: crate::SharedMap<String, NodeId>,
    pub(super) sizes: crate::SharedMap<NodeId, usize>,
    hidden: crate::SharedMap<String, ()>,
    outputs: crate::SharedMap<String, crate::SharedVector<NodeId>>,
    locations: crate::SharedMap<NodeId, Location>,
    ranges: crate::SharedMap<RangeKey, Sequence>,
}
impl Graph {
    pub(super) fn insert(&mut self, node: Node, size: usize) {
        self.paths.insert(node.path.0.clone(), node.id);
        self.sizes.insert(node.id, size);
        if self.nodes.get(&node.id) != Some(&node) {
            self.nodes.insert(node.id, node);
        }
    }
    pub(super) fn hide(&mut self, path: &str) {
        if !self.hidden.contains_key(path) {
            self.hidden.insert(path.into(), ());
        }
    }
    fn show(&mut self, path: &str) {
        self.hidden.remove(path);
    }
    fn hidden_by<'a>(&self, path: &'a str, exact: bool) -> Option<&'a str> {
        if exact && self.hidden.contains_key(path) {
            return Some(path);
        }
        for (index, byte) in path.bytes().enumerate().rev() {
            if byte == b':' && self.hidden.contains_key(&path[..index + 1]) {
                return Some(&path[..index + 1]);
            }
            if (byte == b'/' || byte == b'#') && self.hidden.contains_key(&path[..index]) {
                return Some(&path[..index]);
            }
        }
        None
    }
    fn visible(&self, id: &NodeId) -> bool {
        self.nodes
            .get(id)
            .is_some_and(|node| self.hidden_by(&node.path.0, true).is_none())
            && self.locations.get(id).is_none_or(|location| {
                location.chain.iter().all(|(key, index)| {
                    self.ranges
                        .get(key)
                        .and_then(|range| range.index.stopping_key())
                        .is_none_or(|stop| index <= stop)
                })
            })
    }
    fn visible_under(&self, prefix: &str, out: &mut BTreeSet<NodeId>) {
        if self.hidden_by(prefix, true).is_some() {
            return;
        }
        let roots = self.outputs.get(prefix).cloned().or_else(|| {
            let (parent, slot) = prefix.strip_suffix(':')?.rsplit_once('/')?;
            let parent = self.paths.get(parent).and_then(|id| self.nodes.get(id))?;
            parent
                .slots
                .iter()
                .find(|candidate| candidate.name == slot)
                .map(|slot| slot.children.clone())
        });
        if let Some(roots) = roots {
            let mut pending: Vec<_> = roots.iter().copied().collect();
            while let Some(id) = pending.pop() {
                if self.visible(&id) {
                    out.insert(id);
                    if let Some(node) = self.nodes.get(&id) {
                        pending.extend(
                            node.slots
                                .iter()
                                .flat_map(|slot| slot.children.iter().copied()),
                        );
                    }
                }
            }
            return;
        }
        if let Some(id) = self.paths.get(prefix) {
            out.insert(*id);
        }
        // A template's list yields append '#i'; descendants append '/slot:...'.
        // Other names and repeated occurrences ('~k') are separate identities.
        for (separator, end) in [('#', '$'), ('/', '0')] {
            let mut start = Included(format!("{prefix}{separator}"));
            let end = Excluded(format!("{prefix}{end}"));
            loop {
                let next = self.paths.range((start.clone(), end.clone())).next();
                let Some((path, id)) = next else {
                    break;
                };
                if let Some(hidden) = self.hidden_by(path, true) {
                    let suffix = &path[hidden.len()..];
                    start = if suffix.starts_with('#') {
                        Included(format!("{hidden}$"))
                    } else if suffix.starts_with('/') {
                        Included(format!("{hidden}0"))
                    } else {
                        Excluded(path.clone())
                    };
                } else {
                    out.insert(*id);
                    start = Excluded(path.clone());
                }
            }
        }
    }
    fn project(
        &self,
        old: &Self,
        base: &crate::SharedMap<NodeId, Node>,
    ) -> (crate::SharedMap<NodeId, Node>, u64) {
        let mut changed: BTreeSet<_> = self
            .nodes
            .changed_keys(&old.nodes)
            .into_iter()
            .filter(|id| self.visible(id) || old.visible(id))
            .collect();
        changed.extend(
            self.locations
                .changed_keys(&old.locations)
                .into_iter()
                .filter(|id| self.visible(id) || old.visible(id)),
        );
        for key in self.ranges.changed_keys(&old.ranges) {
            let before = old.ranges.get(&key);
            let after = self.ranges.get(&key);
            let a = before.and_then(|range| range.index.stopping_key().copied());
            let b = after.and_then(|range| range.index.stopping_key().copied());
            if a == b {
                continue;
            }
            let earliest = match (a, b) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            if old.hidden_by(&key.owner.path, true).is_none()
                && let Some(range) = before
            {
                range
                    .index
                    .visit_materialized(earliest.as_ref(), a.as_ref(), |_, input, _| {
                        old.visible_under(&input.path.0, &mut changed)
                    });
            }
            if self.hidden_by(&key.owner.path, true).is_none()
                && let Some(range) = after
            {
                range
                    .index
                    .visit_materialized(earliest.as_ref(), b.as_ref(), |_, input, _| {
                        self.visible_under(&input.path.0, &mut changed)
                    });
            }
        }
        for path in self.hidden.changed_keys(&old.hidden) {
            old.visible_under(&path, &mut changed);
            self.visible_under(&path, &mut changed);
        }
        let mut projected = base.clone();
        let work = changed.len() as u64;
        for id in changed {
            if self.visible(&id) {
                if let Some(node) = self.nodes.get_shared(&id)
                    && projected.get(&id) != Some(node.as_ref())
                {
                    projected.insert(id, node.as_ref().clone());
                }
            } else {
                projected.remove(&id);
            }
        }
        (projected, work)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(id: u64, path: &str) -> Node {
        Node {
            id: NodeId(id),
            path: NodePath(path.into()),
            kind: ComponentId(0),
            role: Role::Group,
            props: Default::default(),
            slots: Vec::new(),
            status: NodeStatus::Sealed,
            origin: "root".into(),
        }
    }
    #[test]
    fn failed_parent_keeps_semantic_children_without_visiting_them_again() {
        let mut graph = Graph::default();
        for i in 0..1000 {
            graph.insert(node(i, &format!("root/children:#{i}")), 1);
        }
        graph.hide("root");
        let empty = crate::SharedMap::default();
        let before = graph.clone();
        graph.insert(node(999, "root/children:#999"), 1);
        let (visible, work) = graph.project(&before, &empty);
        assert!(visible.is_empty());
        assert!(work <= 1, "hidden subtree visited {work} nodes");
        assert_eq!(graph.nodes.len(), 1000);
        let before = graph.clone();
        graph.show("root");
        let (visible, work) = graph.project(&before, &empty);
        assert_eq!(visible.len(), 1000);
        assert_eq!(work, 1000);
    }
    #[test]
    fn visibility_prefixes_respect_repeats_and_flattened_list_boundaries() {
        let mut graph = Graph::default();
        let paths = [
            "root/children:@a",
            "root/children:@a~2",
            "root/children:@a#0",
            "root/children:@a#0/children:#1",
            "root/children:@a#10",
            "root/children:@a2",
        ];
        for (i, path) in paths.iter().enumerate() {
            graph.insert(node(i as u64, path), 1);
        }
        let all = graph.nodes.clone();
        let before = graph.clone();
        graph.hide("root/children:@a#0");
        let (visible, work) = graph.project(&before, &all);
        assert_eq!(visible.len(), 4);
        assert_eq!(work, 2);
        assert!(visible.contains_key(&NodeId(4)));
        let before = graph.clone();
        graph.hide("root/children:@a");
        let (visible, _) = graph.project(&before, &visible);
        assert_eq!(
            visible.keys().copied().collect::<Vec<_>>(),
            vec![NodeId(1), NodeId(5)]
        );
    }
    #[test]
    fn kind_only_changes_are_projected() {
        let mut before = crate::ir::Ir::default();
        let mut after = crate::ir::Ir::default();
        let id = NodeId(7);
        let mut old = node(7, "root");
        old.id = id;
        old.kind = ComponentId(1);
        let mut new = old.clone();
        new.kind = ComponentId(2);
        assert_eq!(old.props, new.props);
        assert_eq!(old.status, new.status);
        before.nodes.insert(id, old);
        after.nodes.insert(id, new);
        let projection = Projection::between(&before, &after, None, None);
        assert!(projection.ids.contains(&id));
    }

    #[test]
    fn restoring_a_parent_skips_its_still_hidden_descendants() {
        let mut graph = Graph::default();
        graph.insert(node(0, "root"), 1001);
        for i in 1..1001 {
            graph.insert(node(i, &format!("root/children:#0/children:#{i}")), 1);
        }
        graph.hide("root/children:#0");
        graph.hide("root");
        let before = graph.clone();
        graph.show("root");
        let (visible, work) = graph.project(&before, &Default::default());
        assert_eq!(visible.len(), 1);
        assert_eq!(work, 1, "hidden descendants must be skipped by index");
    }
    #[test]
    fn hidden_slot_prefixes_are_matched_at_their_exact_byte_boundary() {
        let mut graph = Graph::default();
        graph.hide("root/children:");
        assert_eq!(
            graph.hidden_by("root/children:@a", true),
            Some("root/children:")
        );
        assert_eq!(
            graph.hidden_by("root/children:@a", false),
            Some("root/children:")
        );
        assert_eq!(graph.hidden_by("root/children", false), None);
        assert_eq!(graph.hidden_by("root/other:@a", false), None);
        graph.hide("root");
        assert_eq!(graph.hidden_by("root/children", false), Some("root"));
    }
    #[test]
    fn visible_under_resolves_a_slot_prefix_to_that_slots_children() {
        let mut graph = Graph::default();
        let mut parent = node(0, "root");
        parent.slots.push(crate::ir::IrSlot {
            name: "children".into(),
            children: vec![NodeId(1), NodeId(2)].into(),
            plans: Default::default(),
        });
        graph.insert(parent, 2);
        graph.insert(node(1, "root/children:@a"), 1);
        graph.insert(node(2, "root/children:@b"), 1);
        let mut visible = BTreeSet::new();
        graph.visible_under("root/children:", &mut visible);
        assert_eq!(visible, BTreeSet::from([NodeId(1), NodeId(2)]));
    }
    #[test]
    fn projection_removes_a_node_that_became_hidden_even_if_it_changed() {
        let mut before = Graph::default();
        before.hide("root/children:#0");
        before.insert(node(5, "root/children:#1"), 1);
        let shown = before.nodes.clone();
        let mut after = before.clone();
        // Same hidden set; only the node's own path moved under the hidden
        // prefix, so the hidden-key diff cannot compensate for the filter.
        let mut changed = node(5, "root/children:#0/x");
        changed.kind = ComponentId(9);
        after.insert(changed, 1);
        let (visible, _) = after.project(&before, &shown);
        assert!(
            visible.is_empty(),
            "a hidden node must be removed from the projection"
        );
    }
    #[test]
    fn projection_removes_a_node_its_location_chain_now_hides() {
        let mut before = Graph::default();
        before.insert(node(5, "root/children:#1"), 1);
        before.locations.insert(
            NodeId(5),
            Location {
                parent: None,
                slot: "root".into(),
                chain: Default::default(),
            },
        );
        let shown = before.nodes.clone();
        let mut after = before.clone();
        let owner = TaskId {
            path: "root".into(),
            statement: "root".into(),
        };
        let key = RangeKey {
            owner,
            name: "children".into(),
        };
        let mut sequence = Sequence::default();
        for index in 0..=5usize {
            sequence.index.insert(
                index,
                Input {
                    expression: std::sync::Arc::new(Expr::String { value: "e".into() }),
                    context: Context::at("root", None),
                    path: NodePath(format!("root/children:#{index}")),
                    depth: 1,
                    place: None,
                    reject: false,
                },
            );
        }
        let mut range_work = 0;
        sequence
            .index
            .reconcile_with(2, &mut range_work, |index, _, _| {
                let profile = if *index >= 2 {
                    resource::Profile {
                        stopped: true,
                        ..Default::default()
                    }
                } else {
                    resource::Profile::default()
                };
                (profile, Output::default())
            });
        assert_eq!(sequence.index.stopping_key(), Some(&2));
        after.ranges.insert(key.clone(), sequence);
        after.locations.insert(
            NodeId(5),
            Location {
                parent: Some(NodeId(9)),
                slot: "children".into(),
                chain: std::iter::once((key, 5)).collect(),
            },
        );
        let (visible, _) = after.project(&before, &shown);
        assert!(
            visible.is_empty(),
            "a node behind a stopped range must be removed even when only its location changed"
        );
    }
    #[test]
    fn projection_ignores_node_changes_outside_kind_props_and_status() {
        // §6.4: equal props produce no op. An origin-only change is not a
        // renderer-visible change and must not enter the patch projection.
        let mut before = crate::ir::Ir::default();
        let mut after = crate::ir::Ir::default();
        let id = NodeId(3);
        let mut old = node(3, "root");
        old.id = id;
        let mut new = old.clone();
        new.origin = "elsewhere".into();
        assert_ne!(old, new);
        before.nodes.insert(id, old);
        after.nodes.insert(id, new);
        let projection = Projection::between(&before, &after, None, None);
        assert!(projection.ids.is_empty(), "ids: {:?}", projection.ids);
    }
    #[test]
    fn projection_reinserts_retained_children_of_a_replaced_kind() {
        let mut before = crate::ir::Ir::default();
        let mut after = crate::ir::Ir::default();
        let parent = NodeId(1);
        let child = NodeId(2);
        let mut old_parent = node(1, "root");
        old_parent.id = parent;
        let mut new_parent = old_parent.clone();
        new_parent.kind = ComponentId(4);
        let child_node = {
            let mut n = node(2, "root/children:#0");
            n.id = child;
            n
        };
        new_parent.slots.push(crate::ir::IrSlot {
            name: "children".into(),
            children: vec![child].into(),
            plans: Default::default(),
        });
        before.nodes.insert(parent, old_parent);
        after.nodes.insert(parent, new_parent);
        before.nodes.insert(child, child_node.clone());
        after.nodes.insert(child, child_node);
        let projection = Projection::between(&before, &after, None, None);
        assert!(projection.ids.contains(&parent));
        assert!(
            projection.ids.contains(&child),
            "retained children must be inserted after a kind replacement"
        );
    }
    #[test]
    fn cloning_a_machine_preserves_every_retained_layer() {
        // §3.6: retained tasks, arenas and dirty bookkeeping survive a clone.
        let mut machine = Machine::default();
        machine.graph.insert(node(1, "root"), 1);
        let task = TaskId {
            path: "root".into(),
            statement: "root".into(),
        };
        machine
            .parents
            .insert(task.clone(), std::iter::once(task.clone()).collect());
        machine.dirty.insert(task.clone());
        machine.checked.insert(
            "c".into(),
            Checked {
                value: Some(Value::Null),
                ty: crate::r#lib::T::Any,
                valid: true,
            },
        );
        machine
            .actions
            .insert("a".into(), Expr::String { value: "a".into() });
        machine.sources.insert(
            "s".into(),
            Statement {
                kind: StatementKind::Bind,
                name: "s".into(),
                value: Expr::String { value: "s".into() },
            },
        );
        machine.bindings.insert(
            "b".into(),
            crate::validate::binding::Entry {
                summary: Vec::new(),
                deps: vec!["d".into()],
            },
        );
        machine
            .stages
            .insert("k".into(), vec![Diagnostic::new(Code::NoRoot, None, None)]);
        machine.ambiguous.insert("amb".into());
        machine.values.insert(
            "v".into(),
            ValueEntry {
                kind: Kind::Value,
                source_kind: StatementKind::Bind,
                expression: Expr::String { value: "v".into() },
                value: Some(Value::Int(1)),
            },
        );
        machine
            .templates
            .insert("t".into(), Expr::String { value: "t".into() });
        machine.queries.insert(
            "q".into(),
            QueryDef {
                tool: "tool".into(),
                args: Expr::String { value: "q".into() },
                default: Expr::String { value: "d".into() },
                allowed: true,
            },
        );
        machine.range_dirty.insert(
            RangeKey {
                owner: task.clone(),
                name: "r".into(),
            },
            std::iter::once(7usize).collect(),
        );
        let copy = machine.clone();
        assert_eq!(copy.graph.nodes.len(), 1);
        assert_eq!(copy.previous_graph.nodes.len(), 1);
        assert!(copy.parents.contains_key(&task));
        assert!(copy.dirty.contains(&task));
        assert!(copy.checked.contains_key("c"));
        assert!(copy.actions.contains_key("a"));
        assert!(copy.sources.contains_key("s"));
        assert!(copy.bindings.contains_key("b"));
        assert!(copy.stages.contains_key("k"));
        assert!(copy.ambiguous.contains("amb"));
        assert!(copy.values.contains_key("v"));
        assert!(copy.templates.contains_key("t"));
        assert!(copy.queries.contains_key("q"));
        assert_eq!(copy.range_dirty.len(), 1);
    }
    #[test]
    fn source_and_arena_views_export_every_retained_table() {
        let mut machine = Machine::default();
        machine.values.insert(
            "v".into(),
            ValueEntry {
                kind: Kind::Value,
                source_kind: StatementKind::Bind,
                expression: Expr::String { value: "v".into() },
                value: Some(Value::Int(1)),
            },
        );
        machine
            .templates
            .insert("t".into(), Expr::String { value: "t".into() });
        machine.queries.insert(
            "q".into(),
            QueryDef {
                tool: "tool".into(),
                args: Expr::String { value: "q".into() },
                default: Expr::String { value: "d".into() },
                allowed: true,
            },
        );
        machine.states.insert(
            "s".into(),
            StateDef {
                default: Value::Null,
                ty: crate::r#lib::T::Any,
            },
        );
        let scope = machine.expressions.enter("owner".into(), vec![0]);
        let expression = machine
            .expressions
            .intern(&Expr::String { value: "e".into() });
        machine.expressions.exit(scope);
        machine.action_plans.insert(
            ActionPlanId("act".into()),
            ActionPlan {
                steps: Vec::new(),
                origin: NodeId(1),
                submit_scope: None,
            },
        );
        machine.fallbacks.insert(
            expression.clone(),
            FallbackPlan {
                original: node(1, "root"),
                descendants: Vec::new(),
                template: "t".into(),
            },
        );

        let mut ir = Ir::default();
        machine.source_ir(&mut ir);
        assert_eq!(ir.values.len(), 1);
        assert_eq!(ir.templates.len(), 1);
        assert_eq!(ir.queries.len(), 1);
        assert_eq!(ir.states.len(), 1);

        machine.arenas(&mut ir);
        assert_eq!(ir.expressions, machine.expressions);
        assert_eq!(ir.actions.len(), 1);
        assert_eq!(ir.fallbacks.len(), 1);
        assert!(ir.expressions.get(&expression).is_some());
    }
    #[test]
    fn staging_a_replacement_charges_old_and_new_diagnostics() {
        let mut machine = Machine::default();
        let diagnostic = Diagnostic::new(Code::NoRoot, None, None);
        assert_eq!(machine.stage("k".into(), vec![diagnostic.clone()]), 1);
        assert_eq!(machine.stage("k".into(), vec![diagnostic]), 2);
        assert_eq!(machine.diagnostics.len(), 1);
        assert!(machine.metadata_diagnostics.len() == 1);
    }
    #[test]
    fn begin_retains_the_previous_graph_and_marks_source_readers_dirty() {
        let mut machine = Machine::default();
        machine.graph.insert(node(1, "root"), 1);
        machine.reused_nodes = 5;
        machine.diagnostic_work = 9;
        machine.owned_indices.insert(3);
        machine.frames.push(Frame {
            id: TaskId {
                path: "root".into(),
                statement: "root".into(),
            },
            start: 0,
            profile: Default::default(),
            indices: Default::default(),
            limits: Default::default(),
            owners: Default::default(),
            children: Default::default(),
            child_effects: 0,
            old_source: false,
            old_ranges: Default::default(),
            ranges: Default::default(),
            reuse_ranges: false,
        });
        let reader = TaskId {
            path: "root".into(),
            statement: "a".into(),
        };
        machine
            .readers
            .insert("@a".into(), std::iter::once(reader.clone()).collect());
        let other = TaskId {
            path: "root/children:#0".into(),
            statement: "b".into(),
        };
        machine
            .readers
            .insert("b".into(), std::iter::once(other.clone()).collect());
        let sources = [
            Statement {
                kind: StatementKind::Bind,
                name: "a".into(),
                value: Expr::String { value: "a".into() },
            },
            Statement {
                kind: StatementKind::State,
                name: "s".into(),
                value: Expr::String { value: "s".into() },
            },
        ];
        let dirty = BTreeSet::from(["b".to_string()]);
        let reverse = BTreeMap::new();
        let removed = ["gone".to_string()];
        machine.begin(
            &Changes {
                dirty: &dirty,
                sources: &sources,
                reverse: &reverse,
            },
            removed,
        );
        assert_eq!(machine.previous_graph.nodes.len(), 1);
        assert!(machine.frames.is_empty());
        assert_eq!(machine.reused_nodes, 0);
        assert_eq!(machine.diagnostic_work, 0);
        assert!(machine.owned_indices.is_empty());
        assert_eq!(
            machine.changed_sources,
            BTreeSet::from(["a".to_string(), "gone".to_string()]),
            "state statements are not source dependencies"
        );
        assert!(machine.dirty.contains(&reader));
        assert!(machine.dirty.contains(&other));
    }
    #[test]
    fn metadata_invalidates_readers_of_each_changed_reference_table() {
        let mut machine = Machine::default();
        let state_reader = TaskId {
            path: "root".into(),
            statement: "$s".into(),
        };
        let checked_reader = TaskId {
            path: "root/children:#0".into(),
            statement: "c".into(),
        };
        let action_reader = TaskId {
            path: "root/children:#1".into(),
            statement: "act".into(),
        };
        machine
            .readers
            .insert("$s".into(), std::iter::once(state_reader.clone()).collect());
        machine.readers.insert(
            "chk".into(),
            std::iter::once(checked_reader.clone()).collect(),
        );
        machine.readers.insert(
            "act".into(),
            std::iter::once(action_reader.clone()).collect(),
        );
        machine.states.insert(
            "s".into(),
            StateDef {
                default: Value::Null,
                ty: crate::r#lib::T::Any,
            },
        );
        machine.checked.insert(
            "chk".into(),
            Checked {
                value: Some(Value::Null),
                ty: crate::r#lib::T::Any,
                valid: true,
            },
        );
        machine
            .actions
            .insert("act".into(), Expr::String { value: "a".into() });
        let mut ir = Ir::default();
        ir.states.insert(
            "s".into(),
            StateDef {
                default: Value::Int(1),
                ty: crate::r#lib::T::Int,
            },
        );
        let mut checked = crate::SharedMap::default();
        checked.insert(
            "chk".into(),
            Checked {
                value: Some(Value::Int(1)),
                ty: crate::r#lib::T::Int,
                valid: true,
            },
        );
        let mut actions = crate::SharedMap::default();
        actions.insert("act".into(), Expr::String { value: "b".into() });
        machine.metadata(&ir, &checked, &actions, &BTreeMap::new());
        assert!(machine.dirty.contains(&state_reader));
        assert!(machine.dirty.contains(&checked_reader));
        assert!(machine.dirty.contains(&action_reader));
    }
    #[test]
    fn invalidate_sources_follows_reverse_dependencies() {
        let mut machine = Machine::default();
        let a = TaskId {
            path: "a".into(),
            statement: "a".into(),
        };
        let b = TaskId {
            path: "b".into(),
            statement: "b".into(),
        };
        machine
            .readers
            .insert("a".into(), std::iter::once(a.clone()).collect());
        machine
            .readers
            .insert("b".into(), std::iter::once(b.clone()).collect());
        let mut reverse = BTreeMap::new();
        reverse.insert("a".to_string(), BTreeSet::from(["b".to_string()]));
        machine.invalidate_sources(&BTreeSet::from(["a".to_string()]), &reverse);
        assert!(machine.dirty.contains(&a));
        assert!(
            machine.dirty.contains(&b),
            "reverse dependents must be invalidated"
        );
    }
    fn frame(start: usize) -> Frame {
        Frame {
            id: TaskId {
                path: "root".into(),
                statement: "root".into(),
            },
            start,
            profile: resource::Profile {
                lower: 1,
                upper: Some(10),
                ..Default::default()
            },
            indices: Default::default(),
            limits: Default::default(),
            owners: Default::default(),
            children: Default::default(),
            child_effects: 0,
            old_source: false,
            old_ranges: Default::default(),
            ranges: Default::default(),
            reuse_ranges: false,
        }
    }
    #[test]
    fn constrain_projects_a_child_profile_through_the_allocated_offset() {
        let mut machine = Machine::default();
        machine.frames.push(frame(5));
        machine.constrain(
            8,
            resource::Profile {
                lower: 2,
                upper: Some(3),
                ..Default::default()
            },
        );
        let profile = machine.frames[0].profile;
        assert_eq!(profile.lower, 5);
        assert_eq!(profile.upper, Some(6));
    }
    #[test]
    fn recording_a_diagnostic_charges_one_unit_of_work() {
        let mut machine = Machine::default();
        let id = TaskId {
            path: "root".into(),
            statement: "root".into(),
        };
        machine.diagnostic(
            &id,
            0,
            &OwnedDiagnostic {
                value: Diagnostic::new(Code::NoRoot, None, None),
                order: None,
            },
            true,
        );
        assert_eq!(machine.diagnostic_work, 1);
        assert_eq!(machine.diagnostics.len(), 1);
    }
    fn retained_task(_path: &str, statement: &str, effects: usize) -> Task {
        Task {
            source: std::sync::Arc::new(Expr::String {
                value: statement.into(),
            }),
            profile: resource::Profile {
                effects,
                ..Default::default()
            },
            output: Default::default(),
            plans: Default::default(),
            diagnostics: Vec::new(),
            owners: Vec::new(),
            children: Default::default(),
            effects,
            ranges: Default::default(),
            reject: false,
            reads: Default::default(),
            context: Context::at(statement, None),
            depth: 1,
            place: None,
            ancestors: Default::default(),
        }
    }
    #[test]
    fn deactivation_skips_children_without_effects() {
        let parent = TaskId {
            path: "root".into(),
            statement: "root".into(),
        };
        let live = TaskId {
            path: "root/children:#0".into(),
            statement: "live".into(),
        };
        let inert = TaskId {
            path: "root/children:#1".into(),
            statement: "inert".into(),
        };
        let mut machine = Machine::default();
        let mut parent_task = retained_task("root", "root", 2);
        parent_task.children = [live.clone(), inert.clone()].into_iter().collect();
        machine.records.insert(parent.clone(), parent_task);
        machine
            .records
            .insert(live.clone(), retained_task("root/children:#0", "live", 1));
        machine
            .records
            .insert(inert.clone(), retained_task("root/children:#1", "inert", 0));
        machine.active = [parent.clone(), live.clone(), inert.clone()]
            .into_iter()
            .collect();
        machine.deactivate(&parent, &mut Ir::default());
        assert!(!machine.active.contains(&parent));
        assert!(!machine.active.contains(&live));
        assert!(
            machine.active.contains(&inert),
            "zero-effect children are left untouched"
        );
    }
    #[test]
    fn activation_only_restores_children_with_effects() {
        let parent = TaskId {
            path: "root".into(),
            statement: "root".into(),
        };
        let live = TaskId {
            path: "root/children:#0".into(),
            statement: "live".into(),
        };
        let inert = TaskId {
            path: "root/children:#1".into(),
            statement: "inert".into(),
        };
        let mut machine = Machine::default();
        let mut parent_task = retained_task("root", "root", 2);
        parent_task.children = [live.clone(), inert.clone()].into_iter().collect();
        machine.records.insert(parent.clone(), parent_task);
        machine
            .records
            .insert(live.clone(), retained_task("root/children:#0", "live", 1));
        machine
            .records
            .insert(inert.clone(), retained_task("root/children:#1", "inert", 0));
        machine.activate(&parent, &mut Ir::default());
        assert!(machine.active.contains(&parent));
        assert!(machine.active.contains(&live));
        assert!(
            !machine.active.contains(&inert),
            "zero-effect children are not activated"
        );
    }
    fn input(path: &str, depth: usize, place: Option<&str>, reject: bool) -> Input {
        Input {
            expression: std::sync::Arc::new(Expr::String { value: "e".into() }),
            context: Context::at("s", None),
            path: NodePath(path.into()),
            depth,
            place: place.map(str::to_string),
            reject,
        }
    }
    #[test]
    fn input_identity_compares_every_retained_field() {
        let base = input("root", 1, Some("p"), false);
        assert!(base == base.clone());
        let mut different_arc = input("root", 1, Some("p"), false);
        different_arc.expression = std::sync::Arc::new(Expr::String { value: "e".into() });
        assert!(base != different_arc);
        let mut different_context = base.clone();
        different_context.context = Context::at("other", None);
        assert!(base != different_context);
        let mut different_path = base.clone();
        different_path.path = NodePath("root/children:#0".into());
        assert!(base != different_path);
        let mut different_depth = base.clone();
        different_depth.depth = 2;
        assert!(base != different_depth);
        let mut different_place = base.clone();
        different_place.place = Some("q".into());
        assert!(base != different_place);
        let mut no_place = base.clone();
        no_place.place = None;
        assert!(base != no_place);
        let mut different_reject = base.clone();
        different_reject.reject = true;
        assert!(base != different_reject);
    }
    #[test]
    fn aggregate_sameness_requires_shared_or_both_empty_vectors() {
        let empty_a = Output::default();
        let empty_b = Output::default();
        assert!(resource::Aggregate::same(&empty_a, &empty_b));
        assert!(!empty_a.nodes.ptr_eq(&empty_b.nodes));

        let mut nodes = crate::SharedVector::default();
        nodes.push_back(NodeId(1));
        let mut plans = crate::SharedVector::default();
        plans.push_back(crate::ir::SlotPlan {
            base: NodePath("root".into()),
            expression: ExprId {
                hash: 1,
                key: "k".into(),
            },
            accepts: Vec::new(),
        });
        let shared = Output {
            nodes: nodes.clone(),
            plans: plans.clone(),
        };
        assert!(resource::Aggregate::same(&shared, &shared.clone()));

        let only_nodes = Output {
            nodes: nodes.clone(),
            plans: Default::default(),
        };
        assert!(!resource::Aggregate::same(&shared, &only_nodes));

        let distinct_nodes = crate::SharedVector::from(vec![NodeId(1)]);
        let distinct_plans = crate::SharedVector::from(vec![crate::ir::SlotPlan {
            base: NodePath("root".into()),
            expression: ExprId {
                hash: 1,
                key: "k".into(),
            },
            accepts: Vec::new(),
        }]);
        let equal_but_distinct = Output {
            nodes: distinct_nodes,
            plans: distinct_plans,
        };
        assert!(
            !resource::Aggregate::same(&equal_but_distinct, &shared),
            "equal values with distinct roots are not the same aggregate"
        );
    }
    fn machine_library() -> Library {
        use crate::r#lib::{ComponentDef, GroupDef, Role as LibRole, T};
        Library::builder("tasks", "0.1.0")
            .group(GroupDef::new("G", 0, "Test components."))
            .component(
                ComponentDef::new("Column", LibRole::Group)
                    .group("G")
                    .doc("A column.")
                    .req("children", T::nodes())
                    .example("Column([])"),
            )
            .component(
                ComponentDef::new("Leaf", LibRole::Paragraph)
                    .group("G")
                    .doc("A leaf.")
                    .req("text", T::text())
                    .example("Leaf(\"x\")"),
            )
            .component(
                ComponentDef::new("Control", LibRole::Button)
                    .group("G")
                    .doc("A control.")
                    .opt_none("on_click", T::action())
                    .example("Control()"),
            )
            .build()
            .unwrap()
    }
    fn parsed(source: &str) -> crate::syntax::ast::Program {
        crate::syntax::parse_batch(source.as_bytes(), Default::default()).program
    }
    fn first_pass(
        lib: &Library,
        program: &crate::syntax::ast::Program,
    ) -> (Validation, crate::MaintenanceWork, ValidationCache) {
        let options = Options::default();
        let dirty: BTreeSet<String> = program
            .statements
            .iter()
            .map(|statement| statement.name.clone())
            .collect();
        let reverse = BTreeMap::new();
        let (validation, _, work, cache) = crate::validate::validate_incremental(
            lib,
            &options,
            None,
            Changes {
                dirty: &dirty,
                sources: &program.statements,
                reverse: &reverse,
            },
            None,
            None,
            None,
        );
        (validation, work, cache)
    }
    fn unchanged_pass(
        lib: &Library,
        previous: &Validation,
        cache: &ValidationCache,
    ) -> (Validation, usize, crate::MaintenanceWork, ValidationCache) {
        let options = Options::default();
        let dirty = BTreeSet::new();
        let reverse = BTreeMap::new();
        crate::validate::validate_incremental(
            lib,
            &options,
            Some(previous),
            Changes {
                dirty: &dirty,
                sources: &[],
                reverse: &reverse,
            },
            None,
            Some(cache),
            Some(cache),
        )
    }
    #[test]
    fn an_unchanged_second_pass_reuses_tasks_and_counts_range_examination() {
        // §3.8/I7: revalidation must be bounded by the dirty set, so an
        // unchanged pass reuses retained tasks and reports one range visit per
        // retained task.
        let lib = machine_library();
        let program = parsed("root=Leaf(\"a\")\n");
        let (first, _, first_cache) = first_pass(&lib, &program);
        let (second, reused, work, _) = unchanged_pass(&lib, &first, &first_cache);
        assert_eq!(first.ir.nodes, second.ir.nodes);
        assert!(reused > 0, "retained tasks were not reused");
        assert!(
            work.resource_ranges_examined > 0,
            "reuse must charge range examination work"
        );
    }
    #[test]
    fn a_first_pass_over_a_list_charges_range_reconciliation_work() {
        let lib = machine_library();
        let program = parsed("root=Column([Leaf(\"a\"), Leaf(\"b\")])\n");
        let (validation, work, _) = first_pass(&lib, &program);
        assert!(validation.ir.root.is_some());
        assert!(
            work.resource_ranges_examined > 0,
            "retained list reconciliation must charge range work"
        );
    }
    #[test]
    fn a_partially_dirty_list_charges_only_the_reused_element_usage() {
        let lib = machine_library();
        let program = parsed("one=Leaf(\"a\")\ntwo=Leaf(\"b\")\nroot=Column([one, two])\n");
        let (first, _, first_cache) = first_pass(&lib, &program);
        let options = Options::default();
        let dirty = BTreeSet::from(["one".to_string()]);
        let reverse = BTreeMap::new();
        let (second, reused, _, _) = crate::validate::validate_incremental(
            &lib,
            &options,
            Some(&first),
            Changes {
                dirty: &dirty,
                sources: &[],
                reverse: &reverse,
            },
            None,
            Some(&first_cache),
            Some(&first_cache),
        );
        assert_eq!(first.ir.nodes, second.ir.nodes);
        assert_eq!(
            reused, 2,
            "reused usage counts only the elements the reconciliation did not evaluate"
        );
    }
    fn stale_range_counts_after(mutate: impl FnOnce(&mut Task)) -> usize {
        let lib = machine_library();
        let program = parsed("root=Column([Leaf(\"a\"), Leaf(\"b\")])\n");
        let (first, _, mut first_cache) = first_pass(&lib, &program);
        let id = TaskId {
            path: "root".into(),
            statement: "root".into(),
        };
        let task = first_cache
            .tasks
            .as_mut()
            .and_then(|machine| machine.records.get_mut(&id))
            .expect("retained root task");
        mutate(task);
        task.ranges
            .get_mut("children")
            .expect("retained slot range")
            .counts
            .set(0, 999);
        let options = Options::default();
        let dirty = BTreeSet::new();
        let reverse = BTreeMap::new();
        let (_, _, _, cache) = crate::validate::validate_incremental(
            &lib,
            &options,
            Some(&first),
            Changes {
                dirty: &dirty,
                sources: &[],
                reverse: &reverse,
            },
            None,
            Some(&first_cache),
            Some(&first_cache),
        );
        let key = RangeKey {
            owner: id,
            name: "children".into(),
        };
        cache
            .tasks
            .as_ref()
            .and_then(|machine| machine.graph.ranges.get(&key))
            .map(|sequence| sequence.counts.prefix(1))
            .expect("retained range")
    }
    #[test]
    fn a_range_is_reused_only_when_every_retained_input_matches() {
        // §3.6/§5.5: geometry (depth, place, ancestors) is part of a task's
        // reuse identity. A mismatch must rebuild the range instead of keeping
        // stale materialized counts.
        assert_eq!(
            stale_range_counts_after(|task| task.depth = 99),
            1,
            "a depth mismatch must rebuild the range"
        );
        assert_eq!(
            stale_range_counts_after(|task| task.place = Some("ghost".into())),
            1,
            "a place mismatch must rebuild the range"
        );
        assert_eq!(
            stale_range_counts_after(|task| {
                task.ancestors.insert("ghost".into());
            }),
            1,
            "an ancestors mismatch must rebuild the range"
        );
    }
    #[test]
    fn a_dirty_single_node_task_reuses_its_equal_output_vector() {
        // §3.6: an equal retained result is kept by identity, so a re-walk of a
        // single-node task does not replace the stored output vector.
        let lib = machine_library();
        let program = parsed("root=Leaf(\"a\")\n");
        let (first, _, mut first_cache) = first_pass(&lib, &program);
        let id = TaskId {
            path: "root".into(),
            statement: "root".into(),
        };
        // Make the retained task ineligible for the early reuse branch without
        // changing its recorded result, forcing the post-walk equal-output path.
        first_cache
            .tasks
            .as_mut()
            .and_then(|machine| machine.records.get_mut(&id))
            .expect("retained root task")
            .depth = 99;
        let before = first_cache
            .tasks
            .as_ref()
            .and_then(|machine| machine.records.get(&id))
            .map(|task| task.output.clone())
            .expect("retained root task");
        let options = Options::default();
        let dirty = BTreeSet::from(["root".to_string()]);
        let reverse = BTreeMap::new();
        let (_, _, _, cache) = crate::validate::validate_incremental(
            &lib,
            &options,
            Some(&first),
            Changes {
                dirty: &dirty,
                sources: &[],
                reverse: &reverse,
            },
            None,
            Some(&first_cache),
            Some(&first_cache),
        );
        let after = cache
            .tasks
            .as_ref()
            .and_then(|machine| machine.records.get(&id))
            .map(|task| task.output.clone())
            .expect("retained root task");
        assert!(
            before.ptr_eq(&after),
            "an equal retained output must keep its shared vector"
        );
    }
    fn retained_pass(
        lib: &Library,
        options: &Options,
        previous: &Validation,
        cache: &ValidationCache,
    ) -> (Validation, ValidationCache) {
        let dirty = BTreeSet::new();
        let reverse = BTreeMap::new();
        let (validation, _, _, next) = crate::validate::validate_incremental(
            lib,
            options,
            Some(previous),
            Changes {
                dirty: &dirty,
                sources: &[],
                reverse: &reverse,
            },
            None,
            Some(cache),
            Some(cache),
        );
        (validation, next)
    }

    fn active_paths(cache: &ValidationCache) -> Vec<String> {
        cache
            .tasks
            .as_ref()
            .map(|machine| {
                machine
                    .active
                    .iter()
                    .map(|id| id.path.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    #[test]
    fn range_stop_changes_activate_and_deactivate_retained_effects() {
        // §3.6: a moved stop exchanges live effects with the retained range.
        // Shrinking must deactivate the stopped suffix, and growing it again
        // must restore those effects even when their retained tasks are reused.
        let lib = machine_library();
        let program = parsed(
            "root=Column([Control(on_click:say(\"a\")), Control(on_click:say(\"bb\")), Control(on_click:say(\"ccc\"))])\n",
        );
        let (all, _, cache) = first_pass(&lib, &program);
        assert!(active_paths(&cache).contains(&"root/children:#2".to_string()));

        let mut options = Options::default();
        options.limits.max_nodes = 1;
        let (small, cache) = retained_pass(&lib, &options, &all, &cache);
        assert!(
            !active_paths(&cache).contains(&"root/children:#1".to_string()),
            "{:?}",
            active_paths(&cache)
        );

        options.limits.max_nodes = 3;
        let (_grown, cache) = retained_pass(&lib, &options, &small, &cache);
        assert!(
            active_paths(&cache).contains(&"root/children:#1".to_string()),
            "growing the stop must reactivate the retained suffix: {:?}",
            active_paths(&cache)
        );

        options.limits.max_nodes = 1;
        let (_shrunk, cache) = retained_pass(&lib, &options, &_grown, &cache);
        assert!(
            !active_paths(&cache).contains(&"root/children:#1".to_string()),
            "shrinking the stop must deactivate the suffix: {:?}",
            active_paths(&cache)
        );
    }

    #[test]
    fn a_slot_range_restores_its_hidden_prefix() {
        // §3.6: a retained slot range owns its path prefix, so a fresh range
        // must un-hide the prefix before its children become visible.
        let lib = machine_library();
        let options = Options::default();
        let mut validator = Validator::new(&Program::default(), Vec::new(), &lib, &options);
        let owner = TaskId {
            path: "root".into(),
            statement: "root".into(),
        };
        let mut machine = Machine::default();
        machine.frames.push(Frame {
            id: owner,
            start: 0,
            profile: Default::default(),
            indices: Default::default(),
            limits: Default::default(),
            owners: Default::default(),
            children: Default::default(),
            child_effects: 0,
            old_source: true,
            old_ranges: Default::default(),
            ranges: Default::default(),
            reuse_ranges: false,
        });
        machine.graph.hide("root/children:");
        assert!(machine.graph.hidden_by("root/children:", true).is_some());
        validator.tasks = Some(machine);
        let component = lib
            .components()
            .iter()
            .find(|component| component.name == "Column")
            .expect("Column component");
        let arg = &component.args[0];
        let parent = NodePath("root".into());
        let ctx = Context::at("root", Some(parent.clone()));
        validator.retained_sequence(
            "children",
            &[],
            &ctx,
            &parent,
            2,
            Some("children"),
            Some((arg, 0)),
        );
        let machine = validator.tasks.as_ref().expect("retained machine");
        assert!(
            machine.graph.hidden_by("root/children:", true).is_none(),
            "a new slot range must show its prefix"
        );
    }
    #[test]
    fn nested_child_effects_reach_the_root_for_deactivation() {
        // §3.6: effects propagate through retained children so that removing an
        // ancestor retracts action plans owned below it.
        let lib = machine_library();
        let program = parsed(
            "$s = 0\nact = restore($s)\nleaf = Control(on_click: act)\ninner = Column([leaf])\nroot = Column([inner])\n",
        );
        let (mut validation, _, mut cache) = first_pass(&lib, &program);
        assert_eq!(validation.ir.actions.len(), 1);
        let machine = cache.tasks.as_mut().expect("retained machine");
        machine.deactivate(
            &TaskId {
                path: "root".into(),
                statement: "root".into(),
            },
            &mut validation.ir,
        );
        assert!(
            validation.ir.actions.is_empty(),
            "nested action plans must be retracted with their ancestor"
        );
    }
    #[test]
    fn literal_only_tasks_own_no_effect_frames() {
        // §5.5: owner frames that intern no expression, action or fallback are
        // not effects; retaining them would make a literal-only node look
        // effectful to activation/deactivation.
        let lib = machine_library();
        let program = parsed("root = Leaf(\"a\")\n");
        let (_, _, cache) = first_pass(&lib, &program);
        let id = TaskId {
            path: "root".into(),
            statement: "root".into(),
        };
        let task = cache
            .tasks
            .as_ref()
            .and_then(|machine| machine.records.get(&id))
            .expect("retained root task");
        assert_eq!(
            task.effects, 0,
            "a literal-only task must not own empty effect frames"
        );
        assert!(task.owners.is_empty());
    }
    #[test]
    fn deactivating_a_task_drops_its_expression_free_action_owner() {
        // §5.5: action plans are retained by their source owner. An owner whose
        // plan interns no expression (restore) must still be captured so that
        // deactivation removes the plan from the IR.
        let lib = machine_library();
        let program = parsed("$s = 0\nact = restore($s)\nroot = Control(on_click: act)\n");
        let (validation, _, mut cache) = first_pass(&lib, &program);
        assert_eq!(validation.ir.actions.len(), 1);
        let mut validation = validation;
        let machine = cache.tasks.as_mut().expect("retained machine");
        machine.deactivate(
            &TaskId {
                path: "root".into(),
                statement: "root".into(),
            },
            &mut validation.ir,
        );
        assert!(
            validation.ir.actions.is_empty(),
            "owners must be captured even when they intern no expression"
        );
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct TaskId {
    path: String,
    statement: String,
}
#[derive(Clone)]
struct OwnedArena {
    key: String,
    expression: Option<crate::arena::OwnerSnapshot>,
    action: Option<ActionPlan>,
    fallbacks: Vec<(ExprId, std::sync::Arc<FallbackPlan>)>,
}
impl OwnedArena {
    fn capture(key: &str, ir: &Ir) -> Self {
        let expression = ir.expressions.snapshot_owner(key);
        let action = ir.actions.get(&ActionPlanId(key.into())).cloned();
        // Fallback owners have exactly one unique expression. Other owner keys
        // never need a scan of the fallback arena.
        let fallbacks = if key.ends_with("|$fallback") {
            ir.expressions
                .owner_ids(key)
                .into_iter()
                .filter_map(|id| ir.fallbacks.get_shared(&id).map(|fallback| (id, fallback)))
                .collect()
        } else {
            Vec::new()
        };
        Self {
            key: key.into(),
            expression,
            action,
            fallbacks,
        }
    }
    fn restore(&self, ir: &mut Ir) {
        if let Some(owner) = &self.expression {
            ir.expressions.restore_owner(owner);
        }
        if let Some(action) = &self.action {
            ir.actions
                .insert(ActionPlanId(self.key.as_str().into()), action.clone());
        }
        for (id, plan) in &self.fallbacks {
            ir.fallbacks.insert(id.clone(), plan.as_ref().clone());
        }
    }
}
#[derive(Clone)]
struct OwnedDiagnostic {
    value: Diagnostic,
    order: Option<Vec<u64>>,
}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Event {
    order: Vec<u64>,
    task: TaskId,
    index: usize,
}
#[derive(Clone)]
struct Task {
    source: std::sync::Arc<Expr>,
    profile: resource::Profile,
    output: crate::SharedVector<NodeId>,
    plans: crate::SharedVector<SlotPlan>,
    diagnostics: Vec<OwnedDiagnostic>,
    owners: Vec<OwnedArena>,
    children: im::OrdSet<TaskId>,
    effects: usize,
    ranges: crate::SharedMap<String, Sequence>,
    reject: bool,
    reads: im::OrdSet<String>,
    context: Context,
    depth: usize,
    place: Option<String>,
    ancestors: BTreeSet<String>,
}
pub(super) struct Frame {
    id: TaskId,
    start: usize,
    profile: resource::Profile,
    pub(super) indices: BTreeSet<usize>,
    pub(super) limits: BTreeMap<usize, Vec<u64>>,
    pub(super) owners: BTreeSet<String>,
    children: im::OrdSet<TaskId>,
    child_effects: usize,
    old_source: bool,
    old_ranges: crate::SharedMap<String, Sequence>,
    ranges: crate::SharedMap<String, Sequence>,
    reuse_ranges: bool,
}
#[derive(Default)]
pub(super) struct Machine {
    pub(super) graph: Graph,
    records: crate::SharedMap<TaskId, Task>,
    readers: crate::SharedMap<String, im::OrdSet<TaskId>>,
    parents: crate::SharedMap<TaskId, im::OrdSet<TaskId>>,
    dirty: im::OrdSet<TaskId>,
    active: im::OrdSet<TaskId>,
    diagnostics: crate::Diagnostics,
    metadata_diagnostics: crate::Diagnostics,
    limits: crate::SharedMap<(String, Code), crate::SharedMap<Event, Diagnostic>>,
    // Ephemeral per-validation frames are reset, never cloned as retained work.
    pub(super) frames: Vec<Frame>,
    pub(super) changed_sources: BTreeSet<String>,
    range_parent: crate::SharedMap<TaskId, (RangeKey, usize)>,
    range_dirty: crate::SharedMap<RangeKey, im::OrdSet<usize>>,
    current_range: Option<(RangeKey, usize)>,
    current_reject: bool,
    pub(super) location: Location,
    pub(super) owned_indices: BTreeSet<usize>,
    pub(super) reused_nodes: usize,
    previous_graph: Graph,
    projected: crate::SharedMap<NodeId, Node>,
    states: crate::SharedMap<String, StateDef>,
    checked: crate::SharedMap<String, Checked>,
    actions: crate::SharedMap<String, Expr>,
    pub(super) sources: crate::SharedMap<String, Statement>,
    source_versions: crate::SharedMap<String, Statement>,
    pub(super) bindings: crate::SharedMap<String, crate::validate::binding::Entry>,
    pub(super) stages: crate::SharedMap<String, Vec<Diagnostic>>,
    pub(super) ambiguous: im::OrdSet<String>,
    pub(super) values: crate::SharedMap<String, ValueEntry>,
    pub(super) templates: crate::SharedMap<String, Expr>,
    pub(super) queries: crate::SharedMap<String, QueryDef>,
    pub(super) expressions: crate::ExpressionArena,
    pub(super) action_plans: crate::ActionArena,
    pub(super) fallbacks: crate::SharedMap<ExprId, FallbackPlan>,
    pub(super) diagnostic_work: u64,
}
impl Clone for Machine {
    fn clone(&self) -> Self {
        Self {
            graph: self.graph.clone(),
            records: self.records.clone(),
            readers: self.readers.clone(),
            parents: self.parents.clone(),
            dirty: self.dirty.clone(),
            active: self.active.clone(),
            diagnostics: self.diagnostics.clone(),
            metadata_diagnostics: self.metadata_diagnostics.clone(),
            limits: self.limits.clone(),
            previous_graph: self.graph.clone(),
            projected: self.projected.clone(),
            states: self.states.clone(),
            checked: self.checked.clone(),
            actions: self.actions.clone(),
            sources: self.sources.clone(),
            source_versions: self.source_versions.clone(),
            bindings: self.bindings.clone(),
            stages: self.stages.clone(),
            ambiguous: self.ambiguous.clone(),
            values: self.values.clone(),
            templates: self.templates.clone(),
            queries: self.queries.clone(),
            expressions: self.expressions.clone(),
            action_plans: self.action_plans.clone(),
            fallbacks: self.fallbacks.clone(),
            range_parent: self.range_parent.clone(),
            range_dirty: self.range_dirty.clone(),
            ..Self::default()
        }
    }
}
impl Clone for Frame {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            start: self.start,
            profile: self.profile,
            indices: self.indices.clone(),
            limits: self.limits.clone(),
            owners: self.owners.clone(),
            children: self.children.clone(),
            child_effects: self.child_effects,
            old_source: self.old_source,
            old_ranges: self.old_ranges.clone(),
            ranges: self.ranges.clone(),
            reuse_ranges: self.reuse_ranges,
        }
    }
}
pub(crate) struct Changes<'a> {
    pub dirty: &'a BTreeSet<String>,
    pub sources: &'a [Statement],
    pub reverse: &'a BTreeMap<String, BTreeSet<String>>,
}
impl Machine {
    pub(super) fn metadata_root(
        &mut self,
        source: Option<&Self>,
        work: &mut crate::MaintenanceWork,
    ) {
        let next = source.map_or_else(crate::Diagnostics::default, |source| {
            source.metadata_diagnostics.clone()
        });
        self.diagnostics
            .replace_layer(&self.metadata_diagnostics, &next, work);
        self.metadata_diagnostics = next;
        self.sources = source.map_or_else(Default::default, |source| source.sources.clone());
        self.stages = source.map_or_else(Default::default, |source| source.stages.clone());
        self.ambiguous = source.map_or_else(Default::default, |source| source.ambiguous.clone());
    }
    pub(super) fn retain_ir(&mut self, ir: &Ir) {
        self.values = ir.values.clone();
        self.templates = ir.templates.clone();
        self.queries = ir.queries.clone();
        self.states = ir.states.clone();
        self.expressions = ir.expressions.clone();
        self.action_plans = ir.actions.clone();
        self.fallbacks = ir.fallbacks.clone();
    }
    pub(super) fn source_ir(&self, ir: &mut Ir) {
        ir.values = self.values.clone();
        ir.templates = self.templates.clone();
        ir.queries = self.queries.clone();
        ir.states = self.states.clone();
    }
    pub(super) fn arenas(&self, ir: &mut Ir) {
        ir.expressions = self.expressions.clone();
        ir.actions = self.action_plans.clone();
        ir.fallbacks = self.fallbacks.clone();
    }
    pub(super) fn stage(&mut self, key: String, diagnostics: Vec<Diagnostic>) -> usize {
        let mut work = diagnostics.len();
        if let Some(old) = self.stages.remove(&key) {
            work += old.len();
            for diagnostic in old {
                self.diagnostics.remove_raw(&diagnostic);
                self.metadata_diagnostics.remove_raw(&diagnostic);
            }
        }
        if !diagnostics.is_empty() {
            self.diagnostics.extend_raw(diagnostics.iter().cloned());
            self.metadata_diagnostics
                .extend_raw(diagnostics.iter().cloned());
            self.stages.insert(key, diagnostics);
        }
        work
    }
    pub(super) fn begin(
        &mut self,
        changes: &Changes<'_>,
        removed: impl IntoIterator<Item = String>,
    ) {
        self.previous_graph = self.graph.clone();
        self.frames.clear();
        self.reused_nodes = 0;
        self.diagnostic_work = 0;
        self.owned_indices.clear();
        self.changed_sources = changes
            .sources
            .iter()
            .filter(|source| source.kind != StatementKind::State)
            .map(|source| source.name.clone())
            .collect();
        self.changed_sources.extend(removed);
        self.invalidate(
            changes.dirty.iter().cloned().chain(
                self.changed_sources
                    .iter()
                    .map(|name| format!("@{name}"))
                    .collect::<Vec<_>>(),
            ),
        );
    }
    pub(super) fn metadata(
        &mut self,
        ir: &Ir,
        checked: &crate::SharedMap<String, Checked>,
        actions: &crate::SharedMap<String, Expr>,
        reverse: &BTreeMap<String, BTreeSet<String>>,
    ) {
        let changed_sources = self.sources.changed_keys(&self.source_versions);
        self.changed_sources.extend(changed_sources.iter().cloned());
        self.invalidate(
            changed_sources
                .iter()
                .map(|name| format!("@{name}"))
                .chain(changed_sources.iter().cloned()),
        );
        self.source_versions = self.sources.clone();
        let mut names = Vec::new();
        for name in ir.states.changed_keys(&self.states) {
            if ir.states.get(&name) != self.states.get(&name) {
                names.push(format!("${name}"));
            }
        }
        for name in checked.changed_keys(&self.checked) {
            if checked.get(&name) != self.checked.get(&name) {
                names.push(name);
            }
        }
        for name in actions.changed_keys(&self.actions) {
            if actions.get(&name) != self.actions.get(&name) {
                names.push(name);
            }
        }
        let mut all = BTreeSet::new();
        while let Some(name) = names.pop() {
            if all.insert(name.clone()) {
                names.extend(reverse.get(&name).into_iter().flatten().cloned());
            }
        }
        self.invalidate(all);
        self.states = ir.states.clone();
        self.checked = checked.clone();
        self.actions = actions.clone();
    }
    pub(super) fn finish(&mut self) -> (crate::SharedMap<NodeId, Node>, u64) {
        let (projected, work) = self.graph.project(&self.previous_graph, &self.projected);
        self.projected = projected.clone();
        (projected, work)
    }
    pub(super) fn diagnostics(&self) -> crate::Diagnostics {
        self.diagnostics.clone()
    }
    pub(super) fn discard_root(&mut self, ir: &mut Ir) {
        self.deactivate(
            &TaskId {
                path: "root".into(),
                statement: "root".into(),
            },
            ir,
        );
        self.graph.hide("root");
    }
    pub(super) fn invalidate_sources(
        &mut self,
        names: &BTreeSet<String>,
        reverse: &BTreeMap<String, BTreeSet<String>>,
    ) {
        let mut pending: Vec<_> = names.iter().cloned().collect();
        let mut affected = BTreeSet::new();
        while let Some(name) = pending.pop() {
            if affected.insert(name.clone()) {
                pending.extend(reverse.get(&name).into_iter().flatten().cloned());
            }
        }
        self.invalidate(affected);
    }
    fn invalidate(&mut self, names: impl IntoIterator<Item = String>) {
        let mut pending = Vec::new();
        for name in names {
            pending.extend(self.readers.get(&name).into_iter().flatten().cloned());
        }
        let mut seen = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            self.dirty.insert(id.clone());
            if let Some((range, index)) = self.range_parent.get(&id).cloned() {
                if let Some(dirty) = self.range_dirty.get_mut(&range) {
                    dirty.insert(index);
                } else {
                    self.range_dirty
                        .insert(range, std::iter::once(index).collect());
                }
            }
            pending.extend(self.parents.get(&id).into_iter().flatten().cloned());
        }
    }
    pub(super) fn charge(&mut self, allocated: usize, accepted: bool) {
        for frame in &mut self.frames {
            let offset = allocated.saturating_sub(frame.start);
            if accepted {
                frame.profile.lower = frame.profile.lower.max(offset + 1);
            } else {
                frame.profile.upper = Some(
                    frame
                        .profile
                        .upper
                        .map_or(offset, |upper| upper.min(offset)),
                );
            }
        }
    }
    fn constrain(&mut self, allocated: usize, profile: resource::Profile) {
        for frame in &mut self.frames {
            let offset = allocated.saturating_sub(frame.start);
            frame.profile.lower = frame
                .profile
                .lower
                .max(offset.saturating_add(profile.lower));
            if let Some(upper) = profile.upper {
                let upper = offset.saturating_add(upper);
                frame.profile.upper = Some(frame.profile.upper.map_or(upper, |old| old.min(upper)));
            }
        }
    }
    fn diagnostic(&mut self, id: &TaskId, index: usize, owned: &OwnedDiagnostic, add: bool) {
        self.diagnostic_work += 1;
        if let Some(order) = &owned.order {
            let key = (
                owned.value.stmt.clone().unwrap_or_default(),
                owned.value.code,
            );
            let mut events = self.limits.get(&key).cloned().unwrap_or_default();
            let before = events.values().next().cloned();
            let event = Event {
                order: order.clone(),
                task: id.clone(),
                index,
            };
            if add {
                events.insert(event, owned.value.clone());
            } else {
                events.remove(&event);
            }
            let after = events.values().next().cloned();
            if before != after {
                if let Some(before) = before {
                    self.diagnostics.remove_raw(&before);
                }
                if let Some(after) = after {
                    self.diagnostics.extend_raw([after]);
                }
            }
            if events.is_empty() {
                self.limits.remove(&key);
            } else {
                self.limits.insert(key, events);
            }
        } else if add {
            self.diagnostics.extend_raw([owned.value.clone()]);
        } else {
            self.diagnostics.remove_raw(&owned.value);
        }
    }
    fn remove_own(&mut self, id: &TaskId, task: &Task, ir: &mut Ir) {
        for (index, diagnostic) in task.diagnostics.iter().enumerate() {
            self.diagnostic(id, index, diagnostic, false);
        }
        for owner in &task.owners {
            ir.drop_action(&ActionPlanId(owner.key.as_str().into()));
        }
    }
    fn deactivate(&mut self, id: &TaskId, ir: &mut Ir) {
        if self.active.remove(id).is_none() {
            return;
        }
        let Some(task) = self.records.get_shared(id) else {
            return;
        };
        self.remove_own(id, &task, ir);
        self.graph.hide(&id.path);
        for range in task.ranges.values() {
            range
                .index
                .visit_effects(None, None, |_, input, _| self.deactivate(&input.id(), ir));
        }
        for child in &task.children {
            if self
                .records
                .get(child)
                .is_some_and(|child| child.effects > 0)
            {
                self.deactivate(child, ir);
            }
        }
    }
    fn activate(&mut self, id: &TaskId, ir: &mut Ir) {
        if self.active.contains(id) {
            return;
        }
        let Some(task) = self.records.get_shared(id) else {
            return;
        };
        for (index, diagnostic) in task.diagnostics.iter().enumerate() {
            self.diagnostic(id, index, diagnostic, true);
        }
        for owner in &task.owners {
            owner.restore(ir);
        }
        for range in task.ranges.values() {
            range
                .index
                .visit_effects(None, None, |_, input, _| self.activate(&input.id(), ir));
        }
        for child in &task.children {
            if self
                .records
                .get(child)
                .is_some_and(|child| child.effects > 0)
            {
                self.activate(child, ir);
            }
        }
        self.active.insert(id.clone());
        if task.output.is_empty() {
            self.graph.hide(&id.path);
        } else {
            self.graph.show(&id.path);
        }
    }
}

impl Validator<'_> {
    pub(super) fn walk_retained(
        &mut self,
        expression: &Expr,
        ctx: &Context,
        path: NodePath,
        depth: usize,
        place: Option<&str>,
    ) -> crate::SharedVector<NodeId> {
        let Some(mut machine) = self.tasks.take() else {
            return self.walk_expr_inner(expression, ctx, path, depth, place);
        };
        let id = TaskId {
            path: path.0.clone(),
            statement: ctx.stmt.clone(),
        };
        let old = machine.records.get_shared(&id);
        let old_source = old.is_some() && !machine.changed_sources.contains(&ctx.stmt);
        let prior_effects = old.as_ref().map_or(0, |task| task.effects);
        let reject = std::mem::take(&mut machine.current_reject);
        if let Some(parent) = machine.frames.last_mut() {
            let parent_id = parent.id.clone();
            if let Some((range, index)) = &machine.current_range
                && range.owner == parent_id
            {
                machine
                    .range_parent
                    .insert(id.clone(), (range.clone(), *index));
            } else {
                if parent.children.contains(&id) {
                    parent.child_effects = parent.child_effects.saturating_sub(prior_effects);
                }
                parent.children.insert(id.clone());
            }
            if let Some(parents) = machine.parents.get_mut(&id) {
                parents.insert(parent_id);
            } else {
                machine
                    .parents
                    .insert(id.clone(), std::iter::once(parent_id).collect());
            }
        }
        if let Some(task) = &old {
            if !machine.dirty.contains(&id)
                && task.context == *ctx
                && task.depth == depth
                && task.place.as_deref() == place
                && task.ancestors == self.active
                && task.reject == reject
                && task
                    .profile
                    .valid(self.limits.max_nodes.saturating_sub(self.allocated))
            {
                machine.constrain(self.allocated, task.profile);
                machine.activate(&id, &mut self.ir);
                self.allocated += task.profile.used;
                machine.reused_nodes += task.profile.used;
                self.direct_limit = task.profile.stopped;
                self.pending_plans.append_shared(&task.plans);
                if let Some(parent) = machine.frames.last_mut()
                    && parent.children.contains(&id)
                {
                    parent.child_effects += task.effects;
                }
                self.work.resource_ranges_examined += 1;
                let output = task.output.clone();
                self.tasks = Some(machine);
                return output;
            }
            if machine.active.contains(&id) {
                machine.remove_own(&id, task, &mut self.ir);
            }
        }
        let children = im::OrdSet::new();
        let child_effects = 0;
        machine.frames.push(Frame {
            id: id.clone(),
            start: self.allocated,
            profile: Default::default(),
            indices: BTreeSet::new(),
            limits: BTreeMap::new(),
            owners: BTreeSet::new(),
            children,
            child_effects,
            old_source,
            old_ranges: old
                .as_ref()
                .map_or_else(Default::default, |task| task.ranges.clone()),
            ranges: Default::default(),
            reuse_ranges: old_source
                && old.as_ref().is_some_and(|task| {
                    task.context == *ctx
                        && task.depth == depth
                        && task.place.as_deref() == place
                        && task.ancestors == self.active
                }),
        });
        let source = if old_source {
            old.as_ref().map(|task| task.source.clone())
        } else {
            None
        };
        let source = source.unwrap_or_else(|| std::sync::Arc::new(expression.clone()));
        let retained = self.retained_instance.take();
        if old_source && let Some(instance) = self.instances.get(&self.ids.allocate(&path)) {
            self.retained_instance = Some(instance.clone());
        }
        let ancestors = self.active.clone();
        let pending = std::mem::take(&mut self.pending_plans);
        self.tasks = Some(machine);
        let mut output = if reject {
            self.emit(Code::SlotKind, ctx);
            self.direct_limit = false;
            Default::default()
        } else {
            self.walk_expr_inner(&source, ctx, path.clone(), depth, place)
        };
        if let Some(old) = &old
            && output.len() <= 1
            && output == old.output
        {
            output = old.output.clone();
        }
        self.retained_instance = retained;
        let plans = std::mem::replace(&mut self.pending_plans, pending);
        self.pending_plans.append_shared(&plans);
        let Some(mut machine) = self.tasks.take() else {
            return output;
        };
        let Some(mut frame) = machine.frames.pop() else {
            self.tasks = Some(machine);
            return output;
        };
        frame.profile.used = self.allocated.saturating_sub(frame.start);
        frame.profile.stopped = self.direct_limit;
        let diagnostics: Vec<_> = frame
            .indices
            .iter()
            .filter_map(|index| {
                self.diagnostics.get(*index).map(|value| {
                    machine.owned_indices.insert(*index);
                    OwnedDiagnostic {
                        value: value.clone(),
                        order: frame.limits.get(index).cloned(),
                    }
                })
            })
            .collect();
        let owners: Vec<_> = frame
            .owners
            .iter()
            .map(|key| OwnedArena::capture(key, &self.ir))
            .filter(|owner| {
                owner.expression.is_some() || owner.action.is_some() || !owner.fallbacks.is_empty()
            })
            .collect();
        if let Some(old) = &old {
            for child in &old.children {
                if !frame.children.contains(child) {
                    machine.deactivate(child, &mut self.ir);
                }
            }
            for (name, range) in &old.ranges {
                if !frame.ranges.contains_key(name) {
                    range.index.visit_effects(None, None, |_, input, _| {
                        machine.deactivate(&input.id(), &mut self.ir)
                    });
                    if let Some(prefix) = &range.slot_prefix {
                        machine.graph.hide(prefix);
                    }
                }
            }
            if !old_source {
                for name in &old.reads {
                    if let Some(readers) = machine.readers.get_mut(name) {
                        readers.remove(&id);
                    }
                }
            }
        }
        let reads = if old_source {
            old.as_ref()
                .map_or_else(im::OrdSet::new, |task| task.reads.clone())
        } else {
            let mut reads = BTreeSet::new();
            crate::session::dependencies(&source, &mut reads);
            reads.insert(format!("@{}", ctx.stmt));
            let reads: im::OrdSet<String> = reads.into_iter().collect();
            for name in &reads {
                if let Some(readers) = machine.readers.get_mut(name) {
                    readers.insert(id.clone());
                } else {
                    machine
                        .readers
                        .insert(name.clone(), std::iter::once(id.clone()).collect());
                }
            }
            reads
        };
        let effects = frame.child_effects
            + diagnostics.len()
            + owners.len()
            + frame
                .ranges
                .values()
                .map(|range| range.profile.effects)
                .sum::<usize>();
        frame.profile.effects = effects;
        let task = Task {
            source,
            profile: frame.profile,
            output: output.clone(),
            plans,
            diagnostics,
            owners,
            children: frame.children,
            effects,
            ranges: frame.ranges,
            reject,
            reads,
            context: ctx.clone(),
            depth,
            place: place.map(str::to_owned),
            ancestors,
        };
        for (index, diagnostic) in task.diagnostics.iter().enumerate() {
            machine.diagnostic(&id, index, diagnostic, true);
        }
        if let Some(parent) = machine.frames.last_mut()
            && parent.children.contains(&id)
        {
            parent.child_effects += effects;
        }
        machine
            .graph
            .outputs
            .insert(id.path.clone(), output.clone());
        if output.is_empty() {
            machine.graph.hide(&id.path);
        } else {
            machine.graph.show(&id.path);
        }
        machine.active.insert(id.clone());
        machine.dirty.remove(&id);
        machine.records.insert(id, task);
        self.tasks = Some(machine);
        output
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct RangeKey {
    owner: TaskId,
    name: String,
}
#[derive(Clone)]
struct Input {
    expression: std::sync::Arc<Expr>,
    context: Context,
    path: NodePath,
    depth: usize,
    place: Option<String>,
    reject: bool,
}
impl Input {
    fn id(&self) -> TaskId {
        TaskId {
            path: self.path.0.clone(),
            statement: self.context.stmt.clone(),
        }
    }
}
impl PartialEq for Input {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.expression, &other.expression)
            && self.context == other.context
            && self.path == other.path
            && self.depth == other.depth
            && self.place == other.place
            && self.reject == other.reject
    }
}
#[derive(Clone, Default)]
struct Output {
    nodes: crate::SharedVector<NodeId>,
    plans: crate::SharedVector<SlotPlan>,
}
impl resource::Aggregate for Output {
    fn same(&self, other: &Self) -> bool {
        (self.nodes.ptr_eq(&other.nodes) || (self.nodes.is_empty() && other.nodes.is_empty()))
            && (self.plans.ptr_eq(&other.plans)
                || (self.plans.is_empty() && other.plans.is_empty()))
    }
    fn append(&mut self, other: &Self) {
        self.nodes.append_shared(&other.nodes);
        self.plans.append_shared(&other.plans);
    }
}
#[derive(Clone, Default)]
struct Sequence {
    index: resource::ResourceIndex<usize, Input, Output>,
    counts: PrefixCounts,
    profile: resource::Profile,
    slot_prefix: Option<String>,
}
impl Validator<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn retained_sequence(
        &mut self,
        name: &str,
        expressions: &[Expr],
        ctx: &Context,
        parent: &NodePath,
        depth: usize,
        place: Option<&str>,
        slot: Option<(&crate::r#lib::ArgDef, usize)>,
    ) -> crate::SharedVector<NodeId> {
        let Some(machine) = &mut self.tasks else {
            return Default::default();
        };
        let Some(frame) = machine.frames.last() else {
            return Default::default();
        };
        let key = RangeKey {
            owner: frame.id.clone(),
            name: name.into(),
        };
        let old = frame.old_ranges.get(name).cloned();
        let reused = frame.reuse_ranges && old.is_some();
        let mut sequence = if reused {
            old.clone().unwrap_or_default()
        } else {
            Sequence {
                counts: PrefixCounts::empty(expressions.len().min(self.limits.max_list_len)),
                slot_prefix: slot.map(|_| format!("{}/{}:", parent.0, name)),
                ..Default::default()
            }
        };
        if !reused {
            let mut repeats = BTreeMap::<String, usize>::new();
            for (index, expression) in expressions
                .iter()
                .take(self.limits.max_list_len)
                .enumerate()
            {
                let mut context = ctx.clone();
                let path = if let Some((arg, slot_index)) = slot {
                    let base = match expression {
                        Expr::Name { name } => {
                            let repeat = repeats.entry(name.clone()).or_default();
                            *repeat += 1;
                            if *repeat == 1 {
                                format!("@{name}")
                            } else {
                                format!("@{name}~{repeat}")
                            }
                        }
                        Expr::Call { name, .. } if name == "each" => format!("e{index}"),
                        _ => format!("#{index}"),
                    };
                    context.order.push((name.into(), index, 0));
                    context
                        .allocation_order
                        .extend([1, slot_index as u64, index as u64, 0]);
                    context.accepts =
                        if let Some(crate::r#lib::Accepts::Only(allowed)) = arg.ty.slot() {
                            allowed.clone()
                        } else {
                            Vec::new()
                        };
                    parent.child(name, &base)
                } else {
                    if let Some(last) = context.order.last_mut() {
                        last.2 = index;
                    }
                    if let Some(last) = context.allocation_order.last_mut() {
                        *last = index as u64;
                    }
                    NodePath(format!("{}#{index}", parent.0))
                };
                context.path = Some(path.clone());
                sequence.index.insert(
                    index,
                    Input {
                        expression: std::sync::Arc::new(expression.clone()),
                        context,
                        path,
                        depth,
                        place: place.map(str::to_owned),
                        reject: slot.is_none() && matches!(expression, Expr::List { .. }),
                    },
                );
            }
        }
        if let Some(dirty) = machine.range_dirty.remove(&key) {
            for index in dirty {
                sequence.index.invalidate(&index);
            }
        }
        if let Some(prefix) = &sequence.slot_prefix {
            machine.graph.show(prefix);
        }
        if !reused && let Some(old) = &old {
            old.index.visit_effects(None, None, |_, input, _| {
                machine.deactivate(&input.id(), &mut self.ir)
            });
            old.index
                .visit_materialized(None, None, |_, input, _| machine.graph.hide(&input.path.0));
        }
        let start = self.allocated;
        let previous_stop = old
            .as_ref()
            .and_then(|old| old.index.stopping_key().copied());
        let old_index = old.as_ref().map(|old| old.index.clone());
        let mut counts = sequence.counts.clone();
        let mut range_work = 0;
        let mut evaluated_used = 0;
        let (profile, output) = sequence.index.reconcile_with(
            self.limits.max_nodes.saturating_sub(start),
            &mut range_work,
            |index, input, available| {
                self.allocated = self.limits.max_nodes.saturating_sub(available);
                let pending = std::mem::take(&mut self.pending_plans);
                let (prior_range, prior_location) = if let Some(machine) = &mut self.tasks {
                    machine.current_reject = input.reject;
                    let previous = machine.current_range.replace((key.clone(), *index));
                    let old_location = machine.location.clone();
                    if slot.is_some() {
                        machine.location = Location {
                            parent: Some(self.ids.allocate(parent)),
                            slot: name.into(),
                            chain: Default::default(),
                        };
                    }
                    machine.location.chain.push_back((key.clone(), *index));
                    (previous, old_location)
                } else {
                    (None, Location::default())
                };
                let nodes = self.walk_expr(
                    &input.expression,
                    &input.context,
                    input.path.clone(),
                    input.depth,
                    input.place.as_deref(),
                );
                let plans = std::mem::replace(&mut self.pending_plans, pending);
                let mut task_profile = resource::Profile::default();
                if let Some(machine) = &mut self.tasks {
                    machine.current_range = prior_range;
                    machine.location = prior_location;
                    if let Some(task) = machine.records.get(&input.id()) {
                        task_profile = task.profile;
                    }
                }
                counts.set(*index, nodes.len());
                evaluated_used += task_profile.used;
                (task_profile, Output { nodes, plans })
            },
        );
        self.work.resource_ranges_examined += range_work;
        let stop = sequence.index.stopping_key().copied();
        if let Some(machine) = &mut self.tasks {
            if reused
                && let Some(old_index) = old_index
                && let Some(stop) = stop
                && previous_stop.is_none_or(|previous| stop < previous)
            {
                old_index.visit_effects(Some(&stop), previous_stop.as_ref(), |_, input, _| {
                    machine.deactivate(&input.id(), &mut self.ir)
                });
            }
            if reused
                && let Some(previous) = previous_stop
                && stop.is_none_or(|stop| stop > previous)
            {
                sequence.index.visit_materialized(
                    Some(&previous),
                    stop.as_ref(),
                    |index, _, result| counts.set(*index, result.nodes.len()),
                );
                sequence
                    .index
                    .visit_effects(Some(&previous), stop.as_ref(), |_, input, _| {
                        machine.activate(&input.id(), &mut self.ir)
                    });
            }
            machine.reused_nodes += profile.used.saturating_sub(evaluated_used);
            machine.constrain(start, profile);
            sequence.counts = counts;
            sequence.profile = profile;
            machine.graph.ranges.insert(key.clone(), sequence.clone());
            if let Some(frame) = machine.frames.last_mut() {
                frame.ranges.insert(name.into(), sequence);
            }
        }
        self.allocated = start.saturating_add(profile.used);
        self.direct_limit = profile.stopped;
        self.pending_plans.append_shared(&output.plans);
        output.nodes
    }
}

#[derive(Clone, Default, PartialEq)]
pub(super) struct Location {
    parent: Option<NodeId>,
    slot: String,
    chain: crate::SharedVector<(RangeKey, usize)>,
}
impl Machine {
    pub(super) fn node_location(&mut self, id: NodeId, location: Location) {
        if self.graph.locations.get(&id) != Some(&location) {
            self.graph.locations.insert(id, location);
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Address {
    pub path: String,
    pub parent: Option<String>,
    pub slot: String,
    pub index: usize,
    pub order: Vec<(String, usize)>,
}
#[derive(Clone, Default)]
pub(crate) struct Projection {
    pub ids: Vec<NodeId>,
    pub before: BTreeMap<NodeId, Address>,
    pub after: BTreeMap<NodeId, Address>,
}
impl Graph {
    fn address(&self, id: NodeId) -> Option<Address> {
        let location = self.locations.get(&id)?;
        let index = location
            .chain
            .iter()
            .map(|(key, index)| {
                self.ranges
                    .get(key)
                    .map_or(0, |range| range.counts.prefix(*index))
            })
            .sum();
        let parent = location
            .parent
            .and_then(|id| self.nodes.get(&id).map(|node| node.path.0.clone()));
        let slot = if parent.is_none() {
            "root".into()
        } else {
            location.slot.clone()
        };
        let mut order = Vec::new();
        let mut current = Some(id);
        let mut seen = BTreeSet::new();
        while let Some(id) = current {
            if !seen.insert(id) {
                return None;
            }
            let location = self.locations.get(&id)?;
            if location.parent.is_some() {
                let index = location
                    .chain
                    .iter()
                    .map(|(key, index)| {
                        self.ranges
                            .get(key)
                            .map_or(0, |range| range.counts.prefix(*index))
                    })
                    .sum();
                order.push((location.slot.clone(), index));
            }
            current = location.parent;
        }
        order.reverse();
        Some(Address {
            path: self.nodes.get(&id)?.path.0.clone(),
            parent,
            slot,
            index,
            order,
        })
    }
}
impl Projection {
    pub(crate) fn between(
        before: &Ir,
        after: &Ir,
        old: Option<&ValidationCache>,
        new: Option<&ValidationCache>,
    ) -> Self {
        let old = old.and_then(|cache| cache.tasks.as_ref());
        let new = new.and_then(|cache| cache.tasks.as_ref());
        let mut ids: BTreeSet<_> = before
            .nodes
            .changed_keys(&after.nodes)
            .into_iter()
            .filter(|id| match (before.nodes.get(id), after.nodes.get(id)) {
                (Some(a), Some(b)) => {
                    a.kind != b.kind || a.props != b.props || a.status != b.status
                }
                _ => true,
            })
            .collect();
        if let (Some(old), Some(new)) = (old, new) {
            ids.extend(
                old.graph
                    .locations
                    .changed_keys(&new.graph.locations)
                    .into_iter()
                    .filter(|id| before.nodes.contains_key(id) || after.nodes.contains_key(id)),
            );
        }
        // Replacing a kind removes its old renderer subtree. Its current children
        // must be inserted even if their semantic IDs/props were retained.
        let mut pending: Vec<_> = ids
            .iter()
            .filter_map(|id| match (before.nodes.get(id), after.nodes.get(id)) {
                (Some(a), Some(b)) if a.kind != b.kind => Some(*id),
                _ => None,
            })
            .collect();
        while let Some(id) = pending.pop() {
            ids.insert(id);
            if let Some(node) = after.nodes.get(&id) {
                pending.extend(
                    node.slots
                        .iter()
                        .flat_map(|slot| slot.children.iter().copied()),
                );
            }
        }
        let old_addresses = ids
            .iter()
            .filter_map(|id| {
                old.and_then(|machine| machine.graph.address(*id))
                    .map(|address| (*id, address))
            })
            .collect();
        let new_addresses = ids
            .iter()
            .filter_map(|id| {
                new.and_then(|machine| machine.graph.address(*id))
                    .map(|address| (*id, address))
            })
            .collect();
        Self {
            ids: ids.into_iter().collect(),
            before: old_addresses,
            after: new_addresses,
        }
    }
}
