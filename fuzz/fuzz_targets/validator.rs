#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // TODO(M2): replace this scaffold with the validator entry point.
    let _ = data;
});
