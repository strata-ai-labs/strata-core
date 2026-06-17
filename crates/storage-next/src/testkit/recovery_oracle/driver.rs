//! Recovery-oracle driver: seeded durable workloads crashed at chosen points,
//! reopened, and verified against the expected-state model.
//!
//! The first mechanism is a clean `drop` (a process crash that the OS page cache
//! survives), which must recover *every* acknowledged commit — the zero-loss
//! spine. WAL-tail on-disk damage and in-process backend faults are layered on
//! later. `run_one_drop` is the reusable unit STH-2..5 compose.

use std::collections::BTreeSet;
use std::path::Path;

use strata_core_next::BranchId;

use super::model::{ExpectedState, OracleDurability, RecordedMutation};
use super::verify::{classify_recovered, scan_recovered, CrashFamily, RecoveryOracleViolation};
use crate::api::{
    CommitBatch, CommitMutation, CommitOptions, StorageDurabilityPolicy, StorageKey,
    StorageRuntime, StorageSpaceId, StorageValue,
};
use crate::testkit::rng::SplitMix64;
use crate::testkit::TestkitError;

/// Shared non-empty key prefix for every workload key, so the verifier can
/// enumerate the full live state with one prefix scan (empty keys are rejected).
const ORACLE_PREFIX: u8 = 0x6f;
/// Small key space — the small-scope hypothesis keeps crash enumeration cheap.
const KEY_SPACE: u8 = 8;
const SCAN_LIMIT: usize = 4096;
/// Bounded-exhaustive workload length per seed (≤ a few ops after `fsync`).
const OP_COUNT: usize = 5;
const SEED_BUDGET: u64 = 32;

fn oracle_space() -> StorageSpaceId {
    StorageSpaceId::new(vec![0x20]).expect("oracle space")
}
fn oracle_prefix_key() -> StorageKey {
    StorageKey::new(vec![ORACLE_PREFIX]).expect("oracle prefix")
}
fn oracle_key(index: u8) -> StorageKey {
    StorageKey::new(vec![ORACLE_PREFIX, index]).expect("oracle key")
}

/// The runtime's default branch (`DEFAULT_BRANCH_ID`), created on open.
fn default_branch() -> BranchId {
    BranchId::from_bytes([0x01; BranchId::BYTE_LEN])
}

/// A driver failure: an oracle violation (a real recovery bug) or a harness
/// error (setup/IO problem).
#[derive(Debug)]
pub(crate) enum DriverError {
    Violation {
        seed: u64,
        durability: OracleDurability,
        op_count: usize,
        crash_index: usize,
        violation: RecoveryOracleViolation,
    },
    Harness(TestkitError),
}

impl From<TestkitError> for DriverError {
    fn from(err: TestkitError) -> Self {
        DriverError::Harness(err)
    }
}

/// Deterministic workload: each op is a commit of 1..=3 unique-key put/delete
/// mutations over the small key space, a pure function of `(seed, op_count)`.
fn generate_workload(seed: u64, op_count: usize) -> Vec<Vec<RecordedMutation>> {
    let mut rng = SplitMix64::new(seed);
    let mut ops = Vec::with_capacity(op_count);
    for _ in 0..op_count {
        let mutation_count = 1 + usize::from(rng.gen_u8_below(3));
        let mut mutations = Vec::with_capacity(mutation_count);
        let mut used: BTreeSet<u8> = BTreeSet::new();
        for _ in 0..mutation_count {
            let mut index = rng.gen_u8_below(KEY_SPACE);
            while !used.insert(index) {
                index = (index + 1) % KEY_SPACE;
            }
            if rng.gen_bool() {
                let value = rng.gen_u8_below(255);
                mutations.push(RecordedMutation::Put {
                    space: oracle_space(),
                    key: oracle_key(index),
                    value: StorageValue::new(vec![value]),
                });
            } else {
                mutations.push(RecordedMutation::Delete {
                    space: oracle_space(),
                    key: oracle_key(index),
                });
            }
        }
        ops.push(mutations);
    }
    ops
}

fn to_commit_mutation(mutation: &RecordedMutation) -> CommitMutation {
    match mutation {
        RecordedMutation::Put { space, key, value } => CommitMutation::Put {
            storage_space: space.clone(),
            key: key.clone(),
            value: value.clone(),
            ttl: None,
        },
        RecordedMutation::Delete { space, key } => CommitMutation::Delete {
            storage_space: space.clone(),
            key: key.clone(),
        },
    }
}

fn open_durable(
    root: &Path,
    durability: OracleDurability,
) -> Result<StorageRuntime<'static>, TestkitError> {
    let policy = match durability {
        OracleDurability::Standard => StorageDurabilityPolicy::Standard,
        OracleDurability::Always => StorageDurabilityPolicy::Always,
    };
    let outcome = StorageRuntime::open_durable_local(root.to_path_buf(), policy)
        .map_err(|err| TestkitError::new(format!("open durable: {err:?}")))?;
    Ok(outcome.into_runtime())
}

/// Commit the first `crash_index` ops, drop the runtime (crash), reopen, and
/// assert the recovered state is a prefix of acknowledged history with zero loss.
pub(crate) fn run_one_drop(
    root: &Path,
    seed: u64,
    durability: OracleDurability,
    op_count: usize,
    crash_index: usize,
) -> Result<(), DriverError> {
    let branch = default_branch();
    let workload = generate_workload(seed, op_count);
    let mut model = ExpectedState::new(durability);

    {
        let mut runtime = open_durable(root, durability)?;
        for mutations in workload.iter().take(crash_index) {
            let batch = CommitBatch::new(
                branch,
                mutations.iter().map(to_commit_mutation).collect(),
                CommitOptions::default(),
            )
            .map_err(|err| TestkitError::new(format!("build batch: {err:?}")))?;
            let summary = runtime
                .commit(&batch)
                .map_err(|err| TestkitError::new(format!("commit: {err:?}")))?;
            model.record_ack(branch, summary.commit_version(), mutations.clone());
        }
        // Crash: drop the runtime without a clean close.
    }

    let runtime = open_durable(root, durability)?;
    let recovered = scan_recovered(
        &runtime,
        branch,
        &oracle_space(),
        &oracle_prefix_key(),
        SCAN_LIMIT,
    )?;
    classify_recovered(&model, branch, &recovered, CrashFamily::ZeroLoss).map_err(|violation| {
        DriverError::Violation {
            seed,
            durability,
            op_count,
            crash_index,
            violation,
        }
    })
}

/// Counters describing a recovery-oracle run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecoveryOracleOutcome {
    seeds_executed: usize,
    drop_cases: usize,
    durability_modes_swept: usize,
}

impl RecoveryOracleOutcome {
    #[must_use]
    pub const fn seeds_executed(&self) -> usize {
        self.seeds_executed
    }
    #[must_use]
    pub const fn drop_cases(&self) -> usize {
        self.drop_cases
    }
    #[must_use]
    pub const fn durability_modes_swept(&self) -> usize {
        self.durability_modes_swept
    }
}

fn driver_error_to_testkit(err: DriverError) -> TestkitError {
    match err {
        DriverError::Harness(inner) => inner,
        DriverError::Violation {
            seed,
            durability,
            op_count,
            crash_index,
            violation,
        } => TestkitError::new(format!(
            "recovery oracle violation [seed={seed} durability={durability:?} \
             op_count={op_count} crash_index={crash_index}]: {violation:?}"
        )),
    }
}

/// Sweep seeds × both durability modes × every bounded-exhaustive crash point,
/// dropping the runtime at each point and verifying prefix recovery. `case_limit`
/// caps the number of cases (for CI budgets); `None` runs the full grid.
pub fn run_recovery_oracle_harness(
    root: &Path,
    case_limit: Option<usize>,
) -> Result<RecoveryOracleOutcome, TestkitError> {
    let modes = [OracleDurability::Standard, OracleDurability::Always];
    let mut outcome = RecoveryOracleOutcome {
        durability_modes_swept: modes.len(),
        ..RecoveryOracleOutcome::default()
    };
    let mut cases = 0usize;
    'outer: for seed in 0..SEED_BUDGET {
        outcome.seeds_executed += 1;
        for &durability in &modes {
            for crash_index in 0..=OP_COUNT {
                if case_limit.is_some_and(|limit| cases >= limit) {
                    break 'outer;
                }
                let case_dir = root.join(format!("seed-{seed}-{durability:?}-{crash_index}"));
                std::fs::create_dir_all(&case_dir)
                    .map_err(|err| TestkitError::new(format!("create case dir: {err}")))?;
                run_one_drop(&case_dir, seed, durability, OP_COUNT, crash_index)
                    .map_err(driver_error_to_testkit)?;
                outcome.drop_cases += 1;
                cases += 1;
            }
        }
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::super::verify::RecoveredState;
    use super::*;
    use crate::api::StorageValue;
    use strata_core_next::CommitVersion;

    fn put(index: u8, value: u8) -> RecordedMutation {
        RecordedMutation::Put {
            space: oracle_space(),
            key: oracle_key(index),
            value: StorageValue::new(vec![value]),
        }
    }

    /// Open durable, commit `ops` (recording every ack), drop, reopen, scan.
    fn drive_and_recover(
        dir: &std::path::Path,
        ops: &[Vec<RecordedMutation>],
    ) -> (ExpectedState, RecoveredState) {
        let branch = default_branch();
        let mut model = ExpectedState::new(OracleDurability::Standard);
        {
            let mut runtime = open_durable(dir, OracleDurability::Standard).expect("open");
            for mutations in ops {
                let batch = CommitBatch::new(
                    branch,
                    mutations.iter().map(to_commit_mutation).collect(),
                    CommitOptions::default(),
                )
                .expect("batch");
                let summary = runtime.commit(&batch).expect("commit");
                model.record_ack(branch, summary.commit_version(), mutations.clone());
            }
        }
        let runtime = open_durable(dir, OracleDurability::Standard).expect("reopen");
        let recovered = scan_recovered(
            &runtime,
            branch,
            &oracle_space(),
            &oracle_prefix_key(),
            SCAN_LIMIT,
        )
        .expect("scan");
        (model, recovered)
    }

    #[test]
    fn real_recovery_is_a_nonempty_valid_prefix() {
        let dir = tempfile::tempdir().expect("tmp");
        let ops = vec![vec![put(0, 10)], vec![put(1, 11)], vec![put(0, 20)]];
        let (model, recovered) = drive_and_recover(dir.path(), &ops);
        assert!(
            !recovered.is_empty(),
            "recovered state empty — scan or commit path is broken (would mask real losses)"
        );
        assert_eq!(
            classify_recovered(&model, default_branch(), &recovered, CrashFamily::ZeroLoss),
            Ok(())
        );
    }

    #[test]
    fn oracle_catches_a_fabricated_lost_commit() {
        let dir = tempfile::tempdir().expect("tmp");
        let ops = vec![vec![put(0, 10)], vec![put(1, 11)]];
        let (mut model, recovered) = drive_and_recover(dir.path(), &ops);
        // Claim one more acknowledged commit than the engine ever saw.
        let next = model
            .max_version(default_branch())
            .map_or(1, |version| version.as_u64() + 1);
        model.record_ack(default_branch(), CommitVersion::new(next), vec![put(2, 12)]);
        assert!(matches!(
            classify_recovered(&model, default_branch(), &recovered, CrashFamily::ZeroLoss),
            Err(RecoveryOracleViolation::LostAck { .. })
        ));
    }

    #[test]
    fn oracle_catches_an_unrecorded_recovered_commit() {
        let dir = tempfile::tempdir().expect("tmp");
        let branch = default_branch();
        let ops = [vec![put(0, 10)], vec![put(1, 11)], vec![put(2, 12)]];
        let mut model = ExpectedState::new(OracleDurability::Standard);
        {
            let mut runtime = open_durable(dir.path(), OracleDurability::Standard).expect("open");
            for (index, mutations) in ops.iter().enumerate() {
                let batch = CommitBatch::new(
                    branch,
                    mutations.iter().map(to_commit_mutation).collect(),
                    CommitOptions::default(),
                )
                .expect("batch");
                let summary = runtime.commit(&batch).expect("commit");
                // "Forget" the last commit: the engine has it, the model does not.
                if index < ops.len() - 1 {
                    model.record_ack(branch, summary.commit_version(), mutations.clone());
                }
            }
        }
        let runtime = open_durable(dir.path(), OracleDurability::Standard).expect("reopen");
        let recovered = scan_recovered(
            &runtime,
            branch,
            &oracle_space(),
            &oracle_prefix_key(),
            SCAN_LIMIT,
        )
        .expect("scan");
        assert!(
            classify_recovered(&model, branch, &recovered, CrashFamily::ZeroLoss).is_err(),
            "oracle accepted a recovered commit the model never acknowledged"
        );
    }
}
