# M4P-L8H Implementation Plan: Durable Maintenance Liveness and Publish Decoupling

Status: closed — objective met. Landed: Group A + Group B (Slice 1), Group C C1+C2
(per-table summary cache, eliminated the O(N²) publish rescans), and Group E slice C3
(concurrent maintenance drain + per-wake tuning). Deferred to a dedicated
locking-architecture milestone: off-lock manifest fsync + Group D crash-consistency, the
Group E admission-ramp shaping, and finer-grained commit-vs-maintenance locking. See the
closeout for the profiling finding and the deferred-work rationale:
`docs/architecture/implementation-plans/M4P/m4p-l8h-closeout.md`.

Parent implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8-lifecycle-maintenance-parity-implementation-plan.md`

Predecessor plans:

1. `docs/architecture/implementation-plans/M4P/m4p-l8e-background-maintenance-executor-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4P/m4p-l8f-load-performance-stabilization-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4P/m4p-l8g-cache-mode-lifecycle-policy-implementation-plan.md`

Follow-up test plan:
`docs/architecture/implementation-plans/M4P/m4p-l8h-durable-maintenance-liveness-test-plan.md`

## Objective

Make `StorageMode::DurableLocal` survive sustained write load. Today it does
not: the durable standard 5M sequential load does not merely slow down, it
**fails the load gate** with a hard commit rejection.

L8G removes accidental source/table maintenance from cache mode. Durable mode
is different: flush, compaction, checkpoint, and WAL truncation are real
correctness requirements and cannot be removed. The defect is therefore not
"durable does maintenance" but two specific coupling bugs:

1. **Publish coupling.** Background maintenance runs the publish phase under the
   single runtime lock that every commit needs, and that phase rebuilds
   branch-wide facts and the full table manifest by scanning the entire resident
   dataset on every flush/compaction (then fsyncs), so commits serialize behind
   O(dataset) per-publish work. (Group A's `manifest_persist` counter later showed
   the fsync itself is only 2-4% of the lock-held cost; the rescans are the rest —
   see Group C.)
2. **Admission coupling.** When the L0 backlog reaches the blocking threshold,
   write admission converts sustained backpressure into a commit *rejection*
   instead of slowing the writer until maintenance catches up.

The required end state is the behavior the old engine already has: under
sustained load the writer is throttled, never rejected, and the durable runtime
always completes the load.

This slice is a coupling fix, not a maintenance rewrite. The durable lifecycle
algorithms (flush, compaction, checkpoint, WAL truncation, recovery) stay as
they are. What changes is where their cost lands relative to the commit lock,
and how admission behaves when they fall behind.

## Sharpened Diagnosis

The L8F/L8G work correctly added the counters and corrected the cache-mode
interpretation. Those counters now expose the durable defect directly.

The durable standard load is slow and eventually unable to complete because:

1. each commit takes the runtime lock to append WAL and apply to the memtable;
2. a background worker runs flush/compaction/checkpoint as a
   snapshot → build → publish state machine;
3. the build phase already runs unlocked, but the **publish phase reacquires the
   runtime lock and, under it, rebuilds branch-wide observed-row facts and the
   entire table manifest by scanning the resident dataset — O(dataset) per
   publish — then fsyncs the manifest** (the fsync is the small tail, ~2-4%; see
   Group C);
4. so the foreground spends a large fraction of the run blocked on the lock the
   publish phase holds;
5. background maintenance therefore cannot keep pace with the writer, L0 tables
   accumulate, and admission escalates None → Background → Urgent → Blocking;
6. at the blocking threshold the writer is rejected rather than paced, and the
   load fails.

The old standard engine is not faster because its compaction is better. It is
faster and reliable because its publish does O(tables-changed) work — it never
serializes commits behind a full-dataset rescan or full manifest rebuild — and
never converts compaction backlog into a write failure.

## Current Evidence

Observed 5M durable standard failure:

```text
commit rejected by Blocking storage pressure from LevelZeroTableBacklog:
mutating commit admission requires maintenance progress
```

Storage-next durable standard load facts, **pre-Slice-1** (same environment):

| Scale | Throughput | Elapsed | Result |
| --- | ---: | ---: | --- |
| 100K | 522K ops/s | 0.19s | completes (faster than old) |
| 1M | 41.9K ops/s | 23.9s | completes, ~7.9x slower |
| 5M | — | — | **fails the load gate** |
| 10M | — | — | not reached |

Durable 1M maintenance attribution (`--diagnostic-source-shape`):

| Fact | Value |
| --- | ---: |
| Background maintenance total | 15.82s / 170 tasks |
| Publish phase (lock held) | 10.77s (68% of maintenance) |
| Unlocked build phase | 5.03s |
| Compaction merge | 0.32s |
| Foreground wait on background lock | 9.55s (~half the run) |
| Admission slowdown | 4.66s |
| Admission block wait | 1.10s |

The merge cost is negligible (0.32s). The dominant cost is publish-under-lock
(10.77s) and the foreground lock-wait it causes (9.55s). At 5M these scale until
the writer cannot make admission progress and the load is rejected.

### Post-Slice-1 confirmation (Groups A + B landed)

With the admission-liveness fix in place the 5M durable standard load **now
completes** instead of failing the gate, and the first full 5M attribution
confirms the diagnosis is correct and *sharper at scale* than the 1M baseline
above. Measured 5M durable standard (same environment, commit `c23b8ccd`):

| Scale | Throughput | Elapsed | Result |
| --- | ---: | ---: | --- |
| 5M (storage-next) | 13.1K ops/s | 380.8s | **completes**, 0 rejections, 0 admission timeouts |
| 5M (old standard) | 288.1K ops/s | 17.35s | completes |

Gap vs. old at 5M: **~21.9x**.

Durable 5M maintenance attribution:

| Fact | Value | Share |
| --- | ---: | ---: |
| Background maintenance total | 367.4s / 1186 tasks | — |
| Publish phase (lock held) | 317.4s | **86% of maintenance** |
| Unlocked build phase | 49.4s | 13% of maintenance |
| Foreground wait on background lock | 304.1s | **80% of elapsed** |
| Admission slowdown | 27.2s | 7% of elapsed |
| Admission block wait | 18.1s | 5% of elapsed |
| Compaction merge | 3.9s | ~1% of elapsed |
| WAL append (the real durable write) | 4.35s | ~1% of elapsed |

Two facts sharpen the case for Groups C–E:

1. **Publish-under-lock is now 86% of maintenance (was 68% at 1M), and 304s of
   the 317s publish-lock window — 96% — directly blocks the foreground.** The
   later full-scale sweep (commit `5f2c68e1`) added the `manifest_persist`
   counter, which overturned the original mechanism: fsync is only **2-4%** of
   publish-lock (10.2s of 240.95s at 5M; 39.9s of 1474.3s at 10M). The dominant
   **~96%** is the publish phase rebuilding branch-wide facts and the full
   manifest by scanning the resident dataset on every flush/compaction
   (`refresh_observed_row_facts` + `build_manifest`'s per-row bounds) — O(rows)
   per publish × O(flushes) = O(N²). Group C is therefore re-aimed at eliminating
   the rescans (cache immutable per-table facts; incremental observed-facts and
   manifest), **not** at moving fsync off-lock.
2. **The actual commit/WAL work is small.** Subtracting the 304.1s lock-wait,
   27.2s slowdown, and 18.1s block-wait from the 380.8s run leaves ~31s of real
   commit work — the same ballpark as the old engine's *entire* 17.35s run. The
   ~22x gap is almost entirely maintenance serialization, not the durable write
   path, which is the premise Groups C–E are built on.

Note on the 2x soft target: because the rescans are ~96% of publish-lock,
eliminating them (Group C) should collapse foreground lock-wait by roughly that
fraction — a far larger reclaim than the ~4.4x an off-lock-fsync move alone would
have bought — and is expected to bring the gap from 19x/44x (5M/10M) toward single
digits, plausibly to or past the 2x target. Any residual after C is owned by
Group E (admission ramp + concurrent drain) plus the commit/WAL hot-path
follow-on the soft-target note in Group F already anticipates.

Old engine standard load (same environment):

| Scale | Throughput | Elapsed |
| --- | ---: | ---: |
| 1M | 330.8K ops/s | 3.02s |
| 5M | 290.0K ops/s | 17.2s |
| 10M | 269.0K ops/s | 37.2s |

The old engine completes 10M with moderate throughput decay and never rejects a
commit.

## Why The Current Shape Exists

Do not assume the coupling is arbitrary. It is the simplest correct ordering:

1. persisting the manifest under the runtime lock guarantees that no commit
   observes a layout that disagrees with durable state;
2. doing the manifest fsync synchronously inside publish guarantees that WAL
   truncation and flush-watermark advancement never outrun manifest durability;
3. the bounded admission deadline exists to surface a genuinely stuck or dead
   maintenance executor instead of hanging forever.

The cleanup must preserve all three guarantees. It must move durable I/O out of
the commit-blocking critical section *without* allowing a table to become
authoritative, or WAL to be truncated, before its manifest entry is durable. It
must keep a liveness backstop for a truly dead executor *without* failing a
load that is merely slow.

## Required Invariants

1. A sustained mutating load that outpaces maintenance degrades to bounded
   backpressure and always completes. Admission must not surface a commit
   rejection while maintenance is making progress.
2. Background maintenance publish must not hold the runtime/commit lock during
   durable disk I/O: manifest persistence, fsync, snapshot/checkpoint writes,
   or table-object writes.
3. The runtime lock is held by publish only long enough to swap in-memory
   layout pointers and update in-memory maintenance, branch, and visibility
   state. This update is O(tables changed by the task), never a scan or rebuild
   proportional to the resident dataset (no per-publish row rescan, full manifest
   rebuild, or full catalog clone).
4. Manifest durability ordering is preserved: a table is relied upon for
   recovery only after its manifest entry is durably persisted. A crash between
   the in-memory pointer swap and manifest persistence must recover to a
   consistent state via WAL replay.
5. WAL truncation and flush-watermark advancement must not outrun durable
   manifest persistence for the tables they retire.
6. The admission stall deadline fires only for a provably dead or stuck
   maintenance executor (no active task, no pending task, and zero completions
   across the window), not for sustained backlog.
7. For every blocking pressure reason — including `LevelZeroTableBacklog` — the
   admission wait path guarantees the corresponding maintenance work is enqueued
   or active before it is allowed to give up.
8. Durable-local standard and always modes preserve existing commit, conflict,
   timestamp, branch, read, recovery, snapshot, and WAL semantics.
9. Background worker parallelism is real: under sustained durable load more than
   one worker may make maintenance progress concurrently, bounded only by
   lock-held critical sections.
10. No benchmark-specific fast path, scale check, retry loop, or final-drain
    shortcut is allowed. Cache-mode behavior from L8G is unchanged.

## Scope Summary

| Group | Required Work | Exit Gate |
| --- | --- | --- |
| A. Publish Cost Audit | Attribute durable publish-lock time to each step and classify each as lock-required (pointer/state swap) or movable (durable I/O). | A table names each durable publish step and its required disposition, backed by a counter that separates lock-held publish time from off-lock durable I/O. |
| B. Write-Admission Liveness | Stop converting sustained backlog into commit rejection. | A sustained durable overload load completes; admission never surfaces a retryable pressure rejection while maintenance completes tasks. |
| C. Publish Critical-Section Cost Elimination | Make publish touch only the tables the task changed: cache immutable per-table bounds/facts at seal time, update branch facts incrementally, publish the manifest incrementally; then move the residual fsync off-lock. | Per-task lock-held publish time is flat across 1M→10M (no full-dataset rescan); manifest bytes and recovery byte-identical to baseline; foreground lock-wait drops by the non-fsync residual. |
| D. Crash-Consistency And Recovery | Prove the decoupled publish preserves durability ordering. | Recovery, snapshot, and WAL-replay tests pass; no layout/manifest divergence after simulated crash between pointer swap and manifest persist. |
| E. Maintenance Throughput And Backpressure Shape | Let the worker pool parallelize durable maintenance and pace the writer smoothly. | Durable maintenance wall-clock scales with worker count; admission ramps instead of cliffing at the blocking threshold. |
| F. Benchmark Closeout | Re-run durable standard and always loads versus old standard. | Durable completes 100K-10M with bounded backpressure and no rejection. |

## Implementation Order

Execute in this order. Liveness lands before performance so the engine becomes
usable immediately and all later throughput work is measured against a load
that already completes.

1. **Audit and counters first (Group A).**
   - Confirm or add counters that separate, for durable publish:
     - in-memory pointer/state swap time under the lock;
     - durable manifest persistence and fsync time;
     - durable table-object write time;
     - checkpoint/snapshot write time;
     - WAL truncation time.
   - Add a maintenance-liveness signal usable by admission: last task
     completion instant, active task count, pending task count.
   - Document each durable publish call site and whether it must hold the
     runtime lock.
2. **Admission liveness second (Group B).**
   - Make the stall deadline reset on maintenance progress, not only on
     pressure relief.
   - Only fail a blocking-pressure commit when the maintenance executor is
     provably dead or stuck for the full deadline window.
   - Close the `LevelZeroTableBacklog` enqueue gap so the wait path always has
     enqueued or active compaction to wait on before it can give up.
   - Preserve the existing runtime-shutdown, panic, and recovery-health
     rejection paths unchanged.
3. **Publish critical-section cost elimination third (Group C).**
   - Cache immutable per-table bounds/facts at seal time and make observed-row
     facts and manifest publication incremental, so publish is O(tables changed),
     not O(resident dataset). This is the dominant cost (~96% of publish-lock).
   - Then move the residual fsync off-lock, kept ordered before any reliance on
     the new table for recovery and before WAL truncation that retires its inputs.
4. **Crash-consistency and recovery fourth (Group D).**
   - Prove WAL replay reconstructs any table whose manifest entry was not yet
     persisted at crash.
   - Prove WAL truncation never outruns manifest durability.
5. **Throughput and backpressure shape fifth (Group E).**
   - Allow concurrent durable drain progress across the worker pool.
   - Raise the per-wake runtime budget above per-task build/publish time.
   - Widen and smooth the L0 admission ramp so the writer self-paces before the
     hard block.
6. **Benchmark closeout last (Group F).**

## A. Publish Cost Audit

Goal: a source-owned explanation of what durable publish does under the runtime
lock and which of it is movable.

Required audit table columns:

1. call site;
2. publish step kind (pointer swap, manifest persist, fsync, table-object
   write, checkpoint write, WAL truncation);
3. current lock disposition (held / not held);
4. correctness dependency on holding the lock, if any;
5. durability ordering dependency, if any;
6. proposed disposition (keep under lock / move off lock);
7. counter that proves the disposition.

Minimum call sites to audit:

1. durable background drain publish phase;
2. flush publish and table-manifest publication after flush;
3. compaction install and its manifest update;
4. checkpoint publication and snapshot id advancement;
5. flush-watermark persistence with table-manifest proof;
6. WAL truncation;
7. admission pressure collection and the blocking-pressure wait path.

Exit gates:

1. The plan records why each durable publish step currently holds the lock.
2. No code movement occurs before a step has a disposition and a counter.
3. The audit distinguishes ordering requirements from convenience.

## B. Write-Admission Liveness

Goal: a durable load that merely outpaces maintenance is throttled, never
rejected.

Implementation tasks:

1. Track maintenance liveness (last completion instant, active count, pending
   count) and expose it to the admission wait path.
2. Reset the blocking-pressure stall deadline whenever maintenance makes
   progress, not only when pressure is relieved.
3. Redefine the give-up condition so a blocking-pressure commit fails only when
   maintenance is provably dead or stuck: no active task, no pending task, and
   no completion across the full stall window.
4. In the blocking-pressure wait path, for `LevelZeroTableBacklog` ensure a
   compaction task is enqueued or already active before evaluating give-up, the
   same way `FrozenBacklog` already forces a flush.
5. Keep all waits behind the injected maintenance clock so deterministic
   simulation still controls time.
6. Preserve unchanged: runtime-closed rejection, panic/shutdown health
   rejection, recovery-health rejection, and non-retryable error propagation.

Exit gates:

1. A sustained durable overload completes without surfacing a retryable
   `StoragePressureRejected` to the caller while maintenance completes tasks.
2. A genuinely dead executor still surfaces a typed, bounded failure rather than
   hanging.
3. The give-up path is driven by maintenance liveness, not by an absolute clock
   deadline alone.

## C. Publish Critical-Section Cost Elimination (was: Publish/Manifest Decoupling)

Goal: the publish critical section must touch only the tables the running task
changed — never the whole resident dataset. The dominant lock-held cost is not
durable I/O; it is per-publish recomputation of branch-wide facts and the full
table manifest from a full scan of the resident dataset.

What the Group A counter revealed (full-scale sweep, commit `5f2c68e1`): manifest
fsync (`lifecycle_background_publish_manifest_persist_ns`) is only 2-4% of
publish-lock (10.2s of 240.95s at 5M; 39.9s of 1474.3s at 10M). The other 96-97%
is in-memory recomputation, all under the runtime lock, none captured by the
fsync timer:

1. `refresh_observed_row_facts` (`branch/state.rs`) via `observe_own_rows`
   rescans every row of every owned table on every flush install to recompute
   `max_commit_version`, timestamp min/max, and put/tombstone counts.
2. `build_manifest` (`lifecycle/table_manifest.rs`) rebuilds the entire manifest;
   `manifest_table_bounds` and `timestamp_bounds` rescan every row of every table
   to recompute key/timestamp bounds that are immutable once a table is sealed.
3. the budget pre-encode (`encode_table_manifest`) serializes the whole manifest
   each publish only to measure its byte length, then discards it;
   `record_manifest` re-walks every table ref; compaction
   (`install_published_durable_compaction`) clones the entire catalog
   (`catalog.clone()`).

This is O(resident rows) per publish × O(flushes) publishes = O(N²). It is why
per-task publish-lock time roughly doubles 5M→10M (232ms→460ms) and the
foreground lock-wait reaches 80-86% of the run. Moving fsync off the lock — the
original framing of this group — would reclaim only the 2-4%; the rescans are the
target.

Implementation tasks:

1. **Cache immutable per-table bounds and facts at seal time.** A built table is
   immutable; compute its physical/internal key bounds, timestamp min/max, and
   manifest table facts once when the table object is sealed (flush or compaction
   output) and store them on the owned-table / catalog entry. `build_manifest`
   then reads them in O(1) per table instead of rescanning rows. (Internal-key
   min/max are the first and last row of a sorted table; no scan is needed.)
2. **Make branch observed-row facts incremental.** A flush moves one known frozen
   table into L0; a compaction replaces known inputs with known outputs. Update
   branch `max_commit_version`, timestamp min/max, and put/tombstone counts by
   delta from the changed tables' cached facts, instead of
   `refresh_observed_row_facts` rescanning all owned levels.
3. **Make manifest publication incremental.** Assemble and persist the manifest
   as a function of the tables the task changed (add/remove refs over the prior
   manifest), not a full rebuild of all levels each publish. Remove the throwaway
   budget pre-encode (budget from cached per-table byte counts) and the
   per-compaction full `catalog.clone()`.
4. **Then** move the residual durable I/O (manifest fsync, snapshot/checkpoint
   writes) off the runtime lock — last, and only after tasks 1-3, because once
   the rescans are gone the fsync is the next item but is small (2-4% today).
   Order it per Group D: the new table's manifest entry must be durable before it
   is relied upon for recovery and before any WAL truncation that retires its
   inputs.
5. Preserve durability and format exactly. Cached facts are an in-memory
   acceleration of values recovery already recomputes independently from the
   table objects; the two must agree, and a divergent cached bound is a
   durability correctness bug, not a performance regression. Do not change the
   table format, manifest format, or codec — the manifest bytes written for a
   given logical state must be identical to today's.

Exit gates:

1. Per-task lock-held publish time is independent of total resident row/table
   count: `publish_lock_ns / lifecycle_background_tasks_completed` is flat (within
   noise) across 1M→5M→10M, instead of doubling per scale.
2. The publish path performs no per-row scan of already-sealed tables: a source
   guard shows publish does not call `observe_own_rows`, `manifest_table_bounds`,
   or `timestamp_bounds` over owned tables; bounds and facts come from cached
   values.
3. Foreground wait-on-background-lock drops by the non-fsync residual fraction
   (~96% of publish-lock), not merely the manifest-persist fraction.
4. Manifest bytes and recovery output are byte-identical to the full-rebuild
   baseline for the same write history; durable lifecycle counters keep their
   meaning and durable tests pass.

## D. Crash-Consistency And Recovery

Goal: the decoupling does not weaken durability.

Implementation tasks:

1. Define the durability ordering contract for decoupled publish and assert it
   in tests: pointer swap may precede manifest persistence, but recovery
   reliance and WAL truncation may not.
2. Simulate a crash between pointer swap and manifest persistence and prove
   recovery reconstructs the affected table from WAL.
3. Prove WAL truncation and flush-watermark advancement are gated on manifest
   durability for the retired inputs.
4. Prove snapshot/checkpoint publication remains consistent with the decoupled
   ordering.
5. Keep the WAL-fsync-failure halt-and-resume behavior intact.

Exit gates:

1. Recovery tests pass with simulated mid-publish crashes.
2. No recovered runtime observes a table pointer without a durable manifest
   entry or replayable WAL.
3. WAL truncation never deletes a segment whose data is not yet durable in the
   manifest.

## E. Maintenance Throughput And Backpressure Shape

Goal: maintenance keeps pace, and the writer is paced smoothly rather than
cliffed.

Implementation tasks:

1. Allow the durable drain to make concurrent progress across the worker pool
   instead of a single in-flight drain closure, bounded only by the lock-held
   pointer-swap critical section.
2. Raise the per-wake runtime budget above measured per-task build/publish time
   so a drain round does useful batches instead of a single task.
3. Review the L0 admission thresholds and the urgent slowdown ramp so the
   writer self-paces earlier and more smoothly, approaching the bounded
   backpressure shape of the old engine, without a hard cliff at the blocking
   threshold.
4. Keep durable correctness: parallel publish must still preserve the
   durability ordering from Group D.

Exit gates:

1. Durable maintenance wall-clock improves with worker count under sustained
   load.
2. Admission slowdown ramps with backlog rather than jumping to a block.
3. No regression in durable correctness or recovery tests.

## F. Benchmark Closeout

Required commands (run one scale at a time):

```text
cargo fmt --all
cargo clippy -p strata-storage-next --all-targets --all-features -- -D warnings
cargo test -p strata-storage-next --all-features
cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l9-scale -- \
  --scales 100k \
  --engines standard \
  --workloads load-seq \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000 \
  --diagnostic-source-shape
cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l9-scale -- \
  --scales 1m \
  --engines standard \
  --workloads load-seq \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000 \
  --diagnostic-source-shape
cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l9-scale -- \
  --scales 5m \
  --engines standard \
  --workloads load-seq \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000 \
  --diagnostic-source-shape
cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l9-scale -- \
  --scales 10m \
  --engines standard \
  --workloads load-seq \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000 \
  --diagnostic-source-shape
```

Repeat the four scales with `--engines always` for the stronger durability
policy, and with `storage-old-cache-scale --engine standard` for same-machine
comparison.

Hard gates:

1. Durable standard completes 100K, 1M, 5M, and 10M with no commit rejection.
2. No load surfaces `StoragePressureRejected` to the caller while maintenance is
   alive.
3. Per-task lock-held publish time (`publish_lock_ns / tasks_completed`) is flat
   across 100K-10M, independent of resident dataset size.
4. Foreground wait-on-background-lock is a small fraction of commit time at
   every scale.
5. Durable recovery, snapshot, and WAL tests pass unchanged.
6. Durable always mode also completes every scale.
7. Cache-mode counters and behavior from L8G are unchanged.

Soft targets:

1. 10M durable standard throughput is within 2x of old standard in the same
   environment.
2. If the soft target fails with liveness clean and per-task publish cost flat
   across scale, the next owner is a commit/WAL/flush hot-path slice, not
   admission or publish coupling.

## Stop Conditions

1. If the manifest cannot be persisted off-lock without violating the
   WAL-truncation-after-manifest-durability ordering, stop and design the
   ordering contract before moving any I/O.
2. If a recovery test fails after decoupling, stop and treat it as a durability
   regression, not a benchmark tuning problem.
3. If removing the absolute stall deadline can hang a truly dead executor, stop
   and keep a liveness-gated backstop instead of removing the backstop.
4. If concurrent drain progress requires weakening the durability ordering,
   stop and keep publish serialized rather than racing manifest writes.
5. If throughput improves only when a scale-specific condition is added, reject
   the patch.
6. If durable always mode regresses while standard improves, stop and split the
   always-mode behavior into its own fix.

## Non-Goals

1. No removal of durable flush, compaction, checkpoint, or WAL truncation.
2. No change to table format, manifest format, or codec.
3. No change to conflict, timestamp, or branch semantics.
4. No second durable engine path or duplicate commit/read implementation.
5. No benchmark retry loop, final-drain shortcut, or scale-specific fast path.
6. No change to cache-mode lifecycle policy from L8G.
7. No new public storage mode or public API surface beyond approved diagnostics.
8. No weakening of WAL-fsync-failure halt-and-resume behavior.
