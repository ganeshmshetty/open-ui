//! Focused public-contract and recovery assertions added by the §12.8 mutation audit.
use super::{
    ast::*,
    extract::{self, Extractor, Segment},
};
use crate::diag::Code;
use crate::ir::Ir;

fn nested(null: Expr) -> Vec<Expr> {
    vec![
        Expr::List {
            items: vec![null.clone()],
        },
        Expr::Map {
            entries: vec![Entry {
                key: "x".into(),
                value: null.clone(),
            }],
        },
        Expr::Call {
            name: "f".into(),
            args: vec![Argument {
                name: None,
                value: null.clone(),
            }],
        },
        Expr::Unary {
            op: UnaryOp::Not,
            expr: Box::new(null.clone()),
        },
        Expr::Member {
            base: Box::new(null.clone()),
            key: "x".into(),
        },
        Expr::Lambda {
            params: vec!["x".into()],
            body: Box::new(null.clone()),
        },
        Expr::Binary {
            op: BinaryOp::Add,
            left: Box::new(null.clone()),
            right: Box::new(Expr::null()),
        },
        Expr::Binary {
            op: BinaryOp::Add,
            left: Box::new(Expr::null()),
            right: Box::new(null.clone()),
        },
        Expr::Index {
            base: Box::new(null.clone()),
            index: Box::new(Expr::null()),
        },
        Expr::Index {
            base: Box::new(Expr::null()),
            index: Box::new(null),
        },
    ]
}

#[test]
fn tracked_source_preserves_raw_endpoints_for_repairs_and_crlf() {
    use super::source::Source;
    let mut source = Source::tracked();
    assert_eq!(source.push(b"a\r\nb", false), "a\nb");
    assert_eq!(source.take_raw_ends(), vec![1, 3, 4]);
    assert_eq!(source.push(b"\xf0", false), "");
    assert!(source.take_raw_ends().is_empty());
    assert_eq!(source.push(b"", true), "\u{fffd}");
    assert_eq!(source.take_raw_ends(), vec![5]);
    assert!(source.bad_utf8);
}

#[test]
fn provisional_numeric_span_stays_relative_to_its_original_token() {
    let mut lexer = super::Lexer::default();
    lexer.push(b"\nroot=123");
    let events = lexer.provisional_events();
    assert_eq!(events.len(), 1);
    let super::LexEvent::Token(token) = &events[0] else {
        panic!("number token required")
    };
    assert_eq!(token.token.kind, super::TokenKind::Int);
    assert_eq!(token.token.text, "123");
    assert_eq!(
        (
            token.span.start,
            token.span.end,
            token.span.line,
            token.span.column
        ),
        (6, 9, 2, 6)
    );
}

#[test]
fn live_splitter_source_and_committed_cutoff_diagnostics_are_exact() {
    use super::splitter::Splitter;
    use crate::{diag::Code, limits::SyntaxLimits};
    let mut s = Splitter::new(SyntaxLimits::default());
    s.push(b"\xef\xbb\xbfroot=Text(\"a\")\r");
    assert_eq!(s.source(), "root=Text(\"a\")");
    s.push(b"\nother=Divider()\n");
    assert_eq!(s.source(), "root=Text(\"a\")\nother=Divider()\n");

    let mut s = Splitter::new(SyntaxLimits {
        max_stmt_bytes: 0,
        ..Default::default()
    });
    s.push(b"a=Text(");
    let diagnostics = s.diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, Code::StmtSize);
    assert_eq!(
        diagnostics[0].span.as_ref().map(|s| (s.start, s.end)),
        Some((0, 0))
    );
    assert!(!diagnostics[0].provisional);

    let mut s = Splitter::new(SyntaxLimits {
        max_stmt_bytes: 5,
        ..Default::default()
    });
    s.push("a=§\"abcdef\\q".as_bytes());
    assert_eq!(
        s.diagnostics().iter().map(|d| d.code).collect::<Vec<_>>(),
        vec![Code::BadChar, Code::StmtSize]
    );
    s.push(b"more");
    assert_eq!(
        s.diagnostics().iter().map(|d| d.code).collect::<Vec<_>>(),
        vec![Code::BadChar, Code::StmtSize]
    );
}

#[test]
fn closed_tail_keeps_complete_lambda_bodies_and_removes_only_unfinished_heads() {
    use super::splitter::Splitter;
    for (input, expected) in [
        ("root=Button(\"Go\",on_click:|x, ", "root=Button(\"Go\")\n"),
        (
            "root=Column(each([1],|x|Text(\"a",
            "root=Column(each([1],|x|Text(\"a\")))\n",
        ),
        ("root=Text(foo+ ", "root=Text(foo)\n"),
        ("root=Text(1: ", "root=Text(1)\n"),
        ("root=Text(\"kept\",true: ", "root=Text(\"kept\")\n"),
        ("root=Text(\"kept\",false: ", "root=Text(\"kept\")\n"),
        ("root=Text(\"kept\",null: ", "root=Text(\"kept\")\n"),
    ] {
        let mut s = Splitter::new(Default::default());
        s.push(input.as_bytes());
        let (tail, diagnostics) = s.provisional().unwrap();
        assert!(diagnostics.is_empty(), "{input}: {diagnostics:?}");
        let parsed = super::parser::parse_split(
            super::splitter::SplitResult {
                statements: vec![tail],
                diagnostics,
            },
            &Default::default(),
        );
        let wanted = super::parse_batch(expected.as_bytes(), Default::default());
        assert_eq!(
            parsed.program, wanted.program,
            "{input}: {:?}",
            parsed.diagnostics
        );
        assert!(
            parsed.diagnostics.is_empty(),
            "{input}: {:?}",
            parsed.diagnostics
        );
    }
    let mut query = Splitter::new(Default::default());
    query.push(b"query q=x: ");
    let (tail, diagnostics) = query.provisional().unwrap();
    assert!(diagnostics.is_empty());
    assert_eq!(tail.header.as_ref().unwrap().rhs, 3);
    assert_eq!(
        tail.tokens
            .iter()
            .map(|t| t.token.text.as_str())
            .collect::<Vec<_>>(),
        vec!["query", "q", "="]
    );
}

#[test]
fn recovery_metadata_survives_every_expression_container() {
    for (literal, recovered) in nested(Expr::null())
        .into_iter()
        .zip(nested(Expr::recovered_null()))
    {
        assert_eq!(literal, recovered, "structural equality excludes recovery");
        assert!(!literal.recovery_metadata_eq(&recovered), "{literal:?}");
        assert!(!recovered.recovery_metadata_eq(&literal));
        assert!(recovered.recovery_metadata_eq(&recovered));
        let mut ir = Ir::default();
        let a = ir.intern(&literal);
        let b = ir.intern(&recovered);
        assert_ne!(a, b);
        assert_eq!(ir.intern(&recovered), b);
        assert_eq!(ir.intern(&literal), a);
    }
}

#[test]
fn extractor_public_batch_and_segment_views_preserve_text() {
    let mut scanner = Extractor::default();
    scanner.push(b"Before\n```ui\nroot=Divider()\n");
    let expected = vec![
        Segment::Markdown("Before\n".into()),
        Segment::Program("root=Divider()\n".into()),
    ];
    assert_eq!(scanner.segments(), expected);
    scanner.push(b"```\nAfter\n");
    let finished = scanner.finish();
    assert_eq!(
        finished,
        extract::extract(b"Before\n```ui\nroot=Divider()\n```\nAfter\n")
    );
    assert_eq!(
        finished.segments,
        [expected, vec![Segment::Markdown("After\n".into())]].concat()
    );
    assert!(!finished.rejected);
    assert!(finished.diagnostics.is_empty());
}

#[test]
fn extractor_public_status_views_track_end_rejection_and_diagnostics() {
    let mut fenced = Extractor::default();
    fenced.push(b"```ui\nroot=Divider()\n");
    assert!(!fenced.program_ended());
    assert!(!fenced.rejected());
    assert!(fenced.diagnostics().is_empty());
    fenced.push(b"```\n");
    assert!(fenced.program_ended());
    assert!(!fenced.rejected());
    assert!(fenced.diagnostics().is_empty());

    let mut version = Extractor::default();
    version.push(b"```ui v2\nroot=Divider()\n```\n");
    assert!(version.rejected());
    assert_eq!(
        version
            .diagnostics()
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec![Code::Version]
    );

    let mut extra = Extractor::default();
    extra.push(b"```ui\nroot=Divider()\n```\n```ui\nroot=Divider()\n```\n");
    assert!(!extra.rejected());
    assert_eq!(
        extra
            .diagnostics()
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec![Code::ExtraProgram]
    );
}

#[test]
fn extractor_continues_unfenced_programs_across_lines_without_fences() {
    let extracted = extract::extract(b"root=Text(\"a\")\nnext=Text(\"b\")\n");
    assert_eq!(
        extracted.segments,
        vec![Segment::Program(
            "root=Text(\"a\")\nnext=Text(\"b\")\n".into()
        )]
    );
    assert!(extracted.diagnostics.is_empty());
    assert!(!extracted.rejected);

    let mut split = Extractor::default();
    split.push(b"root=Text(\"a\")\n");
    assert_eq!(
        split.segments(),
        [Segment::Program("root=Text(\"a\")\n".into())]
    );
    split.push(b"next=Text(\"b\")\n");
    assert_eq!(
        split.segments(),
        [Segment::Program(
            "root=Text(\"a\")\nnext=Text(\"b\")\n".into()
        )]
    );
}

#[test]
fn extractor_keeps_prose_prefixes_and_detects_a_later_program_start() {
    for prose in ["a b c\n", "ab cd\n", "query x y\n", "true false\n"] {
        let source = format!("{prose}root=Divider()\n");
        let extracted = extract::extract(source.as_bytes());
        assert_eq!(
            extracted.segments,
            vec![
                Segment::Markdown(prose.into()),
                Segment::Program("root=Divider()\n".into())
            ],
            "{prose}"
        );
        assert!(extracted.diagnostics.is_empty(), "{prose}");
    }
}

#[test]
fn lexer_reports_only_a_pending_colon_and_parses_at_the_numeric_limit() {
    let mut colon = super::Lexer::default();
    colon.push(b"root");
    assert!(!colon.pending_colon());
    colon.push(b":");
    assert!(colon.pending_colon());
    colon.push(b" 1");
    assert!(!colon.pending_colon());

    let mut at_limit = super::Lexer::with_numeric_limit(3);
    at_limit.push(b"123 ");
    let events = at_limit.take_events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, super::LexEvent::Token(token)
            if token.token.kind == super::TokenKind::Int && token.token.text == "123")),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .all(|event| !matches!(event, super::LexEvent::Diagnostic(_))),
        "{events:?}"
    );

    let mut over = super::Lexer::with_numeric_limit(3);
    over.push(b"1234 ");
    let events = over.take_events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, super::LexEvent::Token(token)
            if token.token.kind == super::TokenKind::Null)),
        "{events:?}"
    );
}

#[test]
fn sealing_a_statement_charges_its_relevant_scanner_diagnostics() {
    use crate::limits::SyntaxLimits;
    let mut parser = super::streaming::StreamingParser::new(SyntaxLimits::default());
    parser.push(b"root=999999999999999999999999\n");
    assert!(
        parser
            .diagnostic_snapshot()
            .iter()
            .any(|d| d.code == Code::Overflow),
        "{:?}",
        parser.diagnostic_snapshot()
    );
    assert!(parser.work().diagnostics_examined > 0);
}

#[test]
fn malformed_escape_spans_include_only_the_consumed_escape() {
    use crate::diag::{Code, Span};
    for (source, end) in [
        (br#""\u{zz}""#.as_slice(), 7),
        (br#""\u{zz""#.as_slice(), 6),
    ] {
        let checked = super::lex(source);
        assert_eq!(checked.diagnostics.len(), 1);
        assert_eq!(checked.diagnostics[0].code, Code::BadEscape);
        assert_eq!(
            checked.diagnostics[0].span,
            Some(Span {
                start: 1,
                end,
                line: 1,
                column: 2
            })
        );
    }
}

#[test]
fn incomplete_number_suffixes_keep_exact_token_spans() {
    use super::TokenKind as K;
    for (input, expected) in [
        (b"1.".as_slice(), vec![(K::Int, 0, 1), (K::Dot, 1, 2)]),
        (b"1e".as_slice(), vec![(K::Int, 0, 1), (K::Name, 1, 2)]),
        (
            b"1e+".as_slice(),
            vec![(K::Int, 0, 1), (K::Name, 1, 2), (K::Plus, 2, 3)],
        ),
    ] {
        let checked = super::lex(input);
        assert!(checked.diagnostics.is_empty());
        let actual: Vec<_> = checked
            .tokens
            .iter()
            .zip(checked.spans)
            .map(|(t, s)| {
                assert_eq!((s.line, s.column), (1, s.start + 1));
                (t.kind, s.start, s.end)
            })
            .collect();
        assert_eq!(actual, expected);
    }
}

fn retained_depth(expr: &Expr) -> usize {
    let children = match expr {
        Expr::Null { origin } if origin.recovered => return 0,
        Expr::List { items } => items.iter().map(retained_depth).max().unwrap_or(0),
        Expr::Map { entries } => entries
            .iter()
            .map(|e| retained_depth(&e.value))
            .max()
            .unwrap_or(0),
        Expr::Call { args, .. } => args
            .iter()
            .map(|a| retained_depth(&a.value))
            .max()
            .unwrap_or(0),
        Expr::Unary { expr, .. }
        | Expr::Member { base: expr, .. }
        | Expr::Lambda { body: expr, .. } => retained_depth(expr),
        Expr::Binary { left, right, .. }
        | Expr::Index {
            base: left,
            index: right,
        } => retained_depth(left).max(retained_depth(right)),
        _ => 0,
    };
    1 + children
}

#[test]
fn every_retained_expression_respects_its_enclosing_budget() {
    use crate::limits::SyntaxLimits;
    for limit in 0..=8 {
        for source in [
            "1+2*3",
            "1+a.b.c",
            "a+b+c*d.e",
            "[a.b+c.d,f(1+2*3)]",
            "!a.b+c",
            "(a+b)*c",
            "each([1,2],|x|x.a+b)",
            "{x:1+2,y:[3,4]}",
        ] {
            let result = super::parse_batch(
                format!("root={source}\n").as_bytes(),
                SyntaxLimits {
                    max_expr_depth: limit,
                    ..SyntaxLimits::default()
                },
            );
            for statement in result.program.statements {
                assert!(
                    retained_depth(&statement.value) <= limit,
                    "limit={limit} source={source}: {:?}",
                    statement.value
                );
            }
            assert!(
                result
                    .diagnostics
                    .iter()
                    .filter(|d| d.code == crate::diag::Code::Depth)
                    .count()
                    <= 1
            );
        }
    }
}

#[test]
fn printer_preserves_multiple_map_entries_and_lambda_child_precedence() {
    let parsed = super::parse_batch(b"root={a:1,b:2,c:3}\n", Default::default());
    assert!(parsed.diagnostics.is_empty());
    let text = super::printer::print(&parsed.program);
    assert_eq!(text, "root = {\"a\": 1, \"b\": 2, \"c\": 3}\n");
    assert_eq!(
        super::parse_batch(text.as_bytes(), Default::default()).program,
        parsed.program
    );
    // The printer's public AST API also accepts constructed expression trees.
    let program = Program {
        statements: vec![Statement {
            kind: StatementKind::Bind,
            name: "root".into(),
            value: Expr::Member {
                base: Box::new(Expr::Lambda {
                    params: vec!["x".into()],
                    body: Box::new(Expr::Name { name: "x".into() }),
                }),
                key: "a".into(),
            },
        }],
    };
    assert_eq!(super::printer::print(&program), "root = (|x| x).a\n");
}

#[test]
fn statement_headers_require_every_contextual_query_condition() {
    use super::{LocatedToken, splitter::statement_start};
    for source in ["other x = 1", "query X = 1", "query x + 1", "true x = 1"] {
        let lexed = super::lex(source.as_bytes());
        let tokens: Vec<_> = lexed
            .tokens
            .into_iter()
            .zip(lexed.spans)
            .map(|(token, span)| LocatedToken { token, span })
            .collect();
        assert_eq!(statement_start(&tokens), None, "{source}");
    }
}

#[test]
fn splitter_public_tokens_keep_synthetic_spellings_and_cutoff_spans() {
    use super::{TokenKind as K, splitter::split};
    use crate::{
        diag::{Code, Span},
        limits::SyntaxLimits,
    };
    for (source, close, spelling) in [
        ("root=f(1", K::RParen, ")"),
        ("root=[1", K::RBracket, "]"),
        ("root={a:1", K::RBrace, "}"),
        ("root=([1)", K::RParen, ")"),
    ] {
        let result = split(source.as_bytes(), Default::default());
        let tokens = &result.statements[0].tokens;
        assert_eq!(
            (
                tokens.last().unwrap().token.kind,
                tokens.last().unwrap().token.text.as_str()
            ),
            (close, spelling)
        );
        if source == "root=([1)" {
            assert_eq!(tokens[tokens.len() - 2].token.text, "]");
        }
    }
    let source = "a=0\nroot=[12345,2]\nnext=4\n";
    let result = split(
        source.as_bytes(),
        SyntaxLimits {
            max_stmt_bytes: 9,
            ..Default::default()
        },
    );
    assert_eq!(result.statements.len(), 3);
    let d = result
        .diagnostics
        .iter()
        .find(|d| d.code == Code::StmtSize)
        .unwrap();
    assert_eq!(
        d.span,
        Some(Span {
            start: 4,
            end: 13,
            line: 2,
            column: 1
        })
    );
    let tokens = &result.statements[1].tokens;
    assert_eq!((tokens[0].span.start, tokens[0].span.end), (4, 8));
    assert_eq!((tokens[2].span.start, tokens[2].span.end), (9, 10));
    assert_eq!(tokens.last().unwrap().token.text, "]");
    assert!(tokens.iter().all(|t| t.span.start >= 4 && t.span.line == 2));
}

#[test]
fn splitter_sealed_view_uses_early_line_decisions_without_losing_continuations() {
    use super::splitter::Splitter;
    for continuation in ["[", "$x", "query x", "query x +"] {
        let mut splitter = Splitter::default();
        splitter.push(b"root=[\n");
        splitter.push(continuation.as_bytes());
        splitter.push(b" ");
        assert!(splitter.sealed().is_empty(), "{continuation}");
        splitter.push(b"\nnext=1\n");
        let result = splitter.finish();
        assert_eq!(result.statements.len(), 2, "{continuation}");
        assert_eq!(result.statements[1].header.as_ref().unwrap().name, "next");
    }
}

#[test]
fn splitter_preserves_empty_binder_and_suppresses_only_cutoff_repairs() {
    use super::{TokenKind as K, splitter::split};
    use crate::{diag::Code, limits::SyntaxLimits};
    let empty = split(b"root=", Default::default());
    assert_eq!(
        empty.statements[0]
            .tokens
            .iter()
            .map(|t| t.token.kind)
            .collect::<Vec<_>>(),
        [K::Name, K::Assign]
    );
    assert_eq!(
        empty.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
        [Code::TrailingOperator]
    );
    let cutoff = split(
        b"root=1)extra",
        SyntaxLimits {
            max_stmt_bytes: 7,
            ..Default::default()
        },
    );
    assert_eq!(
        cutoff
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::StrayCloser)
            .count(),
        1
    );
    assert_eq!(
        cutoff
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::StmtSize)
            .count(),
        1
    );
    let nested = split(
        b"root=[(}",
        SyntaxLimits {
            max_bracket_depth: 1,
            ..Default::default()
        },
    );
    assert_eq!(
        nested
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        [Code::Depth, Code::UnclosedBracket]
    );
}

#[test]
fn printer_round_trips_every_binary_operator_pair_in_both_positions() {
    let operators = [
        "+", "-", "*", "/", "%", "&&", "||", "==", "!=", "<", "<=", ">", ">=",
    ];
    for outer in operators {
        for inner in operators {
            for source in [
                format!("root=(a {inner} b) {outer} c\n"),
                format!("root=a {outer} (b {inner} c)\n"),
            ] {
                let parsed = super::parse_batch(source.as_bytes(), Default::default());
                assert!(
                    parsed.diagnostics.is_empty(),
                    "{source}: {:?}",
                    parsed.diagnostics
                );
                let printed = super::printer::print(&parsed.program);
                let reparsed = super::parse_batch(printed.as_bytes(), Default::default());
                assert!(
                    reparsed.diagnostics.is_empty(),
                    "{source} -> {printed}: {:?}",
                    reparsed.diagnostics
                );
                assert_eq!(reparsed.program, parsed.program, "{source} -> {printed}");
            }
        }
    }
    let parsed = super::parse_batch(b"root=(1+2)*3\n", Default::default());
    assert_eq!(
        super::printer::print(&parsed.program),
        "root = (1 + 2) * 3\n"
    );
}
