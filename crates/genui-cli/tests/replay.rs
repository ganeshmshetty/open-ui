use std::{fs, path::Path, process::Command};
#[test]
fn replay_exact_output_and_bad_chunk_arguments() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/replay/basic");
    let output = Command::new(env!("CARGO_BIN_EXE_genui-cli"))
        .arg("replay")
        .arg(root.join("input.ui"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        fs::read_to_string(root.join("want.txt")).unwrap()
    );
    for args in [
        vec!["replay"],
        vec!["replay", "missing.ui"],
        vec!["replay", "file", "--chunk", "0"],
        vec!["replay", "file", "--chunk", "no"],
        vec!["replay", "file", "--chunk"],
        vec!["replay", "file", "--bad", "2"],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_genui-cli"))
            .args(args)
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(
            String::from_utf8(out.stderr)
                .unwrap()
                .starts_with("error: ")
        );
    }
}
