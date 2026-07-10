# M4P-L8K Load-Performance Cycle — Findings & Learnings

Last updated 2026-06-17. Scope: the L8K load-performance cycle — K1 (admission-slowdown
removal), K2/K2a (off-lock publish), K2b (off-lock durability suite + 10M gate). Captured so the
next cycle (billion-scale) starts from evidence, not rediscovery.

## TL;DR

- **Off-lock publish (K2a) works as a load-speed lever.** At 10M it cut the foreground
  runtime-lock wait from **107s → 6.5s**, commit-call time **293s → 49s**, and removed writer
  throttling entirely. Load runs in **51.46s** with **zero** admission-wait timeouts and zero
  rejections.
- **It exposed — did not create — a compaction-throughput deficit.** Once backpressure is
  removed, the writer outruns compaction. 10M `final_l0` moved from **129** (pre-K2, writer
  throttled) to **490** (K2, writer unthrottled). At 5M, compaction still keeps up (`final_l0≈27`),
  so the deficit is **scale-dependent**: it appears when sustained write rate exceeds compaction
  throughput for long enough.
- **The next problem is structural and billion-scale-shaped:** compaction that scales with write
  throughput (RocksDB-class — multi-threaded compaction, sub-compactions, per-shard parallelism,
  flush parallelism) plus a principled L0/stall policy. A single off-lock publish slot is not
  enough. **Do not chase `final_l0` inside L8K** — it is the canary for the billion-scale cycle.

## What landed

### K1 — remove Urgent admission slowdown (`83430c8e`)
Removed the `Urgent` admission-slowdown tier. The writer is no longer aggressively throttled by
the admission path. This is half of why L0 grows under K2: the brake came off here.

### K2a — off-lock publish mechanism (`74e817e0`)
Moves the durable table-manifest fsync off the global runtime lock for background flush /
compaction / materialization. Three-phase publish:

1. **Under lock:** install the new tables in memory, reserve the manifest sequence
   (`reserve_manifest_sequence` — advances `next_manifest_sequence` for cross-branch monotonic
   uniqueness), and try-acquire the per-branch publish slot.
2. **Lock released:** `persist_reserved_manifest` (the fsync) → `publish_replace_manifest` →
   `backend.publish_object`. This is the slow part, now off the global lock.
3. **Under lock:** `record_reserved_manifest` (records tables **without** re-advancing the
   sequence) + finish.

- **Per-branch publish slot** (`try_acquire_branch_publish_guard`): a `compare_exchange`
  try-lock-or-defer on an `Arc<AtomicBool>`. Global → per-branch lock order, never blocks → no
  deadlock. Busy ⇒ the task finishes `Deferred`. RAII guard clears the flag on drop.
- **Fsync-failure handling** (`table_manifest_debt_outcome`): on fsync failure the outcome is
  `Completed` + recovery-health debt (not an immediate retry; recovery handles it on next open).
- Synchronous callers stay 2-phase (publish under the lock) via the
  `install_*_without_publish` / `publish_*_outcome_manifest` split.

### K2b — off-lock durability suite + 10M gate (this commit)
Test-only; **no production-path change**. Adds a `#[cfg(all(test, unix))]` targeted manifest-fault
hook and:
- **Gate:** `off_lock_manifest_fsync_fault_{before_visibility,visible_unconfirmed}_recovers_committed_rows`
  — crash mid-off-lock-publish, reopen, all rows recovered.
- **Invariant:** `durable_table_catalog_reserves_monotonic_sequences_without_double_advance`.
- **Smoke (`#[ignore]`):** `concurrent_same_branch_flush_and_compaction_preserve_manifest_monotonicity`.
- The off-lock window + source guard are covered by K2a tests
  (`assert_durable_background_drain_three_phase_publish`, the liveness off-lock-ns assertions).

## Performance findings (the numbers)

10M, `standard` engine, `load-seq`, 150-byte values, 1000-batch, `--diagnostic-source-shape`,
Apple M1 Pro (8 cores, 16 GB). Old-cache baseline load ≈ **36.78s** (the 2× reference).

| Metric | Pre-K2 (`75e6460b`) | K2 off-lock (`74e817e0`) |
|---|---|---|
| `final_l0` | 129 | **490** |
| `foreground_wait_background_lock_ns` | **107s** | **6.5s** |
| `commit_call_ns` | 293s | 49s |
| writer slowdowns (`admission_slowdown_attempts`) | 2135 (~37s) | 0 |
| load elapsed | (much slower) | **51.46s** (≤ 2× old ✓) |
| `admission_wait_timeouts` | 0 | 0 |
| `maintenance_deferred` | 0 | 0 |
| `background_maintenance_tasks` | 3917 | 694 |

- **5M (`83430c8e`)** stays bounded: `final_l0 ≈ 27` (5.4 / million). At 10M it is 49 / million —
  a ~9× density jump, i.e. compaction falls behind only once the load runs long enough.
- The off-lock change is a clean **speed-for-transient-debt trade**: faster, throttle-free load at
  the cost of more un-compacted L0 during the load. The load itself never stalls (timeouts = 0,
  deferred = 0, no rejections), and the benchmark's read source-shape gate still passes
  (`passed=true`, filter-cache reads).

## The structural insight — why this points at billion-scale

Pre-K2 kept L0 low (129) **by paying** 107s of foreground lock-wait + 37s of writer throttling —
the writer was paced to compaction's speed. K2 removed that pacing. So the real question L8K
surfaced has two halves:

1. **Throughput:** compaction must scale with write throughput. At 1B keys (100× the data, far
   longer sustained pressure) "compaction keeps up only if we throttle the writer" is the wall.
   RocksDB-competitive means multi-threaded compaction + sub-compactions + per-shard/flush
   parallelism — not one off-lock publish slot.
2. **Policy:** we currently have neither a principled write-stall (RocksDB's
   `level0_slowdown/stop_writes_trigger`) nor enough compaction throughput to avoid needing one.
   The billion-scale cycle has to choose: bounded-L0-via-stalls vs. fast-writes-with-bounded-debt,
   and make compaction fast enough that the chosen policy is cheap.

## Durability findings (manifest-fsync faults)

The two fault shapes recover differently — both restore every committed row, but the durable
manifest sequence behaves differently. This is the key gotcha for any future manifest-fault test:

- **Before visibility** (`LocalFsPublishStep::TemporarySync` → `FailedBeforeVisibility`): the
  temp-file fsync fails **before** the rename, so the new manifest never becomes visible. Recovery
  falls back to the prior durable manifest + retained WAL; **`next_manifest_sequence` is
  unchanged** (assert `==`).
- **Visible, durability unconfirmed** (`LocalFsPublishStep::ParentSync` →
  `VisibleDurabilityUnconfirmed`): the rename **succeeds** (manifest is visible = durable) and only
  the parent-directory fsync fails. Recovery sources from the new manifest; **the sequence may
  advance** (assert `>=`, never regress).
- Unified recovery (`apply_loaded_table_manifest_to_branch`) sources owned levels from whichever
  durable manifest exists and replays the retained WAL, so a swap-ahead-of-fsync table recovers
  from the prior manifest + WAL.

## Testing & tooling notes (so we don't rediscover)

- **Driving the off-lock path deterministically:** under
  `StorageMaintenanceSchedulingPolicy::DeterministicInline`,
  `enqueue_lifecycle_maintenance_for_test(MaintenanceTaskRequest::flush(branch))` **drains inline
  immediately** (the background 3-phase off-lock publish) — no `wait_background_idle_for_test`
  needed. You must `rotate_*_for_test()` first (flush targets *frozen* tables).
  `wait_background_idle_for_test()` alone does **not** trigger a flush (nothing is enqueued).
- **`flush_default_branch_for_test` / `flush_branch_for_test` is SYNCHRONOUS** (`rotate +
  flush_frozen` under the lock) — it is *not* the off-lock path. Use it only when you want the
  2-phase synchronous publish.
- **`'static` backend in tests:** `open_with_backend` + the background driver require the backend
  borrowed for `'static`. Use `let backend: &'static _ = Box::leak(Box::new(StorageBackend::local_fs(root)))`
  (as `open_durable_inline_for_admission_test` does). A scoped `&backend` fails to compile.
- **Targeted fault hook:** `LocalFsBackend.publish_fault` is `#[cfg(all(test, unix))]
  Option<(LocalFsPublishStep, Option<String>)>` — an **inlined tuple, not a named type**.
  `StorageBackend::inject_manifest_publish_fault(branch, before_visibility)` arms it against the
  branch table-manifest object only (table-data publishes pass through). A flush publishes
  table-data *before* the manifest, so an untargeted one-shot fault hits the wrong object — target
  by object name.
- **Retired type-inventory guard:** this note originally referenced temporary
  generated type-inventory tooling. That cleanup scaffold has been retired. Do
  not regenerate inventory artifacts for this plan; use focused tests, source
  guards, and review of any new named boundary type instead.
- **`cargo test` takes one positional filter.** Multiple names → `unexpected argument`. Use
  `cargo test ... -- name1 name2` (filters after `--`).
- **Benchmark CLI / output** (`benchmarks/src/bin/storage_next_l9_scale.rs`):
  `--scales 10m --engines standard --workloads load-seq --value-bytes 150 --batch-size 1000
  --diagnostic-source-shape [--diagnostic-final-drain]`. The decisive numbers are on **stderr**
  (`eprintln!`): `load-seq … elapsed=`, `load-phase … admission_wait_timeouts=`,
  `post-load-source-shape … final_l0=`. The JSON in `benchmarks/results/storage-next-l9/` records
  `source_shape_metrics.l0_tables_per_million_rows_after_load` (multiply by millions for
  `final_l0`).
- **`--diagnostic-final-drain` cannot currently settle L0 under off-lock.** It issues flush +
  compaction back-to-back; the off-lock per-branch slot is still held by the flush's publish, so
  the compaction **defers** and the diagnostic does not retry (`final_l0=597, compact_status=deferred,
  compact_changes=0, passed=false`). This is a **diagnostic-tooling gap** (the drain should wait on
  / retry slot-busy), **not** a load-path bug — normal-load `maintenance_deferred=0`. It means we
  do not yet have a clean measured "settled L0" for 10M.

## Deferred / open for the billion-scale cycle

- **Compaction parallelism / per-branch sharding** (L8I Groups D/E) — the main throughput lever;
  scoped out of L8K because the 2× elapsed target was met.
- **Sub-compactions** (split one compaction into parallel key ranges) and **flush parallelism.**
- **L0 / write-stall policy decision** — bounded-L0-via-stalls vs. fast-writes-bounded-debt.
- **Strata vs RocksDB gap benchmark** at 10M / 100M / 1B (load, point, range, mixed; throughput,
  read latency, write amp, space amp, L0/compaction-debt over time) to ground the plan in
  measurement.
- **Settled-L0 measurement:** fix the `--diagnostic-final-drain` / off-lock-slot retry so we can
  observe whether 10M's 490 actually drains, and how fast.
- **ArcSwap lock-free reads** (L8I Group D) and checkpoint/flush-watermark off-lock — still
  deferred from K2a scope.
