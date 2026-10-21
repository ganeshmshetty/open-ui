# Spec changelog

## 2026-10-21 — M0 fixture bootstrap (§2.2, §12.1, §12.2, §12.6)

- Define `want.tokens.json` and lexical comparison fields because the original
  layout listed lexical coverage without a token golden format.
- Make the M0 lexical-only `spec-check` and finish-only chunking scope explicit,
  consistent with §14.1: semantic validation needs M2, and live streaming
  invariants need M3. No later milestone's acceptance requirement is relaxed.
- Specify malformed Unicode escape consumption and a final backslash at a line
  or input boundary so error recovery is deterministic; add matching lexical
  fixtures for those previously unspecified cases.
