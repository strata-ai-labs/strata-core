# M4P-L8H Closeout: Durable Maintenance Liveness and Performance

Status: closed — objective met; remaining performance gap deferred to a dedicated
locking-architecture milestone.

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8h-durable-maintenance-liveness-implementation-plan.md`

## Objective and status

Make `StorageMode::DurableLocal` survive sustained write load. **MET.** The durable
standard sequential load went from *failing the 5M load gate* with a hard commit
rejection —

```text
commit rejected by Blocking storage pressure from LevelZeroTableBacklog:
mutating commit admission requires maintenance progress
```

— to **completing 100K, 1M, 5M, and 10M with zero commit rejections and zero admission
timeouts**, via bounded backpressure. The old-vs-new throughput gap fell from a hard
failure to ~8× (down from 44× at the mid-effort low point).

## What landed

| Slice | Commit | Change |
| --- | --- | --- |
| Slice 1 — A (publish-cost audit/counter) + B (write-admission liveness) | `c23b8ccd`, `5f2c68e1`, `5da21e04` | Admission stall deadline became a liveness watchdog that resets on real maintenance progress (so sustained load is paced, not rejected); closed the `LevelZeroTableBacklog` forced-compaction enqueue gap; added the `manifest_persist` publish-cost counter + audit. |
| C1+C2 — publish critical-section cost elimination | `75e6460b` | Cache an immutable per-table summary (`TableSummaryExtras`: timestamp + physical-key bounds, put/tombstone split) once at the `BranchOwnedTable` seal choke point; `build_manifest` and `refresh_observed_row_facts` read it O(1) instead of rescanning every row of every table on every publish. Eliminated the O(N²) publish rescans. No durable format change; bit-exact (debug oracle + goldens + recovery validation). |
| C3 — concurrent maintenance drain + per-wake tuning (Group E) | `f20c66aa` | Replaced the per-priority single-flight drain with a bounded concurrent-drain counter (cap = worker_count) so the worker pool runs flush ∥ compaction ∥ checkpoint ∥ WAL on different lanes; raised the per-wake budget (8→32 tasks, 25ms→250ms). |

## Performance journey (durable standard, load-seq, same environment)

| Scale | pre-C1+C2 (`5f2c68e1`) | C1+C2 (`75e6460b`) | C3 (`f20c66aa`) | old engine |
| --- | ---: | ---: | ---: | ---: |
| 1M | 22.6s (7.7×) | 10.7s (3.7×) | 12.0s (4.1×) | 2.94s |
| 5M | 318.6s (18.9×) | 92.7s (5.5×) | 89.1s (5.3×) | 16.84s |
| 10M | 1635.9s (44.5×) | 303.6s (8.3×) | 295.6s (8.0×) | 36.78s |

(Pre-Slice-1, 5M did not appear in this table at all — it *failed the gate*.) The big win
is C1+C2 (the O(N²) elimination: 44×→8.3×). C3 crushed admission block-wait (10M: 97.2s →
6.8s) but is ~net-neutral on bulk-load throughput, for the reason the profiling below makes
precise.

## Profiling finding (why the gap stalls at ~8×)

At 10M (C3), the foreground writer's `api_runtime_ns` ≈ **263s** of the 295.6s run breaks
down as:

| Bucket | ~Time | Notes |
| --- | ---: | --- |
| Real commit/WAL work | **~16s** | wal_append 8.6s + memtable insert 3.6s + append-validate 3.0s + validate/prepare 0.8s |
| Admission slowdown + block-wait | ~44s | slowdown 37.4s + block-wait 6.8s |
| **Global-mutex acquisition + admission-retry churn** | **~200s** | `foreground_wait_background_lock` 107s (the aggregate time to acquire the single runtime `ParkingMutex`, recorded in `RuntimeSlot::lock()`), plus the uncounted churn of **647,702 admission wait-attempts for 10,000 commits** — the foreground re-acquires the contended mutex and re-checks pressure ~65× per commit while pacing. |

The decisive conclusions:

1. **The commit/WAL hot-path is not the bottleneck** (~16s). There is essentially nothing
   to optimize there.
2. **Manifest fsync is not the bottleneck** — Group A's counter showed it is only 2–4% of
   the publish-lock window.
3. **The bottleneck is the single global runtime mutex**, contended between the foreground
   writer and background maintenance, amplified by the admission wait-loop re-acquiring it
   on every retry.
4. **C3's concurrency worsened single-branch contention** (foreground lock-wait 62.7s →
   107.4s) with no parallelism payoff, because every drain's publish serializes on that same
   mutex. C3 remains valuable for multi-lane / multi-branch scenarios and eliminates hard
   write-stalls, but for a single-branch bulk load it adds contention.

## Deferred to a dedicated locking-architecture milestone

These are intentionally out of L8H because they are architectural, durability-critical, and
yield little until done together. They — not a commit hot-path slice — are what the L8H
plan's "2× soft target" actually depends on. They are now planned in **M4P-L8I**:
`docs/architecture/implementation-plans/M4P/m4p-l8i-runtime-lock-decoupling-implementation-plan.md`
(+ test plan `…/m4p-l8i-runtime-lock-decoupling-test-plan.md`), which carries the full
contention map, the old-engine blueprint, and the leverage-ordered group plan.

1. **Finer-grained locking (the real lever).** Separate the foreground commit critical
   section from background-maintenance publish so they do not contend on one
   `ParkingMutex`. This is what unblocks the foreground writer (~200s of mutex
   acquisition/churn at 10M).
2. **Off-lock manifest fsync.** Moving the manifest persist out of the lock REQUIRES
   per-branch publish serialization + manifest-sequence reservation, because the manifest is
   one full-snapshot object per branch with no CAS guard — concurrent same-branch off-lock
   writes (flush ∥ compaction) would otherwise land out of order and cause a durable
   sequence regression (silent corruption). Standalone benefit is modest (~8×→~7×; same-branch
   fsyncs still serialize), so it belongs with the locking work. The flush-watermark gate
   already keys on the durable manifest object (`load_required`), so recovery already covers
   the widened swap→fsync window; a crash-between-swap-and-fsync test must be added (Group D).
3. **Cheaper admission wait-loop.** Cut the per-attempt mutex re-acquisition churn in
   `background_wait_after_pressure_rejection` (647k attempts / 10k commits at 10M) — coarser
   wait slices and fewer per-attempt lock acquisitions — without weakening the
   progress-gated watchdog (the Slice-1 liveness backstop; `admission_wait_timeouts` must
   stay 0).
   **Update (2026-06-16): attempted in L8I Group A and abandoned.** A park-until-relief
   rewrite made churn *worse* (1M: `admission_wait_attempts` 29,360 → 341,576, 11.6×) with
   no wall-clock change, because the old one-slice-per-call loop is self-throttling (the
   commit re-execution paces it) and throughput is capped by the drain rate + the deliberate
   admission slowdown, not the wait-loop. The 647k attempts are a *symptom* of drain-rate
   saturation, not the cause of the gap. The admission wait-loop is not a throughput lever;
   the lever is items 1–2 (finer-grained locking + off-lock publish). See
   `m4p-l8i-runtime-lock-decoupling-implementation-plan.md` Group A for the benchmark.
4. **Per-compaction parallelism.** Shard the Rewrite lane by `(branch, level)` so multiple
   disjoint compactions run concurrently — only worthwhile after the locking work, and
   mainly for multi-level/high-ingest workloads.

## 2× soft target

Not met (~8× at 10M). Per the L8H plan's soft-target note, the next owner is not admission
or publish coupling (both addressed) — it is the runtime-lock architecture above. Durable
correctness, liveness, and the headline objective (survive sustained load) are complete and
verified; the remaining throughput gap is a known, profiled, deferred item.
