# Generative UI in Rust

A clean-room implementation of [SPEC.md](spec/SPEC.md), using the temporary
`genui` crate prefix. See [PROGRESS.md](PROGRESS.md) for milestone status and
[AGENTS.md](AGENTS.md) for contribution rules.

M0 provides a Rust stable / edition 2024 workspace, lexical conformance fixtures,
a deterministic byte-chunking runner, lexical specification checks, and fuzz
scaffolds. The parser, library, runtime, and renderer follow in milestone order.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run --locked -p genui-cli -- fixtures
cargo run --locked -p genui-cli -- spec-check
```

`fixtures [directory]` and `spec-check [SPEC.md]` accept optional paths. Default
paths are rooted at the source workspace, independent of the current directory.
`spec-check` currently checks lexical correctness of `ui` and `ui-fragment`
blocks, and recognizes `ui-invalid` blocks; standard-library validation arrives
in M2. Unsupported CLI commands fail with a nonzero exit code.

See [conformance/README.md](conformance/README.md), [fuzz/README.md](fuzz/README.md),
and [PINS.md](PINS.md) for the scaffolds' boundaries and dependency policy.
Third-party sources under `research/openui/` are excluded from Git and builds.
