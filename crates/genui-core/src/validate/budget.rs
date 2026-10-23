//! Greedy state/query allocation over the shared canonical resource index.
use super::resource::{Profile, ResourceIndex};

#[derive(Clone, Default)]
pub(super) struct BudgetIndex {
    tasks: ResourceIndex<String, usize>,
    accepted: crate::SharedMap<String, bool>,
}
impl BudgetIndex {
    pub fn insert(&mut self, key: String, weight: usize) {
        self.tasks.insert(key.clone(), weight);
        // An accepted task may have changed weight. Re-evaluate that task,
        // keeping the other cached task/range profiles and prior snapshot.
        self.tasks.invalidate(&key);
    }
    pub fn reconcile(
        &mut self,
        limit: usize,
        work: &mut u64,
    ) -> (usize, usize, Vec<(String, bool)>) {
        let mut changes = Vec::new();
        let profile = self.tasks.reconcile(limit, work, |key, weight, remaining| {
            let accepted = *weight <= remaining;
            if self.accepted.get(key).copied() != Some(accepted) {
                changes.push((key.clone(), accepted));
                self.accepted.insert(key.clone(), accepted);
            }
            if accepted {
                Profile::new(*weight, *weight, None, false, 0)
            } else {
                Profile::new(0, 0, Some(*weight - 1), false, 1)
            }
        });
        (profile.used, profile.rejected, changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interval_summary_matches_greedy_for_every_small_budget() {
        for seed in 0..40usize {
            let weights: Vec<_> = (0..50).map(|i| (i * 37 + seed * 19) % 17).collect();
            let mut index = BudgetIndex::default();
            for (i, w) in weights.iter().enumerate() {
                index.insert(format!("{i:03}"), *w);
            }
            let mut accepted = std::collections::BTreeMap::new();
            for limit in (0..120).chain((0..120).rev()) {
                let (used, rejected, changes) = index.reconcile(limit, &mut 0);
                for (key, yes) in changes {
                    accepted.insert(key, yes);
                }
                let mut expected_used = 0;
                let mut expected_rejected = 0;
                for (i, w) in weights.iter().enumerate() {
                    let yes = *w <= limit - expected_used;
                    assert_eq!(accepted[&format!("{i:03}")], yes);
                    if yes {
                        expected_used += w;
                    } else {
                        expected_rejected += 1;
                    }
                }
                assert_eq!((used, rejected), (expected_used, expected_rejected));
            }
        }
    }
    #[test]
    fn earlier_insertion_skips_unchanged_ranges_and_preserves_snapshot() {
        let mut index = BudgetIndex::default();
        for i in 0..1000 {
            index.insert(format!("s{i:04}"), 3);
        }
        assert_eq!(index.reconcile(3000, &mut 0).0, 3000);
        let mut previous = index.clone();
        index.insert("a".into(), 3);
        let mut work = 0;
        let (used, rejected, changes) = index.reconcile(3000, &mut work);
        assert_eq!((used, rejected), (3000, 1));
        assert_eq!(changes, vec![("a".into(), true), ("s0999".into(), false)]);
        assert!(work < 60, "{work}");
        assert!(previous.reconcile(3000, &mut 0).2.is_empty());
    }
}
