//! 10,000-item List stress (SPEC §10.3, §14.1 M8).
//!
//! Above 100 children the List uses the kit virtual list. This binary builds
//! 10,000 `Text` items through a ref-backed `each`, then scrolls to distant
//! rows. It reports how many item entities were created and how many rows were
//! built in the last frame, plus construction/app-work/RSS numbers.
#![cfg_attr(not(feature = "screenshot"), allow(dead_code, unused_imports))]

use genui_core::{Session, SessionOptions, session::InputMode, validate::Options, value::Value};
use genui_gpui::{demo::DemoStage, theme::UiTheme, view::UiView};
use gpui_kit::AppContext;
use serde_json::{Value as Json, json};
use std::{path::PathBuf, process::ExitCode, sync::Arc, time::Instant};

#[derive(Clone, Debug)]
struct Args {
    rows: usize,
    metrics: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    theme: genui_gpui::theme::ThemeMode,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            rows: 10_000,
            metrics: None,
            screenshot: None,
            theme: genui_gpui::theme::ThemeMode::Light,
        }
    }
}

fn parse(args: Vec<String>) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--rows" => {
                index += 1;
                parsed.rows = args
                    .get(index)
                    .ok_or("--rows needs a count")?
                    .parse()
                    .map_err(|_| "--rows expects an integer")?;
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
                    args.get(index).ok_or("--screenshot needs a directory")?,
                ));
            }
            "--dark" => parsed.theme = genui_gpui::theme::ThemeMode::Dark,
            "--light" => parsed.theme = genui_gpui::theme::ThemeMode::Light,
            "--help" | "-h" => {
                return Err("usage: list-stress [--rows N] [--metrics FILE] [--screenshot DIR] [--light|--dark]".into());
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
        index += 1;
    }
    Ok(parsed)
}

fn items(count: usize) -> Value {
    let mut rows = Vec::with_capacity(count);
    for index in 0..count {
        rows.push(Value::Map(Arc::new(vec![
            ("id".into(), Value::Int(index as i64)),
            ("label".into(), Value::from(format!("List item {index:05}"))),
        ])));
    }
    Value::List(Arc::new(rows))
}

fn session(count: usize) -> Result<Session, String> {
    let library = genui_std::std_library().map_err(|errors| format!("std build: {errors:?}"))?;
    let options = Options {
        refs: ["stress.list".into()].into(),
        limits: genui_core::limits::Limits {
            max_list_len: 100_000,
            each_max_items: 20_000,
            max_nodes: 100_000,
            max_resolved_data_bytes: 64 * 1024 * 1024,
            ..genui_core::limits::Limits::default()
        },
        capabilities: ["charts".into(), "images".into(), "markdown".into()].into(),
        ..Options::default()
    };
    let mut session = Session::new(SessionOptions {
        library: Arc::new(library),
        validation: options,
        input: InputMode::Program,
        prompt_digest: String::new(),
    });
    if !session.preload_ref("stress.list", items(count)) {
        return Err("the list stress ref was rejected by the preload caps".into());
    }
    session.push(
        b"root=List(each(ref(\"stress.list\"),|item| Text(item.label),key:\"id\"),ordered:true)",
    );
    session.finish();
    Ok(session)
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

#[cfg(feature = "screenshot")]
fn run(args: &Args) -> Result<(), String> {
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
        cx.set_reduce_motion(true);
    });
    let started = Instant::now();
    let baseline_rss = rss_bytes();
    let window_theme = theme.clone();
    let window = cx
        .open_window(theme.demo_window_size(), move |window, cx| {
            let stage = cx.new(|_| DemoStage::new(window_theme.clone()));
            let ui = cx.new(|cx| UiView::new(window_theme.clone(), window, cx));
            stage.update(cx, |stage, cx| {
                stage.set_program(ui, Some("10,000-item List stress".into()));
                cx.notify();
            });
            stage
        })
        .map_err(|error| error.to_string())?;
    let handle = AnyWindowHandle::from(window);
    let mut session = session(args.rows)?;
    let build_started = Instant::now();
    let mut batches = 0;
    let update = session.snapshot();
    if let Some(batch) = update.patches {
        batches += 1;
        window
            .update(&mut cx, |stage, window, cx| {
                if let Some(ui) = &stage.child {
                    ui.update(cx, |view, cx| view.apply(batch.clone(), window, cx));
                }
            })
            .map_err(|error| error.to_string())?;
    }
    cx.run_until_parked();
    let initial = cx
        .capture_screenshot(handle)
        .map_err(|error| error.to_string())?;
    if let Some(dir) = &args.screenshot {
        std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        initial
            .save(dir.join("list-stress-initial.png"))
            .map_err(|error| error.to_string())?;
    }
    drop(initial);
    let build_ms = build_started.elapsed().as_secs_f64() * 1000.0;
    let children = window
        .update(&mut cx, |stage, _, cx| {
            stage
                .child
                .as_ref()
                .and_then(|child| {
                    child
                        .read(cx)
                        .tree()
                        .get("root")
                        .map(|entry| entry.primary_children().len())
                })
                .unwrap_or(0)
        })
        .map_err(|error| error.to_string())?;
    let mut created_max = 0usize;
    let mut frame_max = 0usize;
    let mut samples = Vec::new();
    let mut offsets = vec![0usize, 1_000, 5_000, args.rows.saturating_sub(1)];
    offsets.dedup();
    for (index, row) in offsets.into_iter().enumerate() {
        let scroll_started = Instant::now();
        window
            .update(&mut cx, |stage, window, cx| {
                if let Some(ui) = &stage.child {
                    ui.update(cx, |view, cx| {
                        view.scroll_list("root", row, window, cx);
                    });
                }
            })
            .map_err(|error| error.to_string())?;
        cx.run_until_parked();
        let image = cx
            .capture_screenshot(handle)
            .map_err(|error| error.to_string())?;
        let elapsed = scroll_started.elapsed().as_secs_f64() * 1000.0;
        if let Some(dir) = &args.screenshot {
            image
                .save(dir.join(format!("list-stress-scroll-{index}.png")))
                .map_err(|error| error.to_string())?;
        }
        drop(image);
        let (created, frame) = window
            .update(&mut cx, |stage, _, cx| {
                stage
                    .child
                    .as_ref()
                    .map(|child| {
                        let view = child.read(cx);
                        (
                            view.list_created_rows("root", cx).unwrap_or(0),
                            view.list_frame_rows("root", cx).unwrap_or(0),
                        )
                    })
                    .unwrap_or((0, 0))
            })
            .map_err(|error| error.to_string())?;
        created_max = created_max.max(created);
        frame_max = frame_max.max(frame);
        samples.push(json!({
            "row": row,
            "created_rows": created,
            "frame_rows": frame,
            "scroll_capture_ms": elapsed
        }));
    }
    let summary = window
        .update(&mut cx, |stage, _, cx| {
            stage
                .child
                .as_ref()
                .map(|child| child.read(cx).metrics())
                .unwrap_or_default()
        })
        .map_err(|error| error.to_string())?;
    let rss = rss_bytes();
    let _ = window.update(&mut cx, |_, window, _| window.remove_window());
    println!(
        "list stress: {children} children, {batches} batches, max created rows {created_max}, max frame rows {frame_max}, build {build_ms:.1} ms"
    );
    println!(
        "frame app work: p50 {:.2}ms p95 {:.2}ms p99 {:.2}ms max {:.2}ms; rss {}",
        summary.frame_app.p50_ms,
        summary.frame_app.p95_ms,
        summary.frame_app.p99_ms,
        summary.frame_app.max_ms,
        rss.map(|bytes| format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)))
            .unwrap_or_else(|| "n/a".into())
    );
    if let Some(path) = &args.metrics {
        let value: Json = json!({
            "children": children,
            "batches": batches,
            "build_ms": build_ms,
            "created_rows_max": created_max,
            "frame_rows_max": frame_max,
            "frame_app": {
                "count": summary.frame_app.count,
                "p50_ms": summary.frame_app.p50_ms,
                "p95_ms": summary.frame_app.p95_ms,
                "p99_ms": summary.frame_app.p99_ms,
                "max_ms": summary.frame_app.max_ms
            },
            "apply": {
                "count": summary.apply.count,
                "p50_ms": summary.apply.p50_ms,
                "p99_ms": summary.apply.p99_ms
            },
            "scroll_samples": samples,
            "rss_bytes": rss,
            "baseline_rss_bytes": baseline_rss,
            "wall_ms": started.elapsed().as_secs_f64() * 1000.0
        });
        let text = serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?;
        std::fs::write(path, format!("{text}\n"))
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    if children <= 100 {
        return Err(format!(
            "the stress list did not cross the virtualization threshold: {children} children"
        ));
    }
    if frame_max > 64 {
        return Err(format!(
            "virtualization failed: {frame_max} rows were built in one frame"
        ));
    }
    if created_max > 256 {
        return Err(format!(
            "virtualization failed: {created_max} row entities were created"
        ));
    }
    Ok(())
}

#[cfg(not(feature = "screenshot"))]
fn run(_: &Args) -> Result<(), String> {
    Err("list-stress needs the `screenshot` feature (headless window + capture)".into())
}

fn main() -> ExitCode {
    match parse(std::env::args().skip(1).collect()) {
        Ok(args) => match run(&args) {
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
