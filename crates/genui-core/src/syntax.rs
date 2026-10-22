//! Resumable syntax pipeline (SPEC §§2–3). Batch uses the same scanners as push.
pub mod ast;
pub mod extract;
mod lexer;
pub mod parser;
pub mod printer;
#[cfg(test)]
mod properties;
#[cfg(test)]
mod quality_tests;
mod source;
pub mod splitter;
pub mod streaming;

use crate::diag::{Diagnostic, Span};
pub use lexer::{LexBuffer, Lexer, lex};
pub use parser::{BatchParser, ParseResult, parse_batch, parse_message};
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
    pub spans: Vec<Span>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocatedToken {
    pub token: Token,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LexEvent {
    Token(LocatedToken),
    Diagnostic(Diagnostic),
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Code;

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
