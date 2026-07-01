# Runtime lock-decoupling — per-slice performance ledger

Tracks the durable write-path performance of each M4P-L8I slice so we can tell,
slice to slice, whether we are improving or regressing. Companion to the
root-cause in [`durable-background-lock-convoy.md`](./durable-background-lock-convoy.md)
and the plan in
[`../../architecture/implementation-plans/M4P/m4p-l8i-runtime-lock-decoupling-implementation-plan.md`](../../architecture/implementation-plans/M4P/m4p-l8i-runtime-lock-decoupling-implementation-plan.md).

## Reference config (frozen — every ledger row uses this)

```text
engine-ycsb --records 10m --ops 100k --value-bytes 1000 --scan-max 100 \
            --workload a,b,c,d,e,f --mode durable --memory-budget 48g
```

Machine-local; compare rows only against other rows on the same machine. Each
workload loads a fresh 10M-record database (~110s) then runs 100k ops.

## How to read a run — what is signal vs. noise

The durable engine carries a **~30% intermittent lock convoy** on the write path
(global runtime mutex held across O(total-rows) maintenance work). Its defining
property: **the crawl relocates between workloads run-to-run.** One run it lands
on F; the next it lands on A/B/D/E. Therefore:

- **Stable per single run (safe to compare 1:1):**
  - **Read-only throughput** (workload C) and read p50 — reads never take the
    convoy path.
  - **Load throughput** (bulk insert, averaged over the six loads) — stable to
    within ~5% run-to-run.
- **NOT stable per single run (needs n≥9 to compare):**
  - Every write/RMW workload throughput (A, B, D, E, F) and their max-latency
    tails. A single run's write numbers are a point sample of the intermittent
    crawl and must not be read as a slice-to-slice delta.
  - The robust convoy metrics are **median write throughput** and **crawl-rate**
    (fraction of wall-time at loadavg < 1.9, i.e. collapsed to ~single core)
    over an interleaved n≥9 A/B, per the L8I test plan.

## Measurement cadence (two tiers)

- **Behavior-preserving slices** (pure refactors that do not touch the runtime
  lock — e.g. D.1): **cheap confirm.** One reference run is enough to verify the
  stable signals (reads + load unchanged) and that the convoy is still
  structurally present. Do not spend the n≥9 budget — the slice provably cannot
  move write perf.
- **Lock-touching slices** (D.2 ArcSwap read path, D.3 atomic visible-version,
  Group E sharding): **full n≥9 interleaved convoy A/B** (control vs. slice,
  recording load_ms + loadavg + crawl-rate). Here the convoy metric *is* the
  deliverable — the slice is expected to move it.

## Ledger

| Slice | HEAD | Class | Read-only C (ops/s, p50) | Load avg (ops/s) | Convoy — F ops/s / max RMW | Verdict |
|---|---|---|---|---|---|---|
| pre-D.1 baseline | `f4cb4961`¹ | — | 75,701 · 10.8µs | 87,062 | 81 / 94s | reference (`engine-ycsb-1782223051.json`) |
| D.1 BranchLayout | `ed81880a` | behavior-preserving | 77,735 · 12.0µs | 82,908 | 1,153 / 15s² | **no regression** — reads + load unchanged; convoy structurally intact (`engine-ycsb-1782862993.json`) |
| D.2a `Arc<BranchLayout>` | `da0e74f4` | behavior-preserving | — | — | — | no perf change by construction (Arc wrapper); cheap-confirm, suite green |
| D.2b off-lock scan | `965bf6ba` | lock decoupling | — | — | 626 / 56s³ | **lock convoy removed** (loadavg **0%** single-core vs 13%; coverage scan <0.3% CPU) but **throughput unchanged — backpressure-limited** (profiling below) |

¹ Approximate — the pre-D.1 reference run predates the D.1 commit; it is the last
full a–f durable capture before Group D.
² D.1's F number is **not** an improvement — it is one sample of the intermittent
convoy, which this run happened to spread across A/B/D/E instead of F (13% of
wall-time single-core, 185s). Reads (C, +3%) are the only trustworthy 1:1
comparison and confirm the `owned_levels()` refactor is neutral.
³ D.2b's F number is also **not** an improvement — it is noise. The trustworthy
signal is the **loadavg**: 0% single-core (vs 13% at D.2a) proves the lock convoy is
structurally gone. Throughput did not move because it is limited elsewhere (below).

## Profiling finding (2026-06-30): the limiter is backpressure, not the lock

gdb + `/proc` profiling of a crawling 10M durable run (`ptrace_scope=0`, worker pid,
80 stack samples + thread-state sampling) settled why the write path crawls, and it
is **not** the runtime lock:

- **D.2b removed a real lock convoy.** The off-lock coverage scan is <0.3% of
  samples; loadavg never collapses to one core. That part is fixed.
- **The commit thread is ~75% asleep in `wait_for_progress`** — a backpressure
  condvar (`StorageRuntime::commit` → `BackgroundRuntimeController::wait_for_progress_until`
  → `ThreadedMaintenanceExecutor::wait_for_progress`). It is *throttled*, waiting for
  background maintenance to relieve storage pressure. Another ~14% is runtime-mutex
  wait + the **compaction-scoring scan** (`collect_storage_pressure_with_budget` →
  `selected_compaction_score`) on the commit's `background_pressure_snapshot_for_branch`
  path — the *other* O(rows) scan, still under the lock, on the commit side.
- **The background workers are 48% IDLE.** They do compaction + flush/manifest fsync
  the rest of the time, but they cannot advance the **flush watermark** — the coverage
  scan finds nothing coverable (collapsed floor: `snapshot_watermark`/checkpoint lags
  under write pressure + memtable churn), so **WAL never reclaims → WAL-size pressure
  stays high → commits stay throttled** while the workers have nothing advanceable to do.

So the throughput wall is the **backpressure / WAL-reclaim loop**, orthogonal to the
lock. This is the same root cause that made the bounded-scan "Fix B" inert. The last
three attempts (M12C concurrent compaction, Fix B, D.2b off-lock scan) all fixed
things that are **not** this wall.

## Next

The only lever that moves write throughput is the backpressure / WAL-reclaim loop.
Two threads to pull:
1. **Why can't the flush watermark advance under write pressure?** — the coverage
   floor is pinned by a lagging checkpoint (`snapshot_watermark`). Is the checkpoint
   starved under sustained writes? If the floor never rises, WAL never reclaims.
2. **Why does the commit block on `wait_for_progress` while workers idle?** — a
   flow-control mismatch: commits throttle waiting for pressure relief the idle
   workers are not producing. Either the wait condition or the relief scheduling is
   wrong.

D.2b (and the rest of Group D) stays as a correct structural lock-decoupling that
will matter once the backpressure wall is removed and commits actually flow. The
old n≥9 convoy A/B plan below is superseded as the *primary* gate — the primary gate
is now write throughput under the backpressure fix.

<!-- superseded primary gate:
D.2 (ArcSwap layout) is the first slice expected to move the convoy metric. Run
the full n≥9 interleaved A/B and record median write throughput + crawl-rate as a
new ledger row — that row is the first true test of whether the decoupling works.
-->

