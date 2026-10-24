#![no_main]
use genui_core::validate::Options;
use libfuzzer_sys::fuzz_target;
use std::sync::{Arc, OnceLock};
fuzz_target!(|data: &[u8]| {
    static LIB: OnceLock<Arc<genui_core::r#lib::Library>> = OnceLock::new();
    let library = LIB.get_or_init(|| Arc::new(genui_std::std_library().unwrap()));
    let mut seed = data.iter().fold(0x32751u64, |s, b| {
        s.wrapping_mul(31).wrapping_add(*b as u64)
    });
    let mut options = Options::default();
    options.limits.syntax.max_stmt_bytes = (seed % 1024) as usize;
    options.limits.syntax.max_expr_depth = (seed % 49) as usize;
    options.limits.max_nodes = (seed % 100) as usize;
    options.limits.max_ir_depth = (seed % 25) as usize;
    options.limits.max_state_bytes = (seed % 1024) as usize;
    options.limits.max_queries = (seed % 9) as usize;
    options.limits.syntax.max_program_bytes = (seed % 32769) as usize;
    options.tools.insert("known".into());
    options.refs.insert("t.data".into());
    let mut offset = 0;
    let mut parts = Vec::new();
    while offset < data.len() {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let end = (offset + 1 + (seed % 97) as usize).min(data.len());
        parts.push(&data[offset..end]);
        offset = end;
    }
    for message in [false, true] {
        genui_conformance::streaming::check(
            data,
            &parts,
            library.clone(),
            options.clone(),
            message,
        )
        .unwrap();
    }
});
