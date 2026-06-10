# M4P-L6L Test Plan: Branch Read Hot Path Cleanup

Status: draft follow-on test plan

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l6l-branch-read-hot-path-implementation-plan.md`

## Test Objectives

The test suite must prove two things independently:

1. point-read semantics are unchanged for latest, version-bounded,
   timestamp-bounded, tombstone, TTL, owned, and inherited reads;
2. the hot path no longer traverses or clones rows from sources that cannot
   affect the selected point-read result.

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
   physical key.

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
6. inherited layers are not probed after a local candidate is proven final.

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
6. row-clone bytes are not proportional to matching layers.

Expected performance movement:

1. 100K latest point-read throughput should improve materially before starting
   lazy block decode or block cache work.
2. If 100K throughput does not improve while counters show bounded traversal,
   collect a CPU profile before implementing the next read-path slice.
3. Do not run 5M or 10M as the primary gate until 1M has a sane point-read
   number and counters show the branch path is no longer doing full traversal.

## Verification Commands

Focused commands:

```sh
cargo fmt --manifest-path crates/storage-next/Cargo.toml --all
cargo test --manifest-path crates/storage-next/Cargo.toml --lib table::tests::reader
cargo test --manifest-path crates/storage-next/Cargo.toml --lib branch::tests::point_pruning
cargo test --manifest-path crates/storage-next/Cargo.toml --lib branch::tests::read_view
cargo test --manifest-path crates/storage-next/Cargo.toml --lib branch::tests::inheritance_materialization
cargo test --manifest-path crates/storage-next/Cargo.toml --lib api::tests::read
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
4. lazy block decode or block cache contention becomes the measured dominant
   cost after source traversal and row cloning are bounded;
5. a required fix would change durable table format or public read semantics.

