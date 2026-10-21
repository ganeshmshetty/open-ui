//! Resumable line-based model-message extraction (SPEC §§2.8–2.9).
use super::{LocatedToken, lex, source::Source, splitter::statement_start};
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

pub struct Extractor {
    source: Source,
    line: String,
    mode: Mode,
    seen: bool,
    result: ExtractResult,
}
impl Default for Extractor {
    fn default() -> Self {
        Self {
            source: Source::default(),
            line: String::new(),
            mode: Mode::Prose,
            seen: false,
            result: ExtractResult::default(),
        }
    }
}
impl Extractor {
    pub fn push(&mut self, bytes: &[u8]) {
        let text = self.source.push(bytes, false);
        self.scan(&text);
    }
    pub fn segments(&self) -> &[Segment] {
        &self.result.segments
    }
    pub fn finish(mut self) -> ExtractResult {
        let text = self.source.push(&[], true);
        self.scan(&text);
        if !self.line.is_empty() {
            let line = std::mem::take(&mut self.line);
            self.line(&line);
        }
        if self.source.bad_utf8 {
            self.result
                .diagnostics
                .push(Diagnostic::lexical(Code::BadUtf8, None));
        }
        order_diagnostics(&mut self.result.diagnostics);
        self.result
    }
    fn scan(&mut self, text: &str) {
        for ch in text.chars() {
            self.line.push(ch);
            if ch == '\n' {
                let line = std::mem::take(&mut self.line);
                self.line(&line);
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
    fn program(&mut self, text: &str) {
        if let Some(Segment::Program(value)) = self.result.segments.last_mut() {
            value.push_str(text);
        }
    }
    fn line(&mut self, line: &str) {
        match self.mode {
            Mode::Program(fence) => {
                if closes(line, fence) {
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
                            self.result.diagnostics.push(Diagnostic::new(
                                Code::ExtraProgram,
                                None,
                                None,
                            ));
                            self.markdown(line);
                            self.mode = Mode::ProseFence(fence);
                        } else {
                            self.seen = true;
                            if words
                                .next()
                                .is_some_and(|word| word.starts_with('v') && word != "v1")
                            {
                                self.result.rejected = true;
                                self.result.diagnostics.push(Diagnostic::new(
                                    Code::Version,
                                    None,
                                    None,
                                ));
                            }
                            self.result.segments.push(Segment::Program(String::new()));
                            self.mode = Mode::Program(fence);
                        }
                    } else {
                        self.markdown(line);
                        self.mode = Mode::ProseFence(fence);
                    }
                } else if !self.seen && starts_program(line) {
                    self.seen = true;
                    self.result.segments.push(Segment::Program(line.into()));
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
