# M3E1 Implementation Brief: Manifest Services

Status: implementation brief

Parent plan: `docs/architecture/implementation-plans/m3-m3t-implementation-plan.md`

## Goal

Implement the first L4 manifest services over the storage-next backend, layout,
format, and durable publisher layers.

M3E1 should produce two service surfaces:

1. Database manifest service for `manifest/current`.
2. Payload-opaque table manifest publication service for
   `tables/<branch-id>/manifest`.

The database manifest service owns stable physical recovery facts. The table
manifest service owns durable publication mechanics only. Branch reachability,
inherited-layer meaning, fork frontiers, table algorithms, and manifest payload
format remain M4/M5/M6 work.

## Inputs Read

Architecture and plan inputs:

1. `docs/architecture/storage/l2-object-layout.md`
2. `docs/architecture/storage/l3-durable-format-codec.md`
3. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
4. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
5. `docs/spec/strata-storage-format-v1.md`
6. `docs/architecture/implementation-plans/m3-m3t-implementation-plan.md`
7. `docs/architecture/implementation-plans/m3-porting-log.md`

Current implementation evidence:

1. `crates/storage/src/durability/format/manifest.rs`
2. `crates/storage/src/durability/format/watermark.rs`
3. `crates/storage/src/durability/checkpoint_runtime.rs`
4. `crates/storage/src/durability/recovery_bootstrap.rs`
5. `crates/storage/src/manifest.rs`
6. `crates/storage/src/segmented/mod.rs`
7. `crates/storage/src/segmented/tests/publish_failures.rs`
8. `crates/storage/src/test_hooks.rs`
9. `crates/storage-next/src/layout/mod.rs`
10. `crates/storage-next/src/format/manifest.rs`
11. `crates/storage-next/src/format/watermark.rs`
12. `crates/storage-next/src/service/publish.rs`
13. `crates/storage-next/src/backend/publish.rs`
14. `crates/storage-next/tests/service_fault_windows.rs`

## Existing Behavior To Preserve

1. Database MANIFEST is physical storage metadata: database id, codec id,
   active WAL segment, snapshot watermark, snapshot id, and flushed-through
   commit id.
2. Fresh durable databases start with active WAL segment `1`.
3. Manifest creation and replacement use write-temp, sync-temp, publish, and
   parent-directory sync through the storage-next publisher.
4. Missing database manifest is distinguishable from corrupt database manifest.
   New-database open can create it; existing-database recovery must fail closed
   on corrupt bytes.
5. Updating active WAL segment, snapshot facts, or flush watermark is a full
   manifest replace, not an in-place partial write.
6. Parent-directory sync failure after publish remains visible but durability
   unconfirmed; it must not be collapsed into a generic IO failure.
7. Branch/table manifest publication uses the same durable-publish mechanics as
   database MANIFEST publication.

## Intentional V1 Changes

1. V1 database manifest bytes use storage-next format version `1`; pre-V1
   development manifest version `2` is rejected by the normal V1 decoder.
2. The service must not use raw `std::fs`, local paths, temp-file names, or
   ad hoc string object names. It must consume `ObjectLayout`,
   `ObjectPublisher`, and backend traits.
3. The old manifest-management name and shape are not ported. Use a small
   domain service under `crates/storage-next/src/service/manifest.rs`.
4. The old `segments.manifest` payload format is not ported in M3E1. The table
   manifest service reads and publishes raw bytes at the layout-owned object
   name and leaves payload meaning to later layers.
5. Follower-mode manifest behavior is not ported.
6. Cache mode does not create or persist database MANIFEST or table manifest
   objects. M3E1 may use memory backend tests for service mechanics only, but
   cache lifecycle must not wire these services in as durable state.

## Target Files

Implementation files:

1. `crates/storage-next/src/service/manifest.rs`
2. `crates/storage-next/src/service/mod.rs`
3. `crates/storage-next/src/format/manifest.rs`, only if small accessor or
   mutation support functions are needed.
4. `crates/storage-next/src/backend/publish.rs`, only if M3E1 exposes a
   missing publish fact needed by manifest error handling.

Test files:

1. Module-local tests in `crates/storage-next/src/service/manifest.rs`.
2. `crates/storage-next/tests/service_fault_windows.rs`, if a service-level
   publish fault needs integration coverage.
3. `crates/storage-next/tests/README.md`, only if new manual commands or
   feature requirements are introduced.

Documentation files:

1. `docs/architecture/implementation-plans/m3-porting-log.md` must receive an
   M3E1 source-map note before production code changes.
2. `docs/architecture/v1-progress-tracker.md` should be updated only after
   M3E1 implementation and verification are complete.

## Service Shape

Exact Rust names can adjust during implementation, but the shape should stay
small and repeatable.

Database manifest service:

1. Constructed from `&dyn Backend`.
2. Uses `ObjectLayout::database_manifest()` for the object name.
3. `load_current` returns `Ok(None)` when the object is absent.
4. `load_required` may return a typed missing-manifest error for recovery paths
   that require an existing durable database.
5. `create_initial` builds `DatabaseManifest::new(database_id, codec_id)` and
   publishes with durable create.
6. `publish_current` encodes and publishes with durable replace.
7. Update paths should produce full manifest replacements for:
   active WAL segment, snapshot facts, and flushed-through commit id.
8. Active WAL segment update should accept `NonZeroU64` or reject zero before
   encoding, so invalid recovery facts never reach durable storage.
9. Codec validation should be explicit: loading a manifest with a different
   codec id should return a typed service error, not a string comparison at a
   higher call site.

Table manifest service:

1. Constructed from `&dyn Backend`.
2. Uses `ObjectLayout::branch_table_manifest(branch_id)` for the object name.
3. Reads and publishes raw payload bytes.
4. Returns `Ok(None)` when the manifest is absent.
5. Does not decode table manifest payload bytes.
6. Does not mention inherited layers, fork versions, materialization state, or
   branch visibility.

Shared service errors should preserve:

1. Object role: database manifest or table manifest.
2. Object name.
3. Backend source error for reads and metadata operations.
4. Format source error for database manifest decode/encode.
5. Publish source error and `PublishFailureKind`.
6. Codec mismatch facts when validation is requested.

## Fault And Recovery Rules

1. Missing database manifest on `load_current` is not corruption.
2. Decode failure for an existing database manifest is manifest corruption.
3. Durable create precondition failure must preserve the existing manifest.
4. Durable replace failure before visibility must preserve the previous
   manifest as authoritative.
5. `VisibilityUnknown` and `VisibleDurabilityUnconfirmed` must propagate as
   manifest publish uncertainty so engine-next can map them to
   `ambiguous_commit.manifest_publish` when appropriate.
6. A published manifest must be decodable before it is returned as current
   service state.
7. The service must not silently fall back to listing WAL, table, or snapshot
   objects when the database manifest is corrupt. That recovery policy belongs
   to L8.

## Test Plan

This section defines the minimum M3E1 implementation tests. The comprehensive
follow-up suite is defined in
`docs/architecture/implementation-plans/m3e1-manifest-test-suite-plan.md` and
should land as `M3TE1` before later layers treat manifest behavior as
reference-grade recovery-pointer infrastructure.

Minimum M3E1 tests:

1. Database manifest missing object returns `Ok(None)`.
2. Database manifest create/read roundtrips V1 bytes through the service.
3. Durable create refuses an existing manifest and preserves old bytes.
4. Durable replace updates active WAL segment while preserving database id and
   codec id.
5. Active WAL segment update rejects zero without publishing.
6. Snapshot facts and flush watermark updates persist as full manifest
   replacements.
7. Corrupt database manifest bytes return a typed decode error.
8. Codec mismatch returns a typed validation error.
9. Durable manifest publication on memory/cache backend returns unsupported
   durable publish, not a fake durable success.
10. Table manifest read missing returns `Ok(None)`.
11. Table manifest publish/read roundtrips opaque bytes without interpreting
    them.
12. Table manifest durable create or replace failure preserves
    `PublishFailureKind`.
13. Service tests assert object names come from `ObjectLayout`, not hard-coded
    string construction.

Fault-window coverage:

1. M3E1 should include service-level tests that propagate lower
   `PublishFailureKind` values unchanged.
2. Existing M3TC1 local filesystem tests already cover temp-create,
   temp-write, temp-sync, final-publish, and parent-sync windows for the
   publisher. M3E1 should not add manifest-specific filesystem hooks.
3. If later tests need killed-process crash coverage around manifest update,
   that belongs in a later M3TC/M4 recovery slice, not in this brief.

## Implementation Order

1. Append an `M3E1` source-map note to `m3-porting-log.md`.
2. Add `service::manifest` with database manifest service, table manifest
   service, and service-local error types.
3. Wire module exports through `service/mod.rs`.
4. Add module-local tests for success, missing, corruption, codec mismatch,
   durable unsupported, and publish failure propagation.
5. Add or extend integration tests only for behavior that cannot be expressed
   in module-local tests.
6. Run M3 verification commands and update the progress tracker after the
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

1. No WAL append/read service. That is M3E2.
2. No WAL segment metadata sidecar publication. That is M3E2/M3E3.
3. No snapshot object publication or checkpoint runtime. That is M3E3.
4. No quarantine service or quarantine manifest format. That is M3E4.
5. No L5 table runtime, table block format, or table manifest payload format.
6. No branch visibility, inherited-layer semantics, or fork-frontier logic.
7. No L7 commit runtime or commit timeline.
8. No engine-facing L9 API.
9. No object-store fencing or multi-writer manifest generation history.
