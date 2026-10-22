# Owner playbook: running M3–M8 without the architect session

This file holds the prompts and review steps the owner uses to drive the coding agent (Sol / Codex) through the remaining milestones. Agents follow AGENTS.md; this file is for the owner.

## The loop for every milestone

1. Start a **fresh** agent chat. Paste the milestone prompt below.
2. When the agent reports done, it opens a PR (it does not self-merge until step 4).
3. Run the **review** (Greptile on the PR, plus the reviewer prompt below in a separate fresh chat).
4. Paste the review findings back to the building agent: "Fix every blocker and major, or justify it in the PR. Then merge if all gates are green."
5. Answer anything new in `DECISIONS.md` (accept the recommended default unless you disagree).

If the agent drifts (forgets rules, contradicts the spec, touches `research/openui/`), open a new chat: "Read AGENTS.md and PROGRESS.md, continue."

## Reviewer prompt (use in a separate fresh chat after each milestone)

```
You are reviewing PR <link or branch> for milestone M<n> of this repo. You did not write it. Read AGENTS.md, spec/SPEC.md (sections listed in the milestone row of §14.1 and in docs/PLAYBOOK.md for M<n>), spec/CHANGELOG.md and the milestone report in docs/milestones/M<n>.md. Then review the diff against main.

Check, and report each as PASS / FAIL with file:line evidence:
1. Every "Done when" item in SPEC §14.1 for M<n> is actually met (run the commands; don't trust the report).
2. Spec changes in this PR: each is justified in CHANGELOG, doesn't weaken an invariant, and has matching fixtures.
3. No want.* fixture changed without a matching SPEC + CHANGELOG change.
4. Tests assert exact behavior (not just "no panic"); spot-check 10 tests.
5. No unwrap/expect/panic on model-input paths in genui-core; no stubs; no ignored tests.
6. The milestone-specific checklist in docs/PLAYBOOK.md for M<n>.
Then list findings as Blocker / Major / Minor with a concrete fix each. Do not edit code.
```

## Milestone prompts

Each prompt assumes the agent first re-reads AGENTS.md, PROGRESS.md, ROADMAP.md (items due by this milestone), spec/CHANGELOG.md and the previous milestone report, merges main into its branch, writes fixtures first, and ends with a report in docs/milestones/M<n>.md.

### M3 — streaming and patches (in progress)

Milestone-specific review checklist:
- Stream == batch holds for every fixture under every §12.3 chunking, and on generated programs (check the 100k run log).
- The M2 open issue is fixed: size-cutoff lexer diagnostics are provisional while streaming (look for fixtures that reproduce it byte by byte).
- Sealed statements never change (statement-hash trace), provisional content creates no requests/state/actions.
- Patch guarantees G2, G6, G7 are checked by the harness, not just claimed.
- Canonical JSON encoder sorts keys itself, and the CI job with `serde_json/preserve_order` passes with identical goldens.
- `genui-cli replay` works on a few examples/*.ui files.

### M4 — runtime

```
Implement milestone M4 (SPEC §14.1) on milestone/m4: values and builtins, provenance, state store, forms, actions, queries, refs, and conversation lifecycle.
Read first: SPEC §2.6–2.7, §2.12–2.15, §5.5–5.6, §7 (all, incl. 7.6 provenance/URL policy and 7.9 conversations), §11, §12.4 (numbers, expr, iteration, state, forms, actions, queries, refs, provenance) and §12.5 (interactive tier with script.json).
Must-do: fixtures first for every listed category in both tiers; the §7.5 request table as an explicit state machine with a fixture per transition; provenance tagging and URL acceptance per tag (§7.6) with adversarial fixtures (user text concatenated into Image src, open(), tool URL args); fuel limits per prop; Session::freeze/restore (§7.9) round-trips; fuzz the evaluator 10 minutes; mutation gate on runtime modules. Same rules as always. Open a PR when done; don't self-merge until reviewed.
```
Review checklist: no evaluation order dependence (permutation tests still pass); tainted strings can never reach a URL prop; queries never fire from provisional content or before finish(); stale deliveries are dropped; every §4.3 runtime code now has a fixture or a deferral reason.

### M5 — prompt, feedback, messages

```
Implement milestone M5 (SPEC §14.1) on milestone/m5: the prompt generator, ui-diag feedback, stored messages and ui-event rendering. Also ROADMAP item 3 (write the actual prompt text for S1–S11, including DESIGN.md §8.1 content guidance in S10) and item 4 (model evaluation set).
Read first: SPEC §4.6, §4.7, §9 (all), §8.2, §12.4 (prompt, feedback, message), DESIGN.md §8.1, ROADMAP items 3 and 4.
Must-do: golden prompt per option set, byte-identical on macOS and Linux CI; feedback is a pure function of final text (fixtures prove runtime diagnostics are excluded); eval/ with ~30 tasks, a runner that sends the prompt to a configurable model API (key from env, never committed), validates outputs with parse_batch, and reports valid-program rate, diagnostics per program, tokens vs a JSON baseline. Run it once with whatever model API the owner configures and record results; answer SPEC §14.2 open questions 2 and 3 from the data. Open a PR; don't self-merge until reviewed.
```
Review checklist: prompt is generated (not hand-written signature lines); examples in the prompt all validate; eval results recorded with model name and date; any syntax pain points models showed are logged as proposed spec changes in DECISIONS.md.

### M6 — GPUI skeleton

```
Implement milestone M6 (SPEC §14.1) on milestone/m6: genui-gpui with the exact pins, UiView apply/enqueue (§10.1), retained tree, layout and text components, theming from DESIGN.md, the gallery app with screenshots (§12.7), and the developer inspector (ROADMAP item 5).
Read first: spec/DESIGN.md (all), SPEC §10.1–10.6, §10.9, §10.12, §12.7, docs/spikes/gpui.md, PINS.md, ROADMAP items 5, 9, 15.
Must-do: theme module is the only place with literal colors/sizes (DESIGN §10 token adapter); the one-batch freshness test; canonical JSON goldens unchanged with GPUI in the build; gallery renders every implemented component's examples in light and dark via render_to_image; review every screenshot against DESIGN.md and fix or log each finding; record frame time and memory for a streaming 2,000-node program; write the platform matrix (ROADMAP 9). Open a PR with the screenshots attached; don't self-merge until reviewed.
```
Review checklist: open the screenshots yourself (owner) and compare with DESIGN.md §2–§7: spacing, type sizes, borders, Card look, dark mode; streaming demo has no flicker; no hard-coded styles outside theme (gate passes).

### M7 — inputs and forms

```
Implement milestone M7 (SPEC §14.1) on milestone/m7: GPUI inputs and forms, focus, the input sync rule, gating, accessibility roles, scripted interaction tests (ROADMAP 16), and the renderer extension API (ROADMAP 6).
Read first: SPEC §10.4, §10.10, §7.3–7.4, §7.8, §2.14, DESIGN.md §7 and §7.1, ROADMAP items 6 and 16.
Must-do: scripted GPUI tests for click, type, select, submit, focus for every input component; typed text survives rebuilds; buttons disabled while streaming; errors appear after blur; keyboard-only run of the form demo; gallery screenshots of every input state reviewed. Open a PR; don't self-merge until reviewed.
```
Review checklist: the store↔widget sync never loops or resets the cursor (test exists); submit gating lives in core, not the renderer; ui-event JSON matches §7.8 including the real label.

### M8 — data components and sample host

```
Implement milestone M8 (SPEC §14.1) on milestone/m8: List, Table, Tabs, Chart, Image, Stat, Facts in GPUI; panic containment (§10.8); the image loader contract (ROADMAP 7); the sample host with a real model (ROADMAP 17); the host integration guide (ROADMAP 8).
Read first: SPEC §10.3, §10.7–10.11, §7.5–7.7, §12.7, DESIGN.md §6.10–6.14 and §7, ROADMAP items 7, 8, 17.
Must-do: 50k-row table scrolls; chart rules from DESIGN §6.11 (zero baseline, null gaps, round ticks); panic-injection test shows the placeholder; sample host streams a live model into the UI with recorded transcripts and mid-stream screenshots; 20 transcripts replay; security checklist §11 reviewed item by item in the report. Open a PR; don't self-merge until reviewed. Do not publish crates or tag v0.1 until the owner completes the pre-publish checklist below.
```
Review checklist: run the sample host yourself (owner) with a real model and try 5 different requests; check mid-stream screenshots look intentional; security checklist has evidence per row.

## Before first publish (owner)

- Pick the name (suggested: **Oxiform**: free on crates.io and GitHub as of 2026-10-21) and reserve: GitHub org, placeholder crate, social handles.
- License: MIT OR Apache-2.0 (Rust norm). Add LICENSE files.
- Ask the agent to: rename crates and `<name>` placeholders; add one umbrella crate re-exporting everything with a `gpui` feature; move `research/` to a private repo; write README (pitch, one-line install, demo GIF); run cargo-semver-checks and `cargo doc -D warnings`.
- Launch plan: research/marketing/launch-channels.md (start the Lobsters account and RustWeek CFP, due Jan 10 2027, early).
