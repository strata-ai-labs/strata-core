//! STH-6 config-sweep differential.
//!
//! A database has many internal paths that must produce one logical answer:
//! cache and durable, eager and deferred maintenance, tight and roomy
//! budgets. This harness runs one seeded workload — commits on two branches
//! (one forked mid-stream), interleaved maintenance, pressure-retry under
//! small budgets — under every storage configuration and asserts the
//! *logical read results are identical* at every checkpoint: same keys, same
//! values, same commit versions, per branch. Durability and timing may
//! differ by configuration; the data the caller sees may not.
//!
//! Two oracles run per checkpoint: cross-config equality against the first
//! configuration's snapshot (naming the diverging config, checkpoint,
//! branch, and key on failure), and a metamorphic point-read check inside
//! each configuration (every scanned row must be reproducible by a point
//! read — prefix-scan ⊆ point-read equivalence, the NoREC-style oracle that
//! needs no reference engine). The default branch is additionally checked
//! against the recovery-oracle expected-state model.

use std::collections::BTreeMap;
use std::path::Path;

use strata_core::BranchId;

use super::recovery_oracle::model::{ExpectedState, OracleDurability};
use super::recovery_oracle::workload::{
    default_branch, generate_workload, oracle_prefix_key, oracle_space, to_commit_mutation,
    SCAN_LIMIT,
};
use crate::api::{
    BranchAction, BranchGeneration, BranchRequest, CommitBatch, CommitOptions, MaintenanceRequest,
    MaintenanceScope, MaintenanceTask, PointReadRequest, PrefixScanReadRequest, ReadBound,
    ReadLimit, StorageApiError, StorageBackend, StorageDurabilityPolicy,
    StorageMaintenanceSchedulingPolicy, StorageOpenOptions, StorageRuntime,
};
use crate::testkit::TestkitError;

/// The workload's size knobs: the matrix run stays small (it repeats per
/// config), while targeted cells (the pressure-equivalence check) scale up.
#[derive(Clone, Copy, Debug)]
struct WorkloadShape {
    pre_fork_ops: usize,
    post_fork_rounds: usize,
}

const MATRIX_SHAPE: WorkloadShape = WorkloadShape {
    pre_fork_ops: 6,
    post_fork_rounds: 6,
};
/// Bounded retry budget for pressure-rejected commits under small budgets.
const PRESSURE_RETRY_LIMIT: usize = 128;

/// One storage configuration cell in the differential matrix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageConfigCase {
    mode: ConfigMode,
    budget: ConfigBudget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConfigMode {
    Cache,
    DurableStandard,
    DurableAlways,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConfigBudget {
    Default,
    LowMemory,
}

impl StorageConfigCase {
    fn label(self) -> String {
        format!("{:?}-{:?}", self.mode, self.budget)
    }
}

/// The full configuration matrix the differential sweeps.
#[must_use]
pub fn storage_config_matrix() -> Vec<StorageConfigCase> {
    let mut matrix = Vec::new();
    for mode in [
        ConfigMode::Cache,
        ConfigMode::DurableStandard,
        ConfigMode::DurableAlways,
    ] {
        for budget in [ConfigBudget::Default, ConfigBudget::LowMemory] {
            matrix.push(StorageConfigCase { mode, budget });
        }
    }
    matrix
}

/// Counters describing a differential run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConfigDifferentialOutcome {
    configs_run: usize,
    checkpoints_compared: usize,
    rows_compared: usize,
    point_reads_cross_checked: usize,
    pressure_retries: usize,
}

impl ConfigDifferentialOutcome {
    #[must_use]
    pub const fn configs_run(&self) -> usize {
        self.configs_run
    }
    #[must_use]
    pub const fn checkpoints_compared(&self) -> usize {
        self.checkpoints_compared
    }
    #[must_use]
    pub const fn rows_compared(&self) -> usize {
        self.rows_compared
    }
    #[must_use]
    pub const fn point_reads_cross_checked(&self) -> usize {
        self.point_reads_cross_checked
    }
    #[must_use]
    pub const fn pressure_retries(&self) -> usize {
        self.pressure_retries
    }
}

fn forked_branch() -> BranchId {
    BranchId::from_bytes([0x42; BranchId::BYTE_LEN])
}

/// `(key bytes) -> (value bytes, commit version)` — the logical content one
/// configuration exposes for one branch.
type BranchSnapshot = BTreeMap<Vec<u8>, (Vec<u8>, u64)>;
/// Snapshots for both branches at one checkpoint.
type CheckpointSnapshot = Vec<(BranchId, BranchSnapshot)>;

fn open_options(case: StorageConfigCase) -> StorageOpenOptions {
    let options = match case.mode {
        ConfigMode::Cache => StorageOpenOptions::cache(),
        ConfigMode::DurableStandard => {
            StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
        }
        ConfigMode::DurableAlways => {
            StorageOpenOptions::durable_local(StorageDurabilityPolicy::Always)
        }
    }
    .with_maintenance_scheduling_policy(StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue);
    match case.budget {
        ConfigBudget::Default => options,
        ConfigBudget::LowMemory => options.with_storage_budget_for_test(
            crate::lifecycle::StorageRuntimeBudget::low_memory_test_profile(),
        ),
    }
}

/// Commit with bounded drain-and-retry under typed storage pressure: small
/// budgets legitimately reject mid-stream, and the retry keeps the logical
/// op stream identical across configurations.
fn commit_with_pressure_retry(
    runtime: &mut StorageRuntime<'_>,
    batch: &CommitBatch,
    retries: &mut usize,
) -> Result<u64, TestkitError> {
    for _ in 0..PRESSURE_RETRY_LIMIT {
        match runtime.commit(batch) {
            Ok(summary) => return Ok(summary.commit_version().as_u64()),
            Err(StorageApiError::StoragePressure { retryable, .. }) => {
                if !retryable {
                    return Err(TestkitError::new(
                        "storage pressure rejection was not marked retryable",
                    ));
                }
                *retries += 1;
                // The relieving drain may itself be refused while saturated
                // (e.g. a rotation that would exceed the frozen budget).
                // That refusal must be typed ResourceExhausted and transient:
                // keep draining inside the bounded retry budget, and treat
                // exhaustion of the budget as a livelock finding.
                if let Err(err) = runtime.drain_maintenance() {
                    if err.class() != crate::api::StorageApiErrorClass::ResourceExhausted {
                        return Err(TestkitError::new(format!(
                            "drain under pressure failed non-transiently: {}: {err:?}",
                            err.code()
                        )));
                    }
                }
            }
            Err(other) => {
                return Err(TestkitError::new(format!(
                    "commit failed non-recoverably: {}: {other:?}",
                    other.code()
                )));
            }
        }
    }
    Err(TestkitError::new(format!(
        "commit still pressure-rejected after {PRESSURE_RETRY_LIMIT} drain-and-retry rounds"
    )))
}

fn drain_maintenance_round(runtime: &mut StorageRuntime<'_>, branch: BranchId) {
    for task in [
        MaintenanceTask::Flush,
        MaintenanceTask::Checkpoint,
        MaintenanceTask::Compact,
        MaintenanceTask::SnapshotPruning,
    ] {
        // Enqueue results are discarded: cache mode reports unsupported
        // scopes as no-ops, and drain errors surface below.
        let _ = runtime.enqueue_maintenance(&MaintenanceRequest::new(
            task,
            MaintenanceScope::Branch(branch),
        ));
        let _ =
            runtime.enqueue_maintenance(&MaintenanceRequest::new(task, MaintenanceScope::Global));
        // A maintenance failure is a real differential-relevant fault: it
        // must not silently change what readers see, but the drain itself is
        // expected to succeed on the clean path.
        if let Err(err) = runtime.drain_maintenance() {
            panic!("maintenance drain failed on the clean differential path: {err:?}");
        }
    }
}

/// Scan one branch's full logical content and cross-check every row with a
/// point read (the metamorphic oracle).
fn snapshot_branch(
    runtime: &StorageRuntime<'_>,
    branch: BranchId,
    outcome: &mut ConfigDifferentialOutcome,
) -> Result<BranchSnapshot, TestkitError> {
    let limit = ReadLimit::new(SCAN_LIMIT)
        .map_err(|err| TestkitError::new(format!("read limit: {err:?}")))?;
    let scan = runtime
        .scan_prefix(&PrefixScanReadRequest::new(
            branch,
            oracle_space(),
            oracle_prefix_key(),
            ReadBound::Latest,
            Some(limit),
        ))
        .map_err(|err| TestkitError::new(format!("differential scan: {err:?}")))?;
    let mut snapshot = BranchSnapshot::new();
    for row in scan.rows() {
        if row.is_tombstone() {
            continue;
        }
        let Some(value) = row.value() else {
            continue;
        };
        let point = runtime
            .read_point(&PointReadRequest::new(
                branch,
                oracle_space(),
                row.key().clone(),
                ReadBound::Latest,
            ))
            .map_err(|err| TestkitError::new(format!("differential point read: {err:?}")))?;
        let Some(point_row) = point.row() else {
            return Err(TestkitError::new(format!(
                "metamorphic divergence: scanned key {:02x?} is invisible to a point read",
                row.key().as_bytes()
            )));
        };
        if point_row.value() != Some(value) || point_row.commit_version() != row.commit_version() {
            return Err(TestkitError::new(format!(
                "metamorphic divergence at key {:02x?}: scan sees (v{}, {:02x?}), point read \
                 sees (v{}, {:02x?})",
                row.key().as_bytes(),
                row.commit_version().as_u64(),
                value.as_bytes(),
                point_row.commit_version().as_u64(),
                point_row.value().map(crate::api::StorageValue::as_bytes),
            )));
        }
        outcome.point_reads_cross_checked += 1;
        snapshot.insert(
            row.key().as_bytes().to_vec(),
            (value.as_bytes().to_vec(), row.commit_version().as_u64()),
        );
    }
    Ok(snapshot)
}

/// Run the shared workload under one configuration, returning the checkpoint
/// snapshots and the default-branch model.
fn run_workload_under_config(
    root: &Path,
    case: StorageConfigCase,
    seed: u64,
    shape: WorkloadShape,
    outcome: &mut ConfigDifferentialOutcome,
) -> Result<(Vec<CheckpointSnapshot>, ExpectedState), TestkitError> {
    let branch = default_branch();
    let fork = forked_branch();
    let mut model = ExpectedState::new(match case.mode {
        ConfigMode::DurableAlways => OracleDurability::Always,
        _ => OracleDurability::Standard,
    });
    let mut checkpoints = Vec::new();

    let backend = if case.mode == ConfigMode::Cache {
        StorageBackend::memory()
    } else {
        let case_root = root.join(case.label());
        std::fs::create_dir_all(&case_root)
            .map_err(|err| TestkitError::new(format!("create case root: {err}")))?;
        StorageBackend::local_fs(case_root)
    };
    let mut runtime = StorageRuntime::open_with_backend(open_options(case), &backend)
        .map_err(|err| TestkitError::new(format!("open {}: {err:?}", case.label())))?
        .into_runtime();

    // Phase 1: pre-fork commits on the default branch.
    for mutations in &generate_workload(seed, shape.pre_fork_ops) {
        let batch = CommitBatch::new(
            branch,
            mutations.iter().map(to_commit_mutation).collect(),
            CommitOptions::default(),
        )
        .map_err(|err| TestkitError::new(format!("build batch: {err:?}")))?;
        let version =
            commit_with_pressure_retry(&mut runtime, &batch, &mut outcome.pressure_retries)?;
        model.record_ack(
            branch,
            strata_core::CommitVersion::new(version),
            mutations.clone(),
        );
    }
    checkpoints.push(vec![(branch, snapshot_branch(&runtime, branch, outcome)?)]);

    // Phase 2: fork, then alternate commits across both branches with
    // maintenance interleaved — the paths most likely to diverge.
    runtime
        .branch(&BranchRequest::new(
            fork,
            BranchAction::ForkCurrent { source: branch },
            Some(BranchGeneration::new(1)),
        ))
        .map_err(|err| TestkitError::new(format!("fork: {err:?}")))?;
    let default_stream = generate_workload(seed.wrapping_add(1), shape.post_fork_rounds);
    let fork_stream = generate_workload(seed.wrapping_add(2), shape.post_fork_rounds);
    for round in 0..shape.post_fork_rounds {
        for (target, mutations) in [
            (branch, &default_stream[round]),
            (fork, &fork_stream[round]),
        ] {
            let batch = CommitBatch::new(
                target,
                mutations.iter().map(to_commit_mutation).collect(),
                CommitOptions::default(),
            )
            .map_err(|err| TestkitError::new(format!("build batch: {err:?}")))?;
            let version =
                commit_with_pressure_retry(&mut runtime, &batch, &mut outcome.pressure_retries)?;
            if target == branch {
                model.record_ack(
                    branch,
                    strata_core::CommitVersion::new(version),
                    mutations.clone(),
                );
            }
        }
        if round % 2 == 1 {
            drain_maintenance_round(&mut runtime, branch);
        }
    }
    checkpoints.push(vec![
        (branch, snapshot_branch(&runtime, branch, outcome)?),
        (fork, snapshot_branch(&runtime, fork, outcome)?),
    ]);

    // Phase 3: a final maintenance round then a last read — post-compaction
    // content must still agree.
    drain_maintenance_round(&mut runtime, branch);
    checkpoints.push(vec![
        (branch, snapshot_branch(&runtime, branch, outcome)?),
        (fork, snapshot_branch(&runtime, fork, outcome)?),
    ]);

    Ok((checkpoints, model))
}

/// Model oracle: the default branch's final content must equal the
/// expected-state model's live view.
fn assert_final_default_matches_model(
    label: &str,
    checkpoints: &[CheckpointSnapshot],
    model: &ExpectedState,
) -> Result<(), TestkitError> {
    let final_default = checkpoints
        .last()
        .and_then(|snapshot| snapshot.first())
        .map(|(_, content)| content.clone())
        .unwrap_or_default();
    let branch = default_branch();
    let upper = model
        .max_version(branch)
        .unwrap_or(strata_core::CommitVersion::ZERO);
    let expected: BranchSnapshot = model
        .live_state_at(branch, upper)
        .into_iter()
        .map(|((_, key), (value, version))| {
            (
                key.as_bytes().to_vec(),
                (value.as_bytes().to_vec(), version.as_u64()),
            )
        })
        .collect();
    if final_default != expected {
        let key = first_divergence(&expected, &final_default);
        return Err(TestkitError::new(format!(
            "config {label} diverged from the expected-state model at key {key:02x?}"
        )));
    }
    Ok(())
}

/// The pressure-equivalence cell: one low-memory durable configuration under
/// a stream long enough to hit typed storage pressure, proving the
/// drain-and-retry path preserves logical content exactly (retries must be
/// invisible to readers). Returns the retry count for non-vacuity.
pub fn run_low_memory_pressure_equivalence(root: &Path, seed: u64) -> Result<usize, TestkitError> {
    let mut outcome = ConfigDifferentialOutcome::default();
    let case = StorageConfigCase {
        mode: ConfigMode::DurableStandard,
        budget: ConfigBudget::LowMemory,
    };
    let shape = WorkloadShape {
        pre_fork_ops: 1200,
        post_fork_rounds: 0,
    };
    let (checkpoints, model) = run_workload_under_config(root, case, seed, shape, &mut outcome)?;
    assert_final_default_matches_model("pressure-equivalence", &checkpoints, &model)?;
    Ok(outcome.pressure_retries)
}

fn first_divergence(reference: &BranchSnapshot, candidate: &BranchSnapshot) -> Option<Vec<u8>> {
    for (key, expected) in reference {
        if candidate.get(key) != Some(expected) {
            return Some(key.clone());
        }
    }
    candidate
        .keys()
        .find(|key| !reference.contains_key(*key))
        .cloned()
}

/// Run the shared workload under every configuration and assert identical
/// logical reads at every checkpoint, plus model equality on the default
/// branch.
pub fn run_config_differential(
    root: &Path,
    seed: u64,
) -> Result<ConfigDifferentialOutcome, TestkitError> {
    let mut outcome = ConfigDifferentialOutcome::default();
    let matrix = storage_config_matrix();
    let mut reference: Option<(String, Vec<CheckpointSnapshot>)> = None;

    for case in matrix {
        let (checkpoints, model) =
            run_workload_under_config(root, case, seed, MATRIX_SHAPE, &mut outcome)?;
        outcome.configs_run += 1;

        assert_final_default_matches_model(&case.label(), &checkpoints, &model)?;

        match &reference {
            None => reference = Some((case.label(), checkpoints)),
            Some((reference_label, reference_checkpoints)) => {
                if reference_checkpoints.len() != checkpoints.len() {
                    return Err(TestkitError::new(format!(
                        "config {} produced {} checkpoints, reference {reference_label} \
                         produced {}",
                        case.label(),
                        checkpoints.len(),
                        reference_checkpoints.len()
                    )));
                }
                for (index, (reference_snapshot, candidate_snapshot)) in
                    reference_checkpoints.iter().zip(&checkpoints).enumerate()
                {
                    for ((ref_branch, ref_content), (cand_branch, cand_content)) in
                        reference_snapshot.iter().zip(candidate_snapshot)
                    {
                        assert_eq!(ref_branch, cand_branch, "checkpoint branch order differs");
                        if ref_content != cand_content {
                            let key =
                                first_divergence(ref_content, cand_content).unwrap_or_default();
                            let expected = ref_content.get(&key);
                            let got = cand_content.get(&key);
                            return Err(TestkitError::new(format!(
                                "silent wrong result: config {} disagrees with {reference_label} \
                                 at checkpoint {index}, branch {ref_branch}, key {key:02x?}: \
                                 reference {expected:?}, got {got:?}",
                                case.label()
                            )));
                        }
                        outcome.rows_compared += ref_content.len();
                    }
                    outcome.checkpoints_compared += 1;
                }
            }
        }
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_config_agrees_on_logical_reads() {
        let dir = tempfile::tempdir().expect("tmp");
        let outcome = run_config_differential(dir.path(), 7).expect("config differential");
        assert_eq!(outcome.configs_run(), 6, "matrix did not run all configs");
        assert!(
            outcome.checkpoints_compared() > 0,
            "no checkpoints compared"
        );
        assert!(outcome.rows_compared() > 0, "no rows compared");
        assert!(
            outcome.point_reads_cross_checked() > 0,
            "metamorphic oracle never ran"
        );
    }

    #[test]
    #[ignore = "blocked on #2609: EvaluateAndEnqueue livelocks under sustained low-memory pressure (frozen backlog wedges admission); un-ignore with the fix"]
    fn pressure_retries_are_invisible_to_readers() {
        let dir = tempfile::tempdir().expect("tmp");
        // Non-vacuity for the budget axis: a long stream against the
        // low-memory profile must hit typed pressure, and the retried
        // stream's final content must still match the model exactly.
        let retries =
            run_low_memory_pressure_equivalence(dir.path(), 3).expect("pressure equivalence");
        assert!(
            retries > 0,
            "the low-memory stream never hit storage pressure; grow the stream or shrink the budget"
        );
    }

    /// Soak: the differential across many seeds — the genuine silent-wrong-
    /// result hunt. `#[ignore]` by default; run with `--ignored` nightly.
    #[test]
    #[ignore = "soak: multi-seed config differential; run with --ignored"]
    fn config_differential_soak_across_seeds() {
        let dir = tempfile::tempdir().expect("tmp");
        for seed in 0..16 {
            let outcome = run_config_differential(&dir.path().join(format!("seed-{seed}")), seed)
                .unwrap_or_else(|err| panic!("differential soak seed {seed}: {err}"));
            assert_eq!(outcome.configs_run(), 6);
        }
    }

    #[test]
    fn divergence_reporting_names_the_config_and_key() {
        // Sanity-check the diff reporter itself: two snapshots differing at
        // one key must yield that key.
        let mut reference = BranchSnapshot::new();
        reference.insert(vec![1], (vec![10], 1));
        reference.insert(vec![2], (vec![20], 2));
        let mut candidate = reference.clone();
        candidate.insert(vec![2], (vec![21], 2));
        assert_eq!(first_divergence(&reference, &candidate), Some(vec![2]));
        candidate.remove(&vec![2]);
        assert_eq!(first_divergence(&reference, &candidate), Some(vec![2]));
        let extra_key = {
            let mut extra = reference.clone();
            extra.insert(vec![3], (vec![30], 3));
            first_divergence(&reference, &extra)
        };
        assert_eq!(extra_key, Some(vec![3]));
    }
}
