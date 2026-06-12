# M4P-L8E Implementation Plan: Background Maintenance Executor Parity

Status: draft

Parent implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8-lifecycle-maintenance-parity-implementation-plan.md`

Predecessor plans:

1. `docs/architecture/implementation-plans/M4P/m4p-l8-automatic-maintenance-scheduling-followup.md`
2. `docs/architecture/implementation-plans/M4P/m4p-l8b-lifecycle-maintenance-followup-implementation-plan.md`

Follow-up test plan:
`docs/architecture/implementation-plans/M4P/m4p-l8e-background-maintenance-executor-test-plan.md`

Port source:
`crates/engine/src/background.rs`

## Objective

Port the old engine background scheduler into storage-next and make public
storage-next runtimes drain lifecycle maintenance in the background.

L8 and L8B restored the maintenance queue, pressure classification, scored
flush/compaction/materialization tasks, chaining, resource policy, and
diagnostics. The 5M benchmark still proves a major gap: public runtimes use
deterministic inline maintenance to keep source shape healthy, so sustained
loads pay compaction cost on the write path.

L8E closes that gap. This is not a measure-first gate and not an optional
thread decision. The background executor is required for L9 scale closeout.

## Scope Summary

| Group | Required Work | Exit Gate |
| --- | --- | --- |
| L8E-A. Scheduler Port | Port `crates/engine/src/background.rs` into storage-next with equivalent semantics and tests. | Ported scheduler passes old engine race, shutdown, drain, priority, and panic tests. |
| L8E-B. Runtime Ownership | Add background-capable runtime ownership for cache and durable public opens. | Public cache and owned durable local opens can run maintenance on worker threads. |
| L8E-C. Wake And Drain Policy | Wake background workers from lifecycle enqueue/coalesce and pressure transitions. | Queued maintenance drains without public `run_next_maintenance` or inline post-commit calls. |
| L8E-D. Nonblocking Maintenance Execution | Split long flush/compaction/materialization work into snapshot/build/publish phases. | Public commits do not block on full compaction build/merge work except documented close/blocking-pressure cases. |
| L8E-E. Close And Failure Integration | Shut down workers through lifecycle close, drain required tasks, and surface worker failures. | Close is deterministic; no accepted background task is lost during shutdown. |
| L8E-F. Diagnostics And Benchmark Gate | Expose background scheduler metrics and rerun 100K/1M/5M/10M. | 5M/10M reach point-read measurement without a large inline or final fixed-point compaction cliff. |

## Existing Baseline

Assume the following behavior exists before L8E:

1. `LifecycleMaintenanceExecutor` owns the lifecycle maintenance queue and
   task facts.
2. Post-commit scheduling can enqueue/coalesce flush, compaction,
   materialization, checkpoint, retention, purge, and quarantine tasks.
3. Scored table rewrite tasks can chain until branch source shape is healthy.
4. `LifecycleMaintenanceSchedulingPolicy::DeterministicInline` can drive one
   suggested task after commit or before urgent admission.
5. `LifecycleMaintenanceSchedulingPolicy::EvaluateAndEnqueue` enqueues work
   but does not autonomously drain it.
6. Public runtime opens currently select deterministic inline maintenance to
   avoid frozen-budget failure during benchmarks.

If any of these regress while implementing L8E, restore the invariant before
continuing.

## Mandatory Design Decisions

1. **Scheduler primitive**
   - Port `crates/engine/src/background.rs`.
   - Preserve priority ordering, FIFO within a priority, queue-depth
     backpressure, drain, idempotent shutdown, panic containment, and the
     submit/shutdown TOCTOU fix.
   - Do not write a new scheduler from scratch.
2. **Public runtime default**
   - Public cache and owned durable local opens use background maintenance by
     default.
   - Deterministic inline remains only for deterministic unit tests and
     explicit diagnostic modes.
   - Evaluate-and-enqueue remains for lower-level queue tests and explicit
     manual maintenance scenarios.
3. **No global-lock compaction tax**
   - The worker must not hold a global runtime mutex for an entire flush,
     compaction, materialization, checkpoint, or retention pass.
   - Long maintenance work must be split into:
     1. short locked snapshot/admission phase;
     2. unlocked build/merge/IO phase;
     3. short locked publication/accounting phase.
   - A background thread that merely serializes the same full compaction behind
     a mutex does not satisfy L8E.
4. **Lifecycle queue remains authoritative**
   - The existing `LifecycleMaintenanceExecutor` remains the source of truth for
     maintenance task identity, coalescing, active task facts, close policy, and
     outcome stats.
   - The ported background scheduler executes wake/drain closures; it does not
     replace lifecycle task semantics with a second independent maintenance
     queue.
5. **Worker count**
   - V1 uses one lifecycle maintenance worker per runtime.
   - The ported scheduler may support multiple threads, but lifecycle
     maintenance runs single-worker until branch/table publication semantics
     are proven for parallel maintenance.
6. **Durable ownership**
   - Owned public durable local opens must use an owned or clonable backend
     handle that can cross a worker thread.
   - Borrowed backend opens remain manual/deterministic unless they are
     converted to owned thread-safe handles.
7. **Close**
   - Close stops accepting new background maintenance, wakes the worker,
     drains close-required tasks, joins the worker, and returns typed close
     facts.
   - No background task accepted before close may disappear silently.
8. **Admission**
   - Background mode removes normal post-commit inline maintenance from the
     hot write path.
   - Block severity may still reject or drive bounded recovery according to the
     existing typed pressure contract.
   - Urgent severity in background mode wakes workers and records accepted-
     under-pressure facts; it must not run full maintenance inline.

## Non-Goals

L8E must not:

1. invent a replacement for `crates/engine/src/background.rs`;
2. add benchmark-only maintenance shortcuts;
3. hide the 5M/10M cliff by increasing benchmark timeout;
4. introduce a product retry UI;
5. change L5 row merge semantics;
6. change L6 branch install correctness rules;
7. implement parallel same-branch maintenance;
8. implement retention policy semantics beyond running already-queued retention
   tasks in the background;
9. move commit-runtime L7 into background threads.

## L8E-A. Port The Engine Background Scheduler

Goal: make the old background scheduler available inside storage-next with
behavioral parity.

Tasks:

1. Copy the scheduler core from `crates/engine/src/background.rs` into
   storage-next, under `crates/storage-next/src/lifecycle/background.rs` or a
   closely scoped equivalent module.
2. Preserve these public/internal types with storage-next naming:
   - `TaskPriority`;
   - `BackpressureError` or a lifecycle-specific wrapper;
   - `SchedulerStats`;
   - `BackgroundScheduler`.
3. Preserve these internals:
   - `BinaryHeap<TaskEnvelope>`;
   - `parking_lot::Mutex` and `parking_lot::Condvar`;
   - atomic shutdown flag;
   - atomic queue depth, active task count, task completion count, sequence;
   - `ActiveTaskGuard`;
   - `catch_unwind` around task execution;
   - lost-wakeup prevention around drain and shutdown notifications;
   - lock-held authoritative shutdown check in `submit`.
4. Rename worker threads to storage-next lifecycle names:
   `strata-storage-maint-<runtime-kind>-<n>`.
5. Keep the port self-contained. Storage-next must not depend on
   `strata-engine` to get the scheduler.
6. Add source comments that identify `crates/engine/src/background.rs` as the
   port source and explain any intentional storage-next divergence.

Exit gates:

1. All old scheduler behavior tests pass in storage-next.
2. The submit/shutdown TOCTOU test is preserved.
3. A panic in one task cannot hang drain or kill the worker pool.

## L8E-B. Add Background-Capable Runtime Ownership

Goal: allow a worker thread to run maintenance while public API calls continue
to operate through a safe runtime handle.

Tasks:

1. Introduce a storage-next background runtime handle that owns:
   - the lifecycle runtime state;
   - the ported `BackgroundScheduler`;
   - a wake/drain controller;
   - close/shutdown state.
2. Support both cache and durable local runtimes.
3. Convert public cache and owned durable local opens to background-capable
   runtime variants.
4. Keep borrowed-backend durable opens explicit:
   - either reject background mode for borrowed handles with a typed config
     error;
   - or convert them to owned `Arc<dyn Backend>` handles before starting a
     worker.
5. Add `LifecycleMaintenanceSchedulingPolicy::Background`.
6. Select `Background` in the product-facing `StorageOpenPlan` for public
   cache and owned durable local opens.
7. Preserve `DeterministicInline` for unit tests that require exact task
   ordering.
8. Preserve `EvaluateAndEnqueue` for tests that inspect queued state without
   worker drain.

Exit gates:

1. Public `open_cache`, `open_ephemeral`, and owned `open_durable_local`
   report `Background` scheduling policy.
2. Borrowed durable backend behavior is explicit and tested.
3. Existing deterministic tests can still opt out of background execution.

## L8E-C. Wake And Drain Lifecycle Maintenance

Goal: connect the existing lifecycle maintenance queue to the ported scheduler
without replacing lifecycle task semantics.

Tasks:

1. Add a `LifecycleBackgroundMaintenanceController` that can be notified after:
   - successful maintenance enqueue;
   - coalesced enqueue when pending work exists;
   - post-commit pressure scheduling;
   - urgent accepted-under-pressure admission;
   - explicit maintenance enqueue API calls;
   - branch coverage enqueue;
   - chain resubmission.
2. Coalesce wake submissions so repeated enqueue calls do not flood the
   scheduler with duplicate drain closures.
3. Map lifecycle work to old scheduler priorities:
   - High: flush drain, checkpoint/flush-watermark work needed for budgets or
     close-required drain;
   - Normal: compaction/materialization table rewrite;
   - Low: health collection, retention, purge, quarantine repair unless a
     task's close policy or pressure reason upgrades it.
4. Each background wake runs a bounded drain round:
   - run at most `max_tasks_per_wake`;
   - stop after `max_runtime_per_wake`;
   - stop immediately when close enters close-required drain;
   - resubmit itself if pending work remains after the round.
5. Record stale wake no-ops when a wake finds no eligible lifecycle task.
6. Ensure scheduling remains deterministic under a single worker:
   - lifecycle queue order and coalescing remain authoritative;
   - scheduler priority only decides which wake class runs first.

Exit gates:

1. Queued post-commit maintenance drains without calling public
   `run_next_maintenance`.
2. Coalesced pressure does not create unbounded background scheduler depth.
3. Chain resubmission wakes the worker until source shape is healthy.

## L8E-D. Split Long Maintenance Work

Goal: ensure background execution actually removes compaction tax from the
foreground write path.

Tasks:

1. Audit all `MaintenanceTaskRunner` implementations for long critical
   sections.
2. Split flush execution:
   - locked snapshot/rotation proof;
   - unlocked table build and durable write;
   - locked publish, manifest/watermark update, budget accounting.
3. Split compaction execution:
   - locked candidate snapshot and publication preflight;
   - unlocked L5 merge/build;
   - locked branch/table manifest publication and task outcome accounting.
4. Split materialization execution with the same snapshot/build/publish shape.
5. Split checkpoint/WAL growth work enough that foreground commits are blocked
   only for metadata publication and WAL service synchronization windows.
6. Add a foreground admission counter for time waiting on background-owned
   critical sections.
7. Add a background critical-section counter for:
   - snapshot lock time;
   - publish lock time;
   - unlocked build time;
   - total task time.
8. Keep branch and table publication proofs intact. If a candidate snapshot
   becomes stale before publish, the task must complete as deferred/stale and
   resubmit current pressure rather than publishing stale output.

Exit gates:

1. A foreground commit loop can continue while a background compaction performs
   the unlocked merge/build phase.
2. A stale compaction candidate never publishes over newer branch state.
3. Source shape still converges under sustained writes.

## L8E-E. Close, Shutdown, And Failure Integration

Goal: make background execution deterministic across close, drop, failures, and
panics.

Tasks:

1. Add lifecycle close integration:
   - stop accepting ordinary background wake submissions;
   - wake worker;
   - drain active close-required task if any;
   - drain queued close-required tasks;
   - cancel ordinary tasks according to existing close policy;
   - shut down and join the ported scheduler;
   - return close facts that include background stats.
2. Add drop behavior:
   - dropping an open public runtime must request background shutdown;
   - if shutdown cannot complete within the configured drop policy, record
     health debt and detach only if the plan explicitly allows it.
3. Convert scheduler backpressure to lifecycle maintenance facts:
   - queue full;
   - wake rejected after shutdown;
   - worker panic observed;
   - stale wake no-op;
   - task failure.
4. Preserve the old scheduler guarantee: every accepted background wake either
   runs, drains during shutdown, or is reported as canceled by close policy.
5. Ensure worker panics do not poison lifecycle close or hang drain.

Exit gates:

1. Close cannot hang indefinitely on an idle worker, a panicking task, or an
   empty background queue.
2. Submit-after-shutdown is rejected and counted.
3. Accepted background wakes are not lost in close races.

## L8E-F. Diagnostics, Configuration, And Benchmark Closeout

Goal: expose enough facts to prove background execution removes the inline
maintenance cliff.

Tasks:

1. Extend diagnostics with:
   - background worker count;
   - scheduler queue depth;
   - active background tasks;
   - accepted wake submissions;
   - coalesced wake submissions;
   - rejected wake submissions;
   - stale wake no-ops;
   - tasks completed by background;
   - worker panics;
   - shutdown drain count;
   - foreground wait time on maintenance critical sections.
2. Extend perf trace with the same fields.
3. Add `StorageOpenOptions` or `LifecycleConfig` knobs for:
   - scheduling mode: `Disabled`, `EvaluateAndEnqueue`,
     `DeterministicInline`, `Background`;
   - background worker count, default 1;
   - background scheduler queue depth;
   - max tasks per wake;
   - max runtime per wake.
4. Update benchmark diagnostics so load output separates:
   - foreground commit time;
   - foreground wait on background critical sections;
   - background maintenance task time;
   - final diagnostic drain time.
5. Run the L9 scale benchmark:

```bash
cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l9-scale -- \
  --scales 100k,1m,5m,10m \
  --engines cache,standard \
  --workloads load-seq,point-latest,point-throughput \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000 \
  --progress
```

Exit gates:

1. 100K/1M/5M/10M complete for cache and standard.
2. 5M and 10M reach point-read measurement.
3. Load does not rely on explicit final fixed-point drain to make source shape
   readable.
4. `automatic_maintenance_ns` is reported as background time, not foreground
   commit time.
5. Foreground wait on background critical sections is bounded and materially
   smaller than the previous inline maintenance cost.

## Stop Conditions

Stop and revise this plan only if:

1. `crates/engine/src/background.rs` cannot be ported because storage-next
   cannot use `parking_lot` or `std::thread`;
2. L5/L6 cannot expose snapshot/build/publish boundaries without changing
   correctness-critical public APIs;
3. durable owned backend handles cannot be made `Send + Sync + 'static`;
4. branch publication proofs cannot detect stale background candidates;
5. close cannot join workers without violating already-landed close contracts.

Any stop condition must produce a new implementation plan before L8C or L8D
continues.

## Verification Commands

Focused commands:

```bash
cargo test -p strata-storage-next lifecycle_background --all-features --locked
cargo test -p strata-storage-next api_background_maintenance --all-features --locked
cargo test -p strata-storage-next lifecycle_source_guard --all-features --locked
cargo clippy -p strata-storage-next --lib --all-features --locked -- -D warnings
```

Benchmark command:

```bash
cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l9-scale -- \
  --scales 100k,1m,5m,10m \
  --engines cache,standard \
  --workloads load-seq,point-latest,point-throughput \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000 \
  --progress
```

Full closeout command:

```bash
cargo fmt --all
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo test -p strata-storage-next --all-targets --all-features --locked
```
