//! Gallery: every exemplar and the component state sheet in light and dark.
//!
//! `--screenshot <dir>` uses gpui-kit's test-support headless renderer
//! (`Window::render_to_image`), the approved capture path from SPEC §12.7.
//! Without it the same content opens in a normal window.

use genui_core::validate::Options;
use genui_gpui::{
    demo::DemoStage,
    examples::{self, LoadedProgram},
    state_sheet,
    theme::{ThemeMode, UiTheme},
    view::UiView,
};
use gpui_kit::AppContext;
use serde::Deserialize;
use std::{path::PathBuf, process::ExitCode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Modes {
    Light,
    Dark,
    Both,
}

impl Modes {
    fn first(self) -> ThemeMode {
        match self {
            Modes::Light | Modes::Both => ThemeMode::Light,
            Modes::Dark => ThemeMode::Dark,
        }
    }

    #[cfg(feature = "screenshot")]
    fn selected(self) -> Vec<ThemeMode> {
        match self {
            Modes::Light => vec![ThemeMode::Light],
            Modes::Dark => vec![ThemeMode::Dark],
            Modes::Both => vec![ThemeMode::Light, ThemeMode::Dark],
        }
    }
}

#[derive(Clone, Debug)]
struct Args {
    screenshot: Option<PathBuf>,
    modes: Modes,
    example: Option<String>,
    inspector: bool,
    list: bool,
    sheet_only: bool,
    self_test_freshness: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            screenshot: None,
            modes: Modes::Both,
            example: None,
            inspector: false,
            list: false,
            sheet_only: false,
            self_test_freshness: false,
        }
    }
}

fn parse_args(args: Vec<String>) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--screenshot" => {
                index += 1;
                let value = args.get(index).ok_or("--screenshot needs a directory")?;
                parsed.screenshot = Some(PathBuf::from(value));
            }
            "--mode" => {
                index += 1;
                parsed.modes = match args.get(index).map(String::as_str) {
                    Some("light") => Modes::Light,
                    Some("dark") => Modes::Dark,
                    Some("both") => Modes::Both,
                    _ => return Err("--mode expects light, dark or both".into()),
                };
            }
            "--example" => {
                index += 1;
                parsed.example = Some(args.get(index).ok_or("--example needs a name")?.clone());
            }
            "--inspector" => parsed.inspector = true,
            "--list" => parsed.list = true,
            "--sheet" => parsed.sheet_only = true,
            "--self-test-freshness" => parsed.self_test_freshness = true,
            "--help" | "-h" => {
                return Err(
                    "usage: gallery [--screenshot DIR] [--mode light|dark|both]\n\
                     \x20              [--example NAME] [--inspector] [--list] [--sheet]"
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
        ..Default::default()
    })
}

#[derive(Clone)]
struct Item {
    name: String,
    caption: String,
    batches: Vec<genui_core::patch::PatchBatch>,
    diagnostics: Vec<genui_core::diag::Diagnostic>,
    overrides: genui_gpui::layout::StateOverrides,
    nodes: usize,
    deferred: Vec<String>,
    warnings: usize,
    state_sheet: bool,
    state: std::collections::BTreeMap<String, genui_core::runtime::StateValue>,
}

fn items(args: &Args) -> Result<Vec<Item>, String> {
    let mut items = Vec::new();
    if !args.sheet_only {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        let options = host_options()?;
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&root)
            .map_err(|error| format!("{}: {error}", root.display()))?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|ext| ext == "ui"))
            .collect();
        paths.sort();
        for path in paths {
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("example")
                .to_string();
            if let Some(filter) = &args.example
                && !name.contains(filter)
            {
                continue;
            }
            let source =
                std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
            let loaded = examples::session(&source, &options)
                .map_err(|error| format!("{}: {error}", name))?;
            items.push(item_from_loaded(name, loaded));
        }
        if items.is_empty() {
            return Err("no examples matched".into());
        }
    }
    items.push(state_sheet_item());
    Ok(items)
}

fn item_from_loaded(name: String, loaded: LoadedProgram) -> Item {
    let warnings = loaded
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity >= genui_core::diag::Severity::Warning)
        .count();
    Item {
        caption: format!("{} · {} nodes", name, loaded.tree.len()),
        name,
        batches: loaded.batches,
        diagnostics: loaded.diagnostics,
        overrides: Default::default(),
        nodes: loaded.tree.len(),
        deferred: loaded.deferred,
        warnings,
        state_sheet: false,
        state: loaded.state,
    }
}

fn state_sheet_item() -> Item {
    let sheet = state_sheet::build();
    Item {
        name: "state-sheet".into(),
        caption: "Component states · every implemented component, all DESIGN §7 states".into(),
        batches: state_sheet::batches(),
        diagnostics: Vec::new(),
        overrides: sheet.overrides,
        nodes: sheet.tree.len(),
        deferred: Vec::new(),
        warnings: 0,
        state_sheet: true,
        state: sheet.state.clone(),
    }
}

fn list(items: &[Item]) {
    for item in items {
        println!(
            "{:<28} {:>4} nodes  {:>2} warnings  {}",
            item.name,
            item.nodes,
            item.warnings,
            if item.deferred.is_empty() {
                "all components implemented".to_string()
            } else {
                format!("deferred: {}", item.deferred.join(", "))
            }
        );
    }
}

fn mode_name(mode: ThemeMode) -> &'static str {
    mode.name()
}

#[cfg(feature = "screenshot")]
fn safe_name(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn make_ui(
    item: &Item,
    theme: &UiTheme,
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::App,
    max_width: Option<gpui_kit::Pixels>,
) -> gpui_kit::Entity<UiView> {
    let ui = cx.new(|cx| {
        UiView::new(theme.clone(), window, cx)
            .with_overrides(item.overrides.clone())
            .with_diagnostics(item.diagnostics.clone())
    });
    if let Some(width) = max_width {
        ui.update(cx, |view, cx| view.set_max_width(width, cx));
    }
    for batch in &item.batches {
        ui.update(cx, |view, cx| view.apply(batch.clone(), window, cx));
    }
    ui.update(cx, |view, cx| {
        view.sync_state(item.state.clone(), window, cx);
    });
    ui
}

#[cfg(feature = "screenshot")]
fn screenshot(args: &Args, items: &[Item]) -> Result<(), String> {
    use gpui_kit::{
        AnyWindowHandle, AppContext, HeadlessAppContext,
        platform::{current_headless_renderer, current_platform},
    };
    use std::sync::Arc;

    let directory = args
        .screenshot
        .as_ref()
        .ok_or("screenshot mode needs a directory")?;
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    let text_system = current_platform(true).text_system();
    let mut cx =
        HeadlessAppContext::with_platform(text_system, Arc::new(()), current_headless_renderer);
    cx.update(gpui_kit::init);
    let mut manifest = String::new();
    let mut captured = 0usize;
    for item in items {
        for mode in args.modes.selected() {
            let theme = UiTheme::for_mode(mode);
            cx.update(|cx| theme.install(cx));
            let size = if item.state_sheet {
                theme.state_sheet_window_size()
            } else {
                theme.demo_window_size()
            };
            let caption = format!("{} · {}", item.caption, mode_name(mode));
            let stage_theme = theme.clone();
            let stage_item = item.clone();
            let show_inspector = args.inspector;
            let window = cx
                .open_window(size, move |window, cx| {
                    let stage = cx.new(|_| DemoStage::new(stage_theme.clone()));
                    let max_width = if show_inspector {
                        Some(gpui_kit::px(
                            stage_theme.demo_window_size().width.as_f32()
                                - stage_theme.inspector_width().as_f32()
                                - stage_theme.space(6).as_f32() * 2.0,
                        ))
                    } else {
                        None
                    };
                    let ui = make_ui(&stage_item, &stage_theme, window, cx, max_width);
                    stage.update(cx, |stage, cx| {
                        stage.set_program(ui, Some(caption));
                        stage.show_inspector = show_inspector;
                        stage.refresh_inspector(cx);
                        cx.notify();
                    });
                    stage
                })
                .map_err(|error| format!("{}: {error}", item.name))?;
            let handle = AnyWindowHandle::from(window);
            cx.run_until_parked();
            let image = cx
                .capture_screenshot(handle)
                .map_err(|error| format!("{}: {error}", item.name))?;
            let file = directory.join(format!("{}-{}.png", safe_name(&item.name), mode_name(mode)));
            image
                .save(&file)
                .map_err(|error| format!("{}: {error}", file.display()))?;
            manifest.push_str(&format!(
                "{}\t{}\t{}x{}\t{} nodes\t{} warnings\n",
                item.name,
                mode_name(mode),
                image.width(),
                image.height(),
                item.nodes,
                item.warnings
            ));
            captured += 1;
            // Release the window and its scene before the next capture.
            let _ = window.update(&mut cx, |_, window, _| window.remove_window());
        }
    }
    std::fs::write(directory.join("manifest.tsv"), manifest)
        .map_err(|error| format!("manifest: {error}"))?;
    println!(
        "captured {captured} screenshots for {} items in {}",
        items.len(),
        directory.display()
    );
    Ok(())
}

#[cfg(not(feature = "screenshot"))]
fn screenshot(_args: &Args, _items: &[Item]) -> Result<(), String> {
    Err("gallery was built without the `screenshot` feature; run \
         `cargo run -p genui-gpui --features screenshot --bin gallery -- --screenshot DIR`"
        .into())
}

fn interactive(args: &Args, items: Vec<Item>) -> Result<(), String> {
    let show_inspector = args.inspector;
    let theme = UiTheme::for_mode(args.modes.first());
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        theme.install(cx);
        let size = theme.demo_window_size();
        let options = gpui_kit::WindowOptions {
            window_bounds: Some(gpui_kit::WindowBounds::Windowed(
                gpui_kit::Bounds::centered(None, size, cx),
            )),
            focus: true,
            titlebar: Some(gpui_kit::TitlebarOptions {
                title: Some("Genui gallery".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let first = items.first().cloned();
        if let Err(error) = gpui_kit::open_window(options, cx, move |window, cx| {
            let stage = cx.new(|_| DemoStage::new(theme.clone()));
            if let Some(item) = first {
                if item.state_sheet {
                    window.resize(theme.state_sheet_window_size());
                }
                let max_width = if show_inspector {
                    Some(gpui_kit::px(
                        theme.demo_window_size().width.as_f32()
                            - theme.inspector_width().as_f32()
                            - theme.space(6).as_f32() * 2.0,
                    ))
                } else {
                    None
                };
                let ui = make_ui(&item, &theme, window, cx, max_width);
                let caption = format!("{} · {}", item.caption, mode_name(theme.mode()));
                stage.update(cx, |stage, cx| {
                    stage.set_program(ui, Some(caption));
                    stage.show_inspector = show_inspector;
                    stage.refresh_inspector(cx);
                    cx.notify();
                });
            }
            stage
        }) {
            eprintln!("gallery: {error}");
            cx.quit();
        }
    });
    Ok(())
}

fn run(args: Args) -> Result<(), String> {
    if args.self_test_freshness {
        return freshness::run();
    }
    let items = items(&args)?;
    if args.list {
        list(&items);
        return Ok(());
    }
    if args.screenshot.is_some() {
        return screenshot(&args, &items);
    }
    interactive(&args, items)
}

#[cfg(feature = "screenshot")]
mod freshness {
    use genui_core::{
        ir::{NodeStatus, Role},
        patch::{NodeInit, Op, PatchBatch},
    };
    use genui_gpui::{demo::DemoStage, theme::UiTheme, view::UiView};
    use gpui_kit::{
        AnyWindowHandle, AppContext, HeadlessAppContext,
        platform::{current_headless_renderer, current_platform},
    };
    use serde_json::json;
    use std::{collections::BTreeMap, sync::Arc};

    fn node(id: &str, kind: &str, role: Role, text: Option<&str>) -> NodeInit {
        let props = text
            .map(|text| {
                BTreeMap::from([
                    ("text".to_string(), json!(text)),
                    ("size".to_string(), json!("md")),
                ])
            })
            .unwrap_or_default();
        NodeInit {
            id: id.into(),
            kind: kind.into(),
            role,
            props,
            status: NodeStatus::Sealed,
        }
    }

    fn batch(seq: u64, ops: Vec<Op>) -> PatchBatch {
        PatchBatch { epoch: 1, seq, ops }
    }

    pub fn run() -> Result<(), String> {
        let theme = UiTheme::for_mode(genui_gpui::theme::ThemeMode::Light);
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
                    stage.set_program(ui, None);
                    cx.notify();
                });
                stage
            })
            .map_err(|error| error.to_string())?;
        let handle = AnyWindowHandle::from(window);
        cx.run_until_parked();
        let empty = cx
            .capture_screenshot(handle)
            .map_err(|error| error.to_string())?;
        let first = batch(
            0,
            vec![
                Op::Reset,
                Op::Insert {
                    parent: None,
                    slot: "root".into(),
                    index: 0,
                    node: node("root", "Column", Role::Group, None),
                },
                Op::Insert {
                    parent: Some("root".into()),
                    slot: "children".into(),
                    index: 0,
                    node: node(
                        "root/children:#0",
                        "Text",
                        Role::Paragraph,
                        Some("one batch, no later trigger"),
                    ),
                },
            ],
        );
        window
            .update(&mut cx, |stage, window, cx| {
                if let Some(ui) = &stage.child {
                    ui.update(cx, |view, cx| view.apply(first, window, cx));
                }
            })
            .map_err(|error| error.to_string())?;
        let after = cx
            .capture_screenshot(handle)
            .map_err(|error| error.to_string())?;
        let mut changed = 0usize;
        for (a, b) in empty.pixels().zip(after.pixels()) {
            if a != b {
                changed += 1;
            }
        }
        if changed == 0 {
            return Err("freshness: the applied batch did not reach the capture".into());
        }
        let second = batch(
            1,
            vec![Op::AppendText {
                id: "root/children:#0".into(),
                prop: "text".into(),
                text: " · appended".into(),
            }],
        );
        window
            .update(&mut cx, |stage, window, cx| {
                if let Some(ui) = &stage.child {
                    ui.update(cx, |view, cx| view.apply(second, window, cx));
                }
            })
            .map_err(|error| error.to_string())?;
        let appended = cx
            .capture_screenshot(handle)
            .map_err(|error| error.to_string())?;
        if appended.as_raw() == after.as_raw() {
            return Err("freshness: append_text did not reach the capture".into());
        }
        let reset = batch(2, vec![Op::Reset]);
        window
            .update(&mut cx, |stage, window, cx| {
                if let Some(ui) = &stage.child {
                    ui.update(cx, |view, cx| view.apply(reset, window, cx));
                }
            })
            .map_err(|error| error.to_string())?;
        let cleared = cx
            .capture_screenshot(handle)
            .map_err(|error| error.to_string())?;
        if cleared.as_raw() != empty.as_raw() {
            return Err("freshness: reset did not clear the tree".into());
        }
        let _ = window.update(&mut cx, |_, window, _| window.remove_window());
        println!(
            "freshness: one batch visible without a later trigger ({changed} pixels), append_text visible, reset cleared"
        );
        Ok(())
    }
}

#[cfg(not(feature = "screenshot"))]
mod freshness {
    pub fn run() -> Result<(), String> {
        Err("--self-test-freshness needs the `screenshot` feature".into())
    }
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
