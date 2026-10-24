# Security review — M8 (SPEC §11)

Date: 2026-10-24. Scope: the M8 renderer data components, host policy, image
loader, panic boundary and transcript replay. The parser/validator/runtime
threat controls from M1–M5 are unchanged and remain covered by their fixtures.

Status: **reviewed, with the residual risks listed below.** No unresolved
security defect was found in the changed code after the fixes in this commit.

## Threat checklist

| SPEC §11 threat | Control in the M8 code | Evidence |
| --- | --- | --- |
| Code execution | The language has no code; data components render typed values only. Table/List/Tabs/Chart/Image builders contain no eval or dynamic dispatch from model text. | `table.rs`, `data.rs`, `chart.rs` |
| Heavy computation | Core fuel, node/list/byte limits unchanged. Renderer caps table columns (32), rows (50,000) and cell text (4 KiB); charts cap categories (24). | `table::MAX_TABLE_COLUMNS`, `MAX_TABLE_ROWS`, `MAX_CELL_BYTES`; `chart::MAX_CHART_CATEGORIES` |
| Memory exhaustion | Text props are truncated to 32 KiB; Table rows are parsed once and the duplicate wire prop is dropped from the retained tree; image cache caps bytes, dimensions and source count. | `props::MAX_TEXT_BYTES`; `TableModel` parse/drop test; `image` cap tests |
| Reference blow-up | `preload_ref` applies the same catalog, nesting and byte caps as `ResolveRef` delivery; refs inside `each` expand only after the value is accepted. | `Session::preload_ref`; `preloaded_refs_expand_each_without_a_host_request` |
| ReDoS | No model regexes; formatting is fixed Rust code. | `format.rs` |
| Numeric surprises | Core checked arithmetic; renderer cell/sort values accept only finite numbers and treat invalid values as text/null. | `table::sort_value`, `format::numeric` |
| Exfiltration through images/links | Renderer never fetches; host `image_allowed`/`check_url` require https and an allowlist host, and `open` is denied by default. Model-literal URLs still need the core allowlist. | `host::image_fetches_require_https_and_the_host_allowlist`, `open_urls_require_https_and_an_allowlisted_host` |
| Exfiltration through tools | Sample policy delivers only allowlisted tools/refs and denies everything else; approval is explicit. | `host::allowlisted_refs_are_delivered_and_recorded`, `an_unknown_ref_is_denied_not_invented` |
| Unauthorized tool use | Unknown tools are denied; request loop is bounded at 256; core caps concurrent requests. | `SampleHost::settle` |
| Misleading actions | Only buttons with `on_click` emit activation; the core resolves the real plan and emits the real label. Transcripts show the source as `ui_event`. | `button_element` gating, `View`/`ModelEvent` tests |
| Phishing for secrets | Phase 1 has no secret fields. Sample data contains no credentials; prompts/transcripts are checked by the sample host. | `spec/SPEC.md` Appendix A; `eval/transcripts` |
| Spoofing with bidi/invisible chars | Core strips them from model text before patching; the renderer sanitizes again on every text prop, chart label/tooltip and table header/cell. | `table_cell_text_strips_bidi_and_zero_width_characters`; `props` tests |
| UI redress | No raw colors/fonts/positioning; table and tabs use theme tokens only. `TableColumn` widths are clamped. | theme/table tests |
| Prompt injection through data | Tool/ref values are values, never program text. Table/List render the value; no reparse. | `TableModel`, `HostData` |
| Hallucinated data | Refs must be in the per-turn catalog; unknown refs are denied. | core ref fixtures + host test |
| Render crashes | Builder panic containment, one `render.panic` diagnostic per node revision, quarantine after 3, props-change recovery; diagnostics capped at 64. | `gallery --self-test-panic`, `panics` tests |
| Replay and stale actions | Plans bind to node and epoch; old actions are inert after reset. Transcripts replay through a fresh session. | `Session::activate_at`, `transcripts` gate |

## Findings fixed in this review

1. **Table cells and tooltips bypassed the renderer's text sanitizer.** Cell
   values came from tool/ref data and were formatted directly. They now go
   through `sanitize_text` and are capped at 4 KiB before any widget; headers and
   the first-row fallback do too. Regression test:
   `table_cell_text_strips_bidi_and_zero_width_characters`.
2. **The image source cache was unbounded by source count.** A program with many
   image nodes could grow the cache without limit even under the size cap. It now
   caps at 256 distinct sources and reports `TooManySources`. Regression test:
   `the_source_cache_is_bounded`.
3. **Image delivery accepted bytes until paint-time decode.** `deliver_image`
   validates the byte, dimension and pixel caps and catches `Image::from_bytes`
   panics, turning them into a failed image. Regression test:
   `cache_enforces_byte_and_pixel_caps_without_panicking`.
4. **Renderer panic diagnostics could grow without bound.** `PanicTracker` caps
   retained diagnostics at 64 and reports one per node revision; the quarantine
   test covers the path.
5. **Sample host allowed `open` by default in the first draft.** The demo policy
   is now deny-by-default; only an explicit `allow_open` plus https and an
   allowlisted host passes. Test:
   `open_urls_require_https_and_an_allowlisted_host`.

## Residual risks (documented, not waived)

- GPUI layout/paint runs after the builder; a panic there cannot be reliably
  caught (SPEC §10.8 item 3). Mitigation: clamped values, no model-driven
  geometry, and the headless renderer test.
- Image decoding happens in GPUI's paint path. The loader validates headers and
  caps, but the host must still deliver formats it trusts; a malformed image can
  fail at paint time. The renderer never fetches.
- Table data memory is O(rows): the 50,000-row stress measured 786 MB process RSS
  (54 MB baseline), with only 12 rows built. This is reported in SPEC §10.6; the
  40 MB target applies to 5,000 ordinary nodes, not a 50k-row dataset.
- List is not virtualized in phase 1; the core node/list caps bound it.
- Transcripts are scripted fixtures, not real-model evidence; no prompt-injection
  measurement on a live model has been run in this worktree.
- No password/secret field exists; a host that adds one must not rely on the
  renderer for redaction.
