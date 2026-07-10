# L5C Implementation Plan: Mutable And Frozen Tables

Status: draft implementation plan

Parent plans:

1. `docs/architecture/implementation-plans/m4-m4t-implementation-plan.md`
2. `docs/architecture/implementation-plans/m4-l5-table-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/l5b-row-key-adapters-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/l5c-mutable-frozen-tables-test-plan.md`

## Goal

Port the current in-memory table mechanics into storage-next L5 without carrying
over product, branch, MVCC visibility, cache, or lifecycle policy.

L5C gives later table-runtime slices a deterministic in-memory source that can:

1. accept storage-next `StorageRow` values through the L5B row/key adapters;
2. reject duplicate encoded internal keys;
3. preserve duplicate physical keys at distinct commit versions;
4. iterate rows in V1 encoded internal-key order;
5. report approximate memory and table facts;
6. freeze a mutable table into a read-only in-memory table;
7. answer exact-key, raw range, and physical-prefix queries mechanically.

L5C is not the cursor layer. It may expose direct read helpers or ordinary
iterators for tests and later builders. L5D owns the reusable cursor traits,
seek state machines, and merge cursor. After L5D lands, `MutableTable` and
`FrozenTable` may grow convenience `cursor()` methods that delegate to the L5D
cursor type without moving cursor ownership back into L5C.

## Inputs

1. `docs/architecture/storage/l5-table-runtime.md`
2. `docs/architecture/implementation-plans/M4/l5b-row-key-adapters-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/l5b-row-key-adapters-test-plan.md`
4. `crates/storage-next/src/table/mutable.rs`
5. `crates/storage-next/src/table/key.rs`
6. `crates/storage-next/src/table/error.rs`
7. `crates/storage-next/src/table/facts.rs`
8. `crates/storage-next/src/table/config.rs`
9. `crates/storage-next/src/row/mod.rs`
10. `crates/storage/src/memtable.rs`

## Existing-Code Source Map

| Current file | Relevant evidence | L5C porting rule |
|---|---|---|
| `crates/storage/src/memtable.rs` | Stores entries ordered by internal key, tracks approximate bytes, length, commit min/max, freeze state, sorted iteration, point/prefix/range scans. | Port the ordered in-memory table mechanics over storage-next `TableRow`. Do not port old `Key`, `Value`, `Namespace`, `TypeTag`, TTL methods, or MVCC latest-selection APIs. |
| `crates/storage/src/key_encoding.rs` | Confirms physical key ascending and commit version descending are the ordering facts the memtable relies on. | Use L5B `TableInternalKeyBytes`; do not import old key encoding. |
| `crates/storage/src/bloom.rs` | Current frozen memtable lazily builds a bloom filter for absent-key probes. | Defer bloom/filter behavior to L5G. L5C frozen tables must be correct without accelerators. |
| `crates/storage-next/src/table/key.rs` | Provides `TableRow`, `TableInternalKeyBytes`, `TablePhysicalKeyBytes`, `TableKeyBounds`, and size accounting. | Use these as the only row/key surface. Do not duplicate key encoding in `mutable.rs`. |
| `crates/storage-next/src/table/facts.rs` | Provides table artifact facts but requires nonempty durable table facts. | Add a separate in-memory facts shape if empty mutable/frozen tables need facts. Do not weaken artifact facts just for mutable tables. |

## Scope

L5C implements:

1. mutable in-memory table type;
2. frozen read-only in-memory table type;
3. insert of put and tombstone storage rows;
4. duplicate encoded internal-key rejection;
5. sorted iteration over all rows;
6. exact encoded-key lookup;
7. physical-key prefix lookup for all versions of a physical key;
8. key-bound filtering over encoded bytes;
9. approximate memory accounting;
10. empty and nonempty in-memory facts;
11. unit, generated, and source-guard coverage;
12. M4-L5 porting-log entry for mutable/frozen table mechanics.

L5C does not implement:

1. MVCC "latest visible at version" lookup;
2. branch ownership or branch-local active/frozen placement;
3. fork-version filtering or inherited-layer rewriting;
4. TTL expiry filtering;
5. tombstone elision or retention policy;
6. bloom filters, block cache, or read accelerators;
7. reusable cursor traits or merge cursors;
8. immutable table building or reading;
9. backend/object IO, filesystem paths, or durable publication;
10. WAL-before-visible discipline or commit application.

## Target Module Shape

Primary implementation target:

```text
crates/storage-next/src/table/mutable.rs
```

Supporting changes:

```text
crates/storage-next/src/table/error.rs
crates/storage-next/src/table/facts.rs
crates/storage-next/src/table/mod.rs
crates/storage-next/src/table/tests/mutable.rs
crates/storage-next/src/testkit/table_runtime.rs
crates/storage-next/tests/table_runtime_properties.rs
crates/storage-next/tests/table_runtime_source_guard.rs
docs/architecture/implementation-plans/M4/m4-l5-porting-log.md
```

Keep every production type `pub(crate)`. L9 owns any future public storage API.

## Data Structure Decision

Use a deterministic ordered map keyed by `TableInternalKeyBytes`.

The old memtable uses `crossbeam_skiplist::SkipMap` for concurrent reads. That
was correct for the current mixed storage engine, but L5C should start with a
smaller table-local structure:

```text
BTreeMap<TableInternalKeyBytes, TableRow>
```

Reasons:

1. L6 owns branch-local active table placement and external write serialization.
2. L7 owns commit application and visibility.
3. L5C tests need deterministic behavior more than lock-free reads.
4. L5D can add cursor abstractions over this ordered source without committing
   to a concurrent data structure.
5. A later concurrency slice can replace the internal map if there is a measured
   need, while preserving the same L5C contract.

Do not add `crossbeam_skiplist` to storage-next for L5C unless implementation
discovers a concrete requirement that cannot be satisfied by a `BTreeMap`.

## Type Surface

Use these names unless implementation discovers a clearer local convention.
Changing names is acceptable if the responsibilities stay the same.

### `MutableTable`

Writable in-memory table over sorted encoded internal keys.

Responsibilities:

1. construct empty;
2. insert `StorageRow` or `TableRow`;
3. reject duplicate encoded internal keys;
4. preserve all row metadata exactly;
5. expose `len()`, `is_empty()`, and `approximate_size_bytes()`;
6. expose `first_key()` and `last_key()` for nonempty tables;
7. expose optional min/max commit range for nonempty tables;
8. expose sorted row iteration;
9. expose exact-key lookup by `TableInternalKeyBytes`;
10. expose key-bound filtering by `TableKeyBounds`;
11. expose physical-prefix filtering by `TablePhysicalKeyBytes`;
12. freeze into `FrozenTable`;
13. optionally expose `cursor()` convenience methods after L5D provides the
    cursor implementation.

Prefer consuming freeze:

```text
MutableTable::freeze(self) -> FrozenTable
```

This makes writes-after-freeze impossible by type, instead of relying on a
runtime frozen flag. The old `freeze()` plus panic-on-write behavior should not
be ported unless an owning layer needs shared mutable handles.

### `FrozenTable`

Read-only in-memory table with the same sorted rows and facts as the source
`MutableTable`.

Responsibilities:

1. expose the same read helpers as `MutableTable`;
2. preserve exact row order and facts from freeze time;
3. be cloneable only if cloning is cheap enough or explicitly documented;
4. have no insert, clear, or mutation methods;
5. remain correct without bloom filters or cache accelerators.

### `TableMemoryFacts`

Add a facts type only if existing facts are not sufficient.

Suggested shape:

```text
TableMemoryFacts {
    row_count: usize,
    approximate_size_bytes: usize,
    first_key: Option<Vec<u8>>,
    last_key: Option<Vec<u8>>,
    min_commit: Option<CommitVersion>,
    max_commit: Option<CommitVersion>,
}
```

Rules:

1. empty mutable/frozen tables are valid and have no key or commit range;
2. nonempty tables must have first key, last key, min commit, and max commit;
3. first key must be less than or equal to last key;
4. min commit must be less than or equal to max commit;
5. facts are runtime facts, not durable table artifact facts.

Do not weaken `TableRuntimeFacts`, which models nonempty durable table artifacts
and correctly rejects zero row count.

### Insert API

Provide a minimal insert surface:

```text
insert_row(row: StorageRow) -> TableRuntimeResult<()>
insert_table_row(row: TableRow) -> TableRuntimeResult<()>
```

If both are too much, keep the `StorageRow` method and use `TableRow::new`
internally.

Insert behavior:

1. constructs or receives a `TableRow`;
2. rejects duplicate encoded internal key before modifying state;
3. increments row count and approximate bytes only after successful insertion;
4. updates min/max commit facts after successful insertion;
5. accepts tombstones as ordinary rows;
6. accepts put rows with empty values;
7. accepts duplicate physical keys at distinct commit versions;
8. never filters expired rows or tombstones.

### Read Helpers

L5C read helpers are mechanical.

Allowed:

1. exact encoded internal-key lookup;
2. sorted full iteration;
3. filtered iteration by `TableKeyBounds`;
4. filtered iteration by exact physical-key prefix;
5. collecting owned rows for tests if borrowing lifetimes make cursor work
   harder before L5D.

Not allowed:

1. `get_latest`;
2. `get_versioned`;
3. `visible_at`;
4. `as_of`;
5. `is_expired`;
6. `hide_tombstone`;
7. branch-aware prefix rewriting.

If a helper returns all versions for a physical key, it must return rows in
encoded internal-key order, which means newest commit version first for that
physical key. It must not choose one version.

## Error Policy

Reuse existing L5 errors where they fit:

1. `DuplicateInternalKey` for duplicate exact encoded key insertion;
2. `InvalidRowOrder` if any constructor accepts pre-sorted row slices and finds
   disorder;
3. `InvalidRange` for impossible in-memory facts or bounds;
4. `InvalidTableState` or similarly named variant if a non-consuming freeze or
   other state transition needs a typed error.

Avoid panics for ordinary table misuse. The old memtable panicked on write after
freeze. L5C should make that unrepresentable by consuming `MutableTable` during
freeze, or return a typed error if a runtime state flag is unavoidable.

Diagnostics must stay bounded and storage-mechanical. Do not print row value
bytes or product vocabulary.

## Implementation Steps

1. Read the old memtable tests and classify each test as:
   - preserved in L5C;
   - deferred to L5D/L5G/L6/L7/L8;
   - obsolete because it uses old product/key/value types.
2. Replace `crates/storage-next/src/table/mutable.rs` placeholder with the
   mutable/frozen table implementation.
3. Add `MutableTable` backed by an ordered map of `TableInternalKeyBytes` to
   `TableRow`.
4. Add `FrozenTable` as the read-only frozen representation.
5. Add runtime facts for empty and nonempty in-memory tables, either in
   `mutable.rs` or `facts.rs` depending on reuse.
6. Add insert methods over `StorageRow` and/or `TableRow`.
7. Add duplicate-key rejection before state mutation.
8. Add exact-key lookup and sorted iteration.
9. Add key-bound and physical-prefix filtering helpers.
10. Add approximate byte accounting using L5B row estimates.
11. Add freeze by consuming `MutableTable`.
12. Re-export only crate-private L5C types from `table/mod.rs`.
13. Add module-local unit tests under `crates/storage-next/src/table/tests/`.
14. Extend `crates/storage-next/src/testkit/table_runtime.rs` with generated
    mutable/frozen table checks.
15. Extend `crates/storage-next/tests/table_runtime_properties.rs` to require
    L5C generated coverage.
16. Keep `table_runtime_source_guard` active over the new production file.
17. Update `docs/architecture/implementation-plans/M4/m4-l5-porting-log.md`
    with an `M4-L5C` entry recording preserved/deferred old memtable behavior.

## Deferred Decisions

1. Raw cursor traits and seek state machines move to L5D.
2. Immutable table building from frozen rows moves to L5E.
3. Immutable reader parity with frozen tables moves to L5F.
4. Bloom filters and cache accelerators move to L5G.
5. Compaction over mutable/frozen/table inputs moves to L5H.
6. Object-backed table access moves to L5I.
7. Branch-local active/frozen ownership moves to L6.
8. Commit visibility and WAL ordering move to L7.
9. Recovery scheduling and retention policy move to L8.

## Verification

Minimum L5C closeout commands:

1. `cargo test -p strata-storage-next --locked table::tests::mutable`
2. `cargo test -p strata-storage-next --locked --test table_runtime_source_guard`
3. `cargo test -p strata-storage-next --features testkit --locked --test table_runtime_properties`
4. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test table_runtime_properties`
5. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
6. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
7. `cargo fmt --package strata-storage-next --check`
8. `git diff --check`

Run the full storage-next all-features suite before closing the slice if L5C
touches shared table errors, facts, or source guards.

## Exit Gate

L5C is complete when:

1. mutable tables accept storage-next rows and preserve all row facts;
2. duplicate exact internal keys are rejected before mutation;
3. duplicate physical keys at distinct commit versions are accepted;
4. sorted iteration exactly matches encoded internal-key order;
5. exact-key, key-bound, and physical-prefix lookups are mechanical and do not
   perform MVCC/latest selection;
6. freeze produces a read-only table with identical rows, facts, and byte
   accounting;
7. empty mutable/frozen tables have explicit valid facts distinct from durable
   nonempty table artifact facts;
8. generated tests compare mutable/frozen behavior to an independent ordered-map
   model;
9. source guards prove no old product key/value, backend, filesystem, or upper
   layer imports entered L5;
10. the porting log records preserved old memtable mechanics and deferred
    bloom/MVCC/concurrency behavior.
