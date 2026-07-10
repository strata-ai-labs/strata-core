# M4P-L6L Test Plan: Branch Read Hot Path Cleanup

Status: draft follow-on test plan

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l6l-branch-read-hot-path-implementation-plan.md`

## Test Objectives

The test suite must prove two things independently:

1. point-read semantics are unchanged for latest, version-bounded,
   timestamp-bounded, tombstone, TTL, owned, and inherited reads;
2. the hot path no longer traverses or clones rows from sources that cannot
   affect the selected point-read result;
3. lazy point reads no longer materialize full data blocks when only one
   physical-key chain is needed;
4. runtime-opened lazy table readers use a shared, correctly keyed block cache
   instead of fragmented per-reader caches.

Correctness tests should not depend on perf counters. Mechanical counter tests
should use the existing perf-gated test pattern and should be narrow.

## Correctness Tests

### Table Prepared Lookup Tests

Add or update table tests for `MutableTable`, `FrozenTable`, and
`ImmutableTableReader`.

Required cases:

1. prepared lookup returns the same row as the existing unprepared
   `seek_physical_key` for latest reads;
2. prepared lookup returns the same row for version-bounded reads;
3. prepared lookup returns the same row for timestamp-bounded reads;
4. missing physical keys return `None`;
5. keys before the first table key and after the last table key return `None`;
6. multiple versions for one physical key stop at the first matching visible
   version under the bound;
7. prepared lookup works for eager and lazy immutable readers;
8. prepared lookup preserves table runtime errors from lazy readers.

### Branch Selector Equivalence Tests

For each case below, compare the new ordered selector against the previous
candidate-collection selector kept as a test-only reference, or against an
independent model that sorts all candidates by commit version and source order.

Required source cases:

1. active-only latest hit;
2. frozen-only latest hit;
3. owned L0 latest hit;
4. owned nonzero-level latest hit;
5. inherited L0 latest hit;
6. inherited nonzero-level latest hit;
7. no hit anywhere;
8. active tombstone hides older local and inherited rows;
9. frozen tombstone hides owned and inherited rows when it is the selected row;
10. owned tombstone hides inherited rows when it is the selected row;
11. inherited tombstone is returned by the tombstone-preserving borrowed path
    and hidden by visible-row APIs;
12. TTL-expired rows are hidden only for timestamp-bounded visible reads;
13. latest and version-bounded reads do not apply TTL expiration;
14. wrong-branch physical keys are rejected before any source probes;
15. timestamp reads without coverage are rejected before any source probes.

### Source Ordering And Tie-Break Tests

Add targeted branch tests where multiple sources contain the same physical key.

Required assertions:

1. highest commit version wins across active, frozen, owned, and inherited
   sources;
2. source order only breaks ties after commit version equality;
3. frozen table index ordering matches current `source_order_cmp`;
4. owned L0 table index ordering matches current `source_order_cmp`;
5. owned level ordering matches current `source_order_cmp`;
6. inherited layer index and source branch id ordering match current
   `source_order_cmp`;
7. early exit does not return a lower-version row when a later source can still
   contain a higher version under the effective bound.

### Historical Bound Tests

Use latest, version, and timestamp bounds over the same fixture.

Required assertions:

1. latest active hit exits early;
2. an active row above an `AtVersion` bound does not hide an older valid row in
   frozen or owned sources;
3. an active row above an `AtTimestamp` bound does not hide an older valid row
   in frozen or owned sources;
4. if a source group produces a valid candidate under the bound and remaining
   source facts cannot beat it, the selector exits;
5. if remaining source facts can beat the candidate, the selector continues.

### Inherited Branch Tests

Use child branches with readable, materialized, and non-readable inherited
layers.

Required assertions:

1. child-local row skips inherited traversal when it is newest under the bound;
2. child-local tombstone skips inherited traversal when it is newest under the
   bound;
3. inherited traversal occurs when no local row answers;
4. inherited traversal occurs when an inherited source can still beat the local
   candidate under the bound;
5. inherited rows rewrite source branch id to child branch id only for selected
   returned rows;
6. inherited fork version caps visibility for latest, version, and timestamp
   bounds;
7. materialized or unreadable inherited layers are not probed.

### Regression Guards For Non-Point Reads

The read cleanup must not change scan or history behavior.

Required assertions:

1. existing branch history tests pass without expectation updates;
2. existing prefix/range scan tests pass without expectation updates;
3. read-view capture/pinning tests still pass;
4. commit conflict validation that uses `BranchReadView` still sees the same
   latest/tombstone behavior;
5. L9 public read tests still return the same values and errors.

### Lazy Data-Block Point Seek Tests

Add focused table reader tests for lazy immutable readers opened from a byte
source.

Required assertions:

1. latest lazy point seek returns the same row as the existing full-block decode
   path for a single-version key;
2. version-bounded lazy point seek returns the newest row at or below the bound
   without decoding unrelated rows in the block;
3. timestamp-bounded lazy point seek returns the newest row at or below the
   timestamp bound without decoding unrelated rows in the block;
4. a matching physical-key chain with no visible version continues into the
   next data block when the chain spans a block boundary;
5. a nonmatching key inside a candidate block stops before decoding unrelated
   row payload bytes after the physical key becomes greater than the target;
6. a definite-negative table filter still skips data-block reads before the new
   lazy point block cursor is entered;
7. eager readers keep their existing path and results;
8. scans, cursors, `rows()`, and materialization still decode complete data
   blocks and return the same ordered rows as before.

### Shared Block Cache Correctness Tests

Add table, service, and lifecycle/runtime tests for the shared cache wiring.

Required assertions:

1. two lazy readers for the same table identity and same runtime cache share a
   cached data block;
2. two lazy readers for different table identities but identical block offsets
   and lengths do not collide;
3. runtime-opened branch table readers attach the runtime shared cache by
   default when the block-cache budget is nonzero;
4. zero block-cache budget keeps cache disabled and does not attach an enabled
   cache;
5. explicit `ImmutableTableReader::with_block_cache` still overrides/attaches
   the supplied cache for direct table tests;
6. `remove_table` invalidates all shards for one table without removing another
   table's blocks;
7. `clear` removes entries from every shard;
8. `resize` changes aggregate capacity and evicts as needed without corrupting
   stats;
9. duplicate inserts still return the existing bytes for the same full
   table/block key.

## Mechanical Counter Tests

All tests in this section should be behind the perf-trace feature or the
existing perf-gated assertion style.

### Early Exit Counters

Required assertions:

1. active latest hit records one table seek and zero frozen, owned, and
   inherited probes;
2. frozen latest hit records active plus frozen probes up to the selected
   frozen source, and no owned or inherited probes when remaining source facts
   cannot beat it;
3. owned nonzero-level latest hit records at most one table seek per nonzero
   level entered;
4. child-local hit records zero inherited table seeks when inherited sources
   cannot beat the local candidate;
5. miss records no early exit and probes every source that could contain the
   key;
6. version/timestamp reads record no unsafe early exit when a later source can
   still contain a higher valid version.

### Prepared Key Counters

Required assertions:

1. a local point read builds the local prepared lookup once;
2. the local prepared lookup is reused across active, frozen, and owned tables;
3. inherited prepared lookups are built only for inherited layers that are
   entered;
4. table-level unprepared seek wrappers still build one prepared lookup and
   then call the prepared path;
5. point reads over many L0 tables do not build one internal seek key per L0
   table.

### Clone Counters

Required assertions:

1. a point read with matching rows in active, frozen, owned, and inherited
   sources clones only the selected row;
2. a point read hidden by a selected tombstone clones only the selected
   tombstone row on the tombstone-preserving path;
3. loser rows with large values do not increment row-clone byte counters;
4. inherited branch-id rewrite counters increment only for selected inherited
   rows;
5. misses clone zero rows.

### Eager Filter Counters

Required assertions:

1. eager immutable reader absent-key lookup records a negative filter probe and
   zero rows visited;
2. eager immutable reader positive lookup records a positive filter probe and
   returns the same row as the unfiltered path;
3. eager immutable reader false-positive lookup remains correct;
4. unavailable eager filter falls back to binary search and records an
   unavailable probe;
5. filter construction does not run per point read.

### Lazy Point Decode Counters

Required assertions:

1. cold lazy point hit records one data-block frame read and one lazy point
   block scan;
2. cold lazy point hit decodes only the matching physical-key chain rows, not
   every row in the data block;
3. lazy point miss inside a candidate block records encoded entries inspected
   but zero candidate row payload decodes when no matching physical key exists;
4. lazy point seek over a multi-block version chain records one block scan per
   touched block and row payload decodes only for matching chain entries;
5. full cursor/materialization paths continue to increment full-block decode
   and row materialization counters;
6. cache hits avoid source reads but still perform the lazy point payload scan
   against cached block bytes.

### Shared Cache Counters

Required assertions:

1. a second reader for the same table/block records a cache hit after the first
   reader inserts the block;
2. readers for different table identities record misses for the same offset and
   length;
3. cache stats aggregate hits, misses, inserts, duplicate inserts, evictions,
   removes, clears, skipped oversized, and skipped disabled across shards;
4. the aggregate byte gauge never exceeds configured capacity after inserts and
   evictions settle;
5. disabled runtime cache records skipped inserts or no-cache behavior
   consistently with the existing table-cache contract;
6. perf-gated cache assertions stay narrow and do not become a required part of
   every read-path correctness test.

## Fault And Failure Tests

Required cases:

1. malformed table key-range facts still return the existing branch/table error;
2. inherited physical-key rewrite failure returns the existing inherited-layer
   error;
3. inherited row branch rewrite failure returns the existing inherited-layer
   error;
4. a lazy table read error aborts the point read without mutating branch state;
5. a mismatched supplied filter is rejected at reader construction or filter
   attachment, not during an unrelated point read;
6. a corrupt lazy block touched by a point read still reports the existing table
   error;
7. an untouched corrupt lazy block does not affect a point read for another
   physical key;
8. a corrupt lazy block with the matching row near the front still fails if a
   later encoded entry in the same block violates sorted-order, duplicate-key,
   key-length, row-length, or first/last-index validation;
9. a cache shard mutex poison recovers with the existing poison-tolerant cache
   behavior;
10. cache insert failure or invalid cache key construction aborts reader cache
    attachment without changing read semantics;
11. a failed lazy block read is not inserted into the shared cache.

## Generated Tests

Extend generated branch LSM workloads with point-read equivalence checks.

Generated fixture dimensions:

1. active rows present or absent;
2. zero through several frozen tables;
3. L0 table counts from zero through at least eight;
4. nonzero level counts from one through at least four;
5. nonzero tables with overlapping and non-overlapping physical ranges where
   the branch invariants allow them;
6. inherited layer counts from zero through at least three;
7. materialized and unreadable inherited layers;
8. same-physical-key version chains across several source kinds;
9. tombstones at active, frozen, owned, and inherited source positions;
10. TTL rows with timestamp bounds before and after expiration;
11. version bounds below, inside, and above the retained version chain;
12. timestamp bounds below, inside, and above retained timestamp ranges.
13. lazy tables with one through several data blocks;
14. same-physical-key version chains split at data-block boundaries;
15. shared cache enabled and disabled runtime budgets.

Generated invariants:

1. new ordered point selector matches the independent model for visible rows;
2. new ordered tombstone-preserving selector matches the independent model for
   visible-or-tombstone rows;
3. branch scans and history remain unchanged before and after the selector
   rewrite;
4. no generated workload depends on collecting all candidates for correctness;
5. source probes are bounded by active/frozen/L0 plus at most one table per
   nonzero level unless the fixture intentionally keeps newer possible sources
   after an earlier hit;
6. inherited layers are not probed after a local candidate is proven final;
7. lazy point seek and full-block decode return equivalent point-read results;
8. shared cache hits and misses do not change read results or error results.

## Benchmark Gates

Run benchmarks only after the correctness and counter gates pass.

Required setup:

1. load the benchmark data;
2. manually flush because automatic flush is not yet the target of this slice;
3. explicitly compact to the intended L0-L7 shape;
4. confirm source layout before measuring point reads;
5. run cache and standard modes where available.

Required runs:

```sh
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 100k --engines cache,standard --workloads point-throughput --samples 1000 --value-bytes 150
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 1m --engines cache,standard --workloads point-throughput --samples 1000 --value-bytes 150
```

Expected counter movement:

1. point candidate materialization falls to at most one candidate per result;
2. point table seeks fall sharply for active/frozen/local-hit workloads;
3. inherited table seeks are zero for child-local hits that are final;
4. table point rows visited are bounded by key-chain length plus false-positive
   filter cases;
5. prepared key builds are not proportional to table probes;
6. row-clone bytes are not proportional to matching layers;
7. lazy point reads do not materialize all rows in the touched data block;
8. repeated lazy point reads through separate runtime-opened readers hit the
   shared block cache when they target the same table/block.

Expected performance movement:

1. 100K latest point-read throughput should improve materially after source
   traversal, clone deferral, eager filters, lazy point seek, and shared cache
   wiring are all in place.
2. If 100K throughput does not improve while counters show bounded traversal
   and bounded row materialization, collect a CPU profile before adding another
   read-path slice.
3. Do not run 5M or 10M as the primary gate until 1M has a sane point-read
   number and counters show the branch path is no longer doing full traversal.

## Verification Commands

Focused commands:

```sh
cargo fmt --manifest-path crates/storage-next/Cargo.toml --all
cargo test --manifest-path crates/storage-next/Cargo.toml --lib table::tests::reader
cargo test --manifest-path crates/storage-next/Cargo.toml --lib table::tests::cache
cargo test --manifest-path crates/storage-next/Cargo.toml --lib service::table
cargo test --manifest-path crates/storage-next/Cargo.toml --lib branch::tests::point_pruning
cargo test --manifest-path crates/storage-next/Cargo.toml --lib branch::tests::read_view
cargo test --manifest-path crates/storage-next/Cargo.toml --lib branch::tests::inheritance_materialization
cargo test --manifest-path crates/storage-next/Cargo.toml --lib api::tests::read
cargo test --manifest-path crates/storage-next/Cargo.toml --lib --features perf-trace table::tests::reader
cargo test --manifest-path crates/storage-next/Cargo.toml --lib --features perf-trace table::tests::cache
cargo test --manifest-path crates/storage-next/Cargo.toml --lib --features perf-trace branch::tests::point_pruning
cargo clippy --manifest-path crates/storage-next/Cargo.toml --lib --all-features -- -D warnings
git diff --check
```

Broader gate before benchmarking:

```sh
cargo test --manifest-path crates/storage-next/Cargo.toml --lib --all-features
```

## Stop Conditions

Stop the slice and write a decision note if:

1. generated equivalence finds a semantic disagreement between the new selector
   and the independent model;
2. early-exit counters improve but point-read throughput does not move;
3. eager filters slow table construction enough to affect load or flush
   throughput materially;
4. lazy point seek cannot preserve full data-block corruption validation;
5. shared cache wiring requires changing durable table identity, object layout,
   or public read semantics;
6. sharded cache eviction or stats cannot remain compatible with existing
   `TableBlockCache` behavior.
