# AGENTS.md

Instructions for coding agents working in this repository.

## What this is

A clean-room Rust generative-UI system. A language model writes a compact UI program in our DSL. The program is streamed, parsed, validated and turned into a patch stream that a native GPUI renderer applies. Phase 1 is the Rust core plus the GPUI renderer only.

## Sources of truth

- `spec/SPEC.md` (draft 0.2) is the contract. Read the sections relevant to your task before writing code.
- `spec/REVIEW.md` holds the review that 0.2 resolved. Background only.
- `spec/DESIGN.md` is the visual design language (tokens, layout rules, component anatomy). All renderer visuals come from it. (Draft 0.1. Re-read it before M6.)
- `ROADMAP.md` lists work that is not in the spec yet and the milestone each item is due by. Check it at the start of each milestone.
- `PROGRESS.md` is the running state. Read it first in every new session.
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
8. **Keep `PROGRESS.md` current.** After each milestone (and before stopping mid-milestone): status, evidence, next step, open issues, spec changes.

## Quality bar (no sloppy work)

- "Done" never includes stubs: no `todo!()`, `unimplemented!()`, placeholder returns, or `#[ignore]`d tests to get CI green. If something is out of scope, say so in `PROGRESS.md`.
- Never edit a `want.*` fixture just to make a failing test pass. Either the code is wrong, or the spec is (then rule 2 applies).
- Tests assert behavior and exact outputs, not only "does not panic".
- No `unwrap()`/`expect()` on any path that model input can reach in the core.
- Don't invent APIs. Before using any gpui / gpui-kit item, check it exists in the pinned version on docs.rs (`gpui-component 0.8.0`, `gpui-pre 0.3.9`). Record surprises in `PINS.md`.
- Renderer code uses theme tokens from `spec/DESIGN.md` only: no hard-coded colors, pixel sizes, or font sizes outside the theme module.
- When the spec is silent, don't guess silently: pick the simplest option, write it into the spec with a changelog line, or ask.

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

## Decisions and milestone reports

- **Don't block on owner decisions.** Log them in `DECISIONS.md` (question, options, your recommended default, what you did meanwhile) and keep going with the default unless it is irreversible or changes scope. The owner answers in batches.
- **Milestone report** (end of every milestone, also appended to PROGRESS.md): gates passed with numbers; spec changes and why; open DECISIONS.md items; risks; what the next milestone needs.
- **Self-merge:** you may merge your own milestone branch into main when every CI gate (SPEC §12.7–12.8) is green and the milestone review has no open blockers.

## Asking for help

When a decision is genuinely the project owner's (naming, scope, un-deferring an Appendix A item), stop and ask instead of guessing.
