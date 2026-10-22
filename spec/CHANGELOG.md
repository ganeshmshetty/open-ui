# Spec changelog

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
