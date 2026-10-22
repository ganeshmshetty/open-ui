//! Canonical syntax printer, independent of library definitions.
use super::ast::*;
use std::fmt::Write;

pub fn print(program: &Program) -> String {
    let mut out = String::new();
    for statement in &program.statements {
        match statement.kind {
            StatementKind::State => out.push('$'),
            StatementKind::Query => out.push_str("query "),
            _ => {}
        }
        out.push_str(&statement.name);
        out.push_str(" = ");
        expression(&statement.value, &mut out);
        out.push('\n');
    }
    out
}

fn quoted(value: &str, out: &mut String) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn expression(expr: &Expr, out: &mut String) {
    match expr {
        Expr::Null { .. } => out.push_str("null"),
        Expr::Bool { value } => out.push_str(if *value { "true" } else { "false" }),
        Expr::Int { value } => {
            let _ = write!(out, "{value}");
        }
        Expr::Float { value } => {
            let text = value.to_string();
            out.push_str(&text);
            if !text.contains(['.', 'e', 'E']) {
                out.push_str(".0");
            }
        }
        Expr::String { value } => quoted(value, out),
        Expr::Name { name } => out.push_str(name),
        Expr::State { name } => {
            out.push('$');
            out.push_str(name);
        }
        Expr::List { items } => {
            out.push('[');
            for (i, v) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                expression(v, out);
            }
            out.push(']');
        }
        Expr::Map { entries } => {
            out.push('{');
            for (i, entry) in entries.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                quoted(&entry.key, out);
                out.push_str(": ");
                expression(&entry.value, out);
            }
            out.push('}');
        }
        Expr::Call { name, args } => {
            out.push_str(name);
            out.push('(');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                if let Some(name) = &arg.name {
                    out.push_str(name);
                    out.push_str(": ");
                }
                expression(&arg.value, out);
            }
            out.push(')');
        }
        Expr::Unary { op, expr } => {
            out.push_str(match op {
                UnaryOp::Neg => "-",
                UnaryOp::Not => "!",
            });
            child(expr, 7, out);
        }
        Expr::Binary { op, left, right } => {
            let p = precedence(*op);
            child(left, if matches!(p, 3 | 4) { p + 1 } else { p }, out);
            out.push(' ');
            out.push_str(binary(*op));
            out.push(' ');
            child(right, p + 1, out);
        }
        Expr::Member { base, key } => {
            child(base, 8, out);
            out.push('.');
            out.push_str(key);
        }
        Expr::Index { base, index } => {
            child(base, 8, out);
            out.push('[');
            expression(index, out);
            out.push(']');
        }
        Expr::Lambda { params, body } => {
            out.push('|');
            out.push_str(&params.join(", "));
            out.push_str("| ");
            expression(body, out);
        }
    }
}
fn binary(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Rem => "%",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
    }
}

fn child(expr: &Expr, minimum: u8, out: &mut String) {
    let own = match expr {
        Expr::Binary { op, .. } => precedence(*op),
        Expr::Unary { .. } => 7,
        Expr::Lambda { .. } => 0,
        _ => 8,
    };
    if own < minimum {
        out.push('(');
    }
    expression(expr, out);
    if own < minimum {
        out.push(')');
    }
}
fn precedence(op: BinaryOp) -> u8 {
    match op {
        BinaryOp::Or => 1,
        BinaryOp::And => 2,
        BinaryOp::Eq | BinaryOp::Ne => 3,
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => 4,
        BinaryOp::Add | BinaryOp::Sub => 5,
        _ => 6,
    }
}
