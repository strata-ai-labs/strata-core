# Billion-Scale Plan

Status: **umbrella plan**. This document defines the goal, inventories every known gap, and
splits the work into milestones (BS1–BS6). Each milestone gets its own detailed
implementation + test plan before work starts (the way `rocksdb-aligned-compaction-plan.md`
served its slices). This plan **absorbs and supersedes** the M4P-L8I lock-decoupling plan
(Group D → BS2, Groups B/E → BS5) and builds on:

- `rocksdb-parity-roadmap.md` — the three root causes and their RocksDB mechanisms (RC1/RC2/RC3).
- `rocksdb-aligned-compaction-plan.md` + committed slices `8298cde3`, `f4af70ac`, `3bceb4c3`.
- The YCSB scoreboard harness (`benchmarks/src/bin/{engine_ycsb,rocksdb_ycsb}.rs`, shared
  workload generators — apples-to-apples).

## 1. What "billion scale" means

**Target:** 1 B keys at ~1 KB values (~1 TB logical data) in one durable local database, on a
memory budget far below the dataset (8–64 GB), with RocksDB-competitive throughput, bounded
startup time, and the same binary scaling down to edge budgets (512 MB) — the unified-scale
product vision.

Sizing consequences (current constants: 64 MiB output tables, level growth 10×, base ≤256 MiB):

| dimension | at 1 B × 1 KB | consequence |
|---|---|---|
| durable tables | ~16,000 × 64 MiB | table metadata must be *bounded-open* (reader cache), not all-resident |
| levels | ~5–6 non-zero levels (L5 ≈ 2.5 TB cap) | level math holds; no redesign needed |
| bloom filters @10 bits/key | ~1.25 GB total | filters must be durable + cache-charged, not rebuilt-on-open |
| per-table index | ~1 MiB × 16 K ≈ 16 GB if all open | index blocks must be evictable (block-cache-charged) |
| dataset vs RAM | 1 TB ≫ any budget | **disk-resident tables are mandatory, not an optimization** |

**The hard conclusion from the crate read:** today every durable table is fully resident and
decoded in RAM, and the dataset cannot exceed the memory budget (`bootstrap.rs:673`). Billion
scale is *architecturally unreachable* until BS4. Everything before BS4 makes the engine fast
and correct in the regime it already supports; BS4 changes the regime.

## 2. Where we stand (scoreboard, single-threaded, 1 KB values)

| | load 100 K | load 1 M | load 10 M | run C @10 M | run A @10 M | run E @10 M |
|---|---|---|---|---|---|---|
| strata (durable) | 330 K | 130 K | 90 K | 85 K | **110** | 7.2 K |
| RocksDB (default) | 760 K | 935 K | 660 K | 368 K | 272 K | 39 K |
| gap | 2.3× | 7× | 7.4× | 4.3× | **2473×** | 5.4× |

Plus: a 10 GB dataset hard-fails at an 8 GB budget; open time is O(dataset).

Root causes (full evidence in `rocksdb-parity-roadmap.md`):
**RC1** — one global mutex over reads, writes, and maintenance. **RC2** — every commit re-folds
over every SSTable 5–6× under that mutex (the load-decay). **RC3** — tables fully resident +
decoded; disk is durability-only.

## 2b. Cross-cutting product constraints (binding on every milestone)

Four product invariants that the milestones must preserve. Each milestone plan carries a
constraints subsection; violations are exit-gate failures.

**C1 — WebAssembly compatibility.** Strata must run in-browser (stratadb.org live demo):
`wasm32` with `default-features = false` (no localfs — `lib.rs:13` compile guard), memory
backend, and the **`InlineMaintenanceExecutor`** (the thread-free execution path already
exists alongside `ThreadedMaintenanceExecutor`, `api/runtime/background.rs:447/458`).
Rules: (a) no *new* unconditional `std::thread` / `std::time::Instant` on paths shared with
wasm — threads stay behind the executor abstraction or `cfg(not(target_arch = "wasm32"))`;
timing goes through the existing `MaintenanceClock` abstraction; (b) `arc-swap`/atomics are
wasm-safe (BS2 is fine); (c) **existing debt**: the committed subcompaction fan-out uses
`std::thread::scope` unconditionally (`lifecycle/rewrite_publication.rs`) — would panic on
wasm if reached; fixed in BS3.1 (gap G23); (d) standing gate: `cargo check
--target wasm32-unknown-unknown --no-default-features -p strata-storage-next` added to the
milestone gates from BS1 on.

**C2 — Cache mode is untouched.** Cache mode (no WAL/manifest/snapshot/durable objects;
in-memory tables via `open_bytes`; no background workers; throttle neutralized to 0) keeps
its current semantics and its dataset-resident memory model, including its
`would_exceed_total` budget rejection. Milestone rules: BS1 aggregates are shared
`BranchLocalState` mechanics (behavior-identical, allowed); BS2 gives cache mode the same
snapshot machinery via the shared code path (no separate read implementation); **BS3's
admission regrade + rate ramp are durable-path-scoped** (cache-mode pressure thresholds and
the neutralized throttle unchanged); BS4 keeps cache-mode tables Eager and keeps its budget
check. Every milestone runs the cache-mode suites unchanged as a gate.

**C3 — Memory profiles / one binary, many budgets.** Profiles (embedded → standard →
server; Raspberry Pi → Xeon) are realized by the proportional `from_total_bytes` pool
scaling — one code path, different `memory_budget`. Rules: no milestone may introduce a
budget-dependent code path fork; fixed caps (e.g. the 64 MiB rotation cap) must compose as
`min(pool_fraction, cap)` so small budgets keep their proportional behavior; BS4's pool
rebalance must validate at explicit tiers (**512 MB / 8 GB / 64 GB**) including block-cache
minimums; BS3's admission grades must be checked against small-budget tiers (the byte-
pressure paths, not the L0 count grades, must remain the binding constraint at embedded
sizes). BS6's edge-tier validation (512 MB) is the standing profile gate.

**C4 — Branch isolation (O(1) COW branching) must not break.** Fork = attaching Arc'd
inherited-layer references (`fork_into_empty_child` + `attach_inherited_layers`) — O(1),
copy-on-write, cross-branch references rejected. Rules: BS1 treats fork/attach as aggregate
construction sites (already enumerated); BS2 snapshots carry `inherited_layers` and the
registry updates at fork — the concurrency stress suite must include **fork-during-load,
parent+child reads, and materialization completion** as named invariants; BS4 must keep
inherited-layer reads correct under lazy readers (facts-based validation, per-key probes
with the branch-id prefix swap, inherited-layer hashing in the pruning fingerprint) and its
cold-read suite must include forked branches with active inherited layers and
mid-materialization states; fork must remain O(1) (no eager copying introduced at any
publication/aggregation point — a fork-latency test pins this).

## 3. Gap inventory

Every known gap, with its evidence and its home milestone. "Done" = landed this branch.

| # | Gap | Evidence / anchor | RocksDB mechanism | Milestone |
|---|---|---|---|---|
| G1 | Per-commit O(tables) folds ×5–6 under the lock | `bootstrap.rs:714`, `read_hooks.rs:111`, `compaction.rs:548/2562` | install-time cached aggregates on `VersionStorageInfo`; `finalized_` assert | **BS1** |
| G2 | Runtime memory total recomputed by fold, 2×/commit | `refresh_runtime_memory_total` | atomic counters (`WriteBufferManager` pattern) | **BS1** |
| G3 | Pressure re-collected per backpressure retry (convoy amplifier; 1.8 M retries measured) | `mod.rs:2850`, `:3140` | stall state recomputed on install only; write path checks O(1) flags | **BS1** |
| G4 | Global mutex serializes all reads with writes/maintenance; latest read+scan fully under lock | `background.rs:13`, `mod.rs:1242/1362` | metadata-only mutex + refcounted SuperVersion snapshot (`column_family.cc:1366`) | **BS2** |
| G5 | Versioned reads deep-clone the layout incl. per-table bloom byte-copies | `read_hooks.rs:255`, `cache.rs:477` | snapshots are refcounted, never copied | **BS2** |
| G6 | Scan path: per-key heap clones, full-row clones, whole scan under lock | `read.rs:3032-3189` | iterator over pinned snapshot; readahead | **BS2** (+BS6 readahead) |
| G7 | Update-under-load crawl: L0→L1 throughput + retry convoy (A/F @10 M = ~110 ops/s) | diagnostic: 1.8 M `l0_paced`, 0 flush-paced | compaction keeps up + gradual admission + O(1) retries | **BS3** (with BS1/BS2) |
| G8 | Compaction ~350 MB/s single L0→L1; publish dominates ~63% (durable fsyncs ~88 ms + backend-integrity byte re-read ~67 ms + SHA-256 content digest ~67 ms), merge byte-bound — per-row policy/encode overhead **refuted** (H2/H3) | BS3.2 decomposition (`storage_next_l0_compact` bin) | H1b batch fsyncs (safety-preserving); byte-validate elision = posture change (re-read is the only backend-write-corruption check, +16% measured); digest not cleanly deferrable | **BS3.3 (paused)** |
| G9 | Subcompactions land no win in the memory-bound regime (WIP, `3bceb4c3`) | Slice 4 A/B | subcompactions pay off when I/O-bound | **BS3** re-eval, **BS4** re-A/B |
| G10 | Admission thresholds tight + abrupt vs RocksDB (L0 urgent/block 8/16 vs slowdown/stop 20/36) | `compaction.rs:33-47`, `admission_ramp.rs`, config comment | grades regraded to 20/36 + C3 tier matrix (BS3.4a ✓); debt-adaptive rate ramp (`SetupDelay` port) landed **dark behind `STRATA_ADMISSION`** (BS3.4b ✓); bake `graded` after out-of-band A/B (BS3.4c) | **BS3.4a+b done (b dark); BS3.4c bake pending** |
| G11 | Tables fully resident + decoded; dataset ≤ budget; budget counts encoded (under-counts) | `read.rs:586/651`, `bootstrap.rs:673` | disk-resident blocks; block cache = the RAM bound | **BS4** |
| G12 | Open/recovery O(dataset) (decode every table) | open path | manifest replay + ≤16-file warmup (`version_builder.cc:1641`) | **BS4** |
| G13 | Block cache: O(n) recency scan under shard mutex; shards clamp to 1 | `table/cache.rs:606/623` | sharded O(1) intrusive-LRU (`lru_cache.cc:232`) | **BS4** (prereq) |
| G14 | No durable filter block — blooms rebuilt from decoded rows at open (impossible under lazy reads) | spec §17 filter frame "reserved"; `BuildOnOpen` | persisted filter block, cache-charged | **BS4** (format extension) |
| G15 | No bounded table-reader cache (all readers always open) | — | TableCache LRU keyed by file, count-bounded | **BS4** |
| G16 | Budget-ledger single global mutex (2 locks per published table) | `budget.rs:157/666` | cache-charge deltas, atomics | **BS4** |
| G17 | No group commit; WAL append under the global lock; memtable = BTreeMap behind RwLock | `wal.rs:912`, `mutable.rs:57` | write groups + dedicated WAL mutex + concurrent skiplist | **BS5** |
| G18 | No per-branch sharding (cross-branch ops serialize) | one runtime lock | per-CF SuperVersions | **BS5** |
| G19 | Same-directory publish fsync serialization (minor) | `local_fs.rs:708` | — | **BS5** (observe) |
| G20 | zstd implemented + spec'd but default-off (pointless while resident) | `table/config.rs:123`, spec §17 | on-by-default block compression | **BS6** |
| G21 | No scan readahead / MultiGet-style batched I/O (moot while resident) | — | auto-ramping readahead 8→256 KB | **BS6** |
| G22 | No ≥10 M scoreboard tier; no concurrent-writer benchmark; 1 B validation exceeds dev disk (250 GB free) | harness | — | **BS6** (validation) |
| G23 | Subcompaction fan-out uses unconditional `std::thread::scope` (wasm32 hazard, constraint C1) | `rewrite_publication.rs` (committed `3bceb4c3`) | threads behind executor abstraction / cfg gate | **BS3.1** |
| ✅ | Compaction never enqueued under load / flush-preempted (A.1+A.3, `8298cde3`) | — | — | done |
| ✅ | Budget-scaled 6 GB memtable → 2 GB L0→L1 (Slice 1, `f4af70ac`) | — | fixed 64 MiB write buffer | done |
| ✅ | Single Rewrite lane; per-branch task coalescing (Slice 3, `f4af70ac`) | — | concurrent compactions | done |
| ✅ | Apples-to-apples scoreboard harness (engine-ycsb vs rocksdb-ycsb) | — | — | done |

## 4. Milestones

Naming: `BS{n}` milestones; slices within each get `BS{n}.{k}` codes in the per-milestone
implementation plans (written when the milestone starts). Every milestone ships behind the
standing gates: full storage-next suite (recovery oracle + fault sweep + simulation), clippy
`-D warnings`, fmt, and a scoreboard run showing its target cells moved without regressing
others (n≥9 for crawl-class metrics; load throughput is the stable signal).

---

### BS1 — O(1) write path (install-time aggregates)

**Objective.** A commit's cost stops depending on database size. Gaps: G1, G2, G3.

**Scope.**
- Cached aggregates on `BranchLocalState`/layout: per-level byte sums, owned-table byte/count
  totals — updated incrementally at the existing install points (flush install, compaction
  install, rotation, table drop), all already under the lock.
- Runtime memory total → atomic counter (fetch_add/sub at the same install points); delete
  the branch-catalog fold.
- Storage pressure + compaction scores + eligible-task derivation computed **once per
  install/rotation event**, cached; `evaluate_mutating_write_admission`, post-commit
  scheduling, and the backpressure retry loop read the cached value (retries stop
  re-collecting).
- A `finalized`-style debug assert: cached aggregates equal a fresh fold (debug builds only).

**Out of scope.** Any locking change; any read-path change.

**Exit criteria.** (a) ~~Load throughput flat across 100 K→10 M records~~ **falsified — see
below**; (b) zero O(tables) work in the commit path (assert-verified); (c) workload A/F crawl
frequency reduced (convoy relief) — measured, not gated; (d) all standing gates green.

**Status: COMPLETE (BS1.1–1.3 landed `8490900e`/`e7124f6e`/`7458aae8`; BS1.4 closed by
measurement).** Correctness (b)/(d) met and mutation-proven. But the load-flatness goal (a) was
**falsified** by the BS1.3 A/B: single-threaded load is neutral (10 M ~90 K, ±10 %). The decay is
compaction / write-amp / lock-bound, not fold-bound — at 1000-row batches the folds amortize to
nothing. BS1's value is the **O(1) commit path** (prerequisite for BS2's lock decoupling and BS5's
write concurrency), not a standalone single-threaded win; the flatness/throughput targets move to
BS2 + BS3. Details + the BS1.4 retry measurement: `bs1-o1-write-path-plan.md`.

**Size: S (days).** Risk: stale-aggregate bugs — mitigated by the debug-assert oracle and the
existing suite.

---

### BS2 — Lock-free reads (ArcSwap branch snapshots)

**Objective.** Reads never take the runtime lock; the read gap closes toward RocksDB. Gaps:
G4, G5, G6 (partial). This is M4P-L8I Group D, executed with the SuperVersion semantics.

**Scope.**
- `Arc<BranchSnapshot>` bundling `{active, frozen, owned levels, inherited layers, visible
  version}`, published via `ArcSwap` at every install point (the code already names this
  design at `read.rs:658`). Installers swap under the runtime lock; readers `load_full()` —
  one atomic refcount bump, zero locks. Old snapshots die by refcount (RocksDB's
  `Version::Unref` semantics via `Arc`).
- Latest point/scan and versioned reads all run against the loaded snapshot off-lock.
- `TableReaderFilter` becomes `Arc`-shared (kills the per-snapshot bloom byte-copies).
- Memtable read access: the active `MutableTable` is already behind its own `RwLock` — reads
  take that, not the runtime lock. Verify freeze/rotate publication ordering (snapshot swap
  happens-after rotation).
- Scan-path efficiency pass: eliminate per-key heap clones and candidate-Vec churn where the
  snapshot makes borrowing safe.

**Out of scope.** Write-path locking (BS5); thread-local snapshot caching (RocksDB's
`kSVInUse` dance — plain `load_full` first; add only if profiling demands).

**Exit criteria.** (a) Run C @10 M ≥3× (85 K→≥250 K); (b) reads do not degrade while
compaction/flush runs (measured under load); (c) run B and E improve materially; (d) a
concurrent read+write correctness test (readers on old snapshots complete safely across
installs); (e) standing gates.

**Size: M (1–2 weeks).** Risks: snapshot lifetime bugs (mitigation: `Arc` semantics +
loom-style or stress tests); memory growth from long-lived snapshots pinning old tables
(mitigation: measure; same trade RocksDB makes).

---

### BS3 — Compaction throughput & graceful admission

**Objective.** Update-under-load stops crawling: the write path degrades gradually and the
compaction engine keeps up. Gaps: G7, G8, G9 (re-eval), G10.

**Scope.**
- **Profile the merge engine** (it does ~270 MB/s on resident data — pure CPU): per-row policy
  dispatch, builder append, artifact encode; fix the top offenders.
- **Adopt RocksDB admission grades**: L0 slowdown/stop ≈ 20/36 (vs urgent/block 8/16 today),
  pending-bytes soft/hard analogs, and convert the fixed 20 ms quadratic throttle toward the
  stateful multiplicative rate ramp (×0.8 / ×0.6 near-stop / ×1.25 recover, floor 16 KB/s) —
  the fix #2 scaffolding already exists.
- **Re-evaluate subcompactions** (`3bceb4c3` WIP) with correct expectations: in the resident
  regime they are memory-bound; keep gated off by default unless the profile shows headroom.
  Re-A/B after BS4 when compaction becomes I/O-bound.
- Reunion test (parallel == serial) — owed from Slice 4 regardless.

**Out of scope.** Disk-resident reads (BS4).

**Exit criteria.** (a) Workload A and F @10 M ≥50 K ops/s (from ~110) with **zero deep-crawls
in n≥9**; (b) L0 count stays below the slowdown grade at steady state; (c) sustained-load
throughput within 25 % of burst; (d) standing gates.

**Size: M.** Depends on BS1 (retry convoy) and benefits from BS2 (read/write separation).
Risk: the merge profile may implicate the format encode (CPU) — acceptable finding; informs
BS6 tuning.

---

### BS4 — Disk-resident tables (the regime change)

**Objective.** The dataset is no longer bounded by RAM: tables live on disk, reads fetch
blocks on demand, memory is bounded by caches. **This is the milestone that makes billion
scale possible.** Gaps: G11–G16 (+ unblocks G9, G20, G21).

**Scope (ordered — each step shippable).**
1. **Block cache rewrite**: sharded, byte-bounded, O(1) intrusive-LRU (per-shard mutex, never
   clamping to one shard). This becomes the read hot path; it must be RocksDB-grade first.
2. **Durable filter block**: persist the bloom filter in the table object (spec §17 reserved
   the frame — an additive, versioned format extension gated by golden vectors). Readers load
   the filter from the object; `BuildOnOpen` remains only as a fallback for old objects.
3. **Wire the lazy reader**: `BranchOwnedTable` stops force-materializing
   (`into_materialized`, `read.rs:586`); reads go footer/index/filter-resident + on-demand
   block fetch through the (fixed) block cache via `TableObjectByteSource`. Compaction reads
   flow through the same path with `fill_cache=false` semantics.
4. **Table-reader cache**: bounded count of open readers. *Amended by the BS4 detailed plan
   (`bs4-disk-resident-tables-plan.md`): deferred to the 1 B tier — at the 100 M exit,
   always-open reader metadata is only ~0.15–0.3 GB, and `BranchOwnedTable` holding the
   reader directly makes eviction structurally expensive now. G15 stays open as a 1 B-tier
   follow-up; reader metadata is budget-charged instead.*
5. **Budget remodel**: `memory_budget` bounds `memtables + block cache + reader metadata`
   (the RocksDB model). The dataset-size rejection (`bootstrap.rs:673`) is retired. The
   512 MB edge tier and the 64 GB server tier become the *same* code path with different
   cache sizes — the unified-scale vision.
6. **Open/recovery**: manifest replay + bounded warmup; no data decode at open.

**Out of scope.** Compression default (BS6), readahead (BS6), format changes beyond the
filter frame.

**Exit criteria.** (a) **100 M × 1 KB (~100 GB) loads and serves on an 8 GB budget** (the
cell that hard-fails today); (b) DB open ≤1 s at 100 M (vs O(dataset)); (c) 10 M scoreboard
cells do not regress beyond an agreed band (resident-hot data now flows through the cache —
some regression is acceptable, parity target: within 1.5× of the BS2/BS3 results at 10 M);
(d) crash-recovery byte-identity + golden vectors green across the format extension;
(e) standing gates.

**Size: L (the largest milestone; likely 3–5 slices).** Risks: read-latency regression for
fully-cached workloads (mitigate: pin-capable cache, measure); filter-format compatibility
(mitigate: additive frame + version gate); this milestone changes the perf character of
everything before it (re-baseline the scoreboard after).

---

### BS5 — Write concurrency

**Objective.** Multi-threaded write scaling: commits stop serializing behind one lock. Gaps:
G17, G18, G19. (M4P-L8I Groups B/E.)

**Scope.** Dedicated WAL lock + write-group leader batching (group commit for `Always`
durability); concurrent memtable (`crossbeam-skiplist` — already a vetted workspace dep);
runtime-lock shrink to install-only critical sections; per-branch sharding if multi-branch
workloads demand it.

**Exit criteria.** A new concurrent-writer benchmark (N writer threads) shows near-linear
scaling to ≥4 threads; single-threaded cells do not regress; standing gates.

**Size: M–L.** Sequenced after BS4 because the single-threaded scoreboard (our current
comparison basis) doesn't reward it, and BS2/BS4 shrink the lock first.

---

### BS6 — Billion-scale validation, compression, and tuning

**Objective.** Prove the target: 1 B keys served within a bounded envelope; close the
remaining efficiency levers. Gaps: G20, G21, G22.

**Scope.** zstd on by default for table blocks (re-baseline: smaller I/O, more CPU); scan
readahead (auto-ramping); level-target and cache tuning at scale; scoreboard tiers 100 M
(dev machine: ~100 GB, fits) and 1 B (either 1 B × 100 B ≈ 100 GB on dev, or full
1 B × 1 KB ≈ 1 TB on provisioned hardware — dev disk has 250 GB free, so the full-size run
needs external storage); edge-tier validation (512 MB budget).

**Exit criteria.** (a) 1 B keys loaded, served (point/scan/update), crash-recovered within
the memory envelope; (b) scoreboard within the agreed band of RocksDB across all cells and
tiers (proposal: ≤2× on every cell, ≤1.5× on load and read); (c) the same binary passes the
edge tier.

**Size: M** (mostly measurement + tuning; zstd/readahead are contained changes).

---

## 5. Sequencing and dependencies

```
BS1 (O(1) writes)  ──►  BS2 (lock-free reads)  ──►  BS3 (compaction/admission)
                                                        │
                                                        ▼
                       BS5 (write concurrency)  ◄──  BS4 (disk-resident)  ──►  BS6 (1B validation)
```

- **BS1 → BS2**: shrink what the lock does before changing who takes it.
- **BS2 → BS3**: read/write separation must exist before judging sustained-mixed-load fixes.
- **BS3 → BS4**: compaction must keep up in the easy (resident) regime before adding disk I/O
  to it; BS4 then re-baselines everything (and is where subcompactions get their honest re-test).
- **BS4 → BS5/BS6**: concurrency and validation build on the final regime.
- After every milestone: scoreboard run + a ledger row; after BS4: full re-baseline.

## 6. Validation strategy

- **Scoreboard** (standing): `engine-ycsb` vs `rocksdb-ycsb`, workloads A–F, tiers 100 K/1 M/
  10 M (+100 M from BS4, +1 B in BS6), single-threaded; BS5 adds the concurrent-writer bench.
- **Methodology** (hard-learned this branch): control-first A/B on one binary via env gates;
  n≥9 interleaved for crawl-class metrics; load throughput as the stable signal; falsify
  before celebrating; measurement probes are temporary and stripped before commit.
- **Correctness** (every slice): full storage-next suite — recovery oracle, fault sweep,
  simulation faults — plus milestone-specific gates (BS2: snapshot-lifetime stress; BS4:
  golden vectors + crash-recovery byte-identity across the filter-frame extension).

## 7. Out of scope

Distributed/replicated operation; network server mode; changes to the public L9 API surface;
non-additive durable-format changes; universal/tiered compaction styles (leveled only for V1);
Foundry/FFI concerns.

## 8. Open questions (to resolve in per-milestone plans)

1. BS2: is plain `ArcSwap::load_full` enough, or do read-heavy profiles need the thread-local
   snapshot cache (RocksDB's sentinel dance)? Decide from BS2 profiling.
2. BS4: filter frame format details (per-table full filter vs partitioned) — 1.25 GB of
   filter at 1 B keys says start full-filter-per-table (≤64 MiB tables → ~80 KB filters),
   partitioning only if tables grow.
3. BS4: block size for the durable format at scale (current blocks vs RocksDB's 4–64 KB) —
  measure before changing; the format supports per-block framing already.
4. BS6: where the full 1 TB validation runs (provisioned disk / cloud box) — dev machine
   caps at ~200 GB usable.
5. Milestone code mapping into the repo's M-numbering (`BS*` vs an `M12*` track) — decide at
   BS1 kickoff to keep PR nomenclature consistent.
