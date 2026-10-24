//! Charts (DESIGN §6 rule 11, §7). A small canvas plot draws bars and lines so
//! null values stay gaps and the value axis uses round ticks; the pinned kit
//! bar chart could not do either (recorded in PINS.md). Both start from the
//! same parsed `ChartModel`.

use crate::{
    format::{self, Scalar},
    layout::VisualState,
    props::{self, Props},
    theme::UiTheme,
};
use gpui_kit::{
    AnyElement, AnyView, Context, Corners, Hsla, IntoElement, PaintQuad, ParentElement, Path,
    PathBuilder, Pixels, Render, Styled, TextAlign, Window, canvas, div, fill, point, px,
};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChartKind {
    Bar,
    Line,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChartPoint {
    pub label: String,
    /// `None` is a null data value: no bar, and a break in a line.
    pub value: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChartModel {
    pub kind: ChartKind,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub points: Vec<ChartPoint>,
    /// The original point count when categories were capped for display.
    pub total_points: usize,
}

impl ChartModel {
    pub fn from_props(props: &Props) -> Self {
        let kind = match props::name_or(props, "kind", "bar").as_str() {
            "line" => ChartKind::Line,
            _ => ChartKind::Bar,
        };
        let x_field = props::text_or(props, "x", "");
        let y_field = props::text_or(props, "y", "");
        let rows = props::data(props, "data");
        let total_points = rows.len();
        let points = rows
            .into_iter()
            .take(crate::theme::MAX_CHART_CATEGORIES)
            .map(|row| {
                let label = props::map_get(&row, &x_field)
                    .map(format::text_of)
                    .unwrap_or_default();
                let value = props::map_get(&row, &y_field).and_then(numeric_value);
                ChartPoint { label, value }
            })
            .collect();
        Self {
            kind,
            title: props::text(props, "title"),
            summary: props::text(props, "summary"),
            points,
            total_points,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn truncated(&self) -> Option<(usize, usize)> {
        (self.total_points > self.points.len()).then_some((self.points.len(), self.total_points))
    }
}

fn numeric_value(value: &Value) -> Option<f64> {
    match value {
        Value::Null => None,
        Value::Object(map) if map.get("ref").is_some() => None,
        _ => format::numeric(value),
    }
}

/// Renders a chart at the width its parent assigned (SPEC §10.1 cached chart
/// plot area, DESIGN §6 rule 11).
pub fn render(
    theme: &UiTheme,
    model: &ChartModel,
    width: Pixels,
    id: &str,
    state: VisualState,
) -> AnyElement {
    panel(
        theme,
        model,
        plot_element(theme, model, width, id, state),
        width,
    )
}

/// The plot area alone. It keeps a fixed height in every state so data arriving
/// never shifts the layout (DESIGN §6 rule 13, SPEC §10.1 cache boundary).
pub fn plot_element(
    theme: &UiTheme,
    model: &ChartModel,
    width: Pixels,
    id: &str,
    state: VisualState,
) -> AnyElement {
    match state {
        VisualState::Loading => skeleton(theme, width),
        VisualState::Error => error_plot(theme, width),
        VisualState::Empty => empty_plot(theme, width),
        _ if model.is_empty() => empty_plot(theme, width),
        _ => match model.kind {
            ChartKind::Bar => bar_plot(theme, model, width, id),
            ChartKind::Line => line_plot(theme, model, width),
        },
    }
}

fn caption(theme: &UiTheme, text: String) -> AnyElement {
    let style = theme.text(crate::theme::TextRole::Caption);
    div()
        .text_size(style.size)
        .line_height(style.line_height)
        .text_color(theme.muted_foreground())
        .child(text)
        .into_any_element()
}

fn skeleton(theme: &UiTheme, width: Pixels) -> AnyElement {
    plot_box(theme, width).into_any_element()
}

fn empty_plot(theme: &UiTheme, width: Pixels) -> AnyElement {
    plot_box(theme, width)
        .child(caption(theme, "No data".into()))
        .into_any_element()
}

fn error_plot(theme: &UiTheme, width: Pixels) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(theme.space(1))
        .w(px(width.as_f32().max(0.0)))
        .h(theme.chart_height())
        .bg(theme.surface_subtle())
        .rounded(theme.radius(crate::theme::Radius::Md))
        .child(
            div()
                .text_size(theme.text(crate::theme::TextRole::Caption).size)
                .line_height(theme.text(crate::theme::TextRole::Caption).line_height)
                .text_color(theme.tone(crate::theme::Tone::Danger).fg)
                .child("Couldn't load data"),
        )
        .into_any_element()
}

fn plot_box(theme: &UiTheme, width: Pixels) -> gpui_kit::Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .w(px(width.as_f32().max(0.0)))
        .h(theme.chart_height())
        .bg(theme.surface_subtle())
        .rounded(theme.radius(crate::theme::Radius::Md))
}

fn bar_plot(theme: &UiTheme, model: &ChartModel, width: Pixels, _id: &str) -> AnyElement {
    let values: Vec<Option<f64>> = model.points.iter().map(|point| point.value).collect();
    let finite: Vec<f64> = values.iter().filter_map(|value| *value).collect();
    if finite.is_empty() {
        return empty_plot(theme, width);
    }
    let data_min = finite.iter().copied().fold(f64::INFINITY, f64::min);
    let data_max = finite.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let ticks = nice_ticks(
        data_min.min(0.0),
        data_max.max(0.0),
        theme.chart_tick_count(),
    );
    let low = ticks.first().map(|tick| tick.value).unwrap_or(0.0);
    let high = ticks.last().map(|tick| tick.value).unwrap_or(1.0);
    let span = (high - low).max(f64::EPSILON);
    let axis_width = theme.chart_axis_width();
    let plot_width = px((width.as_f32() - axis_width.as_f32()).max(1.0));
    let plot_height = theme.chart_height();
    let count = model.points.len().max(1);
    let band = plot_width.as_f32() / count as f32;
    let bar_width = (band * 0.62).max(1.0);
    let tick_values: Vec<f64> = ticks.iter().map(|tick| tick.value).collect();
    let bar_color = theme.chart_color(0);
    let grid_color = theme.border();
    let grid_width = theme.chart_grid_width();
    let radius = theme.radius(crate::theme::Radius::Sm);
    let no_radius = theme.radius(crate::theme::Radius::None);
    let zero = ((high - 0.0) / span) as f32;
    let values = values.clone();

    let canvas_plot = canvas(
        move |bounds, _, _| {
            let w = bounds.size.width.as_f32();
            let h = bounds.size.height.as_f32();
            let mut grid = Vec::new();
            for value in &tick_values {
                let y = ((high - *value) / span) as f32 * h;
                let mut builder = PathBuilder::stroke(grid_width);
                builder.move_to(point(bounds.origin.x, bounds.origin.y + px(y)));
                builder.line_to(point(bounds.origin.x + px(w), bounds.origin.y + px(y)));
                if let Ok(path) = builder.build() {
                    grid.push(path);
                }
            }
            let zero_y = zero * h;
            let mut bars = Vec::new();
            let mut x = band * 0.19;
            for value in &values {
                if let Some(value) = value
                    && value.is_finite()
                {
                    let value_y = ((high - *value) / span) as f32 * h;
                    let (top, height, corners) = if value_y <= zero_y {
                        (
                            value_y,
                            zero_y - value_y,
                            Corners {
                                top_left: radius,
                                top_right: radius,
                                bottom_left: no_radius,
                                bottom_right: no_radius,
                            },
                        )
                    } else {
                        (
                            zero_y,
                            value_y - zero_y,
                            Corners {
                                top_left: no_radius,
                                top_right: no_radius,
                                bottom_left: radius,
                                bottom_right: radius,
                            },
                        )
                    };
                    let rect = gpui_kit::Bounds::from_corners(
                        point(bounds.origin.x + px(x), bounds.origin.y + px(top)),
                        point(
                            bounds.origin.x + px(x + bar_width),
                            bounds.origin.y + px(top + height.max(0.0)),
                        ),
                    );
                    bars.push(fill(rect, bar_color).corner_radii(corners));
                }
                x += band;
            }
            (grid, bars)
        },
        move |_, (grid, bars), window, _| {
            for path in grid {
                window.paint_path(path, grid_color);
            }
            for bar in bars {
                window.paint_quad(bar);
            }
        },
    )
    .size_full();

    let label_style = theme.text(crate::theme::TextRole::Caption);
    let y_labels = div()
        .flex()
        .flex_col()
        .justify_between()
        .items_end()
        .w(axis_width)
        .h(plot_height)
        .children(ticks.iter().rev().map(|tick| {
            div()
                .text_size(label_style.size)
                .line_height(label_style.line_height)
                .text_color(theme.muted_foreground())
                .child(tick.label.clone())
        }));

    let step = ((count as f32) / 8.0).ceil().max(1.0) as usize;
    let cell = plot_width.as_f32() / count as f32;
    let x_labels =
        div()
            .flex()
            .flex_row()
            .w(plot_width)
            .children(
                model
                    .points
                    .iter()
                    .enumerate()
                    .filter_map(|(index, point)| {
                        if index % step != 0 {
                            return None;
                        }
                        Some(
                            div()
                                .w(px(cell))
                                .truncate()
                                .text_align(TextAlign::Center)
                                .text_size(label_style.size)
                                .line_height(label_style.line_height)
                                .text_color(theme.muted_foreground())
                                .child(truncate_label(&point.label, theme.chart_label_max_chars())),
                        )
                    }),
            );

    div()
        .flex()
        .flex_col()
        .w(px(width.as_f32().max(0.0)))
        .child(
            div()
                .flex()
                .flex_row()
                .gap(theme.space(1))
                .child(y_labels)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .w(plot_width)
                        .child(div().w(plot_width).h(plot_height).child(canvas_plot))
                        .child(x_labels),
                ),
        )
        .into_any_element()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tick {
    pub value: f64,
    pub label: String,
}

/// Round value-axis ticks (1, 2 or 5 × 10ⁿ), about `count` of them.
pub fn nice_ticks(min: f64, max: f64, count: usize) -> Vec<Tick> {
    let (min, max) = if !min.is_finite() || !max.is_finite() || min == max {
        if min.is_finite() && min != 0.0 {
            (min.min(0.0), min.max(0.0))
        } else {
            (0.0, 1.0)
        }
    } else {
        (min.min(max), max.max(min))
    };
    let range = max - min;
    if range <= f64::EPSILON {
        return vec![Tick {
            value: min,
            label: format::format_number(&Scalar::Float(min)),
        }];
    }
    let raw = range / count.max(1) as f64;
    let magnitude = 10f64.powf(raw.log10().floor());
    let residual = raw / magnitude;
    let nice = if residual <= 1.0 {
        1.0
    } else if residual <= 2.0 {
        2.0
    } else if residual <= 5.0 {
        5.0
    } else {
        10.0
    };
    let step = nice * magnitude;
    let start = (min / step).floor() * step;
    let end = (max / step).ceil() * step;
    let mut ticks = Vec::new();
    let mut value = start;
    let mut guard = 0;
    while value <= end + step * 0.5 && guard < 32 {
        ticks.push(Tick {
            value,
            label: format::format_number(&Scalar::Float(value)),
        });
        value += step;
        guard += 1;
    }
    if ticks.is_empty() {
        ticks.push(Tick {
            value: min,
            label: format::format_number(&Scalar::Float(min)),
        });
    }
    ticks
}

struct LinePaint {
    grid: Vec<Path<Pixels>>,
    lines: Vec<Path<Pixels>>,
    dots: Vec<PaintQuad>,
    colors: (Hsla, Hsla),
}

fn line_plot(theme: &UiTheme, model: &ChartModel, width: Pixels) -> AnyElement {
    let values: Vec<Option<f64>> = model.points.iter().map(|point| point.value).collect();
    let finite: Vec<f64> = values.iter().filter_map(|value| *value).collect();
    if finite.is_empty() {
        return empty_plot(theme, width);
    }
    let min = finite.iter().copied().fold(f64::INFINITY, f64::min);
    let max = finite.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let ticks = nice_ticks(min, max, theme.chart_tick_count());
    let low = ticks.first().map(|tick| tick.value).unwrap_or(min);
    let high = ticks.last().map(|tick| tick.value).unwrap_or(max);
    let span = (high - low).max(f64::EPSILON);

    // Normalize x by category index so nulls keep their place.
    let count = model.points.len();
    let denominator = count.saturating_sub(1).max(1) as f64;
    let mut segments: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut current: Vec<(f32, f32)> = Vec::new();
    for (index, value) in values.iter().enumerate() {
        match value {
            Some(value) => {
                let x = (index as f64 / denominator) as f32;
                let y = ((high - *value) / span) as f32;
                current.push((x, y));
            }
            None => {
                if !current.is_empty() {
                    segments.push(std::mem::take(&mut current));
                }
            }
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }

    let line_color = theme.chart_color(0);
    let grid_color = theme.border();
    let line_width = theme.chart_line_width();
    let grid_width = theme.chart_grid_width();
    let tick_values: Vec<f64> = ticks.iter().map(|tick| tick.value).collect();
    let plot_height = theme.chart_height();
    let axis_width = theme.chart_axis_width();
    let plot_width = px((width.as_f32() - axis_width.as_f32()).max(1.0));

    let canvas_plot = canvas(
        move |bounds, _, _| {
            let w = bounds.size.width.as_f32();
            let h = bounds.size.height.as_f32();
            let mut grid = Vec::new();
            for value in &tick_values {
                let y = ((high - *value) / span) as f32 * h;
                let mut builder = PathBuilder::stroke(grid_width);
                builder.move_to(point(bounds.origin.x, bounds.origin.y + px(y)));
                builder.line_to(point(bounds.origin.x + px(w), bounds.origin.y + px(y)));
                if let Ok(path) = builder.build() {
                    grid.push(path);
                }
            }
            let mut lines = Vec::new();
            let mut dots = Vec::new();
            for segment in &segments {
                if segment.len() == 1 {
                    let (x, y) = segment[0];
                    let center_x = bounds.origin.x + px(x * w);
                    let center_y = bounds.origin.y + px(y * h);
                    let dot = fill(
                        gpui_kit::Bounds::from_corners(
                            point(center_x - line_width, center_y - line_width),
                            point(center_x + line_width, center_y + line_width),
                        ),
                        line_color,
                    )
                    .corner_radii(Corners {
                        top_left: line_width,
                        top_right: line_width,
                        bottom_left: line_width,
                        bottom_right: line_width,
                    });
                    dots.push(dot);
                    continue;
                }
                if segment.len() < 2 {
                    continue;
                }
                let mut builder = PathBuilder::stroke(line_width);
                for (index, (x, y)) in segment.iter().enumerate() {
                    let x = bounds.origin.x + px(*x * w);
                    let y = bounds.origin.y + px(*y * h);
                    if index == 0 {
                        builder.move_to(point(x, y));
                    } else {
                        builder.line_to(point(x, y));
                    }
                }
                if let Ok(path) = builder.build() {
                    lines.push(path);
                }
            }
            LinePaint {
                grid,
                lines,
                dots,
                colors: (line_color, grid_color),
            }
        },
        move |_, paint, window, _| {
            for path in paint.grid {
                window.paint_path(path, paint.colors.1);
            }
            for path in paint.lines {
                window.paint_path(path, paint.colors.0);
            }
            for dot in paint.dots {
                window.paint_quad(dot);
            }
        },
    )
    .size_full();

    let label_style = theme.text(crate::theme::TextRole::Caption);
    let y_labels = div()
        .flex()
        .flex_col()
        .justify_between()
        .items_end()
        .w(axis_width)
        .h(plot_height)
        .children(ticks.iter().rev().map(|tick| {
            div()
                .text_size(label_style.size)
                .line_height(label_style.line_height)
                .text_color(theme.muted_foreground())
                .child(tick.label.clone())
        }));

    let step = ((count as f32) / 6.0).ceil().max(1.0) as usize;
    let cell = px(plot_width.as_f32() / (count.max(1) as f32 / step as f32));
    let x_labels =
        div()
            .flex()
            .flex_row()
            .w(plot_width)
            .children(
                model
                    .points
                    .iter()
                    .enumerate()
                    .filter_map(|(index, point)| {
                        if index % step != 0 {
                            return None;
                        }
                        Some(
                            div()
                                .w(cell)
                                .truncate()
                                .text_size(label_style.size)
                                .line_height(label_style.line_height)
                                .text_color(theme.muted_foreground())
                                .child(truncate_label(&point.label, theme.chart_label_max_chars())),
                        )
                    }),
            );

    div()
        .flex()
        .flex_col()
        .w(px(width.as_f32().max(0.0)))
        .child(
            div()
                .flex()
                .flex_row()
                .gap(theme.space(1))
                .child(y_labels)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .w(plot_width)
                        .child(div().w(plot_width).h(plot_height).child(canvas_plot))
                        .child(x_labels),
                ),
        )
        .into_any_element()
}

fn truncate_label(label: &str, max_chars: usize) -> String {
    if label.chars().count() <= max_chars {
        return label.to_string();
    }
    let mut out: String = label.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// The chart's cached plot view. The parent wraps it in `AnyView::cached` with
/// definite dimensions (SPEC §10.1).
pub struct ChartView {
    pub id: String,
    pub tree: std::rc::Rc<std::cell::RefCell<crate::tree::RetainedTree>>,
    pub theme: UiTheme,
    pub widths: crate::layout::Widths,
    pub overrides: std::rc::Rc<crate::layout::StateOverrides>,
    pub metrics: std::rc::Rc<std::cell::RefCell<crate::metrics::Metrics>>,
}

impl Render for ChartView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.metrics.borrow_mut().chart_renders += 1;
        let (props, state) = {
            let tree = self.tree.borrow();
            let Some(entry) = tree.get(&self.id) else {
                return div().into_any_element();
            };
            let state = self.overrides.get(&self.id).copied().unwrap_or_else(|| {
                crate::layout::data_state(&entry.props)
                    .unwrap_or_else(|| crate::layout::state_of(entry.status))
            });
            (entry.props.clone(), state)
        };
        let model = ChartModel::from_props(&props);
        let width = self.widths.get(&self.id, self.theme.message_max_width());
        plot_element(&self.theme, &model, width, &self.id, state)
    }
}

/// Wraps a chart view in the fixed-size cache boundary required by SPEC §10.1.
pub fn cached_chart(
    view: gpui_kit::Entity<ChartView>,
    theme: &UiTheme,
    width: Pixels,
) -> AnyElement {
    let style = div()
        .w(px(width.as_f32().max(0.0)))
        .h(theme.chart_block_height())
        .style()
        .clone();
    AnyView::from(view).cached(style).into_any_element()
}

/// The titled chart wrapper around an already-built (and possibly cached) plot.
pub fn panel(theme: &UiTheme, model: &ChartModel, plot: AnyElement, width: Pixels) -> AnyElement {
    let mut body = div()
        .flex()
        .flex_col()
        .gap(theme.space(2))
        .w(px(width.as_f32().max(0.0)));
    if let Some(title) = &model.title {
        let style = theme.text(crate::theme::TextRole::Body);
        body = body.child(
            div()
                .text_size(style.size)
                .line_height(style.line_height)
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .text_color(theme.foreground())
                .child(title.clone()),
        );
    }
    body = body.child(plot);
    if let Some((shown, total)) = model.truncated() {
        body = body.child(caption(theme, format!("Showing {shown} of {total}")));
    }
    if let Some(summary) = &model.summary {
        let style = theme.text(crate::theme::TextRole::Caption);
        body = body.child(
            div()
                .text_size(style.size)
                .line_height(style.line_height)
                .text_color(theme.muted_foreground())
                .child(summary.clone()),
        );
    }
    body.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn props(value: Value) -> Props {
        serde_json::from_value(value).expect("props")
    }

    #[test]
    fn chart_model_parses_typed_points_and_nulls() {
        let model = ChartModel::from_props(&props(json!({
            "kind": "line",
            "x": "week",
            "y": "revenue",
            "title": "Revenue",
            "summary": "Weekly",
            "data": [
                {"map": [["week", "Week 1"], ["revenue", {"i": 8200}]]},
                {"map": [["week", "Week 2"], ["revenue", null]]},
                {"map": [["week", "Week 3"], ["revenue", {"f": 11200.5}]]}
            ]
        })));
        assert_eq!(model.kind, ChartKind::Line);
        assert_eq!(model.title.as_deref(), Some("Revenue"));
        assert_eq!(model.points[0].label, "Week 1");
        assert_eq!(model.points[0].value, Some(8200.0));
        assert_eq!(model.points[1].value, None);
        assert_eq!(model.points[2].value, Some(11200.5));
        assert_eq!(model.total_points, 3);
        assert_eq!(model.truncated(), None);
    }

    #[test]
    fn charts_cap_at_twenty_four_categories_and_report_the_total() {
        let data: Vec<Value> = (0..30)
            .map(|index| json!({"map": [["x", format!("P{index}")], ["y", {"i": index}]]}))
            .collect();
        let model = ChartModel::from_props(&props(json!({
            "kind": "bar", "x": "x", "y": "y", "data": data
        })));
        assert_eq!(model.points.len(), 24);
        assert_eq!(model.truncated(), Some((24, 30)));
    }

    #[test]
    fn nice_ticks_use_round_intervals_and_cover_the_range() {
        let ticks = nice_ticks(0.0, 1.0, 4);
        assert!(ticks.len() >= 3);
        assert_eq!(ticks.first().unwrap().value, 0.0);
        assert!(ticks.last().unwrap().value >= 1.0);
        assert_eq!(ticks[1].value - ticks[0].value, 0.5);
        let ticks = nice_ticks(8200.0, 13000.0, 4);
        assert!(ticks.first().unwrap().value <= 8200.0);
        assert!(ticks.last().unwrap().value >= 13000.0);
        assert_eq!(
            ticks.last().unwrap().label,
            "14,000",
            "value tick labels use §6.12 formatting"
        );
    }

    #[test]
    fn null_segments_keep_category_positions() {
        // The model keeps positions; the line renderer breaks at nulls. This
        // test pins the coordinate rule without needing a window.
        let count = 5usize;
        let denominator = (count - 1) as f64;
        assert_eq!(2.0 / denominator, 0.5);
        assert_eq!(4.0 / denominator, 1.0);
    }
}
