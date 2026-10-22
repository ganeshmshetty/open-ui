//! Focused public-contract and recovery assertions added by the §12.8 mutation audit.
use super::{
    ast::*,
    extract::{self, Extractor, Segment},
};
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
