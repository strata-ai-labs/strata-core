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

¹ Approximate — the pre-D.1 reference run predates the D.1 commit; it is the last
full a–f durable capture before Group D.
² D.1's F number is **not** an improvement — it is one sample of the intermittent
convoy, which this run happened to spread across A/B/D/E instead of F (13% of
wall-time single-core, 185s). Reads (C, +3%) are the only trustworthy 1:1
comparison and confirm the `owned_levels()` refactor is neutral. The real convoy
number lands at D.2 under the n≥9 protocol.

## Next

D.2 (ArcSwap layout) is the first slice expected to move the convoy metric. Run
the full n≥9 interleaved A/B and record median write throughput + crawl-rate as a
new ledger row — that row is the first true test of whether the decoupling works.
