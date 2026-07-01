# RocksDB-aligned compaction

Implementation + testing plan. **Supersedes** the concurrency-first
[`compaction-concurrency-lever-b-plan.md`](./compaction-concurrency-lever-b-plan.md) —
that plan chased a symptom (single-lane serialization) before the root cause. Companion
to [`compaction-enqueue-lever-a-plan.md`](./compaction-enqueue-lever-a-plan.md) (A.1+A.3,
committed `8298cde3`). Status: **proposal**. Change class: intentional semantic change
(memory budgeting + compaction). Assurance: S3.

We studied the RocksDB source (`db/compaction/*`, `options/*`) to port proven mechanisms
instead of reinventing them. The findings and file:line anchors are in the investigation
notes; this plan is the adoption.

## Problem (root cause, evidence-grounded)

The update-heavy crawl traces to a single **config decision**, not missing machinery. A
per-compaction probe (level, input tables, bytes, duration) over a 10M/48g workload-A run
showed the compaction tail is **entirely L0→L1**:

| level / kind | rewrites | avg | max | max input |
|---|---|---|---|---|
| **L1 (L0→L1)** | 7 | 4,597 ms | **6,365 ms** | **2,114 MB** |
| L6 (bottommost) | 51 | 434 ms | 1,029 ms | 360 MB |
| all other | 656 metadata promotions (instant) | — | — | — |

`2114 MB / 8 input tables ≈ 264 MB per L0 file`. L0 files are memtable flushes, so **the
memtable is ~264 MB**, because `active_rotation_bytes = pool_limit_bytes(ActiveMutable)`
(`lifecycle/budget.rs:1153`) and the `ActiveMutable` pool **scales with the total
`memory_budget`** (`from_total_bytes` allocates `total × 64/512`, `budget.rs:243`). So the
write buffer — and thus every L0 file — grows with RAM. A single L0→L1 then merges ~8 ×
264 MB = 2 GB serially for ~6 s, monopolizing the single Rewrite lane; L0 balloons (to ~250
tables) and writes crawl.

**RocksDB deliberately does not do this.** `write_buffer_size` defaults to a **fixed
64 MB** and `SanitizeOptions` only clamps it to `[64 KB, 64 GB]` — it never raises it from
system RAM (`include/rocksdb/options.h:190`, `db/column_family.cc:235`). RAM goes to the
**block cache**, not the memtable. With RocksDB defaults, total L0 before an L0→L1 fires is
`write_buffer_size × level0_file_num_compaction_trigger ≈ 64 MB × 4 = 256 MB` → a
sub-second merge. Our 2 GB/6 s compaction exists *only because* we grew the memtable to
264 MB. strata already has a `StorageBudgetPool::BlockCache` — the memory model exists; the
budget is just allocated the way RocksDB avoids.

## RocksDB's layered defense (what we adopt)

| Layer | RocksDB mechanism (anchor) | strata today |
|---|---|---|
| 1. Small L0 | `write_buffer_size` fixed 64 MB; RAM → block cache | memtable = budget fraction (264 MB) — **root** |
| 2. Bound any compaction | `max_compaction_bytes` (= `target_file_size × 25`): 2× cap on input expansion (`compaction_picker.cc:599`) + grandparent output-cut (`compaction_outputs.cc:308`) | no input/expansion cap |
| 3. Small output files | output split at `target_file_size` (`compaction_outputs.cc:287`) | already splits (output=36) ✓ |
| 4. Parallelize one compaction | subcompactions: size-anchored key boundaries (`compaction_job.cc:533`); L0→L1 is the sweet spot | none |
| 5. Run compactions concurrently | `being_compacted` file marks (`version_edit.h:274`, marked under the pick lock) **+ explicit output-range overlap check** (`compaction_picker.cc:321`) | single Rewrite lane |

## Goal / non-goals

**Goal.** Adopt RocksDB's compaction memory model and bounds so no single compaction
monopolizes the pipeline, in decreasing leverage order.

**Non-goals / superseded.**
- The Lever B **level-adjacency conflict predicate** is superseded by RocksDB's per-file
  `being_compacted` + output-range check (Slice 3). My B.1 was reinventing #5 incorrectly
  (disjoint inputs alone don't prevent overlapping outputs).
- The **partial-oldest-L0 hack** and the ad-hoc "size bound" are superseded by fixing the
  memtable size (Slice 1) and porting `max_compaction_bytes` (Slice 2).

## Slice 1 — decouple the write-buffer (flush) size from the budget  *[highest leverage]*

RocksDB separates the **per-memtable flush size** (`write_buffer_size`, fixed 64 MB) from
**total memtable memory** (`db_write_buffer_size` / write-buffer-manager, may be large).
strata conflates them: `active_rotation_bytes = pool_limit_bytes(ActiveMutable)`.

**Change.** Make the memtable *rotation* size a fixed modest cap, independent of the pool:

    // lifecycle/budget.rs, active_rotation_bytes_from_budget
    active_rotation = min(pool_limit_bytes(ActiveMutable), MAX_ACTIVE_ROTATION_BYTES)
    // MAX_ACTIVE_ROTATION_BYTES ≈ 64–128 MiB (RocksDB write_buffer_size default = 64 MiB)

The `ActiveMutable` pool stays as the bound on *total* active+frozen memory (so a large
budget still allows many in-flight 64 MB memtables — the RocksDB `max_write_buffer_number`
role), but each L0 file is now ~64 MB. Redirect the budget headroom freed from
`ActiveMutable` toward `BlockCache` (reads) by adjusting the pool fractions in
`from_total_bytes` (`budget.rs:239-251`) — RocksDB's "RAM → cache" stance.

Result: L0 files ~64 MB → L0→L1 ≈ `trigger × 64 MB` ≈ 256 MB → **sub-second**, and L0 stops
ballooning. Expected to largely eliminate the crawl on its own.

Open question to settle in implementation (cheap): confirm the exact `memory_budget →
total_bytes` mapping (the observed 264 MB implies total ≈ 2 GB, not 48 GB — the engine
budget may not pass through 1:1), and whether a single active memtable or a pool of frozen
memtables is used, so the cap + fraction rebalance are set correctly.

## Slice 2 — port `max_compaction_bytes` (bound any compaction)

Even with small memtables, bound compaction size defensively (RocksDB's belt):

- A byte cap on the non-zero **input-expansion** loop (`branch/state/compaction.rs:944-980`)
  and the L0→L1 overlap gather — stop expanding once estimated input exceeds
  `max_compaction_bytes` at a clean cut (mirror `compaction_picker.cc:599`, the `2×` limit).
- **Grandparent output-cut** — when writing L1 output, cut the current file if its size plus
  overlapping L2 bytes would exceed `max_compaction_bytes`, so a future L1→L2 stays bounded
  (mirror `compaction_outputs.cc:303-311`). strata already splits output at target size;
  this adds the grandparent-aware cut.
- Config: add `max_compaction_bytes` to lifecycle config, default `target_file_size × 25`
  (RocksDB's `column_family.cc:403`).

## Slice 3 — concurrent compaction, the RocksDB way (redo Lever B)

Replace the level-adjacency predicate with RocksDB's mechanism:

1. `being_compacted: bool` on the per-table metadata, mutated **only under the pick lock**.
2. At compaction pick, after input+overlap expansion, reject the candidate if any input is
   already `being_compacted` (`AreFilesInCompaction`); else mark all inputs and register the
   compaction with its `{output_level, [smallest_key, largest_key]}`.
3. **Explicit output-range overlap check**: reject a candidate whose `[lo, hi]` overlaps any
   in-progress compaction on the **same output level** (`lo ≤ b.hi && hi ≥ b.lo`) — this is
   the piece disjoint-inputs alone misses.
4. Recompute per-level scores treating `being_compacted` tables as absent, so the next pick
   targets a free level/range; run the actual compaction lock-free; clear marks on finish.
5. L0 rule: at most one L0→L1 at a time (a dedicated in-progress set), optionally intra-L0
   over the remaining unclaimed L0 files.

This is the correct, per-file version of concurrency and composes with Slice 4.

## Slice 4 — subcompactions (parallelize one L0→L1)

Port `GenSubcompactionBoundaries` (`compaction_job.cc:533`): collect ~128 size-weighted key
anchors per input file, sort-merge, and emit N-1 boundary keys splitting the input by
uniform estimated bytes; run each `[boundary[i-1], boundary[i])` on its own worker with
hard iterator bounds and its own output files. `N = min(max_subcompactions, data-driven)`,
target range `= max(total/N, max_output_file_size)`. L0→L1 is the sweet spot (L0 spans the
keyspace → clean vertical cuts). Only needed if we later keep larger memtables.

## Sequencing

**Slice 1 first, measure, then decide.** If Slice 1 collapses the L1 tail and the crawl (as
predicted), Slices 2–4 become defense-in-depth / larger-scale hardening rather than urgent.
Each slice is independently shippable and measurable.

## Testing plan

### Slice 1 (TDD)
Unit (`lifecycle/tests/budget.rs`):
1. **Rotation is capped.** `active_rotation_bytes_from_budget` returns `MAX_ACTIVE_ROTATION_BYTES`
   for a large budget, the pool fraction for a small budget; monotonic; never 0.
2. **Pool still bounds total active memory** (the `ActiveMutable` pool limit is unchanged;
   only the per-rotation cap is new).
3. **Fraction rebalance** — `from_total_bytes` still sums to ≤ total; block-cache share rises.
Behavioral (`lifecycle/tests/durable.rs`):
4. **L0 files stay ~cap-sized.** Drive enough writes to flush several memtables; assert each
   L0 owned table's byte count ≤ ~`MAX_ACTIVE_ROTATION_BYTES` (not budget-scaled).
5. **Admission unchanged** — the byte-pressure/rotation-stall thresholds still key off the
   pool as before (assert error codes/severities unchanged).

### Slice 2
6. Input expansion stops at the byte cap at a clean boundary (no partial overlap).
7. Grandparent output-cut fires when output+L2-overlap would exceed the cap.
8. Small compactions are unaffected (cap not hit).

### Slice 3
9. `being_compacted` reject: a candidate overlapping an in-flight compaction's inputs is
   dropped; a disjoint one is admitted.
10. Output-range check: two disjoint-input candidates with overlapping output ranges on the
    same level — the second is rejected.
11. L0: only one L0→L1 concurrently; recovery oracle + fault sweep under concurrency.

### Slice 4
12. Boundary generation splits by size into N ranges; each subcompaction's keys stay in
    range; union is a valid non-overlapping level; recovery oracle holds.

Suite gates (all slices): full `cargo test -p strata-storage-next` (incl. recovery oracle +
fault sweep); `clippy --all-targets -D warnings`; `fmt --check`.

## Perf validation (control-first — probe already in the tree)

The per-compaction `STRATA_TRACE` probe (level/input/output/bytes/ms) is the gate. 10M/1000B/
48g workload A, control (`8298cde3`) vs each slice:

- **Slice 1:** L1 (L0→L1) **max-ms collapses from ~6 s toward sub-second**; L0 file bytes
  ~64 MB not 264 MB; L0 backlog stays bounded; **load throughput up, crawl-rate down** over
  n≥9 (the ledger's convoy metric). Verify reads/read-C hold or improve (bigger block cache).
- **Slices 2–4:** compaction max-ms bounded (Slice 2); `active_rewrite` rises + backlog
  drains under concurrency (Slice 3); a single L0→L1 wall-time drops ~N× (Slice 4).

Record a ledger row per slice.

## Risks & mitigations

- **Slice 1 — more flushes.** A 64 MB memtable flushes ~4× more often than 264 MB. This is
  RocksDB-normal (small units, pipelined); flush is a separate lane and drain step 1, and the
  freed budget → block cache offsets read cost. Verify flush-watermark keeps up and frozen
  backlog stays low in the perf run. Mitigation if flush becomes the bottleneck: allow more
  in-flight frozen memtables (the pool already permits it).
- **Slice 1 — smaller memtable dedups fewer updates** (zipfian hot keys), raising write-amp
  slightly. Accepted: RocksDB's decade of tuning favors small memtable + LSM over a giant
  buffer; measure write-amp in the perf run.
- **Slices 3–4 — concurrency correctness.** The S3 gate: recovery oracle + fault sweep under
  concurrency; `being_compacted` + output-range check are RocksDB's proven invariants.

## PR discipline

One slice per PR, slice code in the title (assign against the roadmap), e.g.
`perf(storage-next): cap memtable flush size independent of memory budget (RocksDB-aligned #1)`.
States change class + assurance (S3) and links a ledger row. The `STRATA_TRACE` debug probe
is reverted before each PR.
