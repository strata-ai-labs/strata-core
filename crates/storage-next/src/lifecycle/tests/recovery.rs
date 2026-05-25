use super::*;
use crate::backend::{
    Backend, BackendAppend, BackendCapabilities, BackendError, BackendErrorKind, BackendMetadata,
    BackendRange, BackendResult, BackendWriterGuard, PublishDurability, PublishError, PublishMode,
    PublishOutcome, PublishResult, DURABLE_LOCAL_MODE_REQUIREMENTS,
};
use crate::branch::BranchRuntimeConfig;
use crate::commit::{
    CommitBatch, CommitBatchOptions, CommitBranchGeneration, CommitBranchGenerationGuard,
    CommitConflictValidationMode, CommitDuplicateKeyPolicy, CommitDurabilityClass,
    CommitDurabilityMode, CommitExpiry, CommitManualTimestampSource, CommitMutation, CommitOrigin,
    CommitRetentionHint, CommitRuntimeConfig, CommitRuntimeError, CommitStamp, CommitTimelineEntry,
    CommitTimelineRows, CommitTimestampPolicy, CommitUnresolvedDurable, CommitValidationFacts,
};
use crate::format::{encode_manifest, DatabaseManifest, WalCommitPayload, WalRecord};
use crate::layout::ObjectLayout;
use crate::object::{ObjectName, ObjectPrefix};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::service::{
    SnapshotPublishRequest, SnapshotService, TableObjectService, WalServiceConfig,
};
use crate::table::{ImmutableTableBuilder, TableBuilderConfig, TableIdentity, TableRow};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

const DATABASE_ID: [u8; 16] = [0x9a; 16];

#[test]
fn recovery_empty_database_returns_healthy_package_without_replay() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x31);
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("recovery outcome");

    assert_eq!(outcome.health(), &RecoveryHealth::Healthy);
    assert_eq!(outcome.checkpoint().snapshot_id(), None);
    assert_eq!(outcome.checkpoint().trusted_watermark(), None);
    assert_eq!(outcome.checkpoint().section_count(), 0);
    assert_eq!(outcome.checkpoint().row_count(), 0);
    assert!(outcome.checkpoint().install_outcome().is_none());
    assert_eq!(outcome.wal().replay_start(), CommitVersion::ZERO);
    assert!(outcome.wal().records().is_empty());
    assert!(outcome.wal().truncation().is_none());
    assert!(outcome.wal().repair().is_none());
    assert!(outcome.quarantine().object().is_some());
    assert!(!outcome.quarantine().is_present());
    assert_eq!(outcome.quarantine().byte_count(), 0);
    assert_eq!(outcome.quarantine().entry_count(), 0);
    assert_eq!(outcome.tables().validated_count(), 0);
    assert_eq!(shell.state(), LifecycleState::Recovering);
    assert_eq!(shell.visible_version(), CommitVersion::ZERO);
    assert!(shell.branch_state().is_empty());
    assert!(shell.admit_ordinary_read().is_err());
    assert!(shell.admit_commit().is_err());
}

#[test]
fn bootstrap_empty_recovery_opens_durable_runtime_with_zero_visibility() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x44);
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("recovery outcome");

    let mut runtime = shell
        .complete_recovery(&outcome)
        .expect("bootstrap recovery");

    assert_eq!(runtime.state(), LifecycleState::Open);
    assert_eq!(
        runtime.open_plan().storage_mode(),
        StorageMode::DurableLocalStandard
    );
    assert_eq!(
        runtime.open_outcome().mode(),
        StorageMode::DurableLocalStandard
    );
    assert_eq!(
        runtime.open_outcome().disposition(),
        StorageOpenDisposition::Created
    );
    assert_eq!(
        runtime.open_outcome().recovered_visible_version(),
        Some(CommitVersion::ZERO)
    );
    assert!(runtime.open_outcome().recovery_health().is_healthy());
    assert!(runtime.open_outcome().maintenance_ready());
    assert_eq!(runtime.visible_version(), CommitVersion::ZERO);
    assert_eq!(runtime.bootstrap_report().records_seen(), 0);
    assert_eq!(
        runtime.allocator().version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert!(runtime.branch_state().is_empty());
    assert_eq!(runtime.services().wal().active_segment_id(), 1);
    assert_eq!(runtime.unresolved_durable().expect("gate"), None);
    assert_eq!(runtime.bootstrap_report().gates_cleared(), 0);

    let commit_outcome = runtime
        .execute_durable_commit(
            durable_standard_batch(branch, b"post-open", b"value"),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("post-open durable commit");
    assert_eq!(commit_outcome.commit_version(), Some(CommitVersion::new(1)));
    assert_eq!(runtime.visible_version(), CommitVersion::new(1));
}

#[test]
fn bootstrap_runtime_can_enqueue_and_run_health_collection_maintenance() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x5f);
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("recovery outcome");
    let mut runtime = shell
        .complete_recovery(&outcome)
        .expect("bootstrap recovery");

    let enqueue = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::health_collection())
        .expect("enqueue health collection");
    assert_eq!(enqueue.status(), MaintenanceEnqueueStatus::Enqueued);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);

    let mut runner = MaintenanceTestRunner;
    let maintenance = runtime
        .run_next_maintenance(&mut runner)
        .expect("run maintenance")
        .expect("maintenance outcome");

    assert_eq!(maintenance.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(
        maintenance.task_kind(),
        MaintenanceTaskKind::HealthCollection
    );
    assert!(maintenance.task_id().is_some());
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    assert_eq!(runtime.maintenance_status().stats().completed(), 1);
}

#[test]
fn bootstrap_checkpoint_only_recovery_publishes_visible_and_catches_allocator() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x45);
    let checkpoint_row = put_row(branch, 2, b"checkpoint-bootstrap", b"checkpoint-value");
    publish_snapshot(
        &backend,
        8,
        CommitVersion::new(3),
        std::slice::from_ref(&checkpoint_row),
    );
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(3), Some(8), None)
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("checkpoint recovery outcome");
    assert_eq!(shell.visible_version(), CommitVersion::ZERO);

    let runtime = shell
        .complete_recovery(&outcome)
        .expect("checkpoint bootstrap");

    assert_eq!(runtime.state(), LifecycleState::Open);
    assert_eq!(runtime.visible_version(), CommitVersion::new(3));
    assert_eq!(
        runtime.open_outcome().recovered_visible_version(),
        Some(CommitVersion::new(3))
    );
    assert_eq!(runtime.bootstrap_report().records_seen(), 0);
    assert!(runtime
        .bootstrap_report()
        .checkpoint_visible_publish()
        .is_some());
    assert_eq!(
        runtime.allocator().version_allocator().last_allocated(),
        CommitVersion::new(3)
    );
    assert_eq!(
        runtime.allocator().timestamp_guard().last_allocated(),
        Some(Timestamp::from_micros(200))
    );
    assert_eq!(runtime.open_outcome().database_id(), Some(&DATABASE_ID));
    assert_eq!(runtime.open_outcome().codec_id(), Some("identity"));
    assert!(runtime.open_outcome().checkpoint().is_some());
    assert!(runtime.open_outcome().wal().is_some());
    assert!(runtime.open_outcome().tables().is_some());
    assert!(runtime.open_outcome().quarantine().is_some());
    assert!(runtime.open_outcome().bootstrap().is_some());
    assert_eq!(runtime.open_outcome().stats().open_attempts(), 1);
    let read_view = runtime.read_view().expect("open read view");
    let visible = read_view
        .latest(checkpoint_row.physical_key())
        .expect("latest")
        .expect("checkpoint row visible after bootstrap");
    assert_eq!(visible.row(), &checkpoint_row);
}

#[test]
fn bootstrap_replays_wal_tail_through_commit_runtime() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x46);
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let record = wal_record(branch, 4, b"bootstrap-tail", b"tail-value");
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .expect("append WAL record");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("WAL recovery outcome");

    let runtime = shell.complete_recovery(&outcome).expect("WAL bootstrap");

    assert_eq!(runtime.state(), LifecycleState::Open);
    assert_eq!(runtime.visible_version(), CommitVersion::new(4));
    assert_eq!(runtime.bootstrap_report().records_seen(), 1);
    assert_eq!(runtime.bootstrap_report().records_applied(), 1);
    assert_eq!(runtime.bootstrap_report().records_already_applied(), 0);
    assert_eq!(runtime.bootstrap_report().rows_checked(), 3);
    assert_eq!(runtime.bootstrap_report().rows_applied(), 3);
    assert_eq!(
        runtime.allocator().version_allocator().last_allocated(),
        CommitVersion::new(4)
    );
    assert_eq!(
        runtime.allocator().timestamp_guard().last_allocated(),
        Some(Timestamp::from_micros(400))
    );
    let read_view = runtime.read_view().expect("open read view");
    let visible = read_view
        .latest(&physical_key(branch, b"bootstrap-tail"))
        .expect("latest")
        .expect("replayed row visible");
    assert_eq!(visible.row().value(), b"tail-value");
}

#[test]
fn bootstrap_rejects_timeline_only_wal_payload_before_open() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x47);
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let record = timeline_only_wal_record(branch, 5);
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .expect("append timeline-only WAL record");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("WAL recovery outcome");

    let error = shell
        .complete_recovery(&outcome)
        .expect_err("timeline-only replay rejects");

    assert_commit_runtime_source(
        &error,
        &CommitRuntimeError::InvalidCommitState {
            reason: "replay payload is missing user mutation rows",
        },
    );
}

#[test]
fn bootstrap_rejects_log_record_without_timeline_rows_before_open() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x48);
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let record = user_only_wal_record(branch, 5, b"missing-timeline", b"value");
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .expect("append user-only WAL record");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("WAL recovery outcome");

    let error = shell
        .complete_recovery(&outcome)
        .expect_err("missing timeline replay rejects");

    assert_eq!(
        error,
        LifecycleError::TimelineRecoveryMismatch {
            reason: "replay payload is missing commit timeline rows",
        },
    );
}

#[test]
fn bootstrap_rejects_recovered_log_record_for_unopened_branch() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x49);
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let record = wal_record(branch_id(0x4a), 3, b"foreign-branch", b"value");
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .expect("append foreign branch record");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("recovery outcome");

    assert_eq!(outcome.wal().records(), std::slice::from_ref(&record));
    assert_eq!(
        shell
            .complete_recovery(&outcome)
            .expect_err("foreign branch recovered record rejects"),
        LifecycleError::RecoveryFailed {
            reason: "recovered WAL package contains an unopened branch",
        }
    );
}

#[test]
fn bootstrap_rejects_recovered_log_records_not_strictly_ordered() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x4b);
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let newer = wal_record(branch, 5, b"newer", b"value");
    let older = wal_record(branch, 4, b"older", b"value");
    shell
        .services_mut()
        .wal_mut()
        .append(&newer)
        .expect("append newer record");
    shell
        .services_mut()
        .wal_mut()
        .append(&older)
        .expect("append older record");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("recovery outcome");

    assert_eq!(outcome.wal().records(), &[newer, older]);
    assert_eq!(
        shell
            .complete_recovery(&outcome)
            .expect_err("non-increasing recovered record package rejects"),
        LifecycleError::RecoveryFailed {
            reason: "recovered WAL package must be strictly ordered",
        }
    );
}

#[test]
fn bootstrap_preserves_degraded_recovery_health_while_replaying_tail() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x4c);
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(9), Some(7), None)
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(lossy_open_plan(), branch, &backend).expect("durable shell");
    let replayed = wal_record(branch, 5, b"degraded-tail", b"tail-value");
    shell
        .services_mut()
        .wal_mut()
        .append(&replayed)
        .expect("append replayed record");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("degraded recovery outcome");
    assert!(matches!(
        outcome.health(),
        RecoveryHealth::Degraded {
            class: RecoveryDegradationClass::DataLoss,
            faults
        } if faults.iter().any(|fault| fault.kind() == RecoveryFaultKind::MissingSnapshotObject)
    ));

    let runtime = shell
        .complete_recovery(&outcome)
        .expect("degraded recovery opens");

    assert_eq!(runtime.state(), LifecycleState::Open);
    assert_eq!(runtime.visible_version(), CommitVersion::new(5));
    assert_eq!(runtime.bootstrap_report().records_seen(), 1);
    assert_eq!(runtime.bootstrap_report().records_applied(), 1);
    assert_eq!(
        runtime.open_outcome().recovery_health(),
        runtime.bootstrap_report().recovery_health(),
    );
    assert!(matches!(
        runtime.open_outcome().recovery_health(),
        RecoveryHealth::Degraded {
            class: RecoveryDegradationClass::DataLoss,
            faults
        } if faults.iter().any(|fault| fault.kind() == RecoveryFaultKind::MissingSnapshotObject)
    ));
}

#[test]
fn bootstrap_replay_is_idempotent_for_exactly_installed_rows() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x4d);
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let record = wal_record(branch, 4, b"already-installed", b"value");
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .expect("append WAL record");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("recovery outcome");
    shell
        .branch_state_mut()
        .append_committed_rows_atomically(record.commit_payload().rows().to_vec())
        .expect("seed exact installed rows");

    let runtime = shell
        .complete_recovery(&outcome)
        .expect("idempotent bootstrap");

    assert_eq!(runtime.visible_version(), CommitVersion::new(4));
    assert_eq!(runtime.bootstrap_report().records_seen(), 1);
    assert_eq!(runtime.bootstrap_report().records_applied(), 0);
    assert_eq!(runtime.bootstrap_report().records_already_applied(), 1);
    assert_eq!(runtime.bootstrap_report().rows_checked(), 3);
    assert_eq!(runtime.bootstrap_report().rows_applied(), 0);
}

#[test]
fn bootstrap_replay_clears_matching_unresolved_durable_gate() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x4e);
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let record = wal_record(branch, 6, b"gate-cleared", b"value");
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .expect("append WAL record");
    let stamp = CommitStamp::new(branch, CommitVersion::new(6), Timestamp::from_micros(600))
        .expect("stamp");
    let unresolved = CommitUnresolvedDurable::durable_not_applied_with_facts(
        stamp,
        CommitDurabilityClass::Standard,
        "seeded unresolved durable fact",
    )
    .expect("unresolved durable");
    shell
        .durable_gate()
        .record_unresolved(unresolved)
        .expect("seed unresolved gate");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("recovery outcome");

    let runtime = shell.complete_recovery(&outcome).expect("gate bootstrap");

    assert_eq!(runtime.bootstrap_report().records_seen(), 1);
    assert_eq!(runtime.bootstrap_report().records_applied(), 1);
    assert_eq!(runtime.bootstrap_report().gates_cleared(), 1);
    assert_eq!(runtime.unresolved_durable().expect("gate state"), None);
    assert_eq!(runtime.visible_version(), CommitVersion::new(6));
}

#[test]
fn bootstrap_replay_uses_always_durability_for_always_mode() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x50);
    let mut shell = assemble_shell(
        open_plan_for_mode(StorageMode::DurableLocalAlways, RecoveryStrictness::Strict),
        branch,
        &backend,
    )
    .expect("durable shell");
    let record = wal_record(branch, 6, b"always-gate-cleared", b"value");
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .expect("append WAL record");
    let stamp = CommitStamp::new(branch, CommitVersion::new(6), Timestamp::from_micros(600))
        .expect("stamp");
    let unresolved = CommitUnresolvedDurable::durable_not_applied_with_facts(
        stamp,
        CommitDurabilityClass::Always,
        "seeded always unresolved durable fact",
    )
    .expect("unresolved durable");
    shell
        .durable_gate()
        .record_unresolved(unresolved)
        .expect("seed unresolved gate");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("recovery outcome");

    let runtime = shell.complete_recovery(&outcome).expect("always bootstrap");

    assert_eq!(
        runtime.open_outcome().mode(),
        StorageMode::DurableLocalAlways
    );
    assert_eq!(runtime.bootstrap_report().records_seen(), 1);
    assert_eq!(runtime.bootstrap_report().records_applied(), 1);
    assert_eq!(runtime.bootstrap_report().gates_cleared(), 1);
    assert_eq!(runtime.unresolved_durable().expect("gate state"), None);
    assert_eq!(runtime.visible_version(), CommitVersion::new(6));
}

#[test]
fn bootstrap_replay_rejects_mismatched_unresolved_durable_gate() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x4f);
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let record = wal_record(branch, 6, b"blocked-by-gate", b"value");
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .expect("append WAL record");
    let mismatched_stamp =
        CommitStamp::new(branch, CommitVersion::new(7), Timestamp::from_micros(700))
            .expect("stamp");
    let unresolved = CommitUnresolvedDurable::durable_not_applied_with_facts(
        mismatched_stamp,
        CommitDurabilityClass::Standard,
        "different unresolved durable fact",
    )
    .expect("unresolved durable");
    shell
        .durable_gate()
        .record_unresolved(unresolved)
        .expect("seed unresolved gate");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("recovery outcome");

    let error = shell
        .try_bootstrap_commit_runtime_for_test(&outcome)
        .expect_err("mismatched gate blocks replay");

    assert_commit_runtime_source(
        &error,
        &CommitRuntimeError::UnresolvedDurableCommit {
            branch_id: branch,
            commit_version: CommitVersion::new(7),
            reason: "different unresolved durable commit blocks replay",
        },
    );
    assert_eq!(shell.state(), LifecycleState::Failed);
    assert_eq!(shell.visible_version(), CommitVersion::ZERO);
    assert!(shell.branch_state().is_empty());
    assert_eq!(
        shell.unresolved_durable().expect("gate state"),
        Some(unresolved)
    );
    assert!(shell.admit_commit().is_err());
    assert!(shell.admit_recovery_step().is_err());
}

#[test]
fn recovery_loads_checkpoint_installs_rows_and_packages_only_wal_tail() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x32);
    let checkpoint_row = put_row(branch, 3, b"checkpoint", b"checkpoint-value");
    publish_snapshot(
        &backend,
        5,
        CommitVersion::new(3),
        std::slice::from_ref(&checkpoint_row),
    );
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(3), Some(5), Some(CommitVersion::new(2)))
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let skipped = wal_record(branch, 2, b"skipped", b"old");
    let replayed = wal_record(branch, 4, b"tail", b"tail-value");
    shell
        .services_mut()
        .wal_mut()
        .append(&skipped)
        .expect("append skipped record");
    shell
        .services_mut()
        .wal_mut()
        .append(&replayed)
        .expect("append replayed record");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("recovery outcome");

    assert_eq!(outcome.health(), &RecoveryHealth::Healthy);
    assert_eq!(outcome.checkpoint().snapshot_id(), Some(5));
    assert_eq!(
        outcome.checkpoint().trusted_watermark(),
        Some(CommitVersion::new(3))
    );
    assert_eq!(outcome.checkpoint().section_count(), 1);
    assert_eq!(outcome.checkpoint().row_count(), 1);
    assert!(outcome.checkpoint().install_outcome().is_some());
    assert_eq!(outcome.wal().replay_start(), CommitVersion::new(3));
    assert_eq!(outcome.wal().records(), std::slice::from_ref(&replayed));
    assert!(outcome.wal().truncation().is_none());
    assert!(outcome.wal().repair().is_none());

    let view = shell.branch_state().capture_read_view().expect("read view");
    let visible = view
        .latest(checkpoint_row.physical_key())
        .expect("latest")
        .expect("visible checkpoint row");
    assert_eq!(visible.row(), &checkpoint_row);
    assert_eq!(shell.visible_version(), CommitVersion::ZERO);
}

#[test]
fn recovery_does_not_install_checkpoint_when_later_wal_read_fails() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x37);
    let checkpoint_row = put_row(branch, 3, b"checkpoint", b"checkpoint-value");
    publish_snapshot(
        &backend,
        5,
        CommitVersion::new(3),
        std::slice::from_ref(&checkpoint_row),
    );
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(3), Some(5), None)
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    backend.fail_read_object(ObjectLayout::wal_segment(1).expect("active log object"));
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    let error = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect_err("log read failure rejects");

    assert!(matches!(
        error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::Service,
            reason: "WAL recovery read failed",
            ..
        }
    ));
    assert!(error.source().is_some());
    assert!(shell.branch_state().is_empty());
}

#[test]
fn recovery_repairs_latest_partial_log_tail_only_when_explicitly_lossy() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x40);
    let mut shell = assemble_shell(lossy_open_plan(), branch, &backend).expect("durable shell");
    let record = wal_record(branch, 2, b"valid", b"value");
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .expect("append valid record");
    backend.append_raw(
        ObjectLayout::wal_segment(1).expect("active log object"),
        b"partial",
    );
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("partial tail recovery");

    assert_eq!(outcome.wal().records(), std::slice::from_ref(&record));
    assert!(outcome.wal().truncation().is_some());
    assert!(outcome.wal().repair().is_some());
    assert!(matches!(
        outcome.health(),
        RecoveryHealth::Degraded {
            class: RecoveryDegradationClass::DataLoss,
            faults
        } if faults.iter().any(|fault| fault.kind() == RecoveryFaultKind::WalTailRepairFailed)
    ));
}

#[test]
fn recovery_rejects_latest_partial_log_tail_in_strict_mode() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x41);
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let record = wal_record(branch, 2, b"valid", b"value");
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .expect("append valid record");
    backend.append_raw(
        ObjectLayout::wal_segment(1).expect("active log object"),
        b"partial",
    );
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    assert_eq!(
        LifecycleRecoveryRuntime::new(&mut shell)
            .recover(&request)
            .expect_err("strict partial tail rejects"),
        LifecycleError::WalTailRepairRejected {
            reason: "strict recovery cannot repair partial WAL tail",
        }
    );
}

#[test]
fn recovery_rejects_checkpoint_row_newer_than_snapshot_watermark() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x38);
    let checkpoint_row = put_row(branch, 4, b"too-new", b"value");
    publish_snapshot(
        &backend,
        6,
        CommitVersion::new(3),
        std::slice::from_ref(&checkpoint_row),
    );
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(3), Some(6), None)
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    assert_eq!(
        LifecycleRecoveryRuntime::new(&mut shell).recover(&request),
        Err(LifecycleError::RecoveryFailed {
            reason: "checkpoint row commit version exceeds snapshot watermark",
        })
    );
    assert!(shell.branch_state().is_empty());
}

#[test]
fn database_manifest_rejects_zero_snapshot_id_before_recovery() {
    assert!(
        DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(3), Some(0), None)
            .is_err(),
        "zero snapshot ids must be rejected before lifecycle trusts manifest recovery facts"
    );
}

#[test]
fn recovery_rejects_snapshot_section_count_above_request_limit() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x4a);
    let first = put_row(branch, 2, b"first-section", b"value");
    let second = put_row(branch, 3, b"second-section", b"value");
    SnapshotService::new(&backend)
        .publish_create(SnapshotPublishRequest::new(
            10,
            CommitVersion::new(3),
            Timestamp::from_micros(7_000),
            DATABASE_ID,
            "identity",
            vec![
                encode_checkpoint_row_section(std::slice::from_ref(&first))
                    .expect("first row section"),
                encode_checkpoint_row_section(std::slice::from_ref(&second))
                    .expect("second row section"),
            ],
        ))
        .expect("publish multi-section snapshot");
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(3), Some(10), None)
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request = LifecycleRecoveryRequest::new(
        RecoveryStrictness::Strict,
        shell.open_plan().lifecycle_config().max_recovery_faults(),
        1,
        "limited-section-checkpoint",
    )
    .expect("recovery request");

    assert_eq!(
        LifecycleRecoveryRuntime::new(&mut shell)
            .recover(&request)
            .expect_err("too many snapshot sections reject"),
        LifecycleError::RecoveryFailed {
            reason: "snapshot section count exceeds lifecycle recovery limit",
        }
    );
}

#[test]
fn recovery_rejects_checkpoint_rows_for_unopened_branch() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x39);
    let other_branch_row = put_row(branch_id(0x3a), 3, b"other", b"value");
    publish_snapshot(
        &backend,
        7,
        CommitVersion::new(3),
        std::slice::from_ref(&other_branch_row),
    );
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(3), Some(7), None)
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    assert_eq!(
        LifecycleRecoveryRuntime::new(&mut shell).recover(&request),
        Err(LifecycleError::RecoveryFailed {
            reason: "checkpoint contains rows for unopened branch",
        })
    );
    assert!(shell.branch_state().is_empty());
}

#[test]
fn recovery_rejects_flush_watermark_without_recovered_table_state() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x3b);
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, None, None, Some(CommitVersion::new(4)))
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    assert_eq!(
        LifecycleRecoveryRuntime::new(&mut shell).recover(&request),
        Err(LifecycleError::RecoveryFailed {
            reason: "manifest flush watermark requires recovered flushed table state",
        })
    );
    assert!(shell.branch_state().is_empty());
}

#[test]
fn recovery_rejects_ad_hoc_table_object_references() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x42);
    let table_identity = TableIdentity::new("recovery-table-valid").expect("table identity");
    let table_bytes = table_bytes(
        table_identity.clone(),
        std::slice::from_ref(&put_row(branch, 2, b"table-row", b"value")),
    );
    let write = TableObjectService::new(&backend)
        .publish_create("branch", 0, "table0002", &table_bytes)
        .expect("publish table");
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request = LifecycleRecoveryRequest::from_open_plan(shell.open_plan())
        .expect("recovery request")
        .with_table_objects(vec![LifecycleRecoveryTableObject::new(
            table_identity.clone(),
            write.facts().clone(),
        )]);

    assert_eq!(
        LifecycleRecoveryRuntime::new(&mut shell).recover(&request),
        Err(LifecycleError::RecoveryFailed {
            reason: "table object recovery references require a table manifest",
        })
    );
    assert!(shell.branch_state().is_empty());
}

#[test]
fn recovery_rejects_missing_referenced_table_object() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x3d);
    let table_identity = TableIdentity::new("recovery-table").expect("table identity");
    let table_bytes = table_bytes(
        table_identity.clone(),
        std::slice::from_ref(&put_row(branch, 2, b"table-row", b"value")),
    );
    let write = TableObjectService::new(&backend)
        .publish_create("branch", 0, "table0001", &table_bytes)
        .expect("publish table");
    backend
        .delete_object(write.facts().object())
        .expect("remove referenced table");
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request = LifecycleRecoveryRequest::from_open_plan(shell.open_plan())
        .expect("recovery request")
        .with_table_objects(vec![LifecycleRecoveryTableObject::new(
            table_identity,
            write.facts().clone(),
        )]);

    let error = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect_err("missing table rejects");

    assert_eq!(
        error,
        LifecycleError::RecoveryFailed {
            reason: "table object recovery references require a table manifest",
        }
    );
    assert!(shell.branch_state().is_empty());
}

#[test]
fn recovery_validates_tables_before_wal_tail_repair() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x43);
    let table_identity = TableIdentity::new("recovery-table-before-wal").expect("table identity");
    let table_bytes = table_bytes(
        table_identity.clone(),
        std::slice::from_ref(&put_row(branch, 2, b"table-row", b"value")),
    );
    let write = TableObjectService::new(&backend)
        .publish_create("branch", 0, "table0003", &table_bytes)
        .expect("publish table");
    backend
        .delete_object(write.facts().object())
        .expect("remove referenced table");
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let record = wal_record(branch, 3, b"valid", b"value");
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .expect("append valid record");
    let wal_object = ObjectLayout::wal_segment(1).expect("active log object");
    backend.append_raw(wal_object.clone(), b"partial");
    let before = backend.object_bytes(&wal_object).expect("wal bytes before");
    let request = LifecycleRecoveryRequest::from_open_plan(shell.open_plan())
        .expect("recovery request")
        .with_table_objects(vec![LifecycleRecoveryTableObject::new(
            table_identity,
            write.facts().clone(),
        )]);

    let error = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect_err("missing table rejects before WAL repair");

    assert_eq!(
        error,
        LifecycleError::RecoveryFailed {
            reason: "table object recovery references require a table manifest",
        }
    );
    assert_eq!(
        backend.object_bytes(&wal_object).expect("wal bytes after"),
        before,
        "WAL tail must not be repaired after table validation already failed",
    );
}

#[test]
fn recovery_validates_quarantine_before_wal_tail_repair() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x49);
    let mut shell = assemble_shell(lossy_open_plan(), branch, &backend).expect("durable shell");
    let record = wal_record(branch, 3, b"valid-before-quarantine", b"value");
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .expect("append valid record");
    let wal_object = ObjectLayout::wal_segment(1).expect("active log object");
    backend.append_raw(wal_object.clone(), b"partial");
    let before = backend.object_bytes(&wal_object).expect("wal bytes before");
    backend.fail_read_object(
        ObjectLayout::quarantine_manifest(&branch.to_string()).expect("quarantine object"),
    );
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    let error = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect_err("quarantine failure rejects before WAL repair");

    assert!(matches!(
        error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::Service,
            reason: "quarantine inventory recovery failed",
            ..
        }
    ));
    assert_eq!(
        backend.object_bytes(&wal_object).expect("wal bytes after"),
        before,
        "WAL tail must not be repaired after quarantine recovery already failed",
    );
}

#[test]
fn recovery_degrades_quarantine_inventory_mismatch_only_when_explicitly_lossy() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x3e);
    backend.write_raw(
        ObjectLayout::quarantine_manifest(&branch.to_string()).expect("inventory object"),
        b"not inventory".to_vec(),
    );
    let mut strict_shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("strict shell");
    let strict_request =
        LifecycleRecoveryRequest::from_open_plan(strict_shell.open_plan()).expect("strict request");
    let strict_error = LifecycleRecoveryRuntime::new(&mut strict_shell)
        .recover(&strict_request)
        .expect_err("strict inventory mismatch rejects");
    assert!(matches!(
        strict_error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::Service,
            reason: "quarantine inventory recovery failed",
            ..
        }
    ));
    drop(strict_shell);

    let mut lossy_shell = assemble_shell(lossy_open_plan(), branch, &backend).expect("lossy shell");
    let lossy_request =
        LifecycleRecoveryRequest::from_open_plan(lossy_shell.open_plan()).expect("lossy request");
    let outcome = LifecycleRecoveryRuntime::new(&mut lossy_shell)
        .recover(&lossy_request)
        .expect("lossy inventory mismatch degrades");

    assert!(matches!(
        outcome.health(),
        RecoveryHealth::Degraded {
            class: RecoveryDegradationClass::Telemetry,
            faults
        } if faults.iter().any(
            |fault| fault.kind() == RecoveryFaultKind::QuarantineInventoryMismatch
        )
    ));
    assert_eq!(
        outcome.quarantine().object(),
        Some(&ObjectLayout::quarantine_manifest(&branch.to_string()).expect("quarantine object"))
    );
    assert!(!outcome.quarantine().is_present());
    assert_eq!(outcome.quarantine().byte_count(), 0);
    assert_eq!(outcome.quarantine().entry_count(), 0);
}

#[test]
fn recovery_rejects_missing_snapshot_in_strict_mode() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x33);
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(9), Some(6), None)
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    let error = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect_err("strict missing snapshot rejects");

    assert!(matches!(
        error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::Service,
            reason: "manifest-listed snapshot is missing",
            ..
        }
    ));
    assert!(error.source().is_some());
    assert!(shell.branch_state().is_empty());
}

#[test]
fn recovery_allows_explicit_lossy_missing_snapshot_without_trusting_watermark() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x34);
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(9), Some(7), None)
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(lossy_open_plan(), branch, &backend).expect("durable shell");
    let replayed = wal_record(branch, 5, b"lossy-tail", b"tail-value");
    shell
        .services_mut()
        .wal_mut()
        .append(&replayed)
        .expect("append replayed record");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    assert_eq!(
        request.strictness(),
        RecoveryStrictness::AllowExplicitLossyFallback
    );
    assert!(request.max_faults() > 0);
    assert!(request.max_snapshot_sections() > 0);

    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("lossy recovery outcome");

    assert!(matches!(
        outcome.health(),
        RecoveryHealth::Degraded {
            class: RecoveryDegradationClass::DataLoss,
            faults
        } if faults.iter().any(|fault| fault.kind() == RecoveryFaultKind::MissingSnapshotObject)
    ));
    assert_eq!(outcome.checkpoint().snapshot_id(), Some(7));
    assert_eq!(outcome.checkpoint().trusted_watermark(), None);
    assert_eq!(outcome.wal().replay_start(), CommitVersion::ZERO);
    assert_eq!(outcome.wal().records(), &[replayed]);
}

#[test]
fn lossy_missing_snapshot_allows_uncertain_flush_watermark_as_degraded_data_loss() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x36);
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(9), Some(7), Some(CommitVersion::new(6)))
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(lossy_open_plan(), branch, &backend).expect("durable shell");
    let replayed = wal_record(branch, 5, b"lossy-flush-tail", b"tail-value");
    shell
        .services_mut()
        .wal_mut()
        .append(&replayed)
        .expect("append replayed record");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("lossy recovery with missing checkpoint");

    assert!(matches!(
        outcome.health(),
        RecoveryHealth::Degraded {
            class: RecoveryDegradationClass::DataLoss,
            faults
        } if faults.iter().any(|fault| fault.kind() == RecoveryFaultKind::MissingSnapshotObject)
    ));
    assert_eq!(outcome.checkpoint().snapshot_id(), Some(7));
    assert_eq!(outcome.checkpoint().trusted_watermark(), None);
    assert_eq!(outcome.wal().replay_start(), CommitVersion::ZERO);
    assert_eq!(outcome.wal().records(), &[replayed]);
}

#[test]
fn recovery_request_rejects_lossy_when_open_plan_is_strict() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x35);
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request = LifecycleRecoveryRequest::new(
        RecoveryStrictness::AllowExplicitLossyFallback,
        1,
        1,
        "lossy-request",
    )
    .expect("request");

    assert_eq!(
        LifecycleRecoveryRuntime::new(&mut shell).recover(&request),
        Err(LifecycleError::InvalidOpenPlan {
            reason: "lossy recovery request requires lossy open plan",
        })
    );
}

#[test]
fn recovery_request_validates_limits_and_checkpoint_identity() {
    assert_eq!(
        LifecycleRecoveryRequest::new(RecoveryStrictness::Strict, 0, 1, "valid"),
        Err(LifecycleError::InvalidConfig {
            field: "max_faults",
            reason: "must be nonzero",
        })
    );
    assert_eq!(
        LifecycleRecoveryRequest::new(RecoveryStrictness::Strict, 1, 0, "valid"),
        Err(LifecycleError::InvalidConfig {
            field: "max_snapshot_sections",
            reason: "must be nonzero",
        })
    );
    assert!(matches!(
        LifecycleRecoveryRequest::new(RecoveryStrictness::Strict, 1, 1, ""),
        Err(LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::TableRuntime,
            reason: "invalid recovery table identity",
            ..
        })
    ));
}

#[test]
fn checkpoint_row_section_round_trips_and_rejects_trailing_bytes() {
    let row = put_row(branch_id(0x36), 11, b"section", b"value");
    let section =
        encode_checkpoint_row_section(std::slice::from_ref(&row)).expect("row checkpoint section");
    assert_eq!(section.section_kind(), SNAPSHOT_ROW_SECTION_KIND);
    let container = crate::format::SnapshotContainer::new(
        crate::format::SnapshotHeader::new(
            8,
            CommitVersion::new(11),
            Timestamp::from_micros(1_100),
            DATABASE_ID,
            "identity",
        )
        .expect("header"),
        vec![section.clone()],
    );
    assert_eq!(container.sections()[0], section);

    let mut trailing = section.payload().to_vec();
    trailing.push(0);
    let invalid = crate::format::SnapshotSection::new(SNAPSHOT_ROW_SECTION_KIND, trailing)
        .expect("invalid row section shape");
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x36);
    let snapshot = crate::format::SnapshotContainer::new(
        crate::format::SnapshotHeader::new(
            8,
            CommitVersion::new(11),
            Timestamp::from_micros(1_100),
            DATABASE_ID,
            "identity",
        )
        .expect("header"),
        vec![invalid],
    );
    let snapshot_bytes = crate::format::encode_snapshot_container(&snapshot).expect("snapshot");
    backend.write_raw(ObjectLayout::snapshot(8).expect("snapshot"), snapshot_bytes);
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(11), Some(8), None)
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let error = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect_err("trailing row section rejects");
    assert!(matches!(
        error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::Format,
            reason: "snapshot section decode failed",
            ..
        }
    ));
    assert!(error.source().is_some());
}

#[test]
fn checkpoint_row_section_rejects_declared_rows_without_length_prefixes() {
    let mut payload = Vec::new();
    payload.extend_from_slice(b"STRR");
    payload.extend_from_slice(&1_u32.to_le_bytes());
    payload.extend_from_slice(&u32::MAX.to_le_bytes());
    let invalid = crate::format::SnapshotSection::new(SNAPSHOT_ROW_SECTION_KIND, payload)
        .expect("invalid row section shape");
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x3c);
    let snapshot = crate::format::SnapshotContainer::new(
        crate::format::SnapshotHeader::new(
            9,
            CommitVersion::new(11),
            Timestamp::from_micros(1_100),
            DATABASE_ID,
            "identity",
        )
        .expect("header"),
        vec![invalid],
    );
    let snapshot_bytes = crate::format::encode_snapshot_container(&snapshot).expect("snapshot");
    backend.write_raw(
        ObjectLayout::snapshot(9).expect("snapshot object"),
        snapshot_bytes,
    );
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(11), Some(9), None)
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    let error = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect_err("impossible row count rejects");

    assert!(matches!(
        error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::Format,
            reason: "snapshot section decode failed",
            ..
        }
    ));
    assert!(error.source().is_some());
    assert!(shell.branch_state().is_empty());
}

fn assemble_shell(
    plan: StorageOpenPlan,
    branch: BranchId,
    backend: &RecoveryTestBackend,
) -> LifecycleResult<LifecycleDurableLocalShell<'_>> {
    LifecycleDurableLocalShell::assemble(
        LifecycleDurableLocalOpenRequest::new(
            plan,
            DATABASE_ID,
            branch,
            CommitBranchGeneration::new(1).expect("generation"),
            BranchRuntimeConfig::default(),
            CommitRuntimeConfig::default(),
            WalServiceConfig::default(),
        )?,
        backend,
        timestamp_source(),
    )
}

fn open_plan(recovery_policy: RecoveryStrictness) -> StorageOpenPlan {
    open_plan_for_mode(StorageMode::DurableLocalStandard, recovery_policy)
}

fn open_plan_for_mode(mode: StorageMode, recovery_policy: RecoveryStrictness) -> StorageOpenPlan {
    StorageOpenPlan::new(
        mode,
        LifecycleCodecId::identity(),
        recovery_policy,
        LifecycleConfig::default(),
    )
    .expect("open plan")
}

fn lossy_open_plan() -> StorageOpenPlan {
    let config = LifecycleConfig::new(
        1024,
        16,
        LifecycleCloseTimeoutPolicy::ReturnTypedTimeout,
        LifecycleLossyRecoveryPolicy::ExplicitlyAllowed,
    )
    .expect("lossy lifecycle config");
    StorageOpenPlan::new(
        StorageMode::DurableLocalStandard,
        LifecycleCodecId::identity(),
        RecoveryStrictness::AllowExplicitLossyFallback,
        config,
    )
    .expect("lossy open plan")
}

fn publish_snapshot(
    backend: &RecoveryTestBackend,
    snapshot_id: u64,
    watermark: CommitVersion,
    rows: &[StorageRow],
) {
    SnapshotService::new(backend)
        .publish_create(SnapshotPublishRequest::new(
            snapshot_id,
            watermark,
            Timestamp::from_micros(7_000),
            DATABASE_ID,
            "identity",
            vec![encode_checkpoint_row_section(rows).expect("row section")],
        ))
        .expect("publish snapshot");
}

fn write_manifest(backend: &RecoveryTestBackend, manifest: &DatabaseManifest) {
    backend.write_raw(
        ObjectLayout::database_manifest().expect("database root object"),
        encode_manifest(manifest).expect("database root bytes"),
    );
}

fn wal_record(
    branch: BranchId,
    version: u64,
    user_key: &'static [u8],
    value: &'static [u8],
) -> WalRecord {
    let commit_version = CommitVersion::new(version);
    let timestamp = Timestamp::from_micros(version * 100);
    let stamp = CommitStamp::new(branch, commit_version, timestamp).expect("stamp");
    let row = StorageRow::put(
        physical_key(branch, user_key),
        commit_version,
        timestamp,
        Timestamp::EPOCH,
        value.to_vec(),
    );
    let timeline_rows =
        CommitTimelineRows::from_entry(CommitTimelineEntry::from_stamp(stamp).expect("entry"))
            .expect("timeline rows")
            .into_rows();
    let payload = WalCommitPayload::new(vec![
        row,
        timeline_rows[0].clone(),
        timeline_rows[1].clone(),
    ])
    .expect("payload");
    WalRecord::new(commit_version, branch, timestamp, payload).expect("record")
}

fn timeline_only_wal_record(branch: BranchId, version: u64) -> WalRecord {
    let commit_version = CommitVersion::new(version);
    let timestamp = Timestamp::from_micros(version * 100);
    let stamp = CommitStamp::new(branch, commit_version, timestamp).expect("stamp");
    let timeline_rows =
        CommitTimelineRows::from_entry(CommitTimelineEntry::from_stamp(stamp).expect("entry"))
            .expect("timeline rows")
            .into_rows();
    let payload = WalCommitPayload::new(timeline_rows.into()).expect("payload");
    WalRecord::new(commit_version, branch, timestamp, payload).expect("record")
}

fn user_only_wal_record(
    branch: BranchId,
    version: u64,
    user_key: &'static [u8],
    value: &'static [u8],
) -> WalRecord {
    let commit_version = CommitVersion::new(version);
    let timestamp = Timestamp::from_micros(version * 100);
    let row = StorageRow::put(
        physical_key(branch, user_key),
        commit_version,
        timestamp,
        Timestamp::EPOCH,
        value.to_vec(),
    );
    let payload = WalCommitPayload::new(vec![row]).expect("payload");
    WalRecord::new(commit_version, branch, timestamp, payload).expect("record")
}

fn put_row(
    branch: BranchId,
    version: u64,
    user_key: &'static [u8],
    value: &'static [u8],
) -> StorageRow {
    StorageRow::put(
        physical_key(branch, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(version * 100),
        Timestamp::EPOCH,
        value.to_vec(),
    )
}

fn table_bytes(identity: TableIdentity, rows: &[StorageRow]) -> Vec<u8> {
    let rows = rows.iter().cloned().map(TableRow::new).collect::<Vec<_>>();
    ImmutableTableBuilder::new(TableBuilderConfig::default())
        .expect("table builder")
        .build_from_rows(identity, &rows)
        .expect("build table")
        .into_bytes()
}

fn physical_key(branch: BranchId, user_key: &'static [u8]) -> PhysicalKey {
    PhysicalKey::new(
        branch,
        "lifecycle",
        StorageSpaceId::engine(0x20).expect("space"),
        user_key.to_vec(),
    )
    .expect("physical key")
}

fn durable_standard_batch(
    branch: BranchId,
    user_key: &'static [u8],
    value: &'static [u8],
) -> CommitBatch {
    CommitBatch::mutating(
        branch,
        vec![CommitMutation::put(
            physical_key(branch, user_key),
            value.to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::new(
            CommitDurabilityMode::Standard,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::RuntimeGenerated,
            CommitOrigin::StorageRuntime,
        ),
    )
}

fn assert_commit_runtime_source(error: &LifecycleError, expected: &CommitRuntimeError) {
    assert!(matches!(
        error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::CommitRuntime,
            reason: "commit runtime failed",
            ..
        }
    ));
    let source = error.source().expect("commit source");
    let commit_error = source
        .downcast_ref::<CommitRuntimeError>()
        .expect("commit runtime source");
    assert_eq!(commit_error, expected);
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; 16])
}

fn timestamp_source() -> CommitManualTimestampSource {
    CommitManualTimestampSource::new(Timestamp::from_micros(8_000))
}

#[derive(Debug)]
struct RecoveryTestBackend {
    capabilities: BackendCapabilities,
    objects: Mutex<BTreeMap<ObjectName, Vec<u8>>>,
    fail_reads: Mutex<BTreeSet<ObjectName>>,
    lock_held: Arc<AtomicBool>,
}

impl RecoveryTestBackend {
    fn new() -> Self {
        Self {
            capabilities: BackendCapabilities::from_slice(DURABLE_LOCAL_MODE_REQUIREMENTS),
            objects: Mutex::new(BTreeMap::new()),
            fail_reads: Mutex::new(BTreeSet::new()),
            lock_held: Arc::new(AtomicBool::new(false)),
        }
    }

    fn write_raw(&self, object: ObjectName, bytes: Vec<u8>) {
        self.objects.lock().expect("objects").insert(object, bytes);
    }

    fn append_raw(&self, object: ObjectName, bytes: &[u8]) {
        self.objects
            .lock()
            .expect("objects")
            .entry(object)
            .or_default()
            .extend_from_slice(bytes);
    }

    fn object_bytes(&self, object: &ObjectName) -> Option<Vec<u8>> {
        self.objects.lock().expect("objects").get(object).cloned()
    }

    fn fail_read_object(&self, object: ObjectName) {
        self.fail_reads
            .lock()
            .expect("read failures")
            .insert(object);
    }
}

impl Backend for RecoveryTestBackend {
    fn capabilities(&self) -> BackendCapabilities {
        self.capabilities
    }

    fn read_object(&self, name: &ObjectName) -> BackendResult<Vec<u8>> {
        if self
            .fail_reads
            .lock()
            .expect("read failures")
            .contains(name)
        {
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "injected read failure",
            ));
        }
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
        self.objects.lock().expect("objects").remove(name);
        Ok(())
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        let mut names: Vec<_> = self
            .objects
            .lock()
            .expect("objects")
            .keys()
            .filter(|name| name.as_str().starts_with(prefix.as_str()))
            .cloned()
            .collect();
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

    fn acquire_writer_lock(&self, name: &ObjectName) -> BackendResult<BackendWriterGuard> {
        if self.lock_held.swap(true, Ordering::SeqCst) {
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "writer lock already held",
            ));
        }
        Ok(BackendWriterGuard::new(
            name.clone(),
            HeldWriterLock {
                locked: Arc::clone(&self.lock_held),
            },
        ))
    }

    fn append_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendAppend> {
        let mut objects = self.objects.lock().expect("objects");
        let object = objects.entry(name.clone()).or_default();
        let start_offset = object.len() as u64;
        object.extend_from_slice(bytes);
        Ok(BackendAppend::new(
            start_offset,
            bytes.len() as u64,
            BackendMetadata::new(object.len() as u64, None),
        ))
    }

    fn sync_object(&self, _name: &ObjectName) -> BackendResult<()> {
        Ok(())
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        bytes: &[u8],
        mode: PublishMode,
    ) -> PublishResult<PublishOutcome> {
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

struct HeldWriterLock {
    locked: Arc<AtomicBool>,
}

impl Drop for HeldWriterLock {
    fn drop(&mut self) {
        self.locked.store(false, Ordering::SeqCst);
    }
}

struct MaintenanceTestRunner;

impl MaintenanceTaskRunner for MaintenanceTestRunner {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        Ok(MaintenanceOutcome::new(
            task.kind(),
            MaintenanceOutcomeStatus::Completed,
        ))
    }
}
