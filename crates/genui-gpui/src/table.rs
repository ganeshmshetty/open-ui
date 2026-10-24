//! `Table` over the pinned kit `DataTable` (DESIGN §§5, 6.10, 7; SPEC §10.3).
//!
//! The model is parsed once per node revision and width. The kit table owns
//! row virtualization, the sticky header, sorting, selection and resizable
//! columns, so a 50,000-row table builds only the visible rows (SPEC §10.6).
//! Cell text is sanitized and bounded before it reaches a widget.

use crate::{
    format::{self, Format, Scalar},
    layout::VisualState,
    props,
    theme::{Radius, TextRole, UiTheme},
    tree::NodeEntry,
};
use gpui_kit::{
    AnyView, App, AppContext, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Pixels, SharedString, StatefulInteractiveElement, Styled, TextAlign, Window,
    component::{
        Sizable, Size,
        table::{Column, ColumnSort, DataTable, TableDelegate, TableState},
    },
    div,
};
use serde_json::Value;
use std::sync::Arc;

/// A hard row cap for the stress case. The core's `max_list_len` may be
/// configured lower; this is the renderer's own allocation guard (§10.8).
pub const MAX_TABLE_ROWS: usize = 50_000;
/// No catalog table has more than a handful of columns; cap hostile input.
pub const MAX_TABLE_COLUMNS: usize = 32;
/// A table cell is a short label, not a paragraph.
pub const MAX_CELL_BYTES: usize = 4 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct TableColumnSpec {
    pub header: String,
    pub field: String,
    pub align_end: bool,
    pub format: Format,
    pub currency: String,
}

impl TableColumnSpec {
    pub fn numeric(&self) -> bool {
        !matches!(self.format, Format::Text | Format::Date)
    }

    fn from_node(entry: &NodeEntry) -> Option<Self> {
        let header = props::text(&entry.props, "header")?;
        let field = props::text(&entry.props, "field")?;
        let align_end = props::name_or(&entry.props, "align", "start") == "end";
        let format = Format::from_name(&props::name_or(&entry.props, "format", "text"));
        let currency = props::text_or(&entry.props, "currency", "USD");
        Some(Self {
            header: truncate_cell(&header),
            field,
            align_end,
            format,
            currency,
        })
    }
}

fn truncate_cell(text: &str) -> String {
    let text = props::sanitize_text(text);
    if text.len() <= MAX_CELL_BYTES {
        return text;
    }
    let mut end = MAX_CELL_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

/// A parsed table. Rows keep the tagged wire map shape so formatting happens
/// once per visible cell, not during parsing.
#[derive(Clone, Debug)]
pub struct TableModel {
    pub columns: Arc<Vec<TableColumnSpec>>,
    pub rows: Arc<Vec<Vec<(String, Value)>>>,
    pub empty: String,
    pub status: Option<String>,
    pub rows_truncated: bool,
}

impl TableModel {
    fn column_specs(entry: &NodeEntry, columns: &[NodeEntry]) -> Vec<TableColumnSpec> {
        let mut specs: Vec<TableColumnSpec> = columns
            .iter()
            .filter_map(TableColumnSpec::from_node)
            .take(MAX_TABLE_COLUMNS)
            .collect();
        if specs.is_empty() {
            // A malformed program can still reach the renderer through a
            // snapshot. Fall back to the first row's keys rather than a blank.
            specs = props::table_rows(&entry.props, "rows")
                .0
                .first()
                .map(|row| {
                    row.iter()
                        .take(MAX_TABLE_COLUMNS)
                        .map(|(key, _)| TableColumnSpec {
                            header: truncate_cell(key),
                            field: key.clone(),
                            align_end: false,
                            format: Format::Text,
                            currency: "USD".into(),
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        specs
    }

    pub fn from_entry(entry: &NodeEntry, columns: &[NodeEntry]) -> Self {
        let specs = Self::column_specs(entry, columns);
        let (rows, rows_truncated) = props::table_rows(&entry.props, "rows");
        let empty = truncate_cell(&props::text_or(&entry.props, "empty", "No data"));
        Self {
            columns: Arc::new(specs),
            rows: Arc::new(rows),
            empty,
            status: props::status(&entry.props),
            rows_truncated,
        }
    }

    /// Rebuilds the column specs and status while keeping already parsed rows.
    /// A resize or an unrelated prop update must not re-read 50,000 rows.
    pub fn rebuild(&self, entry: &NodeEntry, columns: &[NodeEntry]) -> Self {
        Self {
            columns: Arc::new(Self::column_specs(entry, columns)),
            rows: self.rows.clone(),
            empty: truncate_cell(&props::text_or(&entry.props, "empty", "No data")),
            status: props::status(&entry.props),
            rows_truncated: self.rows_truncated,
        }
    }

    pub fn visual_state(&self, base: VisualState) -> VisualState {
        match self.status.as_deref() {
            Some("requested") | Some("refreshing") | Some("loading") => VisualState::Loading,
            Some("failed") | Some("denied") => VisualState::Error,
            _ if self.rows.is_empty() => VisualState::Empty,
            _ => base,
        }
    }

    pub fn truncated_display(&self) -> Option<(usize, usize)> {
        self.rows_truncated
            .then_some((self.rows.len(), self.rows.len().saturating_add(1)))
    }

    pub fn signature(&self, width: gpui_kit::Pixels) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        width.as_f32().to_bits().hash(&mut hasher);
        self.columns.len().hash(&mut hasher);
        for column in self.columns.iter() {
            column.header.hash(&mut hasher);
            column.field.hash(&mut hasher);
            column.align_end.hash(&mut hasher);
            (column.format as u8).hash(&mut hasher);
            column.currency.hash(&mut hasher);
        }
        self.rows.len().hash(&mut hasher);
        // Rows are immutable for a given revision; a pointer is a sufficient
        // change detector here and avoids hashing 50,000 rows on every frame.
        Arc::as_ptr(&self.rows).hash(&mut hasher);
        self.empty.hash(&mut hasher);
        self.status.hash(&mut hasher);
        self.rows_truncated.hash(&mut hasher);
        hasher.finish()
    }

    pub fn cell_text(&self, row: usize, col: usize) -> String {
        let Some(spec) = self.columns.get(col) else {
            return String::new();
        };
        let Some(entries) = self.rows.get(row) else {
            return String::new();
        };
        let value = props::map_get(entries, &spec.field).unwrap_or(&Value::Null);
        truncate_cell(&format::format_scalar(
            &Scalar::from_wire(value),
            spec.format,
            &spec.currency,
        ))
    }

    fn sort_value(&self, row: usize, col: usize) -> (bool, f64, String) {
        let Some(spec) = self.columns.get(col) else {
            return (true, 0.0, String::new());
        };
        let Some(entries) = self.rows.get(row) else {
            return (true, 0.0, String::new());
        };
        let value = props::map_get(entries, &spec.field).unwrap_or(&Value::Null);
        match format::numeric(value) {
            Some(number) => (false, number, String::new()),
            None => (true, 0.0, format::text_of(value).to_lowercase()),
        }
    }

    fn sorted_order(&self, sort: Option<(usize, ColumnSort)>) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.rows.len()).collect();
        let Some((col, direction)) = sort else {
            return order;
        };
        if matches!(direction, ColumnSort::Default) {
            return order;
        }
        order.sort_by(|a, b| {
            let (a_text, a_num, a_str) = self.sort_value(*a, col);
            let (b_text, b_num, b_str) = self.sort_value(*b, col);
            let ordering = match (a_text, b_text) {
                (false, false) => a_num
                    .partial_cmp(&b_num)
                    .unwrap_or(std::cmp::Ordering::Equal),
                (true, true) => a_str.cmp(&b_str),
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
            };
            match direction {
                ColumnSort::Descending => ordering.reverse(),
                _ => ordering,
            }
        });
        order
    }

    /// Column widths for the assigned table width. Numeric columns stay
    /// narrower; totals under the width are stretched proportionally.
    pub fn column_widths(&self, width: gpui_kit::Pixels, theme: &UiTheme) -> Vec<gpui_kit::Pixels> {
        let count = self.columns.len().max(1);
        let min = theme.table_min_column_width().as_f32();
        let max = theme.table_max_column_width().as_f32();
        let weights: Vec<f32> = self
            .columns
            .iter()
            .enumerate()
            .map(|(index, column)| {
                let mut weight = if column.numeric() { 1.0 } else { 1.3 };
                if index == 0 {
                    weight += 0.2;
                }
                weight
            })
            .collect();
        let total_weight: f32 = weights.iter().sum::<f32>().max(0.01);
        let available = width.as_f32().max(0.0);
        let mut widths: Vec<f32> = weights
            .iter()
            .map(|weight| (available * weight / total_weight).clamp(min, max))
            .collect();
        // If clamped widths are narrower than the table, hand the remainder
        // back proportionally so the header never floats over dead space.
        let sum: f32 = widths.iter().sum();
        if sum < available && sum > 0.0 {
            let scale = available / sum;
            for value in &mut widths {
                *value = (*value * scale).min(max);
            }
        }
        if count == 0 {
            return Vec::new();
        }
        widths.into_iter().map(gpui_kit::px).collect()
    }

    pub fn viewport_height(&self, theme: &UiTheme) -> gpui_kit::Pixels {
        let loading = matches!(
            self.status.as_deref(),
            Some("requested") | Some("refreshing") | Some("loading")
        );
        theme.table_height(self.rows.len(), loading)
    }
}

/// The kit table adapter. It owns only formatted data and sort state; the
/// entities and virtualization live in `TableState`.
pub struct GenuiTableDelegate {
    pub model: Arc<TableModel>,
    pub order: Vec<usize>,
    pub sort: Option<(usize, ColumnSort)>,
    pub width: gpui_kit::Pixels,
    pub theme: UiTheme,
    pub node: String,
    pub signature: u64,
}

impl GenuiTableDelegate {
    pub fn new(model: TableModel, width: gpui_kit::Pixels, theme: UiTheme, node: String) -> Self {
        let signature = model.signature(width);
        let model = Arc::new(model);
        Self {
            order: model.sorted_order(None),
            model,
            sort: None,
            width,
            theme,
            node,
            signature,
        }
    }

    pub fn update(&mut self, model: TableModel, width: gpui_kit::Pixels, theme: UiTheme) -> bool {
        let signature = model.signature(width);
        if signature == self.signature && self.width == width {
            return false;
        }
        self.model = Arc::new(model);
        self.width = width;
        self.theme = theme;
        self.signature = signature;
        self.order = self.model.sorted_order(self.sort);
        true
    }

    pub fn rows(&self) -> usize {
        self.model.rows.len()
    }

    pub fn cell_text(&self, row_ix: usize, col_ix: usize) -> String {
        let row = self.order.get(row_ix).copied().unwrap_or(row_ix);
        self.model.cell_text(row, col_ix)
    }
}

impl TableDelegate for GenuiTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.model.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.model.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        let Some(spec) = self.model.columns.get(col_ix) else {
            return Column::new("", "");
        };
        let width = self
            .model
            .column_widths(self.width, &self.theme)
            .get(col_ix)
            .copied()
            .unwrap_or_else(|| self.theme.table_min_column_width());
        let mut column = Column::new(
            SharedString::from(spec.field.clone()),
            SharedString::from(spec.header.clone()),
        )
        .width(width)
        .min_width(self.theme.table_min_column_width())
        .max_width(self.theme.table_max_column_width())
        .sortable()
        .resizable(true)
        .movable(false)
        .paddings(gpui_kit::Edges {
            top: self.theme.table_cell_padding_y(),
            bottom: self.theme.table_cell_padding_y(),
            left: self.theme.table_cell_padding_x(),
            right: self.theme.table_cell_padding_x(),
        });
        if spec.align_end || spec.numeric() {
            column = column.text_right();
        }
        column
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        self.sort = if matches!(sort, ColumnSort::Default) {
            None
        } else {
            Some((col_ix, sort))
        };
        self.order = self.model.sorted_order(self.sort);
        cx.notify();
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some(spec) = self.model.columns.get(col_ix) else {
            return div().into_any_element();
        };
        let style = self.theme.text(TextRole::Caption);
        let mut cell = div()
            .w_full()
            .truncate()
            .text_size(style.size)
            .line_height(style.line_height)
            .font_weight(FontWeight::MEDIUM)
            .text_color(self.theme.muted_foreground())
            .child(spec.header.clone());
        if spec.numeric() || spec.align_end {
            cell = cell.text_align(TextAlign::Right);
        }
        cell.into_any_element()
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let text = self.cell_text(row_ix, col_ix);
        let spec = self.model.columns.get(col_ix);
        let style = self.theme.text(TextRole::Body);
        let mut color = self.theme.foreground();
        if let Some(spec) = spec
            && spec.format == Format::Delta
        {
            let row = self.order.get(row_ix).copied().unwrap_or(row_ix);
            let value = self
                .model
                .rows
                .get(row)
                .and_then(|entries| props::map_get(entries, &spec.field))
                .map(Scalar::from_wire)
                .unwrap_or(Scalar::Null);
            if let Some(number) = value.as_f64() {
                color = if number > 0.0 {
                    self.theme.tone(crate::theme::Tone::Success).fg
                } else if number < 0.0 {
                    self.theme.tone(crate::theme::Tone::Danger).fg
                } else {
                    self.theme.foreground()
                };
            }
        }
        let numeric = spec.is_some_and(|spec| spec.numeric() || spec.align_end);
        let tooltip = text.clone();
        let show_tooltip = props::sanitize_text(&tooltip) != text || !text.is_empty();
        let mut cell = div()
            .id(SharedString::from(format!(
                "genui-table-{}-{row_ix}-{col_ix}",
                self.node
            )))
            .w_full()
            .truncate()
            .text_size(style.size)
            .line_height(style.line_height)
            .text_color(color)
            .child(text);
        if numeric {
            cell = cell.text_align(TextAlign::Right);
        }
        if show_tooltip {
            let theme = self.theme.clone();
            cell = cell.tooltip(move |_window, cx| {
                let view = cx.new(|_| crate::components::TextTooltip {
                    text: tooltip.clone(),
                    theme: theme.clone(),
                });
                AnyView::from(view)
            });
        }
        cell.into_any_element()
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let style = self.theme.text(TextRole::Body);
        let color = if matches!(
            self.model.status.as_deref(),
            Some("failed") | Some("denied")
        ) {
            self.theme.tone(crate::theme::Tone::Danger).fg
        } else {
            self.theme.muted_foreground()
        };
        div()
            .flex()
            .items_center()
            .justify_center()
            .w_full()
            .h(self.theme.table_empty_height())
            .text_size(style.size)
            .line_height(style.line_height)
            .text_color(color)
            .child(self.model.empty.clone())
            .into_any_element()
    }

    fn loading(&self, _: &App) -> bool {
        matches!(
            self.model.status.as_deref(),
            Some("requested") | Some("refreshing") | Some("loading")
        )
    }

    fn render_loading(
        &mut self,
        _size: Size,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        crate::motion::skeleton_rows(
            &self.theme,
            self.theme.table_loading_rows(),
            self.theme.table_row_height(),
        )
    }

    fn cell_text(&self, row_ix: usize, col_ix: usize, _: &App) -> String {
        GenuiTableDelegate::cell_text(self, row_ix, col_ix)
    }
}

/// A ready-to-render kit table. `state` must already have been created with
/// the current delegate and refreshed after delegate updates.
pub fn element(
    state: &gpui_kit::Entity<TableState<GenuiTableDelegate>>,
    theme: &UiTheme,
    model: &TableModel,
    inside_card: bool,
) -> gpui_kit::AnyElement {
    let height = model.viewport_height(theme);
    let table = DataTable::new(state)
        .with_size(Size::Medium)
        .header_row_height(theme.table_header_height())
        .bordered(!inside_card)
        .scrollbar_visible(true, true);
    let mut wrapper = div().w_full().h(height).overflow_hidden().child(table);
    if !inside_card {
        wrapper = wrapper
            .rounded(theme.radius(Radius::Xl))
            .border_1()
            .border_color(theme.border());
    }
    wrapper.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{theme::ThemeMode, tree::RetainedTree};
    use genui_core::{
        ir::{NodeStatus, Role},
        patch::{NodeInit, Op, PatchBatch},
    };
    use serde_json::json;

    fn node(id: &str, kind: &str, props: Value) -> NodeInit {
        NodeInit {
            id: id.into(),
            kind: kind.into(),
            role: Role::Group,
            props: serde_json::from_value(props).unwrap_or_default(),
            status: NodeStatus::Sealed,
        }
    }

    fn table_fixture(row_count: usize) -> (TableModel, UiTheme) {
        let rows: Vec<Value> = (0..row_count)
            .map(|index| {
                json!({"map": [
                    ["name", format!("Item {index}")],
                    ["qty", {"i": index}],
                    ["price", {"f": index as f64 + 0.5}]
                ]})
            })
            .collect();
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
                    node: node("root", "Table", json!({"rows": rows, "empty": "Nothing"})),
                },
                Op::Insert {
                    parent: Some("root".into()),
                    slot: "columns".into(),
                    index: 0,
                    node: node(
                        "root/columns:#0",
                        "TableColumn",
                        json!({"header": "Name", "field": "name"}),
                    ),
                },
                Op::Insert {
                    parent: Some("root".into()),
                    slot: "columns".into(),
                    index: 1,
                    node: node(
                        "root/columns:#1",
                        "TableColumn",
                        json!({"header": "Qty", "field": "qty", "format": "number"}),
                    ),
                },
                Op::Insert {
                    parent: Some("root".into()),
                    slot: "columns".into(),
                    index: 2,
                    node: node(
                        "root/columns:#2",
                        "TableColumn",
                        json!({"header": "Price", "field": "price", "format": "currency"}),
                    ),
                },
            ],
        });
        let entry = tree.get("root").unwrap();
        let columns: Vec<NodeEntry> = entry
            .children_in("columns")
            .iter()
            .filter_map(|id| tree.get(id).cloned())
            .collect();
        (
            TableModel::from_entry(entry, &columns),
            UiTheme::for_mode(ThemeMode::Light),
        )
    }

    #[test]
    fn table_model_formats_and_sorts_cells() {
        let (model, theme) = table_fixture(3);
        assert_eq!(model.columns.len(), 3);
        assert_eq!(model.cell_text(0, 0), "Item 0");
        assert_eq!(model.cell_text(1, 1), "1");
        assert_eq!(model.cell_text(1, 2), "$1.50");
        let mut delegate = GenuiTableDelegate::new(
            model,
            crate::theme::test_px(600.0),
            theme.clone(),
            "root".into(),
        );
        delegate.sort = Some((1, ColumnSort::Descending));
        delegate.order = delegate.model.sorted_order(delegate.sort);
        assert_eq!(delegate.cell_text(0, 0), "Item 2");
        assert_eq!(delegate.cell_text(2, 0), "Item 0");
    }

    #[test]
    fn table_cell_text_strips_bidi_and_zero_width_characters() {
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
                    node: node(
                        "root",
                        "Table",
                        json!({"rows": [{"map": [["name", "a\u{202e}b\u{200b}c"]]}]}),
                    ),
                },
                Op::Insert {
                    parent: Some("root".into()),
                    slot: "columns".into(),
                    index: 0,
                    node: node(
                        "root/columns:#0",
                        "TableColumn",
                        json!({"header": "Name", "field": "name"}),
                    ),
                },
            ],
        });
        let entry = tree.get("root").unwrap();
        let model = TableModel::from_entry(entry, &[tree.get("root/columns:#0").unwrap().clone()]);
        assert_eq!(model.cell_text(0, 0), "abc");
    }

    #[test]
    fn table_model_caps_rows_and_sanitizes_cells() {
        let rows: Vec<Value> = (0..MAX_TABLE_ROWS + 5)
            .map(|index| json!({"map": [["name", format!("r{index}")]]}))
            .collect();
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
                    node: node("root", "Table", json!({"rows": rows})),
                },
                Op::Insert {
                    parent: Some("root".into()),
                    slot: "columns".into(),
                    index: 0,
                    node: node(
                        "root/columns:#0",
                        "TableColumn",
                        json!({"header": "Name\u{202e}", "field": "name"}),
                    ),
                },
            ],
        });
        let entry = tree.get("root").unwrap();
        let columns: Vec<NodeEntry> = vec![tree.get("root/columns:#0").unwrap().clone()];
        let model = TableModel::from_entry(entry, &columns);
        assert_eq!(model.rows.len(), MAX_TABLE_ROWS);
        assert!(model.rows_truncated);
        assert_eq!(model.columns[0].header, "Name");
    }

    #[test]
    fn table_grid_height_is_bounded_and_empty_keeps_its_size() {
        let (model, theme) = table_fixture(3);
        assert_eq!(model.viewport_height(&theme), crate::theme::test_px(128.0));
        let (empty, theme) = table_fixture(0);
        assert_eq!(
            empty.viewport_height(&theme),
            crate::theme::test_px(
                theme.table_header_height().as_f32() + theme.table_empty_height().as_f32()
            )
        );
        let (many, theme) = table_fixture(20);
        assert_eq!(
            many.viewport_height(&theme),
            crate::theme::test_px(13.0 * 32.0),
            "more rows than the natural limit still cap below the viewport token"
        );
    }

    #[test]
    fn status_prop_selects_loading_and_error_states() {
        let (mut model, _) = table_fixture(2);
        assert_eq!(model.visual_state(VisualState::Normal), VisualState::Normal);
        model.status = Some("requested".into());
        assert_eq!(
            model.visual_state(VisualState::Normal),
            VisualState::Loading
        );
        model.status = Some("failed".into());
        assert_eq!(model.visual_state(VisualState::Normal), VisualState::Error);
    }
}
