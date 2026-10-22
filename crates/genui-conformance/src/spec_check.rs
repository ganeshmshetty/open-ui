//! Markdown example extraction for SPEC §12.6. M1 parses syntax; library validation follows in M2.
use crate::Result;
use genui_core::{
    diag::{Code, Severity},
    validate::{Options, validate},
};
use std::{fs, path::Path};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExampleKind {
    Program,
    Fragment,
    Invalid,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Example {
    pub line: usize,
    pub kind: ExampleKind,
    pub source: String,
}

struct Fence {
    marker: char,
    width: usize,
    line: usize,
    kind: Option<ExampleKind>,
    source: String,
}

/// Honor surrounding Markdown fences, including four-backtick documentation blocks.
pub fn extract(markdown: &str) -> Result<Vec<Example>> {
    let mut examples = Vec::new();
    let mut open: Option<Fence> = None;
    for (index, line) in markdown.split_inclusive('\n').enumerate() {
        let trimmed = line.trim_start_matches(' ');
        let indent = line.len() - trimmed.len();
        let marker = trimmed.chars().next().unwrap_or(' ');
        let width = trimmed.chars().take_while(|&c| c == marker).count();
        let is_fence = indent <= 3 && matches!(marker, '`' | '~') && width >= 3;
        let info = if is_fence { &trimmed[width..] } else { "" };
        if let Some(fence) = &mut open {
            if is_fence && marker == fence.marker && width >= fence.width && info.trim().is_empty()
            {
                if let Some(kind) = fence.kind {
                    examples.push(Example {
                        line: fence.line,
                        kind,
                        source: std::mem::take(&mut fence.source),
                    });
                }
                open = None;
            } else if fence.kind.is_some() {
                fence.source.push_str(line);
            }
        } else if is_fence && !(marker == '`' && info.contains('`')) {
            let kind = match info.split_whitespace().next() {
                Some("ui") => Some(ExampleKind::Program),
                Some("ui-fragment") => Some(ExampleKind::Fragment),
                Some("ui-invalid") => Some(ExampleKind::Invalid),
                _ => None,
            };
            open = Some(Fence {
                marker,
                width,
                line: index + 1,
                kind,
                source: String::new(),
            });
        }
    }
    if let Some(fence) = open.filter(|f| f.kind.is_some()) {
        return Err(format!(
            "line {}: unterminated UI example fence",
            fence.line
        ));
    }
    Ok(examples)
}

#[derive(Debug, Default)]
pub struct SpecReport {
    pub programs: usize,
    pub fragments: usize,
    pub invalid: usize,
    pub exemplars: usize,
}

pub fn check_text(markdown: &str) -> Result<SpecReport> {
    let library = genui_std::std_library().map_err(|e| format!("std build: {e:?}"))?;
    let options: Options =
        serde_json::from_str(include_str!("../../../conformance/libs/host.json"))
            .map_err(|e| e.to_string())?;
    let examples = extract(markdown)?;
    let mut report = SpecReport::default();
    for example in examples {
        let result = validate(example.source.as_bytes(), &library, &options);
        match example.kind {
            ExampleKind::Program => report.programs += 1,
            ExampleKind::Fragment => report.fragments += 1,
            ExampleKind::Invalid => {
                report.invalid += 1;
                if result.diagnostics.is_empty() {
                    return Err(format!(
                        "line {}: ui-invalid example produced no diagnostics",
                        example.line
                    ));
                }
                continue;
            }
        }
        let bad: Vec<_> = result
            .diagnostics
            .iter()
            .filter(|d| {
                if example.kind == ExampleKind::Fragment
                    && matches!(
                        d.code,
                        Code::NoRoot | Code::Unresolved | Code::Orphan | Code::Undeclared
                    )
                {
                    return false;
                }
                d.severity != Severity::Info
            })
            .collect();
        if !bad.is_empty() {
            return Err(format!(
                "line {}: validation diagnostics in {:?} example: {:?}",
                example.line, example.kind, bad
            ));
        }
    }
    if report.programs == 0 {
        return Err("no complete UI examples found".into());
    }
    Ok(report)
}

pub fn check_file(path: &Path) -> Result<SpecReport> {
    let markdown = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut report = check_text(&markdown).map_err(|e| format!("{}: {e}", path.display()))?;
    let examples = path
        .parent()
        .and_then(Path::parent)
        .map(|p| p.join("examples"));
    if let Some(directory) = examples.filter(|p| p.is_dir()) {
        let entries = fs::read_dir(&directory)
            .map_err(|e| e.to_string())?
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        let mut paths: Vec<_> = entries
            .into_iter()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|s| s == "ui"))
            .collect();
        paths.sort();
        for file in paths {
            let source = fs::read_to_string(&file).map_err(|e| e.to_string())?;
            check_text(&format!("```ui\n{source}\n```\n"))
                .map_err(|e| format!("{}: {e}", file.display()))?;
            report.exemplars += 1;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_spec_parses() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/SPEC.md");
        let report = check_file(&path).unwrap();
        assert_eq!(report.programs, 3);
        assert_eq!(report.fragments, 4);
    }

    #[test]
    fn extracts_only_ui_fences_outside_other_fences() {
        let text = "````text\n```ui\n@\n```\n````\n```rust\n@\n```\n  ~~~ui v=1\nroot=Text(\"x\")\n  ~~~~\n```ui-fragment\na=1\n```\n```ui-invalid\n@\n```\n";
        let report = check_text(text).unwrap();
        assert_eq!(
            (report.programs, report.fragments, report.invalid),
            (1, 1, 1)
        );
    }

    #[test]
    fn bad_examples_and_unclosed_fences_fail() {
        assert!(check_text("```ui\n@\n```\n").is_err());
        assert!(check_text("```ui\nroot=Divider()\n").is_err());
        assert!(check_text("```ui\nroot=Divider()\n```\n```ui-fragment\n@\n```\n").is_err());
        assert!(check_text("```ui-fragment\nx=1\n```\n").is_err());
    }

    #[test]
    fn unicode_prose_does_not_panic() {
        assert!(check_text("😀😀 prose\n```ui\nroot=Divider()\n```\n").is_ok());
    }
}

#[cfg(test)]
mod syntax_gate_tests {
    use super::check_text;

    #[test]
    fn lexically_valid_syntax_errors_fail_complete_and_fragment_examples() {
        assert!(check_text("```ui\nroot={key 1}\n```\n").is_err());
        assert!(
            check_text("```ui\nroot=Divider()\n```\n```ui-fragment\nx={key 1}\n```\n").is_err()
        );
    }

    #[test]
    fn invalid_tag_requires_actual_diagnostics() {
        assert!(
            check_text("```ui\nroot=Divider()\n```\n```ui-invalid\nroot=Divider()\n```\n").is_err()
        );
        let report =
            check_text("```ui\nroot=Divider()\n```\n```ui-invalid\nx={key 1}\n```\n").unwrap();
        assert_eq!(report.invalid, 1);
    }
}
