# M3E3 / M3TC3 Test Suite Plan: Snapshot, Checkpoint, And Sidecar Services

Status: test-suite plan

Parent plan: `docs/architecture/implementation-plans/m3-m3t-implementation-plan.md`

Implementation brief:
`docs/architecture/implementation-plans/m3e3-snapshot-checkpoint-sidecar-implementation-brief.md`

## Goal

Bring the M3E3 snapshot, checkpoint, and sidecar services to reference-grade
durability coverage before L8 recovery and retention depend on them.

The snapshot object is a durable recovery object. The checkpoint service is the
ordering boundary that prevents MANIFEST from pointing at a snapshot before the
snapshot object is durably published. Sidecars are optional accelerators and
must fail softer than authoritative snapshot, WAL, and MANIFEST objects.

M3TC3 must prove success paths, malformed-byte paths, publish uncertainty,
orphan-snapshot windows, retention protection, and optional sidecar fallback
with adversarial tests rather than compile-only or happy-path tests.

## Testing Principles

1. Snapshot tests model physical storage mechanics, not product semantics.
2. Snapshot section payload bytes are opaque. Tests must not assert primitive
   DTO meaning inside L4.
3. MANIFEST must not point to a snapshot unless the snapshot publication
   succeeded durably.
4. Snapshot publication uncertainty must stop checkpoint sequencing before
   MANIFEST snapshot facts are persisted.
5. A snapshot published before a failed MANIFEST watermark update is an orphan,
   not a corrupt database.
6. Snapshot pruning is retention-intent execution only. Tests must provide the
   live snapshot id and retained count explicitly.
7. Sidecars are optional. Missing, corrupt, or mismatched sidecars must be
   distinguishable from authoritative object corruption.
8. Every required test family needs a sensitivity probe: temporarily mutate the
   implementation so at least one test fails for the intended reason.
9. Test labels such as `M3TC3` belong in docs and tracker entries only. They
   must not appear in production file names, type names, comments, or error
   names.

## Scope

In scope:

1. `crates/storage-next/src/service/snapshot.rs`.
2. Optional `crates/storage-next/src/service/sidecar.rs`.
3. Snapshot service module-local tests.
4. Checkpoint sequencing tests over `SnapshotService` and
   `DatabaseManifestService`.
5. Sidecar service module-local tests.
6. Snapshot publish fault windows expressed through deterministic fake
   backends.
7. Local filesystem durable snapshot behavior.
8. Memory/cache backend rejection of durable snapshot and sidecar publication.
9. Prune/delete report behavior.

Out of scope:

1. Full L8 recovery health classification.
2. Process-kill crash tests.
3. L6 row-native snapshot payload construction.
4. Snapshot row install.
5. Public checkpoint commands.
6. Background checkpoint scheduling.
7. Object-store durable snapshot fencing.
8. Cache-mode durable snapshot objects.
9. Primitive DTO snapshot serialization.

## Current Coverage

Already covered by earlier M3 work:

1. M3C4 format tests cover snapshot header, section, container, CRC,
   materialized limits, borrowed visitor, pre-V1/future-version, invalid
   codec, zero snapshot id, reserved bytes, and golden vectors.
2. M3C2 format tests cover segment metadata sidecar bytes.
3. M3E1/M3TE1 manifest tests cover snapshot facts, flush facts, manifest
   replacement, corrupt current MANIFEST, and publish-failure preservation.
4. M3D/M3TC1 publisher tests cover local filesystem temp-write, temp-sync,
   final publish, parent sync, and create/replace precondition windows.
5. M3E2/M3TC2 WAL tests cover WAL append, read, rotation, retention delete,
   and optional sidecar deferral.

Baseline coverage gaps before M3TC3:

1. No snapshot service exists.
2. No service-level validation of snapshot codec and database identity exists.
3. No snapshot object listing/latest logic exists.
4. No checkpoint sequencing test proves MANIFEST update order around snapshot
   publication.
5. No test proves snapshot publish uncertainty prevents MANIFEST snapshot facts
   from being persisted.
6. No prune test protects the live MANIFEST snapshot.
7. No sidecar service tests distinguish missing/corrupt optional sidecars from
   authoritative failures.
8. No state-machine test exercises create/list/latest/prune interactions.

## Target Test Files

Primary module-local files:

1. `crates/storage-next/src/service/snapshot.rs`
2. `crates/storage-next/src/service/snapshot/tests.rs`, if `snapshot.rs`
   crosses the file-size review threshold.
3. `crates/storage-next/src/service/snapshot/tests/publish.rs`
4. `crates/storage-next/src/service/snapshot/tests/load.rs`
5. `crates/storage-next/src/service/snapshot/tests/checkpoint.rs`
6. `crates/storage-next/src/service/snapshot/tests/prune.rs`
7. `crates/storage-next/src/service/sidecar.rs`
8. `crates/storage-next/src/service/sidecar/tests.rs`
9. `crates/storage-next/src/service/snapshot/tests/support.rs`

Optional integration file and required fuzz files:

1. `crates/storage-next/tests/service_fault_windows.rs`
2. `crates/storage-next/src/testkit/service_fuzz.rs`
3. `crates/storage-next/fuzz/fuzz_targets/service_snapshot.rs`

The default should remain module-local tests because the services are
crate-private L4 services. Add testkit exposure only if integration or fuzz
coverage genuinely needs it.

## Test Families

### 1. Construction And Capability Tests

Required cases:

1. Snapshot service can be constructed over memory backend for optional loads.
2. Optional load on memory backend returns missing.
3. Required load on memory backend returns typed missing.
4. Snapshot durable create on memory backend returns unsupported durable
   publish.
5. Sidecar durable publish on memory backend returns unsupported durable
   publish.
6. Local filesystem backend publishes durable snapshot bytes.
7. Local filesystem backend publishes durable sidecar bytes.
8. Backend missing `ReadObject` fails loads before decode.
9. Backend missing `ListPrefix` fails listing before parsing.
10. Backend missing `DeleteObject` fails prune before deleting.
11. Backend missing durable publish or durable sync fails publication before
    encoding is treated as durable success.

Exit gate:

1. Unsupported durable behavior fails at the service boundary, not as fake
   durability.

### 2. Snapshot Publish Tests

Required cases:

1. Snapshot id `0` is rejected before backend access.
2. Snapshot watermark `0` is rejected by checkpoint publication before backend
   access.
3. Empty codec id is rejected as encode/validation failure before publish.
4. Oversized codec id is rejected before publish.
5. Codec id containing NUL is rejected before publish.
6. Section kind `0` is rejected before publish.
7. A single-section snapshot publishes and reloads exactly.
8. A multi-section snapshot preserves section order and opaque payload bytes.
9. Snapshot durable create refuses an existing snapshot id.
10. Failed duplicate publish preserves existing snapshot bytes.
11. Snapshot publish returns object name, snapshot id, watermark, timestamp,
    byte count, section count, and durable publish facts.
12. The service uses `ObjectLayout::snapshot`, not ad hoc object strings.

Publish failure matrix:

1. `Unsupported` preserves old bytes or absence.
2. `PreconditionFailed` preserves old bytes.
3. `FailedBeforeVisibility` preserves old bytes or absence.
4. `VisibilityUnknown` returns publish uncertainty and does not claim old or
   new bytes.
5. `VisibleDurabilityUnconfirmed` returns publish uncertainty and exposes the
   snapshot object as visible but not proven durable.

Exit gate:

1. No failed publish is reported as a successful durable snapshot.

### 3. Snapshot Load And Validation Tests

Required cases:

1. Missing optional load returns `Ok(None)`.
2. Missing required load returns typed missing with snapshot role and object
   name.
3. Valid bytes decode into the expected header and sections.
4. Future-version bytes return decode error.
5. Pre-V1 development-version bytes return decode error.
6. Container CRC mismatch returns decode error.
7. Truncated header returns decode error.
8. Truncated codec id returns decode error.
9. Truncated section header returns decode error.
10. Truncated section payload returns decode error.
11. Trailing partial section bytes return decode error.
12. Invalid section kind returns decode error.
13. Codec mismatch returns typed codec mismatch with expected and actual codec.
14. Database id mismatch returns typed database mismatch.
15. Backend read failure other than `NotFound` returns typed read/backend
    failure.
16. Borrowed visitor validates footer CRC before invoking the callback.
17. Borrowed visitor does not allocate section payloads.
18. Visitor callback failure propagates without being recast as corruption.

Exit gate:

1. Load paths distinguish missing, backend failure, corrupt bytes, codec
   mismatch, and database mismatch.

### 4. Snapshot Listing And Latest Tests

Required cases:

1. Empty snapshot family lists no snapshots.
2. Multiple snapshots list in numeric snapshot-id order.
3. Latest snapshot returns the highest listed snapshot id.
4. `snapshots/<16-lowercase-hex>` parses as snapshot object.
5. Uppercase hex inside snapshot family is rejected.
6. Short id inside snapshot family is rejected.
7. Overlong id inside snapshot family is rejected.
8. Non-hex id inside snapshot family is rejected.
9. Nested object inside snapshot family is rejected.
10. Adjacent object family names matched by backend prefix behavior are ignored
    unless the first path component is exactly `snapshots`.
11. Backend list failure returns typed list error and attempts no reads.

Exit gate:

1. Malformed snapshot-family objects cannot be silently ignored as if recovery
   state were clean.

### 5. Checkpoint Sequencing Tests

Required success cases:

1. Checkpoint requires an existing database MANIFEST.
2. Checkpoint validates manifest codec before snapshot publish.
3. Checkpoint persists active WAL segment before snapshot publish.
4. Checkpoint publishes snapshot before MANIFEST snapshot facts.
5. Checkpoint persists snapshot id and snapshot watermark after snapshot
   publish.
6. Checkpoint outcome reports snapshot id, watermark, active WAL segment,
   timestamp, section count, byte count, and publish facts.

Required failure cases:

1. Missing MANIFEST fails before snapshot publish.
2. Corrupt MANIFEST fails before snapshot publish.
3. Codec mismatch fails before snapshot publish.
4. Active WAL segment `0` is rejected before snapshot publish.
5. Snapshot id `0` is rejected before MANIFEST mutation.
6. Snapshot watermark `0` is rejected before MANIFEST mutation.
7. Active WAL manifest update failure stops before snapshot publish.
8. Snapshot publish failure stops before MANIFEST snapshot facts.
9. Snapshot publish `VisibilityUnknown` stops before MANIFEST snapshot facts.
10. Snapshot publish `VisibleDurabilityUnconfirmed` stops before MANIFEST
    snapshot facts.
11. Final MANIFEST snapshot-fact no-visible failure returns typed orphan
    snapshot facts.
12. Final MANIFEST snapshot-fact `VisibilityUnknown` and
    `VisibleDurabilityUnconfirmed` failures return typed final-MANIFEST
    uncertainty facts, not orphan facts.
13. After final MANIFEST no-visible failure, reloading MANIFEST does not point
    to the orphan snapshot.
14. After final MANIFEST update failure, direct snapshot load by id can still
    load the snapshot if snapshot publish made it visible.

Exit gate:

1. Every failure is classified by where it happened in the sequence: before
   snapshot visibility, publish uncertainty, or orphan-after-publish.

### 6. Prune And Delete Tests

Required cases:

1. `retain_newest = 0` is clamped to one.
2. When snapshot count is at or below retain count, no deletes happen.
3. Newest retained snapshots are protected.
4. Live MANIFEST snapshot is protected even when older than the retained set.
5. Snapshots older than the retained set and not live are deleted.
6. Delete report lists deleted snapshots in ascending id order.
7. Delete report lists protected snapshots in ascending id order.
8. Delete report lists failed snapshots in ascending id order.
9. Individual delete failure does not hide successful deletes.
10. Delete failure does not report the snapshot as deleted.
11. Backend list failure fails before any delete is attempted.
12. Malformed snapshot-family name during prune fails before any delete is
    attempted.
13. Prune never deletes non-snapshot-family objects.

Exit gate:

1. Prune behavior is fact-driven and cannot remove the MANIFEST-live snapshot.

### 7. Sidecar Tests

Required cases:

1. Segment metadata sidecar object name is `meta/wal/<16-hex-segment-id>`.
2. Segment id `0` is rejected before object-name construction.
3. Sidecar publish/read roundtrips segment metadata bytes.
4. Sidecar publish uses durable replace.
5. Missing sidecar returns recoverable missing fact.
6. Corrupt magic returns recoverable corrupt fact.
7. Future sidecar version returns recoverable corrupt fact.
8. Pre-V1 sidecar version returns recoverable corrupt fact.
9. Sidecar CRC mismatch returns recoverable corrupt fact.
10. Sidecar trailing bytes return recoverable corrupt fact.
11. Decoded segment id mismatch returns recoverable corrupt fact.
12. Backend read failure other than `NotFound` returns typed backend error.
13. Sidecar publish failure preserves `PublishFailureKind`.
14. Sidecar publish failure does not touch WAL segment objects.
15. Sidecar delete failure is reported but does not affect authoritative WAL
    state.

Exit gate:

1. Missing and corrupt optional sidecars are observable fallback facts, not
   authoritative recovery failures.

### 8. Fault-Window Tests

Required cases:

1. Snapshot publish temporary-write failure returns
   `FailedBeforeVisibility`.
2. Snapshot publish temporary-sync failure returns `FailedBeforeVisibility`.
3. Snapshot publish final-create precondition returns `PreconditionFailed`.
4. Snapshot publish final-publish failure returns `VisibilityUnknown` when the
   backend cannot prove final visibility.
5. Snapshot publish parent-sync failure returns
   `VisibleDurabilityUnconfirmed`.
6. Checkpoint does not persist snapshot facts for any snapshot publish failure
   or uncertainty kind.
7. Checkpoint reports orphan snapshot facts when final MANIFEST update fails
   after snapshot publish.
8. Sidecar publish uncertainty does not change authoritative WAL or snapshot
   facts.

Exit gate:

1. Fault windows are tested at the service boundary, not only through the
   lower publisher.

### 9. State-Machine Property Tests

Required property:

1. Generate operation sequences of length 1-96 over:
   create snapshot, load snapshot, list snapshots, latest snapshot, prune,
   publish sidecar, corrupt sidecar, load sidecar.
2. Snapshot ids use a small range with duplicates to exercise create
   preconditions.
3. Retain counts use 0-5 to exercise clamping.
4. At least one live snapshot id may be selected from currently known
   snapshots.
5. The model tracks visible snapshots, protected snapshots, sidecar states,
   and expected latest id.
6. Failed create before visibility must not change model-visible snapshots.
7. Create precondition failure must preserve prior bytes.
8. Prune must never remove model-protected snapshots.
9. Sidecar missing/corrupt must not affect model-visible snapshots.

Use hand-rolled `proptest`, not `proptest-state-machine`, to match the M3E1
and M3E2 approach. Regression seeds should be checked in under
`crates/storage-next/proptest-regressions/snapshot_service_state_machine.txt`
only when a seed actually fails during development or CI.

Exit gate:

1. The property fails if snapshot creation, listing, latest selection, prune
   protection, or sidecar fallback drifts from the model.

### 10. Service Fuzz Tests

M3TC3 pulls service fuzzing forward through a hidden testkit entry point rather
than deferring it to later recovery layers.

Add `service_snapshot` under `crates/storage-next/fuzz/fuzz_targets/`. The
target should route arbitrary bytes into a bounded service-operation script over
a hidden durable in-memory backend. The script must exercise snapshot create,
load, list, latest, prune, sidecar publish, sidecar corruption, and sidecar load
operations.

Fuzz invariants are:

1. No panics except the fuzz target's intentional invariant-failure panic.
2. No unbounded allocation; generated scripts must cap operation count and
   payload size.
3. Snapshot listing and latest facts match the model after every operation.
4. Snapshot loads either return the model payload or a typed missing error.
5. Duplicate immutable snapshot creates return precondition failure and preserve
   prior visible bytes.
6. Prune never deletes model-protected snapshots.
7. Sidecar missing/corrupt/present states remain recoverable and never mutate
   authoritative snapshot state.

The existing M3C5 `format_snapshot_envelope` target remains the byte-decoder
fuzz target. `service_snapshot` is the service-level operation fuzzer.

## Adversarial Implementation Protocol

Each M3TC3 slice must use this closeout record in
`docs/architecture/v1-progress-tracker.md`:

1. Suite cases covered.
2. Narrow command.
3. Sensitivity probe.
4. Failure observed.
5. Revert proof.
6. Broad command.

Acceptable sensitivity probes include:

1. Temporarily persist MANIFEST snapshot facts before snapshot publish.
2. Temporarily allow snapshot id `0`.
3. Temporarily ignore malformed snapshot-family names.
4. Temporarily delete the live snapshot during prune.
5. Temporarily treat corrupt sidecar as authoritative corruption.
6. Temporarily collapse `VisibleDurabilityUnconfirmed` into success.

Tests written only against current behavior are not enough. At least one probe
per slice must prove the test would catch the targeted failure mode.

## Suggested Slice Order

1. `M3TC3A`: Snapshot construction, publish, load, and validation matrix.
2. `M3TC3B`: Listing, latest, prune, and state-machine property.
3. `M3TC3C`: Checkpoint sequencing and orphan-snapshot windows.
4. `M3TC3D`: Sidecar optional-fallback behavior.
5. `M3TC3E`: Snapshot publish fault-window matrix and closeout audit.

## Exit Gate

M3TC3 is complete when:

1. Every required case above is implemented or explicitly deferred with a
   reason tied to a later milestone.
2. All sensitivity probes are recorded.
3. `cargo test -p strata-storage-next --locked service::snapshot` passes.
4. `cargo test -p strata-storage-next --locked` passes.
5. `cargo test -p strata-storage-next --features testkit,fault-injection --locked`
   passes if testkit or fault-injection surfaces are used.
6. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
   passes.
7. `cargo doc -p strata-storage-next --no-deps --locked` passes.
8. Comment standards from `docs/architecture/v1-engineering-standards.md` are
   satisfied for every durable publish, checkpoint ordering, orphan snapshot,
   and sidecar fallback invariant.
