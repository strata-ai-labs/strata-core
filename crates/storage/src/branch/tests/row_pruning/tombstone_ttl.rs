use super::*;

#[test]
pub(super) fn tombstone_pruning_rejects_resurrection_risk() {
    let branch = branch_id(0xa4);
    let mut state =
        BranchLocalState::new(branch, BranchRuntimeConfig::new(1, 8, 8).expect("config"))
            .expect("state");
    install_l0_table(
        &mut state,
        branch,
        "tombstone-risk-left",
        vec![StorageRow::tombstone(
            physical_key(branch, b"deleted".to_vec()),
            CommitVersion::new(6),
            Timestamp::from_micros(60),
        )],
    )
    .expect("install delete");
    install_l0_table(
        &mut state,
        branch,
        "tombstone-risk-right",
        vec![storage_row_with(
            branch,
            b"deleted".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"old".to_vec(),
        )],
    )
    .expect("install old");
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = BranchCompactionPruningProof::from_branch_state(&state, CommitVersion::new(7))
        .expect("proof")
        .with_retained_timestamp_floor(Timestamp::from_micros(70))
        .expect("timestamp proof")
        .with_no_readable_inherited_layers()
        .expect("inheritance proof")
        .with_candidate_tables_not_shared()
        .expect("shared-table proof")
        .with_recovery_health(BranchRecoveryHealthAttestation::Healthy)
        .expect("recovery health proof")
        .with_tombstone_elision()
        .expect("tombstone proof");
    let request =
        BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "tombstone-risk")
            .expect("request")
            .with_retention_policy(BranchCompactionRetentionPolicy::DropTombstones)
            .with_pruning_proof(proof);

    let error = state
        .compact_branch_owned_tables(&request)
        .expect_err("resurrection risk rejects");
    assert!(matches!(
        error,
        BranchRuntimeError::InvalidCompaction { .. }
    ));
}

#[test]
pub(super) fn expired_row_pruning_uses_supplied_cutoff() {
    let branch = branch_id(0xa5);
    let mut state =
        BranchLocalState::new(branch, BranchRuntimeConfig::new(1, 8, 8).expect("config"))
            .expect("state");
    let expired_key = physical_key(branch, b"expired".to_vec());
    let live_key = physical_key(branch, b"live".to_vec());
    install_l0_table(
        &mut state,
        branch,
        "ttl-left",
        vec![
            storage_row_with(
                branch,
                b"expired".to_vec(),
                6,
                45,
                Timestamp::from_micros(45),
                b"expired-new".to_vec(),
            ),
            storage_row_with(
                branch,
                b"live".to_vec(),
                3,
                30,
                Timestamp::from_micros(90),
                b"live".to_vec(),
            ),
        ],
    )
    .expect("install left");
    install_l0_table(
        &mut state,
        branch,
        "ttl-right",
        vec![storage_row_with(
            branch,
            b"expired".to_vec(),
            4,
            40,
            Timestamp::from_micros(45),
            b"expired-old".to_vec(),
        )],
    )
    .expect("install right");
    let before = state.capture_read_view().expect("before");
    let before_latest = before
        .latest(&expired_key)
        .expect("expired latest before")
        .expect("expired before")
        .row()
        .clone();
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = BranchCompactionPruningProof::from_branch_state(&state, CommitVersion::new(7))
        .expect("proof")
        .with_retained_timestamp_floor(Timestamp::from_micros(50))
        .expect("timestamp proof")
        .with_no_readable_inherited_layers()
        .expect("inheritance proof")
        .with_candidate_tables_not_shared()
        .expect("shared-table proof")
        .with_recovery_health(BranchRecoveryHealthAttestation::Healthy)
        .expect("recovery health proof")
        .with_ttl_elision(Timestamp::from_micros(50))
        .expect("ttl proof");
    let request = BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "ttl-out")
        .expect("request")
        .with_retention_policy(BranchCompactionRetentionPolicy::DropExpired)
        .with_pruning_proof(proof);

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("ttl compaction");
    let report = outcome.table_report().expect("report");
    assert_eq!(report.dropped_rows(), 1);
    assert!(report.drop_summaries().iter().any(|summary| {
        summary.reason() == TableCompactionDropReason::Expired && summary.rows() == 1
    }));
    let after = state.capture_read_view().expect("after");
    assert_eq!(
        after
            .latest(&expired_key)
            .expect("expired latest")
            .expect("expired survivor")
            .row(),
        &before_latest
    );
    assert_eq!(
        after
            .latest(&live_key)
            .expect("live latest")
            .expect("live")
            .row()
            .value(),
        b"live"
    );
}

#[test]
fn tombstone_pruning_rejects_without_elision_proof() {
    let branch = branch_id(0xb7);
    let mut state = tombstone_only_state(
        branch,
        "no-elision",
        BranchRuntimeConfig::new(1, 8, 8).expect("config"),
    );
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 7, 70);

    let error = state
        .compact_branch_owned_tables(&tombstone_request(branch, "no-elision-out", proof))
        .expect_err("tombstone proof required");

    assert_invalid_compaction(&error);
}

#[test]
fn row_elision_gates_remain_bound_to_parent_proof_safety() {
    let tombstone_branch = branch_id(0xc1);
    let mut tombstone_state = tombstone_only_state(
        tombstone_branch,
        "merged-tombstone-proof",
        BranchRuntimeConfig::new(1, 8, 8).expect("config"),
    );
    tombstone_state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let tombstone_proof =
        BranchCompactionPruningProof::from_branch_state(&tombstone_state, CommitVersion::new(7))
            .expect("proof")
            .with_retained_timestamp_floor(Timestamp::from_micros(70))
            .expect("timestamp proof")
            .with_candidate_tables_not_shared()
            .expect("shared-table proof")
            .with_recovery_health(BranchRecoveryHealthAttestation::Healthy)
            .expect("recovery health proof")
            .with_tombstone_elision()
            .expect("tombstone gate");
    let before_tombstone = tombstone_state.clone();

    let tombstone_error = tombstone_state
        .compact_branch_owned_tables(&tombstone_request(
            tombstone_branch,
            "merged-tombstone-proof-out",
            tombstone_proof,
        ))
        .expect_err("tombstone gate cannot bypass inherited-layer safety");

    assert_invalid_compaction_code(
        &tombstone_error,
        BranchCompactionInvalidity::InheritedLayerUnknown,
    );
    assert_eq!(tombstone_state, before_tombstone);

    let ttl_branch = branch_id(0xc2);
    let mut ttl_state = ttl_state(ttl_branch, "merged-ttl-proof", 4, 40, 45);
    ttl_state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let ttl_proof =
        BranchCompactionPruningProof::from_branch_state(&ttl_state, CommitVersion::new(5))
            .expect("proof")
            .with_retained_timestamp_floor(Timestamp::from_micros(50))
            .expect("timestamp proof")
            .with_no_readable_inherited_layers()
            .expect("inheritance proof")
            .with_recovery_health(BranchRecoveryHealthAttestation::Healthy)
            .expect("recovery health proof")
            .with_ttl_elision(Timestamp::from_micros(50))
            .expect("ttl gate");
    let before_ttl = ttl_state.clone();

    let ttl_error = ttl_state
        .compact_branch_owned_tables(&ttl_request(ttl_branch, "merged-ttl-proof-out", ttl_proof))
        .expect_err("ttl gate cannot bypass shared-table safety");

    assert_invalid_compaction_code(
        &ttl_error,
        BranchCompactionInvalidity::SharedTableSafetyUnknown,
    );
    assert_eq!(ttl_state, before_ttl);
}

#[test]
pub(super) fn bottommost_tombstone_below_floor_can_be_elided() {
    let branch = branch_id(0xb8);
    let mut state = tombstone_only_state(
        branch,
        "bottommost-tombstone",
        BranchRuntimeConfig::new(1, 8, 8).expect("config"),
    );
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 7, 70)
        .with_tombstone_elision()
        .expect("tombstone proof");

    let outcome = state
        .compact_branch_owned_tables(&tombstone_request(
            branch,
            "bottommost-tombstone-out",
            proof,
        ))
        .expect("compaction");

    assert_eq!(outcome.table_report().expect("report").dropped_rows(), 1);
    assert_eq!(history_for(&state, branch, b"deleted"), Vec::<u64>::new());
}

#[test]
fn non_bottommost_tombstone_below_floor_is_kept() {
    let branch = branch_id(0xb9);
    let mut state = tombstone_only_state(branch, "non-bottommost", BranchRuntimeConfig::default());
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 7, 70)
        .with_tombstone_elision()
        .expect("tombstone proof");

    let error = state
        .compact_branch_owned_tables(&tombstone_request(branch, "non-bottommost-out", proof))
        .expect_err("non-bottommost rejects");

    assert_invalid_compaction(&error);
    assert_eq!(history_for(&state, branch, b"deleted"), vec![6]);
}

#[test]
fn tombstone_above_floor_is_kept() {
    let branch = branch_id(0xba);
    let mut state = tombstone_only_state(
        branch,
        "above-floor-tombstone",
        BranchRuntimeConfig::new(1, 8, 8).expect("config"),
    );
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 5, 50)
        .with_tombstone_elision()
        .expect("tombstone proof");

    let outcome = state
        .compact_branch_owned_tables(&tombstone_request(
            branch,
            "above-floor-tombstone-out",
            proof,
        ))
        .expect("compaction");

    assert_eq!(outcome.table_report().expect("report").dropped_rows(), 0);
    assert_eq!(history_for(&state, branch, b"deleted"), vec![6]);
}

#[test]
pub(super) fn tombstone_needed_to_shadow_inherited_value_is_kept() {
    let parent = branch_id(0xbb);
    let child = branch_id(0xbc);
    let mut parent_state = BranchLocalState::empty(parent);
    install_l0_table(
        &mut parent_state,
        parent,
        "inherited-shadow-parent",
        vec![storage_row_with(
            parent,
            b"deleted".to_vec(),
            5,
            50,
            Timestamp::EPOCH,
            b"parent".to_vec(),
        )],
    )
    .expect("install parent");
    let (mut child_state, _) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    install_l0_table(
        &mut child_state,
        child,
        "inherited-shadow-child-left",
        vec![tombstone_row(child, b"deleted".to_vec(), 6, 60)],
    )
    .expect("install child left");
    install_l0_table(
        &mut child_state,
        child,
        "inherited-shadow-child-right",
        vec![storage_row_with(
            child,
            b"other".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            b"other".to_vec(),
        )],
    )
    .expect("install child right");
    child_state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof =
        BranchCompactionPruningProof::from_branch_state(&child_state, CommitVersion::new(7))
            .expect("proof")
            .with_retained_timestamp_floor(Timestamp::from_micros(70))
            .expect("timestamp proof")
            .with_no_readable_inherited_layers()
            .expect("inheritance proof")
            .with_candidate_tables_not_shared()
            .expect("shared-table proof")
            .with_recovery_health(BranchRecoveryHealthAttestation::Healthy)
            .expect("recovery health proof")
            .with_tombstone_elision()
            .expect("tombstone proof");

    let error = child_state
        .compact_branch_owned_tables(&tombstone_request(child, "inherited-shadow-out", proof))
        .expect_err("inherited layer blocks pruning");

    assert_invalid_compaction(&error);
}

#[test]
fn ttl_pruning_rejects_without_ttl_proof() {
    let branch = branch_id(0xbd);
    let mut state = ttl_state(branch, "ttl-missing-proof", 4, 40, 45);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 5, 50);

    let error = state
        .compact_branch_owned_tables(&ttl_request(branch, "ttl-missing-proof-out", proof))
        .expect_err("ttl proof required");

    assert_invalid_compaction(&error);
}

#[test]
fn expired_ttl_above_version_floor_is_kept() {
    let branch = branch_id(0xbe);
    let mut state = ttl_state(branch, "ttl-above-version", 6, 40, 45);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 5, 50)
        .with_ttl_elision(Timestamp::from_micros(50))
        .expect("ttl proof");

    let outcome = state
        .compact_branch_owned_tables(&ttl_request(branch, "ttl-above-version-out", proof))
        .expect("compaction");

    assert_eq!(outcome.table_report().expect("report").dropped_rows(), 0);
    assert_eq!(history_for(&state, branch, b"ttl"), vec![6, 1]);
}

#[test]
fn expired_ttl_needed_by_as_of_timestamp_rejects_cutoff() {
    let branch = branch_id(0xbf);
    let mut state = ttl_state(branch, "ttl-as-of", 4, 40, 70);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());

    let error = BranchCompactionPruningProof::from_branch_state(&state, CommitVersion::new(5))
        .expect("proof")
        .with_retained_timestamp_floor(Timestamp::from_micros(50))
        .expect("timestamp proof")
        .with_no_readable_inherited_layers()
        .expect("inheritance proof")
        .with_candidate_tables_not_shared()
        .expect("shared-table proof")
        .with_recovery_health(BranchRecoveryHealthAttestation::Healthy)
        .expect("recovery health proof")
        .with_ttl_elision(Timestamp::from_micros(80))
        .expect_err("ttl cutoff beyond retained timestamp floor rejects");

    assert_invalid_compaction(&error);
}

#[test]
fn non_expired_ttl_row_is_kept() {
    let branch = branch_id(0xc0);
    let mut state = ttl_state(branch, "ttl-live", 4, 40, 90);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 5, 50)
        .with_ttl_elision(Timestamp::from_micros(50))
        .expect("ttl proof");

    let outcome = state
        .compact_branch_owned_tables(&ttl_request(branch, "ttl-live-out", proof))
        .expect("compaction");

    assert_eq!(outcome.table_report().expect("report").dropped_rows(), 0);
    assert_eq!(history_for(&state, branch, b"ttl"), vec![4, 1]);
}

#[test]
pub(super) fn ttl_pruning_across_inherited_parent_child_keeps_required_parent_row() {
    let parent = branch_id(0xc1);
    let child = branch_id(0xc2);
    let mut parent_state = BranchLocalState::empty(parent);
    install_l0_table(
        &mut parent_state,
        parent,
        "ttl-parent",
        vec![storage_row_with(
            parent,
            b"ttl".to_vec(),
            4,
            40,
            Timestamp::from_micros(45),
            b"parent".to_vec(),
        )],
    )
    .expect("install parent");
    let (mut child_state, _) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    install_l0_table(
        &mut child_state,
        child,
        "ttl-child-left",
        vec![storage_row_with(
            child,
            b"other".to_vec(),
            3,
            30,
            Timestamp::EPOCH,
            b"other".to_vec(),
        )],
    )
    .expect("install child left");
    install_l0_table(
        &mut child_state,
        child,
        "ttl-child-right",
        vec![storage_row_with(
            child,
            b"another".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"another".to_vec(),
        )],
    )
    .expect("install child right");
    child_state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&child_state, 5, 50)
        .with_ttl_elision(Timestamp::from_micros(50))
        .expect("ttl proof");

    let error = child_state
        .compact_branch_owned_tables(&ttl_request(child, "ttl-child-out", proof))
        .expect_err("inherited layer blocks ttl pruning");

    assert_invalid_compaction(&error);
}
