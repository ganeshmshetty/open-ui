//! `prompt` and `feedback` developer commands (SPEC §§4.6,9.1).
use genui_core::{
    Session, SessionOptions, feedback, session::InputMode, validate::Options, validate::validate,
};
use genui_prompt::{PromptOptions, generate, render_stored_message};
use std::path::Path;
use std::sync::Arc;

pub fn prompt(path: Option<&str>) -> Result<genui_prompt::Prompt, String> {
    let library = genui_std::std_library().map_err(|e| format!("std build: {e:?}"))?;
    let options = match path {
        Some(path) => {
            let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            serde_json::from_slice::<PromptOptions>(&bytes).map_err(|e| format!("{path}: {e}"))?
        }
        None => PromptOptions::default(),
    };
    Ok(generate(library.spec(), &options))
}

pub fn diagnostics(path: &Path) -> Result<(String, bool), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let library = genui_std::std_library().map_err(|e| format!("std build: {e:?}"))?;
    let result = validate(&bytes, &library, &Options::default());
    let block = feedback::render(&result.diagnostics, &library)
        .unwrap_or_else(|| "(no model feedback)\n".into());
    Ok((block, true))
}

pub fn message(path: &Path, digest: &str, finish: bool, replay: bool) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let library = genui_std::std_library().map_err(|e| format!("std build: {e:?}"))?;
    let mut session = Session::new(SessionOptions {
        library: Arc::new(library),
        validation: Options::default(),
        input: InputMode::Message,
        prompt_digest: digest.into(),
    });
    session.push(&bytes);
    if finish {
        session.finish();
    }
    let stored = session.stored_message();
    if replay {
        return Ok(render_stored_message(&stored));
    }
    let mut text = genui_core::canonical::serialize(&stored).map_err(|e| e.to_string())?;
    text.push('\n');
    Ok(text)
}
