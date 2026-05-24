//! Lifecycle quarantine, purge, and repair orchestration.

#![allow(
    dead_code,
    reason = "quarantine orchestration hooks are consumed by durable maintenance and tests"
)]

use super::{
    telemetry_health_debt, LifecycleCodecId, LifecycleError, LifecycleLowerLayer, LifecycleResult,
    LifecycleStats, MaintenanceOutcome, MaintenanceOutcomeStatus, MaintenanceTask,
    MaintenanceTaskKind, RecoveryDegradationClass, RecoveryFault, RecoveryFaultKind,
    RecoveryHealth, RetentionDecision,
};
use crate::format::quarantine::QuarantineEntry;
use crate::layout::{ObjectFamily, ObjectLayout};
use crate::object::ObjectName;
use crate::service::{
    QuarantineDeleteOutcome, QuarantineFamilyReconciliation, QuarantineGate,
    QuarantineObjectReport, QuarantineObjectRequest, QuarantineObjectStatus, QuarantinePurgeReport,
    QuarantinePurgeRequest, QuarantineReconciliationKind, QuarantineReconciliationReport,
    QuarantineRecoveryClass, QuarantineService, QuarantineServiceError,
};
use sha2::{Digest, Sha256};
use strata_core_next::{BranchId, Timestamp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleQuarantineProof {
    status: LifecycleQuarantineProofStatus,
    recovery_health: RecoveryHealth,
    source_reachable: bool,
    missing_fact: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum LifecycleQuarantineProofStatus {
    CompleteSafe,
    Referenced,
    Incomplete,
    BlockedByRecoveryHealth,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleQuarantineRequest {
    branch_id: BranchId,
    database_id: [u8; 16],
    codec_id: LifecycleCodecId,
    object_id: String,
    source_object: ObjectName,
    staged_at: Timestamp,
    proof: LifecycleQuarantineProof,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleQuarantineOutcome {
    status: LifecycleQuarantineStatus,
    branch_id: BranchId,
    source_object: Option<ObjectName>,
    quarantine_object: Option<ObjectName>,
    inventory_object: Option<ObjectName>,
    byte_count: u64,
    entry_count: usize,
    recovery_health: Option<RecoveryHealth>,
    source_error: Option<LifecycleError>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum LifecycleQuarantineStatus {
    QuarantinedSourceDeleted,
    AlreadyQuarantined,
    SourceDeleteRetried,
    SourceAlreadyMissingAfterPublish,
    QuarantinedSourceDeleteFailed,
    DeferredReferenced,
    DeferredIncompleteProof,
    BlockedByRecoveryHealth,
    InventoryPublishFailed,
    InventoryPublishUncertain,
    QuarantinePublishFailed,
    QuarantinePublishUncertain,
    InventoryMismatch,
    ServiceFailed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecyclePurgeProof {
    status: LifecyclePurgeProofStatus,
    recovery_health: RecoveryHealth,
    stale: bool,
    missing_fact: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum LifecyclePurgeProofStatus {
    CompleteFresh,
    Stale,
    Incomplete,
    BlockedByRecoveryHealth,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecyclePurgeRequest {
    branch_id: BranchId,
    database_id: [u8; 16],
    codec_id: LifecycleCodecId,
    proof: LifecyclePurgeProof,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecyclePurgeOutcome {
    status: LifecyclePurgeStatus,
    branch_id: BranchId,
    inventory_object: Option<ObjectName>,
    deleted_objects: Vec<ObjectName>,
    already_missing_objects: Vec<ObjectName>,
    failed_objects: Vec<ObjectName>,
    retained_entries: Vec<QuarantineEntry>,
    reclaimed_bytes: u64,
    recovery_health: Option<RecoveryHealth>,
    source_error: Option<LifecycleError>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum LifecyclePurgeStatus {
    Completed,
    CompletedNoop,
    CompletedWithHealthDebt,
    DeferredIncompleteProof,
    BlockedByRecoveryHealth,
    StaleProof,
    InventoryRewriteFailed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleQuarantineRepairRequest {
    scope: LifecycleQuarantineRepairScope,
    database_id: [u8; 16],
    codec_id: LifecycleCodecId,
    allow_mutation: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum LifecycleQuarantineRepairScope {
    Branch(BranchId),
    Family,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleQuarantineRepairOutcome {
    status: LifecycleQuarantineRepairStatus,
    reports: Vec<LifecycleQuarantineRepairReport>,
    recovery_health: Option<RecoveryHealth>,
    source_error: Option<LifecycleError>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum LifecycleQuarantineRepairStatus {
    CompletedClean,
    CompletedWithHealthDebt,
    BackendUnavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleQuarantineRepairReport {
    branch_id: BranchId,
    kind: QuarantineReconciliationKind,
    listed_objects: usize,
    missing_objects: usize,
    unlisted_objects: usize,
    malformed_objects: usize,
    inventory_present: bool,
}

impl LifecycleQuarantineProof {
    pub(crate) fn new(
        status: LifecycleQuarantineProofStatus,
        recovery_health: RecoveryHealth,
        source_reachable: bool,
        missing_fact: Option<&'static str>,
    ) -> Self {
        Self {
            status,
            recovery_health,
            source_reachable,
            missing_fact,
        }
    }

    pub(crate) fn safe(recovery_health: RecoveryHealth) -> Self {
        let status = if recovery_health_blocks_reclaim(&recovery_health) {
            LifecycleQuarantineProofStatus::BlockedByRecoveryHealth
        } else {
            LifecycleQuarantineProofStatus::CompleteSafe
        };
        Self::new(status, recovery_health, false, None)
    }

    pub(crate) fn from_retention_decision(
        decision: RetentionDecision,
        recovery_health: RecoveryHealth,
    ) -> Self {
        if recovery_health_blocks_reclaim(&recovery_health) {
            return Self::new(
                LifecycleQuarantineProofStatus::BlockedByRecoveryHealth,
                recovery_health,
                false,
                Some("recovery_health"),
            );
        }
        match decision {
            RetentionDecision::QuarantineCandidate => Self::new(
                LifecycleQuarantineProofStatus::CompleteSafe,
                recovery_health,
                false,
                None,
            ),
            RetentionDecision::Retain => Self::new(
                LifecycleQuarantineProofStatus::Referenced,
                recovery_health,
                true,
                None,
            ),
            RetentionDecision::SkipUntilProof => Self::new(
                LifecycleQuarantineProofStatus::Incomplete,
                recovery_health,
                false,
                Some("retention_proof"),
            ),
            RetentionDecision::PruneCandidate | RetentionDecision::PurgeCandidate => Self::new(
                LifecycleQuarantineProofStatus::Incomplete,
                recovery_health,
                false,
                Some("quarantine_candidate"),
            ),
        }
    }

    pub(crate) const fn status(&self) -> LifecycleQuarantineProofStatus {
        self.status
    }

    pub(crate) const fn recovery_health(&self) -> &RecoveryHealth {
        &self.recovery_health
    }

    pub(crate) const fn source_reachable(&self) -> bool {
        self.source_reachable
    }

    pub(crate) const fn missing_fact(&self) -> Option<&'static str> {
        self.missing_fact
    }

    const fn gate(&self) -> QuarantineGate {
        match self.status {
            LifecycleQuarantineProofStatus::CompleteSafe => QuarantineGate::Safe,
            LifecycleQuarantineProofStatus::Referenced => QuarantineGate::Referenced,
            LifecycleQuarantineProofStatus::Incomplete => QuarantineGate::ProofIncomplete,
            LifecycleQuarantineProofStatus::BlockedByRecoveryHealth => {
                QuarantineGate::UnsafeRecovery
            }
        }
    }
}

impl LifecycleQuarantineRequest {
    pub(crate) fn new(
        branch_id: BranchId,
        database_id: [u8; 16],
        codec_id: LifecycleCodecId,
        object_id: impl Into<String>,
        source_object: ObjectName,
        staged_at: Timestamp,
        proof: LifecycleQuarantineProof,
    ) -> LifecycleResult<Self> {
        let request = Self {
            branch_id,
            database_id,
            codec_id,
            object_id: object_id.into(),
            source_object,
            staged_at,
            proof,
        };
        request.validate()?;
        Ok(request)
    }

    pub(crate) fn from_source_object(
        branch_id: BranchId,
        database_id: [u8; 16],
        codec_id: LifecycleCodecId,
        source_object: ObjectName,
        staged_at: Timestamp,
        proof: LifecycleQuarantineProof,
    ) -> LifecycleResult<Self> {
        let object_id = derived_quarantine_object_id(&source_object);
        Self::new(
            branch_id,
            database_id,
            codec_id,
            object_id,
            source_object,
            staged_at,
            proof,
        )
    }

    fn validate(&self) -> LifecycleResult<()> {
        if self.object_id.is_empty() {
            return Err(LifecycleError::InvalidConfig {
                field: "quarantine_object_id",
                reason: "must not be empty",
            });
        }
        if self.object_id == ObjectLayout::quarantine_inventory_object_id() {
            return Err(LifecycleError::InvalidConfig {
                field: "quarantine_object_id",
                reason: "must not be the quarantine inventory object id",
            });
        }
        if self.database_id == [0; 16] {
            return Err(LifecycleError::InvalidConfig {
                field: "database_id",
                reason: "must not be zero",
            });
        }
        if matches!(
            ObjectFamily::from_object_name(&self.source_object),
            None | Some(ObjectFamily::Quarantine)
        ) {
            return Err(LifecycleError::InvalidConfig {
                field: "source_object",
                reason: "must name a non-quarantine storage object",
            });
        }
        if self.staged_at == Timestamp::EPOCH {
            return Err(LifecycleError::InvalidConfig {
                field: "quarantine_timestamp",
                reason: "must not be epoch",
            });
        }
        Ok(())
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) fn object_id(&self) -> &str {
        &self.object_id
    }

    pub(crate) const fn source_object(&self) -> &ObjectName {
        &self.source_object
    }

    pub(crate) const fn proof(&self) -> &LifecycleQuarantineProof {
        &self.proof
    }
}

impl LifecycleQuarantineOutcome {
    fn deferred(request: &LifecycleQuarantineRequest, status: LifecycleQuarantineStatus) -> Self {
        let recovery_health = match status {
            LifecycleQuarantineStatus::BlockedByRecoveryHealth => {
                Some(request.proof.recovery_health.clone())
            }
            LifecycleQuarantineStatus::DeferredIncompleteProof
            | LifecycleQuarantineStatus::DeferredReferenced => {
                Some(telemetry_health_debt("quarantine proof is not safe").expect("health debt"))
            }
            _ => None,
        };
        Self {
            status,
            branch_id: request.branch_id,
            source_object: Some(request.source_object.clone()),
            quarantine_object: None,
            inventory_object: None,
            byte_count: 0,
            entry_count: 0,
            recovery_health,
            source_error: None,
        }
    }

    fn from_report(report: &QuarantineObjectReport) -> Self {
        let status = status_from_report(report.status());
        let recovery_health = health_for_quarantine_status(status);
        let source_error = quarantine_report_error(report, status);
        Self {
            status,
            branch_id: report.branch_id(),
            source_object: Some(report.source_object().clone()),
            quarantine_object: Some(report.quarantine_object().clone()),
            inventory_object: report.inventory_write().map(|write| write.object().clone()),
            byte_count: report.byte_count(),
            entry_count: report.entry_count(),
            recovery_health,
            source_error,
        }
    }

    fn failed_from_service(
        branch_id: BranchId,
        source_object: ObjectName,
        source: QuarantineServiceError,
    ) -> Self {
        let status = match &source {
            QuarantineServiceError::InventoryMismatch { .. }
            | QuarantineServiceError::Decode { .. }
            | QuarantineServiceError::DatabaseMismatch { .. }
            | QuarantineServiceError::BranchMismatch { .. }
            | QuarantineServiceError::CodecMismatch { .. } => {
                LifecycleQuarantineStatus::InventoryMismatch
            }
            QuarantineServiceError::Publish { .. } => {
                LifecycleQuarantineStatus::QuarantinePublishFailed
            }
            QuarantineServiceError::UnsafeGate { .. }
            | QuarantineServiceError::InvalidRequest { .. }
            | QuarantineServiceError::UnsupportedCapability { .. }
            | QuarantineServiceError::Layout { .. }
            | QuarantineServiceError::Missing { .. }
            | QuarantineServiceError::Read { .. }
            | QuarantineServiceError::Encode { .. }
            | QuarantineServiceError::InvalidPublishMetadata { .. }
            | QuarantineServiceError::Metadata { .. }
            | QuarantineServiceError::BackendState { .. } => {
                LifecycleQuarantineStatus::ServiceFailed
            }
        };
        Self {
            status,
            branch_id,
            source_object: Some(source_object),
            quarantine_object: None,
            inventory_object: None,
            byte_count: 0,
            entry_count: 0,
            recovery_health: health_for_quarantine_status(status),
            source_error: Some(quarantine_service_error(source)),
        }
    }

    pub(crate) const fn status(&self) -> LifecycleQuarantineStatus {
        self.status
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn source_object(&self) -> Option<&ObjectName> {
        self.source_object.as_ref()
    }

    pub(crate) const fn quarantine_object(&self) -> Option<&ObjectName> {
        self.quarantine_object.as_ref()
    }

    pub(crate) const fn inventory_object(&self) -> Option<&ObjectName> {
        self.inventory_object.as_ref()
    }

    pub(crate) const fn byte_count(&self) -> u64 {
        self.byte_count
    }

    pub(crate) const fn entry_count(&self) -> usize {
        self.entry_count
    }

    pub(crate) const fn recovery_health(&self) -> Option<&RecoveryHealth> {
        self.recovery_health.as_ref()
    }

    pub(crate) const fn source_error(&self) -> Option<&LifecycleError> {
        self.source_error.as_ref()
    }

    pub(crate) fn maintenance_outcome(&self) -> MaintenanceOutcome {
        let status = match self.status {
            LifecycleQuarantineStatus::QuarantinedSourceDeleted
            | LifecycleQuarantineStatus::AlreadyQuarantined
            | LifecycleQuarantineStatus::SourceDeleteRetried
            | LifecycleQuarantineStatus::SourceAlreadyMissingAfterPublish => {
                MaintenanceOutcomeStatus::Completed
            }
            LifecycleQuarantineStatus::DeferredReferenced
            | LifecycleQuarantineStatus::DeferredIncompleteProof
            | LifecycleQuarantineStatus::BlockedByRecoveryHealth => {
                MaintenanceOutcomeStatus::Deferred
            }
            LifecycleQuarantineStatus::QuarantinedSourceDeleteFailed
            | LifecycleQuarantineStatus::InventoryPublishFailed
            | LifecycleQuarantineStatus::InventoryPublishUncertain
            | LifecycleQuarantineStatus::QuarantinePublishFailed
            | LifecycleQuarantineStatus::QuarantinePublishUncertain
            | LifecycleQuarantineStatus::InventoryMismatch
            | LifecycleQuarantineStatus::ServiceFailed => MaintenanceOutcomeStatus::Failed,
        };
        let mut outcome = MaintenanceOutcome::new(MaintenanceTaskKind::Quarantine, status)
            .with_affected_object_names(self.affected_object_names())
            .with_effects(
                self.affected_object_count(),
                self.reclaimed_bytes(),
                self.retryable(),
            )
            .with_state_changes(self.state_changes())
            .with_stats(LifecycleStats::new(
                0,
                self.recovery_health
                    .as_ref()
                    .map_or(0, RecoveryHealth::fault_count),
                1,
                usize::from(status != MaintenanceOutcomeStatus::Completed),
                0,
            ));
        if let Some(reason) = self.reason() {
            outcome = outcome.with_reason(reason);
        }
        if let Some(health) = self.recovery_health.clone() {
            outcome = outcome.with_recovery_health(health);
        }
        if let Some(error) = self.source_error.clone() {
            outcome = outcome.with_source_error(error);
        }
        outcome
    }

    fn affected_object_names(&self) -> Vec<String> {
        [
            self.source_object.as_ref(),
            self.quarantine_object.as_ref(),
            self.inventory_object.as_ref(),
        ]
        .into_iter()
        .flatten()
        .map(ToString::to_string)
        .collect()
    }

    fn affected_object_count(&self) -> usize {
        usize::from(self.source_object.is_some())
            + usize::from(self.quarantine_object.is_some())
            + usize::from(self.inventory_object.is_some())
    }

    const fn reclaimed_bytes(&self) -> u64 {
        match self.status {
            LifecycleQuarantineStatus::QuarantinedSourceDeleted
            | LifecycleQuarantineStatus::SourceDeleteRetried => self.byte_count,
            _ => 0,
        }
    }

    const fn retryable(&self) -> bool {
        matches!(
            self.status,
            LifecycleQuarantineStatus::QuarantinedSourceDeleteFailed
                | LifecycleQuarantineStatus::InventoryPublishUncertain
                | LifecycleQuarantineStatus::QuarantinePublishFailed
                | LifecycleQuarantineStatus::QuarantinePublishUncertain
        )
    }

    const fn state_changes(&self) -> usize {
        match self.status {
            LifecycleQuarantineStatus::QuarantinedSourceDeleted
            | LifecycleQuarantineStatus::SourceDeleteRetried => 2,
            LifecycleQuarantineStatus::AlreadyQuarantined
            | LifecycleQuarantineStatus::SourceAlreadyMissingAfterPublish => 1,
            _ => 0,
        }
    }

    const fn reason(&self) -> Option<&'static str> {
        match self.status {
            LifecycleQuarantineStatus::DeferredReferenced => Some("object is still referenced"),
            LifecycleQuarantineStatus::DeferredIncompleteProof => {
                Some("quarantine proof is incomplete")
            }
            LifecycleQuarantineStatus::BlockedByRecoveryHealth => {
                Some("recovery health blocks quarantine")
            }
            LifecycleQuarantineStatus::QuarantinedSourceDeleteFailed => {
                Some("source delete failed after quarantine")
            }
            LifecycleQuarantineStatus::InventoryPublishFailed
            | LifecycleQuarantineStatus::InventoryPublishUncertain => {
                Some("quarantine inventory publication failed")
            }
            LifecycleQuarantineStatus::QuarantinePublishFailed
            | LifecycleQuarantineStatus::QuarantinePublishUncertain => {
                Some("quarantine object publication failed")
            }
            LifecycleQuarantineStatus::InventoryMismatch => Some("quarantine inventory mismatch"),
            LifecycleQuarantineStatus::ServiceFailed => Some("quarantine service failed"),
            LifecycleQuarantineStatus::QuarantinedSourceDeleted
            | LifecycleQuarantineStatus::AlreadyQuarantined
            | LifecycleQuarantineStatus::SourceDeleteRetried
            | LifecycleQuarantineStatus::SourceAlreadyMissingAfterPublish => None,
        }
    }
}

impl LifecyclePurgeProof {
    pub(crate) fn fresh(recovery_health: RecoveryHealth) -> Self {
        let status = if recovery_health_blocks_reclaim(&recovery_health) {
            LifecyclePurgeProofStatus::BlockedByRecoveryHealth
        } else {
            LifecyclePurgeProofStatus::CompleteFresh
        };
        Self {
            status,
            recovery_health,
            stale: false,
            missing_fact: None,
        }
    }

    pub(crate) fn stale(recovery_health: RecoveryHealth) -> Self {
        Self {
            status: LifecyclePurgeProofStatus::Stale,
            recovery_health,
            stale: true,
            missing_fact: Some("fresh_proof"),
        }
    }

    pub(crate) fn incomplete(recovery_health: RecoveryHealth, missing_fact: &'static str) -> Self {
        Self {
            status: LifecyclePurgeProofStatus::Incomplete,
            recovery_health,
            stale: false,
            missing_fact: Some(missing_fact),
        }
    }

    pub(crate) const fn status(&self) -> LifecyclePurgeProofStatus {
        self.status
    }

    pub(crate) const fn recovery_health(&self) -> &RecoveryHealth {
        &self.recovery_health
    }

    pub(crate) const fn stale_flag(&self) -> bool {
        self.stale
    }

    pub(crate) const fn missing_fact(&self) -> Option<&'static str> {
        self.missing_fact
    }

    const fn gate(&self) -> QuarantineGate {
        match self.status {
            LifecyclePurgeProofStatus::CompleteFresh => QuarantineGate::Safe,
            LifecyclePurgeProofStatus::Stale | LifecyclePurgeProofStatus::Incomplete => {
                QuarantineGate::ProofIncomplete
            }
            LifecyclePurgeProofStatus::BlockedByRecoveryHealth => QuarantineGate::UnsafeRecovery,
        }
    }
}

impl LifecyclePurgeRequest {
    pub(crate) fn new(
        branch_id: BranchId,
        database_id: [u8; 16],
        codec_id: LifecycleCodecId,
        proof: LifecyclePurgeProof,
    ) -> LifecycleResult<Self> {
        let request = Self {
            branch_id,
            database_id,
            codec_id,
            proof,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> LifecycleResult<()> {
        if self.database_id == [0; 16] {
            return Err(LifecycleError::InvalidConfig {
                field: "database_id",
                reason: "must not be zero",
            });
        }
        Ok(())
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn proof(&self) -> &LifecyclePurgeProof {
        &self.proof
    }
}

impl LifecyclePurgeOutcome {
    fn deferred(request: &LifecyclePurgeRequest, status: LifecyclePurgeStatus) -> Self {
        let recovery_health = match status {
            LifecyclePurgeStatus::BlockedByRecoveryHealth => {
                Some(request.proof.recovery_health.clone())
            }
            LifecyclePurgeStatus::DeferredIncompleteProof | LifecyclePurgeStatus::StaleProof => {
                Some(telemetry_health_debt("purge proof is not safe").expect("health debt"))
            }
            _ => None,
        };
        Self {
            status,
            branch_id: request.branch_id,
            inventory_object: None,
            deleted_objects: Vec::new(),
            already_missing_objects: Vec::new(),
            failed_objects: Vec::new(),
            retained_entries: Vec::new(),
            reclaimed_bytes: 0,
            recovery_health,
            source_error: None,
        }
    }

    fn from_report(report: &QuarantinePurgeReport) -> Self {
        let failed = !report.failed().is_empty();
        let inventory_failed = report.inventory_publish_failure().is_some();
        let changed = !report.deleted().is_empty() || !report.already_missing().is_empty();
        let status = if failed || inventory_failed {
            LifecyclePurgeStatus::CompletedWithHealthDebt
        } else if changed {
            LifecyclePurgeStatus::Completed
        } else {
            LifecyclePurgeStatus::CompletedNoop
        };
        let recovery_health = if failed || inventory_failed {
            Some(telemetry_health_debt("quarantine purge has health debt").expect("health debt"))
        } else {
            None
        };
        let source_error = purge_report_error(report);
        Self {
            status,
            branch_id: report.branch_id(),
            inventory_object: Some(report.inventory_object().clone()),
            deleted_objects: delete_objects(report.deleted()),
            already_missing_objects: delete_objects(report.already_missing()),
            failed_objects: delete_objects(report.failed()),
            retained_entries: report.retained_entries().to_vec(),
            reclaimed_bytes: report.reclaimed_bytes(),
            recovery_health,
            source_error,
        }
    }

    fn failed_from_service(branch_id: BranchId, source: QuarantineServiceError) -> Self {
        Self {
            status: LifecyclePurgeStatus::InventoryRewriteFailed,
            branch_id,
            inventory_object: None,
            deleted_objects: Vec::new(),
            already_missing_objects: Vec::new(),
            failed_objects: Vec::new(),
            retained_entries: Vec::new(),
            reclaimed_bytes: 0,
            recovery_health: Some(
                telemetry_health_debt("quarantine purge failed").expect("health debt"),
            ),
            source_error: Some(quarantine_service_error(source)),
        }
    }

    pub(crate) const fn status(&self) -> LifecyclePurgeStatus {
        self.status
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn inventory_object(&self) -> Option<&ObjectName> {
        self.inventory_object.as_ref()
    }

    pub(crate) fn deleted_objects(&self) -> &[ObjectName] {
        &self.deleted_objects
    }

    pub(crate) fn already_missing_objects(&self) -> &[ObjectName] {
        &self.already_missing_objects
    }

    pub(crate) fn failed_objects(&self) -> &[ObjectName] {
        &self.failed_objects
    }

    pub(crate) fn retained_entries(&self) -> &[QuarantineEntry] {
        &self.retained_entries
    }

    pub(crate) const fn reclaimed_bytes(&self) -> u64 {
        self.reclaimed_bytes
    }

    pub(crate) const fn recovery_health(&self) -> Option<&RecoveryHealth> {
        self.recovery_health.as_ref()
    }

    pub(crate) const fn source_error(&self) -> Option<&LifecycleError> {
        self.source_error.as_ref()
    }

    pub(crate) fn maintenance_outcome(&self) -> MaintenanceOutcome {
        let status = match self.status {
            LifecyclePurgeStatus::Completed
            | LifecyclePurgeStatus::CompletedNoop
            | LifecyclePurgeStatus::CompletedWithHealthDebt => MaintenanceOutcomeStatus::Completed,
            LifecyclePurgeStatus::DeferredIncompleteProof
            | LifecyclePurgeStatus::BlockedByRecoveryHealth
            | LifecyclePurgeStatus::StaleProof => MaintenanceOutcomeStatus::Deferred,
            LifecyclePurgeStatus::InventoryRewriteFailed => MaintenanceOutcomeStatus::Failed,
        };
        let mut outcome = MaintenanceOutcome::new(MaintenanceTaskKind::Purge, status)
            .with_affected_object_names(self.affected_object_names())
            .with_effects(
                self.affected_object_count(),
                self.reclaimed_bytes,
                self.retryable(),
            )
            .with_state_changes(self.deleted_objects.len() + self.already_missing_objects.len())
            .with_stats(LifecycleStats::new(
                0,
                self.recovery_health
                    .as_ref()
                    .map_or(0, RecoveryHealth::fault_count),
                1,
                usize::from(status != MaintenanceOutcomeStatus::Completed),
                0,
            ));
        if let Some(reason) = self.reason() {
            outcome = outcome.with_reason(reason);
        }
        if let Some(health) = self.recovery_health.clone() {
            outcome = outcome.with_recovery_health(health);
        }
        if let Some(error) = self.source_error.clone() {
            outcome = outcome.with_source_error(error);
        }
        outcome
    }

    fn affected_object_names(&self) -> Vec<String> {
        self.inventory_object
            .as_ref()
            .into_iter()
            .chain(self.deleted_objects.iter())
            .chain(self.already_missing_objects.iter())
            .chain(self.failed_objects.iter())
            .map(ToString::to_string)
            .collect()
    }

    fn affected_object_count(&self) -> usize {
        usize::from(self.inventory_object.is_some())
            + self.deleted_objects.len()
            + self.already_missing_objects.len()
            + self.failed_objects.len()
    }

    const fn retryable(&self) -> bool {
        matches!(
            self.status,
            LifecyclePurgeStatus::CompletedWithHealthDebt
                | LifecyclePurgeStatus::InventoryRewriteFailed
        )
    }

    const fn reason(&self) -> Option<&'static str> {
        match self.status {
            LifecyclePurgeStatus::CompletedWithHealthDebt => {
                Some("quarantine purge has health debt")
            }
            LifecyclePurgeStatus::DeferredIncompleteProof => Some("purge proof is incomplete"),
            LifecyclePurgeStatus::BlockedByRecoveryHealth => Some("recovery health blocks purge"),
            LifecyclePurgeStatus::StaleProof => Some("purge proof is stale"),
            LifecyclePurgeStatus::InventoryRewriteFailed => {
                Some("quarantine inventory rewrite failed")
            }
            LifecyclePurgeStatus::Completed | LifecyclePurgeStatus::CompletedNoop => None,
        }
    }
}

impl LifecycleQuarantineRepairRequest {
    pub(crate) fn branch(
        branch_id: BranchId,
        database_id: [u8; 16],
        codec_id: LifecycleCodecId,
    ) -> LifecycleResult<Self> {
        Self::new(
            LifecycleQuarantineRepairScope::Branch(branch_id),
            database_id,
            codec_id,
        )
    }

    pub(crate) fn family(
        database_id: [u8; 16],
        codec_id: LifecycleCodecId,
    ) -> LifecycleResult<Self> {
        Self::new(
            LifecycleQuarantineRepairScope::Family,
            database_id,
            codec_id,
        )
    }

    fn new(
        scope: LifecycleQuarantineRepairScope,
        database_id: [u8; 16],
        codec_id: LifecycleCodecId,
    ) -> LifecycleResult<Self> {
        if database_id == [0; 16] {
            return Err(LifecycleError::InvalidConfig {
                field: "database_id",
                reason: "must not be zero",
            });
        }
        Ok(Self {
            scope,
            database_id,
            codec_id,
            allow_mutation: false,
        })
    }

    pub(crate) const fn with_mutation_allowed(mut self, allow_mutation: bool) -> Self {
        self.allow_mutation = allow_mutation;
        self
    }

    pub(crate) const fn scope(&self) -> LifecycleQuarantineRepairScope {
        self.scope
    }

    pub(crate) const fn allow_mutation(&self) -> bool {
        self.allow_mutation
    }
}

impl LifecycleQuarantineRepairOutcome {
    fn from_branch_report(report: &QuarantineReconciliationReport) -> LifecycleResult<Self> {
        let recovery_health = health_for_reconciliation_class(report.recovery_class())?;
        let status = repair_status_from_class(report.recovery_class());
        let source_error = repair_report_error(report);
        Ok(Self {
            status,
            reports: vec![LifecycleQuarantineRepairReport::from_branch_report(report)],
            recovery_health,
            source_error,
        })
    }

    fn from_family_report(report: &QuarantineFamilyReconciliation) -> LifecycleResult<Self> {
        let recovery_health = health_for_reconciliation_class(report.recovery_class())?;
        let status = repair_status_from_class(report.recovery_class());
        let reports = report
            .branch_reports()
            .iter()
            .map(LifecycleQuarantineRepairReport::from_branch_report)
            .collect();
        Ok(Self {
            status,
            reports,
            recovery_health,
            source_error: report
                .unavailable()
                .map(|unavailable| {
                    LifecycleError::quarantine_repair_inconclusive_with(
                        "quarantine backend is unavailable",
                        unavailable.source().clone(),
                    )
                })
                .or_else(|| report.branch_reports().iter().find_map(repair_report_error)),
        })
    }

    fn failed_from_service(source: QuarantineServiceError) -> Self {
        Self {
            status: LifecycleQuarantineRepairStatus::BackendUnavailable,
            reports: Vec::new(),
            recovery_health: Some(RecoveryHealth::failed(
                RecoveryFault::new(
                    RecoveryFaultKind::QuarantineInventoryMismatch,
                    "quarantine reconciliation failed",
                )
                .expect("fault"),
            )),
            source_error: Some(quarantine_service_error(source)),
        }
    }

    pub(crate) const fn status(&self) -> LifecycleQuarantineRepairStatus {
        self.status
    }

    pub(crate) fn reports(&self) -> &[LifecycleQuarantineRepairReport] {
        &self.reports
    }

    pub(crate) const fn recovery_health(&self) -> Option<&RecoveryHealth> {
        self.recovery_health.as_ref()
    }

    pub(crate) const fn source_error(&self) -> Option<&LifecycleError> {
        self.source_error.as_ref()
    }

    pub(crate) fn maintenance_outcome(&self) -> MaintenanceOutcome {
        let status = match self.status {
            LifecycleQuarantineRepairStatus::CompletedClean
            | LifecycleQuarantineRepairStatus::CompletedWithHealthDebt => {
                MaintenanceOutcomeStatus::Completed
            }
            LifecycleQuarantineRepairStatus::BackendUnavailable => MaintenanceOutcomeStatus::Failed,
        };
        let mut outcome = MaintenanceOutcome::new(MaintenanceTaskKind::Repair, status)
            .with_effects(self.reports.len(), 0, self.retryable())
            .with_state_changes(0)
            .with_stats(LifecycleStats::new(
                0,
                self.recovery_health
                    .as_ref()
                    .map_or(0, RecoveryHealth::fault_count),
                1,
                usize::from(status != MaintenanceOutcomeStatus::Completed),
                0,
            ));
        if let Some(reason) = self.reason() {
            outcome = outcome.with_reason(reason);
        }
        if let Some(health) = self.recovery_health.clone() {
            outcome = outcome.with_recovery_health(health);
        }
        if let Some(error) = self.source_error.clone() {
            outcome = outcome.with_source_error(error);
        }
        outcome
    }

    const fn retryable(&self) -> bool {
        matches!(
            self.status,
            LifecycleQuarantineRepairStatus::BackendUnavailable
        )
    }

    const fn reason(&self) -> Option<&'static str> {
        match self.status {
            LifecycleQuarantineRepairStatus::CompletedClean => None,
            LifecycleQuarantineRepairStatus::CompletedWithHealthDebt => {
                Some("quarantine reconciliation reported health debt")
            }
            LifecycleQuarantineRepairStatus::BackendUnavailable => {
                Some("quarantine reconciliation backend unavailable")
            }
        }
    }
}

impl LifecycleQuarantineRepairReport {
    fn from_branch_report(report: &QuarantineReconciliationReport) -> Self {
        Self {
            branch_id: report.branch_id(),
            kind: report.kind(),
            listed_objects: report.listed_objects().len(),
            missing_objects: report.missing_objects().len(),
            unlisted_objects: report.unlisted_objects().len(),
            malformed_objects: report.malformed_objects().len(),
            inventory_present: report.inventory_present(),
        }
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn kind(&self) -> QuarantineReconciliationKind {
        self.kind
    }

    pub(crate) const fn listed_objects(&self) -> usize {
        self.listed_objects
    }

    pub(crate) const fn missing_objects(&self) -> usize {
        self.missing_objects
    }

    pub(crate) const fn unlisted_objects(&self) -> usize {
        self.unlisted_objects
    }

    pub(crate) const fn malformed_objects(&self) -> usize {
        self.malformed_objects
    }

    pub(crate) const fn inventory_present(&self) -> bool {
        self.inventory_present
    }
}

pub(crate) fn quarantine_object(
    service: &QuarantineService<'_>,
    request: &LifecycleQuarantineRequest,
) -> LifecycleQuarantineOutcome {
    match request.proof.status() {
        LifecycleQuarantineProofStatus::CompleteSafe => {}
        LifecycleQuarantineProofStatus::Referenced => {
            return LifecycleQuarantineOutcome::deferred(
                request,
                LifecycleQuarantineStatus::DeferredReferenced,
            );
        }
        LifecycleQuarantineProofStatus::Incomplete => {
            return LifecycleQuarantineOutcome::deferred(
                request,
                LifecycleQuarantineStatus::DeferredIncompleteProof,
            );
        }
        LifecycleQuarantineProofStatus::BlockedByRecoveryHealth => {
            return LifecycleQuarantineOutcome::deferred(
                request,
                LifecycleQuarantineStatus::BlockedByRecoveryHealth,
            );
        }
    }

    let service_request = QuarantineObjectRequest::new(
        request.branch_id,
        request.database_id,
        request.codec_id.as_str(),
        request.object_id.clone(),
        request.source_object.clone(),
        request.staged_at,
        request.proof.gate(),
    );
    match service.quarantine_object(&service_request) {
        Ok(report) => LifecycleQuarantineOutcome::from_report(&report),
        Err(source) => LifecycleQuarantineOutcome::failed_from_service(
            request.branch_id,
            request.source_object.clone(),
            source,
        ),
    }
}

pub(crate) fn purge_quarantine(
    service: &QuarantineService<'_>,
    request: &LifecyclePurgeRequest,
) -> LifecyclePurgeOutcome {
    match request.proof.status() {
        LifecyclePurgeProofStatus::CompleteFresh => {}
        LifecyclePurgeProofStatus::Stale => {
            return LifecyclePurgeOutcome::deferred(request, LifecyclePurgeStatus::StaleProof);
        }
        LifecyclePurgeProofStatus::Incomplete => {
            return LifecyclePurgeOutcome::deferred(
                request,
                LifecyclePurgeStatus::DeferredIncompleteProof,
            );
        }
        LifecyclePurgeProofStatus::BlockedByRecoveryHealth => {
            return LifecyclePurgeOutcome::deferred(
                request,
                LifecyclePurgeStatus::BlockedByRecoveryHealth,
            );
        }
    }

    let service_request = QuarantinePurgeRequest::new(
        request.branch_id,
        request.database_id,
        request.codec_id.as_str(),
        request.proof.gate(),
    );
    match service.purge_quarantine(service_request) {
        Ok(report) => LifecyclePurgeOutcome::from_report(&report),
        Err(source) => LifecyclePurgeOutcome::failed_from_service(request.branch_id, source),
    }
}

pub(crate) fn repair_quarantine(
    service: &QuarantineService<'_>,
    request: &LifecycleQuarantineRepairRequest,
) -> LifecycleResult<LifecycleQuarantineRepairOutcome> {
    if request.allow_mutation {
        return Err(LifecycleError::QuarantineRepairInconclusive {
            reason: "mutating quarantine repair is not supported",
            source: None,
        });
    }
    match request.scope {
        LifecycleQuarantineRepairScope::Branch(branch_id) => {
            match service.reconcile_branch_quarantine(
                branch_id,
                request.database_id,
                request.codec_id.as_str(),
            ) {
                Ok(report) => LifecycleQuarantineRepairOutcome::from_branch_report(&report),
                Err(source) => Ok(LifecycleQuarantineRepairOutcome::failed_from_service(
                    source,
                )),
            }
        }
        LifecycleQuarantineRepairScope::Family => {
            match service
                .reconcile_quarantine_family(request.database_id, request.codec_id.as_str())
            {
                Ok(report) => LifecycleQuarantineRepairOutcome::from_family_report(&report),
                Err(source) => Ok(LifecycleQuarantineRepairOutcome::failed_from_service(
                    source,
                )),
            }
        }
    }
}

pub(crate) fn purge_request_from_maintenance_task(
    task: &MaintenanceTask,
    database_id: [u8; 16],
    codec_id: LifecycleCodecId,
    recovery_health: RecoveryHealth,
    default_branch_id: BranchId,
) -> LifecycleResult<LifecyclePurgeRequest> {
    if task.kind() != MaintenanceTaskKind::Purge {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "purge request requires purge task",
        });
    }
    let branch_id = match task.scope() {
        super::MaintenanceTaskScope::Branch(branch_id) => branch_id,
        super::MaintenanceTaskScope::Quarantine => default_branch_id,
        _ => {
            return Err(LifecycleError::MaintenanceTaskFailed {
                reason: "purge task scope is invalid",
            });
        }
    };
    LifecyclePurgeRequest::new(
        branch_id,
        database_id,
        codec_id,
        LifecyclePurgeProof::fresh(recovery_health),
    )
}

pub(crate) fn repair_request_from_maintenance_task(
    task: &MaintenanceTask,
    database_id: [u8; 16],
    codec_id: LifecycleCodecId,
) -> LifecycleResult<LifecycleQuarantineRepairRequest> {
    if task.kind() != MaintenanceTaskKind::Repair {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "repair request requires repair task",
        });
    }
    match task.scope() {
        super::MaintenanceTaskScope::Branch(branch_id) => {
            LifecycleQuarantineRepairRequest::branch(branch_id, database_id, codec_id)
        }
        super::MaintenanceTaskScope::Quarantine | super::MaintenanceTaskScope::Global => {
            LifecycleQuarantineRepairRequest::family(database_id, codec_id)
        }
        _ => Err(LifecycleError::MaintenanceTaskFailed {
            reason: "repair task scope is invalid",
        }),
    }
}

pub(crate) fn unsupported_quarantine_maintenance(kind: MaintenanceTaskKind) -> MaintenanceOutcome {
    MaintenanceOutcome::new(kind, MaintenanceOutcomeStatus::Deferred)
        .with_reason("storage mode does not support durable quarantine maintenance")
        .with_stats(LifecycleStats::new(0, 0, 1, 1, 0))
}

pub(crate) fn quarantine_task_without_request() -> MaintenanceOutcome {
    MaintenanceOutcome::new(
        MaintenanceTaskKind::Quarantine,
        MaintenanceOutcomeStatus::Deferred,
    )
    .with_reason("quarantine task requires an explicit quarantine request")
    .with_stats(LifecycleStats::new(0, 0, 1, 1, 0))
}

fn status_from_report(status: QuarantineObjectStatus) -> LifecycleQuarantineStatus {
    match status {
        QuarantineObjectStatus::QuarantinedSourceDeleted => {
            LifecycleQuarantineStatus::QuarantinedSourceDeleted
        }
        QuarantineObjectStatus::AlreadyQuarantined => LifecycleQuarantineStatus::AlreadyQuarantined,
        QuarantineObjectStatus::SourceDeleteRetried => {
            LifecycleQuarantineStatus::SourceDeleteRetried
        }
        QuarantineObjectStatus::SourceAlreadyMissingAfterPublish => {
            LifecycleQuarantineStatus::SourceAlreadyMissingAfterPublish
        }
        QuarantineObjectStatus::QuarantinedSourceDeleteFailed => {
            LifecycleQuarantineStatus::QuarantinedSourceDeleteFailed
        }
        QuarantineObjectStatus::InventoryPublishFailed => {
            LifecycleQuarantineStatus::InventoryPublishFailed
        }
        QuarantineObjectStatus::InventoryPublishUncertain => {
            LifecycleQuarantineStatus::InventoryPublishUncertain
        }
        QuarantineObjectStatus::QuarantinePublishFailed => {
            LifecycleQuarantineStatus::QuarantinePublishFailed
        }
        QuarantineObjectStatus::QuarantinePublishUncertain => {
            LifecycleQuarantineStatus::QuarantinePublishUncertain
        }
    }
}

fn quarantine_report_error(
    report: &QuarantineObjectReport,
    status: LifecycleQuarantineStatus,
) -> Option<LifecycleError> {
    match status {
        LifecycleQuarantineStatus::InventoryPublishFailed => {
            report.inventory_publish_failure().map(|failure| {
                LifecycleError::quarantine_publication_failed_with(
                    "quarantine inventory publication failed",
                    failure.source().clone(),
                )
            })
        }
        LifecycleQuarantineStatus::InventoryPublishUncertain => {
            report.inventory_publish_failure().map(|failure| {
                LifecycleError::quarantine_publication_uncertain_with(
                    "quarantine inventory publication uncertain",
                    failure.source().clone(),
                )
            })
        }
        LifecycleQuarantineStatus::QuarantinePublishFailed => {
            report.quarantine_publish_failure().map(|failure| {
                LifecycleError::quarantine_publication_failed_with(
                    "quarantine object publication failed",
                    failure.source().clone(),
                )
            })
        }
        LifecycleQuarantineStatus::QuarantinePublishUncertain => {
            report.quarantine_publish_failure().map(|failure| {
                LifecycleError::quarantine_publication_uncertain_with(
                    "quarantine object publication uncertain",
                    failure.source().clone(),
                )
            })
        }
        LifecycleQuarantineStatus::QuarantinedSourceDeleteFailed => report
            .source_delete()
            .and_then(QuarantineDeleteOutcome::failure)
            .map(|failure| {
                LifecycleError::lower_layer_with(
                    LifecycleLowerLayer::Backend,
                    "quarantine source delete failed",
                    failure.clone(),
                )
            }),
        _ => None,
    }
}

fn purge_report_error(report: &QuarantinePurgeReport) -> Option<LifecycleError> {
    if let Some(failure) = report.inventory_publish_failure() {
        return Some(LifecycleError::quarantine_publication_failed_with(
            "quarantine purge inventory rewrite failed",
            failure.source().clone(),
        ));
    }
    report.failed().iter().find_map(|failure| {
        failure.failure().map(|source| {
            LifecycleError::lower_layer_with(
                LifecycleLowerLayer::Backend,
                "quarantine purge delete failed",
                source.clone(),
            )
        })
    })
}

fn repair_report_error(report: &QuarantineReconciliationReport) -> Option<LifecycleError> {
    report.unavailable().map(|unavailable| {
        LifecycleError::quarantine_repair_inconclusive_with(
            "quarantine backend is unavailable",
            unavailable.source().clone(),
        )
    })
}

fn health_for_quarantine_status(status: LifecycleQuarantineStatus) -> Option<RecoveryHealth> {
    match status {
        LifecycleQuarantineStatus::QuarantinedSourceDeleteFailed
        | LifecycleQuarantineStatus::InventoryPublishFailed
        | LifecycleQuarantineStatus::InventoryPublishUncertain
        | LifecycleQuarantineStatus::QuarantinePublishFailed
        | LifecycleQuarantineStatus::QuarantinePublishUncertain
        | LifecycleQuarantineStatus::InventoryMismatch
        | LifecycleQuarantineStatus::ServiceFailed => Some(
            telemetry_health_debt("quarantine operation has health debt").expect("health debt"),
        ),
        _ => None,
    }
}

fn health_for_reconciliation_class(
    class: QuarantineRecoveryClass,
) -> LifecycleResult<Option<RecoveryHealth>> {
    match class {
        QuarantineRecoveryClass::Healthy => Ok(None),
        QuarantineRecoveryClass::PolicyDowngraded => Ok(Some(RecoveryHealth::degraded(
            RecoveryDegradationClass::PolicyDowngrade,
            vec![RecoveryFault::new(
                RecoveryFaultKind::QuarantineInventoryMismatch,
                "quarantine inventory mismatch",
            )?],
        )?)),
        QuarantineRecoveryClass::Unavailable => {
            Ok(Some(RecoveryHealth::failed(RecoveryFault::new(
                RecoveryFaultKind::QuarantineInventoryMismatch,
                "quarantine backend unavailable",
            )?)))
        }
    }
}

const fn repair_status_from_class(
    class: QuarantineRecoveryClass,
) -> LifecycleQuarantineRepairStatus {
    match class {
        QuarantineRecoveryClass::Healthy => LifecycleQuarantineRepairStatus::CompletedClean,
        QuarantineRecoveryClass::PolicyDowngraded => {
            LifecycleQuarantineRepairStatus::CompletedWithHealthDebt
        }
        QuarantineRecoveryClass::Unavailable => LifecycleQuarantineRepairStatus::BackendUnavailable,
    }
}

fn delete_objects(outcomes: &[QuarantineDeleteOutcome]) -> Vec<ObjectName> {
    outcomes
        .iter()
        .map(|outcome| outcome.object().clone())
        .collect()
}

fn recovery_health_blocks_reclaim(health: &RecoveryHealth) -> bool {
    !matches!(
        health,
        RecoveryHealth::Healthy
            | RecoveryHealth::Degraded {
                class: RecoveryDegradationClass::Telemetry,
                ..
            }
    )
}

fn quarantine_service_error(source: QuarantineServiceError) -> LifecycleError {
    match source {
        QuarantineServiceError::UnsafeGate { .. } => LifecycleError::QuarantineProofBlocked {
            reason: "quarantine proof is not safe",
        },
        QuarantineServiceError::InventoryMismatch { .. }
        | QuarantineServiceError::Decode { .. }
        | QuarantineServiceError::DatabaseMismatch { .. }
        | QuarantineServiceError::BranchMismatch { .. }
        | QuarantineServiceError::CodecMismatch { .. } => {
            LifecycleError::quarantine_inventory_mismatch_with(
                "quarantine inventory mismatch",
                source,
            )
        }
        QuarantineServiceError::Publish { .. } => {
            LifecycleError::quarantine_publication_failed_with(
                "quarantine publication failed",
                source,
            )
        }
        QuarantineServiceError::InvalidRequest { field } => LifecycleError::InvalidConfig {
            field,
            reason: "quarantine request is invalid",
        },
        QuarantineServiceError::UnsupportedCapability { .. }
        | QuarantineServiceError::Layout { .. }
        | QuarantineServiceError::Missing { .. }
        | QuarantineServiceError::Read { .. }
        | QuarantineServiceError::Encode { .. }
        | QuarantineServiceError::InvalidPublishMetadata { .. }
        | QuarantineServiceError::Metadata { .. }
        | QuarantineServiceError::BackendState { .. } => LifecycleError::lower_layer_with(
            LifecycleLowerLayer::Service,
            "quarantine service failed",
            source,
        ),
    }
}

fn derived_quarantine_object_id(source_object: &ObjectName) -> String {
    let digest = Sha256::digest(source_object.as_str().as_bytes());
    let mut suffix = String::with_capacity(32);
    for byte in &digest[..16] {
        use std::fmt::Write as _;
        write!(suffix, "{byte:02x}").expect("write to String");
    }
    format!("object-{suffix}")
}
