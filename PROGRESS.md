# Progress

Updated: 2026-10-21. Milestones follow SPEC §14.1 in order. Update this file after
**every milestone**, recording acceptance evidence and any remaining limitations.
A milestone is complete only after its acceptance checks pass and CI is green.

| Milestone | Status | Evidence / next gate |
| --- | --- | --- |
| M0 — Workspace and verification scaffolding | Complete | 21 lex fixtures / 4,743 chunkings; 7 spec examples; Linux and macOS CI green. |
| M1 — Batch syntax | Awaiting fuzz/CI | 124 fixtures / 28,225 chunkings; 23 tests including 2,048 generated round trips pass locally; final timed campaigns running. |
| M2 — Library, validation, IR and limits | Not started | Category fixtures, diagnostic coverage, full spec validation. |
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

Fuzz status: final 600-second campaigns are running on the frozen source. Early
20-second smoke runs passed. The first fixture-seeded parser campaign found a
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
behavior change; timed acceptance campaigns will use that final source.

Remaining acceptance gates: finish all four clean campaigns and verify hosted
Linux/macOS CI. No known unresolved syntax issue. Semantic library resolution,
constant-state validation, general builtin/type checking, IR, provisional tails,
statement hashes and patch invariants remain M2–M4 work. The dormant IR/runtime/
library/GPUI scaffolds are explicitly outside M1. Next milestone: M2.
