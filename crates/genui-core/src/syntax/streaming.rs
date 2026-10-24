//! Immutable sealing and display-only tails (§3), using the sole splitter.
use super::{
    ast::*,
    parser::parse_split,
    splitter::{SplitResult, SplitStatement, Splitter},
};
use crate::{
    Diagnostics, MaintenanceWork,
    diag::{Code, Diagnostic, Span, order_diagnostics},
    limits::SyntaxLimits,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub struct Seal {
    pub hash: u128,
    pub span: Span,
    pub statement: Option<Statement>,
}
pub struct StreamingParser {
    splitter: Option<Splitter>,
    limits: SyntaxLimits,
    accepted: usize,
    truncated: bool,
    program_size_reported: bool,
    count: usize,
    names: BTreeSet<(bool, String)>,
    seals: Vec<Seal>,
    program: Program,
    scan_diagnostics: Diagnostics,
    diagnostics: Diagnostics,
    tail_diagnostics: Diagnostics,
    provisional: Option<Statement>,
    provisional_diagnostics: Vec<Diagnostic>,
    finished: bool,
    parsed_seals: usize,
    tail_parses: usize,
    tail_bytes: usize,
    scan_diagnostic_work: usize,
    diagnostic_work: MaintenanceWork,
}
impl StreamingParser {
    pub fn new(limits: SyntaxLimits) -> Self {
        let limits = limits.bounded();
        Self {
            splitter: Some(Splitter::new(limits.clone())),
            limits,
            accepted: 0,
            truncated: false,
            program_size_reported: false,
            count: 0,
            names: BTreeSet::new(),
            seals: Vec::new(),
            program: Program::default(),
            scan_diagnostics: Default::default(),
            diagnostics: Default::default(),
            tail_diagnostics: Default::default(),
            provisional: None,
            provisional_diagnostics: Vec::new(),
            finished: false,
            parsed_seals: 0,
            tail_parses: 0,
            tail_bytes: 0,
            scan_diagnostic_work: 0,
            diagnostic_work: Default::default(),
        }
    }
    pub fn push(&mut self, bytes: &[u8]) {
        if self.finished {
            return;
        }
        let n = bytes
            .len()
            .min(self.limits.max_program_bytes.saturating_sub(self.accepted));
        self.accepted += n;
        self.truncated |= n < bytes.len();
        let Some(mut splitter) = self.splitter.take() else {
            return;
        };
        splitter.push(&bytes[..n]);
        let known = self.receive_scan(
            splitter.diagnostic_snapshot(),
            splitter.take_diagnostic_delta(),
        );
        self.scan_diagnostic_work = splitter.diagnostic_work();
        for s in splitter.take_sealed() {
            self.seal(s, &known, splitter.source());
        }
        self.tail_bytes = splitter.held_prefix_bytes();
        if let Some((tail, extra)) = splitter.provisional() {
            self.tail_bytes = self
                .tail_bytes
                .max(tail.span.end.saturating_sub(tail.span.start));
            if self.count >= self.limits.max_stmt_count
                || tail.header.as_ref().is_some_and(|h| {
                    self.names
                        .contains(&(h.kind == StatementKind::State, h.name.clone()))
                })
            {
                self.provisional = None;
                self.provisional_diagnostics.clear();
            } else {
                self.tail_parses += 1;
                let parsed = self.parse_one(tail, &extra);
                if let Some(s) = parsed.program.statements.into_iter().next() {
                    self.provisional = Some(s);
                    self.provisional_diagnostics = parsed.diagnostics;
                }
                // Parse failure preserves the last successful display candidate.
            }
        }
        self.sync_tail_diagnostics();
        self.splitter = Some(splitter);
    }
    fn receive_scan(
        &mut self,
        next: Diagnostics,
        delta: Vec<Diagnostic>,
    ) -> BTreeMap<usize, Vec<Diagnostic>> {
        self.diagnostics
            .replace_layer(&self.scan_diagnostics, &next, &mut self.diagnostic_work);
        self.scan_diagnostics = next;
        // Only these three scanner codes annotate recovered nulls. All scanner
        // diagnostics are already present in the retained diagnostic multiset.
        let mut known: BTreeMap<usize, Vec<Diagnostic>> = BTreeMap::new();
        for d in delta {
            if matches!(d.code, Code::Overflow | Code::NonFinite | Code::Depth)
                && let Some(span) = &d.span
            {
                known.entry(span.start).or_default().push(d);
            }
        }
        known
    }
    fn sync_tail_diagnostics(&mut self) {
        self.diagnostic_work.diagnostics_examined += self.provisional_diagnostics.len() as u64;
        let next: Diagnostics = self
            .provisional_diagnostics
            .iter()
            .cloned()
            .map(|mut d| {
                d.provisional = true;
                d
            })
            .collect::<Vec<_>>()
            .into();
        self.diagnostics
            .replace_layer(&self.tail_diagnostics, &next, &mut self.diagnostic_work);
        self.tail_diagnostics = next;
        if self.truncated && !self.program_size_reported {
            self.diagnostics
                .extend_raw([Diagnostic::new(Code::ProgramSize, None, None)]);
            self.program_size_reported = true;
        }
    }
    fn parse_one(&self, s: SplitStatement, known: &[Diagnostic]) -> super::ParseResult {
        let mut limits = self.limits.clone();
        limits.max_stmt_count = usize::MAX;
        parse_split(
            SplitResult {
                statements: vec![s],
                diagnostics: known.to_vec(),
            },
            &limits,
        )
    }
    fn seal(&mut self, s: SplitStatement, known: &BTreeMap<usize, Vec<Diagnostic>>, source: &str) {
        self.provisional = None;
        self.provisional_diagnostics.clear();
        let hash = xxhash_rust::xxh3::xxh3_128(
            source
                .get(s.span.start..s.span.end)
                .unwrap_or("")
                .as_bytes(),
        );
        let span = s.span.clone();
        let rejection = s.header.as_ref().and_then(|h| {
            self.count += 1;
            if self.count > self.limits.max_stmt_count {
                Some(Code::StmtCount)
            } else if h.name.starts_with("__") {
                Some(Code::ReservedName)
            } else if !self
                .names
                .insert((h.kind == StatementKind::State, h.name.clone()))
            {
                Some(Code::DuplicateName)
            } else {
                None
            }
        });
        let statement = if let Some(code) = rejection {
            self.diagnostics.extend_raw([Diagnostic::new(
                code,
                s.header.as_ref().map(|h| h.name.clone()),
                Some(span.clone()),
            )]);
            None
        } else {
            let relevant: Vec<_> = known
                .range(span.start..=span.end)
                .flat_map(|(_, ds)| ds.iter().cloned())
                .collect();
            self.diagnostic_work.diagnostics_examined += relevant.len() as u64;
            self.parsed_seals += 1;
            let parsed = self.parse_one(s, &relevant);
            let mut included: BTreeMap<Diagnostic, usize> = BTreeMap::new();
            for d in relevant {
                *included.entry(d).or_default() += 1;
            }
            let mut added = Vec::new();
            for d in parsed.diagnostics {
                self.diagnostic_work.diagnostics_examined += 1;
                if let Some(count) = included.get_mut(&d)
                    && *count > 0
                {
                    *count -= 1;
                } else {
                    added.push(d);
                }
            }
            self.diagnostics.extend_raw(added);
            parsed.program.statements.into_iter().next()
        };
        if let Some(s) = &statement {
            self.program.statements.push(s.clone());
        }
        self.seals.push(Seal {
            hash,
            span,
            statement,
        });
    }
    pub fn finish(&mut self) {
        if self.finished {
            return;
        }
        if let Some(splitter) = self.splitter.take() {
            let (result, snapshot, delta, source, work) = splitter.finish_stream();
            let known = self.receive_scan(snapshot, delta);
            self.scan_diagnostic_work = work;
            for s in result.statements {
                self.seal(s, &known, &source);
            }
        }
        self.provisional = None;
        self.provisional_diagnostics.clear();
        self.tail_bytes = 0;
        self.sync_tail_diagnostics();
        self.finished = true;
    }
    pub fn work(&self) -> ScanWork {
        ScanWork {
            accepted_bytes: self.accepted,
            parsed_seals: self.parsed_seals,
            tail_parses: self.tail_parses,
            tail_bytes: self.tail_bytes,
            tail_limit: self.limits.max_stmt_bytes,
            diagnostics_examined: self.scan_diagnostic_work
                + self.diagnostic_work.diagnostics_examined as usize,
        }
    }
    pub fn seals(&self) -> &[Seal] {
        &self.seals
    }
    pub fn program(&self) -> &Program {
        &self.program
    }
    pub fn provisional(&self) -> Option<&Statement> {
        self.provisional.as_ref()
    }
    pub(crate) fn diagnostic_snapshot(&self) -> Diagnostics {
        self.diagnostics.clone()
    }
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let mut ds = self.diagnostics.to_vec();
        order_diagnostics(&mut ds);
        ds
    }
}
/// Logical work includes scanner staging and diagnostic changes, excluding explicit
/// observer traversal/materialization of a returned collection.
#[derive(Clone, Copy, Debug)]
pub struct ScanWork {
    pub accepted_bytes: usize,
    pub parsed_seals: usize,
    pub tail_parses: usize,
    pub tail_bytes: usize,
    pub tail_limit: usize,
    pub diagnostics_examined: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_provisional_statement_whose_name_is_already_sealed_is_discarded() {
        let mut parser = StreamingParser::new(SyntaxLimits::default());
        parser.push(b"x=1\n");
        assert_eq!(parser.seals().len(), 1);
        assert!(parser.seals()[0].statement.is_some());
        assert!(parser.provisional().is_none());

        parser.push(b"x=Leaf(\"a");
        assert!(
            parser.provisional().is_none(),
            "a provisional statement never claims an already sealed name: {:?}",
            parser.provisional()
        );
        parser.finish();
        assert!(
            parser
                .diagnostics()
                .iter()
                .any(|d| d.code == Code::DuplicateName)
        );
    }
    #[test]
    fn the_statement_budget_allows_exactly_the_configured_count() {
        let mut limits = SyntaxLimits::default();
        limits.max_stmt_count = 1;
        let mut parser = StreamingParser::new(limits);
        parser.push(b"a=1\n");
        assert!(
            parser.seals()[0].statement.is_some(),
            "the first statement is within the budget: {:?}",
            parser.diagnostics()
        );
        assert!(
            parser
                .diagnostics()
                .iter()
                .all(|d| d.code != Code::StmtCount)
        );
    }
    #[test]
    fn the_parser_exposes_sealed_programs_and_ordered_diagnostics() {
        let mut parser = StreamingParser::new(SyntaxLimits::default());
        parser.push(b"a=1\nb=2\n");
        parser.finish();
        assert_eq!(
            parser
                .program()
                .statements
                .iter()
                .map(|statement| statement.name.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );

        let mut parser = StreamingParser::new(SyntaxLimits::default());
        parser.push(b"root=999999999999999999999999\n");
        let diagnostics = parser.diagnostics();
        assert!(
            diagnostics.iter().any(|d| d.code == Code::Overflow),
            "{diagnostics:?}"
        );
        let mut ordered = diagnostics.clone();
        order_diagnostics(&mut ordered);
        assert_eq!(diagnostics, ordered);
    }
    #[test]
    fn scanner_and_parser_depth_diagnostics_both_survive_sealing() {
        let mut limits = SyntaxLimits::default();
        limits.max_bracket_depth = 1;
        limits.max_expr_depth = 1;
        let mut parser = StreamingParser::new(limits);
        parser.push(b"root=[[a]]\n");
        parser.finish();
        let depths: Vec<_> = parser
            .diagnostics()
            .into_iter()
            .filter(|d| d.code == Code::Depth)
            .collect();
        assert_eq!(
            depths.len(),
            2,
            "the scanner and the parser each report the truncated bracket: {depths:?}"
        );
        assert_eq!(depths[0].span, depths[1].span);
    }
    #[test]
    fn sealing_charges_each_relevant_scanner_diagnostic() {
        let mut parser = StreamingParser::new(SyntaxLimits::default());
        parser.push(b"root=999999999999999999999999\n");
        assert!(
            parser
                .diagnostic_snapshot()
                .iter()
                .any(|d| d.code == Code::Overflow)
        );
        // The staged scan change and the scan diagnostic matched by the sealed
        // statement are both examined; sealing must not subtract either.
        assert_eq!(
            parser.diagnostic_work.diagnostics_examined, 3,
            "{:?}",
            parser.diagnostic_work
        );
    }
}
