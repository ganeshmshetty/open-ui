#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // TODO(M4): replace this scaffold with the evaluator entry point.
    let _ = data;
});
