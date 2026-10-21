# Generative UI in Rust

A clean-room implementation of [SPEC.md](spec/SPEC.md), using the temporary
`genui` crate prefix. See [PROGRESS.md](PROGRESS.md) for milestone status,
[ROADMAP.md](ROADMAP.md) for future work, and [AGENTS.md](AGENTS.md) for the quality bar.

M1 implements the DSL lexer, one shared statement splitter, model-reply extractor,
batch AST parser and canonical printer. Scanners retain token/escape, UTF-8,
held-CR and line-lookahead state across pushes. The parser handles syntax recovery,
name claiming, and syntax limits. Library validation, IR, runtime and GPUI follow
in milestone order.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run --locked -p genui-cli -- fixtures
cargo run --locked -p genui-cli -- spec-check
```

`fixtures [directory]` and `spec-check [SPEC.md]` accept optional paths. Default
paths are rooted at the source workspace, independent of the current directory.
`spec-check` parses every `ui` and `ui-fragment` documentation block and requires
diagnostics for `ui-invalid`. Full standard-library validation arrives in M2.
The other CLI commands in the final architecture are not implemented yet and
fail explicitly. Applications can use `genui_core::syntax::parse_batch`,
`parse_message`, `BatchParser`, and `syntax::printer::print` directly.

See [conformance/README.md](conformance/README.md), [fuzz/README.md](fuzz/README.md),
and [PINS.md](PINS.md) for verification boundaries and dependency policy.
Third-party sources under `research/openui/` are excluded from Git and builds.
