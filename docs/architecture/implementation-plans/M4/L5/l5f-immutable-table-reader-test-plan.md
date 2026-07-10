# L5F Test Plan: Immutable Table Reader

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/l5f-immutable-table-reader-implementation-plan.md`

## Goal

Prove that L5F reads M3G immutable table artifacts through L5 reader APIs
without importing branch, visibility, durable publication, filesystem/backend,
old segment, cache-global, or product-payload behavior.

The suite must fail if L5F:

1. accepts corrupt table bytes;
2. trusts footer offsets before full-object checksum validation;
3. reads rows out of encoded internal-key order;
4. returns latest-visible rows instead of exact/raw rows;
5. hides tombstones;
6. drops expired-looking rows;
7. collapses duplicate physical-key versions;
8. rewrites branch ids or crosses storage-space prefixes incorrectly;
9. applies snapshot/as-of/fork-version/TTL policy;
10. decodes old `STRAKV` bytes;
11. uses filesystem paths, backend APIs, object names, service APIs, or process
    global caches;
12. behaves differently for byte-backed and range-backed sources.

## Test Locations

Use these locations:

1. `crates/storage-next/src/table/tests/reader.rs` for module-local L5F unit
   tests.
2. `crates/storage-next/src/testkit/table_runtime.rs` for generated reader
   model checks.
3. `crates/storage-next/tests/table_runtime_properties.rs` for generated L5F
   property tests behind the `testkit` feature.
4. `crates/storage-next/tests/table_runtime_source_guard.rs` for source-boundary
   scans and executable guard probes.
5. `docs/architecture/implementation-plans/M4/m4-l5-porting-log.md` for the
   old immutable-reader porting record.

Tests should build valid artifacts through L5E unless they are specifically
corrupting bytes. Do not use old `crates/storage` code as an oracle at runtime.

## Reference Model

The reader model starts from a sorted vector of `TableRow` values and the M3G
bytes produced by L5E.

For each artifact:

```text
model_rows = input rows in encoded internal-key order
model_facts = facts from decoded M3G header/properties
point(key) = first model row whose encoded key == key
range(bounds) = model_rows filtered by TableKeyBounds::contains_key
prefix(prefix_bytes) = model_rows where encoded key starts with prefix_bytes
seek(target) = first model row where encoded key >= target
```

The model intentionally preserves every row. It does not collapse versions,
hide tombstones, filter TTL, interpret branch ids, or decode product values.

Byte-backed and range-backed readers must match the same model.

## Required Unit Tests

### 1. Reader Construction And Facts

1. Byte-backed reader opens a one-row uncompressed artifact.
2. Byte-backed reader opens a multi-row, multi-block uncompressed artifact.
3. Byte-backed reader opens a zstd artifact.
4. Range-backed reader opens the same artifacts.
5. Reader built from `TableRuntimeConfig::reader()` uses the supplied config.
6. Reader facts use caller-supplied `TableIdentity`.
7. Reader facts row count equals decoded properties row count.
8. Reader facts data-block count equals decoded properties data-block count.
9. Reader facts key range equals decoded properties min/max keys.
10. Reader facts commit range equals decoded properties commit min/max.
11. Reader byte count equals source byte count.
12. Invalid table identity is rejected before successful open.
13. Empty byte input is rejected.
14. Header-only or footer-only byte input is rejected.
15. A valid table with one row has equal first/last keys and equal commit
    min/max.

### 2. V1 Validation And Corruption Routing

1. Invalid table magic is rejected.
2. Old `STRAKV` bytes are rejected.
3. Future table version is rejected.
4. Pre-V1 table version is rejected.
5. Invalid header size is rejected.
6. Nonzero header flags are rejected.
7. Nonzero header reserved bytes are rejected.
8. Footer CRC mismatch is rejected before footer offsets are trusted.
9. Invalid footer magic is rejected.
10. Nonzero footer reserved bytes are rejected.
11. Nonzero filter offset/length is rejected until a V1 filter subformat exists.
12. Index offset before header is rejected.
13. Index/properties offset overflow is rejected.
14. Index/properties range past footer is rejected.
15. Hidden bytes between data blocks, index, properties, or footer are rejected.
16. Data block checksum mismatch is rejected.
17. Index block checksum mismatch is rejected.
18. Properties block checksum mismatch is rejected.
19. Unknown block type is rejected.
20. Unknown compression codec is rejected.
21. Nonzero block flags are rejected.
22. Truncated block frame is rejected.
23. Truncated data entry key or row is rejected.
24. Data block with unsorted entries is rejected.
25. Data block with duplicate internal keys is rejected.
26. Index entry count mismatch is rejected.
27. Index entry range that does not match data block facts is rejected.
28. Properties that do not match header/data blocks are rejected.
29. Wrapped decode errors expose their `FormatError` through `source()`.
30. Error display is bounded and uses L5 table-runtime vocabulary.

### 3. Source Read Behavior

1. Range-backed open reads only through the table source abstraction.
2. A source whose `byte_count` is too small is rejected.
3. A source read error during open returns `SourceRead`.
4. A short read during open returns `SourceRead`.
5. An offset+length overflow is rejected before calling the source.
6. A read past byte count is rejected before calling the source.
7. Source read errors during data-block lookup are deferred until lazy
   candidate-block reads exist.
8. Source read errors during cursor block transition are deferred until lazy
   candidate-block reads exist.
9. Byte-backed and range-backed open produce identical facts.
10. Byte-backed and range-backed reads produce identical rows.
11. Full-object checksum validation is performed before footer offsets guide
    subsequent reads.
12. Instrumented range source proves no `std::fs`, path, backend, or service
    API is involved.

### 4. Exact Internal-Key Lookup

1. Lookup of first row returns that row.
2. Lookup of middle row returns that row.
3. Lookup of last row returns that row.
4. Lookup in a one-row table returns the row.
5. Lookup before first key returns `None`.
6. Lookup after last key returns `None`.
7. Lookup in a gap between blocks returns `None`.
8. Lookup for an absent commit version of an existing physical key returns
   `None`.
9. Lookup for each version of a duplicate physical key returns the exact
   matching version.
10. Lookup of a tombstone returns the tombstone row.
11. Lookup of an expired-looking row returns the row.
12. Lookup of an empty-value row returns the row.
13. Lookup of a large-value row returns the row.
14. Lookup of user keys with embedded zero bytes works.
15. Lookup does not inspect commit timestamp.
16. Lookup does not inspect expiry timestamp.
17. Lookup does not use latest-version selection.
18. Lookup outside the table key range returns `None`.
19. Lookup in a multi-block table remains exact; candidate-block decode
   instrumentation is deferred until lazy candidate-block reads exist.

### 5. Full Cursor Contract

1. Full cursor over one-row table emits one row.
2. Full cursor over one-block table emits every row in encoded-key order.
3. Full cursor over multi-block table emits every row in encoded-key order.
4. Cursor crosses data-block boundaries without skipping rows.
5. Cursor crosses from a zstd block to another block.
6. `seek_to_first` positions at the first row.
7. `seek` before the first row positions at the first row.
8. `seek` exactly to a row positions at that row.
9. `seek` into a gap positions at the first greater row.
10. `seek` after the last row exhausts the cursor.
11. `advance` after exhaustion remains exhausted.
12. `current()` remains stable until `advance`.
13. Repeated `seek` to the same target is deterministic.
14. Re-seek after partial iteration repositions from the index.
15. Re-seek after exhaustion repositions from the index.
16. Ordinary state transitions do not panic.

### 6. Range And Prefix Cursors

1. Unbounded cursor returns all rows.
2. Exact bound returns one matching row when present.
3. Exact bound returns empty when absent.
4. Closed range includes lower and upper endpoints.
5. Open range excludes lower and upper endpoints.
6. Lower-unbounded range starts at the first row and stops at the upper bound.
7. Upper-unbounded range starts after the lower bound and continues to
   exhaustion.
8. Equal inclusive bounds return a singleton when present.
9. Equal exclusive bounds return empty.
10. Physical-prefix cursor returns every version for the physical key.
11. Physical-prefix cursor returns tombstones.
12. Physical-prefix cursor returns expired-looking rows.
13. Physical-prefix cursor does not cross branch-id bytes.
14. Physical-prefix cursor does not cross storage-space-id bytes.
15. Prefix-like user-key neighbors are excluded if their encoded physical-key
    bytes differ.
16. Range cursor can start and end in different data blocks.
17. Range cursor can start in the middle of a data block.
18. Range cursor can end in the middle of a data block.
19. Range cursor does not perform latest-version selection.

### 7. Raw Row Preservation

1. Puts preserve physical key bytes.
2. Puts preserve branch id bytes.
3. Puts preserve storage-space id bytes.
4. Puts preserve user key bytes, including embedded zero bytes.
5. Puts preserve commit version.
6. Puts preserve commit timestamp.
7. Puts preserve expiry timestamp.
8. Puts preserve value bytes.
9. Empty values are preserved.
10. Large values within L3 limits are preserved.
11. Tombstones preserve tombstone marker and key facts.
12. Expired-looking rows preserve expiry timestamp.
13. Multiple versions for one physical key are all emitted.
14. Versions for one physical key appear in encoded internal-key order.
15. Rows from different branches and storage spaces are treated as ordinary
    ordered rows.

### 8. Byte Source Parity

1. `open_bytes` and `open_source(BytesTableSource)` facts match.
2. Exact lookup results match for every model row.
3. Missing lookup results match.
4. Full cursor output matches.
5. Closed range cursor output matches.
6. Open range cursor output matches.
7. Physical-prefix cursor output matches.
8. Repeated seeks produce matching outputs.
9. Zstd artifact outputs match.
10. Corrupt bytes fail consistently across byte and range sources.

### 9. Deferred Lazy Decode And Block Routing

Because V1 table CRC validation requires full-object validation, these tests
belong to the later lazy reader follow-up rather than the first L5F
materialized-row implementation.

1. Opening a lazy reader does not materialize all rows into the reader output
   model if stats or instrumentation can prove it.
2. Exact lookup outside table key range decodes zero data blocks.
3. Exact lookup in a table with many blocks decodes only the candidate block.
4. Cursor `seek` starts decoding at the candidate block, not block zero, when
   seeking into the middle of the table.
5. Cursor `advance` decodes the next block only when crossing a block boundary.
6. Re-seek can reuse reader-local decoded block state only if deterministic.
7. Reader-local memoization, if any, is isolated per reader.
8. No process-global block cache state is created.

### 10. Boundary And Vocabulary Guards

1. Reader production code does not import `std::fs`.
2. Reader production code does not import `std::path`.
3. Reader production code does not import platform-specific filesystem traits.
4. Reader production code does not import `crate::backend`.
5. Reader production code does not import `crate::layout`.
6. Reader production code does not import `crate::service`.
7. Reader production code does not import `crate::branch`, `crate::commit`,
   `crate::lifecycle`, or engine crates.
8. Reader production code does not mention `KVSegment`, `SegmentEntry`,
   `SegmentSeekableIter`, `OwnedSegmentIter`, `pread`, mmap, path hash, or old
   segment file vocabulary.
9. Reader production code does not mention old `STRAKV` table bytes except in
   tests or docs.
10. Reader production code does not create `lazy_static`, `OnceLock`, static
    mutable cache state, or process-global block cache state.
11. Reader production code does not use product payload vocabulary such as
    `Value`, primitive names, MessagePack, or bincode product types.
12. Reader production code does not use visibility policy vocabulary such as
    `snapshot`, `as_of`, `latest`, `fork`, `rewrite`, `visible_at`,
    `ttl_filter`, or `live_only`.
13. Reader production code remains crate-private.

## Required Generated Tests

Extend `check_table_runtime_scaffold_contract` or a neighboring hidden testkit
route with an immutable-reader case counter.

For each generated script:

1. generate 1 to 256 sorted rows by default;
2. force at least one one-row table;
3. force at least one one-block table;
4. force at least one multi-block table;
5. include tombstones;
6. include expired-looking rows;
7. include empty values;
8. include nonempty values;
9. include user keys with embedded zero bytes;
10. include duplicate physical keys at different commit versions;
11. include different branch ids;
12. include different storage space ids;
13. vary `target_data_block_size`;
14. vary `rows_per_block`;
15. vary compression between uncompressed and zstd;
16. build valid bytes through L5E;
17. open byte-backed reader;
18. open range-backed reader;
19. compare reader facts to decoded table facts;
20. run exact lookups for present and absent keys;
21. run full cursor scans;
22. run generated seek/re-seek/advance sequences;
23. run closed/open/prefix bounds;
24. compare every reader output to an independent sorted-vector model;
25. inject at least one header/footer/block corruption per generated route, or
    run a bounded rotation of corruption cases across scripts;
26. inject source read errors through an instrumented source;
27. enforce fixed row, byte, operation, and source-read budgets.

Generated tests should keep rows and values bounded. Oversized block, key, and
row limits belong in focused unit tests or L3 format tests.

## Old Segment Reader Regression Map

Review `crates/storage/src/segment.rs` and `crates/storage/src/seekable.rs`.
Port or rewrite only the cases that still match storage-next V1.

Port or rewrite:

1. open valid table;
2. reject corrupt header/footer/block/index/properties;
3. point lookup present/missing;
4. point lookup in multi-block table;
5. iteration from beginning;
6. seek into middle;
7. prefix scan;
8. range scan;
9. block-boundary cursor movement;
10. compression roundtrip;
11. tombstone readback;
12. timestamp and expiry preservation;
13. duplicate physical-key versions;
14. long shared-prefix key routing;
15. byte-source failure behavior.

Do not port:

1. old `STRAKV` byte compatibility;
2. local path opens;
3. file descriptor lifetime tests;
4. `pread` tests;
5. mmap assumptions;
6. process-global block cache tests;
7. bloom/hash-index accelerator tests, except as deferred L5G evidence;
8. partitioned old-index tests that do not match M3G;
9. MVCC snapshot/latest lookup;
10. branch rewrite/inherited-layer behavior;
11. segmented store read-path tests that mix branch state and table mechanics.

## Review Checklist

Before calling L5F complete, review for these edge cases:

1. Does reader open reject corrupt bytes before any successful lookup?
2. Does range-backed open validate the table CRC before trusting footer
   offsets?
3. Do byte-backed and range-backed readers produce identical facts and rows?
4. Does exact lookup return tombstones and expired rows?
5. Does exact lookup avoid latest-version semantics?
6. Do duplicate physical-key versions survive full scans and prefix scans?
7. Does cursor seek into the middle of a multi-block table start at the right
   row?
8. Does cursor advance cross block boundaries without skipping or duplicating?
9. Do range bounds include/exclude endpoints correctly?
10. Does physical-prefix scan stop at exact encoded physical-key boundaries?
11. Are zstd blocks covered?
12. Are block checksum and footer checksum failures tested separately?
13. Are source read failures tested during open and during block read?
14. Are wrapped format errors visible through `source()`?
15. Is there any filesystem/path/backend/service/object-name import?
16. Is there any old segment, product payload, cache-global, or visibility
   policy vocabulary in production reader code?
17. Does the generated property route fail if reader coverage is removed?
18. Do default and no-default-feature testkit lanes pass?
19. Does wasm check still pass?

## Verification Commands

Run at minimum:

```text
cargo test -p strata-storage-next --locked --lib table::tests::reader
cargo test -p strata-storage-next --locked --lib table::tests
cargo test -p strata-storage-next --features testkit --locked --test table_runtime_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test table_runtime_properties
cargo test -p strata-storage-next --locked --test table_runtime_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo fmt --package strata-storage-next --check
git diff --check
```

If L5F promotes L3 format helpers, also run:

```text
cargo test -p strata-storage-next --locked --lib format::table
```

## Exit Criteria

L5F test coverage is complete when:

1. byte-backed and range-backed readers open valid M3G artifacts;
2. invalid/corrupt/truncated artifacts reject with typed L5 errors;
3. source-read failures reject with typed L5 source errors;
4. reader facts match decoded table facts;
5. exact lookup covers present, absent, one-block, multi-block, tombstone,
   expired, duplicate-version, empty-value, and large-value rows;
6. full/range/prefix cursors match independent sorted-vector models;
7. cursor seek, re-seek, advance, and exhaustion match the L5D cursor contract;
8. uncompressed and zstd data blocks are covered;
9. byte-backed and range-backed source parity is proven;
10. generated property tests include reader coverage;
11. source guards enforce L5 boundaries and old-reader vocabulary bans;
12. no test relies on old `crates/storage` code as an oracle at runtime.
