# M3G Implementation Brief: Immutable Table Format

Status: implementation brief

Parent plan: `docs/architecture/implementation-plans/m3-m3t-implementation-plan.md`

## Goal

Implement the storage-next immutable table byte format at L3.

This closes the known gap where `crates/storage-next/src/table/mod.rs` and
`crates/storage-next/tests/table_properties.rs` are placeholders while the V1
format architecture still requires immutable table header, footer, block, entry,
compression, checksum, golden, and fuzz coverage.

M3G is a durable-format slice. It must produce stable table bytes that L5 can
consume, but it must not implement the L5 table runtime itself. Point lookup,
range cursors, block cache policy, bloom/filter semantics, compaction, branch
state, and table object publication remain later slices.

## Inputs Read

Architecture and spec inputs:

1. `docs/architecture/storage/l3-durable-format-codec.md`
2. `docs/architecture/storage/l5-table-runtime.md`
3. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
4. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
5. `docs/architecture/storage/target-crate-shape-and-test-harness.md`
6. `docs/architecture/v1-testing-and-conformance-plan.md`
7. `docs/spec/strata-storage-format-v1.md`
8. `docs/architecture/implementation-plans/m3-m3t-implementation-plan.md`

Current storage-next inputs:

1. `crates/storage-next/src/format/key.rs`
2. `crates/storage-next/src/format/storage_row.rs`
3. `crates/storage-next/src/format/mod.rs`
4. `crates/storage-next/src/row/mod.rs`
5. `crates/storage-next/src/table/mod.rs`
6. `crates/storage-next/tests/table_properties.rs`
7. `crates/storage-next/tests/format_golden.rs`
8. `crates/storage-next/fuzz/`

Old-code evidence:

1. `crates/storage/src/segment_builder.rs`
2. `crates/storage/src/segment.rs`
3. `crates/storage/src/index.rs`
4. `crates/storage/src/bloom.rs`
5. `crates/storage/src/key_encoding.rs`
6. `crates/storage/src/stored_value.rs`
7. `crates/storage/src/memtable.rs`

## Scope

In scope:

1. Stable V1 immutable table byte shape.
2. Table header and footer encoders/decoders.
3. Table block frame encoder/decoder.
4. Uncompressed and zstd block payload support.
5. Data block entry encoding over `StorageRow` bytes and encoded internal keys.
6. Monolithic index block payload format.
7. Table properties block payload format.
8. Whole-table artifact encode/decode helpers for L3 validation and tests.
9. Golden vectors for header/footer/block/table bytes.
10. Table format fuzz targets and proptest coverage.
11. Replacement of placeholder table integration tests with real table-format
    properties.
12. Spec updates to `docs/spec/strata-storage-format-v1.md`.
13. Porting-log entry mapping old segment bytes to the V1 table format.

Out of scope:

1. L5 point lookups.
2. L5 range, prefix, or merge cursors.
3. Block cache implementation.
4. Bloom/filter query semantics.
5. Compaction.
6. L4 table object publication.
7. L6 table manifests, branch levels, inherited layers, or reachability.
8. L7 commit runtime integration.
9. L8 quarantine, purge, or recovery policy for corrupt table objects.
10. Compatibility readers for old pre-V1 `STRAKV` table files.

## Layer Boundary

M3G owns durable bytes only.

L3 may expose:

1. `encode_table_header` and `decode_table_header`
2. `encode_table_footer` and `decode_table_footer`
3. `encode_table_block_frame` and `decode_table_block_frame`
4. `encode_table_data_block` and `decode_table_data_block`
5. `encode_table_index_block` and `decode_table_index_block`
6. `encode_table_properties_block` and `decode_table_properties_block`
7. `encode_immutable_table` and `decode_immutable_table` as bounded
   materialized helpers for tests and small artifacts
8. block-level visitors that let future L5 range-read table objects without
   materializing an entire large table

L3 must not expose:

1. point lookup API
2. range or prefix cursor API
3. branch-aware table installation
4. object names or backend IO
5. cache lookup or eviction policy
6. product data interpretation

The production table runtime will later live in `crates/storage-next/src/table/`
and use these L3 byte helpers. L4 table object publication will later publish
opaque table bytes produced by L5.

## Existing Behavior To Preserve

1. `StorageRow` remains the canonical durable row envelope.
2. `InternalKey` ordering remains physical key ascending and commit version
   descending through the existing L3 key codec.
3. Table bytes remain self-identifying and integrity-checked.
4. Block frames remain length-delimited.
5. Table block compression must support both uncompressed and zstd payloads.
6. L3 errors remain storage-mechanical and avoid product wording.
7. Old table implementation evidence remains useful for block framing,
   compression, index, and filter design, but it is not compatibility law.

## Intentional V1 Changes

1. The stable V1 table format version is `1`.
2. Stable V1 table files do not use old development format version `7`.
3. Stable V1 table files use a new self-identifying magic, not old `STRAKV`
   bytes.
4. Data entries carry storage-row bytes, not bincode product values.
5. Data entries include encoded internal keys so table indexes and future L5
   readers can compare keys without interpreting product values.
6. The encoded internal key and nested `StorageRow` must agree on physical key
   and commit version.
7. The first M3G format uses a monolithic index block. Partitioned indexes and
   bloom/filter semantics are reserved for L5/M4, not required to make the byte
   format stable.
8. Empty immutable tables are rejected. L5 should not publish empty table
   objects.

## Stable Byte Shape

The stable V1 table object layout is:

```text
table_header           64 bytes
data_block_frames      repeated, at least one
filter_block_frame     optional, absent in M3G writers
index_block_frame      one, required
properties_block_frame one, required
table_footer           64 bytes
```

M3G writers MUST omit the filter block. M3G readers MUST accept only an absent
filter block. The footer fields reserve the position so a later L5 filter block
can be added deliberately without changing the surrounding table envelope.

All offsets in footer and index entries are absolute byte offsets from the start
of the table object. All frame lengths are full encoded frame lengths, including
frame header and CRC.

### Header

The header is exactly 64 bytes:

```text
table_magic            4 bytes   "STTB"
format_version         u32 LE, MUST be 1
header_size            u32 LE, MUST be 64
header_flags           u32 LE, MUST be 0
target_data_block_size u32 LE
data_block_count       u32 LE, MUST be nonzero
row_count              u64 LE, MUST be nonzero
commit_min             u64 LE
commit_max             u64 LE
reserved               16 bytes, MUST be zero
```

Validation rules:

1. `commit_min <= commit_max`.
2. `row_count` is nonzero and bounded before allocation.
3. `data_block_count` is nonzero and bounded before allocation.
4. `target_data_block_size` is nonzero.
5. Reserved bytes and flags fail closed until assigned by a later format
   version.
6. Old `STRAKV` bytes are rejected as invalid magic, not treated as V1.

### Block Frame

Every table block uses one frame shape:

```text
block_type             u8
compression_codec      u8
block_flags            u16 LE, MUST be 0
encoded_len            u32 LE
decoded_len            u32 LE
encoded_payload        encoded_len bytes
crc32                  u32 LE
```

The CRC32 covers every byte in the frame before the `crc32` field.

Block frame overhead is 16 bytes.

Block types:

```text
1                      data
2                      index
3                      filter, reserved in M3G
4                      properties
```

Compression codecs:

```text
0                      uncompressed
1                      zstd
```

Rules:

1. Unknown block types are rejected.
2. Unknown compression codecs are rejected.
3. Nonzero flags are rejected.
4. `encoded_len` and `decoded_len` are bounded before allocation.
5. Uncompressed blocks require `encoded_len == decoded_len`.
6. Zstd blocks must decompress to exactly `decoded_len`.
7. Block decode validates CRC before returning decoded payload bytes.
8. A decoder that reads a block from a larger byte slice returns the exact bytes
   consumed so callers can validate table layout.

If adding `zstd` to storage-next breaks no-default or wasm builds, stop and fix
the dependency strategy before merging M3G. Do not silently omit zstd support,
because the V1 format requirement says table readers support it.

### Data Block Payload

A decoded data block payload is:

```text
entry_count            u32 LE, MUST be nonzero
entries                repeated table entry
```

Each table entry is:

```text
internal_key_len       u32 LE, MUST be nonzero
internal_key_bytes     V1 InternalKey encoding
row_len                u32 LE, MUST be nonzero
row_bytes              V1 StorageRow encoding
```

Validation rules:

1. Entries inside a block are strictly sorted by encoded internal-key bytes.
2. Duplicate internal keys are rejected.
3. Duplicate physical keys at different commit versions are allowed if the
   encoded internal-key order is valid.
4. The nested storage row decodes through `decode_storage_row`.
5. `decode_internal_key(internal_key_bytes)` must match the nested row's
   physical key and commit version.
6. Row order is preserved exactly.
7. The data block decoder rejects trailing bytes.
8. Entry count and every length field are bounded before allocation.

The data block format intentionally does not use prefix compression in M3G.
L5 can add a separate V2 data-block payload later only with a format-versioned
change and full golden/fuzz coverage. Stable V1 starts with a simpler format so
the row/table boundary is unambiguous.

### Index Block Payload

The monolithic index block payload is:

```text
index_format_version   u32 LE, MUST be 1
entry_count            u32 LE, MUST equal header.data_block_count
entries                repeated index entry
```

Each index entry is:

```text
first_key_len          u32 LE, MUST be nonzero
first_key_bytes        V1 InternalKey encoding
last_key_len           u32 LE, MUST be nonzero
last_key_bytes         V1 InternalKey encoding
block_offset           u64 LE, absolute table offset
block_frame_len        u32 LE, full encoded data-block frame length
row_count              u32 LE, rows in the data block
```

Validation rules:

1. Index entries are sorted by `first_key_bytes`.
2. Each entry has `first_key_bytes <= last_key_bytes`.
3. Adjacent entries must not overlap in key range.
4. Each referenced frame is inside the table body and before the footer.
5. Each referenced frame decodes as a data block.
6. The decoded data block first and last internal keys match the index entry.
7. The decoded data block row count matches the index entry.
8. Index count and key lengths are bounded before allocation.

### Properties Block Payload

The properties block payload is:

```text
properties_format_version u32 LE, MUST be 1
row_count                 u64 LE
data_block_count          u32 LE
commit_min                u64 LE
commit_max                u64 LE
min_key_len               u32 LE, MUST be nonzero
min_key_bytes             V1 InternalKey encoding
max_key_len               u32 LE, MUST be nonzero
max_key_bytes             V1 InternalKey encoding
```

Validation rules:

1. Properties facts match the header.
2. Properties facts match the decoded data blocks.
3. `min_key_bytes` is the first row key in the table.
4. `max_key_bytes` is the last row key in the table.
5. `commit_min` and `commit_max` are derived from nested `StorageRow` commit
   versions, not from index keys alone.

### Footer

The footer is exactly 64 bytes and appears at the end of the object:

```text
index_block_offset     u64 LE
index_block_frame_len  u32 LE
filter_block_offset    u64 LE, MUST be 0 in M3G
filter_block_frame_len u32 LE, MUST be 0 in M3G
props_block_offset     u64 LE
props_block_frame_len  u32 LE
footer_magic           4 bytes   "STTF"
reserved               20 bytes, MUST be zero
table_crc32            u32 LE
```

The table CRC32 covers every byte in the table object before the final
`table_crc32` field. That includes the header, all block frames, and the footer
fields preceding `table_crc32`.

Validation rules:

1. Footer magic must match.
2. Reserved bytes must be zero.
3. CRC must match before offsets are trusted as table facts.
4. Offsets and lengths must point to the canonical layout:
   data blocks, optional filter, index, properties, footer.
5. M3G rejects any nonzero filter offset or length.
6. No unindexed bytes may exist between the header and footer except canonical
   block frames.

## API Shape

The exact Rust names may change, but the ownership should be close to:

```text
TableHeader
TableFooter
TableBlockKind
TableCompression
TableBlockFrame
TableDataBlock
TableIndexBlock
TableProperties
ImmutableTable
```

Suggested module layout:

```text
crates/storage-next/src/format/table.rs
crates/storage-next/src/format/table/block.rs      if table.rs grows too large
crates/storage-next/src/format/table/data.rs       if table.rs grows too large
crates/storage-next/src/format/table/index.rs      if table.rs grows too large
crates/storage-next/src/format/table/tests.rs      or module-local test splits
```

Keep L3 APIs private to the crate unless a later public boundary needs them.
Fuzz/testkit routing can use `pub(crate)` plus feature-gated testkit helpers as
the other format slices do.

## Error Mapping

Use existing `FormatError` variants where they are precise:

1. bad table/header/footer magic -> `InvalidMagic`
2. version `0` -> `PreV1Format`
3. version greater than `1` -> `FutureFormat`
4. nonzero flags -> `UnsupportedFlags`
5. malformed length/count/offset -> `InvalidLength`
6. invalid ordering or mismatched facts -> `InvalidValue`
7. checksum mismatch -> `ChecksumMismatch`
8. trailing decoded payload bytes -> `TrailingData`
9. nested key or row corruption -> propagate nested `FormatError`

Add new generic format errors only if needed across durable formats. Likely
useful additions:

1. `UnsupportedCompression { format, codec }`
2. `DecompressionFailed { format }`

Do not add table errors that mention KV, JSON, vector, graph, search, or engine
product concepts.

## Spec And Golden Updates

M3G must update `docs/spec/strata-storage-format-v1.md` so section 17 becomes
concrete instead of provisional. The spec update should:

1. mark old `STRAKV` version 7 as pre-V1 evidence only
2. record the stable V1 table magic, header, block frame, payloads, footer, CRC,
   and compression codecs
3. record that M3G writers omit filter blocks
4. record that table entries are storage-row-native
5. record allocation guard expectations without making implementation constants
   part of the public spec unless intended
6. add table goldens to the golden-vector inventory

Golden vectors should include at least:

1. one-row data block, uncompressed
2. multi-row data block with put plus tombstone, uncompressed
3. zstd-compressed data block
4. monolithic index block
5. table properties block
6. complete one-data-block table
7. complete two-data-block table

Goldens that include zstd bytes intentionally pin the current compressed output.
If the zstd library output changes, the golden update must be explicit and
reviewed as a format change.

## Source Map And Retirement Notes

Before implementation code lands, update `m3-porting-log.md` with:

1. old source files inspected
2. behavior preserved: self-identifying table object, framed blocks, checksum,
   sorted internal keys, zstd support, index/properties separation
3. behavior rewritten: stable version `1`, storage-row-native entries, no
   bincode product value payloads, no old `STRAKV` compatibility reader
4. behavior deferred: point lookup, filters, partitioned indexes, block cache,
   compaction, table object publication
5. old tests that remain evidence for M4 table runtime rather than M3G byte
   format

## Implementation Slices

Suggested slices:

1. `M3G1`: Add table format module, constants, header/footer, block frame, and
   uncompressed/zstd block compression tests.
2. `M3G2`: Add data block, index block, properties block, and complete table
   artifact encode/decode helpers.
3. `M3G3`: Add strict corruption matrix, ordering/fact validation, allocation
   guards, table goldens, and replace the placeholder table integration test.
4. `M3G4`: Add fuzz routing, source guards, spec/porting-log/progress closeout,
   and no-default/wasm/all-features verification.

The slices may merge if the patch stays reviewable, but each closeout must
record sensitivity probes and the exact verification commands.

## Verification Commands

Use narrow commands while developing:

```sh
cargo test -p strata-storage-next --locked table_format
cargo test -p strata-storage-next --locked format::table
cargo test -p strata-storage-next --locked table_properties
cargo test -p strata-storage-next --locked format_golden
```

Use property and fuzz commands:

```sh
PROPTEST_CASES=2048 cargo test -p strata-storage-next --locked table_format_model
cd crates/storage-next && cargo +nightly fuzz run format_table_block -- -runs=4096
cd crates/storage-next && cargo +nightly fuzz run format_table_artifact -- -runs=2048
```

Close with:

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

M3G is complete when:

1. The stable table byte format is implemented and documented.
2. Old `STRAKV` development table bytes are not accepted as V1.
3. Table entries are storage-row-native and validated against internal keys.
4. Header, footer, block frame, data block, index block, properties block, and
   complete table artifact goldens exist.
5. Uncompressed and zstd block decode are both covered.
6. Malformed bytes fail closed before allocation bombs or partial table facts.
7. `tests/table_properties.rs` contains real table-format property coverage, not
   only a placeholder existence test.
8. Fuzz routing covers block frames and whole table artifacts.
9. The spec, implementation brief, test plan, porting log, and progress tracker
   agree on the final format.
