//! STH-2 systematic fault-injection sweeps.
//!
//! Routes the counting [`FaultingBackend`](crate::testkit::FaultingBackend) under
//! a durable runtime and applies the `SQLite` discipline: a baseline run traces every
//! backend operation a workload invokes, then each counted position is failed —
//! once and continuously — and the reopened database is checked against the STH-1
//! recovery oracle. No acknowledged commit may be lost; the faulted commit is
//! in-doubt (present-or-absent, atomically); maintenance faults are non-destructive
//! to committed state.

use std::path::{Path, PathBuf};

use strata_core_next::BranchId;

use super::recovery_oracle::model::{ExpectedState, OracleDurability};
use super::recovery_oracle::verify::{
    classify_recovered, scan_recovered, CrashFamily, RecoveryOracleViolation,
};
use super::recovery_oracle::workload::{
    default_branch, generate_workload, oracle_prefix_key, oracle_space, to_commit_mutation,
    SCAN_LIMIT,
};
use crate::api::{
    CommitBatch, CommitOptions, MaintenanceRequest, MaintenanceScope, MaintenanceTask,
    StorageBackend, StorageDurabilityPolicy, StorageMaintenanceSchedulingPolicy,
    StorageOpenOptions, StorageRuntime,
};
use crate::testkit::{
    BackendOperation, FaultKind, FaultMode, FaultRule, FaultScript, TestkitError,
};

/// Seeds swept by the CI-fast lane (the soak budget is the integration target's
/// `case_limit`, which caps total cases regardless of this).
const SWEEP_SEED_BUDGET: u64 = 4;
/// Commits per workload before the forced checkpoint — small so the bounded
/// sweep stays cheap while still exercising the durable op set.
const SWEEP_OP_COUNT: usize = 4;
/// Candidate write-side backend operations — the ones that can lose or corrupt
/// acknowledged data. The sweep fails each that the workload actually invokes; the
/// commit + checkpoint workload exercises append / sync / publish today. Delete,
/// write, and the conditional ops belong to the deferred-deletion / compaction /
/// off-lock manifest paths a bounded synchronous workload never reaches — they are
/// listed so they are swept automatically once a richer workload (a follow-on)
/// invokes them. Read / list / metadata faults are query-time, not data-loss, and
/// are exercised incidentally by the open-recovery path.
const SWEEP_OPS: [BackendOperation; 7] = [
    BackendOperation::AppendObject,
    BackendOperation::SyncObject,
    BackendOperation::PublishObject,
    BackendOperation::WriteObject,
    BackendOperation::DeleteObject,
    BackendOperation::ConditionalCreate,
    BackendOperation::ConditionalUpdate,
];

/// Counters describing a fault-sweep run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FaultSweepOutcome {
    seeds_executed: usize,
    baseline_ops_traced: usize,
    positions_swept: usize,
    once_cases: usize,
    continuously_cases: usize,
}

impl FaultSweepOutcome {
    #[must_use]
    pub const fn seeds_executed(&self) -> usize {
        self.seeds_executed
    }
    #[must_use]
    pub const fn baseline_ops_traced(&self) -> usize {
        self.baseline_ops_traced
    }
    #[must_use]
    pub const fn positions_swept(&self) -> usize {
        self.positions_swept
    }
    #[must_use]
    pub const fn once_cases(&self) -> usize {
        self.once_cases
    }
    #[must_use]
    pub const fn continuously_cases(&self) -> usize {
        self.continuously_cases
    }
}

fn durable_borrowed_options(durability: StorageDurabilityPolicy) -> StorageOpenOptions {
    // A faulting backend has no owned handle (it holds a Mutex), so open via the
    // borrowed path: evaluate-and-enqueue maintenance, driven manually.
    StorageOpenOptions::durable_local(durability)
        .with_maintenance_scheduling_policy(StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue)
}

fn case_dir(root: &Path, label: &str) -> Result<PathBuf, TestkitError> {
    let dir = root.join(label);
    std::fs::create_dir_all(&dir)
        .map_err(|err| TestkitError::new(format!("create case dir: {err}")))?;
    Ok(dir)
}

fn swept_op_count(backend: &StorageBackend, op: BackendOperation) -> usize {
    backend
        .fault_calls()
        .iter()
        .filter(|call| call.operation() == op)
        .count()
}

/// Drive `op_count` commits (Always durability, so each commit appends and syncs),
/// recording every acknowledgement; the first commit that fails is recorded
/// in-doubt and stops the workload (later commits would diverge from the model). A
/// forced checkpoint then exercises the publish + sync maintenance ops; a
/// maintenance fault is non-destructive to committed state and tolerated.
fn drive_workload(
    runtime: &mut StorageRuntime<'_>,
    branch: BranchId,
    seed: u64,
    op_count: usize,
    model: &mut ExpectedState,
) -> Result<(), TestkitError> {
    for mutations in &generate_workload(seed, op_count) {
        let batch = CommitBatch::new(
            branch,
            mutations.iter().map(to_commit_mutation).collect(),
            CommitOptions::default(),
        )
        .map_err(|err| TestkitError::new(format!("build batch: {err:?}")))?;
        let Ok(summary) = runtime.commit(&batch) else {
            // Faulted commit: it may or may not survive recovery, atomically.
            model.record_in_doubt(branch, mutations.clone());
            return Ok(());
        };
        model.record_ack(branch, summary.commit_version(), mutations.clone());
    }
    // Force a checkpoint so the durable-table / manifest / snapshot publish and
    // sync ops occur and can be faulted (non-destructive to committed state).
    run_maintenance(runtime, branch, MaintenanceTask::Checkpoint);
    Ok(())
}

/// Enqueue one maintenance task (branch- and global-scoped) and drain it. Both
/// enqueue and run results are intentionally discarded: an unsupported scope is a
/// no-op and an injected fault is a typed, non-destructive error the oracle still
/// accounts for.
fn run_maintenance(runtime: &mut StorageRuntime<'_>, branch: BranchId, task: MaintenanceTask) {
    let _ = runtime.enqueue_maintenance(&MaintenanceRequest::new(
        task,
        MaintenanceScope::Branch(branch),
    ));
    let _ = runtime.enqueue_maintenance(&MaintenanceRequest::new(task, MaintenanceScope::Global));
    let _ = runtime.drain_maintenance();
}

/// Trace every swept backend operation a clean workload invokes, with its highest
/// call number — the set of positions the sweep will then fail.
fn baseline_trace(root: &Path, seed: u64) -> Result<Vec<(BackendOperation, usize)>, TestkitError> {
    let branch = default_branch();
    let backend = StorageBackend::faulting_local_fs(root.to_path_buf(), FaultScript::empty());
    {
        let runtime = StorageRuntime::open_with_backend(
            durable_borrowed_options(StorageDurabilityPolicy::Always),
            &backend,
        )
        .map_err(|err| TestkitError::new(format!("baseline open: {err:?}")))?;
        let mut runtime = runtime.into_runtime();
        let mut model = ExpectedState::new(OracleDurability::Always);
        drive_workload(&mut runtime, branch, seed, SWEEP_OP_COUNT, &mut model)?;
    }
    Ok(SWEEP_OPS
        .into_iter()
        .filter_map(|op| {
            let count = swept_op_count(&backend, op);
            (count > 0).then_some((op, count))
        })
        .collect())
}

/// Result of failing one operation position.
#[derive(Debug)]
struct FaultRun {
    fired: bool,
    violation: Option<RecoveryOracleViolation>,
}

/// Fail the `call_number`-th `op` (in `mode`) during the workload, then reopen
/// cleanly and check recovery against the oracle.
fn run_one_fault(
    root: &Path,
    seed: u64,
    op: BackendOperation,
    call_number: usize,
    mode: FaultMode,
    kind: FaultKind,
) -> Result<FaultRun, TestkitError> {
    let branch = default_branch();
    let mut model = ExpectedState::new(OracleDurability::Always);

    let fired = {
        let nz = u64::try_from(call_number)
            .ok()
            .and_then(std::num::NonZeroU64::new)
            .ok_or_else(|| TestkitError::new("call number must be non-zero"))?;
        let backend = StorageBackend::faulting_local_fs(
            root.to_path_buf(),
            FaultScript::new([FaultRule::with_mode(op, nz, kind, mode)]),
        );
        // The fault may land during open (recovery/init) or during the workload;
        // either way it surfaces as a typed `StorageApiError`, never a panic.
        if let Ok(outcome) = StorageRuntime::open_with_backend(
            durable_borrowed_options(StorageDurabilityPolicy::Always),
            &backend,
        ) {
            let mut runtime = outcome.into_runtime();
            drive_workload(&mut runtime, branch, seed, SWEEP_OP_COUNT, &mut model)?;
        }
        swept_op_count(&backend, op) >= call_number
    };

    // Clean strict reopen: faults fire *before* the inner operation, so no partial
    // write is left behind — the recovered state must be a clean prefix.
    let violation = {
        let backend = StorageBackend::local_fs(root.to_path_buf());
        let runtime = StorageRuntime::open_with_backend(
            durable_borrowed_options(StorageDurabilityPolicy::Standard),
            &backend,
        )
        .map_err(|err| TestkitError::new(format!("verify reopen: {err:?}")))?
        .into_runtime();
        let recovered = scan_recovered(
            &runtime,
            branch,
            &oracle_space(),
            &oracle_prefix_key(),
            SCAN_LIMIT,
        )?;
        classify_recovered(&model, branch, &recovered, CrashFamily::ZeroLoss).err()
    };

    Ok(FaultRun { fired, violation })
}

fn sweep_violation(
    seed: u64,
    op: BackendOperation,
    call_number: usize,
    mode: FaultMode,
    violation: &RecoveryOracleViolation,
) -> TestkitError {
    TestkitError::new(format!(
        "fault sweep violation [seed={seed} op={} call_number={call_number} mode={mode:?}]: {violation:?}",
        op.name()
    ))
}

/// Sweep seeds × every traced backend-op position × {fail-once, fail-continuously},
/// verifying recovery against the oracle at each. `case_limit` caps the number of
/// fault cases (for CI budgets); `None` runs the full grid.
pub fn run_fault_sweep_harness(
    root: &Path,
    case_limit: Option<usize>,
) -> Result<FaultSweepOutcome, TestkitError> {
    let mut outcome = FaultSweepOutcome::default();
    let mut cases = 0usize;
    'outer: for seed in 0..SWEEP_SEED_BUDGET {
        outcome.seeds_executed += 1;
        let trace = baseline_trace(&case_dir(root, &format!("baseline-{seed}"))?, seed)?;
        outcome.baseline_ops_traced += trace.len();
        for (op, count) in trace {
            for mode in [FaultMode::Once, FaultMode::Continuously] {
                for call_number in 1..=count {
                    if case_limit.is_some_and(|limit| cases >= limit) {
                        break 'outer;
                    }
                    let label = format!("{}-{mode:?}-{seed}-{call_number}", op.name());
                    let run = run_one_fault(
                        &case_dir(root, &label)?,
                        seed,
                        op,
                        call_number,
                        mode,
                        FaultKind::Unavailable,
                    )?;
                    if !run.fired {
                        return Err(TestkitError::new(format!(
                            "fault position [seed={seed} op={} call_number={call_number} \
                             mode={mode:?}] never fired; workload is non-deterministic",
                            op.name()
                        )));
                    }
                    if let Some(violation) = run.violation {
                        return Err(sweep_violation(seed, op, call_number, mode, &violation));
                    }
                    match mode {
                        FaultMode::Once => outcome.once_cases += 1,
                        FaultMode::Continuously => outcome.continuously_cases += 1,
                    }
                    outcome.positions_swept += 1;
                    cases += 1;
                }
            }
        }
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::FaultingBackend;
    use std::num::NonZeroU64;

    fn n(value: u64) -> NonZeroU64 {
        NonZeroU64::new(value).expect("non-zero call number")
    }

    #[test]
    fn fault_mode_once_fires_then_disarms() {
        let backend = FaultingBackend::new(
            (),
            FaultScript::new([FaultRule::new(
                BackendOperation::ReadObject,
                n(2),
                FaultKind::Unavailable,
            )]),
        );
        assert!(backend
            .before_operation(BackendOperation::ReadObject)
            .is_ok()); // call 1
        assert!(backend
            .before_operation(BackendOperation::ReadObject)
            .is_err()); // call 2 fires
        assert!(backend
            .before_operation(BackendOperation::ReadObject)
            .is_ok()); // call 3 disarmed
    }

    #[test]
    fn fault_mode_continuously_keeps_firing() {
        let backend = FaultingBackend::new(
            (),
            FaultScript::new([FaultRule::with_mode(
                BackendOperation::ReadObject,
                n(2),
                FaultKind::Unavailable,
                FaultMode::Continuously,
            )]),
        );
        assert!(backend
            .before_operation(BackendOperation::ReadObject)
            .is_ok()); // call 1
        assert!(backend
            .before_operation(BackendOperation::ReadObject)
            .is_err()); // call 2 fires
        assert!(backend
            .before_operation(BackendOperation::ReadObject)
            .is_err()); // call 3 keeps firing
    }

    #[test]
    fn baseline_traces_durable_ops_so_the_sweep_is_not_vacuous() {
        let dir = tempfile::tempdir().expect("tmp");
        let trace = baseline_trace(dir.path(), 1).expect("baseline");
        // Pin the operations the commit + checkpoint workload actually exercises so
        // a change in coverage is visible, not silent. Delete / write / conditional
        // need a richer workload (see SWEEP_OPS) and are intentionally not asserted.
        for expected in [
            BackendOperation::AppendObject,
            BackendOperation::SyncObject,
            BackendOperation::PublishObject,
        ] {
            assert!(
                trace
                    .iter()
                    .any(|(op, count)| *op == expected && *count > 0),
                "baseline did not exercise {}: {trace:?}",
                expected.name()
            );
        }
    }

    #[test]
    fn append_fault_at_first_commit_is_typed_and_loses_nothing() {
        let dir = tempfile::tempdir().expect("tmp");
        // Failing append #1 fails the first commit before any bytes are written;
        // recovery must hold (the in-doubt commit is simply absent).
        let run = run_one_fault(
            dir.path(),
            7,
            BackendOperation::AppendObject,
            1,
            FaultMode::Once,
            FaultKind::Unavailable,
        )
        .expect("run");
        assert!(run.fired, "append #1 fault did not fire");
        assert!(run.violation.is_none(), "recovery oracle violated: {run:?}");
    }

    #[test]
    fn full_sweep_holds_across_all_positions() {
        let dir = tempfile::tempdir().expect("tmp");
        let outcome = run_fault_sweep_harness(dir.path(), Some(64)).expect("sweep");
        assert!(outcome.positions_swept() > 0, "no positions swept");
        assert!(outcome.once_cases() > 0, "no fail-once cases");
        assert!(
            outcome.continuously_cases() > 0,
            "no fail-continuously cases"
        );
    }

    #[test]
    fn disk_full_at_every_write_position_loses_nothing() {
        // The same position sweep, but with a disk-full (NoSpace) error at each
        // write/append/publish position — recovery must still hold.
        let dir = tempfile::tempdir().expect("tmp");
        let trace = baseline_trace(&dir.path().join("trace"), 3).expect("baseline");
        let mut swept = 0usize;
        for (op, count) in trace {
            for call_number in 1..=count {
                let label = format!("nospace-{}-{call_number}", op.name());
                let run = run_one_fault(
                    &dir.path().join(label),
                    3,
                    op,
                    call_number,
                    FaultMode::Once,
                    FaultKind::NoSpace,
                )
                .expect("run");
                assert!(run.fired, "{} #{call_number} did not fire", op.name());
                assert!(
                    run.violation.is_none(),
                    "disk-full at {} #{call_number} lost data: {run:?}",
                    op.name()
                );
                swept += 1;
            }
        }
        assert!(swept > 0, "no write positions swept");
    }

    #[test]
    fn quota_exhaustion_recovers_and_reopen_resumes() {
        let dir = tempfile::tempdir().expect("tmp");
        let branch = default_branch();
        let mut model = ExpectedState::new(OracleDurability::Always);
        let mut hit_disk_full = false;
        {
            // A small write quota: some commits fit, then ENOSPC.
            let backend = StorageBackend::faulting_local_fs_with_quota(dir.path(), 4096);
            let mut runtime = StorageRuntime::open_with_backend(
                durable_borrowed_options(StorageDurabilityPolicy::Always),
                &backend,
            )
            .expect("open within quota")
            .into_runtime();
            for mutations in &generate_workload(20, 32) {
                let batch = CommitBatch::new(
                    branch,
                    mutations.iter().map(to_commit_mutation).collect(),
                    CommitOptions::default(),
                )
                .expect("batch");
                let Ok(summary) = runtime.commit(&batch) else {
                    model.record_in_doubt(branch, mutations.clone());
                    hit_disk_full = true;
                    break;
                };
                model.record_ack(branch, summary.commit_version(), mutations.clone());
            }
        }
        assert!(
            hit_disk_full,
            "quota was never exhausted; lower it or add commits"
        );
        assert!(
            model.last_acked_version(branch).is_some(),
            "no commit fit within the quota"
        );

        // Reopen with space "freed" (no quota): acknowledged commits recover and
        // writes resume.
        let backend = StorageBackend::local_fs(dir.path());
        let mut runtime = StorageRuntime::open_with_backend(
            durable_borrowed_options(StorageDurabilityPolicy::Standard),
            &backend,
        )
        .expect("clean reopen after freeing space")
        .into_runtime();
        let recovered = scan_recovered(
            &runtime,
            branch,
            &oracle_space(),
            &oracle_prefix_key(),
            SCAN_LIMIT,
        )
        .expect("scan");
        assert_eq!(
            classify_recovered(&model, branch, &recovered, CrashFamily::ZeroLoss),
            Ok(())
        );
        let resume = &generate_workload(98, 1)[0];
        let resume_batch = CommitBatch::new(
            branch,
            resume.iter().map(to_commit_mutation).collect(),
            CommitOptions::default(),
        )
        .expect("batch");
        runtime
            .commit(&resume_batch)
            .expect("reopen should resume writes after space is freed");
    }

    #[test]
    fn sync_failures_recover_uncertain_commits_and_reopen_resumes() {
        let dir = tempfile::tempdir().expect("tmp");
        let branch = default_branch();
        let mut model = ExpectedState::new(OracleDurability::Always);
        let ops = generate_workload(5, 2);
        {
            // Always durability: each commit appends then syncs. Failing every sync
            // leaves each commit durability-uncertain — the append lands but the
            // fsync does not — so the engine returns a typed error per commit while
            // the appended record survives in the page cache.
            let backend = StorageBackend::faulting_local_fs(
                dir.path(),
                FaultScript::new([FaultRule::with_mode(
                    BackendOperation::SyncObject,
                    n(1),
                    FaultKind::Unavailable,
                    FaultMode::Continuously,
                )]),
            );
            let mut runtime = StorageRuntime::open_with_backend(
                durable_borrowed_options(StorageDurabilityPolicy::Always),
                &backend,
            )
            .expect("open")
            .into_runtime();
            for mutations in &ops {
                let batch = CommitBatch::new(
                    branch,
                    mutations.iter().map(to_commit_mutation).collect(),
                    CommitOptions::default(),
                )
                .expect("batch");
                runtime
                    .commit(&batch)
                    .expect_err("a failing sync must surface a typed durability error");
                model.record_in_doubt(branch, mutations.clone());
            }
        }

        // Reopen clean: the in-doubt commit is present-or-absent atomically, and
        // writes resume.
        let backend = StorageBackend::local_fs(dir.path());
        let mut runtime = StorageRuntime::open_with_backend(
            durable_borrowed_options(StorageDurabilityPolicy::Standard),
            &backend,
        )
        .expect("clean reopen")
        .into_runtime();
        let recovered = scan_recovered(
            &runtime,
            branch,
            &oracle_space(),
            &oracle_prefix_key(),
            SCAN_LIMIT,
        )
        .expect("scan");
        assert_eq!(
            classify_recovered(&model, branch, &recovered, CrashFamily::ZeroLoss),
            Ok(())
        );
        let resume = CommitBatch::new(
            branch,
            ops[1].iter().map(to_commit_mutation).collect(),
            CommitOptions::default(),
        )
        .expect("batch");
        runtime
            .commit(&resume)
            .expect("reopen should resume writes after a durability fault");
    }
}
