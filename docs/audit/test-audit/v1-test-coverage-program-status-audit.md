# V1 Test Coverage Program Status Audit

Audit date: 2026-09-02
Worktree: `/home/anibjoshi/Documents/GitHub/strata-core-test-coverage-audit`
Branch: `audit/test-coverage-status`
Audited base: `origin/main` at `8f0bf36e`

Scope: status of `docs/architecture/v1-test-coverage-program.md` slices against
the checked-out tree, CI workflow wiring, and commit history. This audit did not
rerun the expensive suites; it is a repository-evidence audit.

## Status Vocabulary

- Closed: implementation, CI tier, and closure criteria have landed, with no
  blocking open pin found in the audited tree.
- Closed with accepted deferral: implementation is landed, and the remaining
  absence is explicitly deferred with a valid re-entry condition.
- Implemented with open pins: harness exists, but it is intentionally pinning
  known bad behavior. The slice can be mechanically useful but cannot support a
  Phase 4 exit claim until the pins are resolved or explicitly accepted.
- Partial: a named increment landed, but the slice's own stated scope is not
  complete.
- Planned: no implementation commit or live CI/tooling evidence found.
- Certification blocker: a gap that prevents calling the program release-grade,
  even if many individual slices are implemented.

## Executive Verdict

| Phase | Audited Status | Reason |
|---|---|---|
| Phase 1 | Closed with focused cleanup | The focused audit in `docs/audit/phase-1-coverage-audit.md` supersedes the coarse table below: the STH machinery and CI tiers are real, but the main ledger overstates STH-5 quarantine compound-fault coverage, STH-7 coverage/fuzz prose is stale, and the anti-drift guard is artifact-existence only. |
| Phase 2 | Closed with accepted deferrals | The focused audit in `docs/audit/phase-2-coverage-audit.md` supersedes the coarse table below: slices 2.1-2.7 landed, 2.8 was reasonably resolved as not-a-bug, and the remaining actionable work is targeted cleanup: CLI clone-over-HTTP at the binary layer, hermetic inference runtime-cache lifecycle coverage, close/cache/background flush-runner harmonization or an explicit retained deferral, and ledger cleanup. |
| Phase 3 | Implemented with certification cleanup required | The layer-by-layer closeout through TCP3.15 is backed by explicit commits and artifacts, and TCP3.16 also landed as a post-exit addendum. The focused audit in `docs/audit/phase-3-coverage-audit.md` supersedes the coarse table below: the accepted numeric exit criteria were not reconciled with the actual closeout, the debt ledgers have grown since the Phase 3 close point, the branch-merge absence story is stale, and the TCP3.14 wasm bundle-size budget is missing. |
| Phase 4 | Not closed | Many 4.x lanes are real and high-value, but Phase 4 is not certifiable: 4.1b is missing, 4.7 still has active product divergences, 4.8 and 4.9 pin known bad behavior, 4.12 is partial, the mutation-kill plateau is not published, and the promised release-tag soak is absent from `release.yml`. |
| Phase 5 | Partial | 5.1 and 5.2 are live; 5.3-5.6 remain planned/chartered only. |

## Program-Level Findings

1. P0 - The status ledger is self-contradictory and stale. The header says the
   last program slice was TCP4.11c and that no program slice merged after
   2026-07-30 (`docs/architecture/v1-test-coverage-program.md:3`,
   `docs/architecture/v1-test-coverage-program.md:6`), but `origin/main`
   contains later TCP4.12, TCP4.13, TCP4.14, TCP4.15, TCP5.1, TCP5.2, and TCP5.6
   commits. The same header also says every planned Phase 4 lane landed while
   listing non-blocking headroom on 4.1, 4.7, and 4.9
   (`docs/architecture/v1-test-coverage-program.md:17`,
   `docs/architecture/v1-test-coverage-program.md:24`,
   `docs/architecture/v1-test-coverage-program.md:27`). Fix: rewrite the
   program header and slice table from the audited state below, not from the
   paused 2026-08-31 snapshot.

2. P0 - The Phase 4 release leg promised by the charter is not implemented. The
   charter requires a pre-release soak on the tag across generated corpora,
   config matrix, and fault schedules wired into `release.yml`
   (`docs/architecture/v1-test-coverage-program.md:367` through
   `docs/architecture/v1-test-coverage-program.md:374`). The actual release
   workflow runs only the frozen-format and capability-contract gate before
   packaging (`.github/workflows/release.yml:16` through
   `.github/workflows/release.yml:30`). Fix: add a release-tag certification
   job or change the charter if the organization intentionally does not want
   release-tag soak coverage.

3. P0 - Phase 4 cannot exit while shrink-only pins for product defects remain.
   4.7's divergence ledger says it is a bug ledger, not an escape hatch
   (`crates/executor/idl/v1/cross-surface-divergences.yaml:1` through
   `crates/executor/idl/v1/cross-surface-divergences.yaml:10`) and still lists
   #2694, #2695, #2700, #2701, #2702, and #2704. 4.8 still pins #2749/#2750 in
   the error-contract harness (`crates/executor/tests/error_contract.rs:18`
   through `crates/executor/tests/error_contract.rs:23`). 4.9 still pins #2754
   snapshot-object misclassification and #2567 recovery-budget violation
   (`crates/executor/tests/artifact_faults.rs:363`,
   `crates/executor/tests/artifact_faults.rs:404`,
   `crates/engine/tests/recovery_budget.rs:180`). Fix these before treating the
   suite as release-grade.

4. P1 - The deferred register has re-entry conditions that have fired. The
   branch-ops row says merge/compare/promote tests are deferred because the
   operations are unimplemented (`docs/architecture/v1-test-coverage-program.md:662`),
   but branch diff, merge/promote, and preview are now in the IDL and executor
   dispatch (`crates/executor/idl/v1/commands/branch.yaml:32`,
   `crates/executor/idl/v1/commands/branch.yaml:46`,
   `crates/executor/idl/v1/commands/branch.yaml:75`,
   `crates/executor/src/executor/dispatch.rs:87`,
   `crates/executor/src/executor/dispatch.rs:107`). Engine guards also state
   merge left the forbidden list while cherry-pick and revert remain forbidden
   (`crates/engine/tests/dependency_guards.rs:799` through
   `crates/engine/tests/dependency_guards.rs:804`). Fix: split the row into
   landed ops needing audited coverage versus still-deferred ops.

5. P1 - The cross-version metamorphic harness should re-enter. The deferred
   register says it is meaningless until a second V1 tag exists
   (`docs/architecture/v1-test-coverage-program.md:671`); the repo now has
   `v1.0.0`, `v1.1.0`, and `v1.1.1`. Fix: charter and wire the harness, likely
   on top of the 4.2 generated/corpus machinery.

6. P2 - Legacy shell CLI suites have drifted and should not be used as coverage
   evidence until repaired. The charter calls out
   `scripts/cli-tests/08_time_travel.sh` as still using `event len`
   (`docs/architecture/v1-test-coverage-program.md:39`), and the script still
   calls it at `scripts/cli-tests/08_time_travel.sh:51`,
   `scripts/cli-tests/08_time_travel.sh:52`, and
   `scripts/cli-tests/08_time_travel.sh:58`. Other shell corpora also contain
   `event len`; these should either be migrated to `event count` or retired in
   favor of the Rust CLI tests.

## Phase 1 Status

Note: this table is retained as a broad inventory only. The authoritative Phase
1 status is now `docs/audit/phase-1-coverage-audit.md`, which audits every 1.x
slice against current source and separates implementation gaps from stale
status wording and accepted headroom.

| Slice | Audited Status | Notes |
|---|---|---|
| 1.1 STH-7a | Closed | Nightly Miri and ASAN/LSAN jobs exist (`.github/workflows/nightly.yml:22`, `.github/workflows/nightly.yml:53`), and coverage publication/flooring exists (`.github/workflows/nightly.yml:271`). |
| 1.2 STH-5 | Implemented with scope correction required | Compound fault testkit and nightly soak are present (`crates/storage/src/testkit/compound_faults.rs`, `.github/workflows/nightly.yml:120`), but the focused audit found no compound-fault quarantine sequence matching the main ledger's literal wording. |
| 1.3 STH-3b | Closed | Write-ordering watchdog and nightly step are present (`crates/storage/src/testkit/write_ordering_watchdog.rs`, `.github/workflows/nightly.yml:134`). |
| 1.4 STH-6 | Closed with documented scope split | Storage config differential exists and is run nightly (`crates/storage/src/testkit/config_differential.rs`, `.github/workflows/nightly.yml:146`); the policy axis is fixed to `EvaluateAndEnqueue`, with background scheduler liveness covered separately. |
| 1.5 STH-7 full | Closed with accepted deferrals | Diff-scoped mutation gate is wired in per-PR CI (`.github/workflows/ci.yml:194`), scheduled fuzz exists, and full-tree mutation/MC/DC are explicitly deferred; coverage/fuzz count wording is stale. |
| 1.6 Doc repair | Mostly closed | Original STH stale headers were repaired, but the existence-only charter guard does not catch semantic status drift. |
| 1.7 Leak-registry migration | Closed | ASAN/LSAN comments reference the leak registry and storage test code routes fixture leaks through `leak_static` (`.github/workflows/nightly.yml:68`, `crates/storage/src/testkit/leak.rs:17`). |

## Phase 2 Status

Note: this table is retained as a broad inventory only. The authoritative Phase
2 status is now `docs/audit/phase-2-coverage-audit.md`, which audits every 2.x
slice against current source and separates true gaps from accepted deferrals.

| Slice | Audited Status | Notes |
|---|---|---|
| 2.1 Process-level crash harness | Closed | `process_crash` testkit and 200-round nightly soak exist (`crates/storage/src/testkit/process_crash.rs`, `.github/workflows/nightly.yml:136`). |
| 2.2 CI tiers | Closed | Nightly soak lanes, scheduled fuzz, wasm test execution, and release format gate exist. The later Phase 4 pre-release soak is a separate missing gate, not a 2.2 miss; old fixed lane/target counts are stale. |
| 2.3 CLI integration suite | Closed with headroom | Rust real-binary CLI integration tests exist; legacy shell suites still drift and should not be counted. CLI clone-over-real-HTTP remains open at the binary layer. |
| 2.4 Engine branch concurrency races | Closed | Reachable race/lock cases landed. #2618 is fixed in the audited tree; the old loom/shuttle deferral was later superseded by Phase 4.3. |
| 2.5 Inference testkit | Closed with open runtime-cache follow-up | `FakeInferenceEngine`, offline download tests, and the per-PR inference lane exist. Executor deterministic dispatch was later closed by TCP3.9c; hermetic real-runtime cache fill/status/unload lifecycle coverage remains open. |
| 2.6 Small zero-coverage surfaces | Closed with headroom | Wasm and `stratadb` facade tests exist, CLI remote null-origin rendering exists, and hub clone has real HTTP coverage below CLI. CLI clone-over-real-HTTP remains open at the binary layer. |
| 2.7 Multi-branch orphaned-delta recovery | Closed with accepted deferral | Guard-plus-adversarial coverage landed; the per-branch durable-maintenance fix remains deferred. |
| 2.8 Close-time flush surfaces | Resolved not-a-bug with residual | Saturated close/reopen is pinned and no production drain-before-close flush producer was found. Close/cache/background flush runners still use direct rotate-budget guards, so the residual harmonization row should stay visible. |

## Phase 3 Status

Note: this table is retained as a broad inventory only. The authoritative Phase
3 status is now `docs/audit/phase-3-coverage-audit.md`, which corrects several
stale slice labels below and audits every 3.x slice against the accepted plan.

| Slice | Audited Status | Notes |
|---|---|---|
| 3.0 Tracking machinery | Closed | Product-only coverage floors and workspace error-code guard landed. |
| 3.1 Core | Closed | Wire goldens, adversarial decode, hash/boundary, and doc-parity guard landed. |
| 3.2a | Closed | Storage inner-error boundary plumbing and branch coverage landed. |
| 3.2b | Closed | Remaining inner-error surfaces landed. |
| 3.2c | Closed | Error-source/source-chain guard landed. |
| 3.3a | Closed | Storage codec fuzz and layer-fuzz presence guard landed. |
| 3.3b | Closed | Storage lower-layer negative paths landed. |
| 3.3c | Closed | Storage L9 negative paths and method-presence guard landed. |
| 3.3d | Closed | Storage lifecycle error-code residuals landed. |
| 3.4a | Closed | Storage timeline model tests landed. |
| 3.4b | Closed | Storage threaded COW race tests landed. |
| 3.5a | Closed | Error-contract reconciliation and class-parity guard landed. |
| 3.5b | Closed | Engine graph/vector refusal batches landed. |
| 3.5c | Closed | Engine JSON/event/space refusal batches landed. |
| 3.5d | Closed | Engine persistence/hub residuals landed. |
| 3.6 | Closed | Executor plan and closure path landed. |
| 3.6a | Closed | IDL inventory and overlay guard landed. |
| 3.6b-i | Closed | Executor KV/vector coverage landed. |
| 3.6b-ii | Closed | Executor JSON/event/graph/space/admin coverage landed. |
| 3.6c | Closed as not-applicable | Coverage target proved unnecessary or merged into other slices per charter. |
| 3.7 | Closed, later re-entered | Original absence guard was valid for then-unimplemented branch ops. Current branch diff/merge/preview surfaces mean the deferred row must be split and re-audited. |
| 3.8a | Closed | Executor error-case fixture format and replay path landed. |
| 3.8b | Closed | Executor replay coverage ratchet and fixtures landed. |
| 3.8c | Closed | Executor error residuals landed. |
| 3.9 | Closed | Inference residual plan landed. |
| 3.9a | Closed | Download/cache failure coverage landed. |
| 3.9b | Closed | Registry/local model coverage landed. |
| 3.9c-i | Closed | Runtime cache lifecycle leg landed. |
| 3.9c-ii | Closed | Executor inference fake-service leg landed. |
| 3.10 | Closed | CLI residual plan landed. |
| 3.10a | Closed | CLI render coverage landed. |
| 3.10b | Closed | CLI verb enumeration and render result-type guards landed. |
| 3.10c | Closed | CLI doctor/config behavior landed. |
| 3.11a | Closed | CLI KV/vector family coverage landed. |
| 3.11b | Closed | CLI event/graph family coverage landed, including the `event count` rename in Rust tests. |
| 3.11c | Closed | CLI arrow, pipe, raw, command-run, and inference verb coverage landed. |
| 3.12 | Closed | Inference request goldens and wire mapping landed. |
| 3.13 | Closed | Hub clone fault/transport coverage landed. |
| 3.14 | Closed | Wasm and `stratadb` residuals landed. |
| 3.15 | Closed | Engine corruption injection and Phase 3 exit gate landed. |
| 3.15a | Closed | Corruption infra plus KV/JSON landed. |
| 3.15b | Closed | Graph corruption landed. |
| 3.15c | Closed | Vector corruption, reopen, and IO landed. |
| 3.15d | Closed | Control-plane corruption and final Phase 3 exit landed. |
| 3.16 | Closed | Addendum closed CLI doctor/storage lifecycle mapping after the Phase 3 exit. |

## Phase 4 Status

| Slice | Audited Status | Notes |
|---|---|---|
| 4.1 IDL conformance generator | Partial | 4.1a landed: generator emits 498 tests across four families and CI runs `check-tests` (`crates/executor/src/idl_tooling/tests_gen.rs:1`, `.github/workflows/ci.yml:64`). The 4.1b items remain: render-mode goldens, schema-guided adversarial mutations beyond unknown keys, REPL/pipe text round-trip for #2571, and boundary-value generation (`docs/architecture/v1-test-coverage-program.md:414`). |
| 4.2 Differential testing vs reference DBs | Implemented with scope headroom | KV/RocksDB/Redis, JSON/MongoDB, graph/Neo4j, event/Redis Streams, vector/exact-kNN landed and nightly jobs exist (`.github/workflows/nightly.yml:365` through `.github/workflows/nightly.yml:427`). Committed replay corpora exist for JSON/graph/event/vector and replay per-PR (`crates/executor/tests/corpus_replay.rs:1`). Remaining headroom is corpus growth and deeper branch/time-travel metamorphic sequences. |
| 4.3 Exhaustive concurrency-schedule exploration | Closed, doc row stale | The row starts "In progress" but includes 4.3a-c and says #2682 closed (`docs/architecture/v1-test-coverage-program.md:416`). Per-PR loom CI is wired (`.github/workflows/ci.yml:170`). |
| 4.4 Vendored public conformance suites | Closed, doc row stale | The row still says "#3024 pinned open" (`docs/architecture/v1-test-coverage-program.md:417`), but `graph_conformance.rs` says CDLP was promoted from the #3024 pin after the synchronous-propagation fix (`crates/executor/tests/graph_conformance.rs:351`). |
| 4.5 Golden and combinatorial matrix expansion | Closed, doc row stale | The row starts "In progress" but later says 4.5 complete; commits TCP4.5a/b/c and the nightly config-invariance debug/release soaks exist (`.github/workflows/nightly.yml:231`, `.github/workflows/nightly.yml:340`). |
| 4.6 Committed fuzz corpora and surface expansion | Closed | Value fidelity, codec round-trip fuzz seam, dual-mutation, and corpus harvest landed. Scheduled fuzz enumerates targets through `cargo fuzz list`, and `dual_mutation` is now a fuzz target. |
| 4.7 Cross-surface parity harness | Implemented with open pins | Harness and ledger exist, but active divergence entries remain for #2694, #2695, #2700, #2701, #2702, and #2704 (`crates/executor/idl/v1/cross-surface-divergences.yaml:11` through `crates/executor/idl/v1/cross-surface-divergences.yaml:75`). This blocks Phase 4 exit unless each divergence is fixed or formally accepted as product contract. |
| 4.8 Error-contract correctness harness | Implemented with open pins | Harness exists and is valuable, but it still pins #2749 and #2750 (`crates/executor/tests/error_contract.rs:18` through `crates/executor/tests/error_contract.rs:23`, `crates/executor/tests/error_contract.rs:480`, `crates/executor/tests/error_contract.rs:639`). |
| 4.9 Fault-taxonomy extension and health-vs-truth oracle | Partial with open pins | 4.9a/b landed, #2690 sole-WAL loss is now converted into permanent contracts, and multi-segment WAL loss exists at the storage seam (`crates/storage/src/testkit/wal_segment_loss.rs:95`). Still open: #2754 snapshot-object transient misclassification, #2567 recovery budget ignored, durable-table/snapshot family fault coverage, runtime fault health-vs-truth, and continuous randomized crash evolution. |
| 4.10 Single-system logic-bug oracles | Implemented with explicit observable gap | PQS/TLP/NoREC/DQE/graph MRs landed, but inert-index detection needs an explain/stats observable (`crates/executor/tests/oracle_deopt.rs:6` through `crates/executor/tests/oracle_deopt.rs:10`). Treat QPG as deferred until explain exists. |
| 4.11 Deterministic whole-DB simulation harness | Closed | DST substrate, whole-DB multi-epoch harness, seed replay, and nightly whole-DB soak landed (`.github/workflows/nightly.yml:201`). Residual hardening items are tracked as non-blocking in the charter. |
| 4.12 History-based isolation and lineage checking | Partial | 4.12a and 4.12b landed with nightly lineage/concurrent-history soaks (`.github/workflows/nightly.yml:206`, `.github/workflows/nightly.yml:218`). The slice still names full Adya SSG cycle inference and faulted concurrent histories as headroom (`docs/architecture/v1-test-coverage-program.md:425`). |
| 4.13 Stress-as-fuzzing and sanitizer breadth | Closed | Shared-database stress lane, expanded TSan, and debug-assert guard landed. Nightly TSan covers storage, engine, and executor (`.github/workflows/nightly.yml:84`). |
| 4.14 Scheduling-predicate composition guard | Closed | Checkpoint structural-deferral registry and guard landed. |
| 4.15 Backend concurrent-mutator audit | Closed | LocalFs contract table and race tests landed. |

## Phase 5 Status

| Slice | Audited Status | Notes |
|---|---|---|
| 5.1 Liveness budgets on existing lanes | Closed | `ProgressWatchdog` exists (`crates/storage/src/testkit/progress_watchdog.rs:1`), and nightly soak steps now carry explicit timeouts (`.github/workflows/nightly.yml:168`). Remaining adoption is incremental for future lanes. |
| 5.2 Deterministic per-PR gates | Closed for instruction count and binary size | Bench targets exist in `benchmarks/benches`, `perf_floors.py` carries instruction ceilings (`scripts/perf_floors.py:1`), per-PR `perf-gates` run Callgrind (`.github/workflows/ci.yml:137`), and nightly release-mode binary-size ratchet exists (`.github/workflows/nightly.yml:358`). Allocation-count ratchet is deliberately deferred to 5.6. |
| 5.3 Same-runner A/B relative gate | Planned | No TCP5.3 implementation commit or workflow found. |
| 5.4 Nightly macro trend lane | Planned | No trend JSONL/artifact lane found. |
| 5.5 Release-leg comparative and GPU | Planned | No release comparative or GPU gate found. |
| 5.6 Characteristic metrics | Planned/chartered | TCP5.6 charter commit exists, but no implementation commits or gates found. |

## Fix Backlog

### A. Repair the Program Ledger

- Rewrite the top status block with the actual state as of 2026-09-02.
- Replace "no program slice has merged since TCP4.11c" with the actual later
  slice list.
- Reconcile "every planned Phase 4 lane has landed" with the missing 4.1b work,
  open 4.7/4.8/4.9 pins, and 4.12 headroom.
- Update stale 4.3, 4.4, and 4.5 row labels.
- Fix the Phase 3 slice-count wording to acknowledge 3.16 and the lettered
  sub-slices.
- Reconcile Phase 3's accepted numeric exit criteria with the actual closeout:
  current error-code allowlist size, current coverage floors, and the fact that
  replay/debt ledgers can grow unless historical budgets are added.
- Update TCP3.7 to reflect the current branch surface: preview and promote have
  landed, while the absence guard now protects only cherry-pick/revert.
- Add or explicitly defer the TCP3.14 wasm bundle-size budget.

### B. Phase 4 Certification Blockers

- Add or intentionally remove the release-tag pre-release soak promised by the
  Phase 4 charter.
- Publish the Phase 4 mutation-kill plateau review by gap class; current
  per-PR mutation-on-diff is useful but is not evidence of plateau across every
  Phase 4 class.
- Close or explicitly reclassify 4.7 divergence ledger entries #2694, #2695,
  #2700, #2701, #2702, and #2704.
- Close #2749 and #2750 in the public error contract.
- Close #2754 and #2567 in the fault/recovery surfaces.

### C. 4.1b Generator Completion

- Add render-mode goldens for JSON/raw/human.
- Add schema-guided mutations beyond unknown-key insertion.
- Add REPL/pipe text round-trip coverage for #2571; this likely needs the
  Command-to-argv emitter named in the charter.
- Add boundary-value generation from IDL schema metadata.

### D. Fault and Simulation Expansion

- Complete 4.9 durable-table and snapshot-object fault families.
- Add health-vs-truth checks after runtime faults, not only artifact faults.
- Evolve the process-crash harness into the continuous randomized crash tier
  promised by 4.9.
- Complete 4.12 Adya SSG and faulted concurrent-history checks.

### E. Re-entry Work

- Split the branch-op deferred row: compare/diff, preview, and promote/merge are
  landed and need audited coverage; cherry-pick, revert, restore/copy/undo can
  remain deferred if still absent.
- Start the cross-version metamorphic harness now that multiple V1 tags exist.
- Repair or retire the drifted shell CLI suites that still use `event len`.

### F. Phase 5 Remaining Work

- Implement 5.3 same-runner A/B relative perf gates in advisory mode first.
- Implement 5.4 nightly macro trend artifacts and sustained-drift alarms.
- Implement 5.5 release-leg comparative/GPU checks if they remain part of the
  release standard.
- Implement 5.6 characteristic metrics and the allocation-count ratchet audit.
