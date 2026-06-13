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
| L8E-F. Diagnostics And Benchmark Gate | Expose background scheduler, WAL, and closed-loop liveness metrics; rerun 100K/1M/5M/10M. | 5M/10M reach point-read measurement with bounded WAL retention, bounded source shape, and no large inline or final fixed-point compaction cliff. |
| L8E-G. Merge-Cost Fallback | Optimize table-rewrite merge cost if L8E-F proves the serialized compaction chain cannot keep up after throttling. | Background maintenance drain rate is high enough for the 10M standard gate without adding benchmark shortcuts. |
| L8E-H. Preserve The Simulation Boundary | Land the worker thread and maintenance clock behind swappable abstractions so the production drive path stays runnable single-threaded and deterministic. | Drive logic runs unchanged on threaded and inline executors; no `Instant::now()` in drive logic; deterministic-replay smoke test passes. |

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
   - Expose the scheduler only through the `MaintenanceExecutor` trait
     (decision 10); the concrete ported type is one implementation, never named
     directly by the drive logic.
2. **Public runtime default**
   - Public cache and owned durable local opens use background maintenance by
     default.
   - Deterministic inline remains only for deterministic unit tests and
     explicit diagnostic modes, and must be realized as the `Background` drive
     path running on the inline executor (decision 10), not as a parallel
     maintenance-driving implementation.
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
5. **Old storage concurrency invariant**
   - Preserve the old storage engine invariant, not necessarily the old
     implementation detail.
   - The old background scheduler was a configurable multi-worker priority
     executor (`StorageConfig::background_threads`, default
     `min(4, available CPU cores)`), but storage maintenance constrained the
     work submitted to it:
     - at most one compaction chain was in flight;
     - each compaction task performed one highest-scoring branch/level rewrite,
       then re-scored before resubmitting;
     - flush scheduling was coalesced by one in-flight flush drain that drained
       all currently frozen memtables and handled the final-check/flag-clear
       TOCTOU case;
     - write pressure used slowdown-before-stall and a bounded stall deadline
       instead of depending on benchmark retry loops.
   - L8E must preserve those invariants. It may improve the scheduler,
     wake policy, queue accounting, clock abstraction, and fairness mechanics,
     but it must not rediscover a stricter "one total background worker" rule
     or allow parallel same-branch/table publication by accident.
   - Multiple worker threads are allowed for independent lifecycle wake classes
     once the lifecycle queue, active-task facts, and publication guards prove
     the old invariants above. Parallel compaction chains remain out of scope
     until explicitly planned and tested.
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
   - Urgent severity in background mode wakes workers and records accepted-
     under-pressure facts; it must not run full maintenance inline.
   - Urgent severity also applies graduated foreground slowdown. The delay is
     bounded, recorded, and designed to converge foreground write rate toward
     observed background drain rate.
   - Block severity waits for background maintenance progress until a
     configured deadline. It returns the existing typed pressure error only
     after the deadline expires or when maintenance is not making progress.
   - The L9 benchmark must not add retry loops to hide Block errors; the
     runtime admission policy itself must keep sustained load live.
9. **WAL retention is a background-maintenance trigger**
   - WAL growth policy evaluation is a wake source, not just a passive
     diagnostic.
   - Crossing `max_commits_since_checkpoint`, retained-byte, or retained-
     segment thresholds must enqueue checkpoint/flush-watermark work and wake a
     High-priority background drain.
   - Standard durable load is not closed out until retained WAL bytes and
     segments remain bounded throughout the run.
10. **Simulation boundary (do not close the DST door)**
    - **Problem.** storage-next is single-threaded with all I/O behind the
      `Backend` trait and the data-plane clock injectable via
      `CommitTimestampSource`. This satisfies the preconditions for
      deterministic simulation testing (DST) — the highest-leverage technique
      for the rare-interleaving and fault-combination bug class
      (`docs/architecture/v1-storage-testing-taxonomy-and-gaps.md`, class 9).
      L8E introduces the first worker thread and the first wall-clock-dependent
      control logic into the crate. If the scheduler is a concrete threaded type
      that the drive logic calls directly, and timing reads `Instant::now()`
      directly, the production maintenance path becomes unrunnable under a
      single-threaded seeded simulation and the door closes. The retrofit is the
      canonical near-impossible change once L8C, L8D, and M5 build on the
      concrete shape.
    - **Current status.** The first L8E pass landed a concrete
      `BackgroundScheduler` held directly as `Arc<BackgroundScheduler>` by
      `BackgroundRuntimeController`, raw `Instant::now()` throughout the drive
      logic, and `DeterministicInline` as a separate drive path. This already
      crosses the line. L8E-H is the corrective requirement; executing this plan
      brings the existing code into compliance.
    - **Rule.** The production maintenance drive logic must run unchanged on both
      a threaded executor and a single-threaded inline executor, and must read
      time from an injected clock, not `Instant::now()`. This decision refines
      decisions 1 and 2. See L8E-H for the seams and exit gates.

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
9. move commit-runtime L7 into background threads;
10. close the deterministic-simulation door: the worker thread and maintenance
    timing must stay abstracted per decision 10 and L8E-H;
11. call `Instant::now()` directly inside maintenance drive logic;
12. keep `DeterministicInline` as a permanent parallel drive path without a
    deletion condition.

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
   - WAL growth policy evaluation that enqueues checkpoint or flush-watermark
     work;
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
6. Ensure scheduling remains deterministic under any supported worker count:
   - lifecycle queue order and coalescing remain authoritative;
   - at most one compaction chain is in flight;
   - each compaction task performs one selected rewrite, then re-reads facts
     and re-scores before resubmitting;
   - flush drain is coalesced and drains all eligible frozen memtables for its
     scope before lower-priority same-branch table rewrites;
   - scheduler priority and worker count may decide which independent wake
     class runs first, but must not create duplicate publication for the same
     branch/table/scope.
7. Add closed-loop admission support for sustained overload:
   - maintain moving counters for foreground commit production rate,
     background task completion rate, and queue/source-shape backlog;
   - when Urgent pressure persists, inject a bounded per-commit delay before
     admission instead of running full maintenance inline;
   - when Block pressure is reached, wait on a background-progress signal until
     the configured deadline before returning `StoragePressureRejected`;
   - wake blocked/slow-path writers when a background task clears pressure,
     checkpoint/WAL debt, or queue debt.
8. Record admission-throttle facts:
   - slowdown attempts;
   - slowdown sleep nanoseconds;
   - block waits;
   - block wait nanoseconds;
   - deadline expirations;
   - wakeups from background progress.

Exit gates:

1. Queued post-commit maintenance drains without calling public
   `run_next_maintenance`.
2. Coalesced pressure does not create unbounded background scheduler depth.
3. Chain resubmission wakes the worker until source shape is healthy.
4. WAL growth threshold crossings wake checkpoint work and durable WAL
   retention is able to delete covered segments without an explicit benchmark
   drain.
5. A writer that is permanently faster than background maintenance drain
   capacity slows down and eventually converges instead of running until a
   nonzero-level Block rejection.

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
6. Split checkpoint execution so `delete_covered_segments` and any durable
   object deletion run on the background worker, with only the checkpoint
   publication/watermark proof under foreground-visible locks.
7. Add a foreground admission counter for time waiting on background-owned
   critical sections.
8. Add a background critical-section counter for:
   - snapshot lock time;
   - publish lock time;
   - unlocked build time;
   - total task time.
9. Keep branch and table publication proofs intact. If a candidate snapshot
   becomes stale before publish, the task must complete as deferred/stale and
   resubmit current pressure rather than publishing stale output.

Exit gates:

1. A foreground commit loop can continue while a background compaction performs
   the unlocked merge/build phase.
2. A stale compaction candidate never publishes over newer branch state.
3. Source shape still converges under sustained writes.
4. WAL checkpoint and covered-segment deletion do not hold the foreground
   runtime lock for object deletion or segment scanning work.

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
   - background worker count, defaulted and validated as a product runtime
     policy, not as a correctness crutch;
   - background scheduler queue depth;
   - max tasks per wake;
   - max runtime per wake.
4. Update benchmark diagnostics so load output separates:
   - foreground commit time;
   - foreground wait on background critical sections;
   - background maintenance task time;
   - final diagnostic drain time;
   - retained WAL bytes and segments over time;
   - admission slowdown/wait time.
5. Add a debug-mode WAL tripwire for local durable benchmarks:
   - log or assert any `wal/` delete, rename, or truncate not routed through
     `delete_covered_segments`;
   - include the operation, object name, and caller context in the diagnostic;
   - keep the tripwire test-only/debug-only so product release paths are not
     changed.
6. Add a scaled-constants closed-loop liveness test:
   - shrink WAL segment size, memtable rotation, and level targets so a
     50K-row run exercises the same flush/checkpoint/compaction trajectory as
     the 5M/10M benchmark;
   - run under normal CI;
   - assert no permanent commit failure, bounded WAL retention, bounded L0,
     bounded nonzero fanout, and final queue convergence.
7. Run the L9 scale benchmark:

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
6. Retained WAL bytes and segment counts remain below configured thresholds
   throughout standard durable load.
7. The scaled-constants closed-loop liveness test is a hard CI gate.

## L8E-G. Merge-Cost Fallback

Goal: provide the named, implementation-ready fallback if L8E-F shows that the
serialized compaction chain still cannot drain table rewrites fast enough after
wake, nonblocking, and graduated admission policies are correct.

Trigger:

1. The scaled-constants liveness test or 5M/10M standard benchmark shows
   sustained queue/source-shape backlog even though:
   - WAL retention is bounded;
   - background wakes are accepted and completed;
   - foreground commits are slowed according to policy rather than failing
     immediately;
   - foreground wait is not dominated by long lock holds.

Tasks:

1. Compute immutable table facts during table build and hand them directly to
   publication so compaction does not re-decode tables just to recover facts.
2. Handoff decoded table metadata/readers from the build phase to the publish
   phase where correctness permits, avoiding open-bytes re-parse work.
3. Reuse merge-loop row buffers and allocation arenas across table-rewrite
   tasks.
4. Add counters for:
   - facts decoded during build;
   - redundant fact decodes avoided;
   - table reader reopens avoided;
   - merge-loop allocations and reuses;
   - row merge nanoseconds per input row.
5. Rerun the scaled liveness test and 5M/10M standard benchmark after each
   merge-cost change.

Exit gates:

1. Background row-merge cost falls enough for the serialized compaction chain
   plus graduated admission to keep source shape bounded at 10M standard scale.
2. The fallback does not weaken table publication, manifest, watermark, or
   stale-candidate proofs.

## L8E-H. Preserve The Simulation Boundary

Goal: land the worker thread and wall-clock-dependent control logic without
making the production maintenance path unrunnable under deterministic
single-threaded simulation. This is the enabling-seam slice for the future
DST-lite test class (taxonomy class 9); it does not build the simulator, it
keeps the simulator buildable. Because the first L8E pass already crossed the
line (decision 10, "current status"), this group is corrective.

### Problem restatement

DST requires every source of nondeterminism — threads, time, randomness, I/O —
behind a swappable abstraction so a seeded single-threaded run replays exactly.
storage-next already isolates I/O (`Backend`) and the data-plane clock
(`CommitTimestampSource`). The two new sources L8E introduces are the worker
thread and wall-clock timing in the drive logic. Both must be abstracted now,
while the drive logic is still localized in `BackgroundRuntimeController` and
`drain_*_background_round`. Every dependent slice (L8C, L8D, M5) that builds on
the concrete scheduler or `Instant::now()` enlarges the retrofit.

### Targeted fixes

1. **MaintenanceExecutor trait.**
   - Define a `MaintenanceExecutor` trait whose surface is
     submit(priority, work) / drain / shutdown / wait-for-idle / stats.
   - The trait signature must not name `std::thread`, `JoinHandle`,
     `parking_lot`, `Condvar`, or `Instant`.
   - The ported `BackgroundScheduler` becomes one implementation
     (`ThreadedMaintenanceExecutor`) behind the trait.
   - `BackgroundRuntimeController` holds `Arc<dyn MaintenanceExecutor>` or a
     generic `E: MaintenanceExecutor`, never the concrete scheduler type.
2. **InlineMaintenanceExecutor.**
   - Provide a single-threaded, step-driven executor: submit enqueues; an
     explicit `drain`/`run_pending` runs queued work synchronously on the
     calling thread in deterministic priority + FIFO order.
   - No threads, condvars, or sleeps.
3. **Unify the drive path.**
   - The wake/drain/execute control logic (`drain_*_background_round`,
     wake/coalesce, graduated slowdown, block-wait) must be identical regardless
     of executor.
   - Re-express `DeterministicInline` as `Background` +
     `InlineMaintenanceExecutor` so deterministic tests exercise the production
     drive logic. Any temporary separate path must be marked transitional with a
     deletion condition (PR discipline rule 4).
4. **Inject the maintenance clock.**
   - Introduce a `MaintenanceClock` handle (monotonic now/elapsed) threaded
     through all drive-logic timing: `max_runtime_per_wake`, drain-round limits,
     the block-wait deadline, graduated slowdown, and any timing span that
     affects control flow.
   - Provide a real clock (wraps `Instant::now()`) and a manual/simulated clock
     advanced explicitly by a harness.
   - Drive-logic modules must not call `Instant::now()` directly. Raw wall-clock
     reads may remain only in the threaded executor implementation and in
     perf-trace spans that do not affect control flow.
5. **Executor + clock selection.**
   - Public cache and owned durable opens select the threaded executor + real
     clock by default.
   - A test/simulation construction path selects the inline executor + manual
     clock with no other code differences.

### Exit gates

1. `BackgroundRuntimeController` and the drive logic name only the
   `MaintenanceExecutor` trait, not the concrete `BackgroundScheduler`.
2. No drive-logic module references `Instant::now()`; timing flows through
   `MaintenanceClock` (source-guard enforced).
3. The same closed-loop scenario run twice under the inline executor + manual
   clock with identical inputs produces identical maintenance task order, queue
   trajectory, and final source shape.
4. Deterministic lifecycle tests run the `Background` drive path under the inline
   executor, not a separate `DeterministicInline` implementation.
5. The threaded path still passes all L8E-A scheduler parity tests.

## Stop Conditions

Stop and revise this plan only if:

1. `crates/engine/src/background.rs` cannot be ported because storage-next
   cannot use `parking_lot` or `std::thread`;
2. L5/L6 cannot expose snapshot/build/publish boundaries without changing
   correctness-critical public APIs;
3. durable owned backend handles cannot be made `Send + Sync + 'static`;
4. branch publication proofs cannot detect stale background candidates;
5. close cannot join workers without violating already-landed close contracts;
6. graduated slowdown and block-wait admission cannot be implemented without
   changing public retry semantics;
7. WAL retention cannot be bounded without moving ownership of checkpoint or
   segment deletion out of storage-next;
8. the `MaintenanceExecutor` trait cannot wrap the threaded scheduler without
   exposing `std::thread`/`parking_lot` in its signature, or the inline executor
   cannot run the production drive logic synchronously.

Any stop condition must produce a new implementation plan before L8E, L8C, or
L8D continues.

## Verification Commands

Focused commands:

```bash
cargo test -p strata-storage-next lifecycle_background --all-features --locked
cargo test -p strata-storage-next api_background_maintenance --all-features --locked
cargo test -p strata-storage-next lifecycle_source_guard --all-features --locked
cargo test -p strata-storage-next l8e_scaled_liveness --all-features --locked
cargo test -p strata-storage-next lifecycle_simulation_boundary --all-features --locked
cargo clippy -p strata-storage-next --lib --all-features --locked -- -D warnings
```

Thread-safety closeout:

```bash
RUSTFLAGS="-Zsanitizer=thread" cargo +nightly test -p strata-storage-next \
  lifecycle_background --all-features --target x86_64-apple-darwin --locked
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
