use super::*;
use crate::r#lib::{ActionDef, ComponentDef, FunctionDef, GroupDef, Role};
use crate::limits::SyntaxLimits;
use proptest::prelude::*;
fn library() -> Library {
    Library::builder("test", "0.1.0")
        .group(GroupDef::new("G", 0, "`Leaf` and `Stack`."))
        .component(
            ComponentDef::new("Leaf", Role::Paragraph)
                .group("G")
                .doc("A leaf.")
                .req("text", T::text())
                .example("Leaf(\"ok\")"),
        )
        .component(
            ComponentDef::new("Stack", Role::Group)
                .group("G")
                .doc("A stack.")
                .req("children", T::nodes())
                .example("Stack([])"),
        )
        .component(
            ComponentDef::new("Control", Role::Button)
                .group("G")
                .doc("A control.")
                .opt_none("on_click", T::action())
                .example("Control()"),
        )
        .build()
        .unwrap()
}
fn keys(result: &Validation) -> Vec<(Code, Recovery)> {
    result
        .diagnostics
        .iter()
        .map(|d| (d.code, d.recovery))
        .collect()
}
#[test]
fn expression_errors_stop_required_prop_cascades() {
    let lib = library();
    for (input, code) in [
        ("root=Leaf(unknown())", Code::UnknownFn),
        ("root=Leaf(upper(3))", Code::ExprType),
        ("root=Leaf(999999999999999999999999)", Code::Overflow),
    ] {
        let r = validate(input.as_bytes(), &lib, &Options::default());
        assert_eq!(keys(&r), vec![(code, Recovery::ValueNulled)]);
        assert!(r.ir.root.is_none());
    }
}
#[test]
fn literal_only_validation_never_calls_library_functions() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    fn callback(_: &[Value]) -> Value {
        CALLS.fetch_add(1, Ordering::SeqCst);
        Value::from("computed")
    }
    let base = library();
    let original = base.canonical_json().unwrap();
    let lib = base
        .extend()
        .function(FunctionDef {
            name: "custom".into(),
            params: vec![T::Int],
            result: T::Text,
            fuel: 7,
            implementation: Some(callback),
        })
        .build()
        .unwrap();
    let result = validate(b"root=Leaf(custom(1))", &lib, &Options::default());
    assert!(result.diagnostics.is_empty());
    assert_eq!(CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(base.canonical_json().unwrap(), original);
    assert_ne!(base.hash(), lib.hash());
    assert!(matches!(
        result
            .ir
            .nodes
            .values()
            .next()
            .unwrap()
            .props
            .get(&PropId(0)),
        Some(Prop::Dynamic(_))
    ));
    assert_eq!(
        keys(&validate(
            b"root=Leaf(custom(true))",
            &lib,
            &Options::default()
        )),
        vec![(Code::ExprType, Recovery::ValueNulled)]
    );
}
#[test]
fn invalid_action_step_does_not_erase_good_steps() {
    let lib = library();
    let r = validate(
        b"root=Control(on_click:[say(1),say(\"valid\")])",
        &lib,
        &Options::default(),
    );
    assert_eq!(keys(&r), vec![(Code::ExprType, Recovery::ValueNulled)]);
    assert_eq!(r.ir.actions.len(), 1);
    assert!(matches!(
        r.ir.actions.iter().next().unwrap().steps.as_slice(),
        [Step::Say { .. }]
    ));
    assert_eq!(
        r.ir.actions.iter().next().unwrap().origin,
        r.ir.root.unwrap()
    );
}
#[test]
fn custom_actions_are_typed_data() {
    let lib = library()
        .extend()
        .action(ActionDef {
            name: "export".into(),
            params: vec![T::Text],
            doc: "Export data.".into(),
        })
        .build()
        .unwrap();
    let r = validate(
        b"root=Control(on_click:export(\"csv\"))",
        &lib,
        &Options::default(),
    );
    assert!(r.diagnostics.is_empty());
    assert!(
        matches!(&r.ir.actions.iter().next().unwrap().steps[0],Step::Emit{name,params}if name=="export"&&params.len()==1)
    );
}
#[test]
fn long_name_chains_use_heap_worklists() {
    let lib = library();
    let mut source = "root=Leaf(v0)\n".to_string();
    for i in 0..1500 {
        source.push_str(&format!("v{i}=v{}\n", i + 1))
    }
    source.push_str("v1500=\"end\"\n");
    let r = validate(source.as_bytes(), &lib, &Options::default());
    assert!(r.diagnostics.is_empty());
    assert_eq!(r.ir.canonical_tree(&lib)["props"]["text"], "end");
}
#[test]
fn root_cycle_is_detected_at_first_recursive_use() {
    let lib = library();
    let r = validate(b"root=Stack([root])", &lib, &Options::default());
    assert_eq!(r.ir.nodes.len(), 1);
    assert_eq!(r.diagnostics.len(), 1);
    assert_eq!(
        r.diagnostics[0].path.as_deref(),
        Some("root/children:@root")
    );
    assert_eq!(r.diagnostics[0].code, Code::Cycle);
}
proptest! {
 #![proptest_config(ProptestConfig::with_cases(512))]
 #[test]fn semantic_permutations_are_identical(n in 0usize..30,budget in 0usize..40,cycle in any::<bool>()){
  let lib=library();let mut source=String::from("root=Stack([");for i in 0..n{source.push_str(&format!("v{i},"));}source.push_str("])\n");for i in 0..n{source.push_str(&if cycle&&i==0{"v0=Stack([v0])\n".into()}else{format!("v{i}=Leaf(\"{i}\")\n")});}
  let mut options=Options::default();options.limits.max_nodes=budget;let parsed=parse_batch(source.as_bytes(),options.limits.syntax.clone());let expected=validate_program(&parsed.program,parsed.diagnostics.clone(),&lib,&options);
  let mut permuted=parsed.program.clone();permuted.statements[1..].reverse();prop_assert_eq!(validate_program(&permuted,parsed.diagnostics,&lib,&options),expected);
 }
 #[test]fn appending_and_dropping_preserves_source_paths(n in 0usize..20){
  let lib=library();let prefix=(0..n).map(|i|if i%3==0{"Leaf(1),"}else{"Leaf(\"ok\"),"}).collect::<String>();let first=validate(format!("root=Stack([{prefix}])").as_bytes(),&lib,&Options::default());let second=validate(format!("root=Stack([{prefix}Leaf(\"new\")])").as_bytes(),&lib,&Options::default());
  for (id,node)in &first.ir.nodes {let other=second.ir.nodes.get(id).unwrap();prop_assert_eq!(&other.path,&node.path);prop_assert_eq!(other.kind,node.kind);}
 }
}

#[test]
fn extension_cannot_mutate_or_remove_base_definitions() {
    let base = library();
    let mut b = base.extend();
    b.components.remove(0);
    assert_eq!(
        b.build().unwrap_err(),
        vec!["extension cannot modify base components"]
    );
    let mut b = base.extend();
    b.groups[0].notes = "Changed".into();
    assert_eq!(
        b.build().unwrap_err(),
        vec!["extension cannot modify base groups"]
    );
}

#[test]
fn rejected_version_stops_semantic_recovery() {
    let lib = library();
    let (_, r) = validate_message(
        b"```ui v=9\nroot=Leaf(\"ok\")\n```",
        &lib,
        &Options::default(),
    );
    assert_eq!(r.diagnostics.len(), 1);
    assert_eq!(r.diagnostics[0].code, Code::Version);
    assert!(r.ir.root.is_none());
}
#[test]
fn static_state_lists_are_capped_before_allocation() {
    let lib = library();
    let mut options = Options::default();
    options.limits.max_list_len = 1;
    let r = validate(b"$xs=[1,2]\nroot=Leaf(\"ok\")", &lib, &options);
    assert_eq!(keys(&r), vec![(Code::ListLen, Recovery::Truncated)]);
    assert!(matches!(&r.ir.states["xs"].default,Value::List(v)if v.len()==1));
}
#[test]
fn builder_examples_share_the_statement_parser() {
    let b = Library::builder("test", "0.1.0").group(GroupDef::new("G", 0, ""));
    let c = ComponentDef::new("Item", Role::Paragraph)
        .group("G")
        .doc("An item.")
        .req("text", T::Text)
        .example("root:=Item(\"ok\")")
        .example("$s=\"ok\"\nroot=Item($s)");
    assert!(b.component(c).build().is_ok());
}

#[test]
fn expression_sharing_preserves_recovered_null_metadata() {
    let mut ir = Ir::default();
    let literal = Expr::null();
    let recovered = Expr::recovered_null();
    assert_eq!(literal, recovered);
    let a = ir.intern(&literal);
    let b = ir.intern(&recovered);
    assert_ne!(a, b);
    assert_eq!(ir.intern(&literal), a);
    assert_eq!(ir.intern(&recovered), b);
}

#[test]
fn nested_container_aliases_remain_bounded_expression_plans() {
    let lib = library();
    let mut source = String::from("root=Leaf(text(len(v0)))\n");
    for i in 0..1200 {
        source.push_str(&format!("v{i}=[v{}]\n", i + 1));
    }
    source.push_str("v1200=1\n");
    let r = validate(source.as_bytes(), &lib, &Options::default());
    assert!(r.diagnostics.is_empty());
    assert!(r.ir.values["v0"].value.is_none());
    assert!(matches!(
        r.ir.nodes.values().next().unwrap().props.get(&PropId(0)),
        Some(Prop::Dynamic(_))
    ));
}

fn statement(source: &str) -> Statement {
    let mut parsed = parse_batch(source.as_bytes(), SyntaxLimits::default());
    assert!(
        parsed.diagnostics.is_empty(),
        "{source}: {:?}",
        parsed.diagnostics
    );
    parsed.program.statements.pop().expect("one statement")
}

fn retained_base() -> (Library, Options, Validation, ValidationCache) {
    let library = library();
    let options = Options::default();
    let parsed = parse_batch(
        b"root=Stack([Leaf(\"ok\"),keep])\nkeep=Leaf(\"k\")\n",
        options.limits.clone().bounded().syntax,
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let (base, _, cache) =
        Validator::new(&parsed.program, parsed.diagnostics, &library, &options).run_measured();
    (library, options, base, cache)
}

#[test]
fn unrelated_validation_accepts_only_independent_fresh_definitions() {
    let (library, options, base, cache) = retained_base();
    let dirty = BTreeSet::new();

    let value = statement("extra=1");
    let (checked, work, next) = validate_unrelated(
        &cache,
        &base,
        std::slice::from_ref(&value),
        &dirty,
        &library,
        &options,
    )
    .expect("an independent value is local");
    assert_eq!(work.statements_checked, 1, "{work:?}");
    assert!(next.table.contains_key("extra"));
    assert!(checked.ir.values.contains_key("extra"));
    assert!(!checked.ir.templates.contains_key("extra"));

    let node = statement("extra=Leaf(\"x\")");
    let (checked, _, next) = validate_unrelated(
        &cache,
        &base,
        std::slice::from_ref(&node),
        &dirty,
        &library,
        &options,
    )
    .expect("an independent node is local");
    assert!(checked.ir.templates.contains_key("extra"));
    assert!(!checked.ir.values.contains_key("extra"));
    assert!(
        checked.diagnostics.iter().all(|d| d.code == Code::Orphan),
        "{:?}",
        checked.diagnostics
    );
    assert!(next.table.contains_key("extra"));

    let action = statement("extra=say(\"hi\")");
    let (checked, _, next) = validate_unrelated(
        &cache,
        &base,
        std::slice::from_ref(&action),
        &dirty,
        &library,
        &options,
    )
    .expect("an independent action is local");
    assert!(next.action_sources.contains_key("extra"));
    assert!(next.checked["extra"].valid);
    assert!(next.table.contains_key("extra"));
    assert!(checked.ir.values.contains_key("extra"));
}

#[test]
fn unrelated_validation_falls_back_for_root_existing_state_and_dirty_globals() {
    let (library, options, base, cache) = retained_base();
    let fresh = statement("extra=1");
    let dirty = |name: &str| BTreeSet::from([name.to_string()]);

    // Fresh changes whose dirty closure touches a global or an existing name
    // must use the general maintenance path.
    for name in ["root", "keep", "$state"] {
        assert!(
            validate_unrelated(
                &cache,
                &base,
                std::slice::from_ref(&fresh),
                &dirty(name),
                &library,
                &options,
            )
            .is_none(),
            "dirty {name} must fall back"
        );
    }

    // A changed root, an existing definition, and every statement kind other
    // than Bind are likewise not local additions.
    for change in [
        statement("root=Leaf(\"new\")"),
        statement("keep=2"),
        statement("$s=1"),
        statement("query q=tool(\"known\",{})"),
    ] {
        assert!(
            validate_unrelated(
                &cache,
                &base,
                std::slice::from_ref(&change),
                &dirty("missing"),
                &library,
                &options,
            )
            .is_none(),
            "{:?} must fall back",
            (change.kind, change.name)
        );
    }
}

#[test]
fn unrelated_validation_rejects_changes_that_depend_on_each_other() {
    let (library, options, base, cache) = retained_base();
    let dirty = BTreeSet::new();
    for dependent in ["x=-v", "x=v+1", "x=v[0]"] {
        let changes = [statement("v=1"), statement(dependent)];
        assert!(
            validate_unrelated(&cache, &base, &changes, &dirty, &library, &options).is_none(),
            "{dependent} depends on the other change"
        );
    }
}

#[test]
fn unrelated_validation_partitions_owned_diagnostics_exactly() {
    let (library, options, base, cache) = retained_base();
    let dirty = BTreeSet::new();
    let value = statement("extra=upper(1)");
    let (checked, work, next) = validate_unrelated(
        &cache,
        &base,
        std::slice::from_ref(&value),
        &dirty,
        &library,
        &options,
    )
    .expect("a fresh scalar is local");
    assert_eq!(
        next.value_diagnostics["extra"]
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec![Code::ExprType],
        "{:?}",
        checked.diagnostics
    );
    assert_eq!(
        next.orphan_diagnostics["extra"]
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec![Code::Orphan]
    );
    assert!(work.diagnostics_examined > 0);
    assert_eq!(
        work.diagnostics_examined,
        checked
            .diagnostics
            .iter()
            .filter(|d| d.stmt.as_deref() == Some("extra"))
            .count() as u64
    );

    let node = statement("extra=Leaf()");
    let (_, work, next) = validate_unrelated(
        &cache,
        &base,
        std::slice::from_ref(&node),
        &dirty,
        &library,
        &options,
    )
    .expect("a fresh failing node is local");
    assert_eq!(
        next.orphan_diagnostics["extra"]
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec![Code::MissingRequired, Code::Orphan]
    );
    assert!(work.diagnostics_examined > 0);
}

#[test]
fn node_order_reads_the_retained_instance_context() {
    let (_, _, base, cache) = retained_base();
    let root = base.ir.root.expect("root instance");
    assert_eq!(cache.node_order(&root), Some(&Vec::new()));
    let child = base.ir.nodes[&root].slots[0].children[0];
    assert_eq!(
        cache.node_order(&child),
        Some(&vec![("children".to_string(), 0, 0)])
    );
}

#[test]
fn kind_inference_charges_each_table_entry_and_queue_pop() {
    let library = library();
    let options = Options::default();
    let parsed = parse_batch(
        b"root=Leaf(\"ok\")\na=1\nb=2\nc=3\n",
        options.limits.clone().bounded().syntax,
    );
    let mut validator = Validator::new(&parsed.program, parsed.diagnostics, &library, &options);
    validator.work.statements_checked = 0;
    validator.infer_kinds();
    assert_eq!(
        validator.work.statements_checked,
        2 * validator.table.len() as u64
    );
}

#[test]
fn reachability_walks_each_name_exactly_once() {
    let library = library();
    let options = Options::default();
    let parsed = parse_batch(
        b"root=Stack([a])\na=Leaf(b)\nb=\"done\"\nc=\"unused\"\n",
        options.limits.clone().bounded().syntax,
    );
    let mut validator = Validator::new(&parsed.program, parsed.diagnostics, &library, &options);
    validator.work.statements_checked = 0;
    validator.mark_reachable();
    // root, a, b are reached once each; c is never enqueued.
    assert_eq!(validator.work.statements_checked, 3);
    assert_eq!(
        validator.reachable.iter().cloned().collect::<Vec<_>>(),
        vec!["a", "b", "root"]
    );
}

#[test]
fn state_accounting_charges_each_source_without_a_reconcile_visit() {
    let library = library();
    let options = Options::default();
    let parsed = parse_batch(
        b"$x=unknown()\nroot=Leaf(\"ok\")\n",
        options.limits.clone().bounded().syntax,
    );
    assert_eq!(parsed.program.statements.len(), 2);
    let mut validator = Validator::new(&parsed.program, parsed.diagnostics, &library, &options);
    validator.work.statements_checked = 0;
    validator.states();
    assert_eq!(validator.work.statements_checked, 1);
}

#[test]
fn batch_validation_reports_exact_source_and_diagnostic_work() {
    let library = library();
    let options = Options::default();
    let parsed = parse_batch(
        b"root=Stack([Leaf(\"ok\"),keep])\nkeep=Leaf(\"k\")\n",
        options.limits.clone().bounded().syntax,
    );
    let (validation, work, cache) =
        Validator::new(&parsed.program, parsed.diagnostics, &library, &options).run_measured();
    assert!(validation.diagnostics.is_empty());
    assert_eq!(work.statements_checked, 16, "{work:?}");
    assert_eq!(work.diagnostics_examined, 0, "{work:?}");
    assert_eq!(work.instances_examined, 3, "{work:?}");
    assert!(!cache.state_limited);
}

#[test]
fn unrelated_validation_rejects_a_root_name_even_without_a_retained_root() {
    let library = library();
    let options = Options::default();
    let parsed = parse_batch(
        b"keep=Leaf(\"k\")\n",
        options.limits.clone().bounded().syntax,
    );
    let (base, _, cache) =
        Validator::new(&parsed.program, parsed.diagnostics, &library, &options).run_measured();
    assert!(base.ir.root.is_none());
    let fresh = statement("extra=1");
    let root = statement("root=Leaf(\"ok\")");
    assert!(
        validate_unrelated(
            &cache,
            &base,
            std::slice::from_ref(&root),
            &BTreeSet::new(),
            &library,
            &options,
        )
        .is_none(),
        "a new root is not an unrelated definition"
    );
    let dirty = BTreeSet::from(["root".to_string()]);
    assert!(
        validate_unrelated(
            &cache,
            &base,
            std::slice::from_ref(&fresh),
            &dirty,
            &library,
            &options,
        )
        .is_none(),
        "dirty {dirty:?}"
    );
}

#[test]
fn state_limited_tracks_only_state_size_diagnostics() {
    let library = library();
    let options = Options::default();
    let parsed = parse_batch(b"root=Leaf()\n", options.limits.clone().bounded().syntax);
    let (validation, work, cache) =
        Validator::new(&parsed.program, parsed.diagnostics, &library, &options).run_measured();
    assert!(
        validation
            .diagnostics
            .iter()
            .any(|d| d.code == Code::MissingRequired)
    );
    assert_eq!(work.diagnostics_examined, 2, "{work:?}");
    assert!(!cache.state_limited);

    let mut small = Options::default();
    small.limits.max_state_bytes = 1;
    let parsed = parse_batch(
        b"$x=\"too long\"\nroot=Leaf(\"ok\")\n",
        small.limits.clone().bounded().syntax,
    );
    let (validation, _, cache) =
        Validator::new(&parsed.program, parsed.diagnostics, &library, &small).run_measured();
    assert!(
        validation
            .diagnostics
            .iter()
            .any(|d| d.code == Code::StateSize)
    );
    assert!(cache.state_limited);
}

#[test]
fn state_and_query_accounting_never_underflows_from_zero() {
    let mut options = Options::default();
    options.tools.insert("known".into());
    let lib = library();
    let parsed = parse_batch(
        b"$x=null\nquery q=tool(\"known\",{},default:[])\n",
        options.limits.syntax.clone(),
    );
    assert_eq!(
        parsed.program.statements.len(),
        2,
        "{:?}",
        parsed
            .program
            .statements
            .iter()
            .map(|s| (&s.name, s.kind))
            .collect::<Vec<_>>()
    );
    let mut validator = Validator::new(&parsed.program, parsed.diagnostics, &lib, &options);
    validator.work.statements_checked = 0;
    validator.states();
    assert!(
        validator.work.statements_checked > 0,
        "state accounting must grow from zero"
    );
    validator.work.statements_checked = 0;
    assert_eq!(
        validator.table.len(),
        1,
        "query table: {:?}",
        validator.table.keys().collect::<Vec<_>>()
    );
    validator.queries();
    assert!(
        validator.work.statements_checked > 0,
        "query accounting must grow from zero"
    );
}
