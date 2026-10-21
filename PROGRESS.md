# Progress

Updated: 2026-10-21. Milestones follow SPEC §14.1 in order. Update this file after
**every milestone**, recording acceptance evidence and any remaining limitations.
A milestone is complete only after its acceptance checks pass and CI is green.

| Milestone | Status | Evidence / next gate |
| --- | --- | --- |
| M0 — Workspace and verification scaffolding | Complete | 21 lex fixtures / 4,743 chunkings; 7 spec examples; Linux and macOS CI green. |
| M1 — Batch syntax | Not started | Lexer completion, splitter, extractor, AST/parser, round trip, 10-minute fuzz run. |
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

M0 is complete. M1 is the next milestone; no M1 implementation is claimed here.
