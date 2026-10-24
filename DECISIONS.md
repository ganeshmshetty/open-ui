# Owner decisions

No new owner decision is required for the M2 inspection and quality-gate follow-up.
Defaults and their rationale are recorded below; nothing here authorizes a scope
change or an irreversible action.

| Question | Options | Recommended default | Work meanwhile | Status |
| --- | --- | --- | --- | --- |
| What host catalog should `tree` use? | Empty default policy; silently inherit exemplar tools/refs; add host options | Empty policy, matching the core API; exemplar host context remains explicit in spec-check | Implemented and documented in SPEC §12.7 | Default adopted; owner may revisit |

Project naming, license and publishing remain ROADMAP item 10, due before first
publish. No publishing decision was made in this follow-up.

## Approved GPUI spike decisions — resolved by main 6bca7fa

- Resolved: §10.1 applies patches before the frame with Window available; enqueue
  is fallback only. Containers are uncached; fixed-size expensive subtrees may cache.
- Resolved: §12.7 uses windowed test-support render_to_image for gallery capture;
  headless frame driving is separate.
- Resolved: §6.5 requires a core-owned canonical encoder and preserve_order CI.
  Implementing that approved decision in M3; no golden is changed to conceal it.

## M3 defaults recorded while implementing the contract

| Question | Options | Recommended default / action | Status |
| --- | --- | --- | --- |
| How do M3 patches carry Dynamic props before M4? | Evaluate early; leak arena indices; carry structural deferred expressions | Explicit deferred staging encoding; no evaluator, stable across arena reindexing; G9 applies from M4 | Documented in SPEC §§5.5,6.3 |
| How can canonical map keys be sorted while map entry order survives wire round trips? | Lose entry order; exempt objects from sorting; tagged ordered pairs | Tagged ordered pairs for semantic maps only; metadata remains sorted objects; no tag collisions | Documented in SPEC §6.5 |
| How does a host retract provisional diagnostics? | Append-only stale diagnostics; new removal protocol; authoritative current list | Update carries additions; diagnostics() supplies the current list for provisional replacement | Documented in SPEC §3.7 |

## M3 I7 implementation decisions

- Owner reconfirmed I7 must be completed before merge; no deferral is authorized.
- Adopt persistent maps with individually shared payloads and persistent slot
  sequences. Node/AST payload clone counters and logical maintenance counters
  prove retained data is not silently copied; core remains synchronous and UI-free.
- Separate authoritative diagnostic materialization (an observer operation) from
  push/finish publication. Code/flag indexes permit exact additions, withdrawals
  and provisional overlays without traversing retained history.
- Live arena identity must be independent of dense snapshot indices. Keep the
  deterministic snapshot contract where possible; resolve any required contract
  clarification with SPEC, fixtures and CHANGELOG together. Adopt structural live
  expression keys including recovery metadata, canonical dense observer indices,
  and escaped source-owner action keys. M4 still owns epoch-bound runtime handles.

## Remaining M3 I7 architecture (implementation plan, not acceptance)

- General canonical allocation needs retained instantiation tasks, including failed
  and budget-omitted tasks. Each slot range needs a cached resource profile:
  allocation used, valid remaining-budget interval and direct-slot stopping.
  Dropped parents roll back usage while retaining the constraints checked inside.
- Keep semantic task results separate from visible IR projection. Reusing a hidden
  failed subtree must not copy its nodes; a subtree becoming visible may visit the
  newly emitted nodes. Changed task roots drive projection, diagnostics and patches.
- Invalidate actual readers/source positions and bubble aggregate changes through
  ancestors. Do not label the entire program dirty to excuse the full fallback.
- Replace global kind/reachability scans with retained source metadata and dirty
  queues. Null/empty state inference needs canonical syntactic binding evidence,
  including deferred bodies and budget-omitted instances; visible nodes alone are
  insufficient. Preserve per-path cycles, forms and local-variable environments.
- Current local helpers are intermediate implementation, not an I7 waiver. The
  fallback remains functional and batch-equivalent but blocks M3 acceptance. No
  owner scope decision or permission to defer I7 is being requested.

Implementation checkpoint: the shared resource-profile/range kernel is now in
production for state/query allocation, with stop, rollback and deferred-dirty-suffix
tests. General node tasks/visible projection and dirty source inference remain
implementation work; the owner requirement and acceptance conditions are unchanged.

## M5 defaults recorded while implementing the contract

| Question | Options | Recommended default / action | Status |
| --- | --- | --- | --- |
| How is the S1–S11 prose versioned and changed? | Keep prose only in code; duplicate it in the spec; generate from a template file | `PROMPT_TEMPLATE_VERSION` in `-prompt`, byte goldens in conformance, spec defines layout and semantics | Adopted; bump the version with any wording change |
| Should stored segments reuse the extractor's `Segment`? | Add `major` to extraction segments; keep extraction unchanged; separate stored type | `StoredSegment { Markdown, Program { text, major } }`; extraction contract and its goldens stay M1-compatible | Adopted in SPEC §9.2 |
| What does `Session::restore` do before M4 state exists? | Reject snapshots; apply nothing silently and drop them; retain them for M4 | Re-parse the program, retain `restored_state`, round-trip it in `stored_message`; M4 applies values | Adopted; no state effects are claimed in M5 |
| Feedback hint for `schema.enum` | Catalog signature line only; legal values only; both | Detail carries legal values, hint stays the legal-value list; missing-required hints use the catalog signature | Adopted in SPEC §4.6 |
| How does static fallback handle dynamic props? | Evaluate early; omit the prop; emit a placeholder | Omit it (empty text) until M4 supplies evaluated values; never invent data | Documented in SPEC §4.7 |
| `StateSnapshot` shape | Opaque host JSON; tagged value list; versioned value map | `{ v: 1, values: { state-name: Value } }`, plain data, serde-stable | Adopted in SPEC §9.2 |

## M5 model-evaluation item (ROADMAP 4)

ROADMAP item 4 asks for an evaluation set with recorded model transcripts and
answers to the extraction open questions. No model endpoint or recorded model
transcripts are available in this worktree. Options considered: fabricate model
outputs, present the deterministic exemplars as model data, or build only the
deterministic prerequisites and record the gap. The adopted default is the third:
do not label synthetic output as model evidence. Prompt/feedback/message fixtures,
the prompt digest and the CLI demos are ready for a real recording run; the
evaluation set and transcript measurements remain open in `docs/milestones/M5.md`.

## M4 runtime slice boundary

- Implement value evaluation, fuel, state reads, typed state writes and an explicit
  epoch-bound `activate` entry now. Queries resolve to their declared default and
  `ref()` stays a `Ref` placeholder; host dispatch, delivery, events and
  `limit.event` stay in the next runtime slice.
- Wire action props keep the source-owner key and the epoch binding lives in
  `Session::action_handle`. This lets events be authorized without changing every
  action golden; the events slice can decide whether a handle string must also
  carry the epoch on the wire. SPEC §6.5 still requires epoch-bound handles.
- A host-effect step in an activated plan is returned as deferred rather than
  partially dispatched. State steps (`set`/`restore`) run in order. This keeps the
  plan atomic at the current host boundary.
- Runtime expression errors yield `null` with one diagnostic. Runtime node-dropping
  for a required non-nullable prop, slot `min_items` and form/field recovery remain
  future M4 work; the current slice only replaces values and reports diagnostics.
- `each` and node-producing `if`/`coalesce` stay checked plans until node
  instantiation is implemented; calling `each` in a value position is `expr.type`.

## M4 runtime slice 2 decisions (host boundary)

- Queries request after `finish()`; dependency changes are debounced through the
  virtual clock (300 ms). Pending requests beyond `max_concurrent_requests` stay
  queued and are pumped by delivery. A delivery is accepted only for the current
  `(stmt, args_hash)`; stale and cross-epoch ids are dropped. This is the simplest
  reading of §7.5's request table and avoids any real clock or async work in core.
- Tool descriptors are not part of `SessionOptions` yet. Every accepted query
  therefore reports `needs_approval = true`, the §7.5 default for `auto_run =
  false`. When descriptors arrive, this becomes descriptor- and provenance-based.
- Ref delivery error mapping is fixed because §7.7 names only three codes: a
  denied/failed host delivery is `ref.unknown`, too many bytes or too many
  resolutions is `ref.too_large`, and a nested ref is `ref.invalid`.
- Field formats are fixed in §2.14: scalar-value length, one `@` with a dotted
  domain for email, `http(s)://authority` for url, finite `f64` and `i64` parsing.
  Field status is exposed by node path until the renderer adds error/touched props.
- Event truncation drops `ctx` first, then trims `text`. Keeping visible text is
  more useful than keeping optional context; `limit.event` is emitted once.
- Action steps that create host requests are returned in `Update.requests` and,
  for `say`/custom actions, `Update.model_events`. The host still decides whether
  to send the model event; the core never transmits anything.

## M4 runtime slice 3 decisions (data-dependent nodes)

- Keep `Session::ir()` as the static validated tree and expose the expanded
  runtime tree as `Session::live_ir()`/`Resolved::ir()`. This preserves the
  batch/stream IR equality contract while patches, fields, actions and resolved
  trees see instantiated nodes.
- Retain a source element index on `SlotPlan` and `Node` rather than reconstruct
  ordering from path strings; path suffixes are not a total order for repeated
  names.
- Runtime drops keep the failing expression's diagnostic recovery (`ValueNulled`)
  because §4.3 fixes code recovery. A plain `Null` for a required non-nullable
  argument with no default reports `schema.missing_required`, whose recovery is
  `NodeDropped`.
- Action plans instantiated inside `each` capture lambda locals. A static
  template plan is reused only when its owner path is a prefix of the concrete
  element path; otherwise steps are rebuilt from the validated expression.
- Synthetic field/query props (`error`, `touched`, `status`) are emitted by the
  wire projection and ignored by the stream/batch tree comparison because they
  are session state, not batch projection.

## M4 review fixes — defaults adopted

- **Denied refresh.** §7.5 says "default stays", while the §4.3 recovery column
  said `KeptLastGood`. The recovery column was the contradiction; it is now
  `Defaulted`, and a denied refresh clears the previous result. `query.failed`
  keeps `KeptLastGood`, so a failed refresh may keep the last good value.
- **Cross-epoch delivery.** Request ids restart at 1 after `reset()`, so the id
  alone cannot identify a delivery. Pending entries now carry their epoch and
  `Session::deliver_from(epoch, id, ...)` rejects an id whose epoch differs from
  both the pending entry and the current session. `deliver(id, ...)` remains the
  current-epoch convenience for hosts that have just received the request.
- **Ref failure.** §4.3 fixes `ref.unknown`/`ref.too_large`/`ref.invalid` to
  `ValueNulled`; the old placeholder retention contradicted that. A failed
  delivery now evaluates the ref to `Null`, and a required non-nullable node then
  follows the §4.4 ladder (possibly `schema.missing_required` and `NodeDropped`).
- **Runtime library extensions.** Runtime fixtures may supply `library.json` to
  extend std with a custom action. This is used only to exercise `emit` without
  adding a standard action to the public std library.
- **Debounce base.** The 300 ms timer starts from the virtual clock at the
  dependency change (the clock before the observing `advance_clock`), cancels
  when arguments revert, and applies to queued queries.
## M6 renderer defaults recorded while implementing DESIGN/§10

| Question | Options | Recommended default / action | Status |
| --- | --- | --- | --- |
| Which screenshot backend should the M6 gallery use? | Windowed `render_to_image`; test-support headless; OS capture | Test-support `HeadlessAppContext` with `current_headless_renderer`, because it produced real text offscreen and avoids focus/display flakiness. Windowed capture remains implemented by `render_to_image`. | Adopted; SPEC §12.7 clarified |
| Should every `Text` be an entity so `append_text` notifies only it? | Inline leaves with nearest-container notification; one entity per Text | Inline leaves. The 2,000-node stress p99 was 11.6 ms inline versus 20.5 ms with one entity per Text. | Adopted with measured consequence in SPEC §10.6 |
| DESIGN §6 rule 5 says Grid `gap: 3`, but the std catalog default is `gap: 2` | Change the catalog default (new Library Spec hash); follow the catalog; change DESIGN | Follow the catalog in M6. The renderer cannot distinguish an omitted argument from the resolved default. Changing the catalog is a dedicated M2-contract change, not an M6 side effect. | Logged as a known 8 px vs 12 px deviation in PROGRESS/M6 |
| How should Markdown inline code and links look when the pinned TextView overrides the requested colors? | Custom Markdown renderer; accept and log; more theme overrides | Accept and log for M6. The TextView still paints inline code with an accent tint and links inherit the foreground in the captured light/dark set. A custom renderer or a pin-supported style fix is M8 work. | Logged |
| Is `render.panic` an M6 or M8 code? | M6; M8 | M8. The milestone table assigns panic containment to M8; M6 has no subtree panic boundary and does not claim one. | `conformance/deferred.json` and SPEC §12.6 updated |
| Should `rust-toolchain.toml` select 1.99.0? | Keep `stable` (locally 1.94, which cannot build gpui-pre); select 1.99.0 | Select 1.99.0. Core's 1.94 MSRV is recorded in the §1.2 matrix; the renderer needs the newer stable. | Adopted |
## M7 renderer defaults recorded while implementing inputs/forms (§§7.3–7.4,7.8,10.4,10.10,10.14)

| Question | Options | Recommended default / action | Status |
| --- | --- | --- | --- |
| How does a renderer name a node in `UiEvent`? | Keep IR `NodeId`; add a renderer event type; use the patch wire's `NodePath` | Use `NodePath` in the core `UiEvent`; `Session::event` resolves it. This removes a contract mismatch without duplicating the event type. `Session::activate(NodeId, ..)` remains for existing hosts. | Adopted; SPEC §§7.8,13 and CHANGELOG updated |
| How does the renderer learn bound values and revisions? | Put value/rev in synthetic patch props; add a host-side state API | Add `Session::state_values()` and `UiView::sync_state`. Patches stay unchanged, existing form goldens stay stable, and the view still applies the §10.4 rev/origin rule. | Adopted |
| How does a submit-only `Button` activate its form? | Require `on_click`; synthesize a button plan; accept the form's `on_submit` handle for that button | Accept the form handle when the node is a `submit: true` button in exactly that form. A button with both props still runs its own plan and then the form plan. | Adopted; core test and SPEC §7.4 |
| How is Tab traversal implemented when the kit Root consumes Tab? | Rebind Root's actions; let inputs indent; unbind Root/Input Tab and handle it in the view | Unbind `root::Tab`/`root::TabPrev` and `IndentInline`/`OutdentInline`; the view emits `Blur` and calls `Window::focus_next`/`focus_prev`. Notes fields traverse instead of inserting a tab. | Adopted; PINS.md records the pinned behavior |
| NumberInput, Slider and DateInput are named in the M7 brief but not in the phase-1 std catalog | Add components (new Library Spec hash); map to existing types | Map NumberInput to `Input(kind: "number"|"integer")`; Slider and DateInput are not in §8.8. Adding them would change the M2 library contract and hash, not a renderer milestone. | Not implemented; flagged for an owner-scope decision |
| How are extensions bounded? | Full child traversal; leaf builders only | Leaf builders only in phase 1. Hosts compose their own data; the IR child pipeline is unchanged. | Adopted; SPEC §10.14; panic containment stays M8 |
| Accessibility coverage on real platforms | Claim full coverage; measure the pinned projection | Measure the pinned accesskit projection and state that OS coverage is unverified: the test platform has no adapter. | Adopted; SPEC §10.10, PINS.md |
