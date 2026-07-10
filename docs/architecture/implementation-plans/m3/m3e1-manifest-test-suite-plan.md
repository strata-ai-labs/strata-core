# M3E1 / M3TE1 Test Suite Plan: Manifest Services

Status: test-suite plan

Parent plan: `docs/architecture/implementation-plans/m3-m3t-implementation-plan.md`

Implementation brief:
`docs/architecture/implementation-plans/m3e1-manifest-service-implementation-brief.md`

Implementation plan:
`docs/architecture/implementation-plans/m3te1-manifest-test-implementation-plan.md`

## Goal

Bring the M3E1 manifest services from minimum service coverage to
reference-grade recovery-pointer coverage.

The database manifest is the durable recovery pointer for local storage. It
records database identity, codec identity, active WAL segment, snapshot facts,
and flush watermark facts. The table manifest service is a durable publication
surface for branch-local table reachability bytes, but M3E1 deliberately keeps
those bytes payload-opaque.

M3TE1 proves that manifest services preserve old durable state on failed
publication, fail closed on corrupt current state, propagate publish
uncertainty precisely, reject invalid recovery facts before durable bytes are
written, and do not learn table or branch semantics.

## Testing Principles

1. Manifest tests model durable metadata mechanics, not product semantics.
2. The database manifest service owns recovery facts only: database id, codec
   id, active WAL segment, snapshot id, snapshot watermark, and flush
   watermark.
3. Every manifest update is a full replacement. No test should assume partial
   in-place mutation.
4. Missing database manifest is not corruption on optional load. Missing
   database manifest is an error on required load.
5. Corrupt database manifest bytes fail closed. The service must not repair,
   recreate, or infer state from WAL/table/snapshot objects.
6. Publish failure must preserve `PublishFailureKind` and role facts.
7. Table manifest bytes are opaque. Tests should verify opacity explicitly.
8. Test labels such as `M3TE1` belong in docs and tracker entries only. They
   must not appear in production file names, type names, comments, or error
   names.

## Scope

In scope:

1. `crates/storage-next/src/service/manifest.rs` database manifest service.
2. `crates/storage-next/src/service/manifest.rs` table manifest service.
3. Module-local tests in `crates/storage-next/src/service/manifest.rs`.
4. Fault-window checks that can be expressed with fake backends or the existing
   publisher fault hooks.
5. Format-level manifest corruption cases that affect service behavior.
6. Local filesystem durable manifest behavior.
7. Memory/cache backend rejection of durable manifest publication.

Out of scope:

1. L8 lifecycle recovery policy.
2. Process-kill crash tests.
3. Multi-writer manifest fencing.
4. Object-store conditional manifest publication.
5. Table manifest payload format.
6. Branch reachability, inherited-layer semantics, fork frontiers, or table
   algorithms.
7. Snapshot object publication and checkpoint runtime.

## Current Coverage

The current M3E1 tests already cover:

1. Database manifest optional load returns `Ok(None)` when missing.
2. Database manifest required load reports a typed missing error.
3. Database manifest create/read roundtrips V1 bytes on local filesystem.
4. Durable create refuses an existing manifest.
5. Durable publication on memory backend returns unsupported durable publish.
6. Active WAL segment update persists and preserves identity facts.
7. Active WAL segment update rejects zero.
8. Snapshot and flush watermark updates persist as full replacements.
9. Present-but-zero snapshot recovery facts are rejected before publishing.
10. Corrupt database manifest bytes return a typed decode error.
11. Codec mismatch returns a typed validation error.
12. Table manifest missing load returns `Ok(None)`.
13. Table manifest publish/read roundtrips opaque bytes.
14. Table manifest invalid branch id returns a layout error.
15. Database and table publish failures preserve `PublishFailureKind`.

Remaining coverage gaps:

1. No systematic state-machine test for sequences of recovery-fact updates.
2. No full publish-failure matrix by role and intent.
3. No proof that failed replacement leaves old bytes authoritative.
4. No corrupt-current tests for every update path.
5. No invalid-input matrix for codec id, snapshot facts, flush watermark, and
   branch id.
6. No explicit role/object precision checks for every error family.
7. No table-manifest opacity matrix over arbitrary bytes.
8. No property test for manifest update preservation.
9. No explicit cache-mode absence test for manifest object families.
10. No manifest service fuzz or malformed-input routing beyond L3 decode tests.

## Target Test Files

Primary:

1. `crates/storage-next/src/service/manifest.rs`

Optional private support:

1. `crates/storage-next/src/service/manifest/test_support.rs`

Optional integration/fuzz files:

1. `crates/storage-next/tests/service_fault_windows.rs`
2. `crates/storage-next/tests/cache_mode_absence.rs`
3. `crates/storage-next/fuzz/fuzz_targets/service_manifest_bytes.rs`

The default should remain module-local tests because the manifest services are
crate-private L4 services. Add testkit exposure only if integration or fuzz
coverage genuinely needs it.

## Test Families

### 1. Construction And Capability Tests

Required cases:

1. Database manifest service can be constructed over any backend.
2. Table manifest service can be constructed over any backend.
3. Optional database load on memory backend returns missing, not unsupported.
4. Required database load on memory backend returns typed missing.
5. Database manifest durable create on memory backend returns unsupported
   durable publish.
6. Database manifest durable replace on memory backend returns unsupported
   durable publish.
7. Table manifest durable create on memory backend returns unsupported durable
   publish.
8. Table manifest durable replace on memory backend returns unsupported durable
   publish.
9. Local filesystem backend publishes durable database manifest bytes.
10. Local filesystem backend publishes durable table manifest bytes.

Coverage note:

1. Case 10 is already covered by the M3E1 opaque table-manifest roundtrip test.
   Keep it as a regression requirement, not as a new test if the existing test
   already proves durable localfs publication.

Exit gate:

1. Unsupported durable behavior fails at publish time with a typed publish
   failure, never with fake durable success.

### 2. Database Manifest Load Tests

Required cases:

1. Missing object returns `Ok(None)` from optional load.
2. Missing object returns `Missing` from required load.
3. Existing valid bytes decode exactly once into a database manifest.
4. Existing corrupt bytes return `Decode`.
5. Existing future-version bytes return `Decode`.
6. Existing pre-V1 development-version bytes return `Decode`.
7. Existing checksum-mismatched bytes return `Decode`.
8. Existing bytes with partial snapshot facts return `Decode`.
9. Present zero snapshot facts are not representable in manifest bytes because
   zero is the absent sentinel. That defense is exercised through the
   `DatabaseManifest::with_recovery_facts` API path, not through service byte
   load tests.
10. Backend read failure other than `NotFound` returns `Read`.
11. The same backend read-failure distinction is required when the caller uses
    codec-validating load through `load_current_for_codec`.

Exit gate:

1. Load paths distinguish missing, backend failure, and corrupt bytes.

### 3. Database Manifest Create Tests

Required cases:

1. `create_initial` writes database id, codec id, and active WAL segment `1`.
2. `create_initial` rejects invalid codec id before publishing and returns
   `ManifestServiceError::Encode` with the database manifest object.
3. `create_initial` with existing manifest returns publish precondition failure.
4. Failed `create_initial` preserves existing bytes exactly.
5. Published bytes decode through the L3 manifest codec.
6. Returned `DatabaseManifestWrite` matches loaded current manifest.
7. Publish outcome reports durable local publication when using localfs.

Invalid codec cases:

1. Empty codec id.
2. Codec id over the format limit.
3. Codec id containing NUL.
4. Non-UTF8 is not applicable at the service API because codec id is a Rust
   string; L3 owns non-UTF8 byte rejection.
5. Every service-level invalid codec case must surface as `Encode`, not as
   `InvalidRecoveryFact`.

### 4. Database Manifest Replacement Tests

Raw publish path:

1. `publish_current(&DatabaseManifest)` is caller-owned raw publication.
2. It publishes exactly the manifest supplied by the caller.
3. It does not load current state first.
4. It does not preserve database id, codec id, or unrelated recovery facts.
5. It does not detect missing or corrupt current manifest before publishing.
6. Failed publication returns `Publish` with database role.

Current-state update paths:

1. Active WAL segment update.
2. Snapshot facts update.
3. Flush watermark update.

Required cases for each current-state update path:

1. Successful replacement updates only intended fields.
2. Successful replacement preserves database id.
3. Successful replacement preserves codec id.
4. Successful replacement preserves unrelated recovery facts.
5. Failed replacement preserves old bytes exactly when the publish failure kind
   means no replacement became visible.
6. Failed replacement returns `Publish` with database role.
7. Corrupt current manifest causes the update to fail closed before publishing.
8. Missing current manifest causes the update to fail with typed missing.

Coverage discipline:

1. The field-preservation cases may use representative spot checks once the
   state-machine property is in place. The property test owns exhaustive
   preservation coverage for update sequences.

State-machine property:

1. Start from a valid initial manifest.
2. Generate a sequence of 1 to 64 active-WAL, snapshot, flush, and rejected-zero
   updates.
3. Maintain a model of expected manifest facts.
4. After every successful update, load current and compare to the model.
5. Inject rejected zero facts and assert the model and stored bytes do not
   change.

The property test should use a hand-rolled `proptest` loop in the normal test
suite, not `proptest-state-machine`. Failing seeds should be persisted under
`crates/storage-next/proptest-regressions/manifest_state_machine.txt`.

### 5. Recovery Fact Validation Tests

Required invalid cases:

1. Active WAL segment `0` is rejected before encoding.
2. Snapshot id `0` is rejected before encoding.
3. Snapshot watermark `0` is rejected before encoding.
4. Flush watermark `0` is rejected before encoding.
5. Snapshot id present without snapshot watermark is rejected by L3.
6. Snapshot watermark present without snapshot id is rejected by L3.
7. Present zero snapshot id is rejected by the `with_recovery_facts` API path;
   it is not representable in encoded bytes because zero decodes as absent.
8. Present zero snapshot watermark is rejected by the `with_recovery_facts` API
   path; it is not representable in encoded bytes because zero decodes as
   absent.

Required valid cases:

1. Active WAL segment `1`.
2. Active WAL segment `u64::MAX`.
3. Snapshot id `1` with snapshot watermark `1`.
4. Snapshot id `u64::MAX` with snapshot watermark `u64::MAX`.
5. Flush watermark `1`.
6. Flush watermark `u64::MAX`.

Exit gate:

1. Invalid recovery facts never reach durable publication.

### 6. Codec Validation Tests

Required cases:

1. Matching codec id returns the manifest.
2. Mismatching codec id returns `CodecMismatch`.
3. `CodecMismatch` preserves expected codec id.
4. `CodecMismatch` preserves actual codec id.
5. `CodecMismatch` preserves object name.
6. Missing manifest under codec validation returns `Ok(None)`.
7. Corrupt manifest under codec validation returns `Decode`, not
   `CodecMismatch`.
8. Backend read failure other than `NotFound` under codec validation returns
   `Read`, not `CodecMismatch`.

### 7. Publish Failure Matrix

Required roles:

1. Database manifest.
2. Table manifest.

Required intents:

1. Durable create.
2. Durable replace.

Required publish failure kinds:

1. `Unsupported`.
2. `PreconditionFailed`.
3. `FailedBeforeVisibility`.
4. `VisibilityUnknown`.
5. `VisibleDurabilityUnconfirmed`.

Required assertions:

1. Service returns `ManifestServiceError::Publish`.
2. Error preserves manifest role.
3. Error preserves `PublishFailureKind`.
4. Error preserves backend source error.
5. For replace paths, previous bytes remain authoritative when failure kind
   implies no visible replacement.
6. For visibility-unknown paths, the service does not collapse uncertainty into
   a generic backend error.

Note:

1. Lower filesystem windows are already covered by M3TC1 durable-publisher
   tests. M3TE1 only proves manifest services preserve and propagate the
   publisher classification.
2. The full matrix is `2 roles x 2 intents x 5 failure kinds`, or 20 cells.
   Existing M3E1 tests cover only a small subset, so this section is mostly new
   work.

### 8. Table Manifest Opaque Payload Tests

Required cases:

1. Missing table manifest returns `Ok(None)`.
2. Empty payload roundtrips.
3. Arbitrary binary payload roundtrips.
4. Payload that looks like corrupt database manifest bytes roundtrips.
5. Payload that looks like corrupt WAL bytes roundtrips.
6. Payload with embedded NUL bytes roundtrips.
7. Large payload within backend limits roundtrips.
8. Invalid branch id returns layout error before backend access.
9. Publish create refuses existing table manifest.
10. Publish replace updates existing table manifest.
11. Publish replace may create a missing table manifest because the localfs
    replace publisher uses atomic rename to the final object path. The test name
    should make this explicit.

Invalid branch id matrix:

1. Empty branch id returns `Layout` with table role and
   `LayoutError::EmptyComponent`.
2. Branch id containing `/` returns `Layout` with table role and
   `LayoutError::ComponentContainsSeparator`.
3. Branch id containing NUL returns `Layout` with table role and
   `LayoutError::InvalidComponent`.
4. Branch id containing path-invalid bytes such as space, colon, or non-ASCII
   returns `Layout` with table role and `LayoutError::InvalidComponent`.
5. Branch id that is too long as a component returns `Layout` with table role
   and `LayoutError::InvalidComponent`.
6. Branch id that is valid as a component but makes the assembled
   `tables/<branch_id>/manifest` object name too long returns `Layout` with
   table role and `LayoutError::InvalidObjectName`.
7. Ambiguous branch id `.` or `..` returns `Layout` with table role and
   `LayoutError::InvalidComponent`.

Exit gate:

1. Table manifest tests prove payload opacity and layout-owned object naming.

### 9. Role And Object Precision Tests

Required cases:

1. Table layout errors carry table role.
2. Database read errors carry database role and manifest object.
3. Table read errors carry table role and table manifest object.
4. Database publish errors carry database role.
5. Table publish errors carry table role.
6. Database decode errors carry database manifest object.
7. Database encode errors carry database manifest object.
8. Codec mismatch carries database manifest object.
9. Invalid recovery fact carries database role, object, and field name.

Database layout note:

1. Database manifest layout errors are structurally unreachable in M3E1 because
   the database manifest object name is fixed as `manifest/current` by
   `ObjectLayout::database_manifest()`. If that object name ever becomes
   configurable, database layout failures must carry database role. Until then,
   fixed database layout validity is owned by the layout tests, while the
   manifest service keeps the `Layout { role: Database, ... }` branch as a
   defensive mapping.

Encode asymmetry:

1. `ManifestServiceError::Encode` intentionally has no role field because only
   database manifests are encoded by M3E1. Table manifest payloads are opaque
   bytes and are never encoded by this service.

Exit gate:

1. Higher layers can map errors without parsing strings.

### 10. Cache-Mode Absence Tests

M3E1 service tests may use memory backend for unsupported durable publication,
but cache lifecycle must not create durable manifest object families.

Required cases for M3TD1 or earlier:

1. Cache-mode open path creates no `manifest/current`.
2. Cache-mode open path creates no branch table manifest objects.
3. Cache-mode lifecycle does not instantiate database manifest service as
   durable state.

If cache lifecycle is not implemented yet, record these as M3TD1 obligations.

### 11. Fuzz And Malformed Input Tests

Existing L3 fuzz targets already cover manifest bytes. M3TE1 does not require
service-level fuzz unless a narrow testkit route is needed later.

Optional service fuzz target:

1. Arbitrary database manifest object bytes through `load_current`.
2. Arbitrary expected codec id through `load_current_for_codec`.
3. Arbitrary table manifest payload through publish/load using a memory-like
   fake durable backend.

Fuzz invariants:

1. No panic.
2. No allocation blowup.
3. Success returns bytes/facts that decode through L3.
4. Missing, corrupt, and backend errors remain distinguishable.
5. Table manifest payload is never decoded as database manifest.

Manual command, if added:

```bash
cargo fuzz run service_manifest_bytes --manifest-path crates/storage-next/fuzz/Cargo.toml
```

Do not add this command to the default fast test suite.

## Test Support Shape

Allowed private support:

1. Manifest model that tracks database id, codec id, active WAL segment,
   snapshot facts, and flush watermark.
2. Fake backend that can publish successfully but record exact object bytes.
3. Fake backend that can return each `PublishFailureKind`.
4. Fake backend that can fail reads with a selected backend error kind.
5. Byte mutation support for named manifest fields.
6. Table payload generators for arbitrary byte vectors.

Forbidden support:

1. A public manifest test API that mirrors the manifest services.
2. Test support in production modules without `#[cfg(test)]`.
3. Test support names containing roadmap labels.
4. Tests that decode table manifest payload bytes.
5. Tests that inspect local filesystem paths directly when object-name backend
   operations can prove the same fact.

## Execution Tiers

Fast default tier:

```bash
cargo test -p strata-storage-next --locked service::manifest
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

## Implementation Slices

Recommended follow-up slices:

1. `M3TE1A`: Add manifest private test support and load/capability tests.
2. `M3TE1B`: Add database manifest create/replace state-machine tests.
3. `M3TE1C`: Add recovery-fact validation and codec validation tests.
4. `M3TE1D`: Add publish-failure matrix tests for database and table manifests.
5. `M3TE1E`: Add table manifest opacity and role/object precision tests.
6. `M3TD1`: Add cache-mode absence tests when cache lifecycle exists.
7. Optional `M3TB3`: Add service-level manifest fuzz target only if M3TE1 finds
   a service-layer fuzz seam that L3 fuzzing does not cover.

Each slice should be reviewable independently. If a slice needs production
changes, the test should fail before the production fix is applied.

## Exit Gate

M3TE1 is complete when:

1. Every test family above is implemented or explicitly deferred with a named
   owner milestone.
2. Database manifest state-machine tests prove unrelated facts are preserved
   through every update.
3. Failed replacement tests prove old manifest bytes remain authoritative when
   publication fails before visibility.
4. Publish uncertainty is preserved for database and table manifests.
5. Corrupt current manifest tests prove all update paths fail closed.
6. Table manifest opacity tests prove payload bytes are never decoded by M3E1.
7. Invalid recovery facts are rejected before durable publication.
8. The full storage-next verification matrix passes without warnings.
