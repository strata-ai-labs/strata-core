use super::*;

fn scan_row(branch: BranchId, user_key: &str, version: u64) -> StorageRow {
    storage_row_with(
        branch,
        user_key.as_bytes().to_vec(),
        version,
        version.saturating_mul(10),
        Timestamp::EPOCH,
        user_key.as_bytes().to_vec(),
    )
}

fn scan_table(
    branch: BranchId,
    level: BranchLevel,
    identity: &str,
    user_key: &str,
    version: u64,
) -> BranchOwnedTable {
    branch_owned_table(
        branch,
        level,
        identity,
        vec![scan_row(branch, user_key, version)],
    )
}

fn install_single_row_tables(
    state: &mut BranchLocalState,
    branch: BranchId,
    level: BranchLevel,
    identity_prefix: &str,
    key_prefix: &str,
    count: usize,
    base_version: u64,
) {
    for index in 0..count {
        let key = format!("{key_prefix}-{index:03}");
        let version = base_version + u64::try_from(index).expect("table index fits in u64");
        state
            .install_owned_table_at_level(
                level,
                scan_table(
                    branch,
                    level,
                    &format!("{identity_prefix}-{index:03}"),
                    &key,
                    version,
                ),
            )
            .expect("install scan table");
    }
}

fn scan_range(branch: BranchId, lower: &str, upper: &str) -> BranchScanBounds {
    BranchScanBounds::range(
        branch,
        "default",
        StorageSpaceId::engine(0x20).expect("engine storage space"),
        BranchUserKeyBound::included(lower.as_bytes()),
        BranchUserKeyBound::excluded(upper.as_bytes()),
    )
    .expect("scan range")
}

fn scan_prefix(branch: BranchId, prefix: &str) -> BranchScanBounds {
    BranchScanBounds::prefix(&physical_key(branch, prefix.as_bytes().to_vec()))
}

fn row_user_keys(rows: &[BranchHistoryRow]) -> Vec<Vec<u8>> {
    rows.iter()
        .map(|row| row.row().physical_key().user_key().to_vec())
        .collect()
}

#[test]
fn branch_range_scan_opens_only_reached_owned_nonzero_tables() {
    let branch = branch_id(41);
    let mut state = BranchLocalState::empty(branch);
    install_single_row_tables(
        &mut state,
        branch,
        BranchLevel::new(1),
        "scan-owned-range",
        "key",
        100,
        1,
    );
    let bounds = scan_range(branch, "key-042", "key-045");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let rows = state
        .scan_including_tombstones_borrowed(&bounds, BranchReadBound::latest(), None, None)
        .expect("range scan");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(
        row_user_keys(&rows),
        vec![
            b"key-042".to_vec(),
            b"key-043".to_vec(),
            b"key-044".to_vec()
        ]
    );
    assert_eq!(perf.scan_active_cursors(), 1);
    assert_eq!(perf.scan_owned_l0_cursors(), 0);
    assert_eq!(perf.scan_owned_nonzero_level_cursors(), 1);
    assert_eq!(perf.scan_owned_nonzero_table_cursors_opened(), 3);
    assert_eq!(perf.scan_rows_visited(), 3);
    assert_eq!(perf.scan_candidates_materialized(), 3);
    assert_eq!(perf.scan_rows_returned(), 3);
}

#[test]
fn branch_prefix_scan_prunes_owned_nonzero_tables_before_cursor_creation() {
    let branch = branch_id(42);
    let mut state = BranchLocalState::empty(branch);
    install_single_row_tables(
        &mut state,
        branch,
        BranchLevel::new(1),
        "scan-prefix-before",
        "aaa",
        40,
        1,
    );
    install_single_row_tables(
        &mut state,
        branch,
        BranchLevel::new(1),
        "scan-prefix-match",
        "match",
        5,
        100,
    );
    install_single_row_tables(
        &mut state,
        branch,
        BranchLevel::new(1),
        "scan-prefix-after",
        "zzz",
        40,
        200,
    );
    let bounds = scan_prefix(branch, "match-");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let rows = state
        .scan_including_tombstones_borrowed(&bounds, BranchReadBound::latest(), None, None)
        .expect("prefix scan");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(rows.len(), 5);
    assert!(row_user_keys(&rows)
        .iter()
        .all(|key| key.starts_with(b"match-")));
    assert_eq!(perf.scan_owned_nonzero_level_cursors(), 1);
    assert_eq!(perf.scan_owned_nonzero_table_cursors_opened(), 5);
    assert_eq!(perf.scan_rows_visited(), 5);
    assert_eq!(perf.scan_candidates_materialized(), 5);
    assert_eq!(perf.scan_rows_returned(), 5);
}

#[test]
fn branch_limited_scan_does_not_open_all_owned_nonzero_tables() {
    let branch = branch_id(43);
    let mut state = BranchLocalState::empty(branch);
    install_single_row_tables(
        &mut state,
        branch,
        BranchLevel::new(1),
        "scan-owned-limit",
        "limit",
        100,
        1,
    );
    let bounds = BranchScanBounds::unbounded(
        branch,
        "default",
        StorageSpaceId::engine(0x20).expect("engine storage space"),
    )
    .expect("unbounded scan");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let rows = state
        .scan_including_tombstones_borrowed(&bounds, BranchReadBound::latest(), Some(10), None)
        .expect("limited scan");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(rows.len(), 10);
    assert_eq!(perf.scan_owned_nonzero_level_cursors(), 1);
    assert!(
        perf.scan_owned_nonzero_table_cursors_opened() <= 11,
        "limited scan opened {} nonzero table cursors",
        perf.scan_owned_nonzero_table_cursors_opened()
    );
    assert_eq!(perf.scan_rows_visited(), 10);
    assert_eq!(perf.scan_candidates_materialized(), 10);
    assert_eq!(perf.scan_rows_returned(), 10);
}

#[test]
fn branch_limited_scan_bounds_cursors_by_owned_nonzero_level_count() {
    let branch = branch_id(48);
    let mut state = BranchLocalState::empty(branch);
    install_single_row_tables(
        &mut state,
        branch,
        BranchLevel::new(1),
        "scan-owned-nonzero-a-limit",
        "multi-a",
        100,
        1,
    );
    install_single_row_tables(
        &mut state,
        branch,
        BranchLevel::new(2),
        "scan-owned-nonzero-b-limit",
        "multi-b",
        100,
        200,
    );
    let bounds = scan_prefix(branch, "multi-");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let rows = state
        .scan_including_tombstones_borrowed(&bounds, BranchReadBound::latest(), Some(10), None)
        .expect("limited scan");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(rows.len(), 10);
    assert!(row_user_keys(&rows)
        .iter()
        .all(|key| key.starts_with(b"multi-a-")));
    assert_eq!(perf.scan_owned_nonzero_level_cursors(), 2);
    assert!(
        perf.scan_owned_nonzero_table_cursors_opened() <= 12,
        "limited scan opened {} nonzero table cursors",
        perf.scan_owned_nonzero_table_cursors_opened()
    );
    assert_eq!(perf.scan_rows_visited(), 10);
    assert_eq!(perf.scan_candidates_materialized(), 10);
    assert_eq!(perf.scan_rows_returned(), 10);
}

#[test]
fn branch_zero_limit_scan_does_not_open_nonzero_tables() {
    let branch = branch_id(47);
    let mut state = BranchLocalState::empty(branch);
    install_single_row_tables(
        &mut state,
        branch,
        BranchLevel::new(1),
        "scan-owned-zero-limit",
        "zero-limit",
        100,
        1,
    );
    let bounds = BranchScanBounds::unbounded(
        branch,
        "default",
        StorageSpaceId::engine(0x20).expect("engine storage space"),
    )
    .expect("unbounded scan");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let rows = state
        .scan_including_tombstones_borrowed(&bounds, BranchReadBound::latest(), Some(0), None)
        .expect("zero-limit scan");
    let perf = crate::observability::perf_trace::snapshot();

    assert!(rows.is_empty());
    assert_eq!(perf.scan_active_cursors(), 0);
    assert_eq!(perf.scan_owned_nonzero_level_cursors(), 0);
    assert_eq!(perf.scan_owned_nonzero_table_cursors_opened(), 0);
    assert_eq!(perf.scan_rows_visited(), 0);
    assert_eq!(perf.scan_candidates_materialized(), 0);
    assert_eq!(perf.scan_rows_returned(), 0);
}

#[test]
fn branch_visible_scan_counts_returned_rows_after_visibility_filters() {
    let branch = branch_id(49);
    let mut state = BranchLocalState::empty(branch);
    state
        .append_committed_row(scan_row(branch, "counter-keep", 1))
        .expect("append retained row");
    state
        .append_committed_row(scan_row(branch, "counter-deleted", 2))
        .expect("append deleted row");
    state
        .append_committed_row(tombstone_row(branch, b"counter-deleted".to_vec(), 3, 30))
        .expect("append tombstone");
    state
        .append_committed_row(storage_row_with(
            branch,
            b"counter-expired".to_vec(),
            4,
            40,
            Timestamp::from_micros(45),
            b"expired".to_vec(),
        ))
        .expect("append expired row");
    let view = state.capture_read_view().expect("read view");
    let bounds = scan_range(branch, "counter-", "counter~");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let rows = view
        .scan_range(
            &bounds,
            BranchReadBound::at_timestamp(Timestamp::from_micros(50)),
        )
        .expect("visible scan");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].row().physical_key().user_key(), b"counter-keep");
    assert_eq!(perf.scan_rows_returned(), 1);
}

#[test]
fn branch_scan_keeps_l0_per_table_because_ranges_can_overlap() {
    let branch = branch_id(44);
    let mut state = BranchLocalState::empty(branch);
    for index in 0..25 {
        state
            .install_l0_table(scan_table(
                branch,
                BranchLevel::ZERO,
                &format!("scan-l0-overlap-{index:03}"),
                "overlap",
                1 + u64::try_from(index).expect("table index fits in u64"),
            ))
            .expect("install L0");
    }
    let bounds = scan_range(branch, "overlap", "overlap\0");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let rows = state
        .scan_including_tombstones_borrowed(&bounds, BranchReadBound::latest(), None, None)
        .expect("L0 scan");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].row().commit_version(), CommitVersion::new(25));
    assert_eq!(perf.scan_owned_l0_cursors(), 25);
    assert_eq!(perf.scan_owned_nonzero_level_cursors(), 0);
    assert_eq!(perf.scan_owned_nonzero_table_cursors_opened(), 0);
    assert_eq!(perf.scan_rows_visited(), 25);
    assert_eq!(perf.scan_candidates_materialized(), 25);
}

#[test]
fn branch_range_scan_uses_lazy_inherited_nonzero_level_after_key_rewrite() {
    let branch = branch_id(45);
    let parent = branch_id(46);
    let parent_tables = (0..100)
        .map(|index| {
            let key = format!("parent-{index:03}");
            let version = 1 + u64::try_from(index).expect("table index fits in u64");
            scan_table(
                parent,
                BranchLevel::new(1),
                &format!("scan-inherited-range-{index:03}"),
                &key,
                version,
            )
        })
        .collect::<Vec<_>>();
    let inherited = branch_inherited_layer(
        parent,
        CommitVersion::new(200),
        InheritedLayerStatus::Active,
        vec![Vec::new(), parent_tables],
    );
    let mut state = BranchLocalState::empty(branch);
    state
        .attach_inherited_layers(vec![inherited])
        .expect("attach inherited");
    let bounds = scan_range(branch, "parent-042", "parent-045");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let rows = state
        .scan_including_tombstones_borrowed(&bounds, BranchReadBound::latest(), None, None)
        .expect("inherited scan");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(
        row_user_keys(&rows),
        vec![
            b"parent-042".to_vec(),
            b"parent-043".to_vec(),
            b"parent-044".to_vec()
        ]
    );
    assert!(rows
        .iter()
        .all(|row| row.row().physical_key().branch_id() == branch));
    assert_eq!(perf.scan_inherited_nonzero_level_cursors(), 1);
    assert_eq!(perf.scan_inherited_nonzero_table_cursors_opened(), 3);
    assert_eq!(perf.scan_rows_visited(), 3);
    assert_eq!(perf.scan_candidates_materialized(), 3);
    assert_eq!(perf.scan_rows_returned(), 3);
}
