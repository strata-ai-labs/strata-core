# C to 60–70% of RocksDB: the third read-path document

Predecessors: `read-path-rocksdb-vs-strata.md` (the structural audit: no
architectural lock-in) and the B1–B4 close-outs in the ledger. This document
re-evaluates C after the campaign (16 KiB blocks, settled protocol, trusted
reads, bisected seeks) and lays out the path from today's ~19.5K to the
60–70% line. All numbers dev-box /data2, 10M × 1KB, 500K-op settled cells.

## Target arithmetic

- RocksDB reference (as currently measured): C = 428K ops/s = 2.34µs/read.
  60–70% ⇒ **257–300K ops/s ⇒ 3.3–3.9µs mean per read.**
- Today: 19.5K = 51.3µs mean. In the two-term model
  `mean = h·H + (1−h)·M`: h ≈ 0.74, H (hit) ≈ 10.5µs, M (miss) ≈ 150–170µs.
- No single term reaches the target. Required shape: **h ≥ 0.98, H ≈ 3–3.5µs,
  M ≈ 10–20µs** → mean ≈ 3.4–3.8µs. Both terms must fall and the miss rate
  must collapse.

## Evidence (2026-07-08 night, fresh experiments)

### Finding 1 — compression-flattered reference: HYPOTHESISED, then FALSIFIED (C1)

The rocksdb-ycsb harness builds RocksDB with the lz4 feature and both
harnesses used `vec![0x42; 1000]` constant values, so the working hypothesis
was that RocksDB's dataset compressed to ~nothing and its 428K C was a
cache-resident artifact. **C1's measured matrix killed this** (10M × 1KB,
500K-op C, on-disk probe post-load):

| fill | compression | on-disk | run C | read p50 | read max |
|---|---|---|---|---|---|
| constant | default | 9.59GB | 435K | 2.16µs | 30.8µs |
| random | default | 9.59GB | 428K | 2.29µs | 34.2µs |
| random | none | 9.59GB | 434K | 2.19µs | 44.6µs |
| random | lz4 | 9.55GB | 425K | 2.18µs | 232µs |

Stock `Options::default()` in rust-rocksdb writes **uncompressed** — 9.59GB
raw regardless of fill, and all four cells sit within 2.4%. The reference is
honest and insensitive to value content.

The sharper reading: RocksDB serves its entire raw ~9.6GB dataset from the
OS page cache — read **max** ~34µs means zero disk misses across 500K reads.
Its 13s load with modest churn leaves the freshly written files cache-warm.
Strata's misses hit real disk not because the comparison was rigged but
because our own load churn evicts our tables and the block cache never fills
— Finding 2 is the whole miss story. Incompressible values are still the
right protocol (standard YCSB uses random payloads; guards future compressed
references) and are now the harness default.

### Finding 2 — the miss term is disk-bound and the cache-fill gap is the cause

Budget curve (settled C, per-miss cost from mean decomposition):

| budget | RSS post-load | miss rate | run | read p99 |
|---|---|---|---|---|
| 8G  | 6.1GB  | 30% | 18,658 | 295µs |
| 16G | 11.7GB | 31% | 16,794 | 300µs |
| 32G | 22.5GB | 25% | 18,960 | 296µs |

- Misses cost ~150µs at every budget — **real nvme reads, not page-cache
  hits**, even with ~50GB of free RAM at the 8G point. The table bytes are
  not in the page cache at run start: the load's write-amplification streams
  50–100GB through the cache, evicting the early-written (bottom-level,
  long-lived) tables, and nothing re-warms them.
- **The pool is NOT the constraint at 32G**: the block-cache pool scales to
  ~15GB (240/512 of total) > the 11GB dataset. It simply never fills: W2.4's
  publish-time warming is best-effort (`insert_if_free` skips under load
  churn — the SkippedFull counters recorded 34–382K skips), and demand
  misses insert only ~2GB during a 500K-op run. h is a **fill problem, not a
  capacity problem.**

### Finding 3 — the hit path's largest CPU tax is the allocator

Fresh 78-sample gdb profile of the settled C run (debug symbols):

| bucket | share of samples | reading |
|---|---|---|
| blocked in `libc read` (misses) | 45% | the miss term, per Finding 2 |
| jemalloc machinery + `LruSlot` drops | ~19% of wall (~35% of CPU) | **~8–12 allocations per read**: per-source seek-key encodes, `decode_entry_offsets` Vec per bisect (B3 leftover), candidate-row key+value Vecs, physical-key space String + key Vec (B4-deferred), eviction/drop churn |
| key codec (`encode_escaped`/`decode_*`) | ~11% of CPU | per-source seek-key construction on every read |
| cache ops, bloom, probe machinery | remainder | small individually |

Also observed: `LruSlot` drop samples imply eviction churn despite pool >
dataset (accelerator fair-inserts + demand inserts displacing entries), and
`statx`/`ensure_dir` frames on the miss path (filesystem metadata per block
read) worth one look.

## The plan

### C1 — fair baseline (protocol slice, no product code) — DONE

Landed: `ValueFill::Random` (unique splitmix64 payload per value) as the
default in both harnesses with `--value-fill constant` retained, rocksdb-ycsb
`--compression default|lz4|none`, and on-disk post-load probes in both bins.
Outcome: the compression hypothesis was falsified (Finding 1's matrix) — the
existing 428K reference stands as honest. The value of the slice is the
falsification itself plus permanent harness hygiene: every future run records
its effective dataset size, and the reference can never silently become
compression-flattered. Ledger row records the matrix and the re-based
protocol.

### C2 — fill the pool: h → 0.98 (the miss-rate slice) — DONE

Landed (877d30ba + 365eba9d): `CachePreheat` low-tier maintenance (dirty-flag
armed by table installs/reopen, off-lock 128 MiB chunks, fair inserts, sweep
invalidation of dead-table blocks, no-fill recovery walks). Measured medians:
**C 18.8K → 56.9K (x3.0)**, miss 24% → 5.5-6.1%, read p99 300 → 210us, ON
cells stable to ±3%; A 29.4K / B 53.4K (both bests). Exit gate borderline:
the residual ~6% miss floor is structural (settle-300 converges to the same
rate) and its anatomy moves onto C3's critical path — at h=0.94, C3's H→3.5us
alone lands ~95K, so closing the floor is what buys 260-300K.

### C3 — the allocation-free hot read: H → 3–3.5µs — DONE (floor documented)

Landed in two commits (d45eb2af + d6fde965, then 96c40909 + a350f464 +
c497cc95); full numbers in the ledger's C3a/C3b rows.

- **C3a (coverage)**: the ~6% miss floor was TWO probe-found mechanisms, not
  the trigger-eating theory alone — dead-table blocks pinning the pool
  15/15GB (capped quarantine sweep; fixed by `retain_tables` purges at pass
  start/completion) and preheat self-throttling on pressure its own fill
  raised (fixed by excluding cache bytes from the preheat pressure gate).
  Plus the rearm/cursor/paused state machine so mid-pass installs survive
  to a follow-up pass. Result: **miss = 0/500K**, evict = 0, cache = exact
  live set, C 56.9K → 103.0K, read p99 210µs → ~14µs, ms-tails gone.
- **C3b (strip)**: in-place accelerator probe, version-only visited-entry
  parse + byte-compare row match, single escape-encode per read, no
  TableRow re-encode, interned spaces, seek-key capacity reserve.
  C 103.0K → **118.4K**, read p50 → **8.37µs**, bytes/op 16.9K → ~11-12K.
- **Exit gate verdict**: miss ≤1% PASSED; p50 8.37µs ≫ 3.5µs — the 214-sample
  re-profile says the residual is **probe-fundamental**: index bisect +
  key memcmp ≈26%, key codec ~8%, bloom ~3%, allocator now ~long-tail.
  These live in the index/format layer; no further allocation strip or
  cache work moves them. Per this plan's own stop rule, C3 closes here and
  the target moves to C4 with a concrete recommendation.

### C4 — the format-layer decision (now on the critical path)

C2+C3 delivered h → 1.0 and H ≈ 8.4µs; the 260–300K line needs H ≈ 3.3–3.9µs,
i.e. another ~2.4x on the hit path, and the profile says where it lives:

1. **Index bisect + memcmp (~26%)** — every read runs `binary_search_by`
   over full escaped internal keys at three levels (table pick, block pick,
   entry pick). The format answer: prefix-truncated index entries or a
   layout-aware index (fixed-stride or interpolation-friendly), possibly
   fused with the entry-offset accelerator. This is a durable-format change
   → must land before the M3 format freeze or ride a format version bump.
2. **mmap'd table reads** — page-cache-native hits kill the per-hit
   read syscall + copy and shrink the block-cache's job to decoded forms;
   interacts with the trusted-read admission model. Bigger design change;
   evaluate against (1) — they compose but (1) is smaller and profile-first.
3. io_uring / readahead batching — now only relevant to cold/miss paths
   (settled C no longer misses); keep for the general-read story, not C.
4. Compression (LZ4 blocks + cache honesty) — post-V1 flavor, unchanged.
- Concurrency protocol note: the 60–70% target is defined single-threaded
  against RocksDB single-threaded, matching the standing scoreboard.
- Standing math: 118.4K = 8.45µs mean at h≈1.0. The two-term model is dead
  (no miss term); C is now a pure hit-path CPU problem — exactly the regime
  where format-layer work pays linearly.

## Risks / notes

- The 2.9s max draw seen during profiling is the A/C shape lottery (W1.3) —
  orthogonal, but it pollutes p99.9+ gates; keep gates on p50/p99 and medians.
- Preheat must be budget- and IO-polite (BS4.4g no-fill lessons; don't evict
  demand-cached blocks; don't compete with foreground IO — idle-gated).
- The allocator findings suggest wider wins (writes allocate too); keep the
  scope to the read path per this campaign, record the rest.
- 238GB of stale engine-ycsb tempdirs had accumulated under
  /data2/strata-bench/ycsb10m/.benchmark (leftovers from killed runs —
  tempfile only cleans on normal exit). Cleaned during C1. Watch for
  re-accumulation after any killed bench run.
