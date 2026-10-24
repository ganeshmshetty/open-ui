# Generative UI in Rust

A clean-room implementation of [SPEC.md](spec/SPEC.md), using the temporary
`genui` crate prefix. See [PROGRESS.md](PROGRESS.md) for milestone status,
[ROADMAP.md](ROADMAP.md) for future work, and [AGENTS.md](AGENTS.md) for the quality bar.

M3 completes the streaming parser, retained patches and replay; M4 the runtime
(state, actions, queries, refs, events and forms); M5 prompts, feedback and stored
messages; M6 the GPUI renderer skeleton, gallery and streaming dashboard; M7
inputs/forms, focus/keyboard handling, `UiEvent` delivery, accessibility, host
extensions, the form demo and scripted GPUI interaction tests; M8 the data
components (virtualized List/Table, Tabs, chart tooltips, host image loader),
panic containment, the scripted sample host, 20 recorded transcripts and the
50,000-row stress. See PROGRESS.md and `docs/release/v0.1-checklist.md` for the
exact boundary and open owner decisions.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run --locked -p genui-cli -- fixtures
cargo run --locked -p genui-cli -- spec-check
```

`fixtures [directory]` and `spec-check [SPEC.md]` accept optional paths. Default
paths are rooted at the source workspace, independent of the current directory.
`spec-check` validates SPEC examples and all 25 exemplars against genui-std.
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

M6/M7/M8 renderer demos (macOS local verification; the screenshot feature is
M6/M7/M8 renderer demos (macOS local verification; the screenshot feature is
test-support and dev-only):

```sh
cargo run -p genui-gpui --features screenshot --bin gallery -- --list
cargo run -p genui-gpui --features screenshot --bin gallery -- \
  --screenshot /tmp/genui-gallery --mode both
cargo run -p genui-gpui --features screenshot --bin gallery -- --self-test-freshness
cargo run -p genui-gpui --features screenshot --bin gallery -- --self-test-panic
cargo run -p genui-gpui --features screenshot --bin dashboard -- \
  --file examples/01-sales-dashboard.ui --chunk 5 --headless \
  --screenshot /tmp/dashboard.png --patch-log /tmp/patches.jsonl

# M7 form demo: prints ui-events/requests/model events; keyboard-only self-test
cargo run -p genui-gpui --features screenshot --bin form-demo -- \
  --screenshot /tmp/genui-form --mode both
cargo run -p genui-gpui --features screenshot --bin form-demo -- --self-test
cargo test -p genui-gpui --test interaction keyboard_only_run_reaches_submit
```

M8 scripted host, transcripts and table stress:

```sh
cargo run -p genui-cli -- transcripts              # verify 20 exact replays
cargo run -p genui-gpui --features screenshot --bin sample-host -- --list
cargo run -p genui-gpui --features screenshot --bin sample-host -- \
  --transcript eval/transcripts/13-sales-tabs.json --headless \
  --screenshot /tmp/sample-host --record /tmp/sample-host/run.json \
  --freeze-at 3 --restore --regenerate
cargo run -p genui-gpui --features screenshot --bin table-stress -- \
  --rows 50000 --metrics /tmp/table-stress.json --screenshot /tmp/table-stress
cargo run -p genui-gpui --features screenshot --bin list-stress -- \
  --rows 10000 --metrics /tmp/list-stress.json --screenshot /tmp/list-stress
```

The transcripts are deterministic `scripted-fixture` conversations, not
real-model evidence. See [docs/host-guide.md](docs/host-guide.md) for the host
contract (streaming, requests, policy, images, freeze/restore) and
[docs/security-review.md](docs/security-review.md) for the M8 review.

Quality gates (§12.8) run in CI. Mutation testing is an advisory nightly report, run locally
or through **Milestone mutation gate** (manual, or a push whose commit message
contains `[mutation-gate]`). It uses a project-only
copy and never copies research. The core's public syntax APIs and
`genui_core::validate::{validate, validate_message}` are available to applications.

See [conformance/README.md](conformance/README.md), [fuzz/README.md](fuzz/README.md),
and [PINS.md](PINS.md) for verification boundaries and dependency policy.
Third-party sources under `research/openui/` are excluded from Git and builds.
