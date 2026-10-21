#![no_main]
use genui_core::{
    limits::SyntaxLimits,
    syntax::splitter::{Splitter, split},
};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let limits = SyntaxLimits {
        max_stmt_bytes: data.first().map_or(256, |n| usize::from(*n)),
        max_bracket_depth: data.get(1).map_or(8, |n| usize::from(*n) % 9),
        ..SyntaxLimits::default()
    };
    let expected = split(data, limits.clone());
    let mut scanner = Splitter::new(limits);
    for part in data.chunks(1) {
        scanner.push(part);
    }
    assert_eq!(scanner.finish(), expected);
});
