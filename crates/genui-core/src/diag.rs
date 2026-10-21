//! M0 lexical diagnostics. Codes and recoveries follow SPEC §4.3.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Code {
    #[serde(rename = "syntax.bad_utf8")]
    BadUtf8,
    #[serde(rename = "syntax.bad_char")]
    BadChar,
    #[serde(rename = "syntax.bad_escape")]
    BadEscape,
    #[serde(rename = "syntax.unterminated_string")]
    UnterminatedString,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Recovery {
    Skipped,
    Replaced,
    ClosedImplicitly,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Severity {
    Warning,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DiagPhase {
    Syntax,
}

/// Byte offsets refer to the normalized UTF-8 source; lines and columns are one-based.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub code: Code,
    pub severity: Severity,
    pub phase: DiagPhase,
    pub stmt: Option<String>,
    pub path: Option<String>,
    pub node: Option<String>,
    pub span: Option<Span>,
    pub expected: Option<String>,
    pub found: Option<String>,
    pub recovery: Recovery,
    pub message: String,
    pub hint: Option<String>,
    pub provisional: bool,
}

impl Diagnostic {
    pub(crate) fn lexical(code: Code, span: Option<Span>) -> Self {
        let (recovery, message) = match code {
            Code::BadUtf8 => (
                Recovery::Replaced,
                "Invalid UTF-8 was replaced with U+FFFD.",
            ),
            Code::BadChar => (
                Recovery::Skipped,
                "An unknown source character was skipped.",
            ),
            Code::BadEscape => (Recovery::Replaced, "An invalid string escape was replaced."),
            Code::UnterminatedString => (
                Recovery::ClosedImplicitly,
                "A string was closed at the line or input boundary.",
            ),
        };
        Self {
            code,
            severity: Severity::Warning,
            phase: DiagPhase::Syntax,
            stmt: None,
            path: None,
            node: None,
            span,
            expected: None,
            found: None,
            recovery,
            message: message.into(),
            hint: None,
            provisional: false,
        }
    }
}
