#![no_main]
use genui_core::{
    ir::NodeId,
    syntax::{BatchParser, parse_batch},
    validate::{Options, validate, validate_program},
};
use libfuzzer_sys::fuzz_target;
use std::{collections::BTreeSet, sync::OnceLock};

fuzz_target!(|data: &[u8]| {
    static LIB: OnceLock<genui_core::r#lib::Library> = OnceLock::new();
    let library = LIB.get_or_init(|| match genui_std::std_library() {
        Ok(library) => library,
        Err(errors) => panic!("invalid trusted std library: {errors:?}"),
    });
    let seed = data.first().copied().unwrap_or(0) as usize;
    let mut options = Options::default();
    options.limits.max_nodes = seed;
    options.limits.max_ir_depth = seed % 49;
    options.limits.max_list_len = seed * 2;
    options.limits.max_state_bytes = seed * 16;
    options.limits.max_queries = seed % 9;
    options.tools = [
        "sales.by_month".into(),
        "tickets.list".into(),
        "known".into(),
    ]
    .into();
    options.refs = ["t4.places".into(), "t4.orders".into()].into();
    options.url_hosts = ["example.com".into()].into();
    if seed.is_multiple_of(3) {
        options.capabilities.clear();
    }
    let expected = validate(data, library, &options);
    let mut parser = BatchParser::new(options.limits.syntax.clone());
    for part in data.chunks(7) {
        parser.push(part);
    }
    let parsed = parser.finish();
    assert_eq!(
        validate_program(&parsed.program, parsed.diagnostics, library, &options),
        expected
    );
    let mut parsed = parse_batch(data, options.limits.syntax.clone());
    let mut positions: Vec<_> = parsed
        .program
        .statements
        .iter()
        .enumerate()
        .filter(|(_, s)| s.name != "root")
        .map(|(i, _)| i)
        .collect();
    let mut statements: Vec<_> = positions
        .iter()
        .map(|i| parsed.program.statements[*i].clone())
        .collect();
    positions.sort_unstable();
    statements.reverse();
    for (i, statement) in positions.into_iter().zip(statements) {
        parsed.program.statements[i] = statement;
    }
    assert_eq!(
        validate_program(&parsed.program, parsed.diagnostics, library, &options),
        expected
    );
    // Every concrete node belongs to the one tree, stays within budget, and has unique identity.
    let mut pending: Vec<(NodeId, usize)> =
        expected.ir.root.map(|id| (id, 1)).into_iter().collect();
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    while let Some((id, depth)) = pending.pop() {
        assert!(ids.insert(id));
        let node = &expected.ir.nodes[&id];
        assert!(paths.insert(node.path.clone()));
        assert!(depth <= options.limits.max_ir_depth);
        assert!(node.path.0 == "root" || node.path.0.starts_with("root/"));
        for slot in &node.slots {
            pending.extend(slot.children.iter().map(|id| (*id, depth + 1)));
        }
    }
    assert_eq!(ids.len(), expected.ir.nodes.len());
    assert!(ids.len() <= options.limits.max_nodes);
});
