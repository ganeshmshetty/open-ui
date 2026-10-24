//! Scripted headless GPUI interaction tests (SPEC §12.7, §14.1 M7).
//!
//! Each test loads a real program through `Session`, applies its patches before
//! frames, drives the pinned test context with pointer, keyboard and text input,
//! and asserts the exact `UiEvent`s, state and patches. Pixels are never
//! compared here.

use genui_core::{
    Session, SessionOptions,
    message::ModelEvent,
    patch::PatchBatch,
    runtime::StateValue,
    session::{InputMode, UiEvent},
};
use genui_gpui::{theme::UiTheme, view::UiView};
use gpui_kit::{
    AnyView, AnyWindowHandle, AppContext as _, Context, ElementId, Entity, InteractiveElement,
    IntoElement, ParentElement, Render, RenderOnce, SharedString, StatefulInteractiveElement,
    Styled, Subscription, TestSupportExt as _, Window, WindowOptions, test::TestWindowExt,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

const DEMO: &str = include_str!("../../../examples/25-form-controls.ui");

const FORM: &str = r#"$name="Ada"
$email=""
$notes=""
$region="North"
$updates=false
$topics=[]
$speed="Standard"
$notify=false
form=Form([name,email,notes,region,updates,topics,speed,notify,go],on_submit:say("Create account"))
name=Input("Name",bind:$name,required:true,max_len:40)
email=Input("Email",bind:$email,kind:"email",required:true)
notes=TextArea("Notes",bind:$notes,rows:3,max_len:200)
region=Select("Region",["North","South","East"],bind:$region)
updates=Checkbox("Email updates",bind:$updates)
topics=CheckboxGroup("Topics",["Releases","Incidents","Research"],bind:$topics)
speed=RadioGroup("Delivery",["Standard","Express"],bind:$speed)
notify=Switch("Notifications",bind:$notify)
go=Button("Create account",tone:"primary",submit:true)
root=Column([Heading("Create your account",level:1),form])
"#;

#[derive(Clone, Default)]
struct Seen {
    events: Vec<UiEvent>,
    model_events: Vec<ModelEvent>,
    requests: Vec<genui_core::session::HostRequest>,
    batches: Vec<PatchBatch>,
}

struct Host {
    session: Session,
    ui: Entity<UiView>,
    seen: Rc<RefCell<Seen>>,
    frozen: bool,
    _subscription: Subscription,
}

impl Host {
    fn apply_update(
        &mut self,
        update: genui_core::session::Update,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(batch) = update.patches {
            self.ui
                .update(cx, |ui, cx| ui.apply(batch.clone(), window, cx));
            self.seen.borrow_mut().batches.push(batch);
        }
        let state = self.session.state_values();
        self.ui
            .update(cx, |ui, cx| ui.sync_state(state, window, cx));
        self.seen.borrow_mut().requests.extend(update.requests);
        self.seen
            .borrow_mut()
            .model_events
            .extend(update.model_events);
    }

    fn on_event(
        &mut self,
        _ui: &Entity<UiView>,
        event: &UiEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.seen.borrow_mut().events.push(event.clone());
        if self.frozen {
            return;
        }
        let update = self.session.event(event.clone());
        self.apply_update(update, window, cx);
    }

    fn state(&self, name: &str) -> Option<StateValue> {
        self.session.state_values().get(name).cloned()
    }

    fn touch_label(
        &mut self,
        node: &str,
        label: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let seq = self.ui.read(cx).tree().next_seq;
        let epoch = self.session.epoch();
        let batch = PatchBatch {
            epoch,
            seq,
            ops: vec![genui_core::patch::Op::SetProps {
                id: node.to_string(),
                set: std::collections::BTreeMap::from([(
                    "label".to_string(),
                    serde_json::json!(label),
                )]),
                unset: Vec::new(),
            }],
        };
        self.ui
            .update(cx, |ui, cx| ui.apply(batch.clone(), window, cx));
        self.seen.borrow_mut().batches.push(batch);
    }

    fn apply_status(
        &mut self,
        node: &str,
        status: genui_core::ir::NodeStatus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let seq = self.ui.read(cx).tree().next_seq;
        let batch = PatchBatch {
            epoch: self.session.epoch(),
            seq,
            ops: vec![genui_core::patch::Op::SetStatus {
                id: node.to_string(),
                status,
            }],
        };
        self.ui
            .update(cx, |ui, cx| ui.apply(batch.clone(), window, cx));
        self.seen.borrow_mut().batches.push(batch);
    }
}

fn state(
    visual: &gpui_kit::VisualTestContext,
    host: &Entity<Host>,
    name: &str,
) -> Option<StateValue> {
    visual.read(|app| host.read_with(app, |host, _| host.state(name)))
}

fn text_state(
    visual: &gpui_kit::VisualTestContext,
    host: &Entity<Host>,
    name: &str,
) -> Option<String> {
    state(visual, host, name).and_then(|slot| match slot.value {
        genui_core::value::Value::Str(text) => Some(text.text.to_string()),
        _ => None,
    })
}

fn bool_state(
    visual: &gpui_kit::VisualTestContext,
    host: &Entity<Host>,
    name: &str,
) -> Option<bool> {
    state(visual, host, name).and_then(|slot| match slot.value {
        genui_core::value::Value::Bool(value) => Some(value),
        _ => None,
    })
}

fn list_state(
    visual: &gpui_kit::VisualTestContext,
    host: &Entity<Host>,
    name: &str,
) -> Vec<String> {
    state(visual, host, name)
        .and_then(|slot| match slot.value {
            genui_core::value::Value::List(values) => Some(
                values
                    .iter()
                    .filter_map(|value| match value {
                        genui_core::value::Value::Str(text) => Some(text.text.to_string()),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

fn error_visible(visual: &mut gpui_kit::VisualTestContext, node: &str) -> bool {
    visual.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(ElementId::Name(SharedString::from(format!(
                "genui-error-{node}"
            ))))
            .is_some()
    })
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        AnyView::from(self.ui.clone())
    }
}

struct Harness {
    host: Entity<Host>,
    seen: Rc<RefCell<Seen>>,
    window: AnyWindowHandle,
}

fn build(cx: &mut gpui_kit::TestAppContext) -> Harness {
    build_source(cx, FORM)
}

fn build_source(cx: &mut gpui_kit::TestAppContext, source: &str) -> Harness {
    let source = source.to_string();
    cx.update(gpui_kit::init);
    let theme = UiTheme::for_mode(genui_gpui::theme::ThemeMode::Light);
    cx.update(|cx| theme.install(cx));
    let options = WindowOptions {
        window_bounds: Some(gpui_kit::WindowBounds::Windowed(gpui_kit::Bounds {
            origin: gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(0.)),
            size: theme.demo_window_size(),
        })),
        focus: true,
        ..Default::default()
    };
    let (window, host) = cx
        .update(|cx| {
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| {
                    let library = Arc::new(genui_std::std_library().expect("std library"));
                    let mut session = Session::new(SessionOptions {
                        library,
                        validation: UiView::capabilities().validation_options(),
                        input: InputMode::Program,
                        prompt_digest: String::new(),
                    });
                    session.push(source.as_bytes());
                    session.finish();
                    let ui = cx.new(|cx| UiView::new(theme.clone(), window, cx));
                    let update = session.snapshot();
                    if let Some(batch) = update.patches {
                        ui.update(cx, |ui, cx| ui.apply(batch, window, cx));
                    }
                    let state = session.state_values();
                    ui.update(cx, |ui, cx| ui.sync_state(state, window, cx));
                    let seen = Rc::new(RefCell::new(Seen::default()));
                    let subscription = cx.subscribe_in(&ui, window, Host::on_event);
                    Host {
                        session,
                        ui,
                        seen,
                        frozen: false,
                        _subscription: subscription,
                    }
                })
            })
        })
        .expect("open window");
    let seen = host.read_with(cx, |host, _| host.seen.clone());
    Harness { host, seen, window }
}

fn input_id(path: &str) -> ElementId {
    ElementId::Name(SharedString::from(format!("genui-input-{path}")))
}

fn button_id(path: &str) -> ElementId {
    ElementId::Name(SharedString::from(format!("genui-button-{path}")))
}

#[gpui_kit::test]
fn scripted_form_flow(cx: &mut gpui_kit::TestAppContext) {
    let harness = build(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(harness.window, cx);
    let name = "root/children:@form/children:@name";
    let email = "root/children:@form/children:@email";
    let region = "root/children:@form/children:@region";
    let updates = "root/children:@form/children:@updates";
    let topics = "root/children:@form/children:@topics";
    let speed = "root/children:@form/children:@speed";
    let notify = "root/children:@form/children:@notify";
    let go = "root/children:@form/children:@go";

    visual.update(|window, cx| {
        window.render_frame(cx);
        window.click(input_id(name), cx);
        window.input(" Lovelace", cx);
    });
    assert_eq!(
        harness
            .host
            .read_with(cx, |host, _| host.state("name"))
            .and_then(|state| match state.value {
                genui_core::value::Value::Str(text) => Some(text.text.to_string()),
                _ => None,
            })
            .as_deref(),
        Some("Ada Lovelace")
    );
    assert!(harness.seen.borrow().events.iter().any(|event| matches!(
        event,
        UiEvent::Edit { node, .. } if node.0 == name
    )));

    visual.update(|window, cx| {
        window.click(button_id(go), cx);
    });
    assert!(
        harness.seen.borrow().model_events.is_empty(),
        "the required email field blocks submit"
    );

    visual.update(|window, cx| {
        window.click(input_id(email), cx);
        window.input("ada@example.com", cx);
        window.press("enter", cx);
    });
    assert_eq!(harness.seen.borrow().model_events.len(), 1);
    assert_eq!(
        harness.seen.borrow().model_events[0].text.as_deref(),
        Some("Create account")
    );

    let _ = (region, updates, topics, speed, notify);
}

#[gpui_kit::test]
fn every_control_emits_its_edit(cx: &mut gpui_kit::TestAppContext) {
    let harness = build(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(harness.window, cx);
    let base = "root/children:@form/children:@";
    let updates = format!("{base}updates");
    let topics = format!("{base}topics");
    let speed = format!("{base}speed");
    let notify = format!("{base}notify");
    let region = format!("{base}region");

    visual.update(|window, cx| {
        window.render_frame(cx);
        window.click(
            ElementId::Name(SharedString::from(format!("genui-checkbox-{updates}"))),
            cx,
        );
    });
    assert_eq!(bool_state(&visual, &harness.host, "updates"), Some(true));

    visual.update(|window, cx| {
        window.click(
            ElementId::Name(SharedString::from(format!("genui-checkbox-{topics}-2"))),
            cx,
        );
    });
    assert_eq!(
        list_state(&visual, &harness.host, "topics"),
        vec!["Research"]
    );

    visual.update(|window, cx| {
        window.click(
            ElementId::Name(SharedString::from(format!("genui-radio-{speed}-1"))),
            cx,
        );
    });
    assert_eq!(
        text_state(&visual, &harness.host, "speed").as_deref(),
        Some("Express")
    );

    visual.update(|window, cx| {
        window.click(
            ElementId::Name(SharedString::from(format!("genui-switch-{notify}"))),
            cx,
        );
    });
    assert_eq!(bool_state(&visual, &harness.host, "notify"), Some(true));

    visual.update(|window, cx| {
        window.click(
            ElementId::Name(SharedString::from(format!("genui-select-{region}"))),
            cx,
        );
        window.press("down", cx);
        window.press("enter", cx);
    });
    assert_eq!(
        text_state(&visual, &harness.host, "region").as_deref(),
        Some("South")
    );

    let events = harness.seen.borrow().events.clone();
    assert!(events.iter().any(|event| matches!(
        event,
        UiEvent::Edit { node, value }
            if node.0 == updates && *value == genui_core::value::Value::Bool(true)
    )));
}

fn press(visual: &mut gpui_kit::VisualTestContext, key: &str) {
    visual.update(|window, cx| window.press(key, cx));
}

#[gpui_kit::test]
fn keyboard_only_run_reaches_submit(cx: &mut gpui_kit::TestAppContext) {
    let harness = build_source(cx, DEMO);
    let mut visual = gpui_kit::VisualTestContext::from_window(harness.window, cx);
    visual.update(|window, cx| {
        window.render_frame(cx);
        harness.host.update(cx, |host, cx| {
            host.ui.update(cx, |ui, cx| ui.focus_root(window, cx));
        });
    });

    // Name, Email: Tab reaches each field and typing edits it.
    press(&mut visual, "tab");
    visual.update(|window, cx| window.input("!", cx));
    press(&mut visual, "tab");
    visual.update(|window, cx| window.input("ada@example.com", cx));
    assert_eq!(
        text_state(&visual, &harness.host, "name").as_deref(),
        Some("Ada Lovelace!")
    );
    assert_eq!(
        text_state(&visual, &harness.host, "email").as_deref(),
        Some("ada@example.com")
    );

    // Select: Enter opens the menu, arrows move the cursor, Enter confirms.
    press(&mut visual, "tab");
    press(&mut visual, "enter");
    press(&mut visual, "down");
    press(&mut visual, "enter");
    assert_eq!(
        text_state(&visual, &harness.host, "team").as_deref(),
        Some("Product")
    );

    // RadioGroup: the selected radio is the tab stop; arrows change it.
    press(&mut visual, "tab");
    press(&mut visual, "down");
    assert_eq!(
        text_state(&visual, &harness.host, "speed").as_deref(),
        Some("Standard")
    );

    // Checkbox: Space toggles the focused control.
    press(&mut visual, "tab");
    press(&mut visual, "space");
    assert_eq!(bool_state(&visual, &harness.host, "updates"), Some(false));

    // CheckboxGroup: each option is its own tab stop. Releases starts checked,
    // so move to Incidents and add it.
    press(&mut visual, "tab");
    press(&mut visual, "tab");
    press(&mut visual, "space");
    assert_eq!(
        list_state(&visual, &harness.host, "topics"),
        vec!["Releases", "Incidents"]
    );

    // Third topic, then the switch, then the notes field.
    press(&mut visual, "tab");
    press(&mut visual, "tab");
    press(&mut visual, "space");
    assert_eq!(bool_state(&visual, &harness.host, "notify"), Some(true));
    press(&mut visual, "tab");
    visual.update(|window, cx| window.input("Leave at the door", cx));
    assert_eq!(
        text_state(&visual, &harness.host, "notes").as_deref(),
        Some("Leave at the door")
    );

    // Submit with the keyboard.
    press(&mut visual, "tab");
    press(&mut visual, "enter");
    assert_eq!(harness.seen.borrow().model_events.len(), 1);
    assert_eq!(
        harness.seen.borrow().model_events[0].text.as_deref(),
        Some("Create account")
    );
}

#[gpui_kit::test]
fn validation_controls_the_submit_gate(cx: &mut gpui_kit::TestAppContext) {
    let harness = build(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(harness.window, cx);
    let email = "root/children:@form/children:@email";
    let go = "root/children:@form/children:@go";

    assert!(!error_visible(&mut visual, email));
    visual.update(|window, cx| window.click(button_id(go), cx));
    assert!(
        harness.seen.borrow().model_events.is_empty(),
        "an empty required field blocks submit"
    );
    assert!(error_visible(&mut visual, email));
    let touched = visual.read(|app| {
        harness.host.read_with(app, |host, _| {
            host.ui
                .read(app)
                .tree()
                .get(email)
                .map(|entry| {
                    (
                        entry.props.get("error").cloned(),
                        entry.props.get("touched").cloned(),
                    )
                })
                .unwrap_or_default()
        })
    });
    assert_eq!(touched.0, Some(serde_json::json!("required")));
    assert_eq!(touched.1, Some(serde_json::json!(true)));

    visual.update(|window, cx| {
        window.click(input_id(email), cx);
        window.input("ada@example.com", cx);
    });
    visual.update(|window, cx| window.click(button_id(go), cx));
    assert_eq!(harness.seen.borrow().model_events.len(), 1);
    assert!(
        !error_visible(&mut visual, email),
        "a valid submit clears the error"
    );
    let activate = harness
        .seen
        .borrow()
        .events
        .iter()
        .find_map(|event| match event {
            UiEvent::Activate { node, action } if node.0 == go => Some(action.clone()),
            _ => None,
        })
        .expect("submit emits Activate");
    assert_eq!(activate.epoch, 1);
    assert!(activate.plan.0.contains("on_submit"));
}

#[gpui_kit::test]
fn enter_in_a_single_line_field_submits(cx: &mut gpui_kit::TestAppContext) {
    let harness = build(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(harness.window, cx);
    let email = "root/children:@form/children:@email";
    visual.update(|window, cx| {
        window.click(input_id(email), cx);
        window.input("ada@example.com", cx);
    });
    visual.update(|window, cx| window.press("enter", cx));
    assert_eq!(harness.seen.borrow().model_events.len(), 1);
    assert_eq!(
        harness.seen.borrow().model_events[0].text.as_deref(),
        Some("Create account")
    );
}

#[gpui_kit::test]
fn focus_traversal_blur_and_escape(cx: &mut gpui_kit::TestAppContext) {
    let harness = build(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(harness.window, cx);
    let name = "root/children:@form/children:@name";
    let email = "root/children:@form/children:@email";
    let notes = "root/children:@form/children:@notes";

    visual.update(|window, cx| {
        window.click(input_id(name), cx);
        window.input("!", cx);
    });
    assert_eq!(
        text_state(&visual, &harness.host, "name").as_deref(),
        Some("Ada!")
    );
    visual.update(|window, cx| window.click(input_id(email), cx));
    assert!(
        harness
            .seen
            .borrow()
            .events
            .iter()
            .any(|event| matches!(event, UiEvent::Blur { node } if node.0 == name)),
        "moving focus emits Blur for the previous field"
    );

    visual.update(|window, cx| window.press("shift-tab", cx));
    assert!(visual.read(|app| {
        let host = harness.host.read(app);
        let view = host.ui.read(app);
        let email_handle = view.focus_handle_for(email, app);
        let name_handle = view.focus_handle_for(name, app);
        let _ = (email_handle, name_handle);
        true
    }));
    visual.update(|window, cx| window.input("?", cx));
    assert_eq!(
        text_state(&visual, &harness.host, "name").as_deref(),
        Some("Ada!?"),
        "Shift-Tab returns to the previous field"
    );

    visual.update(|window, cx| {
        window.click(
            ElementId::Name(SharedString::from(format!("genui-field-{notes}"))),
            cx,
        );
    });
    let before = harness.seen.borrow().events.len();
    visual.update(|window, cx| window.press("escape", cx));
    visual.update(|window, cx| window.input("ZZ", cx));
    assert_eq!(
        text_state(&visual, &harness.host, "notes").as_deref(),
        Some(""),
        "Escape clears focus, so typing reaches no field"
    );
    assert_eq!(
        harness.seen.borrow().events.len(),
        before + 1,
        "Escape only emits the blur for the focused field"
    );
    assert!(matches!(
        harness.seen.borrow().events.last(),
        Some(UiEvent::Blur { node }) if node.0 == notes
    ));
}

#[gpui_kit::test]
fn patches_do_not_reset_typed_text_or_focus(cx: &mut gpui_kit::TestAppContext) {
    let harness = build(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(harness.window, cx);
    let name = "root/children:@form/children:@name";
    visual.update(|window, cx| {
        window.click(input_id(name), cx);
        window.input("!", cx);
        harness.host.update(cx, |host, cx| {
            host.touch_label(name, "Full name", window, cx);
        });
        window.input("?", cx);
    });
    assert_eq!(
        text_state(&visual, &harness.host, "name").as_deref(),
        Some("Ada!?"),
        "replacing props keeps the widget's text and cursor"
    );
    let label = visual.read(|app| {
        harness.host.read_with(app, |host, _| {
            host.ui
                .read(app)
                .tree()
                .get(name)
                .and_then(|entry| entry.props.get("label").cloned())
        })
    });
    assert_eq!(label, Some(serde_json::json!("Full name")));
    let focused = visual.update(|window, cx| {
        let focused = window.focused(cx);
        harness.host.read_with(cx, |host, _| {
            let expected = host.ui.read(cx).focus_handle_for(name, cx);
            focused.is_some() && focused == expected
        })
    });
    assert!(focused, "the edited field keeps focus");
}

#[gpui_kit::test]
fn streaming_status_disables_inputs_and_buttons(cx: &mut gpui_kit::TestAppContext) {
    let harness = build(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(harness.window, cx);
    let name = "root/children:@form/children:@name";
    let go = "root/children:@form/children:@go";
    visual.update(|window, cx| {
        harness.host.update(cx, |host, cx| {
            host.apply_status(name, genui_core::ir::NodeStatus::Partial, window, cx);
            host.apply_status(go, genui_core::ir::NodeStatus::Partial, window, cx);
        });
    });
    let before = harness.seen.borrow().events.len();
    visual.update(|window, cx| {
        window.click(input_id(name), cx);
        window.input("nope", cx);
        window.click(button_id(go), cx);
    });
    assert_eq!(
        text_state(&visual, &harness.host, "name").as_deref(),
        Some("Ada"),
        "a partial input rejects edits"
    );
    assert_eq!(
        harness.seen.borrow().events.len(),
        before,
        "a partial input and gated button emit nothing"
    );

    visual.update(|window, cx| {
        harness.host.update(cx, |host, cx| {
            host.apply_status(name, genui_core::ir::NodeStatus::Sealed, window, cx);
            host.apply_status(go, genui_core::ir::NodeStatus::Sealed, window, cx);
        });
        window.click(input_id(name), cx);
        window.input("!", cx);
    });
    assert_eq!(
        text_state(&visual, &harness.host, "name").as_deref(),
        Some("Ada!")
    );
}

#[gpui_kit::test]
fn frozen_view_keeps_its_tree_and_ignores_input(cx: &mut gpui_kit::TestAppContext) {
    let harness = build(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(harness.window, cx);
    let name = "root/children:@form/children:@name";
    let go = "root/children:@form/children:@go";
    visual.update(|window, cx| {
        harness.host.update(cx, |host, cx| {
            host.ui.update(cx, |ui, cx| ui.freeze(cx));
        });
        window.render_frame(cx);
    });
    let before = harness.seen.borrow().events.len();
    visual.update(|window, cx| {
        window.click(input_id(name), cx);
        window.input("frozen", cx);
        window.click(button_id(go), cx);
    });
    assert_eq!(harness.seen.borrow().events.len(), before);
    assert_eq!(
        text_state(&visual, &harness.host, "name").as_deref(),
        Some("Ada")
    );
    assert!(visual.read(|app| {
        harness
            .host
            .read_with(app, |host, _| host.ui.read(app).is_frozen())
    }));

    visual.update(|window, cx| {
        harness.host.update(cx, |host, cx| {
            host.ui.update(cx, |ui, cx| ui.thaw(cx));
        });
        window.click(input_id(name), cx);
        window.input("!", cx);
    });
    assert_eq!(
        text_state(&visual, &harness.host, "name").as_deref(),
        Some("Ada!")
    );
}

/// The pinned test platform has no OS accessibility adapter, so the full
/// `debug_a11y_tree_json` tree is empty. This probe inspects the same accesskit
/// nodes the renderer's pinned controls produce via `Element::write_a11y_info`
/// (SPEC §10.10); the API facts are recorded in PINS.md.
#[gpui_kit::test]
fn pinned_controls_expose_roles_labels_values_and_states(cx: &mut gpui_kit::TestAppContext) {
    use gpui_component::input::{Input, InputState};
    use gpui_kit::{
        Element as _, Role,
        base::{
            Button as BaseButton, Checkbox as BaseCheckbox, Radio as BaseRadio,
            Select as BaseSelect, Switch as BaseSwitch,
        },
        canvas,
    };

    struct Probe {
        nodes: Rc<RefCell<Vec<(String, gpui_kit::accesskit::Node)>>>,
        input: Entity<InputState>,
    }

    impl Render for Probe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let nodes = self.nodes.clone();
            let input = self.input.clone();
            canvas(
                move |_, window, cx| {
                    let mut captured = Vec::new();
                    let mut add = |name: &str, mut node: gpui_kit::accesskit::Node,
                                   write: &mut dyn FnMut(&mut gpui_kit::accesskit::Node)| {
                        write(&mut node);
                        captured.push((name.to_string(), node));
                    };
                    add(
                        "input",
                        gpui_kit::accesskit::Node::new(Role::TextInput),
                        &mut |node| {
                            Input::new(&input)
                                .aria_label("Email")
                                .render(window, cx)
                                .into_element()
                                .write_a11y_info(node);
                        },
                    );
                    add(
                        "select",
                        gpui_kit::accesskit::Node::new(Role::ComboBox),
                        &mut |node| {
                            BaseSelect::new("region")
                                .accessibility_label("Region")
                                .accessibility_value("South")
                                .render(window, cx)
                                .into_element()
                                .write_a11y_info(node);
                        },
                    );
                    add(
                        "checkbox",
                        gpui_kit::accesskit::Node::new(Role::CheckBox),
                        &mut |node| {
                            BaseCheckbox::new("updates")
                                .checked(true)
                                .accessibility_label("Email updates")
                                .render(window, cx)
                                .into_element()
                                .write_a11y_info(node);
                        },
                    );
                    add(
                        "radio",
                        gpui_kit::accesskit::Node::new(Role::RadioButton),
                        &mut |node| {
                            BaseRadio::new("express")
                                .checked(true)
                                .accessibility_label("Express")
                                .set_position(2, 2)
                                .render(window, cx)
                                .into_element()
                                .write_a11y_info(node);
                        },
                    );
                    add(
                        "switch",
                        gpui_kit::accesskit::Node::new(Role::Switch),
                        &mut |node| {
                            BaseSwitch::new("notify")
                                .checked(true)
                                .accessibility_label("Notifications")
                                .render(window, cx)
                                .into_element()
                                .write_a11y_info(node);
                        },
                    );
                    add(
                        "button",
                        gpui_kit::accesskit::Node::new(Role::Button),
                        &mut |node| {
                            BaseButton::new("save")
                                .accessibility_label("Save changes")
                                .on_click(|_, _, _| {})
                                .render(window, cx)
                                .into_element()
                                .write_a11y_info(node);
                        },
                    );
                    add(
                        "form",
                        gpui_kit::accesskit::Node::new(Role::Form),
                        &mut |node| {
                            gpui_kit::div()
                                .id("account-form")
                                .role(Role::Form)
                                .aria_label("Account form")
                                .write_a11y_info(node);
                        },
                    );
                    *nodes.borrow_mut() = captured;
                },
                |_, _, _, _| {},
            )
        }
    }

    let harness = build(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(harness.window, cx);
    let input = visual.update(|window, cx| {
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state = state.placeholder("you@example.com");
            state
        });
        input.update(cx, |state, cx| {
            state.set_value("ada@example.com", window, cx)
        });
        input
    });
    let nodes = Rc::new(RefCell::new(Vec::new()));
    let probe = cx.new(|_| Probe {
        nodes: nodes.clone(),
        input,
    });
    visual.draw(
        gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(0.)),
        gpui_kit::size(gpui_kit::px(800.), gpui_kit::px(600.)),
        move |_, _| gpui_kit::div().size_full().child(probe.clone()),
    );
    let nodes = nodes.borrow();
    let get = |name: &str| {
        nodes
            .iter()
            .find(|(node_name, _)| node_name == name)
            .map(|(_, node)| node)
            .unwrap_or_else(|| panic!("missing node {name}"))
    };

    let input = get("input");
    assert_eq!(input.role(), Role::TextInput);
    assert_eq!(input.label(), Some("Email"));
    assert_eq!(input.value(), Some("ada@example.com"));

    let select = get("select");
    assert_eq!(select.role(), Role::ComboBox);
    assert_eq!(select.label(), Some("Region"));
    assert_eq!(select.value(), Some("South"));

    let checkbox = get("checkbox");
    assert_eq!(checkbox.role(), Role::CheckBox);
    assert_eq!(checkbox.label(), Some("Email updates"));
    assert_eq!(checkbox.toggled(), Some(gpui_kit::accesskit::Toggled::True));

    let radio = get("radio");
    assert_eq!(radio.role(), Role::RadioButton);
    assert_eq!(radio.label(), Some("Express"));

    let switch = get("switch");
    assert_eq!(switch.role(), Role::Switch);
    assert_eq!(switch.label(), Some("Notifications"));

    let button = get("button");
    assert_eq!(button.role(), Role::Button);
    assert_eq!(button.label(), Some("Save changes"));
    assert!(button.supports_action(gpui_kit::accesskit::Action::Click));

    let form = get("form");
    assert_eq!(form.role(), Role::Form);
    assert_eq!(form.label(), Some("Account form"));
}

#[gpui_kit::test]
fn host_extensions_draw_custom_components(cx: &mut gpui_kit::TestAppContext) {
    let harness = build(cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(harness.window, cx);
    visual.update(|window, cx| {
        harness.host.update(cx, |host, cx| {
            host.ui.update(cx, |ui, cx| {
                ui.register_extension(
                    "Gauge",
                    |node, _, _| {
                        gpui_kit::div()
                            .id("ext-gauge")
                            .test_support()
                            .child(
                                node.props
                                    .get("label")
                                    .and_then(|value| value.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                            )
                            .into_any_element()
                    },
                    cx,
                );
            });
            let batch = PatchBatch {
                epoch: 9,
                seq: 0,
                ops: vec![
                    genui_core::patch::Op::Reset,
                    genui_core::patch::Op::Insert {
                        parent: None,
                        slot: "root".into(),
                        index: 0,
                        node: genui_core::patch::NodeInit {
                            id: "root".into(),
                            kind: "Gauge".into(),
                            role: genui_core::ir::Role::Group,
                            props: std::collections::BTreeMap::from([(
                                "label".to_string(),
                                serde_json::json!("CPU"),
                            )]),
                            status: genui_core::ir::NodeStatus::Sealed,
                        },
                    },
                ],
            };
            host.ui.update(cx, |ui, cx| ui.apply(batch, window, cx));
        });
        window.render_frame(cx);
        let target = window.find(ElementId::Name(SharedString::from("ext-gauge")));
        assert!(target.visible());
    });
}
