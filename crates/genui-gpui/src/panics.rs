//! Per-subtree panic containment (SPEC §10.8).
//!
//! Every builder call is wrapped in `catch_unwind`. A panic becomes a muted
//! placeholder, one `render.panic` diagnostic per node revision, and after
//! three panics the node is quarantined until its props change. Panics inside
//! GPUI's later layout/paint passes remain a documented limitation (§10.8
//! item 3, §14.3).

use crate::theme::{Radius, TextRole, UiTheme};
use genui_core::{
    diag::{Code, Diagnostic},
    patch::strip_invisibles,
};
use gpui_kit::{AnyElement, IntoElement, ParentElement, Styled, div};
use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

/// SPEC §10.8: quarantine a node after this many panics on one revision.
pub const QUARANTINE_AFTER: u32 = 3;
/// Bound the number of retained diagnostics from a pathological UI.
pub const MAX_PANIC_DIAGNOSTICS: usize = 64;

/// Whether builders can be contained. With `panic = "abort"` the catch is
/// present but a panic still aborts the process, so hosts must build with
/// unwinding for this boundary (SPEC §10.8).
pub const CONTAINMENT_ENABLED: bool = cfg!(panic = "unwind");

#[derive(Clone, Debug, Default)]
struct Entry {
    rev: u64,
    count: u32,
    quarantined: bool,
}

#[derive(Default)]
pub struct PanicTracker {
    entries: BTreeMap<String, Entry>,
    diagnostics: Vec<Diagnostic>,
    panics: u64,
}

impl PanicTracker {
    pub fn is_quarantined(&self, id: &str, rev: u64) -> bool {
        self.entries
            .get(id)
            .is_some_and(|entry| entry.rev == rev && entry.quarantined)
    }

    pub fn count(&self, id: &str) -> u32 {
        self.entries.get(id).map_or(0, |entry| entry.count)
    }

    pub fn panics(&self) -> u64 {
        self.panics
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.diagnostics.clear();
        self.panics = 0;
    }

    fn record(&mut self, id: &str, rev: u64, payload: &str) {
        self.panics = self.panics.saturating_add(1);
        let should_report = match self.entries.get_mut(id) {
            Some(entry) if entry.rev == rev => {
                entry.count = entry.count.saturating_add(1);
                entry.quarantined = entry.count >= QUARANTINE_AFTER;
                false
            }
            previous => {
                if let Some(previous) = previous {
                    *previous = Entry {
                        rev,
                        count: 1,
                        quarantined: false,
                    };
                } else {
                    self.entries.insert(
                        id.to_string(),
                        Entry {
                            rev,
                            count: 1,
                            quarantined: false,
                        },
                    );
                }
                true
            }
        };
        if should_report && self.diagnostics.len() < MAX_PANIC_DIAGNOSTICS {
            let mut diagnostic = Diagnostic::new(Code::RenderPanic, None, None);
            diagnostic.path = Some(id.to_string());
            diagnostic.message = format!(
                "renderer builder panicked at {id}: {} (node shows a placeholder; quarantined after {QUARANTINE_AFTER} panics)",
                strip_invisibles(payload)
            );
            self.diagnostics.push(diagnostic);
        }
    }
}

/// Runs one node builder under `catch_unwind`, returning a placeholder on a
/// panic. `rev` is the node's prop revision, so a props change lifts a
/// quarantine (SPEC §10.8 item 2).
pub fn guard<F>(
    theme: &UiTheme,
    id: &str,
    rev: u64,
    tracker: &Rc<std::cell::RefCell<PanicTracker>>,
    build: F,
) -> AnyElement
where
    F: FnOnce() -> AnyElement,
{
    if tracker.borrow().is_quarantined(id, rev) {
        return placeholder(theme);
    }
    match catch_unwind(AssertUnwindSafe(build)) {
        Ok(element) => element,
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("non-string panic payload");
            tracker.borrow_mut().record(id, rev, message);
            placeholder(theme)
        }
    }
}

/// The visible fallback for a contained subtree (SPEC §10.8).
pub fn placeholder(theme: &UiTheme) -> AnyElement {
    let style = theme.text(TextRole::Caption);
    div()
        .flex()
        .items_center()
        .justify_center()
        .w_full()
        .p(theme.space(3))
        .bg(theme.surface_subtle())
        .border_1()
        .border_color(theme.border())
        .rounded(theme.radius(Radius::Md))
        .text_size(style.size)
        .line_height(style.line_height)
        .text_color(theme.muted_foreground())
        .child("This part could not be shown")
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeMode;

    #[test]
    fn a_panic_becomes_a_placeholder_and_one_diagnostic() {
        let theme = UiTheme::for_mode(ThemeMode::Light);
        let tracker = Rc::new(std::cell::RefCell::new(PanicTracker::default()));
        let element = guard(&theme, "root/children:#1", 0, &tracker, || {
            panic!("injected builder panic")
        });
        let _ = element;
        let tracker = tracker.borrow();
        assert_eq!(tracker.panics(), 1);
        assert_eq!(tracker.diagnostics().len(), 1);
        assert_eq!(tracker.diagnostics()[0].code, Code::RenderPanic);
        assert_eq!(
            tracker.diagnostics()[0].path.as_deref(),
            Some("root/children:#1")
        );
        assert!(!tracker.is_quarantined("root/children:#1", 0));
    }

    #[test]
    fn the_third_panic_quarantines_until_props_change() {
        let theme = UiTheme::for_mode(ThemeMode::Light);
        let tracker = Rc::new(std::cell::RefCell::new(PanicTracker::default()));
        for _ in 0..3 {
            let _ = guard(&theme, "row", 7, &tracker, || panic!("boom"));
        }
        {
            let tracker = tracker.borrow();
            assert_eq!(tracker.count("row"), 3);
            assert!(tracker.is_quarantined("row", 7));
            assert_eq!(tracker.panics(), 3);
            assert_eq!(
                tracker.diagnostics().len(),
                1,
                "one diagnostic per node revision, not one per panic"
            );
        }
        // A props change lifts the quarantine and the builder runs again.
        let element = guard(&theme, "row", 8, &tracker, || {
            div().child("recovered").into_any_element()
        });
        let _ = element;
        assert!(!tracker.borrow().is_quarantined("row", 8));
    }

    #[test]
    fn a_good_builder_is_returned_unchanged() {
        let theme = UiTheme::for_mode(ThemeMode::Light);
        let tracker = Rc::new(std::cell::RefCell::new(PanicTracker::default()));
        let element = guard(&theme, "ok", 0, &tracker, || {
            div().child("ok").into_any_element()
        });
        let _ = element;
        assert_eq!(tracker.borrow().panics(), 0);
        assert!(tracker.borrow().diagnostics().is_empty());
    }
}
