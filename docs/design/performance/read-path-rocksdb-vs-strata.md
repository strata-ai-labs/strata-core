# Read path: RocksDB vs Strata — structural audit (2026-07-08)

Question asked: is the 59× C gap (settled 7.2–14K vs RocksDB 426K at 10M) the
price of product choices (branches, MVCC, time travel), or an architectural
lock-in? Method: full code trace of a `kv.get()` through every layer, plus an
87-sample gdb stack profile of the settled C run phase (W6 discipline).

**Verdict: no lock-in. The product-choice machinery costs ~nothing on this
path. The gap is two fixable mechanism choices (the block cache caches raw
encoded bytes; blocks are sized for scans, not points) plus a layer-boundary
copy tax. Nothing requires a contract or format change to fix — the format
already carries everything needed.**

## The measured shape (settled C, 10M × 1KB, 32G budget)

- run 11.3–14K ops/s (500K-op runs; the 100K-op baseline cells measure a
  cold-cache transient — run LENGTH is a protocol variable now recorded).
- read p50 ≈ 22µs (cache-hit path), p99 ≈ 435µs (miss → disk), mean ≈ 70–135µs.
- **Stack profile: 49% of run-phase samples blocked in `libc read` via
  `local_fs::read_range`** (cold block fetches, 64KB each); CPU side: ~7%
  crc32, ~6% memcpy, ~10% key decode, ~6% LRU/jemalloc bookkeeping.

RocksDB at the same operating point: warm hit ~1–3µs, miss = one 4–8KB block
read; C ≈ 426K ops/s single-threaded.

## What one `get()` does today (trace)

```
engine-ycsb → KvService::get
  → get_versioned → branch_record()        [catalog lookup BY STRING + record clone, per read]
  → encode_kv_key                          [key alloc #1]
  → point_read_request → storage_key       [key copy #2] + storage_space vec![1 byte] alloc
  → StorageRuntime::read_point
      → physical_key                       [key copy #3]
      → load_published_snapshot            [ArcSwap load — cheap, by design (BS2)]
      → read_point_or_tombstone
          → ordered source walk: active RwLock<BTreeMap> probe → frozen →
            L0 tables → levels (bloom-gated) → inherited layers
            [structurally sound: provable early-exit, Arc-shared candidates,
             inherited layers cost zero when unforked]
          → per table: LazyTableState::seek_prepared_point
              → index-entry search → read_data_block_frame [cache hit or 64KB pread]
              → seek_immutable_table_data_block_point
                  → decode_exact_frame                    ← ★ THE HOT-PATH TAX
                  → linear encoded-entry scan (~59 entries avg; no restart points)
      → read_row_from_storage               [key copy #4, VALUE COPY #1, 1-byte vec alloc]
  → PersistenceReadRow::from_storage        [key copy #5, VALUE COPY #2]
  → KvValue::new                            [VALUE COPY #3]
  → get(): versioned.value().clone()        [VALUE COPY #4]
```

★ `decode_table_block_frame_inner` (format/table/mod.rs) runs on EVERY seek —
including block-cache hits, because the cache stores the raw encoded frame
(`cache.insert(key, Arc::clone(&frame.bytes))`). Per seek it:
1. **CRC32-hashes the entire 64KB frame** (~2–6µs even with pclmulqdq — the
   profile's crc32 samples), and
2. **copies the entire 64KB payload to a fresh Vec** (`encoded_payload.to_vec()`
   — the profile's memcpy samples), to return one ~1KB row.

RocksDB verifies the checksum once when the block is read from disk and caches
the verified, decoded block; a hit is a sharded hash lookup + restart-point
binary search, no hash, no copy.

## Findings, ranked

| # | Finding | Class | Cost today | Fix shape |
|---|---|---|---|---|
| B1 | Block cache stores raw ENCODED frames; every seek (hit or miss) re-pays 64KB CRC + 64KB copy | **Mechanism blunder** | LANDED (`e7f0f1ea`) — measured ~nil at 10M C: the cost estimate was miss-path-contaminated; median-hit CRC+copy ≤1µs. Kept: removes wasted CPU, hardens admission, unblocks B2. | Verify once at insert; cache a verified frame (or decoded block); seeks borrow from the Arc. The roadmap's "decoded cache breaks budget accounting" risk note already anticipated this — charge decoded entries to the pool (BS4.5a seams). |
| B2 | 64KB data blocks for a point workload | **Calibration miss** | 64KB IO per miss for a 1KB row (RocksDB: 4–8KB); fewer distinct blocks per pool byte → structurally lower hit rate; 49% of C's wall is these reads | Extend the W2.1b sweep DOWN (4–16KB) for point cells; or partitioned/point-oriented block sizing. Interacts with B1: smaller blocks make per-block fixed costs (B1) relatively worse until B1 lands — sequence B1 first. |
| B3 | No restart points → linear ~59-entry encoded scan per block seek | Known (W2.3) | LANDED (`b36d0c04`) — derived entry-offset accelerator (no format change): **read p50 21.2 → 9.8µs (−54%)**, throughput +24%, interleaved A/B. | Derived per-block offsets under the Accelerator cache kind; trusted hits bisect. |
| B4 | Layer-boundary re-materialization: 4 value copies, 5 key copies, ~10 allocs, a per-read branch-catalog STRING lookup + record clone, per-read request objects | **Layering tax** | LANDED (`fbbf1f68`) — measured ~nil: the engine tax was only ~0.3-1µs mean (api_read_point probe); kept as hygiene. | Borrowed read path end-to-end: `read_point_or_tombstone_borrowed` already EXISTS (branch/read.rs:1550) and the API doesn't use it; pinnable value out (engine returns `Arc`/slice, `get()` stops cloning); cache the branch record on the service handle. |
| B5 | `KvService::get` = `get_versioned().value().clone()` | Paper cut | one 1KB copy | Move, don't clone. Folds into B4. |

## What is NOT the problem (product choices, verified cheap)

- **Branch layering / forks**: inherited-layer probes early-exit to zero work
  on an unforked database (confirmed in `select_ordered_visible_point_candidate`).
- **MVCC per-row versioning + timestamps**: bounded seeks are comparable to
  RocksDB's sequence numbers; version checks are integer compares.
- **Time travel**: the retained timeline index (W3.1) took `as_of` off the read
  path entirely; `Latest` reads never touch it.
- **Off-lock read protocol (BS2)**: snapshot load is an ArcSwap read; the
  memtable RwLock is uncontended in the C profile (zero lock samples).
- **The ordered source walk**: provable early-exit machinery, bloom-gated L0,
  Arc-shared candidates — the LSM probe SHAPE matches RocksDB's.

## Reconstruction of the gap (REVISED post-B1 measurement)

- Hot hit: ~21µs ≈ **B3 (~12–17µs: 57 linear entry-scan steps per read,
  probe-verified)** + B4 (~1–2µs) + probe walk (~2µs) + B1 (≤1µs, landed).
  The original B1 estimate was miss-path-contaminated — see the ledger row.
- Original estimate line kept for the record:
- Hot hit (original, falsified): ~22µs ≈ B1 (8–15µs) + B3 (few µs) + B4 (~1–2µs) + probe walk (~2µs).
  Post-B1/B3/B4 estimate: **~3–5µs** — RocksDB territory.
- Miss: 64KB pread + B1 tax vs RocksDB's 4–8KB pread. Post-B1/B2: miss cost
  drops ~4–8× AND miss RATE drops (more blocks per pool byte).
- With 49% of wall in misses and the rest in the hit tax, B1+B2+B3 plausibly
  carry C from ~10K into the ~50–150K band at 10M before the engine-layer tax
  (B4) becomes the next ceiling. The 290K target then depends on B4 plus cache
  sizing policy — no structural unknowns remain on the list.

## Sequencing recommendation

1. **B1 — verified-frame cache** (biggest single lever; enables honest B2 sweep).
2. **B3 — restart points (W2.3)** — independent, small format addition to block
   payloads (pre-freeze window consideration: format is M3-frozen — restart
   arrays can ride inside the existing block payload without a frame change,
   but goldens must extend).
3. **B2 — block-size sweep for point cells** (W2.1b recast, after B1 so the
   per-block fixed costs don't pollute the sweep).
4. **B4/B5 — borrowed read path through the layers** (engine + api surface
   change; `_borrowed` machinery exists storage-side).

## Protocol notes recorded

- Run LENGTH changes C by 2× (100K ops = cold transient; 500K = 11.3–14K
  steady). Baseline rows should state ops count; steady-state cells should use
  ≥500K ops or a warm-up discard window.
- perf_event is disabled on the dev box (`perf_event_paranoid=4`); the gdb
  parent-attach sampler (`c_read_sampler.sh` pattern) is the working fallback.
