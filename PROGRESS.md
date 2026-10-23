# Progress

Updated: 2026-10-23. Milestones follow SPEC §14.1 in order. Update this file after
**every milestone**, recording acceptance evidence and any remaining limitations.
A milestone is complete only after its acceptance checks pass and CI is green.

| Milestone | Status | Evidence / next gate |
| --- | --- | --- |
| M0 — Workspace and verification scaffolding | Complete | 21 lex fixtures / 4,743 chunkings; 7 spec examples; Linux and macOS CI green. |
| M1 — Batch syntax | Complete | 124 fixtures / 28,225 chunkings; 23 tests including 2,048 generated round trips; four clean 600-second fuzz campaigns; Linux/macOS CI green. |
| M2 — Library, validation, IR and limits | Complete, including tree and quality gates | 295 fixtures / 66,772 chunkings / 400 permutations; 66 Rust + 10 gate tests; 8 CLI fixtures; 24 exemplars; renewed parser/validator ASan 601s each; 609 mutations audited; Linux/macOS/quality and mutation CI green. |
| M3 — Streaming and patches | Complete (mutation advisory) | CI green; fresh 601 s fuzz; 100k property; fixtures/replay. Mutation survivors recorded as backlog per the 2026-10-24 gate change. |
| M4 — Runtime | In progress on milestone/m4 (slice 2) | Slice 1 evaluator/state/actions; slice 2 queries, refs, events, forms and provenance: 24 new runtime fixtures, 404 fixtures / 96,248 chunkings. `each` instantiation and status props remain. |
| M5 — Prompt, feedback and messages | Not started | Golden output and cross-platform determinism. |
| M6 — GPUI skeleton | Not started | Live streaming demo and recorded performance. |
| M7 — Inputs and forms | Not started | Stable editing, keyboard form demo and accessibility. |
| M8 — Data components and sample host | Not started | Table stress, panic containment, transcript replay, security review, v0.1 tag. |

## M0 scope

- Six crate scaffolds (§1.2), Rust stable / edition 2024, UI-free core with unsafe
  code forbidden, checked-in Cargo lockfiles, exact dormant GPUI pins.
- Lexical fixtures written before implementation, strict fixture schemas,
  diagnostic multiset and ordered-token comparisons, seeded byte chunkings.
- `fixtures` and lexical-only `spec-check` CLI commands; later commands explicitly
  unsupported. Actual semantic and live-stream guarantees remain future work.
- Live lexer fuzz target plus seven compiling placeholders; no timed fuzz run
  claimed. Linux and macOS CI runs format, Clippy, tests, fixtures, spec examples,
  and fuzz scaffold checks.
- Third-party `research/openui/` sources were not read and are ignored by Git.

## Verification

Local gates passed:

- `cargo fmt --all -- --check` and fuzz scaffold formatting.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- `cargo test --workspace --locked`: 13 tests, including checked-in conformance.
- `cargo run --locked -p genui-cli -- fixtures`: 21 fixtures, 4,743 chunkings.
- `cargo run --locked -p genui-cli -- spec-check`: 3 complete programs and 4
  fragments, zero lexical diagnostics.
- `cargo check --manifest-path fuzz/Cargo.toml --bins`: all 8 scaffolds compile.
- `cargo tree -p genui-core --edges normal`: only serde and its derive dependencies.

Hosted CI: [M0 acceptance run](https://github.com/ganeshmshetty/open-ui/actions/runs/39634933301)
passed on Linux and macOS for implementation commit `69e959f`. All eight workflow
checks passed on both platforms. The repository is private.

Additional negative checks confirmed that changed token/diagnostic goldens,
unknown metadata fields, and an explicit stream that does not reproduce its
input all exit nonzero.

M0 acceptance evidence above is historical. Current work is recorded below.

## M1 — Batch syntax

Read updated AGENTS.md (including Quality bar), ROADMAP.md and the spec changelog.
No roadmap item is due in M1. Visual design, conversations, Stat/Facts and runtime
changes remain with their owning milestones. Third-party source was not read.

Built:

- Resumable UTF-8/BOM/CR normalization and lexical state (growing names/numbers,
  comments, operators, strings, escapes), token spans and literal bounds.
- Exactly one splitter, shared by batch and push, with token-based statement
  starts, one-line lookahead, resync, closers, bracket/statement budgets and
  immutable sealed statements. Only an oversized retained prefix is re-lexed
  for recovery; no second boundary implementation exists.
- Resumable fenced/unfenced model-message extraction, prose segments, version
  rejection and extra-program handling.
- Structural AST, precedence parser, structural recoveries, duplicate/reserved
  checks and name claiming, query/state forms, canonical printer.
- Strict token/AST/segment/diagnostic fixture schemas; all syntax categories run
  every §12.3 chunking. Documentation examples now parse, with negative checks
  proving lexically valid syntax errors fail and `ui-invalid` needs diagnostics.
- Four live fuzz targets; M0's no-op future targets were removed.

Fixtures were written before their implementation. No M0 token/diagnostic golden
was changed. Later fixtures added regression coverage before the relevant fix.

Local acceptance evidence:

| Category | Fixtures |
| --- | ---: |
| lex | 31 |
| grammar | 33 |
| statements | 32 |
| extract | 26 |
| fuzz-regress | 2 |
| Total | 124 |

- 28,225 chunkings pass, comparing exact final scan/AST/segment results and
  diagnostic multisets to batch. Every parsed fixture also round-trips.
- 23 Rust tests pass, including a 2,048-case generated-program property that
  requires AST equality and zero diagnostics using default budgets.
- `spec-check`: 3 complete programs and 4 fragments parse without diagnostics.
  Synthetic tests exercise accepting and rejecting `ui-invalid` blocks.
- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --workspace`, fuzz formatting and all four fuzz-target builds pass.

Fuzz acceptance: all four campaigns completed on implementation commit
`8586ba9` with AddressSanitizer, seed `20261021`, max input length 32,768 bytes,
and fixture/boundary-seeded corpora. Runs were concurrent. Source and full-log
SHA-256 hashes were checked against the final files; all match.

| Target | Engine seconds | Executions | Exit code |
| --- | ---: | ---: | ---: |
| lexer | 601 | 1,239,855 | 0 |
| splitter | 601 | 179,892 | 0 |
| extractor | 601 | 571,567 | 0 |
| parser | 601 | 293,047 | 0 |

Commands, tool versions and startup/final excerpts are checked in under
`fuzz/runs/m1/`. Rust nightly was `1.101.0-nightly (19833af72 2026-10-20)`;
cargo-fuzz was `0.13.2`. Engine durations exclude compilation/setup time.

Early 20-second smoke runs also passed. The first fixture-seeded parser campaign found a
round-trip **test-budget mismatch**, rather than a model-input panic: printing
an implicit bare component adds parentheses at the bracket ceiling. The original
minimized artifact/log is in `fuzz/runs/m1/initial-*`; two checked-in regressions
cover the reduced behavior. The canonical-output parse now uses syntax ceilings,
and all four timed campaigns were restarted after final scanner corrections.
Interrupted campaigns are not counted toward acceptance.

Spec changes (same commit as fixtures) are fully listed in spec/CHANGELOG.md:
lexical bound recoveries; RHS/name-claim/namespace/count rules; forced-boundary
recovery cardinality and size/depth/string behavior; fence/version/prose edge
cases; strict AST/segment goldens; M1 spec-check and round-trip budget rules.

The first hosted run found one Rust 1.99 Clippy style lint (`while_let_loop`)
that Rust 1.94 did not report. The equivalent loop was rewritten without a
behavior change; the completed campaigns use that final source.

Hosted acceptance CI passed on Linux and macOS for `8586ba9`:
[CI run](https://github.com/ganeshmshetty/open-ui/actions/runs/39649523210).
All eight checks passed on both platforms. Final evidence/documentation is
committed separately and receives the same CI workflow.

M1 is complete. No known unresolved syntax issue. Semantic library resolution,
constant-state validation, general builtin/type checking, IR, provisional tails,
statement hashes and patch invariants remain M2–M4 work. The dormant IR/runtime/
library/GPUI scaffolds are explicitly outside M1. Next milestone: M2.

## M2 — work in progress

Approved M1 was fast-forwarded into main and pushed (38d8912). Work continues on
milestone/m2 from main. Re-read AGENTS.md, PROGRESS.md, ROADMAP.md, CHANGELOG and
the requested syntax, diagnostic, IR, library and conformance sections.
ROADMAP item 2 is due: 20–30 complete exemplar programs and full spec-check.

### Open issue for M3 (M1 review)

`Splitter::finish()` suppresses lexer diagnostics (`BadChar`, `BadEscape`,
`UnterminatedString`, `Overflow`, `NonFinite`) inside size-cutoff skipped ranges.
In streaming, push would already have emitted them. M3 must hold them provisional
until the cutoff decision, or stream != batch. Preserve a regression test when
adding live diagnostic emission; M2 must not claim this is solved.

M2 evaluation is static only. State reads, query results, refs and expressions
requiring evaluation retain typed Dynamic plans. No evaluator is implemented.
Acceptance evidence and diagnostic coverage/deferred-code audit will be added
before completion. Unrelated research/marketing files remain untouched.

### M2 implementation checkpoint — 2026-10-22

Fixtures were authored before implementation (initial 62 cases, then focused
regressions before each affected change). Current totals: 256 fixtures, including
132 new M2 cases; 55,802 byte chunkings and 360 seeded sealed-table permutations.
All five M2 categories pass: library 20, resolve 17, schema 61, ir 15, limits 19.
The 49 Rust tests include 2,048 AST round-trip cases and two 512-case semantic
properties (permutation invariance and path stability across append/recovery).

Built the checked builder, additive extension guards, complete 29-entry std
catalog (including Stat/Facts), canonical Library Spec and independent xxh3-128
hash golden, static resolver/validator, bottom-up recovery, typed values/props,
path allocator with collision rehash, state/query declarations, action/form plans,
capability fallbacks, node-template plans, and bounded static limits. Deferred
plans retain their node definitions and lambda environments. Only concrete
instances consume node/depth budgets or report path cycles; M4 instantiates data.

Full spec-check passes: 3 complete SPEC programs, 4 fragments and all 24 complete
exemplars. ROADMAP item 2 is done. Every std component example validates with no
Warning+ diagnostics. Golden hash: d19a769bd510997ce786ade4d60db23d,
independently calculated with Python xxhash, then compared byte for byte by Rust.

The audited 69-code table has 59 codes covered by fixtures and 10 explicit
execution/replay/renderer deferrals; see conformance/diagnostics.json. Conditional
recovery variants are also audited. New spec clarifications and their reasons
are listed in spec/CHANGELOG.md. There is no evaluator, host request dispatch,
query/ref delivery, live patch stream or renderer in this milestone.

Local fmt, Rust 1.99 Clippy, workspace tests, fixtures, spec-check and stable fuzz
build pass. A 21-second ASan smoke run completed 36,562 executions with exit 0;
that smoke predates final template/slot hardening and is not acceptance evidence.
The frozen-source 600-second validator campaign and hosted Linux/macOS CI are
next. M2 remains in progress until both succeed and final evidence is committed.


### M2 nullable-type regression checkpoint — 2026-10-22

The first implementation CI passed on both Linux and macOS for eb8250b:
https://github.com/ganeshmshetty/open-ui/actions/runs/39705728214 .
Supplemental exact fixtures exposed missing transparency through nullable
wrappers (enum codes, token encoding/range, bindings, actions and node slots).
Those cases were authored and observed failing before the fix; required nullable
and recovered-null behavior is now explicit in SPEC §§4.4,8.3 and CHANGELOG.
Expression interning also preserves recovery metadata despite structural AST
round-trip equality excluding it.

The timed campaign on eb8250b was intentionally interrupted after 415,105
executions (libFuzzer exit 72; no observed crash). It does not count as acceptance.
Retained evidence is in fuzz/runs/m2/interrupted-validator.* . The validator target
now includes a builder-defined Probe component covering nullable compositions.
The corrected source will receive a fresh full 600-second campaign.

Current local acceptance checks pass: 275 fixtures / 62,343 chunkings /
390 permutations, 50 Rust tests, all spec blocks and 24 exemplars. Library has
36 cases, resolve 17, schema 61, ir 15, limits 22. Extra cases cover additive
extension and Facts at 50, at 51, and with a composed global limit. This remains
M2 only; the M3 splitter issue above is still open.


### M2 formatter-plan checkpoint — 2026-10-22

CI passed on Linux and macOS for nullable correction 3144164:
https://github.com/ganeshmshetty/open-ui/actions/runs/39707471805 . Its ASan campaign
completed 601 engine seconds, 448,712 executions, exit 0, with matching frozen
source hashes and 883 MiB peak RSS. Evidence is retained as
fuzz/runs/m2/nullable-checkpoint-validator.* .

A final exact fallback fixture then caught literal chart data being printed as
JSON instead of retaining the required table formatter. Data/float fallback text
now stays Dynamic with its original node/props until M4, as required by the
static-only scope. A separate spec-check regression now checks fence versions
through the shared extractor. The final-source campaign restarts after this fix;
the earlier clean run is checkpoint evidence, not final acceptance.

The same static-only check now leaves non-literal list/map constructors as
Dynamic plans instead of folding referenced data. A focused fixture and a
1,200-statement container-chain test prevent eager evaluation and unbounded
cached type growth. Final local totals are 277 fixtures, 62,884 chunkings,
400 permutations and 52 Rust tests; the corrected source is ready for its final
frozen campaign. No evaluator or additional milestone work was added.


### M2 completion — 2026-10-22

Final source 2bd18eb passed a fresh 601-engine-second validator campaign:
71,760 executions, exit 0, 884 MiB peak RSS, with every recorded source SHA-256
matching the frozen implementation. The harness uses std plus a builder-defined
nullable Probe, varied budgets/capabilities, chunk equality, sealed-table
permutations, path uniqueness and concrete-tree budget/reachability assertions.
Its 282 initial seeds include a 1,200-statement container chain. Final evidence:
`fuzz/runs/m2/validator.json` and `validator.log`.

All local gates pass: fmt, Clippy (Rust 1.94 and 1.99), workspace tests, all
277 fixtures / 62,884 chunkings / 400 seeded permutations, full spec-check on
3 complete SPEC programs + 4 fragments + 24 exemplars, and fuzz target format/
stable builds. There are 52 Rust tests including 2,048 AST round trips and two
512-case semantic properties. Library 36, resolve 18, schema 62, ir 15, limits 22.
The audit covers 59 codes and explicitly defers 10 to M4/M5/M6.

Linux/macOS source CI for 2bd18eb is green:
https://github.com/ganeshmshetty/open-ui/actions/runs/39709952954 .
The final evidence/report commit receives the same workflow. Complete scope,
counts, code-by-code deferrals, every spec change and remaining limitations are
in `docs/milestones/M2.md`. M2 implementation commits cite the affected sections.
The report and final evidence are committed after acceptance.

M2 is complete. Next is M3; no M3 implementation was started. Its splitter
provisional-diagnostic issue above remains open. Runtime behavior, ref/tool
handling, field validation, event execution and resolved output remain M4;
message/prompt/replay work remains M5, and the renderer remains M6–M8.
Unrelated research/marketing files remain untouched.


### M2 follow-up — main integration and inspection gates

Merging local main (91e6a79), which adds SPEC §§12.7–12.8, ROADMAP items
12–18 and the decisions/report/self-merge rules. The changelog conflict keeps
both branches' entries. M2 is reopened for the required `genui-cli tree` command;
quality gates 1–7 follow immediately, before any GPUI spike. No M3 or spike work
is started. Existing M2 fuzz evidence remains attached to unchanged core source.
Next: exact CLI output fixtures first, implement tree inspection, then add and
exercise the seven quality gates, record evidence and update the milestone report.

### M2 tree / quality-gate implementation checkpoint

Merged main in 5114f8a (changelog retains both histories). Re-read AGENTS.md,
ROADMAP and SPEC §§12.7–12.8. Added DECISIONS.md with the reversible empty-host
policy default for tree; no owner decision blocks progress. The command now
prints kinds, typed props, §5.2 paths, deferred plans and exact diagnostic summaries.
Seven CLI output fixtures were authored before the implementation/extension;
argument/I/O tests also pass. The initial repeated-reference expectation was
corrected to the existing `~2` rule and documented in SPEC/CHANGELOG.

Quality gates 1–6 are implemented: non-test core Clippy denies, cargo-deny on the
transitive core graph, source checks for ignores/styles, per-commit fixture
protection, and the covered/deferred audit (59 covered + 10 explicit deferrals).
Seven Python tests include actual negative cargo-deny/Clippy/history probes.
The manual mutation workflow uses a project-only copy, never research.

The first full 609-mutant syntax/IR campaign is running against frozen source.
Early survivors revealed assertion gaps in nested null-recovery metadata and
two extractor APIs; focused tests now cover these without production changes.
Two exact fence-info extraction fixtures cover another survivor. All additions
retain the §12.3 chunk harness. Mutation survivors will be rerun and individually
fixed or justified before completion; no acceptance claim is made yet.

M2 core behavior is unchanged; the existing 601-second validator evidence still
applies to the implementation. Syntax source changes so far add test modules,
and the core root adds deny attributes. M3's provisional-diagnostic issue remains
open. No GPUI spike or streaming/runtime work is started.

### Quality-gate regression checkpoint

Checkpoint 6b51caf passed all three hosted jobs (Linux, macOS and quality):
https://github.com/ganeshmshetty/open-ui/actions/runs/39712881640 .
Expanded exact EOF fixtures and source-span assertions now pass locally:
283 conformance fixtures / 64,174 chunkings / 400 permutations, seven CLI output
cases, 58 Rust tests and eight Python gate tests. Core dependency checking now
also includes dev edges and the project renderer crate. Mutation review rejects
incomplete campaigns, failed baselines, unreviewed survivors and stale reasons.

The full 609-mutant campaign is still running. Three observed equivalents are
justified in conformance/mutations/review.json with source hashes: two temporary
Span.end edits are overwritten by emit; complementing the boolean namespace tag
preserves every name-set membership result. Other observed survivors receive
regressions and will be rerun. This is checkpoint evidence, not mutation acceptance.

Additional mutation regression: statements/033-expression-skip-delimiters now
asserts the existing depth-cutoff rule on nested calls/lists/maps while retaining
outer siblings and exactly one limit.depth. No parser implementation change.

### Mutation-driven expression-depth correction (SPEC §2.5)

The binary-depth survivor exposed a real budget issue: growing binary/postfix
prefix checks ignored enclosing expression depth. New statements/034–035 were
authored first; 034 failed with a nested multiply instead of the required bounded
prefix. Both checks now include enclosing depth. SPEC/CHANGELOG clarify the
remaining-budget rule and existing cutoff-null sentinel. A focused retained-depth
property checks mixed expressions across limits 0–8, and statements/036 checks
that parentheses consume recursive syntax depth despite being absent from ASTs.

The original mutation campaign remains checkpoint evidence. All parser mutants
will be rerun on the corrected implementation, along with remaining survivors.
Parser and validator fuzz targets now vary expression-depth limits, and both will
receive fresh ten-minute frozen-source campaigns before final acceptance.

The contextual-budget fix passes all local gates: 287 conformance fixtures /
65,047 chunkings / 400 permutations, eight CLI output cases, 59 Rust tests,
eight Python gate tests, full spec-check and fuzz target builds. The corrected
source will be frozen for renewed campaigns; M2 follow-up remains in progress.

### Renewed fuzz acceptance — 9b9f8e4

Parser and validator each completed 601 engine seconds under AddressSanitizer:
95,490 and 44,685 executions respectively, exit 0, with matching frozen source
hashes. Both now vary max_expr_depth from 0 through 48. Evidence is retained in
fuzz/runs/m2-quality/. The isolated parser-only mutation restart was intentionally
interrupted while the discovery pass completed; it is not acceptance evidence.
The final acceptance campaign will run every syntax/IR mutant in CI against the
completed regression suite, with survivor rationales checked by the gate script.

### Final quality acceptance candidate

Discovery finished: 609 mutants / 428 caught / 75 unviable / 57 timeouts /
49 survivors, exit 3. It is explicitly non-acceptance evidence in
conformance/mutations/discovery.json. The regression suite addresses 46 observed
survivors; the remaining three have source-matched equivalence reasons. All
timeouts and survivors are rerun on the final candidate, without carrying forward
any old outcome. The final CI gate uses four disjoint shards and a frozen
609-name inventory, and aggregation checks completeness, source hashes and review.

Current local gates pass: 290 conformance fixtures / 65,690 chunkings /
400 permutations, eight CLI output cases, 59 Rust tests, ten Python gate tests,
full spec-check, fmt, Clippy, dependency/source/history checks and fuzz builds.
Renewed parser/validator fuzz acceptance is recorded above. The final CI mutation
result, final review and self-merge remain pending. No spike/M3 work is started.

### Final CI survivor audit checkpoint

The first complete four-shard CI campaign (39716792722) finished with
499 caught, 75 unviable, 9 test timeouts and 26 survivors. The gate correctly
failed for 23 unreviewed survivors; three were already justified equivalents.
Regular CI 39716792529 remains green. Added four fixture-first container/index
budget cases and direct printer/header/splitter regressions: 294 conformance
fixtures / 66,555 chunkings / 400 permutations. The survivor recheck is running.
No production implementation changed in this checkpoint; only test/spec/report
sources changed, so the existing 601-second fuzz runs cover the same executable
parser/validator implementation. Further equivalent reviews require exact reasons
and a complete fresh CI campaign before self-merge.

The expanded suite now has 65 Rust tests, including exact early-lookahead
release assertions. The additional endpoint comparison survivor is equivalent:
filtered lexer errors cannot start at EOF or at the valid Name/State token that
ends a skipped range. Its source-matched rationale is in review.json; the other
three known equivalents remain unchanged. No batch diagnostic is suppressed by
this justification beyond existing behavior. All 609 mutant names are unchanged.

The local survivor-focused campaign caught the parser/printer/header/token
regressions. Four remaining assertion gaps (three early-lookahead timing checks
and a size cutoff inside a token) received exact tests. The full final CI run
will recheck all 609 mutations; no focused result is substituted for acceptance.

### Final review — e3a9b8f

Reviewed the tree traversal/typed prop formatting, explicit default host policy,
operational vs diagnostic exit paths, fixture/deferred audits, mutation inventory
and source-matched rationale checks, and the production-vs-test-only delta after
the accepted fuzz runs. No implementation review blocker remains. The open gate
is the fresh four-shard mutation CI 39735047609; normal CI 39735047595 passes
Linux, macOS and quality. Tree smoke testing examples/02-operations.ui produces
its Stat grid and Facts panel with zero diagnostics. Main is not self-merged
until all mutation shards and aggregation pass.

### Second full-CI survivor checkpoint

Run 39735047609 shard 2 passes (117 caught, 35 unviable, one test timeout).
Shard 3 catches all prior unreviewed survivors but exposes one new survivor:
deleting Add/Sub printer precedence, which had timed out in the first run.
A fixture-first grouped expression and 338 deterministic operator-pair round
trips now assert this rule; the baseline passes. The other timeout cases receive
a focused 120-second recheck before restarting acceptance. Production code is
unchanged. SPEC §12.8 and the gate now use eight disjoint shards for the same
609 mutants and full workspace tests, reducing wall-clock latency without
weakening completeness or review checks. Completion/self-merge remain pending.

Current passing suite: 295 fixtures / 66,772 chunkings / 400 permutations,
66 Rust tests, 10 Python gate tests and eight CLI text fixtures.

The focused 120-second campaign completed: 13 mutants, four caught (including
the additive printer rule and three span-field cases), nine test timeouts, zero
survivors, successful baseline. It is supplementary evidence only, retained in
conformance/mutations/timeout-recheck.json; a new full eight-shard CI campaign
is required for acceptance. All production parser/validator code remains frozen
at 9b9f8e4.

## Consolidated M2 and quality-gate completion report

The historical checkpoints above are retained. This report supersedes their
pending states and counts. ROADMAP items 2, 12 and 18 are done; no spike or M3
implementation is included. milestone/m2 was fast-forwarded locally into main at 8a5d9e8 after all candidate
gates passed. The evidence commit and final main push/CI check follow.


M2 and the immediate quality gates pass. Linux, macOS, quality and complete
mutation CI are green on 8a5d9e8: 609 mutants, 521 caught, 75 unviable, 9 test timeouts, 4 reviewed equivalents.
No review blocker remains. milestone/m2 was fast-forwarded locally into main
at 8a5d9e8 after every candidate gate passed; this evidence commit records the
completion before the final main push/CI check.

M1 was fast-forwarded into main before M2. Local main's new §12.7–12.8 rules
were merged into milestone/m2 in 5114f8a. The follow-up implements tree inspection
and the post-M2 quality gates; no spike, streaming, evaluator or renderer work
was started. Core implementation is frozen at 9b9f8e4, with subsequent fixture,
gate and evidence commits. Renewed parser and validator ASan campaigns each ran
601 engine seconds: 95,490 and 44,685 executions, exit 0, matching source hashes at run completion. Later changes only add cfg(test) assertions and fixtures; executable parser/validator code is unchanged.

Built the checked library builder and additive extension model, all 29 std catalog
entries (27 components plus TableColumn/Tab), canonical Library Spec JSON and its
xxh3-128 hash, static resolver/validator and recovery ladder, typed values/props,
state/query declarations, inert typed action/form plans, capability fallbacks,
node-template plans, canonical limits and path-based identity. String comparison
and salted rehash handle injected xxh3 collisions. No evaluator, query/ref delivery,
request dispatcher, streaming patch engine or renderer was added.

The std golden is `conformance/libs/std.json`; its independent Python xxhash
result is `d19a769bd510997ce786ade4d60db23d`. Rust checks both the hash and exact
pretty JSON bytes. Every component example validates with zero Warning+ diagnostics.

| Category | Fixtures | Byte chunkings | Seeded statement permutations |
| --- | ---: | ---: | ---: |
| library | 36 | 4,535 | — |
| resolve | 18 | 4,292 | 180 |
| schema | 62 | 14,875 | — |
| ir | 15 | 3,082 | — |
| limits | 22 | 7,875 | 220 |
| lex | 35 | 7,975 | — |
| grammar | 37 | 8,383 | — |
| statements | 40 | 8,943 | — |
| extract | 28 | 6,326 | — |
| fuzz-regress | 2 | 486 | — |
| **Total** | **295** | **66,772** | **400** |

There are 171 conformance cases added since M1: 153 original M2 cases and 18
follow-up syntax regressions. Eight additional CLI text fixtures assert tree
output and exit status; they are separate from the conformance counts. Builder/allocator-only cases have direct exact
checks; every case with DSL input runs the byte chunkings. The Library Spec
byte golden is checked in addition to these directory case counts.

| Rust test area | Tests |
| --- | ---: |
| Core syntax | 22 |
| Core resolver/validator/library integration | 14 |
| Core identity | 2 |
| Core values | 3 |
| Core limit ceilings | 2 |
| Std catalog/plans/forms/capabilities | 5 |
| Conformance, schema audits and spec-check | 15 |
| CLI unit/integration tests | 3 |
| **Total** | **66** |

Properties include 2,048 AST round trips, 512 generated semantic permutations,
512 append/recovery path checks, and the 400 fixture permutations above.
A further 72 mixed-expression/limit combinations check enclosing depth budgets,
and 338 deterministic grouped operator pairs check printer precedence.
Ten Python gate tests include negative dependency, lint, history, review and
shard-aggregation probes. The static-vs-dynamic boundary is asserted: custom function callbacks are never
called, and a 1,200-statement container chain stays a bounded expression plan.
Recovery metadata survives expression sharing while remaining outside structural
AST equality, JSON and printing.

`spec-check` passes all 3 complete SPEC programs, 4 fragments and 24 complete
exemplar programs. There are no actual ui-invalid blocks in the current SPEC;
unit tests prove that valid ui-invalid bodies fail the gate and invalid bodies
or unsupported versions produce diagnostics. Version info uses the shared
protocol extractor. ROADMAP item 2 is done.

`genui-cli tree <file.ui>` prints concrete nodes in validated slot order, kinds,
typed props and §5.2 paths. It shows deferred slot/root plans without evaluating
them and prints diagnostic severity/code/recovery/statement/path. Info-only
programs succeed; Warning+ diagnostics fail. The default host catalog is empty,
matching the core API; DECISIONS.md records this adopted reversible default.

All seven quality gates pass: non-test core Clippy denies, cargo-deny
across core's transitive all-feature/all-target graph including dev edges,
ignore/style scanning, per-commit golden protection, covered/deferred audits,
and milestone mutation testing. Eight CI shards must cover the frozen inventory
exactly once, pass their baselines, match current source hashes and justify every
survivor. A source-matched review file records the four known equivalents.

All 69 diagnostic codes are audited. The following 59 have exact fixtures;
`conformance/diagnostics.json` maps each to its cases. Conditional static recovery
variants (including required defaults, enum/type omission/drop and unresolved
slot/value contexts) are separately asserted.

| Family | Covered codes |
| --- | --- |
| cap | `cap.fallback` |
| expr | `expr.fmt_args`, `expr.lambda_misuse`, `expr.non_finite`, `expr.overflow`, `expr.tainted_url`, `expr.type`, `expr.unknown_fn` |
| extract | `extract.extra_program`, `extract.version` |
| limit | `limit.depth`, `limit.ir_depth`, `limit.list_len`, `limit.nodes`, `limit.program_size`, `limit.state`, `limit.stmt_count`, `limit.stmt_size`, `limit.string` |
| name | `name.cycle`, `name.duplicate`, `name.no_root`, `name.orphan`, `name.reserved`, `name.unresolved` |
| query | `query.too_many`, `query.unknown_tool` |
| ref | `ref.invalid`, `ref.unknown` |
| schema | `schema.a11y_missing`, `schema.bad_key`, `schema.bind_target`, `schema.dup_arg`, `schema.dup_key`, `schema.enum`, `schema.extra_positional`, `schema.min_items`, `schema.missing_required`, `schema.positional_after_named`, `schema.range`, `schema.required_by_name`, `schema.slot_kind`, `schema.type`, `schema.unknown_arg`, `schema.unknown_component` |
| state | `state.const_default`, `state.undeclared` |
| syntax | `syntax.bad_char`, `syntax.bad_escape`, `syntax.bad_utf8`, `syntax.bare_component`, `syntax.dup_key`, `syntax.missing_comma`, `syntax.stmt_shape`, `syntax.stray_closer`, `syntax.trailing`, `syntax.trailing_operator`, `syntax.unclosed_bracket`, `syntax.unterminated_string` |

The remaining 10 codes cannot be emitted by the implemented static phase;
`conformance/deferred.json` records their milestone and reason:

| Code | Milestone | Reason |
| --- | --- | --- |
| `expr.div_zero` | M4 | Arithmetic is stored as Dynamic; division is not evaluated in M2. |
| `expr.index_oob` | M4 | Indexing is evaluated against data in M4, even when its syntax is literal. |
| `lib.mismatch` | M5 | Requires stored-message replay with a recorded library identity. |
| `limit.event` | M4 | Requires construction of a gesture event, including resolved label and context. |
| `limit.fuel` | M4 | Requires execution of a fuel-metered expression or action step. |
| `query.denied` | M4 | Requires a host approval decision. |
| `query.failed` | M4 | Requires a host tool delivery. |
| `ref.too_large` | M4 | Requires resolved host data and delivery byte accounting. |
| `render.panic` | M6 | Requires a renderer subtree boundary and renderer failure. |
| `state.type_write` | M4 | Actions are inert plans; this code describes an executed state write. |

For already-covered codes, runtime variants remain M4 work: arithmetic overflow
and non-finiteness; state byte accounting during writes; dynamic
fmt/provenance/type errors; list producers and evaluated each keys. Typed/escaped
keys and duplicate/bad-key recovery are tested directly through the allocator in
M2, without evaluating data. All runtime limit configuration and hard ceilings
exist now; their execution/delivery enforcement is not claimed.

Every spec change and its reason is recorded in `spec/CHANGELOG.md` and paired
with affected fixtures/tests in the implementation commits:

| Sections | Change | Why |
| --- | --- | --- |
| 2.5 | Count enclosing depth when growing binary/postfix prefixes; retain cutoff-null sentinels | Exact mutation-driven fixtures exposed a fresh local budget being granted inside outer expressions. |
| 2.7 | Explicit builtin arities, lambda shapes/options; call namespace distinct from value bindings | Signatures did not fully define static validation or shadowing. |
| 2.7, 2.10–2.11 | Lambda shadowing and node-template environment; locally checked orphans; root depth/count and alias depth; single-node roots; deferred plans | Avoid false instantiation errors, ambiguous root IDs and accidental data evaluation. |
| 2.10, 12.3 | Permute the sealed table after syntax cutoffs; allocate concrete nodes/depth in canonical pre-order | Lexical byte/count cutoffs are inherently source ordered; semantic budgets/cycles must not be. |
| 2.12 | Compact-JSON state byte cost, signed literal constants, canonical first syntactic binding, unused-state behavior | Define deterministic typing/allocation without an evaluator. |
| 2.13–2.15 | Inert action steps, sibling preservation, literal typed emit target, canonical query-name budget and malformed tool signatures | Close recovery/order/target ambiguities without executing effects. |
| 4.3 | Add cap.fallback and lib.mismatch rows | Other sections already required these codes, but the table omitted them. |
| 4.4, 8.3 | Recovered-null metadata; required nullable values; transparent nullable enum/token/action/binding/slot behavior | Prevent duplicate diagnostics and reconcile the recovery ladder with optional types. |
| 5.5 | Static Dynamic encoding, retained node/action/fallback definitions and environments; non-literal containers remain plans | Make the M2 output usable by M4 without partial evaluation or quadratic type expansion. |
| 8.1, 8.4–8.5 | Checked defaults/metadata, self-contained examples, marked group references, total terminal fallback targets and immutable extension base | Make builder checks precise and keep extensions additive. Align the fallible std API example. |
| 8.2–8.3 | Canonical number/object formatting, exclude the hash field, add generic any | A self-containing hash is undefined; generic data/state types needed an explicit type. |
| 8.6, 5.5 | Terminal Text when markdown is absent; data/float fallback text retains an M4 formatter plan | Prevent unsupported fallback chains and raw JSON substituted for a chart table. |
| 8.8 | Correct 29-entry count; structural TableColumn role; literal Facts record shape and ignored extra keys; Code highlighting optional | Align catalog metadata, IR roles and literal schema behavior. |
| 12 | Strict M2 fixture inputs, additive recipes, frozen Library Spec bytes, explicit documentation host context, all exemplars, fence version gate and code deferrals | Make acceptance reproducible and prevent silent unsupported cases. |
| 12.7 | Deterministic tree text, explicit deferred plans, empty host policy, diagnostics and exit status | Define the new inspection command without evaluation or hidden exemplar privileges. |
| 12.8 | Covered/deferred files, scanner/theme/dependency scope, per-commit protection, frozen mutation inventory/eight-shard aggregation and reviewed equivalents | Make the new quality requirements executable and reproducible. Added exact recovery metadata, extractor, span, EOF, delimiter, invalid-key, container/index budget, lookahead, printer and public splitter regressions. |

Final candidate 8a5d9e8 passes Linux, macOS and the quality job:
https://github.com/ganeshmshetty/open-ui/actions/runs/39741887882 .
The first complete mutation CI run (39716792722) correctly failed on 23
unreviewed survivors: 499 caught, 75 unviable, 9 test timeouts, 26 survivors.
The second complete run (39735047609) caught every earlier unreviewed survivor
but exposed the additive printer precedence deletion (previously a timeout).
It finished with 520 caught, 75 unviable, 9 test timeouts and 5 survivors, of
which four are equivalent. A fixture and 338 deterministic operator pairs now
assert the remaining rule. The focused timeout recheck caught four mutants and left nine test timeouts
at 120 seconds, with zero survivors. The fresh eight-shard acceptance run
passes all baselines, source/review checks and complete inventory aggregation:
https://github.com/ganeshmshetty/open-ui/actions/runs/39741887850 .
No failed or focused campaign is used as full acceptance.
Local fmt, Clippy on Rust 1.94 and 1.99, workspace tests, fixtures, spec-check,
fuzz formatting/builds, source/history and dependency checks pass.

Fuzz history remains in fuzz/runs/m2/: the initial run was interrupted and the
nullable checkpoint predates the formatter-plan correction. Original M2 acceptance
on 2bd18eb completed 601 seconds / 71,760 executions. The contextual-depth fix
receives fresh acceptance in fuzz/runs/m2-quality/: parser 601s / 95,490 executions,
validator 601s / 44,685, exit 0, matching frozen source hashes at run completion,
301 initial seeds. test-only-follow-up.json records the two later cfg(test)
source changes; production behavior and fuzz-target code are unchanged.
Both now vary expression-depth limits from 0 through 48. No crash was found.

Mutation discovery tested all 609 original mutants: 428 caught, 75 unviable,
57 timeouts, 49 survivors. That run is explicitly non-acceptance checkpoint
evidence. The new suite addresses 46 survivors. Two equivalent temporary Span.end
edits are overwritten by emit; complementing a private namespace tag preserves
all duplicate-name membership. The fourth equivalent changes a skipped-range
endpoint comparison that no filtered lexer error can equal (EOF or a valid header
token). All four exact rationales and current containing-file hashes are recorded.
Final CI reran every mutant, including every old timeout, with a 60-second
test limit: 521 caught, 75 unviable, 9 test timeouts, 4 reviewed equivalents. All eight baselines and inventory/source/review
aggregation passed. No old outcome was carried into acceptance.
Complete per-mutant acceptance, inventory, discovery, timeout recheck and review
records are in conformance/mutations/. Full raw outcomes, diffs and logs remain
in the final CI artifacts; acceptance.json retains their hashes and every result.

No unresolved M2 contract ambiguity is known. Decisions about roots, template
scope, nullability, canonical allocation and deferred evaluation are explicit in
the spec and fixtures. Runtime-dependent results and live-stream equivalence require M3/M4 validation.

The required M3 open issue remains: `Splitter::finish()` suppresses BadChar,
BadEscape, UnterminatedString, Overflow and NonFinite diagnostics inside
size-cutoff skipped ranges. A streaming push may already have emitted them.
M3 must hold those diagnostics provisional until the cutoff decision, or
stream != batch. This milestone does not claim the issue is solved.

Unrelated untracked `research/marketing/` files were left untouched. No files
under `research/openui/` were opened, searched or copied.


Open decisions: none blocking. The empty-host default for tree is adopted in
DECISIONS.md and can be revisited without a compatibility or scope change.
Naming/license/publishing remain ROADMAP item 10; no publishing decision was made.
Next is ROADMAP item 14's GPUI spike, including the
screenshot pipeline; M3 must then preserve the diagnostic issue recorded above.


## M3 start — 2026-10-22

Created milestone/m3 from main and merged main (already at 6bca7fa). Read AGENTS,
PROGRESS, ROADMAP, CHANGELOG, M2 report and the requested streaming/patch contract.
The approved spike changes are included; throwaway spike code is not merged.
No self-merge: external streaming review is required. Runtime execution remains M4.
Next: streaming/patch fixtures and canonical encoding vectors before implementation.

### M3 implementation checkpoint

Streaming parser and Session now use the existing resumable lexer and sole
splitter. Sealed ASTs are parsed once, held-back UTF-8/CR/fence/header bytes stay
uncommitted, and tail previews close a copy with parse-failure retention. Size
cutoffs filter provisional lexer errors before commit. Added reverse dependency
revisions and static-subtree reuse with canonical budget checks; deferred/runtime
nodes remain checked plans, never evaluated.

The named wire protocol includes diff/coalescing, checked replay, snapshots,
epochs, statuses and append_text; replay CLI has an exact output fixture. Current
new fixture categories contain 20 streaming and 5 patch cases, including explicit
live checkpoints and an epoch/snapshot script. Their hand-written goldens include
the existing std prop defaults/roles; initial uncommitted drafts omitted those
catalog fields and were completed before acceptance. No old semantic golden was
changed to conceal a failure: existing IR whitespace is migrated to approved
compact canonical bytes; six literal map payloads migrate to the explicitly
specified ordered-map encoding in the same spec/fixture change.

The full legacy chunk harness now also checks live Session final IR/diagnostics,
sealed traces, retained-tree replay and absence of provisional effects. A 100,000
seeded-program discovery run passed in release mode (5.90s); a fresh frozen-source
run is required after final hardening. A 61-second ASan streaming smoke completed
224,333 executions with exit 0; it is not the required acceptance campaign.
Default/preserve_order workspace tests passed before the final protocol fixtures.
The std hash remains d19a769bd510997ce786ade4d60db23d.

First non-gating Criterion results before subtree reuse: 64-byte chunk on 1,000
nodes ~7.14ms (sampled p99 7.595ms), syntax batch 200KiB ~3.50ms, validated batch
~7.76ms. The chunk target is missed; final numbers will be remeasured and reported,
not hidden or made gating retroactively. Remaining: final schema/checkpoint and
sequence assertions, full 100k gate, 600s seeded fuzz, complete syntax/IR/patch
mutation campaign, final report and CI. No M3 completion or self-merge is claimed.

### M3 frozen-source validation checkpoint

All 23 streaming and 5 patch fixtures pass, including provisional action aliases
through sealed consumers and a held lookahead header colon. Raw message budget
tracking now preserves CRLF bytes, and a closing fence seals its final statement.
Final production source is frozen for the renewed 600-second streaming fuzz run.
The earlier seeded campaign was interrupted at 89,119 executions after the new
I6 checkpoint exposed a handle-publication bug; it is not acceptance evidence.
The complete mutation inventory now contains 967 candidates; historical M2
evidence is preserved separately. CI and mutation acceptance remain pending.

Important open review issue: static subtree reuse does not eliminate global
validator bookkeeping, IR copies and full tree diff. The scanner/tail counters
assert parse-once/bounded-tail work, but do not establish the full I7 O(dirty)
IR/patch bound. The ~7ms chunk measurement misses the non-gating target. No claim
of complete I7/performance compliance or M3 completion is made.
Next: final gates, mutation analysis, evidence/report, then push a review PR;
no self-merge. See docs/milestones/M3.md for the detailed review scope.

### M3 acceptance evidence checkpoint

Final streaming fuzz: 601 seconds, 113,441 executions, exit 0, no crash; random
chunkings in both Program and Message modes, varied syntax/IR/byte limits, source
hashes verified unchanged. Logs and metadata are in fuzz/runs/m3/. The prior
interrupted campaign and smoke remain explicitly non-acceptance history.
84 Rust tests pass under both JSON backends; 10 Python tests, all 323 fixtures /
73,503 chunkings / 400 permutations, 100k generated cases (6.03s release), fmt,
Clippy, dependency/source checks and spec-check pass. Spec-check: 3 ui, 4 fragments,
24 exemplars (zero ui-invalid fences currently in SPEC). CI run 39777596951 passes
macOS/Linux/quality/preserve_order. Mutation run 39777596929 is still pending;
no acceptance or completion is claimed before its results are reviewed.
ROADMAP 13's replay command is implemented and tested. Final Criterion: 64-byte
chunk / 1,000 nodes 7.594ms, sampled p99 8.827ms; 200KiB syntax batch 3.745ms,
validated batch 8.202ms. Chunk target missed; I7 dirty-only maintenance remains
an explicit external-review issue. Full report: docs/milestones/M3.md.

M2 cutoff issue resolved in M3: fixtures streaming/014–019 exercise all five
cutoff-sensitive lexer diagnostics, plus retention of an independent earlier
error, under mandatory byte-by-byte and all other chunkings. Diagnostics inside
skipped ranges stay provisional and are withdrawn before sealing.

Mutation infrastructure note: shard 4 of run 39777596929 received a runner
shutdown signal at 12:42 UTC and could not upload evidence. The other seven
shards remain useful and continue. GitHub rejects a job retry while this run is
active; retry only missing/failed shards after completion. No incomplete shard
is acceptance evidence.

### Owner-requested pause and disk cleanup — 2026-10-22

Owner requested stopping local work while CI runs, and reducing repository disk
usage. Stopped the local shard-4 mutation process and its children, plus the CI
watch process. GitHub CI was left running for the owner's update; no new runs,
retries or PR were created. Local shard 4 is explicitly incomplete: baseline
success, 57 caught, 14 unviable, 2 timeouts, 1 already-reviewed equivalent; its
partial logs/results remain in /private/tmp/genui-m3-local-shard4 and are not
acceptance evidence.

Removed only generated Cargo build artifacts: workspace dev cache (cargo reports
17.3 GiB logical bytes), fuzz target cache (905 MiB), and an obsolete M2 temporary
source target cache (293 MiB). Stopping mutations also cleaned its isolated build
copies. Source, fixtures, committed benchmark/fuzz evidence, release outputs and
Criterion results are preserved. No source changes or fresh tests were needed
for this cleanup; do not rebuild until the owner resumes work.

M3 remains incomplete: full I7 dirty-only maintenance and complete mutation
acceptance are open. Code commit ffd602f is pushed; local evidence commit 4c14fbc
and subsequent progress/report/history notes remain to be pushed/committed.
No PR exists yet, and no self-merge occurred. Next: wait for the owner to report
CI completion, inspect complete results and resolve the remaining blockers.

### M3 mutation follow-up after owner reported three failed jobs

Normal CI is green. Mutation shards 1 and 6 failed on assertion gaps; shard 4
received a runner shutdown and uploaded no evidence. Seven complete shards
cover 846 candidates: 388 caught, 346 test timeouts, 86 unviable, 26 survivors.
Preserved as non-acceptance discovery in conformance/mutations/m3/discovery.json.

Added direct regressions for binary/index dependency edges and revisions, explicit
Program-vs-Message input, provisional numeric spans, repaired raw endpoints,
CRLF endpoints, normalized splitter source, complete/incomplete lambda heads,
named-key/token tails, and active cutoff diagnostic filtering. Existing goldens
and production core behavior are unchanged. One additional mathematically
redundant bound is reviewed with its precise invariant; six total equivalent
mutants have source-matched rationales. A focused 34-candidate recheck caught 26
and left six equivalents plus two new gaps; explicit mutant applications prove
the final two added regressions kill both remaining gaps. This focused work is
not full milestone acceptance.

Mutation builds now use opt-level 1 tests and no debug symbols, retaining debug
assertions/full workspace tests. Local workspace runtime drops to ~7 seconds and
build artifacts stay around 1 GiB instead of ~13 GiB. The inventory remains
967 names. All eight shards must be rerun against the same updated audited
source hashes; mixing old passing shards with new source would fail the gate.
Production/fuzz-target hashes remain unchanged; the cfg(test)-only follow-up
is documented in fuzz/runs/m3/test-only-follow-up.json, preserving the original
601s fuzz evidence. Final local gates and one fresh CI campaign are next; pause
local work while CI runs, as requested by the owner. I7 remains unresolved and
M3 is not marked done; no PR or merge yet.

Follow-up local gates pass with clean crate artifacts: 90 Rust tests, also with
serde_json/preserve_order, all 323 fixtures / 73,503 chunkings / 400 permutations,
spec-check, fmt, Clippy -D warnings and source checks. The manual mutation recheck
initially contaminated a shared build cache; affected workspace crate artifacts
were removed and every final gate was rebuilt cleanly. No mutated source touched
the repository. Final logs are in docs/milestones/evidence/m3/regressions-*.log.
A single fresh eight-shard campaign will be triggered by this follow-up commit;
pause local jobs/monitoring after push and wait for the owner's CI update.

### Greptile Plus review fixes — I7 remains a merge blocker

The owner authorized merge after fixes and explicitly required completing I7
first. Greptile Plus on 7d8278c completed (run
cff691bf-be88-4b11-a6ad-2a8683ce8014); evidence is in
docs/milestones/evidence/m3/greptile-plus-7d8278c.json. It reported three bugs:
dropped drafts left stale consumer output, draft bindings changed sealed state
types, and previews exceeded the statement-count budget. Added fixtures 025–028
first and fixed all three. Fixture 024 asserts an unrelated growing draft keeps
the displayed tree unchanged; it does not prove the I7 complexity requirement.

An I7 architecture experiment (persistent containers, maintenance counters and
a RED 1,000-node work-bound regression) is unfinished and has been shelved,
not accepted or committed. Its complete tracked diff plus new source files are
preserved at /private/tmp/genui-m3-i7-wip-20261022. The working tree retains the
independent review fixes. I7 still requires persistent validation/instance caches,
dirty-only patch generation and diagnostic deltas; subtree reuse alone is not
sufficient. No claim of M3 completion, PR or merge has been made.

Normal CI on 7d8278c passed. Mutation run 39792801855 ended cancelled, with
shards 4 and 6 failing and other shards cancelled; it is not acceptance evidence.
The current production fixes require fresh fuzz/mutation acceptance after I7 is
complete. Avoid starting another expensive campaign against intermediate code.

The owner confirmed: complete I7, then merge. Local review-fix gates pass:
90 Rust tests, 328 fixtures / 74,849 chunkings / 400 permutations, spec-check,
fmt and Clippy -D warnings. The independent review fixes are checkpointed before
resuming I7; no new CI campaign starts against this intermediate code.

Spec change: §12.2 clarifies validated empty IR slots versus normalized retained
patch trees; CHANGELOG records the reason. No existing golden was changed.

### I7 resumed — retained scalar checks and diagnostic storage (work in progress)

Owner confirmed again: do I7, then merge. Review fixes are committed as b84ac33
(not pushed; no extra CI campaign). The I7 experiment is now active in the working
tree, not shelved: persistent maps share individual payloads, small arenas share
roots, slot child lists are persistent, and validator/instance/type snapshots are
retained. Scalar draft consumers can update one node among 1,000 without visiting
unchanged siblings; sealing that draft visits one wire node and finish does no
extra tree visits. Semantic diagnostic multisets retain separate raw/combined
flag indexes, so provisional overlays and publication use actual deltas rather
than cloning/scanning history. Source work counters include validator and wire
visits; unchanged-payload clone counters also test the storage layer.

Local checkpoint before the next RED test: 98 Rust tests passed (59 core,
30 conformance, 5 std, 4 CLI), including the full 328-fixture chunk harness and
seeded streaming property. Logs: /private/tmp/genui-m3-i7-stage-tests2.log. No
new golden was changed. Newly added late-child work-bound test is RED and is
being implemented, with evidence in /private/tmp/genui-m3-i7-late-child-red.log.

Full I7 is still incomplete: structural/forward-reference changes, canonical
allocation and state/query changes retain the batch-equivalent fallback; scanner
diagnostic history is still scanned; expression/action arena normalization and
full-path dirty accounting need further work. These are M3 blockers, not M4
deferrals. Do not merge, mark M3 done, or treat the old fuzz/mutation evidence as
acceptance for this changed production source. Next: retained slot tasks and
ordered budget summaries, then expanded deterministic work tests, all chunkings,
100k property, fresh 600-second fuzz, complete mutation gate and final review/CI.

### I7 local checkpoint — scanner history and retained slot insertions

The first seven deterministic work probes are green: unrelated draft/tree work,
semantic diagnostic history, a displayed draft among 1,000 nodes, sealing/finish,
a late child among 998 unchanged siblings, a late typed state binding, and 1,000
retained lexer diagnostics. Slot prefix counts preserve zero-width source entries;
new children enter without scanning preceding siblings. Scanner diagnostics now
commit stable prefixes into persistent code/flag indexes and only revisit the
bounded open range; batch and streaming still use one splitter. Recovered-null
annotations receive only the three relevant scanner codes. The full chunk harness
passes after these changes; workspace totals are 102 tests (60 core, 33 conformance,
5 std, 4 CLI), and all 328 fixtures remain unchanged. Evidence:
/private/tmp/genui-m3-i7-scanner-suite2.log. Fuzz target builds with the updated
fuzz lockfile; no new fuzz acceptance is claimed. Quality source/dependency checks
pass. Current small build artifacts use about 2–3 GiB, with ~22 GiB free.

Remaining I7 blockers: general structural replacement/cascading recovery, canonical
budget projection, state-default inference/allocation, queries, incremental global
classification/reachability, and stable expression/action arena identities. The
existing dense live arena representation renumbers unaffected instances; retain
canonical snapshot encoding while separating live identity from dense storage.
This is an implementation/contract issue to resolve with focused fixtures, not an
excuse to evaluate M4 expressions early or waive I7. Existing conservative full
validation fallbacks are functional but do not satisfy I7.

Mutation scope now includes validation, diagnostic storage, persistent storage and
maintenance accounting because the dirty-maintenance implementation lives there.
The old 967-name inventory is stale for this changed scope and must be frozen anew
at the final gate. No extra CI campaign has been started. Continue implementation;
no merge or M3 completion yet.

### I7 checkpoint — stable arenas and diagnostic history (2026-10-23)

Stable expression identity now includes structural/recovery metadata with full-key
collision comparison. Dense indices belong only to canonical snapshot observers;
inserting an earlier dynamic sibling retains the other node payload. Owner scopes
release obsolete expressions. Static action-plan references use escaped source
owner keys; no evaluator or runtime dispatch is introduced. Four static-action
IR goldens and one checkpoint change by the documented formula, with SPEC §§5.5,
6.5 and CHANGELOG updated together. New fixtures 029–030 and library 037 cover
reference stability and escaping; draft fixture prop `pad` was corrected to the
actual §8.8 `padding` signature before accepting it.

Scanner pending diagnostics are indexed by endpoint. Undecided resync headers
retain errors provisionally until the skip boundary is certain (fixtures 031–032).
Long discarded numbers skip conversion that would only produce suppressed errors;
standalone lexical behavior remains unchanged. Held prefixes count toward tail
work even before a header is recognized. Extractor diagnostics use persistent
multisets and finish avoids history materialization. New work probes cover 1,000
unrecognized-prefix errors, 1,000 extra fenced programs and earlier dynamic nodes.

Checkpoint gates: 108 Rust tests (63 core, 36 conformance, 5 std, 4 CLI), 333 fixtures /
76,199 chunkings / 400 permutations; fmt and Clippy -D warnings pass. Logs:
/private/tmp/genui-m3-i7-checkpoint2-{tests,fixtures,clippy}.log. Mutation scope now
includes arena.rs; inventory and final-source acceptance remain pending. No push,
new hosted run, final fuzz acceptance or merge is claimed.

Remaining I7 blockers: general structural replacement and cascading recovery,
canonical allocation summaries, state inference/budget changes, queries and global
classification/reachability. Full fallback validation is still functional and
batch-equivalent, but does not meet I7. Continue those paths before final review,
fuzz/mutation gates and merge; owner has not authorized any deferral.

### I7 checkpoint — canonical state allocation and container props (2026-10-23)

Added a persistent AVL resource index with cached valid-budget intervals. Earlier
state declarations now recompute only the affected acceptance frontier, including
non-monotonic greedy acceptance (an earlier declaration can evict one state and
let another fit). Typed state candidates are retained independently of acceptance;
changed state names invalidate their transitive value/component consumers through
the reverse dependency graph. A 1,000-declaration probe rejects whole-history
maintenance. Null/empty defaults still require the general inference path.

Container prop edits retain slots only when their bound expressions and recovery
metadata are unchanged. A Card title edit over a 1,000-node body now checks its
own instance and one wire node. Ambiguous state inference continues through the
full path rather than retaining an uninferred provisional state type.

Fixtures preceded the corresponding fixes: limits 023–024 cover direct-slot depth
recovery and min_items allocation rollback; streaming 033–037 cover canonical state
allocation, binding detachment/reattachment, container props and transitive aliases.
The 100k property then exposed an old local-graft bug: a depth failure could retain
a later cutoff diagnostic after an earlier reference arrived. Fixture 038 fixes
that regression by returning to canonical allocation when grafting hits either
node or depth limits. SPEC §2.10 and CHANGELOG clarify direct-slot stopping and
rollback; no existing expected fixture output was edited in this checkpoint.

Gates: 112 Rust tests (65 core, 38 conformance, 5 std, 4 CLI), including the expanded
100,000-program stream/batch/replay property; 341 fixtures / 78,212 chunkings / 420
permutations; fmt, Clippy -D warnings, spec-check, source/dependency quality, ten
Python gate tests and serde_json/preserve_order workspace tests pass. Evidence: /private/tmp/genui-m3-i7-expanded-property-tests.log,
/private/tmp/genui-m3-i7-budget-{fixtures2,clippy}.log. The property generator now
includes state-budget and container scenarios; the streaming fuzzer varies the
state byte limit. No renewed 600-second fuzz acceptance is claimed yet.

I7 remains incomplete. The next implementation must remove general structural /
canonical node-budget fallbacks, incremental classification/reachability scans,
query maintenance and null/empty state-inference scans. These are M3 blockers;
the local fast paths and passing functional tests do not waive them. Final-source
fuzz, complete mutation inventory/campaign, final Greptile review and hosted CI
remain pending. No push or merge has occurred, avoiding intermediate CI campaigns.

### I7 checkpoint — retained queries, aliases and failed leaves (2026-10-23)

Queries now use the same persistent canonical resource index as state allocation.
All raw query sources survive budget rejection; per-query diagnostic ownership
allows a newly earlier name to evict the old frontier and withdraw its tool/value/
orphan diagnostics exactly. Query args/default checks share the batch checker and
remain static. A 1,000-query probe checks bounded source/diagnostic visits and zero
visits to the unrelated displayed tree. Accepted queries issue no M3 requests.

Reachability additions follow newly introduced query/value dependencies and remove
only newly reachable orphan-owned checks. Scalar aliases reuse their checked value
and invalidation graph; kind/dependency changes still enter the general path.
Slotless failed component checks are retained, including failures after ordinary
prop checks (Facts). This permits root leaf creation, removal, and diagnostic
replacement without rescanning 1,000 unrelated definitions. The new-root work
probe allows three charged visits (construction, diff and root attachment), not
one; each visit is to the newly dirty node. No expected fixture was edited.

Fixtures 039–047 preceded the fixes they exercise: canonical query eviction,
state-dependent args, optional/required consumers, failed-leaf recovery, Facts
post-check diagnostics, orphan-check withdrawal and scalar aliases. All 114 Rust
tests pass (65 core, 40 conformance, 5 std, 4 CLI), including 100,000 generated
stream/batch/replay cases. All 350 fixtures / 80,695 chunkings / 420 permutations
pass; fmt, Clippy -D warnings, source/dependency checks and spec-check pass. Logs:
/private/tmp/genui-m3-i7-query-suite5.log and /private/tmp/genui-m3-i7-query-{fixtures,clippy}.log.
No spec semantics change was needed. The fuzzer now varies max_queries as well as
state bytes; its final 600-second acceptance has not been rerun.

Remaining blockers include structural recovery inside slots, canonical node-budget
frontier maintenance, general kind/reachability changes, null/empty state inference
and unaffected work in the fallback path. Next: retained slot leaf insertion/removal
with indexed sibling positions, then general task/budget maintenance. Final-source
mutation/fuzz/review/hosted CI are pending. Do not merge or mark I7 complete yet.

### I7 checkpoint — slot leaf recovery and first drafts (2026-10-23)

Retained leaf checks can now insert/remove inside an existing slot while keeping
unrelated siblings untouched. Prefix counts preserve source positions; grafts carry
an explicit canonical order and indexed removal hint. Core removal checks that hint
without scanning siblings; replay keeps the same public protocol. Removed instance
revisions are cleared. Mixed graft/property changes publish both, and slot minimums
are checked after all dirty leaves so a one-column replacement stays atomic.

The 999-sibling work probe asserts bounded maintenance and the exact single insert
or remove operation. New fixtures 048–051 cover insertion, removal, a minimum-slot
replacement and repeated paths. Static action aliases now share the retained scalar
work queue and normalizer (fixture 052); neither normalization nor provisional
content executes actions or produces requests/model events.

First candidates now try retained checks too. A provisional leaf can fill a waiting
slot, with sealed state types held read-only, then seal locally (fixture 053 and a
second 999-sibling probe). Slot-only resolution rejects invalidated scalar/query
consumers (054). Sealed maintenance selects the sealed base when the prior draft
was dropped; it cannot commit that draft or leave its widget behind when another
query/slot seals in the same chunk (055–056). Existing goldens were unchanged.

Checkpoint gates pass: 118 Rust tests (66 core, 43 conformance, 5 std, 4 CLI), the
expanded 100,000-program property (now includes actions and simultaneous draft
recovery), 359 fixtures / 83,488 chunkings / 420 permutations, serde_json/preserve_order
workspace tests, fmt, Clippy -D warnings, spec-check (3 programs, 4 fragments, 24
exemplars), source/dependency checks, 10 Python gate tests and all fuzz-target builds.
Logs: /private/tmp/genui-m3-i7-slot-action-{suite2,fixtures,clippy,preserve,fuzz-build}.log.
Disk still has ~21 GiB free. No CI campaign was started and no push/merge occurred.
No spec semantics were changed in this checkpoint.

I7 is not complete. General canonical node/depth allocation, cascading parent
recovery, kind changes/reachability deletions, null/empty state inference and paths
still using full validation must be replaced. The retained semantic-task/visible-IR
architecture and budget profiles are recorded in DECISIONS.md. Do not count the
functional fallback as dirty-only maintenance. Final-source 600-second fuzz,
mutation inventory/campaign, final review and green hosted CI remain required.

### I7 checkpoint — canonical resource profiles (2026-10-23)

The state/query allocator now uses one generic persistent task-range index. A
retained profile records allocation consumed, its valid remaining-budget interval,
direct-slot stopping and rejected-task count. Composition preserves constraints
when a failed container rolls allocation back and clears its descendant's stop.
Dirty tasks beyond a stopped prefix stay pending until that frontier reaches them;
unchanged ranges and snapshots remain shared. Four tests cover stopped suffixes,
rollback, nested slots and 400 changing canonical allocations; the existing greedy
allocation/property tests remain unchanged. The tests preceded implementation.

Provisional fallback validation now borrows sealed statements plus the candidate
through an iterator, avoiding the additional whole-Program AST clone. Actual full
source-check visits are still counted; this does not disguise that fallback.

Gates: 122 Rust tests (70 core, 43 conformance, 5 std, 4 CLI), including 100,000
generated stream/batch cases; 359 fixtures / 83,488 chunkings / 420 permutations;
fmt, Clippy -D warnings, source/dependency checks and 10 Python gate tests pass.
Logs: /private/tmp/genui-m3-resource-{suite3,fixtures2,clippy3}.log. No existing
golden or spec semantics changed. No push, hosted CI, or merge was performed.

This is allocator groundwork, not I7 acceptance. Node instantiation must still use
retained semantic tasks and a separate visible projection, including hidden failed
subtrees. General source classification/reachability and null/empty state inference
also still need retained dirty maintenance. Final-source fuzz, mutation, review and
hosted CI remain pending; merging remains blocked by the owner's I7 requirement.

### General I7 mechanism — implementation in progress (2026-10-23)

Replaced live instantiation fallbacks with retained semantic task scopes, owned
static arenas/diagnostics, persistent ordered slot/list ranges and an independent
visible projection. Hidden failed subtrees are retained without publishing their
nodes. Resource ranges now cache typed aggregates and distinguish materialized
outputs from semantic effects. Patch projection uses source-rank addresses and
sparse edit ranks; drafts are overlays on the sealed persistent root.

The checked-in 359-fixture chunking test passes after wiring the range/projection
path (`/private/tmp/genui-m3-projection-fixtures2.log`). This is not acceptance:
source classification/checking/inference still run full passes, so work-bound
unit tests remain red. Work is uncommitted while the general source/expr task
mechanism is being completed; no CI, review campaign, push or merge was started.
A new resolve 019 fixture precedes a static-cycle ownership clarification in SPEC
§2.10 + CHANGELOG; its checker implementation and verification remain in progress.

### I7 completion checkpoint — retained source/task mechanism (2026-10-23)

The general live mechanism is now the retained semantic Task/Graph projection plus
retained source summaries; the former full-validation fallbacks are no longer on
the hot path. The five previously RED work probes pass: unchanged-tree draft text,
a 1,000-node draft sibling, a late typed state, sealing/finishing a visible draft,
and a container title over 999 descendants. Name-alias resolution and local
tracking bookkeeping are no longer double-billed as instance visits.

Null/empty state inference now uses a retained syntactic binding graph instead of a
full root walk. Each statement keeps its first-binding summary and referenced-name
list; only the dirty reverse closure is recomposed, unchanged summaries are skipped,
forward-reference summaries stay uncached until their dependency seals, and a later
earlier binding can retract and reorder a previous inferred type. A new 999-sibling
probe covers the inference path. Static value/action cycles now use one SCC owner
(fixture resolve/019, SPEC §2.10 and CHANGELOG).

Gates: fmt, workspace Clippy `-D warnings`, and all Rust tests pass — 129 total
(45 conformance, 75 core, 5 std, 2 CLI/integration, plus unit/doc targets). All 360
fixtures / 83,719 chunkings / 430 permutations pass; spec-check validates 3 programs,
4 fragments and 24 exemplars; the 100,000-program stream/batch/replay property passes
in release (46.62s). No fresh 600-second fuzz campaign, complete mutation campaign,
hosted CI or push/merge was run in this checkpoint, so M3 acceptance is still open.
The 100k property is slower than the pre-retention 6.03s and must be measured before
any performance claim.

### M3 fresh-source fuzz acceptance — b6f17ed

The changed production source (`session`, `validate/**`, `patch`, arenas and storage)
completed two fresh 601-second ASan/libFuzzer campaigns with the standard seed,
dictionary and 32 KiB max length. Both exited 0 with matching frozen source hashes:

| Target | Engine seconds | Executions | Peak RSS |
| --- | ---: | ---: | ---: |
| streaming | 602 | 10,900 | 803 MiB |
| validator | 602 | 19,287 | 558 MiB |

Full logs, `acceptance.json` and `frozen-source.json` are in `fuzz/runs/m3-i7/`.
The campaigns found and fixed two real defects before this run: retained argument
diagnostics were not replayed on a rebuilt task, and action plans from a provisional
reverse closure were still published. Regression tests for both are in
`crates/genui-conformance/src/streaming.rs`. The mutation gate is still running, so
M3 is not yet marked complete and nothing is merged.

### M3 mutation survivor triage (2026-10-23)

The corrected 16-shard campaign completed in ~2h23 but exposed 605 `MISSED`
mutants in the new retained-source/task code, concentrated in `validate/
incremental.rs`, `tree.rs`, `tasks.rs`, `expression.rs`, `mod.rs`, `session.rs`,
`binding.rs` and `resource.rs`. Four parallel file-disjoint worktrees added 217
core tests and 128 source-hashed equivalent rationales in
`conformance/mutations/m3/review.json`. Triage also found and fixed a real bug:
`binding::refresh` could re-push a cyclic dependency forever; the cycle guard now
has a regression. A small number of mutants remain unclassified and will be
rechecked by the next campaign. Full workspace tests, fixtures, fmt and Clippy
(1.94 and 1.99) pass after merging the groups; no want.* golden was weakened.

### Gate change — mutation becomes advisory, demos become mandatory (2026-10-24)

Owner decision: mutation testing no longer blocks merges or milestones. SPEC §12.8
item 7 now defines it as a nightly/advisory report with a complete-inventory review
kept as backlog evidence; M3's done-when requires CI plus fresh fuzz, not a clean
mutation campaign. M3 is marked complete on that basis. AGENTS self-merge now says
mutation never blocks, and adds a demo-first rule: each milestone must end with a
runnable user-visible demo. ROADMAP items 19–20 add a streaming-dashboard vertical slice and the
scheduled mutation workflow; chess is explicitly not a milestone. The 32-shard run was cancelled once the new contract
was recorded; its survivor data remains in the repository for later triage.

## M4 slice 1 — runtime evaluation foundation (2026-10-23)

Fixtures were authored before implementation: `numbers` (6), `expr` (9), `state`
(4) and `actions` (1), each with a canonical `want.resolved.json` retained tree
and exact static-plus-runtime diagnostics. Existing streaming checkpoint 029 was
migrated from the retired M3 `deferred` object to the evaluated string because
SPEC §5.5 already requires M4 to send resolved values; no other golden changed.

Implemented a UI-free tree-walking evaluator over validated expressions:
literals, names, state reads, lists/maps, checked Int and finite Float arithmetic,
comparison/logic with short-circuiting, lazy `if`/`coalesce`, member/index,
lambdas with locals, `map`/`filter`/stable `sort_by`, aggregation, text/format
builtins, `ref()` placeholders, query-default reads and per-prop fuel. Errors map
to `expr.overflow`, `expr.non_finite`, `expr.div_zero`, `expr.index_oob`,
`expr.type`, `expr.unknown_fn` and `limit.fuel` with the §4.3 recovery. Strings
honour `max_string_bytes`; list producers honour `max_list_len`.

`Session` now owns the runtime state store and evaluated props, sends evaluated
Dynamic values in live patches, exposes `runtime_diagnostics()`, `resolved()` and
`action_handle(path, prop)`, and adds an explicit `activate(node, handle)` entry.
Activation is inert before `finish()` or for a stale epoch/handle. `set`/`restore`
run in order with type checks (`state.type_write`) and total byte accounting
(`limit.state`); `refresh`/`say`/`open`/`emit` are returned as deferred steps and
are **not** dispatched. Validation and streaming still never execute actions.

Explicitly not implemented in this slice: host query dispatch and delivery,
ref resolution/delivery, model events and `limit.event`, forms/field status,
provenance mixing and URL policy enforcement, and `each`/node-producing
`if`/`coalesce` instantiation. `each` in a value position remains a runtime
`expr.type` error. The existing I7 review blocker and M3 acceptance state are
unchanged by this slice.

Local gates: `cargo fmt`, workspace Clippy `-D warnings`, `cargo test --workspace`
(138 tests: 82 core, 47 conformance, 5 std, 4 CLI/quality, plus empty/doc
targets), 380 fixtures / 89,161 chunkings / 430 seeded permutations,
`genui-cli fixtures`, `genui-cli spec-check` (3 programs, 4 fragments, 24
exemplars) and the `serde_json/preserve_order` workspace tests. No fuzz or
mutation campaign was run.


## M4 slice 2 — runtime host boundary (2026-10-23)

Fixtures were authored before each fix (and observed failing where behavior was
wrong): `queries` (5), `refs` (4), `events` (2), `forms` (5), `provenance` (6)
and `actions/002`. They use exact `want.resolved.json` (provenance trees encode
`{"s":text,"prov":tag}`), `want.diag.json`, and optional `want.requests.json`,
`want.events.json` and `want.fields.json`.

Implemented:

- Accepted queries request once at `finish()` in name order, up to
  `max_concurrent_requests`; the rest queue and pump on delivery. Dependency
  changes debounce for 300 ms through `advance_clock`. A delivery is accepted only
  for the current `(stmt, args_hash)`; stale, unknown and cross-epoch ids are
  dropped. Results are tagged `Tool`, capped by `max_resolved_data_bytes`, and
  `query.denied`/`query.failed` keep the default or last good value. Request status
  is exposed via `query_status`.
- `ref()` validates `[A-Za-z0-9_.:-]{1,128}`, stays a `Ref` placeholder before
  delivery, and requests only catalog ids that reach a prop. Delivered data is
  tagged `Host`; denial/failure is `ref.unknown`, size or
  `max_ref_resolutions` overflow is `ref.too_large`, and a nested ref is
  `ref.invalid`. No resolution happens before delivery.
- `UiEvent::{Activate,Edit,Blur}` and `deliver` are the runtime entries. `say`,
  `open`, declared custom actions and `refresh` become epoch-bound
  `HostRequest`s; `say`/custom actions also produce a `ModelEvent` with the
  activated node's visible label. `limit.event` truncates by dropping `ctx` then
  trimming text. `open` enforces provenance and the host URL allowlist
  (`expr.tainted_url`). Host effects are never dispatched internally.
- Field registry by nearest `Form`/page scope with the §2.14 fixed check order;
  failed submit marks scope fields touched and runs no step. A submit button runs
  its own `on_click` first, then the form's `on_submit`. Field status is exposed
  by canonical node path; the renderer props remain M6 work.
- Resolved trees are unchanged for existing categories; runtime fixtures compare
  the retained tree and provenance fixtures compare the tagged projection. Stream
  equality, provisional isolation and the absence of streaming host effects are
  still checked by the shared oracle.

Bugs found while authoring fixtures (fixed with the fixture present): debounce was
restarted on every clock advance; delivered values did not mark the wire tree
dirty; multiple ref rejections re-requested the same id forever; `say` after `set`
used the activated node's statement for the form's `on_submit`; `fmt` lost mixed
provenance; oversized events trimmed visible text before context.

Gates: `cargo fmt`, workspace Clippy `-D warnings`, `cargo test --workspace`
(88 core + 47 conformance + 5 std + 4 CLI/quality tests and empty doc targets),
`--features serde_json/preserve_order`, 404 fixtures / 96,248 chunkings / 430
seeded permutations, `genui-cli fixtures`, and `spec-check` (3 programs, 4
fragments, 24 exemplars). `query.denied`, `query.failed`, `ref.too_large` and
`limit.event` moved from deferred to covered; only `lib.mismatch` (M5) and
`render.panic` (M6) remain deferred.

Remaining M4 work: `each`/node-producing `if`/`coalesce` instantiation, synthetic
`Status`/field props on the wire, `ToolDescriptor`-driven `auto_run`/argument
shapes, URL policy for tool arguments, form-scope rendering semantics at M6, and
fuzz/mutation acceptance for this changed runtime source. M3's I7/review state is
unchanged by this slice.
