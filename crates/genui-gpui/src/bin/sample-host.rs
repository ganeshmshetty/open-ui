//! Sample chat host (SPEC §12.7; ROADMAP items 8 and 17).
//!
//! Streams a recorded scripted transcript through a real `Session` into
//! `UiView`, answers host requests from the transcript policy, and can capture
//! mid-stream screenshots, freeze/resume, regenerate and restore. Every
//! transcript is labelled `scripted-fixture`; this binary never claims a live
//! model run.

use genui_core::validate::Options;
#[cfg(feature = "screenshot")]
use genui_core::Session;
#[cfg(feature = "screenshot")]
use genui_core::{SessionOptions, diag::Diagnostic, session::InputMode};
#[cfg(feature = "screenshot")]
use genui_gpui::{
    demo::DemoStage,
    theme::UiTheme,
    view::UiView,
};
use genui_gpui::theme::ThemeMode;
use genui_prompt::{
    PromptOptions, generate,
    transcript::{Transcript, replay, SCRIPTED_LABEL},
};
#[cfg(feature = "screenshot")]
use genui_prompt::transcript::LiveRunner;
#[cfg(feature = "screenshot")]
use gpui_kit::AppContext;
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
};
#[cfg(feature = "screenshot")]
use std::time::Instant;

#[derive(Clone, Debug)]
struct Args {
    transcript: Option<PathBuf>,
    directory: PathBuf,
    list: bool,
    replay_all: bool,
    headless: bool,
    screenshot: Option<PathBuf>,
    record: Option<PathBuf>,
    freeze_at: Option<usize>,
    restore: bool,
    regenerate: bool,
    delay_ms: u64,
    theme: ThemeMode,
}

impl Default for Args {
    fn default() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        Self {
            transcript: None,
            directory: root.join("../../eval/transcripts"),
            list: false,
            replay_all: false,
            headless: false,
            screenshot: None,
            record: None,
            freeze_at: None,
            restore: false,
            regenerate: false,
            delay_ms: 16,
            theme: ThemeMode::Light,
        }
    }
}

fn parse_args(args: Vec<String>) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--transcript" | "--send" => {
                index += 1;
                parsed.transcript = Some(PathBuf::from(
                    args.get(index).ok_or("--transcript needs a path")?,
                ));
            }
            "--dir" => {
                index += 1;
                parsed.directory = PathBuf::from(args.get(index).ok_or("--dir needs a path")?);
            }
            "--list" => parsed.list = true,
            "--replay-all" => parsed.replay_all = true,
            "--headless" => parsed.headless = true,
            "--screenshot" => {
                index += 1;
                parsed.screenshot = Some(PathBuf::from(
                    args.get(index).ok_or("--screenshot needs a directory")?,
                ));
            }
            "--record" => {
                index += 1;
                parsed.record = Some(PathBuf::from(
                    args.get(index).ok_or("--record needs a path")?,
                ));
            }
            "--freeze-at" => {
                index += 1;
                parsed.freeze_at = Some(
                    args.get(index)
                        .ok_or("--freeze-at needs a chunk index")?
                        .parse()
                        .map_err(|_| "--freeze-at expects an integer")?,
                );
            }
            "--restore" => parsed.restore = true,
            "--regenerate" => parsed.regenerate = true,
            "--delay-ms" => {
                index += 1;
                parsed.delay_ms = args
                    .get(index)
                    .ok_or("--delay-ms needs a value")?
                    .parse()
                    .map_err(|_| "--delay-ms expects an integer")?;
            }
            "--light" => parsed.theme = ThemeMode::Light,
            "--dark" => parsed.theme = ThemeMode::Dark,
            "--help" | "-h" => {
                return Err(
                    "usage: sample-host [--transcript FILE] [--dir DIR] [--list]\n\
                     \x20                  [--replay-all] [--headless] [--screenshot DIR]\n\
                     \x20                  [--record FILE] [--freeze-at N] [--restore]\n\
                     \x20                  [--regenerate] [--delay-ms N] [--light|--dark]"
                        .into(),
                );
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
        index += 1;
    }
    Ok(parsed)
}

#[derive(Deserialize)]
struct HostFile {
    tools: Vec<String>,
    refs: Vec<String>,
    url_hosts: Vec<String>,
}

fn host_options() -> Result<Options, String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance/libs/host.json");
    let bytes =
        std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let host: HostFile = serde_json::from_str(&bytes).map_err(|error| error.to_string())?;
    Ok(Options {
        tools: host.tools.into_iter().collect(),
        refs: host.refs.into_iter().collect(),
        url_hosts: host.url_hosts.into_iter().collect(),
        capabilities: ["charts".into(), "images".into(), "markdown".into()].into(),
        ..Options::default()
    })
}

fn library() -> Result<Arc<genui_core::r#lib::Library>, String> {
    Ok(Arc::new(
        genui_std::std_library().map_err(|errors| format!("std build: {errors:?}"))?,
    ))
}

fn transcripts(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|error| format!("{}: {error}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "json"))
        .collect();
    paths.sort();
    Ok(paths)
}

fn load(path: &Path) -> Result<Transcript, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let transcript: Transcript =
        serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))?;
    if transcript.label != SCRIPTED_LABEL {
        return Err(format!(
            "{}: label {:?} is not {SCRIPTED_LABEL:?}",
            path.display(),
            transcript.label
        ));
    }
    Ok(transcript)
}

#[cfg(feature = "screenshot")]
fn report(diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        if diagnostic.severity >= genui_core::diag::Severity::Warning {
            eprintln!(
                "{} {:?} {}",
                diagnostic.path.as_deref().unwrap_or("-"),
                diagnostic.code,
                diagnostic.message
            );
        }
    }
}

#[cfg(feature = "screenshot")]
struct RunReport {
    transcript: String,
    prompt_digest: String,
    chunks: usize,
    batches: usize,
    nodes: usize,
    model_events: usize,
    diagnostics: Vec<String>,
    phase: String,
    screenshots: Vec<String>,
    frozen_at: Option<usize>,
    restored: Option<RestoreReport>,
    regenerated: Option<RegenerateReport>,
    wall_ms: f64,
}

#[derive(Clone, Debug)]
#[cfg(feature = "screenshot")]
struct RestoreReport {
    ok: bool,
    state_equal: bool,
    tree_equal: bool,
    nodes: usize,
}

#[derive(Clone, Debug)]
#[cfg(feature = "screenshot")]
struct RegenerateReport {
    ok: bool,
    identical: bool,
}

#[cfg(feature = "screenshot")]
fn apply_updates(
    updates: Vec<genui_core::Update>,
    window: &mut gpui_kit::WindowHandle<DemoStage>,
    cx: &mut gpui_kit::HeadlessAppContext,
    batches: &mut Vec<String>,
    model_events: &mut usize,
) -> Result<(), String> {
    for update in updates {
        if let Some(batch) = update.patches {
            batches.push(batch.canonical_json().map_err(|error| error.to_string())?);
            let batch_for_view = batch.clone();
            window
                .update(cx, |stage, window, cx| {
                    if let Some(ui) = &stage.child {
                        ui.update(cx, |view, cx| {
                            view.apply(batch_for_view.clone(), window, cx)
                        });
                    }
                })
                .map_err(|error| error.to_string())?;
        }
        *model_events += update.model_events.len();
    }
    Ok(())
}

#[cfg(feature = "screenshot")]
fn stream(
    transcript: &Transcript,
    options: &Options,
    lib: Arc<genui_core::r#lib::Library>,
    window: &mut gpui_kit::WindowHandle<DemoStage>,
    cx: &mut gpui_kit::HeadlessAppContext,
    args: &Args,
    capture: &mut impl FnMut(&mut gpui_kit::HeadlessAppContext, &Path) -> Result<(), String>,
) -> Result<(LiveRunner, RunReport), String> {
    let mut runner = LiveRunner::new(transcript.clone(), lib, options)?;
    let total = runner.chunk_count().max(1);
    let mut batches = Vec::new();
    let mut model_events = 0usize;
    let mut captured = Vec::new();
    let started = Instant::now();
    let mut next_quarter = 1usize;
    let mut frozen = false;
    while let Some(updates) = runner.step()? {
        let chunk = runner.chunk_index();
        apply_updates(updates, window, cx, &mut batches, &mut model_events)?;
        if args.freeze_at == Some(chunk) && !frozen {
            frozen = true;
            cx.run_until_parked();
            if let Some(dir) = &args.screenshot {
                let path = dir.join("frozen.png");
                capture(cx, &path)?;
                captured.push(path.display().to_string());
            }
        }
        let fraction = chunk * 4;
        if fraction >= next_quarter * total && next_quarter <= 4 {
            cx.run_until_parked();
            if let Some(dir) = &args.screenshot {
                let path = dir.join(format!("stream-{}pct.png", next_quarter * 25));
                capture(cx, &path)?;
                captured.push(path.display().to_string());
            }
            next_quarter += 1;
        }
    }
    cx.run_until_parked();
    if let Some(dir) = &args.screenshot {
        let path = dir.join("final.png");
        capture(cx, &path)?;
        captured.push(path.display().to_string());
    }
    let summary = window
        .update(cx, |stage, _, cx| {
            stage
                .child
                .as_ref()
                .map(|child| (child.read(cx).tree().len(), child.read(cx).diagnostics().to_vec()))
                .unwrap_or((0, Vec::new()))
        })
        .map_err(|error| error.to_string())?;
    let phase = format!("{:?}", runner.session().phase());
    report(&runner.session().diagnostics());
    Ok((
        runner,
        RunReport {
            transcript: transcript.id.clone(),
            prompt_digest: transcript.prompt_digest.clone(),
            chunks: total,
            batches: batches.len(),
            nodes: summary.0,
            model_events,
            diagnostics: summary
                .1
                .iter()
                .map(|diagnostic| format!("{:?}", diagnostic.code))
                .collect(),
            phase,
            screenshots: captured,
            frozen_at: args.freeze_at,
            restored: None,
            regenerated: None,
            wall_ms: started.elapsed().as_secs_f64() * 1000.0,
        },
    ))
}

#[cfg(feature = "screenshot")]
fn restore_check(
    runner: &LiveRunner,
    options: &Options,
    lib: Arc<genui_core::r#lib::Library>,
) -> Result<RestoreReport, String> {
    let session = runner.session();
    let stored = session.stored_message();
    let mut restored = Session::restore(
        &stored,
        SessionOptions {
            library: lib,
            validation: options.clone(),
            input: InputMode::Program,
            prompt_digest: session_options_digest(session),
        },
    );
    // Refs are host data, not part of the stored message; a real host
    // rehydrates them when it restores a conversation (SPEC §7.9).
    let mut rehydrated = false;
    for (reference, value) in &runner.transcript().preload {
        rehydrated |= restored.preload_ref(reference, value.clone());
    }
    if rehydrated {
        restored.snapshot();
    }
    let state_equal = restored.state_snapshot() == session.state_snapshot();
    let tree_equal = restored.tree().canonical_tree() == session.tree().canonical_tree();
    Ok(RestoreReport {
        ok: state_equal && tree_equal,
        state_equal,
        tree_equal,
        nodes: restored.tree().nodes.len(),
    })
}

#[cfg(feature = "screenshot")]
fn session_options_digest(session: &Session) -> String {
    session.stored_message().prompt_digest
}

#[cfg(feature = "screenshot")]
fn regenerate_check(
    transcript: &Transcript,
    options: &Options,
    lib: Arc<genui_core::r#lib::Library>,
    first: &[String],
) -> Result<RegenerateReport, String> {
    let second = replay_exact(transcript, lib, options)?;
    Ok(RegenerateReport {
        ok: true,
        identical: second == first,
    })
}

#[cfg(feature = "screenshot")]
fn replay_exact(
    transcript: &Transcript,
    lib: Arc<genui_core::r#lib::Library>,
    options: &Options,
) -> Result<Vec<String>, String> {
    let recording = genui_prompt::transcript::record(transcript, lib, options)?;
    recording
        .patches
        .iter()
        .map(|batch| batch.canonical_json().map_err(|error| error.to_string()))
        .collect()
}

#[cfg(feature = "screenshot")]
fn headless(args: &Args, transcript: &Transcript, options: &Options, lib: Arc<genui_core::r#lib::Library>) -> Result<(), String> {
    use gpui_kit::{
        AnyWindowHandle, HeadlessAppContext,
        platform::{current_headless_renderer, current_platform},
    };

    let theme = UiTheme::for_mode(args.theme);
    let text_system = current_platform(true).text_system();
    let mut cx =
        HeadlessAppContext::with_platform(text_system, Arc::new(()), current_headless_renderer);
    cx.update(gpui_kit::init);
    cx.update(|cx| {
        theme.install(cx);
        // Screenshot runs are deterministic stills: the host asks for reduced
        // motion, so a freshly inserted node is captured at full opacity
        // (DESIGN §8).
        cx.set_reduce_motion(true);
    });
    let title = format!(
        "Sample host · {} · {}",
        transcript.id,
        if transcript.label == SCRIPTED_LABEL {
            "scripted"
        } else {
            &transcript.label
        }
    );
    let window_theme = theme.clone();
    let caption = title.clone();
    let mut window = cx
        .open_window(theme.demo_window_size(), move |window, cx| {
            let stage = cx.new(|_| DemoStage::new(window_theme.clone()));
            let ui = cx.new(|cx| UiView::new(window_theme.clone(), window, cx));
            stage.update(cx, |stage, cx| {
                stage.set_program(ui, Some(caption));
                cx.notify();
            });
            stage
        })
        .map_err(|error| error.to_string())?;
    let handle = AnyWindowHandle::from(window.clone());
    let mut capture = |cx: &mut HeadlessAppContext, path: &Path| -> Result<(), String> {
        let image = cx
            .capture_screenshot(handle)
            .map_err(|error| error.to_string())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        image
            .save(path)
            .map_err(|error| format!("{}: {error}", path.display()))
    };
    if let Some(dir) = &args.screenshot {
        std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    }
    let (runner, mut run) = stream(
        transcript,
        options,
        lib.clone(),
        &mut window,
        &mut cx,
        args,
        &mut capture,
    )?;
    if args.restore {
        run.restored = Some(restore_check(&runner, options, lib.clone())?);
        if let Some(dir) = &args.screenshot {
            let path = dir.join("restored.png");
            capture(&mut cx, &path)?;
            run.screenshots.push(path.display().to_string());
        }
    }
    if args.regenerate {
        let first = replay_exact(transcript, lib.clone(), options)?;
        run.regenerated = Some(regenerate_check(transcript, options, lib.clone(), &first)?);
    }
    let _ = window.update(&mut cx, |_, window, _| window.remove_window());
    println!(
        "sample-host: {} · {} chunks · {} batches · {} nodes · {} event(s) · {} · {:.0} ms",
        run.transcript,
        run.chunks,
        run.batches,
        run.nodes,
        run.model_events,
        run.phase,
        run.wall_ms
    );
    if let Some(restored) = &run.restored {
        println!(
            "restore: state_equal={} tree_equal={} nodes={}",
            restored.state_equal, restored.tree_equal, restored.nodes
        );
        if !restored.ok {
            return Err("restore did not reproduce the tree and state exactly".into());
        }
    }
    if let Some(regenerated) = &run.regenerated {
        println!(
            "regenerate: identical={} ({} batches)",
            regenerated.identical,
            if regenerated.identical { "same" } else { "different" }
        );
        if !regenerated.identical {
            return Err("regenerate produced a different patch stream".into());
        }
    }
    if let Some(path) = &args.record {
        let value = serde_json::json!({
            "v": 1,
            "label": SCRIPTED_LABEL,
            "transcript": run.transcript,
            "prompt_digest": run.prompt_digest,
            "chunks": run.chunks,
            "batches": run.batches,
            "nodes": run.nodes,
            "model_events": run.model_events,
            "diagnostics": run.diagnostics,
            "phase": run.phase,
            "frozen_at": run.frozen_at,
            "screenshots": run.screenshots,
            "restore": run.restored.map(|r| serde_json::json!({
                "ok": r.ok,
                "state_equal": r.state_equal,
                "tree_equal": r.tree_equal,
                "nodes": r.nodes
            })),
            "regenerate": run.regenerated.map(|r| serde_json::json!({
                "ok": r.ok,
                "identical": r.identical
            })),
            "wall_ms": run.wall_ms
        });
        let text = serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?;
        std::fs::write(path, format!("{text}\n"))
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(())
}

#[cfg(not(feature = "screenshot"))]
fn headless(_: &Args, _: &Transcript, _: &Options, _: Arc<genui_core::r#lib::Library>) -> Result<(), String> {
    Err("sample-host --headless needs the `screenshot` feature".into())
}

fn prompt_options() -> PromptOptions {
    PromptOptions {
        capabilities: ["charts".into(), "images".into(), "markdown".into()].into(),
        ..PromptOptions::default()
    }
}

fn check_prompt(
    transcript: &Transcript,
    lib: &genui_core::r#lib::Library,
) -> Result<(), String> {
    let prompt = generate(lib.spec(), &prompt_options());
    if prompt.digest != transcript.prompt_digest {
        return Err(format!(
            "prompt digest differs from the recording: got {}, want {}",
            prompt.digest, transcript.prompt_digest
        ));
    }
    if prompt.text != transcript.prompt {
        return Err("recorded prompt text no longer matches the generator".into());
    }
    Ok(())
}

fn run(args: Args) -> Result<(), String> {
    let lib = library()?;
    let options = host_options()?;
    if args.replay_all {
        let paths = transcripts(&args.directory)?;
        if paths.len() < 20 {
            return Err(format!("expected 20 transcripts, found {}", paths.len()));
        }
        for path in &paths {
            let transcript = load(path)?;
            check_prompt(&transcript, &lib)?;
            replay(&transcript, lib.clone(), &options)
                .map_err(|error| format!("{}: {error}", path.display()))?;
        }
        println!("PASS: {} transcripts replay exactly", paths.len());
        return Ok(());
    }
    if args.list {
        for path in transcripts(&args.directory)? {
            let transcript = load(&path)?;
            println!(
                "{}  {} chunks  {}  {}",
                transcript.id,
                transcript.chunks.len(),
                transcript.label,
                transcript.source.as_deref().unwrap_or("-")
            );
        }
        return Ok(());
    }
    let path = match &args.transcript {
        Some(path) => path.clone(),
        None => transcripts(&args.directory)?
            .into_iter()
            .next()
            .ok_or("no transcripts found")?,
    };
    let transcript = load(&path)?;
    check_prompt(&transcript, &lib)?;
    if args.headless || args.screenshot.is_some() || args.record.is_some() || args.restore || args.regenerate {
        return headless(&args, &transcript, &options, lib);
    }
    Err("interactive window mode is not enabled in this build; pass --headless (see README)".into())
}

fn main() -> ExitCode {
    match parse_args(std::env::args().skip(1).collect()) {
        Ok(args) => match run(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

