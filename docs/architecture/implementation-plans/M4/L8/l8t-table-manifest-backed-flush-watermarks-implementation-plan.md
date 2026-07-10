# L8T Implementation Plan: Table-Manifest-Backed Flush Watermarks

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8t-table-manifest-backed-flush-watermarks-test-plan.md`

Predecessors:

1. `docs/architecture/implementation-plans/M4/L8/l8j-checkpoint-watermark-wal-truncation-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4/L8/l8q-durable-table-manifest-format-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/L8/l8r-table-manifest-publication-recovery-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8s-table-object-reachability-retention-implementation-plan.md`

## Objective

Allow durable table manifests to prove flush-watermark advancement and WAL
truncation.

L8J intentionally accepted only checkpoint-covered flush watermarks. L8R made
branch table manifests recoverable. L8S made table-object reachability
proof-backed. L8T connects those facts so durable local storage can shorten WAL
replay when table manifests prove that branch table state is recoverable
without replaying older WAL records.

The slice must preserve the core L8 rule: no replay-shortening fact may be
persisted from a number alone. A flush watermark is valid only when a typed
proof says every storage row at or below that watermark is recoverable from a
checkpoint, from trusted table manifests, or from a combination of both.

## Inputs

1. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
2. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
3. `docs/architecture/storage/l7-commit-runtime.md`
4. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
5. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
6. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
7. `docs/architecture/implementation-plans/M4/L8/l8j-checkpoint-watermark-wal-truncation-implementation-plan.md`
8. `docs/architecture/implementation-plans/M4/L8/l8r-table-manifest-publication-recovery-implementation-plan.md`
9. `docs/architecture/implementation-plans/M4/L8/l8s-table-object-reachability-retention-implementation-plan.md`
10. `crates/storage-next/src/lifecycle/checkpoint.rs`
11. `crates/storage-next/src/lifecycle/recovery.rs`
12. `crates/storage-next/src/lifecycle/flush.rs`
13. `crates/storage-next/src/lifecycle/durable/maintenance.rs`
14. `crates/storage-next/src/service/manifest.rs`
15. `crates/storage-next/src/service/wal.rs`
16. `crates/storage-next/src/branch/state.rs`
17. `crates/storage-next/src/commit/timeline.rs`
18. `crates/storage/src/durability/checkpoint_runtime.rs`
19. `crates/storage/src/durability/compaction/wal_only.rs`
20. `crates/storage/src/segmented/mod.rs`

## Existing-Code Source Map

| Current file | Evidence | L8T action |
|---|---|---|
| `lifecycle/checkpoint.rs` | L8J owns checkpoint, flush-watermark, and WAL truncation request/outcome types. | Extend the proof vocabulary with table-manifest coverage. Keep operation order and L4 delegation intact. |
| `lifecycle/recovery.rs` | Recovery currently rejects manifest flush watermarks above checkpoint coverage. | Accept higher flush watermarks only when table-manifest recovery facts prove coverage. |
| `lifecycle/flush.rs` | Durable flush publishes table objects and, after L8R, table manifests. | Emit table-manifest coverage candidates only after table manifest publication is durable and validated. |
| `service/manifest.rs` | Database manifest persists `flushed_through_commit_id`; table manifest service publishes branch table manifests. | Persist database flush watermark only after a coverage proof. Do not overload branch table manifests with database-level facts. |
| `service/wal.rs` | WAL deletion already requires `WalRetentionProof::flush_watermark` or snapshot proof. | Continue passing typed proof to L4. L8T does not parse or delete WAL segments. |
| `branch/state.rs` | Branch state tracks observed row facts and table install facts. | Use L6/L8R facts to prove table coverage, not branch absence or allocator counters. |
| `commit/timeline.rs` | Commit timeline rows are storage rows and must remain recoverable when WAL is shortened. | Table-manifest coverage must include timeline rows or explicitly prove they are checkpoint-covered. |
| `lifecycle/retention.rs` | Table-object reachability proofs name live table objects and unsafe health. | Reuse safe-reachability facts as prerequisite for table-manifest watermark proof. |

## Old Codebase Porting Map

The old engine advanced a flush watermark once flushed table state could
recover commits without older WAL. L8T ports the safe part of that behavior
using storage-next manifests and typed proofs.

| Old source | Behavior to preserve | Rewrite decision | Test focus |
|---|---|---|---|
| `checkpoint_runtime.rs::truncate_storage_wal_after_flush` | Persists `flushed_through_commit_id` and deletes WAL below effective watermark when flushed state covers it. | Reintroduce table-backed flush watermark only after L8R table-manifest recovery exists. | Table-manifest-covered watermark persists and recovery succeeds after truncation. |
| `wal_only.rs::effective_watermark` | Effective retention watermark is the max of snapshot and flush watermarks. | Preserve through L4 `WalRetentionProof`; lifecycle supplies typed snapshot/flush proof. | WAL truncation chooses covered segments and keeps active/newer segments. |
| `wal_only.rs::compact_with_active_override` | Active writer segment protects segments even if manifest lags. | Keep active-segment protection in L4. L8T only supplies proof. | Active segment survives table-manifest-backed truncation. |
| `SegmentedStore::flush_oldest_frozen` | Flushed immutable state can cover commits once manifest state is durable. | Use L8R-published branch table manifests as the durable state, not volatile table install alone. | Flush object without manifest does not advance watermark; flush with manifest can. |
| `recover_segments` | Recovery from manifests must happen before WAL replay can skip covered records. | Require table-manifest recovery success before accepting manifest flush watermark. | Reopen after truncation restores rows from table manifests plus WAL tail. |
| `gc_under_degradation.rs` | Corrupt manifest blocks unsafe reclaim and replay shortening. | Unsafe table-manifest health blocks watermark proof. | Corrupt/missing table manifest rejects table-covered watermark. |

Do not port:

1. direct filesystem WAL deletion;
2. direct segment filename parsing in lifecycle;
3. flush-watermark advancement from branch absence;
4. flush-watermark advancement from volatile L6 state only;
5. logs-only WAL truncation failure reporting;
6. product checkpoint/compact commands;
7. object-store production durability.

## Scope

L8T implements:

1. `TableManifestCovered` flush-watermark proof source;
2. coverage proof construction from L8R recovered/published table manifests;
3. table-manifest coverage summaries by branch, commit range, and storage row
   family;
4. recovery validation that accepts manifest flush watermarks covered by trusted
   table manifests;
5. WAL truncation through existing L4 `WalRetentionProof::flush_watermark`;
6. durable maintenance routing for table-manifest-backed flush watermark;
7. health debt for incomplete table coverage, stale table manifests, unsafe
   recovery, and WAL deletion failures;
8. generated/testkit counters for table-covered, rejected, stale, and truncated
   cases;
9. source guards preventing lifecycle from scanning WAL segments or reading
   table object bytes directly.

L8T does not implement:

1. table-manifest format;
2. table-manifest publication or recovery;
3. table-object retention/quarantine/purge;
4. durable compaction/materialization output publication;
5. row pruning;
6. lazy table reads;
7. public API mapping;
8. object-store/OpenDAL production durability.

## Coverage Model

The table-manifest coverage proof is database-wide for the target watermark.

Rules:

1. Every branch with rows at or below the candidate watermark must have trusted
   table-manifest coverage or checkpoint coverage.
2. Every storage row family required for recovery must be covered:
   - branch user rows;
   - tombstones;
   - timeline rows;
   - inherited/materialized table refs;
   - branch metadata rows if represented in storage rows.
3. Active or frozen rows at or below the candidate watermark are not covered by
   table manifests until they have been flushed, table-object published, and
   manifest-published.
4. Table-manifest publication uncertainty is not coverage.
5. Table-object publication without table-manifest publication is not coverage.
6. Checkpoint coverage can combine with table-manifest coverage. The proof must
   name which source covers each commit interval.
7. Branch absence is not coverage unless a trusted branch lifecycle fact proves
   the branch did not exist for the interval. L8Y owns richer branch lifecycle
   facts; until then, absence is conservative.
8. Cache mode never has table-manifest coverage.

## Proof Shape

Suggested shape:

```rust
pub(crate) enum LifecycleFlushWatermarkProof {
    CheckpointCovered { snapshot_watermark: CommitVersion },
    TableManifestCovered(TableManifestFlushCoverageProof),
    Combined {
        checkpoint: CommitVersion,
        table_manifest: TableManifestFlushCoverageProof,
    },
    AlreadyPersisted,
}

pub(crate) struct TableManifestFlushCoverageProof {
    candidate: CommitVersion,
    manifest_epoch: u64,
    branch_coverages: Vec<TableManifestBranchCoverage>,
    recovery_health_epoch: u64,
}

pub(crate) struct TableManifestBranchCoverage {
    branch_id: BranchId,
    covered_min: CommitVersion,
    covered_max: CommitVersion,
    manifest_object: ObjectName,
    table_count: usize,
}
```

The exact type names can change. The important requirement is that the proof is
bound to current manifest, table-object, and recovery-health facts. A stale
proof must fail before database manifest mutation.

## Persist Protocol

Target sequence:

```text
require durable local open runtime
validate requested candidate is nonzero and monotonic
build table-manifest coverage proof from trusted current facts
validate candidate <= proof coverage
persist database manifest flushed_through_commit_id
if requested, build WalRetentionProof::flush_watermark(candidate)
call L4 WAL truncation service
record health debt for truncation failure without undoing persisted watermark
```

Rules:

1. Persisting the database manifest flush watermark happens before WAL deletion.
2. WAL deletion failure does not roll back the persisted watermark; it records
   health debt.
3. Database manifest persist failure means WAL truncation must not run.
4. Equal-to-current candidate is idempotent and does not rewrite the manifest.
5. Candidate below current watermark is rejected as stale unless the request is
   explicitly idempotent.
6. Candidate above global visible version is rejected.
7. Candidate above coverage is rejected.

## Recovery Validation

Recovery must validate manifest flush watermark against trusted coverage.

Rules:

1. If manifest flush watermark is less than or equal to checkpoint watermark,
   existing L8J behavior accepts it.
2. If manifest flush watermark is above checkpoint watermark, table-manifest
   recovery must prove coverage through that watermark before WAL replay starts.
3. If table manifests are missing/corrupt/mismatched, strict recovery fails.
4. Lossy recovery may downgrade only when the open policy allows explicit lossy
   fallback and the missing coverage is recorded as data-loss or policy debt.
5. WAL replay start is the trusted flush watermark only after coverage
   validation succeeds.
6. Duplicate WAL records at the coverage boundary are idempotent.
7. WAL records above the trusted watermark are replayed as tail records.

## Error And Health Vocabulary

Add typed lifecycle errors/faults for:

1. table-manifest flush coverage missing;
2. table-manifest flush coverage stale;
3. table-manifest flush coverage ambiguous;
4. table-manifest flush coverage unsafe recovery;
5. table-manifest flush coverage branch gap;
6. table-manifest flush coverage timeline gap;
7. flush watermark above table coverage;
8. WAL truncation after table coverage failed.

Every error must expose a stable code and preserve lower-layer source chains.

## Source Boundaries

L8T may import:

1. L8R recovered/published table-manifest facts;
2. L8S reachability proof facts;
3. L8J checkpoint/WAL proof types;
4. L4 database manifest and WAL services;
5. L6 branch observed facts and L7 visible-version facts.

L8T must not import:

1. raw filesystem APIs;
2. WAL segment parsing helpers;
3. direct backend delete APIs;
4. table byte decoders directly;
5. engine/product crates;
6. StrataHub code;
7. primitive DTOs.

## Implementation Steps

1. Extend flush-watermark proof vocabulary with table-manifest coverage.
2. Add table-manifest coverage builder over L8R/L8S facts.
3. Add recovery validation for flush watermark above checkpoint coverage.
4. Extend durable checkpoint/maintenance path to request table-manifest-backed
   flush watermark persistence.
5. Keep WAL truncation delegated to L4.
6. Add typed errors, health mapping, tests, generated counters, source guards,
   and porting-log entry.

## Deferred Behavior

Deferred to L8U:

1. table-manifest coverage for compaction/materialization outputs;
2. checkpoint-debt reduction after durable rewrites.

Deferred to L8V:

1. row pruning from old versions, tombstones, and TTL facts after WAL has been
   shortened.

Deferred to L8Y:

1. branch absence and branch deletion facts as positive coverage.

## Exit Gate

L8T is complete when:

1. table-manifest-covered flush-watermark proof exists;
2. table-object publication without manifest publication cannot advance the
   watermark;
3. database manifest flush watermark can advance above checkpoint coverage only
   with trusted table-manifest coverage;
4. recovery validates the proof before using flush watermark as replay start;
5. WAL truncation still goes only through typed L4 retention proof;
6. stale/incomplete/unsafe coverage rejects or records health debt;
7. cache mode cannot claim table-manifest flush coverage;
8. generated and direct tests cover replay after truncation;
9. source guards block raw WAL/table-object scanning in lifecycle;
10. L8U can add durable rewrite coverage without changing this proof contract.
