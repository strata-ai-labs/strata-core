#![allow(clippy::too_many_lines)]

use super::*;

#[test]
fn branch_owned_table_constructor_rejects_descriptor_and_branch_mismatches() {
    let branch = branch_id(49);
    let row = storage_row_with(
        branch,
        b"owned-constructor".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"secret-payload".to_vec(),
    );
    let reader = immutable_reader("owned-constructor", vec![row]);
    let descriptor = branch_table_descriptor(BranchLevel::ZERO, &reader);
    let extras =
        crate::table::TableSummaryExtras::from_rows(reader.rows()).expect("table summary extras");
    let owned = BranchOwnedTable::new(branch, descriptor.clone(), reader.clone(), extras)
        .expect("owned table");
    assert_eq!(owned.branch_id(), branch);
    assert_eq!(owned.descriptor(), &descriptor);
    assert_eq!(owned.facts(), reader.facts());
    assert_eq!(owned.level(), BranchLevel::ZERO);
    assert_eq!(owned.rows().len(), 1);

    let other_reader = immutable_reader(
        "owned-constructor-other",
        vec![storage_row_with(
            branch,
            b"other".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"other".to_vec(),
        )],
    );
    let extras = crate::table::TableSummaryExtras::from_rows(other_reader.rows())
        .expect("table summary extras");
    assert!(matches!(
        BranchOwnedTable::new(branch, descriptor, other_reader, extras),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));

    let wrong_branch_reader = immutable_reader(
        "owned-constructor-wrong",
        vec![storage_row_with(
            branch_id(50),
            b"wrong".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"secret-payload".to_vec(),
        )],
    );
    let wrong_branch_descriptor = branch_table_descriptor(BranchLevel::ZERO, &wrong_branch_reader);
    let extras = crate::table::TableSummaryExtras::from_rows(wrong_branch_reader.rows())
        .expect("table summary extras");
    let error = BranchOwnedTable::new(branch, wrong_branch_descriptor, wrong_branch_reader, extras)
        .expect_err("wrong branch table rejected");
    assert!(matches!(error, BranchRuntimeError::InvalidBranchRow { .. }));
    assert!(!error.to_string().contains("secret-payload"));
}

#[test]
fn branch_owned_table_empty_immutable_input_is_rejected_before_install() {
    let branch = branch_id(68);
    let state = BranchLocalState::empty(branch);
    let builder = ImmutableTableBuilder::new(TableBuilderConfig::default()).expect("builder");
    let rows = Vec::<TableRow>::new();
    let error = builder
        .build_from_rows(
            TableIdentity::new("empty-owned-table").expect("identity"),
            &rows,
        )
        .expect_err("empty immutable table rejected");

    assert!(matches!(
        error,
        crate::table::TableRuntimeError::InvalidRange { field: "row_count" }
    ));
    assert_eq!(state.owned_table_count(), 0);
    assert!(state.is_empty());
}

#[test]
fn branch_local_state_installs_l0_table_and_reads_owned_sources() {
    let branch = branch_id(51);
    let mut state = BranchLocalState::empty(branch);
    let row = storage_row_with(
        branch,
        b"owned-l0".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"owned".to_vec(),
    );
    let table = branch_owned_table(branch, BranchLevel::ZERO, "owned-l0", vec![row.clone()]);

    let outcome: BranchImmutableInstallOutcome = state.install_l0_table(table).expect("install l0");
    assert_eq!(outcome.branch_id(), branch);
    assert_eq!(outcome.level(), BranchLevel::ZERO);
    assert_eq!(outcome.table_index(), 0);
    assert_eq!(outcome.level_table_count(), 1);
    assert_eq!(outcome.owned_table_count(), 1);
    assert_eq!(outcome.replaced_frozen_index(), None);
    assert_eq!(state.owned_table_count(), 1);
    assert_eq!(state.max_commit_version(), Some(CommitVersion::new(5)));
    assert_eq!(state.put_rows(), 1);

    let facts = state.facts().expect("facts");
    assert_eq!(facts.owned_table_count(), 1);
    assert_eq!(facts.max_commit_version(), Some(CommitVersion::new(5)));
    let view = state.capture_read_view().expect("view");
    assert_eq!(view.owned_table_count(), 1);
    assert_eq!(view.owned_levels()[0].len(), 1);
    let visible = view
        .latest(&physical_key(branch, b"owned-l0".to_vec()))
        .expect("latest")
        .expect("owned row");
    assert_eq!(visible.row(), &row);
    assert_eq!(
        visible.source(),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        }
    );
}

#[test]
fn branch_local_state_rejects_reachable_table_identity_collisions_without_mutation() {
    let branch = branch_id(250);
    let source = branch_id(251);
    let child = branch_id(252);
    let mut state = BranchLocalState::empty(branch);
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "owned-identity-collision",
            vec![storage_row_with(
                branch,
                b"identity-a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"a".to_vec(),
            )],
        ))
        .expect("install first identity");
    let before_owned_collision = state.clone();
    let owned_collision = state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "owned-identity-collision",
            vec![storage_row_with(
                branch,
                b"identity-b".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"b".to_vec(),
            )],
        ))
        .expect_err("owned table identity collision rejected");
    assert_eq!(
        owned_collision,
        BranchRuntimeError::InvalidBranchState {
            reason: "branch-owned table identity must not collide with reachable table",
        }
    );
    assert_eq!(state, before_owned_collision);

    let mut source_state = BranchLocalState::empty(source);
    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "inherited-identity-collision",
            vec![storage_row_with(
                source,
                b"inherited-identity".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"source".to_vec(),
            )],
        ))
        .expect("install inherited identity");
    let (mut child_state, _) = source_state
        .fork_into_empty_child(child)
        .expect("fork inherited identity child");
    let before_inherited_collision = child_state.clone();
    let inherited_collision = child_state
        .install_l0_table(branch_owned_table(
            child,
            BranchLevel::ZERO,
            "inherited-identity-collision",
            vec![storage_row_with(
                child,
                b"child-identity".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"child".to_vec(),
            )],
        ))
        .expect_err("inherited table identity collision rejected");
    assert_eq!(
        inherited_collision,
        BranchRuntimeError::InvalidBranchState {
            reason: "branch-owned table identity must not collide with reachable table",
        }
    );
    assert_eq!(child_state, before_inherited_collision);
}

#[test]
fn branch_local_state_rejects_owned_table_for_other_branch_without_mutation() {
    let branch = branch_id(56);
    let other = branch_id(57);
    let mut state = BranchLocalState::empty(branch);
    let table = branch_owned_table(
        other,
        BranchLevel::ZERO,
        "owned-other-branch",
        vec![storage_row_with(
            other,
            b"wrong-branch-owned".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"secret-payload".to_vec(),
        )],
    );
    let before = state.clone();

    let error = state
        .install_l0_table(table)
        .expect_err("other branch table rejected");
    assert!(matches!(error, BranchRuntimeError::InvalidBranchRow { .. }));
    assert!(!error.to_string().contains("secret-payload"));
    assert_eq!(state, before);
}

#[test]
fn branch_local_state_replaces_frozen_with_l0_without_mutating_pinned_views() {
    let branch = branch_id(52);
    let mut state = BranchLocalState::empty(branch);
    let row = storage_row_with(
        branch,
        b"flush".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"flush".to_vec(),
    );
    state.append_committed_row(row.clone()).expect("append row");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    let pinned = state.capture_read_view().expect("pinned view");
    let table = branch_owned_table(branch, BranchLevel::ZERO, "flush-l0", vec![row.clone()]);

    let outcome: BranchImmutableInstallOutcome = state
        .replace_frozen_with_l0_table(0, table)
        .expect("replace frozen");
    assert_eq!(outcome.replaced_frozen_index(), Some(0));
    assert_eq!(state.frozen_table_count(), 0);
    assert_eq!(state.owned_table_count(), 1);

    let key = physical_key(branch, b"flush".to_vec());
    let before = pinned.latest(&key).expect("pinned latest").expect("row");
    assert_eq!(before.row(), &row);
    assert_eq!(before.source(), BranchRowSource::Frozen { index: 0 });

    let after_view = state.capture_read_view().expect("after view");
    let after = after_view.latest(&key).expect("after latest").expect("row");
    assert_eq!(after.row(), &row);
    assert_eq!(
        after.source(),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        }
    );
}

#[test]
fn branch_frozen_replacement_rejects_mismatches_without_mutation() {
    let branch = branch_id(55);
    let mut state = BranchLocalState::empty(branch);
    let row = storage_row_with(
        branch,
        b"flush-mismatch".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"frozen".to_vec(),
    );
    state.append_committed_row(row.clone()).expect("append row");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    let before = state.clone();

    let mismatched_value = storage_row_with(
        branch,
        b"flush-mismatch".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"different".to_vec(),
    );
    let mismatch = branch_owned_table(
        branch,
        BranchLevel::ZERO,
        "flush-mismatch",
        vec![mismatched_value],
    );
    assert!(matches!(
        state.replace_frozen_with_l0_table(0, mismatch),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
    assert_eq!(state, before);

    let replacement =
        branch_owned_table(branch, BranchLevel::ZERO, "flush-out-of-range", vec![row]);
    assert!(matches!(
        state.replace_frozen_with_l0_table(1, replacement),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
    assert_eq!(state, before);
}

#[test]
fn branch_read_view_scans_owned_immutable_tables_and_pins_before_l0_install() {
    let branch = branch_id(58);
    let mut state = BranchLocalState::empty(branch);
    let pinned_before = state.capture_read_view().expect("pre-install view");
    let live = storage_row_with(
        branch,
        b"scan-owned-a".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"live".to_vec(),
    );
    let old_deleted = storage_row_with(
        branch,
        b"scan-owned-b".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"old".to_vec(),
    );
    let deleting_tombstone = tombstone_row(branch, b"scan-owned-b".to_vec(), 4, 40);
    let high = storage_row_with(
        branch,
        vec![b's', b'c', b'a', b'n', 0x80],
        2,
        20,
        Timestamp::EPOCH,
        b"high".to_vec(),
    );
    let table = branch_owned_table(
        branch,
        BranchLevel::ZERO,
        "owned-scan",
        vec![live.clone(), old_deleted, deleting_tombstone, high.clone()],
    );
    state.install_l0_table(table).expect("install scan table");

    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"scan-owned".to_vec()));
    assert!(pinned_before
        .scan_prefix(&prefix, BranchReadBound::latest())
        .expect("pinned prefix")
        .is_empty());

    let view = state.capture_read_view().expect("post-install view");
    let prefix_rows = view
        .scan_prefix(&prefix, BranchReadBound::latest())
        .expect("prefix scan");
    assert_eq!(scan_user_keys(&prefix_rows), vec![b"scan-owned-a".to_vec()]);
    assert_eq!(prefix_rows[0].row(), &live);
    assert_eq!(
        prefix_rows[0].source(),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        }
    );

    let range = BranchScanBounds::closed(
        &physical_key(branch, b"scan-owned-a".to_vec()),
        &physical_key(branch, vec![b's', b'c', b'a', b'n', 0x80]),
    )
    .expect("closed owned range");
    let range_rows = view
        .scan_range(&range, BranchReadBound::latest())
        .expect("range scan");
    assert_eq!(
        scan_user_keys(&range_rows),
        vec![b"scan-owned-a".to_vec(), vec![b's', b'c', b'a', b'n', 0x80]]
    );
    assert_eq!(range_rows[1].row(), &high);
}

#[test]
fn branch_owned_l0_tables_accept_overlaps_and_select_by_version_not_index() {
    let branch = branch_id(59);
    let mut state = BranchLocalState::empty(branch);
    let key = physical_key(branch, b"l0-overlap".to_vec());
    let newer = storage_row_with(
        branch,
        b"l0-overlap".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"newer".to_vec(),
    );
    let older = storage_row_with(
        branch,
        b"l0-overlap".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"older".to_vec(),
    );

    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "l0-overlap-newer",
            vec![newer.clone()],
        ))
        .expect("install newer L0");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "l0-overlap-older",
            vec![older.clone()],
        ))
        .expect("install overlapping older L0");
    assert_eq!(state.owned_levels()[0].len(), 2);
    assert_eq!(
        state.owned_levels()[0][0].descriptor().identity().as_str(),
        "l0-overlap-older"
    );

    let view = state.capture_read_view().expect("view");
    assert_visible_row(
        view.latest(&key).expect("latest").as_ref(),
        &newer,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 1,
        },
    );
    assert_visible_row(
        view.at_version(&key, CommitVersion::new(2))
            .expect("bounded")
            .as_ref(),
        &older,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
}

#[test]
fn branch_frozen_replacement_targets_named_frozen_table_and_keeps_l0_front() {
    let branch = branch_id(60);
    let mut state = BranchLocalState::empty(branch);
    let older_frozen = storage_row_with(
        branch,
        b"replace-old".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"old".to_vec(),
    );
    let newer_frozen = storage_row_with(
        branch,
        b"replace-new".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"new".to_vec(),
    );
    state
        .append_committed_row(older_frozen.clone())
        .expect("append older");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .append_committed_row(newer_frozen.clone())
        .expect("append newer");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "preexisting-l0",
            vec![storage_row_with(
                branch,
                b"preexisting".to_vec(),
                3,
                30,
                Timestamp::EPOCH,
                b"preexisting".to_vec(),
            )],
        ))
        .expect("install preexisting L0");
    let pinned = state.capture_read_view().expect("pinned");

    let outcome = state
        .replace_frozen_with_l0_table(
            1,
            branch_owned_table(
                branch,
                BranchLevel::ZERO,
                "replace-old-l0",
                vec![older_frozen.clone()],
            ),
        )
        .expect("replace older frozen");
    assert_eq!(outcome.replaced_frozen_index(), Some(1));
    assert_eq!(state.frozen_table_count(), 1);
    assert_eq!(state.owned_levels()[0].len(), 2);
    assert_eq!(
        state.owned_levels()[0][0].descriptor().identity().as_str(),
        "replace-old-l0"
    );
    assert_eq!(
        state.owned_levels()[0][1].descriptor().identity().as_str(),
        "preexisting-l0"
    );

    let old_key = physical_key(branch, b"replace-old".to_vec());
    let new_key = physical_key(branch, b"replace-new".to_vec());
    assert_visible_row(
        pinned.latest(&old_key).expect("pinned old").as_ref(),
        &older_frozen,
        BranchRowSource::Frozen { index: 1 },
    );
    let after = state.capture_read_view().expect("after");
    assert_visible_row(
        after.latest(&old_key).expect("after old").as_ref(),
        &older_frozen,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_visible_row(
        after.latest(&new_key).expect("after new").as_ref(),
        &newer_frozen,
        BranchRowSource::Frozen { index: 0 },
    );
}

#[test]
fn branch_owned_nonzero_levels_are_sorted_and_reject_overlaps_without_mutation() {
    let branch = branch_id(53);
    let config = BranchRuntimeConfig::new(3, 64, 32).expect("config");
    let mut state = BranchLocalState::new(branch, config).expect("state");
    let level = BranchLevel::new(1);
    let z_table = branch_owned_table(
        branch,
        level,
        "level-one-z",
        vec![storage_row_with(
            branch,
            b"z".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"z".to_vec(),
        )],
    );
    let ac_table = branch_owned_table(
        branch,
        level,
        "level-one-ac",
        vec![
            storage_row_with(
                branch,
                b"a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"a".to_vec(),
            ),
            storage_row_with(
                branch,
                b"c".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"c".to_vec(),
            ),
        ],
    );

    assert_eq!(
        state
            .install_owned_table_at_level(level, z_table)
            .expect("install z")
            .table_index(),
        0
    );
    assert_eq!(
        state
            .install_owned_table_at_level(level, ac_table)
            .expect("install ac")
            .table_index(),
        0
    );
    assert_eq!(
        state.owned_levels()[1][0].descriptor().identity().as_str(),
        "level-one-ac"
    );
    assert_eq!(
        state.owned_levels()[1][1].descriptor().identity().as_str(),
        "level-one-z"
    );

    let before = state.clone();
    let overlap = branch_owned_table(
        branch,
        level,
        "level-one-overlap",
        vec![storage_row_with(
            branch,
            b"b".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"b".to_vec(),
        )],
    );
    assert!(matches!(
        state.install_owned_table_at_level(level, overlap),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
    assert_eq!(state, before);

    let wrong_level_table = branch_owned_table(
        branch,
        BranchLevel::new(2),
        "wrong-level",
        vec![storage_row(branch, 9)],
    );
    assert!(matches!(
        state.install_owned_table_at_level(level, wrong_level_table),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
}

#[test]
fn branch_owned_nonzero_levels_reject_same_physical_key_with_distinct_versions() {
    let branch = branch_id(54);
    let config = BranchRuntimeConfig::new(3, 64, 32).expect("config");
    let mut state = BranchLocalState::new(branch, config).expect("state");
    let level = BranchLevel::new(1);
    let key = b"physical-overlap".to_vec();
    state
        .install_owned_table_at_level(
            level,
            branch_owned_table(
                branch,
                level,
                "physical-overlap-newer",
                vec![storage_row_with(
                    branch,
                    key.clone(),
                    8,
                    80,
                    Timestamp::EPOCH,
                    b"newer".to_vec(),
                )],
            ),
        )
        .expect("install newer physical range");
    let before = state.clone();
    let error = state
        .install_owned_table_at_level(
            level,
            branch_owned_table(
                branch,
                level,
                "physical-overlap-older",
                vec![storage_row_with(
                    branch,
                    key,
                    3,
                    30,
                    Timestamp::EPOCH,
                    b"older".to_vec(),
                )],
            ),
        )
        .expect_err("same physical key must overlap at nonzero levels");
    assert_eq!(
        error,
        BranchRuntimeError::InvalidBranchState {
            reason: "branch-owned nonzero level tables must not overlap by physical key range",
        }
    );
    assert_eq!(state, before);
}

#[test]
fn branch_compaction_rejects_pruning_policies_without_mutation() {
    let branch = branch_id(121);
    let mut state = BranchLocalState::empty(branch);
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-prune-a",
            vec![storage_row_with(
                branch,
                b"compact-prune-a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"a".to_vec(),
            )],
        ))
        .expect("install first input");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-prune-b",
            vec![storage_row_with(
                branch,
                b"compact-prune-b".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"b".to_vec(),
            )],
        ))
        .expect("install second input");
    let before = state.clone();

    for retention_policy in [
        BranchCompactionRetentionPolicy::DropOlderVersions,
        BranchCompactionRetentionPolicy::DropTombstones,
        BranchCompactionRetentionPolicy::DropExpired,
    ] {
        let request =
            BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "prune-output")
                .expect("request")
                .with_retention_policy(retention_policy);
        let error = state
            .compact_branch_owned_tables(&request)
            .expect_err("pruning rejected without proof");
        assert!(matches!(
            error,
            BranchRuntimeError::InvalidCompaction { .. }
        ));
        assert_eq!(state, before);
    }
}

#[test]
fn branch_compaction_l0_keep_all_installs_replacement_and_preserves_pinned_view() {
    let branch = branch_id(122);
    let mut state = BranchLocalState::empty(branch);
    let key = physical_key(branch, b"compact-l0".to_vec());
    let newer = storage_row_with(
        branch,
        b"compact-l0".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"newer".to_vec(),
    );
    let older = storage_row_with(
        branch,
        b"compact-l0".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"older".to_vec(),
    );
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-l0-newer",
            vec![newer.clone()],
        ))
        .expect("install newer");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-l0-older",
            vec![older.clone()],
        ))
        .expect("install older");
    let pinned = state.capture_read_view().expect("pinned before compaction");

    let request =
        BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "compact-l0-output")
            .expect("request")
            .with_table_compaction_config(TableCompactionConfig::default())
            .with_table_builder_config(TableBuilderConfig::default());
    let plan: BranchCompactionPlan = state.plan_branch_compaction(&request).expect("plan");
    let candidate: &BranchCompactionCandidate = plan.candidate().expect("candidate");
    assert_eq!(candidate.input_refs().len(), 2);
    assert_eq!(candidate.overlap_refs(), &[]);
    assert_eq!(candidate.output_level(), BranchLevel::ZERO);
    assert_eq!(candidate.source_count(), 2);
    assert_eq!(candidate.input_row_count(), 2);

    let outcome: BranchCompactionOutcome = state
        .compact_branch_owned_tables(&request)
        .expect("compact l0");
    assert_eq!(outcome.branch_id(), branch);
    assert!(outcome.installed_replacement_tables());
    assert!(outcome.candidate().is_some());
    assert_eq!(outcome.removed_refs().len(), 2);
    assert_eq!(outcome.output_refs().len(), 1);
    assert_eq!(outcome.owned_table_count(), 1);
    let report = outcome.table_report().expect("table report");
    assert_eq!(report.input_sources(), 2);
    assert_eq!(report.input_rows(), 2);
    assert_eq!(report.kept_rows(), 2);
    assert_eq!(report.dropped_rows(), 0);
    assert_eq!(state.owned_levels()[0].len(), 1);

    assert_visible_row(
        pinned.latest(&key).expect("pinned latest").as_ref(),
        &newer,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 1,
        },
    );
    let after = state.capture_read_view().expect("after compaction");
    assert_visible_row(
        after.latest(&key).expect("after latest").as_ref(),
        &newer,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_eq!(
        history_versions(
            &after
                .history(&key, BranchHistoryOptions::all())
                .expect("history")
        ),
        vec![5, 2]
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_compaction_streams_candidate_sources_with_bounded_output_buffer() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(189);
    let mut state = BranchLocalState::empty(branch);
    for (index, key) in [b"stream-a", b"stream-b", b"stream-c", b"stream-d"]
        .into_iter()
        .enumerate()
    {
        let version = u64::try_from(index)
            .expect("index fits in u64")
            .saturating_add(1);
        let identity = format!("stream-source-{index}");
        state
            .install_l0_table(branch_owned_table(
                branch,
                BranchLevel::ZERO,
                identity.as_str(),
                vec![storage_row_with(
                    branch,
                    key.to_vec(),
                    version,
                    version.saturating_mul(10),
                    Timestamp::EPOCH,
                    key.to_vec(),
                )],
            ))
            .expect("install source table");
    }

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0,
        "stream-compaction-output",
    )
    .expect("request")
    .with_table_compaction_config(TableCompactionConfig::new(1, 16).expect("split config"));
    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    assert_eq!(candidate.source_count(), 4);
    assert_eq!(candidate.input_row_count(), 4);

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("compact sources");
    let report = outcome.table_report().expect("report");
    assert_eq!(report.input_sources(), candidate.source_count());
    assert_eq!(report.input_rows(), candidate.input_row_count());
    assert_eq!(report.kept_rows(), candidate.input_row_count());
    assert!(report.split_count() > 0);
    assert_eq!(report.peak_buffered_rows(), 1);

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(
        perf.branch_compaction_source_opens(),
        u64::try_from(candidate.source_count()).expect("source count fits in u64")
    );
    assert_eq!(
        perf.branch_compaction_peak_buffered_rows(),
        u64::try_from(report.peak_buffered_rows()).expect("peak fits in u64")
    );
    assert_eq!(
        perf.table_compaction_peak_buffered_rows(),
        u64::try_from(report.peak_buffered_rows()).expect("peak fits in u64")
    );
    assert!(perf.branch_compaction_peak_buffered_rows() < candidate.input_row_count());
}

/// BS3.1 reunion test (owed from Slice 4): the parallel subcompaction split must be byte-equivalent
/// to the serial (whole-range) compaction. Build distinct-key L0 sources, force a >1-way split, and
/// assert the per-range bounded builds — concatenated in range order — reproduce the serial output
/// row-for-row. One key is written twice (across two L0 tables) so the test also proves the split
/// keeps every version of a physical key on one side of a boundary — boundaries fall on physical
/// keys, so a bound off-by-one that tore a key's versions apart would break the reunion. This is the
/// correctness contract that lets the fan-out ship (default-off after BS3.1) without diverging.
#[test]
fn subcompaction_ranges_reunite_into_the_serial_compaction_output() {
    const SUBCOMPACTIONS: usize = 4;
    let branch = branch_id(0x3B);
    let mut state = BranchLocalState::empty(branch);
    // Six distinct-key L0 tables — distinct keys are required or the subcompaction boundaries (which
    // fall on table last-keys) collapse to a single serial range.
    let keys: [&[u8]; 6] = [
        b"reunion-a",
        b"reunion-b",
        b"reunion-c",
        b"reunion-d",
        b"reunion-e",
        b"reunion-f",
    ];
    for (index, user_key) in keys.into_iter().enumerate() {
        let version = u64::try_from(index)
            .expect("index fits in u64")
            .saturating_add(1);
        let identity = format!("reunion-source-{index}");
        state
            .install_l0_table(branch_owned_table(
                branch,
                BranchLevel::ZERO,
                identity.as_str(),
                vec![storage_row_with(
                    branch,
                    user_key.to_vec(),
                    version,
                    version.saturating_mul(10),
                    Timestamp::EPOCH,
                    user_key.to_vec(),
                )],
            ))
            .expect("install source table");
    }
    // A repeated physical key across two L0 tables (a second version of `reunion-c`). Both versions
    // share one physical key, so every range boundary — placed on a physical key — must keep them
    // together; the reunion tears if a bounded cursor leaks one version into the neighbouring range.
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "reunion-source-dup",
            vec![storage_row_with(
                branch,
                b"reunion-c".to_vec(),
                7,
                70,
                Timestamp::EPOCH,
                b"reunion-c-v7".to_vec(),
            )],
        ))
        .expect("install duplicate-key source table");

    // target_output_bytes = 1 forces the split; max_output_tables (16) is high enough not to bind.
    let request =
        BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "reunion-output")
            .expect("request")
            .with_table_compaction_config(TableCompactionConfig::new(1, 16).expect("split config"));
    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    // Anti-vacuous: a real merge (not a metadata promotion), and the split must actually fan out.
    assert!(
        !candidate.is_metadata_promotion(),
        "reunion needs a real merge, not a metadata promotion"
    );
    let ranges = state
        .subcompaction_ranges_for_candidate(
            candidate,
            SUBCOMPACTIONS,
            request.table_compaction_config().target_output_bytes(),
        )
        .expect("subcompaction ranges");
    assert!(
        ranges.len() > 1,
        "the split must fan out to more than one range (got {})",
        ranges.len()
    );

    // Serial (whole-range) build.
    let (serial_artifacts, _serial_report) = state
        .prepare_branch_compaction_plan(&request, &plan)
        .expect("serial prepare succeeds")
        .expect("serial produces output");
    let serial_rows: Vec<StorageRow> = serial_artifacts
        .into_iter()
        .flat_map(|artifact| artifact.into_parts_with_rows().2)
        .map(TableRow::into_row)
        .collect();

    // Bounded (per-range) builds, concatenated in range order.
    let mut bounded_rows: Vec<StorageRow> = Vec::new();
    for (index, bounds) in ranges.iter().enumerate() {
        let (artifacts, _report) = state
            .prepare_branch_compaction_plan_bounded(&request, &plan, bounds.as_ref(), index)
            .expect("bounded prepare succeeds")
            .expect("bounded prepare returns a build");
        bounded_rows.extend(
            artifacts
                .into_iter()
                .flat_map(|artifact| artifact.into_parts_with_rows().2)
                .map(TableRow::into_row),
        );
    }

    // Complete row-for-row equality: StorageRow: Eq covers key + value + version + timestamp +
    // expiry + tombstone, unlike the key-only TableRow PartialEq.
    assert_eq!(
        bounded_rows, serial_rows,
        "subcompaction ranges did not reunite into the serial compaction output"
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_compaction_streams_l0_to_nonzero_overlap_sources_once() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(243);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    let first_input = storage_row_with(
        branch,
        b"stream-target-a".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"a".to_vec(),
    );
    let last_input = storage_row_with(
        branch,
        b"stream-target-z".to_vec(),
        6,
        60,
        Timestamp::EPOCH,
        b"z".to_vec(),
    );
    let overlapping_target = storage_row_with(
        branch,
        b"stream-target-m".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"m".to_vec(),
    );
    let preserved_target = storage_row_with(
        branch,
        b"stream-target-zz".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"zz".to_vec(),
    );
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "stream-target-overlap",
                vec![overlapping_target.clone()],
            ),
        )
        .expect("install overlap target");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "stream-target-preserved",
                vec![preserved_target.clone()],
            ),
        )
        .expect("install preserved target");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "stream-input-range",
            vec![first_input.clone(), last_input.clone()],
        ))
        .expect("install input");

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0ToLevelOne,
        "stream-l0-target-output",
    )
    .expect("request")
    .with_table_compaction_config(TableCompactionConfig::new(1, 16).expect("split config"));
    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    assert_eq!(candidate.input_refs().len(), 1);
    assert_eq!(candidate.overlap_refs().len(), 1);
    assert_eq!(candidate.source_count(), 2);
    assert_eq!(candidate.input_row_count(), 3);

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("compact input with overlap");
    let report = outcome.table_report().expect("report");
    assert_eq!(report.input_sources(), candidate.source_count());
    assert_eq!(report.input_rows(), candidate.input_row_count());
    assert_eq!(report.kept_rows(), candidate.input_row_count());
    assert_eq!(report.peak_buffered_rows(), 1);
    assert_eq!(state.owned_levels()[0].len(), 0);
    assert_eq!(state.owned_levels()[1].len(), 4);

    let after = state.capture_read_view().expect("after view");
    for row in [
        &first_input,
        &overlapping_target,
        &last_input,
        &preserved_target,
    ] {
        assert!(after
            .latest(row.physical_key())
            .expect("latest")
            .is_some_and(|visible| visible.row() == row));
    }

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(
        perf.branch_compaction_source_opens(),
        u64::try_from(candidate.source_count()).expect("source count fits in u64")
    );
    assert_eq!(
        perf.branch_compaction_peak_buffered_rows(),
        u64::try_from(report.peak_buffered_rows()).expect("peak fits in u64")
    );
    assert_eq!(
        perf.table_compaction_peak_buffered_rows(),
        u64::try_from(report.peak_buffered_rows()).expect("peak fits in u64")
    );
    assert!(perf.branch_compaction_peak_buffered_rows() < candidate.input_row_count());
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_compaction_streams_nonzero_overlap_sources_once() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(244);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(4, 64, 32).expect("branch config"),
    )
    .expect("state");
    let first_input = storage_row_with(
        branch,
        b"stream-promote-a".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        b"a".to_vec(),
    );
    let last_input = storage_row_with(
        branch,
        b"stream-promote-z".to_vec(),
        8,
        80,
        Timestamp::EPOCH,
        b"z".to_vec(),
    );
    let overlapping_target = storage_row_with(
        branch,
        b"stream-promote-m".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"m".to_vec(),
    );
    let preserved_target = storage_row_with(
        branch,
        b"stream-promote-zz".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"zz".to_vec(),
    );
    state
        .install_owned_table_at_level(
            BranchLevel::new(2),
            branch_owned_table(
                branch,
                BranchLevel::new(2),
                "stream-promote-overlap",
                vec![overlapping_target.clone()],
            ),
        )
        .expect("install overlap target");
    state
        .install_owned_table_at_level(
            BranchLevel::new(2),
            branch_owned_table(
                branch,
                BranchLevel::new(2),
                "stream-promote-preserved",
                vec![preserved_target.clone()],
            ),
        )
        .expect("install preserved target");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "stream-promote-input",
                vec![first_input.clone(), last_input.clone()],
            ),
        )
        .expect("install input");

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::new(1),
            table_index: 0,
        },
        "stream-promote-output",
    )
    .expect("request")
    .with_table_compaction_config(TableCompactionConfig::new(1, 16).expect("split config"));
    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    assert_eq!(candidate.input_refs().len(), 1);
    assert_eq!(candidate.overlap_refs().len(), 1);
    assert_eq!(
        candidate.no_promotion_reason(),
        Some(BranchCompactionNoPromotionReason::TargetLevelOverlap)
    );
    assert_eq!(candidate.output_level(), BranchLevel::new(2));
    assert_eq!(candidate.source_count(), 2);
    assert_eq!(candidate.input_row_count(), 3);

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("compact promoted input");
    let report = outcome.table_report().expect("report");
    assert_eq!(report.input_sources(), candidate.source_count());
    assert_eq!(report.input_rows(), candidate.input_row_count());
    assert_eq!(report.kept_rows(), candidate.input_row_count());
    assert_eq!(report.peak_buffered_rows(), 1);
    assert_eq!(state.owned_levels()[1].len(), 0);
    assert_eq!(state.owned_levels()[2].len(), 4);

    let level_key_ranges = state.owned_levels()[2]
        .iter()
        .map(|table| table.facts().key_range())
        .collect::<Vec<_>>();
    for adjacent in level_key_ranges.windows(2) {
        assert!(adjacent[0].last_key() < adjacent[1].first_key());
    }

    let after = state.capture_read_view().expect("after view");
    for row in [
        &first_input,
        &overlapping_target,
        &last_input,
        &preserved_target,
    ] {
        assert!(after
            .latest(row.physical_key())
            .expect("latest")
            .is_some_and(|visible| visible.row() == row));
    }

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(
        perf.branch_compaction_source_opens(),
        u64::try_from(candidate.source_count()).expect("source count fits in u64")
    );
    assert_eq!(
        perf.branch_compaction_peak_buffered_rows(),
        u64::try_from(report.peak_buffered_rows()).expect("peak fits in u64")
    );
    assert_eq!(
        perf.table_compaction_peak_buffered_rows(),
        u64::try_from(report.peak_buffered_rows()).expect("peak fits in u64")
    );
    assert!(perf.branch_compaction_peak_buffered_rows() < candidate.input_row_count());
}

#[test]
#[allow(clippy::too_many_lines)]
fn branch_compaction_l0_to_l1_includes_overlaps_and_preserves_non_overlaps() {
    let branch = branch_id(123);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    let compacted_key = physical_key(branch, b"compact-l1-m".to_vec());
    let preserved_key = physical_key(branch, b"compact-l1-z".to_vec());
    let l0_row = storage_row_with(
        branch,
        b"compact-l1-m".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"l0".to_vec(),
    );
    let overlapping_l1 = storage_row_with(
        branch,
        b"compact-l1-m".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"l1".to_vec(),
    );
    let non_overlapping_l1 = storage_row_with(
        branch,
        b"compact-l1-z".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"z".to_vec(),
    );
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "compact-l1-overlap",
                vec![overlapping_l1.clone()],
            ),
        )
        .expect("install overlapping l1");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "compact-l1-preserved",
                vec![non_overlapping_l1.clone()],
            ),
        )
        .expect("install non-overlapping l1");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-l0-to-l1",
            vec![l0_row.clone()],
        ))
        .expect("install l0 input");

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0ToLevelOne,
        "compact-l0-to-l1-output",
    )
    .expect("request");
    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    assert_eq!(candidate.input_refs().len(), 1);
    assert_eq!(candidate.overlap_refs().len(), 1);
    assert_eq!(
        candidate.operation(),
        BranchCompactionOperation::TableRewrite
    );
    assert!(candidate.requires_table_rewrite());
    assert_eq!(
        candidate.no_promotion_reason(),
        Some(BranchCompactionNoPromotionReason::TargetLevelOverlap)
    );
    assert_eq!(candidate.output_level(), BranchLevel::new(1));

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("compact l0 to l1");
    assert_eq!(outcome.removed_refs().len(), 2);
    assert_eq!(outcome.output_refs().len(), 1);
    assert_eq!(state.owned_levels()[0].len(), 0);
    assert_eq!(state.owned_levels()[1].len(), 2);
    assert!(state.owned_levels()[1]
        .iter()
        .any(|table| table.descriptor().identity().as_str() == "compact-l1-preserved"));

    let after = state.capture_read_view().expect("after view");
    assert_visible_row(
        after
            .latest(&compacted_key)
            .expect("compacted latest")
            .as_ref(),
        &l0_row,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );
    assert_visible_row(
        after
            .latest(&preserved_key)
            .expect("preserved latest")
            .as_ref(),
        &non_overlapping_l1,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 1,
        },
    );
}

#[test]
fn branch_compaction_l0_to_l1_single_no_overlap_promotes_table_metadata() {
    let branch = branch_id(171);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    let row = storage_row_with(
        branch,
        b"compact-l0-single".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"value".to_vec(),
    );
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-l0-single",
            vec![row.clone()],
        ))
        .expect("install l0 input");

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0ToLevelOne,
        "compact-l0-single-output",
    )
    .expect("request");
    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    assert_eq!(
        candidate.operation(),
        BranchCompactionOperation::MetadataPromotion
    );
    assert!(candidate.is_metadata_promotion());
    assert!(!candidate.requires_table_rewrite());
    assert_eq!(candidate.no_promotion_reason(), None);
    assert_eq!(candidate.input_refs().len(), 1);
    assert!(candidate.overlap_refs().is_empty());
    assert_eq!(candidate.output_level(), BranchLevel::new(1));
    assert_eq!(candidate.input_row_count(), 1);
    assert!(state
        .prepare_branch_compaction_plan(&request, &plan)
        .expect("prepare promotion")
        .is_none());

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("promote l0 input");

    assert_eq!(outcome.table_report(), None);
    assert_eq!(outcome.removed_refs().len(), 1);
    assert_eq!(outcome.output_refs().len(), 1);
    assert_eq!(outcome.removed_refs()[0].level(), BranchLevel::ZERO);
    assert_eq!(outcome.output_refs()[0].level(), BranchLevel::new(1));
    assert_eq!(
        outcome.removed_refs()[0].table_identity(),
        outcome.output_refs()[0].table_identity()
    );
    assert_eq!(state.owned_levels()[0].len(), 0);
    assert_eq!(state.owned_levels()[1].len(), 1);
    assert_eq!(state.owned_levels()[1][0].level(), BranchLevel::new(1));
    assert_eq!(
        state.owned_levels()[1][0].rows(),
        &[TableRow::new(row.clone())]
    );

    let after = state.capture_read_view().expect("after view");
    assert_visible_row(
        after
            .latest(&physical_key(branch, b"compact-l0-single".to_vec()))
            .expect("promoted latest")
            .as_ref(),
        &row,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );
}

#[test]
fn branch_compaction_l0_to_l1_single_no_overlap_inserts_sorted() {
    let branch = branch_id(172);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "compact-l1-existing-a",
                vec![storage_row_with(
                    branch,
                    b"compact-l1-a".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    b"a".to_vec(),
                )],
            ),
        )
        .expect("install first target table");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "compact-l1-existing-z",
                vec![storage_row_with(
                    branch,
                    b"compact-l1-z".to_vec(),
                    2,
                    20,
                    Timestamp::EPOCH,
                    b"z".to_vec(),
                )],
            ),
        )
        .expect("install last target table");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-l0-middle",
            vec![storage_row_with(
                branch,
                b"compact-l1-m".to_vec(),
                3,
                30,
                Timestamp::EPOCH,
                b"middle".to_vec(),
            )],
        ))
        .expect("install l0 input");

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0ToLevelOne,
        "compact-l0-middle-output",
    )
    .expect("request");
    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("promote l0 input");

    assert_eq!(outcome.table_report(), None);
    assert_eq!(state.owned_levels()[0].len(), 0);
    assert_eq!(
        state.owned_levels()[1]
            .iter()
            .map(|table| table.descriptor().identity().as_str())
            .collect::<Vec<_>>(),
        vec![
            "compact-l1-existing-a",
            "compact-l0-middle",
            "compact-l1-existing-z"
        ]
    );
}

#[test]
fn branch_compaction_l0_to_l1_multi_table_rewrite_preserves_newest_row() {
    let branch = branch_id(173);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    let key = physical_key(branch, b"compact-l0-multi".to_vec());
    let newer = storage_row_with(
        branch,
        b"compact-l0-multi".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"newer".to_vec(),
    );
    let older = storage_row_with(
        branch,
        b"compact-l0-multi".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"older".to_vec(),
    );
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-l0-multi-newer",
            vec![newer.clone()],
        ))
        .expect("install newer input");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-l0-multi-older",
            vec![older.clone()],
        ))
        .expect("install older input");

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0ToLevelOne,
        "compact-l0-multi-output",
    )
    .expect("request");
    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    assert_eq!(
        candidate.operation(),
        BranchCompactionOperation::TableRewrite
    );
    assert!(candidate.requires_table_rewrite());
    assert_eq!(
        candidate.no_promotion_reason(),
        Some(BranchCompactionNoPromotionReason::MultipleInputTables)
    );
    assert_eq!(candidate.input_refs().len(), 2);
    assert!(candidate.overlap_refs().is_empty());
    assert_eq!(candidate.output_level(), BranchLevel::new(1));

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("compact inputs");
    let report = outcome.table_report().expect("report");
    assert_eq!(report.input_sources(), 2);
    assert_eq!(report.kept_rows(), 2);
    assert_eq!(state.owned_levels()[0].len(), 0);
    assert_eq!(state.owned_levels()[1].len(), 1);

    let after = state.capture_read_view().expect("after view");
    assert_visible_row(
        after.latest(&key).expect("latest").as_ref(),
        &newer,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );
    assert_visible_row(
        after
            .at_version(&key, CommitVersion::new(2))
            .expect("bounded")
            .as_ref(),
        &older,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );
}

#[test]
fn branch_compaction_l0_to_l1_selects_all_snapshot_inputs() {
    let branch = branch_id(247);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");

    for index in 0..6 {
        let identity = format!("compact-l0-bounded-{index}");
        let key = format!("compact-l0-bounded-key-{index}").into_bytes();
        let row = storage_row_with(
            branch,
            key,
            u64::try_from(index + 1).expect("index fits in u64"),
            u64::try_from(index + 1).expect("index fits in u64"),
            Timestamp::EPOCH,
            format!("value-{index}").into_bytes(),
        );
        state
            .install_l0_table(branch_owned_table(
                branch,
                BranchLevel::ZERO,
                &identity,
                vec![row],
            ))
            .expect("install l0 input");
    }

    assert_eq!(
        state.owned_levels()[0]
            .iter()
            .map(|table| table.descriptor().identity().as_str())
            .collect::<Vec<_>>(),
        vec![
            "compact-l0-bounded-5",
            "compact-l0-bounded-4",
            "compact-l0-bounded-3",
            "compact-l0-bounded-2",
            "compact-l0-bounded-1",
            "compact-l0-bounded-0",
        ]
    );

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0ToLevelOne,
        "compact-l0-bounded-output",
    )
    .expect("request");
    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    assert_eq!(
        candidate.operation(),
        BranchCompactionOperation::TableRewrite
    );
    assert!(candidate.requires_table_rewrite());
    assert_eq!(
        candidate.no_promotion_reason(),
        Some(BranchCompactionNoPromotionReason::MultipleInputTables)
    );
    assert_eq!(candidate.input_refs().len(), 6);
    assert_eq!(
        candidate
            .input_refs()
            .iter()
            .map(BranchTableRef::table_index)
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4, 5]
    );
    assert_eq!(
        candidate
            .input_refs()
            .iter()
            .map(|table_ref| table_ref.table_identity().as_str())
            .collect::<Vec<_>>(),
        vec![
            "compact-l0-bounded-5",
            "compact-l0-bounded-4",
            "compact-l0-bounded-3",
            "compact-l0-bounded-2",
            "compact-l0-bounded-1",
            "compact-l0-bounded-0",
        ]
    );

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("compact full snapshot");
    assert_eq!(outcome.removed_refs().len(), 6);
    assert_eq!(outcome.table_report().expect("report").input_sources(), 6);
    assert_eq!(state.owned_levels()[0].len(), 0);
    assert_eq!(state.owned_levels()[1].len(), 1);

    let after = state.capture_read_view().expect("after view");
    for index in 0..6 {
        let key = physical_key(
            branch,
            format!("compact-l0-bounded-key-{index}").into_bytes(),
        );
        assert!(after.latest(&key).expect("latest").is_some());
    }
}

#[test]
fn branch_compaction_l0_to_l1_includes_target_tables_inside_snapshot_span() {
    let branch = branch_id(248);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    let low = storage_row_with(
        branch,
        b"compact-l0-span-a".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"low".to_vec(),
    );
    let high = storage_row_with(
        branch,
        b"compact-l0-span-z".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"high".to_vec(),
    );
    let gap = storage_row_with(
        branch,
        b"compact-l0-span-m".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"gap".to_vec(),
    );
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "compact-l0-span-gap",
                vec![gap.clone()],
            ),
        )
        .expect("install target gap");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-l0-span-high",
            vec![high.clone()],
        ))
        .expect("install high input");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-l0-span-low",
            vec![low.clone()],
        ))
        .expect("install low input");

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0ToLevelOne,
        "compact-l0-span-output",
    )
    .expect("request");
    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    assert_eq!(candidate.input_refs().len(), 2);
    assert_eq!(candidate.overlap_refs().len(), 1);
    assert_eq!(
        candidate.overlap_refs()[0].table_identity().as_str(),
        "compact-l0-span-gap"
    );

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("compact full span");
    let report = outcome.table_report().expect("report");
    assert_eq!(report.input_sources(), 3);
    assert_eq!(outcome.removed_refs().len(), 3);
    assert_eq!(state.owned_levels()[0].len(), 0);
    assert_eq!(state.owned_levels()[1].len(), 1);

    let after = state.capture_read_view().expect("after view");
    for row in [&low, &gap, &high] {
        assert_visible_row(
            after.latest(row.physical_key()).expect("latest").as_ref(),
            row,
            BranchRowSource::OwnedTable {
                level: BranchLevel::new(1),
                table_index: 0,
            },
        );
    }
}

#[test]
fn branch_compaction_l0_to_l1_preserves_point_scan_history_and_timestamp_reads() {
    let branch = branch_id(249);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    let live_key = physical_key(branch, b"compact-l0-wide-live".to_vec());
    let delete_key = physical_key(branch, b"compact-l0-wide-delete".to_vec());
    let prefix_key = physical_key(branch, b"compact-l0-wide-".to_vec());
    let gap = storage_row_with(
        branch,
        b"compact-l0-wide-gap".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"gap".to_vec(),
    );
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "compact-l0-wide-gap",
                vec![gap],
            ),
        )
        .expect("install gap table");

    let l0_rows = [
        (
            "compact-l0-wide-live-new",
            storage_row_with(
                branch,
                b"compact-l0-wide-live".to_vec(),
                8,
                80,
                Timestamp::EPOCH,
                b"live-new".to_vec(),
            ),
        ),
        (
            "compact-l0-wide-live-old",
            storage_row_with(
                branch,
                b"compact-l0-wide-live".to_vec(),
                3,
                30,
                Timestamp::EPOCH,
                b"live-old".to_vec(),
            ),
        ),
        (
            "compact-l0-wide-delete-tombstone",
            tombstone_row(branch, b"compact-l0-wide-delete".to_vec(), 7, 70),
        ),
        (
            "compact-l0-wide-delete-old",
            storage_row_with(
                branch,
                b"compact-l0-wide-delete".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"delete-old".to_vec(),
            ),
        ),
        (
            "compact-l0-wide-scan-a",
            storage_row_with(
                branch,
                b"compact-l0-wide-scan-a".to_vec(),
                4,
                40,
                Timestamp::EPOCH,
                b"scan-a".to_vec(),
            ),
        ),
        (
            "compact-l0-wide-scan-z",
            storage_row_with(
                branch,
                b"compact-l0-wide-scan-z".to_vec(),
                5,
                50,
                Timestamp::EPOCH,
                b"scan-z".to_vec(),
            ),
        ),
    ];
    for (identity, row) in l0_rows {
        state
            .install_l0_table(branch_owned_table(
                branch,
                BranchLevel::ZERO,
                identity,
                vec![row],
            ))
            .expect("install l0 input");
    }

    let before = state.capture_read_view().expect("before view");
    let before_latest = visible_storage_row(before.latest(&live_key).expect("before latest"));
    let before_version = visible_storage_row(
        before
            .at_version(&live_key, CommitVersion::new(3))
            .expect("before version"),
    );
    let before_timestamp = visible_storage_row(
        before
            .read_point(
                &live_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(30)),
            )
            .expect("before timestamp"),
    );
    let before_delete_latest =
        visible_storage_row(before.latest(&delete_key).expect("before delete latest"));
    let before_delete_history = history_versions(
        &before
            .history(&delete_key, BranchHistoryOptions::all())
            .expect("before delete history"),
    );
    let before_latest_scan = scan_user_keys(
        &before
            .scan_prefix(
                &BranchScanBounds::prefix(&prefix_key),
                BranchReadBound::latest(),
            )
            .expect("before latest scan"),
    );
    let before_timestamp_scan = scan_user_keys(
        &before
            .scan_prefix(
                &BranchScanBounds::prefix(&prefix_key),
                BranchReadBound::at_timestamp(Timestamp::from_micros(45)),
            )
            .expect("before timestamp scan"),
    );

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0ToLevelOne,
        "compact-l0-wide-output",
    )
    .expect("request");
    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    assert_eq!(candidate.input_refs().len(), 6);
    assert_eq!(candidate.overlap_refs().len(), 1);

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("compact wide l0 episode");
    let report = outcome.table_report().expect("report");
    assert_eq!(report.input_sources(), 7);
    assert_eq!(outcome.removed_refs().len(), 7);
    assert_eq!(state.owned_levels()[0].len(), 0);

    let after = state.capture_read_view().expect("after view");
    assert_eq!(
        visible_storage_row(after.latest(&live_key).expect("after latest")),
        before_latest
    );
    assert_eq!(
        visible_storage_row(
            after
                .at_version(&live_key, CommitVersion::new(3))
                .expect("after version")
        ),
        before_version
    );
    assert_eq!(
        visible_storage_row(
            after
                .read_point(
                    &live_key,
                    BranchReadBound::at_timestamp(Timestamp::from_micros(30)),
                )
                .expect("after timestamp")
        ),
        before_timestamp
    );
    assert_eq!(
        visible_storage_row(after.latest(&delete_key).expect("after delete latest")),
        before_delete_latest
    );
    assert_eq!(
        history_versions(
            &after
                .history(&delete_key, BranchHistoryOptions::all())
                .expect("after delete history")
        ),
        before_delete_history
    );
    assert_eq!(
        scan_user_keys(
            &after
                .scan_prefix(
                    &BranchScanBounds::prefix(&prefix_key),
                    BranchReadBound::latest(),
                )
                .expect("after latest scan")
        ),
        before_latest_scan
    );
    assert_eq!(
        scan_user_keys(
            &after
                .scan_prefix(
                    &BranchScanBounds::prefix(&prefix_key),
                    BranchReadBound::at_timestamp(Timestamp::from_micros(45)),
                )
                .expect("after timestamp scan")
        ),
        before_timestamp_scan
    );
}

#[test]
fn branch_compaction_l0_to_l1_prepared_plan_publishes_around_concurrent_flush() {
    let branch = branch_id(253);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    let mut planned_rows = Vec::new();
    for index in 0..5 {
        let row = storage_row_with(
            branch,
            b"compact-l0-stale-wide-key".to_vec(),
            u64::try_from(index + 1).expect("index fits in u64"),
            u64::try_from(index + 1).expect("index fits in u64"),
            Timestamp::EPOCH,
            format!("value-{index}").into_bytes(),
        );
        planned_rows.push(row.clone());
        state
            .install_l0_table(branch_owned_table(
                branch,
                BranchLevel::ZERO,
                &format!("compact-l0-stale-wide-{index}"),
                vec![row],
            ))
            .expect("install input");
    }
    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0ToLevelOne,
        "compact-l0-stale-wide-output",
    )
    .expect("request");
    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    assert_eq!(candidate.input_refs().len(), 5);
    let (artifacts, report) = state
        .prepare_branch_compaction_plan(&request, &plan)
        .expect("prepare")
        .expect("prepared output");
    let output_tables = state
        .compaction_output_tables(candidate.output_level(), artifacts, None)
        .expect("output tables");

    let new_row = storage_row_with(
        branch,
        b"compact-l0-stale-wide-new".to_vec(),
        99,
        99,
        Timestamp::EPOCH,
        b"new".to_vec(),
    );
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-l0-stale-wide-new",
            vec![new_row.clone()],
        ))
        .expect("install concurrent flush");

    let outcome = state
        .install_branch_compaction_prepared_plan(&request, &plan, output_tables, report)
        .expect("prepared plan publishes around concurrent flush");
    assert_eq!(outcome.removed_refs().len(), 5);
    assert_eq!(
        outcome
            .table_report()
            .expect("table report")
            .input_sources(),
        5
    );
    assert_eq!(state.owned_levels()[0].len(), 1);
    assert_eq!(
        state.owned_levels()[0][0].descriptor().identity().as_str(),
        "compact-l0-stale-wide-new"
    );
    assert!(!state.owned_levels()[1].is_empty());

    let after = state.capture_read_view().expect("after view");
    assert_visible_row(
        after
            .latest(&physical_key(branch, b"compact-l0-stale-wide-new".to_vec()))
            .expect("new latest")
            .as_ref(),
        &new_row,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    let planned_key = physical_key(branch, b"compact-l0-stale-wide-key".to_vec());
    assert_visible_row(
        after.latest(&planned_key).expect("planned latest").as_ref(),
        planned_rows.last().expect("planned latest row"),
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );
    for row in &planned_rows {
        assert_visible_row(
            after
                .at_version(&planned_key, row.commit_version())
                .expect("planned version")
                .as_ref(),
            row,
            BranchRowSource::OwnedTable {
                level: BranchLevel::new(1),
                table_index: 0,
            },
        );
    }
    assert_eq!(
        history_versions(
            &after
                .history(&planned_key, BranchHistoryOptions::all())
                .expect("planned history")
        ),
        vec![5, 4, 3, 2, 1]
    );
    assert_eq!(
        scan_user_keys(
            &after
                .scan_prefix(
                    &BranchScanBounds::prefix(&planned_key),
                    BranchReadBound::latest(),
                )
                .expect("planned scan")
        ),
        vec![b"compact-l0-stale-wide-key".to_vec()]
    );
}

#[test]
fn branch_compaction_l0_to_l1_multi_table_rewrite_includes_overlapping_target() {
    let branch = branch_id(175);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    let overlap_key = physical_key(branch, b"compact-l0-target-b".to_vec());
    let l0_overlap = storage_row_with(
        branch,
        b"compact-l0-target-b".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"newer".to_vec(),
    );
    let l0_other = storage_row_with(
        branch,
        b"compact-l0-target-c".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"other".to_vec(),
    );
    let l1_overlap = storage_row_with(
        branch,
        b"compact-l0-target-b".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"base".to_vec(),
    );
    let l1_preserved = storage_row_with(
        branch,
        b"compact-l0-target-z".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"preserved".to_vec(),
    );
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "compact-l0-target-overlap",
                vec![l1_overlap.clone()],
            ),
        )
        .expect("install overlapping target");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "compact-l0-target-preserved",
                vec![l1_preserved],
            ),
        )
        .expect("install preserved target");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-l0-target-newer",
            vec![l0_overlap.clone()],
        ))
        .expect("install overlapping input");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-l0-target-other",
            vec![l0_other.clone()],
        ))
        .expect("install other input");

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0ToLevelOne,
        "compact-l0-target-output",
    )
    .expect("request");
    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    assert_eq!(
        candidate.operation(),
        BranchCompactionOperation::TableRewrite
    );
    assert_eq!(
        candidate.no_promotion_reason(),
        Some(BranchCompactionNoPromotionReason::TargetLevelOverlap)
    );
    assert_eq!(candidate.input_refs().len(), 2);
    assert_eq!(candidate.overlap_refs().len(), 1);

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("compact inputs");
    let report = outcome.table_report().expect("report");
    assert_eq!(report.input_sources(), 3);
    assert_eq!(state.owned_levels()[0].len(), 0);
    assert_eq!(state.owned_levels()[1].len(), 2);
    assert!(state.owned_levels()[1]
        .iter()
        .any(|table| table.descriptor().identity().as_str() == "compact-l0-target-preserved"));

    let after = state.capture_read_view().expect("after view");
    assert_visible_row(
        after.latest(&overlap_key).expect("latest").as_ref(),
        &l0_overlap,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );
    assert_visible_row(
        after
            .at_version(&overlap_key, CommitVersion::new(1))
            .expect("bounded")
            .as_ref(),
        &l1_overlap,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );
}

#[test]
fn branch_compaction_l0_to_l1_multi_table_rewrite_preserves_newest_tombstone() {
    let branch = branch_id(176);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    let key = physical_key(branch, b"compact-l0-delete".to_vec());
    let older = storage_row_with(
        branch,
        b"compact-l0-delete".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"older".to_vec(),
    );
    let tombstone = tombstone_row(branch, b"compact-l0-delete".to_vec(), 5, 50);
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-l0-delete-tombstone",
            vec![tombstone.clone()],
        ))
        .expect("install tombstone");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-l0-delete-older",
            vec![older.clone()],
        ))
        .expect("install older row");

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0ToLevelOne,
        "compact-l0-delete-output",
    )
    .expect("request");
    state
        .compact_branch_owned_tables(&request)
        .expect("compact inputs");

    let after = state.capture_read_view().expect("after view");
    assert_eq!(after.latest(&key).expect("latest"), None);
    assert_visible_row(
        after
            .at_version(&key, CommitVersion::new(2))
            .expect("bounded")
            .as_ref(),
        &older,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );
    assert_eq!(
        history_versions(
            &after
                .history(&key, BranchHistoryOptions::all())
                .expect("history")
        ),
        vec![5, 2]
    );
}

#[test]
fn branch_compaction_l0_to_l1_promotion_plan_publishes_around_concurrent_flush() {
    let branch = branch_id(174);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    let planned_row = storage_row_with(
        branch,
        b"compact-l0-planned".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"planned".to_vec(),
    );
    let new_row = storage_row_with(
        branch,
        b"compact-l0-new".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"new".to_vec(),
    );
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-l0-planned",
            vec![planned_row.clone()],
        ))
        .expect("install planned input");
    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0ToLevelOne,
        "compact-l0-stale-output",
    )
    .expect("request");
    let plan = state.plan_branch_compaction(&request).expect("plan");
    assert!(plan.is_metadata_promotion());

    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-l0-new",
            vec![new_row.clone()],
        ))
        .expect("install new input");

    let outcome = state
        .install_branch_compaction_plan(&request, &plan)
        .expect("promotion publishes around concurrent flush");
    assert_eq!(outcome.table_report(), None);
    assert_eq!(outcome.removed_refs().len(), 1);
    assert_eq!(
        state.owned_levels()[0]
            .iter()
            .map(|table| table.descriptor().identity().as_str())
            .collect::<Vec<_>>(),
        vec!["compact-l0-new"]
    );
    assert_eq!(
        state.owned_levels()[1]
            .iter()
            .map(|table| table.descriptor().identity().as_str())
            .collect::<Vec<_>>(),
        vec!["compact-l0-planned"]
    );

    let after = state.capture_read_view().expect("after view");
    assert_visible_row(
        after
            .latest(&physical_key(branch, b"compact-l0-new".to_vec()))
            .expect("new latest")
            .as_ref(),
        &new_row,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_visible_row(
        after
            .latest(&physical_key(branch, b"compact-l0-planned".to_vec()))
            .expect("planned latest")
            .as_ref(),
        &planned_row,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn branch_compaction_nonzero_level_promotes_overlapping_tables_only() {
    let branch = branch_id(124);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    let compacted_key = physical_key(branch, b"compact-l2-b".to_vec());
    let preserved_key = physical_key(branch, b"compact-l2-z".to_vec());
    let l1_row = storage_row_with(
        branch,
        b"compact-l2-b".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"l1".to_vec(),
    );
    let overlapping_l2 = storage_row_with(
        branch,
        b"compact-l2-b".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"l2".to_vec(),
    );
    let non_overlapping_l2 = storage_row_with(
        branch,
        b"compact-l2-z".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"z".to_vec(),
    );
    state
        .install_owned_table_at_level(
            BranchLevel::new(2),
            branch_owned_table(
                branch,
                BranchLevel::new(2),
                "compact-l2-overlap",
                vec![overlapping_l2.clone()],
            ),
        )
        .expect("install overlapping l2");
    state
        .install_owned_table_at_level(
            BranchLevel::new(2),
            branch_owned_table(
                branch,
                BranchLevel::new(2),
                "compact-l2-preserved",
                vec![non_overlapping_l2.clone()],
            ),
        )
        .expect("install non-overlapping l2");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "compact-l1-input",
                vec![l1_row.clone()],
            ),
        )
        .expect("install l1 input");

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::new(1),
            table_index: 0,
        },
        "compact-l1-to-l2-output",
    )
    .expect("request");
    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    assert_eq!(candidate.input_refs().len(), 1);
    assert_eq!(candidate.overlap_refs().len(), 1);
    assert_eq!(
        candidate.no_promotion_reason(),
        Some(BranchCompactionNoPromotionReason::TargetLevelOverlap)
    );
    assert_eq!(candidate.output_level(), BranchLevel::new(2));
    assert!(candidate.bottommost_for_branch());

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("compact l1 to l2");
    assert_eq!(outcome.removed_refs().len(), 2);
    assert_eq!(outcome.output_refs().len(), 1);
    assert_eq!(state.owned_levels()[1].len(), 0);
    assert_eq!(state.owned_levels()[2].len(), 2);

    let after = state.capture_read_view().expect("after view");
    assert_visible_row(
        after
            .latest(&compacted_key)
            .expect("compacted latest")
            .as_ref(),
        &l1_row,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(2),
            table_index: 0,
        },
    );
    assert_visible_row(
        after
            .latest(&preserved_key)
            .expect("preserved latest")
            .as_ref(),
        &non_overlapping_l2,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(2),
            table_index: 1,
        },
    );
}

#[test]
fn branch_compaction_nonzero_level_expands_source_run_for_target_overlap() {
    let branch = branch_id(0x7d);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(4, 64, 32).expect("branch config"),
    )
    .expect("state");
    let first_key = physical_key(branch, b"connected-overlap-a".to_vec());
    let second_key = physical_key(branch, b"connected-overlap-b".to_vec());
    let preserved_key = physical_key(branch, b"connected-overlap-z".to_vec());
    let first_newer = storage_row_with(
        branch,
        b"connected-overlap-a".to_vec(),
        10,
        10_000,
        Timestamp::EPOCH,
        b"first-newer".to_vec(),
    );
    let second_newer = storage_row_with(
        branch,
        b"connected-overlap-b".to_vec(),
        11,
        11_000,
        Timestamp::EPOCH,
        b"second-newer".to_vec(),
    );
    let preserved = storage_row_with(
        branch,
        b"connected-overlap-z".to_vec(),
        12,
        12_000,
        Timestamp::EPOCH,
        b"preserved".to_vec(),
    );
    let first_older = storage_row_with(
        branch,
        b"connected-overlap-a".to_vec(),
        1,
        1_000,
        Timestamp::EPOCH,
        b"first-older".to_vec(),
    );
    let second_older = storage_row_with(
        branch,
        b"connected-overlap-b".to_vec(),
        2,
        2_000,
        Timestamp::EPOCH,
        b"second-older".to_vec(),
    );
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "connected-overlap-source-a",
                vec![first_newer.clone()],
            ),
        )
        .expect("install first source");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "connected-overlap-source-b",
                vec![second_newer.clone()],
            ),
        )
        .expect("install second source");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "connected-overlap-source-z",
                vec![preserved.clone()],
            ),
        )
        .expect("install preserved source");
    state
        .install_owned_table_at_level(
            BranchLevel::new(2),
            branch_owned_table(
                branch,
                BranchLevel::new(2),
                "connected-overlap-target",
                vec![first_older, second_older],
            ),
        )
        .expect("install target");

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::new(1),
            table_index: 0,
        },
        "connected-overlap-output",
    )
    .expect("request");
    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    assert_eq!(candidate.input_refs().len(), 2);
    assert_eq!(candidate.overlap_refs().len(), 1);
    assert_eq!(candidate.source_count(), 3);
    assert_eq!(
        candidate.no_promotion_reason(),
        Some(BranchCompactionNoPromotionReason::TargetLevelOverlap)
    );

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("compact connected overlap");
    assert_eq!(outcome.removed_refs().len(), 3);
    assert_eq!(state.owned_levels()[1].len(), 1);
    assert_eq!(state.owned_levels()[2].len(), 1);

    let after = state.capture_read_view().expect("after view");
    assert_visible_row(
        after.latest(&first_key).expect("first latest").as_ref(),
        &first_newer,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(2),
            table_index: 0,
        },
    );
    assert_visible_row(
        after.latest(&second_key).expect("second latest").as_ref(),
        &second_newer,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(2),
            table_index: 0,
        },
    );
    assert_visible_row(
        after
            .latest(&preserved_key)
            .expect("preserved latest")
            .as_ref(),
        &preserved,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );
}

#[test]
fn branch_compaction_nonzero_no_overlap_promotes_table_metadata() {
    let branch = branch_id(166);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    let row = storage_row_with(
        branch,
        b"promote-empty-target".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"value".to_vec(),
    );
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "promote-empty-target",
                vec![row.clone()],
            ),
        )
        .expect("install input");
    let input_facts = state.owned_levels()[1][0].facts().clone();

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::new(1),
            table_index: 0,
        },
        "promote-empty-target-output",
    )
    .expect("request");
    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    assert_eq!(
        candidate.operation(),
        BranchCompactionOperation::MetadataPromotion
    );
    assert!(candidate.is_metadata_promotion());
    assert!(!candidate.requires_table_rewrite());
    assert_eq!(candidate.no_promotion_reason(), None);
    assert_eq!(candidate.input_refs().len(), 1);
    assert!(candidate.overlap_refs().is_empty());
    assert_eq!(candidate.output_level(), BranchLevel::new(2));
    assert!(state
        .prepare_branch_compaction_plan(&request, &plan)
        .expect("prepare promotion")
        .is_none());

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("promote table");

    assert_eq!(outcome.output_refs().len(), 1);
    assert_eq!(outcome.removed_refs().len(), 1);
    assert_eq!(outcome.table_report(), None);
    assert_eq!(outcome.removed_refs()[0].level(), BranchLevel::new(1));
    assert_eq!(outcome.output_refs()[0].level(), BranchLevel::new(2));
    assert_eq!(
        outcome.removed_refs()[0].table_identity(),
        outcome.output_refs()[0].table_identity()
    );
    assert_eq!(state.owned_levels()[1].len(), 0);
    assert_eq!(state.owned_levels()[2].len(), 1);
    let promoted = &state.owned_levels()[2][0];
    assert_eq!(promoted.level(), BranchLevel::new(2));
    assert_eq!(promoted.facts(), &input_facts);
    assert_eq!(promoted.rows(), &[TableRow::new(row.clone())]);
    assert_eq!(
        state
            .capture_read_view()
            .expect("view")
            .latest(&physical_key(branch, b"promote-empty-target".to_vec()))
            .expect("latest")
            .expect("visible")
            .row(),
        &row
    );
}

#[test]
fn branch_compaction_nonzero_no_overlap_with_pruning_rewrites_table() {
    let branch = branch_id(173);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    let key = physical_key(branch, b"rewrite-for-pruning".to_vec());
    let newest = storage_row_with(
        branch,
        b"rewrite-for-pruning".to_vec(),
        9,
        90,
        Timestamp::EPOCH,
        b"newest".to_vec(),
    );
    let retained = storage_row_with(
        branch,
        b"rewrite-for-pruning".to_vec(),
        6,
        60,
        Timestamp::EPOCH,
        b"retained".to_vec(),
    );
    let floor_survivor = storage_row_with(
        branch,
        b"rewrite-for-pruning".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"floor-survivor".to_vec(),
    );
    let dropped = storage_row_with(
        branch,
        b"rewrite-for-pruning".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"dropped".to_vec(),
    );
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "rewrite-for-pruning-input",
                vec![
                    newest.clone(),
                    retained.clone(),
                    floor_survivor.clone(),
                    dropped,
                ],
            ),
        )
        .expect("install input");
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = BranchCompactionPruningProof::from_branch_state(&state, CommitVersion::new(5))
        .expect("proof")
        .with_retained_timestamp_floor(Timestamp::from_micros(50))
        .expect("timestamp proof")
        .with_no_readable_inherited_layers()
        .expect("inheritance proof")
        .with_candidate_tables_not_shared()
        .expect("shared-table proof")
        .with_recovery_health(BranchRecoveryHealthAttestation::Healthy)
        .expect("recovery health proof");
    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::new(1),
            table_index: 0,
        },
        "rewrite-for-pruning-output",
    )
    .expect("request")
    .with_retention_policy(BranchCompactionRetentionPolicy::DropOlderVersions)
    .with_pruning_proof(proof);

    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    assert_eq!(
        candidate.operation(),
        BranchCompactionOperation::TableRewrite
    );
    assert!(candidate.requires_table_rewrite());
    assert_eq!(
        candidate.no_promotion_reason(),
        Some(BranchCompactionNoPromotionReason::RowPruningRequested)
    );
    assert_eq!(candidate.input_refs().len(), 1);
    assert!(candidate.overlap_refs().is_empty());
    assert_eq!(candidate.output_level(), BranchLevel::new(2));
    assert!(state
        .prepare_branch_compaction_plan(&request, &plan)
        .expect("prepare rewrite")
        .is_some());

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("compact with pruning");
    let report = outcome.table_report().expect("rewrite report");
    assert_eq!(report.dropped_rows(), 1);
    assert_eq!(state.owned_levels()[1].len(), 0);
    assert_eq!(state.owned_levels()[2].len(), 1);

    let after = state.capture_read_view().expect("after");
    assert_visible_row(
        after.latest(&key).expect("latest").as_ref(),
        &newest,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(2),
            table_index: 0,
        },
    );
    assert_visible_row(
        after
            .at_version(&key, CommitVersion::new(4))
            .expect("floor survivor")
            .as_ref(),
        &floor_survivor,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(2),
            table_index: 0,
        },
    );
}

#[test]
fn branch_compaction_nonzero_promotion_respects_deeper_overlap_budget() {
    let branch = branch_id(174);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(4, 64, 32).expect("branch config"),
    )
    .expect("state");
    let key = physical_key(branch, b"promotion-budget".to_vec());
    let input = storage_row_with(
        branch,
        b"promotion-budget".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        b"input".to_vec(),
    );
    let deeper = storage_row_with(
        branch,
        b"promotion-budget".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"deeper-overlap".to_vec(),
    );
    state
        .install_owned_table_at_level(
            BranchLevel::new(3),
            branch_owned_table(
                branch,
                BranchLevel::new(3),
                "promotion-budget-deeper",
                vec![deeper],
            ),
        )
        .expect("install deeper table");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "promotion-budget-input",
                vec![input.clone()],
            ),
        )
        .expect("install input");

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::new(1),
            table_index: 0,
        },
        "promotion-budget-output",
    )
    .expect("request")
    .with_table_compaction_config(TableCompactionConfig::new(1, 16).expect("tiny target"));
    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    assert_eq!(
        candidate.operation(),
        BranchCompactionOperation::TableRewrite
    );
    assert_eq!(
        candidate.no_promotion_reason(),
        Some(BranchCompactionNoPromotionReason::DeeperLevelOverlapBudgetExceeded)
    );
    assert_eq!(candidate.input_refs().len(), 1);
    assert!(candidate.overlap_refs().is_empty());
    assert_eq!(candidate.output_level(), BranchLevel::new(2));

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("rewrite input");
    assert_eq!(outcome.removed_refs().len(), 1);
    assert_eq!(outcome.output_refs().len(), 1);
    assert!(outcome.table_report().is_some());
    assert_eq!(state.owned_levels()[1].len(), 0);
    assert_eq!(state.owned_levels()[2].len(), 1);
    assert_eq!(state.owned_levels()[3].len(), 1);

    let after = state.capture_read_view().expect("after");
    assert_visible_row(
        after.latest(&key).expect("latest").as_ref(),
        &input,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(2),
            table_index: 0,
        },
    );
}

#[test]
fn branch_compaction_nonzero_promotion_allows_deeper_overlap_within_budget() {
    let branch = branch_id(178);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(4, 64, 32).expect("branch config"),
    )
    .expect("state");
    let key = physical_key(branch, b"promotion-budget-allowed".to_vec());
    let input = storage_row_with(
        branch,
        b"promotion-budget-allowed".to_vec(),
        8,
        80,
        Timestamp::EPOCH,
        b"input".to_vec(),
    );
    let deeper = storage_row_with(
        branch,
        b"promotion-budget-allowed".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"deeper-overlap".to_vec(),
    );
    state
        .install_owned_table_at_level(
            BranchLevel::new(3),
            branch_owned_table(
                branch,
                BranchLevel::new(3),
                "promotion-budget-allowed-deeper",
                vec![deeper.clone()],
            ),
        )
        .expect("install deeper table");
    let deeper_bytes = state.owned_levels()[3][0].facts().byte_count();
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "promotion-budget-allowed-input",
                vec![input.clone()],
            ),
        )
        .expect("install input");
    let target_output_bytes = deeper_bytes.div_ceil(10);

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::new(1),
            table_index: 0,
        },
        "promotion-budget-allowed-output",
    )
    .expect("request")
    .with_table_compaction_config(
        TableCompactionConfig::new(target_output_bytes, 16).expect("boundary target"),
    );
    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    assert_eq!(
        candidate.operation(),
        BranchCompactionOperation::MetadataPromotion
    );
    assert_eq!(candidate.no_promotion_reason(), None);
    assert_eq!(candidate.input_refs().len(), 1);
    assert!(candidate.overlap_refs().is_empty());
    assert_eq!(candidate.output_level(), BranchLevel::new(2));

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("promote input");
    assert_eq!(outcome.table_report(), None);
    assert_eq!(outcome.removed_refs().len(), 1);
    assert_eq!(outcome.output_refs().len(), 1);
    assert_eq!(state.owned_levels()[1].len(), 0);
    assert_eq!(state.owned_levels()[2].len(), 1);
    assert_eq!(state.owned_levels()[3].len(), 1);

    let after = state.capture_read_view().expect("after");
    assert_visible_row(
        after.latest(&key).expect("latest").as_ref(),
        &input,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(2),
            table_index: 0,
        },
    );
    assert_visible_row(
        after
            .at_version(&key, CommitVersion::new(3))
            .expect("deeper version")
            .as_ref(),
        &deeper,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(3),
            table_index: 0,
        },
    );
}

#[test]
fn branch_compaction_nonzero_promotion_inserts_sorted() {
    let branch = branch_id(167);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    let promoted_row = storage_row_with(
        branch,
        b"promote-m".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"middle".to_vec(),
    );
    state
        .install_owned_table_at_level(
            BranchLevel::new(2),
            branch_owned_table(
                branch,
                BranchLevel::new(2),
                "promote-existing-a",
                vec![storage_row_with(
                    branch,
                    b"promote-a".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    b"first".to_vec(),
                )],
            ),
        )
        .expect("install first target table");
    state
        .install_owned_table_at_level(
            BranchLevel::new(2),
            branch_owned_table(
                branch,
                BranchLevel::new(2),
                "promote-existing-z",
                vec![storage_row_with(
                    branch,
                    b"promote-z".to_vec(),
                    2,
                    20,
                    Timestamp::EPOCH,
                    b"last".to_vec(),
                )],
            ),
        )
        .expect("install last target table");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "promote-middle",
                vec![promoted_row],
            ),
        )
        .expect("install input");

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::new(1),
            table_index: 0,
        },
        "promote-middle-output",
    )
    .expect("request");
    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("promote table");

    assert_eq!(outcome.table_report(), None);
    assert_eq!(state.owned_levels()[1].len(), 0);
    assert_eq!(
        state.owned_levels()[2]
            .iter()
            .map(|table| table.descriptor().identity().as_str())
            .collect::<Vec<_>>(),
        vec!["promote-existing-a", "promote-middle", "promote-existing-z"]
    );
}

#[test]
fn branch_compaction_nonzero_promotion_reaches_terminal_level() {
    let branch = branch_id(168);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(4, 64, 32).expect("branch config"),
    )
    .expect("state");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "promote-to-terminal",
                vec![storage_row_with(
                    branch,
                    b"promote-to-terminal".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    b"value".to_vec(),
                )],
            ),
        )
        .expect("install input");

    for (level, seed) in [
        (BranchLevel::new(1), "promote-terminal-first"),
        (BranchLevel::new(2), "promote-terminal-second"),
    ] {
        let request = BranchCompactionRequest::new(
            branch,
            BranchCompactionKind::CompactLevel {
                level,
                table_index: 0,
            },
            seed,
        )
        .expect("request");
        let outcome = state
            .compact_branch_owned_tables(&request)
            .expect("promote table");
        assert_eq!(outcome.table_report(), None);
    }

    assert_eq!(state.owned_levels()[1].len(), 0);
    assert_eq!(state.owned_levels()[2].len(), 0);
    assert_eq!(state.owned_levels()[3].len(), 1);
    assert_eq!(
        state.owned_levels()[3][0].descriptor().identity().as_str(),
        "promote-to-terminal"
    );

    let terminal_request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::new(3),
            table_index: 0,
        },
        "promote-terminal-noop",
    )
    .expect("request");
    assert_eq!(
        state
            .plan_branch_compaction(&terminal_request)
            .expect("terminal plan")
            .noop_reason(),
        Some(BranchCompactionNoopReason::LastLevel)
    );
}

#[test]
fn branch_compaction_bottommost_level_merges_configured_terminal_tables() {
    let branch = branch_id(0xb9);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    let terminal_level = BranchLevel::new(2);
    let mut rows = Vec::new();
    for index in 0_u64..4 {
        let row = storage_row_with(
            branch,
            format!("bottommost-merge-{index}").as_bytes().to_vec(),
            index + 1,
            (index + 1) * 1_000,
            Timestamp::EPOCH,
            format!("value-{index}").as_bytes().to_vec(),
        );
        state
            .install_owned_table_at_level(
                terminal_level,
                branch_owned_table(
                    branch,
                    terminal_level,
                    &format!("bottommost-merge-input-{index}"),
                    vec![row.clone()],
                ),
            )
            .expect("install terminal input");
        rows.push(row);
    }

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactBottommostLevel {
            level: terminal_level,
            start_table_index: 0,
            table_count: 4,
        },
        "bottommost-merge-output",
    )
    .expect("request");
    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    assert_eq!(
        candidate.operation(),
        BranchCompactionOperation::TableRewrite
    );
    assert_eq!(
        candidate.no_promotion_reason(),
        Some(BranchCompactionNoPromotionReason::MultipleInputTables)
    );
    assert_eq!(candidate.input_refs().len(), 4);
    assert!(candidate.overlap_refs().is_empty());
    assert_eq!(candidate.output_level(), terminal_level);
    assert!(candidate.bottommost_for_branch());

    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("compact terminal run");
    assert_eq!(
        outcome
            .table_report()
            .expect("table report")
            .input_sources(),
        4
    );
    assert_eq!(outcome.removed_refs().len(), 4);
    assert_eq!(outcome.output_refs().len(), 1);
    assert_eq!(state.owned_levels()[0].len(), 0);
    assert_eq!(state.owned_levels()[1].len(), 0);
    assert_eq!(state.owned_levels()[2].len(), 1);

    let after = state.capture_read_view().expect("after view");
    for row in rows {
        assert_visible_row(
            after.latest(row.physical_key()).expect("latest").as_ref(),
            &row,
            BranchRowSource::OwnedTable {
                level: terminal_level,
                table_index: 0,
            },
        );
    }
}

#[test]
fn branch_compaction_nonzero_promotion_preserves_materialization_source() {
    let source = branch_id(169);
    let child = branch_id(170);
    let materialization_source = BranchMaterializationSource::new(source, CommitVersion::new(7));
    let mut state = BranchLocalState::new(
        child,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    let row = storage_row_with(
        child,
        b"promote-replacement".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"value".to_vec(),
    );
    let reader = immutable_reader("promote-replacement", vec![row.clone()]);
    let descriptor = branch_table_descriptor(BranchLevel::new(1), &reader);
    let extras =
        crate::table::TableSummaryExtras::from_rows(reader.rows()).expect("table summary extras");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            BranchOwnedTable::new_materialization_replacement(
                child,
                descriptor,
                reader,
                extras,
                materialization_source,
            )
            .expect("replacement table"),
        )
        .expect("install replacement input");

    let request = BranchCompactionRequest::new(
        child,
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::new(1),
            table_index: 0,
        },
        "promote-replacement-output",
    )
    .expect("request");
    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("promote replacement");

    assert_eq!(outcome.table_report(), None);
    assert_eq!(state.owned_levels()[1].len(), 0);
    assert_eq!(state.owned_levels()[2].len(), 1);
    assert_eq!(
        state.owned_levels()[2][0].materialization_source(),
        Some(materialization_source)
    );
    assert_eq!(state.owned_levels()[2][0].rows(), &[TableRow::new(row)]);
}

#[test]
fn branch_compaction_noop_plans_are_explicit() {
    let branch = branch_id(125);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(2, 64, 32).expect("branch config"),
    )
    .expect("state");
    let empty_l0 =
        BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "noop-empty")
            .expect("request");
    assert_eq!(
        state
            .plan_branch_compaction(&empty_l0)
            .expect("empty plan")
            .noop_reason(),
        Some(BranchCompactionNoopReason::EmptyInputLevel)
    );

    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "noop-single",
            vec![storage_row_with(
                branch,
                b"noop-single".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"single".to_vec(),
            )],
        ))
        .expect("install single input");
    assert_eq!(
        state
            .compact_branch_owned_tables(&empty_l0)
            .expect("single noop")
            .noop_reason(),
        Some(BranchCompactionNoopReason::NotEnoughInputTables)
    );

    let last_level = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::new(1),
            table_index: 0,
        },
        "noop-last",
    )
    .expect("request");
    assert_eq!(
        state
            .plan_branch_compaction(&last_level)
            .expect("last level plan")
            .noop_reason(),
        Some(BranchCompactionNoopReason::LastLevel)
    );
}

#[test]
fn branch_compaction_invalid_requests_are_rejected_without_mutation() {
    let branch = branch_id(125);
    let state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(2, 64, 32).expect("branch config"),
    )
    .expect("state");

    let ambiguous_l0 = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
        "noop-ambiguous-l0",
    )
    .expect("request");
    assert!(matches!(
        state.plan_branch_compaction(&ambiguous_l0),
        Err(BranchRuntimeError::InvalidCompaction { .. })
    ));

    let outside_level = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::new(2),
            table_index: 0,
        },
        "noop-outside-level",
    )
    .expect("request");
    assert!(matches!(
        state.plan_branch_compaction(&outside_level),
        Err(BranchRuntimeError::InvalidCompaction { .. })
    ));

    let wrong_branch = BranchCompactionRequest::new(
        branch_id(126),
        BranchCompactionKind::CompactL0,
        "noop-wrong-branch",
    )
    .expect("request");
    assert!(matches!(
        state.plan_branch_compaction(&wrong_branch),
        Err(BranchRuntimeError::InvalidCompaction { .. })
    ));

    let mut nonzero_state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("nonzero config"),
    )
    .expect("nonzero state");
    nonzero_state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "noop-nonzero-existing",
                vec![storage_row_with(
                    branch,
                    b"noop-nonzero-existing".to_vec(),
                    2,
                    20,
                    Timestamp::EPOCH,
                    b"existing".to_vec(),
                )],
            ),
        )
        .expect("install nonzero table");
    let missing_table = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::new(1),
            table_index: 99,
        },
        "noop-missing-table",
    )
    .expect("request");
    assert!(matches!(
        nonzero_state.plan_branch_compaction(&missing_table),
        Err(BranchRuntimeError::InvalidCompaction { .. })
    ));
}

#[test]
fn branch_compaction_install_revalidates_stale_plan_before_mutation() {
    let branch = branch_id(126);
    let mut state = BranchLocalState::empty(branch);
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "stale-plan-a",
            vec![storage_row_with(
                branch,
                b"stale-a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"a".to_vec(),
            )],
        ))
        .expect("install a");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "stale-plan-b",
            vec![storage_row_with(
                branch,
                b"stale-b".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"b".to_vec(),
            )],
        ))
        .expect("install b");
    let request =
        BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "stale-output")
            .expect("request");
    let plan = state.plan_branch_compaction(&request).expect("plan");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "stale-plan-new-front",
            vec![storage_row_with(
                branch,
                b"stale-c".to_vec(),
                3,
                30,
                Timestamp::EPOCH,
                b"c".to_vec(),
            )],
        ))
        .expect("mutate after plan");
    let before_install = state.clone();

    let error = state
        .install_branch_compaction_plan(&request, &plan)
        .expect_err("stale plan rejected");
    assert!(matches!(
        error,
        BranchRuntimeError::InvalidCompaction { .. }
    ));
    assert_eq!(state, before_install);
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_compaction_rejects_stale_plan_before_source_open() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(245);
    let mut state = BranchLocalState::empty(branch);
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "stale-open-a",
            vec![storage_row_with(
                branch,
                b"stale-open-a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"a".to_vec(),
            )],
        ))
        .expect("install a");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "stale-open-b",
            vec![storage_row_with(
                branch,
                b"stale-open-b".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"b".to_vec(),
            )],
        ))
        .expect("install b");
    let request =
        BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "stale-open-output")
            .expect("request");
    let plan = state.plan_branch_compaction(&request).expect("plan");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "stale-open-c",
            vec![storage_row_with(
                branch,
                b"stale-open-c".to_vec(),
                3,
                30,
                Timestamp::EPOCH,
                b"c".to_vec(),
            )],
        ))
        .expect("mutate after plan");
    let before_install = state.clone();

    let error = state
        .install_branch_compaction_plan(&request, &plan)
        .expect_err("stale plan rejected");
    assert!(matches!(
        error,
        BranchRuntimeError::InvalidCompaction { .. }
    ));
    assert_eq!(state, before_install);

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.branch_compaction_source_opens(), 0);
    assert_eq!(perf.branch_compaction_peak_buffered_rows(), 0);
    assert_eq!(perf.table_compaction_peak_buffered_rows(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_compaction_rejects_missing_pruning_proof_before_source_open() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(246);
    let mut state = BranchLocalState::empty(branch);
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "prune-open-a",
            vec![storage_row_with(
                branch,
                b"prune-open-a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"a".to_vec(),
            )],
        ))
        .expect("install a");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "prune-open-b",
            vec![storage_row_with(
                branch,
                b"prune-open-b".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"b".to_vec(),
            )],
        ))
        .expect("install b");
    let before = state.clone();
    let request =
        BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "prune-open-output")
            .expect("request")
            .with_retention_policy(BranchCompactionRetentionPolicy::DropOlderVersions);

    let error = state
        .compact_branch_owned_tables(&request)
        .expect_err("missing proof rejected");
    assert!(matches!(
        error,
        BranchRuntimeError::InvalidCompaction {
            reason: BranchCompactionInvalidity::ProofMissing
        }
    ));
    assert_eq!(state, before);

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.branch_compaction_source_opens(), 0);
    assert_eq!(perf.branch_compaction_peak_buffered_rows(), 0);
    assert_eq!(perf.table_compaction_peak_buffered_rows(), 0);
}

#[test]
fn branch_compaction_rejects_output_identity_collision_without_mutation() {
    let branch = branch_id(127);
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(3, 64, 32).expect("branch config"),
    )
    .expect("state");
    let first = storage_row_with(
        branch,
        b"collision-a".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"a".to_vec(),
    );
    let second = storage_row_with(
        branch,
        b"collision-b".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"b".to_vec(),
    );
    let output_seed = TableIdentity::new("collision-output").expect("identity");
    let colliding_identity = expected_keep_all_compaction_output_identity(
        &output_seed,
        vec![
            (
                branch_compaction_source_id(
                    0,
                    BranchLevel::ZERO,
                    0,
                    "collision-l0-b",
                    std::slice::from_ref(&second),
                ),
                vec![second.clone()],
            ),
            (
                branch_compaction_source_id(
                    1,
                    BranchLevel::ZERO,
                    1,
                    "collision-l0-a",
                    std::slice::from_ref(&first),
                ),
                vec![first.clone()],
            ),
        ],
    );

    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                colliding_identity.as_str(),
                vec![storage_row_with(
                    branch,
                    b"collision-z".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    b"z".to_vec(),
                )],
            ),
        )
        .expect("install colliding survivor");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "collision-l0-a",
            vec![first],
        ))
        .expect("install first l0");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "collision-l0-b",
            vec![second],
        ))
        .expect("install second l0");
    let before = state.clone();
    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0ToLevelOne,
        output_seed.as_str(),
    )
    .expect("request");

    let error = state
        .compact_branch_owned_tables(&request)
        .expect_err("identity collision rejected");
    assert!(matches!(
        error,
        BranchRuntimeError::InvalidCompaction { .. }
    ));
    assert_eq!(state, before);
}

#[test]
fn branch_compaction_rejects_inherited_output_identity_collision_without_mutation() {
    let source = branch_id(137);
    let child = branch_id(138);
    let first = storage_row_with(
        child,
        b"collision-inherited-a".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"a".to_vec(),
    );
    let second = storage_row_with(
        child,
        b"collision-inherited-b".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"b".to_vec(),
    );
    let output_seed = TableIdentity::new("collision-inherited-output").expect("identity");
    let colliding_identity = expected_keep_all_compaction_output_identity(
        &output_seed,
        vec![
            (
                branch_compaction_source_id(
                    0,
                    BranchLevel::ZERO,
                    0,
                    "collision-inherited-l0-b",
                    std::slice::from_ref(&second),
                ),
                vec![second.clone()],
            ),
            (
                branch_compaction_source_id(
                    1,
                    BranchLevel::ZERO,
                    1,
                    "collision-inherited-l0-a",
                    std::slice::from_ref(&first),
                ),
                vec![first.clone()],
            ),
        ],
    );

    let mut source_state = BranchLocalState::empty(source);
    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            colliding_identity.as_str(),
            vec![storage_row_with(
                source,
                b"collision-inherited-source".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"source".to_vec(),
            )],
        ))
        .expect("install inherited colliding table");
    let (mut state, _) = source_state
        .fork_into_empty_child(child)
        .expect("fork child");
    state
        .install_l0_table(branch_owned_table(
            child,
            BranchLevel::ZERO,
            "collision-inherited-l0-a",
            vec![first],
        ))
        .expect("install first child table");
    state
        .install_l0_table(branch_owned_table(
            child,
            BranchLevel::ZERO,
            "collision-inherited-l0-b",
            vec![second],
        ))
        .expect("install second child table");
    let before = state.clone();

    let request =
        BranchCompactionRequest::new(child, BranchCompactionKind::CompactL0, output_seed.as_str())
            .expect("request");
    let error = state
        .compact_branch_owned_tables(&request)
        .expect_err("inherited identity collision rejected");
    assert!(matches!(
        error,
        BranchRuntimeError::InvalidCompaction { .. }
    ));
    assert_eq!(state, before);
}

#[test]
#[allow(clippy::too_many_lines)]
fn branch_compaction_candidates_exclude_mutable_and_inherited_sources() {
    let source = branch_id(128);
    let child = branch_id(129);
    let mut source_state = BranchLocalState::empty(source);
    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "compact-excluded-inherited",
            vec![storage_row_with(
                source,
                b"compact-excluded-inherited".to_vec(),
                4,
                40,
                Timestamp::EPOCH,
                b"inherited-secret".to_vec(),
            )],
        ))
        .expect("install inherited source");
    let (mut state, _) = source_state
        .fork_into_empty_child(child)
        .expect("fork child");
    state
        .append_committed_row(storage_row_with(
            child,
            b"compact-excluded-frozen".to_vec(),
            5,
            50,
            Timestamp::EPOCH,
            b"frozen-secret".to_vec(),
        ))
        .expect("append frozen source");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .append_committed_row(storage_row_with(
            child,
            b"compact-excluded-active".to_vec(),
            6,
            60,
            Timestamp::EPOCH,
            b"active-secret".to_vec(),
        ))
        .expect("append active source");
    state
        .install_l0_table(branch_owned_table(
            child,
            BranchLevel::ZERO,
            "compact-excluded-owned-a",
            vec![storage_row_with(
                child,
                b"compact-excluded-a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"owned-a-secret".to_vec(),
            )],
        ))
        .expect("install owned a");

    let request = BranchCompactionRequest::new(
        child,
        BranchCompactionKind::CompactL0,
        "compact-excluded-output",
    )
    .expect("request");
    assert_eq!(
        state
            .plan_branch_compaction(&request)
            .expect("single owned plan")
            .noop_reason(),
        Some(BranchCompactionNoopReason::NotEnoughInputTables)
    );

    state
        .install_l0_table(branch_owned_table(
            child,
            BranchLevel::ZERO,
            "compact-excluded-owned-b",
            vec![storage_row_with(
                child,
                b"compact-excluded-b".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"owned-b-secret".to_vec(),
            )],
        ))
        .expect("install owned b");
    let plan = state.plan_branch_compaction(&request).expect("owned plan");
    let candidate = plan.candidate().expect("candidate");
    assert_eq!(candidate.input_refs().len(), 2);
    assert_eq!(candidate.overlap_refs().len(), 0);
    assert!(candidate
        .input_refs()
        .iter()
        .all(|table_ref| matches!(table_ref.reference_kind(), BranchTableReferenceKind::Owned)));
    assert_eq!(
        candidate
            .input_refs()
            .iter()
            .map(|table_ref| table_ref.table_identity().as_str())
            .collect::<Vec<_>>(),
        vec!["compact-excluded-owned-b", "compact-excluded-owned-a"]
    );
    let candidate_debug = format!("{candidate:?}");
    for forbidden in [
        "active-secret",
        "frozen-secret",
        "inherited-secret",
        "owned-a-secret",
        "owned-b-secret",
    ] {
        assert!(!candidate_debug.contains(forbidden));
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn branch_compaction_keep_all_read_parity_covers_mutable_inherited_ttl_and_split_outputs() {
    let source = branch_id(130);
    let child = branch_id(131);
    let mut source_state = BranchLocalState::empty(source);
    let inherited_source = storage_row_with(
        source,
        b"compact-parity-inherited".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"inherited".to_vec(),
    );
    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "compact-parity-inherited",
            vec![inherited_source.clone()],
        ))
        .expect("install source table");
    let (mut state, _) = source_state
        .fork_into_empty_child(child)
        .expect("fork child");
    let inherited_child =
        rewrite_row_branch(&inherited_source, source, child).expect("rewrite inherited source");
    let frozen_row = storage_row_with(
        child,
        b"compact-parity-frozen".to_vec(),
        8,
        80,
        Timestamp::EPOCH,
        b"frozen".to_vec(),
    );
    let active_row = storage_row_with(
        child,
        b"compact-parity-active".to_vec(),
        9,
        90,
        Timestamp::EPOCH,
        b"active".to_vec(),
    );
    state
        .append_committed_row(frozen_row.clone())
        .expect("append frozen row");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .append_committed_row(active_row.clone())
        .expect("append active row");

    let live_key = physical_key(child, b"compact-parity-live".to_vec());
    let delete_key = physical_key(child, b"compact-parity-delete".to_vec());
    let ttl_key = physical_key(child, b"compact-parity-ttl".to_vec());
    let prefix_key = physical_key(child, b"compact-parity-scan-".to_vec());
    let range_low = physical_key(child, b"compact-parity-live".to_vec());
    let range_high = physical_key(child, vec![b'c', b'o', b'm', b'p', b'a', b'c', b't', 0xff]);
    let live_newer = storage_row_with(
        child,
        b"compact-parity-live".to_vec(),
        6,
        60,
        Timestamp::EPOCH,
        vec![0xff, 0x00],
    );
    let live_older = storage_row_with(
        child,
        b"compact-parity-live".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        Vec::new(),
    );
    let delete_old = storage_row_with(
        child,
        b"compact-parity-delete".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"deleted".to_vec(),
    );
    let delete_tombstone = StorageRow::tombstone(
        delete_key.clone(),
        CommitVersion::new(7),
        Timestamp::from_micros(70),
    );
    let ttl_row = storage_row_with(
        child,
        b"compact-parity-ttl".to_vec(),
        4,
        40,
        Timestamp::from_micros(60),
        b"ttl".to_vec(),
    );
    let scan_row = storage_row_with(
        child,
        [b"compact-parity-scan-".to_vec(), vec![0xff]].concat(),
        5,
        50,
        Timestamp::EPOCH,
        b"scan".to_vec(),
    );
    let bounded_scan_row = storage_row_with(
        child,
        b"compact-parity-range".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"range".to_vec(),
    );
    state
        .install_l0_table(branch_owned_table(
            child,
            BranchLevel::ZERO,
            "compact-parity-a",
            vec![
                live_newer.clone(),
                delete_tombstone,
                ttl_row.clone(),
                scan_row.clone(),
            ],
        ))
        .expect("install parity a");
    state
        .install_l0_table(branch_owned_table(
            child,
            BranchLevel::ZERO,
            "compact-parity-b",
            vec![
                live_older.clone(),
                delete_old.clone(),
                bounded_scan_row.clone(),
            ],
        ))
        .expect("install parity b");

    let before = state.capture_read_view().expect("before view");
    let before_latest = visible_storage_row(before.latest(&live_key).expect("before latest"));
    let before_version = visible_storage_row(
        before
            .at_version(&live_key, CommitVersion::new(2))
            .expect("before version"),
    );
    let before_timestamp = visible_storage_row(
        before
            .read_point(
                &live_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(20)),
            )
            .expect("before timestamp"),
    );
    let before_history_all = history_versions(
        &before
            .history(&delete_key, BranchHistoryOptions::all())
            .expect("before history all"),
    );
    let before_history_without_tombstones = history_versions(
        &before
            .history(
                &delete_key,
                BranchHistoryOptions::all().include_tombstones(false),
            )
            .expect("before history no tombstones"),
    );
    let before_prefix = scan_user_keys(
        &before
            .scan_prefix(
                &BranchScanBounds::prefix(&prefix_key),
                BranchReadBound::latest(),
            )
            .expect("before prefix"),
    );
    let before_range = scan_user_keys(
        &before
            .scan_range(
                &BranchScanBounds::closed(&range_low, &range_high).expect("range bounds"),
                BranchReadBound::latest(),
            )
            .expect("before range"),
    );
    let before_ttl_before_expiry = visible_storage_row(
        before
            .read_point(
                &ttl_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(59)),
            )
            .expect("before ttl 59"),
    );
    let before_ttl_at_expiry = visible_storage_row(
        before
            .read_point(
                &ttl_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(60)),
            )
            .expect("before ttl 60"),
    );
    let before_ttl_after_expiry = visible_storage_row(
        before
            .read_point(
                &ttl_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(61)),
            )
            .expect("before ttl 61"),
    );
    let inherited_key = physical_key(child, b"compact-parity-inherited".to_vec());
    assert_visible_row(
        before.latest(&inherited_key).expect("inherited").as_ref(),
        &inherited_child,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    assert_visible_row(
        before
            .latest(active_row.physical_key())
            .expect("active")
            .as_ref(),
        &active_row,
        BranchRowSource::Active,
    );
    assert_visible_row(
        before
            .latest(frozen_row.physical_key())
            .expect("frozen")
            .as_ref(),
        &frozen_row,
        BranchRowSource::Frozen { index: 0 },
    );

    let request = BranchCompactionRequest::new(
        child,
        BranchCompactionKind::CompactL0,
        "compact-parity-output",
    )
    .expect("request")
    .with_table_compaction_config(TableCompactionConfig::new(1, 16).expect("split config"));
    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("compact parity");
    let report = outcome.table_report().expect("report");
    assert_eq!(report.dropped_rows(), 0);
    assert!(report.split_count() > 0);
    assert!(outcome.output_refs().len() > 1);

    let after = state.capture_read_view().expect("after view");
    assert_eq!(
        visible_storage_row(after.latest(&live_key).expect("after latest")),
        before_latest
    );
    assert_eq!(
        visible_storage_row(
            after
                .at_version(&live_key, CommitVersion::new(2))
                .expect("after version")
        ),
        before_version
    );
    assert_eq!(
        visible_storage_row(
            after
                .read_point(
                    &live_key,
                    BranchReadBound::at_timestamp(Timestamp::from_micros(20)),
                )
                .expect("after timestamp")
        ),
        before_timestamp
    );
    assert_eq!(
        history_versions(
            &after
                .history(&delete_key, BranchHistoryOptions::all())
                .expect("after history all")
        ),
        before_history_all
    );
    assert_eq!(
        history_versions(
            &after
                .history(
                    &delete_key,
                    BranchHistoryOptions::all().include_tombstones(false),
                )
                .expect("after history no tombstones")
        ),
        before_history_without_tombstones
    );
    assert_eq!(
        scan_user_keys(
            &after
                .scan_prefix(
                    &BranchScanBounds::prefix(&prefix_key),
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
                    &BranchScanBounds::closed(&range_low, &range_high).expect("range bounds"),
                    BranchReadBound::latest(),
                )
                .expect("after range")
        ),
        before_range
    );
    assert_eq!(
        visible_storage_row(
            after
                .read_point(
                    &ttl_key,
                    BranchReadBound::at_timestamp(Timestamp::from_micros(59)),
                )
                .expect("after ttl 59")
        ),
        before_ttl_before_expiry
    );
    assert_eq!(
        visible_storage_row(
            after
                .read_point(
                    &ttl_key,
                    BranchReadBound::at_timestamp(Timestamp::from_micros(60)),
                )
                .expect("after ttl 60")
        ),
        before_ttl_at_expiry
    );
    assert_eq!(
        visible_storage_row(
            after
                .read_point(
                    &ttl_key,
                    BranchReadBound::at_timestamp(Timestamp::from_micros(61)),
                )
                .expect("after ttl 61")
        ),
        before_ttl_after_expiry
    );
    assert!(before_ttl_before_expiry.is_some());
    assert!(before_ttl_at_expiry.is_none());
    assert!(before_ttl_after_expiry.is_none());
    assert_visible_row(
        after.latest(&inherited_key).expect("inherited").as_ref(),
        &inherited_child,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
}

#[test]
fn branch_compaction_materialized_replacements_are_inputs_but_outputs_are_plain_owned_refs() {
    let source = branch_id(132);
    let child = branch_id(133);
    let mut state = BranchLocalState::empty(child);
    state
        .attach_inherited_layers(vec![branch_inherited_layer(
            source,
            CommitVersion::new(5),
            InheritedLayerStatus::Active,
            vec![vec![branch_owned_table(
                source,
                BranchLevel::ZERO,
                "compact-materialize-source",
                vec![storage_row_with(
                    source,
                    b"compact-materialized".to_vec(),
                    5,
                    50,
                    Timestamp::EPOCH,
                    b"source".to_vec(),
                )],
            )]],
        )])
        .expect("attach inherited");
    state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "compact-materialized")
                .expect("materialization request"),
        )
        .expect("materialize");
    assert!(matches!(
        state
            .reachability_snapshot()
            .expect("pre snapshot")
            .table_refs()[0]
            .reference_kind(),
        BranchTableReferenceKind::Replacement { .. }
    ));
    state
        .install_l0_table(branch_owned_table(
            child,
            BranchLevel::ZERO,
            "compact-materialize-owned",
            vec![storage_row_with(
                child,
                b"compact-owned".to_vec(),
                6,
                60,
                Timestamp::EPOCH,
                b"owned".to_vec(),
            )],
        ))
        .expect("install owned");

    let request = BranchCompactionRequest::new(
        child,
        BranchCompactionKind::CompactL0,
        "compact-materialize-output",
    )
    .expect("request");
    let plan = state.plan_branch_compaction(&request).expect("plan");
    let candidate = plan.candidate().expect("candidate");
    assert!(candidate.input_refs().iter().any(|table_ref| matches!(
        table_ref.reference_kind(),
        BranchTableReferenceKind::Replacement { .. }
    )));
    let outcome = state
        .install_branch_compaction_plan(&request, &plan)
        .expect("compact materialized replacement");
    assert!(outcome.removed_refs().iter().any(|table_ref| matches!(
        table_ref.reference_kind(),
        BranchTableReferenceKind::Replacement { .. }
    )));
    let snapshot = state.reachability_snapshot().expect("post snapshot");
    assert!(snapshot
        .table_refs()
        .iter()
        .all(|table_ref| table_ref.reference_kind() == BranchTableReferenceKind::Owned));
}

#[test]
fn branch_compaction_preserves_replacement_refs_for_single_materialization_source() {
    let source = branch_id(139);
    let child = branch_id(140);
    let materialization_source = BranchMaterializationSource::new(source, CommitVersion::new(7));
    let mut state = BranchLocalState::empty(child);
    for (identity, key, version) in [
        (
            "compact-replacement-a",
            b"compact-replacement-a".to_vec(),
            6,
        ),
        (
            "compact-replacement-b",
            b"compact-replacement-b".to_vec(),
            7,
        ),
    ] {
        let reader = immutable_reader(
            identity,
            vec![storage_row_with(
                child,
                key,
                version,
                version * 10,
                Timestamp::EPOCH,
                identity.as_bytes().to_vec(),
            )],
        );
        let descriptor = branch_table_descriptor(BranchLevel::ZERO, &reader);
        let extras = crate::table::TableSummaryExtras::from_rows(reader.rows())
            .expect("table summary extras");
        state
            .install_l0_table(
                BranchOwnedTable::new_materialization_replacement(
                    child,
                    descriptor,
                    reader,
                    extras,
                    materialization_source,
                )
                .expect("replacement table"),
            )
            .expect("install replacement input");
    }

    let request = BranchCompactionRequest::new(
        child,
        BranchCompactionKind::CompactL0,
        "compact-replacement-output",
    )
    .expect("request");
    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("compact replacements");

    assert!(outcome.output_refs().iter().all(|table_ref| matches!(
        table_ref.reference_kind(),
        BranchTableReferenceKind::Replacement {
            source_branch_id,
            fork_version,
        } if source_branch_id == source && fork_version == CommitVersion::new(7)
    )));
    let snapshot = state.reachability_snapshot().expect("post snapshot");
    assert!(snapshot.table_refs().iter().all(|table_ref| matches!(
        table_ref.reference_kind(),
        BranchTableReferenceKind::Replacement {
            source_branch_id,
            fork_version,
        } if source_branch_id == source && fork_version == CommitVersion::new(7)
    )));
}

#[test]
fn branch_compaction_build_failure_preserves_state_without_partial_outputs() {
    let branch = branch_id(134);
    let mut state = BranchLocalState::empty(branch);
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-fail-a",
            vec![storage_row_with(
                branch,
                b"compact-fail-a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"a".to_vec(),
            )],
        ))
        .expect("install a");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "compact-fail-b",
            vec![storage_row_with(
                branch,
                b"compact-fail-b".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"b".to_vec(),
            )],
        ))
        .expect("install b");
    let before = state.clone();
    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0,
        "compact-fail-output",
    )
    .expect("request")
    .with_table_compaction_config(TableCompactionConfig::new(1, 1).expect("failure config"));

    let error = state
        .compact_branch_owned_tables(&request)
        .expect_err("max-output failure");
    assert!(matches!(
        error,
        BranchRuntimeError::TableRuntime {
            source: crate::table::TableRuntimeError::InvalidRange {
                field: "max_output_tables"
            }
        }
    ));
    assert_eq!(state, before);
    assert_eq!(
        state
            .reachability_snapshot()
            .expect("post-failure snapshot")
            .table_refs()
            .iter()
            .map(|table_ref| table_ref.table_identity().as_str())
            .collect::<Vec<_>>(),
        vec!["compact-fail-a", "compact-fail-b"]
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn branch_compaction_release_facts_cover_shared_refs_disagreement_and_clear_outputs() {
    let parent = branch_id(135);
    let child = branch_id(136);
    let mut parent_state = BranchLocalState::empty(parent);
    parent_state
        .install_l0_table(branch_owned_table(
            parent,
            BranchLevel::ZERO,
            "compact-release-a",
            vec![storage_row_with(
                parent,
                b"compact-release-a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"a".to_vec(),
            )],
        ))
        .expect("install a");
    parent_state
        .install_l0_table(branch_owned_table(
            parent,
            BranchLevel::ZERO,
            "compact-release-b",
            vec![storage_row_with(
                parent,
                b"compact-release-b".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"b".to_vec(),
            )],
        ))
        .expect("install b");
    let (child_state, _) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    let child_snapshot = child_state.reachability_snapshot().expect("child snapshot");

    let request = BranchCompactionRequest::new(
        parent,
        BranchCompactionKind::CompactL0,
        "compact-release-output",
    )
    .expect("request");
    let outcome = parent_state
        .compact_branch_owned_tables(&request)
        .expect("compact parent");
    let parent_after = parent_state
        .reachability_snapshot()
        .expect("parent after snapshot");
    let aggregate_after_with_child =
        BranchReachabilityAggregate::from_snapshots(&[parent_after.clone(), child_snapshot])
            .expect("aggregate after with child");
    let stale_runtime_registry = SharedTableRegistry::new();
    let disagreement = BranchReleasePlan::from_removed_refs(
        parent,
        outcome.removed_refs().to_vec(),
        &aggregate_after_with_child,
        Some(&stale_runtime_registry),
    )
    .expect("disagreement release");
    assert_eq!(
        disagreement
            .protected_tables()
            .iter()
            .map(BranchProtectedTable::reason)
            .collect::<Vec<_>>(),
        vec![
            BranchProtectionReason::RegistryDisagreement,
            BranchProtectionReason::RegistryDisagreement,
        ]
    );
    let durable_protected = BranchReleasePlan::from_removed_refs(
        parent,
        outcome.removed_refs().to_vec(),
        &aggregate_after_with_child,
        None,
    )
    .expect("durable protected release");
    assert_eq!(
        durable_protected
            .protected_tables()
            .iter()
            .map(BranchProtectedTable::reason)
            .collect::<Vec<_>>(),
        vec![
            BranchProtectionReason::StillReachable,
            BranchProtectionReason::StillReachable,
        ]
    );

    let clear_outputs = BranchReleasePlan::from_removed_refs(
        parent,
        parent_after.table_refs().to_vec(),
        &BranchReachabilityAggregate::empty(),
        Some(&SharedTableRegistry::new()),
    )
    .expect("clear outputs release");
    assert_eq!(
        clear_outputs
            .releasable_tables()
            .iter()
            .map(TableIdentity::as_str)
            .collect::<Vec<_>>(),
        outcome
            .output_refs()
            .iter()
            .map(|table_ref| table_ref.table_identity().as_str())
            .collect::<Vec<_>>()
    );
    for removed_ref in outcome.removed_refs() {
        assert!(!clear_outputs
            .releasable_tables()
            .iter()
            .any(|identity| identity == removed_ref.table_identity()));
    }
}

#[test]
fn branch_owned_level_outside_configured_count_is_rejected_without_mutation() {
    let branch = branch_id(70);
    let config = BranchRuntimeConfig::new(3, 64, 32).expect("config");
    let mut state = BranchLocalState::new(branch, config).expect("state");
    let outside = BranchLevel::new(3);
    let outside_level_table = branch_owned_table(
        branch,
        outside,
        "outside-level",
        vec![storage_row(branch, 10)],
    );
    let before_outside = state.clone();
    assert!(matches!(
        state.install_owned_table_at_level(outside, outside_level_table),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
    assert_eq!(state, before_outside);
}
