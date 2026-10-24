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
    layout::{
        StateOverrides, VisualState, Widths, data_state, inset_width, row_cell_width, state_of,
    },
    metrics::{Metrics, PatchSummary},
    props::Props,
    theme::{Radius, TextRole, UiTheme},
    tree::{ApplyReport, NodeEntry, RetainedTree},
};
use genui_core::{
    diag::Diagnostic,
    patch::{Op, PatchBatch},
};
use gpui_kit::{
    AnyElement, AnyView, AppContext, Context, Entity, IntoElement, ParentElement, Pixels, Render,
    Styled, Window, div, px,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    time::Instant,
};

pub struct UiView {
    theme: UiTheme,
    tree: Rc<RefCell<RetainedTree>>,
    containers: Rc<RefCell<BTreeMap<String, Entity<NodeView>>>>,
    charts: Rc<RefCell<BTreeMap<String, Entity<ChartView>>>>,
    images: Rc<RefCell<BTreeMap<String, Entity<ImageView>>>>,
    widths: Widths,
    overrides: Rc<StateOverrides>,
    metrics: Rc<RefCell<Metrics>>,
    queue: VecDeque<PatchBatch>,
    diagnostics: Vec<Diagnostic>,
    needs_snapshot: bool,
    max_width: Option<Pixels>,
}

impl UiView {
    /// SPEC §10.1. `window` is accepted for signature parity with later input
    /// state construction; M6 creates no kit input state.
    pub fn new(theme: UiTheme, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _ = cx;
        Self {
            theme,
            tree: Rc::new(RefCell::new(RetainedTree::default())),
            containers: Rc::new(RefCell::new(BTreeMap::new())),
            charts: Rc::new(RefCell::new(BTreeMap::new())),
            images: Rc::new(RefCell::new(BTreeMap::new())),
            widths: Widths::default(),
            overrides: Rc::new(StateOverrides::default()),
            metrics: Rc::new(RefCell::new(Metrics::default())),
            queue: VecDeque::new(),
            diagnostics: Vec::new(),
            needs_snapshot: false,
            max_width: None,
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
        Capabilities::m6()
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
        let before = self.tree.borrow().clone();
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
        self.sync_views(&batch, &before, &report, cx);
    }

    fn sync_views(
        &mut self,
        batch: &PatchBatch,
        before: &RetainedTree,
        report: &ApplyReport,
        cx: &mut Context<Self>,
    ) {
        for op in &batch.ops {
            match op {
                Op::Reset => {
                    self.containers.borrow_mut().clear();
                    self.charts.borrow_mut().clear();
                    self.images.borrow_mut().clear();
                    self.widths.clear();
                    cx.notify();
                }
                Op::Insert {
                    node, parent, slot, ..
                } => {
                    let _ = slot;
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
                    let parent = before.get(id).and_then(|entry| entry.parent.clone());
                    self.prune_entities(id, before);
                    if let Some(parent) = parent {
                        self.notify_for(&parent, cx);
                    } else {
                        cx.notify();
                    }
                }
                Op::Move { id, parent, .. } => {
                    let old_parent = before.get(id).and_then(|entry| entry.parent.clone());
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
                widths: self.widths.clone(),
                overrides: self.overrides.clone(),
                metrics: self.metrics.clone(),
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
                });
                images.insert(id.to_string(), view);
            }
            _ => {}
        }
    }

    fn prune_entities(&self, id: &str, before: &RetainedTree) {
        let mut ids = vec![id.to_string()];
        collect_ids(before, id, &mut ids);
        let mut containers = self.containers.borrow_mut();
        let mut charts = self.charts.borrow_mut();
        let mut images = self.images.borrow_mut();
        for id in ids {
            containers.remove(&id);
            charts.remove(&id);
            images.remove(&id);
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
    pub widths: Widths,
    pub overrides: Rc<StateOverrides>,
    pub metrics: Rc<RefCell<Metrics>>,
}

impl NodeView {
    fn state(&self, entry: &NodeEntry) -> VisualState {
        self.overrides
            .get(&self.id)
            .copied()
            .unwrap_or_else(|| data_state(&entry.props).unwrap_or_else(|| state_of(entry.status)))
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
            _ => vec![width; children.len()],
        };
        (children, widths)
    }

    fn child_element(
        &self,
        id: &str,
        index: usize,
        parent: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(entry) = self.tree.borrow().get(id).cloned() else {
            return div().into_any_element();
        };
        let state =
            self.overrides.get(id).copied().unwrap_or_else(|| {
                data_state(&entry.props).unwrap_or_else(|| state_of(entry.status))
            });
        // Containers are entities so a change notifies only their subtree
        // (SPEC §10.1). Leaves are built inline by their parent.
        let mut element = if let Some(entity) = self.containers.borrow().get(id).cloned() {
            AnyView::from(entity).into_any_element()
        } else if entry.kind == "Chart" {
            let width = self.widths.get(id, self.theme.message_max_width());
            let model = ChartModel::from_props(&entry.props);
            let plot = match self.charts.borrow().get(id).cloned() {
                Some(entity) => chart::cached_chart(entity, &self.theme, width),
                None => chart::plot_element(&self.theme, &model, width, id, state),
            };
            chart::panel(&self.theme, &model, plot, width)
        } else if entry.kind == "Image" {
            let width = self.widths.get(id, self.theme.message_max_width());
            match self.images.borrow().get(id).cloned() {
                Some(entity) => cached_image(entity, &self.theme, width, &entry.props),
                None => components::image(&self.theme, &entry.props, width, state),
            }
        } else {
            self.build_entry(&entry, state, cx)
        };
        if entry.kind == "Heading" && index > 0 {
            element = div()
                .mt(self.theme.heading_extra_top())
                .child(element)
                .into_any_element();
        }
        // A Badge or Button keeps its natural width in a stretching stack
        // (DESIGN §6 rules 4 and 7).
        if matches!(parent, "Column" | "Card" | "Form" | "List" | "Tabs" | "Tab")
            && matches!(entry.kind.as_str(), "Badge" | "Button")
        {
            element = div().self_start().child(element).into_any_element();
        }
        element
    }

    fn build_entry(
        &self,
        entry: &NodeEntry,
        state: VisualState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(_milestone) = components::deferred_milestone(&entry.kind) {
            // Forms, lists and tabs have meaningful children; show them under a
            // clearly labeled placeholder so their anatomy remains inspectable.
            if matches!(entry.kind.as_str(), "Form" | "List" | "Tabs" | "Tab") {
                let width = self.widths.get(&entry.id, self.theme.message_max_width());
                let (children, widths) = self.child_widths(entry, width);
                for (child, width) in children.iter().zip(widths) {
                    self.widths.set(child, width);
                }
                let elements: Vec<AnyElement> = children
                    .iter()
                    .enumerate()
                    .map(|(index, child)| self.child_element(child, index, &entry.kind, cx))
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
                    .map(|(index, child)| self.child_element(child, index, &entry.kind, cx))
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
                    None => components::image(&self.theme, &entry.props, width, state),
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
            "Button" => components::button(&self.theme, &entry.props, state),
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let started = Instant::now();
        self.metrics
            .borrow_mut()
            .node_renders
            .entry(self.id.clone())
            .and_modify(|count| *count += 1)
            .or_insert(1);
        let element = match self.tree.borrow().get(&self.id).cloned() {
            Some(entry) => {
                let state = self.state(&entry);
                self.build_entry(&entry, state, cx)
            }
            None => div().into_any_element(),
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
}

impl Render for ImageView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.metrics.borrow_mut().image_renders += 1;
        let (props, state) = {
            let tree = self.tree.borrow();
            let Some(entry) = tree.get(&self.id) else {
                return div().into_any_element();
            };
            (
                entry.props.clone(),
                self.overrides.get(&self.id).copied().unwrap_or_else(|| {
                    data_state(&entry.props).unwrap_or_else(|| state_of(entry.status))
                }),
            )
        };
        let width = self.widths.get(&self.id, self.theme.message_max_width());
        components::image(&self.theme, &props, width, state)
    }
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
