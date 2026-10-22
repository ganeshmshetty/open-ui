# Syntax / IR mutation evidence (SPEC §12.8)

This directory contains milestone metadata, not conformance cases.

- inventory.json freezes all 609 generated mutant names (cargo-mutants 27.1.0).
- review.json records exact survivor names, reasons and containing-file SHA-256.
- discovery.json records the historical first pass, including all 49 survivors.
  It is not acceptance: the parser budget and regression suite changed afterward,
  and the 30-second test timeout was too tight under concurrent workloads.
- The final CI gate runs four disjoint shards with a 60-second test timeout and
  all workspace tests. Aggregation rejects missing/duplicate mutants, failed
  baselines, incomplete runs, unreviewed survivors and stale source manifests.
  Build timeouts fail; test timeouts are killed mutants under cargo-mutants' model.
- Final acceptance.json is downloaded from the completed CI aggregation artifact.
  Full per-shard outcomes, diffs and logs remain in that run's artifacts.

The two Span.end mutants are equivalent because emit overwrites the temporary
field. Negating the private namespace tag bijectively renames its two classes,
without changing name-set membership. No production fix is needed for these.
