//! Live expression identity is content plus recovery metadata, independent of
//! dense snapshot position. Owners retain only the expressions they still use.
use crate::{
    SharedMap,
    ir::{ActionPlan, ActionPlanId, ExprId},
    syntax::ast::Expr,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
#[derive(Clone, Debug, Default)]
struct Owner {
    order: Vec<u64>,
    uses: Vec<ExprId>,
}
#[derive(Clone, Debug, Default)]
struct Frame {
    key: String,
    order: Vec<u64>,
    uses: Vec<ExprId>,
    action: bool,
}
pub(crate) struct Scope(Option<Frame>);
#[derive(Clone, Debug, Default)]
pub struct ExpressionArena {
    entries: SharedMap<ExprId, Expr>,
    owners: SharedMap<String, Owner>,
    references: SharedMap<ExprId, usize>,
    active: Option<Frame>,
}
impl PartialEq for ExpressionArena {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}
#[derive(Clone)]
pub(crate) struct OwnerSnapshot {
    key: String,
    order: Vec<u64>,
    uses: Vec<(ExprId, Arc<Expr>)>,
}
impl ExpressionArena {
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn get(&self, id: &ExprId) -> Option<&Expr> {
        self.entries.get(id)
    }
    pub fn iter(&self) -> impl Iterator<Item = &Expr> {
        self.entries.values()
    }
    pub fn ptr_eq(&self, other: &Self) -> bool {
        self.entries.ptr_eq(&other.entries)
    }
    pub(crate) fn enter(&mut self, key: String, order: Vec<u64>) -> Scope {
        Scope(self.active.replace(Frame {
            key,
            order,
            ..Frame::default()
        }))
    }
    pub(crate) fn owner(&self) -> Option<&str> {
        self.active.as_ref().map(|frame| frame.key.as_str())
    }
    pub(crate) fn mark_action(&mut self) {
        if let Some(frame) = &mut self.active {
            frame.action = true;
        }
    }
    pub(crate) fn intern(&mut self, expression: &Expr) -> ExprId {
        self.intern_key(expression, false)
    }
    pub(crate) fn intern_unique(&mut self, expression: &Expr) -> ExprId {
        self.intern_key(expression, true)
    }
    fn intern_key(&mut self, expression: &Expr, unique: bool) -> ExprId {
        // Derived Debug is an internal structural key, including NullOrigin. It
        // is never wire data or canonical bytes. Hashes accelerate comparison;
        // equality/order also compare the complete key, including collisions.
        let key: Arc<str> = if unique {
            format!("unique:{:?}:{expression:?}", self.owner()).into()
        } else {
            format!("{expression:?}").into()
        };
        let id = ExprId {
            hash: xxhash_rust::xxh3::xxh3_64(key.as_bytes()),
            key,
        };
        if !self.entries.contains_key(&id) {
            self.entries.insert(id.clone(), expression.clone());
        }
        if let Some(frame) = &mut self.active {
            frame.uses.push(id.clone());
        }
        id
    }
    pub(crate) fn exit(&mut self, scope: Scope) -> (Vec<ExprId>, Option<String>) {
        let frame = self.active.take();
        self.active = scope.0;
        let Some(frame) = frame else {
            return (Vec::new(), None);
        };
        let removed_action = (!frame.action).then(|| frame.key.clone());
        let old = self.owners.get(&frame.key).cloned().unwrap_or_default();
        let old_ids: BTreeSet<_> = old.uses.iter().cloned().collect();
        let next_ids: BTreeSet<_> = frame.uses.iter().cloned().collect();
        let mut removed = Vec::new();
        for id in old_ids.difference(&next_ids) {
            if let Some(count) = self.references.get(id).copied() {
                if count == 1 {
                    self.references.remove(id);
                    self.entries.remove(id);
                    removed.push(id.clone());
                } else {
                    self.references.insert(id.clone(), count - 1);
                }
            }
        }
        for id in next_ids.difference(&old_ids) {
            let count = self.references.get(id).copied().unwrap_or(0);
            self.references.insert(id.clone(), count + 1);
        }
        if frame.uses.is_empty() {
            self.owners.remove(&frame.key);
        } else if old.uses != frame.uses || old.order != frame.order {
            self.owners.insert(
                frame.key,
                Owner {
                    order: frame.order,
                    uses: frame.uses,
                },
            );
        }
        (removed, removed_action)
    }
    pub(crate) fn remove_owner(&mut self, key: &str) -> Vec<ExprId> {
        let scope = self.enter(key.into(), Vec::new());
        self.exit(scope).0
    }
    pub(crate) fn owner_ids(&self, key: &str) -> Vec<ExprId> {
        self.owners
            .get(key)
            .map_or_else(Vec::new, |owner| owner.uses.clone())
    }
    pub(crate) fn snapshot_owner(&self, key: &str) -> Option<OwnerSnapshot> {
        let owner = self.owners.get(key)?;
        Some(OwnerSnapshot {
            key: key.into(),
            order: owner.order.clone(),
            uses: owner
                .uses
                .iter()
                .filter_map(|id| {
                    self.entries
                        .get_shared(id)
                        .map(|expression| (id.clone(), expression))
                })
                .collect(),
        })
    }
    pub(crate) fn restore_owner(&mut self, owner: &OwnerSnapshot) {
        let scope = self.enter(owner.key.clone(), owner.order.clone());
        for (id, expression) in &owner.uses {
            if !self.entries.contains_key(id) {
                self.entries.insert(id.clone(), expression.as_ref().clone());
            }
            if let Some(frame) = &mut self.active {
                frame.uses.push(id.clone());
            }
        }
        self.exit(scope);
    }
    /// Canonical dense indices are a snapshot view, never mutable live identity.
    pub(crate) fn canonical_indices(&self) -> BTreeMap<ExprId, usize> {
        let mut owners = self.owners.iter().collect::<Vec<_>>();
        owners.sort_by(|(ak, a), (bk, b)| (&a.order, ak).cmp(&(&b.order, bk)));
        let mut indices = BTreeMap::new();
        for (_, owner) in owners {
            for id in &owner.uses {
                let next = indices.len();
                indices.entry(id.clone()).or_insert(next);
            }
        }
        // Public standalone intern() calls have no validation owner. Retain a
        // deterministic observer view for them without imposing a live scan.
        for id in self.entries.keys() {
            let next = indices.len();
            indices.entry(id.clone()).or_insert(next);
        }
        indices
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActionArena {
    entries: SharedMap<ActionPlanId, ActionPlan>,
}
impl ActionArena {
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn get(&self, id: &ActionPlanId) -> Option<&ActionPlan> {
        self.entries.get(id)
    }
    pub fn iter(&self) -> impl Iterator<Item = &ActionPlan> {
        self.entries.values()
    }
    pub fn ids(&self) -> impl Iterator<Item = &ActionPlanId> {
        self.entries.keys()
    }
    pub fn ptr_eq(&self, other: &Self) -> bool {
        self.entries.ptr_eq(&other.entries)
    }
    pub(crate) fn insert(&mut self, id: ActionPlanId, plan: ActionPlan) {
        if self.entries.get(&id) != Some(&plan) {
            self.entries.insert(id, plan);
        }
    }
    pub(crate) fn remove(&mut self, id: &ActionPlanId) {
        self.entries.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn string(value: &str) -> Expr {
        Expr::String {
            value: value.into(),
        }
    }
    #[test]
    fn changing_an_owner_releases_obsolete_bodies_and_retains_shared_ones() {
        let mut arena = ExpressionArena::default();
        let scope = arena.enter("a".into(), vec![0]);
        let shared = arena.intern(&string("shared"));
        arena.exit(scope);
        let scope = arena.enter("b".into(), vec![1]);
        assert_eq!(arena.intern(&string("shared")), shared);
        arena.exit(scope);
        for i in 0..1000 {
            let scope = arena.enter("a".into(), vec![0]);
            arena.intern(&string(&format!("new{i}")));
            arena.exit(scope);
            assert_eq!(arena.len(), 2);
            assert!(arena.get(&shared).is_some());
        }
        arena.remove_owner("b");
        assert_eq!(arena.len(), 1);
        assert!(arena.get(&shared).is_none());
    }
    #[test]
    fn snapshot_indices_follow_canonical_use_without_mutating_live_entries() {
        let mut arena = ExpressionArena::default();
        let scope = arena.enter("later".into(), vec![1]);
        let later = arena.intern(&string("later"));
        arena.exit(scope);
        let retained = arena.clone();
        let scope = arena.enter("earlier".into(), vec![0]);
        let earlier = arena.intern(&string("earlier"));
        arena.exit(scope);
        let indices = arena.canonical_indices();
        assert_eq!(indices[&earlier], 0);
        assert_eq!(indices[&later], 1);
        assert!(std::ptr::eq(
            retained.get(&later).unwrap(),
            arena.get(&later).unwrap()
        ));
        let before = arena.clone();
        assert_eq!(arena.canonical_indices(), indices);
        assert!(before.ptr_eq(&arena));
    }
    #[test]
    fn a_hash_collision_never_merges_different_expression_keys() {
        let mut arena = ExpressionArena::default();
        let a = ExprId {
            hash: 7,
            key: "a".into(),
        };
        let b = ExprId {
            hash: 7,
            key: "b".into(),
        };
        arena.entries.insert(a.clone(), string("a"));
        arena.entries.insert(b.clone(), string("b"));
        assert_eq!(arena.len(), 2);
        assert_eq!(arena.get(&a), Some(&string("a")));
        assert_eq!(arena.get(&b), Some(&string("b")));
    }
}
