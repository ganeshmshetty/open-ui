//! Resumable line-based model-message extraction (SPEC §§2.8–2.9).
use super::{
    LexEvent, Lexer, LocatedToken, TokenKind as K, lex,
    source::Source,
    splitter::{lower_name, statement_start},
};
use crate::diag::{Code, Diagnostic, order_diagnostics};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "text", deny_unknown_fields)]
pub enum Segment {
    Markdown(String),
    Program(String),
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExtractResult {
    pub segments: Vec<Segment>,
    pub diagnostics: Vec<Diagnostic>,
    pub rejected: bool,
    pub program_raw: Vec<u8>,
}

#[derive(Clone, Copy)]
struct Fence {
    marker: u8,
    width: usize,
}
enum Mode {
    Prose,
    ProseFence(Fence),
    Program(Fence),
    Unfenced,
}

#[derive(Clone, Copy, Default)]
enum Route {
    #[default]
    Hold,
    Program,
    Markdown,
}

pub struct Extractor {
    route: Route,
    probe: Lexer,
    prefix: Vec<LocatedToken>,
    raw: Vec<u8>,
    raw_base: usize,
    raw_line_start: usize,
    raw_end: usize,
    program_cursor: Option<usize>,
    program_ended: bool,
    source: Source,
    line: String,
    mode: Mode,
    seen: bool,
    result: ExtractResult,
    diagnostics: crate::Diagnostics,
}
impl Default for Extractor {
    fn default() -> Self {
        Self {
            route: Route::Hold,
            probe: Lexer::default(),
            prefix: Vec::new(),
            raw: Vec::new(),
            raw_base: 0,
            raw_line_start: 0,
            raw_end: 0,
            program_cursor: None,
            program_ended: false,
            source: Source::tracked(),
            line: String::new(),
            mode: Mode::Prose,
            seen: false,
            result: ExtractResult::default(),
            diagnostics: Default::default(),
        }
    }
}
impl Extractor {
    pub fn push(&mut self, bytes: &[u8]) {
        self.raw.extend_from_slice(bytes);
        let text = self.source.push(bytes, false);
        let ends = self.source.take_raw_ends();
        self.scan(&text, &ends);
        self.compact_raw();
    }
    pub fn program_raw(&self) -> &[u8] {
        &self.result.program_raw
    }
    pub fn program_ended(&self) -> bool {
        self.program_ended
    }
    pub fn rejected(&self) -> bool {
        self.result.rejected
    }
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    pub fn segments(&self) -> &[Segment] {
        &self.result.segments
    }
    pub(crate) fn diagnostic_snapshot(&self) -> crate::Diagnostics {
        self.diagnostics.clone()
    }
    pub fn finish(self) -> ExtractResult {
        let (mut result, diagnostics) = self.finish_stream();
        result.diagnostics = diagnostics.to_vec();
        order_diagnostics(&mut result.diagnostics);
        result
    }
    pub(crate) fn finish_stream(mut self) -> (ExtractResult, crate::Diagnostics) {
        let text = self.source.push(&[], true);
        let ends = self.source.take_raw_ends();
        self.scan(&text, &ends);
        if !self.line.is_empty() {
            let line = std::mem::take(&mut self.line);
            self.line(&line);
        }
        if self.source.bad_utf8 {
            self.diagnostics
                .extend_raw([Diagnostic::lexical(Code::BadUtf8, None)]);
        }
        (self.result, self.diagnostics)
    }
    fn scan(&mut self, text: &str, ends: &[usize]) {
        for (ch, end) in text.chars().zip(ends.iter().copied()) {
            self.raw_end = end;
            match self.route {
                Route::Program => self.program(ch.encode_utf8(&mut [0; 4])),
                Route::Markdown => self.markdown(ch.encode_utf8(&mut [0; 4])),
                Route::Hold => {
                    self.line.push(ch);
                    if ch != '\n' {
                        match self.mode {
                            Mode::Unfenced => self.route = Route::Program,
                            Mode::Program(fence)
                                if self.line.as_bytes().first() != Some(&fence.marker) =>
                            {
                                self.route = Route::Program
                            }
                            Mode::Prose if !self.seen => {
                                if !self.line.as_bytes().first().is_some_and(|b| {
                                    b.is_ascii_lowercase()
                                        || matches!(*b, b'_' | b'$' | b'`' | b'~')
                                }) {
                                    self.route = Route::Markdown;
                                } else if !self.line.starts_with(['`', '~']) {
                                    self.probe.push(ch.encode_utf8(&mut [0; 4]).as_bytes());
                                    self.prefix.extend(
                                        self.probe.take_events().into_iter().filter_map(|e| {
                                            if let LexEvent::Token(t) = e {
                                                Some(t)
                                            } else {
                                                None
                                            }
                                        }),
                                    );
                                    if statement_start(&self.prefix).is_some() {
                                        self.seen = true;
                                        self.program_cursor = Some(self.raw_line_start);
                                        self.result.segments.push(Segment::Program(String::new()));
                                        self.mode = Mode::Unfenced;
                                        self.route = Route::Program;
                                    } else if self.prefix.len() >= 3
                                        || self.prefix.first().is_some_and(|t| {
                                            !lower_name(&t.token) && t.token.kind != K::State
                                        })
                                        || (self.prefix.len() == 2
                                            && self
                                                .prefix
                                                .first()
                                                .is_some_and(|t| t.token.text != "query"))
                                    {
                                        self.route = Route::Markdown;
                                    }
                                }
                            }
                            _ => {}
                        }
                        match self.route {
                            Route::Program => {
                                let line = std::mem::take(&mut self.line);
                                self.program(&line);
                            }
                            Route::Markdown => {
                                let line = std::mem::take(&mut self.line);
                                self.markdown(&line);
                            }
                            Route::Hold => {}
                        }
                    }
                }
            }
            if ch == '\n' {
                if matches!(self.route, Route::Hold) {
                    let line = std::mem::take(&mut self.line);
                    self.line(&line);
                }
                self.raw_line_start = end;
                self.route = Route::Hold;
                self.probe = Lexer::default();
                self.prefix.clear();
            }
        }
    }
    fn markdown(&mut self, text: &str) {
        if let Some(Segment::Markdown(value)) = self.result.segments.last_mut() {
            value.push_str(text);
        } else {
            self.result.segments.push(Segment::Markdown(text.into()));
        }
    }
    fn compact_raw(&mut self) {
        let count = self
            .raw_line_start
            .saturating_sub(self.raw_base)
            .min(self.raw.len());
        if count > 0 {
            self.raw.drain(..count);
        }
        self.raw_base += count;
    }
    fn program(&mut self, text: &str) {
        if let Some(start) = self.program_cursor {
            if let Some(bytes) = self.raw.get(
                start.saturating_sub(self.raw_base)..self.raw_end.saturating_sub(self.raw_base),
            ) {
                self.result.program_raw.extend_from_slice(bytes);
            }
            self.program_cursor = Some(self.raw_end);
        }
        if let Some(Segment::Program(value)) = self.result.segments.last_mut() {
            value.push_str(text);
        }
    }
    fn line(&mut self, line: &str) {
        match self.mode {
            Mode::Program(fence) => {
                if closes(line, fence) {
                    self.program_cursor = None;
                    self.program_ended = true;
                    self.mode = Mode::Prose;
                } else {
                    self.program(line);
                }
            }
            Mode::Unfenced => self.program(line),
            Mode::ProseFence(fence) => {
                self.markdown(line);
                if closes(line, fence) {
                    self.mode = Mode::Prose;
                }
            }
            Mode::Prose => {
                if let Some((fence, info)) = fence(line) {
                    let mut words = info.split_whitespace();
                    if words.next() == Some("ui") {
                        if self.seen {
                            self.diagnostics.extend_raw([Diagnostic::new(
                                Code::ExtraProgram,
                                None,
                                None,
                            )]);
                            self.markdown(line);
                            self.mode = Mode::ProseFence(fence);
                        } else {
                            self.seen = true;
                            if words
                                .next()
                                .is_some_and(|word| word.starts_with('v') && word != "v1")
                            {
                                self.result.rejected = true;
                                self.diagnostics.extend_raw([Diagnostic::new(
                                    Code::Version,
                                    None,
                                    None,
                                )]);
                            }
                            self.result.segments.push(Segment::Program(String::new()));
                            self.program_cursor = Some(self.raw_end);
                            self.mode = Mode::Program(fence);
                        }
                    } else {
                        self.markdown(line);
                        self.mode = Mode::ProseFence(fence);
                    }
                } else if !self.seen && starts_program(line) {
                    self.seen = true;
                    self.result.segments.push(Segment::Program(String::new()));
                    self.program_cursor = Some(self.raw_line_start);
                    self.program(line);
                    self.mode = Mode::Unfenced;
                } else {
                    self.markdown(line);
                }
            }
        }
    }
}
pub fn extract(bytes: &[u8]) -> ExtractResult {
    let mut extractor = Extractor::default();
    extractor.push(bytes);
    extractor.finish()
}

fn starts_program(line: &str) -> bool {
    if !line
        .as_bytes()
        .first()
        .is_some_and(|b| b.is_ascii_lowercase() || matches!(*b, b'_' | b'$'))
    {
        return false;
    }
    let result = lex(line.as_bytes());
    let tokens: Vec<_> = result
        .tokens
        .into_iter()
        .zip(result.spans)
        .map(|(token, span)| LocatedToken { token, span })
        .collect();
    statement_start(&tokens).is_some()
}
fn fence(line: &str) -> Option<(Fence, &str)> {
    let marker = *line.as_bytes().first()?;
    if !matches!(marker, b'`' | b'~') {
        return None;
    }
    let width = line.bytes().take_while(|b| *b == marker).count();
    if width < 3 {
        return None;
    }
    let info = line[width..].trim();
    if marker == b'`' && info.contains('`') {
        return None;
    }
    Some((Fence { marker, width }, info))
}
fn closes(line: &str, expected: Fence) -> bool {
    fence(line).is_some_and(|(actual, info)| {
        actual.marker == expected.marker && actual.width >= expected.width && info.is_empty()
    })
}
