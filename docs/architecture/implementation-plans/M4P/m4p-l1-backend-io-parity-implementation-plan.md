# M4P-L1 Implementation Plan: Backend IO Parity

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4P/m4p-l1-backend-io-parity-test-plan.md`

## Objective

Close the L1 backend IO audit gaps without changing the storage-next L1-L9
architecture.

M4P-L1 restores the missing old-storage durability mechanics that belong at the
backend boundary:

1. tighten the existing `delete_object` contract so cleanup can distinguish
   durable removal from removal with unconfirmed namespace durability;
2. keep local filesystem details inside `backend/local_fs.rs`;
3. turn the existing backend tests into reusable durable conformance;
4. add a source guard that prevents production storage-next code above L1 from
   reintroducing direct filesystem IO.

The first executable slice is `M4P-L1A`: IO boundary guard and delete-contract
decision scope. This document covers the full L1 package so later L4 cleanup
slices can consume the L1 contract without reopening the backend decision.

## Audit Finding References

Primary audit source:
`docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`

Relevant sections:

1. `L1. Backend IO`
2. `Audit Matrix / Durability, Manifest, WAL, And Recovery Mechanics`
3. `Final Parity Matrix And Architecture-Aligned Gap Plan`

Findings closed by this plan:

1. `delete_object` lacks an L1 durability outcome;
2. durable-local conformance is split across local tests instead of a full L1
   suite;
3. CI does not enforce the L1 IO boundary.

Supporting architecture:

1. `docs/architecture/storage/l1-backend-io.md`
2. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
3. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`

The serving-path proof plan is not a direct performance input for L1. L1 is not
the point-read or scan bottleneck. Its role in M4P is correctness hardening and
future-proofing durable cleanup before larger-scale maintenance tests depend on
safe object deletion.

## Predecessors

Required before implementation:

1. parent M4P program plan;
2. M4P test methodology;
3. L1 audit findings listed above.

No lower-layer implementation predecessor exists because L1 is the bottom IO
layer. L2 object names are already available and must remain the only naming
surface consumed by L1.

## Layer Ownership Check

M4P-L1 owns backend operations over already-validated `ObjectName` values. It
must not grow responsibilities from adjacent layers:

1. L2 owns object families, object-name constructors, prefixes, and reserved
   names. L1 may consume an `ObjectName`; it must not classify WAL, snapshot,
   table, sidecar, or quarantine families.
2. L3 owns durable byte formats. L1 moves opaque bytes only.
3. L4 owns service semantics for WAL, manifests, snapshots, sidecars, table
   manifests, and quarantine inventories. L1 exposes backend outcomes only.
4. L8 owns cleanup policy, recovery health, retention orchestration, and lifecycle
   interpretation. L1 must not map delete uncertainty into lifecycle status.

## Existing-Code Source Map

| Current file | Evidence | L1 action |
| --- | --- | --- |
| `crates/storage-next/src/backend/mod.rs` | Defines `Backend`, capabilities, basic delete, append, sync, publish, writer guard, and unsupported defaults. | Tighten the existing `delete_object` contract and add delete outcome vocabulary without introducing a parallel delete method. |
| `crates/storage-next/src/backend/publish.rs` | Existing publish outcome/failure shape already classifies visibility and durability windows. | Mirror this style for delete outcome/failure instead of adding service-specific delete facts. |
| `crates/storage-next/src/backend/memory.rs` | Cache backend supports delete but advertises no durable sync/publish. | Keep cache delete behavior, but return a delete outcome that makes the lack of durability explicit. |
| `crates/storage-next/src/backend/local_fs.rs` | Owns path mapping, symlink rejection, append, object sync, writer lock, durable publish, and parent sync. | Make `delete_object` perform remove plus parent sync when localfs is used as the durable-local backend, with delete fault classification. |
| `crates/storage-next/src/backend/conformance.rs` | Shared basic backend conformance exists. | Split into basic object conformance, cache-mode requirement validation, and durable-local conformance. |

## Downstream Consumer Map

These files are important consumers of the L1 contract, but they are not M4P-L1
implementation targets. L4 and L8 own service semantics, cleanup policy, and
lifecycle health effects.

| Downstream file | Evidence | Owning follow-up |
| --- | --- | --- |
| `crates/storage-next/src/service/sidecar.rs` | Deletes optional WAL metadata sidecars through `delete_object`. | L4 service cleanup slice maps delete outcomes into sidecar reports. |
| `crates/storage-next/src/service/snapshot/listing.rs` | Snapshot prune deletes objects through `delete_object`. | L4 snapshot service slice decides how delete outcomes affect prune reports. |
| `crates/storage-next/src/service/wal.rs` | WAL retention deletes segment and sidecar objects. | L4 WAL service slice decides truncation/report semantics after typed delete outcomes exist. |
| `crates/storage-next/src/service/quarantine/mutation.rs` | Quarantine copy/purge deletes source and quarantined objects. | L4 quarantine service slice maps backend delete uncertainty into quarantine reports. |
| `crates/storage-next/src/lifecycle/checkpoint.rs` | Checkpoint truncation consumes WAL delete reports. | L8 checkpoint/lifecycle slice maps L4 cleanup uncertainty into health debt. |
| `crates/storage-next/src/lifecycle/quarantine.rs` | Lifecycle wraps quarantine source-delete and purge outcomes. | L8 quarantine/lifecycle slice preserves lifecycle status semantics while consuming L4 reports. |

## Old-Code Porting Map

The old architecture is behavioral evidence, not an API template.

| Old source | Behavior to preserve | Storage-next decision | Test focus |
| --- | --- | --- | --- |
| `crates/engine/src/database/open.rs` | Durable databases create the root and enforce a single-writer `.lock` file with `fs2`. | Already moved to L1 writer guard; keep it there and include it in durable conformance. | Two localfs backends cannot hold the writer guard at the same time; memory does not advertise the guard. |
| `crates/storage/src/durability/layout.rs` | Old storage was filesystem-layout aware. | Do not port path layout upward. L2 owns object names; localfs maps object names to paths privately. | Source guard rejects direct filesystem IO outside `backend/local_fs.rs`. |
| `crates/storage/src/durability/wal/writer.rs` | WAL segment creation/rotation synced files and parent directories. | Publish/append/sync already live in L1; durable conformance should prove the operations together. | Append facts, object sync, publish outcome, and writer guard are covered by one durable suite. |
| `crates/storage/src/manifest.rs` | Manifest publication used temp write, file sync, rename, and parent sync. | Existing `publish_object` remains the durable publish primitive; do not expose POSIX steps. | Publish fault classes remain stable while delete classes are added. |
| `crates/storage/src/durability/checkpoint_runtime.rs` | Snapshot pruning and WAL cleanup followed destructive operations with durability barriers. | Tighten `delete_object` as the backend-owned primitive. | Delete success, already-missing, before-removal failure, removal uncertainty, and parent-sync uncertainty are classified. |
| `crates/storage/src/segmented/quarantine_protocol.rs` | Quarantine movement and cleanup tracked whether source removal succeeded. | Keep quarantine semantics in L4/L8, but make source-removal durability visible through L1 delete outcomes. | Later L4/L8 tests must not claim clean quarantine/purge when delete durability is unconfirmed. |

Do not port:

1. old `DatabaseLayout` path APIs into storage-next;
2. filesystem paths, file descriptors, `fsync`, rename, or directory traversal
   into L4-L9;
3. follower-state layout or follower durability behavior;
4. object-store conditional fencing;
5. product open policy or engine error mapping.

## Scope

M4P-L1 implements:

1. a tightened backend-owned `delete_object` contract in a new
   `backend/delete.rs` module or equivalent location;
2. delete outcome types that mirror the existing publish-outcome style:
   object, delete status, durability status, failure kind, and backend source
   error;
3. localfs `delete_object` behavior that removes the object file, syncs the
   parent namespace on supported durable-local platforms, and classifies fault
   windows;
4. memory/cache `delete_object` outcomes that do not claim durability;
5. reusable durable-local backend conformance tests;
6. a production source guard for direct filesystem IO above L1;
7. documentation updates only if implementation proves a narrower contract than
    this plan describes.

M4P-L1 does not implement:

1. new cleanup algorithms;
2. retention policy changes;
3. checkpoint format changes;
4. table manifest recovery changes;
5. object-durable or distributed durable mode;
6. multi-writer object-store fencing;
7. benchmark fast paths;
8. L2 object-family classification, object-name construction, or layout helper
   changes;
9. L4 service adapters such as `ObjectDeleter`;
10. migration of WAL, snapshot, sidecar, or quarantine cleanup call sites;
11. L8 lifecycle health-debt mapping;
12. public L9 API changes, except diagnostics that later L9 slices may expose.

## Delete Contract

The delete API should stay backend-owned, object-first, and single-method. It
must not expose parent-directory sync, path names, local filesystem details,
delete options, or a parallel delete method.

Target shape:

```text
delete_object(object_name) -> DeleteResult<DeleteOutcome>
```

Expected vocabulary:

1. `DeleteOutcome`
   - object name;
   - status: `Deleted` or `AlreadyMissing`;
   - durability fact: durable-local success is durable; cache/memory success
     carries no durability claim.
2. `DeleteFailureKind`
   - `FailedBeforeRemoval`;
   - `RemovalUnknown`;
   - `RemovedDurabilityUnconfirmed`.
3. `DeleteError`
   - object name;
   - failure kind;
   - backend source error.

Rules:

1. There is one delete method. Do not add `delete_object_durable`,
   `DeleteOptions`, or `DeleteMode` for MVP.
2. The `DeleteObject` capability remains the single delete capability.
3. Cache/memory delete removes the object from the cache backend and returns a
   successful outcome with no durability claim.
4. Durable-local localfs delete returns success only after visible removal and
   namespace durability are complete.
5. Localfs can participate in durable-local mode only on platforms where delete,
   publish, sync, and writer-lock semantics can satisfy durable-local
   conformance.
6. `AlreadyMissing` is a successful idempotent cleanup fact, not a backend
   failure.
7. A parent-sync failure after visible removal must report
   `RemovedDurabilityUnconfirmed`; callers must not record clean durable cleanup.
8. A failure before removal must leave the object readable unless the backend
   reports `RemovalUnknown`.
9. Durable services should consume these `delete_object` outcomes in later L4/L8
   slices when they need cleanup durability. M4P-L1 only provides and proves the
   backend primitive.

If implementation shows that the existing `delete_object` shape cannot represent
mode-defined delete outcomes cleanly, stop and revise before coding further. The
replacement must still keep filesystem-specific parent sync inside L1 and must
not introduce a second delete operation for the same object removal.

## Implementation Steps

### 1. Tighten Delete Vocabulary

1. Add `backend/delete.rs` beside `backend/publish.rs`.
2. Define delete outcome, failure kind, error, and result aliases.
3. Re-export the delete vocabulary from `backend/mod.rs`.
4. Change the existing `Backend::delete_object` return type to use the delete
   outcome vocabulary.
5. Keep `BackendCapability::DeleteObject` as the only delete capability.
6. Add source or unit checks that prevent introducing a parallel
   `delete_object_durable` method.

Acceptance:

1. cache mode requirements continue to require only the existing `DeleteObject`
   capability;
2. durable-local mode requirements continue to require `DeleteObject`, but
   durable-local conformance proves stronger delete semantics for localfs;
3. delete failures preserve source error kind and stable failure kind;
4. no new delete method, delete options, or delete capability is added.

### 2. Implement Cache And Localfs Behavior

Memory backend:

1. keep `delete_object` as a cache operation;
2. return `Deleted` or `AlreadyMissing` outcomes;
3. report no durability claim in cache/memory delete outcomes;
4. test that cache mode does not claim delete durability.

Localfs backend:

1. reject writer-lock object mutation through `delete_object` just as basic write
   and current delete do;
2. resolve object name to private path with existing `path_for`;
3. treat missing object as `AlreadyMissing`;
4. remove the object file;
5. sync the parent namespace on supported platforms;
6. classify before-removal, removal, and parent-sync fault windows;
7. avoid deleting or scanning stale temp files in this slice.

Acceptance:

1. localfs delete has no filesystem calls outside `backend/local_fs.rs`;
2. localfs delete outcome is stable across reopen after successful durable-local
   removal;
3. localfs does not satisfy durable-local conformance on unsupported platforms.

### 3. Build Durable Backend Conformance

Refactor `backend/conformance.rs` into explicit suites:

1. basic object conformance;
2. cache-mode requirement validation;
3. durable-local conformance.

Durable-local conformance must cover:

1. append offset and metadata;
2. object sync;
3. durable publish create and replace;
4. publish fault classification;
5. writer-lock exclusion and release;
6. delete success and already-missing in cache and durable-local modes;
7. localfs delete fault classification;
8. weaker delete durability facts on cache/memory backends;
9. storage-mode capability validation.

Acceptance:

1. memory runs basic conformance, cache-mode requirement validation, and
   non-durable delete outcome checks;
2. localfs runs basic conformance, cache-mode requirement validation, and
   durable-local conformance;
3. conformance helpers are reusable by future backend adapters.

### 4. Add L1 Source Guard

Add a source-level regression guard that scans production
`crates/storage-next/src` files and fails if direct filesystem IO appears outside
`crates/storage-next/src/backend/local_fs.rs`.

Forbidden production patterns include:

1. `std::fs`;
2. `std::fs::File`;
3. `std::fs::OpenOptions`;
4. `fs::rename`;
5. `fs::remove_file`;
6. `fs::remove_dir`;
7. `fs::create_dir`;
8. `File::open`;
9. `File::create`;
10. direct directory sync helpers outside localfs.

The guard should ignore test-only code and explicitly allow
`backend/local_fs.rs`. If line-based filtering cannot distinguish test-only code
reliably, place the guard in a small Rust test utility with an allowlist of
production files and fail closed for new production matches.

Acceptance:

1. direct IO in localfs passes;
2. adding `std::fs::remove_file` to a non-localfs production file fails;
3. comments or plan documents are not scanned.

### 5. Record L4/L8 Follow-Up Requirements

M4P-L1 must leave clear downstream requirements, not implement them.

Required follow-up notes:

1. L4 must decide whether to add an `ObjectDeleter` helper analogous to
   `ObjectPublisher`.
2. L4 service slices must migrate WAL, sidecar, snapshot, and quarantine cleanup
   call sites where cleanup claims durable removal.
3. L8 lifecycle slices must decide how L4 cleanup uncertainty maps into recovery
   health debt.
4. Tests that intentionally remove objects to seed corruption may continue using
   `delete_object`, but must not treat cache-mode outcomes as durable cleanup.

Acceptance:

1. M4P-L1 closeout names the downstream slices or backlog entries;
2. M4P-L1 does not modify service semantics except through compiler-required API
   adaptations caused by tightening the existing backend method;
3. no L1 code interprets WAL, snapshot, sidecar, quarantine, retention, or
   lifecycle policy.

## Source Guards

M4P-L1 adds one guard category:

1. production source guard for direct filesystem IO above L1.

A later L4 cleanup slice may add a service cleanup guard that flags durable
service modules ignoring `delete_object` outcomes when the result is used to
claim durable removal. That guard is not part of M4P-L1.

## Stop Conditions

Stop and revise the plan if:

1. mode-defined delete outcomes cannot be represented without exposing localfs
   parent sync to L4/L8;
2. durable-local open validation cannot be staged without breaking cache or
   wasm-none mode;
3. localfs fault windows cannot distinguish pre-removal from post-removal parent
   sync failure in tests;
4. the source guard cannot avoid false positives without a brittle list of test
   fixtures.

## Expected Mechanical Counter Movement

No read-path or load-path performance counter movement is expected from M4P-L1.

Expected mechanical movement:

1. `DeleteObject` remains the single delete capability;
2. delete outcomes are typed;
3. localfs durable cleanup does not leak filesystem IO above L1;
4. durable backend conformance coverage increases;
5. source guard fails on direct production filesystem IO outside localfs.
6. downstream L4/L8 cleanup work has explicit owners and is not implemented in
   L1.

If benchmarks change materially after this slice, treat it as incidental and
investigate separately. M4P-L1 is not a performance-tuning slice.

## Closeout Criteria

M4P-L1 is complete when:

1. all L1 audit findings listed in this plan are closed or explicitly deferred;
2. delete vocabulary and localfs implementation are in L1;
3. memory/cache non-durable delete outcomes are tested;
4. durable-local conformance covers publish, append, sync, writer lock, and
   delete;
5. production source guard passes and fails on a seeded violation;
6. downstream L4/L8 service cleanup requirements are recorded with owner layers;
7. `cargo test -p strata-storage-next` passes for touched test scopes;
8. `cargo clippy -p strata-storage-next --lib --features perf-trace -- -D warnings`
   passes if storage-next Rust code is changed.
