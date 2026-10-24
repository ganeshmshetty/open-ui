//! SPEC §10.8 panic containment exercised on the GPUI main thread through the
//! gallery self-test (headless capture needs the process main thread).
#![cfg(feature = "screenshot")]

#[test]
fn a_panicking_builder_shows_a_placeholder_and_quarantines() {
    let gallery = env!("CARGO_BIN_EXE_gallery");
    let status = std::process::Command::new(gallery)
        .arg("--self-test-panic")
        .status()
        .expect("run gallery panic self-test");
    assert!(status.success(), "panic self-test failed: {status}");
}
