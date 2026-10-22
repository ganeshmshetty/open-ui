//! Streaming orchestration (§3): no evaluator, requests, deliveries or action execution.
use crate::{
    diag::{Code, Diagnostic, Severity},
    ir::{ActionPlanId, Ir, NodeStatus, Prop, PropValue},
    r#lib::Library,
    patch::{PatchBatch, Tree, strip_invisibles},
    syntax::{
        ast::{Expr, Statement, StatementKind},
        extract::{Extractor, Segment},
        streaming::{Seal, StreamingParser},
    },
    validate::{Options, Validation, validate_program},
};
use serde::{Deserialize, Serialize};
use std::{
    cell::Cell,
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
/// Transport slots reserved for M4. M3 never creates either kind of host effect.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum HostRequest {
    Query { name: String },
    Ref { id: String },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelEvent {
    pub name: String,
    pub payload: serde_json::Value,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Update {
    pub patches: Option<PatchBatch>,
    pub diagnostics: Vec<Diagnostic>,
    pub requests: Vec<HostRequest>,
    pub model_events: Vec<ModelEvent>,
    pub prose: Vec<ProseDelta>,
    pub phase: SessionPhase,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Revision {
    pub origin_rev: u64,
    pub dep_revs: BTreeMap<String, u64>,
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
        let mut pending = vec![name];
        while let Some(n) = pending.pop() {
            if self.dirty.insert(n.clone()) {
                pending.extend(self.reverse.get(&n).into_iter().flatten().cloned());
            }
        }
    }
    fn instance(&self, origin: &str) -> Revision {
        let mut deps = BTreeMap::new();
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
pub struct Session {
    options: SessionOptions,
    parser: StreamingParser,
    extractor: Option<Extractor>,
    program_bytes: usize,
    program_raw: Vec<u8>,
    prose_sent: BTreeMap<u32, usize>,
    segments: Vec<Segment>,
    extraction_diagnostics: Vec<Diagnostic>,
    rejected: bool,
    sealed_count: usize,
    sealed_validation: Validation,
    last_candidate: Option<Statement>,
    current: Validation,
    tree: Tree,
    tracking: DirtyTracking,
    epoch: u32,
    next_seq: u64,
    started: bool,
    finished: bool,
    not_sync: Cell<()>,
    published: Vec<Diagnostic>,
    tree_dirty: bool,
    reused_instances: usize,
    provisional_origins: BTreeSet<String>,
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
            options,
            parser,
            extractor,
            program_bytes: 0,
            program_raw: Vec::new(),
            prose_sent: BTreeMap::new(),
            segments: Vec::new(),
            extraction_diagnostics: Vec::new(),
            rejected: false,
            sealed_count: 0,
            sealed_validation: Validation {
                ir: Ir::default(),
                diagnostics: Vec::new(),
            },
            last_candidate: None,
            current: Validation {
                ir: Ir::default(),
                diagnostics: Vec::new(),
            },
            tree: Tree::default(),
            tracking: DirtyTracking::default(),
            epoch: 1,
            next_seq: 0,
            started: false,
            finished: false,
            not_sync: Cell::new(()),
            published: Vec::new(),
            tree_dirty: true,
            reused_instances: 0,
            provisional_origins: BTreeSet::new(),
        }
    }
    pub fn push(&mut self, chunk: &[u8]) -> Update {
        if self.finished {
            return self.empty_update();
        }
        self.tracking.dirty.clear();
        if let Some(extractor) = &mut self.extractor {
            extractor.push(chunk);
            self.extraction_diagnostics = extractor.diagnostics().to_vec();
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
            let extracted = extractor.finish();
            self.program_raw = extracted.program_raw;
            self.segments = extracted.segments;
            self.extraction_diagnostics = extracted.diagnostics;
            self.rejected = extracted.rejected;
            self.feed_program();
        }
        self.parser.finish();
        self.finished = true;
        self.rebuild(true);
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
    pub fn ir(&self) -> &Ir {
        &self.current.ir
    }
    pub fn tree(&self) -> &Tree {
        &self.tree
    }
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.current.diagnostics
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
            || self
                .current
                .diagnostics
                .iter()
                .any(|d| d.severity == Severity::Fatal && !d.provisional)
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
    fn empty_update(&self) -> Update {
        Update {
            patches: None,
            diagnostics: Vec::new(),
            requests: Vec::new(),
            model_events: Vec::new(),
            prose: Vec::new(),
            phase: self.phase(),
        }
    }
    fn rebuild(&mut self, finish: bool) {
        let new_seals = self.parser.seals().len() != self.sealed_count;
        for seal in &self.parser.seals()[self.sealed_count..] {
            if let Some(s) = &seal.statement {
                self.tracking.change(s);
            }
        }
        self.sealed_count = self.parser.seals().len();
        let syntax = self.parser.diagnostics();
        if new_seals || finish || !self.started {
            self.sealed_validation = if finish {
                validate_program(
                    self.parser.program(),
                    Vec::new(),
                    &self.options.library,
                    &self.options.validation,
                )
            } else {
                let (checked, reused) = crate::validate::validate_incremental(
                    self.parser.program(),
                    &self.options.library,
                    &self.options.validation,
                    &self.current,
                    &self.tracking.dirty,
                );
                self.reused_instances += reused;
                checked
            };
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
            if let Some(s) = &candidate {
                self.tracking.change(s);
            } else if let Some(s) = &self.last_candidate {
                self.tracking.dirty.insert(key(s));
            }
        }
        if new_seals || candidate_changed || finish || !self.started {
            self.tree_dirty = true;
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
            let mut value = if let Some(s) = &candidate {
                let mut program = self.parser.program().clone();
                program.statements.push(s.clone());
                let (checked, reused) = crate::validate::validate_incremental(
                    &program,
                    &self.options.library,
                    &self.options.validation,
                    &self.current,
                    &self.tracking.dirty,
                );
                self.reused_instances += reused;
                checked
            } else {
                self.sealed_validation.clone()
            };
            if !finish {
                // No state declaration or type inference is committed by an unsealed binding.
                value.ir.states = self.sealed_validation.ir.states.clone();
                value.ir.queries = self.sealed_validation.ir.queries.clone();
                if let Some(s) = &candidate {
                    for n in value.ir.nodes.values_mut() {
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
                                Some(id.0)
                            } else {
                                None
                            }
                        })
                        .collect();
                    let mut remap = BTreeMap::new();
                    let mut actions = Vec::new();
                    for (i, a) in std::mem::take(&mut value.ir.actions)
                        .into_iter()
                        .enumerate()
                    {
                        if active_actions.contains(&i)
                            && value
                                .ir
                                .nodes
                                .get(&a.origin)
                                .is_some_and(|n| n.status == NodeStatus::Sealed)
                        {
                            remap.insert(i, actions.len());
                            actions.push(a);
                        }
                    }
                    value.ir.actions = actions;
                    for n in value.ir.nodes.values_mut() {
                        n.props.retain(|_, p| {
                            if let Prop::Static(PropValue::Action(id)) = p {
                                if let Some(new) = remap.get(&id.0) {
                                    *id = ActionPlanId(*new);
                                    true
                                } else {
                                    false
                                }
                            } else {
                                true
                            }
                        });
                    }
                }
                for d in &mut value.diagnostics {
                    d.provisional = true;
                }
            }
            if self.rejected {
                value.ir = Ir::default();
                value.diagnostics.clear();
            }
            self.current = value;
            // Keep exact equal instances, including their structural IDs, and record revisions.
            let old = std::mem::take(&mut self.tracking.instances);
            for n in self.current.ir.nodes.values() {
                let rev = self.tracking.instance(&n.origin);
                let rev = old
                    .get(&n.path.0)
                    .filter(|old| **old == rev)
                    .cloned()
                    .unwrap_or(rev);
                self.tracking.instances.insert(n.path.0.clone(), rev);
            }
        }
        self.last_candidate = candidate;
        // Syntax/extraction diagnostics are rebuilt from authoritative scanner state. Their
        // provisional entries may disappear after a size cutoff; they were never committed.
        self.current.diagnostics.retain(|d| {
            d.span.is_none()
                && !matches!(
                    d.code,
                    Code::BadUtf8 | Code::ProgramSize | Code::Version | Code::ExtraProgram
                )
        });
        if !self.rejected {
            self.current
                .diagnostics
                .extend(syntax.into_iter().filter(|d| {
                    d.code != Code::BadUtf8
                        || !self
                            .extraction_diagnostics
                            .iter()
                            .any(|d| d.code == Code::BadUtf8)
                }));
        }
        self.current
            .diagnostics
            .extend(self.extraction_diagnostics.clone());
        if finish {
            for d in &mut self.current.diagnostics {
                d.provisional = false;
            }
        }
        if self
            .current
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Fatal && !d.provisional && d.code == Code::Version)
        {
            self.rejected = true;
            self.current.ir = Ir::default();
        }
    }
    fn update(&mut self, emit_empty: bool, snapshot: bool) -> Update {
        let ops = if self.tree_dirty || snapshot || !self.started {
            let next = Tree::from_ir(&self.current.ir, &self.options.library);
            let ops = if snapshot || !self.started {
                next.snapshot_ops()
            } else {
                self.tree.diff(&next)
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
        let mut previous = self.published.clone();
        let mut diagnostics = Vec::new();
        for d in &self.current.diagnostics {
            if let Some(i) = previous.iter().position(|old| old == d) {
                previous.remove(i);
            } else {
                diagnostics.push(d.clone());
            }
        }
        self.published = self.current.diagnostics.clone();
        Update {
            patches,
            diagnostics,
            requests: Vec::new(),
            model_events: Vec::new(),
            prose,
            phase: self.phase(),
        }
    }
}
fn key(s: &Statement) -> String {
    if s.kind == StatementKind::State {
        format!("${}", s.name)
    } else {
        s.name.clone()
    }
}
fn dependencies(e: &Expr, out: &mut BTreeSet<String>) {
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
