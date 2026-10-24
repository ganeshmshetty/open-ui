//! Byte-chunk inspection of the actual Session patch stream (§12.7).
use genui_core::{Session, SessionOptions, Update, canonical, diag::Severity};
use std::{fmt::Write, sync::Arc};
pub fn run(args: &[String]) -> Result<bool, String> {
    let path = args
        .first()
        .ok_or("usage: genui-cli replay <file.ui> [--chunk N]")?;
    let chunk = match &args[1..] {
        [] => 64,
        [flag, n] if flag == "--chunk" => n
            .parse::<usize>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or("chunk size must be a positive byte count")?,
        _ => return Err("usage: genui-cli replay <file.ui> [--chunk N]".into()),
    };
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let library = Arc::new(genui_std::std_library().map_err(|e| format!("std build: {e:?}"))?);
    let mut session = Session::new(SessionOptions::program(library));
    let mut output = String::new();
    let mut offset = 0;
    for (i, part) in bytes.chunks(chunk).enumerate() {
        let update = session.push(part);
        let _ = writeln!(
            output,
            "chunk {i} bytes={offset}..{} phase={:?}",
            offset + part.len(),
            update.phase
        );
        write_update(&mut output, &session, &update)?;
        offset += part.len();
    }
    let update = session.finish();
    let _ = writeln!(output, "finish phase={:?}", update.phase);
    write_update(&mut output, &session, &update)?;
    let _ = writeln!(output, "Diagnostics: {}", session.diagnostics().len());
    for d in session.diagnostics() {
        let _ = writeln!(
            output,
            "  {}",
            canonical::serialize(d).map_err(|e| e.to_string())?
        );
    }
    print!("{output}");
    Ok(!session
        .diagnostics()
        .iter()
        .any(|d| d.severity != Severity::Info))
}
fn write_update(output: &mut String, session: &Session, update: &Update) -> Result<(), String> {
    if let Some(batch) = &update.patches {
        let _ = writeln!(
            output,
            "batch {}",
            batch.canonical_json().map_err(|e| e.to_string())?
        );
    } else {
        output.push_str("batch (none)\n");
    }
    let _ = writeln!(
        output,
        "tree {}",
        canonical::encode(&session.tree().canonical_tree())
    );
    Ok(())
}
