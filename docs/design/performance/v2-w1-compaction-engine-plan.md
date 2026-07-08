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
4. **NAMED (jemalloc heap profile, peak dump of 396K high-water dumps, 25.75GB live):**
   - **20.57GB — `ImmutableTableStreamingEncoder::flush_current_block` → `RawVec::finish_grow`**
     via `PendingCompactionOutput::push_row` ← `TableCompactor::compact_inputs` ←
     `prepare_branch_compaction_plan_bounded` ← background compaction builds.
   - 2.53GB — same encoder path via the flush-build caller; 1.72GB — the artifact rows
     vec. All one family: **build artifacts buffer each output table's COMPLETE encoded
     bytes (plus decoded rows) in heap until the whole pass publishes.** Concurrent
     builds × unbounded mid-level pass outputs (GBs per pass) × geometric Vec growth =
     the 26–50GB peaks and both OOMs. It also completes the stall mechanism: a monster
     pass transiently allocates ~2× its output bytes, evicting the page cache and
     collapsing I/O for everything else.
   - **Fix (W1.2c, new critical slice): stream outputs to disk as they complete** —
     publish each output table object inside the build loop (the per-object publish and
     orphaned-partial-publish cleanup already exist) instead of accumulating all
     artifacts to the end of the pass; heap per build drops to ~one in-progress block +
     table. Bounds memory independently of pass size, which ALSO de-fangs the mid-level
     monsters (their harm was heap+page-cache, in addition to lane time).

## W1.2c result (2026-07-07): memory FIXED; stalls persist — attribution still open

Streaming output publish landed: each completed output table publishes (and frees)
inside the build loop via a sink threaded through `compact_inputs_into` →
`prepare_branch_compaction_plan_bounded_into` → `build_range_with_publishing_sink`;
partial-publish cleanup unchanged (the sink's published list feeds the same
`partial_publish_error`). Measured (YCSB A 10M, gauges): post-run allocated
**39.25GB → 16.92GB**, retained **49.5GB → 5.25GB** — the +26GB accumulation and the
50GB VM high-water spikes are gone; resident (~18GB) now tracks block cache + memtables.
Flush-side accumulation (~2.5GB) remains as a smaller follow-up (same sink pattern
through the flush build).

**Stall lottery persists (max 46.1s)** — eliminating, in order: L0→L1 pass size
(W1.1), build parallelism (W1.2a), memory pressure (W1.2c). Every indirect theory is
now dead; per the W6 rule the ONLY next step is a live stack sample of a stall window
(what every maintenance worker and the blocked writer are doing during a multi-second
max) — no further scheduling or memory changes until that lands.

## Stall-window stack sample (2026-07-07): ROOT CAUSE NAMED

Method: YCSB A durable 10M run under gdb (ptrace_scope=1 forces gdb-as-parent),
SIGINT all-thread backtraces every 4s through the run phase; 32 samples; the run
reproduced the stall (update max 45.9s; probe `block_wait_ms=46301` over ONE episode
— the whole max is a single blocked commit). Stack evidence, samples S9–S25 (~64s
window covering the 46s stall):

1. **Blocked writer** (all 17 samples, identical stack): `KvService::put →
   StorageRuntime::execute_commit → wait_for_progress_until →
   ThreadedMaintenanceExecutor::wait_for_progress` — the L0-blocking admission wall,
   waiting for L0→L1 relief.
2. **Workers are NOT saturated**: rewrite lane cap is 4 (`DEFAULT_COMPACTION_LANES`),
   4 background workers exist, and at most TWO ran compaction at any sample (S13:
   T3+T4 both inside `prepare_durable_compaction_publication`); T2 idle in
   `worker_loop` for the entire window. Capacity was available the whole time.
3. **What the busy workers ran**: continuous mid-level pass builds — read →
   `decode_physical_key` → merge → `ImmutableTableStreamingEncoder` → crc32 → 62.7MB
   single `write_all` → fsync per output table (streaming publish per W1.2c working
   as designed) — one pass grinding for ~60s of samples.
4. **Why L0→L1 relief could not run**: dispatch skips candidates that conflict with
   an in-flight rewrite via `rewrite_tasks_conflict` (maintenance.rs): same branch
   AND `level.abs_diff <= 1`. Compaction task scope carries the SOURCE level, so an
   in-flight L1→L2 (level 1) excludes L0→L1 (level 0). Publish-time candidate
   revalidation would reject it anyway — the conflict is real at level granularity.

**Root cause chain**: L0→L1 outputs build L1 tables with UNBOUNDED next-level
(grandparent) overlap — a single full-keyspan L1 table's L1→L2 pass must rewrite its
entire L2 overlap (multi-GB, W1.1c's one-table-input monsters). While any such pass
is in flight, the level±1 conflict rule (correctly) excludes L0→L1, so the L0 wall's
relief waits out the monster: 40–70s. Pass size bounding (W1.1a) never applied to the
monsters, subcompactions (W1.2a) split the build but not the conflict window, and
memory (W1.2c) was a co-symptom, not the cause.

**Fix = W1.3a, grandparent-overlap-bounded output cutting** (RocksDB's
`max_compaction_bytes` mechanism, compaction_job.cc `ShouldStopBefore`): when an
L(n-1)→L(n) pass emits output tables, cut table boundaries so each output overlaps at
most B bytes of L(n+1). Every future mid-level pass then has bounded inputs
(~table + B), the conflict window collapses from ~50s to seconds, and the L0 wall's
relief latency becomes bounded. The cut machinery exists (`finish_current` already
cuts on size); the compactor needs grandparent boundary keys from the planner.
W1.2b (conflict granularity below level±1) is NOT the right first move: with
full-keyspan L1 tables, table-set conflicts would still collide — shape first.

## W1.3a design: grandparent-overlap-bounded output cutting (implemented)

RocksDB analogue: `max_compaction_bytes` enforced by
`CompactionOutputs::ShouldStopBefore` (db/compaction/compaction_outputs.cc) — output
files are cut at grandparent-overlap limits so future compactions of those files have
bounded inputs.

Mechanism, three layers (mirrors the W1.1a/W1.2c seams):

1. **Table** (`table/compaction.rs`): `CompactionOutputCutHints` — sorted grandparent
   start boundaries (physical keys) weighted by table byte counts, plus
   `max_overlap_bytes`. `TableCompactor::with_output_cut_hints` attaches them per
   pass. A `GrandparentCutTracker` walks boundaries alongside the merged-row cursor
   (both are ascending — O(1) amortized) with interval accounting: grandparent `i`
   covers `[start_i, start_{i+1})`; an output's overlap is the byte sum of intervals
   its span touches, including the interval containing its first row. Cut fires
   before appending a row when the output (a) already spans ≥1 CROSSED boundary and
   (b) its overlap exceeds the bound — never inside one physical key (the size-split
   invariant) and never on an empty output. The crossing requirement means one
   grandparent larger than the bound cannot force degenerate near-empty outputs;
   worst-case overlap is `bound + one grandparent`. Both cut causes (size,
   grandparent) rebase the tracker to the interval containing the next output's
   first row. `TableCompactionReport.grandparent_cut_count` records fires.
2. **Branch** (`branch/state/compaction.rs`): `BranchCompactionRequest.
   output_grandparent_overlap_max_bytes` (None = pre-W1.3a). `grandparent_cut_hints`
   collects `(first_physical_key, byte_count)` for every table at
   `candidate.output_level() + 1`; None when the bound is unset, the output level is
   bottommost, or the grandparent level is empty — behavior then byte-identical to
   before.
3. **Lifecycle** (`lifecycle/compaction.rs`):
   `OUTPUT_GRANDPARENT_OVERLAP_MAX_BYTES = 256MiB` (matches
   `L0_PASS_MAX_INPUT_BYTES`; a future one-table mid-level pass reads ~table+bound ≈
   seconds), applied to every kind in `branch_request()` — all entry points route
   through it; bottommost kinds get no hints downstream, so universal application is
   exact.

Correctness argument: cutting changes only WHERE the sorted, non-overlapping output
partition is cut — same merged rows, same order, more tables. Version shadowing and
history are table-boundary-independent. Guarded by differential oracles at the table
layer (cut vs uncut row streams byte-identical) and the branch layer (full per-key
history equality, W1.1a pattern), plus a physical-key-grouping test and the
no-degenerate-outputs test. Durable format untouched (outputs are ordinary tables).

Expected effect: new L1 tables (and deeper) are created with ≤256MiB next-level
overlap, so every future mid-level pass is bounded ≈ table + 256MiB (~3-5s at the
observed ~100MB/s build rate) instead of multi-GB — the conflict window that holds
L0 relief collapses proportionally. Shape adopts progressively as passes rewrite
old full-span tables; the 5× YCSB A gate measures the steady state.

### W1.3a gate result (2026-07-07): STALL LOTTERY ELIMINATED

5× YCSB A durable 10M @ 32g (fresh DB per run, one process per run):

| run | update max | run ops/s | block_wait_ms total | post-run allocated |
|---|---|---|---|---|
| 1 | 3.00s | 1,516 | 3,332 | 16.81GB |
| 2 | 4.59s | 1,011 | 4,644 | 16.75GB |
| 3 | 3.17s | 1,419 | 4,477 | 16.80GB |
| 4 | 3.63s | 830 | 4,194 | 16.81GB |
| 5 | 1.82s | 2,452 | 2,241 | 16.77GB |

Pre-W1.3a lottery: maxes 21s / 55s / 1.1s / 50s / 1.8s (W1.1c gate) and 45.9–56.3s in
the stack-sample era; single blocked episodes of `block_wait_ms=46,301`. The 40–70s
class is GONE: worst max across five runs is 4.6s — the predicted one-bounded-pass
relief window (~table + 256MiB at ~100MB/s), reproduced five-for-five. Run throughput
rose (median 1,419 vs 610–802 pre-slice); wait_timeouts remain 0; W1.2c's memory line
holds (~16.8GB post-run, retained ≤6.6GB). Load throughput dipped on some runs
(85.7–107K vs ~115K) — the write-amp cost of cutting, recorded as the W1.4
calibration input.

The literal W1.1c criterion (max ≤500ms) is NOT met and is superseded: the residual
1.8–4.6s max is the bounded relief window itself (one in-flight bounded pass +
graded near-stop pacing), not a lottery. Tightening it further is a calibration
trade (smaller bound = more write amp — W1.4) or a conflict-granularity change
(W1.2b), both now optional rather than blocking.

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
