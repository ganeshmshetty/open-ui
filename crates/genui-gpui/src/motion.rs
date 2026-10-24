//! Motion (DESIGN §8): a 150 ms fade for newly inserted sealed nodes and a
//! 1.2 s skeleton pulse. `with_animation` automatically renders a static end
//! state when the OS or host asks for reduced motion (`App::reduce_motion`),
//! so the same code path serves both settings.

use crate::theme::{Radius, UiTheme};
use gpui_kit::{
    Animation, AnimationExt, AnyElement, IntoElement, ParentElement, SharedString, Styled,
    ease_out_quint, relative, div,
};
use std::time::Duration;

/// Fades a freshly inserted subtree once. `generation` changes on re-insert so
/// an element that returns to the tree fades again; ordinary prop updates keep
/// the same id and do not animate (DESIGN §8).
pub fn fade_in(theme: &UiTheme, id: &str, generation: u64, element: AnyElement) -> AnyElement {
    let wrapper = div().child(element);
    wrapper
        .with_animation(
            SharedString::from(format!("genui-fade-{id}-{generation}")),
            Animation::new(Duration::from_millis(theme.fade_in_ms()))
                .with_easing(ease_out_quint()),
            |element, progress| element.opacity(progress),
        )
        .into_any_element()
}

/// A pulsing skeleton bar at 60% width (DESIGN §8 loading tables and lists).
pub fn skeleton_bar(
    theme: &UiTheme,
    id: &str,
    height: gpui_kit::Pixels,
    fraction: f32,
) -> AnyElement {
    let min = theme.skeleton_alpha_min();
    div()
        .w(relative(fraction.clamp(0.05, 1.0)))
        .h(height)
        .rounded(theme.radius(Radius::Sm))
        .bg(theme.surface_subtle())
        .with_animation(
            SharedString::from(format!("genui-skeleton-pulse-{id}")),
            Animation::new(Duration::from_millis(theme.skeleton_pulse_ms())).repeat(),
            move |element, progress| {
                let triangle = 1.0 - (2.0 * progress - 1.0).abs();
                element.opacity(min + (1.0 - min) * triangle)
            },
        )
        .into_any_element()
}

/// Applies the same loading pulse to an arbitrary box (a chart skeleton).
pub fn pulse(theme: &UiTheme, id: &str, element: AnyElement) -> AnyElement {
    let min = theme.skeleton_alpha_min();
    div()
        .child(element)
        .with_animation(
            SharedString::from(format!("genui-skeleton-pulse-{id}")),
            Animation::new(Duration::from_millis(theme.skeleton_pulse_ms())).repeat(),
            move |element, progress| {
                let triangle = 1.0 - (2.0 * progress - 1.0).abs();
                element.opacity(min + (1.0 - min) * triangle)
            },
        )
        .into_any_element()
}

/// `count` skeleton rows, used by a loading Table or a ref-backed List.
pub fn skeleton_rows(
    theme: &UiTheme,
    count: usize,
    row_height: gpui_kit::Pixels,
) -> AnyElement {
    let bar_height = theme.skeleton_bar_height().min(row_height);
    let rows: Vec<AnyElement> = (0..count)
        .map(|index| {
            div()
                .flex()
                .items_center()
                .w_full()
                .h(row_height)
                .px(theme.table_cell_padding_x())
                .child(skeleton_bar(theme, &format!("row-{index}"), bar_height, 0.6))
                .into_any_element()
        })
        .collect();
    div()
        .flex()
        .flex_col()
        .w_full()
        .children(rows)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeMode;

    #[test]
    fn motion_tokens_match_design() {
        let theme = UiTheme::for_mode(ThemeMode::Light);
        assert_eq!(theme.fade_in_ms(), 150);
        assert_eq!(theme.skeleton_pulse_ms(), 1_200);
        assert_eq!(theme.skeleton_alpha_min(), 0.5);
    }

    #[test]
    fn skeleton_bar_fraction_is_bounded() {
        let theme = UiTheme::for_mode(ThemeMode::Light);
        // Construction must not panic for hostile or zero values.
        let _ = skeleton_bar(&theme, "test", theme.skeleton_bar_height(), -1.0);
        let _ = skeleton_bar(&theme, "test2", theme.skeleton_bar_height(), 9.0);
        let _ = skeleton_rows(&theme, 0, theme.table_row_height());
    }
}
