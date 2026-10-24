//! Regression tests for mutation survivors in M3 validation (SPEC §§2.10–2.15, 3.5, 7.1).
use super::*;
use crate::r#lib::{ComponentDef, GroupDef, Role, TypeDef};
use crate::session::{Session, SessionOptions};
use std::sync::Arc;

fn library() -> Library {
    let mut builder = Library::builder("regressions", "0.1.0");
    builder
        .groups
        .push(GroupDef::new("G", 0, "Regression fixtures."));
    builder.types.insert(
        "Size".into(),
        TypeDef::Token {
            scale: "px".into(),
            min: 0,
            max: 10,
        },
    );
    builder.types.insert(
        "Kind".into(),
        TypeDef::Enum {
            values: vec!["a".into(), "b".into()],
        },
    );
    let components = vec![
        ComponentDef::new("Leaf", Role::Paragraph)
            .group("G")
            .doc("A leaf.")
            .to_text("{text}")
            .req("text", T::text()),
        ComponentDef::new("Show", Role::Paragraph)
            .group("G")
            .doc("A binding sink.")
            .to_text("{value}")
            .opt_none("value", T::binding(T::text())),
        ComponentDef::new("ListShow", Role::Paragraph)
            .group("G")
            .doc("A list binding sink.")
            .opt_none("items", T::binding(T::list(T::text()))),
        ComponentDef::new("IntListShow", Role::Paragraph)
            .group("G")
            .doc("An integer list binding sink.")
            .opt_none("items", T::binding(T::list(T::Int))),
        ComponentDef::new("Pair", Role::Paragraph)
            .group("G")
            .doc("Two binding sinks.")
            .req("first", T::binding(T::text()))
            .req("second", T::binding(T::Int)),
        ComponentDef::new("Opt", Role::Paragraph)
            .group("G")
            .doc("An optional binding sink.")
            .opt_none("value", T::binding(T::text())),
        ComponentDef::new("Stack", Role::Group)
            .group("G")
            .doc("A stack.")
            .req("children", T::nodes()),
        ComponentDef::new("Row", Role::Group)
            .group("G")
            .doc("A row.")
            .to_text("{children}")
            .requires("images")
            .req("children", T::nodes()),
        ComponentDef::new("Fancy", Role::Paragraph)
            .group("G")
            .doc("A capability-gated leaf.")
            .to_text("Fancy {title} count={count}")
            .requires("images")
            .req("title", T::text())
            .opt_none("count", T::Int),
        ComponentDef::new("FancySlot", Role::Paragraph)
            .group("G")
            .doc("A capability-gated container.")
            .to_text("Fancy {title}: {children}")
            .requires("images")
            .req("title", T::text())
            .req("children", T::nodes()),
        ComponentDef::new("Badge", Role::Paragraph)
            .group("G")
            .doc("A capability-gated token prop.")
            .to_text("Badge {size}")
            .requires("images")
            .req(
                "size",
                T::Named {
                    name: "Size".into(),
                },
            ),
        ComponentDef::new("Dump", Role::Paragraph)
            .group("G")
            .doc("A capability-gated scalar dump.")
            .to_text("{a}|{b}|{c}")
            .requires("images")
            .req("a", T::Any)
            .req("b", T::Any)
            .req("c", T::Any),
        ComponentDef::new("Scalar", Role::Paragraph)
            .group("G")
            .doc("A scalar dump.")
            .to_text("{a}|{b}|{c}")
            .req("a", T::Any)
            .req("b", T::Any)
            .req("c", T::Any),
        ComponentDef::new("Tab", Role::Tab)
            .group("G")
            .doc("A tab.")
            .to_text("{label}")
            .req("label", T::text())
            .opt_none("value", T::text()),
        ComponentDef::new("Choice", Role::Paragraph)
            .group("G")
            .doc("An enum prop.")
            .to_text("{kind}")
            .req(
                "kind",
                T::Named {
                    name: "Kind".into(),
                },
            ),
        ComponentDef::new("Facts", Role::List)
            .group("G")
            .doc("A fact list.")
            .req("items", T::list(T::Any))
            .opt_none("tags", T::list(T::text())),
        ComponentDef::new("Chart", Role::Chart)
            .group("G")
            .doc("A chart.")
            .opt_none("kind", T::Any)
            .opt_none("data", T::Any)
            .opt_none("summary", T::text()),
        ComponentDef::new("Text", Role::Paragraph)
            .group("G")
            .doc("A text fallback.")
            .to_text("{text}")
            .req("text", T::text()),
        ComponentDef::new("Markdown", Role::Paragraph)
            .group("G")
            .doc("A markdown fallback.")
            .to_text("{text}")
            .req("text", T::text()),
    ];
    for component in components {
        builder.components.push(component);
    }
    builder.build().unwrap()
}

fn no_images() -> Options {
    let mut options = Options::default();
    options.capabilities.remove("images");
    options
}

fn live(lines: &[&str]) -> Session {
    let mut session = Session::new(SessionOptions::program(Arc::new(library())));
    for line in lines {
        session.push(line.as_bytes());
    }
    session.finish();
    session
}

fn node_texts(validation: &Validation) -> Vec<String> {
    validation
        .ir
        .nodes
        .values()
        .filter_map(|node| match node.props.get(&PropId(0)) {
            Some(Prop::Static(PropValue::Value(Value::Str(text)))) => Some(text.text.to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn batch_infers_null_and_empty_list_bindings_through_slots() {
    let result = validate(
        b"$n=null\n$xs=[]\nroot=Stack([Show(value:$n), ListShow(items:$xs)])\n",
        &library(),
        &Options::default(),
    );
    assert_eq!(result.ir.states["n"].ty, T::Text);
    assert_eq!(result.ir.states["xs"].ty, T::list(T::Text));
    assert!(
        result.diagnostics.is_empty(),
        "{:?}",
        result
            .diagnostics
            .iter()
            .map(|d| (d.code, d.recovery, d.stmt.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn batch_infers_bindings_through_each_if_and_coalesce() {
    let result = validate(
        b"$e=null\n$i=null\n$c=null\nroot=Stack([each([1], |x| Show(value:$e)), if(true, Show(value:$i), Leaf(\"x\")), coalesce(Show(value:$c), Leaf(\"y\"))])\n",
        &library(),
        &Options::default(),
    );
    assert_eq!(result.ir.states["e"].ty, T::Text);
    assert_eq!(result.ir.states["i"].ty, T::Text);
    assert_eq!(result.ir.states["c"].ty, T::Text);
}

#[test]
fn batch_does_not_infer_from_if_conditions_or_shadowed_locals() {
    let result = validate(
        b"$v=null\n$w=null\nshadow=Show(value:$w)\nroot=Stack([if(Show(value:$v), Leaf(\"a\"), Leaf(\"b\")), each([1], |shadow| Stack([shadow]))])\n",
        &library(),
        &Options::default(),
    );
    assert_eq!(result.ir.states["v"].ty, T::Any);
    assert_eq!(result.ir.states["w"].ty, T::Any);
}

#[test]
fn batch_infers_through_legit_aliases_and_positional_bindings_only() {
    let result = validate(
        b"$v=null\n$a=null\n$b=null\n$o=null\nwidget=Show(value:$v)\nroot=Stack([widget, Pair($a, $b), Opt($o)])\n",
        &library(),
        &Options::default(),
    );
    assert_eq!(result.ir.states["v"].ty, T::Text);
    assert_eq!(result.ir.states["a"].ty, T::Text);
    assert_eq!(result.ir.states["b"].ty, T::Int);
    assert_eq!(result.ir.states["o"].ty, T::Any);
}

#[test]
fn concrete_states_skip_binding_inference_work() {
    let lib = library();
    let options = Options::default();
    let parsed = parse_batch(
        b"$s=\"x\"\nroot=Leaf(\"ok\")\n",
        options.limits.clone().syntax,
    );
    let validator = Validator::new(&parsed.program, parsed.diagnostics, &lib, &options);
    let (_, work, _) = validator.run_measured();
    assert_eq!(work.instances_examined, 1);
}

#[test]
fn ambiguous_states_charge_each_inference_job() {
    let lib = library();
    let options = Options::default();
    let parsed = parse_batch(
        b"$s=null\nroot=Stack([Show(value:$s), Show(value:$s)])\n",
        options.limits.clone().syntax,
    );
    let validator = Validator::new(&parsed.program, parsed.diagnostics, &lib, &options);
    let (_, work, _) = validator.run_measured();
    assert_eq!(work.instances_examined, 7);
}

#[test]
fn tab_without_value_copies_its_label() {
    let result = validate(b"root=Tab(\"One\")\n", &library(), &Options::default());
    let node = result.ir.nodes.values().next().unwrap();
    assert_eq!(
        node.props.get(&PropId(1)),
        Some(&Prop::Static(PropValue::Value(Value::from("One"))))
    );
    assert!(result.diagnostics.is_empty());
}

#[test]
fn facts_caps_only_the_items_prop() {
    let mut options = Options::default();
    options.limits.max_list_len = 200;
    let tags = (0..60)
        .map(|i| format!("\"t{i}\""))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!("root=Facts(items: [], tags: [{tags}])\n");
    let result = validate(source.as_bytes(), &library(), &options);
    assert!(!result.diagnostics.iter().any(|d| d.code == Code::ListLen));
    let node = result.ir.nodes.values().next().unwrap();
    assert!(matches!(
        node.props.get(&PropId(1)),
        Some(Prop::Static(PropValue::Value(Value::List(items)))) if items.len() == 60
    ));
}

#[test]
fn chart_summary_keeps_dynamic_kind_and_static_data_expressions() {
    let result = validate(
        b"k=upper(\"line\")\nroot=Chart(kind:k, data:[1,2])\n",
        &library(),
        &Options::default(),
    );
    let chart = result
        .ir
        .nodes
        .values()
        .find(|node| node.props.contains_key(&PropId(2)))
        .unwrap();
    let Some(Prop::Dynamic(id)) = chart.props.get(&PropId(2)) else {
        panic!("chart summary must be dynamic");
    };
    let summary = result.ir.expressions.get(id).unwrap();
    let Expr::Call { args, .. } = summary else {
        panic!("summary expression shape: {summary:?}");
    };
    let Expr::Call {
        args: kind_args, ..
    } = &args[1].value
    else {
        panic!("kind expression shape: {:?}", args[1].value);
    };
    let Expr::Binary { left, .. } = &kind_args[0].value else {
        panic!("kind comparison shape: {:?}", kind_args[0].value);
    };
    assert!(
        !matches!(left.as_ref(), Expr::Null { .. }),
        "dynamic kind must be retained"
    );
    let Expr::Call {
        args: count_args, ..
    } = &args[2].value
    else {
        panic!("count expression shape: {:?}", args[2].value);
    };
    assert!(
        matches!(&count_args[0].value, Expr::List { .. }),
        "static data must be retained: {:?}",
        count_args[0].value
    );
}

#[test]
fn fallback_renders_scalars_tokens_and_missing_values() {
    let result = validate(
        b"root=Stack([Fancy(\"Title\"), Fancy(\"Title\", count:3), Badge(size:2), Dump(null, 3, true)])\n",
        &library(),
        &no_images(),
    );
    let texts = node_texts(&result);
    assert!(
        texts.iter().any(|text| text == "Fancy Title count="),
        "{texts:?}"
    );
    assert!(
        texts.iter().any(|text| text == "Fancy Title count=3"),
        "{texts:?}"
    );
    assert!(texts.iter().any(|text| text == "Badge 2"), "{texts:?}");
    assert!(texts.iter().any(|text| text == "|3|true"), "{texts:?}");
}

#[test]
fn fallback_row_joins_child_text_and_nested_text_is_rendered() {
    let result = validate(
        b"root=Row([Leaf(\"a\"), Leaf(\"b\")])\n",
        &library(),
        &no_images(),
    );
    let texts = node_texts(&result);
    assert!(texts.iter().any(|text| text == "a \u{b7} b"), "{texts:?}");
}

#[test]
fn fallback_becomes_dynamic_for_descendant_bindings() {
    let result = validate(
        b"$s=\"x\"\nroot=FancySlot(\"Title\", [Show(value:$s)])\n",
        &library(),
        &no_images(),
    );
    let node = result
        .ir
        .nodes
        .values()
        .find(|node| {
            node.props
                .values()
                .any(|prop| matches!(prop, Prop::Dynamic(_)))
        })
        .unwrap();
    assert!(
        node.props
            .values()
            .any(|prop| matches!(prop, Prop::Dynamic(_)))
    );
    assert!(!result.ir.fallbacks.is_empty());
}

#[test]
fn fallback_becomes_dynamic_for_slot_plans() {
    let result = validate(
        b"root=FancySlot(\"Title\", [each([1], |x| Leaf(\"x\"))])\n",
        &library(),
        &no_images(),
    );
    assert!(result.ir.nodes.values().any(|node| {
        node.props
            .values()
            .any(|prop| matches!(prop, Prop::Dynamic(_)))
    }));
    assert!(!result.ir.fallbacks.is_empty());
}

#[test]
fn live_infers_ambiguous_state_types_like_batch() {
    let session = live(&[
        "$n=null\n",
        "$xs=[]\n",
        "root=Stack([Show(value:$n), ListShow(items:$xs)])\n",
    ]);
    assert_eq!(session.ir().states["n"].ty, T::Text);
    assert_eq!(session.ir().states["xs"].ty, T::list(T::Text));
    assert!(
        session.diagnostics().is_empty(),
        "{:?}",
        session
            .diagnostics()
            .iter()
            .map(|d| (d.code, d.stmt.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn live_retraction_rechecks_the_earlier_orphan_binding() {
    // §2.10: when the root first appears, its binding target can retract a
    // type already inferred from a retained orphan. The orphan reader must be
    // revalidated, and the new target itself must stay valid.
    let mut session = Session::new(SessionOptions::program(Arc::new(library())));
    session.push(b"$xs=[]\nalpha=IntListShow(items:$xs)\n");
    session.push(b"zeta=ListShow(items:$xs)\nroot=Stack([zeta])\n");
    assert_eq!(session.ir().states["xs"].ty, T::list(T::Text));
    assert!(
        session
            .diagnostics()
            .iter()
            .any(|d| d.code == Code::BindTarget && d.stmt.as_deref() == Some("alpha")),
        "the retained orphan reader must be rechecked: {:?}",
        session
            .diagnostics()
            .iter()
            .map(|d| (d.code, d.stmt.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        !session
            .diagnostics()
            .iter()
            .any(|d| d.code == Code::BindTarget && d.stmt.as_deref() == Some("zeta")),
        "the first root binding must remain valid: {:?}",
        session
            .diagnostics()
            .iter()
            .map(|d| (d.code, d.stmt.clone()))
            .collect::<Vec<_>>()
    );
}

fn incremental(
    source: &str,
    options: &Options,
) -> (Validation, crate::MaintenanceWork, ValidationCache) {
    let lib = library();
    let parsed = parse_batch(source.as_bytes(), options.limits.clone().syntax);
    let dirty: BTreeSet<_> = parsed
        .program
        .statements
        .iter()
        .map(|statement| statement.name.clone())
        .collect();
    let reverse = BTreeMap::new();
    let (validation, _, work, cache) = validate_incremental(
        &lib,
        options,
        None,
        TaskChanges {
            dirty: &dirty,
            sources: &parsed.program.statements,
            reverse: &reverse,
        },
        None,
        None,
        None,
    );
    (validation, work, cache)
}

#[test]
fn incremental_source_work_and_cache_flags_are_accounted() {
    let (validation, work, cache) = incremental(
        "$s=null\nv=1\norphan=Leaf(\"x\")\nroot=Stack([Show(value:$s), Leaf(text(v))])\n",
        &Options::default(),
    );
    assert_eq!(validation.ir.states["s"].ty, T::Text);
    assert!(cache.ambiguous_states);
    assert!(!cache.allocation_limited);
    assert_eq!(work.statements_checked, 13);
    assert_eq!(work.instances_examined, 7);
    assert_eq!(work.diagnostics_examined, 1);
    assert!(cache.slots.is_empty());
}

#[test]
fn incremental_cache_flags_plain_programs() {
    let (_, work, cache) = incremental("root=Leaf(\"x\")\n", &Options::default());
    assert!(!cache.ambiguous_states);
    assert!(!cache.allocation_limited);
    assert_eq!(work.statements_checked, 1);
    assert_eq!(work.instances_examined, 2);
}

#[test]
fn incremental_cache_reports_node_limits() {
    let mut options = Options::default();
    options.limits.max_nodes = 1;
    let (validation, work, cache) =
        incremental("root=Stack([Leaf(\"a\"), Leaf(\"b\")])\n", &options);
    assert!(validation.diagnostics.iter().any(|d| d.code == Code::Nodes));
    assert!(cache.allocation_limited);
    assert_eq!(work.statements_checked, 1);
    assert_eq!(work.instances_examined, 3);
    assert_eq!(work.diagnostics_examined, 1);
}

#[test]
fn enum_recovery_includes_the_allowed_values_hint() {
    let result = validate(
        b"root=Choice(kind:\"z\")\n",
        &library(),
        &Options::default(),
    );
    let diagnostic = result
        .diagnostics
        .iter()
        .find(|d| d.code == Code::Enum)
        .expect("enum diagnostic");
    assert_eq!(diagnostic.hint.as_deref(), Some("\"a\"|\"b\""));
}

#[test]
fn null_alias_does_not_satisfy_a_non_optional_prop() {
    let result = validate(
        b"n=null\nroot=Leaf(text:n)\n",
        &library(),
        &Options::default(),
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == Code::SchemaType),
        "{:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
    assert!(result.ir.root.is_none());
}

#[test]
fn node_templates_are_drained_even_when_reached_through_each() {
    let result = validate(
        b"tmpl=Leaf(unknown())\nroot=Stack([each([1], |x| Stack([tmpl]))])\n",
        &library(),
        &Options::default(),
    );
    assert!(
        result.diagnostics.iter().any(|d| d.code == Code::UnknownFn),
        "{:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn nested_template_checks_keep_their_pending_work() {
    let result = validate(
        b"first=Leaf(unknown())\nthird=Leaf(\"ok\")\ntmpl=Stack([first, each([1], |x| Stack([third]))])\nroot=Stack([each([1], |x| Stack([tmpl]))])\n",
        &library(),
        &Options::default(),
    );
    assert!(
        result.diagnostics.iter().any(|d| d.code == Code::UnknownFn),
        "{:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn incremental_template_checks_do_not_retain_phantom_slots_or_instances() {
    let (validation, _, cache) = incremental(
        "template=Stack([Leaf(\"t\")])\nroot=Stack([each([1], |x| Stack([template]))])\n",
        &Options::default(),
    );
    assert!(cache.slots.is_empty());
    assert_eq!(cache.instances.len(), validation.ir.nodes.len());
}

#[test]
fn plan_nodes_rejects_malformed_if_and_coalesce_shapes() {
    let lib = library();
    let invalid = [
        "root=Stack([if(true, Leaf(\"a\"))])\n",
        "root=Stack([if(true, Leaf(\"a\"), extra: 1)])\n",
        "root=Stack([if(true, Leaf(\"a\"), Leaf(\"b\"), extra: 1)])\n",
        "root=Stack([coalesce(Leaf(\"a\"))])\n",
    ];
    for source in invalid {
        let result = validate(source.as_bytes(), &lib, &Options::default());
        assert!(
            result.diagnostics.iter().any(|d| d.code == Code::ExprType),
            "{source:?} -> {:?}",
            result
                .diagnostics
                .iter()
                .map(|d| d.code)
                .collect::<Vec<_>>()
        );
    }
    let valid = validate(
        b"root=Stack([coalesce(Leaf(\"a\"), Leaf(\"b\"))])\n",
        &lib,
        &Options::default(),
    );
    assert!(!valid.diagnostics.iter().any(|d| d.code == Code::ExprType));
}

#[test]
fn each_key_lambdas_accept_only_one_or_two_parameters() {
    let lib = library();
    let valid = validate(
        b"root=Stack([each([1], |x| Leaf(\"a\"), key: |x| x)])\n",
        &lib,
        &Options::default(),
    );
    assert!(
        !valid.diagnostics.iter().any(|d| d.code == Code::ExprType),
        "{:?}",
        valid.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
