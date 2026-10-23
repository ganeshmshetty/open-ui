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
