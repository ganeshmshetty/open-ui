# AGENTS.md

Instructions for coding agents working in this repository.

## What this is

A clean-room Rust generative-UI system. A language model writes a compact UI program in our DSL. The program is streamed, parsed, validated and turned into a patch stream that a native GPUI renderer applies. Phase 1 is the Rust core plus the GPUI renderer only.

## Sources of truth

- `spec/SPEC.md` (draft 0.2) is the contract. Read the sections relevant to your task before writing code.
- `spec/REVIEW.md` holds the review that 0.2 resolved. Background only.
- `research/reports/*.md` holds the prior-art studies. Background only.

## Clean-room rule (hard)

- **Never open, read, search or copy anything under `research/openui/`.** That is third-party source code. Work only from `spec/` and `research/reports/`.
- Never copy code from other generative-UI projects either.

## Build order

Work one milestone at a time, in order, as listed in SPEC §14.1:

- **M0** Workspace, CI, conformance runner skeleton, `spec-check`.
- **M1** Lexer, splitter, extractor, batch parser.
- **M2** Library model and `-std`, resolver, validator, IR, limits.
- **M3** Streaming parser and patches.
- **M4** Runtime.
- **M5** Prompt, feedback, messages.
- **M6** GPUI skeleton.
- **M7** GPUI inputs and forms.
- **M8** GPUI data components and the sample host.

A milestone is done only when its "Done when" column in §14.1 is met and CI is green.

Exception: an early **GPUI spike** may run in parallel, on its own branch (`spike/gpui`). Its goal is to prove four things:

- the exact pin set in §10.9 compiles;
- batches queued by `enqueue` and applied inside `render` work (§10.1);
- `AnyView::cached` child views work;
- a hard-coded patch list renders in a window.

The spike is throwaway code that answers those questions.

## Workflow rules

1. **Fixtures first.** For each milestone, write or extend the conformance fixtures (§12) before the implementation. Fixtures define "done".
2. **Changing the spec.** If implementation shows the spec is wrong, ambiguous or contradictory:
   - Do not quietly work around it.
   - Change `spec/SPEC.md` and the affected fixtures in the same commit.
   - Add a line to `spec/CHANGELOG.md` saying what changed and why.
3. **Streaming is sacred.** Stream must equal batch (I1–I7). Run the chunking harness (§12.3) on every change that touches `syntax`, `ir` or `patch`.
4. **The core stays UI-free.** `<name>-core` must never depend on gpui, gpui-kit, tokio, or any UI or async crate.
5. **Safety.** No `unsafe` in core (`#![forbid(unsafe_code)]`). No panics on model input. Every recovery emits exactly one diagnostic (§4).
6. **Pins.** Use exactly the set in §10.9: `gpui-kit`, `gpui-component` and `gpui-base` at `=0.8.0`, and `gpui-pre` at `=0.3.9`. Commit `Cargo.lock`. Bump pins only in a dedicated change that follows §10.9.
7. **Keep changes small.** One milestone, or part of one, per branch or PR. Commit messages say which spec sections they implement.

## Conventions

- Rust stable, edition 2024, a Cargo workspace. Crate names as in §1.2. Use a placeholder name until the project is named, e.g. `genui-core`.
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace` before every commit.
- Write tests next to the code. Conformance fixtures go in `conformance/` (§12.1).
- Error and diagnostic codes must match the §4.3 table exactly.

## Commands (once M0 exists)

```bash
cargo test --workspace                  # unit tests + conformance
cargo run -p genui-cli -- spec-check    # validate every ```ui block in SPEC.md
cargo run -p genui-cli -- fixtures      # run conformance suite with all chunkings
cargo +nightly fuzz run parser -- -max_total_time=600
```

## Asking for help

When a decision is genuinely the project owner's (naming, scope, un-deferring an Appendix A item), stop and ask instead of guessing.
