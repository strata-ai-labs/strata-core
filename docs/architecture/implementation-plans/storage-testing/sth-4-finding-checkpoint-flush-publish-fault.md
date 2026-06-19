# STH-4 finding: silent data loss on a publish fault during checkpoint + flush

**Status:** open bug, captured as a failing-then-fixed regression; **engine fix pending** (own `/audit-fix` slice).
**Found by:** STH-4 deterministic-simulation driver (slice 4c), soak seed 74; minimized to a deterministic repro.
**Severity:** **high** — silent loss of acknowledged, `Always`-durable commits, with no error returned to the caller. Not covered by the "WAL writer halts on fsync failure" rule (that is WAL fsync; this is *object publish*, which the engine swallows).
**Regression test:** `crates/storage-next/src/testkit/simulation/faults.rs::tests::regression_publish_fault_during_checkpoint_flush_loses_no_data` (`#[ignore]` — un-ignore when fixed).

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

## Impact on STH-4 / class 9

Class 9's interleaving driver + replay + soak are delivered (4b/4c/4d). Class 9 **stays open** — the durability invariant the DST checks is currently violated by this bug. Class 9 closes when the engine fix lands and the fault-simulation soak runs clean.
