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
- Recency ordering: L0 tables are appended in flush order (oldest first in
  `owned_levels[0]`); consuming a PREFIX keeps every remaining L0 table newer than
  everything moved to L1. The level invariant "Ln+1 rows are older than overlapping L0
  rows" is preserved exactly because prefix consumption cannot leapfrog a newer L0
  table past an older one. (A non-prefix subset COULD: an older L0 row left behind
  would shadow-invert against the newer row now in L1 during future merges. Prefix-only
  is load-bearing; assert it in the planner.)
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

## Sequencing

W1.1a -> W1.1b -> W1.1c (measure) -> W1.2a (bake-off) -> W1.2b -> W1.3 -> W1.4 -> W1 exit
(roadmap M-A gates: A >= 30K, tails bounded, debt stable).

## Open items

- 100M smoke (running): fold space-amp + reopen-at-100M numbers into W1.3's gates.
- L0 ordering assertion: verify owned_levels[0] append order == flush recency order in
  code (one recon check in W1.1a before relying on prefix semantics).
- `max_pass_input_bytes` policy home: LifecycleCompactionIoPolicy (the existing
  max_bytes_per_task defers oversized plans — W1.1 TRIMS instead of deferring; the
  deferral path remains as the backstop for single-table-oversized cases).
