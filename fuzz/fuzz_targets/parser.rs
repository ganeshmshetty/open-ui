#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // TODO(M1): replace this scaffold with the parser entry point.
    let _ = data;
});
