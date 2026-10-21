#![no_main]
use genui_core::{
    limits::SyntaxLimits,
    syntax::{BatchParser, parse_batch, printer::print},
};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let expected = parse_batch(data, SyntaxLimits::default());
    let mut parser = BatchParser::default();
    for part in data.chunks(7) {
        parser.push(part);
    }
    assert_eq!(parser.finish(), expected);
    let limits = SyntaxLimits {
        max_program_bytes: 4 * 1024 * 1024,
        max_stmt_bytes: 64 * 1024,
        max_expr_depth: 96,
        max_bracket_depth: 64,
        ..SyntaxLimits::default()
    };
    let canonical = print(&expected.program);
    assert_eq!(
        parse_batch(canonical.as_bytes(), limits).program,
        expected.program
    );
});
