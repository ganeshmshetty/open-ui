//! Synchronous, UI-free runtime evaluation (§§2.6–2.7,5.5,7.1–7.4).
//!
//! This module owns values, pure builtins, fuel, state reads/writes, field status,
//! delivered query/ref data and evaluated action effects. The session owns request
//! identity, host delivery and model-event construction so no host effect can be
//! produced by validation or streaming.
mod fields;
mod instantiate;
pub use fields::{Field, FieldError, FieldRegistry, FieldStatus};

/// Synthetic resolved props. They are not library arguments and never visible
/// to expressions; the wire projection names them `error`, `touched` and
/// `status` (§§7.3,7.5).
pub const PROP_ERROR: PropId = PropId(u16::MAX);
pub const PROP_TOUCHED: PropId = PropId(u16::MAX - 1);
pub const PROP_STATUS: PropId = PropId(u16::MAX - 2);

use crate::{
    diag::{Code, Diagnostic},
    ir::{Ir, NodeId, Prop, PropId, PropValue, Step},
    r#lib::{Library, T},
    limits::Limits,
    syntax::ast::{Argument, BinaryOp, Expr, UnaryOp},
    value::{PStr, Provenance, Value},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StateOrigin {
    #[default]
    Default,
    User,
    Action,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StateSlot {
    pub value: Value,
    pub ty: T,
    pub origin: StateOrigin,
    pub default: Value,
    pub rev: u64,
}

/// Runtime state is plain data so the host can snapshot it (§7.2).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StateStore {
    slots: BTreeMap<String, StateSlot>,
}
impl StateStore {
    pub fn from_ir(ir: &Ir) -> Self {
        let mut store = Self::default();
        store.sync(&ir.states);
        store
    }
    /// Add, retype or reset declaration-owned slots. User/action values survive
    /// a declaration refresh with the same type.
    pub fn sync(&mut self, defs: &crate::SharedMap<String, crate::ir::StateDef>) -> bool {
        let mut changed = false;
        let names: BTreeSet<_> = defs.keys().cloned().collect();
        let stale: Vec<_> = self
            .slots
            .keys()
            .filter(|name| !names.contains(*name))
            .cloned()
            .collect();
        for name in stale {
            self.slots.remove(&name);
            changed = true;
        }
        for (name, def) in defs {
            match self.slots.get_mut(name) {
                Some(slot) => {
                    if slot.ty != def.ty || slot.default != def.default {
                        if slot.origin == StateOrigin::Default {
                            slot.value = def.default.clone();
                        }
                        slot.default = def.default.clone();
                        slot.ty = def.ty.clone();
                        slot.rev = slot.rev.saturating_add(1);
                        changed = true;
                    }
                }
                None => {
                    self.slots.insert(
                        name.clone(),
                        StateSlot {
                            value: def.default.clone(),
                            ty: def.ty.clone(),
                            origin: StateOrigin::Default,
                            default: def.default.clone(),
                            rev: 0,
                        },
                    );
                    changed = true;
                }
            }
        }
        changed
    }
    pub fn get(&self, name: &str) -> Option<&StateSlot> {
        self.slots.get(name)
    }
    pub fn value(&self, name: &str) -> Option<&Value> {
        self.slots.get(name).map(|slot| &slot.value)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&String, &StateSlot)> {
        self.slots.iter()
    }
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
    pub fn len(&self) -> usize {
        self.slots.len()
    }
    /// Apply a typed write. The caller decides how a rejected write is reported.
    pub fn write(
        &mut self,
        name: &str,
        value: Value,
        origin: StateOrigin,
        limits: &Limits,
        library: &Library,
    ) -> Result<(), Code> {
        let Some(slot) = self.slots.get(name) else {
            return Err(Code::Undeclared);
        };
        if !library.accepts(&slot.ty, &value) {
            return Err(Code::TypeWrite);
        }
        if value_size(&value) > limits.max_state_bytes {
            return Err(Code::StateSize);
        }
        let mut total = 0usize;
        for (key, slot) in &self.slots {
            total = total.saturating_add(if key == name {
                value_size(&value)
            } else {
                value_size(&slot.value)
            });
        }
        if total > limits.max_state_bytes {
            return Err(Code::StateSize);
        }
        if let Some(slot) = self.slots.get_mut(name) {
            slot.value = value;
            slot.origin = origin;
            slot.rev = slot.rev.saturating_add(1);
        }
        Ok(())
    }
    pub fn restore(&mut self, name: &str) -> bool {
        let Some(slot) = self.slots.get_mut(name) else {
            return false;
        };
        slot.value = slot.default.clone();
        slot.origin = StateOrigin::Default;
        slot.rev = slot.rev.saturating_add(1);
        true
    }
    #[cfg(test)]
    pub(crate) fn insert_test(&mut self, name: &str, value: Value) {
        let ty = crate::r#lib::T::infer(&value);
        self.slots.insert(
            name.into(),
            StateSlot {
                value: value.clone(),
                ty,
                origin: StateOrigin::Default,
                default: value,
                rev: 0,
            },
        );
    }
}
fn value_size(value: &Value) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len())
}

/// Evaluated Dynamic props for the current tree. Static props remain in the IR.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Resolved {
    props: crate::SharedMap<NodeId, crate::SharedMap<PropId, PropValue>>,
    diagnostics: Vec<Diagnostic>,
    /// The runtime projection: the validated IR with data-dependent plans
    /// instantiated and runtime drops applied (§§2.10,5.5). Static IR stays intact.
    ir: Ir,
}
impl Resolved {
    pub fn prop(&self, node: NodeId, prop: PropId) -> Option<&PropValue> {
        self.props.get(&node).and_then(|props| props.get(&prop))
    }
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    /// Expanded tree used by the wire projection and runtime lookups.
    pub fn ir(&self) -> &Ir {
        &self.ir
    }
    /// Expanded tree when one exists, else the static fallback.
    pub fn ir_or<'a>(&'a self, fallback: &'a Ir) -> &'a Ir {
        if self.ir.root.is_some() || !self.ir.nodes.is_empty() || self.ir.root_plan.is_none() {
            &self.ir
        } else {
            fallback
        }
    }
    pub fn is_empty(&self) -> bool {
        self.props.is_empty()
    }
    pub fn entries(&self) -> impl Iterator<Item = (NodeId, PropId, &PropValue)> {
        self.props
            .iter()
            .flat_map(|(node, props)| props.iter().map(move |(prop, value)| (*node, *prop, value)))
    }
    pub fn node_props(&self, node: NodeId) -> Option<&crate::SharedMap<PropId, PropValue>> {
        self.props.get(&node)
    }
    pub fn set(&mut self, node: NodeId, prop: PropId, value: PropValue) {
        let mut props = self.props.get(&node).cloned().unwrap_or_default();
        props.insert(prop, value);
        self.props.insert(node, props);
    }
}

/// Runtime state, delivered host data and the explicit action entry point.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Runtime {
    pub state: StateStore,
    queries: BTreeMap<String, Value>,
    refs: BTreeMap<String, Value>,
}
impl Runtime {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn evaluate(
        &self,
        ir: &Ir,
        library: &Library,
        limits: &Limits,
        catalog: &BTreeSet<String>,
    ) -> Resolved {
        evaluate_full(
            ir,
            library,
            limits,
            &self.state,
            &self.queries,
            &self.refs,
            catalog,
        )
    }
    /// Evaluate one expression outside the resolved tree (query arguments and
    /// action-step arguments). Errors are returned, never dispatched.
    pub fn evaluate_expression(
        &self,
        ir: &Ir,
        library: &Library,
        limits: &Limits,
        catalog: &BTreeSet<String>,
        expression: &Expr,
        stmt: &str,
    ) -> (Value, Vec<Diagnostic>) {
        self.evaluate_expression_with_locals(
            ir,
            library,
            limits,
            catalog,
            expression,
            stmt,
            &BTreeMap::new(),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate_expression_with_locals(
        &self,
        ir: &Ir,
        library: &Library,
        limits: &Limits,
        catalog: &BTreeSet<String>,
        expression: &Expr,
        stmt: &str,
        locals: &BTreeMap<String, Value>,
    ) -> (Value, Vec<Diagnostic>) {
        let mut diagnostics = Vec::new();
        let values = evaluate_values(
            ir,
            library,
            limits,
            &self.state,
            &self.queries,
            &self.refs,
            catalog,
            &mut diagnostics,
        );
        let mut queries = BTreeMap::new();
        let mut evaluator = Evaluator::new(
            ir,
            library,
            limits,
            &self.state,
            &values,
            &self.queries,
            &self.refs,
            catalog,
            &mut queries,
            &mut diagnostics,
        );
        let value = evaluator.eval(
            expression,
            locals,
            &Context {
                stmt: stmt.into(),
                path: None,
            },
        );
        if evaluator.failed {
            (Value::Null, diagnostics)
        } else {
            (value, diagnostics)
        }
    }
    pub fn query_result(&self, name: &str) -> Option<&Value> {
        self.queries.get(name)
    }
    pub fn ref_value(&self, id: &str) -> Option<&Value> {
        self.refs.get(id)
    }
    pub fn set_query_result(&mut self, name: &str, value: Value) {
        self.queries.insert(name.into(), value);
    }
    pub fn set_ref_value(&mut self, id: &str, value: Value) {
        self.refs.insert(id.into(), value);
    }
    pub fn clear_query_result(&mut self, name: &str) {
        self.queries.remove(name);
    }
    pub fn resolved_ref_count(&self) -> usize {
        self.refs.len()
    }
    /// Run a plan's steps in order. State steps apply immediately; host effects
    /// are returned in execution order and never dispatched here (§7.4).
    pub fn execute_action(
        &mut self,
        ir: &Ir,
        library: &Library,
        limits: &Limits,
        catalog: &BTreeSet<String>,
        plan: &crate::ir::ActionPlan,
    ) -> ActionExecution {
        let mut diagnostics = Vec::new();
        let mut effects = Vec::new();
        let mut applied = 0usize;
        for step in &plan.steps {
            let evaluate = |runtime: &Runtime, expression: &crate::ir::ExprId| {
                ir.expressions
                    .get(expression)
                    .map_or((Value::Null, Vec::new()), |expression| {
                        runtime.evaluate_expression_with_locals(
                            ir,
                            library,
                            limits,
                            catalog,
                            expression,
                            &plan_owner(ir, plan),
                            &plan
                                .locals
                                .iter()
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect(),
                        )
                    })
            };
            match step {
                Step::Set { state, value } => {
                    let (evaluated, step_diagnostics) = evaluate(self, value);
                    diagnostics.extend(step_diagnostics);
                    if let Err(code) =
                        self.state
                            .write(state, evaluated, StateOrigin::Action, limits, library)
                    {
                        push_diagnostic(&mut diagnostics, code, Some(state.clone()), None);
                    } else {
                        applied += 1;
                    }
                }
                Step::Restore { states } => {
                    for state in states {
                        if self.state.restore(state) {
                            applied += 1;
                        }
                    }
                }
                Step::Refresh { query } => effects.push(Effect::Refresh {
                    query: query.clone(),
                }),
                Step::Say { text, ctx } => {
                    let (text, text_diagnostics) = evaluate(self, text);
                    diagnostics.extend(text_diagnostics);
                    let (ctx, ctx_diagnostics) = match ctx {
                        Some(ctx) => evaluate(self, ctx),
                        None => (Value::Null, Vec::new()),
                    };
                    diagnostics.extend(ctx_diagnostics);
                    effects.push(Effect::Say { text, ctx });
                }
                Step::Open { url } => {
                    let (url, step_diagnostics) = evaluate(self, url);
                    diagnostics.extend(step_diagnostics);
                    effects.push(Effect::Open { url });
                }
                Step::Emit { name, params } => {
                    let mut values = Vec::new();
                    for params in params {
                        let (value, step_diagnostics) = evaluate(self, params);
                        diagnostics.extend(step_diagnostics);
                        values.push(value);
                    }
                    let params = match values.len() {
                        0 => Value::Null,
                        1 => values.pop().unwrap_or(Value::Null),
                        _ => Value::List(Arc::new(values)),
                    };
                    effects.push(Effect::Emit {
                        name: name.clone(),
                        params,
                    });
                }
            }
        }
        sort_diagnostics(&mut diagnostics);
        ActionExecution {
            applied,
            effects,
            diagnostics,
        }
    }
}
fn plan_owner(ir: &Ir, plan: &crate::ir::ActionPlan) -> String {
    ir.nodes.get(&plan.origin).map_or_else(
        || format!("<action:{}>", plan.origin.0),
        |node| node.origin.clone(),
    )
}
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Refresh { query: String },
    Say { text: Value, ctx: Value },
    Open { url: Value },
    Emit { name: String, params: Value },
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActionExecution {
    pub applied: usize,
    /// Host-facing effects in execution order. The session turns these into
    /// epoch-bound requests; they are not delivered by this module.
    pub effects: Vec<Effect>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Evaluate a fresh IR with declaration defaults (§4.6 static evaluation).
pub fn evaluate_from_ir(ir: &Ir, library: &Library, limits: &Limits) -> Resolved {
    evaluate_with_catalog(ir, library, limits, &BTreeSet::new())
}
/// Evaluate with an explicit ref catalog and empty host caches (§4.6).
pub fn evaluate_with_catalog(
    ir: &Ir,
    library: &Library,
    limits: &Limits,
    catalog: &BTreeSet<String>,
) -> Resolved {
    evaluate_full(
        ir,
        library,
        limits,
        &StateStore::from_ir(ir),
        &BTreeMap::new(),
        &BTreeMap::new(),
        catalog,
    )
}

/// Evaluate all Dynamic props, capped per prop by `fuel_per_prop`.
pub fn evaluate_with_state(
    ir: &Ir,
    library: &Library,
    limits: &Limits,
    state: &StateStore,
) -> Resolved {
    evaluate_full(
        ir,
        library,
        limits,
        state,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeSet::new(),
    )
}

#[allow(clippy::too_many_arguments)]
fn evaluate_full(
    ir: &Ir,
    library: &Library,
    limits: &Limits,
    state: &StateStore,
    query_results: &BTreeMap<String, Value>,
    ref_values: &BTreeMap<String, Value>,
    catalog: &BTreeSet<String>,
) -> Resolved {
    let mut diagnostics = Vec::new();
    let values = evaluate_values(
        ir,
        library,
        limits,
        state,
        query_results,
        ref_values,
        catalog,
        &mut diagnostics,
    );
    let (_, resolved) = instantiate::Builder::build(instantiate::Inputs {
        ir,
        library,
        limits,
        state,
        values: &values,
        query_results,
        ref_values,
        catalog,
        diagnostics,
        queries: BTreeMap::new(),
    });
    resolved
}

fn sort_diagnostics(diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_by(|a, b| {
        (&a.stmt, &a.path, a.code, a.recovery).cmp(&(&b.stmt, &b.path, b.code, b.recovery))
    });
}
fn push_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    code: Code,
    stmt: Option<String>,
    path: Option<String>,
) {
    let mut diagnostic = Diagnostic::new(code, stmt, None);
    diagnostic.path = path;
    if !diagnostics.iter().any(|existing| {
        existing.code == diagnostic.code
            && existing.stmt == diagnostic.stmt
            && existing.path == diagnostic.path
            && existing.recovery == diagnostic.recovery
    }) {
        diagnostics.push(diagnostic);
    }
}

struct Context {
    stmt: String,
    path: Option<String>,
}

struct Evaluator<'a> {
    ir: &'a Ir,
    library: &'a Library,
    limits: &'a Limits,
    state: &'a StateStore,
    values: &'a BTreeMap<String, Value>,
    query_results: &'a BTreeMap<String, Value>,
    ref_values: &'a BTreeMap<String, Value>,
    ref_catalog: &'a BTreeSet<String>,
    queries: &'a mut BTreeMap<String, Value>,
    visiting_queries: BTreeSet<String>,
    diagnostics: &'a mut Vec<Diagnostic>,
    fuel: usize,
    failed: bool,
}
impl<'a> Evaluator<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        ir: &'a Ir,
        library: &'a Library,
        limits: &'a Limits,
        state: &'a StateStore,
        values: &'a BTreeMap<String, Value>,
        query_results: &'a BTreeMap<String, Value>,
        ref_values: &'a BTreeMap<String, Value>,
        ref_catalog: &'a BTreeSet<String>,
        queries: &'a mut BTreeMap<String, Value>,
        diagnostics: &'a mut Vec<Diagnostic>,
    ) -> Self {
        Self {
            ir,
            library,
            limits,
            state,
            values,
            query_results,
            ref_values,
            ref_catalog,
            queries,
            visiting_queries: BTreeSet::new(),
            diagnostics,
            fuel: limits.fuel_per_prop,
            failed: false,
        }
    }
    fn charge(&mut self, amount: usize, context: &Context) -> bool {
        if self.failed {
            return false;
        }
        if self.fuel < amount {
            self.fuel = 0;
            self.fail(Code::Fuel, context);
            false
        } else {
            self.fuel -= amount;
            true
        }
    }
    fn fail(&mut self, code: Code, context: &Context) -> Value {
        self.failed = true;
        push_diagnostic(
            self.diagnostics,
            code,
            Some(context.stmt.clone()),
            context.path.clone(),
        );
        Value::Null
    }
    fn eval(&mut self, e: &Expr, locals: &BTreeMap<String, Value>, context: &Context) -> Value {
        if !self.charge(1, context) {
            return Value::Null;
        }
        match e {
            Expr::Null { .. } => Value::Null,
            Expr::Bool { value } => Value::Bool(*value),
            Expr::Int { value } => Value::Int(*value),
            Expr::Float { value } => {
                if value.is_finite() {
                    Value::Float(*value)
                } else {
                    self.fail(Code::NonFinite, context)
                }
            }
            Expr::String { value } => Value::Str(PStr {
                text: self.cap_string(value, context),
                prov: Provenance::Model,
            }),
            Expr::Name { name } => {
                if let Some(value) = locals.get(name) {
                    return value.clone();
                }
                if let Some(value) = self.values.get(name) {
                    return value.clone();
                }
                if self.ir.queries.contains_key(name) {
                    return self.query_value(name, locals, context);
                }
                Value::Null
            }
            Expr::State { name } => match self.state.get(name) {
                Some(slot) if slot.origin == StateOrigin::User => {
                    retag(slot.value.clone(), Provenance::User)
                }
                Some(slot) => slot.value.clone(),
                None => Value::Null,
            },
            Expr::List { items } => {
                let cap = self.limits.max_list_len;
                if items.len() > cap {
                    push_diagnostic(
                        self.diagnostics,
                        Code::ListLen,
                        Some(context.stmt.clone()),
                        context.path.clone(),
                    );
                }
                let mut values = Vec::new();
                for item in items.iter().take(cap) {
                    if self.failed {
                        break;
                    }
                    values.push(self.eval(item, locals, context));
                }
                if self.failed {
                    Value::Null
                } else {
                    Value::List(Arc::new(values))
                }
            }
            Expr::Map { entries } => {
                let mut values = Vec::new();
                for entry in entries {
                    if self.failed {
                        break;
                    }
                    values.push((entry.key.clone(), self.eval(&entry.value, locals, context)));
                }
                if self.failed {
                    Value::Null
                } else {
                    Value::Map(Arc::new(values))
                }
            }
            Expr::Unary { op, expr } => {
                let value = self.eval(expr, locals, context);
                if self.failed {
                    return Value::Null;
                }
                match op {
                    UnaryOp::Not => match value {
                        Value::Bool(value) => Value::Bool(!value),
                        _ => self.fail(Code::ExprType, context),
                    },
                    UnaryOp::Neg => match value {
                        Value::Int(value) => match value.checked_neg() {
                            Some(value) => Value::Int(value),
                            None => self.fail(Code::Overflow, context),
                        },
                        Value::Float(value) if value.is_finite() => Value::Float(-value),
                        _ => self.fail(Code::ExprType, context),
                    },
                }
            }
            Expr::Binary { op, left, right } => {
                if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    let left = self.eval(left, locals, context);
                    if self.failed {
                        return Value::Null;
                    }
                    let Value::Bool(left) = left else {
                        return self.fail(Code::ExprType, context);
                    };
                    if (*op == BinaryOp::And && !left) || (*op == BinaryOp::Or && left) {
                        return Value::Bool(left);
                    }
                    let right = self.eval(right, locals, context);
                    if self.failed {
                        return Value::Null;
                    }
                    return match right {
                        Value::Bool(right) => Value::Bool(right),
                        _ => self.fail(Code::ExprType, context),
                    };
                }
                let left = self.eval(left, locals, context);
                if self.failed {
                    return Value::Null;
                }
                let right = self.eval(right, locals, context);
                if self.failed {
                    return Value::Null;
                }
                self.binary(*op, left, right, context)
            }
            Expr::Member { base, key } => {
                let base = self.eval(base, locals, context);
                if self.failed {
                    return Value::Null;
                }
                match base {
                    Value::Null => Value::Null,
                    Value::Map(entries) => entries
                        .iter()
                        .find(|(name, _)| name == key)
                        .map_or(Value::Null, |(_, value)| value.clone()),
                    _ => self.fail(Code::ExprType, context),
                }
            }
            Expr::Index { base, index } => {
                let base = self.eval(base, locals, context);
                if self.failed {
                    return Value::Null;
                }
                let index = self.eval(index, locals, context);
                if self.failed {
                    return Value::Null;
                }
                match (base, index) {
                    (Value::Null, _) => Value::Null,
                    (Value::List(items), Value::Int(index)) => {
                        if index < 0 {
                            return self.fail(Code::IndexOob, context);
                        }
                        match usize::try_from(index).ok().and_then(|i| items.get(i)) {
                            Some(value) => value.clone(),
                            None => self.fail(Code::IndexOob, context),
                        }
                    }
                    (Value::Map(entries), Value::Str(key)) => entries
                        .iter()
                        .find(|(name, _)| name.as_str() == key.text.as_ref())
                        .map_or(Value::Null, |(_, value)| value.clone()),
                    _ => self.fail(Code::ExprType, context),
                }
            }
            Expr::Lambda { .. } => self.fail(Code::LambdaMisuse, context),
            Expr::Call { name, args } => self.call(name, args, locals, context),
        }
    }
    fn binary(&mut self, op: BinaryOp, left: Value, right: Value, context: &Context) -> Value {
        match op {
            BinaryOp::Eq | BinaryOp::Ne => {
                let equal = values_equal(&left, &right);
                Value::Bool(if op == BinaryOp::Eq { equal } else { !equal })
            }
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                let ordering = match (&left, &right) {
                    (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
                    (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
                    (Value::Int(a), Value::Float(b)) => (*a as f64).partial_cmp(b),
                    (Value::Float(a), Value::Int(b)) => a.partial_cmp(&(*b as f64)),
                    (Value::Str(a), Value::Str(b)) => Some(a.text.as_ref().cmp(b.text.as_ref())),
                    _ => None,
                };
                let Some(ordering) = ordering else {
                    return self.fail(Code::ExprType, context);
                };
                Value::Bool(match op {
                    BinaryOp::Lt => ordering.is_lt(),
                    BinaryOp::Le => ordering.is_le(),
                    BinaryOp::Gt => ordering.is_gt(),
                    _ => ordering.is_ge(),
                })
            }
            BinaryOp::Add => match (left, right) {
                (Value::Int(a), Value::Int(b)) => match a.checked_add(b) {
                    Some(value) => Value::Int(value),
                    None => self.fail(Code::Overflow, context),
                },
                (Value::Int(a), Value::Float(b)) => self.float(a as f64 + b, context),
                (Value::Float(a), Value::Int(b)) => self.float(a + b as f64, context),
                (Value::Float(a), Value::Float(b)) => self.float(a + b, context),
                (Value::Str(a), Value::Str(b)) => Value::Str(PStr {
                    text: self.cap_string(&format!("{}{}", a.text, b.text), context),
                    prov: mix(a.prov, b.prov),
                }),
                _ => self.fail(Code::ExprType, context),
            },
            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => match (left, right) {
                (Value::Int(a), Value::Int(b)) => self.integer_op(op, a, b, context),
                (Value::Int(a), Value::Float(b)) => self.float_op(op, a as f64, b, context),
                (Value::Float(a), Value::Int(b)) => self.float_op(op, a, b as f64, context),
                (Value::Float(a), Value::Float(b)) => self.float_op(op, a, b, context),
                _ => self.fail(Code::ExprType, context),
            },
            BinaryOp::And | BinaryOp::Or => self.fail(Code::ExprType, context),
        }
    }
    fn integer_op(&mut self, op: BinaryOp, a: i64, b: i64, context: &Context) -> Value {
        let result = match op {
            BinaryOp::Sub => a.checked_sub(b),
            BinaryOp::Mul => a.checked_mul(b),
            BinaryOp::Div => {
                if b == 0 {
                    return self.fail(Code::DivZero, context);
                }
                a.checked_div(b)
            }
            BinaryOp::Rem => {
                if b == 0 {
                    return self.fail(Code::DivZero, context);
                }
                a.checked_rem(b)
            }
            _ => None,
        };
        match result {
            Some(value) => Value::Int(value),
            None => self.fail(Code::Overflow, context),
        }
    }
    fn float_op(&mut self, op: BinaryOp, a: f64, b: f64, context: &Context) -> Value {
        if matches!(op, BinaryOp::Div | BinaryOp::Rem) && b == 0.0 {
            return self.fail(Code::DivZero, context);
        }
        let value = match op {
            BinaryOp::Sub => a - b,
            BinaryOp::Mul => a * b,
            BinaryOp::Div => a / b,
            BinaryOp::Rem => a % b,
            _ => return self.fail(Code::ExprType, context),
        };
        self.float(value, context)
    }
    fn float(&mut self, value: f64, context: &Context) -> Value {
        if value.is_finite() {
            Value::Float(value)
        } else {
            self.fail(Code::NonFinite, context)
        }
    }
    fn call(
        &mut self,
        name: &str,
        args: &[Argument],
        locals: &BTreeMap<String, Value>,
        context: &Context,
    ) -> Value {
        match name {
            "if" => {
                let Some(condition) = args.first() else {
                    return self.fail(Code::ExprType, context);
                };
                let condition = self.eval(&condition.value, locals, context);
                if self.failed {
                    return Value::Null;
                }
                let Value::Bool(condition) = condition else {
                    return self.fail(Code::ExprType, context);
                };
                let Some(branch) = args.get(if condition { 1 } else { 2 }) else {
                    return self.fail(Code::ExprType, context);
                };
                self.eval(&branch.value, locals, context)
            }
            "coalesce" => {
                for argument in args {
                    let value = self.eval(&argument.value, locals, context);
                    if self.failed {
                        return Value::Null;
                    }
                    if value != Value::Null {
                        return value;
                    }
                }
                Value::Null
            }
            "ref" => match args.first().map(|a| &a.value) {
                Some(Expr::String { value }) => {
                    if !valid_ref_id(value) {
                        self.fail(Code::RefInvalid, context)
                    } else if !self.ref_catalog.contains(value) {
                        self.fail(Code::RefUnknown, context)
                    } else if let Some(value) = self.ref_values.get(value) {
                        value.clone()
                    } else {
                        // No host data before delivery (§7.7): keep the placeholder.
                        Value::Ref(value.clone())
                    }
                }
                _ => self.fail(Code::RefInvalid, context),
            },
            "each" => self.fail(Code::ExprType, context),
            _ => {
                if let Some(function) = self.library.function(name).cloned() {
                    if function.params.len() != args.len()
                        || args.iter().any(|argument| argument.name.is_some())
                    {
                        return self.fail(Code::ExprType, context);
                    }
                    let mut values = Vec::with_capacity(args.len());
                    for argument in args {
                        values.push(self.eval(&argument.value, locals, context));
                        if self.failed {
                            return Value::Null;
                        }
                    }
                    if !self.charge(
                        usize::try_from(function.fuel).unwrap_or(usize::MAX),
                        context,
                    ) {
                        return Value::Null;
                    }
                    let Some(implementation) = function.implementation else {
                        return self.fail(Code::UnknownFn, context);
                    };
                    let value = implementation(&values);
                    if !value.is_finite() {
                        return self.fail(Code::NonFinite, context);
                    }
                    value
                } else {
                    self.builtin(name, args, locals, context)
                }
            }
        }
    }
    fn builtin(
        &mut self,
        name: &str,
        args: &[Argument],
        locals: &BTreeMap<String, Value>,
        context: &Context,
    ) -> Value {
        match name {
            "map" | "filter" | "sort_by" => self.higher_order(name, args, locals, context),
            "len" => {
                let value = self.first(args, locals, context);
                if self.failed {
                    return Value::Null;
                }
                let length = match value {
                    Value::List(items) => items.len(),
                    Value::Map(entries) => entries.len(),
                    Value::Str(text) => text.text.chars().count(),
                    _ => return self.fail(Code::ExprType, context),
                };
                Value::Int(i64::try_from(length).unwrap_or(i64::MAX))
            }
            "first" | "last" => {
                let value = self.first(args, locals, context);
                if self.failed {
                    return Value::Null;
                }
                match value {
                    Value::List(items) => {
                        let item = if name == "first" {
                            items.first()
                        } else {
                            items.last()
                        };
                        item.cloned().unwrap_or(Value::Null)
                    }
                    _ => self.fail(Code::ExprType, context),
                }
            }
            "sum" | "avg" | "min" | "max" => self.aggregate(name, args, locals, context),
            "round" | "floor" | "ceil" | "abs" => self.rounding(name, args, locals, context),
            "fixed" => self.fixed(args, locals, context),
            "fmt" => self.format(args, locals, context),
            "text" => {
                let value = self.first(args, locals, context);
                if self.failed {
                    return Value::Null;
                }
                match scalar_text(&value) {
                    Some(text) => Value::Str(PStr {
                        text: self.cap_string(&text, context),
                        prov: provenance(&value),
                    }),
                    None => self.fail(Code::ExprType, context),
                }
            }
            "upper" | "lower" => {
                let value = self.first(args, locals, context);
                if self.failed {
                    return Value::Null;
                }
                match value {
                    Value::Str(text) => {
                        let prov = text.prov;
                        let text = if name == "upper" {
                            text.text.to_uppercase()
                        } else {
                            text.text.to_lowercase()
                        };
                        Value::Str(PStr {
                            text: self.cap_string(&text, context),
                            prov,
                        })
                    }
                    _ => self.fail(Code::ExprType, context),
                }
            }
            "contains" => {
                let haystack = self.first(args, locals, context);
                if self.failed {
                    return Value::Null;
                }
                let needle = self.nth(args, 1, locals, context);
                if self.failed {
                    return Value::Null;
                }
                match (haystack, needle) {
                    (Value::Str(a), Value::Str(b)) => Value::Bool(a.text.contains(b.text.as_ref())),
                    _ => self.fail(Code::ExprType, context),
                }
            }
            "join" => {
                let list = self.first(args, locals, context);
                if self.failed {
                    return Value::Null;
                }
                let separator = self.nth(args, 1, locals, context);
                if self.failed {
                    return Value::Null;
                }
                let Value::Str(separator) = separator else {
                    return self.fail(Code::ExprType, context);
                };
                let Value::List(items) = list else {
                    return self.fail(Code::ExprType, context);
                };
                let mut text = String::new();
                let mut prov = Provenance::Model;
                for (index, item) in items.iter().enumerate() {
                    let Value::Str(item) = item else {
                        return self.fail(Code::ExprType, context);
                    };
                    if index > 0 {
                        text.push_str(&separator.text);
                        prov = mix(prov, separator.prov);
                    }
                    text.push_str(&item.text);
                    prov = mix(prov, item.prov);
                }
                Value::Str(PStr {
                    text: self.cap_string(&text, context),
                    prov,
                })
            }
            _ => self.fail(Code::UnknownFn, context),
        }
    }
    fn first(
        &mut self,
        args: &[Argument],
        locals: &BTreeMap<String, Value>,
        context: &Context,
    ) -> Value {
        self.nth(args, 0, locals, context)
    }
    fn nth(
        &mut self,
        args: &[Argument],
        index: usize,
        locals: &BTreeMap<String, Value>,
        context: &Context,
    ) -> Value {
        match args.get(index) {
            Some(argument) => self.eval(&argument.value, locals, context),
            None => self.fail(Code::ExprType, context),
        }
    }
    fn higher_order(
        &mut self,
        name: &str,
        args: &[Argument],
        locals: &BTreeMap<String, Value>,
        context: &Context,
    ) -> Value {
        let collection = self.first(args, locals, context);
        if self.failed {
            return Value::Null;
        }
        let Value::List(items) = collection else {
            return self.fail(Code::ExprType, context);
        };
        let Some(Argument {
            value: Expr::Lambda { params, body },
            ..
        }) = args.get(1)
        else {
            return self.fail(Code::ExprType, context);
        };
        if params.len() != 1 {
            return self.fail(Code::ExprType, context);
        }
        if name == "sort_by" {
            let descending = match args.iter().find(|a| a.name.as_deref() == Some("desc")) {
                Some(argument) => match self.eval(&argument.value, locals, context) {
                    Value::Bool(value) => value,
                    _ => return self.fail(Code::ExprType, context),
                },
                None => false,
            };
            let mut keyed = Vec::with_capacity(items.len());
            for item in items.iter() {
                if !self.charge(1, context) {
                    return Value::Null;
                }
                let mut scope = locals.clone();
                scope.insert(params[0].clone(), item.clone());
                let key = self.eval(body, &scope, context);
                if self.failed {
                    return Value::Null;
                }
                keyed.push((key, item.clone()));
            }
            if keyed.iter().any(|(key, _)| key.number().is_none())
                && keyed.iter().any(|(key, _)| key.string().is_none())
            {
                return self.fail(Code::ExprType, context);
            }
            let size = keyed.len();
            let sort_cost = size.saturating_mul(if size <= 1 {
                0
            } else {
                usize::BITS as usize - (size - 1).leading_zeros() as usize
            });
            if !self.charge(sort_cost, context) {
                return Value::Null;
            }
            keyed.sort_by(|(a, _), (b, _)| {
                let ordering = match (a, b) {
                    (Value::Int(a), Value::Int(b)) => a.cmp(b),
                    (Value::Float(a), Value::Float(b)) => {
                        a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    (Value::Int(a), Value::Float(b)) => (*a as f64)
                        .partial_cmp(b)
                        .unwrap_or(std::cmp::Ordering::Equal),
                    (Value::Float(a), Value::Int(b)) => a
                        .partial_cmp(&(*b as f64))
                        .unwrap_or(std::cmp::Ordering::Equal),
                    (Value::Str(a), Value::Str(b)) => a.text.cmp(&b.text),
                    _ => std::cmp::Ordering::Equal,
                };
                if descending {
                    ordering.reverse()
                } else {
                    ordering
                }
            });
            return Value::List(Arc::new(keyed.into_iter().map(|(_, item)| item).collect()));
        }
        let cap = self.limits.max_list_len;
        let limit = if name == "filter" { items.len() } else { cap };
        let mut output = Vec::new();
        for item in items.iter().take(limit) {
            if !self.charge(1, context) {
                return Value::Null;
            }
            let mut scope = locals.clone();
            scope.insert(params[0].clone(), item.clone());
            let value = self.eval(body, &scope, context);
            if self.failed {
                return Value::Null;
            }
            match name {
                "map" => {
                    if output.len() < cap {
                        output.push(value);
                    }
                }
                "filter" => match value {
                    Value::Bool(true) => output.push(item.clone()),
                    Value::Bool(false) => {}
                    _ => return self.fail(Code::ExprType, context),
                },
                _ => return self.fail(Code::UnknownFn, context),
            }
        }
        if name == "map" && items.len() > cap {
            push_diagnostic(
                self.diagnostics,
                Code::ListLen,
                Some(context.stmt.clone()),
                context.path.clone(),
            );
        }
        Value::List(Arc::new(output))
    }
    fn aggregate(
        &mut self,
        name: &str,
        args: &[Argument],
        locals: &BTreeMap<String, Value>,
        context: &Context,
    ) -> Value {
        let collection = self.first(args, locals, context);
        if self.failed {
            return Value::Null;
        }
        let Value::List(items) = collection else {
            return self.fail(Code::ExprType, context);
        };
        let field = args.get(1).map(|argument| &argument.value);
        if let Some(field) = field {
            let Expr::String { value } = field else {
                return self.fail(Code::ExprType, context);
            };
            let path: Vec<_> = value.split('.').collect();
            let mut ints = Vec::with_capacity(items.len());
            let mut numbers = Vec::with_capacity(items.len());
            let mut all_int = true;
            for item in items.iter() {
                let mut current = item.clone();
                for part in &path {
                    match current {
                        Value::Map(entries) => {
                            current = entries
                                .iter()
                                .find(|(key, _)| key == part)
                                .map_or(Value::Null, |(_, value)| value.clone());
                        }
                        _ => {
                            current = Value::Null;
                        }
                    }
                }
                match current {
                    Value::Int(value) => {
                        ints.push(value);
                        numbers.push(value as f64);
                    }
                    Value::Float(value) => {
                        all_int = false;
                        numbers.push(value);
                    }
                    _ => return self.fail(Code::ExprType, context),
                }
            }
            return self.aggregate_numbers(name, &ints, &numbers, all_int, context);
        }
        let mut ints = Vec::with_capacity(items.len());
        let mut numbers = Vec::with_capacity(items.len());
        let mut all_int = true;
        for item in items.iter() {
            match item {
                Value::Int(value) => {
                    ints.push(*value);
                    numbers.push(*value as f64);
                }
                Value::Float(value) => {
                    all_int = false;
                    numbers.push(*value);
                }
                _ => return self.fail(Code::ExprType, context),
            }
        }
        self.aggregate_numbers(name, &ints, &numbers, all_int, context)
    }
    fn aggregate_numbers(
        &mut self,
        name: &str,
        ints: &[i64],
        numbers: &[f64],
        all_int: bool,
        context: &Context,
    ) -> Value {
        match name {
            "sum" => {
                if all_int {
                    let mut total: i64 = 0;
                    for value in ints {
                        match total.checked_add(*value) {
                            Some(value) => total = value,
                            None => return self.fail(Code::Overflow, context),
                        }
                    }
                    Value::Int(total)
                } else {
                    self.float(numbers.iter().sum(), context)
                }
            }
            "avg" => {
                if numbers.is_empty() {
                    Value::Null
                } else {
                    self.float(numbers.iter().sum::<f64>() / numbers.len() as f64, context)
                }
            }
            "min" | "max" => {
                if numbers.is_empty() {
                    Value::Null
                } else if all_int {
                    let mut selected = ints[0];
                    for value in &ints[1..] {
                        if (name == "min" && *value < selected)
                            || (name == "max" && *value > selected)
                        {
                            selected = *value;
                        }
                    }
                    Value::Int(selected)
                } else {
                    let value = numbers
                        .iter()
                        .copied()
                        .reduce(|a, b| if name == "min" { a.min(b) } else { a.max(b) });
                    match value {
                        Some(value) => self.float(value, context),
                        None => Value::Null,
                    }
                }
            }
            _ => self.fail(Code::UnknownFn, context),
        }
    }
    fn rounding(
        &mut self,
        name: &str,
        args: &[Argument],
        locals: &BTreeMap<String, Value>,
        context: &Context,
    ) -> Value {
        let value = self.first(args, locals, context);
        if self.failed {
            return Value::Null;
        }
        match value {
            Value::Int(value) => match name {
                "abs" => match value.checked_abs() {
                    Some(value) => Value::Int(value),
                    None => self.fail(Code::Overflow, context),
                },
                _ => Value::Int(value),
            },
            Value::Float(value) => {
                let value = match name {
                    "round" => value.round(),
                    "floor" => value.floor(),
                    "ceil" => value.ceil(),
                    _ => value.abs(),
                };
                self.float(value, context)
            }
            _ => self.fail(Code::ExprType, context),
        }
    }
    fn fixed(
        &mut self,
        args: &[Argument],
        locals: &BTreeMap<String, Value>,
        context: &Context,
    ) -> Value {
        let value = self.first(args, locals, context);
        if self.failed {
            return Value::Null;
        }
        let decimals = self.nth(args, 1, locals, context);
        if self.failed {
            return Value::Null;
        }
        let Value::Int(decimals) = decimals else {
            return self.fail(Code::ExprType, context);
        };
        if !(0..=100).contains(&decimals) {
            return self.fail(Code::ExprType, context);
        }
        let number = match value.number() {
            Some(number) => number,
            None => return self.fail(Code::ExprType, context),
        };
        let text = format!("{number:.decimals$}", decimals = decimals as usize);
        Value::Str(PStr {
            text: self.cap_string(&text, context),
            prov: Provenance::Model,
        })
    }
    fn format(
        &mut self,
        args: &[Argument],
        locals: &BTreeMap<String, Value>,
        context: &Context,
    ) -> Value {
        let template = self.first(args, locals, context);
        if self.failed {
            return Value::Null;
        }
        let Value::Str(template) = template else {
            return self.fail(Code::ExprType, context);
        };
        let mut rendered = Vec::new();
        let mut prov = template.prov;
        for argument in args.iter().skip(1) {
            let value = self.eval(&argument.value, locals, context);
            if self.failed {
                return Value::Null;
            }
            prov = mix(prov, provenance(&value));
            match scalar_text(&value) {
                Some(text) => rendered.push(text),
                None => return self.fail(Code::ExprType, context),
            }
        }
        let mut output = String::new();
        let mut used = 0usize;
        let mut chars = template.text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '{' if chars.peek() == Some(&'{') => {
                    chars.next();
                    output.push('{');
                }
                '}' if chars.peek() == Some(&'}') => {
                    chars.next();
                    output.push('}');
                }
                '{' if chars.peek() == Some(&'}') => {
                    chars.next();
                    if let Some(text) = rendered.get(used) {
                        output.push_str(text);
                    }
                    used += 1;
                }
                other => output.push(other),
            }
        }
        if used != rendered.len() {
            push_diagnostic(
                self.diagnostics,
                Code::FmtArgs,
                Some(context.stmt.clone()),
                context.path.clone(),
            );
        }
        Value::Str(PStr {
            text: self.cap_string(&output, context),
            prov,
        })
    }
    fn query_value(
        &mut self,
        name: &str,
        locals: &BTreeMap<String, Value>,
        context: &Context,
    ) -> Value {
        if let Some(value) = self.query_results.get(name) {
            return value.clone();
        }
        if let Some(value) = self.queries.get(name) {
            return value.clone();
        }
        if !self.visiting_queries.insert(name.into()) {
            return Value::Null;
        }
        let value = match self.ir.queries.get(name).cloned() {
            Some(query) => self.eval(&query.default, locals, context),
            None => Value::Null,
        };
        self.visiting_queries.remove(name);
        self.queries.insert(name.into(), value.clone());
        value
    }
    fn cap_string(&mut self, text: &str, context: &Context) -> Arc<str> {
        let limit = self.limits.syntax.max_string_bytes;
        if text.len() <= limit {
            return text.into();
        }
        push_diagnostic(
            self.diagnostics,
            Code::StringSize,
            Some(context.stmt.clone()),
            context.path.clone(),
        );
        let mut end = limit.min(text.len());
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }
        text[..end].into()
    }
}

#[allow(clippy::too_many_arguments)]
fn evaluate_values(
    ir: &Ir,
    library: &Library,
    limits: &Limits,
    state: &StateStore,
    query_results: &BTreeMap<String, Value>,
    ref_values: &BTreeMap<String, Value>,
    catalog: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeMap<String, Value> {
    let mut values = BTreeMap::new();
    let order = value_order(ir);
    let mut queries = BTreeMap::new();
    for name in order {
        let Some(entry) = ir.values.get(&name) else {
            continue;
        };
        if entry.kind != crate::ir::Kind::Value
            || entry.source_kind == crate::syntax::ast::StatementKind::Query
        {
            continue;
        }
        if let Some(value) = &entry.value {
            values.insert(name, value.clone());
            continue;
        }
        let seen = values.clone();
        let mut evaluator = Evaluator::new(
            ir,
            library,
            limits,
            state,
            &seen,
            query_results,
            ref_values,
            catalog,
            &mut queries,
            diagnostics,
        );
        let value = evaluator.eval(
            &entry.expression,
            &BTreeMap::new(),
            &Context {
                stmt: name.clone(),
                path: None,
            },
        );
        values.insert(name, if evaluator.failed { Value::Null } else { value });
    }
    values
}
fn value_order(ir: &Ir) -> Vec<String> {
    let names: BTreeSet<_> = ir.values.keys().cloned().collect();
    let mut dependencies: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (name, entry) in &ir.values {
        let mut referenced = BTreeSet::new();
        expression_names(&entry.expression, &mut BTreeSet::new(), &mut referenced);
        referenced.retain(|dependency| {
            names.contains(dependency) && dependency.as_str() != name.as_str()
        });
        dependencies.insert(name.clone(), referenced);
    }
    let mut order = Vec::new();
    let mut ready: BTreeSet<String> = dependencies
        .iter()
        .filter(|(_, deps)| deps.is_empty())
        .map(|(name, _)| name.clone())
        .collect();
    let mut remaining = dependencies.clone();
    while let Some(name) = ready.pop_first() {
        order.push(name.clone());
        remaining.remove(&name);
        let next: Vec<_> = remaining
            .iter()
            .filter(|(_, deps)| deps.contains(&name))
            .map(|(candidate, _)| candidate.clone())
            .collect();
        for candidate in next {
            if let Some(deps) = remaining.get_mut(&candidate) {
                deps.remove(&name);
                if deps.is_empty() {
                    ready.insert(candidate);
                }
            }
        }
    }
    // Static cycles are already diagnostics; retain deterministic coverage so a
    // runtime name read still resolves to Null instead of disappearing.
    order.extend(remaining.keys().cloned());
    order
}
fn expression_names(e: &Expr, locals: &mut BTreeSet<String>, out: &mut BTreeSet<String>) {
    match e {
        Expr::Name { name } => {
            if !locals.contains(name) {
                out.insert(name.clone());
            }
        }
        Expr::List { items } => {
            for item in items {
                expression_names(item, locals, out);
            }
        }
        Expr::Map { entries } => {
            for entry in entries {
                expression_names(&entry.value, locals, out);
            }
        }
        Expr::Call { args, .. } => {
            for argument in args {
                expression_names(&argument.value, locals, out);
            }
        }
        Expr::Unary { expr, .. } | Expr::Member { base: expr, .. } => {
            expression_names(expr, locals, out)
        }
        Expr::Binary { left, right, .. }
        | Expr::Index {
            base: left,
            index: right,
        } => {
            expression_names(left, locals, out);
            expression_names(right, locals, out);
        }
        Expr::Lambda { params, body } => {
            let mut nested = locals.clone();
            nested.extend(params.iter().cloned());
            expression_names(body, &mut nested, out);
        }
        _ => {}
    }
}

fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
        (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
        (Value::List(a), Value::List(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| values_equal(a, b))
        }
        (Value::Map(a), Value::Map(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|((ak, av), (bk, bv))| ak == bk && values_equal(av, bv))
        }
        (a, b) => a == b,
    }
}
fn mix(a: Provenance, b: Provenance) -> Provenance {
    if a == b { a } else { Provenance::Mixed }
}
fn provenance(value: &Value) -> Provenance {
    match value {
        Value::Str(text) => text.prov,
        _ => Provenance::Model,
    }
}
fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => Some(String::new()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Int(value) => Some(value.to_string()),
        Value::Float(value) => value.is_finite().then(|| format_float(*value)),
        Value::Str(text) => Some(text.text.to_string()),
        _ => None,
    }
}
/// Shortest round-trip float text (§2.6).
pub fn format_float(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0".into()
        } else {
            "0".into()
        };
    }
    let exponent = format!("{value:e}");
    let Some((mantissa, exponent)) = exponent.split_once('e') else {
        return exponent;
    };
    let Ok(exponent) = exponent.parse::<i32>() else {
        return format!("{value}");
    };
    let negative = mantissa.starts_with('-');
    let mantissa = mantissa.trim_start_matches('-');
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let decimal_exponent = exponent;
    if (-5..=16).contains(&decimal_exponent) {
        let mut output = String::new();
        if negative {
            output.push('-');
        }
        if decimal_exponent < 0 {
            output.push_str("0.");
            for _ in 0..(-decimal_exponent - 1) {
                output.push('0');
            }
            output.push_str(&digits);
        } else {
            let point = (decimal_exponent + 1) as usize;
            if point >= digits.len() {
                output.push_str(&digits);
                for _ in digits.len()..point {
                    output.push('0');
                }
            } else {
                output.push_str(&digits[..point]);
                output.push('.');
                output.push_str(&digits[point..]);
            }
        }
        output
    } else {
        let mut output = String::new();
        if negative {
            output.push('-');
        }
        output.push(digits.chars().next().unwrap_or('0'));
        if digits.len() > 1 {
            output.push('.');
            output.push_str(&digits[1..]);
        }
        output.push('e');
        if decimal_exponent >= 0 {
            output.push('+');
        }
        output.push_str(&decimal_exponent.to_string());
        output
    }
}

/// Ref ids match `[A-Za-z0-9_.:-]{1,128}` (§7.7).
pub fn valid_ref_id(id: &str) -> bool {
    (1..=128).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-'))
}

/// Tag every string in delivered data with the request's provenance (§7.6).
pub fn retag(value: Value, prov: Provenance) -> Value {
    match value {
        Value::Str(text) => Value::Str(PStr {
            text: text.text,
            prov,
        }),
        Value::List(items) => Value::List(Arc::new(
            items.iter().cloned().map(|v| retag(v, prov)).collect(),
        )),
        Value::Map(entries) => Value::Map(Arc::new(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), retag(value.clone(), prov)))
                .collect(),
        )),
        other => other,
    }
}

/// True when delivered host data still contains an unresolved ref (§7.7 nesting).
pub fn contains_ref(value: &Value) -> bool {
    match value {
        Value::Ref(_) => true,
        Value::List(items) => items.iter().any(contains_ref),
        Value::Map(entries) => entries.iter().any(|(_, value)| contains_ref(value)),
        _ => false,
    }
}

/// Canonical data size used by `max_resolved_data_bytes` and `max_state_bytes`.
pub fn value_bytes(value: &Value) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len())
}

/// Resolved retained tree in the §12.2 canonical shape. `provenance` emits
/// `{"s":text,"prov":tag}` for strings; it is used only by provenance fixtures.
pub fn resolved_tree_json(
    ir: &Ir,
    library: &Library,
    resolved: &Resolved,
    provenance: bool,
) -> serde_json::Value {
    fn visit(
        ir: &Ir,
        library: &Library,
        resolved: &Resolved,
        id: NodeId,
        provenance: bool,
    ) -> serde_json::Value {
        use serde_json::{Value as J, json};
        let Some(node) = ir.nodes.get(&id) else {
            return J::Null;
        };
        let Some(component) = library.components().get(node.kind.0 as usize) else {
            return J::Null;
        };
        let props: serde_json::Map<_, _> = node
            .props
            .iter()
            .filter_map(|(prop_id, prop)| {
                let arg = component.args.get(prop_id.0 as usize)?;
                let value = match prop {
                    Prop::Dynamic(_) => {
                        match resolved.node_props(id).and_then(|props| props.get(prop_id)) {
                            Some(PropValue::Value(value)) => value_json(value, provenance),
                            _ => return None,
                        }
                    }
                    Prop::Static(PropValue::Value(value)) => value_json(value, provenance),
                    Prop::Static(PropValue::Token { scale, value }) => {
                        json!({"tok":[scale,value.plain_json()]})
                    }
                    Prop::Static(PropValue::Binding(name)) => json!({"bind":format!("${name}")}),
                    Prop::Static(PropValue::Action(id)) => json!({"action":id.0.as_ref()}),
                };
                Some((arg.name.clone(), value))
            })
            .collect();
        let children: serde_json::Map<_, _> = node
            .slots
            .iter()
            .map(|slot| {
                (
                    slot.name.clone(),
                    J::Array(
                        slot.children
                            .iter()
                            .map(|child| visit(ir, library, resolved, *child, provenance))
                            .collect(),
                    ),
                )
            })
            .collect();
        json!({
            "id": node.path.0,
            "kind": component.name,
            "role": node.role,
            "props": props,
            "status": node.status,
            "children": children,
        })
    }
    ir.root.map_or(serde_json::Value::Null, |root| {
        visit(ir, library, resolved, root, provenance)
    })
}
fn value_json(value: &Value, provenance: bool) -> serde_json::Value {
    use serde_json::{Value as J, json};
    match value {
        Value::Str(text) if provenance => {
            json!({"s": text.text.as_ref(), "prov": prov_name(text.prov)})
        }
        Value::Str(text) => J::String(crate::patch::strip_invisibles(&text.text)),
        Value::List(items) => J::Array(items.iter().map(|v| value_json(v, provenance)).collect()),
        Value::Map(items) => {
            json!({"map":items.iter().map(|(k,v)|json!([k,value_json(v,provenance)])).collect::<Vec<_>>()})
        }
        other => other.ir_json(),
    }
}
fn prov_name(prov: Provenance) -> &'static str {
    match prov {
        Provenance::Model => "model",
        Provenance::Host => "host",
        Provenance::Tool => "tool",
        Provenance::User => "user",
        Provenance::Mixed => "mixed",
    }
}

#[cfg(test)]
mod tests;
