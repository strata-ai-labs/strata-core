# L8J Implementation Plan: Checkpoint, Flush Watermark, And WAL Truncation

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8j-checkpoint-watermark-wal-truncation-test-plan.md`

Predecessors:

1. `docs/architecture/implementation-plans/M4/L8/l8g-commit-bootstrap-recovery-health-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/L8/l8i-flush-table-publication-implementation-plan.md`

## Objective

Implement lifecycle-owned checkpoint execution, safe flush-watermark
publication, and WAL truncation orchestration.

L8J connects existing lower-layer pieces:

1. L7 owns commit admission, quiesce, visible-version facts, and commit clocks.
2. L6 owns branch rows, frozen/owned table state, and snapshot-row install.
3. L4 owns checkpoint snapshot publication, database manifest mutation, and WAL
   segment deletion from typed retention proofs.
4. L8 owns the operation order, proof checks, maintenance outcomes, and health
   debt.

This slice must make durable replay shortening safe. It must never truncate WAL
or advance a manifest watermark from a primitive number alone. Every replay
shortening decision needs a typed storage proof that recovery can validate.

## Inputs

1. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
2. `docs/architecture/storage/l7-commit-runtime.md`
3. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
4. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
5. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
6. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
7. `docs/architecture/implementation-plans/M4/L8/l8i-flush-table-publication-implementation-plan.md`
8. `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
9. `crates/storage-next/src/lifecycle/maintenance.rs`
10. `crates/storage-next/src/lifecycle/outcome.rs`
11. `crates/storage-next/src/lifecycle/recovery.rs`
12. `crates/storage-next/src/service/checkpoint.rs`
13. `crates/storage-next/src/service/manifest.rs`
14. `crates/storage-next/src/service/wal.rs`
15. `crates/storage-next/src/branch/state.rs`
16. `crates/storage-next/src/commit/guard.rs`

## Existing-Code Source Map

| Current file | Evidence | L8J action |
|---|---|---|
| `service/checkpoint.rs` | `CheckpointService::checkpoint` persists active WAL segment, publishes snapshot object, then persists snapshot manifest facts. It already reports orphan/uncertain snapshot windows. | Use it as the only durable checkpoint writer. Do not duplicate manifest/snapshot publish logic in lifecycle. |
| `service/manifest.rs` | `DatabaseManifestService::persist_flush_watermark` updates only `flushed_through_commit_id`. | Call only after L8 has a recovery-valid proof. Do not use it as a proof source. |
| `service/wal.rs` | `WalRetentionProof::{snapshot_watermark, flush_watermark}` and `WalService::delete_covered_segments` already enforce typed deletion inputs and active-segment protection. | Build typed proofs and pass them to L4. Do not delete objects directly. |
| `commit/guard.rs` | `CommitBranchGuardSet::try_begin_quiesce` blocks mutating admission while maintenance snapshots state. | Use quiesce for checkpoint row capture and watermark selection. |
| `branch/state.rs` | Branch state can expose observed facts, read views, reachability snapshots, active/frozen/owned row sources, and snapshot install logic. | Add or reuse branch-owned row collection helpers for checkpoint sections. Do not duplicate row ordering rules in lifecycle if L6 can own them. |
| `lifecycle/recovery.rs` | Recovery trusts checkpoint watermarks and rejects manifest flush watermarks that are not checkpoint-covered. | Keep this invariant until table-manifest-backed flush recovery exists. |
| `lifecycle/flush.rs` | Flush publishes table objects but does not persist table manifest recovery facts or manifest flush watermark. | Treat flush outcomes as candidates only; do not advance a global watermark from object publication alone. |
| Old `durability/checkpoint_runtime.rs` | Shows checkpoint sequencing and retention trigger evidence. | Port sequencing, not old path names, primitive callbacks, or product diagnostics. |
| Old `durability/compaction/wal_only.rs` | Shows snapshot/flush watermark based WAL truncation evidence. | Port proof-driven retention only. Keep deletion in L4. |

## Old Codebase Porting Map

The old storage engine is reference material for operational sequencing and
failure-window coverage. It is not the API surface for this rewrite.

| Old file / function | Behavior to preserve | Rewrite decision | Test focus |
|---|---|---|---|
| `crates/engine/src/database/compaction.rs::Database::checkpoint` | Flushes WAL first, quiesces commits, selects a watermark from fully-applied work rather than from an allocator counter, then collects checkpoint data. | Preserve quiesce-before-watermark and no-unapplied-rows semantics. L8J uses L7 quiesce and visible-version facts instead of engine transaction coordinator state. | Watermark uses visible version, not allocated/latest version; checkpoint rejects or defers if no visible rows exist. |
| `crates/storage/src/durability/checkpoint_runtime.rs::run_storage_checkpoint` | Creates or loads MANIFEST, persists active WAL segment, writes snapshot, then persists snapshot watermark facts. | Preserve ordering, but call `CheckpointService::checkpoint` instead of owning manifest/snapshot writes in lifecycle. | Operation order, snapshot-before-manifest, active WAL segment publication, partial snapshot windows. |
| `crates/storage/src/durability/disk_snapshot/checkpoint.rs::CheckpointCoordinator::checkpoint` | Allocates a snapshot id, serializes sections, writes a crash-safe snapshot, then updates in-memory watermark state on success. | Preserve the "snapshot file first, watermark after success" rule. Replace primitive section DTOs with row-native storage checkpoint sections. | Row-native section validation, snapshot id monotonicity, no primitive vocabulary. |
| `crates/engine/src/database/compaction.rs::Database::compact` | Uses the writer's in-memory active WAL segment because MANIFEST can lag after rotation. | Preserve active-segment protection. L8J passes a typed proof into L4 and lets `WalService` combine proof, listing, and active-segment facts. | Active segment and newer segments are never deleted even when MANIFEST facts lag. |
| `crates/storage/src/durability/compaction/wal_only.rs::WalOnlyCompactor::compact_with_active_override` | Computes an effective retention watermark from snapshot and flush watermarks, lists WAL segments, skips active-or-newer segments, and deletes only fully covered segments. | Preserve proof-driven retention. Do not port direct filesystem deletes or old segment filename parsing into lifecycle. | Covered segment deletion, uncovered segment retention, malformed segment handling, delete failure health debt. |
| `crates/storage/src/durability/compaction/wal_only.rs::segment_covered_by_watermark` | Uses `.meta` sidecar when possible and falls back to codec-aware WAL record scanning. | Keep this responsibility below lifecycle. L8J must not scan WAL records or inspect segment object names. | Source guard and L4 service tests prove lifecycle delegates coverage decisions. |
| `crates/storage/src/durability/checkpoint_runtime.rs::truncate_storage_wal_after_flush` | Computes a global flush watermark from flushed branch state, persists it, then compacts WAL best-effort. | Do not port the old "flushed branch state is enough" rule yet. In storage-next, table-object flush is not recovery-complete until table manifest recovery exists, so only checkpoint-covered flush watermark proofs are accepted in this slice. | Table-flush-only candidates are rejected; checkpoint-covered candidates are accepted; already-persisted candidates are idempotent. |
| `crates/storage/src/segmented/mod.rs::flush_oldest_frozen` | Keeps frozen state readable during I/O, installs immutable state atomically, and exposes flushed commit facts used by the old flush watermark path. | L8I already ports flush publication semantics. L8J consumes only proof facts that recovery can validate, not old segmented-store flush counters. | Flush checkpoint interaction preserves rows, but flush alone cannot shorten replay. |
| `crates/engine/src/database/lifecycle.rs::prune_snapshots_once` | Snapshot pruning is nonfatal after checkpoint success and always preserves the live MANIFEST snapshot. | Defer snapshot pruning policy to a later maintenance slice. If this slice adds hooks, they must preserve live snapshot facts and report pruning as health debt, not checkpoint failure. | Prune hooks, if any, are nonfatal and never delete the manifest-live snapshot. |
| `crates/engine/src/database/transaction.rs::update_flush_watermark` | Flush-watermark truncation is best-effort and errors are logged rather than failing the user transaction. | Preserve maintenance-best-effort posture for WAL truncation after a checkpoint, but surface typed lifecycle health debt instead of logs-only diagnostics. | WAL truncation failure does not invalidate a completed checkpoint; it records debt. |
| `crates/engine/src/database/lifecycle.rs::sync_storage_manifest` | Refreshes active WAL segment in MANIFEST during lifecycle operations without exposing manifest mechanics to engine callers. | Active WAL publication remains service-owned. L8J uses checkpoint service active-segment facts instead of writing manifest fields directly. | Active WAL facts flow through outcomes; lifecycle does not mutate manifest fields directly. |

Do not port these old-code details:

1. primitive snapshot DTOs (`kv`, `events`, `branches`, `json`, `vectors`);
2. product-facing checkpoint/compact APIs or error messages;
3. direct path manipulation, `std::fs`, or segment filename parsing in
   lifecycle code;
4. old best-effort flush watermark advancement from segmented flush counters;
5. raw logs as the only failure record;
6. background scheduling policy, snapshot-pruning policy, or close-time
   checkpoint behavior.

## Scope

L8J implements:

1. lifecycle checkpoint request/outcome types;
2. durable runtime methods for explicit checkpoint execution;
3. maintenance task routing for checkpoint and WAL truncation tasks;
4. commit quiesce around checkpoint row capture;
5. checkpoint watermark selection from trusted visible-version facts;
6. row-native checkpoint section production from storage rows;
7. calls to `CheckpointService::checkpoint`;
8. checkpoint partial-progress reporting for orphan and uncertain snapshot
   windows;
9. proof-gated manifest flush-watermark persistence;
10. typed WAL retention proof construction from checkpoint and accepted flush
    watermark facts;
11. calls to `WalService::delete_covered_segments`;
12. maintenance outcomes and health debt for checkpoint, flush-watermark, and
    WAL-truncation failures;
13. generated lifecycle testkit counters for checkpoint, watermark, retention
    proof, truncation success, and truncation failure;
14. source guards preventing checkpoint orchestration from owning L4 codecs,
    object layout path strings, product vocabulary, or lower-layer delete logic;
15. a porting-log entry after implementation.

L8J does not implement:

1. automatic background checkpoint policy;
2. public checkpoint commands;
3. primitive snapshot materialization or engine callbacks;
4. table-manifest-backed flush watermark advancement without checkpoint
   coverage;
5. snapshot pruning policy beyond scheduling/fact hooks;
6. object retention, quarantine, purge, or repair;
7. compaction or materialization scheduling;
8. close-time checkpoint/drain behavior;
9. localfs crash harnesses beyond direct tests unless the implementation is
   already small enough.

## Core Safety Decisions

### Checkpoint Watermark

The checkpoint watermark is the current global visible commit version observed
after commit quiesce begins.

Rules:

1. A checkpoint with watermark zero is a deferred no-op.
2. The selected watermark must not exceed L6 observed max commit version unless
   the branch is empty and the operation is deferred.
3. Rows included in the checkpoint must have commit version less than or equal
   to the checkpoint watermark.
4. Checkpoint row sections must be row-native and primitive-neutral.
5. A checkpoint is not trusted for recovery until the snapshot object and
   manifest snapshot facts are both durable-visible.

### Flush Watermark

The manifest flush watermark is a global lower bound over state that can be
recovered without WAL replay.

For this slice, accepted flush-watermark proofs are intentionally conservative:

1. `CheckpointCovered`: the candidate watermark is less than or equal to the
   latest durable checkpoint snapshot watermark.
2. `AlreadyPersisted`: the candidate is less than or equal to the manifest's
   current flush watermark and no mutation is needed.

Table-object-only flush publication from L8I is not enough to persist a global
flush watermark because recovery currently cannot rebuild branch state from a
branch/table manifest. When table-manifest recovery lands, it can add a
`TableManifestCovered` proof source. Until then, table flush outcomes may
produce candidate facts but not durable replay-shortening facts by themselves.

### WAL Truncation

WAL truncation may run only from a typed L4 retention proof:

1. checkpoint snapshot watermark -> `WalRetentionProof::snapshot_watermark`;
2. accepted manifest flush watermark -> `WalRetentionProof::flush_watermark`.

L8J must not pass primitive integers directly to WAL deletion. L4 remains
responsible for listing segments, protecting the active segment, reading segment
records, and deleting only fully covered segments.

## Type Surface

Names can change during implementation, but the responsibilities should remain
stable.

```rust
pub(crate) struct LifecycleCheckpointRequest {
    branch_id: BranchId,
    snapshot_id: u64,
    created_at: Timestamp,
    include_storage_rows: bool,
    extra_sections: Vec<SnapshotSection>,
    truncate_wal_after_checkpoint: bool,
}

pub(crate) struct LifecycleCheckpointOutcome {
    status: LifecycleCheckpointStatus,
    branch_id: BranchId,
    checkpoint_watermark: Option<CommitVersion>,
    snapshot_id: Option<u64>,
    row_count: u64,
    section_count: usize,
    snapshot_object: Option<ObjectName>,
    active_wal_segment: Option<u64>,
    flush_watermark: Option<LifecycleFlushWatermarkOutcome>,
    wal_truncation: Option<LifecycleWalTruncationOutcome>,
    recovery_health: Option<RecoveryHealth>,
}

pub(crate) enum LifecycleCheckpointStatus {
    Completed,
    DeferredNoVisibleRows,
    SnapshotPublishedManifestNotUpdated,
    SnapshotVisibilityUncertain,
    Failed,
}

pub(crate) struct LifecycleFlushWatermarkRequest {
    candidate: CommitVersion,
    proof: LifecycleFlushWatermarkProof,
}

pub(crate) enum LifecycleFlushWatermarkProof {
    CheckpointCovered { snapshot_watermark: CommitVersion },
    AlreadyPersisted,
}

pub(crate) struct LifecycleWalTruncationRequest {
    proof: WalRetentionProof,
}
```

Do not expose these outside `pub(crate)` during L8. L9 can wrap storage-facing
facts later.

## Runtime Surface

Add durable-runtime methods, not lower-layer callbacks:

```rust
impl LifecycleDurableLocalRuntime {
    pub(crate) fn checkpoint(
        &mut self,
        request: &LifecycleCheckpointRequest,
    ) -> LifecycleResult<LifecycleCheckpointOutcome>;

    pub(crate) fn persist_flush_watermark(
        &mut self,
        request: &LifecycleFlushWatermarkRequest,
    ) -> LifecycleResult<LifecycleFlushWatermarkOutcome>;

    pub(crate) fn truncate_wal(
        &mut self,
        request: &LifecycleWalTruncationRequest,
    ) -> LifecycleResult<LifecycleWalTruncationOutcome>;

    pub(crate) fn run_next_checkpoint_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>>;

    pub(crate) fn run_next_wal_truncation_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>>;
}
```

Cache runtime behavior:

1. checkpoint requests return a typed deferred/unsupported outcome without
   durable claims; or
2. cache runtime omits checkpoint methods until L9 chooses how to expose
   volatile snapshots.

Do not let cache mode create snapshot, manifest, WAL, table, checkpoint, or
quarantine objects.

## Checkpoint Protocol

Durable checkpoint sequence:

```text
require lifecycle Open
require durable-local storage mode
begin commit quiesce through L7 guard set
read visible version as checkpoint watermark
if watermark is zero: return DeferredNoVisibleRows
collect branch rows with commit_version <= watermark
sort and validate row-native checkpoint rows
encode storage row checkpoint section
append caller-supplied opaque sections, if any
build L4 CheckpointRequest
call CheckpointService::checkpoint
if service reports orphan/uncertain snapshot: return partial outcome with health debt
optionally persist checkpoint-covered flush watermark
optionally build snapshot-watermark WAL retention proof
optionally call WalService::delete_covered_segments
return checkpoint outcome
drop quiesce guard
```

The quiesce guard must cover watermark selection and row collection. It does not
need to cover WAL deletion after the checkpoint manifest facts are durable.

## Row Collection Rules

L8J should prefer adding a branch-owned helper rather than reaching into branch
internals from lifecycle code.

Suggested helper:

```rust
impl BranchLocalState {
    pub(crate) fn checkpoint_rows(
        &self,
        watermark: CommitVersion,
    ) -> BranchRuntimeResult<Vec<StorageRow>>;
}
```

Rules:

1. include active rows, frozen rows, owned immutable table rows, and materialized
   inherited rows already present in the branch state;
2. include tombstones and timeline rows;
3. exclude rows with commit version greater than the checkpoint watermark;
4. sort using the same internal-key ordering expected by snapshot install;
5. reject exact duplicate internal keys unless L6 snapshot install would accept
   them;
6. do not expose row payloads in debug output or lifecycle errors.

## Snapshot Ids

Add an internal snapshot id allocator or deterministic request contract.

Initial implementation can require the caller/test to provide a nonzero
snapshot id in `LifecycleCheckpointRequest`. Before closing L8, the durable
runtime should own a monotonic allocator seeded from recovered manifest facts so
normal checkpoint maintenance does not rely on external ids.

Rules:

1. snapshot id zero is invalid;
2. a new checkpoint id must be greater than the manifest snapshot id currently
   known by the runtime;
3. retry after orphan snapshot may reuse the same id only if the service and
   manifest facts prove the same snapshot object/content;
4. id allocation must be deterministic in tests.

## Flush Watermark Protocol

Flush-watermark persistence sequence:

```text
require lifecycle Open
require durable-local storage mode
load current manifest facts through L4 service
if candidate <= current flush watermark: return already-persisted/noop
validate proof covers candidate
if proof is checkpoint-covered, require candidate <= snapshot watermark
call DatabaseManifestService::persist_flush_watermark(candidate)
return outcome with manifest write facts
```

Rejected candidates:

1. zero candidate;
2. candidate above checkpoint-covered proof;
3. table-flush-only candidate without durable branch/table manifest recovery
   proof;
4. candidate above current visible version;
5. candidate that would cause recovery to reject manifest flush facts.

## WAL Truncation Protocol

WAL truncation sequence:

```text
require lifecycle Open
require durable-local storage mode
validate typed retention proof
call WalService::delete_covered_segments(proof)
map delete report into lifecycle outcome
return completed/deferred/failed maintenance outcome
```

Rules:

1. no truncation in cache mode;
2. no truncation without `WalRetentionProof`;
3. no direct backend delete calls from lifecycle code;
4. no active segment deletion;
5. no failure rollback of already-deleted, proven-covered old segments;
6. delete failure is maintenance health debt unless it prevents correctness.

## Maintenance Integration

Use the existing deterministic executor.

Task mapping:

1. `MaintenanceTaskKind::Checkpoint` maps to checkpoint execution.
2. `MaintenanceTaskKind::WalTruncation` maps to WAL truncation from the latest
   accepted proof.
3. Duplicate checkpoint tasks coalesce by checkpoint scope.
4. Duplicate WAL truncation tasks coalesce by WAL scope.
5. Close/drain policy remains owned by L8H/L8N; this slice only supplies
   concrete runners.

Checkpoint task requests need enough deterministic inputs for tests:

1. branch id;
2. snapshot id;
3. created timestamp;
4. whether WAL truncation should run after checkpoint.

If the existing `MaintenanceTaskRequest` shape cannot carry those facts, add a
checkpoint-specific runtime method first and keep queued checkpoint execution
deferred until the task payload model is extended. Do not encode snapshot ids in
global mutable state hidden from tests.

## Error Mapping

Use lifecycle errors with lower-layer source chains.

Required mappings:

1. `CheckpointServiceError::Manifest` -> lifecycle lower-layer service error
   with manifest operation source preserved;
2. `CheckpointServiceError::Snapshot` -> lifecycle lower-layer service error
   with snapshot source preserved;
3. `CheckpointServiceError::OrphanSnapshot` -> partial checkpoint outcome with
   orphan snapshot facts and health debt;
4. `CheckpointServiceError::FinalManifestUncertain` -> partial checkpoint
   outcome with uncertainty health debt;
5. `ManifestServiceError` from flush-watermark persistence -> lifecycle service
   error with source preserved;
6. `WalServiceError` from truncation -> lifecycle service error with source
   preserved.

Tests should assert stable error codes and source-chain types, not display
strings.

## Implementation Steps

### L8J-A: Plan And Outcome Vocabulary

1. Add checkpoint, flush-watermark, and WAL-truncation request/outcome facts.
2. Add status enums for completed, deferred, partial, and failed states.
3. Add conversion to `MaintenanceOutcome`.
4. Add tests for validation and outcome mapping.

Exit gate: invalid facts fail before lower-layer calls and outcome debug output
uses only storage vocabulary.

### L8J-B: Branch Checkpoint Row Collection

1. Add an L6 helper for checkpoint row extraction or a clearly scoped
   lifecycle helper using public branch facts only.
2. Include all storage rows needed for recovery, including tombstones and
   timeline rows.
3. Sort and validate rows before encoding snapshot sections.
4. Add direct row-collection tests.

Exit gate: checkpoint rows round-trip through existing recovery install tests.

### L8J-C: Durable Checkpoint Execution

1. Add `LifecycleDurableLocalRuntime::checkpoint`.
2. Acquire L7 quiesce before choosing watermark and collecting rows.
3. Use `encode_checkpoint_row_section`.
4. Call `CheckpointService::checkpoint`.
5. Map partial snapshot publication windows into lifecycle outcomes.
6. Add runtime tests.

Exit gate: checkpoint publishes snapshot and manifest facts in service-defined
order and recovery can open from the checkpoint.

### L8J-D: Flush Watermark Proof

1. Add proof-gated flush-watermark request validation.
2. Support checkpoint-covered proof.
3. Reject table-flush-only proof until table manifest recovery exists.
4. Call `DatabaseManifestService::persist_flush_watermark` only after proof
   validation.
5. Add monotonicity and failure-window tests.

Exit gate: manifest flush watermark never causes recovery to reject a valid
database and never advances by branch absence.

### L8J-E: WAL Truncation

1. Add WAL truncation request/outcome.
2. Build `WalRetentionProof::snapshot_watermark` from completed checkpoint
   facts.
3. Build `WalRetentionProof::flush_watermark` only from accepted manifest
   flush-watermark facts.
4. Call `WalService::delete_covered_segments`.
5. Map delete report into maintenance facts.

Exit gate: old covered segments are deleted, active/required segments are
protected, and delete failures become health debt.

### L8J-F: Maintenance Dispatch And Testkit

1. Add concrete checkpoint runner.
2. Add concrete WAL truncation runner.
3. Extend lifecycle testkit counters for checkpoint, watermark, and truncation.
4. Update source guards.
5. Update porting log after implementation.

Exit gate: direct tests, generated testkit checks, source guards, formatting,
and lint gates pass.

## Sensitivity Probes To Record

Record these in the porting log after implementation:

| Probe | Mutation | Expected failing test |
|---|---|---|
| Skip commit quiesce | Capture checkpoint rows while commits can enter | checkpoint quiesce/admission test |
| Use branch max instead of visible version | Checkpoint unvisible rows | watermark/visibility boundary test |
| Drop tombstone rows | Filter tombstones from checkpoint section | checkpoint tombstone recovery test |
| Persist snapshot facts before snapshot publish | Reorder checkpoint service calls | checkpoint ordering test |
| Treat orphan snapshot as success | Collapse partial checkpoint to completed | orphan snapshot failure-window test |
| Accept table-only flush proof | Persist flush watermark from L8I object fact alone | flush watermark proof rejection test |
| Advance flush watermark by branch absence | Empty branch advances watermark | absence safety test |
| Truncate WAL from primitive version | Bypass `WalRetentionProof` | source guard / proof test |
| Delete active WAL segment | Ignore L4 protected segment report | active segment protection test |
| Ignore WAL delete failure | Return completed despite L4 error | truncation health-debt test |
| Add architecture labels to code/tests | Insert slice labels in lifecycle source/tests | lifecycle source guard |

## Verification Commands

Run at minimum:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::checkpoint
cargo test -p strata-storage-next --locked --lib lifecycle::tests::maintenance
cargo test -p strata-storage-next --locked --lib lifecycle::tests::recovery
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --all-features --locked --test object_layout_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

Run local filesystem checkpoint/recovery tests if this slice adds concrete
localfs coverage:

```bash
cargo test -p strata-storage-next --all-features --locked lifecycle_checkpoint -- --ignored
```

## Exit Criteria

L8J is closeable when:

1. durable checkpoint execution is lifecycle-owned and primitive-neutral;
2. checkpoint rows are captured under commit quiesce;
3. checkpoint snapshot and manifest facts use L4 checkpoint service ordering;
4. checkpoint recovery round-trips through existing recovery/bootstrap;
5. flush watermark persistence is proof-gated and monotonic;
6. table-flush-only watermark candidates are rejected or explicitly deferred;
7. WAL truncation uses `WalRetentionProof` only;
8. active and uncovered WAL segments are protected;
9. partial checkpoint and truncation failures surface health debt;
10. cache mode cannot create checkpoint, manifest, or WAL retention claims;
11. testkit counters exercise input-derived checkpoint/watermark/truncation
    cases;
12. source guards prevent direct delete/layout/product ownership drift;
13. porting log records shipped files, verification, and sensitivity probes.
