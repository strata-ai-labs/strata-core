# Billion-Scale Roadmap v2 — Durable Performance to ~70% of RocksDB

Status: **draft for review** (2026-07-07). Supersedes the remaining open items of
`billion-scale-plan.md` (v1) as the organizing plan for durable-mode performance.
Change class: planning document. Owner: storage-next.

## Goal

Strata durable (Standard policy) at **67–70% of RocksDB-default throughput** on the
standing three-way benchmark (YCSB A/B/C, 10M × 1KB, zipfian, identical harness), with
**bounded write tails** (no multi-second stalls) and **honest memory** (RSS tracks the
declared budget). The last 25–30% is accepted as long-tail tuning (RocksDB has a decade
of it) and explicitly out of scope. Cache mode is already 2.5–3.8× ahead of RocksDB on
run phases and is not a target of this plan.

The comparison is apples-to-apples on durability: RocksDB-default writes its WAL
buffered (`sync=false`), which is Strata Standard's durability class. RocksDB-default
reads ride the OS page cache (8MB own block cache); its numbers are the *storage-engine
floor* on this hardware, not a tuned ceiling.

## Where we are (evidence base, 2026-07-07, dev box, /data2 nvme1)

| 10M × 1KB | Strata durable | RocksDB-default | gap | target (≈70%) |
|---|---|---|---|---|
| Load (batch 1000) | 82–96K rows/s | 1.02M rows/s | ~11× | ≥700K |
| A (50r/50u) | 0.5–2.0K ops/s | 334K | 170–680× | ≥230K |
| B (95r/5u) | 4.3K | 404K | ~94× | ≥280K |
| C (read-only) | 3.7K | 425K | ~115× | ≥290K |
| C read p50 / p99 | 59µs / 3.3ms | 2.8µs / 4.4µs | 21× / 750× | ≤6µs / ≤20µs |
| A update max | 0.4–48s (run lottery) | 217µs | — | ≤50ms |

Contrast at 100K records (small scale, low debt): durable A 13K / B 605K / D 502K —
B and D within 1.5–1.7× of CACHE mode. **The 100× gap is a scale phenomenon, not an
engine-architecture phenomenon.** Cache mode at 10M (258K/1.05M/1.60M) proves the
engine layers above storage are sound; every missing order of magnitude lives in
storage-next's behavior when the dataset exceeds memtables and compaction debt is real.

## The gap model — four terms, in causal order

**T1 — Compaction throughput and shape (the keystone).** Ingest at 90MB/s × write-amp
needs ~0.5–1GB/s of sustained compaction; we run serialized, unbounded passes (~1µs/row
single lane; one L0→L1 pass rewrites GBs, ~50s). Consequences cascade into every other
term: debt piles up → graded pacing brakes writers toward the near-stop floor (the A
collapse and the 0.4–48s run lottery on the L0 wall); L0/Ln table counts stay high →
read probes fan out and tails blow up (the C p99 3.3ms); tables-at-close stay high →
reopen floor. v1 (BS1–BS5) never targeted compaction throughput — it fixed concurrency,
lock hygiene, GC wiring, and admission semantics around it.

**T2 — Read path at scale.** p50 59µs vs page-cache-warm ~3µs: multi-source probe
fan-out (active + frozen + L0 count + level per level), block fetch through a cold/
mis-utilized block cache (raw-bytes cache, decode on every fetch; nothing warms it at
load; the OS page cache is evicted by our own 10× longer write phase), no readahead for
scans (E: 8.8× even at small scale). BS6 reserved this ground; it starts here with
attribution rather than assumptions.

**T3 — Per-commit write overhead.** Standard single-put commit ≈ 28µs (BS5.3
decomposition: apply 7.4, WAL append 3.5, admit 2.3, stage 1.7, plus clones, publish,
post-commit) versus RocksDB's ~3µs buffered write. Two structural multipliers of ours:
every commit carries **2 timeline rows** (a 3× row count on single-put commits), and the
group-commit machinery's fixed costs don't amortize for a solo writer. Ceiling today
~35K/s; target ~150–200K/s.

**T4 — Memory honesty.** The both-mode bench OOM-killed at 60.4GB anon RSS under a
declared 32g budget (jemalloc retention + unaccounted working sets: WAL encode buffers,
compaction merge buffers, reader metadata churn). The budget is the product's
Pi-Zero-to-server scaling contract; RSS must track it or every deployment story breaks.

## Workstreams

### W1 — Compaction engine (T1) — the majority of the gap

- **W1.1 Bounded incremental L0→L1 passes.** Cap per-pass input bytes; consume L0
  oldest-first prefixes so partial consumption preserves recency ordering; publish uses
  the existing candidate revalidation. Exit: L0-blocking relief ≤ 2s at 10M; workload A
  max ≤ 500ms across 5 consecutive runs (kills the run lottery).
- **W1.2 Parallel compaction lanes + subcompactions.** Key-range-partitioned
  subcompactions within a pass; concurrent non-overlapping level pairs across lanes
  (the per-branch guard structure already permits it; the off-lock Build machinery
  already runs builds without the runtime lock). Exit: sustained compaction throughput
  ≥ 600MB/s on the dev box; debt stabilizes (not merely grows slower) under a sustained
  100MB/s ingest.
- **W1.3 Leveled-shape targets at scale.** Re-derive level targets/growth for ≥10GB
  datasets (256MiB base × 10 growth is v1-era); verify write-amp ≤ ~12 measured; L0+L1
  table-count steady-state bounds that keep read fan-out ≤ ~8 sources.
- **W1.4 Pacing re-calibration on top.** With debt controllable, re-tune the graded
  ramp floor/knee so steady-state ingest ≈ compaction capacity (writer paces at the
  sustainable rate instead of oscillating between full speed and near-stop).
  Exit: load-seq ≥ 400K rows/s at 10M (stretch 700K with W3's WAL batching).

### W2 — Read path at scale (T2)

- **W2.1 Attribution first.** gdb/stack sampling of the 10M C run (the counters lied
  twice this cycle; stacks decide): I/O wait vs probe CPU vs decode vs cache misses.
- **W2.2 Block cache that works.** Decoded-row (or decoded-block) caching with budget
  accounting (the raw-bytes cache decodes on every fetch — measured as the replay
  elephant, same seam serves reads); admission of hot blocks during compaction writes
  (RocksDB `fill_cache` analog); size the pool per the profile (durable: block cache is
  ~47% of budget — make it earn that).
- **W2.3 Probe fan-out reduction.** Rides W1.3's shape bounds; per-source filter checks
  on every point path (verify bloom coverage end-to-end — the probe work found paths
  that skip filters); single-branch fast path.
- **W2.4 Scan readahead (BS6 item).** Auto-ramping readahead 8→256KB; fixes E and
  scan-range cells.
- Exit: C ≥ 290K ops/s warm (p50 ≤ 6µs, p99 ≤ 20µs); B ≥ 280K; E within 3× of RocksDB.

### W3 — Per-commit write overhead (T3)

- **W3.1 Timeline-row cost.** Single-put commits currently write 3 rows; either encode
  timeline facts into the commit record (no extra memtable rows) or batch/elide with
  exactness preserved for time-travel. This alone is ~2× on single-put workloads.
- **W3.2 Solo-writer fast path.** One uncontended path from API to WAL append + apply:
  skip group formation bookkeeping, cache admission verdicts (O(1) re-check), reuse
  encode buffers; target ≤ 8µs solo commit.
- **W3.3 Standard WAL write coalescing** (recorded v1 lever): buffered appends with
  group flush for concurrent writers.
- Exit: single-writer Standard ≥ 150K single-put commits/s at low debt; A ≥ 230K with
  W1+W2 in place (A is 50% reads — it needs both).

### W4 — Memory honesty (T4)

- **W4.1 RSS attribution** at 10M sustained load: accounted pools vs RSS; jemalloc
  decay/retention tuning (`background_threads`, decay ms); unaccounted structures
  charged or bounded (WAL encode buffers, merge working sets, snapshot clones).
- **W4.2 Budget contract test**: a standing gate that RSS ≤ declared budget × 1.25 at
  steady state on the 10M cell. Exit: three-way runs per-mode within budget; the
  both-mode OOM scenario impossible at spec'd headroom.

### W5 — Reopen & close hygiene (mostly landed; finish line)

- Replay probe landed (365→52µs/row). Remaining: checkpoint cadence at close (the
  0–1.5M-row tail lottery — tie checkpoint triggers to WAL bytes with a close-time
  flush-watermark push), and the O(tables) open floor (rides W1.3's table-count bounds).
- Exit: reopen at 10M ≤ 5s cold after a clean close; ≤ 30s after crash at max tail.

### W6 — Instrument & gates (continuous)

- The three-way (per-mode processes) is the standing scoreboard; every W-slice lands
  with its cell's before/after at 10M.
- Saturation cells (4T per-writer; sustained-load soak ≥ 10 min) run on every
  admission/pacing/compaction change — they caught 5 of this cycle's 7 defects.
- Stack-sampling profile REQUIRED for any attribution driving a slice design (counters
  misled twice this cycle).
- RSS tracked on every bench row.

## Sequencing

```text
M-A (compaction core):   W1.1 → W1.2 → W1.3 → W1.4     (largest, first, gates all)
M-B (read path):         W2.1 → W2.2/W2.3 (parallel) → W2.4   (starts after W1.2)
M-C (write overhead):    W3.1 → W3.2 → W3.3            (independent; can interleave)
M-D (memory):            W4.1 → W4.2                    (parallel with M-B)
M-E (reopen finish):     W5                             (small; after M-A)
```

Milestone exits are measured, not calendared. Rough shape: M-A alone should take
durable A from ~1K to ≥ 30K (tails bounded, pacing sane) and C from 3.7K to ~30–60K
(fan-out + debt tails); M-B and M-C together carry the rest of the way to the 70% line.

## Non-goals (this plan)

- Beating RocksDB-default (the 67–70% line is the bar; parity is tuning-era work).
- Cache-mode performance (already ahead; only W4 touches it).
- `Always`-policy throughput (fsync-bound physics; group amortization landed in v1).
- SkipMap/concurrent memtable (stays parked: T3's fast path attacks fixed overhead
  first; revisit only if multi-writer scaling re-emerges as the bottleneck after M-C).
- Compression, tiered storage, 100M+ tiers (follow after the 10M line is held; the
  same workstreams re-run at 100M as v2.1).

## Risks

| Risk | Mitigation |
|---|---|
| W1.1 partial L0 consumption breaks recency/version invariants | design doc + differential recovery oracle + fault sweeps before merge-path changes; rows are (key,version)-keyed — correctness argument documented per slice |
| Parallel lanes reintroduce lock contention (BS5 territory) | builds already off-lock; lanes coordinate via the existing rewrite-conflict check; saturation cells gate every slice |
| Decoded cache breaks budget accounting (BS4.5a seams) | charge decoded entries to the block-cache pool; W4.2's RSS gate backstops |
| Timeline elision breaks time-travel semantics | engine-level product pathway tests are the gate (`product_pathways`); exactness is non-negotiable, only representation changes |
| Attribution wrong again | W6: stacks before slices, control-first A/B, medians of 3+, per-mode processes |

## Standing references

- v1 plan: `billion-scale-plan.md` (BS1–BS5 landed; BS6 read items absorbed into W2).
- Ledger: `billion-scale-ledger.md` (all evidence rows cited above, 2026-07-07).
- Bake-offs already decided: graded admission default (BS3.4c), jemalloc pinned in the
  bench harness, per-mode bench processes.
- **RocksDB source for reference: `~/Documents/GitHub/rocksdb`** (plus the vendored copy
  under the bench's `librocksdb-sys`). Primary study targets per workstream —
  W1: `db/compaction/compaction_picker_level.cc` (pass selection/trimming),
  `db/compaction/compaction_job.cc` + `subcompaction_state.*` (key-range subcompactions,
  boundary picking), `db/column_family.cc` (`SetupDelay`, already ported);
  W2: `table/block_based/block_based_table_reader.cc` (filter/index partitioning,
  readahead ramp in `block_prefetcher.cc`), `cache/lru_cache.cc` (charged decoded
  entries via `CacheEntryRole`);
  W3: `db/write_thread.cc` (solo-writer fast path shape), `db/db_impl/db_impl_write.cc`
  (WAL buffered coalescing / group follower protocol).
