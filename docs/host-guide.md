# Host integration guide

This guide is the M8 host contract (ROADMAP item 8, SPEC §§7.5–7.9, 10, 12.7).
The reference implementation is `genui-gpui::host` plus the `sample-host` binary;
the renderer itself stays I/O-free.

## 1. Build the library and a session

```rust
let library = Arc::new(genui_std::std_library()?);
let options = genui_core::validate::Options {
    tools,            // allowlisted read-only tools
    refs,             // per-turn ref catalog
    url_hosts,        // host allowlist for model URLs and images
    capabilities: ["charts", "images", "markdown"].into(),
    ..Default::default()
};
let mut session = Session::new(SessionOptions {
    library,
    validation: options,
    input: InputMode::Message,      // model replies; Program for files
    prompt_digest: prompt.digest.clone(),
});
```

Generate the prompt once with `genui_prompt::generate(library.spec(), &options)`
and store the digest on the session and in the stored message. `UiView` receives
the same capabilities through `UiView::capabilities()`.

## 2. Render the stream

```rust
let ui = cx.new(|cx| UiView::new(theme, window, cx));
for chunk in model_chunks {
    let update = session.push(chunk);
    if let Some(batch) = update.patches {
        // Always apply before the frame, with Window available.
        ui.update(cx, |view, cx| view.apply(batch, window, cx));
    }
    for request in update.requests { host.respond(&mut session, request); }
}
let finish = session.finish();
```

`UiView::enqueue` exists for callers without a window; it drains once at the
next `render` and may show one frame later. Do not use it on the streaming path.

## 3. Requests and approval policy

`Update.requests` are host effects (SPEC §7.5). A host must decide each one:

- `ResolveRef`: answer from the per-turn ref catalog only; never invent data.
- `Query`: run the registered read-only tool, or deny. `needs_approval` means the
  host must ask unless policy already approves the tool.
- `Say` / `Emit` / `OpenUrl`: record the model event, gate the URL by host
  allowlist, and open nothing without user intent.

`genui-gpui::host::SampleHost` is a closed-by-default reference policy. It
delivers allowlisted sample refs/tools, records `say`/`emit`, and denies `open`
unless explicitly enabled. Unknown requests are denied and logged; a delivery
uses `Session::deliver`, which rejects stale ids and cross-epoch results.

## 4. Events

`UiView` emits the core path-addressed `UiEvent` directly:

```rust
cx.subscribe(&ui, |host, _, event: &UiEvent, cx| {
    // The session resolves the stable patch path to its live node id.
    let update = session.event(event.clone());
    // Apply update.patches to the same view, then sync bound widgets.
    view.update(cx, |view, cx| {
        view.apply(update.patches.unwrap_or_default(), window, cx);
        view.sync_state(session.state_values(), window, cx);
    });
});
```

Buttons emit `Activate` when sealed, enabled and actionable; inputs and a bound
Tabs emit `Edit`; focus changes emit `Blur`. `Session::activate_at`, `edit_at`
and `blur_at` remain as convenience entry points for hosts that already hold a
path string.

## 5. Images

The renderer never fetches. Check `host.image_allowed(src)` (https + allowlist),
then:

```rust
ui.update(cx, |view, cx| view.request_image(src, cx));
match fetch(src).await {
    Ok(bytes) => { view.deliver_image(src, format, bytes, cx)?; }
    Err(_) => view.fail_image(src, cx),
}
```

The renderer caps 8 MiB, 4096 px per side, 16 MP, 256 sources per message and
catches decode panics. The host owns the 10 s deadline and the real decode
validation.

## 6. Freeze, restore, regenerate

- `Session::freeze(self) -> FrozenMessage { snapshot, state, fallback_text }`
  for read-only history.
- `Session::restore(&stored, SessionOptions)` re-parses the program and applies
  the state snapshot. Refs are host data: call `preload_ref` (or deliver the
  re-issued `ResolveRef` requests) after restore.
- `Session::reset()` starts a regenerate epoch; old action handles are inert and
  stale batches are discarded.
- `state_snapshot()` round-trips declared state and any undeclared snapshot
  values the host carried.

## 7. Transcripts

`eval/transcripts/*.json` are deterministic scripted conversations. Use
`genui_prompt::transcript::LiveRunner` for a chunk-by-chunk host, or
`genui-cli transcripts [dir]` to verify exact patch streams, state and events.
Every file is labelled `scripted-fixture`; never present one as a real model run.

## 8. Commands

```sh
cargo run -p genui-gpui --features screenshot --bin sample-host -- --list
cargo run -p genui-gpui --features screenshot --bin sample-host -- \
  --transcript eval/transcripts/13-sales-tabs.json --headless \
  --screenshot /tmp/host --record /tmp/host/run.json --restore --regenerate
cargo run -p genui-cli -- transcripts
cargo run -p genui-gpui --features screenshot --bin table-stress -- --rows 50000
```

## 9. Checklist

- Apply batches before the frame; retain no model I/O inside the renderer.
- Sanitize/truncate every text prop (the renderer stores 32 KiB; table cells
  4 KiB) and keep URL/image allowlists closed.
- Commit `Cargo.lock`, keep the exact pins, and build with `panic = "unwind"` if
  you want `render.panic` containment.
- Surface `render.panic` diagnostics and request a snapshot on a sequence gap.
- Keep secrets out of prompts, host requests and transcripts; phase 1 has no
  secret fields.
