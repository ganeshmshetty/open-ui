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
