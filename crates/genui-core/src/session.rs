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
    pub fn tree(&self) -> &Tree {
        &self.tree
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
        let previous_ir = self.current.ir.clone();
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
        let ops = if snapshot || !self.started {
            let (tree, work) = Tree::from_ir_measured(&self.current.ir, &self.options.library);
            self.work += work;
            let ops = tree.snapshot_ops();
            self.tree = tree;
            self.tree_dirty = false;
            ops
        } else if let Some(projection) = projection {
            let (ops, work) =
                self.tree
                    .maintain(&self.current.ir, &self.options.library, &projection);
            self.work += work;
            self.tree_dirty = false;
            ops
        } else if let Some(mut grafts) = grafts {
            grafts.sort_by_key(|graft| graft.order.clone());
            let (mut ops, work) =
                self.tree
                    .insert_grafts(&self.current.ir, &self.options.library, &grafts);
            self.work += work;
            if let Some(mut ids) = local {
                ids.sort_by_key(|id| {
                    self.current_cache
                        .as_ref()
                        .and_then(|cache| cache.node_order(id))
                        .cloned()
                });
                let (changes, work) =
                    self.tree
                        .update_props(&self.current.ir, &self.options.library, &ids);
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
            let (ops, work) = self
                .tree
                .update_props(&self.current.ir, &self.options.library, &ids);
            self.work += work;
            self.tree_dirty = false;
            ops
        } else if self.tree_dirty || snapshot || !self.started {
            let (next, work) = Tree::from_ir_measured(&self.current.ir, &self.options.library);
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
mod tracking_tests {
    use super::*;
    fn statement(source: &str) -> Statement {
        let mut parsed =
            crate::syntax::parse_batch(source.as_bytes(), crate::limits::SyntaxLimits::default());
        parsed.program.statements.pop().expect("one statement")
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
