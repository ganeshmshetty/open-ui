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
    #[serde(rename = "syntax.unclosed_bracket")]
    UnclosedBracket,
    #[serde(rename = "syntax.trailing_operator")]
    TrailingOperator,
    #[serde(rename = "syntax.stray_closer")]
    StrayCloser,
    #[serde(rename = "syntax.missing_comma")]
    MissingComma,
    #[serde(rename = "syntax.trailing")]
    Trailing,
    #[serde(rename = "syntax.stmt_shape")]
    StmtShape,
    #[serde(rename = "syntax.bare_component")]
    BareComponent,
    #[serde(rename = "syntax.dup_key")]
    DupKey,
    #[serde(rename = "name.duplicate")]
    DuplicateName,
    #[serde(rename = "name.reserved")]
    ReservedName,
    #[serde(rename = "extract.version")]
    Version,
    #[serde(rename = "extract.extra_program")]
    ExtraProgram,
    #[serde(rename = "expr.overflow")]
    Overflow,
    #[serde(rename = "expr.non_finite")]
    NonFinite,
    #[serde(rename = "expr.lambda_misuse")]
    LambdaMisuse,
    #[serde(rename = "expr.unknown_fn")]
    UnknownFn,
    #[serde(rename = "schema.positional_after_named")]
    PositionalAfterNamed,
    #[serde(rename = "limit.stmt_size")]
    StmtSize,
    #[serde(rename = "limit.program_size")]
    ProgramSize,
    #[serde(rename = "limit.stmt_count")]
    StmtCount,
    #[serde(rename = "limit.depth")]
    Depth,
    #[serde(rename = "limit.string")]
    StringSize,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Recovery {
    Skipped,
    Replaced,
    ClosedImplicitly,
    CommaInserted,
    Truncated,
    Ignored,
    ValueNulled,
    ArgOmitted,
    StatementDropped,
    ProgramRejected,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Fatal,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DiagPhase {
    Syntax,
    Name,
    Extract,
    Expr,
    Schema,
    Limit,
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
        Self::new(code, None, span)
    }

    pub(crate) fn new(code: Code, stmt: Option<String>, span: Option<Span>) -> Self {
        use Code::*;
        let (severity, phase, recovery) = match code {
            BadUtf8 | BadEscape | BareComponent => {
                (Severity::Warning, DiagPhase::Syntax, Recovery::Replaced)
            }
            BadChar => (Severity::Warning, DiagPhase::Syntax, Recovery::Skipped),
            UnterminatedString => (
                Severity::Warning,
                DiagPhase::Syntax,
                Recovery::ClosedImplicitly,
            ),
            UnclosedBracket | TrailingOperator => (
                Severity::Error,
                DiagPhase::Syntax,
                Recovery::ClosedImplicitly,
            ),
            StrayCloser | DupKey => (Severity::Warning, DiagPhase::Syntax, Recovery::Ignored),
            MissingComma => (
                Severity::Warning,
                DiagPhase::Syntax,
                Recovery::CommaInserted,
            ),
            Trailing => (Severity::Warning, DiagPhase::Syntax, Recovery::Truncated),
            StmtShape => (
                Severity::Error,
                DiagPhase::Syntax,
                Recovery::StatementDropped,
            ),
            DuplicateName | ReservedName => {
                (Severity::Error, DiagPhase::Name, Recovery::StatementDropped)
            }
            Version => (
                Severity::Fatal,
                DiagPhase::Extract,
                Recovery::ProgramRejected,
            ),
            ExtraProgram => (Severity::Info, DiagPhase::Extract, Recovery::Ignored),
            Overflow | NonFinite | LambdaMisuse | UnknownFn => {
                (Severity::Error, DiagPhase::Expr, Recovery::ValueNulled)
            }
            PositionalAfterNamed => (Severity::Warning, DiagPhase::Schema, Recovery::ArgOmitted),
            StmtSize | ProgramSize | Depth | StringSize => {
                (Severity::Error, DiagPhase::Limit, Recovery::Truncated)
            }
            StmtCount => (
                Severity::Error,
                DiagPhase::Limit,
                Recovery::StatementDropped,
            ),
        };
        Self {
            code,
            severity,
            phase,
            stmt,
            path: None,
            node: None,
            span,
            expected: None,
            found: None,
            recovery,
            message: match code {
                Code::BadUtf8 => "Invalid UTF-8 was replaced with U+FFFD.",
                Code::BadChar => "Invalid source text was skipped.",
                Code::BadEscape => "An invalid string escape was replaced.",
                Code::UnterminatedString => "A string was closed at the line or input boundary.",
                Code::UnclosedBracket => "An open bracket was closed implicitly.",
                Code::TrailingOperator => "A dangling operator suffix was removed.",
                Code::StrayCloser => "An unmatched closing bracket was ignored.",
                Code::MissingComma => "A comma was inserted between adjacent elements.",
                Code::Trailing => "Tokens after a complete expression were discarded.",
                Code::StmtShape => "A statement with invalid syntax was dropped.",
                Code::BareComponent => "A bare component was replaced with a zero-argument call.",
                Code::DupKey => "A repeated map key was ignored.",
                Code::DuplicateName => "A repeated statement definition was dropped.",
                Code::ReservedName => "A statement using a reserved name was dropped.",
                Code::Version => "A program with an unsupported or malformed version was rejected.",
                Code::ExtraProgram => "A later program was retained as a prose code block.",
                Code::Overflow => "An integer literal outside the i64 range became null.",
                Code::NonFinite => "A non-finite float literal became null.",
                Code::LambdaMisuse => "A lambda outside its allowed argument context became null.",
                Code::UnknownFn => "A tool call outside a query declaration became null.",
                Code::PositionalAfterNamed => {
                    "A positional argument after a named argument was omitted."
                }
                Code::StmtSize => "An oversized statement was truncated.",
                Code::ProgramSize => "An oversized program was truncated.",
                Code::StmtCount => "A statement beyond the statement-count limit was dropped.",
                Code::Depth => "Syntax beyond the depth limit was truncated.",
                Code::StringSize => "An oversized string was truncated at a UTF-8 boundary.",
            }
            .into(),
            hint: if code == Code::UnknownFn {
                Some("Use query name = tool(\"tool.id\", args, default: value).".into())
            } else {
                None
            },
            provisional: false,
        }
    }
}

/// Deterministic source order, independent of when held-back bytes were resolved.
pub(crate) fn order_diagnostics(diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_by_key(|d| (d.span.as_ref().map_or(0, |s| s.end), d.code));
}
