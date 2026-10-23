use super::{LexEvent, LexResult, LocatedToken, Token, TokenKind as K, source::Source};
use crate::diag::{Code, Diagnostic, Span, order_diagnostics};

#[derive(Default)]
enum Mode {
    #[default]
    Ground,
    Word(String, bool),
    Number(String, NumberStage),
    Symbol(char),
    Dollar,
    Comment,
    String(String, Escape),
}
#[derive(Clone, Copy)]
enum NumberStage {
    Int,
    Dot,
    Fraction,
    E,
    Sign,
    Exponent,
}
enum Escape {
    Plain,
    Slash,
    U,
    Hex(String),
}

/// Resumable scanner: tokens, escapes, UTF-8 suffixes and CR survive push boundaries.
/// `take_events` exposes only complete tokens, so growing tokens are never sealed.
pub struct Lexer {
    source: Source,
    normalized: String,
    mode: Mode,
    start: Span,
    cursor: Span,
    escape_start: Span,
    events: Vec<LexEvent>,
    numeric_limit: Option<usize>,
}
pub type LexBuffer = Lexer;

impl Default for Lexer {
    fn default() -> Self {
        let cursor = Span {
            start: 0,
            end: 0,
            line: 1,
            column: 1,
        };
        Self {
            source: Source::default(),
            normalized: String::new(),
            mode: Mode::Ground,
            start: cursor.clone(),
            escape_start: cursor.clone(),
            cursor,
            events: Vec::new(),
            numeric_limit: None,
        }
    }
}

impl Lexer {
    pub(crate) fn with_numeric_limit(limit: usize) -> Self {
        Self {
            numeric_limit: Some(limit),
            ..Self::default()
        }
    }
    pub(crate) fn pending_token(&self) -> bool {
        !matches!(self.mode, Mode::Ground | Mode::Comment)
    }
    pub fn push(&mut self, bytes: &[u8]) {
        let text = self.source.push(bytes, false);
        self.scan(&text);
    }

    pub fn normalized_source(&self) -> &str {
        &self.normalized
    }

    /// A display-only token from the uncommitted lexical state. Growing words and
    /// operator prefixes are deliberately absent; incomplete escapes contribute nothing.
    pub(crate) fn pending_colon(&self) -> bool {
        matches!(self.mode, Mode::Symbol(':'))
    }
    pub(crate) fn provisional_events(&self) -> Vec<LexEvent> {
        let mut copy = Self {
            start: self.start.clone(),
            cursor: self.cursor.clone(),
            numeric_limit: self.numeric_limit,
            ..Self::default()
        };
        match &self.mode {
            Mode::String(value, _) => copy.emit(
                K::String,
                value.clone(),
                self.start.clone(),
                self.cursor.start,
            ),
            Mode::Number(text, stage) => copy.number(text.clone(), *stage),
            _ => {}
        }
        copy.events
            .into_iter()
            .filter(|e| {
                matches!(
                    e,
                    LexEvent::Diagnostic(_)
                        | LexEvent::Token(LocatedToken {
                            token: Token {
                                kind: K::Int | K::Float | K::Null | K::String,
                                ..
                            },
                            ..
                        })
                )
            })
            .collect()
    }

    pub(crate) fn take_events(&mut self) -> Vec<LexEvent> {
        std::mem::take(&mut self.events)
    }

    pub(crate) fn finish_events(mut self) -> (String, Vec<LexEvent>) {
        let text = self.source.push(&[], true);
        self.scan(&text);
        self.flush();
        if self.source.bad_utf8 {
            self.events.push(LexEvent::Diagnostic(Diagnostic::lexical(
                Code::BadUtf8,
                None,
            )));
        }
        (self.normalized, self.events)
    }

    pub fn finish(self) -> LexResult {
        let (_, events) = self.finish_events();
        let mut result = LexResult {
            tokens: Vec::new(),
            spans: Vec::new(),
            diagnostics: Vec::new(),
        };
        for event in events {
            match event {
                LexEvent::Token(t) => {
                    result.tokens.push(t.token);
                    result.spans.push(t.span);
                }
                LexEvent::Diagnostic(d) => result.diagnostics.push(d),
            }
        }
        order_diagnostics(&mut result.diagnostics);
        result
    }

    fn scan(&mut self, text: &str) {
        self.normalized.push_str(text);
        for ch in text.chars() {
            while !self.character(ch) {}
            self.cursor.start += ch.len_utf8();
            self.cursor.end = self.cursor.start;
            if ch == '\n' {
                self.cursor.line += 1;
                self.cursor.column = 1;
            } else {
                self.cursor.column += 1;
            }
        }
    }

    fn emit(&mut self, kind: K, text: String, start: Span, end: usize) {
        self.events.push(LexEvent::Token(LocatedToken {
            token: Token { kind, text },
            span: Span { end, ..start },
        }));
    }
    fn diagnose(&mut self, code: Code, start: Span, end: usize) {
        self.events.push(LexEvent::Diagnostic(Diagnostic::lexical(
            code,
            Some(Span { end, ..start }),
        )));
    }
    fn end(&self, ch: char) -> usize {
        self.cursor.start + ch.len_utf8()
    }

    // Returns false when the current character belongs to the next token.
    fn character(&mut self, ch: char) -> bool {
        let mode = std::mem::take(&mut self.mode);
        match mode {
            Mode::Ground => {
                self.start = self.cursor.clone();
                match ch {
                    ' ' | '\t' => {}
                    '\n' => self.emit(K::Nl, "\n".into(), self.start.clone(), self.end(ch)),
                    '#' if self.cursor.column == 1 => self.mode = Mode::Comment,
                    '"' => self.mode = Mode::String(String::new(), Escape::Plain),
                    '$' => self.mode = Mode::Dollar,
                    c if c.is_ascii_alphabetic() || c == '_' => {
                        self.mode = Mode::Word(c.into(), false)
                    }
                    c if c.is_ascii_digit() => self.mode = Mode::Number(c.into(), NumberStage::Int),
                    ':' | '=' | '!' | '<' | '>' | '&' | '|' | '/' => self.mode = Mode::Symbol(ch),
                    _ => {
                        if let Some(kind) = single(ch) {
                            self.emit(kind, ch.into(), self.start.clone(), self.end(ch));
                        } else {
                            self.diagnose(Code::BadChar, self.start.clone(), self.end(ch));
                        }
                    }
                }
                true
            }
            Mode::Word(mut word, state) => {
                if ch.is_ascii_alphanumeric() || ch == '_' {
                    word.push(ch);
                    self.mode = Mode::Word(word, state);
                    true
                } else {
                    self.word(word, state);
                    false
                }
            }
            Mode::Dollar => {
                if ch.is_ascii_lowercase() || ch == '_' {
                    self.mode = Mode::Word(ch.into(), true);
                    true
                } else {
                    self.diagnose(Code::BadChar, self.start.clone(), self.cursor.start);
                    false
                }
            }
            Mode::Comment => {
                if ch == '\n' {
                    false
                } else {
                    self.mode = Mode::Comment;
                    true
                }
            }
            Mode::Symbol(first) => {
                if first == '/' && ch == '/' {
                    self.mode = Mode::Comment;
                    return true;
                }
                let pair = match (first, ch) {
                    (':', '=') => Some(K::Bind),
                    ('=', '=') => Some(K::Eq),
                    ('!', '=') => Some(K::Ne),
                    ('<', '=') => Some(K::Le),
                    ('>', '=') => Some(K::Ge),
                    ('&', '&') => Some(K::And),
                    ('|', '|') => Some(K::Or),
                    _ => None,
                };
                if let Some(kind) = pair {
                    self.emit(
                        kind,
                        format!("{first}{ch}"),
                        self.start.clone(),
                        self.end(ch),
                    );
                    true
                } else {
                    self.symbol(first);
                    false
                }
            }
            Mode::Number(mut text, stage) => {
                use NumberStage::*;
                let next = match (stage, ch) {
                    (Int, '0'..='9' | '_') => Some(Int),
                    (Int, '.') => Some(Dot),
                    (Dot | Fraction, '0'..='9') => Some(Fraction),
                    (Int | Fraction, 'e' | 'E') => Some(E),
                    (E, '+' | '-') => Some(Sign),
                    (E | Sign | Exponent, '0'..='9') => Some(Exponent),
                    _ => None,
                };
                if let Some(stage) = next {
                    text.push(ch);
                    self.mode = Mode::Number(text, stage);
                    true
                } else {
                    self.number(text, stage);
                    false
                }
            }
            Mode::String(mut value, escape) => {
                match escape {
                    Escape::Plain => match ch {
                        '"' => self.emit(K::String, value, self.start.clone(), self.end(ch)),
                        '\n' => {
                            self.unterminated(value);
                            return false;
                        }
                        '\\' => {
                            self.escape_start = self.cursor.clone();
                            self.mode = Mode::String(value, Escape::Slash);
                        }
                        _ => {
                            value.push(ch);
                            self.mode = Mode::String(value, Escape::Plain);
                        }
                    },
                    Escape::Slash => {
                        if ch == '\n' {
                            self.unterminated(value);
                            return false;
                        }
                        match ch {
                            'u' => {
                                self.mode = Mode::String(value, Escape::U);
                                return true;
                            }
                            '"' | '\\' => value.push(ch),
                            'n' => value.push('\n'),
                            'r' => value.push('\r'),
                            't' => value.push('\t'),
                            _ => {
                                value.push(ch);
                                self.diagnose(
                                    Code::BadEscape,
                                    self.escape_start.clone(),
                                    self.end(ch),
                                );
                            }
                        }
                        self.mode = Mode::String(value, Escape::Plain);
                    }
                    Escape::U => {
                        if ch == '{' {
                            self.mode = Mode::String(value, Escape::Hex(String::new()));
                        } else {
                            value.push('u');
                            self.diagnose(
                                Code::BadEscape,
                                self.escape_start.clone(),
                                self.cursor.start,
                            );
                            self.mode = Mode::String(value, Escape::Plain);
                            return false;
                        }
                    }
                    Escape::Hex(mut digits) => {
                        if ch == '}' || ch == '"' || ch == '\n' {
                            let decoded = if ch == '}'
                                && (1..=6).contains(&digits.len())
                                && digits.bytes().all(|c| c.is_ascii_hexdigit())
                            {
                                u32::from_str_radix(&digits, 16)
                                    .ok()
                                    .and_then(char::from_u32)
                            } else {
                                None
                            };
                            if let Some(c) = decoded {
                                value.push(c);
                            } else {
                                value.push('\u{fffd}');
                                self.diagnose(
                                    Code::BadEscape,
                                    self.escape_start.clone(),
                                    if ch == '}' {
                                        self.end(ch)
                                    } else {
                                        self.cursor.start
                                    },
                                );
                            }
                            self.mode = Mode::String(value, Escape::Plain);
                            return ch == '}';
                        }
                        // Length above six is already invalid; retain a sentinel, not an unbounded escape body.
                        if digits.len() <= 6 {
                            digits.push(ch);
                        }
                        self.mode = Mode::String(value, Escape::Hex(digits));
                    }
                }
                true
            }
        }
    }

    fn unterminated(&mut self, value: String) {
        self.emit(K::String, value, self.start.clone(), self.cursor.start);
        self.diagnose(
            Code::UnterminatedString,
            self.start.clone(),
            self.cursor.start,
        );
    }

    fn word(&mut self, text: String, state: bool) {
        if text.len() > 64 {
            self.diagnose(Code::BadChar, self.start.clone(), self.cursor.start);
            return;
        }
        let kind = if state {
            K::State
        } else {
            match text.as_str() {
                "true" => K::True,
                "false" => K::False,
                "null" => K::Null,
                _ => K::Name,
            }
        };
        self.emit(kind, text, self.start.clone(), self.cursor.start);
    }

    fn number(&mut self, mut text: String, stage: NumberStage) {
        let suffix_len = match stage {
            NumberStage::Dot | NumberStage::E => 1,
            NumberStage::Sign => 2,
            _ => 0,
        };
        let suffix = text.split_off(text.len() - suffix_len);
        let end = self.cursor.start - suffix_len;
        // A token longer than the statement budget is necessarily discarded by
        // the splitter. Its conversion errors would be suppressed by that cutoff.
        if self.numeric_limit.is_some_and(|limit| text.len() > limit) {
            self.emit(K::Null, "null".into(), self.start.clone(), end);
        } else {
            let float = text.contains(['.', 'e', 'E']);
            let clean = text.replace('_', "");
            let error = if float {
                match clean.parse::<f64>() {
                    Ok(v) if v.is_finite() => None,
                    _ => Some(Code::NonFinite),
                }
            } else if clean.parse::<i64>().is_err() {
                Some(Code::Overflow)
            } else {
                None
            };
            if let Some(code) = error {
                self.diagnose(code, self.start.clone(), end);
                self.emit(K::Null, "null".into(), self.start.clone(), end);
            } else {
                self.emit(
                    if float { K::Float } else { K::Int },
                    text,
                    self.start.clone(),
                    end,
                );
            }
        }
        if !suffix.is_empty() {
            self.start = Span {
                start: end,
                end,
                column: self.cursor.column - suffix_len,
                line: self.cursor.line,
            };
            if suffix == "." {
                self.emit(K::Dot, suffix, self.start.clone(), self.cursor.start);
            } else if suffix_len == 1 {
                self.mode = Mode::Word(suffix, false);
            } else {
                self.emit(K::Name, suffix[..1].into(), self.start.clone(), end + 1);
                let sign = if suffix.ends_with('+') {
                    K::Plus
                } else {
                    K::Minus
                };
                let mark = Span {
                    start: end + 1,
                    end: end + 1,
                    column: self.start.column + 1,
                    line: self.start.line,
                };
                self.emit(sign, suffix[1..].into(), mark, self.cursor.start);
            }
        }
    }

    fn symbol(&mut self, ch: char) {
        if let Some(kind) = single(ch) {
            self.emit(kind, ch.into(), self.start.clone(), self.cursor.start);
        } else {
            self.diagnose(Code::BadChar, self.start.clone(), self.cursor.start);
        }
    }

    fn flush(&mut self) {
        match std::mem::take(&mut self.mode) {
            Mode::Word(text, state) => self.word(text, state),
            Mode::Number(text, stage) => {
                self.number(text, stage);
                if matches!(self.mode, Mode::Word(..)) {
                    self.flush();
                }
            }
            Mode::Symbol(ch) => self.symbol(ch),
            Mode::Dollar => self.diagnose(Code::BadChar, self.start.clone(), self.cursor.start),
            Mode::String(mut value, escape) => {
                match escape {
                    Escape::U => {
                        value.push('u');
                        self.diagnose(
                            Code::BadEscape,
                            self.escape_start.clone(),
                            self.cursor.start,
                        );
                    }
                    Escape::Hex(_) => {
                        value.push('\u{fffd}');
                        self.diagnose(
                            Code::BadEscape,
                            self.escape_start.clone(),
                            self.cursor.start,
                        );
                    }
                    _ => {}
                }
                self.unterminated(value);
            }
            _ => {}
        }
    }
}

fn single(ch: char) -> Option<K> {
    Some(match ch {
        '=' => K::Assign,
        '<' => K::Lt,
        '>' => K::Gt,
        '+' => K::Plus,
        '-' => K::Minus,
        '*' => K::Star,
        '/' => K::Slash,
        '%' => K::Percent,
        '!' => K::Not,
        '|' => K::Pipe,
        ':' => K::Colon,
        '.' => K::Dot,
        ',' => K::Comma,
        '(' => K::LParen,
        ')' => K::RParen,
        '[' => K::LBracket,
        ']' => K::RBracket,
        '{' => K::LBrace,
        '}' => K::RBrace,
        _ => return None,
    })
}

pub fn lex(bytes: &[u8]) -> LexResult {
    let mut lexer = Lexer::default();
    lexer.push(bytes);
    lexer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_token_ignores_ground_and_comment_states() {
        let mut lexer = Lexer::default();
        assert!(!lexer.pending_token());
        lexer.push(b"le");
        assert!(lexer.pending_token(), "a growing word is a pending token");
        lexer.push(b"af ");
        assert!(!lexer.pending_token());
        lexer.push(b"\n# note");
        assert!(!lexer.pending_token(), "comment text is not a token");
        lexer.push(b"\n1");
        assert!(lexer.pending_token(), "a growing number is a pending token");
    }

    #[test]
    fn repaired_number_suffix_sign_span_starts_after_the_name() {
        let result = lex(b"1e+");
        let index = result
            .tokens
            .iter()
            .position(|token| token.kind == K::Plus)
            .expect("repaired sign token");
        assert_eq!(
            result.spans[index],
            Span {
                start: 2,
                end: 3,
                column: 3,
                line: 1
            },
            "{:?}",
            result.spans
        );
    }

    #[test]
    fn provisional_numbers_keep_the_configured_numeric_limit() {
        let mut lexer = Lexer::with_numeric_limit(3);
        lexer.push(b"1234");
        let events = lexer.provisional_events();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, LexEvent::Token(token)
                if token.token.kind == K::Null && token.token.text == "null")),
            "{events:?}"
        );
    }
}
