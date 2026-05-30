mod publication_plan;
mod remaining;
mod row_pruning;
mod shared;

use self::shared::*;
use super::*;
use crate::branch::facts::BranchLevel;
use crate::branch::read::{BranchHistoryOptions, BranchReadBound, BranchScanBounds};
use crate::branch::state::compaction::{BranchCompactionKind, BranchCompactionNoopReason};
use crate::branch::state::materialization::BranchMaterializationRecovery;
use crate::branch::state::BranchLocalState;
use strata_core_next::Timestamp;

#[test]
fn table_rewrite_requests_validate_opaque_identity_components() {
    let branch = branch_id(0x41);

    let compaction =
        LifecycleCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "rewrite-seed")
            .expect("compaction request");
    assert_eq!(compaction.branch_id(), branch);
    assert_eq!(compaction.kind(), BranchCompactionKind::CompactL0);
    assert_eq!(compaction.output_identity_seed(), "rewrite-seed");
    assert_eq!(
        compaction.durability(),
        LifecycleTableRewriteDurability::VolatileOnly
    );
    assert!(LifecycleCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "").is_err());
    assert!(
        LifecycleCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "bad/seed")
            .is_err()
    );

    let materialization =
        LifecycleMaterializationRequest::new(branch, 3, "materialize-seed").expect("request");
    assert_eq!(materialization.child_branch_id(), branch);
    assert_eq!(materialization.layer_index(), 3);
    assert_eq!(materialization.output_identity_prefix(), "materialize-seed");
    assert_eq!(
        materialization.durability(),
        LifecycleTableRewriteDurability::VolatileOnly
    );
    assert!(LifecycleMaterializationRequest::new(branch, 0, "bad/prefix").is_err());
}

#[test]
fn maintenance_tasks_map_to_table_rewrite_requests() {
    let branch = branch_id(0x42);
    let compaction_task =
        MaintenanceTask::new_for_test(1, MaintenanceTaskRequest::compaction(branch, 0))
            .expect("task");
    let compaction =
        compaction_request_from_maintenance_task(&compaction_task).expect("compaction");
    assert_eq!(compaction.branch_id(), branch);
    assert_eq!(compaction.kind(), BranchCompactionKind::CompactL0ToLevelOne);
    assert!(compaction
        .output_identity_seed()
        .contains("maintenance-compaction"));

    let materialization_task =
        MaintenanceTask::new_for_test(2, MaintenanceTaskRequest::materialization_layer(branch, 2))
            .expect("task");
    let materialization = materialization_request_from_maintenance_task(&materialization_task)
        .expect("materialization");
    assert_eq!(materialization.child_branch_id(), branch);
    assert_eq!(materialization.layer_index(), 2);
    assert!(materialization
        .output_identity_prefix()
        .contains("maintenance-materialization"));

    let wrong_kind =
        MaintenanceTask::new_for_test(3, MaintenanceTaskRequest::flush(branch)).expect("task");
    assert!(compaction_request_from_maintenance_task(&wrong_kind).is_err());
    assert!(materialization_request_from_maintenance_task(&wrong_kind).is_err());
}

#[test]
fn cache_compaction_defers_without_a_candidate() {
    let branch = branch_id(0x43);
    let mut state = BranchLocalState::empty(branch);
    let request =
        LifecycleCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "empty-rewrite")
            .expect("request");

    let outcome = compact_cache_branch(&mut state, &request).expect("outcome");

    assert_eq!(
        outcome.status(),
        LifecycleCompactionStatus::DeferredNoCandidate
    );
    assert_eq!(outcome.branch_id(), branch);
    assert!(outcome.plan().candidate().is_none());
    assert!(outcome.branch_outcome().noop_reason().is_some());
    assert!(!outcome.checkpoint_required());
    assert!(outcome.recovery_health().is_none());
    assert_eq!(
        outcome.maintenance_outcome().status(),
        MaintenanceOutcomeStatus::Deferred
    );
    assert!(state.is_empty());
}

#[test]
fn cache_compaction_installs_replacement_and_preserves_reads() {
    let branch = branch_id(0x44);
    let mut state = BranchLocalState::empty(branch);
    let key = physical_key(branch, b"history");
    let newer = put_row(branch, b"history", 5, 5_000, b"newer");
    let older = put_row(branch, b"history", 2, 2_000, b"older");
    install_l0_table(
        &mut state,
        branch,
        "cache-rewrite-newer",
        vec![newer.clone()],
    );
    install_l0_table(&mut state, branch, "cache-rewrite-older", vec![older]);

    let request =
        LifecycleCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "cache-rewrite")
            .expect("request");
    let outcome = compact_cache_branch(&mut state, &request).expect("outcome");

    assert_eq!(outcome.status(), LifecycleCompactionStatus::Completed);
    assert!(!outcome.checkpoint_required());
    assert_eq!(outcome.branch_outcome().removed_refs().len(), 2);
    assert_eq!(outcome.branch_outcome().output_refs().len(), 1);
    assert_eq!(state.owned_table_count(), 1);
    assert_eq!(
        outcome.maintenance_outcome().status(),
        MaintenanceOutcomeStatus::Completed
    );
    assert!(!outcome.maintenance_outcome().checkpoint_required());
    assert_eq!(outcome.maintenance_outcome().affected_objects(), 3);

    let view = state.capture_read_view().expect("view");
    let visible = view.latest(&key).expect("read").expect("visible");
    assert_eq!(visible.row(), &newer);
}

#[test]
fn compaction_candidate_reports_level_movement_and_overlap_refs() {
    let branch = branch_id(0x5c);
    let mut state = BranchLocalState::empty(branch);
    install_owned_table(
        &mut state,
        branch,
        BranchLevel::new(1),
        "overlap-base",
        vec![put_row(branch, b"overlap", 1, 1_000, b"base")],
    );
    install_l0_table(
        &mut state,
        branch,
        "overlap-new",
        vec![put_row(branch, b"overlap", 2, 2_000, b"new")],
    );
    let request = LifecycleCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0ToLevelOne,
        "overlap-rewrite",
    )
    .expect("request");

    let outcome = compact_cache_branch(&mut state, &request).expect("outcome");
    let candidate = outcome.plan().candidate().expect("candidate");

    assert_eq!(candidate.input_refs().len(), 1);
    assert_eq!(candidate.overlap_refs().len(), 1);
    assert_eq!(candidate.output_level(), BranchLevel::new(1));
    assert_eq!(candidate.source_count(), 2);
    assert_eq!(candidate.input_row_count(), 2);
    assert_eq!(outcome.branch_outcome().removed_refs().len(), 2);
    assert_eq!(outcome.branch_outcome().output_refs().len(), 1);
    assert!(outcome.branch_outcome().table_report().is_some());
}

#[test]
fn nonzero_level_compaction_reports_output_level() {
    let branch = branch_id(0x5d);
    let mut state = BranchLocalState::empty(branch);
    install_owned_table(
        &mut state,
        branch,
        BranchLevel::new(2),
        "nonzero-base",
        vec![put_row(branch, b"level-key", 1, 1_000, b"base")],
    );
    install_owned_table(
        &mut state,
        branch,
        BranchLevel::new(1),
        "nonzero-input",
        vec![put_row(branch, b"level-key", 2, 2_000, b"input")],
    );
    let request = LifecycleCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::new(1),
            table_index: 0,
        },
        "nonzero-rewrite",
    )
    .expect("request");

    let outcome = compact_cache_branch(&mut state, &request).expect("outcome");
    let candidate = outcome.branch_outcome().candidate().expect("candidate");

    assert_eq!(candidate.output_level(), BranchLevel::new(2));
    assert_eq!(candidate.input_refs().len(), 1);
    assert_eq!(candidate.overlap_refs().len(), 1);
    assert_eq!(candidate.source_count(), 2);
}

#[test]
fn compaction_no_candidate_reasons_are_deferred() {
    let branch = branch_id(0x5e);
    let mut state = BranchLocalState::empty(branch);
    install_l0_table(
        &mut state,
        branch,
        "single-input",
        vec![put_row(branch, b"single", 1, 1_000, b"value")],
    );

    let single = compact_cache_branch(
        &mut state,
        &LifecycleCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "single-rewrite")
            .expect("single request"),
    )
    .expect("single outcome");
    assert_eq!(
        single.status(),
        LifecycleCompactionStatus::DeferredNoCandidate
    );
    assert_eq!(
        single.plan().noop_reason(),
        Some(BranchCompactionNoopReason::NotEnoughInputTables)
    );

    let last = compact_cache_branch(
        &mut state,
        &LifecycleCompactionRequest::new(
            branch,
            BranchCompactionKind::CompactLevel {
                level: BranchLevel::new(6),
                table_index: 0,
            },
            "last-rewrite",
        )
        .expect("last request"),
    )
    .expect("last outcome");
    assert_eq!(
        last.status(),
        LifecycleCompactionStatus::DeferredNoCandidate
    );
    assert_eq!(
        last.plan().noop_reason(),
        Some(BranchCompactionNoopReason::LastLevel)
    );
}

#[test]
fn compaction_preserves_latest_history_and_tombstone_facts() {
    let branch = branch_id(0x5f);
    let mut state = read_shape_state(branch);
    let history_key = physical_key(branch, b"history");
    let scan_key = physical_key(branch, b"scan-a");
    let before = state.capture_read_view().expect("before");
    let before_latest = before
        .latest(&scan_key)
        .expect("before latest")
        .expect("visible")
        .row()
        .clone();
    let before_history = history_versions(
        &before
            .history(&history_key, BranchHistoryOptions::all())
            .expect("history"),
    );

    let outcome = compact_read_shape_state(branch, &mut state);
    assert_eq!(outcome.status(), LifecycleCompactionStatus::Completed);

    let after = state.capture_read_view().expect("after");
    assert_eq!(
        after
            .latest(&scan_key)
            .expect("after latest")
            .expect("visible")
            .row(),
        &before_latest
    );
    assert_eq!(
        history_versions(
            &after
                .history(&history_key, BranchHistoryOptions::all())
                .expect("history")
        ),
        before_history
    );
    assert!(after
        .history(&history_key, BranchHistoryOptions::all())
        .expect("history")
        .iter()
        .any(|row| row.row().is_tombstone()));
}

#[test]
fn compaction_preserves_scan_shapes_and_expiring_rows() {
    let branch = branch_id(0x6a);
    let mut state = read_shape_state(branch);
    let scan_prefix = physical_key(branch, b"scan-");
    let range_lower = physical_key(branch, b"scan-a");
    let range_upper = physical_key(branch, b"scan-z");
    let expiring_key = physical_key(branch, b"scan-b");
    let before = state.capture_read_view().expect("before");
    let before_prefix = scan_user_keys(
        &before
            .scan_prefix(
                &BranchScanBounds::prefix(&scan_prefix),
                BranchReadBound::latest(),
            )
            .expect("prefix"),
    );
    let before_range = scan_user_keys(
        &before
            .scan_range(
                &BranchScanBounds::closed(&range_lower, &range_upper).expect("range"),
                BranchReadBound::latest(),
            )
            .expect("range rows"),
    );

    let outcome = compact_read_shape_state(branch, &mut state);
    assert_eq!(outcome.status(), LifecycleCompactionStatus::Completed);

    let after = state.capture_read_view().expect("after");
    assert_eq!(
        scan_user_keys(
            &after
                .scan_prefix(
                    &BranchScanBounds::prefix(&scan_prefix),
                    BranchReadBound::latest(),
                )
                .expect("prefix")
        ),
        before_prefix
    );
    assert_eq!(
        scan_user_keys(
            &after
                .scan_range(
                    &BranchScanBounds::closed(&range_lower, &range_upper).expect("range"),
                    BranchReadBound::latest(),
                )
                .expect("range rows")
        ),
        before_range
    );
    assert!(after
        .history(&expiring_key, BranchHistoryOptions::all())
        .expect("expired history")
        .iter()
        .any(|row| row.row().expires_at() == Timestamp::from_micros(4_500)));
}

#[test]
fn durable_compaction_reports_checkpoint_debt() {
    let branch = branch_id(0x45);
    let mut state = BranchLocalState::empty(branch);
    install_l0_table(
        &mut state,
        branch,
        "durable-rewrite-left",
        vec![put_row(branch, b"left", 1, 1_000, b"left")],
    );
    install_l0_table(
        &mut state,
        branch,
        "durable-rewrite-right",
        vec![put_row(branch, b"right", 2, 2_000, b"right")],
    );
    let request =
        LifecycleCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "durable-rewrite")
            .expect("request");

    let outcome = compact_durable_branch(&mut state, &request).expect("outcome");

    assert_eq!(
        outcome.status(),
        LifecycleCompactionStatus::CompletedCheckpointRequired
    );
    assert!(outcome.checkpoint_required());
    assert_eq!(
        outcome.maintenance_outcome().status(),
        MaintenanceOutcomeStatus::Completed
    );
    assert!(outcome.maintenance_outcome().checkpoint_required());
}

#[test]
fn materialization_defers_when_no_inherited_layer_exists() {
    let branch = branch_id(0x46);
    let mut state = BranchLocalState::empty(branch);
    let request = LifecycleMaterializationRequest::new(branch, 0, "missing-materialization")
        .expect("request");

    let outcome = materialize_cache_branch(&mut state, &request).expect("outcome");

    assert_eq!(
        outcome.status(),
        LifecycleMaterializationStatus::DeferredNoLayer
    );
    assert_eq!(outcome.child_branch_id(), branch);
    assert_eq!(outcome.layer_index(), 0);
    assert!(outcome.intent().is_none());
    assert!(outcome.branch_outcome().is_none());
    assert!(!outcome.checkpoint_required());
    assert!(outcome.recovery_health().is_none());
    assert_eq!(
        outcome.maintenance_outcome().status(),
        MaintenanceOutcomeStatus::Deferred
    );
}

#[test]
fn cache_materialization_removes_layer_and_preserves_child_precedence() {
    let parent = branch_id(0x47);
    let child = branch_id(0x48);
    let shared_key = physical_key(child, b"shared");
    let inherited_key = physical_key(child, b"inherited");
    let parent_shared = put_row(parent, b"shared", 1, 1_000, b"parent");
    let parent_only = put_row(parent, b"inherited", 2, 2_000, b"inherited");
    let child_shared = put_row(child, b"shared", 3, 3_000, b"child");
    let mut parent_state = BranchLocalState::empty(parent);
    install_l0_table(
        &mut parent_state,
        parent,
        "materialize-parent",
        vec![parent_shared, parent_only.clone()],
    );
    let (mut child_state, _) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    child_state
        .append_committed_row(child_shared.clone())
        .expect("append child row");

    let request =
        LifecycleMaterializationRequest::new(child, 0, "cache-materialize").expect("request");
    let outcome = materialize_cache_branch(&mut child_state, &request).expect("outcome");

    assert_eq!(outcome.status(), LifecycleMaterializationStatus::Completed);
    assert!(!outcome.checkpoint_required());
    assert!(outcome.intent().is_some());
    let branch_outcome = outcome.branch_outcome().expect("branch outcome");
    assert_eq!(
        branch_outcome.recovery(),
        BranchMaterializationRecovery::ReplacementVisibleLayerRemoved
    );
    assert_eq!(child_state.inherited_layer_count(), 0);
    assert!(child_state.owned_table_count() > 0);

    let view = child_state.capture_read_view().expect("view");
    assert_eq!(
        view.latest(&shared_key)
            .expect("shared read")
            .expect("shared visible")
            .row(),
        &child_shared
    );
    assert_eq!(
        view.latest(&inherited_key)
            .expect("inherited read")
            .expect("inherited visible")
            .row()
            .value(),
        parent_only.value()
    );
}

#[test]
fn materialization_retry_with_source_identity_reports_already_materialized() {
    let parent = branch_id(0x58);
    let child = branch_id(0x59);
    let mut parent_state = BranchLocalState::empty(parent);
    install_l0_table(
        &mut parent_state,
        parent,
        "retry-materialize-parent",
        vec![put_row(parent, b"key", 1, 1_000, b"value")],
    );
    let (mut child_state, _) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    let request =
        LifecycleMaterializationRequest::new(child, 0, "retry-materialize").expect("request");
    let completed = materialize_cache_branch(&mut child_state, &request).expect("first");
    let handle = completed.intent().expect("intent").handle();
    assert_eq!(child_state.inherited_layer_count(), 0);

    let retry =
        LifecycleMaterializationRequest::from_handle(handle, "retry-materialize").expect("retry");
    let retried = materialize_cache_branch(&mut child_state, &retry).expect("second");

    assert_eq!(
        retried.status(),
        LifecycleMaterializationStatus::AlreadyMaterialized
    );
    assert_eq!(
        retried.branch_outcome().expect("branch outcome").recovery(),
        BranchMaterializationRecovery::LayerAlreadyMaterialized
    );
    assert_eq!(retried.child_branch_id(), child);
    assert_eq!(retried.layer_index(), 0);
    assert!(!retried.checkpoint_required());
}

#[test]
fn materialization_preserves_child_precedence_and_fork_gate() {
    let parent = branch_id(0x60);
    let child = branch_id(0x61);
    let mut child_state = materialization_read_state(parent, child);
    let shared_key = physical_key(child, b"shared");
    let post_fork_key = physical_key(child, b"post-fork");
    let before = child_state.capture_read_view().expect("before");
    let before_shared = before
        .latest(&shared_key)
        .expect("before shared")
        .expect("shared")
        .row()
        .clone();
    assert!(before
        .latest(&post_fork_key)
        .expect("post fork read")
        .is_none());

    let outcome = materialize_read_state(child, &mut child_state);
    assert_eq!(outcome.status(), LifecycleMaterializationStatus::Completed);
    assert_eq!(child_state.inherited_layer_count(), 0);

    let after = child_state.capture_read_view().expect("after");
    assert_eq!(
        after
            .latest(&shared_key)
            .expect("after shared")
            .expect("shared")
            .row(),
        &before_shared
    );
    assert!(after
        .latest(&post_fork_key)
        .expect("post fork read")
        .is_none());
}

#[test]
fn materialization_preserves_history_scans_and_tombstones() {
    let parent = branch_id(0x6b);
    let child = branch_id(0x6c);
    let mut child_state = materialization_read_state(parent, child);
    let history_key = physical_key(child, b"history");
    let scan_prefix = physical_key(child, b"scan-");
    let before = child_state.capture_read_view().expect("before");
    let before_history = history_versions(
        &before
            .history(&history_key, BranchHistoryOptions::all())
            .expect("history"),
    );
    let before_prefix = scan_user_keys(
        &before
            .scan_prefix(
                &BranchScanBounds::prefix(&scan_prefix),
                BranchReadBound::latest(),
            )
            .expect("prefix"),
    );

    let outcome = materialize_read_state(child, &mut child_state);
    assert_eq!(outcome.status(), LifecycleMaterializationStatus::Completed);
    assert_eq!(child_state.inherited_layer_count(), 0);

    let after = child_state.capture_read_view().expect("after");
    assert_eq!(
        history_versions(
            &after
                .history(&history_key, BranchHistoryOptions::all())
                .expect("history")
        ),
        before_history
    );
    assert_eq!(
        scan_user_keys(
            &after
                .scan_prefix(
                    &BranchScanBounds::prefix(&scan_prefix),
                    BranchReadBound::latest(),
                )
                .expect("prefix")
        ),
        before_prefix
    );
    assert!(after
        .history(&history_key, BranchHistoryOptions::all())
        .expect("history")
        .iter()
        .any(|row| row.row().is_tombstone()));
}

#[test]
fn durable_materialization_reports_checkpoint_debt() {
    let parent = branch_id(0x49);
    let child = branch_id(0x4a);
    let mut parent_state = BranchLocalState::empty(parent);
    install_l0_table(
        &mut parent_state,
        parent,
        "durable-materialize-parent",
        vec![put_row(parent, b"key", 1, 1_000, b"value")],
    );
    let (mut child_state, _) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    let request =
        LifecycleMaterializationRequest::new(child, 0, "durable-materialize").expect("request");

    let outcome = materialize_durable_branch(&mut child_state, &request).expect("outcome");

    assert_eq!(
        outcome.status(),
        LifecycleMaterializationStatus::CompletedCheckpointRequired
    );
    assert!(outcome.checkpoint_required());
    assert_eq!(
        outcome.maintenance_outcome().status(),
        MaintenanceOutcomeStatus::Completed
    );
}

#[test]
fn cache_runtime_rejects_table_rewrites_after_close() {
    let branch = branch_id(0x62);
    let mut runtime = cache_runtime(branch);
    runtime.close().expect("close");
    let compaction =
        LifecycleCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "closed-rewrite")
            .expect("compaction request");
    let materialization = LifecycleMaterializationRequest::new(branch, 0, "closed-materialization")
        .expect("materialization request");

    let compaction_error = runtime
        .compact_branch_tables(&compaction)
        .expect_err("closed compaction");
    let materialization_error = runtime
        .materialize_inherited_layer(&materialization)
        .expect_err("closed materialization");

    assert_eq!(
        compaction_error.code(),
        "failed_precondition.lifecycle.state"
    );
    assert_eq!(
        materialization_error.code(),
        "failed_precondition.lifecycle.state"
    );
}

#[test]
fn compaction_branch_errors_preserve_source_chain_and_code() {
    let state_branch = branch_id(0x63);
    let request_branch = branch_id(0x64);
    let mut state = BranchLocalState::empty(state_branch);
    let request = LifecycleCompactionRequest::new(
        request_branch,
        BranchCompactionKind::CompactL0,
        "wrong-branch-rewrite",
    )
    .expect("request");

    let error = compact_cache_branch(&mut state, &request).expect_err("wrong branch");

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

#[test]
fn storage_pressure_suggests_the_next_table_rewrite_or_flush() {
    let branch = branch_id(0x4b);
    let mut frozen_state = BranchLocalState::empty(branch);
    frozen_state
        .append_committed_row(put_row(branch, b"frozen", 1, 1_000, b"value"))
        .expect("append");
    frozen_state.rotate_active();

    let frozen_pressure = collect_storage_pressure(&frozen_state, empty_maintenance_status());
    assert_eq!(frozen_pressure.branch_id(), branch);
    assert_eq!(
        frozen_pressure.reason(),
        LifecycleStoragePressureReason::FrozenBacklog
    );
    assert_eq!(
        frozen_pressure.severity(),
        LifecycleStoragePressureSeverity::Urgent
    );
    assert_eq!(frozen_pressure.frozen_tables(), 1);
    assert!(matches!(
        frozen_pressure
            .suggested_task()
            .map(MaintenanceTaskRequest::kind),
        Some(MaintenanceTaskKind::Flush)
    ));

    let mut compact_state = BranchLocalState::empty(branch);
    install_l0_table(
        &mut compact_state,
        branch,
        "pressure-left",
        vec![put_row(branch, b"left", 2, 2_000, b"left")],
    );
    install_l0_table(
        &mut compact_state,
        branch,
        "pressure-right",
        vec![put_row(branch, b"right", 3, 3_000, b"right")],
    );
    let compact_pressure = collect_storage_pressure(&compact_state, empty_maintenance_status());
    assert_eq!(
        compact_pressure.reason(),
        LifecycleStoragePressureReason::LevelZeroTableBacklog
    );
    assert_eq!(
        compact_pressure.severity(),
        LifecycleStoragePressureSeverity::Background
    );
    assert_eq!(compact_pressure.level_zero_tables(), 2);
    assert_eq!(compact_pressure.owned_tables(), 2);
    assert!(matches!(
        compact_pressure
            .suggested_task()
            .map(MaintenanceTaskRequest::kind),
        Some(MaintenanceTaskKind::Compaction)
    ));

    let parent = branch_id(0x4c);
    let child = branch_id(0x4d);
    let mut parent_state = BranchLocalState::empty(parent);
    install_l0_table(
        &mut parent_state,
        parent,
        "pressure-parent",
        vec![put_row(parent, b"inherited", 4, 4_000, b"value")],
    );
    let (child_state, _) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    let inherited_pressure = collect_storage_pressure(&child_state, empty_maintenance_status());
    assert_eq!(
        inherited_pressure.reason(),
        LifecycleStoragePressureReason::InheritedLayerBacklog
    );
    assert_eq!(inherited_pressure.inherited_layers(), 1);
    assert!(matches!(
        inherited_pressure
            .suggested_task()
            .map(MaintenanceTaskRequest::kind),
        Some(MaintenanceTaskKind::Materialization)
    ));
    assert_eq!(inherited_pressure.active_rows(), 0);

    let mut executor = LifecycleMaintenanceExecutor::new(8).expect("executor");
    executor
        .enqueue(open_state(), MaintenanceTaskRequest::health_collection())
        .expect("enqueue");
    let idle_pressure =
        collect_storage_pressure(&BranchLocalState::empty(branch), executor.status());
    assert_eq!(
        idle_pressure.reason(),
        LifecycleStoragePressureReason::MaintenanceQueueBacklog
    );
    assert_eq!(idle_pressure.pending_maintenance(), 1);
    assert!(idle_pressure.suggested_task().is_none());
}

#[test]
fn storage_pressure_reports_none_urgent_and_deterministic_facts() {
    let branch = branch_id(0x65);
    let empty = BranchLocalState::empty(branch);
    let empty_pressure = collect_storage_pressure(&empty, empty_maintenance_status());
    assert_eq!(
        empty_pressure.severity(),
        LifecycleStoragePressureSeverity::None
    );
    assert_eq!(
        empty_pressure.reason(),
        LifecycleStoragePressureReason::None
    );
    assert!(empty_pressure.suggested_task().is_none());

    let mut urgent = BranchLocalState::empty(branch);
    for index in 0_u64..4 {
        install_l0_table(
            &mut urgent,
            branch,
            &format!("urgent-rewrite-{index}"),
            vec![put_row(
                branch,
                format!("urgent-{index}").as_bytes(),
                index + 1,
                (index + 1) * 1_000,
                b"value",
            )],
        );
    }
    let first = collect_storage_pressure(&urgent, empty_maintenance_status());
    let second = collect_storage_pressure(&urgent, empty_maintenance_status());
    assert_eq!(first, second);
    assert_eq!(first.severity(), LifecycleStoragePressureSeverity::Urgent);
    assert_eq!(
        first.reason(),
        LifecycleStoragePressureReason::LevelZeroTableBacklog
    );
    assert_eq!(first.level_zero_tables(), 4);
    assert!(matches!(
        first.suggested_task().map(MaintenanceTaskRequest::kind),
        Some(MaintenanceTaskKind::Compaction)
    ));
}

#[test]
fn durable_rewrite_outcomes_report_checkpoint_debt_without_reclaim() {
    let branch = branch_id(0x66);
    let mut state = BranchLocalState::empty(branch);
    install_l0_table(
        &mut state,
        branch,
        "durable-debt-left",
        vec![put_row(branch, b"left", 1, 1_000, b"left")],
    );
    install_l0_table(
        &mut state,
        branch,
        "durable-debt-right",
        vec![put_row(branch, b"right", 2, 2_000, b"right")],
    );
    let compaction = compact_durable_branch(
        &mut state,
        &LifecycleCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "durable-debt")
            .expect("request"),
    )
    .expect("compaction");
    let maintenance = compaction.maintenance_outcome();

    assert_eq!(maintenance.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(maintenance.task_kind(), MaintenanceTaskKind::Compaction);
    assert_eq!(maintenance.bytes_reclaimed(), 0);
    assert!(!maintenance.retryable());
    assert!(compaction.checkpoint_required());
    assert!(maintenance.checkpoint_required());
}

#[test]
fn queued_cache_compaction_skips_other_task_kinds() {
    let branch = branch_id(0x5a);
    let mut runtime = cache_runtime(branch);
    let flush = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::flush(branch))
        .expect("enqueue flush");
    let compaction = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 0))
        .expect("enqueue compaction");

    let outcome = runtime
        .run_next_compaction_maintenance()
        .expect("run compaction")
        .expect("compaction outcome");

    assert_eq!(outcome.task_id(), Some(compaction.task_id()));
    assert_eq!(outcome.task_kind(), MaintenanceTaskKind::Compaction);
    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Deferred);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);

    let remaining = runtime
        .run_next_flush_maintenance()
        .expect("run flush")
        .expect("flush outcome");
    assert_eq!(remaining.task_id(), Some(flush.task_id()));
    assert_eq!(remaining.task_kind(), MaintenanceTaskKind::Flush);
}

#[test]
fn queued_cache_materialization_skips_other_task_kinds() {
    let branch = branch_id(0x5b);
    let mut runtime = cache_runtime(branch);
    let flush = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::flush(branch))
        .expect("enqueue flush");
    let materialization = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::materialization_layer(branch, 0))
        .expect("enqueue materialization");

    let outcome = runtime
        .run_next_materialization_maintenance()
        .expect("run materialization")
        .expect("materialization outcome");

    assert_eq!(outcome.task_id(), Some(materialization.task_id()));
    assert_eq!(outcome.task_kind(), MaintenanceTaskKind::Materialization);
    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Deferred);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);

    let remaining = runtime
        .run_next_flush_maintenance()
        .expect("run flush")
        .expect("flush outcome");
    assert_eq!(remaining.task_id(), Some(flush.task_id()));
    assert_eq!(remaining.task_kind(), MaintenanceTaskKind::Flush);
}
