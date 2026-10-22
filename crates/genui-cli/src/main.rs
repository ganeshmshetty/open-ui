//! Developer entry points (SPEC §1.2 and §12.6).
#![forbid(unsafe_code)]

mod tree;

use genui_conformance::{run_suite, spec_check};
use std::{env, path::PathBuf, process::ExitCode};

fn run(args: Vec<String>) -> Result<bool, String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let Some(command) = args.first() else {
        return Err(
            "usage: genui-cli <fixtures [directory] | spec-check [SPEC.md] | tree <file.ui>>"
                .into(),
        );
    };
    if args.len() > 2 {
        return Err("expected at most one path argument".into());
    }
    match command.as_str() {
        "tree" => {
            let path = args.get(1).ok_or("usage: genui-cli tree <file.ui>")?;
            let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            let library = genui_std::std_library().map_err(|e| format!("std build: {e:?}"))?;
            let (output, success) = tree::inspect(&bytes, &library)?;
            print!("{output}");
            return Ok(success);
        }
        "fixtures" => {
            let path = args
                .get(1)
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("conformance"));
            let report = run_suite(&path)?;
            println!(
                "PASS: {} fixtures, {} chunkings, {} seeded permutations (M2 static validation).",
                report.fixtures, report.chunkings, report.permutations
            );
            for (category, count) in &report.categories {
                println!(
                    "  {category}: {count} fixtures, {} chunkings",
                    report
                        .category_chunkings
                        .get(category)
                        .copied()
                        .unwrap_or(0)
                );
            }
        }
        "spec-check" => {
            let path = args
                .get(1)
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("spec/SPEC.md"));
            let report = spec_check::check_file(&path)?;
            println!(
                "PASS: {} ui programs and {} ui fragments validate; {} ui-invalid blocks rejected; {} exemplar programs validate.",
                report.programs, report.fragments, report.invalid, report.exemplars
            );
        }
        _ => {
            return Err(format!(
                "unknown command {command:?}; M2 supports fixtures, spec-check and tree"
            ));
        }
    }
    Ok(true)
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
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
