# M3E2 Implementation Brief: WAL Service

Status: implementation brief

Parent plan: `docs/architecture/implementation-plans/m3-m3t-implementation-plan.md`

## Goal

Implement the first L4 WAL service over the storage-next backend, layout, and
V1 WAL format layers.

M3E2 should produce a durable-local WAL service that can:

1. Create and open V1 WAL segment objects.
2. Append committed WAL records.
3. Apply `standard` and `always` durability policy mechanics.
4. Read WAL segments strictly for recovery.
5. Report truncation and corruption facts without interpreting commit payloads.

Cache mode remains WAL-free. Object-store durable WAL remains post-V1
substrate work.

## Inputs Read

Architecture and plan inputs:

1. `docs/architecture/storage/l1-backend-io.md`
2. `docs/architecture/storage/l2-object-layout.md`
3. `docs/architecture/storage/l3-durable-format-codec.md`
4. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
5. `docs/architecture/storage/l7-commit-runtime.md`
6. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
7. `docs/spec/strata-storage-format-v1.md`
8. `docs/architecture/implementation-plans/m3-m3t-implementation-plan.md`
9. `docs/architecture/implementation-plans/m3-porting-log.md`

Current implementation evidence:

1. `crates/storage/src/durability/wal/mod.rs`
2. `crates/storage/src/durability/wal/config.rs`
3. `crates/storage/src/durability/wal/mode.rs`
4. `crates/storage/src/durability/wal/writer.rs`
5. `crates/storage/src/durability/wal/reader.rs`
6. `crates/storage/src/durability/format/wal_record.rs`
7. `crates/storage/src/durability/format/segment_meta.rs`
8. `crates/storage/src/durability/commit_adapter.rs`
9. `crates/storage/src/durability/recovery.rs`
10. `crates/storage/src/durability/recovery_bootstrap.rs`

Storage-next implementation inputs:

1. `crates/storage-next/src/backend/mod.rs`
2. `crates/storage-next/src/backend/local_fs.rs`
3. `crates/storage-next/src/backend/memory.rs`
4. `crates/storage-next/src/config/mode.rs`
5. `crates/storage-next/src/format/wal.rs`
6. `crates/storage-next/src/format/segment_metadata.rs`
7. `crates/storage-next/src/layout/mod.rs`
8. `crates/storage-next/src/service/manifest.rs`
9. `crates/storage-next/src/service/publish.rs`

## Existing Behavior To Preserve

1. WAL records are the durable commit point for durable local storage. A commit
   must not become visible until L4 accepts the WAL record under the selected
   durability policy.
2. Fresh durable databases start on WAL segment `1`.
3. WAL segments are ordered by numeric segment id and contain a self-identifying
   segment header before any record envelopes.
4. Segment rotation happens before appending a record that would exceed the
   configured segment size.
5. `standard` accepts append once the record is written into the WAL path, then
   relies on a later durability barrier scheduled by lifecycle code.
6. `always` forces the durability barrier before reporting append success.
7. WAL readers verify the segment header before trusting record bytes.
8. WAL record envelopes verify length CRC before trusting payload length.
9. Inner WAL records verify record-length CRC and payload CRC before parsing
   fields.
10. A partial tail at the end of the latest segment is distinguishable from
    mid-segment corruption.
11. Segment metadata sidecars are optional accelerators. Missing or corrupt
    sidecars must not invalidate an otherwise valid WAL segment.
12. Active WAL segment deletion is protected. The caller supplies the
    covered-through retention watermark, and the service protects its own active
    segment and newer segments.
13. WAL counters and disk/object usage facts remain service facts, not product
    semantics.

## Intentional V1 Changes

1. Cache mode has no WAL service. Memory backend tests may exercise unsupported
   behavior, but cache lifecycle must not create WAL objects.
2. WAL records carry `CommitVersion`, `BranchId`, `Timestamp`, and commit
   payload bytes. Public transaction ids are not reintroduced.
   Supersession note: M3E2 originally kept the payload opaque so the WAL
   service could land before L7. M3F replaces that temporary shape with a
   row-native commit payload before L7 depends on WAL replay.
3. Stable V1 WAL segment, envelope, and inner-record formats use version `1`;
   pre-V1 development versions are rejected by the normal decoder.
4. WAL object names come only from `ObjectLayout::wal_segment` and
   `ObjectLayout::wal_prefix`.
5. The L4 contract exposes WAL operations, not file handles, paths, or
   appendable streams.
6. For the V1 local filesystem implementation, WAL append is an L4 operation
   backed by backend-owned append/sync primitives. If new backend methods are
   needed, they must remain object-name based and must not leak `std::fs::File`
   outside `backend::local_fs`.
7. The service must not implement WAL append by durable full-object replacement
   for every commit. That would make `standard` behave like `always`, rewrite
   whole segments, and hide the proven append/sync behavior from the current
   implementation.
8. Background sync scheduling is not a public L4 API in M3E2. The WAL service
   should expose a clear `force_durable` operation and enough dirty-state facts
   for L8 to schedule periodic sync later.
9. Lossy recovery is not the default reader behavior. If scan-forward lossy
   recovery is ported in M3E2, it must be behind an explicit option and must not
   use follower-mode vocabulary.
10. Future object-store WAL chunking is not implemented, but the service shape
    must not require callers to know whether the backend used append, chunks, or
    another append-equivalent protocol.

## Target Files

Implementation files:

1. `crates/storage-next/src/service/wal.rs`
2. `crates/storage-next/src/service/mod.rs`
3. `crates/storage-next/src/backend/mod.rs`, if an object-name based
   append/sync primitive is needed.
4. `crates/storage-next/src/backend/local_fs.rs`, for local durable WAL
   append/sync mechanics.
5. `crates/storage-next/src/backend/memory.rs`, only to return explicit
   unsupported WAL durability behavior if the backend trait grows.
6. `crates/storage-next/src/config/mode.rs`, if WAL-specific config belongs
   next to durability policy.
7. `crates/storage-next/src/format/wal.rs`, only if service-consumed support
   functions or accessors are missing.
8. `crates/storage-next/src/format/segment_metadata.rs`, only if M3E2 writes or
   reads optional sidecars.

Test files:

1. Module-local tests in `crates/storage-next/src/service/wal.rs`.
2. Module-local backend tests in `crates/storage-next/src/backend/local_fs.rs`
   if append/sync primitives are added.
3. `crates/storage-next/tests/service_fault_windows.rs`, for WAL fault-window
   coverage that needs a real backend.
4. `crates/storage-next/tests/cache_mode_absence.rs`, only if M3TD begins in
   the same slice.
5. `crates/storage-next/tests/README.md`, only if new manual commands or
   feature requirements are introduced.

Documentation files:

1. `docs/architecture/implementation-plans/m3-porting-log.md` must receive an
   M3E2 source-map note before production code changes.
2. `docs/architecture/v1-progress-tracker.md` should be updated only after
   M3E2 implementation and verification are complete.
3. `docs/architecture/storage/l1-backend-io.md` and
   `docs/architecture/storage/l4-log-manifest-snapshot-services.md` should
   be updated if M3E2 closes the append/sync open question with concrete backend
   method names.

## Service Shape

Exact Rust names can adjust during implementation, but the shape should stay
small and mechanical.

WAL configuration:

1. Segment size defaults to the current 64 MiB value unless the storage-next
   runtime profile later overrides it.
2. Testing configuration may use smaller segments for deterministic rotation.
3. Segment size validation should reject values too small to hold a segment
   header plus at least one realistic record envelope.
4. Buffered-sync thresholds from the old public `WalConfig` are not required in
   M3E2 unless the service actively consumes them.

WAL service construction:

1. Constructed from `&dyn Backend`, database id, active segment id, configured
   segment size, and durability policy.
2. Rejects segment id `0`.
3. Requires durable local backend capabilities for durable policies.
4. Creates the active segment header when the segment object is missing.
5. Validates an existing active segment header against the requested segment id
   and database id.
6. Does not create a WAL service for cache mode.

WAL append:

1. Accepts `WalRecord` or already-encoded inner record bytes plus record facts.
2. Applies the storage codec before wrapping the outer WAL envelope. M3E2 may
   use identity codec only if non-identity codec plumbing is still deferred.
3. Appends one complete envelope to the active segment.
4. Rotates to the next segment before append if the active segment would exceed
   the configured segment size.
5. Tracks segment metadata facts from the record: min/max commit version,
   min/max timestamp, and record count.
6. Returns append facts: segment id, record start offset, bytes written, dirty
   byte count, and whether the required durability barrier was forced.
7. Propagates append failures without marking the record durable.
8. In `always`, calls the durability barrier before reporting success.
9. In `standard`, records dirty state and lets `force_durable` or L8 scheduling
   force durability later.

WAL durability:

1. `force_durable` flushes the active segment through the backend-owned durable
   sync primitive.
2. `close` forces durability when durability policy requires it and returns a
   typed error if the barrier fails.
3. Dirty counters are reset only after a successful barrier.
4. A sync failure must remain visible to higher layers. It must not be logged
   and ignored.

WAL read:

1. Lists WAL objects through `ObjectLayout::wal_prefix`.
2. Sorts segment objects by decoded segment id, not raw backend list order.
3. Reads each segment through backend object/range reads.
4. Validates header database id and segment id before decoding envelopes.
5. Returns records in segment order.
6. Provides `read_all` and `read_after_commit_version` at minimum.
7. May provide contiguous-after-watermark recovery facts if the old reader logic
   is ported, but must avoid follower-mode semantics.
8. Treats absent WAL prefix as an empty log for a new database path only when
   lifecycle has established that missing WAL is allowed.

WAL truncation and deletion:

1. Exposes a mechanical delete operation that requires the caller to supply a
   durable retention watermark.
2. Refuses to delete the active segment.
3. Refuses to delete segments newer than or equal to the service's active
   segment id.
4. Deletes older covered segments best-effort per object and reports individual
   failures.
5. Does not decide retention policy. L8 owns that proof.

Optional segment metadata:

1. Segment metadata sidecars are optional in M3E2.
2. If sidecars are implemented now, they must use the V1
   `SegmentMetadata` codec and be rebuildable from authoritative WAL records.
3. Missing sidecars must fall back to scanning WAL segment bytes.
4. Corrupt sidecars must produce a warning/rebuild fact or typed recoverable
   error, not WAL corruption.

## Fault And Recovery Rules

1. Failure before any envelope bytes are appended leaves no durable record.
2. Failure after a partial envelope append is recovered as a partial tail only
   if it is in the latest segment.
3. A partial envelope in a non-latest segment is corruption.
4. Header checksum, header database-id mismatch, and segment-id mismatch are
   hard segment errors.
5. Outer-envelope length CRC mismatch is corruption in strict mode.
6. Inner-record length CRC or payload CRC mismatch is corruption in strict mode.
7. Codec decode failure must remain distinct from generic corruption.
8. `always` sync failure after append must return a typed durability failure;
   higher layers must not treat the commit as visible.
9. `standard` background/periodic sync failure, once L8 wires it, must latch a
   writer-health fault until lifecycle clears or reopens the database.
10. A deleted WAL segment must never be required for recovery according to the
    durable retention proof supplied by L8.

## Test Plan

This section defines the minimum M3E2 implementation tests. The comprehensive
follow-up suite is defined in
`docs/architecture/implementation-plans/m3e2-wal-test-suite-plan.md` and should
land as `M3TC2` before later layers treat WAL behavior as reference-grade.

Minimum M3E2 tests:

1. Missing active segment creates a V1 segment header and appends the first
   record after the header.
2. Existing active segment with matching header opens and appends.
3. Existing active segment with wrong segment id is rejected.
4. Existing active segment with wrong database id is rejected.
5. Append/read roundtrips multiple WAL records with empty and non-empty commit
   payloads.
6. Segment listing does not depend on backend list order.
7. Rotation creates the next segment before an append would exceed segment
   size.
8. Rotation preserves the previous segment as readable and appends the record
   to the new segment.
9. `always` forces a durability barrier before returning append success.
10. `standard` does not force per-append durability but `force_durable` clears
    dirty facts.
11. Memory/cache backend cannot construct a durable WAL service.
12. Truncated latest-segment envelope reports partial-tail truncation facts.
13. Truncated non-latest segment reports corruption.
14. Mid-segment envelope length CRC mismatch reports corruption.
15. Inner WAL record checksum mismatch reports corruption.
16. Segment header corruption is reported before record scanning.
17. Active segment deletion is refused.
18. Covered old-segment deletion skips individual delete failures and reports
    them.
19. Optional sidecar missing/corrupt falls back to segment scanning, if sidecars
    are implemented in M3E2.
20. Service tests assert object names come from `ObjectLayout`, not hard-coded
    string construction.

Fault-window coverage:

1. Append failure before bytes are visible.
2. Append failure after partial bytes are visible.
3. Sync failure in `always`.
4. Close/force-durable sync failure.
5. Delete failure during safe WAL pruning.
6. Faults should use backend/testkit hooks, not production-only branch code.

## Implementation Order

1. Append an `M3E2` source-map note to `m3-porting-log.md`.
2. Expose any missing crate-private WAL format support functions needed by the
   service.
3. Add object-name based backend append/sync primitives only if the current
   backend contract cannot express durable-local WAL append correctly.
4. Implement local filesystem append/sync behind `backend::local_fs`; return
   unsupported from memory/cache for durable WAL behavior.
5. Add `service::wal` error types and configuration types.
6. Implement segment create/open/header validation.
7. Implement append, rotation, dirty counters, `force_durable`, and `close`.
8. Implement strict segment/list/read recovery paths.
9. Implement safe deletion mechanics with caller-supplied retention watermark
   and service-owned active segment protection.
10. Add optional segment metadata sidecar support only if it does not widen the
    slice beyond WAL mechanics.
11. Add module-local tests and service fault-window tests.
12. Run M3 verification commands and update the progress tracker after the
    implementation is complete.

Recommended verification:

```bash
cargo test -p strata-storage-next --locked
cargo test -p strata-storage-next --features testkit,fault-injection --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## Non-Goals

1. No commit runtime, version allocation, or visible-version publication. Those
   are M6/M7-layer concerns.
2. No row-native commit payload format finalization in M3E2. That temporary
   deferral is closed by M3F before L7 depends on WAL replay.
3. No cache-mode WAL.
4. No object-store WAL chunking, fencing, or OpenDAL durable mode.
5. No public transaction API.
6. No follower-mode contiguous-refresh semantics.
7. No snapshot/checkpoint service. That is M3E3.
8. No quarantine service. That is M3E4.
9. No full lifecycle recovery owner. L8 will decide when WAL replay,
   lossy recovery, truncation, and health classification run.
10. No public background-sync API.
