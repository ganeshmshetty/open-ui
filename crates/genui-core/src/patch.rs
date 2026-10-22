//! Named, serializable retained-tree edits and checked replay (§6).
use crate::{
    ir::{Ir, NodeStatus, Prop, PropValue},
    r#lib::Library,
    value::{Provenance, Value},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value as J, json};
use std::collections::{BTreeMap, BTreeSet};

pub type Props = BTreeMap<String, J>;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchBatch {
    pub epoch: u32,
    pub seq: u64,
    pub ops: Vec<Op>,
}
impl PatchBatch {
    pub fn canonical_json(&self) -> Result<String, serde_json::Error> {
        crate::canonical::serialize(self)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Op {
    Reset,
    Insert {
        parent: Option<String>,
        slot: String,
        index: u32,
        node: NodeInit,
    },
    SetProps {
        id: String,
        set: Props,
        unset: Vec<String>,
    },
    AppendText {
        id: String,
        prop: String,
        text: String,
    },
    Remove {
        id: String,
    },
    Move {
        id: String,
        parent: Option<String>,
        slot: String,
        index: u32,
    },
    SetStatus {
        id: String,
        status: NodeStatus,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeInit {
    pub id: String,
    pub kind: String,
    pub role: crate::ir::Role,
    pub props: Props,
    pub status: NodeStatus,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub node: NodeInit,
    pub parent: Option<String>,
    pub slot: String,
    pub children: BTreeMap<String, Vec<String>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tree {
    pub root: Option<String>,
    pub nodes: BTreeMap<String, Entry>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ApplyReport {
    pub errors: Vec<String>,
    pub needs_snapshot: bool,
    pub discarded: bool,
    pub dirty: BTreeSet<String>,
}
#[derive(Clone, Debug, Default)]
pub struct Replay {
    pub tree: Tree,
    epoch: Option<u32>,
    next_seq: u64,
}
impl Replay {
    /// Sequence gaps request a snapshot without partially applying the out-of-order batch.
    pub fn apply(&mut self, batch: &PatchBatch) -> ApplyReport {
        let mut report = ApplyReport::default();
        if self.epoch.is_some_and(|e| batch.epoch < e) {
            report.discarded = true;
            return report;
        }
        let reset = matches!(batch.ops.first(), Some(Op::Reset));
        let new_epoch = self.epoch != Some(batch.epoch);
        if (new_epoch && !reset)
            || (!new_epoch && batch.seq != self.next_seq && !(reset && batch.seq >= self.next_seq))
        {
            report
                .errors
                .push("epoch/sequence gap: snapshot required".into());
            report.needs_snapshot = true;
            return report;
        }
        if !reset && self.epoch.is_none() {
            report.needs_snapshot = true;
            return report;
        }
        for op in &batch.ops {
            match self.tree.apply(op) {
                Ok(dirty) => report.dirty.extend(dirty),
                Err(error) => {
                    report.errors.push(error);
                    report.needs_snapshot = true;
                }
            }
        }
        self.epoch = Some(batch.epoch);
        if let Some(next) = batch.seq.checked_add(1) {
            self.next_seq = next;
        } else {
            report.needs_snapshot = true;
        }
        report
    }
}
impl Tree {
    pub fn from_ir(ir: &Ir, library: &Library) -> Self {
        let mut tree = Self::default();
        let Some(root) = ir.root else {
            return tree;
        };
        let mut queue = vec![(root, None, "root".to_string())];
        while let Some((id, parent, slot)) = queue.pop() {
            let Some(n) = ir.nodes.get(&id) else {
                continue;
            };
            let Some(c) = library.components().get(n.kind.0 as usize) else {
                continue;
            };
            let mut props = Props::new();
            for (id, p) in &n.props {
                let Some(arg) = c.args.get(id.0 as usize) else {
                    continue;
                };
                let value = match p {
                    Prop::Dynamic(e) => {
                        let Some(e) = ir.expressions.get(e.0) else {
                            continue;
                        };
                        json!({"deferred":e})
                    }
                    Prop::Static(PropValue::Value(v)) => wire_value(v),
                    Prop::Static(PropValue::Token { scale, value }) => {
                        json!({"tok":[scale,value.plain_json()]})
                    }
                    Prop::Static(PropValue::Binding(name)) => json!({"bind":format!("${name}")}),
                    Prop::Static(PropValue::Action(id)) => json!({"action":id.0}),
                };
                props.insert(arg.name.clone(), value);
            }
            if parent.is_none() {
                tree.root = Some(n.path.0.clone());
            }
            let mut children = BTreeMap::new();
            for s in n.slots.iter().rev() {
                let ids: Vec<_> = s
                    .children
                    .iter()
                    .filter_map(|id| ir.nodes.get(id).map(|n| n.path.0.clone()))
                    .collect();
                if !ids.is_empty() {
                    children.insert(s.name.clone(), ids);
                }
                for child in s.children.iter().rev() {
                    queue.push((*child, Some(n.path.0.clone()), s.name.clone()));
                }
            }
            tree.nodes.insert(
                n.path.0.clone(),
                Entry {
                    node: NodeInit {
                        id: n.path.0.clone(),
                        kind: c.name.clone(),
                        role: n.role,
                        props,
                        status: n.status,
                    },
                    parent,
                    slot,
                    children,
                },
            );
        }
        tree
    }
    pub fn canonical_tree(&self) -> J {
        fn visit(tree: &Tree, id: &str) -> J {
            let Some(e) = tree.nodes.get(id) else {
                return J::Null;
            };
            let children: BTreeMap<_, _> = e
                .children
                .iter()
                .map(|(k, ids)| (k, ids.iter().map(|id| visit(tree, id)).collect::<Vec<_>>()))
                .collect();
            json!({"id":e.node.id,"kind":e.node.kind,"role":e.node.role,"props":e.node.props,"status":e.node.status,"children":children})
        }
        self.root.as_deref().map_or(J::Null, |id| visit(self, id))
    }
    fn attach(
        &mut self,
        id: &str,
        parent: &Option<String>,
        slot: &str,
        index: usize,
    ) -> Result<(), String> {
        if let Some(parent) = parent {
            let p = self.nodes.get_mut(parent).ok_or("unknown parent")?;
            if index > p.children.get(slot).map_or(0, Vec::len) {
                return Err("invalid sibling index".into());
            }
            p.children
                .entry(slot.into())
                .or_default()
                .insert(index, id.into());
        } else {
            if index != 0 || self.root.as_deref().is_some_and(|root| root != id) {
                return Err("invalid root attachment".into());
            }
            self.root = Some(id.into());
        }
        Ok(())
    }
    fn detach(&mut self, id: &str) {
        let Some(e) = self.nodes.get(id) else {
            return;
        };
        let parent = e.parent.clone();
        let slot = e.slot.clone();
        if let Some(parent) = parent {
            if let Some(p) = self.nodes.get_mut(&parent)
                && let Some(children) = p.children.get_mut(&slot)
            {
                children.retain(|c| c != id);
                if children.is_empty() {
                    p.children.remove(&slot);
                }
            }
        } else if self.root.as_deref() == Some(id) {
            self.root = None;
        }
    }
    pub fn apply(&mut self, op: &Op) -> Result<BTreeSet<String>, String> {
        let mut dirty = BTreeSet::new();
        match op {
            Op::Reset => {
                self.nodes.clear();
                self.root = None;
            }
            Op::Insert {
                parent,
                slot,
                index,
                node,
            } => {
                if self.nodes.contains_key(&node.id) {
                    return Err("duplicate node id".into());
                }
                self.attach(&node.id, parent, slot, *index as usize)?;
                self.nodes.insert(
                    node.id.clone(),
                    Entry {
                        node: node.clone(),
                        parent: parent.clone(),
                        slot: slot.clone(),
                        children: BTreeMap::new(),
                    },
                );
                dirty.insert(node.id.clone());
                dirty.extend(parent.iter().cloned());
            }
            Op::Remove { id } => {
                let e = self.nodes.get(id).ok_or("unknown remove id")?;
                dirty.extend(e.parent.iter().cloned());
                self.detach(id);
                let mut queue = vec![id.clone()];
                while let Some(id) = queue.pop() {
                    if let Some(e) = self.nodes.remove(&id) {
                        queue.extend(e.children.into_values().flatten());
                    }
                }
            }
            Op::Move {
                id,
                parent,
                slot,
                index,
            } => {
                let e = self.nodes.get(id).ok_or("unknown move id")?;
                let old_parent = e.parent.clone();
                let old_slot = e.slot.clone();
                let mut ancestor = parent.as_deref();
                let mut seen = BTreeSet::new();
                while let Some(p) = ancestor {
                    if p == id || !seen.insert(p) {
                        return Err("move would form a cycle".into());
                    }
                    ancestor = self
                        .nodes
                        .get(p)
                        .ok_or("unknown move parent")?
                        .parent
                        .as_deref();
                }
                let length = if let Some(p) = parent {
                    self.nodes
                        .get(p)
                        .and_then(|e| e.children.get(slot))
                        .map_or(0, Vec::len)
                        .saturating_sub(usize::from(old_parent == *parent && old_slot == *slot))
                } else {
                    0
                };
                if *index as usize > length
                    || (parent.is_none() && self.root.as_deref().is_some_and(|r| r != id))
                {
                    return Err("invalid move index/root".into());
                }
                self.detach(id);
                self.attach(id, parent, slot, *index as usize)?;
                if let Some(e) = self.nodes.get_mut(id) {
                    e.parent = parent.clone();
                    e.slot = slot.clone();
                }
                dirty.extend(old_parent);
                dirty.extend(parent.iter().cloned());
                dirty.insert(id.clone());
            }
            Op::SetProps { id, set, unset } => {
                let e = self.nodes.get_mut(id).ok_or("unknown set_props id")?;
                for (name, value) in set {
                    e.node.props.insert(name.clone(), value.clone());
                }
                for name in unset {
                    e.node.props.remove(name);
                }
                dirty.insert(id.clone());
            }
            Op::AppendText { id, prop, text } => {
                let e = self.nodes.get_mut(id).ok_or("unknown append_text id")?;
                let value = e.node.props.get_mut(prop).ok_or("unknown text prop")?;
                let J::String(value) = value else {
                    return Err("append_text requires a string".into());
                };
                value.push_str(text);
                dirty.insert(id.clone());
            }
            Op::SetStatus { id, status } => {
                self.nodes
                    .get_mut(id)
                    .ok_or("unknown set_status id")?
                    .node
                    .status = *status;
                dirty.insert(id.clone());
            }
        }
        Ok(dirty)
    }
    fn preorder(&self) -> Vec<(String, Option<String>, String, u32)> {
        let Some(root) = &self.root else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut queue = vec![(root.clone(), None, "root".into(), 0)];
        while let Some((id, parent, slot, index)) = queue.pop() {
            let Some(e) = self.nodes.get(&id) else {
                continue;
            };
            for (slot, ids) in e.children.iter().rev() {
                for (i, id) in ids.iter().enumerate().rev() {
                    queue.push((id.clone(), Some(e.node.id.clone()), slot.clone(), i as u32));
                }
            }
            out.push((id, parent, slot, index));
        }
        out
    }
    /// Diffing only the final chunk result inherently coalesces transient insert/remove
    /// and repeated prop writes. Never touch an equal retained node.
    pub fn diff(&self, next: &Tree) -> Vec<Op> {
        let mut current = self.clone();
        let mut ops = Vec::new();
        let gone: Vec<_> = self
            .preorder()
            .into_iter()
            .filter(|(id, _, _, _)| {
                next.nodes.get(id).is_none_or(|n| {
                    self.nodes
                        .get(id)
                        .is_some_and(|e| e.node.kind != n.node.kind)
                })
            })
            .map(|v| v.0)
            .collect();
        for id in gone {
            if current.nodes.contains_key(&id) {
                let op = Op::Remove { id };
                if current.apply(&op).is_ok() {
                    ops.push(op);
                }
            }
        }
        for (id, parent, slot, index) in next.preorder() {
            let Some(target) = next.nodes.get(&id) else {
                continue;
            };
            if !current.nodes.contains_key(&id) {
                let op = Op::Insert {
                    parent,
                    slot,
                    index,
                    node: target.node.clone(),
                };
                if current.apply(&op).is_ok() {
                    ops.push(op);
                }
                continue;
            }
            let e = &current.nodes[&id];
            let position = e
                .parent
                .as_ref()
                .and_then(|p| current.nodes.get(p))
                .and_then(|p| p.children.get(&e.slot))
                .and_then(|v| v.iter().position(|p| p == &id))
                .unwrap_or(0);
            if e.parent != parent || e.slot != slot || position != index as usize {
                let op = Op::Move {
                    id: id.clone(),
                    parent,
                    slot,
                    index,
                };
                if current.apply(&op).is_ok() {
                    ops.push(op);
                }
            }
            let Some(old) = current.nodes.get(&id) else {
                continue;
            };
            let mut set = Props::new();
            let mut appends = Vec::new();
            for (key, value) in &target.node.props {
                if old.node.props.get(key) == Some(value) {
                    continue;
                }
                match (old.node.props.get(key), value) {
                    (Some(J::String(old)), J::String(new)) if new.starts_with(old) => {
                        appends.push(Op::AppendText {
                            id: id.clone(),
                            prop: key.clone(),
                            text: new[old.len()..].into(),
                        })
                    }
                    _ => {
                        set.insert(key.clone(), value.clone());
                    }
                }
            }
            let unset: Vec<_> = old
                .node
                .props
                .keys()
                .filter(|k| !target.node.props.contains_key(*k))
                .cloned()
                .collect();
            if !set.is_empty() || !unset.is_empty() {
                ops.push(Op::SetProps {
                    id: id.clone(),
                    set,
                    unset,
                });
            }
            ops.extend(appends);
            if old.node.status != target.node.status {
                ops.push(Op::SetStatus {
                    id,
                    status: target.node.status,
                });
            }
        }
        ops
    }
    pub fn snapshot_ops(&self) -> Vec<Op> {
        let mut ops = vec![Op::Reset];
        for (id, parent, slot, index) in self.preorder() {
            if let Some(e) = self.nodes.get(&id) {
                ops.push(Op::Insert {
                    parent,
                    slot,
                    index,
                    node: e.node.clone(),
                });
            }
        }
        ops
    }
}
pub fn strip_invisibles(text: &str) -> String {
    text.chars().filter(|c|!matches!(*c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'|'\u{200b}'..='\u{200d}'|'\u{2060}')).collect()
}
pub fn wire_value(v: &Value) -> J {
    match v {
        Value::Str(s) if s.prov == Provenance::Model => J::String(strip_invisibles(&s.text)),
        Value::List(items) => J::Array(items.iter().map(wire_value).collect()),
        Value::Map(items) => {
            json!({"map":items.iter().map(|(k,v)|json!([k,wire_value(v)])).collect::<Vec<_>>()})
        }
        _ => v.ir_json(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(id: &str, kind: &str) -> NodeInit {
        NodeInit {
            id: id.into(),
            kind: kind.into(),
            role: crate::ir::Role::Group,
            props: BTreeMap::new(),
            status: NodeStatus::Sealed,
        }
    }
    fn insert(id: &str, parent: Option<&str>, index: u32) -> Op {
        Op::Insert {
            parent: parent.map(str::to_string),
            slot: if parent.is_some() { "children" } else { "root" }.into(),
            index,
            node: node(id, "Column"),
        }
    }
    fn batch(seq: u64, ops: Vec<Op>) -> PatchBatch {
        PatchBatch { epoch: 1, seq, ops }
    }
    #[test]
    fn replay_orders_children_removes_subtrees_and_marks_only_touched_nodes() {
        let mut r = Replay::default();
        let first = batch(
            0,
            vec![
                Op::Reset,
                insert("root", None, 0),
                insert("a", Some("root"), 0),
                insert("leaf", Some("a"), 0),
                insert("b", Some("root"), 1),
            ],
        );
        let report = r.apply(&first);
        assert!(!report.needs_snapshot);
        assert_eq!(r.tree.nodes.len(), 4);
        let report = r.apply(&batch(
            1,
            vec![Op::Move {
                id: "b".into(),
                parent: Some("root".into()),
                slot: "children".into(),
                index: 0,
            }],
        ));
        assert_eq!(report.dirty, ["root".into(), "b".into()].into());
        assert_eq!(r.tree.nodes["root"].children["children"], ["b", "a"]);
        let report = r.apply(&batch(2, vec![Op::Remove { id: "a".into() }]));
        assert!(!report.needs_snapshot);
        assert_eq!(r.tree.nodes.len(), 2);
        assert!(!r.tree.nodes.contains_key("leaf"));
    }
    #[test]
    fn unknown_ids_bad_indices_and_ancestor_moves_request_repair_without_panicking() {
        let mut r = Replay::default();
        r.apply(&batch(
            0,
            vec![
                Op::Reset,
                insert("root", None, 0),
                insert("a", Some("root"), 0),
            ],
        ));
        let before = r.tree.clone();
        let ops = vec![
            insert("bad", Some("missing"), 0),
            insert("bad", Some("root"), 9),
            Op::Move {
                id: "root".into(),
                parent: Some("a".into()),
                slot: "children".into(),
                index: 0,
            },
            Op::Remove {
                id: "missing".into(),
            },
            Op::SetProps {
                id: "missing".into(),
                set: BTreeMap::new(),
                unset: vec![],
            },
            Op::AppendText {
                id: "a".into(),
                prop: "missing".into(),
                text: "x".into(),
            },
            Op::SetStatus {
                id: "missing".into(),
                status: NodeStatus::Partial,
            },
        ];
        let report = r.apply(&batch(1, ops));
        assert_eq!(report.errors.len(), 7);
        assert!(report.needs_snapshot);
        assert_eq!(r.tree, before);
    }
    #[test]
    fn epochs_gaps_and_repair_snapshots_are_explicit() {
        let mut r = Replay::default();
        r.apply(&batch(0, vec![Op::Reset, insert("root", None, 0)]));
        let before = r.tree.clone();
        assert!(
            r.apply(&batch(2, vec![Op::Remove { id: "root".into() }]))
                .needs_snapshot
        );
        assert_eq!(r.tree, before);
        assert!(!r.apply(&batch(3, before.snapshot_ops())).needs_snapshot);
        assert!(!r.apply(&batch(4, vec![])).needs_snapshot);
        let fresh = PatchBatch {
            epoch: 2,
            seq: 0,
            ops: vec![Op::Reset],
        };
        assert!(!r.apply(&fresh).needs_snapshot);
        assert!(r.tree.nodes.is_empty());
        assert!(r.apply(&batch(5, before.snapshot_ops())).discarded);
        assert!(r.tree.nodes.is_empty());
        assert!(
            r.apply(&PatchBatch {
                epoch: 3,
                seq: 0,
                ops: vec![insert("x", None, 0)]
            })
            .needs_snapshot
        );
    }
    #[test]
    fn diff_coalesces_props_appends_reorders_and_kind_replacements() {
        let mut old = Tree::default();
        for op in [
            insert("root", None, 0),
            insert("a", Some("root"), 0),
            insert("b", Some("root"), 1),
        ] {
            old.apply(&op).unwrap();
        }
        old.nodes.get_mut("a").unwrap().node.props = [
            ("text".into(), json!("hello")),
            ("gone".into(), json!(true)),
        ]
        .into();
        let mut new = old.clone();
        new.apply(&Op::Move {
            id: "b".into(),
            parent: Some("root".into()),
            slot: "children".into(),
            index: 0,
        })
        .unwrap();
        new.nodes.get_mut("a").unwrap().node.props = [
            ("text".into(), json!("hello world")),
            ("size".into(), json!({"i":2})),
        ]
        .into();
        new.nodes.get_mut("a").unwrap().node.status = NodeStatus::Partial;
        let ops = old.diff(&new);
        assert!(ops.iter().any(|op| matches!(op,Op::Move{id,..}if id=="b")));
        assert!(ops.iter().any(
            |op| matches!(op,Op::AppendText{id,prop,text}if id=="a"&&prop=="text"&&text==" world")
        ));
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(op,Op::SetProps{id,..}if id=="a"))
                .count(),
            1
        );
        for op in ops {
            old.apply(&op).unwrap();
        }
        assert_eq!(old, new);
        assert!(old.diff(&new).is_empty());
        new.nodes.get_mut("a").unwrap().node.kind = "Row".into();
        let ops = old.diff(&new);
        assert!(matches!(&ops[0],Op::Remove{id}if id=="a"));
        assert!(
            ops.iter()
                .any(|op| matches!(op,Op::Insert{node,..}if node.kind=="Row"))
        );
        for op in ops {
            old.apply(&op).unwrap();
        }
        assert_eq!(old, new);
        let empty = Tree::default();
        assert!(empty.diff(&empty).is_empty());
    }
    #[test]
    fn wire_maps_preserve_entry_order_and_reserved_keys_without_backend_dependence() {
        let map = Value::Map(Arc::new(vec![
            ("z".into(), Value::Int(2)),
            ("i".into(), Value::from("not an integer tag")),
        ]));
        assert_eq!(
            crate::canonical::encode(&wire_value(&map)),
            r#"{"map":[["z",{"i":2}],["i","not an integer tag"]]}"#
        );
        let model = Value::from("a\u{202e}b\u{200b}c");
        assert_eq!(wire_value(&model), json!("abc"));
        let tool = Value::Str(crate::value::PStr {
            text: "x\u{200b}".into(),
            prov: Provenance::Tool,
        });
        assert_eq!(wire_value(&tool), json!("x\u{200b}"));
    }
    use std::sync::Arc;
}
