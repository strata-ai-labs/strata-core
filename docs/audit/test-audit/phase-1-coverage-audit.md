# Phase 1 Coverage Audit

Audit date: 2026-09-02

Worktree: `strata-core-test-coverage-audit`

Base reviewed: `origin/main` at `8f0bf36e`

Source log: `docs/architecture/v1-test-coverage-program.md`

STH ledger: `docs/architecture/implementation-plans/storage-testing/README.md`

## Executive Verdict

Phase 1 is materially closed. The STH machinery exists, is CI-tiered, and is
stronger than the pre-program audit state: the recovery oracle, systematic fault
sweeps, filesystem persistence models, deterministic simulation, compound
failure harness, write-ordering watchdog, config differential, liveness matrix,
sanitizer/Miri/fuzz/coverage lanes, mutation-on-diff gate, charter guard, and
leak registry are all present in the audited tree.

It is not a clean "nothing to fix" closeout. The remaining work is focused:

1. The main Phase 1 ledger overstates STH-5 if read literally: it names
   quarantine as a compound-fault maintenance surface, but the STH-5 compound
   maintenance sequence covers flush, checkpoint, compaction, and snapshot
   pruning.
2. STH-7 process-gate wording is stale. The original 73.0% workspace coverage
   floor has been superseded by Phase 3 product-only per-crate floors, and the
   fuzz target count has grown beyond the documented 28/30-target prose.
3. The anti-drift guard is useful but intentionally shallow: it checks that
   cited artifacts exist, not that slice status text, counts, or scope promises
   remain semantically true. This audit found exactly the sort of content drift
   that the guard cannot catch.

Practical status: keep Phase 1 as closed, but file the cleanup as targeted
Phase 1 closeout hygiene instead of reopening the whole phase. If "quarantine
compound faults" was genuinely part of the intended Phase 1 exit bar, add that
small STH-5 increment before calling the Phase 1 ledger exact.

## Slice Status

| Slice | Audited status | Scope verdict | Remaining work |
|---|---|---|---|
| 1.1 STH-7a | Closed | Implemented correctly | Keep Miri engine coverage and per-test allocator counters as accepted headroom unless risk changes. |
| 1.2 STH-5 | Implemented with scope correction required | Correct for recovery and core maintenance publish faults | Add quarantine compound-fault coverage, or amend the main ledger to say snapshot pruning rather than quarantine. |
| 1.3 STH-3b | Closed with documented fidelity limit | Implemented correctly for watched per-call backend streams | Revisit persistent append-handle coverage if that path becomes default in watched durable tests. |
| 1.4 STH-6 | Closed with documented scope split | Implemented correctly for deterministic config differential and liveness matrix | Update stale plan text: policy axis is fixed to `EvaluateAndEnqueue`; background scheduler liveness is separate. |
| 1.5 STH-7 full | Closed with accepted deferrals | CI gates are real, but not SQLite-style full MC/DC/full-tree mutation | Update stale floor/count prose; keep MC/DC, `testcase!` macros, DB-file fuzz, and full-tree mutation as explicit headroom. |
| 1.6 Doc repair | Mostly closed | STH headers/index were repaired | Add semantic/content drift checks or periodic audits; existence-only guard is insufficient for status truth. |
| 1.7 Leak-registry migration | Closed | Implemented correctly | No Phase 1 gap found. Continue requiring new fixture leaks to use the registry. |

## Phase 1 Findings

### P1 - STH-5 quarantine is claimed in the main ledger but not evidenced in the compound-fault harness

Scope: Phase 1.2 says failure-during-failure should inject faults during
"recovery, compaction, checkpoint, quarantine"
(`docs/architecture/v1-test-coverage-program.md:120`).

Current evidence:

- The STH-5 as-built says the implementation is `compound_faults.rs` plus
  `tests/compound_faults.rs`, with a recovery sweep and maintenance publish
  cases (`docs/architecture/implementation-plans/storage-testing/sth-5-failure-during-failure-implementation-plan.md:5`).
- The source header says the maintenance transitions are "flush, checkpoint,
  compaction, snapshot pruning" (`crates/storage/src/testkit/compound_faults.rs:1`).
- The actual maintenance sequence is exactly `Flush`, `Checkpoint`, `Compact`,
  and `SnapshotPruning` (`crates/storage/src/testkit/compound_faults.rs:62`).
- The compound maintenance runner faults the traced write-side positions inside
  that sequence and then verifies typed surfacing plus in-session/reopen resume
  (`crates/storage/src/testkit/compound_faults.rs:626`).
- `tests/compound_faults.rs` exposes the per-PR integration test and the ignored
  soak (`crates/storage/tests/compound_faults.rs:57`,
  `crates/storage/tests/compound_faults.rs:84`).
- Quarantine is covered by other Phase 1 adjacent lanes: the STH-6 liveness
  matrix includes `MaintenanceTask::Quarantine`
  (`crates/storage/src/api/tests/liveness_matrix.rs:12`), and recovery has
  quarantine-specific tests. That is not the same as "inject a second fault
  during quarantine maintenance publication."

Verdict: STH-5 is implemented correctly for the as-built scope, but the main
Phase 1 row is not exact. Treat this as either a documentation bug or a missing
micro-slice.

Fix split:

- TCP1.STH5-Q-A: decide whether the intended Phase 1 surface is snapshot
  pruning or quarantine.
- TCP1.STH5-Q-B: if quarantine is intended, extend the compound maintenance
  harness with a quarantine-producing setup and a traced quarantine publish
  sequence.
- TCP1.STH5-Q-C: add non-vacuity counters proving the quarantine task fired,
  the fault fired inside the quarantine transition, the failure surfaced typed,
  and reopen/resume remains oracle-valid.

### P1 - STH-7 status prose is stale against current CI

Scope: Phase 1.5 closed STH-7 with a coverage ratchet, mutation-on-diff,
nightly fuzzing, traceability, and anti-drift guard
(`docs/architecture/v1-test-coverage-program.md:123`).

Current evidence:

- The STH-7 as-built still says the nightly coverage job gates a 73.0% workspace
  region floor (`docs/architecture/implementation-plans/storage-testing/sth-7-test-process-gates-implementation-plan.md:44`).
- Current CI runs the coverage job, but the gate is now the Phase 3
  per-crate, product-only script (`.github/workflows/nightly.yml:271`,
  `.github/workflows/nightly.yml:297`).
- `scripts/coverage_floors.py` explicitly says it is a product-only Phase 3
  tracker and lists per-crate floors, not a single 73.0% workspace floor
  (`scripts/coverage_floors.py:1`, `scripts/coverage_floors.py:22`).
- The STH-7 plan says scheduled fuzzing runs all 30 targets, and the older
  design section says 28 targets
  (`docs/architecture/implementation-plans/storage-testing/sth-7-test-process-gates-implementation-plan.md:52`,
  `docs/architecture/implementation-plans/storage-testing/sth-7-test-process-gates-implementation-plan.md:135`).
- The current fuzz workflow enumerates targets dynamically with
  `cargo +nightly fuzz list`, so new targets join automatically
  (`.github/workflows/fuzz.yml:40`). The checked-out tree currently has 38 Rust
  fuzz target files under `crates/storage/fuzz/fuzz_targets/`.
- The nightly workflow's own coverage comment still labels the job
  "baseline only," while the same job now runs the product-only floor gate
  (`.github/workflows/nightly.yml:271`, `.github/workflows/nightly.yml:297`).

Verdict: the implementation is better than the old STH-7 prose, but the status
ledger should not preserve obsolete numeric claims. This is documentation debt,
not a missing test lane.

Fix split:

- TCP1.STH7-DOC-A: rewrite Phase 1.5 and the STH-7 as-built to say "coverage
  publication started in Phase 1; Phase 3 replaced the workspace floor with
  per-crate product-only floors."
- TCP1.STH7-DOC-B: replace fixed fuzz-target counts with "all targets reported
  by `cargo fuzz list`" and optionally record the current count as observed, not
  normative.
- TCP1.STH7-DOC-C: update the nightly coverage comments so they match the
  actual floor-gate step.

### P2 - The anti-drift guard is artifact-existence only

Scope: Phase 1.5/1.6 says the anti-drift guard prevents the map from lying
(`docs/architecture/v1-test-coverage-program.md:123`,
`docs/architecture/v1-test-coverage-program.md:131`).

Current evidence:

- The guard's own module docs state that it parses backticked repository paths
  and asserts cited artifacts exist (`crates/storage/tests/testing_charter_guard.rs:1`).
- It explicitly says it checks existence, not content
  (`crates/storage/tests/testing_charter_guard.rs:10`).
- It also has one reasoned allowed-missing anchor for a reorganized product doc
  (`crates/storage/tests/testing_charter_guard.rs:26`).
- The implemented-slice test pins primary artifacts for STH-1 through STH-7
  (`crates/storage/tests/testing_charter_guard.rs:210`).

Verdict: the guard is appropriate as a deletion/rename tripwire. It is not a
semantic status guard. It cannot detect stale counts, superseded coverage
floors, a narrowed policy axis, or the quarantine/snapshot-pruning mismatch.

Fix split:

- Add a small structured ledger for STH slice status, current CI job names,
  current fuzz-target count, coverage-gate type, and known accepted deferrals.
- Keep the existing path-existence guard; do not make it parse prose.
- Add a separate status-lint test only for machine-readable ledger fields.

### P2 - Phase 1 still has accepted "world-class headroom"

Scope: STH-7 compared the target bar to SQLite-style MC/DC, mutation testing,
sanitizers, leak checks, and continuous fuzzing.

Current evidence:

- The STH-7 as-built explicitly says the coverage gate is not MC/DC and that
  mutation is diff-scoped rather than a full-tree campaign
  (`docs/architecture/implementation-plans/storage-testing/sth-7-test-process-gates-implementation-plan.md:49`).
- The current mutation gate is valuable but cost-bounded: PR diff only, with
  additional package-specific runs for executor and inference
  (`.github/workflows/ci.yml:194`, `.github/workflows/ci.yml:260`).
- `.cargo/mutants.toml` now contains significant justified exclusions for
  loom-gated models, DST simulation harnesses, dual-mutation harnesses, IPC
  glue, and local-gated inference paths (`.cargo/mutants.toml:24`,
  `.cargo/mutants.toml:58`).
- A repository search found no implemented `testcase!`, `always!`, or `never!`
  macros outside the Phase 1 prose.

Verdict: this is acceptable because the plan itself records those limitations.
Do not let Phase 1 be summarized as full SQLite-equivalent MC/DC/full-tree
mutation coverage.

Fix split:

- Keep MC/DC, full-tree mutation campaigns, and test-case macros in a visible
  "not implemented by Phase 1" ledger.
- If pursuing release-grade certification, require a dated full-tree mutation
  campaign and reasoned equivalent-mutant report, separate from the per-PR
  mutation-on-diff gate.

## Foundation: STH-1 Through STH-4

Phase 1 is named "Close STH-1..7 properly," but the 1.x table mostly lands
STH-5, STH-3b, STH-6, and STH-7. I audited the STH-1 through STH-4 foundations
because the later slices compose them.

### STH-1 - Recovery Oracle

Scope: after any crash point in a durable workload, recovered state must be a
prefix of acknowledged history: no lost acknowledged commit outside the allowed
damaged suffix, no phantom, no torn batch, and no gap.

Implemented: Yes. The plan header records the implemented oracle artifacts and
the repaired stale header (`docs/architecture/implementation-plans/storage-testing/sth-1-recovery-oracle-implementation-plan.md:3`).
The verifier has typed `LostAck`, `Phantom`, `TornBatch`, and `Gap` violations
(`crates/storage/src/testkit/recovery_oracle/verify.rs:27`) and finds a matching
watermark against the model before accepting recovery
(`crates/storage/src/testkit/recovery_oracle/verify.rs:119`). The integration
test exercises clean drop, WAL-tail damage, and corruption and asserts
non-vacuity (`crates/storage/tests/crash_recovery_oracle.rs:1`,
`crates/storage/tests/crash_recovery_oracle.rs:36`).

Correctness verdict: implemented correctly. This is the right foundational
oracle and is reused by later STH lanes.

Remaining aspects: no Phase 1 blocker. Future multi-branch durable maintenance
and cross-version recovery semantics are later-phase re-entry surfaces, not
STH-1 implementation misses.

### STH-2 - Systematic Fault Sweeps

Scope: fail the Nth backend operation over V1-reachable write-side operations,
in once and continuous modes, then verify through STH-1. Include ENOSPC and
budget pressure.

Implemented: Yes. The as-built narrows the V1-reachable sweep to append, sync,
publish, and delete, with delete reached through snapshot pruning
(`docs/architecture/implementation-plans/storage-testing/sth-2-fault-injection-sweeps-implementation-plan.md:17`).
The source encodes the same operation set and explains why write, conditional,
read, list, and metadata faults are not part of the STH-2 write-path sweep
(`crates/storage/src/testkit/fault_sweep/mod.rs:39`). The integration target
asserts traced backend operations, swept positions, and budget-pressure
non-vacuity (`crates/storage/tests/fault_sweep.rs:36`,
`crates/storage/tests/fault_sweep.rs:51`). The ignored soak scales with
`STRATA_STORAGE_FAULT_CASES` (`crates/storage/tests/fault_sweep.rs:71`), and
nightly runs it (`.github/workflows/nightly.yml:175`).

Correctness verdict: implemented correctly for the documented V1 write-path
scope. Read/list/metadata recovery faults were intentionally moved to the later
TCP3.3b recovery-read lane, which is reasonable.

Remaining aspects: no Phase 1 blocker.

### STH-3 - Durability Realism

Scope: model non-friendly filesystems, enumerate persistence/crash models, and
add the write-ordering watchdog that proves dependent publishes do not beat WAL
durability.

Implemented: Yes. STH-3a/3c landed in June and STH-3b landed as Phase 1.3
(`docs/architecture/implementation-plans/storage-testing/sth-3-durability-realism-implementation-plan.md:3`).
The FS model tests exercise ordered/atomic, reordered appends, garbage tail, and
split rename against both durability policies
(`crates/storage/tests/fs_persistence_models.rs:1`). The watchdog is a pure
observer over append/sync/publish/delete order and checks manifest, snapshot,
and table publishes (`crates/storage/src/testkit/write_ordering_watchdog.rs:1`,
`crates/storage/src/testkit/write_ordering_watchdog.rs:87`). The public
integration test drives real watched sessions under Always and Standard
durability and asserts non-vacuity (`crates/storage/tests/write_ordering.rs:23`,
`crates/storage/tests/write_ordering.rs:71`).

Correctness verdict: implemented correctly. The watchdog has good non-vacuity
and explicitly accounts for the byte-level observer ambiguity around in-flight
append tails.

Remaining aspects: the known limitation is real but documented: persistent
`BackendAppendHandle` appends bypass decorators, so watched runs use the
per-call fallback path
(`docs/architecture/implementation-plans/storage-testing/sth-3-durability-realism-implementation-plan.md:22`).
This is not a Phase 1 miss unless production/watched durable tests rely on
persistent append handles as the primary path.

### STH-4 - Deterministic Simulation

Scope: a seeded explorer over the production background/lifecycle path, with
replayable trajectories, safety/liveness checks, and fault/crash combinations.

Implemented: Yes. The as-built records the production-path driver, clock hook,
bit-exact facts, fault/crash dimension, and the two found-and-fixed durability
bugs (`docs/architecture/implementation-plans/storage-testing/sth-4-deterministic-simulation-implementation-plan.md:60`).
The clean simulation test asserts seeds execute, maintenance completes, and the
manual clock advances (`crates/storage/tests/simulation_smoke.rs:19`). The
fault simulation test asserts both fault and crash cases execute, at least one
fault fires, and at least one crash perturbs disk state
(`crates/storage/tests/simulation_faults.rs:19`). Nightly runs both ignored
soaks (`.github/workflows/nightly.yml:183`).

Correctness verdict: implemented correctly for Phase 1's prerequisite role.

Remaining aspects: 4a timing-clock cleanup was explicitly descoped because the
facts exclude wall-clock durations. Later whole-DB simulation expansion belongs
to Phase 4, not Phase 1.

## Detailed Slice Audit

### 1.1 - STH-7a Cheap Half

Scope: add Miri and ASAN/LSAN jobs, publish a `cargo-llvm-cov` baseline, and run
the three test-suite legs: debug assertions, release, and coverage.

Implemented: Yes. `nightly.yml` has Miri over `strata-core` and the storage
format layer (`.github/workflows/nightly.yml:22`). It has ASAN/LSAN over storage
and engine (`.github/workflows/nightly.yml:53`) and TSAN over storage, engine,
and executor (`.github/workflows/nightly.yml:84`). The coverage job publishes
lcov, summary, per-file output, and floor artifacts
(`.github/workflows/nightly.yml:271`). The release-mode leg runs
`cargo test --locked --release --workspace`
(`.github/workflows/nightly.yml:326`).

Correctness verdict: closed. The implementation matches the cheap-half intent
and was later widened beyond the original storage-only TSAN wording.

More aspects to cover: engine under Miri remains documented headroom, and the
per-test allocator-counter idea was replaced by whole-process LSAN. That is an
acceptable trade for Phase 1.

### 1.2 - STH-5 Failure During Failure

Scope: inject a second failure during recovery and maintenance transitions, then
prove typed failure surfacing, STH-1 oracle validity, and resumability.

Implemented: Yes for the as-built STH-5 scope. Recovery uses a faulted
checkpoint publish followed by a crash, traces every backend operation recovery
actually invokes, and sweeps second-fault positions
(`crates/storage/src/testkit/compound_faults.rs:1`,
`crates/storage/src/testkit/compound_faults.rs:41`). Maintenance faults every
write-side backend position reached by flush, checkpoint, compaction, and
snapshot pruning, then verifies typed surfacing and resume
(`crates/storage/src/testkit/compound_faults.rs:54`,
`crates/storage/src/testkit/compound_faults.rs:626`). The nightly lane runs both
the compound grid and a 20,000-case soak
(`.github/workflows/nightly.yml:120`, `.github/workflows/nightly.yml:140`).

Correctness verdict: implemented correctly for recovery and the four maintenance
publish transitions in source. The harness has the right shape: dynamic tracing,
once/continuous faults, typed-failure checks, non-vacuity guards, and STH-1
oracle verification.

More aspects to cover: the main Phase 1 row's literal "quarantine" surface is
not covered by this compound sequence. Add it or correct the ledger.

### 1.3 - STH-3b Write-Ordering Watchdog

Scope: prove no manifest/table/snapshot publish becomes durable before the WAL
bytes it depends on have a durability event.

Implemented: Yes. The watchdog tracks WAL segment appends, syncs, whole-segment
publishes, dependent publishes, pending/exonerated publishes, and typed
violations (`crates/storage/src/testkit/write_ordering_watchdog.rs:1`,
`crates/storage/src/testkit/write_ordering_watchdog.rs:106`). The integration
test opens real watched local-fs sessions under Always and Standard durability,
drives commits plus flush/checkpoint/compact/snapshot-pruning, and asserts both
no violations and non-vacuous WAL/publish observations
(`crates/storage/tests/write_ordering.rs:23`,
`crates/storage/tests/write_ordering.rs:68`). Nightly runs the target
(`.github/workflows/nightly.yml:134`).

Correctness verdict: closed. The implementation correctly avoids false
confidence by asserting that the watchdog observed WAL appends, checked
publishes, and saw durability events.

More aspects to cover: persistent append-handle paths still bypass decorators.
Keep this as a tracked harness-fidelity limitation.

### 1.4 - STH-6 Differential And Liveness

Scope: run a seeded workload across storage configurations, assert identical
logical results, add metamorphic checks, and deepen liveness across modes and
maintenance kinds.

Implemented: Yes with a documented scope split. The config differential runs
six mode/budget cells: cache, durable Standard, durable Always crossed with
default and low-memory budgets (`crates/storage/src/testkit/config_differential.rs:79`).
The run compares logical snapshots and model equality
(`crates/storage/src/testkit/config_differential.rs:451`). It also has a
non-ignored low-memory pressure regression asserting retries are invisible to
readers (`crates/storage/src/testkit/config_differential.rs:530`). The liveness
matrix includes all 11 public maintenance kinds, including quarantine
(`crates/storage/src/api/tests/liveness_matrix.rs:11`), across cache, durable
Standard, and durable Always (`crates/storage/src/api/tests/liveness_matrix.rs:118`).
Nightly runs the config differential soak and the perf-trace background
scheduler lane (`.github/workflows/nightly.yml:146`).

Correctness verdict: closed for Phase 1. The issue #2609 regression is live, and
the liveness matrix covers every public maintenance kind at deterministic
`EvaluateAndEnqueue` scheduling.

More aspects to cover: the draft objective and exit gate still say the config
differential covers every mode, policy, and budget
(`docs/architecture/implementation-plans/storage-testing/sth-6-differential-and-liveness-implementation-plan.md:46`,
`docs/architecture/implementation-plans/storage-testing/sth-6-differential-and-liveness-implementation-plan.md:124`).
The as-built fixes policy to `EvaluateAndEnqueue` for determinism, while the
background scheduler is covered separately. Update the doc so this does not read
as a hidden unimplemented policy axis.

### 1.5 - STH-7 Full Process Gates

Scope: coverage/mutation gates, nightly persistent-corpus fuzzing,
requirements-to-test traceability, anti-drift guard, and testcase-style macros.

Implemented: Yes with accepted deferrals. The per-PR mutation-on-diff job exists
and runs `cargo mutants --in-diff` on PRs (`.github/workflows/ci.yml:194`,
`.github/workflows/ci.yml:234`). The fuzz workflow is scheduled, restores a
corpus cache, enumerates all current fuzz targets dynamically, and uploads crash
artifacts (`.github/workflows/fuzz.yml:8`, `.github/workflows/fuzz.yml:33`,
`.github/workflows/fuzz.yml:40`). The coverage baseline/floor job exists
(`.github/workflows/nightly.yml:271`). The charter guard exists and pins
artifact presence for implemented STH claims
(`crates/storage/tests/testing_charter_guard.rs:100`,
`crates/storage/tests/testing_charter_guard.rs:210`).

Correctness verdict: closed with accepted deferrals. The machinery is real and
appropriately tiered. The limitations are not hidden in code: STH-7 explicitly
defers MC/DC and full-tree mutation, and the repo still lacks the
`testcase!`/`always!`/`never!` macro layer named in the main Phase 1 scope.

More aspects to cover: update stale coverage/fuzz wording, keep the accepted
headroom visible, and consider a periodic full-tree mutation certification lane
if this program later needs release-grade evidence beyond per-PR diff mutation.

### 1.6 - Doc Repair

Scope: repair stale STH status headers, split mapped versus built in the
gold-standard delta, and update the STH README status column.

Implemented: Mostly. The STH index now declares the program complete and lists
each STH slice as done with dates (`docs/architecture/implementation-plans/storage-testing/README.md:3`,
`docs/architecture/implementation-plans/storage-testing/README.md:35`). STH-1's
header explicitly records the old stale-header issue as repaired
(`docs/architecture/implementation-plans/storage-testing/sth-1-recovery-oracle-implementation-plan.md:3`).

Correctness verdict: closed for the original stale-header repair. However, the
current audit found later content drift that the Phase 1 guard cannot catch:
coverage gate wording, fuzz counts, STH-6 ignored-regression wording, and the
STH-5 quarantine/snapshot-pruning mismatch.

More aspects to cover: add a machine-readable status ledger or recurring audit
check for the few volatile facts that prose cannot safely own.

### 1.7 - Storage Leak-Registry Migration

Scope: replace intentional storage fixture leaks with a registry helper so the
nightly ASAN/LSAN lane can run with leak detection enabled without drowning in
known benign leaks.

Implemented: Yes. `leak_static` uses `Box::leak` but stores each leaked address
in a process-global registry so LSAN considers the allocation reachable
(`crates/storage/src/testkit/leak.rs:1`). `forget_registered` handles the
single destructor-skipping fixture pattern (`crates/storage/src/testkit/leak.rs:26`).
The storage testkit re-exports both helpers (`crates/storage/src/testkit/mod.rs:166`).
The nightly ASAN/LSAN storage job documents that bare `Box::leak` or
`mem::forget` in storage test code should fail the lane
(`.github/workflows/nightly.yml:68`). A current search found no bare
`Box::leak` or `mem::forget` use in storage tests/source outside the helper
itself, and found 632 registered helper uses.

Correctness verdict: closed. This is a well-scoped implementation: fixture
leaks remain intentional and visible to reviewers, while new unregistered leaks
are not masked.

More aspects to cover: no Phase 1 gap found.

## Recommended Cleanup Split

1. TCP1-DOC-REFRESH: update Phase 1 and STH-7 prose for the current coverage
   gate, fuzz target enumeration, STH-6 pressure regression status, and the
   anti-drift guard's existence-only scope.
2. TCP1-STH5-QUARANTINE: either add compound quarantine-fault coverage or amend
   the Phase 1 row to say the implemented maintenance fault surfaces are flush,
   checkpoint, compaction, and snapshot pruning.
3. TCP1-LEDGER-GUARD: add a structured STH status metadata file and a cheap
   guard for volatile facts: job names, fuzz-target count source, coverage-gate
   type, accepted deferrals, and any allowed missing evidence anchors.
4. TCP1-MUTATION-CERT: optional release-grade headroom. Run and publish a dated
   full-tree mutation campaign for the storage surfaces, with killed/survived/
   timeout/equivalent counts, instead of relying solely on mutation-on-diff.
