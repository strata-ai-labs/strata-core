# L8T Test Plan: Table-Manifest-Backed Flush Watermarks

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8t-table-manifest-backed-flush-watermarks-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that table manifests can shorten WAL replay only when trusted
table-manifest recovery facts cover every row at or below the requested flush
watermark.

The suite must fail if L8T:

1. persists a flush watermark from table-object publication alone;
2. persists a flush watermark from table-manifest publication uncertainty;
3. persists a watermark above table-manifest coverage;
4. treats branch absence as coverage without a trusted branch lifecycle fact;
5. forgets timeline rows or tombstones in coverage;
6. starts WAL replay after an unvalidated flush watermark;
7. truncates WAL without a typed L4 retention proof;
8. deletes active or uncovered WAL segments;
9. lets cache mode claim table-manifest coverage;
10. imports raw WAL scanning, raw IO, product code, or primitive DTOs.

Do not add tests whose only assertion is that planning documents exist or link
to other planning documents.

## Coverage Boundary

Covered:

1. table-manifest flush coverage proof;
2. combined checkpoint/table-manifest coverage;
3. database manifest flush-watermark persistence from table coverage;
4. recovery validation of table-covered flush watermark;
5. WAL truncation through L4 proof;
6. stale/incomplete/unsafe coverage rejection;
7. generated/property counters;
8. source guards.

Not covered:

1. table-manifest format, covered by L8Q;
2. table-manifest publication/recovery, covered by L8R;
3. table-object retention/quarantine, covered by L8S/L8M;
4. durable rewrite publication, covered by L8U;
5. row pruning, covered by L8V;
6. branch lifecycle facts, covered by L8Y.

## Old-Code Regression Sources

| Old source | Behavior to preserve | Storage-next assertion |
|---|---|---|
| `checkpoint_runtime.rs::truncate_storage_wal_after_flush` | Flush watermark can shorten replay only when durable table state covers it. | Table-manifest-covered watermark persists and survives reopen after WAL truncation. |
| `wal_only.rs::effective_watermark` | Snapshot and flush watermarks both contribute to WAL retention. | L4 proof receives the accepted flush watermark, not a raw number. |
| `wal_only.rs::compact_with_active_override` | Active/newer WAL segments are protected. | Truncation keeps active and newer segments under table coverage. |
| `SegmentedStore::flush_oldest_frozen` | Flushed state remains visible and later durable once manifest facts are written. | Table object alone is not coverage; table manifest is required. |
| `recover_segments` | Manifest recovery must precede replay shortening. | Recovery validates table manifests before choosing replay start. |
| `gc_under_degradation.rs` | Corrupt manifest blocks unsafe cleanup. | Unsafe table-manifest health blocks flush-watermark proof. |

Tests must not port:

1. direct filesystem WAL deletion;
2. old public checkpoint/compact commands;
3. primitive checkpoint DTOs;
4. logs-only truncation assertions;
5. product branch policy.

## Test Locations

Use:

1. `crates/storage-next/src/lifecycle/tests/checkpoint.rs` or
   `crates/storage-next/src/lifecycle/tests/watermark.rs` for direct proof and
   recovery tests.
2. `crates/storage-next/src/lifecycle/tests/recovery.rs` for reopen and replay
   start validation.
3. `crates/storage-next/src/lifecycle/tests/flush.rs` for flush-to-manifest
   coverage fixtures.
4. `crates/storage-next/src/testkit/lifecycle/checkpoint.rs` for generated
   scripts.
5. `crates/storage-next/tests/lifecycle_maintenance.rs` for maintenance smoke.
6. `crates/storage-next/tests/lifecycle_source_guard.rs` for boundary tests.
7. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for
   verification and sensitivity-probe records after implementation.

Split direct tests into `tests/watermark/` if a file approaches 1,000 lines.

## Test Data Principles

1. Include rows in active, frozen, and owned table states.
2. Include a durable table object without a table manifest.
3. Include a durable table manifest with matching table object facts.
4. Include a table manifest with publication uncertainty.
5. Include multiple branches, including one branch with no table coverage.
6. Include tombstones, timeline rows, and expiring rows.
7. Include WAL records below, at, and above the candidate watermark.
8. Include active WAL segment protection cases.
9. Assert replay start and recovered reads, not only manifest fields.

## Direct Unit Tests

### 1. Proof Validation

Required tests:

1. `table_manifest_flush_proof_accepts_exact_coverage`
2. `table_manifest_flush_proof_accepts_coverage_above_checkpoint`
3. `table_manifest_flush_proof_rejects_missing_branch_coverage`
4. `table_manifest_flush_proof_rejects_active_rows_below_candidate`
5. `table_manifest_flush_proof_rejects_frozen_rows_below_candidate`
6. `table_manifest_flush_proof_rejects_table_object_without_manifest`
7. `table_manifest_flush_proof_rejects_manifest_publish_uncertain`
8. `table_manifest_flush_proof_rejects_stale_manifest_epoch`
9. `table_manifest_flush_proof_rejects_stale_recovery_health_epoch`
10. `table_manifest_flush_proof_rejects_candidate_above_visible_version`
11. `table_manifest_flush_proof_rejects_zero_candidate`
12. `table_manifest_flush_proof_is_deterministic_for_shuffled_inputs`

Assertions:

1. candidate must be covered by current trusted facts;
2. proof is bound to manifest and health freshness;
3. active/frozen state prevents table-manifest-only coverage.

### 2. Storage Row Family Coverage

Required tests:

1. `table_manifest_coverage_includes_user_rows`
2. `table_manifest_coverage_includes_tombstones`
3. `table_manifest_coverage_includes_timeline_rows`
4. `table_manifest_coverage_includes_materialized_replacement_rows`
5. `table_manifest_coverage_includes_inherited_layer_rows`
6. `table_manifest_coverage_rejects_timeline_gap`
7. `table_manifest_coverage_rejects_tombstone_gap`
8. `table_manifest_coverage_rejects_inherited_layer_gap`

Assertions:

1. coverage is over recoverable storage rows, not just table-object commit max;
2. timeline/tombstone rows cannot be silently dropped;
3. inherited table refs count as recovery dependencies.

### 3. Persisting Flush Watermark

Required tests:

1. `flush_watermark_persists_from_table_manifest_coverage`
2. `flush_watermark_persists_from_combined_checkpoint_and_table_manifest_coverage`
3. `flush_watermark_rejects_table_manifest_candidate_above_coverage`
4. `flush_watermark_rejects_table_manifest_candidate_below_current_as_stale`
5. `flush_watermark_equal_to_current_is_noop`
6. `flush_watermark_persist_failure_prevents_wal_truncation`
7. `flush_watermark_success_records_manifest_fact`
8. `flush_watermark_success_does_not_mutate_branch_state`
9. `flush_watermark_success_does_not_publish_table_manifest`
10. `cache_mode_rejects_table_manifest_flush_watermark`

Assertions:

1. database manifest update happens only after proof validation;
2. WAL truncation does not run if manifest persistence fails;
3. table-manifest publication remains L8R's job.

### 4. WAL Truncation From Table Coverage

Required tests:

1. `wal_truncation_from_table_manifest_flush_watermark_uses_typed_proof`
2. `wal_truncation_from_table_manifest_flush_watermark_deletes_covered_segments`
3. `wal_truncation_keeps_segment_with_record_above_table_manifest_watermark`
4. `wal_truncation_keeps_active_segment_under_table_manifest_watermark`
5. `wal_truncation_keeps_newer_than_active_segment`
6. `wal_truncation_delete_failure_records_health_debt`
7. `wal_truncation_partial_delete_report_preserves_source_chain`
8. `wal_truncation_does_not_parse_wal_objects_in_lifecycle`

Assertions:

1. L8T passes typed proof to L4;
2. L4 remains responsible for segment coverage and active protection;
3. lifecycle records deletion debt without hiding persisted watermark.

### 5. Recovery After Truncation

Required tests:

1. `recovery_accepts_flush_watermark_above_checkpoint_when_table_manifest_covers`
2. `recovery_rejects_flush_watermark_above_table_manifest_coverage`
3. `recovery_rejects_missing_table_manifest_for_flush_watermark`
4. `recovery_rejects_corrupt_table_manifest_for_flush_watermark`
5. `recovery_rejects_table_object_mismatch_for_flush_watermark`
6. `recovery_uses_table_manifest_flush_watermark_as_replay_start_after_validation`
7. `recovery_replays_wal_tail_above_table_manifest_flush_watermark`
8. `recovery_ignores_duplicate_record_at_table_manifest_flush_watermark`
9. `recovery_after_truncation_restores_latest_reads`
10. `recovery_after_truncation_restores_history_reads_within_retained_bounds`

Assertions:

1. recovery validates coverage before replay start;
2. WAL tail still replays correctly;
3. recovered reads match model state after covered segments are gone.

### 6. Incomplete And Unsafe Coverage

Required tests:

1. `unsafe_recovery_health_blocks_table_manifest_flush_proof`
2. `policy_downgrade_blocks_table_manifest_flush_proof`
3. `data_loss_blocks_table_manifest_flush_proof`
4. `telemetry_health_allows_unrelated_table_manifest_flush_proof`
5. `table_manifest_reachability_debt_blocks_flush_proof`
6. `quarantine_inventory_mismatch_blocks_flush_proof_when_relevant`
7. `branch_absence_does_not_advance_flush_watermark`
8. `missing_branch_lifecycle_fact_blocks_absence_coverage`

Assertions:

1. unsafe health prevents replay shortening;
2. proof debt is typed and visible;
3. branch absence is not proof before L8Y.

### 7. Maintenance Integration

Required tests:

1. `maintenance_task_can_request_table_manifest_flush_watermark`
2. `maintenance_task_coalesces_table_manifest_flush_watermark_by_candidate`
3. `maintenance_task_reports_deferred_when_table_coverage_missing`
4. `maintenance_task_reports_health_debt_on_wal_truncation_failure`
5. `maintenance_task_does_not_run_table_manifest_watermark_after_close_begins`
6. `maintenance_task_preserves_stats_for_watermark_and_truncation`
7. `maintenance_task_does_not_claim_checkpoint_execution`

Assertions:

1. table-manifest watermark work is a maintenance task, not a checkpoint task;
2. close admission still applies;
3. stats distinguish proof rejection from WAL deletion failure.

## Generated And Property Tests

Add table-manifest watermark operations:

```text
publish_table_manifest(branch, coverage)
set_visible_version(version)
add_active_row(branch, version)
persist_flush_watermark(candidate, proof_kind)
truncate_wal(candidate)
recover()
assert_model_reads()
```

Required counters:

1. table_manifest_watermark_accepted;
2. table_manifest_watermark_rejected;
3. combined_checkpoint_table_coverage;
4. active_row_blocked;
5. timeline_gap_blocked;
6. stale_proof_blocked;
7. wal_truncated_from_table_manifest;
8. recovery_after_table_truncation;
9. cache_rejected;
10. raw_wal_scan_absent.

Properties:

1. no accepted watermark exceeds model coverage;
2. recovery after truncation equals model state;
3. rejected proofs do not mutate database manifest;
4. cache mode never records table-manifest coverage;
5. shuffling branches/manifests does not change proof result.

## Source Guards

Required source guard tests:

1. `table_manifest_watermark_does_not_import_raw_io`
2. `table_manifest_watermark_does_not_scan_wal_segments`
3. `table_manifest_watermark_does_not_decode_table_bytes_directly`
4. `table_manifest_watermark_does_not_import_backend_delete`
5. `table_manifest_watermark_does_not_import_engine_or_product_crates`
6. `table_manifest_watermark_does_not_import_stratahub`
7. `table_manifest_watermark_does_not_import_primitive_modules`
8. `cache_mode_does_not_import_table_manifest_watermark_runner`

Forbidden production tokens include:

1. `std::fs`
2. `std::path::Path`
3. `read_dir`
4. `list_prefix` in watermark proof code
5. `decode_wal`
6. `WalRecord` scanning in lifecycle watermark code
7. `decode_immutable_table` in lifecycle watermark code
8. `delete_object`
9. `strata_engine`
10. `stratahub`
11. `primitive`

## Sensitivity Probes

Record probes in
`docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` after
implementation.

| Probe | Mutation | Expected failure |
|---|---|---|
| T1 | Accept table-object publication without manifest. | Proof validation test fails. |
| T2 | Ignore active rows below candidate. | Active-row blocker test fails. |
| T3 | Ignore timeline row gap. | Timeline coverage test fails. |
| T4 | Use branch absence as proof. | Branch absence test fails. |
| T5 | Persist watermark before proof validation. | Persist failure/order test fails. |
| T6 | Truncate WAL without typed proof. | WAL proof/source guard test fails. |
| T7 | Start replay after unvalidated flush watermark. | Recovery rejection test fails. |
| T8 | Allow cache mode coverage. | Cache rejection test fails. |
| T9 | Ignore stale manifest epoch. | Stale proof test fails. |
| T10 | Parse WAL segments in lifecycle. | Source guard fails. |

## Command Matrix

Mandatory commands before L8T closeout:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib lifecycle::tests::checkpoint
cargo test -p strata-storage-next --locked --lib lifecycle::tests::recovery
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

## Exit Gate

L8T test coverage is complete when:

1. table-manifest-backed flush-watermark proof accepts only fully covered
   candidates;
2. table-object-only and uncertain-manifest cases reject;
3. database manifest mutation and WAL truncation ordering is tested;
4. recovery after WAL truncation is tested against model reads;
5. unsafe health and incomplete proof block replay shortening;
6. cache mode cannot claim table-manifest coverage;
7. generated tests cover accept/reject/recover categories;
8. source guards enforce delegation boundaries;
9. sensitivity probes and command results are recorded in the porting log.
