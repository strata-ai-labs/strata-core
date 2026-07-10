# M4P-L8 Test Plan: Lifecycle Maintenance Parity

Status: draft

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8-lifecycle-maintenance-parity-implementation-plan.md`

Parent test methodology:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-test-plan.md`

Architecture context:
`docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`

Audit context:
`docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`

## Goal

Prove that storage-next L8 automatically keeps branch LSM source shape healthy
during normal writes.

The tests must catch:

1. mutating commits completing without scheduling needed maintenance;
2. flush pressure requiring one external maintenance call per frozen table;
3. compaction tasks always choosing shallow or fixed table-index work;
4. L0 and nonzero-level source fanout growing linearly with total rows;
5. write admission accepting blocking pressure silently;
6. maintenance changing visible rows;
7. durable maintenance failures being reported as clean success;
8. L9 benchmarks relying on manual fixed-point drains for normal source shape;
9. L8 importing L5/L6 internals, product vocabulary, or benchmark-only flags.

## Test Matrix

| Area | Required proof | Failure caught |
| --- | --- | --- |
| Post-commit scheduling | Mutating commits evaluate pressure and enqueue/coalesce tasks. | Normal writes strand backlog until explicit maintenance. |
| Flush drain | One scheduling event drains all eligible frozen state for scope. | Caller must enqueue one flush per frozen table. |
| Compaction scoring | Highest-pressure branch/level is selected and re-scored after each operation. | L0 drains shallowly or nonzero levels always pick table index zero. |
| Write admission | Pressure severity produces documented accept/drive/reject facts. | Blocking pressure is ignored or product wording leaks into storage errors. |
| Read parity | Reads/scans/history match before and after scheduled maintenance. | Scheduling corrupts branch visibility or ordering. |
| Durable facts | Publication failures create health debt and preserve recoverability. | Partial progress is reported as clean success. |
| Generated workloads | Random sustained writes remain bounded after automatic maintenance. | Hand-written examples miss oscillation, multi-branch, or failure cases. |
| Benchmarks | 100K/1M/5M/10M L9 runs complete without manual source-shape drains. | The measured path is still benchmark-specific. |

## Correctness Tests

### Post-Commit Scheduling

1. Blind put in cache mode with no pressure does not enqueue unnecessary work.
2. Blind put that rotates the active table evaluates pressure after commit.
3. Multiple commits that create frozen state enqueue exactly one coalesced flush
   drain per branch/scope.
4. Flush pressure is scheduled before compaction pressure for the same branch.
5. L0 pressure enqueues compaction after flushable state is absent or drained.
6. Nonzero-level pressure enqueues compaction with the selected level recorded.
7. Inherited-layer pressure enqueues materialization work only through L6
   materialization facts.
8. Scheduler-disabled configuration records a disabled fact and does not enqueue
   tasks.
9. L7 commit tests continue to show no direct lifecycle scheduler calls from
   commit runtime.

### Flush Drain

1. One branch with many frozen tables drains all currently eligible frozen
   tables from one coalesced flush request.
2. Multiple branches drain in deterministic order.
3. A branch with no frozen state returns a skipped/no-op outcome, not a false
   success with completed work.
4. Freeze-during-drain triggers bounded retry and records retry count.
5. Freeze-during-drain beyond the retry limit records deferred work and leaves a
   visible maintenance fact.
6. Durable flush publication failure records partial-progress health debt.
7. Durable flush install failure leaves visible reads unchanged.
8. Flush drain preserves latest, historical, tombstone, and TTL-visible rows.
9. Flush drain does not advance global flush watermark without the existing
   table-manifest or checkpoint proof.

### Compaction Scoring And Chaining

1. L0 below threshold produces no compaction task.
2. L0 at threshold produces a compaction score above target.
3. Larger L0 pressure beats smaller nonzero pressure.
4. Larger nonzero-level pressure beats smaller L0 pressure when its score is
   higher.
5. Multi-branch scoring picks the highest-scoring branch first.
6. After one completed compaction, the scheduler re-reads facts before
   scheduling the next task.
7. Compaction chain stops when all scores are healthy.
8. Compaction chain defers when queue policy or fault injection blocks the next
   task.
9. Nonzero-level compaction does not always choose table index zero when another
   table is the deterministic selected input.
10. Round-robin or compact-pointer state advances deterministically when used.
11. L0-to-L1 overlap and non-overlap cases preserve sorted non-overlapping L1
    output.
12. L1-to-L2 and deeper compactions preserve sorted non-overlapping target
    levels.
13. Metadata-only promotion, when available from L6, avoids byte rewrite and
    records the promotion count.
14. Compaction output does not split one physical-key version chain.
15. Reads, scans, and history match the pre-compaction model after every
    scheduled compaction.

### Write Admission Pressure

1. Healthy pressure admits mutating commit and records clean admission.
2. Background pressure admits commit and enqueues or coalesces maintenance.
3. Urgent pressure either drives bounded inline maintenance or records
   accepted-under-pressure according to policy.
4. Blocking pressure rejects before avoidable mutating work or drives required
   maintenance before retrying admission.
5. Pressure rejection is typed, retryable when maintenance can clear it, and
   uses storage vocabulary.
6. Existing L7 unresolved-durable gate rejection remains distinguishable from
   pressure rejection.
7. Existing L7 branch guard rejection remains distinguishable from pressure
   rejection.
8. Faulted durable maintenance health can block admission when accepting a
   commit would make recovery unsafe.
9. Cache mode and durable-local standard report equivalent branch-local
   pressure for the same logical source shape after durable-only facts are
   ignored.

### Materialization Scheduling

1. Inherited-layer pressure schedules a materialization task only when L6 facts
   indicate materialization is legal.
2. Local child rows shadow inherited rows before and after materialization.
3. Parent compaction before child materialization does not change child-visible
   rows.
4. Materialization failure records health debt and leaves inherited reads
   unchanged.
5. Duplicate materialization requests coalesce by branch/layer scope.

## Fault And Failure Tests

1. Fault before enqueue leaves no queued task and records enqueue failure.
2. Fault after enqueue before start keeps the queued task retryable or records
   deferred work.
3. Fault at task start records started and failed counters exactly once.
4. Flush publication failure in durable mode leaves branch reads unchanged or
   records the existing partial-progress state.
5. Compaction publication failure leaves old tables installed.
6. Compaction install failure leaves old level layout readable.
7. Later compaction output failure handles earlier unpublished artifacts through
   the existing lifecycle failure path.
8. Materialization failure preserves inherited-layer status.
9. Admission pressure maintenance-drive failure returns a typed admission or
   maintenance error without allocating a commit version when rejection happens
   before L7 admission.
10. Queue-full during post-commit scheduling records deferred maintenance and
    does not fail an already-visible commit.

## Generated Tests

Add or extend generated lifecycle workloads with:

1. random commits that rotate active state at different batch sizes;
2. random flush thresholds and output target sizes;
3. random branch counts from one through at least sixteen;
4. random L0 overlap/non-overlap with L1;
5. random L1+ overlap/non-overlap across adjacent levels;
6. random same-physical-key version chains near output split boundaries;
7. random inherited-layer materialization opportunities;
8. random task enqueue/coalesce/cancel/failure points;
9. random pressure oscillation around background, urgent, and blocking
   thresholds;
10. repeated scheduled maintenance under small output target bytes.

Generated invariants:

1. Reads before and after maintenance match the independent model.
2. Output tables remain sorted and non-overlapping where the level requires it.
3. L0 table count after a full automatic-maintenance drain is bounded by the
   configured threshold plus explicitly deferred work.
4. Nonzero-level table probes for point reads are bounded by level count, not
   table count.
5. No workload depends on manual fixed-point drain for correctness.
6. Cache and durable-local standard produce equivalent branch-local source
   shape when no durable fault is injected.

## Mechanical Counter Tests

Only use perf-gated assertions for mechanical tests.

Required assertions:

1. Post-commit scheduler evaluations increment for mutating commits and do not
   increment for read-only diagnostics.
2. Scheduler-disabled configuration increments disabled counters and enqueues
   no tasks.
3. Duplicate pressure events increment coalesced counters.
4. Flush drain completes more than one concrete flush operation from one
   coalesced request when multiple frozen tables exist.
5. Post-drain frozen table count is zero when no failure is injected.
6. Compaction scoring records scored branch/level candidates.
7. Selected compaction level matches the highest score in deterministic
   fixtures.
8. Compaction chain resubmit count is nonzero for unhealthy multi-level
   fixtures.
9. Nonzero-level selected table index varies in fixtures designed to advance
   compact-pointer or round-robin state.
10. L0 final count is bounded after automatic maintenance.
11. `point_source_probes_per_read` does not scale with total flushed table
    count after automatic maintenance.
12. Queue final depth is zero or matches explicit deferred/failure facts.
13. Pressure rejection counters remain distinct from L7 branch guard and
    unresolved-durable gate counters.
14. Durable failure tests increment maintenance health-debt counters.

## Follow-Up Gate Tests

Detailed follow-up implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8b-lifecycle-maintenance-followup-implementation-plan.md`

Detailed follow-up test plan:
`docs/architecture/implementation-plans/M4P/m4p-l8b-lifecycle-maintenance-followup-test-plan.md`

Before treating the 5M/10M benchmark as full lifecycle parity proof, add or
explicitly defer the following tests:

1. Level-target pyramid tests prove that nonzero-level target bytes grow by
   level, or the fixed-target decision is recorded and benchmark source fanout
   remains bounded.
2. Multi-branch coverage tests create one active committing branch and at least
   one quiet branch with stranded frozen or table backlog; automatic
   maintenance must either clear both or record the documented V1 deferral.
3. Compaction IO-rate tests record load throughput and compaction bytes under
   sustained maintenance so an IO throttle can be justified or skipped.
4. Nonzero input policy tests assert the chosen V1 behavior: either
   compact-pointer/round-robin variation or deterministic largest-input
   selection with no hardcoded table-zero fallback.
5. Memtable-byte pressure tests grow active mutable bytes without crossing
   frozen-table thresholds and assert whether the pressure model reports a
   distinct mutable-memory condition.
6. Write-stall policy tests assert one documented behavior: fail-fast retryable
   rejection, bounded wait, or condition-variable wake on pressure clear.
7. Snapshot-floor tests assert that automatic pruning never advances beyond a
   retained snapshot unless the lifecycle or engine-next ownership decision
   provides a proof.
8. Grandparent-overlap tests use deeper-level overlap fixtures to assert either
   split-budget enforcement or a documented lower-layer deferral.
9. Pressure-collection sampling tests record pressure collection calls and
   level-iteration counters before adding old-style sampling intervals.
10. Memory-release tests measure post-flush RSS or allocator facts before
    requiring an explicit freed-memory release hook.

## Source Guards

Add or update source guards to reject:

1. roadmap labels such as `L8A`, `L8B`, or `M4P` in production Rust code,
   comments, panic messages, fixture bytes, or user-visible strings;
2. product, engine, IPC, StrataHub, follower, or primitive modules in lifecycle
   production code;
3. direct `std::fs`, `Path`, `File`, mmap, or environment access in lifecycle
   production code;
4. benchmark-only flags or benchmark module imports in production lifecycle
   code;
5. L8 compaction code importing private table merge internals instead of L5/L6
   public crate-internal APIs;
6. lower layers importing `crate::lifecycle`.

## Benchmark Plan

Required command:

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

Required metadata:

1. git revision;
2. machine and target architecture;
3. storage mode and durability policy;
4. backend kind and localfs feature state;
5. lifecycle scheduler mode;
6. pressure thresholds;
7. L0/nonzero level compaction thresholds;
8. block/cache/table budget sizes;
9. maintenance queue depth;
10. perf-trace enabled state.

Required derived metrics:

1. `load_maintenance_ms_per_million_rows`;
2. `l0_tables_per_million_rows_after_load`;
3. `compaction_tasks_per_flush_task`;
4. `point_source_probes_per_read`;
5. `point_nonzero_table_probes_per_read`;
6. `maintenance_queue_depth_final`;
7. `maintenance_deferred_tasks_per_million_rows`;
8. old-to-new load throughput ratio;
9. old-to-new point-latest throughput ratio;
10. old-to-new point-throughput ratio.

Pass conditions:

1. 100K, 1M, 5M, and 10M cache runs complete load and point-read phases without
   benchmark-specific manual source-shape drain.
2. 100K, 1M, 5M, and 10M durable-local standard runs complete load and
   point-read phases without benchmark-specific manual source-shape drain.
3. 5M and 10M do not spend minutes in final fixed-point prep before point reads.
4. Final L0 table count does not grow linearly with total row count.
5. Point-source probes after load remain explainable by level shape.
6. Any remaining slowdown is attributable to measured L5/L6 counters, not
   unbounded L8 maintenance backlog.

## Verification Commands

Focused:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --locked --test api_conformance
```

Full storage-next:

```bash
cargo fmt --package strata-storage-next --check
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo test -p strata-storage-next --all-targets --all-features --locked
cargo test -p strata-storage-next --no-default-features --features testkit --locked
```

## Closeout Checklist

M4P-L8 test closeout requires:

1. all focused tests pass;
2. all storage-next all-feature and no-default-feature gates pass;
3. source guards pass;
4. generated lifecycle workloads include automatic maintenance pressure cases;
5. benchmark JSON for 100K, 1M, 5M, and 10M is stored;
6. benchmark results separate automatic maintenance from explicit diagnostic
   drains;
7. implementation plan expected counter movement is observed or explained;
8. any deferred durable lifecycle work is linked to L8Q-L8Z or the deferred-work
   ledger, not left as an implicit gap.
