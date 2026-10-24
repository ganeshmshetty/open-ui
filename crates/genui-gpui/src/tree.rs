//! The renderer's retained tree and patch application (SPEC §§6.6, 10.1).
//!
//! This module is UI-free: it owns node state and child order, applies
//! `PatchBatch` ops in order, and reports what changed. `UiView` maps the
//! changed entries onto GPUI entities. No op that names bad input panics; the
//! tree reports it and asks the host for a snapshot (SPEC §6.6).

use genui_core::{
    SharedMap, SharedVector,
    ir::{NodeStatus, Role},
    patch::{NodeInit, Op, PatchBatch},
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub type Props = BTreeMap<String, Value>;

#[derive(Clone, Debug, PartialEq)]
pub struct NodeEntry {
    pub id: String,
    pub kind: String,
    pub role: Role,
    pub props: Props,
    pub status: NodeStatus,
    pub parent: Option<String>,
    pub slot: String,
    pub children: BTreeMap<String, SharedVector<String>>,
    /// Bumped whenever props, text or status change. Renderers cache parsed
    /// models (for example a table's rows) against it.
    pub rev: u64,
    /// Bumped only when a `rows` prop is set or unset. Table data is parsed
    /// once and the raw wire prop is dropped from the retained tree, so an
    /// unrelated prop update must not invalidate the parsed rows.
    pub rows_rev: u64,
}

impl NodeEntry {
    fn from_init(node: NodeInit, parent: Option<String>, slot: String) -> Self {
        Self {
            id: node.id,
            kind: node.kind,
            role: node.role,
            props: node.props,
            status: node.status,
            parent,
            slot,
            children: BTreeMap::new(),
            rev: 0,
            rows_rev: 0,
        }
    }

    /// The slot this component's children live in. Structural helpers such as
    /// `TableColumn` have no children.
    pub fn primary_slot(&self) -> &'static str {
        match self.kind.as_str() {
            "List" => "items",
            "Tabs" => "tabs",
            "Table" => "columns",
            _ => "children",
        }
    }

    pub fn children_in(&self, slot: &str) -> Vec<String> {
        self.children
            .get(slot)
            .map(|children| children.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn primary_children(&self) -> Vec<String> {
        self.children_in(self.primary_slot())
    }

    /// The number of children in the primary slot without cloning the ids.
    pub fn primary_len(&self) -> usize {
        self.children
            .get(self.primary_slot())
            .map_or(0, |children| children.len())
    }

    pub fn is_container(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "Column"
                | "Row"
                | "Grid"
                | "Card"
                | "Stat"
                | "Facts"
                | "Form"
                | "List"
                | "Table"
                | "Tabs"
                | "Tab"
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RetainedTree {
    pub epoch: Option<u32>,
    pub next_seq: u64,
    pub root: Option<String>,
    pub nodes: SharedMap<String, NodeEntry>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ApplyReport {
    /// The batch advanced the tree's sequence.
    pub applied: bool,
    /// Ops that named unknown nodes, duplicated IDs, or impossible positions.
    pub errors: Vec<String>,
    /// The host should call `Session::snapshot()` and apply its reset + inserts.
    pub needs_snapshot: bool,
    /// The batch belonged to an older epoch and was ignored (SPEC §6.3 G8).
    pub discarded: bool,
    /// Nodes whose props, text, status or position changed.
    pub dirty: BTreeSet<String>,
}

impl RetainedTree {
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&NodeEntry> {
        self.nodes.get(id)
    }

    /// Deterministic depth-first text used by tests, logs and the inspector.
    pub fn outline(&self) -> String {
        let mut out = String::new();
        if let Some(root) = &self.root {
            self.outline_node(root, 0, &mut out);
        }
        out
    }

    fn outline_node(&self, id: &str, depth: usize, out: &mut String) {
        let Some(node) = self.nodes.get(id) else {
            return;
        };
        for _ in 0..depth {
            out.push_str("  ");
        }
        out.push_str(&node.kind);
        out.push_str("  ");
        out.push_str(&node.id);
        out.push(' ');
        out.push_str(match node.status {
            NodeStatus::Partial => "partial",
            NodeStatus::Sealed => "sealed",
        });
        out.push('\n');
        for child in node.primary_children() {
            self.outline_node(&child, depth + 1, out);
        }
    }

    /// Applies the batch atomically. Out-of-order batches never partially apply.
    pub fn apply(&mut self, batch: &PatchBatch) -> ApplyReport {
        let mut report = ApplyReport::default();
        if let Some(epoch) = self.epoch
            && batch.epoch < epoch
        {
            report.discarded = true;
            return report;
        }
        let reset = matches!(batch.ops.first(), Some(Op::Reset));
        let new_epoch = self.epoch != Some(batch.epoch);
        if (new_epoch && !reset) || (!new_epoch && batch.seq != self.next_seq && !reset) {
            report
                .errors
                .push("epoch/sequence gap: snapshot required".into());
            report.needs_snapshot = true;
            return report;
        }
        if !reset && self.epoch.is_none() {
            report.needs_snapshot = true;
            report.errors.push("batch before the epoch's reset".into());
            return report;
        }
        let mut next = self.clone();
        for op in &batch.ops {
            if let Err(error) = next.apply_op(op, &mut report.dirty) {
                report.errors.push(error);
                report.needs_snapshot = true;
            }
        }
        next.epoch = Some(batch.epoch);
        next.next_seq = batch.seq.saturating_add(1);
        *self = next;
        report.applied = true;
        report
    }

    fn apply_op(&mut self, op: &Op, dirty: &mut BTreeSet<String>) -> Result<(), String> {
        match op {
            Op::Reset => {
                self.nodes.clear();
                self.root = None;
                dirty.clear();
                Ok(())
            }
            Op::Insert {
                parent,
                slot,
                index,
                node,
            } => {
                if self.nodes.contains_key(&node.id) {
                    return Err(format!("insert names an existing node {:?}", node.id));
                }
                if parent.is_none() && self.root.is_some() {
                    return Err("insert a second root".into());
                }
                if let Some(parent_id) = parent {
                    let Some(entry) = self.nodes.get_mut(parent_id) else {
                        return Err(format!("insert under unknown parent {parent_id:?}"));
                    };
                    let children = entry.children.entry(slot.clone()).or_default();
                    let position = usize::try_from(*index)
                        .unwrap_or(usize::MAX)
                        .min(children.len());
                    children.insert(position, node.id.clone());
                } else {
                    self.root = Some(node.id.clone());
                }
                let id = node.id.clone();
                dirty.insert(id.clone());
                self.nodes.insert(
                    id,
                    NodeEntry::from_init(node.clone(), parent.clone(), slot.clone()),
                );
                Ok(())
            }
            Op::SetProps { id, set, unset } => {
                let Some(entry) = self.nodes.get_mut(id) else {
                    return Err(format!("set_props names unknown node {id:?}"));
                };
                for key in unset {
                    entry.props.remove(key);
                }
                for (key, value) in set {
                    entry.props.insert(key.clone(), value.clone());
                }
                entry.rev = entry.rev.saturating_add(1);
                if set.contains_key("rows") || unset.iter().any(|key| key == "rows") {
                    entry.rows_rev = entry.rows_rev.saturating_add(1);
                }
                dirty.insert(id.clone());
                Ok(())
            }
            Op::AppendText { id, prop, text } => {
                let Some(entry) = self.nodes.get_mut(id) else {
                    return Err(format!("append_text names unknown node {id:?}"));
                };
                match entry.props.get_mut(prop) {
                    Some(Value::String(value)) => value.push_str(text),
                    Some(_) => return Err(format!("append_text on non-string {id:?}.{prop}")),
                    None => {
                        entry
                            .props
                            .insert(prop.clone(), Value::String(text.clone()));
                    }
                }
                entry.rev = entry.rev.saturating_add(1);
                dirty.insert(id.clone());
                Ok(())
            }
            Op::Remove { id } => {
                if !self.nodes.contains_key(id) {
                    return Err(format!("remove names unknown node {id:?}"));
                }
                self.detach(id)?;
                self.drop_subtree(id, dirty);
                Ok(())
            }
            Op::Move {
                id,
                parent,
                slot,
                index,
            } => {
                if !self.nodes.contains_key(id) {
                    return Err(format!("move names unknown node {id:?}"));
                }
                if let Some(parent_id) = parent {
                    if parent_id == id || self.is_ancestor(id, parent_id) {
                        return Err(format!("move would place {id:?} inside itself"));
                    }
                    if !self.nodes.contains_key(parent_id) {
                        return Err(format!("move under unknown parent {parent_id:?}"));
                    }
                } else if self.root.as_deref() != Some(id) {
                    return Err("move a non-root node to the root slot".into());
                }
                self.detach(id)?;
                if let Some(parent_id) = parent {
                    let entry = self
                        .nodes
                        .get_mut(parent_id)
                        .ok_or_else(|| format!("move under unknown parent {parent_id:?}"))?;
                    let children = entry.children.entry(slot.clone()).or_default();
                    let position = usize::try_from(*index)
                        .unwrap_or(usize::MAX)
                        .min(children.len());
                    children.insert(position, id.clone());
                } else {
                    self.root = Some(id.clone());
                }
                if let Some(entry) = self.nodes.get_mut(id) {
                    entry.parent = parent.clone();
                    entry.slot = slot.clone();
                }
                dirty.insert(id.clone());
                Ok(())
            }
            Op::SetStatus { id, status } => {
                let Some(entry) = self.nodes.get_mut(id) else {
                    return Err(format!("set_status names unknown node {id:?}"));
                };
                entry.status = *status;
                entry.rev = entry.rev.saturating_add(1);
                dirty.insert(id.clone());
                Ok(())
            }
        }
    }

    fn detach(&mut self, id: &str) -> Result<(), String> {
        let Some(entry) = self.nodes.get(id) else {
            return Err(format!("detach unknown node {id:?}"));
        };
        let parent = entry.parent.clone();
        let slot = entry.slot.clone();
        if let Some(entry) = self.nodes.get_mut(id) {
            entry.parent = None;
            entry.slot = "root".into();
        }
        if let Some(parent_id) = parent {
            if let Some(parent) = self.nodes.get_mut(&parent_id)
                && let Some(children) = parent.children.get_mut(&slot)
                && let Some(index) = children.iter().position(|child| child == id)
            {
                children.remove(index);
            }
        } else if self.root.as_deref() == Some(id) {
            self.root = None;
        }
        Ok(())
    }

    fn drop_subtree(&mut self, id: &str, dirty: &mut BTreeSet<String>) {
        let Some(entry) = self.nodes.get(id) else {
            return;
        };
        let children: Vec<String> = entry
            .children
            .values()
            .flat_map(|children| children.iter().cloned())
            .collect();
        for child in children {
            self.drop_subtree(&child, dirty);
        }
        dirty.insert(id.to_string());
        self.nodes.remove(id);
    }

    fn is_ancestor(&self, ancestor: &str, node: &str) -> bool {
        let mut current = Some(node.to_string());
        let mut guard = 0usize;
        while let Some(id) = current {
            guard += 1;
            if guard > self.nodes.len() + 1 {
                return false;
            }
            if id == ancestor {
                return true;
            }
            current = self.nodes.get(&id).and_then(|entry| entry.parent.clone());
        }
        false
    }

    /// Replaces this tree with a full reset plus inserts, used by the gallery
    /// and tests that render one completed program.
    pub fn from_batches(batches: &[PatchBatch]) -> (Self, Vec<ApplyReport>) {
        let mut tree = Self::default();
        let reports = batches.iter().map(|batch| tree.apply(batch)).collect();
        (tree, reports)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use genui_core::patch::{Op, PatchBatch};
    use serde_json::json;

    fn node(id: &str, kind: &str) -> NodeInit {
        NodeInit {
            id: id.into(),
            kind: kind.into(),
            role: Role::Group,
            props: BTreeMap::new(),
            status: NodeStatus::Sealed,
        }
    }

    fn batch(epoch: u32, seq: u64, ops: Vec<Op>) -> PatchBatch {
        PatchBatch { epoch, seq, ops }
    }

    fn inserted(id: &str, parent: Option<&str>, index: u32, kind: &str) -> Op {
        Op::Insert {
            parent: parent.map(str::to_string),
            slot: if parent.is_some() {
                "children".into()
            } else {
                "root".into()
            },
            index,
            node: node(id, kind),
        }
    }

    fn seeded() -> RetainedTree {
        let mut tree = RetainedTree::default();
        let report = tree.apply(&batch(
            1,
            0,
            vec![
                Op::Reset,
                inserted("root", None, 0, "Column"),
                inserted("a", Some("root"), 0, "Text"),
                inserted("b", Some("root"), 1, "Card"),
                inserted("b1", Some("b"), 0, "Text"),
            ],
        ));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        tree
    }

    #[test]
    fn inserts_keep_sibling_order_and_outline_is_depth_first() {
        let tree = seeded();
        assert_eq!(
            tree.get("root").unwrap().primary_children(),
            vec!["a".to_string(), "b".to_string()]
        );
        assert_eq!(
            tree.outline(),
            "Column  root sealed\n  Text  a sealed\n  Card  b sealed\n    Text  b1 sealed\n"
        );
    }

    #[test]
    fn set_props_append_text_and_unset_update_only_their_nodes() {
        let mut tree = seeded();
        let report = tree.apply(&batch(
            1,
            1,
            vec![
                Op::SetProps {
                    id: "a".into(),
                    set: BTreeMap::from([("text".into(), json!("hel"))]),
                    unset: vec![],
                },
                Op::AppendText {
                    id: "a".into(),
                    prop: "text".into(),
                    text: "lo".into(),
                },
                Op::SetStatus {
                    id: "b".into(),
                    status: NodeStatus::Partial,
                },
                Op::SetProps {
                    id: "a".into(),
                    set: BTreeMap::from([("bold".into(), json!(true))]),
                    unset: vec!["size".into()],
                },
            ],
        ));
        assert!(report.applied);
        assert_eq!(
            report.dirty,
            BTreeSet::from(["a".to_string(), "b".to_string()])
        );
        assert_eq!(tree.get("a").unwrap().props["text"], json!("hello"));
        assert_eq!(tree.get("a").unwrap().props["bold"], json!(true));
        assert_eq!(tree.get("b").unwrap().status, NodeStatus::Partial);
        assert_eq!(tree.next_seq, 2);
    }

    #[test]
    fn removals_drop_the_whole_subtree_and_moves_reorder() {
        let mut tree = seeded();
        let report = tree.apply(&batch(
            1,
            1,
            vec![Op::Move {
                id: "a".into(),
                parent: Some("b".into()),
                slot: "children".into(),
                index: 1,
            }],
        ));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(
            tree.get("b").unwrap().primary_children(),
            vec!["b1".to_string(), "a".to_string()]
        );
        let report = tree.apply(&batch(1, 2, vec![Op::Remove { id: "b".into() }]));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(tree.get("b").is_none());
        assert!(tree.get("b1").is_none());
        assert!(tree.get("a").is_none());
        assert!(tree.get("root").unwrap().primary_children().is_empty());
    }

    #[test]
    fn a_move_into_own_subtree_is_rejected_without_panicking() {
        let mut tree = seeded();
        let report = tree.apply(&batch(
            1,
            1,
            vec![Op::Move {
                id: "b".into(),
                parent: Some("b1".into()),
                slot: "children".into(),
                index: 0,
            }],
        ));
        assert!(report.needs_snapshot);
        assert!(!report.errors.is_empty());
        assert_eq!(tree.get("b").unwrap().parent.as_deref(), Some("root"));
    }

    #[test]
    fn sequence_gaps_request_a_snapshot_and_do_not_apply() {
        let mut tree = seeded();
        let before = tree.clone();
        let report = tree.apply(&batch(
            1,
            3,
            vec![Op::SetStatus {
                id: "a".into(),
                status: NodeStatus::Partial,
            }],
        ));
        assert!(report.needs_snapshot);
        assert!(!report.applied);
        assert_eq!(tree, before);
        let report = tree.apply(&batch(
            2,
            0,
            vec![
                Op::Reset,
                inserted("root", None, 0, "Column"),
                inserted("fresh", Some("root"), 0, "Text"),
            ],
        ));
        assert!(report.applied, "{:?}", report.errors);
        assert_eq!(tree.epoch, Some(2));
        assert_eq!(tree.root.as_deref(), Some("root"));
        assert!(tree.get("a").is_none());
    }

    #[test]
    fn older_epochs_are_discarded_and_bad_ids_are_reported_not_panics() {
        let mut tree = seeded();
        let before = tree.clone();
        let report = tree.apply(&batch(0, 0, vec![Op::Reset]));
        assert!(report.discarded);
        assert_eq!(tree, before);
        let report = tree.apply(&batch(1, 1, vec![Op::Remove { id: "ghost".into() }]));
        assert!(report.needs_snapshot);
        assert_eq!(report.errors.len(), 1);
        assert!(tree.get("a").is_some());
    }

    #[test]
    fn reset_in_the_same_epoch_clears_the_tree() {
        let mut tree = seeded();
        let report = tree.apply(&batch(1, 1, vec![Op::Reset]));
        assert!(report.applied);
        assert!(tree.is_empty());
        assert_eq!(tree.root, None);
    }

    #[test]
    fn insert_positions_are_clamped_and_duplicate_roots_are_refused() {
        let mut tree = seeded();
        let report = tree.apply(&batch(
            1,
            1,
            vec![
                inserted("late", Some("root"), 99, "Text"),
                inserted("second", None, 0, "Column"),
            ],
        ));
        assert!(report.needs_snapshot);
        assert!(report.errors.iter().any(|error| error.contains("root")));
        assert_eq!(
            tree.get("root").unwrap().primary_children(),
            vec!["a".to_string(), "b".to_string(), "late".to_string()]
        );
    }

    #[test]
    fn snapshot_batches_replace_the_retained_tree() {
        let session_batches = vec![
            batch(
                7,
                0,
                vec![
                    Op::Reset,
                    inserted("root", None, 0, "Column"),
                    inserted("x", Some("root"), 0, "Text"),
                ],
            ),
            batch(7, 1, vec![inserted("root2", None, 0, "Column")]),
        ];
        let (tree, reports) = RetainedTree::from_batches(&session_batches);
        assert_eq!(reports.len(), 2);
        assert!(reports[0].applied);
        assert!(reports[1].needs_snapshot);
        assert_eq!(tree.root.as_deref(), Some("root"));
        assert_eq!(tree.len(), 2);
    }
}
