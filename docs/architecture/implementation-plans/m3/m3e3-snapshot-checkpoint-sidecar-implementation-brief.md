# M3E3 Implementation Brief: Snapshot, Checkpoint, And Sidecar Services

Status: implementation brief

Parent plan: `docs/architecture/implementation-plans/m3-m3t-implementation-plan.md`

Test-suite plan:
`docs/architecture/implementation-plans/m3e3-snapshot-checkpoint-sidecar-test-suite-plan.md`

## Goal

Implement the L4 snapshot, mechanical checkpoint, and optional sidecar services
over the storage-next backend, layout, format, manifest, WAL, and durable
publisher layers.

M3E3 should produce three storage-owned service surfaces:

1. Snapshot service for immutable `snapshots/<snapshot-id>` objects.
2. Mechanical checkpoint service that orders MANIFEST active-WAL update,
   durable snapshot publication, and MANIFEST snapshot-watermark update.
3. Optional WAL segment metadata sidecar service.

These services must remain physical storage services. They must not serialize
primitive DTOs, install decoded rows into branch state, schedule checkpoints,
decide recovery health, or expose public product APIs.

## Inputs Read

Architecture and plan inputs:

1. `docs/architecture/storage/l1-backend-io.md`
2. `docs/architecture/storage/l2-object-layout.md`
3. `docs/architecture/storage/l3-durable-format-codec.md`
4. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
5. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
6. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
7. `docs/spec/strata-storage-format-v1.md`
8. `docs/architecture/implementation-plans/m3-m3t-implementation-plan.md`
9. `docs/architecture/implementation-plans/m3-porting-log.md`

Current implementation evidence:

1. `crates/storage/src/durability/disk_snapshot/writer.rs`
2. `crates/storage/src/durability/disk_snapshot/reader.rs`
3. `crates/storage/src/durability/disk_snapshot/checkpoint.rs`
4. `crates/storage/src/durability/checkpoint_runtime.rs`
5. `crates/storage/src/durability/format/snapshot.rs`
6. `crates/storage/src/durability/format/segment_meta.rs`
7. `crates/storage/src/durability/decoded_snapshot_install.rs`

Storage-next implementation inputs:

1. `crates/storage-next/src/backend/mod.rs`
2. `crates/storage-next/src/backend/local_fs.rs`
3. `crates/storage-next/src/backend/memory.rs`
4. `crates/storage-next/src/layout/mod.rs`
5. `crates/storage-next/src/format/snapshot.rs`
6. `crates/storage-next/src/format/segment_metadata.rs`
7. `crates/storage-next/src/service/manifest.rs`
8. `crates/storage-next/src/service/publish.rs`
9. `crates/storage-next/src/service/wal.rs`

## Existing Behavior To Preserve

1. Snapshot publication follows the same crash-safe shape as current storage:
   write complete bytes, fsync bytes, publish into the final namespace, then
   fsync parent metadata through the backend-owned durable publish primitive.
2. Snapshot readers validate header facts, codec identity, database identity,
   CRC, section framing, and payload limits before returning sections.
3. Snapshot sections are loaded as raw section bytes. Payload meaning belongs
   above L4.
4. Checkpoint mechanics persist active WAL segment facts before publishing a
   snapshot, then persist snapshot id plus snapshot watermark after the
   snapshot object is durably published.
5. A crash or failure after snapshot publication but before MANIFEST snapshot
   watermark update leaves an orphan snapshot. Recovery must ignore it unless
   a later MANIFEST points to it.
6. Snapshot pruning protects the live MANIFEST snapshot and the newest retained
   snapshots.
7. WAL segment metadata sidecars are optional accelerators. Missing or corrupt
   sidecars must not invalidate authoritative WAL segment bytes.
8. Deletion failures during pruning are reported per object and must not hide
   successful deletions or protected objects.

## Intentional V1 Changes

1. Snapshot object names come from `ObjectLayout::snapshot(snapshot_id)` and
   use `snapshots/<16-hex-id>`. Old `snap-NNNNNN.chk` paths are current-code
   evidence only.
2. Snapshot format version `2` from current storage is treated as pre-V1
   development evidence. V1 snapshot services consume the M3C4 format version
   `1` codecs.
3. Storage-next snapshot services validate only storage-mechanical section
   envelopes. Old primitive snapshot tags are not ported into L4.
4. The checkpoint service accepts caller-supplied raw `SnapshotSection` values
   or a `SnapshotContainer`. It does not build sections from KV, JSON, event,
   vector, graph, or branch DTOs.
5. The checkpoint service accepts caller-supplied snapshot id, watermark,
   timestamp, database id, codec id, and active WAL segment. It does not
   allocate commit versions, pick a checkpoint schedule, or choose an active
   WAL segment.
6. The checkpoint service requires an existing database MANIFEST. New-database
   MANIFEST creation remains lifecycle/open work through `create_initial`.
7. Cache mode has no durable snapshot, checkpoint, or sidecar objects. Memory
   backend tests may exercise unsupported durable behavior, but cache lifecycle
   must not wire these services in as persistent state.
8. Snapshot service temporary files are backend-owned publication details.
   M3E3 must not introduce a separate `tmp/snapshots/...` object family unless
   a later design proves it is needed.
9. WAL segment sidecar objects use a metadata namespace, not the WAL segment
   namespace. The V1 target path is `meta/wal/<16-hex-segment-id>` so WAL
   segment listing never has to interpret sidecar object names.
10. Optional sidecar corruption surfaces as a recoverable sidecar fact:
    missing or corrupt sidecar means "scan authoritative object instead," not
    "database recovery failed."

## Target Files

Implementation files:

1. `crates/storage-next/src/service/snapshot.rs`
2. `crates/storage-next/src/service/checkpoint.rs`
3. `crates/storage-next/src/service/sidecar.rs` for optional WAL segment
   metadata sidecars.
4. `crates/storage-next/src/service/mod.rs`
5. `crates/storage-next/src/layout/mod.rs`, for WAL sidecar object-name
   constructors.
6. `crates/storage-next/src/format/snapshot.rs`, only if service-consumed
   accessors or bounded visitor support are missing.
7. `crates/storage-next/src/format/segment_metadata.rs`, only if service
   validation needs small accessors.
8. `crates/storage-next/src/backend/mod.rs`, only if delete/list/publish facts
   need richer typed information.

Test files:

1. Module-local tests in `crates/storage-next/src/service/snapshot.rs`.
2. Private child modules under `crates/storage-next/src/service/snapshot/`,
   `crates/storage-next/src/service/checkpoint/`, or
   `crates/storage-next/src/service/sidecar/` if the test suite crosses the
   file-size review threshold.
3. `crates/storage-next/tests/service_fault_windows.rs`, only for behavior
   that cannot be expressed module-locally.
4. `crates/storage-next/tests/cache_mode_absence.rs`, only if M3TD begins in
   the same change.

Documentation files:

1. `docs/architecture/implementation-plans/m3-porting-log.md` must receive an
   M3E3 source-map note before production code changes.
2. `docs/architecture/storage/l2-object-layout.md` should be updated when
   `meta/wal/<segment-id>` sidecar names are implemented.
3. `docs/spec/strata-storage-format-v1.md` should be updated only if M3E3
   changes sidecar object naming or service-level sidecar policy.
4. `docs/architecture/v1-progress-tracker.md` should be updated only after
   M3E3 implementation and verification are complete.

## Service Shape

Exact Rust names can adjust during implementation, but the shape should stay
small, mechanical, and repeatable.

### Snapshot Service

1. Constructed from `&dyn Backend`.
2. Uses `ObjectLayout::snapshot(snapshot_id)` and
   `ObjectLayout::snapshot_prefix()` for all object names and listings.
3. Rejects snapshot id `0` before encoding or publishing.
4. Rejects checkpoint snapshot watermarks at commit version `0` before
   publishing, because MANIFEST snapshot facts cannot represent a present zero
   watermark.
5. `publish_create` encodes a `SnapshotContainer` and publishes it with durable
   create. Snapshot objects are immutable after publication.
6. `load_current` is not a snapshot-service operation. Callers load by explicit
   snapshot id or ask for `latest_snapshot`.
7. `load_optional(snapshot_id)` returns `Ok(None)` when the snapshot object is
   absent.
8. `load_required(snapshot_id)` returns a typed missing-snapshot error when the
   object is absent.
9. `load_for_codec(snapshot_id, database_id, codec_id)` validates database and
   codec identity before returning a materialized container.
10. `visit_sections(snapshot_id, database_id, codec_id, max_sections, visit)`
    should use the borrowed section visitor so large snapshots are not forced
    through eager allocation.
11. `list_snapshots` parses only exact `snapshots/<16-hex-id>` object names.
    Objects outside the snapshot family are ignored. Malformed names inside the
    snapshot family fail closed with a typed list/parse error.
12. `latest_snapshot` returns the highest listed snapshot id and object facts,
    not the MANIFEST-live snapshot.
13. Service result structs should include snapshot id, object name, byte count,
    section count, watermark, timestamp, and publish durability facts where
    available.

### Checkpoint Service

1. Constructed from `SnapshotService` and `DatabaseManifestService`, or from
   the same backend and internally constructs those L4 services.
2. Takes explicit input facts: database id, codec id, active WAL segment,
   snapshot id, snapshot watermark, created-at timestamp, and raw sections.
3. Requires an existing database MANIFEST and validates codec id against it.
4. Persists the active WAL segment into MANIFEST before publishing the
   snapshot object.
5. Publishes the snapshot object with durable create.
6. Persists snapshot id plus snapshot watermark into MANIFEST only after the
   snapshot publish succeeds.
7. If final MANIFEST update fails after snapshot publication, returns a typed
   error containing the published snapshot facts so L8 can classify the orphan.
8. Does not force WAL durability, build row-native sections, install rows,
   prune snapshots, or delete WAL segments. L8 and L6 own those decisions.

### Snapshot Pruning

1. The snapshot service may expose a mechanical prune/delete operation, but it
   must take retention intent from the caller.
2. Required retention input facts are `live_snapshot_id: Option<u64>` and
   `retain_newest: usize`.
3. `retain_newest` is clamped to at least one.
4. Protected snapshots are the live MANIFEST snapshot and the newest retained
   snapshots.
5. The service deletes only unprotected snapshot objects older than the newest
   retained set.
6. The delete report should include deleted, protected, and failed objects in
   ascending snapshot-id order.
7. Delete failure is non-fatal for the prune report, but backend list failure
   fails before any delete is attempted.

### Sidecar Service

1. M3E3 sidecars are optional WAL segment metadata sidecars only.
2. The object-name constructor should be
   `ObjectLayout::wal_segment_metadata(segment_id)` and target
   `meta/wal/<16-hex-segment-id>`.
3. Segment id `0` is rejected before object-name construction because WAL
   segment id `0` is not a valid authoritative segment.
4. `publish_segment_metadata` uses durable replace because the sidecar is
   derived from authoritative WAL bytes and can be regenerated.
5. `load_segment_metadata(segment_id)` returns one of:
   present and valid, missing, corrupt, or backend failure.
6. Missing and corrupt are recoverable sidecar facts. Backend failures remain
   typed service errors.
7. Decoded sidecar segment id must match the requested segment id.
8. Sidecar publish failure must not mutate or delete WAL segment objects.
9. Sidecar services must not be required by WAL recovery in M3E3. WAL scans
   remain authoritative.

## Error Shape

Snapshot service errors should preserve:

1. Object role: snapshot object, checkpoint manifest update, sidecar object, or
   prune delete.
2. Object name.
3. Snapshot id or segment id when relevant.
4. Backend source error for reads, lists, metadata, deletes, and writes.
5. Format source error for snapshot or sidecar decode/encode.
6. Publish source error and `PublishFailureKind`.
7. Codec mismatch facts: expected and actual codec id.
8. Database mismatch facts: expected and actual database id.
9. Whether a checkpoint failure happened before snapshot visibility, after
   snapshot visibility, or after snapshot durability was unconfirmed.

Do not collapse publish uncertainty into generic IO. The later error mapping
needs to distinguish `FailedBeforeVisibility`, `VisibilityUnknown`, and
`VisibleDurabilityUnconfirmed`.

## Fault And Recovery Rules

1. Missing snapshot object is absence on optional load and typed missing on
   required load.
2. Corrupt snapshot bytes fail closed. The service must not repair, recreate,
   or skip corrupt MANIFEST-live snapshots.
3. Snapshot publish with durable create must not overwrite an existing
   snapshot.
4. If snapshot publish fails before visibility, the checkpoint service must not
   persist MANIFEST snapshot facts.
5. If snapshot publish visibility is unknown, the checkpoint service must not
   persist MANIFEST snapshot facts.
6. If snapshot publish is visible but durability is unconfirmed, the checkpoint
   service must not persist MANIFEST snapshot facts.
7. If MANIFEST snapshot-fact update fails before visibility after snapshot
   publication, the snapshot is an orphan. The error must preserve the snapshot
   id and publish facts.
8. If MANIFEST snapshot-fact update visibility is unknown or visible but
   durability is unconfirmed, the checkpoint result must preserve the snapshot
   facts and publish kind without classifying the snapshot as an orphan.
9. Pruning must never delete the live MANIFEST snapshot even when it falls
   outside the newest retained set.
10. Optional sidecar corruption must not poison the authoritative WAL segment.
11. All non-obvious fault windows need inline comments in production code when
    implemented.

## Test Plan

This section defines the minimum M3E3 implementation tests. The comprehensive
follow-up suite is defined in
`docs/architecture/implementation-plans/m3e3-snapshot-checkpoint-sidecar-test-suite-plan.md`
and should land as `M3TC3` before later recovery layers rely on snapshot and
checkpoint behavior as stable infrastructure.

Minimum M3E3 tests:

1. Snapshot service rejects memory/cache backend durable publication.
2. Snapshot create/read roundtrips V1 bytes through local filesystem backend.
3. Snapshot id `0` is rejected before publish.
4. Snapshot watermark `0` is rejected by checkpoint publication before publish.
5. Durable create refuses an existing snapshot and preserves old bytes.
6. Corrupt snapshot bytes return typed decode errors.
7. Codec mismatch returns typed validation error.
8. Database id mismatch returns typed validation error.
9. Snapshot listing sorts by numeric snapshot id and rejects malformed names
   inside the snapshot family.
10. Latest snapshot returns highest listed snapshot id, not MANIFEST state.
11. Checkpoint success persists active WAL segment, publishes snapshot, and
    persists snapshot facts in that order.
12. Snapshot publish failure does not persist snapshot facts.
13. Final MANIFEST update failure after snapshot publish returns orphan
    snapshot facts.
14. Prune protects live snapshot and newest retained snapshots.
15. Prune reports delete failures without hiding successful deletions.
16. Sidecar publish/read roundtrips segment metadata.
17. Missing and corrupt sidecars return recoverable sidecar facts.
18. Sidecar segment-id mismatch is recoverable sidecar corruption.

Fault-window coverage:

1. Snapshot service tests should propagate every `PublishFailureKind`.
2. Publisher-level temp-write, temp-sync, final-publish, and parent-sync tests
   already exist in M3TC1. M3E3 should add snapshot-specific assertions only
   where checkpoint sequencing changes the outcome.
3. Process-kill crash tests remain later lifecycle/recovery work.

## Implementation Order

M3E3 should land as four implementation slices, not one large change.

1. `M3E3A`: Snapshot layout and publish/load basics.
   - Append the M3E3 source-map note to `m3-porting-log.md`.
   - Add snapshot object-name parsing if the existing layout surface is not
     enough for exact snapshot-family validation.
   - Add `service::snapshot` with publish-create, optional load, required
     load, codec validation, database validation, and typed errors.
   - Wire the snapshot service through `service/mod.rs`.
   - Cover minimum tests 1-8 and publish-failure propagation for snapshot
     create.
2. `M3E3B`: Snapshot listing, latest, and pruning.
   - Add list/latest mechanics over `ObjectLayout::snapshot_prefix()`.
   - Reject malformed objects inside the snapshot family.
   - Add mechanical prune/delete with caller-supplied live snapshot id and
     retained-newest count.
   - Cover minimum tests 9-10 and 14-15.
3. `M3E3C`: Checkpoint sequencing.
   - Add the mechanical checkpoint service over database manifest and snapshot
     services.
   - Persist active WAL segment before snapshot publish.
   - Persist snapshot id plus snapshot watermark only after snapshot publish
     succeeds.
   - Preserve orphan-snapshot facts when the final MANIFEST update fails.
   - Cover minimum tests 11-13.
4. `M3E3D`: Optional WAL segment metadata sidecars.
   - Add `ObjectLayout::wal_segment_metadata(segment_id)` targeting
     `meta/wal/<16-hex-segment-id>`.
   - Add optional sidecar publish/load behavior and fallback facts.
   - Update `l2-object-layout.md` for the sidecar object name.
   - Cover minimum tests 16-18.

Each slice should update `v1-progress-tracker.md` only after its implementation
and verification are complete. Test labels such as `M3E3A` belong only in docs
and tracker entries; they must not appear in production type names, function
names, file names, comments, or error names.

## Non-Goals

1. No primitive snapshot DTO serialization.
2. No row-native snapshot materialization from L6.
3. No snapshot row install.
4. No user-facing checkpoint command.
5. No background checkpoint scheduler.
6. No process-kill crash harness.
7. No object-store snapshot fencing.
8. No cache-mode durable snapshot substitute.
9. No WAL segment deletion from checkpoint success; WAL retention remains a
   caller-supplied lifecycle operation.
