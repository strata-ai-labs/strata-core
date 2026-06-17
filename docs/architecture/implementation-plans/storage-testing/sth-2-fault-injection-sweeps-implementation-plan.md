# STH-2 Implementation Plan: Systematic Fault-Injection Sweeps

Status: draft
Charter class: 5 — Error-path bugs / I/O error, OOM, disk-full (🟡 Partial → ✅)
Companion: `docs/architecture/v1-storage-testing-taxonomy-and-gaps.md`
Depends on: **STH-1** (the recovery oracle is the post-fault integrity check).

## Objective

Replace the 19 hand-enumerated fault windows with the SQLite discipline: **fail
the Nth backend operation, verify integrity, increment N until a full clean
pass.** Cover both fail-once and fail-continuously modes across all eight backend
I/O steps, and add the two injection modes storage-next has *zero* of today:
disk-full (ENOSPC) and budget/memory exhaustion.

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
- The 19 enumerated routes: `run_service_fault_window_harness`
  (`src/testkit/integration_harness.rs:726`, `EXPECTED_CASES = 19`) — these become
  named regression seeds, a *subset* of the sweep.
- Post-fault check: the STH-1 oracle (`testkit/recovery_oracle`).
- Budget seam: `StorageRuntimeBudget` / `scaled_closed_loop_test_profile`
  (`src/lifecycle/budget.rs:247`) for exhaustion driving.

## Coverage target (not line count)

Exit bar = "fail backend op N, sweep N, integrity-check each, over all 8 steps;
plus ENOSPC and budget-exhaustion modes." Measured by: every backend op *position*
on a representative durable workload is failed at least once in both fail-once and
fail-continuously modes, each verified by the oracle; and there exist ENOSPC and
budget-exhaustion sweeps. Not measured by route count.

## Scope and slices (≤1,500 LOC each)

| Slice | Work | Exit gate |
|---|---|---|
| 2a | Op-counting fault backend wrapper | Wraps `Backend`; "fail the Nth op of kind K" (and Nth-overall); fail-once and fail-continuously modes |
| 2b | The sweep harness | For N in 1..: run workload failing op N, assert typed error + oracle-valid recovery + integrity; stop when injection never fires. Over publish + delete + read steps |
| 2c | Disk-full (ENOSPC) mode | Quota-bounded backend returns out-of-space mid-write; sweep + oracle; WAL halt-and-resume contract verified |
| 2d | Budget / memory exhaustion mode | Drive `StorageRuntimeBudget` to exhaustion; assert graceful typed rejection (no panic/OOM), liveness preserved, oracle-valid |

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

- Full fail-once and fail-continuously sweeps over all 8 backend steps on a
  representative durable workload, every step oracle-verified.
- ENOSPC and budget-exhaustion sweeps present and green.
- The 19 legacy windows are a verified subset of the sweep.
- Charter class 5 flips 🟡 → ✅ with this plan as evidence.
