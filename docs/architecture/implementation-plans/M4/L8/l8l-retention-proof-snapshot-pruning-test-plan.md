# L8L Test Plan: Retention Proof And Snapshot Pruning

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8l-retention-proof-snapshot-pruning-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that lifecycle retention never deletes without a current storage proof,
and that snapshot pruning preserves every snapshot still needed for recovery.

Tests should fail if L8L:

1. deletes a snapshot referenced by the live manifest;
2. deletes snapshots inside the configured newest retention window;
3. treats retain count zero as "delete everything";
4. deletes table objects directly instead of classifying them for quarantine;
5. truncates WAL or lists WAL segments from retention code;
6. permits reclaim under data-loss or policy-downgraded recovery health;
7. treats incomplete proof as clean success;
8. hides snapshot delete failures;
9. loses affected object names, reclaimed bytes, or source chains;
10. imports product retention-report, primitive, raw filesystem, or direct
    backend deletion behavior into lifecycle retention code.

Do not add tests whose only assertion is that plan documents exist or link to
other plan documents.

## Coverage Boundary

This test plan closes retention proof and snapshot pruning only.

In scope:

1. request/outcome validation;
2. retention proof completeness;
3. recovery-health gate;
4. object-family decisions;
5. snapshot pruning through `SnapshotService`;
6. maintenance routing for retention and snapshot pruning;
7. maintenance outcome facts and health debt;
8. generated testkit counters;
9. source guards.

Out of scope:

1. quarantine inventory publication;
2. table-object movement into quarantine;
3. purge of quarantine objects;
4. repair/reconciliation;
5. close-time drain policy;
6. public product retention reports;
7. row-version pruning in compaction.

Those belong to L8M, L8N, L8O, L9, or later retention work.

## Old-Code Regression Sources

The old codebase supplies safety behavior, not API names.

| Old source | Behavior to preserve | Storage-next assertion |
|---|---|---|
| `crates/storage/src/durability/checkpoint_runtime.rs::prune_storage_snapshots` | Retain newest N snapshots and always protect the manifest-live snapshot. | Snapshot pruning protects live and newest snapshots even when the live snapshot is outside the newest window. |
| `crates/storage/src/durability/checkpoint_runtime.rs` pruning tests | Retain count zero clamps to one; delete errors are reported but pruning continues. | Direct L8 tests assert clamping and per-object failure reporting. |
| `crates/engine/src/database/tests/snapshot_retention.rs` | Snapshot pruning is nonfatal after checkpoint success and works as no-op under the retention threshold. | Lifecycle maintenance outcome reports completed/noop or health debt without invalidating checkpoint facts. |
| `crates/storage/src/segmented/quarantine_protocol.rs::retention_snapshot` | Incomplete or degraded storage truth blocks retention attribution. | Proof builder reports incomplete/blocked and avoids backend access. |
| `crates/storage/src/segmented/quarantine_protocol.rs::quarantine_segment_if_unreferenced` | Unreferenced segments are staged through quarantine, not directly deleted. | Table objects become quarantine candidates; no direct delete call occurs. |
| `crates/storage/src/segmented/ref_registry.rs` | Runtime reference facts are a deletion barrier but not durable truth by themselves. | Runtime-only table reachability is insufficient for direct durable deletion. |
| `crates/storage/src/durability/compaction/wal_only.rs` | WAL retention uses typed watermark proof and active-segment protection. | Retention code delegates WAL deletion to L8J/L4 and source guards reject WAL scanning/deletion. |
| `crates/engine/src/database/retention_report.rs` | Product branch attribution joins storage facts above storage. | L8 tests assert raw storage decisions and reject product retention vocabulary. |

Tests must not port:

1. old public `retention_report()` DTOs;
2. branch-name/generation product attribution;
3. direct filesystem snapshot paths;
4. logs-only failure checks;
5. quarantine purge behavior;
6. raw WAL segment parsing.

## Test Locations

Use:

1. `crates/storage-next/src/lifecycle/tests/retention.rs` for direct retention
   proof and snapshot-pruning tests.
2. `crates/storage-next/src/lifecycle/tests/retention/` for shared fixtures if
   direct tests approach 1,000 lines.
3. `crates/storage-next/src/lifecycle/tests/checkpoint.rs` only for
   checkpoint/retention interaction tests that need existing checkpoint helpers.
4. `crates/storage-next/src/lifecycle/tests/maintenance.rs` for generic
   executor behavior reused by retention tasks.
5. `crates/storage-next/src/testkit/lifecycle/retention.rs` for generated proof
   and pruning scripts.
6. `crates/storage-next/tests/lifecycle_reclaim_close.rs` for integration smoke
   through lifecycle maintenance entry points.
7. `crates/storage-next/tests/lifecycle_maintenance.rs` only if the existing
   maintenance integration file remains the repo's grouped lifecycle target.
8. `crates/storage-next/tests/lifecycle_source_guard.rs` for source-boundary
   checks.
9. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for
   verification and sensitivity-probe records after implementation.

Do not put behavior assertions in a documentation closeout test.

## Test Data Principles

1. Use storage-shaped object names from `ObjectLayout` and service helpers.
2. Use row/table fixtures only when table reachability is the tested fact.
3. Keep snapshot ids nonzero.
4. Include live snapshot id both inside and outside the newest-retained window.
5. Include snapshot object listings with malformed names in service-level tests
   only; lifecycle tests should consume service errors.
6. Include recovery health variants: healthy, telemetry degraded, policy
   downgrade, data loss, and failed.
7. Include object families: snapshots, table objects, WAL, quarantine.
8. Assert stable error codes and source-chain classes, not display strings.
9. Assert operation logs where the contract says no backend access.
10. Keep architecture labels out of Rust code and fixture strings.

## Direct Unit Tests

### 1. Request And Outcome Validation

Required tests:

1. `retention_request_rejects_empty_scope_when_required`
2. `retention_request_accepts_zero_snapshot_retain_as_clamped_policy`
3. `retention_request_rejects_product_vocabulary_scope`
4. `snapshot_pruning_request_rejects_zero_live_snapshot_id`
5. `retention_outcome_reports_retained_pruned_skipped_and_delegated_counts`
6. `retention_outcome_reports_affected_object_names`
7. `retention_outcome_reports_reclaimed_bytes_when_known`
8. `retention_outcome_debug_uses_storage_vocabulary`
9. `retention_outcome_converts_incomplete_proof_to_deferred_maintenance`
10. `snapshot_pruning_outcome_converts_delete_failure_to_health_debt`

Assertions:

1. invalid facts fail before service calls;
2. deferred outcomes include reason class and no object mutation;
3. outcomes preserve source errors where lower layers fail.

### 2. Proof Completeness

Required tests:

1. `retention_proof_complete_with_manifest_snapshot_and_healthy_recovery`
2. `retention_proof_incomplete_without_manifest_snapshot_when_snapshots_exist`
3. `retention_proof_incomplete_without_manifest_snapshot_even_when_listing_empty`
4. `retention_proof_incomplete_without_branch_reachability_for_tables`
5. `retention_proof_incomplete_without_quarantine_inventory_for_purge_scope`
6. `retention_proof_blocks_on_data_loss_recovery_health`
7. `retention_proof_blocks_on_policy_downgrade_recovery_health`
8. `retention_proof_allows_telemetry_degraded_recovery_when_unrelated`
9. `retention_proof_records_missing_fact_family`
10. `retention_proof_does_not_upgrade_runtime_reachability_to_durable_truth`
11. `retention_proof_is_deterministic_for_shuffled_input_facts`

Assertions:

1. incomplete proof keeps all objects;
2. blocked proof performs no backend access;
3. proof decisions are sorted deterministically.

### 3. Snapshot Pruning

Required tests:

1. `snapshot_pruning_clamps_zero_retain_count_to_one`
2. `snapshot_pruning_retains_live_manifest_snapshot`
3. `snapshot_pruning_retains_live_snapshot_outside_newest_window`
4. `snapshot_pruning_retains_configured_newest_snapshots`
5. `snapshot_pruning_deletes_old_non_live_snapshots`
6. `snapshot_pruning_noops_when_under_retain_count`
7. `snapshot_pruning_is_idempotent_after_success`
8. `snapshot_pruning_delete_failure_records_health_debt_and_continues`
9. `snapshot_pruning_list_failure_preserves_service_source_chain`
10. `snapshot_pruning_malformed_listed_snapshot_fails_closed`
11. `snapshot_pruning_does_not_mutate_manifest_snapshot_facts`
12. `snapshot_pruning_does_not_create_wal_retention_proof`
13. `snapshot_pruning_cache_mode_rejects_before_backend_access`
14. `snapshot_pruning_object_candidate_mode_requires_declared_delete_capability`

Assertions:

1. live snapshot is never in the deleted set;
2. protected snapshots are listed separately from deleted snapshots;
3. failed deletes do not hide successful deletes;
4. cache mode cannot claim durable pruning.

### 4. Table Object Decisions

Required tests:

1. `reachable_table_object_is_retained`
2. `replaced_unreachable_table_object_is_quarantine_candidate`
3. `table_object_with_incomplete_reachability_is_retained_with_debt`
4. `table_object_from_materialization_replacement_preserves_source_identity`
5. `table_object_decision_lists_branch_and_table_identity`
6. `table_object_retention_never_calls_backend_delete`
7. `table_object_retention_never_calls_quarantine_mutation`
8. `table_object_retention_delegates_purge_to_later_repair_slice`
9. `table_object_retention_preserves_compaction_checkpoint_debt`
10. `table_object_retention_ignores_product_branch_attribution`

Assertions:

1. table objects are retained or classified for quarantine only when a caller
   supplies a durable table decision;
2. no direct delete occurs;
3. no row-version pruning is triggered.

### 5. WAL And Quarantine Delegation

Required tests:

1. `wal_objects_are_delegated_to_checkpoint_truncation`
2. `wal_retention_without_checkpoint_or_flush_proof_is_incomplete`
3. `wal_delegation_does_not_list_segments`
4. `wal_delegation_does_not_delete_segments`
5. `quarantine_objects_are_delegated_to_quarantine_slice`
6. `purge_request_is_deferred_without_fresh_safe_proof`
7. `purge_request_does_not_delete_inventory_objects`
8. `retention_health_debt_names_delegated_family`

Assertions:

1. L8L does not call `WalService::delete_covered_segments`;
2. L8L does not call quarantine mutation/purge APIs;
3. delegated families are visible in the outcome.

### 6. Maintenance Routing

Required tests:

1. `snapshot_pruning_task_builds_snapshot_scope`
2. `retention_task_builds_retention_scope`
3. `snapshot_pruning_task_rejected_before_open`
4. `retention_task_rejected_while_closing`
5. `snapshot_pruning_task_coalesces_by_scope_and_retain_policy`
6. `retention_task_coalesces_by_scope`
7. `snapshot_pruning_task_failure_adds_health_debt`
8. `retention_task_incomplete_proof_returns_deferred`
9. `retention_task_blocked_by_recovery_health_returns_failed_or_deferred_by_policy`
10. `retention_task_skips_unrelated_pending_tasks`
11. `global_retention_task_prunes_snapshots_through_durable_maintenance`

Assertions:

1. task admission uses the lifecycle state machine;
2. rejected work does not mutate storage;
3. coalescing does not lose explicit retain-count policy.

### 7. Error And Source Chains

Required tests:

1. `retention_incomplete_error_has_stable_code`
2. `retention_blocked_error_has_stable_code`
3. `snapshot_pruning_service_error_preserves_source`
4. `snapshot_pruning_delete_failure_preserves_backend_error`
5. `cache_retention_unsupported_uses_storage_error_code`
6. `retention_error_display_does_not_include_object_payload_bytes`

Assertions:

1. tests assert `code()`, not `Display`;
2. lower-layer source chains survive outcome conversion;
3. object names may be present, object bytes must not be present.

## Integration Tests

Add or extend `lifecycle_reclaim_close.rs` or the existing lifecycle
maintenance integration target with:

1. `lifecycle_snapshot_pruning_integration`
2. `lifecycle_retention_proof_integration`
3. `lifecycle_retention_blocks_unsafe_recovery_integration`
4. `lifecycle_table_retention_delegates_to_quarantine_integration`
5. `lifecycle_snapshot_pruning_delete_failure_integration`

These should run through the lifecycle maintenance surface, not only direct
helper functions.

## Generated Testkit Contract

Add `check_lifecycle_retention_contract`.

Counters:

1. `complete_proof_cases`
2. `incomplete_proof_cases`
3. `blocked_recovery_cases`
4. `snapshot_pruned_cases`
5. `snapshot_protected_cases`
6. `snapshot_delete_failure_cases`
7. `table_retained_cases`
8. `table_quarantine_candidate_cases`
9. `wal_delegated_cases`
10. `cache_deferred_cases`

The generated contract should decode input bytes into:

1. storage mode;
2. recovery health;
3. manifest snapshot id;
4. snapshot object ids;
5. retain-newest count;
6. branch/table reachability facts;
7. replaced table facts;
8. service fault point.

Keep canonical smoke coverage separate from input-derived counters. Property
tests should prove generated bytes influence at least one retention decision.

## Source Guards

Extend `lifecycle_source_guard.rs`.

Required checks:

1. retention code does not import engine modules;
2. retention code does not import product retention-report modules;
3. retention code does not import primitive modules;
4. retention code does not use raw `std::fs`, `Path`, `File`, `OpenOptions`,
   mmap, or `std::env`;
5. retention code does not call backend `delete_object` directly;
6. retention code does not call `WalService::delete_covered_segments`;
7. retention code does not call quarantine mutation or purge APIs;
8. retention code does not parse object-family paths by hand when L2/L4 helpers
   exist;
9. lower layers do not import lifecycle;
10. Rust code/tests do not include architecture slice labels.

Add fixture self-tests that prove each guard can fail.

## Sensitivity Probes

Record probes in
`docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` after
implementation.

| Probe | Mutation | Expected failure |
|---|---|---|
| L8L-S1 | Delete the manifest-live snapshot. | Live-snapshot pruning test fails. |
| L8L-S2 | Treat retain count zero as zero protected snapshots. | Clamp test fails. |
| L8L-S3 | Ignore newest-retained window. | Newest-window test fails. |
| L8L-S4 | Treat data-loss recovery as safe. | Recovery-health gate test fails. |
| L8L-S5 | Treat incomplete proof as completed. | Incomplete-proof test fails. |
| L8L-S6 | Directly delete a table object. | Table no-delete/source-guard test fails. |
| L8L-S7 | Call WAL truncation from retention code. | WAL source guard fails. |
| L8L-S8 | Call quarantine purge from retention code. | Quarantine source guard fails. |
| L8L-S9 | Hide snapshot delete failure. | Delete-failure health-debt test fails. |
| L8L-S10 | Drop affected object names from outcome. | Outcome object-name test fails. |
| L8L-S11 | Assert display string instead of code. | Error-code test/source review fails. |
| L8L-S12 | Import product retention report. | Source guard fails. |

## Verification Commands

Mandatory commands after implementation:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::retention
cargo test -p strata-storage-next --locked --lib lifecycle::tests::checkpoint
cargo test -p strata-storage-next --locked --test lifecycle_reclaim_close
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo fmt --package strata-storage-next --check
git diff --check
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
```

Optional localfs command if the implementation adds localfs-specific delete
coverage:

```bash
cargo test -p strata-storage-next --features localfs --locked --lib lifecycle::tests::retention
```

## Exit Gate

L8L is complete when:

1. proof-complete, proof-incomplete, and recovery-blocked cases are tested;
2. incomplete proof keeps objects and records debt;
3. unsafe degraded recovery blocks reclaim before backend access;
4. live manifest snapshot is protected;
5. newest snapshot window is protected;
6. snapshot delete failures produce health debt with source chains;
7. table objects are retained or classified for quarantine, never deleted;
8. WAL and quarantine families are delegated, never mutated;
9. cache mode cannot claim durable retention or pruning;
10. maintenance routing covers snapshot pruning and retention tasks;
11. generated testkit counters cover all required retention decision families;
12. source guards cover product, raw IO, direct delete, WAL, quarantine, and
    architecture-label drift;
13. sensitivity probes are recorded;
14. the verification commands pass.
