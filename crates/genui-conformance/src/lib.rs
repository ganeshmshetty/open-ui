//! Conformance scaffolding for SPEC §12. M0 checks lexical output at finish only.
#![forbid(unsafe_code)]

pub mod chunks;
pub mod spec_check;

use genui_core::diag::{Code, Diagnostic, Recovery};
use genui_core::syntax::{LexBuffer, Token, lex};
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
}

/// Fail on unsupported fixture categories rather than silently reporting success.
pub fn run_suite(root: &Path) -> Result<SuiteReport> {
    let schemas = Schemas::load(root)?;
    let mut report = SuiteReport::default();
    for category in directories(root)? {
        if matches!(
            category.file_name().and_then(|s| s.to_str()),
            Some("schemas" | "libs")
        ) {
            continue;
        }
        for path in directories(&category)? {
            if category.file_name().and_then(|s| s.to_str()) != Some("lex") {
                return Err(format!(
                    "{}: category is not implemented in M0",
                    path.display()
                ));
            }
            let count =
                run_lex_case(&path, &schemas).map_err(|e| format!("{}: {e}", path.display()))?;
            report.fixtures += 1;
            report.chunkings += count;
        }
    }
    if report.fixtures == 0 {
        return Err(format!("{}: no fixtures found", root.display()));
    }
    Ok(report)
}

fn run_lex_case(path: &Path, schemas: &Schemas) -> Result<usize> {
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !matches!(
            entry.file_name().to_str(),
            Some("case.json" | "input.ui" | "stream.json" | "want.tokens.json" | "want.diag.json")
        ) {
            return Err(format!(
                "unsupported fixture file: {}",
                entry.path().display()
            ));
        }
    }
    let case: Case = checked_json(&path.join("case.json"), &schemas.case)?;
    if case.tier != Tier::Core || !case.limits.is_empty() {
        return Err("M0 supports core lexical fixtures without limits overrides".into());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_conformance() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance");
        let report = run_suite(&root).unwrap();
        assert!(report.fixtures >= 20);
        assert!(report.chunkings >= report.fixtures * 204);
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
