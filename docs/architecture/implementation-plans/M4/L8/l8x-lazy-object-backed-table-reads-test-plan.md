# L8X Test Plan: Lazy Object-Backed Table Reads

Status: split test plan

Implementation note: the shipped storage-next reader currently performs
bounded range-backed materialized open. It avoids a single full-object read and
preserves source-chain diagnostics, but branch state still requires a row-slice
reader and therefore loads all data blocks before installation. The point,
range, cache, touched-block corruption, and block-budget tests below remain the
required follow-up for full lazy cursor support.

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8x-lazy-object-backed-table-reads-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that durable table objects can be opened and queried without loading the
whole object into memory, while preserving eager-reader semantics and typed
corruption/source errors.

The suite must fail if L8X:

1. reads all table bytes during durable table open;
2. reads unrelated data blocks for point lookups;
3. reads blocks outside prefix/range bounds;
4. bypasses table block cache;
5. ignores L8W reader/block budgets;
6. fails open on corrupt index/properties metadata;
7. fails open on an untouched corrupt data block instead of failing on query;
8. changes row ordering, tombstone, TTL, timestamp, or value bytes;
9. uses path-derived cache identity or process-global cache state;
10. imports raw IO, product query policy, or milestone labels.

Do not add tests whose only assertion is that planning documents exist or link
to other planning documents.

## Coverage Boundary

Covered by the current implementation:

1. metadata-only open;
2. bounded range-backed data-block reads during materialized open;
3. eager/source parity after materialized open;
4. reader budget integration using materialized object size;
5. metadata/data-block corruption and backend range-read failures during open;
6. recovery/open integration;
7. no-default/wasm-compatible memory backend smoke;
8. generated/model and source-guard coverage.

Follow-up coverage required for full lazy cursor support:

1. point lookup laziness;
2. prefix/range cursor laziness;
3. cache hit/miss/eviction integration in the reader path;
4. touched-block budget integration;
5. untouched corrupt block succeeds at open and fails when queried.

Not covered:

1. new table byte format;
2. bloom/filter sidecars;
3. remote object-store read-ahead tuning;
4. public query/index API;
5. row pruning semantics;
6. branch lifecycle completion.

## Old-Code Regression Sources

| Old source | Behavior to preserve | Storage-next assertion |
|---|---|---|
| `storage/src/segment.rs::KVSegment` | Open loads metadata, not every data block. | Lazy open range reads header/footer/index/properties only. |
| `FlatIndex::search` | Index search selects candidate block for a key. | Point lookup reads only candidate block and returns eager-reader parity. |
| Data-block `pread` path | Data blocks are read on demand. | Untouched data blocks are not read or decoded. |
| Block cache integration | Repeated reads use cache. | Second lookup of same block records cache hit and no backend read. |
| Prefix/range cursor | Cursor reads blocks sequentially within bounds. | Range cursor reads exact block set needed by bounds. |
| Path-hash cache keys | Old cache could key from local path. | Cache key uses table identity and block address, never path. |

Tests must not port:

1. raw file descriptors or `pread`;
2. mmap behavior;
3. process-global block cache;
4. host-memory auto detection;
5. product query behavior.

## Test Locations

Use:

1. `crates/storage-next/src/table/tests/reader.rs` for lazy/eager reader parity
   and cursor tests.
2. `crates/storage-next/src/table/tests/cache.rs` for block cache tests.
3. `crates/storage-next/src/service/tests/table_reader.rs` or
   `crates/storage-next/src/service/table.rs` test module for object-backed
   range-read accounting.
4. `crates/storage-next/src/lifecycle/tests/recovery.rs` for recovery/open
   integration.
5. `crates/storage-next/src/testkit/table_runtime.rs` for generated table
   reader scripts.
6. `crates/storage-next/tests/lifecycle_maintenance.rs` for large-table
   integration smoke.
7. `crates/storage-next/tests/lifecycle_source_guard.rs` for source guards.
8. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for
   verification and sensitivity-probe records after implementation.

Split direct tests into submodules once any file approaches 1,000 lines.

## Test Data Principles

1. Build tables with at least four data blocks.
2. Include small values, empty values, and large values.
3. Include duplicate physical keys with multiple commit versions.
4. Include tombstones and TTL-expired rows.
5. Include compressed and uncompressed data blocks.
6. Include keys before first block, inside middle block, and after last block.
7. Record backend range-read calls by offset and length.
8. Use explicit table identities to verify cache-key isolation.
9. Avoid host-dependent timing or memory assertions.

## Direct Unit Tests

### 1. Metadata-Only Open

Required tests:

1. `lazy_table_open_reads_header_footer_index_and_properties_only`
2. `lazy_table_open_does_not_read_data_blocks`
3. `lazy_table_open_validates_object_metadata_before_range_reads`
4. `lazy_table_open_rejects_missing_range_read_capability`
5. `lazy_table_open_rejects_corrupt_header`
6. `lazy_table_open_rejects_corrupt_footer`
7. `lazy_table_open_rejects_corrupt_index`
8. `lazy_table_open_rejects_corrupt_properties`
9. `lazy_table_open_rejects_index_data_block_count_mismatch`
10. `lazy_table_open_reports_reader_budget_usage`

Assertions:

1. no full-object range read occurs;
2. metadata failures happen at open;
3. source chains identify object and range-read phase.

### 2. Point Lookup Laziness

Required tests:

1. `lazy_point_lookup_reads_only_candidate_block`
2. `lazy_point_lookup_missing_before_first_block_reads_no_data_block`
3. `lazy_point_lookup_missing_between_blocks_reads_at_most_one_block`
4. `lazy_point_lookup_missing_after_last_block_reads_no_data_block`
5. `lazy_point_lookup_returns_newer_and_older_versions_by_exact_internal_key`
6. `lazy_point_lookup_preserves_tombstone_rows`
7. `lazy_point_lookup_preserves_ttl_metadata`
8. `lazy_point_lookup_large_value_does_not_load_other_blocks`
9. `lazy_point_lookup_matches_eager_reader`
10. `lazy_point_lookup_releases_current_block_reservation`

Assertions:

1. range reads are limited to candidate block ranges;
2. results match eager reader exactly;
3. block budget releases after lookup/cursor drop.

### 3. Prefix And Range Cursor Laziness

Required tests:

1. `lazy_prefix_cursor_starts_at_index_selected_block`
2. `lazy_prefix_cursor_stops_before_nonmatching_block`
3. `lazy_range_cursor_reads_blocks_within_bounds_only`
4. `lazy_range_cursor_crosses_blocks_in_order`
5. `lazy_range_cursor_empty_range_reads_no_data_blocks`
6. `lazy_range_cursor_seek_middle_of_block_skips_prior_rows`
7. `lazy_range_cursor_advance_after_last_row_reads_no_extra_block`
8. `lazy_bounded_cursor_matches_eager_reader`
9. `lazy_cursor_preserves_tombstone_and_ttl_rows`
10. `lazy_cursor_releases_block_reservation_on_drop`

Assertions:

1. cursor order and boundaries match eager reader;
2. backend reads are proportional to touched blocks, not table size.

### 4. Cache Integration

Required tests:

1. `lazy_reader_cache_miss_reads_backend_once`
2. `lazy_reader_cache_hit_avoids_backend_range_read`
3. `lazy_reader_zero_cache_capacity_serves_uncached`
4. `lazy_reader_block_larger_than_cache_serves_uncached`
5. `lazy_reader_cache_key_includes_table_identity`
6. `lazy_reader_cache_key_includes_block_offset_or_ordinal`
7. `lazy_reader_cache_eviction_preserves_correctness`
8. `lazy_reader_cache_decode_failure_does_not_poison_other_blocks`
9. `lazy_reader_cache_stats_report_hits_misses_and_bytes`
10. `lazy_reader_two_runtime_caches_are_isolated`

Assertions:

1. cache behavior is observable through raw stats;
2. cache identity cannot collide across table objects;
3. zero cache remains correct.

### 5. Budget Integration

Required tests:

1. `lazy_open_uses_metadata_budget_not_whole_table_budget`
2. `lazy_reader_large_table_opens_under_low_memory_profile`
3. `lazy_point_lookup_rejects_block_over_reader_budget_before_read`
4. `lazy_block_cache_budget_pressure_serves_uncached_or_defers`
5. `lazy_cursor_holds_at_most_current_block_budget`
6. `lazy_reader_open_failure_releases_metadata_budget`
7. `lazy_data_block_decode_failure_releases_block_budget`
8. `lazy_recovery_large_table_does_not_use_whole_table_reservation`
9. `lazy_deep_validation_requires_explicit_scan_budget`
10. `lazy_reader_budget_stats_are_deterministic`

Assertions:

1. large tables open under small whole-reader budgets;
2. touched blocks are still budgeted;
3. every failure path releases reservations.

### 6. Corruption And Source Failures

Required tests:

1. `lazy_open_corrupt_metadata_fails_open`
2. `lazy_open_short_index_range_preserves_source_error`
3. `lazy_open_long_properties_range_preserves_source_error`
4. `lazy_query_corrupt_touched_data_block_fails_query`
5. `lazy_query_corrupt_untouched_data_block_does_not_fail_open`
6. `lazy_query_short_data_block_range_preserves_source_error`
7. `lazy_query_backend_failure_preserves_backend_source`
8. `lazy_query_wrong_block_kind_reports_table_decode_error`
9. `lazy_recovery_records_corrupt_table_health_when_query_validation_runs`
10. `lazy_repair_deep_validation_finds_corrupt_untouched_block`

Assertions:

1. metadata corruption blocks open;
2. data-block corruption is query/deep-validation scoped;
3. errors preserve object and lower-layer source.

### 7. Recovery And Lifecycle Integration

Required tests:

1. `recovery_opens_manifest_tables_lazily`
2. `recovery_with_large_manifest_table_does_not_read_full_object`
3. `recovery_latest_read_fetches_only_needed_block`
4. `recovery_range_read_fetches_only_needed_blocks`
5. `recovery_lazy_reader_preserves_branch_read_parity`
6. `flush_reopen_uses_lazy_reader_for_published_object`
7. `rewrite_reopen_uses_lazy_reader_for_published_object`
8. `cache_mode_can_use_eager_reader_without_durable_claim`
9. `durable_mode_uses_lazy_reader_by_default`
10. `lazy_recovery_no_default_features_memory_backend_smoke`

Assertions:

1. table manifests and branch state install lazy readers;
2. L6 read results are unchanged;
3. cache mode remains honest about durability.

### 8. Compatibility And Parity

Required tests:

1. `lazy_reader_matches_eager_for_uncompressed_tables`
2. `lazy_reader_matches_eager_for_zstd_tables`
3. `lazy_reader_matches_eager_for_single_block_tables`
4. `lazy_reader_matches_eager_for_many_block_tables`
5. `lazy_reader_matches_eager_for_empty_values`
6. `lazy_reader_matches_eager_for_large_values`
7. `lazy_reader_matches_eager_for_duplicate_physical_keys`
8. `lazy_reader_matches_eager_for_prefix_bounds`
9. `lazy_reader_matches_eager_for_range_bounds`
10. `lazy_reader_matches_eager_for_full_cursor_iteration`

Assertions:

1. lazy/eager parity covers row shapes and cursor movement;
2. compression does not change lazy behavior.

## Generated And Property Tests

Extend table/lifecycle generated scripts with:

```text
build_table(block_count, rows_per_block, compression)
open_lazy_reader
point_lookup(key)
seek_cursor(bound)
advance_cursor(n)
drop_reader
inject_range_read_fault(offset)
corrupt_block(index)
assert_eager_parity
assert_range_reads(max_blocks)
```

Required counters:

1. metadata_open;
2. data_block_read;
3. cache_hit;
4. cache_miss;
5. point_lookup;
6. bounded_cursor;
7. untouched_corrupt_block;
8. touched_corrupt_block;
9. budget_released;
10. eager_parity;

Properties:

1. lazy results match eager results for all generated valid tables;
2. range-read count is bounded by touched block count plus metadata reads;
3. untouched corrupt data blocks do not affect unrelated point reads;
4. touched corrupt data blocks fail with typed table/source error;
5. dropped readers/cursors release budget reservations;
6. cache hit/miss counters reflect repeated generated reads.

## Source Guards

Required source guard tests:

1. `lazy_reader_does_not_full_read_durable_object_on_open`
2. `lazy_reader_does_not_import_raw_io`
3. `lazy_reader_does_not_use_path_cache_identity`
4. `lazy_reader_does_not_use_process_global_cache`
5. `lazy_reader_does_not_import_product_query_policy`
6. `lazy_reader_does_not_import_backend_delete_or_quarantine`
7. `lazy_reader_does_not_import_stratahub`
8. `lazy_reader_does_not_import_primitive_modules`
9. `lazy_reader_code_and_fixture_names_do_not_use_milestone_labels`

Forbidden production tokens include:

1. `read_full`
2. `read_full_source`
3. `std::fs`
4. `std::path::Path`
5. `File::`
6. `OpenOptions`
7. `mmap`
8. `file_path_hash`
9. `OnceLock<`
10. `static GLOBAL`
11. `strata_engine`
12. `stratahub`
13. `primitive`
14. `delete_object`
15. `quarantine_object`

Scope scans to durable lazy-reader paths. Eager byte-reader test helpers may
still use full in-memory byte slices.

## Fault Windows

Required phase tests:

1. failure after metadata budget reservation before header read releases budget;
2. failure during footer read releases budget;
3. failure during index decode releases budget;
4. failure during data block read releases current block reservation;
5. failure during cache insert does not lose query result;
6. failure during cursor advance leaves cursor in typed failed state;
7. recovery failure from metadata corruption transitions lifecycle correctly;
8. repair/deep validation failure records corrupt block object facts.

Every phase must have a non-ignored unit or integration equivalent.

## Sensitivity Probes

Record probes in
`docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` after
implementation.

| Probe | Mutation | Expected failure |
|---|---|---|
| X1 | Call full object read during open. | Metadata-only open/source guard fails. |
| X2 | Ignore index and scan every block for point lookup. | Point laziness test fails. |
| X3 | Let range cursor read past upper bound. | Range block-count test fails. |
| X4 | Cache by path or object name only. | Cache identity test fails. |
| X5 | Skip block budget release on decode error. | Budget release test fails. |
| X6 | Treat corrupt untouched block as open failure. | Untouched corruption test fails. |
| X7 | Treat corrupt touched block as absence. | Corrupt query test fails. |
| X8 | Drop tombstone/TTL metadata during block decode. | Eager parity test fails. |
| X9 | Reintroduce process-global cache. | Source/isolation test fails. |
| X10 | Import raw file IO in reader. | Source guard fails. |

## Command Matrix

Mandatory commands before L8X closeout:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib table::tests::reader
cargo test -p strata-storage-next --locked --lib table::tests::cache
cargo test -p strata-storage-next --locked --lib service::table
cargo test -p strata-storage-next --locked --lib lifecycle::tests::recovery
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test table_runtime_properties
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

## Exit Gate

L8X test coverage is complete when:

1. durable object-backed open avoids data-block reads;
2. point/range/prefix queries read only necessary blocks;
3. lazy and eager readers are equivalent for valid tables;
4. cache hit/miss/eviction behavior is observed and budgeted;
5. corruption/source failures are classified by metadata vs touched data block;
6. recovery installs lazy readers and preserves branch read parity;
7. large-table low-memory smoke passes without whole-object loading;
8. no-default/wasm check passes;
9. source guards block full durable reads, raw IO, path cache keys, global cache,
   product imports, and milestone labels in Rust code;
10. sensitivity probes and command results are recorded in the porting log.
