//! Input and form anatomy (DESIGN §§6.7, 7, 9; SPEC §§7.3, 10.4, 10.10).
//!
//! The renderer only draws field status; validity and touched state come from
//! the core's synthetic `error`/`touched` resolved props. No function in this
//! module decides whether a value is valid.

use crate::{
    props::{self, OptionItem, Props},
    theme::{Radius, TextRole, Tone, UiTheme},
};
use gpui_component::{
    Disableable, StyledExt as _, ThemeStyled as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::base::{
    Checkbox as BaseCheckbox, Radio as BaseRadio, RadioGroup as BaseRadioGroup,
    Switch as BaseSwitch,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, PathBuilder, Role, SharedString, StatefulInteractiveElement, Styled,
    TestSupportExt as _, Window, canvas, div, point, px,
};
use std::rc::Rc;

pub type CheckboxChange = Rc<dyn Fn(usize, bool, &mut Window, &mut App)>;
pub type RadioChange = Rc<dyn Fn(usize, &mut Window, &mut App)>;
pub type ButtonClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The visual flags shared by the shell of Input, TextArea and Select.
#[derive(Clone, Copy, Debug, Default)]
pub struct ShellState {
    pub focused: bool,
    pub error: bool,
    pub disabled: bool,
}

/// The identity and disabled flag of a checkbox or radio group.
#[derive(Clone, Copy, Debug)]
pub struct GroupLayout<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub disabled: bool,
}

/// The localized text for every core `FieldErrorCode` (SPEC §7.3).
pub fn error_text(code: &str) -> &'static str {
    match code {
        "required" => "This field is required",
        "format" => "Enter a valid value",
        "too_short" => "This value is too short",
        "too_long" => "This value is too long",
        "too_small" => "This value is too small",
        "too_large" => "This value is too large",
        _ => "Check this value",
    }
}

/// Field display state from the synthetic resolved props.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FieldDisplay {
    pub error: Option<String>,
    pub touched: bool,
}

impl FieldDisplay {
    pub fn from_props(props: &Props) -> Self {
        Self {
            error: props::text(props, "error").filter(|error| !error.is_empty()),
            touched: props::bool_(props, "touched", false),
        }
    }

    /// Errors appear after blur or a failed submit, never on the first keystroke.
    pub fn visible_error(&self) -> Option<&str> {
        if self.touched {
            self.error.as_deref()
        } else {
            None
        }
    }

    pub fn shown_error_text(&self) -> Option<&'static str> {
        self.visible_error().map(error_text)
    }
}

/// The accessible description of a field: the error when visible, otherwise the
/// required hint. Never color alone (DESIGN §9).
pub fn field_description(required: bool, display: &FieldDisplay) -> Option<SharedString> {
    if let Some(error) = display.shown_error_text() {
        return Some(error.into());
    }
    required.then_some("Required".into())
}

/// One labelled field: label, control and (when touched) error text.
pub fn field(
    theme: &UiTheme,
    node: &str,
    label: &str,
    control: AnyElement,
    display: &FieldDisplay,
    disabled: bool,
) -> AnyElement {
    let label_style = theme.text(TextRole::Small).weight(FontWeight::MEDIUM);
    let error_style = theme.text(TextRole::Caption);
    let mut element = div()
        .flex()
        .flex_col()
        .w_full()
        .child(
            div()
                .text_size(label_style.size)
                .line_height(label_style.line_height)
                .font_weight(label_style.weight)
                .text_color(theme.foreground())
                .child(props::sanitize_text(label)),
        )
        .child(div().mt(theme.form_label_gap()).w_full().child(control));
    if let Some(error) = display.shown_error_text() {
        element = element.child(
            div()
                .id(ElementId::Name(SharedString::from(format!(
                    "genui-error-{node}"
                ))))
                .test_support()
                .mt(theme.form_error_gap())
                .text_size(error_style.size)
                .line_height(error_style.line_height)
                .text_color(theme.tone(Tone::Danger).fg)
                .child(error),
        );
    }
    let _ = disabled;
    element.into_any_element()
}

/// Error text below a control that draws its own label (checkbox, group,
/// switch). DESIGN §6 rule 7 still applies to the gap.
pub fn field_error(
    theme: &UiTheme,
    node: &str,
    control: AnyElement,
    display: &FieldDisplay,
    disabled: bool,
) -> AnyElement {
    let error_style = theme.text(TextRole::Caption);
    let mut element = div().flex().flex_col().w_full().child(control);
    if let Some(error) = display.shown_error_text() {
        element = element.child(
            div()
                .id(ElementId::Name(SharedString::from(format!(
                    "genui-error-{node}"
                ))))
                .test_support()
                .mt(theme.form_error_gap())
                .text_size(error_style.size)
                .line_height(error_style.line_height)
                .text_color(theme.tone(Tone::Danger).fg)
                .child(error),
        );
    }
    let _ = disabled;
    element.into_any_element()
}

/// The shared border/background of an Input, TextArea and Select shell.
pub fn control_shell(
    theme: &UiTheme,
    control: AnyElement,
    state: ShellState,
    height: gpui_kit::Pixels,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let border = if state.error {
        theme.tone(Tone::Danger).fg
    } else if state.focused {
        theme.ring()
    } else {
        theme.input_border()
    };
    let mut shell = div()
        .flex()
        .items_center()
        .w_full()
        .h(height)
        .px(theme.input_padding_x())
        .bg(theme.surface())
        .border_1()
        .border_color(border)
        .rounded(theme.radius(Radius::Md))
        .when(state.focused, |this| this.focus_ring_style(window, cx))
        .child(control);
    if state.disabled {
        shell = shell.opacity(0.5).cursor_not_allowed();
    }
    shell.into_any_element()
}

/// A multiline shell with DESIGN's `rows × 20 + 12` height.
pub fn textarea_shell(
    theme: &UiTheme,
    control: AnyElement,
    state: ShellState,
    rows: i64,
    window: &Window,
    cx: &App,
) -> AnyElement {
    let rows = rows.clamp(2, 12) as f32;
    let height = px(rows * theme.textarea_row_height().as_f32()
        + theme.textarea_vertical_padding().as_f32() * 2.0);
    control_shell(theme, control, state, height, window, cx)
}

/// A checkbox with DESIGN's 16 px accent box, 8 px gap and 14 px body label.
pub fn checkbox(
    theme: &UiTheme,
    id: ElementId,
    label: &str,
    checked: bool,
    disabled: bool,
    focus_handle: &gpui_kit::FocusHandle,
    on_change: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> AnyElement {
    checkbox_with(theme, id, label, checked, disabled, focus_handle, on_change)
}

fn checkbox_with(
    theme: &UiTheme,
    id: ElementId,
    label: &str,
    checked: bool,
    disabled: bool,
    focus_handle: &gpui_kit::FocusHandle,
    on_change: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let body = theme.text(TextRole::Body);
    let caption = theme.text(TextRole::Caption);
    let label = props::sanitize_text(label);
    BaseCheckbox::new(id)
        .checked(checked)
        .disabled(disabled)
        .track_focus(focus_handle)
        .accessibility_label(label.clone())
        .on_change(move |next, _, window, cx| {
            on_change(
                &matches!(next, gpui_kit::base::CheckboxState::Checked),
                window,
                cx,
            )
        })
        .h_flex()
        .gap(theme.space(2))
        .items_center()
        .cursor_pointer()
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .size(theme.checkbox_size())
                .rounded(theme.radius(Radius::Sm))
                .border_1()
                .border_color(if checked {
                    theme.accent()
                } else if disabled {
                    theme.input_border().opacity(0.5)
                } else {
                    theme.input_border()
                })
                .when(checked, |this| this.bg(theme.accent()))
                .when(checked, |this| {
                    this.child(
                        div()
                            .text_size(caption.size)
                            .line_height(theme.checkbox_size())
                            .text_color(theme.accent_foreground())
                            .child("✓"),
                    )
                }),
        )
        .child(
            div()
                .text_size(body.size)
                .line_height(body.line_height)
                .text_color(if disabled {
                    theme.muted_foreground()
                } else {
                    theme.foreground()
                })
                .child(label),
        )
        .into_any_element()
}

/// A checkbox group: an accessible group label and one checkbox per option.
pub fn checkbox_group(
    theme: &UiTheme,
    layout: GroupLayout<'_>,
    options: &[OptionItem],
    selected: &[String],
    focus_handles: &[gpui_kit::FocusHandle],
    on_change: CheckboxChange,
) -> AnyElement {
    let GroupLayout {
        id: group_id,
        label,
        disabled,
    } = layout;
    let label_style = theme.text(TextRole::Small).weight(FontWeight::MEDIUM);
    let row = options.len() <= 4
        && options
            .iter()
            .all(|option| option.label.chars().count() < 16);
    let mut list = div().flex().w_full().gap(theme.space(2));
    list = if row {
        list.flex_row().flex_wrap()
    } else {
        list.flex_col()
    };
    for (index, option) in options.iter().enumerate() {
        let checked = selected.iter().any(|value| value == &option.value);
        let on_change = on_change.clone();
        let option_label = option.label.clone();
        let handle = focus_handles
            .get(index)
            .expect("checkbox group focus handle");
        list = list.child(checkbox_with(
            theme,
            ElementId::Name(SharedString::from(format!(
                "genui-checkbox-{group_id}-{index}"
            ))),
            &option_label,
            checked,
            disabled,
            handle,
            move |next, window, cx| on_change(index, *next, window, cx),
        ));
    }
    div()
        .id(ElementId::Name(SharedString::from(format!(
            "genui-checkbox-group-{group_id}"
        ))))
        .flex()
        .flex_col()
        .w_full()
        .gap(theme.form_label_gap())
        .role(Role::Group)
        .aria_label(props::sanitize_text(label))
        .child(
            div()
                .text_size(label_style.size)
                .line_height(label_style.line_height)
                .font_weight(label_style.weight)
                .text_color(theme.foreground())
                .child(props::sanitize_text(label)),
        )
        .child(list)
        .into_any_element()
}

/// A radio group. Options flip to a wrapping row for four short labels
/// (DESIGN §7). Arrow keys move the selection and focus.
pub fn radio_group(
    theme: &UiTheme,
    layout: GroupLayout<'_>,
    options: &[OptionItem],
    selected: Option<&str>,
    focus_handles: &[gpui_kit::FocusHandle],
    on_change: RadioChange,
) -> AnyElement {
    let GroupLayout {
        id: group_id,
        label,
        disabled,
    } = layout;
    let label_style = theme.text(TextRole::Small).weight(FontWeight::MEDIUM);
    let body = theme.text(TextRole::Body);
    let row = options.len() <= 4
        && options
            .iter()
            .all(|option| option.label.chars().count() < 16);
    let selected_index = options
        .iter()
        .position(|option| selected == Some(option.value.as_str()));
    let mut list = div().flex().w_full().gap(theme.space(2));
    list = if row {
        list.flex_row().flex_wrap()
    } else {
        list.flex_col()
    };
    for (index, option) in options.iter().enumerate() {
        let checked = selected_index == Some(index);
        let handle = focus_handles.get(index);
        let on_change = on_change.clone();
        let option_label = option.label.clone();
        let mut radio = BaseRadio::new(ElementId::Name(SharedString::from(format!(
            "genui-radio-{group_id}-{index}"
        ))))
        .checked(checked)
        .disabled(disabled)
        .accessibility_label(option_label.clone())
        .set_position(index + 1, options.len())
        .tab_stop(checked || selected_index.is_none() && index == 0)
        .on_change(move |_, _event, window, cx| on_change(index, window, cx))
        .h_flex()
        .gap(theme.space(2))
        .items_center()
        .cursor_pointer()
        .child(
            div()
                .relative()
                .flex_shrink_0()
                .size(theme.checkbox_size())
                .rounded(theme.radius(Radius::Full))
                .border_1()
                .border_color(if checked {
                    theme.accent()
                } else if disabled {
                    theme.input_border().opacity(0.5)
                } else {
                    theme.input_border()
                })
                .when(checked, |this| this.bg(theme.accent()))
                .when(checked, |this| {
                    this.child(
                        div()
                            .absolute()
                            .top(px((theme.checkbox_size().as_f32()
                                - theme.radio_dot_size().as_f32())
                                / 2.0))
                            .left(px((theme.checkbox_size().as_f32()
                                - theme.radio_dot_size().as_f32())
                                / 2.0))
                            .size(theme.radio_dot_size())
                            .rounded(theme.radius(Radius::Full))
                            .bg(theme.accent_foreground()),
                    )
                }),
        )
        .child(
            div()
                .text_size(body.size)
                .line_height(body.line_height)
                .text_color(if disabled {
                    theme.muted_foreground()
                } else {
                    theme.foreground()
                })
                .child(option_label),
        );
        if let Some(handle) = handle {
            radio = radio.track_focus(handle);
        }
        list = list.child(radio);
    }
    let mut element = div()
        .flex()
        .flex_col()
        .w_full()
        .gap(theme.form_label_gap())
        .child(
            div()
                .text_size(label_style.size)
                .line_height(label_style.line_height)
                .font_weight(label_style.weight)
                .text_color(theme.foreground())
                .child(props::sanitize_text(label)),
        );
    element = element.child(
        div()
            .id(ElementId::Name(SharedString::from(format!(
                "genui-radio-group-{group_id}"
            ))))
            .role(Role::RadioGroup)
            .aria_label(props::sanitize_text(label))
            .child(
                BaseRadioGroup::new(ElementId::Name(SharedString::from(format!(
                    "genui-radio-set-{group_id}"
                ))))
                .axis(if row {
                    gpui_kit::Axis::Horizontal
                } else {
                    gpui_kit::Axis::Vertical
                })
                .child(list),
            ),
    );
    element.into_any_element()
}

/// A switch with DESIGN's 32×18 track and 14 px thumb.
pub fn switch(
    theme: &UiTheme,
    id: ElementId,
    label: &str,
    checked: bool,
    disabled: bool,
    focus_handle: &gpui_kit::FocusHandle,
    on_change: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let body = theme.text(TextRole::Body);
    let thumb_left = if checked {
        px(theme.switch_size().width.as_f32()
            - theme.switch_thumb_size().as_f32()
            - theme.switch_inset().as_f32())
    } else {
        theme.switch_inset()
    };
    BaseSwitch::new(id)
        .checked(checked)
        .disabled(disabled)
        .track_focus(focus_handle)
        .accessibility_label(props::sanitize_text(label))
        .on_change(move |next, _, window, cx| on_change(next, window, cx))
        .h_flex()
        .gap(theme.space(2))
        .items_center()
        .cursor_pointer()
        .child(
            div()
                .relative()
                .flex_shrink_0()
                .w(theme.switch_size().width)
                .h(theme.switch_size().height)
                .rounded(theme.radius(Radius::Full))
                .bg(if checked {
                    theme.accent()
                } else if disabled {
                    theme.input_border().opacity(0.5)
                } else {
                    theme.input_border()
                })
                .child(
                    div()
                        .absolute()
                        .top(theme.switch_inset())
                        .left(thumb_left)
                        .size(theme.switch_thumb_size())
                        .rounded(theme.radius(Radius::Full))
                        .bg(theme.background()),
                ),
        )
        .child(
            div()
                .text_size(body.size)
                .line_height(body.line_height)
                .text_color(if disabled {
                    theme.muted_foreground()
                } else {
                    theme.foreground()
                })
                .child(props::sanitize_text(label)),
        )
        .into_any_element()
}

/// A focusable DESIGN button backed by the kit's button, so Enter and Space
/// activate it and assistive technology sees a button.
pub fn button(
    theme: &UiTheme,
    node: &str,
    label: &str,
    tone: &str,
    disabled: bool,
    on_click: ButtonClick,
) -> AnyElement {
    let style = theme.text(TextRole::Body);
    let mut button = Button::new(ElementId::Name(SharedString::from(format!(
        "genui-button-{node}"
    ))))
    .label(SharedString::from(props::sanitize_text(label)))
    .accessibility_label(props::sanitize_text(label))
    .disabled(disabled)
    .h(theme.control_height(crate::theme::ControlSize::Medium))
    .px(theme.space(3))
    .rounded(theme.radius(Radius::Md))
    .text_size(style.size)
    .font_weight(FontWeight::MEDIUM)
    .on_click(move |event, window, cx| on_click(event, window, cx));
    button = match tone {
        "primary" => button.primary(),
        "danger" => button.danger(),
        "ghost" => button.ghost(),
        _ => button,
    };
    button.into_any_element()
}

/// A chevron drawn from theme tokens rather than an icon asset.
pub fn select_chevron(theme: &UiTheme) -> AnyElement {
    let color = theme.muted_foreground();
    let stroke = theme.select_chevron_stroke();
    let size = theme.select_chevron_size();
    let half = size.as_f32() / 2.0;
    canvas(
        move |bounds, _, _| {
            let mut builder = PathBuilder::stroke(stroke);
            builder.move_to(point(
                bounds.origin.x + px(0.0),
                bounds.origin.y + px(half * 0.55),
            ));
            builder.line_to(point(
                bounds.origin.x + px(half),
                bounds.origin.y + px(half * 1.45),
            ));
            builder.line_to(point(
                bounds.origin.x + px(half * 2.0),
                bounds.origin.y + px(half * 0.55),
            ));
            builder.build().ok()
        },
        move |_, path, window, _| {
            if let Some(path) = path {
                window.paint_path(path, color);
            }
        },
    )
    .w(size)
    .h(size)
    .flex_shrink_0()
    .into_any_element()
}

/// The Form container: fields stack 12 px apart; submit controls start 16 px
/// below the last field (DESIGN §6 rule 7).
pub fn form(
    theme: &UiTheme,
    node: &str,
    children: Vec<AnyElement>,
    first_submit: Option<usize>,
) -> AnyElement {
    let mut element = div()
        .id(ElementId::Name(SharedString::from(format!(
            "genui-form-{node}"
        ))))
        .role(Role::Form)
        .flex()
        .flex_col()
        .w_full()
        .gap(theme.form_field_gap());
    for (index, child) in children.into_iter().enumerate() {
        if Some(index) == first_submit {
            element = element.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .mt(theme.form_submit_gap())
                    .child(child),
            );
        } else {
            element = element.child(child);
        }
    }
    element.into_any_element()
}

/// The accesskit role for a component kind (SPEC §10.10).
pub fn role_for(kind: &str) -> Role {
    match kind {
        "Heading" => Role::Heading,
        "Button" => Role::Button,
        "Input" => Role::TextInput,
        "TextArea" => Role::MultilineTextInput,
        "Select" => Role::ComboBox,
        "Checkbox" => Role::CheckBox,
        "CheckboxGroup" => Role::Group,
        "RadioGroup" => Role::RadioGroup,
        "Switch" => Role::Switch,
        "Form" => Role::Form,
        "List" => Role::List,
        "Table" => Role::Grid,
        "Tabs" => Role::TabList,
        "Tab" => Role::TabPanel,
        "Chart" => Role::Image,
        _ => Role::Group,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_field_error_has_localized_text() {
        for (code, text) in [
            ("required", "This field is required"),
            ("format", "Enter a valid value"),
            ("too_short", "This value is too short"),
            ("too_long", "This value is too long"),
            ("too_small", "This value is too small"),
            ("too_large", "This value is too large"),
        ] {
            assert_eq!(error_text(code), text);
        }
        assert_eq!(error_text("future_code"), "Check this value");
    }

    #[test]
    fn errors_are_hidden_until_touched() {
        let untouched = FieldDisplay {
            error: Some("required".into()),
            touched: false,
        };
        assert_eq!(untouched.shown_error_text(), None);
        let touched = FieldDisplay {
            touched: true,
            ..untouched
        };
        assert_eq!(touched.shown_error_text(), Some("This field is required"));
        assert_eq!(
            field_description(true, &touched).as_deref(),
            Some("This field is required")
        );
        assert_eq!(
            field_description(true, &FieldDisplay::default()).as_deref(),
            Some("Required")
        );
        assert_eq!(field_description(false, &FieldDisplay::default()), None);
    }

    #[test]
    fn accessibility_roles_follow_the_ir() {
        assert_eq!(role_for("Input"), Role::TextInput);
        assert_eq!(role_for("TextArea"), Role::MultilineTextInput);
        assert_eq!(role_for("Select"), Role::ComboBox);
        assert_eq!(role_for("Checkbox"), Role::CheckBox);
        assert_eq!(role_for("RadioGroup"), Role::RadioGroup);
        assert_eq!(role_for("Switch"), Role::Switch);
        assert_eq!(role_for("Form"), Role::Form);
    }
}
