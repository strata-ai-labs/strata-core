# STH-2 Implementation Plan: Systematic Fault-Injection Sweeps

Status: implemented (slices 2a–2f) — see **As built** below
Charter class: 5 — Error-path bugs / I/O error, OOM, disk-full (🟡 Partial → ✅)
Companion: `docs/architecture/v1-storage-testing-taxonomy-and-gaps.md`
Depends on: **STH-1** (the recovery oracle is the post-fault integrity check).

## Objective

Replace the 19 hand-enumerated fault windows with the SQLite discipline: **fail
the Nth backend operation, verify integrity, increment N until a full clean
pass.** Cover both fail-once and fail-continuously modes over the V1-reachable
backend operations (commit append + sync, checkpoint/flush publish,
snapshot-pruning delete), and add the two injection modes storage-next has *zero*
of today: disk-full (ENOSPC) and budget/memory exhaustion.

## As built (2026-06-18)

- **Reused, not new**: the sweep is built on the existing counting
  `FaultingBackend` (`src/testkit/mod.rs`) routed under a durable runtime via a
  feature-gated `StorageBackend::faulting_local_fs`, not a new `fault_backend.rs`.
  Harness + cases live in `src/testkit/fault_sweep/`; the integration target is
  `tests/fault_sweep.rs`.
- **Coarse trait-op sweep** over the V1-reachable set `{AppendObject, SyncObject,
  PublishObject, DeleteObject}`, discovered dynamically by a baseline trace; the
  fine 8 publish/delete sub-steps remain covered by the 19-window suite (a verified
  regression subset, untouched).
- **Delete** is reached via snapshot pruning (checkpoint ×2 + `SnapshotPruning`,
  retain newest 1). Deferred compaction-input deletion is multi-cycle and overlaps
  **STH-5** (fault-during-compaction) — out of scope here.
- **Deferred / out of V1 scope**: `ConditionalCreate/Update` and `WriteObject` are
  not reachable in V1 (see Seams); budget-size *sweeping* is a focused exhaustion
  test rather than a per-op sweep.
- **Soak**: seed count scales with `STRATA_STORAGE_FAULT_CASES`; `#[ignore]` soak
  test drives the deep multi-seed run.

## Why this matters (blog beat)

Hand-picked fault windows are testing the bugs you already imagined. The bug
lives two operations over, in the window you didn't write. SQLite's answer is
brutally simple: fail operation 1, check integrity; fail operation 2, check
integrity; … until a run completes with the injection never firing. That sweep
turns "we tested some error paths" into "we tested *every* error path on this
workload." StrataDB already has the seams; it just drives them by hand. This plan
makes the sweep the default and adds the resource-exhaustion faults that real
deployments hit first.

## Seams to build on (verified 2026-06-17)

- Eight backend I/O fault steps already exist:
  - `LocalFsPublishStep`: TemporaryCreate, TemporaryWrite, TemporarySync,
    FinalPublish, ParentSync — injectors at `src/backend/local_fs.rs:281–296`
    (`inject_temporary_write_publish_fault`, `…_sync_…`, `inject_final_publish_fault`,
    `inject_parent_sync_publish_fault`) plus targeted variants (263, 273).
  - `LocalFsDeleteStep`: BeforeRemoval, Removal, ParentSync (internal
    `arm_delete_fault`).
- Conditional manifest ops (`ConditionalCreate`, `ConditionalUpdate`) and
  `WriteObject` are **not reachable in V1**: the durable path publishes via
  `publish_object` (never `write_object`), and LocalFs returns
  `UnsupportedOperation` for the conditional ops — every caller today is a
  conformance test asserting *unsupported*. They are reserved for post-V1
  object-durable / distributed backends and are therefore out of V1 scope; the
  dynamic sweep will cover them automatically once such a backend invokes them.
- The 19 enumerated routes: `run_service_fault_window_harness`
  (`src/testkit/integration_harness.rs:726`, `EXPECTED_CASES = 19`) — these become
  named regression seeds, a *subset* of the sweep.
- Post-fault check: the STH-1 oracle (`testkit/recovery_oracle`).
- Budget seam: `StorageRuntimeBudget` / `scaled_closed_loop_test_profile`
  (`src/lifecycle/budget.rs:247`) for exhaustion driving.

## Coverage target (not line count)

Exit bar = "fail backend op N, sweep N, integrity-check each, over the
V1-reachable ops `{append, sync, publish, delete}`; plus ENOSPC and
budget-exhaustion modes." Measured by: every backend op *position* a
commit + checkpoint + prune workload reaches is failed at least once in both
fail-once and fail-continuously modes, each verified by the oracle; and there
exist ENOSPC and budget-exhaustion cases. Not measured by route count.
(`ConditionalCreate/Update` and `WriteObject` are post-V1 — see Seams.)

## Scope and slices (≤1,500 LOC each)

| Slice | Work | Exit gate |
|---|---|---|
| 2a | Op-counting fault backend wrapper | Wraps `Backend`; "fail the Nth op of kind K" (and Nth-overall); fail-once and fail-continuously modes |
| 2b | The sweep harness | For N in 1..: run workload failing op N, assert typed error + oracle-valid recovery + integrity; stop when injection never fires. Over the V1-reachable ops `{append, sync, publish, delete}` (delete reached via snapshot pruning, slice 2e) |
| 2c | Disk-full (ENOSPC) mode | Quota-bounded backend returns out-of-space mid-write; sweep + oracle; WAL halt-and-resume contract verified |
| 2d | Budget / memory exhaustion mode | Drive `LowMemory` budget to exhaustion; assert graceful typed `StoragePressure` (retryable, no panic/OOM), liveness (drain → resume), oracle-valid |
| 2e | `DeleteObject` coverage + `SWEEP_OPS` trim | Workload's checkpoint ×2 + `SnapshotPruning` issues a delete; sweep covers delete positions; `SWEEP_OPS` = `{append,sync,publish,delete}`; covered-set pinned |
| 2f | Soak depth | Seed count scales with the case budget so the soak deepens past the CI default; `#[ignore]` soak test exists |

## Implementation detail

### 2a — Counting fault backend (`src/testkit/fault_backend.rs`)
A `Backend` decorator holding `Arc<Mutex<FaultPlan>>`. `FaultPlan` = { target op
kind, trigger N, mode: FailOnce | FailContinuously, error: Io | NoSpace }. Counts
matching ops; on the Nth, returns the configured error (and for FailOnce, disarms).
Reuses the existing `LocalFsPublishStep`/`LocalFsDeleteStep` taxonomy so a sweep
can target a specific step or all steps.

### 2b — Sweep harness (`tests/fault_sweep.rs`)
```
for n in 1.. {
    let outcome = drive_workload_failing_op(seed, n, mode);
    assert!(outcome.op_result.is_typed_error_or_ok());   // never panic/UB
    assert_recovery_oracle_holds(outcome.reopened);       // STH-1
    if !outcome.fault_fired { break; }                    // swept past the end
}
```
Two passes (FailOnce, FailContinuously). The 19 legacy routes are asserted as a
covered subset (regression seeds), not deleted.

### 2c — ENOSPC (`src/testkit/fault_backend.rs` + `tests/fault_sweep_enospc.rs`)
A byte-quota mode: once cumulative bytes exceed Q, writes return NoSpace. Sweep Q
downward; at each Q assert the WAL writer halts cleanly (per contract) and an
explicit resume after "freeing space" recovers to an oracle-valid prefix.

### 2d — Budget exhaustion (`tests/fault_sweep_budget.rs`)
Open with a tiny `StorageRuntimeBudget`; drive sustained load; assert admission
returns a typed `StoragePressure`/budget rejection (class+code), the process
never OOMs or panics, background maintenance still makes progress (liveness), and
recovery is oracle-valid.

## Constraints

- Deterministic, seeded; failures print seed + the failing op index N.
- Assert typed error class/code on every injected failure; never display text.
- Behavioral test names only.
- The sweep must terminate (bounded by ops-per-workload); CI runs a scaled
  workload so the full sweep completes in seconds; nightly runs a larger one.

## Exit gate

- Full fail-once and fail-continuously sweeps over the V1-reachable ops
  `{append, sync, publish, delete}` on a commit + checkpoint + prune workload,
  every position oracle-verified.
- ENOSPC (NoSpace position sweep + byte-quota) and budget-exhaustion cases present
  and green; the budget case proves typed, retryable back-pressure that drains and
  resumes.
- The 19 legacy windows remain green as the fine-step regression suite.
- `ConditionalCreate/Update` + `WriteObject` documented post-V1; deferred
  compaction-input deletion tracked under STH-5.
- Charter class 5 flips 🟡 → ✅ with this plan as evidence.
