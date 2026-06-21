# Durable single-commit write cliff — root cause and fix plan

Status: fix #1 (size-driven flush) implemented; fixes #2 (lock-free maintenance
install) and #3 (throttle-not-reject backpressure) still planned. See
[Planned fix](#planned-fix) for status per step.

Layers touched: [L6 branch LSM runtime](./l6-branch-isolated-lsm-runtime.md),
[L7 commit runtime](./l7-commit-runtime.md),
[L8 lifecycle / recovery / maintenance](./l8-lifecycle-recovery-maintenance.md).

## Summary

Durable writes issued one row per commit (the natural YCSB / autocommit shape)
collapse on storage-next: throughput falls from ~900k ops/s to tens of ops/s as
the workload proceeds, and at intermediate scales writes are sometimes
**hard-rejected** with a `LevelZeroTableBacklog` blocking-admission error. The
same data loaded in batches (1000 rows/commit) runs at full speed.

The collapse has **two independent root causes**, both of which the pre-V1
engine (`crates/storage`) and RocksDB — its design model — avoid by design (see
[Reference designs](#reference-designs-how-this-is-supposed-to-work)):

1. **A commit-count checkpoint trigger.** The WAL-growth policy forces a
   checkpoint + memtable flush every `max_commits_since_checkpoint = 1024`
   commits, regardless of how little data those commits carried. With one row
   per commit this fires every 1024 rows, manufacturing a stream of tiny L0
   tables and the compaction churn that follows.
2. **A single global runtime mutex.** All background flush/compaction runs under
   the same `ParkingMutex` that every foreground commit must take, so commits
   serialize behind maintenance I/O. Once maintenance is active, per-commit
   latency is dominated by lock-wait.

Batching hides both: 1000× fewer commits never reach the 1024-commit trigger,
and the few commits that do barely contend on the lock. Concurrency (multiple
writer threads) would also amortize the lock, which is how YCSB normally drives
durable stores — but our single-threaded loaders expose the cliff directly.

## Symptoms

- **Progressive slowdown.** Single-row durable `load-seq` per-commit cost climbs
  from ~100 µs to ~17 ms as the run proceeds.
- **Throughput collapse.** ~9.7k ops/s at 1k commits → 58 ops/s at 100k commits.
- **Non-deterministic write rejection.** At intermediate scales (observed at
  ~4k commits) the load aborts with
  `commit rejected by Blocking storage pressure from LevelZeroTableBacklog`.
  Whether it slows down or hard-rejects depends on the race between L0 creation
  and background compaction, so it is timing-dependent.
- **Engine surface inherits it.** Through engine-next, YCSB workload A (50%
  update) on durable mode degrades from 124 µs/update at 5k ops to 12.8 ms/update
  at 100k ops (141 ops/s overall). Read-only workload C is identical to cache
  (~990k ops/s) — reads are unaffected; the cliff is write-path only.

## Reproduction

Storage layer (no engine):

```bash
# cliffs / hard-rejects
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- \
  --scales 100000 --engines standard --workloads load-seq --batch-size 1
# full speed (batched)
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- \
  --scales 100000 --engines standard --workloads load-seq --batch-size 1000
```

Engine surface:

```bash
cargo run --release --manifest-path benchmarks/Cargo.toml --bin engine-ycsb -- \
  --workload a --mode durable --records 100000 --ops 100000
```

## Root cause

### 1. The commit-count checkpoint trigger

`crates/storage-next/src/lifecycle/config.rs`:

```rust
const DEFAULT_WAL_GROWTH_MAX_BYTES: u64 = 256 * 1024 * 1024;   // 256 MiB
const DEFAULT_WAL_GROWTH_MAX_SEGMENTS: usize = 64;
const DEFAULT_WAL_GROWTH_MAX_COMMITS: u64 = 1024;
```

The WAL-growth policy is evaluated on every commit and fires when **any** of the
three thresholds is exceeded (`lifecycle/wal_growth.rs::trigger_for`). The byte
and segment bounds are data-aware; the commit-count bound is not. It exists to
cap recovery replay *record count*, but it is blind to commit *size*. With 64 B
values, the byte bound (256 MiB) and segment bound are far away, so the
**1024-commit bound is the only trigger that fires** — every 1024 rows.

When it fires, `evaluate_wal_growth_policy` enqueues a flush + checkpoint +
flush-watermark + WAL-truncation cycle
(`lifecycle/durable/maintenance.rs`). The checkpoint publishes "a bounded delta
(active + frozen rows)" as a durable table
(`lifecycle/checkpoint.rs`), i.e. it manufactures an L0 table.

Evidence — single-row durable `load-seq`, default 512 MiB budget, 64 B values:

| commits | throughput | checkpoint_executions | background_maintenance_tasks |
|--:|--:|--:|--:|
| 1,000 | 9,773 ops/s | **0** | **0** |
| 2,000 | 5,398 ops/s | 2 | 8 |
| 100,000 | 58 ops/s | **103** | **13,981** |

The cliff onset is exactly at ~1024 commits, and at 100k there are ~103
checkpoints (≈ 100000 / 1024).

### 2. Checkpoint → flush → L0 → compaction churn

Each checkpoint leaves a small table. Once ≥ 4 L0 tables exist
(`LEVEL_ZERO_COMPACTION_THRESHOLD = 4` in `lifecycle/compaction.rs`), every
subsequent commit's post-commit pressure check (`collect_storage_pressure` →
`storage_pressure_decision`) suggests a flush/compaction, and the per-commit
re-suggestion is **load-bearing**: it is the feedback loop that keeps compaction
fed (one task in flight, re-fed each commit). Suppressing it starves compaction
and drives L0 past the blocking threshold
(`LEVEL_ZERO_BLOCKING_COMPACTION_THRESHOLD = 16`), which is the source of the
non-deterministic `LevelZeroTableBacklog` write rejection.

### 3. The single global runtime mutex serializes commits with maintenance

`crates/storage-next/src/api/runtime/background.rs::RuntimeSlot::lock()` is the
single `ParkingMutex` around the entire runtime; it records
`foreground_wait_background_lock`. Background flush/compaction holds this mutex
while it runs, so every foreground commit blocks behind maintenance.

Per-commit decomposition, single-row durable `load-seq` (scale 1k → 5k):

| per-commit | 1k | 5k |
|---|--:|--:|
| `commit_call` | 103 µs | 258 µs (growing) |
| `wal_append` | 20 µs | 19 µs (flat — *not* the cause) |
| `foreground_wait_background_lock` | 0.1 ms total | **393 ms total** |

The growing cost is **not** WAL/fsync (durable mode is `Standard`, which defers
fsync — only `Always` syncs per commit). It is lock-wait behind background
maintenance plus the per-commit pressure bookkeeping.

### Two failure modes

- **Slowdown** when background compaction keeps up: commits merely stall on the
  lock, and per-commit cost climbs to ~17 ms.
- **Hard rejection** when L0 creation outruns compaction: L0 crosses 16 and
  admission rejects writes. At slow ingest (e.g. the already-degraded 100k run)
  compaction keeps up and the run completes slowly; at faster intermediate
  ingest it can spike L0 and reject. Hence the non-determinism.

## Why batching / concurrency hide it

- **Batching:** 1000 rows/commit means the 1024-commit trigger fires every
  ~1,024,000 rows instead of every 1024 — effectively never at these scales — so
  there is no checkpoint churn, and there are 1000× fewer commits to contend on
  the lock. Measured: batched durable `load-seq` at 100k = **893,503 ops/s** vs
  **58 ops/s** single-row (~15,000×).
- **Concurrency:** multiple writer threads would amortize the lock via group
  commit. Our loaders are single-threaded, which is the honest worst case and
  what surfaces the cliff. Canonical YCSB drives durable stores with many client
  threads for exactly this reason.

## Reference designs: how this is supposed to work

Both the pre-V1 segmented engine (`crates/storage`) and RocksDB — the LSM the
pre-V1 engine was modeled on — handle this identical workload without a cliff.
They diverge from storage-next on exactly the two axes above, and RocksDB makes a
third divergence explicit: overload is handled by *throttling* writers, never by
rejecting a write.

| Axis | RocksDB (reference) | pre-V1 `crates/storage` | storage-next |
|---|---|---|---|
| Flush trigger | **Size** — `write_buffer_size` (64 MiB) + `max_write_buffer_number` (2); WAL size → CF flush. No commit-count notion. | **Size** — `maybe_rotate_branch` when `active.approx_bytes() >= write_buffer_size`. No commit-count notion. | **Commit-count** (`max_commits_since_checkpoint = 1024`) **plus** size. |
| WAL bound | `max_total_wal_size` → flush oldest CF | flush watermark (data-driven) | commit-count / byte WAL-growth triggers |
| Lock during flush/compaction I/O | **Released** — `Unlock` → build SST / merge → `Lock` → install | **Released** — build SST outside lock → atomic `ArcSwap` install | **Held** — global `ParkingMutex` spans the I/O |
| Install | `VersionSet::LogAndApply` under mutex | atomic `ArcSwap` swap | under the held mutex |
| Overload handling | **Throttle**: `WriteController` *sleeps* the writer; hard-stop *waits*. **Never an error.** | write-stall: pause *rotation* when frozen memtables pile up | **Hard-rejects** the commit (`LevelZeroTableBacklog`) |
| L0 slowdown / stop | 20 / 36 files; pending-bytes 64 GiB / 256 GiB | stop-writes by frozen count | **block at 16 L0 tables**, no slowdown tier |
| Group commit | Yes — leader writes many writers' WAL in one fsync | per-branch commit lock | single global commit path |

### Pre-V1 engine (`crates/storage`)

- `segmented/mod.rs` — `maybe_rotate_branch` (size-driven rotation + write-stall),
  `flush_oldest_frozen` (build-outside-lock then atomic install,
  "Build segment to disk (no locks held — I/O-heavy)"),
  `version: ArcSwap<SegmentVersion>`, `branches: DashMap<BranchId, BranchState>`.
- `runtime_config.rs` — size-based defaults (`DEFAULT_TARGET_FILE_SIZE = 64 MiB`,
  `DEFAULT_LEVEL_BASE_BYTES = 256 MiB`); no commit-count flush/checkpoint constant.
- `txn/manager.rs` — `commit_locks: DashMap<BranchId, ...>`, a brief per-branch
  commit lock, not a global runtime lock.

### RocksDB (`/home/anibjoshi/Documents/GitHub/rocksdb`)

The authoritative LSM. Two patterns matter most, both verified directly in the
source:

**1. The DB mutex is dropped around all flush/compaction I/O and re-taken only to
install results.**

- Flush (`db/flush_job.cc`): `db_mutex_->AssertHeld()` (856) →
  **`db_mutex_->Unlock()` (878)** → `BuildTable(...)` SST write (1003) →
  **`db_mutex_->Lock()` (1082)**; result installed via
  `TryInstallMemtableFlushResults` / `VersionSet::LogAndApply` under the mutex.
- Compaction (`db/compaction/compaction_job.cc`): `Run()` / `RunSubcompactions()`
  (1089 / 716) run the merge with the mutex **not** held; only `Install()` (1140,
  `AssertHeld` 1145) → `InstallCompactionResults` → `LogAndApply` takes it.

This is the same shape as the pre-V1 engine's `flush_oldest_frozen`, and is
exactly what storage-next is missing.

**2. Overload throttles writers; it never returns an error.** `DBImpl::WriteImpl`
→ `PreprocessWrite` consults the `WriteController` (`db/write_controller.{h,cc}`)
and, when behind, calls `DelayWrite`, which *sleeps* the writer proportionally;
the hard `StopToken` makes the writer *wait*, not fail
(`db/db_impl/db_impl_write.cc`, `db/column_family.cc::RecalculateWriteStallConditions`).
Thresholds are two-tier and far higher than storage-next's single hard block:
slow at `level0_slowdown_writes_trigger = 20` / `soft_pending_compaction_bytes_limit
= 64 GiB`; stop at `level0_stop_writes_trigger = 36` / `hard_pending_compaction_bytes_limit
= 256 GiB`. Flush is size-driven: `write_buffer_size = 64 MiB`,
`max_write_buffer_number = 2`, `level0_file_num_compaction_trigger = 4`.

**Group commit** (`db/write_thread.cc`: `JoinBatchGroup` / `EnterAsBatchGroupLeader`)
lets one leader write many concurrent writers' WAL records in a single fsync — the
amortization that makes durable throughput scale with writer concurrency.

Both reference engines are, in effect, the design target for the fix.

## Fix attempts that did not work

These were prototyped and reverted; recorded so they are not retried.

1. **Skip the per-commit pressure scan / re-enqueue when a task is already
   pending.** Broke correctness: the per-commit re-suggestion is the compaction
   feedback loop; suppressing it starved compaction, L0 crossed 16, and writes
   were rejected. Proves the per-commit maintenance work is load-bearing, not
   redundant.
2. **Gate the commit-count trigger on retained WAL bytes (≥ 8 MiB).** Only
   delayed the cliff to ~19k commits. The WAL is ~419 B/commit (1 user + 2
   timeline rows + framing), so 8 MiB ≈ 19k commits; and `retained_bytes` does
   not drop after a checkpoint (single 42 MB segment; segment-granular
   truncation deleted 0 segments), so once past 8 MiB the gate latches open and
   the trigger fires every 1024 commits again.
3. **Gate on un-flushed memtable bytes at half the rotation size (32 MiB).** A
   checkpoint snapshot does not shrink the active memtable — only size-driven
   rotation (at `active_rotation_bytes`, default 64 MiB,
   `branch/config.rs` / `lifecycle/budget.rs::active_rotation_bytes_from_budget`)
   does. So `active_bytes` is also monotonic up to 64 MiB and the half-rotation
   gate latched open mid-cycle.
4. **Gate on un-flushed memtable bytes at the full rotation size (64 MiB).**
   Pushed the onset from ~1k to ~5–30k commits but the cliff still returns at
   scale, because regression #2 (the global lock) reintroduces it the moment any
   size-driven maintenance starts. A checkpoint-trigger tweak cannot reach the
   lock contention.

Lesson: regression #1 (commit-count trigger) and regression #2 (global lock) are
independent. Fixing only #1 cannot eliminate the cliff.

## Planned fix

### Primary — lock-free maintenance install (addresses #2)

Adopt the pattern shared by both reference engines (RocksDB
`flush_job.cc` / `compaction_job.cc`: `Unlock` → I/O → `Lock` → `LogAndApply`;
pre-V1 `flush_oldest_frozen`: build outside lock → atomic `ArcSwap` install).
Background flush/compaction must do its I/O-heavy work **without** holding the
runtime mutex, taking the lock only for the brief atomic install of the result.

- Build the output segment(s) / run the compaction merge outside the lock.
- Re-acquire the lock only to swap the new segment set into the branch's level
  view (an `ArcSwap`-style atomic install) and update the manifest reference.
- Foreground commits then contend with maintenance only for the duration of the
  swap, not the I/O.

This removes the foreground stall for *all* maintenance, not just checkpoints,
and is the only change that addresses the dominant `foreground_wait_background_lock`
term. It is the larger architectural lift and the highest-leverage fix.

### Complement — size-driven flush / checkpoint (addresses #1) — IMPLEMENTED

Done. The commit-count WAL-growth trigger is **off by default**: the internal
`LifecycleWalGrowthPolicy.max_commits_since_checkpoint` is now `Option<u64>`,
`None` in `Default`, and the byte/segment bounds + size-based memtable rotation
drive flushing. The commit-count bound remains an explicit opt-in via the public
`StorageWalGrowthPolicy::Thresholds` (recovery-replay-count safety). Changed:
`lifecycle/config.rs`, `lifecycle/wal_growth.rs` (`trigger_for` /
`backpressure_trigger_for` gate on `Some`), `api/runtime/open_close.rs` mapping,
`api/options.rs` docs, `api/runtime/diagnostics.rs`.

Correction to the original plan: WAL truncation does **not** ride on size-driven
rotation — it is enqueued only by the WAL-growth checkpoint path
(`evaluate_wal_growth_policy`). With the commit-count trigger off, WAL truncation
now rests on the byte/segment triggers (256 MiB / 64 segments) + segment rolling.
At the scales that cliffed, the old commit-count checkpoints were deleting **0**
WAL segments anyway (single un-rolled segment), so this loses no real truncation;
WAL stays bounded. Decoupling truncation so it tracks flush cadence (smaller WAL)
is a follow-up.

Measured (single-row durable `load-seq`, default budget, 64 B values):
`load 100k` **58 → 9,068 ops/s** (~156×), `checkpoint_executions` 103 → 0, no
`LevelZeroTableBacklog` rejection; `engine-ycsb` durable workload A
**141 → 16,948 ops/s**, update p50 **12.84 ms → 116 µs**. Batched load unchanged
(~full speed). The residual at 100k is the base per-commit cost (~110 µs) with
background maintenance running concurrently — the global-lock stalls that remain
at larger scale are fix #2.

### Backpressure — throttle, don't reject (addresses the hard-reject failure mode)

storage-next's overload response is the harshest of the three designs: at 16 L0
tables it returns a `LevelZeroTableBacklog` error and aborts the commit, with no
intermediate slowdown tier. RocksDB never errors on overload — it *throttles*.
Adopt the `WriteController` model:

- Add a **slowdown tier** below the stop tier that *delays* (sleeps) admission
  proportionally to backlog, rather than jumping straight to rejection.
- Drive both tiers off backlog magnitude (L0 count and pending-compaction bytes)
  with thresholds in RocksDB's range (slow ≈ 20 L0 / 64 GiB; stop ≈ 36 L0 /
  256 GiB), not the current 16-table hard block.
- Reserve hard rejection for genuine resource exhaustion (the memory budget),
  not for transient compaction lag.

With the primary fix in place, compaction keeps up and these tiers rarely engage;
but they convert the remaining worst case from a write *failure* into graceful
backpressure, matching RocksDB semantics. (Group commit — batching concurrent
writers' WAL into one fsync, RocksDB `WriteThread` — is the complementary lever
for *concurrent* durable throughput and is tracked separately.)

### Sequencing

1. ✅ **Done** — the complement (#1, size-driven flush): a contained, low-risk
   change with a matching test slice; it removes the per-1024-commit churn
   (100k single-row durable 58 → 9,068 ops/s) and de-risks the larger change.
2. Land the primary (#2, lock-free maintenance install); this is the real fix and
   needs careful concurrency review (it changes the runtime locking contract).
3. Land the backpressure change (#3, throttle-not-reject) to remove the hard
   `LevelZeroTableBacklog` rejection failure mode.

## Verification plan

Re-run the reproductions and require:

- Single-row durable `load-seq` throughput stays within a small constant factor
  of batched across 5k → 1M commits (no progressive collapse), and never
  hard-rejects with `LevelZeroTableBacklog` under steady single-row ingest.
- `foreground_wait_background_lock` per commit stays flat with scale.
- engine-ycsb durable workload A/F per-op update latency stays flat with
  `--ops` (no 124 µs → 12.8 ms drift).
- No regression in batched throughput, recovery correctness, or the
  `format_goldens` / lifecycle test suites.

## Risks and open questions

- The global runtime mutex may have been chosen deliberately for correctness or
  simplicity in the rewrite; the lock-free install must preserve commit/branch
  MVCC ordering and recovery invariants. Needs a concurrency review against the
  L7/L8 contracts.
- Loosening the commit-count bound increases worst-case recovery replay for
  pathological many-tiny-commit workloads; bounded by the 256 MiB byte trigger.
- Multi-branch behaviour: any size-driven gate must consider per-branch
  un-flushed data, not just the initial branch.

## References

- storage-next: `lifecycle/config.rs`, `lifecycle/wal_growth.rs`,
  `lifecycle/durable/maintenance.rs`, `lifecycle/checkpoint.rs`,
  `lifecycle/compaction.rs`, `api/runtime/background.rs`, `branch/config.rs`,
  `lifecycle/budget.rs`.
- pre-V1 engine: `crates/storage/src/segmented/mod.rs`,
  `crates/storage/src/runtime_config.rs`, `crates/storage/src/txn/manager.rs`.
- RocksDB (`/home/anibjoshi/Documents/GitHub/rocksdb`):
  `db/flush_job.cc` (`Unlock`/`Lock` around `BuildTable`),
  `db/compaction/compaction_job.cc` (`Run`/`Install`),
  `db/db_impl/db_impl_write.cc` (`WriteImpl`/`PreprocessWrite`/`DelayWrite`),
  `db/write_thread.cc` (group commit), `db/write_controller.{h,cc}`,
  `db/column_family.cc` (`RecalculateWriteStallConditions`),
  `include/rocksdb/options.h`, `include/rocksdb/advanced_options.h` (defaults).
- benchmarks: `benchmarks/src/bin/storage_next_l9_scale.rs` (`--batch-size`),
  `benchmarks/src/bin/engine_ycsb.rs`, `benchmarks/src/bin/engine_kv_scale.rs`.
