# M3TD1 Test Plan: Cache-Mode Durable Absence

## Purpose

M3TD1 proves that cache mode remains explicitly non-durable after the M3 durable
object families exist. The goal is not to show that cache mode compiles, or that
the memory backend has fewer capabilities. The goal is to exercise the real
storage-next service surfaces and prove they do not create durable recovery
state, do not leave durable publish artifacts, and do not claim crash recovery.

Cache mode may write caller-requested cache objects and may report non-durable
publication facts. It must not create or persist database manifests, table
manifests, WAL segments, WAL sidecars, snapshots, checkpoint recovery facts,
quarantine inventories, quarantine object copies, lock objects, or durable
publish temporary objects.

## Source Contracts

- `m3-m3t-implementation-plan.md`: M3TD proves cache mode creates no durable
  WAL, manifest, snapshot, checkpoint, table, quarantine, or lock objects.
- `storage-next/l4-log-manifest-snapshot-services.md`: browser/cache mode must
  not claim crash durability, and L4 durable object families are absent in cache
  mode.
- `storage-next/l7-commit-runtime.md`: cache mode is WAL-free and has no crash
  durability claim.
- `storage-next/l8-lifecycle-recovery-maintenance.md`: cache-mode open/close
  does not make durable recovery claims.
- `storage-next/l9-storage-api-boundary.md`: cache mode may run without durable
  sync and must not claim crash recovery without a durable browser substrate.
- M3E1, M3E2, M3E3, and M3E4 test plans: cache-mode absence cases deferred from
  the durable service suites land here.

## Non-Goals

- Do not implement a durable browser substrate.
- Do not add compatibility shims for old storage formats.
- Do not retest local filesystem durability except where a narrow contrast test
  helps prove cache mode is taking a different path.
- Do not move L6 reachability policy or L8 lifecycle orchestration into L4.
- Do not mark lifecycle-level cache coverage complete if the corresponding
  storage-next lifecycle entry point does not exist yet. Use direct L4 service
  tests plus an explicit deferral record for the missing lifecycle path.

## Durable Families That Must Stay Absent

The test harness must scan the backend before and after each scenario and reject
newly visible objects in these families:

- Database manifest: `manifest/current`.
- Table manifests: branch table manifest object names.
- WAL segments: WAL segment object names.
- WAL sidecars: `meta/wal/<segment-id>`.
- Snapshots: snapshot container objects.
- Checkpoint recovery facts: manifest snapshot facts and active-WAL facts.
- Quarantine: branch inventories and quarantined object copies.
- Locks: writer locks, manifest locks, and any lock-family objects.
- Durable publish artifacts: temporary files, parent-sync sentinels, or any
  backend-specific durable publish staging object.

Use typed layout and object-family helpers when available. Exact string checks
are acceptable only for fixed public names such as `manifest/current` and
documented metadata prefixes.

## Harness Shape

Create a cache-mode absence harness with three jobs:

1. Wrap the cache backend and record every operation: read, range read, list,
   metadata, write, append, sync, publish, and delete.
2. Expose visible object names and bytes before and after every scenario.
3. Provide assertions that fail on any durable object family, any durable publish
   call, any durable sync/append call, or any crash-recovery fact returned from a
   cache path.

The primary backend should be the storage-next memory backend because it is the
cache-mode backend. Add a small observing wrapper rather than changing service
code just for testing. For direct durable-service tests, the wrapper should fail
durable operations with the production unsupported-capability shape and assert
that no mutation happened before the error.

Most scenarios should start from an empty backend. Tests that deliberately seed
stale durable debris must compare against the pre-scenario baseline and assert
that cache mode neither interprets the debris as recovery state nor creates more
durable objects.

## Required Cases

### 1. Capability And Mode Preflight

1. Cache-mode configuration does not require durable publish, durable sync,
   object metadata, or append capability.
2. Constructing durable services directly against a cache backend fails before
   mutation when the required durable capability is absent.
3. Capability errors carry the missing capability precisely enough for targeted
   diagnostics.
4. Preflight failure does not call publish, sync, append, write, or delete.
5. A non-durable publisher path may write only the caller-requested cache object
   and must report non-durable facts.

### 2. Manifest And Table Absence

1. Cache open, if available, creates no `manifest/current`.
2. Cache open, if available, creates no branch table manifest objects.
3. Cache lifecycle, if available, does not instantiate the database manifest
   service as a durable recovery source.
4. Direct database-manifest create/replace on the cache backend returns the
   production unsupported-capability error before a manifest object is visible.
5. Direct table-manifest publish on the cache backend returns the production
   unsupported-capability error before a table manifest object is visible.
6. Failed direct manifest/table operations leave no durable publish temporary
   objects.

### 3. WAL And Sidecar Absence

1. Cache open/commit, if available, does not create WAL segment objects.
2. Cache open/commit, if available, does not call append or durable sync.
3. Direct WAL service open on the cache backend returns unsupported capability
   before creating a segment.
4. Direct WAL append cannot create or extend a segment under cache mode.
5. Direct active-WAL sidecar publish on the cache backend returns unsupported
   before creating `meta/wal/<segment-id>`.
6. Sidecar load paths may read/list only when explicitly invoked; cache
   lifecycle must not use sidecars as durable recovery facts.

### 4. Snapshot And Checkpoint Absence

1. Cache checkpoint/close, if available, creates no snapshot container objects.
2. Cache checkpoint/close, if available, creates no durable manifest snapshot
   recovery facts.
3. Direct snapshot publish on the cache backend returns unsupported before a
   snapshot object is visible.
4. Direct checkpoint publish on the cache backend fails at the first durable
   manifest update and therefore persists neither active-WAL facts nor snapshot
   facts.
5. The checkpoint-internal snapshot-publication failure branch is N/A for a real
   cache backend: reaching it requires a backend that can durably publish the
   active-WAL manifest update but cannot durably publish the snapshot. That
   sequencing remains covered by M3TC3 durable checkpoint fault tests, not by
   cache-mode absence tests.
6. Failed direct snapshot/checkpoint operations leave no durable publish
   temporary objects.

### 5. Quarantine Absence

1. Cache lifecycle and maintenance paths, if available, create no quarantine
   inventory objects.
2. Cache lifecycle and maintenance paths, if available, create no quarantine
   object copies.
3. Direct quarantine-object mutation on the cache backend returns unsupported
   before inventory or copy publication.
4. Direct purge on the cache backend returns unsupported before deleting any
   source or quarantine object.
5. Reconciliation is read-only: it may list/read existing durable families only
   when explicitly invoked, and it must not publish, write, sync, append, or
   delete under cache mode.
6. Quarantine absence checks must reject both branch inventory objects and
   unlisted quarantine object copies.

### 6. Locks And Durable Publish Artifacts

1. Cache open/close, if available, creates no writer lock objects.
2. Cache open/close, if available, creates no manifest lock objects.
3. Cache-mode service paths leave no durable publish temp objects after success.
4. Cache-mode service paths leave no durable publish temp objects after failure.
5. A backend seeded with stale durable publish temp objects must not have those
   objects interpreted as valid cache recovery state.

### 7. Lifecycle Deferral Discipline

If a real cache open/commit/checkpoint lifecycle entry point is not present in
storage-next, do not fake lifecycle coverage by only constructing services. Land
the direct L4 service tests and record each missing lifecycle assertion in the
progress tracker as a named M3TD1 deferral. The deferral must name the future
entry point and the durable families it must prove absent.

## Sensitivity Probes

Every M3TD1 closeout must include at least one red-first probe that demonstrates
the absence assertions are sensitive:

- Temporarily route cache open through durable manifest creation and verify the
  manifest absence test fails.
- Temporarily route cache commit through WAL append and verify the WAL absence
  test fails.
- Temporarily allow durable publish on the memory backend and verify the direct
  durable-service tests fail on visible durable objects.
- Temporarily remove the object-family scan and verify a seeded durable object
  can pass until the scan is restored.

The probe must be reverted before closeout, and the failing test name must be
recorded in the progress tracker.

## Suggested Slices

- `M3TD1A`: Harness, object-family absence scanner, capability/mode preflight,
  and direct publisher non-durable fact checks.
- `M3TD1B`: Manifest, table manifest, WAL, and WAL sidecar absence.
- `M3TD1C`: Snapshot, checkpoint, quarantine, lock, and durable publish artifact
  absence.
- `M3TD1D`: Lifecycle-level coverage if the cache lifecycle entry points exist;
  otherwise explicit named deferrals and progress-tracker closeout.

Slices may be merged if the implementation is small, but the closeout record
must still list which durable families were covered and which lifecycle cases
were deferred.

## Verification Commands

Use the narrowest command while developing, then close with the broad commands:

```sh
cargo test -p strata-storage-next --locked cache_mode_absence
cargo test -p strata-storage-next --features testkit --locked cache_mode_absence
cargo test -p strata-storage-next --no-default-features --locked cache_mode_absence
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked
cargo doc -p strata-storage-next --no-deps --locked
```

If the final test names do not share `cache_mode_absence`, list the exact narrow
commands in the closeout record.

## Exit Gate

M3TD1 is complete only when:

1. Every durable family listed above has an executable absence assertion or a
   named lifecycle deferral.
2. Direct durable service calls on the cache backend either report non-durable
   facts for a caller-requested cache object or fail before mutation with the
   production unsupported-capability shape.
3. No cache path returns a crash-durable recovery fact.
4. No cache path leaves durable publish temp artifacts.
5. At least one red-first sensitivity probe is recorded and reverted.
6. The progress tracker records the narrow command, broad command, sensitivity
   probe, observed failure, revert proof, and any lifecycle deferrals.
