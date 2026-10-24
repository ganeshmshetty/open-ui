//! M2 static fixtures, byte chunking, and seeded sealed-table permutations (§12.3).
use super::*;
use genui_core::{
    ir::{IdAllocator, NodePath},
    r#lib::{Library, LibraryBuilder},
    validate::{Options, validate, validate_program},
    value::Value as SemanticValue,
};
use serde::Serialize;
use std::collections::BTreeSet;

pub struct Checks {
    pub chunkings: usize,
    pub permutations: usize,
}
pub struct Runner {
    library: Library,
    ir: jsonschema::Validator,
    options: jsonschema::Validator,
    paths: jsonschema::Validator,
    library_input: jsonschema::Validator,
}
impl Runner {
    pub fn new(root: &Path) -> Result<Self> {
        let library =
            genui_std::std_library().map_err(|e| format!("standard library build: {e:?}"))?;
        let golden = fs::read_to_string(root.join("libs/std.json")).map_err(|e| e.to_string())?;
        let actual =
            genui_core::canonical::serialize(library.spec()).map_err(|e| e.to_string())? + "\n";
        if actual != golden {
            return Err("Library Spec byte mismatch against libs/std.json".into());
        }
        fn load(root: &Path, name: &str) -> Result<jsonschema::Validator> {
            let value: Value =
                read_json(&root.join("schemas").join(format!("{name}.schema.json")))?;
            jsonschema::validator_for(&value).map_err(|e| e.to_string())
        }
        Ok(Self {
            library,
            ir: load(root, "ir")?,
            options: load(root, "options")?,
            paths: load(root, "paths")?,
            library_input: load(root, "library")?,
        })
    }
    pub fn run(&self, path: &Path, category: &str, schemas: &Schemas) -> Result<Checks> {
        let case: Case = checked_json(&path.join("case.json"), &schemas.case)?;
        if case.tier != Tier::Core {
            return Err("M2 fixtures must use core tier".into());
        }
        let allowed = if category == "library" {
            vec![
                "case.json",
                "library.json",
                "want.build.json",
                "input.ui",
                "want.ir.json",
                "want.diag.json",
            ]
        } else if path.join("paths.json").exists() {
            vec![
                "case.json",
                "paths.json",
                "want.paths.json",
                "want.diag.json",
            ]
        } else {
            vec![
                "case.json",
                "input.ui",
                "stream.json",
                "want.ir.json",
                "want.diag.json",
                "options.json",
            ]
        };
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry
                .file_name()
                .to_str()
                .is_some_and(|n| allowed.contains(&n))
            {
                return Err(format!(
                    "unsupported fixture file {}",
                    entry.path().display()
                ));
            }
        }
        if category == "library" {
            let b: LibraryBuilder = checked_json(&path.join("library.json"), &self.library_input)?;
            let wanted: Vec<String> = read_json(&path.join("want.build.json"))?;
            let b = if case.tags.iter().any(|t| t == "extend") {
                let mut extension = self.library.extend();
                extension.id = b.id;
                extension.version = b.version;
                for g in b.groups {
                    extension = extension.group(g);
                }
                for (n, t) in b.types {
                    extension = extension.type_(&n, t);
                }
                for c in b.components {
                    extension = extension.component(c);
                }
                for f in b.functions {
                    extension = extension.function(f);
                }
                for a in b.actions {
                    extension = extension.action(a);
                }
                extension.formats.extend(b.formats);
                extension
            } else {
                b
            };
            let result = b.build();
            let actual = result.as_ref().err().cloned().unwrap_or_default();
            if actual != wanted {
                return Err(format!(
                    "builder errors: expected {wanted:?}, actual {actual:?}"
                ));
            }
            let mut count = 0;
            if let Ok(library) = result
                && path.join("input.ui").exists()
            {
                let input = fs::read(path.join("input.ui")).map_err(|e| e.to_string())?;
                let want: Value = checked_json(&path.join("want.ir.json"), &self.ir)?;
                let bytes =
                    fs::read_to_string(path.join("want.ir.json")).map_err(|e| e.to_string())?;
                if bytes != genui_core::canonical::encode(&want) + "\n" {
                    return Err("IR golden is not canonical bytes".into());
                }
                let mut diags: Vec<DiagnosticKey> =
                    checked_json(&path.join("want.diag.json"), &schemas.diagnostics)?;
                diags.sort();
                let actual = validate(&input, &library, &Options::default());
                let mut keys: Vec<_> = actual.diagnostics.iter().map(DiagnosticKey::from).collect();
                keys.sort();
                if actual.ir.canonical_tree(&library) != want || keys != diags {
                    return Err(format!(
                        "library program: IR {:?}; diagnostics {keys:?}",
                        actual.ir.canonical_tree(&library)
                    ));
                }
                let oracle = crate::streaming::Oracle::new(
                    &input,
                    std::sync::Arc::new(library.clone()),
                    Options::default(),
                    false,
                );
                chunks::for_each(&input, None, |label, parts| {
                    oracle.check(parts).map_err(|e| format!("{label}: {e}"))?;
                    let mut parser = BatchParser::default();
                    for p in parts {
                        parser.push(p)
                    }
                    let parsed = parser.finish();
                    if validate_program(
                        &parsed.program,
                        parsed.diagnostics,
                        &library,
                        &Options::default(),
                    ) != actual
                    {
                        return Err(format!("{label}: custom library chunking differs"));
                    }
                    count += 1;
                    Ok(())
                })?;
            }
            return Ok(Checks {
                chunkings: count,
                permutations: 0,
            });
        }
        if path.join("paths.json").exists() {
            self.path_case(path)?;
            return Ok(Checks {
                chunkings: 0,
                permutations: 0,
            });
        }
        let mut options: Options = if path.join("options.json").exists() {
            checked_json(&path.join("options.json"), &self.options)?
        } else {
            Options::default()
        };
        options.limits =
            serde_json::from_value(serde_json::to_value(&case.limits).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let input = fs::read(path.join("input.ui")).map_err(|e| e.to_string())?;
        let want: Value = checked_json(&path.join("want.ir.json"), &self.ir)?;
        let bytes = fs::read_to_string(path.join("want.ir.json")).map_err(|e| e.to_string())?;
        if bytes != genui_core::canonical::encode(&want) + "\n" {
            return Err("IR golden is not canonical bytes".into());
        }
        let mut want_diags: Vec<DiagnosticKey> =
            checked_json(&path.join("want.diag.json"), &schemas.diagnostics)?;
        want_diags.sort();
        let batch = validate(&input, &self.library, &options);
        let actual = batch.ir.canonical_tree(&self.library);
        let mut diags: Vec<_> = batch.diagnostics.iter().map(DiagnosticKey::from).collect();
        diags.sort();
        if diags != want_diags {
            return Err(format!(
                "diagnostic mismatch\nexpected {want_diags:?}\nactual {diags:?}"
            ));
        }
        if actual != want {
            return Err(format!("IR mismatch\nexpected {want}\nactual {actual}"));
        }
        let explicit: Option<Vec<String>> = if path.join("stream.json").exists() {
            Some(checked_json(&path.join("stream.json"), &schemas.stream)?)
        } else {
            None
        };
        let mut count = 0;
        let oracle = crate::streaming::Oracle::new(
            &input,
            std::sync::Arc::new(self.library.clone()),
            options.clone(),
            false,
        );
        chunks::for_each(&input, explicit.as_deref(), |label, parts| {
            oracle.check(parts).map_err(|e| format!("{label}: {e}"))?;
            let mut parser = BatchParser::new(options.limits.syntax.clone());
            for p in parts {
                parser.push(p)
            }
            let parsed = parser.finish();
            let result =
                validate_program(&parsed.program, parsed.diagnostics, &self.library, &options);
            if result != batch {
                return Err(format!("{label}: validation differs from batch"));
            }
            count += 1;
            Ok(())
        })?;
        let mut permutations = 0;
        if matches!(category, "resolve" | "limits") {
            let parsed = parse_batch(&input, options.limits.syntax.clone());
            for seed in 1..=10u64 {
                let mut program = parsed.program.clone();
                let mut indices: Vec<_> = program
                    .statements
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| {
                        s.name != "root" || s.kind == genui_core::syntax::ast::StatementKind::State
                    })
                    .map(|(i, _)| i)
                    .collect();
                let mut rng = seed;
                for i in (1..indices.len()).rev() {
                    rng ^= rng << 13;
                    rng ^= rng >> 7;
                    rng ^= rng << 17;
                    let j = (rng % (i as u64 + 1)) as usize;
                    indices.swap(i, j)
                }
                let values: Vec<_> = indices
                    .iter()
                    .map(|i| program.statements[*i].clone())
                    .collect();
                indices.sort_unstable();
                for (i, s) in indices.into_iter().zip(values) {
                    program.statements[i] = s
                }
                if validate_program(
                    &program,
                    parsed.diagnostics.clone(),
                    &self.library,
                    &options,
                ) != batch
                {
                    return Err(format!("permutation seed {seed} differs from batch"));
                }
                permutations += 1;
            }
        }
        Ok(Checks {
            chunkings: count,
            permutations,
        })
    }
    fn path_case(&self, path: &Path) -> Result<usize> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Probe {
            parent: String,
            slot: String,
            base: String,
            keys: Vec<SemanticValue>,
            #[serde(default = "keyed_default")]
            keyed: bool,
        }
        #[derive(Deserialize, Serialize, PartialEq, Debug)]
        #[serde(deny_unknown_fields)]
        struct Want {
            paths: Vec<String>,
            codes: Vec<Code>,
            distinct_ids: usize,
        }
        fn keyed_default() -> bool {
            true
        }
        let probe: Probe = checked_json(&path.join("paths.json"), &self.paths)?;
        let want: Want = read_json(&path.join("want.paths.json"))?;
        let parent = NodePath(probe.parent).child(&probe.slot, &probe.base);
        let mut seen = BTreeSet::new();
        let mut ids = IdAllocator::default();
        let mut actual = Want {
            paths: Vec::new(),
            codes: Vec::new(),
            distinct_ids: 0,
        };
        let mut allocated = BTreeSet::new();
        let wanted_diagnostics: Vec<DiagnosticKey> = read_json(&path.join("want.diag.json"))?;
        let mut diagnostics = Vec::new();
        for (i, key) in probe.keys.iter().enumerate() {
            let (p, code) = parent.element(i, probe.keyed.then_some(key), &mut seen);
            actual.paths.push(p.0.clone());
            actual.codes.extend(code);
            if let Some(code) = code {
                let mut d = Diagnostic::new(code, Some("root".into()), None);
                d.path = Some(p.0.clone());
                diagnostics.push(DiagnosticKey::from(&d));
            }
            let id = ids.allocate_with(&p, |_| 0);
            if ids.path(id) != Some(&p) {
                return Err("hash collision lost string identity".into());
            }
            allocated.insert(id);
        }
        if diagnostics != wanted_diagnostics {
            return Err(format!("allocator diagnostics differ: {diagnostics:?}"));
        }
        actual.distinct_ids = allocated.len();
        if actual != want {
            return Err(format!(
                "path mismatch expected {want:?}, actual {actual:?}"
            ));
        }
        Ok(1)
    }
}

#[cfg(test)]
mod audit_tests {
    use super::*;
    #[test]
    fn every_spec_code_is_covered_or_explicitly_deferred() {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Deferred {
            milestone: String,
            reason: String,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Audit {
            covered: BTreeMap<Code, Vec<String>>,
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance");
        let audit: Audit = read_json(&root.join("diagnostics.json")).unwrap();
        let deferred: BTreeMap<Code, Deferred> = read_json(&root.join("deferred.json")).unwrap();
        let spec = fs::read_to_string(root.join("../spec/SPEC.md")).unwrap();
        let mut codes = BTreeSet::new();
        let table = spec
            .split("### 4.3 Diagnostic codes")
            .nth(1)
            .unwrap()
            .split("### 4.4 Recovery ladder")
            .next()
            .unwrap();
        for line in table.lines().filter_map(|l| l.strip_prefix("| `")) {
            if let Some((name, _)) = line.split_once('`')
                && name.contains('.')
                && !name.contains(' ')
            {
                let code = serde_json::from_value::<Code>(serde_json::json!(name)).unwrap();
                codes.insert(code);
            }
        }
        let actual: BTreeSet<_> = audit
            .covered
            .keys()
            .chain(deferred.keys())
            .copied()
            .collect();
        assert_eq!(actual, codes);
        assert_eq!(actual.len(), 69);
        for (code, fixtures) in audit.covered {
            assert!(!deferred.contains_key(&code));
            assert!(!fixtures.is_empty());
            for fixture in fixtures {
                let diagnostics: Vec<DiagnosticKey> =
                    read_json(&root.join(fixture).join("want.diag.json")).unwrap();
                assert!(diagnostics.iter().any(|d| d.code == code));
            }
        }
        for deferred in deferred.values() {
            assert!(["M4", "M5", "M6"].contains(&deferred.milestone.as_str()));
            assert!(!deferred.reason.is_empty());
        }
    }
    #[test]
    fn all_declared_recovery_variants_have_fixtures() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance");
        let mut pairs = BTreeSet::new();
        for category in directories(&root).unwrap() {
            if matches!(
                category.file_name().and_then(|s| s.to_str()),
                Some("libs" | "schemas")
            ) {
                continue;
            }
            for fixture in directories(&category).unwrap() {
                let path = fixture.join("want.diag.json");
                if path.exists() {
                    let diagnostics: Vec<DiagnosticKey> = read_json(&path).unwrap();
                    pairs.extend(diagnostics.into_iter().map(|d| (d.code, d.recovery)));
                }
            }
        }
        for pair in [
            (Code::Unresolved, Recovery::ChildOmitted),
            (Code::Unresolved, Recovery::ValueNulled),
            (Code::MissingRequired, Recovery::Defaulted),
            (Code::MissingRequired, Recovery::NodeDropped),
            (Code::SchemaType, Recovery::Defaulted),
            (Code::SchemaType, Recovery::ArgOmitted),
            (Code::SchemaType, Recovery::NodeDropped),
            (Code::Enum, Recovery::Defaulted),
            (Code::Enum, Recovery::ArgOmitted),
            (Code::Enum, Recovery::NodeDropped),
        ] {
            assert!(pairs.contains(&pair), "missing {pair:?}");
        }
    }
}
