use std::{fs, path::Path, process::Command};

#[test]
fn tree_output_and_exit_status_match_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tree");
    for (case, success) in [
        ("repeated", true),
        ("dynamic", true),
        ("invalid", false),
        ("empty", false),
        ("root-plan", true),
        ("binding", true),
        ("warning", false),
        ("info", true),
    ] {
        let case = root.join(case);
        let output = Command::new(env!("CARGO_BIN_EXE_genui-cli"))
            .arg("tree")
            .arg(case.join("input.ui"))
            .output()
            .unwrap();
        assert_eq!(output.status.success(), success, "{}", case.display());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            fs::read_to_string(case.join("want.txt")).unwrap(),
            "{}",
            case.display()
        );
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn tree_requires_one_readable_path() {
    for args in [
        vec!["tree"],
        vec!["tree", "missing.ui"],
        vec!["tree", "a", "b"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_genui-cli"))
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .starts_with("error: ")
        );
    }
}
