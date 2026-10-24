//! 50,000-row table stress (SPEC §10.6, §14.1 M8).
//!
//! Builds a ref-backed table, preloads rows as host data, renders it through
//! `UiView` and scrolls to distant rows. Rows are virtualized by the kit table,
//! so the visible range stays bounded while the data model grows. Numbers are
//! reported against the 8 ms app-work target and are not waived.
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
            rows: 50_000,
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
                return Err("usage: table-stress [--rows N] [--metrics FILE] [--screenshot DIR] [--light|--dark]".into());
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
        index += 1;
    }
    Ok(parsed)
}

fn rows(count: usize) -> Value {
    let mut items = Vec::with_capacity(count);
    for index in 0..count {
        let customer = format!("Customer {index:05}");
        let total = (index as i64 % 10_000) as f64 + 0.5;
        let status = match index % 3 {
            0 => "Paid",
            1 => "Pending",
            _ => "Shipped",
        };
        items.push(Value::Map(Arc::new(vec![
            ("index".into(), Value::Int(index as i64)),
            ("customer".into(), Value::from(customer)),
            ("total".into(), Value::Float(total)),
            ("status".into(), Value::from(status)),
            (
                "region".into(),
                Value::from(["North", "South", "East", "West"][index % 4]),
            ),
        ])));
    }
    Value::List(Arc::new(items))
}

fn session(count: usize) -> Result<Session, String> {
    let library = genui_std::std_library().map_err(|errors| format!("std build: {errors:?}"))?;
    let options = Options {
        refs: ["stress.rows".into()].into(),
        limits: genui_core::limits::Limits {
            max_list_len: 100_000,
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
    if !session.preload_ref("stress.rows", rows(count)) {
        return Err("the table stress ref was rejected by the preload caps".into());
    }
    session.push(
        b"root=Table([TableColumn(\"Index\",\"index\",align:\"end\",format:\"number\"),TableColumn(\"Customer\",\"customer\"),TableColumn(\"Total\",\"total\",align:\"end\",format:\"currency\"),TableColumn(\"Status\",\"status\"),TableColumn(\"Region\",\"region\")],ref(\"stress.rows\"))",
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
                stage.set_program(ui, Some("50,000-row table stress".into()));
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
    {
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
    }
    cx.run_until_parked();
    let initial = cx
        .capture_screenshot(handle)
        .map_err(|error| error.to_string())?;
    if let Some(dir) = &args.screenshot {
        std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        initial
            .save(dir.join("table-stress-initial.png"))
            .map_err(|error| error.to_string())?;
    }
    let build_ms = build_started.elapsed().as_secs_f64() * 1000.0;
    drop(initial);
    let summary = window
        .update(&mut cx, |stage, _, cx| {
            stage
                .child
                .as_ref()
                .map(|child| child.read(cx).metrics())
                .unwrap_or_default()
        })
        .map_err(|error| error.to_string())?;
    let table_parse_ms = window
        .update(&mut cx, |stage, _, cx| {
            stage
                .child
                .as_ref()
                .map(|child| child.read(cx).metrics_handle().borrow().table_parse_ns as f64 / 1e6)
                .unwrap_or(0.0)
        })
        .map_err(|error| error.to_string())?;
    let (rows, mut ranges) = window
        .update(&mut cx, |stage, _, cx| {
            stage
                .child
                .as_ref()
                .map(|child| {
                    let view = child.read(cx);
                    (
                        view.table_row_count("root", cx).unwrap_or(0),
                        view.table_visible_range("root", cx)
                            .map(|range| vec![range])
                            .unwrap_or_default(),
                    )
                })
                .unwrap_or((0, Vec::new()))
        })
        .map_err(|error| error.to_string())?;
    let mut scroll_ms = Vec::new();
    for (index, row) in [1000usize, 10_000, 25_000, args.rows.saturating_sub(20)]
        .into_iter()
        .enumerate()
    {
        let started = Instant::now();
        window
            .update(&mut cx, |stage, _, cx| {
                if let Some(ui) = &stage.child {
                    ui.update(cx, |view, cx| {
                        view.scroll_table("root", row, cx);
                    });
                }
            })
            .map_err(|error| error.to_string())?;
        cx.run_until_parked();
        if std::env::var_os("GENUI_TABLE_DEBUG").is_some() {
            let samples = window
                .update(&mut cx, |stage, _, cx| {
                    stage.child.as_ref().map(|child| {
                        child
                            .read(cx)
                            .metrics_handle()
                            .borrow()
                            .frame_app_samples
                            .clone()
                    })
                })
                .ok()
                .flatten()
                .unwrap_or_default();
            eprintln!(
                "scroll {row}: frame samples {:?}",
                samples
                    .iter()
                    .map(|d| d.as_secs_f64() * 1000.0)
                    .collect::<Vec<_>>()
            );
        }
        let image = cx
            .capture_screenshot(handle)
            .map_err(|error| error.to_string())?;
        scroll_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        drop(image);
        let range = window
            .update(&mut cx, |stage, _, cx| {
                stage
                    .child
                    .as_ref()
                    .and_then(|child| child.read(cx).table_visible_range("root", cx))
                    .unwrap_or((0, 0))
            })
            .map_err(|error| error.to_string())?;
        ranges.push(range);
        if let Some(dir) = &args.screenshot {
            let path = dir.join(format!("table-stress-scroll-{index}.png"));
            // Re-capture because the previous image was dropped to bound memory.
            let image = cx
                .capture_screenshot(handle)
                .map_err(|error| error.to_string())?;
            image.save(&path).map_err(|error| error.to_string())?;
        }
    }
    let final_summary = window
        .update(&mut cx, |stage, _, cx| {
            stage
                .child
                .as_ref()
                .map(|child| child.read(cx).metrics())
                .unwrap_or_default()
        })
        .map_err(|error| error.to_string())?;
    drop(session);
    let rss_after_session = rss_bytes();
    let rss = rss_bytes();
    let final_samples = window
        .update(&mut cx, |stage, _, cx| {
            stage
                .child
                .as_ref()
                .map(|child| {
                    child
                        .read(cx)
                        .metrics_handle()
                        .borrow()
                        .frame_app_samples
                        .clone()
                })
                .unwrap_or_default()
        })
        .map_err(|error| error.to_string())?;
    let mut all_samples = final_samples.clone();
    all_samples.sort();
    let construction_ms = all_samples
        .last()
        .map_or(0.0, |sample| sample.as_secs_f64() * 1000.0);
    let steady_samples = all_samples
        .split_last()
        .map_or_else(Vec::new, |(_, rest)| rest.to_vec());
    let steady = genui_gpui::metrics::Metrics {
        frame_app_samples: steady_samples,
        ..Default::default()
    }
    .summary()
    .frame_app;
    let _ = window.update(&mut cx, |_, window, _| window.remove_window());
    let max_visible = ranges
        .iter()
        .map(|(start, end)| end.saturating_sub(*start))
        .max()
        .unwrap_or(0);
    println!(
        "table stress: {rows} rows, {batches} batches, max visible rows {max_visible}, build {build_ms:.1} ms"
    );
    println!(
        "frame app work (all): p50 {:.2}ms p95 {:.2}ms p99 {:.2}ms max {:.2}ms; construction frame {construction_ms:.2}ms; table parse {table_parse_ms:.2}ms",
        final_summary.frame_app.p50_ms,
        final_summary.frame_app.p95_ms,
        final_summary.frame_app.p99_ms,
        final_summary.frame_app.max_ms
    );
    println!(
        "steady frames (excluding the largest construction sample): p50 {:.2}ms p95 {:.2}ms p99 {:.2}ms max {:.2}ms",
        steady.p50_ms, steady.p95_ms, steady.p99_ms, steady.max_ms
    );
    println!(
        "scroll+capture: {} samples, max {:.1} ms; rss {}",
        scroll_ms.len(),
        scroll_ms.iter().copied().fold(0.0, f64::max),
        rss.map(|bytes| format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)))
            .unwrap_or_else(|| "n/a".into())
    );
    if let Some(path) = &args.metrics {
        let value: Json = json!({
            "rows": rows,
            "batches": batches,
            "build_ms": build_ms,
            "table_parse_ms": table_parse_ms,
            "construction_frame_ms": construction_ms,
            "max_visible_rows": max_visible,
            "visible_ranges": ranges.iter().map(|(start, end)| json!([start, end])).collect::<Vec<_>>(),
            "frame_app": {
                "count": final_summary.frame_app.count,
                "p50_ms": final_summary.frame_app.p50_ms,
                "p95_ms": final_summary.frame_app.p95_ms,
                "p99_ms": final_summary.frame_app.p99_ms,
                "max_ms": final_summary.frame_app.max_ms
            },
            "frame_samples_ms": all_samples
                .iter()
                .map(|sample| sample.as_secs_f64() * 1000.0)
                .collect::<Vec<_>>(),
            "steady_frame_app": {
                "count": steady.count,
                "p50_ms": steady.p50_ms,
                "p95_ms": steady.p95_ms,
                "p99_ms": steady.p99_ms,
                "max_ms": steady.max_ms
            },
            "apply": {
                "count": final_summary.apply.count,
                "p50_ms": final_summary.apply.p50_ms,
                "p99_ms": final_summary.apply.p99_ms
            },
            "render": {
                "count": final_summary.render.count,
                "p99_ms": final_summary.render.p99_ms
            },
            "initial_summary": {
                "frames": summary.frames,
                "nodes": summary.frames
            },
            "scroll_capture_ms": scroll_ms,
            "rss_bytes": rss,
            "rss_after_session_drop": rss_after_session,
            "baseline_rss_bytes": baseline_rss,
            "wall_ms": started.elapsed().as_secs_f64() * 1000.0
        });
        let text = serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?;
        std::fs::write(path, format!("{text}\n"))
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    if max_visible > 64 {
        return Err(format!(
            "virtualization failed: {max_visible} rows were visible at once"
        ));
    }
    Ok(())
}

#[cfg(not(feature = "screenshot"))]
fn run(_: &Args) -> Result<(), String> {
    Err("table-stress needs the `screenshot` feature (headless window + capture)".into())
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
