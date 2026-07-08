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

### Finding 1 — the RocksDB reference is compression-flattered (re-baseline required)

The rocksdb-ycsb harness builds RocksDB **with LZ4** and never disables
compression, while both harnesses use `vec![0x42; 1000]` values — constant
bytes that compress ~perfectly. RocksDB's "10GB" dataset is a few hundred MB
on disk at most; its 1M rows/s load and 428K C are measurements of a
cache-resident, decompress-cheap workload that our raw 11GB never gets to
play. (Our numbers are unaffected — Strata does not compress.)

**Consequence: every gap ratio in the ledger overstates the true gap.** The
fix is C1 below: incompressible (seeded-random) values in both harnesses, and
a compression-off RocksDB variant as a second reference point. Standard YCSB
uses random field payloads — constant bytes were our harness's shortcut.

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

### C1 — fair baseline (protocol slice, no product code)

Seeded-incompressible values in engine-ycsb and rocksdb-ycsb (shared
generator); re-run the three-way; add a compression-off RocksDB point.
Re-baseline the ledger reference row. Exit: the honest reference and gap
table everything below is measured against. Expectation: RocksDB's C drops
(its dataset becomes 10GB real; on this 61GB box it likely stays largely
page-cache-resident — but load and write-amp costs become honest), Strata
unchanged.

### C2 — fill the pool: h → 0.98 (the miss-rate slice)

The pool already fits the dataset at 10M/32G; make it FULL by the time the
run starts:
- A **cache preheat maintenance task**: after flush/compaction quiesce (the
  settle window; production analog = background idle), sweep live tables and
  admit their data blocks through the existing warm path until the pool
  reaches target occupancy. Bounded, best-effort, budget-charged; reuses
  `warm_data_blocks_from_encoded`'s verified-insert machinery reading from
  the table source instead of the encode buffer.
- Fix warm-insert lossiness where cheap (retry-once on SkippedFull after
  eviction pressure clears; W2.4b heat-aware admission when pool < dataset).
- Exit gate: settled C at 10M/32G shows miss rate ≤ 2–5% and C ≥ 60–80K
  (mean ≈ H). This gate isolates h from H.

### C3 — the allocation-free hot read: H → 3–3.5µs

Profile-ranked, in order:
1. **Borrowed offsets probe**: stop materializing `Vec<u32>` per bisect —
   probe the cached accelerator bytes directly (aligned LE reads), B3
   leftover.
2. **Seek-key reuse**: one seek-key/physical-key-bytes construction per READ
   (not per source probed), stack/small-vec backed; kill the per-read space
   String and 1-byte space Vec (the B4-deferred items, now profile-justified).
3. **Candidate decode into the response**: the block-hit row decode already
   copies key+value once — ensure it is exactly once end-to-end (audit the
   post-B4 path for stragglers) and consider a small-vec key.
4. **Eviction churn**: with C2's full pool, demand inserts stop evicting;
   verify LruSlot drop samples disappear (else cap accelerator fair-inserts).
5. Re-profile; iterate until H ≤ 3.5µs (settled C p50 gate) or the remaining
   buckets are probe-fundamental (memtable RwLock, bloom, binary searches).
- Exit gate: read p50 ≤ 3.5µs on settled C; combined with C2, C ≥ 230–290K.

### C4 — contingencies (only if C2+C3 undershoot)

- **mmap'd table reads** (page-cache-native hits, no per-hit syscall/copy on
  the miss residue) — the classic embedded answer; larger design change,
  interacts with the trusted-read admission model.
- io_uring / readahead batching for the miss residue.
- Compression (LZ4 blocks + our own cache honesty) — shrinks the effective
  dataset like RocksDB's; big scope, post-V1 flavor.
- Concurrency protocol note: the 60–70% target is defined single-threaded
  against RocksDB single-threaded, matching the standing scoreboard.

## Risks / notes

- The 2.9s max draw seen during profiling is the A/C shape lottery (W1.3) —
  orthogonal, but it pollutes p99.9+ gates; keep gates on p50/p99 and medians.
- Preheat must be budget- and IO-polite (BS4.4g no-fill lessons; don't evict
  demand-cached blocks; don't compete with foreground IO — idle-gated).
- The allocator findings suggest wider wins (writes allocate too); keep the
  scope to the read path per this campaign, record the rest.
- 238GB of stale engine-ycsb tempdirs accumulated under
  /data2/strata-bench/ycsb10m/.benchmark — clean before the C1 re-baseline
  so disk state is comparable.
