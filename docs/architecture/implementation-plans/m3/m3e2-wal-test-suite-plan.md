# M3E2 / M3TC2 Test Suite Plan: WAL Service

Status: test-suite plan

Parent plan: `docs/architecture/implementation-plans/m3-m3t-implementation-plan.md`

Implementation brief:
`docs/architecture/implementation-plans/m3e2-wal-service-implementation-brief.md`

Implementation plan:
`docs/architecture/implementation-plans/m3tc2-wal-test-implementation-plan.md`

## Goal

Bring the M3E2 WAL service from "implemented with focused tests" to
reference-grade durability coverage.

The WAL is the durable commit boundary for local durable storage. Its tests
must prove not only that records append and read correctly, but that the service
fails safely under malformed bytes, backend faults, partial writes, sync
uncertainty, segment rotation, retention, and reopen scenarios.

This plan defines the comprehensive M3E2 test suite to implement as `M3TC2`
before later storage layers rely on WAL behavior as trusted infrastructure.

## Testing Principles

1. WAL tests model durability mechanics, not product semantics. They must not
   mention transactions, branch promotion, search, graph, or user-facing commit
   meaning.
2. Every accepted WAL record must have a model-observable append fact:
   segment id, start offset, byte length, commit version, branch id, timestamp,
   and payload bytes.
3. Every failed append must leave the service in a state that is either
   unchanged, explicitly dirty-but-known, or explicitly corrupt/uncertain.
4. Latest-segment partial tails are recoverable facts. Mid-segment corruption
   and non-latest partial tails are hard corruption.
5. Dirty counters are storage facts. They clear only after a successful durable
   barrier.
6. Tests should use deterministic fake backends and faulting backends before
   process-level crash tests. Crash tests come later when lifecycle recovery
   owns reopen orchestration.
7. Current storage WAL code and tests are evidence, not authority. Preserve
   behavior only when it matches the V1 architecture.
8. Test labels such as `M3TC2` belong in docs and tracker entries only. They
   must not appear in production file names, type names, comments, or error
   names.

## Scope

In scope:

1. `crates/storage-next/src/service/wal.rs` service behavior.
2. WAL service module-local tests in
   `crates/storage-next/src/service/wal/tests.rs`.
3. Private WAL test support under `crates/storage-next/src/service/wal/` if the
   test suite needs reusable builders or model state.
4. Fault-window tests in `crates/storage-next/tests/service_fault_windows.rs`
   only if the required surface can be reached through the feature-gated
   testkit without turning testkit into a second production API.
5. Format fuzz targets that exercise WAL service inputs through L3 bytes.
6. Local filesystem durable WAL behavior.
7. Memory/cache backend rejection of durable WAL behavior.

Out of scope:

1. Full L7 WAL-before-visible commit pipeline.
2. Full L8 recovery health classification.
3. Process-kill crash testing.
4. Object-store WAL chunking.
5. Public WAL APIs.
6. Cache-mode WAL.
7. Sidecar metadata if M3E2 does not implement sidecars.

## Current Coverage

The current M3E2 tests already cover:

1. Memory backend cannot open a durable WAL service.
2. Missing active segment creates a V1 segment header.
3. Append and read roundtrip for multiple records.
4. Reopen rebuilds active segment metadata.
5. `always` forces durability per append.
6. `standard` tracks dirty facts and `force_durable` clears them.
7. Segment rotation before exceeding configured size.
8. Wrong segment id and wrong database id headers are rejected.
9. Latest-segment partial tails produce truncation facts.
10. Latest-segment partial tails prevent blind append after reopen.
11. Latest-segment partial tails prevent rotation after reopen.
12. Non-latest partial tail is corruption.
13. Mid-segment corruption is rejected.
14. Active segment deletion is protected.
15. Covered old segments are deleted after rotation.
16. Backend append misreports for bytes-written and metadata size are rejected.

Baseline coverage gaps before `M3TC2`:

1. Model/property test for long append sequences across rotations. Closed by
   `M3TC2B`.
2. Systematic corruption matrix. Closed by `M3TC2D`.
3. Per-operation fault-window matrix for append, sync, list, read, metadata,
   create, and delete. Append misreport and pre-append failure coverage is
   closed by `M3TC2B`; sync failure coverage is closed by `M3TC2C`; list,
   read, metadata, create, delete, and partial-visibility cases are closed by
   `M3TC2E`.
4. Exact-boundary segment-size tests. Closed by `M3TC2B`.
5. Backend capability-missing matrix. Closed by `M3TC2A`.
6. Dirty-counter behavior under sync failure. Closed by `M3TC2C`.
7. Explicit close-failure test. Closed by `M3TC2C`.
8. List-order and non-WAL-object noise tests. Closed by `M3TC2A`.
9. Deletion failure reporting test. Closed by `M3TC2E`.
10. Read/watermark boundary, duplicate-version, out-of-order-version, and
    mixed-branch-id semantics. Closed by `M3TC2G`.
11. Service-level fuzz target for WAL segment streams. Still open for a later
    fuzz slice if service-level byte fuzzing remains useful after `M3TC2D`.

## Target Test Files

Primary module-local files:

1. `crates/storage-next/src/service/wal/tests.rs`
2. `crates/storage-next/src/service/wal/tests/append.rs`
3. `crates/storage-next/src/service/wal/tests/corruption.rs`
4. `crates/storage-next/src/service/wal/tests/durability.rs`
5. `crates/storage-next/src/service/wal/tests/fault_windows.rs`
6. `crates/storage-next/src/service/wal/tests/localfs.rs`
7. `crates/storage-next/src/service/wal/tests/read.rs`
8. `crates/storage-next/src/service/wal/tests/retention_reopen.rs`
9. `crates/storage-next/src/service/wal/tests/support.rs`

Optional private support:

1. Additional child modules under `crates/storage-next/src/service/wal/tests/`
   when a test family exceeds the unit-test file-size review threshold.

Optional integration/fuzz files:

1. `crates/storage-next/tests/service_fault_windows.rs`
2. `crates/storage-next/fuzz/fuzz_targets/service_wal_segment.rs`

The default should be module-local tests because the WAL service is a
crate-private L4 service. Add testkit exposure only if integration or fuzz
coverage genuinely needs it. If added, the testkit surface must be
`#[doc(hidden)]`, feature-gated, test-only in intent, and narrower than the
production service.

## Test Families

### 1. Construction And Capability Tests

Required cases:

1. Segment id `0` is rejected.
2. Segment size below the minimum is rejected.
3. Memory/cache backend rejects durable WAL construction.
4. A backend missing `ReadObject` is rejected with that capability identified.
5. A backend missing `ReadRange` is rejected with that capability identified.
6. A backend missing `ListPrefix` is rejected with that capability identified.
7. A backend missing `ObjectMetadata` is rejected with that capability
   identified.
8. A backend missing `AppendObject` is rejected with that capability
   identified.
9. A backend missing `DurablePublish` is rejected with that capability
   identified.
10. A backend missing `DurableSync` is rejected with that capability
    identified.
11. Local filesystem backend opens with `standard`.
12. Local filesystem backend opens with `always`.
13. Opening a missing active segment creates exactly one segment header object.
14. Opening an existing valid segment does not rewrite it.
15. Opening with an existing object shorter than a segment header fails closed.
16. Opening with corrupt header magic, version, checksum, or segment id fails
    with `Format` before record scanning.
17. Opening with a header database-id mismatch fails with `DatabaseMismatch`
    before record scanning.

Exit gate:

1. Construction tests prove that unsupported modes fail at open, not at first
   append.

### 2. Segment Listing And Object-Name Tests

Required cases:

1. Backend list order does not affect read order.
2. Non-WAL objects under adjacent prefixes are ignored.
3. Invalid WAL-looking object names under the WAL prefix are rejected as
   `WalServiceError::Backend` with `operation = WalOperation::List`, the
   offending object name preserved, and a source `BackendError` whose kind is
   `InvalidObjectName`. They are not silently ignored and should not be asserted
   as the separate `WalServiceError::List` variant.
4. Listed WAL segments are read in ascending segment-id order independent of
   backend list order. Valid WAL names use fixed-width hex segment ids, so
   lexical path order and numeric segment-id order are equivalent for
   well-formed names.
5. Segment id overflow during rotation returns a typed error.
6. Segment gaps are accepted at the WAL service layer: M3E2 reads listed WAL
   segment objects in numeric order and does not invent missing segment ids.
   Lifecycle recovery can decide later whether a manifest demands contiguity
   from a particular starting segment.

### 3. Append And Rotation Tests

Required cases:

1. Empty payload record appends and reads.
2. Non-empty payload record appends and reads.
3. Large payload that still fits in an empty segment appends and reads.
4. Record larger than the configured segment capacity is rejected before bytes
   are appended.
5. Record that exactly fills the remaining segment capacity stays in the
   current segment.
6. Record that exceeds remaining capacity by one byte rotates before append.
7. Rotation creates a valid new segment header before appending the record.
8. Rotation does not mutate the previous segment after the new segment accepts
   the record.
9. Append offset mismatch is rejected.
10. Append bytes-written mismatch is rejected.
11. Append result metadata-size mismatch is rejected.
12. Active object metadata mismatch before append is rejected before rotation.
13. Active object metadata mismatch after rotation is rejected before append to
    the new segment.
14. Failures before the backend append is accepted do not advance active
    metadata, dirty bytes, dirty records, or active segment size. This includes
    record-too-large rejection, append backend failure, append offset mismatch,
    append length mismatch, and append object-size mismatch.
15. `always`-policy sync failure after the backend append is accepted is a
    different state: active metadata advances, active segment size advances,
    dirty bytes and dirty records remain non-zero, and no successful
    `WalAppend` is returned.

Property test:

1. Generate a sequence of 1 to 128 records with random payload lengths and
   commit versions.
2. Generate small but valid segment sizes, initially 1 KiB to 8 KiB.
3. Append every record to a fresh WAL service.
4. Maintain a model of expected records, segment ids, offsets, and segment
   sizes.
5. Assert after every append that service facts match the model.
6. Reopen and assert `read_all` returns exactly the model records in order.
7. Assert no segment object exceeds the configured segment size.
8. Use a hand-rolled model loop inside `proptest`; do not add
   `proptest-state-machine` unless the model grows beyond this WAL surface.
9. Check failing seeds into
   `crates/storage-next/proptest-regressions/wal_append_model.txt`.

The property test should use `proptest` in the normal test suite, not
`cargo-fuzz`.

### 4. Durability Policy Tests

Required `standard` cases:

1. Append succeeds without forcing sync.
2. Dirty byte and record counters increase after append.
3. `force_durable` syncs the active segment and clears dirty counters.
4. `close` syncs dirty state and clears counters.
5. Sync failure during `force_durable` leaves dirty counters intact.
6. Sync failure during `close` returns a typed error and leaves dirty counters
   intact.

Required `always` cases:

1. Append calls sync before returning success.
2. Successful append returns `forced_durable = true`.
3. Successful append leaves dirty counters at zero.
4. Sync failure after append returns a typed durability error.
5. Sync failure after append does not report the append as durable.
6. Sync failure after append leaves `dirty_bytes()` greater than zero and
   leaves dirty record count non-zero.
7. Sync failure after append leaves the active segment id and active segment
   size reflecting the appended record.
8. Sync failure after append leaves active metadata reflecting the appended
   record.
9. `read_all` after the sync failure returns the appended record with no
   truncation fact when the backend bytes are visible.
10. A subsequent append succeeds normally once sync faults are removed, because
    backend object metadata and service active segment size agree.
11. The test names should call this state "append visible, durability
    unconfirmed." L7/L8 later map this window to `ambiguous_commit.wal_sync`.

Required backend cases:

1. Sync `NotFound` is classified as a backend sync failure.
2. Sync `Interrupted` is classified distinctly from corruption.
3. Sync `Unavailable` remains retry-relevant for lifecycle.

### 5. Read And Watermark Tests

Required cases:

1. `read_all` returns records across multiple segments in append order.
2. `read_after_commit_version(0)` returns all records.
3. `read_after_commit_version(N)` skips records at or below `N`.
4. `read_after_commit_version(MAX)` returns no records.
5. Duplicate commit versions, if accepted by the service, are filtered
   consistently by version.
6. Out-of-order commit versions, if accepted by the service, remain in append
   order and are filtered by version.
7. Records with different branch ids are not interpreted by the WAL service.
8. Records with maximum valid payload sizes read without allocation overflow.
   Use a small deterministic segment size such as 2 KiB and construct a record
   whose encoded envelope fits just under
   `segment_size - WAL_SEGMENT_HEADER_SIZE`. The paired over-limit case belongs
   in the append rejection tests.

The WAL service should not enforce commit-version monotonicity unless the
service contract explicitly adds that rule. Commit ordering belongs to the
commit runtime.

### 6. Corruption Matrix

Required matrix:

| Target | Mutation | Expected result |
|---|---|---|
| Segment header magic | Flip one byte | `Format` error on open/read |
| Segment header version | Future version | `Format` error |
| Segment header checksum | Flip checksum byte | `Format` error |
| Segment header segment id | Mismatch object id | `Format` error |
| Segment header database id | Different database id | `DatabaseMismatch` |
| Envelope length field | Truncate or inflate | Latest tail fact only at latest tail; otherwise `Format` |
| Envelope length CRC | Flip CRC byte | `Format` |
| Envelope payload | Truncate final envelope | Latest tail fact only at latest tail |
| Envelope payload | Truncate non-latest segment | `Format` |
| Envelope payload | Flip encoded-record byte without updating checksums | `Format` |
| Inner record length | Corrupt length | `Format` |
| Inner record length CRC | Flip CRC byte | `Format` |
| Inner record payload CRC | Flip CRC byte | `Format` |
| Inner record payload | Flip payload byte without updating payload CRC | `Format` |
| Inner record branch id | Flip branch-id byte without updating the record-body checksum | `Format` |
| Inner record branch id | Truncate bytes | `Format` |
| Inner record timestamp | Truncate bytes | `Format` |
| Trailing garbage | Latest segment tail | Truncation fact |
| Trailing garbage | Non-latest segment tail | `Format` |

Implementation guidance:

1. Build valid segment bytes through the normal WAL service.
2. Mutate one byte or range at a named boundary.
3. Assert the exact error family, not just "is error."
4. Keep support functions private to WAL tests.
5. Keep the header-id asymmetry explicit: a database-id mismatch is
   `DatabaseMismatch` because it means this WAL belongs to a different
   database, while a segment-id mismatch is `Format` because it corrupts
   recovery metadata for this object.
6. The database-id mismatch case must use a valid header checksum for another
   database id. A raw byte flip without checksum refresh belongs to the header
   checksum case.
7. A shortened envelope length with a refreshed envelope CRC is not a
   recoverable latest-tail fact. It is a valid envelope header around an
   impossible inner record and should fail as `Format`.

### 7. Fault-Window Tests

Required backend operation faults:

| Operation | Fault timing | Expected result |
|---|---|---|
| `publish_object` | Active segment create fails before visibility | Open returns publish/create error; no valid segment is assumed. This path only triggers when metadata lookup reports `NotFound`; pre-existing valid active segments do not publish. |
| `append_object` | Fails before bytes visible | Append returns backend append error; service state unchanged |
| `append_object` | Reports wrong offset | Append returns `UnexpectedAppendOffset` |
| `append_object` | Reports short write | Append returns `UnexpectedAppendLength` |
| `append_object` | Reports wrong metadata size | Append returns `UnexpectedObjectSize` |
| `object_metadata` | Fails before append | Append returns backend metadata error; no rotation occurs |
| `sync_object` | Fails in `always` append | Append returns sync error; durability not claimed |
| `sync_object` | Fails in `force_durable` | Dirty facts remain |
| `sync_object` | Fails in `close` | Close returns sync error; dirty facts remain |
| `list_prefix` | Fails during read | Read returns list error |
| `read_object` | Fails during read | Read returns backend read error |
| `delete_object` | Fails during pruning | Delete report records failed segment |

Partial visibility faults:

1. The generic `FaultingBackend` fails before delegation. It is not enough for
   partial-append coverage.
2. Keep or add a WAL-specific fake backend that can append a prefix of the
   bytes and then return failure or misleading metadata.
3. Assert that reopen treats the visible prefix as a latest partial tail only
   when it is at the latest segment tail.

### 8. Retention And Deletion Tests

Required cases:

1. Active segment is never deleted.
2. Segment containing any record above the covered-through commit watermark is
   not deleted.
3. Segment equal to the active segment is protected.
4. Covered old segments whose records are all at or below the covered-through
   commit watermark are deleted.
5. Delete failure for one old segment does not hide success/failure facts for
   other segments.
6. Deletion ignores non-WAL objects.
7. Deletion works when there are no covered segments.
8. Deletion report is deterministic and sorted.
9. Retention requires `DeleteObject` capability before listing or deleting.

Non-goal:

1. WAL service does not decide the retention proof. The caller supplies the
   covered-through commit watermark, and the service mechanically protects its
   own active segment id and any newer segment.

### 9. Reopen And Crash-Style Tests

M3E2 should use deterministic reopen tests, not process-kill tests.

Required reopen cases:

1. Reopen after clean `standard` close.
2. Reopen after append without explicit `force_durable`.
3. Reopen after `always` append.
4. Reopen after rotation.
5. Reopen after latest partial envelope.
6. Reopen after latest partial envelope, then attempt same-segment append:
   append must be refused until lifecycle repairs/truncates.
7. Reopen after latest partial envelope, then attempt append that would rotate:
   rotation must be refused until lifecycle repairs/truncates. The test must
   assert the candidate record would exceed the segment boundary after the valid
   prefix so this is not accidentally another same-segment append case.
8. Reopen after non-latest partial envelope: read fails strict.
9. Reopen after corrupt header: open/read fails strict.

Deferred crash cases:

1. Process death during append.
2. Process death after append before sync.
3. Process death between successful append and successful sync in `always`
   policy. This is OS- and filesystem-dependent because the appended bytes may
   or may not survive.
4. Process death during sync.
5. Process death during segment rotation.
6. Process death during WAL deletion.

Those belong to L7/L8 process-level crash testing once lifecycle recovery owns
repair, truncate, replay, and health reporting.

### 10. Fuzz Tests

Existing L3 fuzz targets already cover byte decoders. M3TC2 should add
service-level fuzz only if the service can be reached without exposing a
production API.

Targets:

1. Arbitrary WAL segment object bytes plus expected segment id/database id.
2. Arbitrary sequence of valid WAL records with random payloads and segment
   sizes.
3. Arbitrary trailing garbage after one valid record.
4. Arbitrary object-name lists for segment discovery.

Fuzz invariants:

1. No panic.
2. No allocation blowup.
3. Success returns records that decode through L3.
4. Partial-tail success is allowed only for latest segment tail.
5. Errors surface as some variant of `WalServiceError`; fuzz tests must not
   collapse them into a generic failure. Expected variants include
   `InvalidConfig`, `UnsupportedCapability`, `InvalidSegmentId`, `Layout`,
   `Backend`, `List`, `Publish`, `Format`, `DatabaseMismatch`,
   `RecordTooLarge`, `SegmentIdOverflow`, `UnexpectedAppendOffset`,
   `UnexpectedAppendLength`, and `UnexpectedObjectSize`.

Manual command:

```bash
cargo fuzz run service_wal_segment --manifest-path crates/storage-next/fuzz/Cargo.toml
```

Do not add this command to the default fast test suite.

### 11. Sidecar Tests

Sidecars are optional for M3E2. If implemented later, add:

1. Missing sidecar falls back to scanning segment bytes.
2. Corrupt sidecar falls back or reports recoverable sidecar error according to
   the service contract.
3. Sidecar min/max commit version matches rebuilt segment metadata.
4. Sidecar min/max timestamp matches rebuilt segment metadata.
5. Sidecar record count matches rebuilt segment metadata.
6. Sidecar never overrides authoritative WAL bytes.

If sidecars remain deferred, this section remains a checklist for M3E3/L8.

## Test Support Shape

Allowed private support:

1. WAL record builder with deterministic branch id and timestamp.
2. WAL model that tracks records, offsets, segment ids, and dirty counters.
3. Byte mutation support for named WAL boundaries.
4. Fake backend that can misreport append offset, length, and metadata.
5. Fake backend that can partially append bytes before returning a failure.
6. Fake backend that can fail sync and delete operations.

Forbidden support:

1. A public WAL test API that mirrors `WalService`.
2. Test support in production modules without `#[cfg(test)]`.
3. Test support names containing roadmap labels.
4. Test support that bypasses L3 encoders and then claims to prove normal append
   behavior.
5. Tests that inspect local filesystem paths directly when an object-name
   backend operation can prove the same fact.

## Execution Tiers

Fast default tier:

```bash
cargo test -p strata-storage-next --locked service::wal
cargo test -p strata-storage-next --locked
```

Feature/fault tier:

```bash
cargo test -p strata-storage-next --features testkit,fault-injection --locked
cargo test -p strata-storage-next --no-default-features --features testkit,fault-injection --locked
```

No-default tier:

```bash
cargo test -p strata-storage-next --no-default-features --locked
```

Quality tier:

```bash
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

Feature matrix:

```bash
cargo hack -p strata-storage-next --feature-powerset --depth 2 --locked check --all-targets
```

WASM compile guard:

```bash
cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked
```

## Implementation Slices

Recommended follow-up slices:

1. `M3TC2A`: Add WAL private test support and construction/capability tests.
2. `M3TC2B`: Add append, exact-boundary, rotation, and model/property tests.
3. `M3TC2C`: Add durability-policy sync-success and sync-failure tests.
4. `M3TC2D`: Add corruption matrix tests.
5. `M3TC2E`: Add backend fault-window and partial-visibility tests.
6. `M3TC2F`: Add retention, deletion, and deterministic reopen safety tests.
7. `M3TC2G`: Close read/watermark and review-discovered edge-case gaps.
8. `M3TB2`: Add service-level WAL fuzz target if a narrow testkit surface is
   justified.

Each slice should be reviewable independently. If a slice needs production
changes, the test should fail before the production fix is applied.

## Exit Gate

M3E2 WAL hardening is complete when:

1. Every test family above is implemented or explicitly deferred with a named
   owner milestone.
2. The service has property coverage for append/read/rotation sequences.
3. The corruption matrix distinguishes latest partial tails from corruption.
4. The fault-window suite proves append, sync, read, list, metadata, and delete
   failures surface as typed errors.
5. Dirty facts are proven correct across `standard`, `always`, `force_durable`,
   `close`, and sync failure.
6. Reopen tests cover clean state, rotation, latest partial tail, non-latest
   partial tail, and corrupt headers.
7. The full storage-next verification matrix passes without warnings.
