//! Generated lifecycle quarantine contract helpers.

use super::{ensure, script_byte};
use crate::backend::{
    Backend, BackendCapabilities, BackendCapability, BackendError, BackendErrorKind,
    BackendMetadata, BackendRange, BackendResult, PublishDurability, PublishError,
    PublishFailureKind, PublishMode, PublishOutcome, PublishResult,
};
use crate::layout::ObjectLayout;
use crate::lifecycle::{
    purge_quarantine, quarantine_object, repair_branch_quarantine,
    unsupported_quarantine_maintenance, LifecycleCodecId, LifecyclePurgeProof,
    LifecyclePurgeStatus, LifecycleQuarantineProof, LifecycleQuarantineProofStatus,
    LifecycleQuarantineStatus, MaintenanceOutcomeStatus, MaintenanceTaskKind,
    RecoveryDegradationClass, RecoveryFault, RecoveryFaultKind, RecoveryHealth, RetentionDecision,
};
use crate::object::{ObjectName, ObjectPrefix};
use crate::service::QuarantineService;
use crate::testkit::TestkitError;
use std::collections::BTreeMap;
use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use strata_core_next::{BranchId, Timestamp};

const DATABASE_ID: [u8; 16] = [0x71; 16];

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LifecycleQuarantineContractOutcome {
    complete_safe_proofs: usize,
    incomplete_proofs: usize,
    blocked_recovery: usize,
    referenced_candidates: usize,
    staged_objects: usize,
    already_quarantined: usize,
    inventory_publish_failures: usize,
    quarantine_publish_failures: usize,
    source_delete_failures: usize,
    purged_objects: usize,
    purge_delete_failures: usize,
    stale_purge_proofs: usize,
    corrupt_inventory_repairs: usize,
    unlisted_object_repairs: usize,
    cache_deferred: usize,
    input_derived_routes: usize,
    input_identity_rejections: usize,
    input_proof_deferrals: usize,
}

pub fn check_lifecycle_quarantine_contract(
    script: &[u8],
) -> Result<LifecycleQuarantineContractOutcome, TestkitError> {
    let mut outcome = LifecycleQuarantineContractOutcome::default();
    check_proof_states(&mut outcome)?;
    check_quarantine_staging(script, &mut outcome)?;
    check_quarantine_publish_failures(script, &mut outcome)?;
    check_source_delete_failure(script, &mut outcome)?;
    check_purge(script, &mut outcome)?;
    check_repair(script, &mut outcome)?;
    check_cache_deferred(&mut outcome)?;
    check_input_route(script, &mut outcome)?;
    Ok(outcome)
}

impl LifecycleQuarantineContractOutcome {
    pub const fn complete_safe_proof_cases(&self) -> usize {
        self.complete_safe_proofs
    }

    pub const fn incomplete_proof_cases(&self) -> usize {
        self.incomplete_proofs
    }

    pub const fn blocked_recovery_cases(&self) -> usize {
        self.blocked_recovery
    }

    pub const fn referenced_candidate_cases(&self) -> usize {
        self.referenced_candidates
    }

    pub const fn staged_object_cases(&self) -> usize {
        self.staged_objects
    }

    pub const fn already_quarantined_cases(&self) -> usize {
        self.already_quarantined
    }

    pub const fn inventory_publish_failure_cases(&self) -> usize {
        self.inventory_publish_failures
    }

    pub const fn quarantine_publish_failure_cases(&self) -> usize {
        self.quarantine_publish_failures
    }

    pub const fn source_delete_failure_cases(&self) -> usize {
        self.source_delete_failures
    }

    pub const fn purged_object_cases(&self) -> usize {
        self.purged_objects
    }

    pub const fn purge_delete_failure_cases(&self) -> usize {
        self.purge_delete_failures
    }

    pub const fn stale_purge_proof_cases(&self) -> usize {
        self.stale_purge_proofs
    }

    pub const fn corrupt_inventory_repair_cases(&self) -> usize {
        self.corrupt_inventory_repairs
    }

    pub const fn unlisted_object_repair_cases(&self) -> usize {
        self.unlisted_object_repairs
    }

    pub const fn cache_deferred_cases(&self) -> usize {
        self.cache_deferred
    }

    pub const fn input_derived_route_cases(&self) -> usize {
        self.input_derived_routes
    }

    pub const fn input_identity_rejection_cases(&self) -> usize {
        self.input_identity_rejections
    }

    pub const fn input_proof_deferral_cases(&self) -> usize {
        self.input_proof_deferrals
    }
}

fn check_proof_states(
    outcome: &mut LifecycleQuarantineContractOutcome,
) -> Result<(), TestkitError> {
    let safe = LifecycleQuarantineProof::from_retention_decision(
        RetentionDecision::QuarantineCandidate,
        RecoveryHealth::Healthy,
    );
    ensure(
        safe.status() == LifecycleQuarantineProofStatus::CompleteSafe,
        "candidate proof was not complete-safe",
    )?;
    outcome.complete_safe_proofs += 1;

    let referenced = LifecycleQuarantineProof::from_retention_decision(
        RetentionDecision::Retain,
        RecoveryHealth::Healthy,
    );
    ensure(
        referenced.status() == LifecycleQuarantineProofStatus::Referenced,
        "retained candidate was not deferred as referenced",
    )?;
    outcome.referenced_candidates += 1;

    let incomplete = LifecycleQuarantineProof::from_retention_decision(
        RetentionDecision::SkipUntilProof,
        RecoveryHealth::Healthy,
    );
    ensure(
        incomplete.status() == LifecycleQuarantineProofStatus::Incomplete,
        "skip-until-proof candidate was not incomplete",
    )?;
    outcome.incomplete_proofs += 1;

    let blocked = LifecycleQuarantineProof::from_retention_decision(
        RetentionDecision::QuarantineCandidate,
        unsafe_health()?,
    );
    ensure(
        blocked.status() == LifecycleQuarantineProofStatus::BlockedByRecoveryHealth,
        "unsafe recovery health did not block quarantine proof",
    )?;
    outcome.blocked_recovery += 1;
    Ok(())
}

fn check_quarantine_staging(
    script: &[u8],
    outcome: &mut LifecycleQuarantineContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 0));
    let source = source_object(branch, "stage");
    let backend = ScriptQuarantineBackend::new();
    backend.put_object(source.clone(), b"table");
    let service = QuarantineService::new(&backend);
    let request = quarantine_request(branch, source.clone(), "stage").map_err(quarantine_error)?;
    let staged = quarantine_object(&service, &request);

    ensure(
        staged.status() == LifecycleQuarantineStatus::QuarantinedSourceDeleted,
        "safe quarantine did not stage and delete source",
    )?;
    ensure(!backend.contains(&source), "source object was not deleted")?;
    ensure(
        staged.quarantine_object().is_some() && staged.inventory_object().is_some(),
        "staged outcome did not report durable objects",
    )?;
    outcome.staged_objects += 1;

    let retry = quarantine_object(&service, &request);
    ensure(
        retry.status() == LifecycleQuarantineStatus::AlreadyQuarantined,
        "matching quarantine retry was not idempotent",
    )?;
    outcome.already_quarantined += 1;
    Ok(())
}

fn check_quarantine_publish_failures(
    script: &[u8],
    outcome: &mut LifecycleQuarantineContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 1));
    let inventory_source = source_object(branch, "inventory-fault");
    let inventory_backend = ScriptQuarantineBackend::new();
    inventory_backend.put_object(inventory_source.clone(), b"table");
    inventory_backend.fail_publish_call(1, PublishFailureKind::FailedBeforeVisibility);
    let inventory_outcome = quarantine_object(
        &QuarantineService::new(&inventory_backend),
        &quarantine_request(branch, inventory_source, "inventory-fault")
            .map_err(quarantine_error)?,
    );
    ensure(
        inventory_outcome.status() == LifecycleQuarantineStatus::InventoryPublishFailed,
        "inventory publish fault was not classified",
    )?;
    ensure(
        inventory_outcome
            .source_error()
            .and_then(Error::source)
            .is_some(),
        "inventory publish fault lost source chain",
    )?;
    outcome.inventory_publish_failures += 1;

    let object_source = source_object(branch, "object-fault");
    let object_backend = ScriptQuarantineBackend::new();
    object_backend.put_object(object_source.clone(), b"table");
    object_backend.fail_publish_call(2, PublishFailureKind::FailedBeforeVisibility);
    let object_outcome = quarantine_object(
        &QuarantineService::new(&object_backend),
        &quarantine_request(branch, object_source, "object-fault").map_err(quarantine_error)?,
    );
    ensure(
        object_outcome.status() == LifecycleQuarantineStatus::QuarantinePublishFailed,
        "quarantine object publish fault was not classified",
    )?;
    ensure(
        object_outcome
            .inventory_object()
            .is_some_and(|object| object_backend.contains(object)),
        "quarantine object publish fault did not leave inventory evidence",
    )?;
    outcome.quarantine_publish_failures += 1;
    Ok(())
}

fn check_source_delete_failure(
    script: &[u8],
    outcome: &mut LifecycleQuarantineContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 2));
    let source = source_object(branch, "delete-fault");
    let backend = ScriptQuarantineBackend::new();
    backend.put_object(source.clone(), b"table");
    backend.fail_delete(source.clone(), BackendErrorKind::Interrupted);
    let result = quarantine_object(
        &QuarantineService::new(&backend),
        &quarantine_request(branch, source.clone(), "delete-fault").map_err(quarantine_error)?,
    );

    ensure(
        result.status() == LifecycleQuarantineStatus::QuarantinedSourceDeleteFailed,
        "source delete fault was not reported",
    )?;
    ensure(
        backend.contains(&source),
        "source delete fault removed source",
    )?;
    ensure(
        result.source_error().and_then(Error::source).is_some(),
        "source delete fault lost backend source",
    )?;
    outcome.source_delete_failures += 1;
    Ok(())
}

fn check_purge(
    script: &[u8],
    outcome: &mut LifecycleQuarantineContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 3));
    let stale = purge_quarantine(
        &QuarantineService::new(&ScriptQuarantineBackend::new()),
        branch,
        DATABASE_ID,
        &LifecycleCodecId::identity(),
        &LifecyclePurgeProof::stale(RecoveryHealth::Healthy),
    )
    .map_err(quarantine_error)?;
    ensure(
        stale.status() == LifecyclePurgeStatus::StaleProof,
        "stale purge proof was not deferred",
    )?;
    outcome.stale_purge_proofs += 1;

    let source = source_object(branch, "purge");
    let backend = ScriptQuarantineBackend::new();
    backend.put_object(source.clone(), b"table");
    let service = QuarantineService::new(&backend);
    let quarantined = quarantine_object(
        &service,
        &quarantine_request(branch, source, "purge").map_err(quarantine_error)?,
    );
    let quarantine_name = quarantined.quarantine_object().expect("object").clone();
    let purged = purge_quarantine(
        &service,
        branch,
        DATABASE_ID,
        &LifecycleCodecId::identity(),
        &fresh_purge_proof(&service, branch, RecoveryHealth::Healthy)?,
    )
    .map_err(quarantine_error)?;
    ensure(
        purged.status() == LifecyclePurgeStatus::Completed,
        "fresh purge did not complete",
    )?;
    ensure(
        purged.deleted_objects().contains(&quarantine_name),
        "purge did not delete listed quarantine object",
    )?;
    outcome.purged_objects += purged.deleted_objects().len();

    let failing_source = source_object(branch, "purge-fault");
    let failing = ScriptQuarantineBackend::new();
    failing.put_object(failing_source.clone(), b"table");
    let failing_service = QuarantineService::new(&failing);
    let failing_quarantine = quarantine_object(
        &failing_service,
        &quarantine_request(branch, failing_source, "purge-fault").map_err(quarantine_error)?,
    );
    let failing_object = failing_quarantine
        .quarantine_object()
        .expect("object")
        .clone();
    failing.fail_delete(failing_object.clone(), BackendErrorKind::Unavailable);
    let partial = purge_quarantine(
        &failing_service,
        branch,
        DATABASE_ID,
        &LifecycleCodecId::identity(),
        &fresh_purge_proof(&failing_service, branch, RecoveryHealth::Healthy)?,
    )
    .map_err(quarantine_error)?;
    ensure(
        partial.status() == LifecyclePurgeStatus::CompletedWithHealthDebt,
        "purge delete fault did not produce health debt",
    )?;
    ensure(
        partial.failed_objects().contains(&failing_object),
        "purge delete fault did not report failed object",
    )?;
    outcome.purge_delete_failures += partial.failed_objects().len();
    Ok(())
}

fn check_repair(
    script: &[u8],
    outcome: &mut LifecycleQuarantineContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 4));
    let corrupt = ScriptQuarantineBackend::new();
    corrupt.put_object(
        ObjectLayout::quarantine_manifest(&branch.to_string())
            .map_err(|error| TestkitError::new(error.to_string()))?,
        b"not-an-inventory",
    );
    let corrupt_report = repair_branch_quarantine(
        &QuarantineService::new(&corrupt),
        branch,
        DATABASE_ID,
        &LifecycleCodecId::identity(),
    )
    .map_err(quarantine_error)?;
    ensure(
        corrupt_report.completed_with_health_debt(),
        "corrupt quarantine inventory did not produce repair health debt",
    )?;
    ensure(
        corrupt_report.inventory_present_reports() > 0,
        "corrupt inventory report did not preserve inventory fact",
    )?;
    outcome.corrupt_inventory_repairs += 1;

    let unlisted = ScriptQuarantineBackend::new();
    unlisted.put_object(
        ObjectLayout::quarantine_object(&branch.to_string(), "unlisted")
            .map_err(|error| TestkitError::new(error.to_string()))?,
        b"table",
    );
    let unlisted_report = repair_branch_quarantine(
        &QuarantineService::new(&unlisted),
        branch,
        DATABASE_ID,
        &LifecycleCodecId::identity(),
    )
    .map_err(quarantine_error)?;
    ensure(
        unlisted_report.completed_with_health_debt(),
        "unlisted quarantine object did not produce repair health debt",
    )?;
    ensure(
        unlisted_report.unlisted_objects() > 0,
        "unlisted quarantine object was not reported",
    )?;
    outcome.unlisted_object_repairs += 1;
    Ok(())
}

fn check_cache_deferred(
    outcome: &mut LifecycleQuarantineContractOutcome,
) -> Result<(), TestkitError> {
    for kind in [
        MaintenanceTaskKind::Quarantine,
        MaintenanceTaskKind::Purge,
        MaintenanceTaskKind::Repair,
    ] {
        let deferred = unsupported_quarantine_maintenance(kind);
        ensure(
            deferred.status() == MaintenanceOutcomeStatus::Deferred,
            "cache reclaim outcome was not deferred",
        )?;
        outcome.cache_deferred += 1;
    }
    Ok(())
}

fn check_input_route(
    script: &[u8],
    outcome: &mut LifecycleQuarantineContractOutcome,
) -> Result<(), TestkitError> {
    outcome.input_derived_routes += 1;
    if script_byte(script, 6) % 5 == 0 {
        check_cache_deferred(outcome)?;
        return Ok(());
    }

    let branch = branch_id(script_byte(script, 7));
    let database_id = if script_byte(script, 8) % 7 == 0 {
        [0; 16]
    } else {
        DATABASE_ID
    };
    let source = input_source_object(branch, script_byte(script, 9))?;
    let proof = input_quarantine_proof(script_byte(script, 10))?;
    let request = crate::lifecycle::LifecycleQuarantineRequest::new(
        branch,
        database_id,
        LifecycleCodecId::identity(),
        "input-derived",
        source.clone(),
        Timestamp::from_micros(10_000),
        proof.clone(),
    );
    let Ok(request) = request else {
        outcome.input_identity_rejections += 1;
        return Ok(());
    };

    if proof.status() != LifecycleQuarantineProofStatus::CompleteSafe {
        let result = quarantine_object(
            &QuarantineService::new(&ScriptQuarantineBackend::new()),
            &request,
        );
        ensure(
            matches!(
                result.status(),
                LifecycleQuarantineStatus::DeferredReferenced
                    | LifecycleQuarantineStatus::DeferredIncompleteProof
                    | LifecycleQuarantineStatus::BlockedByRecoveryHealth
            ),
            "input-derived proof did not defer unsafe quarantine",
        )?;
        outcome.input_proof_deferrals += 1;
        return Ok(());
    }

    match script_byte(script, 5) % 5 {
        0 => input_quarantine_route(script, source, &request, outcome),
        1 => input_purge_route(branch, source, &request, outcome),
        2 => input_repair_route(script, branch, outcome),
        3 => check_source_delete_failure(script, outcome),
        _ => check_quarantine_publish_failures(script, outcome),
    }
}

fn input_quarantine_route(
    script: &[u8],
    source: ObjectName,
    request: &crate::lifecycle::LifecycleQuarantineRequest,
    outcome: &mut LifecycleQuarantineContractOutcome,
) -> Result<(), TestkitError> {
    let backend = ScriptQuarantineBackend::new();
    backend.put_object(source.clone(), b"input-table");
    match script_byte(script, 11) % 4 {
        1 => backend.fail_publish_call(1, PublishFailureKind::FailedBeforeVisibility),
        2 => backend.fail_publish_call(2, PublishFailureKind::FailedBeforeVisibility),
        3 => backend.fail_delete(source, BackendErrorKind::Interrupted),
        _ => {}
    }
    let result = quarantine_object(&QuarantineService::new(&backend), request);
    match result.status() {
        LifecycleQuarantineStatus::QuarantinedSourceDeleted => outcome.staged_objects += 1,
        LifecycleQuarantineStatus::InventoryPublishFailed => {
            outcome.inventory_publish_failures += 1;
        }
        LifecycleQuarantineStatus::QuarantinePublishFailed => {
            outcome.quarantine_publish_failures += 1;
        }
        LifecycleQuarantineStatus::QuarantinedSourceDeleteFailed => {
            outcome.source_delete_failures += 1;
        }
        _ => {}
    }
    ensure(
        result.maintenance_outcome().status() != MaintenanceOutcomeStatus::Deferred,
        "input quarantine route unexpectedly deferred complete proof",
    )
}

fn input_purge_route(
    branch: BranchId,
    source: ObjectName,
    request: &crate::lifecycle::LifecycleQuarantineRequest,
    outcome: &mut LifecycleQuarantineContractOutcome,
) -> Result<(), TestkitError> {
    let backend = ScriptQuarantineBackend::new();
    backend.put_object(source, b"input-table");
    let service = QuarantineService::new(&backend);
    let staged = quarantine_object(&service, request);
    ensure(
        staged.status() == LifecycleQuarantineStatus::QuarantinedSourceDeleted,
        "input purge setup did not quarantine source",
    )?;
    let purge = purge_quarantine(
        &service,
        branch,
        DATABASE_ID,
        &LifecycleCodecId::identity(),
        &fresh_purge_proof(&service, branch, RecoveryHealth::Healthy)?,
    )
    .map_err(quarantine_error)?;
    ensure(
        purge.status() == LifecyclePurgeStatus::Completed,
        "input purge route did not complete",
    )?;
    outcome.purged_objects += purge.deleted_objects().len();
    Ok(())
}

fn input_repair_route(
    script: &[u8],
    branch: BranchId,
    outcome: &mut LifecycleQuarantineContractOutcome,
) -> Result<(), TestkitError> {
    let backend = ScriptQuarantineBackend::new();
    match script_byte(script, 13) % 3 {
        0 => backend.put_object(
            ObjectLayout::quarantine_manifest(&branch.to_string())
                .map_err(|error| TestkitError::new(error.to_string()))?,
            b"not-an-inventory",
        ),
        1 => backend.put_object(
            ObjectLayout::quarantine_object(&branch.to_string(), "input-unlisted")
                .map_err(|error| TestkitError::new(error.to_string()))?,
            b"table",
        ),
        _ => {}
    }
    let repair = repair_branch_quarantine(
        &QuarantineService::new(&backend),
        branch,
        DATABASE_ID,
        &LifecycleCodecId::identity(),
    )
    .map_err(quarantine_error)?;
    if repair.backend_unavailable() {
        return Err(TestkitError::new("input repair route backend failed"));
    }
    if repair.completed_with_health_debt() {
        if repair.inventory_present_reports() > 0 {
            outcome.corrupt_inventory_repairs += 1;
        }
        if repair.unlisted_objects() > 0 {
            outcome.unlisted_object_repairs += 1;
        }
    }
    Ok(())
}

fn input_quarantine_proof(seed: u8) -> Result<LifecycleQuarantineProof, TestkitError> {
    match seed % 4 {
        0 => Ok(LifecycleQuarantineProof::from_retention_decision(
            RetentionDecision::QuarantineCandidate,
            RecoveryHealth::Healthy,
        )),
        1 => Ok(LifecycleQuarantineProof::from_retention_decision(
            RetentionDecision::Retain,
            RecoveryHealth::Healthy,
        )),
        2 => Ok(LifecycleQuarantineProof::from_retention_decision(
            RetentionDecision::SkipUntilProof,
            RecoveryHealth::Healthy,
        )),
        _ => Ok(LifecycleQuarantineProof::from_retention_decision(
            RetentionDecision::QuarantineCandidate,
            unsafe_health()?,
        )),
    }
}

fn input_source_object(branch: BranchId, seed: u8) -> Result<ObjectName, TestkitError> {
    match seed % 4 {
        0 => Ok(source_object(branch, "input-table")),
        1 => ObjectLayout::snapshot(u64::from(seed).saturating_add(1))
            .map_err(|error| TestkitError::new(error.to_string())),
        2 => ObjectLayout::wal_segment(u64::from(seed))
            .map_err(|error| TestkitError::new(error.to_string())),
        _ => ObjectLayout::quarantine_object(&branch.to_string(), "bad-source")
            .map_err(|error| TestkitError::new(error.to_string())),
    }
}

fn quarantine_request(
    branch: BranchId,
    source: ObjectName,
    object_id: &str,
) -> Result<crate::lifecycle::LifecycleQuarantineRequest, crate::lifecycle::LifecycleError> {
    crate::lifecycle::LifecycleQuarantineRequest::new(
        branch,
        DATABASE_ID,
        LifecycleCodecId::identity(),
        object_id,
        source,
        Timestamp::from_micros(10_000),
        LifecycleQuarantineProof::safe(RecoveryHealth::Healthy),
    )
}

fn unsafe_health() -> Result<RecoveryHealth, TestkitError> {
    RecoveryHealth::degraded(
        RecoveryDegradationClass::DataLoss,
        vec![
            RecoveryFault::new(RecoveryFaultKind::MissingTableObject, "missing table")
                .map_err(quarantine_error)?,
        ],
    )
    .map_err(quarantine_error)
}

fn branch_id(seed: u8) -> BranchId {
    BranchId::from_bytes([seed.max(1); BranchId::BYTE_LEN])
}

fn source_object(branch: BranchId, suffix: &str) -> ObjectName {
    ObjectLayout::table_object(&branch.to_string(), 0, suffix).expect("table object")
}

fn fresh_purge_proof(
    service: &QuarantineService<'_>,
    branch: BranchId,
    recovery_health: RecoveryHealth,
) -> Result<LifecyclePurgeProof, TestkitError> {
    let token = service
        .load_inventory(branch, DATABASE_ID, LifecycleCodecId::identity().as_str())
        .map_err(quarantine_error)?
        .token();
    Ok(LifecyclePurgeProof::fresh(recovery_health, token))
}

fn quarantine_error(error: impl std::error::Error) -> TestkitError {
    TestkitError::new(error.to_string())
}

#[derive(Debug, Default)]
struct ScriptQuarantineBackend {
    objects: Mutex<BTreeMap<ObjectName, Vec<u8>>>,
    delete_failures: Mutex<BTreeMap<ObjectName, BackendErrorKind>>,
    publish_failures: Mutex<BTreeMap<usize, PublishFailureKind>>,
    publish_calls: AtomicUsize,
}

impl ScriptQuarantineBackend {
    fn new() -> Self {
        Self::default()
    }

    fn put_object(&self, object: ObjectName, bytes: &[u8]) {
        self.objects
            .lock()
            .expect("objects")
            .insert(object, bytes.to_vec());
    }

    fn contains(&self, object: &ObjectName) -> bool {
        self.objects.lock().expect("objects").contains_key(object)
    }

    fn fail_delete(&self, object: ObjectName, kind: BackendErrorKind) {
        self.delete_failures
            .lock()
            .expect("delete failures")
            .insert(object, kind);
    }

    fn fail_publish_call(&self, call: usize, kind: PublishFailureKind) {
        self.publish_failures
            .lock()
            .expect("publish failures")
            .insert(call, kind);
    }
}

impl Backend for ScriptQuarantineBackend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::from_slice(&[
            BackendCapability::ReadObject,
            BackendCapability::ReadRange,
            BackendCapability::WriteObject,
            BackendCapability::DeleteObject,
            BackendCapability::ListPrefix,
            BackendCapability::ObjectMetadata,
            BackendCapability::DurablePublish,
            BackendCapability::DurableSync,
        ])
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
        self.put_object(name.clone(), bytes);
        Ok(BackendMetadata::new(bytes.len() as u64, None))
    }

    fn delete_object(&self, name: &ObjectName) -> crate::backend::DeleteResult {
        if let Some(kind) = self
            .delete_failures
            .lock()
            .expect("delete failures")
            .get(name)
            .copied()
        {
            return crate::backend::failed_delete_result(
                name,
                BackendError::new(kind, "delete failed"),
            );
        }
        let removed = self.objects.lock().expect("objects").remove(name).is_some();
        crate::backend::durable_delete_result(name, removed)
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        let mut names = self
            .objects
            .lock()
            .expect("objects")
            .keys()
            .filter(|object| object.as_str().starts_with(prefix.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        names.sort();
        Ok(names)
    }

    fn object_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
        self.objects
            .lock()
            .expect("objects")
            .get(name)
            .map(|bytes| BackendMetadata::new(bytes.len() as u64, None))
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "object not found"))
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        bytes: &[u8],
        mode: PublishMode,
    ) -> PublishResult<PublishOutcome> {
        let call = self
            .publish_calls
            .fetch_add(1, Ordering::SeqCst)
            .saturating_add(1);
        if let Some(kind) = self
            .publish_failures
            .lock()
            .expect("publish failures")
            .get(&call)
            .copied()
        {
            return Err(PublishError::new(
                name.clone(),
                kind,
                BackendError::new(BackendErrorKind::Unavailable, "publish failed"),
            ));
        }
        let mut objects = self.objects.lock().expect("objects");
        if mode == PublishMode::Create && objects.contains_key(name) {
            return Err(PublishError::precondition_failed(
                name,
                "object already exists",
            ));
        }
        objects.insert(name.clone(), bytes.to_vec());
        Ok(PublishOutcome::new(
            name.clone(),
            BackendMetadata::new(bytes.len() as u64, None),
            PublishDurability::Durable,
        ))
    }
}
