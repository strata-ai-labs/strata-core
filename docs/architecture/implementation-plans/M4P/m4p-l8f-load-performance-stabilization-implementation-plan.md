# M4P-L8F Implementation Plan: Load Performance Stabilization

Status: draft

Corrective note:
`docs/architecture/implementation-plans/M4P/m4p-l8g-cache-mode-lifecycle-policy-implementation-plan.md`
supersedes the cache-mode closeout interpretation in this plan. The counters
added here remain useful, but the corrected diagnosis is that storage-next
cache mode should not inherit source/table lifecycle maintenance, admission
pressure, background scheduling, flushes, or compaction for ordinary writes.
Do not continue cache benchmark tuning from this plan until the cache lifecycle
policy boundary is audited and cleaned up.

Parent implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8-lifecycle-maintenance-parity-implementation-plan.md`

Predecessor plans:

1. `docs/architecture/implementation-plans/M4P/m4p-l8e-background-maintenance-executor-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4P/m4p-l8e-background-maintenance-executor-test-plan.md`

Follow-up test plan:
`docs/architecture/implementation-plans/M4P/m4p-l8f-load-performance-stabilization-test-plan.md`

## Objective

Restore storage-next sustained load performance without weakening storage
correctness, adding benchmark retry loops, or hiding pressure failures behind
large-run special cases.

The background executor is now present and stable, but the 100K-10M cache load
benchmark still shows a 5x-6x gap versus the old engine at large sizes. The
gap is not a worker-count problem. It is caused by:

1. foreground urgent-admission slowdown applied as a steady-state per-mutation
   tax;
2. excessive compaction amplification from storage-next compaction-shape
   differences;
3. benchmark diagnostics that do not always capture final source shape for
   load-only runs.

This slice fixes those mechanics. The runtime must remain live under sustained
load because admission policy and maintenance shape are correct, not because
the benchmark retries failed commits or skips compaction work.

## Measured Baseline

Benchmark commit:
`9017953d Stabilize storage-next background maintenance`

Command shape:

```text
cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l9-scale -- \
  --scales 100k,1m,5m,10m \
  --engines cache \
  --workloads load-seq \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000
```

Storage-next cache results:

| Scale | Ops/s | Commit Time | Admission Slowdown | Background Task Time | Compaction Input Rows |
| --- | ---: | ---: | ---: | ---: | ---: |
| 100K | 678,708 | 0.122s | 0s | 0s | 0 |
| 1M | 89,090 | 10.873s | 7.998s | 4.678s | 1,888,063 |
| 5M | 54,059 | 91.168s | 72.093s | 85.675s | 55,504,877 |
| 10M | 51,092 | 193.116s | 152.160s | 183.860s | 110,058,912 |

Old engine cache results:

| Scale | Ops/s | Commit Call Time |
| --- | ---: | ---: |
| 100K | 522,552 | 0.171s |
| 1M | 402,412 | 2.293s |
| 5M | 333,233 | 14.030s |
| 10M | 270,984 | 34.923s |

10M storage-next facts:

1. `lifecycle_write_admission_slowdown_ns=152160100000`.
2. `lifecycle_write_admission_slowdown_attempts=4980`.
3. `lifecycle_write_admission_block_wait_ns=0`.
4. `lifecycle_write_admission_wait_timeouts=0`.
5. `lifecycle_background_task_total_ns=183859910778`.
6. `lifecycle_compaction_elapsed_ns=148158523954`.
7. `table_compaction_merge_ns=49748038126`.
8. `table_compaction_merge_input_rows=107257403`.
9. `lifecycle_compaction_input_rows=110058912`.
10. `lifecycle_compaction_input_bytes=30537606527`.
11. `lifecycle_compaction_output_bytes=30537518012`.
12. `lifecycle_post_commit_maintenance_tasks_enqueued=349`.
13. `lifecycle_post_commit_maintenance_tasks_coalesced=7353`.

The direct conclusions are:

1. Removing the inline compaction tax was not sufficient because urgent
   admission now sleeps for most of the run.
2. Pure table merge cost is large but not the whole gap: the merge loop is
   about 49.75s at 10M, while foreground slowdown alone is 152.16s.
3. Compaction write amplification is too high for append-only load:
   storage-next rewrites about 11x the logical row count at 10M.

## Old Source Map

Old engine sources to preserve as invariants:

1. `crates/engine/src/database/config.rs`
   - `write_buffer_size=128MiB`;
   - `target_file_size=64MiB`;
   - `level_base_bytes=256MiB`;
   - `background_threads=min(4, available_parallelism)`;
   - `l0_slowdown_writes_trigger=0`;
   - `l0_stop_writes_trigger=0`.
2. `crates/engine/src/database/transaction.rs`
   - slowdown/stall policy is disabled by default;
   - when enabled, slowdown is per write call and bounded, not linear in row
     count;
   - stall waits on background progress instead of depending on benchmark
     retry loops.
3. `crates/storage/src/segmented/compaction.rs`
   - L0 score is count-based;
   - L0-to-L1 compaction snapshots all current L0 files plus overlapping L1;
   - nonzero compaction picks one file by compact pointer, merges overlaps,
     and re-scores after one operation;
   - trivial metadata moves avoid rewrite when safe;
   - last level is not compacted further by ordinary write-pressure scoring.
4. `crates/storage/src/segmented/mod.rs`
   - `L0_COMPACTION_TRIGGER=4`;
   - target file size is distinct from per-level target bytes;
   - level targets are derived from level sizes, min base, max base, and
     multiplier rather than from a fixed output-file target.

Storage-next target sources:

1. `crates/storage-next/src/api/runtime.rs`
   - background admission slowdown and block-wait policy.
2. `crates/storage-next/src/api/options.rs`
   - background worker defaults and future compaction-shape options if needed.
3. `crates/storage-next/src/lifecycle/compaction.rs`
   - pressure scoring, level targets, rewrite selection, chain resubmit.
4. `crates/storage-next/src/branch/state/compaction.rs`
   - L0/L1/nonzero compaction planning and publication semantics.
5. `crates/storage-next/src/observability/perf_trace.rs`
   - counters needed to prove lower foreground tax and lower amplification.
6. `benchmarks/src/bin/storage_next_l9_scale.rs`
   - timed load metrics, source-shape diagnostics, benchmark gates.

## Required Invariants

1. No benchmark retry loop is added for load benchmarks.
2. No large-scale special case may skip flush, compaction, checkpoint, or
   publication work.
3. Public background mode remains the product default.
4. Block pressure remains a typed admission outcome with a bounded wait
   deadline.
5. Urgent pressure may slow writers, but it must not impose a steady per-row
   tax while background maintenance is making progress.
6. L0-to-L1 compaction drains all current L0 inputs in the selected snapshot,
   preserving the old non-overlapping L1 invariant.
7. Same-branch compaction publication remains serialized.
8. Concurrent flushes that arrive during an L0-to-L1 build are preserved and
   remain newer than the compaction output.
9. Table output target size remains a file/output-shaping concern; nonzero
   level pressure target bytes remain a level-shape concern.
10. Automatic write-pressure compaction does not rewrite the final configured
    level solely because it can reduce read fanout. Final-level consolidation
    is explicit, low-priority read-shape work unless a separate planned policy
    makes it necessary.
11. The simulation boundary from the background executor slice remains intact:
    production drive logic uses the executor trait and injected clock.

## Scope Summary

| Group | Required Work | Exit Gate |
| --- | --- | --- |
| A. Benchmark Visibility | Capture load-only source shape and per-kind compaction counters. | Every load result records L0, nonzero table counts, queue depth, admission, WAL, and rewrite amplification. |
| B. Adaptive Admission Throttle | Replace per-mutation urgent slowdown with progress-aware throttling. | Sustained urgent pressure slows only when background progress is insufficient; no per-1000-row 25ms floor. |
| C. L0 Episode Width | Make L0-to-L1 compact all current L0 inputs plus overlaps. | L0 pressure drains in old-engine-sized episodes and preserves concurrent flush safety. |
| D. Nonzero Shape Policy | Port the old level-target and nonzero selection invariants. | Rewrite amplification falls and nonzero compaction stops doing tiny churn under write load. |
| E. Benchmark Closeout | Rerun 100K-10M cache and old-engine comparison. | Storage-next reaches large-size load without Block failure, with bounded shape and materially lower slowdown. |

## Implementation Order

Execute the slice in this order. Do not jump to large benchmark tuning before
the source-shape and counter gates can explain the result.

1. **Benchmark visibility first**
   - Add load-only source-shape diagnostics and row/byte amplification fields.
   - Add per-kind compaction counters and selected-input counters.
   - Add source guards proving the timed load loop does not run explicit
     compaction or final-drain work.
   - Run a 1M cache load with observed source-shape diagnostics and confirm the
     JSON records final L0/nonzero/queue facts.
2. **Admission throttle second**
   - Remove the per-mutation urgent slowdown floor.
   - Add the progress-aware `BackgroundAdmissionThrottle` state.
   - Land deterministic manual-clock tests for relief, no-relief escalation,
     and Block deadline behavior.
   - Run 1M cache load before touching compaction shape. Expected result:
     admission slowdown drops materially while correctness counters remain
     clean.
3. **L0 episode width third**
   - Change L0-to-L1 planning to select all current snapshot L0 tables.
   - Preserve concurrent flush safety and stale-candidate handling.
   - Land read-equivalence tests across point, scan, history, and timestamp.
   - Run 1M and 5M cache loads. Expected result: fewer repeated L0 pressure
     episodes and lower compaction task count per million rows.
4. **Level target helper fourth**
   - Port the old segmented level-target calculation into storage-next
     lifecycle compaction.
   - Keep table output target bytes separate from level pressure target bytes.
   - Land fixture parity tests against old empty, shallow, deep, clamped, and
     raised-base target cases.
   - Do not change compact-pointer behavior in the same patch; isolate target
     math so counter movement is attributable.
5. **Nonzero selection fifth**
   - Replace largest/efficiency-only nonzero selection with compact-pointer
     selection for non-final levels.
   - Preserve overlap selection, metadata promotion, and one-operation
     re-score semantics.
   - Stop ordinary write-pressure chains from selecting final configured level
     consolidation.
   - Run 5M cache load. Expected result: row/byte rewrite amplification moves
     toward the 4x gate.
6. **Closed-loop CI guard sixth**
   - Add the scaled sustained-load liveness test with reduced thresholds.
   - Assert no permanent commit failure, bounded L0/nonzero shape, bounded WAL,
     queue convergence, and amplification below the scaled gate.
   - This becomes the cheap regression tripwire before manual 10M runs.
7. **Large benchmark gate last**
   - Run fmt, clippy, and full tests.
   - Run storage-next cache 100K, 1M, 5M, and 10M one at a time with
     source-shape diagnostics.
   - Run old-engine cache 100K, 1M, 5M, and 10M one at a time in the same
     environment.
   - Close the slice only if the hard gates in section E pass. If they do not,
     use the stop conditions to choose the next owner; do not add benchmark
     retries or scale-specific shortcuts.

## A. Benchmark Visibility

Goal: make the benchmark answer "why is load slow?" without requiring a later
read workload or manual JSON spelunking.

Implementation tasks:

1. Change `storage-next-l9-scale` so load-only runs can collect source-shape
   diagnostics after the timed load.
2. Split diagnostic modes:
   - observed shape: no explicit flush or compact after the timed load;
   - final-drain shape: explicit flush and fixed-point compact after the timed
     load, reported separately.
3. Do not include either diagnostic mode in the timed load duration.
4. Record source-shape context on load results, not only on later read results.
5. Add final source layout fields:
   - active rows;
   - frozen rows and frozen table count;
   - owned L0 table count;
   - owned nonzero level table counts;
   - inherited layer/table counts;
   - maintenance queue pending/active/max/deferred/completed facts.
6. Add load-phase derived fields:
   - rewrite amplification by rows and bytes;
   - compaction task count per million rows;
   - admission slowdown milliseconds per million rows;
   - background task milliseconds per million rows;
   - foreground wait milliseconds per million rows.
7. Add compaction-kind counters:
   - L0-to-L1 operations;
   - nonzero level operations;
   - final-level consolidation operations;
   - metadata promotions;
   - table rewrites;
   - stale/deferred operations.
8. Add selected-input counters:
   - selected L0 input table count;
   - selected L0 overlap table count;
   - selected nonzero input table count;
   - selected nonzero overlap table count;
   - selected final-level run length;
   - selected input bytes and overlap bytes by kind.

Exit gates:

1. A load-only cache run with `--diagnostic-source-shape` records known source
   shape on the load result.
2. A load-only cache run with `--diagnostic-final-drain` records both observed
   shape and post-drain shape.
3. The timed load path still has no explicit `MaintenanceTask::Compact` or
   diagnostic drain call.
4. Benchmark JSON can compute rewrite amplification without reading raw
   perf-trace keys by hand.

## B. Adaptive Admission Throttle

Goal: keep sustained load live without turning urgent pressure into a
steady-state per-mutation sleep tax.

Current storage-next behavior:

1. `background_batch_slowdown_duration` adds
   `BACKGROUND_URGENT_PER_MUTATION_SLOWDOWN * mutation_count`.
2. With `batch-size=1000`, an urgent batch pays about 25ms before additional
   pressure scaling.
3. The 10M run paid 152.16s of admission slowdown and zero block wait time.

Required behavior:

1. Remove the linear per-mutation urgent slowdown term.
2. Introduce a runtime-local `BackgroundAdmissionThrottle` state machine with:
   - last observed pressure reason and severity;
   - last pressure units;
   - last background completed-task count;
   - consecutive no-relief rounds;
   - current slowdown duration;
   - last slowdown timestamp from the injected maintenance clock.
3. Define relief as at least one of:
   - background completed task count increased;
   - pressure severity decreased;
   - pressure score decreased;
   - blocking-relevant table count/byte count decreased;
   - WAL retained bytes/segments decreased for WAL-pressure waits.
4. For urgent pressure:
   - wake background maintenance;
   - if relief is observed, accept with no slowdown or a small capped delay
     no greater than 1ms per commit call;
   - if no relief is observed for consecutive rounds, exponentially increase
     delay up to the existing urgent max;
   - reset delay after relief or healthy pressure.
5. For block pressure:
   - preserve the bounded wait deadline;
   - wait for background progress using executor stats and lifecycle pending
     tasks;
   - return the typed pressure error only after deadline/no-relief conditions.
6. Keep all sleeps behind `MaintenanceClock`.
7. Do not run full maintenance inline in public background mode.

Counter movement required:

1. `lifecycle_write_admission_slowdown_attempts` may remain nonzero under
   sustained pressure, but `lifecycle_write_admission_slowdown_ns` must fall by
   at least 80% on the 10M cache load versus the measured 152.16s baseline.
2. `lifecycle_write_admission_block_wait_ns` remains zero for cache 100K-10M
   unless a real Block threshold is hit.
3. `lifecycle_write_admission_wait_timeouts` remains zero.
4. A new no-relief/escalation counter increments only in tests or true
   sustained overload, not throughout the normal 10M cache load.

Exit gates:

1. Urgent pressure with background progress does not impose a 25ms delay on a
   1000-row batch.
2. Urgent pressure without relief escalates to bounded slowdown.
3. Block pressure waits with deadline and typed failure.
4. Deterministic inline tests can advance the injected clock and replay the
   same throttle sequence.

## C. L0 Episode Width

Goal: restore the old L0-to-L1 compaction episode invariant.

Current storage-next behavior:

1. `BranchLocalState::plan_l0_to_l1_compaction` selects only the newest four
   L0 tables via `LEVEL_ZERO_COMPACTION_INPUT_LIMIT`.
2. The old segmented engine compacts all current L0 files plus overlapping L1
   files in the selected episode.
3. Four-table episodes cause recurring L0 pressure and more downstream churn
   during sequential load.

Implementation tasks:

1. Replace four-table L0 selection with all current L0 tables captured in the
   compaction snapshot.
2. Keep overlap detection against L1 over the full L0 key range.
3. Preserve output-level sorted/non-overlapping guarantees.
4. Preserve concurrent flush safety:
   - compaction removes only the snapshotted L0 inputs;
   - L0 tables installed while the build is unlocked remain newer;
   - publication must not reorder concurrent flush output behind older
     compaction output.
5. Preserve stale-candidate handling:
   - if snapshotted inputs are no longer present, defer as stale;
   - do not delete unpublished outputs until publication/reclaim policy owns
     them.
6. Keep metadata promotion only when safe:
   - no overlap;
   - one input table if the existing promotion proof requires one;
   - retention/pruning proof allows it.
7. Record selected L0 input count, overlap count, input rows, and output table
   count.

Exit gates:

1. A branch with N L0 tables plans N L0 inputs for L0-to-L1.
2. A concurrent flush during build survives publication.
3. Reads before and after L0-to-L1 return identical latest/history results.
4. The 1M load no longer needs hundreds of urgent slowdown attempts caused by
   repeated four-table L0 churn.

## D. Nonzero Shape Policy

Goal: reduce rewrite amplification by restoring old leveled-shape invariants.

Current storage-next behavior:

1. `NONZERO_LEVEL_BASE_TARGET_BYTES` is a fixed 64MiB.
2. `table_compaction_config_for_kind` uses the output level target as the
   table output target for nonzero compaction.
3. nonzero selection chooses one candidate by local efficiency under a soft
   budget capped by output target bytes.
4. final configured level consolidation can rewrite rows to reduce table count
   during automatic chains.

Required behavior:

1. Separate table output target from level pressure target:
   - table output target default remains 64MiB;
   - level pressure target derives from level sizes, min base, max base, and
     multiplier, matching the old segmented engine's `recalculate_level_targets`
     behavior.
2. Add a storage-next level-target helper with these inputs:
   - current owned nonzero bytes per level;
   - max base bytes, default 256MiB;
   - min base bytes, default 1MiB;
   - multiplier, default 10;
   - max configured level count.
3. Use the computed per-level target for scoring and severity.
4. Use table output target for output splitting only.
5. Replace largest/efficiency-only nonzero selection with old-style compact
   pointer selection:
   - one selected input table per non-final nonzero compaction;
   - overlap selection in the next level;
   - compact pointer advances on successful publication and wraps.
6. Preserve metadata-only promotion/trivial move:
   - nonzero level;
   - one input;
   - no next-level overlap;
   - grandparent/deeper overlap below the configured split threshold;
   - retention/pruning policy allows it.
7. Stop scheduling final configured level consolidation from write-pressure
   urgency. Final-level consolidation remains available for explicit compact,
   close-required drain, or a separately tagged low-priority read-shape task.
8. Record per-level target bytes, selected pointer index, selected overlap
   bytes, metadata-promotion bytes avoided, and final-level consolidation count.

Exit gates:

1. Level target helper matches old segmented fixtures for empty, shallow,
   deep, clamped, and raised-base cases.
2. Nonzero compaction selection follows compact pointer order and wraps.
3. Safe no-overlap nonzero moves avoid table rewrite.
4. Automatic write-pressure chains do not rewrite final-level tables solely to
   reduce final-level count.
5. 10M cache load compaction input rows drop from about 11x logical rows to at
   most 4x logical rows.

## E. Benchmark Closeout

Goal: prove the mechanics changed in the measured profile.

Required commands:

```text
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l9-scale -- \
  --scales 100k,1m,5m,10m \
  --engines cache \
  --workloads load-seq \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000 \
  --diagnostic-source-shape
cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-old-cache-scale -- \
  --scales 100k,1m,5m,10m \
  --workloads load-seq \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000
```

Benchmark gates:

1. storage-next cache load completes at 100K, 1M, 5M, and 10M.
2. `lifecycle_write_admission_wait_timeouts == 0`.
3. `lifecycle_write_admission_pressure_rejects == 0`.
4. 10M `lifecycle_write_admission_slowdown_ns <= 30_000_000_000`.
5. 10M compaction row amplification
   `lifecycle_compaction_input_rows / operation_count <= 4.0`.
6. 10M compaction byte amplification
   `lifecycle_compaction_input_bytes / logical_written_bytes <= 4.0`.
7. Observed post-load source shape is known for load-only results.
8. Observed post-load L0 table count is below the urgent threshold.
9. Observed maintenance queue depth is bounded and not monotonically growing.
10. 10M storage-next cache throughput is at least 150K ops/s.
11. 10M storage-next cache throughput is no worse than 2x the old engine cache
    result captured in the same run environment.

If all correctness gates pass but the throughput gate misses, use the stop
conditions below to choose the next owner. Do not loosen the benchmark by
adding retries or skipping maintenance.

## Stop Conditions

1. If admission slowdown falls by at least 80% but throughput remains below
   150K ops/s and table merge ns/row remains above 700ns, stop and execute the
   table compaction hot-loop slice.
2. If admission slowdown falls but compaction row amplification remains above
   4x, stop and continue the compaction-shape work before touching commit
   runtime.
3. If compaction amplification falls below 4x and slowdown falls below 30s but
   commit time remains dominated by `api_runtime_ns`, `append_insert_ns`, or
   validation counters, stop and open a commit-runtime hot-path slice.
4. If final source shape is unknown in load-only results, do not trust the
   benchmark gate; fix benchmark visibility first.
5. If Block pressure appears in cache 100K-10M after these changes, treat it as
   a storage scheduling bug unless diagnostics prove actual no-relief overload.

## Non-Goals

1. No benchmark retry loop.
2. No special path for 5M, 10M, or 20M.
3. No disabling compaction, checkpoint, retention, or source-shape safety.
4. No parallel same-branch compaction publication.
5. No changing table format.
6. No changing durability semantics.
7. No read-path materialization policy change except final-level consolidation
   tagging needed to keep write-pressure chains from over-rewriting.
8. No deterministic-simulation harness implementation; only preserve the
   existing executor/clock seam.
