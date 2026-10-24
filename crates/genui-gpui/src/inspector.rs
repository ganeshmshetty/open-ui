//! Developer inspector: retained tree, patch log and diagnostics (ROADMAP item
//! 5, SPEC §10.13). Debug-only; it is not part of the program's IR and adds no
//! protocol. It reads the renderer's own retained state.

use crate::{
    metrics::Metrics,
    theme::{TextRole, UiTheme},
    tree::RetainedTree,
};
use gpui_kit::{
    AnyElement, Context, IntoElement, ParentElement, Render, SharedString, Styled, Window, div,
};
use serde::Serialize;

const MAX_TREE_LINES: usize = 120;
const MAX_PATCH_LINES: usize = 16;
const MAX_DIAGNOSTIC_LINES: usize = 24;

#[derive(Clone, Debug, Default)]
pub struct InspectorInput {
    pub diagnostics: Vec<genui_core::diag::Diagnostic>,
}

/// Deterministic text lines. Kept separate from drawing so the inspector's
/// contents are unit-tested without a window.
pub fn lines(
    tree: &RetainedTree,
    metrics: &Metrics,
    diagnostics: &[genui_core::diag::Diagnostic],
) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "tree · {} nodes · epoch {} · next seq {}",
        tree.len(),
        tree.epoch
            .map(|epoch| epoch.to_string())
            .unwrap_or_else(|| "-".into()),
        tree.next_seq
    ));
    let outline = tree.outline();
    for (tree_lines, line) in outline.lines().enumerate() {
        if tree_lines == MAX_TREE_LINES {
            lines.push(format!(
                "… {} more lines",
                outline.lines().count() - tree_lines
            ));
            break;
        }
        lines.push(line.to_string());
    }
    if tree.is_empty() {
        lines.push("(no concrete root)".into());
    }
    let summary = metrics.summary();
    lines.push(String::new());
    lines.push(format!(
        "frames {} · drains {} · batches {}/{} · ops {} · snapshots {}",
        summary.frames,
        summary.queue_drains,
        summary.batches_applied,
        summary.batches_enqueued,
        summary.ops_applied,
        summary.snapshot_requests
    ));
    lines.push(format!(
        "apply p50 {:.2}ms p99 {:.2}ms · render p50 {:.2}ms p99 {:.2}ms",
        summary.apply.p50_ms, summary.apply.p99_ms, summary.render.p50_ms, summary.render.p99_ms
    ));
    lines.push(String::new());
    lines.push("patches".into());
    for entry in metrics.patch_log.iter().rev().take(MAX_PATCH_LINES).rev() {
        lines.push(format!(
            "epoch {} seq {} · {} ops · {}{}{}",
            entry.epoch,
            entry.seq,
            entry.ops,
            if entry.applied { "applied" } else { "skipped" },
            if entry.discarded { " · discarded" } else { "" },
            if entry.needs_snapshot {
                " · snapshot"
            } else {
                ""
            },
        ));
    }
    if metrics.patch_log.is_empty() {
        lines.push("(no batches)".into());
    }
    lines.push(String::new());
    lines.push(format!("diagnostics · {}", diagnostics.len()));
    for diagnostic in diagnostics.iter().take(MAX_DIAGNOSTIC_LINES) {
        lines.push(format!(
            "{} {} {} {}",
            wire_name(&diagnostic.severity),
            wire_name(&diagnostic.code),
            diagnostic.path.as_deref().unwrap_or("-"),
            diagnostic.message
        ));
    }
    if diagnostics.is_empty() {
        lines.push("(none)".into());
    }
    lines
}

fn wire_name<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .map(|name| name.to_ascii_lowercase())
        .unwrap_or_else(|| "?".into())
}

pub struct InspectorView {
    pub tree: std::rc::Rc<std::cell::RefCell<RetainedTree>>,
    pub metrics: std::rc::Rc<std::cell::RefCell<Metrics>>,
    pub input: InspectorInput,
    pub theme: UiTheme,
}

impl Render for InspectorView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let style = self.theme.text(TextRole::Mono);
        let tree = self.tree.borrow();
        let metrics = self.metrics.borrow();
        let lines = lines(&tree, &metrics, &self.input.diagnostics);
        div()
            .flex()
            .flex_col()
            .w(self.theme.inspector_width())
            .h_full()
            .flex_shrink_0()
            .bg(self.theme.surface_subtle())
            .border_l_1()
            .border_color(self.theme.border())
            .p(self.theme.space(3))
            .gap(self.theme.space(1))
            .overflow_hidden()
            .children(lines.into_iter().map(|line| {
                div()
                    .font_family(self.theme.font_mono())
                    .text_size(style.size)
                    .line_height(self.theme.inspector_line_height())
                    .text_color(if line.starts_with("diagnostics") {
                        self.theme.foreground()
                    } else {
                        self.theme.muted_foreground()
                    })
                    .child(SharedString::from(line))
            }))
    }
}

/// A bordered panel around any program view, with the inspector to the right.
pub fn shell(theme: &UiTheme, program: AnyElement, inspector: Option<AnyElement>) -> AnyElement {
    let style = theme.text(TextRole::Body);
    let mut shell = div()
        .flex()
        .flex_row()
        .size_full()
        .bg(theme.background())
        .text_size(style.size)
        .line_height(style.line_height)
        .text_color(theme.foreground());
    shell = shell.child(
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .p(theme.space(6))
            .child(program),
    );
    if let Some(inspector) = inspector {
        shell = shell.child(inspector);
    }
    shell.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{metrics::PatchSummary, tree::RetainedTree};
    use genui_core::{
        diag::{Code, Diagnostic},
        ir::{NodeStatus, Role},
        patch::{NodeInit, Op, PatchBatch},
    };
    use serde_json::json;
    use std::collections::BTreeMap;

    fn tree() -> RetainedTree {
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
                    node: NodeInit {
                        id: "root".into(),
                        kind: "Column".into(),
                        role: Role::Group,
                        props: BTreeMap::new(),
                        status: NodeStatus::Sealed,
                    },
                },
                Op::Insert {
                    parent: Some("root".into()),
                    slot: "children".into(),
                    index: 0,
                    node: NodeInit {
                        id: "root/children:@a".into(),
                        kind: "Text".into(),
                        role: Role::Paragraph,
                        props: BTreeMap::from([("text".into(), json!("hello"))]),
                        status: NodeStatus::Sealed,
                    },
                },
            ],
        });
        tree
    }

    #[test]
    fn inspector_lines_are_deterministic_and_show_tree_patches_and_diagnostics() {
        let metrics = Metrics {
            frames: 2,
            patch_log: vec![PatchSummary {
                epoch: 1,
                seq: 0,
                ops: 3,
                applied: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let diagnostic = Diagnostic::new(Code::Unresolved, Some("missing".into()), None);
        let text = lines(&tree(), &metrics, &[diagnostic]).join("\n");
        assert!(text.contains("tree · 2 nodes · epoch 1 · next seq 1"));
        assert!(text.contains("Column  root sealed"));
        assert!(text.contains("Text  root/children:@a sealed"));
        assert!(text.contains("epoch 1 seq 0 · 3 ops · applied"));
        assert!(text.contains("name.unresolved"));
        assert!(text.contains("diagnostics · 1"));
        assert!(text.contains("frames 2"));
        assert_eq!(
            text,
            lines(
                &tree(),
                &metrics,
                &[Diagnostic::new(
                    Code::Unresolved,
                    Some("missing".into()),
                    None
                )]
            )
            .join("\n")
        );
    }
}
