//! SPEC §10.1 freshness: one batch applied before a frame must be visible in
//! the capture with no further host trigger.
#![cfg(feature = "screenshot")]

#[test]
fn one_batch_is_visible_without_a_later_trigger() {
    let gallery = env!("CARGO_BIN_EXE_gallery");
    let status = std::process::Command::new(gallery)
        .arg("--self-test-freshness")
        .status()
        .expect("run gallery freshness self-test");
    assert!(status.success(), "freshness self-test failed: {status}");
}
