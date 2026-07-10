# L5F Implementation Plan: Immutable Table Reader

Status: draft implementation plan

Parent plans:

1. `docs/architecture/implementation-plans/m4-m4t-implementation-plan.md`
2. `docs/architecture/implementation-plans/m4-l5-table-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/l5b-row-key-adapters-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/l5c-mutable-frozen-tables-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/l5d-raw-cursors-merge-cursor-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/l5e-immutable-table-builder-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/l5f-immutable-table-reader-test-plan.md`

## Goal

Port immutable-table read mechanics into storage-next L5 over the M3G table
format.

L5F must let L6/L8 read an immutable table artifact mechanically:

1. open and validate M3G table bytes;
2. expose table facts derived from decoded header/properties;
3. read exact internal keys;
4. scan raw encoded-key ranges and physical-key prefixes;
5. support uncompressed and zstd data blocks;
6. preserve tombstones, expired-looking rows, duplicate physical-key versions,
   branch bytes, storage-space ids, timestamps, and values;
7. surface table corruption and source-read failures as typed L5 errors;
8. stay independent from object names, paths, backend syscalls, branch state,
   visibility policy, and product payload meaning.

L5F is not the user read path. It must not select the latest visible version,
hide tombstones, apply TTL, evaluate snapshots, rewrite branch ids, install
tables into manifests, or invoke durable object publication.

## Inputs

1. `docs/architecture/storage/l5-table-runtime.md`
2. `docs/spec/strata-storage-format-v1.md`
3. `docs/architecture/implementation-plans/m4-l5-table-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/l5b-row-key-adapters-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/l5c-mutable-frozen-tables-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/l5d-raw-cursors-merge-cursor-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/l5e-immutable-table-builder-implementation-plan.md`
8. `crates/storage-next/src/format/table/`
9. `crates/storage-next/src/table/reader.rs`
10. `crates/storage-next/src/table/builder.rs`
11. `crates/storage-next/src/table/cursor.rs`
12. `crates/storage-next/src/table/key.rs`
13. `crates/storage-next/src/table/facts.rs`
14. `crates/storage/src/segment.rs`
15. `crates/storage/src/seekable.rs`

## Existing-Code Source Map

| Current file | Relevant evidence | L5F porting rule |
|---|---|---|
| `crates/storage/src/segment.rs` | `KVSegment` opens immutable segment files, reads indexes and blocks, serves point lookup, range/prefix iteration, and classifies corruption. | Preserve reader mechanics and regression cases. Do not port local paths, file handles, `pread`, process-global caches, old `STRAKV` bytes, bloom/hash-index shortcuts, or MVCC snapshot lookup semantics. |
| `crates/storage/src/seekable.rs` | `SegmentSeekableIter` repositions an immutable-table cursor through index-guided seek. | Preserve raw seek mechanics. Do not port `MvccSeekableIter` or branch rewriting. |
| `crates/storage-next/src/format/table/` | M3G table header/footer/block/data/index/properties validation, checksums, compression, and decoded artifact model. | Use L3 for byte-format parsing and validation. L5F may request reader-oriented L3 helper exports, but must not duplicate format parsing ad hoc. |
| `crates/storage-next/src/table/builder.rs` | Builds valid M3G table bytes and facts from sorted L5 rows. | Use as the primary producer in reader tests and generated models. |
| `crates/storage-next/src/table/cursor.rs` | Defines `TableCursor`, `BoundedTableCursor`, and merge-compatible raw cursor contract. | Immutable table cursors must implement the same raw cursor contract and remain policy-free. |

## Scope

L5F implements:

1. immutable table reader types over M3G artifacts;
2. a small range-readable table source trait or equivalent abstraction;
3. an in-memory byte source for direct tests and compaction outputs;
4. reader construction from owned/borrowed bytes;
5. reader construction from a range-readable source plus byte count;
6. M3G metadata loading and validation through L3 helper APIs;
7. table facts derived from decoded header/properties;
8. exact internal-key lookup over validated materialized rows;
9. raw full-table, range, and physical-prefix cursors;
10. validated row materialization for the first implementation, with an API
    that can later support lazy data-block decode;
11. source-read and decode/corruption error routing;
12. generated reader model tests;
13. source guards proving no old reader/path/backend/product vocabulary enters
    L5 production code;
14. M4-L5 porting-log entry for immutable reader mechanics.

L5F does not implement:

1. object-backed table service adapters; L5I owns the L4/L5 object handoff;
2. durable publication or table object naming;
3. lazy candidate-block reads or reader-local block-cache behavior; L5G/L5I or
   a later L5F follow-up owns that optimization.
4. local filesystem paths, `std::fs`, `pread`, mmap, or backend calls;
5. table block cache ownership or eviction; L5G owns cache behavior;
6. bloom/filter accelerators; L5G owns optional accelerators;
7. compaction output production; L5H owns compaction execution;
8. branch-local table placement or level ownership;
9. inherited COW layers, fork gates, or branch id rewriting;
10. snapshot/as-of/latest-visible row selection;
11. tombstone hiding, tombstone elision, TTL expiry filtering, or retention;
12. product `Value`, primitive payload, or engine capability semantics;
12. old `STRAKV`/segment-v7 compatibility.

## Target Module Shape

Primary implementation target:

```text
crates/storage-next/src/table/reader.rs
```

Supporting changes:

```text
crates/storage-next/src/table/mod.rs
crates/storage-next/src/table/tests/reader.rs
crates/storage-next/src/format/table/
crates/storage-next/src/testkit/table_runtime.rs
crates/storage-next/tests/table_runtime_properties.rs
crates/storage-next/tests/table_runtime_source_guard.rs
docs/architecture/implementation-plans/M4/m4-l5-porting-log.md
```

If `reader.rs` grows past a maintainable size, split before adding more
behavior. A likely private module shape is:

```text
reader/mod.rs
reader/source.rs
reader/metadata.rs
reader/cursor.rs
reader/tests.rs
```

Keep all production surfaces `pub(crate)`. L9 owns any future public storage
API.

## V1 Checksum Constraint

The V1 table footer contains a table CRC32 covering every byte before the CRC
field. The spec says readers must validate this checksum before trusting footer
offsets.

That creates a hard reader constraint:

1. byte-backed readers can validate with the full byte slice at open;
2. range-backed readers must either read/hash the full object at open or be
   handed a separately proven validation fact from a lower layer;
3. L5F must not trust footer offsets from an unvalidated source;
4. lazy data-block **decode** is still allowed after full-object checksum
   validation;
5. avoiding any full-object read for range-backed sources would require a
   future format change or a new L4 validation proof, not an L5 shortcut.

For V1, implement the honest path: validate the whole table byte stream at
open, then serve reads from validated table facts and rows. The first
implementation may materialize rows after validation; the reader API must stay
compatible with later data-block-on-demand internals. Range-backed open may
validate by chunked sequential reads so it does not require direct filesystem or
backend APIs.

## Proposed Type Surface

Use these names unless implementation discovers a clearer local convention.
Changing names is acceptable only if the responsibilities stay intact.

### `TableByteSource`

Range-readable table source supplied to L5 by tests, L6, or the later L5I
object-backed adapter.

Suggested shape:

```text
trait TableByteSource {
    fn byte_count(&self) -> u64;
    fn read_at(&self, offset: u64, len: usize) -> TableRuntimeResult<Vec<u8>>;
}
```

Rules:

1. `read_at` returns exactly `len` bytes or a typed `SourceRead` error.
2. zero-length reads are allowed only if useful for implementation; they must
   not call into lower layers unnecessarily.
3. `offset + len` overflow is rejected before calling the source.
4. reads past `byte_count` are rejected before calling the source.
5. the trait is synchronous for M4-L5. Async object-store adaptation remains a
   later L4/L9 design unless the implementation plan is explicitly amended.
6. the trait must not expose object names, backend handles, paths, or file
   descriptors.

### `BytesTableSource`

In-memory implementation used by tests, direct byte opens, and compaction
outputs.

Suggested shape:

```text
BytesTableSource::new(bytes: impl Into<Vec<u8>>) -> Self
```

Rules:

1. reads are exact slices copied into owned `Vec<u8>` results;
2. byte count is stable for the lifetime of the source;
3. no global state or cache is used.

### `ImmutableTableReader`

Reader over one validated immutable table artifact.

Suggested shape:

```text
ImmutableTableReader::open_bytes(
    identity: TableIdentity,
    bytes: Vec<u8>,
    config: TableReaderConfig,
) -> TableRuntimeResult<Self>

ImmutableTableReader::open_source(
    identity: TableIdentity,
    source: impl TableByteSource,
    config: TableReaderConfig,
) -> TableRuntimeResult<Self>

facts(&self) -> &TableRuntimeFacts
config(&self) -> TableReaderConfig
byte_count(&self) -> u64
get_exact(&self, key: &TableInternalKeyBytes) -> Option<TableRow>
cursor(&self) -> ImmutableTableCursor<'_>
bounded_cursor(&self, bounds: TableKeyBounds) -> BoundedTableCursor<'_>
```

Rules:

1. construction validates the table artifact through L3 before returning;
2. `facts` are derived from decoded header/properties and caller-supplied
   `TableIdentity`;
3. `get_exact` is exact encoded-internal-key lookup, not latest-version lookup;
4. missing keys return `None`;
5. keys outside the table key range return `None`;
6. the first implementation may binary-search materialized rows after full
   validation; later index-guided lookup should read at most the candidate data
   block once cache behavior exists;
7. cursors emit raw `TableRow` values in encoded internal-key order;
8. cursors preserve every row, including tombstones, expired-looking rows, and
   multiple versions for a physical key;
9. reader construction and reads never interpret branch ids, storage-space ids,
   timestamps, tombstones, TTL, or product payload bytes.

### `ImmutableTableCursor`

Raw cursor over one immutable table reader.

Suggested shape:

```text
impl TableCursor for ImmutableTableCursor<'_>
```

Rules:

1. `seek_to_first` positions at the first table row;
2. `seek(target)` positions at the first row with key `>= target`;
3. `advance` crosses data-block boundaries;
4. `current` remains stable until `advance`;
5. repeated seeks after partial iteration or exhaustion reposition from the
   index, not from current state;
6. ordinary empty/missing/exhausted states never panic;
7. decoded block rows are kept in cursor-local state or reader-local cache only
   as an optimization. L5G owns shared block-cache policy.

## L3 Helper Policy

L5F should not duplicate the table byte parser in `table/reader.rs`.

If the existing `decode_immutable_table` API is too eager for a lazy reader,
promote narrowly scoped L3 helper APIs from `format/table/` instead of copying
parsing logic into L5. Useful helpers may include:

1. decoded table metadata/descriptor containing header, properties, index
   entries, data-block offsets, frame lengths, and table byte count;
2. validation of full table bytes without materializing every row into an L5
   table source;
3. decode of one data-block frame from exact bytes;
4. conversion from decoded data-block rows into `TableRow` values;
5. helper methods exposing index entry first/last key bytes and frame ranges.

Any new L3 helper must remain format vocabulary, not table-runtime policy.
L3 reports `FormatError`; L5 wraps it in `TableRuntimeError::DecodeFormat`
unless a narrower table-reader variant is added deliberately.

## Reader Open Flow

The byte-backed open path should be:

```text
validate config
validate caller-supplied identity
validate full M3G artifact through L3
derive TableRuntimeFacts from decoded header/properties
store source, metadata/index, facts, and config
return reader
```

The range-backed open path should be:

```text
read byte_count from source
reject objects shorter than header + footer
read/hash full source bytes or use an L4-provided validation proof
validate full M3G artifact through L3
derive metadata/index/properties and facts
store source, metadata/index, facts, and config
return reader
```

For M4-L5F, prefer reading/hash-validating the full source through
`TableByteSource` because no L4 validation proof exists yet.

## Lookup And Cursor Policy

All lookup and cursor behavior is raw table behavior.

Point lookup:

1. accepts a full `TableInternalKeyBytes`;
2. rejects invalid raw key bytes before lookup if the API accepts bytes;
3. returns the matching row only when encoded internal-key bytes are equal;
4. returns tombstone rows as rows;
5. returns expired-looking rows as rows;
6. returns all physical-key versions only through cursor/range/prefix APIs, not
   through a latest-version helper.

Range and prefix cursors:

1. use `TableKeyBounds` from L5B;
2. include/exclude lower and upper endpoints mechanically;
3. physical-prefix bounds use encoded physical-key bytes;
4. do not cross branch-id or storage-space-id bytes unless those bytes are in
   the prefix/range;
5. do not apply snapshot/as-of/latest filtering;
6. do not collapse duplicate physical-key versions.

## Stats And Cache Interaction

L5F may expose simple reader-local stats if useful for tests:

1. data blocks read;
2. data blocks decoded;
3. source bytes read;
4. source read failures.

Shared cache hit/miss/eviction behavior belongs to L5G. L5F should not create
process-global cache state. The V1 reader config records validation policy only;
reader/cache integration remains a later lazy-reader slice.

## Error Policy

Use existing L5 error vocabulary unless implementation needs a clearer variant.

Expected mapping:

| Condition | Preferred error |
|---|---|
| source range read failure | `TableRuntimeError::SourceRead` |
| short source read | `TableRuntimeError::SourceRead` |
| source byte-count overflow | `TableRuntimeError::InvalidRange` |
| invalid table header/footer/block/index/properties | `TableRuntimeError::DecodeFormat` |
| invalid or unsupported compression | `TableRuntimeError::DecodeFormat` |
| corrupt block checksum | `TableRuntimeError::DecodeFormat` |
| invalid lookup key bytes, if raw bytes are accepted | `TableRuntimeError::InvalidRange` or existing key decode error mapping |
| missing exact key | `None` |
| empty valid table | impossible for M3G; decode error |

Wrapped `FormatError` values must remain reachable through `source()` for
decode failures. L5F must not expose `FormatError` directly from public
table-runtime APIs.

## Source Boundary

Production L5F code may import:

1. `crate::format` M3G reader helpers;
2. `crate::row::StorageRow`;
3. local `crate::table::*` types;
4. standard in-memory data structures such as `Arc`, `Vec`, and `BTreeMap` if
   needed.

Production L5F code must not import:

1. `std::fs`, `std::path`, `std::os::*::fs`, mmap, `File`, or `PathBuf`;
2. `crate::backend`, `crate::layout`, `crate::service`, `crate::branch`,
   `crate::commit`, `crate::lifecycle`, or engine crates;
3. old `crates/storage` modules;
4. product `Value`, primitive names, MessagePack, bincode product payloads, or
   old table value types;
5. `KVSegment`, `SegmentEntry`, `SegmentBuilder`, `pread`, path hash, or
   process-global block-cache vocabulary;
6. snapshot/as-of/latest/fork/rewrite/visibility/TTL filtering vocabulary in
   production reader code.

## Implementation Steps

### L5F.1 - Read And Record Source Evidence

Read the relevant reader paths in `crates/storage/src/segment.rs` and
`crates/storage/src/seekable.rs`. Record the porting decision in
`m4-l5-porting-log.md`.

Preserve:

1. index-guided point lookup;
2. seekable immutable-table cursor behavior;
3. block-boundary cursor movement;
4. corruption routing;
5. compression coverage;
6. tombstone and timestamp preservation.

Retire or defer:

1. local path/file-handle readers;
2. `pread` and mmap assumptions;
3. process-global block cache;
4. bloom/hash-index fast paths;
5. old `STRAKV` bytes;
6. MVCC snapshot/latest lookup;
7. branch rewrite and inherited-layer behavior.

### L5F.2 - Decide L3 Reader Helpers

Audit `format/table/` and add the narrow helper exports needed for metadata and
single-block decode. Do not copy table-format parsing into L5.

The implementation may initially use `decode_immutable_table` if that is the
safest path, but it must still keep the L5 reader API compatible with later
lazy block decode.

### L5F.3 - Implement Source Abstractions

Implement `TableByteSource` and `BytesTableSource` or an equivalent local
range-readable abstraction. Keep it crate-private and free of backend/path
types.

### L5F.4 - Implement Reader Open

Implement byte-backed and source-backed open. Validate full table bytes per V1
checksum rules. Derive `TableRuntimeFacts` from decoded metadata.

### L5F.5 - Implement Exact Lookup

Implement exact encoded-internal-key lookup over validated table rows. Keep the
reader API compatible with later index-guided candidate-block lookup.

### L5F.6 - Implement Immutable Cursor

Implement `ImmutableTableCursor` over the same `TableCursor` contract used by
L5D. Support seek, seek-to-first, advance, current, exhaustion, and block
boundary transitions.

### L5F.7 - Implement Bounded Reader Cursors

Expose range and physical-prefix cursor helpers using `TableKeyBounds` and
`BoundedTableCursor`, or equivalent source-independent cursor filtering.

### L5F.8 - Add Unit And Generated Tests

Implement the L5F test plan. Extend the hidden testkit outcome with an
immutable-reader case counter and require it in the property test.

### L5F.9 - Extend Source Guards

Extend source guards to catch:

1. path/filesystem/backend/service imports in reader code;
2. old segment reader names and old table bytes;
3. process-global cache vocabulary;
4. MVCC/snapshot/latest/fork/rewrite/TTL policy vocabulary;
5. product payload vocabulary.

### L5F.10 - Verification

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

L5F is complete when:

1. valid M3G table artifacts open through byte-backed and range-backed reader
   paths;
2. reader facts match decoded header/properties and the caller identity;
3. exact internal-key lookup works for present, absent, first, middle, last,
   one-block, and multi-block rows;
4. raw full, range, and physical-prefix cursors match the L5D cursor contract;
5. cursors cross data-block boundaries correctly;
6. tombstones, expired rows, duplicate physical-key versions, branch bytes,
   storage-space ids, timestamps, and values are preserved;
7. corrupt/truncated/malformed table bytes surface typed L5 errors with
   underlying `FormatError` sources where applicable;
8. source read failures and short reads surface typed L5 source errors;
9. uncompressed and zstd data blocks are covered;
10. generated property tests include reader scenarios;
11. source guards prove L5F has no path/backend/service/object-name,
    upper-layer, old segment, process-global cache, product, or visibility
    policy leakage;
12. L5I can later plug an L4 table-object source into the reader without
    changing reader semantics.
