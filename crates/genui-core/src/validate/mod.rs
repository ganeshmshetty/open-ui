//! Static resolution and validation (§§2.10–2.15,4,5,7.1). No requests or evaluation.
mod expression;
mod tree;
use crate::{
    diag::{Code, Diagnostic, Recovery},
    ir::*,
    r#lib::{CAPABILITIES, Library, T},
    limits::Limits,
    syntax::{
        ast::{Argument, Expr, Program, Statement, StatementKind},
        parse_batch,
    },
    value::Value,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    pub limits: Limits,
    pub capabilities: BTreeSet<String>,
    pub tools: BTreeSet<String>,
    pub refs: BTreeSet<String>,
    pub url_hosts: BTreeSet<String>,
    pub url_schemes: BTreeSet<String>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            capabilities: CAPABILITIES.iter().map(|s| (*s).into()).collect(),
            tools: BTreeSet::new(),
            refs: BTreeSet::new(),
            url_hosts: BTreeSet::new(),
            url_schemes: ["https".into()].into(),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Validation {
    pub ir: Ir,
    pub diagnostics: Vec<Diagnostic>,
}
pub fn validate(bytes: &[u8], library: &Library, options: &Options) -> Validation {
    let parsed = parse_batch(bytes, options.limits.clone().bounded().syntax);
    validate_program(&parsed.program, parsed.diagnostics, library, options)
}
/// Extract a model reply, then validate its sole selected program with the same batch path.
pub fn validate_message(
    bytes: &[u8],
    library: &Library,
    options: &Options,
) -> (crate::syntax::extract::ExtractResult, Validation) {
    let (extracted, parsed) =
        crate::syntax::parse_message(bytes, options.limits.clone().bounded().syntax);
    let result = validate_program(&parsed.program, parsed.diagnostics, library, options);
    (extracted, result)
}
/// Accepts the sealed statement table. Its ordering is immaterial; syntax cutoffs precede this API.
pub fn validate_program(
    program: &Program,
    diagnostics: Vec<Diagnostic>,
    library: &Library,
    options: &Options,
) -> Validation {
    if diagnostics
        .iter()
        .any(|d| d.severity == crate::diag::Severity::Fatal)
    {
        return Validation {
            ir: Ir::default(),
            diagnostics,
        };
    }
    Validator::new(program, diagnostics, library, options).run()
}
/// Reuse unaffected concrete static instances while revisiting canonical budgets.
/// Expression/action arenas remain batch-owned; deferred instances take the regular path.
pub(crate) fn validate_incremental(
    program: &Program,
    library: &Library,
    options: &Options,
    previous: &Validation,
    dirty: &BTreeSet<String>,
) -> (Validation, usize) {
    let count = std::cell::Cell::new(0);
    let mut validator = Validator::new(program, Vec::new(), library, options);
    validator.reuse = Some(Reuse {
        previous,
        dirty,
        count: &count,
    });
    let validation = validator.run();
    (validation, count.get())
}
struct Reuse<'a> {
    previous: &'a Validation,
    dirty: &'a BTreeSet<String>,
    count: &'a std::cell::Cell<usize>,
}
#[derive(Clone, Debug)]
struct Checked {
    value: Option<Value>,
    ty: T,
    valid: bool,
}
impl Checked {
    fn literal(v: Value) -> Self {
        Self {
            ty: T::infer(&v),
            value: Some(v),
            valid: true,
        }
    }
    fn dynamic(ty: T) -> Self {
        Self {
            value: None,
            ty,
            valid: true,
        }
    }
    fn invalid() -> Self {
        Self {
            value: Some(Value::Null),
            ty: T::Any,
            valid: false,
        }
    }
}
#[derive(Clone)]
struct Context {
    stmt: String,
    path: Option<NodePath>,
    locals: BTreeSet<String>,
    accepts: Vec<String>,
    form_scope: Option<NodeId>,
    list_cap: Option<usize>,
    budget_stmt: Option<String>,
}
impl Context {
    fn at(stmt: &str, path: Option<NodePath>) -> Self {
        Self {
            stmt: stmt.into(),
            path,
            locals: BTreeSet::new(),
            accepts: Vec::new(),
            form_scope: None,
            list_cap: None,
            budget_stmt: None,
        }
    }
}
struct TemplateCheck {
    expression: Expr,
    ctx: Context,
    path: NodePath,
    place: Option<String>,
}
struct Validator<'a> {
    reuse: Option<Reuse<'a>>,
    lib: &'a Library,
    options: &'a Options,
    limits: Limits,
    table: BTreeMap<String, Statement>,
    state_sources: BTreeMap<String, Statement>,
    kinds: BTreeMap<String, Kind>,
    ir: Ir,
    diagnostics: Vec<Diagnostic>,
    cache: BTreeMap<String, Checked>,
    action_sources: BTreeMap<String, Expr>,
    reachable: BTreeSet<String>,
    ids: IdAllocator,
    active: BTreeSet<String>,
    allocated: usize,
    limited: BTreeSet<(String, Code)>,
    checking_template: bool,
    checking_orphan: bool,
    pending_plans: Vec<SlotPlan>,
    template_queue: VecDeque<TemplateCheck>,
    template_seen: BTreeSet<(String, Vec<String>)>,
}
fn names(e: &Expr, out: &mut BTreeSet<String>) {
    match e {
        Expr::Name { name } => {
            out.insert(name.clone());
        }
        Expr::List { items } => {
            for e in items {
                names(e, out)
            }
        }
        Expr::Map { entries } => {
            for e in entries {
                names(&e.value, out)
            }
        }
        Expr::Call { args, .. } => {
            for a in args {
                names(&a.value, out)
            }
        }
        Expr::Unary { expr, .. } | Expr::Member { base: expr, .. } => names(expr, out),
        Expr::Binary { left, right, .. }
        | Expr::Index {
            base: left,
            index: right,
        } => {
            names(left, out);
            names(right, out)
        }
        Expr::Lambda { params, body } => {
            let mut n = BTreeSet::new();
            names(body, &mut n);
            out.extend(n.into_iter().filter(|n| !params.contains(n)));
        }
        _ => {}
    }
}
impl<'a> Validator<'a> {
    fn new(
        p: &Program,
        diagnostics: Vec<Diagnostic>,
        lib: &'a Library,
        options: &'a Options,
    ) -> Self {
        let mut table = BTreeMap::new();
        let mut state_sources = BTreeMap::new();
        for s in &p.statements {
            if s.kind == StatementKind::State {
                state_sources
                    .entry(s.name.clone())
                    .or_insert_with(|| s.clone());
            } else {
                table.entry(s.name.clone()).or_insert_with(|| s.clone());
            }
        }
        let kinds = table.keys().map(|n| (n.clone(), Kind::Value)).collect();
        Self {
            reuse: None,
            lib,
            options,
            limits: options.limits.clone().bounded(),
            table,
            state_sources,
            kinds,
            ir: Ir::default(),
            diagnostics,
            cache: BTreeMap::new(),
            action_sources: BTreeMap::new(),
            reachable: BTreeSet::new(),
            ids: IdAllocator::default(),
            active: BTreeSet::new(),
            allocated: 0,
            limited: BTreeSet::new(),
            checking_template: false,
            checking_orphan: false,
            pending_plans: Vec::new(),
            template_queue: VecDeque::new(),
            template_seen: BTreeSet::new(),
        }
    }
    fn emit(&mut self, code: Code, ctx: &Context) -> usize {
        let mut d = Diagnostic::new(code, Some(ctx.stmt.clone()), None);
        if !self.checking_orphan {
            d.path = ctx.path.as_ref().map(|p| p.0.clone());
        }
        self.diagnostics.push(d);
        self.diagnostics.len() - 1
    }
    fn recovery(&mut self, code: Code, recovery: Recovery, ctx: &Context) {
        let i = self.emit(code, ctx);
        if let Some(d) = self.diagnostics.get_mut(i) {
            d.recovery = recovery
        }
    }
    fn limit(&mut self, code: Code, ctx: &Context) {
        if !matches!(code, Code::Nodes | Code::IrDepth)
            || self.limited.insert((ctx.stmt.clone(), code))
        {
            self.emit(code, ctx);
        }
    }
    fn classify(&self, e: &Expr) -> Kind {
        match e {
            Expr::Name { name } => self.kinds.get(name).copied().unwrap_or(Kind::Value),
            Expr::Call { name, args } => {
                if self.lib.component(name).is_some()
                    || name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                    || name == "each"
                {
                    Kind::Node
                } else if crate::r#lib::ACTIONS.contains(&name.as_str())
                    || self.lib.action(name).is_some()
                {
                    Kind::Action
                } else if name == "if" || name == "coalesce" {
                    let branches = if name == "if" {
                        args.get(1..).unwrap_or_default()
                    } else {
                        args
                    };
                    self.join_kinds(branches.iter().map(|a| &a.value))
                } else {
                    Kind::Value
                }
            }
            Expr::List { items } => self.join_kinds(items.iter()),
            _ => Kind::Value,
        }
    }
    fn join_kinds<'b>(&self, es: impl Iterator<Item = &'b Expr>) -> Kind {
        let mut result = Kind::Value;
        for e in es {
            match self.classify(e) {
                Kind::Node => return Kind::Node,
                Kind::Action => result = Kind::Action,
                Kind::Value => {}
            }
        }
        result
    }
    fn infer_kinds(&mut self) {
        let mut users: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (n, s) in &self.table {
            let mut deps = BTreeSet::new();
            names(&s.value, &mut deps);
            for d in deps {
                users.entry(d).or_default().insert(n.clone());
            }
        }
        let mut queue: VecDeque<_> = self.table.keys().cloned().collect();
        let mut queued: BTreeSet<_> = queue.iter().cloned().collect();
        while let Some(n) = queue.pop_front() {
            queued.remove(&n);
            let Some(s) = self.table.get(&n) else {
                continue;
            };
            let k = if s.kind == StatementKind::Query {
                Kind::Value
            } else {
                self.classify(&s.value)
            };
            if self.kinds.get(&n) != Some(&k) {
                self.kinds.insert(n.clone(), k);
                for u in users.get(&n).into_iter().flatten() {
                    if queued.insert(u.clone()) {
                        queue.push_back(u.clone());
                    }
                }
            }
        }
    }
    fn mark_reachable(&mut self) {
        let mut queue = vec!["root".to_string()];
        while let Some(n) = queue.pop() {
            if !self.reachable.insert(n.clone()) {
                continue;
            }
            if let Some(s) = self.table.get(&n) {
                let mut deps = BTreeSet::new();
                names(&s.value, &mut deps);
                queue.extend(deps);
            }
        }
    }
    fn run(mut self) -> Validation {
        self.infer_kinds();
        self.ir.templates = self
            .table
            .iter()
            .filter(|(n, _)| self.kinds.get(*n) == Some(&Kind::Node))
            .map(|(n, s)| (n.clone(), s.value.clone()))
            .collect();
        self.mark_reachable();
        self.states();
        self.infer_binding_defaults();
        self.queries();
        self.value_statements();
        if let Some(root) = self.table.get("root").cloned() {
            let ctx = Context::at("root", Some(NodePath::root()));
            self.active.insert("root".into());
            let mut root_nodes = if self.cache.get("root").is_some_and(|c| !c.valid) {
                Vec::new()
            } else {
                self.walk_expr(&root.value, &ctx, NodePath::root(), 1, None)
            };
            if root_nodes.len() > 1 {
                self.emit(Code::SlotKind, &ctx);
                for id in root_nodes.drain(1..) {
                    self.remove_subtree(id);
                }
            }
            self.active.remove("root");
            self.ir.root = root_nodes.first().copied();
            self.ir.root_plan = self.pending_plans.pop();
        } else {
            self.diagnostics
                .push(Diagnostic::new(Code::NoRoot, None, None));
        }
        // Unreachable definitions are still checked, so fragments and unused bad examples cannot hide errors.
        let orphans: Vec<_> = self
            .table
            .iter()
            .filter(|(n, _)| !self.reachable.contains(*n))
            .map(|(n, s)| (n.clone(), s.clone()))
            .collect();
        for (name, s) in orphans {
            let ctx = Context::at(&name, None);
            if self.kinds.get(&name) == Some(&Kind::Node) {
                let pending = std::mem::take(&mut self.pending_plans);
                let keep = std::mem::take(&mut self.ir);
                self.ir.states = keep.states.clone();
                self.ir.queries = keep.queries.clone();
                let allocated = self.allocated;
                self.checking_orphan = true;
                self.checking_template = true;
                self.walk_expr(&s.value, &ctx, NodePath::root(), 1, None);
                self.ir = keep;
                self.pending_plans = pending;
                self.allocated = allocated;
                self.checking_orphan = false;
                self.checking_template = false;
            }
            self.emit(Code::Orphan, &ctx);
        }
        self.diagnostics.sort_by(|a, b| {
            (&a.stmt, &a.path, a.code, a.recovery).cmp(&(&b.stmt, &b.path, b.code, b.recovery))
        });
        Validation {
            ir: self.ir,
            diagnostics: self.diagnostics,
        }
    }
    fn states(&mut self) {
        let mut bytes = 0usize;
        for (name, s) in self.state_sources.clone() {
            let ctx = Context::at(&name, None);
            if let Some(mut v) = Value::constant_recovered(&s.value) {
                self.cap_value(&mut v, &ctx);
                let size = serde_json::to_vec(&v).map_or(usize::MAX, |v| v.len());
                if size > self.limits.max_state_bytes.saturating_sub(bytes) {
                    self.emit(Code::StateSize, &ctx);
                    continue;
                }
                bytes = bytes.saturating_add(size);
                self.ir.states.insert(
                    name,
                    StateDef {
                        ty: T::infer(&v),
                        default: v,
                    },
                );
            } else {
                self.emit(Code::ConstDefault, &ctx);
            }
        }
    }
    fn queries(&mut self) {
        let queries: Vec<_> = self
            .table
            .iter()
            .filter(|(_, s)| s.kind == StatementKind::Query)
            .map(|(n, s)| (n.clone(), s.clone()))
            .collect();
        for (i, (name, s)) in queries.into_iter().enumerate() {
            let ctx = Context::at(&name, None);
            if i >= self.limits.max_queries {
                self.emit(Code::TooManyQueries, &ctx);
                self.table.remove(&name);
                self.kinds.remove(&name);
                continue;
            }
            let Expr::Call { name: call, args } = s.value else {
                self.emit(Code::ExprType, &ctx);
                continue;
            };
            let positional: Vec<_> = args.iter().filter(|a| a.name.is_none()).collect();
            let default = args.iter().find(|a| a.name.as_deref() == Some("default"));
            let id = positional.first().and_then(|a| {
                if let Expr::String { value } = &a.value {
                    Some(value.clone())
                } else {
                    None
                }
            });
            if call != "tool"
                || positional.len() != 2
                || default.is_none()
                || args.len() != 3
                || id.is_none()
            {
                self.emit(Code::ExprType, &ctx);
                continue;
            }
            if let (Some(tool), Some(default), Some(arg)) = (id, default, positional.get(1)) {
                let allowed = self.options.tools.contains(&tool);
                if !allowed {
                    self.emit(Code::UnknownTool, &ctx);
                }
                self.ir.queries.insert(
                    name,
                    QueryDef {
                        tool,
                        args: arg.value.clone(),
                        default: default.value.clone(),
                        allowed,
                    },
                );
            }
        }
    }
    fn value_statements(&mut self) {
        // Explicit post-order stack: a 20,000-name chain never uses the Rust call stack.
        let roots: Vec<_> = self
            .table
            .keys()
            .filter(|n| self.kinds.get(*n) != Some(&Kind::Node))
            .cloned()
            .collect();
        let mut visiting = BTreeSet::new();
        let mut completed = BTreeSet::new();
        for root in roots {
            let mut stack = vec![(root, false)];
            while let Some((name, finish)) = stack.pop() {
                if completed.contains(&name) {
                    continue;
                }
                let Some(s) = self.table.get(&name).cloned() else {
                    continue;
                };
                if self.kinds.get(&name) == Some(&Kind::Node) {
                    continue;
                }
                if !finish {
                    if !visiting.insert(name.clone()) {
                        let ctx = Context::at(&name, None);
                        self.emit(Code::Cycle, &ctx);
                        self.cache.insert(name.clone(), Checked::invalid());
                        completed.insert(name);
                        continue;
                    }
                    stack.push((name.clone(), true));
                    let mut deps = BTreeSet::new();
                    names(&s.value, &mut deps);
                    for d in deps.into_iter().rev() {
                        if self.table.contains_key(&d) && self.kinds.get(&d) != Some(&Kind::Node) {
                            stack.push((d, false));
                        }
                    }
                    continue;
                }
                visiting.remove(&name);
                let ctx = Context::at(&name, None);
                let kind = self.kinds.get(&name).copied().unwrap_or(Kind::Value);
                let checked = if s.kind == StatementKind::Query {
                    if let Some(q) = self.ir.queries.get(&name).cloned() {
                        self.check_value(&q.args, &ctx);
                        let d = self.check_value(&q.default, &ctx);
                        Checked::dynamic(d.ty)
                    } else {
                        Checked::invalid()
                    }
                } else if kind == Kind::Action {
                    let (valid, source) = self.normalize_action(&s.value, &ctx);
                    self.action_sources.insert(name.clone(), source);
                    if valid {
                        Checked::dynamic(T::Action)
                    } else {
                        Checked::invalid()
                    }
                } else {
                    self.check_value(&s.value, &ctx)
                };
                self.ir.values.insert(
                    name.clone(),
                    ValueEntry {
                        kind,
                        source_kind: s.kind,
                        expression: s.value,
                        value: checked.value.clone().filter(|_| checked.valid),
                    },
                );
                self.cache.insert(name.clone(), checked);
                completed.insert(name);
            }
        }
    }
    fn dynamic(&mut self, e: &Expr) -> Prop {
        Prop::Dynamic(self.ir.intern(e))
    }
    fn remove_subtree(&mut self, id: NodeId) {
        if let Some(n) = self.ir.nodes.remove(&id) {
            self.allocated = self.allocated.saturating_sub(1);
            for s in n.slots {
                for id in s.children {
                    self.remove_subtree(id)
                }
            }
        }
    }
    fn valid_url(&self, v: &Value) -> bool {
        let Value::Str(s) = v else { return false };
        if matches!(
            s.prov,
            crate::value::Provenance::User | crate::value::Provenance::Mixed
        ) {
            return false;
        }
        let Ok(url) = url::Url::parse(&s.text) else {
            return false;
        };
        self.options.url_schemes.contains(url.scheme())
            && url.username().is_empty()
            && url.password().is_none()
            && url.host_str().is_some_and(|h| {
                s.prov == crate::value::Provenance::Host || self.options.url_hosts.contains(h)
            })
    }
}
#[cfg(test)]
mod tests;
