//! M4 runtime fixtures: evaluated trees, runtime diagnostics and explicit actions.
use super::*;
use genui_core::{
    Session, SessionOptions,
    ir::NodePath,
    r#lib::Library,
    session::InputMode,
    validate::{Options, validate},
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Step {
    Activate { node: String, prop: String },
}

pub fn run_case(path: &Path, schemas: &Schemas, library: Arc<Library>) -> Result<semantic::Checks> {
    let case: Case = checked_json(&path.join("case.json"), &schemas.case)?;
    let allowed = [
        "case.json",
        "input.ui",
        "stream.json",
        "want.resolved.json",
        "want.diag.json",
        "options.json",
        "script.json",
    ];
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry
            .file_name()
            .to_str()
            .is_some_and(|name| allowed.contains(&name))
        {
            return Err(format!(
                "unsupported runtime fixture file {}",
                entry.path().display()
            ));
        }
    }
    let mut options: Options = if path.join("options.json").exists() {
        checked_json(&path.join("options.json"), &schemas.options)?
    } else {
        Options::default()
    };
    options.limits =
        serde_json::from_value(serde_json::to_value(&case.limits).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let input = fs::read(path.join("input.ui")).map_err(|e| e.to_string())?;
    let want: Value = checked_json(&path.join("want.resolved.json"), &schemas.resolved)?;
    if fs::read_to_string(path.join("want.resolved.json")).map_err(|e| e.to_string())?
        != genui_core::canonical::encode(&want) + "\n"
    {
        return Err("resolved golden is not canonical bytes".into());
    }
    let mut want_diags: Vec<DiagnosticKey> =
        checked_json(&path.join("want.diag.json"), &schemas.diagnostics)?;
    want_diags.sort();

    let batch = validate(&input, &library, &options);
    let mut session = Session::new(SessionOptions {
        library: library.clone(),
        validation: options.clone(),
        input: InputMode::Program,
    });
    session.push(&input);
    session.finish();
    if path.join("script.json").exists() {
        let steps: Vec<Step> = read_json(&path.join("script.json"))?;
        for step in steps {
            match step {
                Step::Activate { node, prop } => {
                    let path = NodePath(node);
                    let handle = session
                        .action_handle(&path, &prop)
                        .ok_or_else(|| format!("no action handle for {path:?}.{prop}"))?;
                    let id = session
                        .ir()
                        .nodes
                        .values()
                        .find(|node| node.path == path)
                        .map(|node| node.id)
                        .ok_or_else(|| format!("no node {path:?}"))?;
                    session.activate(id, &handle);
                }
            }
        }
    }
    let actual = session.tree().canonical_tree();
    if actual != want {
        return Err(format!(
            "resolved tree mismatch\nexpected {want}\nactual {actual}"
        ));
    }
    let mut actual_diags: Vec<_> = session
        .diagnostics()
        .iter()
        .chain(session.runtime_diagnostics().iter())
        .map(DiagnosticKey::from)
        .collect();
    actual_diags.sort();
    if actual_diags != want_diags {
        return Err(format!(
            "runtime diagnostic mismatch\nexpected {want_diags:?}\nactual {actual_diags:?}"
        ));
    }
    if batch.ir != *session.ir() {
        return Err("runtime session IR differs from batch validation".into());
    }
    let oracle = streaming::Oracle::new(&input, library, options, false);
    let explicit: Option<Vec<String>> = if path.join("stream.json").exists() {
        Some(checked_json(&path.join("stream.json"), &schemas.stream)?)
    } else {
        None
    };
    let mut count = 0;
    chunks::for_each(&input, explicit.as_deref(), |label, parts| {
        oracle.check(parts).map_err(|e| format!("{label}: {e}"))?;
        count += 1;
        Ok(())
    })?;
    Ok(semantic::Checks {
        chunkings: count,
        permutations: 0,
    })
}
