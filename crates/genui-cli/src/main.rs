//! Developer entry points (SPEC §1.2 and §12.6).
#![forbid(unsafe_code)]

use genui_conformance::{run_suite, spec_check};
use std::{env, path::PathBuf, process::ExitCode};

fn run(args: Vec<String>) -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let Some(command) = args.first() else {
        return Err("usage: genui-cli <fixtures [directory] | spec-check [SPEC.md]>".into());
    };
    if args.len() > 2 {
        return Err("expected at most one path argument".into());
    }
    match command.as_str() {
        "fixtures" => {
            let path = args
                .get(1)
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("conformance"));
            let report = run_suite(&path)?;
            println!(
                "PASS: {} lex fixtures, {} chunkings (M0 finish-only lexical checks).",
                report.fixtures, report.chunkings
            );
        }
        "spec-check" => {
            let path = args
                .get(1)
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("spec/SPEC.md"));
            let report = spec_check::check_file(&path)?;
            println!(
                "PASS: {} ui programs and {} ui fragments lex without diagnostics; {} ui-invalid blocks recognized (M0 lexical checks; semantic validation begins in M2).",
                report.programs, report.fragments, report.invalid
            );
        }
        _ => {
            return Err(format!(
                "unknown command {command:?}; M0 supports fixtures and spec-check"
            ));
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_arguments_fail() {
        assert!(run(vec![]).is_err());
        assert!(run(vec!["parse".into()]).is_err());
        assert!(run(vec!["fixtures".into(), "a".into(), "b".into()]).is_err());
        assert!(run(vec!["fixtures".into(), "does-not-exist".into()]).is_err());
        assert!(run(vec!["spec-check".into(), "does-not-exist".into()]).is_err());
    }
}
