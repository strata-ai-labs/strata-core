//! Checkpoint, flush-watermark, and WAL-retention orchestration.

use super::{
    require_generated_artifact_budget, telemetry_health_debt, LifecycleDurableLocalServices,
    LifecycleError, LifecycleLowerLayer, LifecycleResult, LifecycleStats, MaintenanceOutcome,
    MaintenanceOutcomeStatus, MaintenanceTask, MaintenanceTaskKind, MaintenanceTaskScope,
    RecoveryDegradationClass, RecoveryHealth, StorageBudgetLedger,
};
use crate::branch::state::BranchLocalState;
use crate::commit::CommitBranchGuardSet;
use crate::format::{
    SnapshotSection, TableManifest, TableManifestInheritedLayer, TableManifestLevel,
    TableManifestTableRef,
};
use crate::layout::ObjectLayout;
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
    failure: Option<LifecycleError>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum LifecycleCheckpointStatus {
    Completed,
    DeferredNoVisibleRows,
    SnapshotPublishedManifestNotUpdated,
    SnapshotVisibilityUncertain,
    FlushWatermarkFailed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
#[allow(
    dead_code,
    reason = "flush-watermark proof vocabulary is exercised by dedicated maintenance tests"
)]
pub(crate) enum LifecycleFlushWatermarkProof {
    CheckpointCovered {
        snapshot_watermark: CommitVersion,
    },
    TableManifestCovered(LifecycleTableManifestFlushCoverageProof),
    Combined {
        checkpoint: CommitVersion,
        table_manifest: LifecycleTableManifestFlushCoverageProof,
    },
    AlreadyPersisted,
    // Always rejected by `persist_flush_watermark`. Carried in the enum so the
    // sensitivity probe can mutate the rejection arm and break the
    // `flush_watermark_proofs_are_conservative_and_monotonic` test if anyone
    // accidentally accepts table-object publication as a recovery proof.
    TableObjectsOnly {
        flushed_through: CommitVersion,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleTableManifestFlushCoverageProof {
    candidate: CommitVersion,
    manifest_epoch: u64,
    recovery_health_epoch: u64,
    branch_coverages: Vec<LifecycleTableManifestBranchCoverage>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleTableManifestBranchCoverage {
    branch_id: BranchId,
    covered_min: CommitVersion,
    covered_max: CommitVersion,
    covered_versions: Vec<CommitVersion>,
    manifest_sequence: u64,
    manifest_object: ObjectName,
    table_count: usize,
    row_families: LifecycleTableManifestCoverageFamilies,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleTableManifestCoverageFamilies {
    bits: u8,
}

const COVERAGE_USER_ROWS: u8 = 0b0000_0001;
const COVERAGE_TOMBSTONES: u8 = 0b0000_0010;
const COVERAGE_TIMELINE_ROWS: u8 = 0b0000_0100;
const COVERAGE_INHERITED_LAYERS: u8 = 0b0000_1000;
const COVERAGE_MATERIALIZED_REPLACEMENTS: u8 = 0b0001_0000;
const COVERAGE_COMPLETE: u8 = COVERAGE_USER_ROWS
    | COVERAGE_TOMBSTONES
    | COVERAGE_TIMELINE_ROWS
    | COVERAGE_INHERITED_LAYERS
    | COVERAGE_MATERIALIZED_REPLACEMENTS;
type TableManifestFlushContext<'a> = (u64, u64, &'a [(BranchId, u64)]);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleFlushWatermarkOutcome {
    candidate: CommitVersion,
    persisted: Option<CommitVersion>,
    already_persisted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleWalTruncationOutcome {
    covered_through: CommitVersion,
    deleted_segments: usize,
    protected_segments: usize,
    failed_segments: usize,
    recovery_health: Option<super::RecoveryHealth>,
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
            return Err(LifecycleError::InvalidConfig {
                field: "checkpoint_snapshot_id",
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
            failure: None,
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
            failure: None,
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
            failure: Some(LifecycleError::CheckpointSnapshotOrphaned {
                object: Some(snapshot.object().as_str().to_owned()),
                reason,
            }),
        })
    }

    fn with_flush_watermark(mut self, outcome: LifecycleFlushWatermarkOutcome) -> Self {
        self.flush_watermark = Some(outcome);
        self
    }

    fn with_wal_truncation(mut self, outcome: LifecycleWalTruncationOutcome) -> Self {
        if outcome.completed_with_health_debt() {
            self.recovery_health = outcome.recovery_health().cloned();
        }
        self.wal_truncation = Some(outcome);
        self
    }

    fn with_follow_up_health_debt(mut self, reason: &'static str) -> LifecycleResult<Self> {
        self.recovery_health = Some(telemetry_health_debt(reason)?);
        Ok(self)
    }

    fn with_follow_up_failure(
        mut self,
        status: LifecycleCheckpointStatus,
        reason: &'static str,
    ) -> LifecycleResult<Self> {
        self.status = status;
        self.recovery_health = Some(telemetry_health_debt(reason)?);
        self.failure = Some(LifecycleError::CheckpointPublicationFailed { reason });
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

    pub(crate) const fn failure(&self) -> Option<&LifecycleError> {
        self.failure.as_ref()
    }

    pub(crate) fn maintenance_outcome(&self) -> MaintenanceOutcome {
        let status = match self.status {
            LifecycleCheckpointStatus::Completed => MaintenanceOutcomeStatus::Completed,
            LifecycleCheckpointStatus::DeferredNoVisibleRows => MaintenanceOutcomeStatus::Deferred,
            LifecycleCheckpointStatus::SnapshotPublishedManifestNotUpdated
            | LifecycleCheckpointStatus::SnapshotVisibilityUncertain
            | LifecycleCheckpointStatus::FlushWatermarkFailed => MaintenanceOutcomeStatus::Failed,
        };
        let mut outcome = MaintenanceOutcome::new(MaintenanceTaskKind::Checkpoint, status)
            .with_effects(
                usize::from(self.snapshot_object.is_some()),
                0,
                self.retryable(),
            )
            .with_state_changes(usize::from(matches!(
                self.status,
                LifecycleCheckpointStatus::Completed
            )))
            .with_stats(LifecycleStats::new(0, 0, 1, 0, 0));
        if let Some(object) = &self.snapshot_object {
            outcome = outcome.with_affected_object_names(vec![object.as_str().to_owned()]);
        }
        if let Some(reason) = self.status_reason() {
            outcome = outcome.with_reason(reason);
        }
        if let Some(health) = self.recovery_health.clone() {
            outcome = outcome.with_recovery_health(health);
        }
        if let Some(error) = &self.failure {
            outcome = outcome.with_source_error(error.clone());
        }
        outcome
    }

    const fn status_reason(&self) -> Option<&'static str> {
        match self.status {
            LifecycleCheckpointStatus::Completed => None,
            LifecycleCheckpointStatus::DeferredNoVisibleRows => {
                Some("checkpoint has no visible rows to publish")
            }
            LifecycleCheckpointStatus::SnapshotPublishedManifestNotUpdated => {
                Some("checkpoint snapshot published before manifest update failed")
            }
            LifecycleCheckpointStatus::SnapshotVisibilityUncertain => {
                Some("checkpoint final manifest visibility is uncertain")
            }
            LifecycleCheckpointStatus::FlushWatermarkFailed => {
                Some("checkpoint flush watermark persistence failed")
            }
        }
    }

    const fn retryable(&self) -> bool {
        matches!(
            self.status,
            LifecycleCheckpointStatus::SnapshotPublishedManifestNotUpdated
                | LifecycleCheckpointStatus::SnapshotVisibilityUncertain
                | LifecycleCheckpointStatus::FlushWatermarkFailed
        )
    }
}

fn validate_flush_watermark_input(
    candidate: CommitVersion,
    proof: &LifecycleFlushWatermarkProof,
) -> LifecycleResult<()> {
    if candidate.as_u64() == 0 {
        return Err(LifecycleError::WalRetentionProofIncomplete {
            reason: "flush watermark candidate must be nonzero",
        });
    }
    match proof {
        LifecycleFlushWatermarkProof::TableManifestCovered(proof) => {
            proof.validate_for_candidate(candidate)?;
        }
        LifecycleFlushWatermarkProof::Combined { table_manifest, .. } => {
            table_manifest.validate_for_candidate(candidate)?;
        }
        LifecycleFlushWatermarkProof::CheckpointCovered { .. }
        | LifecycleFlushWatermarkProof::AlreadyPersisted
        | LifecycleFlushWatermarkProof::TableObjectsOnly { .. } => {}
    }
    Ok(())
}

fn validate_table_manifest_proof_extending_checkpoint(
    proof: &LifecycleTableManifestFlushCoverageProof,
    checkpoint_watermark: CommitVersion,
    manifest_epoch: Option<u64>,
    recovery_health_epoch: Option<u64>,
    required_branch_epochs: &[(BranchId, u64)],
) -> LifecycleResult<()> {
    let Some(manifest_epoch) = manifest_epoch else {
        return Err(LifecycleError::WalRetentionProofIncomplete {
            reason: "table manifest flush proof requires current manifest epoch",
        });
    };
    let Some(recovery_health_epoch) = recovery_health_epoch else {
        return Err(LifecycleError::WalRetentionProofIncomplete {
            reason: "table manifest flush proof requires current recovery health epoch",
        });
    };
    if manifest_epoch == 0 {
        return Err(LifecycleError::WalRetentionProofIncomplete {
            reason: "table manifest flush proof manifest epoch must be nonzero",
        });
    }
    if recovery_health_epoch == 0 {
        return Err(LifecycleError::WalRetentionProofIncomplete {
            reason: "table manifest flush proof recovery health epoch must be nonzero",
        });
    }

    let branch_epochs = sorted_required_branch_epochs(required_branch_epochs)?;
    if branch_epochs.is_empty() {
        return Err(LifecycleError::WalRetentionProofIncomplete {
            reason: "table manifest flush proof requires a current branch set",
        });
    }
    let required_branches = branch_epochs
        .iter()
        .map(|(branch, _)| *branch)
        .collect::<Vec<_>>();
    proof.validate_current_epochs(manifest_epoch, recovery_health_epoch)?;
    proof.validate_current_branch_epochs(&branch_epochs)?;
    proof.validate_required_branches(&required_branches)?;
    proof.validate_extends_checkpoint(checkpoint_watermark)
}

fn sorted_required_branch_epochs(
    branch_epochs: &[(BranchId, u64)],
) -> LifecycleResult<Vec<(BranchId, u64)>> {
    let mut branch_epochs = branch_epochs.to_vec();
    branch_epochs.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    if branch_epochs.iter().any(|(_, epoch)| *epoch == 0) {
        return Err(LifecycleError::WalRetentionProofIncomplete {
            reason: "table manifest flush proof branch epoch must be nonzero",
        });
    }
    if branch_epochs.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(LifecycleError::WalRetentionProofIncomplete {
            reason: "table manifest flush proof required branch epochs contain duplicates",
        });
    }
    Ok(branch_epochs)
}

#[allow(
    dead_code,
    reason = "table-manifest flush proof diagnostics are consumed by generated and closeout tests"
)]
impl LifecycleTableManifestFlushCoverageProof {
    pub(crate) fn new(
        candidate: CommitVersion,
        manifest_epoch: u64,
        recovery_health_epoch: u64,
        mut branch_coverages: Vec<LifecycleTableManifestBranchCoverage>,
    ) -> LifecycleResult<Self> {
        branch_coverages.sort_by(|left, right| {
            left.branch_id()
                .as_bytes()
                .cmp(right.branch_id().as_bytes())
                .then_with(|| {
                    left.manifest_object()
                        .as_str()
                        .cmp(right.manifest_object().as_str())
                })
        });
        let proof = Self {
            candidate,
            manifest_epoch,
            recovery_health_epoch,
            branch_coverages,
        };
        proof.validate()?;
        Ok(proof)
    }

    // Shape-only constructor: builds a proof from manifests without consulting
    // branch state, so the resulting `covered_versions` are empty. That means
    // `validate_extends_checkpoint` will always reject when the candidate sits
    // above the checkpoint watermark — there's no per-commit evidence that the
    // interval is covered. Use this for proof shape, staleness, and health
    // tests, and use `from_branch_manifest` when actually persisting.
    pub(crate) fn from_table_manifests(
        candidate: CommitVersion,
        manifests: &[TableManifest],
        health: &RecoveryHealth,
    ) -> LifecycleResult<Self> {
        let manifest_epoch = manifests
            .iter()
            .map(TableManifest::manifest_sequence)
            .max()
            .ok_or(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest flush proof requires at least one manifest",
            })?;
        let recovery_health_epoch = recovery_health_epoch(health)?;
        let branch_coverages = manifests
            .iter()
            .map(|manifest| branch_coverage_from_manifest(candidate, manifest))
            .collect::<LifecycleResult<Vec<_>>>()?;
        Self::new(
            candidate,
            manifest_epoch,
            recovery_health_epoch,
            branch_coverages,
        )
    }

    pub(crate) fn from_branch_manifest(
        candidate: CommitVersion,
        branch: &BranchLocalState,
        manifest: &TableManifest,
        health: &RecoveryHealth,
    ) -> LifecycleResult<Self> {
        if branch.branch_id() != manifest.branch_id() {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest flush proof branch does not match branch state",
            });
        }
        if branch_has_unflushed_rows_at_or_below(branch, candidate) {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason:
                    "mutable rows at or below flush watermark are not covered by table manifest",
            });
        }
        let recovery_health_epoch = recovery_health_epoch(health)?;
        let branch_coverage = branch_coverage_from_state_and_manifest(candidate, branch, manifest)?;
        Self::new(
            candidate,
            manifest.manifest_sequence(),
            recovery_health_epoch,
            vec![branch_coverage],
        )
    }

    pub(crate) fn validate_for_candidate(&self, candidate: CommitVersion) -> LifecycleResult<()> {
        self.validate()?;
        if self.candidate != candidate {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "flush watermark candidate does not match table manifest proof",
            });
        }
        Ok(())
    }

    pub(crate) fn validate_current_epochs(
        &self,
        manifest_epoch: u64,
        recovery_health_epoch: u64,
    ) -> LifecycleResult<()> {
        if self.manifest_epoch != manifest_epoch {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest flush proof is stale",
            });
        }
        if self.recovery_health_epoch != recovery_health_epoch {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest flush proof recovery health is stale",
            });
        }
        Ok(())
    }

    pub(crate) fn validate_current_branch_epochs(
        &self,
        branch_epochs: &[(BranchId, u64)],
    ) -> LifecycleResult<()> {
        for (branch, epoch) in branch_epochs {
            let Some(coverage) = self
                .branch_coverages
                .iter()
                .find(|coverage| coverage.branch_id() == *branch)
            else {
                return Err(LifecycleError::WalRetentionProofIncomplete {
                    reason: "table manifest flush proof is missing branch coverage",
                });
            };
            if coverage.manifest_sequence() != *epoch {
                return Err(LifecycleError::WalRetentionProofIncomplete {
                    reason: "table manifest flush proof has stale branch manifest epoch",
                });
            }
        }
        Ok(())
    }

    pub(crate) fn validate_required_branches(&self, branches: &[BranchId]) -> LifecycleResult<()> {
        for branch in branches {
            if !self
                .branch_coverages
                .iter()
                .any(|coverage| coverage.branch_id() == *branch)
            {
                return Err(LifecycleError::WalRetentionProofIncomplete {
                    reason: "table manifest flush proof is missing branch coverage",
                });
            }
        }
        Ok(())
    }

    pub(crate) fn validate_extends_checkpoint(
        &self,
        checkpoint_watermark: CommitVersion,
    ) -> LifecycleResult<()> {
        if checkpoint_watermark == CommitVersion::ZERO {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest flush proof requires a nonzero checkpoint lower bound",
            });
        }
        for coverage in &self.branch_coverages {
            coverage.validate_extends_checkpoint(checkpoint_watermark, self.candidate)?;
        }
        Ok(())
    }

    pub(crate) const fn candidate(&self) -> CommitVersion {
        self.candidate
    }

    pub(crate) const fn manifest_epoch(&self) -> u64 {
        self.manifest_epoch
    }

    pub(crate) const fn recovery_health_epoch(&self) -> u64 {
        self.recovery_health_epoch
    }

    pub(crate) fn branch_coverages(&self) -> &[LifecycleTableManifestBranchCoverage] {
        &self.branch_coverages
    }

    fn validate(&self) -> LifecycleResult<()> {
        if self.candidate == CommitVersion::ZERO {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "flush watermark candidate must be nonzero",
            });
        }
        if self.manifest_epoch == 0 {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest flush proof manifest epoch must be nonzero",
            });
        }
        if self.recovery_health_epoch == 0 {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest flush proof recovery health epoch must be nonzero",
            });
        }
        if self.branch_coverages.is_empty() {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest flush proof requires branch coverage",
            });
        }
        let mut previous: Option<BranchId> = None;
        for coverage in &self.branch_coverages {
            coverage.validate(self.candidate)?;
            if previous.is_some_and(|branch| branch == coverage.branch_id()) {
                return Err(LifecycleError::WalRetentionProofIncomplete {
                    reason: "table manifest flush proof contains duplicate branch coverage",
                });
            }
            previous = Some(coverage.branch_id());
        }
        Ok(())
    }
}

#[allow(
    dead_code,
    reason = "table-manifest branch coverage accessors are consumed by proof tests"
)]
impl LifecycleTableManifestBranchCoverage {
    pub(crate) fn new(
        branch_id: BranchId,
        covered_min: CommitVersion,
        covered_max: CommitVersion,
        manifest_object: ObjectName,
        table_count: usize,
        row_families: LifecycleTableManifestCoverageFamilies,
    ) -> LifecycleResult<Self> {
        let coverage = Self {
            branch_id,
            covered_min,
            covered_max,
            covered_versions: Vec::new(),
            manifest_sequence: 1,
            manifest_object,
            table_count,
            row_families,
        };
        coverage.validate(covered_max)?;
        Ok(coverage)
    }

    fn new_with_versions(
        branch_id: BranchId,
        covered_min: CommitVersion,
        covered_max: CommitVersion,
        mut covered_versions: Vec<CommitVersion>,
        manifest_sequence: u64,
        manifest_object: ObjectName,
        table_count: usize,
        row_families: LifecycleTableManifestCoverageFamilies,
    ) -> LifecycleResult<Self> {
        covered_versions.sort();
        covered_versions.dedup();
        let coverage = Self {
            branch_id,
            covered_min,
            covered_max,
            covered_versions,
            manifest_sequence,
            manifest_object,
            table_count,
            row_families,
        };
        coverage.validate(covered_max)?;
        Ok(coverage)
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn covered_min(&self) -> CommitVersion {
        self.covered_min
    }

    pub(crate) const fn covered_max(&self) -> CommitVersion {
        self.covered_max
    }

    pub(crate) fn covered_versions(&self) -> &[CommitVersion] {
        &self.covered_versions
    }

    pub(crate) const fn manifest_sequence(&self) -> u64 {
        self.manifest_sequence
    }

    pub(crate) const fn manifest_object(&self) -> &ObjectName {
        &self.manifest_object
    }

    pub(crate) const fn table_count(&self) -> usize {
        self.table_count
    }

    pub(crate) const fn row_families(&self) -> LifecycleTableManifestCoverageFamilies {
        self.row_families
    }

    fn validate(&self, candidate: CommitVersion) -> LifecycleResult<()> {
        if self.covered_min == CommitVersion::ZERO {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest branch coverage minimum must be nonzero",
            });
        }
        if self.covered_min > self.covered_max {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest branch coverage range is invalid",
            });
        }
        if self.covered_max < candidate {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "flush watermark candidate exceeds table manifest coverage",
            });
        }
        if self.table_count == 0 {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest branch coverage requires at least one table",
            });
        }
        self.row_families.validate()
    }

    fn validate_extends_checkpoint(
        &self,
        checkpoint_watermark: CommitVersion,
        candidate: CommitVersion,
    ) -> LifecycleResult<()> {
        if candidate <= checkpoint_watermark {
            return Ok(());
        }
        let Some(first_needed) = checkpoint_watermark.as_u64().checked_add(1) else {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest flush proof checkpoint lower bound overflowed",
            });
        };
        if self.covered_min.as_u64() > first_needed || self.covered_max < candidate {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest flush proof does not cover checkpoint extension range",
            });
        }
        if !versions_cover_interval(&self.covered_versions, first_needed, candidate.as_u64()) {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest flush proof has a commit-version gap",
            });
        }
        Ok(())
    }
}

#[allow(
    dead_code,
    reason = "row-family proof toggles are consumed by coverage tests"
)]
impl LifecycleTableManifestCoverageFamilies {
    pub(crate) const fn complete() -> Self {
        Self {
            bits: COVERAGE_COMPLETE,
        }
    }

    pub(crate) const fn without_tombstones(mut self) -> Self {
        self.bits &= !COVERAGE_TOMBSTONES;
        self
    }

    pub(crate) const fn without_timeline_rows(mut self) -> Self {
        self.bits &= !COVERAGE_TIMELINE_ROWS;
        self
    }

    pub(crate) const fn without_inherited_layers(mut self) -> Self {
        self.bits &= !COVERAGE_INHERITED_LAYERS;
        self
    }

    pub(crate) const fn user_rows(self) -> bool {
        self.bits & COVERAGE_USER_ROWS != 0
    }

    pub(crate) const fn tombstones(self) -> bool {
        self.bits & COVERAGE_TOMBSTONES != 0
    }

    pub(crate) const fn timeline_rows(self) -> bool {
        self.bits & COVERAGE_TIMELINE_ROWS != 0
    }

    pub(crate) const fn inherited_layers(self) -> bool {
        self.bits & COVERAGE_INHERITED_LAYERS != 0
    }

    pub(crate) const fn materialized_replacements(self) -> bool {
        self.bits & COVERAGE_MATERIALIZED_REPLACEMENTS != 0
    }

    const fn validate(self) -> LifecycleResult<()> {
        if !self.user_rows() {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest flush proof is missing user row coverage",
            });
        }
        if !self.tombstones() {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest flush proof is missing tombstone coverage",
            });
        }
        if !self.timeline_rows() {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest flush proof is missing timeline coverage",
            });
        }
        if !self.inherited_layers() {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest flush proof is missing inherited layer coverage",
            });
        }
        if !self.materialized_replacements() {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest flush proof is missing materialized replacement coverage",
            });
        }
        Ok(())
    }
}

fn branch_coverage_from_manifest(
    candidate: CommitVersion,
    manifest: &TableManifest,
) -> LifecycleResult<LifecycleTableManifestBranchCoverage> {
    let mut tables = manifest_table_refs(manifest);
    let Some(first) = tables.next() else {
        return Err(LifecycleError::WalRetentionProofIncomplete {
            reason: "table manifest flush proof requires at least one table",
        });
    };
    let mut covered_min = first.facts().commit_min();
    let mut covered_max = first.facts().commit_max();
    let mut table_count = 1usize;
    for table in tables {
        covered_min = covered_min.min(table.facts().commit_min());
        covered_max = covered_max.max(table.facts().commit_max());
        table_count = table_count.saturating_add(1);
    }
    let manifest_object = ObjectLayout::branch_table_manifest(&manifest.branch_id().to_string())
        .map_err(|source| {
            LifecycleError::lower_layer_with(
                LifecycleLowerLayer::Layout,
                "table manifest object layout failed",
                source,
            )
        })?;
    LifecycleTableManifestBranchCoverage::new_with_versions(
        manifest.branch_id(),
        covered_min,
        covered_max,
        Vec::new(),
        manifest.manifest_sequence(),
        manifest_object,
        table_count,
        LifecycleTableManifestCoverageFamilies::complete(),
    )
    .and_then(|coverage| {
        coverage.validate(candidate)?;
        Ok(coverage)
    })
}

fn branch_coverage_from_state_and_manifest(
    candidate: CommitVersion,
    branch: &BranchLocalState,
    manifest: &TableManifest,
) -> LifecycleResult<LifecycleTableManifestBranchCoverage> {
    let base = branch_coverage_from_manifest(candidate, manifest)?;
    let covered_versions = branch_durable_commit_versions_at_or_below(branch, candidate);
    LifecycleTableManifestBranchCoverage::new_with_versions(
        base.branch_id(),
        base.covered_min(),
        base.covered_max(),
        covered_versions,
        manifest.manifest_sequence(),
        base.manifest_object().clone(),
        base.table_count(),
        base.row_families(),
    )
}

pub(crate) fn branch_durable_commit_versions_at_or_below(
    branch: &BranchLocalState,
    candidate: CommitVersion,
) -> Vec<CommitVersion> {
    let mut versions = Vec::new();
    for table in branch.owned_levels().iter().flatten() {
        versions.extend(
            table
                .rows()
                .iter()
                .map(crate::table::TableRow::commit_version)
                .filter(|version| *version <= candidate),
        );
    }
    for layer in branch.inherited_layers() {
        for table in layer.owned_levels().iter().flatten() {
            versions.extend(
                table
                    .rows()
                    .iter()
                    .map(crate::table::TableRow::commit_version)
                    .filter(|version| *version <= candidate),
            );
        }
    }
    versions.sort();
    versions.dedup();
    versions
}

pub(crate) fn branch_durable_rows_cover_interval(
    branch: &BranchLocalState,
    checkpoint_watermark: CommitVersion,
    candidate: CommitVersion,
) -> bool {
    if candidate <= checkpoint_watermark {
        return true;
    }
    checkpoint_watermark
        .as_u64()
        .checked_add(1)
        .is_some_and(|first| {
            versions_cover_interval(
                &branch_durable_commit_versions_at_or_below(branch, candidate),
                first,
                candidate.as_u64(),
            )
        })
}

fn branch_has_unflushed_rows_at_or_below(
    branch: &BranchLocalState,
    candidate: CommitVersion,
) -> bool {
    branch
        .active()
        .iter()
        .any(|row| row.row().commit_version() <= candidate)
        || branch.frozen().iter().any(|table| {
            table
                .iter()
                .any(|row| row.row().commit_version() <= candidate)
        })
}

fn versions_cover_interval(versions: &[CommitVersion], first_needed: u64, candidate: u64) -> bool {
    let mut expected = first_needed;
    for version in versions {
        let raw = version.as_u64();
        if raw < expected {
            continue;
        }
        if raw != expected {
            return false;
        }
        if raw == candidate {
            return true;
        }
        let Some(next) = expected.checked_add(1) else {
            return false;
        };
        expected = next;
    }
    false
}

fn manifest_table_refs(manifest: &TableManifest) -> impl Iterator<Item = &TableManifestTableRef> {
    manifest
        .levels()
        .iter()
        .flat_map(TableManifestLevel::tables)
        .chain(
            manifest
                .inherited_layers()
                .iter()
                .flat_map(TableManifestInheritedLayer::levels)
                .flat_map(TableManifestLevel::tables),
        )
}

fn recovery_health_epoch(health: &RecoveryHealth) -> LifecycleResult<u64> {
    match health {
        RecoveryHealth::Healthy => Ok(1),
        RecoveryHealth::Degraded {
            class: RecoveryDegradationClass::Telemetry,
            faults,
        } => u64::try_from(faults.len())
            .map(|count| count.max(1))
            .map_err(|_| LifecycleError::WalRetentionProofIncomplete {
                reason: "recovery health epoch does not fit in u64",
            }),
        RecoveryHealth::Degraded { .. } | RecoveryHealth::Failed { .. } => {
            Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "unsafe recovery health cannot prove table manifest flush watermark",
            })
        }
    }
}

#[allow(
    dead_code,
    reason = "flush-watermark outcome accessors are consumed by maintenance tests"
)]
impl LifecycleFlushWatermarkOutcome {
    fn persisted(candidate: CommitVersion) -> Self {
        Self {
            candidate,
            persisted: Some(candidate),
            already_persisted: false,
        }
    }

    fn already_persisted(candidate: CommitVersion) -> Self {
        Self {
            candidate,
            persisted: Some(candidate),
            already_persisted: true,
        }
    }

    pub(crate) const fn was_persisted(&self) -> bool {
        !self.already_persisted
    }

    pub(crate) const fn was_already_persisted(&self) -> bool {
        self.already_persisted
    }

    pub(crate) const fn candidate(&self) -> CommitVersion {
        self.candidate
    }

    pub(crate) const fn persisted_watermark(&self) -> Option<CommitVersion> {
        self.persisted
    }

    pub(crate) fn maintenance_outcome(&self) -> MaintenanceOutcome {
        let state_changes = usize::from(self.was_persisted());
        MaintenanceOutcome::new(
            MaintenanceTaskKind::FlushWatermark,
            MaintenanceOutcomeStatus::Completed,
        )
        .with_state_changes(state_changes)
        .with_stats(LifecycleStats::new(0, 0, 1, 0, 0))
    }
}

pub(crate) fn validate_wal_retention_proof(
    proof: WalRetentionProof,
) -> LifecycleResult<WalRetentionProof> {
    if proof.covered_through() == CommitVersion::ZERO {
        return Err(LifecycleError::WalRetentionProofIncomplete {
            reason: "WAL retention proof must be nonzero",
        });
    }
    Ok(proof)
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
            covered_through: proof.covered_through(),
            deleted_segments: report.deleted_segments().len(),
            protected_segments: report.protected_segments().len(),
            failed_segments,
            recovery_health,
        })
    }

    pub(crate) const fn completed_cleanly(&self) -> bool {
        self.recovery_health.is_none()
    }

    pub(crate) const fn completed_with_health_debt(&self) -> bool {
        self.recovery_health.is_some()
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
        let status = if self.completed_cleanly() {
            MaintenanceOutcomeStatus::Completed
        } else {
            MaintenanceOutcomeStatus::Failed
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
    checkpoint_durable_branch_with_budget(
        branch,
        services,
        guard_set,
        read_visible_version,
        request,
        None,
    )
}

pub(crate) fn checkpoint_durable_branch_with_budget(
    branch: &BranchLocalState,
    services: &LifecycleDurableLocalServices<'_>,
    guard_set: &CommitBranchGuardSet,
    read_visible_version: impl FnOnce() -> CommitVersion,
    request: &LifecycleCheckpointRequest,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<LifecycleCheckpointOutcome> {
    request.validate()?;
    if branch.branch_id() != request.branch_id() {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "checkpoint branch id must match branch state",
        });
    }
    publish_checkpoint(
        services,
        guard_set,
        read_visible_version,
        request,
        budget,
        |visible_version| {
            branch
                .checkpoint_rows(visible_version)
                .map_err(branch_error)
        },
    )
}

/// Multi-branch checkpoint entry point: collect rows from every active
/// branch in the catalog and publish a single snapshot section that
/// covers all of them. Used by the durable maintenance dispatch path so
/// non-seeded branch rows survive restart even when no WAL tail covers
/// them. Rows carry `branch_id` via their `PhysicalKey`; the snapshot
/// section format is branch-agnostic.
pub(crate) fn checkpoint_durable_runtime_with_budget(
    branch_catalog: &crate::lifecycle::LifecycleBranchCatalog,
    services: &LifecycleDurableLocalServices<'_>,
    guard_set: &CommitBranchGuardSet,
    read_visible_version: impl FnOnce() -> CommitVersion,
    request: &LifecycleCheckpointRequest,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<LifecycleCheckpointOutcome> {
    request.validate()?;
    // The request's `branch_id` is informational — it identifies the
    // branch whose maintenance task triggered the checkpoint. The
    // encoder reads rows from every active branch in the catalog so
    // forked / created branches survive restart through the snapshot
    // path.
    publish_checkpoint(
        services,
        guard_set,
        read_visible_version,
        request,
        budget,
        |visible_version| {
            let active_descriptors = branch_catalog.list_branches(false);
            let mut combined = Vec::new();
            for descriptor in &active_descriptors {
                let branch = branch_catalog.branch_state(descriptor.branch_id())?;
                let mut rows = branch
                    .checkpoint_rows(visible_version)
                    .map_err(branch_error)?;
                combined.append(&mut rows);
            }
            Ok(combined)
        },
    )
}

fn publish_checkpoint(
    services: &LifecycleDurableLocalServices<'_>,
    guard_set: &CommitBranchGuardSet,
    read_visible_version: impl FnOnce() -> CommitVersion,
    request: &LifecycleCheckpointRequest,
    budget: Option<&StorageBudgetLedger>,
    collect_rows: impl FnOnce(CommitVersion) -> LifecycleResult<Vec<crate::row::StorageRow>>,
) -> LifecycleResult<LifecycleCheckpointOutcome> {
    let quiesce = guard_set.try_begin_quiesce().map_err(commit_error)?;
    let visible_version = read_visible_version();
    if visible_version == CommitVersion::ZERO {
        drop(quiesce);
        return Ok(LifecycleCheckpointOutcome::deferred(request));
    }
    let rows = collect_rows(visible_version)?;
    if rows.is_empty() {
        drop(quiesce);
        return Ok(LifecycleCheckpointOutcome::deferred(request));
    }
    let row_count =
        u64::try_from(rows.len()).map_err(|_| LifecycleError::CheckpointPublicationFailed {
            reason: "checkpoint row count must fit in u64",
        })?;
    validate_snapshot_id_advances(services.manifest(), request.snapshot_id())?;
    let mut sections = Vec::with_capacity(1 + request.extra_sections().len());
    sections.push(encode_checkpoint_row_section(&rows).map_err(format_error)?);
    sections.extend(request.extra_sections().iter().cloned());
    require_checkpoint_artifact_budget(budget, &sections)?;
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

fn require_checkpoint_artifact_budget(
    budget: Option<&StorageBudgetLedger>,
    sections: &[SnapshotSection],
) -> LifecycleResult<()> {
    let Some(budget) = budget else {
        return Ok(());
    };
    let bytes = sections.iter().try_fold(0_u64, |sum, section| {
        let payload_len = u64::try_from(section.payload().len()).map_err(|_| {
            LifecycleError::CheckpointPublicationFailed {
                reason: "checkpoint section length must fit in u64",
            }
        })?;
        sum.checked_add(payload_len)
            .and_then(|sum| sum.checked_add(1))
            .ok_or(LifecycleError::CheckpointPublicationFailed {
                reason: "checkpoint artifact size overflowed",
            })
    })?;
    require_generated_artifact_budget(budget, bytes, "checkpoint artifact exceeds storage budget")
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
            visible_version,
            &LifecycleFlushWatermarkProof::CheckpointCovered {
                snapshot_watermark: visible_version,
            },
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
            WalRetentionProof::snapshot_watermark(visible_version),
        ) else {
            return outcome.with_follow_up_health_debt("checkpoint WAL truncation failed");
        };
        outcome = outcome.with_wal_truncation(truncation);
    }
    Ok(outcome)
}

pub(crate) fn persist_flush_watermark(
    manifest: &DatabaseManifestService<'_>,
    visible_version: CommitVersion,
    candidate: CommitVersion,
    proof: &LifecycleFlushWatermarkProof,
) -> LifecycleResult<LifecycleFlushWatermarkOutcome> {
    persist_flush_watermark_inner(manifest, visible_version, candidate, proof, None)
}

pub(crate) fn persist_flush_watermark_with_table_manifest_proof(
    manifest: &DatabaseManifestService<'_>,
    visible_version: CommitVersion,
    candidate: CommitVersion,
    proof: &LifecycleFlushWatermarkProof,
    manifest_epoch: u64,
    recovery_health_epoch: u64,
    required_branch_epochs: &[(BranchId, u64)],
) -> LifecycleResult<LifecycleFlushWatermarkOutcome> {
    persist_flush_watermark_inner(
        manifest,
        visible_version,
        candidate,
        proof,
        Some((
            manifest_epoch,
            recovery_health_epoch,
            required_branch_epochs,
        )),
    )
}

fn persist_flush_watermark_inner(
    manifest: &DatabaseManifestService<'_>,
    visible_version: CommitVersion,
    candidate: CommitVersion,
    proof: &LifecycleFlushWatermarkProof,
    table_manifest_context: Option<TableManifestFlushContext<'_>>,
) -> LifecycleResult<LifecycleFlushWatermarkOutcome> {
    validate_flush_watermark_input(candidate, proof)?;
    if candidate > visible_version {
        return Err(LifecycleError::WalRetentionProofIncomplete {
            reason: "flush watermark candidate exceeds visible version",
        });
    }
    let current = manifest.load_required().map_err(manifest_error)?;
    if candidate_already_persisted(candidate, current.flushed_through_commit_id())? {
        return Ok(LifecycleFlushWatermarkOutcome::already_persisted(candidate));
    }
    validate_flush_watermark_proof(candidate, proof, &current, table_manifest_context)?;
    manifest
        .persist_flush_watermark(candidate)
        .map_err(manifest_error)?;
    Ok(LifecycleFlushWatermarkOutcome::persisted(candidate))
}

fn candidate_already_persisted(
    candidate: CommitVersion,
    persisted: Option<CommitVersion>,
) -> LifecycleResult<bool> {
    if let Some(persisted) = persisted {
        if candidate < persisted {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "flush watermark candidate is below current persisted watermark",
            });
        }
        if candidate == persisted {
            return Ok(true);
        }
    }
    Ok(false)
}

fn validate_flush_watermark_proof(
    candidate: CommitVersion,
    proof: &LifecycleFlushWatermarkProof,
    current: &crate::format::DatabaseManifest,
    table_manifest_context: Option<TableManifestFlushContext<'_>>,
) -> LifecycleResult<()> {
    match proof {
        LifecycleFlushWatermarkProof::CheckpointCovered { snapshot_watermark } => {
            validate_checkpoint_flush_watermark(candidate, *snapshot_watermark, current)?;
        }
        LifecycleFlushWatermarkProof::TableManifestCovered(proof) => {
            validate_table_manifest_flush_watermark(
                candidate,
                proof,
                current.snapshot_watermark().map(CommitVersion::new),
                table_manifest_context,
            )?;
        }
        LifecycleFlushWatermarkProof::Combined {
            checkpoint,
            table_manifest,
        } => {
            validate_combined_flush_watermark(
                candidate,
                *checkpoint,
                table_manifest,
                current,
                table_manifest_context,
            )?;
        }
        LifecycleFlushWatermarkProof::AlreadyPersisted => {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "flush watermark candidate is not already persisted",
            });
        }
        LifecycleFlushWatermarkProof::TableObjectsOnly { .. } => {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table object flush facts are not a recovery proof for flush watermark",
            });
        }
    }
    Ok(())
}

fn validate_checkpoint_flush_watermark(
    candidate: CommitVersion,
    snapshot_watermark: CommitVersion,
    current: &crate::format::DatabaseManifest,
) -> LifecycleResult<()> {
    if candidate > snapshot_watermark {
        return Err(LifecycleError::WalRetentionProofIncomplete {
            reason: "flush watermark candidate exceeds checkpoint proof",
        });
    }
    if current
        .snapshot_watermark()
        .is_none_or(|snapshot| candidate.as_u64() > snapshot)
    {
        return Err(LifecycleError::WalRetentionProofIncomplete {
            reason: "flush watermark candidate exceeds durable checkpoint facts",
        });
    }
    Ok(())
}

fn validate_table_manifest_flush_watermark(
    candidate: CommitVersion,
    proof: &LifecycleTableManifestFlushCoverageProof,
    snapshot_watermark: Option<CommitVersion>,
    table_manifest_context: Option<TableManifestFlushContext<'_>>,
) -> LifecycleResult<()> {
    proof.validate_for_candidate(candidate)?;
    let Some(snapshot_watermark) = snapshot_watermark else {
        return Err(LifecycleError::WalRetentionProofIncomplete {
            reason: "table manifest flush proof requires durable checkpoint facts",
        });
    };
    let (manifest_epoch, recovery_health_epoch, branch_epochs) =
        unpack_table_manifest_context(table_manifest_context);
    validate_table_manifest_proof_extending_checkpoint(
        proof,
        snapshot_watermark,
        manifest_epoch,
        recovery_health_epoch,
        branch_epochs,
    )
}

fn validate_combined_flush_watermark(
    candidate: CommitVersion,
    checkpoint: CommitVersion,
    table_manifest: &LifecycleTableManifestFlushCoverageProof,
    current: &crate::format::DatabaseManifest,
    table_manifest_context: Option<TableManifestFlushContext<'_>>,
) -> LifecycleResult<()> {
    if candidate <= checkpoint {
        return validate_checkpoint_flush_watermark(candidate, checkpoint, current);
    }
    if checkpoint == CommitVersion::ZERO {
        return Err(LifecycleError::WalRetentionProofIncomplete {
            reason: "table manifest flush proof requires a nonzero checkpoint lower bound",
        });
    }
    if current
        .snapshot_watermark()
        .is_none_or(|snapshot| checkpoint.as_u64() > snapshot)
    {
        return Err(LifecycleError::WalRetentionProofIncomplete {
            reason: "checkpoint lower bound exceeds durable checkpoint facts",
        });
    }
    let (manifest_epoch, recovery_health_epoch, branch_epochs) =
        unpack_table_manifest_context(table_manifest_context);
    table_manifest.validate_for_candidate(candidate)?;
    validate_table_manifest_proof_extending_checkpoint(
        table_manifest,
        checkpoint,
        manifest_epoch,
        recovery_health_epoch,
        branch_epochs,
    )
}

fn unpack_table_manifest_context(
    context: Option<TableManifestFlushContext<'_>>,
) -> (Option<u64>, Option<u64>, &[(BranchId, u64)]) {
    context.map_or((None, None, &[][..]), |(manifest, health, branches)| {
        (Some(manifest), Some(health), branches)
    })
}

pub(crate) fn truncate_wal(
    wal: &WalService<'_>,
    proof: WalRetentionProof,
) -> LifecycleResult<LifecycleWalTruncationOutcome> {
    let proof = validate_wal_retention_proof(proof)?;
    let report = wal.delete_covered_segments(proof).map_err(wal_error)?;
    LifecycleWalTruncationOutcome::completed(proof, &report)
}

pub(crate) fn checkpoint_request_from_maintenance_task(
    task: &MaintenanceTask,
    branch_id: BranchId,
    manifest: &DatabaseManifestService<'_>,
    created_at: Timestamp,
) -> LifecycleResult<LifecycleCheckpointRequest> {
    checkpoint_request_from_maintenance_task_with_snapshot_id(
        task, branch_id, manifest, created_at, None,
    )
}

pub(crate) fn checkpoint_request_from_maintenance_task_with_snapshot_id(
    task: &MaintenanceTask,
    branch_id: BranchId,
    manifest: &DatabaseManifestService<'_>,
    created_at: Timestamp,
    allocated_snapshot_id: Option<u64>,
) -> LifecycleResult<LifecycleCheckpointRequest> {
    if task.kind() != MaintenanceTaskKind::Checkpoint {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "maintenance task kind is not checkpoint",
        });
    }
    if !matches!(
        task.scope(),
        MaintenanceTaskScope::Checkpoint | MaintenanceTaskScope::Global
    ) {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "checkpoint task must target checkpoint scope",
        });
    }
    let current = manifest.load_required().map_err(manifest_error)?;
    let options = task.checkpoint_options();
    let snapshot_id = if let Some(snapshot_id) =
        options.and_then(super::maintenance::MaintenanceCheckpointOptions::snapshot_id)
    {
        snapshot_id
    } else if let Some(snapshot_id) = allocated_snapshot_id {
        snapshot_id
    } else {
        current.snapshot_id().unwrap_or(0).checked_add(1).ok_or(
            LifecycleError::CheckpointPublicationFailed {
                reason: "checkpoint snapshot id overflow",
            },
        )?
    };
    let mut request = LifecycleCheckpointRequest::new(branch_id, snapshot_id, created_at)?;
    if options.is_some_and(
        super::maintenance::MaintenanceCheckpointOptions::truncate_wal_after_checkpoint,
    ) {
        request = request.with_wal_truncation_after_checkpoint(true);
    }
    Ok(request)
}

pub(crate) fn wal_truncation_request_from_maintenance_task(
    task: &MaintenanceTask,
    manifest: &DatabaseManifestService<'_>,
) -> LifecycleResult<Option<WalRetentionProof>> {
    if task.kind() != MaintenanceTaskKind::WalTruncation {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "maintenance task kind is not WAL truncation",
        });
    }
    if task.scope() != MaintenanceTaskScope::Wal {
        return Err(LifecycleError::MaintenanceTaskFailed {
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
    Ok(Some(validate_wal_retention_proof(proof)?))
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
        return Err(LifecycleError::CheckpointPublicationFailed {
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
