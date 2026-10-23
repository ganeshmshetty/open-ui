use super::*;
use crate::r#lib::{ActionDef, ComponentDef, FunctionDef, GroupDef, Role};
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
