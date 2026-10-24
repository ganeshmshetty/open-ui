//! The gallery's per-component state sheet (DESIGN §7, SPEC §12.7).
//!
//! It is a synthetic program, not a model output: it renders every implemented
//! component and the shared visual states in one deterministic tree, so the
//! screenshot review can see them all in light and dark. Deferred components
//! are shown as clearly labeled M7/M8 placeholders.

use crate::{
    layout::{StateOverrides, VisualState},
    tree::RetainedTree,
};
use genui_core::{
    ir::{NodeStatus, Role},
    patch::{NodeInit, Op, PatchBatch},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub struct StateSheet {
    pub tree: RetainedTree,
    pub overrides: StateOverrides,
    pub ops: Vec<Op>,
}

struct Scene {
    ops: Vec<Op>,
    position: usize,
    child_counts: BTreeMap<String, u32>,
}

impl Scene {
    fn new() -> Self {
        Self {
            ops: vec![Op::Reset],
            position: 0,
            child_counts: BTreeMap::new(),
        }
    }

    fn add(&mut self, parent: Option<&str>, kind: &str, props: Value) -> String {
        let id = match parent {
            None => "root".to_string(),
            Some(parent) => {
                let id = format!("{parent}/children:#{}", self.position);
                self.position += 1;
                id
            }
        };
        let props: BTreeMap<String, Value> = serde_json::from_value(props).unwrap_or_default();
        let index = parent
            .map(|parent| {
                let count = self.child_counts.entry(parent.to_string()).or_insert(0);
                let index = *count;
                *count += 1;
                index
            })
            .unwrap_or(0);
        self.ops.push(Op::Insert {
            parent: parent.map(str::to_string),
            slot: if parent.is_some() {
                "children".into()
            } else {
                "root".into()
            },
            index,
            node: NodeInit {
                id: id.clone(),
                kind: kind.into(),
                role: role_of(kind),
                props,
                status: NodeStatus::Sealed,
            },
        });
        id
    }
}

fn role_of(kind: &str) -> Role {
    match kind {
        "Column" | "Row" | "Grid" => Role::Group,
        "Card" => Role::Region,
        "Heading" => Role::Heading,
        "Text" | "Markdown" => Role::Paragraph,
        "Code" => Role::Code,
        "Badge" => Role::Status,
        "Alert" => Role::Alert,
        "Divider" => Role::Separator,
        "Image" => Role::Image,
        "Button" => Role::Button,
        "Form" => Role::Form,
        "Input" => Role::TextInput,
        "TextArea" => Role::MultiLineInput,
        "Select" => Role::Select,
        "Checkbox" => Role::Checkbox,
        "CheckboxGroup" => Role::CheckboxGroup,
        "RadioGroup" => Role::RadioGroup,
        "Switch" => Role::Switch,
        "List" => Role::List,
        "Table" => Role::Table,
        "Stat" => Role::Group,
        "Facts" => Role::List,
        "Chart" => Role::Chart,
        "Tabs" => Role::TabList,
        "Tab" => Role::TabPanel,
        _ => Role::Group,
    }
}

fn space(step: u8) -> Value {
    json!({"tok": ["space", step]})
}

fn text_props(text: &str) -> Value {
    json!({"text": text, "size": "md", "tone": "neutral", "bold": false, "mono": false})
}

fn card(title: &str) -> Value {
    json!({"title": title, "padding": space(4)})
}

fn stat(label: &str, value: Value, format: &str, delta: Option<f64>) -> Value {
    let mut props = json!({
        "label": label,
        "value": value,
        "format": format,
        "good": "up",
        "currency": "USD"
    });
    if let Some(delta) = delta {
        props["delta"] = json!({"f": delta});
    }
    props
}

/// Builds the state sheet. `overrides` force hover/focus/loading/error states
/// that a static program cannot reach without M4 data (DESIGN §7).
pub fn build() -> StateSheet {
    let mut scene = Scene::new();
    let mut overrides = StateOverrides::new();
    let root = scene.add(
        None,
        "Column",
        json!({"gap": space(4), "padding": space(0)}),
    );

    scene.add(
        Some(&root),
        "Heading",
        json!({"text": "Component states", "level": 1}),
    );
    scene.add(
        Some(&root),
        "Text",
        json!({"text": "Every implemented component in light and dark. Deferred components are labeled with their milestone.", "size": "lg"}),
    );

    // Typography.
    let typography = scene.add(Some(&root), "Card", card("Typography"));
    for (level, label) in [
        (1, "Heading level 1 / 24"),
        (2, "Heading level 2 / 20"),
        (3, "Heading level 3 / 16"),
        (4, "Heading level 4 / 14"),
    ] {
        scene.add(
            Some(&typography),
            "Heading",
            json!({"text": label, "level": level}),
        );
    }
    for (label, size, bold, mono) in [
        ("Text sm / 13 · The quick brown fox", "sm", false, false),
        ("Text md / 14 · The quick brown fox", "md", false, false),
        ("Text lg / 16 · The quick brown fox", "lg", false, false),
        ("Text bold · The quick brown fox", "md", true, false),
        ("Text mono · 0123456789", "md", false, true),
    ] {
        scene.add(
            Some(&typography),
            "Text",
            json!({"text": label, "size": size, "bold": bold, "mono": mono}),
        );
    }
    let tones = scene.add(Some(&typography), "Row", json!({"gap": space(2)}));
    for tone in ["neutral", "info", "success", "warning", "danger"] {
        scene.add(
            Some(&tones),
            "Badge",
            json!({"text": format!("{tone} tone"), "tone": tone}),
        );
    }

    // Layout.
    let layout = scene.add(Some(&root), "Card", card("Layout"));
    let column = scene.add(
        Some(&layout),
        "Column",
        json!({"gap": space(2), "padding": space(3), "align": "start"}),
    );
    scene.add(
        Some(&column),
        "Text",
        text_props("Column · gap 2 · padding 3 · align start"),
    );
    scene.add(
        Some(&column),
        "Text",
        text_props("A second line shows the column gap."),
    );
    let row = scene.add(
        Some(&layout),
        "Row",
        json!({"gap": space(2), "wrap": false}),
    );
    scene.add(
        Some(&row),
        "Badge",
        json!({"text": "Toolbar", "tone": "info"}),
    );
    scene.add(
        Some(&row),
        "Button",
        json!({"label": "Secondary", "tone": "secondary"}),
    );
    scene.add(
        Some(&row),
        "Button",
        json!({"label": "Ghost", "tone": "ghost"}),
    );
    let grid = scene.add(Some(&layout), "Grid", json!({"cols": 3, "gap": space(3)}));
    for (label, value, format) in [
        ("Revenue", json!({"i": 42800}), "currency"),
        ("Orders", json!({"i": 324}), "number"),
        ("Conversion", json!({"f": 3.2}), "percent"),
    ] {
        scene.add(Some(&grid), "Stat", stat(label, value, format, None));
    }
    scene.add(Some(&layout), "Divider", json!({}));

    // Content.
    let content = scene.add(Some(&root), "Card", card("Content"));
    for tone in ["info", "success", "warning", "danger"] {
        let alert = scene.add(
            Some(&content),
            "Alert",
            json!({
                "title": format!("{} alert", title_case(tone)),
                "text": format!("An {} message uses tone.bg, tone.border and tone.fg.", tone),
                "tone": tone
            }),
        );
        let _ = alert;
    }
    scene.add(
        Some(&content),
        "Alert",
        json!({"text": "An alert without a title still reads as one block.", "tone": "neutral"}),
    );
    scene.add(
        Some(&content),
        "Markdown",
        json!({"source": "**Markdown** supports `inline code`, [links](https://example.com) and lists:\n\n- Faster exports\n- Improved audit history\n- Better keyboard navigation\n\n## A section heading\n\nParagraph text wraps at the container width and uses body_lg."}),
    );
    scene.add(
        Some(&content),
        "Code",
        json!({"source": "let ready = true;\nfor item in items {\n    render(item);\n}", "lang": "rust"}),
    );
    let images = scene.add(
        Some(&content),
        "Row",
        json!({"gap": space(3), "wrap": true}),
    );
    let pending = scene.add(
        Some(&images),
        "Image",
        json!({"src": "https://example.com/pending", "alt": "Pending image", "height": {"i": 120}}),
    );
    overrides.insert(pending, VisualState::Loading);
    scene.add(
        Some(&images),
        "Image",
        json!({"src": "https://example.com/failed", "alt": "Image unavailable", "height": {"i": 120}}),
    );

    // Data.
    let data = scene.add(Some(&root), "Card", card("Data"));
    let stats = scene.add(Some(&data), "Grid", json!({"cols": 3, "gap": space(3)}));
    scene.add(
        Some(&stats),
        "Stat",
        stat("Revenue", json!({"i": 42800}), "currency", Some(12.0)),
    );
    scene.add(
        Some(&stats),
        "Stat",
        stat("Cost per lead", json!({"f": 38.5}), "currency", Some(-8.2)),
    );
    scene.add(
        Some(&stats),
        "Stat",
        stat("Proposal", json!({"s": "In review"}), "text", None),
    );
    scene.add(
        Some(&stats),
        "Stat",
        json!({"label": "Usage", "value": {"f": 62.0}, "format": "percent", "caption": "of monthly allowance"}),
    );
    scene.add(
        Some(&stats),
        "Stat",
        json!({"label": "Installs", "value": {"i": 1234567}, "format": "compact"}),
    );
    scene.add(
        Some(&stats),
        "Stat",
        json!({"label": "Net change", "value": {"i": -1234}, "format": "delta"}),
    );
    let facts = scene.add(
        Some(&data),
        "Facts",
        json!({"items": [
            {"map": [["label", "Plan"], ["value", "Team"]]},
            {"map": [["label", "Allowance"], ["value", "100,000 requests"]]},
            {"map": [["label", "Renewal"], ["value", "November 1, 2026"]]},
            {"map": [["label", "Billing owner"], ["value", "Elena Ruiz"]]}
        ], "columns": {"i": 2}}),
    );
    let _ = facts;
    scene.add(
        Some(&data),
        "Chart",
        json!({
            "kind": "bar",
            "x": "channel",
            "y": "leads",
            "title": "Qualified leads by channel",
            "summary": "Search led with 124 leads, followed by partners at 78, email at 52 and social at 32.",
            "data": [
                {"map": [["channel", "Search"], ["leads", {"i": 124}]]},
                {"map": [["channel", "Partners"], ["leads", {"i": 78}]]},
                {"map": [["channel", "Email"], ["leads", {"i": 52}]]},
                {"map": [["channel", "Social"], ["leads", {"i": 32}]]}
            ]
        }),
    );
    scene.add(
        Some(&data),
        "Chart",
        json!({
            "kind": "line",
            "x": "week",
            "y": "revenue",
            "title": "Revenue trend · null values break the line",
            "summary": "Week 3 has no value and leaves a gap instead of dropping to zero.",
            "data": [
                {"map": [["week", "Week 1"], ["revenue", {"i": 8200}]]},
                {"map": [["week", "Week 2"], ["revenue", {"i": 10400}]]},
                {"map": [["week", "Week 3"], ["revenue", null]]},
                {"map": [["week", "Week 4"], ["revenue", {"i": 13000}]]}
            ]
        }),
    );
    let chart_states = scene.add(Some(&data), "Grid", json!({"cols": 3, "gap": space(3)}));
    let empty = scene.add(
        Some(&chart_states),
        "Chart",
        json!({"kind": "bar", "x": "x", "y": "y", "data": [], "title": "Empty"}),
    );
    overrides.insert(empty, VisualState::Empty);
    let loading = scene.add(
        Some(&chart_states),
        "Chart",
        json!({"kind": "bar", "x": "x", "y": "y", "data": [], "title": "Loading"}),
    );
    overrides.insert(loading, VisualState::Loading);
    let error = scene.add(
        Some(&chart_states),
        "Chart",
        json!({"kind": "line", "x": "x", "y": "y", "data": [], "title": "Failed"}),
    );
    overrides.insert(error, VisualState::Error);

    // Shared states.
    let states = scene.add(Some(&root), "Card", card("States"));
    let buttons = scene.add(Some(&states), "Row", json!({"gap": space(2), "wrap": true}));
    for (label, state) in [
        ("Normal", VisualState::Normal),
        ("Hover", VisualState::Hover),
        ("Focus", VisualState::Focus),
        ("Disabled", VisualState::Disabled),
        ("Partial", VisualState::Partial),
    ] {
        let id = scene.add(
            Some(&buttons),
            "Button",
            json!({"label": label, "tone": if state == VisualState::Normal { "primary" } else { "secondary" }}),
        );
        overrides.insert(id, state);
    }
    let partial_text = scene.add(
        Some(&states),
        "Text",
        json!({"text": "Partial text renders normally while it streams in.", "tone": "info"}),
    );
    overrides.insert(partial_text, VisualState::Partial);

    // Deferred components.
    let deferred = scene.add(Some(&root), "Card", card("Deferred to M7 and M8"));
    scene.add(
        Some(&deferred),
        "Text",
        json!({"text": "These placeholders are clearly labeled until their milestone lands. No stub API is exposed.", "tone": "warning"}),
    );
    let deferred_row = scene.add(
        Some(&deferred),
        "Row",
        json!({"gap": space(2), "wrap": true}),
    );
    for kind in [
        "Input",
        "TextArea",
        "Select",
        "Checkbox",
        "CheckboxGroup",
        "RadioGroup",
        "Switch",
        "Form",
        "List",
        "Table",
        "Tabs",
    ] {
        scene.add(Some(&deferred_row), kind, json!({}));
    }

    let mut tree = RetainedTree::default();
    let ops = scene.ops;
    let batch = PatchBatch {
        epoch: 1,
        seq: 0,
        ops: ops.clone(),
    };
    let _ = tree.apply(&batch);
    StateSheet {
        tree,
        overrides,
        ops,
    }
}

fn title_case(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The batches the gallery applies to a `UiView` for the state sheet.
pub fn batches() -> Vec<PatchBatch> {
    let sheet = build();
    vec![PatchBatch {
        epoch: 1,
        seq: 0,
        ops: sheet.ops,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_sheet_contains_every_implemented_and_deferred_component() {
        let sheet = build();
        let kinds: Vec<&str> = sheet
            .tree
            .nodes
            .values()
            .map(|entry| entry.kind.as_str())
            .collect();
        for kind in crate::components::IMPLEMENTED {
            assert!(kinds.contains(kind), "missing implemented {kind}");
        }
        for kind in ["Input", "Form", "List", "Table", "Tabs"] {
            assert!(kinds.contains(&kind), "missing deferred {kind}");
        }
        assert!(
            sheet
                .overrides
                .values()
                .any(|state| *state == VisualState::Focus)
        );
        assert!(
            sheet
                .overrides
                .values()
                .any(|state| *state == VisualState::Loading)
        );
        assert!(
            sheet
                .overrides
                .values()
                .any(|state| *state == VisualState::Error)
        );
        assert!(!sheet.tree.is_empty());
    }

    #[test]
    fn state_sheet_tree_is_deterministic() {
        let a = build().tree.outline();
        let b = build().tree.outline();
        assert_eq!(a, b);
        assert_eq!(batches().len(), 1);
    }
}
