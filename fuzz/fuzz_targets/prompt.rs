#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // TODO(M5): replace this scaffold with the prompt entry point.
    let _ = data;
});
