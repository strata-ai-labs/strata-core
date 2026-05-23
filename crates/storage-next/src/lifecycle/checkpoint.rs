//! Checkpoint, flush-watermark, and WAL-retention orchestration.

use super::{
    telemetry_health_debt, LifecycleDurableLocalServices, LifecycleError, LifecycleLowerLayer,
    LifecycleResult, LifecycleStats, MaintenanceOutcome, MaintenanceOutcomeStatus, MaintenanceTask,
    MaintenanceTaskKind, MaintenanceTaskScope,
};
use crate::branch::BranchLocalState;
use crate::commit::CommitBranchGuardSet;
use crate::format::SnapshotSection;
use crate::lifecycle::recovery::encode_checkpoint_row_section;
use crate::object::ObjectName;
use crate::service::{
    CheckpointRequest, CheckpointServiceError, CheckpointSnapshot, CheckpointWrite,
    DatabaseManifestService, ManifestServiceError, WalDeleteReport, WalRetentionProof, WalService,
    WalServiceError,
};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleCheckpointRequest {
    branch_id: BranchId,
    snapshot_id: u64,
    created_at: Timestamp,
    extra_sections: Vec<SnapshotSection>,
    persist_flush_watermark_after_checkpoint: bool,
    truncate_wal_after_checkpoint: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleCheckpointOutcome {
    status: LifecycleCheckpointStatus,
    branch_id: BranchId,
    checkpoint_watermark: Option<CommitVersion>,
    snapshot_id: Option<u64>,
    row_count: u64,
    section_count: usize,
    snapshot_object: Option<ObjectName>,
    active_wal_segment: Option<u64>,
    flush_watermark: Option<LifecycleFlushWatermarkOutcome>,
    wal_truncation: Option<LifecycleWalTruncationOutcome>,
    recovery_health: Option<super::RecoveryHealth>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum LifecycleCheckpointStatus {
    Completed,
    DeferredNoVisibleRows,
    SnapshotPublishedManifestNotUpdated,
    SnapshotVisibilityUncertain,
    FlushWatermarkFailed,
    WalTruncationFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleFlushWatermarkRequest {
    candidate: CommitVersion,
    proof: LifecycleFlushWatermarkProof,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
#[allow(
    dead_code,
    reason = "flush-watermark proof vocabulary is exercised by dedicated maintenance tests"
)]
pub(crate) enum LifecycleFlushWatermarkProof {
    CheckpointCovered { snapshot_watermark: CommitVersion },
    AlreadyPersisted,
    TableObjectsOnly { flushed_through: CommitVersion },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleFlushWatermarkOutcome {
    status: LifecycleFlushWatermarkStatus,
    candidate: CommitVersion,
    persisted: Option<CommitVersion>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum LifecycleFlushWatermarkStatus {
    Persisted,
    AlreadyPersisted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleWalTruncationRequest {
    proof: WalRetentionProof,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleWalTruncationOutcome {
    status: LifecycleWalTruncationStatus,
    covered_through: CommitVersion,
    deleted_segments: usize,
    protected_segments: usize,
    failed_segments: usize,
    recovery_health: Option<super::RecoveryHealth>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum LifecycleWalTruncationStatus {
    Completed,
    CompletedWithHealthDebt,
}

impl LifecycleCheckpointRequest {
    pub(crate) fn new(
        branch_id: BranchId,
        snapshot_id: u64,
        created_at: Timestamp,
    ) -> LifecycleResult<Self> {
        let request = Self {
            branch_id,
            snapshot_id,
            created_at,
            extra_sections: Vec::new(),
            persist_flush_watermark_after_checkpoint: false,
            truncate_wal_after_checkpoint: false,
        };
        request.validate()?;
        Ok(request)
    }

    #[allow(
        dead_code,
        reason = "extra snapshot sections are passed by maintenance integrations"
    )]
    pub(crate) fn with_extra_sections(mut self, extra_sections: Vec<SnapshotSection>) -> Self {
        self.extra_sections = extra_sections;
        self
    }

    #[allow(
        dead_code,
        reason = "checkpoint callers opt into flush-watermark persistence explicitly"
    )]
    pub(crate) const fn with_flush_watermark_after_checkpoint(mut self, enabled: bool) -> Self {
        self.persist_flush_watermark_after_checkpoint = enabled;
        self
    }

    #[allow(
        dead_code,
        reason = "checkpoint callers opt into WAL truncation explicitly"
    )]
    pub(crate) const fn with_wal_truncation_after_checkpoint(mut self, enabled: bool) -> Self {
        self.truncate_wal_after_checkpoint = enabled;
        self
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn snapshot_id(&self) -> u64 {
        self.snapshot_id
    }

    pub(crate) const fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub(crate) fn extra_sections(&self) -> &[SnapshotSection] {
        &self.extra_sections
    }

    pub(crate) const fn persist_flush_watermark_after_checkpoint(&self) -> bool {
        self.persist_flush_watermark_after_checkpoint
    }

    pub(crate) const fn truncate_wal_after_checkpoint(&self) -> bool {
        self.truncate_wal_after_checkpoint
    }

    fn validate(&self) -> LifecycleResult<()> {
        if self.snapshot_id == 0 {
            return Err(LifecycleError::MaintenanceFailed {
                reason: "checkpoint snapshot id must be nonzero",
            });
        }
        Ok(())
    }
}

#[allow(
    dead_code,
    reason = "checkpoint outcome accessors are consumed by maintenance and closeout tests"
)]
impl LifecycleCheckpointOutcome {
    fn deferred(request: &LifecycleCheckpointRequest) -> Self {
        Self {
            status: LifecycleCheckpointStatus::DeferredNoVisibleRows,
            branch_id: request.branch_id(),
            checkpoint_watermark: None,
            snapshot_id: None,
            row_count: 0,
            section_count: 0,
            snapshot_object: None,
            active_wal_segment: None,
            flush_watermark: None,
            wal_truncation: None,
            recovery_health: None,
        }
    }

    fn completed(
        request: &LifecycleCheckpointRequest,
        watermark: CommitVersion,
        row_count: u64,
        write: &CheckpointWrite,
    ) -> Self {
        Self {
            status: LifecycleCheckpointStatus::Completed,
            branch_id: request.branch_id(),
            checkpoint_watermark: Some(watermark),
            snapshot_id: Some(write.snapshot().snapshot_id()),
            row_count,
            section_count: write.snapshot().section_count(),
            snapshot_object: Some(write.snapshot().object().clone()),
            active_wal_segment: Some(write.active_wal_segment()),
            flush_watermark: None,
            wal_truncation: None,
            recovery_health: None,
        }
    }

    fn partial(
        request: &LifecycleCheckpointRequest,
        status: LifecycleCheckpointStatus,
        watermark: CommitVersion,
        row_count: u64,
        snapshot: &CheckpointSnapshot,
        reason: &'static str,
    ) -> LifecycleResult<Self> {
        Ok(Self {
            status,
            branch_id: request.branch_id(),
            checkpoint_watermark: Some(watermark),
            snapshot_id: Some(snapshot.snapshot_id()),
            row_count,
            section_count: snapshot.section_count(),
            snapshot_object: Some(snapshot.object().clone()),
            active_wal_segment: None,
            flush_watermark: None,
            wal_truncation: None,
            recovery_health: Some(telemetry_health_debt(reason)?),
        })
    }

    fn with_flush_watermark(mut self, outcome: LifecycleFlushWatermarkOutcome) -> Self {
        self.flush_watermark = Some(outcome);
        self
    }

    fn with_wal_truncation(mut self, outcome: LifecycleWalTruncationOutcome) -> Self {
        if matches!(
            outcome.status(),
            LifecycleWalTruncationStatus::CompletedWithHealthDebt
        ) {
            self.status = LifecycleCheckpointStatus::WalTruncationFailed;
            self.recovery_health = outcome.recovery_health().cloned();
        }
        self.wal_truncation = Some(outcome);
        self
    }

    fn with_follow_up_failure(
        mut self,
        status: LifecycleCheckpointStatus,
        reason: &'static str,
    ) -> LifecycleResult<Self> {
        self.status = status;
        self.recovery_health = Some(telemetry_health_debt(reason)?);
        Ok(self)
    }

    pub(crate) const fn status(&self) -> LifecycleCheckpointStatus {
        self.status
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn checkpoint_watermark(&self) -> Option<CommitVersion> {
        self.checkpoint_watermark
    }

    pub(crate) const fn snapshot_id(&self) -> Option<u64> {
        self.snapshot_id
    }

    pub(crate) const fn row_count(&self) -> u64 {
        self.row_count
    }

    pub(crate) const fn section_count(&self) -> usize {
        self.section_count
    }

    pub(crate) fn snapshot_object(&self) -> Option<&ObjectName> {
        self.snapshot_object.as_ref()
    }

    pub(crate) const fn active_wal_segment(&self) -> Option<u64> {
        self.active_wal_segment
    }

    pub(crate) const fn flush_watermark(&self) -> Option<&LifecycleFlushWatermarkOutcome> {
        self.flush_watermark.as_ref()
    }

    pub(crate) const fn wal_truncation(&self) -> Option<&LifecycleWalTruncationOutcome> {
        self.wal_truncation.as_ref()
    }

    pub(crate) const fn recovery_health(&self) -> Option<&super::RecoveryHealth> {
        self.recovery_health.as_ref()
    }

    pub(crate) fn maintenance_outcome(&self) -> MaintenanceOutcome {
        let status = match self.status {
            LifecycleCheckpointStatus::Completed => MaintenanceOutcomeStatus::Completed,
            LifecycleCheckpointStatus::DeferredNoVisibleRows => MaintenanceOutcomeStatus::Deferred,
            LifecycleCheckpointStatus::SnapshotPublishedManifestNotUpdated
            | LifecycleCheckpointStatus::SnapshotVisibilityUncertain
            | LifecycleCheckpointStatus::FlushWatermarkFailed
            | LifecycleCheckpointStatus::WalTruncationFailed => MaintenanceOutcomeStatus::Failed,
        };
        let mut outcome = MaintenanceOutcome::new(MaintenanceTaskKind::Checkpoint, status)
            .with_effects(
                usize::from(self.snapshot_object.is_some()),
                0,
                self.retryable(),
            )
            .with_stats(LifecycleStats::new(0, 0, 1, 0, 0));
        if let Some(health) = self.recovery_health.clone() {
            outcome = outcome.with_recovery_health(health);
        }
        outcome
    }

    const fn retryable(&self) -> bool {
        matches!(
            self.status,
            LifecycleCheckpointStatus::SnapshotPublishedManifestNotUpdated
                | LifecycleCheckpointStatus::SnapshotVisibilityUncertain
                | LifecycleCheckpointStatus::FlushWatermarkFailed
                | LifecycleCheckpointStatus::WalTruncationFailed
        )
    }
}

impl LifecycleFlushWatermarkRequest {
    pub(crate) fn new(
        candidate: CommitVersion,
        proof: LifecycleFlushWatermarkProof,
    ) -> LifecycleResult<Self> {
        let request = Self { candidate, proof };
        request.validate_static()?;
        Ok(request)
    }

    pub(crate) const fn candidate(self) -> CommitVersion {
        self.candidate
    }

    pub(crate) const fn proof(self) -> LifecycleFlushWatermarkProof {
        self.proof
    }

    const fn validate_static(self) -> LifecycleResult<()> {
        if self.candidate.as_u64() == 0 {
            return Err(LifecycleError::MaintenanceFailed {
                reason: "flush watermark candidate must be nonzero",
            });
        }
        Ok(())
    }
}

#[allow(
    dead_code,
    reason = "flush-watermark outcome accessors are consumed by maintenance tests"
)]
impl LifecycleFlushWatermarkOutcome {
    fn persisted(candidate: CommitVersion) -> Self {
        Self {
            status: LifecycleFlushWatermarkStatus::Persisted,
            candidate,
            persisted: Some(candidate),
        }
    }

    fn already_persisted(candidate: CommitVersion) -> Self {
        Self {
            status: LifecycleFlushWatermarkStatus::AlreadyPersisted,
            candidate,
            persisted: Some(candidate),
        }
    }

    pub(crate) const fn status(&self) -> LifecycleFlushWatermarkStatus {
        self.status
    }

    pub(crate) const fn candidate(&self) -> CommitVersion {
        self.candidate
    }

    pub(crate) const fn persisted_watermark(&self) -> Option<CommitVersion> {
        self.persisted
    }
}

impl LifecycleWalTruncationRequest {
    pub(crate) fn new(proof: WalRetentionProof) -> LifecycleResult<Self> {
        if proof.covered_through() == CommitVersion::ZERO {
            return Err(LifecycleError::MaintenanceFailed {
                reason: "WAL retention proof must be nonzero",
            });
        }
        Ok(Self { proof })
    }

    pub(crate) const fn proof(self) -> WalRetentionProof {
        self.proof
    }
}

#[allow(
    dead_code,
    reason = "WAL truncation outcome accessors are consumed by maintenance tests"
)]
impl LifecycleWalTruncationOutcome {
    fn completed(proof: WalRetentionProof, report: &WalDeleteReport) -> LifecycleResult<Self> {
        let failed_segments = report.failed_segments().len();
        let recovery_health = if failed_segments == 0 {
            None
        } else {
            Some(telemetry_health_debt(
                "WAL truncation failed for one or more segments",
            )?)
        };
        Ok(Self {
            status: if failed_segments == 0 {
                LifecycleWalTruncationStatus::Completed
            } else {
                LifecycleWalTruncationStatus::CompletedWithHealthDebt
            },
            covered_through: proof.covered_through(),
            deleted_segments: report.deleted_segments().len(),
            protected_segments: report.protected_segments().len(),
            failed_segments,
            recovery_health,
        })
    }

    pub(crate) const fn status(&self) -> LifecycleWalTruncationStatus {
        self.status
    }

    pub(crate) const fn covered_through(&self) -> CommitVersion {
        self.covered_through
    }

    pub(crate) const fn deleted_segments(&self) -> usize {
        self.deleted_segments
    }

    pub(crate) const fn protected_segments(&self) -> usize {
        self.protected_segments
    }

    pub(crate) const fn failed_segments(&self) -> usize {
        self.failed_segments
    }

    pub(crate) const fn recovery_health(&self) -> Option<&super::RecoveryHealth> {
        self.recovery_health.as_ref()
    }

    pub(crate) fn maintenance_outcome(&self) -> MaintenanceOutcome {
        let status = match self.status {
            LifecycleWalTruncationStatus::Completed => MaintenanceOutcomeStatus::Completed,
            LifecycleWalTruncationStatus::CompletedWithHealthDebt => {
                MaintenanceOutcomeStatus::Failed
            }
        };
        let mut outcome = MaintenanceOutcome::new(MaintenanceTaskKind::WalTruncation, status)
            .with_effects(self.deleted_segments, 0, self.failed_segments > 0)
            .with_stats(LifecycleStats::new(0, 0, 1, 0, 0));
        if let Some(health) = self.recovery_health.clone() {
            outcome = outcome.with_recovery_health(health);
        }
        outcome
    }
}

pub(crate) fn checkpoint_durable_branch(
    branch: &BranchLocalState,
    services: &LifecycleDurableLocalServices<'_>,
    guard_set: &CommitBranchGuardSet,
    read_visible_version: impl FnOnce() -> CommitVersion,
    request: &LifecycleCheckpointRequest,
) -> LifecycleResult<LifecycleCheckpointOutcome> {
    request.validate()?;
    if branch.branch_id() != request.branch_id() {
        return Err(LifecycleError::MaintenanceFailed {
            reason: "checkpoint branch id must match branch state",
        });
    }
    let quiesce = guard_set.try_begin_quiesce().map_err(commit_error)?;
    let visible_version = read_visible_version();
    if visible_version == CommitVersion::ZERO {
        drop(quiesce);
        return Ok(LifecycleCheckpointOutcome::deferred(request));
    }
    let rows = branch
        .checkpoint_rows(visible_version)
        .map_err(branch_error)?;
    if rows.is_empty() {
        drop(quiesce);
        return Ok(LifecycleCheckpointOutcome::deferred(request));
    }
    let row_count = u64::try_from(rows.len()).map_err(|_| LifecycleError::MaintenanceFailed {
        reason: "checkpoint row count must fit in u64",
    })?;
    validate_snapshot_id_advances(services.manifest(), request.snapshot_id())?;
    let mut sections = Vec::with_capacity(1 + request.extra_sections().len());
    sections.push(encode_checkpoint_row_section(&rows).map_err(format_error)?);
    sections.extend(request.extra_sections().iter().cloned());
    let active_wal_segment = services.wal().active_segment_id();
    drop(quiesce);

    let service_request = CheckpointRequest::new(
        *services.assembly_facts().database_id(),
        services.assembly_facts().codec_id().to_owned(),
        active_wal_segment,
        request.snapshot_id(),
        visible_version,
        request.created_at(),
        sections,
    );
    let mut outcome = match services.checkpoint().checkpoint(service_request) {
        Ok(write) => {
            LifecycleCheckpointOutcome::completed(request, visible_version, row_count, &write)
        }
        Err(CheckpointServiceError::OrphanSnapshot { snapshot, .. }) => {
            return LifecycleCheckpointOutcome::partial(
                request,
                LifecycleCheckpointStatus::SnapshotPublishedManifestNotUpdated,
                visible_version,
                row_count,
                &snapshot,
                "checkpoint snapshot published before manifest update failed",
            );
        }
        Err(CheckpointServiceError::FinalManifestUncertain { snapshot, .. }) => {
            return LifecycleCheckpointOutcome::partial(
                request,
                LifecycleCheckpointStatus::SnapshotVisibilityUncertain,
                visible_version,
                row_count,
                &snapshot,
                "checkpoint final manifest visibility is uncertain",
            );
        }
        Err(error) => return Err(checkpoint_error(error)),
    };

    outcome = run_checkpoint_follow_ups(services, visible_version, request, outcome)?;

    Ok(outcome)
}

fn run_checkpoint_follow_ups(
    services: &LifecycleDurableLocalServices<'_>,
    visible_version: CommitVersion,
    request: &LifecycleCheckpointRequest,
    mut outcome: LifecycleCheckpointOutcome,
) -> LifecycleResult<LifecycleCheckpointOutcome> {
    if request.persist_flush_watermark_after_checkpoint() {
        let Ok(flush) = persist_flush_watermark(
            services.manifest(),
            visible_version,
            LifecycleFlushWatermarkRequest::new(
                visible_version,
                LifecycleFlushWatermarkProof::CheckpointCovered {
                    snapshot_watermark: visible_version,
                },
            )?,
        ) else {
            return outcome.with_follow_up_failure(
                LifecycleCheckpointStatus::FlushWatermarkFailed,
                "checkpoint flush watermark persistence failed",
            );
        };
        outcome = outcome.with_flush_watermark(flush);
    }
    if request.truncate_wal_after_checkpoint() {
        let Ok(truncation) = truncate_wal(
            services.wal(),
            LifecycleWalTruncationRequest::new(WalRetentionProof::snapshot_watermark(
                visible_version,
            ))?,
        ) else {
            return outcome.with_follow_up_failure(
                LifecycleCheckpointStatus::WalTruncationFailed,
                "checkpoint WAL truncation failed",
            );
        };
        outcome = outcome.with_wal_truncation(truncation);
    }
    Ok(outcome)
}

pub(crate) fn persist_flush_watermark(
    manifest: &DatabaseManifestService<'_>,
    visible_version: CommitVersion,
    request: LifecycleFlushWatermarkRequest,
) -> LifecycleResult<LifecycleFlushWatermarkOutcome> {
    request.validate_static()?;
    if request.candidate() > visible_version {
        return Err(LifecycleError::MaintenanceFailed {
            reason: "flush watermark candidate exceeds visible version",
        });
    }
    let current = manifest.load_required().map_err(manifest_error)?;
    if current
        .flushed_through_commit_id()
        .is_some_and(|persisted| request.candidate() <= persisted)
    {
        return Ok(LifecycleFlushWatermarkOutcome::already_persisted(
            request.candidate(),
        ));
    }
    match request.proof() {
        LifecycleFlushWatermarkProof::CheckpointCovered { snapshot_watermark } => {
            if request.candidate() > snapshot_watermark {
                return Err(LifecycleError::MaintenanceFailed {
                    reason: "flush watermark candidate exceeds checkpoint proof",
                });
            }
            if current
                .snapshot_watermark()
                .is_none_or(|snapshot| request.candidate().as_u64() > snapshot)
            {
                return Err(LifecycleError::MaintenanceFailed {
                    reason: "flush watermark candidate exceeds durable checkpoint facts",
                });
            }
        }
        LifecycleFlushWatermarkProof::AlreadyPersisted => {
            return Err(LifecycleError::MaintenanceFailed {
                reason: "flush watermark candidate is not already persisted",
            });
        }
        LifecycleFlushWatermarkProof::TableObjectsOnly { .. } => {
            return Err(LifecycleError::RetentionBlocked {
                reason: "table object flush facts are not a recovery proof for flush watermark",
            });
        }
    }
    manifest
        .persist_flush_watermark(request.candidate())
        .map_err(manifest_error)?;
    Ok(LifecycleFlushWatermarkOutcome::persisted(
        request.candidate(),
    ))
}

pub(crate) fn truncate_wal(
    wal: &WalService<'_>,
    request: LifecycleWalTruncationRequest,
) -> LifecycleResult<LifecycleWalTruncationOutcome> {
    let report = wal
        .delete_covered_segments(request.proof())
        .map_err(wal_error)?;
    LifecycleWalTruncationOutcome::completed(request.proof(), &report)
}

pub(crate) fn checkpoint_request_from_maintenance_task(
    task: &MaintenanceTask,
    branch_id: BranchId,
    manifest: &DatabaseManifestService<'_>,
    created_at: Timestamp,
) -> LifecycleResult<LifecycleCheckpointRequest> {
    if task.kind() != MaintenanceTaskKind::Checkpoint {
        return Err(LifecycleError::MaintenanceFailed {
            reason: "maintenance task kind is not checkpoint",
        });
    }
    if !matches!(
        task.scope(),
        MaintenanceTaskScope::Checkpoint | MaintenanceTaskScope::Global
    ) {
        return Err(LifecycleError::MaintenanceFailed {
            reason: "checkpoint task must target checkpoint scope",
        });
    }
    let current = manifest.load_required().map_err(manifest_error)?;
    let snapshot_id = current.snapshot_id().unwrap_or(0).checked_add(1).ok_or(
        LifecycleError::MaintenanceFailed {
            reason: "checkpoint snapshot id overflow",
        },
    )?;
    LifecycleCheckpointRequest::new(branch_id, snapshot_id, created_at)
}

pub(crate) fn wal_truncation_request_from_maintenance_task(
    task: &MaintenanceTask,
    manifest: &DatabaseManifestService<'_>,
) -> LifecycleResult<Option<LifecycleWalTruncationRequest>> {
    if task.kind() != MaintenanceTaskKind::WalTruncation {
        return Err(LifecycleError::MaintenanceFailed {
            reason: "maintenance task kind is not WAL truncation",
        });
    }
    if task.scope() != MaintenanceTaskScope::Wal {
        return Err(LifecycleError::MaintenanceFailed {
            reason: "WAL truncation task must target WAL scope",
        });
    }
    let current = manifest.load_required().map_err(manifest_error)?;
    let snapshot = current.snapshot_watermark().map(CommitVersion::new);
    let flush = current.flushed_through_commit_id();
    let proof = match (snapshot, flush) {
        (Some(snapshot), Some(flush)) if flush >= snapshot => {
            WalRetentionProof::flush_watermark(flush)
        }
        (Some(snapshot), _) => WalRetentionProof::snapshot_watermark(snapshot),
        (None, Some(flush)) => WalRetentionProof::flush_watermark(flush),
        (None, None) => return Ok(None),
    };
    Ok(Some(LifecycleWalTruncationRequest::new(proof)?))
}

fn validate_snapshot_id_advances(
    manifest: &DatabaseManifestService<'_>,
    snapshot_id: u64,
) -> LifecycleResult<()> {
    let current = manifest.load_required().map_err(manifest_error)?;
    if current
        .snapshot_id()
        .is_some_and(|current| snapshot_id <= current)
    {
        return Err(LifecycleError::MaintenanceFailed {
            reason: "checkpoint snapshot id must advance",
        });
    }
    Ok(())
}

fn checkpoint_error(error: CheckpointServiceError) -> LifecycleError {
    LifecycleError::lower_layer_with(
        LifecycleLowerLayer::Service,
        "checkpoint service failed",
        error,
    )
}

fn manifest_error(error: ManifestServiceError) -> LifecycleError {
    LifecycleError::lower_layer_with(
        LifecycleLowerLayer::Service,
        "database manifest service failed",
        error,
    )
}

fn wal_error(error: WalServiceError) -> LifecycleError {
    LifecycleError::lower_layer_with(LifecycleLowerLayer::Service, "WAL service failed", error)
}

fn branch_error(error: impl std::error::Error + Send + Sync + 'static) -> LifecycleError {
    LifecycleError::lower_layer_with(
        LifecycleLowerLayer::BranchRuntime,
        "branch runtime failed",
        error,
    )
}

fn commit_error(error: impl std::error::Error + Send + Sync + 'static) -> LifecycleError {
    LifecycleError::lower_layer_with(
        LifecycleLowerLayer::CommitRuntime,
        "commit runtime failed",
        error,
    )
}

fn format_error(error: impl std::error::Error + Send + Sync + 'static) -> LifecycleError {
    LifecycleError::lower_layer_with(LifecycleLowerLayer::Format, "format failed", error)
}
