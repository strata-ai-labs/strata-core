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

#[cfg(feature = "perf-trace")]
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

fn inherited_nonzero_layer(
    source: BranchId,
    user_key: &str,
    version: u64,
    identity: &str,
) -> BranchInheritedLayer {
    branch_inherited_layer(
        source,
        CommitVersion::new(version.saturating_add(100)),
        InheritedLayerStatus::Active,
        vec![
            Vec::new(),
            vec![point_table(
                source,
                BranchLevel::new(1),
                identity,
                user_key,
                version,
            )],
        ],
    )
}

#[cfg(feature = "perf-trace")]
fn inherited_mixed_layer(
    source: BranchId,
    user_key: &str,
    l0_version: u64,
    nonzero_version: u64,
) -> BranchInheritedLayer {
    branch_inherited_layer(
        source,
        CommitVersion::new(l0_version.max(nonzero_version).saturating_add(100)),
        InheritedLayerStatus::Active,
        vec![
            vec![point_table(
                source,
                BranchLevel::ZERO,
                "point-baseline-parent-l0",
                user_key,
                l0_version,
            )],
            vec![point_table(
                source,
                BranchLevel::new(1),
                "point-baseline-parent-nonzero",
                user_key,
                nonzero_version,
            )],
        ],
    )
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_point_read_counters_capture_active_early_exit() {
    let branch = branch_id(180);
    let parent = branch_id(181);
    let mut state = BranchLocalState::empty(branch);
    state
        .attach_inherited_layers(vec![inherited_mixed_layer(parent, "target", 20, 10)])
        .expect("attach inherited");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            point_table(
                branch,
                BranchLevel::new(1),
                "point-baseline-owned-nonzero",
                "target",
                30,
            ),
        )
        .expect("install nonzero");
    state
        .install_l0_table(point_table(
            branch,
            BranchLevel::ZERO,
            "point-baseline-owned-l0",
            "target",
            40,
        ))
        .expect("install L0");
    let frozen = point_row(branch, "target", 50);
    state.append_committed_row(frozen).expect("append frozen");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    let active = point_row(branch, "target", 60);
    state
        .append_committed_row(active.clone())
        .expect("append active");
    let view = state.capture_read_view().expect("read view");
    let target = physical_key(branch, b"target".to_vec());

    let _capture = crate::observability::perf_trace::begin_test_capture();
    assert_eq!(
        crate::observability::perf_trace::snapshot().point_table_seeks(),
        0
    );
    let actual = view.latest(&target).expect("point read");
    let perf = crate::observability::perf_trace::snapshot();

    assert_visible_row(actual.as_ref(), &active, BranchRowSource::Active);
    assert_eq!(perf.point_active_probes(), 1);
    assert_eq!(perf.point_frozen_probes(), 0);
    assert_eq!(perf.point_owned_l0_table_probes(), 0);
    assert_eq!(perf.point_owned_nonzero_level_searches(), 0);
    assert_eq!(perf.point_owned_nonzero_table_probes(), 0);
    assert_eq!(perf.point_inherited_layer_searches(), 0);
    assert_eq!(perf.point_inherited_l0_table_probes(), 0);
    assert_eq!(perf.point_inherited_nonzero_level_searches(), 0);
    assert_eq!(perf.point_inherited_nonzero_table_probes(), 0);
    assert_eq!(perf.point_table_seeks(), 1);
    assert_eq!(perf.point_candidates_materialized(), 1);
    assert_eq!(perf.point_candidate_row_clones(), 1);
    assert_eq!(perf.point_selected_active(), 1);
    assert_eq!(perf.point_selected_frozen(), 0);
    assert_eq!(perf.point_selected_owned_l0(), 0);
    assert_eq!(perf.point_selected_owned_nonzero(), 0);
    assert_eq!(perf.point_selected_inherited(), 0);
    assert_eq!(perf.point_inherited_key_rewrites(), 0);
    assert_eq!(perf.point_early_exit_active(), 1);
    assert_eq!(perf.point_early_exit_frozen(), 0);
    assert_eq!(perf.point_early_exit_owned_l0(), 0);
    assert_eq!(perf.point_early_exit_owned_nonzero(), 0);
    assert_eq!(perf.point_early_exit_inherited(), 1);
    assert_eq!(perf.point_remaining_source_skips(), 6);
    assert_eq!(perf.table_point_lookup_key_builds(), 1);
    assert_eq!(perf.table_point_lookup_key_reuses(), 0);
    assert_eq!(perf.table_eager_filter_probes(), 0);
    assert_eq!(perf.table_eager_filter_unavailable_probes(), 0);
    assert_eq!(perf.table_eager_filter_negative_probes(), 0);
    assert_eq!(perf.table_eager_filter_positive_probes(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_point_read_does_not_early_exit_when_later_source_can_beat_active() {
    let branch = branch_id(182);
    let parent = branch_id(183);
    let mut state = BranchLocalState::empty(branch);
    state
        .attach_inherited_layers(vec![inherited_nonzero_layer(
            parent,
            "target",
            5,
            "point-unsafe-exit-parent",
        )])
        .expect("attach inherited");
    let owned = point_row(branch, "target", 70);
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "point-unsafe-exit-owned",
                vec![owned.clone()],
            ),
        )
        .expect("install nonzero");
    state
        .append_committed_row(point_row(branch, "target", 60))
        .expect("append active");
    let view = state.capture_read_view().expect("read view");
    let target = physical_key(branch, b"target".to_vec());

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let actual = view.latest(&target).expect("point read");
    let perf = crate::observability::perf_trace::snapshot();

    assert_visible_row(
        actual.as_ref(),
        &owned,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );
    assert_eq!(perf.point_active_probes(), 1);
    assert_eq!(perf.point_owned_nonzero_level_searches(), 1);
    assert_eq!(perf.point_owned_nonzero_table_probes(), 1);
    assert_eq!(perf.point_inherited_layer_searches(), 0);
    assert_eq!(perf.point_table_seeks(), 2);
    assert_eq!(perf.point_candidates_materialized(), 2);
    assert_eq!(perf.point_candidate_row_clones(), 1);
    assert_eq!(perf.point_selected_active(), 0);
    assert_eq!(perf.point_selected_owned_nonzero(), 1);
    assert_eq!(perf.point_early_exit_active(), 0);
    assert_eq!(perf.point_early_exit_owned_nonzero(), 0);
    assert_eq!(perf.point_early_exit_inherited(), 1);
    assert_eq!(perf.point_remaining_source_skips(), 2);
    assert_eq!(perf.point_inherited_key_rewrites(), 0);
    assert_eq!(perf.table_point_lookup_key_builds(), 1);
    assert_eq!(perf.table_point_lookup_key_reuses(), 1);
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_point_read_deferred_clone_does_not_clone_loser_row_value() {
    let branch = branch_id(187);
    let mut state = BranchLocalState::empty(branch);
    let large_loser_value = vec![0xab; 128 * 1024];
    let large_loser = storage_row_with(
        branch,
        b"target".to_vec(),
        60,
        600,
        Timestamp::EPOCH,
        large_loser_value.clone(),
    );
    let winner = point_row(branch, "target", 70);
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "point-deferred-clone-loser",
                vec![large_loser],
            ),
        )
        .expect("install nonzero");
    state
        .install_owned_table_at_level(
            BranchLevel::new(2),
            branch_owned_table(
                branch,
                BranchLevel::new(2),
                "point-deferred-clone-winner",
                vec![winner.clone()],
            ),
        )
        .expect("install second nonzero");
    let view = state.capture_read_view().expect("read view");
    let target = physical_key(branch, b"target".to_vec());

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let actual = view.latest(&target).expect("point read");
    let perf = crate::observability::perf_trace::snapshot();

    assert_visible_row(
        actual.as_ref(),
        &winner,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(2),
            table_index: 0,
        },
    );
    assert_eq!(perf.point_candidates_materialized(), 2);
    assert_eq!(perf.point_candidate_row_clones(), 1);
    assert!(
        perf.point_candidate_row_clone_bytes()
            < u64::try_from(large_loser_value.len()).expect("value size fits in u64"),
        "loser value should not contribute to point candidate clone bytes"
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_point_read_at_version_early_exits_when_remaining_ranges_start_above_bound() {
    let branch = branch_id(186);
    let mut state = BranchLocalState::empty(branch);
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            point_table(
                branch,
                BranchLevel::new(1),
                "point-at-version-range-above-bound",
                "target",
                100,
            ),
        )
        .expect("install nonzero");
    let active = point_row(branch, "target", 5);
    state
        .append_committed_row(active.clone())
        .expect("append active");
    let view = state.capture_read_view().expect("read view");
    let target = physical_key(branch, b"target".to_vec());

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let actual = view
        .at_version(&target, CommitVersion::new(10))
        .expect("bounded read");
    let perf = crate::observability::perf_trace::snapshot();

    assert_visible_row(actual.as_ref(), &active, BranchRowSource::Active);
    assert_eq!(perf.point_active_probes(), 1);
    assert_eq!(perf.point_owned_nonzero_level_searches(), 0);
    assert_eq!(perf.point_owned_nonzero_table_probes(), 0);
    assert_eq!(perf.point_table_seeks(), 1);
    assert_eq!(perf.point_candidates_materialized(), 1);
    assert_eq!(perf.point_selected_active(), 1);
    assert_eq!(perf.point_early_exit_active(), 1);
    assert_eq!(perf.point_remaining_source_skips(), 1);
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_point_read_local_tombstone_early_exits_before_inherited_sources() {
    let branch = branch_id(184);
    let parent = branch_id(185);
    let inherited_row = point_row(parent, "deleted", 3);
    let inherited = branch_inherited_layer(
        parent,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![
            Vec::new(),
            vec![branch_owned_table(
                parent,
                BranchLevel::new(1),
                "point-tombstone-early-exit-parent",
                vec![inherited_row],
            )],
        ],
    );
    let mut state = BranchLocalState::empty(branch);
    state
        .attach_inherited_layers(vec![inherited])
        .expect("attach inherited");
    let tombstone = tombstone_row(branch, b"deleted".to_vec(), 6, 60);
    state
        .append_committed_row(tombstone.clone())
        .expect("append tombstone");
    let view = state.capture_read_view().expect("read view");
    let target = physical_key(branch, b"deleted".to_vec());

    {
        let _capture = crate::observability::perf_trace::begin_test_capture();
        let latest = view.latest(&target).expect("latest");
        let perf = crate::observability::perf_trace::snapshot();

        assert!(latest.is_none());
        assert_eq!(perf.point_active_probes(), 1);
        assert_eq!(perf.point_inherited_layer_searches(), 0);
        assert_eq!(perf.point_inherited_nonzero_level_searches(), 0);
        assert_eq!(perf.point_table_seeks(), 1);
        assert_eq!(perf.point_candidates_materialized(), 1);
        assert_eq!(perf.point_selected_active(), 1);
        assert_eq!(perf.point_early_exit_active(), 0);
        assert_eq!(perf.point_early_exit_inherited(), 1);
        assert_eq!(perf.point_remaining_source_skips(), 2);
        assert_eq!(perf.point_inherited_key_rewrites(), 0);
    }

    {
        let _capture = crate::observability::perf_trace::begin_test_capture();
        let selected = state
            .read_point_or_tombstone_borrowed(&target, BranchReadBound::latest())
            .expect("borrowed tombstone");
        let perf = crate::observability::perf_trace::snapshot();

        assert_eq!(selected.expect("selected tombstone").row(), &tombstone);
        assert_eq!(perf.point_active_probes(), 1);
        assert_eq!(perf.point_inherited_layer_searches(), 0);
        assert_eq!(perf.point_early_exit_active(), 0);
        assert_eq!(perf.point_early_exit_inherited(), 1);
    }
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_point_read_child_local_row_short_circuits_inherited_boundary() {
    let branch = branch_id(188);
    let parent = branch_id(189);
    let mut state = BranchLocalState::empty(branch);
    state
        .attach_inherited_layers(vec![inherited_nonzero_layer(
            parent,
            "target",
            50,
            "point-inherited-boundary-parent-loser",
        )])
        .expect("attach inherited");
    let local = point_row(branch, "target", 60);
    state
        .append_committed_row(local.clone())
        .expect("append child-local row");
    let view = state.capture_read_view().expect("read view");
    let target = physical_key(branch, b"target".to_vec());

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let actual = view.latest(&target).expect("point read");
    let perf = crate::observability::perf_trace::snapshot();

    assert_visible_row(actual.as_ref(), &local, BranchRowSource::Active);
    assert_eq!(perf.point_active_probes(), 1);
    assert_eq!(perf.point_frozen_probes(), 0);
    assert_eq!(perf.point_owned_l0_table_probes(), 0);
    assert_eq!(perf.point_owned_nonzero_level_searches(), 0);
    assert_eq!(perf.point_inherited_layer_searches(), 0);
    assert_eq!(perf.point_inherited_nonzero_level_searches(), 0);
    assert_eq!(perf.point_inherited_nonzero_table_probes(), 0);
    assert_eq!(perf.point_table_seeks(), 1);
    assert_eq!(perf.point_candidates_materialized(), 1);
    assert_eq!(perf.point_selected_active(), 1);
    assert_eq!(perf.point_selected_inherited(), 0);
    assert_eq!(perf.point_early_exit_active(), 0);
    assert_eq!(perf.point_early_exit_inherited(), 1);
    assert_eq!(perf.point_remaining_source_skips(), 2);
    assert_eq!(perf.point_inherited_key_rewrites(), 0);
    assert_eq!(perf.table_point_lookup_key_builds(), 1);
    assert_eq!(perf.table_point_lookup_key_reuses(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_point_read_enters_inherited_when_inherited_can_beat_local() {
    let branch = branch_id(190);
    let parent = branch_id(191);
    let mut state = BranchLocalState::empty(branch);
    state
        .attach_inherited_layers(vec![inherited_nonzero_layer(
            parent,
            "target",
            80,
            "point-inherited-boundary-parent-winner",
        )])
        .expect("attach inherited");
    state
        .append_committed_row(point_row(branch, "target", 60))
        .expect("append child-local row");
    let expected = point_row(branch, "target", 80);
    let view = state.capture_read_view().expect("read view");
    let target = physical_key(branch, b"target".to_vec());

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
    assert_eq!(perf.point_active_probes(), 1);
    assert_eq!(perf.point_inherited_layer_searches(), 1);
    assert_eq!(perf.point_inherited_nonzero_level_searches(), 1);
    assert_eq!(perf.point_inherited_nonzero_table_probes(), 1);
    assert_eq!(perf.point_table_seeks(), 2);
    assert_eq!(perf.point_candidates_materialized(), 2);
    assert_eq!(perf.point_candidate_row_clones(), 1);
    assert_eq!(perf.point_selected_active(), 0);
    assert_eq!(perf.point_selected_inherited(), 1);
    assert_eq!(perf.point_early_exit_active(), 0);
    assert_eq!(perf.point_early_exit_inherited(), 0);
    assert_eq!(perf.point_remaining_source_skips(), 0);
    assert_eq!(perf.point_inherited_key_rewrites(), 1);
    assert_eq!(perf.table_point_lookup_key_builds(), 2);
    assert_eq!(perf.table_point_lookup_key_reuses(), 0);
}

#[test]
fn branch_point_read_active_hit_shadows_older_sources() {
    let branch = branch_id(19);
    let parent = branch_id(20);
    let mut state = BranchLocalState::empty(branch);
    state
        .attach_inherited_layers(vec![inherited_nonzero_layer(
            parent,
            "target",
            10,
            "point-precedence-parent-active",
        )])
        .expect("attach inherited");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            point_table(
                branch,
                BranchLevel::new(1),
                "point-precedence-owned-active",
                "target",
                20,
            ),
        )
        .expect("install nonzero");
    state
        .install_l0_table(point_table(
            branch,
            BranchLevel::ZERO,
            "point-precedence-l0-active",
            "target",
            30,
        ))
        .expect("install L0");
    let frozen = point_row(branch, "target", 40);
    state.append_committed_row(frozen).expect("append frozen");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    let active = point_row(branch, "target", 50);
    state
        .append_committed_row(active.clone())
        .expect("append active");
    let view = state.capture_read_view().expect("read view");
    let target = physical_key(branch, b"target".to_vec());

    let actual = view.latest(&target).expect("point read");

    assert_visible_row(actual.as_ref(), &active, BranchRowSource::Active);
}

#[test]
fn branch_point_read_frozen_hit_shadows_owned_and_inherited_sources() {
    let branch = branch_id(21);
    let parent = branch_id(22);
    let mut state = BranchLocalState::empty(branch);
    state
        .attach_inherited_layers(vec![inherited_nonzero_layer(
            parent,
            "target",
            10,
            "point-precedence-parent-frozen",
        )])
        .expect("attach inherited");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            point_table(
                branch,
                BranchLevel::new(1),
                "point-precedence-owned-frozen",
                "target",
                20,
            ),
        )
        .expect("install nonzero");
    state
        .install_l0_table(point_table(
            branch,
            BranchLevel::ZERO,
            "point-precedence-l0-frozen",
            "target",
            30,
        ))
        .expect("install L0");
    let frozen = point_row(branch, "target", 40);
    state
        .append_committed_row(frozen.clone())
        .expect("append frozen");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    let view = state.capture_read_view().expect("read view");
    let target = physical_key(branch, b"target".to_vec());

    let actual = view.latest(&target).expect("point read");

    assert_visible_row(
        actual.as_ref(),
        &frozen,
        BranchRowSource::Frozen { index: 0 },
    );
}

#[test]
fn branch_point_read_l0_hit_shadows_nonzero_and_inherited_sources() {
    let branch = branch_id(23);
    let parent = branch_id(24);
    let mut state = BranchLocalState::empty(branch);
    state
        .attach_inherited_layers(vec![inherited_nonzero_layer(
            parent,
            "target",
            10,
            "point-precedence-parent-l0",
        )])
        .expect("attach inherited");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            point_table(
                branch,
                BranchLevel::new(1),
                "point-precedence-owned-l0",
                "target",
                20,
            ),
        )
        .expect("install nonzero");
    let l0 = point_row(branch, "target", 30);
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "point-precedence-l0",
            vec![l0.clone()],
        ))
        .expect("install L0");
    let view = state.capture_read_view().expect("read view");
    let target = physical_key(branch, b"target".to_vec());

    let actual = view.latest(&target).expect("point read");

    assert_visible_row(
        actual.as_ref(),
        &l0,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
}

#[test]
fn branch_point_read_inherited_nonzero_hit_rewrites_key_and_applies_fork_bound() {
    let branch = branch_id(32);
    let parent = branch_id(33);
    let visible_at_fork = point_row(parent, "inherited-hit", 5);
    let hidden_after_fork = point_row(parent, "inherited-hit", 7);
    let inherited = branch_inherited_layer_unchecked_for_fork_gate_tests(
        parent,
        CommitVersion::new(6),
        InheritedLayerStatus::Active,
        vec![
            Vec::new(),
            vec![branch_owned_table(
                parent,
                BranchLevel::new(1),
                "point-inherited-fork-bound",
                vec![visible_at_fork.clone(), hidden_after_fork],
            )],
        ],
    );
    let mut state = BranchLocalState::empty(branch);
    state
        .attach_inherited_layers(vec![inherited])
        .expect("attach inherited");
    let view = state.capture_read_view().expect("read view");
    let target = physical_key(branch, b"inherited-hit".to_vec());
    let expected = rewrite_row_branch(&visible_at_fork, parent, branch).expect("rewrite");

    assert_visible_row(
        view.latest(&target).expect("latest").as_ref(),
        &expected,
        BranchRowSource::Inherited {
            source_branch_id: parent,
            layer_index: 0,
        },
    );
    assert_visible_row(
        view.at_version(&target, CommitVersion::new(6))
            .expect("at fork")
            .as_ref(),
        &expected,
        BranchRowSource::Inherited {
            source_branch_id: parent,
            layer_index: 0,
        },
    );
    assert!(view
        .at_version(&target, CommitVersion::new(4))
        .expect("before inherited row")
        .is_none());
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

#[test]
fn branch_point_read_owned_nonzero_table_uses_visible_version_chain() {
    let branch = branch_id(25);
    let mut state = BranchLocalState::empty(branch);
    let old = point_row(branch, "versioned", 10);
    let middle = point_row(branch, "versioned", 20);
    let newest = point_row(branch, "versioned", 30);
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "point-version-chain",
                vec![old.clone(), middle.clone(), newest.clone()],
            ),
        )
        .expect("install nonzero");
    let view = state.capture_read_view().expect("read view");
    let target = physical_key(branch, b"versioned".to_vec());

    let latest = view.latest(&target).expect("latest");
    assert_visible_row(
        latest.as_ref(),
        &newest,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );

    let bounded = view
        .at_version(&target, CommitVersion::new(25))
        .expect("bounded");
    assert_visible_row(
        bounded.as_ref(),
        &middle,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );

    assert!(view
        .at_version(&target, CommitVersion::new(5))
        .expect("below chain")
        .is_none());
}

#[test]
fn branch_point_read_timestamp_bound_selects_nonzero_row_by_timestamp_contract() {
    let branch = branch_id(26);
    let mut state = BranchLocalState::empty(branch);
    let older = storage_row_with(
        branch,
        b"as-of".to_vec(),
        10,
        80,
        Timestamp::EPOCH,
        b"older".to_vec(),
    );
    let newest_visible_at_130 = storage_row_with(
        branch,
        b"as-of".to_vec(),
        30,
        120,
        Timestamp::EPOCH,
        b"newer".to_vec(),
    );
    let future_timestamp = storage_row_with(
        branch,
        b"as-of".to_vec(),
        20,
        140,
        Timestamp::EPOCH,
        b"future-time".to_vec(),
    );
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "point-timestamp-chain",
                vec![
                    older.clone(),
                    newest_visible_at_130.clone(),
                    future_timestamp,
                ],
            ),
        )
        .expect("install nonzero");
    let view = state
        .capture_read_view()
        .expect("read view")
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    let target = physical_key(branch, b"as-of".to_vec());

    let before_all = view
        .read_point(
            &target,
            BranchReadBound::at_timestamp(Timestamp::from_micros(79)),
        )
        .expect("before all");
    assert!(before_all.is_none());

    assert_visible_row(
        view.read_point(
            &target,
            BranchReadBound::at_timestamp(Timestamp::from_micros(100)),
        )
        .expect("at older")
        .as_ref(),
        &older,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );
    assert_visible_row(
        view.read_point(
            &target,
            BranchReadBound::at_timestamp(Timestamp::from_micros(130)),
        )
        .expect("at newer")
        .as_ref(),
        &newest_visible_at_130,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );
}

#[test]
fn branch_point_read_ttl_expiry_filters_selected_nonzero_row_without_fallthrough() {
    let branch = branch_id(27);
    let mut state = BranchLocalState::empty(branch);
    let older = storage_row_with(
        branch,
        b"ttl".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"older".to_vec(),
    );
    let expiring = storage_row_with(
        branch,
        b"ttl".to_vec(),
        2,
        20,
        Timestamp::from_micros(30),
        b"expiring".to_vec(),
    );
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "point-ttl-chain",
                vec![older, expiring.clone()],
            ),
        )
        .expect("install nonzero");
    let view = state
        .capture_read_view()
        .expect("read view")
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    let target = physical_key(branch, b"ttl".to_vec());

    assert_visible_row(
        view.read_point(
            &target,
            BranchReadBound::at_timestamp(Timestamp::from_micros(29)),
        )
        .expect("before expiry")
        .as_ref(),
        &expiring,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );

    let expired = view
        .read_point(
            &target,
            BranchReadBound::at_timestamp(Timestamp::from_micros(30)),
        )
        .expect("at expiry");
    assert!(expired.is_none());

    assert_visible_row(
        view.latest(&target)
            .expect("latest ignores wall clock")
            .as_ref(),
        &expiring,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );
}

#[test]
fn branch_point_read_child_tombstone_hides_inherited_nonzero_row() {
    let branch = branch_id(28);
    let parent = branch_id(29);
    let inherited_row = point_row(parent, "shadowed", 3);
    let inherited = branch_inherited_layer(
        parent,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![
            Vec::new(),
            vec![branch_owned_table(
                parent,
                BranchLevel::new(1),
                "point-child-shadow-parent",
                vec![inherited_row.clone()],
            )],
        ],
    );
    let mut state = BranchLocalState::empty(branch);
    state
        .attach_inherited_layers(vec![inherited])
        .expect("attach inherited");
    state
        .append_committed_row(tombstone_row(branch, b"shadowed".to_vec(), 6, 60))
        .expect("append child tombstone");
    let view = state.capture_read_view().expect("read view");
    let target = physical_key(branch, b"shadowed".to_vec());
    let expected = rewrite_row_branch(&inherited_row, parent, branch).expect("rewrite");

    let latest = view.latest(&target).expect("latest");
    assert!(latest.is_none());

    assert_visible_row(
        view.at_version(&target, CommitVersion::new(4))
            .expect("before tombstone")
            .as_ref(),
        &expected,
        BranchRowSource::Inherited {
            source_branch_id: parent,
            layer_index: 0,
        },
    );
}

#[test]
fn branch_point_read_source_tombstone_is_hidden_in_latest_and_visible_in_history() {
    let branch = branch_id(30);
    let mut state = BranchLocalState::empty(branch);
    let put = point_row(branch, "deleted", 1);
    let tombstone = tombstone_row(branch, b"deleted".to_vec(), 2, 20);
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "point-source-tombstone",
                vec![put.clone(), tombstone],
            ),
        )
        .expect("install nonzero");
    let view = state.capture_read_view().expect("read view");
    let target = physical_key(branch, b"deleted".to_vec());

    let latest = view.latest(&target).expect("latest");
    assert!(latest.is_none());

    let history = view
        .history(&target, BranchHistoryOptions::all())
        .expect("history");
    assert_eq!(history_versions(&history), vec![2, 1]);
    assert!(history[0].row().is_tombstone());
    assert_eq!(
        history_versions(
            &view
                .history(
                    &target,
                    BranchHistoryOptions::all().include_tombstones(false),
                )
                .expect("history without tombstones")
        ),
        vec![1],
    );
}

#[test]
fn branch_point_read_selected_nonzero_range_is_not_authoritative_for_absent_key() {
    let branch = branch_id(31);
    let mut state = BranchLocalState::empty(branch);
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "point-inside-range-miss",
                vec![
                    point_row(branch, "gap-010", 10),
                    point_row(branch, "gap-012", 12),
                ],
            ),
        )
        .expect("install nonzero");
    let view = state.capture_read_view().expect("read view");
    let target = physical_key(branch, b"gap-011".to_vec());

    let actual = view.latest(&target).expect("point read");

    assert!(actual.is_none());
}

#[test]
fn branch_point_read_outside_owned_nonzero_level_edges_returns_none() {
    let branch = branch_id(34);
    let mut state = BranchLocalState::empty(branch);
    for (index, key) in ["edge-010", "edge-020", "edge-030"].into_iter().enumerate() {
        state
            .install_owned_table_at_level(
                BranchLevel::new(1),
                point_table(
                    branch,
                    BranchLevel::new(1),
                    &format!("point-edge-miss-{index}"),
                    key,
                    10 + u64::try_from(index).expect("index fits in u64"),
                ),
            )
            .expect("install nonzero");
    }
    let view = state.capture_read_view().expect("read view");

    assert!(view
        .latest(&physical_key(branch, b"edge-000".to_vec()))
        .expect("below first table")
        .is_none());
    assert!(view
        .latest(&physical_key(branch, b"edge-999".to_vec()))
        .expect("above last table")
        .is_none());
}

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

        let actual = view.latest(&target).expect("point read");

        assert_visible_row(
            actual.as_ref(),
            &expected,
            BranchRowSource::OwnedTable {
                level: BranchLevel::new(1),
                table_index: 10,
            },
        );
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
    assert_eq!(perf.point_candidates_materialized(), 0);
    assert_eq!(perf.point_candidate_row_clones(), 0);
    assert_eq!(perf.point_selected_active(), 0);
    assert_eq!(perf.point_selected_frozen(), 0);
    assert_eq!(perf.point_selected_owned_l0(), 0);
    assert_eq!(perf.point_selected_owned_nonzero(), 0);
    assert_eq!(perf.point_selected_inherited(), 0);
    assert_eq!(perf.table_point_lookup_key_builds(), 1);
    assert_eq!(perf.table_point_lookup_key_reuses(), 0);
    assert_eq!(perf.table_eager_filter_probes(), 0);
    assert_eq!(perf.point_remaining_source_skips(), 0);
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
    assert_eq!(perf.point_candidates_materialized(), 2);
    assert_eq!(perf.point_candidate_row_clones(), 1);
    assert_eq!(perf.point_selected_owned_nonzero(), 1);
    assert_eq!(perf.table_point_lookup_key_builds(), 1);
    assert_eq!(perf.table_point_lookup_key_reuses(), 2);
    assert_eq!(perf.table_eager_filter_probes(), 2);
    assert_eq!(perf.table_eager_filter_positive_probes(), 2);
    assert_eq!(perf.table_eager_filter_unavailable_probes(), 0);
    assert_eq!(perf.point_remaining_source_skips(), 0);
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
    assert_eq!(perf.point_candidates_materialized(), 1);
    assert_eq!(perf.point_candidate_row_clones(), 1);
    assert_eq!(perf.point_selected_inherited(), 1);
    assert_eq!(perf.point_inherited_key_rewrites(), 1);
    assert_eq!(perf.table_point_lookup_key_builds(), 2);
    assert_eq!(perf.table_point_lookup_key_reuses(), 0);
    assert_eq!(perf.table_eager_filter_probes(), 1);
    assert_eq!(perf.table_eager_filter_positive_probes(), 1);
    assert_eq!(perf.table_eager_filter_unavailable_probes(), 0);
    assert_eq!(perf.point_remaining_source_skips(), 0);
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
    assert_eq!(perf.point_candidates_materialized(), 25);
    assert_eq!(perf.point_candidate_row_clones(), 1);
    assert_eq!(perf.point_selected_owned_l0(), 1);
    assert_eq!(perf.table_point_lookup_key_builds(), 1);
    assert_eq!(perf.table_point_lookup_key_reuses(), 25);
    assert_eq!(perf.table_eager_filter_probes(), 25);
    assert_eq!(perf.table_eager_filter_positive_probes(), 25);
    assert_eq!(perf.table_eager_filter_unavailable_probes(), 0);
    assert_eq!(perf.point_remaining_source_skips(), 0);
}
