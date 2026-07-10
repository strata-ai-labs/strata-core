# L8L Implementation Plan: Retention Proof And Snapshot Pruning

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8l-retention-proof-snapshot-pruning-test-plan.md`

Predecessors:

1. `docs/architecture/implementation-plans/M4/L8/l8f-recovery-orchestration-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/L8/l8j-checkpoint-watermark-wal-truncation-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-implementation-plan.md`

## Objective

Implement lifecycle-owned retention proof construction and snapshot pruning.

L8L is the first reclaim slice, but it is deliberately not the quarantine or
purge slice. Its job is to decide what storage facts are sufficient to call an
object unreachable, what facts are insufficient, and which snapshot objects can
be pruned immediately without violating recovery safety.

This slice connects existing lower-layer pieces:

1. L4 owns snapshot listing/deletion and WAL retention deletion services.
2. L6 owns branch/table reachability facts and inherited-layer visibility.
3. L7 owns visible-version facts and commit timeline facts.
4. L8J owns checkpoint, flush-watermark, and WAL-truncation proof mechanics.
5. L8K owns compaction/materialization outcomes that name replaced table
   objects, but does not delete those objects.
6. L8L owns proof assembly, retention decisions, snapshot pruning, and health
   debt when proof is incomplete.

The key invariant is simple: L8L must never delete an object merely because it
is old. It may prune snapshots only when they are not the manifest-live
snapshot and outside the configured newest-snapshot retention window. It must
not directly delete table objects that require quarantine staging; it should
classify them for L8M.

## Inputs

1. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
2. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
3. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
4. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
5. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
6. `docs/architecture/implementation-plans/M4/L8/l8j-checkpoint-watermark-wal-truncation-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-implementation-plan.md`
8. `crates/storage-next/src/lifecycle/checkpoint.rs`
9. `crates/storage-next/src/lifecycle/compaction.rs`
10. `crates/storage-next/src/lifecycle/maintenance.rs`
11. `crates/storage-next/src/lifecycle/outcome.rs`
12. `crates/storage-next/src/lifecycle/recovery.rs`
13. `crates/storage-next/src/service/snapshot.rs`
14. `crates/storage-next/src/service/snapshot/listing.rs`
15. `crates/storage-next/src/service/manifest.rs`
16. `crates/storage-next/src/service/wal.rs`
17. `crates/storage-next/src/service/quarantine.rs`
18. `crates/storage-next/src/branch/state.rs`

## Existing-Code Source Map

| Current file | Evidence | L8L action |
|---|---|---|
| `service/snapshot.rs` and `service/snapshot/listing.rs` | `SnapshotService::list_snapshots` and `SnapshotService::prune_snapshots` already list snapshot objects, protect the live snapshot id, retain newest N, and report delete failures. | Use this as the only snapshot deletion primitive. Lifecycle must not parse snapshot object names or call backend delete directly. |
| `service/manifest.rs` | Database manifest records durable snapshot id, snapshot watermark, flush watermark, database id, and codec id. | Treat manifest facts as the authoritative live snapshot and recovery watermark source. |
| `service/wal.rs` | WAL deletion already requires `WalRetentionProof`; L8J already builds checkpoint/flush proofs. | Do not reimplement WAL truncation in L8L. Retention proofs may reference WAL coverage facts but WAL object deletion remains L8J/L4. |
| `lifecycle/checkpoint.rs` | Checkpoint outcomes expose snapshot id, checkpoint watermark, flush-watermark outcome, WAL-truncation outcome, and snapshot orphan facts. | Retention proof should consume completed checkpoint facts and ignore incomplete/orphaned checkpoint windows for deletion. |
| `lifecycle/compaction.rs` | Compaction/materialization outcomes expose affected object names, checkpoint-required debt, and replaced/output refs. | Convert replaced table refs into retention decisions. Table objects requiring quarantine become `QuarantineCandidate`, not direct deletes. |
| `lifecycle/recovery.rs` | Recovery reports health, snapshot/table/WAL/quarantine facts, and degraded classes. | Unsafe degraded recovery blocks reclaim. Healthy or telemetry-only health can permit proof construction when required facts are present. |
| `lifecycle/maintenance.rs` | `MaintenanceTaskKind::{SnapshotPruning, Retention}` and scopes already exist. | Add concrete request constructors/runners for retention proof and snapshot pruning. |
| `service/quarantine.rs` | Quarantine service has inventory, object movement, purge, and reconciliation mechanics. | Do not call mutation/purge paths in L8L. Only shape handoff facts for L8M. |
| Old `durability/checkpoint_runtime.rs` | Storage snapshot pruning retained newest snapshots and the live manifest snapshot; delete failures were nonfatal after checkpoint. | Port snapshot-pruning safety and failure classification, not old path handling or logs-only diagnostics. |
| Old `segmented/quarantine_protocol.rs` | Retention snapshot and quarantine protocol distinguish live retention, post-publish debt, quarantine staging, and purge proof. | Port the proof vocabulary and staged decisions. Defer mutation to L8M. |

## Old Codebase Porting Map

The old storage engine is reference material for retention safety and fault
windows. It is not the storage-next API surface.

| Old file / function | Behavior to preserve | Rewrite decision | Test focus |
|---|---|---|---|
| `crates/storage/src/durability/checkpoint_runtime.rs::prune_storage_snapshots` | Retains the manifest-live snapshot even when it falls outside the newest-N window; clamps zero retention to one; delete failures do not invalidate the checkpoint. | Use `SnapshotService::prune_snapshots` through lifecycle request/outcome types. Preserve live-snapshot protection and nonfatal health debt. | Live snapshot retained, newest snapshots retained, zero clamps to one, delete failure records debt. |
| `crates/engine/src/database/lifecycle.rs::prune_snapshots_once` | Snapshot pruning is best-effort and storage-owned, triggered after checkpoint success. | Keep pruning as maintenance work with explicit request/outcome facts. Do not make checkpoint success depend on pruning. | Pruning after checkpoint is separate, idempotent, and cannot turn completed checkpoint into failure. |
| `crates/engine/src/database/tests/snapshot_retention.rs` | Exercises retain-count, live-snapshot preservation, delete failure, no-op under retention window, and ephemeral skip. | Port the storage-shaped cases without engine config or public API wording. | Direct lifecycle snapshot-pruning tests mirror the safety cases. |
| `crates/storage/src/segmented/quarantine_protocol.rs::retention_snapshot` | Produces storage retention facts and refuses when branch storage truth is degraded or incomplete. | Build a lifecycle retention proof from manifest, recovery health, branch reachability, checkpoint/WAL facts, and object-family facts. | Incomplete proof keeps objects and reports retention debt before backend access. |
| `crates/storage/src/segmented/quarantine_protocol.rs::quarantine_segment_if_unreferenced` | Refuses direct delete; stages unreferenced table files through quarantine before purge. | L8L must classify table objects as quarantine candidates and leave object movement to L8M. | Replaced table objects are never deleted directly by retention proof. |
| `crates/storage/src/segmented/quarantine_protocol.rs::purge_all_quarantines` | Purge requires inventory-listed objects and a fresh safe proof. | Defer purge to L8M; L8L only records that purge is not allowed from stale/incomplete proof. | Purge task is rejected/deferred in L8L rather than partially implemented. |
| `crates/storage/src/segmented/ref_registry.rs` | Runtime reference registry is an accelerator and deletion barrier, not durable truth by itself. | Use L6 reachability facts as inputs, but require durable manifest/recovery facts before deletion-class decisions. | Runtime-only reachability is insufficient for durable deletion. |
| `crates/storage/src/durability/compaction/wal_only.rs` | WAL retention decisions require snapshot/flush watermark proof and active-segment protection. | Do not touch WAL segments from retention code. Use existing L8J/L4 truncation surfaces only. | Source guard blocks WAL record scanning and direct WAL object deletes in retention code. |
| `crates/engine/src/database/retention_report.rs` | Joins storage retention facts with product branch vocabulary for user-facing reporting. | Do not port product attribution. L8L exposes raw storage decisions; L9/engine can translate later. | Debug/output tests reject product branch-retention wording. |

Do not port:

1. product `retention_report()` DTOs or user-facing attribution wording;
2. branch-name/generation product vocabulary;
3. direct filesystem path handling or raw `std::fs` deletion;
4. old logs-only retention debt;
5. table-object purge or quarantine mutation;
6. row-version pruning policy;
7. background snapshot-pruning threads;
8. primitive snapshot DTO retention.

## Scope

L8L implements:

1. retention proof request and outcome types;
2. snapshot-pruning request and outcome types;
3. proof construction from:
   - manifest snapshot id and snapshot watermark;
   - manifest flush watermark;
   - recovery health;
   - checkpoint completion facts;
   - snapshot object listing facts;
   - delegated WAL/quarantine proof family facts where available;
4. conservative object-family decisions:
   - live manifest snapshot -> retain;
   - newest retained snapshots -> retain;
   - old non-live snapshots -> prune candidate;
   - table objects that require staging -> quarantine candidate;
   - WAL segments -> defer to L8J typed proof;
   - quarantine objects -> defer to L8M;
5. snapshot pruning through `SnapshotService::prune_snapshots`;
6. maintenance task routing for `SnapshotPruning` and `Retention`;
7. health debt and maintenance outcomes for incomplete proof, unsafe recovery,
   service delete failure, and skipped object families;
8. testkit counters for proof-complete, proof-incomplete, unsafe-recovery,
   snapshot-pruned, snapshot-protected, and delete-failure cases;
9. source guards preventing retention code from owning backend deletion logic
   outside `SnapshotService`, WAL segment parsing, product retention reporting,
   or quarantine mutation.

L8L does not implement:

1. quarantine inventory publication;
2. table-object movement into quarantine;
3. purge of quarantine objects;
4. repair or reconciliation;
5. public retention commands;
6. branch deletion or branch clear orchestration;
7. row-version/tombstone/TTL pruning policy;
8. table-manifest durable reachability publication;
9. WAL truncation implementation;
10. close-time pruning/drain policy.

## Core Safety Decisions

### Proof Completeness

A retention proof is complete only when all required storage facts are present
and recovery health permits trusting them.

Rules:

1. Missing manifest facts make the proof incomplete.
2. Missing live snapshot facts make snapshot deletion unsafe. An empty object
   listing is not a durable proof because another writer or retry can publish a
   snapshot between proof construction and pruning.
3. Degraded recovery with `DataLoss` blocks reclaim.
4. Degraded recovery with `PolicyDowngrade` blocks reclaim unless the request
   explicitly scopes itself to telemetry-only decisions.
5. Telemetry-only degradation can still allow retention decisions when the
   missing/corrupt telemetry does not participate in the object family being
   pruned.
6. Runtime-only branch reachability is insufficient for direct durable object
   deletion when table-manifest recovery cannot prove the object graph.
7. Every skipped decision must name the missing proof family.

### Snapshot Pruning

Snapshot pruning is the only object deletion L8L performs.

Rules:

1. Retain count zero is clamped to one.
2. The manifest-live snapshot is always protected.
3. The newest retained snapshots are protected.
4. Delete failures do not hide successfully deleted snapshots.
5. Delete failures become maintenance health debt.
6. Snapshot pruning does not mutate manifest snapshot facts.
7. Snapshot pruning does not create WAL retention proof.
8. Snapshot pruning is idempotent.

### Table Objects

Table objects are not deleted by L8L.

Rules:

1. A table object referenced by L6 reachability is retained.
2. A replaced table object from compaction/materialization may become a
   quarantine candidate only if no current reachability fact references it.
3. A table object with incomplete proof is retained with debt.
4. L8L records object names and reasons for L8M, but does not call
   `QuarantineService::quarantine_object` or backend delete.

### WAL Objects

WAL object deletion remains owned by L8J and L4.

Rules:

1. L8L may include WAL watermark facts in proof summaries.
2. L8L may report that WAL truncation proof is present or incomplete.
3. L8L must not list WAL segments, parse WAL object names, or call
   `WalService::delete_covered_segments`.

## Code Organization

Recommended files:

1. `crates/storage-next/src/lifecycle/retention.rs`
2. `crates/storage-next/src/lifecycle/tests/retention.rs`
3. `crates/storage-next/src/lifecycle/tests/retention/` if the test file
   approaches 1,000 lines
4. `crates/storage-next/src/testkit/lifecycle/retention.rs`
5. `crates/storage-next/tests/lifecycle_reclaim_close.rs` for integration smoke
6. `crates/storage-next/tests/lifecycle_source_guard.rs` updates

Do not put concrete retention proof or snapshot-pruning logic into
`maintenance.rs`; the executor should stay generic.

Do not put architecture milestone labels, slice labels, or parent-plan names in
Rust code, test names, comments, fixture bytes, or panic messages. Keep that
vocabulary in planning documents and the porting log only.

## Type Surface

Names can change during implementation, but responsibilities should remain
stable.

```rust
pub(crate) struct LifecycleRetentionRequest {
    scope: LifecycleRetentionScope,
    retain_newest_snapshots: usize,
    allow_telemetry_degraded_recovery: bool,
}

pub(crate) enum LifecycleRetentionScope {
    Global,
    SnapshotObjects,
    TableObjects { branch_id: BranchId },
}

pub(crate) struct LifecycleRetentionProof {
    status: LifecycleRetentionProofStatus,
    recovery_health: RecoveryHealth,
    live_snapshot_id: Option<u64>,
    snapshot_watermark: Option<CommitVersion>,
    flush_watermark: Option<CommitVersion>,
    retained_table_identities: Vec<TableIdentity>,
    warnings: Vec<LifecycleRetentionWarning>,
}

pub(crate) enum LifecycleRetentionProofStatus {
    Complete,
    Incomplete,
    BlockedByRecoveryHealth,
}

pub(crate) struct LifecycleRetentionDecisionRecord {
    object: ObjectName,
    family: LifecycleRetentionObjectFamily,
    decision: RetentionDecision,
    reason: LifecycleRetentionDecisionReason,
}

pub(crate) enum LifecycleRetentionObjectFamily {
    Snapshot,
    Table,
    Wal,
    Quarantine,
}

pub(crate) enum LifecycleRetentionDecisionReason {
    LiveManifestSnapshot,
    NewestSnapshotWindow,
    SnapshotPruneCandidate,
    ReachableTable,
    TableRequiresQuarantine,
    ProofIncomplete,
    UnsafeRecoveryHealth,
    DelegatedToWalTruncation,
    DelegatedToQuarantine,
}

pub(crate) struct LifecycleRetentionOutcome {
    status: LifecycleRetentionStatus,
    proof: LifecycleRetentionProof,
    decisions: Vec<LifecycleRetentionDecisionRecord>,
    objects_pruned: usize,
    objects_retained: usize,
    objects_skipped: usize,
    reclaimed_bytes: u64,
    recovery_health: Option<RecoveryHealth>,
}

pub(crate) enum LifecycleRetentionStatus {
    Completed,
    CompletedWithHealthDebt,
    DeferredIncompleteProof,
    BlockedByRecoveryHealth,
}

pub(crate) struct LifecycleSnapshotPruningRequest {
    live_snapshot_id: Option<u64>,
    retain_newest: usize,
    proof: LifecycleRetentionProof,
}

pub(crate) struct LifecycleSnapshotPruningOutcome {
    status: LifecycleSnapshotPruningStatus,
    deleted: Vec<SnapshotObject>,
    protected: Vec<SnapshotObject>,
    failed: Vec<SnapshotDeleteFailure>,
    reclaimed_bytes: u64,
    recovery_health: Option<RecoveryHealth>,
}

pub(crate) enum LifecycleSnapshotPruningStatus {
    Completed,
    CompletedNoop,
    CompletedWithHealthDebt,
    DeferredIncompleteProof,
    BlockedByRecoveryHealth,
}
```

## Execution Flows

### Build Retention Proof

1. Validate request scope and retention settings.
2. Read the durable manifest facts already loaded by the durable runtime.
3. Read current recovery health.
4. Read branch/table reachability facts where a caller has already supplied a
   durable table decision. Automatic table-reachability proof assembly remains
   deferred until the durable table-manifest/quarantine slices.
5. Read completed checkpoint/flush/WAL facts from lifecycle outcomes where
   available.
6. Read quarantine inventory facts only as evidence; do not mutate inventory.
7. Classify proof as complete, incomplete, or blocked.
8. Produce object-family decisions without deleting anything.

### Run Snapshot Pruning

1. Require durable mode and open lifecycle state.
2. Build or accept a complete retention proof for snapshot objects.
3. Call `SnapshotService::prune_snapshots(live_snapshot_id, retain_newest)`.
4. Convert deleted/protected/failed reports into lifecycle decisions.
5. Report delete failures as health debt, not as clean success.
6. Convert the result into `MaintenanceOutcome` with affected object names and
   reclaimed bytes where known.

### Run Retention Maintenance Task

1. Admit only in `Open`.
2. Build proof for the requested scope.
3. If proof is incomplete, return deferred outcome with retention health debt.
4. If proof is blocked by recovery health, return blocked outcome with health
   debt and no backend access.
5. If scope includes snapshots, run snapshot pruning.
6. If scope includes tables, only surface caller-supplied table decisions.
   Automatic table-object quarantine candidate discovery remains deferred.
7. If scope includes WAL/quarantine families, produce delegated decisions.
8. Never delete table/WAL/quarantine objects from this task.

## Error And Health Mapping

Use typed lifecycle errors and stable codes:

1. invalid request -> `invalid_argument.lifecycle.config` or a more specific
   retention request code if added;
2. incomplete proof -> `failed_precondition.lifecycle.retention`;
3. blocked by recovery health -> `failed_precondition.lifecycle.retention`;
4. snapshot service failure -> lower-layer service error with source chain;
5. snapshot delete failures -> completed-with-health-debt outcome;
6. cache mode durable retention request -> deferred/unsupported maintenance
   outcome before backend access;
7. quarantine/purge request in this slice -> deferred to L8M with structured
   reason.

Add new error variants only if existing `RetentionBlocked` cannot distinguish
the needed cases under rule-29 tests. Prefer:

1. `RetentionProofIncomplete`;
2. `RetentionBlockedByRecoveryHealth`;
3. `SnapshotPruningFailed`.

Each variant must have a stable `<class>.lifecycle.<detail>` code and preserve
lower-layer source chains when applicable.

## Source Guard Policy

Add or extend source guards so lifecycle retention code:

1. does not use `std::fs`, `Path`, `File`, `OpenOptions`, mmap, or `std::env`;
2. does not import engine or product retention-report modules;
3. does not import primitive modules;
4. does not parse snapshot, WAL, table, or quarantine object names by string
   when a layout/service helper exists;
5. does not call backend `delete_object` directly;
6. does not call `WalService::delete_covered_segments`;
7. does not call quarantine mutation or purge APIs;
8. does not import lifecycle from lower layers;
9. avoids architecture labels in Rust code/tests.

## Testkit And Porting Log

Add generated retention testkit counters for:

1. complete proof;
2. incomplete proof;
3. recovery-health-blocked proof;
4. snapshot protected;
5. snapshot pruned;
6. snapshot delete failure;
7. table retained;
8. table quarantine candidate;
9. WAL delegated;
10. cache unsupported/deferred.

After implementation, update
`docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` with:

1. old-code files read;
2. preserved behavior;
3. behavior intentionally changed;
4. retired product/primitive behavior;
5. deferred L8M/L8N/L8O items;
6. command evidence;
7. sensitivity probes.

## Deferred To Later Slices

1. Quarantine inventory publication: L8M.
2. Table-object quarantine/move/mark: L8M.
3. Purge of quarantine objects: L8M.
4. Repair/reconciliation: L8M.
5. Close-time retention drain: L8N.
6. Generated/fuzz/crash retention assurance beyond direct/testkit counters:
   L8O.
7. Public retention commands and product retention reports: L9/engine-next.
8. Row-version/tombstone/TTL pruning under retention policy: later table/branch
   retention work after proof semantics are closed.
9. Automatic table reachability proof assembly and table-manifest-backed direct
   table-object deletion: later durable table manifest recovery work, if ever
   allowed.

## Implementation Steps

1. Add `lifecycle/retention.rs` and export crate-private types from
   `lifecycle/mod.rs`.
2. Add request/outcome/proof/decision types.
3. Add validation for retention scope, retain count, mode, and recovery health.
4. Add proof builder over currently available manifest/recovery/branch facts.
5. Add snapshot-pruning runner that delegates to `SnapshotService`.
6. Add retention maintenance routing for `SnapshotPruning` and `Retention`
   tasks.
7. Add maintenance outcome conversion with affected object names, reclaimed
   bytes, health debt, and source chains.
8. Add direct unit tests.
9. Add lifecycle integration smoke for retention/snapshot pruning.
10. Add testkit retention contract and counters.
11. Add source guards.
12. Update the porting log.
13. Run the command matrix below.

## Verification Commands

Run after implementation:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::retention
cargo test -p strata-storage-next --locked --lib lifecycle::tests::checkpoint
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --locked --lib lifecycle::tests
cargo fmt --package strata-storage-next --check
git diff --check
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
```

If the slice adds localfs-specific snapshot pruning coverage, also run:

```bash
cargo test -p strata-storage-next --features localfs --locked --lib lifecycle::tests::retention
```

## Exit Gate

L8L is complete when:

1. retention proof distinguishes complete, incomplete, and recovery-blocked
   states;
2. incomplete proof keeps objects and records debt;
3. unsafe recovery health blocks reclaim before backend access;
4. snapshot pruning retains the live manifest snapshot;
5. snapshot pruning retains the configured newest window, clamping zero to one;
6. snapshot delete failures are nonfatal but visible as health debt;
7. table objects are not deleted directly;
8. WAL objects are not deleted by retention code;
9. cache mode does not claim durable retention;
10. maintenance outcomes list retained/pruned/skipped/delegated objects;
11. generated testkit counters cover each retention decision family;
12. source guards prevent product, raw IO, direct delete, WAL-truncation, and
    quarantine-mutation drift;
13. the L8L porting-log entry records old-code mapping and sensitivity probes;
14. the verification commands pass.
