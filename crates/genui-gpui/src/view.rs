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
    data::{self, TabsView},
    event::RendererEvent,
    image::{ImageCache, ImageError, ImageLimits, ImageState},
    layout::{StateOverrides, VisualState, Widths, inset_width, row_cell_width, state_of},
    metrics::{Metrics, PatchSummary},
    panics::{PanicTracker, guard},
    props::Props,
    table::{self, GenuiTableDelegate, TableModel},
    theme::{Radius, TextRole, UiTheme},
    tree::{ApplyReport, NodeEntry, RetainedTree},
};
use genui_core::{
    diag::Diagnostic,
    patch::{Op, PatchBatch},
};
use gpui_kit::{
    AnyElement, AnyView, AppContext, Context, Entity, EventEmitter, ImageFormat, IntoElement,
    ParentElement, Pixels, Render, StatefulInteractiveElement, Styled, WeakEntity, Window,
    component::table::TableState, div, px,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    time::Instant,
};

type TableEntity = Entity<gpui_kit::component::table::TableState<GenuiTableDelegate>>;
type TableMap = Rc<RefCell<BTreeMap<String, TableEntity>>>;
type TableModelCache = Rc<RefCell<BTreeMap<String, (u64, u64, u64, TableModel)>>>;
type TabsMap = Rc<RefCell<BTreeMap<String, Entity<TabsView>>>>;
type ListMap = Rc<RefCell<BTreeMap<String, Entity<crate::data::ListView>>>>;

pub struct UiView {
    theme: UiTheme,
    tree: Rc<RefCell<RetainedTree>>,
    containers: Rc<RefCell<BTreeMap<String, Entity<NodeView>>>>,
    charts: Rc<RefCell<BTreeMap<String, Entity<ChartView>>>>,
    images: Rc<RefCell<BTreeMap<String, Entity<ImageView>>>>,
    tables: TableMap,
    table_models: TableModelCache,
    tabs: TabsMap,
    lists: ListMap,
    image_cache: Rc<RefCell<ImageCache>>,
    panics: Rc<RefCell<PanicTracker>>,
    fades: Rc<RefCell<BTreeMap<String, u64>>>,
    widths: Widths,
    overrides: Rc<StateOverrides>,
    metrics: Rc<RefCell<Metrics>>,
    queue: VecDeque<PatchBatch>,
    diagnostics: Vec<Diagnostic>,
    needs_snapshot: bool,
    max_width: Option<Pixels>,
    panic_injection: bool,
    self_view: Option<WeakEntity<UiView>>,
}

impl UiView {
    /// SPEC §10.1. `window` is accepted for signature parity with later input
    /// state construction; M6 creates no kit input state.
    pub fn new(theme: UiTheme, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            theme,
            tree: Rc::new(RefCell::new(RetainedTree::default())),
            containers: Rc::new(RefCell::new(BTreeMap::new())),
            charts: Rc::new(RefCell::new(BTreeMap::new())),
            images: Rc::new(RefCell::new(BTreeMap::new())),
            tables: Rc::new(RefCell::new(BTreeMap::new())),
            table_models: Rc::new(RefCell::new(BTreeMap::new())),
            tabs: Rc::new(RefCell::new(BTreeMap::new())),
            lists: Rc::new(RefCell::new(BTreeMap::new())),
            image_cache: Rc::new(RefCell::new(ImageCache::default())),
            panics: Rc::new(RefCell::new(PanicTracker::default())),
            fades: Rc::new(RefCell::new(BTreeMap::new())),
            widths: Widths::default(),
            overrides: Rc::new(StateOverrides::default()),
            metrics: Rc::new(RefCell::new(Metrics::default())),
            queue: VecDeque::new(),
            diagnostics: Vec::new(),
            needs_snapshot: false,
            max_width: None,
            panic_injection: false,
            self_view: Some(cx.entity().downgrade()),
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

    pub fn capabilities() -> Capabilities {
        Capabilities::m8()
    }

    /// `true` when the build uses unwinding and builder panics can be
    /// contained (SPEC §10.8). A `panic = "abort"` host should surface this in
    /// its startup diagnostics.
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
    /// its builder so the containment path can be exercised end to end. The
    /// kind is not in the std catalog, so model programs cannot reach it.
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

    pub fn table_row_count(&self, id: &str, cx: &gpui_kit::App) -> Option<usize> {
        let entity = self.tables.borrow().get(id).cloned()?;
        Some(entity.read(cx).delegate().rows())
    }

    /// The rendered row range of a virtualized table (SPEC §10.6).
    pub fn table_visible_range(&self, id: &str, cx: &gpui_kit::App) -> Option<(usize, usize)> {
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

    /// Rows whose `NodeView` entities have been created on demand.
    pub fn list_created_rows(&self, id: &str, cx: &gpui_kit::App) -> Option<usize> {
        self.lists
            .borrow()
            .get(id)
            .map(|list| list.read(cx).created.get())
    }

    /// Rows built in the most recent List frame.
    pub fn list_frame_rows(&self, id: &str, cx: &gpui_kit::App) -> Option<usize> {
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
    pub fn apply(&mut self, batch: PatchBatch, _window: &mut Window, cx: &mut Context<Self>) {
        self.apply_batch(batch, cx);
    }

    /// Fallback for callers without a window. Drained once at the next render.
    pub fn enqueue(&mut self, batch: PatchBatch, cx: &mut Context<Self>) {
        self.metrics.borrow_mut().batches_enqueued += 1;
        self.queue.push_back(batch);
        cx.notify();
    }

    fn apply_batch(&mut self, batch: PatchBatch, cx: &mut Context<Self>) {
        let started = Instant::now();
        // Keep only the parent links and subtree ids needed by `sync_views`.
        // Cloning the whole retained tree would copy a 50,000-row table prop on
        // every batch (SPEC §10.6 bounded-memory stress).
        let mut parents: BTreeMap<String, Option<String>> = BTreeMap::new();
        let mut removed: BTreeMap<String, Vec<String>> = BTreeMap::new();
        {
            let tree = self.tree.borrow();
            for op in &batch.ops {
                match op {
                    Op::Remove { id } => {
                        let mut ids = Vec::new();
                        collect_ids(&tree, id, &mut ids);
                        removed.insert(id.clone(), ids);
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
        self.sync_views(&batch, &parents, &removed, &report, cx);
    }

    fn sync_views(
        &mut self,
        batch: &PatchBatch,
        parents: &BTreeMap<String, Option<String>>,
        removed: &BTreeMap<String, Vec<String>>,
        report: &ApplyReport,
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
                    self.ensure_entity(&node.id, cx);
                    if let Some(parent) = parent {
                        self.notify_for(parent, cx);
                    } else {
                        self.notify_root(cx);
                    }
                }
                Op::SetProps { id, .. } | Op::AppendText { id, .. } | Op::SetStatus { id, .. } => {
                    self.notify_for(id, cx);
                }
                Op::Remove { id } => {
                    let parent = parents.get(id).cloned().flatten();
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
            self.ensure_entity(id, cx);
        }
        if report.needs_snapshot {
            cx.notify();
        }
    }

    fn ensure_entity(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(entry) = self.tree.borrow().get(id).cloned() else {
            return;
        };
        let is_root = self.tree.borrow().root.as_deref() == Some(id);
        if is_root || entry.is_container() {
            let mut containers = self.containers.borrow_mut();
            if containers.contains_key(id) {
                return;
            }
            let view = cx.new(|_| NodeView {
                id: id.to_string(),
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
                view: self.self_view.clone(),
                inject_panic: self.panic_injection,
                fade: true,
            });
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

impl Render for UiView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let started = Instant::now();
        if !self.queue.is_empty() {
            self.metrics.borrow_mut().queue_drains += 1;
            while let Some(batch) = self.queue.pop_front() {
                self.apply_batch(batch, cx);
            }
        }
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
            .size_full()
            .bg(self.theme.background())
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
    pub view: Option<WeakEntity<UiView>>,
    pub inject_panic: bool,
    pub fade: bool,
}

/// Creates leaf `NodeView` entities on demand for the virtualized List. Only
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
    pub view: Option<WeakEntity<UiView>>,
    pub inject_panic: bool,
}

impl NodeFactory {
    pub(crate) fn entity<C: AppContext>(&self, id: &str, cx: &mut C) -> Option<Entity<NodeView>> {
        if let Some(entity) = self.containers.borrow().get(id).cloned() {
            return Some(entity);
        }
        self.tree.borrow().get(id)?;
        let view = cx.new(|_| NodeView {
            id: id.to_string(),
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
            view: self.view.clone(),
            inject_panic: self.inject_panic,
            fade: false,
        });
        self.containers
            .borrow_mut()
            .insert(id.to_string(), view.clone());
        Some(view)
    }
}

impl NodeView {
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
            view: self.view.clone(),
            inject_panic: self.inject_panic,
        }
    }

    fn state(&self, entry: &NodeEntry) -> VisualState {
        self.overrides
            .get(&entry.id)
            .copied()
            .unwrap_or_else(|| state_of(entry.status))
    }

    fn data_state(&self, entry: &NodeEntry) -> VisualState {
        data::data_state(
            self.state(entry),
            crate::props::status(&entry.props).as_deref(),
        )
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
        &self,
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
        let state = self.data_state(&entry);
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
                .child(element)
                .into_any_element();
        }
        let _ = status;
        let generation = self.fades.borrow().get(id).copied();
        if let Some(generation) = generation {
            element = crate::motion::fade_in(&self.theme, id, generation, element);
        }
        element
    }

    fn build_entry(
        &self,
        entry: &NodeEntry,
        state: VisualState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.inject_panic && entry.kind == "PanicProbe" {
            panic!("injected renderer panic at {}", entry.id);
        }
        if let Some(_milestone) = components::deferred_milestone(&entry.kind) {
            // Forms and inputs have meaningful children; show them under a
            // clearly labeled placeholder so their anatomy remains inspectable.
            if entry.kind == "Form" {
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
                let caption = components::deferred_label(&entry.kind);
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
            "Column" | "Row" | "Grid" | "Card" | "Stat" | "Facts" | "Tab" => {
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
                    "Tab" => data::tab_panel(&self.theme, props, state, elements),
                    _ => components::deferred(&self.theme, &entry.kind),
                }
            }
            "List" if crate::data::should_virtualize(entry.primary_children().len()) => {
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
                let ordered = crate::props::bool_(&entry.props, "ordered", false);
                data::list(&self.theme, ordered, self.data_state(entry), elements)
            }
            "Table" => self.table_element(&entry.id, window, cx),
            "Tabs" => self.tabs_element(&entry.id, cx),
            "TableColumn" => div().into_any_element(),
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
            "Button" => self.button_element(entry, state),
            "Markdown" => components::markdown(
                &self.theme,
                &entry.id,
                &crate::props::text_or(&entry.props, "source", ""),
            ),
            "Code" => components::code(&self.theme, &entry.id, &entry.props),
            _ => components::deferred(&self.theme, &entry.kind),
        }
    }

    fn button_element(&self, entry: &NodeEntry, state: VisualState) -> AnyElement {
        let button = components::button(&self.theme, &entry.id, &entry.props, state);
        let actionable = entry.props.contains_key("on_click")
            && !crate::props::bool_(&entry.props, "disabled", false)
            && state != VisualState::Partial
            && state != VisualState::Disabled;
        if actionable && let Some(view) = self.view.clone() {
            let path = entry.id.clone();
            return button
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |_, cx| {
                        cx.emit(RendererEvent::Activate {
                            path: path.clone(),
                            prop: "on_click".into(),
                        })
                    });
                })
                .into_any_element();
        }
        button.into_any_element()
    }

    fn table_element(&self, id: &str, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
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
                if std::env::var_os("GENUI_TABLE_DEBUG").is_some() {
                    eprintln!("table parse {} rows in {:?}", model.rows.len(), elapsed);
                }
                // The parsed model is now the single owner of the row data.
                // Drop the duplicate wire JSON from the retained tree so a
                // 50,000-row table does not keep two copies (SPEC §10.6).
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
        let started = Instant::now();
        let element = table::element(&table, &theme, &model, inside_card);
        if std::env::var_os("GENUI_TABLE_DEBUG").is_some() {
            eprintln!(
                "table element {} rows in {:?}",
                model.rows.len(),
                started.elapsed()
            );
        }
        element
    }

    fn virtual_list_element(
        &self,
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

    fn tabs_element(&self, id: &str, cx: &mut Context<Self>) -> AnyElement {
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
                        self.view.clone(),
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
}

impl EventEmitter<RendererEvent> for UiView {}

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
            Some(_) => match self.tree.borrow().get(&self.id).cloned() {
                Some(entry) => {
                    let state = self.data_state(&entry);
                    guard(&theme, &self.id, rev, &tracker, || {
                        self.build_entry(&entry, state, window, cx)
                    })
                }
                None => div().into_any_element(),
            },
            None => div().into_any_element(),
        };
        let generation = self.fades.borrow().get(&self.id).copied();
        let element = match generation {
            Some(generation) if self.fade => {
                crate::motion::fade_in(&theme, &self.id, generation, element)
            }
            _ => element,
        };
        self.metrics.borrow_mut().node_render_ns += started.elapsed().as_nanos() as u64;
        div().w_full().child(element).into_any_element()
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
            (
                entry.props.clone(),
                data::data_state(base, crate::props::status(&entry.props).as_deref()),
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
