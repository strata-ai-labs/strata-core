# M4-L8 Test Plan: Lifecycle, Recovery, Maintenance

Status: test-suite plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

## Goal

Prove that storage-next L8 orchestrates open, recovery, maintenance, retention,
quarantine, and close over lower storage layers without importing product
lifecycle semantics.

The suite must fail if L8:

1. creates durable objects before capability validation succeeds;
2. lets cache mode claim crash recovery or create durable recovery objects;
3. opens durable storage from corrupt or incomplete state as healthy;
4. loses, duplicates, or invents durable WAL records during recovery;
5. bootstraps L7 clocks or visible version from untrusted facts;
6. truncates WAL without a typed durable retention proof;
7. deletes or purges objects without a current reachability proof;
8. reports flush, checkpoint, or manifest publication uncertainty as clean
   success;
9. runs maintenance tasks after close begins unless the task is part of close
   drain/sync;
10. deadlocks close, quiesce, or maintenance drain;
11. exposes product open policy, IPC, follower mode, primitive reconstruction, or
   user-facing recovery wording;
12. imports engine/product/StrataHub concepts into production lifecycle code.

M4-L8 is tested in the same four parts used by the implementation plan:

1. **L8-Core: Open + Recovery**
   Proves lifecycle state, open plans, capability validation, cache/durable
   open/create, recovery orchestration, recovery health, and L7 bootstrap.
2. **L8-Maintenance + Checkpoint**
   Proves deterministic maintenance tasks, flush, checkpoints, WAL truncation,
   compaction/materialization scheduling, and storage pressure facts.
3. **L8-Reclaim + Close + Assurance**
   Proves retention, quarantine, purge, repair, close, generated/fault/crash
   coverage, source guards, and closeout.
4. **L8-Durable Tables + Storage Hardening**
   Proves durable table manifests, table-object reachability, table-manifest
   backed watermarks, durable rewrite publication, row pruning, memory budgets,
   lazy reads, branch lifecycle completion, and commit hardening.

Each part should be independently closeable. Later parts may add stronger
generated or crash coverage over earlier parts, but they must not weaken earlier
exit gates.

## Testing Principles

1. Test storage lifecycle facts, not product open UX.
2. Use storage-shaped rows, manifests, snapshots, WAL records, table objects, and
   reachability facts.
3. Every side effect must be ordered and observable in the test harness.
4. Every failure must classify by lifecycle phase.
5. Recovery expected state must come from an independent model or explicit
   durable fixture, not from production recovery output.
6. Maintenance tests should use a deterministic single-threaded executor first.
7. Crash tests should cover durable publication boundaries, not only API return
   values.
8. Retention and purge tests must prove safety, not only successful deletion.
9. Source guards are part of the suite, not advisory documentation.
10. Closeout inventory should check implementation assurance, not whether plan
    documents link to each other.

## Test Harness Layout

Recommended locations:

1. `crates/storage-next/src/lifecycle/` for small module-local tests.
2. `crates/storage-next/src/lifecycle/tests/` for larger direct suites.
3. `crates/storage-next/src/testkit/lifecycle/` for generated model, operation
   scripts, fake services, and crash/fault helpers.
4. `crates/storage-next/tests/lifecycle_recovery.rs` for open/recovery tests.
5. `crates/storage-next/tests/lifecycle_maintenance.rs` for flush/checkpoint and
   maintenance executor tests.
6. `crates/storage-next/tests/lifecycle_reclaim_close.rs` for retention,
   quarantine, purge, repair, and close tests.
7. `crates/storage-next/tests/lifecycle_faults.rs` for fault-window tests.
8. `crates/storage-next/tests/crash_recovery.rs` for local filesystem
   crash/reopen tests, with slow cases marked `#[ignore]`.
9. `crates/storage-next/tests/lifecycle_properties.rs` for generated L8
   conformance properties.
10. `crates/storage-next/tests/lifecycle_source_guard.rs` for production boundary
    scans.
11. `crates/storage-next/tests/lifecycle_closeout.rs` for closeout inventory and
    command evidence.
12. `crates/storage-next/fuzz/fuzz_targets/lifecycle_recovery.rs` for corrupted
    recovery input ordering.
13. `crates/storage-next/fuzz/fuzz_targets/lifecycle_maintenance.rs` for task
    ordering and fault scripts.
14. `crates/storage-next/fuzz/fuzz_targets/lifecycle_retention.rs` for retention
    and quarantine proof scripts.

Required regression files:

1. `crates/storage-next/proptest-regressions/lifecycle.txt`, created only when a
   failing generated case is captured.
2. `crates/storage-next/fuzz/corpus/lifecycle_*` seed directories for each fuzz
   target.

## Part Gates

### Part 1: L8-Core

L8-Core closes when direct and generated tests prove:

1. lifecycle module/source boundaries;
2. lifecycle state transitions;
3. open plan and outcome validation;
4. storage-mode capability validation before side effects;
5. cache-mode open/close with durable-service absence;
6. durable local open/create with service assembly;
7. recovery from empty database, manifest+WAL, checkpoint-only, and
   checkpoint+WAL tail;
8. strict corruption/degradation classification;
9. L7 replay/bootstrap from recovered facts;
10. storage-shaped recovery health.

Core tests may use fake L4/L6/L7 services where the target is ordering. At least
one local filesystem integration path should be added before Part 1 closes.

### Part 2: L8-Maintenance + Checkpoint

L8-Maintenance + Checkpoint closes when direct and fault tests prove:

1. maintenance task queue ordering, coalescing, drain, and cancel behavior;
2. flush converts frozen L6 state to L5 table artifacts and durable L4 objects;
3. flush watermarks advance only with proof;
4. checkpoint quiesces commits through L7 and writes row-native snapshot facts;
5. manifest snapshot watermark update is ordered after snapshot publication;
6. WAL truncation requires typed retention proof;
7. compaction/materialization scheduling calls lower-layer hooks without owning
   algorithms;
8. maintenance failures produce typed health debt.

Part 2 tests should stay deterministic. Threaded/background maintenance can be
added after deterministic contracts are stable.

### Part 3: L8-Reclaim + Close + Assurance

L8-Reclaim + Close + Assurance closes when direct, generated, fuzz, crash, and
closeout tests prove:

1. retention keeps objects when proof is incomplete;
2. snapshot pruning preserves live manifest snapshot and configured newest
   snapshots;
3. quarantine inventory is durable before purge eligibility;
4. purge requires a fresh safe proof;
5. unsafe degraded recovery blocks reclaim;
6. repair/reconciliation reports facts without inventing state;
7. close stops commits, drains/cancels maintenance, quiesces L7, syncs durable
   services, and releases backend guards;
8. close is idempotent after `Closed` and retryable after typed timeout;
9. generated/fuzz/crash tests cover all lifecycle phase families;
10. source guards enforce layer boundaries;
11. sensitivity probes are recorded;
12. the closeout command matrix passes.

### Part 4: L8-Durable Tables + Storage Hardening

L8-Durable Tables + Storage Hardening closes when direct, generated, fuzz, and
closeout tests prove:

1. durable table-manifest bytes are canonical, versioned, checksummed, and
   primitive-neutral;
2. table-manifest publication and recovery rebuild table reachability without
   trusting orphan table objects;
3. table-object retention uses manifest-backed proof and does not report
   unsupported scopes as clean success;
4. flush watermarks and WAL truncation can rely on table-manifest coverage only
   when the proof is complete;
5. durable compaction/materialization publication has typed fault windows;
6. row pruning preserves as-of, history, TTL, tombstone, and inherited-layer
   guarantees;
7. memory/cache budgets bound storage-owned memory under low-memory profiles;
8. lazy object-backed reads avoid whole-table loading;
9. branch lifecycle create/list/delete/clear/fork-at-history/generation behavior
   is complete enough for L9;
10. commit hardening gaps are closed or explicitly deferred with stable tests.

## Reference Model

Use an independent lifecycle model. Do not derive expected results by reading
production lifecycle runtime state.

Suggested shape:

```text
ModelLifecycle {
  state: ModelLifecycleState
  mode: ModelStorageMode
  durable_objects: ModelObjectInventory
  manifest: Option<ModelManifest>
  snapshots: Vec<ModelSnapshot>
  wal_records: Vec<ModelWalRecord>
  branch_state: ModelBranchState
  commit_clock: ModelCommitClock
  visible_version: CommitVersion
  recovery_health: ModelRecoveryHealth
  maintenance_queue: Vec<ModelMaintenanceTask>
  quarantine: ModelQuarantineInventory
  retained_objects: BTreeSet<ModelObjectName>
}

ModelObjectInventory {
  manifests
  wal_segments
  snapshots
  table_objects
  table_manifests
  quarantine_inventory
  temp_objects
}

ModelMaintenanceTask {
  kind
  priority
  coalescing_key
  lower_layer_effects
  outcome
}
```

The model must:

1. separate lifecycle state from durable object inventory;
2. represent cache mode as no durable recovery claim;
3. represent durable records, snapshots, and manifests as independent facts;
4. distinguish allocated, durable, applied, visible, checkpointed, flushed, and
   retained versions;
5. model recovery health independently from production health classification;
6. keep retention proof separate from deletion;
7. model quarantine as a state before purge;
8. model close as an ordered sequence with retryable timeouts;
9. preserve lower-layer ownership by using abstract L4/L6/L7 facts instead of
   production internals.

## Generators

### Open Plan Generator

Generate:

1. cache mode;
2. durable standard;
3. durable always;
4. object durable candidate;
5. missing backend capabilities;
6. extra backend capabilities;
7. codec mismatch;
8. unknown codec id;
9. valid/invalid recovery strictness knobs;
10. lifecycle hook/fault configuration.

### Durable Inventory Generator

Generate object inventories with:

1. no manifest;
2. valid manifest and empty WAL;
3. manifest with checkpoint only;
4. manifest with checkpoint and WAL tail;
5. manifest pointing to missing snapshot;
6. manifest pointing to missing table object;
7. corrupt snapshot;
8. corrupt WAL header/envelope/record;
9. partial WAL tail;
10. timeline rows missing or mismatched;
11. quarantine inventory present/missing/corrupt;
12. stale temp objects.

### Maintenance Script Generator

Generate scripts over:

1. flush requested;
2. checkpoint requested;
3. WAL truncation requested;
4. compaction requested;
5. materialization requested;
6. retention requested;
7. quarantine requested;
8. purge requested;
9. repair requested;
10. close requested;
11. duplicate/coalescible tasks;
12. conflicting tasks;
13. queue-full events;
14. task cancellation;
15. fault injection at task start, durable publish, lower-layer install, and
    outcome publication.

### Retention Proof Generator

Generate:

1. complete reachability proof;
2. missing manifest proof;
3. missing inherited-layer proof;
4. degraded recovery health;
5. live object referenced by branch table;
6. live object referenced by inherited layer;
7. live object referenced by manifest snapshot;
8. unreferenced table object;
9. stale quarantine inventory;
10. purge with fresh proof;
11. purge with stale proof;
12. backend without safe move/rename.

## Required Cases

### 1. Module And Boundary Guards

1. `lifecycle` module compiles under default features.
2. `lifecycle` module compiles under no-default features.
3. `lifecycle` module compiles under all features.
4. Production lifecycle APIs remain `pub(crate)` unless an L9 wrapper explicitly
   exposes them later.
5. Production `lifecycle/` does not import engine crates.
6. Production `lifecycle/` does not import product DTOs or product vocabulary:
   `Value`, `VersionedValue`, `EntityRef`, JSON, graph, vector, search, event,
   embedding, inference, or intelligence.
7. Production `lifecycle/` does not import StrataHub client/server modules.
8. Production `lifecycle/` does not import follower refresh modules or follower
   vocabulary.
9. Production `lifecycle/` does not use raw `std::fs`, `Path`, `File`, mmap, or
   environment variables except through backend-owned implementations.
10. Lower layers do not import `crate::lifecycle`.
11. Lifecycle errors are typed and preserve lower-layer source errors where
    useful.
12. Lifecycle code files stay within engineering thresholds or split into
    submodules.

### 2. Lifecycle State Machine

1. `New -> Opening` succeeds.
2. `Opening -> Recovering` succeeds for durable open.
3. `Opening -> Open` succeeds for cache open when no recovery is needed.
4. `Recovering -> Open` succeeds after healthy or accepted degraded recovery.
5. `Opening -> Failed` preserves failure facts.
6. `Recovering -> Failed` preserves recovery facts.
7. `Open -> Closing` succeeds.
8. `Closing -> Closed` succeeds.
9. `Closing -> Failed` preserves close failure facts.
10. `Closed -> Closed` close retry is idempotent.
11. Invalid transitions return typed errors.
12. Commits are rejected outside `Open`.
13. Ordinary maintenance is rejected outside `Open`, except close-required drain.
14. Reads during `Recovering` follow documented policy and do not expose partial
    state.
15. Close after timeout is retryable.

### 3. Open Plan And Outcome Validation

1. Missing storage mode rejects before side effects.
2. Cache open plan rejects durable policy.
3. Durable open plan requires explicit durability policy.
4. Codec id is validated before durable service creation.
5. Runtime budget facts are accepted as storage facts and not product policy.
6. Product access mode/IPC fields do not exist in `StorageOpenPlan`.
7. Primitive extension lists do not exist in `StorageOpenPlan`.
8. `StorageOpenOutcome` reports opened vs created.
9. `StorageOpenOutcome` reports recovered visible version.
10. `StorageOpenOutcome` reports raw recovery health.
11. `StorageOpenOutcome` does not claim user-facing open acceptance policy.

### 4. Capability Validation

1. Cache mode accepts in-memory/minimal backend facts by documented policy.
2. Cache mode rejects durable WAL, manifest, snapshot, quarantine, or writer-lock
   requirements.
3. Durable standard requires append, durable publish, durable sync, metadata, list
   prefix, and single-writer guard.
4. Durable always requires per-commit force durability support through L4.
5. Missing writer guard rejects before manifest creation.
6. Missing durable publish rejects before WAL or manifest creation.
7. Missing durable sync rejects before WAL or manifest creation.
8. Object durable candidate cannot claim production durability without explicit
   experimental gate.
9. Capability rejection leaves no durable objects.
10. Capability rejection releases any temporary backend guard it acquired.

### 5. Cache-Mode Open And Close

1. Cache open creates in-memory L6 branch state.
2. Cache open creates L7 clock/visibility state.
3. Cache open creates no database manifest object.
4. Cache open creates no WAL object.
5. Cache open creates no snapshot/checkpoint object.
6. Cache open creates no durable table object.
7. Cache open creates no quarantine inventory.
8. Cache open reports no crash recovery claim.
9. Cache close is idempotent.
10. Cache close does not attempt WAL flush or manifest sync.
11. Cache lifecycle can run cache commits and reads through existing L6/L7
    surfaces.
12. Cache reopened state is empty unless a future cache persistence mode is
    explicitly added.

### 6. Durable Open/Create

1. New durable database creates database manifest after capability validation.
2. Existing durable database loads database manifest before WAL replay.
3. Durable open acquires writer guard before mutating durable state.
4. Writer guard failure rejects before manifest mutation.
5. Durable standard assembles standard WAL services.
6. Durable always assembles always/force-durable WAL services.
7. Manifest create failure preserves typed source error.
8. Manifest publish uncertainty is classified.
9. Durable open does not initialize product primitive registries.
10. Durable open does not start public user maintenance commands.
11. Durable open reports service facts in `StorageOpenOutcome`.

### 7. Recovery Orchestration

1. Empty durable database recovers to empty branch state.
2. Manifest plus WAL records replays all committed rows.
3. Checkpoint-only state recovers rows from snapshot.
4. Checkpoint plus WAL tail recovers checkpoint rows plus tail rows.
5. WAL replay starts after checkpoint/flush watermark.
6. WAL partial tail is repaired/truncated by the documented L4/L7 path.
7. WAL corruption in strict mode returns typed failure.
8. Lossy WAL fallback, if enabled, reports degraded recovery.
9. Corrupt database manifest returns typed failure.
10. Missing manifest-listed snapshot returns degraded or failed health by policy.
11. Missing manifest-listed table object returns degraded or failed health by
    policy.
12. Corrupt table object preserves table/format source error.
13. Branch/table manifest corruption is classified.
14. Inherited-layer loss is classified.
15. Quarantine inventory mismatch is classified.
16. Recovery never reports `Healthy` after data loss or policy downgrade.
17. Recovery does not call product primitive reconstruction.
18. Recovery returns raw facts that L9/engine can render later.

### 8. L7 Replay And Bootstrap

1. WAL rows are replayed through L7 replay hooks.
2. Replay uses original WAL commit version.
3. Replay uses original WAL commit timestamp.
4. Replay bypasses normal conflict validation.
5. Replay exact duplicate is idempotent.
6. Replay mismatch fails closed.
7. L7 version allocator catches up above recovered max version.
8. L7 visible version is published only after replayed rows install into L6.
9. Timeline rows are validated or rebuilt by documented policy.
10. Timeline mismatch is classified.
11. Unresolved durable gate from prior crash is reconciled.
12. Bootstrap does not invent transaction ids.

### 9. Recovery Health

1. Healthy recovery contains no degradation faults.
2. Missing optional telemetry object is telemetry degradation, not data loss.
3. Missing required table object is data-loss or failed recovery.
4. Lossy WAL fallback is policy downgrade.
5. Codec mismatch is failed recovery, not healthy open.
6. Manifest publish uncertainty is health debt until resolved.
7. Quarantine mismatch is health debt.
8. Retention is blocked under unsafe degraded recovery.
9. Health facts have stable debug/display text with no product wording.
10. Health facts are included in open outcome.

### 10. Maintenance Executor

1. Task priority ordering is deterministic.
2. Equal-priority task ordering is deterministic.
3. Duplicate flush tasks coalesce.
4. Duplicate checkpoint tasks coalesce by documented policy.
5. Conflicting tasks either serialize or reject with typed reason.
6. Queue-full returns typed maintenance error.
7. Drain runs accepted tasks before close.
8. Cancel prevents not-yet-started tasks from running.
9. Fault at task start records task failure.
10. Fault after lower-layer partial progress records health debt.
11. Metrics count enqueued, coalesced, run, failed, skipped, canceled, and
    drained tasks.
12. Executor tests do not depend on wall-clock sleeps.

### 11. Flush And Flush Watermark

1. Flush discovers frozen mutable state through L6 facts.
2. Flush builds table artifact through L5.
3. Flush publishes table object through L4.
4. Flush publishes branch/table manifest facts through L4 where required.
5. Flush installs table into L6 after durable publication.
6. Reads before and after flush are equivalent.
7. Flush failure before publication leaves L6 state unchanged.
8. Table publication success followed by manifest failure records partial-progress
   facts.
9. L6 install failure after publication records health debt.
10. Flush watermark advances only when all required branch facts are covered.
11. Branch absence does not advance global flush watermark without proof.
12. Flush watermark is monotonic.
13. WAL truncation is not called until retention proof is built.

### 12. Checkpoint And WAL Truncation

1. Checkpoint rejects while storage is closing.
2. Checkpoint quiesces commits through L7.
3. Checkpoint captures a row-native committed storage snapshot.
4. Optional engine-owned checkpoint sections remain opaque bytes.
5. Checkpoint writes snapshot/checkpoint object through L4.
6. Manifest snapshot watermark update happens after snapshot publication.
7. Snapshot publication failure leaves manifest unchanged.
8. Manifest update failure after snapshot publication records recovery facts.
9. Checkpoint output is deterministic for same input facts.
10. Recovery from checkpoint-only state works.
11. Recovery from checkpoint plus WAL tail works.
12. WAL truncation proof uses snapshot/flush watermarks.
13. WAL truncation treats already-pruned objects as idempotent where L4 supports
    it.
14. WAL truncation never removes active or required WAL segment.
15. WAL truncation failure records maintenance health debt.

### 13. Compaction And Materialization Scheduling

1. L8 asks L6 for compaction candidates.
2. L8 calls L5 table compaction/build APIs without reimplementing merge
   semantics.
3. L8 publishes compaction output table through L4.
4. L8 asks L6 to swap/install compaction output.
5. Replaced tables are retained or quarantined by reachability proof.
6. Compaction output publication failure leaves branch read results unchanged.
7. L6 swap failure after publication records health debt.
8. Materialization scheduling uses L6 materialization facts.
9. Materialization does not rewrite rows in L8 directly.
10. Compaction and materialization tasks coalesce by branch/level/source facts.
11. Write-stall facts are emitted from storage pressure facts.
12. Product write-stall wording is absent.

### 14. Retention And Snapshot Pruning

1. Retention with complete proof may prune unreferenced objects.
2. Retention with incomplete proof keeps objects and reports debt.
3. Retention under unsafe degraded recovery keeps objects.
4. Newest snapshot count is honored.
5. Live manifest snapshot is retained even if older than count policy.
6. Snapshot pruning is idempotent.
7. Referenced table object is retained.
8. Inherited-layer referenced table object is retained.
9. Active WAL and required recovery WAL are retained.
10. Unreferenced object is not deleted directly when quarantine protocol requires
    staging.
11. Reclaimed bytes are reported where known and omitted where unknown.
12. Retention outcome lists retained, pruned, skipped, and warned objects.

### 15. Quarantine, Reclaim, Purge, And Repair

1. Quarantine blocks under unsafe degraded recovery.
2. Quarantine requires proof object is not referenced by live manifests.
3. Quarantine requires proof object is not referenced by L6 reachability facts.
4. Quarantine inventory is published durably before object is considered staged.
5. Inventory publish failure leaves object not purged.
6. Local filesystem move/rename behavior is tested separately from object-backend
   mark-only behavior.
7. Purge requires a fresh safe proof.
8. Purge with stale proof rejects.
9. Purge with missing inventory rejects or reports inconclusive by policy.
10. Purge idempotently handles already-deleted staged object where L4 supports
    not-found-as-gone.
11. Quarantine inventory mismatch is health debt.
12. Repair/reconciliation reports what it changed.
13. Repair/reconciliation reports inconclusive facts without inventing state.
14. Repair never deletes without retention proof.

### 16. Close And Shutdown

1. Close stops accepting new commits.
2. Close drains maintenance tasks that are required to complete.
3. Close cancels tasks that are safe to cancel.
4. Close obtains L7 quiesce or returns typed timeout.
5. Close timeout leaves storage retryable.
6. Close stops writer/background sync loops.
7. Durable close flushes/syncs WAL according to mode.
8. Durable close persists required manifest state.
9. Durable close records writer/sync halt as health/failure.
10. Close releases backend writer guard.
11. Close releases locks exactly once.
12. Double close is idempotent.
13. Close after failed close retries remaining required steps.
14. Close does not start ordinary maintenance.
15. Cache close does not call durable sync.
16. Close outcome reports final health and release facts.

### 17. Health And Metrics

1. Recovery duration and replay counts are recorded.
2. WAL bytes/segments are recorded where known.
3. Snapshot count and bytes are recorded where known.
4. Table count and bytes by branch/level are recorded where known.
5. Frozen mutable-table count is recorded.
6. Flush task counts and failures are recorded.
7. Compaction task counts and reclaimed bytes are recorded.
8. Quarantine object count and bytes are recorded where known.
9. Maintenance queue depth and active task facts are recorded.
10. Write stall counts and durations are recorded if stall hooks exist.
11. Selected runtime budget facts are reported raw.
12. Metrics remain raw and product-neutral.

### 18. Generated Property Harness

Create generated lifecycle contracts that run bounded scripts through production
and the independent model.

The generated harness should grow by part:

1. Core scripts cover open plans, capabilities, recovery inventories, and L7
   bootstrap.
2. Maintenance scripts add flush, checkpoint, truncation, compaction, and
   materialization scheduling.
3. Reclaim/close scripts add retention, quarantine, purge, repair, close, crash
   windows, and source-boundary assertions.

Required contracts:

1. lifecycle state transitions;
2. capability validation before side effects;
3. recovery inventory convergence;
4. replay/bootstrap facts;
5. maintenance task ordering;
6. flush/checkpoint ordering;
7. WAL/object retention proof safety;
8. quarantine/purge safety;
9. close idempotence and retry;
10. health classification.

The property harness must assert:

1. every generated script reaches at least one side-effecting operation unless
   it is explicitly a validation-only script;
2. every storage mode is exercised over the default case set;
3. every lifecycle phase has at least one generated route or direct test;
4. production recovered visible state matches model visible state after healthy
   recovery;
5. production deletion sets are subsets of model-proven-deletable objects.

### 19. Fuzz Targets

Required fuzz targets:

1. `lifecycle_recovery`
   - arbitrary bytes decode into manifest/snapshot/WAL/table/quarantine fixture
     descriptions;
   - recovery either converges to model state or returns typed health/failure;
   - corrupt inputs return typed errors, not panics.

2. `lifecycle_maintenance`
   - arbitrary scripts choose tasks, priorities, faults, and close timing;
   - task ordering, coalescing, and health debt must match the model.

3. `lifecycle_retention`
   - arbitrary scripts build object inventories, reachability facts, and recovery
     health;
   - deletion and purge must never exceed model proof.

Every fuzz target must have checked-in seed corpora and must call a distinct
contract function. Closeout tests must reject targets that only call a shared
scaffold contract.

### 20. Fault And Crash Windows

Direct fault tests must cover:

1. capability mismatch before side effects;
2. writer guard acquired then manifest create fails;
3. manifest create visible but publish durability uncertain;
4. snapshot object published but manifest watermark update fails;
5. checkpoint manifest update succeeds then WAL truncation fails;
6. WAL partial tail during recovery;
7. WAL corruption strict failure;
8. L7 replay failure during recovery;
9. L7 replay succeeds then visible publication fails;
10. table object publication succeeds then L6 install fails during flush;
11. branch/table manifest publish fails after table object publish;
12. compaction output published then branch swap fails;
13. quarantine inventory publish fails before purge;
14. purge deletes object then inventory update fails;
15. close times out waiting for L7 quiesce;
16. close WAL sync fails;
17. close manifest sync fails;
18. process crash after WAL append before L6 visibility;
19. process crash after snapshot publish before manifest update;
20. process crash after manifest update before WAL truncation;
21. process crash after quarantine inventory publish before object move/mark;
22. process crash after object quarantine before purge.

Crash tests should use local filesystem where durable behavior matters. Slow
process-level crash tests may be `#[ignore]`, but each ignored test must have a
smaller non-ignored unit or integration test covering the same phase
classification.

### 21. Source Guards

Add `lifecycle_source_guard.rs`.

It must prove:

1. `src/lifecycle/` exposes no public product lifecycle API through crate root;
2. production lifecycle items remain `pub(crate)` unless explicitly wrapped by
   L9 later;
3. `src/lifecycle/` does not import engine crates;
4. `src/lifecycle/` does not import product DTOs or product primitive modules;
5. `src/lifecycle/` does not import StrataHub modules;
6. `src/lifecycle/` does not import follower refresh modules;
7. `src/lifecycle/` does not use raw filesystem/environment APIs except through
   backend-owned code paths;
8. `src/backend/`, `src/layout/`, `src/format/`, `src/service/`, `src/table/`,
   `src/branch/`, and `src/commit/` do not import `crate::lifecycle`;
9. lifecycle testkit and tests are behind test/testkit targets;
10. closeout inventory is focused on implementation assurance, not doc-link
    trivia.

### 22. Sensitivity Probes

Record each probe in
`docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md`.

Minimum probes:

| Probe | Mutation | Expected failure |
|---|---|---|
| S1 | Create manifest before capability validation. | Capability no-side-effect test fails. |
| S2 | Let cache mode create WAL or manifest object. | Cache absence test fails. |
| S3 | Report corrupt WAL recovery as healthy. | Recovery health test fails. |
| S4 | Skip L7 replay allocator catch-up. | Replay/bootstrap test fails. |
| S5 | Advance visible version before recovered rows install. | Recovery visibility test fails. |
| S6 | Treat timeline mismatch as healthy. | Timeline recovery test fails. |
| S7 | Advance flush watermark from branch absence. | Flush watermark test fails. |
| S8 | Truncate WAL without proof. | WAL retention proof test fails. |
| S9 | Publish checkpoint manifest before snapshot object. | Checkpoint ordering fault test fails. |
| S10 | Delete referenced table object. | Retention model/property test fails. |
| S11 | Purge quarantine with stale proof. | Quarantine purge test fails. |
| S12 | Allow reclaim under unsafe degraded recovery. | Reclaim health gate test fails. |
| S13 | Start ordinary maintenance after close begins. | Close/executor test fails. |
| S14 | Double-release writer guard on double close. | Close idempotence test fails. |
| S15 | Collapse close timeout into closed success. | Close timeout test fails. |
| S16 | Import engine/product code from lifecycle. | Source guard fails. |
| S17 | Import lifecycle from lower layers. | Source guard fails. |
| S18 | Expose public maintenance command API from L8. | Source guard/closeout inventory fails. |

### 23. Closeout Inventory

Add `lifecycle_closeout.rs`.

It must verify:

1. generated harness exposes counters for every required category;
2. property tests assert every required counter;
3. source guard covers boundary categories;
4. fuzz targets exist and call distinct contracts;
5. fuzz corpora contain non-empty seed scenarios;
6. crash/fault tests cover every durable publication phase family;
7. porting log records preserved/changed/retired/deferred behavior;
8. sensitivity probes are recorded with mutation target and failing test;
9. command matrix is recorded.

Closeout inventory should not test that planning documents exist or link to each
other. Documentation consistency is reviewed in the porting log, while automated
closeout tests stay focused on implementation assurance.

### 24. Command Matrix

Mandatory commands before L8 closeout:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked lifecycle
cargo test -p strata-storage-next --locked --test lifecycle_recovery
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --locked --test lifecycle_reclaim_close
cargo test -p strata-storage-next --locked --test lifecycle_faults
cargo test -p strata-storage-next --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --locked --test lifecycle_closeout
cargo test -p strata-storage-next --locked --test crash_recovery
cargo test -p strata-storage-next --locked --quiet
cargo test -p strata-storage-next --no-default-features --locked lifecycle
cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked
cargo hack check -p strata-storage-next --feature-powerset --depth 2
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

Optional when nightly/libfuzzer is available:

```bash
cargo +nightly fuzz run lifecycle_recovery -- -max_total_time=60
cargo +nightly fuzz run lifecycle_maintenance -- -max_total_time=60
cargo +nightly fuzz run lifecycle_retention -- -max_total_time=60
```

If nightly fuzzing is unavailable, closeout inventory must still prove target
registration, distinct contract routing, and checked-in seed corpora.

## Deferred Behavior Map

The canonical deferred behavior map lives in
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`.
This test plan should not duplicate it. L8 closeout tests must verify the
porting log records any test deferral against that canonical map.

## Exit Gate

M4-L8 can close only when:

1. direct tests prove cache and durable open/create protocols;
2. recovery tests prove manifest, snapshot, WAL, table, timeline, and quarantine
   recovery health;
3. L7 replay/bootstrap tests prove recovered commits become coherent L6/L7 state;
4. maintenance tests prove deterministic task ordering, flush, checkpoint, WAL
   truncation, and scheduling hooks;
5. retention/quarantine tests prove deletion safety;
6. close tests prove ordered, idempotent, retryable shutdown;
7. model/property tests prove lifecycle state and object-inventory safety;
8. fault/crash tests cover durable publication windows;
9. source guards prove layer boundaries;
10. fuzz targets are registered and seeded;
11. sensitivity probes are recorded;
12. closeout command matrix passes.
