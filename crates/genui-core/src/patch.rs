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
    pub children: BTreeMap<String, crate::SharedVector<String>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tree {
    pub root: Option<String>,
    pub nodes: crate::SharedMap<String, Entry>,
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
    fn props_from_ir(
        ir: &Ir,
        library: &Library,
        id: crate::ir::NodeId,
        resolved: Option<&crate::runtime::Resolved>,
        work: &mut crate::MaintenanceWork,
    ) -> Option<NodeInit> {
        let n = ir.nodes.get(&id)?;
        let c = library.components().get(n.kind.0 as usize)?;
        let mut props = Props::new();
        for (prop_id, p) in &n.props {
            work.props_compared += 1;
            let Some(arg) = c.args.get(prop_id.0 as usize) else {
                continue;
            };
            let value = match p {
                Prop::Dynamic(e) => {
                    if let Some(PropValue::Value(value)) =
                        resolved.and_then(|resolved| resolved.prop(id, *prop_id))
                    {
                        wire_value(value)
                    } else {
                        let Some(e) = ir.expressions.get(e) else {
                            continue;
                        };
                        json!({"deferred":e})
                    }
                }
                Prop::Static(PropValue::Value(v)) => wire_value(v),
                Prop::Static(PropValue::Token { scale, value }) => {
                    json!({"tok":[scale,value.plain_json()]})
                }
                Prop::Static(PropValue::Binding(name)) => json!({"bind":format!("${name}")}),
                Prop::Static(PropValue::Action(id)) => json!({"action":id.0.as_ref()}),
            };
            props.insert(arg.name.clone(), value);
        }
        if let Some(resolved) = resolved {
            for (prop, name) in [
                (crate::runtime::PROP_ERROR, "error"),
                (crate::runtime::PROP_TOUCHED, "touched"),
                (crate::runtime::PROP_STATUS, "status"),
            ] {
                if let Some(PropValue::Value(value)) = resolved.prop(id, prop) {
                    props.insert(name.to_string(), wire_value(value));
                }
            }
        }
        Some(NodeInit {
            id: n.path.0.clone(),
            kind: c.name.clone(),
            role: n.role,
            props,
            status: n.status,
        })
    }
    pub(crate) fn insert_grafts(
        &mut self,
        ir: &Ir,
        library: &Library,
        resolved: Option<&crate::runtime::Resolved>,
        grafts: &[crate::validate::Graft],
    ) -> (Vec<Op>, crate::MaintenanceWork) {
        let mut work = crate::MaintenanceWork::default();
        let mut ops = Vec::new();
        for graft in grafts {
            if let Some(id) = &graft.remove {
                work.wire_nodes_examined += 1;
                let op = Op::Remove { id: id.clone() };
                if self.apply_with_position(&op, Some(graft.index)).is_ok() {
                    ops.push(op);
                }
            }
            let mut queue = Vec::new();
            for (i, id) in graft.children.iter().enumerate().rev() {
                queue.push((
                    *id,
                    Some(graft.parent.clone()),
                    graft.slot.clone(),
                    graft.index + i,
                ));
            }
            while let Some((id, parent, slot, index)) = queue.pop() {
                work.wire_nodes_examined += 1;
                let Some(node) = Self::props_from_ir(ir, library, id, resolved, &mut work) else {
                    continue;
                };
                let path = node.id.clone();
                if self
                    .nodes
                    .get(&path)
                    .is_some_and(|old| old.node.kind == node.kind)
                {
                    let (changes, charged) = self.update_props(ir, library, resolved, &[id]);
                    work += charged;
                    ops.extend(changes);
                } else {
                    if self.nodes.contains_key(&path) {
                        let op = Op::Remove { id: path.clone() };
                        if self.apply(&op).is_ok() {
                            ops.push(op);
                        }
                    }
                    let op = Op::Insert {
                        parent,
                        slot,
                        index: index as u32,
                        node,
                    };
                    if self.apply(&op).is_ok() {
                        ops.push(op);
                    }
                }
                if let Some(node) = ir.nodes.get(&id) {
                    let mut slots = node.slots.iter().collect::<Vec<_>>();
                    slots.sort_by(|a, b| a.name.cmp(&b.name));
                    for slot in slots.into_iter().rev() {
                        for (i, id) in slot.children.iter().enumerate().rev() {
                            queue.push((*id, Some(path.clone()), slot.name.clone(), i));
                        }
                    }
                }
            }
        }
        (ops, work)
    }
    pub(crate) fn maintain(
        &mut self,
        ir: &Ir,
        library: &Library,
        resolved: Option<&crate::runtime::Resolved>,
        projection: &crate::validate::Projection,
    ) -> (Vec<Op>, crate::MaintenanceWork) {
        use crate::ir::NodeId;
        let mut work = crate::MaintenanceWork::default();
        let mut ops = Vec::new();
        let mut gone = BTreeMap::<String, NodeId>::new();
        for id in &projection.ids {
            if let Some(address) = projection.before.get(id) {
                let path = address.path.clone();
                if let Some(entry) = self.nodes.get(&path)
                    && ir.nodes.get(id).is_none_or(|node| {
                        library
                            .components()
                            .get(node.kind.0 as usize)
                            .is_none_or(|component| component.name != entry.node.kind)
                    })
                {
                    gone.insert(path, *id);
                }
            }
        }
        let has_ancestor = |path: &str, removed: &BTreeMap<String, NodeId>| {
            let mut at = path;
            while let Some((parent, _)) = at.rsplit_once('/') {
                if removed.contains_key(parent) {
                    return true;
                }
                at = parent;
            }
            false
        };
        let mut removals: Vec<_> = gone
            .iter()
            .filter(|(path, _)| !has_ancestor(path, &gone))
            .map(|(path, id)| (path.clone(), *id))
            .collect();
        removals.sort_by_key(|(_, id)| {
            projection
                .before
                .get(id)
                .map(|address| address.order.clone())
        });
        struct Edits {
            removed: crate::validate::PrefixCounts,
            prefix: usize,
        }
        let mut edits = BTreeMap::<(Option<String>, String), Edits>::new();
        for (path, id) in removals {
            let Some(address) = projection.before.get(&id) else {
                continue;
            };
            let key = (address.parent.clone(), address.slot.clone());
            let length = address
                .parent
                .as_ref()
                .and_then(|parent| self.nodes.get(parent))
                .and_then(|parent| parent.children.get(&address.slot))
                .map_or(0, |children| children.len());
            let edit = edits.entry(key).or_insert_with(|| Edits {
                removed: crate::validate::PrefixCounts::empty(length),
                prefix: 0,
            });
            let index = address
                .index
                .saturating_sub(edit.removed.prefix(address.index));
            let op = Op::Remove { id: path };
            if self.apply_with_position(&op, Some(index)).is_ok() {
                ops.push(op);
                edit.removed.set(address.index, 1);
            }
            work.wire_nodes_examined += 1;
        }
        let mut ordered: Vec<_> = projection
            .ids
            .iter()
            .filter(|id| ir.nodes.contains_key(id))
            .copied()
            .collect();
        ordered.sort_by_key(|id| {
            projection
                .after
                .get(id)
                .map(|address| address.order.clone())
        });
        for id in ordered {
            let Some(address) = projection.after.get(&id) else {
                continue;
            };
            let Some(node) = ir.nodes.get(&id) else {
                continue;
            };
            let path = node.path.0.clone();
            let key = (address.parent.clone(), address.slot.clone());
            let length = address
                .parent
                .as_ref()
                .and_then(|parent| self.nodes.get(parent))
                .and_then(|parent| parent.children.get(&address.slot))
                .map_or(0, |children| children.len());
            let edit = edits.entry(key).or_insert_with(|| Edits {
                removed: crate::validate::PrefixCounts::empty(length),
                prefix: 0,
            });
            if !self.nodes.contains_key(&path) {
                work.wire_nodes_examined += 1;
                let Some(node) = Self::props_from_ir(ir, library, id, resolved, &mut work) else {
                    continue;
                };
                let op = Op::Insert {
                    parent: address.parent.clone(),
                    slot: address.slot.clone(),
                    index: address.index as u32,
                    node,
                };
                if self.apply(&op).is_ok() {
                    ops.push(op);
                    edit.prefix += 1;
                }
            } else {
                if let Some(old) = projection.before.get(&id) {
                    // Process the new prefix in canonical order. Original ranks
                    // lose prior removals/moves and gain the inserted prefix.
                    let position =
                        old.index.saturating_sub(edit.removed.prefix(old.index)) + edit.prefix;
                    if position != address.index
                        || old.parent != address.parent
                        || old.slot != address.slot
                    {
                        let op = Op::Move {
                            id: path.clone(),
                            parent: address.parent.clone(),
                            slot: address.slot.clone(),
                            index: address.index as u32,
                        };
                        if self.apply_with_position(&op, Some(position)).is_ok() {
                            ops.push(op);
                            edit.removed.set(old.index, 1);
                            edit.prefix += 1;
                        }
                    }
                }
                let (changes, charged) = self.update_props(ir, library, resolved, &[id]);
                work += charged;
                ops.extend(changes);
            }
        }
        (ops, work)
    }
    pub(crate) fn update_props(
        &mut self,
        ir: &Ir,
        library: &Library,
        resolved: Option<&crate::runtime::Resolved>,
        ids: &[crate::ir::NodeId],
    ) -> (Vec<Op>, crate::MaintenanceWork) {
        let mut work = crate::MaintenanceWork::default();
        let mut ops = Vec::new();
        for id in ids {
            work.wire_nodes_examined += 1;
            let Some(target) = Self::props_from_ir(ir, library, *id, resolved, &mut work) else {
                continue;
            };
            let Some(entry) = self.nodes.get_mut(&target.id) else {
                continue;
            };
            let old = &entry.node;
            let mut set = Props::new();
            let mut appends = Vec::new();
            for (key, value) in &target.props {
                work.props_compared += 1;
                if old.props.get(key) == Some(value) {
                    continue;
                }
                match (old.props.get(key), value) {
                    (Some(J::String(old)), J::String(new)) if new.starts_with(old) => {
                        appends.push(Op::AppendText {
                            id: target.id.clone(),
                            prop: key.clone(),
                            text: new[old.len()..].into(),
                        })
                    }
                    _ => {
                        set.insert(key.clone(), value.clone());
                    }
                }
            }
            work.props_compared += old.props.len() as u64;
            let unset = old
                .props
                .keys()
                .filter(|key| !target.props.contains_key(*key))
                .cloned()
                .collect::<Vec<_>>();
            if !set.is_empty() || !unset.is_empty() {
                ops.push(Op::SetProps {
                    id: target.id.clone(),
                    set,
                    unset,
                });
            }
            ops.extend(appends);
            if old.status != target.status {
                ops.push(Op::SetStatus {
                    id: target.id.clone(),
                    status: target.status,
                });
            }
            entry.node = target;
        }
        (ops, work)
    }
    pub fn from_ir(ir: &Ir, library: &Library) -> Self {
        Self::from_ir_measured(ir, library, None).0
    }
    pub fn from_ir_resolved(
        ir: &Ir,
        library: &Library,
        resolved: &crate::runtime::Resolved,
    ) -> Self {
        Self::from_ir_measured(resolved.ir_or(ir), library, Some(resolved)).0
    }
    pub(crate) fn from_ir_measured(
        ir: &Ir,
        library: &Library,
        resolved: Option<&crate::runtime::Resolved>,
    ) -> (Self, crate::MaintenanceWork) {
        let mut work = crate::MaintenanceWork::default();
        let mut tree = Self::default();
        let Some(root) = ir.root else {
            return (tree, work);
        };
        let mut queue = vec![(root, None, "root".to_string())];
        while let Some((id, parent, slot)) = queue.pop() {
            work.wire_nodes_examined += 1;
            let Some(n) = ir.nodes.get(&id) else {
                continue;
            };
            let Some(init) = Self::props_from_ir(ir, library, id, resolved, &mut work) else {
                continue;
            };
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
                    children.insert(s.name.clone(), ids.into());
                }
                for child in s.children.iter().rev() {
                    queue.push((*child, Some(n.path.0.clone()), s.name.clone()));
                }
            }
            tree.nodes.insert(
                n.path.0.clone(),
                Entry {
                    node: init,
                    parent,
                    slot,
                    children,
                },
            );
        }
        (tree, work)
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
            if index > p.children.get(slot).map_or(0, |v| v.len()) {
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
    fn detach(&mut self, id: &str, position: Option<usize>) -> Result<(), String> {
        let Some(e) = self.nodes.get(id) else {
            return Ok(());
        };
        let parent = e.parent.clone();
        let slot = e.slot.clone();
        if let Some(parent) = parent {
            if let Some(p) = self.nodes.get_mut(&parent)
                && let Some(children) = p.children.get_mut(&slot)
            {
                let index = if let Some(index) = position {
                    if children.get(index).is_none_or(|child| child != id) {
                        return Err("invalid retained remove position".into());
                    }
                    Some(index)
                } else {
                    children.iter().position(|c| c == id)
                };
                if let Some(index) = index {
                    children.remove(index);
                }
                if children.is_empty() {
                    p.children.remove(&slot);
                }
            }
        } else if self.root.as_deref() == Some(id) {
            self.root = None;
        }
        Ok(())
    }
    pub fn apply(&mut self, op: &Op) -> Result<BTreeSet<String>, String> {
        self.apply_with_position(op, None)
    }
    fn apply_with_position(
        &mut self,
        op: &Op,
        position: Option<usize>,
    ) -> Result<BTreeSet<String>, String> {
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
                self.detach(id, position)?;
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
                        .map_or(0, |v| v.len())
                        .saturating_sub(usize::from(old_parent == *parent && old_slot == *slot))
                } else {
                    0
                };
                if *index as usize > length
                    || (parent.is_none() && self.root.as_deref().is_some_and(|r| r != id))
                {
                    return Err("invalid move index/root".into());
                }
                self.detach(id, position)?;
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
    pub(crate) fn preorder(&self) -> Vec<(String, Option<String>, String, u32)> {
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
        self.diff_measured(next).0
    }
    pub(crate) fn diff_measured(&self, next: &Tree) -> (Vec<Op>, crate::MaintenanceWork) {
        // Clone plus the two preorders and target pass all visit retained nodes.
        let mut work = crate::MaintenanceWork {
            wire_nodes_examined: 2 * self.nodes.len() as u64 + 2 * next.nodes.len() as u64,
            ..Default::default()
        };
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
                .and_then(|v| {
                    v.iter().position(|p| {
                        work.wire_nodes_examined += 1;
                        p == &id
                    })
                })
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
                work.props_compared += 1;
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
            work.props_compared += old.node.props.len() as u64;
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
        (ops, work)
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
        assert_eq!(
            r.tree.nodes["root"].children["children"]
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["b", "a"]
        );
        let report = r.apply(&batch(2, vec![Op::Remove { id: "a".into() }]));
        assert!(!report.needs_snapshot);
        assert_eq!(r.tree.nodes.len(), 2);
        assert!(!r.tree.nodes.contains_key("leaf"));
    }
    #[test]
    fn a_retained_remove_hint_is_checked_without_searching_for_another_position() {
        let mut tree = Tree::default();
        for op in [
            insert("root", None, 0),
            insert("a", Some("root"), 0),
            insert("b", Some("root"), 1),
        ] {
            tree.apply(&op).unwrap();
        }
        let before = tree.clone();
        let remove = Op::Remove { id: "b".into() };
        assert!(tree.apply_with_position(&remove, Some(0)).is_err());
        assert_eq!(tree, before);
        let mut replay = before;
        replay.apply(&remove).unwrap();
        tree.apply_with_position(&remove, Some(1)).unwrap();
        assert_eq!(tree, replay);
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
    fn diff_measured_charges_base_preorders_and_position_scans() {
        let (old, _lib, _ir) = tree_with(&["a", "b"]);
        let mut next = Tree::default();
        next.apply(&insert("root2", None, 0)).unwrap();
        for (index, id) in ["x", "y", "z"].iter().enumerate() {
            next.apply(&insert(id, Some("root2"), index as u32))
                .unwrap();
        }
        let (ops, work) = old.diff_measured(&next);
        assert!(!ops.is_empty());
        assert_eq!(
            work.wire_nodes_examined, 14,
            "two preorders over three retained and four next nodes"
        );

        let (old, _lib, _ir) = tree_with(&["a", "b"]);
        let (_, work) = old.diff_measured(&old);
        assert_eq!(
            work.wire_nodes_examined, 15,
            "the shared children scan each retained sibling once"
        );
    }
    #[test]
    fn diff_measured_charges_prop_comparisons_and_retained_props() {
        let (mut old, _lib, _ir) = tree_with(&["a"]);
        old.nodes.get_mut("a").unwrap().node.props = [
            ("text".into(), json!("hello")),
            ("gone".into(), json!(true)),
        ]
        .into();
        let mut next = old.clone();
        next.nodes.get_mut("a").unwrap().node.props = [("text".into(), json!("world"))].into();
        let (ops, work) = old.diff_measured(&next);
        assert_eq!(
            work.props_compared, 3,
            "one target prop plus the two retained props"
        );
        assert_eq!(
            ops,
            vec![Op::SetProps {
                id: "a".into(),
                set: [("text".into(), json!("world"))].into(),
                unset: vec!["gone".into()],
            }],
            "a non-prefix edit replaces the text and unsets the removed prop"
        );
        for op in ops {
            old.apply(&op).unwrap();
        }
        assert_eq!(old, next);
    }
    #[test]
    fn diff_emits_set_props_when_only_unsets_remain() {
        let (mut old, _lib, _ir) = tree_with(&["a"]);
        old.nodes.get_mut("a").unwrap().node.props = [("gone".into(), json!(true))].into();
        let mut next = old.clone();
        next.nodes.get_mut("a").unwrap().node.props.clear();
        let ops = old.diff(&next);
        assert_eq!(
            ops,
            vec![Op::SetProps {
                id: "a".into(),
                set: BTreeMap::new(),
                unset: vec!["gone".into()],
            }]
        );
        for op in ops {
            old.apply(&op).unwrap();
        }
        assert_eq!(old, next);
    }
    #[test]
    fn diff_moves_a_retained_node_that_only_changes_its_slot() {
        let (mut old, _lib, _ir) = tree_with(&["a"]);
        old.apply(&Op::Insert {
            parent: Some("root".into()),
            slot: "other".into(),
            index: 0,
            node: node("x", "Column"),
        })
        .unwrap();
        let mut next = old.clone();
        next.apply(&Op::Move {
            id: "a".into(),
            parent: Some("root".into()),
            slot: "other".into(),
            index: 0,
        })
        .unwrap();
        let ops = old.diff(&next);
        assert!(
            matches!(&ops[..], [Op::Move { id, slot, index: 0, .. }] if id == "a" && slot == "other"),
            "unexpected ops: {ops:?}"
        );
        for op in ops {
            old.apply(&op).unwrap();
        }
        assert_eq!(old, next);
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
    fn wire_library() -> Library {
        use crate::r#lib::{ComponentDef, GroupDef, Role as LibRole, T};
        Library::builder("wire", "0.1.0")
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
            .build()
            .unwrap()
    }
    fn column(id: u64, path: &str) -> crate::ir::Node {
        crate::ir::Node {
            id: crate::ir::NodeId(id),
            path: crate::ir::NodePath(path.into()),
            kind: crate::ir::ComponentId(0),
            role: crate::ir::Role::Group,
            props: BTreeMap::new(),
            slots: Vec::new(),
            status: NodeStatus::Sealed,
            origin: "root".into(),
        }
    }
    fn address(path: &str, parent: Option<&str>, index: usize) -> crate::validate::Address {
        crate::validate::Address {
            path: path.into(),
            parent: parent.map(str::to_string),
            slot: if parent.is_some() {
                "children".into()
            } else {
                "root".into()
            },
            index,
            order: parent
                .map(|_| vec![("children".into(), index)])
                .unwrap_or_default(),
        }
    }
    fn projection(
        ids: Vec<u64>,
        before: &[(u64, &str, Option<&str>, usize)],
        after: &[(u64, &str, Option<&str>, usize)],
    ) -> crate::validate::Projection {
        crate::validate::Projection {
            ids: ids.into_iter().map(crate::ir::NodeId).collect(),
            before: before
                .iter()
                .map(|(id, path, parent, index)| {
                    (crate::ir::NodeId(*id), address(path, *parent, *index))
                })
                .collect(),
            after: after
                .iter()
                .map(|(id, path, parent, index)| {
                    (crate::ir::NodeId(*id), address(path, *parent, *index))
                })
                .collect(),
        }
    }
    fn tree_with(children: &[&str]) -> (Tree, Library, Ir) {
        let lib = wire_library();
        let mut tree = Tree::default();
        tree.apply(&insert("root", None, 0)).unwrap();
        let mut ir = Ir::default();
        ir.nodes.insert(crate::ir::NodeId(1), column(1, "root"));
        for (index, id) in children.iter().enumerate() {
            tree.apply(&insert(id, Some("root"), index as u32)).unwrap();
        }
        for (index, id) in children.iter().enumerate() {
            ir.nodes.insert(
                crate::ir::NodeId(11 + index as u64),
                column(11 + index as u64, id),
            );
        }
        (tree, lib, ir)
    }
    fn root_children(tree: &Tree) -> Vec<String> {
        tree.nodes
            .get("root")
            .and_then(|entry| entry.children.get("children"))
            .map(|children| children.iter().cloned().collect())
            .unwrap_or_default()
    }
    #[test]
    fn maintain_inserts_a_prefix_without_moving_retained_siblings() {
        let (mut tree, lib, mut ir) = tree_with(&["a"]);
        ir.nodes.insert(crate::ir::NodeId(12), column(12, "x"));
        let projection = projection(
            vec![12, 11],
            &[(11, "a", Some("root"), 0)],
            &[(12, "x", Some("root"), 0), (11, "a", Some("root"), 1)],
        );
        let (ops, _) = tree.maintain(&ir, &lib, &projection);
        assert_eq!(ops.len(), 1, "only the insert is required: {ops:?}");
        assert!(matches!(&ops[0], Op::Insert { node, index: 0, .. } if node.id == "x"));
        assert_eq!(root_children(&tree), vec!["x", "a"]);
    }
    #[test]
    fn maintain_reorders_siblings_from_their_retained_positions() {
        let (mut tree, lib, ir) = tree_with(&["a", "b"]);
        let projection = projection(
            vec![11, 12],
            &[(11, "a", Some("root"), 0), (12, "b", Some("root"), 1)],
            &[(12, "b", Some("root"), 0), (11, "a", Some("root"), 1)],
        );
        let (ops, _) = tree.maintain(&ir, &lib, &projection);
        assert!(
            matches!(&ops[0], Op::Move { id, index: 0, .. } if id == "b"),
            "unexpected ops: {ops:?}"
        );
        assert_eq!(root_children(&tree), vec!["b", "a"]);
    }
    #[test]
    fn maintain_reorders_retained_siblings_after_an_inserted_prefix() {
        let (mut tree, lib, mut ir) = tree_with(&["a", "b"]);
        ir.nodes.insert(crate::ir::NodeId(13), column(13, "x"));
        let projection = projection(
            vec![13, 11, 12],
            &[(11, "a", Some("root"), 0), (12, "b", Some("root"), 1)],
            &[
                (13, "x", Some("root"), 0),
                (12, "b", Some("root"), 1),
                (11, "a", Some("root"), 2),
            ],
        );
        let (ops, _) = tree.maintain(&ir, &lib, &projection);
        assert_eq!(
            ops,
            vec![
                Op::Insert {
                    parent: Some("root".into()),
                    slot: "children".into(),
                    index: 0,
                    node: node("x", "Column"),
                },
                Op::Move {
                    id: "b".into(),
                    parent: Some("root".into()),
                    slot: "children".into(),
                    index: 1,
                },
            ],
            "the inserted prefix shifts each later retained detach estimate"
        );
        assert_eq!(root_children(&tree), vec!["x", "b", "a"]);
    }
    #[test]
    fn maintain_reorders_after_an_earlier_move_in_the_same_slot() {
        let (mut tree, lib, ir) = tree_with(&["a", "b", "c"]);
        let projection = projection(
            vec![12, 13, 11],
            &[
                (11, "a", Some("root"), 0),
                (12, "b", Some("root"), 1),
                (13, "c", Some("root"), 2),
            ],
            &[
                (12, "b", Some("root"), 0),
                (13, "c", Some("root"), 1),
                (11, "a", Some("root"), 2),
            ],
        );
        let (ops, _) = tree.maintain(&ir, &lib, &projection);
        assert_eq!(
            ops,
            vec![
                Op::Move {
                    id: "b".into(),
                    parent: Some("root".into()),
                    slot: "children".into(),
                    index: 0,
                },
                Op::Move {
                    id: "c".into(),
                    parent: Some("root".into()),
                    slot: "children".into(),
                    index: 1,
                },
            ],
            "the first move shifts the next retained detach estimate"
        );
        assert_eq!(root_children(&tree), vec!["b", "c", "a"]);
    }
    #[test]
    fn maintain_moves_a_node_between_slots_at_an_unchanged_index() {
        let (mut tree, lib, ir) = tree_with(&["a", "other"]);
        let projection = projection(
            vec![11],
            &[(11, "a", Some("root"), 0)],
            &[(11, "a", Some("other"), 0)],
        );
        let (ops, _) = tree.maintain(&ir, &lib, &projection);
        assert!(
            matches!(&ops[0], Op::Move { id, parent: Some(parent), index: 0, .. }
                if id == "a" && parent == "other"),
            "unexpected ops: {ops:?}"
        );
        assert_eq!(
            tree.nodes.get("a").and_then(|entry| entry.parent.clone()),
            Some("other".into())
        );
    }
    #[test]
    fn insert_grafts_inserts_the_prefix_in_order_and_counts_each_wire_node() {
        let (mut tree, lib, mut ir) = tree_with(&[]);
        ir.nodes.insert(crate::ir::NodeId(12), column(12, "x"));
        ir.nodes.insert(crate::ir::NodeId(13), column(13, "y"));
        let graft = crate::validate::Graft {
            parent: "root".into(),
            slot: "children".into(),
            index: 0,
            children: vec![crate::ir::NodeId(12), crate::ir::NodeId(13)].into(),
            remove: None,
            order: Vec::new(),
        };
        let (ops, work) = tree.insert_grafts(&ir, &lib, &[graft]);
        assert_eq!(root_children(&tree), vec!["x", "y"]);
        assert!(matches!(&ops[0], Op::Insert { node, index: 0, .. } if node.id == "x"));
        assert!(matches!(&ops[1], Op::Insert { node, index: 1, .. } if node.id == "y"));
        assert_eq!(work.wire_nodes_examined, 2);
    }
    #[test]
    fn insert_grafts_applies_a_retained_removal_before_its_children() {
        let (mut tree, lib, ir) = tree_with(&["a"]);
        let graft = crate::validate::Graft {
            parent: "root".into(),
            slot: "children".into(),
            index: 0,
            children: Vec::new().into(),
            remove: Some("a".into()),
            order: Vec::new(),
        };
        let (ops, work) = tree.insert_grafts(&ir, &lib, &[graft]);
        assert_eq!(ops, vec![Op::Remove { id: "a".into() }]);
        assert!(!tree.nodes.contains_key("a"));
        assert_eq!(work.wire_nodes_examined, 1);
    }
    #[test]
    fn insert_grafts_updates_a_same_kind_node_instead_of_replacing_it() {
        let (mut tree, lib, mut ir) = tree_with(&["a"]);
        let node = ir.nodes.get_mut(&crate::ir::NodeId(11)).unwrap();
        node.props.insert(
            crate::ir::PropId(0),
            Prop::Static(PropValue::Value(Value::from("new"))),
        );
        let graft = crate::validate::Graft {
            parent: "root".into(),
            slot: "children".into(),
            index: 0,
            children: vec![crate::ir::NodeId(11)].into(),
            remove: None,
            order: Vec::new(),
        };
        let (ops, _) = tree.insert_grafts(&ir, &lib, &[graft]);
        assert_eq!(ops.len(), 1, "same-kind nodes are updated: {ops:?}");
        assert!(
            matches!(&ops[0], Op::SetProps { id, .. } if id == "a"),
            "unexpected ops: {ops:?}"
        );
    }
    #[test]
    fn maintain_charges_removals_and_insertions_as_wire_visits() {
        let (mut tree, lib, mut ir) = tree_with(&["a", "b"]);
        ir.nodes.remove(&crate::ir::NodeId(11));
        let removal = projection(vec![11], &[(11, "a", Some("root"), 0)], &[]);
        let (ops, work) = tree.maintain(&ir, &lib, &removal);
        assert_eq!(ops, vec![Op::Remove { id: "a".into() }]);
        assert_eq!(work.wire_nodes_examined, 1);

        let (mut tree, lib, mut ir) = tree_with(&["a"]);
        ir.nodes.insert(crate::ir::NodeId(12), column(12, "x"));
        let insertion = projection(vec![12], &[], &[(12, "x", Some("root"), 0)]);
        let (ops, work) = tree.maintain(&ir, &lib, &insertion);
        assert!(matches!(&ops[0], Op::Insert { node, .. } if node.id == "x"));
        assert_eq!(work.wire_nodes_examined, 1);
    }
    #[test]
    fn invalid_root_and_move_positions_leave_the_retained_tree_unchanged() {
        let mut empty = Tree::default();
        assert!(empty.apply(&insert("root", None, 1)).is_err());
        assert_eq!(empty, Tree::default());

        let mut tree = Tree::default();
        for op in [
            insert("root", None, 0),
            insert("a", Some("root"), 0),
            insert("b", Some("root"), 1),
        ] {
            tree.apply(&op).unwrap();
        }
        let before = tree.clone();
        assert!(
            tree.apply(&insert("other", None, 0)).is_err(),
            "a second root cannot replace the retained one"
        );
        assert_eq!(tree, before);

        let past_end = Op::Move {
            id: "a".into(),
            parent: Some("root".into()),
            slot: "children".into(),
            index: 2,
        };
        assert!(tree.apply(&past_end).is_err());
        assert_eq!(tree, before, "a rejected same-slot move must not detach");

        let far = Op::Move {
            id: "a".into(),
            parent: Some("root".into()),
            slot: "children".into(),
            index: 9,
        };
        assert!(tree.apply(&far).is_err());
        assert_eq!(tree, before, "an out-of-range index must be rejected first");

        let root_cycle = Op::Move {
            id: "a".into(),
            parent: None,
            slot: "root".into(),
            index: 0,
        };
        assert!(tree.apply(&root_cycle).is_err());
        assert_eq!(tree, before, "a non-root node cannot become the root");
    }
    #[test]
    fn moving_between_slots_allows_the_full_destination_length() {
        let mut tree = Tree::default();
        tree.apply(&insert("root", None, 0)).unwrap();
        for (index, id) in ["a", "b"].iter().enumerate() {
            tree.apply(&Op::Insert {
                parent: Some("root".into()),
                slot: "children".into(),
                index: index as u32,
                node: node(id, "Column"),
            })
            .unwrap();
        }
        for (index, id) in ["x", "y"].iter().enumerate() {
            tree.apply(&Op::Insert {
                parent: Some("root".into()),
                slot: "other".into(),
                index: index as u32,
                node: node(id, "Column"),
            })
            .unwrap();
        }
        let op = Op::Move {
            id: "a".into(),
            parent: Some("root".into()),
            slot: "other".into(),
            index: 2,
        };
        tree.apply(&op).unwrap();
        assert_eq!(
            tree.nodes["root"].children["other"]
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["x", "y", "a"]
        );
    }
    #[test]
    fn update_props_charges_one_comparison_per_candidate_and_retained_prop() {
        let lib = wire_library();
        let id = crate::ir::NodeId(1);
        let mut ir = Ir {
            root: Some(id),
            ..Ir::default()
        };
        let mut node = column(1, "root");
        node.kind = crate::ir::ComponentId(1);
        node.role = crate::ir::Role::Paragraph;
        node.props.insert(
            crate::ir::PropId(0),
            Prop::Static(PropValue::Value(Value::from("old"))),
        );
        ir.nodes.insert(id, node.clone());
        let mut tree = Tree::from_ir(&ir, &lib);
        node.props.insert(
            crate::ir::PropId(0),
            Prop::Static(PropValue::Value(Value::from("new"))),
        );
        ir.nodes.insert(id, node);
        let (ops, work) = tree.update_props(&ir, &lib, &[id]);
        assert!(ops.iter().any(|op| matches!(op, Op::SetProps { .. })));
        assert_eq!(
            work.props_compared, 3,
            "one target prop, one retained prop and the re-materialized prop"
        );
    }
    #[test]
    fn wire_values_recurse_through_lists_and_maps_for_model_text_hygiene() {
        let list = Value::List(Arc::new(vec![Value::from("a\u{202e}b")]));
        assert_eq!(wire_value(&list), json!(["ab"]));
        let map = Value::Map(Arc::new(vec![(
            "k".into(),
            Value::List(Arc::new(vec![Value::from("c\u{200b}d")])),
        )]));
        assert_eq!(wire_value(&map), json!({"map": [["k", ["cd"]]]}));
    }
    use std::sync::Arc;
}
