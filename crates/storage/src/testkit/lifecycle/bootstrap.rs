//! Lifecycle bootstrap conformance helpers.

use super::super::TestkitError;
use super::recovery::{
    assemble_shell, branch_id, lossy_open_plan, open_plan, physical_key, publish_snapshot, put_row,
    testkit_error, write_database_root, write_empty_wal_segment, RecoveryScriptBackend,
    DATABASE_ID,
};
use super::{ensure, script_byte};
use crate::commit::{CommitStamp, CommitTimelineEntry, CommitTimelineRows};
use crate::format::{DatabaseManifest, WalCommitPayload, WalRecord};
use crate::lifecycle::{
    LifecycleError, LifecycleRecoveryRequest, LifecycleRecoveryRuntime, LifecycleState,
    RecoveryDegradationClass, RecoveryHealth, RecoveryStrictness,
};
use crate::row::StorageRow;
use strata_core::{BranchId, CommitVersion, Timestamp};

/// Coverage counters for commit-bootstrap contract checks.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[expect(
    clippy::struct_field_names,
    reason = "counter suffix keeps the testkit getter vocabulary explicit"
)]
pub struct LifecycleBootstrapContractOutcome {
    empty_bootstrap_cases: usize,
    checkpoint_bootstrap_cases: usize,
    wal_replay_bootstrap_cases: usize,
    degraded_bootstrap_cases: usize,
    replay_rejection_cases: usize,
    input_derived_empty_bootstrap_cases: usize,
    input_derived_checkpoint_bootstrap_cases: usize,
    input_derived_wal_replay_bootstrap_cases: usize,
    input_derived_degraded_bootstrap_cases: usize,
    input_derived_replay_rejection_cases: usize,
}

impl LifecycleBootstrapContractOutcome {
    /// Number of empty recovery packages that opened a durable runtime.
    pub const fn empty_bootstrap_cases(&self) -> usize {
        self.empty_bootstrap_cases
    }

    /// Number of checkpoint-only packages that advanced visible state.
    pub const fn checkpoint_bootstrap_cases(&self) -> usize {
        self.checkpoint_bootstrap_cases
    }

    /// Number of recovered WAL tails replayed through commit recovery.
    pub const fn wal_replay_bootstrap_cases(&self) -> usize {
        self.wal_replay_bootstrap_cases
    }

    /// Number of degraded recovery packages accepted into an open runtime.
    pub const fn degraded_bootstrap_cases(&self) -> usize {
        self.degraded_bootstrap_cases
    }

    /// Number of malformed replay packages rejected before opening.
    pub const fn replay_rejection_cases(&self) -> usize {
        self.replay_rejection_cases
    }

    /// Number of script-derived empty bootstrap cases.
    pub const fn input_derived_empty_bootstrap_cases(&self) -> usize {
        self.input_derived_empty_bootstrap_cases
    }

    /// Number of script-derived checkpoint bootstrap cases.
    pub const fn input_derived_checkpoint_bootstrap_cases(&self) -> usize {
        self.input_derived_checkpoint_bootstrap_cases
    }

    /// Number of script-derived WAL replay bootstrap cases.
    pub const fn input_derived_wal_replay_bootstrap_cases(&self) -> usize {
        self.input_derived_wal_replay_bootstrap_cases
    }

    /// Number of script-derived degraded bootstrap cases.
    pub const fn input_derived_degraded_bootstrap_cases(&self) -> usize {
        self.input_derived_degraded_bootstrap_cases
    }

    /// Number of script-derived malformed replay rejection cases.
    pub const fn input_derived_replay_rejection_cases(&self) -> usize {
        self.input_derived_replay_rejection_cases
    }
}

/// Exercises bootstrap behavior through the recovered durable runtime.
pub fn check_lifecycle_bootstrap_contract(
    script: &[u8],
) -> Result<LifecycleBootstrapContractOutcome, TestkitError> {
    let mut outcome = LifecycleBootstrapContractOutcome::default();
    check_empty_bootstrap(script, &mut outcome)?;
    check_checkpoint_bootstrap(script, &mut outcome)?;
    check_wal_replay_bootstrap(script, &mut outcome)?;
    check_degraded_bootstrap(script, &mut outcome)?;
    check_replay_rejection_bootstrap(script, &mut outcome)?;
    check_input_derived_bootstrap(script, &mut outcome)?;
    Ok(outcome)
}

fn check_empty_bootstrap(
    script: &[u8],
    outcome: &mut LifecycleBootstrapContractOutcome,
) -> Result<(), TestkitError> {
    let backend: &'static RecoveryScriptBackend =
        crate::testkit::leak_static(RecoveryScriptBackend::new());
    let branch = branch_id(script_byte(script, 13));
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, backend)?;
    let request = LifecycleRecoveryRequest::from_open_plan(shell.open_plan())
        .map_err(|error| testkit_error(&error))?;
    let recovered = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .map_err(|error| testkit_error(&error))?;
    let runtime = shell
        .complete_recovery(&recovered)
        .map_err(|error| testkit_error(&error))?;

    ensure(
        runtime.state() == LifecycleState::Open,
        "bootstrap did not open",
    )?;
    ensure(
        runtime.visible_version() == CommitVersion::ZERO,
        "empty bootstrap advanced visible version",
    )?;
    ensure(
        runtime.bootstrap_report().records_seen() == 0,
        "empty bootstrap replayed records",
    )?;
    ensure(
        runtime.open_outcome().bootstrap().is_some(),
        "empty bootstrap did not publish bootstrap facts",
    )?;
    runtime.read_view().map_err(|error| testkit_error(&error))?;
    outcome.empty_bootstrap_cases += 1;
    Ok(())
}

fn check_checkpoint_bootstrap(
    script: &[u8],
    outcome: &mut LifecycleBootstrapContractOutcome,
) -> Result<(), TestkitError> {
    let backend: &'static RecoveryScriptBackend =
        crate::testkit::leak_static(RecoveryScriptBackend::new());
    let branch = branch_id(script_byte(script, 14));
    let checkpoint_version = CommitVersion::new(2);
    let checkpoint_row = put_row(
        branch,
        checkpoint_version,
        b"bootstrap-checkpoint",
        b"value",
    );
    publish_snapshot(
        backend,
        30,
        checkpoint_version,
        std::slice::from_ref(&checkpoint_row),
    )?;
    write_database_root(
        backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .map_err(testkit_error)?
            .with_recovery_facts(1, Some(checkpoint_version.as_u64()), Some(30), None)
            .map_err(testkit_error)?,
    )?;
    // #2765: a checkpoint-attesting manifest requires its WAL chain on disk
    // under strict recovery — seed the attested active segment.
    write_empty_wal_segment(backend, 1)?;
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, backend)?;
    let request = LifecycleRecoveryRequest::from_open_plan(shell.open_plan())
        .map_err(|error| testkit_error(&error))?;
    let recovered = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .map_err(|error| testkit_error(&error))?;
    let runtime = shell
        .complete_recovery(&recovered)
        .map_err(|error| testkit_error(&error))?;

    ensure(
        runtime.visible_version() == checkpoint_version,
        "checkpoint bootstrap did not publish recovered visibility",
    )?;
    ensure(
        runtime
            .bootstrap_report()
            .checkpoint_visible_publish()
            .is_some(),
        "checkpoint bootstrap did not report visibility catch-up",
    )?;
    ensure(
        runtime.allocator().version_allocator().last_allocated() == checkpoint_version,
        "checkpoint bootstrap did not catch up version allocator",
    )?;
    let visible = runtime
        .read_view()
        .map_err(|error| testkit_error(&error))?
        .latest(checkpoint_row.physical_key())
        .map_err(testkit_error)?
        .ok_or_else(|| TestkitError::new("checkpoint row missing after bootstrap"))?;
    ensure(
        visible.row() == &checkpoint_row,
        "checkpoint bootstrap returned wrong visible row",
    )?;
    outcome.checkpoint_bootstrap_cases += 1;
    Ok(())
}

fn check_wal_replay_bootstrap(
    script: &[u8],
    outcome: &mut LifecycleBootstrapContractOutcome,
) -> Result<(), TestkitError> {
    let backend: &'static RecoveryScriptBackend =
        crate::testkit::leak_static(RecoveryScriptBackend::new());
    let branch = branch_id(script_byte(script, 15));
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, backend)?;
    let record = replayable_wal_record(branch, CommitVersion::new(3), b"bootstrap-tail", b"value")?;
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .map_err(testkit_error)?;
    // Recovery reads the on-disk log, not the writer's coalescing buffer —
    // an unflushed append is legally invisible (BS5 write-group contract),
    // so the crash-tail scenario forces its record durable first.
    shell
        .services_mut()
        .wal_mut()
        .force_durable()
        .map_err(testkit_error)?;
    let request = LifecycleRecoveryRequest::from_open_plan(shell.open_plan())
        .map_err(|error| testkit_error(&error))?;
    let recovered = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .map_err(|error| testkit_error(&error))?;
    let runtime = shell
        .complete_recovery(&recovered)
        .map_err(|error| testkit_error(&error))?;

    if runtime.visible_version() != record.commit_version() {
        return Err(TestkitError::new(format!(
            "WAL bootstrap did not publish replayed visibility: visible {:?} vs record {:?}, seen {} applied {}",
            runtime.visible_version(),
            record.commit_version(),
            runtime.bootstrap_report().records_seen(),
            runtime.bootstrap_report().records_applied(),
        )));
    }
    ensure(
        runtime.bootstrap_report().records_seen() == 1
            && runtime.bootstrap_report().records_applied() == 1,
        "WAL bootstrap did not replay through commit recovery",
    )?;
    ensure(
        runtime.bootstrap_report().rows_applied() == record.commit_payload().rows().len(),
        "WAL bootstrap row counters did not match payload",
    )?;
    let key = physical_key(branch, b"bootstrap-tail")?;
    let visible = runtime
        .read_view()
        .map_err(|error| testkit_error(&error))?
        .latest(&key)
        .map_err(testkit_error)?
        .ok_or_else(|| TestkitError::new("WAL row missing after bootstrap"))?;
    ensure(
        visible.row().value() == b"value",
        "WAL bootstrap returned wrong visible value",
    )?;
    outcome.wal_replay_bootstrap_cases += 1;
    Ok(())
}

fn check_degraded_bootstrap(
    script: &[u8],
    outcome: &mut LifecycleBootstrapContractOutcome,
) -> Result<(), TestkitError> {
    let backend: &'static RecoveryScriptBackend =
        crate::testkit::leak_static(RecoveryScriptBackend::new());
    let branch = branch_id(script_byte(script, 16));
    write_database_root(
        backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .map_err(testkit_error)?
            .with_recovery_facts(1, Some(7), Some(77), None)
            .map_err(testkit_error)?,
    )?;
    let mut shell = assemble_shell(lossy_open_plan(), branch, backend)?;
    let record = replayable_wal_record(
        branch,
        CommitVersion::new(4),
        b"degraded-bootstrap",
        b"value",
    )?;
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .map_err(testkit_error)?;
    // Recovery reads the on-disk log, not the writer's coalescing buffer —
    // force the record durable so the lossy path has a tail to replay.
    shell
        .services_mut()
        .wal_mut()
        .force_durable()
        .map_err(testkit_error)?;
    let request = LifecycleRecoveryRequest::from_open_plan(shell.open_plan())
        .map_err(|error| testkit_error(&error))?;
    let recovered = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .map_err(|error| testkit_error(&error))?;
    ensure(
        matches!(
            recovered.health(),
            RecoveryHealth::Degraded {
                class: RecoveryDegradationClass::DataLoss,
                ..
            }
        ),
        "degraded bootstrap fixture did not recover with data-loss health",
    )?;
    let runtime = shell
        .complete_recovery(&recovered)
        .map_err(|error| testkit_error(&error))?;

    ensure(
        runtime.bootstrap_report().recovery_health() == runtime.open_outcome().recovery_health(),
        "bootstrap health not propagated to open outcome",
    )?;
    ensure(
        !runtime.open_outcome().recovery_health().is_healthy(),
        "degraded bootstrap opened as healthy",
    )?;
    ensure(
        runtime.visible_version() == record.commit_version(),
        "degraded bootstrap did not replay WAL tail",
    )?;
    outcome.degraded_bootstrap_cases += 1;
    Ok(())
}

fn check_replay_rejection_bootstrap(
    script: &[u8],
    outcome: &mut LifecycleBootstrapContractOutcome,
) -> Result<(), TestkitError> {
    let backend: &'static RecoveryScriptBackend =
        crate::testkit::leak_static(RecoveryScriptBackend::new());
    let branch = branch_id(script_byte(script, 17));
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, backend)?;
    let record = timeline_only_replay_record(branch, CommitVersion::new(5))?;
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .map_err(testkit_error)?;
    // Recovery reads the on-disk log, not the writer's coalescing buffer —
    // the malformed record must be durable for replay to reject it.
    shell
        .services_mut()
        .wal_mut()
        .force_durable()
        .map_err(testkit_error)?;
    let request = LifecycleRecoveryRequest::from_open_plan(shell.open_plan())
        .map_err(|error| testkit_error(&error))?;
    let recovered = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .map_err(|error| testkit_error(&error))?;
    let error = shell
        .complete_recovery(&recovered)
        .expect_err("timeline-only replay rejects before open");

    ensure(
        matches!(error, LifecycleError::LowerLayer { .. }),
        "timeline-only bootstrap did not fail through commit runtime",
    )?;
    outcome.replay_rejection_cases += 1;
    Ok(())
}

fn check_input_derived_bootstrap(
    script: &[u8],
    outcome: &mut LifecycleBootstrapContractOutcome,
) -> Result<(), TestkitError> {
    check_input_derived_empty_bootstrap(script, outcome)?;
    check_input_derived_checkpoint_bootstrap(script, outcome)?;
    check_input_derived_wal_replay_bootstrap(script, outcome)?;
    check_input_derived_degraded_bootstrap(script, outcome)?;
    check_input_derived_replay_rejection_bootstrap(script, outcome)?;
    Ok(())
}

fn check_input_derived_empty_bootstrap(
    script: &[u8],
    outcome: &mut LifecycleBootstrapContractOutcome,
) -> Result<(), TestkitError> {
    check_empty_bootstrap(&script[0..script.len().min(1)], outcome)?;
    outcome.input_derived_empty_bootstrap_cases += 1;
    Ok(())
}

fn check_input_derived_checkpoint_bootstrap(
    script: &[u8],
    outcome: &mut LifecycleBootstrapContractOutcome,
) -> Result<(), TestkitError> {
    let backend: &'static RecoveryScriptBackend =
        crate::testkit::leak_static(RecoveryScriptBackend::new());
    let branch = branch_id(script_byte(script, 18));
    let version = CommitVersion::new(1 + u64::from(script_byte(script, 19) % 8));
    let snapshot_id = 100 + u64::from(script_byte(script, 20));
    let row = put_row(branch, version, b"generated-bootstrap-checkpoint", b"value");
    publish_snapshot(backend, snapshot_id, version, std::slice::from_ref(&row))?;
    write_database_root(
        backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .map_err(testkit_error)?
            .with_recovery_facts(1, Some(version.as_u64()), Some(snapshot_id), None)
            .map_err(testkit_error)?,
    )?;
    // #2765: a checkpoint-attesting manifest requires its WAL chain on disk
    // under strict recovery — seed the attested active segment.
    write_empty_wal_segment(backend, 1)?;
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, backend)?;
    let request = LifecycleRecoveryRequest::from_open_plan(shell.open_plan())
        .map_err(|error| testkit_error(&error))?;
    let recovered = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .map_err(|error| testkit_error(&error))?;
    let runtime = shell
        .complete_recovery(&recovered)
        .map_err(|error| testkit_error(&error))?;
    ensure(
        runtime.visible_version() == version,
        "input-derived checkpoint bootstrap visible mismatch",
    )?;
    outcome.input_derived_checkpoint_bootstrap_cases += 1;
    Ok(())
}

fn check_input_derived_wal_replay_bootstrap(
    script: &[u8],
    outcome: &mut LifecycleBootstrapContractOutcome,
) -> Result<(), TestkitError> {
    let backend: &'static RecoveryScriptBackend =
        crate::testkit::leak_static(RecoveryScriptBackend::new());
    let branch = branch_id(script_byte(script, 21));
    let version = CommitVersion::new(1 + u64::from(script_byte(script, 22) % 8));
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, backend)?;
    let record = replayable_wal_record(branch, version, b"generated-bootstrap-tail", b"value")?;
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .map_err(testkit_error)?;
    // Recovery reads the on-disk log, not the writer's coalescing buffer.
    shell
        .services_mut()
        .wal_mut()
        .force_durable()
        .map_err(testkit_error)?;
    let request = LifecycleRecoveryRequest::from_open_plan(shell.open_plan())
        .map_err(|error| testkit_error(&error))?;
    let recovered = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .map_err(|error| testkit_error(&error))?;
    let runtime = shell
        .complete_recovery(&recovered)
        .map_err(|error| testkit_error(&error))?;
    ensure(
        runtime.bootstrap_report().records_applied() == 1,
        "input-derived WAL bootstrap did not apply one record",
    )?;
    outcome.input_derived_wal_replay_bootstrap_cases += 1;
    Ok(())
}

fn check_input_derived_degraded_bootstrap(
    script: &[u8],
    outcome: &mut LifecycleBootstrapContractOutcome,
) -> Result<(), TestkitError> {
    let backend: &'static RecoveryScriptBackend =
        crate::testkit::leak_static(RecoveryScriptBackend::new());
    let branch = branch_id(script_byte(script, 23));
    write_database_root(
        backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .map_err(testkit_error)?
            .with_recovery_facts(1, Some(8), Some(108), None)
            .map_err(testkit_error)?,
    )?;
    let mut shell = assemble_shell(lossy_open_plan(), branch, backend)?;
    let version = CommitVersion::new(1 + u64::from(script_byte(script, 24) % 8));
    let record = replayable_wal_record(branch, version, b"generated-degraded-tail", b"value")?;
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .map_err(testkit_error)?;
    // Recovery reads the on-disk log, not the writer's coalescing buffer.
    shell
        .services_mut()
        .wal_mut()
        .force_durable()
        .map_err(testkit_error)?;
    let request = LifecycleRecoveryRequest::from_open_plan(shell.open_plan())
        .map_err(|error| testkit_error(&error))?;
    let recovered = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .map_err(|error| testkit_error(&error))?;
    let runtime = shell
        .complete_recovery(&recovered)
        .map_err(|error| testkit_error(&error))?;
    ensure(
        !runtime.open_outcome().recovery_health().is_healthy(),
        "input-derived degraded bootstrap opened healthy",
    )?;
    outcome.input_derived_degraded_bootstrap_cases += 1;
    Ok(())
}

fn check_input_derived_replay_rejection_bootstrap(
    script: &[u8],
    outcome: &mut LifecycleBootstrapContractOutcome,
) -> Result<(), TestkitError> {
    let backend: &'static RecoveryScriptBackend =
        crate::testkit::leak_static(RecoveryScriptBackend::new());
    let branch = branch_id(script_byte(script, 25));
    let version = CommitVersion::new(1 + u64::from(script_byte(script, 26) % 8));
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, backend)?;
    let record = timeline_only_replay_record(branch, version)?;
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .map_err(testkit_error)?;
    // Recovery reads the on-disk log, not the writer's coalescing buffer.
    shell
        .services_mut()
        .wal_mut()
        .force_durable()
        .map_err(testkit_error)?;
    let request = LifecycleRecoveryRequest::from_open_plan(shell.open_plan())
        .map_err(|error| testkit_error(&error))?;
    let recovered = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .map_err(|error| testkit_error(&error))?;
    shell
        .complete_recovery(&recovered)
        .expect_err("input-derived malformed replay rejects");
    outcome.input_derived_replay_rejection_cases += 1;
    Ok(())
}

fn replayable_wal_record(
    branch: BranchId,
    version: CommitVersion,
    user_key: &'static [u8],
    value: &'static [u8],
) -> Result<WalRecord, TestkitError> {
    let timestamp = Timestamp::from_micros(version.as_u64() * 100);
    let stamp = CommitStamp::new(branch, version, timestamp).map_err(testkit_error)?;
    let row = StorageRow::put(
        physical_key(branch, user_key)?,
        version,
        timestamp,
        Timestamp::EPOCH,
        value.to_vec(),
    );
    let timeline_rows = CommitTimelineRows::from_entry(
        CommitTimelineEntry::from_stamp(stamp).map_err(testkit_error)?,
    )
    .map_err(testkit_error)?
    .into_rows();
    let payload = WalCommitPayload::new(vec![
        row,
        timeline_rows[0].clone(),
        timeline_rows[1].clone(),
    ])
    .map_err(testkit_error)?;
    WalRecord::new(version, branch, timestamp, payload).map_err(testkit_error)
}

fn timeline_only_replay_record(
    branch: BranchId,
    version: CommitVersion,
) -> Result<WalRecord, TestkitError> {
    let timestamp = Timestamp::from_micros(version.as_u64() * 100);
    let stamp = CommitStamp::new(branch, version, timestamp).map_err(testkit_error)?;
    let timeline_rows = CommitTimelineRows::from_entry(
        CommitTimelineEntry::from_stamp(stamp).map_err(testkit_error)?,
    )
    .map_err(testkit_error)?
    .into_rows();
    let payload = WalCommitPayload::new(timeline_rows.to_vec()).map_err(testkit_error)?;
    WalRecord::new(version, branch, timestamp, payload).map_err(testkit_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Default-lane pin (#2902): the bootstrap contract's scenarios are
    /// otherwise reachable only through feature-gated targets, so the
    /// mutation gate's run has kill coverage here — every scenario must
    /// execute and count.
    #[test]
    fn bootstrap_contract_holds_with_all_scenarios_counted() {
        let outcome = check_lifecycle_bootstrap_contract(b"lifecycle-default-lane-pin")
            .expect("bootstrap contract holds");
        // check_input_derived_bootstrap reruns the empty scenario, so the
        // empty counter legitimately lands at 2.
        assert_eq!(outcome.empty_bootstrap_cases(), 2);
        assert_eq!(outcome.checkpoint_bootstrap_cases(), 1);
        assert_eq!(outcome.wal_replay_bootstrap_cases(), 1);
        assert_eq!(outcome.degraded_bootstrap_cases(), 1);
        assert_eq!(outcome.replay_rejection_cases(), 1);
        assert_eq!(outcome.input_derived_empty_bootstrap_cases(), 1);
        assert_eq!(outcome.input_derived_checkpoint_bootstrap_cases(), 1);
        assert_eq!(outcome.input_derived_wal_replay_bootstrap_cases(), 1);
        assert_eq!(outcome.input_derived_degraded_bootstrap_cases(), 1);
        assert_eq!(outcome.input_derived_replay_rejection_cases(), 1);
    }
}
