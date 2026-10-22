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
