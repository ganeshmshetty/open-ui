# Roadmap: work not yet in the spec

Each item has a "due by" milestone. When starting a milestone, pick up every item due by it: write it into the spec (with a `spec/CHANGELOG.md` line) or as its own doc, then implement.

| # | Item | Due by | Output |
| --- | --- | --- | --- |
| 1 | Design language: tokens, layout rules, component anatomy and states, streaming visuals | M6 (start) | `spec/DESIGN.md` (draft 0.1 done; style-guide demo window due M6) |
| 2 | Exemplar programs: 20–30 hand-written ideal DSL programs (dashboards, forms, lists, data by ref) used as prompt examples, fixtures and GPUI demos | M2 | `examples/*.ui`, checked by `spec-check` |
| 3 | Prompt text: the actual wording of sections S1–S11 (§9.1) | M5 | `crates/*-prompt` templates + golden |
| 4 | Model evaluation set: tasks, scoring (valid-program rate, diagnostics per program, tokens vs JSON baseline), recorded transcripts; answers open questions 2 and 3 | M5 | `eval/` + report |
| 5 | Developer inspector: tree view, patch log, diagnostics list (debug-only view in `-gpui`) | M6 | spec §10 addition |
| 6 | Renderer-side extension API: how an app registers GPUI drawing for a custom component | M7 | spec §10 addition |
| 7 | Image loader contract: host fetch API, caching, size/time limits, pending and failed display | M8 | spec §10 addition |
| 8 | Host integration guide: model streaming → Session → UiView, requests, approval policy, tool calling | M8 | `docs/host-guide.md` |
| 9 | Platform matrix: macOS/Linux/Windows support, MSRV, CI targets | M6 | spec §1.2 addition |
| 10 | Project name, license (MIT/Apache-2.0 dual suggested), README positioning, crate release policy, cargo-deny/audit | before first publish | owner decision + docs |
| 11 | Contributor docs (CONTRIBUTING, code of conduct) | before going public | docs |
| 12 | `genui-cli tree` command (SPEC §12.7) | M2 | CLI + tests |
| 13 | `genui-cli replay` command (SPEC §12.7) | M3 | CLI + tests |
| 14 | GPUI spike including the screenshot pipeline: open a window, render a hard-coded patch list, save a PNG headlessly or via the OS (AGENTS.md spike) | right after M2 | `spike/gpui` branch + findings in PROGRESS.md |
| 15 | Gallery app with `--screenshot` and a component state sheet; screenshot review at the end of M6, M7 and M8 (SPEC §12.7) | M6 | demo binary in `-gpui` |
| 16 | Scripted GPUI interaction tests (SPEC §12.7) | M7 | tests in `-gpui` |
| 17 | Real-model loop: live stream, recorded transcripts, deterministic replay, mid-stream screenshots (SPEC §12.7) | M8 | sample host + `eval/transcripts/` |

Deferred features (edit mode, write tools, secret fields, more components, other renderers) are in SPEC Appendix A, not here.
