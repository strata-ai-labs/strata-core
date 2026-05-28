# L9C Test Plan: Reads And Timeline Resolution

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L9/l9c-reads-timeline-resolution-implementation-plan.md`

## Goal

Prove that L9 read APIs match L6/L7 visibility and timeline semantics while
preserving storage boundary errors.

## Test Locations

1. `crates/storage-next/src/api/tests/read.rs`
2. `crates/storage-next/tests/api_conformance.rs`
3. `crates/storage-next/tests/api_properties.rs`
4. `crates/storage-next/src/testkit/api/model.rs`

## Required Tests

### Point Reads

1. `read_latest_returns_newest_visible_value`
2. `read_latest_returns_none_for_absent_key`
3. `read_latest_returns_tombstone_fact_for_visible_delete`
4. `read_at_version_returns_exact_retained_value`
5. `read_at_version_uses_latest_at_or_before_version`
6. `read_at_version_rejects_unretained_history`
7. `read_at_timestamp_resolves_to_commit_version`
8. `read_at_timestamp_rejects_insufficient_history`
9. `read_after_close_rejects_closed_runtime`
10. `read_unknown_branch_rejects`

### History

1. `history_returns_newest_first`
2. `history_limit_is_enforced`
3. `history_before_version_excludes_newer_versions`
4. `history_preserves_tombstone_entries`
5. `history_pruned_versions_return_retention_error`
6. `history_empty_key_returns_empty_history`

### Prefix And Range Scans

1. `prefix_scan_returns_sorted_keys`
2. `prefix_scan_applies_version_bound`
3. `prefix_scan_applies_timestamp_bound`
4. `prefix_scan_limit_is_stable`
5. `range_scan_respects_start_and_end`
6. `range_scan_empty_range_returns_empty`
7. `range_scan_tombstone_visibility_matches_point_read`
8. `scan_inherited_rows_match_point_reads`

### Timeline

1. `timestamp_lookup_returns_newest_commit_at_or_before_timestamp`
2. `timestamp_lookup_equal_timestamps_uses_greatest_version`
3. `timestamp_lookup_before_retained_range_rejects`
4. `version_lookup_returns_commit_timestamp`
5. `version_lookup_unretained_version_rejects`
6. `timeline_bounds_report_retained_range`
7. `timeline_corruption_maps_to_diagnostic_error`

## Generated Read Contract

Generate scripts with:

1. put;
2. delete;
3. commit timestamp;
4. point read latest/version/timestamp;
5. history read;
6. prefix scan;
7. range scan;
8. branch fork and inherited reads once L9E lands.

The model must independently compute visibility and retained-history misses.

## Sensitivity Probes

1. Convert timestamp-history miss to not-found.
2. Reverse scan ordering.
3. Ignore tombstones in scans.
4. Use smallest version for duplicate timestamps.
5. Drop inherited rows from scans.

## Verification

```bash
cargo test -p strata-storage-next --locked --lib api
cargo test -p strata-storage-next --features testkit --locked --test api_properties
```
