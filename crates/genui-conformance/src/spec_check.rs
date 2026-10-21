//! Markdown example extraction for SPEC §12.6. Validation is lexical until M2.
use crate::Result;
use genui_core::syntax::lex;
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
}

pub fn check_text(markdown: &str) -> Result<SpecReport> {
    let examples = extract(markdown)?;
    let mut report = SpecReport::default();
    for example in examples {
        let result = lex(example.source.as_bytes());
        match example.kind {
            ExampleKind::Program => report.programs += 1,
            ExampleKind::Fragment => report.fragments += 1,
            ExampleKind::Invalid => {
                report.invalid += 1;
                continue;
            }
        }
        if !result.diagnostics.is_empty() {
            return Err(format!(
                "line {}: lexical diagnostics in {:?} example: {:?}",
                example.line, example.kind, result.diagnostics
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
    check_text(&markdown).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_spec_lexes() {
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
        assert!(check_text("```ui\nroot=1\n").is_err());
        assert!(check_text("```ui\nroot=1\n```\n```ui-fragment\n@\n```\n").is_err());
        assert!(check_text("```ui-fragment\nx=1\n```\n").is_err());
    }

    #[test]
    fn unicode_prose_does_not_panic() {
        assert!(check_text("😀😀 prose\n```ui\nroot=1\n```\n").is_ok());
    }
}
