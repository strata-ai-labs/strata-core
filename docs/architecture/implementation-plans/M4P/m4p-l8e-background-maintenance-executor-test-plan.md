# M4P-L8E Test Plan: Background Maintenance Executor Parity

Status: draft

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8e-background-maintenance-executor-implementation-plan.md`

Port source:
`crates/engine/src/background.rs`

Parent test plans:

1. `docs/architecture/implementation-plans/M4P/m4p-l8-lifecycle-maintenance-parity-test-plan.md`
2. `docs/architecture/implementation-plans/M4P/m4p-l8b-lifecycle-maintenance-followup-test-plan.md`

## Goal

Prove that storage-next public runtimes drain lifecycle maintenance through a
ported old-engine background scheduler, not through inline post-commit
compaction or benchmark-only fixed-point drains.

The test suite must fail if:

1. the scheduler is rewritten without preserving old-engine semantics;
2. background mode accepts work and loses it during shutdown;
3. foreground commits still pay full flush/compaction/materialization cost;
4. source shape only becomes healthy through explicit benchmark drains;
5. deterministic tests lose their ability to opt out of background execution.

## Test Matrix

| Area | Required Proof | Failure Caught |
| --- | --- | --- |
| Scheduler port parity | Old `BackgroundScheduler` semantics are preserved in storage-next. | Lost wakeups, dropped accepted tasks, panic hangs, priority regressions. |
| Runtime mode selection | Public opens use background mode; tests can request deterministic/manual modes. | Product path silently falls back to inline maintenance. |
| Wake/coalesce integration | Lifecycle enqueue/coalesce wakes workers without flooding scheduler queue. | Queue grows unbounded or tasks remain pending forever. |
| Nonblocking execution | Long task build/merge work occurs outside foreground critical sections. | Background thread merely moves the inline tax behind a mutex. |
| Close/shutdown | Close drains or cancels tasks according to lifecycle policy and joins workers. | Dropped tasks, deadlocks, submit-after-shutdown races. |
| Benchmark proof | 5M/10M reach reads with bounded source shape and bounded foreground wait. | Compaction cliff remains hidden behind final drain or timeout. |

## Scheduler Port Parity Tests

Port these tests from `crates/engine/src/background.rs` into storage-next with
only naming/module changes:

1. `submit_and_drain`
   - Submit several normal tasks.
   - Drain returns after all run.
2. `priority_ordering`
   - One worker is blocked by a barrier.
   - Queue Low, Normal, High.
   - Assert High, Normal, Low execution order.
3. `fifo_within_same_priority`
   - One worker.
   - Queue several Normal tasks.
   - Assert submission order.
4. `backpressure`
   - Queue depth is limited.
   - The first task blocks the worker.
   - Filling the queue rejects the next submit.
5. `shutdown_drains_remaining`
   - Shutdown after queued work is accepted.
   - Assert every accepted task runs before shutdown completes.
6. `drain_returns_when_idle`
   - Drain on an idle scheduler returns immediately.
7. `stats`
   - Queue depth, active task count, completed count, and worker count match
     executed work.
8. `submit_after_shutdown_rejected`
   - Shutdown rejects new submit calls.
9. `task_panic_does_not_hang_drain`
   - A panicking task is caught.
   - Drain still returns.
   - Later tasks still run.
10. `concurrent_submits`
    - Multiple submitter threads enqueue tasks.
    - Drain observes all accepted tasks complete.
11. `shutdown_is_idempotent`
    - Multiple shutdown calls do not panic or deadlock.
12. `submit_shutdown_toctou`
    - Race submit against shutdown.
    - Every submit returning `Ok(())` must execute.
13. `drain_then_submit_then_drain`
    - Drain does not kill workers.
    - Later submit/drain still works.

Mechanical source guard:

1. The storage-next scheduler module must cite
   `crates/engine/src/background.rs` as its port source.
2. The module must include an authoritative shutdown check under the queue
   lock.
3. The module must catch panics around task execution.
4. The module must use a drop guard or equivalent to decrement active task
   count on panic.

## Runtime Mode Tests

Correctness tests:

1. Public `StorageRuntime::open_cache()` reports background scheduling.
2. Public `StorageRuntime::open_ephemeral()` reports background scheduling.
3. Public owned `StorageRuntime::open_durable_local(...)` reports background
   scheduling when `localfs` is enabled.
4. Borrowed durable backend opens either:
   - report background scheduling after converting the backend to an owned
     thread-safe handle; or
   - reject background scheduling with a typed config error and require an
     explicit deterministic/manual mode.
5. `StorageOpenOptions` can explicitly select deterministic inline for tests.
6. `StorageOpenOptions` can explicitly select evaluate-and-enqueue for queue
   inspection tests.
7. Disabled scheduling still disables post-commit enqueue and worker wake.

Mechanical counter tests:

1. Background worker count is reported in diagnostics.
2. Background scheduler queue depth is reported.
3. Background mode open increments a background-runtime-created counter.
4. Deterministic inline opens do not spawn background workers.
5. Evaluate-and-enqueue opens do not spawn background workers.

Pass gates:

1. No product-facing open path defaults to deterministic inline.
2. Existing deterministic lifecycle tests can still opt out of background.

## Wake And Drain Tests

Correctness tests:

1. A mutating commit that creates frozen-table pressure enqueues flush work and
   wakes the worker.
2. A mutating commit that creates L0 pressure enqueues compaction work and
   wakes the worker.
3. A mutating commit that creates nonzero-level pressure enqueues the scored
   nonzero compaction and wakes the worker.
4. A branch with inherited-layer pressure enqueues materialization and wakes
   the worker.
5. Coalescing a duplicate task does not submit unbounded duplicate background
   wake work.
6. A chain resubmission after one compaction wakes the worker again.
7. A stale wake that finds no pending task records a no-op and does not fail
   the runtime.
8. Explicit API `enqueue_maintenance` wakes the worker in background mode.
9. Explicit API `run_next_maintenance` still works in deterministic/manual
   modes and is not needed for public background mode.
10. Background wake priority maps lifecycle work correctly:
    - flush/checkpoint close-required or pressure-clearing work is High;
    - compaction/materialization is Normal;
    - health/retention/purge/quarantine repair is Low unless upgraded by
      policy.

Mechanical counter tests:

1. `background_wake_submitted` increments on accepted wake submissions.
2. `background_wake_coalesced` increments on duplicate wake suppression.
3. `background_wake_rejected` increments after shutdown or queue full.
4. `background_stale_wake_noop` increments when a wake finds no task.
5. `background_drain_rounds` increments per worker drain round.
6. `background_tasks_completed` matches lifecycle completed task facts.

Generated tests:

1. Random enqueue/coalesce sequences under a single worker.
2. Random maintenance queue capacity limits.
3. Random chain resubmission depth.
4. Random task priority mixes.

Pass gates:

1. Pending maintenance eventually reaches zero after `drain_background()`.
2. Scheduler wake queue depth remains bounded under duplicate pressure.
3. No maintenance task requires inline post-commit execution to start.

## Nonblocking Execution Tests

Correctness tests:

1. A foreground commit can complete while a background compaction is in its
   unlocked build/merge phase.
2. A foreground commit can complete while a background flush is in its
   unlocked table-build or durable-write phase.
3. A foreground commit can complete while materialization is in its unlocked
   build phase.
4. Foreground commit may wait only for short snapshot/publish critical
   sections.
5. If a background candidate becomes stale before publish, the task returns a
   deferred/stale outcome and current pressure is resubmitted.
6. Reads before, during, and after background compaction observe valid rows.
7. Scans before, during, and after background compaction observe valid order
   and tombstone semantics.
8. History/as-of reads before, during, and after background compaction match
   the model.
9. Branch clear/delete/fork operations do not publish stale background output
   over newer branch facts.

Mechanical counter tests:

1. `background_task_snapshot_lock_ns` records short locked snapshot time.
2. `background_task_unlocked_build_ns` records long build/merge/IO time.
3. `background_task_publish_lock_ns` records short publication time.
4. `foreground_wait_background_lock_ns` remains bounded in synthetic long-build
   fixtures.
5. `background_candidate_stale_deferred` increments for stale candidate tests.

Generated tests:

1. Random commit streams while background compaction sleeps in the unlocked
   build phase.
2. Random branch operations while background tasks hold candidate snapshots.
3. Random flush/compaction/materialization interleavings.

Pass gates:

1. No full compaction build holds the foreground runtime lock.
2. Stale background candidates cannot publish.
3. Foreground wait time is measured and bounded.

## Close And Shutdown Tests

Correctness tests:

1. Close on an idle background runtime returns immediately and joins workers.
2. Close with queued ordinary tasks cancels them according to close policy.
3. Close with close-required tasks drains them before returning.
4. Close with an active task waits for the active task or deadline policy.
5. Submit-after-close is rejected and counted.
6. Worker panic during ordinary task records failure and does not hang close.
7. Worker panic during close-required task records failure and close returns a
   typed lifecycle error.
8. Drop of an open public runtime initiates background shutdown.
9. Repeated close calls are idempotent and return prior final facts.
10. Accepted background wakes are either executed, close-drained, or
    close-canceled; none disappear silently.

Race tests:

1. Race post-commit enqueue against close.
2. Race explicit enqueue against close.
3. Race worker drain round resubmission against close.
4. Race scheduler shutdown against wake submit, preserving the old
   submit/shutdown TOCTOU guarantee.

Mechanical counter tests:

1. `background_shutdowns` increments once per runtime shutdown.
2. `background_shutdown_joined_workers` equals worker count.
3. `background_shutdown_drained_tasks` matches close-required drain outcomes.
4. `background_shutdown_canceled_tasks` matches ordinary canceled tasks.
5. `background_submit_after_shutdown_rejected` increments on rejected wake.

Pass gates:

1. Close cannot hang on empty queues, panics, stale wakes, or shutdown races.
2. No accepted work is lost.

## API And Diagnostics Tests

Correctness tests:

1. `maintenance_status()` includes lifecycle queue state and background
   scheduler state.
2. `diagnostics()` reports background worker facts for cache and durable modes.
3. Benchmark load diagnostics separate:
   - foreground commit time;
   - foreground wait on background critical sections;
   - background maintenance time;
   - final diagnostic drain time.
4. Explicit diagnostic drains remain available but are not required for normal
   source-shape convergence.
5. Product-facing APIs do not expose old engine internals or thread handles.

Source guard tests:

1. Public runtime open code must not select `DeterministicInline` as the
   default product-facing policy.
2. Benchmark code must not call explicit fixed-point drain to make 5M/10M
   point reads possible.
3. Lifecycle background code must not import `strata-engine`; it must contain
   the storage-next port.

Pass gates:

1. Diagnostics can explain where maintenance time ran.
2. Benchmark output cannot mislabel background work as foreground commit cost.

## Benchmark Tests

Required command:

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

Required assertions:

1. 100K cache and standard complete.
2. 1M cache and standard complete.
3. 5M cache and standard complete.
4. 10M cache and standard complete.
5. 5M and 10M reach point-read measurement.
6. Source-shape diagnostics pass after load:
   - bounded L0;
   - bounded nonzero fanout;
   - final lifecycle queue depth zero or explained by close/failure facts.
7. No final fixed-point drain is needed for the normal benchmark path.
8. Foreground commit time excludes background build/merge/IO time.
9. Foreground wait on background critical sections is bounded.
10. Background maintenance time is reported separately.

Failure interpretation:

1. If 5M/10M fail to reach reads because background tasks do not drain, L8E-C
   failed.
2. If foreground commit time still includes compaction build/merge cost, L8E-D
   failed.
3. If source shape is unbounded despite background completion, L8B scoring or
   chaining regressed.
4. If close or shutdown loses accepted work, L8E-E failed.

## Verification Commands

Focused:

```bash
cargo test -p strata-storage-next lifecycle_background --all-features --locked
cargo test -p strata-storage-next api_background_maintenance --all-features --locked
cargo test -p strata-storage-next lifecycle_source_guard --all-features --locked
```

Lint:

```bash
cargo clippy -p strata-storage-next --lib --all-features --locked -- -D warnings
```

Full closeout:

```bash
cargo fmt --all
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo test -p strata-storage-next --all-targets --all-features --locked
```
