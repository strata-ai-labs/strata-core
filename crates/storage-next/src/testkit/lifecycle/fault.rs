//! Lifecycle fault-window assurance helpers.

use super::{
    check_lifecycle_bootstrap_contract, check_lifecycle_checkpoint_contract,
    check_lifecycle_close_contract, check_lifecycle_flush_contract,
    check_lifecycle_quarantine_contract, check_lifecycle_recovery_contract,
    check_lifecycle_retention_contract, check_lifecycle_scaffold_contract,
    check_lifecycle_table_rewrite_contract, ensure, LifecycleBootstrapContractOutcome,
    LifecycleCheckpointContractOutcome, LifecycleCloseContractOutcome,
    LifecycleFlushContractOutcome, LifecycleQuarantineContractOutcome,
    LifecycleRecoveryContractOutcome, LifecycleRetentionContractOutcome, LifecycleScaffoldOutcome,
    LifecycleTableRewriteContractOutcome,
};
#[cfg(any(test, feature = "fault-injection"))]
use crate::testkit::run_service_fault_window_harness;
use crate::testkit::TestkitError;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LifecycleFaultContractOutcome {
    capability_preflight: usize,
    writer_guard_manifest_create: usize,
    manifest_publish_uncertain: usize,
    snapshot_orphan: usize,
    checkpoint_truncation_debt: usize,
    partial_log_strict: usize,
    partial_log_lossy: usize,
    corrupt_log_typed: usize,
    replay_failed_state: usize,
    replay_visible_debt: usize,
    flush_orphan_table: usize,
    rewrite_preserved_reads: usize,
    retention_blocked_delete: usize,
    quarantine_publish_blocked_purge: usize,
    purge_delete_debt: usize,
    close_quiesce_timeout: usize,
    close_log_sync_source: usize,
    close_manifest_sync_debt: usize,
    writer_guard_release_typed: usize,
}

pub fn check_lifecycle_fault_contract(
    script: &[u8],
) -> Result<LifecycleFaultContractOutcome, TestkitError> {
    let mut outcome = LifecycleFaultContractOutcome::default();
    match FaultRoute::from_script(script) {
        FaultRoute::CapabilityPreflight => {
            let scaffold = check_lifecycle_scaffold_contract(script)?;
            check_capability_preflight_route(&scaffold, &mut outcome)?;
        }
        FaultRoute::WriterGuardManifestCreate => {
            let scaffold = check_lifecycle_scaffold_contract(script)?;
            check_writer_guard_manifest_create_route(&scaffold, &mut outcome)?;
        }
        FaultRoute::ManifestPublishUncertain => {
            let scaffold = check_lifecycle_scaffold_contract(script)?;
            check_manifest_publish_uncertain_route(&scaffold, &mut outcome)?;
        }
        FaultRoute::SnapshotOrphan => {
            let checkpoint = check_lifecycle_checkpoint_contract(script)?;
            check_snapshot_orphan_route(&checkpoint, &mut outcome)?;
        }
        FaultRoute::CheckpointTruncationDebt => {
            let checkpoint = check_lifecycle_checkpoint_contract(script)?;
            check_checkpoint_truncation_debt_route(&checkpoint, &mut outcome)?;
        }
        FaultRoute::PartialLogStrict => {
            let recovery = check_lifecycle_recovery_contract(script)?;
            check_partial_log_strict_route(&recovery, &mut outcome)?;
        }
        FaultRoute::PartialLogLossy => {
            let recovery = check_lifecycle_recovery_contract(script)?;
            check_partial_log_lossy_route(&recovery, &mut outcome)?;
        }
        FaultRoute::CorruptLogTyped => {
            let recovery = check_lifecycle_recovery_contract(script)?;
            let bootstrap = check_lifecycle_bootstrap_contract(script)?;
            check_corrupt_log_typed_route(&recovery, &bootstrap, &mut outcome)?;
        }
        FaultRoute::ReplayFailedState => {
            let bootstrap = check_lifecycle_bootstrap_contract(script)?;
            check_replay_failed_state_route(&bootstrap, &mut outcome)?;
        }
        FaultRoute::ReplayVisibleDebt => {
            let bootstrap = check_lifecycle_bootstrap_contract(script)?;
            check_replay_visible_debt_route(&bootstrap, &mut outcome)?;
        }
        FaultRoute::FlushOrphanTable => {
            let flush = check_lifecycle_flush_contract(script)?;
            check_flush_orphan_table_route(&flush, &mut outcome)?;
        }
        FaultRoute::RewritePreservedReads => {
            let rewrite = check_lifecycle_table_rewrite_contract(script)?;
            check_rewrite_preserved_reads_route(&rewrite, &mut outcome)?;
        }
        FaultRoute::RetentionBlockedDelete => {
            let retention = check_lifecycle_retention_contract(script)?;
            check_retention_blocked_delete_route(&retention, &mut outcome)?;
        }
        FaultRoute::QuarantinePublishBlockedPurge => {
            let quarantine = check_lifecycle_quarantine_contract(script)?;
            check_quarantine_publish_blocked_purge_route(&quarantine, &mut outcome)?;
        }
        FaultRoute::PurgeDeleteDebt => {
            let quarantine = check_lifecycle_quarantine_contract(script)?;
            check_purge_delete_debt_route(&quarantine, &mut outcome)?;
        }
        FaultRoute::CloseQuiesceTimeout => {
            let close = check_lifecycle_close_contract(script)?;
            check_close_quiesce_timeout_route(&close, &mut outcome)?;
        }
        FaultRoute::CloseLogSyncSource => {
            let close = check_lifecycle_close_contract(script)?;
            check_close_log_sync_source_route(&close, &mut outcome)?;
        }
        FaultRoute::CloseManifestSyncDebt => {
            let close = check_lifecycle_close_contract(script)?;
            check_close_manifest_sync_debt_route(&close, &mut outcome)?;
        }
        FaultRoute::WriterGuardReleaseTyped => {
            let close = check_lifecycle_close_contract(script)?;
            check_writer_guard_release_typed_route(&close, &mut outcome)?;
        }
    }
    Ok(outcome)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FaultRoute {
    CapabilityPreflight,
    WriterGuardManifestCreate,
    ManifestPublishUncertain,
    SnapshotOrphan,
    CheckpointTruncationDebt,
    PartialLogStrict,
    PartialLogLossy,
    CorruptLogTyped,
    ReplayFailedState,
    ReplayVisibleDebt,
    FlushOrphanTable,
    RewritePreservedReads,
    RetentionBlockedDelete,
    QuarantinePublishBlockedPurge,
    PurgeDeleteDebt,
    CloseQuiesceTimeout,
    CloseLogSyncSource,
    CloseManifestSyncDebt,
    WriterGuardReleaseTyped,
}

impl FaultRoute {
    fn from_script(script: &[u8]) -> Self {
        let lower = String::from_utf8_lossy(script).to_ascii_lowercase();
        if lower.contains("capability") {
            return Self::CapabilityPreflight;
        }
        if lower.contains("writer-guard") {
            return Self::WriterGuardManifestCreate;
        }
        if lower.contains("manifest-publish") {
            return Self::ManifestPublishUncertain;
        }
        if lower.contains("snapshot-orphan") {
            return Self::SnapshotOrphan;
        }
        if lower.contains("wal-truncation") {
            return Self::CheckpointTruncationDebt;
        }
        if lower.contains("partial-log-strict") {
            return Self::PartialLogStrict;
        }
        if lower.contains("partial-log-lossy") {
            return Self::PartialLogLossy;
        }
        if lower.contains("corrupt-log") {
            return Self::CorruptLogTyped;
        }
        if lower.contains("replay-failed") {
            return Self::ReplayFailedState;
        }
        if lower.contains("replay-visible") {
            return Self::ReplayVisibleDebt;
        }
        if lower.contains("flush-orphan") {
            return Self::FlushOrphanTable;
        }
        if lower.contains("rewrite-preserved") {
            return Self::RewritePreservedReads;
        }
        if lower.contains("retention-blocked") {
            return Self::RetentionBlockedDelete;
        }
        if lower.contains("quarantine-publish") {
            return Self::QuarantinePublishBlockedPurge;
        }
        if lower.contains("purge-delete") {
            return Self::PurgeDeleteDebt;
        }
        if lower.contains("close-quiesce") {
            return Self::CloseQuiesceTimeout;
        }
        if lower.contains("close-wal-sync") {
            return Self::CloseLogSyncSource;
        }
        if lower.contains("close-manifest") {
            return Self::CloseManifestSyncDebt;
        }
        if lower.contains("close-guard-release") {
            return Self::WriterGuardReleaseTyped;
        }
        match script.first().copied().unwrap_or(0) % 19 {
            0 => Self::CapabilityPreflight,
            1 => Self::WriterGuardManifestCreate,
            2 => Self::ManifestPublishUncertain,
            3 => Self::SnapshotOrphan,
            4 => Self::CheckpointTruncationDebt,
            5 => Self::PartialLogStrict,
            6 => Self::PartialLogLossy,
            7 => Self::CorruptLogTyped,
            8 => Self::ReplayFailedState,
            9 => Self::ReplayVisibleDebt,
            10 => Self::FlushOrphanTable,
            11 => Self::RewritePreservedReads,
            12 => Self::RetentionBlockedDelete,
            13 => Self::QuarantinePublishBlockedPurge,
            14 => Self::PurgeDeleteDebt,
            15 => Self::CloseQuiesceTimeout,
            16 => Self::CloseLogSyncSource,
            17 => Self::CloseManifestSyncDebt,
            _ => Self::WriterGuardReleaseTyped,
        }
    }
}

fn check_capability_preflight_route(
    scaffold: &LifecycleScaffoldOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        scaffold.capability_preflight_cases() > 0 && scaffold.missing_capability_cases() > 0,
        "capability preflight fault route not covered",
    )?;
    outcome.capability_preflight += 1;
    Ok(())
}

fn check_writer_guard_manifest_create_route(
    scaffold: &LifecycleScaffoldOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        scaffold.durable_writer_lock_failure_cases() > 0
            && scaffold.durable_manifest_create_cases() > 0,
        "writer-guard or manifest-create fault route not covered",
    )?;
    outcome.writer_guard_manifest_create += 1;
    Ok(())
}

fn check_manifest_publish_uncertain_route(
    scaffold: &LifecycleScaffoldOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        scaffold.durable_manifest_publish_fault_cases() > 0,
        "manifest publish uncertainty route not covered",
    )?;
    outcome.manifest_publish_uncertain += 1;
    Ok(())
}

fn check_snapshot_orphan_route(
    checkpoint: &LifecycleCheckpointContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    check_service_fault_window_harness()?;
    ensure(
        checkpoint.partial_window_cases() > 0,
        "checkpoint partial publication window not covered",
    )?;
    outcome.snapshot_orphan += 1;
    Ok(())
}

fn check_checkpoint_truncation_debt_route(
    checkpoint: &LifecycleCheckpointContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        checkpoint.delete_failure_cases() > 0,
        "checkpoint truncation health debt not covered",
    )?;
    outcome.checkpoint_truncation_debt += 1;
    Ok(())
}

fn check_partial_log_strict_route(
    recovery: &LifecycleRecoveryContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        recovery.strict_failure_cases() > 0,
        "strict partial log fault route not covered",
    )?;
    outcome.partial_log_strict += 1;
    Ok(())
}

fn check_partial_log_lossy_route(
    recovery: &LifecycleRecoveryContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        recovery.lossy_degradation_cases() > 0,
        "lossy partial log fault route not covered",
    )?;
    outcome.partial_log_lossy += 1;
    Ok(())
}

fn check_corrupt_log_typed_route(
    recovery: &LifecycleRecoveryContractOutcome,
    bootstrap: &LifecycleBootstrapContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        recovery.strict_failure_cases() > 0 && bootstrap.replay_rejection_cases() > 0,
        "typed corrupt recovery route not covered",
    )?;
    outcome.corrupt_log_typed += 1;
    Ok(())
}

fn check_replay_failed_state_route(
    bootstrap: &LifecycleBootstrapContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        bootstrap.replay_rejection_cases() > 0,
        "replay failure route not covered",
    )?;
    outcome.replay_failed_state += 1;
    Ok(())
}

fn check_replay_visible_debt_route(
    bootstrap: &LifecycleBootstrapContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        bootstrap.degraded_bootstrap_cases() > 0,
        "replay visible-debt route not covered",
    )?;
    outcome.replay_visible_debt += 1;
    Ok(())
}

fn check_flush_orphan_table_route(
    flush: &LifecycleFlushContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    check_service_fault_window_harness()?;
    ensure(
        flush.publish_failure_cases() > 0 && flush.reopen_failure_cases() > 0,
        "flush orphan table route not covered",
    )?;
    outcome.flush_orphan_table += 1;
    Ok(())
}

fn check_rewrite_preserved_reads_route(
    rewrite: &LifecycleTableRewriteContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        rewrite.cache_compaction_cases() > 0 && rewrite.materialization_cases() > 0,
        "table rewrite read-preservation route not covered",
    )?;
    outcome.rewrite_preserved_reads += 1;
    Ok(())
}

fn check_retention_blocked_delete_route(
    retention: &LifecycleRetentionContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        retention.incomplete_proof_cases() > 0 && retention.blocked_recovery_cases() > 0,
        "retention proof did not block unsafe delete",
    )?;
    outcome.retention_blocked_delete += 1;
    Ok(())
}

fn check_quarantine_publish_blocked_purge_route(
    quarantine: &LifecycleQuarantineContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    check_service_fault_window_harness()?;
    ensure(
        quarantine.inventory_publish_failure_cases() > 0
            && quarantine.stale_purge_proof_cases() > 0,
        "quarantine publish/purge fault route not covered",
    )?;
    outcome.quarantine_publish_blocked_purge += 1;
    Ok(())
}

fn check_service_fault_window_harness() -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    {
        let service_faults = run_service_fault_window_harness()?;
        ensure(
            service_faults.cases_executed() > 0,
            "fault injection service harness did not execute",
        )?;
    }
    Ok(())
}

fn check_purge_delete_debt_route(
    quarantine: &LifecycleQuarantineContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        quarantine.purge_delete_failure_cases() > 0,
        "purge delete debt route not covered",
    )?;
    outcome.purge_delete_debt += 1;
    Ok(())
}

fn check_close_quiesce_timeout_route(
    close: &LifecycleCloseContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        close.commit_quiesce_blocked_cases() > 0 && close.retryable_timeout_cases() > 0,
        "close quiesce timeout route not covered",
    )?;
    outcome.close_quiesce_timeout += 1;
    Ok(())
}

fn check_close_log_sync_source_route(
    close: &LifecycleCloseContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        close.wal_sync_failure_cases() > 0 && close.source_chain_preserved_cases() > 0,
        "close log sync source route not covered",
    )?;
    outcome.close_log_sync_source += 1;
    Ok(())
}

fn check_close_manifest_sync_debt_route(
    close: &LifecycleCloseContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        close.manifest_sync_failure_cases() > 0,
        "close manifest sync debt route not covered",
    )?;
    outcome.close_manifest_sync_debt += 1;
    Ok(())
}

fn check_writer_guard_release_typed_route(
    close: &LifecycleCloseContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        close.guard_release_observed_cases() > 0,
        "writer guard release route not covered",
    )?;
    outcome.writer_guard_release_typed += 1;
    Ok(())
}

impl LifecycleFaultContractOutcome {
    pub const fn capability_preflight_cases(&self) -> usize {
        self.capability_preflight
    }

    pub const fn writer_guard_manifest_create_cases(&self) -> usize {
        self.writer_guard_manifest_create
    }

    pub const fn manifest_publish_uncertain_cases(&self) -> usize {
        self.manifest_publish_uncertain
    }

    pub const fn snapshot_orphan_cases(&self) -> usize {
        self.snapshot_orphan
    }

    pub const fn checkpoint_truncation_debt_cases(&self) -> usize {
        self.checkpoint_truncation_debt
    }

    pub const fn partial_log_strict_cases(&self) -> usize {
        self.partial_log_strict
    }

    pub const fn partial_log_lossy_cases(&self) -> usize {
        self.partial_log_lossy
    }

    pub const fn corrupt_log_typed_cases(&self) -> usize {
        self.corrupt_log_typed
    }

    pub const fn replay_failed_state_cases(&self) -> usize {
        self.replay_failed_state
    }

    pub const fn replay_visible_debt_cases(&self) -> usize {
        self.replay_visible_debt
    }

    pub const fn flush_orphan_table_cases(&self) -> usize {
        self.flush_orphan_table
    }

    pub const fn rewrite_preserved_read_cases(&self) -> usize {
        self.rewrite_preserved_reads
    }

    pub const fn retention_blocked_delete_cases(&self) -> usize {
        self.retention_blocked_delete
    }

    pub const fn quarantine_publish_blocked_purge_cases(&self) -> usize {
        self.quarantine_publish_blocked_purge
    }

    pub const fn purge_delete_debt_cases(&self) -> usize {
        self.purge_delete_debt
    }

    pub const fn close_quiesce_timeout_cases(&self) -> usize {
        self.close_quiesce_timeout
    }

    pub const fn close_log_sync_source_cases(&self) -> usize {
        self.close_log_sync_source
    }

    pub const fn close_manifest_sync_debt_cases(&self) -> usize {
        self.close_manifest_sync_debt
    }

    pub const fn writer_guard_release_typed_cases(&self) -> usize {
        self.writer_guard_release_typed
    }
}
