# M4P-L8D Implementation Plan: Durable Table Manifest, Row Pruning, Budget, And Branch Lifecycle Parity

Status: draft

Parent implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8-lifecycle-maintenance-parity-implementation-plan.md`

Sibling plans:

1. `docs/architecture/implementation-plans/M4P/m4p-l8b-lifecycle-maintenance-followup-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4P/m4p-l8c-lifecycle-recovery-close-parity-implementation-plan.md`

Follow-up test plan:
`docs/architecture/implementation-plans/M4P/m4p-l8d-durable-table-manifest-row-pruning-parity-test-plan.md`

Architecture context:

1. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
2. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
4. `docs/spec/strata-storage-format-v1.md`

## Objective

Close the lifecycle parity gaps that sit in the durable-table-manifest stack,
retention-aware row pruning, the storage budget surface, and branch lifecycle
completeness, so the L8Q-L8Y exit gates from the broader L8 plan are
provable.

L8 and L8B closed the maintenance-scheduling critical path. L8C addressed
recovery, close, and quarantine contracts. L8D closes the remaining
implementation surface required by the broader L8 plan parts 3 and 4
(reclaim/close/assurance and durable tables/storage hardening) that are not
maintenance-scheduling-shaped.

L8D covers five groups:

1. byte-stable durable table manifest format with provenance and fault
   windows;
2. retention-aware row pruning enforcement on the read path;
3. storage budget profile decisions and allocation-site enforcement;
4. branch lifecycle completeness, including fork-on-dirty-source semantics;
5. retention defaults and periodic scheduling.

## Scope Summary

| Group | Gaps Covered | Required Before V1 | Primary Decision |
| --- | --- | --- | --- |
| L8D-A. Durable Table Manifest Stack | TM1, TM2, TR1, TR2, FW1, RP1 | Yes for TM1; decision for TM2 | Lock byte stability with golden vectors; record provenance discrimination model. |
| L8D-B. Retention-Aware Row Pruning | RP-V1, RP-V2, RP-V3 | Yes | Verify history/as-of/timestamp reads reject below floor and inherited-layer safety holds. |
| L8D-C. Storage Budget Profiles | BG1, BG2 | Decision required | Pick embedded production profile; add allocation-site source guard. |
| L8D-D. Branch Lifecycle Completeness | BL1, BL2, BL3, BL4 | Yes for BL2/BL3/BL4; decision for BL1 | Record fork-on-dirty policy; verify race-free clear/delete/reclaim. |
| L8D-E. Retention Defaults And Periodic Scheduling | RT1, RT2, RT3 | Decision required | Record default snapshot count and periodic-pass owner. |

## Existing Baseline

Assume the following implementation is already in place:

1. `LifecycleDurableTableCatalog` records tables with monotonic
   `manifest_sequence` and rejects sequence regressions or conflicting
   identities;
2. `TableManifest` carries `levels`, `inherited_layers`, table refs, bounds,
   facts, and provenance, plus a `RetainedHistoryExtensionPayload`;
3. recovery uses `validate_flush_watermark_is_recoverable` and
   `preflight_table_manifest_with_checkpoint` before installing either source;
4. `LifecycleTableObjectProofContext` + `LifecycleTableObjectProofToken`
   gate reclaim against a typed proof context;
5. `lifecycle/rewrite_publication.rs` publishes compaction and materialization
   outputs through manifest-backed paths;
6. `lifecycle/budget.rs` exposes seven explicit pools and a five-tier pressure
   severity model, plus a `low_memory_test_profile`;
7. `LifecycleBranchCatalog` supports create/clear/delete/fork/fork-at-version/
   fork-at-timestamp, pinned reachability, and recovery exclusivity tokens.

If any of those regress while implementing L8D, stop and restore the existing
behavior before adding broader parity mechanics.

## Non-Goals

L8D must not implement:

1. backwards-compatible durable byte migration for pre-V1 databases;
2. public maintenance commands or product retry UX;
3. multi-process or distributed table-manifest publication;
4. new L5 row merge algorithms;
5. new L6 branch install semantics beyond proof-gated invariants;
6. new pressure thresholds — those remain L8B-owned;
7. recovery-strictness defaults or close deadlines — those remain L8C-owned.

## L8D-A. Durable Table Manifest Stack

Gaps covered: TM1, TM2, TR1, TR2, FW1, RP1.

Goal: lock the durable table-manifest format, provenance discrimination, and
publication fault windows before L9 begins consuming table-state facts.

Tasks:

1. Add golden-vector fixtures for `TableManifest` encoding.
   - One fixture per supported provenance variant.
   - One fixture per documented extension section, including
     `RetainedHistoryExtensionPayload`.
   - Stored under `testdata/goldens/storage-format-v1/` with a versioned
     manifest header.
   - Counter: `format_table_manifest_golden_assertions`.
2. Add corruption fuzz coverage on the table manifest decoder.
   - Reuses the existing `format_*` fuzz targets where possible.
   - Covers: bad version, sequence regression, unknown extension section,
     mismatched checksum, truncated payload, oversized payload.
3. Decide and document table provenance discrimination.
   - Confirm which `TableManifestTableProvenance` variants exist in V1
     (`Flush`, `Compact`, `Materialize`, others) and whether recovery uses
     provenance to choose between checkpoint and manifest as authoritative.
   - If provenance is purely diagnostic, record that decision; if it gates
     recovery branches, the test plan must cover every variant.
4. Verify proof token epoch validation under concurrent publication.
   - Issue a proof token, publish a new table manifest sequence, then
     attempt reclaim using the old token.
   - Test must assert the old token fails `validates_for` even when the
     proof context still appears live.
5. Verify branch-absence guard on flush watermark advance.
   - Plan §"Flush And Flush Watermark" rule 2 forbids advancing the flush
     watermark from branch absence.
   - Add a fixture with a branch that has not flushed yet and confirm
     `validate_flush_watermark_is_recoverable` does not advance the
     watermark on its behalf.
6. Verify durable rewrite publication fault windows.
   - Implement and test each fault window required by the broader L8 test
     plan §20 cases 11 and 12:
     - branch/table manifest publish fails after table object publish;
     - compaction output published then branch swap fails.
   - Each case must leave reads unchanged and record health debt.
7. Counters:
   - `table_manifest_publish_attempts`,
   - `table_manifest_publish_failures`,
   - `table_manifest_recovery_preflight_rejected`,
   - `table_object_proof_token_revoked`,
   - `table_object_proof_token_validated`.

Exit gates:

1. Golden vectors lock the table manifest byte format under
   `cargo test format_goldens`.
2. Fuzz target rejects every documented corruption class without panicking.
3. Proof token epoch validation rejects stale tokens.
4. Branch-absence flush-watermark advance is impossible without a typed
   error.
5. Fault windows leave reads unchanged and record typed health debt.

## L8D-B. Retention-Aware Row Pruning

Gaps covered: RP-V1, RP-V2, RP-V3.

Goal: prove that pruning past a floor cannot make subsequent history,
timestamp, or as-of reads return wrong results, including across inherited
layers.

Tasks:

1. Enforce read-side rejection below the retained version floor.
   - `branch.read_history(key, version)` must return a typed
     `BranchHistoryUnavailable` (or equivalent) when `version <
     retained_version_floor`.
   - `branch.read_at_timestamp(key, ts)` must return the equivalent typed
     error when `ts < retained_timestamp_floor`.
   - As-of point lookup must apply the same rule when an as-of version is
     requested below floor.
2. Wire the retained-history extension into the read path on recovery.
   - Confirm `branch.set_timestamp_coverage` runs after recovery installs
     the manifest's `RetainedHistoryFacts`, so post-reopen reads see the
     same floor.
3. Add proof-gated TTL/tombstone pruning checks.
   - Verify `BranchCompactionRetentionPolicy::DropTombstones` only drops
     when the proof asserts no live snapshot still references the tombstone
     range.
   - Verify `DropExpired` only drops when the TTL is past for every
     retained snapshot.
   - Add a "negative proof" test that confirms missing proof keeps
     tombstones alive.
4. Enforce inherited-layer pruning safety.
   - A parent branch's compaction must not prune a row version still
     visible through a child's inherited-layer view.
   - Verify by forking a child at a pre-prune version, then compacting the
     parent with a pruning proof; child reads must remain correct.
5. Add counters:
   - `branch_read_rejected_below_version_floor`,
   - `branch_read_rejected_below_timestamp_floor`,
   - `compaction_versions_pruned`,
   - `compaction_tombstones_pruned`,
   - `compaction_expired_rows_pruned`,
   - `compaction_pruning_blocked_by_inherited_layer`.

Exit gates:

1. Reads below the retained floor return typed errors, not stale or empty
   data.
2. TTL/tombstone pruning is proof-gated and reversible-on-no-proof.
3. Inherited-layer reads remain correct after parent pruning.
4. Counters expose pruning shape to benchmarks and tests.

## L8D-C. Storage Budget Profiles

Gaps covered: BG1, BG2.

Goal: make budget profiles a typed product of storage-next configuration and
prove that every production allocation respects a pool.

Tasks:

1. Add a documented embedded production profile.
   - Distinct from `low_memory_test_profile` (64 KiB total) which is too
     small for production embedded use.
   - Suggested target: 8-32 MiB total for typical embedded.
   - Validate against the existing budget invariants.
2. Add an allocation-site source guard.
   - Reject any `Vec::with_capacity`, `Box::new`, `Arc::new`, mmap, or
     similar allocation in production lifecycle code that is not preceded
     by a `require_*` call against a documented `StorageBudgetPool`.
   - Add an exemption list for trivial allocations (e.g., single-byte
     reservations) with explicit `// budget-exempt:` markers.
3. Decide the budget-exhaustion behavior for each pool.
   - Some pools (BlockCache) should evict.
   - Some pools (ActiveMutable) should reject mutating admission.
   - Some pools (MaintenanceQueue) should defer optional maintenance.
   - The current implementation has a `StorageBudgetPressureSeverity` enum
     covering these; verify each pool uses the correct severity.
4. Add allocation telemetry.
   - `budget_pool_reservations_granted` per pool,
   - `budget_pool_reservations_denied` per pool,
   - `budget_pool_releases` per pool.
5. Verify that the `BudgetedCommitBranch` enforces commit-time admission
   under each tier.

Exit gates:

1. Production embedded profile is documented and tested.
2. Allocation source guard catches new lifecycle allocations without a
   pool.
3. Pressure severity per pool matches the documented exhaustion behavior.
4. Telemetry counters are present on every reserve/release path.

## L8D-D. Branch Lifecycle Completeness

Gaps covered: BL1, BL2, BL3, BL4.

Goal: lock down race-prone branch operations and record the fork-on-dirty
decision before L9 exposes branch mechanics.

Tasks:

1. Decide fork-on-dirty-source policy.
   - Option A: keep current strict rejection (`SourceHasUnflushedRows`)
     and document the "callers must flush before fork" requirement.
   - Option B: auto-flush before fork inside `fork_current`.
   - Option C: copy active + frozen rows into the child (old-engine
     behavior) — much more code, deferred.
   - Whichever option lands, the semantic decision register must record
     the choice.
2. Verify generation guard under concurrent clear or delete.
   - Two simultaneous `clear_branch` calls for the same branch must result
     in exactly one success and one typed
     `LifecycleError::GenerationGuardFailed` (or equivalent).
   - Likewise for `delete_branch` and for `clear` racing `delete`.
3. Verify deleted-branch reclaim race protection.
   - A branch in `LifecycleBranchStatus::Deleted` must keep its release
     plan visible to retention until table-object reachability proof is
     fresh.
   - Test: delete a branch with `n` table objects; reclaim must consult the
     release plan; the table objects must transition through quarantine
     before purge.
4. Verify `RecoveryExclusivityToken` single-use semantics.
   - The token must be acquired exactly once per recovery pass.
   - Attempting to acquire twice must return a typed error.
   - Acquisition during normal operation (non-recovery) must fail.
5. Add counters:
   - `branch_clear_attempts`, `branch_clear_failures`,
   - `branch_delete_attempts`, `branch_delete_failures`,
   - `branch_fork_rejected_dirty_source`,
   - `recovery_exclusivity_token_acquired`,
   - `recovery_exclusivity_token_rejected`.

Exit gates:

1. Fork-on-dirty policy is recorded.
2. Generation guard is provably race-free under property test.
3. Deleted branches cannot lose their release plan before reclaim closes.
4. Recovery exclusivity token cannot be acquired twice.

## L8D-E. Retention Defaults And Periodic Scheduling

Gaps covered: RT1, RT2, RT3.

Goal: record the retention configuration and scheduling owner before L9
opens the storage API.

Tasks:

1. Decide default `retain_newest_snapshots`.
   - Current hardcoded value is 1.
   - Option A: make it part of `LifecycleConfig` with an explicit default.
   - Option B: assign to L9 or engine-next configuration.
   - Whichever option lands, the default must be reachable through
     `StorageOpenPlan` and the decision register.
2. Verify live-manifest-snapshot exception.
   - Plan §"Retention And Snapshot Pruning" rule 5 requires the live
     manifest snapshot to be retained even if older than the count policy.
   - Verify `SnapshotService::prune_snapshots(live_snapshot_id,
     retain_newest)` enforces this; add a test with `retain_newest = 1`
     and a live snapshot older than one other snapshot.
3. Decide periodic retention scheduling.
   - Option A: enqueue `MaintenanceTaskKind::Retention` automatically from
     `HealthCollection` runs.
   - Option B: leave retention as on-demand only, schedule from L9 or
     engine-next.
   - Option C: post-V1 — record as deferred.
   - Whichever option lands, the documented schedule (or absence of one)
     must be recorded.
4. Add counters:
   - `retention_passes_scheduled`,
   - `retention_passes_skipped_proof_incomplete`,
   - `retention_passes_skipped_degraded_health`,
   - `snapshot_pruning_outcomes_deferred`,
   - `snapshot_pruning_outcomes_completed`.

Exit gates:

1. Default snapshot count and ownership are recorded.
2. Live-snapshot exception is enforced and tested.
3. Periodic retention policy is documented (auto, on-demand, or deferred).
4. Counters expose retention activity to benchmarks and tests.

## Execution Order

Recommended order:

1. L8D-A1 and L8D-A2 — golden vectors and fuzz coverage first; they lock
   the byte format before any other manifest work can move it.
2. L8D-B1 and L8D-B2 — pruning rejection on the read path; this is a
   correctness safety net independent of manifest work.
3. L8D-C1 and L8D-C2 — production budget profile and allocation guard;
   independent of A/B and gates the L9 open contract.
4. L8D-A3 to A7 — manifest provenance, fault windows, counters.
5. L8D-D1 to D5 — branch lifecycle work; can land in parallel with A's
   later steps.
6. L8D-B3 to B5 — TTL/tombstone and inherited-layer pruning; depends on
   the read-side rejection landing.
7. L8D-E1 to E4 — retention defaults and scheduling; depends on A/B/D
   being stable so the periodic pass has clean infrastructure to call.

## Stop Conditions

Stop and revise this plan if:

1. table manifest byte stability requires changing the L3 format crate's
   public traits;
2. inherited-layer pruning safety needs L6 branch install semantics that
   are owned by a separate slice;
3. embedded budget profile cannot satisfy the lifecycle invariants without
   special-casing pool limits;
4. fork-on-dirty resolution requires changing the L6 branch fork
   primitive;
5. periodic retention scheduling needs a background thread;
6. proof token epoch validation depends on backend trait changes the
   L1 layer has not exposed.

## Verification Commands

Focused commands:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::table_manifest_recovery
cargo test -p strata-storage-next --locked --lib lifecycle::tests::retention
cargo test -p strata-storage-next --locked --lib lifecycle::tests::budget_runtime
cargo test -p strata-storage-next --locked --lib lifecycle::tests::flush_watermark
cargo test -p strata-storage-next --locked --test lifecycle_reclaim_close
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --locked --test format_goldens
```

Full storage-next gates:

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

## Completion Criteria

L8D is complete when:

1. table manifest byte format is locked by golden vectors and corruption
   fuzz;
2. table provenance discrimination is documented and tested;
3. proof token epoch validation rejects stale tokens;
4. branch-absence flush-watermark advance is impossible without typed
   error;
5. durable rewrite publication fault windows are typed, retryable, and
   reads stay unchanged;
6. history, as-of, and timestamp reads reject below the retained floor;
7. TTL/tombstone pruning is proof-gated and inherited-layer-safe;
8. production embedded budget profile is documented and tested;
9. allocation-site source guard rejects unbudgeted lifecycle allocations;
10. fork-on-dirty policy is recorded;
11. branch generation guard is race-free under property tests;
12. deleted branches cannot lose their release plan before reclaim;
13. recovery exclusivity token cannot be acquired twice;
14. default snapshot count and live-snapshot exception are recorded and
    tested;
15. periodic retention scheduling policy is documented (auto, on-demand,
    or deferred).
