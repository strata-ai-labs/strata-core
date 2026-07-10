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
5. deterministic tests lose their ability to opt out of background execution;
6. WAL retention grows without a background checkpoint wake;
7. sustained max-rate writes fail permanently instead of slowing to the
   background drain rate;
8. the production maintenance drive path cannot run single-threaded and
   deterministically (the deterministic-simulation door is closed).

## Test Matrix

| Area | Required Proof | Failure Caught |
| --- | --- | --- |
| Scheduler port parity | Old `BackgroundScheduler` semantics are preserved in storage-next. | Lost wakeups, dropped accepted tasks, panic hangs, priority regressions. |
| Runtime mode selection | Public opens use background mode; tests can request deterministic/manual modes. | Product path silently falls back to inline maintenance. |
| Wake/coalesce integration | Lifecycle enqueue/coalesce wakes workers without flooding scheduler queue. | Queue grows unbounded or tasks remain pending forever. |
| Nonblocking execution | Long task build/merge work occurs outside foreground critical sections. | Background thread merely moves the inline tax behind a mutex. |
| Close/shutdown | Close drains or cancels tasks according to lifecycle policy and joins workers. | Dropped tasks, deadlocks, submit-after-shutdown races. |
| WAL retention | WAL-growth thresholds wake checkpoint work and covered segments are deleted. | Durable standard load retains hundreds of MB or many segments with no failing gate. |
| Closed-loop overload | Writer rate converges to background drain rate under sustained pressure. | One worker falls behind until a nonzero-level Block error terminates the run. |
| Benchmark proof | 5M/10M reach reads with bounded source shape, bounded WAL, and bounded foreground wait. | Compaction cliff or WAL-retention cliff remains hidden behind final drain or timeout. |
| Simulation boundary | Production drive logic runs unchanged under a single-threaded inline executor + manual clock and replays deterministically. | DST door closed: scheduler/clock baked into drive logic; deterministic tests run a parallel path. |

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
10. WAL growth threshold crossings wake checkpoint work:
    - sustained commits past `max_commits_since_checkpoint` enqueue checkpoint;
    - retained-byte and retained-segment thresholds enqueue checkpoint or
      flush-watermark work;
    - the background worker executes the checkpoint;
    - durable `delete_covered_segments` fires and retained segments decrease.
11. Background wake priority maps lifecycle work correctly:
    - flush/checkpoint close-required or pressure-clearing work is High;
    - compaction/materialization is Normal;
    - health/retention/purge/quarantine repair is Low unless upgraded by
      policy.
12. Urgent pressure wakes the worker and enters the configured slowdown path
    without running full maintenance inline.
13. Block pressure waits for background progress until the configured deadline
    before returning the typed pressure error.

Mechanical counter tests:

1. `background_wake_submitted` increments on accepted wake submissions.
2. `background_wake_coalesced` increments on duplicate wake suppression.
3. `background_wake_rejected` increments after shutdown or queue full.
4. `background_stale_wake_noop` increments when a wake finds no task.
5. `background_drain_rounds` increments per worker drain round.
6. `background_tasks_completed` matches lifecycle completed task facts.
7. WAL-growth wake counters distinguish threshold evaluation, checkpoint
   enqueue, checkpoint wake submission, and covered-segment deletion.
8. Admission-throttle counters record slowdown attempts, slowdown nanoseconds,
   block waits, block-wait nanoseconds, deadline expirations, and wakeups from
   background progress.

Generated tests:

1. Random enqueue/coalesce sequences under one-worker and multi-worker
   scheduler configurations, asserting the old storage invariant: one
   compaction chain in flight, coalesced flush drain, and no duplicate
   same-branch/table publication.
2. Random maintenance queue capacity limits.
3. Random chain resubmission depth.
4. Random task priority mixes.
5. Random WAL-growth threshold crossings interleaved with flush and checkpoint
   tasks.
6. Random sustained-overload scripts where writer production exceeds initial
   drain rate.

Pass gates:

1. Pending maintenance eventually reaches zero after `drain_background()`.
2. Scheduler wake queue depth remains bounded under duplicate pressure.
3. No maintenance task requires inline post-commit execution to start.
4. WAL retention converges after checkpoint wake without an explicit benchmark
   drain.
5. Sustained overload slows commits or waits with deadline; it must not run
   until a permanent Block failure without prior measured slowdown.

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
6. Checkpoint/WAL deletion background task time is reported separately from
   foreground commit time.
7. WAL delete/rename tripwire records no unexpected `wal/` mutations outside
   `delete_covered_segments` during local durable tests.

Generated tests:

1. Random commit streams while background compaction sleeps in the unlocked
   build phase.
2. Random branch operations while background tasks hold candidate snapshots.
3. Random flush/compaction/materialization interleavings.

Pass gates:

1. No full compaction build holds the foreground runtime lock.
2. Stale background candidates cannot publish.
3. Foreground wait time is measured and bounded.
4. WAL checkpoint and covered-segment deletion do not hold foreground locks for
   object deletion or segment scanning.

## Closed-Loop Liveness Tests

Correctness tests:

1. Scaled-constants sustained load:
   - shrink WAL segment size, memtable rotation, L0 thresholds, and nonzero
     level targets so 50K rows exercise the 5M/10M trajectory;
   - run through the public background runtime path;
   - do not call public fixed-point drain;
   - assert every commit either succeeds normally or enters the bounded
     slowdown/wait path and then succeeds;
   - assert no permanent `StoragePressureRejected` occurs before the configured
     deadline policy says the system is unhealthy.
2. The same scaled load in durable standard mode:
   - retained WAL segment count remains bounded throughout the load;
   - retained WAL bytes remain bounded throughout the load;
   - at least one checkpoint executes in the background;
   - at least one `delete_covered_segments` call deletes covered segments.
3. Queue convergence after scaled load:
   - final pending lifecycle tasks are zero, or every remaining task has an
     explicit close/failure/deferred fact;
   - L0 table count is bounded;
   - nonzero fanout is bounded.
4. Writer-faster-than-drain fixture:
   - inject a slow background merge/build phase;
   - drive foreground commits faster than the worker can initially drain;
   - assert foreground commit latency includes bounded slowdown/wait facts;
   - assert the run converges without a permanent Block error.

Mechanical counter tests:

1. Scaled liveness records nonzero background wake submissions.
2. Scaled liveness records nonzero background task completions.
3. Scaled liveness records WAL checkpoint and covered-segment deletion facts in
   durable standard mode.
4. Sustained-overload fixtures record slowdown or block-wait facts before any
   deadline expiration.

Pass gates:

1. This suite is a normal CI gate once L8E-C/D/F land.
2. Before the full L8E closeout it may be marked expected-fail only with the
   exact missing slice named in the failure message.
3. It becomes a hard gate before the 5M/10M benchmark is accepted.

## Simulation Boundary Tests

These tests prove L8E did not close the deterministic-simulation door (taxonomy
class 9; see `docs/architecture/v1-storage-testing-taxonomy-and-gaps.md` and
implementation-plan group L8E-H). They are the enabling-seam proof, not the full
simulator.

Correctness tests:

1. Inline-executor deterministic replay:
   - run a fixed closed-loop scenario (open, fixed commit stream, drive
     maintenance, close) under `InlineMaintenanceExecutor` + manual clock;
   - run it twice with identical inputs;
   - assert identical maintenance task execution order, lifecycle queue-depth
     trajectory, final source shape, and final visible version.
2. Executor parity:
   - run the same scenario under the threaded executor and the inline executor;
   - assert the same final source shape and the same set of completed
     maintenance tasks (only timing/interleaving may differ).
3. Unified drive path:
   - deterministic lifecycle tests that previously used `DeterministicInline`
     drive the `Background` path on the inline executor and still assert exact
     task ordering.
4. Manual-clock control:
   - advancing the manual clock past `max_runtime_per_wake` ends a drain round
     at the simulated deadline with no dependence on wall-clock time;
   - a block-wait scenario resolves against the manual-clock deadline, not
     `Instant::now()`;
   - a graduated-slowdown scenario computes its bounded delay from the manual
     clock.

Source guard tests:

1. The drive-logic modules (`BackgroundRuntimeController`,
   `drain_*_background_round`, pressure wait/slowdown) must not reference
   `std::time::Instant::now()`; all control-flow timing must flow through
   `MaintenanceClock`. Raw reads are allowed only in the threaded executor
   implementation and in non-control-flow perf-trace spans.
2. `BackgroundRuntimeController` and the drive logic must name the
   `MaintenanceExecutor` trait, not the concrete `BackgroundScheduler` type.
3. The `MaintenanceExecutor` trait signature must not expose `std::thread`,
   `JoinHandle`, `parking_lot`, `Condvar`, or `Instant`.
4. Any remaining `DeterministicInline` drive path carries a deletion-condition
   comment and is not referenced as a default product path.

Pass gates:

1. The production maintenance drive path is runnable single-threaded and
   replays deterministically.
2. Deterministic tests and production share one drive implementation.
3. Maintenance control-flow timing is injectable.

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
5. Race WAL-growth checkpoint wake against close and commit admission.
6. Race slowdown/block-wait wakeup against background task failure.

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
6. Diagnostics report retained WAL bytes and segments for durable standard
   runs.
7. Diagnostics report admission slowdown/wait time separately from foreground
   commit work.

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
3. Diagnostics can explain whether WAL retention was bounded by background
   checkpoint/deletion work.

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
11. Retained WAL segment count remains bounded throughout standard durable
    load.
12. Retained WAL bytes remain bounded throughout standard durable load.
13. Standard durable benchmark reports checkpoint executions and covered
    segment deletions when WAL thresholds are crossed.
14. Sustained pressure does not terminate the benchmark with
    `NonZeroLevelTableBacklog` or another permanent Block error unless the
    configured deadline expires and the result is reported as a failed gate.

Failure interpretation:

1. If 5M/10M fail to reach reads because background tasks do not drain, L8E-C
   failed.
2. If foreground commit time still includes compaction build/merge cost, L8E-D
   failed.
3. If source shape is unbounded despite background completion, L8B scoring or
   chaining regressed.
4. If close or shutdown loses accepted work, L8E-E failed.
5. If retained WAL bytes or segments grow without checkpoint/delete progress,
   WAL-growth wake or durable checkpoint/deletion integration failed.
6. If backlog grows monotonically until Block rejection under max-rate writes,
   graduated slowdown/block-wait admission failed.
7. If all scheduler/admission/WAL facts are correct but background row-merge
   drain rate is still too low, execute the L8E-G merge-cost fallback slice:
   facts-computed-during-build, decoded reader handoff, and merge-loop
   allocation reuse.

## Sanitizer And Tripwire Tests

Required checks:

1. Run ThreadSanitizer on the lifecycle background test target in CI or in the
   L8E closeout job.
2. Local durable debug/test builds must include a WAL mutation tripwire for the
   benchmark gate:
   - any `wal/` delete, rename, or truncate outside `delete_covered_segments`
     fails or emits a structured diagnostic;
   - the diagnostic includes object name, operation, and caller context;
   - the tripwire is disabled for product release builds.
3. The WAL tripwire must be active during the 5M/10M standard reruns.

## Verification Commands

Focused:

```bash
cargo test -p strata-storage-next lifecycle_background --all-features --locked
cargo test -p strata-storage-next api_background_maintenance --all-features --locked
cargo test -p strata-storage-next lifecycle_source_guard --all-features --locked
cargo test -p strata-storage-next l8e_scaled_liveness --all-features --locked
cargo test -p strata-storage-next lifecycle_simulation_boundary --all-features --locked
```

Lint:

```bash
cargo clippy -p strata-storage-next --lib --all-features --locked -- -D warnings
```

Thread-safety closeout:

```bash
RUSTFLAGS="-Zsanitizer=thread" cargo +nightly test -p strata-storage-next \
  lifecycle_background --all-features --target x86_64-apple-darwin --locked
```

Full closeout:

```bash
cargo fmt --all
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo test -p strata-storage-next --all-targets --all-features --locked
```
