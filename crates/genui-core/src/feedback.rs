//! Model-facing diagnostic feedback (SPEC §§4.3,4.6). Pure and deterministic.
use crate::{
    diag::{Code, Diagnostic, Recovery, Severity},
    r#lib::Library,
};
use std::collections::BTreeSet;

pub const MAX_LINES: usize = 20;

/// Render the `ui-diag` block for final static diagnostics, or `None` when no line
/// is eligible. Runtime diagnostics never enter this function (SPEC §4.6).
pub fn render(diagnostics: &[Diagnostic], _library: &Library) -> Option<String> {
    let mut eligible: Vec<&Diagnostic> = diagnostics
        .iter()
        .filter(|d| !d.provisional)
        .filter(|d| d.severity >= Severity::Warning)
        .filter(|d| d.feedback_eligible())
        .collect();
    eligible.sort_by(|a, b| {
        severity_rank(b.severity)
            .cmp(&severity_rank(a.severity))
            .then_with(|| position(a).cmp(&position(b)))
            .then_with(|| a.stmt.cmp(&b.stmt))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.code.cmp(&b.code))
    });
    let mut seen = BTreeSet::new();
    eligible.retain(|d| seen.insert((d.code, d.stmt.clone(), d.path.clone())));
    eligible.truncate(MAX_LINES);
    if eligible.is_empty() {
        return None;
    }
    let mut text = String::from("```ui-diag\n");
    for diagnostic in eligible {
        text.push_str(&line(diagnostic));
        text.push('\n');
    }
    text.push_str("```\n");
    Some(text)
}

fn line(diagnostic: &Diagnostic) -> String {
    let mut out = format!(
        "{} at {}: {}",
        code_name(diagnostic.code),
        location(diagnostic),
        detail(diagnostic)
    );
    if let Some(hint) = &diagnostic.hint
        && !hint.is_empty()
        && diagnostic.expected.as_deref() != Some(hint.as_str())
    {
        out.push_str("; hint ");
        out.push_str(&single_line(hint));
    }
    out.push_str("; ");
    out.push_str(recovery_words(diagnostic.recovery));
    out
}

fn location(diagnostic: &Diagnostic) -> String {
    if let Some(path) = diagnostic.feedback.as_ref().and_then(|f| f.path.as_ref()) {
        return path.clone();
    }
    match (&diagnostic.stmt, &diagnostic.path) {
        (Some(stmt), Some(path)) if path != stmt => format!("{stmt}.{path}"),
        (Some(stmt), _) => stmt.clone(),
        (None, Some(path)) => path.clone(),
        (None, None) => "-".into(),
    }
}

fn detail(diagnostic: &Diagnostic) -> String {
    if let Some(detail) = diagnostic.feedback.as_ref().and_then(|f| f.detail.as_ref()) {
        return single_line(detail);
    }
    if let (Some(expected), Some(found)) = (&diagnostic.expected, &diagnostic.found) {
        return format!(
            "found {}, expected {}",
            single_line(found),
            single_line(expected)
        );
    }
    if let Some(found) = &diagnostic.found {
        return match diagnostic.code {
            Code::Unresolved => format!("no statement {}", quoted(found)),
            Code::MissingRequired => format!("{} is required", single_line(found)),
            Code::UnknownArg => format!("unknown argument {}", quoted(found)),
            Code::UnknownComponent => format!("unknown component {}", quoted(found)),
            _ => format!("found {}", single_line(found)),
        };
    }
    if !diagnostic.message.contains("recovered according to SPEC") {
        return single_line(&diagnostic.message)
            .trim_end_matches('.')
            .to_string();
    }
    default_detail(diagnostic.code).into()
}

fn default_detail(code: Code) -> &'static str {
    use Code::*;
    match code {
        Unresolved => "name is not defined",
        Cycle => "reference cycle",
        NoRoot => "root is missing",
        Orphan => "unused statement",
        UnknownComponent => "unknown component",
        UnknownArg => "unknown argument",
        MissingRequired => "required argument is missing",
        ExtraPositional => "too many positional arguments",
        DuplicateArg => "argument is repeated",
        RequiredByName => "required argument passed by name",
        SchemaType => "value has the wrong type",
        Enum => "value is not one of the allowed values",
        Range => "value is outside its range",
        SlotKind => "value is not a valid child node",
        MinItems => "slot has too few items",
        BindTarget => "bind target is not a declared state slot",
        DuplicateKey => "duplicate index key",
        BadKey => "index key must be a string or an integer",
        A11yMissing => "accessible text is missing",
        ExprType => "expression has the wrong type",
        DivZero => "division by zero",
        IndexOob => "index is out of bounds",
        FmtArgs => "format arguments do not match the placeholders",
        TaintedUrl => "URL is not allowed from this provenance",
        Undeclared => "state slot is not declared",
        ConstDefault => "state default is not a constant",
        TypeWrite => "state write has the wrong type",
        UnknownTool => "tool is not registered",
        TooManyQueries => "too many queries",
        QueryDenied => "query was denied",
        QueryFailed => "query failed",
        RefInvalid => "invalid ref id",
        RefUnknown => "ref id is not in the catalog",
        RefTooLarge => "resolved ref data is too large",
        IrDepth => "IR depth limit exceeded",
        Nodes => "node limit exceeded",
        Fuel => "fuel limit exceeded",
        ListLen => "list length limit exceeded",
        StateSize => "state size limit exceeded",
        EventSize => "event size limit exceeded",
        RenderPanic => "renderer failed",
        CapFallback => "component capability fallback",
        LibMismatch => "library identity differs",
        _ => "diagnostic",
    }
}

fn recovery_words(recovery: Recovery) -> &'static str {
    match recovery {
        Recovery::None => "no change",
        Recovery::ChildOmitted => "child omitted",
        Recovery::NodeDropped => "node dropped",
        Recovery::Defaulted => "defaulted",
        Recovery::Clamped => "clamped",
        Recovery::BindingDropped => "binding dropped",
        Recovery::KeptLastGood => "last good kept",
        Recovery::Skipped => "skipped",
        Recovery::Replaced => "replaced",
        Recovery::ClosedImplicitly => "closed implicitly",
        Recovery::CommaInserted => "comma inserted",
        Recovery::Truncated => "truncated",
        Recovery::Ignored => "ignored",
        Recovery::ValueNulled => "value set to null",
        Recovery::ArgOmitted => "argument omitted",
        Recovery::StatementDropped => "statement dropped",
        Recovery::ProgramRejected => "program rejected",
    }
}

fn severity_rank(severity: Severity) -> u8 {
    match severity {
        Severity::Info => 0,
        Severity::Warning => 1,
        Severity::Error => 2,
        Severity::Fatal => 3,
    }
}

fn position(diagnostic: &Diagnostic) -> (usize, usize, usize) {
    diagnostic
        .span
        .as_ref()
        .map_or((usize::MAX, usize::MAX, usize::MAX), |span| {
            (span.start, span.line, span.column)
        })
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", single_line(text))
}

fn single_line(text: &str) -> String {
    text.chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect()
}

fn code_name(code: Code) -> &'static str {
    use Code::*;
    match code {
        Unresolved => "name.unresolved",
        Cycle => "name.cycle",
        NoRoot => "name.no_root",
        Orphan => "name.orphan",
        UnknownComponent => "schema.unknown_component",
        UnknownArg => "schema.unknown_arg",
        MissingRequired => "schema.missing_required",
        ExtraPositional => "schema.extra_positional",
        DuplicateArg => "schema.dup_arg",
        RequiredByName => "schema.required_by_name",
        SchemaType => "schema.type",
        Enum => "schema.enum",
        Range => "schema.range",
        SlotKind => "schema.slot_kind",
        MinItems => "schema.min_items",
        BindTarget => "schema.bind_target",
        DuplicateKey => "schema.dup_key",
        BadKey => "schema.bad_key",
        A11yMissing => "schema.a11y_missing",
        ExprType => "expr.type",
        DivZero => "expr.div_zero",
        IndexOob => "expr.index_oob",
        FmtArgs => "expr.fmt_args",
        TaintedUrl => "expr.tainted_url",
        Undeclared => "state.undeclared",
        ConstDefault => "state.const_default",
        TypeWrite => "state.type_write",
        UnknownTool => "query.unknown_tool",
        TooManyQueries => "query.too_many",
        QueryDenied => "query.denied",
        QueryFailed => "query.failed",
        RefInvalid => "ref.invalid",
        RefUnknown => "ref.unknown",
        RefTooLarge => "ref.too_large",
        IrDepth => "limit.ir_depth",
        Nodes => "limit.nodes",
        Fuel => "limit.fuel",
        ListLen => "limit.list_len",
        StateSize => "limit.state",
        EventSize => "limit.event",
        RenderPanic => "render.panic",
        CapFallback => "cap.fallback",
        LibMismatch => "lib.mismatch",
        BadUtf8 => "syntax.bad_utf8",
        BadChar => "syntax.bad_char",
        BadEscape => "syntax.bad_escape",
        UnterminatedString => "syntax.unterminated_string",
        UnclosedBracket => "syntax.unclosed_bracket",
        TrailingOperator => "syntax.trailing_operator",
        StrayCloser => "syntax.stray_closer",
        MissingComma => "syntax.missing_comma",
        Trailing => "syntax.trailing",
        StmtShape => "syntax.stmt_shape",
        BareComponent => "syntax.bare_component",
        DupKey => "syntax.dup_key",
        DuplicateName => "name.duplicate",
        ReservedName => "name.reserved",
        Version => "extract.version",
        ExtraProgram => "extract.extra_program",
        Overflow => "expr.overflow",
        NonFinite => "expr.non_finite",
        LambdaMisuse => "expr.lambda_misuse",
        UnknownFn => "expr.unknown_fn",
        PositionalAfterNamed => "schema.positional_after_named",
        StmtSize => "limit.stmt_size",
        ProgramSize => "limit.program_size",
        StmtCount => "limit.stmt_count",
        Depth => "limit.depth",
        StringSize => "limit.string",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Span;

    fn library() -> Library {
        Library::builder("test", "0.1.0")
            .build()
            .expect("test library")
    }

    #[test]
    fn non_feedback_codes_and_provisional_lines_are_filtered() {
        let mut eligible = Diagnostic::new(Code::Unresolved, Some("root".into()), None);
        eligible.set_feedback_detail("no statement \"side\"".into());
        let mut info = Diagnostic::new(Code::Orphan, Some("side".into()), None);
        info.severity = Severity::Info;
        let mut provisional = Diagnostic::new(Code::Enum, Some("root".into()), None);
        provisional.provisional = true;
        let rendered = render(&[eligible, info, provisional], &library()).unwrap();
        assert_eq!(
            rendered,
            "```ui-diag\nname.unresolved at root: no statement \"side\"; value set to null\n```\n"
        );
    }

    #[test]
    fn fatal_sorts_first_source_position_breaks_ties_and_duplicates_merge() {
        let mut fatal = Diagnostic::new(Code::NoRoot, None, None);
        fatal.set_feedback_detail("root is missing".into());
        let mut later = Diagnostic::new(Code::UnknownArg, Some("root".into()), None);
        later.set_feedback_detail("unknown argument \"b\"".into());
        later.span = Some(Span {
            start: 20,
            end: 21,
            line: 2,
            column: 1,
        });
        let mut earlier = Diagnostic::new(Code::UnknownArg, Some("root".into()), None);
        earlier.set_feedback_detail("unknown argument \"b\"".into());
        earlier.span = Some(Span {
            start: 4,
            end: 5,
            line: 1,
            column: 5,
        });
        let rendered = render(&[later.clone(), earlier, later, fatal], &library()).unwrap();
        assert_eq!(
            rendered,
            "```ui-diag\nname.no_root at -: root is missing; program rejected\nschema.unknown_arg at root: unknown argument \"b\"; argument omitted\n```\n"
        );
    }

    #[test]
    fn output_is_capped_at_twenty_lines() {
        let diagnostics: Vec<_> = (0..25)
            .map(|index| {
                let mut diagnostic = Diagnostic::new(Code::UnknownArg, Some("root".into()), None);
                diagnostic.path = Some(format!("root.args[{index}]"));
                diagnostic.set_feedback_path(format!("root.args[{index}]"));
                diagnostic
            })
            .collect();
        let rendered = render(&diagnostics, &library()).unwrap();
        assert_eq!(rendered.matches('\n').count(), 22);
        assert_eq!(rendered.matches("schema.unknown_arg").count(), MAX_LINES);
    }

    #[test]
    fn recovery_words_are_ascii_and_fixed() {
        for (recovery, words) in [
            (Recovery::NodeDropped, "node dropped"),
            (Recovery::ChildOmitted, "child omitted"),
            (Recovery::ProgramRejected, "program rejected"),
        ] {
            let diagnostic = Diagnostic {
                recovery,
                ..Diagnostic::new(Code::UnknownArg, None, None)
            };
            let line = render(&[diagnostic], &library()).unwrap();
            assert!(line.contains(words), "{line}");
            assert!(words.is_ascii());
        }
    }
}
