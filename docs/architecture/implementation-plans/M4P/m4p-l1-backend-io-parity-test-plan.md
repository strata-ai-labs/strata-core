# M4P-L1 Test Plan: Backend IO Parity

Status: implementation-aligned test plan

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l1-backend-io-parity-implementation-plan.md`

Parent test methodology:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-test-plan.md`

## Goal

Prove that backend IO owns one mode-defined `delete_object` contract,
durable-local conformance, and the filesystem boundary.

Tests should fail if M4P-L1:

1. lets cache mode silently claim delete durability;
2. reports clean durable cleanup when a localfs parent namespace sync failed;
3. loses the source backend error behind a delete failure;
4. treats delete outcomes as service-specific WAL, snapshot, or quarantine
   operation;
5. exposes filesystem paths, parent-directory sync, or `std::fs` calls above
   `backend/local_fs.rs`;
6. weakens existing publish, append, sync, writer-lock, or capability tests;
7. implements L4 cleanup policy, L8 health-debt mapping, or L2 object-family
   classification inside L1.

## Audit Finding References

Primary audit source:
`docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`

Relevant sections:

1. `L1. Backend IO`
2. `Audit Matrix / Durability, Manifest, WAL, And Recovery Mechanics`
3. `Final Parity Matrix And Architecture-Aligned Gap Plan`

Findings covered by this test plan:

1. `delete_object` lacks an L1 durability outcome;
2. durable-local conformance is split across local tests instead of a full L1
   suite;
3. CI does not enforce the L1 IO boundary.

## Coverage Boundary

In scope for M4P-L1:

1. delete vocabulary validation;
2. capability validation;
3. memory/cache non-durable delete outcomes;
4. localfs durable-local delete success, already-missing, and fault windows;
5. reusable durable-local backend conformance for the operations L1 owns:
   writer lock, publish, append, sync, and delete;
6. source guards for filesystem IO, including seeded guard-detection tests;
7. downstream L4/L8 follow-up requirements recorded as deferrals, not
   implemented behavior.

Out of scope for M4P-L1:

1. new cleanup policies;
2. checkpoint format changes;
3. WAL truncation algorithm changes;
4. snapshot retention policy changes;
5. object-durable mode;
6. distributed locks or leases;
7. service-facing cleanup helpers or adapters;
8. migration of WAL, snapshot, sidecar, or quarantine cleanup semantics beyond
   compiler-required consumption of the typed backend result;
9. lifecycle health-debt mapping for delete uncertainty;
10. L2 object-family classification or object-name constructors;
11. performance benchmarks.

## Old-Code Regression Sources

| Old source | Behavior to preserve | Storage-next assertion |
| --- | --- | --- |
| `crates/engine/src/database/open.rs` | Durable open uses one writer lock. | Durable conformance proves localfs writer lock exclusion and release. |
| `crates/storage/src/durability/wal/writer.rs` | WAL files are appended and synced through backend-owned durability. | Durable conformance proves append offsets, sync, and localfs capability claims. |
| `crates/storage/src/manifest.rs` | Durable publication reports visibility and parent-sync failure windows. | Existing publish fault tests remain intact while delete fault tests mirror the same classification style. |
| `crates/storage/src/durability/checkpoint_runtime.rs` | Destructive cleanup is followed by durability barriers where required. | Localfs `delete_object` reports durable success only after removal and namespace durability are proven. |
| `crates/storage/src/segmented/quarantine_protocol.rs` | Quarantine cleanup distinguishes source removal success and failure. | L1 exposes delete uncertainty; later L4/L8 tests preserve source-delete statuses. |

Tests must not port:

1. raw filesystem path assertions above localfs;
2. product open or close behavior;
3. follower-state cleanup;
4. engine error mappings;
5. benchmark-only helpers.

## Test Locations

Use:

1. `crates/storage-next/src/backend/delete.rs` for delete vocabulary unit tests if
   a new module is added.
2. `crates/storage-next/src/backend/conformance.rs` for basic backend
   conformance, cache-mode requirement validation, and durable-local conformance
   helpers.
3. `crates/storage-next/src/backend/local_fs.rs` for localfs-specific fault
   injection tests.
4. `crates/storage-next/src/backend/memory.rs` for cache delete outcome tests.
5. `crates/storage-next/tests/backend_io_boundary.rs` or an equivalent
    integration test for source guards.

Keep Rust test names behavior-focused. Do not use `M4P`, `L1`, or slice labels
inside Rust identifiers, comments, fixture bytes, panic messages, or user-facing
text.

## Direct Unit Tests

The lists below are behavior requirements. Rust test names should remain
behavior-focused and may group closely related assertions when that keeps the
suite easier to maintain.

### 1. Delete Vocabulary

Required behavior:

1. delete status names are stable;
2. delete failure-kind names are stable;
3. delete outcomes report object, status, and durability;
4. delete errors preserve object, failure kind, source error, and the standard
   error source chain;
5. successful outcomes can report durable-local success and cache/no-durability
   success;
6. `AlreadyMissing` is represented as a successful delete outcome;
7. `RemovedDurabilityUnconfirmed` is represented as a failure, not as a clean
   durable outcome;
8. production source contains no parallel durable-delete API such as
   `delete_object_durable`, `DeleteOptions`, or `DeleteMode`.

Assertions:

1. tests assert enum variants and stable names, not display strings;
2. `DeleteError::source()` returns the backend error;
3. `AlreadyMissing` cannot carry a deletion failure;
4. successful `DeleteOutcome` values can distinguish durable-local success from
   cache success with no durability claim;
5. unconfirmed durability is not represented as a clean durable outcome;
6. no test or source file introduces `delete_object_durable`, `DeleteOptions`,
   or `DeleteMode`.

### 2. Capability Validation

Required behavior:

1. `DeleteObject` has the stable `delete_object` capability name;
2. cache requirements include only the existing delete capability;
3. durable-local requirements use the same single delete capability;
4. memory advertises delete without durable publish, durable sync, append, or
   writer-lock capabilities;
5. localfs satisfies durable-local mode only on supported platforms;
6. object-durable candidate mode does not gain delete durability by default;
7. capability mismatch reporting does not introduce a parallel durable-delete
   capability.

Assertions:

1. durable-local mode validation continues to use the single `DeleteObject`
   capability, while durable-local conformance proves stronger localfs semantics;
2. unsupported object-durable mode remains rejected at public V1 boundaries;
3. memory/cache validation remains unchanged except for explicit non-durable
   delete outcome assertions.

### 3. Memory Backend

Required behavior:

1. memory delete removes an existing object and reports non-durable success;
2. memory delete of a missing object reports idempotent non-durable
   `AlreadyMissing`;
3. memory delete outcomes never claim durable cleanup;
4. memory cache-mode validation remains valid;
5. memory durable-local validation reports the durable capabilities it lacks.

Assertions:

1. cache delete removes the in-memory object;
2. cache delete reports no durability claim;
3. cache delete treats missing objects as an idempotent `AlreadyMissing`
   outcome;
4. durable-local validation still fails for memory because it lacks durable
   publish, sync, metadata, append, and writer-lock capabilities.

### 4. Localfs Delete

Required behavior:

1. localfs delete removes an existing object and reports durable success on
   supported durable-local platforms;
2. localfs delete of a missing object reports idempotent `AlreadyMissing`;
3. localfs delete rejects the reserved writer-lock object;
4. localfs before-removal faults leave the object visible;
5. localfs removal faults report removal uncertainty;
6. localfs parent-sync faults report unconfirmed durability and do not produce a
   clean outcome;
7. localfs successful delete survives reopening the backend;
8. localfs delete ignores stale publish temporary files;
9. localfs symlink and non-file protections fail closed;
10. non-Unix localfs builds do not satisfy durable-local conformance until
    directory-sync semantics are implemented there.

Assertions:

1. pre-removal failure keeps the object readable;
2. parent-sync failure does not produce a clean `DeleteOutcome`;
3. all filesystem operations stay inside localfs;
4. symlink and non-file protections still fail closed;
5. deleting a missing object is idempotent and reports the correct durability
   fact for the active backend.

### 5. Durable Backend Conformance

Required behavior:

1. memory satisfies basic object conformance and cache-mode requirements;
2. memory rejects durable-local conformance with explicit missing capabilities;
3. localfs satisfies basic object conformance and cache-mode requirements;
4. localfs satisfies durable-local conformance on supported platforms;
5. durable conformance covers append, sync, publish, writer lock, and delete;
6. writer-lock conformance uses two backend handles so a dummy lock guard cannot
   pass by only proving acquisition and release;
7. cache backends continue to report weaker delete durability.

Assertions:

1. conformance helpers are reusable functions, not one-off localfs tests;
2. durable conformance runs only on backends that advertise the required
   capabilities;
3. future backend adapters can call the same helpers without knowing localfs
   internals.

## Downstream Test Requirements

M4P-L1 records these tests for later L4/L8 slices. They are not M4P-L1 closeout
requirements.

L4 service cleanup follow-up:

1. WAL retention consumes `delete_object` outcomes for segments when cleanup
   claims durable removal.
2. Sidecar cleanup reports delete success, already-missing, and
   uncertainty without turning optional sidecar cleanup into authoritative WAL
   failure.
3. Snapshot pruning preserves retention selection while reporting delete
   uncertainty in prune reports.
4. Quarantine source delete and purge preserve existing quarantine statuses while
   consuming backend delete uncertainty.

L8 lifecycle follow-up:

1. checkpoint truncation maps L4 delete uncertainty into recovery health debt;
2. quarantine lifecycle maps L4 source-delete/purge uncertainty into lifecycle
   health facts;
3. lifecycle tests preserve source error chains and do not inspect localfs facts.

## Source Guard Tests

### 1. Filesystem Boundary Guard

Required behavior:

1. production storage code uses direct filesystem IO only in
   `backend/local_fs.rs`;
2. the guard allows the localfs backend implementation to own filesystem IO;
3. the guard ignores test fixtures, `_tests.rs` files, `_tests/` fixture
   directories, `test_support`, and testkit sources;
4. seeded guard checks reject `fs::remove_file` outside localfs;
5. seeded guard checks reject `OpenOptions::new` outside localfs;
6. seeded guard checks avoid false positives for storage API names such as
   `StorageOpenOptions` and module paths such as `local_fs::LocalFsBackend`.

Assertions:

1. guard scans production `crates/storage-next/src` files;
2. guard excludes `crates/storage-next/src/backend/local_fs.rs`;
3. guard does not scan plan documents;
4. guard is tested with seeded strings or helper fixtures, not by editing
   production files during the test.

### 2. Cleanup Outcome Guard

A cleanup outcome guard belongs to the later L4 service cleanup slice, not
M4P-L1. M4P-L1 may only record that follow-up.

## Fault And Crash Proof

M4P-L1 needs fault-window tests, not full crash/restart harness expansion.

Required localfs fault points:

1. before removal;
2. during removal or simulated ambiguous removal;
3. after removal before parent namespace sync;
4. cache delete outcome with no durability claim.

Required restart proof:

1. after successful localfs `delete_object`, reopen localfs and assert the
   object remains absent;
2. after parent-sync fault, reopen and assert the object state is classified by
   recovery/service tests rather than assumed clean;
3. writer lock remains releasable after delete fault tests.

Full crash orchestration belongs to later L4/L8 durable hardening slices if
service recovery behavior needs broader restart coverage.

## Mode Testing

Cache mode:

1. delete works;
2. delete outcome reports no durability claim;
3. cache open validation is unchanged;
4. L1 does not add service cleanup behavior in cache mode.

Durable-local mode:

1. localfs `delete_object` reports durable success only after namespace
   durability is proven;
2. localfs durable-local conformance includes delete behavior on supported
   platforms;
3. delete uncertainty is available for later L4/L8 service and lifecycle facts.

Wasm-none mode:

1. no localfs delete durability code is required;
2. durable-local mode remains unsupported where localfs is unavailable;
3. source guards do not require localfs-only files to compile on wasm-none.

## Generated And Fuzz Tests

No new fuzz target is required for M4P-L1 unless delete outcome serialization is
introduced. If a generated test is added, it should focus on backend-operation
scripts:

1. write;
2. publish;
3. append;
4. sync;
5. delete;
6. list;
7. reopen.

The independent model is an object map plus per-object visibility/durability
facts. The model must never use wall-clock timing or filesystem paths.

## Verification Commands

Run the narrow commands first:

1. `cargo test -p strata-storage-next backend::`

Then run package health:

1. `cargo test -p strata-storage-next`
2. `cargo clippy -p strata-storage-next --lib --features perf-trace -- -D warnings`

If source guards are implemented as integration tests, include:

1. `cargo test -p strata-storage-next --test backend_io_boundary`

## Closeout Requirements

M4P-L1 closes only when:

1. every L1 audit finding cited above is closed or explicitly deferred;
2. delete outcome tests pass;
3. memory/cache non-durable delete outcome tests pass;
4. localfs delete success and fault-window tests pass;
5. durable backend conformance includes append, sync, publish, writer lock, and
   delete;
6. downstream L4/L8 service cleanup tests are recorded as follow-up requirements;
7. source guard passes and has seeded-failure coverage;
8. no production file outside `backend/local_fs.rs` uses direct filesystem IO;
9. package tests and clippy pass for touched scopes;
10. deferred items list owner layer, reason, and follow-up slice.

## Deferral Rules

Allowed deferrals:

1. full L4/L8 crash harness expansion, if L1 fault-window tests already prove the
   backend outcome classes;
2. object-durable mode delete semantics;
3. cleanup outcome guard, because it belongs to the later L4 service cleanup
   slice.

Disallowed deferrals:

1. single-method delete outcome vocabulary;
2. localfs durable-local delete behavior on supported durable-local platforms;
3. memory/cache non-durable delete outcomes;
4. basic source guard against direct production filesystem IO above L1;
5. durable conformance coverage for localfs.
