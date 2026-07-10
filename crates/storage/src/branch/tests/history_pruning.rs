use super::*;
use crate::observability::perf_trace;
use crate::table::FrozenTable;

fn history_row(branch: BranchId, key: &[u8], version: u64) -> StorageRow {
    storage_row_with(
        branch,
        key.to_vec(),
        version,
        version.saturating_mul(10),
        Timestamp::EPOCH,
        key.to_vec(),
    )
}

fn history_table(
    branch: BranchId,
    level: BranchLevel,
    identity: &str,
    rows: Vec<StorageRow>,
) -> BranchOwnedTable {
    branch_owned_table(branch, level, identity, rows)
}

fn record_fact_row(
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

fn read_view_facts_for_history(
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
        record_fact_row(
            row.row(),
            &mut max_commit_version,
            &mut timestamp_min,
            &mut timestamp_max,
        );
    }
    for table in frozen {
        for row in table.iter() {
            record_fact_row(
                row.row(),
                &mut max_commit_version,
                &mut timestamp_min,
                &mut timestamp_max,
            );
        }
    }
    for table in owned_levels.iter().flatten() {
        for row in table.rows() {
            record_fact_row(
                row.row(),
                &mut max_commit_version,
                &mut timestamp_min,
                &mut timestamp_max,
            );
        }
    }
    for layer in inherited_layers {
        if layer.status() == InheritedLayerStatus::Materialized {
            continue;
        }
        for table in layer.owned_levels().iter().flatten() {
            for row in table.rows() {
                if row.commit_version().as_u64() <= layer.fork_version().as_u64() {
                    record_fact_row(
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

fn unrelated_table(
    branch: BranchId,
    level: BranchLevel,
    identity_prefix: &str,
    key_prefix: &str,
    index: u64,
    version: u64,
) -> BranchOwnedTable {
    let identity = format!("{identity_prefix}-{index:03}");
    let key = format!("{key_prefix}-{index:03}");
    history_table(
        branch,
        level,
        &identity,
        vec![history_row(branch, key.as_bytes(), version)],
    )
}

fn install_unrelated_nonzero_tables(
    state: &mut BranchLocalState,
    branch: BranchId,
    start_version: u64,
) {
    for index in 0..8 {
        state
            .install_owned_table_at_level(
                BranchLevel::new(1),
                unrelated_table(
                    branch,
                    BranchLevel::new(1),
                    "history-owned-before",
                    "aaa-history-before",
                    index,
                    start_version + index,
                ),
            )
            .expect("install unrelated table before target");
    }
    for index in 0..8 {
        state
            .install_owned_table_at_level(
                BranchLevel::new(1),
                unrelated_table(
                    branch,
                    BranchLevel::new(1),
                    "history-owned-after",
                    "zzz-history-after",
                    index,
                    start_version + 20 + index,
                ),
            )
            .expect("install unrelated table after target");
    }
}

fn inherited_nonzero_tables(parent: BranchId, target: &[u8]) -> Vec<BranchOwnedTable> {
    let mut tables = Vec::new();
    for index in 0..8 {
        tables.push(unrelated_table(
            parent,
            BranchLevel::new(1),
            "history-parent-before",
            "aaa-history-parent-before",
            index,
            10 + index,
        ));
    }
    tables.push(history_table(
        parent,
        BranchLevel::new(1),
        "history-parent-target",
        vec![
            history_row(parent, target, 50),
            history_row(parent, target, 40),
        ],
    ));
    for index in 0..8 {
        tables.push(unrelated_table(
            parent,
            BranchLevel::new(1),
            "history-parent-after",
            "zzz-history-parent-after",
            index,
            20 + index,
        ));
    }
    tables
}

fn assert_branch_facts_rows_not_observed(perf: &perf_trace::StoragePerfSnapshot) {
    assert_eq!(perf.branch_facts_rows_observed(), 0);
    assert_eq!(perf.branch_facts_active_rows_observed(), 0);
    assert_eq!(perf.branch_facts_frozen_rows_observed(), 0);
    assert_eq!(perf.branch_facts_owned_l0_rows_observed(), 0);
    assert_eq!(perf.branch_facts_owned_nonzero_rows_observed(), 0);
    assert_eq!(perf.branch_facts_inherited_l0_rows_observed(), 0);
    assert_eq!(perf.branch_facts_inherited_nonzero_rows_observed(), 0);
}

#[test]
fn single_key_history_visits_only_the_target_version_chain() {
    let branch = branch_id(181);
    let parent = branch_id(182);
    let target = b"history-target";
    let mut state = BranchLocalState::empty(branch);

    state
        .append_committed_row(history_row(branch, target, 100))
        .expect("append target to frozen");
    for index in 0..32 {
        let key = format!("frozen-unrelated-{index:03}");
        state
            .append_committed_row(history_row(branch, key.as_bytes(), 1_000 + index))
            .expect("append unrelated frozen row");
    }
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));

    state
        .append_committed_row(history_row(branch, target, 120))
        .expect("append target to active");
    for index in 0..32 {
        let key = format!("active-unrelated-{index:03}");
        state
            .append_committed_row(history_row(branch, key.as_bytes(), 1_100 + index))
            .expect("append unrelated active row");
    }

    state
        .install_l0_table(history_table(
            branch,
            BranchLevel::ZERO,
            "history-owned-l0",
            vec![
                history_row(branch, b"owned-l0-unrelated", 89),
                history_row(branch, target, 90),
                history_row(branch, target, 80),
                history_row(branch, b"owned-l0-unrelated-z", 79),
            ],
        ))
        .expect("install owned L0 table");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            history_table(
                branch,
                BranchLevel::new(1),
                "history-owned-target",
                vec![
                    history_row(branch, target, 70),
                    history_row(branch, target, 60),
                ],
            ),
        )
        .expect("install target nonzero table");
    install_unrelated_nonzero_tables(&mut state, branch, 2_000);

    let inherited_layers = vec![branch_inherited_layer(
        parent,
        CommitVersion::new(55),
        InheritedLayerStatus::Active,
        vec![Vec::new(), inherited_nonzero_tables(parent, target)],
    )];
    let facts = read_view_facts_for_history(
        branch,
        state.active(),
        state.frozen(),
        state.owned_levels(),
        &inherited_layers,
    );
    let view = BranchReadView::new_with_inherited(
        branch,
        state.active().clone(),
        state.frozen().to_vec(),
        state.owned_levels().to_vec(),
        inherited_layers,
        facts,
    )
    .expect("capture read view");
    let target_key = physical_key(branch, target.to_vec());

    let _capture = perf_trace::begin_test_capture();
    let history = view
        .history(&target_key, BranchHistoryOptions::all())
        .expect("single-key history");
    let perf = perf_trace::snapshot();

    assert_eq!(
        history_versions(&history),
        vec![120, 100, 90, 80, 70, 60, 50, 40]
    );
    for row in &history {
        assert_eq!(row.row().physical_key(), &target_key);
    }

    assert_eq!(perf.history_active_rows_visited(), 1);
    assert_eq!(perf.history_frozen_rows_visited(), 1);
    assert_eq!(perf.history_owned_l0_rows_visited(), 2);
    assert_eq!(perf.history_owned_nonzero_rows_visited(), 2);
    assert_eq!(perf.history_inherited_l0_rows_visited(), 0);
    assert_eq!(perf.history_inherited_nonzero_rows_visited(), 2);
    assert_eq!(perf.history_candidates_materialized(), 8);
    assert_eq!(perf.point_rows_visited(), 8);
}

#[test]
fn single_key_history_options_preserve_semantics_without_unrelated_row_visits() {
    let branch = branch_id(188);
    let target = b"history-options";
    let target_key = physical_key(branch, target.to_vec());
    let mut state = BranchLocalState::empty(branch);

    state
        .append_committed_row(storage_row_with(
            branch,
            target.to_vec(),
            50,
            500,
            Timestamp::from_micros(450),
            b"expires-but-stays-in-history".to_vec(),
        ))
        .expect("append expiring target row");
    state
        .append_committed_row(tombstone_row(branch, target.to_vec(), 40, 400))
        .expect("append target tombstone");
    state
        .append_committed_row(history_row(branch, target, 30))
        .expect("append older target row");
    for index in 0..48 {
        let key = format!("history-options-unrelated-{index:03}");
        state
            .append_committed_row(history_row(branch, key.as_bytes(), 1_000 + index))
            .expect("append unrelated active row");
    }

    let view = state.capture_read_view().expect("capture read view");

    let _capture = perf_trace::begin_test_capture();
    let all = view
        .history(&target_key, BranchHistoryOptions::all())
        .expect("all history");
    let without_tombstones = view
        .history(
            &target_key,
            BranchHistoryOptions::all().include_tombstones(false),
        )
        .expect("history without tombstones");
    let before_fifty = view
        .history(
            &target_key,
            BranchHistoryOptions::all().before_version(CommitVersion::new(50)),
        )
        .expect("history before version");
    let limited = view
        .history(&target_key, BranchHistoryOptions::all().limit(2))
        .expect("limited history");
    let zero_limit = view
        .history(&target_key, BranchHistoryOptions::all().limit(0))
        .expect("zero-limit history");
    let perf = perf_trace::snapshot();

    assert_eq!(history_versions(&all), vec![50, 40, 30]);
    assert_eq!(all[0].row().expires_at(), Timestamp::from_micros(450));
    assert!(all[1].row().is_tombstone());
    assert_eq!(history_versions(&without_tombstones), vec![50, 30]);
    assert_eq!(history_versions(&before_fifty), vec![40, 30]);
    assert_eq!(history_versions(&limited), vec![50, 40]);
    assert!(zero_limit.is_empty());
    assert_eq!(perf.history_active_rows_visited(), 12);
    assert_eq!(perf.history_frozen_rows_visited(), 0);
    assert_eq!(perf.history_owned_l0_rows_visited(), 0);
    assert_eq!(perf.history_owned_nonzero_rows_visited(), 0);
    assert_eq!(perf.history_inherited_l0_rows_visited(), 0);
    assert_eq!(perf.history_inherited_nonzero_rows_visited(), 0);
    assert_eq!(perf.history_candidates_materialized(), 12);
    assert_eq!(perf.point_rows_visited(), 12);
}

#[test]
fn inherited_history_keeps_fork_bounds_rewrites_rows_and_records_key_local_work() {
    let parent = branch_id(189);
    let child = branch_id(190);
    let target = b"inherited-history-pruning";
    let child_key = physical_key(child, target.to_vec());

    let inherited_layers = vec![branch_inherited_layer_unchecked_for_fork_gate_tests(
        parent,
        CommitVersion::new(35),
        InheritedLayerStatus::Active,
        vec![
            vec![history_table(
                parent,
                BranchLevel::ZERO,
                "history-parent-l0",
                vec![
                    history_row(parent, b"inherited-history-l0-unrelated", 90),
                    history_row(parent, target, 30),
                    history_row(parent, target, 20),
                ],
            )],
            vec![history_table(
                parent,
                BranchLevel::new(1),
                "history-parent-nonzero-target",
                vec![
                    history_row(parent, target, 40),
                    tombstone_row(parent, target.to_vec(), 35, 350),
                    history_row(parent, target, 10),
                ],
            )],
        ],
    )];
    let mut state = BranchLocalState::empty(child);
    state
        .append_committed_row(tombstone_row(child, target.to_vec(), 50, 500))
        .expect("append child tombstone");
    let facts = read_view_facts_for_history(
        child,
        state.active(),
        state.frozen(),
        state.owned_levels(),
        &inherited_layers,
    );
    let view = BranchReadView::new_with_inherited(
        child,
        state.active().clone(),
        state.frozen().to_vec(),
        state.owned_levels().to_vec(),
        inherited_layers,
        facts,
    )
    .expect("capture read view");

    let _capture = perf_trace::begin_test_capture();
    let latest = view.latest(&child_key).expect("latest");
    let all = view
        .history(&child_key, BranchHistoryOptions::all())
        .expect("inherited history");
    let without_tombstones = view
        .history(
            &child_key,
            BranchHistoryOptions::all().include_tombstones(false),
        )
        .expect("inherited history without tombstones");
    let perf = perf_trace::snapshot();

    assert!(latest.is_none());
    assert_eq!(history_versions(&all), vec![50, 35, 30, 20, 10]);
    assert_eq!(history_versions(&without_tombstones), vec![30, 20, 10]);
    assert!(history_versions(&all).iter().all(|version| *version != 40));
    for row in all.iter().filter(|row| {
        matches!(
            row.source(),
            BranchRowSource::Inherited {
                source_branch_id,
                layer_index: 0,
            } if source_branch_id == parent
        )
    }) {
        assert_eq!(row.row().physical_key(), &child_key);
    }
    assert_eq!(perf.history_active_rows_visited(), 2);
    assert_eq!(perf.history_frozen_rows_visited(), 0);
    assert_eq!(perf.history_owned_l0_rows_visited(), 0);
    assert_eq!(perf.history_owned_nonzero_rows_visited(), 0);
    assert_eq!(perf.history_inherited_l0_rows_visited(), 4);
    assert_eq!(perf.history_inherited_nonzero_rows_visited(), 6);
    assert_eq!(perf.history_candidates_materialized(), 10);
}

#[test]
fn branch_facts_use_maintained_state_without_row_observation() {
    let branch = branch_id(183);
    let parent = branch_id(184);
    let mut state = BranchLocalState::empty(branch);

    state
        .attach_inherited_layers(vec![branch_inherited_layer(
            parent,
            CommitVersion::new(35),
            InheritedLayerStatus::Active,
            vec![vec![history_table(
                parent,
                BranchLevel::ZERO,
                "facts-parent-l0",
                vec![history_row(parent, b"facts-parent", 30)],
            )]],
        )])
        .expect("attach inherited layer");

    let _capture = perf_trace::begin_test_capture();
    let facts = state.facts().expect("branch facts");
    let perf = perf_trace::snapshot();

    assert_eq!(facts.active_rows(), 0);
    assert_eq!(facts.frozen_table_count(), 0);
    assert_eq!(facts.owned_table_count(), 0);
    assert_eq!(facts.inherited_layer_count(), 1);
    assert_eq!(facts.max_commit_version(), Some(CommitVersion::new(30)));
    assert_eq!(facts.timestamp_min(), Some(Timestamp::from_micros(300)));
    assert_eq!(facts.timestamp_max(), Some(Timestamp::from_micros(300)));
    assert_branch_facts_rows_not_observed(&perf);
}

#[test]
fn branch_facts_preserve_own_row_counters_without_hot_path_row_observation() {
    let branch = branch_id(187);
    let mut state = BranchLocalState::empty(branch);

    state
        .append_committed_row(history_row(branch, b"facts-put", 10))
        .expect("append put row");
    state
        .append_committed_row(tombstone_row(branch, b"facts-tombstone".to_vec(), 20, 200))
        .expect("append tombstone row");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .install_l0_table(history_table(
            branch,
            BranchLevel::ZERO,
            "facts-owned-put",
            vec![history_row(branch, b"facts-owned-put", 30)],
        ))
        .expect("install owned table");

    let _capture = perf_trace::begin_test_capture();
    let facts = state.facts().expect("branch facts");
    let perf = perf_trace::snapshot();

    assert_eq!(facts.active_rows(), 0);
    assert_eq!(facts.frozen_table_count(), 1);
    assert_eq!(facts.owned_table_count(), 1);
    assert_eq!(facts.max_commit_version(), Some(CommitVersion::new(30)));
    assert_eq!(facts.timestamp_min(), Some(Timestamp::from_micros(100)));
    assert_eq!(facts.timestamp_max(), Some(Timestamp::from_micros(300)));
    assert_eq!(state.put_rows(), 2);
    assert_eq!(state.tombstone_rows(), 1);
    assert_branch_facts_rows_not_observed(&perf);
}

#[test]
fn branch_facts_stay_maintained_after_flush_replacement_without_hot_path_row_observation() {
    let branch = branch_id(197);
    let mut state = BranchLocalState::empty(branch);
    let flushed_rows = sorted_storage_rows(vec![
        history_row(branch, b"facts-flush-put", 10),
        tombstone_row(branch, b"facts-flush-tombstone".to_vec(), 20, 200),
    ]);

    for row in &flushed_rows {
        state
            .append_committed_row(row.clone())
            .expect("append flushed row");
    }
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    let outcome = state
        .replace_frozen_with_l0_table(
            0,
            history_table(
                branch,
                BranchLevel::ZERO,
                "facts-flush-replacement",
                flushed_rows,
            ),
        )
        .expect("replace frozen table with immutable output");
    assert_eq!(outcome.replaced_frozen_index(), Some(0));

    let _capture = perf_trace::begin_test_capture();
    let facts = state.facts().expect("branch facts");
    let perf = perf_trace::snapshot();

    assert_eq!(facts.active_rows(), 0);
    assert_eq!(facts.frozen_table_count(), 0);
    assert_eq!(facts.owned_table_count(), 1);
    assert_eq!(facts.max_commit_version(), Some(CommitVersion::new(20)));
    assert_eq!(facts.timestamp_min(), Some(Timestamp::from_micros(100)));
    assert_eq!(facts.timestamp_max(), Some(Timestamp::from_micros(200)));
    assert_eq!(state.put_rows(), 1);
    assert_eq!(state.tombstone_rows(), 1);
    assert_branch_facts_rows_not_observed(&perf);
}

#[test]
fn branch_facts_stay_maintained_after_compaction_without_hot_path_row_observation() {
    let branch = branch_id(191);
    let mut state = BranchLocalState::empty(branch);

    state
        .append_committed_row(history_row(branch, b"facts-frozen", 10))
        .expect("append frozen row");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .install_l0_table(history_table(
            branch,
            BranchLevel::ZERO,
            "facts-compact-a",
            vec![history_row(branch, b"facts-compact-a", 20)],
        ))
        .expect("install first compacted table");
    state
        .install_l0_table(history_table(
            branch,
            BranchLevel::ZERO,
            "facts-compact-b",
            vec![tombstone_row(branch, b"facts-compact-b".to_vec(), 30, 300)],
        ))
        .expect("install second compacted table");

    let request = BranchCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0,
        "facts-compact-output",
    )
    .expect("compaction request");
    let outcome = state
        .compact_branch_owned_tables(&request)
        .expect("compact branch-owned tables");
    assert!(outcome.installed_replacement_tables());

    let _capture = perf_trace::begin_test_capture();
    let facts = state.facts().expect("branch facts");
    let perf = perf_trace::snapshot();

    assert_eq!(facts.active_rows(), 0);
    assert_eq!(facts.frozen_table_count(), 1);
    assert_eq!(facts.owned_table_count(), 1);
    assert_eq!(facts.max_commit_version(), Some(CommitVersion::new(30)));
    assert_eq!(facts.timestamp_min(), Some(Timestamp::from_micros(100)));
    assert_eq!(facts.timestamp_max(), Some(Timestamp::from_micros(300)));
    assert_eq!(state.put_rows(), 2);
    assert_eq!(state.tombstone_rows(), 1);
    assert_branch_facts_rows_not_observed(&perf);
}

#[test]
fn branch_facts_stay_maintained_after_materialization_without_hot_path_row_observation() {
    let parent = branch_id(192);
    let child = branch_id(193);
    let mut state = BranchLocalState::empty(child);
    state
        .attach_inherited_layers(vec![branch_inherited_layer(
            parent,
            CommitVersion::new(30),
            InheritedLayerStatus::Active,
            vec![vec![history_table(
                parent,
                BranchLevel::ZERO,
                "facts-materialize-parent",
                vec![
                    history_row(parent, b"facts-parent", 20),
                    history_row(parent, b"facts-parent-after-fork", 30),
                ],
            )]],
        )])
        .expect("attach inherited layer");

    state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "facts-materialized")
                .expect("materialization request"),
        )
        .expect("materialize inherited layer");
    state
        .append_committed_row(history_row(child, b"facts-child", 10))
        .expect("append child row");

    let _capture = perf_trace::begin_test_capture();
    let facts = state.facts().expect("branch facts");
    let perf = perf_trace::snapshot();

    assert_eq!(facts.active_rows(), 1);
    assert_eq!(facts.frozen_table_count(), 0);
    assert_eq!(facts.owned_table_count(), 1);
    assert_eq!(facts.inherited_layer_count(), 0);
    assert_eq!(facts.max_commit_version(), Some(CommitVersion::new(30)));
    assert_eq!(facts.timestamp_min(), Some(Timestamp::from_micros(100)));
    assert_eq!(facts.timestamp_max(), Some(Timestamp::from_micros(300)));
    assert_eq!(state.put_rows(), 3);
    assert_eq!(state.tombstone_rows(), 0);
    assert_branch_facts_rows_not_observed(&perf);
}

#[test]
fn branch_facts_stay_maintained_after_snapshot_install_without_hot_path_row_observation() {
    let branch = branch_id(194);
    let mut branches = Vec::new();

    let snapshot_rows = sorted_storage_rows(vec![
        history_row(branch, b"facts-snapshot-a", 40),
        tombstone_row(branch, b"facts-snapshot-b".to_vec(), 50, 500),
    ]);
    let request = BranchSnapshotInstallRequest::from_rows("facts-snapshot", snapshot_rows)
        .expect("snapshot request")
        .with_missing_branch_policy(BranchSnapshotMissingBranchPolicy::Create {
            config: BranchRuntimeConfig::default(),
        })
        .with_max_rows_per_table(1)
        .expect("snapshot table split config");
    let outcome = install_snapshot_rows_into_branches(&mut branches, &request)
        .expect("install snapshot rows");
    assert_eq!(outcome.rows_installed(), 2);
    assert_eq!(outcome.tables_created(), 2);
    assert_eq!(outcome.branches_created(), 1);
    assert_eq!(outcome.branches_replaced(), 0);

    let _capture = perf_trace::begin_test_capture();
    let facts = branches[0].facts().expect("branch facts");
    let perf = perf_trace::snapshot();

    assert_eq!(facts.active_rows(), 0);
    assert_eq!(facts.frozen_table_count(), 0);
    assert_eq!(facts.owned_table_count(), 2);
    assert_eq!(facts.inherited_layer_count(), 0);
    assert_eq!(facts.max_commit_version(), Some(CommitVersion::new(50)));
    assert_eq!(facts.timestamp_min(), Some(Timestamp::from_micros(400)));
    assert_eq!(facts.timestamp_max(), Some(Timestamp::from_micros(500)));
    assert_eq!(branches[0].put_rows(), 1);
    assert_eq!(branches[0].tombstone_rows(), 1);
    assert_branch_facts_rows_not_observed(&perf);
}

#[test]
fn timestamp_lookup_scan_remains_counter_visible_until_timeline_facts_exist() {
    let branch = branch_id(185);
    let parent = branch_id(186);
    let mut state = BranchLocalState::empty(branch);

    state
        .attach_inherited_layers(vec![branch_inherited_layer(
            parent,
            CommitVersion::new(35),
            InheritedLayerStatus::Active,
            vec![vec![history_table(
                parent,
                BranchLevel::ZERO,
                "timestamp-parent-l0",
                vec![history_row(parent, b"timestamp-parent", 30)],
            )]],
        )])
        .expect("attach inherited layer");

    let _capture = perf_trace::begin_test_capture();
    let version = state
        .resolve_timestamp_to_commit_version(Timestamp::from_micros(350))
        .expect("resolve timestamp");
    let perf = perf_trace::snapshot();

    assert_eq!(version, Some(CommitVersion::new(30)));
    assert_eq!(perf.timestamp_active_rows_scanned(), 0);
    assert_eq!(perf.timestamp_frozen_rows_scanned(), 0);
    assert_eq!(perf.timestamp_owned_l0_rows_scanned(), 0);
    assert_eq!(perf.timestamp_inherited_l0_rows_scanned(), 1);
}

#[test]
fn timestamp_lookup_miss_and_fork_cap_are_counter_visible_until_timeline_facts_exist() {
    let branch = branch_id(195);
    let parent = branch_id(196);
    let mut state = BranchLocalState::empty(branch);
    state
        .attach_inherited_layers(vec![branch_inherited_layer_unchecked_for_fork_gate_tests(
            parent,
            CommitVersion::new(25),
            InheritedLayerStatus::Active,
            vec![vec![history_table(
                parent,
                BranchLevel::ZERO,
                "timestamp-parent-l0-fork",
                vec![
                    history_row(parent, b"timestamp-parent-visible", 25),
                    history_row(parent, b"timestamp-parent-after-fork", 40),
                ],
            )]],
        )])
        .expect("attach inherited layer");
    state
        .append_committed_row(history_row(branch, b"timestamp-own", 20))
        .expect("append own row");
    state
        .install_l0_table(history_table(
            branch,
            BranchLevel::ZERO,
            "timestamp-owned-l0",
            vec![history_row(branch, b"timestamp-owned", 30)],
        ))
        .expect("install owned table");

    let _capture = perf_trace::begin_test_capture();
    let miss = state
        .resolve_timestamp_to_commit_version(Timestamp::from_micros(5))
        .expect("resolve timestamp");
    let at_fork = state
        .resolve_timestamp_to_commit_version(Timestamp::from_micros(450))
        .expect("resolve timestamp");
    let perf = perf_trace::snapshot();

    assert_eq!(miss, None);
    assert_eq!(at_fork, Some(CommitVersion::new(30)));
    assert_eq!(perf.timestamp_active_rows_scanned(), 2);
    assert_eq!(perf.timestamp_owned_l0_rows_scanned(), 2);
    assert_eq!(perf.timestamp_inherited_l0_rows_scanned(), 4);
}

#[test]
fn timestamp_lookup_after_lifecycle_source_changes_stays_counter_visible_until_timeline_facts_exist(
) {
    let flushed_branch = branch_id(198);
    let mut flushed_state = BranchLocalState::empty(flushed_branch);
    let flushed_row = history_row(flushed_branch, b"timestamp-flushed", 10);
    flushed_state
        .append_committed_row(flushed_row.clone())
        .expect("append flushed row");
    assert!(matches!(
        flushed_state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    flushed_state
        .replace_frozen_with_l0_table(
            0,
            history_table(
                flushed_branch,
                BranchLevel::ZERO,
                "timestamp-flush-output",
                vec![flushed_row],
            ),
        )
        .expect("replace frozen table with immutable output");
    flushed_state
        .install_l0_table(history_table(
            flushed_branch,
            BranchLevel::ZERO,
            "timestamp-compaction-input",
            vec![history_row(flushed_branch, b"timestamp-compacted", 20)],
        ))
        .expect("install compaction input");
    let compaction_request = BranchCompactionRequest::new(
        flushed_branch,
        BranchCompactionKind::CompactL0,
        "timestamp-compaction-output",
    )
    .expect("compaction request");
    flushed_state
        .compact_branch_owned_tables(&compaction_request)
        .expect("compact post-flush tables");

    let parent = branch_id(199);
    let materialized_branch = branch_id(200);
    let mut materialized_state = BranchLocalState::empty(materialized_branch);
    materialized_state
        .attach_inherited_layers(vec![branch_inherited_layer(
            parent,
            CommitVersion::new(30),
            InheritedLayerStatus::Active,
            vec![vec![history_table(
                parent,
                BranchLevel::ZERO,
                "timestamp-materialization-source",
                vec![history_row(parent, b"timestamp-materialized", 30)],
            )]],
        )])
        .expect("attach inherited layer");
    materialized_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(
                materialized_branch,
                0,
                "timestamp-materialization-output",
            )
            .expect("materialization request"),
        )
        .expect("materialize inherited layer");

    let snapshot_branch = branch_id(201);
    let mut snapshot_states = Vec::new();
    let snapshot_rows = sorted_storage_rows(vec![
        history_row(snapshot_branch, b"timestamp-snapshot-a", 40),
        history_row(snapshot_branch, b"timestamp-snapshot-b", 50),
    ]);
    let snapshot_request =
        BranchSnapshotInstallRequest::from_rows("timestamp-snapshot-output", snapshot_rows)
            .expect("snapshot request")
            .with_missing_branch_policy(BranchSnapshotMissingBranchPolicy::Create {
                config: BranchRuntimeConfig::default(),
            });
    install_snapshot_rows_into_branches(&mut snapshot_states, &snapshot_request)
        .expect("install snapshot rows");

    let _capture = perf_trace::begin_test_capture();
    let flushed_version = flushed_state
        .resolve_timestamp_to_commit_version(Timestamp::from_micros(250))
        .expect("resolve timestamp");
    let materialized_version = materialized_state
        .resolve_timestamp_to_commit_version(Timestamp::from_micros(350))
        .expect("resolve timestamp");
    let snapshot_version = snapshot_states[0]
        .resolve_timestamp_to_commit_version(Timestamp::from_micros(550))
        .expect("resolve timestamp");
    let perf = perf_trace::snapshot();

    assert_eq!(flushed_version, Some(CommitVersion::new(20)));
    assert_eq!(materialized_version, Some(CommitVersion::new(30)));
    assert_eq!(snapshot_version, Some(CommitVersion::new(50)));
    assert_eq!(perf.timestamp_active_rows_scanned(), 0);
    assert_eq!(perf.timestamp_frozen_rows_scanned(), 0);
    assert_eq!(perf.timestamp_owned_l0_rows_scanned(), 5);
    assert_eq!(perf.timestamp_owned_nonzero_rows_scanned(), 0);
    assert_eq!(perf.timestamp_inherited_l0_rows_scanned(), 0);
    assert_eq!(perf.timestamp_inherited_nonzero_rows_scanned(), 0);
}
