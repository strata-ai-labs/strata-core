use super::shared::*;
use super::*;
use crate::branch::{
    BranchCompactionKind, BranchLocalState, BranchMaterializationRecovery, BranchReadBound,
    BranchRuntimeError, BranchScanBounds, BranchTableReferenceKind,
};
use strata_core_next::Timestamp;

#[test]
fn table_rewrite_requests_reject_bad_components_and_wrong_branch_execution() {
    let branch = branch_id(0x70);
    let other = branch_id(0x71);
    assert!(LifecycleMaterializationRequest::new(branch, 0, "").is_err());

    let mut state = BranchLocalState::empty(branch);
    let compaction =
        LifecycleCompactionRequest::new(other, BranchCompactionKind::CompactL0, "wrong-branch")
            .expect("compaction request");
    let materialization = LifecycleMaterializationRequest::new(other, 0, "wrong-materialization")
        .expect("materialization request");

    let compaction_error =
        compact_cache_branch(&mut state, &compaction).expect_err("wrong branch compaction");
    let materialization_error = materialize_cache_branch(&mut state, &materialization)
        .expect_err("wrong branch materialization");

    assert_eq!(
        compaction_error.code(),
        "failed_precondition.lifecycle.branch_runtime"
    );
    assert_eq!(
        materialization_error.code(),
        "failed_precondition.lifecycle.branch_runtime"
    );
    assert!(state.is_empty());
}

#[test]
fn table_rewrite_tasks_coalesce_by_exact_storage_scope() {
    let branch = branch_id(0x72);
    let other = branch_id(0x73);
    let mut executor = LifecycleMaintenanceExecutor::new(8).expect("executor");

    let first = executor
        .enqueue(open_state(), MaintenanceTaskRequest::compaction(branch, 0))
        .expect("first compaction");
    let duplicate = executor
        .enqueue(open_state(), MaintenanceTaskRequest::compaction(branch, 0))
        .expect("duplicate compaction");
    let other_level = executor
        .enqueue(open_state(), MaintenanceTaskRequest::compaction(branch, 1))
        .expect("other level");
    let other_branch = executor
        .enqueue(open_state(), MaintenanceTaskRequest::compaction(other, 0))
        .expect("other branch");
    let materialization = executor
        .enqueue(
            open_state(),
            MaintenanceTaskRequest::materialization_layer(branch, 0),
        )
        .expect("materialization");
    let duplicate_materialization = executor
        .enqueue(
            open_state(),
            MaintenanceTaskRequest::materialization_layer(branch, 0),
        )
        .expect("duplicate materialization");

    assert_eq!(duplicate.task_id(), first.task_id());
    assert_eq!(duplicate.status(), MaintenanceEnqueueStatus::Coalesced);
    assert_ne!(other_level.task_id(), first.task_id());
    assert_ne!(other_branch.task_id(), first.task_id());
    assert_eq!(
        duplicate_materialization.task_id(),
        materialization.task_id()
    );
    assert_eq!(executor.status().pending_tasks(), 4);
    assert_eq!(executor.stats().coalesced(), 2);
}

#[test]
fn queued_table_rewrites_are_blocked_after_close_without_mutating_branch() {
    let branch = branch_id(0x74);
    let mut runtime = cache_runtime(branch);
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 0))
        .expect("enqueue compaction");
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::materialization_layer(branch, 0))
        .expect("enqueue materialization");

    let close = runtime.close().expect("close");
    let compaction_error = runtime
        .run_next_compaction_maintenance()
        .expect_err("closed compaction");
    let materialization_error = runtime
        .run_next_materialization_maintenance()
        .expect_err("closed materialization");

    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(
        compaction_error.code(),
        "failed_precondition.lifecycle.state"
    );
    assert_eq!(
        materialization_error.code(),
        "failed_precondition.lifecycle.state"
    );
    assert_eq!(runtime.state(), LifecycleState::Closed);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    assert!(runtime.branch_state().is_empty());
}

#[test]
fn failed_table_rewrite_attempt_does_not_close_runtime() {
    let branch = branch_id(0x75);
    let other = branch_id(0x76);
    let mut runtime = cache_runtime(branch);
    let request =
        LifecycleCompactionRequest::new(other, BranchCompactionKind::CompactL0, "runtime-error")
            .expect("request");

    let error = runtime
        .compact_branch_tables(&request)
        .expect_err("wrong branch");

    assert_eq!(error.code(), "failed_precondition.lifecycle.branch_runtime");
    assert_eq!(runtime.state(), LifecycleState::Open);
    assert!(runtime.branch_state().is_empty());
}

#[test]
fn compaction_preserves_branch_timestamp_and_value_facts() {
    let branch = branch_id(0x77);
    let mut state = BranchLocalState::empty(branch);
    install_l0_table(
        &mut state,
        branch,
        "fact-left",
        vec![put_row(branch, b"fact", 1, 1_000, b"old")],
    );
    install_l0_table(
        &mut state,
        branch,
        "fact-right",
        vec![put_row(branch, b"fact", 2, 2_000, b"new")],
    );
    let key = physical_key(branch, b"fact");

    let outcome = compact_cache_branch(
        &mut state,
        &LifecycleCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "fact-rewrite")
            .expect("request"),
    )
    .expect("outcome");
    let visible = state
        .capture_read_view()
        .expect("view")
        .latest(&key)
        .expect("read")
        .expect("row")
        .row()
        .clone();

    assert_eq!(outcome.status(), LifecycleCompactionStatus::Completed);
    assert_eq!(visible.physical_key().branch_id(), branch);
    assert_eq!(visible.commit_timestamp(), Timestamp::from_micros(2_000));
    assert_eq!(visible.value(), b"new");
}

#[test]
fn materialization_handle_binds_source_identity_and_reports_intent_facts() {
    let parent = branch_id(0x78);
    let child = branch_id(0x79);
    let wrong_source = branch_id(0x7a);
    let mut parent_state = BranchLocalState::empty(parent);
    install_l0_table(
        &mut parent_state,
        parent,
        "handle-parent",
        vec![put_row(parent, b"key", 3, 3_000, b"value")],
    );
    let (mut child_state, _) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    let intent = child_state
        .mark_inherited_layer_materializing(0)
        .expect("intent");
    let handle = intent.handle();
    let mismatched = crate::branch::BranchMaterializationHandle::new(
        child,
        wrong_source,
        handle.fork_version(),
        handle.layer_index(),
    );
    let request = LifecycleMaterializationRequest::from_handle(handle, "handle-materialization")
        .expect("request");
    let bad_request =
        LifecycleMaterializationRequest::from_handle(mismatched, "bad-handle").expect("bad");

    let error = materialize_cache_branch(&mut child_state.clone(), &bad_request)
        .expect_err("mismatched source");
    let outcome = materialize_cache_branch(&mut child_state, &request).expect("materialized");

    assert_eq!(error.code(), "failed_precondition.lifecycle.branch_runtime");
    let branch_outcome = outcome.branch_outcome().expect("branch outcome");
    assert_eq!(outcome.intent().expect("intent").handle(), handle);
    assert_eq!(branch_outcome.source_branch_id(), parent);
    assert_eq!(branch_outcome.fork_version(), handle.fork_version());
    assert_eq!(
        branch_outcome.recovery(),
        BranchMaterializationRecovery::ReplacementVisibleLayerRemoved
    );
}

#[test]
fn materialization_handle_marks_active_layer_before_materializing() {
    let parent = branch_id(0x8a);
    let child = branch_id(0x8b);
    let mut parent_state = BranchLocalState::empty(parent);
    install_l0_table(
        &mut parent_state,
        parent,
        "handle-active-parent",
        vec![put_row(parent, b"key", 3, 3_000, b"value")],
    );
    let (mut child_state, fork) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    let handle =
        crate::branch::BranchMaterializationHandle::new(child, parent, fork.fork_version(), 0);
    let request =
        LifecycleMaterializationRequest::from_handle(handle, "handle-active").expect("request");

    let outcome = materialize_cache_branch(&mut child_state, &request).expect("materialized");

    assert!(outcome
        .intent()
        .expect("intent")
        .reachability_snapshot()
        .table_refs()
        .iter()
        .all(|table_ref| matches!(
            table_ref.reference_kind(),
            BranchTableReferenceKind::MaterializingSource { .. }
        )));
    assert_eq!(child_state.inherited_layer_count(), 0);
}

#[test]
fn materialization_preserves_range_scan_and_child_immutable_precedence() {
    let parent = branch_id(0x7b);
    let child = branch_id(0x7c);
    let mut child_state = materialization_read_state(parent, child);
    let shared_key = physical_key(child, b"shared");
    let lower = physical_key(child, b"scan-a");
    let upper = physical_key(child, b"scan-z");
    let before = child_state.capture_read_view().expect("before");
    let before_shared = before
        .latest(&shared_key)
        .expect("before shared")
        .expect("shared")
        .row()
        .clone();
    let before_range = scan_user_keys(
        &before
            .scan_range(
                &BranchScanBounds::closed(&lower, &upper).expect("range"),
                BranchReadBound::latest(),
            )
            .expect("range rows"),
    );

    let outcome = materialize_read_state(child, &mut child_state);
    assert_eq!(outcome.status(), LifecycleMaterializationStatus::Completed);
    let after = child_state.capture_read_view().expect("after");

    assert_eq!(
        after
            .latest(&shared_key)
            .expect("after shared")
            .expect("shared")
            .row(),
        &before_shared
    );
    assert_eq!(
        scan_user_keys(
            &after
                .scan_range(
                    &BranchScanBounds::closed(&lower, &upper).expect("range"),
                    BranchReadBound::latest(),
                )
                .expect("range rows")
        ),
        before_range
    );
}

#[test]
fn materializing_inherited_layers_report_pressure_without_mutating_reads() {
    let parent = branch_id(0x7d);
    let child = branch_id(0x7e);
    let mut child_state = materialization_read_state(parent, child);
    let key = physical_key(child, b"scan-a");
    let before = child_state
        .capture_read_view()
        .expect("before")
        .latest(&key)
        .expect("read")
        .expect("visible")
        .row()
        .clone();
    child_state
        .mark_inherited_layer_materializing(0)
        .expect("mark materializing");

    let pressure = collect_storage_pressure(&child_state, empty_maintenance_status());
    let after = child_state
        .capture_read_view()
        .expect("after")
        .latest(&key)
        .expect("read")
        .expect("visible")
        .row()
        .clone();

    assert_eq!(
        pressure.reason(),
        LifecycleStoragePressureReason::InheritedLayerBacklog
    );
    assert_eq!(pressure.inherited_layers(), 1);
    assert_eq!(before, after);
}

#[test]
fn table_rewrite_outcomes_count_deferred_completed_and_affected_objects() {
    let branch = branch_id(0x7f);
    let mut state = BranchLocalState::empty(branch);
    let deferred = compact_cache_branch(
        &mut state,
        &LifecycleCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "empty-count")
            .expect("request"),
    )
    .expect("deferred")
    .maintenance_outcome();
    assert_eq!(deferred.status(), MaintenanceOutcomeStatus::Deferred);
    assert_eq!(deferred.stats().maintenance_tasks(), 1);

    install_l0_table(
        &mut state,
        branch,
        "count-left",
        vec![put_row(branch, b"left", 1, 1_000, b"left")],
    );
    install_l0_table(
        &mut state,
        branch,
        "count-right",
        vec![put_row(branch, b"right", 2, 2_000, b"right")],
    );
    let completed = compact_cache_branch(
        &mut state,
        &LifecycleCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "count-rewrite")
            .expect("request"),
    )
    .expect("completed")
    .maintenance_outcome();

    assert_eq!(completed.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(completed.affected_objects(), 3);
    assert_eq!(completed.affected_object_names().len(), 3);
    assert!(completed
        .affected_object_names()
        .iter()
        .any(|identity| identity.contains("count-left")));
    assert_eq!(completed.bytes_reclaimed(), 0);
    assert!(!completed.retryable());
    assert!(completed.recovery_health().is_none());
}

#[test]
fn pressure_debug_output_uses_storage_vocabulary() {
    let branch = branch_id(0x80);
    let mut state = BranchLocalState::empty(branch);
    install_l0_table(
        &mut state,
        branch,
        "vocabulary-left",
        vec![put_row(branch, b"left", 1, 1_000, b"left")],
    );
    install_l0_table(
        &mut state,
        branch,
        "vocabulary-right",
        vec![put_row(branch, b"right", 2, 2_000, b"right")],
    );

    let debug = format!(
        "{:?}",
        collect_storage_pressure(&state, empty_maintenance_status())
    );

    for forbidden in ["write stall", "customer", "tenant", "redis", "query"] {
        assert!(!debug.to_ascii_lowercase().contains(forbidden));
    }
}

#[test]
fn storage_pressure_blocks_mutating_admission_for_large_table_backlog() {
    let branch = branch_id(0x82);
    let mut state = BranchLocalState::empty(branch);
    for index in 0_u64..8 {
        install_l0_table(
            &mut state,
            branch,
            &format!("blocking-backlog-{index}"),
            vec![put_row(
                branch,
                format!("key-{index}").as_bytes(),
                index + 1,
                1_000 + index,
                b"value",
            )],
        );
    }

    let pressure = collect_storage_pressure(&state, empty_maintenance_status());

    assert_eq!(
        pressure.severity(),
        LifecycleStoragePressureSeverity::BlockMutatingAdmission
    );
    assert_eq!(
        pressure.reason(),
        LifecycleStoragePressureReason::LevelZeroTableBacklog
    );
    assert!(matches!(
        pressure.suggested_task().map(MaintenanceTaskRequest::kind),
        Some(MaintenanceTaskKind::Compaction)
    ));
}

#[test]
fn branch_pruning_policies_remain_below_lifecycle_until_retention_proof_exists() {
    let branch = branch_id(0x81);
    let mut state = BranchLocalState::empty(branch);
    install_l0_table(
        &mut state,
        branch,
        "prune-left",
        vec![put_row(branch, b"key", 1, 1_000, b"old")],
    );
    install_l0_table(
        &mut state,
        branch,
        "prune-right",
        vec![put_row(branch, b"key", 2, 2_000, b"new")],
    );

    let request = crate::branch::BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0,
        "prune-rewrite",
    )
    .expect("request")
    .with_retention_policy(crate::branch::BranchCompactionRetentionPolicy::DropOlderVersions);
    let error = state
        .compact_branch_owned_tables(&request)
        .expect_err("retention proof required");

    assert!(matches!(
        error,
        BranchRuntimeError::InvalidCompaction { .. }
    ));
    assert_eq!(state.owned_table_count(), 2);
}

#[test]
fn materialization_branch_errors_preserve_source_chain_and_code() {
    let state_branch = branch_id(0x82);
    let request_branch = branch_id(0x83);
    let mut state = BranchLocalState::empty(state_branch);
    let request = LifecycleMaterializationRequest::new(request_branch, 0, "materialization-error")
        .expect("request");

    let error = materialize_cache_branch(&mut state, &request).expect_err("wrong branch");

    assert_eq!(error.code(), "failed_precondition.lifecycle.branch_runtime");
    match error {
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::BranchRuntime,
            source: Some(_),
            ..
        } => {}
        other => panic!("unexpected error shape: {other:?}"),
    }
}
