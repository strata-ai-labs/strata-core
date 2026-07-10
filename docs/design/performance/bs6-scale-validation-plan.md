# BS6 — Billion-scale validation, compression, and tuning: implementation and test plan

Status: **ready to implement after BS4 (BS5 recommended first per the umbrella)**. Milestone
BS6 of `billion-scale-plan.md` (gaps G20, G21, G22; closes the program). Change class: perf
tuning + validation; one writer-side format-legal default change (compression). Assurance:
S3 for the code slices; the milestone's core is measurement.

## Problem (recap)

Three levers only become meaningful once tables are disk-resident (BS4), plus the program's
final proof obligations:

- **G20 — compression is implemented but off.** zstd is fully wired (per-block codec byte,
  `format/table/mod.rs:436/562`; decode handles both codecs; a zstd golden fixture already
  exists — `table-data-block-zstd-frame.hex`), but the builder default is
  `TableCompression::Uncompressed` (`table/config.rs:123`). Pointless while resident;
  decisive for disk footprint and I/O once lazy.
- **G21 — no readahead.** The lazy cursor issues one `read_at` per 64 KB block; sequential
  scans and compaction inputs pay per-block round trips. RocksDB auto-ramps readahead
  (8 KB → 256 KB on sequentiality) and reads compaction inputs with large fixed readahead.
- **G22 — the target is unproven.** No ≥10 M scoreboard tier existed before this program;
  100 M lands with BS4; **1 B has never been run**. The dev machine bounds what runs
  locally: 250 GB free disk → 1 B × 100 B (~100 GB) fits; the full 1 B × 1 KB (~1 TB)
  needs provisioned hardware.
- Program closure: the final parity re-baseline vs RocksDB and the acceptance band.

## What verification established

1. **The block cache stores raw frame bytes and decodes per access**
   (`table/reader.rs:650-664`: `read_data_block_frame` → `decode_immutable_table_data_block`
   on *every* call — CRC verify + decompress + row decode happen on cache **hits** too; the
   cache stores the undecoded frame). Consequence: with zstd on, every hot read pays
   decompression; even uncompressed, every hit pays CRC + decode. **Cache granularity is a
   first-order BS6 decision**, flagged from BS4's open items.
2. **The zstd default flip is config-only** — format, decoder, and a golden fixture all
   exist; the per-block codec byte even permits *mixed* tables (some blocks compressed,
   some not), which enables RocksDB-style adaptive compression (write uncompressed when
   the ratio isn't worth it).
3. **YCSB values are LCG-generated (effectively incompressible)** — benchmarking
   compression on them alone would be dishonest; a compressible-value corpus must be added
   to the harness.
4. **Backend `read_range` is the prefetch primitive** — readahead = one large `read_at`
   spanning multiple blocks, served as block slices; pure computation + trait calls
   (wasm-safe).

## Slices

### BS6.1 — Compression default + cache granularity (measure-first)

**Changes.**
1. **Adaptive zstd for durable tables:** the builder compresses each block and keeps the
   compressed form only when it saves ≥ a threshold (e.g. 12.5 %, RocksDB's heuristic);
   otherwise writes the block uncompressed (the per-block codec byte makes this
   format-legal today, no reader change). Config default flips to adaptive-zstd for
   durable builds; cache mode untouched (no durable tables).
2. **Harness corpus:** add a `--value-profile {random|templated}` mode to both YCSB
   harnesses — `templated` produces realistic compressible values (structured filler,
   ~2–4× zstd ratio) so compression is measured on both corpora. RocksDB side gets the
   same values (apples-to-apples).
3. **The cache-granularity decision (measure, then implement the winner):**
   - *Option A — status quo:* frame cache, decode per hit (zero change; highest hit cost).
   - *Option B — decoded-row cache:* cache `Arc<[TableRow]>` per block (approximate byte
     charging); hits skip CRC/decompress/decode entirely (RocksDB caches uncompressed
     parsed blocks — this is its analog).
   - *Option C — two-tier:* small decoded cache over the frame cache.
   Measure run C/B @10 M with zstd on for A vs B; implement the winner (expected: B — the
   per-hit decode is pure overhead for hot blocks; charging accuracy is the trade).

**A/B matrix (gates the default):** {random, templated} × {load, run C, run A} × disk
footprint × open time, control = uncompressed. **Ship adaptive-zstd as default only if**:
templated corpus shows the footprint/I-O win, random corpus shows ≤ noise regression
(adaptive should write ~everything uncompressed there — the heuristic's own test), and the
granularity winner keeps hot-read cells within the BS4 band.

**Tests.** Adaptive-choice unit (compressible block → codec 1, incompressible → codec 0);
mixed-table round-trip + goldens (a mixed-codec table fixture + negative mutations);
crash sweep over compressed-table writes; eviction correctness with compressed frames
(charge = stored bytes); decoded-cache (if B) differential — identical results vs frame
cache under randomized reads + evictions.

### BS6.2 — Readahead (scans + compaction)

**Changes.**
1. **Auto-ramping scan readahead** on the lazy cursor path: a prefetch buffer on the cursor
   (not the shared reader — per-scan state); sequentiality detection (next block index ==
   previous + 1, N consecutive before engaging, RocksDB uses 2); ramp doubling from 2
   blocks (~128 KB) to a cap (~16 blocks / 1 MB — strata blocks are 64 KB vs RocksDB's
   4 KB, so the block-count ramp is shorter); ramp-down on non-sequential access. Prefetched
   frames serve subsequent `read_data_block_frame` calls from the buffer (cache insertion
   policy unchanged).
2. **Compaction/materialization inputs:** fixed large readahead (e.g. 1–2 MB) on their
   bounded cursors, composing with BS4's `fill_cache=false` (big sequential reads, zero
   cache pollution). This is also where subcompactions' I/O-bound re-test (BS4.6) gets its
   fair shake — parallel ranges each with their own readahead stream.
3. Point reads: untouched (readahead never engages below the sequentiality threshold).

**Tests.** Equivalence property: any scan/compaction result is byte-identical with
prefetch on/off (randomized bounds + evictions); ramp state-machine unit (engage, double,
cap, ramp-down); over-fetch bound test (a point-read-only workload issues zero prefetches);
I/O-count assertions via perf-trace (a sequential scan of N blocks issues ≤ N/2 + c reads
at steady ramp).

### BS6.3 — Scale tiers + the 1 B proof

**Changes (harness + validation, minimal production code).**
1. **Tier formalization** in the benchmark harness + gated tests: 10 M (per-slice,
   standing), 100 M nightly (from BS4), **1 B × 100 B** (dev-runnable: ~100 GB raw,
   ~250 GB free — the *key-count* stress tier: 1 B keys exercise blooms ~1.25 GB, ~16 K
   tables of metadata, timeline volume), **1 B × 1 KB** (the full ~1 TB tier: provisioned
   hardware — umbrella open question 4; the harness takes a data-dir/target so the same
   binary runs both).
2. **G15 revisit with real numbers:** measure always-open reader metadata at 1 B×100 B
   (16 K tables); if it breaches the tier's budget, scope the deferred index-eviction /
   reader-cache work as a follow-on slice (decision recorded either way — this is the
   checkpoint the BS4 deferral promised).
3. **Crash-recovery drill at scale:** kill −9 during 1 B load (multiple positions: mid-load,
   during heavy compaction, post-load), recover, verify open time + sampled correctness +
   recovery-oracle invariants.
4. Open-time and admission behavior measured at each tier.

**Tests.** The tiers *are* the tests: completion, sampled read correctness (point/scan/
history vs a sampled oracle), `lazy_full_materialization == 0`, open-time gates
(≤1 s @100 M from BS4; measured + banded @1 B), crash drill green.

### BS6.4 — Tuning sweeps (ledger-driven)

Measurement-only until a winner is baked; every sweep is a control-first A/B with a ledger
row. Pre-scoped dimensions:
- **Table size** (64 → 128/256 MB at scale: fewer tables, less metadata, bigger
  compactions — interacts with G15).
- **Block size** (64 KB vs 16–32 KB for point-heavy tiers — per-hit decode cost vs I/O
  count; interacts with the BS6.1 granularity choice).
- **Cache fractions** (block cache vs memtable split per tier).
- **Level targets + admission grades at scale** (BS3's open item: byte-based soft/hard
  pending-compaction analogs, re-derived from 1 B-tier compaction-debt data).
- **Subcompaction cap at scale** (post-BS6.2 readahead; the final word on gap G9).

Bake only sweep winners that hold on **both** corpora and at ≥2 tiers; everything else is
recorded as tier-specific tuning guidance in the docs.

### BS6.5 — Edge tier + wasm validation (constraints C1/C3 as deliverables)

**Changes (validation + fixes-as-found).**
1. **Profile matrix**: {512 MB, 8 GB, 64 GB} × {10 M, 100 M where it fits}: full
   correctness suite + scoreboard smoke per cell; the 512 MB tier must complete the 10 M
   workloads within its envelope (cache-bounded, post-BS4) — the Raspberry-Pi proof.
2. **Wasm deliverable (stratadb.org readiness):** the `wasm32-unknown-unknown
   --no-default-features` build gate graduates from check to **test**: a headless browser
   (or wasmtime + memory backend) smoke running cache-mode open → commit → read → scan →
   fork → read-on-fork. This is C1's end-state, not just a compile check.

**Tests.** The matrix and the smoke are the tests; any failure files a fix slice.

### BS6.6 — Program closure

1. **Final parity re-baseline**: full scoreboard (A–F × {10 M, 100 M} × both corpora ×
   single- and multi-writer from BS5.0), n≥3 per cell (n≥9 for crawl-class), vs the same
   RocksDB harness.
2. **The acceptance band becomes the formal gate** (from the umbrella proposal, ratified
   here): **≤2× RocksDB on every cell; ≤1.5× on load and read cells.** Cells outside the
   band get a root-cause note and either a fix slice or an accepted-gap entry — no silent
   misses.
3. Gap-table closure in `billion-scale-plan.md` (G1–G23 statuses), ledger finalization,
   and a closing summary doc (what was achieved, residual gaps, post-program
   recommendations — e.g. G15/index eviction, BS5.3/5.4 if still measure-gated).

## Exit gates (milestone = program exit)

1. **1 B keys loaded, served (point/scan/update), and crash-recovered within the memory
   envelope** — 1 B × 100 B on dev hardware; 1 B × 1 KB on the provisioned tier.
2. **Scoreboard within the acceptance band** (≤2× every cell; ≤1.5× load + read) on both
   corpora.
3. **The same binary passes the edge tier** (512 MB profile matrix cell) **and the wasm
   smoke** (browser cache-mode).
4. Standing gates green everywhere; goldens extended (mixed-codec tables); ledger + gap
   table + closing doc committed.

## Cross-cutting constraints (umbrella §2b)

- **C1 (wasm):** readahead and compression are pure computation + backend-trait reads —
  wasm-safe; BS6.5 upgrades the wasm gate from compile-check to a browser smoke (the
  stratadb.org deliverable).
- **C2 (cache mode):** compression and readahead are durable-path only (cache mode has no
  durable tables/objects); cache-mode suites run unchanged; the wasm smoke *is* cache
  mode, giving it end-to-end coverage.
- **C3 (profiles):** BS6.5's profile matrix is this constraint's formal proof — one binary,
  512 MB → 64 GB, proportional budgets; tier-specific tuning ships as configuration
  guidance, never as code forks.
- **C4 (branching):** the 100 M tier includes a forked-branch scenario (fork at 50 M,
  divergent writes, inherited-layer reads, one materialization) — branch isolation proven
  at scale; the wasm smoke includes fork.

## Risks

| Risk | Mitigation |
|---|---|
| zstd default regresses the (incompressible) benchmark while helping real data | adaptive per-block compression (uncompressed when ratio < threshold) + the dual-corpus A/B is the shipping gate; worst case the default stays uncompressed and adaptive-zstd ships as a profile recommendation |
| Per-hit decode cost dominates hot reads (frame-cache granularity) | the BS6.1 granularity decision is measure-first with Option B (decoded-row cache) expected; BS4's 1.5× band already prices the frame-cache status quo |
| Readahead over-fetches random workloads / wastes cache | sequentiality gate (N consecutive) + ramp-down + `fill_cache` policy unchanged; over-fetch bound test |
| 1 B × 1 KB logistics (no local 1 TB) | dev-runnable 1 B × 100 B proves key-count scale; the full tier is harness-portable to provisioned hardware (explicit open item with owner/date at BS6.3 kickoff) |
| Tuning overfits YCSB | dual corpora + ≥2-tier rule for baking defaults; tier-specific findings ship as guidance |
| G15 (reader metadata at 16 K tables) breaches small budgets at 1 B | BS6.3's explicit checkpoint: measure, then scope index-eviction as a follow-on — the BS4 deferral's promised revisit |
| Parity band missed on some cells | closure discipline: root-cause note + fix slice or accepted-gap entry; no silent misses |

## Sequencing & PR discipline

BS6.1 → BS6.2 → BS6.3 → BS6.4 → BS6.5 → BS6.6, though 6.1/6.2 are independent and 6.5 can
run any time after BS4. One PR per slice, `BS6.{n}` titles, standing gates every slice;
6.1/6.2 carry golden/fault gates; 6.3–6.6 are predominantly harness + measurement +
documentation. Depends on BS4 (the regime the levers exist for); BS5.0's write-scaling
column joins the closure scoreboard.

## Open items

- Provisioned hardware for the 1 TB tier (owner + timing at BS6.3 kickoff — umbrella §8.4).
- Adaptive-compression threshold value (start at RocksDB's, sweep in 6.4).
- Whether the decoded-row cache (if chosen) should also serve compaction inputs or stay
  read-path-only (`fill_cache=false` suggests read-only; measure).
- Block-size / table-size winners may motivate a format-default change for *new* tables
  only (per-table config is already format-legal) — decide in 6.4.
