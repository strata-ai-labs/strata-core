# L5D Implementation Plan: Raw Cursors And Merge Cursor

Status: draft implementation plan

Parent plans:

1. `docs/architecture/implementation-plans/m4-m4t-implementation-plan.md`
2. `docs/architecture/implementation-plans/m4-l5-table-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/l5b-row-key-adapters-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/l5c-mutable-frozen-tables-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/l5d-raw-cursors-merge-cursor-test-plan.md`

## Goal

Port the raw cursor mechanics from the current storage engine into
storage-next L5 without carrying over MVCC latest selection, branch inheritance,
fork-version filtering, or product-level read semantics.

L5D gives later table-runtime slices a reusable cursor surface that can:

1. seek to the first row whose encoded internal key is greater than or equal to
   a target key;
2. advance through one sorted source in encoded internal-key order;
3. restrict cursor output with raw encoded-key bounds or physical-key prefixes;
4. expose raw cursors over `MutableTable` and `FrozenTable`;
5. merge several sorted raw cursors into one deterministic sorted stream;
6. preserve tombstones, expired rows, duplicate physical keys, and duplicate
   exact internal keys across sources;
7. provide the cursor contract that L5F immutable readers and L5H compaction
   can implement later.

L5D is still not the user read path. It must not select the latest visible row,
hide tombstones, evaluate TTL, rewrite branch ids, or apply branch/fork
visibility rules.

## Inputs

1. `docs/architecture/storage/l5-table-runtime.md`
2. `docs/architecture/implementation-plans/m4-l5-table-runtime-test-plan.md`
3. `docs/architecture/implementation-plans/M4/l5b-row-key-adapters-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/l5b-row-key-adapters-test-plan.md`
5. `docs/architecture/implementation-plans/M4/l5c-mutable-frozen-tables-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/l5c-mutable-frozen-tables-test-plan.md`
7. `crates/storage-next/src/table/key.rs`
8. `crates/storage-next/src/table/mutable.rs`
9. `crates/storage-next/src/table/cursor.rs`
10. `crates/storage/src/merge_iter.rs`
11. `crates/storage/src/seekable.rs`
12. `crates/storage/src/memtable.rs`
13. `crates/storage/src/segment.rs`

## Existing-Code Source Map

| Current file | Relevant evidence | L5D porting rule |
|---|---|---|
| `crates/storage/src/merge_iter.rs` | `MergeIterator` performs a raw k-way merge over sorted sources, uses a linear path for up to four sources, uses a heap above that threshold, and preserves all rows. | Port the raw merge mechanics and deterministic source-index tie break. Do not port `MvccIterator`. |
| `crates/storage/src/seekable.rs` | `SeekableIterator`, `MemtableSeekableIter`, and `MergeSeekableIter` define persistent seek/advance mechanics. | Port raw seek and merge cursor mechanics only. Do not port `MvccSeekableIter` or `RewritingSeekableIter`. |
| `crates/storage/src/memtable.rs` | Demonstrates sorted in-memory source behavior. | Use as behavioral evidence for cursor movement over `MutableTable` and `FrozenTable`; do not import old row/value types. |
| `crates/storage/src/segment.rs` | Demonstrates seekable immutable-table reader behavior. | Use as evidence for the trait shape L5F will implement later. Do not implement immutable reader behavior in L5D. |
| `crates/storage/src/segmented/mod.rs` | Mixed read-path and scan behavior. | Treat as regression evidence only. It mixes L5, L6, L7, and L8 concerns and must not be ported wholesale. |

## Scope

L5D implements:

1. the raw table-cursor trait or equivalent cursor contract;
2. cursor state semantics for valid, exhausted, seeked, and advanced states;
3. mutable-table cursor over `MutableTable`;
4. frozen-table cursor over `FrozenTable`;
5. optional bounds wrapper for exact, range, and physical-prefix filtering;
6. k-way merge cursor over raw table cursors;
7. deterministic source-index tie breaking for equal encoded internal keys;
8. linear merge path and heap merge path;
9. generated model tests for cursor movement and merge behavior;
10. source-guard coverage for the new cursor module;
11. M4-L5 porting-log entries for raw cursor mechanics.

L5D does not implement:

1. immutable table building;
2. immutable table reading;
3. object-backed table access;
4. block cache behavior;
5. bloom/filter accelerators;
6. compaction policies or table artifact output;
7. MVCC latest-visible selection;
8. snapshot/as-of reads;
9. TTL expiry filtering;
10. tombstone hiding or elision;
11. branch inheritance, branch id rewriting, or fork-version gates;
12. commit version allocation;
13. WAL, manifest, lifecycle, recovery, or retention behavior;
14. product payload interpretation.

## Target Module Shape

Primary implementation target:

```text
crates/storage-next/src/table/cursor.rs
```

Supporting changes:

```text
crates/storage-next/src/table/mod.rs
crates/storage-next/src/table/tests/cursor.rs
crates/storage-next/src/testkit/table_runtime.rs
crates/storage-next/tests/table_runtime_properties.rs
crates/storage-next/tests/table_runtime_source_guard.rs
docs/architecture/implementation-plans/M4/m4-l5-porting-log.md
```

If `cursor.rs` grows past the local engineering threshold during
implementation, split it into a private `table/cursor/` module before adding
more behavior. A likely split is:

```text
cursor/mod.rs
cursor/memory.rs
cursor/bounds.rs
cursor/merge.rs
```

Keep every production type `pub(crate)`. L9 owns any future public storage API.

## Cursor Contract

Use these names unless implementation discovers a clearer local convention.
Changing names is acceptable if the responsibilities stay the same.

### `TableCursor`

Raw seekable cursor over rows sorted by `TableInternalKeyBytes`.

Suggested shape:

```text
seek(&mut self, target: &TableInternalKeyBytes) -> TableRuntimeResult<()>
seek_to_first(&mut self) -> TableRuntimeResult<()>
advance(&mut self) -> TableRuntimeResult<()>
current(&self) -> Option<&TableRow>
current_key(&self) -> Option<&TableInternalKeyBytes>
is_valid(&self) -> bool
```

Rules:

1. `seek(target)` positions at the first row with key `>= target`.
2. `seek_to_first()` positions at the first row, or exhaustion for an empty
   source.
3. `advance()` moves past the current row, or remains exhausted if already
   exhausted.
4. `current()` returns `None` while exhausted and must not panic.
5. `current_key()` is a convenience over `current().map(TableRow::key)`.
6. A cursor emits rows in encoded internal-key order.
7. A cursor emits tombstones, expired rows, and all versions mechanically.
8. A cursor does not allocate or decode product payloads during ordinary
   movement.
9. A cursor over an immutable source in L5F can cache the current decoded row
   internally and still satisfy this trait.

Prefer this valid/exhausted contract over the old `current_key().unwrap()`
style. Panics are acceptable only for impossible internal invariants, not for
ordinary cursor movement.

### `MemoryTableCursor`

Raw cursor over `MutableTable` or `FrozenTable`.

The first implementation may store a borrowed ordered slice/vector of
`&TableRow` values collected from the table at cursor construction time. That
keeps L5D simple and avoids cloning row payloads. A future optimization may
replace this with a direct `BTreeMap` range cursor if profiling justifies it.

Rules:

1. cursor lifetime is tied to the borrowed table;
2. a mutable table cannot be mutated while a cursor borrows it;
3. row payload bytes are not cloned during normal movement;
4. exact, gap, before-first, and after-last seeks use binary search over
   encoded internal keys;
5. mutable and frozen cursors over the same rows produce identical output.

### `BoundedTableCursor`

Optional wrapper over any raw cursor that enforces `TableKeyBounds`.

Responsibilities:

1. choose the correct initial seek target from the lower bound;
2. enforce inclusive and exclusive lower bounds;
3. stop at inclusive and exclusive upper bounds;
4. stop at the physical-prefix boundary for prefix bounds;
5. preserve rows without visibility filtering;
6. work over mutable, frozen, future immutable, and merge cursors.

If implementing a wrapper adds unnecessary complexity, equivalent bounded
constructors on the memory cursor are acceptable for L5D, but the behavior
must remain source-independent enough for L5F and L5H to reuse.

### `MergeTableCursor`

Raw k-way merge over child `TableCursor` values.

Rules:

1. child cursors are already sorted individually;
2. merging zero children yields an exhausted cursor;
3. merging one child is identity;
4. merging several children yields global encoded-key order;
5. equal encoded internal keys across children are all emitted;
6. equal-key ties are resolved by ascending source index;
7. source-index tie break is deterministic only, not a visibility priority;
8. tombstones and expired rows are preserved;
9. no MVCC deduplication is performed;
10. no branch id rewriting is performed.

The merge cursor should not advance a selected child before the caller has had
a chance to read the selected row. A safe state machine is:

1. `seek` or `seek_to_first` positions every child;
2. merge selection records the current winning child index;
3. `current()` borrows from that child;
4. `advance()` advances the previously selected child, then selects the next
   minimum child.

This avoids cloning full rows just to keep a stable current item.

## Merge Algorithm

Preserve the useful old split:

```text
MERGE_HEAP_THRESHOLD = 4
```

The implementation may expose this threshold as a `pub(crate)` constant for
tests, with a compile-time assertion that it stays at least two. It may also
expose a `CursorMergePath` enum so tests can assert whether a merge used the
empty, single, linear, or heap path without inspecting private cursor state.

1. Use a direct empty/single-source path for zero or one child.
2. Use a linear scan path for two to four child cursors.
3. Use a binary heap path for five or more child cursors.

The heap entry should cache only the child's current encoded key and source
index. It must not become the authoritative row. The child cursor remains the
source of the current row.

Comparison order:

```text
(encoded_internal_key ASC, source_index ASC)
```

Do not add commit-version, timestamp, tombstone, branch, or source-age
semantics beyond the encoded key bytes. Commit-version ordering is already part
of the encoded internal key.

## Bounds Semantics

L5D uses `TableKeyBounds` from L5B.

Rules:

1. unbounded cursor returns all rows;
2. exact bounds return the one matching encoded internal key, if present;
3. closed ranges include both endpoints;
4. open ranges exclude both endpoints;
5. lower-unbounded ranges start at the first source row;
6. upper-unbounded ranges continue until source exhaustion;
7. equal inclusive bounds may return a singleton;
8. equal exclusive bounds return empty;
9. prefix bounds compare raw encoded physical-key prefix bytes;
10. no bound interprets branch or product meaning.

If an existing `TableKeyBounds` constructor already rejects malformed ranges,
the cursor should not duplicate that validation except where needed to protect
new constructors.

## Error Policy

Use existing L5 errors where possible:

1. `InvalidRange` for malformed cursor bounds if construction accepts raw
   bounds.
2. `InvalidRowOrder` for test/model cursor constructors that accept unsorted
   raw rows.
3. `DuplicateInternalKey` for source constructors that require per-source
   uniqueness and receive duplicates.
4. `SourceRead` reserved for future L5F immutable/object-backed cursors.

Ordinary empty, missing, exhausted, or after-last movement is not an error.
Represent it as `current() == None`.

## Implementation Steps

1. Read the old `MergeIterator` and `SeekableIterator` tests and classify each
   behavior as raw L5D, deferred L6/L7, or obsolete.
2. Replace the `table/cursor.rs` placeholder with the raw cursor contract.
3. Add a memory cursor over a borrowed ordered row set from `MutableTable` and
   `FrozenTable`.
4. Add `seek_to_first`, `seek`, `advance`, `current`, and `current_key`
   behavior with no panics for ordinary exhausted states.
5. Add bounded cursor behavior over `TableKeyBounds`, either as a wrapper or
   source-specific constructor.
6. Add `MergeTableCursor` for zero, one, linear, and heap source counts.
7. Implement deterministic equal-key source-index tie breaking.
8. Ensure merge seek re-seeks children in place instead of rebuilding child
   sources.
9. Ensure merge `advance` advances only the previously selected child.
10. Re-export only crate-private cursor types from `table/mod.rs`.
11. Add module-local tests under `crates/storage-next/src/table/tests/cursor.rs`.
12. Extend the table-runtime testkit with generated cursor and merge model
    checks.
13. Extend `table_runtime_properties` so generated L5D coverage is mandatory
    under the `testkit` feature.
14. Strengthen `table_runtime_source_guard` if new cursor vocabulary risks
    importing old MVCC or branch rewrite terms.
15. Update `docs/architecture/implementation-plans/M4/m4-l5-porting-log.md`
    with an `M4-L5D` entry recording preserved and deferred cursor behavior.

## Non-Goals As Implementation Guards

Do not add methods or fields named after higher-layer behavior, including:

1. `latest`;
2. `visible`;
3. `snapshot`;
4. `as_of`;
5. `fork`;
6. `inherited`;
7. `rewrite`;
8. `ttl_filter`;
9. `live_only`.

Tests may mention these names in source-guard probes. Production L5D code
should not.

## Deferred Decisions

1. Immutable table reader cursors move to L5F.
2. Cache-aware cursor movement moves to L5G.
3. Compaction-specific cursor policy moves to L5H.
4. Object-backed immutable source cursors move to L5I.
5. MVCC latest-visible selection moves to L6/L7.
6. Branch inheritance and key rewriting move to L6.
7. Cursor fuzzing may be finalized in L5J if the fuzz harness would distract
   from the L5D API landing, but the generated operation model must land in
   L5D.

## Verification

Minimum L5D closeout commands:

1. `cargo test -p strata-storage-next --locked table::tests::cursor`
2. `cargo test -p strata-storage-next --locked --test table_runtime_source_guard`
3. `cargo test -p strata-storage-next --features testkit --locked --test table_runtime_properties`
4. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test table_runtime_properties`
5. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
6. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
7. `cargo fmt --package strata-storage-next --check`
8. `git diff --check`

Run the full storage-next all-features suite before closing the slice if L5D
touches shared table errors, key bounds, testkit generators, or source guards.

## Exit Gate

L5D is complete when:

1. mutable and frozen tables expose raw cursors in encoded internal-key order;
2. seek and advance behavior matches the independent sorted-vector model;
3. exact, range, and physical-prefix bounds are enforced mechanically;
4. merge cursor output is sorted, deterministic, and preserves all raw rows;
5. equal encoded internal keys across sources are emitted in source-index order;
6. linear and heap merge paths are both covered;
7. generated tests compare cursor and merge behavior to independent models;
8. tombstones, expired rows, and duplicate physical-key versions are not hidden;
9. no MVCC, branch rewrite, lifecycle, backend, filesystem, or product
   semantics enter production L5D code;
10. the porting log records preserved raw cursor mechanics and deferred
    MVCC/branch behavior.
