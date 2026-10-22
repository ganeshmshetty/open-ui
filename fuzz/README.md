# Syntax fuzzing

M1 has four live, AddressSanitizer-instrumented cargo-fuzz targets:

| Target | Behavioral checks |
| --- | --- |
| lexer | Batch equals byte-by-byte lexing, including spans and diagnostics. |
| splitter | Batch equals byte-by-byte splitting with varied small statement/depth budgets. |
| extractor | Batch equals byte-by-byte extraction; at most one program segment. |
| parser | Batch equals chunked parsing; printed recovered AST reparses identically with syntax ceilings. |

The AST reparse budget includes `max_bracket_depth=64`: printing a bare component
adds `()`, and the original input may already reach its bracket budget. The
initial fuzz property caught that budget mismatch; reduced fixtures now cover
it, and the original minimized artifact and failure log are retained in
`runs/m1/initial-*`. Valid generated programs also round-trip with default
budgets without diagnostics in the regular test suite.

With Rust nightly and cargo-fuzz installed, from the repository root:

```sh
cargo +nightly fuzz build
cargo +nightly fuzz run lexer -- -max_total_time=600 -max_len=32768 -seed=20261021 -dict=fuzz/syntax.dict -print_final_stats=1
cargo +nightly fuzz run splitter -- -max_total_time=600 -max_len=32768 -seed=20261021 -dict=fuzz/syntax.dict -print_final_stats=1
cargo +nightly fuzz run extractor -- -max_total_time=600 -max_len=32768 -seed=20261021 -dict=fuzz/syntax.dict -print_final_stats=1
cargo +nightly fuzz run parser -- -max_total_time=600 -max_len=32768 -seed=20261021 -dict=fuzz/syntax.dict -print_final_stats=1
```

Acceptance campaigns use fixture-seeded corpora plus statement-size and nesting
boundary inputs, and may run concurrently. Corpus mutations remain local in
`fuzz/corpus/`; preserve behavioral failures as checked-in conformance fixtures.
`runs/m1/*.json` records commands, tool versions, source/log SHA-256 hashes,
execution counts, engine durations and exit codes. Adjacent logs retain startup
and final output; the JSON names the full local logs. Counts from interrupted
runs are not acceptance evidence. See PROGRESS.md for the completion status.

M0's four no-op future fuzz targets were removed. Validator, evaluator, prompt
and renderer targets will be added when their entry points are implemented in
M2, M4, M5 and M6. They are not part of M1's coverage. CI formats and compiles the
four implemented targets; timed campaigns are separately recorded.

M2 adds the live `validator` target. It uses the actual std library, varied static
limits and capabilities, and checks byte-chunk equality, sealed-table permutation
invariance, concrete-tree reachability, unique path/ID identity and node/depth
budgets. It performs no evaluation or host effects.

```sh
cargo +nightly fuzz run validator fuzz/corpus/m2-validator-acceptance -- -max_total_time=600 -max_len=32768 -seed=20261022 -dict=fuzz/syntax.dict -print_final_stats=1
```

The corpus is seeded from all fixture and exemplar DSL inputs. M2 acceptance
metrics, source hashes and retained startup/final log excerpts live in `runs/m2/`.
