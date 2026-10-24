## 2026-10-24 — M8 data components, panic containment, host loop and stress (DESIGN §7; SPEC §§7.9,10.1,10.3,10.6,10.8,12.6–12.7)

- §10.1 adds the M8 path-addressed `RendererEvent` bridge. M8 starts from the M6
  tip and M7's input wiring is not present, so the renderer reports
  `{ path, prop/value }` and the host resolves it with
  `Session::activate_at` / `edit_at` / `blur_at`. The core `UiEvent` and patch
  wire are unchanged.
- §10.1/§10.3 define the host image loader: `request_image`, `deliver_image` and
  `fail_image`, with 8 MiB / 4096 px / 16 MP / 256-source caps and a 10 s host
  deadline. The renderer still performs no I/O.
- §10.3 records the implemented mapping: kit `DataTable` for Table with sticky
  header, row virtualization, sorting, selection and resizable columns; a
  renderer-built Tabs bar with local selection and keyboard switching; List uses
  the plain column at or below 100 children and the kit virtual list above 100;
  chart hover tooltips and motion.
- §10.3/§10.6 add the 10,000-item List measurement: 77 created item entities,
  at most 22 rows built per frame, p99 2.34 ms app work, 775 ms construction,
  198 MB RSS. Ordered numbering widens the 16 px gutter for four-digit lists.
- §10.6 adds the 50,000-row measurement: 12 visible rows at every offset, steady
  app work p99 0.05 ms, one 323 ms construction frame (111 ms parse + 147 ms
  snapshot apply) and 772 MB process RSS. The construction-cost and memory
  targets are reported as misses, not waived.
- §10.8 records the implemented builder containment, one `render.panic`
  diagnostic per node revision, three-panic quarantine and props-change
  recovery, with the `--self-test-panic` evidence and the `panic = "unwind"`
  build requirement exposed by `CONTAINMENT_ENABLED`.
- §12.6 clarifies that `render.panic` is renderer-only and covered by that
  self-test rather than a core fixture; `conformance/deferred.json` records the
  reason.
- §12.7 adds `genui-cli transcripts`, the scripted sample host, the 25/50/75/100%
  screenshots, restore/regenerate checks and `table-stress`. The 20 checked-in
  conversations are labelled `scripted-fixture`; no live model run is claimed.
- §7.9 keeps undeclared values from a restored state snapshot so a host
  round-trips its data, while declared slots always reflect live state.
- DESIGN 0.4 records the 480 px table viewport cap, sticky header, list
  non-virtualization and the M8 motion/tooltip review.

# Spec changelog

## 2026-10-24 — M6 review integration: inert links, status visuals (§§10.7,10.12)

- Model-written Markdown/Code links are inert in the pinned `TextView` instead of
  falling back to `App::open_url`, closing a URL-policy bypass. A host policy hook
  is deferred; the renderer performs no I/O.
- The renderer maps the synthetic query/ref `status` prop to loading (requested,
  refreshing) and error (denied, failed) visuals for data components and charts.

## 2026-10-24 — M6 renderer implementation review (DESIGN §§4,10; SPEC §§1.2,10.1,10.3,10.6,10.12–10.13,12.6–12.7,14.1)

- DESIGN 0.3 raises the light `danger` text to `#c81e1e`: the documented
  `#dc2626` on `#fef2f2` measures 4.41:1, below the same section's WCAG AA
  requirement. `#c81e1e` measures 5.24:1. The theme's contrast tests cover every
  tone pair in both modes.
- Add the M6 platform matrix to §1.2: macOS arm64 is locally verified, Linux is
  compile/test verified by CI, macOS x86_64 and Windows remain supported by the
  pinned crates but unexercised, and the UI-free crates keep a Rust 1.94 MSRV.
  The renderer needs Rust 1.99 because `gpui-pre 0.3.9` uses
  `std::hint::cold_path`; `rust-toolchain.toml` now selects 1.99.0.
- §10.1/§10.6 record the measured notification boundary. M6 first tried one
  entity per `Text`, then measured the 2,000-node stress stream: app-work p99
  rose from 11.6 ms to 20.5 ms, so leaves stay inline and their nearest
  container is notified. The §10.6 p99 8 ms target is reported as missed, not
  waived. Cached boundaries are limited to Chart plots and Images.
- §10.3 updates the implementation mapping after the pinned-version audit: Card
  is the DESIGN anatomy; `Tag::custom` takes the background first; Alert uses
  explicit tone colors; Chart uses a small canvas plot because the kit bar chart
  ticks were not round and its line chart cannot leave null gaps. Chart hover
  tooltips and the Markdown inline-code/link color mismatch are recorded M8
  follow-ups.
- §10.13 adds the debug-only developer inspector: retained tree, bounded patch
  log and diagnostics list, with no protocol or IR changes.
- §12.7 allows the test-support headless `render_to_image` path used by the M6
  gallery, alongside the windowed path. Captures remain excluded from
  performance numbers.
- §12.6 and `conformance/deferred.json` move `render.panic` to M8, matching the
  milestone table's panic-containment scope; M6 does not claim a renderer panic
  boundary.
- Examples 01, 02 and 22 now pass percentages as percentages (`3.2`, `99.9`,
  `0.1`, `62`), not fractions. DESIGN §6.12 defines `percent` as a value already
  in percent; the old exemplars rendered `0%` for a fractional input.

## 2026-10-24 — M4/M5 integration: emit events join the stored event shape (§§7.8,9.3)

- Unify `ModelEvent` on the §9.3 shape. `emit` events now carry `name` and
  `params` in the fixed field order between `text` and `label`; `say` events are
  unchanged. This resolves the duplicate event types introduced independently by
  M4 (runtime emission) and M5 (stored rendering). No DSL behavior changed.

## 2026-10-24 — M5 prompt, feedback and message protocol (§§4.6,9.1–9.3,12.1–12.2,12.7,13)

- Define the static prompt layout (header, preamble, S1–S11, blank-line
  separators, one terminating LF), signature/type spelling, capability and
  example filtering, and `Prompt { text, digest, template_version }`. ROADMAP
  item 3's prose now lives in `genui-prompt`; golden fixtures pin it byte for byte.
- Define the per-turn ref catalog format and exact `ui-diag` feedback placement.
  Feedback paths prefer argument/slot locations over raw IR paths, hints are
  optional, duplicate merge and the 20-line cap are explicit, and spanless static
  diagnostics use a deterministic statement-name fallback order.
- Split storage segments from extractor segments: `StoredSegment` carries the
  program's language major so replay can emit `ui vN`. Define canonical replay
  text, `lib.mismatch` as Info while replay proceeds, and the `StateSnapshot`
  shape. This removes `lib.mismatch` from the deferred set; M4/M6 codes remain.
- Give `ui-event` a fixed field order and explicit optional-field omission so
  event text is byte-stable across platforms and JSON map backends.
- Add `prompt`, `feedback` and `message` conformance categories, exact prompt,
  stored-message, replay, event and feedback goldens, and the corresponding CLI
  inspection commands.


## 2026-10-24 — Mutation campaign sharding adjustment (§12.8)

- Split the complete mutation campaign over thirty-two disjoint CI shards. With
  thirty-two shards the repeated runner shutdown on the `syntax/source.rs` heavy
  shard is avoided by halving each job's mutant count; the inventory, per-mutant
  timeout and review rules are unchanged.

## 2026-10-23 — M4 review fixes (§§4.3,6.5,7.3–7.8,11,12.1,12.4–12.5,13)

- `query.denied` recovery is now `Defaulted`, matching the §7.5 effect text: a
  denied refresh clears the previous result and the declared default returns.
  `query.failed` keeps `KeptLastGood`.
- Deliveries carry an epoch. Request ids restart at 1 after `reset()`, so
  `Session::deliver_from(epoch, id, ...)` rejects an id from another epoch even
  when the number is reused; `deliver` remains the current-epoch convenience.
- A failed or denied ref delivery replaces the `Ref` placeholder with `Null`
  under `ValueNulled`; the §4.4 ladder then continues (a required node may drop).
  This resolves the contradiction between the recovery table and the old
  placeholder behavior.
- `max_event_bytes` is enforced for every event shape: dropping `ctx`, then one
  linear text trim, and exactly one `limit.event` even when text/label/params
  alone exceed the limit or the limit is 0.
- Dynamic field props (`bind`, `required`, `kind`, `min_len`, `max_len`, `min`,
  `max`) and dynamic `submit`/`on_submit` are read from evaluated props, not only
  from static literals.
- Data limits use canonical §6.5 bytes. Bidi/zero-width stripping now also
  covers `Mixed` strings, map keys, model-event text/labels and host-request text.
  `open` applies the host `UrlPolicy` to `Tool` provenance as well.
- The 300 ms debounce starts at the dependency change, is cancelled when the
  arguments revert, and is not skipped for queued queries. Runtime diagnostics
  keep repeated independent occurrences; only re-evaluation de-duplicates.
- Document the runtime `script.json` ops (including `reset`, `finish`, `push` and
  an optional delivery epoch), runtime `library.json` extensions, and the new
  `want.events.json`/`want.fields.json` comparison. Runners now reject unexpected
  requests/events when the corresponding want file is absent and replay every
  scripted patch batch.

## 2026-10-23 — M4 data-dependent nodes and runtime recovery (§§2.10–2.11,4.4,5.5,7.3–7.4,7.5,12.3)

- Instantiate retained `each`, node-producing `if` and `coalesce` plans at runtime
  in canonical order. `SlotPlan` and `Node` now retain their source element index
  so concrete children and plans keep their original interleaving; runtime
  instances consume `max_nodes`/`max_ir_depth` and a dropped subtree releases its
  allocation before the next sibling.
- Apply the §4.4 ladder to runtime props: an argument with a default takes it,
  an optional argument is omitted, and a required non-nullable argument that
  stays `Null` drops the smallest node. A required `Null` with no default reports
  `schema.missing_required`; an expression failure keeps its own diagnostic.
  Slot `min_items` therefore drops the parent after plan expansion.
- Action plans instantiated inside `each` capture their lambda environment and
  use the concrete element path as their identity.
- Resolved trees now carry synthetic `error`/`touched` props for fields and an
  aggregated `status` prop for query-reading nodes. They are not library
  arguments and never reach expressions.
- Clarify that an `each` key lambda binds its own parameters (item and index)
  independently of the body lambda.
- Add the `iteration` conformance category (12 cases) covering keys, duplicates,
  bad keys, nested templates, conditional production, drops, budgets and action
  locals. Existing runtime goldens whose required prop now drops the node were
  updated with this contract change, not to conceal a failure.

## 2026-10-23 — Mutation campaign sharding (§12.8)

- Split the complete mutation campaign over sixteen disjoint CI shards instead of
  eight and raise the per-shard job timeout to 240 minutes. The retained-source
  task/validator expansion pushed an eight-shard campaign past the old 180-minute
  limit before all mutants finished; the inventory and per-mutant timeout are
  unchanged.

## 2026-10-23 — M4 host boundary (§§2.14–2.15,5.5,7.3,7.5,7.7–7.8,12.2,13)

- Define fixed field format/length/range rules so `FieldStatus` is deterministic
  without a renderer: scalar-value length, four named formats, numeric comparison
  and the existing first-failure order. Expose status by canonical node path.
- Define query issuance: accepted queries at `finish()` in name order up to
  `max_concurrent_requests`, queued remainder, `advance_clock` 300 ms debounce,
  current `(stmt,args_hash)` delivery check, `Tool` tagging and size caps. Assume
  `auto_run = false` until tool descriptors are supplied, so approval is requested.
- Define ref delivery outcomes: placeholder before delivery, `Host` tagging,
  `ref.unknown` for denied/failed, `ref.too_large` for size or the per-message
  resolution cap, and `ref.invalid` for nested refs.
- Define model-event truncation order (drop `ctx`, then trim `text`) and that
  `open` yields only a request plus URL-policy errors.
- Runtime fixtures may assert `want.requests.json`, `want.events.json` and
  `want.fields.json`; §13 lists the new inspection entries.

## 2026-10-23 — M4 runtime evaluation foundation (§§2.6–2.7,5.5,7.1–7.4,12.2)

- Dynamic props are evaluated for live patches to resolved values; a failing
  expression is `null` with one runtime diagnostic. Query names resolve to their
  declared default and `ref()` stays a `Ref` placeholder until host delivery.
- Action plans remain inert; state-writing steps run only through an explicit
  epoch-bound activation after `finish()`. Host-effect steps are returned as
  deferred and are not dispatched in this slice.
- Add runtime fixture categories `numbers`, `expr`, `state` and `actions` with
  canonical `want.resolved.json` trees. Streaming checkpoint 029 now asserts the
  evaluated string rather than the retired M3 deferred representation.
- Clarify resolved-tree comparison bytes. No DSL or recovery rule changed.

## 2026-10-23 — Static cycle ownership (§2.10)

- Specify canonical ownership for cycles among static value/action definitions,
  which have no node-instantiation path. The previous DFS accidentally moved an
  existing cycle diagnostic when an earlier acyclic user arrived. One diagnostic
  per cyclic component, at its smallest name, is deterministic and retainable;
  node/path cycle recovery is unchanged. Resolve fixture 019 covers the change.

## 2026-10-23 — Budget recovery scope (§2.10)

- Clarify that a budget failure stops its immediate slot, independently of
  diagnostic deduplication. A dropped component reclaims allocation before its
  ancestor continues. New limits 023–024 reproduce inconsistent repeated-instance
  recovery and lost siblings after min_items rollback.

## 2026-10-23 — Provisional scanner work (§3.7–3.8)

- Clarify provisional lexer errors on possible resync headers, diagnostic dirty
  entries, held-back prefix accounting and explicit snapshot materialization.
  Streaming fixtures 031–032 catch premature cutoff suppression and history scans.

## 2026-10-22 — Stable live arena identities (§3.5–3.8, §5.5, §6.5)

- Separate opaque live expression identity from dense canonical snapshot indices.
  An earlier forward reference exposed the conflict: dense live indices forced
  writes to otherwise unchanged siblings, violating I7. Snapshot indices retain
  canonical first-use order; owners release obsolete expression bodies.
- Static action-plan references now use their owning node path and argument,
  instead of arena position. The action tag carries an opaque string, preventing
  unrelated insertion/rehash from changing a retained plan reference. M4 must
  still issue epoch-bound runtime handles; no dispatch or evaluator is added.
- Add focused earlier-expression/action fixtures and update the four existing
  static action-reference goldens and the provisional-action checkpoint by the
  documented source-owner formula. Host argument escaping has its own fixture.


## 2026-10-22 — M3 review regressions (§3.4–3.5, §12.2)

- Add exact streaming checkpoints for dropped drafts invalidating their consumers,
  draft bindings retaining sealed state types, and statement-count limits applying
  to previews. Greptile Plus identified these three implementation bugs; the
  existing provisional-isolation and limit rules remain unchanged.
- Clarify fixture comparison of empty slots: IR includes validated empty slot
  arrays, while retained patch trees normalize them because inserts do not carry
  empty slot declarations. A new statement-budget fixture exposed this distinction.


## 2026-10-22 — M3 mutation regression assertions (§12.8)

- Require direct assertions for binary/index dependency edges, repaired raw-byte
  endpoints, provisional closed-copy tails and active cutoff suppression. Two
  mutation shards exposed missing assertions despite final stream/batch equality.
  No DSL, recovery or production core behavior changes in this follow-up.
- Optimize mutation test builds and omit debug symbols to reduce full live-harness
  timeouts and disk use; preserve every workspace test and the complete inventory.
  A separate shard was interrupted by a runner shutdown and has no acceptance.

## 2026-10-22 — M3 streaming and wire clarifications (§3, §5.5, §6, §12.7, §13)

- Preserve selected raw program bytes alongside normalized extraction segments and seal at a completed closing fence. A CRLF budget fixture exposed M2 counting normalized message bytes, contrary to §2.5.
- Extend the I6 action-handle guard through reverse dependencies: a sealed consumer must not expose a handle from an unsealed action alias. An explicit live checkpoint caught this before acceptance.
- Scope G9 evaluation to M4 and carry stable structural deferred expressions in
  M3 wire props. The old text simultaneously required M3 patches and M4 evaluation.
- Encode typed maps as ordered tagged pairs: sorted JSON keys cannot preserve
  map entry order, and untagged maps collide with int/ref/handle tags. Metadata
  and Library Spec defaults stay ordinary JSON; no std hash change is intended.
- Add strict live checkpoints/session-script fixtures and expand the milestone mutation inventory to the M3 protocol, Session and canonical encoder.
- Specify root attachment, deterministic diff/coalescing order, empty push and
  idempotent finish behavior, snapshot Update/sequence consumption, replay output
  and authoritative provisional diagnostic withdrawal. Allow conservative semantic
  provisional diagnostics and hold cutoff-sensitive lexer errors until sealing.

## 2026-10-22 — M3 canonical golden bytes (§6.5, §8.2, §12.2)

- Apply the approved core-owned canonical encoder to Library Spec bytes/hash and
  IR fixtures. Goldens use sorted compact JSON plus a file-terminating LF; the
  LF is outside the hashed document. This replaces pretty fixture whitespace
  without changing the Library Spec hash or expected tree values.

## 2026-10-22 — GPUI spike findings (§6.5, §10.1, §10.3, §10.6, §12.7, DESIGN §10)

Approves the six proposals in docs/spikes/gpui.md:
- §10.1: patches are applied before the frame through the window handle (`apply`
  with `&mut Window`); `enqueue` is only a fallback, because notifications from
  draining in `render` landed a frame late. Adds a one-batch freshness test.
- §10.1, §10.6: nested `AnyView::cached` does not hold (an ancestor cache miss
  re-renders cached descendants), so caching is limited to fixed-size, expensive
  subtrees; containers are not cached.
- §10.1, §10.3: use `gpui_kit::open_window` (it installs `Root`); Card is a
  DESIGN-styled container, not kit `GroupBox`.
- §6.5: canonical JSON is written by the core's own encoder, independent of
  `serde_json`'s map backend, because GPUI enables `preserve_order` and feature
  unification changed the Library Spec bytes. CI checks this with the feature on.
- §12.7: the gallery screenshots use windowed `render_to_image`; headless driving
  is investigated separately.
- DESIGN §10: explicit adapter from gpui-base's named tokens; DESIGN's scales win.

## 2026-10-22 — Exhaustive printer pairs and faster mutation scheduling (§12.8)

- A prior timeout became a survivor in the renewed CI run: deleting the additive
  printer precedence rule. Add a grouped-expression fixture and 338 deterministic
  operator-pair round trips. No production printer behavior changed.
- Increase the complete campaign from four to eight disjoint CI shards to reduce
  wall-clock latency; preserve the same 609-name inventory, full workspace tests,
  baseline/source/review checks, and exact aggregation coverage.

## 2026-10-22 — Final mutation regression coverage (§12.8)

- Extend exact assertions to container-prefix/index budgets, multi-entry map
  printing, contextual header rejection and public splitter token/span output.
  The first complete CI campaign exposed assertion gaps in these existing rules;
  four new statement fixtures and focused API tests preserve current behavior.
  No production implementation or diagnostic contract changed.

## 2026-10-22 — Contextual expression budgets (§2.5)

- Clarify that growing binary/postfix prefixes consume the remaining budget after
  enclosing syntax. Mutation-driven exact fixtures exposed the parser granting
  each nested prefix a fresh max_expr_depth budget. Count enclosing depth in both
  checks; retain the existing cutoff-null sentinel behavior and one diagnostic
  per statement. Parser mutation testing and parser/validator fuzzing are renewed.

## 2026-10-22 — Enforced quality gates (§12.8)

- Split runtime deferrals into the required deferred.json; keep the covered-code
  fixture map disjoint and audit both against §4.3. Specify source-scan boundaries,
  theme locations, all-target/all-feature dependency scope (including dev edges)
  and per-commit fixture protection so the new
  gates have a reproducible policy. Run the milestone mutation job manually or via an explicit commit marker;
  aggregate four shards against a frozen inventory and current source hashes.
  Retain isolated mutation evidence and add
  exact nested-recovery/public-extractor, source-span, EOF-token/escape and
  fence-info, invalid separator/key and nested expression-cutoff regressions
  exposed by
  mutation testing; existing syntax behavior is unchanged.

## 2026-10-22 — M2 tree inspection (§12.7)

- Define deterministic text output, explicit deferred plans, default host policy,
  diagnostics and exit statuses for the new tree command, including successful
  Info-only and failing Warning+ reports. The first output fixture
  mistakenly used `~1`; corrected it to §5.2's existing second-use `~2` rule before
  accepting the implementation. No identity rule changed.

## 2026-10-22 — M2 static library, resolution and IR clarifications

- §2.7: spell out builtin arities, lambda shapes and named options; the function
  list had signatures without complete static call-check rules.
- §§2.7,2.10–2.11: define lambda shadowing/template scope, non-instantiated
  orphan checks, root depth/count, single-node roots, alias depth, deferred
  node plans and the sealed-table scope of permutations, so static validation
  does not evaluate data or invent multiple root identities.
- §§2.12–2.15: define deterministic state/query allocation, constant signed
  literals, state inference order, unused-state handling, malformed tool
  signatures, literal emit targets, lambda shadowing in actions and action-list
  recovery. Writes and deliveries remain M4 work.
- §§4.3–4.4: add the already-referenced cap.fallback/lib.mismatch rows and retain
  recovered-null metadata outside structural ASTs to prevent duplicate errors.
- §§5.5,8.1,8.4: describe static Dynamic/node/action/fallback plans; require total
  terminal fallback targets; clarify builder examples, metadata validation and
  backtick-marked component references in group prose. Align the fallible std
  builder example and enforce additive extension without base mutation.
- §§8.2–8.3: exclude the hash from its own input, specify canonical JSON/hash
  formatting, and add the missing generic any type used by data and inference.
  Make optional wrappers preserve underlying enum/token/action/binding/slot
  behavior and explicitly allow null on a required nullable argument; the generic
  recovery ladder otherwise contradicted the optional type definition.
- §§8.6,8.8: use terminal Text when markdown is unavailable; fix the stale std
  count to 29 (27 components plus 2 helpers), and assign TableColumn's structural
  node the group role. Define malformed literal Facts records and extra-key
  removal. Code highlighting remains an optional enhancement.
- §12: define M2 fixture inputs, frozen byte-for-byte Library Spec goldens,
  documentation host context, additive extension recipes, exemplar checks and
  the audited runtime deferrals. Check documentation fence versions through the
  shared extractor so a syntactically valid v2 body cannot silently pass.

## 2026-10-22 — Quality gates (§12.8)

- Add §12.8: seven CI gates that enforce the AGENTS.md rules (lints, dependency
  ban, no skipped tests, no hard-coded styles, fixture protection, diagnostic
  coverage, mutation testing), so milestones need less manual review.

## 2026-10-22 — End-to-end checks (§12.7, §14.1)

- Add §12.7: `tree` and `replay` CLI commands, scripted GPUI interaction tests,
  the gallery app with a screenshot review loop, and the real-model loop. The
  conformance suite proves the core but nothing checked that the whole system
  works and looks right before M8.
- M6, M7 and M8 done criteria now include these checks.

## 2026-10-21 — M1 syntax and recovery contract (§2.2–2.5, §2.8–2.9, §12)

- Specify one-token recovery for overlong names, underscore conversion and null
  replacement for out-of-range/non-finite numeric literals; the prior lexical
  bounds had no recovery behavior.
- Define unrecoverable RHS drops, name-claim retention, the state/name namespaces,
  reserved checks and statement-budget counting so duplicate and bad-input
  behavior is deterministic without a library model.
- Specify per-bracket recovery, one dangling-suffix recovery, both diagnostics
  on combined resync, normalized statement-byte measurement, UTF-8 cutoffs and
  suppression of errors induced solely by a size cutoff. Define null depth
  placeholders and bounded expression/string behavior for safe batch parsing.
- Generalize fences to matching backtick/tilde widths, define malformed/optional
  version words, open-fence EOF, normalized prose, empty programs and later prose.
  The original extraction text left these edge cases unspecified.
- Clarify that recovered-AST round trips reparse with syntax-limit ceilings: a
  bare component at the bracket ceiling prints as a call and needs another pair.
  Record the minimized fuzz case as a structural regression.
- Define strict AST/segment goldens and the M1 parse-level spec-check gate; keep
  library validation and live IR/patch invariants assigned to M2 and M3.

## 2026-10-21 — Stat, Facts and number formats (§8.8, §10.3, Appendix A)

- Add `Stat` (KPI) and `Facts` (key-value list) components and the `compact`,
  `currency` and `delta` formats, with a `currency` prop on TableColumn and Stat.
  Models were otherwise hand-building KPI cards and detail lists inconsistently.
  Rendering rules are in DESIGN.md §6.12 and §7.
- Add the `scalar` type (number or string) to §8.3 for `Stat.value`.
- Defer table density, chart height tiers and structured list items.

## 2026-10-21 — DESIGN.md 0.2

- Add table rules (truncation, alignment by type, header wrap), chart rules
  (zero-based bars, null gaps, round gridlines, category cap), number formatting,
  same-size states, empty states, interaction rules, and content guidance for
  prompt section S10, from the constrained-vocabulary and data-dense research.
- Add DESIGN.md Appendix A: six proposed component and prop additions (Stat,
  Facts, more formats, table density, chart heights, list items). Not in SPEC
  until the owner approves.

## 2026-10-21 — Design language (§8.8, §10.5, DESIGN.md)

- Add `spec/DESIGN.md` (draft 0.1): tokens, layout rules, component anatomy,
  streaming and loading visuals, built on gpui-base `SemanticThemeTokens`.
- §8.8: `Card` padding default changes from 3 to 4 (16 px), per DESIGN.md §6.
- §10.5: points to DESIGN.md as normative for the GPUI renderer.

## 2026-10-21 — Conversations and prose (§7.9, §10.12, §13)

- Add §7.9: one session per message, message lifecycle (streaming, live, frozen),
  freeze and restore, per-message state, and message ids on events. A chat has
  many messages, and draft 0.2 only specified a single message.
- Add §10.12 and `Update.prose`: how prose segments around the program reach the
  renderer and are sanitized. Draft 0.2 extracted prose but never said who draws it.

## 2026-10-21 — M0 fixture bootstrap (§2.2, §12.1, §12.2, §12.6)

- Define `want.tokens.json` and lexical comparison fields because the original
  layout listed lexical coverage without a token golden format.
- Make the M0 lexical-only `spec-check` and finish-only chunking scope explicit,
  consistent with §14.1: semantic validation needs M2, and live streaming
  invariants need M3. No later milestone's acceptance requirement is relaxed.
- Specify malformed Unicode escape consumption and a final backslash at a line
  or input boundary so error recovery is deterministic; add matching lexical
  fixtures for those previously unspecified cases.
