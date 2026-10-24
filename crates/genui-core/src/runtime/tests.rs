use super::*;

fn library() -> Library {
    crate::r#lib::Library::builder("test", "0.1.0")
        .group(crate::r#lib::GroupDef::new("G", 0, "`Leaf`."))
        .component(
            crate::r#lib::ComponentDef::new("Leaf", crate::ir::Role::Paragraph)
                .group("G")
                .doc("A leaf.")
                .req("text", T::text())
                .req("value", T::scalar()),
        )
        .build()
        .unwrap()
}

#[test]
fn arithmetic_uses_checked_integer_and_finite_float_rules() {
    assert_eq!(format_float(12.0), "12");
    assert_eq!(format_float(2.5), "2.5");
    assert_eq!(format_float(0.00123), "0.00123");
    assert_eq!(format_float(1e20), "1e+20");
    assert_eq!(format_float(-1.5e-7), "-1.5e-7");
    assert_eq!(format!("{}", 2.675_f64), "2.675");
}

#[test]
fn evaluation_is_deterministic_and_does_not_mutate_state() {
    let library = library();
    let result = crate::validate::validate(
        b"$n=2\nroot=Leaf(text($n+1),$n)",
        &library,
        &crate::validate::Options::default(),
    );
    let state = StateStore::from_ir(&result.ir);
    let a = evaluate_with_state(&result.ir, &library, &Limits::default(), &state);
    let b = evaluate_with_state(&result.ir, &library, &Limits::default(), &state);
    assert_eq!(a, b);
    assert_eq!(state.value("n"), Some(&Value::Int(2)));
}

#[test]
fn state_writes_are_typed_and_size_accounted() {
    let library = library();
    let result = crate::validate::validate(
        b"$n=2\nroot=Leaf(\"x\",$n)",
        &library,
        &crate::validate::Options::default(),
    );
    let mut state = StateStore::from_ir(&result.ir);
    let mut limits = Limits::default();
    assert!(
        state
            .write("n", Value::Int(3), StateOrigin::Action, &limits, &library)
            .is_ok()
    );
    assert_eq!(state.value("n"), Some(&Value::Int(3)));
    assert_eq!(
        state.write(
            "n",
            Value::from("no"),
            StateOrigin::Action,
            &limits,
            &library
        ),
        Err(Code::TypeWrite)
    );
    limits.max_state_bytes = 0;
    assert_eq!(
        state.write("n", Value::Int(4), StateOrigin::Action, &limits, &library),
        Err(Code::StateSize)
    );
}

#[test]
fn format_float_matches_positional_and_exponent_rules() {
    assert_eq!(format_float(0.0), "0");
    assert_eq!(format_float(123.0), "123");
    assert_eq!(format_float(0.00001), "0.00001");
    assert_eq!(format_float(0.000001), "1e-6");
    assert_eq!(format_float(1e16), "10000000000000000");
    assert_eq!(format_float(1e17), "1e+17");
    assert_eq!(format_float(1e15), "1000000000000000");
}

fn session_library() -> Library {
    crate::r#lib::Library::builder("test-actions", "0.1.0")
        .group(crate::r#lib::GroupDef::new("G", 0, "Test components."))
        .component(
            crate::r#lib::ComponentDef::new("Text", crate::ir::Role::Paragraph)
                .group("G")
                .doc("Text.")
                .req("text", T::text()),
        )
        .component(
            crate::r#lib::ComponentDef::new("Stack", crate::ir::Role::Group)
                .group("G")
                .doc("Stack.")
                .req("children", T::nodes()),
        )
        .component(
            crate::r#lib::ComponentDef::new("Control", crate::ir::Role::Button)
                .group("G")
                .doc("Control.")
                .opt_none("on_click", T::action()),
        )
        .build()
        .unwrap()
}

fn text_prop(session: &crate::Session) -> Option<String> {
    let tree = session.tree().canonical_tree();
    tree["children"]["children"][0]["props"]["text"]
        .as_str()
        .map(str::to_string)
}

fn action_session(source: &str) -> crate::Session {
    let mut session = crate::Session::new(crate::SessionOptions {
        library: std::sync::Arc::new(session_library()),
        validation: crate::validate::Options::default(),
        input: crate::session::InputMode::Program,
        prompt_digest: String::new(),
    });
    session.push(source.as_bytes());
    session.finish();
    session
}

#[test]
fn actions_are_inert_until_an_explicit_epoch_bound_activation() {
    let source = "$n=1\ngo=[set($n,2)]\nroot=Stack([Text(text($n)),Control(on_click:go)])";
    let mut session = action_session(source);
    assert_eq!(text_prop(&session).as_deref(), Some("1"));
    let path = crate::ir::NodePath("root/children:#1".into());
    let handle = session.action_handle(&path, "on_click").unwrap();
    let node = session
        .ir()
        .nodes
        .values()
        .find(|node| node.path == path)
        .unwrap()
        .id;
    let other = session
        .ir()
        .nodes
        .values()
        .find(|node| node.path != path)
        .unwrap()
        .id;
    let ignored = session.activate(other, &handle);
    assert!(ignored.patches.is_none());
    assert_eq!(text_prop(&session).as_deref(), Some("1"));
    let update = session.activate(node, &handle);
    assert!(update.patches.is_some());
    assert_eq!(text_prop(&session).as_deref(), Some("2"));

    session.reset();
    session.push(source.as_bytes());
    session.finish();
    let stale = session.activate(node, &handle);
    assert!(stale.patches.is_none());
    assert_eq!(text_prop(&session).as_deref(), Some("1"));
}

#[test]
fn actions_are_gated_until_finish() {
    let source = "$n=1\ngo=[set($n,2)]\nroot=Stack([Text(text($n)),Control(on_click:go)])";
    let mut session = crate::Session::new(crate::SessionOptions {
        library: std::sync::Arc::new(session_library()),
        validation: crate::validate::Options::default(),
        input: crate::session::InputMode::Program,
        prompt_digest: String::new(),
    });
    session.push(source.as_bytes());
    let path = crate::ir::NodePath("root/children:#1".into());
    assert!(session.action_handle(&path, "on_click").is_none());
    session.finish();
    assert_eq!(text_prop(&session).as_deref(), Some("1"));
    let handle = session.action_handle(&path, "on_click").unwrap();
    let node = session
        .ir()
        .nodes
        .values()
        .find(|node| node.path == path)
        .unwrap()
        .id;
    session.activate(node, &handle);
    assert_eq!(text_prop(&session).as_deref(), Some("2"));
}

#[test]
fn data_byte_limits_use_canonical_semantic_bytes() {
    assert_eq!(value_bytes(&Value::Int(2)), r#"{"i":2}"#.len());
    assert_eq!(value_bytes(&Value::Float(2.0)), r#"{"f":2.0}"#.len());
    let map = Value::Map(std::sync::Arc::new(vec![
        ("z".into(), Value::Int(1)),
        ("a".into(), Value::from("x")),
    ]));
    assert_eq!(
        value_bytes(&map),
        crate::canonical::encode(&map.ir_json()).len()
    );
    assert!(value_bytes(&map) > serde_json::to_vec(&map).unwrap().len());
}

#[test]
fn request_and_event_text_is_sanitized() {
    let value = Value::Str(crate::value::PStr {
        text: "a\u{202e}b\u{200b}c".into(),
        prov: Provenance::Mixed,
    });
    let sanitized = sanitize_value(value);
    assert!(
        matches!(&sanitized, Value::Str(text) if text.text.as_ref() == "abc" && text.prov == Provenance::Mixed)
    );
    let list = Value::List(std::sync::Arc::new(vec![
        Value::from("x\u{2066}y"),
        Value::Map(std::sync::Arc::new(vec![(
            "k\u{2060}".into(),
            Value::from("z\u{200d}"),
        )])),
    ]));
    let sanitized = sanitize_value(list);
    assert_eq!(
        crate::canonical::encode(&sanitized.ir_json()),
        r#"["xy",{"map":[["k","z"]]}]"#
    );
}
