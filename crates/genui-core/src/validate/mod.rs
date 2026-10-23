//! Static resolution and validation (§§2.10–2.15,4,5,7.1). No requests or evaluation.
mod budget;
mod expression;
mod incremental;
mod queries;
mod resource;
pub(crate) use incremental::{Invalidation, validate_props};
mod binding;
mod slots;
mod source;
mod tasks;
pub(crate) use tasks::{Changes as TaskChanges, Projection};
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
pub(crate) use slots::PrefixCounts;
pub(crate) use slots::{Graft, resolve_waiting};
use slots::{SlotCheck, SlotItem, SlotKey};
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
    pub diagnostics: crate::Diagnostics,
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
            diagnostics: diagnostics.into(),
        };
    }
    Validator::new(program, diagnostics, library, options).run()
}
/// Reuse unaffected concrete static instances while revisiting canonical budgets.
/// Expression/action arenas remain batch-owned; deferred instances take the regular path.
pub(crate) fn validate_incremental(
    library: &Library,
    options: &Options,
    previous: Option<&Validation>,
    changes: tasks::Changes<'_>,
    sealed_states: Option<&crate::SharedMap<String, StateDef>>,
    previous_cache: Option<&ValidationCache>,
    instance_cache: Option<&ValidationCache>,
) -> (Validation, usize, crate::MaintenanceWork, ValidationCache) {
    let mut validator = Validator::new(&Program::default(), Vec::new(), library, options);
    validator.sealed_states = sealed_states;
    if let Some(previous) = previous {
        validator.ir = previous.ir.clone();
    }
    if let Some(cache) = previous_cache {
        validator.table = cache.table.clone();
        validator.state_sources = cache.state_sources.clone();
        validator.query_sources = cache.query_sources.clone();
        validator.query_diagnostics = cache.query_diagnostics.clone();
        validator.orphan_diagnostics = cache.orphan_diagnostics.clone();
        validator.query_budget = cache.query_budget.clone();
        validator.kinds = cache.kinds.clone();
        validator.cache = cache.checked.clone();
        validator.action_sources = cache.action_sources.clone();
        validator.reachable = cache.reachable.clone();
        validator.ids = cache.ids.clone();
        validator.slots = cache.slots.clone();
        validator.slot_readers = cache.slot_readers.clone();
        validator.instances = cache.instances.clone();
        validator.readers = cache.readers.clone();
        validator.value_diagnostics = cache.value_diagnostics.clone();
        validator.state_bytes = cache.state_bytes;
        validator.state_budget = cache.state_budget.clone();
        validator.state_candidates = cache.state_candidates.clone();
    }
    if let Some(source) = previous_cache.and_then(|cache| cache.tasks.as_ref()) {
        source.source_ir(&mut validator.ir);
    }
    let mut machine = instance_cache
        .or(previous_cache)
        .and_then(|cache| cache.tasks.clone())
        .unwrap_or_default();
    machine.arenas(&mut validator.ir);
    machine.metadata_root(
        previous_cache.and_then(|cache| cache.tasks.as_ref()),
        &mut validator.work,
    );
    machine.begin(&changes, std::iter::empty());
    if let Some(cache) = instance_cache.or(previous_cache) {
        validator.instances = cache.instances.clone();
        validator.readers = cache.readers.clone();
        validator.ids = cache.ids.clone();
    }
    validator.ir.nodes = machine.graph.nodes.clone();
    validator.tasks = Some(machine);
    let (validation, work, cache) = validator.run_live(changes);
    let reused = cache
        .tasks
        .as_ref()
        .map_or(0, |machine| machine.reused_nodes);
    (validation, reused, work, cache)
}
#[derive(Clone, Default)]
pub(crate) struct ValidationCache {
    table: crate::SharedMap<String, Statement>,
    state_sources: crate::SharedMap<String, Statement>,
    query_sources: crate::SharedMap<String, Statement>,
    query_diagnostics: crate::SharedMap<String, Vec<Diagnostic>>,
    orphan_diagnostics: crate::SharedMap<String, Vec<Diagnostic>>,
    query_budget: budget::BudgetIndex,

    kinds: crate::SharedMap<String, Kind>,
    checked: crate::SharedMap<String, Checked>,
    action_sources: crate::SharedMap<String, Expr>,
    reachable: im::OrdSet<String>,
    instances: crate::SharedMap<NodeId, InstanceCheck>,
    readers: crate::SharedMap<String, im::OrdSet<NodeId>>,
    value_diagnostics: crate::SharedMap<String, Vec<Diagnostic>>,
    diagnostics: crate::Diagnostics,
    ids: IdAllocator,
    slots: crate::SharedMap<SlotKey, SlotCheck>,
    slot_readers: crate::SharedMap<String, im::OrdSet<(SlotKey, usize)>>,
    ambiguous_states: bool,
    allocation_limited: bool,
    tasks: Option<tasks::Machine>,
    state_bytes: usize,
    state_budget: budget::BudgetIndex,
    state_candidates: crate::SharedMap<String, StateDef>,
    state_limited: bool,
}

/// Validate a disconnected new definition without rebuilding the displayed graph.
/// Dependent/global allocation changes use the general dirty-maintenance path.
pub(crate) fn validate_unrelated(
    cached: &ValidationCache,
    base: &Validation,
    changes: &[Statement],
    dirty: &BTreeSet<String>,
    library: &Library,
    options: &Options,
) -> Option<(Validation, crate::MaintenanceWork, ValidationCache)> {
    if cached.tasks.is_some() {
        return None;
    }
    if changes.iter().any(|s| {
        s.kind != StatementKind::Bind || s.name == "root" || cached.table.contains_key(&s.name)
    }) || dirty
        .iter()
        .any(|name| name == "root" || name.starts_with('$') || cached.table.contains_key(name))
    {
        return None;
    }
    let changed: BTreeSet<_> = changes.iter().map(|s| s.name.as_str()).collect();
    for s in changes {
        let mut deps = BTreeSet::new();
        names(&s.value, &mut deps);
        if deps.iter().any(|name| changed.contains(name.as_str())) {
            return None;
        }
    }
    let mut v = Validator::new(&Program::default(), Vec::new(), library, options);
    v.table = cached.table.clone();
    v.state_sources = cached.state_sources.clone();
    v.kinds = cached.kinds.clone();
    v.cache = cached.checked.clone();
    v.action_sources = cached.action_sources.clone();
    v.reachable = cached.reachable.clone();
    v.orphan_diagnostics = cached.orphan_diagnostics.clone();
    v.slots = cached.slots.clone();
    v.slot_readers = cached.slot_readers.clone();
    v.instances = cached.instances.clone();
    v.readers = cached.readers.clone();
    v.value_diagnostics = cached.value_diagnostics.clone();
    v.ir = base.ir.clone();
    for s in changes {
        v.work.statements_checked += 1;
        let kind = v.classify(&s.value);
        v.table.insert(s.name.clone(), s.clone());
        v.kinds.insert(s.name.clone(), kind);
        let ctx = Context::at(&s.name, None);
        let own_start = v.diagnostics.len();
        if kind == Kind::Node {
            v.ir.templates.insert(s.name.clone(), s.value.clone());
            let keep = std::mem::take(&mut v.ir);
            v.ir.states = keep.states.clone();
            v.ir.queries = keep.queries.clone();
            v.checking_orphan = true;
            v.checking_template = true;
            v.walk_expr(&s.value, &ctx, NodePath::root(), 1, None);
            v.ir = keep;
            v.pending_plans.clear();
            v.checking_orphan = false;
            v.checking_template = false;
        } else {
            let checked = if kind == Kind::Action {
                let (valid, source) = v.normalize_action(&s.value, &ctx);
                v.action_sources.insert(s.name.clone(), source);
                if valid {
                    Checked::dynamic(T::Action)
                } else {
                    Checked::invalid()
                }
            } else {
                v.check_value(&s.value, &ctx)
            };
            v.ir.values.insert(
                s.name.clone(),
                ValueEntry {
                    kind,
                    source_kind: s.kind,
                    expression: s.value.clone(),
                    value: checked.value.clone().filter(|_| checked.valid),
                },
            );
            v.cache.insert(s.name.clone(), checked);
        }
        let orphan_start = if kind == Kind::Node {
            own_start
        } else {
            v.value_diagnostics
                .insert(s.name.clone(), v.diagnostics[own_start..].to_vec());
            v.diagnostics.len()
        };
        v.emit(Code::Orphan, &ctx);
        v.orphan_diagnostics
            .insert(s.name.clone(), v.diagnostics[orphan_start..].to_vec());
    }
    let mut diagnostics = base.diagnostics.clone();
    v.work.diagnostics_examined += v.diagnostics.len() as u64;
    diagnostics.extend_raw(v.diagnostics);
    let cache = ValidationCache {
        table: v.table,
        state_sources: v.state_sources,
        query_sources: cached.query_sources.clone(),
        query_diagnostics: cached.query_diagnostics.clone(),
        orphan_diagnostics: v.orphan_diagnostics,
        query_budget: cached.query_budget.clone(),
        kinds: v.kinds,
        checked: v.cache,
        action_sources: v.action_sources,
        reachable: v.reachable,
        instances: v.instances,
        readers: v.readers,
        value_diagnostics: v.value_diagnostics,
        diagnostics: diagnostics.clone(),
        ids: v.ids,
        slots: v.slots,
        slot_readers: v.slot_readers,
        ambiguous_states: cached.ambiguous_states,
        allocation_limited: cached.allocation_limited,
        tasks: None,
        state_bytes: cached.state_bytes,
        state_budget: cached.state_budget.clone(),
        state_candidates: cached.state_candidates.clone(),
        state_limited: cached.state_limited,
    };
    Some((
        Validation {
            ir: v.ir,
            diagnostics,
        },
        v.work,
        cache,
    ))
}

impl ValidationCache {
    pub(crate) fn node_order(&self, id: &NodeId) -> Option<&Vec<(String, usize, usize)>> {
        self.instances.get(id).map(|instance| &instance.ctx.order)
    }
}
struct Reuse<'a> {
    previous: &'a Validation,
    dirty: &'a BTreeSet<String>,
    count: &'a std::cell::Cell<usize>,
}
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Eq, PartialEq)]
struct Context {
    stmt: String,
    path: Option<NodePath>,
    locals: BTreeSet<String>,
    accepts: Vec<String>,
    form_scope: Option<NodeId>,
    list_cap: Option<usize>,
    budget_stmt: Option<String>,
    order: Vec<(String, usize, usize)>,
    allocation_order: Vec<u64>,
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
            order: Vec::new(),
            allocation_order: Vec::new(),
        }
    }
}
struct TemplateCheck {
    expression: Expr,
    ctx: Context,
    path: NodePath,
    place: Option<String>,
}
#[derive(Clone)]
struct InstanceCheck {
    source: std::sync::Arc<Expr>,
    ctx: Context,
    path: NodePath,
    depth: usize,
    bound: crate::SharedMap<usize, Expr>,
    reads: BTreeSet<String>,
    diagnostics: Vec<Diagnostic>,
}
struct Validator<'a> {
    tasks: Option<tasks::Machine>,
    source_reverse: Option<&'a BTreeMap<String, BTreeSet<String>>>,
    instances: crate::SharedMap<NodeId, InstanceCheck>,
    readers: crate::SharedMap<String, im::OrdSet<NodeId>>,
    value_diagnostics: crate::SharedMap<String, Vec<Diagnostic>>,
    slots: crate::SharedMap<SlotKey, SlotCheck>,
    slot_readers: crate::SharedMap<String, im::OrdSet<(SlotKey, usize)>>,
    retained_instance: Option<InstanceCheck>,
    retained_slots: Option<Vec<IrSlot>>,
    work: crate::MaintenanceWork,
    reuse: Option<Reuse<'a>>,
    sealed_states: Option<&'a crate::SharedMap<String, StateDef>>,
    lib: &'a Library,
    options: &'a Options,
    limits: Limits,
    table: crate::SharedMap<String, Statement>,
    state_sources: crate::SharedMap<String, Statement>,
    query_sources: crate::SharedMap<String, Statement>,
    query_diagnostics: crate::SharedMap<String, Vec<Diagnostic>>,
    orphan_diagnostics: crate::SharedMap<String, Vec<Diagnostic>>,
    query_budget: budget::BudgetIndex,

    kinds: crate::SharedMap<String, Kind>,
    ir: Ir,
    diagnostics: Vec<Diagnostic>,
    cache: crate::SharedMap<String, Checked>,
    action_sources: crate::SharedMap<String, Expr>,
    reachable: im::OrdSet<String>,
    ids: IdAllocator,
    active: BTreeSet<String>,
    allocated: usize,
    direct_limit: bool,
    state_bytes: usize,
    state_budget: budget::BudgetIndex,
    state_candidates: crate::SharedMap<String, StateDef>,
    limited: BTreeSet<(String, Code)>,
    checking_template: bool,
    checking_orphan: bool,
    pending_plans: crate::SharedVector<SlotPlan>,
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
        Self::from_statements(p.statements.iter(), diagnostics, lib, options)
    }
    fn from_statements<'s>(
        statements: impl IntoIterator<Item = &'s Statement>,
        diagnostics: Vec<Diagnostic>,
        lib: &'a Library,
        options: &'a Options,
    ) -> Self {
        let mut table = crate::SharedMap::new();
        let mut state_sources = crate::SharedMap::new();
        let mut statements_checked = 0;
        for s in statements {
            statements_checked += 1;
            if s.kind == StatementKind::State {
                if !state_sources.contains_key(&s.name) {
                    state_sources.insert(s.name.clone(), s.clone());
                }
            } else {
                if !table.contains_key(&s.name) {
                    table.insert(s.name.clone(), s.clone());
                }
            }
        }
        let kinds = table.keys().map(|n| (n.clone(), Kind::Value)).collect();
        Self {
            work: crate::MaintenanceWork {
                statements_checked,
                ..Default::default()
            },
            tasks: None,
            source_reverse: None,
            instances: Default::default(),
            readers: Default::default(),
            value_diagnostics: Default::default(),
            slots: Default::default(),
            slot_readers: Default::default(),
            retained_instance: None,
            retained_slots: None,
            reuse: None,
            sealed_states: None,
            lib,
            options,
            limits: options.limits.clone().bounded(),
            table,
            state_sources,
            query_sources: Default::default(),
            query_diagnostics: Default::default(),
            orphan_diagnostics: Default::default(),
            query_budget: Default::default(),
            kinds,
            ir: Ir::default(),
            diagnostics,
            cache: crate::SharedMap::new(),
            action_sources: crate::SharedMap::new(),
            reachable: im::OrdSet::new(),
            ids: IdAllocator::default(),
            active: BTreeSet::new(),
            allocated: 0,
            direct_limit: false,
            state_bytes: 0,
            state_budget: Default::default(),
            state_candidates: Default::default(),
            limited: BTreeSet::new(),
            checking_template: false,
            checking_orphan: false,
            pending_plans: Default::default(),
            template_queue: VecDeque::new(),
            template_seen: BTreeSet::new(),
        }
    }
    fn begin_owner(&mut self, key: String, order: Vec<u64>) -> crate::arena::Scope {
        if !self.checking_orphan
            && !self.checking_template
            && let Some(frame) = self
                .tasks
                .as_mut()
                .and_then(|machine| machine.frames.last_mut())
        {
            frame.owners.insert(key.clone());
        }
        self.ir.begin_owner(key, order)
    }
    fn emit(&mut self, code: Code, ctx: &Context) -> usize {
        let mut d = Diagnostic::new(code, Some(ctx.stmt.clone()), None);
        if !self.checking_orphan {
            d.path = ctx.path.as_ref().map(|p| p.0.clone());
        }
        self.diagnostics.push(d);
        let index = self.diagnostics.len() - 1;
        if let Some(frame) = self
            .tasks
            .as_mut()
            .and_then(|machine| machine.frames.last_mut())
        {
            frame.indices.insert(index);
        }
        index
    }
    fn recovery(&mut self, code: Code, recovery: Recovery, ctx: &Context) {
        let i = self.emit(code, ctx);
        if let Some(d) = self.diagnostics.get_mut(i) {
            d.recovery = recovery
        }
    }
    fn limit(&mut self, code: Code, ctx: &Context) {
        if !self.checking_orphan
            && !self.checking_template
            && matches!(code, Code::Nodes | Code::IrDepth)
            && self
                .tasks
                .as_ref()
                .is_some_and(|machine| !machine.frames.is_empty())
        {
            let index = self.emit(code, ctx);
            if let Some(frame) = self
                .tasks
                .as_mut()
                .and_then(|machine| machine.frames.last_mut())
            {
                frame.limits.insert(index, ctx.allocation_order.clone());
            }
            return;
        }
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
            self.work.statements_checked += 1;
            let mut deps = BTreeSet::new();
            names(&s.value, &mut deps);
            for d in deps {
                users.entry(d).or_default().insert(n.clone());
            }
        }
        let mut queue: VecDeque<_> = self.table.keys().cloned().collect();
        let mut queued: BTreeSet<_> = queue.iter().cloned().collect();
        while let Some(n) = queue.pop_front() {
            self.work.statements_checked += 1;
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
            self.work.statements_checked += 1;
            if self.reachable.contains(&n) {
                continue;
            }
            self.reachable.insert(n.clone());
            if let Some(s) = self.table.get(&n) {
                let mut deps = BTreeSet::new();
                names(&s.value, &mut deps);
                queue.extend(deps);
            }
        }
    }
    fn run(self) -> Validation {
        self.run_measured().0
    }
    fn run_measured(mut self) -> (Validation, crate::MaintenanceWork, ValidationCache) {
        self.infer_kinds();
        self.work.statements_checked += self.table.len() as u64;
        self.ir.templates = self
            .table
            .iter()
            .filter(|(n, _)| self.kinds.get(*n) == Some(&Kind::Node))
            .map(|(n, s)| (n.clone(), s.value.clone()))
            .collect();
        self.mark_reachable();
        self.states();
        if let Some(states) = self.sealed_states {
            self.ir.states = states.clone();
        } else {
            self.infer_binding_defaults();
        }
        self.queries();
        self.value_statements();
        if let Some(machine) = &mut self.tasks {
            let empty = BTreeMap::new();
            machine.metadata(
                &self.ir,
                &self.cache,
                &self.action_sources,
                self.source_reverse.unwrap_or(&empty),
            );
        }
        if let Some(root) = self.table.get("root").cloned() {
            let ctx = Context::at("root", Some(NodePath::root()));
            self.active.insert("root".into());
            let mut root_nodes = if self.cache.get("root").is_some_and(|c| !c.valid) {
                Default::default()
            } else {
                self.walk_expr(&root.value, &ctx, NodePath::root(), 1, None)
            };
            if root_nodes.len() > 1 {
                self.emit(Code::SlotKind, &ctx);
                for id in root_nodes.split_off(1) {
                    self.remove_subtree(id);
                }
            }
            self.active.remove("root");
            self.ir.root = root_nodes.first().copied();
            self.ir.root_plan = self.pending_plans.pop_back();
        } else {
            self.diagnostics
                .push(Diagnostic::new(Code::NoRoot, None, None));
        }
        // Unreachable definitions are still checked, so fragments and unused bad examples cannot hide errors.
        self.work.statements_checked += self.table.len() as u64;
        let orphans: Vec<_> = self
            .table
            .iter()
            .filter(|(n, _)| !self.reachable.contains(*n))
            .map(|(n, s)| (n.clone(), s.clone()))
            .collect();
        for (name, s) in orphans {
            self.work.statements_checked += 1;
            let ctx = Context::at(&name, None);
            let own_start = self.diagnostics.len();
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
            self.orphan_diagnostics
                .insert(name, self.diagnostics[own_start..].to_vec());
        }
        let retained_diagnostics = if let Some(machine) = &mut self.tasks {
            let (nodes, work) = machine.finish();
            self.ir.nodes = nodes;
            self.work.instances_examined += work;
            let mut index = 0;
            self.diagnostics.retain(|_| {
                let keep = !machine.owned_indices.contains(&index);
                index += 1;
                keep
            });
            Some(machine.diagnostics())
        } else {
            None
        };
        self.diagnostics.sort_by(|a, b| {
            self.work.diagnostics_examined += 1;
            (&a.stmt, &a.path, a.code, a.recovery).cmp(&(&b.stmt, &b.path, b.code, b.recovery))
        });
        self.work.statements_checked += self.ir.states.len() as u64;
        let ambiguous_states = self.ir.states.values().any(|state| {
            state.default == Value::Null
                || matches!(&state.default,Value::List(items) if items.is_empty())
        });
        self.work.diagnostics_examined += self.diagnostics.len() as u64;
        let state_limited = self.diagnostics.iter().any(|d| d.code == Code::StateSize);
        self.work.diagnostics_examined += self.diagnostics.len() as u64;
        let allocation_limited = self
            .diagnostics
            .iter()
            .any(|d| matches!(d.code, Code::Nodes | Code::IrDepth));
        let mut diagnostics: crate::Diagnostics = retained_diagnostics.unwrap_or_default();
        diagnostics.extend_raw(self.diagnostics);
        let cache = ValidationCache {
            table: self.table,
            state_sources: self.state_sources,
            query_sources: self.query_sources,
            query_diagnostics: self.query_diagnostics,
            orphan_diagnostics: self.orphan_diagnostics,
            query_budget: self.query_budget,
            kinds: self.kinds,
            checked: self.cache,
            action_sources: self.action_sources,
            reachable: self.reachable,
            instances: self.instances,
            readers: self.readers,
            value_diagnostics: self.value_diagnostics,
            diagnostics: diagnostics.clone(),
            ids: self.ids,
            slots: self.slots,
            slot_readers: self.slot_readers,
            ambiguous_states,
            allocation_limited,
            tasks: self.tasks,
            state_bytes: self.state_bytes,
            state_budget: self.state_budget,
            state_candidates: self.state_candidates,
            state_limited,
        };
        (
            Validation {
                ir: self.ir,
                diagnostics,
            },
            self.work,
            cache,
        )
    }
    fn states(&mut self) {
        self.work.statements_checked += self.state_sources.len() as u64;
        for (name, s) in self.state_sources.clone() {
            let ctx = Context::at(&name, None);
            if let Some(mut value) = Value::constant_recovered(&s.value) {
                self.cap_value(&mut value, &ctx);
                let size = serde_json::to_vec(&value).map_or(usize::MAX, |bytes| bytes.len());
                self.state_budget.insert(name.clone(), size);
                self.state_candidates.insert(
                    name,
                    StateDef {
                        ty: T::infer(&value),
                        default: value,
                    },
                );
            } else {
                self.emit(Code::ConstDefault, &ctx);
            }
        }
        let (bytes, _, changes) = self.state_budget.reconcile(
            self.limits.max_state_bytes,
            &mut self.work.statements_checked,
        );
        for (name, accepted) in changes {
            if accepted {
                if let Some(value) = self.state_candidates.get(&name) {
                    self.ir.states.insert(name, value.clone());
                }
            } else {
                self.emit(Code::StateSize, &Context::at(&name, None));
            }
        }
        self.state_bytes = bytes;
    }
    fn queries(&mut self) {
        self.work.statements_checked += self.table.len() as u64;
        for (name, source) in &self.table {
            if source.kind == StatementKind::Query {
                self.query_sources.insert(name.clone(), source.clone());
                self.query_budget.insert(name.clone(), 1);
            }
        }
        let (_, _, changes) = self
            .query_budget
            .reconcile(self.limits.max_queries, &mut self.work.statements_checked);
        for (name, accepted) in changes {
            let start = self.diagnostics.len();
            if accepted {
                if let Some(source) = self.query_sources.get(&name).cloned() {
                    self.query_header(&source);
                }
            } else {
                self.emit(Code::TooManyQueries, &Context::at(&name, None));
                self.table.remove(&name);
                self.kinds.remove(&name);
            }
            self.query_diagnostics
                .insert(name, self.diagnostics[start..].to_vec());
        }
    }
    fn value_statements(&mut self) {
        self.work.statements_checked += self.table.len() as u64;
        let sources = self
            .table
            .keys()
            .filter(|name| self.kinds.get(*name) != Some(&Kind::Node))
            .cloned()
            .collect();
        self.check_sources(&sources);
    }
    fn dynamic(&mut self, e: &Expr) -> Prop {
        Prop::Dynamic(self.ir.intern(e))
    }
    fn remove_subtree(&mut self, id: NodeId) {
        if !self.checking_orphan
            && !self.checking_template
            && let Some(machine) = &mut self.tasks
        {
            if let Some(node) = machine.graph.nodes.get(&id) {
                self.allocated = self
                    .allocated
                    .saturating_sub(machine.graph.sizes.get(&id).copied().unwrap_or(1));
                machine.graph.hide(&node.path.0.clone());
            }
            return;
        }
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
