# L5C Test Plan: Mutable And Frozen Tables

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/l5c-mutable-frozen-tables-implementation-plan.md`

## Goal

Prove that L5C ports the current mutable/frozen in-memory table mechanics into
storage-next without importing product semantics, branch behavior, MVCC
visibility, bloom/cache accelerators, or backend IO.

The suite must fail if L5C:

1. accepts duplicate encoded internal keys;
2. rejects duplicate physical keys at distinct commit versions;
3. returns rows out of V1 encoded internal-key order;
4. performs latest-visible, snapshot, TTL, tombstone-hiding, or branch filtering;
5. mutates a table after freeze;
6. changes row metadata during insert or freeze;
7. misreports empty/nonempty table facts;
8. updates length, facts, or memory accounting after a failed insert;
9. depends on old `crates/storage` key/value/product types;
10. uses backend, filesystem, environment, or upper-layer APIs.

## Test Locations

Use these locations:

1. `crates/storage-next/src/table/tests/mutable.rs` for module-local L5C unit
   tests, with `mod mutable;` from `src/table/tests/mod.rs`.
2. `crates/storage-next/src/testkit/table_runtime.rs` for generated L5C
   scaffold-contract checks.
3. `crates/storage-next/tests/table_runtime_properties.rs` for generated L5C
   property tests behind the `testkit` feature.
4. `crates/storage-next/tests/table_runtime_source_guard.rs` for source-boundary
   scans and executable guard probes.
5. `docs/architecture/implementation-plans/M4/m4-l5-porting-log.md` for the
   old-memtable porting record.

Do not add a separate fuzz target for L5C unless mutable/frozen constructors
begin accepting arbitrary encoded byte streams. Byte fuzzing remains primarily
L3 and L5F.

## Generator Model

The generated model should use a deterministic ordered map keyed by encoded
internal-key bytes:

```text
BTreeMap<Vec<u8>, StorageRow>
```

For every generated script:

1. generate a bounded sequence of storage-next `StorageRow` values;
2. compute each expected key with the V1 internal-key codec or L5B adapter;
3. insert rows into both the model and the mutable table;
4. treat duplicate exact encoded keys as expected insert failures;
5. compare table iteration to model key order;
6. compare table facts to model facts;
7. freeze the mutable table;
8. compare frozen iteration, facts, exact lookup, bounds, and prefix lookup to
   the same model.

Default generated budget:

1. 0 to 128 insert attempts per generated case;
2. user-key bytes capped at 256 by default;
3. value bytes capped at 1024 by default;
4. at least one forced duplicate exact internal-key case per generated script;
5. at least one forced duplicate physical-key, different-version case per
   generated script when row count permits.

Generated tests must terminate under a fixed operation and row budget.

## Required Unit Tests

### 1. Empty Tables

1. New mutable table has length zero.
2. New mutable table reports empty.
3. New mutable table reports zero approximate bytes.
4. New mutable table iteration is empty.
5. New mutable table exact lookup returns missing.
6. New mutable table range lookup returns empty.
7. New mutable table physical-prefix lookup returns empty.
8. Empty mutable table facts have no first key, last key, min commit, or max
   commit.
9. Freezing an empty mutable table produces an empty frozen table.
10. Empty frozen table has the same empty facts as the source mutable table.

### 2. Insert

1. Insert one put row succeeds.
2. Insert one empty-value put row succeeds.
3. Insert one tombstone row succeeds.
4. Insert with storage-owned storage-space id succeeds.
5. Insert with engine-owned storage-space id succeeds.
6. Insert preserves physical key, commit version, commit timestamp, expiry,
   tombstone flag, and value bytes.
7. Insert increases row count by one.
8. Insert increases approximate bytes by at least the inserted row estimate.
9. Insert updates first and last key for the first row.
10. Insert updates min and max commit range.

### 3. Ordering

1. Inserting rows out of order still iterates in encoded internal-key order.
2. Rows with different branch id bytes sort by encoded key.
3. Rows with different space names sort by encoded key.
4. Rows with different storage space ids sort by encoded key.
5. Rows with user-key zero bytes sort by encoded key.
6. Duplicate physical keys at different commit versions are accepted.
7. Duplicate physical keys at different commit versions iterate newest commit
   first for that physical key.
8. Commit timestamps do not affect ordering.
9. Expiry timestamps do not affect ordering.
10. Tombstones do not receive special ordering beyond their encoded key.

### 4. Duplicate Internal Keys

1. Inserting the same `StorageRow` twice returns `DuplicateInternalKey`.
2. Inserting a different row with the same physical key and same commit version
   returns `DuplicateInternalKey`.
3. Duplicate rejection preserves the original row.
4. Duplicate rejection does not change length.
5. Duplicate rejection does not change approximate bytes.
6. Duplicate rejection does not change first key, last key, or commit facts.
7. Duplicate rejection error message is bounded and storage-mechanical.

### 5. Exact Lookup

1. Exact lookup finds first inserted row.
2. Exact lookup finds middle inserted row.
3. Exact lookup finds last inserted row.
4. Exact lookup misses a key before the first row.
5. Exact lookup misses a key after the last row.
6. Exact lookup misses same physical key with absent commit version.
7. Exact lookup returns tombstones as rows.
8. Exact lookup returns expired rows as rows.
9. Exact lookup does not return a latest visible version.

### 6. Key-Bound Lookup

1. Unbounded bounds return all rows in order.
2. Exact bounds return one exact encoded key when present.
3. Exact bounds return empty when absent.
4. Closed range includes both endpoints.
5. Open range excludes both endpoints.
6. Lower-unbounded range returns all rows up to the upper bound.
7. Upper-unbounded range returns all rows after the lower bound.
8. Equal inclusive bounds produce a singleton when the key exists.
9. Equal exclusive bounds produce an empty result.
10. Bounds with gaps match independent vector filtering.
11. Bounds do not skip tombstones.
12. Bounds do not skip expired rows.

### 7. Physical-Prefix Lookup

1. Physical-prefix lookup for a present physical key returns all versions of
   that physical key.
2. Returned versions are in encoded internal-key order.
3. For one physical key, returned versions are newest commit first.
4. Physical-prefix lookup does not return adjacent user keys.
5. Physical-prefix lookup does not return keys from another branch id.
6. Physical-prefix lookup does not return keys from another storage space id.
7. Missing physical prefix returns empty.
8. Tombstone versions are returned.
9. Expired put versions are returned.
10. No helper selects a single latest visible row.

### 8. Freeze

1. Freezing consumes the mutable table and returns a frozen table.
2. Frozen table row count equals pre-freeze mutable row count.
3. Frozen table approximate bytes equal pre-freeze mutable approximate bytes.
4. Frozen table facts equal pre-freeze mutable facts.
5. Frozen table iteration equals pre-freeze mutable iteration.
6. Frozen table exact lookup equals pre-freeze mutable exact lookup.
7. Frozen table key-bound lookup equals pre-freeze mutable key-bound lookup.
8. Frozen table physical-prefix lookup equals pre-freeze mutable prefix lookup.
9. Frozen table has no insert API.
10. Frozen table debug output does not print full value payloads.

The no-insert API rule is mostly structural. If Rust tests cannot express it
without compile-fail machinery, keep it as a review/source-surface check.

### 9. Facts And Memory Accounting

1. Empty facts are valid and distinguish absent ranges from empty byte ranges.
2. Nonempty facts include first key and last key.
3. Nonempty facts include min and max commit version.
4. First key equals first iterator row key.
5. Last key equals last iterator row key.
6. Min commit is the minimum across all rows, not the first row's commit.
7. Max commit is the maximum across all rows, not the last row's commit.
8. Approximate bytes are deterministic.
9. Approximate bytes are monotonic under successful inserts.
10. Approximate bytes do not change after failed duplicate inserts.
11. Approximate bytes are preserved through freeze.
12. Large generated rows do not overflow the configured property-test budget.

### 10. Non-Goals As Tests

Add tests that would fail if L5C accidentally implements higher-layer behavior:

1. A tombstone row is returned by all mechanical iterators.
2. An expired put row is returned by all mechanical iterators.
3. Multiple versions for one physical key are all returned by prefix lookup.
4. No helper accepts a snapshot commit version.
5. No helper mentions branch visibility, inherited layers, or fork versions.
6. No production code imports old `Value`, `Key`, `Namespace`, `TypeTag`, or
   `VersionedValue` types.
7. No production code imports backend or filesystem APIs.

## Required Property Tests

Add generated L5C checks to the existing table-runtime property harness.

For each generated script:

1. build an ordered-map model;
2. insert generated rows into mutable table and model;
3. assert successful insert count equals model unique key count;
4. assert duplicate insert attempts produce duplicate-key errors;
5. assert mutable iteration equals model order;
6. assert mutable facts equal model facts;
7. assert exact lookup for sampled present keys returns model rows;
8. assert exact lookup for sampled absent keys returns missing;
9. assert key-bound results equal independent model filtering;
10. assert physical-prefix results equal independent model filtering;
11. freeze the table;
12. assert frozen iteration, facts, exact lookup, bounds, and prefix lookup equal
    mutable/model results.

Include deterministic edge rows in every generated case where possible:

1. empty value put row;
2. tombstone row;
3. expired-looking put row;
4. duplicate physical key at a lower commit version;
5. duplicate exact internal key;
6. user key with embedded zero bytes;
7. storage-owned storage-space id;
8. engine-owned storage-space id.

## Source Guards

The existing `table_runtime_source_guard` applies to production files under
`crates/storage-next/src/table/`. L5C must keep that guard passing and should
extend executable probes if new vocabulary risks appear.

Forbidden in production L5C code:

1. `crates/storage`;
2. `crate::key_encoding`;
3. old `Key`, `Namespace`, `TypeTag`, `Value`, `VersionedValue`, or
   `StoredValue` surfaces;
4. product DTO vocabulary;
5. `crate::branch`, `crate::commit`, `crate::lifecycle`, or engine crates;
6. `crate::backend`;
7. `std::fs`, `std::path`, or local filesystem APIs;
8. `std::env` and environment reads;
9. process-global cache state.

Allowed:

1. storage-next `StorageRow`;
2. L5B table row/key adapters;
3. standard collections such as `BTreeMap`;
4. ordinary `value` field access on storage-next rows.

## Sensitivity Probes

Before marking L5C complete, temporarily introduce each mutation and confirm a
targeted test fails:

1. store rows in insertion order instead of encoded-key order;
2. silently replace duplicate internal keys;
3. reject duplicate physical keys at distinct commit versions;
4. update approximate bytes before duplicate rejection;
5. compute min/max commit from first/last key instead of all rows;
6. make prefix lookup return only one latest row;
7. filter tombstones from iteration;
8. filter expired rows from iteration;
9. make freeze drop rows or recompute different facts;
10. import an old `crates/storage` key/value type into `mutable.rs`;
11. add a backend or filesystem call to `mutable.rs`.

Record the probes in the porting log.

## Verification Commands

Minimum L5C closeout commands:

1. `cargo test -p strata-storage-next --locked table::tests::mutable`
2. `cargo test -p strata-storage-next --locked --test table_runtime_source_guard`
3. `cargo test -p strata-storage-next --features testkit --locked --test table_runtime_properties`
4. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test table_runtime_properties`
5. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
6. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
7. `cargo fmt --package strata-storage-next --check`
8. `git diff --check`

Run `cargo test -p strata-storage-next --all-features --locked` before closing
the slice if shared table errors, facts, source guards, or testkit routing
changed.

## Exit Gate

L5C test coverage is complete when:

1. empty and nonempty mutable/frozen facts are directly tested;
2. insert behavior is covered for puts, empty values, tombstones, storage-owned
   ids, and engine-owned ids;
3. duplicate exact internal-key rejection is proven non-mutating;
4. duplicate physical keys at distinct commit versions are proven valid;
5. sorted iteration, exact lookup, key-bound lookup, and physical-prefix lookup
   match independent ordered-map models;
6. freeze preserves rows, facts, ordering, and byte accounting;
7. generated tests cover edge rows and randomized operation sequences;
8. source guards prove no old product or upper-layer behavior leaked into L5;
9. tests do not require cursors, immutable builders/readers, cache, compaction,
   backend objects, branch runtime, commit runtime, or lifecycle orchestration.
