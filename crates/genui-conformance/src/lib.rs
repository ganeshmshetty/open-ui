//! Exact M1 syntax conformance and byte-chunking checks (SPEC §12).
#![forbid(unsafe_code)]

pub mod chunks;
mod semantic;
pub mod spec_check;

use genui_core::diag::{Code, Diagnostic, Recovery};
use genui_core::{
    limits::SyntaxLimits,
    syntax::{
        LexBuffer, Token,
        ast::Program,
        extract::Segment,
        lex, parse_batch, parse_message,
        parser::{BatchParser, MessageParser},
        printer,
    },
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub title: String,
    pub tier: Tier,
    pub tags: Vec<String>,
    pub notes: String,
    #[serde(default)]
    pub limits: BTreeMap<String, u64>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    Core,
    Interactive,
    GpuiSmoke,
}

/// Diagnostic comparison ignores presentation and source locations (SPEC §12.2).
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(deny_unknown_fields)]
struct DiagnosticKey {
    code: Code,
    stmt: Option<String>,
    path: Option<String>,
    recovery: Recovery,
}

impl From<&Diagnostic> for DiagnosticKey {
    fn from(d: &Diagnostic) -> Self {
        Self {
            code: d.code,
            stmt: d.stmt.clone(),
            path: d.path.clone(),
            recovery: d.recovery,
        }
    }
}

struct Schemas {
    case: jsonschema::Validator,
    tokens: jsonschema::Validator,
    diagnostics: jsonschema::Validator,
    stream: jsonschema::Validator,
    ast: jsonschema::Validator,
    segments: jsonschema::Validator,
}

impl Schemas {
    fn load(root: &Path) -> Result<Self> {
        fn load(root: &Path, name: &str) -> Result<jsonschema::Validator> {
            let path = root.join("schemas").join(format!("{name}.schema.json"));
            let value: Value = read_json(&path)?;
            jsonschema::validator_for(&value).map_err(|e| format!("{}: {e}", path.display()))
        }
        Ok(Self {
            case: load(root, "case")?,
            tokens: load(root, "tokens")?,
            diagnostics: load(root, "diag")?,
            stream: load(root, "stream")?,
            ast: load(root, "ast")?,
            segments: load(root, "segments")?,
        })
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn checked_json<T: DeserializeOwned>(path: &Path, schema: &jsonschema::Validator) -> Result<T> {
    let value: Value = read_json(path)?;
    schema
        .validate(&value)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_value(value).map_err(|e| format!("{}: {e}", path.display()))
}

fn directories(path: &Path) -> Result<Vec<PathBuf>> {
    let mut directories = Vec::new();
    for entry in fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", path.display()))?;
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            directories.push(entry.path());
        }
    }
    directories.sort();
    Ok(directories)
}

#[derive(Debug, Default)]
pub struct SuiteReport {
    pub fixtures: usize,
    pub chunkings: usize,
    pub permutations: usize,
    pub category_chunkings: BTreeMap<String, usize>,
    pub categories: BTreeMap<String, usize>,
}

/// Fail on unsupported fixture categories rather than silently reporting success.
pub fn run_suite(root: &Path) -> Result<SuiteReport> {
    let schemas = Schemas::load(root)?;
    let semantic = semantic::Runner::new(root)?;
    let mut report = SuiteReport::default();
    let mut failures = Vec::new();
    for category in directories(root)? {
        if matches!(
            category.file_name().and_then(|s| s.to_str()),
            Some("schemas" | "libs")
        ) {
            continue;
        }
        for path in directories(&category)? {
            let category_name = category.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if !matches!(
                category_name,
                "lex"
                    | "grammar"
                    | "statements"
                    | "extract"
                    | "fuzz-regress"
                    | "library"
                    | "resolve"
                    | "schema"
                    | "ir"
                    | "limits"
            ) {
                return Err(format!(
                    "{}: category is not implemented in M1",
                    path.display()
                ));
            }
            let result = if matches!(
                category_name,
                "library" | "resolve" | "schema" | "ir" | "limits"
            ) {
                semantic.run(&path, category_name, &schemas)
            } else {
                run_case(&path, category_name, &schemas).map(|chunkings| semantic::Checks {
                    chunkings,
                    permutations: 0,
                })
            };
            match result {
                Ok(count) => {
                    report.fixtures += 1;
                    report.chunkings += count.chunkings;
                    report.permutations += count.permutations;
                    *report
                        .category_chunkings
                        .entry(category_name.into())
                        .or_default() += count.chunkings;
                    *report.categories.entry(category_name.into()).or_default() += 1;
                }
                Err(error) => failures.push(format!("{}: {error}", path.display())),
            }
        }
    }
    if !failures.is_empty() {
        return Err(failures.join("\n"));
    }
    if report.fixtures == 0 {
        return Err(format!("{}: no fixtures found", root.display()));
    }
    Ok(report)
}

fn run_case(path: &Path, category: &str, schemas: &Schemas) -> Result<usize> {
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let file = entry.file_name();
        let common = matches!(
            file.to_str(),
            Some("case.json" | "input.ui" | "stream.json" | "want.diag.json")
        );
        let output = matches!(
            (category, file.to_str()),
            ("lex", Some("want.tokens.json"))
                | ("extract", Some("want.segments.json"))
                | (
                    "grammar" | "statements" | "extract" | "fuzz-regress",
                    Some("want.ast.json")
                )
        );
        if !common && !output {
            return Err(format!(
                "unsupported fixture file: {}",
                entry.path().display()
            ));
        }
    }
    let case: Case = checked_json(&path.join("case.json"), &schemas.case)?;
    if case.tier != Tier::Core {
        return Err("M1 supports only core syntax fixtures".into());
    }
    let supported = [
        "max_program_bytes",
        "max_stmt_bytes",
        "max_stmt_count",
        "max_bracket_depth",
        "max_expr_depth",
        "max_string_bytes",
    ];
    if case
        .limits
        .keys()
        .any(|key| !supported.contains(&key.as_str()))
    {
        return Err("unsupported M1 limits override".into());
    }
    if category != "lex" {
        return run_parse_case(path, category, schemas, &case);
    }
    if !case.limits.is_empty() {
        return Err("lex fixtures do not take limits overrides".into());
    }
    let input = fs::read(path.join("input.ui")).map_err(|e| e.to_string())?;
    let want_tokens: Vec<Token> = checked_json(&path.join("want.tokens.json"), &schemas.tokens)?;
    let mut want_diagnostics: Vec<DiagnosticKey> =
        checked_json(&path.join("want.diag.json"), &schemas.diagnostics)?;
    want_diagnostics.sort();
    let batch = lex(&input);
    if batch.tokens != want_tokens {
        return Err(format!(
            "token mismatch in {}\nexpected: {want_tokens:?}\nactual: {:?}",
            case.title, batch.tokens
        ));
    }
    let mut actual: Vec<DiagnosticKey> =
        batch.diagnostics.iter().map(DiagnosticKey::from).collect();
    actual.sort();
    if actual != want_diagnostics {
        return Err(format!(
            "diagnostic mismatch in {}\nexpected: {want_diagnostics:?}\nactual: {actual:?}",
            case.title
        ));
    }
    let explicit: Option<Vec<String>> = if path.join("stream.json").exists() {
        Some(checked_json(&path.join("stream.json"), &schemas.stream)?)
    } else {
        None
    };
    let mut count = 0;
    chunks::for_each(&input, explicit.as_deref(), |label, parts| {
        let mut buffer = LexBuffer::default();
        for part in parts {
            buffer.push(part);
        }
        if buffer.finish() != batch {
            return Err(format!(
                "{}: chunking {label} differs from batch",
                case.title
            ));
        }
        count += 1;
        Ok(())
    })?;
    Ok(count)
}

fn run_parse_case(path: &Path, category: &str, schemas: &Schemas, case: &Case) -> Result<usize> {
    let limits: SyntaxLimits =
        serde_json::from_value(serde_json::to_value(&case.limits).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let input = fs::read(path.join("input.ui")).map_err(|e| e.to_string())?;
    let want_ast: Program = checked_json(&path.join("want.ast.json"), &schemas.ast)?;
    let mut want_diagnostics: Vec<DiagnosticKey> =
        checked_json(&path.join("want.diag.json"), &schemas.diagnostics)?;
    want_diagnostics.sort();
    let message = category == "extract";
    let (segments, batch) = if message {
        let (extracted, parsed) = parse_message(&input, limits.clone());
        let wanted: Vec<Segment> =
            checked_json(&path.join("want.segments.json"), &schemas.segments)?;
        if extracted.segments != wanted {
            return Err(format!(
                "segment mismatch\nexpected: {wanted:?}\nactual: {:?}",
                extracted.segments
            ));
        }
        (Some(extracted), parsed)
    } else {
        (None, parse_batch(&input, limits.clone()))
    };
    let mut actual: Vec<_> = batch.diagnostics.iter().map(DiagnosticKey::from).collect();
    actual.sort();
    if actual != want_diagnostics {
        return Err(format!(
            "diagnostic mismatch\nexpected: {want_diagnostics:?}\nactual: {actual:?}"
        ));
    }
    if batch.program != want_ast {
        return Err(format!(
            "AST mismatch\nexpected: {want_ast:?}\nactual: {:?}",
            batch.program
        ));
    }
    let printed = printer::print(&batch.program);
    let roundtrip = parse_batch(
        printed.as_bytes(),
        SyntaxLimits {
            max_program_bytes: 4 * 1024 * 1024,
            max_stmt_bytes: 64 * 1024,
            max_bracket_depth: 64,
            max_expr_depth: 96,
            ..SyntaxLimits::default()
        },
    );
    if roundtrip.program != batch.program {
        return Err(format!(
            "round-trip AST mismatch after printing {printed:?}"
        ));
    }
    let explicit: Option<Vec<String>> = if path.join("stream.json").exists() {
        Some(checked_json(&path.join("stream.json"), &schemas.stream)?)
    } else {
        None
    };
    let mut count = 0;
    chunks::for_each(&input, explicit.as_deref(), |label, parts| {
        if message {
            let mut parser = MessageParser::new(limits.clone());
            for part in parts {
                parser.push(part);
            }
            let (extracted, actual) = parser.finish();
            if Some(extracted) != segments || actual != batch {
                return Err(format!("chunking {label} differs from batch"));
            }
        } else {
            let mut parser = BatchParser::new(limits.clone());
            for part in parts {
                parser.push(part);
            }
            if parser.finish() != batch {
                return Err(format!("chunking {label} differs from batch"));
            }
        }
        count += 1;
        Ok(())
    })?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_conformance() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance");
        let report = run_suite(&root).unwrap();
        assert!(report.fixtures >= 124);
        for category in ["lex", "grammar", "statements", "extract", "fuzz-regress"] {
            assert!(
                report
                    .categories
                    .get(category)
                    .is_some_and(|count| *count > 0)
            );
        }
        assert!(report.chunkings >= 124 * 204);
    }

    #[test]
    fn schemas_reject_unknown_fields_and_bad_types() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance");
        let schemas = Schemas::load(&root).unwrap();
        assert!(
            !schemas
                .tokens
                .is_valid(&serde_json::json!([{"kind":"Name", "text":"a", "typo":1}]))
        );
        assert!(
            !schemas
                .tokens
                .is_valid(&serde_json::json!([{"kind":"Unknown", "text":"a"}]))
        );
        assert!(!schemas.diagnostics.is_valid(&serde_json::json!([{"code":"syntax.typo", "stmt":null, "path":null, "recovery":"Skipped"}])));
        assert!(!schemas.diagnostics.is_valid(&serde_json::json!([{"code":"syntax.bad_char", "stmt":null, "path":null, "recovery":"Skipped", "typo":true}])));
        assert!(!schemas.stream.is_valid(&serde_json::json!(["a", 42])));
        assert!(!schemas.case.is_valid(
            &serde_json::json!({"title":"x", "tier":"core", "tags":[], "notes":"", "typo":true})
        ));
        assert!(!schemas.case.is_valid(&serde_json::json!({"title":"x", "tier":"core", "tags":[], "notes":"", "limits":{"unknown":4}})));
    }

    #[test]
    fn diagnostics_are_compared_as_a_multiset() {
        let result = lex(b"@@");
        let mut keys: Vec<_> = result.diagnostics.iter().map(DiagnosticKey::from).collect();
        keys.sort();
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0], keys[1]);
    }
}
