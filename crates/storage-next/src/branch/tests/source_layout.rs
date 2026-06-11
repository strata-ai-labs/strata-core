#![allow(clippy::too_many_lines)]

use super::*;
use crate::table::FrozenTable;

fn level_counts(counts: &[super::super::facts::BranchLevelTableCount]) -> Vec<(u8, usize)> {
    counts
        .iter()
        .map(|count| (count.level().raw(), count.table_count()))
        .collect()
}

fn mutable_table(rows: Vec<StorageRow>) -> MutableTable {
    let mut table = MutableTable::new();
    for row in rows {
        table.insert_row(row).expect("mutable insert");
    }
    table
}

fn layout_row(branch: BranchId, key: &str, version: u64) -> StorageRow {
    storage_row_with(
        branch,
        key.as_bytes().to_vec(),
        version,
        version.saturating_mul(10),
        Timestamp::EPOCH,
        key.as_bytes().to_vec(),
    )
}

fn layout_table(
    branch: BranchId,
    level: BranchLevel,
    identity: &str,
    rows: impl IntoIterator<Item = StorageRow>,
) -> BranchOwnedTable {
    branch_owned_table(branch, level, identity, rows.into_iter().collect())
}

fn record_layout_fact_row(
    row: &StorageRow,
    max_commit_version: &mut Option<CommitVersion>,
    timestamp_min: &mut Option<Timestamp>,
    timestamp_max: &mut Option<Timestamp>,
) {
    *max_commit_version = Some(max_commit_version.map_or(row.commit_version(), |current| {
        if row.commit_version().as_u64() > current.as_u64() {
            row.commit_version()
        } else {
            current
        }
    }));
    *timestamp_min = Some(timestamp_min.map_or(row.commit_timestamp(), |current| {
        current.min(row.commit_timestamp())
    }));
    *timestamp_max = Some(timestamp_max.map_or(row.commit_timestamp(), |current| {
        current.max(row.commit_timestamp())
    }));
}

fn read_view_facts(
    branch: BranchId,
    active: &MutableTable,
    frozen: &[FrozenTable],
    owned_levels: &[Vec<BranchOwnedTable>],
    inherited_layers: &[BranchInheritedLayer],
) -> BranchStateFacts {
    let mut max_commit_version = None;
    let mut timestamp_min = None;
    let mut timestamp_max = None;
    for row in active.iter() {
        record_layout_fact_row(
            row.row(),
            &mut max_commit_version,
            &mut timestamp_min,
            &mut timestamp_max,
        );
    }
    for table in frozen {
        for row in table.iter() {
            record_layout_fact_row(
                row.row(),
                &mut max_commit_version,
                &mut timestamp_min,
                &mut timestamp_max,
            );
        }
    }
    for table in owned_levels.iter().flatten() {
        for row in table.rows() {
            record_layout_fact_row(
                row.row(),
                &mut max_commit_version,
                &mut timestamp_min,
                &mut timestamp_max,
            );
        }
    }
    for layer in inherited_layers {
        if !matches!(
            layer.status(),
            InheritedLayerStatus::Active | InheritedLayerStatus::Materializing
        ) {
            continue;
        }
        for table in layer.owned_levels().iter().flatten() {
            for row in table.rows() {
                if row.commit_version().as_u64() <= layer.fork_version().as_u64() {
                    record_layout_fact_row(
                        row.row(),
                        &mut max_commit_version,
                        &mut timestamp_min,
                        &mut timestamp_max,
                    );
                }
            }
        }
    }
    BranchStateFacts::new(
        branch,
        u64::try_from(active.len()).expect("active row count fits in u64"),
        frozen.len(),
        owned_levels.iter().map(Vec::len).sum(),
        inherited_layers.len(),
        max_commit_version,
        timestamp_min,
        timestamp_max,
    )
    .expect("read view facts")
}

#[cfg(feature = "perf-trace")]
fn assert_source_counters_zero(perf: &crate::observability::perf_trace::StoragePerfSnapshot) {
    assert_eq!(perf.point_active_probes(), 0);
    assert_eq!(perf.point_frozen_probes(), 0);
    assert_eq!(perf.point_owned_l0_table_probes(), 0);
    assert_eq!(perf.point_owned_nonzero_level_searches(), 0);
    assert_eq!(perf.point_owned_nonzero_table_probes(), 0);
    assert_eq!(perf.point_inherited_layer_searches(), 0);
    assert_eq!(perf.point_inherited_l0_table_probes(), 0);
    assert_eq!(perf.point_inherited_nonzero_level_searches(), 0);
    assert_eq!(perf.point_inherited_nonzero_table_probes(), 0);
    assert_eq!(perf.point_table_seeks(), 0);
    assert_eq!(perf.scan_active_cursors(), 0);
    assert_eq!(perf.scan_frozen_cursors(), 0);
    assert_eq!(perf.scan_owned_l0_cursors(), 0);
    assert_eq!(perf.scan_owned_nonzero_level_cursors(), 0);
    assert_eq!(perf.scan_owned_nonzero_table_cursors_opened(), 0);
    assert_eq!(perf.scan_inherited_l0_cursors(), 0);
    assert_eq!(perf.scan_inherited_nonzero_level_cursors(), 0);
    assert_eq!(perf.scan_inherited_nonzero_table_cursors_opened(), 0);
    assert_eq!(perf.scan_source_cursor_seeks(), 0);
    assert_eq!(perf.scan_rows_returned(), 0);
    assert_eq!(perf.history_active_rows_visited(), 0);
    assert_eq!(perf.history_frozen_rows_visited(), 0);
    assert_eq!(perf.history_owned_l0_rows_visited(), 0);
    assert_eq!(perf.history_owned_nonzero_rows_visited(), 0);
    assert_eq!(perf.history_inherited_l0_rows_visited(), 0);
    assert_eq!(perf.history_inherited_nonzero_rows_visited(), 0);
    assert_eq!(perf.history_candidates_materialized(), 0);
    assert_eq!(perf.timestamp_active_rows_scanned(), 0);
    assert_eq!(perf.timestamp_frozen_rows_scanned(), 0);
    assert_eq!(perf.timestamp_owned_l0_rows_scanned(), 0);
    assert_eq!(perf.timestamp_owned_nonzero_rows_scanned(), 0);
    assert_eq!(perf.timestamp_inherited_l0_rows_scanned(), 0);
    assert_eq!(perf.timestamp_inherited_nonzero_rows_scanned(), 0);
    assert_eq!(perf.branch_facts_active_rows_observed(), 0);
    assert_eq!(perf.branch_facts_frozen_rows_observed(), 0);
    assert_eq!(perf.branch_facts_owned_l0_rows_observed(), 0);
    assert_eq!(perf.branch_facts_owned_nonzero_rows_observed(), 0);
    assert_eq!(perf.branch_facts_inherited_l0_rows_observed(), 0);
    assert_eq!(perf.branch_facts_inherited_nonzero_rows_observed(), 0);
}

#[test]
fn branch_source_layout_reports_active_and_frozen_shapes() {
    let branch = branch_id(222);
    let mut state = BranchLocalState::empty(branch);
    state
        .append_committed_row(layout_row(branch, "active-a", 1))
        .expect("append active");
    state
        .append_committed_row(layout_row(branch, "active-b", 2))
        .expect("append active");

    let active_layout = state.source_layout();
    assert_eq!(active_layout.active_rows(), 2);
    assert_eq!(active_layout.frozen_table_count(), 0);
    assert_eq!(active_layout.frozen_rows(), 0);
    assert_eq!(active_layout.owned_total_tables(), 0);
    assert_eq!(active_layout.inherited_total_tables(), 0);

    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .append_committed_row(layout_row(branch, "active-c", 3))
        .expect("append active");

    let frozen_layout = state.source_layout();
    assert_eq!(frozen_layout.active_rows(), 1);
    assert_eq!(frozen_layout.frozen_table_count(), 1);
    assert_eq!(frozen_layout.frozen_rows(), 2);
    assert_eq!(frozen_layout.owned_l0_tables(), 0);
    assert_eq!(frozen_layout.owned_nonzero_level_count(), 0);
    assert_eq!(frozen_layout.inherited_layers(), 0);
}

#[test]
fn branch_source_layout_reports_overlapping_owned_l0_tables() {
    let branch = branch_id(223);
    let mut state = BranchLocalState::empty(branch);
    for version in 1..=3 {
        state
            .install_l0_table(layout_table(
                branch,
                BranchLevel::ZERO,
                &format!("layout-l0-overlap-{version}"),
                [layout_row(branch, "overlap", version)],
            ))
            .expect("install overlapping L0 table");
    }

    let layout = state.source_layout();
    assert_eq!(layout.active_rows(), 0);
    assert_eq!(layout.frozen_table_count(), 0);
    assert_eq!(layout.owned_l0_tables(), 3);
    assert_eq!(layout.owned_nonzero_level_count(), 0);
    assert!(layout.owned_nonzero_level_table_counts().is_empty());
    assert_eq!(layout.owned_total_tables(), 3);
}

#[test]
fn branch_source_layout_reports_many_owned_nonzero_tables_by_level() {
    let branch = branch_id(224);
    let mut state = BranchLocalState::empty(branch);
    for index in 0..8 {
        state
            .install_owned_table_at_level(
                BranchLevel::new(1),
                layout_table(
                    branch,
                    BranchLevel::new(1),
                    &format!("layout-owned-nonzero-1-{index}"),
                    [layout_row(
                        branch,
                        &format!("level-1-{index:02}"),
                        10 + index,
                    )],
                ),
            )
            .expect("install level 1 table");
    }
    for index in 0..2 {
        state
            .install_owned_table_at_level(
                BranchLevel::new(2),
                layout_table(
                    branch,
                    BranchLevel::new(2),
                    &format!("layout-owned-nonzero-2-{index}"),
                    [layout_row(
                        branch,
                        &format!("level-2-{index:02}"),
                        40 + index,
                    )],
                ),
            )
            .expect("install level 2 table");
    }

    let layout = state.source_layout();
    assert_eq!(layout.owned_l0_tables(), 0);
    assert_eq!(
        level_counts(layout.owned_nonzero_level_table_counts()),
        vec![(1, 8), (2, 2)]
    );
    assert_eq!(layout.owned_nonzero_level_count(), 2);
    assert_eq!(layout.owned_total_tables(), 10);
}

#[test]
fn branch_source_layout_reports_inherited_l0_only_layer() {
    let branch = branch_id(225);
    let parent = branch_id(226);
    let mut state = BranchLocalState::empty(branch);
    state
        .attach_inherited_layers(vec![branch_inherited_layer(
            parent,
            CommitVersion::new(100),
            InheritedLayerStatus::Active,
            vec![vec![
                layout_table(
                    parent,
                    BranchLevel::ZERO,
                    "layout-inherited-l0-a",
                    [layout_row(parent, "inherited-l0-a", 10)],
                ),
                layout_table(
                    parent,
                    BranchLevel::ZERO,
                    "layout-inherited-l0-b",
                    [layout_row(parent, "inherited-l0-b", 11)],
                ),
            ]],
        )])
        .expect("attach inherited L0 layer");

    let layout = state.source_layout();
    assert_eq!(layout.inherited_layers(), 1);
    assert_eq!(layout.inherited_readable_layers(), 1);
    assert_eq!(layout.inherited_l0_tables(), 2);
    assert_eq!(layout.inherited_nonzero_level_count(), 0);
    assert!(layout.inherited_nonzero_level_table_counts().is_empty());
    assert_eq!(layout.inherited_total_tables(), 2);
}

#[test]
fn branch_source_layout_reports_mixed_owned_and_inherited_sources() {
    let branch = branch_id(227);
    let parent = branch_id(228);
    let active = mutable_table(vec![layout_row(branch, "mixed-active", 10)]);
    let frozen = vec![mutable_table(vec![layout_row(branch, "mixed-frozen", 11)]).freeze()];
    let owned_levels = vec![
        vec![layout_table(
            branch,
            BranchLevel::ZERO,
            "layout-mixed-owned-l0",
            [layout_row(branch, "mixed-owned-l0", 20)],
        )],
        vec![
            layout_table(
                branch,
                BranchLevel::new(1),
                "layout-mixed-owned-l1-a",
                [layout_row(branch, "mixed-owned-l1-a", 30)],
            ),
            layout_table(
                branch,
                BranchLevel::new(1),
                "layout-mixed-owned-l1-b",
                [layout_row(branch, "mixed-owned-l1-b", 31)],
            ),
        ],
    ];
    let inherited_layers = vec![branch_inherited_layer(
        parent,
        CommitVersion::new(100),
        InheritedLayerStatus::Active,
        vec![
            vec![layout_table(
                parent,
                BranchLevel::ZERO,
                "layout-mixed-inherited-l0",
                [layout_row(parent, "mixed-inherited-l0", 40)],
            )],
            Vec::new(),
            vec![layout_table(
                parent,
                BranchLevel::new(2),
                "layout-mixed-inherited-l2",
                [layout_row(parent, "mixed-inherited-l2", 41)],
            )],
        ],
    )];
    let facts = read_view_facts(branch, &active, &frozen, &owned_levels, &inherited_layers);
    let view = BranchReadView::new_with_inherited(
        branch,
        active,
        frozen,
        owned_levels,
        inherited_layers,
        facts,
    )
    .expect("mixed source view");

    let layout = view.source_layout();
    assert_eq!(layout.active_rows(), 1);
    assert_eq!(layout.frozen_table_count(), 1);
    assert_eq!(layout.frozen_rows(), 1);
    assert_eq!(layout.owned_l0_tables(), 1);
    assert_eq!(
        level_counts(layout.owned_nonzero_level_table_counts()),
        vec![(1, 2)]
    );
    assert_eq!(layout.owned_total_tables(), 3);
    assert_eq!(layout.inherited_layers(), 1);
    assert_eq!(layout.inherited_l0_tables(), 1);
    assert_eq!(
        level_counts(layout.inherited_nonzero_level_table_counts()),
        vec![(2, 1)]
    );
    assert_eq!(layout.inherited_total_tables(), 2);
}

#[test]
fn branch_inherited_nonzero_layout_rewrites_keys_and_applies_fork_bound() {
    let branch = branch_id(229);
    let parent = branch_id(230);
    let target = physical_key(branch, b"inherited-target".to_vec());
    let active = MutableTable::new();
    let frozen = Vec::new();
    let owned_levels = Vec::new();
    let inherited_layers = vec![branch_inherited_layer_unchecked_for_fork_gate_tests(
        parent,
        CommitVersion::new(100),
        InheritedLayerStatus::Active,
        vec![
            Vec::new(),
            vec![layout_table(
                parent,
                BranchLevel::new(1),
                "layout-inherited-fork-l1",
                [
                    layout_row(parent, "inherited-target", 90),
                    layout_row(parent, "inherited-target", 110),
                ],
            )],
        ],
    )];
    let facts = read_view_facts(branch, &active, &frozen, &owned_levels, &inherited_layers);
    let view = BranchReadView::new_with_inherited(
        branch,
        active,
        frozen,
        owned_levels,
        inherited_layers,
        facts,
    )
    .expect("inherited nonzero view");

    let layout = view.source_layout();
    assert_eq!(layout.inherited_layers(), 1);
    assert_eq!(layout.inherited_l0_tables(), 0);
    assert_eq!(
        level_counts(layout.inherited_nonzero_level_table_counts()),
        vec![(1, 1)]
    );
    assert_eq!(layout.inherited_total_tables(), 1);

    let row = view
        .latest(&target)
        .expect("inherited read")
        .expect("visible inherited row");
    assert_eq!(row.row().physical_key().branch_id(), branch);
    assert_eq!(row.row().commit_version(), CommitVersion::new(90));
    assert_eq!(
        row.source(),
        BranchRowSource::Inherited {
            source_branch_id: parent,
            layer_index: 0
        }
    );
}

#[test]
fn branch_source_layout_reports_multiple_inherited_layers_in_order() {
    let branch = branch_id(231);
    let near_parent = branch_id(232);
    let far_parent = branch_id(233);
    let active = MutableTable::new();
    let frozen = Vec::new();
    let owned_levels = Vec::new();
    let inherited_layers = vec![
        branch_inherited_layer(
            near_parent,
            CommitVersion::new(100),
            InheritedLayerStatus::Active,
            vec![vec![layout_table(
                near_parent,
                BranchLevel::ZERO,
                "layout-near-layer-l0",
                [layout_row(near_parent, "shared", 50)],
            )]],
        ),
        branch_inherited_layer(
            far_parent,
            CommitVersion::new(100),
            InheritedLayerStatus::Active,
            vec![vec![layout_table(
                far_parent,
                BranchLevel::ZERO,
                "layout-far-layer-l0",
                [layout_row(far_parent, "shared", 50)],
            )]],
        ),
    ];
    let facts = read_view_facts(branch, &active, &frozen, &owned_levels, &inherited_layers);
    let view = BranchReadView::new_with_inherited(
        branch,
        active,
        frozen,
        owned_levels,
        inherited_layers,
        facts,
    )
    .expect("multiple inherited layer view");

    let layout = view.source_layout();
    assert_eq!(layout.inherited_layers(), 2);
    assert_eq!(layout.inherited_readable_layers(), 2);
    assert_eq!(layout.inherited_l0_tables(), 2);
    assert_eq!(layout.inherited_total_tables(), 2);
    let row = view
        .latest(&physical_key(branch, b"shared".to_vec()))
        .expect("inherited read")
        .expect("visible inherited row");
    assert_eq!(
        row.source(),
        BranchRowSource::Inherited {
            source_branch_id: near_parent,
            layer_index: 0
        }
    );
}

#[test]
fn branch_source_layout_reports_owned_source_shape() {
    let branch = branch_id(211);
    let mut state = BranchLocalState::empty(branch);

    state
        .append_committed_row(storage_row_with(
            branch,
            b"active-a".to_vec(),
            10,
            100,
            Timestamp::EPOCH,
            b"active-a".to_vec(),
        ))
        .expect("append active");
    state
        .append_committed_row(storage_row_with(
            branch,
            b"active-b".to_vec(),
            11,
            110,
            Timestamp::EPOCH,
            b"active-b".to_vec(),
        ))
        .expect("append active");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .append_committed_row(storage_row_with(
            branch,
            b"active-c".to_vec(),
            12,
            120,
            Timestamp::EPOCH,
            b"active-c".to_vec(),
        ))
        .expect("append active");

    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "layout-owned-l0-a",
            vec![storage_row_with(
                branch,
                b"owned-l0-a".to_vec(),
                20,
                200,
                Timestamp::EPOCH,
                b"owned-l0-a".to_vec(),
            )],
        ))
        .expect("install owned L0");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "layout-owned-l0-b",
            vec![storage_row_with(
                branch,
                b"owned-l0-b".to_vec(),
                21,
                210,
                Timestamp::EPOCH,
                b"owned-l0-b".to_vec(),
            )],
        ))
        .expect("install owned L0");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "layout-owned-l1-a",
                vec![storage_row_with(
                    branch,
                    b"owned-l1-a".to_vec(),
                    30,
                    300,
                    Timestamp::EPOCH,
                    b"owned-l1-a".to_vec(),
                )],
            ),
        )
        .expect("install owned L1");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "layout-owned-l1-b",
                vec![storage_row_with(
                    branch,
                    b"owned-l1-b".to_vec(),
                    31,
                    310,
                    Timestamp::EPOCH,
                    b"owned-l1-b".to_vec(),
                )],
            ),
        )
        .expect("install owned L1");
    state
        .install_owned_table_at_level(
            BranchLevel::new(2),
            branch_owned_table(
                branch,
                BranchLevel::new(2),
                "layout-owned-l2-a",
                vec![storage_row_with(
                    branch,
                    b"owned-l2-a".to_vec(),
                    40,
                    400,
                    Timestamp::EPOCH,
                    b"owned-l2-a".to_vec(),
                )],
            ),
        )
        .expect("install owned L2");

    let layout = state.source_layout();
    assert_eq!(layout.active_rows(), 1);
    assert_eq!(layout.frozen_table_count(), 1);
    assert_eq!(layout.frozen_rows(), 2);
    assert_eq!(layout.owned_l0_tables(), 2);
    assert_eq!(
        level_counts(layout.owned_nonzero_level_table_counts()),
        vec![(1, 2), (2, 1)]
    );
    assert_eq!(layout.owned_nonzero_level_count(), 2);
    assert_eq!(layout.owned_total_tables(), 5);
    assert_eq!(layout.inherited_layers(), 0);
    assert_eq!(layout.inherited_total_tables(), 0);

    let view = state.capture_read_view().expect("read view");
    assert_eq!(view.source_layout(), layout);
}

#[test]
fn branch_source_layout_reports_inherited_status_shape() {
    let branch = branch_id(212);
    let parent = branch_id(213);
    let archived_parent = branch_id(214);
    let mut state = BranchLocalState::empty(branch);

    let active_layer = branch_inherited_layer(
        parent,
        CommitVersion::new(100),
        InheritedLayerStatus::Active,
        vec![
            vec![branch_owned_table(
                parent,
                BranchLevel::ZERO,
                "layout-parent-l0",
                vec![storage_row_with(
                    parent,
                    b"parent-l0".to_vec(),
                    50,
                    500,
                    Timestamp::EPOCH,
                    b"parent-l0".to_vec(),
                )],
            )],
            vec![
                branch_owned_table(
                    parent,
                    BranchLevel::new(1),
                    "layout-parent-l1-a",
                    vec![storage_row_with(
                        parent,
                        b"parent-l1-a".to_vec(),
                        51,
                        510,
                        Timestamp::EPOCH,
                        b"parent-l1-a".to_vec(),
                    )],
                ),
                branch_owned_table(
                    parent,
                    BranchLevel::new(1),
                    "layout-parent-l1-b",
                    vec![storage_row_with(
                        parent,
                        b"parent-l1-b".to_vec(),
                        52,
                        520,
                        Timestamp::EPOCH,
                        b"parent-l1-b".to_vec(),
                    )],
                ),
            ],
        ],
    );
    let materializing_layer = branch_inherited_layer(
        archived_parent,
        CommitVersion::new(100),
        InheritedLayerStatus::Materializing,
        vec![
            Vec::new(),
            Vec::new(),
            vec![branch_owned_table(
                archived_parent,
                BranchLevel::new(2),
                "layout-archived-l2",
                vec![storage_row_with(
                    archived_parent,
                    b"archived-l2".to_vec(),
                    60,
                    600,
                    Timestamp::EPOCH,
                    b"archived-l2".to_vec(),
                )],
            )],
        ],
    );
    let materialized_layer = branch_inherited_layer(
        branch_id(215),
        CommitVersion::new(100),
        InheritedLayerStatus::Materialized,
        Vec::new(),
    );
    state
        .attach_inherited_layers(vec![active_layer, materializing_layer, materialized_layer])
        .expect("attach inherited layers");

    let layout = state.source_layout();
    assert_eq!(layout.active_rows(), 0);
    assert_eq!(layout.frozen_table_count(), 0);
    assert_eq!(layout.owned_total_tables(), 0);
    assert_eq!(layout.inherited_layers(), 3);
    assert_eq!(layout.inherited_readable_layers(), 2);
    assert_eq!(layout.inherited_active_layers(), 1);
    assert_eq!(layout.inherited_materializing_layers(), 1);
    assert_eq!(layout.inherited_materialized_layers(), 1);
    assert_eq!(layout.inherited_unavailable_layers(), 0);
    assert_eq!(layout.inherited_l0_tables(), 1);
    assert_eq!(
        level_counts(layout.inherited_nonzero_level_table_counts()),
        vec![(1, 2), (2, 1)]
    );
    assert_eq!(layout.inherited_nonzero_level_count(), 2);
    assert_eq!(layout.inherited_total_tables(), 4);

    let view = state.capture_read_view().expect("read view");
    assert_eq!(view.source_layout(), layout);
}

#[test]
fn branch_source_layout_can_count_unavailable_inherited_layers() {
    let unavailable_layer = branch_inherited_layer_unchecked_for_fork_gate_tests(
        branch_id(216),
        CommitVersion::new(100),
        InheritedLayerStatus::Unavailable,
        Vec::new(),
    );
    let layout = super::super::read::source_layout_from_sources(
        &MutableTable::new(),
        &[],
        &[],
        &[unavailable_layer],
    );
    assert_eq!(layout.inherited_layers(), 1);
    assert_eq!(layout.inherited_readable_layers(), 0);
    assert_eq!(layout.inherited_active_layers(), 0);
    assert_eq!(layout.inherited_materializing_layers(), 0);
    assert_eq!(layout.inherited_materialized_layers(), 0);
    assert_eq!(layout.inherited_unavailable_layers(), 1);
    assert_eq!(layout.inherited_total_tables(), 0);
}

#[test]
fn branch_source_layout_is_available_from_read_views() {
    let branch = branch_id(217);
    let mut state = BranchLocalState::empty(branch);
    state
        .append_committed_row(storage_row_with(
            branch,
            b"active".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"active".to_vec(),
        ))
        .expect("append active");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "layout-view-l0",
            vec![storage_row_with(
                branch,
                b"l0".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"l0".to_vec(),
            )],
        ))
        .expect("install L0");

    let view = state.capture_read_view().expect("view");
    let layout = view.source_layout();
    assert_eq!(layout.active_rows(), 1);
    assert_eq!(layout.owned_l0_tables(), 1);
    assert_eq!(layout.owned_total_tables(), 1);
    assert_eq!(layout.inherited_layers(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_source_layout_does_not_increment_perf_trace_counters() {
    let branch = branch_id(234);
    let parent = branch_id(235);
    let active = mutable_table(vec![layout_row(branch, "active", 10)]);
    let frozen = vec![mutable_table(vec![layout_row(branch, "frozen", 11)]).freeze()];
    let owned_levels = vec![
        vec![layout_table(
            branch,
            BranchLevel::ZERO,
            "layout-counter-owned-l0",
            [layout_row(branch, "owned-l0", 20)],
        )],
        vec![layout_table(
            branch,
            BranchLevel::new(1),
            "layout-counter-owned-l1",
            [layout_row(branch, "owned-l1", 30)],
        )],
    ];
    let inherited_layers = vec![branch_inherited_layer(
        parent,
        CommitVersion::new(100),
        InheritedLayerStatus::Active,
        vec![
            vec![layout_table(
                parent,
                BranchLevel::ZERO,
                "layout-counter-inherited-l0",
                [layout_row(parent, "inherited-l0", 40)],
            )],
            vec![layout_table(
                parent,
                BranchLevel::new(1),
                "layout-counter-inherited-l1",
                [layout_row(parent, "inherited-l1", 41)],
            )],
        ],
    )];
    let facts = read_view_facts(branch, &active, &frozen, &owned_levels, &inherited_layers);
    let view = BranchReadView::new_with_inherited(
        branch,
        active,
        frozen,
        owned_levels,
        inherited_layers,
        facts,
    )
    .expect("source-layout counter view");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let layout = view.source_layout();
    assert_eq!(layout.active_rows(), 1);
    assert_eq!(layout.frozen_table_count(), 1);
    assert_eq!(layout.owned_total_tables(), 2);
    assert_eq!(layout.inherited_total_tables(), 2);
    assert_source_counters_zero(&crate::observability::perf_trace::snapshot());
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_source_counters_reset_and_snapshot_deterministically() {
    let branch = branch_id(236);
    let target = physical_key(branch, b"active".to_vec());
    let active = mutable_table(vec![layout_row(branch, "active", 10)]);
    let facts = read_view_facts(branch, &active, &[], &[], &[]);
    let view = BranchReadView::new(branch, active, Vec::new(), Vec::new(), facts)
        .expect("active-only view");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let first = view.latest(&target).expect("first point read");
    assert!(first.is_some());
    let first_snapshot = crate::observability::perf_trace::snapshot();
    assert_eq!(first_snapshot.point_active_probes(), 1);
    assert_eq!(first_snapshot.point_table_seeks(), 1);

    crate::observability::perf_trace::reset();
    assert_source_counters_zero(&crate::observability::perf_trace::snapshot());

    let second = view.latest(&target).expect("second point read");
    assert!(second.is_some());
    let second_snapshot = crate::observability::perf_trace::snapshot();
    assert_eq!(second_snapshot.point_active_probes(), 1);
    assert_eq!(second_snapshot.point_table_seeks(), 1);
    assert_eq!(second_snapshot.point_frozen_probes(), 0);
    assert_eq!(second_snapshot.point_owned_l0_table_probes(), 0);
    assert_eq!(second_snapshot.point_inherited_layer_searches(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_point_perf_trace_splits_source_classes() {
    let branch = branch_id(218);
    let parent = branch_id(219);
    let target = physical_key(branch, b"target".to_vec());
    let active = mutable_table(vec![storage_row_with(
        branch,
        b"target".to_vec(),
        10,
        100,
        Timestamp::EPOCH,
        b"active".to_vec(),
    )]);
    let frozen = vec![mutable_table(vec![storage_row_with(
        branch,
        b"frozen-only".to_vec(),
        11,
        110,
        Timestamp::EPOCH,
        b"frozen".to_vec(),
    )])
    .freeze()];
    let owned_levels = vec![
        vec![branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "point-owned-l0",
            vec![storage_row_with(
                branch,
                b"owned-l0".to_vec(),
                20,
                200,
                Timestamp::EPOCH,
                b"owned-l0".to_vec(),
            )],
        )],
        vec![branch_owned_table(
            branch,
            BranchLevel::new(1),
            "point-owned-l1",
            vec![storage_row_with(
                branch,
                b"target".to_vec(),
                30,
                300,
                Timestamp::EPOCH,
                b"owned-l1".to_vec(),
            )],
        )],
    ];
    let inherited_layers = vec![branch_inherited_layer(
        parent,
        CommitVersion::new(100),
        InheritedLayerStatus::Active,
        vec![
            vec![branch_owned_table(
                parent,
                BranchLevel::ZERO,
                "point-parent-l0",
                vec![storage_row_with(
                    parent,
                    b"parent-l0".to_vec(),
                    40,
                    400,
                    Timestamp::EPOCH,
                    b"parent-l0".to_vec(),
                )],
            )],
            vec![branch_owned_table(
                parent,
                BranchLevel::new(1),
                "point-parent-l1",
                vec![storage_row_with(
                    parent,
                    b"target".to_vec(),
                    41,
                    410,
                    Timestamp::EPOCH,
                    b"parent-l1".to_vec(),
                )],
            )],
        ],
    )];
    let facts = BranchStateFacts::new(
        branch,
        1,
        1,
        2,
        1,
        Some(CommitVersion::new(41)),
        Some(Timestamp::from_micros(100)),
        Some(Timestamp::from_micros(410)),
    )
    .expect("facts");
    let view = BranchReadView::new_with_inherited(
        branch,
        active,
        frozen,
        owned_levels,
        inherited_layers,
        facts,
    )
    .expect("view");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let _ = view.latest(&target).expect("point read");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(perf.point_active_probes(), 1);
    assert_eq!(perf.point_frozen_probes(), 1);
    assert_eq!(perf.point_owned_l0_table_probes(), 1);
    assert_eq!(perf.point_owned_nonzero_level_searches(), 1);
    assert_eq!(perf.point_owned_nonzero_table_probes(), 1);
    assert_eq!(perf.point_inherited_layer_searches(), 1);
    assert_eq!(perf.point_inherited_l0_table_probes(), 1);
    assert_eq!(perf.point_inherited_nonzero_level_searches(), 1);
    assert_eq!(perf.point_inherited_nonzero_table_probes(), 1);
    assert_eq!(perf.point_table_seeks(), 6);
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_read_view_scan_perf_trace_splits_source_classes() {
    let branch = branch_id(237);
    let parent = branch_id(238);
    let active = mutable_table(vec![layout_row(branch, "scan-active", 10)]);
    let frozen = vec![mutable_table(vec![layout_row(branch, "scan-frozen", 11)]).freeze()];
    let owned_levels = vec![
        vec![layout_table(
            branch,
            BranchLevel::ZERO,
            "scan-view-owned-l0",
            [layout_row(branch, "scan-owned-l0", 20)],
        )],
        vec![layout_table(
            branch,
            BranchLevel::new(1),
            "scan-view-owned-l1",
            [layout_row(branch, "scan-owned-l1", 30)],
        )],
    ];
    let inherited_layers = vec![branch_inherited_layer(
        parent,
        CommitVersion::new(100),
        InheritedLayerStatus::Active,
        vec![
            vec![layout_table(
                parent,
                BranchLevel::ZERO,
                "scan-view-parent-l0",
                [layout_row(parent, "scan-parent-l0", 40)],
            )],
            vec![layout_table(
                parent,
                BranchLevel::new(1),
                "scan-view-parent-l1",
                [layout_row(parent, "scan-parent-l1", 41)],
            )],
        ],
    )];
    let facts = read_view_facts(branch, &active, &frozen, &owned_levels, &inherited_layers);
    let view = BranchReadView::new_with_inherited(
        branch,
        active,
        frozen,
        owned_levels,
        inherited_layers,
        facts,
    )
    .expect("scan source view");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let bounds = BranchScanBounds::unbounded(
        branch,
        "default",
        StorageSpaceId::engine(0x20).expect("engine storage space"),
    )
    .expect("scan bounds");
    let rows = view
        .scan_range_including_tombstones(&bounds, BranchReadBound::latest())
        .expect("read-view scan");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(rows.len(), 6);
    assert_eq!(perf.scan_active_cursors(), 1);
    assert_eq!(perf.scan_frozen_cursors(), 1);
    assert_eq!(perf.scan_owned_l0_cursors(), 1);
    assert_eq!(perf.scan_owned_nonzero_level_cursors(), 1);
    assert_eq!(perf.scan_owned_nonzero_table_cursors_opened(), 1);
    assert_eq!(perf.scan_inherited_l0_cursors(), 1);
    assert_eq!(perf.scan_inherited_nonzero_level_cursors(), 1);
    assert_eq!(perf.scan_inherited_nonzero_table_cursors_opened(), 1);
    assert_eq!(perf.scan_rows_returned(), 6);
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_facts_perf_trace_does_not_scan_rows_on_hot_path() {
    let branch = branch_id(239);
    let active_parent = branch_id(240);
    let materializing_parent = branch_id(241);
    let materialized_parent = branch_id(242);
    let mut state = BranchLocalState::empty(branch);
    state
        .attach_inherited_layers(vec![
            branch_inherited_layer(
                active_parent,
                CommitVersion::new(100),
                InheritedLayerStatus::Active,
                vec![vec![layout_table(
                    active_parent,
                    BranchLevel::ZERO,
                    "facts-counter-active-l0",
                    [layout_row(active_parent, "facts-active", 10)],
                )]],
            ),
            branch_inherited_layer(
                materializing_parent,
                CommitVersion::new(90),
                InheritedLayerStatus::Materializing,
                vec![
                    Vec::new(),
                    vec![layout_table(
                        materializing_parent,
                        BranchLevel::new(1),
                        "facts-counter-materializing-l1",
                        [layout_row(materializing_parent, "facts-materializing", 20)],
                    )],
                ],
            ),
            branch_inherited_layer(
                materialized_parent,
                CommitVersion::new(80),
                InheritedLayerStatus::Materialized,
                vec![vec![layout_table(
                    materialized_parent,
                    BranchLevel::ZERO,
                    "facts-counter-materialized-l0",
                    [layout_row(materialized_parent, "facts-materialized", 30)],
                )]],
            ),
        ])
        .expect("attach inherited layers");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let _ = state.facts().expect("branch facts");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(perf.branch_facts_rows_observed(), 0);
    assert_eq!(perf.branch_facts_active_rows_observed(), 0);
    assert_eq!(perf.branch_facts_frozen_rows_observed(), 0);
    assert_eq!(perf.branch_facts_owned_l0_rows_observed(), 0);
    assert_eq!(perf.branch_facts_owned_nonzero_rows_observed(), 0);
    assert_eq!(perf.branch_facts_inherited_l0_rows_observed(), 0);
    assert_eq!(perf.branch_facts_inherited_nonzero_rows_observed(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_scan_history_timestamp_and_facts_perf_trace_split_source_classes() {
    let branch = branch_id(220);
    let parent = branch_id(221);

    let mut own_state = BranchLocalState::empty(branch);
    own_state
        .append_committed_row(storage_row_with(
            branch,
            b"target".to_vec(),
            10,
            100,
            Timestamp::EPOCH,
            b"active".to_vec(),
        ))
        .expect("append active");
    own_state
        .append_committed_row(storage_row_with(
            branch,
            b"frozen".to_vec(),
            11,
            110,
            Timestamp::EPOCH,
            b"frozen".to_vec(),
        ))
        .expect("append active");
    assert!(matches!(
        own_state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    own_state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "scan-owned-l0",
            vec![storage_row_with(
                branch,
                b"owned-l0".to_vec(),
                20,
                200,
                Timestamp::EPOCH,
                b"owned-l0".to_vec(),
            )],
        ))
        .expect("install L0");
    own_state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "scan-owned-l1",
                vec![storage_row_with(
                    branch,
                    b"owned-l1".to_vec(),
                    30,
                    300,
                    Timestamp::EPOCH,
                    b"owned-l1".to_vec(),
                )],
            ),
        )
        .expect("install L1");

    let mut inherited_state = BranchLocalState::empty(branch);
    inherited_state
        .attach_inherited_layers(vec![branch_inherited_layer(
            parent,
            CommitVersion::new(100),
            InheritedLayerStatus::Active,
            vec![
                vec![branch_owned_table(
                    parent,
                    BranchLevel::ZERO,
                    "scan-parent-l0",
                    vec![storage_row_with(
                        parent,
                        b"parent-l0".to_vec(),
                        40,
                        400,
                        Timestamp::EPOCH,
                        b"parent-l0".to_vec(),
                    )],
                )],
                vec![branch_owned_table(
                    parent,
                    BranchLevel::new(1),
                    "scan-parent-l1",
                    vec![storage_row_with(
                        parent,
                        b"parent-l1".to_vec(),
                        41,
                        410,
                        Timestamp::EPOCH,
                        b"parent-l1".to_vec(),
                    )],
                )],
            ],
        )])
        .expect("attach inherited layer");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let bounds = BranchScanBounds::unbounded(
        branch,
        "default",
        StorageSpaceId::engine(0x20).expect("engine storage space"),
    )
    .expect("scan bounds");
    let own_rows = own_state
        .scan_including_tombstones_borrowed(&bounds, BranchReadBound::latest(), None, None)
        .expect("own scan");
    let inherited_rows = inherited_state
        .scan_including_tombstones_borrowed(&bounds, BranchReadBound::latest(), None, None)
        .expect("inherited scan");
    assert!(!own_rows.is_empty());
    assert!(!inherited_rows.is_empty());

    let own_view = own_state.capture_read_view().expect("own view");
    let target = physical_key(branch, b"target".to_vec());
    let _ = own_view
        .history(&target, BranchHistoryOptions::all())
        .expect("own history");
    let inherited_view = inherited_state.capture_read_view().expect("inherited view");
    let _ = inherited_view
        .history(
            &physical_key(branch, b"parent-l0".to_vec()),
            BranchHistoryOptions::all(),
        )
        .expect("inherited history");
    let _ = own_state.resolve_timestamp_to_commit_version(Timestamp::from_micros(500));
    let _ = inherited_state.resolve_timestamp_to_commit_version(Timestamp::from_micros(500));
    let _ = own_state.facts().expect("own facts");
    let _ = inherited_state.facts().expect("inherited facts");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(perf.scan_active_cursors(), 2);
    assert_eq!(perf.scan_frozen_cursors(), 1);
    assert_eq!(perf.scan_owned_l0_cursors(), 1);
    assert_eq!(perf.scan_owned_nonzero_level_cursors(), 1);
    assert_eq!(perf.scan_owned_nonzero_table_cursors_opened(), 1);
    assert_eq!(perf.scan_inherited_l0_cursors(), 1);
    assert_eq!(perf.scan_inherited_nonzero_level_cursors(), 1);
    assert_eq!(perf.scan_inherited_nonzero_table_cursors_opened(), 1);
    assert_eq!(perf.scan_source_cursor_seeks(), 7);
    assert_eq!(
        perf.scan_rows_returned(),
        (own_rows.len() + inherited_rows.len()) as u64
    );
    assert_eq!(perf.history_active_rows_visited(), 0);
    assert_eq!(perf.history_frozen_rows_visited(), 1);
    assert_eq!(perf.history_owned_l0_rows_visited(), 0);
    assert_eq!(perf.history_owned_nonzero_rows_visited(), 0);
    assert_eq!(perf.history_inherited_l0_rows_visited(), 1);
    assert_eq!(perf.history_inherited_nonzero_rows_visited(), 0);
    assert_eq!(perf.history_candidates_materialized(), 2);
    assert_eq!(perf.timestamp_active_rows_scanned(), 0);
    assert_eq!(perf.timestamp_frozen_rows_scanned(), 2);
    assert_eq!(perf.timestamp_owned_l0_rows_scanned(), 1);
    assert_eq!(perf.timestamp_owned_nonzero_rows_scanned(), 1);
    assert_eq!(perf.timestamp_inherited_l0_rows_scanned(), 1);
    assert_eq!(perf.timestamp_inherited_nonzero_rows_scanned(), 1);
    assert_eq!(perf.branch_facts_rows_observed(), 0);
    assert_eq!(perf.branch_facts_active_rows_observed(), 0);
    assert_eq!(perf.branch_facts_frozen_rows_observed(), 0);
    assert_eq!(perf.branch_facts_owned_l0_rows_observed(), 0);
    assert_eq!(perf.branch_facts_owned_nonzero_rows_observed(), 0);
    assert_eq!(perf.branch_facts_inherited_l0_rows_observed(), 0);
    assert_eq!(perf.branch_facts_inherited_nonzero_rows_observed(), 0);
}
