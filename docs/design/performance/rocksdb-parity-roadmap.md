# RocksDB parity roadmap: the three root causes and their proven fixes

Status: **strategy**. Companion to `rocksdb-aligned-compaction-plan.md` (the compaction slices)
and the M4P-L8I lock-decoupling plan. Grounded in (a) a full read of the storage-next crate,
(b) a YCSB matrix (strata vs RocksDB, same harness: `benchmarks/src/bin/{engine_ycsb,rocksdb_ycsb}.rs`),
and (c) a deep read of the RocksDB source for each mechanism (anchors below reference
`~/Documents/GitHub/rocksdb`).

## The scoreboard (YCSB, 1000 B values, durable Standard vs RocksDB defaults)

| | load 100 K | load 1 M | load 10 M | run C (read) 10 M | run A (update) 10 M |
|---|---|---|---|---|---|
| strata | 330 K | 130 K | 90 K | 85 K | **110** |
| RocksDB | 760 K | 935 K | 660 K | 368 K | 272 K |
| gap | 2.3× | 7× | 7.4× | 4.3× | **2473×** |

Three root causes, confirmed by code reading:

- **RC1 — one global `Mutex` over everything.** Every read, scan, commit, and maintenance
  pick/install serializes through `parking_lot::Mutex<LifecycleDurableLocalRuntime>`
  (`api/runtime/background.rs:13`). Latest reads run entirely under it; versioned reads
  deep-clone the whole layout (incl. every bloom filter) under it. The repo's own convoy
  analysis: 10 M load = ~16 s work + ~200 s mutex churn; the pre-V1 engine did it in ~37 s
  with `ArcSwap`.
- **RC2 — per-commit O(tables) bookkeeping.** Each commit re-folds over every owned table
  5–6× under the lock: `refresh_runtime_memory_total` ×2 (`durable/bootstrap.rs:714` via
  `read_hooks.rs:111`), `collect_storage_pressure_with_budget` ×2 (`lifecycle/compaction.rs:548`,
  `:2562`), `eligible_compaction_tasks` ×1. Tables grow with data ⇒ load throughput decays
  330 K→90 K while RocksDB stays flat.
- **RC3 — every durable table fully resident *and decoded* in RAM.** `BranchOwnedTable`
  force-materializes readers into `Arc<[TableRow]>` (`branch/read.rs:586`, `:651`); disk is
  durability-only; the dataset cannot exceed the memory budget (`bootstrap.rs:673`). The lazy
  path (`TableByteSource`, `LazyTableRows`, `TableObjectByteSource`, `TableBlockCache`) exists
  but is dead code (`service/table.rs:704`). **Billion-scale (~1 TB) is unreachable resident.**

## RC1 ← RocksDB: metadata-only mutex + SuperVersion lock-free reads

**What RocksDB does.** The DB mutex protects *only* metadata transitions — version installs,
memtable-list switches, compaction picking, stall recompute — and is explicitly dropped around
every I/O (compaction run `db_impl_compaction_flush.cc:4395`, MANIFEST write
`version_set.cc:6100`). Reads never touch it: a read acquires a refcounted **SuperVersion**
`{mem, imm, current}` (`column_family.h:205`) via a thread-local cache with `kSVInUse`/
`kSVObsolete` sentinels — one atomic swap on the hot path, zero locks
(`column_family.cc:1366-1394`). Installers publish a new SuperVersion under the mutex and
`Scrape` every thread-local slot to obsolete (`column_family.cc:1414-1485`); in-flight readers
keep the old snapshot alive by refcount until they finish. Writes join a lock-free CAS write
group (`write_thread.cc:226`), the leader appends the whole group to the WAL under a
*dedicated* `wal_write_mutex_` (`db_impl.h:2805`), followers insert into a lock-free
`InlineSkipList` concurrently; the DB mutex appears only on `UNLIKELY` structural transitions
(`db_impl_write.cc:1492-1575`).

**The strata port.**
1. **`ArcSwap<BranchSnapshot>` per branch** — bundle `{active, frozen, owned_levels,
   inherited_layers, visible_version}` into one refcounted snapshot published at install time.
   The code already anticipates exactly this (`branch/read.rs:658` — "becomes the
   ArcSwap-published layout so reads … take no runtime lock"); `Arc<BranchLayout>` (D.2a) is
   the first half. Plain `ArcSwap::load_full()` (one atomic refcount bump) is sufficient —
   RocksDB's thread-local sentinel dance is an optional optimization.
2. **Reads stop taking the runtime lock entirely** — latest point/scan load the snapshot and
   probe it off-lock; the versioned-read deep-clone (incl. bloom-filter byte copies —
   `TableReaderFilter` must become `Arc`-shared) disappears.
3. **Shrink the mutex to installs** — commits and maintenance keep the runtime lock, but its
   critical sections become pointer swaps + cached-aggregate updates (after RC2).
4. Later (Group B/E of M4P-L8I): dedicated WAL lock + write-group leader batching; concurrent
   memtable (`crossbeam-skiplist` is already a vetted workspace dep) — only worth it once the
   global mutex no longer dominates.

## RC2 ← RocksDB: install-time cached aggregates; O(1) write path

**What RocksDB does.** Every size-dependent quantity is computed **once per version install**
and cached on `VersionStorageInfo`: compaction scores (`ComputeCompactionScore`, called only
from `AppendVersion`, `version_set.cc:5794-5804`), per-level bytes/base sizes
(`PrepareForVersionAppend`, `:3422`), `estimated_compaction_needed_bytes_`,
`l0_delay_trigger_count_`, `files_marked_for_compaction_` — all guarded by a `finalized_`
assert. Write-stall state is recomputed only in `InstallSuperVersion`
(`column_family.cc:1437`); the write path checks the O(1) `write_controller_` flags
(`db_impl_write.cc:1545`). Memory accounting is atomic counters (`WriteBufferManager`
`ReserveMem`/`FreeMem`, `write_buffer_manager.cc:60/96`) — never a fold. A write =
lock-free group join + WAL append + memtable insert + flag checks. Nothing scales with
SSTable count.

**The strata port.**
1. **Cache the aggregates on `BranchLocalState`/`BranchLayout`** — `owned_table_byte_count`,
   per-level byte sums, table counts — maintained incrementally at the install points that
   already run under the lock (flush install, compaction install, rotation). Kill the 5–6
   per-commit folds.
2. **Compute pressure/scores once per install, not per commit** — a cached
   `LifecycleStoragePressure` (plus compaction scores / eligible-task list) refreshed on
   flush/compaction install and memtable rotation; `evaluate_mutating_write_admission` and
   post-commit scheduling read the cached value. The commit-retry loop under backpressure
   stops re-collecting O(tables) pressure per retry (the convoy amplifier).
3. **Runtime memory total → atomic counter** — `fetch_add`/`fetch_sub` at rotation, flush
   install, compaction install, table drop; `refresh_runtime_memory_total`'s branch-catalog
   fold is deleted.

This is the smallest slice, has no new concurrency, and directly flattens the 330 K→90 K
degradation curve.

## RC3 ← RocksDB: disk-resident tables, two caches, on-demand blocks

**What RocksDB does.** An open table holds *metadata only* — file handle, footer, index,
filter (`Rep`, `block_based_table_reader.h:604-789`); data blocks stay on disk. Two caches
bound RAM: the **TableCache** (LRU of open readers, count-bounded by `max_open_files`,
`table_cache.cc:176`) and the **block cache** (uncompressed blocks, byte-bounded, sharded
O(1)-LRU with per-shard mutex + intrusive list — `lru_cache.h:437`, `lru_cache.cc:232/256/323`).
A Get = filter probe → index bsearch → block-cache get → on miss, one block-sized pread +
decompress + insert (`block_based_table_reader.cc:2080/1824/1521/1575`). Eviction is a future
miss, never an error. Scans use auto-ramping readahead (8 KB→256 KB,
`block_prefetcher.cc:158`). DB open = manifest replay + ≤16-file warmup
(`version_builder.cc:1641-1710`) — milliseconds regardless of dataset size.

**The strata port.** The machinery exists and is dead:
1. **Stop force-materializing** — `BranchOwnedTable::new` keeps a lazy reader
   (`open_reader_with_diagnostics` → `TableObjectByteSource` → `LazyTableRows`) instead of
   `into_materialized()` (`branch/read.rs:586`). Index/filter stay resident; rows are decoded
   per block on demand.
2. **Fix the block cache first** — replace the O(n) recency `VecDeque` scan under a shard
   mutex (`table/cache.rs:606`, shard count clamping to 1 at `:623`) with a sharded O(1)
   intrusive-LRU. This becomes the read hot path the moment lazy readers land — it must be
   RocksDB-grade before then. (It also explains the subcompaction non-win: with tables
   resident, compaction is memory-bound, and the cache was never even reachable.)
3. **Budget = caches, not dataset** — the memory budget bounds
   `memtables + block cache + open-reader metadata` (the RocksDB model) instead of total
   data; the "commit would exceed the database memory budget" rejection at `bootstrap.rs:673`
   is retired. This is the change that makes **billion-scale** possible at all.
4. **Then zstd** — block compression is already implemented and spec'd
   (`format/table/mod.rs:436`, spec §17) but default-off; it becomes valuable exactly when
   blocks are read from disk. *(Status: steps 1–3 landed as BS4 on `v1-billion-scale-perf`;
   blocks now come from disk, so this step is live — scheduled for BS6 together with scan
   readahead and the block-size sweep. Handoff notes in `billion-scale-plan.md` §BS6;
   measured re-baseline pending per `bs4-6-exit-runbook.md`.)*

## Sequencing

| # | Slice | Size | Wins | Depends on |
|---|---|---|---|---|
| 1 | **RC2: install-time aggregates + atomic memory counter** | S | flattens load decay; shrinks every lock hold; relieves the crawl's retry convoy | — |
| 2 | **RC1: `ArcSwap` branch snapshot → lock-free reads** | M | read gap (4–6×); commits no longer blocked by reads; the pre-V1 engine proves ~7× on load | RC2 (small critical sections make the swap points clean) |
| 3 | **RC3: lazy block readers + O(1) sharded block cache + budget remodel** | L | removes the RAM ceiling (mandatory for billion-scale); real read locality; makes zstd meaningful | RC1 (reads must be off-lock before adding I/O to them) |
| 4 | Write-group/WAL lock split, concurrent memtable, per-branch sharding (M4P-L8I B/E) | M | multi-writer scaling | RC1/RC2 |

Validation: the YCSB matrix (`engine-ycsb` vs `rocksdb-ycsb`, 100 K/1 M/10 M, A–F) is the
standing scoreboard; each slice must move its target cells without regressing the others, and
the full storage-next suite (recovery oracle + fault sweep) gates every step.

## Invariants (unchanged from the layer contracts)

Durability-before-ack, crash-recovery byte-identity, frozen durable format (no format change
is needed for any of the above — RC3 reads the existing STTB blocks), and the M4P-L8I
stop-conditions apply to every slice.
