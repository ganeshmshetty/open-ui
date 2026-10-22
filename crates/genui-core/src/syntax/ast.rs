//! Syntax-only AST. Locations and recovery diagnostics live outside structural equality.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Program {
    pub statements: Vec<Statement>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum StatementKind {
    Bind,
    State,
    Query,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Statement {
    pub kind: StatementKind,
    pub name: String,
    pub value: Expr,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Argument {
    pub name: Option<String>,
    pub value: Expr,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub key: String,
    pub value: Expr,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum UnaryOp {
    Neg,
    Not,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Expr {
    Null {
        #[serde(skip)]
        origin: NullOrigin,
    },
    Bool {
        value: bool,
    },
    Int {
        value: i64,
    },
    Float {
        value: f64,
    },
    String {
        value: String,
    },
    Name {
        name: String,
    },
    State {
        name: String,
    },
    List {
        items: Vec<Expr>,
    },
    Map {
        entries: Vec<Entry>,
    },
    Call {
        name: String,
        args: Vec<Argument>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Member {
        base: Box<Expr>,
        key: String,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Lambda {
        params: Vec<String>,
        body: Box<Expr>,
    },
}

/// Recovery metadata is deliberately excluded from structural AST equality and JSON.
#[derive(Clone, Copy, Debug, Default)]
pub struct NullOrigin {
    pub recovered: bool,
}
impl PartialEq for NullOrigin {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Expr {
    pub fn null() -> Self {
        Self::Null {
            origin: NullOrigin { recovered: false },
        }
    }
    pub(crate) fn recovered_null() -> Self {
        Self::Null {
            origin: NullOrigin { recovered: true },
        }
    }
}

impl Expr {
    /// Structural AST equality omits recovery metadata; the IR must retain it when sharing expressions.
    pub(crate) fn recovery_metadata_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null { origin: a }, Self::Null { origin: b }) => a.recovered == b.recovered,
            (Self::List { items: a }, Self::List { items: b }) => {
                a.iter().zip(b).all(|(a, b)| a.recovery_metadata_eq(b))
            }
            (Self::Map { entries: a }, Self::Map { entries: b }) => a
                .iter()
                .zip(b)
                .all(|(a, b)| a.value.recovery_metadata_eq(&b.value)),
            (Self::Call { args: a, .. }, Self::Call { args: b, .. }) => a
                .iter()
                .zip(b)
                .all(|(a, b)| a.value.recovery_metadata_eq(&b.value)),
            (Self::Unary { expr: a, .. }, Self::Unary { expr: b, .. })
            | (Self::Member { base: a, .. }, Self::Member { base: b, .. })
            | (Self::Lambda { body: a, .. }, Self::Lambda { body: b, .. }) => {
                a.recovery_metadata_eq(b)
            }
            (
                Self::Binary {
                    left: a, right: b, ..
                },
                Self::Binary {
                    left: c, right: d, ..
                },
            )
            | (Self::Index { base: a, index: b }, Self::Index { base: c, index: d }) => {
                a.recovery_metadata_eq(c) && b.recovery_metadata_eq(d)
            }
            _ => true,
        }
    }
}
