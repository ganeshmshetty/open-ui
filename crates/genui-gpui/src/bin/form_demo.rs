//! Runnable form demo (SPEC §14.1 M7; DESIGN §§6.7, 7).
//!
//! Loads `examples/25-form-controls.ui`, streams it through `Session`, renders
//! it with `UiView`, and acts as the host: every `UiEvent` is printed and passed
//! back to the session, whose patches are applied before the frame and whose
//! `$state` is pushed into the inputs.
//!
//! `--screenshot DIR` uses the test-support headless backend. `--headless`
//! renders once without a visible window and prints its node/state summary.

use genui_core::{
    Session, SessionOptions,
    message::ModelEvent,
    session::{InputMode, UiEvent},
};
use genui_gpui::{
    theme::{TextRole, ThemeMode, UiTheme},
    view::UiView,
};
use gpui_kit::{
    AnyView, AppContext as _, Context, Entity, IntoElement, ParentElement, Render, Styled,
    Subscription, Window, div,
};
use std::{path::PathBuf, process::ExitCode, sync::Arc};

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
    program: Option<PathBuf>,
    headless: bool,
    self_test: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            screenshot: None,
            modes: Modes::Both,
            program: None,
            headless: false,
            self_test: false,
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
                parsed.screenshot = Some(PathBuf::from(
                    args.get(index).ok_or("--screenshot needs a directory")?,
                ));
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
            "--program" => {
                index += 1;
                parsed.program = Some(PathBuf::from(
                    args.get(index).ok_or("--program needs a file")?,
                ));
            }
            "--headless" => parsed.headless = true,
            "--self-test" => parsed.self_test = true,
            "--help" | "-h" => {
                return Err(
                    "usage: form-demo [--screenshot DIR] [--mode light|dark|both]\n\
                     \x20                [--program FILE] [--headless] [--self-test]"
                        .into(),
                );
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
        index += 1;
    }
    Ok(parsed)
}

fn default_program() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/25-form-controls.ui")
}

fn load_source(args: &Args) -> Result<String, String> {
    let path = args.program.clone().unwrap_or_else(default_program);
    std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))
}

fn make_ui(
    source: &str,
    theme: &UiTheme,
    window: &mut Window,
    cx: &mut gpui_kit::App,
) -> Result<(Session, Entity<UiView>), String> {
    let library = Arc::new(genui_std::std_library().map_err(|errors| format!("{errors:?}"))?);
    let mut session = Session::new(SessionOptions {
        library,
        validation: UiView::capabilities().validation_options(),
        input: InputMode::Program,
        prompt_digest: String::new(),
    });
    session.push(source.as_bytes());
    session.finish();
    let ui = cx.new(|cx| UiView::new(theme.clone(), window, cx));
    let snapshot = session.snapshot();
    if let Some(batch) = snapshot.patches {
        ui.update(cx, |ui, cx| ui.apply(batch, window, cx));
    }
    let state = session.state_values();
    ui.update(cx, |ui, cx| ui.sync_state(state, window, cx));
    Ok((session, ui))
}

struct FormHost {
    theme: UiTheme,
    session: Session,
    ui: Entity<UiView>,
    model_events: usize,
    _subscription: Subscription,
}

impl FormHost {
    fn apply_update(
        &mut self,
        update: genui_core::session::Update,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(batch) = update.patches {
            self.ui.update(cx, |ui, cx| ui.apply(batch, window, cx));
        }
        let state = self.session.state_values();
        self.ui
            .update(cx, |ui, cx| ui.sync_state(state, window, cx));
        for request in &update.requests {
            println!("host-request {}", request.id.0);
        }
        for event in &update.model_events {
            self.model_events += 1;
            print_model_event(event);
        }
    }

    fn on_event(
        &mut self,
        _ui: &Entity<UiView>,
        event: &UiEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        print_ui_event(event);
        let update = self.session.event(event.clone());
        self.apply_update(update, window, cx);
    }
}

impl Render for FormHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let caption = self.theme.text(TextRole::Caption);
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(self.theme.background())
            .p(self.theme.space(6))
            .gap(self.theme.space(2))
            .child(
                div()
                    .text_size(caption.size)
                    .line_height(caption.line_height)
                    .text_color(self.theme.muted_foreground())
                    .child(
                        "Form controls · Tab moves focus · Enter in a field submits when the form has one submit button",
                    ),
            )
            .child(AnyView::from(self.ui.clone()))
    }
}

fn print_ui_event(event: &UiEvent) {
    match event {
        UiEvent::Activate { node, action } => println!(
            "ui-event activate node={} epoch={} plan={}",
            node.0, action.epoch, action.plan.0
        ),
        UiEvent::Edit { node, value } => println!(
            "ui-event edit node={} value={}",
            node.0,
            serde_json::to_string(value).unwrap_or_else(|_| "null".into())
        ),
        UiEvent::Blur { node } => println!("ui-event blur node={}", node.0),
    }
}

fn print_model_event(event: &ModelEvent) {
    println!(
        "model-event kind={} label={:?} text={:?}",
        event.kind, event.label, event.text
    );
}

fn build_host(
    source: &str,
    theme: UiTheme,
    window: &mut Window,
    cx: &mut Context<FormHost>,
) -> Result<FormHost, String> {
    let (session, ui) = make_ui(source, &theme, window, cx)?;
    let subscription = cx.subscribe_in(&ui, window, FormHost::on_event);
    Ok(FormHost {
        theme,
        session,
        ui,
        model_events: 0,
        _subscription: subscription,
    })
}

#[cfg(feature = "screenshot")]
fn summary(ui: &Entity<UiView>, cx: &gpui_kit::App) -> String {
    let view = ui.read(cx);
    let tree = view.tree();
    let kinds: Vec<String> = {
        let mut kinds: Vec<String> = tree
            .nodes
            .values()
            .map(|entry| entry.kind.clone())
            .collect();
        kinds.sort();
        kinds.dedup();
        kinds
    };
    let state = view
        .state_values()
        .keys()
        .cloned()
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{} nodes · kinds: {} · state: {state}",
        tree.len(),
        kinds.join(",")
    )
}

#[cfg(feature = "screenshot")]
fn screenshot(args: &Args, source: &str) -> Result<(), String> {
    use gpui_kit::{
        AnyWindowHandle, HeadlessAppContext,
        platform::{current_headless_renderer, current_platform},
    };

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
    for mode in args.modes.selected() {
        let theme = UiTheme::for_mode(mode);
        cx.update(|cx| theme.install(cx));
        let size = theme.demo_window_size();
        let window_theme = theme.clone();
        let source = source.to_string();
        let window = cx
            .open_window(size, move |window, cx| {
                cx.new(|cx| build_host(&source, window_theme, window, cx).expect("form host"))
            })
            .map_err(|error| error.to_string())?;
        let handle = AnyWindowHandle::from(window);
        cx.run_until_parked();
        let summary_text = window
            .read_with(&cx, |host, cx| summary(&host.ui, cx))
            .map_err(|error| error.to_string())?;
        let image = cx
            .capture_screenshot(handle)
            .map_err(|error| error.to_string())?;
        let file = directory.join(format!("form-demo-{}.png", mode.name()));
        image
            .save(&file)
            .map_err(|error| format!("{}: {error}", file.display()))?;
        manifest.push_str(&format!(
            "{}\t{}x{}\t{}\n",
            file.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("form"),
            image.width(),
            image.height(),
            summary_text
        ));
        println!("captured {} · {summary_text}", file.display());
        let _ = window.update(&mut cx, |_, window, _| window.remove_window());
    }
    std::fs::write(directory.join("manifest.tsv"), manifest)
        .map_err(|error| format!("manifest: {error}"))?;
    Ok(())
}

#[cfg(not(feature = "screenshot"))]
fn screenshot(_args: &Args, _source: &str) -> Result<(), String> {
    Err("form-demo was built without the `screenshot` feature; run \
         `cargo run -p genui-gpui --features screenshot --bin form-demo -- --screenshot DIR`"
        .into())
}

fn headless(args: &Args, source: &str) -> Result<(), String> {
    #[cfg(feature = "screenshot")]
    {
        use gpui_kit::{
            HeadlessAppContext,
            platform::{current_headless_renderer, current_platform},
        };
        let theme = UiTheme::for_mode(args.modes.first());
        let text_system = current_platform(true).text_system();
        let mut cx =
            HeadlessAppContext::with_platform(text_system, Arc::new(()), current_headless_renderer);
        cx.update(gpui_kit::init);
        cx.update(|cx| theme.install(cx));
        let window_theme = theme.clone();
        let source = source.to_string();
        let window = cx
            .open_window(theme.demo_window_size(), move |window, cx| {
                cx.new(|cx| build_host(&source, window_theme, window, cx).expect("form host"))
            })
            .map_err(|error| error.to_string())?;
        cx.run_until_parked();
        let summary_text = window
            .read_with(&cx, |host, cx| summary(&host.ui, cx))
            .map_err(|error| error.to_string())?;
        let tree = window
            .read_with(&cx, |host, cx| host.ui.read(cx).tree().outline())
            .map_err(|error| error.to_string())?;
        if args.self_test {
            for kind in [
                "Input",
                "TextArea",
                "Select",
                "Checkbox",
                "CheckboxGroup",
                "RadioGroup",
                "Switch",
                "Form",
                "Button",
            ] {
                if !tree.contains(kind) {
                    return Err(format!("self-test: missing {kind}"));
                }
            }
            let state = window
                .read_with(&cx, |host, cx| host.ui.read(cx).state_values().len())
                .map_err(|error| error.to_string())?;
            if state < 8 {
                return Err(format!("self-test: expected 8 bound fields, got {state}"));
            }
            println!("PASS form-demo self-test · {summary_text}");
        } else {
            println!("headless form · {summary_text}");
            println!("{tree}");
        }
        Ok(())
    }
    #[cfg(not(feature = "screenshot"))]
    {
        let _ = (args, source);
        Err("--headless needs the `screenshot` feature (test-support renderer)".into())
    }
}

fn interactive(args: &Args, source: &str) -> Result<(), String> {
    let theme = UiTheme::for_mode(args.modes.first());
    let source = source.to_string();
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
                title: Some("Genui form demo".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let program = source.clone();
        let window_theme = theme.clone();
        if let Err(error) = gpui_kit::open_window(options, cx, move |window, cx| {
            cx.new(|cx| {
                build_host(&program, window_theme, window, cx)
                    .expect("form host: the program comes from the demo example")
            })
        }) {
            eprintln!("form-demo: {error}");
            cx.quit();
        }
    });
    let _ = args;
    Ok(())
}

fn run(args: Args) -> Result<(), String> {
    let source = load_source(&args)?;
    if args.screenshot.is_some() {
        return screenshot(&args, &source);
    }
    if args.headless || args.self_test {
        return headless(&args, &source);
    }
    interactive(&args, &source)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(args).and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("form-demo: {error}");
            ExitCode::FAILURE
        }
    }
}
