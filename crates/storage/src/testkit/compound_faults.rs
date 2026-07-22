//! STH-5 failure-during-failure (compound anomalies).
//!
//! Breaks recovery *while it is recovering*: stages a first failure (a faulted
//! checkpoint publish followed by a crash), then injects a second fault into
//! the reopen's recovery path — and, separately, injects faults inside the
//! maintenance publish transitions (flush, checkpoint, compaction, snapshot
//! pruning) and proves the interrupted operation resumes. Every case ends with
//! the STH-1 recovery oracle: the recovered database must be exactly a prefix
//! of acknowledged history — no lost acks, no phantoms, no torn batches, no
//! gaps — and the store must accept new writes afterwards. The intermediate
//! failure must always surface as a typed error, never a panic and never
//! silent partial state.

use std::path::{Path, PathBuf};

use strata_core::BranchId;

use super::recovery_oracle::model::{ExpectedState, OracleDurability};
use super::recovery_oracle::verify::{classify_recovered, scan_recovered, CrashFamily};
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

/// Seeds explored when no case budget is given; a case budget scales freely.
const DEFAULT_SWEEP_SEEDS: u64 = 2;
/// Commits staged before the first failure — small keeps each case cheap while
/// still leaving a WAL tail that recovery must replay.
const STAGE_OP_COUNT: usize = 4;
/// Every fault seam the counting backend can fire; the sweep arms whichever of
/// these the traced path actually invoked, so coverage follows the real op
/// stream instead of a hardcoded guess.
const ALL_OPERATIONS: [BackendOperation; 11] = [
    BackendOperation::ReadObject,
    BackendOperation::ReadRange,
    BackendOperation::WriteObject,
    BackendOperation::DeleteObject,
    BackendOperation::ListPrefix,
    BackendOperation::ObjectMetadata,
    BackendOperation::AppendObject,
    BackendOperation::SyncObject,
    BackendOperation::ConditionalCreate,
    BackendOperation::ConditionalUpdate,
    BackendOperation::PublishObject,
];
/// The write-side operations maintenance publish transitions go through.
const MAINTENANCE_OPS: [BackendOperation; 5] = [
    BackendOperation::WriteObject,
    BackendOperation::DeleteObject,
    BackendOperation::AppendObject,
    BackendOperation::SyncObject,
    BackendOperation::PublishObject,
];
/// The maintenance sequence driven by the compound maintenance cases, in
/// order. Flush publishes tables, checkpoint publishes a snapshot, compaction
/// rewrites tables, pruning deletes the superseded snapshot.
const MAINTENANCE_SEQUENCE: [MaintenanceTask; 4] = [
    MaintenanceTask::Flush,
    MaintenanceTask::Checkpoint,
    MaintenanceTask::Compact,
    MaintenanceTask::SnapshotPruning,
];

/// Counters describing a compound fault-during-recovery sweep.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompoundFaultRecoveryOutcome {
    seeds_executed: usize,
    staged_crashes: usize,
    recovery_ops_traced: usize,
    positions_swept: usize,
    faulted_opens_failed_typed: usize,
    faulted_opens_succeeded: usize,
    resumes_verified: usize,
}

impl CompoundFaultRecoveryOutcome {
    #[must_use]
    pub const fn seeds_executed(&self) -> usize {
        self.seeds_executed
    }
    #[must_use]
    pub const fn staged_crashes(&self) -> usize {
        self.staged_crashes
    }
    #[must_use]
    pub const fn recovery_ops_traced(&self) -> usize {
        self.recovery_ops_traced
    }
    #[must_use]
    pub const fn positions_swept(&self) -> usize {
        self.positions_swept
    }
    #[must_use]
    pub const fn faulted_opens_failed_typed(&self) -> usize {
        self.faulted_opens_failed_typed
    }
    #[must_use]
    pub const fn faulted_opens_succeeded(&self) -> usize {
        self.faulted_opens_succeeded
    }
    #[must_use]
    pub const fn resumes_verified(&self) -> usize {
        self.resumes_verified
    }
}

/// Counters describing the fault-during-maintenance cases.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompoundFaultMaintenanceOutcome {
    positions_swept: usize,
    faults_surfaced_typed: usize,
    in_session_resumes: usize,
    reopen_resumes: usize,
}

impl CompoundFaultMaintenanceOutcome {
    #[must_use]
    pub const fn positions_swept(&self) -> usize {
        self.positions_swept
    }
    #[must_use]
    pub const fn faults_surfaced_typed(&self) -> usize {
        self.faults_surfaced_typed
    }
    #[must_use]
    pub const fn in_session_resumes(&self) -> usize {
        self.in_session_resumes
    }
    #[must_use]
    pub const fn reopen_resumes(&self) -> usize {
        self.reopen_resumes
    }
}

fn durable_options(durability: StorageDurabilityPolicy) -> StorageOpenOptions {
    StorageOpenOptions::durable_local(durability)
        .with_maintenance_scheduling_policy(StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue)
}

fn testkit_err(context: &str, err: impl std::fmt::Debug) -> TestkitError {
    TestkitError::new(format!("{context}: {err:?}"))
}

fn case_dir(root: &Path, label: &str) -> Result<PathBuf, TestkitError> {
    let dir = root.join(label);
    std::fs::create_dir_all(&dir).map_err(|err| testkit_err("create case dir", err))?;
    Ok(dir)
}

/// Byte-for-byte copy of a staged (crashed) store so each swept case reopens
/// the *same* crashed state — a successful reopen heals the directory, so
/// cases can never share one.
fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), TestkitError> {
    std::fs::create_dir_all(destination).map_err(|err| testkit_err("copy mkdir", err))?;
    for entry in std::fs::read_dir(source).map_err(|err| testkit_err("copy read_dir", err))? {
        let entry = entry.map_err(|err| testkit_err("copy entry", err))?;
        let target = destination.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|err| testkit_err("copy file_type", err))?;
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target).map_err(|err| testkit_err("copy file", err))?;
        }
    }
    Ok(())
}

fn op_count(backend: &StorageBackend, op: BackendOperation) -> usize {
    backend
        .fault_calls()
        .iter()
        .filter(|call| call.operation() == op)
        .count()
}

fn fault_rule(
    op: BackendOperation,
    call_number: usize,
    mode: FaultMode,
) -> Result<FaultRule, TestkitError> {
    let position = u64::try_from(call_number)
        .ok()
        .and_then(std::num::NonZeroU64::new)
        .ok_or_else(|| TestkitError::new("fault call number must be non-zero"))?;
    Ok(FaultRule::with_mode(
        op,
        position,
        FaultKind::Unavailable,
        mode,
    ))
}

fn commit_all(
    runtime: &StorageRuntime<'_>,
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
        .map_err(|err| testkit_err("build batch", err))?;
        let summary = runtime
            .commit(&batch)
            .map_err(|err| testkit_err("staging commit must acknowledge", err))?;
        model.record_ack(branch, summary.commit_version(), mutations.clone());
    }
    Ok(())
}

/// How a maintenance fault surfaced. All these channels are typed and
/// contract-legal: a drain error propagates a `StorageApiError`; a task
/// summary reports `Failed` with a source error code; or the pass completes
/// while *recording* the sub-failure as a source error code (the
/// manifest-publish-debt path: the failed publish becomes tracked debt,
/// dependent checkpoints defer, and a later flush republishes). `Silent` — a
/// fault that fired but left no typed trace anywhere — is the bug this
/// harness exists to catch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MaintenanceFaultSignal {
    DrainError,
    TaskReported,
    Silent,
}

fn enqueue_tasks(runtime: &StorageRuntime<'_>, branch: BranchId, task: MaintenanceTask) {
    // Enqueue results are discarded on purpose: an unsupported scope for a
    // task is a no-op, and the drain surfaces any injected fault.
    let _ = runtime.enqueue_maintenance(&MaintenanceRequest::new(
        task,
        MaintenanceScope::Branch(branch),
    ));
    let _ = runtime.enqueue_maintenance(&MaintenanceRequest::new(task, MaintenanceScope::Global));
}

fn enqueue_and_drain(
    runtime: &mut StorageRuntime<'_>,
    branch: BranchId,
    tasks: &[MaintenanceTask],
) -> Result<(), crate::api::StorageApiError> {
    for task in tasks {
        enqueue_tasks(runtime, branch, *task);
    }
    runtime.drain_maintenance().map(|_| ())
}

/// Drain the standard maintenance sequence and classify how (whether) a fault
/// surfaced. An absorbed failure without a typed source error code is a
/// harness-level assertion failure, not a signal.
fn drain_sequence_signal(
    runtime: &mut StorageRuntime<'_>,
    branch: BranchId,
    tasks: &[MaintenanceTask],
) -> Result<MaintenanceFaultSignal, TestkitError> {
    for &task in tasks {
        enqueue_tasks(runtime, branch, task);
        let summary = match runtime.drain_maintenance() {
            Ok(summary) => summary,
            Err(err) => {
                if err.code().is_empty() {
                    return Err(TestkitError::new(format!(
                        "maintenance drain failed without a code: {err:?}"
                    )));
                }
                return Ok(MaintenanceFaultSignal::DrainError);
            }
        };
        for outcome in summary.outcomes() {
            let failed = outcome.status() == crate::api::MaintenanceSummaryStatus::Failed;
            let recorded = outcome.source_error_code().is_some();
            if failed && !recorded {
                return Err(TestkitError::new(format!(
                    "maintenance task {:?} failed without a typed source error \
                     code — an untyped absorbed failure",
                    outcome.task()
                )));
            }
            if failed || recorded {
                return Ok(MaintenanceFaultSignal::TaskReported);
            }
        }
    }
    Ok(MaintenanceFaultSignal::Silent)
}

fn verify_recovered_and_resume(
    root: &Path,
    branch: BranchId,
    model: &ExpectedState,
    context: &str,
) -> Result<(), TestkitError> {
    let backend = StorageBackend::local_fs(root.to_path_buf());
    // Retry-on-Unavailable absorbs the prior runtime's detached-worker
    // writer-lock window (#2720 — the resume leg reopens the same copied
    // store its healing runtime just dropped); any other failure stays loud.
    let runtime = crate::testkit::reopen_retry::open_with_retry_on_unavailable(|| {
        StorageRuntime::open_with_backend(
            durable_options(StorageDurabilityPolicy::Standard),
            &backend,
        )
    })
    .map_err(|err| testkit_err(&format!("{context}: clean reopen must succeed"), err))?
    .into_runtime();
    let recovered = scan_recovered(
        &runtime,
        branch,
        &oracle_space(),
        &oracle_prefix_key(),
        SCAN_LIMIT,
    )?;
    if let Err(violation) = classify_recovered(model, branch, &recovered, CrashFamily::ZeroLoss) {
        return Err(TestkitError::new(format!(
            "{context}: recovery oracle violation: {violation:?}"
        )));
    }
    // Resumability is part of the contract: the store must accept new writes
    // after the compound event, not merely read back clean.
    let resume = &generate_workload(u64::MAX, 1)[0];
    let batch = CommitBatch::new(
        branch,
        resume.iter().map(to_commit_mutation).collect(),
        CommitOptions::default(),
    )
    .map_err(|err| testkit_err("build resume batch", err))?;
    runtime
        .commit(&batch)
        .map_err(|err| testkit_err(&format!("{context}: resume commit must succeed"), err))?;
    Ok(())
}

/// Count publishes a clean staging run performs before its checkpoint, so the
/// staged first failure can be armed at the checkpoint-publish position rather
/// than the store-creation manifest publish.
fn trace_publishes_before_checkpoint(root: &Path, seed: u64) -> Result<usize, TestkitError> {
    let branch = default_branch();
    let backend = StorageBackend::faulting_local_fs(root.to_path_buf(), FaultScript::empty());
    let runtime = StorageRuntime::open_with_backend(
        durable_options(StorageDurabilityPolicy::Always),
        &backend,
    )
    .map_err(|err| testkit_err("staging trace open", err))?
    .into_runtime();
    let mut model = ExpectedState::new(OracleDurability::Always);
    commit_all(&runtime, branch, seed, STAGE_OP_COUNT, &mut model)?;
    let before_checkpoint = op_count(&backend, BackendOperation::PublishObject);
    let mut runtime = runtime;
    enqueue_and_drain(&mut runtime, branch, &[MaintenanceTask::Checkpoint])
        .map_err(|err| testkit_err("staging trace checkpoint", err))?;
    let after_checkpoint = op_count(&backend, BackendOperation::PublishObject);
    if after_checkpoint <= before_checkpoint {
        return Err(TestkitError::new(
            "staging trace: checkpoint published nothing; the first failure cannot be staged",
        ));
    }
    Ok(before_checkpoint)
}

/// Stage the first failure: drive acknowledged commits, fault the checkpoint
/// publish (typed maintenance error), then crash without closing. The returned
/// model is the acknowledged history the compound recovery must preserve.
fn stage_crashed_store(
    root: &Path,
    scratch: &Path,
    seed: u64,
) -> Result<ExpectedState, TestkitError> {
    let publishes_before = trace_publishes_before_checkpoint(scratch, seed)?;
    let branch = default_branch();
    let mut model = ExpectedState::new(OracleDurability::Always);
    let backend = StorageBackend::faulting_local_fs(
        root.to_path_buf(),
        FaultScript::new([fault_rule(
            BackendOperation::PublishObject,
            publishes_before + 1,
            FaultMode::Once,
        )?]),
    );
    let mut runtime = StorageRuntime::open_with_backend(
        durable_options(StorageDurabilityPolicy::Always),
        &backend,
    )
    .map_err(|err| testkit_err("staging open", err))?
    .into_runtime();
    commit_all(&runtime, branch, seed, STAGE_OP_COUNT, &mut model)?;
    let signal = drain_sequence_signal(&mut runtime, branch, &[MaintenanceTask::Checkpoint])?;
    if signal == MaintenanceFaultSignal::Silent {
        return Err(TestkitError::new(
            "staged checkpoint fault left no typed trace; first failure was not staged",
        ));
    }
    if op_count(&backend, BackendOperation::PublishObject) <= publishes_before {
        return Err(TestkitError::new(
            "staged checkpoint fault never fired; staging is non-deterministic",
        ));
    }
    // Crash: scope drop without close. Under Always durability every
    // acknowledged commit is synced, so recovery owes the full model.
    Ok(model)
}

/// Trace which backend operations the reopen of the crashed store performs —
/// the positions the second fault will be swept across.
fn trace_recovery_ops(
    crashed: &Path,
    scratch: &Path,
) -> Result<Vec<(BackendOperation, usize)>, TestkitError> {
    let baseline = case_dir(scratch, "recovery-trace")?;
    copy_dir_recursive(crashed, &baseline)?;
    let backend = StorageBackend::faulting_local_fs(baseline, FaultScript::empty());
    let outcome = StorageRuntime::open_with_backend(
        durable_options(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .map_err(|err| testkit_err("recovery trace reopen", err))?;
    drop(outcome);
    Ok(ALL_OPERATIONS
        .into_iter()
        .filter_map(|op| {
            let count = op_count(&backend, op);
            (count > 0).then_some((op, count))
        })
        .collect())
}

struct SecondFaultCase {
    fired: bool,
    open_failed_typed: bool,
}

/// Reopen a copy of the crashed store with the second fault armed. The open
/// must either fail with a typed error or succeed; either way a clean reopen
/// afterwards must be oracle-valid and accept writes.
fn run_second_fault_case(
    crashed: &Path,
    case_root: &Path,
    model: &ExpectedState,
    op: BackendOperation,
    call_number: usize,
    mode: FaultMode,
) -> Result<SecondFaultCase, TestkitError> {
    copy_dir_recursive(crashed, case_root)?;
    let branch = default_branch();
    let (fired, open_failed_typed) = {
        let backend = StorageBackend::faulting_local_fs(
            case_root.to_path_buf(),
            FaultScript::new([fault_rule(op, call_number, mode)?]),
        );
        let open_failed_typed = match StorageRuntime::open_with_backend(
            durable_options(StorageDurabilityPolicy::Standard),
            &backend,
        ) {
            // A tolerated fault (fallback path) must still recover correctly.
            Ok(outcome) => {
                let runtime = outcome.into_runtime();
                let recovered = scan_recovered(
                    &runtime,
                    branch,
                    &oracle_space(),
                    &oracle_prefix_key(),
                    SCAN_LIMIT,
                )?;
                if let Err(violation) =
                    classify_recovered(model, branch, &recovered, CrashFamily::ZeroLoss)
                {
                    return Err(TestkitError::new(format!(
                        "open tolerated the fault but recovered wrong state \
                         [op={} call={call_number} mode={mode:?}]: {violation:?}",
                        op.name()
                    )));
                }
                false
            }
            // The typed-error contract: a code is always present; reaching
            // this arm at all proves no panic and no abort.
            Err(err) => {
                if err.code().is_empty() {
                    return Err(TestkitError::new(format!(
                        "recovery fault produced an error without a code: {err:?}"
                    )));
                }
                true
            }
        };
        (op_count(&backend, op) >= call_number, open_failed_typed)
    };
    verify_recovered_and_resume(
        case_root,
        branch,
        model,
        &format!(
            "after second fault [op={} call={call_number} mode={mode:?}]",
            op.name()
        ),
    )?;
    Ok(SecondFaultCase {
        fired,
        open_failed_typed,
    })
}

/// The compound fault-during-recovery sweep: stage a crash whose checkpoint
/// already failed, then sweep a second fault across every backend operation
/// the recovery path performs — once and continuously — verifying the oracle
/// and resumability after every case.
pub fn run_compound_fault_recovery_sweep(
    root: &Path,
    case_limit: Option<usize>,
) -> Result<CompoundFaultRecoveryOutcome, TestkitError> {
    let mut outcome = CompoundFaultRecoveryOutcome::default();
    let mut cases = 0usize;
    let seed_budget = if case_limit.is_some() {
        u64::MAX
    } else {
        DEFAULT_SWEEP_SEEDS
    };
    'outer: for seed in 0..seed_budget {
        if case_limit.is_some_and(|limit| cases >= limit) {
            break;
        }
        outcome.seeds_executed += 1;
        let seed_root = case_dir(root, &format!("seed-{seed}"))?;
        let crashed = case_dir(&seed_root, "crashed")?;
        let scratch = case_dir(&seed_root, "scratch")?;
        let model = stage_crashed_store(&crashed, &scratch, seed)?;
        outcome.staged_crashes += 1;
        let trace = trace_recovery_ops(&crashed, &scratch)?;
        if trace.is_empty() {
            return Err(TestkitError::new(
                "recovery trace is empty; the sweep would be vacuous",
            ));
        }
        outcome.recovery_ops_traced += trace.len();
        for (op, count) in trace {
            for mode in [FaultMode::Once, FaultMode::Continuously] {
                for call_number in 1..=count {
                    if case_limit.is_some_and(|limit| cases >= limit) {
                        break 'outer;
                    }
                    let label = format!("{}-{mode:?}-{call_number}", op.name());
                    let case = run_second_fault_case(
                        &crashed,
                        &case_dir(&seed_root, &label)?,
                        &model,
                        op,
                        call_number,
                        mode,
                    )
                    .map_err(|err| {
                        TestkitError::new(format!(
                            "compound recovery case [seed={seed} op={} call={call_number} \
                             mode={mode:?}]: {err}",
                            op.name()
                        ))
                    })?;
                    if !case.fired {
                        return Err(TestkitError::new(format!(
                            "second fault never fired [seed={seed} op={} call={call_number} \
                             mode={mode:?}]; recovery is non-deterministic",
                            op.name()
                        )));
                    }
                    if case.open_failed_typed {
                        outcome.faulted_opens_failed_typed += 1;
                    } else {
                        outcome.faulted_opens_succeeded += 1;
                    }
                    outcome.resumes_verified += 1;
                    outcome.positions_swept += 1;
                    cases += 1;
                }
            }
        }
    }
    Ok(outcome)
}

/// Per-op call-number windows for each maintenance task, measured on a clean
/// baseline run so the fault cases can arm positions that land *inside* a
/// specific maintenance transition.
fn maintenance_windows(
    root: &Path,
    seed: u64,
) -> Result<Vec<(MaintenanceTask, BackendOperation, usize, usize)>, TestkitError> {
    let branch = default_branch();
    let backend = StorageBackend::faulting_local_fs(root.to_path_buf(), FaultScript::empty());
    let mut runtime = StorageRuntime::open_with_backend(
        durable_options(StorageDurabilityPolicy::Always),
        &backend,
    )
    .map_err(|err| testkit_err("maintenance baseline open", err))?
    .into_runtime();
    let mut model = ExpectedState::new(OracleDurability::Always);
    commit_all(&runtime, branch, seed, STAGE_OP_COUNT, &mut model)?;
    let mut windows = Vec::new();
    let mut cursor: Vec<(BackendOperation, usize)> = MAINTENANCE_OPS
        .into_iter()
        .map(|op| (op, op_count(&backend, op)))
        .collect();
    for task in MAINTENANCE_SEQUENCE {
        enqueue_and_drain(&mut runtime, branch, &[task])
            .map_err(|err| testkit_err("maintenance baseline drain", err))?;
        for (op, seen) in &mut cursor {
            let now = op_count(&backend, *op);
            if now > *seen {
                windows.push((task, *op, *seen + 1, now));
            }
            *seen = now;
        }
    }
    Ok(windows)
}

/// Fault every write-side backend position inside each maintenance publish
/// transition; the fault must surface typed, the interrupted maintenance must
/// resume (in-session where the runtime allows it, via reopen otherwise), and
/// committed state must survive untouched.
pub fn run_compound_fault_maintenance_cases(
    root: &Path,
    seed: u64,
) -> Result<CompoundFaultMaintenanceOutcome, TestkitError> {
    let windows = maintenance_windows(&case_dir(root, "baseline")?, seed)?;
    if windows.is_empty() {
        return Err(TestkitError::new(
            "no maintenance windows traced; the maintenance cases would be vacuous",
        ));
    }
    let mut outcome = CompoundFaultMaintenanceOutcome::default();
    for (task, op, first, last) in windows {
        for call_number in first..=last {
            let label = format!("{task:?}-{}-{call_number}", op.name());
            let dir = case_dir(root, &label)?;
            let branch = default_branch();
            let mut model = ExpectedState::new(OracleDurability::Always);
            {
                let backend = StorageBackend::faulting_local_fs(
                    dir.clone(),
                    FaultScript::new([fault_rule(op, call_number, FaultMode::Once)?]),
                );
                let mut runtime = StorageRuntime::open_with_backend(
                    durable_options(StorageDurabilityPolicy::Always),
                    &backend,
                )
                .map_err(|err| testkit_err("maintenance case open", err))?
                .into_runtime();
                commit_all(&runtime, branch, seed, STAGE_OP_COUNT, &mut model)?;
                let signal = drain_sequence_signal(&mut runtime, branch, &MAINTENANCE_SEQUENCE)?;
                let fired = op_count(&backend, op) >= call_number;
                if signal == MaintenanceFaultSignal::Silent {
                    // Distinguish a harness bug (the armed position was never
                    // reached) from the product bug this class exists to
                    // catch (the fault fired and left no typed trace).
                    return Err(TestkitError::new(if fired {
                        format!(
                            "SILENT ABSORBED FAILURE [{label}]: the fault fired \
                             but neither the drain nor any task summary \
                             reported it"
                        )
                    } else {
                        format!(
                            "maintenance fault [{label}] never fired \
                             (reached {} of {call_number} calls); the baseline \
                             trace diverged",
                            op_count(&backend, op)
                        )
                    }));
                }
                outcome.faults_surfaced_typed += 1;
                if !fired {
                    return Err(TestkitError::new(format!(
                        "maintenance fault [{label}] surfaced without firing; \
                         another fault seam is interfering"
                    )));
                }
                // Resume in-session: the fault is disarmed (Once), so a fresh
                // drain of the same sequence must either complete cleanly or
                // the runtime has entered a typed halted state that only a
                // reopen clears — both are contract-legal; record which.
                match drain_sequence_signal(&mut runtime, branch, &MAINTENANCE_SEQUENCE)? {
                    MaintenanceFaultSignal::Silent => outcome.in_session_resumes += 1,
                    MaintenanceFaultSignal::DrainError | MaintenanceFaultSignal::TaskReported => {
                        outcome.reopen_resumes += 1;
                    }
                }
                // Crash without close: recovery must not depend on a clean
                // shutdown after an interrupted maintenance pass.
            }
            verify_recovered_and_resume(
                &dir,
                branch,
                &model,
                &format!("after maintenance fault [{label}] (staged during {task:?})"),
            )?;
            outcome.positions_swept += 1;
        }
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_resume_verifier_detects_a_fabricated_lost_ack() {
        // Sabotage: the verifier must actually verify. An empty (validly
        // created) store checked against a model claiming an acknowledged
        // commit must fail with an oracle violation — a no-op verifier
        // (`Ok(())`) passes every counter assertion in this suite and would
        // only be caught here.
        let dir = tempfile::tempdir().expect("tmp");
        let branch = default_branch();
        {
            let backend = StorageBackend::local_fs(dir.path().to_path_buf());
            let _runtime = StorageRuntime::open_with_backend(
                durable_options(StorageDurabilityPolicy::Standard),
                &backend,
            )
            .expect("create store")
            .into_runtime();
        }
        let mut model = ExpectedState::new(OracleDurability::Always);
        model.record_ack(
            branch,
            strata_core::CommitVersion::new(1),
            generate_workload(99, 1)[0].clone(),
        );
        let err = verify_recovered_and_resume(dir.path(), branch, &model, "sabotage")
            .expect_err("a phantom acked commit must be detected");
        let message = format!("{err:?}");
        assert!(
            message.contains("recovery oracle violation"),
            "expected an oracle violation, got: {message}"
        );
    }

    #[test]
    fn staged_crash_recovers_oracle_valid_without_a_second_fault() {
        let dir = tempfile::tempdir().expect("tmp");
        let crashed = case_dir(dir.path(), "crashed").expect("dir");
        let scratch = case_dir(dir.path(), "scratch").expect("dir");
        let model = stage_crashed_store(&crashed, &scratch, 11).expect("stage");
        verify_recovered_and_resume(
            &crashed,
            default_branch(),
            &model,
            "staged crash without second fault",
        )
        .expect("staged crash must recover clean");
    }

    #[test]
    fn recovery_trace_is_not_vacuous() {
        let dir = tempfile::tempdir().expect("tmp");
        let crashed = case_dir(dir.path(), "crashed").expect("dir");
        let scratch = case_dir(dir.path(), "scratch").expect("dir");
        stage_crashed_store(&crashed, &scratch, 3).expect("stage");
        let trace = trace_recovery_ops(&crashed, &scratch).expect("trace");
        assert!(
            trace
                .iter()
                .any(|(op, _)| *op == BackendOperation::ReadObject),
            "recovery never read an object: {trace:?}"
        );
    }

    #[test]
    fn first_recovery_read_fault_is_typed_and_resumable() {
        let dir = tempfile::tempdir().expect("tmp");
        let crashed = case_dir(dir.path(), "crashed").expect("dir");
        let scratch = case_dir(dir.path(), "scratch").expect("dir");
        let model = stage_crashed_store(&crashed, &scratch, 5).expect("stage");
        let case = run_second_fault_case(
            &crashed,
            &case_dir(dir.path(), "case").expect("dir"),
            &model,
            BackendOperation::ReadObject,
            1,
            FaultMode::Continuously,
        )
        .expect("case");
        assert!(case.fired, "read fault never fired during recovery");
        assert!(
            case.open_failed_typed,
            "a continuously failing first read should fail the open"
        );
    }

    #[test]
    fn bounded_compound_recovery_sweep_holds() {
        let dir = tempfile::tempdir().expect("tmp");
        let outcome =
            run_compound_fault_recovery_sweep(dir.path(), Some(24)).expect("compound sweep");
        assert!(outcome.staged_crashes() > 0, "no crashes staged");
        assert!(outcome.positions_swept() > 0, "no positions swept");
        assert!(
            outcome.faulted_opens_failed_typed() > 0,
            "no recovery fault ever failed an open; the sweep is vacuous"
        );
        assert_eq!(
            outcome.resumes_verified(),
            outcome.positions_swept(),
            "every case must verify resumability"
        );
    }

    #[test]
    fn maintenance_publish_faults_surface_typed_and_resume() {
        let dir = tempfile::tempdir().expect("tmp");
        let outcome =
            run_compound_fault_maintenance_cases(dir.path(), 9).expect("maintenance cases");
        assert!(outcome.positions_swept() > 0, "no positions swept");
        assert_eq!(
            outcome.faults_surfaced_typed(),
            outcome.positions_swept(),
            "every maintenance fault must surface typed"
        );
        assert!(
            outcome.in_session_resumes() + outcome.reopen_resumes() == outcome.positions_swept(),
            "every case must resume one way or the other"
        );
    }
}
