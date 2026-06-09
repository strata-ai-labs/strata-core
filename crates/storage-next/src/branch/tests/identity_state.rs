use super::*;

#[test]
fn branch_row_result_shells_preserve_row_and_source() {
    let row = storage_row(branch_id(4), 17);
    let visible = BranchVisibleRow::new(row.clone(), BranchRowSource::Active);
    assert_eq!(visible.row(), &row);
    assert_eq!(visible.source(), BranchRowSource::Active);

    let source = BranchRowSource::OwnedTable {
        level: BranchLevel::ZERO,
        table_index: 2,
    };
    let history = BranchHistoryRow::new(row.clone(), source);
    assert_eq!(history.row(), &row);
    assert_eq!(history.source(), source);

    let inherited_source = BranchRowSource::Inherited {
        source_branch_id: branch_id(5),
        layer_index: 1,
    };
    let inherited = BranchVisibleRow::new(row, inherited_source);
    assert_eq!(inherited.source(), inherited_source);

    let frozen_source = BranchRowSource::Frozen { index: 3 };
    let frozen = BranchHistoryRow::new(storage_row(branch_id(4), 18), frozen_source);
    assert_eq!(frozen.source(), frozen_source);
}

#[test]
fn branch_row_identity_accepts_matching_rows_and_rejects_mismatches() {
    let expected = branch_id(10);
    let other = branch_id(11);
    let row = storage_row(expected, 21);

    assert!(row_matches_branch(expected, &row));
    assert!(!row_matches_branch(other, &row));
    require_physical_key_branch(expected, row.physical_key()).expect("matching key");

    let identity: BranchRowIdentity =
        require_row_branch(expected, &row).expect("matching row identity");
    assert_eq!(identity.branch_id(), expected);
    assert_eq!(identity.physical_key(), row.physical_key());
    assert_eq!(identity.commit_version(), CommitVersion::new(21));
    assert_eq!(identity.commit_timestamp(), Timestamp::from_micros(21));

    let mismatch = require_row_branch(other, &row).expect_err("branch mismatch");
    assert!(matches!(
        mismatch,
        BranchRuntimeError::InvalidBranchRow { .. }
    ));
    assert!(!mismatch.to_string().contains("row-bytes"));
}

#[test]
fn branch_physical_key_validation_accepts_opaque_edge_key_shapes() {
    let zero_branch = BranchId::from_bytes([0x00; BranchId::BYTE_LEN]);
    let high_branch = BranchId::from_bytes([0xff; BranchId::BYTE_LEN]);
    let mixed_branch = BranchId::from_bytes([
        0x00, 0x01, 0x02, 0x03, 0x80, 0x81, 0xfe, 0xff, 0x10, 0x20, 0x30, 0x40, 0x55, 0xaa, 0x7e,
        0xe7,
    ]);
    let keys = [
        physical_key_with(
            zero_branch,
            "system",
            StorageSpaceId::COMMIT_TIMELINE,
            Vec::new(),
        ),
        physical_key_with(
            high_branch,
            "default-prefix",
            StorageSpaceId::engine(0xff).expect("engine storage space"),
            vec![0x00, 0x00, 0xff],
        ),
        physical_key_with(
            mixed_branch,
            "default",
            StorageSpaceId::engine(0x20).expect("engine storage space"),
            vec![0x80, 0x00, 0x81, 0xfe],
        ),
    ];

    for key in &keys {
        require_physical_key_branch(key.branch_id(), key).expect("opaque branch key accepted");
        let same_branch = rewrite_physical_key_branch(key, key.branch_id())
            .expect("same-branch physical key rewrite");
        assert_eq!(same_branch, *key);
    }

    let rewritten =
        rewrite_physical_key_branch(&keys[1], mixed_branch).expect("physical key rewrite");
    assert_eq!(rewritten.branch_id(), mixed_branch);
    assert_eq!(rewritten.space(), "default-prefix");
    assert_eq!(
        rewritten.storage_space_id(),
        StorageSpaceId::engine(0xff).expect("engine storage space")
    );
    assert_eq!(rewritten.user_key(), &[0x00, 0x00, 0xff]);
    assert_eq!(
        rewrite_physical_key_branch(&rewritten, high_branch).expect("round trip key"),
        keys[1],
    );
    assert_eq!(
        &TablePhysicalKeyBytes::from_physical_key(&rewritten).as_slice()[..BranchId::BYTE_LEN],
        mixed_branch.as_bytes()
    );

    let mismatch =
        require_physical_key_branch(high_branch, &keys[0]).expect_err("wrong branch key");
    assert!(matches!(
        mismatch,
        BranchRuntimeError::InvalidBranchRow { .. }
    ));
    let text = mismatch.to_string();
    assert!(!text.contains("main"));
    assert!(!text.contains("default-prefix"));
    assert!(!text.contains("row-bytes"));
}

#[test]
fn branch_row_validation_accepts_put_tombstone_and_edge_rows_without_policy() {
    let branch = branch_id(20);
    let other = branch_id(21);
    let put = StorageRow::put(
        physical_key_with(
            branch,
            "system",
            StorageSpaceId::COMMIT_TIMELINE,
            Vec::new(),
        ),
        CommitVersion::ZERO,
        Timestamp::EPOCH,
        Timestamp::from_micros(1),
        Vec::new(),
    );
    let put_identity = require_row_branch(branch, &put).expect("edge put row");
    assert_eq!(put_identity.branch_id(), branch);
    assert_eq!(put_identity.physical_key(), put.physical_key());
    assert_eq!(put_identity.commit_version(), CommitVersion::ZERO);
    assert_eq!(put_identity.commit_timestamp(), Timestamp::EPOCH);
    assert_eq!(put.expires_at(), Timestamp::from_micros(1));
    assert!(put.value().is_empty());
    assert!(!put.is_tombstone());

    let tombstone = StorageRow::tombstone(
        physical_key_with(
            branch,
            "default",
            StorageSpaceId::engine(0x21).expect("engine storage space"),
            vec![0x00, 0xff],
        ),
        CommitVersion::MAX,
        Timestamp::MAX,
    );
    let tombstone_identity = require_row_branch(branch, &tombstone).expect("tombstone row");
    assert_eq!(tombstone_identity.physical_key(), tombstone.physical_key());
    assert_eq!(tombstone_identity.commit_version(), CommitVersion::MAX);
    assert_eq!(tombstone_identity.commit_timestamp(), Timestamp::MAX);
    assert!(tombstone.is_tombstone());
    assert!(tombstone.value().is_empty());

    let wrong_put = StorageRow::put(
        physical_key(other, b"wrong-put".to_vec()),
        CommitVersion::new(1),
        Timestamp::from_micros(1),
        Timestamp::EPOCH,
        b"secret-payload".to_vec(),
    );
    let wrong_tombstone = tombstone_row(other, b"wrong-delete".to_vec(), 2, 2);
    for row in [&wrong_put, &wrong_tombstone] {
        let error = require_row_branch(branch, row).expect_err("wrong-branch row rejected");
        assert!(matches!(error, BranchRuntimeError::InvalidBranchRow { .. }));
        assert!(!error.to_string().contains("secret-payload"));
    }
}

#[test]
fn branch_rewrite_preserves_put_and_tombstone_row_facts() {
    let source = branch_id(12);
    let target = branch_id(13);
    let put = storage_row_with(
        source,
        b"\x00user\x00\x00key".to_vec(),
        31,
        310,
        Timestamp::from_micros(999),
        b"secret-payload".to_vec(),
    );

    let rewritten_key =
        rewrite_physical_key_branch(put.physical_key(), target).expect("rewrite key");
    assert_eq!(rewritten_key.branch_id(), target);
    assert_eq!(rewritten_key.space(), put.physical_key().space());
    assert_eq!(
        rewritten_key.storage_space_id(),
        put.physical_key().storage_space_id()
    );
    assert_eq!(rewritten_key.user_key(), put.physical_key().user_key());

    let rewritten = rewrite_row_branch(&put, source, target).expect("rewrite put");
    assert_eq!(rewritten.physical_key().branch_id(), target);
    assert_eq!(rewritten.physical_key().space(), put.physical_key().space());
    assert_eq!(
        rewritten.physical_key().user_key(),
        put.physical_key().user_key()
    );
    assert_eq!(rewritten.commit_version(), put.commit_version());
    assert_eq!(rewritten.commit_timestamp(), put.commit_timestamp());
    assert_eq!(rewritten.expires_at(), put.expires_at());
    assert_eq!(rewritten.value(), put.value());
    assert!(!rewritten.is_tombstone());

    let round_trip = rewrite_row_branch(&rewritten, target, source).expect("round trip");
    assert_eq!(round_trip, put);
    assert_eq!(
        rewrite_row_branch(&put, source, source).expect("same branch rewrite"),
        put
    );
    assert!(matches!(
        rewrite_row_branch(&put, target, source),
        Err(BranchRuntimeError::InvalidBranchRow { .. })
    ));

    let tombstone = tombstone_row(source, b"deleted".to_vec(), 44, 440);
    let rewritten_tombstone =
        rewrite_row_branch(&tombstone, source, target).expect("rewrite tombstone");
    assert_eq!(rewritten_tombstone.physical_key().branch_id(), target);
    assert_eq!(
        rewritten_tombstone.physical_key().user_key(),
        tombstone.physical_key().user_key()
    );
    assert_eq!(rewritten_tombstone.commit_version(), CommitVersion::new(44));
    assert_eq!(
        rewritten_tombstone.commit_timestamp(),
        Timestamp::from_micros(440)
    );
    assert!(rewritten_tombstone.is_tombstone());
    assert!(rewritten_tombstone.value().is_empty());
}

#[test]
fn branch_rewrite_preserves_empty_put_values_and_storage_owned_keys() {
    let source = branch_id(22);
    let target = branch_id(23);
    let put = StorageRow::put(
        physical_key_with(
            source,
            "system",
            StorageSpaceId::COMMIT_TIMELINE,
            Vec::new(),
        ),
        CommitVersion::new(9),
        Timestamp::from_micros(90),
        Timestamp::from_micros(100),
        Vec::new(),
    );

    let rewritten = rewrite_row_branch(&put, source, target).expect("rewrite empty put");
    assert_eq!(rewritten.physical_key().branch_id(), target);
    assert_eq!(rewritten.physical_key().space(), "system");
    assert_eq!(
        rewritten.physical_key().storage_space_id(),
        StorageSpaceId::COMMIT_TIMELINE
    );
    assert!(rewritten.physical_key().user_key().is_empty());
    assert_eq!(rewritten.commit_version(), put.commit_version());
    assert_eq!(rewritten.commit_timestamp(), put.commit_timestamp());
    assert_eq!(rewritten.expires_at(), put.expires_at());
    assert!(rewritten.value().is_empty());
    assert!(!rewritten.is_tombstone());
}

#[test]
fn branch_rewrite_groups_inherited_rows_with_child_local_encoded_keys() {
    let source = branch_id(16);
    let target = branch_id(17);
    let user_key = b"shared-logical-key".to_vec();
    let inherited = storage_row_with(
        source,
        user_key.clone(),
        7,
        70,
        Timestamp::EPOCH,
        b"inherited".to_vec(),
    );
    let child_local =
        storage_row_with(target, user_key, 5, 50, Timestamp::EPOCH, b"child".to_vec());

    let rewritten = rewrite_row_branch(&inherited, source, target).expect("rewrite inherited row");
    let rewritten_prefix = TablePhysicalKeyBytes::from_row(&rewritten);
    let child_prefix = TablePhysicalKeyBytes::from_row(&child_local);
    assert_eq!(rewritten_prefix.as_slice(), child_prefix.as_slice());
    assert_eq!(
        &rewritten_prefix.as_slice()[..BranchId::BYTE_LEN],
        target.as_bytes()
    );

    let mut rows = vec![TableRow::new(child_local), TableRow::new(rewritten)];
    sort_table_rows_by_key(&mut rows);
    assert_eq!(rows[0].commit_version(), CommitVersion::new(7));
    assert_eq!(rows[1].commit_version(), CommitVersion::new(5));
    assert_eq!(
        TablePhysicalKeyBytes::from_row(rows[0].row()).as_slice(),
        TablePhysicalKeyBytes::from_row(rows[1].row()).as_slice()
    );
}

#[test]
fn branch_effective_read_bounds_apply_inclusive_own_and_inherited_caps() {
    let row = storage_row_with(
        branch_id(14),
        b"bounded".to_vec(),
        50,
        500,
        Timestamp::EPOCH,
        b"value".to_vec(),
    );

    let latest = BranchEffectiveReadBound::for_own_branch(BranchReadBound::latest());
    assert_eq!(latest.max_commit_version(), None);
    assert_eq!(latest.max_commit_timestamp(), None);
    assert!(latest.matches_row(&row));

    let exact_version = BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_version(
        CommitVersion::new(50),
    ));
    assert!(exact_version.matches_row(&row));
    let before_version = BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_version(
        CommitVersion::new(49),
    ));
    assert!(!before_version.row_version_in_bound(&row));

    let exact_timestamp = BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_timestamp(
        Timestamp::from_micros(500),
    ));
    assert!(exact_timestamp.matches_row(&row));
    let before_timestamp = BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_timestamp(
        Timestamp::from_micros(499),
    ));
    assert!(!before_timestamp.row_timestamp_in_bound(&row));

    let inherited_latest = BranchEffectiveReadBound::for_inherited_layer(
        BranchReadBound::latest(),
        CommitVersion::new(50),
    );
    assert_eq!(
        inherited_latest.max_commit_version(),
        Some(CommitVersion::new(50))
    );
    assert!(inherited_latest.matches_row(&row));

    let inherited_timestamp = BranchEffectiveReadBound::for_inherited_layer(
        BranchReadBound::at_timestamp(Timestamp::from_micros(500)),
        CommitVersion::new(49),
    );
    assert_eq!(
        inherited_timestamp.max_commit_version(),
        Some(CommitVersion::new(49))
    );
    assert_eq!(
        inherited_timestamp.max_commit_timestamp(),
        Some(Timestamp::from_micros(500))
    );
    assert!(!inherited_timestamp.row_version_in_bound(&row));
    assert!(inherited_timestamp.row_timestamp_in_bound(&row));
    assert!(!inherited_timestamp.matches_row(&row));

    let inherited_version = BranchEffectiveReadBound::for_inherited_layer(
        BranchReadBound::at_version(CommitVersion::new(70)),
        CommitVersion::new(55),
    );
    assert_eq!(
        inherited_version.max_commit_version(),
        Some(CommitVersion::new(55))
    );
}

#[test]
fn branch_own_bounds_cover_zero_epoch_and_below_equal_above_edges() {
    let branch = branch_id(24);
    let zero_row = storage_row_with(
        branch,
        b"zero".to_vec(),
        0,
        0,
        Timestamp::EPOCH,
        b"zero".to_vec(),
    );
    let later_row = storage_row_with(
        branch,
        b"later".to_vec(),
        1,
        1,
        Timestamp::EPOCH,
        b"later".to_vec(),
    );

    let latest = BranchEffectiveReadBound::for_own_branch(BranchReadBound::latest());
    assert!(latest.matches_row(&zero_row));
    assert!(latest.matches_row(&later_row));

    let version_zero =
        BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_version(CommitVersion::ZERO));
    assert_eq!(version_zero.max_commit_version(), Some(CommitVersion::ZERO));
    assert_eq!(version_zero.max_commit_timestamp(), None);
    assert!(version_zero.matches_row(&zero_row));
    assert!(!version_zero.row_version_in_bound(&later_row));

    let version_one = BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_version(
        CommitVersion::new(1),
    ));
    assert!(version_one.row_version_in_bound(&zero_row));
    assert!(version_one.row_version_in_bound(&later_row));

    let timestamp_epoch =
        BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_timestamp(Timestamp::EPOCH));
    assert_eq!(timestamp_epoch.max_commit_version(), None);
    assert_eq!(
        timestamp_epoch.max_commit_timestamp(),
        Some(Timestamp::EPOCH)
    );
    assert!(timestamp_epoch.matches_row(&zero_row));
    assert!(!timestamp_epoch.row_timestamp_in_bound(&later_row));

    let timestamp_one = BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_timestamp(
        Timestamp::from_micros(1),
    ));
    assert!(timestamp_one.row_timestamp_in_bound(&zero_row));
    assert!(timestamp_one.row_timestamp_in_bound(&later_row));
}

#[test]
fn branch_inherited_bounds_cover_fork_edges_and_combined_timestamp_match() {
    let branch = branch_id(25);
    let fork_version = CommitVersion::new(4);
    let row_at_fork = storage_row_with(
        branch,
        b"at-fork".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"at".to_vec(),
    );
    let row_after_fork = storage_row_with(
        branch,
        b"after-fork".to_vec(),
        5,
        40,
        Timestamp::EPOCH,
        b"after".to_vec(),
    );
    let row_after_timestamp = storage_row_with(
        branch,
        b"after-time".to_vec(),
        3,
        41,
        Timestamp::EPOCH,
        b"after-time".to_vec(),
    );

    let latest =
        BranchEffectiveReadBound::for_inherited_layer(BranchReadBound::latest(), fork_version);
    assert_eq!(latest.max_commit_version(), Some(fork_version));
    assert!(latest.matches_row(&row_at_fork));
    assert!(!latest.row_version_in_bound(&row_after_fork));

    for (requested, expected) in [
        (CommitVersion::new(3), CommitVersion::new(3)),
        (CommitVersion::new(4), CommitVersion::new(4)),
        (CommitVersion::new(5), CommitVersion::new(4)),
    ] {
        let bound = BranchEffectiveReadBound::for_inherited_layer(
            BranchReadBound::at_version(requested),
            fork_version,
        );
        assert_eq!(bound.max_commit_version(), Some(expected));
        assert_eq!(bound.max_commit_timestamp(), None);
    }

    let timestamp_bound = BranchEffectiveReadBound::for_inherited_layer(
        BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        fork_version,
    );
    assert_eq!(timestamp_bound.max_commit_version(), Some(fork_version));
    assert_eq!(
        timestamp_bound.max_commit_timestamp(),
        Some(Timestamp::from_micros(40))
    );
    assert!(timestamp_bound.matches_row(&row_at_fork));

    assert!(!timestamp_bound.row_version_in_bound(&row_after_fork));
    assert!(timestamp_bound.row_timestamp_in_bound(&row_after_fork));
    assert!(!timestamp_bound.matches_row(&row_after_fork));

    assert!(timestamp_bound.row_version_in_bound(&row_after_timestamp));
    assert!(!timestamp_bound.row_timestamp_in_bound(&row_after_timestamp));
    assert!(!timestamp_bound.matches_row(&row_after_timestamp));
}

#[test]
fn branch_effective_bounds_filter_row_chains_without_collapsing_versions() {
    let branch = branch_id(18);
    let user_key = b"row-chain".to_vec();
    let mut rows = vec![
        TableRow::new(storage_row_with(
            branch,
            user_key.clone(),
            3,
            30,
            Timestamp::from_micros(25),
            b"expired-looking".to_vec(),
        )),
        TableRow::new(storage_row_with(
            branch,
            user_key.clone(),
            5,
            50,
            Timestamp::EPOCH,
            b"newest".to_vec(),
        )),
        TableRow::new(tombstone_row(branch, user_key.clone(), 4, 40)),
        TableRow::new(storage_row_with(
            branch,
            user_key,
            2,
            60,
            Timestamp::EPOCH,
            b"timestamp-late".to_vec(),
        )),
    ];
    sort_table_rows_by_key(&mut rows);
    assert_eq!(row_versions(&rows), vec![5, 4, 3, 2]);

    let version_bound = BranchEffectiveReadBound::new(Some(CommitVersion::new(4)), None);
    assert_eq!(matching_versions(&rows, version_bound), vec![4, 3, 2]);

    let timestamp_bound = BranchEffectiveReadBound::new(None, Some(Timestamp::from_micros(40)));
    assert_eq!(matching_versions(&rows, timestamp_bound), vec![4, 3]);

    let combined_bound = BranchEffectiveReadBound::new(
        Some(CommitVersion::new(4)),
        Some(Timestamp::from_micros(40)),
    );
    let combined = matching_versions(&rows, combined_bound);
    assert_eq!(combined, vec![4, 3]);
    assert!(
        combined.len() > 1,
        "candidate filtering must not collapse row history into one visible row",
    );

    let candidates = rows
        .iter()
        .filter(|row| combined_bound.matches_row(row.row()))
        .collect::<Vec<_>>();
    assert!(candidates
        .iter()
        .any(|candidate| candidate.row().is_tombstone()));
    assert!(candidates
        .iter()
        .any(|candidate| candidate.row().expires_at() == Timestamp::from_micros(25)));

    let wrong_branch = storage_row(branch_id(19), 4);
    assert!(matches!(
        require_row_branch(branch, &wrong_branch),
        Err(BranchRuntimeError::InvalidBranchRow { .. })
    ));
}

#[test]
fn branch_candidate_filtering_preserves_tombstone_and_expiry_without_visibility_policy() {
    let branch = branch_id(15);
    let expired_looking = storage_row_with(
        branch,
        b"expired".to_vec(),
        10,
        100,
        Timestamp::from_micros(90),
        b"value".to_vec(),
    );
    let bound = BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_timestamp(
        Timestamp::from_micros(100),
    ));
    let source = BranchRowSource::Active;
    assert_eq!(source, BranchRowSource::Active);
    assert_eq!(expired_looking.physical_key().branch_id(), branch);
    assert_eq!(expired_looking.commit_version(), CommitVersion::new(10));
    assert_eq!(
        expired_looking.commit_timestamp(),
        Timestamp::from_micros(100)
    );
    assert_eq!(expired_looking.expires_at(), Timestamp::from_micros(90));
    assert!(!expired_looking.is_tombstone());
    assert!(bound.matches_row(&expired_looking));

    let tombstone = tombstone_row(branch, b"deleted".to_vec(), 11, 100);
    let tombstone_source = BranchRowSource::Frozen { index: 0 };
    assert_eq!(tombstone_source, BranchRowSource::Frozen { index: 0 });
    assert!(tombstone.is_tombstone());
    assert!(bound.matches_row(&tombstone));
}

#[test]
fn branch_effective_bound_records_each_axis_independently() {
    let row = storage_row_with(
        branch_id(26),
        b"axis".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"value".to_vec(),
    );

    let version_miss = BranchEffectiveReadBound::new(
        Some(CommitVersion::new(4)),
        Some(Timestamp::from_micros(50)),
    );
    assert!(!version_miss.row_version_in_bound(&row));
    assert!(version_miss.row_timestamp_in_bound(&row));
    assert!(!version_miss.matches_row(&row));

    let timestamp_miss = BranchEffectiveReadBound::new(
        Some(CommitVersion::new(5)),
        Some(Timestamp::from_micros(49)),
    );
    assert!(timestamp_miss.row_version_in_bound(&row));
    assert!(!timestamp_miss.row_timestamp_in_bound(&row));
    assert!(!timestamp_miss.matches_row(&row));

    let both_miss = BranchEffectiveReadBound::new(
        Some(CommitVersion::new(4)),
        Some(Timestamp::from_micros(49)),
    );
    assert!(!both_miss.row_version_in_bound(&row));
    assert!(!both_miss.row_timestamp_in_bound(&row));
    assert!(!both_miss.matches_row(&row));

    let latest = BranchEffectiveReadBound::for_own_branch(BranchReadBound::latest());
    assert!(latest.matches_row(&row));
}

#[test]
fn branch_local_state_constructs_empty_active_state() {
    let branch = branch_id(30);
    let config = BranchRuntimeConfig::new(3, 4, 2).expect("valid config");
    let state = BranchLocalState::new(branch, config).expect("branch-local state");

    assert_eq!(state.branch_id(), branch);
    assert_eq!(state.config(), config);
    assert!(state.is_empty());
    assert_eq!(state.active_row_count(), 0);
    assert!(state.active().is_empty());
    assert!(state.frozen().is_empty());
    assert_eq!(state.frozen_table_count(), 0);
    assert_eq!(state.max_commit_version(), None);
    assert_eq!(state.timestamp_min(), None);
    assert_eq!(state.timestamp_max(), None);
    assert_eq!(state.put_rows(), 0);
    assert_eq!(state.tombstone_rows(), 0);
    assert_eq!(
        state.facts().expect("facts"),
        BranchStateFacts::empty(branch)
    );

    let default_state = BranchLocalState::empty(branch);
    assert_eq!(default_state.branch_id(), branch);
    assert!(default_state.is_empty());
}

#[test]
fn branch_local_state_appends_puts_tombstones_and_preserves_row_facts() {
    let branch = branch_id(31);
    let mut state = BranchLocalState::empty(branch);
    let put = storage_row_with(
        branch,
        b"same-logical-key".to_vec(),
        5,
        50,
        Timestamp::from_micros(500),
        b"secret-payload".to_vec(),
    );
    let older_same_key = storage_row_with(
        branch,
        b"same-logical-key".to_vec(),
        4,
        40,
        Timestamp::from_micros(40),
        Vec::new(),
    );
    let same_version_other_key = storage_row_with(
        branch,
        b"other-logical-key".to_vec(),
        5,
        60,
        Timestamp::EPOCH,
        b"other".to_vec(),
    );
    let tombstone = tombstone_row(branch, b"deleted".to_vec(), 9, 30);

    let put_outcome = state
        .append_committed_row(put.clone())
        .expect("append put row");
    assert_eq!(put_outcome.branch_id(), branch);
    assert_eq!(put_outcome.commit_version(), CommitVersion::new(5));
    assert_eq!(put_outcome.commit_timestamp(), Timestamp::from_micros(50));
    assert!(!put_outcome.is_tombstone());
    assert_eq!(put_outcome.active_rows(), 1);
    assert!(put_outcome.approximate_active_bytes() > 0);
    assert_eq!(
        state
            .active()
            .get(&TableInternalKeyBytes::from_row(&put))
            .expect("stored put")
            .row(),
        &put
    );

    state
        .append_committed_row(older_same_key.clone())
        .expect("same physical key at different version");
    state
        .append_committed_row(same_version_other_key.clone())
        .expect("different physical key at same version");
    let tombstone_outcome = state
        .append_committed_row(tombstone.clone())
        .expect("append tombstone row");
    assert!(tombstone_outcome.is_tombstone());
    assert_eq!(state.active_row_count(), 4);
    assert_eq!(state.put_rows(), 3);
    assert_eq!(state.tombstone_rows(), 1);
    for row in [&older_same_key, &same_version_other_key, &tombstone] {
        assert_eq!(
            state
                .active()
                .get(&TableInternalKeyBytes::from_row(row))
                .expect("stored row")
                .row(),
            row
        );
    }

    let facts = state.facts().expect("facts");
    assert_eq!(facts.active_rows(), 4);
    assert_eq!(facts.frozen_table_count(), 0);
    assert_eq!(facts.owned_table_count(), 0);
    assert_eq!(facts.inherited_layer_count(), 0);
    assert_eq!(facts.max_commit_version(), Some(CommitVersion::new(9)));
    assert_eq!(facts.timestamp_min(), Some(Timestamp::from_micros(30)));
    assert_eq!(facts.timestamp_max(), Some(Timestamp::from_micros(60)));
}

#[test]
fn branch_local_state_tracks_zero_max_version_and_timestamp_edges() {
    let branch = branch_id(37);
    let mut state = BranchLocalState::empty(branch);
    let zero = storage_row_with(branch, b"zero".to_vec(), 0, 0, Timestamp::EPOCH, Vec::new());
    let max = storage_row_with(
        branch,
        b"max".to_vec(),
        u64::MAX,
        u64::MAX,
        Timestamp::MAX,
        b"max".to_vec(),
    );

    state
        .append_committed_row(zero)
        .expect("append zero edge row");
    let zero_facts = state.facts().expect("zero facts");
    assert_eq!(zero_facts.max_commit_version(), Some(CommitVersion::ZERO));
    assert_eq!(zero_facts.timestamp_min(), Some(Timestamp::EPOCH));
    assert_eq!(zero_facts.timestamp_max(), Some(Timestamp::EPOCH));

    state
        .append_committed_row(max)
        .expect("append max edge row");
    let max_facts = state.facts().expect("max facts");
    assert_eq!(max_facts.max_commit_version(), Some(CommitVersion::MAX));
    assert_eq!(max_facts.timestamp_min(), Some(Timestamp::EPOCH));
    assert_eq!(max_facts.timestamp_max(), Some(Timestamp::MAX));

    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated {
            frozen_index: 0,
            frozen_rows: 2,
            frozen_tables: 1,
        }
    ));
    let rotated_facts = state.facts().expect("rotated facts");
    assert_eq!(rotated_facts.active_rows(), 0);
    assert_eq!(rotated_facts.frozen_table_count(), 1);
    assert_eq!(rotated_facts.max_commit_version(), Some(CommitVersion::MAX));
    assert_eq!(rotated_facts.timestamp_min(), Some(Timestamp::EPOCH));
    assert_eq!(rotated_facts.timestamp_max(), Some(Timestamp::MAX));
}

#[test]
fn branch_local_state_accepts_opaque_branch_ids_and_key_shapes() {
    let branch_ids = [
        BranchId::from_bytes([0x00; BranchId::BYTE_LEN]),
        BranchId::from_bytes([0xff; BranchId::BYTE_LEN]),
        BranchId::from_bytes([
            0x00, 0x01, 0x02, 0x03, 0x80, 0x81, 0xfe, 0xff, 0x10, 0x20, 0x30, 0x40, 0x55, 0xaa,
            0x7e, 0xe7,
        ]),
    ];

    for branch in branch_ids {
        let mut state = BranchLocalState::empty(branch);
        let rows = [
            StorageRow::put(
                physical_key_with(
                    branch,
                    "system",
                    StorageSpaceId::COMMIT_TIMELINE,
                    Vec::new(),
                ),
                CommitVersion::new(1),
                Timestamp::from_micros(100),
                Timestamp::EPOCH,
                Vec::new(),
            ),
            storage_row_with(
                branch,
                vec![0x00, 0x00, 0xff],
                2,
                100,
                Timestamp::from_micros(1),
                b"nul-key".to_vec(),
            ),
            storage_row_with(
                branch,
                vec![0x80, 0x00, 0xfe, 0xff],
                3,
                90,
                Timestamp::EPOCH,
                b"high-bit".to_vec(),
            ),
            StorageRow::put(
                physical_key_with(
                    branch,
                    "default-prefix",
                    StorageSpaceId::engine(0xff).expect("engine storage space"),
                    b"shared-prefix-key".to_vec(),
                ),
                CommitVersion::new(4),
                Timestamp::from_micros(110),
                Timestamp::EPOCH,
                b"prefixed-space".to_vec(),
            ),
        ];

        for row in &rows {
            state
                .append_committed_row(row.clone())
                .expect("append opaque branch/key row");
            assert_eq!(
                state
                    .active()
                    .get(&TableInternalKeyBytes::from_row(row))
                    .expect("stored opaque row")
                    .row(),
                row
            );
        }

        let facts = state.facts().expect("opaque facts");
        assert_eq!(
            facts.active_rows(),
            u64::try_from(rows.len()).expect("row count fits in u64")
        );
        assert_eq!(facts.frozen_table_count(), 0);
        assert_eq!(facts.max_commit_version(), Some(CommitVersion::new(4)));
        assert_eq!(facts.timestamp_min(), Some(Timestamp::from_micros(90)));
        assert_eq!(facts.timestamp_max(), Some(Timestamp::from_micros(110)));
    }
}

#[test]
fn branch_local_state_rejects_wrong_branch_rows_without_mutation() {
    let branch = branch_id(32);
    let wrong_branch = branch_id(33);
    let mut state = BranchLocalState::empty(branch);
    let baseline_facts = state.facts().expect("baseline facts");
    let wrong_put = storage_row_with(
        wrong_branch,
        b"wrong".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"secret-payload".to_vec(),
    );
    let wrong_tombstone = tombstone_row(wrong_branch, b"wrong-delete".to_vec(), 2, 20);

    for row in [wrong_put, wrong_tombstone] {
        let error = state
            .append_committed_row(row)
            .expect_err("wrong branch rejected");
        assert!(matches!(error, BranchRuntimeError::InvalidBranchRow { .. }));
        assert!(!error.to_string().contains("secret-payload"));
        assert_eq!(
            state.facts().expect("facts after rejection"),
            baseline_facts
        );
        assert!(state.active().is_empty());
        assert!(state.frozen().is_empty());
    }
}

#[test]
fn branch_local_state_rejects_active_duplicates_and_defers_frozen_duplicate_resolution() {
    let branch = branch_id(34);
    let mut state = BranchLocalState::empty(branch);
    let row = storage_row_with(
        branch,
        b"duplicate".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        b"secret-payload".to_vec(),
    );

    state
        .append_committed_row(row.clone())
        .expect("initial append");
    let facts_after_append = state.facts().expect("append facts");
    let active_duplicate = state
        .append_committed_row(row.clone())
        .expect_err("active duplicate rejected");
    assert_duplicate_internal_key(&active_duplicate);
    assert_eq!(
        state.facts().expect("after active duplicate"),
        facts_after_append
    );
    assert_eq!(state.active_row_count(), 1);

    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated {
            frozen_index: 0,
            frozen_rows: 1,
            frozen_tables: 1,
        }
    ));
    let frozen_duplicate = state
        .append_committed_row(row)
        .expect("frozen duplicate append is resolved by source ordering");
    assert_eq!(frozen_duplicate.active_rows(), 1);
    assert_eq!(state.active_row_count(), 1);
    assert_eq!(state.frozen_table_count(), 1);
}

#[test]
fn branch_local_state_atomic_batch_append_rejects_invalid_rows_without_mutation() {
    let branch = branch_id(34);
    let wrong_branch = branch_id(35);
    let row = storage_row_with(
        branch,
        b"batch-duplicate".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        b"secret-payload".to_vec(),
    );
    let wrong_row = storage_row_with(
        wrong_branch,
        b"wrong".to_vec(),
        8,
        80,
        Timestamp::EPOCH,
        b"secret-payload".to_vec(),
    );

    let mut empty_state = BranchLocalState::empty(branch);
    let empty_facts = empty_state.facts().expect("empty facts");
    assert!(matches!(
        empty_state.append_committed_rows_atomically(Vec::new()),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
    assert_eq!(empty_state.facts().expect("after empty"), empty_facts);

    let mut wrong_state = BranchLocalState::empty(branch);
    let wrong_facts = wrong_state.facts().expect("wrong baseline facts");
    assert!(matches!(
        wrong_state.append_committed_rows_atomically(vec![wrong_row]),
        Err(BranchRuntimeError::InvalidBranchRow { .. })
    ));
    assert_eq!(
        wrong_state.facts().expect("after wrong branch"),
        wrong_facts
    );
    assert_eq!(wrong_state.active_row_count(), 0);

    let mut active_duplicate_state = BranchLocalState::empty(branch);
    active_duplicate_state
        .append_committed_row(row.clone())
        .expect("seed active row");
    let active_duplicate_facts = active_duplicate_state
        .facts()
        .expect("active duplicate baseline");
    let active_duplicate = active_duplicate_state
        .append_committed_rows_atomically(vec![row.clone()])
        .expect_err("active duplicate rejected");
    assert_duplicate_internal_key(&active_duplicate);
    assert_eq!(
        active_duplicate_state
            .facts()
            .expect("after active duplicate"),
        active_duplicate_facts
    );
    assert_eq!(active_duplicate_state.active_row_count(), 1);

    let mut frozen_duplicate_state = BranchLocalState::empty(branch);
    frozen_duplicate_state
        .append_committed_row(row.clone())
        .expect("seed frozen row");
    assert!(matches!(
        frozen_duplicate_state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    let frozen_duplicate = frozen_duplicate_state
        .append_committed_rows_atomically(vec![row.clone()])
        .expect("frozen duplicate append is resolved by source ordering");
    assert_eq!(frozen_duplicate.appended_rows(), 1);
    assert_eq!(frozen_duplicate_state.active_row_count(), 1);
    assert_eq!(frozen_duplicate_state.frozen_table_count(), 1);

    let mut batch_duplicate_state = BranchLocalState::empty(branch);
    let batch_duplicate_facts = batch_duplicate_state
        .facts()
        .expect("batch duplicate baseline");
    let batch_duplicate = batch_duplicate_state
        .append_committed_rows_atomically(vec![row.clone(), row])
        .expect_err("batch duplicate rejected");
    assert_duplicate_internal_key(&batch_duplicate);
    assert_eq!(
        batch_duplicate_state
            .facts()
            .expect("after batch duplicate"),
        batch_duplicate_facts
    );
    assert_eq!(batch_duplicate_state.active_row_count(), 0);
}

#[test]
fn branch_local_state_atomic_batch_append_reports_batch_state_after_success() {
    let branch = branch_id(34);
    let mut state = BranchLocalState::empty(branch);
    let first = storage_row_with(
        branch,
        b"first".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"first".to_vec(),
    );
    let second = tombstone_row(branch, b"second".to_vec(), 4, 40);

    let outcome = state
        .append_committed_rows_atomically(vec![first.clone(), second.clone()])
        .expect("atomic batch append succeeds");

    assert_eq!(outcome.branch_id(), branch);
    assert_eq!(outcome.appended_rows(), 2);
    assert_eq!(outcome.active_rows(), 2);
    assert!(outcome.approximate_active_bytes() > 0);
    assert_eq!(outcome.max_commit_version(), Some(CommitVersion::new(4)));
    assert_eq!(state.active_row_count(), 2);
    assert_eq!(state.put_rows(), 1);
    assert_eq!(state.tombstone_rows(), 1);
    assert_eq!(
        state
            .active()
            .get(&TableInternalKeyBytes::from_row(&first))
            .expect("stored first")
            .row(),
        &first
    );
    assert_eq!(
        state
            .active()
            .get(&TableInternalKeyBytes::from_row(&second))
            .expect("stored second")
            .row(),
        &second
    );
}

#[test]
fn branch_local_state_rotation_preserves_rows_and_newest_first_order() {
    let branch = branch_id(35);
    let mut state =
        BranchLocalState::new(branch, BranchRuntimeConfig::new(7, 64, 4).expect("config"))
            .expect("state");

    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Skipped {
            reason: BranchRotationSkipReason::EmptyActive,
        }
    ));
    assert!(state.frozen().is_empty());

    let first = storage_row_with(
        branch,
        b"first".to_vec(),
        1,
        100,
        Timestamp::EPOCH,
        b"first".to_vec(),
    );
    state
        .append_committed_row(first.clone())
        .expect("append first");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated {
            frozen_index: 0,
            frozen_rows: 1,
            frozen_tables: 1,
        }
    ));
    assert!(state.active().is_empty());
    assert_eq!(
        state.frozen()[0]
            .get(&TableInternalKeyBytes::from_row(&first))
            .expect("first frozen row")
            .row(),
        &first
    );

    let second = storage_row_with(
        branch,
        b"second".to_vec(),
        2,
        50,
        Timestamp::EPOCH,
        b"second".to_vec(),
    );
    state
        .append_committed_row(second.clone())
        .expect("append second");
    assert_eq!(
        state.frozen()[0]
            .get(&TableInternalKeyBytes::from_row(&first))
            .expect("frozen row unchanged after active append")
            .row(),
        &first
    );
    assert_eq!(
        state
            .active()
            .get(&TableInternalKeyBytes::from_row(&second))
            .expect("second active row")
            .row(),
        &second
    );
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated {
            frozen_index: 0,
            frozen_rows: 1,
            frozen_tables: 2,
        }
    ));

    assert_eq!(
        state.frozen()[0]
            .get(&TableInternalKeyBytes::from_row(&second))
            .expect("newest frozen row")
            .row(),
        &second
    );
    assert_eq!(
        state.frozen()[1]
            .get(&TableInternalKeyBytes::from_row(&first))
            .expect("older frozen row")
            .row(),
        &first
    );
    let facts = state.facts().expect("facts");
    assert_eq!(facts.active_rows(), 0);
    assert_eq!(facts.frozen_table_count(), 2);
    assert_eq!(facts.max_commit_version(), Some(CommitVersion::new(2)));
    assert_eq!(facts.timestamp_min(), Some(Timestamp::from_micros(50)));
    assert_eq!(facts.timestamp_max(), Some(Timestamp::from_micros(100)));
}

#[test]
fn branch_local_state_respects_frozen_limit_without_dropping_active_rows() {
    let branch = branch_id(36);
    let config = BranchRuntimeConfig::new(7, 64, 1).expect("config");
    let mut state = BranchLocalState::new(branch, config).expect("state");
    let frozen_row = storage_row(branch, 1);
    let active_row = storage_row_with(
        branch,
        b"still-active".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"active".to_vec(),
    );
    let additional_active_row = storage_row_with(
        branch,
        b"still-active-too".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"active-too".to_vec(),
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
    let before_skip = state.facts().expect("before skip facts");

    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Skipped {
            reason: BranchRotationSkipReason::FrozenLimitReached,
        }
    ));
    assert_eq!(state.facts().expect("after skip facts"), before_skip);
    assert_eq!(state.active_row_count(), 1);
    assert_eq!(state.frozen_table_count(), 1);
    assert!(state
        .active()
        .get(&TableInternalKeyBytes::from_row(&active_row))
        .is_some());
    assert!(state.frozen()[0]
        .get(&TableInternalKeyBytes::from_row(&frozen_row))
        .is_some());

    state
        .append_committed_row(additional_active_row.clone())
        .expect("append after frozen-limit skip");
    assert_eq!(state.active_row_count(), 2);
    assert_eq!(state.frozen_table_count(), 1);
    assert_eq!(
        state
            .active()
            .get(&TableInternalKeyBytes::from_row(&additional_active_row))
            .expect("additional active row")
            .row(),
        &additional_active_row
    );
    let after_append = state.facts().expect("after append facts");
    assert_eq!(after_append.active_rows(), 2);
    assert_eq!(after_append.frozen_table_count(), 1);
    assert_eq!(
        after_append.max_commit_version(),
        Some(CommitVersion::new(3))
    );
}
