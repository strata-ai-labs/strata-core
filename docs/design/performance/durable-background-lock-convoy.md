# Durable Background Maintenance Lock Convoy — Root Cause

**Status:** Root cause **pinned** with userspace stack evidence (engine path, n>1).
**Date:** 2026-06-22
**Branch:** `v1-billion-scale-perf`
**Supersedes:** the RC1 conclusion of
[`durable-load-amplification-evidence.md`](./durable-load-amplification-evidence.md)
(that doc measured the unrepresentative L9 inline path; see
[§ Relationship to prior evidence](#relationship-to-prior-evidence)).

## TL;DR

A durable load intermittently (~30 % of runs) **collapses to a single core and
crawls** — 3–5× slower, sometimes never finishing. It is **not** single-threaded
compaction, **not** memory pressure, and **not** disk I/O. It is a **lock convoy
on the one global runtime mutex** (`parking_lot::Mutex<LifecycleDurableLocalRuntime>`).

The mutex is held across an **O(total-rows)** computation:
`run_next_background_flush_watermark_maintenance` →
`branch_durable_commit_versions_at_or_below`, which **scans every row of every
table** in the branch and **sorts** the resulting `Vec<CommitVersion>` — *while
holding the global lock*. All other background workers and the foreground commit
thread block on that mutex. As the branch grows, the scan's lock-hold time grows
until it dominates; past that point the convoy is self-sustaining and throughput
falls to whatever one core can do.

This single bug explains every earlier mis-diagnosis (see
[§ Why this explains the false positives](#why-this-explains-the-false-positives)).

## The symptom (measured, engine path)

All measurements use the **engine path** (`benchmarks/.../engine-ycsb`, durable
mode), the shipping product path — not the L9 inline tool. Host: 61.9 GiB RAM,
single non-dedicated box. Workload: 10M records, 1 KB values.

### It completes fine *most* of the time — and the slowdown is not budget-driven

Budget sweep (n=1 per budget), with the memory confound instrumented:

| Budget | Load throughput | Load time | peak RSS | peak swap | min avail |
|--:|--:|--:|--:|--:|--:|
| 24 GiB | 77,268 ops/s | 129 s | 24.5 GB | 6.4 GB | 31.8 GB |
| 32 GiB | 79,359 ops/s | 126 s | 25.6 GB | 6.5 GB | 31.6 GB |
| 40 GiB | 32,219 ops/s | **310 s** | 25.9 GB | 6.5 GB | 31.0 GB |
| 48 GiB | 80,417 ops/s | 124 s | 33.9 GB | 6.5 GB | 22.7 GB |

The 40 GiB outlier has *completely normal* memory (swap is flat at a pre-existing
6.4 GB; min-available never drops below 22 GB on any run). **Not memory pressure.**

### It is intermittent, ~30 %, and correlates with LOW load, not high

Interleaved repeated runs (3 reps × 3 budgets), recording load time and the 1-min
`loadavg` at end of run:

| Budget | rep 1 | rep 2 | rep 3 |
|--:|--:|--:|--:|
| 32 GiB | 124 s (la 2.46) | 130 s (la 3.15) | **TIMEOUT** (la 1.52) |
| 40 GiB | 128 s (la 3.09) | 144 s (la 2.67) | 122 s (la 2.44) |
| 48 GiB | 129 s (la 2.90) | **354 s** (la 1.50) | **TIMEOUT** (la 1.08) |

- ~3 of 9 runs crawl, on **both** 32 GiB and 48 GiB — budget-independent and
  intermittent.
- **Fast runs sit at loadavg ~2.8 (2–3 cores); crawls sit at ~1.1–1.5 (~1 core).**
  This is the *opposite* of external contention — the engine intermittently
  **collapses to single-core operation.**

### When it collapses, threads are *blocked on a futex*, not computing or doing I/O

Per-thread sampling (`/proc/<tid>/{comm,stat,wchan}`) during a caught crawl vs a
fast run:

| | Running (R) | **Blocked on futex (S)** | Disk (D) |
|---|--:|--:|--:|
| Fast run | 69 (59 %) | 43 (37 %) | 5 |
| Crawl steady-state | 43 (27 %, all ≤ 50 %) | **116 (72 %)** | 1 |

In the crawl no thread is pegged (the one semi-active worker sits at 20–50 %); the
rest are parked on `futex_do_wait`. Fast runs have 3–4 `strata-storage-` workers
running at 70–99 % *in parallel*. The trigger at the tip-over is a burst of
`jbd2_log_wait_commit` (an ext4 journal/fsync stall) — fast runs shrug those off;
a crawl run falls into the convoy after one and never recovers.

## Root cause (userspace stacks, gdb)

`perf` is restricted (`perf_event_paranoid=4`) and `gdb -p` attach is blocked
(`ptrace_scope=1`), so the engine was run **under gdb as its parent** and
all-thread backtraces were snapshotted (8×) while stalled. Findings, consistent
across snapshots:

### The lock

A single `parking_lot::Mutex` wrapping the **entire** durable runtime:
`Arc<ParkingMutex<LifecycleDurableLocalRuntime<…>>>`
(`api/runtime/background.rs:14`). Every background drain and every foreground
commit must take it.

### What holds it — an O(N) scan under the lock

`drain_durable_background_round` (`api/runtime/maintenance.rs:559`) takes
`runtime.lock()` for the **whole round** (`:573`) and, inside it, calls
`run_next_background_flush_watermark_maintenance()` (`:579`). That method
(`lifecycle/durable/maintenance.rs:2135`) →
`flush_watermark_task_has_table_coverage` (`:2209`) →
`flush_watermark_candidate_has_table_coverage` (`:2282`) →
`LifecycleTableManifestFlushCoverageProof::from_branch_manifest`
(`lifecycle/checkpoint.rs:614`) → `branch_coverage_from_state_and_manifest`
(`:1022`) → **`branch_durable_commit_versions_at_or_below`** (`:1041`):

```rust
pub(crate) fn branch_durable_commit_versions_at_or_below(
    branch: &BranchLocalState, candidate: CommitVersion,
) -> Vec<CommitVersion> {
    let mut versions = Vec::new();
    for table in branch.owned_levels().iter().flatten() {       // every table
        versions.extend(table.rows().iter()                      // EVERY ROW
            .map(TableRow::commit_version).filter(|v| *v <= candidate));
    }
    for layer in branch.inherited_layers() { /* … every layer's tables/rows … */ }
    versions.sort();                                             // multi-million Vec
    versions.dedup();
    versions
}
```

This is **O(total rows in the branch)** — for a 10M-row load it scans millions of
rows, builds a multi-million-element `Vec<CommitVersion>`, and sorts it, **on every
flush-watermark drain, under the global mutex.** The 8 gdb snapshots show the
holder *rotating* across workers (T4→T2→T2→T3→T4→T5→T3) but always inside this
scan: `core::slice::sort::…merge_down<CommitVersion>`,
`NonNull<TableRow>::add`, `ptr::write<CommitVersion>`, `__memcpy_avx512`.

### The asymmetry that makes it a bug

In the same drain round, the *other* maintenance kinds use the **off-lock
start/build pattern** — `start_next_background_flush_maintenance`,
`start_next_background_checkpoint_maintenance`,
`start_next_background_wal_truncation_maintenance`,
`start_next_background_table_rewrite_maintenance` all *start* a task under the lock
and do the expensive build **off-lock**. Only `run_next_background_flush_watermark_maintenance`
is a `run_*` that performs its heavy work **inline under the lock**. It is the odd
one out.

### What blocks on it — including the commit thread

- The other `strata-storage-` workers park in
  `drain_durable_background_round → Mutex::lock → futex_wait` — they cannot run
  flush/compaction/checkpoint.
- The **foreground commit thread** hit admission backpressure and is parked in
  `StorageRuntime::execute_commit` (`api/runtime/mod.rs:2612`) →
  `background_wait_after_pressure_rejection` (`:2850`) →
  `BackgroundScheduler::wait_for_progress_until` → `Condvar::wait` — waiting for
  background progress that cannot happen because the workers are stuck on the lock.

### The self-reinforcing loop

```
commit throttled (levels backed up)
   → waits for background maintenance progress
      → maintenance serialized behind the O(N) flush-watermark scan
        under the single global runtime mutex
         → compaction/flush can't run → levels back up further
            → more throttling …
```

Scan cost ∝ row count, so partway through the load it crosses the point where it
dominates the lock-hold; past that the convoy is self-sustaining. Intermittent
(~30 %) because it is a threshold crossing whose timing depends on scheduling (and
is nudged by the occasional `jbd2` fsync stall at the tip-over).

## Why this explains the false positives

This one bug produced a chain of mis-diagnoses earlier in M12:

- **"Single-threaded compaction can't keep up."** Compaction is one of the drains
  *blocked behind the lock*; L0 backs up while a worker scans. The symptom (one
  core at ~100 %, L0=60) is real; the *cause* is the lock holder, not compaction.
- **M12C (concurrent compaction) regressed.** Adding rewrite workers adds *more
  contenders on the same global mutex* — it amplifies the convoy. Exactly the
  measured regression (worse footprint, admission blocking).
- **"Memory pressure at 48 GiB."** Budget-independent; it is lock-hold time ∝ rows,
  not memory. The earlier 48 GiB "proof" was confounded by a memory-pressured box
  *and* this intermittent convoy.
- **The write-cliff doc's "global runtime mutex" intuition** was directionally
  right after all — not as constant commit-count churn (fixed by size-driven
  flush, `67219ba1`), but as this one path still doing heavy work under the lock.

### Relationship to prior evidence

[`durable-load-amplification-evidence.md`](./durable-load-amplification-evidence.md)
correctly **refuted** the `#3a` count-vs-byte trigger, but its positive conclusion
("RC1: single-threaded maintenance cannot keep up; 99.4 % of maintenance
coalesced") was measured on the **L9 inline `api` path**, which is not how the
engine runs maintenance (background worker pool). Treat that doc's RC1/SA2 numbers
as **path-specific and superseded** by this document for the engine path. Its
WAL-bounded finding (RC3) still holds.

## Fix direction (to be designed and validated — not yet implemented)

1. **Primary — get the coverage proof off the lock.** Convert
   `run_next_background_flush_watermark_maintenance` to the same
   *start-under-lock / compute-off-lock / install-under-lock* pattern its siblings
   already use (`start_next_background_*`). This removes the convoy regardless of
   the scan's cost.
2. **Complementary — make coverage O(tables), not O(rows).** Answer "highest
   durably-flushed commit ≤ watermark" from **per-table commit-version ranges**
   (min/max the tables already track) instead of enumerating + sorting every row.
   This removes the wasted work so it does not recur every drain round. *Confirm
   first* that the caller (`branch_coverage_from_state_and_manifest`) needs only
   the floor, not the full version set.

Either fix breaks the crawl; together they are robust. Neither is the M12C
concurrent-compaction work (reverted) nor a byte-trigger change (`#3a`, refuted).

## Validation protocol (control-first)

The decisive test is **not** "make one run fast" — the engine is already fast ~70 %
of the time. A real fix must **drive the crawl rate from ~30 % to ~0** across many
interleaved runs, with no throughput regression on the fast path:

1. Establish the baseline crawl rate on the **unmodified** build at the exact
   config (control first), n ≥ 9 interleaved runs.
2. Apply the fix; re-run the identical harness.
3. Pass criteria: crawl rate ≈ 0; loadavg stays ~2.8 (multi-core) on every run;
   no fast-path regression; per-thread sampling shows no single worker holding the
   runtime mutex across an O(N) computation.

## Reproduce

```bash
# Clean control binary (release):
cargo build --release --manifest-path benchmarks/Cargo.toml --bin engine-ycsb

# Variance harness — interleaved repeated runs, prints load_ms + loadavg:
#   for rep in 1 2 3; do for b in 32 40 48; do
#     engine-ycsb --workload a --records 10m --ops 10k --value-bytes 1000 \
#       --mode durable --memory-budget ${b}g ; done; done
# ~30% of runs collapse to ~1 core (loadavg ~1.3) and run 3-5x slower / time out.

# Pin the lock (perf restricted, gdb -p blocked by ptrace_scope=1 → run under gdb
# as parent, SIGINT-sample all-thread backtraces while the DB-size growth stalls).
```

Raw evidence for this investigation: `/tmp/budget-sweep-out.log`,
`/tmp/variance-out.log`, `/tmp/char-threads-*.log`, `/tmp/lockcap/cap-1.log`
(8 all-thread backtrace snapshots).

## Methodology note

This investigation followed control-first discipline after an earlier chain of
false positives in M12: every claim was made
against the unmodified engine, at multiple operating points (budget sweep), with
the confound (memory) instrumented, on the path that ships, and the symptom was
*directly observed* (per-thread states; userspace stacks) rather than inferred
from proxy counters. The intermittency was caught only because the crawl rate was
characterized over n ≥ 9 runs — a single run would have mis-concluded either
"fixed" or "broken."
