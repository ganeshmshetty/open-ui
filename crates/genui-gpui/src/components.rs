//! Component anatomy (DESIGN §§6–7). Builders take resolved tokens and return
//! GPUI elements; none of them contains a color or pixel literal.

use crate::{
    format::{self, Format, Scalar},
    layout::{VisualState, grid_cell_width, grid_columns, row_is_toolbar},
    props::{self, Props},
    theme::{Radius, TextRole, ThemeMode, Tone, UiTheme},
};
use gpui_kit::{
    AnyElement, AnyView, AppContext, ClipboardItem, Context, ElementId, FontWeight, Hsla,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Window, div,
};

/// Components this milestone draws for real.
pub const IMPLEMENTED: &[&str] = &[
    "Column",
    "Row",
    "Grid",
    "Card",
    "Divider",
    "Heading",
    "Text",
    "Markdown",
    "Code",
    "Badge",
    "Alert",
    "Image",
    "Button",
    "Form",
    "Input",
    "TextArea",
    "Select",
    "Checkbox",
    "CheckboxGroup",
    "RadioGroup",
    "Switch",
    "Stat",
    "Facts",
    "Chart",
    "List",
    "Table",
    "TableColumn",
    "Tabs",
    "Tab",
];

/// Phase 1 implements every catalog component; there is no deferred milestone
/// placeholder left. Unknown host extension kinds still render a labeled
/// fallback, but no standard component is owned by a later milestone.
pub fn deferred_milestone(_kind: &str) -> Option<&'static str> {
    None
}

pub fn is_implemented(kind: &str) -> bool {
    IMPLEMENTED.contains(&kind)
}

fn text_color(theme: &UiTheme, tone: Tone) -> Hsla {
    if tone == Tone::Neutral {
        theme.foreground()
    } else {
        theme.tone(tone).fg
    }
}

pub fn heading(theme: &UiTheme, text: &str, level: i64) -> AnyElement {
    let style = theme.heading(level);
    div()
        .text_size(style.size)
        .line_height(style.line_height)
        .font_weight(style.weight)
        .text_color(theme.foreground())
        .child(props::sanitize_text(text))
        .into_any_element()
}

pub fn text(theme: &UiTheme, props: &Props, state: VisualState) -> AnyElement {
    let value = props::text_or(props, "text", "");
    let size = props::name_or(props, "size", "md");
    let tone = Tone::from_name(&props::name_or(props, "tone", "neutral"));
    let mono = props::bool_(props, "mono", false);
    let bold = props::bool_(props, "bold", false);
    let role = if mono {
        TextRole::Mono
    } else {
        match size.as_str() {
            "sm" => TextRole::Small,
            "lg" => TextRole::BodyLg,
            _ => TextRole::Body,
        }
    };
    let style = theme.text(role);
    let weight = if bold || (mono && bold) {
        FontWeight::SEMIBOLD
    } else {
        style.weight
    };
    let mut element = div()
        .text_size(style.size)
        .line_height(style.line_height)
        .font_weight(weight)
        .text_color(text_color(theme, tone))
        .child(value);
    if mono {
        element = element.font_family(theme.font_mono());
    }
    if state == VisualState::Disabled {
        element = element.opacity(0.5);
    }
    element.into_any_element()
}

pub fn badge(theme: &UiTheme, text: &str, tone: Tone, state: VisualState) -> AnyElement {
    let colors = theme.tone(tone);
    let style = theme.text(TextRole::Caption);
    let mut tag = gpui_kit::component::tag::Tag::custom(colors.bg, colors.fg, colors.border)
        .text_size(style.size)
        .line_height(style.line_height)
        .font_weight(FontWeight::MEDIUM)
        .h(theme.badge_height())
        .px(theme.badge_padding_x())
        .py(theme.badge_padding_y())
        .rounded(theme.radius(Radius::Sm))
        .text_color(colors.fg)
        .child(props::sanitize_text(text));
    if state == VisualState::Disabled {
        tag = tag.opacity(0.5);
    }
    tag.into_any_element()
}

pub fn alert(theme: &UiTheme, id: &str, props: &Props) -> AnyElement {
    use gpui_kit::component::alert::{Alert, AlertVariant};
    let tone = Tone::from_name(&props::name_or(props, "tone", "info"));
    let colors = theme.tone(tone);
    let variant = match tone {
        Tone::Success => AlertVariant::Success,
        Tone::Warning => AlertVariant::Warning,
        Tone::Danger => AlertVariant::Error,
        Tone::Info | Tone::Neutral => AlertVariant::Info,
    };
    let mut alert = Alert::new(
        ElementId::Name(SharedString::from(format!("genui-alert-{id}"))),
        props::text_or(props, "text", ""),
    )
    .with_variant(variant)
    .bg(colors.bg)
    .border_color(colors.border)
    .text_color(colors.fg)
    .p(theme.space(3))
    .rounded(theme.radius(Radius::Md));
    if let Some(title) = props::text(props, "title") {
        alert = alert.title(title);
    }
    alert.into_any_element()
}

pub fn divider(theme: &UiTheme) -> AnyElement {
    div()
        .w_full()
        .py(theme.divider_margin())
        .child(div().w_full().h(theme.hairline()).bg(theme.border()))
        .into_any_element()
}

/// The visible label for a deferred component.
pub fn deferred_label(kind: &str) -> String {
    match deferred_milestone(kind) {
        Some(milestone) => format!("{kind} · arrives in {milestone}"),
        None => format!("Unknown component {kind}"),
    }
}

/// A labeled placeholder for components owned by M7/M8. It is intentionally
/// visible, so a screenshot cannot mistake it for finished work.
pub fn deferred(theme: &UiTheme, kind: &str) -> AnyElement {
    let label = deferred_label(kind);
    let style = theme.text(TextRole::Caption);
    div()
        .flex()
        .items_center()
        .justify_center()
        .p(theme.space(3))
        .bg(theme.surface_subtle())
        .border_1()
        .border_color(theme.border())
        .rounded(theme.radius(Radius::Md))
        .text_size(style.size)
        .line_height(style.line_height)
        .text_color(theme.muted_foreground())
        .child(label)
        .into_any_element()
}

/// A button. Interaction is attached by the view; this function owns only the
/// anatomy (DESIGN §7).
pub fn button(
    theme: &UiTheme,
    id: &str,
    props: &Props,
    state: VisualState,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    let label = props::text_or(props, "label", "");
    let tone = props::name_or(props, "tone", "secondary");
    let style = theme.text(TextRole::Body);
    let disabled = state == VisualState::Disabled
        || state == VisualState::Partial
        || props::bool_(props, "disabled", false);
    let (background, foreground, border) = match tone.as_str() {
        "primary" => (theme.primary(), theme.primary_foreground(), theme.primary()),
        "danger" => (
            theme.tokens().colors.destructive,
            theme.tokens().colors.destructive_foreground,
            theme.tokens().colors.destructive,
        ),
        "ghost" => (theme.background(), theme.foreground(), theme.background()),
        _ => (theme.secondary(), theme.foreground(), theme.secondary()),
    };
    let mut element = div()
        .id(gpui_kit::ElementId::Name(SharedString::from(format!(
            "genui-button-{id}"
        ))))
        .flex()
        .items_center()
        .justify_center()
        .h(theme.control_height(crate::theme::ControlSize::Medium))
        .px(theme.space(3))
        .rounded(theme.radius(Radius::Md))
        .bg(if state == VisualState::Hover {
            theme.secondary()
        } else {
            background
        })
        .text_color(foreground)
        .text_size(style.size)
        .line_height(style.line_height)
        .font_weight(FontWeight::MEDIUM)
        .border_1()
        .border_color(if state == VisualState::Focus {
            theme.ring()
        } else {
            border
        })
        .child(label);
    if state == VisualState::Focus {
        element = element.border_2();
    }
    if disabled {
        element = element.opacity(0.5);
    }
    element
}

pub fn markdown(theme: &UiTheme, id: &str, source: &str) -> AnyElement {
    let style = theme.text(TextRole::BodyLg);
    let heading_theme = theme.clone();
    let mut text_style = gpui_kit::component::text::TextViewStyle::default()
        .paragraph_gap(theme.paragraph_gap())
        .heading_font_size(move |level, _| heading_theme.markdown_heading(level))
        .inline_code(gpui_kit::HighlightStyle {
            background_color: Some(theme.surface_subtle()),
            color: Some(theme.foreground()),
            ..Default::default()
        });
    text_style.is_dark = theme.mode() == ThemeMode::Dark;
    div()
        .w_full()
        .text_size(style.size)
        .line_height(style.line_height)
        .text_color(theme.foreground())
        .child(
            gpui_kit::component::text::TextView::markdown(
                ElementId::Name(SharedString::from(format!("genui-markdown-{id}"))),
                props::sanitize_text(source),
            )
            .style(text_style)
            // Model content is untrusted and the renderer performs no I/O
            // (SPEC §10.12): links are inert until the host installs a policy.
            .on_link_click(|_, _, _, _| {}),
        )
        .into_any_element()
}

pub fn code(theme: &UiTheme, id: &str, props: &Props) -> AnyElement {
    let source = props::text_or(props, "source", "");
    let lang = props::text(props, "lang");
    let fence = match lang {
        Some(lang) if !lang.is_empty() => format!("```{lang}\n{source}\n```"),
        _ => format!("```\n{source}\n```"),
    };
    let mut block = div()
        .bg(theme.surface_subtle())
        .rounded(theme.radius(Radius::Md))
        .p(theme.space(3));
    let block_style = block.style().clone();
    let mut text_style =
        gpui_kit::component::text::TextViewStyle::default().code_block(block_style);
    text_style.is_dark = theme.mode() == ThemeMode::Dark;
    let copy_style = theme.text(TextRole::Caption);
    let copy_color = theme.accent();
    let code_id = format!("genui-code-{id}");
    let copy_id = format!("genui-code-copy-{id}");
    div()
        .w_full()
        .child(
            gpui_kit::component::text::TextView::markdown(
                ElementId::Name(SharedString::from(code_id)),
                fence,
            )
            .style(text_style)
            .on_link_click(|_, _, _, _| {})
            .code_block_actions(move |block, _window, _cx| {
                let copy_id = copy_id.clone();
                let code = block.code();
                let label = block.lang().unwrap_or_else(|| "code".into());
                div()
                    .id(ElementId::Name(SharedString::from(copy_id)))
                    .cursor_pointer()
                    .text_size(copy_style.size)
                    .line_height(copy_style.line_height)
                    .text_color(copy_color)
                    .child(SharedString::from(format!("Copy {label}")))
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(code.to_string()));
                    })
            }),
        )
        .into_any_element()
}

pub fn column(theme: &UiTheme, props: &Props, children: Vec<AnyElement>) -> AnyElement {
    let gap = theme.space(props::token_step(props, "gap", 2));
    let padding = theme.space(props::token_step(props, "padding", 0));
    let align = props::name_or(props, "align", "stretch");
    let mut element = div().flex().flex_col().w_full().gap(gap).p(padding);
    element = match align.as_str() {
        "start" => element.items_start(),
        "center" => element.items_center(),
        "end" => element.items_end(),
        _ => element.items_stretch(),
    };
    element.children(children).into_any_element()
}

pub fn row(
    theme: &UiTheme,
    props: &Props,
    children: Vec<AnyElement>,
    kinds: &[String],
) -> AnyElement {
    let gap = theme.space(props::token_step(props, "gap", 2));
    let align = props::name_or(props, "align", "center");
    let wrap = props::bool_(props, "wrap", false) || row_is_toolbar(kinds);
    let mut element = div().flex().flex_row().w_full().gap(gap);
    if wrap {
        element = element.flex_wrap();
    }
    element = match align.as_str() {
        "start" => element.items_start(),
        "end" => element.items_end(),
        "stretch" => element.items_stretch(),
        _ => element.items_center(),
    };
    element.children(children).into_any_element()
}

pub fn grid(
    theme: &UiTheme,
    props: &Props,
    children: Vec<AnyElement>,
    width: gpui_kit::Pixels,
) -> AnyElement {
    let requested = u16::try_from(props::int(props, "cols").unwrap_or(2)).unwrap_or(2);
    // The catalog default is 2; DESIGN rule 5 prefers 3. The renderer keeps
    // the catalog value because resolved IR props do not record whether the
    // model passed the argument (logged in PROGRESS.md).
    let gap = theme.space(props::token_step(props, "gap", 2));
    let cols = grid_columns(width, requested, gap, theme.grid_min_cell());
    let _cell = grid_cell_width(width, cols, gap);
    div()
        .grid()
        .grid_cols(cols)
        .gap(gap)
        .w_full()
        .children(children)
        .into_any_element()
}

pub fn card(
    theme: &UiTheme,
    props: &Props,
    children: Vec<AnyElement>,
    first_is_heading: bool,
) -> AnyElement {
    let padding = theme.space(props::token_step(props, "padding", 4));
    let title = props::text(props, "title");
    let mut element = div()
        .flex()
        .flex_col()
        .w_full()
        .p(padding)
        .bg(theme.surface())
        .border_1()
        .border_color(theme.border())
        .rounded(theme.radius(Radius::Xl));
    if let Some(title) = title.filter(|_| !first_is_heading) {
        let style = theme.text(TextRole::Body);
        element = element
            .gap(theme.card_title_gap())
            .child(
                div()
                    .text_size(style.size)
                    .line_height(style.line_height)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.foreground())
                    .child(title),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap(theme.card_child_gap())
                    .children(children),
            );
    } else {
        element = element.gap(theme.card_child_gap()).children(children);
    }
    element.into_any_element()
}

pub fn stat(theme: &UiTheme, props: &Props) -> AnyElement {
    let label = props::text_or(props, "label", "");
    let value = props
        .get("value")
        .map(Scalar::from_wire)
        .unwrap_or(Scalar::Null);
    let format = Format::from_name(&props::name_or(props, "format", "number"));
    let currency = props::text_or(props, "currency", "USD");
    let formatted = format::format_scalar(&value, format, &currency);
    let value_style = theme.text(TextRole::Heading);
    let label_style = theme.text(TextRole::Caption);
    let mut value_row = div().flex().flex_row().items_center().gap(theme.space(2));
    value_row = value_row.child(
        div()
            .text_size(value_style.size)
            .line_height(value_style.line_height)
            .font_weight(value_style.weight)
            .font_features(theme.tabular_features())
            .text_color(theme.foreground())
            .child(formatted),
    );
    if let Some(delta) = props::number(props, "delta") {
        let good = props::name_or(props, "good", "up");
        let tone = if delta == 0.0 {
            Tone::Neutral
        } else if (delta > 0.0 && good == "up") || (delta < 0.0 && good == "down") {
            Tone::Success
        } else {
            Tone::Danger
        };
        let arrow = if delta > 0.0 {
            "↑"
        } else if delta < 0.0 {
            "↓"
        } else {
            "→"
        };
        let delta_text = format::format_number(&Scalar::Float(delta.abs()));
        value_row = value_row.child(badge(
            theme,
            &format!("{arrow} {delta_text}"),
            tone,
            VisualState::Normal,
        ));
    }
    let mut element = div()
        .flex()
        .flex_col()
        .gap(theme.space(1))
        .w_full()
        .child(
            div()
                .text_size(label_style.size)
                .line_height(label_style.line_height)
                .text_color(theme.muted_foreground())
                .child(label),
        )
        .child(value_row);
    if let Some(caption) = props::text(props, "caption") {
        element = element.child(
            div()
                .text_size(label_style.size)
                .line_height(label_style.line_height)
                .text_color(theme.muted_foreground())
                .child(caption),
        );
    }
    element.into_any_element()
}

pub fn facts(theme: &UiTheme, props: &Props, width: gpui_kit::Pixels) -> AnyElement {
    let items: Vec<_> = props::data(props, "items").into_iter().take(50).collect();
    let columns = props::int(props, "columns").unwrap_or(1).clamp(1, 2) as usize;
    let gap = theme.fact_column_gap();
    let cell_width = if columns == 2 {
        gpui_kit::px(((width.as_f32() - gap.as_f32()) / 2.0).max(0.0))
    } else {
        width
    };
    let label_style = theme.text(TextRole::Small);
    let value_style = theme.text(TextRole::Body);
    let label_width = gpui_kit::px((cell_width.as_f32() * theme.fact_label_fraction()).max(0.0));
    let mut rows = Vec::new();
    for (chunk_index, chunk) in items.chunks(columns).enumerate() {
        let mut row = div().flex().flex_row().w_full().gap(gap).items_start();
        for (item_index, item) in chunk.iter().enumerate() {
            let label = props::map_get(item, "label")
                .map(format::text_of)
                .unwrap_or_default();
            let value = props::map_get(item, "value")
                .map(format::text_of)
                .unwrap_or_default();
            let value_theme = theme.clone();
            let tooltip_text = value.clone();
            let value_id = ElementId::Name(SharedString::from(format!(
                "fact-{}-{}-{}",
                chunk_index, item_index, label
            )));
            row = row.child(
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .w(cell_width)
                    .gap(theme.space(2))
                    .child(
                        div()
                            .w(label_width)
                            .flex_shrink_0()
                            .text_size(label_style.size)
                            .line_height(label_style.line_height)
                            .text_color(theme.muted_foreground())
                            .child(label),
                    )
                    .child(
                        div()
                            .id(value_id)
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(value_style.size)
                            .line_height(value_style.line_height)
                            .text_color(theme.foreground())
                            .tooltip(move |_, cx| {
                                AnyView::from(cx.new(|_| TextTooltip {
                                    text: tooltip_text.clone(),
                                    theme: value_theme.clone(),
                                }))
                            })
                            .child(value),
                    ),
            );
        }
        rows.push(div().w_full().child(row));
    }
    div()
        .flex()
        .flex_col()
        .w_full()
        .gap(theme.fact_row_gap())
        .children(rows)
        .into_any_element()
}

/// The DESIGN pending/failed box. The view uses `crate::image::element` when
/// the host has delivered bytes (SPEC §10.3, ROADMAP item 7).
pub fn image(
    theme: &UiTheme,
    props: &Props,
    width: gpui_kit::Pixels,
    state: VisualState,
) -> AnyElement {
    let alt = props::text_or(props, "alt", "");
    let height = (props::int(props, "height").unwrap_or(160) as f32).clamp(
        theme.image_min_height().as_f32(),
        theme.image_max_height().as_f32(),
    );
    let fit = props::name_or(props, "fit", "cover");
    let image_state = if state == VisualState::Error {
        crate::image::ImageState::Failed
    } else {
        crate::image::ImageState::Pending
    };
    crate::image::element(
        theme,
        &alt,
        gpui_kit::px(height),
        width,
        &fit,
        &image_state,
        state,
    )
}

/// A minimal tooltip view for truncated values (DESIGN §7 Facts/Table).
pub struct TextTooltip {
    pub text: String,
    pub theme: UiTheme,
}

impl Render for TextTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let style = self.theme.text(TextRole::Small);
        div()
            .p(self.theme.space(2))
            .bg(self.theme.tokens().colors.surface)
            .border_1()
            .border_color(self.theme.border())
            .rounded(self.theme.radius(Radius::Md))
            .text_size(style.size)
            .line_height(style.line_height)
            .text_color(self.theme.foreground())
            .max_w(self.theme.tooltip_max_width())
            .child(self.text.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deferred_components_name_their_milestone() {
        for kind in [
            "Form",
            "Input",
            "TextArea",
            "Select",
            "Checkbox",
            "CheckboxGroup",
            "RadioGroup",
            "Switch",
        ] {
            assert_eq!(deferred_milestone(kind), None, "{kind}");
            assert!(is_implemented(kind));
        }
        for kind in ["List", "Table", "TableColumn", "Tabs", "Tab"] {
            assert_eq!(deferred_milestone(kind), None, "{kind}");
            assert!(is_implemented(kind));
        }
        assert_eq!(deferred_milestone("Nope"), None);
        assert!(is_implemented("Chart"));
    }
}
