use super::*;

#[test]
fn branch_read_view_merges_owned_tables_with_active_and_frozen_by_commit_version() {
    let branch = branch_id(54);
    let mut state = BranchLocalState::empty(branch);
    let key = physical_key(branch, b"owned-chain".to_vec());
    let frozen_newer = storage_row_with(
        branch,
        b"owned-chain".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        b"frozen".to_vec(),
    );
    let active_older = storage_row_with(
        branch,
        b"owned-chain".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"active".to_vec(),
    );
    let owned_middle = storage_row_with(
        branch,
        b"owned-chain".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"owned".to_vec(),
    );

    state
        .append_committed_row(frozen_newer.clone())
        .expect("append frozen");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .append_committed_row(active_older.clone())
        .expect("append active");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "owned-chain",
            vec![owned_middle.clone()],
        ))
        .expect("install owned");

    let view = state.capture_read_view().expect("view");
    assert_eq!(
        view.latest(&key).expect("latest").expect("newest").source(),
        BranchRowSource::Frozen { index: 0 }
    );
    assert_eq!(
        view.at_version(&key, CommitVersion::new(5))
            .expect("at five")
            .expect("owned")
            .source(),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        }
    );
    assert_eq!(
        view.at_version(&key, CommitVersion::new(2))
            .expect("at two")
            .expect("active")
            .row(),
        &active_older
    );
    assert_eq!(
        history_versions(
            &view
                .history(&key, BranchHistoryOptions::all())
                .expect("history")
        ),
        vec![7, 5, 2]
    );
}

#[test]
fn branch_immutable_point_reads_choose_newer_between_active_and_l0() {
    let branch = branch_id(61);
    let mut state = BranchLocalState::empty(branch);
    let active_wins = storage_row_with(
        branch,
        b"active-wins".to_vec(),
        9,
        90,
        Timestamp::EPOCH,
        b"active".to_vec(),
    );
    let owned_beats_active_old = storage_row_with(
        branch,
        b"owned-beats-active".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"old-active".to_vec(),
    );
    state
        .append_committed_row(active_wins.clone())
        .expect("append active winner");
    state
        .append_committed_row(owned_beats_active_old)
        .expect("append active loser");
    let owned_rows = vec![
        storage_row_with(
            branch,
            b"active-wins".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            b"old-owned".to_vec(),
        ),
        storage_row_with(
            branch,
            b"owned-beats-active".to_vec(),
            7,
            70,
            Timestamp::EPOCH,
            b"owned-active".to_vec(),
        ),
    ];
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "active-l0-precedence",
            owned_rows.clone(),
        ))
        .expect("install L0");

    let view = state.capture_read_view().expect("view");
    assert_visible_row(
        view.latest(&physical_key(branch, b"active-wins".to_vec()))
            .expect("active wins")
            .as_ref(),
        &active_wins,
        BranchRowSource::Active,
    );
    assert_visible_row(
        view.latest(&physical_key(branch, b"owned-beats-active".to_vec()))
            .expect("owned active")
            .as_ref(),
        &owned_rows[1],
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
}

#[test]
fn branch_immutable_point_reads_choose_newer_between_frozen_l0_and_l1() {
    let branch = branch_id(62);
    let mut state = BranchLocalState::empty(branch);
    let frozen_wins = storage_row_with(
        branch,
        b"frozen-wins".to_vec(),
        8,
        80,
        Timestamp::EPOCH,
        b"frozen".to_vec(),
    );
    state
        .append_committed_row(frozen_wins.clone())
        .expect("append frozen winner");
    state
        .append_committed_row(storage_row_with(
            branch,
            b"owned-beats-frozen".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"old-frozen".to_vec(),
        ))
        .expect("append frozen loser");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));

    let owned_rows = vec![
        storage_row_with(
            branch,
            b"frozen-wins".to_vec(),
            5,
            50,
            Timestamp::EPOCH,
            b"old-owned".to_vec(),
        ),
        storage_row_with(
            branch,
            b"owned-beats-frozen".to_vec(),
            6,
            60,
            Timestamp::EPOCH,
            b"owned-frozen".to_vec(),
        ),
    ];
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "frozen-l0-precedence",
            owned_rows.clone(),
        ))
        .expect("install L0");
    let l1_only = storage_row_with(
        branch,
        b"l1-only".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"l1".to_vec(),
    );
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "source-precedence-l1",
                vec![l1_only.clone()],
            ),
        )
        .expect("install L1");

    let view = state.capture_read_view().expect("view");
    assert_visible_row(
        view.latest(&physical_key(branch, b"frozen-wins".to_vec()))
            .expect("frozen wins")
            .as_ref(),
        &frozen_wins,
        BranchRowSource::Frozen { index: 0 },
    );
    assert_visible_row(
        view.latest(&physical_key(branch, b"owned-beats-frozen".to_vec()))
            .expect("owned frozen")
            .as_ref(),
        &owned_rows[1],
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_visible_row(
        view.latest(&physical_key(branch, b"l1-only".to_vec()))
            .expect("l1 only")
            .as_ref(),
        &l1_only,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );
}

#[test]
fn branch_immutable_version_reads_cover_tombstone_bounds() {
    let branch = branch_id(63);
    let mut state = BranchLocalState::empty(branch);
    let deleted_put = storage_row_with(
        branch,
        b"deleted".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"before-delete".to_vec(),
    );
    let tombstone_above_put = storage_row_with(
        branch,
        b"tombstone-above".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"visible-before-tombstone".to_vec(),
    );
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "version-tombstone-l0",
            vec![
                deleted_put.clone(),
                tombstone_row(branch, b"deleted".to_vec(), 3, 30),
                tombstone_above_put.clone(),
                tombstone_row(branch, b"tombstone-above".to_vec(), 5, 50),
            ],
        ))
        .expect("install version table");
    let view = state.capture_read_view().expect("view");

    assert_visible_row(
        view.at_version(
            &physical_key(branch, b"deleted".to_vec()),
            CommitVersion::new(2),
        )
        .expect("before delete")
        .as_ref(),
        &deleted_put,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert!(view
        .at_version(
            &physical_key(branch, b"deleted".to_vec()),
            CommitVersion::new(3),
        )
        .expect("at delete")
        .is_none());
    assert_visible_row(
        view.at_version(
            &physical_key(branch, b"tombstone-above".to_vec()),
            CommitVersion::new(4),
        )
        .expect("below tombstone")
        .as_ref(),
        &tombstone_above_put,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert!(view
        .at_version(
            &physical_key(branch, b"tombstone-above".to_vec()),
            CommitVersion::new(5),
        )
        .expect("at tombstone")
        .is_none());
}

#[test]
fn branch_immutable_version_reads_cover_zero_and_max_commit_bounds() {
    let branch = branch_id(64);
    let mut state = BranchLocalState::empty(branch);
    let zero = storage_row_with(
        branch,
        b"zero-owned".to_vec(),
        0,
        0,
        Timestamp::EPOCH,
        b"zero".to_vec(),
    );
    let max = storage_row_with(
        branch,
        b"max-owned".to_vec(),
        u64::MAX,
        90,
        Timestamp::EPOCH,
        b"max".to_vec(),
    );
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "version-extremes-l0",
            vec![zero.clone(), max.clone()],
        ))
        .expect("install version extremes");
    let view = state.capture_read_view().expect("view");

    assert_visible_row(
        view.at_version(
            &physical_key(branch, b"zero-owned".to_vec()),
            CommitVersion::ZERO,
        )
        .expect("zero")
        .as_ref(),
        &zero,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_visible_row(
        view.at_version(
            &physical_key(branch, b"max-owned".to_vec()),
            CommitVersion::MAX,
        )
        .expect("max")
        .as_ref(),
        &max,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
}

#[test]
fn branch_immutable_history_filters_tombstones_limits_and_cross_level_versions() {
    let branch = branch_id(65);
    let mut state = BranchLocalState::empty(branch);
    let key = physical_key(branch, b"owned-history".to_vec());
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "history-l1",
                vec![storage_row_with(
                    branch,
                    b"owned-history".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    b"old".to_vec(),
                )],
            ),
        )
        .expect("install history L1");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "history-l0",
            vec![
                storage_row_with(
                    branch,
                    b"owned-history".to_vec(),
                    3,
                    30,
                    Timestamp::EPOCH,
                    b"new".to_vec(),
                ),
                tombstone_row(branch, b"owned-history".to_vec(), 2, 20),
            ],
        ))
        .expect("install history L0");
    let view = state.capture_read_view().expect("view");

    assert_eq!(
        history_versions(
            &view
                .history(&key, BranchHistoryOptions::all())
                .expect("all")
        ),
        vec![3, 2, 1]
    );
    assert_eq!(
        history_versions(
            &view
                .history(&key, BranchHistoryOptions::all().include_tombstones(false))
                .expect("without tombstones")
        ),
        vec![3, 1]
    );
    assert_eq!(
        history_versions(
            &view
                .history(
                    &key,
                    BranchHistoryOptions::all().before_version(CommitVersion::new(3)),
                )
                .expect("before three")
        ),
        vec![2, 1]
    );
    assert!(view
        .history(&key, BranchHistoryOptions::all().limit(0))
        .expect("limit zero")
        .is_empty());
    assert_eq!(
        history_versions(
            &view
                .history(
                    &key,
                    BranchHistoryOptions::all()
                        .include_tombstones(false)
                        .limit(1),
                )
                .expect("filtered limit")
        ),
        vec![3]
    );
}

#[test]
fn branch_immutable_prefix_scans_merge_sources_and_respect_spaces() {
    let branch = branch_id(66);
    let mut state = BranchLocalState::empty(branch);
    let frozen = storage_row_with(
        branch,
        b"scan-b".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"frozen".to_vec(),
    );
    let active = storage_row_with(
        branch,
        b"scan-a".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"active".to_vec(),
    );
    state
        .append_committed_row(frozen.clone())
        .expect("append frozen");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .append_committed_row(active.clone())
        .expect("append active");
    let scan_c_new = storage_row_with(
        branch,
        b"scan-c".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"new-c".to_vec(),
    );
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "scan-l0-new",
            vec![
                scan_c_new.clone(),
                tombstone_row(branch, b"scan-d".to_vec(), 6, 60),
            ],
        ))
        .expect("install scan L0 new");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "scan-l0-old",
            vec![
                storage_row_with(
                    branch,
                    b"scan-c".to_vec(),
                    2,
                    20,
                    Timestamp::EPOCH,
                    b"old-c".to_vec(),
                ),
                storage_row_with(
                    branch,
                    b"scan-d".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    b"old-d".to_vec(),
                ),
                storage_row_with_named_space(
                    branch,
                    "other-space",
                    StorageSpaceId::engine(0x20).expect("engine space"),
                    b"scan-a".to_vec(),
                    9,
                    90,
                    b"other-space".to_vec(),
                ),
            ],
        ))
        .expect("install scan L0 old");

    let view = state.capture_read_view().expect("view");
    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"scan-".to_vec()));
    let prefix_rows = view
        .scan_prefix(&prefix, BranchReadBound::latest())
        .expect("prefix scan");
    assert_eq!(
        scan_user_keys(&prefix_rows),
        vec![b"scan-a".to_vec(), b"scan-b".to_vec(), b"scan-c".to_vec(),]
    );
    assert_eq!(prefix_rows[0].row(), &active);
    assert_eq!(prefix_rows[1].row(), &frozen);
    assert_eq!(prefix_rows[2].row(), &scan_c_new);
}

#[test]
fn branch_immutable_prefix_scan_includes_l1_and_excludes_storage_space_id() {
    let branch = branch_id(69);
    let mut state = BranchLocalState::empty(branch);
    let l1_row = storage_row_with(
        branch,
        b"scan-l1".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"l1".to_vec(),
    );
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "scan-other-storage-space",
            vec![storage_row_with_named_space(
                branch,
                "default",
                StorageSpaceId::engine(0x21).expect("engine space"),
                b"scan-l1".to_vec(),
                2,
                20,
                b"other-storage-space".to_vec(),
            )],
        ))
        .expect("install other storage-space L0");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "scan-l1-default-space",
                vec![l1_row.clone()],
            ),
        )
        .expect("install L1");

    let view = state.capture_read_view().expect("view");
    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"scan-".to_vec()));
    let rows = view
        .scan_prefix(&prefix, BranchReadBound::latest())
        .expect("prefix scan");
    assert_eq!(scan_user_keys(&rows), vec![b"scan-l1".to_vec()]);
    assert_visible_row(
        rows.first(),
        &l1_row,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );
}

#[test]
fn branch_immutable_range_scans_cover_l1_edge_and_degenerate_bounds() {
    let branch = branch_id(67);
    let mut state = BranchLocalState::empty(branch);
    let scan_e = storage_row_with(
        branch,
        b"scan-e".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"e".to_vec(),
    );
    let scan_g = storage_row_with(
        branch,
        b"scan-g".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"g".to_vec(),
    );
    let scan_h = storage_row_with(
        branch,
        b"scan-h".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"h".to_vec(),
    );
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "scan-l1-e-g",
                vec![scan_e, scan_g],
            ),
        )
        .expect("install scan L1 e-g");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(branch, BranchLevel::new(1), "scan-l1-h", vec![scan_h]),
        )
        .expect("install scan L1 h");

    let view = state.capture_read_view().expect("view");
    let closed = BranchScanBounds::closed(
        &physical_key(branch, b"scan-e".to_vec()),
        &physical_key(branch, b"scan-g".to_vec()),
    )
    .expect("closed range");
    assert_eq!(
        scan_user_keys(
            &view
                .scan_range(&closed, BranchReadBound::latest())
                .expect("closed range")
        ),
        vec![b"scan-e".to_vec(), b"scan-g".to_vec()]
    );
    let open = BranchScanBounds::open(
        &physical_key(branch, b"scan-e".to_vec()),
        &physical_key(branch, b"scan-g".to_vec()),
    )
    .expect("open range");
    assert!(view
        .scan_range(&open, BranchReadBound::latest())
        .expect("open range")
        .is_empty());
    let degenerate = BranchScanBounds::closed(
        &physical_key(branch, b"scan-h".to_vec()),
        &physical_key(branch, b"scan-h".to_vec()),
    )
    .expect("degenerate range");
    assert_eq!(
        scan_user_keys(
            &view
                .scan_range(&degenerate, BranchReadBound::latest())
                .expect("degenerate range")
        ),
        vec![b"scan-h".to_vec()]
    );
}

#[test]
fn branch_borrowed_prefix_scan_matches_read_view_across_local_sources() {
    let branch = branch_id(111);
    let mut state = BranchLocalState::empty(branch);
    let frozen = storage_row_with(
        branch,
        b"borrowed-b".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"frozen".to_vec(),
    );
    let active = storage_row_with(
        branch,
        b"borrowed-a".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"active".to_vec(),
    );
    let l0 = storage_row_with(
        branch,
        b"borrowed-c".to_vec(),
        6,
        60,
        Timestamp::EPOCH,
        b"l0".to_vec(),
    );
    let l1 = storage_row_with(
        branch,
        b"borrowed-d".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"l1".to_vec(),
    );

    state.append_committed_row(frozen).expect("append frozen");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state.append_committed_row(active).expect("append active");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "borrowed-local-l0",
            vec![l0],
        ))
        .expect("install L0");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(branch, BranchLevel::new(1), "borrowed-local-l1", vec![l1]),
        )
        .expect("install L1");

    let bounds = BranchScanBounds::prefix(&physical_key(branch, b"borrowed-".to_vec()));
    let read_view_rows = state
        .capture_read_view()
        .expect("view")
        .scan_prefix_including_tombstones(&bounds, BranchReadBound::latest())
        .expect("read-view scan");
    let borrowed_rows = state
        .scan_including_tombstones_borrowed(&bounds, BranchReadBound::latest(), None, None)
        .expect("borrowed scan");

    assert_eq!(borrowed_rows, read_view_rows);
    assert_eq!(
        history_user_keys(&borrowed_rows),
        vec![
            b"borrowed-a".to_vec(),
            b"borrowed-b".to_vec(),
            b"borrowed-c".to_vec(),
            b"borrowed-d".to_vec(),
        ]
    );
}

#[test]
fn branch_borrowed_range_scan_applies_visible_limit_like_read_view() {
    let branch = branch_id(112);
    let mut state = BranchLocalState::empty(branch);
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "borrowed-visible-limit",
            vec![
                storage_row_with(
                    branch,
                    b"limit-a".to_vec(),
                    1,
                    10,
                    Timestamp::from_micros(5),
                    b"expired".to_vec(),
                ),
                storage_row_with(
                    branch,
                    b"limit-b".to_vec(),
                    2,
                    20,
                    Timestamp::EPOCH,
                    b"visible-one".to_vec(),
                ),
                tombstone_row(branch, b"limit-c".to_vec(), 3, 30),
                storage_row_with(
                    branch,
                    b"limit-d".to_vec(),
                    4,
                    40,
                    Timestamp::EPOCH,
                    b"visible-two".to_vec(),
                ),
                storage_row_with(
                    branch,
                    b"limit-e".to_vec(),
                    5,
                    50,
                    Timestamp::EPOCH,
                    b"after-limit".to_vec(),
                ),
            ],
        ))
        .expect("install limit table");

    let bounds = BranchScanBounds::closed(
        &physical_key(branch, b"limit-a".to_vec()),
        &physical_key(branch, b"limit-e".to_vec()),
    )
    .expect("range bounds");
    let limit_timestamp = Timestamp::from_micros(10);
    let read_view_rows = state
        .capture_read_view()
        .expect("view")
        .scan_range_including_tombstones(&bounds, BranchReadBound::latest())
        .expect("read-view scan");
    let expected_rows = expected_borrowed_visible_limit_rows(&read_view_rows, 2, limit_timestamp);
    let borrowed_rows = state
        .scan_including_tombstones_borrowed(
            &bounds,
            BranchReadBound::latest(),
            Some(2),
            Some(limit_timestamp),
        )
        .expect("borrowed scan");

    assert_eq!(borrowed_rows, expected_rows);
    assert_eq!(
        history_user_keys(&borrowed_rows),
        vec![
            b"limit-a".to_vec(),
            b"limit-b".to_vec(),
            b"limit-c".to_vec(),
            b"limit-d".to_vec(),
        ]
    );
}

#[expect(
    clippy::too_many_lines,
    reason = "single fixture pins source-order ties across active, frozen, L0, and L1"
)]
#[test]
fn branch_borrowed_scan_preserves_source_order_ties() {
    let branch = branch_id(113);
    let mut state = BranchLocalState::empty(branch);
    let frozen_active_tie = storage_row_with(
        branch,
        b"tie-active".to_vec(),
        10,
        10,
        Timestamp::EPOCH,
        b"frozen-loses".to_vec(),
    );
    let active_winner = storage_row_with(
        branch,
        b"tie-active".to_vec(),
        10,
        10,
        Timestamp::EPOCH,
        b"active-wins".to_vec(),
    );
    let frozen_owned_tie = storage_row_with(
        branch,
        b"tie-frozen-owned".to_vec(),
        8,
        8,
        Timestamp::EPOCH,
        b"frozen-wins".to_vec(),
    );
    let l0_l1_winner = storage_row_with(
        branch,
        b"tie-l0-l1".to_vec(),
        7,
        7,
        Timestamp::EPOCH,
        b"l0-wins".to_vec(),
    );

    state
        .append_committed_row(frozen_active_tie)
        .expect("append frozen active tie");
    state
        .append_committed_row(frozen_owned_tie)
        .expect("append frozen owned tie");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .append_committed_row(active_winner)
        .expect("append active winner");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "borrowed-source-tie-l0",
            vec![
                storage_row_with(
                    branch,
                    b"tie-active".to_vec(),
                    10,
                    10,
                    Timestamp::EPOCH,
                    b"l0-loses-active".to_vec(),
                ),
                storage_row_with(
                    branch,
                    b"tie-frozen-owned".to_vec(),
                    8,
                    8,
                    Timestamp::EPOCH,
                    b"l0-loses-frozen".to_vec(),
                ),
                l0_l1_winner,
            ],
        ))
        .expect("install L0");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "borrowed-source-tie-l1",
                vec![storage_row_with(
                    branch,
                    b"tie-l0-l1".to_vec(),
                    7,
                    7,
                    Timestamp::EPOCH,
                    b"l1-loses".to_vec(),
                )],
            ),
        )
        .expect("install L1");

    let bounds = BranchScanBounds::prefix(&physical_key(branch, b"tie-".to_vec()));
    let read_view_rows = state
        .capture_read_view()
        .expect("view")
        .scan_prefix_including_tombstones(&bounds, BranchReadBound::latest())
        .expect("read-view scan");
    let borrowed_rows = state
        .scan_including_tombstones_borrowed(&bounds, BranchReadBound::latest(), None, None)
        .expect("borrowed scan");

    assert_eq!(borrowed_rows, read_view_rows);
    assert_eq!(
        borrowed_rows
            .iter()
            .map(BranchHistoryRow::source)
            .collect::<Vec<_>>(),
        vec![
            BranchRowSource::Active,
            BranchRowSource::Frozen { index: 0 },
            BranchRowSource::OwnedTable {
                level: BranchLevel::ZERO,
                table_index: 0,
            },
        ]
    );
}

fn expected_borrowed_visible_limit_rows(
    rows: &[BranchHistoryRow],
    visible_limit: usize,
    visible_limit_timestamp: Timestamp,
) -> Vec<BranchHistoryRow> {
    let mut limited = Vec::new();
    let mut visible_rows = 0usize;
    for row in rows {
        let counts_for_limit = !row.row().is_tombstone()
            && (row.row().expires_at() == Timestamp::EPOCH
                || row.row().expires_at() > visible_limit_timestamp);
        limited.push(row.clone());
        if counts_for_limit {
            visible_rows = visible_rows.saturating_add(1);
            if visible_rows >= visible_limit {
                break;
            }
        }
    }
    limited
}
