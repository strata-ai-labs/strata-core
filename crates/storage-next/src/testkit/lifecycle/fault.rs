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
    let recovery = check_lifecycle_recovery_contract(script)?;
    let bootstrap = check_lifecycle_bootstrap_contract(script)?;
    let scaffold = check_lifecycle_scaffold_contract(script)?;
    let checkpoint = check_lifecycle_checkpoint_contract(script)?;
    let flush = check_lifecycle_flush_contract(script)?;
    let rewrite = check_lifecycle_table_rewrite_contract(script)?;
    let retention = check_lifecycle_retention_contract(script)?;
    let quarantine = check_lifecycle_quarantine_contract(script)?;
    let close = check_lifecycle_close_contract(script)?;

    check_open_fault_routes(&scaffold, &mut outcome)?;
    check_checkpoint_fault_routes(&checkpoint, &mut outcome)?;
    check_recovery_fault_routes(&recovery, &bootstrap, &mut outcome)?;
    check_rewrite_fault_routes(&flush, &rewrite, &mut outcome)?;
    check_reclaim_fault_routes(&retention, &quarantine, &mut outcome)?;
    check_close_fault_routes(&close, &mut outcome)?;
    Ok(outcome)
}

fn check_open_fault_routes(
    scaffold: &LifecycleScaffoldOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        scaffold.capability_preflight_cases() > 0 && scaffold.missing_capability_cases() > 0,
        "capability preflight fault route not covered",
    )?;
    outcome.capability_preflight += 1;
    ensure(
        scaffold.durable_writer_lock_failure_cases() > 0
            && scaffold.durable_manifest_create_cases() > 0,
        "writer-guard or manifest-create fault route not covered",
    )?;
    outcome.writer_guard_manifest_create += 1;
    ensure(
        scaffold.durable_manifest_publish_fault_cases() > 0,
        "manifest publish uncertainty route not covered",
    )?;
    outcome.manifest_publish_uncertain += 1;
    Ok(())
}

fn check_checkpoint_fault_routes(
    checkpoint: &LifecycleCheckpointContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        checkpoint.partial_window_cases() > 0,
        "checkpoint partial publication window not covered",
    )?;
    outcome.snapshot_orphan += 1;
    ensure(
        checkpoint.delete_failure_cases() > 0,
        "checkpoint truncation health debt not covered",
    )?;
    outcome.checkpoint_truncation_debt += 1;
    Ok(())
}

fn check_recovery_fault_routes(
    recovery: &LifecycleRecoveryContractOutcome,
    bootstrap: &LifecycleBootstrapContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        recovery.strict_failure_cases() > 0,
        "strict partial log fault route not covered",
    )?;
    outcome.partial_log_strict += 1;
    ensure(
        recovery.lossy_degradation_cases() > 0,
        "lossy partial log fault route not covered",
    )?;
    outcome.partial_log_lossy += 1;
    ensure(
        recovery.strict_failure_cases() > 0 && bootstrap.replay_rejection_cases() > 0,
        "typed corrupt recovery route not covered",
    )?;
    outcome.corrupt_log_typed += 1;

    ensure(
        bootstrap.replay_rejection_cases() > 0,
        "replay failure route not covered",
    )?;
    outcome.replay_failed_state += 1;
    ensure(
        bootstrap.degraded_bootstrap_cases() > 0,
        "replay visible-debt route not covered",
    )?;
    outcome.replay_visible_debt += 1;
    Ok(())
}

fn check_rewrite_fault_routes(
    flush: &LifecycleFlushContractOutcome,
    rewrite: &LifecycleTableRewriteContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        flush.publish_failure_cases() > 0 && flush.reopen_failure_cases() > 0,
        "flush orphan table route not covered",
    )?;
    outcome.flush_orphan_table += 1;
    ensure(
        rewrite.cache_compaction_cases() > 0 && rewrite.materialization_cases() > 0,
        "table rewrite read-preservation route not covered",
    )?;
    outcome.rewrite_preserved_reads += 1;
    Ok(())
}

fn check_reclaim_fault_routes(
    retention: &LifecycleRetentionContractOutcome,
    quarantine: &LifecycleQuarantineContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        retention.incomplete_proof_cases() > 0 && retention.blocked_recovery_cases() > 0,
        "retention proof did not block unsafe delete",
    )?;
    outcome.retention_blocked_delete += 1;
    ensure(
        quarantine.inventory_publish_failure_cases() > 0
            && quarantine.stale_purge_proof_cases() > 0,
        "quarantine publish/purge fault route not covered",
    )?;
    outcome.quarantine_publish_blocked_purge += 1;
    ensure(
        quarantine.purge_delete_failure_cases() > 0,
        "purge delete debt route not covered",
    )?;
    outcome.purge_delete_debt += 1;
    Ok(())
}

fn check_close_fault_routes(
    close: &LifecycleCloseContractOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    ensure(
        close.commit_quiesce_blocked_cases() > 0 && close.retryable_timeout_cases() > 0,
        "close quiesce timeout route not covered",
    )?;
    outcome.close_quiesce_timeout += 1;
    ensure(
        close.wal_sync_failure_cases() > 0 && close.source_chain_preserved_cases() > 0,
        "close log sync source route not covered",
    )?;
    outcome.close_log_sync_source += 1;
    ensure(
        close.manifest_sync_failure_cases() > 0,
        "close manifest sync debt route not covered",
    )?;
    outcome.close_manifest_sync_debt += 1;
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
