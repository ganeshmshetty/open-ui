# Conformance fixtures

The layout and comparison contract live in SPEC §12. M1 implements `lex`,
`grammar`, `statements`, `extract`, and syntax `fuzz-regress`.

Every fixture has strict JSON metadata (`case.json`), raw bytes (`input.ui`), and
a diagnostic multiset (`want.diag.json`). Unknown fields and unsupported files
fail, including output files that do not belong to the fixture's category.

- `lex`: `want.tokens.json` is an ordered array of `{kind, text}`. Strings are
  decoded, state names omit `$`, newlines use LF, other tokens retain spelling.
- Syntax categories: `want.ast.json` is a structural AST. Statement kinds are
  Bind, State and Query; expressions use a tagged `kind`. Locations are excluded.
- `extract`: also requires `want.segments.json`, an ordered array of
  `{kind: Markdown|Program, text}` with exact normalized segment text.
- Diagnostics: compare `(code, stmt, path, recovery)` as a multiset, preserving
  multiplicity. Standalone lexer diagnostics have null `stmt` and `path`.

`schemas/` contains JSON Schema 2020-12 definitions, checked by the Rust runner.
`008-bad-utf8/input.ui` and the extraction UTF-8 fixture intentionally contain
invalid bytes. `.gitattributes` preserves all fixture inputs verbatim.

An optional `stream.json` is an array of strings whose UTF-8 bytes must concatenate
exactly to the input. The runner also tries whole input, byte by byte, every
two-way split for inputs up to 2 KiB, 200 deterministic random chunkings (sizes
1–97, seeds 1–200), and LF-delimited lines. Byte chunks may split UTF-8 or CR LF.
Chunkings are visited one at a time, without quadratic retained input storage.

Each chunking feeds the actual resumable scanner. Final tokens, spans, AST,
segments and diagnostics must equal batch, which uses the same scanner path.
Every parsed fixture additionally prints and reparses to the same structural
AST, with syntax ceilings for the canonical output (SPEC §12.6).

```sh
cargo run -p genui-cli -- fixtures
cargo run -p genui-cli -- fixtures conformance
cargo test -p genui-conformance
```

M1 supports core fixtures and six syntax-limit overrides: `max_program_bytes`,
`max_stmt_bytes`, `max_stmt_count`, `max_bracket_depth`, `max_expr_depth`, and
`max_string_bytes`. Other overrides and interactive fixtures fail explicitly.

M2 adds Library Spec/IR goldens and semantic validation; M3 adds live invariants,
statement hashes, patch replay, and provisional-effect checks. M1's byte-scanner
checks do not claim those later guarantees. See `fuzz-regress/` for the reduced
round-trip budget regression and `fuzz/runs/m1/` for recorded fuzz evidence.

M2 implements `library`, `resolve`, `schema`, `ir` and `limits`. Cases with DSL
input compare canonical concrete IR and exact diagnostic multisets under every
byte chunking. `resolve` and `limits` additionally run 10 seeded permutations of
the sealed non-root statement table. Syntax cutoff diagnostics are retained from
the original parse. No evaluation occurs: Dynamic props and node plans remain
symbolic, with focused Rust assertions checking their retained data.

Library cases use strict `library.json` builder input and exact `want.build.json`
error arrays. Cases with `input.ui` also run all chunkings against that library.
Focused allocator cases use `paths.json`, `want.paths.json` and exact
`want.diag.json`; they exercise typed/escaped keys, index fallback and injected
initial hash collisions without evaluating `each`.

`diagnostics.json` maps every covered code to fixture paths. `deferred.json`
records the milestone/reason for every deferred code. Tests audit it against the spec's full
code table and require all conditional static recovery variants. Runtime-only
limits remain M4 work; every limit's hard ceiling is checked in the core tests.
The runner reports chunkings and permutations separately from direct builder/
allocator fixture checks. `spec-check` also checks all 24 exemplar programs.

M3 implements `streaming` and `patches`. Their IR and diagnostic goldens use the
same batch oracle; patch fixtures also require canonical `want.patches.json`.
An optional `checkpoints.json` asserts exact live props/absence/status after
selected explicit chunks. A patch `script.json` can interleave push, finish,
snapshot and reset, with exact batches including epoch and sequence values.
Unknown checkpoint/script fields fail deserialization.

Every category now also runs the live Session harness: immutable ordered seals,
final IR/diagnostic equality, no provisional state/queries/action handles, prose
reconstruction, checked patch replay and contiguous sequence numbers. Parser work
counters verify parse-once seals and bounded tail reparsing; they do not prove
the unresolved dirty-only IR/patch complexity requirement (see M3 report).

M4 implements runtime evaluation and the host boundary. `numbers`, `expr`,
`iteration`, `state`, `actions`, `queries`, `refs`, `events`, `forms` and
`provenance` fixtures compare a canonical `want.resolved.json` retained tree
(`id`, `kind`, `role`, evaluated `props`, `status`, `children`) plus static and
runtime diagnostics. They may also assert `want.requests.json`,
`want.events.json` and `want.fields.json`; a missing request/event file means
none are permitted, so unexpected host effects fail the fixture. `want.ir.json`
remains the symbolic validation IR and is not compared in runtime categories.

`script.json` is an ordered array of tagged ops:
`{"op":"activate","node":"<path>","prop":"<name>"}`,
`{"op":"edit","node":"<path>","value":...}`,
`{"op":"blur","node":"<path>"}`,
`{"op":"deliver","request":N,"epoch":E,"result":...}` (or `"error"`:
`denied`/`failed`/`too_large`),
`{"op":"advance","ms":N}`, `{"op":"reset"}`, `{"op":"finish"}` and
`{"op":"push","input":"..."}`. `epoch` is optional and defaults to the current
epoch; deliveries from another epoch are dropped. Activation is ignored before
`finish()` or for a stale epoch/handle. The runner applies every returned patch
batch through `Replay` and requires it to equal the live tree (G6), including
across `reset()` epochs. A runtime fixture may add `library.json` to extend std
with custom actions (used by the `emit` fixture).

Queries request after `finish()`, debounce dependency changes for 300 ms through
`advance_clock`, and deny/fail according to the request table. Denied refreshes
return to the declared default; failed ones keep the last good value. `ref()`
stays a placeholder until delivery, then becomes `Host` data or `Null` with
`ref.unknown`/`ref.too_large`. Synthetic `error`/`touched` props mirror field
status and `status` mirrors query state.

Canonical JSON goldens are compact sorted bytes plus one file LF, independently
encoded by the core. Typed semantic maps retain entry order as tagged pairs.

`mutations/m3/` holds the frozen M3 inventory and source-matched equivalent-mutant
reviews; historical M2 acceptance stays in `mutations/`. Mutation metadata is not
a fixture category and does not contribute to the fixture count.
