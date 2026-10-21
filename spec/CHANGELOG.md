# Spec changelog

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
