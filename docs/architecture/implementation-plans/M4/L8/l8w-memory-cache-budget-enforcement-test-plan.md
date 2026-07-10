# L8W Test Plan: Memory And Cache Budget Enforcement

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8w-memory-cache-budget-enforcement-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that storage-next respects explicit memory budgets for cache, readers,
active/frozen branch state, maintenance queues, generated artifacts, and
manifest/catalog metadata.

The suite must fail if L8W:

1. auto-detects host memory;
2. creates hidden process-global cache state;
3. clamps a tiny cache profile up to a large default;
4. opens whole table readers beyond the reader budget;
5. builds generated table/checkpoint artifacts beyond budget;
6. lets active/frozen branch state grow without admission;
7. leaks budget reservations on failure;
8. lets maintenance queues grow unbounded;
9. reports product policy instead of raw storage pressure facts;
10. imports raw IO, product resource-policy crates, or milestone labels.

Do not add tests whose only assertion is that planning documents exist or link
to other planning documents.

## Coverage Boundary

Covered:

1. budget config validation;
2. database-local budget ledger and reservation release;
3. block/table cache capacity and stats;
4. table reader admission;
5. active/frozen branch state budget;
6. generated artifact budget;
7. maintenance queue/task budget;
8. manifest/catalog metadata budget;
9. low-memory profile smoke;
10. generated/model and source-guard coverage.

Not covered:

1. lazy object-backed reads, covered by L8X;
2. public resource-profile configuration, covered by L9;
3. product write-stall wording or policy;
4. object-store provider-local shared caches;
5. benchmark performance targets.

## Old-Code Regression Sources

| Old source | Behavior to preserve | Storage-next assertion |
|---|---|---|
| `storage/src/block_cache.rs::BlockCache::new` | Explicit byte capacity controls cache storage. | Cache capacity comes from runtime budget and is database-local. |
| `zero_capacity_cache_does_not_store` | Zero capacity disables cache storage without breaking reads. | Zero block cache budget stores no entries and still serves uncached reads. |
| `BlockCacheStats` | Stats include hits, misses, entries, size, capacity, and pinned bytes. | Lifecycle budget facts expose equivalent raw storage stats. |
| CLOCK eviction tests | Cache evicts or skips caching under pressure without unbounded scans. | Eviction effort is bounded and overlarge blocks are served uncached. |
| pinned priority tests | Pinned bytes are tracked and protected. | Pinned reader/cache bytes count against budget and cannot be silently evicted. |
| `test_issue_1735_no_minimum_cache_clamp` | Tiny available memory does not get clamped to a large hidden minimum. | Low-memory profile keeps small/zero configured cache values exactly. |
| `set_global_capacity` / `global_cache` | Process-global cache is mutable shared state. | Storage-next does not create hidden global cache state. |

Tests must not port:

1. `/proc/meminfo` parsing;
2. host-memory auto detection;
3. local path cache identity;
4. product resource-profile selection;
5. public write-stall UX.

## Test Locations

Use:

1. `crates/storage-next/src/lifecycle/tests/budget.rs` for budget config,
   ledger, pressure, and outcome tests.
2. `crates/storage-next/src/table/tests/cache.rs` for cache capacity/eviction
   tests.
3. `crates/storage-next/src/table/tests/reader.rs` for reader admission tests.
4. `crates/storage-next/src/branch/tests/budget.rs` for active/frozen branch
   state accounting.
5. `crates/storage-next/src/lifecycle/tests/maintenance.rs` for queue/task
   budget tests.
6. `crates/storage-next/src/testkit/lifecycle/budget.rs` for generated budget
   scripts.
7. `crates/storage-next/tests/lifecycle_maintenance.rs` for low-memory
   integration smoke.
8. `crates/storage-next/tests/lifecycle_source_guard.rs` for source guards.
9. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for
   verification and sensitivity-probe records after implementation.

Split direct tests into submodules once any file approaches 1,000 lines.

## Test Data Principles

1. Use explicit numeric budgets in every test.
2. Include zero, tiny, exact-fit, one-byte-over, and large budgets.
3. Assert pool, requested bytes/count, used bytes/count, and limit.
4. Assert release on success and every failure path.
5. Test cache mode and durable local mode.
6. Test two runtime instances to catch hidden global state.
7. Avoid host-dependent memory or timing assertions.

## Direct Unit Tests

### 1. Budget Config Validation

Required tests:

1. `storage_budget_accepts_explicit_low_memory_profile`
2. `storage_budget_accepts_zero_optional_block_cache`
3. `storage_budget_rejects_zero_mandatory_active_pool`
4. `storage_budget_rejects_total_smaller_than_required_pools`
5. `storage_budget_rejects_overflowing_pool_sum`
6. `storage_budget_rejects_reader_count_zero_when_readers_required`
7. `storage_budget_rejects_frozen_table_count_zero_when_flush_enabled`
8. `storage_budget_reports_all_pool_limits`
9. `storage_budget_profile_does_not_probe_host_memory`
10. `low_memory_profile_does_not_apply_hidden_minimum_cache`

Assertions:

1. config errors use stable codes;
2. optional zero means disabled;
3. no host-dependent default appears in validated config.

### 2. Budget Ledger And Reservations

Required tests:

1. `budget_reservation_acquire_and_release`
2. `budget_reservation_rejects_one_byte_over_limit`
3. `budget_reservation_exact_fit_succeeds`
4. `budget_reservation_drop_releases_usage`
5. `budget_reservation_failed_acquire_does_not_change_usage`
6. `budget_reservation_nested_failure_releases_outer`
7. `budget_reservation_overflow_rejects`
8. `budget_stats_are_deterministic`
9. `budget_pressure_reports_pool_usage_and_limit`
10. `budget_ledger_is_database_local`

Assertions:

1. every acquire/release changes only the targeted pool;
2. failed operations leave usage unchanged;
3. two runtimes do not share hidden usage.

### 3. Block/Table Cache

Required tests:

1. `zero_capacity_table_cache_does_not_store`
2. `small_cache_serves_oversized_block_uncached`
3. `table_cache_respects_capacity_after_insert`
4. `table_cache_eviction_effort_is_bounded`
5. `table_cache_pinned_bytes_reported`
6. `table_cache_pinned_entries_not_silently_evicted`
7. `table_cache_shrink_records_pressure`
8. `table_cache_stats_include_hits_misses_entries_bytes`
9. `table_cache_keys_use_table_identity_not_path`
10. `two_runtime_caches_are_isolated`

Assertions:

1. no process-global cache affects another runtime;
2. capacity/usage/pinned stats match operations;
3. disabled cache still allows reads.

### 4. Table Reader Admission

Required tests:

1. `reader_open_exact_budget_succeeds`
2. `reader_open_over_budget_rejects_before_decode`
3. `reader_open_failure_releases_reservation`
4. `reader_drop_releases_reservation`
5. `reader_budget_counts_concurrent_readers`
6. `reader_count_limit_rejects_extra_reader`
7. `reader_budget_error_names_table_identity`
8. `reader_budget_recovery_decode_rejects_large_table`
9. `reader_budget_cache_mode_and_durable_mode_match`
10. `reader_budget_waits_for_l8x_before_large_lazy_reads`

Assertions:

1. admission happens before whole-object decode/allocation;
2. live readers hold reservations;
3. large durable tables fail closed until lazy reads exist.

### 5. Active And Frozen Branch State

Required tests:

1. `active_append_under_budget_succeeds`
2. `active_append_over_budget_rejects_before_mutation`
3. `active_append_failure_does_not_advance_commit_visibility`
4. `active_budget_reports_approximate_bytes`
5. `rotate_active_under_frozen_budget_succeeds`
6. `rotate_active_over_frozen_byte_budget_rejects`
7. `rotate_active_over_frozen_count_budget_rejects`
8. `flush_releases_frozen_budget_after_install`
9. `flush_failure_keeps_frozen_budget_reserved`
10. `cache_and_durable_active_budget_behavior_match`

Assertions:

1. mutation admission precedes L6 state changes;
2. frozen reservations are released only when state actually changes;
3. visibility does not advance on budget rejection.

### 6. Generated Artifacts

Required tests:

1. `flush_artifact_exact_budget_succeeds`
2. `flush_artifact_over_budget_rejects_before_publish`
3. `compaction_artifact_over_budget_defers_before_publish`
4. `materialization_artifact_over_budget_defers_before_publish`
5. `checkpoint_encode_over_budget_rejects_before_snapshot_publish`
6. `recovery_decode_over_budget_fails_closed`
7. `partial_artifact_failure_releases_budget`
8. `artifact_actual_size_reconciles_with_estimate`
9. `artifact_budget_reports_output_bytes`
10. `artifact_budget_does_not_truncate_wal_or_delete_objects`

Assertions:

1. generated bytes are accounted before durable publication/install;
2. over-budget outputs do not leave visible state changes;
3. budget failure does not perform unrelated cleanup.

### 7. Maintenance Queue And Active Tasks

Required tests:

1. `maintenance_queue_count_limit_rejects_extra_task`
2. `maintenance_queue_byte_limit_rejects_large_task`
3. `maintenance_coalescing_happens_before_reservation`
4. `maintenance_cancel_releases_reservation`
5. `maintenance_active_task_holds_reservation`
6. `maintenance_task_failure_releases_reservation`
7. `maintenance_close_drain_releases_reservations`
8. `maintenance_optional_task_deferred_under_pressure`
9. `maintenance_mandatory_close_task_admitted_under_optional_pressure`
10. `maintenance_budget_pressure_added_to_outcome`

Assertions:

1. queue cannot grow unbounded;
2. coalesced duplicates do not double-count;
3. close drains/cancels without leaking budget.

### 8. Manifest And Catalog Metadata

Required tests:

1. `manifest_decode_rejects_large_section_count_before_allocation`
2. `table_manifest_catalog_over_budget_rejects`
3. `quarantine_inventory_over_budget_rejects_before_vector_allocation`
4. `retention_graph_over_budget_defers_optional_reclaim`
5. `recovery_mandatory_metadata_budget_failure_is_typed`
6. `metadata_pressure_blocks_optional_maintenance_first`
7. `metadata_budget_stats_report_catalog_bytes`
8. `corrupt_metadata_does_not_allocate_unbounded_memory`

Assertions:

1. metadata count/byte limits are checked before large allocation;
2. optional maintenance defers under pressure;
3. recovery failures preserve typed source chains.

### 9. Low-Memory Profile Smoke

Required tests:

1. `low_memory_profile_opens_cache_runtime`
2. `low_memory_profile_opens_durable_runtime_on_memory_backend`
3. `low_memory_profile_allows_small_commit_read_flush_checkpoint_close`
4. `low_memory_profile_defers_large_compaction_artifact`
5. `low_memory_profile_rejects_large_whole_table_reader_until_lazy_reads`
6. `low_memory_profile_zero_cache_still_reads_uncached`
7. `low_memory_profile_reports_pressure_without_product_policy`
8. `low_memory_profile_does_not_auto_detect_host_memory`

Assertions:

1. small ordinary workflows succeed;
2. large optional work defers or rejects with raw pressure facts;
3. no budget is inflated by environment-dependent defaults.

## Generated And Property Tests

Extend lifecycle generated scripts with:

```text
set_budget(profile)
open_cache/open_durable
commit(bytes)
rotate_active
flush
open_reader(bytes)
enqueue_task(kind, bytes)
run_task
inject_failure(phase)
drop_reader
close
assert_usage(pool)
```

Required counters:

1. budget_accept;
2. budget_reject;
3. reservation_release_on_success;
4. reservation_release_on_failure;
5. cache_eviction;
6. reader_reject;
7. active_reject;
8. artifact_defer;
9. maintenance_queue_reject;
10. low_memory_smoke;

Properties:

1. usage never exceeds configured hard limits after any successful operation;
2. rejected operations leave usage unchanged;
3. dropping readers/tasks/releases returns usage to baseline;
4. optional work defers before mandatory recovery/close work is blocked;
5. two runtime instances remain budget-isolated;
6. no generated route depends on host memory or wall-clock timing.

## Source Guards

Required source guard tests:

1. `memory_budget_does_not_probe_host_memory`
2. `memory_budget_does_not_use_process_global_cache`
3. `memory_budget_does_not_import_product_resource_policy`
4. `memory_budget_does_not_import_raw_io`
5. `memory_budget_does_not_import_backend_delete_or_quarantine`
6. `memory_budget_does_not_import_stratahub`
7. `memory_budget_does_not_import_primitive_modules`
8. `memory_budget_code_and_fixture_names_do_not_use_milestone_labels`

Forbidden production tokens include:

1. `/proc/meminfo`
2. `MemAvailable`
3. `sysinfo`
4. `System`
5. `OnceLock<`
6. `static GLOBAL`
7. `std::fs`
8. `std::env`
9. `available_memory`
10. `strata_engine`
11. `stratahub`
12. `primitive`
13. `delete_object`
14. `quarantine_object`

Scope scans to production budget/cache modules to avoid false positives from
old-code references in docs and tests.

## Fault Windows

Required phase tests:

1. failure after active reservation before L6 mutation releases reservation;
2. failure after generated artifact reservation before publish releases
   reservation;
3. failure after table publish before install releases artifact reservation but
   preserves orphan facts;
4. failure during reader decode releases reader reservation;
5. failure during maintenance task execution releases active-task reservation;
6. close cancellation releases queued task reservations;
7. recovery decode budget failure leaves shell Failed with typed source;
8. budget stats remain consistent after panic-free fault paths.

Every phase must have a non-ignored unit or integration equivalent.

## Sensitivity Probes

Record probes in
`docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` after
implementation.

| Probe | Mutation | Expected failure |
|---|---|---|
| W1 | Clamp zero cache to default capacity. | Zero-cache test fails. |
| W2 | Use process-global cache. | Runtime isolation test/source guard fails. |
| W3 | Open reader before reserving bytes. | Over-budget-before-decode test fails. |
| W4 | Leak reservation on publish failure. | Fault release test fails. |
| W5 | Ignore frozen byte budget on rotate. | Frozen budget test fails. |
| W6 | Allocate duplicate queue reservation before coalescing. | Coalescing test fails. |
| W7 | Decode manifest count before budget check. | Metadata allocation test fails. |
| W8 | Report product write-stall wording. | Source/vocabulary guard fails. |
| W9 | Probe host memory. | Source guard fails. |
| W10 | Let generated usage exceed limit. | Property test fails. |

## Command Matrix

Mandatory commands before L8W closeout:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib table::tests::cache
cargo test -p strata-storage-next --locked --lib table::tests::reader
cargo test -p strata-storage-next --locked --lib branch::tests
cargo test -p strata-storage-next --locked --lib lifecycle::tests::budget
cargo test -p strata-storage-next --locked --lib lifecycle::tests::maintenance
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

## Exit Gate

L8W test coverage is complete when:

1. every memory pool has direct budget validation tests;
2. cache capacity, zero-cache behavior, eviction, and pinned usage are covered;
3. table reader admission rejects large whole-object reads before decode;
4. active/frozen branch state cannot exceed configured budgets;
5. generated artifacts and metadata decode reject or defer before unbounded
   allocation;
6. maintenance queue and active-task reservations are bounded and released;
7. low-memory profile smoke passes without hidden default inflation;
8. generated properties prove usage never exceeds limits after successful
   operations;
9. source guards block host-memory probing, hidden globals, product policy,
   raw IO, deletion, and milestone labels in Rust code;
10. sensitivity probes and command results are recorded in the porting log.
