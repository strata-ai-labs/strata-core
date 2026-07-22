//! The seeded simulation run: one trajectory over the production lifecycle path.
//!
//! Opens a durable runtime on an owned backend under `DeterministicInline` (the
//! production `Background` path with the inline executor + manual clock, no worker
//! threads), then drives a seeded action sequence — client commits, explicit
//! maintenance requests drained at a sim-chosen cadence, and seeded clock
//! advancement. After every step the live database must still equal the model's
//! acknowledged state (the recovery oracle under `ZeroLoss`), and maintenance must
//! make progress (liveness). The whole trajectory is a pure function of the seed,
//! so it replays bit-exact.
//!
//! Under `DeterministicInline` the inline executor owns the maintenance queue, so
//! enqueued work is driven by [`drain_maintenance`](StorageRuntime::drain_maintenance)
//! (not `run_next_maintenance`, which serves the empty manual queue here). The
//! interleaving knob is therefore *when* enqueued maintenance is drained relative to
//! client commits, crossed with seeded clock advancement.

use std::path::Path;
use std::time::Duration;

use strata_core::BranchId;

use crate::api::{
    CommitBatch, CommitOptions, DiagnosticsRequest, DiagnosticsScope,
    DiagnosticsStoragePressureSeverity, MaintenanceRequest, MaintenanceScope, MaintenanceTask,
    StorageApiError, StorageBackend, StorageDurabilityPolicy, StorageMaintenanceSchedulingPolicy,
    StorageOpenOptions, StorageRuntime,
};
use crate::testkit::recovery_oracle::model::{ExpectedState, OracleDurability, RecordedMutation};
use crate::testkit::recovery_oracle::verify::{
    classify_recovered, scan_recovered, CrashFamily, RecoveredState,
};
use crate::testkit::recovery_oracle::workload::{
    default_branch, generate_workload, oracle_prefix_key, oracle_space, to_commit_mutation,
    SCAN_LIMIT,
};
use crate::testkit::rng::SplitMix64;
use crate::testkit::TestkitError;

/// Decorrelate the action stream from the workload stream (both seeded from `seed`).
const ACTION_SALT: u64 = 0x5347_5f34_6472_6976;
/// Max seeded clock jitter per `AdvanceClock`, in milliseconds.
const MAX_JITTER_MS: u8 = 50;

/// One seeded simulation action against the runtime.
#[derive(Clone, Copy, Debug)]
enum SimAction {
    /// Apply the next workload commit (the client write load).
    Commit,
    /// Let enqueued maintenance debt accumulate (run nothing this step).
    MaintainNone,
    /// Drain all pending maintenance (runs the inline executor to quiescence).
    DrainMaintenance,
    /// Request a branch flush.
    EnqueueFlush,
    /// Request a branch checkpoint.
    EnqueueCheckpoint,
    /// Request global snapshot pruning.
    EnqueueSnapshotPruning,
    /// Advance the manual maintenance clock by a seeded jitter (ms).
    AdvanceClock(u64),
}

impl SimAction {
    fn label(self) -> &'static str {
        match self {
            SimAction::Commit => "commit",
            SimAction::MaintainNone => "maintain_none",
            SimAction::DrainMaintenance => "drain_maintenance",
            SimAction::EnqueueFlush => "enqueue_flush",
            SimAction::EnqueueCheckpoint => "enqueue_checkpoint",
            SimAction::EnqueueSnapshotPruning => "enqueue_snapshot_pruning",
            SimAction::AdvanceClock(_) => "advance_clock",
        }
    }
}

fn draw_action(rng: &mut SplitMix64) -> SimAction {
    match rng.gen_u8_below(8) {
        // Bias toward commits so there is real write load to maintain against.
        0 | 1 => SimAction::Commit,
        2 => SimAction::MaintainNone,
        3 => SimAction::DrainMaintenance,
        4 => SimAction::EnqueueFlush,
        5 => SimAction::EnqueueCheckpoint,
        6 => SimAction::EnqueueSnapshotPruning,
        _ => SimAction::AdvanceClock(u64::from(rng.gen_u8_below(MAX_JITTER_MS))),
    }
}

/// Deterministic facts captured over one simulation run. Contains only state +
/// sequencing facts — never wall-clock timing numbers — so two runs of the same
/// seed produce a bit-identical value (the replay guarantee).
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SimFacts {
    action_trace: Vec<&'static str>,
    commit_versions: Vec<u64>,
    queue_trajectory: Vec<usize>,
    maintenance_completed: usize,
    pressure_retries: usize,
    clock_advances: usize,
    final_pending_tasks: usize,
    final_owned_l0_tables: usize,
    final_live_state: RecoveredState,
}

impl SimFacts {
    pub(super) fn steps(&self) -> usize {
        self.action_trace.len()
    }
    pub(super) fn maintenance_completed(&self) -> usize {
        self.maintenance_completed
    }
    pub(super) fn pressure_retries(&self) -> usize {
        self.pressure_retries
    }
    pub(super) fn clock_advances(&self) -> usize {
        self.clock_advances
    }
}

/// In-flight state of one simulation run, bound to the open runtime.
struct SimRun<'a> {
    runtime: StorageRuntime<'a>,
    branch: BranchId,
    model: ExpectedState,
    facts: SimFacts,
    workload: Vec<Vec<RecordedMutation>>,
    commit_index: usize,
    seed: u64,
}

impl<'a> SimRun<'a> {
    /// Open the durable runtime under deterministic-inline on the owned backend.
    fn open(
        backend: &'a StorageBackend,
        seed: u64,
        step_budget: usize,
    ) -> Result<Self, TestkitError> {
        // Retry-on-Unavailable absorbs the prior runtime's detached-worker
        // writer-lock window (#2727); any other failure stays loud.
        let runtime = crate::testkit::reopen_retry::open_with_retry_on_unavailable(|| {
            StorageRuntime::open_with_backend(
                StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
                    .with_maintenance_scheduling_policy(
                        StorageMaintenanceSchedulingPolicy::DeterministicInline,
                    ),
                backend,
            )
        })
        .map_err(|err| TestkitError::new(format!("[seed={seed}] sim open: {err:?}")))?
        .into_runtime();

        let run = Self {
            runtime,
            branch: default_branch(),
            model: ExpectedState::new(OracleDurability::Standard),
            facts: SimFacts::default(),
            workload: generate_workload(seed, step_budget.max(1)),
            commit_index: 0,
            seed,
        };
        // Prove the deterministic-inline runtime exposes a manual clock the sim can
        // drive; a `false` here means the seam is not wired and the run is invalid.
        if !run
            .runtime
            .advance_maintenance_clock_for_test(Duration::ZERO)
        {
            return Err(TestkitError::new(format!(
                "[seed={seed}] deterministic-inline runtime exposes no manual maintenance clock"
            )));
        }
        Ok(run)
    }

    fn apply(&mut self, action: SimAction, step: usize) -> Result<(), TestkitError> {
        self.facts.action_trace.push(action.label());
        match action {
            SimAction::Commit => self.apply_commit(step)?,
            SimAction::MaintainNone => {}
            SimAction::DrainMaintenance => {
                self.runtime
                    .drain_maintenance()
                    .map_err(|err| self.error(step, format!("drain_maintenance: {err:?}")))?;
            }
            SimAction::EnqueueFlush => self.enqueue(
                MaintenanceTask::Flush,
                MaintenanceScope::Branch(self.branch),
            ),
            SimAction::EnqueueCheckpoint => {
                self.enqueue(
                    MaintenanceTask::Checkpoint,
                    MaintenanceScope::Branch(self.branch),
                );
            }
            SimAction::EnqueueSnapshotPruning => {
                self.enqueue(MaintenanceTask::SnapshotPruning, MaintenanceScope::Global);
            }
            SimAction::AdvanceClock(ms) => {
                if self
                    .runtime
                    .advance_maintenance_clock_for_test(Duration::from_millis(ms))
                {
                    self.facts.clock_advances += 1;
                }
            }
        }
        Ok(())
    }

    /// Enqueue a maintenance request; an unsupported scope is a no-op the engine
    /// rejects, which is fine — the drain cadence is what the sim exercises.
    fn enqueue(&mut self, task: MaintenanceTask, scope: MaintenanceScope) {
        let _ = self
            .runtime
            .enqueue_maintenance(&MaintenanceRequest::new(task, scope));
    }

    fn apply_commit(&mut self, step: usize) -> Result<(), TestkitError> {
        if self.commit_index >= self.workload.len() {
            return Ok(());
        }
        let mutations = self.workload[self.commit_index].clone();
        self.commit_index += 1;
        let batch = CommitBatch::new(
            self.branch,
            mutations.iter().map(to_commit_mutation).collect(),
            CommitOptions::default(),
        )
        .map_err(|err| self.error(step, format!("build batch: {err:?}")))?;
        match self.runtime.commit(&batch) {
            Ok(summary) => {
                self.facts
                    .commit_versions
                    .push(summary.commit_version().as_u64());
                self.model
                    .record_ack(self.branch, summary.commit_version(), mutations);
            }
            Err(StorageApiError::StoragePressure { retryable, .. }) => {
                if !retryable {
                    return Err(self.error(step, "storage pressure rejection was not retryable"));
                }
                // Liveness: draining maintenance must let the same commit through.
                self.runtime
                    .drain_maintenance()
                    .map_err(|err| self.error(step, format!("drain after pressure: {err:?}")))?;
                let summary = self.runtime.commit(&batch).map_err(|err| {
                    self.error(step, format!("resume commit after drain: {err:?}"))
                })?;
                self.facts
                    .commit_versions
                    .push(summary.commit_version().as_u64());
                self.facts.pressure_retries += 1;
                self.model
                    .record_ack(self.branch, summary.commit_version(), mutations);
            }
            Err(other) => {
                return Err(self.error(
                    step,
                    format!("unexpected commit error {}: {other:?}", other.code()),
                ));
            }
        }
        Ok(())
    }

    /// Live safety: the visible database must equal the model's acknowledged state.
    /// Reuses the recovery oracle under `ZeroLoss` — on a clean backend nothing is
    /// in-doubt, so the recovered watermark must reach the last acknowledged commit.
    fn assert_safety(&self, step: usize, label: &str) -> Result<(), TestkitError> {
        let recovered = scan_recovered(
            &self.runtime,
            self.branch,
            &oracle_space(),
            &oracle_prefix_key(),
            SCAN_LIMIT,
        )
        .map_err(|err| self.error(step, format!("live scan: {err}")))?;
        if let Some(violation) =
            classify_recovered(&self.model, self.branch, &recovered, CrashFamily::ZeroLoss).err()
        {
            return Err(self.error(
                step,
                format!("live-safety violation after {label}: {violation:?}"),
            ));
        }
        Ok(())
    }

    /// Liveness probed each step: no maintenance task may enter a failed state, and
    /// the pending-queue depth is recorded into the trajectory.
    fn assert_liveness(&mut self, step: usize) -> Result<(), TestkitError> {
        let status = self
            .runtime
            .maintenance_status()
            .map_err(|err| self.error(step, format!("maintenance status: {err:?}")))?;
        if status.failed() != 0 {
            return Err(self.error(
                step,
                format!("maintenance entered a failed state: {status:?}"),
            ));
        }
        self.facts.queue_trajectory.push(status.pending_tasks());
        Ok(())
    }

    /// Drain to quiescence and assert the terminal invariants: the queue is empty,
    /// no failures, admission is not blocked, and the visible state still matches.
    fn quiesce(&mut self) -> Result<(), TestkitError> {
        self.runtime
            .drain_maintenance()
            .map_err(|err| self.error(usize::MAX, format!("final drain: {err:?}")))?;
        let status = self
            .runtime
            .maintenance_status()
            .map_err(|err| self.error(usize::MAX, format!("quiesce status: {err:?}")))?;
        if status.pending_tasks() != 0 {
            return Err(self.error(
                usize::MAX,
                format!("maintenance queue did not drain at quiesce: {status:?}"),
            ));
        }
        if status.failed() != 0 {
            return Err(self.error(
                usize::MAX,
                format!("maintenance failures at quiesce: {status:?}"),
            ));
        }
        let report = self
            .runtime
            .diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Global))
            .map_err(|err| self.error(usize::MAX, format!("quiesce diagnostics: {err:?}")))?;
        if report.pressure().severity()
            == DiagnosticsStoragePressureSeverity::BlockMutatingAdmission
        {
            return Err(self.error(
                usize::MAX,
                "storage left in blocking admission pressure at quiesce",
            ));
        }
        let recovered = scan_recovered(
            &self.runtime,
            self.branch,
            &oracle_space(),
            &oracle_prefix_key(),
            SCAN_LIMIT,
        )
        .map_err(|err| self.error(usize::MAX, format!("quiesce scan: {err}")))?;
        if let Some(violation) =
            classify_recovered(&self.model, self.branch, &recovered, CrashFamily::ZeroLoss).err()
        {
            return Err(self.error(
                usize::MAX,
                format!("quiesce live-safety violation: {violation:?}"),
            ));
        }
        self.facts.maintenance_completed = status.completed();
        self.facts.final_pending_tasks = status.pending_tasks();
        self.facts.final_owned_l0_tables = report.source_layout().owned_l0_tables();
        self.facts.final_live_state = recovered;
        Ok(())
    }

    fn error(&self, step: usize, detail: impl Into<String>) -> TestkitError {
        TestkitError::new(format!(
            "[seed={} step={step}] {}",
            self.seed,
            detail.into()
        ))
    }

    fn into_facts(self) -> SimFacts {
        self.facts
    }
}

/// Run one seeded simulation trajectory in `root`, returning its deterministic facts.
pub(super) fn run_one_sim(
    root: &Path,
    seed: u64,
    step_budget: usize,
) -> Result<SimFacts, TestkitError> {
    let mut rng = SplitMix64::new(seed ^ ACTION_SALT);
    // Owned local-fs backend: deterministic-inline durable opens require an owned
    // handle. Scoped so it drops with the run.
    let backend = StorageBackend::local_fs(root.to_path_buf());
    let mut run = SimRun::open(&backend, seed, step_budget)?;
    for step in 0..step_budget {
        let action = draw_action(&mut rng);
        run.apply(action, step)?;
        run.assert_safety(step, action.label())?;
        run.assert_liveness(step)?;
    }
    run.quiesce()?;
    Ok(run.into_facts())
}

#[cfg(test)]
mod tests {
    use super::run_one_sim;

    /// The determinism guard: the same seed yields a bit-identical trajectory.
    #[test]
    fn same_seed_replays_bit_exact() {
        let first_dir = tempfile::tempdir().expect("tmp");
        let second_dir = tempfile::tempdir().expect("tmp");
        let first = run_one_sim(first_dir.path(), 42, 48).expect("first run");
        let second = run_one_sim(second_dir.path(), 42, 48).expect("second run");
        assert_eq!(first, second, "same seed produced divergent trajectories");
    }

    /// One run actually completes maintenance and advances the clock (non-vacuous).
    #[test]
    fn one_run_is_non_vacuous() {
        let dir = tempfile::tempdir().expect("tmp");
        let facts = run_one_sim(dir.path(), 7, 48).expect("run");
        assert!(
            facts.maintenance_completed() > 0,
            "no maintenance completed: {facts:?}"
        );
        assert!(
            facts.clock_advances() > 0,
            "clock never advanced: {facts:?}"
        );
    }

    /// Different seeds explore different trajectories (the sweep is not degenerate).
    #[test]
    fn distinct_seeds_diverge() {
        let dir_a = tempfile::tempdir().expect("tmp");
        let dir_b = tempfile::tempdir().expect("tmp");
        let a = run_one_sim(dir_a.path(), 1, 48).expect("run a");
        let b = run_one_sim(dir_b.path(), 2, 48).expect("run b");
        assert_ne!(a, b, "distinct seeds produced identical trajectories");
    }
}
