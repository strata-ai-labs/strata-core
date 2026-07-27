use super::*;

#[test]
#[allow(clippy::too_many_lines)]
fn branch_snapshot_install_builds_l0_tables_and_preserves_reads() {
    let branch_a = branch_id(201);
    let branch_b = branch_id(202);
    let untouched_branch = branch_id(210);
    let key_a = physical_key(branch_a, b"snapshot-a".to_vec());
    let key_b = physical_key(branch_b, b"snapshot-b".to_vec());
    let mut rows_a = vec![
        storage_row_with(
            branch_a,
            b"snapshot-a".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"old".to_vec(),
        ),
        storage_row_with(
            branch_a,
            b"snapshot-a".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"new".to_vec(),
        ),
    ];
    sort_storage_rows_by_internal_key(&mut rows_a);
    let mut rows_b = vec![storage_row_with(
        branch_b,
        b"snapshot-b".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"created".to_vec(),
    )];
    sort_storage_rows_by_internal_key(&mut rows_b);

    let request = BranchSnapshotInstallRequest::new(
        "snapshot-seed",
        vec![
            BranchSnapshotInstallGroup::new(branch_a, rows_a),
            BranchSnapshotInstallGroup::new(branch_b, rows_b),
        ],
    )
    .expect("snapshot request")
    .with_missing_branch_policy(BranchSnapshotMissingBranchPolicy::Create {
        config: BranchRuntimeConfig::default(),
    })
    .with_table_builder_config(TableBuilderConfig::default())
    .with_max_rows_per_table(1)
    .expect("max rows per table");
    assert_eq!(
        request.missing_branch_policy(),
        BranchSnapshotMissingBranchPolicy::Create {
            config: BranchRuntimeConfig::default(),
        }
    );
    assert_eq!(
        request.table_builder_config(),
        TableBuilderConfig::default()
    );
    assert_eq!(request.max_rows_per_table(), 1);

    let mut branches = vec![
        BranchLocalState::empty(untouched_branch),
        BranchLocalState::empty(branch_a),
    ];
    let outcome: BranchSnapshotInstallOutcome =
        install_snapshot_rows_into_branches(&mut branches, &request).expect("snapshot install");

    assert!(!outcome.is_empty_plan_noop());
    assert_eq!(outcome.rows_installed(), 3);
    assert_eq!(outcome.tables_created(), 3);
    assert_eq!(outcome.branches_created(), 1);
    assert_eq!(outcome.branches_replaced(), 1);
    assert_eq!(outcome.table_identities().len(), 3);
    assert_eq!(outcome.timestamp_max(), Some(Timestamp::from_micros(30)));
    assert_eq!(
        branches
            .iter()
            .map(BranchLocalState::branch_id)
            .collect::<Vec<_>>(),
        vec![untouched_branch, branch_a, branch_b],
        "install must not reorder unrelated existing branch states",
    );

    let installed_a = branches
        .iter()
        .find(|state| state.branch_id() == branch_a)
        .expect("branch a installed");
    let installed_b = branches
        .iter()
        .find(|state| state.branch_id() == branch_b)
        .expect("branch b installed");
    assert_eq!(installed_a.active_row_count(), 0);
    assert_eq!(installed_a.owned_table_count(), 2);
    assert_eq!(installed_b.owned_table_count(), 1);
    assert_eq!(
        installed_a
            .reachability_snapshot()
            .expect("reachability")
            .facts()
            .owned_table_count(),
        2
    );

    let view_a = installed_a.capture_read_view().expect("branch a view");
    let view_b = installed_b.capture_read_view().expect("branch b view");
    assert_eq!(
        visible_storage_row(view_a.latest(&key_a).expect("latest a")),
        Some(storage_row_with(
            branch_a,
            b"snapshot-a".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"new".to_vec(),
        ))
    );
    assert_eq!(
        visible_storage_row(
            view_a
                .at_version(&key_a, CommitVersion::new(1))
                .expect("version a")
        ),
        Some(storage_row_with(
            branch_a,
            b"snapshot-a".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"old".to_vec(),
        ))
    );
    assert_eq!(
        visible_storage_row(view_b.latest(&key_b).expect("latest b")),
        Some(storage_row_with(
            branch_b,
            b"snapshot-b".to_vec(),
            3,
            30,
            Timestamp::EPOCH,
            b"created".to_vec(),
        ))
    );
}

#[test]
fn branch_snapshot_install_empty_plan_is_noop() {
    let branch = branch_id(203);
    let mut branches = vec![BranchLocalState::empty(branch)];
    let before = branches.clone();
    let request =
        BranchSnapshotInstallRequest::from_rows("snapshot-empty", Vec::new()).expect("request");
    assert!(request.groups().is_empty());

    let outcome =
        install_snapshot_rows_into_branches(&mut branches, &request).expect("empty install");

    assert!(outcome.is_empty_plan_noop());
    assert_eq!(outcome.rows_installed(), 0);
    assert_eq!(outcome.tables_created(), 0);
    assert_eq!(branches, before);
}

#[test]
fn branch_snapshot_install_from_rows_sorts_each_branch_group_by_internal_key() {
    let branch = branch_id(211);
    let key = physical_key(branch, b"snapshot-unsorted".to_vec());
    let newer = storage_row_with(
        branch,
        b"snapshot-unsorted".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"newer".to_vec(),
    );
    let older = storage_row_with(
        branch,
        b"snapshot-unsorted".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"older".to_vec(),
    );
    let request =
        BranchSnapshotInstallRequest::from_rows("snapshot-unsorted", vec![newer.clone(), older])
            .expect("from_rows request");
    let mut branches = vec![BranchLocalState::empty(branch)];

    install_snapshot_rows_into_branches(&mut branches, &request)
        .expect("install from unsorted rows");

    let view = branches[0].capture_read_view().expect("view");
    assert_eq!(
        visible_storage_row(view.latest(&key).expect("latest")),
        Some(newer)
    );
}

#[test]
fn branch_snapshot_install_rejects_missing_and_non_empty_targets_without_mutation() {
    let existing = branch_id(204);
    let missing = branch_id(205);
    let mut branches = vec![BranchLocalState::empty(existing)];
    let missing_request = BranchSnapshotInstallRequest::new(
        "snapshot-missing",
        vec![BranchSnapshotInstallGroup::new(
            missing,
            sorted_storage_rows(vec![storage_row(missing, 1)]),
        )],
    )
    .expect("missing request");
    let before_missing = branches.clone();

    assert!(matches!(
        install_snapshot_rows_into_branches(&mut branches, &missing_request),
        Err(BranchRuntimeError::BranchNotFound { branch_id }) if branch_id == missing
    ));
    assert_eq!(branches, before_missing);

    branches[0]
        .append_committed_row(storage_row(existing, 1))
        .expect("non-empty target row");
    let before_non_empty = branches.clone();
    let non_empty_request = BranchSnapshotInstallRequest::new(
        "snapshot-non-empty",
        vec![BranchSnapshotInstallGroup::new(
            existing,
            sorted_storage_rows(vec![storage_row_with(
                existing,
                b"snapshot-new".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"new".to_vec(),
            )]),
        )],
    )
    .expect("non-empty request");

    assert!(matches!(
        install_snapshot_rows_into_branches(&mut branches, &non_empty_request),
        Err(BranchRuntimeError::InvalidSnapshotInstall { reason })
            if reason == "snapshot install target branch must be empty"
    ));
    assert_eq!(branches, before_non_empty);
}

#[test]
fn branch_snapshot_install_rejects_invalid_rows_before_any_branch_mutates() {
    let branch_a = branch_id(206);
    let branch_b = branch_id(207);
    let mut branches = vec![
        BranchLocalState::empty(branch_a),
        BranchLocalState::empty(branch_b),
    ];
    let before = branches.clone();
    let mismatch_request = BranchSnapshotInstallRequest::new(
        "snapshot-mismatch",
        vec![BranchSnapshotInstallGroup::new(
            branch_a,
            sorted_storage_rows(vec![storage_row(branch_b, 1)]),
        )],
    )
    .expect("mismatch request");

    assert!(matches!(
        install_snapshot_rows_into_branches(&mut branches, &mismatch_request),
        Err(BranchRuntimeError::InvalidBranchRow { .. })
    ));
    assert_eq!(branches, before);

    let duplicate = storage_row(branch_a, 1);
    let duplicate_request = BranchSnapshotInstallRequest::new(
        "snapshot-duplicate",
        vec![BranchSnapshotInstallGroup::new(
            branch_a,
            vec![duplicate.clone(), duplicate],
        )],
    )
    .expect("duplicate request");
    assert_duplicate_internal_key(
        &install_snapshot_rows_into_branches(&mut branches, &duplicate_request)
            .expect_err("duplicate rejected"),
    );
    assert_eq!(branches, before);

    let unsorted_request = BranchSnapshotInstallRequest::new(
        "snapshot-unsorted",
        vec![BranchSnapshotInstallGroup::new(
            branch_a,
            vec![
                storage_row_with(
                    branch_a,
                    b"z".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    b"z".to_vec(),
                ),
                storage_row_with(
                    branch_a,
                    b"a".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    b"a".to_vec(),
                ),
            ],
        )],
    )
    .expect("unsorted request");
    assert!(matches!(
        install_snapshot_rows_into_branches(&mut branches, &unsorted_request),
        Err(BranchRuntimeError::InvalidSnapshotInstall { reason })
            if reason == "snapshot install rows must be strictly sorted by internal key"
    ));
    assert_eq!(branches, before);
}

#[test]
fn branch_snapshot_install_rejects_unsorted_branch_groups_without_mutation() {
    let lower = branch_id(211);
    let higher = branch_id(212);
    let mut branches = vec![
        BranchLocalState::empty(lower),
        BranchLocalState::empty(higher),
    ];
    let before = branches.clone();
    let request = BranchSnapshotInstallRequest::new(
        "snapshot-group-order",
        vec![
            BranchSnapshotInstallGroup::new(
                higher,
                sorted_storage_rows(vec![storage_row(higher, 1)]),
            ),
            BranchSnapshotInstallGroup::new(
                lower,
                sorted_storage_rows(vec![storage_row(lower, 1)]),
            ),
        ],
    )
    .expect("group order request");

    assert!(matches!(
        install_snapshot_rows_into_branches(&mut branches, &request),
        Err(BranchRuntimeError::InvalidSnapshotInstall { reason })
            if reason == "snapshot install branch groups must be sorted by branch id"
    ));
    assert_eq!(branches, before);
}

#[test]
fn branch_snapshot_install_table_build_failure_preserves_every_target() {
    let lower = branch_id(213);
    let higher = branch_id(214);
    let huge_key = vec![b'x'; 70 * 1024];
    let request = BranchSnapshotInstallRequest::new(
        "snapshot-build-fail",
        vec![
            BranchSnapshotInstallGroup::new(
                lower,
                sorted_storage_rows(vec![storage_row(lower, 1)]),
            ),
            BranchSnapshotInstallGroup::new(
                higher,
                sorted_storage_rows(vec![storage_row_with(
                    higher,
                    huge_key,
                    1,
                    10,
                    Timestamp::EPOCH,
                    b"secret-payload".to_vec(),
                )]),
            ),
        ],
    )
    .expect("build failure request");
    let debug_text = format!("{request:?}");
    assert!(!debug_text.contains("secret-payload"));

    let mut branches = vec![
        BranchLocalState::empty(lower),
        BranchLocalState::empty(higher),
    ];
    let before = branches.clone();
    let error = install_snapshot_rows_into_branches(&mut branches, &request)
        .expect_err("oversized key rejected by table builder");

    assert!(matches!(
        error,
        BranchRuntimeError::TableRuntime {
            source: crate::table::TableRuntimeError::InvalidRange { .. },
        }
    ));
    assert!(!error.to_string().contains("secret-payload"));
    assert_eq!(branches, before);
}

#[test]
fn branch_snapshot_install_rejects_output_identity_collisions_without_mutation() {
    let existing = branch_id(208);
    let target = branch_id(209);
    let target_rows = sorted_storage_rows(vec![storage_row(target, 1)]);
    let target_request = BranchSnapshotInstallRequest::new(
        "snapshot-collision",
        vec![BranchSnapshotInstallGroup::new(target, target_rows.clone())],
    )
    .expect("target request")
    .with_missing_branch_policy(BranchSnapshotMissingBranchPolicy::Create {
        config: BranchRuntimeConfig::default(),
    });
    let mut dry_run = Vec::new();
    let dry_run_outcome =
        install_snapshot_rows_into_branches(&mut dry_run, &target_request).expect("dry run");
    let collision_identity = dry_run_outcome.table_identities()[0].clone();

    let mut branches = vec![
        BranchLocalState::empty(existing),
        BranchLocalState::empty(target),
    ];
    branches[0]
        .install_l0_table(branch_owned_table(
            existing,
            BranchLevel::ZERO,
            collision_identity.as_str(),
            vec![storage_row(existing, 1)],
        ))
        .expect("install colliding table");
    let before = branches.clone();
    let request = BranchSnapshotInstallRequest::new(
        "snapshot-collision",
        vec![BranchSnapshotInstallGroup::new(target, target_rows)],
    )
    .expect("collision request");

    assert!(matches!(
        install_snapshot_rows_into_branches(&mut branches, &request),
        Err(BranchRuntimeError::InvalidSnapshotInstall { reason })
            if reason == "snapshot output identity must not collide with existing reachable table"
    ));
    assert_eq!(branches, before);
}

fn assert_duplicate_internal_key(error: &BranchRuntimeError) {
    assert!(matches!(
        error,
        BranchRuntimeError::TableRuntime {
            source: crate::table::TableRuntimeError::DuplicateInternalKey { .. },
        }
    ));
    assert!(error.source().is_some());
    assert!(!error.to_string().contains("secret-payload"));
}

#[test]
fn fork_snapshot_tolerates_identical_replay_redundancy_across_own_sources() {
    // ACID-005: idempotent recovery replay can leave the SAME row (same
    // internal key, same bytes) in a rotated-out source AND the replayed
    // active memtable. The fork snapshot must treat that byte-identical
    // redundancy as one row — refusing it bricks recovery's fork-row
    // rebuild for every child of such a source (#2823).
    let branch = branch_id(0x70);
    let target = branch_id(0x71);
    let mut state = BranchLocalState::empty(branch);
    let row = storage_row_with(
        branch,
        b"replayed".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"value".to_vec(),
    );
    state
        .append_committed_row(row.clone())
        .expect("first copy into the active memtable");
    state.rotate_active();
    state
        .append_committed_row(row)
        .expect("idempotent replay re-applies the same row into the new active");

    let rows = state
        .fork_snapshot_rows(CommitVersion::new(3), target)
        .expect("a replay-redundant source must still be forkable");
    assert_eq!(rows.len(), 1, "the redundancy collapses to one row");
}

#[test]
fn fork_snapshot_still_refuses_divergent_duplicate_internal_keys() {
    // Direction control: the SAME internal key with DIFFERENT bytes is a
    // genuine corruption signal and must keep refusing.
    let branch = branch_id(0x72);
    let target = branch_id(0x73);
    let mut state = BranchLocalState::empty(branch);
    state
        .append_committed_row(storage_row_with(
            branch,
            b"diverged".to_vec(),
            3,
            30,
            Timestamp::EPOCH,
            b"one".to_vec(),
        ))
        .expect("first copy");
    state.rotate_active();
    state
        .append_committed_row(storage_row_with(
            branch,
            b"diverged".to_vec(),
            3,
            30,
            Timestamp::EPOCH,
            b"two".to_vec(),
        ))
        .expect("divergent copy into the new active");

    let error = state
        .fork_snapshot_rows(CommitVersion::new(3), target)
        .expect_err("divergent duplicates are corruption and must refuse");
    assert!(matches!(
        error,
        BranchRuntimeError::InvalidBranchState { .. }
    ));
}
