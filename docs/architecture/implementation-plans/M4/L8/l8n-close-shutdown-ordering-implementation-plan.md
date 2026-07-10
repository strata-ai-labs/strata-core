# L8N Implementation Plan: Close And Shutdown Ordering

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8n-close-shutdown-ordering-test-plan.md`

Predecessors:

1. `docs/architecture/implementation-plans/M4/L8/l8d-cache-open-close-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/L8/l8i-flush-table-publication-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8j-checkpoint-watermark-wal-truncation-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L8/l8l-retention-proof-snapshot-pruning-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L8/l8m-quarantine-reclaim-repair-implementation-plan.md`

## Objective

Implement storage-internal close orchestration for cache and durable-local
runtimes.

L8N turns the close vocabulary, state-machine transitions, maintenance
drain/cancel hooks, commit-quiesce guard, and durable service handles into a
single ordered shutdown protocol:

1. reject new storage mutations and ordinary maintenance;
2. cancel work that is safe to cancel;
3. drain work that must complete before close;
4. acquire commit-runtime quiesce;
5. stop durable writer/sync loops represented inside storage-next;
6. flush or close WAL state when durability requires it;
7. persist final storage health facts that belong to L8;
8. release storage-owned backend guards exactly once;
9. transition to `Closed`;
10. make retries and double close deterministic and idempotent.

The key invariant: a close outcome must never claim the storage runtime is
closed and clean while required maintenance, commit quiescence, durable sync, or
guard release remains unresolved.

## Inputs

1. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
2. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
3. `docs/architecture/storage/l7-commit-runtime.md`
4. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
5. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
6. `docs/architecture/implementation-plans/M4/L8/l8d-cache-open-close-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-implementation-plan.md`
8. `crates/storage-next/src/lifecycle/cache.rs`
9. `crates/storage-next/src/lifecycle/durable.rs`
10. `crates/storage-next/src/lifecycle/durable/maintenance.rs`
11. `crates/storage-next/src/lifecycle/maintenance.rs`
12. `crates/storage-next/src/lifecycle/state.rs`
13. `crates/storage-next/src/lifecycle/outcome.rs`
14. `crates/storage-next/src/lifecycle/facts.rs`
15. `crates/storage-next/src/lifecycle/error.rs`
16. `crates/storage-next/src/commit/guard.rs`
17. `crates/storage-next/src/service/wal.rs`
18. `crates/storage-next/src/service/manifest.rs`
19. `crates/engine/src/database/lifecycle.rs`
20. `crates/engine/src/background.rs`
21. `crates/storage/src/durability/checkpoint_runtime.rs`
22. `crates/storage/src/durability/recovery_bootstrap.rs`
23. `crates/storage/src/durability/compaction/wal_only.rs`

## Existing-Code Source Map

| Current file | Evidence | L8N action |
|---|---|---|
| `lifecycle/state.rs` | Close transitions and `LifecycleCloseFact::{Requested, Complete, RetryPending, AlreadyClosed}` already exist. | Use the state machine for every close entry point. Do not hand-roll state flags. |
| `lifecycle/outcome.rs` | `CloseOutcome`, `CloseOutcomeEffects`, and `CloseOutcomeStatus` carry successful/idempotent close phase, effects, and stats. Failure source chains stay on `LifecycleError` through the existing `LifecycleResult<CloseOutcome>` API. | Fill this envelope with durable close facts for successful closes instead of adding a parallel outcome type. |
| `lifecycle/cache.rs` | Cache close is already idempotent, cancels cancelable work, rejects drain-required leftovers, and avoids durable services. | Keep cache close volatile and align any new outcome fields with durable close without adding durable side effects. |
| `lifecycle/maintenance.rs` | Executor has close policies, `cancel_pending_for_close`, and `drain_for_close`. | Consume these hooks during close. The executor stays generic and must not know durable service details. |
| `lifecycle/durable.rs` | Durable shell owns service handles and the writer guard before bootstrap; durable runtime owns them after recovery. | Add close orchestration around this ownership without moving maintenance code into bootstrap. |
| `lifecycle/durable/maintenance.rs` | Durable runtime dispatches concrete maintenance handlers. | Add close-specific durable methods in a sibling close module so maintenance and bootstrap stay separated. |
| `commit/guard.rs` | `CommitBranchGuardSet::try_begin_quiesce` blocks new branch guards and fails while active guards exist. | Use this as the commit quiesce boundary; do not inspect commit internals directly. |
| `service/wal.rs` | `WalService::close` provides the WAL close/sync boundary. | Call through this surface and map failures into lifecycle close errors with source chains. |
| `service/manifest.rs` | Manifest services publish and persist storage facts. | Persist only final L8-owned health/manifest facts that are already modeled; do not invent product-level close metadata. |

## Old-Code Porting Map

The old architecture is evidence for shutdown ordering, not an API template.

| Old source | Behavior to preserve | Storage-next rewrite decision | Test focus |
|---|---|---|---|
| `crates/engine/src/database/lifecycle.rs` | Close first gates new work, waits for background work, flushes durable state, and releases locks idempotently. | Preserve the storage-owned ordering and facts. Product freeze hooks, IPC, registry release, and user-facing mapping remain outside L8. | No new commits after close starts; double close is idempotent; guard release is once. |
| `crates/engine/src/background.rs` | Background tasks can be drained or canceled according to policy, not by nondeterministic sleeps. | Use the deterministic L8H executor; no background thread is introduced in L8N. | Drain-required tasks run; cancelable tasks cancel; ordinary tasks are not started during close. |
| `crates/storage/src/durability/checkpoint_runtime.rs` | Checkpoint/WAL state is synced before a clean durable shutdown is reported. | Close invokes existing L8J/WAL service boundaries; it does not reimplement checkpoint construction. | WAL sync/close failure returns typed close failure and retry fact. |
| `crates/storage/src/durability/recovery_bootstrap.rs` | Writer lock ownership is established during open and released during shutdown. | L8N releases the storage-owned writer guard exactly once; lock release is represented in close effects. | Second runtime can acquire writer lock after close; second close does not double-release. |
| `crates/storage/src/durability/compaction/wal_only.rs` | WAL-retention and compaction work does not run after close admission. | L8N blocks ordinary maintenance and does not start new retention/compaction work while closing. | Queued ordinary tasks are canceled or left deferred according to policy, never started. |

Do not port:

1. engine primitive freeze hooks;
2. IPC/server shutdown;
3. product branch registry release;
4. raw filesystem close, fsync, rename, or directory traversal code;
5. background thread loops;
6. public database handle semantics;
7. product logs or metrics DTOs;
8. StrataHub synchronization.

## Scope

L8N implements:

1. a storage-internal close request/options shape if the existing config is not
   enough;
2. cache close alignment with the final close outcome fields;
3. durable close orchestration for shell/runtime states;
4. close admission through `LifecycleStateMachine`;
5. cancellation of cancel-before-close maintenance tasks;
6. draining of drain-before-close maintenance tasks;
7. commit quiesce through `CommitBranchGuardSet`;
8. WAL close/sync through L4 `WalService`;
9. final storage health/manifest persistence where the current L8 facts require
   it;
10. writer guard release facts and exactly-once behavior;
11. typed close errors for close rejection, timeout, sync halt, and backend IO
    failure;
12. retry semantics for timeout and partial close failure;
13. generated/testkit counters for close success, retry, idempotence, timeout,
    drain, cancel, sync failure, and guard release;
14. source guards preventing close logic from drifting into recovery bootstrap
    or cache durable services;
15. a porting-log entry after implementation.

L8N does not implement:

1. public L9 close API;
2. engine handle registry, IPC, or primitive freeze hooks;
3. background worker threads;
4. product/user maintenance commands;
5. new checkpoint or retention algorithms;
6. row-retention or TTL cleanup;
7. branch deletion/clear policy;
8. follower-mode shutdown;
9. multi-process distributed leases beyond the L4 writer guard;
10. crash/fuzz closeout, which belongs to L8O/L8P.

## Core Protocol

### Close Admission

Close is a lifecycle transition, not a best-effort helper.

Rules:

1. `Open -> Closing` records `LifecycleCloseFact::Requested` before side
   effects.
2. `Closing` with retry-pending facts may retry idempotent phases.
3. `Closed` returns an idempotent success with the prior final facts.
4. `New`, `Opening`, and `Recovering` reject close unless a later implementation
   explicitly adds interrupted-open recovery.
5. `Failed` rejects ordinary close unless the failure fact is a retryable close
   failure owned by L8N.
6. Close admission must happen before cancel, drain, quiesce, sync, or guard
   release.

### Stop New Work

After close starts:

1. mutating commits reject through lifecycle or commit admission;
2. ordinary reads may be rejected unless they are explicit diagnostic health
   reads;
3. ordinary maintenance enqueue/run rejects;
4. close-required drain is allowed only through the close path;
5. no new flush, checkpoint, compaction, retention, quarantine, purge, or repair
   work is started unless it is already queued as drain-required close work.

### Maintenance Drain And Cancel

Close consumes the L8H executor policies.

Rules:

1. `CancelBeforeClose` pending tasks are canceled and counted.
2. `DrainBeforeClose` pending tasks run to completion before durable sync.
3. `Ordinary` pending tasks are canceled or deferred consistently; they must not
   survive into `Closed` as runnable work.
4. Active tasks cannot be canceled by removing queue entries. Close either waits
   through a deterministic no-sleep hook or returns typed timeout.
5. Drain failure prevents clean close and records source error/health debt.
6. Drain outcomes contribute affected-object names, state changes, and stats to
   the close outcome where known.

### Commit Quiesce

Close uses the L7 guard set.

Rules:

1. quiesce begins after new lifecycle work is stopped and before durable sync;
2. active commit guards make close return typed timeout or retryable failure;
3. successful quiesce blocks new branch guards until close completes or fails
   back to a retryable state;
4. close must drop the quiesce guard on retryable failure only if the runtime is
   left able to retry safely;
5. no close path may inspect or mutate branch guard internals directly.

### Durable Sync And Final Facts

Durable close is mode-aware.

Rules:

1. cache mode never calls WAL, manifest, snapshot, table, or quarantine
   services;
2. durable standard mode closes/syncs WAL state according to current L4 service
   semantics;
3. durable always mode still closes service handles and records the close fact,
   even if every commit was already force-durable;
4. unresolved durable commit gates must prevent clean close unless recovery
   facts make the state safe;
5. final manifest or health facts are persisted only when L8 already owns a
   durable representation for them;
6. sync failure returns typed close failure with lower-layer source chain;
7. close must not truncate WAL, prune snapshots, purge quarantine, or run
   retention unless those tasks were explicitly drained before the sync phase.

### Guard Release

The writer guard is storage-owned durable state.

Rules:

1. release happens after durable sync/final fact publication succeeds;
2. release is represented in `CloseOutcomeEffects`;
3. release is exactly-once even if close is called twice;
4. if release is RAII-only, the close path must consume/drop the owner at a
   single documented point and test that a second runtime can acquire the guard;
5. a release failure from a backend that can report one maps to a typed close
   backend failure.

## Cache Mode

Cache close remains volatile.

Rules:

1. no durable service imports in `lifecycle/cache.rs`;
2. close cancels cancel-before-close work;
3. drain-required work blocks or drains according to existing executor policy;
4. close reports commits quiesced, maintenance drained/canceled, no durable
   sync, and guards released in the volatile sense;
5. double close reports idempotent close with prior-final facts;
6. cache close never claims WAL, manifest, snapshot, or writer-lock facts.

## Durable Mode

Durable close must be implemented beside durable maintenance, not in recovery
bootstrap.

Recommended file layout:

```text
crates/storage-next/src/lifecycle/durable/
  close.rs
  maintenance.rs
  bootstrap.rs
```

Responsibilities:

1. `bootstrap.rs`: recovery bootstrap and open finalization only;
2. `maintenance.rs`: open-state maintenance task dispatch;
3. `close.rs`: close phase executor, sync, final facts, and guard release.

The durable root module may re-export only the crate-private runtime methods
needed by tests and later L9 wrappers.

## Type Surface

Names can change during implementation, but responsibilities should remain
stable.

```rust
pub(crate) struct LifecycleCloseRequest {
    timeout_policy: LifecycleCloseTimeoutPolicy,
    final_sync: LifecycleFinalSyncPolicy,
}

pub(crate) enum LifecycleFinalSyncPolicy {
    Required,
    BestEffortTelemetry,
    SkipForCache,
}

pub(crate) struct LifecycleCloseProgress {
    prior_state: LifecycleState,
    current_phase: ClosePhase,
    canceled_tasks: usize,
    drained_tasks: usize,
    commits_quiesced: bool,
    durable_synced: bool,
    guards_released: bool,
    final_health: RecoveryHealth,
}
```

Existing `CloseOutcome` should remain the returned envelope unless an
implementation review proves it cannot carry the required facts.

## Error Mapping

Required close errors:

1. `invalid_state.lifecycle.close`: close rejected by lifecycle state;
2. `deadline_exceeded.lifecycle.close`: close timeout while waiting for
   maintenance drain, active commit guard, or backend sync;
3. `unavailable.lifecycle.close`: writer/sync loop halted or backend guard
   unavailable;
4. `internal.lifecycle.close`: invariant violation, such as closed state with
   pending drain-required task;
5. `data_loss.lifecycle.close` only if a lower layer proves data durability was
   lost during close.

Tests must assert on stable `code()` values and source-chain classes, not
display strings.

## Implementation Steps

### L8N-A: Close Request, Outcome, And Error Vocabulary

1. Audit `CloseOutcome`, `CloseOutcomeEffects`, `CloseOutcomeStatus`,
   `LifecycleStats`, and `LifecycleError`.
2. Add missing fields or helper constructors for:
   - prior state;
   - final state;
   - drained count;
   - canceled count;
   - quiesce result;
   - durable sync facts;
   - manifest/final-health facts;
   - guard release facts.
3. Add close-specific error variants or reason classes where the current
   vocabulary is too coarse.
4. Keep all surfaces `pub(crate)`.

### L8N-B: Cache Close Alignment

1. Preserve existing cache behavior: no durable services.
2. Fill any newly required close outcome fields.
3. Ensure cancelable and ordinary pending tasks cannot remain runnable after
   `Closed`.
4. Keep idempotent second-close behavior.

### L8N-C: Durable Close Phase Executor

1. Add a durable close module.
2. Route durable runtime close through lifecycle admission.
3. Cancel and drain maintenance through the executor.
4. Acquire commit quiesce.
5. Perform WAL close/sync through `WalService`.
6. Persist final health/manifest facts if modeled.
7. Release writer guard.
8. Transition to `Closed`.

### L8N-D: Retry And Failure Windows

1. Encode timeout as retryable close state.
2. Preserve source chains for drain, quiesce, WAL, manifest, and backend
   failures.
3. Ensure a retry does not rerun completed non-idempotent phases incorrectly.
4. Ensure a second close after success returns prior final facts.

### L8N-E: Testkit And Source Guards

1. Add generated close counters to lifecycle testkit.
2. Add source guards:
   - cache close does not import durable services;
   - durable close does not live in bootstrap;
   - close code does not import engine/product modules;
   - lower layers do not import lifecycle close.
3. Keep milestone labels out of Rust code, test names, comments, fixture bytes,
   and panic messages.

### L8N-F: Porting Log

Record:

1. old-code behavior map;
2. shipped files;
3. raw health/fact vocabulary preserved;
4. verification commands;
5. sensitivity probes;
6. deferred items.

## Exit Criteria

L8N is complete when:

1. cache close remains volatile and idempotent;
2. durable close follows the ordered protocol;
3. close blocks new commits and ordinary maintenance;
4. close drains or cancels maintenance according to policy;
5. commit quiesce is acquired or a typed timeout is returned;
6. durable sync/final facts are complete before clean close;
7. writer guard release is exactly-once;
8. failure leaves retryable state where the plan says retryable;
9. double close returns idempotent prior-final facts;
10. source guards enforce the slice boundary;
11. full verification commands in the test plan pass.

## Deferred

1. Public close API and product error mapping: L9/engine.
2. IPC/server shutdown: engine.
3. Primitive freeze hooks: engine.
4. Background thread implementation: later runtime shell if needed.
5. Crash/fuzz closeout: L8O/L8P.
6. Multi-process lease renewal/handoff beyond current writer guard: later
   object-backend durability work.
7. Branch deletion/clear close policy: later branch lifecycle work.
