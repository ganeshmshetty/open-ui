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
    fn empty_owner_views_follow_the_live_frame_and_released_uses() {
        let mut arena = ExpressionArena::default();
        assert!(arena.is_empty());
        assert_eq!(arena.owner(), None);
        let scope = arena.enter("a".into(), vec![1]);
        assert_eq!(arena.owner(), Some("a"), "the active frame names its owner");
        let first = arena.intern(&string("first"));
        assert!(!arena.is_empty());
        arena.exit(scope);
        assert_eq!(arena.owner(), None);
        assert_eq!(arena.owner_ids("a"), vec![first.clone()]);

        let scope = arena.enter("a".into(), vec![1]);
        let second = arena.intern(&string("second"));
        arena.exit(scope);
        assert_eq!(
            arena.owner_ids("a"),
            vec![second],
            "a changed owner retains only its current uses"
        );
        assert!(arena.get(&first).is_none());
        assert_eq!(arena.len(), 1);
    }
    #[test]
    fn canonical_indices_use_the_recorded_owner_order_not_the_owner_key() {
        let mut arena = ExpressionArena::default();
        let scope = arena.enter("later".into(), vec![1]);
        let later = arena.intern(&string("later"));
        arena.exit(scope);
        let scope = arena.enter("earlier".into(), vec![2]);
        let earlier = arena.intern(&string("earlier"));
        arena.exit(scope);
        let indices = arena.canonical_indices();
        assert_eq!(indices[&later], 0, "owner order 1 sorts before order 2");
        assert_eq!(indices[&earlier], 1);

        let scope = arena.enter("later".into(), vec![3]);
        arena.intern(&string("later"));
        arena.exit(scope);
        let reordered = arena.canonical_indices();
        assert_eq!(
            reordered[&earlier], 0,
            "an owner's updated order participates in canonical sorting"
        );
        assert_eq!(reordered[&later], 1);
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
    #[test]
    fn public_arena_views_report_exactly_the_retained_entries() {
        // SPEC §§5.5,7: owners retain only live expressions; the observer views
        // must enumerate those bodies and distinguish shared roots.
        let mut arena = ExpressionArena::default();
        let scope = arena.enter("a".into(), vec![0]);
        let first = arena.intern(&string("first"));
        let second = arena.intern(&string("second"));
        arena.exit(scope);
        let mut observed: Vec<_> = arena.iter().cloned().collect();
        observed.sort_by_key(|e| format!("{e:?}"));
        assert_eq!(observed, vec![string("first"), string("second")]);
        let mut ids = arena.owner_ids("a");
        ids.sort();
        let mut expected = vec![first, second];
        expected.sort();
        assert_eq!(ids, expected);
        assert!(arena.owner_ids("missing").is_empty());

        let clone = arena.clone();
        assert!(arena.ptr_eq(&clone));
        let mut other = ExpressionArena::default();
        let scope = other.enter("a".into(), vec![0]);
        other.intern(&string("first"));
        other.exit(scope);
        assert!(!arena.ptr_eq(&other));
    }
    #[test]
    fn action_arena_views_enumerate_plans_and_distinguish_roots() {
        let plan = ActionPlan {
            steps: vec![crate::ir::Step::Set {
                state: "x".into(),
                value: ExprId {
                    hash: 1,
                    key: "k".into(),
                },
            }],
            origin: crate::ir::NodeId(1),
            submit_scope: None,
            locals: Default::default(),
        };
        let mut arena = ActionArena::default();
        assert!(arena.is_empty());
        let id = ActionPlanId("root|arg:on_click".into());
        arena.insert(id.clone(), plan.clone());
        assert!(!arena.is_empty());
        assert_eq!(arena.len(), 1);
        assert_eq!(arena.iter().collect::<Vec<_>>(), vec![&plan]);
        assert_eq!(arena.ids().collect::<Vec<_>>(), vec![&id]);

        let clone = arena.clone();
        assert!(arena.ptr_eq(&clone));
        let distinct = ActionArena::default();
        assert!(!arena.ptr_eq(&distinct));
        let mut same_contents = ActionArena::default();
        same_contents.insert(id, plan);
        assert!(!arena.ptr_eq(&same_contents));
    }
}
