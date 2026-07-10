# M4P-L3 Implementation Plan: Durable Format Parity

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4P/m4p-l3-durable-format-parity-test-plan.md`

## Objective

Close the L3 durable-format audit gaps without rewriting storage-next byte
formats, moving lifecycle or service policy into L3, or restoring old
development-format compatibility.

M4P-L3 is primarily a boundary-hardening and test-coverage slice. Storage-next
already has a substantially stronger durable-format layer than old storage:
V1 versions, typed `FormatError`, CRC validation, strict decoders, storage-row
native commit payloads, table codecs, sidecar codecs, and golden fixtures. The
remaining work is to ensure every durable byte format is owned by `format/`,
specified, golden-tested, and reachable through the fuzz/testkit decoder
surface.

The first executable slice is `M4P-L3A`: format-impact decision. It should
prove that the next implementation steps are codec moves and coverage work, not
durable table/WAL/snapshot format changes.

## Audit Finding References

Primary audit source:
`docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`

Relevant sections:

1. `L3. Durable Format / Codec`
2. `Audit Matrix / Durability, Manifest, WAL, And Recovery Mechanics`
3. `Final Parity Matrix And Architecture-Aligned Gap Plan`

Findings closed by this plan:

1. checkpoint row-section payload bytes live in
   `crates/storage-next/src/lifecycle/recovery.rs`;
2. retained-history table-manifest extension payload bytes live in
   `crates/storage-next/src/lifecycle/retained_history_extension.rs`;
3. branch catalog and pending releases manifest formats are implemented but
   under-specified and under-wired in default golden tests;
4. fuzz/testkit routing does not cover the full implemented decoder surface;
5. V1 identity codec behavior is service-local and needs either an explicit
   L3 boundary decision or a small L3 codec API.

Supporting architecture:

1. `docs/architecture/storage/l3-durable-format-codec.md`
2. `docs/spec/strata-storage-format-v1.md`
3. `docs/architecture/storage/storage-space-id-registry.md`
4. `docs/architecture/storage/target-crate-shape-and-test-harness.md`
5. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
6. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`

Performance context:

1. `docs/architecture/perf-tuning/storage-next-serving-path-parity-plan.md`
2. `docs/architecture/perf-tuning/perf-i1-point-read-fix-plan.md`
3. `docs/architecture/perf-tuning/perf-i4-branch-scan-iterator-plan.md`

L3 is not the current point-read or scan fanout bottleneck. This slice must not
add benchmark fast paths, table index structures, branch source-layout changes,
or compaction scheduling changes.

## Predecessors

Required before implementation:

1. parent M4P program plan;
2. M4P test methodology;
3. L3 audit findings listed above;
4. M4P-L1 backend IO boundary fixed enough that L3 can remain byte-only;
5. M4P-L2 object-layout parity fixed enough that L3 can validate persisted
   `ObjectName` strings through L2/object helpers without owning object paths.

No public L9 API predecessor exists. L3 helpers remain crate-private unless a
later diagnostics slice explicitly exposes format facts.

## Layer Ownership Check

M4P-L3 owns durable bytes, not runtime decisions:

1. L1 owns backend IO and opaque object movement. L3 must not open, publish,
   delete, rename, fsync, or map object names to paths.
2. L2 owns object names, prefixes, object families, and object-role
   classification. L3 may decode persisted object-name strings and validate
   them through `ObjectName`/`ObjectLayout`, but must not invent object paths.
3. L4 owns durable services and service error mapping. L3 may report
   `FormatError`; L4 decides whether corrupt bytes are unavailable state,
   recovery debt, quarantine input, or user-visible service failure.
4. L5 owns table reader/writer runtime and compaction output selection. L3 owns
   immutable table artifact bytes and block/frame decode rules.
5. L6 owns branch LSM mechanics and visibility. L3 may encode branch IDs and
   table references in manifests, but does not decide branch existence,
   inheritance, or materialization policy.
6. L8 owns checkpoint scheduling, recovery install policy, retention policy,
   and lifecycle health. L3 owns checkpoint payload bytes and extension payload
   bytes consumed by L8.
7. L9 owns public API behavior. L3 must not expose storage-format internals as
   public product behavior in this slice.

## Existing-Code Source Map

| Current file | Evidence | L3 action |
| --- | --- | --- |
| `crates/storage-next/src/format/mod.rs` | Central format exports, V1 constants, and `FormatError`. | Add exports for any moved checkpoint-row, retained-history, and optional codec-boundary helpers. |
| `crates/storage-next/src/format/snapshot.rs` | Snapshot container/header/section envelope codec and section visitor. | Either add checkpoint row-section payload helpers here or create a sibling L3 module and re-export from `format`. |
| `crates/storage-next/src/format/table_manifest.rs` | Table manifest bytes and extension-section container. | Keep extension-section container here; move retained-history extension payload codec into `format` without moving lifecycle semantics. |
| `crates/storage-next/src/format/branch_catalog_manifest.rs` | Branch catalog V1 codec and ignored golden emitter. | Add default golden assertions and spec coverage. |
| `crates/storage-next/src/format/pending_releases_manifest.rs` | Pending releases V1 codec and ignored golden emitter. | Add default golden assertions and spec coverage. |
| `crates/storage-next/src/format/fuzzing.rs` | Testkit/fuzz routing for many L3 decoders. | Add routes for branch catalog, pending releases, checkpoint row sections, retained-history extension payloads, and any codec API added by this slice. |
| `crates/storage-next/src/format/tests.rs` | Core format golden/corruption tests. | Add default golden assertions for all implemented non-table formats not already asserted. |
| `crates/storage-next/src/format/table/golden_tests.rs` | Immutable table golden tests. | Leave table artifact goldens here unless broad golden harness consolidation is chosen. |
| `crates/storage-next/testdata/goldens/storage-format-v1/` | Stored V1 fixture files. | Add or wire fixtures for moved payload codecs and existing branch/pending manifests. |
| `crates/storage-next/src/lifecycle/recovery.rs` | Defines `SNAPSHOT_ROW_SECTION_KIND`, `SNAPSHOT_ROWS_MAGIC`, `SNAPSHOT_ROWS_VERSION`, `encode_checkpoint_row_section`, and `decode_checkpoint_row_payload`. | Move payload constants and encode/decode to L3. Keep recovery routing, install policy, and health mapping in L8. |
| `crates/storage-next/src/lifecycle/retained_history_extension.rs` | Defines `storage.retained_history`, 24-byte payload codec, and lifecycle conversion to/from timestamp coverage. | Move the payload codec and extension-kind constant to L3. Keep timestamp-coverage conversion in L8 lifecycle code. |
| `crates/storage-next/src/service/wal.rs` | Applies V1 identity codec behavior around WAL bytes. | For M4P-L3A, decide whether to document identity-only no-op application or add a small L3 codec helper. Do not add encryption support in this slice. |

## Old-Code Source Map

Old storage provides evidence about durable-byte responsibilities, not a
compatibility requirement:

| Old source | Behavior to preserve | Storage-next decision |
| --- | --- | --- |
| `crates/storage/src/durability/format/wal_record.rs` | WAL bytes have explicit segment/record versions, framing, and CRCs. | Preserve strict V1 WAL framing and CRC behavior; do not restore old v2/v3 compatibility. |
| `crates/storage/src/durability/format/manifest.rs` | Database manifest bytes are storage-owned physical metadata. | Keep database manifest bytes in L3 and manifest publish/load policy in L4. |
| `crates/storage/src/durability/format/snapshot.rs` | Snapshot files have `SNAP` container mechanics, section envelopes, and footer CRC. | Keep container/envelope mechanics in L3; keep install/recovery policy in L8. |
| `crates/storage/src/durability/format/segment_meta.rs` and `crates/storage/src/durability/format/watermark.rs` | Sidecars and watermark bytes are storage-owned. | Keep V1 sidecar/watermark codecs in L3 with strict corruption errors. |
| `crates/storage/src/durability/format/writeset.rs`, `primitives.rs`, `primitive_tags.rs`, and `payload.rs` | Old commit/snapshot payloads carried primitive/product-shaped data. | Do not port product primitive formats. Storage-next uses storage-row-native commit/checkpoint payloads. |
| `crates/storage/src/durability/codec/` | Identity and AES-GCM codec evidence. | V1 remains identity-only unless encryption configuration/key management is productized later. |
| `crates/storage/src/key_encoding.rs` | Internal key ordering is physical key ascending, commit version descending. | Preserve ordering through `format/key.rs` tests. |
| `crates/storage/src/segment_builder.rs` and `crates/storage/src/segment.rs` | Immutable table bytes had block framing, compression, and corruption checks. | Keep table artifact bytes in `format/table/`; do not make table runtime own durable bytes. |

Do not port:

1. old durable byte versions as compatibility inputs;
2. old `STRAKV` table format;
3. old primitive snapshot DTOs;
4. old MessagePack writeset payloads;
5. AES-GCM codec implementation without productized encryption configuration;
6. service/recovery policy into L3;
7. public API compatibility promises for pre-launch bytes.

## Scope

M4P-L3 implements:

1. an explicit format-impact decision (`M4P-L3A`) proving whether implementation
   is byte-preserving codec migration and coverage work, or a real V1 format
   revision;
2. an L3 checkpoint row-section payload codec;
3. an L3 retained-history extension payload codec;
4. branch catalog and pending releases manifest spec sections and default golden
   assertions;
5. fuzz/testkit routes for every implemented L3 decoder;
6. a V1 identity codec boundary decision, documented and optionally represented
   by a small L3 helper API;
7. source guards or review gates that keep durable payload byte codecs out of
   lifecycle/service modules after this slice.

M4P-L3 does not implement:

1. new table/WAL/snapshot object formats unless `M4P-L3A` proves an unavoidable
   format impact;
2. old-format compatibility;
3. encryption support;
4. object-name or backend changes;
5. WAL publish, manifest update, checkpoint scheduling, recovery install,
   lifecycle retention, or compaction policy changes;
6. LSM source-layout changes;
7. benchmark fast paths;
8. public L9 API changes.

## Execution Plan

### M4P-L3A. Format-Impact Decision

Goal: prove the implementation can preserve current V1 bytes while moving
ownership and strengthening coverage.

Steps:

1. Inventory each moved codec and record its exact current wire bytes:
   - checkpoint row section: `STRR`, version `1`, row count, row length, encoded
     storage rows, section kind `1`;
   - retained-history extension: kind `storage.retained_history`, 24-byte
     payload with version floor, timestamp flag/value, and reserved zeros.
2. Record current decode behavior as well as encoded bytes. In particular,
   decide whether a retained-history payload with timestamp flag `0` and
   nonzero timestamp bytes remains accepted for compatibility with current
   storage-next development builds, or becomes a strict V1 rejection.
3. Compare current tests and fixtures to the L3 audit findings.
4. Decide whether moving the codec changes any byte sequence or strict decode
   behavior.
5. Record the decision in the implementation plan or a small decision note:
   - expected decision: no format revision, only ownership migration and
     coverage;
   - stop condition: if byte output or strict decode behavior changes, stop and
     write a format decision note before implementation continues.

Exit gate:

1. exact moved-byte contract is recorded;
2. non-goals are reaffirmed;
3. no performance claim is attached to L3 work.

### M4P-L3B. Checkpoint Row-Section Codec

Goal: move durable checkpoint row-section payload bytes from L8 recovery into
L3 while preserving recovery behavior.

Implementation target:

1. create `crates/storage-next/src/format/snapshot_rows.rs`, or a similar L3
   module, if keeping the code in `snapshot.rs` would make that file too broad;
2. define crate-private L3 helpers:
   - `SNAPSHOT_ROW_SECTION_KIND`;
   - `encode_snapshot_row_section(rows: &[StorageRow]) -> Result<SnapshotSection, FormatError>`;
   - `decode_snapshot_row_payload(payload: &[u8]) -> Result<Vec<StorageRow>, FormatError>`.
3. Re-export these helpers from `format/mod.rs`.
4. Replace lifecycle recovery imports/calls:
   - `encode_checkpoint_row_section` delegates to or is removed in favor of
     `encode_snapshot_row_section`;
   - `decode_checkpoint_row_payload` is removed from L8 or becomes a thin
     private policy wrapper that only maps `FormatError` to `LifecycleError`.
5. Keep `decode_checkpoint_rows` in L8 because iterating snapshot sections and
   choosing installable sections is recovery policy.

Required invariants:

1. `SnapshotSection::new(SNAPSHOT_ROW_SECTION_KIND, payload)` still validates
   section envelope mechanics in L3;
2. `decode_storage_row` remains the row decoder;
3. malformed payloads produce `FormatError` before L8 maps them;
4. L8 owns max section count, install routing, health facts, and degradation
   class.

### M4P-L3C. Retained-History Extension Payload Codec

Goal: move the 24-byte retained-history payload into L3 while keeping lifecycle
timestamp-coverage semantics in L8.

Implementation target:

1. create `crates/storage-next/src/format/retained_history_extension.rs`, or a
   focused submodule near `table_manifest`;
2. define L3 helpers and facts:
   - `RETAINED_HISTORY_EXTENSION_KIND`;
   - `RETAINED_HISTORY_EXTENSION_PAYLOAD_LEN`;
   - `RetainedHistoryExtensionPayload { retained_version_floor, retained_timestamp_floor }`;
   - `encode_retained_history_extension_payload`;
   - `decode_retained_history_extension_payload`;
   - helper to build/read a `TableManifestExtensionSection` if that keeps
     extension container code centralized in L3.
3. Update `lifecycle/retained_history_extension.rs` so it only maps between
   `BranchTimestampCoverage` and the L3 payload fact.
4. Preserve existing extension behavior:
   - kind remains `storage.retained_history`;
   - `preserve_on_rewrite` remains true;
   - timestamp flag values remain 0/1;
   - reserved bytes must remain zero.

Required invariants:

1. lifecycle still decides when retained-history facts are emitted;
2. L3 does not import `BranchTimestampCoverage`;
3. invalid length, invalid flag, and nonzero reserved bytes remain strict
   `FormatError` cases.

### M4P-L3D. Manifest-Family Spec And Goldens

Goal: ensure implemented manifest-family byte formats are described and asserted
by normal tests.

Implementation target:

1. update `docs/spec/strata-storage-format-v1.md` with sections for:
   - branch catalog manifest;
   - pending releases manifest.
2. Include:
   - magic bytes;
   - version;
   - database id;
   - manifest sequence;
   - entry counts and ordering;
   - branch id/generation/status fields;
   - parent fields for branch catalog;
   - released table identity fields for pending releases;
   - CRC/checksum or trailing-data behavior if present;
   - future/pre-V1 handling.
3. Add default golden assertions in `format/tests.rs` or adjacent format tests
   for these existing fixtures:
   - `branch-catalog-manifest-empty.hex`;
   - `branch-catalog-manifest-single-active.hex`;
   - `branch-catalog-manifest-active-and-deleted.hex`;
   - `branch-catalog-manifest-with-parent.hex`;
   - `pending-releases-manifest-empty.hex`;
   - `pending-releases-manifest-single.hex`;
   - `pending-releases-manifest-multi.hex`.
4. Ensure ignored golden emitters remain regeneration-only and do not replace
   default fixture assertions.

Required invariants:

1. no manifest service policy moves into L3;
2. branch lifecycle semantics remain in L6/L8;
3. golden assertions compare stable bytes, not only roundtrip equality.

### M4P-L3E. Fuzz/Testkit Decoder Surface

Goal: make every implemented L3 decoder reachable through the documented fuzz
and testkit routing surface.

Implementation target:

1. update `format/fuzzing.rs` with routes for:
   - branch catalog manifest;
   - pending releases manifest;
   - checkpoint row-section payload;
   - retained-history extension payload.
2. Update the fuzz/testkit documentation in
   `docs/architecture/storage/target-crate-shape-and-test-harness.md` if it
   enumerates decoder names.
3. Add seeded corpus guidance or fixture references for every new route.
4. If a route decodes an extension section, route the payload decoder directly
   and keep table-manifest extension-section container fuzzing under the table
   manifest route.

Required invariants:

1. fuzz routes return success/failure only; they do not apply service or
   lifecycle policy;
2. payload decoders must reject trailing bytes and invalid reserved bytes;
3. no route allocates unbounded memory from a hostile input.

### M4P-L3F. V1 Codec Boundary

Goal: prevent codec handling from fragmenting across services.

Decision options:

1. Document-only for V1:
   - V1 supports identity codec only;
   - L3 validates exact codec IDs in format headers;
   - service application is no-op by design;
   - encryption/compression policy is deferred.
2. Small L3 helper API:
   - `StorageCodecId`;
   - `decode_codec_id`;
   - `apply_identity_codec` or `decode_payload_with_codec`;
   - future unsupported codec errors map to `FormatError`.

Recommended first slice:

Choose the document-only V1 decision unless current WAL/snapshot/table services
already duplicate enough codec switching to justify an API. The goal is to stop
fragmentation, not invent an encryption abstraction.

Exit gate:

1. L3 architecture/spec records the V1 identity-only boundary;
2. any service-local codec checks are either removed through a helper or
   explicitly documented as no-op application after L3 header validation.

### M4P-L3G. L3 Source Guard

Goal: prevent future durable payload codecs from drifting back into lifecycle or
service code.

Implementation target:

1. add a narrow source guard under `crates/storage-next/tests/` or extend an
   existing architecture guard;
2. fail on production lifecycle/service files that define durable byte-format
   markers such as:
   - `*_MAGIC`;
   - `*_FORMAT_VERSION`;
   - `*_HEADER_SIZE`;
   - `to_le_bytes` / `from_le_bytes` blocks paired with storage-format field
     names;
   - hand-rolled payload encode/decode loops for snapshot sections or manifest
     extensions.
3. Allow:
   - `src/format/**`;
   - L1 backend byte movement over opaque object bytes;
   - tests and testkit fixtures;
   - short non-durable numeric conversions in lifecycle/service code.

Required invariants:

1. the guard must include seeded failing probes;
2. the guard must not block service code from mapping `FormatError` into service
   or lifecycle errors;
3. the guard must not block L8 from deciding when to include a section or
   extension.

## Testing Methodology

The detailed test plan is in:
`docs/architecture/implementation-plans/M4P/m4p-l3-durable-format-parity-test-plan.md`

Minimum verification for the full slice:

1. focused format unit tests for moved codecs;
2. strict corruption tests for every new L3 decoder;
3. golden fixture assertions for moved/newly wired formats;
4. existing lifecycle recovery tests after moving checkpoint row-section bytes;
5. existing row-pruning/retained-history tests after moving extension bytes;
6. fuzz/testkit route compile checks;
7. source guard tests;
8. `cargo clippy -p strata-storage-next --lib --features perf-trace -- -D warnings`;
9. `cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked`.

## Stop Conditions

Stop implementation and write a decision note if:

1. any proposed move changes V1 bytes;
2. a moved codec requires lifecycle/service state to decode bytes;
3. L3 would need to import lifecycle, service, branch runtime, backend, or API
   modules;
4. golden fixtures cannot be asserted without regeneration;
5. fuzz routes require unbounded allocation;
6. a codec-boundary API grows into encryption/key-management design;
7. performance work is proposed as part of L3.

## Closeout Requirements

M4P-L3 is complete when:

1. checkpoint row-section payload bytes are owned by L3;
2. retained-history extension payload bytes are owned by L3;
3. branch catalog and pending releases manifest formats are fully specified;
4. default tests assert their existing golden fixtures;
5. all implemented L3 decoders are fuzz/testkit routable;
6. V1 identity codec behavior is explicitly documented or represented by an L3
   helper;
7. source guards or equivalent tests prevent durable payload codecs from
   returning to lifecycle/service modules;
8. lifecycle/service behavior remains unchanged except for consuming L3 codecs;
9. no old-format compatibility or performance claim is added.
