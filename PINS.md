# GPUI dependency pins

Recorded 2026-10-21 from SPEC §10.9 (the spec records verification on this date).

| Package | Exact version | Reference |
| --- | --- | --- |
| gpui-kit | =0.8.0 | https://docs.rs/gpui-kit/0.8.0/ |
| gpui-component | =0.8.0 | https://docs.rs/gpui-component/0.8.0/ |
| gpui-base | =0.8.0 | https://docs.rs/gpui-base/0.8.0/ |
| gpui-pre | =0.3.9 | https://docs.rs/gpui-pre/0.3.9/ |

M0 reserves these exact workspace dependency declarations. The `genui-gpui`
crate is an empty scaffold: it does not yet activate GPUI dependencies, and
`Cargo.lock` only contains the active M0 dependency graph. M6 (or the separately
authorized spike) must activate the entire set together, record compilation
verification, and update the lockfile. No rendering or pin compilation is
claimed by M0. Pin changes require a dedicated change per SPEC §10.9.

## M6 activation — 2026-10-24

The four pins are active in `genui-gpui` and `Cargo.lock`; the renderer uses
GPUI only through `gpui-kit`'s re-exports. Verified on arm64 macOS 26.7 with
Rust 1.99.0 and Xcode:

- default features and `--features screenshot` (kit `test-support`) build;
- one `gpui-pre 0.3.9` is shared by the four pins;
- the UI-free workspace still passes under default features, including
  `serde_json/preserve_order` feature unification.

| Surface checked (pinned docs/source) | API / difference recorded |
| --- | --- |
| gpui-kit 0.8.0 `init`, `open_window`, `application` | `init` calls `gpui_component::init`; `open_window` returns `(AnyWindowHandle, Entity<V>)` and installs Root. The headless test context opens `WindowHandle<V>` instead. |
| gpui-pre 0.3.9 `Window::render_to_image` | Available under `test-support` but hidden from default rustdoc. `HeadlessAppContext::with_platform(..., current_headless_renderer)` captured real text offscreen without a visible window. Window resize after creation did not re-render the old scene in a headless window; M6 opens one correctly sized window per capture. |
| gpui-component 0.8.0 `Tag` | `Tag::custom(color, foreground, border)` takes the **background** first. M6 initially passed `(fg, bg, border)` and fixed it before the screenshot review. |
| gpui-component 0.8.0 `TextViewStyle` | `is_dark` is a public field, not a builder method. `inline_code` is a `HighlightStyle`; the pinned TextView still paints inline code with the component accent tint in M6 screenshots (logged finding). |
| gpui-component 0.8.0 `Alert` | `Styled` overrides apply after the variant colors, so M6 sets `tone.bg`/`tone.fg`/`tone.border` explicitly. |
| gpui-component 0.8.0 `chart::BarChart` / `chart::LineChart` | Usable, but the bar value ticks were 0/41.33/82.67/124 for a 124 maximum (not DESIGN §6.11's round intervals), and the line chart has no null-gap representation. M6 draws both plots with `gpui::canvas` instead. |
| gpui-pre 0.3.9 `AnyView::cached` | Takes a `StyleRefinement` and needs definite bounds; M6 caches only chart plots and images with parent-assigned width and theme height. |
| gpui-pre 0.3.9 `canvas`, `PathBuilder`, `fill`, `Window::paint_quad/paint_path` | Used for charts. `fill(bounds, color).corner_radii(Corners)` paints bars and singleton line points. |
| gpui-base 0.8.0 `FontFeatures` | Public tuple field; M6 enables `tnum` for tabular numbers. |

The upstream `block 0.1.6` future-incompatibility advisory remains; builds and
Clippy pass without suppression. No pin changed.

## M8 data/host verification — 2026-10-24

The pins are unchanged. New surfaces checked against the pinned sources:

| Surface | API / difference recorded |
| --- | --- |
| gpui-component 0.8.0 `table::DataTable` | `DataTable::new(&Entity<TableState<D>>)` owns sizing, borders, header height and scrollbar visibility. `sortable`, `row_selectable`, `col_resizable`, `col_movable` and `row_header` live on `TableState`, not on `DataTable`; a new state needs `&mut Window`. `TableState::refresh` is required after delegate column/row changes, and `scroll_to_row` only needs the state context. |
| gpui-component 0.8.0 `TableDelegate` | `render_td`/`render_th`/`render_empty`/`render_loading` return `impl IntoElement`; the delegate receives `&mut Context<TableState<Self>>`, so model updates must go through `delegate_mut()` and `refresh`. `loading()` receives `&App`. |
| gpui-component 0.8.0 `Column` | `sortable()`, `text_right()`, `width/min_width/max_width`, `paddings(Edges)` and `resizable(bool)` are the supported builder surface; `Edges::symmetric` does not exist in the pinned geometry API. |
| gpui-pre 0.3.9 animation | `AnimationExt::with_animation` automatically renders the static end state for a oneshot animation and the static **start** state for a repeating animation when `App::reduce_motion` is set. Repeating skeletons therefore start at full opacity, not at their low point. `App::set_reduce_motion(true)` is public and is what the screenshot hosts use. |
| gpui-pre 0.3.9 headless capture | `HeadlessAppContext` and `Window::render_to_image` must be created on the process main thread; integration tests invoke the gallery self-tests in a child process. `WindowHandle<T>` is `Copy`; `AnyWindowHandle::from(window)` does not need a clone. |
| gpui-pre 0.3.9 `on_mouse_move` | The event position is window-relative. Chart hover stores the canvas paint bounds in a `Cell<Bounds<Pixels>>` and subtracts the origin; there is no local-coordinate variant in the fluent listener. |
| gpui-component 0.8.0 `TextTooltip`/tooltip | `.tooltip(builder)` is on `StatefulInteractiveElement`; the builder receives `&mut App` (not a `Context`), so the tooltip entity is created with `AppContext::new`. A cell needs an `id` before attaching a tooltip. |
| `genui-core` M8 additions | `Session::preload_ref`, `state_snapshot`, `freeze`, `node_id_at`, `action_handle_at`, `activate_at`, `edit_at`, `blur_at` are core APIs and are covered by core tests; the renderer uses only the path-addressed methods. |

## M8 List virtualization — 2026-10-24

| Surface | API / difference recorded |
| --- | --- |
| gpui-component 0.8.0 `list::List` / `ListState` | `ListState::new(delegate, window, cx)` creates a search `InputState` and subscription even when `searchable(false)`. `List::new(&state)` is `RenderOnce`; the viewport cap is read from the element's `max_size.height` (`.max_h`), not the parent wrapper. `ListState::selectable(false)` disables row selection. `scroll_to_item(IndexPath, ScrollStrategy::Top, window, cx)` is public; `IndexPath` is re-exported at `gpui_kit::component::IndexPath`. |
| gpui-component 0.8.0 `ListDelegate` | The list is a `v_virtual_list`; `render_item` is called only for visible rows. Every item is measured once and the same size is used for the rest (`MeasuredEntrySize`), so phase-1 List rows are assumed uniform. `ListItem` is the ready-made `Selectable + IntoElement` row; `.p(space(0))` and `.pb(gap)` override its default padding. There is no public visible-range getter, so the stress counts `render_item` calls per frame and created item entities. |

## M7 input/form/a11y audit — 2026-10-24

No pin changed. Checked the same pinned source against the M7 surfaces:

| Surface checked (pinned docs/source) | API / difference recorded |
| --- | --- |
| `gpui-component 0.8.0` `input::{InputState, TextareaState, InputEvent}` | `InputEvent::{Change, PressEnter, Focus, Blur}`; `set_value` does not emit `Change`. The focus events are tied to the state handle, while a pointer press can focus the BaseInput frame; M7 therefore names pointer focus itself and emits `Blur` from the handle it leaves. |
| `gpui-component 0.8.0` `input::{Input, Textarea}` | `Input` and `Textarea` expose `accessibility_label`/`aria_label`; `Textarea` has no builder `id`, so the demo/tests target the DESIGN field wrapper instead. `appearance(false)` leaves text color unset, so M7 sets `text_color` explicitly for dark mode. |
| `gpui-component 0.8.0` `select::{Select, SelectState}` | `SelectState::new(SearchableVec<_>, Option<IndexPath>, window, cx)`; Enter/down/Enter drives the list in the test context. The component trigger draws no caret in headless captures (the icon asset is not guaranteed), so M7 draws a theme-token chevron and makes the kit icon transparent. |
| `gpui-component 0.8.0` `checkbox`, `radio`, `switch` | Component `Switch` is 36×20; DESIGN requires 32×18. Component `Checkbox` fills with the theme `primary` (black) although DESIGN requires `accent` (blue). M7 uses `gpui-base` primitives with DESIGN geometry instead. |
| `gpui-base 0.8.0` `RadioGroup` | Sets the `RadioGroup` role and orientation but binds no arrow keys. M7 handles Up/Down/Left/Right on the group wrapper and moves focus with the roster handles. |
| `gpui-base 0.8.0` `button::Button`, `checkbox::Checkbox`, `switch::Switch` | `on_click`/`on_change` report Enter and Space as keyboard activation. `track_focus` accepts a caller-owned handle, which M7 uses to emit `Blur` and draw rings. |
| `gpui_base` Root tab traversal | Root binds Tab/TabPrev and consumes them before `UiView` can emit `Blur`. M7 unbinds `root::Tab`/`root::TabPrev` plus the input `IndentInline`/`OutdentInline` pair, then calls `Window::focus_next`/`focus_prev` from the view key handler. The view is a `tab_group`; `focus_root` starts traversal. |
| `gpui-pre 0.3.9` accessibility | `Role`, `aria_label`, `aria_value`, `aria_toggled`, `on_a11y_action`, `Element::write_a11y_info` and accesskit node getters exist. The test platform installs no accessibility adapter: `Window::debug_a11y_tree_json()` is empty in tests and `TestAppContext::disable_accessibility` is the only control. `write_a11y_info` must run during layout/prepaint/paint, so the test probe draws a canvas. Component `Select`/`Button` `RenderOnce` results do not forward their a11y info to `AnyElement`; the probe verifies the base primitives they use. |
| `gpui-pre 0.3.9` test context | `TestWindowExt` clicks by `ElementId` and requires `.test_support()` on plain elements. Frame effects from a key press are processed on the following `render_frame`, so the scripted tests drive one key per update. |

The upstream `block 0.1.6` future-incompatibility advisory remains; builds and
Clippy pass without suppression.
