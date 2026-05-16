# M3G Test Plan: Immutable Table Format

Status: test-suite plan

Parent brief:
`docs/architecture/implementation-plans/m3g-immutable-table-format-implementation-brief.md`

## Goal

Prove that immutable table bytes are stable, storage-row-native, strict, and
ready for L5 table runtime work.

The suite must fail if old development table bytes are accepted as V1, if valid
table entries can be built from product-value payloads instead of `StorageRow`
bytes, if internal-key ordering is not enforced, if zstd support silently
disappears, or if corrupt table bytes can produce partial table facts.

## Testing Principles

1. Test storage bytes, not product table semantics.
2. Valid construction goes through `StorageRow` values and encoded internal keys.
3. Invalid construction mutates encoded bytes after a valid encode or uses
   explicit malformed byte fixtures.
4. Every accepted table preserves exact row order, exact row bytes, and exact
   internal-key ordering.
5. Every rejected table fails before callers can install partial table facts.
6. Header/footer/block offsets are treated as adversarial input.
7. Allocation guards are tested with deterministic fixtures.
8. Golden vectors are part of the durable format contract.
9. Fuzz targets must exercise block-level and whole-table decode paths.
10. Sensitivity probes must prove tests fail if ordering, checksum, or
    row/key-fact validation is bypassed.

## Required Cases

### 1. Header Codec

1. Header round-trips with one data block and nonzero rows.
2. Bad header magic returns `InvalidMagic`.
3. Old `STRAKV` development header bytes are rejected as invalid magic.
4. Header version `0` returns `PreV1Format`.
5. Header future version returns `FutureFormat` with max-supported version `1`.
6. Header size different from `64` is rejected.
7. Nonzero header flags are rejected as unsupported flags.
8. Nonzero reserved bytes are rejected.
9. Zero target data block size is rejected.
10. Zero data block count is rejected.
11. Zero row count is rejected.
12. `commit_min > commit_max` is rejected.
13. Oversized data block count is rejected before allocating index vectors.
14. Oversized row count is rejected before allocating row vectors.
15. Truncated header is rejected at each fixed-field boundary.

### 2. Footer Codec

1. Footer round-trips with index/properties offsets and absent filter fields.
2. Bad footer magic returns `InvalidMagic`.
3. Nonzero reserved bytes are rejected.
4. Nonzero filter offset with zero filter length is rejected in M3G.
5. Zero filter offset with nonzero filter length is rejected in M3G.
6. Nonzero filter offset and length are rejected in M3G.
7. Checksum mismatch is rejected before offsets are trusted.
8. Index offset before the header is rejected.
9. Index frame length zero is rejected.
10. Properties offset before the index frame is rejected.
11. Properties frame length zero is rejected.
12. Offset plus length overflow is rejected.
13. Offset plus length past the footer start is rejected.
14. Overlapping index/properties ranges are rejected.
15. Hidden unreferenced bytes before the footer are rejected.
16. Truncated footer is rejected at each fixed-field boundary.

### 3. Block Frame Codec

1. Uncompressed data block frame round-trips.
2. Zstd-compressed data block frame round-trips.
3. Empty uncompressed payload is rejected for data/index/properties payloads that
   require nonzero decoded payloads.
4. Unknown block type is rejected.
5. Unknown compression codec is rejected.
6. Nonzero block flags are rejected.
7. Uncompressed frame with `encoded_len != decoded_len` is rejected.
8. Zstd frame whose decompressed length differs from `decoded_len` is rejected.
9. Zstd frame with invalid compressed bytes is rejected as decompression failure
   or precise invalid format error.
10. Frame checksum mismatch is rejected before returning decoded payload bytes.
11. `encoded_len = u32::MAX` is rejected before allocation.
12. `decoded_len = u32::MAX` is rejected before allocation or decompression.
13. Length fields whose sum overflows are rejected.
14. Truncated payload is rejected.
15. Truncated CRC is rejected.
16. Decoding one frame from a sequence returns exact bytes consumed.
17. Decoding a frame as the wrong expected block type is rejected.
18. Zstd support is exercised under default, all-features, and no-default builds.

### 4. Data Block And Entry Codec

1. One put row round-trips.
2. Put row plus tombstone row round-trips in encoded order.
3. Empty entry list is rejected before bytes are emitted or accepted.
4. Zero entry count in durable bytes is rejected.
5. Entry count above the implementation limit is rejected before allocation.
6. Zero internal-key length is rejected.
7. Internal-key length above the implementation limit is rejected.
8. Internal-key bytes that fail `decode_internal_key` are rejected.
9. Zero row length is rejected.
10. Row length above the implementation limit is rejected.
11. Row bytes that fail `decode_storage_row` are rejected.
12. Encoded internal key whose physical key disagrees with the row is rejected.
13. Encoded internal key whose commit version disagrees with the row is rejected.
14. Entries not sorted by encoded internal-key bytes are rejected by constructor
    and durable decode.
15. Duplicate internal keys are rejected.
16. Duplicate physical keys at different commit versions are accepted when
    internal-key ordering is valid.
17. Commit-version descending order for the same physical key is preserved.
18. Trailing bytes after declared entries are rejected.
19. Corrupt nested storage-row tombstone payload is rejected through the nested
    row decoder.
20. Corrupt nested storage-row flags are rejected through the nested row decoder.
21. Data block encoding is deterministic for the same row sequence.

### 5. Index Block Codec

1. Single-entry index block round-trips.
2. Multi-entry index block round-trips in sorted order.
3. Index format version `0` returns `PreV1Format`.
4. Index future version returns `FutureFormat`.
5. Zero index entry count is rejected.
6. Index entry count above the implementation limit is rejected before
   allocation.
7. Zero first-key length is rejected.
8. Zero last-key length is rejected.
9. Invalid first or last internal-key bytes are rejected.
10. `first_key > last_key` is rejected.
11. Index entries not sorted by `first_key` are rejected.
12. Overlapping adjacent key ranges are rejected.
13. Block offset before the header is rejected during whole-table validation.
14. Block frame length zero is rejected.
15. Offset plus frame length overflow is rejected.
16. Row count zero is rejected.
17. Referenced frame that is not a data block is rejected during whole-table
    validation.
18. Referenced data block whose first key differs from index first key is
    rejected.
19. Referenced data block whose last key differs from index last key is rejected.
20. Referenced data block whose row count differs from index row count is
    rejected.
21. Trailing index payload bytes are rejected.

### 6. Properties Block Codec

1. Properties block round-trips.
2. Properties format version `0` returns `PreV1Format`.
3. Properties future version returns `FutureFormat`.
4. Zero row count is rejected.
5. Zero data block count is rejected.
6. `commit_min > commit_max` is rejected.
7. Zero min-key length is rejected.
8. Zero max-key length is rejected.
9. Invalid min or max internal-key bytes are rejected.
10. `min_key > max_key` is rejected.
11. Properties row count mismatch with header is rejected.
12. Properties data block count mismatch with header is rejected.
13. Properties commit range mismatch with decoded rows is rejected.
14. Properties min key mismatch with decoded rows is rejected.
15. Properties max key mismatch with decoded rows is rejected.
16. Trailing properties payload bytes are rejected.

### 7. Whole Table Artifact

1. One-block uncompressed table round-trips rows and table facts.
2. Two-block uncompressed table round-trips rows and table facts.
3. One-block zstd table round-trips rows and table facts.
4. Mixed compression by block round-trips if the API allows per-block codecs.
5. Empty table construction is rejected before bytes are emitted.
6. Input rows not sorted by internal key are rejected.
7. Input rows with duplicate internal keys are rejected.
8. Header row count must match decoded data rows.
9. Header data block count must match decoded data blocks.
10. Header commit range must match decoded data rows.
11. Footer CRC mismatch rejects the table before index or properties facts are
    installed.
12. Footer index pointer to an index frame with the wrong block type is rejected.
13. Footer properties pointer to a properties frame with the wrong block type is
    rejected.
14. Missing data block referenced by index is rejected.
15. Extra data block not referenced by index is rejected.
16. Extra bytes between data and index are rejected.
17. Extra bytes between index and properties are rejected.
18. Extra bytes between properties and footer are rejected.
19. Table shorter than header plus footer is rejected.
20. Footer with valid CRC but impossible offsets is rejected.
21. Header with valid bytes but footer facts from a different table is rejected.
22. Decoded rows are returned in table order.
23. Storage space IDs are preserved as opaque row facts and not interpreted as
    product capabilities.

### 8. Golden Vectors

1. One-row uncompressed data block matches a checked-in golden.
2. Put-plus-tombstone uncompressed data block matches a checked-in golden.
3. Zstd-compressed data block matches a checked-in golden.
4. Monolithic index block matches a checked-in golden.
5. Properties block matches a checked-in golden.
6. Complete one-block table matches a checked-in golden.
7. Complete two-block table matches a checked-in golden.
8. Golden tests fail if row order changes.
9. Golden tests fail if nested `StorageRow` bytes change without intentional
   storage-row golden updates.
10. Golden tests fail if footer CRC coverage stops including footer offset
    fields.
11. Golden inventory in `docs/spec/strata-storage-format-v1.md` lists the table
    vectors.

### 9. Allocation And Size Bounds

1. Header `data_block_count = u32::MAX` is rejected before allocation.
2. Data block `entry_count = u32::MAX` is rejected before allocation.
3. Index `entry_count = u32::MAX` is rejected before allocation.
4. Internal-key length above the implementation limit is rejected before slicing.
5. Row length above the implementation limit is rejected before slicing.
6. Block `encoded_len` above the implementation limit is rejected before
   allocation.
7. Block `decoded_len` above the implementation limit is rejected before
   allocation or decompression.
8. Zstd decoded size above the implementation limit is rejected even if the
   compressed payload is small.
9. Offset plus length overflow is rejected for every footer and index pointer.
10. A table at the generated property budget of 128 rows decodes when total
    block bytes fit limits.
11. Rows above the generated property budget are not rejected solely for test
    budget reasons; durable row-count rejection is exercised by
    `MAX_TABLE_ROWS + 1` header and properties facts.

### 10. Fuzz And Property Coverage

1. `format_table_block` fuzz target routes arbitrary bytes through block-frame
   decode.
2. `format_table_artifact` fuzz target routes arbitrary bytes through whole-table
   decode.
3. Fuzz invariant: arbitrary bytes never panic.
4. Fuzz invariant: successful block decode consumes exactly one frame.
5. Fuzz invariant: successful table decode consumes all bytes.
6. Fuzz invariant: successful table decode has nonzero row count.
7. Fuzz invariant: successful table decode has sorted rows by internal key.
8. Fuzz invariant: successful table decode has header, index, properties, and
   data-block facts that agree.
9. Fuzz invariant: checksum mismatch never succeeds.
10. Property test generates 1 to 128 rows, value sizes 0 to 512 bytes, put and
    tombstone rows, duplicate physical keys with distinct commit versions, and
    target data block sizes that force one-block and multi-block tables.
11. Property model sorts rows by encoded internal key, computes expected commit
    range and key range independently, and asserts encode/decode identity.
12. Property test runs both uncompressed and zstd paths.
13. Regression seeds go under
    `crates/storage-next/proptest-regressions/table_format.txt` only if a
    failing seed is captured.

### 11. No Product Payload Leakage

1. No valid table-format test constructs a data entry from bincode product value
   bytes or arbitrary strings without a `StorageRow`.
2. No storage-next table format code imports engine crates or product primitive
   types.
3. Old `STRAKV` version 7 bytes are not accepted as valid V1 table bytes.
4. The spec marks old table v7 bytes as historical evidence only.
5. Source scans reject test names or comments that describe valid V1 table
   payloads as KV, primitive, transaction, entity, JSON, graph, vector, search,
   or product payloads.
6. Source scans intentionally allow `StorageSpaceId::engine` where used to
   construct opaque storage-row keys.

### 12. Placeholder Harness Replacement

1. `crates/storage-next/tests/table_properties.rs` no longer only checks that
   `src/table/mod.rs` exists.
2. The integration harness runs at least one generated table artifact property
   through the L3 table format APIs.
3. The harness stays storage-mechanical and does not test L5 point lookup,
   cursor, cache, or compaction behavior before those runtimes exist.
4. The placeholder existence test may remain only as an additional smoke test,
   not as the primary table coverage.

## Sensitivity Probes

Each implementation closeout should record at least four probes:

1. Temporarily accept old `STRAKV` magic.
   Expected failure: old-development-format rejection test.
2. Temporarily skip table footer CRC validation.
   Expected failure: footer checksum or offset-fact mutation test.
3. Temporarily skip data-entry sorted-order validation.
   Expected failure: unsorted data block or unsorted whole-table test.
4. Temporarily skip internal-key versus `StorageRow` validation.
   Expected failure: internal-key/row mismatch test.
5. Optional: temporarily treat unknown compression codec as uncompressed.
   Expected failure: unknown codec test.
6. Optional: temporarily ignore extra bytes between blocks.
   Expected failure: hidden-byte table layout test.
7. Optional: temporarily allocate directly from unchecked entry count.
   Expected failure: allocation-guard test or review/clippy rejection.

The mutation must be reverted before closeout. The progress tracker must name
the failing test and the verification command that passed after revert.

## Suggested Test Layout

Prefer splitting tests before files become large:

1. `crates/storage-next/src/format/table.rs` for small codec tests.
2. `crates/storage-next/src/format/table/block_tests.rs` or equivalent split for
   block frame and compression tests.
3. `crates/storage-next/src/format/table/data_tests.rs` for data-block entry
   tests.
4. `crates/storage-next/src/format/table/artifact_tests.rs` for whole-table
   validation.
5. `crates/storage-next/tests/format_golden.rs` for golden vector registration.
6. `crates/storage-next/tests/table_properties.rs` for generated artifact
   properties.
7. `crates/storage-next/src/testkit/format_fuzz.rs` for hidden fuzz routing.
8. `crates/storage-next/fuzz/fuzz_targets/format_table_block.rs`.
9. `crates/storage-next/fuzz/fuzz_targets/format_table_artifact.rs`.

Do not create production module names with roadmap labels.

## Verification Commands

Narrow commands:

```sh
cargo test -p strata-storage-next --locked table_format
cargo test -p strata-storage-next --locked format::table
cargo test -p strata-storage-next --locked table_properties
cargo test -p strata-storage-next --locked format_golden
```

Property/fuzz commands:

```sh
PROPTEST_CASES=2048 cargo test -p strata-storage-next --locked table_format_model
cd crates/storage-next && cargo +nightly fuzz run format_table_block -- -runs=4096
cd crates/storage-next && cargo +nightly fuzz run format_table_artifact -- -runs=2048
```

Broad commands:

```sh
cargo test -p strata-storage-next --locked
cargo test -p strata-storage-next --no-default-features --locked
cargo test -p strata-storage-next --all-features --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked
cargo doc -p strata-storage-next --no-deps --locked
cargo fmt --package strata-storage-next --check
git diff --check
```

## Exit Gate

The suite is complete when:

1. Every required case above is covered by executable tests or explicitly
   classified as unreachable with a code reference.
2. Golden vectors cover the stable table byte shape.
3. No test blesses old `STRAKV` bytes as valid V1 table bytes.
4. No valid table entry can be constructed without storage-row bytes.
5. Fuzz/property coverage exercises length, count, compression, checksum, and
   offset guardrails.
6. `tests/table_properties.rs` contains real generated table-format coverage.
7. Sensitivity probes are recorded and reverted.
8. The spec, implementation brief, test plan, porting log, and progress tracker
   agree on the final table format.
