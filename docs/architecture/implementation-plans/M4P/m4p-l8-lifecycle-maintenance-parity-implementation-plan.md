# M4P-L8 Implementation Plan: Lifecycle Maintenance Parity

Status: draft

Parent plan:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-implementation-plan.md`

Parent test methodology:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-test-plan.md`

Architecture context:
`docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`

Detailed L8 context:

1. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4P/m4p-l8-automatic-maintenance-scheduling-followup.md`
3. `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8i-flush-table-publication-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L8/storage-deferred-work-ledger.md`

Audit context:

1. `docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`,
   especially `### L8. Lifecycle / Recovery / Maintenance`,
   `### 4. LSM Layout, Level Invariants, And Compaction`, and
   `GAP-L8`.
2. `docs/architecture/perf-tuning/storage-next-serving-path-parity-plan.md`,
   especially the serving-path proof gates for maintenance, compaction, and
   source fanout.

## Objective

Restore the L8 maintenance mechanics needed for sustained storage-next loads to
stay in a healthy LSM shape through normal L9 writes.

This plan covers the M4P-L8 critical path:

1. automatic maintenance scheduling after mutating commits;
2. complete flush drain from one pressure event;
3. score-based compaction selection and chaining;
4. write-admission pressure policy;
5. L9 benchmark/counter closeout for 100K, 1M, 5M, and 10M loads.

The goal is not to port the old engine background thread wholesale. The goal is
to restore the old mechanical invariant: normal writes must not strand frozen
state, L0 tables, or shallow-level backlog until a benchmark or user manually
runs a fixed-point maintenance drain.

## Status Update

The first-pass L8 implementation has landed the immediate maintenance loop:

1. post-commit maintenance scheduling evaluates pressure and enqueues/coalesces
   work;
2. flush drain can drain all eligible frozen state for the selected scope;
3. compaction and materialization tasks are scored, executed, and re-scored for
   chaining;
4. write admission emits pressure facts and can reject blocking pressure before
   avoidable commit allocation;
5. deterministic-inline urgent admission can drive one suggested maintenance
   task when the runtime policy allows it;
6. benchmark diagnostics expose maintenance queue, source shape, and point-read
   probe facts.

Residual parity work is tracked in:

1. `docs/architecture/implementation-plans/M4P/m4p-l8b-lifecycle-maintenance-followup-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4P/m4p-l8b-lifecycle-maintenance-followup-test-plan.md`

The audit findings below are the historical starting point for L8. Treat the
L8B follow-up docs as the current owner for remaining level-shape,
cross-branch coverage, pressure/admission, resource-throttling, and
snapshot/pruning gaps.

## Audit Findings

Primary audit section:
`docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`,
`### L8. Lifecycle / Recovery / Maintenance`.

Findings to close in this plan:

1. Automatic maintenance scheduling is not restored.
   - Old writes called `schedule_flush_if_needed` after successful mutating
     commits. That coalesced flush work, drained frozen memtables, then
     scheduled a compaction chain.
   - Storage-next currently exposes explicit maintenance and pressure facts,
     but ordinary commits do not enqueue or drive enough flush, compaction, or
     materialization work to keep source fanout bounded.
2. Write-admission backpressure is diagnostic-only today.
   - Old `maybe_apply_write_backpressure` could synchronously flush, slow,
     stall, or reject based on L0, mutable memory, and metadata pressure.
   - Storage-next can report `Background`, `Urgent`, and
     `BlockMutatingAdmission` pressure, but the normal mutating commit path does
     not enforce a policy before admission.
3. Flush drain semantics are weaker than old storage.
   - Old flush scheduling drained all currently eligible frozen state, with a
     bounded retry loop for freeze-during-drain.
   - Storage-next flushes one selected frozen table for one branch per task
     unless an explicit caller drives more work.
4. Compaction scheduling lacks old scoring and chain behavior.
   - Old storage scored branches/levels, picked the highest-scoring work item,
     compacted one unit, then re-scored and resubmitted while unhealthy.
   - Storage-next suggests mostly L0 table-count work and does not select the
     highest-pressure level across branches. Nonzero compaction selection is
     not yet a real scheduler decision.
5. L9 scale benchmark preparation pays a large final fixed-point compaction
   cliff.
   - The L6 read path is now mostly bounded for the completed 100K/1M point
     measurements.
   - The 5M load path still spent minutes CPU-bound in final source-shape
     preparation because maintenance had not kept the LSM healthy during the
     load.

Findings intentionally not closed by this plan:

| Finding | Owner | Reason |
| --- | --- | --- |
| Multi-branch table-manifest flush-watermark proof | L8T | Needed for durable WAL truncation breadth, but not required to prove cache-mode source fanout scheduling. |
| Pending-release durable persistence and purge completion | L8Y/L8Z plus retention/quarantine slices | Important durable closeout, but separate from post-commit scheduling and score-based compaction. |
| Rich adaptive checkpoint scheduler | Post-V1 beyond L8Z minimal WAL-growth guard | V1 already owns minimal WAL-growth protection; adaptive timing policy is not needed for source-fanout parity. |
| Threaded/background executor | Post-V1 unless measurement forces it | Deterministic in-process scheduling is enough to prove the mechanics. A later thread can consume the same queue. |
| New L5 table merge algorithms or L6 table install semantics | L5/L6 | L8 schedules and orders work. It must not own row merge, table split, or branch replacement algorithms. |

## Old Source Map

Old storage and engine evidence:

1. `crates/engine/src/database/transaction.rs`
   - `schedule_flush_if_needed`;
   - `schedule_background_compaction`;
   - `maybe_apply_write_backpressure`;
   - write-path maintenance triggers after mutating commits.
2. `crates/engine/src/background.rs`
   - task queue, coalescing, drain, cancellation, and resubmission evidence.
3. `crates/engine/src/database/compaction.rs`
   - background compaction entry points.
4. `crates/engine/src/database/lifecycle.rs`
   - maintenance drain and shutdown ordering.
5. `crates/storage/src/segmented/mod.rs`
   - frozen flush, branch pressure facts, and LSM maintenance integration.
6. `crates/storage/src/segmented/compaction.rs`
   - compaction scores, L0 trigger, nonzero-level scoring, compact pointers,
     one-operation compaction, chain resubmission, trivial moves, and
     grandparent split evidence.
7. `crates/storage/src/pressure.rs`
   - pressure levels and old write-admission thresholds.
8. `crates/storage/src/rate_limiter.rs`
   - old smoothing evidence.
9. `crates/storage/src/runtime_config.rs`
   - L0 slowdown/stop thresholds, memtable budget, and background maintenance
     configuration.
10. `crates/storage/src/segmented/tests/flush.rs` and
    `crates/storage/src/segmented/tests/leveled.rs`
    - old flush and leveled compaction behavior coverage.

## Storage-Next Target Map

Primary targets:

1. `crates/storage-next/src/lifecycle/maintenance.rs`
2. `crates/storage-next/src/lifecycle/flush.rs`
3. `crates/storage-next/src/lifecycle/compaction.rs`
4. `crates/storage-next/src/lifecycle/pressure.rs`
5. `crates/storage-next/src/lifecycle/cache.rs`
6. `crates/storage-next/src/lifecycle/durable/maintenance.rs`
7. `crates/storage-next/src/lifecycle/durable.rs`
8. `crates/storage-next/src/lifecycle/wal_growth.rs`
9. `crates/storage-next/src/branch/state/compaction.rs`
10. `crates/storage-next/src/branch/facts.rs`
11. `crates/storage-next/src/commit/facts.rs`
12. `crates/storage-next/src/api/runtime.rs`
13. `crates/storage-next/src/api/maintenance.rs`
14. `crates/storage-next/src/observability/perf_trace.rs`

Benchmark and test targets:

1. `benchmarks/src/bin/storage_next_l9_scale.rs`
2. `crates/storage-next/src/lifecycle/tests/`
3. `crates/storage-next/src/api/tests/maintenance.rs`
4. `crates/storage-next/tests/lifecycle_maintenance.rs`
5. `crates/storage-next/tests/lifecycle_properties.rs`
6. `crates/storage-next/tests/lifecycle_source_guard.rs`

## Preconditions

Required before implementation:

1. L5 table compaction and table reader counters exist.
2. L6 branch LSM read-path counters and bounded point-read behavior are in
   place.
3. L6 branch compaction can plan and install L0, L0-to-L1, and selected
   nonzero-level compactions.
4. L7 commit pressure/admission facts exist and do not call L8 scheduling
   directly.
5. L8 deterministic maintenance executor, flush handlers, compaction handlers,
   materialization handlers, and WAL-growth policy exist.

All five are true enough to start this M4P-L8 slice. If any expected target
surface is missing during implementation, add the smallest L8-owned adapter
rather than reaching into L5/L6 internals.

## Non-Goals

This plan must not implement:

1. public product maintenance commands or UX wording;
2. benchmark-only flush/compact shortcuts;
3. L9 read or scan bypasses;
4. new L5 row merge, table split, or block-cache algorithms;
5. new L6 branch replacement semantics;
6. durable byte-format changes;
7. production object-store durability;
8. distributed locks, consensus, or multi-process maintenance;
9. a required background thread;
10. rich adaptive checkpoint policy beyond the existing minimal WAL-growth
    guard;
11. public transaction sessions;
12. product-level blocking/retry policy.

If write admission needs a caller-visible stall/retry contract, L8 should emit
typed storage facts and L9 should decide the public API mapping.

## Correctness Rules

L8 maintenance parity must preserve these rules:

1. Scheduling maintenance must never change visible rows by itself; only the
   lower-layer flush, compaction, or materialization operation may change
   storage shape.
2. Flush must run before compaction when frozen mutable state exists for the
   same branch.
3. A coalesced flush event must drain all currently eligible frozen state for
   the selected scope, subject to a bounded freeze-during-drain retry policy.
4. Compaction selection must use L6 candidate facts and L5 table facts. L8 must
   not inspect or merge rows directly.
5. Compaction chaining must re-read source-shape facts after every completed
   operation before deciding whether to enqueue the next operation.
6. L0 pressure is count-based; nonzero-level pressure is byte/count based using
   documented target thresholds.
7. Nonzero-level input choice must be deterministic and must not always select
   table index zero when other tables carry the pressure.
8. Materialization scheduling must use L6 inherited-layer facts and preserve
   inherited visibility semantics.
9. Admission policy must run before avoidable mutating work when pressure is
   already blocking.
10. Admission policy must return typed storage errors/facts for reject/defer
    decisions and must not use product vocabulary.
11. Durable maintenance failures must become health debt and must not be
    reported as clean success.
12. Cache and durable modes must produce the same branch-local level shape for
    the same logical workload after persistence overhead is ignored.

## Implementation Slices

### M4P-L8-A. Maintenance Scheduling Baseline

Goal: make post-commit maintenance scheduling observable and configurable
before policy changes.

Tasks:

1. Add a lifecycle-local scheduler policy type that can be enabled, disabled,
   or set to deterministic-inline mode for tests.
2. Add post-mutating-commit integration points in cache and durable lifecycle
   runtimes.
3. Evaluate existing L6/L8 pressure facts after a successful mutating commit.
4. Convert pressure facts into maintenance intents:
   - flush when frozen state exists;
   - compaction when L0 or nonzero-level pressure exceeds threshold;
   - materialization when inherited-layer pressure exceeds threshold;
   - checkpoint only through the existing WAL-growth policy.
5. Enqueue intents through the existing deterministic maintenance executor.
6. Coalesce duplicate intents by branch and task scope.
7. Record counters for post-commit evaluations, tasks suggested, tasks
   enqueued, tasks coalesced, tasks deferred, and scheduler-disabled decisions.
8. Ensure L7 commit runtime still only emits facts; it must not call the
   scheduler.

Exit gate:

- A sustained mutating workload schedules maintenance without explicit L9
  maintenance calls.
- Scheduling can be disabled only by explicit lifecycle configuration and the
  disabled decision is visible in facts/counters.

### M4P-L8-B. Complete Flush Drain Policy

Goal: make one flush scheduling event drain all currently eligible frozen state
for the selected scope.

Tasks:

1. Add a flush-drain request that expands from branch/global pressure into one
   or more concrete flush operations.
2. Drain frozen tables before scheduling compaction for the same branch.
3. Iterate branch flushes until no currently eligible frozen table remains.
4. Add a bounded retry when new frozen state appears during the drain.
5. Preserve deterministic ordering across branches.
6. Convert per-flush outcomes into a single drain outcome with completed,
   deferred, failed, and skipped counts.
7. In durable mode, preserve table publication, manifest, and flush-watermark
   facts from the existing flush handler.
8. Report partial progress as health debt, not clean success.
9. Record counters for frozen tables discovered, flush operations completed,
   freeze-during-drain retries, drain failures, and post-drain frozen tables.

Exit gate:

- A workload that rotates many active tables does not require one explicit
  flush task per frozen table from the caller.
- After a flush drain with no injected failure, eligible frozen table count is
  zero for the selected scope.

### M4P-L8-C. Score-Based Compaction Selection And Chaining

Goal: replace fixed or caller-selected compaction with old-style scored
branch/level work selection.

Tasks:

1. Add a compaction score model over branch facts:
   - L0 score from table count against the L0 trigger;
   - L1+ scores from level bytes/table counts against per-level targets;
   - inherited-layer materialization pressure as a separate score class.
2. Score all eligible branches and levels when a compaction chain runs.
3. Pick the highest-scoring eligible unit of work.
4. Convert the selected score into an L6 compaction request:
   - L0 count compaction;
   - L0-to-L1 compaction;
   - selected nonzero-level compaction;
   - metadata-only promotion where L6 exposes a safe candidate.
5. Add deterministic nonzero-level input selection. Use compact pointers,
   round-robin state, or score-local input choice; do not hard-code table index
   zero.
6. Run one compaction operation per task.
7. Re-read source-shape facts after completion.
8. Re-score and resubmit while any score remains unhealthy and queue/defer
   policy allows more work.
9. Coalesce duplicate compaction chains for the same branch/scope.
10. Record selected branch, selected level, score before/after, input table
    count, overlap table count, output table count, output bytes, trivial move
    count, resubmit count, and post-drain level shape.

Exit gate:

- L0 and nonzero-level shape remain bounded through normal L9 writes at 100K,
  1M, 5M, and 10M.
- Explicit fixed-point drain remains available for diagnostics, but is not the
  normal source-shape mechanism.

### M4P-L8-D. Write-Admission Pressure Policy

Goal: make pressure facts affect mutating admission without moving sleeps,
compaction, or product retry policy into L7.

Tasks:

1. Evaluate pressure before mutating commit admission when the lifecycle runtime
   can do so safely.
2. Define documented policy for each severity:
   - healthy/background: accept and optionally enqueue background work;
   - urgent: drive bounded inline maintenance or accept with an
     accepted-under-pressure fact;
   - blocking: reject with typed storage pressure error or drive required
     maintenance before retrying admission;
   - degraded/faulted: reject if accepting would make recovery or durability
     unsafe.
3. Keep L7 branch guard and unresolved-durable gate semantics unchanged.
4. Emit facts that distinguish:
   - accepted cleanly;
   - accepted under pressure;
   - maintenance required before admission;
   - maintenance driven inline;
   - rejected by pressure;
   - rejected by existing commit gate or branch guard.
5. Avoid indefinite waits in L8 V1 unless a separate blocking API decision is
   recorded.
6. Ensure pressure rejection is retryable when maintenance progress can clear
   the condition.
7. Record counters for pressure evaluations, accepted-under-pressure commits,
   inline maintenance attempts, pressure rejects, and pressure-cleared retries.

Exit gate:

- Mutating commits no longer silently accept unbounded LSM pressure unless the
  plan records an explicit no-stall V1 policy and the benchmark proves bounded
  fanout anyway.

### M4P-L8-E. L9 Benchmark And Diagnostics Closeout

Goal: prove the restored maintenance loop through normal L9 workloads.

Tasks:

1. Extend diagnostics and perf-trace snapshots with final queue depth, maximum
   queue depth, scheduled/coalesced/completed task counts, L0 max/final counts,
   per-level table counts, and per-level bytes.
2. Ensure benchmark output separates:
   - load commit time;
   - automatic maintenance time;
   - explicit diagnostic final drain time;
   - point-read throughput after the normal load path.
3. Remove benchmark-specific manual flush/compact loops from the default
   source-shape preparation path after automatic scheduling is proven.
4. Keep an explicit diagnostic drain mode for one-off compaction timing.
5. Run cache and durable-local standard at 100K, 1M, 5M, and 10M.
6. Record results under `benchmarks/results/storage-next-l9/`.
7. Update the follow-up doc or closeout section with the measured decision.

Exit gate:

- 5M and 10M runs reach point-read measurement without a large final
  fixed-point compaction cliff.
- `l0_tables_per_million_rows_after_load` and
  `point_source_probes_per_read` do not scale linearly with key count after
  automatic maintenance has run.

## Post-Verification Follow-Up Slices

Detailed follow-up plan:
`docs/architecture/implementation-plans/M4P/m4p-l8b-lifecycle-maintenance-followup-implementation-plan.md`

Detailed follow-up test plan:
`docs/architecture/implementation-plans/M4P/m4p-l8b-lifecycle-maintenance-followup-test-plan.md`

The L8-A through L8-E implementation closes the immediate maintenance loop,
but the verification review identified additional old-engine parity gaps that
must be either implemented or explicitly deferred before the 5M/10M benchmark
gate can be treated as a full lifecycle parity proof.

Required before benchmark closeout:

1. **L8-F. Level Target Pyramid And Adaptive Targets**
   - Current storage-next scoring uses a fixed nonzero-level target size.
   - Old storage used level-specific target growth and recalculated level
     targets from live shape.
   - Implement level-specific target bytes or record a semantic decision that
     the fixed target is a V1 simplification, then prove that 5M/10M source
     fanout remains bounded anyway.
2. **L8-G. Cross-Branch Maintenance Coverage**
   - Post-commit scheduling starts from the committing branch.
   - Compaction chaining must continue to select the highest-scored live
     branch, but quiet branches with stranded frozen state or table backlog
     still need an explicit coverage policy.
   - Add a fairness/coverage sweep, an idle maintenance pass, or a documented
     V1 deferral with counters showing no quiet-branch backlog in benchmark
     workloads.
3. **L8-H. Compaction IO Rate Limiting**
   - Old storage had a rate limiter for background compaction IO.
   - Storage-next deterministic maintenance currently has no IO throttle.
   - Add a budgeted compaction IO limiter or prove with benchmark counters
     that compaction does not starve writes at 5M/10M.

High-priority decision slices:

1. **L8-I. Nonzero Input Rotation Policy**
   - The current direct and scored nonzero compaction path deterministically
     chooses the largest input table.
   - Decide whether always-largest is the V1 policy or whether storage-next
     should restore compact-pointer or round-robin advancement.
   - Update the mechanical counter test that currently expects selected table
     index variation.
2. **L8-J. Memtable-Bytes Pressure Signal**
   - Current pressure facts emphasize frozen tables and table counts.
   - Old backpressure also considered mutable-memory growth.
   - Add an active/mutable byte signal or record why the storage-next active
     table budget is sufficient without a separate pressure tier.
3. **L8-K. Write Stall Budget And Wake Policy**
   - Current blocking pressure rejects with retryable facts.
   - Decide whether lifecycle owns a bounded wait API, a condition-variable
     wake on pressure clear, or leaves retry policy entirely to L9.
4. **L8-L. Snapshot Floor And Pruning Coupling**
   - Old storage coupled safe-point advancement with maintenance before flush
     and compaction pruning.
   - Storage-next pruning is currently proof-driven per request.
   - Record a semantic decision assigning snapshot-floor ownership to L8 or
     engine-next before enabling broader automatic pruning.
5. **L8-M. Grandparent Overlap Output Splitting**
   - Storage-next tracks deeper overlap facts, but does not yet use them as an
     output split budget.
   - Decide whether to restore old grandparent-overlap split behavior in L8 or
     keep it as an L5/L6 compaction-output enhancement.

Measure-first follow-ups:

1. **L8-N. Pressure Collection Sampling Counters**
   - Add counters for pressure-collection calls and level iterations before
     reintroducing old-style expensive-check sampling.
2. **L8-O. Idle-Round Chain Anchor**
   - Old background maintenance allowed several idle rounds before stopping a
     chain.
   - Keep this deferred unless benchmark or production counters show repeated
     near-immediate resubmission after the branch first appears healthy.
3. **L8-P. Flush Memory Release**
   - Old storage released freed memory after flush.
   - Keep this deferred unless RSS or allocator counters show long-load memory
     retention after frozen tables drain.
4. **L8-Q. Pressure-Clear Wake Signal**
   - A wake signal is useful only if L8/L9 grows a blocking wait API.
   - Keep it deferred while pressure rejection remains fail-fast and retryable.

## Expected Counter Movement

Required counter movement after implementation:

1. `maintenance_tasks_enqueued` increases during normal load without explicit
   L9 maintenance calls.
2. `maintenance_tasks_coalesced` is nonzero under bursty commit workloads.
3. `flush_tasks_completed` tracks frozen table creation and post-drain frozen
   table count returns to zero when no fault is injected.
4. `compaction_tasks_completed` tracks L0 and nonzero-level pressure rather
   than only explicit maintenance calls.
5. `compaction_tasks_per_flush_task` is high enough to keep L0 bounded but not
   unbounded due to duplicate chains.
6. `l0_tables_max_during_load` may spike, but `l0_tables_final` and
   `l0_tables_per_million_rows_after_load` should stay bounded by configured
   thresholds.
7. `owned_level_table_counts` should show data advancing beyond L0/L1 at large
   scales.
8. `maintenance_queue_depth_final` should be zero or explainable by typed
   deferred/failure facts.
9. `commit_admission_under_pressure` may rise during load, but
   `commit_admission_requires_maintenance` should correlate with scheduled or
   driven maintenance.
10. Point-read counters after load should stay bounded by active/frozen/L0 plus
    one table per nonzero level, not total flushed table count.

## Stop Conditions

Stop and revise the plan if:

1. implementing post-commit scheduling requires L7 to call lifecycle code
   directly instead of emitting facts to its caller;
2. bounded fanout cannot be achieved without changing L5 compaction output or
   L6 branch compaction semantics;
3. durable table rewrite publication needs new byte formats not covered by
   L8Q-L8U;
4. admission policy cannot be expressed without public L9 blocking/retry API
   decisions;
5. the 5M/10M benchmark remains dominated by table compaction hot-loop counters
   after source shape is bounded, indicating the bottleneck moved back to L5;
6. deterministic in-process execution cannot make progress without a required
   background thread.

## Verification Commands

Focused commands:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --locked --test api_conformance
```

Full storage-next gates:

```bash
cargo fmt --package strata-storage-next --check
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo test -p strata-storage-next --all-targets --all-features --locked
cargo test -p strata-storage-next --no-default-features --features testkit --locked
```

Benchmark proof:

```bash
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- \
  --scales 100k,1m,5m,10m \
  --engines cache,standard \
  --workloads load-seq,point-latest,point-throughput \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000 \
  --progress
```

## Completion Criteria

M4P-L8 lifecycle maintenance parity is complete when:

1. post-commit maintenance scheduling is automatic, coalesced, and observable;
2. flush scheduling drains all eligible frozen state for the selected scope;
3. compaction scheduling scores branches/levels, picks highest-pressure work,
   and chains until healthy;
4. write admission consumes pressure facts and either drives maintenance,
   accepts with typed facts, or rejects with typed retryable storage errors;
5. cache and durable-local standard sustain 100K, 1M, 5M, and 10M L9 loads
   without benchmark-specific manual maintenance to control fanout;
6. source-shape counters prove L0/source fanout is bounded after load;
7. source guards prevent L8 from importing product, benchmark-only, or lower
   layer implementation shortcuts;
8. durable lifecycle hardening that is not part of this critical path remains
   linked to L8Q-L8Z or the deferred-work ledger.
