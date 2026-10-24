//! `UiView`: the retained GPUI tree, batch application and rendering (SPEC §10.1).
//!
//! `apply` is the streaming path: it mutates the retained tree with `&mut
//! Window` available and notifies only the affected entities. `enqueue` is the
//! fallback; the queue is drained at the start of the next `render`, where some
//! notifications can land one frame later (measured in the GPUI spike).

use crate::{
    capabilities::Capabilities,
    chart::{self, ChartModel, ChartView},
    components,
    controls::{self, FieldDisplay},
    data::{self, TabsView},
    extensions::{ExtensionNode, ExtensionRegistry},
    image::{ImageCache, ImageError, ImageLimits, ImageState},
    layout::{
        StateOverrides, VisualState, Widths, data_state, inset_width, row_cell_width, state_of,
    },
    metrics::{Metrics, PatchSummary},
    panics::{PanicTracker, guard},
    props::{self, Props},
    table::{self, GenuiTableDelegate, TableModel},
    theme::{Radius, TextRole, UiTheme},
    tree::{ApplyReport, NodeEntry, RetainedTree},
};
use genui_core::{
    diag::Diagnostic,
    ir::{ActionPlanId, NodePath},
    patch::{Op, PatchBatch},
    runtime::{StateOrigin, StateValue},
    session::{ActionHandle, UiEvent},
    value::{PStr, Provenance, Value},
};
use gpui_component::FocusableExt as _;
use gpui_component::{
    input::{Input, InputContentType, InputEvent, InputState, Textarea, TextareaState},
    searchable_list::SearchableVec,
    select::{Select, SelectEvent, SelectState},
};
use gpui_kit::{
    AnyElement, AnyView, App, AppContext, Context, ElementId, Entity, EventEmitter, FocusHandle,
    Focusable, ImageFormat, InteractiveElement, IntoElement, ParentElement, Pixels, Render,
    SharedString, StatefulInteractiveElement, Styled, Subscription, TestSupportExt as _,
    WeakEntity, Window, component::table::TableState, div, px,
};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    sync::Arc,
    time::Instant,
};

/// Renderer-visible `$state` values, shared with every input entity (§10.4).
type StateMap = Rc<RefCell<BTreeMap<String, StateValue>>>;

/// Focus handles owned by nodes, used to name the field that gained focus and
/// to emit `Blur` when Tab, a pointer or Escape moves it (§10.4).
type FocusIndex = Rc<RefCell<Vec<(FocusHandle, String)>>>;

type TableEntity = Entity<gpui_kit::component::table::TableState<GenuiTableDelegate>>;
type TableMap = Rc<RefCell<BTreeMap<String, TableEntity>>>;
type TableModelCache = Rc<RefCell<BTreeMap<String, (u64, u64, u64, TableModel)>>>;
type TabsMap = Rc<RefCell<BTreeMap<String, Entity<TabsView>>>>;
type ListMap = Rc<RefCell<BTreeMap<String, Entity<crate::data::ListView>>>>;

fn register_focus(index: &FocusIndex, handle: &FocusHandle, id: &str) {
    let mut index = index.borrow_mut();
    if !index.iter().any(|(_, path)| path == id) {
        index.push((handle.clone(), id.to_string()));
    }
}

fn lookup_focus(index: &FocusIndex, handle: &FocusHandle) -> Option<String> {
    index
        .borrow()
        .iter()
        .find(|(candidate, _)| candidate == handle)
        .map(|(_, path)| path.clone())
}

/// Make `id` the focused field, emitting a blur for the field it left.
fn focus_node(focused: &Rc<RefCell<Option<String>>>, sink: &EventSink, id: &str, cx: &mut App) {
    let previous = focused.borrow().clone();
    if previous.as_deref() == Some(id) {
        return;
    }
    if let Some(previous) = previous {
        sink.blur(&previous, cx);
    }
    *focused.borrow_mut() = Some(id.to_string());
}

fn clear_focus(focused: &Rc<RefCell<Option<String>>>, sink: &EventSink, cx: &mut App) {
    if let Some(previous) = focused.borrow_mut().take() {
        sink.blur(&previous, cx);
    }
}

/// Forwards a renderer event to `UiView`'s `EventEmitter` and applies the
/// epoch/frozen gate before the host sees anything. Built by `UiView`, cloned by
/// child closures.
#[derive(Clone)]
pub struct EventSink {
    view: WeakEntity<UiView>,
    epoch: Rc<Cell<u32>>,
    frozen: Rc<Cell<bool>>,
}

impl EventSink {
    fn emit(&self, event: UiEvent, cx: &mut App) {
        if self.frozen.get() {
            return;
        }
        let _ = self.view.update(cx, |_, cx| cx.emit(event));
    }

    fn activate(&self, node: &str, plan: &str, cx: &mut App) {
        self.emit(
            UiEvent::Activate {
                node: NodePath(node.to_string()),
                action: ActionHandle {
                    plan: ActionPlanId(Arc::from(plan)),
                    epoch: self.epoch.get(),
                },
            },
            cx,
        );
    }

    pub(crate) fn edit(&self, node: &str, value: Value, cx: &mut App) {
        self.emit(
            UiEvent::Edit {
                node: NodePath(node.to_string()),
                value,
            },
            cx,
        );
    }

    fn blur(&self, node: &str, cx: &mut App) {
        self.emit(
            UiEvent::Blur {
                node: NodePath(node.to_string()),
            },
            cx,
        );
    }

    fn frozen(&self) -> bool {
        self.frozen.get()
    }
}

/// One text widget. `InputState` and `TextareaState` are separate pinned types.
pub enum TextWidget {
    Input(Entity<InputState>),
    Area(Entity<TextareaState>),
}

impl TextWidget {
    fn value(&self, cx: &App) -> String {
        match self {
            Self::Input(state) => state.read(cx).value().to_string(),
            Self::Area(state) => state.read(cx).value().to_string(),
        }
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self {
            Self::Input(state) => state.read(cx).focus_handle(cx),
            Self::Area(state) => state.read(cx).focus_handle(cx),
        }
    }

    fn set_value(&self, text: &str, window: &mut Window, cx: &mut App) {
        match self {
            Self::Input(state) => state.update(cx, |state, cx| state.set_value(text, window, cx)),
            Self::Area(state) => state.update(cx, |state, cx| state.set_value(text, window, cx)),
        }
    }

    fn set_disabled(&self, disabled: bool, cx: &mut App) {
        match self {
            Self::Input(state) => {
                state.update(cx, |state, cx| state.set_disabled(disabled, cx));
            }
            Self::Area(state) => {
                state.update(cx, |state, cx| state.set_disabled(disabled, cx));
            }
        }
    }
}

pub struct SelectWidget {
    state: Entity<SelectState<SearchableVec<String>>>,
    focus: FocusHandle,
    values: Vec<String>,
}

/// Per-input widget state, created lazily with a `Window` on first render.
#[derive(Default)]
pub struct ControlState {
    text: Option<TextWidget>,
    select: Option<SelectWidget>,
}

pub struct UiView {
    theme: UiTheme,
    tree: Rc<RefCell<RetainedTree>>,
    containers: Rc<RefCell<BTreeMap<String, Entity<NodeView>>>>,
    charts: Rc<RefCell<BTreeMap<String, Entity<ChartView>>>>,
    images: Rc<RefCell<BTreeMap<String, Entity<ImageView>>>>,
    widths: Widths,
    overrides: Rc<StateOverrides>,
    tables: TableMap,
    table_models: TableModelCache,
    tabs: TabsMap,
    lists: ListMap,
    image_cache: Rc<RefCell<ImageCache>>,
    panics: Rc<RefCell<PanicTracker>>,
    fades: Rc<RefCell<BTreeMap<String, u64>>>,
    metrics: Rc<RefCell<Metrics>>,
    queue: VecDeque<PatchBatch>,
    diagnostics: Vec<Diagnostic>,
    needs_snapshot: bool,
    max_width: Option<Pixels>,
    panic_injection: bool,
    state: StateMap,
    epoch: Rc<Cell<u32>>,
    frozen: Rc<Cell<bool>>,
    focused: Rc<RefCell<Option<String>>>,
    focus_index: FocusIndex,
    extensions: ExtensionRegistry,
    root_focus: FocusHandle,
}

impl UiView {
    /// SPEC §10.1. Input widget state is created on the first apply/render that
    /// has a `Window` available.
    pub fn new(theme: UiTheme, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        install_key_bindings(cx);
        let root_focus = cx.focus_handle();
        Self {
            theme,
            tree: Rc::new(RefCell::new(RetainedTree::default())),
            containers: Rc::new(RefCell::new(BTreeMap::new())),
            charts: Rc::new(RefCell::new(BTreeMap::new())),
            images: Rc::new(RefCell::new(BTreeMap::new())),
            widths: Widths::default(),
            overrides: Rc::new(StateOverrides::default()),
            tables: Rc::new(RefCell::new(BTreeMap::new())),
            table_models: Rc::new(RefCell::new(BTreeMap::new())),
            tabs: Rc::new(RefCell::new(BTreeMap::new())),
            lists: Rc::new(RefCell::new(BTreeMap::new())),
            image_cache: Rc::new(RefCell::new(ImageCache::default())),
            panics: Rc::new(RefCell::new(PanicTracker::default())),
            fades: Rc::new(RefCell::new(BTreeMap::new())),
            metrics: Rc::new(RefCell::new(Metrics::default())),
            queue: VecDeque::new(),
            diagnostics: Vec::new(),
            needs_snapshot: false,
            max_width: None,
            panic_injection: false,
            state: Rc::new(RefCell::new(BTreeMap::new())),
            epoch: Rc::new(Cell::new(0)),
            frozen: Rc::new(Cell::new(false)),
            focused: Rc::new(RefCell::new(None)),
            focus_index: Rc::new(RefCell::new(Vec::new())),
            extensions: ExtensionRegistry::default(),
            root_focus,
        }
    }

    /// Focus the view's tab group. Hosts call this when a message receives
    /// keyboard focus; pressing Tab then enters the first control.
    pub fn focus_root(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.root_focus.focus(window, cx);
    }

    /// Push the session's current `$state` into the view (§10.4). Hosts call
    /// this after applying the patches from a `Session` update.
    pub fn sync_state(
        &mut self,
        values: BTreeMap<String, StateValue>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        *self.state.borrow_mut() = values;
        let ids: Vec<String> = self
            .tree
            .borrow()
            .nodes
            .values()
            .filter(|node| props::binding(&node.props, "bind").is_some())
            .map(|node| node.id.clone())
            .collect();
        for id in ids {
            if let Some(entity) = self.containers.borrow().get(&id).cloned() {
                entity.update(cx, |view, cx| {
                    view.sync_widget(window, cx);
                    cx.notify();
                });
            }
        }
        cx.notify();
    }

    /// The session's state values this view currently renders.
    pub fn state_values(&self) -> BTreeMap<String, StateValue> {
        self.state.borrow().clone()
    }

    /// SPEC §7.9 `Frozen`: the view keeps its tree, but inputs and buttons are
    /// disabled and emit no events.
    pub fn freeze(&mut self, cx: &mut Context<Self>) {
        self.frozen.set(true);
        self.notify_controls(cx);
        cx.notify();
    }

    pub fn thaw(&mut self, cx: &mut Context<Self>) {
        self.frozen.set(false);
        self.notify_controls(cx);
        cx.notify();
    }

    fn notify_controls(&self, cx: &mut Context<Self>) {
        for entity in self.containers.borrow().values() {
            entity.update(cx, |_, cx| cx.notify());
        }
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen.get()
    }

    fn sink(&self, cx: &mut Context<Self>) -> EventSink {
        EventSink {
            view: cx.entity().downgrade(),
            epoch: self.epoch.clone(),
            frozen: self.frozen.clone(),
        }
    }

    pub fn with_overrides(mut self, overrides: StateOverrides) -> Self {
        self.overrides = Rc::new(overrides);
        self
    }

    pub fn with_diagnostics(mut self, diagnostics: Vec<Diagnostic>) -> Self {
        self.diagnostics = diagnostics;
        self
    }

    /// Register a host builder for a custom component kind (§10.14). Register
    /// before the program's first batch so inserted nodes find the builder.
    pub fn with_extension(
        self,
        kind: impl Into<String>,
        builder: impl Fn(&ExtensionNode, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.extensions.register(kind, builder);
        self
    }

    /// Register an extension on a live view.
    pub fn register_extension(
        &mut self,
        kind: impl Into<String>,
        builder: impl Fn(&ExtensionNode, &mut Window, &mut App) -> AnyElement + 'static,
        cx: &mut Context<Self>,
    ) {
        self.extensions.register(kind, builder);
        cx.notify();
    }

    pub fn extensions(&self) -> &ExtensionRegistry {
        &self.extensions
    }

    pub fn capabilities() -> Capabilities {
        Capabilities::m8()
    }

    /// `true` when the build uses unwinding and builder panics can be
    /// contained (SPEC §10.8).
    pub const fn panic_containment_enabled() -> bool {
        crate::panics::CONTAINMENT_ENABLED
    }

    pub fn theme(&self) -> &UiTheme {
        &self.theme
    }

    pub fn tree(&self) -> std::cell::Ref<'_, RetainedTree> {
        self.tree.borrow()
    }

    pub fn metrics(&self) -> crate::metrics::Summary {
        self.metrics.borrow().summary()
    }

    pub fn metrics_handle(&self) -> Rc<RefCell<Metrics>> {
        self.metrics.clone()
    }

    pub fn tree_handle(&self) -> Rc<RefCell<RetainedTree>> {
        self.tree.clone()
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// The current widget text or selected option for an input node. This is
    /// the value the user sees, which can differ from `$state` while focused.
    pub fn field_value(&self, path: &str, cx: &App) -> Option<String> {
        self.containers
            .borrow()
            .get(path)?
            .read_with(cx, |view, cx| {
                view.control
                    .text
                    .as_ref()
                    .map(|text| text.value(cx))
                    .or_else(|| {
                        view.control
                            .select
                            .as_ref()
                            .and_then(|select| select.state.read(cx).selected_value().cloned())
                    })
            })
    }

    /// The focus handle owned by a focusable node, when it exists. Hosts use
    /// this for focus assertions; the renderer owns creation and lifetime.
    pub fn focus_handle_for(&self, path: &str, cx: &App) -> Option<FocusHandle> {
        self.containers
            .borrow()
            .get(path)?
            .read_with(cx, |view, cx| view.focus_handle(cx))
    }

    pub fn set_diagnostics(&mut self, diagnostics: Vec<Diagnostic>, cx: &mut Context<Self>) {
        self.diagnostics = diagnostics;
        cx.notify();
    }

    /// Caps program content width when the host reserves columns beside it
    /// (for example the developer inspector).
    pub fn set_max_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        self.max_width = Some(width);
        cx.notify();
    }

    /// Host image contract (SPEC §10.3, ROADMAP item 7). `request_image`
    /// records the pending state; the host fetches outside the renderer and
    /// calls `deliver_image` or `fail_image`.
    pub fn request_image(&mut self, src: &str, cx: &mut Context<Self>) {
        self.image_cache.borrow_mut().request(src);
        cx.notify();
    }

    pub fn deliver_image(
        &mut self,
        src: &str,
        format: ImageFormat,
        bytes: Vec<u8>,
        cx: &mut Context<Self>,
    ) -> Result<(), ImageError> {
        let result = self.image_cache.borrow_mut().deliver(src, format, bytes);
        cx.notify();
        result
    }

    pub fn fail_image(&mut self, src: &str, cx: &mut Context<Self>) {
        self.image_cache.borrow_mut().fail(src);
        cx.notify();
    }

    pub fn image_state(&self, src: &str) -> ImageState {
        self.image_cache.borrow().state(src)
    }

    pub fn with_image_limits(mut self, limits: ImageLimits) -> Self {
        self.image_cache = Rc::new(RefCell::new(ImageCache::with_limits(limits)));
        self
    }

    /// Test-only panic injection. A node whose kind is `PanicProbe` panics in
    /// its builder so the containment path can be exercised end to end.
    pub fn with_panic_injection(mut self) -> Self {
        self.panic_injection = true;
        self
    }

    pub fn render_diagnostics(&self) -> Vec<Diagnostic> {
        self.panics.borrow().diagnostics().to_vec()
    }

    pub fn panic_count(&self) -> u64 {
        self.panics.borrow().panics()
    }

    pub fn is_quarantined(&self, id: &str) -> bool {
        let rev = self.tree.borrow().get(id).map_or(0, |entry| entry.rev);
        self.panics.borrow().is_quarantined(id, rev)
    }

    /// Scrolls a table to a row. Used by the 50,000-row stress demo and tests.
    pub fn scroll_table(&mut self, id: &str, row: usize, cx: &mut Context<Self>) -> bool {
        let Some(entity) = self.tables.borrow().get(id).cloned() else {
            return false;
        };
        entity.update(cx, |table, cx| table.scroll_to_row(row, cx));
        true
    }

    pub fn table_row_count(&self, id: &str, cx: &App) -> Option<usize> {
        let entity = self.tables.borrow().get(id).cloned()?;
        Some(entity.read(cx).delegate().rows())
    }

    pub fn table_visible_range(&self, id: &str, cx: &App) -> Option<(usize, usize)> {
        let entity = self.tables.borrow().get(id).cloned()?;
        let state = entity.read(cx);
        let range = state.visible_range().rows();
        Some((range.start, range.end))
    }

    /// Scroll a virtualized List to a row (SPEC §10.3).
    pub fn scroll_list(
        &mut self,
        id: &str,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(entity) = self.lists.borrow().get(id).cloned() else {
            return false;
        };
        entity.update(cx, |list, cx| list.scroll_to_row(row, window, cx));
        true
    }

    pub fn list_created_rows(&self, id: &str, cx: &App) -> Option<usize> {
        self.lists
            .borrow()
            .get(id)
            .map(|list| list.read(cx).created.get())
    }

    pub fn list_frame_rows(&self, id: &str, cx: &App) -> Option<usize> {
        self.lists
            .borrow()
            .get(id)
            .map(|list| list.read(cx).frame_rendered.get())
    }

    /// True once a sequence gap or bad op asked the host for a snapshot.
    pub fn take_needs_snapshot(&mut self) -> bool {
        std::mem::take(&mut self.needs_snapshot)
    }

    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// Applies a batch now, before the next frame, then notifies affected views.
    pub fn apply(&mut self, batch: PatchBatch, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_batch(batch, window, cx);
    }

    /// Fallback for callers without a window. Drained once at the next render.
    pub fn enqueue(&mut self, batch: PatchBatch, cx: &mut Context<Self>) {
        self.metrics.borrow_mut().batches_enqueued += 1;
        self.queue.push_back(batch);
        cx.notify();
    }

    fn apply_batch(&mut self, batch: PatchBatch, window: &mut Window, cx: &mut Context<Self>) {
        let started = Instant::now();
        // Keep only the lightweight ids/parents needed by `sync_views`. A full
        // tree clone would copy a 50,000-row table prop on every batch
        // (SPEC §10.6). `before` is kept for focus recovery only when it is
        // actually needed.
        let mut parents: BTreeMap<String, Option<String>> = BTreeMap::new();
        let mut removed: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut needs_before = false;
        {
            let tree = self.tree.borrow();
            for op in &batch.ops {
                match op {
                    Op::Remove { id } => {
                        let mut ids = Vec::new();
                        collect_ids(&tree, id, &mut ids);
                        removed.insert(id.clone(), ids);
                        needs_before |= self.focused.borrow().is_some();
                    }
                    Op::Move { id, .. } => {
                        parents.insert(
                            id.clone(),
                            tree.get(id).and_then(|entry| entry.parent.clone()),
                        );
                    }
                    _ => {}
                }
            }
        }
        let before = (needs_before).then(|| self.tree.borrow().clone());
        let report = {
            let mut tree = self.tree.borrow_mut();
            tree.apply(&batch)
        };
        {
            let mut metrics = self.metrics.borrow_mut();
            metrics.record_patch(PatchSummary {
                epoch: batch.epoch,
                seq: batch.seq,
                ops: batch.ops.len(),
                applied: report.applied,
                discarded: report.discarded,
                needs_snapshot: report.needs_snapshot,
                errors: report.errors.len(),
            });
            if report.applied {
                metrics.batches_applied += 1;
                metrics.ops_applied += batch.ops.len() as u64;
                metrics.nodes_inserted += batch
                    .ops
                    .iter()
                    .filter(|op| matches!(op, Op::Insert { .. }))
                    .count() as u64;
                metrics.nodes_removed += batch
                    .ops
                    .iter()
                    .filter(|op| matches!(op, Op::Remove { .. }))
                    .count() as u64;
            }
            if report.needs_snapshot {
                metrics.snapshot_requests += 1;
            }
            metrics.record_apply(started.elapsed());
        }
        if report.needs_snapshot {
            self.needs_snapshot = true;
        }
        if report.discarded {
            return;
        }
        if report.applied {
            self.epoch.set(batch.epoch);
        }
        self.sync_views(
            &batch,
            &parents,
            &removed,
            before.as_ref(),
            &report,
            window,
            cx,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn sync_views(
        &mut self,
        batch: &PatchBatch,
        parents: &BTreeMap<String, Option<String>>,
        removed: &BTreeMap<String, Vec<String>>,
        before: Option<&RetainedTree>,
        report: &ApplyReport,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for op in &batch.ops {
            match op {
                Op::Reset => {
                    self.containers.borrow_mut().clear();
                    self.charts.borrow_mut().clear();
                    self.images.borrow_mut().clear();
                    self.tables.borrow_mut().clear();
                    self.table_models.borrow_mut().clear();
                    self.tabs.borrow_mut().clear();
                    self.lists.borrow_mut().clear();
                    self.widths.clear();
                    self.fades.borrow_mut().clear();
                    self.panics.borrow_mut().clear();
                    self.focused.borrow_mut().take();
                    window.blur(cx);
                    cx.notify();
                }
                Op::Insert {
                    node, parent, slot, ..
                } => {
                    let _ = slot;
                    self.fades
                        .borrow_mut()
                        .entry(node.id.clone())
                        .and_modify(|generation| *generation = generation.saturating_add(1))
                        .or_insert(1);
                    self.ensure_entity(&node.id, window, cx);
                    if let Some(parent) = parent {
                        self.notify_for(parent, cx);
                    } else {
                        self.notify_root(cx);
                    }
                }
                Op::SetProps { id, .. } | Op::AppendText { id, .. } | Op::SetStatus { id, .. } => {
                    self.ensure_entity(id, window, cx);
                    self.notify_for(id, cx);
                }
                Op::Remove { id } => {
                    let parent = parents.get(id).cloned().flatten();
                    if let Some(before) = before {
                        self.recover_focus(id, before, window, cx);
                    }
                    self.prune_entities(id, removed);
                    self.fades.borrow_mut().remove(id);
                    if let Some(parent) = parent {
                        self.notify_for(&parent, cx);
                    } else {
                        cx.notify();
                    }
                }
                Op::Move { id, parent, .. } => {
                    let old_parent = parents.get(id).cloned().flatten();
                    if let Some(old) = old_parent {
                        self.notify_for(&old, cx);
                    }
                    if let Some(parent) = parent {
                        self.notify_for(parent, cx);
                    } else {
                        self.notify_root(cx);
                    }
                }
            }
        }
        // A kind change is remove + insert in one batch; make sure every dirty
        // node that remains has the entity its kind needs.
        for id in &report.dirty {
            self.ensure_entity(id, window, cx);
        }
        for id in &report.dirty {
            if let Some(entity) = self.containers.borrow().get(id).cloned() {
                entity.update(cx, |view, cx| {
                    view.sync_widget(window, cx);
                    cx.notify();
                });
            }
        }
        if report.needs_snapshot {
            cx.notify();
        }
    }

    /// SPEC §10.4: removing a focused node moves focus to the nearest remaining
    /// focusable ancestor, or clears it.
    fn recover_focus(
        &mut self,
        removed: &str,
        before: &RetainedTree,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focused = self.focused.borrow().clone();
        let Some(focused) = focused else {
            return;
        };
        let removed_ids: Vec<String> = std::iter::once(removed.to_string())
            .chain(
                before
                    .get(removed)
                    .map(|entry| entry.descendants(before))
                    .unwrap_or_default(),
            )
            .collect();
        if !removed_ids.iter().any(|id| id == &focused) {
            return;
        }
        let mut parent = before.get(removed).and_then(|entry| entry.parent.clone());
        while let Some(id) = parent {
            let entity = self.containers.borrow().get(&id).cloned();
            if let Some(entity) = entity {
                let focus = entity.read_with(cx, |view, cx| view.focus_handle(cx));
                if let Some(focus) = focus {
                    focus.focus(window, cx);
                    *self.focused.borrow_mut() = Some(id);
                    return;
                }
            }
            parent = before.get(&id).and_then(|entry| entry.parent.clone());
        }
        *self.focused.borrow_mut() = None;
        window.blur(cx);
    }

    fn ensure_entity(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.tree.borrow().get(id).cloned() else {
            return;
        };
        let is_root = self.tree.borrow().root.as_deref() == Some(id);
        if !is_root && under_virtual_list(&self.tree.borrow(), id) {
            // A >100-item List creates item entities on demand in `NodeFactory`.
            return;
        }
        if is_root || entry.is_container() || entry.is_focusable() {
            let mut containers = self.containers.borrow_mut();
            if containers.contains_key(id) {
                return;
            }
            let sink = self.sink(cx);
            let state = self.state.clone();
            let focused = self.focused.clone();
            let focus_index = self.focus_index.clone();
            let extensions = self.extensions.clone();
            let view = cx.new(|_| NodeView {
                id: id.to_string(),
                kind: entry.kind.clone(),
                theme: self.theme.clone(),
                tree: self.tree.clone(),
                containers: self.containers.clone(),
                charts: self.charts.clone(),
                images: self.images.clone(),
                tables: self.tables.clone(),
                table_models: self.table_models.clone(),
                tabs: self.tabs.clone(),
                lists: self.lists.clone(),
                image_cache: self.image_cache.clone(),
                panics: self.panics.clone(),
                fades: self.fades.clone(),
                widths: self.widths.clone(),
                overrides: self.overrides.clone(),
                metrics: self.metrics.clone(),
                state,
                sink,
                focused,
                focus_index,
                extensions,
                control: ControlState::default(),
                local: Rc::new(RefCell::new(Value::Null)),
                focus_handles: Vec::new(),
                subscriptions: Vec::new(),
                last_rev: None,
                pending: None,
                pending_user_edit: false,
                attached: false,
                watched_handles: 0,
                inject_panic: self.panic_injection,
                fade: true,
            });
            view.update(cx, |view, cx| view.attach(window, cx));
            containers.insert(id.to_string(), view);
            return;
        }
        match entry.kind.as_str() {
            "Chart" => {
                let mut charts = self.charts.borrow_mut();
                if charts.contains_key(id) {
                    return;
                }
                let view = cx.new(|_| ChartView {
                    id: id.to_string(),
                    tree: self.tree.clone(),
                    theme: self.theme.clone(),
                    widths: self.widths.clone(),
                    overrides: self.overrides.clone(),
                    metrics: self.metrics.clone(),
                    hover: None,
                    plot_bounds: Rc::new(std::cell::Cell::new(gpui_kit::Bounds::default())),
                });
                charts.insert(id.to_string(), view);
            }
            "Image" => {
                let mut images = self.images.borrow_mut();
                if images.contains_key(id) {
                    return;
                }
                let view = cx.new(|_| ImageView {
                    id: id.to_string(),
                    tree: self.tree.clone(),
                    theme: self.theme.clone(),
                    widths: self.widths.clone(),
                    overrides: self.overrides.clone(),
                    metrics: self.metrics.clone(),
                    image_cache: self.image_cache.clone(),
                });
                images.insert(id.to_string(), view);
            }
            _ => {}
        }
    }

    fn prune_entities(&self, id: &str, removed: &BTreeMap<String, Vec<String>>) {
        let mut ids = vec![id.to_string()];
        ids.extend(removed.get(id).into_iter().flatten().cloned());
        let mut containers = self.containers.borrow_mut();
        let mut charts = self.charts.borrow_mut();
        let mut images = self.images.borrow_mut();
        let mut tables = self.tables.borrow_mut();
        let mut table_models = self.table_models.borrow_mut();
        let mut tabs = self.tabs.borrow_mut();
        let mut lists = self.lists.borrow_mut();
        for id in ids {
            containers.remove(&id);
            charts.remove(&id);
            images.remove(&id);
            tables.remove(&id);
            table_models.remove(&id);
            tabs.remove(&id);
            lists.remove(&id);
        }
    }

    fn notify_for(&self, id: &str, cx: &mut Context<Self>) {
        if let Some(entity) = self.containers.borrow().get(id).cloned() {
            entity.update(cx, |_, cx| cx.notify());
            return;
        }
        if let Some(entity) = self.charts.borrow().get(id).cloned() {
            entity.update(cx, |_, cx| cx.notify());
            // The titled wrapper around the cached plot is built by the parent.
        } else if let Some(entity) = self.images.borrow().get(id).cloned() {
            entity.update(cx, |_, cx| cx.notify());
            // The parent owns the cached size, so it must rebuild too.
        }
        // A stateless leaf has no entity; notify its nearest container.
        let mut parent = self
            .tree
            .borrow()
            .get(id)
            .and_then(|entry| entry.parent.clone());
        let mut guard = 0;
        while let Some(id) = parent {
            guard += 1;
            if guard > 256 {
                break;
            }
            if let Some(entity) = self.containers.borrow().get(&id).cloned() {
                entity.update(cx, |_, cx| cx.notify());
                return;
            }
            parent = self
                .tree
                .borrow()
                .get(&id)
                .and_then(|entry| entry.parent.clone());
        }
        cx.notify();
    }

    fn notify_root(&self, cx: &mut Context<Self>) {
        // The root can appear or disappear between frames. Rebuild UiView's own
        // element tree in that case; normal child updates only notify entities.
        cx.notify();
        let root = self.tree.borrow().root.clone();
        if let Some(root) = root {
            self.notify_for(&root, cx);
        }
    }

    fn content_width(&self, window: &Window) -> Pixels {
        let available = window.viewport_size().width.as_f32().max(0.0);
        let host_cap = self.max_width.map(|width| width.as_f32());
        let message = self.theme.message_max_width().as_f32();
        px(available.min(host_cap.unwrap_or(message)))
    }
}

fn under_virtual_list(tree: &RetainedTree, id: &str) -> bool {
    let mut current = tree.get(id).and_then(|entry| entry.parent.clone());
    let mut guard = 0usize;
    while let Some(parent) = current {
        guard += 1;
        if guard > tree.len() + 1 {
            return false;
        }
        let Some(entry) = tree.get(&parent) else {
            return false;
        };
        if entry.kind == "List" && crate::data::should_virtualize(entry.primary_len()) {
            return true;
        }
        current = entry.parent.clone();
    }
    false
}

fn collect_ids(tree: &RetainedTree, id: &str, out: &mut Vec<String>) {
    if let Some(entry) = tree.get(id) {
        for child in entry
            .children
            .values()
            .flat_map(|children| children.iter().cloned().collect::<Vec<_>>())
        {
            out.push(child.clone());
            collect_ids(tree, &child, out);
        }
    }
}

impl EventEmitter<UiEvent> for UiView {}

/// GPUI has no built-in Tab traversal; multi-line inputs indent on Tab. In a
/// generative UI form, Tab is focus traversal (SPEC §10.4), so remove the
/// input-local indent binding and let `UiView`'s root key handler move focus.
fn install_key_bindings(cx: &mut App) {
    use gpui_component::input::{IndentInline, OutdentInline};
    use gpui_kit::{Action as _, KeyBinding, Unbind};
    cx.bind_keys([
        // The kit Root owns global Tab traversal and consumes the key before
        // the view sees it. Unbind that pair so this view can emit `Blur`
        // before it advances focus.
        KeyBinding::new("tab", Unbind(SharedString::from("root::Tab")), None),
        KeyBinding::new(
            "shift-tab",
            Unbind(SharedString::from("root::TabPrev")),
            None,
        ),
        KeyBinding::new("tab", Unbind(IndentInline.name().into()), Some("Input")),
        KeyBinding::new(
            "shift-tab",
            Unbind(OutdentInline.name().into()),
            Some("Input"),
        ),
    ]);
}

fn value_text(value: &Value) -> Option<String> {
    match value {
        Value::Str(text) => Some(text.text.to_string()),
        Value::Int(value) => Some(value.to_string()),
        Value::Float(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

/// User-entered strings carry `User` provenance (SPEC §7.6).
fn user_text(text: &str) -> Value {
    Value::Str(PStr {
        text: Arc::from(text),
        prov: Provenance::User,
    })
}

impl Render for UiView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let started = Instant::now();
        if !self.queue.is_empty() {
            self.metrics.borrow_mut().queue_drains += 1;
            while let Some(batch) = self.queue.pop_front() {
                self.apply_batch(batch, window, cx);
            }
        }
        let root_key = ElementId::Name(SharedString::from("genui-root"));
        let focused = self.focused.clone();
        let focus_index = self.focus_index.clone();
        let sink = self.sink(cx);
        let width = self.content_width(window);
        let root_id = self.tree.borrow().root.clone();
        if let Some(root_id) = &root_id {
            self.widths.set(root_id, width);
        }
        let program: AnyElement = match root_id.and_then(|root| {
            self.containers
                .borrow()
                .get(&root)
                .cloned()
                .map(|entity| (root, entity))
        }) {
            Some((_, entity)) => AnyView::from(entity).into_any_element(),
            None => {
                let style = self.theme.text(TextRole::Body);
                div()
                    .text_size(style.size)
                    .line_height(style.line_height)
                    .text_color(self.theme.muted_foreground())
                    .child("Waiting for a program…")
                    .into_any_element()
            }
        };
        let element = div()
            .id(root_key)
            .track_focus(&self.root_focus)
            .tab_group()
            .size_full()
            .bg(self.theme.background())
            .on_key_down(
                move |event, window, cx| match event.keystroke.key.as_str() {
                    "tab" => {
                        clear_focus(&focused, &sink, cx);
                        if event.keystroke.modifiers.shift {
                            window.focus_prev(cx);
                        } else {
                            window.focus_next(cx);
                        }
                        if let Some(handle) = window.focused(cx)
                            && let Some(path) = lookup_focus(&focus_index, &handle)
                        {
                            *focused.borrow_mut() = Some(path);
                        }
                        cx.stop_propagation();
                    }
                    "escape" => {
                        clear_focus(&focused, &sink, cx);
                        window.blur(cx);
                        cx.stop_propagation();
                    }
                    _ => {}
                },
            )
            .child(div().w(width).h_full().child(program));
        {
            let mut metrics = self.metrics.borrow_mut();
            metrics.record_render(started.elapsed());
            let total = metrics.node_render_ns;
            let delta = total.saturating_sub(metrics.frame_app_baseline_ns);
            metrics.frame_app_baseline_ns = total;
            metrics
                .frame_app_samples
                .push(std::time::Duration::from_nanos(delta));
        }
        element
    }
}

/// One retained container or focusable node (SPEC §10.1). Stateless leaves are
/// built by their parent; this view also serves the root when the root is a leaf.
pub struct NodeView {
    pub id: String,
    pub kind: String,
    pub theme: UiTheme,
    pub tree: Rc<RefCell<RetainedTree>>,
    pub containers: Rc<RefCell<BTreeMap<String, Entity<NodeView>>>>,
    pub charts: Rc<RefCell<BTreeMap<String, Entity<ChartView>>>>,
    pub images: Rc<RefCell<BTreeMap<String, Entity<ImageView>>>>,
    pub tables: TableMap,
    pub table_models: TableModelCache,
    pub tabs: TabsMap,
    pub lists: ListMap,
    pub image_cache: Rc<RefCell<ImageCache>>,
    pub panics: Rc<RefCell<PanicTracker>>,
    pub fades: Rc<RefCell<BTreeMap<String, u64>>>,
    pub widths: Widths,
    pub overrides: Rc<StateOverrides>,
    pub metrics: Rc<RefCell<Metrics>>,
    pub state: StateMap,
    pub sink: EventSink,
    pub focused: Rc<RefCell<Option<String>>>,
    pub focus_index: FocusIndex,
    pub extensions: ExtensionRegistry,
    pub control: ControlState,
    pub local: Rc<RefCell<Value>>,
    pub focus_handles: Vec<FocusHandle>,
    pub subscriptions: Vec<Subscription>,
    pub last_rev: Option<u64>,
    pub pending: Option<StateValue>,
    pub pending_user_edit: bool,
    pub attached: bool,
    pub watched_handles: usize,
    pub inject_panic: bool,
    pub fade: bool,
}

/// Creates item `NodeView` entities on demand for the virtualized List. Only
/// visible rows are built, and the entity is cached so a later update notifies
/// just that row (SPEC §10.1/§10.3).
#[derive(Clone)]
pub(crate) struct NodeFactory {
    pub theme: UiTheme,
    pub tree: Rc<RefCell<RetainedTree>>,
    pub containers: Rc<RefCell<BTreeMap<String, Entity<NodeView>>>>,
    pub charts: Rc<RefCell<BTreeMap<String, Entity<ChartView>>>>,
    pub images: Rc<RefCell<BTreeMap<String, Entity<ImageView>>>>,
    pub tables: TableMap,
    pub table_models: TableModelCache,
    pub tabs: TabsMap,
    pub lists: ListMap,
    pub image_cache: Rc<RefCell<ImageCache>>,
    pub panics: Rc<RefCell<PanicTracker>>,
    pub fades: Rc<RefCell<BTreeMap<String, u64>>>,
    pub widths: Widths,
    pub overrides: Rc<StateOverrides>,
    pub metrics: Rc<RefCell<Metrics>>,
    pub state: StateMap,
    pub sink: EventSink,
    pub focused: Rc<RefCell<Option<String>>>,
    pub focus_index: FocusIndex,
    pub extensions: ExtensionRegistry,
    pub inject_panic: bool,
}

impl NodeFactory {
    pub(crate) fn entity<C: AppContext>(
        &self,
        id: &str,
        window: &mut Window,
        cx: &mut C,
    ) -> Option<Entity<NodeView>> {
        if let Some(entity) = self.containers.borrow().get(id).cloned() {
            return Some(entity);
        }
        let kind = self.tree.borrow().get(id)?.kind.clone();
        let view = cx.new(|_| NodeView {
            id: id.to_string(),
            kind,
            theme: self.theme.clone(),
            tree: self.tree.clone(),
            containers: self.containers.clone(),
            charts: self.charts.clone(),
            images: self.images.clone(),
            tables: self.tables.clone(),
            table_models: self.table_models.clone(),
            tabs: self.tabs.clone(),
            lists: self.lists.clone(),
            image_cache: self.image_cache.clone(),
            panics: self.panics.clone(),
            fades: self.fades.clone(),
            widths: self.widths.clone(),
            overrides: self.overrides.clone(),
            metrics: self.metrics.clone(),
            state: self.state.clone(),
            sink: self.sink.clone(),
            focused: self.focused.clone(),
            focus_index: self.focus_index.clone(),
            extensions: self.extensions.clone(),
            control: ControlState::default(),
            local: Rc::new(RefCell::new(Value::Null)),
            focus_handles: Vec::new(),
            subscriptions: Vec::new(),
            last_rev: None,
            pending: None,
            pending_user_edit: false,
            attached: false,
            watched_handles: 0,
            inject_panic: self.inject_panic,
            fade: false,
        });
        view.update(cx, |view, cx| view.attach(window, cx));
        self.containers
            .borrow_mut()
            .insert(id.to_string(), view.clone());
        Some(view)
    }
}

impl NodeView {
    fn entry(&self) -> Option<NodeEntry> {
        self.tree.borrow().get(&self.id).cloned()
    }

    fn meta(&self, id: &str) -> Option<(String, genui_core::ir::NodeStatus, u64)> {
        self.tree
            .borrow()
            .get(id)
            .map(|entry| (entry.kind.clone(), entry.status, entry.rev))
    }

    fn factory(&self) -> NodeFactory {
        NodeFactory {
            theme: self.theme.clone(),
            tree: self.tree.clone(),
            containers: self.containers.clone(),
            charts: self.charts.clone(),
            images: self.images.clone(),
            tables: self.tables.clone(),
            table_models: self.table_models.clone(),
            tabs: self.tabs.clone(),
            lists: self.lists.clone(),
            image_cache: self.image_cache.clone(),
            panics: self.panics.clone(),
            fades: self.fades.clone(),
            widths: self.widths.clone(),
            overrides: self.overrides.clone(),
            metrics: self.metrics.clone(),
            state: self.state.clone(),
            sink: self.sink.clone(),
            focused: self.focused.clone(),
            focus_index: self.focus_index.clone(),
            extensions: self.extensions.clone(),
            inject_panic: self.inject_panic,
        }
    }

    fn state(&self, entry: &NodeEntry) -> VisualState {
        self.overrides
            .get(&self.id)
            .copied()
            .unwrap_or_else(|| data_state(&entry.props).unwrap_or_else(|| state_of(entry.status)))
    }

    fn disabled(&self, entry: &NodeEntry) -> bool {
        self.sink.frozen()
            || entry.status == genui_core::ir::NodeStatus::Partial
            || self.state(entry) == VisualState::Disabled
            || self.state(entry) == VisualState::Partial
    }

    fn binding(&self, entry: &NodeEntry) -> Option<String> {
        props::binding(&entry.props, "bind")
    }

    fn store_value(&self, entry: &NodeEntry) -> Option<StateValue> {
        let binding = self.binding(entry)?;
        self.state.borrow().get(&binding).cloned()
    }

    fn value_or(&self, entry: &NodeEntry, fallback: Value) -> Value {
        self.store_value(entry)
            .map(|state| state.value)
            .unwrap_or_else(|| {
                let local = self.local.borrow().clone();
                if matches!(local, Value::Null) {
                    fallback
                } else {
                    local
                }
            })
    }

    fn focus_handle(&self, cx: &App) -> Option<FocusHandle> {
        match &self.control.text {
            Some(text) => Some(text.focus_handle(cx)),
            None => self
                .control
                .select
                .as_ref()
                .map(|select| select.focus.clone())
                .or_else(|| self.focus_handles.first().cloned()),
        }
    }

    fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.focus_handle(cx)
            .is_some_and(|handle| handle.is_focused(window))
    }

    /// Create the widget state and subscriptions once per retained node
    /// (SPEC §10.1, §10.4).
    fn attach(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.attached {
            return;
        }
        self.attached = true;
        let Some(entry) = self.entry() else {
            return;
        };
        match entry.kind.as_str() {
            "Input" => self.attach_input(window, cx, &entry, false),
            "TextArea" => self.attach_input(window, cx, &entry, true),
            "Select" => self.attach_select(window, cx, &entry),
            "Checkbox" | "Switch" => {
                self.focus_handles.push(cx.focus_handle());
                self.watch_handles(window, cx);
            }
            "CheckboxGroup" | "RadioGroup" => {
                let count = props::options(&entry.props, "options").len();
                while self.focus_handles.len() < count {
                    self.focus_handles.push(cx.focus_handle());
                }
                self.watch_handles(window, cx);
            }
            _ => {}
        }
    }

    fn watch_handles(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for index in self.watched_handles..self.focus_handles.len() {
            let handle = self.focus_handles[index].clone();
            register_focus(&self.focus_index, &handle, &self.id);
            let sink = self.sink.clone();
            let id = self.id.clone();
            let focused = self.focused.clone();
            let focus_sink = sink.clone();
            let focus_id = id.clone();
            let focus_state = focused.clone();
            let focus = cx.on_focus(&handle, window, move |_, _, cx| {
                focus_node(&focus_state, &focus_sink, &focus_id, cx);
            });
            self.subscriptions.push(focus);
            let subscription = cx.on_blur(&handle, window, move |_, _, cx| {
                if focused.borrow().as_deref() == Some(id.as_str()) {
                    focused.borrow_mut().take();
                    sink.blur(&id, cx);
                }
            });
            self.subscriptions.push(subscription);
        }
        self.watched_handles = self.focus_handles.len();
    }

    fn attach_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        entry: &NodeEntry,
        textarea: bool,
    ) {
        let placeholder = props::text_or(&entry.props, "placeholder", "");
        let text = if textarea {
            let state = cx.new(|cx| {
                let mut state = TextareaState::new(window, cx);
                state = state.placeholder(placeholder);
                state
            });
            TextWidget::Area(state)
        } else {
            let state = cx.new(|cx| {
                let mut state = InputState::new(window, cx);
                state = state.placeholder(placeholder);
                state
            });
            TextWidget::Input(state)
        };
        let focus = text.focus_handle(cx);
        self.focus_handles.push(focus.clone());
        register_focus(&self.focus_index, &focus, &self.id);
        self.watch_handles(window, cx);
        let sink = self.sink.clone();
        let id = self.id.clone();
        let focused = self.focused.clone();
        let node = cx.entity().downgrade();
        let subscription =
            match &text {
                TextWidget::Input(state) => {
                    let state = state.clone();
                    cx.subscribe_in(&state, window, move |view, state, event, window, cx| {
                        match event {
                            InputEvent::Change => {
                                let value = user_text(state.read(cx).value().as_ref());
                                *view.local.borrow_mut() = value.clone();
                                sink.edit(&id, value, cx);
                            }
                            InputEvent::Focus => {
                                focus_node(&focused, &sink, &id, cx);
                                node.update(cx, |_, cx| cx.notify()).ok();
                            }
                            InputEvent::Blur => {
                                if focused.borrow().as_deref() == Some(id.as_str()) {
                                    focused.borrow_mut().take();
                                }
                                view.flush_pending(window, cx);
                            }
                            InputEvent::PressEnter { .. } => {
                                view.submit_from_keyboard(cx);
                            }
                        }
                    })
                }
                TextWidget::Area(state) => {
                    let state = state.clone();
                    cx.subscribe_in(&state, window, move |view, state, event, window, cx| {
                        match event {
                            InputEvent::Change => {
                                let value = user_text(state.read(cx).value().as_ref());
                                *view.local.borrow_mut() = value.clone();
                                sink.edit(&id, value, cx);
                            }
                            InputEvent::Focus => {
                                focus_node(&focused, &sink, &id, cx);
                                node.update(cx, |_, cx| cx.notify()).ok();
                            }
                            InputEvent::Blur => {
                                if focused.borrow().as_deref() == Some(id.as_str()) {
                                    focused.borrow_mut().take();
                                }
                                view.flush_pending(window, cx);
                            }
                            // Enter is a newline in a multiline field.
                            InputEvent::PressEnter { .. } => {}
                        }
                    })
                }
            };
        self.subscriptions.push(subscription);
        self.control.text = Some(text);
        self.sync_widget(window, cx);
    }

    fn attach_select(&mut self, window: &mut Window, cx: &mut Context<Self>, entry: &NodeEntry) {
        let options = props::options(&entry.props, "options");
        let values: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
        let selected_value = self
            .store_value(entry)
            .and_then(|state| match state.value {
                Value::Str(text) => Some(text.text.to_string()),
                _ => None,
            })
            .or_else(|| match &*self.local.borrow() {
                Value::Str(text) => Some(text.text.to_string()),
                _ => None,
            });
        let selected = selected_value
            .and_then(|value| values.iter().position(|item| item == &value))
            .map(|index| gpui_component::IndexPath::default().row(index));
        let state =
            cx.new(|cx| SelectState::new(SearchableVec::new(values.clone()), selected, window, cx));
        let focus = state.read(cx).focus_handle(cx);
        self.focus_handles.push(focus.clone());
        register_focus(&self.focus_index, &focus, &self.id);
        self.watch_handles(window, cx);
        let sink = self.sink.clone();
        let id = self.id.clone();
        let subscription = cx.subscribe_in(&state, window, move |view, _, event, _, cx| {
            if let SelectEvent::Confirm(Some(value)) = event {
                view.pending_user_edit = true;
                sink.edit(&id, user_text(value), cx);
            }
        });
        self.subscriptions.push(subscription);
        let focused = self.focused.clone();
        let focus_sink = self.sink.clone();
        let focus_id = self.id.clone();
        let focus_state = focused.clone();
        let focus_in = cx.on_focus(&focus, window, move |_, _, cx| {
            focus_node(&focus_state, &focus_sink, &focus_id, cx);
        });
        self.subscriptions.push(focus_in);
        let blur_sink = self.sink.clone();
        let blur_id = self.id.clone();
        let blur = cx.on_blur(&focus, window, move |_, _, cx| {
            if focused.borrow().as_deref() == Some(blur_id.as_str()) {
                focused.borrow_mut().take();
            }
            blur_sink.blur(&blur_id, cx);
        });
        self.subscriptions.push(blur);
        self.control.select = Some(SelectWidget {
            state,
            focus,
            values,
        });
        self.sync_widget(window, cx);
    }

    /// Store-to-widget sync (SPEC §10.4). User echoes and revisions already
    /// pushed are ignored; a non-user change while focused waits for blur.
    fn sync_widget(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.entry() else {
            return;
        };
        if self.control.text.is_none() && self.control.select.is_none() {
            return;
        }
        let disabled = self.disabled(&entry);
        if let Some(text) = &self.control.text {
            text.set_disabled(disabled, cx);
        }
        self.sync_select(&entry, window, cx);
        let Some(store) = self.store_value(&entry) else {
            return;
        };
        if self.last_rev == Some(store.rev) {
            return;
        }
        let focused = self.is_focused(window, cx);
        if store.origin == StateOrigin::User && self.pending_user_edit {
            self.pending_user_edit = false;
            self.last_rev = Some(store.rev);
            self.pending = None;
            return;
        }
        if focused {
            self.pending = Some(store);
            return;
        }
        self.apply_store(&store, window, cx);
    }

    fn sync_select(&mut self, entry: &NodeEntry, window: &mut Window, cx: &mut Context<Self>) {
        let options = props::options(&entry.props, "options");
        let values: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
        let selected = self
            .store_value(entry)
            .and_then(|state| value_text(&state.value))
            .or_else(|| value_text(&self.local.borrow()));
        let Some(select) = self.control.select.take() else {
            return;
        };
        if select.values != values {
            let items = SearchableVec::new(values.clone());
            select
                .state
                .update(cx, |state, cx| state.set_items(items, window, cx));
        }
        if let Some(selected) = selected {
            let current = select.state.read(cx).selected_value().cloned();
            if current.as_deref() != Some(selected.as_str()) {
                select.state.update(cx, |state, cx| {
                    state.set_selected_value(&selected, window, cx)
                });
            }
        }
        let mut select = select;
        select.values = values;
        self.control.select = Some(select);
    }

    fn flush_pending(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(store) = self.pending.take() {
            self.apply_store(&store, window, cx);
        }
    }

    fn apply_store(&mut self, store: &StateValue, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.entry() else {
            return;
        };
        if let Some(text) = &self.control.text {
            let value = value_text(&store.value).unwrap_or_default();
            if text.value(cx) != value {
                text.set_value(&value, window, cx);
            }
        }
        if let Some(select) = self.control.select.take() {
            let value = value_text(&store.value);
            if let Some(value) = value {
                select.state.update(cx, |state, cx| {
                    state.set_selected_value(&value, window, cx);
                });
            }
            self.control.select = Some(select);
        }
        self.last_rev = Some(store.rev);
        let _ = entry;
    }

    /// DESIGN/SPEC §10.4: Enter in a single-line field submits only when the
    /// enclosing form has exactly one submit button.
    fn submit_from_keyboard(&self, cx: &mut App) {
        let Some((node, action)) = self.submit_button() else {
            return;
        };
        self.sink.activate(&node, &action, cx);
    }

    fn submit_button(&self) -> Option<(String, String)> {
        let tree = self.tree.borrow();
        let mut parent = tree.get(&self.id).and_then(|entry| entry.parent.clone());
        let mut form = None;
        while let Some(id) = parent {
            let entry = tree.get(&id)?;
            if entry.kind == "Form" {
                form = Some(entry.clone());
                break;
            }
            parent = entry.parent.clone();
        }
        let form = form?;
        let mut submits = Vec::new();
        for id in form.descendants(&tree) {
            let entry = tree.get(&id)?;
            if entry.kind == "Button" && props::bool_(&entry.props, "submit", false) {
                submits.push(entry.clone());
            }
        }
        if submits.len() != 1 {
            return None;
        }
        let button = &submits[0];
        let action = props::action(&button.props, "on_click")
            .or_else(|| props::action(&form.props, "on_submit"))?;
        Some((button.id.clone(), action))
    }

    fn form_submit_action(&self) -> Option<String> {
        let tree = self.tree.borrow();
        let entry = tree.get(&self.id)?;
        if entry.kind != "Button" || !props::bool_(&entry.props, "submit", false) {
            return None;
        }
        if let Some(action) = props::action(&entry.props, "on_click") {
            return Some(action);
        }
        let mut parent = entry.parent.clone();
        while let Some(id) = parent {
            let form = tree.get(&id)?;
            if form.kind == "Form" {
                return props::action(&form.props, "on_submit");
            }
            parent = form.parent.clone();
        }
        None
    }

    fn child_widths(&self, entry: &NodeEntry, width: Pixels) -> (Vec<String>, Vec<Pixels>) {
        let children: Vec<String> = entry.primary_children();
        let widths = match entry.kind.as_str() {
            "Card" => {
                let padding =
                    self.theme
                        .space(crate::props::token_step(&entry.props, "padding", 4));
                let content = inset_width(width, padding);
                vec![content; children.len()]
            }
            "Grid" => {
                let requested = u16::try_from(crate::props::int(&entry.props, "cols").unwrap_or(2))
                    .unwrap_or(2);
                let gap = self
                    .theme
                    .space(crate::props::token_step(&entry.props, "gap", 2));
                let cols =
                    crate::layout::grid_columns(width, requested, gap, self.theme.grid_min_cell());
                let cell = crate::layout::grid_cell_width(width, cols, gap);
                vec![cell; children.len()]
            }
            "Row" => {
                let gap = self
                    .theme
                    .space(crate::props::token_step(&entry.props, "gap", 2));
                let cell = row_cell_width(width, children.len(), gap);
                vec![cell; children.len()]
            }
            "List" => {
                let inset = self.theme.list_gutter_width() + self.theme.space(1);
                let content = px((width.as_f32() - inset.as_f32()).max(0.0));
                vec![content; children.len()]
            }
            _ => vec![width; children.len()],
        };
        (children, widths)
    }

    fn child_element(
        &mut self,
        id: &str,
        index: usize,
        parent: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((kind, status, _rev)) = self.meta(id) else {
            return div().into_any_element();
        };
        // Containers are entities so a change notifies only their subtree
        // (SPEC §10.1). Do not clone their props: a table prop can hold 50,000
        // rows, and this runs on every parent frame.
        if let Some(entity) = self.containers.borrow().get(id).cloned() {
            return AnyView::from(entity).into_any_element();
        }
        let Some(entry) = self.tree.borrow().get(id).cloned() else {
            return div().into_any_element();
        };
        let status_value = props::status(&entry.props);
        let state = data::data_state(
            self.overrides
                .get(id)
                .copied()
                .unwrap_or_else(|| state_of(status)),
            status_value.as_deref(),
        );
        let mut element = if kind == "Chart" {
            let width = self.widths.get(id, self.theme.message_max_width());
            let model = ChartModel::from_props(&entry.props);
            let plot = match self.charts.borrow().get(id).cloned() {
                Some(entity) => chart::cached_chart(entity, &self.theme, width),
                None => chart::plot_element(&self.theme, &model, width, id, state),
            };
            chart::panel(&self.theme, &model, plot, width)
        } else if kind == "Image" {
            let width = self.widths.get(id, self.theme.message_max_width());
            match self.images.borrow().get(id).cloned() {
                Some(entity) => cached_image(entity, &self.theme, width, &entry.props),
                None => image_element(&self.theme, &entry.props, width, state, &self.image_cache),
            }
        } else {
            self.build_entry(&entry, state, window, cx)
        };
        if kind == "Heading" && index > 0 {
            element = div()
                .mt(self.theme.heading_extra_top())
                .child(element)
                .into_any_element();
        }
        // A Badge or Button keeps its natural width in a stretching stack
        // (DESIGN §6 rules 4 and 7).
        if matches!(parent, "Column" | "Card" | "Form" | "List" | "Tabs" | "Tab")
            && matches!(kind.as_str(), "Badge" | "Button")
        {
            element = div()
                .flex()
                .flex_row()
                .items_center()
                .self_start()
                .child(element)
                .into_any_element();
        }
        if let Some(generation) = self.fades.borrow().get(id).copied() {
            element = crate::motion::fade_in(&self.theme, id, generation, element);
        }
        element
    }

    fn table_element(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let width = self.widths.get(id, self.theme.message_max_width());
        let (rows_rev, props_rev, column_ids) = {
            let tree = self.tree.borrow();
            let Some(entry) = tree.get(id) else {
                return div().into_any_element();
            };
            (entry.rows_rev, entry.rev, entry.children_in("columns"))
        };
        let mut columns_key = 17u64;
        {
            let tree = self.tree.borrow();
            for column in &column_ids {
                if let Some(entry) = tree.get(column) {
                    columns_key = columns_key
                        .wrapping_mul(31)
                        .wrapping_add(entry.rev)
                        .wrapping_add(u64::from(entry.kind.len() as u32));
                }
            }
        }
        let cached = self
            .table_models
            .borrow()
            .get(id)
            .filter(|(cached_rows, _, _, _)| *cached_rows == rows_rev)
            .map(|(_, cached_rev, cached_columns, model)| {
                (*cached_rev, *cached_columns, model.clone())
            });
        let model = match cached {
            Some((cached_rev, cached_columns, model))
                if cached_rev == props_rev && cached_columns == columns_key =>
            {
                model
            }
            Some((_, _, model)) => {
                let (entry, columns) = {
                    let tree = self.tree.borrow();
                    let Some(entry) = tree.get(id).cloned() else {
                        return div().into_any_element();
                    };
                    let columns: Vec<NodeEntry> = column_ids
                        .iter()
                        .filter_map(|column| tree.get(column).cloned())
                        .collect();
                    (entry, columns)
                };
                let rebuilt = model.rebuild(&entry, &columns);
                self.table_models.borrow_mut().insert(
                    id.to_string(),
                    (rows_rev, props_rev, columns_key, rebuilt.clone()),
                );
                rebuilt
            }
            None => {
                let (entry, columns) = {
                    let tree = self.tree.borrow();
                    let Some(entry) = tree.get(id).cloned() else {
                        return div().into_any_element();
                    };
                    let columns: Vec<NodeEntry> = column_ids
                        .iter()
                        .filter_map(|column| tree.get(column).cloned())
                        .collect();
                    (entry, columns)
                };
                let started = Instant::now();
                let model = TableModel::from_entry(&entry, &columns);
                let elapsed = started.elapsed();
                self.metrics.borrow_mut().table_parse_ns += elapsed.as_nanos() as u64;
                // The parsed model is now the single owner of the row data.
                if let Some(entry) = self.tree.borrow_mut().nodes.get_mut(id) {
                    entry.props.remove("rows");
                }
                self.table_models.borrow_mut().insert(
                    id.to_string(),
                    (rows_rev, props_rev, columns_key, model.clone()),
                );
                model
            }
        };
        let inside_card = self
            .tree
            .borrow()
            .get(id)
            .and_then(|entry| entry.parent.clone())
            .and_then(|parent| {
                self.tree
                    .borrow()
                    .get(&parent)
                    .map(|entry| entry.kind.clone())
            })
            .is_some_and(|kind| kind == "Card");
        let theme = self.theme.clone();
        let existing = self.tables.borrow().get(id).cloned();
        let table = match existing {
            Some(entity) => {
                let model_for_update = model.clone();
                let theme_for_update = theme.clone();
                entity.update(cx, |table, cx| {
                    if table
                        .delegate_mut()
                        .update(model_for_update, width, theme_for_update)
                    {
                        table.refresh(cx);
                    }
                });
                entity
            }
            None => {
                let delegate =
                    GenuiTableDelegate::new(model.clone(), width, theme.clone(), id.to_string());
                let entity = cx.new(|cx| {
                    TableState::new(delegate, window, cx)
                        .sortable(true)
                        .row_selectable(true)
                        .col_resizable(true)
                        .col_movable(false)
                        .row_header(false)
                });
                self.tables
                    .borrow_mut()
                    .insert(id.to_string(), entity.clone());
                entity
            }
        };
        table::element(&table, &theme, &model, inside_card)
    }

    fn virtual_list_element(
        &mut self,
        entry: &NodeEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let existing = self.lists.borrow().get(&entry.id).cloned();
        let entity = match existing {
            Some(entity) => {
                entity.update(cx, |list, cx| {
                    if list.last_rev != entry.rev {
                        list.last_rev = entry.rev;
                        cx.notify();
                    }
                });
                entity
            }
            None => {
                let entity = cx.new(|cx| {
                    crate::data::ListView::new(
                        entry.id.clone(),
                        self.tree.clone(),
                        self.theme.clone(),
                        self.widths.clone(),
                        self.factory(),
                        window,
                        cx,
                    )
                });
                self.lists
                    .borrow_mut()
                    .insert(entry.id.clone(), entity.clone());
                entity
            }
        };
        AnyView::from(entity).into_any_element()
    }

    fn tabs_element(&mut self, id: &str, cx: &mut Context<Self>) -> AnyElement {
        let Some(rev) = self.tree.borrow().get(id).map(|entry| entry.rev) else {
            return div().into_any_element();
        };
        let existing = self.tabs.borrow().get(id).cloned();
        let entity = match existing {
            Some(entity) => {
                entity.update(cx, |tabs, cx| {
                    if tabs.last_rev != rev {
                        tabs.last_rev = rev;
                        cx.notify();
                    }
                });
                entity
            }
            None => {
                let entity = cx.new(|cx| {
                    TabsView::new(
                        id.to_string(),
                        self.tree.clone(),
                        self.containers.clone(),
                        self.theme.clone(),
                        self.widths.clone(),
                        self.overrides.clone(),
                        self.sink.clone(),
                        cx,
                    )
                });
                self.tabs
                    .borrow_mut()
                    .insert(id.to_string(), entity.clone());
                entity
            }
        };
        AnyView::from(entity).into_any_element()
    }

    fn build_control(
        &mut self,
        entry: &NodeEntry,
        _state: VisualState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme.clone();
        let display = FieldDisplay::from_props(&entry.props);
        let disabled = self.disabled(entry);
        let label = props::text_or(&entry.props, "label", "");
        let required = props::bool_(&entry.props, "required", false);
        let node = cx.entity().downgrade();
        let sink = self.sink.clone();
        let local = self.local.clone();
        match entry.kind.as_str() {
            "Input" | "TextArea" => {
                let focused = self.is_focused(window, cx);
                let error = display.visible_error().is_some();
                let kind = props::name_or(&entry.props, "kind", "text");
                let control: AnyElement = match &self.control.text {
                    Some(TextWidget::Input(input)) => {
                        let mut element = Input::new(input)
                            .id(ElementId::Name(SharedString::from(format!(
                                "genui-input-{}",
                                entry.id
                            ))))
                            .appearance(false)
                            .bordered(false)
                            .focus_bordered(false)
                            .disabled(disabled)
                            .aria_label(label.clone())
                            .text_color(theme.foreground())
                            .h_full();
                        element = match kind.as_str() {
                            "email" => element.content_type(InputContentType::EmailAddress),
                            "url" => element.content_type(InputContentType::Url),
                            _ => element,
                        };
                        element.into_any_element()
                    }
                    Some(TextWidget::Area(area)) => Textarea::new(area)
                        .appearance(false)
                        .bordered(false)
                        .disabled(disabled)
                        .aria_label(label.clone())
                        .text_color(theme.foreground())
                        .h_full()
                        .into_any_element(),
                    None => div().into_any_element(),
                };
                let shell = if entry.kind == "TextArea" {
                    let rows = props::int(&entry.props, "rows").unwrap_or(4);
                    controls::textarea_shell(
                        &theme,
                        control,
                        controls::ShellState {
                            focused,
                            error,
                            disabled,
                        },
                        rows,
                        window,
                        cx,
                    )
                } else {
                    controls::control_shell(
                        &theme,
                        control,
                        controls::ShellState {
                            focused,
                            error,
                            disabled,
                        },
                        theme.control_height(crate::theme::ControlSize::Medium),
                        window,
                        cx,
                    )
                };
                let mut field = div()
                    .id(ElementId::Name(SharedString::from(format!(
                        "genui-field-{}",
                        entry.id
                    ))))
                    .test_support();
                if let Some(handle) = self.focus_handle(cx) {
                    let sink = self.sink.clone();
                    let focused = self.focused.clone();
                    let node_id = entry.id.clone();
                    field = field.on_click(move |_, window, cx| {
                        focus_node(&focused, &sink, &node_id, cx);
                        handle.focus(window, cx);
                    });
                }
                let shell = field.child(shell).into_any_element();
                let _ = (required, &local, &node);
                controls::field(&theme, &entry.id, &label, shell, &display, disabled)
            }
            "Select" => {
                let focused = self.is_focused(window, cx);
                let error = display.visible_error().is_some();
                let control: AnyElement = match &self.control.select {
                    Some(select) => {
                        let mut element = Select::new(&select.state)
                            .id(ElementId::Name(SharedString::from(format!(
                                "genui-select-{}",
                                entry.id
                            ))))
                            .appearance(false)
                            .focus_ring(false)
                            .disabled(disabled)
                            .accessibility_label(label.clone())
                            .text_color(theme.foreground())
                            .icon(
                                gpui_component::Icon::new(gpui_component::IconName::ChevronDown)
                                    .text_color(theme.surface()),
                            );
                        if let Some(placeholder) = props::text(&entry.props, "placeholder") {
                            element = element.placeholder(placeholder);
                        }
                        element.into_any_element()
                    }
                    None => div().into_any_element(),
                };
                let control = div()
                    .flex()
                    .items_center()
                    .w_full()
                    .gap(theme.space(1))
                    .child(div().flex_1().min_w_0().child(control))
                    .child(controls::select_chevron(&theme))
                    .into_any_element();
                let shell = controls::control_shell(
                    &theme,
                    control,
                    controls::ShellState {
                        focused,
                        error,
                        disabled,
                    },
                    theme.control_height(crate::theme::ControlSize::Medium),
                    window,
                    cx,
                );
                let mut field = div()
                    .id(ElementId::Name(SharedString::from(format!(
                        "genui-field-{}",
                        entry.id
                    ))))
                    .test_support();
                if let Some(handle) = self.focus_handle(cx) {
                    let sink = self.sink.clone();
                    let focused = self.focused.clone();
                    let node_id = entry.id.clone();
                    field = field.on_click(move |_, window, cx| {
                        focus_node(&focused, &sink, &node_id, cx);
                        handle.focus(window, cx);
                    });
                }
                let shell = field.child(shell).into_any_element();
                let _ = (required, &local, &node);
                controls::field(&theme, &entry.id, &label, shell, &display, disabled)
            }
            "Checkbox" => {
                let checked = matches!(self.value_or(entry, Value::Bool(false)), Value::Bool(true));
                let node_id = entry.id.clone();
                if self.focus_handles.is_empty() {
                    self.focus_handles.push(cx.focus_handle());
                }
                self.watch_handles(window, cx);
                let handle = self.focus_handles[0].clone();
                let control = controls::checkbox(
                    &theme,
                    ElementId::Name(SharedString::from(format!("genui-checkbox-{}", entry.id))),
                    &label,
                    checked,
                    disabled,
                    Some(&handle),
                    move |next, _window, cx| {
                        *local.borrow_mut() = Value::Bool(*next);
                        sink.edit(&node_id, Value::Bool(*next), cx);
                        node.update(cx, |_, cx| cx.notify()).ok();
                    },
                );
                controls::field_error(&theme, &entry.id, control, &display, disabled)
            }
            "CheckboxGroup" => {
                let options = props::options(&entry.props, "options");
                let values = match self.value_or(entry, Value::List(Arc::new(Vec::new()))) {
                    Value::List(values) => values
                        .iter()
                        .filter_map(value_text)
                        .collect::<Vec<String>>(),
                    _ => Vec::new(),
                };
                while self.focus_handles.len() < options.len() {
                    self.focus_handles.push(cx.focus_handle());
                }
                self.watch_handles(window, cx);
                let option_values: Vec<String> =
                    options.iter().map(|option| option.value.clone()).collect();
                let option_values_for_event = option_values.clone();
                let values_for_event = values.clone();
                let id = entry.id.clone();
                let on_change = Rc::new(
                    move |index: usize, checked: bool, _w: &mut Window, cx: &mut App| {
                        let Some(value) = option_values_for_event.get(index) else {
                            return;
                        };
                        let mut next: Vec<String> = values_for_event.clone();
                        if checked {
                            if !next.iter().any(|item| item == value) {
                                next.push(value.clone());
                            }
                        } else {
                            next.retain(|item| item != value);
                        }
                        next.sort_by_key(|item| {
                            option_values_for_event
                                .iter()
                                .position(|value| value == item)
                                .unwrap_or(usize::MAX)
                        });
                        let value = Value::List(Arc::new(
                            next.into_iter().map(|item| user_text(&item)).collect(),
                        ));
                        *local.borrow_mut() = value.clone();
                        sink.edit(&id, value, cx);
                        node.update(cx, |_, cx| cx.notify()).ok();
                    },
                );
                let control = controls::checkbox_group(
                    &theme,
                    controls::GroupLayout {
                        id: &entry.id,
                        label: &label,
                        disabled,
                    },
                    &options,
                    &values,
                    &self.focus_handles,
                    on_change,
                );
                controls::field_error(&theme, &entry.id, control, &display, disabled)
            }
            "RadioGroup" => {
                let options = props::options(&entry.props, "options");
                while self.focus_handles.len() < options.len() {
                    self.focus_handles.push(cx.focus_handle());
                }
                self.watch_handles(window, cx);
                let selected = value_text(&self.value_or(entry, Value::Null));
                let option_values: Vec<String> =
                    options.iter().map(|option| option.value.clone()).collect();
                let values_for_event = option_values.clone();
                let id = entry.id.clone();
                let on_change = Rc::new(move |index: usize, _w: &mut Window, cx: &mut App| {
                    let Some(value) = values_for_event.get(index) else {
                        return;
                    };
                    let value = user_text(value);
                    *local.borrow_mut() = value.clone();
                    sink.edit(&id, value, cx);
                    node.update(cx, |_, cx| cx.notify()).ok();
                });
                let group = controls::radio_group(
                    &theme,
                    controls::GroupLayout {
                        id: &entry.id,
                        label: &label,
                        disabled,
                    },
                    &options,
                    selected.as_deref(),
                    &self.focus_handles,
                    on_change.clone(),
                );
                let key_values = option_values;
                let key_handles = self.focus_handles.clone();
                let key_selected = selected.clone();
                let key_id = entry.id.clone();
                let key_sink = self.sink.clone();
                let key_local = self.local.clone();
                let key_node = cx.entity().downgrade();
                let control = div()
                    .id(ElementId::Name(SharedString::from(format!(
                        "genui-radio-key-{}",
                        entry.id
                    ))))
                    .on_key_down(move |event, window, cx| {
                        if disabled {
                            return;
                        }
                        let key = event.keystroke.key.as_str();
                        if !matches!(key, "up" | "down" | "left" | "right") {
                            return;
                        }
                        if key_values.is_empty() {
                            return;
                        }
                        let current = key_selected
                            .as_deref()
                            .and_then(|value| key_values.iter().position(|item| item == value))
                            .unwrap_or(0);
                        let next = if matches!(key, "up" | "left") {
                            (current + key_values.len() - 1) % key_values.len()
                        } else {
                            (current + 1) % key_values.len()
                        };
                        let Some(value) = key_values.get(next) else {
                            return;
                        };
                        let value = user_text(value);
                        *key_local.borrow_mut() = value.clone();
                        key_sink.edit(&key_id, value, cx);
                        if let Some(handle) = key_handles.get(next) {
                            handle.focus(window, cx);
                        }
                        key_node.update(cx, |_, cx| cx.notify()).ok();
                        cx.stop_propagation();
                    })
                    .child(group)
                    .into_any_element();
                controls::field_error(&theme, &entry.id, control, &display, disabled)
            }
            "Switch" => {
                if self.focus_handles.is_empty() {
                    self.focus_handles.push(cx.focus_handle());
                }
                self.watch_handles(window, cx);
                let checked = matches!(self.value_or(entry, Value::Bool(false)), Value::Bool(true));
                let handle = self.focus_handles[0].clone();
                let node_id = entry.id.clone();
                let control = controls::switch(
                    &theme,
                    ElementId::Name(SharedString::from(format!("genui-switch-{}", entry.id))),
                    &label,
                    checked,
                    disabled,
                    &handle,
                    move |next, _window, cx| {
                        *local.borrow_mut() = Value::Bool(next);
                        sink.edit(&node_id, Value::Bool(next), cx);
                        node.update(cx, |_, cx| cx.notify()).ok();
                    },
                );
                controls::field_error(&theme, &entry.id, control, &display, disabled)
            }
            "Button" => {
                let tone = props::name_or(&entry.props, "tone", "secondary");
                let action =
                    props::action(&entry.props, "on_click").or_else(|| self.form_submit_action());
                let disabled = disabled || props::bool_(&entry.props, "disabled", false);
                let node_id = entry.id.clone();
                let focused = self.focused.clone();
                let action = action.unwrap_or_default();
                controls::button(
                    &theme,
                    &entry.id,
                    &label,
                    &tone,
                    disabled,
                    Rc::new(move |_, _, cx| {
                        if !disabled && !action.is_empty() {
                            clear_focus(&focused, &sink, cx);
                            sink.activate(&node_id, &action, cx);
                        }
                    }),
                )
            }
            _ => components::deferred(&theme, &entry.kind),
        }
    }

    fn build_entry(
        &mut self,
        entry: &NodeEntry,
        state: VisualState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.inject_panic && entry.kind == "PanicProbe" {
            panic!("injected renderer panic at {}", entry.id);
        }
        if let Some(builder) = self.extensions.get(&entry.kind) {
            let node = ExtensionNode {
                id: entry.id.clone(),
                kind: entry.kind.clone(),
                role: entry.role,
                props: entry.props.clone(),
                status: entry.status,
            };
            return builder(&node, window, cx);
        }
        if let Some(_milestone) = components::deferred_milestone(&entry.kind) {
            // Lists and tabs have meaningful children; show them under a
            // clearly labeled placeholder so their anatomy remains inspectable.
            if matches!(entry.kind.as_str(), "List" | "Tabs" | "Tab") {
                let width = self.widths.get(&entry.id, self.theme.message_max_width());
                let (children, widths) = self.child_widths(entry, width);
                for (child, width) in children.iter().zip(widths) {
                    self.widths.set(child, width);
                }
                let elements: Vec<AnyElement> = children
                    .iter()
                    .enumerate()
                    .map(|(index, child)| self.child_element(child, index, &entry.kind, window, cx))
                    .collect();
                let mut caption = components::deferred_label(&entry.kind);
                if entry.kind == "Tab"
                    && let Some(label) = crate::props::text(&entry.props, "label")
                {
                    caption = format!("{label} · {}", components::deferred_label("Tab"));
                }
                let style = self.theme.text(TextRole::Caption);
                return div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap(self.theme.space(2))
                    .p(self.theme.space(3))
                    .bg(self.theme.surface_subtle())
                    .border_1()
                    .border_color(self.theme.border())
                    .rounded(self.theme.radius(Radius::Md))
                    .child(
                        div()
                            .text_size(style.size)
                            .line_height(style.line_height)
                            .text_color(self.theme.muted_foreground())
                            .child(caption),
                    )
                    .children(elements)
                    .into_any_element();
            }
            return components::deferred(&self.theme, &entry.kind);
        }
        match entry.kind.as_str() {
            "List" if data::should_virtualize(entry.primary_len()) => {
                self.virtual_list_element(entry, window, cx)
            }
            "List" => {
                let width = self.widths.get(&entry.id, self.theme.message_max_width());
                let (children, widths) = self.child_widths(entry, width);
                for (child, width) in children.iter().zip(widths) {
                    self.widths.set(child, width);
                }
                let elements: Vec<AnyElement> = children
                    .iter()
                    .enumerate()
                    .map(|(index, child)| self.child_element(child, index, &entry.kind, window, cx))
                    .collect();
                let ordered = props::bool_(&entry.props, "ordered", false);
                data::list(&self.theme, ordered, state, elements)
            }
            "Table" => self.table_element(&entry.id, window, cx),
            "Tabs" => self.tabs_element(&entry.id, cx),
            "Tab" => {
                let width = self.widths.get(&entry.id, self.theme.message_max_width());
                let (children, widths) = self.child_widths(entry, width);
                for (child, width) in children.iter().zip(widths) {
                    self.widths.set(child, width);
                }
                let elements: Vec<AnyElement> = children
                    .iter()
                    .enumerate()
                    .map(|(index, child)| self.child_element(child, index, &entry.kind, window, cx))
                    .collect();
                data::tab_panel(&self.theme, &entry.props, state, elements)
            }
            "TableColumn" => div().into_any_element(),
            "Form" => {
                let width = self.widths.get(&entry.id, self.theme.message_max_width());
                let (children, widths) = self.child_widths(entry, width);
                for (child, width) in children.iter().zip(widths) {
                    self.widths.set(child, width);
                }
                let first_submit = children.iter().position(|child| {
                    self.tree.borrow().get(child).is_some_and(|entry| {
                        entry.kind == "Button" && props::bool_(&entry.props, "submit", false)
                    })
                });
                let elements: Vec<AnyElement> = children
                    .iter()
                    .enumerate()
                    .map(|(index, child)| self.child_element(child, index, &entry.kind, window, cx))
                    .collect();
                controls::form(&self.theme, &entry.id, elements, first_submit)
            }
            "Input" | "TextArea" | "Select" | "Checkbox" | "CheckboxGroup" | "RadioGroup"
            | "Switch" | "Button" => self.build_control(entry, state, window, cx),
            "Column" | "Row" | "Grid" | "Card" | "Stat" | "Facts" => {
                let width = self.widths.get(&entry.id, self.theme.message_max_width());
                let (children, widths) = self.child_widths(entry, width);
                for (child, width) in children.iter().zip(widths) {
                    self.widths.set(child, width);
                }
                let kinds: Vec<String> = children
                    .iter()
                    .filter_map(|child| {
                        self.tree
                            .borrow()
                            .get(child)
                            .map(|entry| entry.kind.clone())
                    })
                    .collect();
                let elements: Vec<AnyElement> = children
                    .iter()
                    .enumerate()
                    .map(|(index, child)| self.child_element(child, index, &entry.kind, window, cx))
                    .collect();
                let props = &entry.props;
                match entry.kind.as_str() {
                    "Column" => components::column(&self.theme, props, elements),
                    "Row" => components::row(&self.theme, props, elements, &kinds),
                    "Grid" => components::grid(&self.theme, props, elements, width),
                    "Card" => {
                        let first_is_heading = children.first().is_some_and(|child| {
                            self.tree
                                .borrow()
                                .get(child)
                                .is_some_and(|entry| entry.kind == "Heading")
                        });
                        components::card(&self.theme, props, elements, first_is_heading)
                    }
                    "Stat" => components::stat(&self.theme, props),
                    "Facts" => components::facts(&self.theme, props, width),
                    _ => components::deferred(&self.theme, &entry.kind),
                }
            }
            "Chart" => {
                let width = self.widths.get(&entry.id, self.theme.message_max_width());
                let model = ChartModel::from_props(&entry.props);
                let plot = match self.charts.borrow().get(&entry.id).cloned() {
                    Some(entity) => chart::cached_chart(entity, &self.theme, width),
                    None => chart::plot_element(&self.theme, &model, width, &entry.id, state),
                };
                chart::panel(&self.theme, &model, plot, width)
            }
            "Image" => {
                let width = self.widths.get(&entry.id, self.theme.message_max_width());
                match self.images.borrow().get(&entry.id).cloned() {
                    Some(entity) => cached_image(entity, &self.theme, width, &entry.props),
                    None => {
                        image_element(&self.theme, &entry.props, width, state, &self.image_cache)
                    }
                }
            }
            "Heading" => components::heading(
                &self.theme,
                &crate::props::text_or(&entry.props, "text", ""),
                crate::props::int(&entry.props, "level").unwrap_or(2),
            ),
            "Text" => components::text(&self.theme, &entry.props, state),
            "Badge" => components::badge(
                &self.theme,
                &crate::props::text_or(&entry.props, "text", ""),
                crate::theme::Tone::from_name(&crate::props::name_or(
                    &entry.props,
                    "tone",
                    "neutral",
                )),
                state,
            ),
            "Alert" => components::alert(&self.theme, &entry.id, &entry.props),
            "Divider" => components::divider(&self.theme),
            "Markdown" => components::markdown(
                &self.theme,
                &entry.id,
                &crate::props::text_or(&entry.props, "source", ""),
            ),
            "Code" => components::code(&self.theme, &entry.id, &entry.props),
            _ => components::deferred(&self.theme, &entry.kind),
        }
    }
}

impl Render for NodeView {
    #[allow(refining_impl_trait)]
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let started = Instant::now();
        self.metrics
            .borrow_mut()
            .node_renders
            .entry(self.id.clone())
            .and_modify(|count| *count += 1)
            .or_insert(1);
        let theme = self.theme.clone();
        let tracker = self.panics.clone();
        let meta = self.meta(&self.id);
        let rev = meta.as_ref().map_or(0, |(_, _, rev)| *rev);
        let kind = meta.as_ref().map(|(kind, _, _)| kind.clone());
        let element = match kind.as_deref() {
            Some("Table") => {
                let id = self.id.clone();
                guard(&theme, &id, rev, &tracker, || {
                    self.table_element(&id, window, cx)
                })
            }
            Some("Tabs") => {
                let id = self.id.clone();
                guard(&theme, &id, rev, &tracker, || self.tabs_element(&id, cx))
            }
            Some(_) => match self.entry() {
                Some(entry) => {
                    let status = props::status(&entry.props);
                    let state = data::data_state(self.state(&entry), status.as_deref());
                    let id = self.id.clone();
                    guard(&theme, &id, rev, &tracker, || {
                        self.build_entry(&entry, state, window, cx)
                    })
                }
                None => div().into_any_element(),
            },
            None => div().into_any_element(),
        };
        let element = match self.fades.borrow().get(&self.id).copied() {
            Some(generation) if self.fade => {
                crate::motion::fade_in(&theme, &self.id, generation, element)
            }
            _ => element,
        };
        self.metrics.borrow_mut().node_render_ns += started.elapsed().as_nanos() as u64;
        if crate::components::IMPLEMENTED.contains(&self.kind.as_str())
            && !matches!(
                self.kind.as_str(),
                "Column"
                    | "Row"
                    | "Grid"
                    | "Card"
                    | "Stat"
                    | "Facts"
                    | "Form"
                    | "List"
                    | "Table"
                    | "Tabs"
                    | "Tab"
            )
        {
            element
        } else {
            div().w_full().child(element).into_any_element()
        }
    }
}

pub struct ImageView {
    pub id: String,
    pub tree: Rc<RefCell<RetainedTree>>,
    pub theme: UiTheme,
    pub widths: Widths,
    pub overrides: Rc<StateOverrides>,
    pub metrics: Rc<RefCell<Metrics>>,
    pub image_cache: Rc<RefCell<ImageCache>>,
}

impl Render for ImageView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.metrics.borrow_mut().image_renders += 1;
        let (props, state) = {
            let tree = self.tree.borrow();
            let Some(entry) = tree.get(&self.id) else {
                return div().into_any_element();
            };
            let base = self
                .overrides
                .get(&self.id)
                .copied()
                .unwrap_or_else(|| state_of(entry.status));
            let status = props::status(&entry.props);
            (
                entry.props.clone(),
                data::data_state(base, status.as_deref()),
            )
        };
        let width = self.widths.get(&self.id, self.theme.message_max_width());
        image_element(&self.theme, &props, width, state, &self.image_cache)
    }
}

/// Resolves the host cache plus the node status into the DESIGN pending,
/// failed or ready image (DESIGN §7, SPEC §10.3).
fn image_element(
    theme: &UiTheme,
    props: &Props,
    width: Pixels,
    state: VisualState,
    cache: &Rc<RefCell<ImageCache>>,
) -> AnyElement {
    let src = crate::props::text_or(props, "src", "");
    let alt = crate::props::text_or(props, "alt", "");
    let height = px(
        (crate::props::int(props, "height").unwrap_or(160) as f32).clamp(
            theme.image_min_height().as_f32(),
            theme.image_max_height().as_f32(),
        ),
    );
    let fit = crate::props::name_or(props, "fit", "cover");
    let resolved = {
        let mut cache = cache.borrow_mut();
        cache.request(&src);
        let cached = cache.state(&src);
        match (&cached, state) {
            (_, VisualState::Error) => ImageState::Failed,
            (ImageState::Failed, _) => ImageState::Failed,
            (_, VisualState::Loading | VisualState::Refreshing) => ImageState::Pending,
            (state, _) => state.clone(),
        }
    };
    crate::image::element(theme, &alt, height, width, &fit, &resolved, state)
}

/// Wraps an image in the fixed-size cache boundary required by SPEC §10.1.
pub fn cached_image(
    entity: Entity<ImageView>,
    theme: &UiTheme,
    width: Pixels,
    props: &Props,
) -> AnyElement {
    let height = (crate::props::int(props, "height").unwrap_or(160) as f32).clamp(
        theme.image_min_height().as_f32(),
        theme.image_max_height().as_f32(),
    );
    let style = div()
        .w(px(width.as_f32().max(0.0)))
        .h(px(height))
        .style()
        .clone();
    AnyView::from(entity).cached(style).into_any_element()
}

/// Builds `UiView` after `gpui_kit::init`, with the renderer's capabilities.
pub fn validation_options() -> genui_core::validate::Options {
    UiView::capabilities().validation_options()
}

#[cfg(test)]
mod tests {
    use super::*;
    use genui_core::{
        ir::{NodeStatus, Role},
        patch::NodeInit,
    };
    use serde_json::json;

    fn node(id: &str, kind: &str) -> NodeInit {
        NodeInit {
            id: id.into(),
            kind: kind.into(),
            role: Role::Group,
            props: BTreeMap::new(),
            status: NodeStatus::Sealed,
        }
    }

    #[test]
    fn cached_image_height_is_clamped_to_design_bounds() {
        let theme = UiTheme::for_mode(crate::theme::ThemeMode::Light);
        assert_eq!(theme.image_min_height().as_f32(), 40.0);
        assert_eq!(theme.image_max_height().as_f32(), 480.0);
        let props: Props = serde_json::from_value(json!({"height": 9999})).expect("props");
        let height = (crate::props::int(&props, "height").unwrap_or(160) as f32).clamp(
            theme.image_min_height().as_f32(),
            theme.image_max_height().as_f32(),
        );
        assert_eq!(height, 480.0);
    }

    #[test]
    fn root_can_be_a_leaf_and_still_have_an_entity_kind() {
        let mut tree = RetainedTree::default();
        tree.apply(&PatchBatch {
            epoch: 1,
            seq: 0,
            ops: vec![
                Op::Reset,
                Op::Insert {
                    parent: None,
                    slot: "root".into(),
                    index: 0,
                    node: node("root", "Text"),
                },
            ],
        });
        assert_eq!(tree.root.as_deref(), Some("root"));
        assert!(!tree.get("root").unwrap().is_container());
    }
}
