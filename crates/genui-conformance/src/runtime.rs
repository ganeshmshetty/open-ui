//! M4 runtime fixtures: evaluated trees, delivered host data and explicit actions.
use super::*;
use genui_core::{
    Session, SessionOptions,
    ir::NodePath,
    r#lib::Library,
    session::{HostError, InputMode, RequestId},
    validate::{Options, validate},
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Step {
    Activate {
        node: String,
        prop: String,
    },
    Deliver {
        request: u64,
        #[serde(default)]
        epoch: Option<u32>,
        #[serde(default)]
        result: Option<genui_core::value::Value>,
        #[serde(default)]
        error: Option<String>,
    },
    Reset,
    Finish,
    Push {
        input: String,
    },
    Advance {
        ms: u64,
    },
    Edit {
        node: String,
        value: genui_core::value::Value,
    },
    Blur {
        node: String,
    },
}

pub fn run_case(
    path: &Path,
    schemas: &Schemas,
    library: Arc<Library>,
    category: &str,
) -> Result<semantic::Checks> {
    let case: Case = checked_json(&path.join("case.json"), &schemas.case)?;
    let allowed = [
        "case.json",
        "input.ui",
        "stream.json",
        "want.resolved.json",
        "want.diag.json",
        "want.requests.json",
        "want.events.json",
        "want.fields.json",
        "options.json",
        "script.json",
        "library.json",
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
    let library = if path.join("library.json").exists() {
        let extension: genui_core::r#lib::LibraryBuilder =
            checked_json(&path.join("library.json"), &schemas.library_input)?;
        let mut builder = library.extend();
        builder.id = extension.id;
        builder.version = extension.version;
        for group in extension.groups {
            builder = builder.group(group);
        }
        for (name, ty) in extension.types {
            builder = builder.type_(&name, ty);
        }
        for component in extension.components {
            builder = builder.component(component);
        }
        for function in extension.functions {
            builder = builder.function(function);
        }
        for action in extension.actions {
            builder = builder.action(action);
        }
        builder.formats.extend(extension.formats);
        std::sync::Arc::new(
            builder
                .build()
                .map_err(|errors| format!("library: {errors:?}"))?,
        )
    } else {
        library
    };
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
    let mut replay = genui_core::patch::Replay::default();
    let mut requests = Vec::new();
    let mut events = Vec::new();
    let update = session.push(&input);
    apply_replay(&mut replay, &update, "push")?;
    requests.extend(
        update
            .requests
            .into_iter()
            .filter_map(|request| genui_core::canonical::serialize(&request).ok()),
    );
    let update = session.finish();
    apply_replay(&mut replay, &update, "finish")?;
    requests.extend(
        update
            .requests
            .into_iter()
            .filter_map(|request| genui_core::canonical::serialize(&request).ok()),
    );
    if path.join("script.json").exists() {
        let steps: Vec<Step> = read_json(&path.join("script.json"))?;
        for step in steps {
            let update = match step {
                Step::Activate { node, prop } => {
                    let path = NodePath(node);
                    let handle = session
                        .action_handle(&path, &prop)
                        .ok_or_else(|| format!("no action handle for {path:?}.{prop}"))?;
                    let id = session
                        .live_ir()
                        .nodes
                        .values()
                        .find(|node| node.path == path)
                        .map(|node| node.id)
                        .ok_or_else(|| format!("no node {path:?}"))?;
                    session.activate(id, &handle)
                }
                Step::Deliver {
                    request,
                    epoch,
                    result,
                    error,
                } => {
                    let result = match error.as_deref() {
                        Some("denied") => Err(HostError::Denied),
                        Some("failed") => Err(HostError::Failed),
                        Some("too_large") => Err(HostError::TooLarge),
                        Some(other) => return Err(format!("unknown delivery error {other:?}")),
                        None => Ok(result.unwrap_or(genui_core::value::Value::Null)),
                    };
                    session.deliver_from(
                        epoch.unwrap_or_else(|| session.epoch()),
                        RequestId(request),
                        result,
                    )
                }
                Step::Reset => session.reset(),
                Step::Finish => session.finish(),
                Step::Push { input } => session.push(input.as_bytes()),
                Step::Advance { ms } => session.advance_clock(ms),
                Step::Edit { node, value } => {
                    let node = node_by_path(&session, &node)?;
                    session.event(genui_core::session::UiEvent::Edit { node, value })
                }
                Step::Blur { node } => {
                    let node = node_by_path(&session, &node)?;
                    session.event(genui_core::session::UiEvent::Blur { node })
                }
            };
            apply_replay(&mut replay, &update, "script")?;
            requests.extend(
                update
                    .requests
                    .into_iter()
                    .filter_map(|request| genui_core::canonical::serialize(&request).ok()),
            );
            events.extend(
                update
                    .model_events
                    .into_iter()
                    .filter_map(|event| genui_core::canonical::serialize(&event).ok()),
            );
        }
    }
    if replay.tree != *session.tree() {
        return Err(format!(
            "scripted patch replay differs from the live tree: {:?} vs {:?}",
            replay.tree,
            session.tree()
        ));
    }
    let actual = if category == "provenance" {
        genui_core::runtime::resolved_tree_json(
            session.resolved().ir_or(session.ir()),
            &library,
            session.resolved(),
            true,
        )
    } else {
        session.tree().canonical_tree()
    };
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
    if path.join("want.requests.json").exists() {
        let want: Vec<Value> = checked_json(&path.join("want.requests.json"), &schemas.requests)?;
        let actual: Vec<Value> = requests
            .iter()
            .filter_map(|request| serde_json::from_str(request).ok())
            .collect();
        if actual != want {
            return Err(format!(
                "request mismatch\nexpected {want:?}\nactual {actual:?}"
            ));
        }
    } else if !requests.is_empty() {
        return Err(format!(
            "unexpected host requests without want.requests.json: {requests:?}"
        ));
    }
    if path.join("want.events.json").exists() {
        let want: Vec<Value> = checked_json(&path.join("want.events.json"), &schemas.events)?;
        let actual: Vec<Value> = events
            .iter()
            .filter_map(|event| serde_json::from_str(event).ok())
            .collect();
        if actual != want {
            return Err(format!(
                "model-event mismatch\nexpected {want:?}\nactual {actual:?}"
            ));
        }
    } else if !events.is_empty() {
        return Err(format!(
            "unexpected model events without want.events.json: {events:?}"
        ));
    }
    if path.join("want.fields.json").exists() {
        let want: Vec<Value> = checked_json(&path.join("want.fields.json"), &schemas.fields)?;
        let actual: Vec<Value> = session
            .field_status()
            .iter()
            .map(|(path, status)| {
                serde_json::json!({
                    "path": path,
                    "error": status.error,
                    "touched": status.touched,
                })
            })
            .collect();
        if actual != want {
            return Err(format!(
                "field status mismatch\nexpected {want:?}\nactual {actual:?}"
            ));
        }
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

fn apply_replay(
    replay: &mut genui_core::patch::Replay,
    update: &genui_core::session::Update,
    label: &str,
) -> Result<()> {
    if let Some(batch) = &update.patches {
        let report = replay.apply(batch);
        if !report.errors.is_empty() || report.needs_snapshot || report.discarded {
            return Err(format!(
                "{label}: scripted replay rejected a batch: {report:?}"
            ));
        }
    }
    Ok(())
}

fn node_by_path(session: &Session, path: &str) -> Result<genui_core::ir::NodeId> {
    session
        .live_ir()
        .nodes
        .values()
        .find(|node| node.path.0 == path)
        .map(|node| node.id)
        .ok_or_else(|| format!("no node {path:?}"))
}
