//! Pure layout rules (DESIGN §6) and the visual-state overrides used by the
//! gallery's component state sheet. No GPUI types are constructed here beyond
//! token values, which keeps the rules unit-testable.

use genui_core::ir::NodeStatus;
use gpui_kit::Pixels;
use std::collections::BTreeMap;

/// DESIGN §7 shared states, plus `Empty` for data components.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisualState {
    Normal,
    Hover,
    Focus,
    Disabled,
    Partial,
    Loading,
    Error,
    Empty,
}

impl VisualState {
    pub fn name(self) -> &'static str {
        match self {
            VisualState::Normal => "normal",
            VisualState::Hover => "hover",
            VisualState::Focus => "focus",
            VisualState::Disabled => "disabled",
            VisualState::Partial => "partial",
            VisualState::Loading => "loading",
            VisualState::Error => "error",
            VisualState::Empty => "empty",
        }
    }
}

pub type StateOverrides = BTreeMap<String, VisualState>;

/// The default visual state for a node before any gallery override.
/// Map the session's synthetic `status` prop (§§7.3,7.5) onto loading/error.
pub fn data_state(props: &crate::props::Props) -> Option<VisualState> {
    match props.get("status").and_then(|value| value.as_str()) {
        Some("requested" | "refreshing") => Some(VisualState::Loading),
        Some("denied" | "failed") => Some(VisualState::Error),
        _ => None,
    }
}

pub fn state_of(status: NodeStatus) -> VisualState {
    match status {
        NodeStatus::Partial => VisualState::Partial,
        NodeStatus::Sealed => VisualState::Normal,
    }
}

/// Widths computed by a parent and read by a child during the same render pass.
/// Cached subtrees need definite dimensions (SPEC §10.1).
#[derive(Clone, Default, Debug)]
pub struct Widths(std::rc::Rc<std::cell::RefCell<BTreeMap<String, Pixels>>>);

impl Widths {
    pub fn set(&self, id: &str, width: Pixels) {
        self.0.borrow_mut().insert(id.to_string(), width);
    }

    pub fn get(&self, id: &str, fallback: Pixels) -> Pixels {
        self.0.borrow().get(id).copied().unwrap_or(fallback)
    }

    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }
}

/// DESIGN §6 rule 5: drop columns until every cell is at least `min_cell`
/// wide, down to one column.
pub fn grid_columns(available: Pixels, requested: u16, gap: Pixels, min_cell: Pixels) -> u16 {
    let available = available.as_f32().max(0.0);
    let gap = gap.as_f32().max(0.0);
    let min_cell = min_cell.as_f32().max(0.0);
    let mut cols = requested.clamp(1, 4);
    while cols > 1 {
        let count = f32::from(cols);
        let cell = (available - gap * (count - 1.0)) / count;
        if cell >= min_cell {
            break;
        }
        cols -= 1;
    }
    cols
}

pub fn grid_cell_width(available: Pixels, cols: u16, gap: Pixels) -> Pixels {
    let count = f32::from(cols.max(1));
    let gap = gap.as_f32().max(0.0);
    let width = (available.as_f32() - gap * (count - 1.0)) / count;
    gpui_kit::px(width.max(0.0))
}

/// Rows place children side by side; an unknown width degrades to an equal
/// share rather than overflowing.
pub fn row_cell_width(available: Pixels, children: usize, gap: Pixels) -> Pixels {
    if children == 0 {
        return available;
    }
    let count = children as f32;
    let gap = gap.as_f32().max(0.0);
    gpui_kit::px(((available.as_f32() - gap * (count - 1.0)) / count).max(0.0))
}

/// A Card's content width after its own padding.
pub fn inset_width(width: Pixels, padding: Pixels) -> Pixels {
    gpui_kit::px((width.as_f32() - padding.as_f32() * 2.0).max(0.0))
}

/// DESIGN §6 rule 3: a Heading that is not first gets extra space above it.
pub fn heading_top_margin(theme: &crate::theme::UiTheme, is_first: bool) -> Pixels {
    if is_first {
        theme.space(0)
    } else {
        theme.heading_extra_top()
    }
}

/// DESIGN §6 rule 4: a Row made only of Buttons and Badges is a toolbar and
/// wraps even when `wrap: false`.
pub fn row_is_toolbar(children: &[String]) -> bool {
    !children.is_empty()
        && children
            .iter()
            .all(|kind| matches!(kind.as_str(), "Button" | "Badge"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::test_px;

    #[test]
    fn synthetic_status_maps_to_loading_and_error() {
        let mut requested = crate::props::Props::new();
        requested.insert("status".into(), serde_json::json!("requested"));
        assert_eq!(data_state(&requested), Some(VisualState::Loading));
        let mut refreshing = crate::props::Props::new();
        refreshing.insert("status".into(), serde_json::json!("refreshing"));
        assert_eq!(data_state(&refreshing), Some(VisualState::Loading));
        let mut failed = crate::props::Props::new();
        failed.insert("status".into(), serde_json::json!("failed"));
        assert_eq!(data_state(&failed), Some(VisualState::Error));
        let mut ready = crate::props::Props::new();
        ready.insert("status".into(), serde_json::json!("ready"));
        assert_eq!(data_state(&ready), None);
    }

    #[test]
    fn grid_drops_columns_until_cells_are_wide_enough() {
        assert_eq!(
            grid_columns(test_px(760.), 4, test_px(12.), test_px(220.)),
            3
        );
        assert_eq!(
            grid_columns(test_px(928.), 4, test_px(12.), test_px(220.)),
            4
        );
        assert_eq!(
            grid_columns(test_px(500.), 4, test_px(12.), test_px(220.)),
            2
        );
        assert_eq!(
            grid_columns(test_px(200.), 4, test_px(12.), test_px(220.)),
            1
        );
        assert_eq!(grid_columns(test_px(0.), 2, test_px(12.), test_px(220.)), 1);
    }

    #[test]
    fn grid_cells_share_the_remaining_width() {
        let cell = grid_cell_width(test_px(760.), 3, test_px(12.)).as_f32();
        assert!((cell - 736.0 / 3.0).abs() < 0.01);
        assert_eq!(
            grid_cell_width(test_px(760.), 1, test_px(12.)),
            test_px(760.)
        );
        assert_eq!(row_cell_width(test_px(760.), 2, test_px(8.)), test_px(376.));
        assert_eq!(row_cell_width(test_px(760.), 0, test_px(8.)), test_px(760.));
        assert_eq!(inset_width(test_px(760.), test_px(16.)), test_px(728.));
        assert_eq!(inset_width(test_px(20.), test_px(16.)), test_px(0.));
    }

    #[test]
    fn toolbar_rows_are_detected_by_component_kind() {
        assert!(row_is_toolbar(&["Button".into(), "Badge".into()]));
        assert!(!row_is_toolbar(&["Text".into(), "Button".into()]));
        assert!(!row_is_toolbar(&[]));
    }

    #[test]
    fn widths_start_empty_and_keep_the_last_value() {
        let widths = Widths::default();
        assert_eq!(widths.get("a", test_px(1.)), test_px(1.));
        widths.set("a", test_px(2.));
        assert_eq!(widths.get("a", test_px(1.)), test_px(2.));
        widths.clear();
        assert_eq!(widths.get("a", test_px(1.)), test_px(1.));
    }
}
