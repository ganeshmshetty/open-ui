//! Developer entry points (SPEC §1.2 and §12.6).
#![forbid(unsafe_code)]

mod prompt;
mod replay;
mod transcripts;
mod tree;

use genui_conformance::{run_suite, spec_check};
use std::{env, path::PathBuf, process::ExitCode};

fn run(args: Vec<String>) -> Result<bool, String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let Some(command) = args.first() else {
        return Err(
            "usage: genui-cli <fixtures [directory] | spec-check [SPEC.md] | tree <file.ui> | replay <file.ui> [--chunk N] | prompt [options.json] | feedback <file.ui> | message <file.ui> [digest] [--cut] [--replay] | transcripts [directory] [--record|--verify]>"
                .into(),
        );
    };
    if command == "replay" {
        return replay::run(&args[1..]);
    }
    if command == "transcripts" {
        return transcripts::run(&args[1..]);
    }
    if command == "prompt" {
        let mut path = None;
        let mut digest = false;
        for arg in &args[1..] {
            match arg.as_str() {
                "--digest" => digest = true,
                other if other.starts_with("--") => {
                    return Err(format!("unknown prompt option {other:?}"));
                }
                other if path.is_none() => path = Some(other),
                other => return Err(format!("unexpected prompt argument {other:?}")),
            }
        }
        let prompt = prompt::prompt(path)?;
        if digest {
            println!("{}", prompt.digest);
        } else {
            print!("{}", prompt.text);
        }
        return Ok(true);
    }
    if command == "feedback" {
        if args.len() != 2 {
            return Err("usage: genui-cli feedback <file.ui>".into());
        }
        let (output, success) = prompt::diagnostics(&PathBuf::from(&args[1]))?;
        print!("{output}");
        return Ok(success);
    }
    if command == "message" {
        let mut path = None;
        let mut digest = String::new();
        let mut finish = true;
        let mut replay = false;
        for arg in &args[1..] {
            match arg.as_str() {
                "--cut" => finish = false,
                "--replay" => replay = true,
                other if other.starts_with("--") => {
                    return Err(format!("unknown message option {other:?}"));
                }
                other if path.is_none() => path = Some(PathBuf::from(other)),
                other if digest.is_empty() => digest = other.into(),
                other => return Err(format!("unexpected message argument {other:?}")),
            }
        }
        let path = path.ok_or("usage: genui-cli message <file.ui> [digest] [--cut] [--replay]")?;
        print!("{}", prompt::message(&path, &digest, finish, replay)?);
        return Ok(true);
    }
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
                "PASS: {} fixtures, {} chunkings, {} seeded permutations (live M3 invariants, static validation and M5 message goldens).",
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
                "unknown command {command:?}; M8 supports fixtures, spec-check, tree, replay, prompt, feedback, message and transcripts"
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
        assert!(run(vec!["feedback".into()]).is_err());
        assert!(run(vec!["message".into()]).is_err());
        assert!(run(vec!["message".into(), "a".into(), "--bogus".into()]).is_err());
        assert!(run(vec!["prompt".into(), "a".into(), "b".into()]).is_err());
    }

    #[test]
    fn m5_commands_run() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        assert!(run(vec!["prompt".into()]).unwrap());
        assert!(
            run(vec![
                "feedback".into(),
                root.join("conformance/feedback/001-mixed/input.ui")
                    .display()
                    .to_string()
            ])
            .unwrap()
        );
        assert!(
            run(vec![
                "message".into(),
                root.join("conformance/message/001-fenced-reply/input.ui")
                    .display()
                    .to_string(),
                "--replay".into()
            ])
            .unwrap()
        );
    }
}
