use super::*;

fn point_row(branch: BranchId, user_key: &str, version: u64) -> StorageRow {
    storage_row_with(
        branch,
        user_key.as_bytes().to_vec(),
        version,
        version.saturating_mul(10),
        Timestamp::EPOCH,
        user_key.as_bytes().to_vec(),
    )
}

fn point_table(
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
        vec![point_row(branch, user_key, version)],
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
) -> Vec<StorageRow> {
    (0..count)
        .map(|index| {
            let key = format!("{key_prefix}-{index:03}");
            let version = base_version + u64::try_from(index).expect("table index fits in u64");
            let row = point_row(branch, &key, version);
            state
                .install_owned_table_at_level(
                    level,
                    branch_owned_table(
                        branch,
                        level,
                        &format!("{identity_prefix}-{index:03}"),
                        vec![row.clone()],
                    ),
                )
                .expect("install table");
            row
        })
        .collect()
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_point_read_prunes_owned_nonzero_level_to_selected_table() {
    let branch = branch_id(12);
    let mut state = BranchLocalState::empty(branch);
    let rows = install_single_row_tables(
        &mut state,
        branch,
        BranchLevel::new(1),
        "point-prune-owned",
        "key",
        100,
        1,
    );
    let target = physical_key(branch, b"key-042".to_vec());
    let expected = rows[42].clone();
    let view = state.capture_read_view().expect("read view");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let actual = view.latest(&target).expect("point read");
    let perf = crate::observability::perf_trace::snapshot();

    assert_visible_row(
        actual.as_ref(),
        &expected,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 42,
        },
    );
    assert_eq!(perf.point_active_probes(), 1);
    assert_eq!(perf.point_owned_l0_table_probes(), 0);
    assert_eq!(perf.point_owned_nonzero_level_searches(), 1);
    assert_eq!(perf.point_owned_nonzero_table_probes(), 1);
    assert_eq!(perf.point_table_seeks(), 2);
    assert_eq!(perf.point_rows_visited(), 1);
    assert!(perf.point_owned_nonzero_table_probes() <= perf.point_owned_nonzero_level_searches());
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_point_read_selects_owned_nonzero_table_by_multi_key_range_edges() {
    let branch = branch_id(18);
    let mut state = BranchLocalState::empty(branch);
    for index in 0..10 {
        let key = format!("low-{index:03}");
        state
            .install_owned_table_at_level(
                BranchLevel::new(1),
                point_table(
                    branch,
                    BranchLevel::new(1),
                    &format!("point-prune-low-{index:03}"),
                    &key,
                    1 + u64::try_from(index).expect("table index fits in u64"),
                ),
            )
            .expect("install low table");
    }
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "point-prune-range",
                vec![
                    point_row(branch, "range-010", 1000),
                    point_row(branch, "range-011", 1001),
                    point_row(branch, "range-012", 1002),
                ],
            ),
        )
        .expect("install range table");
    for index in 0..10 {
        let key = format!("zzz-{index:03}");
        state
            .install_owned_table_at_level(
                BranchLevel::new(1),
                point_table(
                    branch,
                    BranchLevel::new(1),
                    &format!("point-prune-high-{index:03}"),
                    &key,
                    2000 + u64::try_from(index).expect("table index fits in u64"),
                ),
            )
            .expect("install high table");
    }
    let view = state.capture_read_view().expect("read view");

    for (user_key, version) in [
        ("range-010", 1000),
        ("range-011", 1001),
        ("range-012", 1002),
    ] {
        let target = physical_key(branch, user_key.as_bytes().to_vec());
        let expected = point_row(branch, user_key, version);

        let _capture = crate::observability::perf_trace::begin_test_capture();
        let actual = view.latest(&target).expect("point read");
        let perf = crate::observability::perf_trace::snapshot();

        assert_visible_row(
            actual.as_ref(),
            &expected,
            BranchRowSource::OwnedTable {
                level: BranchLevel::new(1),
                table_index: 10,
            },
        );
        assert_eq!(perf.point_owned_nonzero_level_searches(), 1);
        assert_eq!(perf.point_owned_nonzero_table_probes(), 1);
        assert_eq!(perf.point_table_seeks(), 2);
        assert_eq!(perf.point_rows_visited(), 1);
    }
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_point_read_skips_nonzero_table_seek_when_key_is_outside_level_ranges() {
    let branch = branch_id(13);
    let mut state = BranchLocalState::empty(branch);
    install_single_row_tables(
        &mut state,
        branch,
        BranchLevel::new(1),
        "point-prune-miss",
        "key",
        100,
        1,
    );
    let target = physical_key(branch, b"zzz".to_vec());
    let view = state.capture_read_view().expect("read view");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let actual = view.latest(&target).expect("point read");
    let perf = crate::observability::perf_trace::snapshot();

    assert!(actual.is_none());
    assert_eq!(perf.point_active_probes(), 1);
    assert_eq!(perf.point_owned_nonzero_level_searches(), 1);
    assert_eq!(perf.point_owned_nonzero_table_probes(), 0);
    assert_eq!(perf.point_table_seeks(), 1);
    assert_eq!(perf.point_rows_visited(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_point_read_prunes_each_owned_nonzero_level_independently() {
    let branch = branch_id(14);
    let mut state = BranchLocalState::empty(branch);
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            point_table(branch, BranchLevel::new(1), "point-prune-l1", "shared", 10),
        )
        .expect("install level 1");
    state
        .install_owned_table_at_level(
            BranchLevel::new(2),
            point_table(branch, BranchLevel::new(2), "point-prune-l2", "shared", 20),
        )
        .expect("install level 2");
    install_single_row_tables(
        &mut state,
        branch,
        BranchLevel::new(1),
        "point-prune-l1-filler",
        "aaa",
        40,
        100,
    );
    install_single_row_tables(
        &mut state,
        branch,
        BranchLevel::new(2),
        "point-prune-l2-filler",
        "zzz",
        40,
        200,
    );
    let target = physical_key(branch, b"shared".to_vec());
    let expected = point_row(branch, "shared", 20);
    let view = state.capture_read_view().expect("read view");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let actual = view.latest(&target).expect("point read");
    let perf = crate::observability::perf_trace::snapshot();

    assert_visible_row(
        actual.as_ref(),
        &expected,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(2),
            table_index: 0,
        },
    );
    assert_eq!(perf.point_owned_nonzero_level_searches(), 2);
    assert_eq!(perf.point_owned_nonzero_table_probes(), 2);
    assert_eq!(perf.point_table_seeks(), 3);
    assert_eq!(perf.point_rows_visited(), 2);
    assert!(perf.point_owned_nonzero_table_probes() <= perf.point_owned_nonzero_level_searches());
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_point_read_prunes_inherited_nonzero_levels_after_key_rewrite() {
    let branch = branch_id(15);
    let parent = branch_id(16);
    let mut parent_tables = Vec::new();
    for index in 0..100 {
        let key = format!("parent-{index:03}");
        let version = 1 + u64::try_from(index).expect("table index fits in u64");
        parent_tables.push(branch_owned_table(
            parent,
            BranchLevel::new(1),
            &format!("point-prune-parent-{index:03}"),
            vec![point_row(parent, &key, version)],
        ));
    }
    let inherited = branch_inherited_layer(
        parent,
        CommitVersion::new(200),
        InheritedLayerStatus::Active,
        vec![Vec::new(), parent_tables],
    );
    let mut state = BranchLocalState::empty(branch);
    state
        .attach_inherited_layers(vec![inherited])
        .expect("attach inherited layer");
    let target = physical_key(branch, b"parent-042".to_vec());
    let expected = point_row(branch, "parent-042", 43);
    let view = state.capture_read_view().expect("read view");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let actual = view.latest(&target).expect("point read");
    let perf = crate::observability::perf_trace::snapshot();

    assert_visible_row(
        actual.as_ref(),
        &expected,
        BranchRowSource::Inherited {
            source_branch_id: parent,
            layer_index: 0,
        },
    );
    assert_eq!(perf.point_inherited_layer_searches(), 1);
    assert_eq!(perf.point_inherited_l0_table_probes(), 0);
    assert_eq!(perf.point_inherited_nonzero_level_searches(), 1);
    assert_eq!(perf.point_inherited_nonzero_table_probes(), 1);
    assert_eq!(perf.point_table_seeks(), 2);
    assert_eq!(perf.point_rows_visited(), 1);
    assert!(
        perf.point_inherited_nonzero_table_probes()
            <= perf.point_inherited_nonzero_level_searches()
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_point_read_keeps_l0_linear_because_ranges_can_overlap() {
    let branch = branch_id(17);
    let mut state = BranchLocalState::empty(branch);
    for index in 0..25 {
        state
            .install_l0_table(point_table(
                branch,
                BranchLevel::ZERO,
                &format!("point-prune-overlap-{index:03}"),
                "overlap",
                1 + u64::try_from(index).expect("table index fits in u64"),
            ))
            .expect("install L0");
    }
    let target = physical_key(branch, b"overlap".to_vec());
    let expected = point_row(branch, "overlap", 25);
    let view = state.capture_read_view().expect("read view");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let actual = view.latest(&target).expect("point read");
    let perf = crate::observability::perf_trace::snapshot();

    assert_visible_row(
        actual.as_ref(),
        &expected,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_eq!(perf.point_owned_l0_table_probes(), 25);
    assert_eq!(perf.point_owned_nonzero_level_searches(), 0);
    assert_eq!(perf.point_owned_nonzero_table_probes(), 0);
    assert_eq!(perf.point_table_seeks(), 26);
    assert_eq!(perf.point_rows_visited(), 25);
}
