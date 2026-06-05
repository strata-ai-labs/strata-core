# PERF-I4 Branch Scan Iterator Pipeline Plan

## Scope

PERF-I4 ports the old storage scan methodology into storage-next's branch scan
path. The goal is to replace the remaining per-row generic branch merge loop
with a seekable iterator pipeline that caches current source state, groups one
physical key at a time, and stops when the public scan limit is satisfied.

This is a targeted serving-path correction. It is not a new index, not a public
API change, not a table format change, and not a branch rearchitecture.

PERF-I3 restored bounded seeks and limit pushdown. PERF-I4 is the follow-up
needed after scale-valid flush runs showed that source-count scaling remains
too expensive.

## Evidence

Latest storage-next cache runs use public L9 APIs, 64-byte values, 100 scan
samples, and scan limit 64.

| Scale | Load shape | Scan sources | Logical key encodes | Scan elapsed | Scan throughput |
| --- | --- | ---: | ---: | ---: | ---: |
| 100K | active only | 1 | 19,200 | ~8.02 ms | ~12.5K ops/s |
| 250K | flush every 100K rows | 3 | 44,800 | ~8.12 ms | ~12.3K ops/s |
| 1M | flush every 100K rows | 11 | 134,400 | ~16.27 ms | ~6.1K ops/s |

At 1M, storage-next still returns only 6,400 visible rows, but scans perform
1,100 cursor seeks and 134,400 logical key encodes. The hot phases are:

1. source setup: ~2.36 ms;
2. min-key selection: ~4.93 ms;
3. group-key checks: ~3.99 ms;
4. row candidate materialization: ~0.40 ms;
5. cursor advance: ~0.17 ms;
6. public API row mapping: ~0.18 ms.

Conclusion:

1. The active-mutable budget no longer blocks scale measurement when load uses
   public flush maintenance.
2. The remaining regression is source-count proportional scan loop overhead.
3. The specific repeated work is logical-key recomputation and linear min/group
   selection across branch sources.
4. Candidate row cloning is visible but not the dominant 1M bottleneck.

## Old Methodology To Port

Old storage does not scan every source repeatedly for each output row. It uses:

1. a seekable iterator pipeline built once per scan;
2. per-source iterators positioned with one seek;
3. a merge iterator with cached current key and source index;
4. an MVCC iterator that groups versions of one logical key;
5. lazy yielding until the caller's limit is reached.

Reference files:

1. `crates/engine/src/primitives/kv.rs::scan`
2. `crates/storage/src/segmented/mod.rs::StorageIterator`
3. `crates/storage/src/seekable.rs::MergeSeekableIter`
4. `crates/storage/src/seekable.rs::MvccSeekableIter`

Storage-next should port that iterator discipline into its own branch/table
vocabulary rather than route through old storage code.

## Non-Goals

1. Do not add a secondary scan index.
2. Do not special-case benchmark key formats.
3. Do not change L9 scan request or response types.
4. Do not change durable table, WAL, object, manifest, or recovery formats.
5. Do not remove tombstone, TTL, timestamp, version-bound, or inherited-layer
   behavior.
6. Do not replace table cursors wholesale.
7. Do not rewrite compaction, materialization, snapshot install, or maintenance.
8. Do not leave benchmark-only fast paths in production code.

## Correctness Contract

The new iterator path must match the current branch scan result for:

1. latest reads;
2. version-bound reads;
3. timestamp-bound reads after timeline resolution;
4. prefix scans;
5. bounded range scans;
6. empty ranges;
7. lower-bound seeks where the lower key is absent;
8. active rows;
9. frozen rows;
10. owned immutable-table rows;
11. inherited rows rewritten from source branch to child branch;
12. inherited fork-version visibility caps;
13. non-readable inherited layers;
14. tombstones suppressing older values;
15. tombstones returned through the internal including-tombstones path;
16. TTL expiration only when a read timestamp is supplied;
17. duplicate physical keys across multiple sources;
18. duplicate physical keys across multiple commit versions.

Candidate selection must preserve the existing ordering in
`branch/read.rs::sort_candidates_newest_first`:

1. commit version descending;
2. source order tie-break: active, frozen by index, owned by level/table index,
   inherited by layer/source branch.

Public scan output must remain user-key ascending after MVCC/tombstone/TTL
selection.

## Implementation Shape

Add a branch-level scan iterator pipeline that owns no row data beyond the
current candidate group.

Recommended internal pieces:

1. `BranchScanSourceCursor`: wraps one `TableCursor`, its source identity, its
   effective read bound, and cached current logical key.
2. `BranchScanSourceItem`: cached heap item containing logical physical key and
   source index.
3. `BranchScanMerge`: single-source direct path plus multi-source heap path.
4. `BranchScanReducer`: consumes one logical physical key group, applies
   version/timestamp/source ordering, tombstone/TTL rules, and yields at most
   one `BranchHistoryRow`.

The first implementation can still return `Vec<BranchHistoryRow>` from the
existing `scan_including_tombstones_*` functions for API compatibility. The
vector must be filled from the iterator stream and stop as soon as the visible
limit is reached.

## Work Slices

Keep every implementation slice under the normal 1,500 net LOC target. Split
before implementation if a slice grows beyond that.

### PERF-I4A: Pin Current Semantics

Files:

1. `crates/storage-next/src/branch/read.rs`
2. `crates/storage-next/src/branch/tests` or the closest existing branch-read
   test module

Work:

1. Add comparison tests that run the current captured read-view scan and the
   borrowed scan helper over the same branch state.
2. Cover active-only, active plus frozen, active plus owned, and inherited
   sources.
3. Cover tombstone, version-bound, timestamp-bound, TTL, prefix, range, empty
   range, and visible limit cases.
4. Add a test where two sources contain the same physical key and commit
   version so source-order tie-break remains pinned.

Exit gates:

1. tests fail if source ordering changes;
2. tests fail if inherited rows leak source-branch keys;
3. no production behavior change in this slice.

### PERF-I4B: Source Cursor With Cached Logical Key

Files:

1. `crates/storage-next/src/branch/read.rs` or new
   `crates/storage-next/src/branch/scan.rs`
2. `crates/storage-next/src/branch/mod.rs`
3. `crates/storage-next/src/observability/perf_trace.rs`

Work:

1. Introduce a private source cursor wrapper around existing `TableCursor`.
2. Cache current logical physical key after `seek_to_first` and after each
   advance.
3. Apply inherited key rewriting only when refreshing the cached key.
4. Preserve effective read-bound checks in the row reducer, not in heap key
   ordering.
5. Add counters for source count, cached-key refreshes, heap pops, and emitted
   groups if current counters are insufficient.

Exit gates:

1. existing branch read tests pass;
2. new unit tests prove cached key refresh matches `current_logical_physical_key`;
3. no public API changes.

### PERF-I4C: Single-Source Direct Scan Path

Files:

1. `crates/storage-next/src/branch/read.rs` or new branch scan module

Work:

1. When scan source construction yields exactly one source, iterate it directly.
2. Group adjacent rows with the same logical physical key.
3. Select the visible row using the same commit-version and source-order rules.
4. Stop immediately once the visible limit has been reached.
5. Keep tombstone rows available to the including-tombstones internal path.

Exit gates:

1. 100K active-only scan no longer performs three logical key encodes per
   returned row.
2. 100K active-only scan approaches old-cache order of magnitude without
   changing returned rows.
3. `cargo test -p strata-storage-next --lib --features perf-trace branch`.

### PERF-I4D: Multi-Source Heap Merge Path

Files:

1. `crates/storage-next/src/branch/read.rs` or new branch scan module
2. `crates/storage-next/src/table/cursor.rs` only if a small cursor helper is
   needed

Work:

1. Replace linear min-key selection across every cursor with a min-heap keyed
   by cached logical physical key.
2. Pop all sources for the current logical physical key into a candidate group.
3. Advance only popped sources and push them back if still valid.
4. Preserve deterministic source order for equal keys and equal versions.
5. Do not allocate a candidate `Vec` larger than the number of sources
   currently sharing the same logical key.

Exit gates:

1. 250K and 1M scans no longer show logical key encodes growing as
   `returned_rows * source_count * repeated_checks`.
2. 1M `branch_scan_min_key_ns` and `branch_scan_group_key_ns` fall materially.
3. branch scan correctness tests from PERF-I4A still pass.

### PERF-I4E: API Integration And Benchmark Closeout

Files:

1. `crates/storage-next/src/api/runtime.rs`
2. `benchmarks/src/bin/storage_next_l9_scale.rs`
3. `docs/architecture/perf-tuning/perf-p2-point-read-isolation-report.md` or a
   new scan closeout report

Work:

1. Keep `scan_prefix` and `scan_range` routed through the borrowed latest path.
2. Keep version/timestamp scans on existing read-view path unless this slice
   has already proven parity for those bounds.
3. Rerun the same benchmark matrix:
   - 100K cache active-only;
   - 250K cache with `--flush-every 100000`;
   - 1M cache with `--flush-every 100000`;
   - old-cache 100K comparison.
4. Record result JSON paths and before/after counters.
5. Remove or narrow temporary diagnostic counters that are no longer useful.

Exit gates:

1. `cargo fmt --all -- --check`
2. `cargo check -p strata-storage-next --features perf-trace`
3. `cargo check --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale`
4. targeted branch read tests
5. same-result scan parity tests
6. benchmark report shows whether the source-count regression is resolved

## Stop Conditions

Stop and reassess if any of these happen:

1. the single-source path changes scan results in any parity test;
2. the heap path cannot preserve source tie-break order cleanly;
3. inherited-layer rewriting needs broad branch-state changes;
4. 1M scan time does not materially improve after min/group key timers fall;
5. the implementation requires durable format, table format, or public API
   changes.

## Expected Result

After PERF-I4, storage-next scan cost should scale primarily with:

1. number of rows returned;
2. number of sources that actually overlap the scan bounds;
3. number of duplicate versions for the same physical key.

It should no longer scale with repeated logical-key recomputation across every
source for every returned row. The 1M cache scan should move back toward the
100K scan time band for the same returned-row count, subject to actual source
count and table cursor costs.
