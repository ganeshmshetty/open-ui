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
