use super::checkpoint::shared::{
    branch_id, durable_batch, generation_guard, CheckpointBackendEvent, CheckpointTestBackend,
};
use super::*;
use crate::backend::memory::MemoryBackend;
use crate::branch::config::BranchRuntimeConfig;
use crate::commit::{
    CommitBatch, CommitBatchOptions, CommitBranchGeneration, CommitConflictValidationMode,
    CommitDuplicateKeyPolicy, CommitDurabilityMode, CommitExpiry, CommitManualTimestampSource,
    CommitMutation, CommitOrigin, CommitRetentionHint, CommitRuntimeConfig, CommitTimestampPolicy,
    CommitValidationFacts,
};
use crate::row::{PhysicalKey, StorageSpaceId};
use crate::service::{WalGrowthFacts, WalServiceConfig};
use strata_core_next::{BranchId, Timestamp};

const DATABASE_ID: [u8; 16] = [0x7a; 16];

#[test]
fn wal_growth_policy_triggers_on_each_threshold_deterministically() {
    let policy = LifecycleWalGrowthPolicy::new(100, 2, 3);
    let by_bytes = WalGrowthFacts::new_for_policy(1, 101, 1, 101, 0, 0);
    let by_segments = WalGrowthFacts::new_for_policy(3, 90, 3, 30, 0, 0);
    let by_commits = WalGrowthFacts::new_for_policy(1, 90, 1, 90, 0, 0);

    assert_eq!(
        policy.trigger_for(by_bytes, 0),
        Some(LifecycleWalGrowthTrigger::RetainedBytes)
    );
    assert_eq!(
        policy.trigger_for(by_segments, 0),
        Some(LifecycleWalGrowthTrigger::RetainedSegments)
    );
    assert_eq!(
        policy.trigger_for(by_commits, 4),
        Some(LifecycleWalGrowthTrigger::CommitsSinceCheckpoint)
    );
    assert_eq!(policy.trigger_for(by_commits, 3), None);
    assert_eq!(
        LifecycleWalGrowthPolicy::disabled().trigger_for(by_bytes, 99),
        None
    );
}

#[test]
fn automatic_checkpoint_does_not_trigger_below_threshold() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x91);
    let policy = LifecycleWalGrowthPolicy::new(u64::MAX, usize::MAX, u64::MAX);
    let mut runtime = open_durable_runtime(branch, &backend, policy);

    runtime
        .execute_durable_commit(
            durable_batch(branch, b"below-policy", b"value"),
            generation_guard(),
        )
        .expect("durable commit");
    let outcome = runtime
        .last_wal_growth_outcome()
        .expect("automatic policy outcome");

    assert_eq!(outcome.status(), LifecycleWalGrowthStatus::BelowThreshold);
    assert!(outcome.trigger().is_none());
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
}

#[test]
fn automatic_checkpoint_triggers_when_wal_bytes_exceed_threshold() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x92);
    let policy = LifecycleWalGrowthPolicy::new(1, usize::MAX, u64::MAX);
    let mut runtime = open_durable_runtime(branch, &backend, policy);

    runtime
        .execute_durable_commit(
            durable_batch(branch, b"bytes-policy", b"value"),
            generation_guard(),
        )
        .expect("durable commit");
    let outcome = runtime
        .last_wal_growth_outcome()
        .expect("automatic policy outcome");

    assert_eq!(
        outcome.status(),
        LifecycleWalGrowthStatus::CheckpointEnqueued
    );
    assert_eq!(
        outcome.trigger(),
        Some(LifecycleWalGrowthTrigger::RetainedBytes)
    );
    assert!(outcome.facts().retained_bytes() > 1);
    assert!(outcome.facts().retained_segments() >= 1);
    assert!(outcome.facts().active_segment_id() >= 1);
    assert!(outcome.facts().active_segment_size() > 0);
    assert!(outcome.facts().dirty_bytes() > 0);
    assert!(outcome.facts().dirty_records() > 0);
    assert_eq!(outcome.commits_since_checkpoint(), 1);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
    assert!(backend.snapshot_objects().is_empty());
    assert_eq!(backend.delete_calls(), 0);

    let maintenance = runtime
        .run_next_checkpoint_maintenance()
        .expect("checkpoint runner")
        .expect("checkpoint outcome");
    assert_eq!(maintenance.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(backend.snapshot_objects().len(), 1);
    assert_eq!(backend.delete_calls(), 0);
}

#[test]
fn automatic_checkpoint_triggers_when_retained_segments_exceed_threshold() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x97);
    let policy = LifecycleWalGrowthPolicy::new(u64::MAX, 1, u64::MAX);
    let mut runtime = open_durable_runtime_with_wal_segment_size(branch, &backend, policy, 4096);

    for index in 0..8 {
        runtime
            .execute_durable_commit(
                dynamic_durable_batch(
                    branch,
                    format!("segment-policy-{index}").into_bytes(),
                    vec![index; 3600],
                ),
                generation_guard(),
            )
            .expect("durable commit");
        if runtime.services().wal().active_segment_id() > 1 {
            break;
        }
    }
    let outcome = runtime
        .last_wal_growth_outcome()
        .expect("automatic policy outcome");

    assert_eq!(
        outcome.status(),
        LifecycleWalGrowthStatus::CheckpointEnqueued
    );
    assert_eq!(
        outcome.trigger(),
        Some(LifecycleWalGrowthTrigger::RetainedSegments)
    );
    assert!(outcome.facts().retained_segments() > 1);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
}

#[test]
fn automatic_checkpoint_coalesces_existing_checkpoint_task() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x93);
    let policy = LifecycleWalGrowthPolicy::new(1, usize::MAX, u64::MAX);
    let mut runtime = open_durable_runtime(branch, &backend, policy);

    runtime
        .execute_durable_commit(
            durable_batch(branch, b"coalesce-policy", b"value"),
            generation_guard(),
        )
        .expect("durable commit");
    let first = runtime
        .last_wal_growth_outcome()
        .cloned()
        .expect("automatic policy outcome");
    let second = runtime.evaluate_wal_growth_policy().expect("policy retry");

    assert_eq!(first.status(), LifecycleWalGrowthStatus::CheckpointEnqueued);
    assert_eq!(
        second.status(),
        LifecycleWalGrowthStatus::CheckpointCoalesced
    );
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
    assert_eq!(
        first.enqueue().map(|outcome| outcome.task_id()),
        second.enqueue().map(|outcome| outcome.task_id())
    );
}

#[test]
fn automatic_checkpoint_uses_existing_maintenance_executor() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x8d);
    let policy = LifecycleWalGrowthPolicy::new(1, usize::MAX, u64::MAX);
    let mut runtime = open_durable_runtime(branch, &backend, policy);

    runtime
        .execute_durable_commit(
            durable_batch(branch, b"executor-policy", b"value"),
            generation_guard(),
        )
        .expect("durable commit");
    let enqueue = runtime
        .last_wal_growth_outcome()
        .expect("automatic policy outcome")
        .enqueue()
        .copied()
        .expect("checkpoint enqueue");

    assert!(enqueue.was_enqueued());
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
    assert!(backend.snapshot_objects().is_empty());

    let maintenance = runtime
        .run_next_checkpoint_maintenance()
        .expect("checkpoint runner")
        .expect("checkpoint outcome");

    assert_eq!(maintenance.task_id(), Some(enqueue.task_id()));
    assert_eq!(maintenance.task_kind(), MaintenanceTaskKind::Checkpoint);
    assert_eq!(maintenance.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    assert_eq!(backend.snapshot_objects().len(), 1);
}

#[test]
fn automatic_checkpoint_failure_records_health_debt() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x98);
    let policy = LifecycleWalGrowthPolicy::new(1, usize::MAX, u64::MAX);
    let mut runtime = open_durable_runtime(branch, &backend, policy);

    backend.fail_wal_listing();
    runtime
        .execute_durable_commit(
            durable_batch(branch, b"failure-policy", b"value"),
            generation_guard(),
        )
        .expect("commit stays successful");
    let outcome = runtime
        .last_wal_growth_outcome()
        .expect("automatic policy outcome");

    assert_eq!(outcome.status(), LifecycleWalGrowthStatus::Deferred);
    assert!(outcome.recovery_health().is_some());
    assert!(outcome.source_error().is_some());
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
}

#[test]
fn automatic_checkpoint_disable_requires_explicit_config() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x99);
    let mut runtime = open_durable_runtime(branch, &backend, LifecycleWalGrowthPolicy::disabled());

    runtime
        .execute_durable_commit(
            durable_batch(branch, b"disabled-policy", b"value"),
            generation_guard(),
        )
        .expect("durable commit");
    let outcome = runtime
        .last_wal_growth_outcome()
        .expect("automatic policy outcome");

    assert_eq!(outcome.status(), LifecycleWalGrowthStatus::Disabled);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
}

#[test]
fn automatic_checkpoint_deferred_while_quiesce_active() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x94);
    let policy = LifecycleWalGrowthPolicy::new(1, usize::MAX, u64::MAX);
    let mut runtime = open_durable_runtime(branch, &backend, policy);

    runtime
        .execute_durable_commit(
            durable_batch(branch, b"quiesce-policy", b"value"),
            generation_guard(),
        )
        .expect("durable commit");
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
    runtime
        .run_next_checkpoint_maintenance()
        .expect("checkpoint runner")
        .expect("checkpoint outcome");
    let quiesce = runtime
        .guard_set()
        .try_begin_quiesce()
        .expect("begin quiesce");
    let outcome = runtime
        .evaluate_wal_growth_policy()
        .expect("policy evaluation");

    assert_eq!(outcome.status(), LifecycleWalGrowthStatus::Deferred);
    assert!(outcome.recovery_health().is_none());
    assert!(outcome.source_error().is_some());
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    drop(quiesce);
    let retry = runtime.evaluate_wal_growth_policy().expect("policy retry");
    assert_eq!(retry.status(), LifecycleWalGrowthStatus::CheckpointEnqueued);
}

#[test]
fn automatic_checkpoint_deferred_while_close_in_progress() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x8e);
    let policy = LifecycleWalGrowthPolicy::new(1, usize::MAX, u64::MAX);
    let mut runtime = open_durable_runtime(branch, &backend, policy);

    runtime
        .execute_durable_commit(
            durable_batch(branch, b"close-policy", b"value"),
            generation_guard(),
        )
        .expect("durable commit");
    runtime
        .run_next_checkpoint_maintenance()
        .expect("checkpoint runner")
        .expect("checkpoint outcome");
    runtime
        .force_close_requested_for_test()
        .expect("close requested");
    let outcome = runtime
        .evaluate_wal_growth_policy()
        .expect("policy evaluation");

    assert_eq!(outcome.status(), LifecycleWalGrowthStatus::Deferred);
    assert!(outcome.recovery_health().is_none());
    assert_eq!(
        outcome.source_error().expect("source error").code(),
        "failed_precondition.lifecycle.state"
    );
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
}

#[test]
fn automatic_checkpoint_deferred_while_recovery_in_progress() {
    let mut state = LifecycleStateMachine::new();
    state
        .transition(LifecycleTransitionTrigger::OpenRequested)
        .expect("opening");
    state
        .transition(LifecycleTransitionTrigger::DurableRecoveryRequired)
        .expect("recovering");

    let error = policy_admission_error(state).expect("recovery rejects maintenance");

    assert_eq!(state.state(), LifecycleState::Recovering);
    assert_eq!(error.code(), "failed_precondition.lifecycle.state");
}

#[test]
fn automatic_checkpoint_cache_mode_reports_no_durable_action() {
    let branch = branch_id(0x95);
    let backend = MemoryBackend::new();
    let runtime = LifecycleCacheRuntime::open(
        LifecycleCacheOpenRequest::new(
            StorageOpenPlan::new(
                StorageMode::Cache,
                LifecycleCodecId::identity(),
                RecoveryStrictness::Strict,
                LifecycleConfig::default(),
            )
            .expect("cache plan"),
            branch,
            CommitBranchGeneration::new(1).expect("generation"),
        )
        .expect("cache request"),
        &backend,
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        CommitManualTimestampSource::new(Timestamp::from_micros(1_000)),
    )
    .expect("cache runtime");

    let outcome = runtime.evaluate_wal_growth_policy();
    assert_eq!(outcome.status(), LifecycleWalGrowthStatus::NoDurableAction);
    assert_eq!(outcome.facts(), WalGrowthFacts::empty());
}

#[test]
fn wal_growth_pressure_facts_are_visible_to_public_boundary() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x8f);
    let policy = LifecycleWalGrowthPolicy::new(1, usize::MAX, u64::MAX);
    let mut runtime = open_durable_runtime(branch, &backend, policy);

    runtime
        .execute_durable_commit(
            durable_batch(branch, b"facts-policy", b"value"),
            generation_guard(),
        )
        .expect("durable commit");
    let outcome = runtime
        .last_wal_growth_outcome()
        .expect("automatic policy outcome");
    let facts = outcome.facts();

    assert_eq!(
        outcome.status(),
        LifecycleWalGrowthStatus::CheckpointEnqueued
    );
    assert_eq!(
        outcome.trigger(),
        Some(LifecycleWalGrowthTrigger::RetainedBytes)
    );
    assert_eq!(outcome.commits_since_checkpoint(), 1);
    assert!(facts.retained_bytes() > 0);
    assert!(facts.retained_segments() > 0);
    assert!(facts.active_segment_size() > 0);
    assert!(facts.dirty_bytes() > 0);
    assert!(facts.dirty_records() > 0);
}

#[test]
fn automatic_checkpoint_policy_is_deterministic_without_background_thread() {
    let first_backend = CheckpointTestBackend::new();
    let second_backend = CheckpointTestBackend::new();
    let branch = branch_id(0x8a);
    let policy = LifecycleWalGrowthPolicy::new(1, usize::MAX, u64::MAX);
    let mut first = open_durable_runtime(branch, &first_backend, policy);
    let mut second = open_durable_runtime(branch, &second_backend, policy);

    for runtime in [&mut first, &mut second] {
        runtime
            .execute_durable_commit(
                durable_batch(branch, b"deterministic-policy", b"value"),
                generation_guard(),
            )
            .expect("durable commit");
    }
    let first_outcome = first
        .last_wal_growth_outcome()
        .expect("first automatic policy outcome");
    let second_outcome = second
        .last_wal_growth_outcome()
        .expect("second automatic policy outcome");

    assert_eq!(first_outcome.status(), second_outcome.status());
    assert_eq!(first_outcome.trigger(), second_outcome.trigger());
    assert_eq!(
        first_outcome.commits_since_checkpoint(),
        second_outcome.commits_since_checkpoint()
    );
    assert_eq!(first_outcome.facts(), second_outcome.facts());
    assert_eq!(first.maintenance_status().pending_tasks(), 1);
    assert_eq!(second.maintenance_status().pending_tasks(), 1);
    assert!(first_backend.snapshot_objects().is_empty());
    assert!(second_backend.snapshot_objects().is_empty());
}

fn open_durable_runtime(
    branch: BranchId,
    backend: &CheckpointTestBackend,
    policy: LifecycleWalGrowthPolicy,
) -> LifecycleDurableLocalRuntime<'_, CommitManualTimestampSource> {
    open_durable_runtime_with_wal_segment_size(
        branch,
        backend,
        policy,
        WalServiceConfig::default().segment_size(),
    )
}

fn open_durable_runtime_with_wal_segment_size(
    branch: BranchId,
    backend: &CheckpointTestBackend,
    policy: LifecycleWalGrowthPolicy,
    segment_size: u64,
) -> LifecycleDurableLocalRuntime<'_, CommitManualTimestampSource> {
    let lifecycle_config = LifecycleConfig::default()
        .with_wal_growth_policy(policy)
        .expect("lifecycle config");
    let request = LifecycleDurableLocalOpenRequest::new(
        StorageOpenPlan::new(
            StorageMode::DurableLocalStandard,
            LifecycleCodecId::identity(),
            RecoveryStrictness::Strict,
            lifecycle_config,
        )
        .expect("open plan"),
        DATABASE_ID,
        branch,
        CommitBranchGeneration::new(1).expect("generation"),
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        WalServiceConfig::new(segment_size),
    )
    .expect("durable request");
    let mut shell = LifecycleDurableLocalShell::assemble(
        request,
        backend,
        CommitManualTimestampSource::new(Timestamp::from_micros(9_000)),
    )
    .expect("durable shell");
    let recovery_request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let recovery = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&recovery_request)
        .expect("recovery outcome");
    shell.complete_recovery(&recovery).expect("runtime")
}

fn dynamic_durable_batch(branch: BranchId, user_key: Vec<u8>, value: Vec<u8>) -> CommitBatch {
    CommitBatch::mutating(
        branch,
        vec![CommitMutation::put(
            dynamic_physical_key(branch, user_key),
            value,
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

fn dynamic_physical_key(branch: BranchId, user_key: Vec<u8>) -> PhysicalKey {
    PhysicalKey::new(
        branch,
        "commit-hardening",
        StorageSpaceId::engine(0x35).expect("engine storage space"),
        user_key,
    )
    .expect("physical key")
}

#[test]
fn automatic_checkpoint_does_not_truncate_wal_without_retention_proof() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x96);
    let policy = LifecycleWalGrowthPolicy::new(1, usize::MAX, u64::MAX);
    let mut runtime = open_durable_runtime(branch, &backend, policy);

    runtime
        .execute_durable_commit(
            durable_batch(branch, b"proof-policy", b"value"),
            generation_guard(),
        )
        .expect("durable commit");
    let maintenance = runtime
        .run_next_checkpoint_maintenance()
        .expect("checkpoint runner")
        .expect("checkpoint outcome");

    assert_eq!(maintenance.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(backend.delete_calls(), 0);
    assert!(!backend
        .events()
        .iter()
        .any(|event| matches!(event, CheckpointBackendEvent::ObjectDelete)));
}

#[test]
fn automatic_checkpoint_truncates_wal_only_after_checkpoint_or_table_manifest_proof() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x8b);
    let policy = LifecycleWalGrowthPolicy::new(1, usize::MAX, u64::MAX);
    let mut runtime = open_durable_runtime_with_wal_segment_size(branch, &backend, policy, 4096);

    for index in 0..8 {
        runtime
            .execute_durable_commit(
                dynamic_durable_batch(
                    branch,
                    format!("proof-rotation-{index}").into_bytes(),
                    vec![index; 3600],
                ),
                generation_guard(),
            )
            .expect("durable commit");
        if runtime.maintenance_status().pending_tasks() > 0 {
            runtime
                .run_next_checkpoint_maintenance()
                .expect("automatic checkpoint runner")
                .expect("automatic checkpoint outcome");
        }
        assert_eq!(backend.delete_calls(), 0);
        if runtime.services().wal().active_segment_id() > 1 {
            break;
        }
    }
    assert!(
        runtime.services().wal().active_segment_id() > 1,
        "test setup must rotate the log"
    );

    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::checkpoint_with_options(
            MaintenanceCheckpointOptions::new(Some(99), true),
        ))
        .expect("proof checkpoint enqueue");
    let maintenance = runtime
        .run_next_checkpoint_maintenance()
        .expect("proof checkpoint runner")
        .expect("proof checkpoint outcome");

    assert_eq!(maintenance.status(), MaintenanceOutcomeStatus::Completed);
    assert!(backend.delete_calls() > 0);
}
