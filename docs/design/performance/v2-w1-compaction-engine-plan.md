# V2-W1 — Compaction Engine: implementation and test plan

Status: **W1.1 recon complete, design ready for review** (2026-07-07). Workstream W1 of
`billion-scale-roadmap-v2.md`. Branch: `v2-billion-scale-perf`. Change class per slice:
intentional semantic change (compaction scheduling); assurance S3 (recovery oracle +
fault sweeps gate every merge-path change).

## Problem (from the roadmap)

Compaction runs serialized, unbounded passes. `plan_l0_to_l1_compaction` takes EVERY L0
table plus EVERY overlapping L1 table in one pass (`table_refs_at_level(0,
0..input_count)` + `overlapping_refs_for_output_range`, state/compaction.rs:835-844); at
10M x 1KB one pass rewrites GBs (~50s), so L0-blocking relief is a ~50s lottery (A max
0.4-48s across runs) and sustained ingest paces against ever-growing debt (load 90K
rows/s vs RocksDB 1.02M). 100M smoke (in flight) additionally shows mid-load space
amplification ~38x — superseded tables outlive their usefulness by whole pass-latencies.

## Recon findings (all anchors verified on `95f49a9e`)

1. **Subcompactions already exist and ship DARK.**
   `prepare_branch_compaction_plan_bounded(request, plan, bounds, index)` builds one
   half-open physical-key range of a pass with salted output identities
   (state/compaction.rs:499-512); `rewrite_publication.rs:136-208` splits a pass into
   `subcompaction_cap()` disjoint ranges — but `DEFAULT_SUBCOMPACTIONS = 1` behind
   `STRATA_SUBCOMPACTIONS` (lifecycle/maintenance.rs:1739-1749). W1.2 is therefore a
   bake-off + default flip + lane scheduling, not a build.
2. **Partial passes already exist for the bottommost level.**
   `BranchCompactionKind::CompactBottommostLevel { start_table_index, table_count }` —
   the plan layer already understands bounded table ranges; L0->L1 is the only
   unbounded-by-construction kind.
3. **Publish-time candidate revalidation exists** (used by the concurrent-worker
   dispatch: "a conflicting compaction that slips through is rejected at publish by
   candidate revalidation") — partial passes can reuse it unchanged.
4. **The off-lock Build machinery (BS5.3b/BS5.5) already runs compaction builds without
   the runtime lock**, so parallel lanes do not reopen BS5 lock territory.

## Design

### W1.1 — Bounded L0->L1 passes

Add an input bound to `plan_l0_to_l1_compaction`: select the OLDEST-first prefix of L0
(`0..k`) whose input bytes + L1 overlap bytes fit `max_pass_input_bytes` (policy; default
~256MiB), instead of all of L0. Correctness argument for partial L0 consumption:

- Rows are `(physical_key, commit_version)`-keyed; reads merge sources by version, so
  moving any L0 subset into L1 cannot change read results — shadowing is by version,
  not by source position.
- Recency ordering: **verified — L0 installs at index 0 (`state.rs:223,306`), so
  `owned_levels[0]` is NEWEST-FIRST and the oldest-first consumption unit is the index
  SUFFIX `(len-k)..len`**, not the 0..k prefix. Consuming the suffix keeps every
  remaining L0 table newer than everything moved to L1; the level invariant "Ln+1 rows
  are older than overlapping L0 rows" is preserved because suffix consumption cannot
  leapfrog a newer table past an older one. (A non-suffix subset COULD: an older L0 row
  left behind would shadow-invert against the newer row now in L1 during future merges.
  Suffix-only is load-bearing; assert it in the planner.)
- Crash windows: unchanged — the pass publishes through the existing atomic install +
  candidate revalidation; a partial pass is indistinguishable from a small full pass.

Relief semantics: each bounded pass completes in ~seconds and reduces the L0 count
incrementally, so L0-blocking admission relief becomes incremental (roadmap gate:
relief <= 2s, workload A max <= 500ms over 5 consecutive runs).

Slices:
- **W1.1a** planner bound + prefix assertion + unit/differential tests (plan produces
  identical MERGED CONTENT as N bounded passes vs 1 unbounded pass — differential
  oracle over randomized L0/L1 states).
- **W1.1b** scheduler follow-up: a bounded pass that leaves L0 over threshold
  re-enqueues immediately (coalescing handles storms; the coverage hysteresis from
  BS5.5 prevents spin).
- **W1.1c** measure: 10M YCSB A x5 runs (max, p99.9, throughput), l9 10M ladder.

### W1.2 — Parallel lanes + subcompactions (un-dark)

- **W1.2a** subcompaction bake-off: `STRATA_SUBCOMPACTIONS={1,2,4,8}` on the 10M load +
  compaction-throughput microbench; pick default (likely min(4, cores/4)); flip default
  with the same discipline as BS3.4c (saturation cells + full battery).
- **W1.2b** concurrent lanes: dispatch non-overlapping level pairs (L1->L2 alongside
  L0->L1) across maintenance workers — the rewrite-conflict check
  (`rewrite_conflicts_with_active`) already exists for exactly this; lift the effective
  one-build-at-a-time constraint (`has_active_build_task` gating) to per-branch
  per-level-pair granularity.
- Gate: sustained compaction throughput >= 600MB/s dev box; debt stabilizes under
  100MB/s ingest.

### W1.3 — Level shape at scale

Re-derive level targets for >=10GB datasets (current: 256MiB max base x10 growth,
v1-era). Add the space-amp exit gate from the 100M smoke finding (steady-state disk <=
3x logical after GC settles; mid-load transient bounded by pass size + GC cadence).
Measured write-amp <= 12 at 10M.

### W1.4 — Pacing re-calibration

With debt controllable: re-tune graded ramp knee/floor so steady ingest ~= compaction
capacity. Gate: load-seq >= 400K rows/s at 10M (stretch 700K with W3 WAL batching).

## Test plan

- Differential merge oracle (W1.1a): bounded-pass sequences vs single unbounded pass
  produce identical visible rows + identical tombstone semantics across randomized
  states (proptest-style over table counts/overlaps/versions).
- Recovery oracle + fault sweeps at every pass boundary (crash between bounded passes =
  crash between small full passes; assert via existing sweeps + one new sweep point).
- Prefix-violation assertion test (planner rejects non-prefix subsets).
- Saturation cells every slice: 4T per-writer, sustained 10-min soak, YCSB A x5 variance.
- Standing three-way per W1.x landing (W6 discipline).

## W1.1c result (2026-07-07): gate FAILED — attribution re-sequences the workstream

5× YCSB A durable 10M with bounded L0→L1 passes: update maxes 21s / 55.3s / 1.1s /
50.1s / 1.8s — the stall lottery survived. Per-kind counters (l9 10M load): **205 of
229 passes are MID-LEVEL (`CompactLevel`)**, L0→L1 only 24, with the single compaction
lane busy ~100% of the load's wall. The stalls are L0-blocked writers queueing behind
mid-level lane occupants, which W1.1's bound does not touch. A mid-level pass cannot be
input-trimmed (its input is already one table; the unbounded part is the L(n+1)
overlap, and splitting overlap requires splitting the input's key range — i.e., the
already-existing subcompaction machinery). W1.1a/b remain correct and necessary
(bounded L0→L1 + chaining are prerequisites for lane fairness) but insufficient alone.

**Re-sequenced:** W1.2 is pulled forward as the critical path —
W1.2a (subcompaction bake-off, machinery exists dark) cuts each monster's wall-time
N×; W1.2b (concurrent lanes) lets L0→L1 run beside mid-level passes. W1.3's smaller
L1 output targets then bound per-file overlap structurally.

## W1.2a result (2026-07-07): split extended; bake-off NO-WIN; RSS escalates to blocker

The subcompaction split now covers mid-level (`CompactLevel`) and bottommost rewrites
(was L0→L1-only gating; the boundary derivation was already candidate-generic), with a
mid-level reunion differential test. But the bake-off did NOT move the gate:
`STRATA_SUBCOMPACTIONS=4` on YCSB A 10M produced maxes 25.8s/27.6s and WORSE throughput
(449–461 vs 764 ops/s at SUBC=1) — likely 4-way build threads contending with the
16-worker pool. Default stays 1; no flip without a win.

**Two escalations:**
1. **One SUBC=1 run OOM-killed at 61.3GB anon RSS — single-mode durable, 32g budget.**
   The RSS-vs-budget gap (roadmap T4, task #59) is now an active W1 BLOCKER and a
   probable confound in every stall measurement (RSS pressure evicts page cache → I/O
   collapses → compaction slows → stalls lengthen). T4 attribution must run BEFORE
   further compaction scheduling work — the stall numbers cannot be trusted until
   memory is honest.
2. Counter-based lane attribution has now missed twice (W1.1c bound, W1.2a
   parallelism). Per W6 discipline: the next step is a STACK PROFILE of a live stall
   window (gdb sampling of maintenance workers + the blocked writer during a
   multi-second max) plus an RSS timeline, before any further scheduling changes.

## T4 attribution results (2026-07-07 evening)

1. **Every engine-level bench bin was silently running GLIBC MALLOC.** The jemalloc
   `#[global_allocator]` lives in the benchmark LIB crate, and bins that never
   reference the lib don't link it — only `storage-next-concurrent-writers` did. All
   engine-ycsb evidence to date (three-ways, stall investigations) was measured under
   glibc; deltas remain valid (consistent within themselves) but absolute numbers and
   the RSS story carried an allocator confound. Fixed: every bin now
   `extern crate strata_benchmarks`, and the probe prints live jemalloc gauges
   (`tikv-jemalloc-ctl`: allocated/active/resident/retained per phase).
2. **The RSS runaway is APP-HELD, not allocator retention.** With jemalloc truly
   active: post-load allocated=13.66GB (block cache filling its 15GiB pool —
   plausible), post-run allocated=**39.25GB ≈ resident 41.84GB** under a 32g budget —
   +26GB of LIVE heap accumulated during the run phase's compaction churn. Stalls
   persist under jemalloc (max 56.3s) — the allocator was not the stall cause either.
3. Hypotheses eliminated by code/counter checks: rewrite outputs DO lazy-reopen
   (BS4.4l applies to compaction), the block cache DOES enforce per-shard eviction.
4. **Next (the named-structure step): jemalloc sampling heap profiler** —
   `tikv-jemallocator` `profiling` feature, `_RJEM_MALLOC_CONF=prof:true,prof_final:true`
   (note the `_RJEM_` prefix; plain MALLOC_CONF is ignored by the prefixed build),
   dump analyzed via jeprof/`jemalloc_pprof`. One 10M run names the 26GB holder.

## Sequencing (revised)

W1.1a ✅ -> W1.1b ✅ -> W1.1c ❌ -> W1.2a ✅(split extended; no-win; default stays 1) ->
**T4 RSS attribution (task #59, now blocking) + stall-window stack profile** ->
re-attribute -> W1.2b/W1.3 as the profile directs -> re-run the W1.1c gate -> W1.4 ->
W1 exit. The W1.1+W1.2 slices PR to v1 together once the gate passes.

## Open items

- 100M smoke (running): fold space-amp + reopen-at-100M numbers into W1.3's gates.
- ~~L0 ordering assertion~~ RESOLVED: installs are `insert(0, ...)` — newest-first;
  the consumption unit is the oldest-first SUFFIX. Plan text updated.
- `max_pass_input_bytes` policy home: LifecycleCompactionIoPolicy (the existing
  max_bytes_per_task defers oversized plans — W1.1 TRIMS instead of deferring; the
  deferral path remains as the backstop for single-table-oversized cases).
