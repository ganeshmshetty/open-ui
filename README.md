# Generative UI in Rust

A clean-room implementation of [SPEC.md](spec/SPEC.md), using the temporary
`genui` crate prefix. See [PROGRESS.md](PROGRESS.md) for milestone status,
[ROADMAP.md](ROADMAP.md) for future work, and [AGENTS.md](AGENTS.md) for the quality bar.

M2 adds the checked library builder, standard catalog, static resolver/validator,
recovery ladder, typed IR with path IDs and limits to M1's syntax pipeline.
Dynamic data stays in expression and slot plans until the M4 runtime.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run --locked -p genui-cli -- fixtures
cargo run --locked -p genui-cli -- spec-check
```

`fixtures [directory]` and `spec-check [SPEC.md]` accept optional paths. Default
paths are rooted at the source workspace, independent of the current directory.
`spec-check` validates SPEC examples and all 24 exemplars against genui-std.
`tree <file.ui>` prints the static IR, typed props, identity paths, deferred plans
and diagnostics. It uses the default empty tool/ref catalog, so examples requiring
host data will report missing catalogs. Warning+ diagnostics exit nonzero.

```sh
cargo run --locked -p genui-cli -- tree examples/01-sales-dashboard.ui
python3 -m unittest discover -s scripts -p 'test_*.py'
python3 scripts/quality.py sources
python3 scripts/quality.py dependencies  # cargo-deny 0.20.2
python3 scripts/quality.py fixtures --base main --head HEAD
python3 scripts/mutation.py --output /tmp/genui-mutations  # cargo-mutants 27.1.0
```

Quality gates (§12.8) run in CI. Mutation testing is a milestone gate, run locally
or through the manual **Milestone mutation gate** workflow. It uses a project-only
copy and never copies research. The core's public syntax APIs and
`genui_core::validate::{validate, validate_message}` are available to applications.

See [conformance/README.md](conformance/README.md), [fuzz/README.md](fuzz/README.md),
and [PINS.md](PINS.md) for verification boundaries and dependency policy.
Third-party sources under `research/openui/` are excluded from Git and builds.
