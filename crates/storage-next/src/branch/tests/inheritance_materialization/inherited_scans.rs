#[test]
fn branch_inherited_history_filters_tombstones_limits_and_fork_gates() {
    let source = branch_id(89);
    let child = branch_id(90);
    let key = b"history-inherited".to_vec();
    let post_fork = storage_row_with(
        source,
        key.clone(),
        5,
        50,
        Timestamp::EPOCH,
        b"post-fork-secret".to_vec(),
    );
    let tombstone = tombstone_row(source, key.clone(), 4, 40);
    let visible = storage_row_with(
        source,
        key.clone(),
        3,
        30,
        Timestamp::from_micros(300),
        b"visible".to_vec(),
    );
    let older = storage_row_with(
        source,
        key.clone(),
        1,
        10,
        Timestamp::EPOCH,
        b"older".to_vec(),
    );
    let layer = branch_inherited_layer_unchecked_for_fork_gate_tests(
        source,
        CommitVersion::new(4),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "inherited-history",
            vec![post_fork, tombstone.clone(), visible.clone(), older.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited history layer");
    let view = child_state.capture_read_view().expect("view");
    let child_key = physical_key(child, key);

    assert!(
        view.latest(&child_key).expect("latest").is_none(),
        "selected inherited tombstone shadows older inherited puts"
    );

    let all = view
        .history(&child_key, BranchHistoryOptions::all())
        .expect("all inherited history");
    assert_eq!(history_versions(&all), vec![4, 3, 1]);
    assert!(all[0].row().is_tombstone());
    assert_eq!(
        all[1].source(),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        }
    );
    assert_eq!(
        all[1].row(),
        &rewrite_row_branch(&visible, source, child).expect("visible rewrite")
    );

    let without_tombstones = view
        .history(
            &child_key,
            BranchHistoryOptions::all().include_tombstones(false),
        )
        .expect("history without tombstones");
    assert_eq!(history_versions(&without_tombstones), vec![3, 1]);

    let before_fork = view
        .history(
            &child_key,
            BranchHistoryOptions::all().before_version(CommitVersion::new(4)),
        )
        .expect("history before tombstone");
    assert_eq!(history_versions(&before_fork), vec![3, 1]);

    let limited_after_filter = view
        .history(
            &child_key,
            BranchHistoryOptions::all()
                .include_tombstones(false)
                .limit(1),
        )
        .expect("limited inherited history");
    assert_eq!(history_versions(&limited_after_filter), vec![3]);
    assert_eq!(
        limited_after_filter[0].row(),
        &rewrite_row_branch(&visible, source, child).expect("limited rewrite")
    );
    assert!(
        history_versions(&all).iter().all(|version| *version <= 4),
        "rows above the fork version must stay out of history"
    );
}

#[test]
fn branch_inherited_l0_overlap_and_l1_tables_participate_in_point_reads() {
    let source = branch_id(91);
    let child = branch_id(92);
    let overlapping_key = b"overlap".to_vec();
    let old_l0 = storage_row_with(
        source,
        overlapping_key.clone(),
        2,
        20,
        Timestamp::EPOCH,
        b"old-l0".to_vec(),
    );
    let new_l0 = storage_row_with(
        source,
        overlapping_key.clone(),
        5,
        50,
        Timestamp::EPOCH,
        b"new-l0".to_vec(),
    );
    let l1_row = storage_row_with(
        source,
        b"from-l1".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"l1".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![
            vec![
                branch_owned_table(
                    source,
                    BranchLevel::ZERO,
                    "inherited-overlap-l0-new",
                    vec![new_l0.clone()],
                ),
                branch_owned_table(
                    source,
                    BranchLevel::ZERO,
                    "inherited-overlap-l0-old",
                    vec![old_l0],
                ),
            ],
            vec![branch_owned_table(
                source,
                BranchLevel::new(1),
                "inherited-l1-point",
                vec![l1_row.clone()],
            )],
        ],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited overlap layer");
    let view = child_state.capture_read_view().expect("view");

    assert_visible_row(
        view.latest(&physical_key(child, overlapping_key))
            .expect("overlap latest")
            .as_ref(),
        &rewrite_row_branch(&new_l0, source, child).expect("new L0 rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    assert_visible_row(
        view.latest(&physical_key(child, b"from-l1".to_vec()))
            .expect("L1 latest")
            .as_ref(),
        &rewrite_row_branch(&l1_row, source, child).expect("L1 rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
}

#[test]
fn branch_child_owned_table_shadows_inherited_exact_duplicate_key() {
    let source = branch_id(81);
    let child = branch_id(82);
    let source_row = storage_row_with(
        source,
        b"exact-duplicate".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"source".to_vec(),
    );
    let child_row = storage_row_with(
        child,
        b"exact-duplicate".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"child-owned".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "exact-duplicate-inherited",
            vec![source_row],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited");
    child_state
        .install_l0_table(branch_owned_table(
            child,
            BranchLevel::ZERO,
            "exact-duplicate-owned",
            vec![child_row.clone()],
        ))
        .expect("install child-owned shadow");

    let visible = child_state
        .capture_read_view()
        .expect("view")
        .latest(&physical_key(child, b"exact-duplicate".to_vec()))
        .expect("latest")
        .expect("child-owned row");
    assert_eq!(visible.row(), &child_row);
    assert_eq!(
        visible.source(),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        }
    );
}

#[test]
fn branch_inherited_scans_and_history_rewrite_before_grouping() {
    let source = branch_id(76);
    let child = branch_id(77);
    let source_put = storage_row_with(
        source,
        b"scan-a".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"source".to_vec(),
    );
    let source_tombstone = StorageRow::tombstone(
        physical_key(source, b"scan-b".to_vec()),
        CommitVersion::new(4),
        Timestamp::from_micros(40),
    );
    let child_put = storage_row_with(
        child,
        b"scan-a".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"child".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(4),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "scan-inherited",
            vec![source_put.clone(), source_tombstone.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited");
    child_state
        .append_committed_row(child_put.clone())
        .expect("child put");

    let view = child_state.capture_read_view().expect("view");
    let bounds = BranchScanBounds::prefix(&physical_key(child, b"scan-".to_vec()));
    let scan = view
        .scan_prefix(&bounds, BranchReadBound::latest())
        .expect("scan");
    assert_eq!(scan_user_keys(&scan), vec![b"scan-a".to_vec()]);
    assert_eq!(scan[0].row(), &child_put);
    assert_eq!(scan[0].source(), BranchRowSource::Active);

    let history = view
        .history(
            &physical_key(child, b"scan-a".to_vec()),
            BranchHistoryOptions::all(),
        )
        .expect("history");
    assert_eq!(history_versions(&history), vec![5, 2]);
    assert_eq!(history[0].source(), BranchRowSource::Active);
    assert_eq!(
        history[1].source(),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        }
    );
    assert_eq!(
        history[1].row(),
        &rewrite_row_branch(&source_put, source, child).expect("source rewrite")
    );
}

#[test]
fn branch_borrowed_latest_scan_matches_read_view_with_inherited_sources() {
    let source = branch_id(98);
    let child = branch_id(99);
    let source_a = storage_row_with(
        source,
        b"fast-scan-a".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"source-a".to_vec(),
    );
    let source_b_tombstone = tombstone_row(source, b"fast-scan-b".to_vec(), 4, 40);
    let source_c = storage_row_with(
        source,
        b"fast-scan-c".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"source-c".to_vec(),
    );
    let child_a = storage_row_with(
        child,
        b"fast-scan-a".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"child-a".to_vec(),
    );
    let child_d = storage_row_with(
        child,
        b"fast-scan-d".to_vec(),
        6,
        60,
        Timestamp::EPOCH,
        b"child-d".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(4),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "fast-scan-inherited",
            vec![source_a, source_b_tombstone.clone(), source_c.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited");
    child_state
        .append_committed_row(child_a.clone())
        .expect("child active row");
    child_state
        .install_l0_table(branch_owned_table(
            child,
            BranchLevel::ZERO,
            "fast-scan-owned",
            vec![child_d],
        ))
        .expect("child owned row");

    let bounds = BranchScanBounds::prefix(&physical_key(child, b"fast-scan-".to_vec()));
    let view = child_state.capture_read_view().expect("view");
    let read_view_rows = view
        .scan_prefix_including_tombstones(&bounds, BranchReadBound::latest())
        .expect("read-view scan");
    let borrowed_rows = child_state
        .scan_including_tombstones_borrowed(&bounds, BranchReadBound::latest(), Some(2), None)
        .expect("borrowed scan");

    assert_eq!(
        borrowed_rows
            .iter()
            .map(|row| row.row())
            .collect::<Vec<_>>(),
        read_view_rows
            .iter()
            .take(3)
            .map(|row| row.row())
            .collect::<Vec<_>>(),
        "borrowed scan keeps the same visible ordering and includes tombstones before the visible limit"
    );
    assert_eq!(
        borrowed_rows
            .iter()
            .map(BranchHistoryRow::source)
            .collect::<Vec<_>>(),
        vec![
            BranchRowSource::Active,
            BranchRowSource::Inherited {
                source_branch_id: source,
                layer_index: 0,
            },
            BranchRowSource::Inherited {
                source_branch_id: source,
                layer_index: 0,
            },
        ]
    );
    assert_eq!(borrowed_rows[0].row(), &child_a);
    assert!(borrowed_rows[1].row().is_tombstone());
    assert_eq!(
        borrowed_rows[2].row(),
        &rewrite_row_branch(&source_c, source, child).expect("source c rewrite")
    );
}

#[test]
fn branch_borrowed_latest_scan_applies_inherited_fork_bound() {
    let source = branch_id(100);
    let child = branch_id(101);
    let post_fork = storage_row_with(
        source,
        b"fast-fork-a".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"post-fork".to_vec(),
    );
    let at_fork = storage_row_with(
        source,
        b"fast-fork-a".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"at-fork".to_vec(),
    );
    let layer = branch_inherited_layer_unchecked_for_fork_gate_tests(
        source,
        CommitVersion::new(4),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "fast-fork-inherited",
            vec![post_fork, at_fork.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited");

    let bounds = BranchScanBounds::prefix(&physical_key(child, b"fast-fork-".to_vec()));
    let borrowed_rows = child_state
        .scan_including_tombstones_borrowed(&bounds, BranchReadBound::latest(), None, None)
        .expect("borrowed scan");

    assert_eq!(history_versions(&borrowed_rows), vec![4]);
    assert_eq!(
        borrowed_rows[0].row(),
        &rewrite_row_branch(&at_fork, source, child).expect("at-fork rewrite")
    );
}

#[test]
fn branch_inherited_scans_preserve_space_boundaries() {
    let fixture = inherited_scan_boundary_fixture();
    let closed = fixture
        .view
        .scan_range(
            &BranchScanBounds::closed(&fixture.lower, &fixture.upper).expect("closed bounds"),
            BranchReadBound::latest(),
        )
        .expect("closed inherited scan");
    assert_eq!(
        scan_user_keys(&closed),
        vec![b"scan-a".to_vec(), b"scan-b".to_vec(), b"scan-c".to_vec()]
    );
    assert!(closed.iter().all(|row| {
        row.row().physical_key().space() == "default"
            && row.row().physical_key().storage_space_id() == fixture.engine_space
    }));
    let system_rows = fixture
        .view
        .scan_prefix(
            &BranchScanBounds::prefix(&physical_key_with(
                fixture.child,
                "system",
                fixture.engine_space,
                b"scan-".to_vec(),
            )),
            BranchReadBound::latest(),
        )
        .expect("system prefix scan");
    assert_eq!(system_rows.len(), 1);
    assert_eq!(system_rows[0].row().physical_key().space(), "system");
    assert_eq!(
        system_rows[0].row().physical_key().storage_space_id(),
        fixture.engine_space
    );
    assert_eq!(system_rows[0].row().physical_key().user_key(), b"scan-b");
}

#[test]
fn branch_inherited_scans_preserve_range_edges() {
    let fixture = inherited_scan_boundary_fixture();
    let open = fixture
        .view
        .scan_range(
            &BranchScanBounds::open(&fixture.lower, &fixture.upper).expect("open bounds"),
            BranchReadBound::latest(),
        )
        .expect("open inherited scan");
    assert_eq!(scan_user_keys(&open), vec![b"scan-b".to_vec()]);
    assert_eq!(
        open[0].row(),
        &rewrite_row_branch(&fixture.scan_b, fixture.source, fixture.child)
            .expect("middle rewrite")
    );

    let closed_degenerate = fixture
        .view
        .scan_range(
            &BranchScanBounds::closed(&fixture.middle, &fixture.middle).expect("closed degenerate"),
            BranchReadBound::latest(),
        )
        .expect("closed degenerate scan");
    assert_eq!(scan_user_keys(&closed_degenerate), vec![b"scan-b".to_vec()]);

    let open_degenerate = fixture
        .view
        .scan_range(
            &BranchScanBounds::open(&fixture.middle, &fixture.middle).expect("open degenerate"),
            BranchReadBound::latest(),
        )
        .expect("open degenerate scan");
    assert!(open_degenerate.is_empty());
}

#[test]
fn branch_inherited_rejects_wrong_branch_before_timestamp_reads_without_payload() {
    let source = branch_id(95);
    let child = branch_id(96);
    let row = storage_row_with(
        source,
        b"reject".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"secret-payload".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(2),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "inherited-reject-secret",
            vec![row],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let view = child_state.capture_read_view().expect("view");

    let wrong_branch_error = view
        .latest(&physical_key(branch_id(97), b"reject".to_vec()))
        .expect_err("wrong branch rejected before inherited lookup");
    assert!(matches!(
        wrong_branch_error,
        BranchRuntimeError::InvalidBranchRow { .. }
    ));
    assert!(!wrong_branch_error.to_string().contains("secret-payload"));

    let timestamp_row = view
        .read_point(
            &physical_key(child, b"reject".to_vec()),
            BranchReadBound::at_timestamp(Timestamp::from_micros(20)),
        )
        .expect("timestamp inherited read")
        .expect("inherited timestamp row");
    assert_eq!(
        timestamp_row.row(),
        &rewrite_row_branch(
            &storage_row_with(
                source,
                b"reject".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"secret-payload".to_vec(),
            ),
            source,
            child,
        )
        .expect("expected rewrite")
    );
}

#[test]
fn branch_chained_fork_prefers_nearest_inherited_layer_for_exact_ties() {
    let grandparent = branch_id(78);
    let parent = branch_id(79);
    let child = branch_id(80);
    let key = b"tie".to_vec();
    let grandparent_row = storage_row_with(
        grandparent,
        key.clone(),
        5,
        50,
        Timestamp::EPOCH,
        b"grandparent".to_vec(),
    );
    let parent_row = storage_row_with(
        parent,
        key.clone(),
        5,
        50,
        Timestamp::EPOCH,
        b"parent".to_vec(),
    );

    let mut grandparent_state = BranchLocalState::empty(grandparent);
    grandparent_state
        .install_l0_table(branch_owned_table(
            grandparent,
            BranchLevel::ZERO,
            "grandparent-tie",
            vec![grandparent_row],
        ))
        .expect("grandparent install");
    let (mut parent_state, _) = grandparent_state
        .fork_into_empty_child(parent)
        .expect("fork parent");
    parent_state
        .install_l0_table(branch_owned_table(
            parent,
            BranchLevel::ZERO,
            "parent-tie",
            vec![parent_row.clone()],
        ))
        .expect("parent install");

    let (child_state, outcome) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    assert_eq!(outcome.inherited_layer_count(), 2);
    let view = child_state.capture_read_view().expect("view");
    let visible = view
        .latest(&physical_key(child, key))
        .expect("latest")
        .expect("nearest inherited row");
    assert_eq!(
        visible.row(),
        &rewrite_row_branch(&parent_row, parent, child).expect("parent rewrite")
    );
    assert_eq!(
        visible.source(),
        BranchRowSource::Inherited {
            source_branch_id: parent,
            layer_index: 0,
        }
    );
}

struct ForkStatusFixture {
    grandparent: BranchId,
    source: BranchId,
    child: BranchId,
    source_owned: StorageRow,
    inherited_row: StorageRow,
    child_state: BranchLocalState,
    outcome: BranchForkOutcome,
}

fn fork_status_fixture() -> ForkStatusFixture {
    let grandparent = branch_id(83);
    let materialized_source = branch_id(84);
    let source = branch_id(85);
    let child = branch_id(86);
    let inherited_row = storage_row_with(
        grandparent,
        b"materializing".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"grandparent".to_vec(),
    );
    let source_owned = storage_row_with(
        source,
        b"source-owned".to_vec(),
        6,
        60,
        Timestamp::EPOCH,
        b"source".to_vec(),
    );
    let materializing = branch_inherited_layer(
        grandparent,
        CommitVersion::new(3),
        InheritedLayerStatus::Materializing,
        vec![vec![branch_owned_table(
            grandparent,
            BranchLevel::ZERO,
            "fork-materializing",
            vec![inherited_row.clone()],
        )]],
    );
    let materialized = branch_inherited_layer(
        materialized_source,
        CommitVersion::new(2),
        InheritedLayerStatus::Materialized,
        vec![vec![branch_owned_table(
            materialized_source,
            BranchLevel::ZERO,
            "fork-materialized",
            vec![storage_row_with(
                materialized_source,
                b"materialized".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"old-materialized".to_vec(),
            )],
        )]],
    );
    let mut source_state = BranchLocalState::empty(source);
    source_state
        .attach_inherited_layers(vec![materializing, materialized])
        .expect("attach source inherited layers");
    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "fork-source-owned-status",
            vec![source_owned.clone()],
        ))
        .expect("install source-owned table after inherited attach");
    let (child_state, outcome) = source_state
        .fork_into_empty_child(child)
        .expect("fork child with inherited layers");
    ForkStatusFixture {
        grandparent,
        source,
        child,
        source_owned,
        inherited_row,
        child_state,
        outcome,
    }
}

struct InheritedScanBoundaryFixture {
    source: BranchId,
    child: BranchId,
    engine_space: StorageSpaceId,
    view: BranchReadView,
    lower: PhysicalKey,
    middle: PhysicalKey,
    upper: PhysicalKey,
    scan_b: StorageRow,
}

fn inherited_scan_boundary_fixture() -> InheritedScanBoundaryFixture {
    let source = branch_id(93);
    let child = branch_id(94);
    let engine_space = StorageSpaceId::engine(0x21).expect("engine space");
    let system_space = StorageSpaceId::COMMIT_TIMELINE;
    let scan_a = storage_row_with_named_space(
        source,
        "default",
        engine_space,
        b"scan-a".to_vec(),
        2,
        20,
        b"a".to_vec(),
    );
    let scan_b = storage_row_with_named_space(
        source,
        "default",
        engine_space,
        b"scan-b".to_vec(),
        3,
        30,
        b"b".to_vec(),
    );
    let scan_c = storage_row_with_named_space(
        source,
        "default",
        engine_space,
        b"scan-c".to_vec(),
        4,
        40,
        b"c".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(6),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "inherited-scan-boundaries",
            vec![
                scan_a,
                scan_b.clone(),
                scan_c,
                storage_row_with_named_space(
                    source,
                    "system",
                    engine_space,
                    b"scan-b".to_vec(),
                    5,
                    50,
                    b"wrong-name".to_vec(),
                ),
                storage_row_with_named_space(
                    source,
                    "default",
                    system_space,
                    b"scan-b".to_vec(),
                    6,
                    60,
                    b"wrong-storage".to_vec(),
                ),
            ],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited scan layer");
    InheritedScanBoundaryFixture {
        source,
        child,
        engine_space,
        view: child_state.capture_read_view().expect("view"),
        lower: physical_key_with(child, "default", engine_space, b"scan-a".to_vec()),
        middle: physical_key_with(child, "default", engine_space, b"scan-b".to_vec()),
        upper: physical_key_with(child, "default", engine_space, b"scan-c".to_vec()),
        scan_b,
    }
}
