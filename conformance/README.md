# Conformance fixtures

The layout and comparison contract live in SPEC §12. M0 implements only `lex`.

Each lexical fixture has strict JSON metadata (`case.json`), raw bytes
(`input.ui`), ordered tokens (`want.tokens.json`), and a diagnostic multiset
(`want.diag.json`). An optional `stream.json` is an array of strings whose UTF-8
bytes must concatenate exactly to the input. `008-bad-utf8/input.ui` intentionally
contains invalid UTF-8 bytes; do not convert or normalize fixture inputs.

`schemas/` contains JSON Schema 2020-12 definitions, validated by the Rust runner.
Unknown fields, token kinds, diagnostic codes, and unsupported fixture files
fail the suite. M0 diagnostic goldens contain only the four comparison fields
`code`, `stmt`, `path`, `recovery`. Multiplicity matters; diagnostic order does not.
Tokens contain `kind` and `text`: strings are decoded, state names omit `$`,
newlines are normalized, and other tokens retain their spelling. Source spans
are not token golden data. Reserved words keep distinct token kinds; M1's parser
will accept them as plain names in key position.

Run from any directory using the manifest path, or from the repository root:

```sh
cargo run -p genui-cli -- fixtures
cargo run -p genui-cli -- fixtures conformance
cargo test -p genui-conformance
```

Every fixture runs whole-input, byte-by-byte, every two-way split for inputs up
to 2 KiB, 200 deterministic random chunkings (sizes 1–97), LF-delimited lines,
and any explicit stream. The seed is fixed per random run (1 through 200);
chunks are bytes and may split UTF-8 or CR LF. Chunkings are visited one at a time.

**M0 scope:** `LexBuffer` collects bytes and lexes at `finish()`. This establishes
the chunking runner and finish equality only. M1 adds parsing and literal range
and name length enforcement; M2 adds the library golden and validated IR; M3
adds live invariant, statement-hash, patch replay and provisional-effect checks.
No M0 result asserts those later guarantees. Unsupported categories, interactive
fixtures, and nonempty limits overrides fail explicitly. Additional output
schemas will arrive alongside their owning milestones.
