# Owner decisions

No new owner decision is required for the M2 inspection and quality-gate follow-up.
Defaults and their rationale are recorded below; nothing here authorizes a scope
change or an irreversible action.

| Question | Options | Recommended default | Work meanwhile | Status |
| --- | --- | --- | --- | --- |
| What host catalog should `tree` use? | Empty default policy; silently inherit exemplar tools/refs; add host options | Empty policy, matching the core API; exemplar host context remains explicit in spec-check | Implemented and documented in SPEC §12.7 | Default adopted; owner may revisit |

Project naming, license and publishing remain ROADMAP item 10, due before first
publish. No publishing decision was made in this follow-up.

## Approved GPUI spike decisions — resolved by main 6bca7fa

- Resolved: §10.1 applies patches before the frame with Window available; enqueue
  is fallback only. Containers are uncached; fixed-size expensive subtrees may cache.
- Resolved: §12.7 uses windowed test-support render_to_image for gallery capture;
  headless frame driving is separate.
- Resolved: §6.5 requires a core-owned canonical encoder and preserve_order CI.
  Implementing that approved decision in M3; no golden is changed to conceal it.

## M3 defaults recorded while implementing the contract

| Question | Options | Recommended default / action | Status |
| --- | --- | --- | --- |
| How do M3 patches carry Dynamic props before M4? | Evaluate early; leak arena indices; carry structural deferred expressions | Explicit deferred staging encoding; no evaluator, stable across arena reindexing; G9 applies from M4 | Documented in SPEC §§5.5,6.3 |
| How can canonical map keys be sorted while map entry order survives wire round trips? | Lose entry order; exempt objects from sorting; tagged ordered pairs | Tagged ordered pairs for semantic maps only; metadata remains sorted objects; no tag collisions | Documented in SPEC §6.5 |
| How does a host retract provisional diagnostics? | Append-only stale diagnostics; new removal protocol; authoritative current list | Update carries additions; diagnostics() supplies the current list for provisional replacement | Documented in SPEC §3.7 |
