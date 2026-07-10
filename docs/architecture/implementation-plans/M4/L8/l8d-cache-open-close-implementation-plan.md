# L8D Implementation Plan: Cache Open And Close

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8d-cache-open-close-test-plan.md`

## Objective

Implement the cache-mode lifecycle runtime baseline.

L8D should turn the side-effect-free L8A-L8C lifecycle facts into a usable
cache-mode storage runtime that opens volatile L6/L7 state, admits cache commits
through the existing L7 cache executor, exposes raw storage facts, and closes
idempotently without durable recovery, durable services, writer locks,
manifests, WAL objects, snapshots, table objects, or quarantine inventory.

This slice is intentionally narrow. Durable local service assembly starts in
L8E. Recovery starts in L8F/L8G. Maintenance, flush, checkpoint, retention,
quarantine, repair, and full close drain land in later L8 slices.

## Inputs

1. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
2. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8a-lifecycle-scaffold-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L8/l8b-lifecycle-state-open-plan-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L8/l8c-storage-mode-capability-validation-implementation-plan.md`
7. `crates/storage-next/src/lifecycle/`
8. `crates/storage-next/src/backend/memory.rs`
9. `crates/storage-next/src/branch/`
10. `crates/storage-next/src/commit/cache.rs`
11. `crates/storage-next/src/commit/branch_registry.rs`
12. `crates/storage-next/src/commit/visibility.rs`
13. `crates/storage-next/src/commit/durable_gate.rs`
14. `crates/engine/src/database/recovery.rs`
15. `crates/engine/src/database/lifecycle.rs`

## Existing-Code Source Map

| Current file | L8D evidence | L8D action |
|---|---|---|
| `crates/engine/src/database/recovery.rs` | Old recovery explicitly states cache/ephemeral databases never recover because they have no disk state. | Preserve the rule as storage-owned L8 behavior: cache open reports healthy/no recovered durable visibility and skips recovery orchestration. |
| `crates/engine/src/database/lifecycle.rs` | Old close path is ordered and idempotent, but includes product hooks, WAL flush, manifest sync, and registry/file-lock release. | Port only the idempotent close state shape. Cache close must not run durable sync, product freeze hooks, writer-lock release, or registry release. |
| `crates/storage-next/src/lifecycle/state.rs` | L8B state machine already supports `New -> Opening -> Open -> Closing -> Closed`. | Use the state machine as the authoritative lifecycle gate for cache open, commits/reads, and close. |
| `crates/storage-next/src/lifecycle/capability.rs` | L8C validates cache backend capabilities by reading only `backend.capabilities()`. | Cache open must call the L8C preflight before assembling volatile runtime state. |
| `crates/storage-next/src/branch/state.rs` | `BranchLocalState::empty`, `capture_read_view`, and `append_committed_rows_atomically` provide volatile branch state. | Use these L6 surfaces for cache mode; do not introduce a bespoke row store. |
| `crates/storage-next/src/commit/cache.rs` | `CommitCacheRuntime` executes no-WAL commits over L6 state, commit allocation, branch guards, visible tracker, and unresolved gate. | Compose these existing L7 parts; L8D must not reimplement commit stamping or conflict validation. |
| `crates/storage-next/src/commit/branch_registry.rs` | Branch registry, generation guards, and per-branch admission exist in L7. | Initialize the root cache branch descriptor and keep branch admission inside L7. |
| `crates/storage-next/src/backend/memory.rs` | Memory backend satisfies cache-mode capability requirements. | Use as the concrete positive backend for cache lifecycle tests. |

## Scope

L8D implements:

1. a crate-private cache lifecycle runtime module;
2. a cache-open request/fact shape for the initial storage branch;
3. cache open preflight that accepts only `StorageMode::Cache`;
4. backend capability validation through L8C before runtime assembly;
5. volatile L6 `BranchLocalState` construction;
6. volatile L7 branch registry, branch guard set, commit fact allocator,
   visible-version tracker, unresolved-durable gate, and commit config
   construction;
7. `StorageOpenOutcome` reporting `StorageMode::Cache`, `Created`, healthy
   recovery, no recovered visible version, and no durable maintenance readiness;
8. cache commit execution by delegating to `CommitCacheRuntime`;
9. cache read-view access through the existing L6 `BranchReadView`;
10. lifecycle admission checks that reject commits/reads before open and after
    close;
11. idempotent cache close that transitions through the L8B state machine and
    performs no durable flush/sync/recovery side effects;
12. generated/testkit counters for cache open, cache close, absence of durable
    services, cache commit/read smoke, and empty reopen semantics;
13. an L8D porting-log entry after implementation.

L8D does not implement:

1. public L9 open or branch API;
2. durable local service assembly;
3. writer-lock acquisition or release;
4. database manifest load/create/publish;
5. WAL service open, append, replay, repair, truncation, or sync;
6. snapshot/checkpoint object load or publication;
7. durable table object publication;
8. quarantine inventory load/publish/purge;
9. L7 durable commit runtime construction;
10. L7 replay or allocator catch-up from durable facts;
11. maintenance scheduling;
12. flush, checkpoint, compaction, materialization, retention, quarantine,
    purge, repair, or full close drain;
13. product open policy, product branch workflows, primitive registries,
    follower mode, IPC, StrataHub, or engine freeze hooks.

## Design Decisions

### Cache Mode Is Volatile By Construction

Cache open starts from empty storage state every time. It may use a backend for
capability preflight, but it must not inspect backend object inventory or infer
state from existing objects.

Rules:

1. `StorageOpenOutcome::recovered_visible_version()` is always `None`.
2. `StorageOpenOutcome::recovery_health()` is healthy.
3. `StorageOpenDisposition` is `Created`; cache mode has no durable
   opened-existing meaning in V1.
4. Reopening cache mode creates a fresh empty volatile runtime.
5. Cache mode must not claim crash recovery or degraded recovery.
6. Cache open records backend capability facts and raw open stats, but leaves
   all durable recovery fact fields empty.

### L8D Composes L6 And L7

L8D should not create a second commit path. The runtime should own the L7 state
objects needed by `CommitCacheRuntime` and create short-lived cache executors
per commit.

Expected L7 parts:

1. `CommitRuntimeConfig`;
2. `CommitBranchRegistry`;
3. `CommitBranchGuardSet`;
4. `CommitFactAllocator<S>`;
5. `VisibleVersionTracker`;
6. `CommitUnresolvedDurableGate`;
7. `CommitBranchGeneration` for the initial branch;
8. `CommitCacheRuntime` as the mutating executor.

Expected L6 parts:

1. `BranchLocalState`;
2. `BranchRuntimeConfig`;
3. `BranchReadView`.

### Cache Close Is A Lifecycle Gate, Not Durable Shutdown

L8D close is intentionally minimal:

1. reject new commits/ordinary reads once closing begins;
2. transition `Open -> Closing -> Closed`;
3. report the first `CloseOutcome` with `ClosePhase::Closed` and
   `CloseOutcomeStatus::Complete`;
4. allow repeated close after `Closed` with
   `CloseOutcomeStatus::Idempotent`;
5. first close carries `LifecycleCloseFact::Complete`; repeated close carries
   `LifecycleCloseFact::AlreadyClosed`;
6. close effects mark commits quiesced, maintenance drained, and guards
   released, but not durable sync;
7. do not flush WAL, sync manifest, release writer locks, stop durable
   background workers, or run engine freeze hooks.

Later L8 close slices may extend the durable close path. They must not make
cache close start doing durable work.

### Initial Branch Fact

L8D should accept an explicit initial `BranchId` supplied by the caller above
L8. That keeps L8 storage-internal and product-neutral: L8 does not decide what
the default branch is called, but it can construct the volatile branch state
needed for L6/L7 cache operation.

Suggested rule:

1. one initial active branch is created at generation `1`;
2. the branch registry contains exactly that branch after open;
3. the branch state is empty;
4. the visible version starts at `CommitVersion::ZERO`;
5. the commit version allocator starts at `CommitVersion::ZERO`;
6. timestamp guard starts with no allocated timestamp;
7. unresolved durable gate starts empty.

Multi-branch lifecycle operations remain above or after L8D. L8D may keep the
internal shape extensible enough to add more branches later, but it should not
invent product branch creation workflows.

## Module Layout

Add a focused cache lifecycle module:

```text
crates/storage-next/src/lifecycle/
  cache.rs
```

Update `mod.rs` to crate-private re-export the L8D surface.

Tests should stay split:

```text
crates/storage-next/src/lifecycle/tests/
  cache.rs
```

Expected ownership after L8D:

1. `capability.rs`: side-effect-free storage-mode capability preflight.
2. `state.rs`: lifecycle transition and operation admission.
3. `cache.rs`: cache open/commit/read/close baseline runtime.
4. `tests/cache.rs`: direct cache runtime tests.

## Proposed Type Surface

Names may change if responsibilities remain intact. All production items stay
`pub(crate)`.

### `LifecycleCacheOpenRequest`

Suggested shape:

```text
LifecycleCacheOpenRequest {
  plan: StorageOpenPlan,
  initial_branch_id: BranchId,
  branch_generation: CommitBranchGeneration,
}
```

Rules:

1. `plan.storage_mode()` must be `StorageMode::Cache`;
2. branch generation must be nonzero;
3. request validation must not inspect durable objects;
4. request carries storage facts only, not product branch names.

If a separate request type is unnecessary, the same facts may be parameters to
`LifecycleCacheRuntime::open`.

### `LifecycleCacheRuntime<S>`

Suggested shape:

```text
LifecycleCacheRuntime<S> {
  state: LifecycleStateMachine,
  open_plan: StorageOpenPlan,
  open_outcome: StorageOpenOutcome,
  capability_outcome: LifecycleCapabilityOutcome,
  branch: BranchLocalState,
  registry: CommitBranchRegistry,
  guard_set: CommitBranchGuardSet,
  allocator: CommitFactAllocator<S>,
  visible: VisibleVersionTracker,
  durable_gate: CommitUnresolvedDurableGate,
  commit_config: CommitRuntimeConfig,
}
```

Rules:

1. `open` validates the plan and capabilities before constructing branch/commit
   runtime state;
2. `open` transitions through `OpenRequested` and `CacheOpenReady`;
3. failed open returns a typed `LifecycleError` and no partially opened runtime;
4. `state()` exposes raw lifecycle state for tests and later slices;
5. `open_outcome()` returns the cache open facts;
6. `branch_state()` and `read_view()` expose L6 facts for storage-internal
   consumers;
7. `execute_cache_commit()` delegates to `CommitCacheRuntime`;
8. `close()` is idempotent and has no durable side effects;
9. operations check `LifecycleStateMachine::admit` before touching L6/L7.

### Timestamp Source

Use the existing L7 `CommitTimestampSource` abstraction.

Implementation choices:

1. tests can use `CommitManualTimestampSource`;
2. production L9 may later supply a monotonic source;
3. L8D must not call wall-clock APIs directly.

### Error Mapping

Cache runtime methods should map lower-layer failures into lifecycle errors
only at lifecycle boundaries.

Rules:

1. capability failure remains `LifecycleError::CapabilityMismatch`;
2. invalid cache mode remains `LifecycleError::InvalidOpenPlan`;
3. L6 failures map to `LifecycleLowerLayer::BranchRuntime`;
4. L7 failures map to `LifecycleLowerLayer::CommitRuntime`;
5. close-state failures remain `LifecycleError::InvalidLifecycleState` or
   `LifecycleError::CloseFailed`;
6. source errors should be preserved where the lower layer exposes an error
   source.

## Open Sequence

Target cache open sequence:

```text
validate StorageOpenPlan
reject non-cache storage mode
transition New -> Opening
validate backend capabilities through L8C
construct empty BranchLocalState for initial branch
construct CommitBranchRegistry and register initial branch generation
construct CommitBranchGuardSet
construct CommitFactAllocator from zero version/timestamp floor
construct VisibleVersionTracker at ZERO
construct CommitUnresolvedDurableGate
construct CommitRuntimeConfig
construct healthy cache StorageOpenOutcome
transition Opening -> Open
return LifecycleCacheRuntime
```

Ordering rules:

1. capability validation happens before L6/L7 state construction;
2. no durable service can be constructed before or after capability validation
   in this slice;
3. the open outcome is validated before the runtime is returned;
4. any failure before `CacheOpenReady` returns no opened runtime;
5. no backend method other than `capabilities()` is called.

## Commit And Read Smoke

L8D should include only enough runtime methods to prove the assembled cache
parts are coherent.

Required operations:

1. execute a cache mutating batch through `CommitCacheRuntime`;
2. capture a branch read view and read the committed row through L6;
3. reject commit attempts before open and after close through lifecycle
   admission;
4. reject durable commit batches through the existing L7 cache executor.

Do not add a public storage query API in L8D. The smoke path can remain
crate-private and test-oriented until L9 wraps it.

## Close Sequence

Target cache close sequence:

```text
if Closed: return idempotent closed outcome
admit Close in Open
transition Open -> Closing
perform no durable drain/sync/release operations
transition Closing -> Closed
return close outcome
```

Rules:

1. close from `Closed` is idempotent;
2. close from `New`, `Opening`, `Recovering`, or `Failed` is rejected unless
   later slices add explicit recovery/drop behavior;
3. no WAL flush or manifest sync is attempted;
4. no writer guard release is attempted because cache mode did not acquire one;
5. the volatile runtime remains inspectable for storage facts after close but
   rejects ordinary reads and commits.

## Source-Boundary Rules

L8D production code may import:

1. `crate::backend` capability and backend trait types;
2. `crate::branch` runtime types;
3. `crate::commit` cache-runtime and supporting types;
4. `crate::lifecycle` sibling modules;
5. `strata_core_next` storage fact atoms.

L8D production code must not import or call:

1. `crate::service`;
2. `crate::layout`;
3. `crate::format`;
4. raw filesystem/path/env APIs;
5. engine/product modules;
6. public API modules;
7. follower or refresh modules;
8. StrataHub modules;
9. primitive registries or `VersionedValue` surfaces.

## Implementation Steps

### L8D-A: Cache Open Facts

1. Add the cache open request/fact type or equivalent open parameters.
2. Reject non-cache storage mode with `LifecycleError::InvalidOpenPlan`.
3. Keep branch generation explicit and nonzero.
4. Add direct tests for request validation.

### L8D-B: Cache Runtime Assembly

1. Add `lifecycle/cache.rs`.
2. Construct volatile L6/L7 state from zero baselines.
3. Preserve the accepted L8C capability outcome.
4. Return a validated cache `StorageOpenOutcome`.
5. Add direct tests for opened state and baseline facts.

### L8D-C: Cache Commit/Read Smoke

1. Add a crate-private commit helper that builds `CommitCacheRuntime` over the
   runtime's owned L6/L7 state.
2. Add a read-view helper that returns an L6 `BranchReadView`.
3. Keep the method names storage-shaped and crate-private.
4. Add tests that commit one put and read it back through L6.

### L8D-D: Cache Close

1. Add idempotent close.
2. Use the L8B state machine for transitions.
3. Reject commits/reads after close.
4. Assert no durable backend calls occur during close.

### L8D-E: Generated Coverage And Guards

1. Extend the lifecycle testkit outcome with cache-open/cache-close counters.
2. Add generated scripts for open, commit/read smoke, close, and reopen-empty.
3. Extend source guards so `lifecycle/cache.rs` cannot import durable service
   assembly modules.
4. Update the L8 porting log after implementation.

## Edge Cases

1. non-cache plan passed to cache open;
2. invalid lifecycle config;
3. capability rejection from memory-like backend missing a cache requirement;
4. capability validation side-effect counter proves only `capabilities()` ran;
5. initial branch generation zero;
6. duplicate initial branch registration cannot occur during open;
7. cache commit before open rejected;
8. cache commit after close rejected;
9. ordinary read after close rejected;
10. close called twice returns idempotent closed outcome;
11. cache open ignores preexisting durable-looking backend objects;
12. cache reopen creates empty branch state and `CommitVersion::ZERO` visibility;
13. durable-mode batch sent to cache executor rejects through L7;
14. cache applied-not-visible gate behavior remains owned by L7 and is not
    suppressed by L8D.

## Deferred

1. Durable local open/create and service assembly: L8E.
2. Recovery orchestration: L8F.
3. L7 replay/bootstrap from durable facts: L8G.
4. Maintenance executor: L8H.
5. Flush/table publication/checkpoint: L8I-L8J.
6. Retention/quarantine/repair: L8L-L8M.
7. Full durable close ordering and timeout retry: later close slice.
8. Public L9 storage API wrappers.
9. Multi-branch public lifecycle workflows.
10. Cache persistence mode, if ever added.

## Verification Commands

Minimum commands after implementation:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::cache
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --all-features --locked --lib lifecycle
cargo test -p strata-storage-next --all-features --locked --test lifecycle_properties
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```
