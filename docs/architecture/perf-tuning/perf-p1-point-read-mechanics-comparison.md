# PERF-P1 Point-Read Mechanics Comparison

## Scope

This note compares the old storage engine's point-read mechanics against
storage-next after the PERF-P0 counter run. The goal is to decide the next
measurement or correction step without turning performance tuning into another
architecture rewrite.

This is analysis only. No serving-path behavior changed in PERF-P1.

## Baseline Evidence

PERF-P0 measured the 100K-key workload on the same local machine:

| Engine | Mode | Load | Point latest | Range scan |
| --- | --- | ---: | ---: | ---: |
| old storage | cache | 434,758 ops/s | 464,936 ops/s | 88,382 ops/s |
| storage-next | cache | 35,472 ops/s | 44 ops/s | 22 ops/s |
| storage-next | standard | 42,838 ops/s | 45 ops/s | 22 ops/s |

For 1,000 storage-next point reads at 100K keys, PERF-P0 recorded:

| Counter | Cache | Standard |
| --- | ---: | ---: |
| read-view captures | 1,000 | 1,000 |
| read-view rows cloned | 100,200,000 | 100,200,000 |
| point rows visited | 100,200,000 | 100,200,000 |
| point candidates materialized | 1,000 | 1,000 |
| table seeks | 0 | 0 |

The 100,200 rows per lookup are the 100,000 user rows plus two timeline rows per
100 load commits. The counters prove two independent row-proportional costs on
every point read:

1. `BranchReadView` capture clones all branch rows.
2. Candidate collection scans all branch rows to find one physical key.

## Old Engine Mechanics

The old engine used its ordered internal key layout as the serving structure.
It did not need a separate point-read index.

`crates/storage/src/memtable.rs:1` documents the in-memory layout:

1. rows are ordered by `InternalKey`;
2. that order is physical key ascending and commit id descending;
3. point reads seek to `(key, +infinity)` and return the first visible version.

The actual memtable point path does that directly. `get_versioned_preencoded`
constructs a seek key, uses `self.map.range(seek_key..)`, stops as soon as the
typed key changes, and returns the first commit visible to the snapshot
(`crates/storage/src/memtable.rs:230`).

The branch point read probes sources in precedence order and uses keyed lookup
for each source (`crates/storage/src/segmented/mod.rs:4336`):

1. active memtable;
2. frozen memtables, newest first;
3. L0 segments, newest first;
4. L1+ segments through level search;
5. inherited layers.

Immutable segments also use the ordered-key structure. `KVSegment::point_lookup`
does bloom check, index search, and block scan bounded to the target physical
key (`crates/storage/src/segment.rs:529`, `crates/storage/src/segment.rs:541`,
`crates/storage/src/segment.rs:704`).

The old read snapshot is also cheap. `snapshot_branch` clones `Arc` handles for
active memtable, frozen memtables, segment version, and inherited layers
(`crates/storage/src/segmented/mod.rs:5137`). It does not clone every row.

## Storage-Next Mechanics

Storage-next kept the same internal-key ordering foundation. `encode_internal_key`
appends the bitwise inverse commit version in big-endian order, so ordinary
ascending byte order returns newest commit versions first for the same physical
key (`crates/storage-next/src/format/key.rs:36`).

The table layer also has ordered storage:

1. `MutableTable` stores rows in `BTreeMap<TableInternalKeyBytes, TableRow>`
   (`crates/storage-next/src/table/mutable.rs:47`).
2. `FrozenTable` uses the same shape (`crates/storage-next/src/table/mutable.rs:149`).
3. `ImmutableTableReader` stores sorted rows and supports binary search for exact
   internal keys (`crates/storage-next/src/table/reader.rs:126`).
4. Table cursors support binary seek (`crates/storage-next/src/table/cursor.rs:14`).

But the current branch read path does not use that ordering for point reads.
`BranchReadView::point_candidates` scans each source with `iter().filter(...)`
and clones each matching row (`crates/storage-next/src/branch/read.rs:889`).
Inherited point candidates do the same over inherited owned tables
(`crates/storage-next/src/branch/read.rs:993`).

`BranchReadView` is owned, not pinned. It stores `MutableTable`, `Vec<FrozenTable>`,
owned levels, and inherited layers by value (`crates/storage-next/src/branch/read.rs:659`).
`BranchLocalState::capture_read_view` constructs it by cloning active, frozen,
owned, and inherited sources (`crates/storage-next/src/branch/state/read_hooks.rs:216`).

For durable/flushed tables there is an additional old-vs-new gap:
`ImmutableTableReader::open_source` reads every data block and decodes all rows
into memory at open time (`crates/storage-next/src/table/reader.rs:93`,
`crates/storage-next/src/table/reader.rs:277`). The old segment reader loaded
metadata and used bloom/index/block-cache reads on demand.

## Interpretation

The regression is not caused by missing key-order semantics. Storage-next already
has the same logical ordering needed for old-style point lookup.

The regression is caused by adapter-level serving-path divergence:

1. Point reads use row filtering instead of ordered-key seek.
2. Read snapshots deep-clone table contents instead of pinning shared state.
3. Immutable table readers eagerly decode all rows, which will matter more after
   flush/durable serving enters the read path.

This means adding a new secondary index is the wrong first move. The old engine's
point-read speed came from using the primary ordered internal-key layout correctly.

## Fix Options

### Option A: Implement Point Seek First

Add physical-key seek helpers to `MutableTable`, `FrozenTable`, and
`ImmutableTableReader`, then update branch point reads to collect only the target
key's version chain.

Pros:

1. Directly restores the old point-read mechanism.
2. Bounded and easy to test against existing MVCC/tombstone/TTL cases.
3. Should reduce `point_rows_visited` from 100,200 rows per lookup to
   `source_count * (log rows + versions_for_key)`.

Cons:

1. `read_view_rows_cloned` stays at 100,200 rows per lookup until read-view
   pinning is fixed.
2. The benchmark may still look bad enough to hide the seek improvement.

### Option B: Implement Read-View Pinning First

Make `BranchReadView` pin table sources rather than owning deep clones.

Pros:

1. Removes the row-proportional snapshot cost from points and scans.
2. Restores the old snapshot shape.
3. Helps all read operations, not just point lookups.

Cons:

1. Higher correctness risk because snapshot isolation depends on mutation and
   rotation behavior.
2. Point reads still scan all rows unless point seek lands too.
3. This is a larger production slice.

### Option C: Small Combined Point-Read Slice

Make only enough snapshot/source handling changes to avoid row clones for point
reads and use ordered-key seek for point lookup.

Pros:

1. Most likely to move point throughput materially in one benchmark rerun.
2. Keeps the correction focused on the proven worst path.

Cons:

1. Higher risk of accidentally becoming a partial read-view rearchitecture.
2. Requires careful scoping so scan/history behavior is not changed implicitly.

## Decision

Do not start a broad production correction yet.

The right next step is a narrow PERF-P2 spike that runs old-style point lookup on
storage-next table data and measures the two costs independently:

1. current read-view capture plus current point candidate scan;
2. current read-view capture plus direct ordered-key point seek;
3. synthetic pinned/source-borrowed view plus current point candidate scan;
4. synthetic pinned/source-borrowed view plus direct ordered-key point seek.

The spike should be benchmark-local or test-local. It should not change
production semantics. It should produce the expected movement for each isolated
mechanism before we promote `PERF-T3`, `PERF-T4`, or a deliberately scoped
combined point-read correction.

## PERF-P2 Exit Gates

PERF-P2 must answer:

1. How much time remains after replacing point candidate scans with ordered-key
   seek while keeping current read-view capture?
2. How much time remains after replacing read-view capture with a borrowed/pinned
   model while keeping current point candidate scans?
3. Does combining both changes predict an order-of-magnitude improvement on the
   100K point-read benchmark?
4. Which single production slice has the best risk-adjusted payoff?

Promotion rules:

1. Promote `PERF-T4` if point seek alone produces material improvement and the
   remaining read-view clone cost is tolerable for the next benchmark gate.
2. Promote `PERF-T3` if read-view clone removal alone produces material
   improvement and point scan work is not the immediate limiter.
3. Promote a small combined point-read correction only if neither isolated
   change moves the benchmark enough, but the combined spike does.
4. Stop and collect CPU profiles if the combined spike does not produce an
   order-of-magnitude point-read improvement.

## Non-Goals

1. Do not add a secondary point-read index.
2. Do not change L9 API semantics.
3. Do not change table object format in the point-read spike.
4. Do not alter scan/history behavior as part of the point-read proof.
5. Do not merge read-view pinning, point seek, lazy scan, immutable-table laziness,
   blind commit, and append staging into one performance bundle.

