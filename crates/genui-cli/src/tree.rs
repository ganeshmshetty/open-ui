//! Static IR inspection; deferred expressions remain plans (SPEC §§5,12.7).
use genui_core::{
    diag::Severity,
    ir::{Ir, NodeId, Prop, PropValue, SlotPlan},
    r#lib::Library,
    syntax::{
        ast::{Program, Statement, StatementKind},
        printer,
    },
    validate::{Options, validate},
};
use std::{collections::BTreeMap, fmt::Write};

fn expression(ir: &Ir, plan: &SlotPlan) -> Result<String, String> {
    let value = ir
        .expressions
        .get(plan.expression.0)
        .ok_or("missing plan expression")?;
    let printed = printer::print(&Program {
        statements: vec![Statement {
            kind: StatementKind::Bind,
            name: "plan".into(),
            value: value.clone(),
        }],
    });
    Ok(printed
        .trim_end()
        .strip_prefix("plan = ")
        .ok_or("invalid printed plan")?
        .into())
}

pub fn inspect(bytes: &[u8], library: &Library) -> Result<(String, bool), String> {
    let checked = validate(bytes, library, &Options::default());
    let ir = &checked.ir;
    let mut out = String::new();
    enum Task<'a> {
        Node(NodeId, usize),
        Slot(&'a str, usize),
        Plan(&'a SlotPlan, usize),
    }
    let mut work = Vec::new();
    if let Some(id) = ir.root {
        work.push(Task::Node(id, 0));
    } else if let Some(plan) = &ir.root_plan {
        work.push(Task::Plan(plan, 0));
    } else {
        out.push_str("(no concrete root)\n");
    }
    while let Some(task) = work.pop() {
        match task {
            Task::Slot(name, depth) => {
                let _ = writeln!(out, "{}{name}:", "  ".repeat(depth));
            }
            Task::Plan(plan, depth) => {
                let _ = writeln!(
                    out,
                    "{}Dynamic [{}] {}",
                    "  ".repeat(depth),
                    plan.base.0,
                    expression(ir, plan)?
                );
            }
            Task::Node(id, depth) => {
                let node = ir.nodes.get(&id).ok_or("missing IR node")?;
                let component = library
                    .components()
                    .get(node.kind.0 as usize)
                    .ok_or("missing component")?;
                let mut props = BTreeMap::new();
                for (id, prop) in &node.props {
                    let name = &component
                        .args
                        .get(id.0 as usize)
                        .ok_or("missing prop")?
                        .name;
                    let value = match prop {
                        Prop::Dynamic(i) => serde_json::json!({"dynamic": i.0}),
                        Prop::Static(v) => match v {
                            PropValue::Value(v) => v.ir_json(),
                            PropValue::Token { scale, value } => {
                                serde_json::json!({"tok": [scale, value.plain_json()]})
                            }
                            PropValue::Binding(b) => serde_json::json!({"bind": format!("${b}")}),
                            PropValue::Action(i) => serde_json::json!({"action": i.0}),
                        },
                    };
                    props.insert(name, value);
                }
                let props = serde_json::to_string(&props).map_err(|e| e.to_string())?;
                let _ = writeln!(
                    out,
                    "{}{} [{}] {props}",
                    "  ".repeat(depth),
                    component.name,
                    node.path.0
                );
                for slot in node.slots.iter().rev() {
                    // Concrete children retain validated slot order; plans are shown separately.
                    for plan in slot.plans.iter().rev() {
                        work.push(Task::Plan(plan, depth + 2));
                    }
                    for child in slot.children.iter().rev() {
                        work.push(Task::Node(*child, depth + 2));
                    }
                    work.push(Task::Slot(&slot.name, depth + 1));
                }
            }
        }
    }
    let _ = writeln!(out, "Diagnostics: {}", checked.diagnostics.len());
    for diagnostic in &checked.diagnostics {
        let code = serde_json::to_value(diagnostic.code).map_err(|e| e.to_string())?;
        let _ = writeln!(
            out,
            "  {:?} {} recovery={:?} stmt={} path={}",
            diagnostic.severity,
            code.as_str().ok_or("invalid code")?,
            diagnostic.recovery,
            diagnostic.stmt.as_deref().unwrap_or("-"),
            diagnostic.path.as_deref().unwrap_or("-")
        );
    }
    let success = checked
        .diagnostics
        .iter()
        .all(|d| d.severity == Severity::Info);
    Ok((out, success))
}
