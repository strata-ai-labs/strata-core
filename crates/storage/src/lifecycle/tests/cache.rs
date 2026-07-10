use super::*;
use crate::backend::memory::MemoryBackend;
use crate::backend::{
    Backend, BackendAppend, BackendCapabilities, BackendError, BackendErrorKind, BackendFence,
    BackendMetadata, BackendRange, BackendResult, BackendWriterGuard, PublishError, PublishMode,
    PublishOutcome, PublishResult, CACHE_MODE_REQUIREMENTS,
};
use crate::branch::config::BranchRuntimeConfig;
use crate::branch::facts::{BranchLevel, BranchTableDescriptor};
use crate::branch::read::BranchOwnedTable;
#[cfg(feature = "perf-trace")]
use crate::branch::read::BranchScanBounds;
#[cfg(feature = "perf-trace")]
use crate::branch::state::compaction::{BranchCompactionKind, BranchCompactionRequest};
use crate::branch::state::BranchLocalState;
use crate::commit::{
    CommitBatch, CommitBatchOptions, CommitBranchGeneration, CommitBranchGenerationGuard,
    CommitDurabilityClass, CommitDurabilityMode, CommitExpiry, CommitManualTimestampSource,
    CommitMutation, CommitObservedVersion, CommitOrigin, CommitReadFact, CommitRetentionHint,
    CommitRuntimeConfig, CommitRuntimeError, CommitStamp, CommitTimestampPolicy,
    CommitUnresolvedDurable, CommitValidationFacts,
};
#[cfg(feature = "perf-trace")]
use crate::lifecycle::cache::{
    pause_next_cache_background_build_for_test, CacheBackgroundBuildKind,
    CacheBackgroundBuildPauseGuard, CacheBackgroundMaintenanceBuild,
    CacheBackgroundMaintenanceBuilt,
};
use crate::object::{ObjectName, ObjectPrefix};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    sort_table_rows_by_key, ImmutableTableBuilder, ImmutableTableReader, TableBuilderConfig,
    TableIdentity, TableReaderConfig, TableRow,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use strata_core::{BranchId, CommitVersion, Timestamp};

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
    assert_eq!(outcome.mutation_counts().timeline_rows(), 0);
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
fn cache_global_flush_task_drains_branches_in_deterministic_order() {
    let branch_high = branch_id(0x62);
    let branch_low = branch_id(0x61);
    let backend = MemoryBackend::new();
    let config = LifecycleConfig::default()
        .with_maintenance_scheduling_policy(LifecycleMaintenanceSchedulingPolicy::Disabled)
        .expect("disabled automatic maintenance");
    let mut runtime = open_runtime_with_config(branch_high, &backend, config);
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
fn queued_cache_compaction_defers_when_pressure_clears_before_run() {
    let branch = branch_id(0x88);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 1))
        .expect("enqueue stale compaction");

    let outcome = runtime
        .run_next_compaction_maintenance()
        .expect("run stale compaction")
        .expect("stale compaction outcome");

    assert_eq!(outcome.task_kind(), MaintenanceTaskKind::Compaction);
    assert_eq!(
        outcome.task_scope(),
        Some(MaintenanceTaskScope::TableLevel {
            branch_id: branch,
            level: 1,
        })
    );
    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Deferred);
    assert_eq!(
        outcome.reason(),
        Some("compaction task no longer has a pressure candidate")
    );
    assert_eq!(runtime.maintenance_status().stats().deferred(), 1);
    assert!(runtime.branch_state().owned_levels()[1].is_empty());
}

#[test]
fn stale_compaction_level_resubmits_current_scored_level() {
    let branch = branch_id(0x8c);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    build_l0_tables_with_scheduled_flushes(&mut runtime, branch, 4);
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 1))
        .expect("enqueue obsolete nonzero-level compaction");

    let stale = runtime
        .run_next_compaction_maintenance()
        .expect("run obsolete compaction")
        .expect("obsolete compaction outcome");
    assert_eq!(stale.status(), MaintenanceOutcomeStatus::Deferred);
    assert_eq!(
        stale.task_scope(),
        Some(MaintenanceTaskScope::TableLevel {
            branch_id: branch,
            level: 1,
        })
    );
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);

    let current = runtime
        .run_next_compaction_maintenance()
        .expect("run resubmitted compaction")
        .expect("resubmitted compaction outcome");
    assert_eq!(current.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(
        current.task_scope(),
        Some(MaintenanceTaskScope::TableLevel {
            branch_id: branch,
            level: 0,
        })
    );
    assert!(runtime.branch_state().owned_levels()[0].is_empty());
}

#[test]
fn compaction_chain_resubmits_highest_scored_branch() {
    let branch_low = branch_id(0x89);
    let branch_high = branch_id(0x8a);
    let backend = MemoryBackend::new();
    let config = LifecycleConfig::default()
        .with_maintenance_scheduling_policy(LifecycleMaintenanceSchedulingPolicy::Disabled)
        .expect("disabled automatic maintenance");
    let mut runtime = open_runtime_with_config(branch_low, &backend, config);
    runtime
        .create_branch(
            branch_high,
            CommitBranchGeneration::new(1).expect("generation"),
            None,
        )
        .expect("create high-pressure branch");

    build_l0_tables_with_manual_flushes_from(&mut runtime, branch_low, 4, 10_000);
    build_l0_tables_with_manual_flushes_from(&mut runtime, branch_high, 8, 20_000);
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch_low, 0))
        .expect("enqueue low-pressure compaction");

    let first = runtime
        .run_next_compaction_maintenance()
        .expect("run first compaction")
        .expect("first compaction outcome");
    assert_eq!(
        first.task_scope(),
        Some(MaintenanceTaskScope::TableLevel {
            branch_id: branch_low,
            level: 0,
        })
    );
    assert_eq!(first.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);

    let second = runtime
        .run_next_compaction_maintenance()
        .expect("run resubmitted compaction")
        .expect("resubmitted compaction outcome");
    assert_eq!(
        second.task_scope(),
        Some(MaintenanceTaskScope::TableLevel {
            branch_id: branch_high,
            level: 0,
        })
    );
    assert_eq!(second.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(
        runtime
            .branch_catalog()
            .branch_state(branch_high)
            .expect("high branch")
            .owned_levels()[0]
            .len(),
        0
    );
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_compaction_is_preempted_when_flush_pressure_exists() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x7b);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    for index in 0..4 {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                CommitBranchGenerationGuard::exact(
                    CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        install_owned_table_for_cache_test(
            state,
            branch,
            BranchLevel::ZERO,
            &format!("flush-preempt-l0-{index}"),
            vec![active_pressure_put_row(
                branch,
                format!("flush-preempt-l0-{index}").as_bytes(),
                index + 1,
                (index + 1) * 1_000,
                512,
                0x64,
            )],
        );
    }
    // Flush must be at the blocking backlog (>= FROZEN_BLOCKING_FLUSH_THRESHOLD) for it to preempt
    // compaction; below that, a frozen memtable is drained concurrently rather than preempting.
    let flush_block = crate::lifecycle::compaction::FROZEN_BLOCKING_FLUSH_THRESHOLD;
    for frozen_index in 0..flush_block {
        let version = 99 + frozen_index as u64;
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                CommitBranchGenerationGuard::exact(
                    CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        state
            .append_committed_rows_atomically(vec![active_pressure_put_row(
                branch,
                format!("flush-preempt-frozen-{frozen_index}").as_bytes(),
                version,
                version * 1_000,
                512,
                0x65,
            )])
            .expect("append frozen pressure row");
        state.rotate_active();
        assert_eq!(state.frozen_table_count(), frozen_index + 1);
    }
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 0))
        .expect("enqueue compaction");
    crate::observability::perf_trace::reset();

    let outcome = runtime
        .run_next_compaction_maintenance()
        .expect("run compaction maintenance")
        .expect("compaction outcome");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Deferred);
    assert!(outcome.retryable());
    assert_eq!(
        outcome.reason(),
        Some("flush pressure preempted compaction")
    );
    assert_eq!(
        runtime.maintenance_status().pending_tasks(),
        2,
        "preemption must keep compaction reachable behind the flush task"
    );
    assert_eq!(perf.lifecycle_compaction_flush_preemptions(), 1);
    assert_eq!(perf.lifecycle_compaction_operations_completed(), 0);

    let flush = runtime
        .run_next_flush_maintenance()
        .expect("run flush maintenance")
        .expect("flush outcome");
    assert_eq!(flush.status(), MaintenanceOutcomeStatus::Completed);
    let compaction = runtime
        .run_next_compaction_maintenance()
        .expect("run compaction maintenance after flush")
        .expect("compaction outcome after flush");
    assert_eq!(compaction.status(), MaintenanceOutcomeStatus::Completed);
}

#[cfg(feature = "perf-trace")]
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "stale-candidate regression keeps setup, mutation, publish, and resubmit assertions together"
)]
fn cache_background_compaction_stale_candidate_defers_and_resubmits_pressure() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x7c);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                CommitBranchGenerationGuard::exact(
                    CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        for index in 0..4 {
            install_owned_table_for_cache_test(
                state,
                branch,
                BranchLevel::ZERO,
                &format!("background-stale-l0-{index}"),
                vec![active_pressure_put_row(
                    branch,
                    format!("m-background-stale-l0-{index}").as_bytes(),
                    index + 1,
                    (index + 1) * 1_000,
                    256,
                    0x71,
                )],
            );
        }
    }
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 0))
        .expect("enqueue compaction");

    let step = runtime
        .start_next_background_table_rewrite_maintenance()
        .expect("start split background compaction")
        .expect("background compaction step");
    let candidate = match step {
        CacheBackgroundMaintenanceStep::Build(candidate) => *candidate,
        CacheBackgroundMaintenanceStep::Completed(outcome) => {
            panic!("expected background build step, got {outcome:?}")
        }
    };
    let prepared = candidate
        .build()
        .expect("build compaction outside runtime lock");
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                CommitBranchGenerationGuard::exact(
                    CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        let concurrent_request = BranchCompactionRequest::new(
            branch,
            BranchCompactionKind::CompactL0ToLevelOne,
            "background-stale-concurrent-output",
        )
        .expect("concurrent compaction request");
        state
            .compact_branch_owned_tables(&concurrent_request)
            .expect("concurrent compaction consumes planned inputs");
        for index in 0..4 {
            install_owned_table_for_cache_test(
                state,
                branch,
                BranchLevel::ZERO,
                &format!("background-stale-fresh-{index}"),
                vec![active_pressure_put_row(
                    branch,
                    format!("background-stale-fresh-{index}").as_bytes(),
                    99 + index,
                    (99 + index) * 1_000,
                    256,
                    0x72,
                )],
            );
        }
    }

    let outcome = runtime
        .finish_background_maintenance(prepared)
        .expect("finish stale background compaction");

    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Deferred);
    assert_eq!(
        outcome.reason(),
        Some("background maintenance candidate became stale before publish")
    );
    assert!(outcome.retryable());
    let status = runtime.maintenance_status();
    assert_eq!(status.active_task(), None);
    assert_eq!(
        status.pending_tasks(),
        1,
        "stale publish must resubmit current table pressure"
    );
    let layout = runtime
        .branch_catalog()
        .branch_state(branch)
        .expect("branch state")
        .source_layout();
    assert_eq!(
        layout.owned_l0_tables(),
        4,
        "stale output must not be published over fresh L0 pressure"
    );
    assert_eq!(
        layout
            .owned_nonzero_level_table_counts()
            .iter()
            .find(|count| count.level() == BranchLevel::new(1))
            .map_or(0, |count| count.table_count()),
        1
    );
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_background_candidate_stale_deferred(), 1);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_background_flush_deleted_branch_finishes_stale_and_clears_active_task() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x9a);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    commit_cache_put(&mut runtime, branch, b"background-flush-delete", 1_000);
    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate active table");
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::flush(branch))
        .expect("enqueue flush");

    let step = runtime
        .start_next_background_flush_maintenance()
        .expect("start split background flush")
        .expect("background flush step");
    let candidate = match step {
        CacheBackgroundMaintenanceStep::Build(candidate) => *candidate,
        CacheBackgroundMaintenanceStep::Completed(outcome) => {
            panic!("expected background build step, got {outcome:?}")
        }
    };
    let prepared = candidate.build().expect("build flush outside runtime lock");
    runtime
        .delete_branch(
            branch,
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
            Some(runtime.visible_version()),
        )
        .expect("delete branch before publish");
    crate::observability::perf_trace::reset();

    let outcome = runtime
        .finish_background_maintenance(prepared)
        .expect("finish stale background flush");

    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Deferred);
    assert_eq!(
        outcome.reason(),
        Some("background maintenance candidate became stale before publish")
    );
    assert!(outcome.retryable());
    let status = runtime.maintenance_status();
    assert_eq!(status.active_task(), None);
    assert_eq!(
        status.pending_tasks(),
        0,
        "deleted branches must not receive stale flush requeues"
    );
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_background_candidate_stale_deferred(), 1);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_background_compaction_deleted_branch_finishes_stale_and_clears_active_task() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x9b);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                CommitBranchGenerationGuard::exact(
                    CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        for index in 0..4 {
            install_owned_table_for_cache_test(
                state,
                branch,
                BranchLevel::ZERO,
                &format!("background-delete-l0-{index}"),
                vec![active_pressure_put_row(
                    branch,
                    format!("m-background-delete-l0-{index}").as_bytes(),
                    index + 1,
                    (index + 1) * 1_000,
                    256,
                    0x73,
                )],
            );
        }
    }
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 0))
        .expect("enqueue compaction");

    let step = runtime
        .start_next_background_table_rewrite_maintenance()
        .expect("start split background compaction")
        .expect("background compaction step");
    let candidate = match step {
        CacheBackgroundMaintenanceStep::Build(candidate) => *candidate,
        CacheBackgroundMaintenanceStep::Completed(outcome) => {
            panic!("expected background build step, got {outcome:?}")
        }
    };
    let prepared = candidate
        .build()
        .expect("build compaction outside runtime lock");
    runtime
        .delete_branch(
            branch,
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
            Some(runtime.visible_version()),
        )
        .expect("delete branch before publish");
    crate::observability::perf_trace::reset();

    let outcome = runtime
        .finish_background_maintenance(prepared)
        .expect("finish stale background compaction");

    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Deferred);
    assert_eq!(
        outcome.reason(),
        Some("background maintenance candidate became stale before publish")
    );
    assert!(outcome.retryable());
    let status = runtime.maintenance_status();
    assert_eq!(status.active_task(), None);
    assert_eq!(status.pending_tasks(), 0);
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_background_candidate_stale_deferred(), 1);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_background_materialization_deleted_branch_finishes_stale_and_clears_active_task() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let parent = branch_id(0x9c);
    let child = branch_id(0x9d);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(parent, &backend);

    commit_cache_put(
        &mut runtime,
        parent,
        b"background-materialize-parent",
        1_000,
    );
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
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::materialization(child))
        .expect("enqueue materialization");

    let step = runtime
        .start_next_background_table_rewrite_maintenance()
        .expect("start split background materialization")
        .expect("background materialization step");
    let candidate = match step {
        CacheBackgroundMaintenanceStep::Build(candidate) => *candidate,
        CacheBackgroundMaintenanceStep::Completed(outcome) => {
            panic!("expected background build step, got {outcome:?}")
        }
    };
    let prepared = candidate
        .build()
        .expect("build materialization outside runtime lock");
    runtime
        .delete_branch(
            child,
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
            Some(runtime.visible_version()),
        )
        .expect("delete child before publish");
    crate::observability::perf_trace::reset();

    let outcome = runtime
        .finish_background_maintenance(prepared)
        .expect("finish stale background materialization");

    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Deferred);
    assert_eq!(
        outcome.reason(),
        Some("background maintenance candidate became stale before publish")
    );
    assert!(outcome.retryable());
    let status = runtime.maintenance_status();
    assert_eq!(status.active_task(), None);
    assert_eq!(status.pending_tasks(), 0);
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_background_candidate_stale_deferred(), 1);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_foreground_commit_completes_while_background_flush_build_is_paused() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x9e);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    commit_cache_put(&mut runtime, branch, b"background-flush-snapshot", 1_000);
    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate active table");
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::flush(branch))
        .expect("enqueue flush");

    let step = runtime
        .start_next_background_flush_maintenance()
        .expect("start split background flush")
        .expect("background flush step");
    let candidate = expect_cache_background_build(step);
    assert!(
        runtime.maintenance_status().active_task().is_some(),
        "split build must leave the background task active while unlocked work is pending"
    );
    let (worker, mut pause) =
        spawn_paused_cache_background_build(candidate, CacheBackgroundBuildKind::Flush);
    pause.wait_until_entered();

    commit_cache_put(&mut runtime, branch, b"background-flush-foreground", 2_000);
    assert_eq!(
        cache_latest_value(&runtime, branch, b"background-flush-foreground"),
        b"background-flush-foreground".to_vec(),
        "foreground commit must be visible before the background flush publishes"
    );

    pause.release();
    let prepared = worker
        .join()
        .expect("background flush build thread")
        .expect("build flush outside runtime lock");
    let outcome = runtime
        .finish_background_maintenance(prepared)
        .expect("finish background flush");
    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(runtime.maintenance_status().active_task(), None);
    assert_eq!(
        cache_latest_value(&runtime, branch, b"background-flush-foreground"),
        b"background-flush-foreground".to_vec()
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_background_flush_publish_matches_frozen_rows_after_concurrent_rotation() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x9f);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    commit_cache_put(&mut runtime, branch, b"background-flush-old", 1_000);
    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate original active table");
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::flush(branch))
        .expect("enqueue flush");

    let step = runtime
        .start_next_background_flush_maintenance()
        .expect("start split background flush")
        .expect("background flush step");
    let candidate = expect_cache_background_build(step);
    let (worker, mut pause) =
        spawn_paused_cache_background_build(candidate, CacheBackgroundBuildKind::Flush);
    pause.wait_until_entered();

    commit_cache_put(&mut runtime, branch, b"background-flush-new", 2_000);
    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate new active table while flush builds");
    assert_eq!(
        runtime.branch_state().frozen_table_count(),
        2,
        "new rotation should shift the prepared frozen table index"
    );

    pause.release();
    let prepared = worker
        .join()
        .expect("background flush build thread")
        .expect("build flush outside runtime lock");
    let outcome = runtime
        .finish_background_maintenance(prepared)
        .expect("finish background flush");
    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(
        runtime.branch_state().frozen_table_count(),
        1,
        "publish should replace the matching frozen rows, not the stale index"
    );
    assert_eq!(
        cache_latest_value(&runtime, branch, b"background-flush-old"),
        b"background-flush-old".to_vec()
    );
    assert_eq!(
        cache_latest_value(&runtime, branch, b"background-flush-new"),
        b"background-flush-new".to_vec()
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_reads_and_commit_continue_while_background_compaction_build_is_paused() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x9f);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    let history_key = b"background-compaction-history";

    build_l0_history_tables_with_manual_flushes(&mut runtime, branch, history_key, 4);
    let before_history = cache_history_facts(&runtime, branch, history_key);
    let before_scan = cache_scan_user_keys(&runtime, branch, b"background-compaction-");
    assert_eq!(before_scan, vec![history_key.to_vec()]);
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 0))
        .expect("enqueue compaction");

    let step = runtime
        .start_next_background_table_rewrite_maintenance()
        .expect("start split background compaction")
        .expect("background compaction step");
    let candidate = expect_cache_background_build(step);
    assert!(runtime.maintenance_status().active_task().is_some());
    let (worker, mut pause) =
        spawn_paused_cache_background_build(candidate, CacheBackgroundBuildKind::Compaction);
    pause.wait_until_entered();

    assert_eq!(
        cache_history_facts(&runtime, branch, history_key),
        before_history,
        "history must be readable while compaction build is unfinished"
    );
    commit_cache_put(
        &mut runtime,
        branch,
        b"background-compaction-foreground",
        50_000,
    );
    assert_eq!(
        cache_latest_value(&runtime, branch, b"background-compaction-foreground"),
        b"background-compaction-foreground".to_vec()
    );
    let during_scan = cache_scan_user_keys(&runtime, branch, b"background-compaction-");
    assert_eq!(
        during_scan,
        vec![
            b"background-compaction-foreground".to_vec(),
            history_key.to_vec()
        ],
        "scan order must remain valid while compaction is active"
    );

    pause.release();
    let prepared = worker
        .join()
        .expect("background compaction build thread")
        .expect("build compaction outside runtime lock");
    let outcome = runtime
        .finish_background_maintenance(prepared)
        .expect("finish background compaction");
    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(runtime.maintenance_status().active_task(), None);
    assert_eq!(
        cache_history_facts(&runtime, branch, history_key),
        before_history
    );
    assert_eq!(
        cache_scan_user_keys(&runtime, branch, b"background-compaction-"),
        during_scan
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_foreground_commit_completes_while_background_materialization_build_is_paused() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let parent = branch_id(0xa0);
    let child = branch_id(0xa1);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(parent, &backend);

    commit_cache_put(
        &mut runtime,
        parent,
        b"background-materialization-parent",
        1_000,
    );
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
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::materialization(child))
        .expect("enqueue materialization");

    let step = runtime
        .start_next_background_table_rewrite_maintenance()
        .expect("start split background materialization")
        .expect("background materialization step");
    let candidate = expect_cache_background_build(step);
    assert!(runtime.maintenance_status().active_task().is_some());
    let (worker, mut pause) =
        spawn_paused_cache_background_build(candidate, CacheBackgroundBuildKind::Materialization);
    pause.wait_until_entered();

    assert_eq!(
        cache_latest_value(&runtime, child, b"background-materialization-parent"),
        b"background-materialization-parent".to_vec(),
        "inherited rows must remain readable while materialization is active"
    );
    commit_cache_put(
        &mut runtime,
        child,
        b"background-materialization-child",
        2_000,
    );
    assert_eq!(
        cache_latest_value(&runtime, child, b"background-materialization-child"),
        b"background-materialization-child".to_vec()
    );

    pause.release();
    let prepared = worker
        .join()
        .expect("background materialization build thread")
        .expect("build materialization outside runtime lock");
    let outcome = runtime
        .finish_background_maintenance(prepared)
        .expect("finish background materialization");
    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(runtime.maintenance_status().active_task(), None);
    assert_eq!(
        cache_latest_value(&runtime, child, b"background-materialization-parent"),
        b"background-materialization-parent".to_vec()
    );
    assert_eq!(
        cache_latest_value(&runtime, child, b"background-materialization-child"),
        b"background-materialization-child".to_vec()
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_background_compaction_clear_branch_finishes_stale_without_publishing_output() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0xa2);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    build_l0_tables_with_scheduled_flushes(&mut runtime, branch, 4);
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 0))
        .expect("enqueue compaction");
    let step = runtime
        .start_next_background_table_rewrite_maintenance()
        .expect("start split background compaction")
        .expect("background compaction step");
    let candidate = expect_cache_background_build(step);
    let prepared = candidate
        .build()
        .expect("build compaction outside runtime lock");

    runtime
        .clear_branch(
            branch,
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("clear branch before publish");
    crate::observability::perf_trace::reset();

    let outcome = runtime
        .finish_background_maintenance(prepared)
        .expect("finish stale background compaction");
    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Deferred);
    assert_eq!(runtime.maintenance_status().active_task(), None);
    let layout = runtime
        .branch_catalog()
        .branch_state(branch)
        .expect("branch state")
        .source_layout();
    assert_eq!(layout.owned_total_tables(), 0);
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_background_candidate_stale_deferred(), 1);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_fork_during_background_compaction_keeps_child_reads_valid_after_publish() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let parent = branch_id(0xa3);
    let child = branch_id(0xa4);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(parent, &backend);
    let inherited_key = b"background-fork-during-compaction";

    build_l0_history_tables_with_manual_flushes(&mut runtime, parent, inherited_key, 4);
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(parent, 0))
        .expect("enqueue parent compaction");
    let step = runtime
        .start_next_background_table_rewrite_maintenance()
        .expect("start split background compaction")
        .expect("background compaction step");
    let candidate = expect_cache_background_build(step);

    runtime
        .fork_current(
            parent,
            child,
            CommitBranchGeneration::new(1).expect("generation"),
        )
        .expect("fork child while parent compaction is active");
    let child_history_before = cache_history_facts(&runtime, child, inherited_key);
    assert_eq!(child_history_before.len(), 4);

    let prepared = candidate.build().expect("build parent compaction");
    let outcome = runtime
        .finish_background_maintenance(prepared)
        .expect("finish parent compaction");
    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(
        cache_history_facts(&runtime, child, inherited_key),
        child_history_before,
        "forked child must not lose inherited history when parent publishes background output"
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_explicit_compaction_drain_obeys_io_budget_policy() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x7d);
    let backend = MemoryBackend::new();
    let config = LifecycleConfig::default()
        .with_compaction_io_policy(LifecycleCompactionIoPolicy::per_task_byte_budget(1))
        .expect("compaction IO policy");
    let mut runtime = open_runtime_with_config(branch, &backend, config);
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                CommitBranchGenerationGuard::exact(
                    CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        for index in 0..4 {
            install_owned_table_for_cache_test(
                state,
                branch,
                BranchLevel::ZERO,
                &format!("explicit-budget-l0-{index}"),
                vec![active_pressure_put_row(
                    branch,
                    format!("explicit-budget-l0-{index}").as_bytes(),
                    index + 1,
                    (index + 1) * 1_000,
                    1024,
                    0x6b,
                )],
            );
        }
    }
    crate::observability::perf_trace::reset();

    let outcome = runtime
        .compact_branch_tables_to_fixed_point(
            &LifecycleCompactionDrainRequest::new(branch, "explicit-budget-drain")
                .expect("drain request"),
        )
        .expect("explicit compaction drain");
    let maintenance = outcome.maintenance_outcome();
    let state = runtime
        .branch_catalog_mut_for_test()
        .branch_state(branch)
        .expect("branch state after defer");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(maintenance.status(), MaintenanceOutcomeStatus::Deferred);
    assert_eq!(
        maintenance.reason(),
        Some("compaction IO byte budget deferred table rewrite")
    );
    assert!(maintenance.retryable());
    assert_eq!(outcome.operations_attempted(), 1);
    assert_eq!(outcome.operations_installed(), 0);
    assert_eq!(state.owned_levels()[0].len(), 4);
    assert_eq!(perf.lifecycle_compaction_io_budget_deferrals(), 1);
    assert_eq!(perf.lifecycle_compaction_operations_completed(), 0);
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
fn cache_maintenance_coverage_closing_state_records_failure_without_enqueue() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let active = branch_id(0x9f);
    let quiet = branch_id(0xa0);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(active, &backend);
    runtime
        .create_branch(
            quiet,
            CommitBranchGeneration::new(1).expect("generation"),
            None,
        )
        .expect("create quiet branch");
    commit_cache_put(&mut runtime, quiet, b"coverage-closing-seed", 1_000);
    runtime
        .rotate_active_for_branch_for_maintenance(quiet)
        .expect("rotate quiet branch");
    runtime
        .force_close_requested_for_test()
        .expect("force closing state");

    crate::observability::perf_trace::reset();
    let _ = runtime.schedule_post_commit_maintenance_for_test(active);
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(runtime.state(), LifecycleState::Closing);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    assert_eq!(perf.lifecycle_maintenance_coverage_scans(), 0);
    assert_eq!(perf.lifecycle_maintenance_coverage_tasks_enqueued(), 0);
    assert_eq!(perf.lifecycle_maintenance_coverage_tasks_coalesced(), 0);
    assert_eq!(perf.lifecycle_maintenance_coverage_stop_queue_full(), 0);
    assert_eq!(perf.lifecycle_maintenance_coverage_stop_failure(), 1);
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
fn cache_close_drains_stale_active_maintenance_before_closing() {
    let branch = branch_id(0x56);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    let active = MaintenanceTask::new_for_test(
        77,
        MaintenanceTaskRequest::new(
            MaintenanceTaskKind::HealthCollection,
            MaintenanceTaskPriority::High,
            MaintenanceTaskScope::Global,
            MaintenanceTaskPolicy::drain_before_close(),
        )
        .expect("active close-drain task"),
    )
    .expect("active task");
    runtime.set_active_maintenance_for_test(active);

    let close = runtime.close().expect("close drains active task");

    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(runtime.state(), LifecycleState::Closed);
    assert_eq!(runtime.maintenance_status().active_task(), None);
    assert_eq!(runtime.maintenance_status().stats().drained(), 1);
    assert_eq!(close.stats().maintenance_tasks(), 1);
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

#[test]
fn cache_commit_stays_clean_under_synthetic_source_shape_pressure() {
    let branch = branch_id(0x6b);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    // Build many frozen tables — enough to exceed the old FROZEN/L0 pressure
    // thresholds — by rotating between commits without ever flushing.
    for index in 0..7 {
        commit_cache_put(
            &mut runtime,
            branch,
            format!("synthetic-pressure-{index}").as_bytes(),
            1_000 + u64::try_from(index).expect("index fits"),
        );
        runtime
            .rotate_active_for_branch_for_maintenance(branch)
            .expect("rotate active table into frozen state");
    }
    assert!(runtime.branch_state().frozen_table_count() >= 6);

    // Cache neutralizes source-shape write-admission pressure entirely.
    let pressure = runtime.storage_pressure();
    assert_eq!(pressure.severity(), LifecycleStoragePressureSeverity::None);
    assert_eq!(pressure.reason(), LifecycleStoragePressureReason::None);
    assert!(pressure.suggested_task().is_none());
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);

    // A further commit is admitted clean, not under pressure, and runs no
    // inline maintenance.
    commit_cache_put(&mut runtime, branch, b"synthetic-pressure-final", 9_000);
    let admission = runtime
        .last_write_admission()
        .expect("clean admission facts");
    assert_eq!(
        admission.status(),
        LifecycleWriteAdmissionStatus::AcceptedClean
    );
    assert!(!admission.inline_maintenance_driven());
}

#[test]
fn cache_reads_remain_correct_across_frozen_without_flush() {
    let branch = branch_id(0x6c);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    let keys: [&[u8]; 5] = [
        b"frozen-read-a",
        b"frozen-read-b",
        b"frozen-read-c",
        b"frozen-read-d",
        b"frozen-read-e",
    ];
    for (index, key) in keys.iter().enumerate() {
        commit_cache_put(
            &mut runtime,
            branch,
            key,
            1_000 + u64::try_from(index).expect("index fits"),
        );
        runtime
            .rotate_active_for_branch_for_maintenance(branch)
            .expect("rotate each key into its own frozen table");
    }

    // Nothing has been flushed: frozen tables exist, but L0 is empty.
    assert!(runtime.branch_state().frozen_table_count() > 0);
    assert!(runtime.branch_state().owned_levels()[0].is_empty());

    let read_all = |runtime: &LifecycleCacheRuntime<CommitManualTimestampSource>| {
        for key in keys {
            let row = runtime
                .read_latest_point_or_tombstone_for_branch(branch, &physical_key(branch, key))
                .expect("read latest")
                .expect("visible row");
            assert_eq!(row.row().value(), key);
        }
    };

    // Reads are correct directly from frozen tables, before any flush.
    read_all(&runtime);

    // An explicit flush drains the frozen tables into L0.
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::flush(branch))
        .expect("enqueue explicit flush");
    runtime
        .run_next_flush_maintenance()
        .expect("run explicit flush")
        .expect("flush outcome");

    // Reads return identical values after flush — they never depended on it.
    read_all(&runtime);
}

#[test]
fn cache_neutralizes_l0_owned_table_backlog_pressure() {
    let branch = branch_id(0x6d);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    // Install many L0 owned tables directly — well past the durable urgent and
    // blocking L0 backlog thresholds — so the durable classifier would raise
    // LevelZeroTableBacklog. Cache neutralizes the entire source-shape class.
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                CommitBranchGenerationGuard::exact(
                    CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("cache branch state");
        for index in 0..16 {
            install_owned_table_for_cache_test(
                state,
                branch,
                BranchLevel::ZERO,
                &format!("cache-l0-backlog-{index}"),
                vec![active_pressure_put_row(
                    branch,
                    format!("l0-backlog-{index}").as_bytes(),
                    1 + u64::try_from(index).expect("index fits"),
                    10_000 + u64::try_from(index).expect("index fits"),
                    128,
                    0x41,
                )],
            );
        }
    }
    assert_eq!(runtime.branch_state().owned_levels()[0].len(), 16);
    // The synthetic tables carried commit versions 1..=16; advance the runtime
    // frontier past them so a subsequent ordinary commit is well-formed.
    runtime
        .catch_up_commit_frontier_for_test(CommitVersion::new(16), Timestamp::from_micros(10_016));

    let pressure = runtime.storage_pressure();
    assert_eq!(pressure.severity(), LifecycleStoragePressureSeverity::None);
    assert_eq!(pressure.reason(), LifecycleStoragePressureReason::None);
    assert!(pressure.suggested_task().is_none());

    commit_cache_put(&mut runtime, branch, b"l0-backlog-admit", 60_000);
    let admission = runtime
        .last_write_admission()
        .expect("clean admission facts");
    assert_eq!(
        admission.status(),
        LifecycleWriteAdmissionStatus::AcceptedClean
    );
    assert!(!admission.inline_maintenance_driven());
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
}

#[test]
fn cache_neutralizes_nonzero_level_table_backlog_pressure() {
    let branch = branch_id(0x6e);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    // Install many owned tables at a nonzero level. On a durable runtime this
    // raises NonZeroLevelTableBacklog table-rewrite pressure; cache must report
    // no source-shape pressure regardless.
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                CommitBranchGenerationGuard::exact(
                    CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("cache branch state");
        for index in 0..16 {
            install_owned_table_for_cache_test(
                state,
                branch,
                BranchLevel::new(1),
                &format!("cache-l1-backlog-{index}"),
                vec![active_pressure_put_row(
                    branch,
                    format!("l1-backlog-{index}").as_bytes(),
                    1 + u64::try_from(index).expect("index fits"),
                    10_000 + u64::try_from(index).expect("index fits"),
                    128,
                    0x42,
                )],
            );
        }
    }
    assert_eq!(runtime.branch_state().owned_levels()[1].len(), 16);
    runtime
        .catch_up_commit_frontier_for_test(CommitVersion::new(16), Timestamp::from_micros(10_016));

    let pressure = runtime.storage_pressure();
    assert_eq!(pressure.severity(), LifecycleStoragePressureSeverity::None);
    assert_eq!(pressure.reason(), LifecycleStoragePressureReason::None);
    assert!(pressure.suggested_task().is_none());

    commit_cache_put(&mut runtime, branch, b"l1-backlog-admit", 60_000);
    let admission = runtime
        .last_write_admission()
        .expect("clean admission facts");
    assert_eq!(
        admission.status(),
        LifecycleWriteAdmissionStatus::AcceptedClean
    );
    assert!(!admission.inline_maintenance_driven());
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_pressure_records_no_block_wait_or_pressure_reject() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x6f);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    // Build a large frozen backlog with real commits and rotations — enough to
    // exceed the durable urgent/blocking frozen thresholds — then drive a commit
    // through write admission under that shape.
    for index in 0..7 {
        commit_cache_put(
            &mut runtime,
            branch,
            format!("clock-pressure-seed-{index}").as_bytes(),
            1_000 + u64::try_from(index).expect("index fits"),
        );
        runtime
            .rotate_active_for_branch_for_maintenance(branch)
            .expect("rotate into frozen state");
    }
    assert!(runtime.branch_state().frozen_table_count() >= 6);
    crate::observability::perf_trace::reset();

    commit_cache_put(&mut runtime, branch, b"clock-pressure-final", 90_000);

    // Cache never enters admission block-wait: there is no wait attempt and no
    // pressure rejection under synthetic source-shape pressure.
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_write_admission_block_wait_ns(), 0);
    assert_eq!(perf.lifecycle_write_admission_pressure_rejects(), 0);
}

#[test]
fn cache_rejects_commit_after_close_with_typed_invalid_state() {
    let branch = branch_id(0x70);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    runtime.close().expect("cache close");

    let error = runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"after-close"),
                b"value".to_vec(),
                Timestamp::from_micros(5_000),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect_err("commit after close rejected");

    assert!(matches!(
        error,
        LifecycleError::InvalidLifecycleState { .. }
    ));
    assert_eq!(error.code(), "failed_precondition.lifecycle.state");
}

#[test]
fn cache_rejects_durable_commit_with_typed_durability_unavailable() {
    use crate::commit::CommitRuntimeError;

    let branch = branch_id(0x7c);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    // A durable commit request against the cache executor is rejected with the
    // existing typed durability-unavailable error, asserted on class/code and
    // the downcast variant — never on display text.
    let error = runtime
        .execute_cache_commit(
            put_batch_with_durability(
                branch,
                physical_key(branch, b"durable-on-cache"),
                b"value".to_vec(),
                Timestamp::from_micros(1_000),
                CommitDurabilityMode::Standard,
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect_err("durable commit rejected by cache executor");

    assert_eq!(error.code(), "failed_precondition.lifecycle.commit_runtime");
    let LifecycleError::LowerLayer {
        layer: LifecycleLowerLayer::CommitRuntime,
        source: Some(source),
        ..
    } = &error
    else {
        panic!("expected commit-runtime lower-layer error, got {error:?}");
    };
    let commit_error = source
        .downcast_ref::<CommitRuntimeError>()
        .expect("source must downcast to CommitRuntimeError");
    assert!(
        matches!(
            commit_error,
            CommitRuntimeError::DurabilityUnavailable { .. }
        ),
        "expected DurabilityUnavailable, got {commit_error:?}"
    );
}

#[test]
fn cache_reads_and_writes_succeed_with_zero_background_tasks() {
    // Cache correctness never depends on a background worker draining tasks: a
    // load and its reads succeed while the maintenance queue reports zero
    // background workers, zero active tasks, and zero completed tasks.
    let branch = branch_id(0x7d);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    for index in 0..16 {
        commit_cache_put(
            &mut runtime,
            branch,
            format!("no-worker-{index}").as_bytes(),
            1_000 + u64::try_from(index).expect("index fits"),
        );
    }

    // Every committed key reads back correctly with no maintenance progress.
    for index in 0..16 {
        let user_key = format!("no-worker-{index}");
        let row = runtime
            .read_latest_point_or_tombstone_for_branch(
                branch,
                &physical_key(branch, user_key.as_bytes()),
            )
            .expect("latest read")
            .expect("visible row");
        assert_eq!(row.row().value(), user_key.as_bytes());
    }

    let status = runtime.maintenance_status();
    assert_eq!(status.active_tasks(), 0);
    assert_eq!(status.active_task(), None);
    assert_eq!(status.stats().completed(), 0);
    assert_eq!(status.pending_tasks(), 0);
}

fn delete_batch(branch: BranchId, key: PhysicalKey, timestamp: Timestamp) -> CommitBatch {
    CommitBatch::mutating(
        branch,
        vec![CommitMutation::delete(key)],
        CommitValidationFacts::empty(),
        CommitBatchOptions::new(
            CommitDurabilityMode::Cache,
            crate::commit::CommitConflictValidationMode::Skip,
            crate::commit::CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::Explicit(timestamp),
            CommitOrigin::StorageRuntime,
        ),
    )
}

fn commit_cache_delete(
    runtime: &mut LifecycleCacheRuntime<CommitManualTimestampSource>,
    branch: BranchId,
    user_key: &[u8],
    timestamp_micros: u64,
) {
    runtime
        .execute_cache_commit(
            delete_batch(
                branch,
                physical_key(branch, user_key),
                Timestamp::from_micros(timestamp_micros),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("cache delete commit");
}

#[test]
fn cache_read_correctness_without_maintenance_point_and_repeated_puts() {
    let branch = branch_id(0x71);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    let key = physical_key(branch, b"repeat");

    // Several versions of the same key, rotated into frozen tables between each
    // commit. No flush or compaction is ever run.
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                key.clone(),
                b"v1".to_vec(),
                Timestamp::from_micros(1_000),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("first put");
    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate after first put");
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                key.clone(),
                b"v2".to_vec(),
                Timestamp::from_micros(2_000),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("second put");
    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate after second put");
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                key.clone(),
                b"v3".to_vec(),
                Timestamp::from_micros(3_000),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("third put");

    let latest = runtime
        .read_latest_point_or_tombstone_for_branch(branch, &key)
        .expect("latest read")
        .expect("visible row");
    assert_eq!(latest.row().value(), b"v3");
    assert!(!latest.row().is_tombstone());
}

#[test]
fn cache_read_correctness_without_maintenance_deletes_and_tombstones() {
    let branch = branch_id(0x72);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    let key = physical_key(branch, b"deleted");

    commit_cache_put(&mut runtime, branch, b"deleted", 1_000);
    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate put into frozen");
    commit_cache_delete(&mut runtime, branch, b"deleted", 2_000);

    // Latest visible read sees nothing once the key is deleted.
    assert!(runtime
        .read_view_for_branch(branch)
        .expect("read view")
        .latest(&key)
        .expect("latest read")
        .is_none());

    // The tombstone is still observable through the tombstone-inclusive read.
    let with_tombstone = runtime
        .read_latest_point_or_tombstone_for_branch(branch, &key)
        .expect("tombstone read")
        .expect("tombstone row");
    assert!(with_tombstone.row().is_tombstone());
}

/// BS2.2 deliberate behavior change: a row applied above `visible` (the state a
/// visible-publication failure leaves behind — `applied_not_visible`) is hidden from the
/// bounded runtime Latest read while the gate blocks follow-on commits. The unbounded
/// snapshot path (`read_view`) still serves it — the pinned same-branch read-your-writes
/// contract for cache mode is unchanged.
#[test]
fn cache_bounded_latest_read_hides_applied_not_visible_row_while_gate_blocks_commits() {
    let branch = branch_id(0x7B);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    // A normally committed row: `visible` covers it.
    commit_cache_put(&mut runtime, branch, b"vis-acked", 1_000);
    assert_eq!(runtime.visible_version(), CommitVersion::new(1));

    // Recreate the applied-not-visible shape without a publish-failure seam: trip the gate,
    // then apply a row above `visible` directly into branch state.
    let hidden_version = CommitVersion::new(2);
    let stamp =
        CommitStamp::new(branch, hidden_version, Timestamp::from_micros(2_000)).expect("stamp");
    let unresolved = CommitUnresolvedDurable::applied_not_visible(
        stamp,
        CommitDurabilityClass::NotDurable,
        "test: visible publication failed after apply",
    )
    .expect("unresolved fact");
    runtime
        .durable_gate_for_test()
        .record_unresolved(unresolved)
        .expect("trip the unresolved gate");
    let generation = runtime
        .branch_catalog()
        .registry()
        .lookup(branch)
        .expect("branch lookup")
        .generation();
    let hidden_key = physical_key(branch, b"vis-hidden");
    runtime
        .branch_catalog_mut_for_test()
        .branch_state_mut(branch, CommitBranchGenerationGuard::exact(generation))
        .expect("branch state")
        .append_committed_row(StorageRow::put(
            hidden_key.clone(),
            hidden_version,
            Timestamp::from_micros(2_000),
            Timestamp::EPOCH,
            b"hidden-value".to_vec(),
        ))
        .expect("apply row above visible");
    // BS2.3: this test mutates branch state directly (bypassing the commit publish); resync.
    runtime.publish_branch_snapshot_for_test(branch);

    // The bounded Latest point read hides the unacknowledged row...
    assert!(runtime
        .read_latest_point_or_tombstone_for_branch(branch, &hidden_key)
        .expect("bounded point read")
        .is_none());
    // ...while rows at or below `visible` stay served...
    let acked = runtime
        .read_latest_point_or_tombstone_for_branch(branch, &physical_key(branch, b"vis-acked"))
        .expect("bounded point read")
        .expect("acked row stays visible");
    assert_eq!(acked.row().commit_version(), CommitVersion::new(1));
    // ...and the bounded scan agrees.
    let bounds = crate::branch::read::BranchScanBounds::prefix(&physical_key(branch, b"vis-"));
    let scanned = runtime
        .scan_latest_including_tombstones_for_branch(branch, &bounds, None)
        .expect("bounded scan");
    assert_eq!(scanned.len(), 1);
    assert_eq!(scanned[0].row().physical_key().user_key(), b"vis-acked");

    // The unbounded snapshot path still serves the applied row (cache RYW, unchanged).
    assert!(runtime
        .read_view_for_branch(branch)
        .expect("read view")
        .latest(&hidden_key)
        .expect("snapshot read")
        .is_some());

    // And the gate blocks the next mutating commit at the runtime level.
    let error = runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"vis-blocked"),
                b"blocked".to_vec(),
                Timestamp::from_micros(3_000),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect_err("unresolved gate blocks the follow-on commit");
    let LifecycleError::LowerLayer {
        layer: LifecycleLowerLayer::CommitRuntime,
        source: Some(source),
        ..
    } = error
    else {
        panic!("expected commit-runtime lower-layer error, got {error:?}");
    };
    let commit_error = source
        .downcast_ref::<CommitRuntimeError>()
        .expect("commit runtime source");
    assert!(matches!(
        commit_error,
        CommitRuntimeError::UnresolvedDurableCommit { .. }
    ));
}

#[test]
fn cache_read_correctness_without_maintenance_range_scans_with_limit() {
    let branch = branch_id(0x73);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    // Commit several keys across multiple batches, rotating each into its own
    // frozen table so the scan must merge across frozen sources without flush.
    let keys: [&[u8]; 4] = [b"scan-a", b"scan-b", b"scan-c", b"scan-d"];
    for (index, user_key) in keys.iter().enumerate() {
        commit_cache_put(
            &mut runtime,
            branch,
            user_key,
            1_000 + u64::try_from(index).expect("index fits"),
        );
        runtime
            .rotate_active_for_branch_for_maintenance(branch)
            .expect("rotate each key into frozen table");
    }

    let bounds = crate::branch::read::BranchScanBounds::prefix(&physical_key(branch, b"scan-"));

    // Full scan returns all four keys in order.
    let full = runtime
        .scan_latest_including_tombstones_for_branch(branch, &bounds, None)
        .expect("full prefix scan");
    let full_keys: Vec<Vec<u8>> = full
        .iter()
        .map(|row| row.row().physical_key().user_key().to_vec())
        .collect();
    assert_eq!(
        full_keys,
        keys.iter().map(|key| key.to_vec()).collect::<Vec<_>>()
    );

    // A bounded scan honors the visible limit.
    let bounded = runtime
        .scan_latest_including_tombstones_for_branch(branch, &bounds, Some(2))
        .expect("bounded prefix scan");
    assert_eq!(bounded.len(), 2);
    assert_eq!(bounded[0].row().physical_key().user_key(), b"scan-a");
    assert_eq!(bounded[1].row().physical_key().user_key(), b"scan-b");
}

#[test]
fn cache_read_correctness_without_maintenance_history_across_versions() {
    let branch = branch_id(0x74);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    let key = physical_key(branch, b"history");

    for (index, (value, timestamp)) in [
        (b"h1".to_vec(), 1_000_u64),
        (b"h2".to_vec(), 2_000),
        (b"h3".to_vec(), 3_000),
    ]
    .into_iter()
    .enumerate()
    {
        runtime
            .execute_cache_commit(
                put_batch(
                    branch,
                    key.clone(),
                    value,
                    Timestamp::from_micros(timestamp),
                ),
                CommitBranchGenerationGuard::not_supplied(),
            )
            .expect("history put");
        if index < 2 {
            runtime
                .rotate_active_for_branch_for_maintenance(branch)
                .expect("rotate each version into frozen table");
        }
    }

    let history = runtime
        .read_view_for_branch(branch)
        .expect("read view")
        .history(&key, crate::branch::read::BranchHistoryOptions::all())
        .expect("history");
    let versions: Vec<u64> = history
        .iter()
        .map(|row| row.row().commit_version().as_u64())
        .collect();
    assert_eq!(
        versions,
        vec![3, 2, 1],
        "history is newest-first across versions"
    );
}

#[test]
fn cache_read_correctness_without_maintenance_timestamp_reads() {
    let branch = branch_id(0x75);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    let key = physical_key(branch, b"ts");

    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                key.clone(),
                b"early".to_vec(),
                Timestamp::from_micros(2_000),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("early put");
    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate early version into frozen table");
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                key.clone(),
                b"late".to_vec(),
                Timestamp::from_micros(4_000),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("late put");

    let view = runtime.read_view_for_branch(branch).expect("read view");

    // Before any write: no visible row.
    assert!(view
        .read_point(
            &key,
            crate::branch::read::BranchReadBound::at_timestamp(Timestamp::from_micros(1_000))
        )
        .expect("read before first write")
        .is_none());
    // At the early write timestamp: early value.
    assert_eq!(
        view.read_point(
            &key,
            crate::branch::read::BranchReadBound::at_timestamp(Timestamp::from_micros(2_000))
        )
        .expect("read at early ts")
        .expect("early row")
        .row()
        .value(),
        b"early"
    );
    // Between writes: still the early value.
    assert_eq!(
        view.read_point(
            &key,
            crate::branch::read::BranchReadBound::at_timestamp(Timestamp::from_micros(3_000))
        )
        .expect("read between writes")
        .expect("early row")
        .row()
        .value(),
        b"early"
    );
    // At or after the late write timestamp: late value.
    assert_eq!(
        view.read_point(
            &key,
            crate::branch::read::BranchReadBound::at_timestamp(Timestamp::from_micros(5_000))
        )
        .expect("read after late ts")
        .expect("late row")
        .row()
        .value(),
        b"late"
    );
}

#[test]
fn cache_read_correctness_without_maintenance_branch_fork_reads() {
    let parent = branch_id(0x76);
    let child = branch_id(0x77);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(parent, &backend);

    commit_cache_put(&mut runtime, parent, b"shared", 1_000);
    // fork_current requires the source to hold no active rows and no frozen
    // tables; flush the parent into an owned L0 table first. The fork itself
    // runs no maintenance — it inherits the parent's owned tables by reference.
    runtime
        .rotate_active_for_branch_for_maintenance(parent)
        .expect("rotate parent into frozen table");
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::flush(parent))
        .expect("enqueue parent flush");
    runtime
        .run_next_flush_maintenance()
        .expect("run parent flush")
        .expect("flush outcome");

    runtime
        .fork_current(
            parent,
            child,
            CommitBranchGeneration::new(1).expect("generation"),
        )
        .expect("fork child branch");

    // The child inherits the parent row without any flush or materialization.
    // Inherited rows are read through the child-scoped key.
    let inherited = runtime
        .read_latest_point_or_tombstone_for_branch(child, &physical_key(child, b"shared"))
        .expect("child inherited read")
        .expect("inherited row");
    assert_eq!(inherited.row().value(), b"shared");

    // A child-local commit is visible only on the child; the parent's own
    // branch-scoped key for the same user key never sees it.
    commit_cache_put(&mut runtime, child, b"child-only", 2_000);
    assert!(runtime
        .read_view_for_branch(child)
        .expect("child view")
        .latest(&physical_key(child, b"child-only"))
        .expect("child read")
        .is_some());
    assert!(runtime
        .read_view_for_branch(parent)
        .expect("parent view")
        .latest(&physical_key(parent, b"child-only"))
        .expect("parent read")
        .is_none());
}

#[test]
fn cache_read_results_are_identical_before_and_after_flush_and_compaction() {
    let branch = branch_id(0x78);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    let keys: [&[u8]; 4] = [b"stable-a", b"stable-b", b"stable-c", b"stable-d"];
    for (index, user_key) in keys.iter().enumerate() {
        commit_cache_put(
            &mut runtime,
            branch,
            user_key,
            1_000 + u64::try_from(index).expect("index fits"),
        );
        runtime
            .rotate_active_for_branch_for_maintenance(branch)
            .expect("rotate each key into frozen table");
    }

    let read_all = |runtime: &LifecycleCacheRuntime<CommitManualTimestampSource>| -> Vec<Vec<u8>> {
        keys.iter()
            .map(|user_key| {
                runtime
                    .read_latest_point_or_tombstone_for_branch(
                        branch,
                        &physical_key(branch, user_key),
                    )
                    .expect("latest read")
                    .expect("visible row")
                    .row()
                    .value()
                    .to_vec()
            })
            .collect()
    };

    let before = read_all(&runtime);

    // Explicit test-only flush, then explicit test-only compaction.
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::flush(branch))
        .expect("enqueue explicit flush");
    runtime
        .run_next_flush_maintenance()
        .expect("run explicit flush")
        .expect("flush outcome");
    let after_flush = read_all(&runtime);

    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 0))
        .expect("enqueue explicit compaction");
    runtime
        .run_next_compaction_maintenance()
        .expect("run explicit compaction")
        .expect("compaction outcome");
    let after_compaction = read_all(&runtime);

    assert_eq!(before, after_flush, "reads must not depend on flush");
    assert_eq!(
        before, after_compaction,
        "reads must not depend on compaction"
    );
}

#[test]
fn cache_ordinary_commits_enqueue_no_maintenance_across_shapes() {
    let branch = branch_id(0x79);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    let assert_no_maintenance = |runtime: &LifecycleCacheRuntime<CommitManualTimestampSource>| {
        let status = runtime.maintenance_status();
        assert_eq!(status.pending_tasks(), 0);
        assert_eq!(status.stats().enqueued(), 0);
        assert_eq!(status.stats().completed(), 0);
    };

    // One batch.
    commit_cache_put(&mut runtime, branch, b"single-batch", 1_000);
    assert_no_maintenance(&runtime);

    // Many batches.
    for index in 0..32 {
        commit_cache_put(
            &mut runtime,
            branch,
            format!("many-batches-{index}").as_bytes(),
            2_000 + u64::try_from(index).expect("index fits"),
        );
    }
    assert_no_maintenance(&runtime);

    // Large values.
    for index in 0..4 {
        runtime
            .execute_cache_commit(
                put_batch(
                    branch,
                    physical_key(branch, format!("large-value-{index}").as_bytes()),
                    vec![0x55; 256 * 1024],
                    Timestamp::from_micros(10_000 + u64::try_from(index).expect("index fits")),
                ),
                CommitBranchGenerationGuard::not_supplied(),
            )
            .expect("large value commit");
    }
    assert_no_maintenance(&runtime);

    // Many distinct key ranges.
    for range in 0..8 {
        for offset in 0..8 {
            commit_cache_put(
                &mut runtime,
                branch,
                format!("range-{range}-key-{offset}").as_bytes(),
                20_000 + u64::try_from(range * 8 + offset).expect("index fits"),
            );
        }
    }
    assert_no_maintenance(&runtime);
}

#[test]
fn cache_explicit_test_only_maintenance_door_still_runs_flush() {
    // Ordinary cache commits never enqueue maintenance (proven above), but the
    // explicit test-only maintenance door remains fully functional: the
    // mechanics are intact, separated from product cache scheduling policy.
    let branch = branch_id(0x7b);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    commit_cache_put(&mut runtime, branch, b"explicit-door", 1_000);
    runtime
        .rotate_active_for_branch_for_maintenance(branch)
        .expect("rotate into frozen source");
    assert!(runtime.branch_state().frozen_table_count() > 0);

    let enqueue = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::flush(branch))
        .expect("enqueue explicit flush");
    assert!(enqueue.was_enqueued());
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);

    let outcome = runtime
        .run_next_flush_maintenance()
        .expect("run explicit flush")
        .expect("flush outcome");
    assert_eq!(outcome.task_kind(), MaintenanceTaskKind::Flush);
    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Completed);

    let status = runtime.maintenance_status();
    assert_eq!(status.pending_tasks(), 0);
    assert_eq!(status.stats().completed(), 1);
    assert_eq!(runtime.branch_state().frozen_table_count(), 0);
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

fn active_pressure_put_row(
    branch: BranchId,
    user_key: &[u8],
    version: u64,
    timestamp: u64,
    value_len: usize,
    byte: u8,
) -> StorageRow {
    StorageRow::put(
        physical_key(branch, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(timestamp),
        Timestamp::EPOCH,
        vec![byte; value_len],
    )
}

fn install_owned_table_for_cache_test(
    state: &mut BranchLocalState,
    branch: BranchId,
    level: BranchLevel,
    identity: &str,
    rows: Vec<StorageRow>,
) {
    state
        .install_owned_table_at_level(
            level,
            owned_table_for_cache_test(branch, level, identity, rows),
        )
        .expect("install owned table");
}

fn owned_table_for_cache_test(
    branch: BranchId,
    level: BranchLevel,
    identity: &str,
    rows: Vec<StorageRow>,
) -> BranchOwnedTable {
    let identity = TableIdentity::new(identity).expect("identity");
    let mut table_rows = rows.into_iter().map(TableRow::new).collect::<Vec<_>>();
    sort_table_rows_by_key(&mut table_rows);
    let artifact = ImmutableTableBuilder::new(TableBuilderConfig::default())
        .expect("builder")
        .build_from_rows(identity.clone(), &table_rows)
        .expect("built table");
    let reader = ImmutableTableReader::open_bytes(
        identity.clone(),
        artifact.into_bytes(),
        TableReaderConfig::default(),
    )
    .expect("reader");
    let descriptor =
        BranchTableDescriptor::new(identity, reader.facts().clone(), level).expect("descriptor");
    let extras =
        crate::table::TableSummaryExtras::from_rows(reader.rows()).expect("table summary extras");
    BranchOwnedTable::new(branch, descriptor, reader, extras).expect("owned table")
}

#[cfg(feature = "perf-trace")]
fn spawn_paused_cache_background_build(
    build: CacheBackgroundMaintenanceBuild,
    kind: CacheBackgroundBuildKind,
) -> (
    std::thread::JoinHandle<LifecycleResult<CacheBackgroundMaintenanceBuilt>>,
    CacheBackgroundBuildPauseGuard,
) {
    let pause = pause_next_cache_background_build_for_test(kind);
    let build_thread = std::thread::spawn(move || build.build());
    (build_thread, pause)
}

#[cfg(feature = "perf-trace")]
fn expect_cache_background_build(
    step: CacheBackgroundMaintenanceStep,
) -> CacheBackgroundMaintenanceBuild {
    match step {
        CacheBackgroundMaintenanceStep::Build(candidate) => *candidate,
        CacheBackgroundMaintenanceStep::Completed(outcome) => {
            panic!("expected background build step, got {outcome:?}")
        }
    }
}

#[cfg(feature = "perf-trace")]
fn cache_latest_value(
    runtime: &LifecycleCacheRuntime<CommitManualTimestampSource>,
    branch: BranchId,
    user_key: &[u8],
) -> Vec<u8> {
    runtime
        .read_view_for_branch(branch)
        .expect("read view")
        .latest(&physical_key(branch, user_key))
        .expect("latest read")
        .expect("visible row")
        .row()
        .value()
        .to_vec()
}

#[cfg(feature = "perf-trace")]
fn cache_scan_user_keys(
    runtime: &LifecycleCacheRuntime<CommitManualTimestampSource>,
    branch: BranchId,
    prefix: &[u8],
) -> Vec<Vec<u8>> {
    runtime
        .read_view_for_branch(branch)
        .expect("read view")
        .scan_prefix(
            &BranchScanBounds::prefix(&physical_key(branch, prefix)),
            crate::branch::read::BranchReadBound::latest(),
        )
        .expect("prefix scan")
        .iter()
        .map(|row| row.row().physical_key().user_key().to_vec())
        .collect()
}

#[cfg(feature = "perf-trace")]
fn cache_history_facts(
    runtime: &LifecycleCacheRuntime<CommitManualTimestampSource>,
    branch: BranchId,
    user_key: &[u8],
) -> Vec<(PhysicalKey, u64, u64, u64, Vec<u8>, bool)> {
    runtime
        .read_view_for_branch(branch)
        .expect("read view")
        .history(
            &physical_key(branch, user_key),
            crate::branch::read::BranchHistoryOptions::all(),
        )
        .expect("history")
        .iter()
        .map(|row| {
            (
                row.row().physical_key().clone(),
                row.row().commit_version().as_u64(),
                row.row().commit_timestamp().as_micros(),
                row.row().expires_at().as_micros(),
                row.row().value().to_vec(),
                row.row().is_tombstone(),
            )
        })
        .collect()
}

#[cfg(feature = "perf-trace")]
fn build_l0_history_tables_with_manual_flushes(
    runtime: &mut LifecycleCacheRuntime<CommitManualTimestampSource>,
    branch: BranchId,
    user_key: &[u8],
    table_count: usize,
) {
    assert!(table_count > 0);
    for index in 0..table_count {
        runtime
            .execute_cache_commit(
                put_batch(
                    branch,
                    physical_key(branch, user_key),
                    format!("history-value-{index}").into_bytes(),
                    Timestamp::from_micros(10_000 + u64::try_from(index).expect("index fits")),
                ),
                CommitBranchGenerationGuard::exact(
                    CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("cache history put commit");
        runtime
            .rotate_active_for_branch_for_maintenance(branch)
            .expect("rotate history table");
        runtime
            .enqueue_maintenance(MaintenanceTaskRequest::flush(branch))
            .expect("enqueue history flush");
        run_queued_flush(runtime);
        assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    }
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
    for index in 0..table_count {
        {
            let state = runtime
                .branch_catalog_mut_for_test()
                .branch_state_mut(
                    branch,
                    CommitBranchGenerationGuard::exact(
                        CommitBranchGeneration::new(1).expect("generation"),
                    ),
                )
                .expect("branch state");
            state
                .append_committed_rows_atomically(vec![active_pressure_put_row(
                    branch,
                    format!("scheduled-l0-trigger-{index}").as_bytes(),
                    1 + u64::try_from(index).expect("index fits"),
                    base_timestamp_micros + u64::try_from(index).expect("index fits"),
                    128,
                    0x51,
                )])
                .expect("append scheduled L0 fixture row");
            state.rotate_active();
        }
        runtime
            .enqueue_maintenance(MaintenanceTaskRequest::flush(branch))
            .expect("enqueue scheduled fixture flush");
        assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
        run_queued_flush(runtime);
        assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    }
    runtime.catch_up_commit_frontier_for_test(
        CommitVersion::new(u64::try_from(table_count).expect("table count fits")),
        Timestamp::from_micros(
            base_timestamp_micros + u64::try_from(table_count - 1).expect("table count fits"),
        ),
    );
}

fn build_l0_tables_with_manual_flushes_from(
    runtime: &mut LifecycleCacheRuntime<CommitManualTimestampSource>,
    branch: BranchId,
    table_count: usize,
    base_timestamp_micros: u64,
) {
    build_l0_tables_with_scheduled_flushes_from(
        runtime,
        branch,
        table_count,
        base_timestamp_micros,
    );
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
