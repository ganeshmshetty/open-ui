//! M5 conformance: prompt goldens, feedback text and stored messages/events (SPEC §12.4).
use super::*;
use genui_core::{
    Session, SessionOptions, feedback,
    message::{ModelEvent, StoredMessage},
    protocol::RefEntry,
    session::InputMode,
    validate::{Options, validate},
};
use genui_prompt::{
    PromptOptions, example_programs, generate, render_event, render_stored_message,
};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct MessageOptions {
    prompt_digest: String,
    finish: bool,
    program: bool,
    chunks: Vec<String>,
}

impl Default for MessageOptions {
    fn default() -> Self {
        Self {
            prompt_digest: String::new(),
            finish: true,
            program: false,
            chunks: Vec::new(),
        }
    }
}

pub fn run_case(path: &Path, category: &str, schemas: &Schemas) -> Result<semantic::Checks> {
    let case: Case = checked_json(&path.join("case.json"), &schemas.case)?;
    if case.tier != Tier::Core {
        return Err("M5 fixtures must use core tier".into());
    }
    match category {
        "prompt" => prompt_case(path, schemas),
        "feedback" => feedback_case(path, schemas),
        "message" => message_case(path, schemas),
        other => Err(format!("unknown M5 category {other}")),
    }
}

fn allowed(path: &Path, files: &[&str]) -> Result<()> {
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry
            .file_name()
            .to_str()
            .is_some_and(|name| files.contains(&name))
        {
            return Err(format!(
                "unsupported fixture file {}",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TurnOptions {
    #[serde(default)]
    refs: Vec<RefEntry>,
    feedback: Option<String>,
}

fn prompt_case(path: &Path, schemas: &Schemas) -> Result<semantic::Checks> {
    allowed(
        path,
        &[
            "case.json",
            "prompt.json",
            "want.prompt.txt",
            "want.prompt.digest",
            "turn.json",
            "want.turn.txt",
        ],
    )?;
    let options: PromptOptions = checked_json(&path.join("prompt.json"), &schemas.prompt)?;
    let golden = fs::read_to_string(path.join("want.prompt.txt")).map_err(|e| e.to_string())?;
    let library = genui_std::std_library().map_err(|e| format!("{e:?}"))?;
    let first = generate(library.spec(), &options);
    let second = generate(library.spec(), &options);
    if first != second {
        return Err("prompt generation is not deterministic".into());
    }
    if first.text != golden {
        return Err(format!(
            "prompt mismatch\n--- expected ---\n{golden}\n--- actual ---\n{}",
            first.text
        ));
    }
    if first.text.contains('\r') {
        return Err("prompt must use LF line endings".into());
    }
    if !first.text.ends_with('\n') {
        return Err("prompt must end with one LF".into());
    }
    if path.join("want.prompt.digest").exists() {
        let digest = fs::read_to_string(path.join("want.prompt.digest"))
            .map_err(|e| e.to_string())?
            .trim()
            .to_string();
        if digest != first.digest {
            return Err(format!(
                "prompt digest mismatch: expected {digest}, actual {}",
                first.digest
            ));
        }
    }
    if path.join("turn.json").exists() {
        let turn: TurnOptions = checked_json(&path.join("turn.json"), &schemas.turn)?;
        let catalog = genui_core::protocol::RefCatalog { entries: turn.refs };
        let rendered = genui_prompt::render_turn_block(&catalog, turn.feedback.as_deref());
        let wanted = fs::read_to_string(path.join("want.turn.txt")).map_err(|e| e.to_string())?;
        if rendered != wanted {
            return Err(format!(
                "turn block mismatch\n--- expected ---\n{wanted}\n--- actual ---\n{rendered}"
            ));
        }
    }
    let host = Options {
        url_hosts: ["example.com".into()].into(),
        ..Default::default()
    };
    for example in example_programs(library.spec(), &options) {
        let checked = validate(example.as_bytes(), &library, &host);
        if checked
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity != genui_core::diag::Severity::Info)
        {
            return Err(format!("prompt example does not validate: {example}"));
        }
    }
    Ok(semantic::Checks {
        chunkings: 0,
        permutations: 0,
    })
}

fn feedback_case(path: &Path, schemas: &Schemas) -> Result<semantic::Checks> {
    allowed(
        path,
        &["case.json", "input.ui", "options.json", "want.feedback.txt"],
    )?;
    let options: Options = if path.join("options.json").exists() {
        checked_json(&path.join("options.json"), &schemas.options)?
    } else {
        Options::default()
    };
    let input = fs::read(path.join("input.ui")).map_err(|e| e.to_string())?;
    let library = genui_std::std_library().map_err(|e| format!("{e:?}"))?;
    let result = validate(&input, &library, &options);
    let first = feedback::render(&result.diagnostics, &library);
    let second = feedback::render(&result.diagnostics, &library);
    if first != second {
        return Err("feedback rendering is not deterministic".into());
    }
    let wanted = path.join("want.feedback.txt");
    if wanted.exists() {
        let golden = fs::read_to_string(&wanted).map_err(|e| e.to_string())?;
        match first {
            Some(actual) if actual == golden => {}
            Some(actual) => {
                return Err(format!(
                    "feedback mismatch\n--- expected ---\n{golden}\n--- actual ---\n{actual}"
                ));
            }
            None => return Err("expected feedback, got none".into()),
        }
    } else if first.is_some() {
        return Err(format!(
            "unexpected feedback: {}",
            first.unwrap_or_default()
        ));
    }
    Ok(semantic::Checks {
        chunkings: 0,
        permutations: 0,
    })
}

fn message_case(path: &Path, schemas: &Schemas) -> Result<semantic::Checks> {
    if path.join("event.json").exists() {
        return event_case(path, schemas);
    }
    if path.join("stored.json").exists() {
        return stored_case(path, schemas);
    }
    session_case(path, schemas)
}

fn event_case(path: &Path, schemas: &Schemas) -> Result<semantic::Checks> {
    allowed(path, &["case.json", "event.json", "want.event.txt"])?;
    let event: ModelEvent = checked_json(&path.join("event.json"), &schemas.event)?;
    let golden = fs::read_to_string(path.join("want.event.txt")).map_err(|e| e.to_string())?;
    let first = render_event(&event);
    let second = render_event(&event);
    if first != second {
        return Err("event rendering is not deterministic".into());
    }
    if first != golden {
        return Err(format!(
            "event mismatch\n--- expected ---\n{golden}\n--- actual ---\n{first}"
        ));
    }
    if event.v != 1 || event.source != "ui_event" {
        return Err("event must use v 1 and source ui_event".into());
    }
    Ok(semantic::Checks {
        chunkings: 0,
        permutations: 0,
    })
}

fn stored_case(path: &Path, schemas: &Schemas) -> Result<semantic::Checks> {
    allowed(
        path,
        &[
            "case.json",
            "stored.json",
            "want.turn.txt",
            "want.diag.json",
            "want.ir.json",
            "want.restore.diag.json",
        ],
    )?;
    let stored: StoredMessage = checked_json(&path.join("stored.json"), &schemas.stored)?;
    let library = genui_std::std_library().map_err(|e| format!("{e:?}"))?;
    let turn = render_stored_message(&stored);
    let golden = fs::read_to_string(path.join("want.turn.txt")).map_err(|e| e.to_string())?;
    if turn != golden {
        return Err(format!(
            "stored replay mismatch\n--- expected ---\n{golden}\n--- actual ---\n{turn}"
        ));
    }
    let mismatch = stored.library_mismatch(library.spec());
    let wanted = path.join("want.diag.json");
    if wanted.exists() {
        let mut expected: Vec<DiagnosticKey> = checked_json(&wanted, &schemas.diagnostics)?;
        expected.sort();
        let mut actual: Vec<DiagnosticKey> = mismatch.iter().map(DiagnosticKey::from).collect();
        actual.sort();
        if actual != expected {
            return Err(format!(
                "library identity mismatch: expected {expected:?}, actual {actual:?}"
            ));
        }
    } else if mismatch.is_some() {
        return Err("unexpected lib.mismatch".into());
    }
    if path.join("want.ir.json").exists() {
        let want: Value = checked_json(&path.join("want.ir.json"), &schemas.ir)?;
        let options = SessionOptions::new(Arc::new(library.clone()));
        let restored = Session::restore(&stored, options);
        if restored.ir().canonical_tree(&library) != want {
            return Err("restored IR differs from golden".into());
        }
    }
    Ok(semantic::Checks {
        chunkings: 0,
        permutations: 0,
    })
}

fn session_case(path: &Path, schemas: &Schemas) -> Result<semantic::Checks> {
    allowed(
        path,
        &[
            "case.json",
            "input.ui",
            "message.json",
            "want.stored.json",
            "want.turn.txt",
        ],
    )?;
    let options: MessageOptions = if path.join("message.json").exists() {
        checked_json(&path.join("message.json"), &schemas.message)?
    } else {
        MessageOptions::default()
    };
    let input = fs::read(path.join("input.ui")).map_err(|e| e.to_string())?;
    let library = genui_std::std_library().map_err(|e| format!("{e:?}"))?;
    let mut session = Session::new(SessionOptions {
        library: Arc::new(library.clone()),
        validation: Options::default(),
        input: if options.program {
            InputMode::Program
        } else {
            InputMode::Message
        },
        prompt_digest: options.prompt_digest.clone(),
    });
    if options.chunks.is_empty() {
        let update = session.push(&input);
        if !update.requests.is_empty() || !update.model_events.is_empty() {
            return Err("stored-message construction must not create host effects".into());
        }
    } else {
        for chunk in &options.chunks {
            let update = session.push(chunk.as_bytes());
            if !update.requests.is_empty() || !update.model_events.is_empty() {
                return Err("stored-message construction must not create host effects".into());
            }
        }
    }
    if options.finish {
        session.finish();
    }
    let stored = session.stored_message();
    let bytes = genui_core::canonical::serialize(&stored).map_err(|e| e.to_string())? + "\n";
    let golden = fs::read_to_string(path.join("want.stored.json")).map_err(|e| e.to_string())?;
    if bytes != golden {
        return Err(format!(
            "stored message mismatch\n--- expected ---\n{golden}\n--- actual ---\n{bytes}"
        ));
    }
    if stored.lib != genui_core::message::LibRef::of(library.spec()) {
        return Err("stored library identity differs from the active library".into());
    }
    if path.join("want.turn.txt").exists() {
        let want = fs::read_to_string(path.join("want.turn.txt")).map_err(|e| e.to_string())?;
        if render_stored_message(&stored) != want {
            return Err("stored replay mismatch".into());
        }
    }
    Ok(semantic::Checks {
        chunkings: 0,
        permutations: 0,
    })
}

#[cfg(test)]
mod restore_tests {
    use super::*;
    use genui_core::{
        message::{LibRef, StateSnapshot, StoredMessage, StoredSegment},
        value::Value,
    };

    fn stored() -> StoredMessage {
        StoredMessage {
            v: 1,
            segments: vec![StoredSegment::Program {
                text: "root = Column([Heading(\"Overview\"), Text(\"Ready\")])\n".into(),
                major: 1,
            }],
            lib: LibRef {
                id: "std".into(),
                version: "0.1.0".into(),
                hash: "d19a769bd510997ce786ade4d60db23d".into(),
            },
            prompt_digest: "0123456789abcdef0123456789abcdef".into(),
            complete: true,
            fallback_text: "## Overview\nReady\n".into(),
            diagnostics: Vec::new(),
            state: Some(StateSnapshot {
                v: 1,
                values: [("count".into(), Value::Int(7))].into(),
            }),
        }
    }

    #[test]
    fn restore_replays_the_program_and_keeps_state() {
        let library = genui_std::std_library().unwrap();
        let expected = genui_core::validate::validate(
            b"root = Column([Heading(\"Overview\"), Text(\"Ready\")])\n",
            &library,
            &Options::default(),
        );
        let restored = Session::restore(&stored(), SessionOptions::new(Arc::new(library.clone())));
        assert_eq!(restored.ir(), &expected.ir);
        assert!(restored.diagnostics().iter().all(|d| !d.provisional));
        assert_eq!(
            restored
                .restored_state()
                .and_then(|state| state.values.get("count")),
            Some(&Value::Int(7))
        );
        assert_eq!(
            restored.stored_message().prompt_digest,
            stored().prompt_digest
        );
    }

    #[test]
    fn mismatch_is_info_and_does_not_block_replay() {
        let library = genui_std::std_library().unwrap();
        let mut message = stored();
        message.lib.version = "0.0.9".into();
        let restored = Session::restore(&message, SessionOptions::new(Arc::new(library.clone())));
        let mismatch: Vec<_> = restored
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code == genui_core::diag::Code::LibMismatch)
            .collect();
        assert_eq!(mismatch.len(), 1);
        assert_eq!(mismatch[0].severity, genui_core::diag::Severity::Info);
        assert!(restored.ir().root.is_some());
    }
}
