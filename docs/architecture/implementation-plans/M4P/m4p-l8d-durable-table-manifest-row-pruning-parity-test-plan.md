# M4P-L8D Test Plan: Durable Table Manifest, Row Pruning, Budget, And Branch Lifecycle Parity

Status: draft

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8d-durable-table-manifest-row-pruning-parity-implementation-plan.md`

Sibling test plans:

1. `docs/architecture/implementation-plans/M4P/m4p-l8b-lifecycle-maintenance-followup-test-plan.md`
2. `docs/architecture/implementation-plans/M4P/m4p-l8c-lifecycle-recovery-close-parity-test-plan.md`

Parent test methodology:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-test-plan.md`

## Goal

Prove or explicitly defer the durable-table-manifest, row-pruning, budget,
and branch-lifecycle parity gaps that affect the V1 storage contract.

A gap can close in one of two ways:

1. implementation restores or hardens behavior and tests prove it; or
2. the plan records a V1 decision, counters prove the behavior is safe for
   the expected workload, and a later owner is linked if broader parity is
   still needed.

## Test Matrix

| Area | Required Proof | Failure Caught |
| --- | --- | --- |
| Durable table manifest stack | Byte stability, provenance, fault windows, proof epochs. | Format drift, undetected publish failure, stale proof accepted. |
| Retention-aware row pruning | Reads reject below floor, TTL/tombstone proof, inherited-layer safety. | History or as-of reads silently return wrong data after pruning. |
| Storage budget profiles | Embedded profile is documented, allocations are pool-gated. | Production embedded device OOMs or unbudgeted allocations slip in. |
| Branch lifecycle completeness | Fork-on-dirty decision, race-free clear/delete, reclaim safety. | Concurrent branch operations corrupt state or orphan objects. |
| Retention defaults | Snapshot count owner, live exception, periodic scheduling. | Retention silently runs or silently never runs. |

## Semantic Decision Register Tests

Every decision test should assert that the decision is recorded in the
lifecycle architecture or implementation plan before weakening an old-engine
oracle.

Required decisions:

1. **Table provenance discrimination**
   - Allowed outcomes: provenance is diagnostic-only, provenance gates
     recovery, or per-variant policy.
   - Test failure: provenance value is silently ignored under any recovery
     path.
2. **Fork-on-dirty-source policy**
   - Allowed outcomes: strict reject (current), auto-flush, or accept-with-copy.
   - Test failure: fork against an active source produces undefined branch
     state.
3. **Default `retain_newest_snapshots`**
   - Allowed outcomes: lifecycle config default, L9 owner, engine-next
     owner.
   - Test failure: default value differs between two opens of the same
     database without an explicit config change.
4. **Periodic retention scheduling**
   - Allowed outcomes: auto from health collection, on-demand only,
     deferred post-V1.
   - Test failure: retention runs under no documented schedule.
5. **Embedded budget profile**
   - Allowed outcomes: `low_memory_test_profile` accepted as default,
     dedicated embedded profile, no embedded support.
   - Test failure: production embedded device opens a budget that is
     known-too-small without a typed rejection or warning.
6. **Deleted-branch reclaim window**
   - Allowed outcomes: immediate reclaim eligibility, time-window,
     proof-only.
   - Test failure: a deleted branch's table objects are purged before the
     release plan is durable.
7. **Recovery exclusivity token semantics**
   - Allowed outcomes: single-use, retryable, session-bounded.
   - Test failure: two concurrent recovery passes both acquire the token.

## Durable Table Manifest Stack Tests

Coverage for TM1, TM2, TR1, TR2, FW1, RP1.

Correctness tests:

1. Golden vectors decode to the original `TableManifest` for every
   supported provenance variant.
2. Golden vectors decode to the original `TableManifest` with the
   `RetainedHistoryExtensionPayload`.
3. Sequence regression is rejected by `record_manifest`.
4. Conflicting identity on the same object is rejected.
5. Conflicting object on the same identity is rejected.
6. Recovery `preflight_table_manifest_with_checkpoint` accepts byte
   duplicates and rejects byte divergence.
7. A proof token issued before a new manifest sequence fails
   `validates_for` after the sequence advances.
8. `validate_flush_watermark_is_recoverable` rejects a flush watermark not
   covered by checkpoint or table manifest.
9. Branch-absence flush-watermark advance is rejected with a typed error.
10. Compaction publication failure leaves reads unchanged and records
    typed health debt.
11. Materialization publication failure leaves reads unchanged and records
    typed health debt.
12. Branch swap failure after compaction output publication leaves the
    pre-swap level layout readable.

Mechanical counter tests:

1. `table_manifest_publish_attempts` increments per publication attempt.
2. `table_manifest_publish_failures` increments on rejected publications.
3. `table_manifest_recovery_preflight_rejected` increments when checkpoint
   and manifest diverge.
4. `table_object_proof_token_revoked` increments after sequence advance
   invalidates a token.
5. `table_object_proof_token_validated` increments on every accepted
   reclaim.
6. `format_table_manifest_golden_assertions` matches the configured
   fixture count.

Fuzz tests:

1. `format_table_manifest` rejects bad versions, sequence regressions,
   unknown extension sections, mismatched checksums, truncated payloads,
   and oversized payloads without panicking.
2. Generated publish/recovery scripts cover at least the publication
   windows enumerated in the broader L8 test plan §20 cases 8-12.

Pass gates:

1. No fuzz crash on documented corruption classes.
2. Every fault window has a typed health debt and an unchanged-reads
   assertion.
3. Proof tokens cannot survive a manifest sequence advance.

## Retention-Aware Row Pruning Tests

Coverage for RP-V1, RP-V2, RP-V3.

Correctness tests:

1. `branch.read_history(key, version)` returns a typed
   `BranchHistoryUnavailable` for `version < retained_version_floor`.
2. `branch.read_at_timestamp(key, ts)` returns the equivalent typed error
   for `ts < retained_timestamp_floor`.
3. As-of point lookup applies the same rule.
4. After reopen, the floor restored from the manifest extension drives the
   same rejection.
5. `DropOlderVersions` keeps at least one version per key.
6. `DropTombstones` does not drop tombstones still referenced by a
   retained snapshot.
7. `DropExpired` does not drop expired rows whose TTL is still inside a
   retained snapshot range.
8. Missing pruning proof keeps all versions, tombstones, and expired rows
   alive.
9. Parent-branch compaction that prunes a version still visible through a
   child's inherited layer is rejected with a typed error.
10. After a parent prunes safely (no child dependency), child reads remain
    correct.

Mechanical counter tests:

1. `branch_read_rejected_below_version_floor` increments under negative
   test fixtures.
2. `branch_read_rejected_below_timestamp_floor` increments under negative
   test fixtures.
3. `compaction_versions_pruned`, `compaction_tombstones_pruned`,
   `compaction_expired_rows_pruned` increment in positive-case fixtures.
4. `compaction_pruning_blocked_by_inherited_layer` increments when a
   parent compaction is held back by a child fork.

Generated tests:

1. Random history depths and prune floors.
2. Random TTL profiles around retained snapshot ranges.
3. Random fork-and-prune interleavings between parent and child.

Pass gates:

1. No combination of fork, prune, and read produces a wrong-data answer.
2. Read-side rejection of below-floor queries is deterministic.
3. Inherited-layer safety holds across all generated parent/child
   interleavings.

## Storage Budget Profiles Tests

Coverage for BG1, BG2.

Correctness tests:

1. The embedded production profile validates with the existing pool
   invariants.
2. The embedded production profile rejects allocations that would exceed a
   pool limit with a typed `StorageBudgetExceeded`.
3. The `BudgetedCommitBranch` rejects a mutating commit when
   `ActiveMutable` is at `RejectMutatingAdmission`.
4. The `MaintenanceQueue` pool emits `DeferOptionalMaintenance` at the
   correct severity transition.
5. `BlockCache` pool emits `Evicting` at the correct severity transition.

Mechanical counter tests:

1. `budget_pool_reservations_granted` increments per `reserve`.
2. `budget_pool_reservations_denied` increments per rejected reservation.
3. `budget_pool_releases` increments per `StorageBudgetReservation::release`.
4. Severity transitions match the pool-specific exhaustion behavior
   documented in the implementation plan.

Source guard tests:

1. A new `Vec::with_capacity`, `Box::new`, or `Arc::new` added to
   production lifecycle code without a `// budget-exempt:` marker or a
   preceding `require_*` call fails the source guard.

Generated tests:

1. Random allocation patterns under each profile.
2. Random concurrent reservation/release patterns.

Pass gates:

1. Embedded profile is reachable through `StorageOpenPlan` without an
   explicit override.
2. No production allocation is unbudgeted.
3. Severity transitions are deterministic.

## Branch Lifecycle Completeness Tests

Coverage for BL1, BL2, BL3, BL4.

Correctness tests:

1. `fork_current` against a source with active rows returns
   `SourceHasUnflushedRows` (or, if auto-flush is chosen, flushes first
   and forks cleanly).
2. `fork_at_retained_version` rejects `fork_version < retained_floor`.
3. `fork_at_retained_timestamp` rejects `timestamp < retained_timestamp_floor`.
4. Two simultaneous `clear_branch` calls for the same branch produce
   exactly one success.
5. Two simultaneous `delete_branch` calls produce exactly one success.
6. `clear` racing `delete` produces a typed conflict, never an
   intermediate state.
7. A deleted branch's release plan is consultable by retention until table
   reachability proof is fresh.
8. A deleted branch's table objects transition through quarantine before
   purge.
9. `RecoveryExclusivityToken` cannot be acquired twice in the same
   recovery pass.
10. Acquiring the token outside a recovery pass returns a typed error.

Mechanical counter tests:

1. `branch_clear_attempts` and `branch_clear_failures` match the
   race-test outcomes.
2. `branch_delete_attempts` and `branch_delete_failures` match the
   race-test outcomes.
3. `branch_fork_rejected_dirty_source` increments under the strict policy.
4. `recovery_exclusivity_token_acquired` and
   `recovery_exclusivity_token_rejected` reflect the documented
   single-use semantics.

Generated tests:

1. Random branch counts up to 64 with random create/clear/delete/fork
   interleavings.
2. Random fork-at-version and fork-at-timestamp boundaries around the
   retained floor.
3. Random recovery interleavings around branch lifecycle transitions.

Pass gates:

1. No combination of branch lifecycle operations produces orphaned table
   objects or wrongly purged live objects.
2. Generation guard never silently accepts a stale generation.
3. Recovery exclusivity token cannot be obtained concurrently.

## Retention Defaults And Periodic Scheduling Tests

Coverage for RT1, RT2, RT3.

Correctness tests:

1. Default `retain_newest_snapshots` is reachable through the documented
   owner (`LifecycleConfig`, L9, or engine-next).
2. `SnapshotService::prune_snapshots(live_snapshot_id, 1)` retains the
   live snapshot even when an older snapshot would be retained by count.
3. Periodic retention, when enabled, enqueues `Retention` tasks under the
   documented schedule.
4. Periodic retention, when disabled, never enqueues `Retention` tasks
   from `HealthCollection`.

Mechanical counter tests:

1. `retention_passes_scheduled` increments per scheduled pass.
2. `retention_passes_skipped_proof_incomplete` increments per deferred
   pass.
3. `retention_passes_skipped_degraded_health` increments per
   recovery-blocked pass.
4. `snapshot_pruning_outcomes_deferred` matches the deferred report
   count.
5. `snapshot_pruning_outcomes_completed` matches the completed report
   count.

Generated tests:

1. Random snapshot counts around the configured retain value.
2. Random degraded-recovery transitions between retention passes.
3. Random `HealthCollection` cadences under the chosen periodic policy.

Pass gates:

1. Default snapshot count is deterministic across opens.
2. Live snapshot is always retained.
3. Periodic schedule, if any, matches the documented owner.

## Source Guards

Source guards must reject:

1. raw filesystem APIs in lifecycle production code outside backend
   modules;
2. product, engine, IPC, StrataHub, or follower modules in lifecycle
   production code;
3. roadmap labels in production Rust code, comments, panic messages,
   fixture bytes, or user-visible strings;
4. table-manifest publication paths that bypass the typed proof model;
5. row pruning that does not consult `BranchCompactionPruningProof`;
6. allocations in lifecycle production code without a `require_*` call or
   `// budget-exempt:` marker;
7. branch operations that bypass `LifecycleBranchCatalog`;
8. retention paths that do not consult `LifecycleRetentionProof`.

## Verification Commands

Focused:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::table_manifest_recovery
cargo test -p strata-storage-next --locked --lib lifecycle::tests::retention
cargo test -p strata-storage-next --locked --lib lifecycle::tests::budget_runtime
cargo test -p strata-storage-next --locked --lib lifecycle::tests::flush_watermark
cargo test -p strata-storage-next --locked --test lifecycle_reclaim_close
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --locked --test format_goldens
```

Full storage-next:

```bash
cargo fmt --package strata-storage-next --check
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo test -p strata-storage-next --all-targets --all-features --locked
cargo test -p strata-storage-next --no-default-features --features testkit --locked
```

Fuzz proofs:

```bash
cargo +nightly fuzz run format_table_manifest -- -max_total_time=120
```

## Closeout Checklist

L8D test closeout requires:

1. all semantic decisions recorded;
2. all focused tests pass;
3. source guards pass;
4. golden vectors locked under `format_goldens`;
5. fuzz target registered with non-empty seed corpus;
6. generated tests include manifest provenance, row pruning, branch
   lifecycle race, and retention scheduling cases;
7. counters appear in `StorageOpenOutcome`, `MaintenanceOutcome`, and
   benchmark snapshots as required by the implementation plan;
8. any remaining deferral has a trigger counter and named owner.
