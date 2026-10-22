//! Stable diagnostic catalog and recoveries (SPEC §4.3).
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Code {
    #[serde(rename = "name.unresolved")]
    Unresolved,
    #[serde(rename = "name.cycle")]
    Cycle,
    #[serde(rename = "name.no_root")]
    NoRoot,
    #[serde(rename = "name.orphan")]
    Orphan,
    #[serde(rename = "schema.unknown_component")]
    UnknownComponent,
    #[serde(rename = "schema.unknown_arg")]
    UnknownArg,
    #[serde(rename = "schema.missing_required")]
    MissingRequired,
    #[serde(rename = "schema.extra_positional")]
    ExtraPositional,
    #[serde(rename = "schema.dup_arg")]
    DuplicateArg,
    #[serde(rename = "schema.required_by_name")]
    RequiredByName,
    #[serde(rename = "schema.type")]
    SchemaType,
    #[serde(rename = "schema.enum")]
    Enum,
    #[serde(rename = "schema.range")]
    Range,
    #[serde(rename = "schema.slot_kind")]
    SlotKind,
    #[serde(rename = "schema.min_items")]
    MinItems,
    #[serde(rename = "schema.bind_target")]
    BindTarget,
    #[serde(rename = "schema.dup_key")]
    DuplicateKey,
    #[serde(rename = "schema.bad_key")]
    BadKey,
    #[serde(rename = "schema.a11y_missing")]
    A11yMissing,
    #[serde(rename = "expr.type")]
    ExprType,
    #[serde(rename = "expr.div_zero")]
    DivZero,
    #[serde(rename = "expr.index_oob")]
    IndexOob,
    #[serde(rename = "expr.fmt_args")]
    FmtArgs,
    #[serde(rename = "expr.tainted_url")]
    TaintedUrl,
    #[serde(rename = "state.undeclared")]
    Undeclared,
    #[serde(rename = "state.const_default")]
    ConstDefault,
    #[serde(rename = "state.type_write")]
    TypeWrite,
    #[serde(rename = "query.unknown_tool")]
    UnknownTool,
    #[serde(rename = "query.too_many")]
    TooManyQueries,
    #[serde(rename = "query.denied")]
    QueryDenied,
    #[serde(rename = "query.failed")]
    QueryFailed,
    #[serde(rename = "ref.invalid")]
    RefInvalid,
    #[serde(rename = "ref.unknown")]
    RefUnknown,
    #[serde(rename = "ref.too_large")]
    RefTooLarge,
    #[serde(rename = "limit.ir_depth")]
    IrDepth,
    #[serde(rename = "limit.nodes")]
    Nodes,
    #[serde(rename = "limit.fuel")]
    Fuel,
    #[serde(rename = "limit.list_len")]
    ListLen,
    #[serde(rename = "limit.state")]
    StateSize,
    #[serde(rename = "limit.event")]
    EventSize,
    #[serde(rename = "render.panic")]
    RenderPanic,
    #[serde(rename = "cap.fallback")]
    CapFallback,
    #[serde(rename = "lib.mismatch")]
    LibMismatch,

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
    None,
    ChildOmitted,
    NodeDropped,
    Defaulted,
    Clamped,
    BindingDropped,
    KeptLastGood,
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
    State,
    Query,
    Ref,
    Render,
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

    pub fn new(code: Code, stmt: Option<String>, span: Option<Span>) -> Self {
        use Code::*;
        let (severity, phase, recovery) = match code {
            Unresolved => (Severity::Warning, DiagPhase::Name, Recovery::ValueNulled),
            Cycle => (Severity::Error, DiagPhase::Name, Recovery::ChildOmitted),
            NoRoot => (Severity::Fatal, DiagPhase::Name, Recovery::ProgramRejected),
            Orphan => (Severity::Info, DiagPhase::Name, Recovery::None),
            UnknownComponent => (Severity::Error, DiagPhase::Schema, Recovery::NodeDropped),
            UnknownArg => (Severity::Warning, DiagPhase::Schema, Recovery::ArgOmitted),
            MissingRequired => (Severity::Error, DiagPhase::Schema, Recovery::NodeDropped),
            ExtraPositional => (Severity::Warning, DiagPhase::Schema, Recovery::ArgOmitted),
            DuplicateArg => (Severity::Warning, DiagPhase::Schema, Recovery::ArgOmitted),
            RequiredByName => (Severity::Info, DiagPhase::Schema, Recovery::None),
            SchemaType => (Severity::Error, DiagPhase::Schema, Recovery::ArgOmitted),
            Enum => (Severity::Error, DiagPhase::Schema, Recovery::ArgOmitted),
            Range => (Severity::Warning, DiagPhase::Schema, Recovery::Clamped),
            SlotKind => (Severity::Error, DiagPhase::Schema, Recovery::ChildOmitted),
            MinItems => (Severity::Error, DiagPhase::Schema, Recovery::NodeDropped),
            BindTarget => (Severity::Error, DiagPhase::Schema, Recovery::BindingDropped),
            DuplicateKey => (Severity::Warning, DiagPhase::Schema, Recovery::Replaced),
            BadKey => (Severity::Warning, DiagPhase::Schema, Recovery::Replaced),
            A11yMissing => (Severity::Info, DiagPhase::Schema, Recovery::Defaulted),
            ExprType => (Severity::Error, DiagPhase::Expr, Recovery::ValueNulled),
            DivZero => (Severity::Error, DiagPhase::Expr, Recovery::ValueNulled),
            IndexOob => (Severity::Error, DiagPhase::Expr, Recovery::ValueNulled),
            FmtArgs => (Severity::Warning, DiagPhase::Expr, Recovery::Replaced),
            TaintedUrl => (Severity::Error, DiagPhase::Expr, Recovery::ValueNulled),
            Undeclared => (Severity::Error, DiagPhase::State, Recovery::ValueNulled),
            ConstDefault => (
                Severity::Error,
                DiagPhase::State,
                Recovery::StatementDropped,
            ),
            TypeWrite => (Severity::Warning, DiagPhase::State, Recovery::Ignored),
            UnknownTool => (Severity::Error, DiagPhase::Query, Recovery::KeptLastGood),
            TooManyQueries => (
                Severity::Error,
                DiagPhase::Query,
                Recovery::StatementDropped,
            ),
            QueryDenied => (Severity::Error, DiagPhase::Query, Recovery::KeptLastGood),
            QueryFailed => (Severity::Error, DiagPhase::Query, Recovery::KeptLastGood),
            RefInvalid => (Severity::Error, DiagPhase::Ref, Recovery::ValueNulled),
            RefUnknown => (Severity::Error, DiagPhase::Ref, Recovery::ValueNulled),
            RefTooLarge => (Severity::Error, DiagPhase::Ref, Recovery::ValueNulled),
            IrDepth => (Severity::Error, DiagPhase::Limit, Recovery::ChildOmitted),
            Nodes => (Severity::Error, DiagPhase::Limit, Recovery::ChildOmitted),
            Fuel => (Severity::Error, DiagPhase::Limit, Recovery::ValueNulled),
            ListLen => (Severity::Error, DiagPhase::Limit, Recovery::Truncated),
            StateSize => (Severity::Warning, DiagPhase::Limit, Recovery::Ignored),
            EventSize => (Severity::Warning, DiagPhase::Limit, Recovery::Truncated),
            RenderPanic => (Severity::Error, DiagPhase::Render, Recovery::KeptLastGood),
            CapFallback => (Severity::Info, DiagPhase::Schema, Recovery::Replaced),
            LibMismatch => (Severity::Info, DiagPhase::Schema, Recovery::None),

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
                Code::Unresolved => "name.unresolved: recovered according to SPEC §4.3.",
                Code::Cycle => "name.cycle: recovered according to SPEC §4.3.",
                Code::NoRoot => "name.no_root: recovered according to SPEC §4.3.",
                Code::Orphan => "name.orphan: recovered according to SPEC §4.3.",
                Code::UnknownComponent => {
                    "schema.unknown_component: recovered according to SPEC §4.3."
                }
                Code::UnknownArg => "schema.unknown_arg: recovered according to SPEC §4.3.",
                Code::MissingRequired => {
                    "schema.missing_required: recovered according to SPEC §4.3."
                }
                Code::ExtraPositional => {
                    "schema.extra_positional: recovered according to SPEC §4.3."
                }
                Code::DuplicateArg => "schema.dup_arg: recovered according to SPEC §4.3.",
                Code::RequiredByName => {
                    "schema.required_by_name: recovered according to SPEC §4.3."
                }
                Code::SchemaType => "schema.type: recovered according to SPEC §4.3.",
                Code::Enum => "schema.enum: recovered according to SPEC §4.3.",
                Code::Range => "schema.range: recovered according to SPEC §4.3.",
                Code::SlotKind => "schema.slot_kind: recovered according to SPEC §4.3.",
                Code::MinItems => "schema.min_items: recovered according to SPEC §4.3.",
                Code::BindTarget => "schema.bind_target: recovered according to SPEC §4.3.",
                Code::DuplicateKey => "schema.dup_key: recovered according to SPEC §4.3.",
                Code::BadKey => "schema.bad_key: recovered according to SPEC §4.3.",
                Code::A11yMissing => "schema.a11y_missing: recovered according to SPEC §4.3.",
                Code::ExprType => "expr.type: recovered according to SPEC §4.3.",
                Code::DivZero => "expr.div_zero: recovered according to SPEC §4.3.",
                Code::IndexOob => "expr.index_oob: recovered according to SPEC §4.3.",
                Code::FmtArgs => "expr.fmt_args: recovered according to SPEC §4.3.",
                Code::TaintedUrl => "expr.tainted_url: recovered according to SPEC §4.3.",
                Code::Undeclared => "state.undeclared: recovered according to SPEC §4.3.",
                Code::ConstDefault => "state.const_default: recovered according to SPEC §4.3.",
                Code::TypeWrite => "state.type_write: recovered according to SPEC §4.3.",
                Code::UnknownTool => "query.unknown_tool: recovered according to SPEC §4.3.",
                Code::TooManyQueries => "query.too_many: recovered according to SPEC §4.3.",
                Code::QueryDenied => "query.denied: recovered according to SPEC §4.3.",
                Code::QueryFailed => "query.failed: recovered according to SPEC §4.3.",
                Code::RefInvalid => "ref.invalid: recovered according to SPEC §4.3.",
                Code::RefUnknown => "ref.unknown: recovered according to SPEC §4.3.",
                Code::RefTooLarge => "ref.too_large: recovered according to SPEC §4.3.",
                Code::IrDepth => "limit.ir_depth: recovered according to SPEC §4.3.",
                Code::Nodes => "limit.nodes: recovered according to SPEC §4.3.",
                Code::Fuel => "limit.fuel: recovered according to SPEC §4.3.",
                Code::ListLen => "limit.list_len: recovered according to SPEC §4.3.",
                Code::StateSize => "limit.state: recovered according to SPEC §4.3.",
                Code::EventSize => "limit.event: recovered according to SPEC §4.3.",
                Code::RenderPanic => "render.panic: recovered according to SPEC §4.3.",
                Code::CapFallback => "cap.fallback: recovered according to SPEC §4.3.",
                Code::LibMismatch => "lib.mismatch: recovered according to SPEC §4.3.",

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
                Code::UnknownFn => "An unknown or invalidly placed function call became null.",
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
            hint: None,
            provisional: false,
        }
    }
}

/// Deterministic source order, independent of when held-back bytes were resolved.
pub(crate) fn order_diagnostics(diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_by_key(|d| (d.span.as_ref().map_or(0, |s| s.end), d.code));
}
