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

## M3E4A: Quarantine Inventory Source Map And Codec

### Current Files Read

- `crates/storage/src/quarantine.rs`
- `crates/storage/src/segmented/quarantine_protocol.rs`
- `crates/storage/src/segmented/compaction.rs`
- `docs/architecture/storage/l2-object-layout.md`
- `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
- `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
- `docs/spec/strata-storage-format-v1.md`

### Behavior Preserved

- Quarantine remains a branch-local durable inventory used during safe reclaim.
- Absence of a quarantine inventory represents an empty state only when no
  quarantine objects exist for that branch.
- Inventory bytes are integrity-checked and malformed inventories fail closed.
- Entries are relocation-safe. They record database-relative storage identity,
  not absolute filesystem paths.
- Quarantine inventory publication remains separate from final purge.

### Intentional V1 Changes

- Storage-next does not read or write the old `STRAQRTN` quarantine manifest
  bytes. Those bytes were local-filesystem and segment-filename specific.
- V1 inventory entries record a quarantine object id, source `ObjectName`,
  byte count, and timestamp. They do not record old segment ids or branch-local
  filenames.
- V1 inventory stores branch identity as raw `BranchId` bytes. Object paths use
  `BranchId` display text produced by core-next.
- Source family is derived from `source_object` during validation and reporting;
  it is not stored redundantly in durable bytes.
- The portable V1 service will use publish-inventory, publish-quarantine-object,
  delete-source ordering because the storage-next backend has no rename
  primitive.

### Deferred

- Quarantine service load/publish mechanics move to M3E4B.
- Quarantine object movement, purge, and retry semantics move to M3E4C.
- Recovery reconciliation and policy-downgrade classification move to M3E4D.
- L6 reachability proof and L8 reclaim orchestration remain out of M3E4A.

### Tests Ported Or Added

- Add V1 quarantine inventory encode/decode unit tests.
- Add golden vectors for empty and multi-entry quarantine inventories.
- Add malformed-input and validation tests for invalid magic, version, CRC,
  truncation, invalid object ids, unknown source families, duplicate object ids,
  duplicate source objects, noncanonical entry order, and oversized entry
  counts.
- Add a `format_quarantine` cargo-fuzz target through the hidden testkit
  decoder surface.

### Retirement

- Deleted: none.
- Legacy-retained: old `crates/storage/src/quarantine.rs` and segmented
  quarantine protocol still serve old storage consumers.
- Follow-up: M3E4B-D should make old quarantine inventory and protocol code
  eligible for retirement once old storage is no longer a workspace consumer.

## M3E4B: Quarantine Inventory Service

### Current Files Read

- `crates/storage/src/quarantine.rs`
- `crates/storage/src/segmented/quarantine_protocol.rs`
- `crates/storage/src/segmented/recovery.rs`
- `crates/storage-next/src/format/quarantine.rs`
- `crates/storage-next/src/service/publish.rs`
- `crates/storage-next/src/layout/mod.rs`

### Behavior Preserved

- Inventory load treats absence as an empty inventory only at the inventory
  service boundary; reconciliation still has to inspect branch quarantine
  objects before calling a branch clean.
- Corrupt inventory bytes fail closed as decode errors and are not converted to
  empty state.
- Database, branch, and codec identity mismatches are rejected before callers
  can trust inventory entries.
- Publishing an empty inventory is valid and represents an explicitly drained
  branch quarantine inventory.
- Durable inventory replacement preserves publish failure kind and source facts
  so recovery layers can distinguish no-visible failures from uncertain
  publication.

### Intentional V1 Changes

- The inventory service owns typed load and publish reports instead of exposing
  old storage's path-oriented manifest handling.
- Inventory object names are produced only through `ObjectLayout`; old
  `quarantine.manifest` paths are not read or written.
- Durable publication uses the shared storage-next publisher and backend
  capabilities rather than local-filesystem-specific quarantine manifest writes.
- Memory/cache backends can load absent inventory as empty but cannot pretend to
  publish durable quarantine inventory.

### Deferred

- Quarantine object movement and purge mechanics move to M3E4C.
- Recovery reconciliation and policy-downgrade classification move to M3E4D.
- Stateful quarantine service fuzzing and property coverage move to M3TC4.
- Cache-mode absence across open/close/maintenance paths remains M3TD1.

### Tests Ported Or Added

- Add inventory load tests for absent inventory, required missing inventory,
  corrupt bytes, backend read failure, and database/branch/codec mismatches.
- Add durable replace tests for creating a missing inventory object, replacing
  an existing inventory object, publishing an explicit empty inventory, memory
  backend durable rejection, and missing durable-sync capability preflight.
- Add publish-failure tests for all five `PublishFailureKind` values, including
  no-visible old-byte preservation and visible-but-unconfirmed byte facts.

### Retirement

- Deleted: none.
- Legacy-retained: old quarantine manifest loading and publication still serve
  the old storage crate.
- Follow-up: once M3E4C-D and M3TC4 close, old quarantine manifest service
  behavior can be retired with old storage consumers.

## M3E4C: Quarantine Object Movement And Purge

### Current Files Read

- `crates/storage/src/quarantine.rs`
- `crates/storage/src/segmented/quarantine_protocol.rs`
- `crates/storage/src/segmented/compaction.rs`
- `crates/storage/src/segmented/tests/gc_under_degradation.rs`
- `crates/storage/src/segmented/tests/lifecycle.rs`
- `crates/storage-next/src/service/quarantine.rs`
- `crates/storage-next/src/service/publish.rs`
- `crates/storage-next/src/backend/publish.rs`

### Behavior Preserved

- Quarantine remains a safety buffer: source bytes are not deleted until a
  quarantine copy has been durably published.
- Reclaim requires a fresh safe gate fact. Referenced, unsafe-recovery, and
  proof-incomplete facts stop before backend access.
- Inventory is published before object movement so a copy failure can be
  explained by later reconciliation as a missing quarantine object, not as lost
  in-flight work.
- Existing complete quarantine state is idempotent. If the inventory entry and
  quarantine object already exist, the service can retry source deletion after
  validating the source and quarantine bytes agree.
- Purge deletes only inventory-listed quarantine objects and keeps failed
  deletes in the rewritten inventory for retry.
- Missing objects during purge are reported as already drained and removed from
  the retained inventory.

### Intentional V1 Changes

- Storage-next uses portable publish-inventory, publish-quarantine-object, and
  delete-source ordering because the backend contract has no rename operation.
- Reachability and recovery-safety proof are caller-supplied gate facts. The
  storage service does not scan tables or choose compaction policy.
- Inventory mismatch is a typed service error before mutation rather than an
  implicit old-storage maintenance decision.
- Visibility-unknown and visible-but-unconfirmed quarantine object publish
  windows share one top-level uncertain status; callers can inspect the
  retained `PublishFailureKind` for the exact split.
- Purge rewrite failure preserves the delete report instead of hiding objects
  that were already deleted.

### Deferred

- Automatic repair of mismatched inventory/object state remains an L8 policy
  operation.
- L6 reachability proof remains outside storage-next.
- Recovery reconciliation over mismatch states moves to M3E4D.
- Stateful quarantine service fuzzing and operation-stream property tests move
  to M3TC4.

### Tests Ported Or Added

- Add quarantine request validation and safe-gate tests that prove malformed or
  unsafe requests fail before backend access.
- Add operation-order tests for source read, source metadata, inventory publish,
  quarantine object publish, and source delete.
- Add fault-window tests for source missing, source read failure, metadata
  failure, metadata size mismatch, corrupt inventory, inventory publish
  failures, quarantine publish failures or uncertainty, and source delete
  failure.
- Add existing-entry tests for idempotent retry, missing quarantine copy,
  source/quarantine byte drift, source mismatch, and byte-count drift.
- Add purge tests for unsafe gate preflight, empty inventory, delete failure
  retention, missing object removal, deterministic ordering, and inventory
  rewrite failure reporting.

### Retirement

- Deleted: none.
- Legacy-retained: old quarantine movement, compaction, and purge behavior
  still serve the old storage crate.
- Follow-up: M3TC4 should port remaining adversarial fault-window and
  state-machine coverage before old quarantine mutation tests are retired.

## M3E4D: Quarantine Recovery Classification And Reconciliation

### Current Files Read

- `crates/storage/src/quarantine.rs`
- `crates/storage/src/segmented/quarantine_protocol.rs`
- `crates/storage/src/segmented/recovery.rs`
- `crates/storage/src/durability/recovery.rs`
- `crates/storage/src/segmented/tests/quarantine_reconciliation.rs`
- `crates/storage-next/src/service/quarantine.rs`
- `crates/storage-next/src/service/quarantine/mutation.rs`
- `crates/storage-next/src/layout/mod.rs`

### Behavior Preserved

- Reconciliation treats inventory/object disagreement as degraded recovery
  state, not as a clean empty branch.
- Missing quarantine inventory is healthy only when the branch quarantine prefix
  has no quarantine objects.
- Corrupt inventory remains visible as a recovery fact instead of being
  converted to empty state.
- Backend read/list failures prevent classification and surface as unavailable
  recovery state.
- Unknown or malformed quarantine objects are retained and reported. They are
  not repaired or deleted by reconciliation.

### Intentional V1 Changes

- Recovery classifications are returned as typed service reports instead of
  string-oriented old storage diagnostics.
- Branch-local reconciliation consumes a concrete `BranchId`; family
  reconciliation parses the global quarantine family and reports invalid branch
  path text that branch-local reconciliation cannot discover.
- Inventory database, branch, and codec mismatches are classified as corrupt
  inventory facts for recovery reporting.
- Family reconciliation routes malformed object ids with valid branch text into
  the matching branch-local report. Family-level malformed facts are reserved
  for names that cannot be assigned to a branch report.
- Reconciliation is read-only. Repair and purge remain separate operations that
  require caller-supplied safe gate facts.

### Deferred

- Stateful quarantine service fuzzing and property coverage move to M3TC4.
- Cache-mode durable quarantine absence remains M3TD1.
- L6 reachability proof and L8 repair policy remain outside storage-next.
- Old storage quarantine code stays until old storage consumers are retired.

### Tests Ported Or Added

- Add branch reconciliation tests for clean empty, explicit empty inventory,
  matching inventory/object state, corrupt inventory, missing quarantine
  objects, unlisted quarantine objects, malformed object ids, backend list
  failure, and inventory read failure.
- Add family reconciliation tests for malformed branch ids, weak-prefix
  adjacent-family filtering, and valid-branch malformed object routing.
- Add identity-mismatch classification coverage for corrupt inventory facts.
- Add read-only assertions that reconciliation performs no publish, write,
  delete, or metadata mutation.

### Retirement

- Deleted: none.
- Legacy-retained: old recovery and quarantine diagnostic paths still serve the
  old storage crate.
- Follow-up: M3TC4 should port any remaining adversarial reconciliation cases
  into the property/fuzz suite before old quarantine recovery tests are retired.

## M3TC4: Quarantine Test Suite And Service Fuzz Hardening

### Current Files Read

- `crates/storage/src/quarantine.rs`
- `crates/storage/src/segmented/quarantine_protocol.rs`
- `crates/storage/src/segmented/tests/quarantine_reconciliation.rs`
- `crates/storage/src/segmented/tests/publish_failures.rs`
- `crates/storage/src/segmented/tests/gc_under_degradation.rs`
- `crates/storage/src/segmented/tests/lifecycle.rs`
- `crates/storage-next/src/format/quarantine.rs`
- `crates/storage-next/src/service/quarantine.rs`
- `crates/storage-next/src/service/quarantine/mutation.rs`
- `docs/architecture/implementation-plans/m3e4-quarantine-recovery-test-suite-plan.md`

### Behavior Preserved

- Malformed or corrupt quarantine inventories fail closed and are never treated
  as empty inventory state.
- Safe reclaim remains ordered so source bytes are deleted only after inventory
  evidence and quarantine-copy visibility are established.
- Publish-failure windows preserve enough facts for later recovery to
  distinguish no-visible publication from visibility or durability uncertainty.
- Purge is retryable: failed deletes stay listed, already-missing objects are
  drained, and inventory rewrite failure does not hide per-object delete facts.
- Reconciliation remains read-only and classifies inventory/object disagreement
  as degraded recovery state instead of repairing or deleting bytes.

### Intentional V1 Changes

- The old path-oriented tests are re-expressed against portable `ObjectName`
  layout and typed service reports.
- Stateful coverage uses a shared bounded bytecode runner for proptest and
  cargo-fuzz instead of old local-filesystem scenario setup.
- The service model checks exact inventory and reconciliation facts, including
  object id, quarantine object name, source object, byte count, and timestamp.
- The fuzz runner models both visible and not-visible arms of inventory
  `VisibilityUnknown`, while deterministic service tests still pin each
  publisher boundary independently.

### Deferred

- Cache-mode absence across durable quarantine maintenance remains M3TD1.
- Old storage quarantine concurrency and end-to-end segmented-store scenarios
  remain legacy-retained until the old storage crate is retired.
- L6 reachability proof and L8 repair/purge orchestration remain outside M3TC4.

### Tests Ported Or Added

- Add exhaustive quarantine codec tests for old-magic rejection, durable branch
  bytes, reserved inventory object ids, overlong assembled paths, duplicate
  entries, duplicate source objects, invalid source families, and fuzz-corpus
  golden seeds.
- Add inventory-service tests for missing, corrupt, identity-mismatched,
  capability-missing, durable-replace, explicit-empty, publish-failure, and
  visible-uncertain inventory states.
- Add quarantine mutation and purge tests for unsafe gate preflight, malformed
  requests, operation ordering, all publish/copy/delete fault windows,
  idempotent retry, byte-count drift, source/quarantine byte drift, partial
  purge failure, and inventory rewrite failure.
- Add reconciliation and service-fuzz coverage for missing, corrupt, unlisted,
  malformed, and read/list-unavailable recovery states, with property checks
  after every generated operation.

### Retirement

- Deleted: none.
- Legacy-retained: old quarantine manifest, segmented quarantine protocol,
  publish-failure, lifecycle, and recovery tests still serve current storage
  consumers.
- Follow-up: when old storage consumers are removed, retire the old quarantine
  tests whose behavior is now represented by the storage-next suite and the
  M3TC4 fuzz/property runner.

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
- `docs/architecture/storage/l2-object-layout.md`

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
- `docs/architecture/storage/l3-durable-format-codec.md`
- `docs/architecture/storage/storage-space-id-registry.md`
- `docs/architecture/engine/storage-space-id-registry.md`
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
- `docs/architecture/storage/l3-durable-format-codec.md`
- `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
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
- `docs/architecture/storage/l3-durable-format-codec.md`
- `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
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
- `docs/architecture/storage/l3-durable-format-codec.md`
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
- `docs/architecture/storage/target-crate-shape-and-test-harness.md`
- `docs/architecture/storage/l3-durable-format-codec.md`

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
- WAL envelope payloads now pass through an explicit storage-codec boundary.
  V1 accepts only the configured `identity` codec, so bytes remain unchanged but
  non-identity WAL configs fail before backend access.
- Segment rotation happens before appending a record that would exceed the
  configured segment size.
- `standard` records dirty WAL state without forcing a per-append durability
  barrier; `always` forces durability before append success is reported.
- Strict WAL reads distinguish latest-segment partial tails from corruption.
- Latest-segment partial tails can be repaired by durably replacing the active
  WAL object with the validated prefix. Stale truncation facts are rejected if
  object size changed before repair.
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

- Non-identity storage codecs for WAL payloads remain deferred until the codec
  registry grows beyond the required V1 identity codec.
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

## M3 L1 Hardening: Local Filesystem Writer Guard

### Current Files Read

- `crates/engine/src/database/open.rs`
- `crates/engine/src/database/lifecycle.rs`
- `crates/storage-next/src/backend/mod.rs`
- `crates/storage-next/src/backend/local_fs.rs`
- `crates/storage-next/src/backend/conformance.rs`
- `crates/storage-next/src/config/mode.rs`
- `crates/storage-next/src/layout/mod.rs`
- `docs/architecture/storage/l1-backend-io.md`
- `docs/architecture/storage/l2-object-layout.md`

### Behavior Preserved

- Durable local open still requires a single-writer guard before upper layers
  can make commit-ordering claims.
- The local writer guard uses an OS advisory exclusive lock held by an open file
  descriptor, matching the current engine-side local database lock pattern.
- Lock contention is surfaced as a transient unavailable backend error so L8/L9
  can later map it to the writer-lock diagnostic contract.

### Intentional V1 Changes

- The writer lock is now exposed as a backend capability plus an executable
  `Backend::acquire_writer_lock` operation. Advertising
  `single_writer_lock` without an operation is not sufficient for storage-next.
- The local filesystem lock lives at the reserved backend object
  `locks/writer`, so object-family scans and cache-mode absence tests can see
  the lock family consistently.
- `fs2` is a storage-next dependency only when the `localfs` feature is enabled
  on non-wasm targets. Default-feature wasm builds still fail at the
  storage-next localfs boundary rather than inside a transitive lock crate.

### Deferred

- Lifecycle code must acquire and hold the writer guard during durable local
  open once storage-next exposes the L8/L9 open path.
- Product error mapping for lock contention remains in the later error and API
  layers.
- Object-durable multi-writer fencing remains separate from local filesystem
  writer locking.

### Tests Ported Or Added

- Add local filesystem backend tests proving the reported
  `SingleWriterLock` capability is backed by real exclusion, the lock can be
  reacquired after guard drop, symlink lock files fail closed, and Unix localfs
  now satisfies durable-local mode validation.
- Update backend conformance so basic backends may either reject durable-local
  mode or, when they expose the full durable-local capability set, satisfy it.

### Retirement

- Deleted: none.
- Legacy-retained: current engine database open still owns product-level lock
  acquisition until the storage-next lifecycle path is wired.
- Follow-up: M3TD1 should prove cache mode does not create `locks/writer` or
  any other durable lock-family object.

## M3F: WAL Commit Payload Format

### Current Files Read

- `crates/storage/src/durability/payload.rs`
- `crates/storage/src/durability/commit_adapter.rs`
- `crates/storage/src/durability/format/wal_record.rs`
- `crates/storage/src/txn/context.rs`
- `crates/storage/src/key_encoding.rs`
- `crates/storage/src/stored_value.rs`
- `crates/storage-next/src/format/wal.rs`
- `crates/storage-next/src/format/storage_row.rs`
- `crates/storage-next/src/row/mod.rs`
- `docs/spec/strata-storage-format-v1.md`
- `docs/architecture/storage/l3-durable-format-codec.md`

### Behavior Preserved

- WAL records remain the durable commit replay unit for durable local storage.
- The outer WAL record still carries commit version, branch id, and commit
  timestamp facts.
- Replay payloads still preserve put/delete intent, row values, expiry facts,
  commit timestamps, and branch-local physical keys.
- WAL record length CRC and payload CRC remain the integrity boundary for
  payload bytes inside WAL segments.

### Intentional V1 Changes

- Valid V1 WAL payloads are storage-row-native byte batches, not legacy
  primitive-shaped payloads and not MessagePack transaction payloads.
- `EntityRef`, primitive tags, transaction ids, and product operation names are
  not part of the storage-next WAL payload contract.
- Arbitrary opaque payload bytes remain useful only as malformed/corruption
  fixtures after this slice; valid construction goes through `StorageRow`
  values.

### Deferred

- L7 commit runtime will build WAL commit payloads from validated commit
  batches later.
- Conflict validation, commit-version allocation, visible-version publication,
  and branch commit guards remain outside M3F.
- Immutable table encoding remains a separate M3 follow-up.

### Tests Ported Or Added

- Added focused WAL commit payload codec tests for row-native roundtrips,
  strict magic/version/count/length handling, nested storage-row decode
  failures, trailing-byte rejection, deterministic encoding, and
  row/outer-fact validation.
- Updated WAL record tests so valid construction goes through
  `WalCommitPayload`; old empty-payload bytes are retained only as a
  malformed historical fixture.
- Updated WAL service tests and helpers so append/read/reopen, read-after,
  rotation, partial-tail, and cache-mode probes use row-native payload records
  instead of arbitrary valid payload bytes.
- Updated golden tests and fuzz routing with checked-in row-native WAL commit
  payload vectors and a direct `format_wal_commit_payload` fuzz target.

### Retirement

- Deleted: none.
- Legacy-retained: old storage WAL payload code still serves current storage
  consumers until storage-next replaces the old commit path.
- Follow-up: L7 must build `WalCommitPayload` from validated commit batches;
  immutable table encoding and table object publication remain separate M3
  closure work.

## M3G1: Immutable Table Header, Footer, And Block Frame

### Current Files Read

- `crates/storage/src/segment_builder.rs`
- `crates/storage/src/segment.rs`
- `crates/storage/src/index.rs`
- `crates/storage/src/bloom.rs`
- `crates/storage/src/key_encoding.rs`
- `crates/storage/src/stored_value.rs`
- `crates/storage-next/src/format/key.rs`
- `crates/storage-next/src/format/storage_row.rs`
- `crates/storage-next/src/format/mod.rs`
- `crates/storage-next/src/table/mod.rs`
- `docs/spec/strata-storage-format-v1.md`
- `docs/architecture/storage/l3-durable-format-codec.md`

### Behavior Preserved

- Table objects remain self-identifying durable byte artifacts.
- Table headers and footers carry commit-range, row-count, and block-layout
  facts before L5 interprets table contents.
- Table blocks remain framed, length-delimited, and CRC-protected.
- Table block payloads continue to support both uncompressed and zstd encoded
  bytes.

### Intentional V1 Changes

- Stable storage-next table bytes use `STTB` and `STTF` magic with format
  version `1`; old `STRAKV`/version-7 bytes are historical evidence only.
- M3G1 reserves the filter block value but does not accept filter block frames as
  valid V1 table payloads.
- Footer CRC covers the header/body plus footer offset fields before offsets are
  trusted by later table validation.
- Block frames record both encoded and decoded payload lengths so zstd decode
  cannot size allocations from unchecked compressed bytes.

### Deferred

- Data block entry bytes, monolithic index payloads, properties payloads, and
  complete table artifact encode/decode are M3G2/M3G3.
- Point lookup, range cursors, filters, block cache, compaction, and table object
  publication remain outside M3G.
- Golden vectors, fuzz routing, and source-vocabulary guards are completed in
  later M3G slices after the full table artifact exists.

### Tests Ported Or Added

- Add focused table header tests for strict magic/version/size/flag/reserved and
  fact validation, including old `STRAKV` rejection.
- Add focused table footer tests for absent-filter enforcement, footer CRC
  validation, and index/properties range checks over a synthetic table prefix.
- Add focused table block-frame tests for uncompressed and zstd roundtrips,
  unknown/reserved block types, unknown compression, flags, length bounds,
  checksum failure, truncation, exact consumed byte count, and expected-kind
  rejection.

### Retirement

- Deleted: none.
- Legacy-retained: old storage table implementation still serves current storage
  consumers until storage-next L5 table runtime is implemented.
- Follow-up: M3G2 must build data/index/properties payloads on top of these
  frames instead of adding a second table frame shape.

## M3G2: Immutable Table Data, Index, And Properties Payloads

### Current Files Read

- `crates/storage/src/segment_builder.rs`
- `crates/storage/src/segment.rs`
- `crates/storage/src/index.rs`
- `crates/storage/src/key_encoding.rs`
- `crates/storage/src/stored_value.rs`
- `crates/storage-next/src/format/key.rs`
- `crates/storage-next/src/format/storage_row.rs`
- `crates/storage-next/src/format/table/mod.rs`
- `crates/storage-next/src/row/mod.rs`
- `docs/spec/strata-storage-format-v1.md`
- `docs/architecture/storage/l3-durable-format-codec.md`

### Behavior Preserved

- Table data stays sorted by durable internal-key bytes.
- Index entries describe absolute table offsets and full encoded data-block
  frame lengths rather than runtime-only handles.
- Properties blocks carry table-level row, block, key-range, and commit-range
  facts for later fast rejection and recovery diagnostics.

### Intentional V1 Changes

- Data entries carry V1 `StorageRow` bytes and V1 `InternalKey` bytes, not old
  prefix-compressed keys or bincode product values.
- The first V1 table index is monolithic and versioned independently from the
  outer block frame.
- Properties are storage-mechanical facts derived from row bytes and key bytes;
  product table semantics remain out of L3.

### Deferred

- Whole-table artifact encode/decode, cross-block validation against actual
  framed data blocks, table goldens, fuzz routing, and placeholder integration
  test replacement remain M3G3/M3G4 work.
- Point lookup, range cursors, filters, block cache, compaction, and table object
  publication remain outside M3G.

### Tests Ported Or Added

- Add data-block payload tests for put/tombstone rows, strict ordering,
  duplicate-key rejection, key/row fact mismatch rejection, nested row failures,
  trailing bytes, deterministic encoding, and allocation guards.
- Add index-block payload tests for version routing, sorted non-overlapping key
  ranges, zero/oversized counts and lengths, row-count and frame-length guards,
  and trailing bytes.
- Add properties-block payload tests for version routing, row/block/commit/key
  fact validation, invalid key bytes, trailing bytes, and constructor guards.

### Retirement

- Deleted: none.
- Legacy-retained: old storage table implementation still serves current storage
  consumers until storage-next L5 table runtime is implemented.
- Follow-up: M3G3 must compose these payloads with M3G1 block frames into whole
  table artifact helpers and cross-block validation.

## M3G3: Immutable Table Artifact Helpers And Cross-Block Validation

### Current Files Read

- `crates/storage/src/segment_builder.rs`
- `crates/storage/src/segment.rs`
- `crates/storage/src/index.rs`
- `crates/storage/src/key_encoding.rs`
- `crates/storage/src/stored_value.rs`
- `crates/storage-next/src/format/table/mod.rs`
- `crates/storage-next/src/format/table/data.rs`
- `crates/storage-next/src/format/table/index.rs`
- `crates/storage-next/src/format/table/properties.rs`
- `docs/architecture/implementation-plans/m3g-immutable-table-format-implementation-brief.md`
- `docs/architecture/implementation-plans/m3g-immutable-table-format-test-plan.md`

### Behavior Preserved

- Complete table bytes remain a single durable artifact with header, framed data
  blocks, a framed monolithic index, framed table properties, and a CRC-protected
  footer.
- Index entries continue to point at absolute table offsets and encoded frame
  lengths for data blocks.
- Decoded rows are returned in durable table order after the index, properties,
  header, and actual data-block frames agree.

### Intentional V1 Changes

- Whole-table decode is strict about cross-block facts: header counts, commit
  range, properties facts, index key ranges, index offsets, frame lengths, and
  decoded data-block rows must all match.
- Hidden bytes between the data region, index frame, properties frame, and footer
  are rejected instead of treated as padding.
- The helper remains storage-row-native and does not provide point lookup,
  cursor, cache, compaction, or product-table behavior.

### Deferred

- Golden vectors, table artifact fuzz routing, source-vocabulary guards, and
  placeholder integration-test replacement remain M3G4 work.
- Point lookup, range cursors, filters, block cache, compaction, and table object
  publication remain outside M3G.

### Tests Ported Or Added

- Add whole-table artifact tests for one-block, two-block, zstd, and mixed
  compression round trips.
- Add construction tests for empty, unsorted, and duplicate-row rejection.
- Add corruption tests for header fact drift, footer CRC drift, wrong index or
  properties frame types, properties row/block/commit/key fact drift, missing or
  extra data-block facts, index offset/length/row/key drift, non-data-frame
  index references, hidden bytes, short tables, impossible footer offsets,
  mismatched header/footer facts, and opaque storage-space preservation.

### Retirement

- Deleted: none.
- Legacy-retained: old storage table implementation still serves current storage
  consumers until storage-next L5 table runtime is implemented.
- Follow-up: M3G4 must pin the table byte shape with goldens, fuzz/property
  routing, source guards, and integration harness replacement.

## M3G4: Immutable Table Goldens, Fuzz Routing, And Harness Closeout

### Current Files Read

- `crates/storage/src/segment_builder.rs`
- `crates/storage/src/segment.rs`
- `crates/storage/src/index.rs`
- `crates/storage/src/key_encoding.rs`
- `crates/storage/src/stored_value.rs`
- `crates/storage-next/src/format/table/mod.rs`
- `crates/storage-next/src/format/table/artifact.rs`
- `crates/storage-next/src/format/table/data.rs`
- `crates/storage-next/src/format/table/index.rs`
- `crates/storage-next/src/format/table/properties.rs`
- `crates/storage-next/tests/table_properties.rs`
- `crates/storage-next/tests/format_golden.rs`
- `docs/spec/strata-storage-format-v1.md`
- `docs/architecture/implementation-plans/m3g-immutable-table-format-implementation-brief.md`
- `docs/architecture/implementation-plans/m3g-immutable-table-format-test-plan.md`

### Behavior Preserved

- Table bytes are pinned by checked-in vectors instead of being validated only by
  unit-test construction paths.
- Whole-table artifacts remain contiguous durable bytes with header, data block
  frames, index frame, properties frame, and footer.
- Format fuzzing continues to route arbitrary bytes through narrow hidden
  testkit decoder surfaces rather than exporting durable format internals as
  public production API.
- Table test coverage remains storage-row-native; it does not depend on product
  table, cache, compaction, or engine-layer payload concepts.

### Intentional V1 Changes

- The old placeholder table integration test is replaced with generated
  artifact checks over uncompressed and zstd table paths.
- Table goldens now cover framed data blocks, zstd compression, monolithic index
  payloads, properties payloads, and one-block/two-block whole artifacts.
- Table fuzz targets are split between block-frame decoding and whole-artifact
  decoding so failures shrink against the narrowest stable byte boundary.
- A source-vocabulary guard now prevents reintroducing product payload or engine
  crate coupling into the table format surface, while still allowing opaque
  storage-row atoms such as `StorageSpaceId::engine`.

### Deferred

- Point lookup, range cursors, filters, block cache, compaction, and table object
  publication remain outside M3G.
- Old storage table runtime remains the active production runtime until
  storage-next L5 consumes the immutable table byte format.
- Backend conformance and crash/stress integration harness replacement remain
  separate M3 test-track work.

### Tests Ported Or Added

- Add seven table golden vectors: one-put data frame, put-plus-tombstone data
  frame, zstd data frame, index payload, properties payload, one-block whole
  artifact, and two-block whole artifact.
- Add golden inventory and corpus-drift checks so table fuzz seeds stay pinned
  to the checked-in vectors.
- Add `format_table_block` and `format_table_artifact` cargo-fuzz targets with
  golden-seeded corpora.
- Add a generated table artifact property harness over duplicate physical keys,
  distinct commit versions, tombstones, uncompressed frames, zstd frames, and
  one-block/multi-block construction paths.
- Add a source guard for table format surfaces.

### Retirement

- Deleted: none.
- Legacy-retained: old storage table implementation still serves current storage
  consumers until storage-next L5 table runtime is implemented.
- M3G is closed. Follow-up table work moves to L4 table object publication and
  L5 table runtime integration rather than additional durable table byte
  format slices.

## M3H1: L4 Immutable Table Object Publication

### Current Files Read

- `crates/storage/src/segment_builder.rs`
- `crates/storage/src/segment.rs`
- `crates/storage/src/manifest.rs`
- `crates/storage-next/src/layout/mod.rs`
- `crates/storage-next/src/format/table/artifact.rs`
- `crates/storage-next/src/service/publish.rs`
- `crates/storage-next/src/service/manifest.rs`
- `docs/architecture/storage/l4-log-manifest-snapshot-services.md`

### Behavior Preserved

- Old table file construction made table bytes durable before table reachability
  metadata could name them. Storage-next preserves that ordering by adding an L4
  service for durable table-object creation separate from table manifest
  publication.
- Table objects remain immutable create-only objects. A duplicate publish
  returns the backend precondition failure and leaves existing bytes untouched.
- L4 owns backend publication and object naming. L5 remains responsible for
  producing the table bytes and L6 remains responsible for deciding table
  reachability.

### Intentional V1 Changes

- The service validates supplied bytes with the stable V1 immutable-table
  decoder before publishing, and it checks durable publish/sync capability
  before table decode so unsupported backends fail before service work starts.
  L4 will not make malformed table bytes durable through this path.
- Returned facts are storage-mechanical: object name, byte count, row count,
  data-block count, and commit range. They do not include point-lookup,
  compaction, cache, or product payload semantics.
- Cache-mode absence coverage now includes direct durable table object
  publication, not just table manifest publication.

### Deferred

- L5 table builders, point lookup, range cursors, filters, block cache, and
  compaction remain separate table-runtime work.
- L6 branch/table manifest contents and reachability rules remain separate
  branch-LSM work.
- Object-store fencing for multi-writer table object namespaces remains future
  object-durable work.

### Tests Ported Or Added

- Add service tests for successful create-only table object publication,
  invalid layout, invalid immutable-table bytes before publish, duplicate
  immutable object preservation, all five publish-failure kinds, and malformed
  publish outcome object, size, and durability metadata.
- Add capability-ordering tests proving missing durable publish or sync support
  wins before table byte decode, plus a local filesystem service round trip.
- Extend cache-mode absence tests so cache backends reject durable table object
  publication before mutation.

### Retirement

- Deleted: none.
- Legacy-retained: old storage table file creation remains the active production
  runtime until storage-next L5 table runtime calls this service.

## M3H2: L1-L4 Audit Hardening

### Current Files Read

- `crates/storage-next/src/backend/local_fs.rs`
- `crates/storage-next/src/format/quarantine.rs`
- `crates/storage-next/src/service/quarantine.rs`
- `crates/storage-next/src/service/wal.rs`
- `crates/storage-next/src/service/sidecar.rs`
- `crates/storage-next/src/service/table.rs`
- `docs/architecture/storage/l1-backend-io.md`
- `docs/spec/strata-storage-format-v1.md`

### Behavior Preserved

- Quarantine inventory bytes stay stable. Duplicate object ids, duplicate
  source objects, invalid object-name bytes, ordering drift, and checksum
  failures remain L3 codec errors.
- Quarantine source-family and full layout checks still fail closed before
  inventory bytes are trusted by services; the checks now live in L4 where
  object families and full object layout are owned.
- WAL segment deletion still protects the active segment and deletes only old
  segments whose records are covered by the supplied durable watermark.
- Quarantine mutation still publishes inventory before the quarantine copy and
  deletes source bytes only after the quarantine copy is durably created.

### Intentional V1 Changes

- WAL retention now requires a `WalRetentionProof` instead of a bare
  `CommitVersion`, so future lifecycle callers must state whether the coverage
  came from a snapshot watermark or flushed-table watermark.
- WAL retention treats delete-not-found as an idempotent already-pruned result
  and best-effort deletes the optional WAL segment metadata sidecar after the
  authoritative segment is removed.
- Quarantine mutation no longer reads the full quarantine object on the
  new-entry path just to detect an unlisted existing copy. It uses metadata for
  existence and only reads quarantine bytes for existing inventory entries where
  byte equality must be checked.
- Quarantine reserved inventory object id validation uses the literal
  `manifest` reservation instead of recomputing it through layout on each
  request.
- The LocalFS writer-lock name is documented as the single L1/L2 bootstrap
  exception needed to enforce durable-local single-writer open.
- Sidecar publication now validates that backend publish outcomes claim durable
  publication, matching the stronger checks used by authoritative services.
- Table object publication keeps its explicit capability preflight before table
  decode and documents that as intentional defense in depth over
  `ObjectPublisher`.

### Deferred

- LocalFS retry-exhausted temporary object cleanup remains a future
  operator-visible maintenance concern. The backend does not delete matching
  temp paths on retry exhaustion because they can belong to an in-flight publish
  in the same process.
- Conditional-publish fences and typed core-next branch/table IDs remain future
  object-store and L6 integration work.
- Quarantine source copy still has linear memory cost because V1 backend
  publishing is byte-slice based rather than streaming. Streaming publish is
  post-V1 substrate work.
- Manifest-vs-writer active-WAL mismatch coverage belongs to the L8 lifecycle
  integration that will combine manifest facts with a live writer.

### Tests Ported Or Added

- Add WAL retention/reopen tests for delete-not-found idempotency, header-only
  old segment deletion, optional sidecar cleanup on segment deletion,
  consecutive retention idempotency, and retention followed by reopen.
- Extend quarantine purge inventory-rewrite failure coverage across all publish
  failure windows, including visibility-unknown and visible-but-unconfirmed
  replacement visibility.
- Add quarantine mutation assertions proving the new-entry path checks
  quarantine-object existence through metadata and does not read unlisted
  quarantine object bytes before failing closed.

### Retirement

- Deleted: none.
- Legacy-retained: old storage WAL retention and quarantine mutation paths still
  serve current storage consumers until storage-next L7/L8 replace those
  runtimes.

## M3TF1: Backend Conformance Closeout

### Current Files Read

- `crates/storage-next/src/backend/conformance.rs`
- `crates/storage-next/tests/backend_conformance.rs`
- `crates/storage-next/src/testkit/integration_harness.rs`
- `docs/architecture/v1-progress-tracker.md`
- `docs/architecture/implementation-plans/m3-m3t-implementation-plan.md`

### Behavior Verified

- The private lower-layer backend conformance suite still passes for the memory
  backend in both default and no-default builds.
- The private lower-layer backend conformance suite still passes for the local
  filesystem backend in the default localfs build.
- The external testkit backend selector accepts the memory backend in a
  no-default `testkit` build and continues to reject `localfs` with the
  targeted feature-requirement error when the feature is absent.
- The external testkit backend selector accepts the local filesystem backend in
  a `testkit,localfs` build.

### Intentional V1 Changes

- Widened the `ObjectLayout` import cfg in
  `crates/storage-next/src/testkit/integration_harness.rs` so the hidden
  localfs crash-recovery/table-object helper compiles when `testkit` and
  `localfs` are enabled without `fault-injection`.

### Verification

- `cargo test -p strata-storage-next --locked backend::conformance`
- `cargo test -p strata-storage-next --no-default-features --locked backend::conformance`
- `STRATA_STORAGE_TEST_BACKEND=memory cargo test -p strata-storage-next --no-default-features --features testkit --test backend_conformance --locked`
- `STRATA_STORAGE_TEST_BACKEND=localfs cargo test -p strata-storage-next --features testkit --test backend_conformance --locked`

### Retirement

- Deleted: none.
- Legacy-retained: old storage lower-layer backend code remains active until
  storage-next L8/L9 owns the public storage runtime.
