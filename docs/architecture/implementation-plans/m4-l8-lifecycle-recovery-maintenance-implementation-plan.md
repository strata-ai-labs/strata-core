# M4-L8 Implementation Plan: Lifecycle, Recovery, Maintenance

Status: draft implementation plan

## Objective

Build the storage-next lifecycle runtime.

M4-L8 turns the lower storage primitives into an operational storage runtime. It
opens or creates storage, validates mode and backend capabilities, recovers
durable state, bootstraps L7 commit clocks and visibility, schedules storage
maintenance, checkpoints committed rows, reclaims proven-unreachable objects,
and closes storage in a typed and repeatable order.

L8 is still storage-internal. It exposes raw storage facts upward through L9,
but it must not turn those facts into product policy, public lifecycle commands,
IPC behavior, primitive reconstruction, StrataHub behavior, or user-facing
recovery wording.

M4-L8 is intentionally delivered in four logical parts:

1. **L8-Core: Open + Recovery**
   Defines lifecycle vocabulary, open plans, capability validation, cache and
   durable open/create, recovery orchestration, recovery health, and L7 bootstrap.
2. **L8-Maintenance + Checkpoint**
   Adds deterministic maintenance tasks, flush, checkpoints, WAL truncation,
   compaction scheduling, materialization scheduling, and storage pressure facts.
3. **L8-Reclaim + Close + Assurance**
   Adds retention, quarantine, repair, close ordering, generated/fault/crash
   assurance, source guards, sensitivity probes, and baseline closeout.
4. **L8-Durable Tables + Storage Hardening**
   Adds durable table manifests, table-object reachability, table-manifest-backed
   flush watermarks, durable rewrite publication, retention-aware row pruning,
   memory budgets, lazy object-backed reads, branch lifecycle completion, and
   commit-runtime hardening before L9 wraps the storage boundary.
   This part still targets local durable storage semantics. Production
   object-store/OpenDAL/S3 durability remains outside V1.

The `L8A`, `L8B`, ... slice labels remain the detailed work units. The
four-part structure is the delivery boundary used for planning, review, and
commit grouping.

## Inputs

1. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
2. `docs/architecture/storage/l7-commit-runtime.md`
3. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
4. `docs/architecture/storage/l5-table-runtime.md`
5. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
6. `docs/architecture/storage/target-crate-shape-and-test-harness.md`
7. `docs/architecture/storage/implementation-patterns.md`
8. `docs/architecture/implementation-plans/m4-m4t-implementation-plan.md`
9. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
10. `docs/spec/strata-storage-format-v1.md`
11. `crates/storage-next/src/backend/`
12. `crates/storage-next/src/layout/`
13. `crates/storage-next/src/service/`
14. `crates/storage-next/src/table/`
15. `crates/storage-next/src/branch/`
16. `crates/storage-next/src/commit/`

## Existing-Code Source Map

The current implementation evidence is split across old storage durability,
segmented storage, and engine database lifecycle code. The porting rule is to
extract storage lifecycle mechanics, not engine product policy.

| Current file | Relevant L8 evidence | Porting rule |
|---|---|---|
| `crates/engine/src/database/open.rs` | Primary open sequencing, support directory setup, WAL writer construction, runtime config application, flush thread start, post-recovery compaction scheduling. | Port only storage-shaped open sequencing. Product access mode, IPC fallback, primitive registry wiring, and public open wording remain above L8. |
| `crates/engine/src/database/recovery.rs` | Engine bridge over storage recovery, primitive snapshot callbacks, recovery policy conversion. | Port raw storage recovery ordering and health facts. Do not port primitive DTO reconstruction or product recovery UX. |
| `crates/engine/src/database/lifecycle.rs` | Shutdown gates, close ordering, writer health, background drain, lock release. | Port storage-owned close ordering and writer/sync facts. Keep product handle registry, IPC, and freeze hooks above L8. |
| `crates/engine/src/database/compaction.rs` | Background compaction scheduling and user-triggered compaction evidence. | Port storage maintenance scheduling facts only. Public compaction commands are not L8. |
| `crates/engine/src/database/transaction.rs` | Flush scheduling, write backpressure, writer health checks around commit. | Port storage pressure facts and maintenance triggers. L7 owns commit ordering; L9/engine owns product stall policy. |
| `crates/engine/src/background.rs` | Background task queue, drain, cancellation, task coalescing evidence. | Rebuild as storage-internal maintenance executor with deterministic test mode. Do not depend on engine product task objects. |
| `crates/storage/src/durability/recovery_bootstrap.rs` | Manifest/codec preparation, recovery config, `SegmentedStore` construction, lossy fallback evidence. | Use as recovery ordering evidence. Rebuild over L4 services and L6/L7 facts, not paths or old store constructors. |
| `crates/storage/src/durability/recovery.rs` | Manifest/snapshot/WAL replay coordinator and corruption classification. | Port strict recovery classification and callback shape. WAL bytes are storage-next L3/L4; replay goes through L7. |
| `crates/storage/src/durability/checkpoint_runtime.rs` | Storage checkpoint flow, snapshot writes, manifest watermark update, pruning triggers. | Port row-native checkpoint sequencing. Primitive sections remain opaque or above L8. |
| `crates/storage/src/durability/compaction/wal_only.rs` | WAL truncation and snapshot/flush watermark evidence. | Port proof-driven WAL retention. L8 supplies typed retention proof to L4. |
| `crates/storage/src/durability/disk_snapshot/checkpoint.rs` | Snapshot object publication and checkpoint envelope evidence. | Use as service choreography evidence. Durable publication stays L4. |
| `crates/storage/src/segmented/mod.rs` | Flush, branch-table install, compaction hooks, recovery helpers, version tracking. | Split branch state mechanics into L6 and lifecycle orchestration into L8. |
| `crates/storage/src/segmented/compaction.rs` | Branch compaction candidate selection and output install evidence. | Keep table merge in L5 and branch swap facts in L6. L8 schedules and reports health. |
| `crates/storage/src/segmented/recovery.rs` | Table/segment recovery and degraded state classification. | Port storage-shaped recovery health. Avoid old product vocabulary. |
| `crates/storage/src/segmented/quarantine_protocol.rs` | Quarantine, purge, retention snapshot, repair evidence. | Port staged reclaim protocol. L6 supplies reachability; L4 publishes quarantine inventory; L8 orchestrates. |
| `crates/storage/src/segmented/ref_registry.rs` | Shared table reference registry and deletion barrier evidence. | Use L6 reachability facts as the authority; L8 must not maintain a second inconsistent reachability model. |
| `crates/engine/src/database/refresh.rs` | Follower refresh and blocked transaction watermarks. | Retire for storage-next V1. Follower mode is not L8 scope. |

Storage-next already provides:

1. L1 backend capability and IO traits;
2. L2 object layout ownership and reserved-name checks;
3. L3 durable codecs and WAL/table/snapshot formats;
4. L4 WAL, manifest, snapshot, table-object, sidecar, and quarantine services;
5. L5 table building, reading, compaction, and table facts;
6. L6 branch LSM state, snapshot row install, reachability facts, and
   materialization facts;
7. L7 cache/durable commit runtime, replay hooks, commit timeline rows, quiesce,
   unresolved durable gates, and visible-version facts.

## L8 Boundaries

L8 owns:

1. storage lifecycle state machine;
2. storage open/create mechanics below engine policy;
3. storage mode and backend capability validation;
4. cache-mode lifecycle selection;
5. durable local service assembly;
6. raw recovery orchestration;
7. manifest/snapshot/WAL/table recovery ordering;
8. L7 replay invocation and commit-runtime bootstrap;
9. recovery health classification and reporting;
10. storage maintenance task model;
11. deterministic maintenance executor for tests;
12. flush scheduling and flush watermark updates;
13. checkpoint execution and snapshot watermark publication;
14. WAL truncation trigger mechanics with typed proofs;
15. branch/table compaction scheduling hooks;
16. inherited-layer materialization scheduling hooks;
17. retention and snapshot pruning;
18. quarantine, reclaim, purge, repair, and reconciliation orchestration;
19. storage close ordering below product close policy;
20. raw storage health and metrics;
21. lifecycle fault injection and crash/recovery test seams.

L8 must not own:

1. public database open policy;
2. IPC, multi-process product behavior, or follower mode;
3. public manual maintenance commands;
4. product recovery wording;
5. primitive snapshot materialization;
6. JSON, event, graph, vector, search, embedding, or intelligence semantics;
7. product branch workflows such as merge, publish, restore, or review;
8. product time-travel UX;
9. StrataHub fleet behavior;
10. distributed consensus;
11. backend IO implementation;
12. byte format layout;
13. table merge algorithms;
14. commit version allocation;
15. public transaction sessions;
16. engine observer side effects.

## Delivery Parts

### Part 1: L8-Core

L8-Core establishes storage open and recovery.

It includes:

1. lifecycle module scaffold and porting log;
2. lifecycle state machine;
3. `StorageOpenPlan` and `StorageOpenOutcome`;
4. storage mode and capability validation;
5. cache-mode open/close baseline;
6. durable local open/create service assembly;
7. recovery health model;
8. manifest/snapshot/WAL/table recovery orchestration;
9. L7 replay and allocator/bootstrap handoff.

Exit gate:

1. unsupported mode/capability combinations fail before durable side effects;
2. cache mode opens without durable recovery claims or durable objects;
3. durable local open/create assembles only supported L4 services;
4. recovery converges to L6/L7 state or returns typed degraded/failed facts;
5. L7 visible version and clocks are bootstrapped only from recovered facts;
6. recovery health remains storage-shaped and product-neutral.

### Part 2: L8-Maintenance + Checkpoint

L8-Maintenance + Checkpoint keeps an opened runtime bounded and healthy.

It includes:

1. maintenance task and outcome model;
2. deterministic maintenance executor;
3. flush frozen mutable state;
4. table-object publication and branch/table manifest update choreography;
5. flush watermark updates;
6. explicit checkpoint execution;
7. WAL truncation after checkpoint or flush proof;
8. snapshot retention and pruning hooks where coupled to checkpoint;
9. compaction scheduling hooks;
10. inherited-layer materialization scheduling hooks;
11. storage pressure and write-stall facts.

Exit gate:

1. maintenance tasks have deterministic inputs, outputs, and lower-layer calls;
2. flush publishes table state without skipping recovery facts;
3. checkpoint is primitive-neutral and row-native;
4. WAL truncation requires a typed durable retention proof;
5. compaction/materialization scheduling does not own L5 or L6 algorithms;
6. task failures become typed maintenance health debt unless they block
   correctness.

### Part 3: L8-Reclaim + Close + Assurance

L8-Reclaim + Close + Assurance closes lifecycle safety.

It includes:

1. retention proof model;
2. snapshot pruning finalization;
3. quarantine/reclaim/purge protocol;
4. repair and reconciliation facts;
5. storage close plan and state transitions;
6. maintenance drain/cancel behavior;
7. writer/sync close behavior;
8. lifecycle health and metrics surface;
9. generated/property/fuzz/fault/crash harnesses;
10. source guards;
11. sensitivity ledger;
12. closeout inventory.

Exit gate:

1. L8 never deletes objects without a current proof;
2. quarantine is a durable safety buffer before purge;
3. repair/reconciliation reports facts without inventing state;
4. close is ordered, idempotent after `Closed`, and retryable after typed
   timeouts;
5. generated/fault/crash tests cover recovery, maintenance, reclaim, and close;
6. source guards prevent product, engine, and lower-layer ownership drift.

## Lifecycle State Model

Target states:

```text
New
  -> Opening
  -> Recovering
  -> Open
  -> Closing
  -> Closed

Opening/Recovering/Closing
  -> Failed
```

Rules:

1. `New` accepts only open/create.
2. `Opening` accepts no commits, reads, maintenance, or close except typed
   cancellation hooks if implemented.
3. `Recovering` accepts no commits, no ordinary reads, and no maintenance except
   recovery-owned repair/reconciliation steps.
4. `Open` accepts L9-facing storage reads, commits, explicit checkpoint triggers,
   maintenance scheduling, health queries, and close.
5. `Closing` accepts health queries and close retry/status checks, but rejects
   new commits and ordinary maintenance.
6. `Closed` accepts idempotent close and health/status queries only.
7. `Failed` accepts status, health, and explicit drop/cleanup according to the
   failed phase.
8. Close after `Closed` is idempotent.
9. Close after a typed timeout is retryable until `Closed`.
10. Open failure may clean up temporary objects only when the lower layer has a
    precise rollback contract.

## Open Plans And Outcomes

`StorageOpenPlan` should describe storage mechanics before side effects begin:

```text
StorageOpenPlan
  storage_mode
  backend capability facts
  layout root or logical root
  durability policy
  codec configuration
  runtime budget/profile facts
  recovery strictness
  lifecycle hook/fault configuration
```

It must not contain product access mode, IPC behavior, primitive extension lists,
engine subsystem wiring, or StrataHub behavior.

`StorageOpenOutcome` should report raw storage facts:

```text
StorageOpenOutcome
  opened_existing_or_created
  storage_mode
  backend capabilities used
  database id when durable mode owns one
  codec id
  recovered visible version
  snapshot recovery facts
  WAL replay facts
  table/manifest recovery facts
  recovery health
  maintenance readiness
  raw metrics
```

The outcome should be detailed enough for L9/engine-next to decide product
policy, but L8 itself should not decide whether a degraded open is acceptable as
a user-facing product matter.

## Storage Mode Capability Validation

L8 owns open-time validation against L1 backend capabilities.

Required V1 modes:

1. `cache`
   - no durable recovery claim;
   - no WAL, database manifest, durable snapshots, table objects, quarantine, or
     writer lock;
   - in-memory L6/L7 state only.
2. `durable local filesystem / standard`
   - WAL, manifest, snapshots, table objects, quarantine, recovery, background
     or periodic sync, close sync;
   - no per-commit force-durability claim.
3. `durable local filesystem / always`
   - same as durable standard plus per-commit durable barrier.
4. `object durable candidate`
   - experimental only until conditional publish, recovery, retention, and
     compaction tests prove a production durability contract.

Capability mismatch must fail before creating manifest, WAL, table, snapshot,
checkpoint, quarantine, or lock objects.

## Recovery Protocol

Durable recovery should be explicit and testable:

```text
validate backend capabilities
validate codec configuration
load or create database manifest according to open mode
open durable services
load checkpoint snapshot if present
install row-native snapshot rows into L6
replay WAL records after checkpoint/flush watermark through L7
recover branch/table manifest and table-object state
reconcile quarantine inventory
classify recovery health
bootstrap commit runtime from recovered facts
return StorageOpenOutcome
```

Rules:

1. Cache mode skips durable recovery and starts from empty storage state.
2. Follower recovery does not exist in storage-next V1.
3. Recovery replay uses L7 replay hooks; it must not allocate new versions.
4. Replay bypasses normal conflict validation because WAL records are durability
   facts.
5. L8 must reject fact mismatches rather than silently rewriting recovered data.
6. Recovery health must distinguish healthy, degraded, data-loss, policy
   downgrade, and telemetry-only facts.
7. Lossy WAL fallback, if retained, must be explicit and reported as degraded
   recovery.
8. Timeline rows are storage-owned rows; recovery must validate/catch up timeline
   facts with L7.

## Flush And WAL Truncation

Flush bridges mutable branch state and durable table state:

```text
L6 reports frozen mutable state
L8 schedules or runs flush task
L6 snapshots oldest frozen table
L5 builds table object
L4 publishes table object
L4 publishes branch/table manifest facts when required
L6 installs table into branch LSM state
L8 updates flush watermark when globally safe
L4 truncates covered WAL objects from a typed retention proof
L8 reports outcome and health
```

Rules:

1. The flush watermark is a global lower bound over flushed branch state.
2. Branch absence must not advance the flush watermark unless absence is proven
   by durable branch facts.
3. Flush publication failure after partial progress becomes storage health debt.
4. If data is visible but manifest durability is uncertain, L8 must preserve
   that fact until recovery proves the state clean.
5. WAL truncation must receive typed proof, not a primitive integer watermark.

## Checkpoint Protocol

Checkpointing is storage-ordered and primitive-neutral:

```text
reject if storage is closing
quiesce commits through L7
obtain checkpoint watermark
collect row-native committed storage snapshot
accept optional opaque engine-owned checkpoint sections
L4 writes snapshot/checkpoint object
L4 updates database manifest snapshot watermark
L8 optionally schedules WAL truncation
L8 optionally schedules snapshot pruning
return raw checkpoint outcome
```

Rules:

1. L8 must not reconstruct graph, vector, JSON, event, search, or intelligence
   semantics.
2. Optional engine-owned sections are opaque to L8.
3. Snapshot publication before manifest update is a typed uncertain window.
4. Manifest update failure after snapshot publication is a typed recovery fact,
   not silent success.
5. Checkpoint output should be deterministic for the same row-native state and
   codec configuration.

## Compaction And Materialization Scheduling

L8 schedules; lower layers own mechanics.

Rules:

1. L5 owns table merge algorithms.
2. L6 owns branch level state, reachability, inherited-layer visibility, and
   swap/install facts.
3. L8 owns coalescing, cancellation, priority, and health reporting.
4. Replaced tables are quarantined or retained according to L6 reachability and
   L8 retention proof.
5. Materialization runs only through L6 materialization facts; L8 does not
   rewrite rows directly.
6. Write stalls should be based on storage pressure facts: mutable memory, frozen
   table count, L0 count, WAL debt, manifest debt, and maintenance queue debt.
7. Product stall wording and resource-profile selection belong above L8.

## Retention And Quarantine

Retention is fact-driven.

Inputs:

1. live database manifest;
2. live snapshot facts;
3. branch/table reachability facts from L6;
4. WAL retention facts from L7/L8 checkpoints and flushes;
5. quarantine inventory;
6. recovery health gate;
7. backend capabilities.

Outputs:

1. retained object set;
2. pruned object set;
3. skipped object set with reasons;
4. quarantined object set;
5. purged object set;
6. reclaimed bytes where known;
7. retention debt and warnings.

Rules:

1. Never delete data that is only probably unreachable.
2. Incomplete proof keeps objects and reports retention debt.
3. Unsafe degraded recovery blocks reclaim.
4. Quarantine inventory must be published durably before purge-eligible state is
   considered durable.
5. Purge requires a fresh safe proof, not only an old quarantine record.
6. Local filesystem may implement quarantine with move/rename when safe; object
   backends may need manifest-marked quarantine state.

## Close Protocol

Storage close is ordered and idempotent.

Target sequence:

```text
stop accepting new storage commits
drain or cancel maintenance tasks according to task policy
wait for L7 commit quiescence or return typed timeout
stop writer/background sync loops
force/flush durable WAL state when durability requires it
persist required manifest state
publish final storage health facts
release storage-owned backend guards, leases, or locks
transition to Closed
```

Rules:

1. Close after `Closed` is a no-op success with the prior final facts.
2. Close timeout leaves storage in a retryable state unless a lower-layer failure
   is sticky.
3. Close must not start new maintenance work except close-required drain/sync
   tasks.
4. Durable mode close must not report clean close while required WAL or manifest
   sync is unresolved.
5. Engine-owned primitive freeze hooks, IPC shutdown, registry release, and
   product error mapping wrap L8 rather than living inside it.

## Implementation Slices

### Part 1: L8-Core

| Slice | Title | Implementation scope | Test scope | Exit gate |
|---|---|---|---|---|
| `L8A` | Lifecycle scaffold and source map | Create `lifecycle` module structure, error/config/fact/result shells, crate-private exports, source guards, and porting log. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8a-lifecycle-scaffold-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8a-lifecycle-scaffold-test-plan.md`. | Compile-only tests, error display/source-chain tests, source-guard skeleton. | Lifecycle module compiles without product lifecycle API leakage. |
| `L8B` | Lifecycle state, open plan, and open outcome | Add lifecycle state machine, `StorageOpenPlan`, `StorageOpenOutcome`, storage mode vocabulary, raw health/fact shells, and transition validation. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8b-lifecycle-state-open-plan-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8b-lifecycle-state-open-plan-test-plan.md`. | State transition tests, plan/outcome validation, close retry/idempotence skeleton. | L8 can represent open/recover/open/close outcomes without side effects. |
| `L8C` | Storage mode capability validation | Validate cache, durable standard, durable always, and object-candidate mode requests against backend capability facts before side effects. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8c-storage-mode-capability-validation-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8c-storage-mode-capability-validation-test-plan.md`. | Capability mismatch matrix, no-side-effect rejection tests, mode-specific service absence checks. | Unsupported modes fail before durable objects or services are created. |
| `L8D` | Cache-mode open and close baseline | Open in-memory L6/L7 state, report no durable recovery claim, expose raw health, and close idempotently without durable services. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8d-cache-open-close-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8d-cache-open-close-test-plan.md`. | Cache open/close, absence of manifest/WAL/snapshot/quarantine, cache commit/read smoke through existing L6/L7 surfaces. | Cache lifecycle works without creating or claiming durable state. |
| `L8E` | Durable local open/create and service assembly | Assemble L4 durable services, load/create database manifest according to mode, acquire writer guard, initialize L6/L7 shells, and preserve raw service facts. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8e-durable-open-create-service-assembly-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8e-durable-open-create-service-assembly-test-plan.md`. | New database create, existing manifest load, writer guard failure, manifest publish uncertainty, standard/always service selection. | Durable local runtime can open/create services without replay yet. |
| `L8F` | Recovery orchestration | Drive manifest, snapshot, WAL, table, timeline, and quarantine recovery ordering; classify corrupt/missing/degraded inputs; call lower-layer decoders and services without product callbacks. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8f-recovery-orchestration-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8f-recovery-orchestration-test-plan.md`. | Empty recovery, manifest+WAL, checkpoint-only, checkpoint+WAL tail, corrupt manifest/table/WAL, partial-tail handling, quarantine reconciliation. | Durable recovery converges or returns typed health facts. |
| `L8G` | Commit-runtime bootstrap and recovery health | Feed recovered WAL rows through L7 replay, catch up L7 clocks/visible facts, validate timeline recovery, and finalize `RecoveryHealth`. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8g-commit-bootstrap-recovery-health-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8g-commit-bootstrap-recovery-health-test-plan.md`. | L7 replay invocation, allocator catch-up, visible-version restore, timeline mismatch, unresolved durable gate reconciliation. | Opened runtime has coherent L6/L7 state after recovery. |

### Part 2: L8-Maintenance + Checkpoint

| Slice | Title | Implementation scope | Test scope | Exit gate |
|---|---|---|---|---|
| `L8H` | Maintenance task model and deterministic executor | Add `MaintenanceTask`, `MaintenanceOutcome`, priorities, coalescing, drain/cancel semantics, deterministic single-threaded executor, metrics, and task fault hooks. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-test-plan.md`. | Task ordering, coalescing, cancellation, drain-before-close, queue-full, deterministic fault injection. | Maintenance can be tested without background thread nondeterminism. |
| `L8I` | Flush frozen state and table publication | Schedule/run flush of L6 frozen state, build L5 table artifacts, publish L4 table objects/manifests, install into L6, and report flush health. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8i-flush-table-publication-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8i-flush-table-publication-test-plan.md`. | Flush happy path, publication failure windows, L6 install failure, read parity after flush, no branch absence watermark advance. | Frozen mutable state can become durable table state with typed partial-progress facts. |
| `L8J` | Checkpoint, flush watermark, and WAL truncation | Execute row-native checkpoints, update manifest snapshot/flush watermarks, build typed WAL retention proof, and call L4 WAL retention/truncation. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8j-checkpoint-watermark-wal-truncation-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8j-checkpoint-watermark-wal-truncation-test-plan.md`. | Checkpoint determinism, checkpoint-only recovery facts, checkpoint+WAL tail, manifest update failure, WAL truncation proof, retention idempotency. | Checkpoints and WAL truncation are safe and recoverable. |
| `L8K` | Compaction and materialization scheduling hooks | Add scheduling hooks over L6 candidate facts, L5 table compaction, L4 output publication, L6 install/swap, inherited-layer materialization triggers, and storage pressure facts. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-test-plan.md`. | Candidate selection routing, no algorithm ownership drift, publish failure rollback/health, materialization trigger ordering, write-stall facts. | L8 schedules maintenance without owning L5/L6 semantics. |

### Part 3: L8-Reclaim + Close + Assurance

| Slice | Title | Implementation scope | Test scope | Exit gate |
|---|---|---|---|---|
| `L8L` | Retention proof and snapshot pruning | Build retention proof model over manifests, snapshots, WAL watermarks, L6 reachability, recovery health, and backend facts; prune snapshots and skip unsafe deletes. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8l-retention-proof-snapshot-pruning-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8l-retention-proof-snapshot-pruning-test-plan.md`. | Incomplete proof keeps objects, newest snapshot retention, live manifest snapshot retained, unsafe recovery blocks reclaim, idempotency. | Retention never deletes without proof. |
| `L8M` | Quarantine, reclaim, purge, and repair facts | Orchestrate quarantine inventory publication, object quarantine/move/mark, purge with fresh proof, repair/reconciliation facts, and debt reporting. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8m-quarantine-reclaim-repair-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8m-quarantine-reclaim-repair-test-plan.md`. | Quarantine before purge, publish failure windows, purge blocked by stale proof, inventory mismatch, repair facts, local durable-backend behavior split. | Unsafe deletion is impossible through L8 paths. |
| `L8N` | Close and shutdown ordering | Stop commits, drain/cancel maintenance, quiesce L7, stop writer/sync loops, flush/sync durable services, persist final health, release backend guards, and make close retryable/idempotent. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8n-close-shutdown-ordering-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8n-close-shutdown-ordering-test-plan.md`. | Clean close, timeout, retry, double close, failure preserving health, guard release, no new tasks while closing. | Storage close is ordered and recoverable. |
| `L8O` | Generated, fault, and crash assurance | Add lifecycle testkit model, generated scripts, fuzz targets, localfs crash/reopen harnesses, and fault windows across open/recovery/flush/checkpoint/reclaim/close. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8o-generated-fault-crash-assurance-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8o-generated-fault-crash-assurance-test-plan.md`. | Property/fuzz/crash coverage for lifecycle state, recovery, maintenance, retention, quarantine, close, and health. | Assurance covers operation ordering, not only unit examples. |
| `L8P` | Baseline lifecycle conformance closeout | Consolidate source guards, command matrix, old-code behavior ledger, sensitivity probes, deferred map, health inventory, and closeout tests for L8A-L8P. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8p-l8-conformance-closeout-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8p-l8-conformance-closeout-test-plan.md`. | Closeout inventory, source guard, fuzz inventory, crash/fault inventory, sensitivity-probe ledger, full command set for the baseline lifecycle/reclaim/assurance work. | L8A-L8P close and L8Q-L8Z can start over a stable lifecycle baseline. |

### Part 4: L8-Durable Tables + Storage Hardening

| Slice | Title | Implementation scope | Test scope | Exit gate |
|---|---|---|---|---|
| `L8Q` | Durable table manifest format | Define the semantic durable table-manifest payload over table identities, local table-object names, branch ids/generations, levels, bounds, provenance, recovery facts, versioning, checksums, and canonical ordering. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8q-durable-table-manifest-format-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8q-durable-table-manifest-format-test-plan.md`. | Golden encoding tests, canonical ordering, corrupt/future-version rejection, checksum mismatch, primitive-neutral section handling. | Durable table reachability has stable bytes independent of checkpoint row payloads. |
| `L8R` | Table manifest publication and recovery | Publish and replace table manifests after flush/rewrite work; recover L6 state from table manifests plus table objects; classify missing, corrupt, and ambiguous table-object state. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8r-table-manifest-publication-recovery-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8r-table-manifest-publication-recovery-test-plan.md`. | Publish windows, idempotent retry, recovery rebuild, missing/corrupt local table object health, manifest/object mismatch. | Durable open can trust table manifests rather than relying only on checkpoints and WAL. |
| `L8S` | Table-object reachability and retention | Compute the live local table-object graph from manifests, checkpoints, WAL, quarantine inventory, and recovery health; classify retain, quarantine-candidate, and delete-candidate sets without unsafe direct deletion. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8s-table-object-reachability-retention-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8s-table-object-reachability-retention-test-plan.md`. | Live object retained, orphan becomes quarantine candidate, degraded health blocks deletion, deterministic graph ordering, no-op scopes rejected or deferred. | Table-object retention is proof-backed instead of a silent no-op. |
| `L8T` | Table-manifest-backed flush watermarks | Allow flush watermarks and WAL truncation to rely on durable table-manifest coverage, not only checkpoint coverage. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8t-table-manifest-backed-flush-watermarks-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8t-table-manifest-backed-flush-watermarks-test-plan.md`. | Reject uncovered watermarks, accept table-manifest-covered watermarks, replay after truncation, stale manifest proof rejection. | Flushed durable tables can shorten WAL safely. |
| `L8U` | Durable rewrite publication | Publish compaction and materialization outputs as durable local table objects and table-manifest updates, with typed publish/install fault windows and retry semantics. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8u-durable-rewrite-publication-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8u-durable-rewrite-publication-test-plan.md`. | Compaction output publication, materialization output publication, ambiguous visibility, manifest retry, checkpoint-debt reduction. | Rewrites can be durable without relying solely on later checkpoint debt. |
| `L8V` | Retention-aware row pruning | Add proof-gated pruning of old MVCC versions, tombstones, and TTL-expired rows while preserving as-of, history, timestamp, and branch inheritance guarantees. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8v-retention-aware-row-pruning-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8v-retention-aware-row-pruning-test-plan.md`. | As-of safety, history-bound enforcement, TTL/tombstone proof, inherited-layer safety, generated model coverage. | Compaction can reclaim row history safely. |
| `L8W` | Memory and cache budget enforcement | Add explicit storage memory profiles and budget accounting for block cache, readers, active/frozen state, maintenance queues, generated artifacts, and embedded-device profiles. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8w-memory-cache-budget-enforcement-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8w-memory-cache-budget-enforcement-test-plan.md`. | Budget admission, pressure facts, low-memory profile, cache eviction, no unbounded preallocation, Raspberry Pi Zero-style profile smoke. | Runtime memory use is bounded by explicit storage budgets. |
| `L8X` | Lazy object-backed table reads | Avoid whole-table loading by using range/block reads, reader admission, and cache integration for durable local table objects. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8x-lazy-object-backed-table-reads-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8x-lazy-object-backed-table-reads-test-plan.md`. | Point/range laziness, cache hit/miss behavior, corruption handling, no-default/wasm compatibility, large-table smoke. | Huge durable tables do not require whole-object memory. |
| `L8Y` | Branch lifecycle completeness | Complete storage-internal branch create/list/delete/clear/fork-at-history/generation/pinned-view behavior over L6/L7/L8 facts without adding product API policy. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8y-branch-lifecycle-completeness-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8y-branch-lifecycle-completeness-test-plan.md`. | Duplicate create, missing source, fork-at-history, delete/clear with pinned views, generation reuse, inter-branch isolation. | L9 can expose branch mechanics without lower-layer lifecycle gaps. |
| `L8Z` | Commit hardening and pre-L9 readiness | Close remaining commit-runtime hardening around branch generation guards, transaction-id policy, conflict/concurrency edge cases, quiesce integration, minimal checkpoint/WAL-growth policy, and Q-Z closeout. Detailed plans: `docs/architecture/implementation-plans/M4/L8/l8z-commit-hardening-pre-l9-readiness-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L8/l8z-commit-hardening-pre-l9-readiness-test-plan.md`. | Generation reuse, optional transaction-id decision, conflict/concurrency windows, quiesce behavior, bounded-WAL checkpoint trigger, Q-Z command matrix and source guards. | L9 can start over stable storage semantics; physical format freeze is L10-owned. |

## Implementation Budget

Each slice should stay under the engineering-standard 1,500 LOC review budget.
Expected scope:

| Slice group | Expected LOC | Split trigger |
|---|---:|---|
| `L8A` to `L8C` | 300-1,000 LOC each | Split if state machine, plan/outcome, and capability validation share too many files. |
| `L8D` to `L8G` | 700-1,500 LOC each | Split recovery if manifest/snapshot/WAL/table replay cannot be reviewed independently. |
| `L8H` to `L8K` | 700-1,500 LOC each | Split executor, flush, checkpoint, and compaction hooks before task code mixes concerns. |
| `L8L` to `L8N` | 700-1,500 LOC each | Split retention, quarantine, and close if delete-proof logic and shutdown logic mix. |
| `L8O` to `L8P` | 500-1,500 LOC each | Split generated harnesses and closeout/source guards into separate modules before they cross the limit. |
| `L8Q` to `L8T` | 700-1,500 LOC each | Split table-manifest codec, publication, reachability, and watermark work before manifest logic absorbs retention policy. |
| `L8U` to `L8X` | 700-1,500 LOC each | Split rewrite publication, row pruning, memory budgets, and lazy reads before durable-object and cache logic mix. |
| `L8Y` to `L8Z` | 700-1,500 LOC each | Split branch lifecycle completion from commit hardening if either starts changing public-facing behavior. |

If a slice approaches the limit, create a narrower sub-slice before coding
rather than accepting a large mixed patch.

## Error And Outcome Shape

L8 errors must be phase-specific and preserve lower-layer source chains.

Required categories:

1. invalid lifecycle state transition;
2. invalid open plan;
3. storage mode unsupported;
4. backend capability mismatch;
5. codec mismatch;
6. writer guard unavailable;
7. manifest load/create/publish failure;
8. snapshot load/install/write failure;
9. WAL replay corruption;
10. WAL partial-tail repair/truncation failure;
11. lossy recovery used or rejected;
12. table object missing or corrupt;
13. branch/table manifest corruption;
14. timeline recovery mismatch;
15. L7 replay failure;
16. recovery health degraded;
17. maintenance queue full;
18. maintenance task failed;
19. flush publication failure;
20. checkpoint publication failure;
21. WAL retention proof incomplete;
22. retention proof incomplete;
23. reclaim blocked by degraded recovery;
24. quarantine inventory mismatch;
25. purge proof stale;
26. repair inconclusive;
27. close rejected by state;
28. close timeout;
29. writer/sync halted;
30. backend IO failure.

`StorageOpenOutcome` should report:

1. storage mode;
2. opened existing vs created;
3. backend capability facts;
4. database id when durable;
5. codec id;
6. recovered visible version;
7. recovered max commit version;
8. snapshot recovery facts;
9. WAL replay facts;
10. table/manifest recovery facts;
11. quarantine recovery facts;
12. L7 bootstrap facts;
13. recovery health;
14. maintenance readiness;
15. raw metrics.

`MaintenanceOutcome` should report:

1. task kind;
2. task id or deterministic sequence;
3. lower-layer objects touched;
4. committed state changes;
5. durable publication facts;
6. skipped/deferred reasons;
7. reclaimed bytes where known;
8. health debt added or cleared.

`StorageCloseOutcome` should report:

1. prior lifecycle state;
2. final lifecycle state;
3. drained task count;
4. canceled task count;
5. commit quiesce result;
6. WAL flush/sync facts;
7. manifest sync facts;
8. backend guard release facts;
9. final health.

## Source Guard Policy

Production `lifecycle/` code may import:

1. `crate::backend` capability and backend-handle traits;
2. `crate::layout` object layout types and constructors;
3. `crate::format` decoders only through documented recovery paths;
4. `crate::service` durable services;
5. `crate::table` public table-runtime APIs;
6. `crate::branch` public branch-runtime APIs;
7. `crate::commit` public crate-internal commit-runtime APIs;
8. `crate::row` storage row types;
9. `strata_core_next::{BranchId, CommitVersion, Timestamp}`;
10. standard library synchronization and collection primitives.

Production `lifecycle/` code must not import:

1. engine crates;
2. product DTOs;
3. JSON, graph, vector, search, event, embedding, inference, or intelligence
   modules;
4. public transaction-session vocabulary;
5. StrataHub client/server modules;
6. follower refresh modules;
7. raw `std::fs`, `Path`, `File`, mmap, or process-global environment variables
   except in backend implementations that already own those responsibilities;
8. lower-layer test helpers outside test/testkit targets.

L8 may orchestrate lower layers, but lower layers must not import `crate::lifecycle`.

All production lifecycle APIs default to `pub(crate)`. L9 owns the future public
storage API boundary.

## Porting Log

Create `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md`
before behavior lands. This follows the grouped `M4/L6` and `M4/L7` slice-doc
convention while the parent plan remains flat at
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`.

Every L8 slice must record:

1. old code mapped to storage-next code;
2. behavior preserved;
3. behavior intentionally changed;
4. behavior retired;
5. behavior deferred to L9, engine-next, or post-V1;
6. command evidence;
7. sensitivity probes or structural guards;
8. raw health/fact vocabulary introduced by the slice.

## Deferred Behavior Map

The following remain outside V1 L8 even after L8Q-L8Z. They are not
implementation gaps for this milestone. The detailed deferred-work ledger is
`docs/architecture/implementation-plans/M4/L8/storage-deferred-work-ledger.md`.

1. public database open policy;
2. IPC and multi-process product behavior;
3. follower mode and follower refresh state;
4. public manual maintenance commands;
5. product recovery assistant UX;
6. product recovery wording;
7. primitive snapshot materialization;
8. JSON, graph, vector, search, event, embedding, or intelligence recovery;
9. product branch merge/cherry-pick/revert/restore/publish workflows;
10. StrataHub fleet reporting;
11. distributed locks or consensus;
12. production object-store/OpenDAL/S3 durability;
13. physical storage format freeze, backwards compatibility, migration policy,
    and golden vectors, which are L10-owned;
14. new lower-layer table merge algorithms beyond proof-gated L8 scheduling and
    pruning hooks;
15. new distributed/global commit version allocation policy;
16. public storage API mapping and response DTOs;
17. rich automatic checkpoint policy beyond the L8Z minimal WAL-growth trigger;
18. threaded maintenance executor if deterministic single-threaded execution is
    sufficient for V1.

These belong to L9, engine-next, later storage milestones, or post-V1.

## Exit Gate

M4-L8 is complete when:

1. cache mode opens/closes with no durable recovery claim and no durable objects;
2. durable local standard and always modes validate capabilities before side
   effects;
3. durable open/create assembles WAL, manifest, snapshot, table, and quarantine
   services correctly;
4. recovery from manifest/snapshot/WAL/table state converges to L6/L7 state or
   returns typed degraded/failed health;
5. L7 replay/bootstrap is sufficient for durable WAL recovery;
6. recovery health is storage-shaped and product-neutral;
7. maintenance tasks are deterministic, coalesced, drainable, and cancellable;
8. flush converts frozen mutable state into durable table state without losing
   recovery facts;
9. checkpoints are row-native, primitive-neutral, deterministic, and recoverable;
10. WAL truncation and object deletion require typed proofs;
11. retention and quarantine never delete probably-reachable data;
12. close ordering drains maintenance, quiesces commits, flushes/syncs durable
    state, and releases backend guards;
13. raw health and metrics cover recovery, maintenance, reclaim, and close;
14. source guards prevent product, engine, follower, StrataHub, and raw filesystem
    leakage;
15. generated/property/fuzz/fault/crash tests cover every lifecycle phase;
16. the porting log records preserved, changed, retired, and deferred behavior;
17. semantic durable table manifests exist and recover table reachability;
18. table-object retention/quarantine uses durable reachability proof and never
    reports successful no-op handling for unsupported scopes;
19. table-manifest-backed flush watermarks can shorten WAL safely;
20. durable compaction and materialization output publication has typed
    fault-window outcomes;
21. retention-aware row pruning preserves as-of, history, TTL, tombstone, and
    branch-inheritance guarantees;
22. explicit memory budgets bound cache, read, active/frozen, maintenance, and
    generated-artifact memory;
23. lazy object-backed reads avoid whole-object loading in durable mode;
24. branch create/list/delete/clear/fork-at-history/generation behavior is
    complete enough for L9 to wrap;
25. commit-runtime hardening gaps are closed or explicitly deferred with stable
    tests;
26. minimal automatic checkpoint/WAL-growth policy prevents unbounded local WAL
    growth or reports typed pressure/deferred facts;
27. closeout commands pass under default, no-default, all-features, and wasm
    where applicable.
