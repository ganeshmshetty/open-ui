//! `List` and `Tabs`/`Tab` (DESIGN §§6, 7; SPEC §10.3).
//!
//! Lists render their IR children with the DESIGN marker gutter. Tabs keep a
//! local selection, render only the selected panel, and support Left/Right,
//! Home and End when focused. A bound `Tabs` selection also emits
//! `UiEvent::Edit`, so the host can persist it in state (SPEC §7.8).

use crate::{
    event::RendererEvent,
    layout::{StateOverrides, VisualState, Widths},
    props::{self, Props},
    theme::{Radius, TextRole, UiTheme},
    tree::RetainedTree,
};
use gpui_kit::{
    AnyElement, AnyView, AppContext, Context, Entity, FocusHandle, FontWeight, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, Render, ScrollStrategy, SharedString,
    StatefulInteractiveElement, Styled, WeakEntity, Window,
    component::{
        IndexPath,
        list::{List, ListDelegate, ListItem, ListState},
    },
    div,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
};

/// A marker gutter (DESIGN §7: 16 px) followed by the item body.
pub fn list(
    theme: &UiTheme,
    ordered: bool,
    state: VisualState,
    items: Vec<AnyElement>,
) -> AnyElement {
    if items.is_empty() {
        if state == VisualState::Loading {
            return crate::motion::skeleton_rows(theme, 3, theme.list_row_min_height());
        }
        // DESIGN §6 rule 14: an empty List renders nothing.
        return div().into_any_element();
    }
    let items_len = items.len();
    let mut column = div()
        .flex()
        .flex_col()
        .w_full()
        .gap(theme.list_item_gap())
        .children(
            items
                .into_iter()
                .enumerate()
                .map(|(index, item)| list_row(theme, ordered, index, items_len, item)),
        );
    if state == VisualState::Refreshing {
        column = column.opacity(theme.refreshing_opacity());
    }
    if state == VisualState::Error {
        column = column.child(error_caption(theme));
    }
    column.into_any_element()
}

/// The 16 px marker gutter (dot or ordered number) plus the item body.
pub fn list_row(
    theme: &UiTheme,
    ordered: bool,
    index: usize,
    total_items: usize,
    item: AnyElement,
) -> AnyElement {
    let marker: AnyElement = if ordered {
        let style = theme.text(TextRole::Small);
        div()
            .w(theme.list_number_gutter(total_items))
            .flex_shrink_0()
            .text_align(gpui_kit::TextAlign::Right)
            .text_size(style.size)
            .line_height(style.line_height)
            .text_color(theme.muted_foreground())
            .child(SharedString::from(format!("{}", index + 1)))
            .into_any_element()
    } else {
        div()
            .w(theme.list_gutter_width())
            .h(theme.list_row_min_height())
            .flex_shrink_0()
            .flex()
            .items_center()
            .child(
                div()
                    .w(theme.list_marker_size())
                    .h(theme.list_marker_size())
                    .rounded(theme.radius(Radius::Full))
                    .bg(theme.muted_foreground()),
            )
            .into_any_element()
    };
    div()
        .flex()
        .flex_row()
        .items_start()
        .w_full()
        .gap(theme.space(1))
        .child(marker)
        .child(div().flex_1().min_w(theme.space(0)).child(item))
        .into_any_element()
}

/// Above this many children the List uses the kit virtual list (SPEC §10.3).
pub const VIRTUAL_LIST_THRESHOLD: usize = 100;

pub fn should_virtualize(children: usize) -> bool {
    children > VIRTUAL_LIST_THRESHOLD
}

#[derive(Clone, Debug)]
struct ListModel {
    items: Arc<Vec<String>>,
    ordered: bool,
    status: Option<String>,
}

/// The kit list adapter over retained IR children. Only `render_item` calls
/// create a leaf `NodeView`; the kit virtual list calls it for visible rows.
pub struct GenuiListDelegate {
    model: ListModel,
    theme: UiTheme,
    factory: crate::view::NodeFactory,
    created: Rc<Cell<usize>>,
    frame_rendered: Rc<Cell<usize>>,
    item_width: gpui_kit::Pixels,
    selected: Option<usize>,
}

impl GenuiListDelegate {
    fn new(
        model: ListModel,
        theme: UiTheme,
        factory: crate::view::NodeFactory,
        created: Rc<Cell<usize>>,
        frame_rendered: Rc<Cell<usize>>,
    ) -> Self {
        let item_width = theme.message_max_width();
        Self {
            model,
            theme,
            factory,
            created,
            frame_rendered,
            item_width,
            selected: None,
        }
    }

    fn update(&mut self, model: ListModel) {
        self.model = model;
    }
}

impl ListDelegate for GenuiListDelegate {
    type Item = ListItem;

    fn items_count(&self, _section: usize, _cx: &gpui_kit::App) -> usize {
        self.model.items.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        let id = self.model.items.get(ix.row)?.clone();
        self.factory.widths.set(&id, self.item_width);
        let existed = self.factory.containers.borrow().contains_key(&id);
        let entity = self.factory.entity(&id, cx)?;
        if !existed {
            self.created.set(self.created.get().saturating_add(1));
        }
        self.frame_rendered
            .set(self.frame_rendered.get().saturating_add(1));
        let row = list_row(
            &self.theme,
            self.model.ordered,
            ix.row,
            self.model.items.len(),
            AnyView::from(entity).into_any_element(),
        );
        Some(
            ListItem::new(gpui_kit::ElementId::Name(SharedString::from(format!(
                "genui-list-item-{}-{}",
                self.model.items.get(ix.row)?,
                ix.row
            ))))
            .p(self.theme.space(0))
            .pb(self.theme.list_item_gap())
            .child(row),
        )
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) {
        self.selected = ix.map(|ix| ix.row);
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        // DESIGN §6 rule 14: an empty List renders nothing.
        div()
    }

    fn loading(&self, _cx: &gpui_kit::App) -> bool {
        matches!(
            self.model.status.as_deref(),
            Some("requested") | Some("loading")
        )
    }

    fn render_loading(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        crate::motion::skeleton_rows(&self.theme, 3, self.theme.list_row_height())
    }
}

/// The virtualized List entity. It owns the kit `ListState` and rebuilds the
/// delegate model when the retained List revision changes.
pub struct ListView {
    pub id: String,
    pub tree: Rc<RefCell<RetainedTree>>,
    pub theme: UiTheme,
    pub widths: Widths,
    pub state: Entity<ListState<GenuiListDelegate>>,
    pub created: Rc<Cell<usize>>,
    pub frame_rendered: Rc<Cell<usize>>,
    pub last_rev: u64,
    model: ListModel,
}

impl ListView {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: String,
        tree: Rc<RefCell<RetainedTree>>,
        theme: UiTheme,
        widths: Widths,
        factory: crate::view::NodeFactory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (model, rev) = read_list_model(&tree, &id);
        let created = Rc::new(Cell::new(0));
        let frame_rendered = Rc::new(Cell::new(0));
        let delegate = GenuiListDelegate::new(
            model.clone(),
            theme.clone(),
            factory,
            created.clone(),
            frame_rendered.clone(),
        );
        let state = cx.new(|cx| ListState::new(delegate, window, cx).selectable(false));
        Self {
            id,
            tree,
            theme,
            widths,
            state,
            created,
            frame_rendered,
            last_rev: rev,
            model,
        }
    }

    pub fn scroll_to_row(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let ix = IndexPath {
            section: 0,
            row,
            column: 0,
        };
        self.state.update(cx, |state, cx| {
            state.scroll_to_item(ix, ScrollStrategy::Top, window, cx);
        });
    }
}

fn read_list_model(tree: &Rc<RefCell<RetainedTree>>, id: &str) -> (ListModel, u64) {
    let tree = tree.borrow();
    let Some(entry) = tree.get(id) else {
        return (
            ListModel {
                items: Arc::new(Vec::new()),
                ordered: false,
                status: None,
            },
            0,
        );
    };
    (
        ListModel {
            items: Arc::new(entry.primary_children()),
            ordered: props::bool_(&entry.props, "ordered", false),
            status: props::status(&entry.props),
        },
        entry.rev,
    )
}

impl Render for ListView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rev = self
            .tree
            .borrow()
            .get(&self.id)
            .map(|entry| entry.rev)
            .unwrap_or(0);
        if rev != self.last_rev {
            let (model, current_rev) = read_list_model(&self.tree, &self.id);
            self.last_rev = current_rev;
            self.state.update(cx, |state, _| {
                state.delegate_mut().update(model.clone());
            });
            self.model = model;
            cx.notify();
        }
        let model = &self.model;
        self.frame_rendered.set(0);
        let width = self.widths.get(&self.id, self.theme.message_max_width());
        let item_width = gpui_kit::px(
            (width.as_f32()
                - self.theme.list_number_gutter(model.items.len()).as_f32()
                - self.theme.space(1).as_f32())
            .max(0.0),
        );
        self.state.update(cx, |state, _| {
            state.delegate_mut().item_width = item_width;
        });
        let loading = matches!(model.status.as_deref(), Some("requested") | Some("loading"));
        let refreshing = matches!(model.status.as_deref(), Some("refreshing"));
        let failed = matches!(model.status.as_deref(), Some("failed") | Some("denied"));
        let mut list = div()
            .flex()
            .flex_col()
            .w(width)
            .h(self.theme.list_viewport_height())
            .child(
                List::new(&self.state)
                    .scrollbar_visible(true)
                    .max_h(self.theme.list_viewport_height()),
            );
        if refreshing {
            list = list.opacity(self.theme.refreshing_opacity());
        }
        if failed {
            list = list.child(error_caption(&self.theme));
        }
        if loading && model.items.is_empty() {
            return div()
                .w(width)
                .child(crate::motion::skeleton_rows(
                    &self.theme,
                    3,
                    self.theme.list_row_height(),
                ))
                .into_any_element();
        }
        list.into_any_element()
    }
}

pub fn error_caption(theme: &UiTheme) -> AnyElement {
    let style = theme.text(TextRole::Caption);
    div()
        .text_size(style.size)
        .line_height(style.line_height)
        .text_color(theme.tone(crate::theme::Tone::Danger).fg)
        .child("Couldn't load data")
        .into_any_element()
}

/// The visual state derived from the node's synthetic status prop.
pub fn data_state(base: VisualState, status: Option<&str>) -> VisualState {
    match status {
        Some("requested") | Some("loading") => VisualState::Loading,
        Some("refreshing") => VisualState::Refreshing,
        Some("failed") | Some("denied") => VisualState::Error,
        _ => base,
    }
}

/// One tab's label and value (`value` defaults to the label, SPEC §8.8).
#[derive(Clone, Debug, PartialEq)]
pub struct TabSpec {
    pub id: String,
    pub label: String,
    pub value: String,
}

pub fn tab_specs(tree: &RetainedTree, ids: &[String]) -> Vec<TabSpec> {
    ids.iter()
        .filter_map(|id| {
            let entry = tree.get(id)?;
            let label = props::text_or(&entry.props, "label", "");
            let value = props::text(&entry.props, "value").unwrap_or_else(|| label.clone());
            Some(TabSpec {
                id: id.clone(),
                label,
                value,
            })
        })
        .collect()
}

/// The tabs bar + selected panel. Only the selected `Tab` entity is mounted.
pub struct TabsView {
    pub id: String,
    pub tree: Rc<RefCell<RetainedTree>>,
    pub containers: Rc<RefCell<BTreeMap<String, Entity<crate::view::NodeView>>>>,
    pub theme: UiTheme,
    pub widths: Widths,
    pub overrides: Rc<StateOverrides>,
    pub view: Option<WeakEntity<crate::view::UiView>>,
    pub focus: FocusHandle,
    pub selected: usize,
    pub last_count: usize,
    pub last_rev: u64,
}

impl TabsView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        tree: Rc<RefCell<RetainedTree>>,
        containers: Rc<RefCell<BTreeMap<String, Entity<crate::view::NodeView>>>>,
        theme: UiTheme,
        widths: Widths,
        overrides: Rc<StateOverrides>,
        view: Option<WeakEntity<crate::view::UiView>>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            id,
            tree,
            containers,
            theme,
            widths,
            overrides,
            view,
            focus: cx.focus_handle().tab_stop(true),
            selected: 0,
            last_count: 0,
            last_rev: 0,
        }
    }

    fn tab_ids(&self) -> Vec<String> {
        self.tree
            .borrow()
            .get(&self.id)
            .map(|entry| entry.children_in("tabs"))
            .unwrap_or_default()
    }

    fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        let count = self.tab_ids().len();
        if count == 0 {
            return;
        }
        let next = index.min(count - 1);
        if next != self.selected {
            self.selected = next;
            let value = tab_specs(&self.tree.borrow(), &self.tab_ids())
                .get(next)
                .map(|tab| tab.value.clone());
            if let (Some(value), Some(view)) = (value, self.view.as_ref())
                && let Some(entry) = self.tree.borrow().get(&self.id).cloned()
                && entry.props.contains_key("bind")
            {
                let _ = view.update(cx, |_, cx| {
                    cx.emit(RendererEvent::Edit {
                        path: self.id.clone(),
                        value: genui_core::value::Value::from(value),
                    })
                });
            }
            cx.notify();
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let count = self.tab_ids().len();
        if count == 0 {
            return;
        }
        match event.keystroke.key.as_str() {
            "left" => self.select(self.selected.saturating_sub(1), cx),
            "right" => self.select((self.selected + 1).min(count - 1), cx),
            "home" => self.select(0, cx),
            "end" => self.select(count - 1, cx),
            _ => {}
        }
    }

    fn bar(&self, specs: &[TabSpec], cx: &mut Context<Self>) -> AnyElement {
        let style = self.theme.text(TextRole::Body);
        let tabs = specs.iter().enumerate().map(|(index, spec)| {
            let selected = index == self.selected;
            let color = if selected {
                self.theme.foreground()
            } else {
                self.theme.muted_foreground()
            };
            let underline = if selected {
                self.theme.accent()
            } else {
                self.theme.background()
            };
            div()
                .id(SharedString::from(format!("genui-tab-{}-{index}", self.id)))
                .flex()
                .items_center()
                .h(self.theme.tab_bar_height())
                .px(self.theme.space(2))
                .text_size(style.size)
                .line_height(style.line_height)
                .font_weight(FontWeight::MEDIUM)
                .text_color(color)
                .border_b(self.theme.tab_underline_height())
                .border_color(underline)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| this.select(index, cx)))
                .child(spec.label.clone())
        });
        div()
            .flex()
            .flex_row()
            .items_end()
            .gap(self.theme.tab_gap())
            .w_full()
            .border_b_1()
            .border_color(self.theme.border())
            .children(tabs)
            .into_any_element()
    }
}

impl Render for TabsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ids = self.tab_ids();
        if ids.len() != self.last_count {
            self.last_count = ids.len();
            self.selected = self.selected.min(ids.len().saturating_sub(1));
        }
        let specs = tab_specs(&self.tree.borrow(), &ids);
        let bar = self.bar(&specs, cx);
        let width = self.widths.get(&self.id, self.theme.message_max_width());
        let selected = ids.get(self.selected).cloned();
        let panel: AnyElement =
            match selected.and_then(|id| self.containers.borrow().get(&id).cloned()) {
                Some(entity) => AnyView::from(entity).into_any_element(),
                None => div().into_any_element(),
            };
        div()
            .flex()
            .flex_col()
            .w(width)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .child(bar)
            .child(div().pt(self.theme.tab_panel_gap()).w_full().child(panel))
    }
}

/// A `Tab` panel body. It is only mounted by its parent `TabsView`.
pub fn tab_panel(
    theme: &UiTheme,
    props: &Props,
    state: VisualState,
    children: Vec<AnyElement>,
) -> AnyElement {
    let _ = props;
    let mut body = div()
        .flex()
        .flex_col()
        .w_full()
        .gap(theme.space(2))
        .children(children);
    if state == VisualState::Refreshing {
        body = body.opacity(theme.refreshing_opacity());
    }
    body.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeMode;
    use genui_core::{
        ir::{NodeStatus, Role},
        patch::{NodeInit, Op, PatchBatch},
    };
    use serde_json::json;

    fn node(id: &str, kind: &str, props: serde_json::Value) -> NodeInit {
        NodeInit {
            id: id.into(),
            kind: kind.into(),
            role: Role::Group,
            props: serde_json::from_value(props).unwrap_or_default(),
            status: NodeStatus::Sealed,
        }
    }

    fn tabs_fixture() -> RetainedTree {
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
                    node: node("root", "Tabs", json!({"bind": {"bind": "$tab"}})),
                },
                Op::Insert {
                    parent: Some("root".into()),
                    slot: "tabs".into(),
                    index: 0,
                    node: node(
                        "root/tabs:#0",
                        "Tab",
                        json!({"label": "Overview", "value": "overview"}),
                    ),
                },
                Op::Insert {
                    parent: Some("root".into()),
                    slot: "tabs".into(),
                    index: 1,
                    node: node("root/tabs:#1", "Tab", json!({"label": "Orders"})),
                },
            ],
        });
        tree
    }

    #[test]
    fn tab_values_default_to_labels_and_keep_order() {
        let tree = tabs_fixture();
        let specs = tab_specs(&tree, &tree.get("root").unwrap().children_in("tabs"));
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].value, "overview");
        assert_eq!(specs[1].value, "Orders");
    }

    #[test]
    fn list_virtualization_threshold_is_strictly_above_one_hundred() {
        assert!(!should_virtualize(0));
        assert!(!should_virtualize(100));
        assert!(should_virtualize(101));
        assert!(should_virtualize(10_000));
    }

    #[test]
    fn data_status_maps_to_visual_states() {
        assert_eq!(
            data_state(VisualState::Normal, Some("requested")),
            VisualState::Loading
        );
        assert_eq!(
            data_state(VisualState::Normal, Some("refreshing")),
            VisualState::Refreshing
        );
        assert_eq!(
            data_state(VisualState::Normal, Some("failed")),
            VisualState::Error
        );
        assert_eq!(data_state(VisualState::Normal, None), VisualState::Normal);
    }

    #[test]
    fn theme_has_list_and_tab_tokens() {
        let theme = UiTheme::for_mode(ThemeMode::Light);
        assert_eq!(theme.list_gutter_width(), crate::theme::test_px(16.0));
        assert_eq!(theme.tab_underline_height(), crate::theme::test_px(2.0));
        assert_eq!(theme.fade_in_ms(), 150);
    }
}
