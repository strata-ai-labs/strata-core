# STH-4 finding: silent data loss on a publish fault during checkpoint + flush

**Status:** ✅ **RESOLVED** (2026-06-18) — fix landed; regression un-ignored and green for every swept publish position. See "Resolution".
**Found by:** STH-4 deterministic-simulation driver (slice 4c), soak seed 74; minimized to a deterministic repro.
**Severity:** **high** — silent loss of acknowledged, `Always`-durable commits, with no error returned to the caller. Not covered by the "WAL writer halts on fsync failure" rule (that is WAL fsync; this is *object publish*, which the engine swallows).
**Regression test:** `crates/storage-next/src/testkit/simulation/faults.rs::tests::regression_publish_fault_during_checkpoint_flush_loses_no_data` (permanent guard; passes for all swept publish positions) + the self-healing companion `transient_manifest_publish_failure_defers_then_resumes_checkpoint`.

## Symptom

On a durable runtime (`Always` durability, `EvaluateAndEnqueue` scheduling), a `PublishObject` fault that returns `NoSpace` during a **batched `[Checkpoint, Flush]` drain** silently discards committed data. Every `commit` returns `Ok`, every `drain_maintenance` returns `Ok`, then a clean strict reopen recovers **0** rows.

## Minimal deterministic repro

1. Open durable, `Always`, `EvaluateAndEnqueue`, on a faulting local-fs backend armed with `PublishObject` → `NoSpace`, **Once**, at publish call **#7**.
2. Commit 4 distinct puts (k0..k3).
3. `enqueue_maintenance(Checkpoint, Branch)` → `drain_maintenance()` (creates snapshot 1; truncates the WAL).
4. Commit 4 more distinct puts (k4..k7) — 8 acknowledged commits total.
5. `enqueue_maintenance(Checkpoint, Branch)` + `enqueue_maintenance(Flush, Branch)` → **one** `drain_maintenance()`. The 7th publish faults (`NoSpace`); the drain returns `Ok`.
6. Drop, reopen on a plain local-fs backend (strict). `scan_recovered` → **0 keys** (expected 8).

## What is and isn't required (each isolated empirically)

- **Not** faults in general — without the fault, recovery equals the model state exactly.
- **Not** commit count / WAL rotation — 20 distinct puts with no maintenance recover fully.
- **Not** the workload's deletes, **not** snapshot pruning, **not** a checkpoint alone (all safe).
- **Required:** a publish fault **during a batched drain that contains a `Flush`**, after a prior checkpoint truncated the WAL. (`Checkpoint`-only and `Checkpoint`+`SnapshotPruning` batches do not lose; adding `Flush` does.)

## Mechanism (code trace)

1. The first checkpoint truncates the WAL — the early data now lives only in snapshot 1.
2. In the batched `[Checkpoint, Flush]` drain, the flush's `persist_flush_watermark` + `truncate_wal` advance the WAL-truncation point from the **manifest watermark** — *not atomically* with the snapshot/manifest publish.
3. The snapshot/manifest `PublishObject` faults (`NoSpace`). The drain loop **swallows it**: `finish_started` records a `Failed` `MaintenanceOutcome` but the drain returns `Ok` (no signal to the caller).
4. The WAL is now truncated past a watermark whose snapshot is missing / inconsistent.
5. Recovery sees a manifest watermark referencing a missing snapshot → lossy fallback sets the trusted replay-start to `CommitVersion::ZERO`, but the WAL was already truncated → **0 rows, no error**.

## Suspect code (for the fix)

- `crates/storage-next/src/service/checkpoint.rs::checkpoint()` — the `persist active WAL segment` → `publish snapshot` → `persist snapshot facts` ordering.
- `crates/storage-next/src/lifecycle/checkpoint.rs::run_checkpoint_follow_ups()` + `persist_flush_watermark` + `truncate_wal`, and `wal_truncation_request_from_maintenance_task()` — **WAL truncation must be gated on a durably-published snapshot**, not on the manifest watermark alone.
- The durable drain loop in `crates/storage-next/src/lifecycle/durable/maintenance.rs` — a checkpoint/flush publish failure is recorded as `Failed` but **not surfaced**, so `drain_maintenance` returns `Ok` (callers cannot detect it).
- `crates/storage-next/src/lifecycle/recovery.rs` — the lossy fallback to `CommitVersion::ZERO` against an already-truncated WAL is where silent emptiness materializes; recovery arguably should fail loud here.

## Recommended fix direction

1. **Do not advance WAL truncation (or the flush watermark used for truncation) until the snapshot/manifest publish is confirmed durable.** Make the truncation point a function of *durably published* state, not the in-flight manifest watermark.
2. **Stop swallowing the publish failure** in the drain — surface it (and/or halt + require explicit resume, consistent with the WAL-fsync-failure rule) so a failed checkpoint/flush cannot be followed by a destructive truncation and cannot report `Ok`.
3. Re-run `regression_publish_fault_during_checkpoint_flush_loses_no_data` (un-ignored) and the STH-4 fault-simulation soak to confirm closure.

## Resolution (2026-06-18)

The prove step corrected the initial hypothesis. The failing publish is the **flush's table-manifest publish**, not the checkpoint's snapshot publish: the flush installs an L0 table in memory (`record_table`) and its manifest publish then faults (`NoSpace`), leaving **reserved-but-unpublished table-manifest debt** — the rows live only in an L0 table no durable manifest references. The batched checkpoint then advances the WAL-replay floor (snapshot + `active_wal_segment`) past those rows; on reopen the durable manifest does not list the table and the WAL is already truncated → silent loss.

**Fix (surgical — realizes the invariant via deferral rather than a halt):** a checkpoint **defers** while a table-manifest publish is outstanding, so the floor can never move past rows no durable manifest covers; the WAL retains them and recovery replays them. The signal is a shared catalog flag `LifecycleDurableTableCatalog::manifest_publish_pending` — set on `record_table_with_provenance` (in-memory install), cleared on `record_manifest` / `record_reserved_manifest` (durable publish). A failed publish leaves it set. Both checkpoint entry points consult it: the background `start_next_background_checkpoint_maintenance` **and** the synchronous `DurableCheckpointMaintenanceRunner` (the path `drain_maintenance` / `EvaluateAndEnqueue` takes — the original miss was patching only the background path). The defer is **self-healing**: once a later flush republishes the manifest the debt clears and the checkpoint resumes. Recovery rebuilds the catalog via `record_manifest`, which clears the flag, so a reopen always starts settled.

Files: `crates/storage-next/src/lifecycle/table_manifest.rs` (the flag), `crates/storage-next/src/lifecycle/durable/maintenance.rs` (both checkpoint defers). Tests: the regression (un-ignored, all positions) + `transient_manifest_publish_failure_defers_then_resumes_checkpoint` (self-healing, loss-free).

> **Note (multi-branch precision):** the flag is catalog-global. For a single branch it is exact (a branch manifest is cumulative over its owned tables, so a later publish covers earlier installs). Across branches a successful publish on branch B clears the flag set by branch A's install; the harness is single-branch, so this is untested. Tracked as a follow-up — go per-branch if a multi-branch soak ever exercises it.

## Impact on STH-4 / class 9

Class 9's interleaving driver + replay + soak are delivered (4b/4c/4d). This specific bug is **resolved** and the soak now clears it (and every seed through 154). Class 9 **still stays open**: with seed 74 fixed the soak progressed and surfaced a *separate, pre-existing* power-loss recovery `Gap` at seed 155 (SplitRename / Standard, no injected fault — independent of this fix). See `sth-4-finding-splitrename-power-loss-gap.md`. Class 9 closes when that bug is fixed and the fault-simulation soak runs clean end-to-end.
