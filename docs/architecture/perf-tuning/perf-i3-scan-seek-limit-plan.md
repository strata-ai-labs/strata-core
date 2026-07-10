# PERF-I3 Scan Seek And Limit Pushdown Plan

## Scope

PERF-I3 restores storage-next scan mechanics to the old storage shape:
bounded, ordered cursor scans with limit pushdown. This is a targeted serving
path fix for L9 `scan_prefix` and `scan_range`; it is not an index project and
not a branch/table rearchitecture.

## Goal

Make storage-next scans proportional to the returned key window and source
count, not proportional to the total rows in the branch.

Required behavior:

1. build scan cursors from existing ordered table keys;
2. seek each source to the requested lower bound or prefix start;
3. merge sources in internal-key order;
4. apply MVCC, tombstone, TTL, inherited-layer fork-version, and timestamp
   semantics while streaming;
5. stop as soon as the public API limit has produced enough visible rows;
6. avoid full `BranchReadView` row cloning for L9 scan reads once the borrowed
   scan path is proven equivalent.

## Evidence

Latest 100K-key cache scan comparison, 100 samples, limit 64, 150-byte values:

| Workload | Old cache | Storage-next cache |
| --- | ---: | ---: |
| scan-prefix | p50 42.83 us, 22,831 ops/s | p50 29.80 ms, 34 ops/s |
| scan-range-throughput | 23,675 ops/s | 20 ops/s |

Measured old-cache scan counters:

| Counter | Prefix | Range |
| --- | ---: | ---: |
| scan calls | 100 | 100 |
| iterator seeks | 100 | 100 |
| iterator pipeline builds | 100 | 100 |
| rows yielded | 6,400 | 6,400 |

Measured storage-next scan counters:

| Counter | Prefix | Range |
| --- | ---: | ---: |
| read views | 100 | 100 |
| read-view rows cloned | 10,020,000 | 10,020,000 |
| read-view validation rows scanned | 10,020,000 | 10,020,000 |
| scan rows visited | 10,020,000 | 10,020,000 |
| scan candidates materialized | 2,435 | 4,339,280 |
| table seeks | 0 | 0 |

Conclusion:

1. Old storage performs one seek per scan and yields exactly `limit * samples`
   rows.
2. Storage-next captures a full read view for each scan and walks the whole
   branch before applying the L9 result limit.
3. The immediate bottleneck is late limit application plus full-source candidate
   collection, compounded by row-proportional read-view capture.

Hot storage-next files:

1. `crates/storage-next/src/api/runtime.rs`
2. `crates/storage-next/src/branch/read.rs`
3. `crates/storage-next/src/branch/state/read_hooks.rs`
4. `crates/storage-next/src/table/cursor.rs`
5. `crates/storage-next/src/table/mutable.rs`
6. `crates/storage-next/src/table/reader.rs`

Old mechanics to preserve as the reference:

1. `crates/engine/src/primitives/kv.rs::scan`
2. `crates/storage/src/segmented/mod.rs::StorageIterator`
3. `crates/storage/src/segmented/mod.rs::scan_range_from_snapshot`

## Non-Goals

1. Do not add a secondary scan index.
2. Do not special-case the benchmark key format.
3. Do not change L9 scan API semantics.
4. Do not change storage row, table, object, WAL, manifest, or durable format
   layout.
5. Do not remove tombstone, TTL, timestamp, inherited-layer, or version-bound
   behavior.
6. Do not rewrite compaction, materialization, or snapshot install code.
7. Do not route scans through old storage code.
8. Do not optimize history reads in this slice unless a small shared helper is
   unavoidable and covered by unchanged tests.

## Correctness Contract

The new scan path must match the current `BranchReadView` scan result for:

1. latest reads;
2. version-bound reads;
3. timestamp-bound reads after timeline resolution;
4. prefix scans;
5. bounded range scans with included lower and excluded upper user-key bounds;
6. tombstones that suppress older values;
7. visible tombstones when L9 mapping needs them for deletion semantics;
8. TTL expiration at the selected read timestamp only;
9. inherited layers with source-branch-to-child-branch row rewriting;
10. inherited fork-version visibility caps;
11. active, frozen, owned, and inherited source precedence;
12. duplicated physical keys across multiple sources and commit versions.

The observable public result ordering must remain user-key ascending for visible
rows after MVCC deduplication.

## Implementation Shape

PERF-I3 should reuse the existing table cursor surface instead of adding an
index:

1. `MutableTable::cursor` and `FrozenTable::cursor` already expose ordered
   memory cursors.
2. `ImmutableTableReader::cursor` already exposes ordered immutable-table
   cursors.
3. `BoundedTableCursor` and `MergeTableCursor` already implement bounded cursor
   and merge mechanics.
4. Existing point-read seek helpers prove the table key order is usable for
   bounded reads.

The missing production shape is a branch-level scan stream that:

1. converts `BranchScanBounds` into table-key bounds per source;
2. constructs a bounded cursor for every eligible source;
3. rewrites inherited source rows as they are pulled, not by scanning every
   inherited row first;
4. feeds the merged cursor into a small MVCC group reducer;
5. emits one visible-or-tombstone row per physical key;
6. stops immediately after the L9 limit is satisfied.

The first production implementation may return a `Vec<BranchHistoryRow>` for
API compatibility, but the vector must be built from a limit-aware stream. It
must not first materialize all candidates in the scan range.

## Work Steps

### PERF-I3A: Table Scan Bounds And Seek Counters

Files:

1. `crates/storage-next/src/table/key.rs`
2. `crates/storage-next/src/table/mutable.rs`
3. `crates/storage-next/src/table/reader.rs`
4. `crates/storage-next/src/table/cursor.rs`
5. `crates/storage-next/src/observability/perf_trace.rs`

Work:

1. Add helpers that build `TableKeyBounds` from a physical-key prefix or
   physical-key range without scanning table rows.
2. Ensure bounded cursors seek to the lower bound on `seek_to_first`, rather
   than starting at row zero and advancing until in bounds.
3. Add or reuse a scan seek counter so PERF-I3 can prove scans do table seeks.
4. Add row-yield counters for scan cursors if current counters cannot
   distinguish cursor rows yielded from full-source rows visited.
5. Keep helpers private to table/branch internals.

Exit gates:

1. Table cursor tests prove prefix, range, empty range, single-key range, and
   absent lower-bound seek behavior.
2. A bounded cursor over 100K rows with a narrow lower bound does not visit rows
   before the bound.
3. Existing table cursor, reader, compaction, and key tests pass.

### PERF-I3B: Branch Scan Cursor Source Builder

Files:

1. `crates/storage-next/src/branch/read.rs`
2. `crates/storage-next/src/branch/state/read_hooks.rs`

Work:

1. Add a borrowed branch scan helper on `BranchLocalState`, similar in boundary
   to the point-read borrowed helper.
2. Build bounded cursors for active, frozen, owned, and inherited tables.
3. Skip source tables whose physical key range cannot overlap the scan bounds.
4. For inherited layers, convert child scan bounds to source-branch scan bounds
   before cursor construction, then rewrite rows back to the child branch when
   rows are emitted.
5. Enforce inherited fork-version bounds while streaming, not after full
   materialization.
6. Record scan source count, table seek count, cursor rows yielded, and visible
   rows emitted.

Correctness fences:

1. Do not apply source precedence before commit-version ordering.
2. Do not drop tombstones before they suppress older values.
3. Do not apply TTL without the selected timestamp.
4. Do not return source-branch physical keys from inherited rows.
5. Do not let a non-readable inherited layer contribute rows.

Exit gates:

1. New branch-level tests compare borrowed scan output against
   `capture_read_view().scan_*_including_tombstones(...)`.
2. Tests cover active, frozen, owned, inherited, tombstone, TTL, version-bound,
   timestamp-bound, prefix, range, empty, and limit cases.
3. Scan candidate counters no longer scale with total branch rows for bounded
   scans in a targeted perf-trace test.

### PERF-I3C: Streaming MVCC Reducer With Limit Pushdown

Files:

1. `crates/storage-next/src/branch/read.rs`
2. `crates/storage-next/src/table/cursor.rs`

Work:

1. Add a small branch scan reducer that groups adjacent rows by physical key
   from the merged cursor.
2. For each physical-key group, select the same visible row or visible tombstone
   as the current `select_visible_row_or_tombstone` logic.
3. Emit at most one output row per physical key.
4. Stop when the caller's requested limit has been satisfied.
5. Continue past tombstone-only and TTL-expired groups when the public result
   excludes them, until enough visible rows are returned or the cursor is
   exhausted.

Correctness fences:

1. The reducer must preserve current candidate ordering semantics.
2. The reducer must be able to see all versions of the current physical key
   before deciding which row is visible.
3. The reducer must not need to see rows for later physical keys once the limit
   is satisfied.
4. Limit `0` must return without building expensive scan work.

Exit gates:

1. Existing scan tests pass without expectation changes.
2. New tests prove `limit` stops after visible rows, not raw candidate rows.
3. New tests prove tombstone groups do not consume public visible-row limit.
4. Perf counters for 100 limited scans over 100K rows show row work near
   returned rows plus duplicate/tombstone rows, not 10,020,000.

### PERF-I3D: L9 Scan Routing

Files:

1. `crates/storage-next/src/api/runtime.rs`
2. `crates/storage-next/src/lifecycle/cache.rs`
3. `crates/storage-next/src/lifecycle/durable/bootstrap.rs`

Work:

1. Add lifecycle helpers for borrowed `scan_prefix` and `scan_range`.
2. Route `StorageRuntime::scan_prefix` and `StorageRuntime::scan_range` through
   the borrowed branch scan path only for bounds proven by tests.
3. Keep `read_history` and any unproven scan modes on the existing read-view
   path until their equivalence is tested.
4. Resolve timestamp bounds once, using the same timeline behavior as the
   current path.
5. Pass the L9 limit into the branch scan call so limit pushdown happens before
   row materialization.
6. Preserve closed-runtime and branch-not-found errors.

Exit gates:

1. Existing L9 scan API tests pass without expectation changes.
2. Add API-level perf-trace tests proving `read_view_rows_cloned` is zero or not
   row-proportional for L9 scans using the new path.
3. Add API-level perf-trace tests proving scan rows visited are bounded by the
   scanned window and limit.

### PERF-I3E: Benchmark Rerun And Decision Report

Run the same comparison that identified the issue:

```sh
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 100k --engines cache --workloads load-seq,scan-prefix,scan-range-throughput --samples 100 --branch-samples 100 --value-bytes 150
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-old-cache-scale -- --scales 100k --workloads load-seq,scan-prefix,scan-range-throughput --samples 100 --branch-samples 100 --value-bytes 150
```

Also run standard mode to confirm this is not cache-specific:

```sh
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 100k --engines standard --workloads load-seq,scan-prefix,scan-range-throughput --samples 100 --branch-samples 100 --value-bytes 150
```

Required test and check commands:

```sh
cargo fmt --all -- --check
cargo check -p strata-storage-next --features perf-trace
cargo test -p strata-storage-next --features perf-trace scan
cargo check --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale
cargo check --manifest-path benchmarks/Cargo.toml --bin storage-old-cache-scale
```

Expected storage-next counters after PERF-I3:

1. `read_view_rows_cloned` for L9 scan workloads: zero or not row-proportional.
2. `read_view_validation_rows_scanned` for L9 scan workloads: zero or not
   row-proportional.
3. `scan_rows_visited` for 100 scans over 100K keys: near returned rows plus
   same-key version/tombstone overhead, not 10,020,000.
4. `table_seeks`: at least one seek per source cursor used by each scan.
5. `scan_candidates`: near emitted candidate groups, not millions for a
   64-row-limited scan.

Expected benchmark movement:

1. scan-prefix should move from millisecond-scale p50 toward microsecond-scale
   p50.
2. scan-range-throughput should improve by at least one order of magnitude from
   the current 20 ops/s result.
3. If counters are bounded but throughput remains more than 2x slower than old
   cache, collect a CPU profile before implementing another scan optimization.
4. If counters remain row-proportional, PERF-I3 is not complete and no follow-up
   performance slice should start.

## Review Checklist

Before merging PERF-I3:

1. The implementation uses existing ordered table keys and cursors.
2. No secondary index was introduced.
3. L9 scan semantics match the existing read-view implementation.
4. Limit pushdown happens before full candidate materialization.
5. Inherited rows are rewritten and fork-capped correctly.
6. Tombstone and TTL behavior is covered by tests.
7. Cache and standard modes share the same scan mechanics.
8. The benchmark rerun includes old-cache and storage-next cache results from
   the same machine.
9. The decision report records both throughput/latency and row-work counters.

