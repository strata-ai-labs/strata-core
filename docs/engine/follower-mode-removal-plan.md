# Follower Mode Removal Plan

Status: V1 cleanup plan, aligned with engine-next architecture

This document is still active as the implementation cleanup plan for removing
follower mode from the current codebase. The target architecture is defined in
`docs/architecture/engine-next-architecture.md` and
`docs/architecture/engine-next/ipc-and-command-boundary-contract.md`.

## Purpose

Remove follower mode from the current Strata codebase before storage-next and
engine-next work begins.

Follower mode currently acts as a second local database runtime over the same
disk state. It avoids the primary lock, reconstructs state through recovery,
tails WAL records through `Database::refresh`, maintains local watermarks, and
publishes graph/vector/search derived-state updates through refresh hooks.

That is a different technical mechanism from IPC, but it is not the product
path Strata should keep for V1. The supported V1 model should be:

1. A local embedded primary opens the database and owns durability.
2. `AccessMode::ReadOnly` is an access policy on a product handle or session.
3. IPC is the local shared-process path when another process owns the primary.
4. There is no secondary local database runtime reading the primary WAL.

This plan removes follower mode deliberately so future architecture work does
not have to preserve a second recovery, visibility, and derived-index
publication model.

## Decision

Follower mode is removed from the V1 product and engine runtime.

IPC remains as the local multi-process access mechanism. Read-only open remains
as an access policy. A locked database without an IPC socket should produce a
clear error telling the user to start the IPC server; it should not recommend a
read-only follower.

## Non-Goals

1. Do not redesign IPC in this work.
2. Do not change WAL, manifest, checkpoint, or snapshot file formats.
3. Do not change normal primary recovery semantics.
4. Do not introduce storage-next abstractions.
5. Do not preserve follower compatibility shims. Strata is pre-v1, and keeping
   dead compatibility surface would make the next architecture pass harder.

## Current Surface Area

Follower mode is not isolated. It crosses product API, CLI, engine runtime,
recovery, storage layout, and primitive derived-state code.

Product and CLI surface:

- `OpenOptions::follower`
- `OpenOptions::follower(true)`
- CLI `--follower`
- product-open lock error text that recommends `--follower`
- `DescribeResult::follower`
- executor error hint: "This database is a read-only follower..."

Engine runtime surface:

- `DatabaseMode::Follower`
- `OpenSpec::follower`
- `Database::open_follower`
- `open_runtime_follower`
- `acquire_follower_db`
- `Database::is_follower`
- `Database.follower`
- follower-specific skip paths in checkpoint, compaction, pruning, shutdown,
  repair, lifecycle, and maintenance code

Recovery and durability surface:

- `RecoveryMode::Follower`
- `StorageRecoveryMode::FollowerNeverCreateManifest`
- `RecoveryOutcome::persisted_follower_state`
- follower missing-manifest behavior
- follower state restore and invalid-state cleanup
- `follower_state.json`
- `follower_audit.log`
- layout helpers for follower state and audit files

Refresh and derived-state surface:

- `Database::refresh`
- `Database::follower_status`
- `Database::admin_skip_blocked_record`
- `ContiguousWatermark`
- `RefreshGate`
- `RefreshOutcome`
- `RefreshHook`
- `PreparedRefresh`
- `ReplayObserver`
- graph/vector/search refresh hook implementations
- vector follower cache distrust/rebuild policy

Tests and docs:

- `crates/engine/tests/follower_tests.rs`
- follower-related recovery language in
  `docs/architecture/storage-next/l8-lifecycle-recovery-maintenance.md`
- follower allowances in `docs/storage/v1-storage-consumption-contract.md`
- follower mentions in `docs/engine/engine-crate-map.md`
- follower entry in `docs/product/strata-v1-feature-inventory.md`
- CLI external tests using `--follower`

## Target Behavior

Opening a database:

1. `Strata::open(path)` opens a local primary if no other primary owns the path.
2. `Strata::open_with(path, OpenOptions::new().access_mode(AccessMode::ReadOnly))`
   opens the local primary with a read-only product handle if no other primary
   owns the path.
3. If another process owns the primary lock and `<data_dir>/strata.sock` exists,
   product open returns an IPC-backed handle.
4. If another process owns the primary lock and no socket exists, product open
   fails with a message that points to `strata up`.
5. There is no option to open a read-only secondary over shared storage.

Reads and writes:

1. Read-only access rejects writes at the executor/session boundary before
   mutation.
2. Local read-only and IPC read-only use the same access-mode semantics.
3. Engine primary internals no longer need follower-specific commit guards.

Recovery:

1. Disk recovery has one product runtime policy: primary recovery.
2. Cache mode remains no-disk/no-WAL.
3. Storage recovery does not need a follower mode that refuses manifest creation.
4. WAL reading outside primary recovery is not part of the V1 runtime.

Derived indexes:

1. Graph, vector, and search derived state is updated by primary commit,
   recovery, freeze/load, or explicit rebuild paths.
2. There is no follower staged-publication path.
3. No primitive implementation needs to distrust primary sidecar state because a
   secondary runtime is observing a different visibility watermark.

## Removal Sequence

### Phase 1: Freeze The Product Contract

Update product and architecture docs before deleting code.

Required changes:

1. Split "Follower and IPC Open" in the feature inventory into separate entries:
   "IPC/shared process access" and "Follower mode".
2. Mark follower mode as removed from V1.
3. Keep read-only open as V1 required.
4. Keep IPC as the local shared-process mechanism.
5. Update the storage consumption contract to state that engine consumes storage
   for primary and cache only.
6. Update durability/recovery architecture docs to remove follower refresh as a
   current runtime contract.

Acceptance checks:

1. Product docs no longer present follower as an optional V1 pathway.
2. Architecture docs describe IPC and read-only access without referring users to
   follower mode.
3. Historical follower details may remain only in archived cleanup documents.

### Phase 2: Remove Product And CLI Entry Points

Remove user-facing ways to request follower mode while leaving deeper engine
runtime code temporarily intact if needed for compile sequencing.

Required changes:

1. Remove `OpenOptions::follower` and the follower builder.
2. Remove CLI `--follower`.
3. Remove product-open follower branching.
4. Remove lock-error text recommending `--follower`.
5. Remove follower-specific read-only forcing from CLI open.
6. Remove `DescribeResult::follower` unless there is another non-follower
   product meaning for it.
7. Remove follower-specific executor access-denied hints.

Expected replacement behavior:

1. `--read-only` remains available.
2. A locked database with a socket still opens through IPC.
3. A locked database without a socket tells the user to run `strata up`.
4. Product and CLI users cannot request follower mode.

Acceptance checks:

1. `rg "follower\\(" crates/cli crates/executor crates/engine/src/database/open_options.rs`
   has no production matches.
2. `rg "--follower" crates/cli tests docs/product docs/engine docs/storage`
   has no current-doc or production-code matches.
3. Executor and CLI read-only tests still pass.
4. IPC fallback tests still pass.

### Phase 3: Remove Engine Follower Open Runtime

Delete the second database runtime path.

Required changes:

1. Remove `DatabaseMode::Follower`.
2. Remove `OpenSpec::follower`.
3. Remove `Database::open_follower`.
4. Remove `open_runtime_follower`.
5. Remove `acquire_follower_db`.
6. Remove `Database.follower`.
7. Remove `Database::is_follower`.
8. Remove follower branches from open, lifecycle, checkpoint, compaction,
   pruning, shutdown, repair, and maintenance code.
9. Keep primary and cache open behavior unchanged.

Important sequencing rule:

If deleting `Database::is_follower` creates many compile failures, fix each
callsite by deciding whether the code should:

1. run for primary/cache;
2. run only for persistent primary;
3. become unnecessary and be deleted.

Do not replace `is_follower()` with a temporary compatibility method returning
`false`; that hides dead code and defeats the cleanup.

Acceptance checks:

1. `rg "DatabaseMode::Follower|OpenSpec::follower|open_follower|is_follower|\\.follower" crates`
   has no production matches.
2. Primary open, cache open, read-only open, and IPC fallback tests pass.
3. Checkpoint, compact, prune, shutdown, and maintenance tests pass without
   follower-specific no-op branches.

### Phase 4: Simplify Recovery And Storage Durability Contracts

Remove follower recovery policy from engine and storage.

Required changes:

1. Replace `RecoveryMode::{Primary, Follower}` with a primary-only recovery
   path, or delete the enum if it no longer carries useful information.
2. Remove `StorageRecoveryMode::FollowerNeverCreateManifest`.
3. Remove follower missing-manifest recovery behavior.
4. Remove `RecoveryOutcome::persisted_follower_state`.
5. Remove follower-state restore and invalid-state cleanup.
6. Remove follower-specific recovery error wording.
7. Remove layout helpers for follower state and audit files.
8. Update storage recovery tests to cover primary behavior only.

Acceptance checks:

1. `rg "FollowerNeverCreateManifest|RecoveryMode::Follower|persisted_follower_state|follower_state|follower_audit" crates`
   has no production matches.
2. Primary recovery tests still cover manifest creation, snapshot install, WAL
   replay, degraded recovery, lossy recovery, and codec behavior.
3. Storage recovery APIs no longer expose follower policy.

### Phase 5: Delete Refresh And Replay Machinery

Remove the WAL-tail secondary-publication system.

Required changes:

1. Delete `Database::refresh`.
2. Delete `Database::follower_status`.
3. Delete `Database::admin_skip_blocked_record`.
4. Delete `database/refresh.rs` if no non-follower types remain.
5. Delete `ContiguousWatermark`, `RefreshGate`, `RefreshOutcome`,
   `BlockReason`, `BlockedTxn`, `UnblockError`, `RefreshHook`,
   `PreparedRefresh`, and related follower-only types.
6. Delete replay observers if they exist only for follower replay.
7. Remove refresh publication barriers from `Database`.
8. Remove follower shutdown serialization with refresh.

Acceptance checks:

1. `rg "refresh\\(|RefreshHook|PreparedRefresh|ReplayObserver|follower_status|admin_skip_blocked_record|ContiguousWatermark" crates`
   has no production matches.
2. Shutdown tests still pass.
3. No primitive runtime depends on refresh hooks.

### Phase 6: Clean Up Graph, Vector, And Search

Remove primitive code that only existed to keep follower-derived state coherent.

Required changes:

1. Remove graph refresh hook implementation.
2. Remove vector refresh hook implementation.
3. Remove search refresh hook implementation.
4. Remove vector follower sidecar-cache distrust/rebuild policy.
5. Keep primary recovery, freeze/load, commit observers, abort observers, and
   explicit rebuild paths intact.
6. Replace follower characterization tests with primary recovery or explicit
   rebuild tests only when the behavior remains product-relevant.

Acceptance checks:

1. `rg "RefreshHook|apply_refresh|pre_delete_read|follower" crates/engine/src/graph crates/engine/src/vector crates/engine/src/search`
   has no production matches, except unrelated historical comments if any.
2. Graph, vector, and search recovery tests pass.
3. Primitive commit/abort behavior is unchanged.

### Phase 7: Remove Tests That Only Prove Follower Behavior

Delete follower-only tests rather than rewriting them into no-op compatibility
tests.

Required changes:

1. Delete `crates/engine/tests/follower_tests.rs`.
2. Remove executor tests that open `OpenOptions::follower(true)`.
3. Remove CLI external tests using `--follower`.
4. Remove product-open follower tests.
5. Keep or add replacement tests for:
   - read-only local open;
   - read-only IPC fallback;
   - locked-without-socket error;
   - primary recovery;
   - primitive recovery/rebuild behavior that remains product-relevant.

Acceptance checks:

1. Test names and fixtures no longer refer to follower mode.
2. Read-only and IPC behavior are still directly tested.
3. The full engine and executor suites do not depend on follower-only helpers.

### Phase 8: Add Guards Against Reintroduction

Add guard tests after code deletion, not before.

Required checks:

1. Production code must not expose `OpenOptions::follower`.
2. Production CLI code must not define `--follower`.
3. Production engine code must not define `DatabaseMode::Follower`.
4. Production recovery/storage code must not define follower recovery mode.
5. Production docs outside archives must not present follower mode as a current
   product pathway.

The guard should allow historical mentions under `docs/engine/archive` and
`docs/storage/archive`, but current docs should stay clean.

### Phase 9: Close Out Documentation

Update active documentation to the new model.

Required changes:

1. `docs/product/strata-v1-feature-inventory.md`
2. `docs/product/strata-v1-product-requirements.md`
3. `docs/engine/engine-consolidation-plan.md`
4. `docs/engine/engine-crate-map.md`
5. `docs/storage/v1-storage-consumption-contract.md`
6. `docs/architecture/storage-next/l8-lifecycle-recovery-maintenance.md`

The final docs should describe:

1. primary open;
2. cache open;
3. read-only access policy;
4. IPC fallback/shared access;
5. recovery/checkpoint/WAL rules without follower exceptions.

## Risk Register

### Risk: Read-Only Open Becomes Confused With Follower Removal

Read-only open must remain. Removing follower mode should not remove
`AccessMode::ReadOnly` or `--read-only`.

Mitigation:

1. Keep read-only tests in executor and CLI.
2. Add read-only IPC fallback coverage.
3. Make the docs say read-only is an access policy, not a runtime mode.

### Risk: IPC Looks Like The Only Way To Read

IPC should be the answer only when another process owns the primary lock. A
single-process embedded user should still be able to open a local handle and
read normally.

Mitigation:

1. Keep `Strata::open(path)` and local read-only open tests.
2. Keep IPC fallback classification limited to primary lock failures.

### Risk: Recovery Coverage Drops

Follower tests currently cover useful WAL/recovery edge cases. Some of those
cases are follower-only, but some may be primary-relevant.

Mitigation:

1. Before deleting each follower test file, scan every test and classify it as
   follower-only or primary-relevant.
2. Port primary-relevant recovery assertions to primary recovery tests.
3. Do not keep follower APIs just to preserve tests.

### Risk: Primitive Rebuild Semantics Regress

Graph, vector, and search refresh hooks may be covering derived-state
publication invariants that still matter after follower removal.

Mitigation:

1. Preserve commit observer and recovery/freeze/load tests.
2. Add explicit rebuild/recovery tests where follower tests were the only
   coverage for a product-relevant derived-state behavior.

### Risk: Storage API Keeps Dead Follower Concepts

Leaving `FollowerNeverCreateManifest`, follower layout paths, or follower
state files in storage would make storage-next consume a dead concept.

Mitigation:

1. Remove storage follower symbols in the same cleanup series.
2. Add guard tests that fail on production follower recovery symbols.
3. Update the storage consumption contract as part of closeout.

## Suggested Review Order

1. Product/API removal: no user can request follower.
2. Engine runtime removal: no second local database runtime exists.
3. Recovery/storage cleanup: no follower durability policy remains.
4. Primitive cleanup: no refresh hook publication path remains.
5. Tests/guards/docs closeout.

This order keeps behavior reviewable. User-facing behavior changes first, then
the dead machinery is removed behind it.

## Final Acceptance Criteria

1. `rg -n "follower|Follower|refresh\\(|RefreshHook|ReplayObserver" crates`
   returns no production follower-mode matches.
2. Current docs outside archive do not describe follower mode as supported.
3. `--read-only` still works.
4. IPC fallback still works.
5. Locked-without-socket errors point to `strata up`, not follower mode.
6. Primary recovery, checkpoint, WAL replay, snapshot install, graph, vector,
   search, executor, and CLI tests pass.
7. Storage consumption docs list no follower-specific storage operation.
