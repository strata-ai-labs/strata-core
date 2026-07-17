//! Recovery-time read/list/metadata fault sweep (TCP3.3b).
//!
//! The write-path fault sweep (`fault_sweep`) deliberately excludes
//! `ReadObject` / `ListPrefix` / `ObjectMetadata`: on the *write* path those
//! are query-time, not data-loss. But there is a second place where a read
//! failure is a correctness concern — **open recovery**, where the runtime
//! scans the backend (lists prefixes, reads the manifest, heads objects) to
//! decide what durably exists. A transient list/read/metadata failure there
//! must never let the runtime silently conclude the store is intact when it
//! could not see part of it.
//!
//! This sweep populates a durable store, traces the read/list/metadata
//! positions the *open* path touches, then fails each one and asserts the
//! safety invariant: **a recovery-scan fault either fails the open with a
//! typed error, or leaves recovery health non-`Healthy` — never a
//! silently-`Healthy` open.** A false-`Healthy` would mean the runtime lost
//! track of objects without noticing.

use std::num::NonZeroU64;
use std::path::Path;

use strata_core::BranchId;

use crate::api::{
    CommitBatch, CommitMutation, CommitOptions, MaintenanceRequest, MaintenanceScope,
    MaintenanceTask, RecoveryHealthSummary, StorageBackend, StorageDurabilityPolicy, StorageKey,
    StorageMaintenanceSchedulingPolicy, StorageOpenOptions, StorageRuntime, StorageSpaceId,
    StorageValue,
};

use super::{BackendOperation, FaultKind, FaultRule, FaultScript, TestkitError};

/// The recovery-scan backend operations. These are exactly what the write-path
/// sweep excludes, and exactly what open recovery leans on.
const RECOVERY_READ_OPS: [BackendOperation; 3] = [
    BackendOperation::ListPrefix,
    BackendOperation::ReadObject,
    BackendOperation::ObjectMetadata,
];

/// Representative recoverable fault kinds a real backend surfaces mid-scan.
const FAULT_KINDS: [FaultKind; 2] = [FaultKind::Unavailable, FaultKind::Interrupted];

/// Counters describing a recovery-read fault sweep run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecoveryReadFaultOutcome {
    positions_swept: usize,
    open_failures: usize,
    degraded_opens: usize,
    healthy_opens: usize,
    op_kinds_swept: usize,
}

impl RecoveryReadFaultOutcome {
    /// Total (op, position, kind) cases the sweep failed and reopened under.
    #[must_use]
    pub const fn positions_swept(&self) -> usize {
        self.positions_swept
    }

    /// Cases where the recovery-scan fault failed the open with a typed error.
    #[must_use]
    pub const fn open_failures(&self) -> usize {
        self.open_failures
    }

    /// Cases where the open survived but reported degraded/failed health.
    #[must_use]
    pub const fn degraded_opens(&self) -> usize {
        self.degraded_opens
    }

    /// Cases where the fault did not fire (the position was not reached this
    /// run) and the open was cleanly healthy. These are not violations — the
    /// injected fault simply never landed.
    #[must_use]
    pub const fn healthy_opens(&self) -> usize {
        self.healthy_opens
    }

    /// How many of the three recovery-read op kinds the open path actually
    /// touched (and the sweep therefore exercised).
    #[must_use]
    pub const fn op_kinds_swept(&self) -> usize {
        self.op_kinds_swept
    }

    /// Cases where the injected fault fired (failed the open or degraded it).
    /// If this is zero the invariant was never actually tested.
    #[must_use]
    pub const fn faults_fired(&self) -> usize {
        self.open_failures + self.degraded_opens
    }
}

fn borrowed_options(durability: StorageDurabilityPolicy) -> StorageOpenOptions {
    StorageOpenOptions::durable_local(durability)
        .with_maintenance_scheduling_policy(StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue)
}

fn swept_op_count(backend: &StorageBackend, op: BackendOperation) -> usize {
    backend
        .fault_calls()
        .iter()
        .filter(|call| call.operation() == op)
        .count()
}

fn put(key: &[u8], value: &[u8]) -> CommitMutation {
    CommitMutation::Put {
        storage_space: StorageSpaceId::new(vec![0x20]).expect("engine space"),
        key: StorageKey::new(key.to_vec()).expect("key"),
        value: StorageValue::new(value.to_vec()),
        ttl: None,
    }
}

/// Populate a durable store on disk: several commits plus maintenance so the
/// backend holds a manifest, WAL, snapshots, and durable table objects — the
/// artifacts recovery must scan on reopen.
fn populate(root: &Path, branch: BranchId, seed: u64) -> Result<(), TestkitError> {
    let backend = StorageBackend::faulting_local_fs(root.to_path_buf(), FaultScript::empty());
    let runtime = StorageRuntime::open_with_backend(
        borrowed_options(StorageDurabilityPolicy::Always),
        &backend,
    )
    .map_err(|err| TestkitError::new(format!("populate open: {err:?}")))?;
    let mut runtime = runtime.into_runtime();

    for index in 0..8u64 {
        let key = format!("k{seed:x}-{index}");
        let value = format!("v{seed:x}-{index}");
        let batch = CommitBatch::new(
            branch,
            vec![put(key.as_bytes(), value.as_bytes())],
            CommitOptions::default(),
        )
        .map_err(|err| TestkitError::new(format!("populate batch: {err:?}")))?;
        runtime
            .commit(&batch)
            .map_err(|err| TestkitError::new(format!("populate commit: {err:?}")))?;
    }
    // Force durable artifacts: two checkpoints publish snapshots, pruning
    // deletes the older one.
    for task in [
        MaintenanceTask::Checkpoint,
        MaintenanceTask::Checkpoint,
        MaintenanceTask::SnapshotPruning,
    ] {
        let _ = runtime.enqueue_maintenance(&MaintenanceRequest::new(
            task,
            MaintenanceScope::Branch(branch),
        ));
        let _ =
            runtime.enqueue_maintenance(&MaintenanceRequest::new(task, MaintenanceScope::Global));
        let _ = runtime.drain_maintenance();
    }
    Ok(())
}

/// Reopen the populated store cleanly and count the read/list/metadata calls
/// the open path makes — the positions the sweep will fail.
fn trace_open_reads(root: &Path) -> Result<Vec<(BackendOperation, usize)>, TestkitError> {
    let backend = StorageBackend::faulting_local_fs(root.to_path_buf(), FaultScript::empty());
    {
        let _runtime = StorageRuntime::open_with_backend(
            borrowed_options(StorageDurabilityPolicy::Standard),
            &backend,
        )
        .map_err(|err| TestkitError::new(format!("trace open: {err:?}")))?;
    }
    Ok(RECOVERY_READ_OPS
        .into_iter()
        .filter_map(|op| {
            let count = swept_op_count(&backend, op);
            (count > 0).then_some((op, count))
        })
        .collect())
}

/// Fail one recovery-scan position and assert the safety invariant on reopen.
fn sweep_one(
    root: &Path,
    op: BackendOperation,
    position: usize,
    kind: FaultKind,
    outcome: &mut RecoveryReadFaultOutcome,
) {
    outcome.positions_swept += 1;
    let call_number = NonZeroU64::new(position as u64).expect("position >= 1");
    let backend = StorageBackend::faulting_local_fs(
        root.to_path_buf(),
        FaultScript::new([FaultRule::new(op, call_number, kind)]),
    );

    match StorageRuntime::open_with_backend(
        borrowed_options(StorageDurabilityPolicy::Standard),
        &backend,
    ) {
        Err(error) => {
            // A typed failure is safe: the runtime refused rather than pretend.
            assert!(
                !error.code().is_empty(),
                "recovery-scan fault produced an untyped failure: {error:?}"
            );
            outcome.open_failures += 1;
        }
        Ok(open) => {
            let health = open.summary().recovery_health();
            let fired = swept_op_count(&backend, op) >= position;
            if fired {
                // The fault landed and the open still succeeded: recovery MUST
                // have noticed it could not fully scan the store. A healthy
                // verdict here is the misclassification this sweep guards
                // against.
                assert_ne!(
                    health,
                    RecoveryHealthSummary::Healthy,
                    "a {op:?} fault at position {position} ({kind:?}) fired during \
                     recovery, yet the open reported Healthy — recovery lost track \
                     of objects without noticing"
                );
                outcome.degraded_opens += 1;
            } else {
                // The fault never fired (this reopen touched fewer ops than the
                // trace). A clean healthy open is correct.
                outcome.healthy_opens += 1;
            }
        }
    }
}

/// Run the recovery-read fault sweep for one seed. Populates a store, traces
/// the read/list/metadata open positions, and fails each under each fault kind,
/// asserting a recovery-scan fault never yields a silently-healthy open.
///
/// `root` is a caller-owned empty directory (typically a `tempfile::tempdir`).
///
/// # Errors
///
/// Returns [`TestkitError`] if the durable scaffold cannot be built. Safety
/// violations panic (they are test failures, not recoverable conditions).
pub fn run_recovery_read_fault_harness(
    root: &Path,
    seed: u64,
) -> Result<RecoveryReadFaultOutcome, TestkitError> {
    // The runtime creates DEFAULT_BRANCH_ID on open; commit against it.
    let branch = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);

    populate(root, branch, seed)?;
    let positions = trace_open_reads(root)?;
    assert!(
        !positions.is_empty(),
        "open recovery touched no read/list/metadata ops — the trace is wrong"
    );

    let mut outcome = RecoveryReadFaultOutcome {
        op_kinds_swept: positions.len(),
        ..RecoveryReadFaultOutcome::default()
    };
    for (op, count) in positions {
        for position in 1..=count {
            for kind in FAULT_KINDS {
                sweep_one(root, op, position, kind, &mut outcome);
            }
        }
    }
    Ok(outcome)
}

/// Run the recovery-read fault sweep across `seeds` seeds under one caller-owned
/// root, accumulating the outcome. The deep bug-hunt form of
/// [`run_recovery_read_fault_harness`].
///
/// # Errors
///
/// Returns [`TestkitError`] if the scaffold cannot be built for a seed.
pub fn run_recovery_read_fault_soak(
    root: &Path,
    seeds: u64,
) -> Result<RecoveryReadFaultOutcome, TestkitError> {
    let mut total = RecoveryReadFaultOutcome::default();
    for seed in 0..seeds {
        let dir = root.join(format!("seed-{seed:x}"));
        std::fs::create_dir_all(&dir)
            .map_err(|err| TestkitError::new(format!("create soak dir: {err}")))?;
        let outcome = run_recovery_read_fault_harness(&dir.join("db"), seed.wrapping_mul(0x9e37))?;
        total.positions_swept += outcome.positions_swept;
        total.open_failures += outcome.open_failures;
        total.degraded_opens += outcome.degraded_opens;
        total.healthy_opens += outcome.healthy_opens;
        total.op_kinds_swept = total.op_kinds_swept.max(outcome.op_kinds_swept);
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::run_recovery_read_fault_harness;

    #[test]
    fn recovery_scan_faults_never_produce_a_silently_healthy_open() {
        let temp = tempfile::tempdir().expect("tempdir");
        let outcome =
            run_recovery_read_fault_harness(&temp.path().join("db"), 0x3c3c).expect("harness runs");
        assert!(
            outcome.positions_swept() > 0,
            "swept no positions: {outcome:?}"
        );
        // Every fault that fired was accounted for as a failure or a degraded
        // open; the invariant assertions inside the sweep did the checking.
        assert_eq!(
            outcome.positions_swept(),
            outcome.open_failures() + outcome.degraded_opens() + outcome.healthy_opens(),
            "every swept case must be classified"
        );
        assert!(
            outcome.faults_fired() > 0,
            "no injected fault fired — the safety invariant was never exercised: {outcome:?}"
        );
        assert!(
            outcome.op_kinds_swept() >= 2,
            "open recovery touched fewer than two read/list/metadata op kinds              ({outcome:?}) — the sweep is thinner than intended"
        );
    }
}
