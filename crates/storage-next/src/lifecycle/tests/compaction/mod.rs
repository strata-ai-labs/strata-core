#![allow(clippy::too_many_lines)]

mod publication_plan;
mod remaining;
mod row_pruning;
mod shared;

use self::shared::*;
use super::*;
use crate::branch::config::BranchRuntimeConfig;
use crate::branch::facts::{BranchLevel, BranchLevelTableCount};
use crate::branch::read::{BranchHistoryOptions, BranchReadBound, BranchScanBounds};
use crate::branch::state::compaction::{
    BranchCompactionKind, BranchCompactionNoopReason, BranchCompactionOperation,
};
use crate::branch::state::materialization::BranchMaterializationRecovery;
use crate::branch::state::BranchLocalState;
#[cfg(feature = "perf-trace")]
use crate::lifecycle::compaction::defer_compaction_for_resource_policy;
use crate::lifecycle::compaction::{
    compact_cache_branch_to_fixed_point, current_compaction_request_from_maintenance_task,
    current_compaction_request_from_maintenance_task_with_budget, nonzero_compaction_pressure,
    nonzero_level_target_bytes, nonzero_level_targets_from_level_bytes,
    LifecycleCompactionDrainRequest,
};
use crate::lifecycle::flush::{
    flush_cache_branch, FlushFrozenRequest, FlushTableIdentitySeed, FlushTableObjectId,
};
use crate::lifecycle::StorageBudgetPressureSeverity;
use crate::table::TablePhysicalKeyBytes;
use strata_core_next::{BranchId, Timestamp};

const fn mib(value: u64) -> u64 {
    value * 1024 * 1024
}

const fn gib(value: u64) -> u64 {
    value * 1024 * 1024 * 1024
}

const fn tib(value: u64) -> u64 {
    value * 1024 * 1024 * 1024 * 1024
}

#[cfg_attr(not(feature = "perf-trace"), allow(dead_code))]
fn target_bytes_for_state(state: &BranchLocalState) -> Vec<u64> {
    let level_bytes = state
        .owned_levels()
        .iter()
        .map(|tables| {
            tables.iter().fold(0u64, |total, table| {
                total.saturating_add(table.facts().byte_count())
            })
        })
        .collect::<Vec<_>>();
    nonzero_level_targets_from_level_bytes(&level_bytes)
}

fn table_last_physical_key(table: &crate::branch::read::BranchOwnedTable) -> TablePhysicalKeyBytes {
    TablePhysicalKeyBytes::from_encoded_internal_key(table.facts().key_range().last_key())
}

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
    let nonzero_compaction_task =
        MaintenanceTask::new_for_test(4, MaintenanceTaskRequest::compaction(branch, 1))
            .expect("task");
    assert!(compaction_request_from_maintenance_task(&nonzero_compaction_task).is_err());

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
fn nonzero_compaction_level_targets_match_segmented_empty_fixture() {
    let targets = nonzero_level_targets_from_level_bytes(&[0, 0, 0, 0, 0, 0, 0]);

    assert_eq!(
        targets,
        vec![
            0,
            mib(1),
            mib(10),
            mib(100),
            mib(1_000),
            mib(10_000),
            mib(100_000)
        ]
    );
}

#[test]
fn nonzero_compaction_level_targets_match_segmented_shallow_fixture() {
    let targets = nonzero_level_targets_from_level_bytes(&[0, mib(100), 0, 0, 0, 0, 0]);

    assert_eq!(
        targets,
        vec![
            0,
            mib(100),
            mib(1_000),
            mib(10_000),
            mib(100_000),
            mib(1_000_000),
            mib(10_000_000),
        ]
    );
}

#[test]
fn nonzero_compaction_level_targets_match_segmented_deep_fixture() {
    let deep_bytes = 10 * tib(1);
    let targets = nonzero_level_targets_from_level_bytes(&[0, 0, 0, 0, 0, 0, deep_bytes]);
    let expected_base = deep_bytes / 100_000;

    assert_eq!(targets[1], expected_base);
    assert_eq!(targets[2], expected_base * 10);
    assert_eq!(targets[6], expected_base * 100_000);
}

#[test]
fn nonzero_compaction_level_targets_match_segmented_clamped_fixture() {
    let targets = nonzero_level_targets_from_level_bytes(&[0, 0, 0, 30 * gib(1), 0, 0, 0]);

    assert_eq!(
        targets,
        vec![
            0,
            mib(256),
            mib(2_560),
            mib(25_600),
            mib(256_000),
            mib(2_560_000),
            mib(25_600_000)
        ]
    );
}

#[test]
fn nonzero_compaction_level_targets_match_segmented_raised_base_fixture() {
    let targets = nonzero_level_targets_from_level_bytes(&[0, 0, 0, 0, mib(5), 0, 0]);
    let expected_base = {
        let mut base = mib(5);
        for _ in 1..4 {
            base /= 10;
        }
        for _ in 1..4 {
            base *= 10;
        }
        base
    };

    assert_eq!(targets[1], mib(256));
    assert_eq!(targets[2], mib(256));
    assert_eq!(targets[3], mib(256));
    assert_eq!(targets[4], expected_base);
    assert_eq!(targets[5], expected_base * 10);
    assert_eq!(targets[6], expected_base * 100);
}

#[test]
fn compaction_requests_use_table_output_target_bytes() {
    let branch = branch_id(0x5a);
    let default_output_target =
        crate::table::TableCompactionConfig::default().target_output_bytes();

    let l0_rewrite =
        LifecycleCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "target-l0")
            .expect("l0 request")
            .branch_request()
            .expect("l0 branch request");
    assert_eq!(
        l0_rewrite.table_compaction_config().target_output_bytes(),
        default_output_target
    );

    let l0_to_l1 = LifecycleCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0ToLevelOne,
        "target-l1",
    )
    .expect("l0 to l1 request")
    .branch_request()
    .expect("l0 to l1 branch request");
    assert_eq!(
        l0_to_l1.table_compaction_config().target_output_bytes(),
        default_output_target
    );

    let l1_to_l2 = LifecycleCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::new(1),
            table_index: 0,
        },
        "target-l2",
    )
    .expect("l1 to l2 request")
    .branch_request()
    .expect("l1 to l2 branch request");
    assert_eq!(
        l1_to_l2.table_compaction_config().target_output_bytes(),
        default_output_target
    );
    assert_ne!(
        default_output_target,
        nonzero_level_target_bytes(BranchLevel::new(1))
    );
}

#[test]
fn generated_nonzero_level_target_sweep_matches_empty_layout_pyramid() {
    let sampled_levels = [1, 2, 3, 4, 8, 16, 32, 64, 128, 255];
    let mut previous = 0;
    for raw_level in sampled_levels {
        let level = BranchLevel::new(raw_level);
        let target = nonzero_level_target_bytes(level);

        assert!(
            target >= previous,
            "target for level {raw_level} regressed below prior sampled level"
        );
        if raw_level == 1 {
            assert_eq!(target, mib(1));
        }
        if previous != 0 && previous <= u64::MAX / 10 {
            assert!(target >= previous.saturating_mul(10));
        }
        assert_eq!(target, nonzero_level_target_bytes(level));
        previous = target;
    }
}

#[test]
fn nonzero_byte_pressure_threshold_boundaries_match_level_targets() {
    for raw_level in [1, 2, 3, 4] {
        let level = BranchLevel::new(raw_level);
        let target = nonzero_level_target_bytes(level);
        let just_below_target = target.saturating_sub(1);
        let urgent_threshold = target.saturating_mul(2);
        let blocking_threshold = target.saturating_mul(4);

        assert!(
            nonzero_compaction_pressure(level, 1, just_below_target).is_none(),
            "level {raw_level} should not report byte pressure below target"
        );

        let (severity, score, target_bytes) =
            nonzero_compaction_pressure(level, 1, target).expect("target pressure");
        assert_eq!(target_bytes, target);
        assert_eq!(severity, LifecycleStoragePressureSeverity::Background);
        assert_eq!(score, 1_000);

        let (severity, score, _) = nonzero_compaction_pressure(level, 1, urgent_threshold - 1)
            .expect("pre-urgent pressure");
        assert_eq!(severity, LifecycleStoragePressureSeverity::Background);
        assert!(score < 2_000);

        let (severity, score, _) =
            nonzero_compaction_pressure(level, 1, urgent_threshold).expect("urgent pressure");
        assert_eq!(severity, LifecycleStoragePressureSeverity::Urgent);
        assert_eq!(score, 2_000);

        let (severity, score, _) = nonzero_compaction_pressure(level, 1, blocking_threshold - 1)
            .expect("pre-blocking pressure");
        assert_eq!(severity, LifecycleStoragePressureSeverity::Urgent);
        assert!(score < 4_000);

        let (severity, score, _) =
            nonzero_compaction_pressure(level, 1, blocking_threshold).expect("blocking pressure");
        assert_eq!(
            severity,
            LifecycleStoragePressureSeverity::BlockMutatingAdmission
        );
        assert_eq!(score, 4_000);
    }

    let level = BranchLevel::new(1);
    let (severity, score, _) =
        nonzero_compaction_pressure(level, 4, 1).expect("count pressure at background threshold");
    assert_eq!(severity, LifecycleStoragePressureSeverity::Background);
    assert_eq!(score, 1_000);

    let (severity, score, _) =
        nonzero_compaction_pressure(level, 8, 1).expect("count pressure at urgent threshold");
    assert_eq!(severity, LifecycleStoragePressureSeverity::Urgent);
    assert_eq!(score, 2_000);

    let (severity, score, _) =
        nonzero_compaction_pressure(level, 16, 1).expect("count pressure at blocking threshold");
    assert_eq!(
        severity,
        LifecycleStoragePressureSeverity::BlockMutatingAdmission
    );
    assert_eq!(score, 4_000);
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
fn compaction_candidate_reports_single_l0_promotion() {
    let branch = branch_id(0x5b);
    let mut state = BranchLocalState::empty(branch);
    let key = physical_key(branch, b"single-promote");
    let row = put_row(branch, b"single-promote", 1, 1_000, b"value");
    install_l0_table(&mut state, branch, "single-promote", vec![row.clone()]);
    let request = LifecycleCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0ToLevelOne,
        "single-promote",
    )
    .expect("request");

    let outcome = compact_cache_branch(&mut state, &request).expect("outcome");
    let candidate = outcome.plan().candidate().expect("candidate");

    assert_eq!(outcome.status(), LifecycleCompactionStatus::Completed);
    assert_eq!(
        candidate.operation(),
        BranchCompactionOperation::MetadataPromotion
    );
    assert_eq!(candidate.input_refs().len(), 1);
    assert!(candidate.overlap_refs().is_empty());
    assert_eq!(candidate.output_level(), BranchLevel::new(1));
    assert_eq!(candidate.source_count(), 1);
    assert_eq!(candidate.input_row_count(), 1);
    assert_eq!(outcome.branch_outcome().removed_refs().len(), 1);
    assert_eq!(outcome.branch_outcome().output_refs().len(), 1);
    assert!(outcome.branch_outcome().table_report().is_none());
    assert_eq!(state.owned_levels()[0].len(), 0);
    assert_eq!(state.owned_levels()[1].len(), 1);
    assert_eq!(
        state
            .capture_read_view()
            .expect("view")
            .latest(&key)
            .expect("latest")
            .expect("visible")
            .row(),
        &row
    );
}

#[test]
fn compaction_fixed_point_drain_empty_branch_is_idempotent() {
    let branch = branch_id(0x5e);
    let mut state = BranchLocalState::empty(branch);
    let before = state.source_layout();
    let request =
        LifecycleCompactionDrainRequest::new(branch, "fixed-point-empty").expect("request");

    assert_eq!(request.branch_id(), branch);
    assert_eq!(request.output_identity_prefix(), "fixed-point-empty");
    assert_eq!(request.max_passes(), 16);

    let outcome = compact_cache_branch_to_fixed_point(&mut state, &request).expect("first drain");

    assert_eq!(outcome.branch_id(), branch);
    assert_eq!(outcome.operations_attempted(), 0);
    assert_eq!(outcome.operations_installed(), 0);
    assert_eq!(outcome.table_rewrites(), 0);
    assert_eq!(outcome.metadata_promotions(), 0);
    assert!(outcome.levels_touched().is_empty());
    assert_eq!(outcome.input_tables_removed(), 0);
    assert_eq!(outcome.output_tables_installed(), 0);
    assert_eq!(outcome.final_source_layout(), &before);
    assert_eq!(state.source_layout(), before);

    let repeated = compact_cache_branch_to_fixed_point(&mut state, &request).expect("second drain");
    assert_eq!(repeated.operations_attempted(), 0);
    assert_eq!(repeated.final_source_layout(), &state.source_layout());
}

#[test]
fn compaction_fixed_point_drain_moves_l0_data_to_last_configured_level() {
    let branch = branch_id(0x5f);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(4, 64, 32).expect("branch config"),
    )
    .expect("state");
    let first_key = physical_key(branch, b"fixed-point-a");
    let first = put_row(branch, b"fixed-point-a", 1, 1_000, b"a");
    let second = put_row(branch, b"fixed-point-b", 2, 2_000, b"b");
    install_l0_table(&mut state, branch, "fixed-point-l0-a", vec![first.clone()]);
    install_l0_table(&mut state, branch, "fixed-point-l0-b", vec![second]);

    let request =
        LifecycleCompactionDrainRequest::new(branch, "fixed-point-cascade").expect("request");
    let outcome =
        compact_cache_branch_to_fixed_point(&mut state, &request).expect("fixed-point drain");

    assert_eq!(outcome.operations_attempted(), 3);
    assert_eq!(outcome.operations_installed(), 3);
    assert_eq!(outcome.table_rewrites(), 1);
    assert_eq!(outcome.metadata_promotions(), 2);
    assert_eq!(
        outcome.levels_touched(),
        &[
            BranchLevel::ZERO,
            BranchLevel::new(1),
            BranchLevel::new(2),
            BranchLevel::new(3)
        ]
    );
    assert_eq!(outcome.input_tables_removed(), 4);
    assert_eq!(outcome.output_tables_installed(), 3);
    assert_eq!(outcome.final_source_layout(), &state.source_layout());
    assert_eq!(outcome.final_source_layout().owned_l0_tables(), 0);
    assert_eq!(
        outcome
            .final_source_layout()
            .owned_nonzero_level_table_counts()
            .iter()
            .map(|count: &BranchLevelTableCount| (count.level(), count.table_count()))
            .collect::<Vec<_>>(),
        vec![(BranchLevel::new(3), 1)]
    );
    assert_eq!(
        state
            .capture_read_view()
            .expect("view")
            .latest(&first_key)
            .expect("latest")
            .expect("visible")
            .row(),
        &first
    );

    let repeated = compact_cache_branch_to_fixed_point(&mut state, &request).expect("second drain");
    assert_eq!(repeated.operations_attempted(), 0);
    assert_eq!(repeated.operations_installed(), 0);
    assert_eq!(repeated.final_source_layout(), &state.source_layout());
}

#[test]
fn compaction_fixed_point_drain_rewrites_overlaps_and_promotes_gaps() {
    let branch = branch_id(0x60);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(4, 64, 32).expect("branch config"),
    )
    .expect("state");
    let overlap_key = physical_key(branch, b"fixed-point-m");
    let newest = put_row(branch, b"fixed-point-m", 7, 7_000, b"newest");
    let l0_other = put_row(branch, b"fixed-point-n", 6, 6_000, b"other");
    let l1_overlap = put_row(branch, b"fixed-point-m", 5, 5_000, b"l1");
    let l1_gap = put_row(branch, b"fixed-point-q", 4, 4_000, b"gap");
    let l2_overlap = put_row(branch, b"fixed-point-m", 3, 3_000, b"l2");
    let l2_preserved = put_row(branch, b"fixed-point-z", 2, 2_000, b"preserved");
    install_owned_table(
        &mut state,
        branch,
        BranchLevel::new(2),
        "fixed-point-l2-overlap",
        vec![l2_overlap.clone()],
    );
    install_owned_table(
        &mut state,
        branch,
        BranchLevel::new(2),
        "fixed-point-l2-preserved",
        vec![l2_preserved],
    );
    install_owned_table(
        &mut state,
        branch,
        BranchLevel::new(1),
        "fixed-point-l1-overlap",
        vec![l1_overlap.clone()],
    );
    install_owned_table(
        &mut state,
        branch,
        BranchLevel::new(1),
        "fixed-point-l1-gap",
        vec![l1_gap.clone()],
    );
    install_l0_table(
        &mut state,
        branch,
        "fixed-point-l0-newest",
        vec![newest.clone()],
    );
    install_l0_table(&mut state, branch, "fixed-point-l0-other", vec![l0_other]);

    let request =
        LifecycleCompactionDrainRequest::new(branch, "fixed-point-mixed").expect("request");
    let outcome =
        compact_cache_branch_to_fixed_point(&mut state, &request).expect("fixed-point drain");

    assert_eq!(outcome.table_rewrites(), 2);
    assert_eq!(outcome.metadata_promotions(), 4);
    assert_eq!(
        outcome.operations_attempted(),
        outcome.operations_installed()
    );
    assert_eq!(outcome.final_source_layout().owned_l0_tables(), 0);
    assert_eq!(
        outcome
            .final_source_layout()
            .owned_nonzero_level_table_counts()
            .iter()
            .map(|count: &BranchLevelTableCount| (count.level(), count.table_count()))
            .collect::<Vec<_>>(),
        vec![(BranchLevel::new(3), 3)]
    );
    assert_eq!(state.owned_levels()[0].len(), 0);
    assert_eq!(state.owned_levels()[1].len(), 0);
    assert_eq!(state.owned_levels()[2].len(), 0);
    assert_eq!(state.owned_levels()[3].len(), 3);

    let view = state.capture_read_view().expect("view");
    assert_eq!(
        view.latest(&overlap_key)
            .expect("latest")
            .expect("visible")
            .row(),
        &newest
    );
    assert_eq!(
        view.at_version(&overlap_key, CommitVersion::new(3))
            .expect("bounded")
            .expect("visible")
            .row(),
        &l2_overlap
    );
}

#[test]
fn generated_shape_fixed_point_compaction_preserves_reads_and_nonoverlap() {
    for source_count in 1..=8 {
        let source_count_byte = u8::try_from(source_count).expect("source count fits in u8");
        let branch = branch_id(0x70 + source_count_byte);
        let mut state = BranchLocalState::new(
            branch,
            BranchRuntimeConfig::new(4, 128, 32).expect("branch config"),
        )
        .expect("state");
        let table_shapes = generated_compaction_table_shapes(branch);
        for (source_index, (level, rows)) in table_shapes.into_iter().take(source_count).enumerate()
        {
            let identity = format!("generated-shape-{source_count}-{source_index}");
            if level == BranchLevel::ZERO {
                install_l0_table(&mut state, branch, &identity, rows);
            } else {
                install_owned_table(&mut state, branch, level, &identity, rows);
            }
        }

        let probe_keys = generated_compaction_probe_keys(branch);
        let scan_prefix = physical_key(branch, b"generated-");
        let range_lower = physical_key(branch, b"generated-a");
        let range_upper = physical_key(branch, b"generated-z");
        let before = state.capture_read_view().expect("before");
        let before_latest = probe_keys
            .iter()
            .map(|key| {
                before
                    .latest(key)
                    .expect("before latest")
                    .map(|row| row.row().clone())
            })
            .collect::<Vec<_>>();
        let before_history = probe_keys
            .iter()
            .map(|key| {
                history_versions(
                    &before
                        .history(key, BranchHistoryOptions::all())
                        .expect("before history"),
                )
            })
            .collect::<Vec<_>>();
        let before_prefix = scan_user_keys(
            &before
                .scan_prefix(
                    &BranchScanBounds::prefix(&scan_prefix),
                    BranchReadBound::latest(),
                )
                .expect("before prefix"),
        );
        let before_range = scan_user_keys(
            &before
                .scan_range(
                    &BranchScanBounds::closed(&range_lower, &range_upper).expect("range"),
                    BranchReadBound::latest(),
                )
                .expect("before range"),
        );

        let request =
            LifecycleCompactionDrainRequest::new(branch, format!("generated-shape-{source_count}"))
                .expect("request");
        let outcome =
            compact_cache_branch_to_fixed_point(&mut state, &request).unwrap_or_else(|error| {
                panic!("fixed-point drain failed for source count {source_count}: {error:?}")
            });
        assert_eq!(
            outcome.operations_attempted(),
            outcome.operations_installed()
        );
        assert_eq!(outcome.final_source_layout(), &state.source_layout());

        let after = state.capture_read_view().expect("after");
        let after_latest = probe_keys
            .iter()
            .map(|key| {
                after
                    .latest(key)
                    .expect("after latest")
                    .map(|row| row.row().clone())
            })
            .collect::<Vec<_>>();
        let after_history = probe_keys
            .iter()
            .map(|key| {
                history_versions(
                    &after
                        .history(key, BranchHistoryOptions::all())
                        .expect("after history"),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(after_latest, before_latest);
        assert_eq!(after_history, before_history);
        assert_eq!(
            scan_user_keys(
                &after
                    .scan_prefix(
                        &BranchScanBounds::prefix(&scan_prefix),
                        BranchReadBound::latest(),
                    )
                    .expect("after prefix")
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
                    .expect("after range")
            ),
            before_range
        );
        assert_owned_nonzero_tables_are_sorted_and_nonoverlapping(&state);

        let repeated =
            compact_cache_branch_to_fixed_point(&mut state, &request).expect("repeat drain");
        assert_eq!(repeated.operations_attempted(), 0);
        assert_eq!(repeated.operations_installed(), 0);
        assert_eq!(repeated.final_source_layout(), &state.source_layout());
    }
}

fn generated_compaction_table_shapes(
    branch: strata_core_next::BranchId,
) -> Vec<(BranchLevel, Vec<crate::row::StorageRow>)> {
    vec![
        (
            BranchLevel::ZERO,
            vec![
                put_row(branch, b"generated-shared", 10, 10_000, &[0x10; 96]),
                put_row(branch, b"generated-shared", 9, 9_000, &[0x09; 96]),
            ],
        ),
        (
            BranchLevel::ZERO,
            vec![tombstone_row(branch, b"generated-shared", 8, 8_000)],
        ),
        (
            BranchLevel::ZERO,
            vec![put_row(branch, b"generated-a", 7, 7_000, &[0x07; 80])],
        ),
        (
            BranchLevel::new(1),
            vec![put_row(branch, b"generated-shared", 5, 5_000, &[0x05; 80])],
        ),
        (
            BranchLevel::new(1),
            vec![put_expiring_row(
                branch,
                b"generated-x-expiring",
                6,
                6_000,
                6_500,
                &[0x06; 80],
            )],
        ),
        (
            BranchLevel::new(2),
            vec![put_row(branch, b"generated-shared", 3, 3_000, &[0x03; 80])],
        ),
        (
            BranchLevel::new(2),
            vec![put_row(branch, b"generated-z", 2, 2_000, &[0x02; 80])],
        ),
        (
            BranchLevel::ZERO,
            vec![put_row(branch, b"generated-m", 4, 4_000, &[0x04; 80])],
        ),
    ]
}

fn generated_compaction_probe_keys(
    branch: strata_core_next::BranchId,
) -> Vec<crate::row::PhysicalKey> {
    vec![
        physical_key(branch, b"generated-shared"),
        physical_key(branch, b"generated-a"),
        physical_key(branch, b"generated-x-expiring"),
        physical_key(branch, b"generated-m"),
        physical_key(branch, b"generated-z"),
    ]
}

fn assert_owned_nonzero_tables_are_sorted_and_nonoverlapping(state: &BranchLocalState) {
    for level_tables in state.owned_levels().iter().skip(1) {
        let mut previous_last = None::<Vec<u8>>;
        for table in level_tables {
            let rows = table.rows();
            assert!(rows
                .windows(2)
                .all(|window| window[0].key() < window[1].key()));
            let first = table.facts().key_range().first_key();
            let last = table.facts().key_range().last_key();
            assert!(first <= last);
            if let Some(previous) = previous_last {
                assert!(
                    previous.as_slice() < first,
                    "nonzero compaction output tables must not overlap"
                );
            }
            previous_last = Some(last.to_vec());
        }
    }
}

#[test]
fn compaction_fixed_point_drain_enforces_pass_limit() {
    let branch = branch_id(0x61);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    install_l0_table(
        &mut state,
        branch,
        "fixed-point-limit",
        vec![put_row(branch, b"fixed-point-limit", 1, 1_000, b"value")],
    );
    let request = LifecycleCompactionDrainRequest::new(branch, "fixed-point-limit")
        .expect("request")
        .with_max_passes(0);

    let error =
        compact_cache_branch_to_fixed_point(&mut state, &request).expect_err("pass limit enforced");

    assert!(matches!(
        error,
        LifecycleError::MaintenanceTaskFailed {
            reason: "compaction drain exceeded pass limit"
        }
    ));
    assert_eq!(state.owned_levels()[0].len(), 1);
    assert_eq!(state.owned_levels()[1].len(), 0);
}

#[test]
fn compaction_fixed_point_drain_treats_only_configured_level_as_terminal() {
    let branch = branch_id(0x62);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(1, 64, 32).expect("branch config"),
    )
    .expect("state");
    install_l0_table(
        &mut state,
        branch,
        "fixed-point-terminal",
        vec![put_row(branch, b"fixed-point-terminal", 1, 1_000, b"value")],
    );
    let before = state.source_layout();
    let request =
        LifecycleCompactionDrainRequest::new(branch, "fixed-point-terminal").expect("request");

    let outcome =
        compact_cache_branch_to_fixed_point(&mut state, &request).expect("fixed-point drain");

    assert_eq!(outcome.operations_attempted(), 0);
    assert_eq!(outcome.operations_installed(), 0);
    assert_eq!(outcome.final_source_layout(), &before);
    assert_eq!(state.source_layout(), before);
}

#[test]
fn compaction_fixed_point_drain_rejects_branch_mismatch_without_mutation() {
    let branch = branch_id(0x63);
    let other = branch_id(0x64);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    install_l0_table(
        &mut state,
        branch,
        "fixed-point-mismatch",
        vec![put_row(branch, b"fixed-point-mismatch", 1, 1_000, b"value")],
    );
    let before = state.clone();
    let request =
        LifecycleCompactionDrainRequest::new(other, "fixed-point-mismatch").expect("request");

    let error =
        compact_cache_branch_to_fixed_point(&mut state, &request).expect_err("mismatch rejected");

    assert_eq!(error.code(), "failed_precondition.lifecycle.branch_runtime");
    assert_eq!(state, before);
}

#[test]
fn compaction_fixed_point_drain_rejects_invalid_identity_prefix() {
    let branch = branch_id(0x65);

    assert!(matches!(
        LifecycleCompactionDrainRequest::new(branch, ""),
        Err(LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::TableRuntime,
            ..
        })
    ));
    assert!(matches!(
        LifecycleCompactionDrainRequest::new(branch, "bad/prefix"),
        Err(LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::TableRuntime,
            ..
        })
    ));
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
                level: BranchLevel::new(7),
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
    assert!(outcome.materialization_handle().is_none());
    assert!(outcome.reachability_snapshot().is_none());
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
    assert!(outcome.materialization_handle().is_some());
    assert!(outcome.reachability_snapshot().is_some());
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
    let handle = completed
        .materialization_handle()
        .expect("materialization handle");
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
    assert_eq!(frozen_pressure.active_bytes(), 0);
    assert!(frozen_pressure.frozen_bytes() > 0);
    assert!(matches!(
        frozen_pressure
            .suggested_task()
            .map(MaintenanceTaskRequest::kind),
        Some(MaintenanceTaskKind::Flush)
    ));

    let mut compact_state = BranchLocalState::empty(branch);
    for index in 0_u64..3 {
        install_l0_table(
            &mut compact_state,
            branch,
            &format!("pressure-below-threshold-{index}"),
            vec![put_row(
                branch,
                format!("below-threshold-{index}").as_bytes(),
                index + 2,
                (index + 2) * 1_000,
                b"value",
            )],
        );
    }
    let below_threshold_pressure =
        collect_storage_pressure(&compact_state, empty_maintenance_status());
    assert_eq!(
        below_threshold_pressure.reason(),
        LifecycleStoragePressureReason::None
    );
    assert_eq!(
        below_threshold_pressure.severity(),
        LifecycleStoragePressureSeverity::None
    );
    assert_eq!(below_threshold_pressure.level_zero_tables(), 3);
    assert!(below_threshold_pressure.suggested_task().is_none());

    install_l0_table(
        &mut compact_state,
        branch,
        "pressure-background-threshold",
        vec![put_row(branch, b"background-threshold", 5, 5_000, b"value")],
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
    assert_eq!(compact_pressure.level_zero_tables(), 4);
    assert_eq!(compact_pressure.owned_tables(), 4);
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
fn optional_maintenance_is_deferred_under_global_memory_pressure() {
    let branch = branch_id(0x4b);

    // Four L0 tables suggest an optional (Background) compaction.
    let mut compact_state = BranchLocalState::empty(branch);
    for index in 0_u64..4 {
        install_l0_table(
            &mut compact_state,
            branch,
            &format!("defer-optional-{index}"),
            vec![put_row(
                branch,
                format!("defer-optional-{index}").as_bytes(),
                index + 2,
                (index + 2) * 1_000,
                b"value",
            )],
        );
    }
    let optional = collect_storage_pressure(&compact_state, empty_maintenance_status());
    assert_eq!(
        optional.severity(),
        LifecycleStoragePressureSeverity::Background
    );
    assert!(optional.suggested_task().is_some());

    // No database-wide memory pressure: the optional task is kept.
    assert!(optional
        .deferred_under_global_memory_pressure(StorageBudgetPressureSeverity::Normal)
        .suggested_task()
        .is_some());

    // Under memory pressure the optional task is deferred: no task, neutral severity, while
    // the descriptive shape counts are preserved for diagnostics.
    let deferred = optional.deferred_under_global_memory_pressure(
        StorageBudgetPressureSeverity::DeferOptionalMaintenance,
    );
    assert!(deferred.suggested_task().is_none());
    assert_eq!(deferred.severity(), LifecycleStoragePressureSeverity::None);
    assert_eq!(deferred.level_zero_tables(), 4);

    // Required (Urgent) maintenance gates write admission, so even the highest memory
    // pressure never defers it.
    let mut frozen_state = BranchLocalState::empty(branch);
    frozen_state
        .append_committed_row(put_row(branch, b"frozen", 1, 1_000, b"value"))
        .expect("append");
    frozen_state.rotate_active();
    let required = collect_storage_pressure(&frozen_state, empty_maintenance_status());
    assert_eq!(
        required.severity(),
        LifecycleStoragePressureSeverity::Urgent
    );
    assert!(required
        .deferred_under_global_memory_pressure(
            StorageBudgetPressureSeverity::RejectMutatingAdmission
        )
        .suggested_task()
        .is_some());
}

#[test]
fn compaction_is_surfaced_independently_of_the_flush_first_task() {
    let branch = branch_id(0x5a);
    // A backed-up L0 (four tables) alongside a frozen memtable. The single
    // `suggested_task` is the flush (frozen wins the priority cascade), but the
    // decoupled `compaction_task` still surfaces the eligible compaction so the
    // backlog is scheduled rather than starved behind flush.
    let mut state = BranchLocalState::empty(branch);
    for index in 0_u64..4 {
        install_l0_table(
            &mut state,
            branch,
            &format!("decoupled-{index}"),
            vec![put_row(
                branch,
                format!("decoupled-{index}").as_bytes(),
                index + 2,
                (index + 2) * 1_000,
                b"value",
            )],
        );
    }
    state
        .append_committed_row(put_row(branch, b"frozen", 9, 9_000, b"value"))
        .expect("append");
    state.rotate_active();

    let pressure = collect_storage_pressure(&state, empty_maintenance_status());
    assert_eq!(pressure.frozen_tables(), 1);
    assert_eq!(pressure.level_zero_tables(), 4);
    // Flush still wins the single suggestion...
    assert_eq!(
        pressure.suggested_task().map(MaintenanceTaskRequest::kind),
        Some(MaintenanceTaskKind::Flush)
    );
    // ...but the compaction is surfaced independently.
    assert_eq!(
        crate::lifecycle::compaction::eligible_compaction_task(
            &state,
            None,
            StorageBudgetPressureSeverity::Normal,
        )
        .map(MaintenanceTaskRequest::kind),
        Some(MaintenanceTaskKind::Compaction)
    );
}

#[test]
fn compaction_task_is_none_below_the_rewrite_trigger() {
    let branch = branch_id(0x5b);
    // Three L0 tables are below the compaction trigger, with a frozen memtable
    // present: no compaction task is surfaced, so the post-commit enqueue cannot
    // flood the queue with sub-threshold work.
    let mut state = BranchLocalState::empty(branch);
    for index in 0_u64..3 {
        install_l0_table(
            &mut state,
            branch,
            &format!("below-trigger-{index}"),
            vec![put_row(
                branch,
                format!("below-trigger-{index}").as_bytes(),
                index + 2,
                (index + 2) * 1_000,
                b"value",
            )],
        );
    }
    state
        .append_committed_row(put_row(branch, b"frozen", 9, 9_000, b"value"))
        .expect("append");
    state.rotate_active();

    let pressure = collect_storage_pressure(&state, empty_maintenance_status());
    assert_eq!(pressure.level_zero_tables(), 3);
    assert_eq!(
        pressure.suggested_task().map(MaintenanceTaskRequest::kind),
        Some(MaintenanceTaskKind::Flush)
    );
    assert!(crate::lifecycle::compaction::eligible_compaction_task(
        &state,
        None,
        StorageBudgetPressureSeverity::Normal,
    )
    .is_none());
}

#[test]
fn optional_compaction_is_deferred_under_memory_pressure_even_when_frozen() {
    let branch = branch_id(0x5c);
    // An optional (Background) L0 compaction plus a frozen memtable: the overall
    // severity is Urgent (frozen), so the whole-pressure neutralization does not fire,
    // but the decoupled optional compaction must still be held back under memory
    // pressure while the required flush is kept.
    let mut state = BranchLocalState::empty(branch);
    for index in 0_u64..4 {
        install_l0_table(
            &mut state,
            branch,
            &format!("defer-frozen-{index}"),
            vec![put_row(
                branch,
                format!("defer-frozen-{index}").as_bytes(),
                index + 2,
                (index + 2) * 1_000,
                b"value",
            )],
        );
    }
    state
        .append_committed_row(put_row(branch, b"frozen", 9, 9_000, b"value"))
        .expect("append");
    state.rotate_active();

    let pressure = collect_storage_pressure(&state, empty_maintenance_status());
    assert_eq!(
        pressure.severity(),
        LifecycleStoragePressureSeverity::Urgent
    );
    assert_eq!(
        pressure.suggested_task().map(MaintenanceTaskRequest::kind),
        Some(MaintenanceTaskKind::Flush)
    );

    // No memory pressure: the compaction is surfaced.
    assert!(crate::lifecycle::compaction::eligible_compaction_task(
        &state,
        None,
        StorageBudgetPressureSeverity::Normal,
    )
    .is_some());

    // Under memory pressure: the optional compaction is deferred (the required flush,
    // surfaced via `suggested_task`, is unaffected).
    assert!(crate::lifecycle::compaction::eligible_compaction_task(
        &state,
        None,
        StorageBudgetPressureSeverity::DeferOptionalMaintenance,
    )
    .is_none());
}

#[test]
fn storage_pressure_reports_active_mutable_byte_pressure_before_rotation() {
    let branch = branch_id(0x68);
    let rotation_bytes = 1024 * 1024;

    let mut background = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::default()
            .with_active_rotation_bytes(rotation_bytes)
            .expect("custom active rotation threshold"),
    )
    .expect("branch state");
    let background_value = vec![0x41; 600 * 1024];
    background
        .append_committed_row(put_row(
            branch,
            b"active-byte-background",
            1,
            1_000,
            &background_value,
        ))
        .expect("append background pressure row");
    let background_pressure = collect_storage_pressure(&background, empty_maintenance_status());
    assert_eq!(
        background_pressure.reason(),
        LifecycleStoragePressureReason::ActiveMutableBytes
    );
    assert_eq!(
        background_pressure.severity(),
        LifecycleStoragePressureSeverity::Background
    );
    assert_eq!(background_pressure.frozen_tables(), 0);
    assert_eq!(background_pressure.frozen_bytes(), 0);
    assert!(background_pressure.active_bytes() >= 600 * 1024);
    assert!(background_pressure.suggested_task().is_none());

    let mut urgent = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::default()
            .with_active_rotation_bytes(rotation_bytes)
            .expect("custom active rotation threshold"),
    )
    .expect("branch state");
    let urgent_value = vec![0x42; 850 * 1024];
    urgent
        .append_committed_row(put_row(
            branch,
            b"active-byte-urgent",
            2,
            2_000,
            &urgent_value,
        ))
        .expect("append urgent pressure row");
    let urgent_pressure = collect_storage_pressure(&urgent, empty_maintenance_status());
    assert_eq!(
        urgent_pressure.reason(),
        LifecycleStoragePressureReason::ActiveMutableBytes
    );
    assert_eq!(
        urgent_pressure.severity(),
        LifecycleStoragePressureSeverity::Urgent
    );
    assert_eq!(urgent_pressure.frozen_tables(), 0);
    assert_eq!(urgent_pressure.frozen_bytes(), 0);
    assert!(urgent_pressure.active_bytes() >= 850 * 1024);
    assert!(urgent_pressure.suggested_task().is_none());
}

#[test]
fn storage_pressure_generated_active_byte_threshold_sweep_matches_policy() {
    let rotation_bytes = 1024 * 1024;
    for (case_index, value_bytes, expected_severity) in [
        (0_u8, 128 * 1024, LifecycleStoragePressureSeverity::None),
        (1, 600 * 1024, LifecycleStoragePressureSeverity::Background),
        (2, 850 * 1024, LifecycleStoragePressureSeverity::Urgent),
    ] {
        let branch = branch_id(0x90 + case_index);
        let state = active_byte_pressure_state(branch, rotation_bytes, value_bytes, case_index);
        let pressure = collect_storage_pressure(&state, empty_maintenance_status());

        assert_eq!(pressure.severity(), expected_severity);
        if expected_severity == LifecycleStoragePressureSeverity::None {
            assert_eq!(pressure.reason(), LifecycleStoragePressureReason::None);
        } else {
            assert_eq!(
                pressure.reason(),
                LifecycleStoragePressureReason::ActiveMutableBytes
            );
        }
        assert_eq!(pressure.frozen_tables(), 0);
        assert_eq!(pressure.frozen_bytes(), 0);
        assert!(pressure.active_bytes() < u64::try_from(rotation_bytes).expect("threshold fits"));
    }

    let blocked_branch = branch_id(0x93);
    let blocked = blocked_active_byte_pressure_state(blocked_branch, 512 * 1024);
    let blocked_pressure = collect_storage_pressure(&blocked, empty_maintenance_status());
    assert_eq!(
        blocked_pressure.severity(),
        LifecycleStoragePressureSeverity::BlockMutatingAdmission
    );
    assert_eq!(
        blocked_pressure.reason(),
        LifecycleStoragePressureReason::ActiveMutableBytes
    );
    assert!(blocked_pressure.frozen_tables() > 0);
    assert!(blocked_pressure.frozen_bytes() > 0);
}

#[test]
fn storage_pressure_blocks_active_bytes_when_rotation_is_backed_up() {
    let branch = branch_id(0x69);
    let rotation_bytes = 512 * 1024;
    let config = BranchRuntimeConfig::new(8, 64, 1)
        .expect("branch config")
        .with_active_rotation_bytes(rotation_bytes)
        .expect("custom active rotation threshold");
    let mut state = BranchLocalState::new(branch, config).expect("branch state");
    let first_value = vec![0x43; 600 * 1024];
    state
        .append_committed_row(put_row(
            branch,
            b"active-byte-rotates-to-frozen",
            1,
            1_000,
            &first_value,
        ))
        .expect("append first large row");
    assert_eq!(state.frozen_table_count(), 1);
    assert_eq!(state.active_row_count(), 0);

    let second_value = vec![0x44; 600 * 1024];
    state
        .append_committed_row(put_row(
            branch,
            b"active-byte-blocked-by-frozen-limit",
            2,
            2_000,
            &second_value,
        ))
        .expect("append second large row");
    assert_eq!(state.frozen_table_count(), 1);
    assert_eq!(state.active_row_count(), 1);

    let pressure = collect_storage_pressure(&state, empty_maintenance_status());

    assert_eq!(
        pressure.reason(),
        LifecycleStoragePressureReason::ActiveMutableBytes
    );
    assert_eq!(
        pressure.severity(),
        LifecycleStoragePressureSeverity::BlockMutatingAdmission
    );
    assert!(pressure.active_bytes() >= u64::try_from(rotation_bytes).expect("threshold fits"));
    assert!(pressure.frozen_bytes() > 0);
    assert!(matches!(
        pressure.suggested_task().map(MaintenanceTaskRequest::kind),
        Some(MaintenanceTaskKind::Flush)
    ));
}

fn active_byte_pressure_state(
    branch: BranchId,
    rotation_bytes: usize,
    value_bytes: usize,
    case_index: u8,
) -> BranchLocalState {
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::default()
            .with_active_rotation_bytes(rotation_bytes)
            .expect("custom active rotation threshold"),
    )
    .expect("branch state");
    state
        .append_committed_row(put_row(
            branch,
            format!("active-byte-sweep-{case_index}").as_bytes(),
            u64::from(case_index) + 1,
            (u64::from(case_index) + 1) * 1_000,
            &vec![case_index; value_bytes],
        ))
        .expect("append active pressure row");
    state
}

fn blocked_active_byte_pressure_state(branch: BranchId, rotation_bytes: usize) -> BranchLocalState {
    let config = BranchRuntimeConfig::new(8, 64, 1)
        .expect("branch config")
        .with_active_rotation_bytes(rotation_bytes)
        .expect("custom active rotation threshold");
    let mut state = BranchLocalState::new(branch, config).expect("branch state");
    let value_bytes = rotation_bytes.saturating_add(64 * 1024);
    state
        .append_committed_row(put_row(
            branch,
            b"active-byte-sweep-frozen",
            1,
            1_000,
            &vec![0x71; value_bytes],
        ))
        .expect("append first large row");
    state
        .append_committed_row(put_row(
            branch,
            b"active-byte-sweep-blocked",
            2,
            2_000,
            &vec![0x72; value_bytes],
        ))
        .expect("append second large row");
    state
}

#[test]
fn storage_pressure_prefers_clearable_table_rewrite_when_active_block_has_no_flush_candidate() {
    let branch = branch_id(0x6a);
    let rotation_bytes = 512 * 1024;
    let config = BranchRuntimeConfig::new(8, 64, 1)
        .expect("branch config")
        .with_active_rotation_bytes(rotation_bytes)
        .expect("custom active rotation threshold");
    let mut state = BranchLocalState::new(branch, config).expect("branch state");
    let first_value = vec![0x45; 600 * 1024];
    state
        .append_committed_row(put_row(
            branch,
            b"active-byte-fill-frozen-capacity",
            1,
            1_000,
            &first_value,
        ))
        .expect("append first large row");
    let second_value = vec![0x46; 600 * 1024];
    state
        .append_committed_row(put_row(
            branch,
            b"active-byte-over-threshold-with-full-frozen",
            2,
            2_000,
            &second_value,
        ))
        .expect("append second large row");
    assert_eq!(state.frozen_table_count(), 1);
    assert!(state.active_byte_count() >= u64::try_from(rotation_bytes).expect("threshold fits"));

    let flush = FlushFrozenRequest::new(
        branch,
        Some(0),
        FlushTableIdentitySeed::new("active-block-flush-seed").expect("flush seed"),
        FlushTableObjectId::new("active-block-flush-object").expect("flush object"),
    )
    .expect("flush request");
    flush_cache_branch(&mut state, &flush).expect("flush frozen table");
    assert_eq!(state.frozen_table_count(), 0);
    assert!(state.active_byte_count() >= u64::try_from(rotation_bytes).expect("threshold fits"));

    let active_only_pressure = collect_storage_pressure(&state, empty_maintenance_status());
    assert_eq!(
        active_only_pressure.reason(),
        LifecycleStoragePressureReason::ActiveMutableBytes
    );
    assert_eq!(
        active_only_pressure.severity(),
        LifecycleStoragePressureSeverity::Urgent
    );
    assert!(active_only_pressure.suggested_task().is_none());

    for index in 0..16 {
        install_l0_table(
            &mut state,
            branch,
            &format!("clearable-table-rewrite-l0-{index}"),
            vec![put_row(
                branch,
                format!("clearable-table-rewrite-{index}").as_bytes(),
                index + 10,
                (index + 10) * 1_000,
                b"value",
            )],
        );
    }

    let pressure = collect_storage_pressure(&state, empty_maintenance_status());

    assert_eq!(
        pressure.reason(),
        LifecycleStoragePressureReason::LevelZeroTableBacklog
    );
    assert_eq!(
        pressure.severity(),
        LifecycleStoragePressureSeverity::BlockMutatingAdmission
    );
    assert!(matches!(
        pressure.suggested_task().map(MaintenanceTaskRequest::kind),
        Some(MaintenanceTaskKind::Compaction)
    ));
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
    for index in 0_u64..8 {
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
    assert_eq!(first.level_zero_tables(), 8);
    assert!(matches!(
        first.suggested_task().map(MaintenanceTaskRequest::kind),
        Some(MaintenanceTaskKind::Compaction)
    ));
}

#[test]
fn storage_pressure_reports_l0_table_backlog_boundaries() {
    fn pressure_for_l0_table_count(count: u64) -> LifecycleStoragePressure {
        let branch = branch_id(0x67);
        let mut state = BranchLocalState::empty(branch);
        for index in 0..count {
            install_l0_table(
                &mut state,
                branch,
                &format!("boundary-{count}-{index}"),
                vec![put_row(
                    branch,
                    format!("boundary-{count}-{index}").as_bytes(),
                    index + 1,
                    (index + 1) * 1_000,
                    b"value",
                )],
            );
        }
        collect_storage_pressure(&state, empty_maintenance_status())
    }

    for (table_count, expected_severity) in [
        (3, LifecycleStoragePressureSeverity::None),
        (4, LifecycleStoragePressureSeverity::Background),
        (7, LifecycleStoragePressureSeverity::Background),
        (8, LifecycleStoragePressureSeverity::Urgent),
        (15, LifecycleStoragePressureSeverity::Urgent),
        (16, LifecycleStoragePressureSeverity::BlockMutatingAdmission),
    ] {
        let pressure = pressure_for_l0_table_count(table_count);

        let table_count = usize::try_from(table_count).expect("fixture table count fits in usize");
        assert_eq!(pressure.level_zero_tables(), table_count);
        assert_eq!(pressure.severity(), expected_severity);
        if expected_severity == LifecycleStoragePressureSeverity::None {
            assert_eq!(pressure.reason(), LifecycleStoragePressureReason::None);
            assert!(pressure.suggested_task().is_none());
        } else {
            assert_eq!(
                pressure.reason(),
                LifecycleStoragePressureReason::LevelZeroTableBacklog
            );
            assert!(matches!(
                pressure.suggested_task().map(MaintenanceTaskRequest::kind),
                Some(MaintenanceTaskKind::Compaction)
            ));
        }
    }
}

#[test]
fn storage_pressure_throttle_ratio_tracks_l0_backlog() {
    fn ratio_for_l0(count: u64) -> u16 {
        let branch = branch_id(0x68);
        let mut state = BranchLocalState::empty(branch);
        for index in 0..count {
            install_l0_table(
                &mut state,
                branch,
                &format!("throttle-{count}-{index}"),
                vec![put_row(
                    branch,
                    format!("throttle-{count}-{index}").as_bytes(),
                    index + 1,
                    (index + 1) * 1_000,
                    b"value",
                )],
            );
        }
        collect_storage_pressure(&state, empty_maintenance_status()).throttle_ratio_permille()
    }

    // With no active/frozen bytes, the throttle ratio is the L0 fullness against the blocking
    // threshold (16): count/16 in permille, clamped to 1000 when over-full.
    assert_eq!(ratio_for_l0(0), 0);
    assert_eq!(ratio_for_l0(4), 250);
    assert_eq!(ratio_for_l0(8), 500);
    assert_eq!(ratio_for_l0(16), 1000);
    assert_eq!(ratio_for_l0(20), 1000);
}

#[test]
fn storage_pressure_throttle_ratio_tracks_frozen_table_backlog() {
    fn ratio_for_frozen(count: u64) -> u16 {
        let branch = branch_id(0x69);
        let mut state = BranchLocalState::empty(branch);
        for index in 0..count {
            state
                .append_committed_row(put_row(
                    branch,
                    format!("frozen-{index}").as_bytes(),
                    index + 1,
                    (index + 1) * 1_000,
                    b"value",
                ))
                .expect("append active row");
            state.rotate_active();
        }
        collect_storage_pressure(&state, empty_maintenance_status()).throttle_ratio_permille()
    }

    // Without a budget the frozen-BYTE dimension is skipped, so the ratio reflects frozen-table
    // COUNT fullness against FROZEN_BLOCKING_FLUSH_THRESHOLD (4) — the dimension fix #2 added so
    // the throttle ramps before the frozen-count cliff (the in-budget collapse driver).
    assert_eq!(ratio_for_frozen(0), 0);
    assert_eq!(ratio_for_frozen(2), 500);
    assert_eq!(ratio_for_frozen(4), 1000);
}

#[test]
fn storage_pressure_suggests_flush_before_blocking_l0_compaction() {
    let branch = branch_id(0x6c);
    let mut state = BranchLocalState::empty(branch);
    for index in 0..16 {
        install_l0_table(
            &mut state,
            branch,
            &format!("blocking-with-frozen-{index}"),
            vec![put_row(
                branch,
                format!("blocking-with-frozen-{index}").as_bytes(),
                index + 1,
                (index + 1) * 1_000,
                b"value",
            )],
        );
    }
    state
        .append_committed_row(put_row(branch, b"flush-first", 20, 20_000, b"value"))
        .expect("append active row");
    state.rotate_active();

    let pressure = collect_storage_pressure(&state, empty_maintenance_status());

    assert_eq!(
        pressure.reason(),
        LifecycleStoragePressureReason::FrozenBacklog
    );
    assert_eq!(pressure.level_zero_tables(), 16);
    assert_eq!(pressure.frozen_tables(), 1);
    assert!(matches!(
        pressure.suggested_task().map(MaintenanceTaskRequest::kind),
        Some(MaintenanceTaskKind::Flush)
    ));
}

#[test]
fn storage_pressure_does_not_schedule_bottommost_level_compaction() {
    let branch = branch_id(0x6b);
    let mut state = BranchLocalState::empty(branch);
    let terminal_level =
        BranchLevel::new(u8::try_from(state.owned_levels().len() - 1).expect("level fits in u8"));
    for index in 0..4 {
        install_owned_table(
            &mut state,
            branch,
            terminal_level,
            &format!("terminal-pressure-{index}"),
            vec![put_row(
                branch,
                format!("terminal-pressure-{index}").as_bytes(),
                index + 1,
                (index + 1) * 1_000,
                b"value",
            )],
        );
    }

    let pressure = collect_storage_pressure(&state, empty_maintenance_status());

    assert_eq!(pressure.severity(), LifecycleStoragePressureSeverity::None);
    assert_eq!(pressure.reason(), LifecycleStoragePressureReason::None);
    assert!(pressure.suggested_task().is_none());

    let task = MaintenanceTask::new_for_test(
        1,
        MaintenanceTaskRequest::compaction(branch, terminal_level.raw()),
    )
    .expect("maintenance task");
    let request = current_compaction_request_from_maintenance_task(&task, &state)
        .expect("request")
        .expect("current compaction request");
    assert_eq!(
        request.kind(),
        BranchCompactionKind::CompactBottommostLevel {
            level: terminal_level,
            start_table_index: 0,
            table_count: 4,
        }
    );
}

#[test]
fn storage_pressure_skips_unclearable_bottommost_table_count_under_output_budget() {
    let branch = branch_id(0x6d);
    let mut state = BranchLocalState::empty(branch);
    let terminal_level =
        BranchLevel::new(u8::try_from(state.owned_levels().len() - 1).expect("level fits in u8"));
    for index in 0..4 {
        let value = vec![0x55; 16 * 1024];
        install_owned_table(
            &mut state,
            branch,
            terminal_level,
            &format!("terminal-unclearable-{index}"),
            vec![put_row(
                branch,
                format!("terminal-unclearable-{index}").as_bytes(),
                index + 1,
                (index + 1) * 1_000,
                &value,
            )],
        );
    }

    let budget = StorageRuntimeBudget::low_memory_test_profile();
    let pressure =
        collect_storage_pressure_with_budget(&state, empty_maintenance_status(), Some(budget));

    assert_eq!(pressure.severity(), LifecycleStoragePressureSeverity::None);
    assert_eq!(pressure.reason(), LifecycleStoragePressureReason::None);
    assert!(pressure.suggested_task().is_none());

    let task = MaintenanceTask::new_for_test(
        1,
        MaintenanceTaskRequest::compaction(branch, terminal_level.raw()),
    )
    .expect("maintenance task");
    assert!(
        current_compaction_request_from_maintenance_task_with_budget(&task, &state, Some(budget))
            .expect("request")
            .is_none()
    );
}

#[test]
fn nonzero_compaction_uses_pointer_candidate_and_keeps_overlap_selection() {
    let branch = branch_id(0x6e);
    let mut state = BranchLocalState::empty(branch);
    let source_level = BranchLevel::new(1);
    let next_level = BranchLevel::new(2);
    let large_value = vec![0x71; 4096];

    install_owned_table(
        &mut state,
        branch,
        source_level,
        "source-broad-overlap",
        vec![
            put_row(branch, b"a-000", 1, 1_000, &large_value),
            put_row(branch, b"m-999", 2, 2_000, &large_value),
        ],
    );
    install_owned_table(
        &mut state,
        branch,
        source_level,
        "source-bounded-primary",
        vec![put_row(branch, b"n-000", 3, 3_000, &[0x72; 128])],
    );
    install_owned_table(
        &mut state,
        branch,
        source_level,
        "source-bounded-secondary",
        vec![put_row(branch, b"o-000", 4, 4_000, &[0x73; 64])],
    );
    install_owned_table(
        &mut state,
        branch,
        source_level,
        "source-bounded-tertiary",
        vec![put_row(branch, b"p-000", 5, 5_000, &[0x74; 32])],
    );
    for (index, first, last) in [
        (0, b"b-000".as_slice(), b"b-999".as_slice()),
        (1, b"d-000".as_slice(), b"d-999".as_slice()),
        (2, b"h-000".as_slice(), b"h-999".as_slice()),
        (3, b"l-000".as_slice(), b"l-999".as_slice()),
    ] {
        install_owned_table(
            &mut state,
            branch,
            next_level,
            &format!("next-overlap-{index}"),
            vec![
                put_row(
                    branch,
                    first,
                    10 + index,
                    (10 + index) * 1_000,
                    &large_value,
                ),
                put_row(branch, last, 20 + index, (20 + index) * 1_000, &large_value),
            ],
        );
    }

    let task = MaintenanceTask::new_for_test(
        1,
        MaintenanceTaskRequest::compaction(branch, source_level.raw()),
    )
    .expect("maintenance task");
    let request = current_compaction_request_from_maintenance_task_with_budget(
        &task,
        &state,
        Some(StorageRuntimeBudget::low_memory_test_profile()),
    )
    .expect("request")
    .expect("current compaction request");

    assert_eq!(
        request.kind(),
        BranchCompactionKind::CompactLevel {
            level: source_level,
            table_index: 0,
        }
    );

    let expected_pointer =
        table_last_physical_key(&state.owned_levels()[usize::from(source_level.raw())][0]);
    let outcome = compact_cache_branch(&mut state, &request).expect("compaction");
    assert_eq!(
        outcome.maintenance_outcome().status(),
        MaintenanceOutcomeStatus::Completed
    );
    assert_eq!(
        state
            .compact_pointer(source_level)
            .expect("compact pointer advanced after rewrite"),
        &expected_pointer
    );
    let output_keys = state.owned_levels()[usize::from(next_level.raw())]
        .iter()
        .flat_map(crate::branch::read::BranchOwnedTable::rows)
        .map(|row| row.physical_key().user_key().to_vec())
        .collect::<Vec<_>>();
    for expected in [b"a-000".as_slice(), b"b-000", b"l-999", b"m-999"] {
        assert!(
            output_keys
                .iter()
                .any(|actual| actual.as_slice() == expected),
            "expected compacted output to retain overlapping key {:?}",
            std::str::from_utf8(expected).expect("fixture key is utf8")
        );
    }
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
fn queued_cache_compaction_moves_only_the_requested_table_level() {
    let branch = branch_id(0x5c);
    let mut runtime = cache_runtime(branch);
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                crate::commit::CommitBranchGenerationGuard::exact(
                    crate::commit::CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        install_owned_table(
            state,
            branch,
            BranchLevel::new(1),
            "queued-nonzero-input",
            vec![put_row(branch, b"queued-nonzero", 1, 1_000, b"value")],
        );
    }
    let compaction = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 1))
        .expect("enqueue compaction");

    let outcome = runtime
        .run_next_compaction_maintenance()
        .expect("run compaction")
        .expect("compaction outcome");
    let state = runtime.branch_state();

    assert_eq!(outcome.task_id(), Some(compaction.task_id()));
    assert_eq!(outcome.task_kind(), MaintenanceTaskKind::Compaction);
    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(state.owned_levels()[1].len(), 0);
    assert_eq!(state.owned_levels()[2].len(), 1);
    assert_eq!(state.owned_levels()[3].len(), 0);
}

#[test]
fn compaction_pressure_selects_highest_scored_level() {
    let branch = branch_id(0x6b);
    let mut l0_dominant = BranchLocalState::empty(branch);
    for index in 0..8 {
        install_l0_table(
            &mut l0_dominant,
            branch,
            &format!("score-l0-dominant-l0-{index}"),
            vec![put_row(
                branch,
                format!("l0-dominant-{index}").as_bytes(),
                index + 1,
                (index + 1) * 1_000,
                b"value",
            )],
        );
    }
    for index in 0..4 {
        install_owned_table(
            &mut l0_dominant,
            branch,
            BranchLevel::new(1),
            &format!("score-l0-dominant-l1-{index}"),
            vec![put_row(
                branch,
                format!("l1-smaller-{index}").as_bytes(),
                index + 20,
                (index + 20) * 1_000,
                b"value",
            )],
        );
    }
    let l0_pressure = collect_storage_pressure(&l0_dominant, empty_maintenance_status());
    assert_eq!(
        l0_pressure.reason(),
        LifecycleStoragePressureReason::LevelZeroTableBacklog
    );
    assert!(matches!(
        l0_pressure.suggested_task().map(MaintenanceTaskRequest::scope),
        Some(MaintenanceTaskScope::TableLevel {
            branch_id,
            level: 0
        }) if branch_id == branch
    ));

    let mut nonzero_dominant = BranchLocalState::empty(branch);
    for index in 0..4 {
        install_l0_table(
            &mut nonzero_dominant,
            branch,
            &format!("score-nonzero-dominant-l0-{index}"),
            vec![put_row(
                branch,
                format!("l0-smaller-{index}").as_bytes(),
                index + 40,
                (index + 40) * 1_000,
                b"value",
            )],
        );
    }
    for index in 0..6 {
        install_owned_table(
            &mut nonzero_dominant,
            branch,
            BranchLevel::new(1),
            &format!("score-nonzero-dominant-l1-{index}"),
            vec![put_row(
                branch,
                format!("l1-dominant-{index}").as_bytes(),
                index + 60,
                (index + 60) * 1_000,
                b"value",
            )],
        );
    }
    let nonzero_pressure = collect_storage_pressure(&nonzero_dominant, empty_maintenance_status());
    assert_eq!(
        nonzero_pressure.reason(),
        LifecycleStoragePressureReason::NonZeroLevelTableBacklog
    );
    assert!(matches!(
        nonzero_pressure
            .suggested_task()
            .map(MaintenanceTaskRequest::scope),
        Some(MaintenanceTaskScope::TableLevel {
            branch_id,
            level: 1
        }) if branch_id == branch
    ));
}

#[test]
fn queued_compaction_runs_highest_scored_branch_first() {
    let branch_a = branch_id(0x6c);
    let branch_b = branch_id(0x6d);
    let mut runtime = cache_runtime(branch_a);
    runtime
        .create_branch(
            branch_b,
            crate::commit::CommitBranchGeneration::new(1).expect("generation"),
            None,
        )
        .expect("create branch-b");
    {
        let catalog = runtime.branch_catalog_mut_for_test();
        let state_a = catalog
            .branch_state_mut(
                branch_a,
                crate::commit::CommitBranchGenerationGuard::exact(
                    crate::commit::CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch-a state");
        for index in 0..4 {
            install_l0_table(
                state_a,
                branch_a,
                &format!("score-branch-a-l0-{index}"),
                vec![put_row(
                    branch_a,
                    format!("branch-a-{index}").as_bytes(),
                    index + 1,
                    (index + 1) * 1_000,
                    b"value",
                )],
            );
        }
        let state_b = catalog
            .branch_state_mut(
                branch_b,
                crate::commit::CommitBranchGenerationGuard::exact(
                    crate::commit::CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch-b state");
        for index in 0..6 {
            install_owned_table(
                state_b,
                branch_b,
                BranchLevel::new(1),
                &format!("score-branch-b-l1-{index}"),
                vec![put_row(
                    branch_b,
                    format!("branch-b-{index}").as_bytes(),
                    index + 20,
                    (index + 20) * 1_000,
                    b"value",
                )],
            );
        }
    }
    let low_pressure_task = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch_a, 0))
        .expect("enqueue branch-a");
    let high_pressure_task = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch_b, 1))
        .expect("enqueue branch-b");

    let outcome = runtime
        .run_next_compaction_maintenance()
        .expect("run compaction")
        .expect("compaction outcome");
    let branch_b_state = runtime
        .branch_catalog()
        .branch_state(branch_b)
        .expect("branch-b state");

    assert_eq!(outcome.task_id(), Some(high_pressure_task.task_id()));
    assert_ne!(outcome.task_id(), Some(low_pressure_task.task_id()));
    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(runtime.branch_state().owned_levels()[0].len(), 4);
    assert_eq!(branch_b_state.owned_levels()[1].len(), 5);
    assert_eq!(branch_b_state.owned_levels()[2].len(), 1);
}

#[test]
fn queued_nonzero_compaction_starts_at_first_pointer_candidate() {
    let branch = branch_id(0x5b);
    let mut runtime = cache_runtime(branch);
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                crate::commit::CommitBranchGenerationGuard::exact(
                    crate::commit::CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        for index in 0_u64..4 {
            install_owned_table(
                state,
                branch,
                BranchLevel::new(1),
                &format!("pointer-start-input-{index}"),
                vec![put_row(
                    branch,
                    format!("pointer-start-{index}").as_bytes(),
                    index + 1,
                    (index + 1) * 1_000,
                    b"same-size",
                )],
            );
        }
    }
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 1))
        .expect("enqueue compaction");

    let outcome = runtime
        .run_next_compaction_maintenance()
        .expect("run compaction")
        .expect("compaction outcome");
    let state = runtime.branch_state();
    let promoted_key = state.owned_levels()[2][0].rows()[0]
        .physical_key()
        .user_key();
    let pointer = state
        .compact_pointer(BranchLevel::new(1))
        .expect("compact pointer advanced");

    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(state.owned_levels()[1].len(), 3);
    assert_eq!(promoted_key, b"pointer-start-0");
    assert_eq!(
        pointer,
        &table_last_physical_key(&state.owned_levels()[2][0])
    );
}

#[test]
fn queued_nonzero_compaction_advances_past_compact_pointer_and_wraps() {
    let branch = branch_id(0x6e);
    let mut runtime = cache_runtime(branch);
    let initial_pointer;
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                crate::commit::CommitBranchGenerationGuard::exact(
                    crate::commit::CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        for index in 0_u64..4 {
            install_owned_table(
                state,
                branch,
                BranchLevel::new(1),
                &format!("pointer-advance-input-{index}"),
                vec![put_row(
                    branch,
                    format!("pointer-advance-{index}").as_bytes(),
                    index + 1,
                    (index + 1) * 1_000,
                    b"same-size",
                )],
            );
        }
        initial_pointer = table_last_physical_key(&state.owned_levels()[1][1]);
        state.set_compact_pointer_for_test(BranchLevel::new(1), Some(initial_pointer.clone()));
    }
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 1))
        .expect("enqueue compaction");

    let first = runtime
        .run_next_compaction_maintenance()
        .expect("run first compaction")
        .expect("first compaction outcome");
    let state = runtime.branch_state();
    let promoted_key = state.owned_levels()[2][0].rows()[0]
        .physical_key()
        .user_key();

    assert_eq!(first.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(promoted_key, b"pointer-advance-2");
    assert_ne!(
        state
            .compact_pointer(BranchLevel::new(1))
            .expect("compact pointer advanced"),
        &initial_pointer
    );

    let wrap_pointer = table_last_physical_key(
        runtime.branch_state().owned_levels()[1]
            .last()
            .expect("remaining level-one table"),
    );
    runtime
        .branch_catalog_mut_for_test()
        .branch_state_mut(
            branch,
            crate::commit::CommitBranchGenerationGuard::exact(
                crate::commit::CommitBranchGeneration::new(1).expect("generation"),
            ),
        )
        .expect("branch state")
        .set_compact_pointer_for_test(BranchLevel::new(1), Some(wrap_pointer));
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 1))
        .expect("enqueue wrap compaction");

    let second = runtime
        .run_next_compaction_maintenance()
        .expect("run wrap compaction")
        .expect("wrap compaction outcome");
    let wrapped_key = runtime.branch_state().owned_levels()[2]
        .iter()
        .flat_map(crate::branch::read::BranchOwnedTable::rows)
        .find(|row| row.physical_key().user_key() == b"pointer-advance-0")
        .expect("wrapped table promoted")
        .physical_key()
        .user_key()
        .to_vec();

    assert_eq!(second.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(wrapped_key, b"pointer-advance-0");
}

#[test]
fn current_nonzero_compaction_request_uses_compact_pointer_table() {
    let branch = branch_id(0x5c);
    let mut state = BranchLocalState::empty(branch);
    for index in 0_u64..4 {
        install_owned_table(
            &mut state,
            branch,
            BranchLevel::new(1),
            &format!("selected-request-input-{index}"),
            vec![put_row(
                branch,
                format!("selected-request-{index}").as_bytes(),
                index + 1,
                (index + 1) * 1_000,
                b"same-size",
            )],
        );
    }
    let pointer = table_last_physical_key(&state.owned_levels()[1][0]);
    state.set_compact_pointer_for_test(BranchLevel::new(1), Some(pointer));
    let task = MaintenanceTask::new_for_test(1, MaintenanceTaskRequest::compaction(branch, 1))
        .expect("task");

    let request = current_compaction_request_from_maintenance_task(&task, &state)
        .expect("request")
        .expect("current compaction request");

    assert_eq!(
        request.kind(),
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::new(1),
            table_index: 1,
        }
    );
}

#[test]
fn completed_compaction_resubmits_chain_until_branch_is_healthy() {
    let branch = branch_id(0x6f);
    let mut runtime = cache_runtime(branch);
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                crate::commit::CommitBranchGenerationGuard::exact(
                    crate::commit::CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        for index in 0..5 {
            install_owned_table(
                state,
                branch,
                BranchLevel::new(1),
                &format!("score-chain-l1-{index}"),
                vec![put_row(
                    branch,
                    format!("chain-{index}").as_bytes(),
                    index + 1,
                    (index + 1) * 1_000,
                    b"value",
                )],
            );
        }
    }
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 1))
        .expect("enqueue compaction");

    let first = runtime
        .run_next_compaction_maintenance()
        .expect("run first")
        .expect("first outcome");
    assert_eq!(first.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(runtime.branch_state().owned_levels()[1].len(), 4);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);

    let second = runtime
        .run_next_compaction_maintenance()
        .expect("run second")
        .expect("second outcome");
    assert_eq!(second.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(runtime.branch_state().owned_levels()[1].len(), 3);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
}

#[test]
fn completed_compaction_resubmits_materialization_when_inheritance_remains() {
    let parent = branch_id(0xa8);
    let child = branch_id(0xa9);
    let mut runtime = cache_runtime(child);
    *runtime
        .branch_catalog_mut_for_test()
        .branch_state_mut(
            child,
            crate::commit::CommitBranchGenerationGuard::exact(
                crate::commit::CommitBranchGeneration::new(1).expect("generation"),
            ),
        )
        .expect("branch state") = single_inherited_state(parent, child);
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                child,
                crate::commit::CommitBranchGenerationGuard::exact(
                    crate::commit::CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        for index in 0..4 {
            install_l0_table(
                state,
                child,
                &format!("cross-rewrite-l0-{index}"),
                vec![put_row(
                    child,
                    format!("cross-rewrite-{index}").as_bytes(),
                    index + 10,
                    (index + 10) * 1_000,
                    b"value",
                )],
            );
        }
    }
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(child, 0))
        .expect("enqueue compaction");

    let compaction = runtime
        .run_next_compaction_maintenance()
        .expect("run compaction")
        .expect("compaction outcome");
    assert_eq!(compaction.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(runtime.branch_state().inherited_layer_count(), 1);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
    assert!(
        runtime
            .run_next_compaction_maintenance()
            .expect("run compaction again")
            .is_none(),
        "remaining rewrite pressure should be queued as materialization"
    );

    let materialization = runtime
        .run_next_materialization_maintenance()
        .expect("run materialization")
        .expect("materialization outcome");
    assert_eq!(
        materialization.status(),
        MaintenanceOutcomeStatus::Completed
    );
    assert_eq!(
        materialization.task_kind(),
        MaintenanceTaskKind::Materialization
    );
    assert_eq!(runtime.branch_state().inherited_layer_count(), 0);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
}

#[test]
fn materialization_pressure_competes_with_compaction_score() {
    let far = branch_id(0xa0);
    let near = branch_id(0xa1);
    let child = branch_id(0xa2);
    let mut child_state = inherited_chain_state(far, near, child);
    for index in 0..4 {
        install_l0_table(
            &mut child_state,
            child,
            &format!("mixed-pressure-l0-{index}"),
            vec![put_row(
                child,
                format!("mixed-pressure-{index}").as_bytes(),
                index + 10,
                (index + 10) * 1_000,
                b"value",
            )],
        );
    }

    let pressure = collect_storage_pressure(&child_state, empty_maintenance_status());

    assert_eq!(
        pressure.reason(),
        LifecycleStoragePressureReason::InheritedLayerBacklog
    );
    assert_eq!(pressure.inherited_layers(), 2);
    assert_eq!(pressure.level_zero_tables(), 4);
    assert!(matches!(
        pressure.suggested_task().map(MaintenanceTaskRequest::scope),
        Some(MaintenanceTaskScope::InheritedLayer { branch_id, .. }) if branch_id == child
    ));
}

#[test]
fn materialization_pressure_selects_highest_pressure_inherited_layer() {
    let far = branch_id(0xa6);
    let near = branch_id(0xa7);
    let child = branch_id(0xa8);
    let child_state = inherited_chain_state_with_far_backlog(far, near, child, 3);

    let pressure = collect_storage_pressure(&child_state, empty_maintenance_status());

    assert_eq!(
        pressure.reason(),
        LifecycleStoragePressureReason::InheritedLayerBacklog
    );
    assert_eq!(pressure.inherited_layers(), 2);
    assert!(matches!(
        pressure.suggested_task().map(MaintenanceTaskRequest::scope),
        Some(MaintenanceTaskScope::InheritedLayer {
            branch_id,
            layer_index: 1
        }) if branch_id == child
    ));
}

#[cfg(feature = "perf-trace")]
#[test]
fn materialization_score_perf_trace_records_layer_candidates() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let far = branch_id(0xa9);
    let near = branch_id(0xaa);
    let child = branch_id(0xab);
    let child_state = inherited_chain_state_with_far_backlog(far, near, child, 3);
    crate::observability::perf_trace::reset();

    let pressure = collect_storage_pressure(&child_state, empty_maintenance_status());
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(
        pressure.reason(),
        LifecycleStoragePressureReason::InheritedLayerBacklog
    );
    assert_eq!(perf.lifecycle_materialization_score_candidates(), 2);
    assert_eq!(perf.lifecycle_materialization_score_layer_index_sum(), 1);
    assert_eq!(perf.lifecycle_materialization_score_table_count(), 4);
    assert!(perf.lifecycle_materialization_score_byte_count() > 0);
}

#[test]
fn queued_table_rewrite_runs_highest_scored_work_first() {
    let far = branch_id(0xaa);
    let near = branch_id(0xab);
    let child = branch_id(0xac);
    let mut runtime = cache_runtime(child);
    *runtime
        .branch_catalog_mut_for_test()
        .branch_state_mut(
            child,
            crate::commit::CommitBranchGenerationGuard::exact(
                crate::commit::CommitBranchGeneration::new(1).expect("generation"),
            ),
        )
        .expect("branch state") = inherited_chain_state(far, near, child);
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                child,
                crate::commit::CommitBranchGenerationGuard::exact(
                    crate::commit::CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        for index in 0..4 {
            install_l0_table(
                state,
                child,
                &format!("queued-mixed-l0-{index}"),
                vec![put_row(
                    child,
                    format!("queued-mixed-{index}").as_bytes(),
                    index + 10,
                    (index + 10) * 1_000,
                    b"value",
                )],
            );
        }
    }
    let compaction = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(child, 0))
        .expect("enqueue compaction");
    let materialization = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::materialization_layer(child, 0))
        .expect("enqueue materialization");

    let first = runtime
        .run_next_table_rewrite_maintenance()
        .expect("run table rewrite")
        .expect("rewrite outcome");

    assert_eq!(first.task_id(), Some(materialization.task_id()));
    assert_eq!(first.task_kind(), MaintenanceTaskKind::Materialization);
    assert_eq!(first.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(runtime.branch_state().inherited_layer_count(), 1);

    let second = runtime
        .run_next_compaction_maintenance()
        .expect("run compaction")
        .expect("compaction outcome");
    assert_eq!(second.task_id(), Some(compaction.task_id()));
    assert_eq!(second.task_kind(), MaintenanceTaskKind::Compaction);
}

#[test]
fn queued_table_rewrite_runs_compaction_when_it_has_higher_score() {
    let parent = branch_id(0xad);
    let child = branch_id(0xae);
    let mut runtime = cache_runtime(child);
    *runtime
        .branch_catalog_mut_for_test()
        .branch_state_mut(
            child,
            crate::commit::CommitBranchGenerationGuard::exact(
                crate::commit::CommitBranchGeneration::new(1).expect("generation"),
            ),
        )
        .expect("branch state") = single_inherited_state(parent, child);
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                child,
                crate::commit::CommitBranchGenerationGuard::exact(
                    crate::commit::CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        for index in 0..8 {
            install_l0_table(
                state,
                child,
                &format!("queued-compaction-dominant-l0-{index}"),
                vec![put_row(
                    child,
                    format!("queued-compaction-dominant-{index}").as_bytes(),
                    index + 10,
                    (index + 10) * 1_000,
                    b"value",
                )],
            );
        }
    }
    let materialization = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::materialization_layer(child, 0))
        .expect("enqueue materialization");
    let compaction = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(child, 0))
        .expect("enqueue compaction");

    let first = runtime
        .run_next_table_rewrite_maintenance()
        .expect("run table rewrite")
        .expect("rewrite outcome");

    assert_eq!(first.task_id(), Some(compaction.task_id()));
    assert_eq!(first.task_kind(), MaintenanceTaskKind::Compaction);
    assert_eq!(first.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(runtime.branch_state().owned_levels()[0].len(), 0);
    assert_eq!(runtime.branch_state().inherited_layer_count(), 1);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);

    let second = runtime
        .run_next_materialization_maintenance()
        .expect("run materialization")
        .expect("materialization outcome");
    assert_eq!(second.task_id(), Some(materialization.task_id()));
    assert_eq!(second.task_kind(), MaintenanceTaskKind::Materialization);
    assert_eq!(second.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(runtime.branch_state().owned_levels()[0].len(), 1);
    assert_eq!(runtime.branch_state().inherited_layer_count(), 0);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
}

#[test]
fn completed_materialization_resubmits_chain_until_branch_is_healthy() {
    let far = branch_id(0xa3);
    let near = branch_id(0xa4);
    let child = branch_id(0xa5);
    let mut runtime = cache_runtime(child);
    *runtime
        .branch_catalog_mut_for_test()
        .branch_state_mut(
            child,
            crate::commit::CommitBranchGenerationGuard::exact(
                crate::commit::CommitBranchGeneration::new(1).expect("generation"),
            ),
        )
        .expect("branch state") = inherited_chain_state(far, near, child);
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::materialization_layer(child, 0))
        .expect("enqueue materialization");

    let first = runtime
        .run_next_materialization_maintenance()
        .expect("run first materialization")
        .expect("first materialization outcome");
    assert_eq!(first.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(runtime.branch_state().inherited_layer_count(), 1);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);

    let second = runtime
        .run_next_materialization_maintenance()
        .expect("run second materialization")
        .expect("second materialization outcome");
    assert_eq!(second.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(runtime.branch_state().inherited_layer_count(), 0);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
}

fn inherited_chain_state(far: BranchId, near: BranchId, child: BranchId) -> BranchLocalState {
    inherited_chain_state_with_far_backlog(far, near, child, 1)
}

fn inherited_chain_state_with_far_backlog(
    far: BranchId,
    near: BranchId,
    child: BranchId,
    far_table_count: usize,
) -> BranchLocalState {
    let mut far_state = BranchLocalState::empty(far);
    for index in 0..far_table_count {
        install_l0_table(
            &mut far_state,
            far,
            &format!("scored-materialization-far-{index}"),
            vec![put_row(
                far,
                format!("far-{index}").as_bytes(),
                1 + u64::try_from(index).expect("index fits"),
                1_000 + u64::try_from(index).expect("index fits"),
                b"far",
            )],
        );
    }
    let (mut near_state, _) = far_state.fork_into_empty_child(near).expect("fork near");
    install_l0_table(
        &mut near_state,
        near,
        "scored-materialization-near",
        vec![put_row(near, b"near", 100, 100_000, b"near")],
    );
    let (child_state, outcome) = near_state.fork_into_empty_child(child).expect("fork child");
    assert_eq!(outcome.inherited_layer_count(), 2);
    child_state
}

fn single_inherited_state(parent: BranchId, child: BranchId) -> BranchLocalState {
    let mut parent_state = BranchLocalState::empty(parent);
    install_l0_table(
        &mut parent_state,
        parent,
        "scored-materialization-parent",
        vec![put_row(parent, b"parent", 1, 1_000, b"parent")],
    );
    let (child_state, outcome) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child from parent");
    assert_eq!(outcome.inherited_layer_count(), 1);
    child_state
}

#[cfg(feature = "perf-trace")]
#[test]
fn storage_pressure_perf_trace_records_collection_shape_and_active_bytes() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0xb1);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::default()
            .with_active_rotation_bytes(1024 * 1024)
            .expect("custom active rotation threshold"),
    )
    .expect("branch state");
    let value = vec![0x51; 850 * 1024];
    state
        .append_committed_row(put_row(
            branch,
            b"pressure-collection-active-bytes",
            1,
            1_000,
            &value,
        ))
        .expect("append active pressure row");
    for index in 0..2 {
        install_l0_table(
            &mut state,
            branch,
            &format!("pressure-collection-l0-{index}"),
            vec![put_row(
                branch,
                format!("pressure-collection-{index}").as_bytes(),
                index + 2,
                (index + 2) * 1_000,
                b"value",
            )],
        );
    }
    crate::observability::perf_trace::reset();

    let pressure = collect_storage_pressure(&state, empty_maintenance_status());
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(
        pressure.reason(),
        LifecycleStoragePressureReason::ActiveMutableBytes
    );
    assert_eq!(
        pressure.severity(),
        LifecycleStoragePressureSeverity::Urgent
    );
    assert_eq!(perf.lifecycle_pressure_collection_calls(), 1);
    assert_eq!(perf.lifecycle_pressure_collection_branches_inspected(), 1);
    assert_eq!(
        perf.lifecycle_pressure_collection_levels_inspected(),
        u64::try_from(state.owned_levels().len()).expect("level count fits u64")
    );
    assert_eq!(perf.lifecycle_pressure_collection_tables_inspected(), 2);
    assert_eq!(perf.lifecycle_pressure_collection_sampling_skips(), 0);
    assert_eq!(perf.lifecycle_pressure_collection_full_scans(), 1);
    assert_eq!(perf.lifecycle_active_byte_pressure_urgent(), 1);
}

#[cfg(feature = "perf-trace")]
#[test]
fn storage_pressure_perf_trace_records_generated_active_byte_observations() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let rotation_bytes = 1024 * 1024;
    let states = [
        active_byte_pressure_state(branch_id(0xb3), rotation_bytes, 128 * 1024, 0),
        active_byte_pressure_state(branch_id(0xb4), rotation_bytes, 600 * 1024, 1),
        active_byte_pressure_state(branch_id(0xb5), rotation_bytes, 850 * 1024, 2),
        blocked_active_byte_pressure_state(branch_id(0xb6), 512 * 1024),
    ];
    crate::observability::perf_trace::reset();

    let pressures = states
        .iter()
        .map(|state| collect_storage_pressure(state, empty_maintenance_status()))
        .collect::<Vec<_>>();
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(
        pressures
            .iter()
            .map(|pressure| pressure.severity())
            .collect::<Vec<_>>(),
        vec![
            LifecycleStoragePressureSeverity::None,
            LifecycleStoragePressureSeverity::Background,
            LifecycleStoragePressureSeverity::Urgent,
            LifecycleStoragePressureSeverity::BlockMutatingAdmission,
        ]
    );
    assert_eq!(perf.lifecycle_pressure_collection_calls(), 4);
    assert_eq!(perf.lifecycle_pressure_collection_branches_inspected(), 4);
    assert_eq!(
        perf.lifecycle_pressure_collection_levels_inspected(),
        states
            .iter()
            .map(|state| u64::try_from(state.owned_levels().len()).expect("level count fits"))
            .sum::<u64>()
    );
    assert_eq!(perf.lifecycle_pressure_collection_tables_inspected(), 1);
    assert!(perf.lifecycle_pressure_collection_ns() > 0);
    assert_eq!(perf.lifecycle_pressure_collection_sampling_skips(), 0);
    assert_eq!(perf.lifecycle_pressure_collection_full_scans(), 4);
    assert_eq!(perf.lifecycle_active_byte_pressure_background(), 1);
    assert_eq!(perf.lifecycle_active_byte_pressure_urgent(), 1);
    assert_eq!(perf.lifecycle_active_byte_pressure_blocking(), 1);
}

#[cfg(feature = "perf-trace")]
#[test]
fn storage_pressure_counts_active_byte_signal_when_table_rewrite_wins() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0xb2);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::default()
            .with_active_rotation_bytes(1024 * 1024)
            .expect("custom active rotation threshold"),
    )
    .expect("branch state");
    let value = vec![0x52; 850 * 1024];
    state
        .append_committed_row(put_row(
            branch,
            b"pressure-active-byte-l0-precedence",
            1,
            1_000,
            &value,
        ))
        .expect("append active pressure row");
    for index in 0..8 {
        install_l0_table(
            &mut state,
            branch,
            &format!("active-byte-counter-l0-{index}"),
            vec![put_row(
                branch,
                format!("active-byte-counter-{index}").as_bytes(),
                index + 2,
                (index + 2) * 1_000,
                b"value",
            )],
        );
    }
    crate::observability::perf_trace::reset();

    let pressure = collect_storage_pressure(&state, empty_maintenance_status());
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(
        pressure.reason(),
        LifecycleStoragePressureReason::LevelZeroTableBacklog
    );
    assert_eq!(
        pressure.severity(),
        LifecycleStoragePressureSeverity::Urgent
    );
    assert_eq!(perf.lifecycle_active_byte_pressure_urgent(), 1);
    assert_eq!(perf.lifecycle_active_byte_pressure_background(), 0);
    assert_eq!(perf.lifecycle_active_byte_pressure_blocking(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_compaction_score_perf_trace_records_candidates_selection_and_operation() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0xa6);
    let mut runtime = cache_runtime(branch);
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                crate::commit::CommitBranchGenerationGuard::exact(
                    crate::commit::CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        for index in 0..4 {
            install_l0_table(
                state,
                branch,
                &format!("perf-score-l0-{index}"),
                vec![put_row(
                    branch,
                    format!("perf-score-l0-{index}").as_bytes(),
                    index + 1,
                    (index + 1) * 1_000,
                    b"value",
                )],
            );
        }
        for index in 0..6 {
            install_owned_table(
                state,
                branch,
                BranchLevel::new(1),
                &format!("perf-score-l1-{index}"),
                vec![put_row(
                    branch,
                    format!("perf-score-l1-{index}").as_bytes(),
                    index + 20,
                    (index + 20) * 1_000,
                    b"value",
                )],
            );
        }
    }
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 0))
        .expect("enqueue compaction");
    crate::observability::perf_trace::reset();

    let outcome = runtime
        .run_next_compaction_maintenance()
        .expect("run compaction")
        .expect("compaction outcome");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Completed);
    assert!(
        perf.lifecycle_compaction_score_candidates() >= 2,
        "scoring should evaluate L0 and nonzero candidates"
    );
    assert_eq!(perf.lifecycle_compaction_selected(), 1);
    assert_eq!(perf.lifecycle_compaction_selected_level_sum(), 1);
    assert_eq!(perf.lifecycle_compaction_operations_completed(), 1);
    assert!(perf.lifecycle_compaction_input_tables() > 0);
    assert!(perf.lifecycle_compaction_output_tables() > 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_compaction_perf_trace_records_io_budget_and_elapsed_facts() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x5e);
    let mut runtime = cache_runtime(branch);
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                crate::commit::CommitBranchGenerationGuard::exact(
                    crate::commit::CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        for index in 0..4 {
            install_l0_table(
                state,
                branch,
                &format!("io-account-l0-{index}"),
                vec![put_row(
                    branch,
                    format!("io-account-l0-{index}").as_bytes(),
                    index + 1,
                    (index + 1) * 1_000,
                    &[0x5a; 512],
                )],
            );
        }
    }
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 0))
        .expect("enqueue compaction");
    crate::observability::perf_trace::reset();

    let outcome = runtime
        .run_next_compaction_maintenance()
        .expect("run compaction")
        .expect("compaction outcome");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(perf.lifecycle_compaction_operations_completed(), 1);
    assert!(perf.lifecycle_compaction_input_bytes() > 0);
    assert!(perf.lifecycle_compaction_output_bytes() > 0);
    assert!(
        perf.lifecycle_compaction_io_budget_consumed_bytes()
            >= perf.lifecycle_compaction_input_bytes()
    );
    assert!(perf.lifecycle_compaction_elapsed_ns() > 0);
    assert!(perf.lifecycle_compaction_input_rows() > 0);
    assert_eq!(
        perf.lifecycle_compaction_rewrite_bytes_per_row(),
        perf.lifecycle_compaction_io_budget_consumed_bytes()
            / perf.lifecycle_compaction_input_rows()
    );
    assert!(perf.lifecycle_compaction_rewrite_bytes_per_row() > 0);
    assert_eq!(perf.lifecycle_compaction_io_budget_deferrals(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn metadata_promotion_compaction_records_avoided_rewrite_bytes() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x5f);
    let config = LifecycleConfig::default()
        .with_compaction_io_policy(LifecycleCompactionIoPolicy::per_task_byte_budget(1))
        .expect("compaction IO policy");
    let mut runtime = cache_runtime_with_config(branch, config);
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                crate::commit::CommitBranchGenerationGuard::exact(
                    crate::commit::CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        install_owned_table(
            state,
            branch,
            BranchLevel::ZERO,
            "metadata-avoid-l0",
            vec![put_row(branch, b"metadata-avoid", 1, 1_000, &[0x61; 512])],
        );
    }
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 0))
        .expect("enqueue compaction");
    crate::observability::perf_trace::reset();

    let outcome = runtime
        .run_next_compaction_maintenance()
        .expect("run compaction")
        .expect("compaction outcome");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(perf.lifecycle_compaction_trivial_moves(), 1);
    assert_eq!(perf.lifecycle_compaction_io_budget_deferrals(), 0);
    assert_eq!(perf.lifecycle_compaction_output_bytes(), 0);
    assert_eq!(perf.lifecycle_compaction_input_bytes(), 0);
    assert_eq!(perf.lifecycle_compaction_input_rows(), 0);
    assert!(perf.lifecycle_compaction_metadata_bytes_avoided() > 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn compaction_perf_trace_bytes_per_row_is_weighted_across_operations() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x62);
    let mut runtime = cache_runtime(branch);
    crate::observability::perf_trace::reset();

    for pass in 0_u64..2 {
        {
            let state = runtime
                .branch_catalog_mut_for_test()
                .branch_state_mut(
                    branch,
                    crate::commit::CommitBranchGenerationGuard::exact(
                        crate::commit::CommitBranchGeneration::new(1).expect("generation"),
                    ),
                )
                .expect("branch state");
            for index in 0_u64..4 {
                let key = format!("weighted-pass-{pass}-l0-{index}");
                install_l0_table(
                    state,
                    branch,
                    &key,
                    vec![put_row(
                        branch,
                        key.as_bytes(),
                        pass * 10 + index + 1,
                        (pass * 10 + index + 1) * 1_000,
                        &[0x66; 512],
                    )],
                );
            }
        }
        runtime
            .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 0))
            .expect("enqueue compaction");
        let outcome = runtime
            .run_next_compaction_maintenance()
            .expect("run compaction")
            .expect("compaction outcome");
        assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Completed);
    }

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_compaction_operations_completed(), 2);
    assert!(perf.lifecycle_compaction_input_rows() > 0);
    assert_eq!(
        perf.lifecycle_compaction_rewrite_bytes_per_row(),
        perf.lifecycle_compaction_io_budget_consumed_bytes()
            / perf.lifecycle_compaction_input_rows()
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn constrained_compaction_io_budget_defers_without_mutating_sources() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x60);
    let config = LifecycleConfig::default()
        .with_compaction_io_policy(LifecycleCompactionIoPolicy::per_task_byte_budget(1))
        .expect("compaction IO policy");
    let mut runtime = cache_runtime_with_config(branch, config);
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                crate::commit::CommitBranchGenerationGuard::exact(
                    crate::commit::CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        for index in 0..4 {
            install_l0_table(
                state,
                branch,
                &format!("io-budget-l0-{index}"),
                vec![put_row(
                    branch,
                    format!("io-budget-l0-{index}").as_bytes(),
                    index + 1,
                    (index + 1) * 1_000,
                    &[0x62; 512],
                )],
            );
        }
    }
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 0))
        .expect("enqueue compaction");
    crate::observability::perf_trace::reset();

    let outcome = runtime
        .run_next_compaction_maintenance()
        .expect("run compaction")
        .expect("compaction outcome");
    let state = runtime
        .branch_catalog_mut_for_test()
        .branch_state_mut(
            branch,
            crate::commit::CommitBranchGenerationGuard::exact(
                crate::commit::CommitBranchGeneration::new(1).expect("generation"),
            ),
        )
        .expect("branch state after defer");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Deferred);
    assert!(outcome.retryable());
    assert!(outcome.recovery_health().is_some());
    assert_eq!(
        outcome.reason(),
        Some("compaction IO byte budget deferred table rewrite")
    );
    assert_eq!(state.owned_levels()[0].len(), 4);
    assert_eq!(perf.lifecycle_compaction_operations_completed(), 0);
    assert_eq!(perf.lifecycle_compaction_io_budget_deferrals(), 1);
    assert!(perf.lifecycle_compaction_io_budget_deferred_bytes() > 1);
    assert_eq!(perf.lifecycle_compaction_io_budget_limit_bytes(), 1);
}

#[cfg(feature = "perf-trace")]
#[test]
fn compaction_resource_policy_is_deterministic_for_repeated_budget_checks() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x61);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(5, 64, 32).expect("branch config"),
    )
    .expect("branch state");
    for index in 0..4 {
        install_l0_table(
            &mut state,
            branch,
            &format!("deterministic-budget-l0-{index}"),
            vec![put_row(
                branch,
                format!("deterministic-budget-l0-{index}").as_bytes(),
                index + 1,
                (index + 1) * 1_000,
                &[0x63; 512],
            )],
        );
    }
    let task = MaintenanceTask::new_for_test(1, MaintenanceTaskRequest::compaction(branch, 0))
        .expect("task");
    let request = current_compaction_request_from_maintenance_task(&task, &state)
        .expect("request")
        .expect("current request");
    crate::observability::perf_trace::reset();

    let first = defer_compaction_for_resource_policy(
        &state,
        &request,
        LifecycleCompactionIoPolicy::per_task_byte_budget(1),
    )
    .expect("first policy")
    .expect("first defer");
    let second = defer_compaction_for_resource_policy(
        &state,
        &request,
        LifecycleCompactionIoPolicy::per_task_byte_budget(1),
    )
    .expect("second policy")
    .expect("second defer");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(first.status(), MaintenanceOutcomeStatus::Deferred);
    assert_eq!(second.status(), MaintenanceOutcomeStatus::Deferred);
    assert_eq!(first.reason(), second.reason());
    assert_eq!(perf.lifecycle_compaction_io_budget_deferrals(), 2);
    assert_eq!(
        perf.lifecycle_compaction_io_budget_deferred_bytes() % 2,
        0,
        "repeated checks should emit the same estimated byte cost"
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn generated_compaction_io_budget_sweep_defers_rewrites_by_estimated_size() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    crate::observability::perf_trace::reset();
    let mut previous_deferred_bytes = 0;

    for (case_index, value_len) in [64usize, 256, 1024, 4096].into_iter().enumerate() {
        let branch = branch_id(0x80 + u8::try_from(case_index).expect("case fits"));
        let mut state = BranchLocalState::new(
            branch,
            BranchRuntimeConfig::new(5, 64, 32).expect("branch config"),
        )
        .expect("branch state");
        for table_index in 0_u64..4 {
            let key = format!("budget-sweep-{case_index}-{table_index}");
            install_l0_table(
                &mut state,
                branch,
                &key,
                vec![put_row(
                    branch,
                    key.as_bytes(),
                    table_index + 1,
                    (table_index + 1) * 1_000,
                    &vec![0x67; value_len],
                )],
            );
        }
        let task = MaintenanceTask::new_for_test(1, MaintenanceTaskRequest::compaction(branch, 0))
            .expect("task");
        let request = current_compaction_request_from_maintenance_task(&task, &state)
            .expect("request")
            .expect("current request");
        assert!(
            defer_compaction_for_resource_policy(
                &state,
                &request,
                LifecycleCompactionIoPolicy::Unlimited,
            )
            .expect("unlimited policy")
            .is_none(),
            "unlimited policy must not defer rewrite case {case_index}"
        );
        let deferred = defer_compaction_for_resource_policy(
            &state,
            &request,
            LifecycleCompactionIoPolicy::per_task_byte_budget(1),
        )
        .expect("budget policy")
        .expect("budget should defer rewrite");
        let perf = crate::observability::perf_trace::snapshot();

        assert_eq!(deferred.status(), MaintenanceOutcomeStatus::Deferred);
        assert!(deferred.retryable());
        assert_eq!(
            deferred.reason(),
            Some("compaction IO byte budget deferred table rewrite")
        );
        assert!(
            perf.lifecycle_compaction_io_budget_deferred_bytes() > previous_deferred_bytes,
            "larger generated payloads should increase deferred byte accounting"
        );
        previous_deferred_bytes = perf.lifecycle_compaction_io_budget_deferred_bytes();
    }

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_compaction_io_budget_deferrals(), 4);
    assert_eq!(perf.lifecycle_compaction_io_budget_limit_bytes(), 4);
    assert_eq!(perf.lifecycle_compaction_operations_completed(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn generated_flush_and_compaction_pressure_overlap_preempts_rewrites() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    crate::observability::perf_trace::reset();

    for frozen_tables in 1_usize..=3 {
        let branch = branch_id(0x88 + u8::try_from(frozen_tables).expect("case fits"));
        let mut state = BranchLocalState::new(
            branch,
            BranchRuntimeConfig::new(5, 64, 32).expect("branch config"),
        )
        .expect("branch state");
        for table_index in 0_u64..4 {
            let key = format!("overlap-l0-{frozen_tables}-{table_index}");
            install_l0_table(
                &mut state,
                branch,
                &key,
                vec![put_row(
                    branch,
                    key.as_bytes(),
                    table_index + 1,
                    (table_index + 1) * 1_000,
                    &[0x68; 512],
                )],
            );
        }
        for frozen_index in 0..frozen_tables {
            let frozen_index_u64 = u64::try_from(frozen_index).expect("index fits");
            state
                .append_committed_rows_atomically(vec![put_row(
                    branch,
                    format!("overlap-frozen-{frozen_tables}-{frozen_index}").as_bytes(),
                    100 + frozen_index_u64,
                    100_000 + frozen_index_u64,
                    &[0x69; 512],
                )])
                .expect("append frozen row");
            state.rotate_active();
        }
        let task = MaintenanceTask::new_for_test(1, MaintenanceTaskRequest::compaction(branch, 0))
            .expect("task");
        let request = current_compaction_request_from_maintenance_task(&task, &state)
            .expect("request")
            .expect("current request");
        let preempted = defer_compaction_for_resource_policy(
            &state,
            &request,
            LifecycleCompactionIoPolicy::Unlimited,
        )
        .expect("policy")
        .expect("flush pressure should preempt compaction");

        assert_eq!(preempted.status(), MaintenanceOutcomeStatus::Deferred);
        assert!(preempted.retryable());
        assert_eq!(
            preempted.reason(),
            Some("flush pressure preempted compaction")
        );
    }

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_compaction_flush_preemptions(), 3);
    assert_eq!(perf.lifecycle_compaction_io_budget_deferrals(), 0);
    assert_eq!(perf.lifecycle_compaction_operations_completed(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn generated_metadata_promotion_and_rewrite_candidates_follow_io_budget_policy() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    crate::observability::perf_trace::reset();

    for (case_index, table_count) in [(0usize, 1usize), (1, 2)] {
        let branch = branch_id(0x90 + u8::try_from(case_index).expect("case fits"));
        let mut state = BranchLocalState::new(
            branch,
            BranchRuntimeConfig::new(5, 64, 32).expect("branch config"),
        )
        .expect("branch state");
        for table_index in 0..table_count {
            let key = format!("promotion-vs-rewrite-{case_index}-{table_index}");
            install_l0_table(
                &mut state,
                branch,
                &key,
                vec![put_row(
                    branch,
                    key.as_bytes(),
                    (table_index + 1) as u64,
                    ((table_index + 1) * 1_000) as u64,
                    &[0x6a; 1024],
                )],
            );
        }
        let task = MaintenanceTask::new_for_test(1, MaintenanceTaskRequest::compaction(branch, 0))
            .expect("task");
        let request = current_compaction_request_from_maintenance_task(&task, &state)
            .expect("request")
            .expect("current request");
        let outcome = defer_compaction_for_resource_policy(
            &state,
            &request,
            LifecycleCompactionIoPolicy::per_task_byte_budget(1),
        )
        .expect("policy");

        if table_count == 1 {
            assert!(
                outcome.is_none(),
                "metadata promotion should avoid rewrite IO budget deferral"
            );
        } else {
            let deferred = outcome.expect("rewrite should be budget deferred");
            assert_eq!(deferred.status(), MaintenanceOutcomeStatus::Deferred);
            assert_eq!(
                deferred.reason(),
                Some("compaction IO byte budget deferred table rewrite")
            );
        }
    }

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_compaction_io_budget_deferrals(), 1);
    assert!(perf.lifecycle_compaction_io_budget_deferred_bytes() > 1);
}

#[cfg(feature = "perf-trace")]
#[test]
fn compaction_shape_perf_trace_records_targets_and_input_policy() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x5d);
    let mut runtime = cache_runtime(branch);
    let expected_selected_target;
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                crate::commit::CommitBranchGenerationGuard::exact(
                    crate::commit::CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        let small_value = vec![b's'; 16];
        let large_value = vec![b'l'; 4096];
        for (index, value) in [
            small_value.as_slice(),
            small_value.as_slice(),
            large_value.as_slice(),
            small_value.as_slice(),
        ]
        .into_iter()
        .enumerate()
        {
            install_owned_table(
                state,
                branch,
                BranchLevel::new(1),
                &format!("perf-input-policy-{index}"),
                vec![put_row(
                    branch,
                    format!("perf-input-policy-{index}").as_bytes(),
                    index as u64 + 1,
                    (index as u64 + 1) * 1_000,
                    value,
                )],
            );
        }
        install_owned_table(
            state,
            branch,
            BranchLevel::new(3),
            "perf-deeper-overlap",
            vec![put_row(
                branch,
                b"perf-input-policy-0",
                20,
                20_000,
                b"deeper",
            )],
        );
        let expected_targets = target_bytes_for_state(state);
        assert!(
            expected_targets.len() > usize::from(BranchLevel::new(1).raw()),
            "fixture should include level one target"
        );
        expected_selected_target = expected_targets[usize::from(BranchLevel::new(1).raw())];
    }
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 1))
        .expect("enqueue compaction");
    crate::observability::perf_trace::reset();

    let outcome = runtime
        .run_next_compaction_maintenance()
        .expect("run compaction")
        .expect("compaction outcome");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Completed);
    assert!(perf.lifecycle_compaction_level_target_evaluations() >= 1);
    assert!(perf.lifecycle_compaction_level_target_bytes() >= expected_selected_target);
    assert_eq!(perf.lifecycle_compaction_nonzero_input_selections(), 1);
    assert_eq!(perf.lifecycle_compaction_nonzero_input_table_index_sum(), 0);
    assert!(perf.lifecycle_compaction_nonzero_input_bytes() > 0);
    assert_eq!(perf.lifecycle_compaction_nonzero_input_rows(), 1);
    assert_eq!(perf.lifecycle_compaction_deeper_overlap_evaluations(), 1);
    assert!(perf.lifecycle_compaction_deeper_overlap_bytes() > 0);
    assert_eq!(perf.lifecycle_compaction_output_split_budget_applied(), 0);
    assert_eq!(perf.lifecycle_compaction_output_split_budget_deferred(), 1);
    assert_eq!(
        perf.lifecycle_compaction_output_split_budget_deferred_bytes(),
        perf.lifecycle_compaction_deeper_overlap_bytes()
    );
    assert_eq!(
        perf.lifecycle_compaction_selected_target_bytes(),
        expected_selected_target
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn generated_deeper_overlap_layouts_record_split_budget_decision() {
    let cases: &[(u8, &[&[u8]], bool)] = &[
        (0x80, &[b"a-10", b"z-10"], false),
        (0x81, &[b"a-00"], true),
        (0x82, &[b"a-00", b"a-01"], true),
    ];

    for (branch_byte, deeper_keys, expects_overlap) in cases {
        let _capture = crate::observability::perf_trace::begin_test_capture();
        let branch = branch_id(*branch_byte);
        let state = compaction_shape_state_with_deeper_overlap(branch, deeper_keys);
        let task = MaintenanceTask::new_for_test(1, MaintenanceTaskRequest::compaction(branch, 1))
            .expect("task");
        crate::observability::perf_trace::reset();

        let request = current_compaction_request_from_maintenance_task(&task, &state)
            .expect("request")
            .expect("current request");
        let perf = crate::observability::perf_trace::snapshot();

        assert_eq!(
            request.kind(),
            BranchCompactionKind::CompactLevel {
                level: BranchLevel::new(1),
                table_index: 0,
            }
        );
        assert_eq!(perf.lifecycle_compaction_nonzero_input_selections(), 1);
        assert_eq!(perf.lifecycle_compaction_deeper_overlap_evaluations(), 1);
        assert_eq!(perf.lifecycle_compaction_output_split_budget_applied(), 0);
        if *expects_overlap {
            assert!(
                perf.lifecycle_compaction_deeper_overlap_bytes() > 0,
                "case {branch_byte:#x} should record deeper overlap bytes"
            );
            assert_eq!(perf.lifecycle_compaction_output_split_budget_deferred(), 1);
        } else {
            assert_eq!(perf.lifecycle_compaction_deeper_overlap_bytes(), 0);
            assert_eq!(perf.lifecycle_compaction_output_split_budget_deferred(), 0);
        }
        assert_eq!(
            perf.lifecycle_compaction_output_split_budget_deferred_bytes(),
            perf.lifecycle_compaction_deeper_overlap_bytes()
        );
    }
}

#[cfg(feature = "perf-trace")]
fn compaction_shape_state_with_deeper_overlap(
    branch: BranchId,
    deeper_keys: &[&[u8]],
) -> BranchLocalState {
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(5, 64, 32).expect("branch config"),
    )
    .expect("branch state");
    for (index, key) in [b"a-00".as_slice(), b"b-00".as_slice()]
        .into_iter()
        .enumerate()
    {
        let version = u64::try_from(index).expect("fixture index fits in u64") + 1;
        install_owned_table(
            &mut state,
            branch,
            BranchLevel::new(1),
            &format!("overlap-small-left-{index}"),
            vec![put_row(branch, key, version, version * 1_000, b"small")],
        );
    }
    install_owned_table(
        &mut state,
        branch,
        BranchLevel::new(1),
        "overlap-selected",
        vec![
            put_row(branch, b"m-20", 10, 10_000, &[0x10; 512]),
            put_row(branch, b"m-80", 11, 11_000, &[0x11; 512]),
        ],
    );
    install_owned_table(
        &mut state,
        branch,
        BranchLevel::new(1),
        "overlap-small-right",
        vec![put_row(branch, b"z-00", 12, 12_000, b"small")],
    );
    for (index, key) in deeper_keys.iter().enumerate() {
        let version = u64::try_from(index).expect("fixture index fits in u64") + 20;
        install_owned_table(
            &mut state,
            branch,
            BranchLevel::new(3),
            &format!("overlap-deeper-{index}"),
            vec![put_row(branch, key, version, version * 1_000, b"deeper")],
        );
    }
    state
}

#[cfg(feature = "perf-trace")]
#[test]
fn storage_pressure_perf_trace_records_level_specific_targets() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0x5e);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(5, 64, 32).expect("branch config"),
    )
    .expect("branch state");
    for index in 0..4 {
        install_owned_table(
            &mut state,
            branch,
            BranchLevel::new(1),
            &format!("target-facts-l1-{index}"),
            vec![put_row(
                branch,
                format!("target-facts-l1-{index}").as_bytes(),
                index + 1,
                (index + 1) * 1_000,
                b"value",
            )],
        );
        install_owned_table(
            &mut state,
            branch,
            BranchLevel::new(2),
            &format!("target-facts-l2-{index}"),
            vec![put_row(
                branch,
                format!("target-facts-l2-{index}").as_bytes(),
                index + 10,
                (index + 10) * 1_000,
                b"value",
            )],
        );
    }
    let expected_targets = target_bytes_for_state(&state);
    crate::observability::perf_trace::reset();

    let pressure = collect_storage_pressure(&state, empty_maintenance_status());
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(
        pressure.reason(),
        LifecycleStoragePressureReason::NonZeroLevelTableBacklog
    );
    assert_eq!(perf.lifecycle_compaction_level_target_evaluations(), 3);
    assert_eq!(
        perf.lifecycle_compaction_level_target_level_sum(),
        1 + 2 + 3
    );
    assert_eq!(
        perf.lifecycle_compaction_level_target_bytes(),
        expected_targets[1] + expected_targets[2] + expected_targets[3]
    );
    assert_ne!(expected_targets[1], expected_targets[2]);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_compaction_chain_perf_trace_records_resubmit() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0xa7);
    let mut runtime = cache_runtime(branch);
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                crate::commit::CommitBranchGenerationGuard::exact(
                    crate::commit::CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        for index in 0..5 {
            install_owned_table(
                state,
                branch,
                BranchLevel::new(1),
                &format!("perf-resubmit-l1-{index}"),
                vec![put_row(
                    branch,
                    format!("perf-resubmit-{index}").as_bytes(),
                    index + 1,
                    (index + 1) * 1_000,
                    b"value",
                )],
            );
        }
    }
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 1))
        .expect("enqueue compaction");
    crate::observability::perf_trace::reset();

    let outcome = runtime
        .run_next_compaction_maintenance()
        .expect("run compaction")
        .expect("compaction outcome");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
    assert_eq!(perf.lifecycle_compaction_resubmits(), 1);
    assert_eq!(perf.lifecycle_compaction_resubmit_deferred(), 0);
    assert_eq!(perf.lifecycle_table_rewrite_post_operation_scores(), 1);
    assert_eq!(perf.lifecycle_table_rewrite_post_operation_remaining(), 1);
    assert_eq!(
        perf.lifecycle_table_rewrite_post_operation_score_sum(),
        1_000
    );
    assert_eq!(perf.lifecycle_table_rewrite_post_operation_item_count(), 4);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_compaction_chain_perf_trace_records_terminal_score() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0xaf);
    let mut runtime = cache_runtime(branch);
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                crate::commit::CommitBranchGenerationGuard::exact(
                    crate::commit::CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        for index in 0..4 {
            install_l0_table(
                state,
                branch,
                &format!("perf-terminal-l0-{index}"),
                vec![put_row(
                    branch,
                    format!("perf-terminal-{index}").as_bytes(),
                    index + 1,
                    (index + 1) * 1_000,
                    b"value",
                )],
            );
        }
    }
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 0))
        .expect("enqueue compaction");
    crate::observability::perf_trace::reset();

    let outcome = runtime
        .run_next_compaction_maintenance()
        .expect("run compaction")
        .expect("compaction outcome");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    assert_eq!(perf.lifecycle_compaction_resubmits(), 0);
    assert_eq!(perf.lifecycle_compaction_resubmit_deferred(), 0);
    assert_eq!(perf.lifecycle_table_rewrite_post_operation_scores(), 1);
    assert_eq!(perf.lifecycle_table_rewrite_post_operation_remaining(), 0);
    assert_eq!(perf.lifecycle_table_rewrite_post_operation_score_sum(), 0);
    assert_eq!(perf.lifecycle_table_rewrite_post_operation_item_count(), 0);
    assert_eq!(perf.lifecycle_table_rewrite_post_operation_byte_count(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_compaction_chain_perf_trace_records_deferred_resubmit() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(0xb0);
    let config = LifecycleConfig::new(
        1,
        64,
        LifecycleCloseTimeoutPolicy::ReturnTypedTimeout,
        LifecycleLossyRecoveryPolicy::Disabled,
    )
    .expect("single-slot queue config");
    let mut runtime = cache_runtime_with_config(branch, config);
    {
        let state = runtime
            .branch_catalog_mut_for_test()
            .branch_state_mut(
                branch,
                crate::commit::CommitBranchGenerationGuard::exact(
                    crate::commit::CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("branch state");
        for index in 0..5 {
            install_owned_table(
                state,
                branch,
                BranchLevel::new(1),
                &format!("perf-deferred-resubmit-l1-{index}"),
                vec![put_row(
                    branch,
                    format!("perf-deferred-resubmit-{index}").as_bytes(),
                    index + 1,
                    (index + 1) * 1_000,
                    b"value",
                )],
            );
        }
    }
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::health_collection())
        .expect("fill maintenance queue");
    crate::observability::perf_trace::reset();

    runtime.resubmit_table_rewrite_if_branch_still_unhealthy_for_test(branch);
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
    assert_eq!(perf.lifecycle_compaction_resubmits(), 0);
    assert_eq!(perf.lifecycle_compaction_resubmit_deferred(), 1);
    assert_eq!(perf.lifecycle_table_rewrite_post_operation_scores(), 1);
    assert_eq!(perf.lifecycle_table_rewrite_post_operation_remaining(), 1);
    assert_eq!(
        perf.lifecycle_table_rewrite_post_operation_score_sum(),
        1_250
    );
    assert_eq!(perf.lifecycle_table_rewrite_post_operation_item_count(), 5);
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
