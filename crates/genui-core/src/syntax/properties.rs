use super::{Lexer, ast::*, lex, parser::parse_batch, printer::print, splitter::Splitter};
use crate::{diag::Code, limits::SyntaxLimits};
use proptest::prelude::*;

fn expression() -> impl Strategy<Value = String> {
    let leaf = prop_oneof![
        any::<i32>().prop_map(|n| n.to_string()),
        any::<u16>().prop_map(|n| format!("{n}.25")),
        "[a-zA-Z0-9 /#\\n\\t]{0,20}"
            .prop_map(|s| format!("\"{}\"", s.replace('\n', "\\n").replace('\t', "\\t"))),
        Just("null".into()),
        Just("true".into()),
        Just("false".into()),
        Just("other".into()),
        Just("$s".into()),
    ];
    leaf.prop_recursive(4, 96, 4, |inner| {
        prop_oneof![
            (
                inner.clone(),
                inner.clone(),
                prop::sample::select(vec![
                    "+", "-", "*", "/", "%", "==", "!=", "<", "<=", ">", ">=", "&&", "||"
                ])
            )
                .prop_map(|(a, b, op)| format!("({a} {op} {b})")),
            inner.clone().prop_map(|a| format!("!({a})")),
            inner.clone().prop_map(|a| format!("-({a})")),
            prop::collection::vec(inner.clone(), 0..4).prop_map(|a| format!("[{}]", a.join(", "))),
            inner.clone().prop_map(|a| format!("{{true: {a}}}")),
            inner
                .clone()
                .prop_map(|a| format!("Text({a}, title: \"x\")")),
            inner
                .clone()
                .prop_map(|a| format!("map([{a}], |p| p.photoUrl)")),
            inner.prop_map(|a| format!("({a})[0].query")),
        ]
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 2048, failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn print_parse_preserves_ast(value in expression()) {
        let source = format!("$s=0\nquery q=tool(\"x\", {{}})\nroot={value}\n");
        let first = parse_batch(source.as_bytes(), SyntaxLimits::default());
        prop_assert!(first.diagnostics.is_empty(), "{source}: {:?}", first.diagnostics);
        let canonical = print(&first.program);
        let second = parse_batch(canonical.as_bytes(), SyntaxLimits::default());
        prop_assert!(second.diagnostics.is_empty(), "{canonical}: {:?}", second.diagnostics);
        prop_assert_eq!(first.program, second.program);
    }
}

#[test]
fn scanner_holds_growing_tokens_utf8_and_cr() {
    let mut lexer = Lexer::default();
    lexer.push(b"foo");
    assert!(lexer.take_events().is_empty());
    lexer.push(b"Bar=\"");
    lexer.push(&[0xc3]);
    lexer.push(&[0xa9]);
    lexer.push(b"\"\r");
    lexer.push(b"\n");
    let rest = lexer.finish();
    assert_eq!(rest.tokens[0].text, "fooBar");
    assert_eq!(rest.tokens[2].text, "é");
    assert_eq!(rest.tokens[3].kind, super::TokenKind::Nl);
    assert!(rest.diagnostics.is_empty());
}

#[test]
fn resync_seals_previous_statement_before_next_line_ends() {
    let mut splitter = Splitter::default();
    splitter.push(b"a=Text(\nroot = ");
    assert_eq!(splitter.sealed().len(), 1);
    let sealed = splitter.sealed()[0].clone();
    splitter.push(b"Text(\"x\")\r");
    assert_eq!(splitter.sealed()[0], sealed);
    splitter.push(b"\n");
    let result = splitter.finish();
    assert_eq!(result.statements.len(), 2);
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        [Code::UnclosedBracket]
    );
}

#[test]
fn long_left_associative_expression_prints_without_extra_depth() {
    let source = format!("root={}", vec!["1"; 47].join("+"));
    let first = parse_batch(source.as_bytes(), SyntaxLimits::default());
    assert!(first.diagnostics.is_empty());
    let second = parse_batch(print(&first.program).as_bytes(), SyntaxLimits::default());
    assert_eq!(first.program, second.program);
    assert!(second.diagnostics.is_empty());
}

#[test]
fn no_duplicate_utf8_diagnostic_for_multiple_bad_sequences() {
    let result = lex(&[b'"', 0xff, 0xfe, b'"']);
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        [Code::BadUtf8]
    );
    assert_eq!(result.tokens[0].text, "��");
}

#[test]
fn printer_preserves_float_type_escapes_and_lambda_keys() {
    let source = b"root=each([1e0], |x| Text(\"\\u{0}\\n\\\\\\\"\"), key: |x| x.id)";
    let first = parse_batch(source, SyntaxLimits::default());
    assert!(first.diagnostics.is_empty());
    let printed = print(&first.program);
    assert!(printed.contains("1.0"));
    assert_eq!(
        parse_batch(printed.as_bytes(), SyntaxLimits::default()).program,
        first.program
    );
}

#[test]
fn query_and_state_ast_are_distinct() {
    let result = parse_batch(
        b"a=1\n$a=2\nquery q=tool(\"x\",{})",
        SyntaxLimits::default(),
    );
    assert_eq!(
        result
            .program
            .statements
            .iter()
            .map(|s| s.kind)
            .collect::<Vec<_>>(),
        [
            StatementKind::Bind,
            StatementKind::State,
            StatementKind::Query
        ]
    );
}

#[test]
fn size_recovery_keeps_token_locations_in_the_original_program() {
    let limits = SyntaxLimits {
        max_stmt_bytes: 16,
        ..SyntaxLimits::default()
    };
    let result = super::splitter::split(b"a=1\nroot=Text(\"abcdefghijklmnop\")", limits);
    assert_eq!(result.statements.len(), 2);
    let second = &result.statements[1];
    assert_eq!(
        second.header.as_ref().map(|h| h.name.as_str()),
        Some("root")
    );
    assert!(second.tokens.iter().all(|t| t.span.line == 2));
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        [Code::StmtSize]
    );
}
