//! Standard catalog defined with the §8.1 builder (§8.8).
#![forbid(unsafe_code)]
use genui_core::r#lib::*;
pub fn std_library() -> Result<Library, Vec<String>> {
    let mut b = Library::builder("std", "0.1.0");
    b = b.group(GroupDef::new(
        "Layout",
        1,
        "`Column` for vertical stacks; `Row` for horizontal ones.",
    ));
    b = b.group(GroupDef::new(
        "Content",
        2,
        "`Heading` leads; `Text` explains; `Badge` marks status.",
    ));
    b = b.group(GroupDef::new(
        "Actions",
        3,
        "`Button` labels use verbs; `Form` groups related fields.",
    ));
    b = b.group(GroupDef::new(
        "Data",
        4,
        "`Stat` for headline numbers; `Facts` for details; `Table` for records.",
    ));
    b = b.type_(
        "Space",
        TypeDef::Token {
            scale: "space".into(),
            min: 0,
            max: 8,
        },
    );
    b = b.type_(
        "Align",
        TypeDef::Enum {
            values: vec![
                "start".into(),
                "center".into(),
                "end".into(),
                "stretch".into(),
            ],
        },
    );
    b = b.type_(
        "Tone",
        TypeDef::Enum {
            values: vec![
                "neutral".into(),
                "info".into(),
                "success".into(),
                "warning".into(),
                "danger".into(),
            ],
        },
    );
    b = b.type_(
        "ButtonTone",
        TypeDef::Enum {
            values: vec![
                "primary".into(),
                "secondary".into(),
                "ghost".into(),
                "danger".into(),
            ],
        },
    );
    b = b.type_(
        "TextSize",
        TypeDef::Enum {
            values: vec!["sm".into(), "md".into(), "lg".into()],
        },
    );
    b = b.type_(
        "InputKind",
        TypeDef::Enum {
            values: vec![
                "text".into(),
                "email".into(),
                "url".into(),
                "number".into(),
                "integer".into(),
            ],
        },
    );
    b = b.type_(
        "ColFormat",
        TypeDef::Enum {
            values: vec![
                "text".into(),
                "number".into(),
                "compact".into(),
                "currency".into(),
                "percent".into(),
                "delta".into(),
                "date".into(),
            ],
        },
    );
    b = b.type_(
        "ChartKind",
        TypeDef::Enum {
            values: vec!["bar".into(), "line".into()],
        },
    );
    b = b.type_(
        "ImageFit",
        TypeDef::Enum {
            values: vec!["cover".into(), "contain".into()],
        },
    );
    b = b.type_(
        "ColumnAlign",
        TypeDef::Enum {
            values: vec!["start".into(), "end".into()],
        },
    );
    b = b.type_(
        "GoodDirection",
        TypeDef::Enum {
            values: vec!["up".into(), "down".into()],
        },
    );
    b = b.component(
        ComponentDef::new("Column", Role::Group)
            .group("Layout")
            .doc("A vertical stack of child nodes.")
            .req("children", T::nodes())
            .opt("gap", T::enum_("Space"), 2)
            .opt("align", T::enum_("Align"), "stretch")
            .opt("padding", T::enum_("Space"), 0)
            .to_text("{children}")
            .example("Column([Text(\"Overview\")])"),
    );
    b = b.component(
        ComponentDef::new("Row", Role::Group)
            .group("Layout")
            .doc("A horizontal row of child nodes, optionally wrapping.")
            .req("children", T::nodes())
            .opt("gap", T::enum_("Space"), 2)
            .opt("align", T::enum_("Align"), "center")
            .opt("wrap", T::bool(), false)
            .to_text("{children}")
            .example("Row([Badge(\"Ready\"),Button(\"Open\")])"),
    );
    b = b.component(
        ComponentDef::new("Grid", Role::Group)
            .group("Layout")
            .doc("A responsive grid with one to four columns; use for headline Stat groups.")
            .req("children", T::nodes())
            .opt("cols", T::int(), 2)
            .range(1.0, 4.0)
            .opt("gap", T::enum_("Space"), 2)
            .to_text("{children}")
            .example("Grid([Stat(\"Revenue\",42000),Stat(\"Orders\",120)])"),
    );
    b = b.component(
        ComponentDef::new("Card", Role::Region)
            .group("Layout")
            .doc("A titled region grouping related content.")
            .req("children", T::nodes())
            .opt_none("title", T::text())
            .opt("padding", T::enum_("Space"), 4)
            .to_text("**{title}**\n{children}")
            .example("Card([Text(\"Delivery is on schedule\")],title:\"Delivery\")"),
    );
    b = b.component(
        ComponentDef::new("Divider", Role::Separator)
            .group("Layout")
            .doc("A separator between sections.")
            .to_text("---")
            .example("Divider()"),
    );
    b = b.component(
        ComponentDef::new("Heading", Role::Heading)
            .group("Content")
            .doc("A section heading with level one to four.")
            .req("text", T::text())
            .opt("level", T::int(), 2)
            .range(1.0, 4.0)
            .to_text("## {text}")
            .example("Heading(\"Overview\")"),
    );
    b = b.component(
        ComponentDef::new("Text", Role::Paragraph)
            .group("Content")
            .doc("Plain text with semantic size and tone; no inline markup.")
            .req("text", T::text())
            .opt("size", T::enum_("TextSize"), "md")
            .opt("tone", T::enum_("Tone"), "neutral")
            .opt("bold", T::bool(), false)
            .opt("mono", T::bool(), false)
            .to_text("{text}")
            .example("Text(\"Revenue grew 12% this month\")"),
    );
    b = b.component(
        ComponentDef::new("Markdown", Role::Region)
            .group("Content")
            .doc("CommonMark content with policy-checked links; no raw HTML or inline images.")
            .req("source", T::text())
            .to_text("{source}")
            .requires("markdown")
            .example("Markdown(\"**Ready** to ship\")"),
    );
    b = b.component(
        ComponentDef::new("Code", Role::Code)
            .group("Content")
            .doc("A code block; code_highlight enables optional syntax colors.")
            .req("source", T::text())
            .opt_none("lang", T::text())
            .to_text("```{lang}\n{source}\n```")
            .example("Code(\"let ready = true;\",lang:\"rust\")"),
    );
    b = b.component(
        ComponentDef::new("Badge", Role::Status)
            .group("Content")
            .doc("A compact status label with a semantic tone.")
            .req("text", T::text())
            .opt("tone", T::enum_("Tone"), "neutral")
            .to_text("[{text}]")
            .example("Badge(\"Active\",tone:\"success\")"),
    );
    b = b.component(
        ComponentDef::new("Alert", Role::Alert)
            .group("Content")
            .doc("An important or urgent message with an optional title.")
            .req("text", T::text())
            .opt_none("title", T::text())
            .opt("tone", T::enum_("Tone"), "info")
            .to_text("> **{title}** {text}")
            .example("Alert(\"Payment is overdue\",tone:\"warning\")"),
    );
    b = b.component(
        ComponentDef::new("Image", Role::Image)
            .group("Content")
            .doc("A host-fetched image with a required accessible description and URL policy.")
            .req("src", T::url())
            .req("alt", T::text())
            .opt("fit", T::enum_("ImageFit"), "cover")
            .opt("height", T::int(), 160)
            .range(40.0, 480.0)
            .to_text("[image: {alt}]")
            .requires("images")
            .example("Image(\"https://example.com/photo\",\"Mountain trail\")"),
    );
    b = b.component(
        ComponentDef::new("Button", Role::Button)
            .group("Actions")
            .doc("A labeled action or form submit control.")
            .req("label", T::text())
            .opt("tone", T::enum_("ButtonTone"), "secondary")
            .opt_none("on_click", T::action())
            .opt("submit", T::bool(), false)
            .opt("disabled", T::bool(), false)
            .to_text("[{label}]")
            .example("Button(\"Save\",tone:\"primary\",on_click:say(\"Save my changes\"))"),
    );
    b=b.component(ComponentDef::new("Form",Role::Form).group("Actions").doc("A field scope with a submit action; fields are checked before submission.")
  .req("children",T::nodes())
  .opt_none("on_submit",T::action())
  .to_text("{children}")
  .example("Form([Input(\"Name\"),Button(\"Create account\",submit:true)],on_submit:say(\"Create my account\"))"));
    b = b.component(
        ComponentDef::new("Input", Role::TextInput)
            .group("Actions")
            .doc("A labeled single-line text field, optionally bound to string state.")
            .req("label", T::text())
            .opt_none("bind", T::binding(T::text()))
            .opt("kind", T::enum_("InputKind"), "text")
            .opt_none("placeholder", T::text())
            .opt("required", T::bool(), false)
            .opt_none("min_len", T::int())
            .opt_none("max_len", T::int())
            .opt_none("min", T::number())
            .opt_none("max", T::number())
            .to_text("{label}: {value}")
            .requires("text_input")
            .example("Input(\"Email\",kind:\"email\",required:true)"),
    );
    b = b.component(
        ComponentDef::new("TextArea", Role::MultiLineInput)
            .group("Actions")
            .doc("A labeled multiline text field with bounded rows.")
            .req("label", T::text())
            .opt_none("bind", T::binding(T::text()))
            .opt_none("placeholder", T::text())
            .opt("required", T::bool(), false)
            .opt_none("max_len", T::int())
            .opt("rows", T::int(), 4)
            .range(2.0, 12.0)
            .to_text("{label}: {value}")
            .requires("text_input")
            .example("TextArea(\"Notes\",max_len:500)"),
    );
    b = b.component(
        ComponentDef::new("Select", Role::Select)
            .group("Actions")
            .doc("Choose one value from strings or label/value options.")
            .req("label", T::text())
            .req("options", T::options())
            .opt_none("bind", T::binding(T::text()))
            .opt_none("placeholder", T::text())
            .opt("required", T::bool(), false)
            .to_text("{label}: {value}")
            .example("Select(\"Region\",[\"North\",\"South\"])"),
    );
    b = b.component(
        ComponentDef::new("Checkbox", Role::Checkbox)
            .group("Actions")
            .doc("A labeled boolean checkbox, optionally bound to state.")
            .req("label", T::text())
            .opt_none("bind", T::binding(T::bool()))
            .to_text("[ ] {label}")
            .example("Checkbox(\"Email updates\")"),
    );
    b = b.component(
        ComponentDef::new("CheckboxGroup", Role::CheckboxGroup)
            .group("Actions")
            .doc("Choose multiple string values from labeled options.")
            .req("label", T::text())
            .req("options", T::options())
            .opt_none("bind", T::binding(T::list(T::text())))
            .opt("required", T::bool(), false)
            .to_text("{label}: {values}")
            .example("CheckboxGroup(\"Topics\",[\"News\",\"Releases\"])"),
    );
    b = b.component(
        ComponentDef::new("RadioGroup", Role::RadioGroup)
            .group("Actions")
            .doc("Choose one string value from labeled radio options.")
            .req("label", T::text())
            .req("options", T::options())
            .opt_none("bind", T::binding(T::text()))
            .opt("required", T::bool(), false)
            .to_text("{label}: {value}")
            .example("RadioGroup(\"Delivery\",[\"Standard\",\"Express\"])"),
    );
    b = b.component(
        ComponentDef::new("Switch", Role::Switch)
            .group("Actions")
            .doc("A labeled boolean on/off control.")
            .req("label", T::text())
            .opt_none("bind", T::binding(T::bool()))
            .to_text("{label}: on/off")
            .example("Switch(\"Notifications\")"),
    );
    b = b.component(
        ComponentDef::new("List", Role::List)
            .group("Data")
            .doc("An ordered or unordered list of child nodes.")
            .req("items", T::nodes())
            .opt("ordered", T::bool(), false)
            .to_text("{items}")
            .example("List([Text(\"Prepare\"),Text(\"Ship\")])"),
    );
    b = b.component(
        ComponentDef::new("Table", Role::Table)
            .group("Data")
            .doc("A record table whose columns are TableColumn helpers.")
            .req("columns", T::nodes_of(&["TableColumn"]))
            .min_items(1)
            .req("rows", T::data())
            .opt("empty", T::text(), "No data")
            .to_text("{columns}\n{rows}")
            .example("Table([TableColumn(\"Name\",\"name\")],[{name:\"Ada\"}])"),
    );
    b=b.component(ComponentDef::new("TableColumn",Role::Group).group("Data").doc("A structural column declaration, valid only in Table.columns.")
  .req("header",T::text())
  .req("field",T::text())
  .opt("align",T::enum_("ColumnAlign"),"start")
  .opt("format",T::enum_("ColFormat"),"text")
  .opt("currency",T::text(),"USD")
  .to_text("").only_in("Table.columns")
  .example("Table([TableColumn(\"Revenue\",\"revenue\",format:\"currency\")],[{revenue:42000}])"));
    b = b.component(
        ComponentDef::new("Stat", Role::Group)
            .group("Data")
            .doc("A headline number or text value with optional trend, formatting and caption.")
            .req("label", T::text())
            .req("value", T::scalar())
            .opt_none("delta", T::number())
            .opt("good", T::enum_("GoodDirection"), "up")
            .opt("format", T::enum_("ColFormat"), "number")
            .opt("currency", T::text(), "USD")
            .opt_none("caption", T::text())
            .to_text("**{label}:** {value}{delta}")
            .example("Stat(\"Revenue\",42000,delta:12,format:\"currency\")"),
    );
    b = b.component(
        ComponentDef::new("Facts", Role::List)
            .group("Data")
            .doc("A details panel of text labels and values, limited to 50 records.")
            .req("items", T::data())
            .opt("columns", T::int(), 1)
            .range(1.0, 2.0)
            .to_text("{items}")
            .example("Facts([{label:\"Owner\",value:\"Ada\"},{label:\"Region\",value:\"North\"}])"),
    );
    b=b.component(ComponentDef::new("Chart",Role::Chart).group("Data").doc("A single-series bar or line chart; provide an accessible summary.")
  .req("kind",T::enum_("ChartKind"))
  .req("data",T::data())
  .req("x",T::text())
  .req("y",T::text())
  .opt_none("title",T::text())
  .opt_none("summary",T::text())
  .to_text("{summary}\n{data}").requires("charts")
  .example("Chart(\"bar\",[{month:\"Jan\",sales:10},{month:\"Feb\",sales:12},{month:\"Mar\",sales:14},{month:\"Apr\",sales:18}],\"month\",\"sales\",summary:\"Sales rose from 10 to 18\")"));
    b=b.component(ComponentDef::new("Tabs",Role::TabList).group("Data").doc("A collection of Tab panels with optional bound selection.")
  .req("tabs",T::nodes_of(&["Tab"])).min_items(1)
  .opt_none("bind",T::binding(T::text()))
  .to_text("{tabs}")
  .example("Tabs([Tab(\"Overview\",[Text(\"Ready\")]),Tab(\"Details\",[Text(\"All systems operational\")])])"));
    b = b.component(
        ComponentDef::new("Tab", Role::TabPanel)
            .group("Data")
            .doc("A structural panel in Tabs.tabs; value defaults to its label.")
            .req("label", T::text())
            .req("children", T::nodes())
            .opt_none("value", T::text())
            .to_text("{label}\n{children}")
            .only_in("Tabs.tabs")
            .example("Tabs([Tab(\"Overview\",[Text(\"Ready\")])])"),
    );
    b.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use genui_core::{
        diag::{Code, Severity},
        ir::{Prop, PropValue, Step},
        validate::{Options, validate},
    };
    #[test]
    fn catalog_and_independent_hash_golden() {
        let lib = std_library().unwrap();
        assert_eq!(lib.components().len(), 29);
        assert_eq!(lib.hash(), "d19a769bd510997ce786ade4d60db23d");
        let want = include_str!("../../../conformance/libs/std.json");
        let actual = genui_core::canonical::serialize(lib.spec()).unwrap() + "\n";
        assert_eq!(actual, want);
    }
    #[test]
    fn every_component_example_validates() {
        let lib = std_library().unwrap();
        for c in lib.components() {
            for e in &c.examples {
                let source = format!("root={e}");
                let options = Options {
                    url_hosts: ["example.com".into()].into(),
                    ..Default::default()
                };
                let r = validate(source.as_bytes(), &lib, &options);
                assert!(
                    r.diagnostics.iter().all(|d| d.severity == Severity::Info),
                    "{}: {:?}",
                    c.name,
                    r.diagnostics
                );
            }
        }
    }
    #[test]
    fn dynamic_nodes_keep_plans_and_validate_free_names() {
        let lib = std_library().unwrap();
        let r = validate(
            b"root=List(each([{id:1}],|p|Text(fmt(\"{}\",p.id)),key:\"id\"))",
            &lib,
            &Options::default(),
        );
        assert!(r.diagnostics.is_empty());
        let root = r.ir.nodes.get(&r.ir.root.unwrap()).unwrap();
        assert_eq!(root.slots[0].plans.len(), 1);
        assert!(root.slots[0].children.is_empty());
        assert_eq!(root.slots[0].plans[0].base.0, "root/items:e0");
    }
    #[test]
    fn fallback_preserves_dynamic_source_and_terminal_capability() {
        let lib = std_library().unwrap();
        let options = Options {
            capabilities: Default::default(),
            ..Default::default()
        };
        let r = validate(b"$s=\"text\"\nroot=Markdown($s)", &lib, &options);
        assert_eq!(r.diagnostics.len(), 1);
        assert_eq!(r.diagnostics[0].code, Code::CapFallback);
        assert_eq!(r.ir.canonical_tree(&lib)["kind"], "Text");
        let n = r.ir.nodes.get(&r.ir.root.unwrap()).unwrap();
        assert!(n.props.values().any(|p| matches!(p, Prop::Dynamic(_))));
        assert_eq!(r.ir.fallbacks.len(), 1);
        assert_eq!(
            lib.components()[r.ir.fallbacks.values().next().unwrap().original.kind.0 as usize].name,
            "Markdown"
        );
    }
    #[test]
    fn forms_hold_scopes_and_actions_without_executing() {
        let lib = std_library().unwrap();
        let r=validate(b"$s=\"\"\nroot=Form([Input(\"Name\",bind:$s),Button(\"Save\",submit:true)],on_submit:say(\"Save\",{name:$s}))",&lib,&Options::default());
        assert!(r.diagnostics.is_empty());
        assert_eq!(r.ir.actions.len(), 1);
        let plan = &r.ir.actions[0];
        assert_eq!(plan.submit_scope, r.ir.root);
        assert!(matches!(plan.steps.as_slice(), [Step::Say { .. }]));
        assert!(
            r.ir.nodes
                .values()
                .flat_map(|n| n.props.values())
                .any(|p| matches!(p,Prop::Static(PropValue::Binding(s))if s=="s"))
        );
        assert_eq!(r.ir.states["s"].default, genui_core::value::Value::from(""));
    }
}
