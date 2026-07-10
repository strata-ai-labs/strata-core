# M4P-L8I Test Plan: Runtime Lock Decoupling

Status: draft

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8i-runtime-lock-decoupling-implementation-plan.md`

## Goal

Prove that moving all durable I/O and heavy work off the global runtime mutex
(group-commit WAL fsync, off-lock manifest/checkpoint persistence, arc-swapped
layout, lock-free admission) eliminates the contention **without weakening
durability, crash-consistency, recovery, admission liveness, the frozen durable
format, or cache-mode behavior.** Correctness is the gate; throughput is the
benchmark target.

The suite must fail if any change:

1. acknowledges a `Standard`/`Always` commit before its WAL record is durable;
2. lets a durable manifest sequence regress, or two same-branch manifest writes
   land out of order;
3. truncates WAL or advances the flush watermark before the covering manifest is
   durable;
4. recovers to a state that differs from a fully-synchronous baseline for the same
   write history, or that observes a table pointer without a durable manifest entry
   or replayable WAL;
5. exposes a lock-free reader to layout/derived-facts skew;
6. weakens the L8H admission watchdog (a serviceable overload times out, or a dead
   executor hangs);
7. weakens WAL-fsync-failure halt-and-resume;
8. changes any on-disk format / golden vector;
9. regresses cache-mode (L8G) behavior;
10. improves throughput via a benchmark-only path, retry, or scale-specific shortcut.

## Test matrix

| Area (group) | Required proof | Failure caught |
| --- | --- | --- |
| Admission churn (A) | Lock-free pressure snapshot equals the locked computation; foreground parks and wakes on relief; wait-attempts/commit collapses. | Pressure snapshot drifts; busy-poll churn persists. |
| Admission liveness (A) | Serviceable overload completes with `admission_wait_timeouts == 0`; dead executor → bounded typed failure; progress-reset still emitted. | Watchdog semantics regressed. |
| Group-commit durability (B) | A commit's ack strictly follows its WAL record durability; group fsync covers all batched commits. | Ack precedes durability (data loss on crash). |
| WAL crash windows (B) | Crash after append-before-fsync ⇒ commit NOT recovered (was not acked durable); crash after fsync ⇒ recovered. WAL-fsync-failure halts + resumes. | Lost or phantom commits; halt weakened. |
| Publish ordering / sequence (C) | Concurrent same-branch flush∥compaction never regress the durable manifest sequence; manifest bytes byte-identical. | Durable sequence regression / corruption. |
| Publish crash windows (C/D) | Crash between pointer-swap and manifest persist ⇒ recovery loads prior manifest + replays WAL ⇒ reconstructs table; recovery == synchronous baseline. | Decoupling drops a not-yet-persisted table. |
| Durability ordering (C) | WAL truncation / flush-watermark never outrun durable manifest. | Retired segment not durable in manifest. |
| Layout consistency (D) | Lock-free reader observes layout + `ObservedBranchRows`/timestamp facts atomically; read results == locked baseline under concurrent publish. | Reader sees layout vN with facts v(N−1). |
| Visible-version (D) | Atomic visible-version is monotonic; reads/checkpoints observe a consistent version. | Torn / non-monotonic visibility. |
| Concurrency stress | Randomized commit+flush+compaction+read interleavings recover to a consistent layout == synchronous baseline; no double-publish / lane violation. | Races, double-claim, recovery divergence. |
| Durable regression | Existing durable commit/conflict/timestamp/branch/read/recovery/WAL tests pass unchanged. | Decoupling changed durable semantics. |
| Cache regression | L8G cache absence counters and behavior unchanged. | Durable change leaked into cache. |
| Format goldens | All format golden vectors pass. | On-disk format drift. |
| Simulation boundary | All waits use the injected clock; deterministic-inline path stays deterministic. | Wall-clock dependency re-enters drive path. |
| Benchmark closeout | Settle-to-quiescence 100K–10M vs old, standard + always; **plus YCSB-F run-phase throughput at 10M (≥ ~100K ops/s / ≤ 2× old) and the interleaved control-vs-fixed crawl-rate A/B (convoy rate → ~0)** — the sharp convoy signal (F collapses to ~81 ops/s pre-fix). | Result explained by a shortcut; convoy persists at 10M. |

## A — Admission wait-loop (lock-free + condvar)

> **ABANDONED (2026-06-16).** Group A was implemented and reverted as a dead end —
> park-until-relief made admission churn 11.6× worse with no throughput gain, and the
> wait-loop is not a throughput lever (the bottleneck is drain rate). See the
> implementation plan's Group A detail for the benchmark. The tests below are not to
> be implemented unless Group A is revived purely as a CPU-churn reduction.

Correctness:
1. Lock-free pressure snapshot equals the value the previous locked
   `storage_pressure_for_branch` would compute, over randomized branch states
   (property test).
2. Under sustained overload, the foreground parks and is woken by the drain's
   relief notification (not a busy-poll); `lifecycle_write_admission_wait_attempts`
   per committed row drops by ~an order of magnitude vs the L8H C3 baseline (a
   regression guard asserting attempts/commit below a threshold).
3. Per-iteration runtime-mutex acquisitions ≤ 2 (source-guard / counter that the
   wait path no longer re-locks for pressure/progress snapshots).

Liveness (preserve L8H Slice 1):
4. Serviceable overload completes with `lifecycle_write_admission_wait_timeouts == 0`.
5. Provably dead executor (no active/pending/completions) → bounded typed
   `failed_precondition.storage_api.storage_pressure` (asserts `== 1` timeout),
   not a hang.
6. `record_lifecycle_write_admission_wait_progress_reset` still emitted on real
   progress; manual-clock determinism holds; inline executor degrades to
   run-one-then-recheck under `drain_immediately`.

## B — Group-commit WAL fsync off the lock

Correctness:
1. **Ack-after-durable**: with a fault-injected fsync delay, a `Standard` commit's
   return strictly follows its WAL record's `sync_object` completion (assert
   ordering via the fault hook / a durability event log). Same for `Always`.
2. **Group batching**: when N commits are appended before one group fsync, all N
   acks follow that fsync; none earlier.
3. **No runtime lock during fsync**: source guard / counter that the commit's
   runtime-lock hold excludes the fsync window (`foreground_wait_background_lock`
   per commit no longer includes `wal_append_ns`).

Crash windows (fault injection + recovery):
4. Crash after WAL append but before group fsync ⇒ on recovery the un-fsynced
   commit is **absent** (it was never acked durable) and the store is consistent.
5. Crash after fsync (acked) ⇒ commit is **present** after recovery.
6. WAL-fsync-failure ⇒ writer halts, covered commits' acks fail, explicit resume
   recovers (behavior unchanged from today).

## C — Off-lock publish + per-branch serialization + crash consistency

Correctness:
1. **No sequence regression**: drive concurrent same-branch flush + compaction
   publishes (the L8H C3 concurrency path) with fault-injected fsync reordering;
   assert the durable manifest object never regresses sequence and ends at the
   highest committed sequence.
2. **Publish holds the lock only for the swap**: source guard that the locked
   publish phase calls no manifest/checkpoint persist (`publish_replace_manifest`,
   checkpoint writes) — those run off-lock under the per-branch publish lock.
3. **Byte-identical manifest** for a given logical state (goldens + a round-trip
   compare vs the synchronous path).

Crash windows + ordering (the Group D suite):
4. Fault-inject a crash **after pointer-swap, before manifest persist**: recovery
   loads the prior durable manifest, replays WAL, reconstructs the table; final
   state == a synchronous-publish baseline for the same history.
5. WAL truncation / flush-watermark never advance past a table whose manifest is
   not yet durable (assert via the watermark proof + a fault that delays the
   persist).
6. fsync failure on publish ⇒ `table_manifest_debt_outcome` (table visible,
   manifest not durable, watermark does not advance, recovery reconstructs).
7. Generated: randomized crash points across swap / persist / record / truncate
   recover consistently every time; recovery matches the synchronous baseline.

## D — ArcSwap layout + atomic visible-version

Correctness:
1. A lock-free reader that `load()`s the layout during a concurrent publish observes
   the table set and its `ObservedBranchRows`/timestamp-coverage facts **atomically**
   (the facts ride in the swapped `Arc`): a targeted interleave test that swaps
   between the reader's layout-load and facts-load and asserts they are consistent.
2. Read results (point/scan/history/timeline) under concurrent flush/compaction are
   identical to the single-locked baseline for the same operations (differential
   test).
3. Visible-version atomic is monotonic and a read never observes a version newer
   than a durable/visible commit.
4. The C1+C2 cached-facts oracle (`refresh_observed_row_facts == observe_rows()`)
   still holds with the arc-swapped layout.

## Concurrency stress (cross-group)

1. Randomized multi-threaded workload (commits + reads + background flush/compaction
   + checkpoint + WAL truncation) over many iterations; after quiesce, recovery
   reconstructs a layout identical to a synchronous-execution baseline of the same
   logical history.
2. No double-publish / maintenance-lane violation (extend the L8H C3 concurrency
   tests).
3. Run the concurrent paths under `--test-threads` stress and, where available, a
   loom/thread-sanitizer pass on the new atomics/condvar/arc-swap interactions.

## Durable + cache + format regression

1. All existing durable commit, conflict, timestamp, branch, read, recovery, WAL,
   and lifecycle tests pass unchanged (no asserted class/code changes).
2. L8G cache absence counters stay zero; cache needs no controller/condvar/wall-clock
   for correctness; the durable changes do not import into cache policy (source guard).
3. All format golden vectors pass unchanged (`format_golden`, table-manifest /
   checkpoint / WAL goldens) — proves zero on-disk format change.
4. Any new named storage boundary type is documented with its owning layer and
   rationale.

## Benchmark gates

Settle-to-quiescence harness (load completes AND L0 fully compacted, no backlog),
one scale at a time, standard then always, vs `storage-old-cache-scale --engine standard`:

```text
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- \
  --scales {100k,1m,5m,10m} --engines standard --workloads load-seq \
  --value-bytes 150 --batch-size 1000 --samples 1000 --diagnostic-source-shape
```

Hard gates:
1. Durable standard + always complete every scale with no commit rejection and
   `admission_wait_timeouts == 0`.
2. Per-commit foreground runtime-lock hold excludes the WAL fsync (Group B).
3. Publish runtime-lock hold excludes manifest/checkpoint I/O (Group C).
4. Point/scan reads take no runtime lock for the layout (Group D).
5. Durable recovery, snapshot, and WAL tests pass after the 10M load.
6. Cache-mode counters/behavior unchanged.

Soft target:
1. 10M durable standard within **2×** of old standard at quiescence (the L8H
   deferred target). Track the per-group progression: admission churn (A) →
   foreground fsync off-lock (B) → publish off-lock (C) → lock-free reads (D).

## Regression commands

```text
cargo fmt --all
cargo clippy -p strata-storage-next --all-targets --all-features -- -D warnings
cargo test -p strata-storage-next --all-features                 # incl. crash + concurrency suites
cargo test -p strata-storage-next --test lifecycle_recovery
cargo test -p strata-storage-next --test lifecycle_faults
cargo test -p strata-storage-next --test service_fault_windows
cargo test -p strata-storage-next runtime_lock_decoupling        # required discoverability filter
```

(If module names differ, keep them equally descriptive and update this plan.)

## Failure interpretation

1. A commit acked before its WAL record is durable ⇒ Group B broke the
   ack/fsync ordering; durability regression, not a tuning issue.
2. A durable manifest sequence regression under concurrency ⇒ Group C's per-branch
   serialization is incomplete; stop and serialize publish.
3. A recovered state differing from the synchronous baseline ⇒ a durability-ordering
   violation in B or C; treat as a correctness bug.
4. A lock-free reader seeing inconsistent layout/facts ⇒ Group D did not fold facts
   into the swapped `Arc`.
5. A serviceable overload timing out, or a dead executor hanging ⇒ Group A weakened
   the L8H watchdog.
6. A format golden failing ⇒ an unintended on-disk format change; out of scope.
7. A throughput gain that vanishes under the settle-to-quiescence harness ⇒ the gain
   was a backlog shortcut, not real.
