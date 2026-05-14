# M3 Porting Log

Status: active during M3

## Purpose

This document records how lower storage behavior moves from the current
`crates/storage` implementation into `crates/storage-next` during M3.

The M3 implementation plan owns order and scope. This log owns the porting
audit trail: what was read, what was preserved, what changed, what was deferred,
and what old code became eligible for retirement.

## Rules

1. Add or update a slice entry before changing storage-next implementation code.
2. Prefer porting, splitting, and tightening existing storage behavior over
   fresh implementation.
3. Fresh implementation is allowed only when the entry records why existing
   behavior is obsolete, out of scope, or inconsistent with V1.
4. Do not delete old storage code until replacement tests exist and workspace
   references are gone.
5. If old code cannot be deleted because current crates still depend on it,
   record it as legacy-retained instead of adding compatibility glue to
   storage-next.
6. Treat old tests as evidence, not authority. Preserve the cases that still
   match V1 semantics; reject or rewrite cases that freeze obsolete behavior.

## Entry Template

```md
## <Slice>: <Title>

### Current Files Read

- `crates/storage/src/...`

### Behavior Preserved

- ...

### Intentional V1 Changes

- ...

### Deferred

- ...

### Tests Ported Or Added

- ...

### Retirement

- Deleted:
- Legacy-retained:
- Follow-up:
```

## Baseline Source Map

| Target area | Current source material | Initial disposition |
|---|---|---|
| Backend filesystem behavior | `crates/storage/src/durability/layout.rs`, `crates/storage/src/manifest.rs`, `crates/storage/src/segment_builder.rs`, `crates/storage/src/durability/wal/writer.rs`, `crates/storage/src/durability/disk_snapshot/writer.rs` | Port proven filesystem behavior behind storage-next backend traits. |
| Object layout | `crates/storage/src/durability/layout.rs`, `crates/storage/src/layout.rs`, `crates/storage/src/quarantine.rs`, `crates/storage/src/segmented/quarantine_protocol.rs` | Move object-family and path construction into `storage-next::layout`. |
| Durable format codec | `crates/storage/src/durability/format/*`, `crates/storage/src/key_encoding.rs`, `crates/storage/src/stored_value.rs`, `crates/storage/src/durability/payload.rs`, `crates/storage/src/segment.rs`, `crates/storage/src/segment_builder.rs` | Port durable byte decisions in codec-sized pieces and lock with golden vectors. |
| WAL service | `crates/storage/src/durability/wal/*`, `crates/storage/src/durability/format/wal_record.rs`, `crates/storage/src/durability/recovery.rs`, `crates/storage/src/durability/recovery_bootstrap.rs` | Preserve fault and recovery mechanics that match V1; keep public transaction semantics out. |
| Manifest and watermark service | `crates/storage/src/durability/format/manifest.rs`, `crates/storage/src/durability/format/watermark.rs`, `crates/storage/src/manifest.rs`, `crates/storage/src/durability/commit_adapter.rs` | Port durable manifest mechanics; defer branch and commit meaning. |
| Snapshot and checkpoint service | `crates/storage/src/durability/disk_snapshot/*`, `crates/storage/src/durability/format/snapshot.rs`, `crates/storage/src/durability/checkpoint_runtime.rs`, `crates/storage/src/durability/decoded_snapshot_install.rs` | Port container and envelope mechanics; do not reintroduce engine primitive snapshot semantics. |
| Quarantine and recovery classification | `crates/storage/src/quarantine.rs`, `crates/storage/src/segmented/quarantine_protocol.rs`, `crates/storage/src/segmented/recovery.rs`, `crates/storage/src/durability/recovery.rs` | Port as storage diagnostics and recovery classifications. |
| Existing lower-layer tests | `crates/storage/src/segmented/tests/publish_failures.rs`, `crates/storage/src/segmented/tests/quarantine_reconciliation.rs`, `crates/storage/src/segmented/tests/post_restart_branch.rs`, `crates/storage/src/segmented/tests/gc_under_degradation.rs`, `crates/storage/src/segmented/tests/lifecycle.rs` | Mine for M3T cases; do not preserve obsolete behavior blindly. |

## Slice Entries

## M3A1: Backend Capability Validation

### Current Files Read

- `crates/storage/src/durability/layout.rs`
- `crates/storage/src/durability/wal/mode.rs`
- `crates/storage/src/durability/wal/writer.rs`
- `crates/storage/src/manifest.rs`
- `crates/storage/src/segment_builder.rs`
- `crates/storage-next/src/backend/mod.rs`
- `crates/storage-next/src/backend/memory.rs`
- `crates/storage-next/src/backend/local_fs.rs`

### Behavior Preserved

- Cache mode is allowed without WAL, manifest, durable sync, or writer-lock
  capability.
- Durable local mode requires a stronger contract than basic object IO.
- Local filesystem code remains the only place that touches raw filesystem
  APIs in storage-next.
- Memory/cache and current localfs backend behavior remains basic object IO
  only until durable publisher, sync, and writer-lock mechanics are implemented.

### Intentional V1 Changes

- Follower-mode paths from the old layout are not carried into capability
  validation.
- Capability validation is backend-mode based, not feature-name based.
- Localfs compiling does not imply durable local mode is supported.

### Deferred

- Durable publish, sync, and writer-lock operations move to later M3D/M3E
  slices.
- Lifecycle/open integration waits for M4/L8.
- Object-store/OpenDAL durable mode remains an unsupported candidate.

### Tests Ported Or Added

- Add storage-next capability validation tests for cache, durable local
  standard, durable local always, and object durable candidate requirements.
- Add conformance coverage that current memory/localfs backends reject durable
  modes through the same validation function.

### Retirement

- Deleted: none.
- Legacy-retained: current `crates/storage` durability layout, WAL, manifest,
  and segment builder code still serve old storage consumers.
- Follow-up: M3D/M3E should retire or mark old durable publish/service code as
  replacement services become tested owners.

## M3B1: Object Families And Reserved Object Paths

### Current Files Read

- `crates/storage/src/durability/layout.rs`
- `crates/storage/src/layout.rs`
- `crates/storage/src/quarantine.rs`
- `crates/storage/src/segmented/quarantine_protocol.rs`
- `crates/storage-next/src/object/mod.rs`
- `crates/storage-next/src/layout/mod.rs`
- `docs/architecture/storage-next/l2-object-layout.md`

### Behavior Preserved

- Canonical storage objects stay database-relative.
- WAL, table, snapshot, manifest, temporary, quarantine, lock, and metadata
  locations have one layout owner.
- Quarantine inventory remains separate from source table objects.
- Follower-state and follower-audit paths are not part of the target layout.

### Intentional V1 Changes

- The layout now exposes object names and prefixes, not filesystem paths.
- Old filesystem names such as `MANIFEST`, `wal-NNNNNN.seg`,
  `snap-NNNNNN.chk`, `segments.manifest`, `quarantine.manifest`, and
  `__quarantine__/` are treated as source evidence, not target names.
- Branch IDs, table IDs, snapshot IDs, and operation IDs are accepted as
  validated layout components for now; exact durable ID types remain deferred.
- WAL segment IDs and snapshot IDs now use fixed-width lowercase hex object-name
  components, and table levels use fixed-width `lNNNN` components in the range
  `0..=9999` for lexical ordering.

### Deferred

- Durable publish and cleanup behavior for `tmp/` waits for L4 service work.
- Writer lock protocol waits for backend/lifecycle service work.
- Branch/table manifest meaning waits for later table and branch milestones.
- Format bytes for manifest, WAL, snapshot, and quarantine wait for M3C/M3E.

### Tests Ported Or Added

- Add constructor tests for every reserved object family.
- Add prefix tests for listing WAL, tables, snapshots, temporary objects,
  quarantine, locks, and metadata.
- Add validation tests proving invalid components cannot create traversal,
  absolute, empty-component, or trailing-slash names.
- Add absence tests for follower-state and follower-audit names.

### Retirement

- Deleted: none.
- Legacy-retained: old filesystem layout and quarantine protocol still serve
  old storage consumers.
- Follow-up: M3D/M3E should retire or mark old publish/quarantine layout code
  after new services own the behavior with fault tests.

## M3B2: Layout Property Tests And Ad Hoc Construction Guard

### Current Files Read

- `crates/storage/src/durability/layout.rs`
- `crates/storage/src/layout.rs`
- `crates/storage/src/quarantine.rs`
- `crates/storage/src/segmented/quarantine_protocol.rs`
- `crates/storage-next/src/layout/mod.rs`
- `crates/storage-next/src/backend/local_fs.rs`
- `crates/storage-next/tests/object_layout_properties.rs`

### Behavior Preserved

- Object names remain database-relative, validated, and ASCII-only.
- WAL and snapshot IDs retain lexical ordering that matches numeric ordering.
- Table, temporary, and quarantine objects remain under their branch or
  operation prefixes.
- The local filesystem backend remains the owner of object-name-to-filesystem
  path mapping.

### Intentional V1 Changes

- Property coverage now enforces the layout invariants instead of relying only
  on example constructor tests.
- Source-level guard coverage keeps reserved durable layout names owned by the
  layout module; future service code should consume layout constructors instead
  of hardcoding object-family strings.

### Deferred

- Branch ID and table ID durable atom types remain deferred until branch/table
  implementation slices.
- Durable publish cleanup and quarantine recovery protocol remain deferred to
  M3D/M3E.

### Tests Ported Or Added

- Add generated tests for WAL and snapshot lexical ordering.
- Add generated tests for table, temporary, and quarantine prefix ownership.
- Add generated invalid-component tests for layout constructors.
- Move layout unit coverage into `crates/storage-next/src/layout/tests.rs` so
  the production layout module stays below the V1 file-size threshold.
- Add a source guard that scans production storage-next code for reserved
  layout-name construction outside the layout/object/local-fs boundary.

### Retirement

- Deleted: none.
- Legacy-retained: current `crates/storage` layout and quarantine code still
  serve old storage consumers.
- Follow-up: M3D/M3E should replace old durable path construction with the new
  layout constructors as each service is ported.

## M3C1: Key, Row, Storage-Space, And Stored-Value Format

### Current Files Read

- `crates/storage/src/key_encoding.rs`
- `crates/storage/src/stored_value.rs`
- `crates/storage/src/durability/format/primitive_tags.rs`
- `crates/storage/src/durability/format/primitives.rs`
- `crates/storage/src/durability/format/writeset.rs`
- `crates/storage/src/durability/payload.rs`
- `docs/architecture/storage-next/l3-durable-format-codec.md`
- `docs/architecture/storage-next/storage-space-id-registry.md`
- `docs/architecture/engine-next/storage-space-id-registry.md`
- `docs/spec/strata-storage-format-v1.md`

### Behavior Preserved

- Internal keys keep the current order-preserving shape:
  branch id bytes, NUL-terminated space, one storage-space byte,
  byte-stuffed user key, and big-endian bitwise-NOT commit version.
- User-key byte-stuffing still encodes `0x00` as `0x00 0x01` and terminates
  the user key with `0x00 0x00`.
- Versions for one physical key still sort newest first without a separate
  pointer structure.
- Stored rows still carry commit version, timestamp, value bytes, tombstone
  state, and expiry metadata.

### Intentional V1 Changes

- The old primitive-shaped `TypeTag` byte becomes an opaque
  `storage_space_id`. Storage owns only the range split and does not map
  engine-owned bytes to KV, JSON, event, vector, graph, or search.
- Old primitive snapshot tags are treated as current-code evidence only. V1
  engine-owned rows start at `0x20`; `0x01` is storage-owned commit timeline
  space.
- Storage row payloads use a storage-native binary format instead of
  MessagePack or `EntityRef`-shaped writesets.
- Expiry is encoded as an absolute microsecond timestamp, not the old
  in-memory TTL duration packing.
- Tombstone row decoders reject non-empty value bytes or nonzero expiry.

### Deferred

- Commit payload batching and WAL record framing wait for M3C3.
- Manifest, watermark, and segment metadata codecs wait for M3C2.
- Snapshot container and section codecs wait for M3C4.
- Table block/header/footer encoding waits for later M3C/M4 table slices.
- Engine-owned storage-space assignments are validated by engine-next later;
  storage-next validates only the storage-vs-engine range split.

### Tests Ported Or Added

- Add unit coverage for storage-space range validation.
- Add internal-key round-trip and newest-first ordering tests.
- Add malformed decode tests for invalid storage-space id, trailing key bytes,
  invalid row version, nonzero row flags, invalid tombstone byte, tombstone
  value payload, and tombstone expiry.
- Add checked-in golden vectors for ordinary internal key bytes, escaped
  zero-byte internal key bytes, a put row, and a tombstone row.
- Extend the format golden integration harness to require those fixtures.

### Retirement

- Deleted: none.
- Legacy-retained: old `crates/storage` key encoding, stored value, primitive
  snapshot DTO, writeset, and MessagePack transaction payload code still serve
  old storage consumers.
- Follow-up: M3C2-M3C4 should continue replacing old durable byte owners one
  object family at a time before M3D/M3E services consume them.

## M3C2: Manifest, Watermark, And Segment Metadata Format

### Current Files Read

- `crates/storage/src/durability/format/manifest.rs`
- `crates/storage/src/durability/format/watermark.rs`
- `crates/storage/src/durability/format/segment_meta.rs`
- `crates/storage/src/manifest.rs`
- `docs/architecture/storage-next/l3-durable-format-codec.md`
- `docs/architecture/storage-next/l4-log-manifest-snapshot-services.md`
- `docs/spec/strata-storage-format-v1.md`

### Behavior Preserved

- The database manifest remains storage-physical metadata: database id, codec
  id, active WAL segment, snapshot watermark, snapshot id, and flushed-through
  commit id.
- The manifest and segment metadata formats keep CRC32 protection over all
  bytes before the checksum.
- Snapshot watermark bytes preserve the compact current shape:
  `has_data`, optional snapshot id, optional commit-version watermark, and
  update timestamp.
- Segment metadata tracks segment id, timestamp range, commit-version
  range, and record count for fast coverage checks.

### Intentional V1 Changes

- Stable V1 manifest format starts at version `1`; pre-V1 development
  manifest versions are rejected by the normal decoder.
- Manifest, watermark, and segment metadata decoders reject trailing bytes
  instead of accepting extension data implicitly.
- Manifest codec ids are bounded and validated before allocation-heavy decode
  work.
- Manifest recovery facts are stricter than the old permissive decoder:
  active WAL segment must be nonzero, and snapshot id plus snapshot watermark
  must appear as a pair.
- Present snapshot watermarks reject snapshot id `0`; empty watermark remains
  the one-byte `00` encoding.
- Segment metadata version `0` is reported as pre-V1, not future.
- Filesystem persistence, temp files, rename, and fsync behavior stay out of
  the format layer and move to M3D/M3E services.

### Deferred

- WAL envelope and record codecs wait for M3C3.
- Snapshot container and section codecs wait for M3C4.
- Manifest load/update/publish service mechanics wait for M3E1.
- Segment metadata sidecar publication and recovery fallback policy wait for
  M3E2/M3E4.

### Tests Ported Or Added

- Add strict round-trip and malformed-input tests for manifest, watermark, and
  segment metadata bytes.
- Add checksum mismatch, invalid magic, pre-V1/future-version, invalid codec,
  invalid recovery facts, truncation, and trailing-data coverage where
  applicable.
- Add checked-in golden vectors for an identity-codec manifest, empty and
  present snapshot watermarks, and a segment metadata sidecar.

### Retirement

- Deleted: none.
- Legacy-retained: old manifest, watermark, and segment metadata codecs still
  serve old storage consumers.
- Follow-up: M3E1/M3E2 should consume these V1 codecs through durable services
  and then record old service-code retirement disposition.

## M3C3: WAL Segment, Envelope, And Record Format

### Current Files Read

- `crates/storage/src/durability/format/wal_record.rs`
- `crates/storage/src/durability/wal/reader.rs`
- `crates/storage/src/durability/wal/writer.rs`
- `crates/storage/src/durability/commit_adapter.rs`
- `crates/storage/src/durability/payload.rs`
- `crates/storage/src/durability/codec/`
- `docs/architecture/storage-next/l3-durable-format-codec.md`
- `docs/architecture/storage-next/l4-log-manifest-snapshot-services.md`
- `docs/spec/strata-storage-format-v1.md`

### Behavior To Preserve

- WAL segment headers keep the `STRA` magic, segment id, database id, and CRC32
  over the first 32 bytes.
- WAL record bytes keep a length prefix protected by CRC32 before the decoder
  trusts the record length.
- WAL record payload CRC still covers the inner record payload fields.
- The codec-aware outer envelope remains a separate frame:
  `encoded_len`, `encoded_len_crc32`, and encoded inner record bytes.

### Intentional V1 Changes

- Stable V1 segment and record versions start at `1`; pre-launch development
  versions are rejected by the normal decoder instead of being migrated.
- WAL records carry a `commit_version` field rather than reintroducing a public
  transaction id atom into core-next.
- WAL record `commit_payload` remains opaque bytes in M3C3. The row-native
  commit payload format lands in a later commit-runtime slice.

### Deferred

- Filesystem segment append/read/truncate mechanics wait for M3E2.
- Codec application is owned by the WAL service. M3C3 only frames already
  encoded bytes in the outer envelope.
- Recovery scan-forward, lossy recovery, and corruption classification wait for
  L4/L8 service work.

### Tests To Port Or Add

- Add strict round-trip and malformed-input tests for segment header, outer
  envelope, and inner WAL record bytes.
- Add checksum mismatch, invalid magic, pre-V1/future-version, segment id
  mismatch, truncated frame, and multiple-record sequence coverage.
- Add checked-in golden vectors for a segment header, empty-payload record,
  non-empty-payload record, and an identity-encoded outer envelope.

### Retirement

- Deleted: none.
- Legacy-retained: old WAL segment and record codecs still serve old storage
  consumers.

## M3C4: Snapshot Container And Section Format

### Current Files Read

- `crates/storage/src/durability/format/snapshot.rs`
- `crates/storage/src/durability/disk_snapshot/reader.rs`
- `crates/storage/src/durability/disk_snapshot/writer.rs`
- `docs/architecture/storage-next/l3-durable-format-codec.md`
- `docs/spec/strata-storage-format-v1.md`

### Behavior Preserved

- Snapshot containers keep the `SNAP` magic and 64-byte fixed header.
- Codec id bytes still immediately follow the fixed header and are included in
  container integrity protection.
- Snapshot sections remain length-delimited by a 9-byte envelope:
  one section kind byte and an eight-byte little-endian payload length.
- A footer CRC32 covers the header, codec id, every section envelope, and every
  section payload byte before install trusts the container.
- Truncated section envelopes, truncated section payloads, checksum mismatches,
  invalid magic, invalid UTF-8 codec ids, and trailing partial section bytes
  fail decode instead of being ignored.
- Large section payloads can be inspected through a borrowed section visitor
  instead of forcing an eager copy of every payload in the container.

### Intentional V1 Changes

- Stable V1 snapshot format starts at version `1`; current format version `2`
  is rejected as pre-V1 development evidence.
- The recovery watermark is a storage commit version, not a transaction id.
- Snapshot id `0` is invalid.
- Header reserved bytes must be zero.
- Storage-next validates only the mechanical section envelope. Old primitive
  snapshot tags and DTO payloads are not ported into storage-next.
- The materialized container decoder is bounded by section-count and total
  payload limits; future large-snapshot services should use the borrowed visitor
  rather than the materialized convenience decoder.

### Deferred

- Snapshot object publication, temporary-object cleanup, and manifest
  watermark update mechanics wait for M3E3.
- Row-native snapshot payload construction and install semantics wait for
  later table/recovery and engine persistence slices.
- Codec application remains a service concern; M3C4 records and validates the
  codec identity bytes but does not transform section payloads.

### Tests Ported Or Added

- Add strict round-trip and malformed-input tests for snapshot headers,
  section envelopes, and whole containers.
- Add checksum mismatch, pre-V1/future-version, zero snapshot id, reserved-byte,
  invalid codec id, truncated codec id, truncated section, and trailing partial
  section coverage.
- Add length-overflow, max codec id, codec NUL, materialized-payload-limit, and
  section-count-limit coverage.
- Add checked-in golden vectors for an identity-codec snapshot header, an empty
  section envelope, and a single-section container with footer CRC.

### Retirement

- Deleted: none.
- Legacy-retained: old snapshot reader/writer and primitive snapshot DTOs still
  serve old storage consumers.
- Follow-up: M3E3 should consume these V1 codecs from the durable snapshot
  service, then record old snapshot service retirement disposition.

## M3C5: First Format Fuzz Package

### Current Files Read

- `crates/storage-next/fuzz/README.md`
- `crates/storage-next/fuzz/fuzz_targets/README.md`
- `crates/storage-next/src/format/*`
- `crates/storage-next/src/testkit/mod.rs`
- `docs/architecture/storage-next/target-crate-shape-and-test-harness.md`
- `docs/architecture/storage-next/l3-durable-format-codec.md`

### Behavior Preserved

- There is no old cargo-fuzz package to port from current storage.
- M3C5 preserves the M2 decision that fuzz targets must not claim coverage
  until real byte parsers exist.
- Fuzz access stays outside the production API by going through the hidden
  `testkit` feature.

### Intentional V1 Changes

- `crates/storage-next/fuzz/` is now a real cargo-fuzz package rather than
  documentation-only scaffolding.
- The first targets exercise durable decoder families for manifest bytes,
  snapshot envelopes, storage rows, and WAL records.
- The fuzz package disables default features so format fuzzing covers the
  memory/cache-compatible build surface rather than pulling in local filesystem
  support.

### Deferred

- Object-name, table-block, commit-payload, timeline-row, and recovery
  inventory fuzz targets wait for the corresponding parsers or services.
- Scheduled fuzzing and corpus management remain outside M3C5.
- Wrong-error-class assertions wait until storage error classification is wired
  through the durable services.

### Tests Ported Or Added

- Add `crates/storage-next/fuzz/Cargo.toml` with cargo-fuzz metadata and four
  initial fuzz binaries.
- Add a hidden testkit routing surface that sends arbitrary byte slices through
  selected durable format decoders.
- Extend the testkit boundary probe so external testkit consumers can compile
  the format fuzz routing surface only when `testkit` is enabled.

### Retirement

- Deleted: none.
- Legacy-retained: old storage has no fuzz package to retire.
- Follow-up: later M3D/M3E slices should add fuzz targets for service-level
  recovery inputs once those inputs exist.

## M3D1: Object Publish Primitive

### Current Files Read

- `crates/storage/src/durability/format/manifest.rs`
- `crates/storage/src/manifest.rs`
- `crates/storage/src/segment_builder.rs`
- `crates/storage/src/durability/disk_snapshot/writer.rs`
- `crates/storage/src/quarantine.rs`
- `crates/storage-next/src/backend/mod.rs`
- `crates/storage-next/src/backend/local_fs.rs`
- `crates/storage-next/src/backend/memory.rs`
- `crates/storage-next/src/layout/mod.rs`
- `crates/storage-next/src/service/mod.rs`

### Behavior Preserved

- Local durable publication preserves the proven write-temp, sync-temp,
  rename-to-final, and sync-parent-directory sequence used by current
  MANIFEST, segment manifest, snapshot, quarantine, and table publication
  paths.
- Durable create now uses an atomic no-clobber link step so a race cannot
  replace an object created after the preflight check.
- Failures before the final publish step leave the final object untouched and
  clean up the unique temporary object path on the write/sync path.
- A failure after rename but before parent-directory sync is not collapsed into
  a generic write error; it remains a distinct published-but-unconfirmed
  window.
- Cache publication remains explicitly non-durable.

### Intentional V1 Changes

- The durable publish sequence becomes a backend-owned object publish primitive
  consumed by L4 services instead of repeated ad hoc filesystem code.
- Local filesystem temporary files are backend-internal implementation details;
  upper layers name final objects through `ObjectName` and layout constructors.
- The first durable local implementation only claims durable publish/sync on
  Unix-like platforms where the POSIX rename/link and parent-directory sync
  sequence is available. Other local filesystem targets can still compile but
  must not advertise durable local publication until they provide an equivalent
  backend primitive.
- Cache mode receives a non-durable publish path that reports non-durable facts
  rather than pretending to satisfy local durability requirements.

### Deferred

- Single-writer lock support remains deferred; durable local open should still
  fail capability validation until the writer guard exists.
- Non-Unix durable local publish remains deferred until the backend can provide
  an atomic replace/no-clobber primitive plus durable directory metadata sync.
- WAL append, manifest service, snapshot service, table manifest service, and
  quarantine service mechanics remain M3E work.
- Full fault-window integration tests for injected write, sync, rename, parent
  sync, and cleanup failures remain M3TC work.

### Tests Ported Or Added

- Add backend and service tests for local durable publish success, create
  precondition failure, replace-over-existing behavior, temporary-file cleanup,
  publish-specific symlink rejection, stale temporary file handling, cache
  non-durable publication, and unsupported publish modes.
- Extend backend conformance and testkit fault surfaces to recognize publish
  operations without making publish a product API.

### Retirement

- Deleted: none.
- Legacy-retained: old publish call sites remain until manifest, WAL,
  snapshot, table, and quarantine services consume the storage-next publisher.
- Follow-up: M3E slices should retire duplicated current-storage publish
  sequences as each service lands.

## M3TC1: Durable Publish Fault Windows

### Current Files Read

- `crates/storage/src/manifest.rs`
- `crates/storage/src/quarantine.rs`
- `crates/storage/src/segment_builder.rs`
- `crates/storage/src/test_hooks.rs`
- `crates/storage/src/segmented/tests/publish_failures.rs`
- `crates/storage-next/src/backend/local_fs.rs`
- `crates/storage-next/src/backend/publish.rs`
- `crates/storage-next/tests/service_fault_windows.rs`

### Behavior Preserved

- Failures before the final publish step remain classified as
  before-visibility failures and preserve the previous final object.
- Parent-directory sync failures after the final publish step remain a distinct
  visible-but-durability-unconfirmed state.
- Temporary objects are cleaned up after injected temp-write, temp-sync, and
  final-publish failures.

### Intentional V1 Changes

- Fault injection for the lower publish primitive is backend-local test-only
  state instead of the old crate-global manifest-specific test hook.
- The V1 tests target the backend object-publish primitive directly; manifest,
  WAL, snapshot, table-manifest, and quarantine service recovery tests land
  when those services consume the publisher.

### Deferred

- Process crash-window tests remain M3E/M4 work because M3TC1 injects
  classified operation failures, not killed-process recovery points.
- WAL append, manifest update, snapshot publish, and quarantine fault windows
  remain later M3TC slices.

### Tests Ported Or Added

- Add Unix LocalFS durable-publish fault-window tests for temporary creation,
  temporary write, temporary sync, final publish, and parent-directory sync.
- Each test asserts the classified `PublishFailureKind`, source backend error
  class, final object visibility, and generated temporary object cleanup.

### Retirement

- Deleted: none.
- Legacy-retained: old manifest-specific fault hooks remain until the current
  storage crate is retired.
- Follow-up: M3E service slices should reuse the same publish primitive rather
  than add service-specific filesystem test hooks.

## M3E1: Manifest Services

### Current Files Read

- `crates/storage/src/durability/format/manifest.rs`
- `crates/storage/src/durability/format/watermark.rs`
- `crates/storage/src/durability/checkpoint_runtime.rs`
- `crates/storage/src/durability/recovery_bootstrap.rs`
- `crates/storage/src/manifest.rs`
- `crates/storage/src/segmented/mod.rs`
- `crates/storage/src/segmented/tests/publish_failures.rs`
- `crates/storage/src/test_hooks.rs`
- `crates/storage-next/src/layout/mod.rs`
- `crates/storage-next/src/format/manifest.rs`
- `crates/storage-next/src/format/watermark.rs`
- `crates/storage-next/src/service/publish.rs`
- `crates/storage-next/src/backend/publish.rs`
- `crates/storage-next/tests/service_fault_windows.rs`

### Behavior Preserved

- The database MANIFEST remains physical storage metadata: database id, codec
  id, active WAL segment, snapshot recovery facts, and flushed-through commit
  id.
- Fresh durable database manifests start on active WAL segment `1`.
- Manifest create and replace operations consume the durable publisher, which
  preserves the current write-temp, sync-temp, publish, and parent-directory
  sync sequence.
- Missing database MANIFEST is distinct from corrupt database MANIFEST.
- Active WAL segment, snapshot facts, and flush watermark updates are full
  manifest replacements.
- Branch/table manifest publication consumes the same durable-publish mechanics
  as database MANIFEST publication.
- Parent-directory sync failure after publish remains a distinct
  visible-but-durability-unconfirmed state.

### Intentional V1 Changes

- The old `ManifestManager` shape is not ported. Storage-next uses small
  service types under `service::manifest`.
- Database manifest bytes use V1 format version `1`; pre-V1 development
  manifest versions are rejected by the normal decoder.
- The old `segments.manifest` payload format is not ported in M3E1. Table
  manifest publication is payload-opaque; branch/table meaning waits for later
  layers.
- Follower-mode manifest behavior is not ported.
- Cache lifecycle must not wire database or table manifest services in as
  durable state.

### Deferred

- WAL append/read service remains M3E2.
- Snapshot, checkpoint, and sidecar services remain M3E3.
- Quarantine service and recovery classifications remain M3E4.
- Table manifest payload format and table runtime remain M4/M5/M6.
- Branch visibility, inherited-layer semantics, fork-frontier logic, and
  commit timeline remain later milestones.

### Tests Ported Or Added

- Add database manifest service tests for missing, create/read, create
  precondition failure, active WAL update, snapshot/flush fact update, corrupt
  bytes, codec mismatch, and unsupported durable publish.
- Add payload-opaque table manifest tests for missing, publish/read, and
  publish failure propagation.
- Preserve lower publish fault-window tests from M3TC1 instead of adding new
  manifest-specific filesystem hooks.

### Retirement

- Deleted: none.
- Legacy-retained: old database manifest manager and segment manifest code
  still serve current storage consumers.
- Follow-up: M4/M5/M6 should decide when current `segments.manifest` semantics
  are replaced by table and branch runtime services.

## M3E2: WAL Service Mechanics

### Current Files Read

- `crates/storage/src/durability/wal/mod.rs`
- `crates/storage/src/durability/wal/config.rs`
- `crates/storage/src/durability/wal/mode.rs`
- `crates/storage/src/durability/wal/writer.rs`
- `crates/storage/src/durability/wal/reader.rs`
- `crates/storage/src/durability/format/wal_record.rs`
- `crates/storage/src/durability/format/segment_meta.rs`
- `crates/storage/src/durability/commit_adapter.rs`
- `crates/storage/src/durability/recovery.rs`
- `crates/storage/src/durability/recovery_bootstrap.rs`
- `crates/storage-next/src/backend/mod.rs`
- `crates/storage-next/src/backend/local_fs.rs`
- `crates/storage-next/src/format/wal.rs`
- `crates/storage-next/src/format/segment_metadata.rs`
- `crates/storage-next/src/layout/mod.rs`
- `docs/architecture/implementation-plans/m3e2-wal-service-implementation-brief.md`

### Behavior Preserved

- WAL records remain the durable commit point for durable local storage.
- WAL segment headers keep the `STRA` magic, segment id, database id, and CRC32
  validation already locked by M3C3.
- WAL append keeps the current separation between record bytes and the
  codec-aware outer envelope.
- Segment rotation happens before appending a record that would exceed the
  configured segment size.
- `standard` records dirty WAL state without forcing a per-append durability
  barrier; `always` forces durability before append success is reported.
- Strict WAL reads distinguish latest-segment partial tails from corruption.
- Segment metadata sidecars remain optional accelerators rather than
  authoritative recovery state.

### Intentional V1 Changes

- Cache mode has no WAL service and does not create WAL objects.
- WAL records carry `CommitVersion`, `BranchId`, `Timestamp`, and opaque commit
  payload bytes rather than public transaction ids.
- Stable V1 WAL segment, envelope, and inner-record versions start at `1`;
  pre-V1 development versions are rejected instead of migrated.
- WAL object names come from `ObjectLayout::wal_segment` and
  `ObjectLayout::wal_prefix`; old `wal-NNNNNN.seg` filenames are not target
  durable names.
- Storage-next adds an object-name based backend append/sync primitive for the
  local durable WAL path. The primitive does not expose paths, file handles, or
  append streams above `backend::local_fs`.
- WAL append is not implemented as full-object durable replacement per commit.

### Deferred

- Non-identity storage codecs for WAL payloads remain deferred until codec
  plumbing is finalized.
- Full L8 recovery orchestration, lossy recovery policy, and health
  classification remain later work.
- Commit runtime wiring, WAL-before-visible enforcement, and visible-version
  publication remain M6/M7 work.
- Object-store WAL chunking and manifest fencing remain post-V1 substrate work.
- WAL segment metadata sidecar publication may be added only if needed for
  service diagnostics or performance.

### Tests Ported Or Added

- Add backend tests for object-name based append and sync behavior on local
  filesystem.
- Add WAL service tests for segment create/open, append/read roundtrip,
  rotation, standard/always durability policy, cache backend rejection, segment
  mismatch, database mismatch, partial-tail detection, mid-segment corruption,
  and active segment delete protection.

### Retirement

- Deleted: none.
- Legacy-retained: old WAL writer, reader, recovery, and commit-adapter code
  still serve current storage consumers.
- Follow-up: M6/M7 should retire old commit-adapter WAL wiring after the new
  commit runtime consumes storage-next WAL service.

## M3E3A: Snapshot Publish And Load Basics

### Current Files Read

- `crates/storage/src/durability/disk_snapshot/writer.rs`
- `crates/storage/src/durability/disk_snapshot/reader.rs`
- `crates/storage/src/durability/disk_snapshot/checkpoint.rs`
- `crates/storage/src/durability/checkpoint_runtime.rs`
- `crates/storage/src/durability/format/snapshot.rs`
- `crates/storage-next/src/format/snapshot.rs`
- `crates/storage-next/src/service/manifest.rs`
- `crates/storage-next/src/service/publish.rs`
- `docs/architecture/implementation-plans/m3e3-snapshot-checkpoint-sidecar-implementation-brief.md`

### Behavior Preserved

- Snapshot publication uses the same durable publish primitive as MANIFEST and
  table manifest publication.
- Snapshot readers validate header facts, database identity, codec identity,
  container CRC, and section framing before returning bytes to upper layers.
- The snapshot service exposes a borrowed section visitor for large snapshot
  inspection without forcing materialized section payloads.
- Snapshot section payloads remain opaque to L4.
- Snapshot objects are immutable once created; duplicate create attempts fail
  without overwriting old bytes.

### Intentional V1 Changes

- Snapshot objects use `snapshots/<16-hex-id>` from `ObjectLayout`, not old
  `snap-NNNNNN.chk` filenames.
- Storage-next snapshots carry commit-version watermarks, not transaction ids.
- The service accepts explicit snapshot facts and raw sections; it does not
  serialize primitive checkpoint DTOs.
- Snapshot id `0` and snapshot watermark `0` are rejected before backend
  access.

### Deferred

- Snapshot listing, latest lookup, and pruning wait for M3E3B.
- Mechanical checkpoint sequencing over MANIFEST and snapshot publication waits
  for M3E3C.
- Optional WAL segment metadata sidecars wait for M3E3D.
- Row-native snapshot payload construction and install remain L6/L8 work.

### Tests Ported Or Added

- Add snapshot service tests for missing optional and required loads, durable
  backend rejection, local filesystem publish/load roundtrip, invalid snapshot
  facts, duplicate immutable create, corrupt bytes, header/object id mismatch,
  decoded zero-watermark rejection, codec mismatch, database mismatch, publish
  failure kind propagation, returned durable-byte facts, borrowed visitor
  success, CRC-before-callback validation, identity-before-callback validation,
  and callback error propagation.

### Retirement

- Deleted: none.
- Legacy-retained: old snapshot writer, reader, checkpoint runtime, and
  primitive checkpoint DTO code still serve current storage consumers.
- Follow-up: M3E3B-D should add list/prune/checkpoint/sidecar mechanics before
  L8 recovery consumes storage-next snapshots.

## M3E3B: Snapshot Listing, Latest Lookup, And Pruning

### Current Files Read

- `crates/storage/src/durability/disk_snapshot/writer.rs`
- `crates/storage/src/durability/disk_snapshot/reader.rs`
- `crates/storage/src/durability/disk_snapshot/checkpoint.rs`
- `crates/storage-next/src/layout/mod.rs`
- `crates/storage-next/src/backend/mod.rs`
- `crates/storage-next/src/backend/memory.rs`
- `crates/storage-next/src/service/snapshot.rs`
- `docs/architecture/implementation-plans/m3e3-snapshot-checkpoint-sidecar-implementation-brief.md`
- `docs/architecture/implementation-plans/m3e3-snapshot-checkpoint-sidecar-test-suite-plan.md`

### Behavior Preserved

- Snapshot retention remains caller-driven. The storage service executes
  explicit live-snapshot and retain-newest facts; it does not decide checkpoint
  policy.
- Snapshot pruning protects the live MANIFEST snapshot and newest retained
  snapshots before deleting any older snapshot objects.
- Delete failures are reported per object without hiding successful deletions
  or protected objects.

### Intentional V1 Changes

- Snapshot listing parses only exact lowercase `snapshots/<16-hex-id>` object
  names from `ObjectLayout::snapshot_prefix()`.
- Malformed names inside the snapshot family fail closed instead of being
  silently ignored.
- Objects outside the snapshot family are ignored even if a backend returns
  them during prefix listing.
- Latest snapshot means highest listed snapshot object id. It does not imply
  the MANIFEST-live snapshot.

### Deferred

- Mechanical checkpoint sequencing waits for M3E3C.
- Optional WAL segment metadata sidecars wait for M3E3D.
- Recovery health classification for malformed snapshot listings remains L8
  work.

### Tests Ported Or Added

- Add private snapshot listing/prune tests for empty listings, numeric ordering,
  latest selection, malformed snapshot-family names, weak-prefix family ignores,
  list failure routing, live/newest retention protection, retain count clamping,
  malformed snapshot-family rejection during prune before any delete, delete
  failure reporting, zero live-snapshot rejection, and delete-capability
  preflight.

### Retirement

- Deleted: none.
- Legacy-retained: old snapshot reader/writer and checkpoint runtime still
  serve current storage consumers.
- Follow-up: M3E3C-D should add checkpoint sequencing and optional sidecar
  mechanics before L8 recovery consumes storage-next snapshots.

## M3E3C: Checkpoint Sequencing

### Current Files Read

- `crates/storage/src/durability/disk_snapshot/checkpoint.rs`
- `crates/storage/src/durability/checkpoint_runtime.rs`
- `crates/storage-next/src/service/manifest.rs`
- `crates/storage-next/src/service/snapshot.rs`
- `crates/storage-next/src/service/publish.rs`
- `docs/architecture/implementation-plans/m3e3-snapshot-checkpoint-sidecar-implementation-brief.md`
- `docs/architecture/implementation-plans/m3e3-snapshot-checkpoint-sidecar-test-suite-plan.md`

### Behavior Preserved

- Checkpoint sequencing remains mechanical: active WAL facts are persisted
  before snapshot publication, and MANIFEST snapshot facts are persisted only
  after snapshot publication succeeds.
- Final MANIFEST no-visible failures after snapshot publication are classified
  as orphan snapshots, not corrupt databases.
- Final MANIFEST publish uncertainty after snapshot publication is classified
  separately because MANIFEST may already point to the snapshot.
- The checkpoint layer preserves enough published snapshot facts for later
  lifecycle and recovery code to classify or inspect the snapshot.

### Intentional V1 Changes

- The checkpoint service takes caller-supplied raw `SnapshotSection` values and
  explicit database, codec, active WAL, snapshot id, watermark, and timestamp
  facts. It does not build row-native sections.
- The service validates the existing database MANIFEST identity before
  snapshot publication and rejects invalid checkpoint facts before MANIFEST
  mutation.
- The active-WAL MANIFEST update reuses the already-loaded MANIFEST from
  identity validation instead of loading current state a second time.
- Typed checkpoint errors own the sequencing boundary: load/current MANIFEST
  failures, active-WAL MANIFEST failures, snapshot publish failures, database
  mismatch, invalid input facts, orphan-after-publish failures, and final
  MANIFEST uncertainty are distinct.

### Deferred

- Optional WAL segment metadata sidecars wait for M3E3D.
- WAL durability forcing, snapshot payload construction, snapshot install,
  checkpoint scheduling, snapshot pruning policy, and WAL deletion remain L6/L8
  lifecycle work.

### Tests Ported Or Added

- Add private checkpoint sequencing tests for successful publish order, missing
  and corrupt MANIFEST rejection, codec and database mismatch rejection,
  invalid input fact rejection before mutation, active-WAL MANIFEST publish
  failure, all snapshot publish `PublishFailureKind` values, orphan snapshot
  facts on final MANIFEST no-visible failures, final MANIFEST uncertainty for
  `VisibilityUnknown` and `VisibleDurabilityUnconfirmed`, direct orphan snapshot
  loadability, preservation of previous MANIFEST snapshot facts, and the
  single-load active-WAL update path.

### Retirement

- Deleted: none.
- Legacy-retained: old snapshot checkpoint runtime still serves current storage
  consumers.
- Follow-up: M3E3D should add optional sidecar mechanics before L8 recovery
  consumes storage-next sidecar facts.

## M3E3D: Optional WAL Segment Metadata Sidecars

### Current Files Read

- `crates/storage/src/durability/format/segment_meta.rs`
- `crates/storage/src/durability/wal/writer.rs`
- `crates/storage/src/durability/wal/reader.rs`
- `crates/storage/src/durability/compaction/wal_only.rs`
- `crates/storage-next/src/format/segment_metadata.rs`
- `crates/storage-next/src/layout/mod.rs`
- `crates/storage-next/src/service/publish.rs`
- `docs/architecture/implementation-plans/m3e3-snapshot-checkpoint-sidecar-implementation-brief.md`
- `docs/architecture/implementation-plans/m3e3-snapshot-checkpoint-sidecar-test-suite-plan.md`

### Behavior Preserved

- WAL segment metadata sidecars remain optional accelerators. Missing,
  corrupt, future-version, pre-V1, checksum-mismatched, trailing-byte, and
  segment-id-mismatched sidecars are reported as fallback facts, not
  authoritative recovery failures.
- Sidecar publication uses a durable replace operation and preserves
  `PublishFailureKind` on publish failure.
- Sidecar deletion failures are reported without hiding the authoritative WAL
  segment state.

### Intentional V1 Changes

- Current `.meta` filesystem paths such as `wal-000001.meta` are not ported.
  Storage-next sidecars live under `meta/wal/<16-hex-segment-id>` through
  `ObjectLayout`.
- The sidecar service is separate from the WAL service. M3E3D publishes,
  loads, and deletes optional sidecar objects, but WAL recovery still scans
  authoritative segment bytes when sidecars are absent or invalid.
- Segment id `0` is rejected at the service boundary before object-name
  construction.

### Deferred

- Writing sidecars automatically on WAL rotation, flush, or checkpoint remains
  lifecycle/recovery work.
- Using sidecars to skip WAL scans during recovery or retention remains L8
  work.
- Table or future sidecar families are not implemented in this slice.

### Tests Ported Or Added

- Add private sidecar service tests for exact object naming, zero segment-id
  rejection across load/publish/delete, publish/load roundtrip, durable replace
  mode, memory-backend durable rejection, local filesystem durable publication,
  missing fallback, corrupt-byte fallback, segment-id mismatch fallback, backend
  read failure routing, all publish failure kinds, WAL object preservation on
  publish failure, delete failure reporting, and missing-delete no-op facts.

### Retirement

- Deleted: none.
- Legacy-retained: old WAL writer/reader sidecar mechanics still serve current
  storage consumers.
- Follow-up: M3E4 should implement quarantine service mechanics and recovery
  integration.
