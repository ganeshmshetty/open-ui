#![no_main]
use genui_core::syntax::extract::{Extractor, Segment, extract};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let expected = extract(data);
    let mut scanner = Extractor::default();
    for part in data.chunks(1) {
        scanner.push(part);
    }
    let actual = scanner.finish();
    assert!(
        actual
            .segments
            .iter()
            .filter(|s| matches!(s, Segment::Program(_)))
            .count()
            <= 1
    );
    assert_eq!(actual, expected);
});
