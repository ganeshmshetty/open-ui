//! Persistent canonical task ranges. A profile describes the inputs for which
//! the retained result is valid, including a direct-slot stop and rollback.

use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Profile {
    pub used: usize,
    pub lower: usize,
    pub upper: Option<usize>,
    pub stopped: bool,
    pub rejected: usize,
    pub effects: usize,
}
impl Profile {
    pub fn new(
        used: usize,
        lower: usize,
        upper: Option<usize>,
        stopped: bool,
        rejected: usize,
    ) -> Self {
        Self {
            used,
            lower,
            upper,
            stopped,
            rejected,
            effects: 0,
        }
    }
    pub fn valid(self, remaining: usize) -> bool {
        remaining >= self.lower && self.upper.is_none_or(|n| remaining <= n)
    }
    pub fn then(self, next: Self) -> Self {
        if self.stopped {
            return self;
        }
        let upper = match (self.upper, next.upper.map(|n| n.saturating_add(self.used))) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        Self {
            used: self.used.saturating_add(next.used),
            lower: self.lower.max(self.used.saturating_add(next.lower)),
            upper,
            stopped: next.stopped,
            rejected: self.rejected.saturating_add(next.rejected),
            effects: self.effects.saturating_add(next.effects),
        }
    }
}
pub(super) trait Aggregate: Clone + Default {
    fn same(&self, other: &Self) -> bool;
    fn append(&mut self, other: &Self);
}
impl Aggregate for () {
    fn same(&self, _: &Self) -> bool {
        true
    }
    fn append(&mut self, _: &Self) {}
}
type Link<K, T, R> = Option<Arc<Node<K, T, R>>>;
#[derive(Clone)]
struct Inputs<R> {
    left: Option<Arc<R>>,
    item: Option<Arc<R>>,
    right: Option<Arc<R>>,
}
impl<R> Inputs<R> {
    fn same(&self, other: &Self) -> bool {
        fn same<R>(a: &Option<Arc<R>>, b: &Option<Arc<R>>) -> bool {
            match (a, b) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
        }
        same(&self.left, &other.left)
            && same(&self.item, &other.item)
            && same(&self.right, &other.right)
    }
}
#[derive(Clone)]
struct Node<K, T, R> {
    key: K,
    task: T,
    profile: Option<Profile>,
    result: Option<Arc<R>>,
    left: Link<K, T, R>,
    right: Link<K, T, R>,
    height: usize,
    summary: Option<Profile>,
    aggregate: Option<Arc<R>>,
    inputs: Option<Inputs<R>>,
}
fn height<K, T, R>(node: &Link<K, T, R>) -> usize {
    node.as_ref().map_or(0, |node| node.height)
}
impl<K: Ord + Clone, T: Clone + PartialEq, R: Aggregate> Node<K, T, R> {
    fn refresh(&mut self) {
        self.height = 1 + height(&self.left).max(height(&self.right));
        self.summary = None;
    }
    fn rotate_left(mut self) -> Self {
        let Some(right) = self.right.take() else {
            return self;
        };
        let mut right = (*right).clone();
        self.right = right.left.take();
        self.refresh();
        right.left = Some(Arc::new(self));
        right.refresh();
        right
    }
    fn rotate_right(mut self) -> Self {
        let Some(left) = self.left.take() else {
            return self;
        };
        let mut left = (*left).clone();
        self.left = left.right.take();
        self.refresh();
        left.right = Some(Arc::new(self));
        left.refresh();
        left
    }
    fn balance(mut self) -> Self {
        self.refresh();
        if height(&self.left) > height(&self.right) + 1 {
            if let Some(left) = &self.left
                && height(&left.right) > height(&left.left)
            {
                self.left = Some(Arc::new((**left).clone().rotate_left()));
            }
            self.rotate_right()
        } else if height(&self.right) > height(&self.left) + 1 {
            if let Some(right) = &self.right
                && height(&right.left) > height(&right.right)
            {
                self.right = Some(Arc::new((**right).clone().rotate_right()));
            }
            self.rotate_left()
        } else {
            self
        }
    }
    fn insert(link: &Link<K, T, R>, key: K, task: T) -> Arc<Self> {
        let Some(old) = link else {
            return Arc::new(Self {
                key,
                task,
                profile: None,
                result: None,
                left: None,
                right: None,
                height: 1,
                summary: None,
                aggregate: None,
                inputs: None,
            });
        };
        let mut node = (**old).clone();
        match key.cmp(&node.key) {
            std::cmp::Ordering::Less => node.left = Some(Self::insert(&node.left, key, task)),
            std::cmp::Ordering::Greater => node.right = Some(Self::insert(&node.right, key, task)),
            std::cmp::Ordering::Equal => {
                if node.task == task {
                    return old.clone();
                }
                node.task = task;
                node.profile = None;
            }
        }
        Arc::new(node.balance())
    }
    fn invalidate(link: &Link<K, T, R>, key: &K) -> Link<K, T, R> {
        let Some(old) = link else {
            return None;
        };
        let mut node = (**old).clone();
        match key.cmp(&node.key) {
            std::cmp::Ordering::Less => node.left = Self::invalidate(&node.left, key),
            std::cmp::Ordering::Greater => node.right = Self::invalidate(&node.right, key),
            std::cmp::Ordering::Equal => node.profile = None,
        }
        node.summary = None;
        Some(Arc::new(node))
    }
    fn reconcile(
        link: &Link<K, T, R>,
        remaining: usize,
        work: &mut u64,
        evaluate: &mut impl FnMut(&K, &T, usize) -> (Profile, R),
    ) -> (Link<K, T, R>, Profile, Option<Arc<R>>) {
        let Some(old) = link else {
            return (None, Profile::default(), None);
        };
        *work += 1;
        if let Some(summary) = old.summary
            && summary.valid(remaining)
        {
            return (link.clone(), summary, old.aggregate.clone());
        }
        let mut node = (**old).clone();
        let (left, before, left_result) = Self::reconcile(&node.left, remaining, work, evaluate);
        node.left = left;
        let (summary, inputs) = if before.stopped {
            (
                before,
                Inputs {
                    left: left_result,
                    item: None,
                    right: None,
                },
            )
        } else {
            let available = remaining.saturating_sub(before.used);
            let item = if let Some(profile) = node.profile
                && profile.valid(available)
            {
                profile
            } else {
                let (profile, result) = evaluate(&node.key, &node.task, available);
                if node.result.as_ref().is_none_or(|old| !old.same(&result)) {
                    node.result = Some(Arc::new(result));
                }
                profile
            };
            debug_assert!(item.valid(available) && item.used <= available);
            node.profile = Some(item);
            let prefix = before.then(item);
            if prefix.stopped {
                (
                    prefix,
                    Inputs {
                        left: left_result,
                        item: node.result.clone(),
                        right: None,
                    },
                )
            } else {
                let (right, after, right_result) = Self::reconcile(
                    &node.right,
                    remaining.saturating_sub(prefix.used),
                    work,
                    evaluate,
                );
                node.right = right;
                (
                    prefix.then(after),
                    Inputs {
                        left: left_result,
                        item: node.result.clone(),
                        right: right_result,
                    },
                )
            }
        };
        if node.inputs.as_ref().is_none_or(|old| !old.same(&inputs)) {
            let mut aggregate = R::default();
            for part in [&inputs.left, &inputs.item, &inputs.right]
                .into_iter()
                .flatten()
            {
                aggregate.append(part);
            }
            node.aggregate = Some(Arc::new(aggregate));
        }
        node.inputs = Some(inputs);
        node.summary = Some(summary);
        let result = node.aggregate.clone();
        (Some(Arc::new(node)), summary, result)
    }
    fn stopping_key(&self) -> Option<&K> {
        if !self.summary.is_some_and(|summary| summary.stopped) {
            return None;
        }
        if let Some(left) = &self.left
            && left.summary.is_some_and(|summary| summary.stopped)
        {
            return left.stopping_key();
        }
        if self.profile.is_some_and(|profile| profile.stopped) {
            return Some(&self.key);
        }
        self.right.as_ref().and_then(|right| right.stopping_key())
    }
    fn visit(
        &self,
        after: Option<&K>,
        until: Option<&K>,
        effects_only: bool,
        visit: &mut impl FnMut(&K, &T, &R),
    ) {
        if !self
            .summary
            .is_some_and(|profile| (!effects_only && profile.used > 0) || profile.effects > 0)
        {
            return;
        }
        if after.is_none_or(|after| self.key > *after)
            && let Some(left) = &self.left
        {
            left.visit(after, until, effects_only, visit);
        }
        if self
            .left
            .as_ref()
            .is_some_and(|left| left.summary.is_some_and(|profile| profile.stopped))
        {
            return;
        }
        if after.is_none_or(|after| self.key > *after)
            && until.is_none_or(|until| self.key <= *until)
            && self
                .profile
                .is_some_and(|profile| (!effects_only && profile.used > 0) || profile.effects > 0)
            && let Some(result) = &self.result
        {
            visit(&self.key, &self.task, result);
        }
        if self.profile.is_some_and(|profile| profile.stopped) {
            return;
        }
        if until.is_none_or(|until| self.key < *until)
            && let Some(right) = &self.right
        {
            right.visit(after, until, effects_only, visit);
        }
    }
}
#[derive(Clone)]
pub(super) struct ResourceIndex<K, T, R = ()> {
    root: Link<K, T, R>,
}
impl<K, T, R> Default for ResourceIndex<K, T, R> {
    fn default() -> Self {
        Self { root: None }
    }
}
impl<K: Ord + Clone, T: Clone + PartialEq, R: Aggregate> ResourceIndex<K, T, R> {
    pub fn insert(&mut self, key: K, task: T) {
        self.root = Some(Node::insert(&self.root, key, task));
    }
    pub fn invalidate(&mut self, key: &K) {
        self.root = Node::invalidate(&self.root, key);
    }
    pub fn reconcile_with(
        &mut self,
        limit: usize,
        work: &mut u64,
        mut evaluate: impl FnMut(&K, &T, usize) -> (Profile, R),
    ) -> (Profile, R) {
        let (root, profile, result) = Node::reconcile(&self.root, limit, work, &mut evaluate);
        self.root = root;
        (
            profile,
            result.map_or_else(R::default, |result| result.as_ref().clone()),
        )
    }
    pub fn stopping_key(&self) -> Option<&K> {
        self.root.as_ref().and_then(|root| root.stopping_key())
    }
    pub fn visit_effects(
        &self,
        after: Option<&K>,
        until: Option<&K>,
        mut visit: impl FnMut(&K, &T, &R),
    ) {
        if let Some(root) = &self.root {
            root.visit(after, until, true, &mut visit);
        }
    }
    pub fn visit_materialized(
        &self,
        after: Option<&K>,
        until: Option<&K>,
        mut visit: impl FnMut(&K, &T, &R),
    ) {
        if let Some(root) = &self.root {
            root.visit(after, until, false, &mut visit);
        }
    }
}
impl<K: Ord + Clone, T: Clone + PartialEq> ResourceIndex<K, T> {
    pub fn reconcile(
        &mut self,
        limit: usize,
        work: &mut u64,
        mut evaluate: impl FnMut(&K, &T, usize) -> Profile,
    ) -> Profile {
        self.reconcile_with(limit, work, |key, task, remaining| {
            (evaluate(key, task, remaining), ())
        })
        .0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn stopped_suffix_keeps_invalidations_until_it_is_reached() {
        let mut index = ResourceIndex::default();
        for i in 0..1000 {
            index.insert(i, 1usize);
        }
        let evaluate = |_: &usize, weight: &usize, available: usize| {
            if *weight <= available {
                Profile::new(*weight, *weight, None, false, 0)
            } else {
                Profile::new(0, 0, Some(*weight - 1), true, 1)
            }
        };
        let mut seen = Vec::new();
        let summary = index.reconcile(20, &mut 0, |key, weight, available| {
            seen.push(*key);
            evaluate(key, weight, available)
        });
        assert_eq!(seen, (0..=20).collect::<Vec<_>>());
        assert_eq!(
            (summary.used, summary.stopped, summary.rejected),
            (20, true, 1)
        );
        let mut previous = index.clone();
        index.insert(900, 2);
        index.invalidate(&901);
        seen.clear();
        let mut work = 0;
        index.reconcile(20, &mut work, |key, weight, available| {
            seen.push(*key);
            evaluate(key, weight, available)
        });
        assert!(seen.is_empty());
        assert!(work < 40, "unchanged prefix examined {work} ranges");
        let mut accepted = BTreeMap::new();
        index.reconcile(2000, &mut 0, |key, weight, available| {
            accepted.insert(*key, *weight);
            evaluate(key, weight, available)
        });
        assert_eq!(accepted.get(&900), Some(&2));
        assert_eq!(accepted.get(&901), Some(&1));
        assert!(
            previous
                .reconcile(20, &mut 0, |_, _, _| panic!("snapshot changed"))
                .stopped
        );
    }

    #[test]
    fn rolled_back_parent_retains_its_budget_constraints() {
        // A Tab reserves one node, then its required child fails at capacity 1.
        // Dropping the parent rolls allocation back, but its result is only
        // reusable when the next input budget still cannot fit the child.
        let rolled_back = Profile::new(0, 1, Some(1), false, 1);
        let sibling = Profile::new(1, 1, None, false, 0);
        let result = rolled_back.then(sibling);
        assert_eq!((result.used, result.lower, result.upper), (1, 1, Some(1)));
        assert!(!result.valid(0));
        assert!(result.valid(1));
        assert!(!result.valid(2));
        assert!(
            !result.stopped,
            "descendant stops do not stop the parent's slot"
        );
    }

    #[test]
    fn nested_slots_clear_only_their_own_stop_and_rollback_failed_containers() {
        let mut outer = ResourceIndex::default();
        let mut inner = std::collections::BTreeMap::new();
        for key in 0..12usize {
            outer.insert(key, key % 4 + 1);
            let mut slot = ResourceIndex::default();
            for child in 0..key % 4 + 1 {
                slot.insert(child, 1usize);
            }
            inner.insert(key, slot);
        }
        let mut results = BTreeMap::new();
        for budget in (0..60).chain((0..60).rev()) {
            let profile = outer.reconcile(budget, &mut 0, |key, count, remaining| {
                if remaining == 0 {
                    results.insert(*key, 0);
                    return Profile::new(0, 0, Some(0), true, 1);
                }
                let slot = inner.get_mut(key).unwrap();
                let children = slot.reconcile(remaining - 1, &mut 0, |_, _, available| {
                    if available > 0 {
                        Profile::new(1, 1, None, false, 0)
                    } else {
                        Profile::new(0, 0, Some(0), true, 1)
                    }
                });
                let mut result = Profile::new(1, 1, None, false, 0).then(children);
                result.stopped = false;
                // Every child is required, so a partial container rolls back.
                if children.used < *count {
                    result.used = 0;
                }
                results.insert(*key, result.used);
                result
            });
            let mut remaining = budget;
            let mut expected_stop = false;
            for key in 0..12usize {
                if remaining == 0 {
                    expected_stop = true;
                    break;
                }
                let required = 1 + key % 4 + 1;
                let used = if required <= remaining { required } else { 0 };
                assert_eq!(results[&key], used, "budget {budget}, task {key}");
                remaining -= used;
            }
            assert_eq!(
                (profile.used, profile.stopped),
                (budget - remaining, expected_stop)
            );
        }
    }

    #[test]
    fn nested_ranges_match_fresh_canonical_execution_across_frontier_moves() {
        let mut index = ResourceIndex::default();
        let mut weights = BTreeMap::new();
        for key in 0..80 {
            let weight = (key * 37) % 11 + 1;
            weights.insert(key, weight);
            index.insert(key, weight);
        }
        let mut observed = BTreeMap::new();
        for step in 0..400usize {
            let key = step * 31 % 80;
            let weight = step * 17 % 13 + 1;
            weights.insert(key, weight);
            index.insert(key, weight);
            let budget = step * 19 % 160;
            let profile = index.reconcile(budget, &mut 0, |key, weight, available| {
                // A failed container rolls back while preserving the upper
                // bound from its failing child. Its enclosing slot continues.
                let result = if *weight <= available {
                    Profile::new(*weight, *weight, None, false, 0)
                } else {
                    Profile::new(0, 0, Some(*weight - 1), false, 1)
                };
                observed.insert(*key, result.used);
                result
            });
            let mut remaining = budget;
            let mut rejected = 0;
            for (key, weight) in &weights {
                let used = if *weight <= remaining {
                    *weight
                } else {
                    rejected += 1;
                    0
                };
                assert_eq!(observed[key], used, "step {step}, key {key}");
                remaining -= used;
            }
            assert_eq!(
                (profile.used, profile.rejected),
                (budget - remaining, rejected)
            );
        }
    }
}
