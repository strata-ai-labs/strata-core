# L8M Implementation Plan: Quarantine, Reclaim, Purge, And Repair Facts

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8m-quarantine-reclaim-repair-test-plan.md`

Predecessors:

1. `docs/architecture/implementation-plans/M4/L8/l8f-recovery-orchestration-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/L8/l8j-checkpoint-watermark-wal-truncation-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L8/l8l-retention-proof-snapshot-pruning-implementation-plan.md`

## Objective

Implement lifecycle-owned quarantine, purge, and repair orchestration.

L8M is the second reclaim slice. L8L decides which objects are retained,
prunable, delegated, or quarantine candidates. L8M consumes those storage facts
and performs the durable safety-buffer protocol:

1. prove reclaim is safe for the requested object;
2. publish quarantine inventory before any destructive step;
3. copy or mark the object as quarantined through L4;
4. delete the source object only after the quarantine copy is durable;
5. purge quarantine inventory later only with a fresh safe proof;
6. reconcile inventory/object disagreements into raw recovery and maintenance
   facts without inventing reachability.

The key invariant: no lifecycle path may permanently delete an object from its
source location unless a current proof says the object is not reachable and the
quarantine service has a durable inventory trail for the operation.

## Inputs

1. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
2. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
3. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
4. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
5. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
6. `docs/architecture/implementation-plans/M4/L8/l8l-retention-proof-snapshot-pruning-implementation-plan.md`
7. `crates/storage-next/src/service/quarantine.rs`
8. `crates/storage-next/src/service/quarantine/mutation.rs`
9. `crates/storage-next/src/service/quarantine/reconcile.rs`
10. `crates/storage-next/src/format/quarantine.rs`
11. `crates/storage-next/src/lifecycle/retention.rs`
12. `crates/storage-next/src/lifecycle/maintenance.rs`
13. `crates/storage-next/src/lifecycle/durable/maintenance.rs`
14. `crates/storage-next/src/lifecycle/recovery.rs`
15. `crates/storage-next/src/lifecycle/outcome.rs`
16. `crates/storage-next/src/lifecycle/error.rs`

## Existing-Code Source Map

| Current file | Evidence | L8M action |
|---|---|---|
| `service/quarantine.rs` | `QuarantineService` loads, publishes, and validates per-branch quarantine inventory. | Use this as the only durable inventory entry point. Lifecycle must not encode inventory bytes directly. |
| `service/quarantine/mutation.rs` | `quarantine_object` publishes inventory before quarantine copy, then deletes the source; `purge_quarantine` deletes only inventory-listed quarantine objects and rewrites retained failures. | Wrap these calls in lifecycle proof, admission, error, health, and maintenance outcomes. Do not duplicate mutation order in L8. |
| `service/quarantine/reconcile.rs` | Reconciliation distinguishes clean inventory, corrupt inventory, unlisted quarantine object, missing quarantine object, malformed listed object, and backend unavailability. | Convert reconciliation reports into lifecycle repair facts and recovery-health debt. |
| `format/quarantine.rs` | Inventory carries database id, branch id, codec id, object id, source object, byte count, and quarantine timestamp. | Preserve these facts in L8 outcomes so L9/engine can diagnose without parsing bytes. |
| `lifecycle/retention.rs` | Table objects can be classified as `QuarantineCandidate`; quarantine objects are delegated to the quarantine slice. | Consume retention decisions and refuse quarantine/purge when proof is incomplete or blocked. |
| `lifecycle/recovery.rs` | Quarantine inventory mismatch already becomes recovery health debt during open. | Keep recovery reconciliation read-only in L8M; repair may report mismatch facts but must not silently clear them. |
| `lifecycle/maintenance.rs` | `MaintenanceTaskKind::{Quarantine, Purge, Repair}` and quarantine/global scopes already exist. | Add request constructors, concrete runners, coalescing expectations, and mode-specific support. |
| `lifecycle/durable/maintenance.rs` | Durable maintenance dispatch already routes flush, checkpoint, compaction, materialization, and retention. | Add durable quarantine, purge, and repair runners without moving bootstrap code back into recovery bootstrap. |
| `lifecycle/outcome.rs` | `MaintenanceOutcome` now carries reason class, affected object names, reclaimed bytes, source error, state changes, and stats. | Use the existing outcome envelope for quarantine/purge/repair effects; add specialized lifecycle outcome types below it. |

## Old Codebase Porting Map

The old codebase is reference material for safety behavior and failure windows,
not an API template.

| Old file / function | Behavior to preserve | Rewrite decision | Test focus |
|---|---|---|---|
| `crates/storage/src/segmented/quarantine_protocol.rs::quarantine_segment_if_unreferenced` | Blocks reclaim under unsafe degraded recovery; uses runtime ref registry only as candidate input; walks durable manifest/inherited-layer truth before mutation. | L8M requires retention/L6 proof before calling `QuarantineService::quarantine_object`. Runtime-only facts are not enough. | Unsafe health blocks before backend access; referenced object is not quarantined. |
| `quarantine_segment_if_unreferenced_inner` | Publishes quarantine inventory before moving the file; failure between inventory publish and object movement is recoverable and visible. | Preserve publish-before-copy by delegating to `QuarantineService`. Lifecycle records partial statuses and affected objects. | Inventory publish failure does not delete; copy failure leaves inventory evidence. |
| `purge_all_quarantines` and `purge_quarantine_for_branch` | Purge is later than quarantine, idempotent for missing files, rewrites inventory with retained failures, and is blocked by degraded recovery. | `LifecyclePurgeRequest` requires fresh proof and calls `QuarantineService::purge_quarantine`. Delete failures stay in inventory and health debt. | Purge stale proof rejects; delete failure keeps retained entry. |
| `reconcile_quarantine_on_recovery` | Inventory/object disagreement degrades recovery health and blocks reclaim until reconciled. | L8M repair/reconciliation reports mismatch facts; it does not assume missing bytes are safe or invent live state. | Corrupt inventory and orphaned quarantine object become repair facts. |
| `retention_snapshot` | Reports quarantined bytes and count as storage facts for higher-level attribution. | Lifecycle outcomes report affected object names, byte counts, and reclaimed bytes where known; product attribution remains above storage. | Outcome facts are storage-shaped and product-neutral. |
| `SegmentRefRegistry::deletion_write_guard` | Prevents fork/delete race but is not durable truth by itself. | L8M relies on L6/retention proof and service-level atomicity rather than exposing a registry lock. | Runtime-only candidate cannot bypass proof. |
| `crates/storage/src/quarantine.rs` | Per-branch quarantine manifest is relocation-safe and contains only branch-local entries. | Storage-next inventory is object-backend-safe and database/codec-scoped; keep identity validation strict. | Database id, branch id, codec id mismatch fails closed. |

Do not port:

1. raw path, rename, `std::fs`, or directory-fsync code into lifecycle;
2. public retention report DTOs or product branch attribution;
3. engine background thread behavior;
4. logs-only repair reporting;
5. branch deletion or branch clear policy;
6. follower-mode refresh behavior;
7. direct backend delete from lifecycle code.

## Scope

L8M implements:

1. lifecycle quarantine request, proof, outcome, and repair fact types;
2. validation that quarantine and purge requests carry a complete current proof;
3. conversion from L8L table quarantine candidates into quarantine object
   requests;
4. durable quarantine object staging through `QuarantineService`;
5. durable purge through `QuarantineService`;
6. repair/reconciliation wrappers over branch and family reconciliation reports;
7. maintenance routing for `Quarantine`, `Purge`, and `Repair`;
8. cache-mode rejection/deferred outcomes before durable service access;
9. error and health mapping for unsafe gates, inventory mismatch, publish
   uncertainty, delete failure, corrupt inventory, and backend unavailability;
10. generated testkit counters for staged, purged, blocked, failed, and repair
    cases;
11. source guards preventing raw IO, product imports, and direct delete.

L8M does not implement:

1. public reclaim, repair, or admin commands;
2. branch deletion or branch clear orchestration;
3. direct table-object deletion that bypasses quarantine;
4. WAL truncation or snapshot pruning;
5. row-version/tombstone/TTL cleanup;
6. table-manifest durable reachability publication;
7. distributed/object-store lease management beyond L4 capabilities;
8. automatic background scheduling policy beyond the existing maintenance
   executor;
9. close-time final drain/sync ordering, which belongs to L8N;
10. crash/fuzz closeout, which belongs to L8O/L8P.

## Core Safety Decisions

### Quarantine Admission

Quarantine admission must be proof-based.

Rules:

1. `RecoveryHealth::Healthy` permits quarantine only when proof is complete.
2. Telemetry degradation may permit quarantine only if the proof explicitly says
   the degraded fact is unrelated to the candidate.
3. `DataLoss`, `PolicyDowngrade`, or `Failed` recovery health blocks quarantine
   before backend access.
4. A referenced or maybe-referenced object is retained and reported as
   deferred, not staged.
5. Missing proof family returns deferred health debt, not success.
6. Candidate object ids must be deterministic and derived from object identity
   or caller-provided stable facts, not queue position.

### Quarantine Mutation

L8M delegates mutation to L4 service mechanics.

Rules:

1. Inventory publish happens before quarantine object publish.
2. Source delete happens only after the quarantine object is durably published.
3. Inventory publish failure means no purge and no source delete.
4. Quarantine publish failure leaves inventory evidence and reports health debt.
5. Source delete failure is retryable and keeps the object named in the outcome.
6. Existing matching inventory/object state is idempotent.
7. Existing conflicting inventory/object state fails closed.

### Purge

Purge is a separate later step.

Rules:

1. Purge requires a fresh safe proof, not only an old inventory entry.
2. Purge deletes only inventory-listed quarantine objects.
3. Missing listed quarantine object is idempotently counted as gone only when
   the service reports that exact not-found condition.
4. Delete failures are retained in inventory and surfaced as health debt.
5. Inventory rewrite failure after delete attempts is reported with source
   chain and retained-object facts; callers must not assume all entries are
   gone.
6. Purge never deletes original source objects.

### Repair And Reconciliation

Repair in this slice is conservative reconciliation.

Rules:

1. Clean inventory produces a completed no-op repair outcome.
2. Corrupt inventory, database/codec mismatch, missing listed object, unlisted
   object, malformed object, and backend unavailability are distinct repair
   facts.
3. Repair may publish a corrected inventory only when the correction is
   mechanically safe and explicitly represented in the request. V1 can report
   facts without mutation.
4. Repair never deletes an object without a fresh retention/quarantine proof.
5. Family-level reconciliation must be deterministic across branches.

### Cache Mode

Cache mode has no durable quarantine state.

Rules:

1. Cache quarantine, purge, and repair tasks return unsupported/deferred
   maintenance outcomes before service access.
2. Cache mode must not construct quarantine inventory, object, manifest, WAL, or
   table-object services.
3. Cache mode must not report durable reclaim success.

## Code Organization

Recommended files:

1. `crates/storage-next/src/lifecycle/quarantine.rs`
2. `crates/storage-next/src/lifecycle/tests/quarantine.rs`
3. `crates/storage-next/src/lifecycle/tests/quarantine/` if direct tests
   approach 1,000 lines
4. `crates/storage-next/src/testkit/lifecycle/quarantine.rs`
5. `crates/storage-next/tests/lifecycle_reclaim_close.rs` for integration smoke
   if the repo introduces that target in this slice
6. `crates/storage-next/tests/lifecycle_maintenance.rs` if lifecycle
   integration tests remain grouped there
7. `crates/storage-next/tests/lifecycle_source_guard.rs` updates

Do not put concrete quarantine or purge logic into `maintenance.rs`; it should
stay a generic executor.

Do not move durable maintenance orchestration back into
`lifecycle/durable/bootstrap.rs`. Durable quarantine runners belong beside the
other durable maintenance runners.

Do not put architecture milestone labels, slice labels, or parent-plan names in
Rust code, test names, comments, fixture bytes, or panic messages. Keep that
vocabulary in planning documents and the porting log only.

## Type Surface

Names can change during implementation, but responsibilities should remain
stable.

```rust
pub(crate) struct LifecycleQuarantineRequest {
    branch_id: BranchId,
    database_id: [u8; 16],
    codec_id: LifecycleCodecId,
    source_object: ObjectName,
    object_id: LifecycleQuarantineObjectId,
    proof: LifecycleQuarantineProof,
    staged_at: Timestamp,
}

pub(crate) struct LifecycleQuarantineProof {
    status: LifecycleQuarantineProofStatus,
    recovery_health: RecoveryHealth,
    retention_decision: Option<LifecycleRetentionDecisionRecord>,
    source_reachable: bool,
    missing_fact: Option<&'static str>,
}

pub(crate) enum LifecycleQuarantineProofStatus {
    CompleteSafe,
    Referenced,
    Incomplete,
    BlockedByRecoveryHealth,
}

pub(crate) struct LifecycleQuarantineOutcome {
    status: LifecycleQuarantineStatus,
    branch_id: BranchId,
    source_object: ObjectName,
    quarantine_object: Option<ObjectName>,
    inventory_object: Option<ObjectName>,
    byte_count: u64,
    recovery_health: Option<RecoveryHealth>,
    source_error: Option<LifecycleError>,
}

pub(crate) enum LifecycleQuarantineStatus {
    QuarantinedSourceDeleted,
    AlreadyQuarantined,
    SourceDeleteRetried,
    SourceAlreadyMissingAfterPublish,
    QuarantinedSourceDeleteFailed,
    DeferredReferenced,
    DeferredIncompleteProof,
    BlockedByRecoveryHealth,
    InventoryPublishFailed,
    InventoryPublishUncertain,
    QuarantinePublishFailed,
    QuarantinePublishUncertain,
    InventoryMismatch,
}

pub(crate) struct LifecyclePurgeRequest {
    branch_id: BranchId,
    database_id: [u8; 16],
    codec_id: LifecycleCodecId,
    proof: LifecyclePurgeProof,
}

pub(crate) struct LifecyclePurgeOutcome {
    status: LifecyclePurgeStatus,
    inventory_object: ObjectName,
    deleted_objects: Vec<ObjectName>,
    retained_objects: Vec<ObjectName>,
    failed_objects: Vec<ObjectName>,
    reclaimed_bytes: u64,
    recovery_health: Option<RecoveryHealth>,
    source_error: Option<LifecycleError>,
}

pub(crate) enum LifecyclePurgeStatus {
    Completed,
    CompletedNoop,
    CompletedWithHealthDebt,
    DeferredIncompleteProof,
    BlockedByRecoveryHealth,
    InventoryRewriteFailed,
}

pub(crate) struct LifecycleQuarantineRepairRequest {
    scope: LifecycleQuarantineRepairScope,
    database_id: [u8; 16],
    codec_id: LifecycleCodecId,
    allow_mutation: bool,
}

pub(crate) enum LifecycleQuarantineRepairScope {
    Branch(BranchId),
    Family,
}

pub(crate) struct LifecycleQuarantineRepairOutcome {
    status: LifecycleQuarantineRepairStatus,
    reports: Vec<LifecycleQuarantineRepairReport>,
    recovery_health: Option<RecoveryHealth>,
    source_error: Option<LifecycleError>,
}
```

## Execution Flows

### Quarantine Candidate

1. Admit only in `Open`.
2. Validate database id, codec id, branch id, object id, source object, staged
   timestamp, and proof.
3. Reject or defer when proof is referenced, incomplete, or blocked.
4. Convert the lifecycle proof into `QuarantineGate::Safe`.
5. Call `QuarantineService::quarantine_object`.
6. Map service status into `LifecycleQuarantineOutcome`.
7. Convert outcome into `MaintenanceOutcome` with affected object names, byte
   count, retryability, health debt, and source chain.

### Purge Quarantine

1. Admit only in `Open`.
2. Validate fresh purge proof and branch/database/codec facts.
3. Reject or defer when proof is stale, incomplete, or blocked.
4. Call `QuarantineService::purge_quarantine`.
5. Count deleted and already-missing objects as reclaimed where byte counts are
   known.
6. Keep failed entries in the retained set.
7. Convert inventory publish failure into health debt and retryable outcome.

### Repair / Reconciliation

1. Admit only in `Open`.
2. Validate scope and expected database/codec facts.
3. For branch scope, call `reconcile_branch_quarantine`.
4. For family scope, call `reconcile_quarantine_family`.
5. Classify clean reports as completed.
6. Classify corrupt inventory, missing listed object, unlisted object,
   malformed object, and backend unavailable as health debt or failed repair
   according to severity.
7. Do not mutate by default. If a later request enables mutation, require a
   named correction and fresh proof.

### Durable Maintenance Routing

1. Add `MaintenanceTaskRequest::quarantine(...)`,
   `MaintenanceTaskRequest::purge(...)`, and
   `MaintenanceTaskRequest::repair_quarantine(...)` only if the request needs
   payload beyond the existing generic constructors.
2. Add durable runners in `lifecycle/durable/maintenance.rs`.
3. Run only matching task kinds and leave unrelated pending tasks untouched.
4. Coalesce quarantine by branch and source object; coalesce purge by branch;
   coalesce repair by branch/family scope.
5. Cache mode returns unsupported/deferred outcomes for durable reclaim tasks
   before service access.

## Error And Health Mapping

Add or use stable lifecycle error variants with rule-29-friendly codes:

1. invalid request -> `invalid_argument.lifecycle.config`;
2. referenced object -> `failed_precondition.lifecycle.quarantine`;
3. incomplete quarantine proof -> `failed_precondition.lifecycle.quarantine`;
4. unsafe recovery health -> `failed_precondition.lifecycle.quarantine`;
5. inventory mismatch -> `corruption.lifecycle.quarantine`;
6. inventory publish failure -> `failed_precondition.lifecycle.quarantine_inventory`;
7. uncertain inventory/object publish -> `unknown.lifecycle.quarantine_publication`;
8. purge proof incomplete -> `failed_precondition.lifecycle.purge`;
9. purge delete failure -> completed-with-health-debt outcome, not a clean
   success;
10. repair inconclusive -> `failed_precondition.lifecycle.repair`;
11. service/backend failures -> lower-layer source chain preserved.

Prefer typed variants if tests would otherwise need to match reason strings:

1. `QuarantineProofBlocked`;
2. `QuarantineInventoryMismatch`;
3. `QuarantinePublicationFailed`;
4. `QuarantinePublicationUncertain`;
5. `PurgeProofStale`;
6. `QuarantineRepairInconclusive`.

## Source Guard Policy

Add or extend source guards so lifecycle quarantine/reclaim code:

1. does not use `std::fs`, `Path`, `File`, `OpenOptions`, mmap, or `std::env`;
2. does not import engine or product modules;
3. does not import primitive modules;
4. does not call backend `delete_object` directly;
5. does not encode/decode quarantine inventory bytes directly;
6. does not call `WalService::delete_covered_segments`;
7. does not call snapshot pruning APIs;
8. does not parse object paths by hand when L2/L4 helpers exist;
9. does not mutate branch rows or table rows directly;
10. keeps recovery bootstrap free of quarantine maintenance orchestration;
11. lower layers do not import lifecycle;
12. Rust code/tests do not include architecture slice labels.

## Testkit And Porting Log

Add generated quarantine testkit counters for:

1. complete-safe proof;
2. incomplete proof;
3. recovery-health-blocked proof;
4. referenced candidate;
5. staged object;
6. already-quarantined idempotency;
7. inventory publish failure;
8. quarantine publish failure;
9. source delete failure;
10. purged object;
11. purge delete failure;
12. stale purge proof;
13. corrupt inventory repair fact;
14. unlisted quarantine object repair fact;
15. cache unsupported/deferred.

After implementation, update
`docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` with:

1. old-code files read;
2. preserved behavior;
3. behavior intentionally changed;
4. retired product/primitive behavior;
5. deferred L8N/L8O/L8P items;
6. command evidence;
7. sensitivity probes.

## Deferred To Later Slices

1. Close-time final quarantine drain/sync policy: L8N.
2. Crash/fault/fuzz reclaim assurance beyond direct/testkit counters: L8O.
3. L8 closeout inventory and sensitivity ledger consolidation: L8P.
4. Public repair/reclaim commands: L9/engine-next.
5. Product retention attribution and user-facing repair messages:
   engine-next.
6. Automatic table-manifest-backed discovery of all table-object candidates:
   later durable table-manifest work if needed.
7. Object-store lease/compare-and-swap extensions beyond current L4
   capabilities.

## Implementation Steps

1. Add `lifecycle/quarantine.rs` and crate-private exports.
2. Add request/proof/outcome/repair fact types.
3. Add proof validation helpers that consume L8L retention decisions and
   recovery health.
4. Add quarantine-object orchestration over `QuarantineService`.
5. Add purge orchestration over `QuarantineService`.
6. Add repair/reconciliation orchestration over `QuarantineService`
   reconciliation reports.
7. Add maintenance request/routing support for quarantine, purge, and repair.
8. Add cache-mode unsupported/deferred handling.
9. Add error mapping and stable error codes.
10. Add direct unit tests.
11. Add lifecycle integration smoke through maintenance entry points.
12. Add generated testkit quarantine contract and counters.
13. Add source guards.
14. Update the porting log.
15. Run the command matrix below.

## Verification Commands

Run after implementation:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::quarantine
cargo test -p strata-storage-next --locked --lib lifecycle::tests::retention
cargo test -p strata-storage-next --locked --lib service::quarantine
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --locked --lib lifecycle::tests
cargo fmt --package strata-storage-next --check
git diff --check
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
```

If a dedicated integration target is added, also run:

```bash
cargo test -p strata-storage-next --locked --test lifecycle_reclaim_close
```

## Exit Gate

L8M is complete when:

1. quarantine requires complete safe proof;
2. unsafe recovery health blocks quarantine and purge before backend access;
3. referenced or maybe-referenced objects are retained/deferred;
4. inventory is published before quarantine copy/source delete;
5. copy/publish/delete fault windows produce typed outcomes with source chains;
6. existing matching quarantine state is idempotent;
7. conflicting inventory/object state fails closed;
8. purge requires fresh safe proof;
9. purge deletes only inventory-listed quarantine objects;
10. purge delete failures retain entries and report health debt;
11. repair/reconciliation distinguishes clean, corrupt, missing, unlisted,
    malformed, and unavailable facts;
12. repair does not delete without proof;
13. cache mode does not claim durable quarantine/purge/repair;
14. maintenance outcomes preserve affected object names, byte counts, reclaimed
    bytes where known, retryability, reason class, source errors, and stats;
15. generated testkit counters cover proof, mutation, purge, repair, and cache
    cases;
16. source guards prevent raw IO, product imports, direct delete, WAL/snapshot
    mutation, and bootstrap scope creep;
17. the L8M porting-log entry records old-code mapping and sensitivity probes;
18. the verification commands pass.
