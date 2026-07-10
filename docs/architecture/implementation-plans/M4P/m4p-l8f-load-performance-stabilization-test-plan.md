# M4P-L8F Test Plan: Load Performance Stabilization

Status: draft

Corrective note:
`docs/architecture/implementation-plans/M4P/m4p-l8g-cache-mode-lifecycle-policy-test-plan.md`
supersedes the cache-mode benchmark gates in this plan. The visibility and
counter tests here remain useful, but cache mode must first prove absence of
source/table lifecycle maintenance, admission pressure, background scheduling,
flushes, and compaction for ordinary writes.

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8f-load-performance-stabilization-implementation-plan.md`

Parent test plans:

1. `docs/architecture/implementation-plans/M4P/m4p-l8-lifecycle-maintenance-parity-test-plan.md`
2. `docs/architecture/implementation-plans/M4P/m4p-l8e-background-maintenance-executor-test-plan.md`

## Goal

Prove that storage-next load performance improves because storage mechanics are
correct:

1. urgent admission throttles only when background progress is insufficient;
2. L0 compaction drains old-engine-sized episodes;
3. nonzero level scoring and selection reduce rewrite amplification;
4. benchmark output exposes source shape and queue state for load-only runs.

The suite must fail if the implementation:

1. restores throughput by turning off pressure safety;
2. hides Block errors with benchmark retries;
3. keeps the per-mutation urgent slowdown floor;
4. continues four-table L0 churn;
5. rewrites final-level tables during ordinary write-pressure chains;
6. cannot explain benchmark results with counters.

## Test Matrix

| Area | Required Proof | Failure Caught |
| --- | --- | --- |
| Benchmark visibility | Load-only runs record source shape and queue facts. | Large-run result cannot explain L0 debt, final shape, or queue growth. |
| Admission throttle | Urgent slowdown depends on relief/no-relief, not mutation count. | 1000-row batches pay steady 25ms sleeps under normal progress. |
| Block wait | Block pressure waits with deadline and typed failure. | Infinite wait, silent accept under Block, or benchmark retry dependency. |
| L0 episode width | L0-to-L1 selects all snapshot L0 inputs and overlaps. | Repeated four-table churn and recurring urgent pressure. |
| Concurrent flush safety | Flushes racing compaction publication survive as newer L0 tables. | Lost tables, reordered source precedence, stale output install. |
| Nonzero targets | Level target helper matches old segmented target calculation. | Static output-file target used as level pressure target. |
| Nonzero selection | Compact pointer chooses one non-final input and wraps. | Same table repeatedly selected; tiny churn under write load. |
| Metadata move | Safe no-overlap nonzero compaction avoids rewrite. | Unnecessary table rewrite and excess amplification. |
| Final-level policy | Write-pressure chains do not consolidate final level. | Rows rewritten solely to reduce final-level read fanout during load. |
| Closed-loop load | Scaled sustained load stays live with bounded shape and amplification. | Regression only appears in manual 5M/10M benchmarks. |

## Benchmark Visibility Tests

Correctness tests:

1. Load-only run with observed source-shape diagnostics records:
   - `post_load_source_shape`;
   - active rows;
   - frozen table count;
   - owned L0 table count;
   - owned nonzero level table counts;
   - maintenance queue facts.
2. Load-only run without diagnostic flags does not run explicit flush or
   compact after timing.
3. Load-only run with final-drain diagnostics records observed shape and
   post-drain shape separately.
4. Final-drain diagnostics do not change load elapsed time.
5. A source-layout unavailable outcome is a benchmark failure when the user
   requested source-shape diagnostics.

Source guards:

1. The timed load loop must not call `MaintenanceTask::Compact`.
2. The timed load loop must not call diagnostics unless an explicit diagnostic
   option was selected.
3. Shape metrics for load results must not depend on a later read workload.
4. Benchmark result JSON must include row and byte amplification fields.

Counter tests:

1. Synthetic perf snapshots produce correct row amplification:
   `compaction_input_rows / operation_count`.
2. Synthetic perf snapshots produce correct byte amplification:
   `compaction_input_bytes / logical_written_bytes`.
3. Queue facts preserve `pending`, `active`, `max_pending`, `completed`,
   `coalesced`, `deferred`, and `failed`.

## Admission Throttle Tests

Correctness tests:

1. Healthy pressure accepts without slowdown.
2. Background severity accepts without slowdown.
3. Urgent pressure with observed background progress records
   accepted-under-pressure and applies no delay or a delay at or below 1ms.
4. Urgent pressure with no relief for consecutive rounds escalates delay.
5. Relief resets no-relief rounds and reduces or clears delay.
6. Batch size does not create a linear slowdown floor:
   - same pressure, one mutation;
   - same pressure, 1000 mutations;
   - 1000-mutation delay must not be 1000x the one-mutation delay.
7. Slowdown uses the injected maintenance clock in deterministic inline mode.
8. Public background mode never runs full maintenance inline from the urgent
   slowdown path.
9. Slowdown counters record attempts, nanoseconds, no-relief escalations, and
   relief resets.
10. Block pressure waits for background progress until deadline.
11. Block wait returns typed pressure failure after no relief and deadline.
12. Block wait exits early when background progress clears pressure.

Generated tests:

1. Random sequences of pressure scores and completed-task counts.
2. Random batch sizes from 1 to 10,000 under identical pressure.
3. Random no-relief streaks with manual clock advancement.
4. Random alternation between urgent, background, healthy, and block severity.

Pass gates:

1. No test observes a fixed 25ms floor for 1000-row urgent batches.
2. No test requires inline compaction to clear urgent pressure.
3. Deterministic replay of the same pressure/progress script yields the same
   slowdown sequence.

## L0 Episode Width Tests

Correctness tests:

1. A branch with 1 L0 table plans 1 L0 input.
2. A branch with 4 L0 tables plans 4 L0 inputs.
3. A branch with 17 L0 tables plans 17 L0 inputs.
4. L0-to-L1 includes all overlapping L1 tables for the full L0 key range.
5. L0-to-L1 excludes non-overlapping L1 tables.
6. L0-to-L1 output preserves latest-value reads.
7. L0-to-L1 output preserves history reads.
8. L0-to-L1 output preserves timestamp reads.
9. L0-to-L1 output preserves range scans.
10. Empty L0 returns a no-candidate outcome.
11. If snapshotted L0 inputs are stale at publication, the task defers and
    does not remove unrelated tables.
12. If a concurrent flush publishes while L0-to-L1 is building, the flush table
    remains in L0 and wins source precedence as newer data.
13. Metadata promotion is not used for multi-input L0-to-L1 unless the promotion
    proof is extended and explicitly tested.

Counter tests:

1. Selected L0 input count equals the snapshot L0 count.
2. Selected L0 overlap count matches the overlapping L1 fixture.
3. Completed compaction input table counters match the planned candidate.
4. Stale L0-to-L1 publication increments stale/deferred counters.

Generated tests:

1. Random L0 table counts from 0 to 64.
2. Random key-range overlap layouts between L0 and L1.
3. Random concurrent flush insertion points during unlocked build.
4. Random stale publication after branch deletion or replacement.

Pass gates:

1. There is no four-table hard limit in L0-to-L1 planning.
2. Concurrent flush safety is covered by a deterministic interleaving test.
3. Read equivalence is checked for point, scan, history, and timestamp paths.

## Nonzero Shape Tests

Level target tests:

1. Empty nonzero levels match the old segmented target calculation.
2. A shallow L1-only layout matches the old target calculation.
3. A deep bottom-heavy layout raises base level like the old calculation.
4. A tiny deep layout clamps to min base.
5. A very large base clamps to max base.
6. L0 bytes do not affect nonzero level target calculation.
7. Target file size does not replace level pressure target bytes.
8. Level target facts are recorded per scored level.

Selection tests:

1. Compact pointer selects the first eligible nonzero table.
2. Successful publication advances the pointer.
3. Pointer wraps after the last table.
4. Pointer skips missing/stale tables safely.
5. Selected nonzero table includes all overlapping next-level tables.
6. Selected nonzero table records grandparent/deeper overlap bytes.
7. Safe no-overlap nonzero work uses metadata promotion/trivial move.
8. Unsafe no-overlap work falls back to table rewrite when retention or pruning
   policy requires rewrite.
9. Nonzero compaction re-scores after one operation.

Final-level policy tests:

1. Ordinary write-pressure scoring ignores final configured level table-count
   pressure.
2. Explicit compact can request final-level consolidation.
3. Close-required drain can request final-level consolidation if close policy
   marks it required.
4. Low-priority read-shape consolidation is distinguishable from write-pressure
   compaction in counters.
5. Final-level consolidation never drives urgent write-admission slowdown.

Generated tests:

1. Random per-level byte layouts compared against the old target function.
2. Random compact-pointer positions and table deletions.
3. Random overlap layouts for level N to level N+1.
4. Random final-level table counts under write-pressure and explicit compact.

Pass gates:

1. Nonzero target tests use the same expected values as the old segmented
   target algorithm for equivalent fixtures.
2. Automatic write-pressure chains do not rewrite final-level rows solely for
   fanout reduction.
3. Metadata move coverage proves rewrite avoidance on safe no-overlap work.

## Closed-Loop Liveness Tests

Use scaled constants so the same trajectories occur at about 50K rows.

Correctness tests:

1. Sustained sequential load with background workers completes without
   permanent commit failure.
2. L0 table count remains below urgent threshold after background drain.
3. Nonzero table count remains bounded after background drain.
4. Maintenance queue reaches zero or a documented low-priority read-shape task.
5. Admission slowdown remains below the scaled gate.
6. Block waits remain zero unless the fixture intentionally creates no-relief
   pressure.
7. WAL retained bytes and segments remain bounded in durable local mode.
8. Point reads after load return all sampled latest values.
9. Range scans after load return sorted expected rows.

Generated tests:

1. Random write batch sizes.
2. Random background worker counts from 1 to 4.
3. Random flush rotation thresholds.
4. Random level target scales.
5. Random injected background delays.

Pass gates:

1. No permanent commit failure in the healthy closed-loop load fixture.
2. Slowdown appears only in no-relief or intentionally overloaded scripts.
3. Rewrite amplification in the scaled fixture stays below 4x.

## Benchmark Gates

Run storage-next and old engine cache load benchmarks sequentially, one scale
at a time, with no resource splitting:

```text
cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l9-scale -- \
  --scales 100k \
  --engines cache \
  --workloads load-seq \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000 \
  --diagnostic-source-shape

cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l9-scale -- \
  --scales 1m \
  --engines cache \
  --workloads load-seq \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000 \
  --diagnostic-source-shape

cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l9-scale -- \
  --scales 5m \
  --engines cache \
  --workloads load-seq \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000 \
  --diagnostic-source-shape

cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l9-scale -- \
  --scales 10m \
  --engines cache \
  --workloads load-seq \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000 \
  --diagnostic-source-shape
```

Repeat the same scales with `storage-old-cache-scale` for same-machine
comparison.

Hard gates:

1. Every storage-next run completes.
2. `lifecycle_write_admission_pressure_rejects == 0`.
3. `lifecycle_write_admission_wait_timeouts == 0`.
4. 10M `lifecycle_write_admission_slowdown_ns <= 30_000_000_000`.
5. 10M row rewrite amplification is at most 4x.
6. 10M byte rewrite amplification is at most 4x.
7. Observed post-load source shape is known.
8. Observed post-load L0 table count is below urgent threshold.
9. 10M storage-next cache throughput is at least 150K ops/s.
10. 10M storage-next cache throughput is no worse than 2x old engine cache on
    the same machine.

Soft diagnostic targets:

1. Background task time may remain high, but foreground commit time should no
   longer be dominated by admission sleep.
2. Coalesced wake count should remain bounded relative to commits.
3. Metadata-promotion bytes avoided should increase on no-overlap nonzero
   fixtures.
4. Table merge ns/row should not regress by more than 10%.

## Regression Commands

Required before closeout:

```text
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test -p strata-storage-next lifecycle_load_performance
cargo test -p strata-storage-next lifecycle_background_closed_loop
```

The named test filters are required source-level gates. If the actual test
modules use different names, keep the names descriptive and add a README entry
so future runs are discoverable.

## Source Guards

1. No benchmark load loop may contain an explicit compact call.
2. No storage-next admission path may multiply urgent slowdown by mutation
   count.
3. No L0-to-L1 planner may contain a fixed four-table input cap.
4. No write-pressure scorer may classify final configured level consolidation
   as urgent write pressure.
5. No production drive logic may call `Instant::now()` directly.
6. No public background mode may run full maintenance inline after commit.

## Failure Interpretation

1. If slowdown remains above the hard gate, inspect admission no-relief
   counters before touching compaction.
2. If slowdown passes but rewrite amplification remains above 4x, continue
   compaction-shape work.
3. If slowdown and amplification pass but throughput remains below 150K ops/s,
   inspect commit runtime counters and table merge ns/row.
4. If table merge ns/row regresses, move to table hot-path work and do not
   change lifecycle scheduling to compensate.
5. If source shape is unknown, benchmark output is invalid for this gate.
