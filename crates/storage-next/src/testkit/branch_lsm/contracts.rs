pub fn check_branch_lsm_scaffold_contract(
    script: &[u8],
) -> Result<BranchLsmScaffoldOutcome, TestkitError> {
    let mut outcome = BranchLsmScaffoldOutcome::default();

    check_valid_config(script)?;
    outcome.valid_config += 1;
    outcome.invalid_config += check_invalid_configs()?;

    check_read_bounds(script)?;
    outcome.read_bounds += 1;

    check_valid_facts(script)?;
    outcome.valid_facts += 1;
    outcome.invalid_facts += check_invalid_facts(script)?;

    check_descriptors(script)?;
    outcome.descriptors += 1;

    check_error_sources()?;
    outcome.error_sources += 1;

    let identity_outcome = check_row_identity_and_rewrites(script)?;
    outcome.matching_rows += identity_outcome.matching_rows;
    outcome.mismatching_rows += identity_outcome.mismatching_rows;
    outcome.physical_key_rewrites += identity_outcome.physical_key_rewrites;
    outcome.row_rewrites += identity_outcome.row_rewrites;

    let bounds_outcome = check_effective_bounds_and_candidates(script)?;
    outcome.own_bounds += bounds_outcome.own_bounds;
    outcome.inherited_bounds += bounds_outcome.inherited_bounds;
    outcome.candidate_puts += bounds_outcome.candidate_puts;
    outcome.candidate_tombstones += bounds_outcome.candidate_tombstones;

    let edge_outcome = check_edge_rows_and_encoded_grouping(script)?;
    outcome.edge_rows += edge_outcome.edge_rows;
    outcome.encoded_grouping += edge_outcome.encoded_grouping;

    let chain_outcome = check_row_chains_and_fork_edges(script)?;
    outcome.row_chains += chain_outcome.row_chains;
    outcome.fork_edges += chain_outcome.fork_edges;

    outcome.absorb_state_outcome(check_branch_local_state(script)?);
    outcome.absorb_read_outcome(check_branch_read_view(script)?);
    outcome.absorb_immutable_outcome(check_branch_owned_immutable(script)?);
    outcome.absorb_inheritance_outcome(check_branch_inheritance(script)?);
    outcome.absorb_timestamp_outcome(check_branch_timestamp_visibility(script)?);
    outcome.absorb_materialization_outcome(check_branch_materialization(script)?);
    outcome.absorb_reachability_outcome(check_branch_reachability(script)?);
    outcome.absorb_compaction_outcome(check_branch_compaction(script)?);
    outcome.absorb_snapshot_install_outcome(check_branch_snapshot_install(script)?);

    check_branch_lsm_inheritance_model_contract(script)?;
    outcome.inheritance_model_cases += 1;
    check_branch_lsm_install_model_contract(script)?;
    outcome.install_model_cases += 1;

    Ok(outcome)
}

/// Runs the branch-read-specific fuzz contract over generated branch states.
pub fn check_branch_lsm_reads_contract(script: &[u8]) -> Result<(), TestkitError> {
    check_branch_lsm_reference_model_contract(script)?;
    check_row_chains_and_fork_edges(script)?;
    check_branch_read_view(script)?;
    check_branch_timestamp_visibility(script)?;
    check_branch_owned_immutable(script)?;
    Ok(())
}

/// Runs the branch-inheritance-specific fuzz contract over generated forks.
pub fn check_branch_lsm_inheritance_contract(script: &[u8]) -> Result<(), TestkitError> {
    check_branch_lsm_fault_window_contract(script)?;
    check_branch_lsm_inheritance_model_contract(script)?;
    check_branch_inheritance(script)?;
    check_branch_timestamp_visibility(script)?;
    check_branch_materialization(script)?;
    check_branch_reachability(script)?;
    Ok(())
}

/// Runs the branch-install-specific fuzz contract over generated install plans.
pub fn check_branch_lsm_install_contract(script: &[u8]) -> Result<(), TestkitError> {
    check_branch_lsm_fault_window_contract(script)?;
    check_branch_lsm_install_model_contract(script)?;
    check_branch_owned_immutable(script)?;
    check_branch_compaction(script)?;
    check_branch_snapshot_install(script)?;
    check_branch_reachability(script)?;
    Ok(())
}

/// Replays generated inheritance/materialization cases against an independent
/// branch/layer model.
fn check_branch_lsm_inheritance_model_contract(script: &[u8]) -> Result<(), TestkitError> {
    check_model_backed_inheritance_script(script)?;
    check_model_backed_inheritance(script)?;
    check_model_backed_materialization(script)?;
    Ok(())
}

/// Replays generated install cases against an independent row model.
fn check_branch_lsm_install_model_contract(script: &[u8]) -> Result<(), TestkitError> {
    check_model_backed_install_script(script)?;
    check_model_backed_snapshot_install(script)?;
    check_model_backed_compaction_install(script)?;
    Ok(())
}

/// Replays generated own-branch operations against production and an
/// independent row-list model.
pub fn check_branch_lsm_reference_model_contract(script: &[u8]) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 194));
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(7, 64, 64)
            .map_err(|err| TestkitError::new(format!("model config failed: {err}")))?,
    )
    .map_err(|err| TestkitError::new(format!("model state failed: {err}")))?;
    let mut model = ModelBranch::new(branch);
    let mut next_version = 1_u64;

    for step in 0..32 {
        let opcode = script_byte(script, 195 + step);
        match opcode % 5 {
            0 => {
                let row = model_put_row(branch, opcode, next_version, step)?;
                append_model_row(&mut state, &mut model, row)?;
                next_version = next_version.saturating_add(1);
            }
            1 => {
                let row = model_tombstone_row(branch, opcode, next_version, step)?;
                append_model_row(&mut state, &mut model, row)?;
                next_version = next_version.saturating_add(1);
            }
            2 => match state.rotate_active() {
                BranchRotationOutcome::Rotated { .. } | BranchRotationOutcome::Skipped { .. } => {}
            },
            3 => {
                let first = model_put_row(branch, opcode, next_version, step)?;
                let second = model_put_row(branch, opcode.wrapping_add(1), next_version + 1, step)?;
                install_model_l0_rows(
                    &mut state,
                    &mut model,
                    &format!("model-l0-{step}"),
                    vec![first, second],
                )?;
                next_version = next_version.saturating_add(2);
            }
            _ => {
                let row = model_expiring_row(branch, opcode, next_version, step)?;
                append_model_row(&mut state, &mut model, row)?;
                next_version = next_version.saturating_add(1);
            }
        }
        assert_model_matches_state(script, step, &model, &state)?;
    }
    Ok(())
}

/// Exercises branch state-transition fault windows that do not require an L1 backend.
pub fn check_branch_lsm_fault_window_contract(script: &[u8]) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 228));
    let other = branch_id(script_byte(script, 228).wrapping_add(1));

    let mut duplicate_state = BranchLocalState::empty(branch);
    let duplicate = storage_row_with(
        branch,
        b"fault-duplicate".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 229)],
    )?;
    duplicate_state
        .append_committed_row(duplicate.clone())
        .map_err(|err| TestkitError::new(format!("fault duplicate seed failed: {err}")))?;
    let before_duplicate = duplicate_state.clone();
    expect_duplicate_internal_key(duplicate_state.append_committed_row(duplicate))?;
    if duplicate_state != before_duplicate {
        return Err(TestkitError::new("duplicate append fault mutated state"));
    }

    let mut install_state = BranchLocalState::empty(branch);
    let wrong_table = branch_owned_table(
        other,
        BranchLevel::ZERO,
        "fault-wrong-branch-install",
        vec![storage_row_with(
            other,
            b"fault-wrong-branch".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            vec![script_byte(script, 230)],
        )?],
    )?;
    let before_install = install_state.clone();
    expect_invalid_branch_row(install_state.install_l0_table(wrong_table))?;
    if install_state != before_install {
        return Err(TestkitError::new(
            "wrong-branch install fault mutated state",
        ));
    }

    check_materialization_fault_window(script, branch, other)?;
    check_snapshot_fault_window(script, branch, other)?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct IdentityOutcome {
    matching_rows: usize,
    mismatching_rows: usize,
    physical_key_rewrites: usize,
    row_rewrites: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct BoundsOutcome {
    own_bounds: usize,
    inherited_bounds: usize,
    candidate_puts: usize,
    candidate_tombstones: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct EdgeOutcome {
    edge_rows: usize,
    encoded_grouping: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ChainOutcome {
    row_chains: usize,
    fork_edges: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct StateOutcome {
    state_construction: usize,
    committed_put_appends: usize,
    committed_tombstone_appends: usize,
    wrong_branch_append_rejections: usize,
    active_duplicate_rejections: usize,
    cross_layer_duplicate_appends: usize,
    same_key_version_appends: usize,
    same_version_key_appends: usize,
    active_rotations: usize,
    empty_rotation_skips: usize,
    frozen_limit_skips: usize,
    active_only_facts: usize,
    frozen_only_facts: usize,
    mixed_active_frozen_facts: usize,
    timestamp_edge_facts: usize,
    max_commit_edge_facts: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ReadOutcome {
    read_view_captures: usize,
    pinned_append_isolations: usize,
    pinned_rotation_isolations: usize,
    latest_point_reads: usize,
    version_bounded_point_reads: usize,
    tombstone_shadow_reads: usize,
    history_reads: usize,
    history_tombstones: usize,
    history_limits: usize,
    prefix_scans: usize,
    range_scans: usize,
    scan_tombstone_suppressions: usize,
    active_frozen_merge_reads: usize,
    wrong_branch_read_rejections: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ImmutableOutcome {
    immutable_descriptor_cases: usize,
    immutable_l0_installs: usize,
    immutable_l1_installs: usize,
    invalid_immutable_install_rejections: usize,
    immutable_l1_overlap_rejections: usize,
    frozen_replacements: usize,
    pinned_immutable_install_isolations: usize,
    immutable_latest_reads: usize,
    immutable_version_bounded_reads: usize,
    immutable_history_reads: usize,
    immutable_prefix_scans: usize,
    immutable_range_scans: usize,
    immutable_tombstone_shadows: usize,
    active_frozen_immutable_merge_reads: usize,
    immutable_source_attributions: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct InheritanceOutcome {
    inherited_fork_captures: usize,
    inherited_layer_validations: usize,
    inherited_latest_reads: usize,
    inherited_version_bounded_reads: usize,
    inherited_history_reads: usize,
    inherited_prefix_scans: usize,
    inherited_range_scans: usize,
    inherited_key_rewrites: usize,
    inherited_child_put_shadows: usize,
    inherited_child_tombstone_shadows: usize,
    inherited_post_fork_invisibility: usize,
    inherited_chained_ancestry: usize,
    invalid_inherited_layer_rejections: usize,
    pinned_inherited_view_isolations: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TimestampOutcome {
    timestamp_point_reads: usize,
    active_timestamp_point_reads: usize,
    frozen_timestamp_point_reads: usize,
    owned_timestamp_point_reads: usize,
    timestamp_scan_reads: usize,
    timestamp_prefix_scans: usize,
    timestamp_range_scans: usize,
    ttl_before_expiry_reads: usize,
    ttl_exact_expiry_suppressions: usize,
    ttl_after_expiry_suppressions: usize,
    ttl_max_expiry_reads: usize,
    timestamp_tombstone_shadows: usize,
    timestamp_tombstone_after_non_shadows: usize,
    timestamp_scan_boundary_reads: usize,
    timestamp_scan_space_isolations: usize,
    timestamp_empty_scans: usize,
    non_monotonic_timestamp_reads: usize,
    inherited_timestamp_reads: usize,
    inherited_timestamp_point_reads: usize,
    inherited_timestamp_scan_reads: usize,
    inherited_timestamp_fork_gates: usize,
    inherited_timestamp_child_put_shadows: usize,
    inherited_timestamp_child_tombstone_shadows: usize,
    inherited_timestamp_nearest_ties: usize,
    pinned_timestamp_view_isolations: usize,
    unknown_timestamp_coverage_reads: usize,
    insufficient_timestamp_history_rejections: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct MaterializationOutcome {
    materialization_attempts: usize,
    successful_materializations: usize,
    empty_materializations: usize,
    idempotent_materialization_retries: usize,
    materialized_rows: usize,
    materialized_tables: usize,
    skipped_materialization_post_fork_rows: usize,
    skipped_materialization_exact_duplicates: usize,
    materialization_latest_read_parity: usize,
    materialization_version_read_parity: usize,
    materialization_timestamp_read_parity: usize,
    materialization_history_read_parity: usize,
    materialization_prefix_scan_parity: usize,
    materialization_range_scan_parity: usize,
    materialization_pinned_view_isolations: usize,
    materialization_tombstone_preservations: usize,
    materialization_ttl_preservations: usize,
    invalid_materialization_rejections: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ReachabilityOutcome {
    reachability_snapshots: usize,
    reachability_owned_refs: usize,
    reachability_inherited_refs: usize,
    materializing_reachability_refs: usize,
    reachability_aggregate_rebuilds: usize,
    shared_table_detections: usize,
    reachability_release_candidates: usize,
    protected_release_attempts: usize,
    registry_rebuilds: usize,
    registry_unregisters: usize,
    registry_disagreements: usize,
    fork_reachability_cases: usize,
    failed_fork_reachability_rollbacks: usize,
    materialization_release_cases: usize,
    branch_clear_release_cases: usize,
    reachability_deterministic_orderings: usize,
    invalid_reachability_rejections: usize,
}

#[allow(clippy::struct_field_names)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CompactionOutcome {
    compaction_noop_cases: usize,
    l0_compaction_candidate_cases: usize,
    l0_to_l1_compaction_candidate_cases: usize,
    nonzero_level_compaction_candidate_cases: usize,
    keep_all_compaction_cases: usize,
    compaction_output_install_cases: usize,
    compaction_output_split_cases: usize,
    stale_candidate_rejection_cases: usize,
    unsafe_old_version_pruning_rejection_cases: usize,
    unsafe_tombstone_pruning_rejection_cases: usize,
    unsafe_ttl_pruning_rejection_cases: usize,
    compaction_latest_parity_cases: usize,
    compaction_version_parity_cases: usize,
    compaction_timestamp_parity_cases: usize,
    compaction_history_parity_cases: usize,
    compaction_prefix_scan_parity_cases: usize,
    compaction_range_scan_parity_cases: usize,
    compaction_pinned_view_isolation_cases: usize,
    compaction_release_candidate_cases: usize,
    compaction_protected_release_cases: usize,
    invalid_compaction_request_rejection_cases: usize,
}

#[allow(clippy::struct_field_names)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SnapshotInstallOutcome {
    snapshot_empty_install_noop_cases: usize,
    snapshot_single_branch_install_cases: usize,
    snapshot_multi_branch_install_cases: usize,
    snapshot_missing_branch_rejection_cases: usize,
    snapshot_missing_branch_create_cases: usize,
    snapshot_non_empty_target_rejection_cases: usize,
    snapshot_empty_group_rejection_cases: usize,
    snapshot_duplicate_branch_group_rejection_cases: usize,
    snapshot_duplicate_row_rejection_cases: usize,
    snapshot_unsorted_row_rejection_cases: usize,
    snapshot_branch_mismatch_rejection_cases: usize,
    snapshot_output_identity_collision_rejection_cases: usize,
    snapshot_table_build_failure_atomicity_cases: usize,
    snapshot_latest_parity_cases: usize,
    snapshot_version_parity_cases: usize,
    snapshot_timestamp_parity_cases: usize,
    snapshot_history_parity_cases: usize,
    snapshot_prefix_scan_parity_cases: usize,
    snapshot_range_scan_parity_cases: usize,
    snapshot_tombstone_preservation_cases: usize,
    snapshot_ttl_preservation_cases: usize,
    snapshot_pinned_view_isolation_cases: usize,
    snapshot_reachability_cases: usize,
    snapshot_source_boundary_guard_cases: usize,
}
