# Progress

Updated: 2026-10-22. Milestones follow SPEC §14.1 in order. Update this file after
**every milestone**, recording acceptance evidence and any remaining limitations.
A milestone is complete only after its acceptance checks pass and CI is green.

| Milestone | Status | Evidence / next gate |
| --- | --- | --- |
| M0 — Workspace and verification scaffolding | Complete | 21 lex fixtures / 4,743 chunkings; 7 spec examples; Linux and macOS CI green. |
| M1 — Batch syntax | Complete | 124 fixtures / 28,225 chunkings; 23 tests including 2,048 generated round trips; four clean 600-second fuzz campaigns; Linux/macOS CI green. |
| M2 — Library, validation, IR and limits | Follow-up in progress | 277 fixtures / 62,884 chunkings / 400 permutations; 52 tests; 24 exemplars; final validator ASan 601s / 71,760 executions / exit 0; Linux/macOS source CI green. |
| M3 — Streaming and patches | Not started | Live invariant checks, patch replay, 100k generated programs. |
| M4 — Runtime | Not started | Evaluation and interactive fixture categories. |
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
