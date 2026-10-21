# Fuzz scaffolding

M0 provides one live target (`lexer`) and seven compiling placeholders for SPEC
§12.6. The placeholders intentionally do no work and are **not coverage**.
Wire splitter/extractor/parser in M1, validator in M2, evaluator in M4,
prompt generation in M5, and renderer patch application in M6.

From this directory, with cargo-fuzz and Rust nightly installed:

```sh
cargo +nightly fuzz run lexer -- -max_total_time=600
```

The repository's `parser` command becomes meaningful in M1. CI only checks that
all scaffolds build; M0 does not claim a timed fuzzing run. Keep minimized crash
inputs in `conformance/fuzz-regress/` when the relevant runner exists.
