# M4P-L8H Group A — Durable Publish Cost Audit

Status: complete (Slice 1)

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8h-durable-maintenance-liveness-implementation-plan.md`

This is the Group A deliverable: a source-owned explanation of what the durable
background-maintenance **publish** phase does while holding the runtime/commit
lock, which of it is movable off-lock, and the counter that proves each
disposition. It is the baseline for the Slice 2 (L8H-CD) publish/manifest
decoupling. No publish code is *moved* in Slice 1 — only measured.

## How publish is structured today

The durable background drain runs a `snapshot → build → publish` state machine
(`crates/storage-next/src/api/runtime.rs`, `drain_durable_background_round`,
~line 5185):

1. **start** (lock held) — select the next task and snapshot inputs.
2. **build** (lock released) — `pending_build.build()` at runtime.rs:5270 does the
   merge/encode off-lock. Measured by
   `record_lifecycle_background_task_unlocked_build` (runtime.rs:5271).
3. **publish** (lock re-acquired at runtime.rs:5276) —
   `finish_background_maintenance` swaps the in-memory layout pointers **and**
   persists the durable table manifest (fsync) under the same lock. Measured in
   aggregate by `record_lifecycle_background_task_publish_lock` (runtime.rs:5282).

The defect L8H targets: the **publish** critical section includes durable disk
I/O (manifest fsync, snapshot/checkpoint writes), so foreground commits — which
need the same runtime lock — serialize behind background durable I/O. At 1M:
publish-under-lock ≈ 10.77s, foreground lock-wait ≈ 9.55s, merge ≈ 0.32s. The
first full 5M run after Slice 1 confirms the coupling worsens with scale:
publish-under-lock ≈ 317.4s (**86%** of the 367.4s maintenance total, up from 68%
at 1M), foreground lock-wait ≈ 304.1s (**80%** of the 380.8s run), unlocked build
≈ 49.4s, merge ≈ 3.9s. 304 of the 317s publish-lock window — 96% — directly
blocks the foreground writer.

## New Slice-1 counter

`lifecycle_background_publish_manifest_persist_ns` (perf_trace.rs) times the
manifest-persist chokepoint `publish_table_manifest_for_branch_with_budget`
(`crates/storage-next/src/lifecycle/table_manifest.rs`, around the
`service.publish_replace_manifest(...)` call). One instrumentation point covers
both flush publish (`publish_table_manifest_after_flush`) and compaction install
(`install_published_durable_compaction`), which both route through it. With it:

```
in-memory pointer/state swap ≈ publish_lock_ns − publish_manifest_persist_ns
durable manifest persist (fsync) = publish_manifest_persist_ns
```

This is the separation Group A's exit gate requires and the metric Slice 2 will
drive toward zero-under-lock. The benchmark
(`benchmarks/src/bin/storage_next_l9_scale.rs`) emits this counter alongside
`lifecycle_background_task_publish_lock_ns` in the perf-trace dump, so the
publish-lock → manifest-persist split — and Slice 2's progress driving it to
zero-under-lock — is observable from benchmark output, not only from unit tests.

## Audit table

Disposition legend: **LOCK** = must stay under the runtime lock (in-memory
pointer/state swap); **OFF** = movable off-lock in Slice 2 (durable I/O).

| # | Call site | Step kind | Lock today | Lock-correctness dependency | Durability-ordering dependency | Proposed (Slice 2) | Proving counter |
|---|---|---|---|---|---|---|---|
| 1 | `drain_durable_background_round` publish (runtime.rs:5276–5284) | aggregate publish | held | swaps shared layout pointers visible to commits | — | split: LOCK swap, OFF persist | `lifecycle_background_task_publish_lock_ns` |
| 2 | flush pointer swap — `install_prepared_durable_flush` → `replace_frozen_with_level_zero_table` (flush.rs:~806) | pointer swap | held | a commit must never observe a layout disagreeing with frozen/L0 state | — | **LOCK** | (publish_lock − manifest_persist) |
| 3 | flush manifest publish — `publish_table_manifest_after_flush` → `publish_table_manifest_for_branch_with_budget` → `publish_replace_manifest` (table_manifest.rs:~380) | manifest persist + fsync | held | none (durability, not visibility) | new L0 table must be durable in manifest before recovery relies on it / before WAL retiring its inputs is truncated | **OFF** | `lifecycle_background_publish_manifest_persist_ns` |
| 4 | compaction pointer swap — `install_published_durable_compaction` → `install_branch_compaction_prepared_plan` (compaction.rs:~631) | pointer swap | held | a commit must never observe a half-installed level layout | — | **LOCK** | (publish_lock − manifest_persist) |
| 5 | compaction manifest publish — `install_published_durable_compaction` → `publish_table_manifest_for_branch_with_budget` (rewrite_publication.rs:~251) | manifest persist + fsync | held | none | compacted output durable in manifest before inputs are retired/truncated | **OFF** | `lifecycle_background_publish_manifest_persist_ns` |
| 6 | checkpoint snapshot write — `publish_checkpoint_rows` → `CheckpointService::checkpoint` snapshot publish (checkpoint.rs:~269) | snapshot object write + fsync | held | none | snapshot object durable before snapshot-facts manifest references it | **OFF** | (publish_lock; finer split deferred to Slice 2) |
| 7 | checkpoint manifest/snapshot-id publish — `CheckpointService::checkpoint` snapshot-facts persist (checkpoint.rs:~287) | manifest persist + fsync | held | snapshot id advancement visible to readers | snapshot-facts durable only after snapshot object durable | **LOCK** for id advance, **OFF** for fsync | (publish_lock; Slice 2) |
| 8 | flush-watermark persist — `run_next_background_flush_watermark_maintenance` → `persist_flush_watermark_inner` → `persist_flush_watermark` (maintenance.rs:~1522) | manifest persist (watermark) | held (runs in start phase, synchronous) | watermark advance visible to truncation | watermark may advance only after manifests for the retired frozen state are durable | **OFF** persist, keep ordering gate | (publish_lock; Slice 2/D) |
| 9 | WAL truncation — `truncate_wal` → `delete_covered_segments` (checkpoint.rs:~1731) | segment delete | **not held** (build phase) | — | a segment may be deleted only after the manifest covering its data is durable | already OFF; **add ordering gate** in Slice 2/D | `lifecycle_background_task_unlocked_build_ns` |
| 10 | admission pressure collection + blocking-pressure wait — `collect_storage_pressure_with_budget` / `storage_pressure_decision` (compaction.rs); `background_wait_after_pressure_rejection` (runtime.rs) | admission decision/wait | collection under commit lock; wait off-lock | pressure read consistent with committed layout | — | **Slice 1 (Group B): liveness fix landed** | `lifecycle_write_admission_wait_attempts` / `_wait_progress_resets` / `_wait_timeouts` |

## Why each step currently holds the lock (preserve in Slice 2)

1. Persisting the manifest under the runtime lock guarantees no commit observes a
   layout that disagrees with durable state (rows 2, 4, 7).
2. Synchronous manifest fsync inside publish guarantees WAL truncation and
   flush-watermark advancement never outrun manifest durability (rows 3, 5, 8, 9).
3. The bounded admission deadline (row 10) surfaces a genuinely dead/stuck
   maintenance executor instead of hanging — **now liveness-gated** (Group B).

Slice 2 must preserve all three while moving rows 3, 5, 6, 7-fsync, 8 off the
lock: pointer swap stays under the lock; manifest/snapshot persistence runs
off-lock but is ordered *before* any recovery reliance on the new table and
*before* WAL truncation that retires its inputs.
