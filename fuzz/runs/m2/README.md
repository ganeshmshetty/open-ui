# M2 validator campaigns

`validator.json` and `validator.log` are final acceptance on source 2bd18eb:
601 engine seconds, 71,760 executions, exit 0, AddressSanitizer, matching source
hashes and 884 MiB peak RSS. The 282 initial corpus files include fixtures,
exemplars, nullable Probe combinations and a 1,200-statement container chain.
The t4.people exemplar correction was added while the corpus was live.

`nullable-checkpoint-validator.*` records a clean 601-second run on 3144164
before the final static-data/fallback correction. It is not final acceptance.
`interrupted-validator.*` records a deliberate interruption on eb8250b after
415,105 executions, libFuzzer exit 72. It is excluded from acceptance; no crash
was observed. The early 21-second smoke in PROGRESS.md is also not acceptance.

JSON files preserve commands, tool versions, source/log SHA-256 hashes and full
local log paths. Adjacent logs retain startup and completion excerpts. Corpus
mutations and full logs remain local. See PROGRESS.md and docs/milestones/M2.md.
