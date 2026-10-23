//! Streaming orchestration (§3): no evaluator, requests, deliveries or action execution.
use crate::{
    diag::{Code, Diagnostic},
    ir::{Ir, NodeStatus, Prop, PropValue},
    r#lib::Library,
    patch::{PatchBatch, Tree, strip_invisibles},
    syntax::{
        ast::{Expr, Statement, StatementKind},
        extract::{Extractor, Segment},
        streaming::{Seal, StreamingParser},
    },
    validate::{Options, Validation},
};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, OnceCell},
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputMode {
    Program,
    Message,
}
#[derive(Clone)]
pub struct SessionOptions {
    pub library: Arc<Library>,
    pub validation: Options,
    pub input: InputMode,
}
impl SessionOptions {
    pub fn new(library: Arc<Library>) -> Self {
        Self {
            library,
            validation: Options::default(),
            input: InputMode::Message,
        }
    }
    pub fn program(library: Arc<Library>) -> Self {
        Self {
            input: InputMode::Program,
            ..Self::new(library)
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    Waiting,
    Streaming,
    Done,
    Rejected,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProseDelta {
    pub segment: u32,
    pub append: String,
}
/// Transport for host effects. Requests are produced only after `finish()` and
/// are never dispatched by the core (§7.5).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct RequestId(pub u64);
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestOrigin {
    Program,
    UserGesture { node: crate::ir::NodeId },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReqKind {
    Query {
        stmt: String,
        tool: String,
        args: crate::value::Value,
        args_hash: u64,
        needs_approval: bool,
    },
    ResolveRef {
        #[serde(rename = "ref")]
        reference: String,
        stmt: String,
        path: Option<String>,
    },
    Say {
        text: String,
        ctx: crate::value::Value,
        label: String,
        node: String,
        stmt: String,
    },
    OpenUrl {
        url: String,
        node: String,
        stmt: String,
    },
    Emit {
        name: String,
        params: crate::value::Value,
        label: String,
        node: String,
        stmt: String,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HostRequest {
    pub id: RequestId,
    pub epoch: u32,
    pub origin: RequestOrigin,
    #[serde(flatten)]
    pub kind: ReqKind,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostError {
    Denied,
    Failed,
    TooLarge,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryStatus {
    #[default]
    Idle,
    Requested,
    Ready,
    Denied,
    Failed,
    Refreshing,
}
/// A model event is emitted only after the host accepts the corresponding step
/// (§7.8); `Update.requests` carries the host request that authorizes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelEvent {
    pub v: u8,
    pub source: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<crate::value::Value>,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctx: Option<crate::value::Value>,
    pub node: String,
    pub stmt: String,
}
/// Renderer-originated input (§7.8). Editing and blur work on sealed fields even
/// while streaming; activation requires a live session.
#[derive(Clone, Debug, PartialEq)]
pub enum UiEvent {
    Activate {
        node: crate::ir::NodeId,
        action: ActionHandle,
    },
    Edit {
        node: crate::ir::NodeId,
        value: crate::value::Value,
    },
    Blur {
        node: crate::ir::NodeId,
    },
}
/// An epoch-bound handle for an explicit action entry point (§7.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionHandle {
    pub plan: crate::ir::ActionPlanId,
    pub epoch: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Update {
    pub patches: Option<PatchBatch>,
    pub diagnostics: Vec<Diagnostic>,
    /// Runtime evaluation and action-write diagnostics for the current tree.
    pub runtime_diagnostics: Vec<Diagnostic>,
    pub requests: Vec<HostRequest>,
    pub model_events: Vec<ModelEvent>,
    pub prose: Vec<ProseDelta>,
    pub phase: SessionPhase,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Revision {
    pub origin_rev: u64,
    pub dep_revs: crate::SharedMap<String, u64>,
}
#[derive(Clone, Debug, Default)]
pub struct DirtyTracking {
    pub revisions: BTreeMap<String, u64>,
    pub reverse: BTreeMap<String, BTreeSet<String>>,
    pub instances: BTreeMap<String, Revision>,
    pub dirty: BTreeSet<String>,
    dependencies: BTreeMap<String, BTreeSet<String>>,
}
impl DirtyTracking {
    fn change(&mut self, s: &Statement) {
        let name = key(s);
        if let Some(old) = self.dependencies.remove(&name) {
            for d in old {
                if let Some(users) = self.reverse.get_mut(&d) {
                    users.remove(&name);
                }
            }
        }
        let mut deps = BTreeSet::new();
        dependencies(&s.value, &mut deps);
        for d in &deps {
            self.reverse
                .entry(d.clone())
                .or_default()
                .insert(name.clone());
        }
        self.dependencies.insert(name.clone(), deps);
        *self.revisions.entry(name.clone()).or_default() += 1;
        self.invalidate(name);
    }
    fn remove(&mut self, name: String) {
        if let Some(old) = self.dependencies.remove(&name) {
            for dependency in old {
                if let Some(users) = self.reverse.get_mut(&dependency) {
                    users.remove(&name);
                }
            }
        }
        *self.revisions.entry(name.clone()).or_default() += 1;
        self.invalidate(name);
    }
    fn invalidate(&mut self, name: String) {
        let mut pending = vec![name];
        while let Some(n) = pending.pop() {
            if self.dirty.insert(n.clone()) {
                pending.extend(self.reverse.get(&n).into_iter().flatten().cloned());
            }
        }
    }
    fn instance(&self, origin: &str) -> Revision {
        let mut deps = crate::SharedMap::new();
        let mut queue = vec![origin.to_string()];
        let mut seen = BTreeSet::new();
        while let Some(n) = queue.pop() {
            if !seen.insert(n.clone()) {
                continue;
            }
            for d in self.dependencies.get(&n).into_iter().flatten() {
                deps.insert(d.clone(), self.revisions.get(d).copied().unwrap_or(0));
                queue.push(d.clone());
            }
        }
        Revision {
            origin_rev: self.revisions.get(origin).copied().unwrap_or(0),
            dep_revs: deps,
        }
    }
}
#[derive(Clone, Debug, Default)]
struct QueryState {
    status: QueryStatus,
    args_hash: Option<u64>,
    request: Option<RequestId>,
    origin: Option<RequestOrigin>,
    queued: bool,
}
#[derive(Clone, Debug, Default)]
struct RefState {
    status: QueryStatus,
    request: Option<RequestId>,
    stmt: String,
    path: Option<String>,
}
#[derive(Clone, Debug)]
enum PendingKind {
    Query {
        epoch: u32,
        name: String,
        args_hash: u64,
    },
    Ref {
        epoch: u32,
        id: String,
    },
}
impl PendingKind {
    fn epoch(&self) -> u32 {
        match self {
            Self::Query { epoch, .. } | Self::Ref { epoch, .. } => *epoch,
        }
    }
}
pub struct Session {
    work: crate::MaintenanceWork,
    options: SessionOptions,
    parser: StreamingParser,
    extractor: Option<Extractor>,
    program_bytes: usize,
    program_raw: Vec<u8>,
    prose_sent: BTreeMap<u32, usize>,
    segments: Vec<Segment>,
    extraction_diagnostics: crate::Diagnostics,
    rejected: bool,
    sealed_count: usize,
    sealed_validation: Validation,
    sealed_cache: Option<crate::validate::ValidationCache>,
    last_candidate: Option<Statement>,
    current_cache: Option<crate::validate::ValidationCache>,
    current: Validation,
    before_static: Ir,
    tree: Tree,
    tracking: DirtyTracking,
    epoch: u32,
    next_seq: u64,
    started: bool,
    finished: bool,
    not_sync: Cell<()>,
    published: [crate::Diagnostics; 3],
    syntax_diagnostics: crate::Diagnostics,
    extracted_diagnostics: crate::Diagnostics,
    diagnostic_snapshot: OnceCell<Vec<Diagnostic>>,
    tree_dirty: bool,
    local_wire: Option<Vec<crate::ir::NodeId>>,
    projection: Option<crate::validate::Projection>,
    local_grafts: Option<Vec<crate::validate::Graft>>,
    reused_instances: usize,
    provisional_origins: BTreeSet<String>,
    runtime: crate::runtime::Runtime,
    resolved: crate::runtime::Resolved,
    runtime_diagnostics: Vec<Diagnostic>,
    action_diagnostics: Vec<Diagnostic>,
    host_diagnostics: Vec<Diagnostic>,
    queries: BTreeMap<String, QueryState>,
    refs_state: BTreeMap<String, RefState>,
    pending: BTreeMap<RequestId, PendingKind>,
    next_request: u64,
    clock_ms: u64,
    debounce: BTreeMap<String, u64>,
    refreshed: BTreeSet<String>,
    request_outbox: Vec<HostRequest>,
    event_outbox: Vec<ModelEvent>,
    fields: crate::runtime::FieldRegistry,
    touched: BTreeSet<String>,
    field_status: BTreeMap<String, crate::runtime::FieldStatus>,
}
impl Session {
    pub fn new(mut options: SessionOptions) -> Self {
        options.validation.limits = options.validation.limits.clone().bounded();
        let parser = StreamingParser::new(options.validation.limits.syntax.clone());
        let extractor = if options.input == InputMode::Message {
            Some(Extractor::default())
        } else {
            None
        };
        Self {
            work: Default::default(),
            options,
            parser,
            extractor,
            program_bytes: 0,
            program_raw: Vec::new(),
            prose_sent: BTreeMap::new(),
            segments: Vec::new(),
            extraction_diagnostics: Default::default(),
            rejected: false,
            sealed_count: 0,
            sealed_validation: Validation {
                ir: Ir::default(),
                diagnostics: Default::default(),
            },
            sealed_cache: None,
            last_candidate: None,
            current_cache: None,
            current: Validation {
                ir: Ir::default(),
                diagnostics: Default::default(),
            },
            before_static: Ir::default(),
            tree: Tree::default(),
            tracking: DirtyTracking::default(),
            epoch: 1,
            next_seq: 0,
            started: false,
            finished: false,
            not_sync: Cell::new(()),
            published: Default::default(),
            syntax_diagnostics: Default::default(),
            extracted_diagnostics: Default::default(),
            diagnostic_snapshot: OnceCell::new(),
            tree_dirty: true,
            local_wire: None,
            projection: None,
            local_grafts: None,
            reused_instances: 0,
            provisional_origins: BTreeSet::new(),
            runtime: crate::runtime::Runtime::new(),
            resolved: crate::runtime::Resolved::default(),
            runtime_diagnostics: Vec::new(),
            action_diagnostics: Vec::new(),
            host_diagnostics: Vec::new(),
            queries: BTreeMap::new(),
            refs_state: BTreeMap::new(),
            pending: BTreeMap::new(),
            next_request: 1,
            clock_ms: 0,
            debounce: BTreeMap::new(),
            refreshed: BTreeSet::new(),
            request_outbox: Vec::new(),
            event_outbox: Vec::new(),
            fields: crate::runtime::FieldRegistry::default(),
            touched: BTreeSet::new(),
            field_status: BTreeMap::new(),
        }
    }
    pub fn push(&mut self, chunk: &[u8]) -> Update {
        if self.finished {
            return self.empty_update();
        }
        self.tracking.dirty.clear();
        if let Some(extractor) = &mut self.extractor {
            extractor.push(chunk);
            self.extraction_diagnostics = extractor.diagnostic_snapshot();
            self.rejected = extractor.rejected();
            self.feed_program();
        } else {
            self.parser.push(chunk);
        }
        self.rebuild(false);
        self.update(true, false)
    }
    fn feed_program(&mut self) {
        if self.rejected {
            return;
        }
        let (bytes, ended) = if let Some(extractor) = &self.extractor {
            (extractor.program_raw(), extractor.program_ended())
        } else {
            (self.program_raw.as_slice(), true)
        };
        if let Some(bytes) = bytes.get(self.program_bytes..) {
            self.parser.push(bytes);
            self.program_bytes += bytes.len();
        }
        if ended {
            self.parser.finish();
        }
    }
    pub fn finish(&mut self) -> Update {
        if self.finished {
            return self.empty_update();
        }
        self.tracking.dirty.clear();
        if let Some(extractor) = self.extractor.take() {
            let (extracted, diagnostics) = extractor.finish_stream();
            self.program_raw = extracted.program_raw;
            self.segments = extracted.segments;
            self.extraction_diagnostics = diagnostics;
            self.rejected = extracted.rejected;
            self.feed_program();
        }
        self.parser.finish();
        self.finished = true;
        self.rebuild(true);
        self.pump_queries(true);
        self.pump_refs();
        if !self.queries.is_empty() {
            // Requested/queued status is a synthetic prop; keep the static
            // finish path local when the program has no queries (§7.5).
            self.mark_tree_dirty();
            self.evaluate();
        }
        self.update(false, false)
    }
    pub fn reset(&mut self) -> Update {
        let Some(epoch) = self.epoch.checked_add(1) else {
            self.rejected = true;
            return self.empty_update();
        };
        let options = self.options.clone();
        *self = Self::new(options);
        self.epoch = epoch;
        self.update(true, true)
    }
    /// A repair snapshot consumes the next sequence number, within the current epoch.
    pub fn snapshot(&mut self) -> Update {
        self.update(true, true)
    }
    pub fn maintenance_work(&self) -> crate::MaintenanceWork {
        let mut work = self.work;
        work.diagnostics_examined += self.parser.work().diagnostics_examined as u64;
        work
    }
    pub fn ir(&self) -> &Ir {
        &self.current.ir
    }
    /// Runtime-projected IR: the validated tree with data plans instantiated.
    /// Static validation, projection diffing and replay equality keep `ir()`.
    pub fn live_ir(&self) -> &Ir {
        if self.rejected {
            &self.current.ir
        } else {
            self.resolved.ir_or(&self.current.ir)
        }
    }
    pub fn tree(&self) -> &Tree {
        &self.tree
    }
    /// Evaluated Dynamic props for the current tree (§5.5).
    pub fn resolved(&self) -> &crate::runtime::Resolved {
        &self.resolved
    }
    /// Runtime-only diagnostics. Static validation diagnostics stay in `diagnostics()`.
    pub fn runtime_diagnostics(&self) -> &[Diagnostic] {
        &self.runtime_diagnostics
    }
    /// Look up the current handle for a node's action prop by its wire name.
    pub fn action_handle(&self, path: &crate::ir::NodePath, prop: &str) -> Option<ActionHandle> {
        let node = self
            .live_ir()
            .nodes
            .values()
            .find(|node| node.path == *path)?;
        let component = self
            .options
            .library
            .components()
            .get(node.kind.0 as usize)?;
        let index = component.args.iter().position(|arg| arg.name == prop)?;
        match node
            .props
            .get(&crate::ir::PropId(u16::try_from(index).ok()?))
        {
            Some(Prop::Static(PropValue::Action(plan))) => Some(ActionHandle {
                plan: plan.clone(),
                epoch: self.epoch,
            }),
            _ => None,
        }
    }
    /// Explicit action entry point. Events are gated while streaming and stale
    /// epoch/plan handles are inert. Host effects become requests/model events.
    pub fn activate(&mut self, node: crate::ir::NodeId, handle: &ActionHandle) -> Update {
        self.event(UiEvent::Activate {
            node,
            action: handle.clone(),
        })
    }
    /// Renderer input entry point (§7.8). Editing/blurring a sealed field writes
    /// user state while streaming; activation requires a live session.
    pub fn event(&mut self, event: UiEvent) -> Update {
        match event {
            UiEvent::Activate { node, action } => self.activate_event(node, action),
            UiEvent::Edit { node, value } => {
                self.edit(node, value);
                self.tree_dirty = true;
                self.evaluate();
                self.update(false, false)
            }
            UiEvent::Blur { node } => {
                if let Some(node) = self.live_ir().nodes.get(&node) {
                    self.touched.insert(node.path.0.clone());
                }
                self.mark_tree_dirty();
                self.evaluate();
                self.update(false, false)
            }
        }
    }
    fn edit(&mut self, node: crate::ir::NodeId, value: crate::value::Value) {
        let Some(node) = self.live_ir().nodes.get(&node) else {
            return;
        };
        if node.status != crate::ir::NodeStatus::Sealed {
            return;
        }
        let Some(state) = self.bound_state(node, &self.options.library) else {
            return;
        };
        if let Err(code) = self.runtime.state.write(
            &state,
            value,
            crate::runtime::StateOrigin::User,
            &self.options.validation.limits,
            &self.options.library,
        ) {
            self.push_host_diagnostic(code, Some(state), None);
        }
    }
    fn activate_event(&mut self, node: crate::ir::NodeId, handle: ActionHandle) -> Update {
        if !self.finished || handle.epoch != self.epoch {
            return self.empty_update();
        }
        let Some(plan) = self.live_ir().actions.get(&handle.plan).cloned() else {
            return self.empty_update();
        };
        if plan.origin != node {
            return self.empty_update();
        }
        if !self.live_ir().nodes.contains_key(&node) {
            return self.empty_update();
        }
        self.action_diagnostics.clear();
        let button_submit = self.is_submit_button(node);
        let gate = if let Some(scope) = plan.submit_scope {
            Some(Some(scope))
        } else if button_submit {
            Some(self.nearest_form(node))
        } else {
            None
        };
        if let Some(scope) = gate
            && !self.validate_scope(scope)
        {
            self.mark_tree_dirty();
            self.evaluate();
            return self.update(false, false);
        }
        let mut effects = self.execute_plan(&plan);
        if button_submit
            && plan.submit_scope.is_none()
            && let Some(form) = self.nearest_form(node)
            && let Some(form_plan) = self.form_submit_plan(form)
        {
            effects.extend(self.execute_plan(&form_plan));
        }
        for (effect, effect_stmt) in effects {
            self.apply_effect(effect, node, &effect_stmt);
        }
        self.mark_tree_dirty();
        self.evaluate();
        self.pump_queries(false);
        self.pump_refs();
        self.mark_tree_dirty();
        self.evaluate();
        self.update(false, false)
    }
    fn execute_plan(
        &mut self,
        plan: &crate::ir::ActionPlan,
    ) -> Vec<(crate::runtime::Effect, String)> {
        let live = self.live_ir().clone();
        let execution = self.runtime.execute_action(
            &live,
            &self.options.library,
            &self.options.validation.limits,
            &self.options.validation.refs,
            plan,
        );
        self.action_diagnostics.extend(execution.diagnostics);
        let stmt = self
            .current
            .ir
            .nodes
            .get(&plan.origin)
            .map_or_else(|| "root".to_string(), |node| node.origin.clone());
        execution
            .effects
            .into_iter()
            .map(|effect| (effect, stmt.clone()))
            .collect()
    }
    /// The current reset epoch (§6.3 G8). Request ids alone are not globally
    /// unique: they restart after `reset()`, so deliveries also carry an epoch.
    pub fn epoch(&self) -> u32 {
        self.epoch
    }
    /// Deliver one host result for the current epoch (§7.5).
    pub fn deliver(
        &mut self,
        id: RequestId,
        result: Result<crate::value::Value, HostError>,
    ) -> Update {
        self.deliver_from(self.epoch, id, result)
    }
    /// Deliver one host result, rejecting an id from another epoch. Unknown,
    /// stale and cross-epoch deliveries are dropped without changing anything.
    pub fn deliver_from(
        &mut self,
        epoch: u32,
        id: RequestId,
        result: Result<crate::value::Value, HostError>,
    ) -> Update {
        if !self.finished {
            return self.empty_update();
        }
        let Some(pending) = self.pending.get(&id).cloned() else {
            return self.empty_update();
        };
        if pending.epoch() != epoch || epoch != self.epoch {
            return self.empty_update();
        }
        self.pending.remove(&id);
        match pending {
            PendingKind::Query {
                name, args_hash, ..
            } => {
                let current = self.queries.get(&name).is_some_and(|state| {
                    state.request == Some(id) && state.args_hash == Some(args_hash)
                });
                if !current {
                    return self.empty_update();
                }
                let Some(state) = self.queries.get_mut(&name) else {
                    return self.empty_update();
                };
                state.request = None;
                match result {
                    Ok(value)
                        if crate::runtime::value_bytes(&value)
                            > self.options.validation.limits.max_resolved_data_bytes =>
                    {
                        state.status = QueryStatus::Failed;
                        self.push_host_diagnostic(Code::QueryFailed, Some(name.clone()), None);
                    }
                    Ok(value) => {
                        state.status = QueryStatus::Ready;
                        self.remove_query_errors(&name);
                        self.runtime.set_query_result(
                            &name,
                            crate::runtime::retag(value, crate::value::Provenance::Tool),
                        );
                    }
                    Err(HostError::Denied) => {
                        state.status = QueryStatus::Denied;
                        // Refreshing after a good result must show the declared
                        // default again, not the previous delivery (§7.5).
                        self.runtime.clear_query_result(&name);
                        self.push_host_diagnostic(Code::QueryDenied, Some(name.clone()), None);
                    }
                    Err(HostError::Failed | HostError::TooLarge) => {
                        state.status = QueryStatus::Failed;
                        self.push_host_diagnostic(Code::QueryFailed, Some(name.clone()), None);
                    }
                }
            }
            PendingKind::Ref { id: reference, .. } => {
                let current = self
                    .refs_state
                    .get(&reference)
                    .is_some_and(|state| state.request == Some(id));
                if !current {
                    return self.empty_update();
                }
                let Some(state) = self.refs_state.get_mut(&reference) else {
                    return self.empty_update();
                };
                state.request = None;
                let stmt = state.stmt.clone();
                let path = state.path.clone();
                match result {
                    Ok(value) if crate::runtime::contains_ref(&value) => {
                        state.status = QueryStatus::Failed;
                        self.runtime.fail_ref(&reference);
                        self.push_host_diagnostic(Code::RefInvalid, Some(stmt), path);
                    }
                    Ok(value)
                        if crate::runtime::value_bytes(&value)
                            > self.options.validation.limits.max_resolved_data_bytes =>
                    {
                        state.status = QueryStatus::Failed;
                        self.runtime.fail_ref(&reference);
                        self.push_host_diagnostic(Code::RefTooLarge, Some(stmt), path);
                    }
                    Ok(value) => {
                        if self.runtime.ref_value(&reference).is_none()
                            && self.runtime.resolved_ref_count()
                                >= self.options.validation.limits.max_ref_resolutions
                        {
                            state.status = QueryStatus::Failed;
                            self.runtime.fail_ref(&reference);
                            self.push_host_diagnostic(Code::RefTooLarge, Some(stmt), path);
                        } else {
                            state.status = QueryStatus::Ready;
                            self.runtime.set_ref_value(
                                &reference,
                                crate::runtime::retag(value, crate::value::Provenance::Host),
                            );
                        }
                    }
                    Err(HostError::TooLarge) => {
                        state.status = QueryStatus::Failed;
                        self.runtime.fail_ref(&reference);
                        self.push_host_diagnostic(Code::RefTooLarge, Some(stmt), path);
                    }
                    Err(HostError::Denied | HostError::Failed) => {
                        state.status = QueryStatus::Failed;
                        self.runtime.fail_ref(&reference);
                        self.push_host_diagnostic(Code::RefUnknown, Some(stmt), path);
                    }
                }
            }
        }
        self.mark_tree_dirty();
        self.evaluate();
        self.pump_queries(false);
        self.pump_refs();
        self.mark_tree_dirty();
        self.evaluate();
        self.update(false, false)
    }
    /// Virtual clock for the 300 ms query debounce (§2.15,7.5).
    pub fn advance_clock(&mut self, now_ms: u64) -> Update {
        if !self.finished || self.rejected {
            self.clock_ms = self.clock_ms.max(now_ms);
            return self.empty_update();
        }
        // Schedule from the clock at which the dependency changed, not from the
        // later call that first observes it. A revert cancels the timer.
        let names: Vec<_> = self.current.ir.queries.keys().cloned().collect();
        for name in names {
            let Some(query) = self.current.ir.queries.get(&name) else {
                continue;
            };
            if !query.allowed {
                continue;
            }
            let (args, diagnostics) = self.runtime.evaluate_expression(
                &self.current.ir,
                &self.options.library,
                &self.options.validation.limits,
                &self.options.validation.refs,
                &query.args,
                &name,
            );
            self.extend_host_diagnostics(diagnostics);
            let hash = args_hash(&crate::runtime::sanitize_value(args));
            let state = self.queries.entry(name.clone()).or_default();
            match state.args_hash {
                Some(current) if current != hash => {
                    let due = self.clock_ms.saturating_add(300);
                    self.debounce.entry(name).or_insert(due);
                }
                Some(_) => {
                    self.debounce.remove(&name);
                }
                None => {}
            }
        }
        self.clock_ms = self.clock_ms.max(now_ms);
        let due: Vec<_> = self
            .debounce
            .iter()
            .filter(|(_, due)| **due <= self.clock_ms)
            .map(|(name, _)| name.clone())
            .collect();
        for name in due {
            self.debounce.remove(&name);
            self.refreshed.insert(name);
        }
        self.pump_queries(false);
        self.mark_tree_dirty();
        self.evaluate();
        self.update(false, false)
    }
    pub fn query_status(&self, name: &str) -> QueryStatus {
        self.queries
            .get(name)
            .map_or(QueryStatus::Idle, |s| s.status)
    }
    pub fn field_status(&self) -> &BTreeMap<String, crate::runtime::FieldStatus> {
        &self.field_status
    }
    fn validate_scope(&mut self, scope: Option<crate::ir::NodeId>) -> bool {
        let statuses = self
            .fields
            .validate(scope, &self.runtime.state, &self.touched);
        let failed = statuses.values().any(|status| status.error.is_some());
        if failed {
            for field in self.fields.scope(scope) {
                self.touched.insert(field.path.0.clone());
            }
        }
        self.field_status = self.fields.all(&self.runtime.state, &self.touched);
        !failed
    }
    fn resolved_arg(
        &self,
        node: &crate::ir::Node,
        component: &crate::r#lib::ComponentDef,
        name: &str,
    ) -> Option<crate::ir::PropValue> {
        let index = component.args.iter().position(|arg| arg.name == name)?;
        let index = u16::try_from(index).ok()?;
        let prop = crate::ir::PropId(index);
        match node.props.get(&prop) {
            Some(crate::ir::Prop::Static(value)) => Some(value.clone()),
            Some(crate::ir::Prop::Dynamic(_)) => self.resolved.prop(node.id, prop).cloned(),
            None => None,
        }
    }
    fn is_submit_button(&self, node: crate::ir::NodeId) -> bool {
        let Some(node) = self.live_ir().nodes.get(&node) else {
            return false;
        };
        let Some(component) = self.options.library.components().get(node.kind.0 as usize) else {
            return false;
        };
        if component.name != "Button" {
            return false;
        }
        matches!(
            self.resolved_arg(node, component, "submit"),
            Some(crate::ir::PropValue::Value(crate::value::Value::Bool(true)))
        )
    }
    fn form_submit_plan(&self, form: crate::ir::NodeId) -> Option<crate::ir::ActionPlan> {
        let node = self.live_ir().nodes.get(&form)?;
        let component = self
            .options
            .library
            .components()
            .get(node.kind.0 as usize)?;
        match self.resolved_arg(node, component, "on_submit") {
            Some(crate::ir::PropValue::Action(id)) => self.live_ir().actions.get(&id).cloned(),
            _ => None,
        }
    }
    fn nearest_form(&self, mut node: crate::ir::NodeId) -> Option<crate::ir::NodeId> {
        let parents = parents(self.live_ir());
        while let Some(parent) = parents.get(&node).copied() {
            let is_form = self
                .current
                .ir
                .nodes
                .get(&parent)
                .and_then(|n| self.options.library.components().get(n.kind.0 as usize))
                .is_some_and(|c| c.name == "Form");
            if is_form {
                return Some(parent);
            }
            node = parent;
        }
        None
    }
    fn bound_state(
        &self,
        node: &crate::ir::Node,
        library: &crate::r#lib::Library,
    ) -> Option<String> {
        let component = library.components().get(node.kind.0 as usize)?;
        let index = component.args.iter().position(|arg| arg.name == "bind")?;
        let index = u16::try_from(index).ok()?;
        match node.props.get(&crate::ir::PropId(index)) {
            Some(crate::ir::Prop::Static(crate::ir::PropValue::Binding(name))) => {
                Some(name.clone())
            }
            _ => None,
        }
    }
    fn apply_effect(
        &mut self,
        effect: crate::runtime::Effect,
        node: crate::ir::NodeId,
        stmt: &str,
    ) {
        use crate::runtime::Effect;
        match effect {
            Effect::Refresh { query } => {
                if self.current.ir.queries.contains_key(&query) {
                    self.refreshed.insert(query.clone());
                    let state = self.queries.entry(query).or_default();
                    state.status = QueryStatus::Idle;
                    state.origin = Some(RequestOrigin::UserGesture { node });
                }
            }
            Effect::Say { text, ctx } => {
                let crate::value::Value::Str(text) = text else {
                    self.push_host_diagnostic(Code::ExprType, Some(stmt.into()), None);
                    return;
                };
                let ctx = match ctx {
                    crate::value::Value::Null => None,
                    crate::value::Value::Map(_) => Some(ctx),
                    _ => {
                        self.push_host_diagnostic(Code::ExprType, Some(stmt.into()), None);
                        return;
                    }
                };
                let label = self.label(node);
                let text = crate::patch::strip_invisibles(&text.text).to_string();
                let ctx = ctx.map(crate::runtime::sanitize_value);
                let reference = self
                    .current
                    .ir
                    .nodes
                    .get(&node)
                    .map_or_else(String::new, |n| n.path.0.clone());
                let id = RequestId(self.next_request);
                self.next_request = self.next_request.saturating_add(1);
                self.request_outbox.push(HostRequest {
                    id,
                    epoch: self.epoch,
                    origin: RequestOrigin::UserGesture { node },
                    kind: ReqKind::Say {
                        text: text.clone(),
                        ctx: ctx.clone().unwrap_or(crate::value::Value::Null),
                        label: label.clone(),
                        node: reference.clone(),
                        stmt: stmt.into(),
                    },
                });
                let event =
                    self.model_event("say", Some(text), None, None, &label, ctx, &reference, stmt);
                self.event_outbox.push(event);
            }
            Effect::Open { url } => {
                let crate::value::Value::Str(url) = url else {
                    self.push_host_diagnostic(Code::ExprType, Some(stmt.into()), None);
                    return;
                };
                let url = crate::value::PStr {
                    text: crate::patch::strip_invisibles(&url.text).into(),
                    prov: url.prov,
                };
                if !self.url_allowed(&url) {
                    self.push_host_diagnostic(Code::TaintedUrl, Some(stmt.into()), None);
                    return;
                }
                let reference = self
                    .current
                    .ir
                    .nodes
                    .get(&node)
                    .map_or_else(String::new, |n| n.path.0.clone());
                let id = RequestId(self.next_request);
                self.next_request = self.next_request.saturating_add(1);
                self.request_outbox.push(HostRequest {
                    id,
                    epoch: self.epoch,
                    origin: RequestOrigin::UserGesture { node },
                    kind: ReqKind::OpenUrl {
                        url: url.text.to_string(),
                        node: reference,
                        stmt: stmt.into(),
                    },
                });
            }
            Effect::Emit { name, params } => {
                let label = self.label(node);
                let name = crate::patch::strip_invisibles(&name);
                let params = crate::runtime::sanitize_value(params);
                let reference = self
                    .current
                    .ir
                    .nodes
                    .get(&node)
                    .map_or_else(String::new, |n| n.path.0.clone());
                let id = RequestId(self.next_request);
                self.next_request = self.next_request.saturating_add(1);
                self.request_outbox.push(HostRequest {
                    id,
                    epoch: self.epoch,
                    origin: RequestOrigin::UserGesture { node },
                    kind: ReqKind::Emit {
                        name: name.clone(),
                        params: params.clone(),
                        label: label.clone(),
                        node: reference.clone(),
                        stmt: stmt.into(),
                    },
                });
                let event = self.model_event(
                    "emit",
                    None,
                    Some(name),
                    Some(params),
                    &label,
                    None,
                    &reference,
                    stmt,
                );
                self.event_outbox.push(event);
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn model_event(
        &mut self,
        kind: &str,
        text: Option<String>,
        name: Option<String>,
        params: Option<crate::value::Value>,
        label: &str,
        ctx: Option<crate::value::Value>,
        node: &str,
        stmt: &str,
    ) -> ModelEvent {
        let mut event = ModelEvent {
            v: 1,
            source: "ui_event".into(),
            kind: kind.into(),
            text: text.map(|text| crate::patch::strip_invisibles(&text)),
            name: name.map(|name| crate::patch::strip_invisibles(&name)),
            params: params.map(crate::runtime::sanitize_value),
            label: crate::patch::strip_invisibles(label),
            ctx: ctx.map(crate::runtime::sanitize_value),
            node: node.into(),
            stmt: stmt.into(),
        };
        let limit = self.options.validation.limits.max_event_bytes;
        let mut truncated = false;
        if event_len(&event) > limit {
            truncated = true;
            // §7.8: drop optional context first, then trim text linearly.
            if event.ctx.take().is_some() && event_len(&event) > limit {
                // still oversized: fall through to the text trim
            }
            if event_len(&event) > limit
                && let Some(text) = event.text.take()
            {
                event.text = fit_text(&event, &text, limit);
            }
        }
        if truncated {
            self.push_host_diagnostic(Code::EventSize, Some(stmt.into()), None);
        }
        event
    }
    fn url_allowed(&self, url: &crate::value::PStr) -> bool {
        match url.prov {
            crate::value::Provenance::Host => true,
            crate::value::Provenance::User | crate::value::Provenance::Mixed => false,
            // Model literals need the allowlist; Tool results are accepted only
            // subject to the same host URL policy (§7.6).
            crate::value::Provenance::Model | crate::value::Provenance::Tool => {
                let text = url.text.as_ref();
                let Some((scheme, rest)) = text.split_once("://") else {
                    return false;
                };
                if !self.options.validation.url_schemes.contains(scheme) {
                    return false;
                }
                let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
                if authority.contains('@') {
                    return false;
                }
                let host = authority.split(':').next().unwrap_or("");
                self.options.validation.url_hosts.contains(host)
            }
        }
    }
    fn label(&self, node: crate::ir::NodeId) -> String {
        let Some(node) = self.live_ir().nodes.get(&node) else {
            return String::new();
        };
        let Some(component) = self.options.library.components().get(node.kind.0 as usize) else {
            return String::new();
        };
        let Some(index) = component.args.iter().position(|arg| arg.name == "label") else {
            return String::new();
        };
        let Ok(index) = u16::try_from(index) else {
            return String::new();
        };
        let prop_id = crate::ir::PropId(index);
        match node.props.get(&prop_id) {
            Some(crate::ir::Prop::Static(crate::ir::PropValue::Value(
                crate::value::Value::Str(text),
            ))) => crate::patch::strip_invisibles(&text.text),
            Some(crate::ir::Prop::Dynamic(_)) => self
                .resolved
                .prop(node.id, prop_id)
                .and_then(|value| match value {
                    crate::ir::PropValue::Value(crate::value::Value::Str(text)) => {
                        Some(crate::patch::strip_invisibles(&text.text))
                    }
                    _ => None,
                })
                .unwrap_or_default(),
            _ => String::new(),
        }
    }
    fn pump_queries(&mut self, initial: bool) {
        if !self.finished || self.rejected {
            return;
        }
        let names: Vec<_> = self.current.ir.queries.keys().cloned().collect();
        for name in names {
            let Some(query) = self.current.ir.queries.get(&name).cloned() else {
                continue;
            };
            if !query.allowed {
                continue;
            }
            let (args, diagnostics) = self.runtime.evaluate_expression(
                &self.current.ir,
                &self.options.library,
                &self.options.validation.limits,
                &self.options.validation.refs,
                &query.args,
                &name,
            );
            self.extend_host_diagnostics(diagnostics);
            let args = crate::runtime::sanitize_value(args);
            let hash = args_hash(&args);
            let force = self.refreshed.remove(&name);
            let current = self.queries.get(&name).cloned().unwrap_or_default();
            let changed = current.args_hash.is_some_and(|current| current != hash);
            if changed {
                if !force {
                    // A dependency change waits for the 300 ms debounce, even
                    // when the query is queued behind the concurrency cap.
                    let due = self.clock_ms.saturating_add(300);
                    self.debounce.entry(name.clone()).or_insert(due);
                    continue;
                }
            } else {
                self.debounce.remove(&name);
            }
            if let Some(request) = current.request {
                if force {
                    // A new key or a refresh supersedes the outstanding request;
                    // its delivery is no longer current (§7.5).
                    self.pending.remove(&request);
                } else {
                    continue;
                }
            }
            let send = force || current.queued || (initial && current.args_hash.is_none());
            if !send {
                continue;
            }
            let origin = current.origin.unwrap_or(RequestOrigin::Program);
            let Some(id) = self.enqueue(PendingKind::Query {
                epoch: self.epoch,
                name: name.clone(),
                args_hash: hash,
            }) else {
                let state = self.queries.entry(name).or_default();
                state.status = QueryStatus::Idle;
                state.queued = true;
                state.args_hash = Some(hash);
                continue;
            };
            let state = self.queries.entry(name.clone()).or_default();
            state.args_hash = Some(hash);
            state.request = Some(id);
            state.status = QueryStatus::Requested;
            state.queued = false;
            self.request_outbox.push(HostRequest {
                id,
                epoch: self.epoch,
                origin,
                kind: ReqKind::Query {
                    stmt: name.clone(),
                    tool: query.tool.clone(),
                    args,
                    args_hash: hash,
                    // ToolDescriptor.auto_run is not supplied yet; the spec default
                    // for an unregistered auto-run policy is host approval.
                    needs_approval: true,
                },
            });
        }
    }
    fn pump_refs(&mut self) {
        if !self.finished || self.rejected {
            return;
        }
        let mut attached: Vec<(String, String, String)> = Vec::new();
        for (node_id, _, value) in self.resolved.entries() {
            let crate::ir::PropValue::Value(crate::value::Value::Ref(reference)) = value else {
                continue;
            };
            let Some(node) = self.live_ir().nodes.get(&node_id) else {
                continue;
            };
            attached.push((node.path.0.clone(), reference.clone(), node.origin.clone()));
        }
        attached.sort();
        attached.dedup();
        for (path, reference, stmt) in attached {
            if !crate::runtime::valid_ref_id(&reference)
                || !self.options.validation.refs.contains(&reference)
            {
                continue;
            }
            if self.runtime.ref_value(&reference).is_some() {
                continue;
            }
            let current = self.refs_state.get(&reference).cloned().unwrap_or_default();
            if current.request.is_some()
                || matches!(
                    current.status,
                    QueryStatus::Failed | QueryStatus::Denied | QueryStatus::Ready
                )
            {
                continue;
            }
            let Some(id) = self.enqueue(PendingKind::Ref {
                epoch: self.epoch,
                id: reference.clone(),
            }) else {
                continue;
            };
            let state = self.refs_state.entry(reference.clone()).or_default();
            state.stmt = stmt.clone();
            state.path = Some(path.clone());
            state.request = Some(id);
            state.status = QueryStatus::Requested;
            self.request_outbox.push(HostRequest {
                id,
                epoch: self.epoch,
                origin: RequestOrigin::Program,
                kind: ReqKind::ResolveRef {
                    reference,
                    stmt,
                    path: Some(path),
                },
            });
        }
    }
    fn enqueue(&mut self, kind: PendingKind) -> Option<RequestId> {
        if self.pending.len() >= self.options.validation.limits.max_concurrent_requests {
            return None;
        }
        let id = RequestId(self.next_request);
        self.next_request = self.next_request.saturating_add(1);
        self.pending.insert(id, kind);
        Some(id)
    }
    fn push_host_diagnostic(&mut self, code: Code, stmt: Option<String>, path: Option<String>) {
        let mut diagnostic = Diagnostic::new(code, stmt, None);
        diagnostic.path = path;
        // Each host event is an independent occurrence (§4.2).
        self.host_diagnostics.push(diagnostic);
    }
    fn extend_host_diagnostics(&mut self, diagnostics: Vec<Diagnostic>) {
        // Evaluation diagnostics describe the current evaluation, not a new
        // event, so repeated evaluation must not accumulate duplicates.
        for diagnostic in diagnostics {
            let mut candidate = Diagnostic::new(diagnostic.code, diagnostic.stmt, None);
            candidate.path = diagnostic.path;
            if !self.host_diagnostics.iter().any(|existing| {
                existing.code == candidate.code
                    && existing.stmt == candidate.stmt
                    && existing.path == candidate.path
                    && existing.recovery == candidate.recovery
            }) {
                self.host_diagnostics.push(candidate);
            }
        }
    }
    fn remove_query_errors(&mut self, name: &str) {
        self.host_diagnostics.retain(|diagnostic| {
            !(matches!(diagnostic.code, Code::QueryDenied | Code::QueryFailed)
                && diagnostic.stmt.as_deref() == Some(name))
        });
    }
    pub fn diagnostics(&self) -> &[Diagnostic] {
        self.diagnostic_snapshot.get_or_init(|| {
            self.current
                .diagnostics
                .iter()
                .chain(self.syntax_diagnostics.iter())
                .chain(self.extracted_diagnostics.iter())
                .cloned()
                .collect()
        })
    }
    pub fn parser_work(&self) -> crate::syntax::streaming::ScanWork {
        self.parser.work()
    }
    pub fn seals(&self) -> &[Seal] {
        self.parser.seals()
    }
    pub fn provisional_origins(&self) -> &BTreeSet<String> {
        &self.provisional_origins
    }
    pub fn reused_instances(&self) -> usize {
        self.reused_instances
    }
    pub fn dirty(&self) -> &DirtyTracking {
        &self.tracking
    }
    pub fn segments(&self) -> &[Segment] {
        self.extractor
            .as_ref()
            .map_or(self.segments.as_slice(), |e| e.segments())
    }
    pub fn phase(&self) -> SessionPhase {
        if self.rejected
            || self.current.diagnostics.has_committed_fatal()
            || self.syntax_diagnostics.has_committed_fatal()
            || self.extracted_diagnostics.has_committed_fatal()
        {
            SessionPhase::Rejected
        } else if self.finished {
            SessionPhase::Done
        } else if self.current.ir.root.is_some() || !self.parser.seals().is_empty() {
            SessionPhase::Streaming
        } else {
            SessionPhase::Waiting
        }
    }
    /// Host results and request issuance change synthetic status/text props that
    /// the static projection did not predict. Drop the partial maintenance caches
    /// so the next diff observes them (§6.4).
    fn mark_tree_dirty(&mut self) {
        self.tree_dirty = true;
        self.projection = None;
        self.local_wire = None;
        self.local_grafts = None;
    }
    fn empty_update(&self) -> Update {
        Update {
            patches: None,
            diagnostics: Vec::new(),
            runtime_diagnostics: Vec::new(),
            requests: Vec::new(),
            model_events: Vec::new(),
            prose: Vec::new(),
            phase: self.phase(),
        }
    }
    fn rebuild(&mut self, finish: bool) {
        let previous_ir = self.current.ir.clone();
        self.before_static = previous_ir.clone();
        let previous_cache = self.current_cache.clone();
        let new_seals = self.parser.seals().len() != self.sealed_count;
        let changes: Vec<_> = self.parser.seals()[self.sealed_count..]
            .iter()
            .filter_map(|seal| seal.statement.clone())
            .collect();
        for seal in &self.parser.seals()[self.sealed_count..] {
            if let Some(s) = &seal.statement {
                self.tracking.change(s);
            }
        }
        let candidate = self
            .parser
            .provisional()
            .filter(|s| s.kind == StatementKind::Bind)
            .cloned();
        let candidate_changed = candidate != self.last_candidate
            || match (&candidate, &self.last_candidate) {
                (Some(a), Some(b)) => !a.value.recovery_metadata_eq(&b.value),
                _ => false,
            };
        if candidate_changed {
            if let Some(old) = &self.last_candidate
                && candidate.as_ref().is_none_or(|new| key(new) != key(old))
                && !self.parser.seals()[self.sealed_count..]
                    .iter()
                    .filter_map(|seal| seal.statement.as_ref())
                    .any(|sealed| key(sealed) == key(old))
            {
                self.tracking.remove(key(old));
            }
            if let Some(s) = &candidate {
                self.tracking.change(s);
            }
        }
        self.sealed_count = self.parser.seals().len();
        let syntax = self.parser.diagnostic_snapshot();
        let mut sealed_local = false;
        if new_seals || finish || !self.started {
            let retained = self.sealed_cache.as_ref().and_then(|cache| {
                crate::validate::validate_unrelated(
                    cache,
                    &self.sealed_validation,
                    &changes,
                    &self.tracking.dirty,
                    &self.options.library,
                    &self.options.validation,
                )
            });
            if let Some((checked, work, cache)) = retained {
                self.work += work;
                self.sealed_validation = checked;
                self.sealed_cache = Some(cache);
            } else {
                let use_current = self
                    .last_candidate
                    .as_ref()
                    .is_none_or(|old| changes.iter().any(|source| key(source) == key(old)));
                let (cache, base) = if use_current {
                    (self.current_cache.as_ref(), &self.current)
                } else {
                    (self.sealed_cache.as_ref(), &self.sealed_validation)
                };
                let attempt = cache.map(|cache| {
                    crate::validate::validate_props(
                        cache,
                        base,
                        &changes,
                        crate::validate::Invalidation {
                            dirty: &self.tracking.dirty,
                            reverse: &self.tracking.reverse,
                        },
                        None,
                        &self.options.library,
                        &self.options.validation,
                    )
                });
                if let Some(attempt) = &attempt {
                    self.work += attempt.work;
                }
                if let Some((checked, cache, grafts)) = attempt.and_then(|attempt| attempt.result) {
                    self.local_grafts = (!grafts.is_empty()).then_some(grafts);
                    sealed_local = use_current && self.current.ir.root == checked.ir.root;
                    self.sealed_validation = checked;
                    self.sealed_cache = Some(cache);
                } else {
                    let structural = self.sealed_cache.as_ref().map(|cache| {
                        crate::validate::resolve_waiting(
                            cache,
                            &self.sealed_validation,
                            &changes,
                            &self.tracking.dirty,
                            None,
                            &self.options.library,
                            &self.options.validation,
                        )
                    });
                    if let Some(attempt) = &structural {
                        self.work += attempt.work;
                    }
                    if let Some((checked, cache, grafts)) =
                        structural.and_then(|attempt| attempt.result)
                    {
                        self.sealed_validation = checked;
                        self.sealed_cache = Some(cache);
                        self.local_grafts = use_current.then_some(grafts);
                        sealed_local = use_current;
                    } else {
                        let (checked, reused, work, cache) = crate::validate::validate_incremental(
                            &self.options.library,
                            &self.options.validation,
                            Some(&self.sealed_validation),
                            crate::validate::TaskChanges {
                                dirty: &self.tracking.dirty,
                                sources: &changes,
                                reverse: &self.tracking.reverse,
                            },
                            None,
                            self.sealed_cache.as_ref(),
                            self.current_cache.as_ref(),
                        );
                        self.reused_instances += reused;
                        self.work += work;
                        self.sealed_validation = checked;
                        self.sealed_cache = Some(cache);
                        sealed_local = true;
                    }
                }
            }
        }
        if new_seals || candidate_changed || finish || !self.started {
            self.provisional_origins.clear();
            if let Some(s) = &candidate {
                let mut pending = vec![key(s)];
                while let Some(name) = pending.pop() {
                    if self.provisional_origins.insert(name.clone()) {
                        pending.extend(
                            self.tracking
                                .reverse
                                .get(&name)
                                .into_iter()
                                .flatten()
                                .cloned(),
                        );
                    }
                }
            }
            let mut preview_dirty = BTreeSet::new();
            if let Some(s) = &candidate {
                let mut pending = vec![key(s)];
                while let Some(name) = pending.pop() {
                    if preview_dirty.insert(name.clone()) {
                        pending.extend(
                            self.tracking
                                .reverse
                                .get(&name)
                                .into_iter()
                                .flatten()
                                .cloned(),
                        );
                    }
                }
            }
            let mut retained_graph = false;
            let mut local_update = sealed_local;
            let mut value = if let Some(s) = &candidate {
                let retained = self.sealed_cache.as_ref().and_then(|cache| {
                    crate::validate::validate_unrelated(
                        cache,
                        &self.sealed_validation,
                        std::slice::from_ref(s),
                        &preview_dirty,
                        &self.options.library,
                        &self.options.validation,
                    )
                });
                if let Some((checked, work, cache)) = retained {
                    self.work += work;
                    retained_graph = true;
                    self.current_cache = Some(cache);
                    checked
                } else {
                    let attempt = if !new_seals
                        && self
                            .last_candidate
                            .as_ref()
                            .is_none_or(|old| key(old) == key(s))
                    {
                        self.current_cache.as_ref().map(|cache| {
                            crate::validate::validate_props(
                                cache,
                                &self.current,
                                std::slice::from_ref(s),
                                crate::validate::Invalidation {
                                    dirty: &preview_dirty,
                                    reverse: &self.tracking.reverse,
                                },
                                Some(&self.sealed_validation.ir.states),
                                &self.options.library,
                                &self.options.validation,
                            )
                        })
                    } else {
                        None
                    };
                    if let Some(attempt) = &attempt {
                        self.work += attempt.work;
                    }
                    if let Some((checked, cache, grafts)) =
                        attempt.and_then(|attempt| attempt.result)
                    {
                        self.local_grafts = (!grafts.is_empty()).then_some(grafts);
                        self.current_cache = Some(cache);
                        local_update = self.current.ir.root == checked.ir.root;
                        self.reused_instances += self.current.ir.nodes.len().saturating_sub(
                            self.current.ir.nodes.changed_keys(&checked.ir.nodes).len(),
                        );
                        checked
                    } else {
                        let structural = if !new_seals {
                            self.sealed_cache.as_ref().map(|cache| {
                                crate::validate::resolve_waiting(
                                    cache,
                                    &self.sealed_validation,
                                    std::slice::from_ref(s),
                                    &preview_dirty,
                                    Some(&self.sealed_validation.ir.states),
                                    &self.options.library,
                                    &self.options.validation,
                                )
                            })
                        } else {
                            None
                        };
                        if let Some(attempt) = &structural {
                            self.work += attempt.work;
                        }
                        if let Some((checked, cache, grafts)) =
                            structural.and_then(|attempt| attempt.result)
                        {
                            self.current_cache = Some(cache);
                            self.local_grafts = Some(grafts);
                            local_update = true;
                            checked
                        } else {
                            local_update = true;
                            let (checked, reused, work, cache) =
                                crate::validate::validate_incremental(
                                    &self.options.library,
                                    &self.options.validation,
                                    Some(&self.sealed_validation),
                                    crate::validate::TaskChanges {
                                        dirty: &preview_dirty,
                                        sources: std::slice::from_ref(s),
                                        reverse: &self.tracking.reverse,
                                    },
                                    Some(&self.sealed_validation.ir.states),
                                    self.sealed_cache.as_ref(),
                                    self.current_cache.as_ref(),
                                );
                            self.reused_instances += reused;
                            self.work += work;
                            self.current_cache = Some(cache);
                            checked
                        }
                    }
                }
            } else {
                self.current_cache = self.sealed_cache.clone();
                self.sealed_validation.clone()
            };
            if !finish {
                // No state declaration or type inference is committed by an unsealed binding.
                value.ir.states = self.sealed_validation.ir.states.clone();
                value.ir.queries = self.sealed_validation.ir.queries.clone();
                if local_update && let Some(s) = &candidate {
                    let changed = self.current.ir.nodes.changed_keys(&value.ir.nodes);
                    let mut removed_actions = Vec::new();
                    for id in changed {
                        let needs_mutation = value.ir.nodes.get(&id).is_some_and(|node| {
                            node.origin == s.name
                                || (self.provisional_origins.contains(&node.origin)
                                    && node.props.values().any(|prop| {
                                        matches!(prop, Prop::Static(PropValue::Action(_)))
                                    }))
                        });
                        if needs_mutation && let Some(node) = value.ir.nodes.get_mut(&id) {
                            if node.origin == s.name {
                                node.status = NodeStatus::Partial;
                            }
                            if self.provisional_origins.contains(&node.origin) {
                                node.props.retain(|_, prop| {
                                    if let Prop::Static(PropValue::Action(id)) = prop {
                                        removed_actions.push(id.clone());
                                        false
                                    } else {
                                        true
                                    }
                                });
                            }
                        }
                    }
                    for id in removed_actions {
                        value.ir.drop_action(&id);
                    }
                    // A draft's reverse closure includes sealed consumers. While
                    // any of those are provisional, no action plan owned by them
                    // may be published, even if the node itself did not change.
                    let stale: Vec<_> = value
                        .ir
                        .actions
                        .ids()
                        .filter(|id| {
                            value.ir.actions.get(id).is_some_and(|plan| {
                                value.ir.nodes.get(&plan.origin).is_some_and(|node| {
                                    self.provisional_origins.contains(&node.origin)
                                })
                            })
                        })
                        .cloned()
                        .collect();
                    for id in stale {
                        let origin = value.ir.actions.get(&id).map(|plan| plan.origin);
                        if let Some(origin) = origin
                            && let Some(node) = value.ir.nodes.get_mut(&origin)
                        {
                            node.props.retain(|_, prop| {
                                !matches!(prop, Prop::Static(PropValue::Action(handle)) if *handle == id)
                            });
                        }
                        value.ir.drop_action(&id);
                    }
                }
                if let Some(s) = &candidate
                    && !retained_graph
                    && !local_update
                {
                    self.work.instances_examined += 3 * value.ir.nodes.len() as u64;
                    for id in value.ir.nodes.keys().copied().collect::<Vec<_>>() {
                        let Some(n) = value.ir.nodes.get_mut(&id) else {
                            continue;
                        };
                        if n.origin == s.name {
                            n.status = NodeStatus::Partial;
                        }
                        if self.provisional_origins.contains(&n.origin) {
                            n.props
                                .retain(|_, p| !matches!(p, Prop::Static(PropValue::Action(_))));
                        }
                    }
                    let active_actions: BTreeSet<_> = value
                        .ir
                        .nodes
                        .values()
                        .flat_map(|n| n.props.values())
                        .filter_map(|p| {
                            if let Prop::Static(PropValue::Action(id)) = p {
                                Some(id.clone())
                            } else {
                                None
                            }
                        })
                        .collect();
                    let removed = value
                        .ir
                        .actions
                        .ids()
                        .filter(|id| {
                            !active_actions.contains(*id)
                                || value.ir.actions.get(id).is_none_or(|a| {
                                    value
                                        .ir
                                        .nodes
                                        .get(&a.origin)
                                        .is_none_or(|n| n.status != NodeStatus::Sealed)
                                })
                        })
                        .cloned()
                        .collect::<Vec<_>>();
                    for id in removed {
                        value.ir.drop_action(&id);
                    }
                }
                value.diagnostics.force_provisional(true);
            }
            if self.rejected {
                value.ir = Ir::default();
                value.diagnostics.clear();
            }
            let graph_changed = self.current.ir.root != value.ir.root
                || !self.current.ir.nodes.ptr_eq(&value.ir.nodes)
                || !self.current.ir.expressions.ptr_eq(&value.ir.expressions)
                || !self.current.ir.actions.ptr_eq(&value.ir.actions);
            self.tree_dirty |= graph_changed;
            if local_update {
                let changed = self.current.ir.nodes.changed_keys(&value.ir.nodes);
                for id in &changed {
                    if let Some(node) = value.ir.nodes.get(id) {
                        let rev = if candidate.as_ref().is_some_and(|s| s.name == node.origin) {
                            self.tracking.instance(&node.origin)
                        } else if let Some(old) = self.tracking.instances.get(&node.path.0) {
                            let mut rev = old.clone();
                            for name in &self.tracking.dirty {
                                if rev.dep_revs.contains_key(name) {
                                    rev.dep_revs.insert(
                                        name.clone(),
                                        self.tracking.revisions.get(name).copied().unwrap_or(0),
                                    );
                                }
                            }
                            rev
                        } else {
                            self.tracking.instance(&node.origin)
                        };
                        self.tracking.instances.insert(node.path.0.clone(), rev);
                    } else if let Some(old) = self.current.ir.nodes.get(id) {
                        self.tracking.instances.remove(&old.path.0);
                    }
                }
                let grafted: BTreeSet<_> = self
                    .local_grafts
                    .iter()
                    .flatten()
                    .flat_map(|graft| graft.children.iter().copied())
                    .collect();
                self.local_wire = Some(
                    changed
                        .into_iter()
                        .filter(|id| {
                            if grafted.contains(id) {
                                return false;
                            }
                            match (self.current.ir.nodes.get(id), value.ir.nodes.get(id)) {
                                (Some(old), Some(new)) => {
                                    old.kind != new.kind
                                        || old.props != new.props
                                        || old.status != new.status
                                }
                                _ => self.local_grafts.is_none(),
                            }
                        })
                        .collect(),
                );
            } else if graph_changed {
                self.local_wire = None;
                self.local_grafts = None;
                self.work.instances_examined += value.ir.nodes.len() as u64;
                let old = std::mem::take(&mut self.tracking.instances);
                for n in value.ir.nodes.values() {
                    let rev = self.tracking.instance(&n.origin);
                    let rev = old
                        .get(&n.path.0)
                        .filter(|old| **old == rev)
                        .cloned()
                        .unwrap_or(rev);
                    self.tracking.instances.insert(n.path.0.clone(), rev);
                }
            }
            self.current = value;
        }
        self.last_candidate = candidate;
        self.diagnostic_snapshot.take();
        let has_bad_utf8 = self.extraction_diagnostics.contains_code(Code::BadUtf8);
        self.syntax_diagnostics = if self.rejected {
            Default::default()
        } else if has_bad_utf8 {
            syntax.excluding(Code::BadUtf8)
        } else {
            syntax
        };
        self.extracted_diagnostics = self.extraction_diagnostics.clone();
        if finish {
            self.current.diagnostics.force_provisional(false);
            self.syntax_diagnostics.force_provisional(false);
            self.extracted_diagnostics.force_provisional(false);
        }
        if self.extraction_diagnostics.contains_code(Code::Version) {
            self.rejected = true;
            self.current.ir = Ir::default();
        }
        self.projection = Some(crate::validate::Projection::between(
            &previous_ir,
            &self.current.ir,
            previous_cache.as_ref(),
            self.current_cache.as_ref(),
        ));
        self.evaluate();
    }
    fn evaluate(&mut self) {
        if self.rejected {
            self.resolved = crate::runtime::Resolved::default();
            self.runtime_diagnostics.clear();
            self.field_status.clear();
            return;
        }
        self.runtime.state.sync(&self.current.ir.states);
        let next = self.runtime.evaluate(
            &self.current.ir,
            &self.options.library,
            &self.options.validation.limits,
            &self.options.validation.refs,
        );
        let a = runtime_projection_present(&self.before_static, self.resolved.ir());
        let b = runtime_projection_changed(
            &self.before_static,
            &self.current.ir,
            self.resolved.ir(),
            next.ir(),
        );
        let structure_changed = a || b;
        self.resolved = next;
        if structure_changed {
            // Runtime plans changed the tree independently of validation.
            self.tree_dirty = true;
            self.projection = None;
            self.local_wire = None;
            self.local_grafts = None;
        }
        let live = self.resolved.ir().clone();
        self.fields =
            crate::runtime::FieldRegistry::build(&live, &self.options.library, &self.resolved);
        self.field_status = self.fields.all(&self.runtime.state, &self.touched);
        self.resolve_field_props();
        self.resolve_status_props(&live);
        let mut diagnostics = self.resolved.diagnostics().to_vec();
        diagnostics.extend(self.action_diagnostics.iter().cloned());
        diagnostics.extend(self.host_diagnostics.iter().cloned());
        if !self.finished {
            for diagnostic in &mut diagnostics {
                diagnostic.provisional = true;
            }
        }
        diagnostics.sort_by(|a, b| {
            (&a.stmt, &a.path, a.code, a.recovery).cmp(&(&b.stmt, &b.path, b.code, b.recovery))
        });
        self.runtime_diagnostics = diagnostics;
    }
    /// Mirror §7.3 field status into synthetic resolved props so the renderer
    /// never decides validity itself.
    fn resolve_field_props(&mut self) {
        let statuses = self.field_status.clone();
        for (path, status) in statuses {
            let Some(node_id) = self
                .resolved
                .ir()
                .nodes
                .values()
                .find(|node| node.path.0 == path)
                .map(|node| node.id)
            else {
                continue;
            };
            let error = status.error.map_or(crate::value::Value::Null, |error| {
                crate::value::Value::from(match error {
                    crate::runtime::FieldError::Required => "required",
                    crate::runtime::FieldError::Format => "format",
                    crate::runtime::FieldError::TooShort => "too_short",
                    crate::runtime::FieldError::TooLong => "too_long",
                    crate::runtime::FieldError::TooSmall => "too_small",
                    crate::runtime::FieldError::TooLarge => "too_large",
                })
            });
            self.resolved
                .set(node_id, crate::runtime::PROP_ERROR, PropValue::Value(error));
            self.resolved.set(
                node_id,
                crate::runtime::PROP_TOUCHED,
                PropValue::Value(crate::value::Value::Bool(status.touched)),
            );
        }
    }

    /// Attach the aggregated query status to nodes whose props read a query
    /// (§7.5). It is a resolved prop and never visible to expressions.
    fn resolve_status_props(&mut self, ir: &Ir) {
        if self.queries.is_empty() {
            return;
        }
        for node in ir.nodes.values() {
            let mut queries = BTreeSet::new();
            for prop in node.props.values() {
                if let Prop::Dynamic(expression) = prop
                    && let Some(expression) = ir.expressions.get(expression)
                {
                    let mut deps = BTreeSet::new();
                    dependencies(expression, &mut deps);
                    queries.extend(
                        deps.into_iter()
                            .filter(|name| self.current.ir.queries.contains_key(name)),
                    );
                }
            }
            if queries.is_empty() {
                continue;
            }
            let status = queries.iter().fold(QueryStatus::Idle, |worst, name| {
                let status = self
                    .queries
                    .get(name)
                    .map_or(QueryStatus::Idle, |query| query.status);
                aggregate_status(worst, status)
            });
            self.resolved.set(
                node.id,
                crate::runtime::PROP_STATUS,
                PropValue::Value(crate::value::Value::from(status_name(status))),
            );
        }
    }

    fn update(&mut self, emit_empty: bool, snapshot: bool) -> Update {
        let local = if snapshot {
            None
        } else {
            self.local_wire.take()
        };
        let grafts = if snapshot {
            None
        } else {
            self.local_grafts.take()
        };
        let projection = self.projection.take();
        let live = self.live_ir().clone();
        let ops = if snapshot || !self.started {
            let (tree, work) =
                Tree::from_ir_measured(&live, &self.options.library, Some(&self.resolved));
            self.work += work;
            let ops = tree.snapshot_ops();
            self.tree = tree;
            self.tree_dirty = false;
            ops
        } else if let Some(projection) = projection {
            let (ops, work) = self.tree.maintain(
                &live,
                &self.options.library,
                Some(&self.resolved),
                &projection,
            );
            self.work += work;
            self.tree_dirty = false;
            ops
        } else if let Some(mut grafts) = grafts {
            grafts.sort_by_key(|graft| graft.order.clone());
            let (mut ops, work) = self.tree.insert_grafts(
                &live,
                &self.options.library,
                Some(&self.resolved),
                &grafts,
            );
            self.work += work;
            if let Some(mut ids) = local {
                ids.sort_by_key(|id| {
                    self.current_cache
                        .as_ref()
                        .and_then(|cache| cache.node_order(id))
                        .cloned()
                });
                let (changes, work) = self.tree.update_props(
                    &live,
                    &self.options.library,
                    Some(&self.resolved),
                    &ids,
                );
                self.work += work;
                ops.extend(changes);
            }
            self.tree_dirty = false;
            ops
        } else if let Some(mut ids) = local {
            ids.sort_by_key(|id| {
                self.current_cache
                    .as_ref()
                    .and_then(|cache| cache.node_order(id))
                    .cloned()
            });
            let (ops, work) =
                self.tree
                    .update_props(&live, &self.options.library, Some(&self.resolved), &ids);
            self.work += work;
            self.tree_dirty = false;
            ops
        } else if self.tree_dirty || snapshot || !self.started {
            let (next, work) =
                Tree::from_ir_measured(&live, &self.options.library, Some(&self.resolved));
            self.work += work;
            let ops = if snapshot || !self.started {
                next.snapshot_ops()
            } else {
                let (ops, work) = self.tree.diff_measured(&next);
                self.work += work;
                ops
            };
            self.tree = next;
            self.tree_dirty = false;
            ops
        } else {
            Vec::new()
        };
        let patches = if emit_empty || !ops.is_empty() {
            let batch = PatchBatch {
                epoch: self.epoch,
                seq: self.next_seq,
                ops,
            };
            if let Some(seq) = self.next_seq.checked_add(1) {
                self.next_seq = seq;
            } else {
                self.rejected = true;
            }
            self.started = true;
            Some(batch)
        } else {
            None
        };
        let mut prose = Vec::new();
        let segments = self
            .extractor
            .as_ref()
            .map_or(self.segments.as_slice(), |e| e.segments());
        for (i, s) in segments.iter().enumerate() {
            if let Segment::Markdown(text) = s {
                let n = self.prose_sent.entry(i as u32).or_default();
                if let Some(new) = text.get(*n..)
                    && !new.is_empty()
                {
                    prose.push(ProseDelta {
                        segment: i as u32,
                        append: strip_invisibles(new),
                    });
                }
                *n = text.len();
            }
        }
        self.not_sync.set(());
        let layers = [
            &self.current.diagnostics,
            &self.syntax_diagnostics,
            &self.extracted_diagnostics,
        ];
        let mut diagnostics = Vec::new();
        for (layer, previous) in layers.into_iter().zip(&mut self.published) {
            diagnostics.extend(layer.additions_since(previous, &mut self.work));
            *previous = layer.clone();
        }
        Update {
            patches,
            diagnostics,
            runtime_diagnostics: self.runtime_diagnostics.clone(),
            requests: std::mem::take(&mut self.request_outbox),
            model_events: std::mem::take(&mut self.event_outbox),
            prose,
            phase: self.phase(),
        }
    }
}
fn event_len(event: &ModelEvent) -> usize {
    crate::canonical::serialize(event).map_or(usize::MAX, |json| json.len())
}

/// Trim `text` once for the canonical event size (§7.8). The prefix is chosen
/// in one pass over characters; no repeated whole-event serialization.
fn fit_text(event: &ModelEvent, text: &str, limit: usize) -> Option<String> {
    let base = event_len(event);
    let budget = limit.checked_sub(base.saturating_add(8))?;
    if budget < 2 {
        return None;
    }
    let mut used = 2usize;
    let mut output = String::new();
    for ch in text.chars() {
        let size = match ch {
            '"' | '\\' | '\n' | '\r' | '\t' | '\u{0008}' | '\u{000c}' => 2,
            ch if ch < '\u{0020}' => 6,
            ch => ch.len_utf8(),
        };
        if used.saturating_add(size) > budget {
            break;
        }
        used += size;
        output.push(ch);
    }
    Some(output)
}

fn aggregate_status(a: QueryStatus, b: QueryStatus) -> QueryStatus {
    fn rank(status: QueryStatus) -> u8 {
        match status {
            QueryStatus::Idle => 0,
            QueryStatus::Ready => 1,
            QueryStatus::Requested | QueryStatus::Refreshing => 2,
            QueryStatus::Denied => 3,
            QueryStatus::Failed => 4,
        }
    }
    if rank(b) > rank(a) { b } else { a }
}
fn status_name(status: QueryStatus) -> &'static str {
    match status {
        QueryStatus::Idle => "idle",
        QueryStatus::Requested => "requested",
        QueryStatus::Ready => "ready",
        QueryStatus::Denied => "denied",
        QueryStatus::Failed => "failed",
        QueryStatus::Refreshing => "refreshing",
    }
}

/// True when a projection contains instantiated nodes or an unexpanded plan, so
/// incremental static projection cannot describe the wire tree on its own.
fn runtime_projection_present(static_ir: &Ir, expanded: &Ir) -> bool {
    static_ir.root_plan.is_some()
        || static_ir
            .nodes
            .values()
            .any(|node| node.slots.iter().any(|slot| !slot.plans.is_empty()))
        || expanded
            .nodes
            .values()
            .any(|node| !static_ir.nodes.contains_key(&node.id))
        || static_ir
            .nodes
            .keys()
            .any(|id| !expanded.nodes.contains_key(id))
}

fn runtime_projection_changed(
    before_static: &Ir,
    current_static: &Ir,
    previous: &Ir,
    next: &Ir,
) -> bool {
    if runtime_projection_present(before_static, previous)
        || runtime_projection_present(current_static, next)
    {
        return previous.root != next.root
            || previous.nodes.len() != next.nodes.len()
            || previous
                .nodes
                .iter()
                .zip(next.nodes.iter())
                .any(|((a, before), (b, after))| a != b || before != after);
    }
    false
}

fn parents(ir: &Ir) -> BTreeMap<crate::ir::NodeId, crate::ir::NodeId> {
    let mut parents = BTreeMap::new();
    for node in ir.nodes.values() {
        for slot in &node.slots {
            for child in slot.children.iter() {
                parents.insert(*child, node.id);
            }
        }
    }
    parents
}
fn args_hash(value: &crate::value::Value) -> u64 {
    xxhash_rust::xxh3::xxh3_64(crate::canonical::encode(&value.plain_json()).as_bytes())
}
fn key(s: &Statement) -> String {
    if s.kind == StatementKind::State {
        format!("${}", s.name)
    } else {
        s.name.clone()
    }
}
pub(crate) fn dependencies(e: &Expr, out: &mut BTreeSet<String>) {
    match e {
        Expr::Name { name } => {
            out.insert(name.clone());
        }
        Expr::State { name } => {
            out.insert(format!("${name}"));
        }
        Expr::List { items } => {
            for e in items {
                dependencies(e, out);
            }
        }
        Expr::Map { entries } => {
            for e in entries {
                dependencies(&e.value, out);
            }
        }
        Expr::Call { args, .. } => {
            for a in args {
                dependencies(&a.value, out);
            }
        }
        Expr::Unary { expr, .. } | Expr::Member { base: expr, .. } => dependencies(expr, out),
        Expr::Binary { left, right, .. }
        | Expr::Index {
            base: left,
            index: right,
        } => {
            dependencies(left, out);
            dependencies(right, out);
        }
        Expr::Lambda { params, body } => {
            let mut deps = BTreeSet::new();
            dependencies(body, &mut deps);
            out.extend(deps.into_iter().filter(|n| !params.contains(n)));
        }
        _ => {}
    }
}

#[cfg(test)]
mod session_contract_tests {
    use super::*;
    use crate::r#lib::{ComponentDef, GroupDef, Library, Role, T};

    fn library() -> Arc<Library> {
        Arc::new(
            Library::builder("test", "0.1.0")
                .group(GroupDef::new("G", 0, "`Leaf` and `Stack`."))
                .component(
                    ComponentDef::new("Leaf", Role::Paragraph)
                        .group("G")
                        .doc("A leaf.")
                        .req("text", T::text())
                        .example("Leaf(\"ok\")"),
                )
                .component(
                    ComponentDef::new("Stack", Role::Group)
                        .group("G")
                        .doc("A stack.")
                        .req("children", T::nodes())
                        .example("Stack([])"),
                )
                .component(
                    ComponentDef::new("Control", Role::Button)
                        .group("G")
                        .doc("A control.")
                        .opt_none("on_click", T::action())
                        .opt_none("label", T::text())
                        .example("Control()"),
                )
                .component(
                    ComponentDef::new("ShowList", Role::Paragraph)
                        .group("G")
                        .doc("A list binding sink.")
                        .req("value", T::binding(T::list(T::text()))),
                )
                .build()
                .unwrap(),
        )
    }

    fn session() -> Session {
        Session::new(SessionOptions::program(library()))
    }

    fn node<'a>(session: &'a Session, path: &str) -> &'a crate::ir::Node {
        session
            .ir()
            .nodes
            .values()
            .find(|node| node.path.0 == path)
            .unwrap_or_else(|| panic!("missing node {path}"))
    }

    #[test]
    fn phase_reports_waiting_and_rejection_from_each_committed_layer() {
        let waiting = session();
        assert_eq!(waiting.phase(), SessionPhase::Waiting);

        let mut rejected = session();
        rejected.push(b"x=1\n");
        rejected.finish();
        assert!(
            rejected.current.diagnostics.has_committed_fatal(),
            "{:?}",
            rejected.diagnostics()
        );
        assert_eq!(rejected.phase(), SessionPhase::Rejected);
    }
    #[test]
    fn a_draft_publishes_its_reverse_provisional_closure() {
        let mut session = session();
        session.push(b"root=Stack([draft])\ndraft=Leaf(\"a");
        let origins = session.provisional_origins();
        assert!(origins.contains("draft"), "{origins:?}");
        assert!(
            origins.contains("root"),
            "sealed consumers of a draft stay provisional: {origins:?}"
        );
    }
    #[test]
    fn sealing_an_unrelated_definition_reuses_retained_instances() {
        let mut session = session();
        session.push(b"root=Stack([a])\na=Leaf(\"x\")\n");
        let before = session.reused_instances();
        session.push(b"extra=1\n");
        assert!(
            session.reused_instances() > before,
            "the unchanged displayed subtree must be reused"
        );
    }
    #[test]
    fn withdrawing_a_stale_action_keeps_the_nodes_other_props() {
        let mut session = session();
        session.push(b"root=Stack([Control(on_click:say(\"go\"),label:\"Keep\"),draft])\n");
        assert_eq!(
            session.ir().actions.len(),
            1,
            "the sealed control publishes its action plan"
        );
        session.push(b"draft=Leaf(\"a");
        assert!(session.ir().actions.is_empty());
        let control = session
            .ir()
            .nodes
            .values()
            .find(|node| node.kind.0 == 2)
            .expect("control node");
        assert!(
            control.props.values().any(
                |prop| matches!(prop, Prop::Static(PropValue::Value(value)) if value.string() == Some("Keep"))
            ),
            "the stale action withdrawal must not drop the label: {:?}",
            control.props
        );
    }
    #[test]
    fn a_recovered_null_becoming_a_literal_rebuilds_the_draft_candidate() {
        let mut session = session();
        session.push(
            b"root=Stack([draft])\ndraft=Leaf(99999999999999999999999999999999999999999999999999",
        );
        let before = session.dirty().revisions["draft"];
        session.push(b",");
        assert!(
            session.dirty().revisions["draft"] > before,
            "a recovery-metadata-only candidate change must still be tracked"
        );
    }
    #[test]
    fn a_visible_draft_change_advances_its_instance_revision() {
        let mut session = session();
        session.push(b"root=Stack([draft])\ndraft=Leaf(\"a");
        let path = "root/children:@draft";
        let first = session.dirty().instances[path].origin_rev;
        session.push(b"b");
        let second = session.dirty().instances[path].origin_rev;
        assert!(
            second > first,
            "a changed draft-owned node must take the candidate's new revision: {first} -> {second}"
        );
    }
    #[test]
    fn a_visible_draft_change_advances_its_instance_revision_once() {
        let mut session = session();
        session.push(b"root=Stack([draft])\ndraft=Leaf(\"a");
        let path = "root/children:@draft";
        let first = session.dirty().revisions["draft"];
        session.push(b"b");
        let second = session.dirty().revisions["draft"];
        assert_eq!(
            second - first,
            1,
            "a retained candidate edit is one change, not remove plus change"
        );
        assert!(session.dirty().instances.contains_key(path));
    }
    #[test]
    fn sealing_a_late_definition_revalidates_the_preview_without_losing_it() {
        let mut session = session();
        session.push(b"root=Stack([a,b])\na=Leaf(\"x\")\n");
        session.push(b"b=Leaf(\"y\")\nc=Leaf(\"z");
        let paths: Vec<_> = session
            .ir()
            .nodes
            .values()
            .map(|node| node.path.0.as_str())
            .collect();
        assert!(
            paths.contains(&"root/children:@b"),
            "the newly sealed definition must join the preview: {paths:?}"
        );
        assert!(session.provisional_origins().contains("c"), "{paths:?}");
    }
    #[test]
    fn a_visible_draft_text_edits_one_node_and_then_seals_in_place() {
        let mut session = session();
        session.push(b"root=Stack([a,draft])\na=Leaf(\"x\")\ndraft=Leaf(\"a");
        let draft = node(&session, "root/children:@draft");
        assert_eq!(
            draft.props.get(&crate::ir::PropId(0)),
            Some(&Prop::Static(PropValue::Value(crate::value::Value::from(
                "a"
            ))))
        );
        assert_eq!(draft.status, NodeStatus::Partial);

        let update = session.push(b"b");
        assert_eq!(
            update.patches.expect("draft edit patch").ops,
            vec![crate::patch::Op::AppendText {
                id: "root/children:@draft".into(),
                prop: "text".into(),
                text: "b".into(),
            }]
        );
        assert_eq!(
            node(&session, "root/children:@draft")
                .props
                .get(&crate::ir::PropId(0)),
            Some(&Prop::Static(PropValue::Value(crate::value::Value::from(
                "ab"
            ))))
        );

        let update = session.push(b"\")\n");
        assert!(update.patches.is_some());
        let draft = node(&session, "root/children:@draft");
        assert_eq!(draft.status, NodeStatus::Sealed);
        assert_eq!(
            draft.props.get(&crate::ir::PropId(0)),
            Some(&Prop::Static(PropValue::Value(crate::value::Value::from(
                "ab"
            ))))
        );
        assert_eq!(session.phase(), SessionPhase::Streaming);
        assert!(
            session.diagnostics().is_empty(),
            "{:?}",
            session.diagnostics()
        );
    }

    #[test]
    fn provisional_action_props_stay_unpublished_until_the_source_seals() {
        let mut session = session();
        session.push(b"root=Stack([draft])\ndraft=Control(on_click:say(\"hi\"),label:\"keep\")");
        let draft = node(&session, "root/children:@draft");
        assert_eq!(draft.status, NodeStatus::Partial);
        assert!(
            draft
                .props
                .values()
                .all(|prop| !matches!(prop, Prop::Static(PropValue::Action(_)))),
            "{:?}",
            draft.props
        );
        assert!(
            draft
                .props
                .values()
                .any(|prop| matches!(prop, Prop::Static(PropValue::Value(value)) if value.string() == Some("keep"))),
            "{:?}",
            draft.props
        );
        assert_eq!(session.ir().actions.len(), 0);
        assert!(
            session.diagnostics().is_empty(),
            "{:?}",
            session.diagnostics()
        );

        let update = session.push(b")\n");
        assert!(update.patches.is_some());
        let draft = node(&session, "root/children:@draft");
        assert_eq!(draft.status, NodeStatus::Sealed);
        assert!(
            draft
                .props
                .values()
                .any(|prop| matches!(prop, Prop::Static(PropValue::Action(_)))),
            "{:?}",
            draft.props
        );
        assert_eq!(session.ir().actions.len(), 1);
        assert_eq!(session.provisional_origins().len(), 0);
    }

    #[test]
    fn a_first_root_candidate_is_published_as_partial_with_bounded_work() {
        let mut session = session();
        let before = session.maintenance_work();
        let update = session.push(b"root=Leaf(\"a");
        let after = session.maintenance_work();
        let draft = node(&session, "root");
        assert_eq!(draft.status, NodeStatus::Partial);
        assert_eq!(update.phase, SessionPhase::Streaming);
        assert!(
            after.instances_examined - before.instances_examined <= 8,
            "{before:?} -> {after:?}"
        );

        let update = session.push(b"b\")\n");
        assert!(update.patches.is_some());
        session.finish();
        let root = node(&session, "root");
        assert_eq!(root.status, NodeStatus::Sealed);
        assert_eq!(
            root.props.get(&crate::ir::PropId(0)),
            Some(&Prop::Static(PropValue::Value(crate::value::Value::from(
                "ab"
            ))))
        );
    }

    #[test]
    fn live_reinference_keeps_an_empty_list_binding_target() {
        let mut session = session();
        session.push(b"$xs = []\nroot = Stack([ShowList($xs)])\n");
        assert_eq!(
            session.ir().states["xs"].ty,
            T::list(T::text()),
            "the first binding target widens the empty-list state"
        );
        session.push(b"extra = 1\n");
        assert_eq!(
            session.ir().states["xs"].ty,
            T::list(T::text()),
            "live inference must not retract the widened list type"
        );
    }

    #[test]
    fn sealing_an_unrelated_definition_leaves_the_displayed_tree_unchanged() {
        let mut session = session();
        session.push(b"root=Stack([a])\na=Leaf(\"x\")\n");
        let before: Vec<_> = session
            .tree()
            .nodes
            .iter()
            .map(|(path, node)| (path.clone(), node.clone()))
            .collect();
        let work = session.maintenance_work();
        session.push(b"extra=1\n");
        let after: Vec<_> = session
            .tree()
            .nodes
            .iter()
            .map(|(path, node)| (path.clone(), node.clone()))
            .collect();
        assert_eq!(before, after);
        let after = session.maintenance_work();
        assert_eq!(
            after.statements_checked - work.statements_checked,
            6,
            "{work:?} -> {after:?}"
        );
        assert_eq!(
            after.diagnostics_examined - work.diagnostics_examined,
            2,
            "{work:?} -> {after:?}"
        );
        assert_eq!(
            after.instances_examined, work.instances_examined,
            "{work:?} -> {after:?}"
        );
        assert!(session.diagnostics().iter().any(|d| d.code == Code::Orphan));
    }

    #[test]
    fn maintenance_work_includes_retained_scanner_diagnostics() {
        let mut session = session();
        session.push(b"root=Stack([a])\na=Leaf(\"x\")\n");
        let base = session.maintenance_work();
        assert_eq!(
            base.diagnostics_examined,
            session.work.diagnostics_examined + session.parser_work().diagnostics_examined as u64
        );
        session.push(b"bad=1 &\n");
        let after = session.maintenance_work();
        assert!(
            after.diagnostics_examined > base.diagnostics_examined,
            "{base:?} -> {after:?}"
        );
        assert_eq!(
            after.diagnostics_examined,
            session.work.diagnostics_examined + session.parser_work().diagnostics_examined as u64
        );
    }
}

#[cfg(test)]
mod tracking_tests {
    use super::*;
    fn statement(source: &str) -> Statement {
        let mut parsed =
            crate::syntax::parse_batch(source.as_bytes(), crate::limits::SyntaxLimits::default());
        parsed.program.statements.pop().expect("one statement")
    }
    #[test]
    fn dependency_collection_covers_unary_member_binary_index_and_lambda() {
        for (source, expected) in [
            ("x=-a.b\n", vec!["a"]),
            ("x=a+b[0]\n", vec!["a", "b"]),
            ("x=!a.b + c[1]\n", vec!["a", "c"]),
        ] {
            let statement = statement(source);
            let mut deps = BTreeSet::new();
            dependencies(&statement.value, &mut deps);
            assert_eq!(deps.into_iter().collect::<Vec<_>>(), expected, "{source}");
        }
        let lambda = Expr::Lambda {
            params: vec!["p".into()],
            body: Box::new(Expr::Binary {
                op: crate::syntax::ast::BinaryOp::Add,
                left: Box::new(Expr::Name { name: "p".into() }),
                right: Box::new(Expr::Name { name: "q".into() }),
            }),
        };
        let mut deps = BTreeSet::new();
        dependencies(&lambda, &mut deps);
        assert_eq!(deps.into_iter().collect::<Vec<_>>(), vec!["q"]);
    }

    #[test]
    fn removing_a_draft_clears_its_dependency_edges() {
        let mut tracking = DirtyTracking::default();
        tracking.change(&statement("draft=other\n"));
        assert!(tracking.dependencies.contains_key("draft"));
        assert!(
            tracking
                .reverse
                .get("other")
                .is_some_and(|users| users.contains("draft"))
        );
        tracking.remove("draft".into());
        assert!(!tracking.dependencies.contains_key("draft"));
        assert!(
            tracking
                .reverse
                .get("other")
                .is_none_or(|users| !users.contains("draft"))
        );
        assert!(tracking.dirty.contains("draft"));
        assert_eq!(tracking.revisions.get("draft"), Some(&2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ir::Role, r#lib::ComponentDef, r#lib::GroupDef, r#lib::Library, r#lib::T};
    use std::sync::Arc;

    fn library() -> Arc<Library> {
        Arc::new(
            Library::builder("session-test", "0.1.0")
                .group(GroupDef::new("G", 0, "Test components."))
                .component(
                    ComponentDef::new("Text", Role::Paragraph)
                        .group("G")
                        .doc("Text.")
                        .req("text", T::text()),
                )
                .component(
                    ComponentDef::new("Stack", Role::Group)
                        .group("G")
                        .doc("Stack.")
                        .req("children", T::nodes()),
                )
                .component(
                    ComponentDef::new("Control", Role::Button)
                        .group("G")
                        .doc("Control.")
                        .opt_none("on_click", T::action()),
                )
                .component(
                    ComponentDef::new("Button", Role::Button)
                        .group("G")
                        .doc("Button.")
                        .req("label", T::text())
                        .opt_none("on_click", T::action()),
                )
                .build()
                .unwrap(),
        )
    }
    fn session(source: &str, tools: &[&str], refs: &[&str]) -> Session {
        let options = Options {
            tools: tools.iter().map(|s| (*s).into()).collect(),
            refs: refs.iter().map(|s| (*s).into()).collect(),
            ..Options::default()
        };
        let mut session = Session::new(SessionOptions {
            library: library(),
            validation: options,
            input: InputMode::Program,
        });
        session.push(source.as_bytes());
        session
    }
    fn text(session: &Session) -> Option<String> {
        session.tree().canonical_tree()["props"]["text"]
            .as_str()
            .map(str::to_string)
    }
    fn text_at(session: &Session, path: &str) -> Option<String> {
        fn walk(value: &serde_json::Value, path: &str) -> Option<String> {
            if value["id"].as_str() == Some(path) {
                return value["props"]["text"].as_str().map(str::to_string);
            }
            let children = value["children"].as_object()?;
            for slot in children.values() {
                for child in slot.as_array()? {
                    if let Some(text) = walk(child, path) {
                        return Some(text);
                    }
                }
            }
            None
        }
        walk(&session.tree().canonical_tree(), path)
    }

    #[test]
    fn queries_request_only_after_finish_and_delivery_is_one_shot() {
        let mut session = session(
            "query q=tool(\"known\",{},default:\"d\")\nroot=Text(q)",
            &["known"],
            &[],
        );
        let update = session.push(b"");
        assert!(update.requests.is_empty());
        assert_eq!(session.query_status("q"), QueryStatus::Idle);
        let update = session.finish();
        assert_eq!(update.requests.len(), 1);
        let id = update.requests[0].id;
        assert_eq!(session.query_status("q"), QueryStatus::Requested);
        let update = session.deliver(id, Ok(crate::value::Value::from("host value")));
        assert!(update.patches.is_some());
        assert_eq!(text(&session).as_deref(), Some("host value"));
        assert_eq!(session.query_status("q"), QueryStatus::Ready);
        let stale = session.deliver(id, Ok(crate::value::Value::from("again")));
        assert!(stale.patches.is_none());
        assert_eq!(text(&session).as_deref(), Some("host value"));
    }

    #[test]
    fn denied_query_keeps_its_default_and_reports_once() {
        let mut session = session(
            "query q=tool(\"known\",{},default:\"d\")\nroot=Text(q)",
            &["known"],
            &[],
        );
        let update = session.finish();
        let id = update.requests[0].id;
        let update = session.deliver(id, Err(HostError::Denied));
        assert_eq!(session.query_status("q"), QueryStatus::Denied);
        assert_eq!(text(&session).as_deref(), Some("d"));
        assert!(
            update
                .runtime_diagnostics
                .iter()
                .any(|d| d.code == Code::QueryDenied)
        );
    }

    #[test]
    fn refs_stay_placeholders_until_host_delivery() {
        let mut session = session("root=Text(ref(\"t4.places\"))", &[], &["t4.places"]);
        let update = session.finish();
        assert_eq!(update.requests.len(), 1);
        assert_eq!(
            text(&session),
            None,
            "a ref placeholder is not a text value"
        );
        let id = update.requests[0].id;
        session.deliver(id, Ok(crate::value::Value::from("Ramen")));
        assert_eq!(text(&session).as_deref(), Some("Ramen"));
    }

    #[test]
    fn denied_refresh_reverts_to_the_declared_default() {
        let mut session = session(
            "$n=1\nquery q=tool(\"known\",{x:$n},default:\"d\")\nctl=Control(on_click:set($n,2))\nroot=Stack([Text(q),ctl])",
            &["known"],
            &[],
        );
        let update = session.finish();
        let first = update.requests[0].id;
        session.deliver(first, Ok(crate::value::Value::from("good")));
        assert_eq!(
            text_at(&session, "root/children:#0").as_deref(),
            Some("good")
        );
        let path = crate::ir::NodePath("root/children:@ctl".into());
        let handle = session.action_handle(&path, "on_click").unwrap();
        let node = session
            .live_ir()
            .nodes
            .values()
            .find(|node| node.path == path)
            .unwrap()
            .id;
        session.activate(node, &handle);
        let update = session.advance_clock(300);
        let second = update.requests[0].id;
        session.deliver(second, Err(HostError::Denied));
        assert_eq!(text_at(&session, "root/children:#0").as_deref(), Some("d"));
        assert_eq!(session.query_status("q"), QueryStatus::Denied);
    }

    #[test]
    fn cross_epoch_delivery_is_dropped_although_ids_restart() {
        let source = "query q=tool(\"known\",{},default:\"d\")\nroot=Text(q)";
        let mut session = session(source, &["known"], &[]);
        let update = session.finish();
        let first = update.requests[0].id;
        assert_eq!(update.requests[0].epoch, 1);
        session.reset();
        session.push(source.as_bytes());
        let update = session.finish();
        let second = update.requests[0].id;
        assert_eq!(second, first, "ids restart after reset");
        assert_eq!(update.requests[0].epoch, 2);
        let stale = session.deliver_from(1, second, Ok(crate::value::Value::from("old")));
        assert!(stale.patches.is_none());
        assert_eq!(text(&session).as_deref(), Some("d"));
        session.deliver_from(2, second, Ok(crate::value::Value::from("new")));
        assert_eq!(text(&session).as_deref(), Some("new"));
    }

    #[test]
    fn oversized_events_report_once_even_without_text_room() {
        for (label, limit) in [
            (
                "Export the complete quarterly report to the finance archive",
                60,
            ),
            ("Go", 0),
        ] {
            let options = Options {
                limits: crate::limits::Limits {
                    max_event_bytes: limit,
                    ..crate::limits::Limits::default()
                },
                ..Options::default()
            };
            let mut session = Session::new(SessionOptions {
                library: library(),
                validation: options,
                input: InputMode::Program,
            });
            session.push(
                format!("btn=Button(\"{label}\",on_click:say(\"Recommend\"))\nroot=Stack([btn])")
                    .as_bytes(),
            );
            session.finish();
            let path = crate::ir::NodePath("root/children:@btn".into());
            let handle = session.action_handle(&path, "on_click").unwrap();
            let node = session
                .live_ir()
                .nodes
                .values()
                .find(|node| node.path == path)
                .unwrap()
                .id;
            let update = session.activate(node, &handle);
            assert_eq!(update.model_events.len(), 1);
            assert_eq!(
                update
                    .runtime_diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.code == Code::EventSize)
                    .count(),
                1,
                "label {label:?} at limit {limit}"
            );
        }
    }

    #[test]
    fn repeated_independent_host_diagnostics_are_kept() {
        let mut session = session(
            "query q=tool(\"known\",{},default:\"d\")\nctl=Control(on_click:refresh(q))\nroot=Stack([Text(q),ctl])",
            &["known"],
            &[],
        );
        let update = session.finish();
        session.deliver(update.requests[0].id, Err(HostError::Denied));
        let path = crate::ir::NodePath("root/children:@ctl".into());
        let handle = session.action_handle(&path, "on_click").unwrap();
        let node = session
            .live_ir()
            .nodes
            .values()
            .find(|node| node.path == path)
            .unwrap()
            .id;
        let update = session.activate(node, &handle);
        session.deliver(update.requests[0].id, Err(HostError::Denied));
        assert_eq!(
            session
                .runtime_diagnostics()
                .iter()
                .filter(|diagnostic| diagnostic.code == Code::QueryDenied)
                .count(),
            2
        );
    }

    #[test]
    fn editing_is_ignored_before_the_session_is_live() {
        let mut session = session(
            "$n=1\nroot=Stack([Text(text($n)),Control(on_click:set($n,2))])",
            &[],
            &[],
        );
        session.push(b"");
        let node = session
            .ir()
            .nodes
            .values()
            .find(|n| n.path.0 == "root/children:#1")
            .map(|n| n.id)
            .unwrap();
        let handle =
            session.action_handle(&crate::ir::NodePath("root/children:#1".into()), "on_click");
        assert!(handle.is_none());
        let update = session.activate(
            node,
            &ActionHandle {
                plan: crate::ir::ActionPlanId(Arc::from("nope")),
                epoch: 1,
            },
        );
        assert!(update.patches.is_none());
        session.finish();
        let tree = session.tree().canonical_tree();
        assert_eq!(
            tree["children"]["children"][0]["props"]["text"]
                .as_str()
                .map(str::to_string)
                .as_deref(),
            Some("1")
        );
    }
}
