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
