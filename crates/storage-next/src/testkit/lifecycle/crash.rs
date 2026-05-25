//! Lifecycle crash-window assurance helpers.

use super::{
    check_lifecycle_bootstrap_contract, check_lifecycle_checkpoint_contract,
    check_lifecycle_close_contract, check_lifecycle_flush_contract,
    check_lifecycle_quarantine_contract, ensure,
};
use crate::testkit::TestkitError;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LifecycleCrashContractOutcome {
    log_append_replay: usize,
    unresolved_gate_reconcile: usize,
    orphan_snapshot_ignored: usize,
    checkpoint_tail_recovered: usize,
    orphan_table_reported: usize,
    quarantine_inventory_debt: usize,
    object_quarantine_preserved: usize,
    close_reopen_consistent: usize,
    ignored_case_equivalents: usize,
    harness_environment: usize,
}

pub fn check_lifecycle_crash_contract(
    script: &[u8],
) -> Result<LifecycleCrashContractOutcome, TestkitError> {
    let mut outcome = LifecycleCrashContractOutcome::default();
    match CrashRoute::from_script(script) {
        CrashRoute::LogAppendReplay => {
            let bootstrap = check_lifecycle_bootstrap_contract(script)?;
            ensure(
                bootstrap.wal_replay_bootstrap_cases() > 0,
                "log append replay crash route not covered",
            )?;
            outcome.log_append_replay += 1;
        }
        CrashRoute::UnresolvedGate => {
            let bootstrap = check_lifecycle_bootstrap_contract(script)?;
            ensure(
                bootstrap.replay_rejection_cases() > 0,
                "unresolved gate crash route not covered",
            )?;
            outcome.unresolved_gate_reconcile += 1;
        }
        CrashRoute::OrphanSnapshot => {
            let checkpoint = check_lifecycle_checkpoint_contract(script)?;
            ensure(
                checkpoint.partial_window_cases() > 0,
                "orphan snapshot crash route not covered",
            )?;
            outcome.orphan_snapshot_ignored += 1;
        }
        CrashRoute::CheckpointTail => {
            let checkpoint = check_lifecycle_checkpoint_contract(script)?;
            ensure(
                checkpoint.checkpoint_truncation_round_trip_cases() > 0,
                "checkpoint tail crash route not covered",
            )?;
            outcome.checkpoint_tail_recovered += 1;
        }
        CrashRoute::OrphanTable => {
            let flush = check_lifecycle_flush_contract(script)?;
            ensure(
                flush.publish_failure_cases() > 0,
                "orphan table crash route not covered",
            )?;
            outcome.orphan_table_reported += 1;
        }
        CrashRoute::QuarantineInventory => {
            let quarantine = check_lifecycle_quarantine_contract(script)?;
            ensure(
                quarantine.inventory_publish_failure_cases() > 0,
                "quarantine inventory crash route not covered",
            )?;
            outcome.quarantine_inventory_debt += 1;
        }
        CrashRoute::ObjectQuarantine => {
            let quarantine = check_lifecycle_quarantine_contract(script)?;
            ensure(
                quarantine.already_quarantined_cases() > 0,
                "object quarantine crash route not covered",
            )?;
            outcome.object_quarantine_preserved += 1;
        }
        CrashRoute::CloseReopen => {
            let close = check_lifecycle_close_contract(script)?;
            ensure(
                close.durable_close_completed_cases() > 0
                    && close.guard_release_observed_cases() > 0,
                "close reopen crash route not covered",
            )?;
            outcome.close_reopen_consistent += 1;
        }
        CrashRoute::NonignoredEquivalent => {
            outcome.ignored_case_equivalents += 1;
        }
    }
    outcome.harness_environment += 1;
    Ok(outcome)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CrashRoute {
    LogAppendReplay,
    UnresolvedGate,
    OrphanSnapshot,
    CheckpointTail,
    OrphanTable,
    QuarantineInventory,
    ObjectQuarantine,
    CloseReopen,
    NonignoredEquivalent,
}

impl CrashRoute {
    fn from_script(script: &[u8]) -> Self {
        let lower = String::from_utf8_lossy(script).to_ascii_lowercase();
        if lower.contains("wal-append") || lower.contains("log-append") {
            return Self::LogAppendReplay;
        }
        if lower.contains("unresolved-gate") {
            return Self::UnresolvedGate;
        }
        if lower.contains("orphan-snapshot") || lower.contains("snapshot-orphan") {
            return Self::OrphanSnapshot;
        }
        if lower.contains("checkpoint-tail") {
            return Self::CheckpointTail;
        }
        if lower.contains("orphan-table") || lower.contains("table-orphan") {
            return Self::OrphanTable;
        }
        if lower.contains("quarantine-inventory") {
            return Self::QuarantineInventory;
        }
        if lower.contains("object-quarantine") {
            return Self::ObjectQuarantine;
        }
        if lower.contains("close-reopen") {
            return Self::CloseReopen;
        }
        if lower.contains("nonignored-equivalent") {
            return Self::NonignoredEquivalent;
        }
        match script.first().copied().unwrap_or(0) % 9 {
            0 => Self::LogAppendReplay,
            1 => Self::UnresolvedGate,
            2 => Self::OrphanSnapshot,
            3 => Self::CheckpointTail,
            4 => Self::OrphanTable,
            5 => Self::QuarantineInventory,
            6 => Self::ObjectQuarantine,
            7 => Self::CloseReopen,
            _ => Self::NonignoredEquivalent,
        }
    }
}

impl LifecycleCrashContractOutcome {
    pub const fn log_append_replay_cases(&self) -> usize {
        self.log_append_replay
    }

    pub const fn unresolved_gate_reconcile_cases(&self) -> usize {
        self.unresolved_gate_reconcile
    }

    pub const fn orphan_snapshot_ignored_cases(&self) -> usize {
        self.orphan_snapshot_ignored
    }

    pub const fn checkpoint_tail_recovered_cases(&self) -> usize {
        self.checkpoint_tail_recovered
    }

    pub const fn orphan_table_reported_cases(&self) -> usize {
        self.orphan_table_reported
    }

    pub const fn quarantine_inventory_debt_cases(&self) -> usize {
        self.quarantine_inventory_debt
    }

    pub const fn object_quarantine_preserved_cases(&self) -> usize {
        self.object_quarantine_preserved
    }

    pub const fn close_reopen_consistent_cases(&self) -> usize {
        self.close_reopen_consistent
    }

    pub const fn ignored_case_equivalent_cases(&self) -> usize {
        self.ignored_case_equivalents
    }

    pub const fn harness_environment_cases(&self) -> usize {
        self.harness_environment
    }
}
