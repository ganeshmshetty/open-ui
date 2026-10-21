#![no_main]
use genui_core::syntax::{Lexer, lex};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let expected = lex(data);
    let mut scanner = Lexer::default();
    for part in data.chunks(1) {
        scanner.push(part);
    }
    assert_eq!(scanner.finish(), expected);
});
