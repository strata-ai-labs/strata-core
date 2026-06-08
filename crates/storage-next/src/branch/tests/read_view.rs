use super::*;

#[test]
fn branch_read_view_is_pinned_across_append_and_rotation() {
    let branch = branch_id(38);
    let mut state = BranchLocalState::empty(branch);
    let first = storage_row_with(
        branch,
        b"pinned".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"first".to_vec(),
    );
    let tombstone = tombstone_row(branch, b"pinned".to_vec(), 2, 20);

    state
        .append_committed_row(first.clone())
        .expect("append first");
    let view = state.capture_read_view().expect("capture read view");
    let captured_facts = view.facts();

    state
        .append_committed_row(tombstone.clone())
        .expect("append tombstone after capture");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    let after = state.capture_read_view().expect("capture after mutation");

    let key = physical_key(branch, b"pinned".to_vec());
    let visible = view.latest(&key).expect("pinned latest").expect("row");
    assert_eq!(visible.row(), &first);
    assert_eq!(visible.source(), BranchRowSource::Active);
    assert_eq!(view.facts(), captured_facts);
    assert_eq!(view.active_row_count(), 1);
    assert_eq!(view.frozen_table_count(), 0);

    assert_eq!(after.latest(&key).expect("after latest"), None);
    assert_eq!(after.active_row_count(), 0);
    assert_eq!(after.frozen_table_count(), 1);
    assert_eq!(
        after
            .history(&key, BranchHistoryOptions::all())
            .expect("after history")
            .iter()
            .map(|row| row.row().commit_version().as_u64())
            .collect::<Vec<_>>(),
        vec![2, 1],
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_read_view_capture_pins_source_handles_without_row_copies() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(147);
    let parent = branch_id(148);
    let inherited_row = storage_row_with(
        parent,
        b"capture-parent".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"inherited".to_vec(),
    );
    let inherited_layer = branch_inherited_layer(
        parent,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            parent,
            BranchLevel::ZERO,
            "read-view-capture-inherited",
            vec![inherited_row.clone()],
        )]],
    );
    let mut state = BranchLocalState::empty(branch);
    state
        .attach_inherited_layers(vec![inherited_layer])
        .expect("attach inherited layer");

    let frozen_row = storage_row_with(
        branch,
        b"capture-frozen".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"frozen".to_vec(),
    );
    let active_row = storage_row_with(
        branch,
        b"capture-active".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"active".to_vec(),
    );
    let owned_row = storage_row_with(
        branch,
        b"capture-owned".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"owned".to_vec(),
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
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "read-view-capture-owned",
            vec![owned_row.clone()],
        ))
        .expect("install owned table");

    let view = state.capture_read_view().expect("capture read view");
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.read_view_captures(), 1);
    assert_eq!(perf.read_view_source_handles_cloned(), 5);
    assert_eq!(perf.read_view_rows_cloned(), 0);
    assert_eq!(perf.read_view_row_clone_bytes(), 0);
    assert_eq!(perf.read_view_validation_rows_scanned(), 0);

    assert_visible_row(
        view.latest(&physical_key(branch, b"capture-active".to_vec()))
            .expect("active read")
            .as_ref(),
        &active_row,
        BranchRowSource::Active,
    );
    assert_visible_row(
        view.latest(&physical_key(branch, b"capture-frozen".to_vec()))
            .expect("frozen read")
            .as_ref(),
        &frozen_row,
        BranchRowSource::Frozen { index: 0 },
    );
    assert_visible_row(
        view.latest(&physical_key(branch, b"capture-owned".to_vec()))
            .expect("owned read")
            .as_ref(),
        &owned_row,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    let inherited_expected =
        rewrite_row_branch(&inherited_row, parent, branch).expect("rewrite inherited row");
    assert_visible_row(
        view.latest(&physical_key(branch, b"capture-parent".to_vec()))
            .expect("inherited read")
            .as_ref(),
        &inherited_expected,
        BranchRowSource::Inherited {
            source_branch_id: parent,
            layer_index: 0,
        },
    );
}

#[test]
fn branch_read_view_is_pinned_across_table_rewrites_and_inherited_materialization() {
    let branch = branch_id(149);
    let source = branch_id(150);
    let mut state = BranchLocalState::empty(branch);

    let inherited = storage_row_with(
        source,
        b"pinned-inherited".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"inherited".to_vec(),
    );
    state
        .attach_inherited_layers(vec![branch_inherited_layer(
            source,
            CommitVersion::new(5),
            InheritedLayerStatus::Active,
            vec![vec![branch_owned_table(
                source,
                BranchLevel::ZERO,
                "pinned-inherited-source",
                vec![inherited.clone()],
            )]],
        )])
        .expect("attach inherited layer");

    let frozen = storage_row_with(
        branch,
        b"pinned-flush".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"frozen".to_vec(),
    );
    state
        .append_committed_row(frozen.clone())
        .expect("append frozen row");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));

    let compact_newer = storage_row_with(
        branch,
        b"pinned-compact".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"newer".to_vec(),
    );
    let compact_older = storage_row_with(
        branch,
        b"pinned-compact".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"older".to_vec(),
    );
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "pinned-compact-newer",
            vec![compact_newer.clone()],
        ))
        .expect("install newer table");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "pinned-compact-older",
            vec![compact_older],
        ))
        .expect("install older table");

    let pinned = state.capture_read_view().expect("capture pinned view");

    state
        .replace_frozen_with_l0_table(
            0,
            branch_owned_table(
                branch,
                BranchLevel::ZERO,
                "pinned-flush-output",
                vec![frozen.clone()],
            ),
        )
        .expect("replace frozen table");
    let compaction = state
        .compact_branch_owned_tables(
            &BranchCompactionRequest::new(
                branch,
                BranchCompactionKind::CompactL0,
                "pinned-compaction-output",
            )
            .expect("compaction request"),
        )
        .expect("compact branch tables");
    assert!(compaction.installed_replacement_tables());
    let materialization = state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(branch, 0, "pinned-materialized")
                .expect("materialization request"),
        )
        .expect("materialize inherited layer");
    assert_eq!(materialization.rows_materialized(), 1);
    assert_eq!(materialization.tables_created(), 1);
    assert_eq!(materialization.inherited_layers_remaining(), 0);
    assert_eq!(state.inherited_layer_count(), 0);

    let frozen_key = physical_key(branch, b"pinned-flush".to_vec());
    assert_visible_row(
        pinned
            .latest(&frozen_key)
            .expect("pinned frozen latest")
            .as_ref(),
        &frozen,
        BranchRowSource::Frozen { index: 0 },
    );

    let compact_key = physical_key(branch, b"pinned-compact".to_vec());
    assert_visible_row(
        pinned
            .latest(&compact_key)
            .expect("pinned compact latest")
            .as_ref(),
        &compact_newer,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 1,
        },
    );

    let inherited_expected =
        rewrite_row_branch(&inherited, source, branch).expect("rewrite inherited row");
    let inherited_key = physical_key(branch, b"pinned-inherited".to_vec());
    assert_visible_row(
        pinned
            .latest(&inherited_key)
            .expect("pinned inherited latest")
            .as_ref(),
        &inherited_expected,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );

    let current = state.capture_read_view().expect("current view");
    assert_eq!(current.frozen_table_count(), 0);
    assert_eq!(current.inherited_layer_count(), 0);
    assert_eq!(
        current
            .latest(&frozen_key)
            .expect("current frozen latest")
            .expect("current frozen row")
            .row(),
        &frozen
    );
    assert_eq!(
        current
            .latest(&compact_key)
            .expect("current compact latest")
            .expect("current compact row")
            .row(),
        &compact_newer
    );
    assert_eq!(
        current
            .latest(&inherited_key)
            .expect("current inherited latest")
            .expect("current inherited row")
            .row(),
        &inherited_expected
    );
}

#[test]
fn branch_read_view_constructor_rejects_stale_facts_and_wrong_branch_sources() {
    let branch = branch_id(43);
    let other = branch_id(44);
    let row = storage_row_with(
        branch,
        b"constructor".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"value".to_vec(),
    );
    let mut active = MutableTable::new();
    active.insert_row(row.clone()).expect("insert row");
    let valid_facts = BranchStateFacts::new(
        branch,
        1,
        0,
        0,
        0,
        Some(CommitVersion::new(3)),
        Some(Timestamp::from_micros(30)),
        Some(Timestamp::from_micros(30)),
    )
    .expect("valid facts");
    BranchReadView::new(branch, active.clone(), Vec::new(), Vec::new(), valid_facts)
        .expect("valid read view");

    assert!(matches!(
        BranchReadView::new(
            branch,
            active.clone(),
            Vec::new(),
            Vec::new(),
            BranchStateFacts::empty(branch)
        ),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));

    let stale_facts = BranchStateFacts::new(
        branch,
        1,
        0,
        0,
        0,
        Some(CommitVersion::new(2)),
        Some(Timestamp::from_micros(30)),
        Some(Timestamp::from_micros(30)),
    )
    .expect("stale facts shape");
    assert!(matches!(
        BranchReadView::new(branch, active.clone(), Vec::new(), Vec::new(), stale_facts),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));

    let unsupported_inherited_facts =
        BranchStateFacts::new(branch, 0, 0, 0, 1, None, None, None).expect("inherited facts shape");
    assert!(matches!(
        BranchReadView::new(
            branch,
            MutableTable::new(),
            Vec::new(),
            Vec::new(),
            unsupported_inherited_facts
        ),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));

    let unavailable = branch_inherited_layer(
        other,
        CommitVersion::new(3),
        InheritedLayerStatus::Unavailable,
        Vec::new(),
    );
    assert!(matches!(
        BranchReadView::new_with_inherited(
            branch,
            MutableTable::new(),
            Vec::new(),
            Vec::new(),
            vec![unavailable],
            unsupported_inherited_facts
        ),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. })
    ));

    let wrong_branch_row = storage_row_with(
        other,
        b"constructor".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"secret-payload".to_vec(),
    );
    let mut wrong_active = MutableTable::new();
    wrong_active
        .insert_row(wrong_branch_row)
        .expect("insert wrong row");
    let wrong_error =
        BranchReadView::new(branch, wrong_active, Vec::new(), Vec::new(), valid_facts)
            .expect_err("wrong-branch source rejected");
    assert!(matches!(
        wrong_error,
        BranchRuntimeError::InvalidBranchState { .. }
    ));
    assert!(!wrong_error.to_string().contains("secret-payload"));
}

#[test]
fn branch_read_view_constructor_rejects_frozen_source_and_fact_mismatches() {
    let branch = branch_id(45);
    let other = branch_id(46);
    let row = storage_row_with(
        branch,
        b"frozen-constructor".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"value".to_vec(),
    );
    let mut frozen_source = MutableTable::new();
    frozen_source.insert_row(row).expect("insert frozen row");
    let valid_facts = BranchStateFacts::new(
        branch,
        0,
        1,
        0,
        0,
        Some(CommitVersion::new(3)),
        Some(Timestamp::from_micros(30)),
        Some(Timestamp::from_micros(30)),
    )
    .expect("valid frozen facts");
    let frozen = frozen_source.freeze();
    BranchReadView::new(
        branch,
        MutableTable::new(),
        vec![frozen.clone()],
        Vec::new(),
        valid_facts,
    )
    .expect("valid frozen read view");

    let stale_count = BranchStateFacts::new(
        branch,
        0,
        2,
        0,
        0,
        Some(CommitVersion::new(3)),
        Some(Timestamp::from_micros(30)),
        Some(Timestamp::from_micros(30)),
    )
    .expect("stale frozen count facts");
    assert!(matches!(
        BranchReadView::new(
            branch,
            MutableTable::new(),
            vec![frozen.clone()],
            Vec::new(),
            stale_count
        ),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));

    let stale_timestamps = BranchStateFacts::new(
        branch,
        0,
        1,
        0,
        0,
        Some(CommitVersion::new(3)),
        Some(Timestamp::from_micros(29)),
        Some(Timestamp::from_micros(30)),
    )
    .expect("stale timestamp facts");
    assert!(matches!(
        BranchReadView::new(
            branch,
            MutableTable::new(),
            vec![frozen.clone()],
            Vec::new(),
            stale_timestamps,
        ),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));

    let wrong_row = storage_row_with(
        other,
        b"frozen-constructor".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"secret-payload".to_vec(),
    );
    let mut wrong_frozen = MutableTable::new();
    wrong_frozen
        .insert_row(wrong_row)
        .expect("insert wrong frozen row");
    let wrong_error = BranchReadView::new(
        branch,
        MutableTable::new(),
        vec![wrong_frozen.freeze()],
        Vec::new(),
        valid_facts,
    )
    .expect_err("wrong frozen row rejected");
    assert!(matches!(
        wrong_error,
        BranchRuntimeError::InvalidBranchState { .. }
    ));
    assert!(!wrong_error.to_string().contains("secret-payload"));
}

#[test]
fn branch_read_view_empty_and_single_row_cases_are_stable() {
    let branch = branch_id(45);
    let empty_state = BranchLocalState::empty(branch);
    let empty_view = empty_state.capture_read_view().expect("empty view");
    let key = physical_key(branch, b"single".to_vec());
    assert_eq!(empty_view.latest(&key).expect("empty latest"), None);
    assert!(empty_view
        .history(&key, BranchHistoryOptions::all())
        .expect("empty history")
        .is_empty());
    let empty_prefix = BranchScanBounds::prefix(&physical_key(branch, Vec::new()));
    assert!(empty_view
        .scan_prefix(&empty_prefix, BranchReadBound::latest())
        .expect("empty prefix")
        .is_empty());
    let empty_range = BranchScanBounds::unbounded(
        branch,
        "default",
        StorageSpaceId::engine(0x20).expect("engine space"),
    )
    .expect("empty range");
    assert!(empty_view
        .scan_range(&empty_range, BranchReadBound::latest())
        .expect("empty range")
        .is_empty());

    let mut single_state = BranchLocalState::empty(branch);
    let expired_looking = storage_row_with(
        branch,
        b"single".to_vec(),
        1,
        10,
        Timestamp::from_micros(5),
        Vec::new(),
    );
    single_state
        .append_committed_row(expired_looking.clone())
        .expect("append expired-looking row");
    let single_view = single_state.capture_read_view().expect("single view");
    let latest = single_view
        .latest(&key)
        .expect("latest")
        .expect("single row");
    assert_eq!(latest.row(), &expired_looking);
    assert_eq!(latest.source(), BranchRowSource::Active);
    assert_eq!(latest.row().value(), b"");
    assert_eq!(latest.row().expires_at(), Timestamp::from_micros(5));
    assert_eq!(
        single_view
            .at_version(&key, CommitVersion::ZERO)
            .expect("below single row"),
        None
    );
    assert_eq!(
        single_view
            .at_version(&key, CommitVersion::MAX)
            .expect("max bound")
            .expect("max row")
            .row(),
        &expired_looking
    );
}

#[test]
fn branch_read_view_frozen_limit_skip_does_not_mutate_captured_view() {
    let branch = branch_id(45);
    let config = BranchRuntimeConfig::new(7, 64, 1).expect("config");
    let mut limited_state = BranchLocalState::new(branch, config).expect("limited state");
    let frozen = storage_row_with(
        branch,
        b"limited".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"frozen".to_vec(),
    );
    let active = storage_row_with(
        branch,
        b"limited".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"active".to_vec(),
    );
    limited_state
        .append_committed_row(frozen.clone())
        .expect("append frozen row");
    assert!(matches!(
        limited_state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    let pinned = limited_state.capture_read_view().expect("pinned view");
    let pinned_facts = pinned.facts();
    limited_state
        .append_committed_row(active.clone())
        .expect("append active row");
    assert_eq!(
        limited_state.rotate_active(),
        BranchRotationOutcome::Skipped {
            reason: BranchRotationSkipReason::FrozenLimitReached,
        }
    );
    let limited_key = physical_key(branch, b"limited".to_vec());
    assert_eq!(
        pinned
            .latest(&limited_key)
            .expect("pinned latest")
            .expect("pinned row")
            .row(),
        &frozen
    );
    assert_eq!(pinned.facts(), pinned_facts);
    assert_eq!(
        limited_state
            .capture_read_view()
            .expect("after skip view")
            .latest(&limited_key)
            .expect("after skip latest")
            .expect("active row")
            .row(),
        &active
    );
}

#[test]
fn branch_read_view_latest_and_version_reads_follow_row_chain_not_source_order() {
    let branch = branch_id(39);
    let mut state = BranchLocalState::empty(branch);
    let key = physical_key(branch, b"versioned".to_vec());
    let frozen_newer = storage_row_with(
        branch,
        b"versioned".to_vec(),
        10,
        100,
        Timestamp::EPOCH,
        b"frozen-newer".to_vec(),
    );
    let active_older = storage_row_with(
        branch,
        b"versioned".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        b"active-older".to_vec(),
    );
    let hidden_by_tombstone = storage_row_with(
        branch,
        b"hidden".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"hidden".to_vec(),
    );
    let tombstone = tombstone_row(branch, b"hidden".to_vec(), 5, 50);

    state
        .append_committed_row(frozen_newer.clone())
        .expect("append frozen newer");
    state
        .append_committed_row(hidden_by_tombstone)
        .expect("append hidden");
    state
        .append_committed_row(tombstone)
        .expect("append tombstone");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .append_committed_row(active_older.clone())
        .expect("append active older");

    let view = state
        .capture_read_view()
        .expect("view")
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    let latest = view.latest(&key).expect("latest").expect("latest row");
    assert_eq!(latest.row(), &frozen_newer);
    assert_eq!(latest.source(), BranchRowSource::Frozen { index: 0 });

    let at_seven = view
        .at_version(&key, CommitVersion::new(7))
        .expect("at version")
        .expect("older row");
    assert_eq!(at_seven.row(), &active_older);
    assert_eq!(at_seven.source(), BranchRowSource::Active);
    assert_eq!(
        view.at_version(&key, CommitVersion::new(6))
            .expect("below all"),
        None
    );
    assert_eq!(
        view.latest(&physical_key(branch, b"hidden".to_vec()))
            .expect("tombstone shadows"),
        None
    );
}

#[test]
fn branch_read_view_version_bounds_respect_tombstone_edges_and_extremes() {
    let branch = branch_id(46);
    let mut state = BranchLocalState::empty(branch);
    let live_before_tombstone = storage_row_with(
        branch,
        b"deleted".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"live".to_vec(),
    );
    let deleting_tombstone = tombstone_row(branch, b"deleted".to_vec(), 3, 30);
    let zero_row = storage_row_with(
        branch,
        b"zero".to_vec(),
        0,
        0,
        Timestamp::EPOCH,
        b"zero".to_vec(),
    );
    let max_row = storage_row_with(
        branch,
        b"max".to_vec(),
        u64::MAX,
        u64::MAX,
        Timestamp::EPOCH,
        b"max".to_vec(),
    );
    for row in [
        live_before_tombstone.clone(),
        deleting_tombstone,
        zero_row.clone(),
        max_row.clone(),
    ] {
        state.append_committed_row(row).expect("append version row");
    }
    let view = state
        .capture_read_view()
        .expect("view")
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    let deleted_key = physical_key(branch, b"deleted".to_vec());
    assert_eq!(view.latest(&deleted_key).expect("latest deleted"), None);
    assert_eq!(
        view.at_version(&deleted_key, CommitVersion::new(2))
            .expect("before tombstone")
            .expect("live row")
            .row(),
        &live_before_tombstone
    );
    assert_eq!(
        view.at_version(&deleted_key, CommitVersion::new(3))
            .expect("at tombstone"),
        None
    );
    assert_eq!(
        view.at_version(&physical_key(branch, b"zero".to_vec()), CommitVersion::ZERO)
            .expect("zero bound")
            .expect("zero row")
            .row(),
        &zero_row
    );
    assert_eq!(
        view.at_version(&physical_key(branch, b"max".to_vec()), CommitVersion::MAX)
            .expect("max bound")
            .expect("max row")
            .row(),
        &max_row
    );
}

#[test]
fn branch_read_view_timestamp_reads_filter_by_timestamp_then_commit_version() {
    let branch = branch_id(57);
    let mut state = BranchLocalState::empty(branch);
    let key = physical_key(branch, b"as-of".to_vec());
    let older = storage_row_with(
        branch,
        b"as-of".to_vec(),
        7,
        80,
        Timestamp::EPOCH,
        b"older".to_vec(),
    );
    let highest_version = storage_row_with(
        branch,
        b"as-of".to_vec(),
        10,
        100,
        Timestamp::EPOCH,
        b"highest-version".to_vec(),
    );
    let lower_version_later_timestamp = storage_row_with(
        branch,
        b"as-of".to_vec(),
        8,
        120,
        Timestamp::EPOCH,
        b"later-timestamp".to_vec(),
    );
    for row in [
        older.clone(),
        highest_version.clone(),
        lower_version_later_timestamp,
    ] {
        state
            .append_committed_row(row)
            .expect("append timestamp row");
    }

    let view = state
        .capture_read_view()
        .expect("view")
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    assert_eq!(
        view.read_point(
            &key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(79))
        )
        .expect("before all"),
        None
    );
    assert_eq!(
        view.read_point(
            &key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(80))
        )
        .expect("at older")
        .expect("older row")
        .row(),
        &older
    );
    assert_eq!(
        view.read_point(
            &key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(119))
        )
        .expect("before lower version later timestamp")
        .expect("highest version row")
        .row(),
        &highest_version
    );
    assert_eq!(
        view.read_point(
            &key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(130))
        )
        .expect("after all")
        .expect("highest version row")
        .row(),
        &highest_version,
        "timestamp bounds filter eligibility, then row chains still select newest commit version",
    );
}

#[test]
fn branch_read_view_timestamp_reads_cover_frozen_and_owned_sources() {
    let branch = branch_id(57);
    let mut state = BranchLocalState::empty(branch);
    let frozen_key = physical_key(branch, b"as-of-frozen".to_vec());
    let frozen_visible = storage_row_with(
        branch,
        b"as-of-frozen".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"frozen-visible".to_vec(),
    );
    let frozen_future = storage_row_with(
        branch,
        b"as-of-frozen".to_vec(),
        3,
        50,
        Timestamp::EPOCH,
        b"frozen-future".to_vec(),
    );
    state
        .append_committed_row(frozen_visible.clone())
        .expect("append frozen visible");
    state
        .append_committed_row(frozen_future)
        .expect("append frozen future");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));

    let owned_key = physical_key(branch, b"as-of-owned".to_vec());
    let owned_visible = storage_row_with(
        branch,
        b"as-of-owned".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"owned-visible".to_vec(),
    );
    let owned_future = storage_row_with(
        branch,
        b"as-of-owned".to_vec(),
        6,
        80,
        Timestamp::EPOCH,
        b"owned-future".to_vec(),
    );
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "timestamp-owned-source",
            vec![owned_visible.clone(), owned_future],
        ))
        .expect("install owned timestamp table");

    let view = state
        .capture_read_view()
        .expect("view")
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    assert_visible_row(
        view.read_point(
            &frozen_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(30)),
        )
        .expect("frozen timestamp read")
        .as_ref(),
        &frozen_visible,
        BranchRowSource::Frozen { index: 0 },
    );
    assert_eq!(
        view.read_point(
            &owned_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(39)),
        )
        .expect("before owned timestamp"),
        None
    );
    assert_visible_row(
        view.read_point(
            &owned_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .expect("owned timestamp read")
        .as_ref(),
        &owned_visible,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
}

#[test]
fn branch_read_view_timestamp_tombstones_suppress_fallthrough() {
    let branch = branch_id(58);
    let mut state = BranchLocalState::empty(branch);
    let deleted_key = physical_key(branch, b"deleted-at-time".to_vec());
    let deleted_put = storage_row_with(
        branch,
        b"deleted-at-time".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"deleted-put".to_vec(),
    );
    let deleted_tombstone = tombstone_row(branch, b"deleted-at-time".to_vec(), 3, 30);
    for row in [deleted_put.clone(), deleted_tombstone] {
        state
            .append_committed_row(row)
            .expect("append visibility row");
    }
    let view = state
        .capture_read_view()
        .expect("view")
        .with_timestamp_coverage(BranchTimestampCoverage::complete());

    assert_eq!(
        view.read_point(
            &deleted_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(29)),
        )
        .expect("before tombstone")
        .expect("deleted put")
        .row(),
        &deleted_put
    );
    assert_eq!(
        view.read_point(
            &deleted_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(30)),
        )
        .expect("at tombstone"),
        None,
        "tombstone exactly at read timestamp shadows older puts",
    );
}

#[test]
fn branch_read_view_timestamp_ttl_boundaries_suppress_fallthrough() {
    let branch = branch_id(58);
    let mut state = BranchLocalState::empty(branch);
    let ttl_key = physical_key(branch, b"ttl".to_vec());
    let never_expires_key = physical_key(branch, b"ttl-epoch".to_vec());
    let ttl_old = storage_row_with(
        branch,
        b"ttl".to_vec(),
        1,
        5,
        Timestamp::EPOCH,
        b"old".to_vec(),
    );
    let ttl_new = storage_row_with(
        branch,
        b"ttl".to_vec(),
        2,
        10,
        Timestamp::from_micros(20),
        b"new".to_vec(),
    );
    let never_expires = storage_row_with(
        branch,
        b"ttl-epoch".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        Vec::new(),
    );
    for row in [ttl_old, ttl_new.clone(), never_expires.clone()] {
        state.append_committed_row(row).expect("append ttl row");
    }
    let view = state
        .capture_read_view()
        .expect("view")
        .with_timestamp_coverage(BranchTimestampCoverage::complete());

    assert_eq!(
        view.read_point(
            &ttl_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(19)),
        )
        .expect("before expiry")
        .expect("ttl row")
        .row(),
        &ttl_new
    );
    assert_eq!(
        view.read_point(
            &ttl_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(20)),
        )
        .expect("exact expiry"),
        None,
        "selected expired rows suppress the key instead of falling through",
    );
    assert_eq!(
        view.read_point(
            &ttl_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(21)),
        )
        .expect("after expiry"),
        None
    );
    assert_eq!(
        view.latest(&ttl_key)
            .expect("latest ignores wall clock")
            .expect("latest ttl row")
            .row(),
        &ttl_new,
        "latest reads do not invent a wall-clock timestamp",
    );
    assert_eq!(
        view.read_point(
            &never_expires_key,
            BranchReadBound::at_timestamp(Timestamp::MAX),
        )
        .expect("max timestamp")
        .expect("epoch-expiry row")
        .row(),
        &never_expires
    );
}

#[test]
fn branch_read_view_timestamp_max_expiry_is_far_future_not_no_expiry() {
    let branch = branch_id(58);
    let mut state = BranchLocalState::empty(branch);
    let max_expiry_key = physical_key(branch, b"ttl-max".to_vec());
    let max_expiry = storage_row_with(
        branch,
        b"ttl-max".to_vec(),
        1,
        10,
        Timestamp::MAX,
        b"far-future".to_vec(),
    );
    state
        .append_committed_row(max_expiry.clone())
        .expect("append max-expiry row");
    let view = state.capture_read_view().expect("view");

    assert_eq!(
        view.read_point(
            &max_expiry_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(u64::MAX - 1)),
        )
        .expect("before max expiry")
        .expect("visible before max expiry")
        .row(),
        &max_expiry
    );
    assert_eq!(
        view.read_point(
            &max_expiry_key,
            BranchReadBound::at_timestamp(Timestamp::MAX),
        )
        .expect("at max expiry"),
        None,
        "Timestamp::MAX expiry is an actual far-future expiry, not the no-expiry sentinel",
    );
}

#[test]
fn branch_read_view_timestamp_scans_apply_tombstone_and_ttl_per_key() {
    let branch = branch_id(59);
    let mut state = BranchLocalState::empty(branch);
    let visible = storage_row_with(
        branch,
        b"ts-scan-a".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"a".to_vec(),
    );
    let future = storage_row_with(
        branch,
        b"ts-scan-b".to_vec(),
        2,
        50,
        Timestamp::EPOCH,
        b"future".to_vec(),
    );
    let expired_old = storage_row_with(
        branch,
        b"ts-scan-c".to_vec(),
        1,
        5,
        Timestamp::EPOCH,
        b"old".to_vec(),
    );
    let expired_new = storage_row_with(
        branch,
        b"ts-scan-c".to_vec(),
        3,
        30,
        Timestamp::from_micros(35),
        b"expired".to_vec(),
    );
    let deleted_old = storage_row_with(
        branch,
        b"ts-scan-d".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"deleted".to_vec(),
    );
    let deleted_tombstone = tombstone_row(branch, b"ts-scan-d".to_vec(), 4, 40);
    for row in [
        visible.clone(),
        future,
        expired_old,
        expired_new,
        deleted_old,
        deleted_tombstone,
    ] {
        state.append_committed_row(row).expect("append scan row");
    }
    let view = state.capture_read_view().expect("view");
    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"ts-scan-".to_vec()));
    let prefix_rows = view
        .scan_prefix(
            &prefix,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .expect("timestamp prefix scan");
    assert_eq!(scan_user_keys(&prefix_rows), vec![b"ts-scan-a".to_vec()]);
    assert_eq!(prefix_rows[0].row(), &visible);

    let range = BranchScanBounds::closed(
        &physical_key(branch, b"ts-scan-a".to_vec()),
        &physical_key(branch, b"ts-scan-d".to_vec()),
    )
    .expect("closed range");
    let range_rows = view
        .scan_range(
            &range,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .expect("timestamp range scan");
    assert_eq!(scan_user_keys(&range_rows), vec![b"ts-scan-a".to_vec()]);
}

#[test]
fn branch_read_view_timestamp_scans_preserve_bounds_and_empty_results() {
    let branch = branch_id(59);
    let mut state = BranchLocalState::empty(branch);
    let scan_a = storage_row_with(
        branch,
        b"ts-bound-a".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"a".to_vec(),
    );
    let scan_b = storage_row_with(
        branch,
        b"ts-bound-b".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"b".to_vec(),
    );
    let scan_c_future = storage_row_with(
        branch,
        b"ts-bound-c".to_vec(),
        3,
        50,
        Timestamp::EPOCH,
        b"c".to_vec(),
    );
    for row in [scan_a.clone(), scan_b.clone(), scan_c_future] {
        state.append_committed_row(row).expect("append scan row");
    }
    let view = state
        .capture_read_view()
        .expect("view")
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"ts-bound-".to_vec()));

    assert!(
        view.scan_prefix(
            &prefix,
            BranchReadBound::at_timestamp(Timestamp::from_micros(9)),
        )
        .expect("before all timestamp scan")
        .is_empty(),
        "timestamp scan with no eligible rows should return an empty result",
    );
    let closed = BranchScanBounds::closed(
        &physical_key(branch, b"ts-bound-a".to_vec()),
        &physical_key(branch, b"ts-bound-c".to_vec()),
    )
    .expect("closed bounds");
    let closed_rows = view
        .scan_range(
            &closed,
            BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
        )
        .expect("closed timestamp range");
    assert_eq!(
        scan_user_keys(&closed_rows),
        vec![b"ts-bound-a".to_vec(), b"ts-bound-b".to_vec()],
        "timestamp scan remains sorted and preserves inclusive range edges",
    );
    let open = BranchScanBounds::open(
        &physical_key(branch, b"ts-bound-a".to_vec()),
        &physical_key(branch, b"ts-bound-c".to_vec()),
    )
    .expect("open bounds");
    let open_rows = view
        .scan_range(
            &open,
            BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
        )
        .expect("open timestamp range");
    assert_eq!(scan_user_keys(&open_rows), vec![b"ts-bound-b".to_vec()]);
}

#[test]
fn branch_read_view_timestamp_scans_preserve_key_spaces() {
    let branch = branch_id(59);
    let mut state = BranchLocalState::empty(branch);
    let engine_space = StorageSpaceId::engine(0x20).expect("engine space");
    let other_space = StorageSpaceId::engine(0x21).expect("other space");
    let default_row = storage_row_with(
        branch,
        b"ts-space-a".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"default".to_vec(),
    );
    let system_row = storage_row_with_named_space(
        branch,
        "system",
        engine_space,
        b"ts-space-a".to_vec(),
        2,
        10,
        b"system".to_vec(),
    );
    let other_storage_space_row = StorageRow::put(
        physical_key_with(branch, "default", other_space, b"ts-space-a".to_vec()),
        CommitVersion::new(3),
        Timestamp::from_micros(10),
        Timestamp::EPOCH,
        b"other-space".to_vec(),
    );
    for row in [
        default_row.clone(),
        system_row.clone(),
        other_storage_space_row.clone(),
    ] {
        state.append_committed_row(row).expect("append scan row");
    }
    let view = state.capture_read_view().expect("view");
    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"ts-space-".to_vec()));

    let default_rows = view
        .scan_prefix(
            &prefix,
            BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
        )
        .expect("default prefix timestamp scan");
    assert_eq!(
        scan_user_keys(&default_rows),
        vec![b"ts-space-a".to_vec()],
        "default-space scan must not leak named-space or storage-space rows",
    );
    assert_eq!(default_rows[0].row(), &default_row);
    let system_rows = view
        .scan_prefix(
            &BranchScanBounds::prefix(&physical_key_with(
                branch,
                "system",
                engine_space,
                b"ts-space-".to_vec(),
            )),
            BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
        )
        .expect("system prefix timestamp scan");
    assert_eq!(system_rows.len(), 1);
    assert_eq!(system_rows[0].row(), &system_row);
    assert_eq!(
        view.scan_prefix(
            &BranchScanBounds::prefix(&physical_key_with(
                branch,
                "default",
                other_space,
                b"ts-space-".to_vec(),
            )),
            BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
        )
        .expect("other storage-space timestamp scan")
        .first()
        .expect("other storage-space row")
        .row(),
        &other_storage_space_row,
    );
}

#[test]
fn branch_inherited_timestamp_scans_rewrite_source_keys_before_grouping() {
    let source = branch_id(63);
    let child = branch_id(64);
    let source_visible = storage_row_with(
        source,
        b"ts-inherited-scan-a".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"visible".to_vec(),
    );
    let source_future = storage_row_with(
        source,
        b"ts-inherited-scan-b".to_vec(),
        4,
        70,
        Timestamp::EPOCH,
        b"future".to_vec(),
    );
    let source_after_fork = storage_row_with(
        source,
        b"ts-inherited-scan-c".to_vec(),
        7,
        20,
        Timestamp::EPOCH,
        b"post-fork".to_vec(),
    );
    let layer = branch_inherited_layer_unchecked_for_fork_gate_tests(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "timestamp-inherited-scan",
            vec![source_visible.clone(), source_future, source_after_fork],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let view = child_state.capture_read_view().expect("view");
    let expected = rewrite_row_branch(&source_visible, source, child).expect("rewrite expected");
    let prefix = BranchScanBounds::prefix(&physical_key(child, b"ts-inherited-scan-".to_vec()));
    let prefix_rows = view
        .scan_prefix(
            &prefix,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .expect("timestamp inherited prefix scan");
    assert_eq!(
        scan_user_keys(&prefix_rows),
        vec![b"ts-inherited-scan-a".to_vec()]
    );
    assert_visible_row(
        prefix_rows.first(),
        &expected,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );

    let range = BranchScanBounds::closed(
        &physical_key(child, b"ts-inherited-scan-a".to_vec()),
        &physical_key(child, b"ts-inherited-scan-c".to_vec()),
    )
    .expect("closed inherited range");
    let range_rows = view
        .scan_range(
            &range,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .expect("timestamp inherited range scan");
    assert_eq!(range_rows.len(), 1);
    assert_eq!(range_rows[0].row(), &expected);
}

#[test]
fn branch_read_view_timestamp_views_are_pinned_across_later_mutations() {
    let branch = branch_id(65);
    let mut state = BranchLocalState::empty(branch);
    let point = storage_row_with(
        branch,
        b"pinned-ts".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"point".to_vec(),
    );
    let scan = storage_row_with(
        branch,
        b"pinned-scan-a".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"scan".to_vec(),
    );
    for row in [point.clone(), scan.clone()] {
        state.append_committed_row(row).expect("append pinned row");
    }
    let pinned = state.capture_read_view().expect("pinned view");

    state
        .append_committed_row(storage_row_with(
            branch,
            b"pinned-ts".to_vec(),
            3,
            30,
            Timestamp::EPOCH,
            b"later-point".to_vec(),
        ))
        .expect("append later point");
    state
        .append_committed_row(storage_row_with(
            branch,
            b"pinned-scan-b".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            b"later-scan".to_vec(),
        ))
        .expect("append later scan");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "pinned-timestamp-owned",
            vec![storage_row_with(
                branch,
                b"pinned-scan-c".to_vec(),
                5,
                50,
                Timestamp::EPOCH,
                b"owned".to_vec(),
            )],
        ))
        .expect("install later owned table");

    let point_key = physical_key(branch, b"pinned-ts".to_vec());
    assert_visible_row(
        pinned
            .read_point(
                &point_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(35)),
            )
            .expect("pinned point read")
            .as_ref(),
        &point,
        BranchRowSource::Active,
    );
    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"pinned-scan-".to_vec()));
    let pinned_scan = pinned
        .scan_prefix(
            &prefix,
            BranchReadBound::at_timestamp(Timestamp::from_micros(60)),
        )
        .expect("pinned timestamp scan");
    assert_eq!(
        scan_user_keys(&pinned_scan),
        vec![b"pinned-scan-a".to_vec()]
    );
    assert_eq!(pinned_scan[0].row(), &scan);

    let current = state.capture_read_view().expect("current view");
    assert_eq!(
        current
            .scan_prefix(
                &prefix,
                BranchReadBound::at_timestamp(Timestamp::from_micros(60)),
            )
            .expect("current timestamp scan")
            .len(),
        3
    );
}

#[test]
fn branch_timestamp_coverage_rejects_only_proven_insufficient_history() {
    let branch = branch_id(60);
    let mut state = BranchLocalState::empty(branch);
    let row = storage_row_with(
        branch,
        b"coverage".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"coverage".to_vec(),
    );
    state.append_committed_row(row.clone()).expect("append row");
    let key = physical_key(branch, b"coverage".to_vec());
    let canonical = state.capture_read_view().expect("canonical coverage");
    assert_eq!(
        canonical.timestamp_coverage(),
        BranchTimestampCoverage::unknown()
    );
    assert_eq!(
        canonical
            .read_point(
                &key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(49)),
            )
            .expect("unknown coverage does not invent an insufficiency proof"),
        None
    );

    let complete_since = state
        .capture_read_view()
        .expect("coverage view")
        .with_timestamp_coverage(BranchTimestampCoverage::complete_since(
            Timestamp::from_micros(50),
        ));
    let error = complete_since
        .read_point(
            &key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(49)),
        )
        .expect_err("known insufficient history");
    assert_eq!(
        error,
        BranchRuntimeError::InsufficientTimestampHistory {
            branch_id: branch,
            requested_timestamp: Timestamp::from_micros(49),
            earliest_available_timestamp: Some(Timestamp::from_micros(50)),
            source: BranchTimestampHistorySource::Combined,
        }
    );
    assert!(!error.to_string().contains("coverage"));
    assert_eq!(
        complete_since
            .read_point(
                &key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(50))
            )
            .expect("at coverage floor")
            .expect("row")
            .row(),
        &row
    );
    assert_eq!(
        state
            .capture_read_view()
            .expect("complete coverage")
            .with_timestamp_coverage(BranchTimestampCoverage::complete())
            .read_point(&key, BranchReadBound::at_timestamp(Timestamp::EPOCH))
            .expect("complete coverage permits timestamp read"),
        None
    );

    state.set_timestamp_coverage(BranchTimestampCoverage::complete_since(
        Timestamp::from_micros(50),
    ));
    assert_eq!(
        state.timestamp_coverage(),
        BranchTimestampCoverage::complete_since(Timestamp::from_micros(50)),
    );
    let captured_error = state
        .capture_read_view()
        .expect("state coverage view")
        .read_point(
            &key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(49)),
        )
        .expect_err("state coverage is wired into capture_read_view");
    assert_eq!(
        captured_error,
        BranchRuntimeError::InsufficientTimestampHistory {
            branch_id: branch,
            requested_timestamp: Timestamp::from_micros(49),
            earliest_available_timestamp: Some(Timestamp::from_micros(50)),
            source: BranchTimestampHistorySource::Combined,
        }
    );
}

#[test]
fn branch_read_view_multiple_frozen_tables_preserve_source_facts() {
    let branch = branch_id(47);
    let mut state = BranchLocalState::empty(branch);
    let old_frozen = storage_row_with(
        branch,
        b"multi-frozen".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"old".to_vec(),
    );
    let new_frozen = storage_row_with(
        branch,
        b"multi-frozen".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"new".to_vec(),
    );
    let active_middle = storage_row_with(
        branch,
        b"multi-frozen".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"active".to_vec(),
    );

    state
        .append_committed_row(old_frozen.clone())
        .expect("append old frozen");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .append_committed_row(new_frozen.clone())
        .expect("append new frozen");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .append_committed_row(active_middle.clone())
        .expect("append active middle");

    let view = state.capture_read_view().expect("view");
    assert_eq!(view.frozen_table_count(), 2);
    let key = physical_key(branch, b"multi-frozen".to_vec());
    let latest = view.latest(&key).expect("latest").expect("new frozen");
    assert_eq!(latest.row(), &new_frozen);
    assert_eq!(latest.source(), BranchRowSource::Frozen { index: 0 });
    let at_two = view
        .at_version(&key, CommitVersion::new(2))
        .expect("at two")
        .expect("active middle");
    assert_eq!(at_two.row(), &active_middle);
    assert_eq!(at_two.source(), BranchRowSource::Active);
    let at_one = view
        .at_version(&key, CommitVersion::new(1))
        .expect("at one")
        .expect("old frozen");
    assert_eq!(at_one.row(), &old_frozen);
    assert_eq!(at_one.source(), BranchRowSource::Frozen { index: 1 });
}

#[test]
fn branch_read_view_history_preserves_tombstones_limits_and_before_version() {
    let branch = branch_id(40);
    let mut state = BranchLocalState::empty(branch);
    let key = physical_key(branch, b"history".to_vec());
    let rows = [
        storage_row_with(
            branch,
            b"history".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"one".to_vec(),
        ),
        tombstone_row(branch, b"history".to_vec(), 2, 20),
        storage_row_with(
            branch,
            b"history".to_vec(),
            3,
            30,
            Timestamp::from_micros(25),
            Vec::new(),
        ),
    ];

    for row in rows {
        state.append_committed_row(row).expect("append history row");
    }
    let view = state.capture_read_view().expect("view");

    let all = view
        .history(&key, BranchHistoryOptions::all())
        .expect("all history");
    assert_eq!(history_versions(&all), vec![3, 2, 1]);
    assert!(all.iter().any(|row| row.row().is_tombstone()));
    assert_eq!(all[0].row().value(), b"");
    assert_eq!(all[0].row().expires_at(), Timestamp::from_micros(25));

    let before_three = view
        .history(
            &key,
            BranchHistoryOptions::all().before_version(CommitVersion::new(3)),
        )
        .expect("before");
    assert_eq!(history_versions(&before_three), vec![2, 1]);

    let one = view
        .history(&key, BranchHistoryOptions::all().limit(1))
        .expect("limited");
    assert_eq!(history_versions(&one), vec![3]);

    let zero = view
        .history(&key, BranchHistoryOptions::all().limit(0))
        .expect("zero limit");
    assert!(zero.is_empty());

    let without_tombstones = view
        .history(&key, BranchHistoryOptions::all().include_tombstones(false))
        .expect("without tombstones");
    assert_eq!(history_versions(&without_tombstones), vec![3, 1]);

    let before_zero = view
        .history(
            &key,
            BranchHistoryOptions::all().before_version(CommitVersion::ZERO),
        )
        .expect("before zero");
    assert!(before_zero.is_empty());
}

#[test]
fn branch_read_view_prefix_and_range_scans_group_by_physical_key() {
    let branch = branch_id(41);
    let view = branch_read_view_with_scan_rows(branch);
    assert_eq!(view.branch_id(), branch);

    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"ap".to_vec()));
    let prefix_rows = view
        .scan_prefix(&prefix, BranchReadBound::latest())
        .expect("prefix scan");
    assert_eq!(
        scan_user_keys(&prefix_rows),
        vec![b"apple".to_vec(), b"apricot".to_vec()]
    );
    assert_eq!(prefix_rows[0].row().value(), b"new-apple");

    let closed = BranchScanBounds::closed(
        &physical_key(branch, b"apple".to_vec()),
        &physical_key(branch, b"banana".to_vec()),
    )
    .expect("closed range");
    let range_rows = view
        .scan_range(&closed, BranchReadBound::at_version(CommitVersion::new(4)))
        .expect("range scan");
    assert_eq!(
        scan_user_keys(&range_rows),
        vec![b"apple".to_vec(), b"apricot".to_vec(), b"banana".to_vec()]
    );

    let open = BranchScanBounds::open(
        &physical_key(branch, b"apple".to_vec()),
        &physical_key(branch, b"banana".to_vec()),
    )
    .expect("open range");
    let open_rows = view
        .scan_range(&open, BranchReadBound::latest())
        .expect("open range scan");
    assert_eq!(scan_user_keys(&open_rows), vec![b"apricot".to_vec()]);

    let bounded = BranchScanBounds::range(
        branch,
        "default",
        StorageSpaceId::engine(0x20).expect("engine space"),
        BranchUserKeyBound::included(b"apple".to_vec()),
        BranchUserKeyBound::excluded(b"banana".to_vec()),
    )
    .expect("manual range");
    let bounded_rows = view
        .scan_range(&bounded, BranchReadBound::latest())
        .expect("manual range scan");
    assert_eq!(
        scan_user_keys(&bounded_rows),
        vec![b"apple".to_vec(), b"apricot".to_vec()]
    );

    let unbounded = BranchScanBounds::unbounded(
        branch,
        "default",
        StorageSpaceId::engine(0x20).expect("engine space"),
    )
    .expect("unbounded scan");
    let unbounded_rows = view
        .scan_range(
            &unbounded,
            BranchReadBound::at_version(CommitVersion::new(6)),
        )
        .expect("unbounded scan");
    assert_eq!(
        scan_user_keys(&unbounded_rows),
        vec![
            b"apple".to_vec(),
            b"apricot".to_vec(),
            b"banana".to_vec(),
            vec![0x80, 0x00, 0xff],
        ]
    );
}

#[test]
fn branch_read_view_scans_cover_empty_prefix_zero_bytes_and_degenerate_ranges() {
    let branch = branch_id(48);
    let mut state = BranchLocalState::empty(branch);
    let empty_key_row = storage_row_with(branch, Vec::new(), 1, 10, Timestamp::EPOCH, Vec::new());
    let nul_row = storage_row_with(
        branch,
        b"nul\0a".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"nul-a".to_vec(),
    );
    let nul_tombstone = tombstone_row(branch, b"nul\0b".to_vec(), 3, 30);
    let other_storage_space = StorageRow::put(
        physical_key_with(
            branch,
            "default",
            StorageSpaceId::engine(0x21).expect("engine space"),
            b"nul\0a".to_vec(),
        ),
        CommitVersion::new(4),
        Timestamp::from_micros(40),
        Timestamp::EPOCH,
        b"other-storage-space".to_vec(),
    );
    for row in [
        empty_key_row.clone(),
        nul_row.clone(),
        nul_tombstone,
        other_storage_space,
    ] {
        state
            .append_committed_row(row)
            .expect("append scan edge row");
    }
    let view = state.capture_read_view().expect("view");

    let empty_prefix = BranchScanBounds::prefix(&physical_key(branch, Vec::new()));
    assert_eq!(
        scan_user_keys(
            &view
                .scan_prefix(&empty_prefix, BranchReadBound::latest())
                .expect("empty prefix scan")
        ),
        vec![Vec::new(), b"nul\0a".to_vec()]
    );

    let nul_prefix = BranchScanBounds::prefix(&physical_key(branch, b"nul\0".to_vec()));
    let nul_rows = view
        .scan_prefix(&nul_prefix, BranchReadBound::latest())
        .expect("nul prefix scan");
    assert_eq!(scan_user_keys(&nul_rows), vec![b"nul\0a".to_vec()]);
    assert_eq!(nul_rows[0].row(), &nul_row);

    let lower = physical_key(branch, b"nul\0a".to_vec());
    let open_degenerate = BranchScanBounds::open(&lower, &lower).expect("open degenerate");
    assert!(view
        .scan_range(&open_degenerate, BranchReadBound::latest())
        .expect("open degenerate scan")
        .is_empty());
    let closed_degenerate = BranchScanBounds::closed(&lower, &lower).expect("closed degenerate");
    let closed_degenerate_rows = view
        .scan_range(&closed_degenerate, BranchReadBound::latest())
        .expect("closed degenerate scan");
    assert_eq!(closed_degenerate_rows.len(), 1);
    assert_eq!(closed_degenerate_rows[0].row(), &nul_row);
}

fn branch_read_view_with_scan_rows(branch: BranchId) -> BranchReadView {
    let mut state = BranchLocalState::empty(branch);
    let rows = [
        storage_row_with(
            branch,
            b"apple".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"old-apple".to_vec(),
        ),
        storage_row_with(
            branch,
            b"apple".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"new-apple".to_vec(),
        ),
        storage_row_with(
            branch,
            b"apricot".to_vec(),
            3,
            30,
            Timestamp::EPOCH,
            b"apricot".to_vec(),
        ),
        storage_row_with(
            branch,
            b"banana".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            b"banana".to_vec(),
        ),
        tombstone_row(branch, b"apex".to_vec(), 5, 50),
        storage_row_with(
            branch,
            vec![0x80, 0x00, 0xff],
            6,
            60,
            Timestamp::EPOCH,
            b"high".to_vec(),
        ),
        StorageRow::put(
            physical_key_with(
                branch,
                "other-space",
                StorageSpaceId::engine(0x20).expect("engine space"),
                b"apple".to_vec(),
            ),
            CommitVersion::new(7),
            Timestamp::from_micros(70),
            Timestamp::EPOCH,
            b"other-space".to_vec(),
        ),
        StorageRow::put(
            physical_key_with(
                branch,
                "default",
                StorageSpaceId::engine(0x21).expect("engine space"),
                b"apple".to_vec(),
            ),
            CommitVersion::new(8),
            Timestamp::from_micros(80),
            Timestamp::EPOCH,
            b"other-storage-space".to_vec(),
        ),
    ];
    for row in rows {
        state.append_committed_row(row).expect("append scan row");
    }
    state.capture_read_view().expect("view")
}

#[test]
fn branch_read_view_rejects_wrong_branch_before_timestamp_reads_without_payload() {
    let branch = branch_id(42);
    let other = branch_id(43);
    let mut state = BranchLocalState::empty(branch);
    let row = storage_row_with(
        branch,
        b"payload".to_vec(),
        1,
        10,
        Timestamp::from_micros(20),
        b"secret-payload".to_vec(),
    );
    state.append_committed_row(row).expect("append row");
    let view = state.capture_read_view().expect("view");

    let wrong_branch_error = view
        .latest(&physical_key(other, b"payload".to_vec()))
        .expect_err("wrong branch rejected");
    assert!(matches!(
        wrong_branch_error,
        BranchRuntimeError::InvalidBranchRow { .. }
    ));
    assert!(!wrong_branch_error.to_string().contains("secret-payload"));

    let timestamp_row = view
        .read_point(
            &physical_key(branch, b"payload".to_vec()),
            BranchReadBound::at_timestamp(Timestamp::from_micros(19)),
        )
        .expect("timestamp read")
        .expect("timestamp row");
    assert_eq!(timestamp_row.row().value(), b"secret-payload");

    let scan_rows = view
        .scan_prefix(
            &BranchScanBounds::prefix(&physical_key(branch, b"payload".to_vec())),
            BranchReadBound::at_timestamp(Timestamp::from_micros(19)),
        )
        .expect("timestamp scan");
    assert_eq!(scan_user_keys(&scan_rows), vec![b"payload".to_vec()]);

    let wrong_branch_scan = view
        .scan_prefix(
            &BranchScanBounds::prefix(&physical_key(other, b"payload".to_vec())),
            BranchReadBound::latest(),
        )
        .expect_err("wrong branch scan rejected");
    assert!(matches!(
        wrong_branch_scan,
        BranchRuntimeError::InvalidBranchRow { .. }
    ));

    assert!(matches!(
        BranchScanBounds::closed(
            &physical_key(branch, b"z".to_vec()),
            &physical_key(branch, b"a".to_vec()),
        ),
        Err(BranchRuntimeError::InvalidReadBound { .. })
    ));
    assert!(matches!(
        BranchScanBounds::unbounded(
            branch,
            "",
            StorageSpaceId::engine(0x20).expect("engine space"),
        ),
        Err(BranchRuntimeError::InvalidReadBound { .. })
    ));
    assert!(matches!(
        BranchScanBounds::range(
            branch,
            "bad\0space",
            StorageSpaceId::engine(0x20).expect("engine space"),
            BranchUserKeyBound::Unbounded,
            BranchUserKeyBound::Unbounded,
        ),
        Err(BranchRuntimeError::InvalidReadBound { .. })
    ));
}
