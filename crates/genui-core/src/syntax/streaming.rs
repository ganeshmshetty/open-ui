//! Immutable sealing and display-only tail parsing (§3), sharing the sole splitter.
use super::{
    ast::*,
    parser::parse_split,
    splitter::{SplitResult, SplitStatement, Splitter},
};
use crate::{
    diag::{Code, Diagnostic, Span, order_diagnostics},
    limits::SyntaxLimits,
};
use std::collections::BTreeSet;

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
    count: usize,
    names: BTreeSet<(bool, String)>,
    seals: Vec<Seal>,
    program: Program,
    parse_diagnostics: Vec<Diagnostic>,
    scan_diagnostics: Vec<Diagnostic>,
    provisional: Option<Statement>,
    provisional_diagnostics: Vec<Diagnostic>,
    finished: bool,
    parsed_seals: usize,
    tail_parses: usize,
    tail_bytes: usize,
}
impl StreamingParser {
    pub fn new(limits: SyntaxLimits) -> Self {
        let limits = limits.bounded();
        Self {
            splitter: Some(Splitter::new(limits.clone())),
            limits,
            accepted: 0,
            truncated: false,
            count: 0,
            names: BTreeSet::new(),
            seals: Vec::new(),
            program: Program::default(),
            parse_diagnostics: Vec::new(),
            scan_diagnostics: Vec::new(),
            provisional: None,
            provisional_diagnostics: Vec::new(),
            finished: false,
            parsed_seals: 0,
            tail_parses: 0,
            tail_bytes: 0,
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
        let diagnostics = splitter.diagnostics();
        for s in splitter.take_sealed() {
            self.seal(s, &diagnostics, splitter.source());
        }
        self.scan_diagnostics = diagnostics;
        self.tail_bytes = 0;
        if let Some((tail, extra)) = splitter.provisional() {
            self.tail_bytes = tail.span.end.saturating_sub(tail.span.start);
            let header = tail.header.as_ref();
            if header.is_some_and(|h| {
                self.names
                    .contains(&(h.kind == StatementKind::State, h.name.clone()))
            }) {
                self.provisional = None;
                self.provisional_diagnostics.clear();
            } else {
                self.tail_parses += 1;
                let parsed = self.parse_one(tail, &extra);
                if let Some(s) = parsed.program.statements.into_iter().next() {
                    self.provisional = Some(s);
                    self.provisional_diagnostics = parsed.diagnostics;
                }
                // Parse failure keeps the previous successful display candidate unchanged.
            }
        }
        self.splitter = Some(splitter);
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
    fn seal(&mut self, s: SplitStatement, known: &[Diagnostic], source: &str) {
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
            self.parse_diagnostics.push(Diagnostic::new(
                code,
                s.header.as_ref().map(|h| h.name.clone()),
                Some(span.clone()),
            ));
            None
        } else {
            let relevant: Vec<_> = known
                .iter()
                .filter(|d| {
                    d.span
                        .as_ref()
                        .is_some_and(|p| p.start >= span.start && p.start <= span.end)
                })
                .cloned()
                .map(|mut d| {
                    d.provisional = false;
                    d
                })
                .collect();
            self.parsed_seals += 1;
            let mut parsed = self.parse_one(s, &relevant);
            for d in relevant {
                if let Some(i) = parsed.diagnostics.iter().position(|v| *v == d) {
                    parsed.diagnostics.remove(i);
                }
            }
            self.parse_diagnostics.extend(parsed.diagnostics);
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
            let (result, source) = splitter.finish_with_source();
            for s in result.statements {
                self.seal(s, &result.diagnostics, &source);
            }
            self.scan_diagnostics = result.diagnostics;
        }
        self.provisional = None;
        self.provisional_diagnostics.clear();
        self.finished = true;
    }
    pub fn work(&self) -> ScanWork {
        ScanWork {
            accepted_bytes: self.accepted,
            parsed_seals: self.parsed_seals,
            tail_parses: self.tail_parses,
            tail_bytes: self.tail_bytes,
            tail_limit: self.limits.max_stmt_bytes,
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
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let mut ds = self.scan_diagnostics.clone();
        ds.extend(self.parse_diagnostics.clone());
        ds.extend(self.provisional_diagnostics.iter().cloned().map(|mut d| {
            d.provisional = true;
            d
        }));
        if self.truncated {
            ds.push(Diagnostic::new(Code::ProgramSize, None, None));
        }
        order_diagnostics(&mut ds);
        ds
    }
}

/// Source-level work counters used by the invariant harness; wall-clock targets are non-gating.
#[derive(Clone, Copy, Debug)]
pub struct ScanWork {
    pub accepted_bytes: usize,
    pub parsed_seals: usize,
    pub tail_parses: usize,
    pub tail_bytes: usize,
    pub tail_limit: usize,
}
