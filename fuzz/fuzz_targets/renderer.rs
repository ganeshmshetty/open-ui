#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // TODO(M6): replace this scaffold with the renderer entry point.
    let _ = data;
});
