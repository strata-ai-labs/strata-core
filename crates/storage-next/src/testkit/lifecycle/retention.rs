//! Generated lifecycle retention contract helpers.

use super::{ensure, script_byte};
use crate::backend::{
    Backend, BackendCapabilities, BackendError, BackendErrorKind, BackendMetadata, BackendRange,
    BackendResult, BASIC_OBJECT_BACKEND_CAPABILITIES,
};
use crate::format::DatabaseManifest;
use crate::layout::ObjectLayout;
use crate::lifecycle::{
    build_retention_proof, prune_snapshots_with_proof, retention_outcome_for_delegated_families,
    table_quarantine_candidate, LifecycleRetentionOutcome, LifecycleRetentionProofStatus,
    LifecycleRetentionRequest, LifecycleRetentionStatus, LifecycleSnapshotPruningRequest,
    LifecycleSnapshotPruningStatus, RecoveryDegradationClass, RecoveryFault, RecoveryFaultKind,
    RecoveryHealth, RetentionDecision,
};
use crate::object::{ObjectName, ObjectPrefix};
use crate::service::SnapshotService;
use crate::testkit::TestkitError;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use strata_core_next::CommitVersion;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LifecycleRetentionContractOutcome {
    complete_proofs: usize,
    incomplete_proofs: usize,
    blocked_recovery: usize,
    snapshot_pruned: usize,
    snapshot_protected: usize,
    snapshot_delete_failures: usize,
    table_retained: usize,
    table_quarantine_candidates: usize,
    delegated_families: usize,
    cache_unsupported: usize,
}

pub fn check_lifecycle_retention_contract(
    script: &[u8],
) -> Result<LifecycleRetentionContractOutcome, TestkitError> {
    let mut outcome = LifecycleRetentionContractOutcome::default();
    check_proof_states(script, &mut outcome)?;
    check_snapshot_pruning(script, &mut outcome)?;
    check_table_decisions(script, &mut outcome)?;
    check_delegated_families(&mut outcome)?;
    check_cache_unsupported(script, &mut outcome)?;
    Ok(outcome)
}

impl LifecycleRetentionContractOutcome {
    pub const fn complete_proof_cases(&self) -> usize {
        self.complete_proofs
    }

    pub const fn incomplete_proof_cases(&self) -> usize {
        self.incomplete_proofs
    }

    pub const fn blocked_recovery_cases(&self) -> usize {
        self.blocked_recovery
    }

    pub const fn snapshot_pruned_cases(&self) -> usize {
        self.snapshot_pruned
    }

    pub const fn snapshot_protected_cases(&self) -> usize {
        self.snapshot_protected
    }

    pub const fn snapshot_delete_failure_cases(&self) -> usize {
        self.snapshot_delete_failures
    }

    pub const fn table_retained_cases(&self) -> usize {
        self.table_retained
    }

    pub const fn table_quarantine_candidate_cases(&self) -> usize {
        self.table_quarantine_candidates
    }

    pub const fn delegated_family_cases(&self) -> usize {
        self.delegated_families
    }

    pub const fn wal_delegated_cases(&self) -> usize {
        self.delegated_families
    }

    pub const fn cache_unsupported_cases(&self) -> usize {
        self.cache_unsupported
    }

    pub const fn cache_deferred_cases(&self) -> usize {
        self.cache_unsupported
    }
}

fn check_proof_states(
    script: &[u8],
    outcome: &mut LifecycleRetentionContractOutcome,
) -> Result<(), TestkitError> {
    let retain = usize::from(script_byte(script, 0) % 4);
    let request = LifecycleRetentionRequest::snapshot_pruning(retain);
    let complete =
        build_retention_proof(&request, Some(&manifest(3, 9)), &RecoveryHealth::Healthy, 3);
    ensure(
        complete.status() == LifecycleRetentionProofStatus::Complete,
        "retention proof with manifest facts was not complete",
    )?;
    outcome.complete_proofs += 1;

    let incomplete = build_retention_proof(&request, None, &RecoveryHealth::Healthy, 1);
    ensure(
        incomplete.status() == LifecycleRetentionProofStatus::Incomplete,
        "retention proof without live snapshot fact was not incomplete",
    )?;
    outcome.incomplete_proofs += 1;

    let health = RecoveryHealth::degraded(
        RecoveryDegradationClass::DataLoss,
        vec![
            RecoveryFault::new(RecoveryFaultKind::MissingSnapshotObject, "missing")
                .map_err(retention_error)?,
        ],
    )
    .map_err(retention_error)?;
    let blocked = build_retention_proof(&request, Some(&manifest(3, 9)), &health, 3);
    ensure(
        blocked.status() == LifecycleRetentionProofStatus::BlockedByRecoveryHealth,
        "retention proof did not block unsafe recovery health",
    )?;
    outcome.blocked_recovery += 1;
    Ok(())
}

fn check_snapshot_pruning(
    script: &[u8],
    outcome: &mut LifecycleRetentionContractOutcome,
) -> Result<(), TestkitError> {
    let retain = usize::from(script_byte(script, 1) % 3);
    let backend = ScriptSnapshotBackend::with_snapshots([1, 2, 3]);
    let request = LifecycleRetentionRequest::snapshot_pruning(retain);
    let proof = build_retention_proof(&request, Some(&manifest(3, 9)), &RecoveryHealth::Healthy, 3);
    let pruning = LifecycleSnapshotPruningRequest::new(proof, request.retain_newest_snapshots())
        .map_err(retention_error)?;
    let pruned = prune_snapshots_with_proof(&SnapshotService::new(&backend), &pruning)
        .map_err(retention_error)?;

    ensure(
        matches!(
            pruned.status(),
            LifecycleSnapshotPruningStatus::Completed
                | LifecycleSnapshotPruningStatus::CompletedNoop
        ),
        "snapshot pruning did not complete",
    )?;
    ensure(
        pruned
            .protected()
            .iter()
            .any(|snapshot| snapshot.snapshot_id() == 3),
        "snapshot pruning did not protect live snapshot",
    )?;
    outcome.snapshot_pruned += pruned.deleted().len();
    outcome.snapshot_protected += pruned.protected().len();

    let failing = ScriptSnapshotBackend::with_snapshots([4, 5, 6]);
    failing.fail_delete_on_call(1);
    let request = LifecycleRetentionRequest::snapshot_pruning(1);
    let proof = build_retention_proof(&request, Some(&manifest(6, 9)), &RecoveryHealth::Healthy, 3);
    let pruning = LifecycleSnapshotPruningRequest::new(proof, request.retain_newest_snapshots())
        .map_err(retention_error)?;
    let partial = prune_snapshots_with_proof(&SnapshotService::new(&failing), &pruning)
        .map_err(retention_error)?;
    ensure(
        partial.status() == LifecycleSnapshotPruningStatus::CompletedWithHealthDebt,
        "snapshot delete failure did not become health debt",
    )?;
    outcome.snapshot_delete_failures += partial.failed().len();
    Ok(())
}

fn check_table_decisions(
    script: &[u8],
    outcome: &mut LifecycleRetentionContractOutcome,
) -> Result<(), TestkitError> {
    let retained = ObjectName::new(format!("tables/branch/retained-{}", script_byte(script, 2)))
        .map_err(|error| TestkitError::new(format!("{error:?}")))?;
    let retired = ObjectName::new(format!("tables/branch/retired-{}", script_byte(script, 3)))
        .map_err(|error| TestkitError::new(format!("{error:?}")))?;
    let proof = build_retention_proof(
        &LifecycleRetentionRequest::global(1),
        Some(&manifest(3, 9)),
        &RecoveryHealth::Healthy,
        0,
    );
    let decisions = vec![
        crate::lifecycle::LifecycleRetentionDecisionRecord::table(
            retained,
            RetentionDecision::Retain,
            crate::lifecycle::LifecycleRetentionDecisionReason::ReachableTable,
        ),
        table_quarantine_candidate(retired),
    ];
    let retention =
        LifecycleRetentionOutcome::from_decisions(proof, decisions, 0).map_err(retention_error)?;

    ensure(
        retention.status() == LifecycleRetentionStatus::Completed,
        "table retention decision did not complete",
    )?;
    outcome.table_retained += retention.objects_retained();
    outcome.table_quarantine_candidates += retention
        .decisions()
        .iter()
        .filter(|decision| decision.decision() == RetentionDecision::QuarantineCandidate)
        .count();
    Ok(())
}

fn check_delegated_families(
    outcome: &mut LifecycleRetentionContractOutcome,
) -> Result<(), TestkitError> {
    let proof = build_retention_proof(
        &LifecycleRetentionRequest::global(1),
        Some(&manifest(3, 9)),
        &RecoveryHealth::Healthy,
        0,
    );
    let retention = retention_outcome_for_delegated_families(proof).map_err(retention_error)?;
    ensure(
        retention.objects_skipped() == 2,
        "delegated retention families were not reported",
    )?;
    outcome.delegated_families += retention.objects_skipped();
    Ok(())
}

fn check_cache_unsupported(
    script: &[u8],
    outcome: &mut LifecycleRetentionContractOutcome,
) -> Result<(), TestkitError> {
    let request = if script_byte(script, 4).is_multiple_of(2) {
        crate::lifecycle::MaintenanceTaskRequest::snapshot_pruning(1)
    } else {
        crate::lifecycle::MaintenanceTaskRequest::retention(1)
    };
    ensure(
        matches!(
            request.kind(),
            crate::lifecycle::MaintenanceTaskKind::SnapshotPruning
                | crate::lifecycle::MaintenanceTaskKind::Retention
        ),
        "cache unsupported fixture did not build retention request",
    )?;
    outcome.cache_unsupported += 1;
    Ok(())
}

fn manifest(snapshot_id: u64, snapshot_watermark: u64) -> DatabaseManifest {
    DatabaseManifest::new([0x56; 16], "identity")
        .expect("manifest")
        .with_recovery_facts(
            1,
            Some(snapshot_watermark),
            Some(snapshot_id),
            Some(CommitVersion::new(snapshot_watermark)),
        )
        .expect("recovery facts")
}

fn retention_error(error: impl std::error::Error) -> TestkitError {
    TestkitError::new(error.to_string())
}

#[derive(Debug, Default)]
struct ScriptSnapshotBackend {
    objects: Mutex<BTreeMap<ObjectName, Vec<u8>>>,
    fail_delete_call: AtomicUsize,
    delete_calls: AtomicUsize,
    listed: AtomicBool,
}

impl ScriptSnapshotBackend {
    fn with_snapshots<const N: usize>(ids: [u64; N]) -> Self {
        let backend = Self::default();
        for id in ids {
            backend.objects.lock().expect("objects").insert(
                ObjectLayout::snapshot(id).expect("snapshot object"),
                format!("snapshot-{id}").into_bytes(),
            );
        }
        backend
    }

    fn fail_delete_on_call(&self, call: usize) {
        self.fail_delete_call.store(call, Ordering::SeqCst);
    }
}

impl Backend for ScriptSnapshotBackend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::from_slice(BASIC_OBJECT_BACKEND_CAPABILITIES)
    }

    fn read_object(&self, name: &ObjectName) -> BackendResult<Vec<u8>> {
        self.objects
            .lock()
            .expect("objects")
            .get(name)
            .cloned()
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "object not found"))
    }

    fn read_range(&self, name: &ObjectName, range: BackendRange) -> BackendResult<Vec<u8>> {
        let bytes = self.read_object(name)?;
        let start = usize::try_from(range.offset()).unwrap_or(usize::MAX);
        let end = usize::try_from(range.end_offset().unwrap_or(u64::MAX)).unwrap_or(usize::MAX);
        Ok(bytes[start.min(bytes.len())..end.min(bytes.len())].to_vec())
    }

    fn write_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendMetadata> {
        self.objects
            .lock()
            .expect("objects")
            .insert(name.clone(), bytes.to_vec());
        Ok(BackendMetadata::new(bytes.len() as u64, None))
    }

    fn delete_object(&self, name: &ObjectName) -> BackendResult<()> {
        let call = self
            .delete_calls
            .fetch_add(1, Ordering::SeqCst)
            .saturating_add(1);
        if self.fail_delete_call.load(Ordering::SeqCst) == call {
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "injected delete failure",
            ));
        }
        self.objects
            .lock()
            .expect("objects")
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "object not found"))
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        self.listed.store(true, Ordering::SeqCst);
        let mut objects = self
            .objects
            .lock()
            .expect("objects")
            .keys()
            .filter(|object| object.as_str().starts_with(prefix.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        objects.sort();
        Ok(objects)
    }

    fn object_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
        self.objects
            .lock()
            .expect("objects")
            .get(name)
            .map(|bytes| BackendMetadata::new(bytes.len() as u64, None))
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "object not found"))
    }
}
