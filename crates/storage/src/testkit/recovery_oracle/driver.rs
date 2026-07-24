//! Recovery-oracle driver: seeded durable workloads crashed at chosen points,
//! reopened, and verified against the expected-state model.
//!
//! Three crash mechanisms run today. A clean `drop` (a process crash the OS page
//! cache survives) must recover *every* acknowledged commit — the zero-loss
//! spine. WAL-tail truncation (shaving the active segment's trailing record) tears
//! the most recent commit; a lossy reopen repairs the torn tail down to the intact
//! prefix, which must still be a clean prefix of acknowledged history. WAL byte
//! corruption (a length-preserving bit flip) must never be silently tolerated: the
//! engine either fails loud or opens onto a clean prefix. In-process backend
//! faults are layered on later. `run_one` is the reusable unit the rest of the
//! hardening program composes.

use std::path::{Path, PathBuf};

use strata_core::BranchId;

use super::model::{ExpectedState, OracleDurability};
use super::verify::{classify_recovered, scan_recovered, CrashFamily, RecoveryOracleViolation};
use super::workload::{
    default_branch, generate_workload, oracle_prefix_key, oracle_space, to_commit_mutation,
    SCAN_LIMIT,
};
use crate::api::{
    CommitBatch, CommitOptions, StorageBackend, StorageDurabilityPolicy, StorageOpenOptions,
    StorageRuntime,
};
use crate::testkit::rng::SplitMix64;
use crate::testkit::TestkitError;

/// Bounded-exhaustive workload length per seed (≤ a few ops after `fsync`).
const OP_COUNT: usize = 5;
const SEED_BUDGET: u64 = 32;
/// Salt mixing the seed for the WAL-tail truncation amount, so the damage size is
/// independent of the workload draws yet deterministic and replayable per seed.
const WAL_TRUNCATE_SALT: u64 = 0x5741_4c5f_5441_494c;
/// Exclusive upper bound on bytes shaved off the active WAL segment's tail. Small
/// relative to a commit record, so it tears the trailing record without erasing
/// the whole segment.
const WAL_TRUNCATE_MAX: u8 = 64;
/// Salt mixing the seed for the WAL byte-corruption offset, independent of the
/// workload and truncation draws yet deterministic and replayable per seed.
const WAL_CORRUPT_SALT: u64 = 0x4259_5445_524f_5421;

/// How a case crashes the workload, and therefore how its recovery is judged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CrashMechanism {
    /// Drop the runtime without a clean close — a process crash the page cache
    /// survives. Recovery must lose nothing (`CrashFamily::ZeroLoss`).
    Drop,
    /// Tear the active WAL segment's trailing record, then reopen with lossy
    /// recovery — on-disk damage tolerating a lost suffix (`OnDiskDamage`).
    WalTruncate,
    /// Flip a byte of the active WAL segment (bit-rot / torn write that keeps the
    /// file length), then reopen. The engine must never silently return corrupted
    /// or gapped state: it either fails loud or opens onto a clean prefix. The
    /// byte-level error taxonomy is owned by the service-layer WAL corruption
    /// suite; this is the end-to-end, model-checked complement.
    WalCorruptByte,
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
        mechanism: CrashMechanism,
        violation: RecoveryOracleViolation,
    },
    Harness(TestkitError),
}

impl From<TestkitError> for DriverError {
    fn from(err: TestkitError) -> Self {
        DriverError::Harness(err)
    }
}

fn durability_policy(durability: OracleDurability) -> StorageDurabilityPolicy {
    match durability {
        OracleDurability::Standard => StorageDurabilityPolicy::Standard,
        OracleDurability::Always => StorageDurabilityPolicy::Always,
    }
}

/// Open with strict recovery — the reopen path for a zero-loss crash, where a
/// torn WAL tail would be a real bug rather than tolerated damage.
fn open_durable(
    root: &Path,
    durability: OracleDurability,
) -> Result<StorageRuntime<'static>, TestkitError> {
    // Retry-on-Unavailable absorbs the prior runtime's detached-worker
    // writer-lock window (#2727); any other failure stays loud.
    let outcome = crate::testkit::reopen_retry::open_with_retry_on_unavailable(|| {
        StorageRuntime::open_durable_local(root.to_path_buf(), durability_policy(durability))
    })
    .map_err(|err| TestkitError::new(format!("open durable: {err:?}")))?;
    Ok(outcome.into_runtime())
}

/// Open with lossy WAL-tail recovery — the reopen path after on-disk damage.
/// Strict recovery repairs a torn LATEST tail (an unacknowledged mid-append
/// artifact) on its own; lossy opt-in is for deeper damage — non-tail loss,
/// attested-suffix loss — where recovery must be allowed to drop
/// acknowledged bytes. Leaks the backend for a `'static` runtime (test-only;
/// reclaimed on process exit).
fn open_durable_lossy(
    root: &Path,
    durability: OracleDurability,
) -> Result<StorageRuntime<'static>, TestkitError> {
    let backend: &'static StorageBackend =
        crate::testkit::leak_static(StorageBackend::local_fs(root.to_path_buf()));
    // Retry-on-Unavailable absorbs the prior runtime's detached-worker
    // writer-lock window (#2727); any other failure stays loud.
    let outcome = crate::testkit::reopen_retry::open_with_retry_on_unavailable(|| {
        let options = StorageOpenOptions::durable_local(durability_policy(durability))
            .with_strict_recovery(false);
        StorageRuntime::open_with_backend(options, backend)
    })
    .map_err(|err| TestkitError::new(format!("lossy reopen: {err:?}")))?;
    Ok(outcome.into_runtime())
}

/// Path of the active (highest-id) WAL segment file under `root`, if any. Segment
/// objects are named `<16-hex-id>.object@`; any other `wal/` object is ignored so
/// the damage targets a real segment and is replayable.
fn active_wal_segment(root: &Path) -> Option<PathBuf> {
    let wal_dir = root.join("wal");
    let mut best: Option<(String, PathBuf)> = None;
    for entry in std::fs::read_dir(&wal_dir).ok()?.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(id) = name.strip_suffix(".object@") else {
            continue;
        };
        if id.len() != 16 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            continue;
        }
        let id = id.to_owned();
        if best
            .as_ref()
            .is_none_or(|(best_id, _)| id.as_str() > best_id.as_str())
        {
            best = Some((id, path));
        }
    }
    best.map(|(_, path)| path)
}

/// Shave `bytes` off the active WAL segment's tail — a torn-write / power-loss
/// simulation that leaves a partial trailing record. Returns whether a segment
/// was found to damage.
fn truncate_wal_tail(root: &Path, bytes: u64) -> Result<bool, TestkitError> {
    let Some(segment) = active_wal_segment(root) else {
        return Ok(false);
    };
    let len = std::fs::metadata(&segment)
        .map_err(|err| TestkitError::new(format!("wal segment metadata: {err}")))?
        .len();
    let new_len = len.saturating_sub(bytes.max(1));
    std::fs::OpenOptions::new()
        .write(true)
        .open(&segment)
        .map_err(|err| TestkitError::new(format!("open wal segment: {err}")))?
        .set_len(new_len)
        .map_err(|err| TestkitError::new(format!("truncate wal segment: {err}")))?;
    Ok(true)
}

/// Flip the byte at `index` of the active WAL segment (a bit-rot / torn write
/// that preserves file length, so it cannot be mistaken for a clean truncation).
/// Returns whether a byte was flipped.
fn flip_wal_byte(root: &Path, index: usize) -> Result<bool, TestkitError> {
    let Some(segment) = active_wal_segment(root) else {
        return Ok(false);
    };
    let mut bytes = std::fs::read(&segment)
        .map_err(|err| TestkitError::new(format!("read wal segment: {err}")))?;
    if index >= bytes.len() {
        return Ok(false);
    }
    bytes[index] ^= 0xff;
    std::fs::write(&segment, &bytes)
        .map_err(|err| TestkitError::new(format!("write wal segment: {err}")))?;
    Ok(true)
}

/// Flip a seeded byte anywhere in the active WAL segment. Across the seed sweep
/// the offset lands in the header, an interior record, and the trailing record,
/// so one mechanism exercises every corruption placement end-to-end.
fn corrupt_wal_byte(root: &Path, rng: &mut SplitMix64) -> Result<bool, TestkitError> {
    let Some(segment) = active_wal_segment(root) else {
        return Ok(false);
    };
    let len = std::fs::metadata(&segment)
        .map_err(|err| TestkitError::new(format!("wal segment metadata: {err}")))?
        .len();
    if len == 0 {
        return Ok(false);
    }
    let index = usize::try_from(rng.next_u64() % len).unwrap_or(0);
    flip_wal_byte(root, index)
}

/// Scan the reopened runtime and classify it against the model — the shared
/// post-condition for the mechanisms that expect a successful recovery.
fn verify_recovered_prefix(
    runtime: &StorageRuntime<'_>,
    model: &ExpectedState,
    branch: BranchId,
    family: CrashFamily,
) -> Result<Option<RecoveryOracleViolation>, TestkitError> {
    let recovered = scan_recovered(
        runtime,
        branch,
        &oracle_space(),
        &oracle_prefix_key(),
        SCAN_LIMIT,
    )?;
    Ok(classify_recovered(model, branch, &recovered, family).err())
}

/// The corruption post-condition: a byte-corrupted WAL must never silently
/// surface as wrong data. The engine must EITHER fail to open (a loud recovery
/// error — safe) OR open onto a clean prefix of acknowledged history. Only a
/// successful open with a gap / phantom / torn batch is a violation.
fn verify_corruption(
    root: &Path,
    model: &ExpectedState,
    branch: BranchId,
    durability: OracleDurability,
) -> Result<Option<RecoveryOracleViolation>, TestkitError> {
    let backend: &'static StorageBackend =
        crate::testkit::leak_static(StorageBackend::local_fs(root.to_path_buf()));
    let options = StorageOpenOptions::durable_local(durability_policy(durability))
        .with_strict_recovery(false);
    match StorageRuntime::open_with_backend(options, backend) {
        // Failed loud — safe. (Whether the loud error is a checksum mismatch,
        // bad magic, etc. is asserted by the service-layer corruption suite; the
        // oracle only requires that corruption is not silently tolerated.)
        Err(_) => Ok(None),
        Ok(outcome) => verify_recovered_prefix(
            &outcome.into_runtime(),
            model,
            branch,
            CrashFamily::OnDiskDamage,
        ),
    }
}

/// Commit the first `crash_index` ops (recording every ack), crash via
/// `mechanism`, reopen, and assert the recovered state is a prefix of
/// acknowledged history with the strictness the mechanism's crash family demands.
/// The reusable unit the rest of the hardening program calls.
pub(crate) fn run_one(
    root: &Path,
    seed: u64,
    durability: OracleDurability,
    op_count: usize,
    crash_index: usize,
    mechanism: CrashMechanism,
) -> Result<(), DriverError> {
    let branch = default_branch();
    let workload = generate_workload(seed, op_count);
    let mut model = ExpectedState::new(durability);

    {
        let runtime = open_durable(root, durability)?;
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

    // Apply the mechanism's on-disk effect, then verify recovery with the
    // post-condition that mechanism demands.
    let violation = match mechanism {
        CrashMechanism::Drop => {
            let runtime = open_durable(root, durability)?;
            verify_recovered_prefix(&runtime, &model, branch, CrashFamily::ZeroLoss)?
        }
        CrashMechanism::WalTruncate => {
            let mut rng = SplitMix64::new(seed.rotate_left(13) ^ WAL_TRUNCATE_SALT);
            truncate_wal_tail(root, 1 + u64::from(rng.gen_u8_below(WAL_TRUNCATE_MAX)))?;
            let runtime = open_durable_lossy(root, durability)?;
            verify_recovered_prefix(&runtime, &model, branch, CrashFamily::OnDiskDamage)?
        }
        CrashMechanism::WalCorruptByte => {
            let mut rng = SplitMix64::new(seed.rotate_left(29) ^ WAL_CORRUPT_SALT);
            corrupt_wal_byte(root, &mut rng)?;
            verify_corruption(root, &model, branch, durability)?
        }
    };

    violation.map_or(Ok(()), |violation| {
        Err(DriverError::Violation {
            seed,
            durability,
            op_count,
            crash_index,
            mechanism,
            violation,
        })
    })
}

/// Counters describing a recovery-oracle run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecoveryOracleOutcome {
    seeds_executed: usize,
    drop_cases: usize,
    wal_damage_cases: usize,
    corruption_cases: usize,
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
    pub const fn wal_damage_cases(&self) -> usize {
        self.wal_damage_cases
    }
    #[must_use]
    pub const fn corruption_cases(&self) -> usize {
        self.corruption_cases
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
            mechanism,
            violation,
        } => TestkitError::new(format!(
            "recovery oracle violation [seed={seed} durability={durability:?} \
             op_count={op_count} crash_index={crash_index} mechanism={mechanism:?}]: {violation:?}"
        )),
    }
}

/// Run one case in a fresh subdirectory (no cross-case leakage), mapping any
/// violation into a harness error carrying the replay tuple.
fn run_case(
    root: &Path,
    tag: &str,
    seed: u64,
    durability: OracleDurability,
    crash_index: usize,
    mechanism: CrashMechanism,
) -> Result<(), TestkitError> {
    let case_dir = root.join(format!("{tag}-{seed}-{durability:?}-{crash_index}"));
    std::fs::create_dir_all(&case_dir)
        .map_err(|err| TestkitError::new(format!("create case dir: {err}")))?;
    run_one(
        &case_dir,
        seed,
        durability,
        OP_COUNT,
        crash_index,
        mechanism,
    )
    .map_err(driver_error_to_testkit)
}

/// Sweep seeds × both durability modes × every bounded-exhaustive crash point ×
/// every crash mechanism (clean drop, WAL-tail truncation, WAL byte corruption),
/// verifying the matching post-condition at each. `case_limit` caps the number of
/// cases (for CI budgets); `None` runs the full grid.
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
                // Clean drop at every crash point (the zero-loss spine).
                if case_limit.is_some_and(|limit| cases >= limit) {
                    break 'outer;
                }
                run_case(
                    root,
                    "drop",
                    seed,
                    durability,
                    crash_index,
                    CrashMechanism::Drop,
                )?;
                outcome.drop_cases += 1;
                cases += 1;

                // On-disk damage needs at least one committed op to act on.
                if crash_index >= 1 {
                    if case_limit.is_some_and(|limit| cases >= limit) {
                        break 'outer;
                    }
                    run_case(
                        root,
                        "waltrunc",
                        seed,
                        durability,
                        crash_index,
                        CrashMechanism::WalTruncate,
                    )?;
                    outcome.wal_damage_cases += 1;
                    cases += 1;

                    if case_limit.is_some_and(|limit| cases >= limit) {
                        break 'outer;
                    }
                    run_case(
                        root,
                        "corrupt",
                        seed,
                        durability,
                        crash_index,
                        CrashMechanism::WalCorruptByte,
                    )?;
                    outcome.corruption_cases += 1;
                    cases += 1;
                }
            }
        }
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::super::model::RecordedMutation;
    use super::super::verify::RecoveredState;
    use super::super::workload::oracle_key;
    use super::*;
    use crate::api::{
        StorageApiError, StorageApiErrorClass, StorageApiResult, StorageOpenOutcome, StorageValue,
    };
    use strata_core::CommitVersion;

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
            let runtime = open_durable(dir, OracleDurability::Standard).expect("open");
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
            let runtime = open_durable(dir.path(), OracleDurability::Standard).expect("open");
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

    /// The WAL-tail mechanism must induce *real* suffix loss (not a no-op) and the
    /// recovered prefix must be clean under the damage family yet a loss under the
    /// zero-loss family — proving both the mechanism and the family distinction.
    #[test]
    fn wal_truncation_loses_tail_and_recovers_clean_prefix() {
        let dir = tempfile::tempdir().expect("tmp");
        let branch = default_branch();
        let mut model = ExpectedState::new(OracleDurability::Standard);
        {
            let runtime = open_durable(dir.path(), OracleDurability::Standard).expect("open");
            for index in 0u8..5 {
                let mutations = vec![put(index, 100 + index)];
                let batch = CommitBatch::new(
                    branch,
                    mutations.iter().map(to_commit_mutation).collect(),
                    CommitOptions::default(),
                )
                .expect("batch");
                let summary = runtime.commit(&batch).expect("commit");
                model.record_ack(branch, summary.commit_version(), mutations);
            }
        }
        // Tear the active WAL segment's trailing record, then reopen lossily.
        assert!(
            truncate_wal_tail(dir.path(), 32).expect("truncate"),
            "expected an active WAL segment to truncate"
        );
        let runtime =
            open_durable_lossy(dir.path(), OracleDurability::Standard).expect("lossy reopen");
        let recovered = scan_recovered(
            &runtime,
            branch,
            &oracle_space(),
            &oracle_prefix_key(),
            SCAN_LIMIT,
        )
        .expect("scan");
        // Real damage: at least the tail commit is gone.
        assert!(
            recovered.len() < 5,
            "truncation should drop at least the tail commit (recovered {})",
            recovered.len()
        );
        // The survivor is a clean prefix for the damage family,
        assert_eq!(
            classify_recovered(&model, branch, &recovered, CrashFamily::OnDiskDamage),
            Ok(())
        );
        // but a genuine loss for the zero-loss family — the distinction bites.
        assert!(
            classify_recovered(&model, branch, &recovered, CrashFamily::ZeroLoss).is_err(),
            "zero-loss family must reject a lost-tail recovery"
        );
    }

    /// Drive `n` single-put commits, drop, and return the active WAL segment's
    /// on-disk length (so a test can target the header / interior / tail).
    fn commit_n_and_segment_len(dir: &std::path::Path, n: u8) -> u64 {
        let branch = default_branch();
        {
            let runtime = open_durable(dir, OracleDurability::Standard).expect("open");
            for index in 0..n {
                let mutations = [put(index, 100 + index)];
                let batch = CommitBatch::new(
                    branch,
                    mutations.iter().map(to_commit_mutation).collect(),
                    CommitOptions::default(),
                )
                .expect("batch");
                runtime.commit(&batch).expect("commit");
            }
        }
        let segment = active_wal_segment(dir).expect("active segment");
        std::fs::metadata(&segment).expect("segment metadata").len()
    }

    /// Reopen the corrupted database with lossy recovery, keeping the raw
    /// `StorageApiError` so the test can assert its class at the public boundary.
    fn reopen_lossy_raw(dir: &std::path::Path) -> StorageApiResult<StorageRuntime<'static>> {
        let backend: &'static StorageBackend =
            crate::testkit::leak_static(StorageBackend::local_fs(dir.to_path_buf()));
        let options = StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
            .with_strict_recovery(false);
        StorageRuntime::open_with_backend(options, backend).map(StorageOpenOutcome::into_runtime)
    }

    /// A corruption must surface as a loud, permanent recovery failure at the
    /// public open boundary — never a silent successful open, and never a
    /// transient outage a caller would retry forever.
    fn assert_lifecycle_open_failure(result: StorageApiResult<StorageRuntime<'static>>) {
        match result {
            Ok(_) => panic!("corruption was silently tolerated — open succeeded"),
            Err(error) => {
                assert_eq!(
                    error.code(),
                    "failed_precondition.storage_api.recovery_degraded"
                );
                assert_eq!(error.class(), StorageApiErrorClass::FailedPrecondition);
                assert!(
                    matches!(&error, StorageApiError::RecoveryDegraded { .. }),
                    "expected a permanent recovery-corruption failure"
                );
            }
        }
    }

    #[test]
    fn segment_header_corruption_fails_open_loudly() {
        let dir = tempfile::tempdir().expect("tmp");
        commit_n_and_segment_len(dir.path(), 3);
        // Byte 0 is the segment magic — flipping it cannot be a recoverable tail.
        assert!(
            flip_wal_byte(dir.path(), 0).expect("flip"),
            "expected an active WAL segment to corrupt"
        );
        assert_lifecycle_open_failure(reopen_lossy_raw(dir.path()));
    }

    #[test]
    fn mid_stream_corruption_fails_open_loudly() {
        let dir = tempfile::tempdir().expect("tmp");
        // Several commits so the midpoint lands inside an interior (non-tail)
        // record — proving recovery never skips a bad record and replays later
        // ones (which would be a silent gap).
        let len = commit_n_and_segment_len(dir.path(), 6);
        let midpoint = usize::try_from(len / 2).expect("midpoint fits usize");
        assert!(
            flip_wal_byte(dir.path(), midpoint).expect("flip"),
            "no flip"
        );
        assert_lifecycle_open_failure(reopen_lossy_raw(dir.path()));
    }

    #[test]
    fn corrupt_tail_byte_is_not_salvaged_by_lossy_recovery() {
        let dir = tempfile::tempdir().expect("tmp");
        let len = commit_n_and_segment_len(dir.path(), 5);
        // A byte near the end falls inside the trailing record's checksum region.
        let tail = usize::try_from(len.saturating_sub(3)).expect("tail fits usize");
        assert!(flip_wal_byte(dir.path(), tail).expect("flip"), "no flip");
        // Contrast with `wal_truncation_loses_tail_and_recovers_clean_prefix`: a
        // *short* tail is salvaged, but a full-length CRC-invalid tail is NOT
        // auto-repaired even by lossy recovery — it fails loud (current contract).
        assert_lifecycle_open_failure(reopen_lossy_raw(dir.path()));
    }
}
