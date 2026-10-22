# Progress

Updated: 2026-10-22. Milestones follow SPEC §14.1 in order. Update this file after
**every milestone**, recording acceptance evidence and any remaining limitations.
A milestone is complete only after its acceptance checks pass and CI is green.

| Milestone | Status | Evidence / next gate |
| --- | --- | --- |
| M0 — Workspace and verification scaffolding | Complete | 21 lex fixtures / 4,743 chunkings; 7 spec examples; Linux and macOS CI green. |
| M1 — Batch syntax | Complete | 124 fixtures / 28,225 chunkings; 23 tests including 2,048 generated round trips; four clean 600-second fuzz campaigns; Linux/macOS CI green. |
| M2 — Library, validation, IR and limits | In progress | M1 merged into main; milestone/m2 created from main. Fixtures and contract clarifications first. |
| M3 — Streaming and patches | Not started | Live invariant checks, patch replay, 100k generated programs. |
| M4 — Runtime | Not started | Evaluation and interactive fixture categories. |
| M5 — Prompt, feedback and messages | Not started | Golden output and cross-platform determinism. |
| M6 — GPUI skeleton | Not started | Live streaming demo and recorded performance. |
| M7 — Inputs and forms | Not started | Stable editing, keyboard form demo and accessibility. |
| M8 — Data components and sample host | Not started | Table stress, panic containment, transcript replay, security review, v0.1 tag. |

## M0 scope

- Six crate scaffolds (§1.2), Rust stable / edition 2024, UI-free core with unsafe
  code forbidden, checked-in Cargo lockfiles, exact dormant GPUI pins.
- Lexical fixtures written before implementation, strict fixture schemas,
  diagnostic multiset and ordered-token comparisons, seeded byte chunkings.
- `fixtures` and lexical-only `spec-check` CLI commands; later commands explicitly
  unsupported. Actual semantic and live-stream guarantees remain future work.
- Live lexer fuzz target plus seven compiling placeholders; no timed fuzz run
  claimed. Linux and macOS CI runs format, Clippy, tests, fixtures, spec examples,
  and fuzz scaffold checks.
- Third-party `research/openui/` sources were not read and are ignored by Git.

## Verification

Local gates passed:

- `cargo fmt --all -- --check` and fuzz scaffold formatting.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- `cargo test --workspace --locked`: 13 tests, including checked-in conformance.
- `cargo run --locked -p genui-cli -- fixtures`: 21 fixtures, 4,743 chunkings.
- `cargo run --locked -p genui-cli -- spec-check`: 3 complete programs and 4
  fragments, zero lexical diagnostics.
- `cargo check --manifest-path fuzz/Cargo.toml --bins`: all 8 scaffolds compile.
- `cargo tree -p genui-core --edges normal`: only serde and its derive dependencies.

Hosted CI: [M0 acceptance run](https://github.com/ganeshmshetty/open-ui/actions/runs/39634933301)
passed on Linux and macOS for implementation commit `69e959f`. All eight workflow
checks passed on both platforms. The repository is private.

Additional negative checks confirmed that changed token/diagnostic goldens,
unknown metadata fields, and an explicit stream that does not reproduce its
input all exit nonzero.

M0 acceptance evidence above is historical. Current work is recorded below.

## M1 — Batch syntax

Read updated AGENTS.md (including Quality bar), ROADMAP.md and the spec changelog.
No roadmap item is due in M1. Visual design, conversations, Stat/Facts and runtime
changes remain with their owning milestones. Third-party source was not read.

Built:

- Resumable UTF-8/BOM/CR normalization and lexical state (growing names/numbers,
  comments, operators, strings, escapes), token spans and literal bounds.
- Exactly one splitter, shared by batch and push, with token-based statement
  starts, one-line lookahead, resync, closers, bracket/statement budgets and
  immutable sealed statements. Only an oversized retained prefix is re-lexed
  for recovery; no second boundary implementation exists.
- Resumable fenced/unfenced model-message extraction, prose segments, version
  rejection and extra-program handling.
- Structural AST, precedence parser, structural recoveries, duplicate/reserved
  checks and name claiming, query/state forms, canonical printer.
- Strict token/AST/segment/diagnostic fixture schemas; all syntax categories run
  every §12.3 chunking. Documentation examples now parse, with negative checks
  proving lexically valid syntax errors fail and `ui-invalid` needs diagnostics.
- Four live fuzz targets; M0's no-op future targets were removed.

Fixtures were written before their implementation. No M0 token/diagnostic golden
was changed. Later fixtures added regression coverage before the relevant fix.

Local acceptance evidence:

| Category | Fixtures |
| --- | ---: |
| lex | 31 |
| grammar | 33 |
| statements | 32 |
| extract | 26 |
| fuzz-regress | 2 |
| Total | 124 |

- 28,225 chunkings pass, comparing exact final scan/AST/segment results and
  diagnostic multisets to batch. Every parsed fixture also round-trips.
- 23 Rust tests pass, including a 2,048-case generated-program property that
  requires AST equality and zero diagnostics using default budgets.
- `spec-check`: 3 complete programs and 4 fragments parse without diagnostics.
  Synthetic tests exercise accepting and rejecting `ui-invalid` blocks.
- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --workspace`, fuzz formatting and all four fuzz-target builds pass.

Fuzz acceptance: all four campaigns completed on implementation commit
`8586ba9` with AddressSanitizer, seed `20261021`, max input length 32,768 bytes,
and fixture/boundary-seeded corpora. Runs were concurrent. Source and full-log
SHA-256 hashes were checked against the final files; all match.

| Target | Engine seconds | Executions | Exit code |
| --- | ---: | ---: | ---: |
| lexer | 601 | 1,239,855 | 0 |
| splitter | 601 | 179,892 | 0 |
| extractor | 601 | 571,567 | 0 |
| parser | 601 | 293,047 | 0 |

Commands, tool versions and startup/final excerpts are checked in under
`fuzz/runs/m1/`. Rust nightly was `1.101.0-nightly (19833af72 2026-10-20)`;
cargo-fuzz was `0.13.2`. Engine durations exclude compilation/setup time.

Early 20-second smoke runs also passed. The first fixture-seeded parser campaign found a
round-trip **test-budget mismatch**, rather than a model-input panic: printing
an implicit bare component adds parentheses at the bracket ceiling. The original
minimized artifact/log is in `fuzz/runs/m1/initial-*`; two checked-in regressions
cover the reduced behavior. The canonical-output parse now uses syntax ceilings,
and all four timed campaigns were restarted after final scanner corrections.
Interrupted campaigns are not counted toward acceptance.

Spec changes (same commit as fixtures) are fully listed in spec/CHANGELOG.md:
lexical bound recoveries; RHS/name-claim/namespace/count rules; forced-boundary
recovery cardinality and size/depth/string behavior; fence/version/prose edge
cases; strict AST/segment goldens; M1 spec-check and round-trip budget rules.

The first hosted run found one Rust 1.99 Clippy style lint (`while_let_loop`)
that Rust 1.94 did not report. The equivalent loop was rewritten without a
behavior change; the completed campaigns use that final source.

Hosted acceptance CI passed on Linux and macOS for `8586ba9`:
[CI run](https://github.com/ganeshmshetty/open-ui/actions/runs/39649523210).
All eight checks passed on both platforms. Final evidence/documentation is
committed separately and receives the same CI workflow.

M1 is complete. No known unresolved syntax issue. Semantic library resolution,
constant-state validation, general builtin/type checking, IR, provisional tails,
statement hashes and patch invariants remain M2–M4 work. The dormant IR/runtime/
library/GPUI scaffolds are explicitly outside M1. Next milestone: M2.

## M2 — work in progress

Approved M1 was fast-forwarded into main and pushed (38d8912). Work continues on
milestone/m2 from main. Re-read AGENTS.md, PROGRESS.md, ROADMAP.md, CHANGELOG and
the requested syntax, diagnostic, IR, library and conformance sections.
ROADMAP item 2 is due: 20–30 complete exemplar programs and full spec-check.

### Open issue for M3 (M1 review)

`Splitter::finish()` suppresses lexer diagnostics (`BadChar`, `BadEscape`,
`UnterminatedString`, `Overflow`, `NonFinite`) inside size-cutoff skipped ranges.
In streaming, push would already have emitted them. M3 must hold them provisional
until the cutoff decision, or stream != batch. Preserve a regression test when
adding live diagnostic emission; M2 must not claim this is solved.

M2 evaluation is static only. State reads, query results, refs and expressions
requiring evaluation retain typed Dynamic plans. No evaluator is implemented.
Acceptance evidence and diagnostic coverage/deferred-code audit will be added
before completion. Unrelated research/marketing files remain untouched.

### M2 implementation checkpoint — 2026-10-22

Fixtures were authored before implementation (initial 62 cases, then focused
regressions before each affected change). Current totals: 256 fixtures, including
132 new M2 cases; 55,802 byte chunkings and 360 seeded sealed-table permutations.
All five M2 categories pass: library 20, resolve 17, schema 61, ir 15, limits 19.
The 49 Rust tests include 2,048 AST round-trip cases and two 512-case semantic
properties (permutation invariance and path stability across append/recovery).

Built the checked builder, additive extension guards, complete 29-entry std
catalog (including Stat/Facts), canonical Library Spec and independent xxh3-128
hash golden, static resolver/validator, bottom-up recovery, typed values/props,
path allocator with collision rehash, state/query declarations, action/form plans,
capability fallbacks, node-template plans, and bounded static limits. Deferred
plans retain their node definitions and lambda environments. Only concrete
instances consume node/depth budgets or report path cycles; M4 instantiates data.

Full spec-check passes: 3 complete SPEC programs, 4 fragments and all 24 complete
exemplars. ROADMAP item 2 is done. Every std component example validates with no
Warning+ diagnostics. Golden hash: d19a769bd510997ce786ade4d60db23d,
independently calculated with Python xxhash, then compared byte for byte by Rust.

The audited 69-code table has 59 codes covered by fixtures and 10 explicit
execution/replay/renderer deferrals; see conformance/diagnostics.json. Conditional
recovery variants are also audited. New spec clarifications and their reasons
are listed in spec/CHANGELOG.md. There is no evaluator, host request dispatch,
query/ref delivery, live patch stream or renderer in this milestone.

Local fmt, Rust 1.99 Clippy, workspace tests, fixtures, spec-check and stable fuzz
build pass. A 21-second ASan smoke run completed 36,562 executions with exit 0;
that smoke predates final template/slot hardening and is not acceptance evidence.
The frozen-source 600-second validator campaign and hosted Linux/macOS CI are
next. M2 remains in progress until both succeed and final evidence is committed.


### M2 nullable-type regression checkpoint — 2026-10-22

The first implementation CI passed on both Linux and macOS for eb8250b:
https://github.com/ganeshmshetty/open-ui/actions/runs/39705728214 .
Supplemental exact fixtures exposed missing transparency through nullable
wrappers (enum codes, token encoding/range, bindings, actions and node slots).
Those cases were authored and observed failing before the fix; required nullable
and recovered-null behavior is now explicit in SPEC §§4.4,8.3 and CHANGELOG.
Expression interning also preserves recovery metadata despite structural AST
round-trip equality excluding it.

The timed campaign on eb8250b was intentionally interrupted after 415,105
executions (libFuzzer exit 72; no observed crash). It does not count as acceptance.
Retained evidence is in fuzz/runs/m2/interrupted-validator.* . The validator target
now includes a builder-defined Probe component covering nullable compositions.
The corrected source will receive a fresh full 600-second campaign.

Current local acceptance checks pass: 275 fixtures / 62,343 chunkings /
390 permutations, 50 Rust tests, all spec blocks and 24 exemplars. Library has
36 cases, resolve 17, schema 61, ir 15, limits 22. Extra cases cover additive
extension and Facts at 50, at 51, and with a composed global limit. This remains
M2 only; the M3 splitter issue above is still open.
