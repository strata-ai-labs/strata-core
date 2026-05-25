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
    // The service-fault harness runs all 19 FaultingBackend (and equivalent
    // filesystem/decoder) injection cases. Each route below asserts on its
    // own counter from the resulting outcome — counter-presence here means
    // the route's specific fault case actually executed, not just that some
    // contract's pre-existing fixture incremented an unrelated counter.
    let service_faults = collect_service_fault_harness()?;
    match FaultRoute::from_script(script) {
        FaultRoute::CapabilityPreflight => {
            let scaffold = check_lifecycle_scaffold_contract(script)?;
            check_capability_preflight_route(&scaffold, &service_faults, &mut outcome)?;
        }
        FaultRoute::WriterGuardManifestCreate => {
            let scaffold = check_lifecycle_scaffold_contract(script)?;
            check_writer_guard_manifest_create_route(&scaffold, &service_faults, &mut outcome)?;
        }
        FaultRoute::ManifestPublishUncertain => {
            let scaffold = check_lifecycle_scaffold_contract(script)?;
            check_manifest_publish_uncertain_route(&scaffold, &service_faults, &mut outcome)?;
        }
        FaultRoute::SnapshotOrphan => {
            let checkpoint = check_lifecycle_checkpoint_contract(script)?;
            check_snapshot_orphan_route(&checkpoint, &service_faults, &mut outcome)?;
        }
        FaultRoute::CheckpointTruncationDebt => {
            let checkpoint = check_lifecycle_checkpoint_contract(script)?;
            check_checkpoint_truncation_debt_route(&checkpoint, &service_faults, &mut outcome)?;
        }
        FaultRoute::PartialLogStrict => {
            let recovery = check_lifecycle_recovery_contract(script)?;
            check_partial_log_strict_route(&recovery, &service_faults, &mut outcome)?;
        }
        FaultRoute::PartialLogLossy => {
            let recovery = check_lifecycle_recovery_contract(script)?;
            check_partial_log_lossy_route(&recovery, &service_faults, &mut outcome)?;
        }
        FaultRoute::CorruptLogTyped => {
            let recovery = check_lifecycle_recovery_contract(script)?;
            let bootstrap = check_lifecycle_bootstrap_contract(script)?;
            check_corrupt_log_typed_route(&recovery, &bootstrap, &service_faults, &mut outcome)?;
        }
        FaultRoute::ReplayFailedState => {
            let bootstrap = check_lifecycle_bootstrap_contract(script)?;
            check_replay_failed_state_route(&bootstrap, &service_faults, &mut outcome)?;
        }
        FaultRoute::ReplayVisibleDebt => {
            let bootstrap = check_lifecycle_bootstrap_contract(script)?;
            check_replay_visible_debt_route(&bootstrap, &service_faults, &mut outcome)?;
        }
        FaultRoute::FlushOrphanTable => {
            let flush = check_lifecycle_flush_contract(script)?;
            check_flush_orphan_table_route(&flush, &service_faults, &mut outcome)?;
        }
        FaultRoute::RewritePreservedReads => {
            let rewrite = check_lifecycle_table_rewrite_contract(script)?;
            check_rewrite_preserved_reads_route(&rewrite, &service_faults, &mut outcome)?;
        }
        FaultRoute::RetentionBlockedDelete => {
            let retention = check_lifecycle_retention_contract(script)?;
            check_retention_blocked_delete_route(&retention, &service_faults, &mut outcome)?;
        }
        FaultRoute::QuarantinePublishBlockedPurge => {
            let quarantine = check_lifecycle_quarantine_contract(script)?;
            check_quarantine_publish_blocked_purge_route(
                &quarantine,
                &service_faults,
                &mut outcome,
            )?;
        }
        FaultRoute::PurgeDeleteDebt => {
            let quarantine = check_lifecycle_quarantine_contract(script)?;
            check_purge_delete_debt_route(&quarantine, &service_faults, &mut outcome)?;
        }
        FaultRoute::CloseQuiesceTimeout => {
            let close = check_lifecycle_close_contract(script)?;
            check_close_quiesce_timeout_route(&close, &service_faults, &mut outcome)?;
        }
        FaultRoute::CloseLogSyncSource => {
            let close = check_lifecycle_close_contract(script)?;
            check_close_log_sync_source_route(&close, &service_faults, &mut outcome)?;
        }
        FaultRoute::CloseManifestSyncDebt => {
            let close = check_lifecycle_close_contract(script)?;
            check_close_manifest_sync_debt_route(&close, &service_faults, &mut outcome)?;
        }
        FaultRoute::WriterGuardReleaseTyped => {
            let close = check_lifecycle_close_contract(script)?;
            check_writer_guard_release_typed_route(&close, &service_faults, &mut outcome)?;
        }
    }
    Ok(outcome)
}

#[cfg(any(test, feature = "fault-injection"))]
type ServiceFaultHarnessOutcome = crate::testkit::ServiceFaultWindowHarnessOutcome;
#[cfg(not(any(test, feature = "fault-injection")))]
type ServiceFaultHarnessOutcome = ();

fn collect_service_fault_harness() -> Result<ServiceFaultHarnessOutcome, TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    {
        let outcome = run_service_fault_window_harness()?;
        ensure(
            outcome.cases_executed() >= 19,
            "service fault harness did not execute all 19 routes",
        )?;
        Ok(outcome)
    }
    #[cfg(not(any(test, feature = "fault-injection")))]
    {
        Ok(())
    }
}

#[cfg(any(test, feature = "fault-injection"))]
fn require_service_fault_route(
    outcome: &ServiceFaultHarnessOutcome,
    cases: usize,
    route: &'static str,
) -> Result<(), TestkitError> {
    ensure(
        cases > 0,
        // The static message is the only deterministic surface the harness
        // can return; the dynamic route label lives in the outcome for
        // debugging.
        match route {
            "capability_preflight" => "capability_preflight FaultingBackend case did not execute",
            "writer_guard_manifest_create" => {
                "writer_guard_manifest_create FaultingBackend case did not execute"
            }
            "manifest_publish_uncertain" => {
                "manifest_publish_uncertain FaultingBackend case did not execute"
            }
            "snapshot_orphan" => "snapshot_orphan FaultingBackend case did not execute",
            "checkpoint_truncation_debt" => {
                "checkpoint_truncation_debt FaultingBackend case did not execute"
            }
            "partial_log_strict" => "partial_log_strict FaultingBackend case did not execute",
            "partial_log_lossy" => "partial_log_lossy FaultingBackend case did not execute",
            "corrupt_log_typed" => "corrupt_log_typed FaultingBackend case did not execute",
            "replay_failed_state" => "replay_failed_state FaultingBackend case did not execute",
            "replay_visible_debt" => "replay_visible_debt FaultingBackend case did not execute",
            "flush_orphan_table" => "flush_orphan_table FaultingBackend case did not execute",
            "rewrite_preserved_reads" => {
                "rewrite_preserved_reads FaultingBackend case did not execute"
            }
            "retention_blocked_delete" => {
                "retention_blocked_delete FaultingBackend case did not execute"
            }
            "quarantine_publish_blocked_purge" => {
                "quarantine_publish_blocked_purge FaultingBackend case did not execute"
            }
            "purge_delete_debt" => "purge_delete_debt FaultingBackend case did not execute",
            "close_quiesce_timeout" => "close_quiesce_timeout FaultingBackend case did not execute",
            "close_log_sync_source" => "close_log_sync_source FaultingBackend case did not execute",
            "close_manifest_sync_debt" => {
                "close_manifest_sync_debt FaultingBackend case did not execute"
            }
            "writer_guard_release_typed" => {
                "writer_guard_release_typed FaultingBackend case did not execute"
            }
            _ => "FaultingBackend case did not execute",
        },
    )?;
    let _ = outcome;
    Ok(())
}

#[cfg(not(any(test, feature = "fault-injection")))]
#[allow(
    dead_code,
    reason = "fault-injection-disabled builds route to the no-op `let _ = service;` arms in each check_X_route; this helper exists only to keep the cfg branches symmetric"
)]
fn require_service_fault_route(
    _outcome: &ServiceFaultHarnessOutcome,
    _cases: usize,
    _route: &'static str,
) -> Result<(), TestkitError> {
    Ok(())
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
    service: &ServiceFaultHarnessOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    require_service_fault_route(
        service,
        service.capability_preflight_cases(),
        "capability_preflight",
    )?;
    #[cfg(not(any(test, feature = "fault-injection")))]
    let _ = service;
    ensure(
        scaffold.capability_preflight_cases() > 0 && scaffold.missing_capability_cases() > 0,
        "capability preflight fault route not covered",
    )?;
    outcome.capability_preflight += 1;
    Ok(())
}

fn check_writer_guard_manifest_create_route(
    scaffold: &LifecycleScaffoldOutcome,
    service: &ServiceFaultHarnessOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    require_service_fault_route(
        service,
        service.writer_guard_manifest_create_cases(),
        "writer_guard_manifest_create",
    )?;
    #[cfg(not(any(test, feature = "fault-injection")))]
    let _ = service;
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
    service: &ServiceFaultHarnessOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    require_service_fault_route(
        service,
        service.manifest_publish_uncertain_cases(),
        "manifest_publish_uncertain",
    )?;
    #[cfg(not(any(test, feature = "fault-injection")))]
    let _ = service;
    ensure(
        scaffold.durable_manifest_publish_fault_cases() > 0,
        "manifest publish uncertainty route not covered",
    )?;
    outcome.manifest_publish_uncertain += 1;
    Ok(())
}

fn check_snapshot_orphan_route(
    checkpoint: &LifecycleCheckpointContractOutcome,
    service: &ServiceFaultHarnessOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    require_service_fault_route(service, service.snapshot_orphan_cases(), "snapshot_orphan")?;
    #[cfg(not(any(test, feature = "fault-injection")))]
    let _ = service;
    ensure(
        checkpoint.partial_window_cases() > 0,
        "checkpoint partial publication window not covered",
    )?;
    outcome.snapshot_orphan += 1;
    Ok(())
}

fn check_checkpoint_truncation_debt_route(
    checkpoint: &LifecycleCheckpointContractOutcome,
    service: &ServiceFaultHarnessOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    require_service_fault_route(
        service,
        service.checkpoint_truncation_debt_cases(),
        "checkpoint_truncation_debt",
    )?;
    #[cfg(not(any(test, feature = "fault-injection")))]
    let _ = service;
    ensure(
        checkpoint.delete_failure_cases() > 0,
        "checkpoint truncation health debt not covered",
    )?;
    outcome.checkpoint_truncation_debt += 1;
    Ok(())
}

fn check_partial_log_strict_route(
    recovery: &LifecycleRecoveryContractOutcome,
    service: &ServiceFaultHarnessOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    require_service_fault_route(
        service,
        service.partial_log_strict_cases(),
        "partial_log_strict",
    )?;
    #[cfg(not(any(test, feature = "fault-injection")))]
    let _ = service;
    ensure(
        recovery.strict_failure_cases() > 0,
        "strict partial log fault route not covered",
    )?;
    outcome.partial_log_strict += 1;
    Ok(())
}

fn check_partial_log_lossy_route(
    recovery: &LifecycleRecoveryContractOutcome,
    service: &ServiceFaultHarnessOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    require_service_fault_route(
        service,
        service.partial_log_lossy_cases(),
        "partial_log_lossy",
    )?;
    #[cfg(not(any(test, feature = "fault-injection")))]
    let _ = service;
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
    service: &ServiceFaultHarnessOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    require_service_fault_route(
        service,
        service.corrupt_log_typed_cases(),
        "corrupt_log_typed",
    )?;
    #[cfg(not(any(test, feature = "fault-injection")))]
    let _ = service;
    ensure(
        recovery.strict_failure_cases() > 0 && bootstrap.replay_rejection_cases() > 0,
        "typed corrupt recovery route not covered",
    )?;
    outcome.corrupt_log_typed += 1;
    Ok(())
}

fn check_replay_failed_state_route(
    bootstrap: &LifecycleBootstrapContractOutcome,
    service: &ServiceFaultHarnessOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    require_service_fault_route(
        service,
        service.replay_failed_state_cases(),
        "replay_failed_state",
    )?;
    #[cfg(not(any(test, feature = "fault-injection")))]
    let _ = service;
    ensure(
        bootstrap.replay_rejection_cases() > 0,
        "replay failure route not covered",
    )?;
    outcome.replay_failed_state += 1;
    Ok(())
}

fn check_replay_visible_debt_route(
    bootstrap: &LifecycleBootstrapContractOutcome,
    service: &ServiceFaultHarnessOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    require_service_fault_route(
        service,
        service.replay_visible_debt_cases(),
        "replay_visible_debt",
    )?;
    #[cfg(not(any(test, feature = "fault-injection")))]
    let _ = service;
    ensure(
        bootstrap.degraded_bootstrap_cases() > 0,
        "replay visible-debt route not covered",
    )?;
    outcome.replay_visible_debt += 1;
    Ok(())
}

fn check_flush_orphan_table_route(
    flush: &LifecycleFlushContractOutcome,
    service: &ServiceFaultHarnessOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    require_service_fault_route(
        service,
        service.flush_orphan_table_cases(),
        "flush_orphan_table",
    )?;
    #[cfg(not(any(test, feature = "fault-injection")))]
    let _ = service;
    ensure(
        flush.publish_failure_cases() > 0 && flush.reopen_failure_cases() > 0,
        "flush orphan table route not covered",
    )?;
    outcome.flush_orphan_table += 1;
    Ok(())
}

fn check_rewrite_preserved_reads_route(
    rewrite: &LifecycleTableRewriteContractOutcome,
    service: &ServiceFaultHarnessOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    require_service_fault_route(
        service,
        service.rewrite_preserved_reads_cases(),
        "rewrite_preserved_reads",
    )?;
    #[cfg(not(any(test, feature = "fault-injection")))]
    let _ = service;
    ensure(
        rewrite.cache_compaction_cases() > 0 && rewrite.materialization_cases() > 0,
        "table rewrite read-preservation route not covered",
    )?;
    outcome.rewrite_preserved_reads += 1;
    Ok(())
}

fn check_retention_blocked_delete_route(
    retention: &LifecycleRetentionContractOutcome,
    service: &ServiceFaultHarnessOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    require_service_fault_route(
        service,
        service.retention_blocked_delete_cases(),
        "retention_blocked_delete",
    )?;
    #[cfg(not(any(test, feature = "fault-injection")))]
    let _ = service;
    ensure(
        retention.incomplete_proof_cases() > 0 && retention.blocked_recovery_cases() > 0,
        "retention proof did not block unsafe delete",
    )?;
    outcome.retention_blocked_delete += 1;
    Ok(())
}

fn check_quarantine_publish_blocked_purge_route(
    quarantine: &LifecycleQuarantineContractOutcome,
    service: &ServiceFaultHarnessOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    require_service_fault_route(
        service,
        service.quarantine_publish_blocked_purge_cases(),
        "quarantine_publish_blocked_purge",
    )?;
    #[cfg(not(any(test, feature = "fault-injection")))]
    let _ = service;
    ensure(
        quarantine.inventory_publish_failure_cases() > 0
            && quarantine.stale_purge_proof_cases() > 0,
        "quarantine publish/purge fault route not covered",
    )?;
    outcome.quarantine_publish_blocked_purge += 1;
    Ok(())
}

fn check_purge_delete_debt_route(
    quarantine: &LifecycleQuarantineContractOutcome,
    service: &ServiceFaultHarnessOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    require_service_fault_route(
        service,
        service.purge_delete_debt_cases(),
        "purge_delete_debt",
    )?;
    #[cfg(not(any(test, feature = "fault-injection")))]
    let _ = service;
    ensure(
        quarantine.purge_delete_failure_cases() > 0,
        "purge delete debt route not covered",
    )?;
    outcome.purge_delete_debt += 1;
    Ok(())
}

fn check_close_quiesce_timeout_route(
    close: &LifecycleCloseContractOutcome,
    service: &ServiceFaultHarnessOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    require_service_fault_route(
        service,
        service.close_quiesce_timeout_cases(),
        "close_quiesce_timeout",
    )?;
    #[cfg(not(any(test, feature = "fault-injection")))]
    let _ = service;
    ensure(
        close.commit_quiesce_blocked_cases() > 0 && close.retryable_timeout_cases() > 0,
        "close quiesce timeout route not covered",
    )?;
    outcome.close_quiesce_timeout += 1;
    Ok(())
}

fn check_close_log_sync_source_route(
    close: &LifecycleCloseContractOutcome,
    service: &ServiceFaultHarnessOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    require_service_fault_route(
        service,
        service.close_log_sync_source_cases(),
        "close_log_sync_source",
    )?;
    #[cfg(not(any(test, feature = "fault-injection")))]
    let _ = service;
    ensure(
        close.wal_sync_failure_cases() > 0 && close.source_chain_preserved_cases() > 0,
        "close log sync source route not covered",
    )?;
    outcome.close_log_sync_source += 1;
    Ok(())
}

fn check_close_manifest_sync_debt_route(
    close: &LifecycleCloseContractOutcome,
    service: &ServiceFaultHarnessOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    require_service_fault_route(
        service,
        service.close_manifest_sync_debt_cases(),
        "close_manifest_sync_debt",
    )?;
    #[cfg(not(any(test, feature = "fault-injection")))]
    let _ = service;
    ensure(
        close.manifest_sync_failure_cases() > 0,
        "close manifest sync debt route not covered",
    )?;
    outcome.close_manifest_sync_debt += 1;
    Ok(())
}

fn check_writer_guard_release_typed_route(
    close: &LifecycleCloseContractOutcome,
    service: &ServiceFaultHarnessOutcome,
    outcome: &mut LifecycleFaultContractOutcome,
) -> Result<(), TestkitError> {
    #[cfg(any(test, feature = "fault-injection"))]
    require_service_fault_route(
        service,
        service.writer_guard_release_typed_cases(),
        "writer_guard_release_typed",
    )?;
    #[cfg(not(any(test, feature = "fault-injection")))]
    let _ = service;
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
