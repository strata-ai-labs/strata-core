# L8J Test Plan: Checkpoint, Flush Watermark, And WAL Truncation

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8j-checkpoint-watermark-wal-truncation-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that lifecycle-owned checkpoint, flush-watermark, and WAL-truncation
operations shorten durable replay only when recovery can validate the proof.

Tests should fail if L8J:

1. selects a checkpoint watermark from allocated-but-not-visible versions;
2. captures rows without commit quiesce;
3. drops tombstones, timeline rows, branch ids, commit versions, or timestamps
   from checkpoint sections;
4. persists checkpoint manifest facts before the snapshot object is durable;
5. treats a partial checkpoint window as clean success;
6. persists a flush watermark from table-object publication alone;
7. advances a flush watermark from branch absence or stale branch facts;
8. truncates WAL without a typed `WalRetentionProof`;
9. deletes the active WAL segment or an uncovered segment;
10. hides WAL delete failures instead of surfacing maintenance health debt;
11. lets cache mode create checkpoint, manifest, WAL, or retention claims;
12. copies old primitive checkpoint DTOs or product vocabulary into
    storage-next lifecycle code/tests.

Do not add tests whose only assertion is that plan documents exist or link to
other plan documents.

## Old-Code Regression Sources

The old codebase supplies regression behaviors, not API shapes.

| Old source | Behavior to preserve | Storage-next assertion |
|---|---|---|
| `crates/engine/src/database/compaction.rs::Database::checkpoint` | WAL is flushed first, commit admission is quiesced, and the watermark comes from fully-applied work. | Checkpoint uses L7 quiesce and the visible-version tracker; allocated or hidden rows are excluded. |
| `crates/storage/src/durability/checkpoint_runtime.rs::run_storage_checkpoint` | Active WAL segment is persisted before checkpointing; snapshot is written before manifest snapshot facts. | Runtime delegates to `CheckpointService` and preserves service call order in direct tests. |
| `crates/storage/src/durability/disk_snapshot/checkpoint.rs::CheckpointCoordinator::checkpoint` | Snapshot id and watermark are updated only after snapshot write succeeds. | Failed snapshot publish does not advance checkpoint or flush facts. |
| `crates/engine/src/database/compaction.rs::Database::compact` | In-memory active WAL segment protects segments beyond stale manifest facts. | WAL truncation test proves active/newer segments remain after retention. |
| `crates/storage/src/durability/compaction/wal_only.rs::WalOnlyCompactor::compact_with_active_override` | Effective retention watermark deletes only fully-covered, non-active segments. | L8 passes typed proof to L4 and asserts L4 delete report, not direct segment inspection in lifecycle. |
| `crates/storage/src/durability/compaction/wal_only.rs::segment_covered_by_watermark` | Codec-aware WAL scanning is below the lifecycle layer. | Source guard rejects lifecycle WAL record scanning or direct object-name parsing. |
| `crates/storage/src/durability/checkpoint_runtime.rs::truncate_storage_wal_after_flush` | Flush-watermark truncation is best-effort after a persisted watermark. | Storage-next narrows the proof: checkpoint-covered flush watermark only until table-manifest recovery exists. |
| `crates/storage/src/segmented/mod.rs::flush_oldest_frozen` | Frozen state remains visible during I/O and is replaced atomically. | Checkpoint tests include rows that crossed the flush boundary, but flush alone does not prove replay shortening. |
| `crates/engine/src/database/lifecycle.rs::prune_snapshots_once` | Pruning is nonfatal and preserves the manifest-live snapshot. | Any pruning hook records health debt and cannot fail an otherwise completed checkpoint. |

Tests must not port:

1. old primitive section names as storage-next row sections;
2. old `Database::checkpoint` / `Database::compact` public command behavior;
3. direct filesystem, path, or segment filename logic in lifecycle code;
4. logs-only diagnostics as proof of a handled fault.

## Test Locations

Use:

1. `crates/storage-next/src/lifecycle/tests/checkpoint.rs` for direct
   checkpoint, flush-watermark, and WAL-truncation runtime tests.
2. `crates/storage-next/src/lifecycle/tests/checkpoint/shared.rs` if fakes or
   fixtures push the main file near 1,000 lines.
3. `crates/storage-next/src/lifecycle/tests/maintenance.rs` only for executor
   dispatch tests shared with the generic maintenance runtime.
4. `crates/storage-next/src/lifecycle/tests/recovery.rs` for checkpoint
   recovery round trips and invalid manifest/watermark recovery interactions.
5. `crates/storage-next/src/testkit/lifecycle/checkpoint.rs` for generated
   checkpoint/watermark/truncation scripts.
6. `crates/storage-next/src/testkit/lifecycle/maintenance.rs` for generated
   maintenance counters.
7. `crates/storage-next/tests/lifecycle_maintenance.rs` for integration smoke
   tests through the lifecycle maintenance entry point.
8. `crates/storage-next/tests/lifecycle_properties.rs` for generated lifecycle
   properties behind `testkit`.
9. `crates/storage-next/tests/lifecycle_source_guard.rs` for source-boundary
   checks.
10. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for
    verification and sensitivity-probe records after implementation.

Do not put the main checkpoint assertions in a doc-closeout test. The closeout
test can check source boundaries and counters, but behavior belongs in runtime
tests.

## Test Data Principles

1. Build committed rows through L7/L6 helpers where possible.
2. Use row-native `StorageRow` fixtures; do not use primitive DTOs.
3. Include multiple branches, tombstones, timeline rows, expiring rows, and
   materialized replacement rows in at least one checkpoint fixture.
4. Keep row keys free of reserved layout literals unless testing layout guards.
5. Distinguish visible, applied-but-not-visible, durable-but-not-visible, and
   allocated-only versions.
6. Exercise checkpoint rows that originated from active, frozen, and owned
   immutable branch state.
7. Exercise rows that crossed a flush boundary before checkpoint.
8. Assert stable lifecycle error codes and source-chain types, not display
   strings.
9. Keep canonical smoke scripts separate from generated input-derived
   coverage counters.
10. Generated properties must track counters for input-derived checkpoint,
    flush-watermark, and truncation actions separately from setup.

## Direct Unit Tests

### 1. Request And Outcome Validation

Required tests:

1. `checkpoint_request_rejects_zero_snapshot_id`
2. `checkpoint_request_rejects_empty_storage_scope`
3. `checkpoint_request_rejects_external_manifest_facts`
4. `checkpoint_request_rejects_product_section_vocabulary`
5. `checkpoint_outcome_reports_completed_snapshot_and_watermark`
6. `checkpoint_outcome_reports_deferred_no_visible_rows`
7. `checkpoint_outcome_reports_partial_snapshot_publication`
8. `checkpoint_outcome_reports_uncertain_manifest_publication`
9. `checkpoint_outcome_converts_to_maintenance_completed_status`
10. `checkpoint_outcome_converts_partial_to_health_debt`
11. `checkpoint_outcome_debug_uses_storage_vocabulary`
12. `flush_watermark_request_rejects_zero_candidate`
13. `flush_watermark_request_rejects_table_only_proof`
14. `wal_truncation_request_requires_typed_retention_proof`

Assertions:

1. invalid facts fail before lower-layer calls;
2. outcomes carry checkpoint, watermark, and WAL delete facts separately;
3. cache and durable outcomes cannot be confused.

### 2. Quiesce And Watermark Selection

Required tests:

1. `checkpoint_acquires_quiesce_before_reading_visible_version`
2. `checkpoint_releases_quiesce_after_row_capture_failure`
3. `checkpoint_releases_quiesce_after_service_failure`
4. `checkpoint_watermark_uses_visible_version_not_allocator_version`
5. `checkpoint_watermark_excludes_applied_not_visible_rows`
6. `checkpoint_watermark_excludes_durable_not_visible_rows`
7. `checkpoint_deferred_when_visible_version_is_zero`
8. `checkpoint_rejected_when_runtime_not_open`
9. `checkpoint_rejected_while_close_is_in_progress`
10. `checkpoint_rejected_in_cache_mode_without_durable_claims`

Assertions:

1. no mutating admission can enter between watermark selection and row capture;
2. allocated-only versions cannot appear in the checkpoint;
3. failed checkpoint attempts do not leave the branch guard set quiesced.

### 3. Row Collection

Required tests:

1. `checkpoint_rows_include_active_rows_at_or_below_watermark`
2. `checkpoint_rows_include_frozen_rows_at_or_below_watermark`
3. `checkpoint_rows_include_owned_table_rows_at_or_below_watermark`
4. `checkpoint_rows_include_materialized_replacement_rows`
5. `checkpoint_rows_include_tombstones`
6. `checkpoint_rows_include_timeline_rows`
7. `checkpoint_rows_preserve_branch_ids`
8. `checkpoint_rows_preserve_commit_versions`
9. `checkpoint_rows_preserve_commit_timestamps`
10. `checkpoint_rows_exclude_rows_above_watermark`
11. `checkpoint_rows_are_sorted_by_internal_key`
12. `checkpoint_rows_reject_duplicate_internal_keys_when_snapshot_install_would_reject`
13. `checkpoint_rows_do_not_log_or_debug_payload_bytes`
14. `checkpoint_rows_round_trip_through_snapshot_install`

Assertions:

1. collection uses L6-owned row ordering and validation;
2. no lifecycle code reconstructs internal keys by string parsing;
3. checkpoint row facts can be consumed by recovery without WAL replay below
   the checkpoint watermark.

### 4. Checkpoint Service Ordering

Required tests:

1. `checkpoint_persists_active_wal_segment_before_snapshot_publish`
2. `checkpoint_publishes_snapshot_before_manifest_snapshot_facts`
3. `checkpoint_uses_checkpoint_service_not_manifest_direct_write`
4. `checkpoint_does_not_create_table_or_quarantine_objects`
5. `checkpoint_snapshot_publish_failure_leaves_manifest_unchanged`
6. `checkpoint_manifest_publish_failure_reports_partial_snapshot`
7. `checkpoint_final_manifest_uncertainty_reports_uncertain_status`
8. `checkpoint_existing_snapshot_id_collision_fails_closed`
9. `checkpoint_snapshot_id_must_advance_past_recovered_manifest_snapshot_id`
10. `checkpoint_retry_after_orphan_snapshot_distinguishes_same_snapshot_from_collision`

Assertions:

1. lifecycle delegates all durable checkpoint publication to L4;
2. snapshot success without manifest success is not clean success;
3. active WAL facts are visible in the outcome.

### 5. Checkpoint Recovery Round Trips

Required tests:

1. `checkpoint_recovery_restores_branch_rows_without_covered_wal`
2. `checkpoint_recovery_restores_tombstones_without_covered_wal`
3. `checkpoint_recovery_restores_timeline_rows_without_covered_wal`
4. `checkpoint_recovery_replays_tail_after_checkpoint_watermark`
5. `checkpoint_recovery_ignores_duplicate_record_at_checkpoint_watermark`
6. `checkpoint_recovery_rejects_manifest_snapshot_missing_object_in_strict_mode`
7. `checkpoint_recovery_marks_missing_snapshot_data_loss_in_lossy_mode`
8. `checkpoint_recovery_rejects_flush_watermark_above_checkpoint_coverage`
9. `checkpoint_recovery_accepts_flush_watermark_at_checkpoint_coverage`
10. `checkpoint_recovery_round_trip_after_frozen_flush`

Assertions:

1. recovery can rebuild every row at or below checkpoint without WAL;
2. tail replay starts strictly after the trusted checkpoint watermark;
3. manifest snapshot and flush facts agree with recovery invariants.

### 6. Flush-Watermark Proofs

Required tests:

1. `flush_watermark_accepts_checkpoint_covered_candidate`
2. `flush_watermark_accepts_already_persisted_candidate`
3. `flush_watermark_rejects_candidate_above_checkpoint_watermark`
4. `flush_watermark_rejects_candidate_above_visible_version`
5. `flush_watermark_rejects_table_flush_only_candidate`
6. `flush_watermark_rejects_branch_absence_candidate`
7. `flush_watermark_is_monotonic`
8. `flush_watermark_equal_to_current_is_noop`
9. `flush_watermark_persist_failure_preserves_source_chain`
10. `flush_watermark_success_updates_manifest_facts_only_after_proof`
11. `flush_watermark_does_not_run_in_cache_mode`
12. `flush_watermark_does_not_mutate_branch_state`

Assertions:

1. L8I table-object publication is not a replay-shortening proof;
2. accepted flush watermark facts cannot make recovery reject the database;
3. lower-layer manifest errors remain available through `Error::source`.

### 7. WAL Truncation

Required tests:

1. `wal_truncation_from_checkpoint_watermark_uses_snapshot_retention_proof`
2. `wal_truncation_from_flush_watermark_uses_flush_retention_proof`
3. `wal_truncation_rejects_primitive_watermark_without_proof`
4. `wal_truncation_deletes_only_fully_covered_segments`
5. `wal_truncation_keeps_segment_with_record_above_watermark`
6. `wal_truncation_keeps_active_segment`
7. `wal_truncation_keeps_segment_newer_than_active_segment`
8. `wal_truncation_handles_empty_segment_through_l4_report`
9. `wal_truncation_preserves_l4_delete_error_source`
10. `wal_truncation_delete_failure_records_health_debt`
11. `wal_truncation_partial_delete_report_is_not_clean_reclaim`
12. `wal_truncation_no_segments_is_completed_noop`
13. `wal_truncation_does_not_scan_wal_records_in_lifecycle`
14. `wal_truncation_does_not_parse_segment_object_names_in_lifecycle`

Assertions:

1. lifecycle never owns segment coverage logic;
2. L4 is the only WAL segment deletion path;
3. delete failures do not invalidate a completed checkpoint but are reported.

### 8. Checkpoint Plus WAL Truncation

Required tests:

1. `checkpoint_with_truncation_builds_snapshot_proof_after_manifest_publish`
2. `checkpoint_with_truncation_skips_truncation_when_checkpoint_deferred`
3. `checkpoint_with_truncation_reports_checkpoint_success_and_truncation_failure`
4. `checkpoint_with_truncation_does_not_truncate_after_partial_checkpoint`
5. `checkpoint_with_truncation_keeps_tail_records_after_watermark`
6. `checkpoint_with_truncation_recovery_round_trip_after_delete`
7. `checkpoint_with_truncation_preserves_active_wal_segment`
8. `checkpoint_with_truncation_records_both_checkpoint_and_delete_metrics`

Assertions:

1. truncation proof is created only after trusted checkpoint facts exist;
2. partial checkpoint cannot be used to shorten replay;
3. deleted WAL is covered by a recovery round trip.

### 9. Maintenance Integration

Required tests:

1. `checkpoint_task_runs_through_maintenance_executor`
2. `checkpoint_task_deferred_when_no_visible_rows`
3. `checkpoint_task_failure_adds_health_debt`
4. `checkpoint_task_rejected_after_close_requested`
5. `checkpoint_task_canceled_before_start_does_not_call_service`
6. `duplicate_checkpoint_tasks_coalesce_by_storage_scope`
7. `wal_truncation_task_runs_through_maintenance_executor`
8. `wal_truncation_task_deferred_without_retention_proof`
9. `wal_truncation_task_failure_adds_health_debt`
10. `duplicate_wal_truncation_tasks_coalesce_by_retention_scope`
11. `maintenance_stats_include_checkpoint_and_truncation_counts`
12. `maintenance_outcome_preserves_checkpoint_and_truncation_facts`

Assertions:

1. executor admission and cancellation policy remains owned by L8H;
2. checkpoint/truncation runners are deterministic;
3. maintenance health debt is surfaced through the common outcome model.

### 10. Cache Mode

Required tests:

1. `cache_checkpoint_returns_deferred_or_unsupported_without_durable_claims`
2. `cache_checkpoint_does_not_call_checkpoint_service`
3. `cache_flush_watermark_rejected_without_manifest_claim`
4. `cache_wal_truncation_rejected_without_wal_claim`
5. `cache_maintenance_checkpoint_task_does_not_create_durable_objects`
6. `cache_maintenance_wal_truncation_task_does_not_create_durable_objects`
7. `cache_mode_source_guard_blocks_checkpoint_manifest_wal_imports`

Assertions:

1. cache mode never creates durable recovery or retention facts;
2. cache outcomes use explicit unsupported/deferred status instead of fake
   success.

### 11. Error Codes And Source Chains

Required tests:

1. `checkpoint_snapshot_service_error_has_stable_code`
2. `checkpoint_manifest_service_error_has_stable_code`
3. `checkpoint_orphan_snapshot_has_stable_code_or_partial_status`
4. `flush_watermark_manifest_error_has_stable_code`
5. `wal_truncation_service_error_has_stable_code`
6. `checkpoint_errors_preserve_lower_layer_source_chain`
7. `flush_watermark_errors_preserve_lower_layer_source_chain`
8. `wal_truncation_errors_preserve_lower_layer_source_chain`
9. `checkpoint_error_tests_do_not_assert_display_strings`
10. `checkpoint_error_debug_redacts_payload_bytes`

Assertions:

1. all failures return lifecycle error codes in the standard format;
2. lower-layer source chains are available for diagnostics;
3. tests never rely on user-facing display text.

### 12. Source Guards

Required guards:

1. lifecycle checkpoint code does not import `std::fs`, `std::path::Path`,
   `std::fs::File`, `OpenOptions`, mmap, or `std::env`;
2. lifecycle checkpoint code does not import old storage modules;
3. lifecycle checkpoint code does not import product/engine/database modules;
4. lifecycle checkpoint code does not contain primitive DTO names from the old
   checkpoint path;
5. lifecycle checkpoint code does not contain architecture slice labels in
   code, test names, comments, or panic messages;
6. cache-mode lifecycle code does not import manifest, WAL, snapshot,
   checkpoint, table-object, or quarantine services;
7. lifecycle WAL truncation code does not parse object names or segment names;
8. lifecycle WAL truncation code calls L4 delete APIs through typed retention
   proofs.

Source-guard tests may inspect source text, but they should protect real
module boundaries. Do not add guards for plan-document existence.

## Generated And Property Coverage

Add generated lifecycle scripts with input-derived counters for:

1. checkpoint requests accepted;
2. checkpoint requests deferred;
3. checkpoint service failures;
4. partial checkpoint windows;
5. row capture from active rows;
6. row capture from frozen rows;
7. row capture from owned tables;
8. tombstone checkpoint round trips;
9. timeline checkpoint round trips;
10. flush-watermark accepted by checkpoint proof;
11. flush-watermark rejected by table-only proof;
12. flush-watermark monotonic noop;
13. WAL truncation accepted;
14. WAL truncation rejected without proof;
15. WAL truncation delete failures;
16. checkpoint plus truncation recovery round trips;
17. cache-mode rejection/defer paths.

Generated tests must fail if a canonical setup script alone satisfies these
counters. Track setup counters separately from input-derived operation counters.

## Fault Windows

Required fault-window tests:

1. failure before quiesce is acquired leaves no state change;
2. failure during row capture releases quiesce and leaves no manifest change;
3. failure while encoding checkpoint section leaves no lower-layer calls;
4. failure before snapshot publish leaves no manifest snapshot facts;
5. snapshot published but manifest not updated returns partial checkpoint;
6. final manifest uncertainty returns uncertain checkpoint status;
7. checkpoint partial status cannot drive flush watermark proof;
8. checkpoint partial status cannot drive WAL truncation proof;
9. flush-watermark persist failure leaves previous manifest facts intact;
10. WAL delete failure after checkpoint success records health debt;
11. WAL delete partial success records exact delete facts;
12. recovery after each durable fault window is deterministic.

## Sensitivity Probe Ledger

Record this table in the porting log after implementation with concrete test
names and mutated files/lines.

| Probe | Mutation | Required failing test family |
|---|---|---|
| Quiesce omitted | Remove commit quiesce before checkpoint row capture | Quiesce and watermark tests |
| Allocator watermark used | Use allocated/latest version instead of visible version | Watermark boundary tests |
| Hidden rows captured | Include applied-not-visible rows in checkpoint | Recovery round-trip and boundary tests |
| Tombstones dropped | Filter tombstone rows from checkpoint section | Tombstone checkpoint recovery test |
| Timeline rows dropped | Filter timeline rows from checkpoint section | Timeline checkpoint recovery test |
| Snapshot/manifest order inverted | Persist manifest snapshot facts before snapshot object | Service ordering tests |
| Partial snapshot marked success | Collapse orphan/uncertain checkpoint to completed | Partial checkpoint tests |
| Table-only flush proof accepted | Allow L8I flush result to persist flush watermark | Flush proof rejection tests |
| Branch absence advances watermark | Treat missing branch as full flush coverage | Flush proof absence test |
| Primitive integer truncation | Bypass `WalRetentionProof` | Retention proof/source guard tests |
| Active segment deleted | Remove active-segment protection | WAL active segment tests |
| Delete failure ignored | Return clean success after L4 delete error | Health-debt tests |
| Cache mode creates durable facts | Allow cache checkpoint/truncation to call durable services | Cache-mode tests |
| Old primitive DTO imported | Reintroduce old checkpoint DTO names | Source guard |
| Architecture label added to code | Insert slice labels in code/tests/comments | Source guard |

## Verification Commands

Run at minimum:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::checkpoint
cargo test -p strata-storage-next --locked --lib lifecycle::tests::maintenance
cargo test -p strata-storage-next --locked --lib lifecycle::tests::recovery
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --all-features --locked --test object_layout_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

Run local filesystem coverage if the implementation adds a localfs-backed
checkpoint/recovery harness:

```bash
cargo test -p strata-storage-next --all-features --locked lifecycle_checkpoint -- --ignored
```

## Exit Criteria

L8J test coverage is closeable when:

1. checkpoint watermark selection is quiesced and visible-version based;
2. checkpoint rows include active, frozen, owned-table, tombstone, and timeline
   state at or below the watermark;
3. checkpoint service ordering is pinned;
4. partial checkpoint windows are typed and cannot drive retention;
5. recovery round-trips from checkpoint without covered WAL;
6. flush-watermark persistence accepts only recovery-valid proofs;
7. table-object-only flush candidates are rejected or explicitly deferred;
8. WAL truncation uses typed L4 retention proof only;
9. active and uncovered WAL segments are protected;
10. WAL delete failures record maintenance health debt;
11. cache mode cannot create durable checkpoint or retention facts;
12. generated scripts exercise input-derived checkpoint/watermark/truncation
    routes;
13. source guards block old-code primitive DTOs, direct path/delete logic,
    product imports, and architecture labels in code/tests;
14. porting log records old-code behaviors ported, old-code details rejected,
    verification commands, and sensitivity probes.
