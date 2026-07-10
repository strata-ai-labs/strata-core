# L8I Test Plan: Flush Frozen State And Table Publication

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8i-flush-table-publication-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that L8I safely converts L6 frozen mutable state into L5/L4 immutable
table state through lifecycle maintenance, without owning lower-layer
algorithms or advancing checkpoint/WAL retention state.

Tests should fail if L8I:

1. mutates L6 frozen state before table bytes are built and publication is
   complete;
2. publishes table objects without L4 validation;
3. installs branch-owned tables without L6 validation;
4. loses rows, tombstones, branch ids, commit versions, or timestamps during
   flush;
5. advances the global flush watermark or truncates WAL segments;
6. treats branch absence as flush coverage proof;
7. deletes or quarantines orphaned table objects without a retention proof;
8. reports publication or install uncertainty as clean success;
9. lets cache mode call durable table, manifest, WAL, snapshot, checkpoint, or
   quarantine services;
10. uses product, engine, public command, or architecture-label vocabulary in
    code/tests.

Do not add tests whose only assertion is that plan documents exist or link to
other plan documents.

## Test Locations

Use:

1. `crates/storage-next/src/lifecycle/tests/flush.rs` for direct flush handler
   tests.
2. `crates/storage-next/src/lifecycle/tests/flush/shared.rs` for shared fake
   publishers/runners if direct tests approach 1,000 lines.
3. `crates/storage-next/src/lifecycle/tests/maintenance.rs` only for executor
   integration assertions shared with the generic maintenance runner.
4. `crates/storage-next/src/testkit/lifecycle/flush.rs` for generated flush
   scripts and model checks.
5. `crates/storage-next/src/testkit/lifecycle/maintenance.rs` for maintenance
   counters that dispatch flush operations.
6. `crates/storage-next/tests/lifecycle_maintenance.rs` for memory/durable
   integration smoke tests.
7. `crates/storage-next/tests/lifecycle_properties.rs` for generated lifecycle
   properties behind `testkit`.
8. `crates/storage-next/tests/lifecycle_source_guard.rs` for source-boundary
   checks.
9. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for
   verification and sensitivity-probe records after implementation.

## Test Data Principles

1. Build committed rows through existing L7/L6 helpers when possible.
2. Rotate active rows into frozen state explicitly; do not rely on hidden test
   setup side effects.
3. Build table bytes through `ImmutableTableBuilder`.
4. Publish durable table bytes through `TableObjectService`.
5. Reopen durable table objects through `TableObjectReaderService` before L6
   install.
6. Use storage row keys and branch ids; no product primitive DTOs.
7. Avoid reserved layout literals in fixture bytes and test names.
8. Assert error codes and source-chain presence, not display strings.
9. Keep canonical smoke cases separate from generated-input coverage.
10. Generated tests must count input-derived flush operations separately from
    fixed setup.

## Direct Unit Tests

### 1. Request And Outcome Validation

Required tests:

1. `flush_request_accepts_branch_scope_and_optional_frozen_index`
2. `flush_request_rejects_empty_table_identity_seed`
3. `flush_request_rejects_empty_table_object_id`
4. `flush_request_rejects_non_l0_target_for_initial_slice`
5. `flush_outcome_reports_deferred_no_frozen_state`
6. `flush_outcome_reports_table_identity_object_and_install_facts`
7. `flush_outcome_converts_to_maintenance_completed_status`
8. `flush_outcome_converts_partial_publication_to_failed_status`
9. `flush_outcome_debug_uses_storage_vocabulary`

Assertions:

1. request facts are branch/table/storage facts only;
2. invalid requests fail before lower-layer calls;
3. no outcome uses product or public command wording.

### 2. Frozen Candidate Selection

Required tests:

1. `flush_deferred_when_branch_has_no_frozen_tables`
2. `flush_does_not_implicitly_rotate_active_rows`
3. `flush_named_frozen_index_must_exist`
4. `flush_default_selects_oldest_frozen_table`
5. `flush_named_index_replaces_that_table_only`
6. `flush_repeated_after_success_reports_no_matching_frozen_state`
7. `flush_keeps_other_frozen_tables_in_order`

Assertions:

1. selection is deterministic;
2. active rows alone are not flushed;
3. L6 frozen ordering assumptions are pinned.

### 3. Cache-Mode Flush

Required tests:

1. `cache_flush_builds_table_from_frozen_rows`
2. `cache_flush_replaces_frozen_with_l0_table`
3. `cache_flush_preserves_read_results_before_and_after`
4. `cache_flush_preserves_tombstones`
5. `cache_flush_preserves_commit_timestamp_facts`
6. `cache_flush_reports_no_durable_object`
7. `cache_flush_does_not_touch_manifest_wal_snapshot_or_quarantine_services`
8. `cache_flush_install_failure_leaves_frozen_state_unchanged`
9. `cache_flush_builder_failure_leaves_frozen_state_unchanged`

Assertions:

1. cache flush is an in-memory representation change;
2. cache flush never claims durable publication;
3. branch reads are equivalent across the flush boundary.

### 4. Durable Table Publication

Required tests:

1. `durable_flush_builds_l5_table_before_publish`
2. `durable_flush_publishes_table_object_before_branch_install`
3. `durable_flush_reopens_published_table_before_branch_install`
4. `durable_flush_installs_l6_table_after_reopen_validation`
5. `durable_flush_reports_l4_publish_outcome`
6. `durable_flush_reports_l4_table_object_facts`
7. `durable_flush_reports_l6_install_outcome`
8. `durable_flush_preserves_read_results_before_and_after`
9. `durable_flush_object_bytes_match_built_table_bytes`
10. `durable_flush_does_not_update_database_manifest_flush_watermark`
11. `durable_flush_does_not_truncate_wal`
12. `durable_flush_does_not_publish_checkpoint`

Assertions:

1. durable object creation precedes L6 replacement;
2. L4 and L5 facts agree before install;
3. global checkpoint/retention state is untouched.

### 5. Lower-Layer Error Mapping

Required tests:

1. `flush_builder_error_preserves_table_runtime_source`
2. `flush_publish_error_preserves_table_object_service_source`
3. `flush_invalid_publish_metadata_is_typed`
4. `flush_reopen_error_preserves_table_object_read_source`
5. `flush_branch_owned_table_error_preserves_branch_source`
6. `flush_l6_install_error_preserves_branch_source`
7. `flush_error_codes_are_stable`
8. `flush_errors_do_not_assert_display_strings`

Assertions:

1. every lower-layer failure has a lifecycle error code;
2. lower-layer source chains remain available;
3. no failure path panics.

### 6. Publication Failure Windows

Required tests:

1. `publish_failure_before_create_leaves_frozen_state_unchanged`
2. `publish_success_then_reopen_failure_leaves_frozen_state_unchanged`
3. `publish_success_then_branch_table_construction_failure_leaves_frozen_state`
4. `publish_success_then_install_failure_leaves_frozen_state_unchanged`
5. `publish_success_then_install_failure_reports_orphaned_object_fact`
6. `matching_existing_object_can_resume_install`
7. `conflicting_existing_object_fails_closed`
8. `matching_l0_table_after_retry_is_idempotent`
9. `retry_with_changed_frozen_rows_fails_closed`

Assertions:

1. L6 frozen state is removed only after install succeeds;
2. orphaned table objects are reported, not deleted;
3. retries distinguish exact match from collision.

### 7. Maintenance Integration

Required tests:

1. `flush_task_runs_through_maintenance_executor`
2. `duplicate_flush_task_coalesces_by_branch`
3. `flush_task_no_frozen_state_returns_deferred`
4. `flush_task_success_updates_maintenance_stats`
5. `flush_task_failure_adds_health_debt`
6. `flush_task_canceled_before_start_does_not_build_or_publish`
7. `flush_task_rejected_after_close_requested`
8. `flush_task_status_reports_pending_active_and_completed_counts`

Assertions:

1. flush never bypasses lifecycle admission;
2. coalescing remains the L8H policy;
3. maintenance outcome facts carry flush-specific details.

### 8. Watermark And WAL Absence

Required tests:

1. `successful_flush_reports_candidate_commit_max`
2. `successful_flush_does_not_persist_flush_watermark`
3. `branch_absence_does_not_advance_flush_watermark`
4. `flush_watermark_service_is_not_called`
5. `wal_retention_proof_is_not_constructed`
6. `wal_delete_or_truncate_is_not_called`
7. `active_wal_segment_fact_is_unchanged`

Assertions:

1. L8I does not implement L8J behavior early;
2. manifest state remains unchanged except table-object publication effects
   explicitly owned by this slice.

### 9. Identity And Collision Handling

Required tests:

1. `flush_table_identity_is_deterministic_for_same_frozen_rows`
2. `flush_table_identity_changes_for_different_commit_range`
3. `flush_table_identity_changes_for_different_branch`
4. `flush_table_identity_collision_with_reachable_table_rejects_before_install`
5. `flush_object_id_collision_with_matching_bytes_is_retryable`
6. `flush_object_id_collision_with_different_bytes_rejects`
7. `flush_identity_does_not_use_reserved_layout_literals_in_source`

Assertions:

1. table identity and object id are stable enough for retry;
2. collisions cannot silently alias different rows.

## Integration Tests

Extend `crates/storage-next/tests/lifecycle_maintenance.rs`.

Required tests:

1. cache runtime commit/rotate/flush/read round trip;
2. durable runtime commit/rotate/flush/read round trip in memory backend;
3. durable flush publishes exactly one table object for one frozen table;
4. duplicate flush task coalesces in runtime integration;
5. no-frozen flush is a clean deferred outcome;
6. publication failure returns typed failed outcome and leaves rows readable;
7. install failure returns typed failed outcome and leaves rows readable;
8. manifest flush watermark is unchanged after durable flush;
9. WAL object set is unchanged after durable flush.

Local filesystem integration is optional in this slice. If added, it should be
feature/platform gated and should prove only table-object durability and reopen
readability. Full crash-window coverage belongs to the later assurance slice.

## Generated Properties

Add or extend lifecycle testkit scripts for flush.

Script operations:

1. append committed row;
2. rotate active to frozen;
3. enqueue flush;
4. run flush;
5. inject build/publish/reopen/install fault;
6. retry flush;
7. read latest value before/after flush;
8. query manifest/WAL unchanged facts;
9. close and ensure pending flush is canceled or rejected by policy.

Model facts:

1. active row count;
2. frozen table count;
3. owned L0 table count;
4. published table object count;
5. visible latest rows by key;
6. candidate flushed max version;
7. manifest flush watermark unchanged flag;
8. failed/orphaned object facts.

Required generated assertions:

1. production latest reads equal model latest reads;
2. production frozen and owned table counts equal model counts;
3. published object count increases only after durable publish succeeds;
4. frozen count decreases only after L6 install succeeds;
5. no generated script advances manifest flush watermark;
6. no generated script calls WAL truncation;
7. generated input, not only canonical setup, reaches success, no-op, publish
   failure, install failure, and retry categories.

## Source Guards

Extend `lifecycle_source_guard.rs`.

Required checks:

1. cache lifecycle flush path does not import durable services;
2. flush source does not import engine, product, StrataHub, follower, raw
   filesystem, path, environment, sleep, thread, or async runtime APIs;
3. flush source does not contain architecture slice labels or milestone words;
4. flush tests do not contain sleeps or thread spawns;
5. lower layers do not import `crate::lifecycle`;
6. flush tests do not hardcode reserved object-layout path literals;
7. lifecycle source does not call WAL truncation or manifest flush watermark
   persistence from the flush module.

## Sensitivity Probes

Record probes in
`docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` after
implementation.

Minimum probes:

| Probe | Mutation | Expected failure |
|---|---|---|
| I1 | Remove L4 publish before L6 install. | Durable publication ordering test fails. |
| I2 | Remove reopen validation before install. | Reopen-before-install test fails. |
| I3 | Remove frozen rows before publish. | Publication failure state test fails. |
| I4 | Treat no frozen state as completed flush. | Deferred no-op test fails. |
| I5 | Select newest instead of oldest default frozen table. | Candidate selection test fails. |
| I6 | Drop tombstone rows while building table. | Read parity/tombstone test fails. |
| I7 | Advance manifest flush watermark after table install. | Watermark absence test fails. |
| I8 | Call WAL truncation after flush. | WAL absence/source guard test fails. |
| I9 | Delete orphaned table object after install failure. | Orphaned-object fact test fails. |
| I10 | Let cache mode call table-object service. | Cache source guard/integration test fails. |
| I11 | Collapse publish error into display string only. | Source-chain test fails. |
| I12 | Add architecture label to flush code. | Source guard fails. |

## Deferred Coverage

These are not L8I obligations:

1. checkpoint publication;
2. database manifest snapshot watermark update;
3. database manifest flush watermark persistence;
4. WAL retention proof and truncation;
5. compaction and materialization scheduling;
6. retention proof for orphaned published table objects;
7. quarantine/purge/repair;
8. full close drain with durable sync;
9. localfs crash/reopen matrix for every failure window;
10. fuzz target inventory.

Each deferred item must be covered by the later slice that implements the
behavior.

## Verification Commands

After implementation:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::flush
cargo test -p strata-storage-next --locked --lib lifecycle::tests::maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --all-features --locked --lib lifecycle
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

When `cargo-hack` is available:

```bash
cargo hack check -p strata-storage-next --feature-powerset --depth 2
```

## Exit Criteria

L8I tests are complete when:

1. cache and durable flush paths both have direct tests;
2. durable flush is proven to publish/reopen before L6 install;
3. all publication and install failure windows preserve frozen state;
4. read parity before and after flush is tested for puts and tombstones;
5. no test or source path advances flush watermark or truncates WAL;
6. retry behavior distinguishes matching object, conflicting object, and
   already-installed table cases;
7. generated scripts use an independent model for counts and latest reads;
8. source guards prevent cache durable-service drift and architecture-label
   leakage;
9. the L8 porting log records shipped files, verification commands, and
   sensitivity probes.
