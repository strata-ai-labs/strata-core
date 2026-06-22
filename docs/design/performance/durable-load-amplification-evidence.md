# Durable Load Amplification — Conclusive Bottleneck Evidence

**Status:** Measured evidence (pre-implementation gate)
**Date:** 2026-06-22
**Branch:** `v1-billion-scale-perf`
**Tool:** `benchmarks/src/bin/storage_next_l9_scale.rs` (L9 `api` surface, `perf-trace` on)

## Why this document exists

The billion-scale roadmap (`docs/architecture/strata-v1-billion-scale-roadmap.md`)
and the plan-mode slice `#3a` both led with a hypothesis: durable space/write
amplification comes from **count-based over-compaction** — nonzero levels firing
on a table-count trigger *before* reaching their byte target, rewriting data
through the levels more than necessary.

Per the previous engine's hardest-won lesson — *instrument before theorizing; no
fix ships for a bottleneck it did not first measure* — we instrumented the
durable path and measured it **before writing any fix.**

**The hypothesis is wrong.** The measured cause is the opposite of premature
compaction, and `#3a` (a byte-based trigger) would not address it and could make
it worse. This document is the evidence.

## TL;DR

A durable `standard` load, measured at 1M/3M/5M records (1 KB values, 8 GiB
budget), shows a **super-linear collapse** with three compounding, conclusively
identified causes — none of which is the count-vs-byte trigger:

1. **Single-threaded background maintenance cannot keep up (RC1).** At 5M, the
   load spends **849 s in background maintenance**, **197 s** of foreground time
   *blocked waiting on the maintenance lock*, and **48.6 s** in admission stalls
   (119,976 stall events). **99.4 % of maintenance requests are coalesced/dropped**
   (190,128 of 191,211 suggested; only 1,083 scheduled). This is the root cause.
2. **Obsolete tables are never reclaimed (SA2).** At 5M, the database holds
   **2,295 table files on disk but only 155 are live** (L0=64, L1=47, L2=44) —
   **~2,140 dead tables (93 %) un-reclaimed**, even though this is a single
   branch (no copy-on-write sharing to block deletion). Result: **78.2 GB on disk
   for ~5 GB logical = 15.8× space amplification.**
3. **Compaction write amplification ≈ 15×.** Compaction read+wrote **74.5 GB for
   5 GB logical**; each of 5M unique rows is rewritten **~13×** (69M merge input
   rows, 0 dropped — no dedup, these are unique keys re-merged repeatedly).

Crucially, when compaction *does* run, the chosen levels are **4.3–8.5× OVER
their byte target**, not under it — so compaction runs **too late**, not too
early. The `#3a` "fires under target" premise does not hold.

WAL is bounded (≤ 662 MB, 80 segments truncated) — RC3 holds; not a factor.

## Methodology

- Tool: the existing `storage-next-l9-scale` perf-trace runner, which calls
  `perf_trace::reset()` before each phase and `snapshot()` after, against the
  public `strata_storage_next::api` surface (no engine-next overhead).
- **No change to the measured crate.** Two additive changes to the *measurement
  tool only*:
  - `--memory-budget SIZE` — threads an explicit `StorageMemoryBudget` through
    `open_durable_local_with_options`, so the tool can set an operating point
    (the default ~512 MiB profile cannot complete a multi-GB load — see below).
  - a `lifecycle-compaction` line added to the stderr dump, so a scale that
    completes prints its per-compaction counters even if a later scale errors.
- Run: `--scales 1m,3m,5m --engines standard --workloads load-seq
  --value-bytes 1000 --memory-budget 8g --diagnostic-source-shape --keep-dir`.
- On-disk footprint measured with `du` + a file count of the kept directory.

### Operating-point note

At the **default** resource profile the durable load **cannot complete** — it
fails at ~400 K records with `active_mutable exhausted` (a hard admission
*rejection*, used 511 MiB of a 512 MiB cap). With `--flush-every` driving
explicit flushes, a 10M attempt failed differently: `flush follow-up compaction
task was not runnable`. Both are themselves RC1/RC2 signals — the maintenance
pipeline cannot absorb the load — but they also mean the clean amplification
numbers below were taken at a budget (8 GiB) where the load completes via
automatic maintenance.

## Measured data

| Scale | Logical | Load throughput | Load time | On-disk tables | **Space-amp** | Compaction output | **Write-amp** | Compaction ops (l0→l1 / nonzero) | Source shape |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--|
| 1M | 1 GB | 140,886 ops/s | 7.1 s | 1.1 GB | **1.1×** | 0 (not run) | 0× | 0 / 0 | L0=3, nonzero=none |
| 3M | 3 GB | 63,530 ops/s | 47.2 s | 14 GB | **4.7×** | 8.3 GB | 2.8× | 4 / 1 | L0=72, L1=36, L2=17 |
| 5M | 5 GB | 16,667 ops/s | 300.0 s | 78.2 GB | **15.8×** | 74.5 GB | **14.9×** | 16 / 17 | L0=64, L1=47, L2=44 |

Load time is **super-linear**: 5× the data (1M→5M) takes **42× the time**
(7.1 s → 300 s). Throughput collapses 140,886 → 16,667 ops/s. This is the
durable "crawl" reproduced, and it worsens with scale — consistent with the
10M YCSB baseline (durable run as low as 548 ops/s, ~580× slower than RocksDB).

## Finding 1 — Single-threaded maintenance cannot keep up (RC1, the root cause)

Load-phase counters at 5M:

| Counter | Value | Reading |
|---|--:|---|
| `automatic_maintenance_ns` | 849.2 s | background maintenance work during load |
| `background_maintenance_tasks` | 2,530 | … all on one lane |
| `foreground_wait_background_lock_ns` | 197.1 s | commits **blocked** on the maintenance lock |
| `admission_block_wait_ns` | 48.6 s | admission throttle/stall time |
| `admission_wait_attempts` | 119,976 | how often commits hit the wall |
| `maintenance_suggested` | 191,211 | maintenance the engine *wanted* |
| `maintenance_scheduled` | 1,083 | maintenance it actually ran |
| `maintenance_coalesced` | 190,128 | **99.4 % dropped/merged — it is hopelessly behind** |

The pipeline is oversubscribed by ~175×: it coalesces away 99.4 % of the
maintenance it knows it needs. Everything below is a *consequence* of this.

## Finding 2 — Obsolete tables never reclaimed (SA2, the dominant space cost)

At 5M the `tables/` directory holds **2,295 object files (78.2 GB, avg 34.9 MB)**,
but the live source shape is **155 tables** (L0=64 + L1=47 + L2=44). Compaction
*built* 1,216 output tables over the run; flushes built the rest. **~2,140 files
(93 %) are obsolete and were never deleted.** On-disk bytes ≈ the sum of *every*
table ever written (flush + compaction), i.e. essentially nothing is reclaimed.

This is single-branch — `Arc::strong_count == 1`, no COW blocker set — so reclaim
should be trivial. It still does not happen, because reclaim runs on the same
starved maintenance lane (Finding 1) and is coalesced away with everything else.

## Finding 3 — Write amplification ≈ 15×

At 5M, compaction `input_bytes` = `output_bytes` = **74.5 GB** for a **5 GB**
logical dataset. `merge_input_rows` = 69M for 5M logical rows, `dropped_rows` = 0
— so each unique row is re-merged ~13× with no dedup to show for it (these are
unique inserts). 257 s of the run is inside compaction merge alone. With only two
nonzero levels and small per-level targets, the bottom level is rewritten
repeatedly as data trickles down — classic LSM write amplification, made
pathological because compaction runs in large late bursts rather than steadily.

## Finding 4 — Compaction runs LATE, not early (refutes `#3a`)

The `#3a` premise is that nonzero levels compact *before* reaching their byte
target (count-triggered). The measured `selected_byte_count` vs
`selected_target_bytes` says the opposite:

| Scale | Avg selected level bytes | Avg level target | Over-target ratio |
|---|--:|--:|--:|
| 3M | 818 MB | 190 MB | **4.3× over** |
| 5M | 1.80 GB | 211 MB | **8.5× over** |

Levels are selected for compaction at **4–8× over** their byte target, and the
ratio worsens with scale — the signature of compaction falling progressively
further behind (Finding 1), not firing prematurely. A byte-based trigger (`#3a`)
changes *when* a level qualifies; these levels already qualify under *both*
count and byte rules. `#3a` would not reduce this amplification and, by making
the trigger less eager, risks letting levels overflow even further.

## Finding 5 — WAL is bounded (RC3 holds)

`wal_retained_bytes_max` = 662 MB, `wal_retained_segments_max` = 11,
`wal_truncation_deleted_segments` = 80, on-disk WAL ≤ 62 MB at end. The
flush-driven WAL reclaim landed earlier (`ec4063b8`, `67219ba1`) is working.
Do not reopen it; keep the guard.

## Conclusion and roadmap implications

The durable bottleneck is **conclusively** a single-threaded maintenance pipeline
(RC1) that cannot keep up, which **starves obsolete-table reclaim (SA2)** and
forces **late, giant, repeated compactions (≈15× write-amp)** — together
producing ~16× space amplification and a super-linear throughput collapse. It is
**not** the count-vs-byte compaction trigger.

Reprioritization for `M12` (was: lead with `#3a`/byte-score selection):

1. **Promote — concurrent/throughput-capable maintenance (RC1, was M12C).** The
   one-lane drain coalescing 99 % of its own work is the root cause. This is now
   the lead slice, not `#3a`.
2. **Promote — obsolete-table reclaim (SA2, was M12B).** 93 % dead tables on a
   single branch means single-branch reclaim must run *eagerly and cheaply*,
   decoupled from the starved maintenance lane. The COW blocker-set work remains,
   but the immediate win is single-branch eager reclaim.
3. **Reframe — write amplification.** Reduce the ~15× via the level
   ladder / bottom-level rewriting, measured per change.
4. **Demote — `#3a` byte-score trigger (was M12A).** Keep as a tidiness/selection
   change, but it is **not** the amplification fix and must not be sequenced as
   "the force multiplier." Re-validate only after RC1/SA2 land.

Re-measure every change against this same harness at 1M/3M/5M (and 10M once the
load completes), tracking the full vector: load throughput, on-disk bytes, live
vs total table count, compaction input/output bytes, and the maintenance
keep-up counters above.

## Update (2026-06-22): M12B-1 landed — eager single-branch reclaim

The first fix slice — single-branch eager obsolete-table reclaim at compaction
completion — is implemented and re-measured on the same harness. It wires the
reclaim chain end-to-end for the single-branch case: after a compaction's new
manifest is durable, the runtime runs the existing table-object retention proof
and deletes the proven-dead objects directly, decoupled from the coalesced
maintenance lane (mirrors `reclaim_wal_after_flush`; multi-branch defers to the
branch-aware GC frontier). Code: `TableObjectService::delete_object_best_effort`
(`service/table.rs`) + `reclaim_obsolete_tables_after_compaction` hooked into
`run_compaction_maintenance_task` (`lifecycle/durable/maintenance.rs`).

Re-measured (same 1M/3M/5M, 8 GiB, standard, load-only):

| Scale | Metric | Before | After M12B-1 |
|---|---|--:|--:|
| 5M | on-disk tables | 78 GB | **13 GB** |
| | table files (live / total) | 155 / 2,295 | 79 / 174 |
| | space-amp | 15.8× | **~2.6×** |
| | compaction output | 74.5 GB | **6.5 GB** |
| | load throughput | 16,667 ops/s | **88,203 ops/s (5.3×)** |
| | load time | 300 s | **57 s** |
| 3M | on-disk | 14 GB | **4.6 GB** (4.7× → ~1.5×) |
| 1M | on-disk | 1.3 GB | 1.3 GB (no compaction; nothing to reclaim) |

The space win was expected (SA2). The throughput/write-amp collapse was a
cascade: reclaiming dead objects keeps the on-disk structure lean, so compaction
stopped re-merging through bloat (trivial moves rose 2 → 283 at 5M; the
super-linear load collapse went from 42× to ~8× time for 5× data). RC1 (the
single-lane maintenance keep-up) is unchanged and remains the lead for M12C.

**Residual / follow-ups.** ~95 dead tables remain at 5M (down from 2,140; ~96% of
dead objects reclaimed, but live/total is 45% and space-amp 2.6× is still above
the M12B ≤1.5× gate). Tightening: also reclaim on the background-compaction path,
and widen the proof-status gate to accept `CompletedWithHealthDebt` (a complete
listing). So M12B-1 is a large step, not the full M12B close.

**Validation.** storage-next lib suite 3,130 passing (incl. a new
`delete_object_best_effort` unit test); clippy + fmt clean. **Test-track gap:**
the synthetic-input lifecycle harness (`CheckpointTestBackend` + `install_l0_table`)
cannot exercise end-to-end reclaim — its inputs are not real `tables/` objects, so
the retention proof finds nothing to delete (verified: gate passes, status
`Completed`, zero candidates). The benchmark above is the integration validation;
M12TB needs a real-object (flush → compact → reclaim) end-to-end test.

## Reproduce

```bash
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- \
  --scales 1m,3m,5m --engines standard --workloads load-seq \
  --value-bytes 1000 --memory-budget 8g --diagnostic-source-shape --keep-dir
# then: du -sh <root>/standard-5000000-*/tables ; find <...>/tables -type f | wc -l
```

Raw run: `/tmp/l9-proof-sweep.log`; JSON report under
`benchmarks/.benchmark/l9-proof/results/`.
