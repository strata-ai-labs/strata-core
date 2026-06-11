use super::*;
use crate::backend::memory::MemoryBackend;
use crate::backend::{
    Backend, BackendAppend, BackendCapabilities, BackendError, BackendErrorKind, BackendFence,
    BackendMetadata, BackendRange, BackendResult, BackendWriterGuard, PublishError, PublishMode,
    PublishOutcome, PublishResult, CACHE_MODE_REQUIREMENTS,
};
use crate::branch::config::BranchRuntimeConfig;
use crate::commit::{
    CommitBatch, CommitBatchOptions, CommitBranchGeneration, CommitBranchGenerationGuard,
    CommitDurabilityMode, CommitExpiry, CommitManualTimestampSource, CommitMutation,
    CommitObservedVersion, CommitOrigin, CommitReadFact, CommitRetentionHint, CommitRuntimeConfig,
    CommitRuntimeError, CommitTimestampPolicy, CommitValidationFacts,
};
use crate::object::{ObjectName, ObjectPrefix};
use crate::row::{PhysicalKey, StorageSpaceId};
use std::sync::atomic::{AtomicUsize, Ordering};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[test]
fn cache_open_builds_volatile_branch_commit_baseline_without_recovery_claims() {
    let branch = branch_id(0x44);
    let backend = MemoryBackend::new();
    let runtime = open_runtime(branch, &backend);

    assert_eq!(runtime.state(), LifecycleState::Open);
    assert_eq!(runtime.open_plan().storage_mode(), StorageMode::Cache);
    assert_eq!(runtime.open_outcome().mode(), StorageMode::Cache);
    assert_eq!(
        runtime.open_outcome().disposition(),
        StorageOpenDisposition::Created
    );
    assert_eq!(runtime.open_outcome().recovered_visible_version(), None);
    assert!(runtime.open_outcome().recovery_health().is_healthy());
    assert!(runtime.open_outcome().maintenance_ready());
    assert_eq!(
        runtime.open_outcome().backend_capabilities(),
        Some(backend.capabilities())
    );
    assert_eq!(runtime.open_outcome().stats().open_attempts(), 1);
    assert!(runtime.open_outcome().checkpoint().is_none());
    assert!(runtime.open_outcome().bootstrap().is_none());
    assert_eq!(
        runtime.capability_outcome().storage_mode(),
        StorageMode::Cache
    );
    assert_eq!(runtime.visible_version(), CommitVersion::ZERO);
    assert_eq!(runtime.branch_state().branch_id(), branch);
    assert!(runtime.branch_state().is_empty());
    assert_eq!(
        runtime.unresolved_durable().expect("gate state"),
        None,
        "cache open starts with no unresolved durable gate fact"
    );
}

#[test]
fn cache_open_reports_maintenance_ready_after_executor_attached() {
    let branch = branch_id(0x4f);
    let backend = MemoryBackend::new();
    let runtime = open_runtime(branch, &backend);

    assert!(runtime.open_outcome().maintenance_ready());
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    assert_eq!(
        runtime.maintenance_status().stats(),
        LifecycleMaintenanceStats::default()
    );
}

#[test]
fn cache_runtime_can_enqueue_and_run_health_collection_maintenance() {
    let branch = branch_id(0x4e);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    let enqueue = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::health_collection())
        .expect("enqueue health collection");
    assert!(enqueue.was_enqueued());
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);

    let mut runner = MaintenanceTestRunner;
    let outcome = runtime
        .run_next_maintenance(&mut runner)
        .expect("run maintenance")
        .expect("task outcome");

    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(outcome.task_kind(), MaintenanceTaskKind::HealthCollection);
    assert!(outcome.task_id().is_some());
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    assert_eq!(runtime.maintenance_status().stats().completed(), 1);
}

#[test]
fn cache_open_rejects_non_cache_plan_before_backend_preflight() {
    let backend = CountingBackend::new(BackendCapabilities::from_slice(CACHE_MODE_REQUIREMENTS));
    assert!(LifecycleCacheOpenRequest::new(
        open_plan(StorageMode::Cache),
        branch_id(0x45),
        CommitBranchGeneration::new(1).expect("generation"),
    )
    .is_ok());
    for mode in [
        StorageMode::DurableLocalStandard,
        StorageMode::DurableLocalAlways,
        StorageMode::ObjectDurableCandidate,
    ] {
        assert_eq!(
            LifecycleCacheOpenRequest::new(
                open_plan(mode),
                branch_id(0x45),
                CommitBranchGeneration::new(1).expect("generation"),
            ),
            Err(LifecycleError::InvalidOpenPlan {
                reason: "cache lifecycle runtime requires cache storage mode",
            })
        );
    }
    assert!(CommitBranchGeneration::new(0).is_err());
    assert_eq!(backend.capability_calls(), 0);
    assert_eq!(backend.other_calls(), 0);
}

#[test]
fn cache_open_request_validation_rejects_invalid_plan_shapes() {
    assert_eq!(
        LifecycleConfig::new(
            0,
            1,
            LifecycleCloseTimeoutPolicy::ReturnTypedTimeout,
            LifecycleLossyRecoveryPolicy::Disabled,
        ),
        Err(LifecycleError::InvalidConfig {
            field: "max_maintenance_queue_depth",
            reason: "must be nonzero",
        })
    );
    assert_eq!(
        StorageOpenPlan::new(
            StorageMode::Cache,
            LifecycleCodecId::identity(),
            RecoveryStrictness::AllowExplicitLossyFallback,
            LifecycleConfig::new(
                1,
                1,
                LifecycleCloseTimeoutPolicy::ReturnTypedTimeout,
                LifecycleLossyRecoveryPolicy::ExplicitlyAllowed,
            )
            .expect("valid lossy-enabled config"),
        ),
        Err(LifecycleError::InvalidOpenPlan {
            reason: "cache mode cannot request durable recovery fallback",
        })
    );
}

#[test]
fn cache_open_runs_capability_preflight_without_backend_side_effects() {
    let backend = CountingBackend::new(BackendCapabilities::from_slice(CACHE_MODE_REQUIREMENTS));
    let _runtime = open_runtime(branch_id(0x46), &backend);

    assert_eq!(backend.capability_calls(), 1);
    assert_eq!(
        backend.other_calls(),
        0,
        "cache open must not read, list, write, publish, sync, or lock backend objects"
    );

    let rejected = CountingBackend::new(BackendCapabilities::empty());
    assert!(LifecycleCacheRuntime::open(
        request(branch_id(0x47)),
        &rejected,
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        CommitManualTimestampSource::new(Timestamp::from_micros(1_000)),
    )
    .is_err());
    assert_eq!(rejected.capability_calls(), 1);
    assert_eq!(rejected.other_calls(), 0);
}

#[test]
fn cache_runtime_executes_cache_commit_and_reads_through_branch_state() {
    let branch = branch_id(0x48);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    let key = physical_key(branch, b"alpha");

    let outcome = runtime
        .execute_cache_commit(
            put_batch(
                branch,
                key.clone(),
                b"value".to_vec(),
                Timestamp::from_micros(1_234),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("cache commit");

    assert_eq!(outcome.branch_id(), branch);
    assert_eq!(outcome.commit_version(), Some(CommitVersion::new(1)));
    assert_eq!(
        outcome.durability(),
        crate::commit::CommitDurabilityClass::NotDurable
    );
    assert_eq!(outcome.mutation_counts().puts(), 1);
    assert_eq!(outcome.mutation_counts().timeline_rows(), 2);
    assert_eq!(runtime.visible_version(), CommitVersion::new(1));
    assert_eq!(
        runtime.branch_state().max_commit_version(),
        Some(CommitVersion::new(1))
    );

    let read_view = runtime.read_view().expect("read view");
    let visible = read_view
        .latest(&key)
        .expect("latest read")
        .expect("visible row");
    assert_eq!(visible.row().value(), b"value");
    assert_eq!(
        visible.row().commit_timestamp(),
        Timestamp::from_micros(1_234)
    );
}

#[test]
fn cache_commit_schedules_flush_when_post_commit_pressure_suggests_it() {
    let branch = branch_id(0x68);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"flush-pressure-a"),
                b"value-a".to_vec(),
                Timestamp::from_micros(1_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("first cache commit");
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);

    runtime
        .rotate_active_for_maintenance()
        .expect("rotate active table");
    assert!(runtime.storage_pressure().suggested_task().is_some());

    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"flush-pressure-b"),
                b"value-b".to_vec(),
                Timestamp::from_micros(2_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("second cache commit");

    let status = runtime.maintenance_status();
    assert_eq!(status.pending_tasks(), 1);
    assert_eq!(status.stats().enqueued(), 1);

    let outcome = runtime
        .run_next_flush_maintenance()
        .expect("run flush maintenance")
        .expect("flush task");
    assert_eq!(outcome.task_kind(), MaintenanceTaskKind::Flush);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
}

#[test]
fn cache_commit_coalesces_repeated_post_commit_flush_suggestions() {
    let branch = branch_id(0x69);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"coalesce-a"),
                b"value-a".to_vec(),
                Timestamp::from_micros(1_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("first cache commit");
    runtime
        .rotate_active_for_maintenance()
        .expect("rotate active table");

    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"coalesce-b"),
                b"value-b".to_vec(),
                Timestamp::from_micros(2_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("second cache commit");
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"coalesce-c"),
                b"value-c".to_vec(),
                Timestamp::from_micros(3_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("third cache commit");

    let status = runtime.maintenance_status();
    assert_eq!(status.pending_tasks(), 1);
    assert_eq!(status.stats().enqueued(), 1);
    assert_eq!(status.stats().coalesced(), 1);
}

#[test]
fn cache_coalesced_flush_task_drains_all_currently_frozen_tables() {
    let branch = branch_id(0x79);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    commit_cache_put(&mut runtime, branch, b"drain-coalesce-a", 1_000);
    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate first frozen table");
    commit_cache_put(&mut runtime, branch, b"drain-coalesce-b", 2_000);
    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate second frozen table");
    commit_cache_put(&mut runtime, branch, b"drain-coalesce-c", 3_000);
    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate third frozen table");
    commit_cache_put(&mut runtime, branch, b"drain-coalesce-d", 4_000);

    let status = runtime.maintenance_status();
    assert_eq!(status.pending_tasks(), 1);
    assert_eq!(status.stats().enqueued(), 1);
    assert_eq!(status.stats().coalesced(), 2);
    assert_eq!(runtime.branch_state().frozen_table_count(), 3);

    let flush = runtime
        .run_next_flush_maintenance()
        .expect("run flush maintenance")
        .expect("flush task");

    assert_eq!(flush.task_kind(), MaintenanceTaskKind::Flush);
    assert_eq!(flush.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(flush.stats().maintenance_tasks(), 3);
    assert_eq!(runtime.branch_state().frozen_table_count(), 0);
    assert_eq!(runtime.branch_state().owned_levels()[0].len(), 3);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
}

#[test]
fn cache_post_commit_flush_coalescing_is_branch_scoped() {
    let branch_a = branch_id(0x6e);
    let branch_b = branch_id(0x6f);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch_a, &backend);
    runtime
        .create_branch(
            branch_b,
            CommitBranchGeneration::new(1).expect("generation"),
            None,
        )
        .expect("create second branch");

    commit_cache_put(&mut runtime, branch_a, b"branch-a-seed", 1_000);
    runtime
        .rotate_active_for_branch_for_maintenance(branch_a)
        .expect("rotate branch-a active table");
    commit_cache_put(&mut runtime, branch_a, b"branch-a-trigger", 2_000);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);

    commit_cache_put(&mut runtime, branch_b, b"branch-b-seed", 3_000);
    runtime
        .rotate_active_for_branch_for_maintenance(branch_b)
        .expect("rotate branch-b active table");
    commit_cache_put(&mut runtime, branch_b, b"branch-b-trigger", 4_000);
    assert_eq!(
        runtime.maintenance_status().pending_tasks(),
        2,
        "same flush kind but different branch scopes must not coalesce"
    );

    commit_cache_put(&mut runtime, branch_a, b"branch-a-coalesce", 5_000);
    let status = runtime.maintenance_status();
    assert_eq!(status.pending_tasks(), 2);
    assert_eq!(status.stats().enqueued(), 2);
    assert_eq!(status.stats().coalesced(), 1);
}

#[test]
fn cache_global_flush_task_drains_branches_in_deterministic_order() {
    let branch_high = branch_id(0x62);
    let branch_low = branch_id(0x61);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch_high, &backend);
    runtime
        .create_branch(
            branch_low,
            CommitBranchGeneration::new(1).expect("generation"),
            None,
        )
        .expect("create second branch");

    commit_cache_put(&mut runtime, branch_high, b"global-flush-high", 1_000);
    runtime
        .rotate_active_for_branch_for_maintenance(branch_high)
        .expect("rotate high branch active table");
    commit_cache_put(&mut runtime, branch_low, b"global-flush-low", 2_000);
    runtime
        .rotate_active_for_branch_for_maintenance(branch_low)
        .expect("rotate low branch active table");

    let drain_order = runtime
        .branch_catalog()
        .list_branches(false)
        .into_iter()
        .map(LifecycleBranchDescriptor::branch_id)
        .collect::<Vec<_>>();
    assert_eq!(drain_order, vec![branch_low, branch_high]);

    runtime
        .enqueue_maintenance(
            MaintenanceTaskRequest::new(
                MaintenanceTaskKind::Flush,
                MaintenanceTaskPriority::Normal,
                MaintenanceTaskScope::Global,
                MaintenanceTaskPolicy::coalescing(),
            )
            .expect("global flush task"),
        )
        .expect("enqueue global flush");

    let flush = runtime
        .run_next_flush_maintenance()
        .expect("run global flush")
        .expect("flush task");

    assert_eq!(flush.task_kind(), MaintenanceTaskKind::Flush);
    assert_eq!(flush.task_scope(), Some(MaintenanceTaskScope::Global));
    assert_eq!(flush.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(flush.stats().maintenance_tasks(), 2);
    assert_eq!(
        runtime
            .branch_catalog()
            .branch_state(branch_high)
            .expect("high branch")
            .frozen_table_count(),
        0
    );
    assert_eq!(
        runtime
            .branch_catalog()
            .branch_state(branch_low)
            .expect("low branch")
            .frozen_table_count(),
        0
    );
    assert_eq!(
        runtime
            .branch_catalog()
            .branch_state(branch_high)
            .expect("high branch")
            .owned_levels()[0]
            .len(),
        1
    );
    assert_eq!(
        runtime
            .branch_catalog()
            .branch_state(branch_low)
            .expect("low branch")
            .owned_levels()[0]
            .len(),
        1
    );
}

#[test]
fn cache_post_commit_schedules_flush_before_compaction_when_both_are_needed() {
    let branch = branch_id(0x70);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    build_l0_tables_with_scheduled_flushes(&mut runtime, branch, 4);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);

    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate active row into frozen state");
    commit_cache_put(
        &mut runtime,
        branch,
        b"flush-before-compaction-trigger",
        20_000,
    );

    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
    let flush = runtime
        .run_next_flush_maintenance()
        .expect("run flush maintenance")
        .expect("flush task");
    assert_eq!(flush.task_kind(), MaintenanceTaskKind::Flush);
    assert_eq!(flush.status(), MaintenanceOutcomeStatus::Completed);
}

#[test]
fn cache_post_commit_flush_preempts_blocking_l0_compaction_pressure() {
    let branch = branch_id(0x75);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    build_l0_tables_with_scheduled_flushes(&mut runtime, branch, 16);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);

    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate active row into frozen state");
    commit_cache_put(
        &mut runtime,
        branch,
        b"flush-before-blocking-compaction-trigger",
        30_000,
    );

    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
    assert!(
        runtime
            .run_next_compaction_maintenance()
            .expect("compaction lookup")
            .is_none(),
        "flushable state must be queued before blocking L0 compaction"
    );
    let flush = runtime
        .run_next_flush_maintenance()
        .expect("run flush maintenance")
        .expect("flush task");
    assert_eq!(flush.task_kind(), MaintenanceTaskKind::Flush);
    assert_eq!(flush.status(), MaintenanceOutcomeStatus::Completed);
}

#[test]
fn cache_post_commit_schedules_l0_compaction_after_flushable_state_is_drained() {
    let branch = branch_id(0x71);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    build_l0_tables_with_scheduled_flushes(&mut runtime, branch, 4);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);

    commit_cache_put(&mut runtime, branch, b"l0-compaction-trigger", 20_000);

    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
    let compaction = runtime
        .run_next_compaction_maintenance()
        .expect("run compaction maintenance")
        .expect("compaction task");
    assert_eq!(compaction.task_kind(), MaintenanceTaskKind::Compaction);
    assert_eq!(compaction.status(), MaintenanceOutcomeStatus::Completed);
}

#[test]
fn cache_commit_deterministic_inline_runs_suggested_flush() {
    let branch = branch_id(0x76);
    let backend = MemoryBackend::new();
    let config = LifecycleConfig::default()
        .with_maintenance_scheduling_policy(
            LifecycleMaintenanceSchedulingPolicy::DeterministicInline,
        )
        .expect("inline maintenance scheduling");
    let mut runtime = open_runtime_with_config(branch, &backend, config);

    commit_cache_put(&mut runtime, branch, b"inline-flush-a", 1_000);
    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate active table");
    commit_cache_put(&mut runtime, branch, b"inline-flush-b", 2_000);

    let status = runtime.maintenance_status();
    assert_eq!(status.pending_tasks(), 0);
    assert_eq!(status.stats().enqueued(), 1);
    assert_eq!(status.stats().started(), 1);
    assert_eq!(status.stats().completed(), 1);
    assert!(runtime.storage_pressure().suggested_task().is_none());
}

#[test]
fn cache_post_commit_schedules_materialization_for_inherited_branch() {
    let parent = branch_id(0x72);
    let child = branch_id(0x73);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(parent, &backend);

    commit_cache_put(&mut runtime, parent, b"parent-seed", 1_000);
    runtime
        .rotate_active_for_branch_for_maintenance(parent)
        .expect("rotate parent active table");
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::flush(parent))
        .expect("enqueue parent flush");
    run_queued_flush(&mut runtime);
    runtime
        .fork_current(
            parent,
            child,
            CommitBranchGeneration::new(1).expect("generation"),
        )
        .expect("fork child from flushed parent");

    commit_cache_put(&mut runtime, child, b"child-trigger", 2_000);

    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
    let materialization = runtime
        .run_next_materialization_maintenance()
        .expect("run materialization maintenance")
        .expect("materialization task");
    assert_eq!(
        materialization.task_kind(),
        MaintenanceTaskKind::Materialization
    );
    assert_eq!(
        materialization.status(),
        MaintenanceOutcomeStatus::Completed
    );
}

#[test]
fn cache_commit_respects_disabled_post_commit_maintenance_scheduling() {
    let branch = branch_id(0x6a);
    let backend = MemoryBackend::new();
    let config = LifecycleConfig::default()
        .with_maintenance_scheduling_policy(LifecycleMaintenanceSchedulingPolicy::Disabled)
        .expect("disable maintenance scheduling");
    let mut runtime = open_runtime_with_config(branch, &backend, config);

    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"disabled-a"),
                b"value-a".to_vec(),
                Timestamp::from_micros(1_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("first cache commit");
    runtime
        .rotate_active_for_maintenance()
        .expect("rotate active table");
    assert!(runtime.storage_pressure().suggested_task().is_some());

    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"disabled-b"),
                b"value-b".to_vec(),
                Timestamp::from_micros(2_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("second cache commit");

    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    assert_eq!(runtime.maintenance_status().stats().enqueued(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_post_commit_maintenance_perf_trace_counts_suggestions_and_coalescing() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x6b);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"perf-a"),
                b"value-a".to_vec(),
                Timestamp::from_micros(1_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("first cache commit");
    runtime
        .rotate_active_for_maintenance()
        .expect("rotate active table");
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"perf-b"),
                b"value-b".to_vec(),
                Timestamp::from_micros(2_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("second cache commit");
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"perf-c"),
                b"value-c".to_vec(),
                Timestamp::from_micros(3_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("third cache commit");

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_post_commit_maintenance_evaluations(), 3);
    assert_eq!(perf.lifecycle_post_commit_maintenance_no_task(), 1);
    assert_eq!(perf.lifecycle_post_commit_maintenance_tasks_suggested(), 2);
    assert_eq!(perf.lifecycle_post_commit_maintenance_tasks_enqueued(), 1);
    assert_eq!(perf.lifecycle_post_commit_maintenance_tasks_coalesced(), 1);
    assert_eq!(perf.lifecycle_post_commit_maintenance_tasks_deferred(), 0);
    assert_eq!(perf.lifecycle_post_commit_maintenance_disabled(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_flush_drain_perf_trace_counts_discovered_completed_and_remaining_tables() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x7a);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    commit_cache_put(&mut runtime, branch, b"drain-perf-a", 1_000);
    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate first frozen table");
    commit_cache_put(&mut runtime, branch, b"drain-perf-b", 2_000);
    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate second frozen table");
    commit_cache_put(&mut runtime, branch, b"drain-perf-c", 3_000);
    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate third frozen table");
    commit_cache_put(&mut runtime, branch, b"drain-perf-d", 4_000);
    crate::observability::perf_trace::reset();

    let flush = runtime
        .run_next_flush_maintenance()
        .expect("run flush maintenance")
        .expect("flush task");
    assert_eq!(flush.status(), MaintenanceOutcomeStatus::Completed);

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_flush_drain_frozen_tables_discovered(), 3);
    assert_eq!(perf.lifecycle_flush_drain_operations_completed(), 3);
    assert_eq!(perf.lifecycle_flush_drain_freeze_retries(), 0);
    assert_eq!(perf.lifecycle_flush_drain_failures(), 0);
    assert_eq!(perf.lifecycle_flush_drain_post_drain_frozen_tables(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_post_commit_maintenance_perf_trace_counts_disabled_and_skips_read_only() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x6d);
    let backend = MemoryBackend::new();
    let config = LifecycleConfig::default()
        .with_maintenance_scheduling_policy(LifecycleMaintenanceSchedulingPolicy::Disabled)
        .expect("disable maintenance scheduling");
    let mut runtime = open_runtime_with_config(branch, &backend, config);

    assert!(
        runtime
            .execute_cache_commit(
                read_only_batch(branch, physical_key(branch, b"diagnostic")),
                CommitBranchGenerationGuard::exact(
                    CommitBranchGeneration::new(1).expect("generation")
                ),
            )
            .is_err(),
        "read-only diagnostics are rejected before lifecycle scheduling"
    );
    assert_eq!(
        crate::observability::perf_trace::snapshot()
            .lifecycle_post_commit_maintenance_evaluations(),
        0
    );

    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"disabled-perf-a"),
                b"value-a".to_vec(),
                Timestamp::from_micros(1_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("first cache commit");
    runtime
        .rotate_active_for_maintenance()
        .expect("rotate active table");
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"disabled-perf-b"),
                b"value-b".to_vec(),
                Timestamp::from_micros(2_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("second cache commit");

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_post_commit_maintenance_evaluations(), 2);
    assert_eq!(perf.lifecycle_post_commit_maintenance_disabled(), 2);
    assert_eq!(perf.lifecycle_post_commit_maintenance_no_task(), 0);
    assert_eq!(perf.lifecycle_post_commit_maintenance_tasks_suggested(), 0);
    assert_eq!(perf.lifecycle_post_commit_maintenance_tasks_enqueued(), 0);
    assert_eq!(perf.lifecycle_post_commit_maintenance_tasks_coalesced(), 0);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_post_commit_queue_full_records_deferred_without_failing_commit() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x74);
    let backend = MemoryBackend::new();
    let config = LifecycleConfig::new(
        1,
        64,
        LifecycleCloseTimeoutPolicy::ReturnTypedTimeout,
        LifecycleLossyRecoveryPolicy::Disabled,
    )
    .expect("single-slot maintenance queue config");
    let mut runtime = open_runtime_with_config(branch, &backend, config);

    commit_cache_put(&mut runtime, branch, b"queue-full-seed", 1_000);
    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate active table");
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::health_collection())
        .expect("fill maintenance queue");
    crate::observability::perf_trace::reset();

    commit_cache_put(&mut runtime, branch, b"queue-full-visible", 2_000);

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_post_commit_maintenance_evaluations(), 1);
    assert_eq!(perf.lifecycle_post_commit_maintenance_tasks_suggested(), 1);
    assert_eq!(perf.lifecycle_post_commit_maintenance_tasks_enqueued(), 0);
    assert_eq!(perf.lifecycle_post_commit_maintenance_tasks_coalesced(), 0);
    assert_eq!(perf.lifecycle_post_commit_maintenance_tasks_deferred(), 1);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);

    let key = physical_key(branch, b"queue-full-visible");
    let visible = runtime
        .read_view()
        .expect("read view")
        .latest(&key)
        .expect("latest read")
        .expect("visible row");
    assert_eq!(visible.row().value(), b"queue-full-visible");
}

#[test]
fn cache_commit_rejects_blocking_table_pressure_before_allocating_version() {
    let branch = branch_id(0x77);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    build_l0_tables_with_scheduled_flushes(&mut runtime, branch, 16);
    assert_eq!(runtime.visible_version(), CommitVersion::new(17));
    assert_eq!(
        runtime.storage_pressure().severity(),
        LifecycleStoragePressureSeverity::BlockMutatingAdmission
    );

    let error = runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"blocked-by-table-pressure"),
                b"blocked".to_vec(),
                Timestamp::from_micros(40_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect_err("blocking pressure rejects commit");

    assert!(matches!(
        error,
        LifecycleError::StoragePressureRejected {
            branch_id: rejected_branch,
            severity: LifecycleStoragePressureSeverity::BlockMutatingAdmission,
            pressure_reason: LifecycleStoragePressureReason::LevelZeroTableBacklog,
            retryable: true,
            ..
        } if rejected_branch == branch
    ));
    assert_eq!(runtime.visible_version(), CommitVersion::new(17));
    let key = physical_key(branch, b"blocked-by-table-pressure");
    assert!(
        runtime
            .read_view()
            .expect("read view")
            .latest(&key)
            .expect("latest read")
            .is_none(),
        "rejected admission must not append rows"
    );
}

#[test]
fn cache_commit_stale_generation_takes_precedence_over_blocking_pressure() {
    let branch = branch_id(0x7c);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    build_l0_tables_with_scheduled_flushes(&mut runtime, branch, 16);
    assert_eq!(
        runtime.storage_pressure().severity(),
        LifecycleStoragePressureSeverity::BlockMutatingAdmission
    );

    let error = runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"stale-generation-under-pressure"),
                b"blocked".to_vec(),
                Timestamp::from_micros(40_100),
            ),
            CommitBranchGenerationGuard::exact(
                CommitBranchGeneration::new(99).expect("generation"),
            ),
        )
        .expect_err("stale generation rejects before pressure policy");

    assert!(matches!(
        error,
        LifecycleError::BranchGenerationMismatch {
            branch_id: rejected_branch,
            expected: 1,
            actual: 99,
        } if rejected_branch == branch
    ));
    assert_eq!(runtime.last_write_admission(), None);
    assert_eq!(runtime.visible_version(), CommitVersion::new(17));
    assert!(
        runtime
            .read_view()
            .expect("read view")
            .latest(&physical_key(branch, b"stale-generation-under-pressure"))
            .expect("latest read")
            .is_none(),
        "rejected stale-generation commit must not append rows"
    );
}

#[test]
fn cache_commit_wrong_mode_takes_precedence_over_blocking_pressure() {
    let branch = branch_id(0x7d);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    build_l0_tables_with_scheduled_flushes(&mut runtime, branch, 16);
    assert_eq!(
        runtime.storage_pressure().severity(),
        LifecycleStoragePressureSeverity::BlockMutatingAdmission
    );

    let error = runtime
        .execute_cache_commit(
            put_batch_with_durability(
                branch,
                physical_key(branch, b"wrong-mode-under-pressure"),
                b"blocked".to_vec(),
                Timestamp::from_micros(40_200),
                CommitDurabilityMode::Standard,
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect_err("wrong durability rejects before pressure policy");

    assert_commit_runtime_error(&error);
    assert_eq!(runtime.last_write_admission(), None);
    assert_eq!(runtime.visible_version(), CommitVersion::new(17));
    assert!(
        runtime
            .read_view()
            .expect("read view")
            .latest(&physical_key(branch, b"wrong-mode-under-pressure"))
            .expect("latest read")
            .is_none(),
        "rejected wrong-mode commit must not append rows"
    );
}

#[test]
fn cache_commit_background_l0_pressure_records_clean_admission() {
    let branch = branch_id(0x80);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    build_l0_tables_with_scheduled_flushes(&mut runtime, branch, 4);
    assert_eq!(
        runtime.storage_pressure().severity(),
        LifecycleStoragePressureSeverity::Background
    );

    let outcome = runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"background-admission"),
                b"value".to_vec(),
                Timestamp::from_micros(40_300),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("background pressure admits commit");

    assert_eq!(outcome.commit_version(), Some(CommitVersion::new(6)));
    let admission = runtime
        .last_write_admission()
        .expect("background admission facts");
    assert_eq!(
        admission.status(),
        LifecycleWriteAdmissionStatus::AcceptedClean
    );
    assert_eq!(
        admission.pressure().severity(),
        LifecycleStoragePressureSeverity::Background
    );
    assert_eq!(
        admission.pressure().reason(),
        LifecycleStoragePressureReason::LevelZeroTableBacklog
    );
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
}

#[test]
fn cache_commit_urgent_l0_pressure_records_under_pressure_admission() {
    let branch = branch_id(0x81);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    build_l0_tables_with_scheduled_flushes(&mut runtime, branch, 8);
    assert_eq!(
        runtime.storage_pressure().severity(),
        LifecycleStoragePressureSeverity::Urgent
    );

    let outcome = runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"urgent-admission"),
                b"value".to_vec(),
                Timestamp::from_micros(40_400),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("urgent pressure admits commit with pressure facts");

    assert_eq!(outcome.commit_version(), Some(CommitVersion::new(10)));
    let admission = runtime
        .last_write_admission()
        .expect("urgent admission facts");
    assert_eq!(
        admission.status(),
        LifecycleWriteAdmissionStatus::AcceptedUnderPressure
    );
    assert_eq!(
        admission.pressure().severity(),
        LifecycleStoragePressureSeverity::Urgent
    );
    assert_eq!(
        admission.pressure().reason(),
        LifecycleStoragePressureReason::LevelZeroTableBacklog
    );
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
}

#[test]
fn cache_commit_retry_after_pressure_maintenance_succeeds() {
    let branch = branch_id(0x78);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    build_l0_tables_with_scheduled_flushes(&mut runtime, branch, 16);
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"retry-after-pressure"),
                b"blocked".to_vec(),
                Timestamp::from_micros(40_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect_err("blocking pressure rejects first attempt");
    assert_eq!(runtime.last_write_admission(), None);

    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 0))
        .expect("enqueue compaction");
    runtime
        .run_next_compaction_maintenance()
        .expect("run compaction")
        .expect("compaction outcome");
    assert_ne!(
        runtime.storage_pressure().severity(),
        LifecycleStoragePressureSeverity::BlockMutatingAdmission
    );

    let outcome = runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"retry-after-pressure"),
                b"visible".to_vec(),
                Timestamp::from_micros(41_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("retry after maintenance succeeds");

    assert_eq!(outcome.commit_version(), Some(CommitVersion::new(18)));
    let admission = runtime
        .last_write_admission()
        .expect("retry records accepted admission facts");
    assert_ne!(
        admission.pressure().severity(),
        LifecycleStoragePressureSeverity::BlockMutatingAdmission
    );
    assert!(
        admission.cleared_prior_rejection(),
        "retry after maintenance should record the cleared pressure rejection"
    );
    let visible = runtime
        .read_view()
        .expect("read view")
        .latest(&physical_key(branch, b"retry-after-pressure"))
        .expect("latest read")
        .expect("visible row");
    assert_eq!(visible.row().value(), b"visible");
}

#[test]
fn cache_commit_branch_guard_rejection_remains_distinct_from_pressure_rejection() {
    let branch = branch_id(0x7a);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    let guard = runtime
        .guard_set()
        .try_acquire_branch_guard(branch)
        .expect("active branch guard");

    let error = runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"guard-still-distinct"),
                b"value".to_vec(),
                Timestamp::from_micros(1_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect_err("active guard rejects commit");

    assert!(matches!(
        error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::CommitRuntime,
            ..
        }
    ));
    assert_eq!(error.code(), "failed_precondition.lifecycle.commit_runtime");
    drop(guard);
}

#[test]
fn cache_commit_branch_guard_rejection_takes_precedence_over_blocking_pressure() {
    let branch = branch_id(0x84);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    build_l0_tables_with_scheduled_flushes(&mut runtime, branch, 16);
    assert_eq!(
        runtime.storage_pressure().severity(),
        LifecycleStoragePressureSeverity::BlockMutatingAdmission
    );
    let guard = runtime
        .guard_set()
        .try_acquire_branch_guard(branch)
        .expect("active branch guard");

    let error = runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"guard-before-pressure"),
                b"value".to_vec(),
                Timestamp::from_micros(50_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect_err("active guard rejects before pressure policy");

    assert!(matches!(
        commit_runtime_source(&error),
        CommitRuntimeError::BranchGuardUnavailable {
            branch_id: rejected_branch,
            ..
        } if *rejected_branch == branch
    ));
    assert_eq!(runtime.last_write_admission(), None);
    drop(guard);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_branch_guard_rejection_under_pressure_keeps_pressure_counters_separate() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x85);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    build_l0_tables_with_scheduled_flushes(&mut runtime, branch, 16);
    let guard = runtime
        .guard_set()
        .try_acquire_branch_guard(branch)
        .expect("active branch guard");
    crate::observability::perf_trace::reset();

    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"guard-pressure-counters"),
                b"value".to_vec(),
                Timestamp::from_micros(50_100),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect_err("active guard rejects before pressure policy");

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.commit_branch_guard_attempts(), 1);
    assert_eq!(perf.commit_branch_guard_acquired(), 0);
    assert_eq!(perf.commit_branch_guard_rejected(), 1);
    assert_eq!(perf.lifecycle_write_admission_evaluations(), 0);
    assert_eq!(perf.lifecycle_write_admission_pressure_rejects(), 0);
    drop(guard);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_write_admission_perf_trace_records_pressure_policy() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x7b);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    crate::observability::perf_trace::reset();
    commit_cache_put(&mut runtime, branch, b"clean-admission", 1_000);
    let clean = crate::observability::perf_trace::snapshot();
    assert_eq!(clean.lifecycle_write_admission_evaluations(), 1);
    assert_eq!(clean.lifecycle_write_admission_clean_accepts(), 1);
    assert_eq!(clean.lifecycle_write_admission_under_pressure_accepts(), 0);
    assert_eq!(clean.lifecycle_write_admission_pressure_rejects(), 0);

    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate active");
    crate::observability::perf_trace::reset();
    commit_cache_put(&mut runtime, branch, b"urgent-admission", 2_000);
    let admission = runtime
        .last_write_admission()
        .expect("under-pressure admission facts");
    assert_eq!(
        admission.status(),
        LifecycleWriteAdmissionStatus::AcceptedUnderPressure
    );
    assert_eq!(
        admission.pressure().reason(),
        LifecycleStoragePressureReason::FrozenBacklog
    );
    assert!(!admission.cleared_prior_rejection());
    let urgent = crate::observability::perf_trace::snapshot();
    assert_eq!(urgent.lifecycle_write_admission_evaluations(), 1);
    assert_eq!(urgent.lifecycle_write_admission_under_pressure_accepts(), 1);
    assert_eq!(urgent.lifecycle_write_admission_pressure_rejects(), 0);
    runtime
        .run_next_flush_maintenance()
        .expect("run scheduled flush")
        .expect("flush outcome");

    build_l0_tables_with_scheduled_flushes_from(&mut runtime, branch, 15, 10_000);
    assert_eq!(
        runtime.storage_pressure().severity(),
        LifecycleStoragePressureSeverity::BlockMutatingAdmission
    );
    crate::observability::perf_trace::reset();
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"pressure-rejected"),
                b"blocked".to_vec(),
                Timestamp::from_micros(50_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect_err("blocking pressure rejects");
    let rejected = crate::observability::perf_trace::snapshot();
    assert_eq!(rejected.lifecycle_write_admission_evaluations(), 1);
    assert_eq!(rejected.lifecycle_write_admission_requires_maintenance(), 1);
    assert_eq!(rejected.lifecycle_write_admission_pressure_rejects(), 1);
    assert_eq!(rejected.lifecycle_write_admission_retryable_rejects(), 1);

    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 0))
        .expect("enqueue compaction");
    runtime
        .run_next_compaction_maintenance()
        .expect("run compaction")
        .expect("compaction outcome");
    crate::observability::perf_trace::reset();
    commit_cache_put(&mut runtime, branch, b"pressure-cleared", 51_000);
    let cleared = crate::observability::perf_trace::snapshot();
    assert_eq!(cleared.lifecycle_write_admission_evaluations(), 1);
    assert_eq!(
        cleared.lifecycle_write_admission_pressure_cleared_retries(),
        1
    );
}

#[test]
fn cache_runtime_generated_timestamp_proves_zero_allocator_and_empty_timestamp_guard() {
    let branch = branch_id(0x4c);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime_with_timestamp(branch, &backend, Timestamp::from_micros(1));
    let key = physical_key(branch, b"generated-timestamp");

    let outcome = runtime
        .execute_cache_commit(
            runtime_generated_put_batch(branch, key.clone(), b"generated".to_vec()),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("runtime-generated cache commit");

    assert_eq!(
        outcome.commit_version(),
        Some(CommitVersion::new(1)),
        "first commit proves the version allocator opened at zero"
    );
    let visible = runtime
        .read_view()
        .expect("read view")
        .latest(&key)
        .expect("read")
        .expect("visible row");
    assert_eq!(
        visible.row().commit_timestamp(),
        Timestamp::from_micros(1),
        "first runtime-generated timestamp proves the timestamp guard opened empty"
    );
}

#[test]
fn cache_runtime_rejects_wrong_mode_batch_and_preserves_state() {
    let branch = branch_id(0x49);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    let key = physical_key(branch, b"durable");

    let before_visible = runtime.visible_version();
    let before_rows = runtime.branch_state().active_row_count();
    for durability in [CommitDurabilityMode::Standard, CommitDurabilityMode::Always] {
        let error = runtime
            .execute_cache_commit(
                put_batch_with_durability(
                    branch,
                    key.clone(),
                    b"value".to_vec(),
                    Timestamp::from_micros(2_000),
                    durability,
                ),
                CommitBranchGenerationGuard::not_supplied(),
            )
            .expect_err("durable batch rejected by cache runtime");

        assert_commit_runtime_error(&error);
        assert_eq!(runtime.visible_version(), before_visible);
        assert_eq!(runtime.branch_state().active_row_count(), before_rows);
    }

    let accepted = runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"after-reject"),
                b"accepted".to_vec(),
                Timestamp::from_micros(2_001),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("valid cache commit after rejection");
    assert_eq!(accepted.commit_version(), Some(CommitVersion::new(1)));
}

#[test]
fn cache_runtime_rejects_read_only_wrong_branch_stale_generation_and_conflict() {
    let branch = branch_id(0x4a);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    let key = physical_key(branch, b"guarded");

    assert_commit_runtime_error(
        &runtime
            .execute_cache_commit(
                read_only_batch(branch, key.clone()),
                CommitBranchGenerationGuard::not_supplied(),
            )
            .expect_err("read-only diagnostic rejected by mutating cache executor"),
    );
    assert_eq!(runtime.visible_version(), CommitVersion::ZERO);

    let other_branch = branch_id(0x4b);
    assert_commit_runtime_error(
        &runtime
            .execute_cache_commit(
                put_batch(
                    other_branch,
                    physical_key(other_branch, b"wrong-branch"),
                    b"value".to_vec(),
                    Timestamp::from_micros(2_100),
                ),
                CommitBranchGenerationGuard::not_supplied(),
            )
            .expect_err("wrong branch rejected"),
    );
    assert_eq!(runtime.visible_version(), CommitVersion::ZERO);

    let stale_generation = runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"stale-generation"),
                b"value".to_vec(),
                Timestamp::from_micros(2_200),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(2).expect("generation")),
        )
        .expect_err("stale generation rejected");
    assert!(matches!(
        stale_generation,
        LifecycleError::BranchGenerationMismatch {
            branch_id: rejected_branch,
            expected: 1,
            actual: 2,
        } if rejected_branch == branch
    ));
    assert_eq!(runtime.visible_version(), CommitVersion::ZERO);

    let first = runtime
        .execute_cache_commit(
            put_batch(
                branch,
                key.clone(),
                b"first".to_vec(),
                Timestamp::from_micros(2_300),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("first commit");
    assert_eq!(first.commit_version(), Some(CommitVersion::new(1)));

    assert_commit_runtime_error(
        &runtime
            .execute_cache_commit(
                put_batch_with_validation(
                    branch,
                    key.clone(),
                    b"conflict".to_vec(),
                    Timestamp::from_micros(2_400),
                    CommitValidationFacts::new(
                        vec![CommitReadFact::new(
                            key.clone(),
                            CommitObservedVersion::Missing,
                        )],
                        Vec::new(),
                    ),
                    crate::commit::CommitConflictValidationMode::Validate,
                ),
                CommitBranchGenerationGuard::not_supplied(),
            )
            .expect_err("stale read fact rejected"),
    );
    assert_eq!(runtime.visible_version(), CommitVersion::new(1));

    let second = runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"after-conflict"),
                b"second".to_vec(),
                Timestamp::from_micros(2_500),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("second accepted commit");
    assert_eq!(second.commit_version(), Some(CommitVersion::new(2)));
}

#[test]
fn cache_close_is_idempotent_blocks_commits_and_reads_and_avoids_backend_calls() {
    let branch = branch_id(0x4a);
    let backend = CountingBackend::new(BackendCapabilities::from_slice(CACHE_MODE_REQUIREMENTS));
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"before-close"),
                b"value".to_vec(),
                Timestamp::from_micros(2_900),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("cache commit before close");

    let close = runtime.close().expect("cache close");
    assert_eq!(close.phase(), ClosePhase::Closed);
    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(close.close_fact(), Some(LifecycleCloseFact::Complete));
    assert!(close.commits_quiesced());
    assert!(close.maintenance_drained());
    assert!(!close.durable_synced());
    assert!(close.guards_released());
    assert!(!close.prior_final());
    assert_eq!(close.stats().close_attempts(), 1);
    assert_eq!(runtime.state(), LifecycleState::Closed);

    let second = runtime.close().expect("idempotent close");
    assert_eq!(second.phase(), ClosePhase::Closed);
    assert_eq!(second.status(), CloseOutcomeStatus::Idempotent);
    assert_eq!(second.close_fact(), Some(LifecycleCloseFact::AlreadyClosed));
    assert!(second.prior_final());
    // Idempotent close must surface the *same* stats the caller observed on
    // the first call — close_attempts, canceled_tasks, etc. — rather than
    // a fabricated baseline. Without this, retry-observability tools see
    // different counts on the second call than the first.
    assert_eq!(second.stats(), close.stats());
    assert_eq!(runtime.state(), LifecycleState::Closed);

    // A third call must still see the same prior stats — repeated
    // idempotent retries do not drift.
    let third = runtime.close().expect("third idempotent close");
    assert_eq!(third.stats(), close.stats());
    assert_eq!(third.status(), CloseOutcomeStatus::Idempotent);

    assert!(matches!(
        runtime.read_view().expect_err("read after close rejected"),
        LifecycleError::InvalidLifecycleState { .. }
    ));
    let key = physical_key(branch, b"closed");
    assert!(matches!(
        runtime
            .execute_cache_commit(
                put_batch(
                    branch,
                    key,
                    b"value".to_vec(),
                    Timestamp::from_micros(3_000)
                ),
                CommitBranchGenerationGuard::not_supplied(),
            )
            .expect_err("commit after close rejected"),
        LifecycleError::InvalidLifecycleState { .. }
    ));
    assert_eq!(backend.capability_calls(), 1);
    assert_eq!(
        backend.other_calls(),
        0,
        "cache commit and close must not touch durable backend methods"
    );
    assert_eq!(runtime.open_outcome().mode(), StorageMode::Cache);
    assert!(runtime.open_outcome().recovery_health().is_healthy());
}

#[test]
fn cache_close_drains_pending_drain_required_maintenance_before_transitioning() {
    // Cache supports its own drain-required path now: queued drain-class
    // tasks are dispatched through the cache close runner so the close
    // path can complete without leaving the runtime stuck on a pending
    // drain task. The task must run to completion before the state
    // transitions to Closed.
    let branch = branch_id(0x50);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .enqueue_maintenance(
            MaintenanceTaskRequest::new(
                MaintenanceTaskKind::HealthCollection,
                MaintenanceTaskPriority::Normal,
                MaintenanceTaskScope::Global,
                MaintenanceTaskPolicy::drain_before_close(),
            )
            .expect("drain-required health task"),
        )
        .expect("enqueue drain-required task");

    let close = runtime
        .close()
        .expect("cache close drains drain-required task");

    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(close.close_fact(), Some(LifecycleCloseFact::Complete));
    assert_eq!(runtime.state(), LifecycleState::Closed);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    // Executor records the drained task in its stats.
    assert_eq!(runtime.maintenance_status().stats().drained(), 1);
}

#[test]
fn cache_close_cancels_cancelable_pending_work() {
    let branch = branch_id(0x51);
    let backend = CountingBackend::new(BackendCapabilities::from_slice(CACHE_MODE_REQUIREMENTS));
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .enqueue_maintenance(
            MaintenanceTaskRequest::new(
                MaintenanceTaskKind::HealthCollection,
                MaintenanceTaskPriority::Normal,
                MaintenanceTaskScope::Global,
                MaintenanceTaskPolicy::cancel_before_close(),
            )
            .expect("cancelable health task"),
        )
        .expect("enqueue cancelable task");

    let close = runtime.close().expect("cache close");

    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(close.stats().maintenance_tasks(), 1);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    assert_eq!(backend.other_calls(), 0);
}

#[test]
fn cache_close_cancels_ordinary_pending_work_before_closed() {
    let branch = branch_id(0x52);
    let backend = CountingBackend::new(BackendCapabilities::from_slice(CACHE_MODE_REQUIREMENTS));
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::health_collection())
        .expect("enqueue ordinary task");

    let close = runtime.close().expect("cache close");

    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(close.close_fact(), Some(LifecycleCloseFact::Complete));
    assert_eq!(close.stats().maintenance_tasks(), 1);
    assert_eq!(runtime.state(), LifecycleState::Closed);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    assert_eq!(
        runtime.run_next_maintenance(&mut MaintenanceTestRunner),
        Err(LifecycleError::InvalidLifecycleState {
            reason: "operation is not admitted in current lifecycle state",
        })
    );
    assert_eq!(
        backend.other_calls(),
        0,
        "cache close must not start durable backend work while canceling maintenance"
    );
}

#[test]
fn cache_close_retry_from_closing_finishes_without_durable_side_effects() {
    let branch = branch_id(0x55);
    let backend = CountingBackend::new(BackendCapabilities::from_slice(CACHE_MODE_REQUIREMENTS));
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::health_collection())
        .expect("enqueue ordinary task");
    runtime
        .force_close_requested_for_test()
        .expect("force closing state");

    let close = runtime.close().expect("retry close");

    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(runtime.state(), LifecycleState::Closed);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    assert_eq!(backend.other_calls(), 0);
}

#[test]
fn cache_close_drains_drain_required_work() {
    // Cache no longer rejects on drain-required work — it dispatches the
    // queued task through the cache close runner. The runtime closes
    // cleanly with the drained task reflected in maintenance stats.
    let branch = branch_id(0x53);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .enqueue_maintenance(
            MaintenanceTaskRequest::new(
                MaintenanceTaskKind::HealthCollection,
                MaintenanceTaskPriority::Normal,
                MaintenanceTaskScope::Global,
                MaintenanceTaskPolicy::drain_before_close(),
            )
            .expect("drain-required task"),
        )
        .expect("enqueue");

    let close = runtime.close().expect("close drains task");

    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(runtime.state(), LifecycleState::Closed);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    assert!(runtime.maintenance_status().stats().drained() >= 1);
}

#[test]
fn cache_close_does_not_start_ordinary_maintenance() {
    let branch = branch_id(0x54);
    let backend = CountingBackend::new(BackendCapabilities::from_slice(CACHE_MODE_REQUIREMENTS));
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::health_collection())
        .expect("enqueue");

    let close = runtime.close().expect("cache close");

    assert_eq!(close.stats().maintenance_tasks(), 1);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    assert_eq!(
        runtime.run_next_maintenance(&mut MaintenanceTestRunner),
        Err(LifecycleError::InvalidLifecycleState {
            reason: "operation is not admitted in current lifecycle state",
        })
    );
    assert_eq!(backend.other_calls(), 0);
}

#[test]
fn cache_close_does_not_call_wal_manifest_snapshot_table_or_quarantine_services() {
    let branch = branch_id(0x55);
    let backend = CountingBackend::new(BackendCapabilities::from_slice(CACHE_MODE_REQUIREMENTS));
    let mut runtime = open_runtime(branch, &backend);

    runtime.close().expect("cache close");

    assert_eq!(backend.capability_calls(), 1);
    assert_eq!(backend.other_calls(), 0);
}

#[test]
fn cache_close_reports_no_durable_sync() {
    let branch = branch_id(0x56);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    let close = runtime.close().expect("cache close");

    assert!(!close.durable_synced());
    assert!(close.guards_released());
}

#[test]
fn cache_close_releases_volatile_guards() {
    let branch = branch_id(0x57);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    let close = runtime.close().expect("cache close");

    assert!(close.commits_quiesced());
    assert!(close.guards_released());
}

#[test]
fn cache_double_close_returns_idempotent_prior_facts() {
    let branch = branch_id(0x58);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    let first = runtime.close().expect("first close");

    let second = runtime.close().expect("second close");

    assert_eq!(second.status(), CloseOutcomeStatus::Idempotent);
    assert_eq!(second.close_fact(), Some(LifecycleCloseFact::AlreadyClosed));
    assert!(second.prior_final());
    // The idempotent retry must surface the same stats the caller
    // observed on the first close. A regression that swaps
    // `idempotent_from_prior_close` back to the fabricated baseline
    // would silently drift here.
    assert_eq!(second.stats(), first.stats());

    // Third call must still report the same prior stats — repeated
    // idempotent retries do not drift.
    let third = runtime.close().expect("third close");
    assert_eq!(third.stats(), first.stats());
    assert_eq!(third.status(), CloseOutcomeStatus::Idempotent);
}

#[test]
fn cache_commit_after_close_rejects_before_allocation() {
    let branch = branch_id(0x59);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    runtime.close().expect("cache close");
    let visible_before = runtime.visible_version();
    let rows_before = runtime.branch_state().active_row_count();

    let error = runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"after-close"),
                b"value".to_vec(),
                Timestamp::from_micros(3_100),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect_err("commit after close");

    assert_eq!(error.code(), "failed_precondition.lifecycle.state");
    assert_eq!(runtime.visible_version(), visible_before);
    assert_eq!(runtime.branch_state().active_row_count(), rows_before);
}

#[test]
fn cache_read_after_close_rejects_as_lifecycle_state() {
    let branch = branch_id(0x5a);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    runtime.close().expect("cache close");

    let error = runtime.read_view().expect_err("read after close");

    assert_eq!(error.code(), "failed_precondition.lifecycle.state");
}

#[test]
fn cache_open_commit_close_reopen_is_empty_and_no_durable_calls() {
    let branch = branch_id(0x5b);
    let backend = CountingBackend::new(BackendCapabilities::from_slice(CACHE_MODE_REQUIREMENTS));
    let key = physical_key(branch, b"volatile-reopen");
    let mut first = open_runtime(branch, &backend);
    first
        .execute_cache_commit(
            put_batch(
                branch,
                key.clone(),
                b"value".to_vec(),
                Timestamp::from_micros(3_200),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("cache commit");
    first.close().expect("close first");

    let second = open_runtime(branch, &backend);

    assert_eq!(second.visible_version(), CommitVersion::ZERO);
    assert!(second.branch_state().is_empty());
    assert!(second
        .read_view()
        .expect("read view")
        .latest(&key)
        .expect("latest")
        .is_none());
    assert_eq!(backend.other_calls(), 0);
}

#[test]
fn cache_close_without_commits_completes_and_preserves_diagnostic_facts() {
    let branch = branch_id(0x4d);
    let backend = CountingBackend::new(BackendCapabilities::from_slice(CACHE_MODE_REQUIREMENTS));
    let mut runtime = open_runtime(branch, &backend);

    let close = runtime.close().expect("cache close without commits");
    assert_eq!(close.phase(), ClosePhase::Closed);
    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(runtime.state(), LifecycleState::Closed);
    assert_eq!(runtime.open_plan().storage_mode(), StorageMode::Cache);
    assert_eq!(runtime.open_outcome().recovered_visible_version(), None);
    assert_eq!(runtime.visible_version(), CommitVersion::ZERO);
    assert_eq!(backend.capability_calls(), 1);
    assert_eq!(backend.other_calls(), 0);
}

#[test]
fn cache_reopen_starts_empty_even_when_prior_runtime_committed_rows() {
    let branch = branch_id(0x4b);
    let backend = MemoryBackend::new();
    let mut first = open_runtime(branch, &backend);
    let key = physical_key(branch, b"volatile");

    first
        .execute_cache_commit(
            put_batch(
                branch,
                key.clone(),
                b"ephemeral".to_vec(),
                Timestamp::from_micros(4_000),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("cache commit");
    assert!(first
        .read_view()
        .expect("view")
        .latest(&key)
        .expect("read")
        .is_some());
    first.close().expect("close first runtime");

    let second = open_runtime(branch, &backend);
    assert_eq!(second.visible_version(), CommitVersion::ZERO);
    assert!(second.branch_state().is_empty());
    assert!(second
        .read_view()
        .expect("second view")
        .latest(&key)
        .expect("read")
        .is_none());
    assert_eq!(second.open_outcome().recovered_visible_version(), None);
}

fn open_runtime(
    branch: BranchId,
    backend: &dyn Backend,
) -> LifecycleCacheRuntime<CommitManualTimestampSource> {
    open_runtime_with_timestamp(branch, backend, Timestamp::from_micros(1_000))
}

fn open_runtime_with_timestamp(
    branch: BranchId,
    backend: &dyn Backend,
    next_timestamp: Timestamp,
) -> LifecycleCacheRuntime<CommitManualTimestampSource> {
    LifecycleCacheRuntime::open(
        request(branch),
        backend,
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        CommitManualTimestampSource::new(next_timestamp),
    )
    .expect("cache runtime opens")
}

fn open_runtime_with_config(
    branch: BranchId,
    backend: &dyn Backend,
    config: LifecycleConfig,
) -> LifecycleCacheRuntime<CommitManualTimestampSource> {
    LifecycleCacheRuntime::open(
        request_with_config(branch, config),
        backend,
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        CommitManualTimestampSource::new(Timestamp::from_micros(1_000)),
    )
    .expect("cache runtime opens")
}

fn request(branch: BranchId) -> LifecycleCacheOpenRequest {
    request_with_config(branch, LifecycleConfig::default())
}

fn request_with_config(branch: BranchId, config: LifecycleConfig) -> LifecycleCacheOpenRequest {
    LifecycleCacheOpenRequest::new(
        open_plan_with_config(StorageMode::Cache, config),
        branch,
        CommitBranchGeneration::new(1).expect("generation"),
    )
    .expect("cache open request")
}

fn open_plan(mode: StorageMode) -> StorageOpenPlan {
    open_plan_with_config(mode, LifecycleConfig::default())
}

fn open_plan_with_config(mode: StorageMode, config: LifecycleConfig) -> StorageOpenPlan {
    StorageOpenPlan::new(
        mode,
        LifecycleCodecId::identity(),
        RecoveryStrictness::Strict,
        config,
    )
    .expect("open plan")
}

fn put_batch(
    branch: BranchId,
    key: PhysicalKey,
    value: Vec<u8>,
    timestamp: Timestamp,
) -> CommitBatch {
    put_batch_with_durability(branch, key, value, timestamp, CommitDurabilityMode::Cache)
}

fn put_batch_with_durability(
    branch: BranchId,
    key: PhysicalKey,
    value: Vec<u8>,
    timestamp: Timestamp,
    durability: CommitDurabilityMode,
) -> CommitBatch {
    put_batch_with_options(
        branch,
        key,
        value,
        timestamp,
        durability,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                physical_key(branch, b"read-fact"),
                CommitObservedVersion::Missing,
            )],
            Vec::new(),
        ),
        crate::commit::CommitConflictValidationMode::Skip,
    )
}

fn put_batch_with_validation(
    branch: BranchId,
    key: PhysicalKey,
    value: Vec<u8>,
    timestamp: Timestamp,
    validation: CommitValidationFacts,
    conflict_validation: crate::commit::CommitConflictValidationMode,
) -> CommitBatch {
    put_batch_with_options(
        branch,
        key,
        value,
        timestamp,
        CommitDurabilityMode::Cache,
        validation,
        conflict_validation,
    )
}

fn put_batch_with_options(
    branch: BranchId,
    key: PhysicalKey,
    value: Vec<u8>,
    timestamp: Timestamp,
    durability: CommitDurabilityMode,
    validation: CommitValidationFacts,
    conflict_validation: crate::commit::CommitConflictValidationMode,
) -> CommitBatch {
    CommitBatch::mutating(
        branch,
        vec![CommitMutation::put(
            key,
            value,
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        validation,
        CommitBatchOptions::new(
            durability,
            conflict_validation,
            crate::commit::CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::Explicit(timestamp),
            CommitOrigin::StorageRuntime,
        ),
    )
}

fn read_only_batch(branch: BranchId, key: PhysicalKey) -> CommitBatch {
    CommitBatch::read_only_diagnostic(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(key, CommitObservedVersion::Missing)],
            Vec::new(),
        ),
        CommitBatchOptions::new(
            CommitDurabilityMode::Cache,
            crate::commit::CommitConflictValidationMode::Validate,
            crate::commit::CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::Explicit(Timestamp::from_micros(2_050)),
            CommitOrigin::Diagnostic,
        ),
    )
}

fn runtime_generated_put_batch(branch: BranchId, key: PhysicalKey, value: Vec<u8>) -> CommitBatch {
    CommitBatch::mutating(
        branch,
        vec![CommitMutation::put(
            key,
            value,
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::new(
            CommitDurabilityMode::Cache,
            crate::commit::CommitConflictValidationMode::Skip,
            crate::commit::CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::RuntimeGenerated,
            CommitOrigin::StorageRuntime,
        ),
    )
}

fn commit_cache_put(
    runtime: &mut LifecycleCacheRuntime<CommitManualTimestampSource>,
    branch: BranchId,
    user_key: &[u8],
    timestamp_micros: u64,
) {
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, user_key),
                user_key.to_vec(),
                Timestamp::from_micros(timestamp_micros),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("cache put commit");
}

fn build_l0_tables_with_scheduled_flushes(
    runtime: &mut LifecycleCacheRuntime<CommitManualTimestampSource>,
    branch: BranchId,
    table_count: usize,
) {
    build_l0_tables_with_scheduled_flushes_from(runtime, branch, table_count, 10_000);
}

fn build_l0_tables_with_scheduled_flushes_from(
    runtime: &mut LifecycleCacheRuntime<CommitManualTimestampSource>,
    branch: BranchId,
    table_count: usize,
    base_timestamp_micros: u64,
) {
    assert!(table_count > 0);
    commit_cache_put(runtime, branch, b"scheduled-l0-seed", base_timestamp_micros);
    for index in 0..table_count {
        runtime
            .rotate_active_for_branch_for_maintenance(branch)
            .expect("rotate active table");
        let key = format!("scheduled-l0-trigger-{index}");
        commit_cache_put(
            runtime,
            branch,
            key.as_bytes(),
            base_timestamp_micros + 1 + index as u64,
        );
        assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
        run_queued_flush(runtime);
        assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    }
}

fn run_queued_flush(runtime: &mut LifecycleCacheRuntime<CommitManualTimestampSource>) {
    let outcome = runtime
        .run_next_flush_maintenance()
        .expect("run flush maintenance")
        .expect("flush task");
    assert_eq!(outcome.task_kind(), MaintenanceTaskKind::Flush);
    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Completed);
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

fn assert_commit_runtime_error(error: &LifecycleError) {
    assert!(matches!(
        error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::CommitRuntime,
            ..
        }
    ));
}

fn commit_runtime_source(error: &LifecycleError) -> &CommitRuntimeError {
    let LifecycleError::LowerLayer {
        layer: LifecycleLowerLayer::CommitRuntime,
        source: Some(source),
        ..
    } = error
    else {
        panic!("expected commit runtime error, got {error:?}");
    };
    source
        .downcast_ref::<CommitRuntimeError>()
        .expect("commit runtime source")
}

#[test]
fn cache_clear_branch_requires_quiesce_and_rejects_when_branch_guard_active() {
    let branch = branch_id(0x60);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    let pre_branches = runtime.list_branches(false).len();

    let guard = runtime
        .guard_set()
        .try_acquire_branch_guard(branch)
        .expect("active commit guard");

    let error = runtime
        .clear_branch(
            branch,
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect_err("clear_branch must reject while branch guard is active");
    assert_cache_quiesce_unavailable(&error);
    assert_eq!(runtime.list_branches(false).len(), pre_branches);
    drop(guard);
}

#[test]
fn cache_delete_branch_requires_quiesce_and_rejects_when_branch_guard_active() {
    let branch = branch_id(0x61);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    let pre_branches = runtime.list_branches(false).len();

    let guard = runtime
        .guard_set()
        .try_acquire_branch_guard(branch)
        .expect("active commit guard");

    let error = runtime
        .delete_branch(
            branch,
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
            None,
        )
        .expect_err("delete_branch must reject while branch guard is active");
    assert_cache_quiesce_unavailable(&error);
    assert_eq!(runtime.list_branches(false).len(), pre_branches);
    drop(guard);
}

#[test]
fn cache_fork_current_requires_quiesce_and_rejects_when_branch_guard_active() {
    let branch = branch_id(0x62);
    let other = branch_id(0x63);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    let pre_branches = runtime.list_branches(false).len();

    let guard = runtime
        .guard_set()
        .try_acquire_branch_guard(branch)
        .expect("active commit guard");

    let error = runtime
        .fork_current(
            branch,
            other,
            CommitBranchGeneration::new(1).expect("generation"),
        )
        .expect_err("fork_current must reject while branch guard is active");
    assert_cache_quiesce_unavailable(&error);
    assert_eq!(runtime.list_branches(false).len(), pre_branches);
    drop(guard);
}

#[test]
fn cache_fork_at_retained_version_requires_quiesce_and_rejects_when_branch_guard_active() {
    let branch = branch_id(0x64);
    let other = branch_id(0x65);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    // Seed a commit so the fork target version exists in retained history.
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"fork-seed"),
                b"value".to_vec(),
                Timestamp::from_micros(1_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("seed cache commit");
    let pre_branches = runtime.list_branches(false).len();

    let guard = runtime
        .guard_set()
        .try_acquire_branch_guard(branch)
        .expect("active commit guard");

    let error = runtime
        .fork_at_retained_version(
            branch,
            other,
            CommitBranchGeneration::new(1).expect("generation"),
            CommitVersion::new(1),
            CommitVersion::ZERO,
        )
        .expect_err("fork_at_retained_version must reject while branch guard is active");
    assert_cache_quiesce_unavailable(&error);
    assert_eq!(runtime.list_branches(false).len(), pre_branches);
    drop(guard);
}

#[test]
fn cache_fork_at_retained_timestamp_requires_quiesce_and_rejects_when_branch_guard_active() {
    let branch = branch_id(0x66);
    let other = branch_id(0x67);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    // Seed a commit so the fork target timestamp exists in retained history.
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"fork-seed-ts"),
                b"value".to_vec(),
                Timestamp::from_micros(2_000),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("seed cache commit");
    let pre_branches = runtime.list_branches(false).len();

    let guard = runtime
        .guard_set()
        .try_acquire_branch_guard(branch)
        .expect("active commit guard");

    let error = runtime
        .fork_at_retained_timestamp(
            branch,
            other,
            CommitBranchGeneration::new(1).expect("generation"),
            Timestamp::from_micros(2_000),
            CommitVersion::ZERO,
        )
        .expect_err("fork_at_retained_timestamp must reject while branch guard is active");
    assert_cache_quiesce_unavailable(&error);
    assert_eq!(runtime.list_branches(false).len(), pre_branches);
    drop(guard);
}

fn assert_cache_quiesce_unavailable(error: &LifecycleError) {
    use crate::commit::CommitRuntimeError;
    let LifecycleError::LowerLayer { layer, source, .. } = error else {
        panic!("expected LifecycleError::LowerLayer, got {error:?}");
    };
    assert_eq!(*layer, LifecycleLowerLayer::CommitRuntime);
    let source = source
        .as_ref()
        .expect("lower-layer error must carry a source");
    let commit_error = source
        .downcast_ref::<CommitRuntimeError>()
        .expect("source must downcast to CommitRuntimeError");
    assert!(
        matches!(
            commit_error,
            CommitRuntimeError::CommitQuiesceUnavailable { .. }
        ),
        "expected CommitQuiesceUnavailable, got {commit_error:?}"
    );
}

fn physical_key(branch: BranchId, user_key: &[u8]) -> PhysicalKey {
    PhysicalKey::new(
        branch,
        "cache",
        StorageSpaceId::engine(0x20).expect("engine space"),
        user_key.to_vec(),
    )
    .expect("physical key")
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

struct CountingBackend {
    capabilities: BackendCapabilities,
    capability_calls: AtomicUsize,
    other_calls: AtomicUsize,
}

impl CountingBackend {
    fn new(capabilities: BackendCapabilities) -> Self {
        Self {
            capabilities,
            capability_calls: AtomicUsize::new(0),
            other_calls: AtomicUsize::new(0),
        }
    }

    fn capability_calls(&self) -> usize {
        self.capability_calls.load(Ordering::SeqCst)
    }

    fn other_calls(&self) -> usize {
        self.other_calls.load(Ordering::SeqCst)
    }

    fn record_other(&self) {
        self.other_calls.fetch_add(1, Ordering::SeqCst);
    }

    fn unsupported(&self) -> BackendError {
        self.record_other();
        BackendError::new(
            BackendErrorKind::UnsupportedOperation,
            "unexpected backend call",
        )
    }
}

impl Backend for CountingBackend {
    fn capabilities(&self) -> BackendCapabilities {
        self.capability_calls.fetch_add(1, Ordering::SeqCst);
        self.capabilities
    }

    fn read_object(&self, _name: &ObjectName) -> BackendResult<Vec<u8>> {
        Err(self.unsupported())
    }

    fn read_range(&self, _name: &ObjectName, _range: BackendRange) -> BackendResult<Vec<u8>> {
        Err(self.unsupported())
    }

    fn write_object(&self, _name: &ObjectName, _bytes: &[u8]) -> BackendResult<BackendMetadata> {
        Err(self.unsupported())
    }

    fn delete_object(&self, name: &ObjectName) -> crate::backend::DeleteResult {
        crate::backend::failed_delete_result(name, self.unsupported())
    }

    fn list_prefix(&self, _prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        Err(self.unsupported())
    }

    fn object_metadata(&self, _name: &ObjectName) -> BackendResult<BackendMetadata> {
        Err(self.unsupported())
    }

    fn acquire_writer_lock(&self, name: &ObjectName) -> BackendResult<BackendWriterGuard> {
        self.record_other();
        Ok(BackendWriterGuard::new(name.clone(), ()))
    }

    fn append_object(&self, _name: &ObjectName, _bytes: &[u8]) -> BackendResult<BackendAppend> {
        Err(self.unsupported())
    }

    fn sync_object(&self, _name: &ObjectName) -> crate::backend::BackendResult<()> {
        Err(self.unsupported())
    }

    fn conditional_create(
        &self,
        _name: &ObjectName,
        _bytes: &[u8],
    ) -> BackendResult<BackendMetadata> {
        Err(self.unsupported())
    }

    fn conditional_update(
        &self,
        _name: &ObjectName,
        _expected: &BackendFence,
        _bytes: &[u8],
    ) -> BackendResult<BackendMetadata> {
        Err(self.unsupported())
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        _bytes: &[u8],
        _mode: PublishMode,
    ) -> PublishResult<PublishOutcome> {
        Err(PublishError::unsupported(name, self.unsupported()))
    }
}
