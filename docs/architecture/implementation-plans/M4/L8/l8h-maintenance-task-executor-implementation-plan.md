# L8H Implementation Plan: Maintenance Task Executor

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-test-plan.md`

Predecessor:
`docs/architecture/implementation-plans/M4/L8/l8g-commit-bootstrap-recovery-health-implementation-plan.md`

## Objective

Add the storage-internal maintenance task model and deterministic executor that
later L8 slices use for flush, checkpoint, compaction, materialization,
retention, quarantine, repair, and close drain.

L8H is infrastructure only. It makes maintenance explicit, bounded, ordered,
coalesced, drainable, cancellable, observable, and testable without background
thread nondeterminism. It must not implement the durable behavior of flush,
checkpoint, WAL truncation, compaction, retention, quarantine, purge, repair, or
close. Those tasks plug into the executor in L8I-L8N.

## Inputs

1. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
2. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8a-lifecycle-scaffold-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L8/l8b-lifecycle-state-open-plan-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L8/l8d-cache-open-close-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L8/l8g-commit-bootstrap-recovery-health-implementation-plan.md`
8. `crates/storage-next/src/lifecycle/config.rs`
9. `crates/storage-next/src/lifecycle/facts.rs`
10. `crates/storage-next/src/lifecycle/outcome.rs`
11. `crates/storage-next/src/lifecycle/state.rs`
12. `crates/storage-next/src/lifecycle/cache.rs`
13. `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
14. `crates/storage-next/src/lifecycle/health.rs`
15. `crates/storage-next/src/testkit/lifecycle/`

## Existing-Code Source Map

| Current file | L8H evidence | L8H action |
|---|---|---|
| `lifecycle/facts.rs` | Defines `MaintenanceTaskKind`, `LifecycleStats`, and lifecycle operation vocabulary. | Keep these as the shared vocabulary; add task ids, priorities, queue facts, and executor counters in L8-owned terms. |
| `lifecycle/outcome.rs` | Defines `MaintenanceOutcome` and status shell. | Extend only as needed for executor facts: sequence, skipped/deferred reason, canceled/drained counts, and health debt. |
| `lifecycle/config.rs` | Defines `max_maintenance_queue_depth`. | Enforce this limit in the executor; do not leave it as an unused configuration knob. |
| `lifecycle/state.rs` | Allows `OrdinaryMaintenance` only in `Open` and `CloseRequiredDrain` only in `Closing`. | Use the state machine for every public-internal executor entry point. |
| `lifecycle/cache.rs` | Cache open and close currently have no maintenance queue. | Attach the same executor shape to cache runtime without allowing durable tasks to make durable claims. |
| `lifecycle/durable/bootstrap.rs` | Durable open currently reports maintenance readiness conservatively. | Attach executor readiness after successful bootstrap, but keep task execution capability-specific. |
| Old engine background scheduler | Provides evidence for coalescing, drain, cancellation, and metrics. | Port behavior shape only; do not import engine task types or product scheduling policy. |

## Scope

L8H implements:

1. a crate-private maintenance module, likely
   `crates/storage-next/src/lifecycle/maintenance.rs`;
2. `MaintenanceTask` with deterministic task id, kind, priority, scope,
   coalescing key, close policy, and fault-injection tag;
3. `MaintenanceTaskPriority` with stable ordering;
4. `MaintenanceTaskScope` that can distinguish global, branch, WAL,
   checkpoint, quarantine, retention, compaction, and materialization work
   without carrying product DTOs;
5. `MaintenanceTaskPolicy` for coalescing, drain-required, cancelable, and
   ordinary background work;
6. a deterministic single-threaded executor with no wall-clock sleeps and no
   background thread requirement;
7. queue admission that uses `LifecycleStateMachine::admit`:
   - ordinary enqueue/run requires `Open`;
   - drain-only execution requires `Closing`;
   - health/status queries are admitted in every lifecycle state;
8. queue-depth enforcement from `LifecycleConfig::max_maintenance_queue_depth`;
9. deterministic priority ordering, then FIFO sequence ordering for equal
   priority;
10. coalescing of redundant task requests by documented coalesce key;
11. explicit non-coalescing for tasks that must run independently;
12. cancellation of not-yet-started cancelable tasks;
13. drain of drain-required tasks for close integration;
14. active-task tracking so close and health status can observe in-flight work;
15. metrics for enqueued, coalesced, started, completed, deferred, failed,
   canceled, drained, and queue-full events;
16. `MaintenanceOutcome` production for completed, deferred, failed, and
   canceled work;
17. typed `LifecycleError::MaintenanceFailed` or a narrower error variant if
   existing vocabulary is too coarse;
18. maintenance health debt recording as `RecoveryHealth::Degraded` telemetry
   for failed non-critical tasks, without claiming data loss;
19. deterministic task-boundary fault hooks for tests:
   - before enqueue;
   - after enqueue before start;
   - at task start;
   - after task handler returns;
   - during drain;
20. cache and durable runtime integration that exposes internal maintenance
   methods and status while preserving the existing `pub(crate)` boundary;
21. `StorageOpenOutcome::maintenance_ready` semantics based on executor
   availability and recovery health;
22. testkit counters for generated maintenance scripts;
23. source guards that ensure lower layers do not import lifecycle maintenance
   and L8 does not import engine/product/background scheduler objects;
24. a porting-log entry after implementation.

L8H does not implement:

1. L6 frozen-state flush;
2. L5 table artifact building;
3. L4 table publication;
4. checkpoint object publication;
5. manifest watermark updates;
6. WAL truncation or retention proof construction;
7. compaction candidate selection or table compaction;
8. inherited-layer materialization mechanics;
9. snapshot pruning;
10. quarantine inventory mutation;
11. object purge or repair;
12. durable close/shutdown ordering;
13. product/user-facing maintenance commands;
14. wall-clock background loops;
15. async runtime integration.

Later slices plug concrete handlers into the executor:

| Later slice | Plugs into L8H |
|---|---|
| `L8I` | Flush frozen state and table publication handlers. |
| `L8J` | Checkpoint, flush watermark, and WAL truncation handlers. |
| `L8K` | Compaction and materialization scheduling handlers. |
| `L8L` | Snapshot pruning and retention-proof handlers. |
| `L8M` | Quarantine, reclaim, purge, and repair handlers. |
| `L8N` | Close drain/cancel/shutdown integration. |

## Type Surface

Names may change during implementation, but the responsibilities should remain
stable.

```rust
pub(crate) struct MaintenanceTask {
    id: MaintenanceTaskId,
    kind: MaintenanceTaskKind,
    priority: MaintenanceTaskPriority,
    scope: MaintenanceTaskScope,
    policy: MaintenanceTaskPolicy,
    coalesce_key: Option<MaintenanceCoalesceKey>,
}

pub(crate) struct MaintenanceTaskId(u64);

pub(crate) enum MaintenanceTaskPriority {
    Critical,
    High,
    Normal,
    Low,
}

pub(crate) enum MaintenanceTaskScope {
    Global,
    Branch(BranchId),
    Wal,
    Checkpoint,
    Quarantine,
    Retention,
    TableLevel { branch_id: BranchId, level: u8 },
    InheritedLayer { branch_id: BranchId },
}

pub(crate) enum MaintenanceTaskPolicy {
    Ordinary,
    Coalesce,
    DrainBeforeClose,
    CancelBeforeClose,
}

pub(crate) struct MaintenanceCoalesceKey {
    kind: MaintenanceTaskKind,
    scope: MaintenanceTaskScope,
}

pub(crate) struct LifecycleMaintenanceExecutor {
    next_id: u64,
    max_queue_depth: usize,
    queue: Vec<MaintenanceTask>,
    active: Option<MaintenanceTask>,
    stats: LifecycleMaintenanceStats,
}

pub(crate) trait MaintenanceTaskRunner {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome>;
}
```

The executor should stay single-threaded and deterministic in L8H. A future
threaded executor may wrap the same task queue and runner contract, but it must
not change task ordering semantics.

## Task Ordering

Ordering rules:

1. `Critical` runs before `High`.
2. `High` runs before `Normal`.
3. `Normal` runs before `Low`.
4. Equal priority preserves original enqueue sequence.
5. Coalesced tasks keep the earliest sequence id and may update only
   explicitly documented mergeable facts.
6. A task that is already active is never coalesced away.
7. A non-coalescing task never merges with any pending task, even when kind and
   scope match.

This ordering must be implemented with stable comparisons over explicit fields,
not by relying on map iteration order.

## Coalescing Policy

Initial V1 coalescing:

| Task kind | Coalescing key | Rationale |
|---|---|---|
| `Flush` | kind + branch scope | Multiple flush requests for the same branch can collapse before execution. |
| `Checkpoint` | kind + global/checkpoint scope | One checkpoint request is enough while pending. |
| `WalTruncation` | kind + WAL scope | Later L8J will recompute proof when it runs. |
| `Compaction` | kind + branch/level scope | Later L8K owns candidate selection. |
| `Materialization` | kind + branch/inherited-layer scope | Later L8K owns source facts. |
| `SnapshotPruning` | kind + retention scope | Later L8L recomputes proof when it runs. |
| `Retention` | kind + retention scope | Later L8L recomputes proof when it runs. |
| `Quarantine` | kind + quarantine scope | Later L8M recomputes inventory when it runs. |
| `Purge` | none by default | Purge should be proof-specific once implemented. |
| `Repair` | none by default | Repair should preserve each diagnostic request. |
| `HealthCollection` | kind + global scope | Multiple pending health snapshots can collapse. |

Coalescing should return an outcome/fact that names the existing task id. It
must not silently pretend the duplicate was enqueued.

## Admission And Readiness

Executor entry points must use lifecycle admission:

1. `enqueue_ordinary` requires `LifecycleOperationKind::OrdinaryMaintenance`.
2. `run_next_ordinary` requires `LifecycleOperationKind::OrdinaryMaintenance`.
3. `drain_for_close` requires `LifecycleOperationKind::CloseRequiredDrain`.
4. `cancel_pending_for_close` requires `LifecycleOperationKind::CloseRequiredDrain`.
5. `status` and `stats` require `LifecycleOperationKind::HealthQuery`.

`StorageOpenOutcome::maintenance_ready` should mean:

1. the runtime has an executor attached;
2. ordinary maintenance admission is possible once the lifecycle reaches
   `Open`;
3. recovery health is not `Failed`;
4. unresolved durable state does not require recovery before ordinary
   maintenance;
5. individual task kinds may still defer because their concrete L8I-L8M
   handler has not landed or because proof is incomplete.

Conservative policy:

1. cache mode may report readiness once the executor exists, but durable-only
   task handlers must return deferred/not-applicable outcomes;
2. durable mode may report readiness after L8G opens successfully when recovery
   health is `Healthy` or telemetry-only degraded;
3. data-loss or policy-downgrade recovery should keep readiness false until
   retention/quarantine slices decide which tasks are safe.

## Execution Protocol

Running a task follows this sequence:

1. admit operation through lifecycle state;
2. choose the next task by deterministic ordering;
3. mark it active and remove it from the pending queue;
4. fire a task-start fault hook;
5. call the task runner;
6. fire an after-run fault hook;
7. clear active task;
8. update metrics and return a `MaintenanceOutcome`.

If the runner returns:

1. `Completed`: count completed and preserve effects;
2. `Deferred`: count deferred and keep no active state;
3. `Failed`: count failed and attach telemetry health debt;
4. `Err`: count failed, clear active, and return typed lifecycle error with
   source chain when one exists.

No task is allowed to leave the executor with `active.is_some()` after a normal
return or typed error.

## Drain And Cancel

Close integration is prepared in L8H but fully consumed by L8N.

Rules:

1. drain runs only tasks whose policy requires drain-before-close;
2. cancel removes only not-yet-started tasks whose policy allows cancellation;
3. active tasks cannot be canceled by L8H; L8N decides whether to wait or return
   timeout;
4. close-required drain must not start ordinary cancelable background tasks;
5. drain and cancel both report deterministic counts;
6. repeated drain/cancel is idempotent over an empty queue.

## Runtime Integration

Cache runtime:

1. stores a `LifecycleMaintenanceExecutor`;
2. exposes crate-private enqueue/run/status methods;
3. rejects durable-only tasks through the handler layer, not by importing L4;
4. keeps cache mode free of WAL, manifest, table-object, checkpoint, and
   quarantine service imports.

Durable runtime:

1. stores the same executor after L8G bootstrap succeeds;
2. exposes crate-private enqueue/run/status methods;
3. routes concrete durable task handlers to L8I-L8M later;
4. keeps L8H from directly calling L4/L5/L6 mutation APIs except through test
   doubles for handler contracts.

## Source Guard Policy

L8H source guards should enforce:

1. `src/lifecycle/maintenance.rs` does not import engine modules, product DTOs,
   StrataHub modules, follower paths, raw filesystem APIs, or environment APIs;
2. cache runtime maintenance integration does not import durable services;
3. lower layers do not import `crate::lifecycle`;
4. L8H tests do not assert plan-document links as implementation behavior;
5. executor tests use deterministic task runners, not sleeps or real threads.

## Implementation Steps

### L8H-A: Maintenance Types

1. Add the maintenance module.
2. Add task id, priority, scope, policy, coalescing key, executor stats, and
   task status facts.
3. Add validation constructors that reject invalid queue depth, invalid scope,
   missing branch id where needed, and empty diagnostic reasons.

### L8H-B: Deterministic Queue

1. Implement enqueue with queue-depth check.
2. Implement stable priority/FIFO selection.
3. Implement coalescing by key.
4. Implement cancel-pending and drain-pending helpers.

### L8H-C: Runner Protocol

1. Add the runner trait or equivalent explicit callback.
2. Implement `run_next`.
3. Guarantee active-task cleanup on every return path.
4. Map runner errors to lifecycle maintenance errors while preserving source
   chains where available.

### L8H-D: Runtime Integration

1. Attach executor to cache runtime.
2. Attach executor to durable runtime after bootstrap.
3. Add crate-private methods for enqueue, run-next, drain, cancel, and status.
4. Update open outcome readiness according to the conservative readiness policy.

### L8H-E: Testkit And Source Guards

1. Add testkit maintenance script helpers and counters.
2. Add direct unit tests in `src/lifecycle/tests/maintenance.rs`.
3. Add integration tests in `tests/lifecycle_maintenance.rs`.
4. Extend source guards for maintenance executor boundaries.
5. Update the L8 porting log after implementation.

## Edge Cases

The implementation must explicitly handle:

1. queue depth exactly at capacity;
2. queue depth one;
3. duplicate task while original is pending;
4. duplicate task while original is active;
5. same priority ordering across many tasks;
6. cancellation of an empty queue;
7. drain of an empty queue;
8. task failure clearing the active slot;
9. fault injected at task start;
10. fault injected after runner success;
11. runner returning deferred;
12. runner returning failed outcome without returning `Err`;
13. lifecycle state changing to `Closing` before ordinary run;
14. cache mode receiving durable task kinds;
15. data-loss recovery producing readiness false;
16. telemetry-degraded recovery producing readiness true if executor is present;
17. stats saturating or rejecting overflow rather than wrapping silently.

## Deferred To Later Slices

1. Flush implementation: L8I.
2. Checkpoint and WAL truncation implementation: L8J.
3. Compaction/materialization implementation: L8K.
4. Retention and snapshot pruning implementation: L8L.
5. Quarantine, purge, repair implementation: L8M.
6. Full close drain and timeout policy: L8N.
7. Process-level crash tests and fuzz target inventory: L8O/L8P.
8. Threaded/background executor wrapper: post-V1 unless deterministic executor
   is insufficient.

## Verification Commands

After implementation:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::maintenance
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

When `cargo-hack` is available:

```bash
cargo hack check -p strata-storage-next --feature-powerset --depth 2
```

## Exit Criteria

L8H is complete when:

1. maintenance tasks have stable ids, priority, scope, policy, and coalescing
   keys;
2. queue depth is enforced;
3. priority and equal-priority ordering are deterministic;
4. duplicate task coalescing is observable and does not lie about enqueue;
5. drain and cancel behave deterministically;
6. active task state is cleared on success, deferred, failure, and typed error;
7. cache and durable runtimes expose crate-private executor status and control
   hooks;
8. readiness semantics are documented and tested;
9. source guards preserve L8 boundaries;
10. L8I-L8N can plug concrete task handlers into the executor without changing
    queue semantics.
