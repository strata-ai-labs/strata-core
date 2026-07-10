# L7M Implementation Plan: Generated Fuzz And Fault Assurance

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L7/l7m-generated-fuzz-fault-assurance-test-plan.md`

## Objective

L7A through L7L built the commit-runtime behavior with focused unit and
integration tests. L7M adds assurance depth over that behavior:

1. generated multi-branch operation scripts;
2. an independent commit-runtime reference model;
3. distinct fuzz targets and checked-in seed corpora;
4. richer fault-window scripts for cache, durable, replay, timeline, conflict,
   and quiesce behavior.

L7M is not a new production feature slice. It should not change the commit
protocol unless a bug is discovered while building the assurance harness. L7N
owns final closeout inventory, sensitivity-probe evidence, and the full command
matrix.

## Inputs

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L7/l7a-commit-runtime-scaffold-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L7/l7b-commit-batch-mutation-model-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L7/l7c-version-and-timestamp-clocks-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L7/l7d-outcomes-visibility-read-only-implementation-plan.md`
8. `docs/architecture/implementation-plans/M4/L7/l7e-branch-registry-commit-guards-implementation-plan.md`
9. `docs/architecture/implementation-plans/M4/L7/l7f-conflict-validation-implementation-plan.md`
10. `docs/architecture/implementation-plans/M4/L7/l7g-commit-timeline-substrate-implementation-plan.md`
11. `docs/architecture/implementation-plans/M4/L7/l7h-cache-no-wal-commit-path-implementation-plan.md`
12. `docs/architecture/implementation-plans/M4/L7/l7i-wal-record-envelope-integration-implementation-plan.md`
13. `docs/architecture/implementation-plans/M4/L7/l7j-durable-but-not-visible-classification-implementation-plan.md`
14. `docs/architecture/implementation-plans/M4/L7/l7k-recovery-replay-allocator-catch-up-implementation-plan.md`
15. `docs/architecture/implementation-plans/M4/L7/l7l-concurrency-quiesce-hardening-implementation-plan.md`
16. `crates/storage-next/src/commit/`
17. `crates/storage-next/src/testkit/commit_runtime*.rs`
18. `crates/storage-next/tests/commit_runtime_properties.rs`
19. `crates/storage-next/tests/commit_runtime_faults.rs`
20. `crates/storage-next/fuzz/Cargo.toml`
21. `crates/storage-next/fuzz/fuzz_targets/`
22. `crates/storage-next/fuzz/corpus/`

## Current State

The existing L7 testkit has useful category helpers:

1. `check_commit_runtime_scaffold_contract` exercises many direct categories;
2. `CommitRuntimeScaffoldOutcome` exposes counters for each slice;
3. direct cache, durable, replay, guard, conflict, and timeline tests cover
   concrete edge cases;
4. `commit_runtime_faults.rs` currently proves only a shallow fault scaffold;
5. no commit-runtime fuzz targets are registered yet;
6. the generated property test still validates category presence more than
   end-to-end operation semantics.

L7M should keep the existing scaffold route as a smoke contract, but add a
script-driven model route that can catch correlated mistakes across several
commit phases.

## Scope

L7M implements:

1. a bounded commit-runtime operation-script decoder;
2. an independent model for branch lifecycle, visible version, version/timestamp
   allocation, per-branch visible rows, timeline facts, unresolved durable gate,
   and guard/quiesce state;
3. a production harness that executes the same operation script against
   crate-private L7 runtime types;
4. invariant checks after every script step;
5. generated property tests over multi-branch scripts;
6. distinct testkit contract functions for batch, conflict, durable, and
   timeline fuzz targets;
7. commit-runtime fuzz target registration and seed corpora;
8. richer fault-window tests that execute protocol boundaries rather than only
   checking that a `FaultScript` value can be constructed;
9. porting-log evidence for what L7M covers and what remains for L7N.

L7M does not implement:

1. public storage APIs;
2. public transaction/session objects;
3. L8 process-open recovery orchestration;
4. real backend crash/reopen harnesses;
5. branch clear/delete public commands;
6. closeout inventory tests;
7. sensitivity-probe mutation evidence. L7N owns the final ledger.

## File Layout

Keep testkit files below the reviewable size threshold by splitting generated
support into focused modules.

Preferred layout:

```text
crates/storage-next/src/testkit/commit_runtime.rs
crates/storage-next/src/testkit/commit_runtime_model.rs
crates/storage-next/src/testkit/commit_runtime_script.rs
crates/storage-next/src/testkit/commit_runtime_runner.rs
crates/storage-next/src/testkit/commit_runtime_faults.rs
crates/storage-next/src/testkit/commit_runtime_fuzz.rs
```

The existing `commit_runtime_allocator.rs`, `commit_runtime_cache.rs`,
`commit_runtime_conflicts.rs`, `commit_runtime_durable.rs`,
`commit_runtime_timeline.rs`, `commit_runtime_outcome.rs`, and
`commit_runtime_branch_guards.rs` should remain as direct category helpers.
L7M should compose them; it should not fold everything into one large file.

If module churn would be too large for one patch, use the flat file names above
without creating a nested directory. The important boundary is logical
separation, not the exact module path.

## Script Model

Add a bounded script representation:

```text
CommitRuntimeScript
  branches: 1..=8
  operations: 0..=64
  max mutations per operation: bounded by CommitRuntimeConfig
```

Recommended operations:

1. `RegisterBranch(branch)`
2. `MarkDeleting(branch)`
3. `RecreateBranch(branch)`
4. `CachePut(branch, key, value, timestamp_policy, validation_mode)`
5. `CacheDelete(branch, key, timestamp_policy, validation_mode)`
6. `DurablePut(branch, key, value, durability, fault_point)`
7. `DurableDelete(branch, key, durability, fault_point)`
8. `ReadOnlyDiagnostic(branch)`
9. `ReadFactCommit(branch, key, observed_version)`
10. `CasCommit(branch, key, observed_version)`
11. `BeginQuiesce`
12. `ReleaseQuiesce`
13. `AcquireBranchGuard(branch)`
14. `ReleaseBranchGuard(branch)`
15. `ReplayWalRecord(branch, version, replay_mode, fault_point)`
16. `AssertVisible(branch, key)`
17. `AssertTimeline(branch, timestamp_or_version)`

The decoder should consume arbitrary bytes deterministically and clamp sizes.
Malformed or impossible script choices should become skipped operations or typed
expected errors, not panics.

## Independent Model

Add a model that is deliberately simpler than production:

```text
ModelCommitRuntime
  registry: branch -> lifecycle/generation
  branch_rows: branch -> key -> versioned row facts
  timeline: branch -> [(timestamp, version)]
  allocator_last_version
  timestamp_floor
  visible_version
  unresolved_durable: Option<ModelUnresolvedDurable>
  guard_state: active branch guards + quiesce flag
```

Model rules:

1. one mutating commit allocates exactly one version and one timestamp;
2. read-only diagnostics do not allocate;
3. cache commits apply rows and publish visible only after conflict validation;
4. durable commits append logically before apply and visible publication;
5. clean WAL failures do not apply and do not record an unresolved durable fact;
6. uncertain WAL failures do not apply and remain not-visible;
7. durable success followed by L6 apply failure records durable-not-applied;
8. durable success followed by visible publication failure records
   applied-not-visible;
9. unresolved durable state blocks normal cache and durable mutating commits;
10. replay may apply already-durable rows idempotently and catch up allocator
    floors;
11. visible reads see only rows at or below `visible_version`;
12. timeline timestamp lookup returns the greatest retained timestamp at or
    before the requested timestamp, with commit version as the tiebreaker.

The model must not call production `CommitCacheRuntime`, `CommitDurableRuntime`,
`CommitReplayRuntime`, `CommitTimelineView`, or conflict validation helpers to
derive expected results. It may reuse primitive value types like `BranchId`,
`CommitVersion`, `Timestamp`, `PhysicalKey`, and `StorageRow` only as data
containers.

## Production Runner

Add a runner that applies the same script to real L7 internals:

1. `CommitBranchRegistry`
2. `CommitBranchGuardSet`
3. `CommitFactAllocator`
4. `VisibleVersionTracker`
5. `CommitUnresolvedDurableGate`
6. one `BranchLocalState` per model branch
7. fake `CommitWalAppender` for durable outcomes
8. fake apply/visible surfaces for injected faults where necessary

After each operation, compare production and model:

1. visible version;
2. allocator last version and timestamp floor;
3. unresolved durable gate state;
4. branch registry lifecycle/generation facts;
5. visible latest row per touched key;
6. timeline lookups for touched timestamps and versions;
7. expected error phase for rejected operations;
8. absence of user value bytes in error display/debug text;
9. guard/quiesce open/blocked state.

## Fault Points

Define a commit-runtime fault enum in the testkit, separate from backend fault
scripts:

```text
CommitRuntimeFaultPoint
  None
  TimestampSourceUnavailable
  WalCleanFailure
  WalWriterHalted
  WalSegmentIdOverflow
  WalUncertainFailure
  BranchApplyFailureAfterWal
  VisiblePublishFailureAfterApply
  ReplayApplyFailure
  ReplayVisiblePublishFailure
  GateRecordFailure
  GateClearFailure
```

Faults should be injected through narrow fake traits already used by L7I-L7K
tests. Do not add sleeps, threads, async runtimes, filesystem IO, or backend
direct calls to commit testkit.

## Fuzz Targets

Add these targets to `crates/storage-next/fuzz/Cargo.toml`:

1. `commit_runtime_batch`
2. `commit_runtime_conflict`
3. `commit_runtime_durable`
4. `commit_runtime_timeline`

Each target must call a distinct testkit contract:

```text
check_commit_runtime_batch_contract(data)
check_commit_runtime_conflict_contract(data)
check_commit_runtime_durable_contract(data)
check_commit_runtime_timeline_contract(data)
```

The targets may share lower-level script decoders, but each must exercise a
different behavioral surface and must not route through only
`check_commit_runtime_scaffold_contract`.

Add seed corpora:

```text
crates/storage-next/fuzz/corpus/commit_runtime_batch/
crates/storage-next/fuzz/corpus/commit_runtime_conflict/
crates/storage-next/fuzz/corpus/commit_runtime_durable/
crates/storage-next/fuzz/corpus/commit_runtime_timeline/
```

Each corpus should include at least:

1. a happy cache put/delete script;
2. a stale read-set conflict script;
3. a durable WAL clean failure script;
4. a durable post-WAL L6 apply failure script;
5. a visible-publication failure script;
6. a replay duplicate exact-match script;
7. duplicate timestamp/timeline tiebreak script;
8. a quiesce/guard contention script.

## Implementation Steps

### L7M-A: Inventory And Split Testkit

1. Inventory current `commit_runtime*.rs` helpers and line counts.
2. Add new modules for script, model, runner, faults, and fuzz contracts.
3. Keep existing direct category helpers intact.
4. Export new testkit functions only under the existing `testkit` feature.

Exit gate: testkit compiles with no production public API changes.

### L7M-B: Script Decoder

1. Add bounded byte-to-script decoding.
2. Clamp branch count, operation count, key length, value length, and mutation
   count.
3. Ensure empty input still produces a minimal deterministic script or a typed
   no-op contract result.
4. Add counters for decoded operations by category.

Exit gate: decoder tests prove arbitrary bytes cannot panic or allocate beyond
configured bounds.

### L7M-C: Independent Model

1. Implement `ModelCommitRuntime`.
2. Implement cache, durable, read-only, conflict, timeline, quiesce, guard, and
   replay model transitions.
3. Implement model visible read and timeline lookup helpers.
4. Keep model errors phase-shaped but independent of production error variants.

Exit gate: model unit tests cover each transition without production runtime
calls.

### L7M-D: Production Runner And Oracle

1. Build real runtime fixtures from script branch set.
2. Execute each script step against production and model.
3. Compare post-step state and expected phase.
4. Record detailed counters for success, rejection, and fault routes.
5. Keep failure messages bounded and value-free.

Exit gate: generated property harness uses the model runner, not only static
category helpers.

### L7M-E: Fault Harness Expansion

1. Replace shallow `commit_runtime_faults.rs` checks with scripted protocol
   faults.
2. Cover every fault point listed above.
3. Assert guard release, gate behavior, visible-version behavior, and allocator
   gap behavior for each phase.
4. Retain direct L7 unit tests as precise regression tests.

Exit gate: `commit_runtime_faults.rs` fails if fault scripts stop reaching the
phase-specific code paths.

### L7M-F: Fuzz Contracts And Corpora

1. Add four fuzz target files.
2. Register all targets in fuzz `Cargo.toml`.
3. Add checked-in seed corpora.
4. Add contract tests or source guards that ensure each fuzz target calls its
   distinct contract function.
5. Keep each fuzz target small and panic-free.

Exit gate: target registration, distinct routing, and seed directories are
visible before L7N closeout.

### L7M-G: Property Harness Upgrade

1. Update `commit_runtime_properties.rs` to run script-model contracts.
2. Keep the current category-counter assertion as an additional smoke check.
3. Use a bounded proptest case count that is useful locally and acceptable for
   CI.
4. Add failure persistence for generated script regressions.

Exit gate: property tests assert both category coverage and model parity.

### L7M-H: Porting Log

Record in `m4-l7-porting-log.md`:

1. generated script categories implemented;
2. independent model boundaries;
3. fuzz targets and seed corpora added;
4. fault windows covered;
5. commands run;
6. items intentionally left for L7N.

## Required Counters

Add counters to a generated outcome type for at least:

1. decoded scripts;
2. cache commits attempted/succeeded/rejected;
3. durable commits attempted/succeeded/rejected;
4. replay attempts/successes/rejections;
5. read-only diagnostics;
6. read-set conflicts;
7. CAS conflicts;
8. branch lifecycle rejections;
9. guard/quiesce contentions;
10. timeline rows and lookups;
11. WAL clean failures;
12. WAL uncertain failures;
13. writer-halted failures;
14. segment-overflow failures;
15. post-WAL apply failures;
16. visible-publish failures;
17. unresolved durable gate blocks;
18. model-production parity checks.

## Source Boundaries

L7M testkit and fuzz code may import crate-private commit test helpers behind
`testkit` or test targets. Production `src/commit/` boundaries must not change:

1. no `pub` crate-root commit runtime API;
2. no backend/layout/filesystem imports in production commit code;
3. no product, Hub, graph, search, vector, embedding, or API DTO vocabulary;
4. no direct sleeps, threads, async runtimes, wall-clock timeout logic, or
   process-global mutable state.

## Review Risks

1. Do not let the model call production helper code for expected results.
2. Do not create a giant `commit_runtime.rs`; split modules before size becomes
   unreviewable.
3. Do not make generated tests so expensive that normal CI becomes unusable.
4. Do not add arbitrary runtime sleeps or actual thread scheduling.
5. Do not treat fuzz-target file presence as sufficient; route each target to a
   distinct contract.
6. Do not encode document-link tests. Tests should prove storage behavior and
   harness coverage.

## Exit Gate

L7M is complete when:

1. generated scripts execute against both model and production;
2. property tests prove model parity across cache, durable, conflict, timeline,
   quiesce, guard, and replay operations;
3. direct fault harness covers every L7 fault phase;
4. fuzz targets are registered, distinct, and seeded;
5. source guards still pass;
6. no production public API is introduced;
7. porting log records coverage and remaining L7N closeout work.
