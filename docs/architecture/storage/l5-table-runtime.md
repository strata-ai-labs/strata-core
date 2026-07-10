# L5. Table Runtime

Status: V1 architecture draft

Depends on:

- [L3. Durable Format / Codec](./l3-durable-format-codec.md)
- [L4. Log / Manifest / Snapshot Services](./l4-log-manifest-snapshot-services.md)

Consumed by:

- L6. Branch-Isolated LSM Runtime
- L8. Lifecycle / Recovery / Maintenance, only for table validation, repair, and
  diagnostic facts. L8 must not bypass L6 for branch table-state mutation.

## Purpose

L5 provides Strata's reusable table substrate.

It owns mutable tables, immutable table files, table readers, table builders,
raw sorted cursors, indexes, filters, block cache behavior, and generic table
compaction algorithms.

L5 is intentionally narrower than "the LSM." Strata's actual LSM architecture
is branch-aware: branch-local mutable state, branch-local levels, inherited COW
layers, fork-version gates, and materialization all begin in L6. L5 gives L6
the efficient ordered-table machinery needed to build that branch-isolated LSM
forest.

## Core Decision

Storage-next should treat table runtime as a reusable mechanical layer, not as
the storage engine.

The current `SegmentedStore` mixes table mechanics with branch ownership,
commit application, materialization, manifest publication, recovery facts, and
maintenance control. Storage-next should split that center of gravity:

```text
L5: build/read/compact sorted tables
L6: assemble tables into branch-local LSM state
L7: make committed writes visible
L8: schedule recovery and maintenance
L4: publish durable objects and manifests
```

This keeps the table layer independently testable. It also prevents the
branch-aware COW design from being hidden inside generic table code.

## Responsibilities

L5 owns:

- mutable table data structures
- frozen immutable in-memory table views
- immutable table building
- immutable table reading
- table key comparison over ordered bytes
- stored row bytes and row metadata needed by table algorithms
- table properties and statistics
- data blocks
- index blocks
- bloom or filter blocks
- block cache lookup and eviction policy
- raw point/range/prefix seek support
- raw sorted merge cursors
- generic table compaction algorithms
- output table splitting by size or overlap constraints
- table-level corruption detection
- table-level validation and conformance tests

L5 does not own:

- branch lifecycle
- branch-local level state
- inherited COW layers
- fork-version gates
- key rewriting across branches
- materialization decisions
- commit version allocation
- WAL-before-visible discipline
- conflict validation
- checkpoint scheduling
- recovery orchestration
- retention policy
- quarantine policy
- durable publication mechanics
- object names, filesystem paths, or backend syscalls
- engine data capability semantics
- public maintenance commands

## Layer Boundary

L5 sits above durable bytes and durable publication, but below branch semantics.

```text
L6 branch LSM runtime
  asks L5 to read, build, merge, or compact tables
        |
        v
L5 table runtime
  uses L3 table bytes and L4 table-object services
        |
        v
L4 durable publication
```

L5 may produce table bytes or a table artifact ready for publication. L4 makes
that artifact durable and visible. L6 decides when the published table becomes
reachable from a branch's table manifest.

L5 must not call `std::fs` or backend IO directly in production code. Local
filesystem, browser/cache, and future object-store behavior must be below L4.

## Current Code Reference Map

Current table mechanics are spread across several files. The current crate does
not keep the L5 boundary clean, so this map separates core L5 files from mixed
files that also contain L6, L7, or L8 behavior.

### Core L5 Files

These files are the strongest current evidence for the L5 table runtime:

- `crates/storage/src/memtable.rs`
- `crates/storage/src/segment_builder.rs`
- `crates/storage/src/segment.rs`
- `crates/storage/src/index.rs`
- `crates/storage/src/bloom.rs`
- `crates/storage/src/block_cache.rs`

Current roles:

- `memtable.rs`: mutable table, frozen table behavior, sorted iteration, and
  in-memory bloom support.
- `segment_builder.rs`: immutable table/SST builder, block construction,
  index/filter/property writing, and split-output builder mechanics.
- `segment.rs`: immutable table/SST reader, point lookup, range/prefix
  iteration, block/index/filter reads, and table corruption detection.
- `index.rs`: table index support.
- `bloom.rs`: bloom filter implementation.
- `block_cache.rs`: current process-global table block cache.

### L5/L6 Boundary Files

These files contain table mechanics but also encode branch/version meaning that
belongs to L6:

- `crates/storage/src/key_encoding.rs`
- `crates/storage/src/stored_value.rs`
- `crates/storage/src/ttl.rs`

Current roles:

- `key_encoding.rs`: ordered internal key encoding. The table-key ordering
  mechanics are L5 evidence, but branch ID, storage space id, and commit-version
  meaning belong to L6.
- `stored_value.rs`: stored row/value metadata. L5 stores and decodes row
  metadata; L6 interprets MVCC and visibility meaning.
- `ttl.rs`: row TTL helpers. L5 can carry TTL metadata; L6/L8 decide
  visibility and retention policy.

### Iterator Boundary Files

These files contain reusable cursor mechanics mixed with MVCC and COW behavior:

- `crates/storage/src/merge_iter.rs`
- `crates/storage/src/seekable.rs`

Current roles:

- `merge_iter.rs`: raw sorted merge logic plus `MvccIterator` and
  `RewritingIterator`, which are L6 visibility/COW behavior.
- `seekable.rs`: seekable cursor stack plus MVCC and inherited-layer wrappers.

Storage-next should preserve the raw merge/seek cursor mechanics in L5 and move
MVCC latest selection, fork-version filtering, and branch key rewriting to L6.

### Mixed L5/L6/L7/L8 Files

These files contain important L5 logic, but they should not be copied wholesale:

- `crates/storage/src/compaction.rs`
- `crates/storage/src/segmented/mod.rs`
- `crates/storage/src/segmented/compaction.rs`

Current roles:

- `compaction.rs`: compaction iterator mechanics with policy leakage from
  version retention, snapshot floors, tombstone safety, TTL expiry, and
  primitive-specific exceptions.
- `segmented/mod.rs`: flush/read helpers and table installation mixed with L6
  branch state, L7 apply behavior, and L8 recovery/maintenance helpers.
- `segmented/compaction.rs`: table compaction execution mixed with branch-level
  level selection, branch state mutation, manifest publication, and maintenance
  policy.

Storage-next should extract policy-free table algorithms from these files and
leave branch topology, commit visibility, recovery, retention, and scheduling
to L6-L8.

Current important facts:

- `Memtable` stores ordered internal keys and entries in memory.
- `SegmentBuilder` builds immutable `.sst` files from sorted entries.
- `KVSegment` opens immutable table files and serves point/range reads through
  indexes, bloom filters, and block cache.
- `MergeIterator` merges sorted sources.
- `MvccIterator` and rewriting iterators mix L5 cursor mechanics with L6
  visibility and branch behavior.
- `CompactionIterator` prunes versions and tombstones during compaction.
- `segmented/compaction.rs` mixes table compaction algorithms with branch-level
  level selection and state mutation.

Storage-next should keep the valuable table algorithms but move branch,
visibility, scheduling, and publication decisions to their owning layers.

## Table Key Model

L5 works with ordered table-key bytes.

The current key encoding includes branch ID, space, type tag, user key, and a
descending commit-version suffix. Storage-next renames the type-tag position to
an opaque storage space id. That ordering is a good current design, but its
meaning belongs partly to L6.

Target split:

- L6 constructs row keys with the ordering Strata needs.
- L5 compares row keys as ordered bytes or through a supplied comparator.
- L5 may understand key-prefix boundaries mechanically.
- L5 must not interpret branch IDs, product spaces, storage space ids, or
  branch workflow meaning.

This distinction is important. A branch ID may physically appear in a table key
byte string, but L5 should treat it as part of the ordered key, not as a branch
runtime concept.

## Row Model

L5 stores rows. L6 gives those rows storage meaning.

A table row should minimally carry:

- ordered row key bytes
- value bytes or tombstone marker
- commit timestamp metadata
- expiry timestamp metadata, with zero meaning no expiry
- encoded row-size and checksum facts required by table format

L5 may expose row metadata to callers. It should not decide product visibility.

Examples:

- L5 can report that a row is a tombstone.
- L6 decides whether that tombstone hides an inherited value.
- L5 can report timestamp and TTL metadata.
- L6/L8 decide visibility and compaction policy from those facts.

## Mutable Tables

The mutable table is the write-optimized in-memory ordered structure.

L5 owns:

- insert/update of ordered row keys
- delete/tombstone row insertion
- approximate memory usage
- iteration in sorted order
- freeze into an immutable in-memory view
- raw point/range seek over mutable entries

L5 does not own:

- which branch owns the mutable table
- when a branch rotates active to frozen
- how many frozen tables a branch may hold
- whether a write is committed
- whether a write must be present in WAL first

L6 owns active/frozen table placement per branch. L7 owns commit visibility.

## Immutable Tables

The immutable table is the persistent sorted table object.

L5 owns:

- table builder
- table reader
- table properties
- block layout consumption through L3
- point lookup against a table
- prefix/range iteration
- corruption detection
- optional compression use within table blocks
- optional bloom/filter and index use

L5 should not own:

- table object names
- local paths
- durable publish protocol
- table reachability manifests
- branch-level install state
- deletion or quarantine policy

Current `KVSegment` uses local file handles and path hashes for cache keys.
Storage-next should replace that with object-backed table readers and stable
table object identities supplied by L4/L6. Cache keys should not depend on
local filesystem paths as the only durable identity.

## Cursors And Iterators

L5 should expose raw sorted cursors.

Useful concepts:

- `TableCursor`: seek/next over one immutable table
- `MutableTableCursor`: seek/next over one mutable or frozen table
- `MergeCursor`: sorted merge over multiple raw cursors
- `CompactionCursor`: cursor wrapper used by table compaction

L5 cursors should not own:

- MVCC latest selection
- timestamp-bounded `as_of` behavior
- branch inherited-layer rewriting
- fork-version filtering
- product history presentation

Those are L6 responsibilities built on top of raw L5 cursor mechanics.

## Compaction

L5 owns table compaction algorithms, not compaction policy.

L5 may provide:

- merge sorted table and mutable-table inputs
- preserve table-key ordering
- apply a caller-supplied prune policy
- apply a caller-supplied tombstone policy
- apply a caller-supplied TTL expiry policy
- split output tables by target size
- split output tables by overlap constraints supplied by the caller
- return table output artifacts and table stats

L5 must not decide:

- which branch to compact
- which level to compact
- when compaction should run
- whether a version is safe to prune
- whether an inherited layer must remain reachable
- whether a shared table can be deleted
- whether WAL can be truncated

The current `CompactionIterator` has valuable mechanics, but the prune floor,
snapshot floor, maximum versions, bottommost status, TTL behavior, and event
exceptions show why policy must be supplied from above. L5 can execute
compaction; L6/L8 decide what compaction is allowed to remove.

If primitive-specific retention exceptions remain necessary, they must reach L5
as generic row-retention policy. L5 must not hard-code product type checks such
as "event rows are special."

## Table Publication Flow

The target flow should be:

```text
L6 decides branch state needs a new table, or L8 schedules lifecycle work
through L6 facts
  |
  v
L5 builds table artifact and returns table facts
  |
  v
L4 publishes table object durably
  |
  v
L6 installs table into branch-local reachability state
  |
  v
L4 publishes table/branch manifest supplied by L6
```

L5 should be able to validate the artifact it built before publication. L4
classifies publication failures. L6 decides whether a published table becomes
reachable. L8 handles recovery if a crash leaves an orphan table object, but it
does that through L6 branch-state operations rather than mutating table
reachability directly.

This split is what makes checkpointing at L4 important: table code never needs
to know the backend's publication protocol, and branch code never needs to
reimplement it.

## Block Cache

L5 owns table block caching behavior.

The V1 cache owner is database-local. A database-local cache avoids the
test-isolation and concurrent-open surprises caused by the current
process-global block cache while still allowing engine or embedding code to
choose a per-database memory budget.

Future shared/provider-local caches may be added deliberately, but they must be
explicit resources with deterministic tests and must not be hidden process
globals.

Cache keys should use stable table object identity plus block address, not
local path-only hashes.

L5 must consume a resolved storage runtime budget. It should not auto-detect
host memory or classify the device; the engine-owned resource planner owns that
decision and passes explicit storage limits downward.

## Filters And Indexes

L5 owns table-local read accelerators:

- block indexes
- partitioned indexes
- bloom/filter blocks
- filter indexes
- table properties used for fast rejection

These accelerators must be optional from a correctness perspective. If a bloom
filter or optional sidecar is missing or corrupt, the system should either fall
back to authoritative table bytes or surface a typed table corruption, depending
on whether the accelerator is authoritative.

L3 owns the byte format. L5 owns how readers use the decoded structures.

## TTL And Tombstones

L5 may store and expose TTL and tombstone metadata. It should not own the final
visibility policy.

Rules:

1. Tombstones are rows.
2. TTL metadata is row metadata.
3. L5 compaction may drop expired rows only when supplied a policy proving that
   doing so is safe.
4. L5 compaction may elide tombstones only when supplied a policy proving that
   no lower or inherited data can be incorrectly resurrected.

This is stricter than the current mixed implementation and is necessary because
branch inheritance changes tombstone safety. A child-branch tombstone may be
needed to hide an inherited parent value.

## Failure Model

L5 errors should be typed around table mechanics:

- invalid table key ordering
- duplicate table key where disallowed
- table artifact build failure
- table header decode failure
- table footer decode failure
- table block checksum failure
- malformed data block
- malformed index block
- malformed filter block
- unsupported table format version
- unsupported compression codec
- table object read failure
- table object range read failure
- block cache read/decode failure
- compaction input ordering violation
- compaction output validation failure

L5 should not report product errors. L8 may decide that a corrupt table should
be quarantined. Engine-next may decide how to describe that to a user.

## Testing Requirements

L5 should have direct tests that do not require engine primitives.

Required test families:

1. Mutable table insert/delete/iteration tests.
2. Mutable table freeze tests.
3. Table builder/reader roundtrip tests.
4. Table format golden-vector tests through L3.
5. Table corruption tests for header, footer, block CRC, index, and filter
   failures.
6. Point lookup tests.
7. Prefix and range cursor tests.
8. Raw merge cursor tests.
9. Bloom/filter correctness tests.
10. Block cache hit/miss/eviction tests.
11. Compression roundtrip tests if compression is retained.
12. Compaction property tests over generated sorted inputs.
13. Compaction tombstone and TTL policy tests.
14. Output splitting tests.
15. Object-backed table reader tests through a faulting L4 object service.
16. Fuzz tests for table decode and cursor movement.

The test harness should be reusable. Table tests should run against in-memory
objects and local filesystem-backed objects without changing table algorithms.

## V1 Minimum

The first storage-next L5 implementation needs:

1. Mutable table with sorted iteration.
2. Frozen table view.
3. Immutable table builder.
4. Immutable table reader.
5. Raw point lookup.
6. Raw prefix/range cursor.
7. Raw sorted merge cursor.
8. Block cache with explicit ownership.
9. Bloom/filter and index support matching the chosen table format.
10. Generic table compaction with caller-supplied retention policy.
11. Table object publication through L4, not direct filesystem writes.
12. Direct conformance tests and fuzz targets.

It does not need:

1. Branch-local level ownership.
2. COW inherited layers.
3. Materialization.
4. WAL commit integration.
5. Recovery orchestration.
6. User maintenance commands.
7. New physical table format unless the storage format spec explicitly approves
   one.

## Open Questions

1. Should L5 expose table keys only as opaque byte slices, or as a small
   `TableKey` newtype with comparator hooks?
2. Should row metadata live in the L5 row envelope or in L3-encoded value
   bytes interpreted by L6?
3. Should the first storage-next implementation preserve the current v7 table
   bytes exactly, or introduce a new table format version as part of the formal
   storage spec?
4. Should bloom filters remain table-internal, or can optional sidecars exist
   for future object-store modes?
5. How much of current partitioned index behavior should be retained in V1?
6. Does L5 need an async-capable reader shape for browser/object backends, or
   should async be hidden below L4 for the first implementation?
7. Which table stats become stable facts consumed by L6/L8?

## Next Step

After L5 and L6, [L7. Commit Runtime](./l7-commit-runtime.md) defines how
validated commit batches receive versions, enter L6 branch state, satisfy
read-your-writes and atomicity rules, and expose storage-local commit units
without preserving public transaction sessions.
