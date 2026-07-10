# L5E Implementation Plan: Immutable Table Builder

Status: draft implementation plan

Parent plans:

1. `docs/architecture/implementation-plans/m4-m4t-implementation-plan.md`
2. `docs/architecture/implementation-plans/m4-l5-table-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/l5b-row-key-adapters-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/l5c-mutable-frozen-tables-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/l5d-raw-cursors-merge-cursor-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/l5e-immutable-table-builder-test-plan.md`

## Goal

Port the immutable-table build boundary into storage-next L5 by wrapping the
M3G table encoder behind a table-runtime builder API.

L5E must turn already-sorted L5 table rows into self-identifying V1 immutable
table bytes and table facts that later layers can publish, install, read, or
compact. It must not write files, publish objects, choose object names, install
tables into branch state, or decide retention policy.

L5E is deliberately an adapter over the M3G format implementation:

1. L3 owns the table byte format.
2. L5 owns table-runtime input validation, config application, artifact facts,
   and builder ergonomics.
3. L4 owns durable table object publication.
4. L6-L8 own branch placement, commit visibility, flush scheduling, retention,
   and manifest installation.

## Inputs

1. `docs/architecture/storage/l5-table-runtime.md`
2. `docs/spec/strata-storage-format-v1.md`
3. `docs/architecture/implementation-plans/m4-l5-table-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/l5b-row-key-adapters-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/l5c-mutable-frozen-tables-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/l5d-raw-cursors-merge-cursor-implementation-plan.md`
7. `crates/storage-next/src/format/table/`
8. `crates/storage-next/src/table/builder.rs`
9. `crates/storage-next/src/table/config.rs`
10. `crates/storage-next/src/table/facts.rs`
11. `crates/storage-next/src/table/key.rs`
12. `crates/storage-next/src/table/mutable.rs`
13. `crates/storage/src/segment_builder.rs`

## Existing-Code Source Map

| Current file | Relevant evidence | L5E porting rule |
|---|---|---|
| `crates/storage/src/segment_builder.rs` | Builds immutable segment bytes from sorted entries; computes segment metadata; supports uncompressed/zstd; contains many builder regression cases. | Preserve sorted-input validation, metadata/fact derivation, compression coverage, deterministic output, and splitting evidence. Do not port old `STRAKV` bytes or filesystem writes. |
| `crates/storage-next/src/format/table/artifact.rs` | Provides M3G `encode_immutable_table`, `decode_immutable_table`, table header/footer/index/properties validation, block framing, and compression. | Use as the only byte encoder. Do not duplicate table-format construction in L5. |
| `crates/storage-next/src/table/key.rs` | Provides `TableRow`, encoded internal-key bytes, sorted-unique validation, and row metadata access. | Use for L5 prevalidation and for converting rows to `StorageRow` input expected by L3. |
| `crates/storage-next/src/table/mutable.rs` | Provides `MutableTable` and `FrozenTable` sorted row sources. | Add builder entry points over frozen/mutable rows without reordering. |
| `crates/storage-next/src/table/facts.rs` | Provides `TableIdentity`, `TableKeyRange`, `TableCommitRange`, and `TableRuntimeFacts`. | Build artifact facts from decoded M3G facts. Require caller-supplied identity; L5E must not invent object names. |

## Scope

L5E implements:

1. an immutable table builder type over `TableBuilderConfig`;
2. a built artifact type containing table bytes and table facts;
3. build entry points for sorted `TableRow` slices and frozen table sources;
4. optional convenience entry point for sorted `StorageRow` slices if useful;
5. strict L5 prevalidation for empty input, unsorted rows, and duplicate
   internal keys;
6. conversion from L5 rows to the `StorageRow` slice accepted by the M3G
   encoder;
7. application of target data-block size, rows-per-block, and compression
   config;
8. decode-after-build validation of the produced bytes;
9. fact derivation from decoded table header/properties;
10. deterministic byte output for the same rows and config;
11. testkit generated builder checks;
12. source guards proving no old table bytes, backend IO, object naming, or
   product vocabulary entered the builder layer;
13. M4-L5 porting-log entry for immutable table build mechanics.

L5E does not implement:

1. immutable table reading APIs;
2. range-readable table sources;
3. point lookup, range lookup, or prefix lookup against immutable bytes;
4. object publication or object-name construction;
5. filesystem paths, temp files, rename, fsync, or backend calls;
6. table installation into branch manifests;
7. WAL, checkpoint, lifecycle, recovery, retention, or quarantine behavior;
8. block cache or bloom/filter accelerators;
9. compaction input merging or output splitting policy;
10. MVCC latest selection, snapshot/as-of reads, fork gates, branch rewriting,
    TTL filtering, or tombstone elision;
11. old `STRAKV` or segment-v7 compatibility.

## Target Module Shape

Primary implementation target:

```text
crates/storage-next/src/table/builder.rs
```

Supporting changes:

```text
crates/storage-next/src/table/mod.rs
crates/storage-next/src/table/tests/builder.rs
crates/storage-next/src/testkit/table_runtime.rs
crates/storage-next/tests/table_runtime_properties.rs
crates/storage-next/tests/table_runtime_source_guard.rs
docs/architecture/implementation-plans/M4/m4-l5-porting-log.md
```

Keep all production surfaces `pub(crate)`. L9 owns any future public storage
API.

## Proposed Type Surface

Use these names unless implementation discovers a clearer local convention.
Changing names is acceptable only if the responsibilities stay intact.

### `ImmutableTableBuilder`

Builder configured by `TableBuilderConfig`.

Suggested shape:

```text
ImmutableTableBuilder::new(config: TableBuilderConfig) -> TableRuntimeResult<Self>
ImmutableTableBuilder::from_runtime_config(config: &TableRuntimeConfig) -> TableRuntimeResult<Self>
build_from_rows(identity: TableIdentity, rows: &[TableRow]) -> TableRuntimeResult<BuiltTableArtifact>
build_from_mutable(identity: TableIdentity, table: &MutableTable) -> TableRuntimeResult<BuiltTableArtifact>
build_from_frozen(identity: TableIdentity, table: &FrozenTable) -> TableRuntimeResult<BuiltTableArtifact>
```

Optional if it reduces call-site friction:

```text
build_from_storage_rows(identity: TableIdentity, rows: &[StorageRow]) -> TableRuntimeResult<BuiltTableArtifact>
```

Rules:

1. `new` validates the builder config.
2. `build_from_rows` requires nonempty, strictly sorted, unique internal keys.
3. `build_from_mutable` and `build_from_frozen` must not reorder rows.
4. any `StorageRow` convenience path must convert through `TableRow` or the
   same L5 validation path.
5. output bytes are always M3G table bytes produced by
   `encode_immutable_table`.
6. build errors from L3 are wrapped as `TableRuntimeError::BuildFormat`.
7. decode-after-build errors are wrapped as `TableRuntimeError::DecodeFormat`
   unless implementation chooses a single build-verification error variant.

### `BuiltTableArtifact`

Owned result of one successful immutable table build.

Suggested shape:

```text
BuiltTableArtifact {
    bytes: Vec<u8>,
    facts: TableRuntimeFacts,
}
```

Accessors:

```text
bytes(&self) -> &[u8]
byte_count(&self) -> u64
facts(&self) -> &TableRuntimeFacts
into_bytes(self) -> Vec<u8>
into_parts(self) -> (Vec<u8>, TableRuntimeFacts)
```

Rules:

1. `bytes` is the exact payload L4 table publication should persist.
2. `facts.byte_count` equals `bytes.len()`.
3. `facts.identity` is caller supplied and opaque.
4. `facts.row_count`, `data_block_count`, key range, and commit range are
   derived from decoded M3G header/properties, not independently guessed.
5. the artifact owns bytes; no object name or backend handle is embedded.

## Builder Validation

L5E should prevalidate the mechanical table-runtime contract before calling
L3:

1. reject empty input before allocation-heavy work;
2. reject unsorted input with `TableRuntimeError::InvalidRowOrder`;
3. reject duplicate encoded internal keys with
   `TableRuntimeError::DuplicateInternalKey`;
4. reject invalid builder config through `TableBuilderConfig`;
5. preserve duplicate physical keys at distinct commit versions;
6. preserve tombstones and expired-looking rows;
7. preserve rows from any branch id or storage space id without interpretation;
8. never drop rows for TTL, tombstone, visibility, or retention reasons.

L3 still performs full byte-format validation. L5E prevalidation is for clearer
error vocabulary and to keep the table-runtime contract explicit.

## Block Construction Policy

The M3G encoder currently partitions data blocks by `rows_per_block` and stores
`target_data_block_size` as a table header fact. L5E should use that encoder
directly.

Consequences:

1. do not add a second table-byte construction path in L5;
2. do not reimplement block framing, index encoding, properties encoding, or
   footer checksums in L5;
3. use `TableBuilderConfig::rows_per_block` as the hard rows-per-block limit;
4. pass `TableBuilderConfig::target_data_block_size` through to the encoder;
5. cover one-block and multi-block output by varying `rows_per_block`;
6. if later profiling requires byte-packed blocks, extend the M3G encoder
   deliberately instead of adding ad hoc packing to L5.

## Compression Policy

L5E should pass `TableBuilderConfig::compression` to the M3G encoder.

Rules:

1. `Uncompressed` must work in all builds.
2. `Zstd` must be covered when the crate supports it.
3. output bytes should decode through L3; if L3 exposes block-frame
   compression facts, tests should assert the configured data-block codec.
4. compression choice is a table-build config, not an environment variable.
5. no encrypted or codec-aware table path is introduced in L5E.

## Fact Derivation

After encoding, decode the table bytes with `decode_immutable_table` and build
facts from decoded data.

Facts must include:

1. caller-supplied `TableIdentity`;
2. decoded row count;
3. decoded data block count;
4. decoded min/max encoded internal keys;
5. decoded min/max commit versions;
6. byte count.

The fact derivation must reject impossible decoded facts before returning a
`BuiltTableArtifact`. This keeps L5 facts aligned with the stable M3G bytes and
prevents a future builder optimization from drifting away from the decoder.

## Error Policy

Use the existing L5 error vocabulary unless a new variant is clearly justified.

Expected mapping:

| Condition | Preferred error |
|---|---|
| empty input | `TableRuntimeError::InvalidRange { field: "row_count" }` or a narrowly named equivalent |
| unsorted rows | `TableRuntimeError::InvalidRowOrder` |
| duplicate internal key | `TableRuntimeError::DuplicateInternalKey` |
| invalid builder config | `TableRuntimeError::InvalidConfig` |
| L3 encode failure | `TableRuntimeError::BuildFormat` |
| L3 decode-after-build failure | `TableRuntimeError::DecodeFormat` |
| impossible derived facts | `TableRuntimeError::InvalidRange` |

Do not expose `FormatError` directly from L5 APIs.

## Source Boundary

Production L5E code may import:

1. `crate::format::{encode_immutable_table, decode_immutable_table, TableCompression}`;
2. `crate::row::StorageRow` only as the stable storage-row payload type;
3. `crate::table::*` local table-runtime types;
4. `strata_core_next` scalar ids already carried by storage rows.

Production L5E code must not import:

1. `std::fs`, `std::path`, or temp-file helpers;
2. `crate::backend`, `crate::layout`, `crate::service`, `crate::branch`,
   `crate::commit`, `crate::lifecycle`, or engine crates;
3. old `crates/storage` modules;
4. product value or primitive payload types;
5. object names, table object layout, or publish services.

## Implementation Steps

### L5E.1 - Read And Record Source Evidence

Read the relevant `crates/storage/src/segment_builder.rs` builder tests and
record the porting decision in `m4-l5-porting-log.md`.

Preserve:

1. sorted input builds;
2. tombstone preservation;
3. timestamp/metadata preservation;
4. deterministic output;
5. compression coverage;
6. one-block and multi-block behavior.

Retire or defer:

1. path writes;
2. temp-file protocol;
3. directory fsync;
4. old bloom/filter durable bytes;
5. old `STRAKV` byte layout;
6. split-output compaction builder policy.

### L5E.2 - Implement Builder Types

Replace the placeholder `table/builder.rs` with the builder and artifact
types. Re-export only crate-private surfaces from `table/mod.rs`.

### L5E.3 - Implement Input Collection And Validation

Add helpers that:

1. collect borrowed `TableRow` values without reordering;
2. validate nonempty input;
3. validate strictly sorted unique encoded keys;
4. clone only the `StorageRow` values needed by L3 encoding;
5. keep row metadata unchanged.

### L5E.4 - Call M3G Encoder

Call `encode_immutable_table` with:

1. collected `StorageRow` rows;
2. `target_data_block_size`;
3. `rows_per_block`;
4. `compression`.

Wrap encoder errors in L5 errors.

### L5E.5 - Decode And Derive Facts

Decode the produced bytes. Build `TableRuntimeFacts` from decoded facts and
the caller-supplied identity. Assert byte count fits in `u64`.

### L5E.6 - Add Unit And Generated Tests

Implement the L5E test plan. Extend the hidden testkit outcome with a builder
case counter and require it in the property test.

### L5E.7 - Extend Source Guards

Extend source guards to catch:

1. old table-format magic such as `STRAKV`;
2. path/filesystem/object publish vocabulary in the builder module;
3. old segment builder imports;
4. branch/visibility/TTL policy vocabulary in builder production code.

### L5E.8 - Verification

Run at minimum:

```text
cargo test -p strata-storage-next --locked --lib table::tests::builder
cargo test -p strata-storage-next --features testkit --locked --test table_runtime_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test table_runtime_properties
cargo test -p strata-storage-next --locked --test table_runtime_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo fmt --package strata-storage-next --check
git diff --check
```

## Exit Criteria

L5E is complete when:

1. L5 can build M3G immutable table bytes from sorted L5 rows.
2. Empty, unsorted, and duplicate-key inputs are rejected before successful
   artifact creation.
3. Built bytes decode through L3 and facts match decoded header/properties.
4. One-block and multi-block outputs are tested.
5. Uncompressed and zstd paths are tested where available.
6. Tombstones, expired rows, duplicate physical-key versions, branch bytes,
   storage-space bytes, timestamps, and value bytes round trip.
7. Byte output is deterministic for identical input and config.
8. L5E source code has no object IO, filesystem IO, upper-layer imports,
   old-format bytes, or product visibility policy.
9. The generated property harness includes builder scenarios.
10. The porting log records preserved, changed, deferred, and retired old
    segment-builder behavior.
