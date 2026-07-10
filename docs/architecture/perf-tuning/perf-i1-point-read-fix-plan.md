# PERF-I1 Point Read Fix Plan

## Goal

Restore storage-next point-read mechanics to the old storage shape:

1. do not deep-clone every branch row for one point lookup;
2. do not scan every branch row to find one physical key;
3. use the existing ordered internal-key table layout;
4. keep scan, history, immutable-table laziness, append staging, and blind commit
   work out of this slice.

PERF-P2 proved that either isolated change is insufficient. The production fix
must combine a borrowed point-read path with ordered-key seek.

## Baseline

PERF-P2 at 100K keys:

| Case | Throughput | Rows cloned | Rows visited |
| --- | ---: | ---: | ---: |
| current view + current scan | 35 ops/s | 100,200,000 | 100,200,000 |
| current view + direct seek | 44 ops/s | 100,200,000 | 1,000 |
| borrowed view + current scan | 478 ops/s | 0 | 100,200,000 |
| borrowed view + direct seek | 1,127,342 ops/s | 0 | 1,000 |

The implementation target is the combined shape. The benchmark target is not
exactly 1.1M ops/s, because production API mapping and lifecycle admission still
add overhead. The required gate is at least one order of magnitude over the
current L9 point-read result, with counters proving bounded row work.

## Scope

In scope:

1. production point reads through the L9 storage API;
2. cache and durable runtime point paths;
3. active and frozen in-memory tables;
4. owned branch tables;
5. inherited branch layers;
6. latest, version-bound, and timestamp-bound point reads;
7. tombstone and TTL visibility behavior;
8. perf counters for production point reads.

Out of scope:

1. range scans and prefix scans;
2. history reads;
3. lazy immutable-table readers;
4. secondary point-read indexes;
5. storage format changes;
6. commit conflict read-view optimization;
7. append staging or blind commit optimization.

## Architecture Rules

PERF-I1 must not create a parallel point-read product inside storage-next. The
fix has to preserve one semantic model with multiple source adapters.

Required shape:

1. Table code owns ordered-key seek primitives.
2. Branch read code owns point candidate selection and visibility semantics.
3. `BranchReadView` remains the owned snapshot adapter.
4. `BranchLocalState` may add a borrowed point adapter, but it must feed the same
   branch point-selection core as `BranchReadView`.
5. Lifecycle/API code may route to the borrowed adapter only after branch-level
   equivalence tests prove it matches the existing read-view behavior.

Forbidden shape:

1. No lifecycle/API-only point lookup logic.
2. No source-precedence decisions outside branch read code.
3. No latest-only behavior hidden behind a general point-read method name.
4. No scan/history changes to justify the point-read fix.
5. No new storage structure whose only purpose is masking the scan bug.

## Implementation Steps

### PERF-I1A: Ordered-Key Seek Helpers

Promote the benchmark-only seek helper into normal table code.

Files:

1. `crates/storage-next/src/table/mutable.rs`
2. `crates/storage-next/src/table/reader.rs`
3. `crates/storage-next/src/table/mod.rs`

Work:

1. Add non-`perf-trace` physical-key seek helpers for `MutableTable` and
   `FrozenTable`.
2. The helper must seek from `(physical_key, CommitVersion::MAX)` and stop when
   the physical key changes.
3. The helper must accept an effective read bound, not just `Latest`, so
   historical and timestamp reads do not need a fallback scan.
4. Keep the current `perf_trace::record_table_seek()` counter wired when the
   feature is enabled.
5. Do not add a secondary index.

Exit gates:

1. Unit tests prove active/frozen seek returns newest visible row for duplicate
   physical keys across versions.
2. Tests cover missing key and version-bound reads.
3. `point_rows_visited` is bounded to the target key chain in seek tests.

### PERF-I1B: Borrowed Point Source

Add a point-only borrowed read path on `BranchLocalState`.

Files:

1. `crates/storage-next/src/branch/state/read_hooks.rs`
2. `crates/storage-next/src/branch/read.rs`

Work:

1. Add `BranchLocalState::read_point_or_tombstone_borrowed(key, bound)` or
   equivalent so the borrowed path preserves the existing delete-fact behavior.
2. This method must validate branch id and timestamp coverage the same way
   `BranchReadView::read_point` does.
3. Extract shared point-selection helpers so both `BranchReadView` and
   `BranchLocalState` use the same candidate ordering, tombstone, TTL, and
   inherited-layer rules.
4. The borrowed adapter must borrow active, frozen, owned, and inherited sources
   directly from `BranchLocalState`.
5. It must collect only matching key-chain candidates from each source and then
   call the shared visible-row selection logic.
6. Keep scans/history on `BranchReadView` in this slice.

Correctness fences:

1. Do not choose a source manually before applying commit-version ordering.
   Existing selection sorts by commit version first, then source precedence.
2. Inherited rows must still rewrite the source branch id into the child branch
   id before returning.
3. Inherited rows must still cap visibility at the inherited layer fork version.
4. Timestamp reads must still enforce `BranchTimestampCoverage` before reading.
5. TTL expiration must remain timestamp-bound only.

Exit gates:

1. Existing branch read-view tests still pass.
2. New borrowed-point tests compare borrowed results against the old
   `capture_read_view().read_point(...)` result for active, frozen, owned, and
   inherited sources.
3. Tests cover latest, at-version, at-timestamp, tombstone, and TTL-expired
   outcomes.

### PERF-I1C: L9 API Point Path

Route only L9 point reads to the borrowed seek path.

Files:

1. `crates/storage-next/src/api/runtime.rs`
2. `crates/storage-next/src/lifecycle/cache.rs`
3. `crates/storage-next/src/lifecycle/durable/bootstrap.rs`

Work:

1. Add runtime/lifecycle helpers that return a borrowed point result instead of
   a full `BranchReadView`.
2. Name helper methods according to their supported bound. If the first
   production route supports only `ReadBound::Latest`, name it as latest-only
   and leave version/timestamp reads on the existing path until a point-only
   timeline resolver exists.
3. Update `StorageRuntime::read_point` to use the new helper only for bounds
   whose semantics are fully covered by branch-level equivalence tests.
4. Keep `read_history`, `scan_prefix`, `scan_range`, and timeline reads on
   `read_view_for_branch`.
5. Preserve `visible_tombstone_at_bound` behavior. If that helper requires a
   full read view, add a point-only equivalent rather than capturing a full view
   on every miss.
6. Closed-runtime and branch-not-found errors must stay unchanged.

Exit gates:

1. Existing API point-read tests pass without expectation changes.
2. Add an API-level perf-trace assertion that 1,000 point reads over 100K rows do
   not record row-proportional read-view clones.
3. Add an API-level perf-trace assertion that point row visits are bounded to
   key-chain work.

### PERF-I1D: Verification and PERF-P2 Rerun

Rerun both production and isolation benchmarks.

Commands:

```sh
cargo fmt --all -- --check
cargo check --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale
cargo check --manifest-path benchmarks/Cargo.toml --bin storage-next-point-spike
cargo test -p strata-storage-next --features perf-trace
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 100k --engines cache,standard --workloads point-throughput --samples 1000 --value-bytes 150
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-point-spike -- --scale 100k --samples 1000 --value-bytes 150
```

Expected production counters after PERF-I1:

1. `read_view_captures` for L9 point reads: `0` or not row-proportional.
2. `read_view_rows_cloned` for L9 point reads: `0` or not row-proportional.
3. `point_rows_visited` for 1,000 point reads: near key-chain count, not
   100,200,000.
4. `table_seeks`: at least one seek per point request per searched table source.

Expected benchmark result:

1. L9 point throughput improves by at least 10x from the PERF-P0 44-45 ops/s
   baseline.
2. If production point reads remain below 10x improvement while counters are
   bounded, collect a CPU profile before implementing another performance slice.
3. If counters still show row-proportional cloning or row visits, PERF-I1 is not
   complete.

## Review Checklist

1. No scan/history code path changed except shared helper extraction.
2. No secondary index or new persistent format was added.
3. The old `BranchReadView` path remains available for tests, scans, history,
   timelines, and conflict validation.
4. Borrowed point reads match existing read-view point semantics on all covered
   source kinds.
5. PERF-P2 is rerun after implementation and the report is updated with
   production L9 results, not only benchmark-local spike results.

## Stop Conditions

Stop before broadening scope if any of these happen:

1. The borrowed point path needs ownership changes that affect branch mutation,
   compaction, flush, or snapshot install.
2. Historical/timestamp reads require scanning full tables to stay correct.
3. Inherited source precedence differs from current `BranchReadView` semantics.
4. The production benchmark does not improve after counters prove bounded row
   work.

In those cases, collect a CPU profile and write a new decision report before
starting the next performance slice.
