//! M0 lexical bootstrap (SPEC §2.2). Parsing and literal range checks belong to M1.
use crate::diag::{Code, Diagnostic, Span};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TokenKind {
    Name,
    State,
    Int,
    Float,
    String,
    True,
    False,
    Null,
    Nl,
    Assign,
    Bind,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    And,
    Or,
    Not,
    Pipe,
    Colon,
    Dot,
    Comma,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
}

/// Strings are decoded, state names omit `$`, and other tokens retain their spelling.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LexResult {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

/// A finish-only byte accumulator for M0's chunking tests, not a streaming parser.
#[derive(Default)]
pub struct LexBuffer {
    bytes: Vec<u8>,
}

impl LexBuffer {
    pub fn push(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    pub fn finish(self) -> LexResult {
        lex(&self.bytes)
    }
}

/// Normalize once, then tokenize without failing on model input.
pub fn lex(bytes: &[u8]) -> LexResult {
    let mut diagnostics = Vec::new();
    if std::str::from_utf8(bytes).is_err() {
        diagnostics.push(Diagnostic::lexical(Code::BadUtf8, None));
    }
    let text = String::from_utf8_lossy(bytes);
    let source = text
        .strip_prefix('\u{feff}')
        .unwrap_or(&text)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    Lexer {
        source: &source,
        pos: 0,
        line: 1,
        column: 1,
        tokens: Vec::new(),
        diagnostics,
    }
    .run()
}

struct Lexer<'a> {
    source: &'a str,
    pos: usize,
    line: usize,
    column: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

impl Lexer<'_> {
    fn peek(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(ch)
    }

    fn emit(&mut self, kind: TokenKind, text: String) {
        self.tokens.push(Token { kind, text });
    }

    fn mark(&self) -> Span {
        Span {
            start: self.pos,
            end: self.pos,
            line: self.line,
            column: self.column,
        }
    }

    fn diagnose(&mut self, code: Code, mut span: Span) {
        span.end = self.pos;
        self.diagnostics.push(Diagnostic::lexical(code, Some(span)));
    }

    fn run(mut self) -> LexResult {
        while let Some(ch) = self.peek() {
            let start = self.pos;
            match ch {
                ' ' | '\t' => {
                    self.bump();
                }
                '\n' => {
                    self.bump();
                    self.emit(TokenKind::Nl, "\n".into());
                }
                '/' if self.source[start..].starts_with("//") => self.comment(),
                '#' if self.column == 1 => self.comment(),
                '"' => self.string(),
                '$' => {
                    let span = self.mark();
                    self.bump();
                    if self
                        .peek()
                        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
                    {
                        let name = self.pos;
                        self.name_tail();
                        self.emit(TokenKind::State, self.source[name..self.pos].into());
                    } else {
                        self.diagnose(Code::BadChar, span);
                    }
                }
                c if c.is_ascii_alphabetic() || c == '_' => {
                    self.name_tail();
                    let text = &self.source[start..self.pos];
                    let kind = match text {
                        "true" => TokenKind::True,
                        "false" => TokenKind::False,
                        "null" => TokenKind::Null,
                        _ => TokenKind::Name,
                    };
                    self.emit(kind, text.into());
                }
                c if c.is_ascii_digit() => self.number(),
                _ => self.symbol(),
            }
        }
        LexResult {
            tokens: self.tokens,
            diagnostics: self.diagnostics,
        }
    }

    fn comment(&mut self) {
        while self.peek().is_some_and(|c| c != '\n') {
            self.bump();
        }
    }

    fn name_tail(&mut self) {
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            self.bump();
        }
    }

    fn number(&mut self) {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '_') {
            self.bump();
        }
        let mut kind = TokenKind::Int;
        let tail = self.source.as_bytes();
        if tail.get(self.pos) == Some(&b'.')
            && tail.get(self.pos + 1).is_some_and(u8::is_ascii_digit)
        {
            kind = TokenKind::Float;
            self.bump();
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.bump();
            }
        }
        if matches!(tail.get(self.pos), Some(b'e' | b'E')) {
            let mut end = self.pos + 1;
            if matches!(tail.get(end), Some(b'+' | b'-')) {
                end += 1;
            }
            if tail.get(end).is_some_and(u8::is_ascii_digit) {
                kind = TokenKind::Float;
                while self.pos < end {
                    self.bump();
                }
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.bump();
                }
            }
        }
        self.emit(kind, self.source[start..self.pos].into());
    }

    fn string(&mut self) {
        let start = self.mark();
        self.bump();
        let mut value = String::new();
        loop {
            match self.peek() {
                Some('"') => {
                    self.bump();
                    break;
                }
                None | Some('\n') => {
                    self.diagnose(Code::UnterminatedString, start);
                    break;
                }
                Some('\\') => {
                    let escape = self.mark();
                    self.bump();
                    match self.peek() {
                        None | Some('\n') => {
                            self.diagnose(Code::UnterminatedString, start);
                            break;
                        }
                        Some(c) => {
                            self.bump();
                            match c {
                                '"' | '\\' => value.push(c),
                                'n' => value.push('\n'),
                                't' => value.push('\t'),
                                'r' => value.push('\r'),
                                'u' if self.peek() == Some('{') => {
                                    self.bump();
                                    let hex_start = self.pos;
                                    while self
                                        .peek()
                                        .is_some_and(|c| c != '}' && c != '"' && c != '\n')
                                    {
                                        self.bump();
                                    }
                                    let digits = &self.source[hex_start..self.pos];
                                    let closed = self.peek() == Some('}');
                                    let decoded = if closed
                                        && (1..=6).contains(&digits.len())
                                        && digits.bytes().all(|c| c.is_ascii_hexdigit())
                                    {
                                        u32::from_str_radix(digits, 16)
                                            .ok()
                                            .and_then(char::from_u32)
                                    } else {
                                        None
                                    };
                                    if closed {
                                        self.bump();
                                    }
                                    if let Some(ch) = decoded {
                                        value.push(ch);
                                    } else {
                                        value.push('\u{fffd}');
                                        self.diagnose(Code::BadEscape, escape);
                                    }
                                }
                                _ => {
                                    value.push(c);
                                    self.diagnose(Code::BadEscape, escape);
                                }
                            }
                        }
                    }
                }
                Some(c) => {
                    self.bump();
                    value.push(c);
                }
            }
        }
        self.emit(TokenKind::String, value);
    }

    fn symbol(&mut self) {
        use TokenKind::*;
        const SYMBOLS: &[(&str, TokenKind)] = &[
            (":=", Bind),
            ("==", Eq),
            ("!=", Ne),
            ("<=", Le),
            (">=", Ge),
            ("&&", And),
            ("||", Or),
            ("=", Assign),
            ("<", Lt),
            (">", Gt),
            ("+", Plus),
            ("-", Minus),
            ("*", Star),
            ("/", Slash),
            ("%", Percent),
            ("!", Not),
            ("|", Pipe),
            (":", Colon),
            (".", Dot),
            (",", Comma),
            ("(", LParen),
            (")", RParen),
            ("[", LBracket),
            ("]", RBracket),
            ("{", LBrace),
            ("}", RBrace),
        ];
        for &(text, kind) in SYMBOLS {
            if self.source[self.pos..].starts_with(text) {
                for _ in text.chars() {
                    self.bump();
                }
                self.emit(kind, text.into());
                return;
            }
        }
        let span = self.mark();
        self.bump();
        self.diagnose(Code::BadChar, span);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_codes_have_normative_severity_and_recovery() {
        let result = lex(b"\xff @ \"\\x\n");
        let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(
            codes,
            [
                Code::BadUtf8,
                Code::BadChar,
                Code::BadChar,
                Code::BadEscape,
                Code::UnterminatedString
            ]
        );
        assert!(result.diagnostics.iter().all(|d| !d.provisional));
    }

    #[test]
    fn tab_counts_as_one_column_and_newlines_reset_it() {
        let result = lex("\t@\r\n\t😀".as_bytes());
        let spans: Vec<_> = result
            .diagnostics
            .iter()
            .filter_map(|d| d.span.as_ref())
            .map(|s| (s.line, s.column))
            .collect();
        assert_eq!(spans, [(1, 2), (2, 2)]);
    }
}
