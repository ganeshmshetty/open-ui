# M2 quality follow-up fuzz acceptance

Both targets ran on frozen implementation 9b9f8e4 with AddressSanitizer,
nightly 1.101.0-nightly (2026-10-20) and cargo-fuzz 0.13.2. Each completed 601
engine seconds with exit 0 and matching source hashes. Parser: 95,490 executions;
validator: 44,685. Both vary max_expr_depth from 0 through 48, checking batch/chunk
equality; parser also checks AST round trips and validator checks permutations,
identity/reachability and concrete budgets. Validator uses std plus nullable Probe.

Each corpus began with 301 seeds from fixtures, exemplars, CLI inputs, nullable
Probe programs, low-depth prefixes and a long container-alias chain. Source
manifests, exact commands, timing, memory, exit status and full-log SHA-256 hashes
are in the JSON files. Logs retain startup/final excerpts. Full local logs are
/tmp/genui-m2-quality-parser.log and /tmp/genui-m2-quality-validator.log.

These supersede the earlier M2 run for the contextual-expression-budget change;
earlier acceptance and interrupted/checkpoint campaigns remain unchanged in m2/.
