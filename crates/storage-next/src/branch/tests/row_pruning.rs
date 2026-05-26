use super::*;
use crate::table::TableCompactionDropReason;

mod required_plan;
mod tombstone_ttl;

#[test]
fn row_pruning_request_without_proof_rejects() {
    let branch = branch_id(0xa1);
    let mut state = BranchLocalState::empty(branch);
    install_l0_table(
        &mut state,
        branch,
        "prune-missing-left",
        vec![storage_row_with(
            branch,
            b"missing".to_vec(),
            5,
            50,
            Timestamp::EPOCH,
            b"new".to_vec(),
        )],
    )
    .expect("install left");
    install_l0_table(
        &mut state,
        branch,
        "prune-missing-right",
        vec![storage_row_with(
            branch,
            b"missing".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"old".to_vec(),
        )],
    )
    .expect("install right");
    let before = state.clone();

    let request =
        BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "missing-proof")
            .expect("request")
            .with_retention_policy(BranchCompactionRetentionPolicy::DropOlderVersions);

    let error = state
        .compact_branch_owned_tables(&request)
        .expect_err("missing proof rejects");
    assert_eq!(
        error.code(),
        BranchCompactionInvalidity::ProofMissing.code()
    );
    assert_eq!(state, before);
}

#[test]
fn version_pruning_keeps_retained_rows_and_floor_survivor() {
    let branch = branch_id(0xa2);
    let mut state = BranchLocalState::empty(branch);
    let key = physical_key(branch, b"version-prune".to_vec());
    install_l0_table(
        &mut state,
        branch,
        "version-prune-high",
        vec![
            storage_row_with(
                branch,
                b"version-prune".to_vec(),
                9,
                90,
                Timestamp::EPOCH,
                b"v9".to_vec(),
            ),
            storage_row_with(
                branch,
                b"version-prune".to_vec(),
                6,
                60,
                Timestamp::EPOCH,
                b"v6".to_vec(),
            ),
        ],
    )
    .expect("install high");
    install_l0_table(
        &mut state,
        branch,
        "version-prune-low",
        vec![
            storage_row_with(
                branch,
                b"version-prune".to_vec(),
                4,
                40,
                Timestamp::EPOCH,
                b"v4".to_vec(),
            ),
            storage_row_with(
                branch,
                b"version-prune".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"v2".to_vec(),
            ),
        ],
    )
    .expect("install low");
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = BranchCompactionPruningProof::from_branch_state(&state, CommitVersion::new(5))
        .expect("proof")
        .with_retained_timestamp_floor(Timestamp::from_micros(50))
        .expect("timestamp proof")
        .with_inherited_safety(BranchInheritancePruningProof::NoReadableInheritedLayers)
        .expect("inheritance proof")
        .with_shared_table_safety(BranchSharedTableSafety::NotShared)
        .expect("shared-table proof")
        .with_recovery_health(BranchRecoveryHealthAttestation::Healthy)
        .expect("recovery health proof");
    let request =
        BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "version-pruned")
            .expect("request")
            .with_retention_policy(BranchCompactionRetentionPolicy::DropOlderVersions)
            .with_pruning_proof(proof);

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("pruned compaction");
    let report = outcome.table_report().expect("report");
    assert_eq!(report.dropped_rows(), 1);
    assert!(report.drop_summaries().iter().any(|summary| {
        summary.reason() == TableCompactionDropReason::OlderVersion && summary.rows() == 1
    }));
    let after = state.capture_read_view().expect("after");
    assert_eq!(
        history_versions(
            &after
                .history(&key, BranchHistoryOptions::all())
                .expect("history")
        ),
        vec![9, 6, 4]
    );
}

#[test]
fn pruning_without_dropped_rows_does_not_narrow_timestamp_coverage() {
    let branch = branch_id(0xa6);
    let mut state = BranchLocalState::empty(branch);
    install_l0_table(
        &mut state,
        branch,
        "no-drop-left",
        vec![storage_row_with(
            branch,
            b"no-drop".to_vec(),
            9,
            90,
            Timestamp::EPOCH,
            b"v9".to_vec(),
        )],
    )
    .expect("install left");
    install_l0_table(
        &mut state,
        branch,
        "no-drop-right",
        vec![storage_row_with(
            branch,
            b"no-drop".to_vec(),
            6,
            60,
            Timestamp::EPOCH,
            b"v6".to_vec(),
        )],
    )
    .expect("install right");
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = BranchCompactionPruningProof::from_branch_state(&state, CommitVersion::new(5))
        .expect("proof")
        .with_retained_timestamp_floor(Timestamp::from_micros(50))
        .expect("timestamp proof")
        .with_inherited_safety(BranchInheritancePruningProof::NoReadableInheritedLayers)
        .expect("inheritance proof")
        .with_shared_table_safety(BranchSharedTableSafety::NotShared)
        .expect("shared-table proof")
        .with_recovery_health(BranchRecoveryHealthAttestation::Healthy)
        .expect("recovery health proof");
    let request =
        BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "no-drop-out")
            .expect("request")
            .with_retention_policy(BranchCompactionRetentionPolicy::DropOlderVersions)
            .with_pruning_proof(proof);

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("compaction");

    assert_eq!(outcome.table_report().expect("report").dropped_rows(), 0);
    assert_eq!(
        state.timestamp_coverage(),
        BranchTimestampCoverage::complete()
    );
}

#[test]
fn row_pruning_proof_recovery_health_unknown_rejects() {
    // A proof that omits the live-recovery-health attestation (or sets
    // it explicitly to Unknown) must be rejected — lifecycle-layer
    // wiring must observe `RecoveryHealth::Healthy` before attesting.
    let branch = branch_id(0xc8);
    let mut state = version_chain_state(branch, "recovery-unknown", &[4, 2]);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = BranchCompactionPruningProof::from_branch_state(&state, CommitVersion::new(3))
        .expect("proof")
        .with_retained_timestamp_floor(Timestamp::from_micros(30))
        .expect("timestamp proof")
        .with_inherited_safety(BranchInheritancePruningProof::NoReadableInheritedLayers)
        .expect("inheritance proof")
        .with_shared_table_safety(BranchSharedTableSafety::NotShared)
        .expect("shared-table proof");
    let before = state.clone();

    let error = state
        .compact_branch_owned_tables(&drop_older_request(branch, "recovery-unknown-out", proof))
        .expect_err("missing recovery health attestation rejects");

    assert_invalid_compaction_code(
        &error,
        BranchCompactionInvalidity::ProofUnsafeRecoveryHealth,
    );
    assert_eq!(state, before);
}

#[test]
fn row_pruning_proof_visible_version_below_branch_state_rejects() {
    // A caller that lies low about the visible version (claiming to have
    // observed v=3 when the branch actually has rows up to v=9) is
    // inconsistent with the fingerprinted state and must be rejected.
    let branch = branch_id(0xa3);
    let mut state = version_chain_state(branch, "visible-low", &[9, 6, 4, 2]);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = BranchCompactionPruningProof::new(
        branch,
        1,
        1,
        branch_pruning_fingerprint(&state),
        CommitVersion::new(3),
        CommitVersion::new(3),
    )
    .expect("proof")
    .with_retained_timestamp_floor(Timestamp::from_micros(30))
    .expect("timestamp proof")
    .with_inherited_safety(BranchInheritancePruningProof::NoReadableInheritedLayers)
    .expect("inheritance proof")
    .with_shared_table_safety(BranchSharedTableSafety::NotShared)
    .expect("shared-table proof")
    .with_recovery_health(BranchRecoveryHealthAttestation::Healthy)
    .expect("recovery health proof");
    let before = state.clone();

    let error = state
        .compact_branch_owned_tables(&drop_older_request(branch, "visible-low-out", proof))
        .expect_err("visible version below branch state rejects");

    assert_invalid_compaction_code(
        &error,
        BranchCompactionInvalidity::ProofVisibleVersionBelowState,
    );
    assert_eq!(state, before);
}

#[test]
fn row_pruning_proof_branch_mismatch_rejects() {
    let branch = branch_id(0xa7);
    let other = branch_id(0xa8);
    let mut state = two_version_state(branch, "branch-mismatch", 4, 2);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = BranchCompactionPruningProof::new(
        other,
        1,
        1,
        branch_pruning_fingerprint(&state),
        CommitVersion::new(3),
        CommitVersion::new(4),
    )
    .expect("proof")
    .with_retained_timestamp_floor(Timestamp::from_micros(30))
    .expect("timestamp proof")
    .with_inherited_safety(BranchInheritancePruningProof::NoReadableInheritedLayers)
    .expect("inheritance proof")
    .with_shared_table_safety(BranchSharedTableSafety::NotShared)
    .expect("shared-table proof")
    .with_recovery_health(BranchRecoveryHealthAttestation::Healthy)
    .expect("recovery health proof");
    let before = state.clone();

    let error = state
        .compact_branch_owned_tables(&drop_older_request(branch, "branch-mismatch-out", proof))
        .expect_err("branch mismatch rejects");

    assert_invalid_compaction_code(&error, BranchCompactionInvalidity::ProofBranchMismatch);
    assert_eq!(state, before);
}

#[test]
fn row_pruning_proof_degraded_recovery_rejects() {
    let branch = branch_id(0xa9);

    let error = BranchCompactionPruningProof::new(
        branch,
        1,
        0,
        1,
        CommitVersion::new(1),
        CommitVersion::new(1),
    )
    .expect_err("zero recovery-health epoch rejects");

    assert_invalid_compaction_code(
        &error,
        BranchCompactionInvalidity::ProofUnsafeRecoveryHealth,
    );
}

#[test]
fn row_pruning_proof_retained_floor_above_visible_rejects() {
    let branch = branch_id(0xaa);

    let error = BranchCompactionPruningProof::new(
        branch,
        1,
        1,
        1,
        CommitVersion::new(6),
        CommitVersion::new(5),
    )
    .expect_err("floor above visible rejects");

    assert_invalid_compaction_code(
        &error,
        BranchCompactionInvalidity::RetainedFloorAboveVisible,
    );
}

#[test]
fn row_pruning_proof_timestamp_floor_without_coverage_rejects() {
    let branch = branch_id(0xab);
    let mut state = two_version_state(branch, "timestamp-coverage", 4, 2);
    let proof = proof_for(&state, 3, 30);
    let before = state.clone();

    let error = state
        .compact_branch_owned_tables(&drop_older_request(branch, "timestamp-coverage-out", proof))
        .expect_err("unknown timestamp coverage rejects");

    assert_invalid_compaction(&error);
    assert_eq!(state, before);
}

#[test]
fn row_pruning_proof_active_view_below_floor_rejects() {
    let branch = branch_id(0xac);

    let error = BranchCompactionPruningProof::new(
        branch,
        1,
        1,
        1,
        CommitVersion::new(5),
        CommitVersion::new(9),
    )
    .expect("proof")
    .with_pinned_view_floor(CommitVersion::new(4))
    .expect_err("active view below floor rejects");

    assert_invalid_compaction(&error);
}

#[test]
fn row_pruning_proof_pinned_view_below_floor_rejects() {
    let branch = branch_id(0xad);

    let error = BranchCompactionPruningProof::new(
        branch,
        1,
        1,
        1,
        CommitVersion::new(5),
        CommitVersion::new(9),
    )
    .expect("proof")
    .with_pinned_view_floor(CommitVersion::new(1))
    .expect_err("pinned view below floor rejects");

    assert_invalid_compaction(&error);
}

#[test]
fn row_pruning_proof_inherited_layer_unknown_rejects() {
    let branch = branch_id(0xae);
    let mut state = two_version_state(branch, "unknown-inheritance", 4, 2);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = BranchCompactionPruningProof::from_branch_state(&state, CommitVersion::new(3))
        .expect("proof")
        .with_retained_timestamp_floor(Timestamp::from_micros(30))
        .expect("timestamp proof");
    let before = state.clone();

    let error = state
        .compact_branch_owned_tables(&drop_older_request(
            branch,
            "unknown-inheritance-out",
            proof,
        ))
        .expect_err("unknown inheritance rejects");

    assert_invalid_compaction(&error);
    assert_eq!(state, before);
}

#[test]
fn row_pruning_proof_zero_floor_keeps_all() {
    let branch = branch_id(0xaf);
    let mut state = two_version_state(branch, "zero-floor", 4, 2);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 0, 0);

    let outcome = state
        .compact_branch_owned_tables(&drop_older_request(branch, "zero-floor-out", proof))
        .expect("compaction");

    assert_eq!(outcome.table_report().expect("report").dropped_rows(), 0);
    assert_eq!(
        state.timestamp_coverage(),
        BranchTimestampCoverage::complete()
    );
    assert_eq!(
        history_for(&state, branch, b"key"),
        vec![4, 2],
        "zero retained floor must keep all row history"
    );
}

#[test]
fn row_pruning_proof_is_deterministic_for_shuffled_facts() {
    let branch = branch_id(0xb0);
    let mut left = BranchLocalState::empty(branch);
    let mut right = BranchLocalState::empty(branch);
    install_l0_table(
        &mut left,
        branch,
        "shuffle-left",
        vec![
            storage_row_with(
                branch,
                b"a".to_vec(),
                3,
                30,
                Timestamp::EPOCH,
                b"a3".to_vec(),
            ),
            storage_row_with(
                branch,
                b"a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"a1".to_vec(),
            ),
        ],
    )
    .expect("install left");
    install_l0_table(
        &mut right,
        branch,
        "shuffle-left",
        vec![
            storage_row_with(
                branch,
                b"a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"a1".to_vec(),
            ),
            storage_row_with(
                branch,
                b"a".to_vec(),
                3,
                30,
                Timestamp::EPOCH,
                b"a3".to_vec(),
            ),
        ],
    )
    .expect("install right");

    assert_eq!(
        branch_pruning_fingerprint(&left),
        branch_pruning_fingerprint(&right)
    );
}

#[test]
fn row_pruning_proof_stale_epoch_rejects_without_mutation() {
    let branch = branch_id(0xa3);
    let mut state = BranchLocalState::empty(branch);
    install_l0_table(
        &mut state,
        branch,
        "stale-proof-left",
        vec![storage_row_with(
            branch,
            b"stale".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            b"v4".to_vec(),
        )],
    )
    .expect("install left");
    install_l0_table(
        &mut state,
        branch,
        "stale-proof-right",
        vec![storage_row_with(
            branch,
            b"stale".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"v2".to_vec(),
        )],
    )
    .expect("install right");
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = BranchCompactionPruningProof::from_branch_state(&state, CommitVersion::new(3))
        .expect("proof")
        .with_retained_timestamp_floor(Timestamp::from_micros(30))
        .expect("timestamp proof")
        .with_inherited_safety(BranchInheritancePruningProof::NoReadableInheritedLayers)
        .expect("inheritance proof")
        .with_shared_table_safety(BranchSharedTableSafety::NotShared)
        .expect("shared-table proof")
        .with_recovery_health(BranchRecoveryHealthAttestation::Healthy)
        .expect("recovery health proof");
    install_l0_table(
        &mut state,
        branch,
        "stale-proof-mutation",
        vec![storage_row_with(
            branch,
            b"other".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"other".to_vec(),
        )],
    )
    .expect("mutate state");
    let before = state.clone();
    let request =
        BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "stale-proof-out")
            .expect("request")
            .with_retention_policy(BranchCompactionRetentionPolicy::DropOlderVersions)
            .with_pruning_proof(proof);

    let error = state
        .compact_branch_owned_tables(&request)
        .expect_err("stale proof rejects");
    assert!(matches!(
        error,
        BranchRuntimeError::InvalidCompaction { .. }
    ));
    assert_eq!(state, before);
}

#[test]
fn version_pruning_preserves_getv_within_floor() {
    let branch = branch_id(0xb1);
    let mut state = version_chain_state(branch, "getv", &[9, 6, 4, 2]);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 5, 50);

    state
        .compact_branch_owned_tables(&drop_older_request(branch, "getv-out", proof))
        .expect("compaction");
    let view = state.capture_read_view().expect("view");
    let key = physical_key(branch, b"key".to_vec());

    assert_eq!(
        view.at_version(&key, CommitVersion::new(6))
            .expect("getv retained")
            .expect("retained")
            .row()
            .value(),
        b"v6"
    );
    assert_eq!(
        view.at_version(&key, CommitVersion::new(4))
            .expect("getv survivor")
            .expect("survivor")
            .row()
            .value(),
        b"v4"
    );
}

#[test]
fn version_pruning_as_of_below_floor_returns_insufficient_history() {
    let branch = branch_id(0xb2);
    let mut state = version_chain_state(branch, "as-of-boundary", &[9, 6, 4, 2]);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 5, 50);

    state
        .compact_branch_owned_tables(&drop_older_request(branch, "as-of-boundary-out", proof))
        .expect("compaction");
    let error = state
        .capture_read_view()
        .expect("view")
        .read_point(
            &physical_key(branch, b"key".to_vec()),
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .expect_err("below timestamp floor rejects");

    assert!(matches!(
        error,
        BranchRuntimeError::InsufficientTimestampHistory { .. }
    ));
}

#[test]
fn version_pruning_non_monotone_timestamps_respects_timestamp_floor() {
    let branch = branch_id(0xb3);
    let mut state = BranchLocalState::empty(branch);
    install_l0_table(
        &mut state,
        branch,
        "non-monotone-left",
        vec![
            storage_row_with(
                branch,
                b"key".to_vec(),
                9,
                30,
                Timestamp::EPOCH,
                b"v9".to_vec(),
            ),
            storage_row_with(
                branch,
                b"key".to_vec(),
                4,
                80,
                Timestamp::EPOCH,
                b"v4".to_vec(),
            ),
        ],
    )
    .expect("install left");
    install_l0_table(
        &mut state,
        branch,
        "non-monotone-right",
        vec![
            storage_row_with(
                branch,
                b"key".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"v2".to_vec(),
            ),
            storage_row_with(
                branch,
                b"key".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"v1".to_vec(),
            ),
        ],
    )
    .expect("install right");
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 5, 50);

    let outcome = state
        .compact_branch_owned_tables(&drop_older_request(branch, "non-monotone-out", proof))
        .expect("compaction");

    assert_eq!(outcome.table_report().expect("report").dropped_rows(), 1);
    assert_eq!(history_for(&state, branch, b"key"), vec![9, 4, 2]);
}

#[test]
fn max_versions_keeps_newest_n_versions() {
    let branch = branch_id(0xb4);
    let mut state = version_chain_state(branch, "max-versions", &[8, 7, 6, 5]);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 9, 90)
        .with_max_versions_per_key(2)
        .expect("max versions proof");

    let outcome = state
        .compact_branch_owned_tables(&drop_older_request(branch, "max-versions-out", proof))
        .expect("compaction");

    assert_eq!(outcome.table_report().expect("report").dropped_rows(), 2);
    assert_eq!(history_for(&state, branch, b"key"), vec![8, 7]);
}

#[test]
fn max_versions_zero_means_unbounded_when_floor_keeps_all() {
    let branch = branch_id(0xb5);
    let mut state = version_chain_state(branch, "max-zero", &[8, 7, 6, 5]);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 0, 0)
        .with_max_versions_per_key(0)
        .expect("max zero proof");

    let outcome = state
        .compact_branch_owned_tables(&drop_older_request(branch, "max-zero-out", proof))
        .expect("compaction");

    assert_eq!(outcome.table_report().expect("report").dropped_rows(), 0);
    assert_eq!(history_for(&state, branch, b"key"), vec![8, 7, 6, 5]);
}

#[test]
fn max_versions_zero_with_floor_above_data_keeps_only_below_floor_survivor() {
    // Documents that `max_versions = Some(0)` removes only the explicit
    // version cap. The below-floor survivor rule is part of
    // `DropOlderVersions` itself, not the max-versions cap, so it still
    // applies. The earlier `_when_floor_keeps_all` case avoided the rule by
    // setting `floor=0`; this case exercises a real floor.
    let branch = branch_id(0xe8);
    let mut state = version_chain_state(branch, "max-zero-floor", &[8, 7, 6, 5]);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 9, 90)
        .with_max_versions_per_key(0)
        .expect("max zero proof");

    let outcome = state
        .compact_branch_owned_tables(&drop_older_request(branch, "max-zero-floor-out", proof))
        .expect("compaction");

    assert_eq!(outcome.table_report().expect("report").dropped_rows(), 3);
    assert_eq!(history_for(&state, branch, b"key"), vec![8]);
}

#[test]
fn max_versions_counts_values_but_not_required_tombstones() {
    let branch = branch_id(0xb6);
    let mut state = BranchLocalState::empty(branch);
    install_l0_table(
        &mut state,
        branch,
        "max-tombstone-left",
        vec![
            storage_row_with(
                branch,
                b"key".to_vec(),
                8,
                80,
                Timestamp::EPOCH,
                b"v8".to_vec(),
            ),
            tombstone_row(branch, b"key".to_vec(), 7, 70),
        ],
    )
    .expect("install left");
    install_l0_table(
        &mut state,
        branch,
        "max-tombstone-right",
        vec![
            storage_row_with(
                branch,
                b"key".to_vec(),
                6,
                60,
                Timestamp::EPOCH,
                b"v6".to_vec(),
            ),
            storage_row_with(
                branch,
                b"key".to_vec(),
                5,
                50,
                Timestamp::EPOCH,
                b"v5".to_vec(),
            ),
        ],
    )
    .expect("install right");
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 9, 90)
        .with_max_versions_per_key(1)
        .expect("max one proof");

    let outcome = state
        .compact_branch_owned_tables(&drop_older_request(branch, "max-tombstone-out", proof))
        .expect("compaction");

    assert_eq!(outcome.table_report().expect("report").dropped_rows(), 2);
    assert_eq!(history_for(&state, branch, b"key"), vec![8, 7]);
}

fn install_l0_table(
    state: &mut BranchLocalState,
    branch: BranchId,
    identity: &str,
    rows: Vec<StorageRow>,
) -> BranchRuntimeResult<BranchImmutableInstallOutcome> {
    state.install_l0_table(branch_owned_table(
        branch,
        BranchLevel::ZERO,
        identity,
        rows,
    ))
}

fn assert_invalid_compaction(error: &BranchRuntimeError) {
    assert!(matches!(
        error,
        BranchRuntimeError::InvalidCompaction { .. }
    ));
}

fn assert_invalid_compaction_code(
    error: &BranchRuntimeError,
    expected: BranchCompactionInvalidity,
) {
    match error {
        BranchRuntimeError::InvalidCompaction { reason } => {
            assert_eq!(
                reason.code(),
                expected.code(),
                "expected compaction invalidity code {} but got {}",
                expected.code(),
                reason.code(),
            );
        }
        other => panic!("expected InvalidCompaction, got {other:?}"),
    }
}

fn proof_for(
    state: &BranchLocalState,
    retained_floor: u64,
    retained_timestamp_floor: u64,
) -> BranchCompactionPruningProof {
    BranchCompactionPruningProof::from_branch_state(state, CommitVersion::new(retained_floor))
        .expect("proof")
        .with_retained_timestamp_floor(Timestamp::from_micros(retained_timestamp_floor))
        .expect("timestamp proof")
        .with_inherited_safety(BranchInheritancePruningProof::NoReadableInheritedLayers)
        .expect("inheritance proof")
        .with_shared_table_safety(BranchSharedTableSafety::NotShared)
        .expect("shared-table proof")
        .with_recovery_health(BranchRecoveryHealthAttestation::Healthy)
        .expect("recovery health proof")
}

fn drop_older_request(
    branch: BranchId,
    seed: &str,
    proof: BranchCompactionPruningProof,
) -> BranchCompactionRequest {
    BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, seed)
        .expect("request")
        .with_retention_policy(BranchCompactionRetentionPolicy::DropOlderVersions)
        .with_pruning_proof(proof)
}

fn tombstone_request(
    branch: BranchId,
    seed: &str,
    proof: BranchCompactionPruningProof,
) -> BranchCompactionRequest {
    BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, seed)
        .expect("request")
        .with_retention_policy(BranchCompactionRetentionPolicy::DropTombstones)
        .with_pruning_proof(proof)
}

fn ttl_request(
    branch: BranchId,
    seed: &str,
    proof: BranchCompactionPruningProof,
) -> BranchCompactionRequest {
    BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, seed)
        .expect("request")
        .with_retention_policy(BranchCompactionRetentionPolicy::DropExpired)
        .with_pruning_proof(proof)
}

fn two_version_state(branch: BranchId, seed: &str, newest: u64, oldest: u64) -> BranchLocalState {
    let mut state = BranchLocalState::empty(branch);
    install_l0_table(
        &mut state,
        branch,
        &format!("{seed}-left"),
        vec![storage_row_with(
            branch,
            b"key".to_vec(),
            newest,
            newest.saturating_mul(10),
            Timestamp::EPOCH,
            format!("v{newest}").into_bytes(),
        )],
    )
    .expect("install newest");
    install_l0_table(
        &mut state,
        branch,
        &format!("{seed}-right"),
        vec![storage_row_with(
            branch,
            b"key".to_vec(),
            oldest,
            oldest.saturating_mul(10),
            Timestamp::EPOCH,
            format!("v{oldest}").into_bytes(),
        )],
    )
    .expect("install oldest");
    state
}

fn version_chain_state(branch: BranchId, seed: &str, versions: &[u64]) -> BranchLocalState {
    let mut state = BranchLocalState::empty(branch);
    let midpoint = versions.len().saturating_add(1) / 2;
    let left = versions[..midpoint]
        .iter()
        .map(|version| {
            storage_row_with(
                branch,
                b"key".to_vec(),
                *version,
                version.saturating_mul(10),
                Timestamp::EPOCH,
                format!("v{version}").into_bytes(),
            )
        })
        .collect();
    let right = versions[midpoint..]
        .iter()
        .map(|version| {
            storage_row_with(
                branch,
                b"key".to_vec(),
                *version,
                version.saturating_mul(10),
                Timestamp::EPOCH,
                format!("v{version}").into_bytes(),
            )
        })
        .collect();
    install_l0_table(&mut state, branch, &format!("{seed}-left"), left).expect("install left");
    install_l0_table(&mut state, branch, &format!("{seed}-right"), right).expect("install right");
    state
}

fn tombstone_only_state(
    branch: BranchId,
    seed: &str,
    config: BranchRuntimeConfig,
) -> BranchLocalState {
    let mut state = BranchLocalState::new(branch, config).expect("state");
    install_l0_table(
        &mut state,
        branch,
        &format!("{seed}-left"),
        vec![tombstone_row(branch, b"deleted".to_vec(), 6, 60)],
    )
    .expect("install tombstone");
    install_l0_table(
        &mut state,
        branch,
        &format!("{seed}-right"),
        vec![storage_row_with(
            branch,
            b"other".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"other".to_vec(),
        )],
    )
    .expect("install other");
    state
}

fn ttl_state(
    branch: BranchId,
    seed: &str,
    version: u64,
    timestamp: u64,
    expires_at: u64,
) -> BranchLocalState {
    let mut state =
        BranchLocalState::new(branch, BranchRuntimeConfig::new(1, 8, 8).expect("config"))
            .expect("state");
    install_l0_table(
        &mut state,
        branch,
        &format!("{seed}-left"),
        vec![storage_row_with(
            branch,
            b"ttl".to_vec(),
            version,
            timestamp,
            Timestamp::from_micros(expires_at),
            format!("v{version}").into_bytes(),
        )],
    )
    .expect("install ttl");
    install_l0_table(
        &mut state,
        branch,
        &format!("{seed}-right"),
        vec![storage_row_with(
            branch,
            b"ttl".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"v1".to_vec(),
        )],
    )
    .expect("install survivor");
    state
}

fn history_for(state: &BranchLocalState, branch: BranchId, key: &[u8]) -> Vec<u64> {
    let view = state.capture_read_view().expect("view");
    history_versions(
        &view
            .history(
                &physical_key(branch, key.to_vec()),
                BranchHistoryOptions::all(),
            )
            .expect("history"),
    )
}
