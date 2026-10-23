//! Persistent diagnostic multisets, indexed by code and provisional flag.
//! Materialization is an explicit observer operation, never part of publication.
use crate::{
    MaintenanceWork,
    diag::{Code, Diagnostic, Severity},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Deref,
    sync::{Arc, OnceLock},
};
type Counts = im::OrdMap<Diagnostic, usize>;
type Groups = im::OrdMap<Code, Counts>;
#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    entries: [Groups; 2],
    combined: Groups,
    empty: Counts,
    forced: Option<bool>,
    excluded: BTreeSet<Code>,
    // Per-code totals: multiplicity, fatal, committed fatal. The catalog has a
    // fixed number of codes, independent of the length of model input/history.
    totals: BTreeMap<Code, (usize, usize, usize)>,
    flat: Arc<OnceLock<Vec<Diagnostic>>>,
}
impl PartialEq for Diagnostics {
    fn eq(&self, other: &Self) -> bool {
        self.deref() == other.deref()
    }
}
impl Eq for Diagnostics {}
impl Diagnostics {
    pub fn len(&self) -> usize {
        self.totals
            .iter()
            .filter(|(code, _)| !self.excluded.contains(code))
            .map(|(_, n)| n.0)
            .sum()
    }
    pub(crate) fn contains_code(&self, code: Code) -> bool {
        !self.excluded.contains(&code) && self.totals.contains_key(&code)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn has_committed_fatal(&self) -> bool {
        self.totals
            .iter()
            .filter(|(code, _)| !self.excluded.contains(code))
            .any(|(_, n)| match self.forced {
                Some(true) => false,
                Some(false) => n.1 > 0,
                None => n.2 > 0,
            })
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn force_provisional(&mut self, flag: bool) {
        if self.forced != Some(flag) {
            self.forced = Some(flag);
            self.flat = Arc::new(OnceLock::new());
        }
    }
    pub(crate) fn excluding(&self, code: Code) -> Self {
        let mut next = self.clone();
        next.excluded.insert(code);
        next.flat = Arc::new(OnceLock::new());
        next
    }
    pub(crate) fn extend_raw(&mut self, values: impl IntoIterator<Item = Diagnostic>) {
        for mut d in values {
            let totals = self.totals.entry(d.code).or_default();
            totals.0 += 1;
            if d.severity == Severity::Fatal {
                totals.1 += 1;
                if !d.provisional {
                    totals.2 += 1;
                }
            }
            let flag = usize::from(d.provisional);
            d.provisional = false;
            for groups in [&mut self.entries[flag], &mut self.combined] {
                let mut group = groups.get(&d.code).cloned().unwrap_or_default();
                let count = group.get(&d).copied().unwrap_or(0);
                group.insert(d.clone(), count + 1);
                groups.insert(d.code, group);
            }
        }
        self.flat = Arc::new(OnceLock::new());
    }
    pub(crate) fn remove_raw(&mut self, diagnostic: &Diagnostic) {
        let mut d = diagnostic.clone();
        let flag = usize::from(d.provisional);
        d.provisional = false;
        if !self.entries[flag]
            .get(&d.code)
            .is_some_and(|group| group.contains_key(&d))
        {
            return;
        }
        for groups in [&mut self.entries[flag], &mut self.combined] {
            if let Some(group) = groups.get_mut(&d.code) {
                if let Some(n) = group.get(&d).copied() {
                    if n == 1 {
                        group.remove(&d);
                    } else {
                        group.insert(d.clone(), n - 1);
                    }
                }
                if group.is_empty() {
                    groups.remove(&d.code);
                }
            }
        }
        if let Some(totals) = self.totals.get_mut(&d.code) {
            totals.0 -= 1;
            if d.severity == Severity::Fatal {
                totals.1 -= 1;
                if flag == 0 {
                    totals.2 -= 1;
                }
            }
            if totals.0 == 0 {
                self.totals.remove(&d.code);
            }
        }
        self.flat = Arc::new(OnceLock::new());
    }
    fn effective(&self, flag: bool, code: Code) -> &Counts {
        if self.excluded.contains(&code) {
            return &self.empty;
        }
        let groups = match self.forced {
            Some(forced) if forced == flag => &self.combined,
            Some(_) => return &self.empty,
            None => &self.entries[usize::from(flag)],
        };
        groups.get(&code).unwrap_or(&self.empty)
    }
    fn changes(
        &self,
        previous: &Self,
        work: &mut MaintenanceWork,
    ) -> Vec<(Diagnostic, usize, usize)> {
        use im::ordmap::DiffItem;
        let codes: BTreeSet<_> = self
            .totals
            .keys()
            .chain(previous.totals.keys())
            .copied()
            .collect();
        let mut changes = Vec::new();
        for code in codes {
            for flag in [false, true] {
                for change in previous
                    .effective(flag, code)
                    .diff(self.effective(flag, code))
                {
                    work.diagnostics_examined += 1;
                    let (d, old, new) = match change {
                        DiffItem::Add(d, n) => (d, 0, *n),
                        DiffItem::Remove(d, n) => (d, *n, 0),
                        DiffItem::Update {
                            old: (_, old),
                            new: (d, new),
                        } => (d, *old, *new),
                    };
                    let mut d = d.clone();
                    d.provisional = flag;
                    changes.push((d, old, new));
                }
            }
        }
        changes
    }
    pub(crate) fn additions_since(
        &self,
        previous: &Self,
        work: &mut MaintenanceWork,
    ) -> Vec<Diagnostic> {
        self.changes(previous, work)
            .into_iter()
            .flat_map(|(d, old, new)| std::iter::repeat_n(d, new.saturating_sub(old)))
            .collect()
    }
    pub(crate) fn replace_layer(
        &mut self,
        previous: &Self,
        next: &Self,
        work: &mut MaintenanceWork,
    ) {
        for (d, old, new) in next.changes(previous, work) {
            for _ in new..old {
                self.remove_raw(&d);
            }
            self.extend_raw(std::iter::repeat_n(d, new.saturating_sub(old)));
        }
    }
}
impl From<Vec<Diagnostic>> for Diagnostics {
    fn from(values: Vec<Diagnostic>) -> Self {
        let mut ds = Self::default();
        ds.extend_raw(values);
        ds
    }
}
impl Deref for Diagnostics {
    type Target = [Diagnostic];
    fn deref(&self) -> &Self::Target {
        self.flat.get_or_init(|| {
            self.totals
                .keys()
                .flat_map(|code| {
                    [false, true].into_iter().flat_map(move |flag| {
                        self.effective(flag, *code).iter().flat_map(move |(d, n)| {
                            let mut d = d.clone();
                            d.provisional = flag;
                            std::iter::repeat_n(d, *n)
                        })
                    })
                })
                .collect()
        })
    }
}
impl IntoIterator for Diagnostics {
    type Item = Diagnostic;
    type IntoIter = std::vec::IntoIter<Diagnostic>;
    fn into_iter(self) -> Self::IntoIter {
        self.deref().to_vec().into_iter()
    }
}
impl<'a> IntoIterator for &'a Diagnostics {
    type Item = &'a Diagnostic;
    type IntoIter = std::slice::Iter<'a, Diagnostic>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Code;
    #[test]
    fn duplicate_multiplicity_survives_flag_overlays_and_withdrawal() {
        let d = Diagnostic::new(Code::Orphan, Some("a".into()), None);
        let mut old: Diagnostics = vec![d.clone(), d.clone()].into();
        old.force_provisional(true);
        let mut next: Diagnostics = vec![d.clone(), d.clone(), d.clone()].into();
        next.force_provisional(true);
        let delta = next.additions_since(&old, &mut MaintenanceWork::default());
        assert_eq!(delta.len(), 1);
        assert!(delta[0].provisional);
        next.force_provisional(false);
        let delta = next.additions_since(&old, &mut MaintenanceWork::default());
        assert_eq!(delta, vec![d.clone(), d.clone(), d.clone()]);
        let mut raw: Diagnostics = vec![d.clone(), d.clone()].into();
        raw.remove_raw(&d);
        assert_eq!(raw.len(), 1);
        raw.remove_raw(&d);
        assert!(raw.is_empty());
    }
    #[test]
    fn forcing_a_flag_merges_raw_entries_with_different_flags() {
        let d = Diagnostic::new(Code::Orphan, Some("a".into()), None);
        let mut provisional = d.clone();
        provisional.provisional = true;
        let old: Diagnostics = vec![d.clone(), provisional.clone()].into();
        let mut next = old.clone();
        next.force_provisional(true);
        assert_eq!(
            next.additions_since(&old, &mut MaintenanceWork::default()),
            vec![provisional]
        );
    }
}
