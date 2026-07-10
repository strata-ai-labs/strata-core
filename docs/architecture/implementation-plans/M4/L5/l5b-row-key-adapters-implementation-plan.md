# L5B Implementation Plan: Row And Key Adapters

Status: draft implementation plan

Parent plans:

1. `docs/architecture/implementation-plans/m4-m4t-implementation-plan.md`
2. `docs/architecture/implementation-plans/m4-l5-table-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/l5a-table-runtime-scaffold-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/l5b-row-key-adapters-test-plan.md`

## Goal

Define the storage-next L5 row/key adapter surface used by later table runtime
slices.

L5B gives table code a small, storage-native way to:

1. turn a `StorageRow` into ordered internal-key bytes;
2. compare rows by encoded internal-key bytes;
3. validate sorted unique row sequences;
4. express table key bounds without cursor behavior;
5. estimate row/key memory size for later mutable-table accounting;
6. preserve row metadata without interpreting product, branch, or visibility
   meaning.

L5B is still not the mutable table. It is the adapter layer that L5C, L5D,
L5E, L5F, and L5H build on.

## Inputs

1. `docs/architecture/storage/l5-table-runtime.md`
2. `docs/spec/strata-storage-format-v1.md`, especially sections 15, 16, and
   17.
3. `crates/storage-next/src/row/mod.rs`
4. `crates/storage-next/src/format/key.rs`
5. `crates/storage-next/src/format/storage_row.rs`
6. `crates/storage-next/src/format/table/data.rs`
7. `crates/storage-next/src/table/key.rs`
8. `crates/storage-next/src/table/error.rs`
9. `crates/storage-next/src/table/facts.rs`
10. `crates/storage/src/key_encoding.rs`
11. `crates/storage/src/memtable.rs`

## Existing-Code Source Map

| Current file | Relevant evidence | L5B porting rule |
|---|---|---|
| `crates/storage/src/key_encoding.rs` | Internal key ordering: physical key ascending, commit version descending. Escaped user-key bytes. Prefix helpers. | Port the byte-ordering mechanics only. Do not port old `Key`, `TypeTag`, or product-space semantics. |
| `crates/storage/src/memtable.rs` | Sorted map over internal keys, duplicate-key behavior, point/range seek evidence. | Use only as evidence for ordering and duplicate rules. Mutable-table behavior is L5C. |
| `crates/storage-next/src/row/mod.rs` | Storage-native `PhysicalKey`, `InternalKey`, `StorageRow`, `StorageSpaceId`. | This is the row model L5B adapts. Do not invent a second durable row type. |
| `crates/storage-next/src/format/key.rs` | V1 physical/internal key codec and ordering suffix. | Promote the narrow crate-private key codec surface L5 needs. |
| `crates/storage-next/src/format/table/data.rs` | M3G data entries already validate sorted unique encoded internal keys. | Reuse as a consistency oracle, not as the L5 runtime adapter API. |

## Scope

L5B implements:

1. table-local key byte wrapper types;
2. table-local row adapter types over `StorageRow`;
3. strict encoded-key ordering helpers;
4. duplicate internal-key detection;
5. key-bound and prefix-bound construction helpers for future cursors;
6. approximate size accounting helpers;
7. unit, property, and source-guard tests;
8. M4-L5 porting-log entries for row/key mechanics.

L5B does not implement:

1. mutable-table insertion or storage;
2. frozen table snapshots;
3. cursor movement;
4. merge cursors;
5. immutable table building;
6. immutable table reading;
7. cache behavior;
8. compaction;
9. object-backed reads;
10. branch visibility, MVCC latest selection, TTL interpretation, or
    tombstone retention decisions.

## Target Module Shape

Primary implementation target:

```text
crates/storage-next/src/table/key.rs
```

Supporting changes:

```text
crates/storage-next/src/format/mod.rs
crates/storage-next/src/table/error.rs
crates/storage-next/src/table/mod.rs
crates/storage-next/src/table/tests/
crates/storage-next/src/testkit/table_runtime.rs
crates/storage-next/tests/table_runtime_properties.rs
crates/storage-next/tests/table_runtime_source_guard.rs
docs/architecture/implementation-plans/M4/m4-l5-porting-log.md
```

Keep all production types `pub(crate)`. The table module remains internal until
the L9 API boundary deliberately exposes storage behavior.

## Type Surface

Use these names unless implementation discovers a clearer local convention.
Changing the names is acceptable if the responsibilities stay the same.

### `TableInternalKeyBytes`

Owned, validated encoded V1 internal-key bytes.

Responsibilities:

1. construct from `StorageRow`;
2. construct from `InternalKey`;
3. optionally construct from raw encoded bytes by decoding and re-encoding to
   prove canonical V1 layout;
4. expose `as_slice()` for comparisons and downstream L3 table codecs;
5. implement byte-order comparison through ordinary slice ordering;
6. expose `commit_version()` and `physical_key()` only when needed for
   diagnostics, never for visibility decisions.

Construction from a `StorageRow` must use:

```text
InternalKey::new(row.physical_key().clone(), row.commit_version())
```

then V1 `encode_internal_key`.

### `TablePhysicalKeyBytes`

Owned, validated encoded V1 physical-key bytes.

Responsibilities:

1. construct from `PhysicalKey`;
2. expose exact physical-key prefix bytes for all versions of the same physical
   key;
3. support future raw prefix/range cursors without decoding product meaning.

This type treats branch id, storage space id, and user key as opaque ordered
bytes.

### `TableRow`

Owned row adapter containing:

1. the original `StorageRow`;
2. its `TableInternalKeyBytes`;
3. an approximate heap-size estimate.

Responsibilities:

1. preserve put rows, empty-value put rows, tombstones, timestamps, expiry, and
   storage space ids exactly;
2. expose `row()`, `key()`, `encoded_key()`, `is_tombstone()`,
   `commit_version()`, and size facts;
3. avoid visibility helpers such as "is live", "is expired", "latest", or
   "visible at".

L5B should prefer owned adapters first. Borrowing adapters such as
`TableRowRef<'a>` may be added only if they avoid immediate cloning without
making later slices harder to reason about.

### `TableKeyBounds`

Validated key-bound descriptor for future point, prefix, and range cursors.

Responsibilities:

1. represent unbounded lower and upper bounds;
2. represent inclusive and exclusive encoded-key bounds;
3. reject malformed ranges where the lower bound is strictly greater than the
   upper bound;
4. allow ranges that are well-formed but empty by bound exclusivity;
5. provide `contains_key(&TableInternalKeyBytes) -> bool`;
6. provide constructors for exact key, closed range, open range, and prefix
   range where practical.

`TableKeyBound` is the lower/upper bound enum used by `TableKeyBounds::range`
and should remain a mechanical encoded-key bound, not a visibility or product
predicate.

`TableKeyRange` in `facts.rs` is a table-fact range with concrete first and
last keys. `TableKeyBounds` is a query/bounds helper. Do not merge them unless
the resulting API keeps both meanings clear.

### Row Sequence Helpers

Add helpers for later slices:

1. `validate_strictly_sorted_unique_rows(&[TableRow])`;
2. `validate_strictly_sorted_unique_keys<I>(keys: I)`;
3. `sort_table_rows_by_key(&mut [TableRow])`, if tests and later builders need
   a canonical pre-sort helper;
4. `first_table_key` and `last_table_key` extraction helpers for nonempty
   sorted slices.

Duplicate physical keys at distinct commit versions must be accepted. Duplicate
encoded internal keys must be rejected by any helper that promises uniqueness.

### Size Accounting

Add a documented approximate size helper:

```text
approximate_row_size = encoded_internal_key_len
                     + value_len
                     + fixed row overhead
                     + adapter overhead
```

The exact constants may be conservative. The important invariants are:

1. size is deterministic;
2. size is nonzero for all rows;
3. size is monotonic when value or key bytes grow;
4. tombstones still account for key and metadata bytes;
5. size is explicitly approximate and not a durable byte format fact.

## Error Policy

Reuse L5A error vocabulary when possible:

1. `InvalidRowOrder` for unsorted rows;
2. `DuplicateInternalKey` for exact encoded-key duplicates;
3. `InvalidRange` for malformed bounds;
4. `DecodeFormat` or a more precise key-format wrapper for invalid encoded key
   bytes.

Error messages should include enough key context for diagnostics without
printing unbounded row payloads.

## Implementation Steps

1. Promote the narrow V1 key codec functions L5 needs from `format/mod.rs`.
   Prefer crate-private re-exports such as `encode_internal_key`,
   `decode_internal_key`, and `encode_physical_key`. Do not expose old
   `crates/storage` key APIs.
2. Replace the placeholder `table/key.rs` with the L5B adapter types.
3. Add `TableInternalKeyBytes` constructors and ordering implementations.
4. Add `TablePhysicalKeyBytes` and exact physical-key prefix helpers.
5. Add `TableRow` and row metadata accessors.
6. Add strict sorted-unique sequence validation helpers.
7. Add `TableKeyBounds` and `contains_key` semantics.
8. Add approximate size accounting and document the constants.
9. Re-export only the crate-private types that later `table` submodules need
   from `table/mod.rs`.
10. Add module-local unit tests under `crates/storage-next/src/table/tests/`.
11. Extend `crates/storage-next/src/testkit/table_runtime.rs` with generated
    row/key adapter checks.
12. Extend `crates/storage-next/tests/table_runtime_properties.rs` so L5B has
    generated coverage behind the existing `testkit` feature.
13. Extend `table_runtime_source_guard` to reject old key/value imports and any
    use of `crates/storage` mechanics in production L5 code.
14. Update `docs/architecture/implementation-plans/M4/m4-l5-porting-log.md`
    with a new `M4-L5B` section.

## Deferred Decisions

1. Whether mutable tables store `TableRow` directly or split key and row bytes
   for cache locality is L5C.
2. Whether immutable builders consume `Vec<TableRow>` or an iterator of row
   references is L5E.
3. Whether reader cursors expose borrowed rows or owned rows is L5F.
4. Whether prefix bounds need a richer physical-key builder from L6 is L5D/L6.
5. Whether size accounting feeds a precise memory budget or only an
   approximation is L5C/L5G.

## Verification

Minimum L5B closeout commands:

1. `cargo test -p strata-storage-next --locked table::tests::`
2. `cargo test -p strata-storage-next --locked --test table_runtime_source_guard`
3. `cargo test -p strata-storage-next --features testkit --locked --test table_runtime_properties`
4. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test table_runtime_properties`
5. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
6. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
7. `cargo fmt --package strata-storage-next --check`
8. `git diff --check`

## Exit Gate

L5B is complete when:

1. `StorageRow` can be adapted into canonical encoded internal-key bytes;
2. row ordering matches V1 internal-key ordering;
3. duplicate internal keys are rejected where uniqueness is required;
4. duplicate physical keys at distinct commit versions are accepted;
5. key bounds and prefix helpers operate only on encoded bytes;
6. row metadata is preserved but not interpreted;
7. size accounting is deterministic and documented;
8. generated tests cover ordering, bounds, duplicates, storage-space opacity,
   and value/tombstone cases;
9. source guards prove L5B did not import old product key/value types or upper
   storage layers;
10. the porting log records which old key mechanics were reused and which old
    product semantics were left behind.
