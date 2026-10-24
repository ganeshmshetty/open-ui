//! Streaming dashboard demo (ROADMAP item 19, SPEC §14.1 M6).
//!
//! Streams an exemplar through `Session` into `UiView`, applies each batch
//! before the next frame, records the patch log, and reports app-work frame
//! numbers and RSS. `--screenshot` uses the test-support headless renderer.

use genui_core::{
    Session, SessionOptions,
    diag::{Diagnostic, Severity},
    patch::PatchBatch,
    session::InputMode,
    validate::Options,
};
use genui_gpui::{
    demo::DemoStage,
    metrics::Summary,
    theme::{ThemeMode, UiTheme},
    tree::RetainedTree,
    view::UiView,
};
use gpui_kit::AppContext;
use serde::Deserialize;
use std::{path::PathBuf, process::ExitCode, sync::Arc, time::Instant};

#[derive(Clone, Debug)]
struct Args {
    file: PathBuf,
    chunk: usize,
    delay_ms: u64,
    patch_log: Option<PathBuf>,
    metrics: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    print_patches: bool,
    stress: Option<usize>,
    replay: Option<PathBuf>,
    capture_every: usize,
    theme_mode: ThemeMode,
    headless: bool,
}

impl Default for Args {
    fn default() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        Self {
            file: root.join("../../examples/01-sales-dashboard.ui"),
            chunk: 5,
            delay_ms: 16,
            patch_log: None,
            metrics: None,
            screenshot: None,
            print_patches: false,
            stress: None,
            replay: None,
            capture_every: 8,
            theme_mode: ThemeMode::Light,
            headless: false,
        }
    }
}

fn parse_args(args: Vec<String>) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--file" => {
                index += 1;
                parsed.file = PathBuf::from(args.get(index).ok_or("--file needs a path")?);
            }
            "--chunk" => {
                index += 1;
                parsed.chunk = args
                    .get(index)
                    .ok_or("--chunk needs a positive size")?
                    .parse()
                    .map_err(|_| "--chunk expects a positive integer")?;
                if parsed.chunk == 0 {
                    return Err("--chunk expects a positive integer".into());
                }
            }
            "--delay-ms" => {
                index += 1;
                parsed.delay_ms = args
                    .get(index)
                    .ok_or("--delay-ms needs a value")?
                    .parse()
                    .map_err(|_| "--delay-ms expects an integer")?;
            }
            "--patch-log" => {
                index += 1;
                parsed.patch_log = Some(PathBuf::from(
                    args.get(index).ok_or("--patch-log needs a path")?,
                ));
            }
            "--metrics" => {
                index += 1;
                parsed.metrics = Some(PathBuf::from(
                    args.get(index).ok_or("--metrics needs a path")?,
                ));
            }
            "--screenshot" => {
                index += 1;
                parsed.screenshot = Some(PathBuf::from(
                    args.get(index).ok_or("--screenshot needs a path")?,
                ));
            }
            "--print-patches" => parsed.print_patches = true,
            "--stress" => {
                index += 1;
                parsed.stress = Some(
                    args.get(index)
                        .ok_or("--stress needs a node count")?
                        .parse()
                        .map_err(|_| "--stress expects an integer")?,
                );
            }
            "--replay" => {
                index += 1;
                parsed.replay = Some(PathBuf::from(
                    args.get(index).ok_or("--replay needs a path")?,
                ));
            }
            "--capture-every" => {
                index += 1;
                parsed.capture_every = args
                    .get(index)
                    .ok_or("--capture-every needs a value")?
                    .parse::<usize>()
                    .map_err(|_| "--capture-every expects an integer")?
                    .max(1);
            }
            "--headless" => parsed.headless = true,
            "--dark" => parsed.theme_mode = ThemeMode::Dark,
            "--light" => parsed.theme_mode = ThemeMode::Light,
            "--help" | "-h" => {
                return Err(
                    "usage: dashboard [--file FILE] [--chunk N] [--delay-ms N]\n\
                     \x20                 [--patch-log FILE] [--metrics FILE]\n\
                     \x20                 [--screenshot FILE] [--print-patches]\n\
                     \x20                 [--stress N] [--replay FILE] [--capture-every N]\n\
                     \x20                 [--headless] [--light|--dark]"
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

fn host_options() -> Options {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance/libs/host.json");
    let Ok(bytes) = std::fs::read_to_string(path) else {
        return Options::default();
    };
    let Ok(host) = serde_json::from_str::<HostFile>(&bytes) else {
        return Options::default();
    };
    Options {
        tools: host.tools.into_iter().collect(),
        refs: host.refs.into_iter().collect(),
        url_hosts: host.url_hosts.into_iter().collect(),
        ..Default::default()
    }
}

fn source(args: &Args) -> Result<Vec<u8>, String> {
    if let Some(count) = args.stress {
        // Group the Text nodes so the program stays well inside the
        // statement-count and statement-size budgets (SPEC §7.1).
        let groups = count.div_ceil(20);
        let mut text = String::from("root=Column([");
        for group in 0..groups {
            if group > 0 {
                text.push(',');
            }
            text.push_str(&format!("g{group}"));
        }
        text.push_str("])\n");
        for group in 0..groups {
            text.push_str(&format!("g{group}=Column(["));
            for item in 0..20 {
                let index = group * 20 + item;
                if index >= count {
                    break;
                }
                if item > 0 {
                    text.push(',');
                }
                text.push_str(&format!("Text(\"Streamed node {index}\")"));
            }
            text.push_str("])\n");
        }
        return Ok(text.into_bytes());
    }
    std::fs::read(&args.file).map_err(|error| format!("{}: {error}", args.file.display()))
}

fn new_session() -> Result<Session, String> {
    let library = genui_std::std_library().map_err(|errors| format!("std library: {errors:?}"))?;
    Ok(Session::new(SessionOptions {
        library: Arc::new(library),
        validation: host_options(),
        input: InputMode::Program,
    }))
}

fn rss_bytes() -> Option<u64> {
    let pid = std::process::id().to_string();
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let kb: u64 = text.trim().parse().ok()?;
    Some(kb * 1024)
}

fn write_log(path: &PathBuf, log: &[PatchBatch]) -> Result<(), String> {
    let mut text = String::new();
    for batch in log {
        let json = batch.canonical_json().map_err(|error| error.to_string())?;
        text.push_str(&json);
        text.push('\n');
    }
    std::fs::write(path, text).map_err(|error| format!("{}: {error}", path.display()))
}

fn write_metrics(
    path: &PathBuf,
    summary: &Summary,
    nodes: usize,
    rss: Option<u64>,
    wall_ms: f64,
    frames: usize,
    baseline_rss: Option<u64>,
) -> Result<(), String> {
    let value = serde_json::json!({
        "frames": summary.frames,
        "queue_drains": summary.queue_drains,
        "batches_enqueued": summary.batches_enqueued,
        "batches_applied": summary.batches_applied,
        "ops_applied": summary.ops_applied,
        "nodes": nodes,
        "captured_frames": frames,
        "snapshot_requests": summary.snapshot_requests,
        "apply": {
            "count": summary.apply.count,
            "p50_ms": summary.apply.p50_ms,
            "p95_ms": summary.apply.p95_ms,
            "p99_ms": summary.apply.p99_ms,
            "max_ms": summary.apply.max_ms
        },
        "render": {
            "count": summary.render.count,
            "p50_ms": summary.render.p50_ms,
            "p95_ms": summary.render.p95_ms,
            "p99_ms": summary.render.p99_ms,
            "max_ms": summary.render.max_ms
        },
        "node_render_total_ms": summary.node_render_ns as f64 / 1_000_000.0,
        "frame_app": {
            "count": summary.frame_app.count,
            "p50_ms": summary.frame_app.p50_ms,
            "p95_ms": summary.frame_app.p95_ms,
            "p99_ms": summary.frame_app.p99_ms,
            "max_ms": summary.frame_app.max_ms
        },
        "rss_bytes": rss,
        "baseline_rss_bytes": baseline_rss,
        "rss_delta_bytes": match (rss, baseline_rss) {
            (Some(now), Some(base)) => Some(now.saturating_sub(base)),
            _ => None
        },
        "wall_ms": wall_ms
    });
    let text = serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?;
    std::fs::write(path, text + "\n").map_err(|error| format!("{}: {error}", path.display()))
}

fn replay(path: &PathBuf) -> Result<(), String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut tree = RetainedTree::default();
    let mut batches = 0;
    for (line, raw) in text.lines().enumerate() {
        if raw.trim().is_empty() {
            continue;
        }
        let batch: PatchBatch =
            serde_json::from_str(raw).map_err(|error| format!("line {}: {error}", line + 1))?;
        let report = tree.apply(&batch);
        if !report.errors.is_empty() {
            return Err(format!(
                "line {}: renderer rejected the batch: {:?}",
                line + 1,
                report.errors
            ));
        }
        batches += 1;
    }
    println!(
        "replay: {batches} batches, {} nodes, epoch {:?}, no panic",
        tree.len(),
        tree.epoch
    );
    println!("{}", tree.outline());
    Ok(())
}

fn report_diagnostics(diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        if diagnostic.severity >= Severity::Warning {
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
type HeadlessResult = (Summary, usize, Vec<f64>, usize, Option<u64>);

#[cfg(feature = "screenshot")]
fn run_headless(args: &Args, source: &[u8]) -> Result<HeadlessResult, String> {
    use gpui_kit::{
        AnyWindowHandle, AppContext, HeadlessAppContext,
        platform::{current_headless_renderer, current_platform},
    };

    let theme = UiTheme::for_mode(args.theme_mode);
    let text_system = current_platform(true).text_system();
    let mut cx =
        HeadlessAppContext::with_platform(text_system, Arc::new(()), current_headless_renderer);
    cx.update(gpui_kit::init);
    cx.update(|cx| theme.install(cx));
    let window_theme = theme.clone();
    let window = cx
        .open_window(theme.demo_window_size(), move |window, cx| {
            let stage = cx.new(|_| DemoStage::new(window_theme.clone()));
            let ui = cx.new(|cx| UiView::new(window_theme.clone(), window, cx));
            stage.update(cx, |stage, cx| {
                stage.set_program(ui, Some("Streaming dashboard".into()));
                cx.notify();
            });
            stage
        })
        .map_err(|error| error.to_string())?;
    let handle = AnyWindowHandle::from(window);
    let baseline_rss = rss_bytes();
    let mut session = new_session()?;
    let mut log = Vec::new();
    let mut diagnostics = Vec::new();
    let mut chunks = 0usize;
    let mut captured = 0usize;
    let mut frame_ms = Vec::new();
    for chunk in source.chunks(args.chunk) {
        let update = session.push(chunk);
        if let Some(batch) = update.patches {
            if args.print_patches
                && let Ok(json) = batch.canonical_json()
            {
                println!("{json}");
            }
            let batch_for_view = batch.clone();
            window
                .update(&mut cx, |stage, window, cx| {
                    if let Some(ui) = &stage.child {
                        ui.update(cx, |view, cx| {
                            view.apply(batch_for_view.clone(), window, cx)
                        });
                    }
                })
                .map_err(|error| error.to_string())?;
            log.push(batch);
        }
        diagnostics.extend(update.diagnostics);
        chunks += 1;
        if chunks.is_multiple_of(args.capture_every) {
            cx.run_until_parked();
            let started = Instant::now();
            let image = cx
                .capture_screenshot(handle)
                .map_err(|error| error.to_string())?;
            frame_ms.push(started.elapsed().as_secs_f64() * 1000.0);
            captured += 1;
            drop(image);
        }
    }
    let finish = session.finish();
    if let Some(batch) = finish.patches {
        let batch_for_view = batch.clone();
        window
            .update(&mut cx, |stage, window, cx| {
                if let Some(ui) = &stage.child {
                    ui.update(cx, |view, cx| {
                        view.apply(batch_for_view.clone(), window, cx)
                    });
                }
            })
            .map_err(|error| error.to_string())?;
        log.push(batch);
    }
    diagnostics.extend(finish.diagnostics);
    diagnostics = session.diagnostics().to_vec();
    cx.run_until_parked();
    let image = cx
        .capture_screenshot(handle)
        .map_err(|error| error.to_string())?;
    if let Some(path) = &args.screenshot {
        image
            .save(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    captured += 1;
    let summary = window
        .update(&mut cx, |stage, _, cx| {
            stage
                .child
                .as_ref()
                .map(|child| child.read(cx).metrics())
                .unwrap_or_default()
        })
        .map_err(|error| error.to_string())?;
    let nodes = window
        .update(&mut cx, |stage, _, cx| {
            stage
                .child
                .as_ref()
                .map(|child| child.read(cx).tree().len())
                .unwrap_or(0)
        })
        .map_err(|error| error.to_string())?;
    if let Some(path) = &args.patch_log {
        write_log(path, &log)?;
    }
    if std::env::var_os("GENUI_DEBUG_RENDERS").is_some() {
        let debug = window
            .update(&mut cx, |stage, _, cx| {
                stage.child.as_ref().map(|child| {
                    let metrics = child.read(cx).metrics_handle();
                    let m = metrics.borrow();
                    (m.node_renders.clone(), m.chart_renders, m.image_renders)
                })
            })
            .ok()
            .flatten();
        eprintln!("renders: {debug:?}");
    }
    report_diagnostics(&diagnostics);
    let _ = window.update(&mut cx, |_, window, _| window.remove_window());
    if session.phase() == genui_core::session::SessionPhase::Rejected {
        return Err("session rejected the program".into());
    }
    println!(
        "stream: {} bytes, {chunks} chunks, {} batches, {nodes} nodes, {captured} frames",
        source.len(),
        log.len()
    );
    Ok((summary, nodes, frame_ms, captured, baseline_rss))
}

fn run_interactive(args: &Args, source: Vec<u8>) -> Result<(), String> {
    let theme = UiTheme::for_mode(args.theme_mode);
    let delay = args.delay_ms;
    let chunk = args.chunk;
    let metrics_path = args.metrics.clone();
    let log_path = args.patch_log.clone();
    let print_patches = args.print_patches;
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        theme.install(cx);
        let window_theme = theme.clone();
        let opened = gpui_kit::open_window(
            gpui_kit::WindowOptions {
                window_bounds: Some(gpui_kit::WindowBounds::Windowed(
                    gpui_kit::Bounds::centered(None, window_theme.demo_window_size(), cx),
                )),
                focus: true,
                titlebar: Some(gpui_kit::TitlebarOptions {
                    title: Some("Genui streaming dashboard".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            cx,
            move |window, cx| {
                let stage = cx.new(|_| DemoStage::new(window_theme.clone()));
                let ui = cx.new(|cx| UiView::new(window_theme.clone(), window, cx));
                stage.update(cx, |stage, cx| {
                    stage.set_program(ui, Some("Streaming dashboard".into()));
                    cx.notify();
                });
                stage
            },
        );
        let (window, stage) = match opened {
            Ok(value) => value,
            Err(error) => {
                eprintln!("dashboard: could not open a window: {error}");
                cx.quit();
                return;
            }
        };
        let ui = stage.read(cx).child.clone();
        let Some(ui) = ui else {
            eprintln!("dashboard: window has no program view");
            cx.quit();
            return;
        };
        cx.spawn(async move |cx| {
            let mut session = match new_session() {
                Ok(session) => session,
                Err(error) => {
                    eprintln!("{error}");
                    cx.update(|cx| cx.quit());
                    return;
                }
            };
            let mut log = Vec::new();
            let mut diagnostics = Vec::new();
            let started = Instant::now();
            for chunk in source.chunks(chunk) {
                let update = session.push(chunk);
                if let Some(batch) = update.patches {
                    if print_patches
                        && let Ok(json) = batch.canonical_json()
                    {
                        println!("{json}");
                    }
                    let batch_for_view = batch.clone();
                    let _ = window.update(cx, |_, window, cx| {
                        ui.update(cx, |view, cx| {
                            view.apply(batch_for_view.clone(), window, cx)
                        });
                    });
                    log.push(batch);
                }
                diagnostics.extend(update.diagnostics);
                if delay > 0 {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(delay))
                        .await;
                }
            }
            let finish = session.finish();
            if let Some(batch) = finish.patches {
                let batch_for_view = batch.clone();
                let _ = window.update(cx, |_, window, cx| {
                    ui.update(cx, |view, cx| {
                        view.apply(batch_for_view.clone(), window, cx)
                    });
                });
                log.push(batch);
            }
            diagnostics.extend(finish.diagnostics);
            report_diagnostics(&diagnostics);
            if let Some(path) = &log_path {
                let _ = write_log(path, &log);
            }
            let summary = cx.update(|cx| ui.read(cx).metrics());
            let nodes = cx.update(|cx| ui.read(cx).tree().len());
            let rss = rss_bytes();
            let wall = started.elapsed().as_secs_f64() * 1000.0;
            println!(
                "stream: {} batches, {nodes} nodes, apply p50 {:.2}ms p99 {:.2}ms, render p50 {:.2}ms p99 {:.2}ms, rss {}",
                log.len(),
                summary.apply.p50_ms,
                summary.apply.p99_ms,
                summary.render.p50_ms,
                summary.render.p99_ms,
                rss.map(|bytes| format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)))
                    .unwrap_or_else(|| "n/a".into())
            );
            if let Some(path) = &metrics_path {
                let _ = write_metrics(path, &summary, nodes, rss, wall, 0, None);
            }
        })
        .detach();
    });
    Ok(())
}

fn run(args: Args) -> Result<(), String> {
    if let Some(path) = &args.replay {
        return replay(path);
    }
    let source = source(&args)?;
    if args.headless || args.screenshot.is_some() || args.metrics.is_some() {
        #[cfg(feature = "screenshot")]
        {
            let (summary, nodes, mut frame_ms, captured, baseline_rss) =
                run_headless(&args, &source)?;
            let rss = rss_bytes();
            frame_ms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let p50 = frame_ms.get(frame_ms.len() / 2).copied().unwrap_or(0.0);
            let p99 = frame_ms
                .get((frame_ms.len() * 99 / 100).min(frame_ms.len().saturating_sub(1)))
                .copied()
                .unwrap_or(0.0);
            println!(
                "metrics: nodes {nodes}, apply p50 {:.2}ms p99 {:.2}ms, render p50 {:.2}ms p99 {:.2}ms",
                summary.apply.p50_ms,
                summary.apply.p99_ms,
                summary.render.p50_ms,
                summary.render.p99_ms
            );
            println!(
                "frame app work: p50 {:.2}ms p95 {:.2}ms p99 {:.2}ms max {:.2}ms (node element builds)",
                summary.frame_app.p50_ms,
                summary.frame_app.p95_ms,
                summary.frame_app.p99_ms,
                summary.frame_app.max_ms
            );
            println!(
                "frame total (capture path): p50 {p50:.2}ms p99 {p99:.2}ms over {captured} captures; rss {} (baseline {})",
                rss.map(|bytes| format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)))
                    .unwrap_or_else(|| "n/a".into()),
                baseline_rss
                    .map(|bytes| format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)))
                    .unwrap_or_else(|| "n/a".into())
            );
            if let Some(path) = &args.metrics {
                write_metrics(
                    path,
                    &summary,
                    nodes,
                    rss,
                    frame_ms.iter().sum(),
                    captured,
                    baseline_rss,
                )?;
            }
            return Ok(());
        }
        #[cfg(not(feature = "screenshot"))]
        {
            return Err("dashboard --screenshot/--metrics needs the `screenshot` feature".into());
        }
    }
    run_interactive(&args, source)
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
