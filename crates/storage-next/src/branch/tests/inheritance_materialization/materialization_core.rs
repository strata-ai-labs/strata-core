#[test]
#[allow(clippy::too_many_lines)]
fn branch_materialization_rewrites_retained_rows_without_cleanup() {
    let source = branch_id(91);
    let child = branch_id(92);
    let key = b"materialized-history".to_vec();
    let old_source = storage_row_with(
        source,
        key.clone(),
        1,
        10,
        Timestamp::EPOCH,
        b"old".to_vec(),
    );
    let mid_source = storage_row_with(
        source,
        key.clone(),
        3,
        30,
        Timestamp::EPOCH,
        b"mid".to_vec(),
    );
    let layer = branch_inherited_layer_unchecked_for_fork_gate_tests(
        source,
        CommitVersion::new(3),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materialize-source-history",
            vec![old_source.clone(), mid_source.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let child_newer = storage_row_with(
        child,
        key.clone(),
        5,
        50,
        Timestamp::EPOCH,
        b"child-newer".to_vec(),
    );
    child_state
        .append_committed_row(child_newer.clone())
        .expect("append child newer row");

    let child_key = physical_key(child, key);
    let before = child_state.capture_read_view().expect("before view");
    assert_visible_row(
        before.latest(&child_key).expect("before latest").as_ref(),
        &child_newer,
        BranchRowSource::Active,
    );
    assert_visible_row(
        before
            .at_version(&child_key, CommitVersion::new(2))
            .expect("before getv")
            .as_ref(),
        &rewrite_row_branch(&old_source, source, child).expect("old rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    assert_visible_row(
        before
            .read_point(
                &child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(35)),
            )
            .expect("before as-of")
            .as_ref(),
        &rewrite_row_branch(&mid_source, source, child).expect("mid rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );

    let outcome: BranchMaterializationOutcome = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-history")
                .expect("materialization request"),
        )
        .expect("materialize inherited layer");
    assert_eq!(outcome.child_branch_id(), child);
    assert_eq!(outcome.source_branch_id(), source);
    assert_eq!(outcome.fork_version(), CommitVersion::new(3));
    assert_eq!(outcome.layer_index(), 0);
    assert_eq!(outcome.rows_materialized(), 2);
    assert_eq!(outcome.tables_created(), 1);
    assert_eq!(outcome.skipped_post_fork_rows(), 0);
    assert_eq!(outcome.skipped_exact_duplicate_rows(), 0);
    assert_eq!(outcome.inherited_layers_remaining(), 0);
    assert_eq!(outcome.replacement_owned_table_count(), 1);
    assert_eq!(
        outcome.recovery(),
        BranchMaterializationRecovery::ReplacementVisibleLayerRemoved
    );
    assert_eq!(child_state.inherited_layer_count(), 0);
    assert_eq!(child_state.owned_table_count(), 1);

    let after = child_state.capture_read_view().expect("after view");
    assert_visible_row(
        after.latest(&child_key).expect("after latest").as_ref(),
        &child_newer,
        BranchRowSource::Active,
    );
    assert_visible_row(
        after
            .at_version(&child_key, CommitVersion::new(2))
            .expect("after getv")
            .as_ref(),
        &rewrite_row_branch(&old_source, source, child).expect("old rewrite"),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_visible_row(
        after
            .read_point(
                &child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(35)),
            )
            .expect("after as-of")
            .as_ref(),
        &rewrite_row_branch(&mid_source, source, child).expect("mid rewrite"),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_eq!(
        history_versions(
            &after
                .history(
                    &child_key,
                    BranchHistoryOptions::all().include_tombstones(true)
                )
                .expect("after history"),
        ),
        vec![5, 3, 1],
        "materialization must preserve retained row history",
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn branch_materialization_skips_post_fork_rows_and_exact_duplicates_only() {
    let source = branch_id(93);
    let child = branch_id(94);
    let post_fork = storage_row_with(
        source,
        b"post-fork".to_vec(),
        8,
        80,
        Timestamp::EPOCH,
        b"post".to_vec(),
    );
    let exact_duplicate = storage_row_with(
        source,
        b"exact".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"source-exact".to_vec(),
    );
    let retained_history = storage_row_with(
        source,
        b"history".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"source-history".to_vec(),
    );
    let layer = branch_inherited_layer_unchecked_for_fork_gate_tests(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materialize-filter-source",
            vec![post_fork, exact_duplicate.clone(), retained_history.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let child_exact_duplicate = storage_row_with(
        child,
        b"exact".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"source-exact".to_vec(),
    );
    child_state
        .append_committed_row(child_exact_duplicate.clone())
        .expect("append exact duplicate");
    let child_newer_history = storage_row_with(
        child,
        b"history".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        b"child-newer-history".to_vec(),
    );
    child_state
        .append_committed_row(child_newer_history.clone())
        .expect("append newer history");

    let outcome = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-filter")
                .expect("materialization request"),
        )
        .expect("materialize inherited layer");
    assert_eq!(outcome.rows_materialized(), 1);
    assert_eq!(outcome.tables_created(), 1);
    assert_eq!(outcome.skipped_post_fork_rows(), 1);
    assert_eq!(outcome.skipped_exact_duplicate_rows(), 1);

    let view = child_state.capture_read_view().expect("after view");
    assert!(
        view.latest(&physical_key(child, b"post-fork".to_vec()))
            .expect("post-fork latest")
            .is_none(),
        "post-fork inherited rows must not be materialized",
    );
    assert_visible_row(
        view.latest(&physical_key(child, b"exact".to_vec()))
            .expect("exact latest")
            .as_ref(),
        &child_exact_duplicate,
        BranchRowSource::Active,
    );
    assert_eq!(
        history_versions(
            &view
                .history(
                    &physical_key(child, b"exact".to_vec()),
                    BranchHistoryOptions::all().include_tombstones(true),
                )
                .expect("exact history"),
        ),
        vec![4],
        "exact duplicate inherited row should be suppressed",
    );
    let history_key = physical_key(child, b"history".to_vec());
    assert_visible_row(
        view.latest(&history_key).expect("history latest").as_ref(),
        &child_newer_history,
        BranchRowSource::Active,
    );
    assert_visible_row(
        view.at_version(&history_key, CommitVersion::new(3))
            .expect("history getv")
            .as_ref(),
        &rewrite_row_branch(&retained_history, source, child).expect("history rewrite"),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn branch_materialization_rejects_same_internal_key_when_row_facts_differ() {
    let source = branch_id(97);
    let child = branch_id(98);
    let key = b"materialized-same-version-timestamp".to_vec();
    let inherited_visible_at_timestamp = storage_row_with(
        source,
        key.clone(),
        4,
        30,
        Timestamp::EPOCH,
        b"inherited-visible-at-40".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materialize-same-version-source",
            vec![inherited_visible_at_timestamp.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let child_same_internal_key_later_timestamp = storage_row_with(
        child,
        key.clone(),
        4,
        50,
        Timestamp::EPOCH,
        b"child-hidden-at-40".to_vec(),
    );
    child_state
        .append_committed_row(child_same_internal_key_later_timestamp.clone())
        .expect("append child same internal key");

    let child_key = physical_key(child, key);
    let rewritten =
        rewrite_row_branch(&inherited_visible_at_timestamp, source, child).expect("rewrite");
    let before = child_state.capture_read_view().expect("before view");
    assert_visible_row(
        before
            .read_point(
                &child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            )
            .expect("before as-of")
            .as_ref(),
        &rewritten,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    let before_history = before
        .history(
            &child_key,
            BranchHistoryOptions::all().include_tombstones(true),
        )
        .expect("before history");
    assert_eq!(history_versions(&before_history), vec![4, 4]);
    assert_eq!(
        before_history[0].row(),
        &child_same_internal_key_later_timestamp
    );
    assert_eq!(before_history[1].row(), &rewritten);

    let before_state = child_state.clone();
    let error = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-same-version")
                .expect("request"),
        )
        .expect_err("same internal key with different row facts is rejected");
    assert!(matches!(
        error,
        BranchRuntimeError::InvalidInheritedLayer { .. }
    ));
    assert_eq!(child_state, before_state);
}

#[test]
fn branch_materialization_rejects_child_owned_immutable_internal_key_collision() {
    let source = branch_id(63);
    let child = branch_id(64);
    let key = b"materialized-owned-immutable-collision".to_vec();
    let inherited_visible_at_timestamp = storage_row_with(
        source,
        key.clone(),
        4,
        30,
        Timestamp::EPOCH,
        b"inherited-visible-at-40".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materialize-owned-immutable-collision-source",
            vec![inherited_visible_at_timestamp.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let child_same_internal_key_later_timestamp = storage_row_with(
        child,
        key.clone(),
        4,
        50,
        Timestamp::EPOCH,
        b"child-hidden-at-40".to_vec(),
    );
    child_state
        .install_l0_table(branch_owned_table(
            child,
            BranchLevel::ZERO,
            "materialize-owned-immutable-collision-child",
            vec![child_same_internal_key_later_timestamp.clone()],
        ))
        .expect("install child-owned immutable collision");

    let child_key = physical_key(child, key);
    let rewritten =
        rewrite_row_branch(&inherited_visible_at_timestamp, source, child).expect("rewrite");
    let before = child_state.capture_read_view().expect("before view");
    assert_visible_row(
        before
            .read_point(
                &child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            )
            .expect("before as-of")
            .as_ref(),
        &rewritten,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    let before_state = child_state.clone();

    let error = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-owned-immutable-collision")
                .expect("request"),
        )
        .expect_err("same internal key in child-owned immutable table is rejected");

    assert_eq!(
        error,
        BranchRuntimeError::InvalidInheritedLayer {
            reason: "materialized inherited rows must not collide with higher-precedence rows",
        }
    );
    assert_eq!(child_state, before_state);
}

#[test]
#[allow(clippy::too_many_lines)]
fn branch_materialization_preserves_scans_tombstones_ttl_and_pinned_views() {
    let source = branch_id(99);
    let child = branch_id(100);
    let visible = storage_row_with(
        source,
        b"materialized-scan-a".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"visible".to_vec(),
    );
    let expired = storage_row_with(
        source,
        b"materialized-scan-expired".to_vec(),
        2,
        20,
        Timestamp::from_micros(25),
        b"expired".to_vec(),
    );
    let deleted_put = storage_row_with(
        source,
        b"materialized-scan-deleted".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"deleted-put".to_vec(),
    );
    let deleting_tombstone = tombstone_row(source, b"materialized-scan-deleted".to_vec(), 3, 30);
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(3),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materialize-scan-source",
            vec![
                visible.clone(),
                expired.clone(),
                deleted_put.clone(),
                deleting_tombstone.clone(),
            ],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let pinned = child_state.capture_read_view().expect("pinned before view");
    let visible_key = physical_key(child, b"materialized-scan-a".to_vec());
    let expired_key = physical_key(child, b"materialized-scan-expired".to_vec());
    let deleted_key = physical_key(child, b"materialized-scan-deleted".to_vec());
    let visible_rewritten = rewrite_row_branch(&visible, source, child).expect("visible rewrite");
    let expired_rewritten = rewrite_row_branch(&expired, source, child).expect("expired rewrite");

    let prefix = BranchScanBounds::prefix(&physical_key(child, b"materialized-scan-".to_vec()));
    let range = BranchScanBounds::closed(
        &physical_key(child, b"materialized-scan-a".to_vec()),
        &physical_key(child, b"materialized-scan-expired".to_vec()),
    )
    .expect("closed materialization range");
    assert_eq!(
        scan_user_keys(
            &pinned
                .scan_prefix(
                    &prefix,
                    BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
                )
                .expect("before timestamp prefix scan"),
        ),
        vec![b"materialized-scan-a".to_vec()]
    );
    assert_eq!(
        scan_user_keys(
            &pinned
                .scan_range(
                    &range,
                    BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
                )
                .expect("before timestamp range scan"),
        ),
        vec![b"materialized-scan-a".to_vec()]
    );

    let outcome = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-scan").expect("request"),
        )
        .expect("materialize inherited layer");
    assert_eq!(outcome.rows_materialized(), 4);
    assert_eq!(outcome.tables_created(), 1);
    assert_eq!(child_state.inherited_layer_count(), 0);

    assert_visible_row(
        pinned
            .read_point(
                &visible_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            )
            .expect("pinned visible")
            .as_ref(),
        &visible_rewritten,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );

    let after = child_state.capture_read_view().expect("after view");
    assert_visible_row(
        after
            .read_point(
                &visible_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            )
            .expect("after visible")
            .as_ref(),
        &visible_rewritten,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_visible_row(
        after
            .read_point(
                &expired_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(24)),
            )
            .expect("expired before expiry")
            .as_ref(),
        &expired_rewritten,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert!(
        after
            .read_point(
                &expired_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
            )
            .expect("expired at expiry")
            .is_none(),
        "materialization must preserve TTL visibility without cleanup",
    );
    assert!(
        after
            .read_point(
                &deleted_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            )
            .expect("deleted at timestamp")
            .is_none(),
        "materialized tombstone must keep suppressing older puts",
    );
    assert_eq!(
        history_versions(
            &after
                .history(
                    &deleted_key,
                    BranchHistoryOptions::all().include_tombstones(true),
                )
                .expect("deleted history"),
        ),
        vec![3, 1],
    );
    assert_eq!(
        scan_user_keys(
            &after
                .scan_prefix(
                    &prefix,
                    BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
                )
                .expect("after timestamp prefix scan"),
        ),
        vec![b"materialized-scan-a".to_vec()]
    );
    assert_eq!(
        scan_user_keys(
            &after
                .scan_range(
                    &range,
                    BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
                )
                .expect("after timestamp range scan"),
        ),
        vec![b"materialized-scan-a".to_vec()]
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn branch_materialization_preserves_reads_across_inherited_lsm_sources() {
    let source = branch_id(119);
    let child = branch_id(120);
    let l0_new = storage_row_with(
        source,
        b"materialized-level-a".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"l0-new".to_vec(),
    );
    let l0_old = storage_row_with(
        source,
        b"materialized-level-a".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"l0-old".to_vec(),
    );
    let l1_row = storage_row_with(
        source,
        b"materialized-level-b".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"l1".to_vec(),
    );
    let l2_row = storage_row_with(
        source,
        b"materialized-level-c".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"l2".to_vec(),
    );
    let deleted_put = storage_row_with(
        source,
        b"materialized-level-deleted".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"deleted-put".to_vec(),
    );
    let deleted_tombstone = tombstone_row(source, b"materialized-level-deleted".to_vec(), 4, 40);
    let exact_duplicate = storage_row_with(
        source,
        b"materialized-level-shadow".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"shadowed".to_vec(),
    );
    let post_fork = storage_row_with(
        source,
        b"materialized-level-post-fork".to_vec(),
        8,
        80,
        Timestamp::EPOCH,
        b"post-fork".to_vec(),
    );
    let layer = branch_inherited_layer_unchecked_for_fork_gate_tests(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![
            vec![
                branch_owned_table(
                    source,
                    BranchLevel::ZERO,
                    "materialize-level-l0-new",
                    vec![l0_new.clone(), post_fork],
                ),
                branch_owned_table(
                    source,
                    BranchLevel::ZERO,
                    "materialize-level-l0-old",
                    vec![l0_old.clone()],
                ),
            ],
            vec![branch_owned_table(
                source,
                BranchLevel::new(1),
                "materialize-level-l1",
                vec![
                    l1_row.clone(),
                    deleted_put.clone(),
                    deleted_tombstone.clone(),
                    exact_duplicate.clone(),
                ],
            )],
            vec![branch_owned_table(
                source,
                BranchLevel::new(2),
                "materialize-level-l2",
                vec![l2_row.clone()],
            )],
        ],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited levels");
    let child_l1_newer = storage_row_with(
        child,
        b"materialized-level-b".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        b"child-newer".to_vec(),
    );
    child_state
        .append_committed_row(child_l1_newer.clone())
        .expect("append child newer row");
    child_state
        .append_committed_row(
            rewrite_row_branch(&exact_duplicate, source, child).expect("rewrite duplicate"),
        )
        .expect("append exact duplicate");

    let key_a = physical_key(child, b"materialized-level-a".to_vec());
    let key_b = physical_key(child, b"materialized-level-b".to_vec());
    let key_c = physical_key(child, b"materialized-level-c".to_vec());
    let key_deleted = physical_key(child, b"materialized-level-deleted".to_vec());
    let key_post_fork = physical_key(child, b"materialized-level-post-fork".to_vec());
    let prefix = BranchScanBounds::prefix(&physical_key(child, b"materialized-level-".to_vec()));
    let range = BranchScanBounds::closed(
        &physical_key(child, b"materialized-level-a".to_vec()),
        &physical_key(child, b"materialized-level-c".to_vec()),
    )
    .expect("closed source-level range");

    let before = child_state.capture_read_view().expect("before view");
    let before_latest_a = visible_storage_row(before.latest(&key_a).expect("before a"));
    let before_latest_b = visible_storage_row(before.latest(&key_b).expect("before b"));
    let before_l1_history = before
        .history(
            &key_b,
            BranchHistoryOptions::all().include_tombstones(true),
        )
        .expect("before l1 history");
    let before_l1_versions = history_versions(&before_l1_history);
    let before_latest_c = visible_storage_row(before.latest(&key_c).expect("before c"));
    let before_deleted_history = before
        .history(
            &key_deleted,
            BranchHistoryOptions::all().include_tombstones(true),
        )
        .expect("before deleted history");
    let before_deleted_versions = history_versions(&before_deleted_history);
    let before_prefix_rows = visible_rows(
        &before
            .scan_prefix(&prefix, BranchReadBound::latest())
            .expect("before prefix scan"),
    );
    let before_range_rows = visible_rows(
        &before
            .scan_range(&range, BranchReadBound::latest())
            .expect("before range scan"),
    );

    assert_visible_row(
        before.latest(&key_a).expect("before inherited a").as_ref(),
        &rewrite_row_branch(&l0_new, source, child).expect("rewrite l0"),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    assert_eq!(before_l1_versions, vec![7, 3]);
    assert_eq!(
        before_l1_history[1].source(),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    assert_eq!(before_deleted_versions, vec![4, 1]);
    assert_eq!(
        before_deleted_history[0].source(),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    assert!(
        before
            .latest(&key_deleted)
            .expect("before deleted latest")
            .is_none(),
        "source tombstone must hide older source put before materialization",
    );
    assert!(
        before
            .latest(&key_post_fork)
            .expect("before post-fork latest")
            .is_none(),
        "post-fork source row must be filtered before materialization",
    );

    let pinned = before;
    let outcome = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-level")
                .expect("request"),
        )
        .expect("materialize inherited levels");
    assert_eq!(outcome.rows_materialized(), 6);
    assert_eq!(outcome.tables_created(), 1);
    assert_eq!(outcome.skipped_post_fork_rows(), 1);
    assert_eq!(outcome.skipped_exact_duplicate_rows(), 1);
    assert_eq!(child_state.inherited_layer_count(), 0);

    assert_visible_row(
        pinned.latest(&key_c).expect("pinned c").as_ref(),
        &rewrite_row_branch(&l2_row, source, child).expect("rewrite l2 pinned"),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );

    let after = child_state.capture_read_view().expect("after view");
    assert_eq!(
        visible_storage_row(after.latest(&key_a).expect("after a")),
        before_latest_a,
    );
    assert_eq!(
        visible_storage_row(after.latest(&key_b).expect("after b")),
        before_latest_b,
    );
    assert_eq!(
        visible_storage_row(after.latest(&key_c).expect("after c")),
        before_latest_c,
    );
    let after_l1_history = after
        .history(
            &key_b,
            BranchHistoryOptions::all().include_tombstones(true),
        )
        .expect("after l1 history");
    assert_eq!(history_versions(&after_l1_history), before_l1_versions);
    assert_eq!(
        after_l1_history[1].row(),
        &rewrite_row_branch(&l1_row, source, child).expect("rewrite l1 after"),
    );
    assert_eq!(
        after_l1_history[1].source(),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    let after_deleted_history = after
        .history(
            &key_deleted,
            BranchHistoryOptions::all().include_tombstones(true),
        )
        .expect("after deleted history");
    assert_eq!(
        history_versions(&after_deleted_history),
        before_deleted_versions,
    );
    assert_eq!(
        after_deleted_history[0].row(),
        &rewrite_row_branch(&deleted_tombstone, source, child).expect("rewrite tombstone after"),
    );
    assert_eq!(
        after_deleted_history[0].source(),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_eq!(
        visible_rows(
            &after
                .scan_prefix(&prefix, BranchReadBound::latest())
                .expect("after prefix scan"),
        ),
        before_prefix_rows,
    );
    assert_eq!(
        visible_rows(
            &after
                .scan_range(&range, BranchReadBound::latest())
                .expect("after range scan"),
        ),
        before_range_rows,
    );
    assert_visible_row(
        after.latest(&key_a).expect("after owned a").as_ref(),
        &rewrite_row_branch(&l0_new, source, child).expect("rewrite l0 after"),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_visible_row(
        after.latest(&key_c).expect("after owned c").as_ref(),
        &rewrite_row_branch(&l2_row, source, child).expect("rewrite l2 after"),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
}

#[test]
fn branch_materialization_splits_large_outputs_and_validates_identity_prefixes() {
    let source = branch_id(101);
    let child = branch_id(102);
    let rows = (0_u64..4_097)
        .map(|index| {
            storage_row_with(
                source,
                format!("materialized-split-{index:04}").into_bytes(),
                1,
                1,
                Timestamp::EPOCH,
                index.to_le_bytes().to_vec(),
            )
        })
        .collect::<Vec<_>>();
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(1),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materialize-split-source",
            rows,
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let outcome = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-split").expect("request"),
        )
        .expect("materialize split layer");
    assert_eq!(outcome.rows_materialized(), 4_097);
    assert_eq!(outcome.tables_created(), 2);
    assert_eq!(child_state.owned_levels()[0].len(), 2);
    assert_eq!(
        child_state.owned_levels()[0][0]
            .descriptor()
            .identity()
            .as_str(),
        "materialized-split-layer-0-table-0",
    );
    assert_eq!(
        child_state.owned_levels()[0][1]
            .descriptor()
            .identity()
            .as_str(),
        "materialized-split-layer-0-table-1",
    );

    assert!(matches!(
        BranchMaterializationRequest::new(child, 0, "bad/path"),
        Err(BranchRuntimeError::InvalidConfig {
            field: "output_identity_prefix",
            ..
        }),
    ));
    assert!(matches!(
        BranchMaterializationRequest::new(child, 0, "bad\0prefix"),
        Err(BranchRuntimeError::InvalidConfig {
            field: "output_identity_prefix",
            ..
        }),
    ));
}

#[cfg(feature = "perf-trace")]
#[test]
fn branch_materialization_prepare_records_streaming_source_counters() {
    let source = branch_id(107);
    let child = branch_id(108);
    let retained_rows = (0_u64..4_097)
        .map(|index| {
            storage_row_with(
                source,
                format!("materialized-counter-{index:04}").into_bytes(),
                3,
                30,
                Timestamp::EPOCH,
                index.to_le_bytes().to_vec(),
            )
        })
        .collect::<Vec<_>>();
    let exact_duplicate = storage_row_with(
        source,
        b"materialized-counter-shadow".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"shadowed".to_vec(),
    );
    let post_fork = storage_row_with(
        source,
        b"materialized-counter-post-fork".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        b"post-fork".to_vec(),
    );
    let mut first_table_rows = retained_rows[..2_049].to_vec();
    first_table_rows.push(post_fork);
    let mut second_table_rows = retained_rows[2_049..].to_vec();
    second_table_rows.push(exact_duplicate.clone());
    let layer = branch_inherited_layer_unchecked_for_fork_gate_tests(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![
            vec![branch_owned_table(
                source,
                BranchLevel::ZERO,
                "materialize-counter-source-a",
                first_table_rows,
            )],
            vec![branch_owned_table(
                source,
                BranchLevel::new(1),
                "materialize-counter-source-b",
                second_table_rows,
            )],
        ],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    child_state
        .append_committed_row(
            rewrite_row_branch(&exact_duplicate, source, child).expect("rewrite duplicate"),
        )
        .expect("append exact duplicate");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let prepared = child_state
        .prepare_materialization_output(
            &BranchMaterializationRequest::new(child, 0, "materialized-counter")
                .expect("request"),
        )
        .expect("prepare materialization")
        .expect("prepared output");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(prepared.artifacts().len(), 2);
    assert_eq!(perf.branch_materialization_source_opens(), 2);
    assert_eq!(perf.branch_materialization_rows_rewritten(), 4_098);
    assert_eq!(perf.branch_materialization_rows_skipped_by_fork(), 1);
    assert_eq!(perf.branch_materialization_rows_skipped_by_shadowing(), 1);
    assert_eq!(perf.branch_materialization_output_tables(), 2);
    assert_eq!(perf.branch_materialization_peak_buffered_rows(), 4_096);
}

#[test]
fn branch_materialization_rejects_prepared_output_with_changed_artifact_rows() {
    let source = branch_id(109);
    let child = branch_id(110);
    let source_row = storage_row_with(
        source,
        b"materialized-stale-artifact".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"prepared".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(3),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materialize-stale-artifact-source",
            vec![source_row.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let request =
        BranchMaterializationRequest::new(child, 0, "materialized-stale").expect("request");
    let prepared = child_state
        .prepare_materialization_output(&request)
        .expect("prepare materialization")
        .expect("prepared output");
    let wrong_row = storage_row_with(
        child,
        b"materialized-stale-artifact".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"changed".to_vec(),
    );
    let wrong_table = branch_owned_table(
        child,
        BranchLevel::ZERO,
        "materialized-stale-layer-0-table-0",
        vec![wrong_row],
    );

    let error = child_state
        .install_materialization_prepared_output(&request, &prepared, vec![wrong_table])
        .expect_err("changed prepared artifact rejected");
    assert_eq!(
        error,
        BranchRuntimeError::InvalidInheritedLayer {
            reason: "materialization replacement tables must match prepared output",
        },
    );
    assert_eq!(child_state.inherited_layer_count(), 1);
    assert_eq!(child_state.owned_table_count(), 0);
}

#[test]
fn branch_materialization_rejects_prepared_output_with_wrong_replacement_source() {
    let source = branch_id(121);
    let child = branch_id(122);
    let wrong_source = branch_id(123);
    let source_row = storage_row_with(
        source,
        b"materialized-wrong-source-replacement".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"prepared".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(3),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materialize-wrong-source-replacement-source",
            vec![source_row],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let request =
        BranchMaterializationRequest::new(child, 0, "materialized-wrong-source").expect("request");
    let prepared = child_state
        .prepare_materialization_output(&request)
        .expect("prepare materialization")
        .expect("prepared output");
    let artifact = prepared.artifacts()[0].clone();
    let identity = artifact.facts().identity().clone();
    let reader = ImmutableTableReader::open_bytes(
        identity.clone(),
        artifact.bytes().to_vec(),
        TableReaderConfig::default(),
    )
    .expect("replacement reader");
    let descriptor = BranchTableDescriptor::new(identity, reader.facts().clone(), BranchLevel::ZERO)
        .expect("replacement descriptor");
    let wrong_replacement = BranchOwnedTable::new_materialization_replacement(
        child,
        descriptor,
        reader,
        BranchMaterializationSource::new(wrong_source, CommitVersion::new(3)),
    )
    .expect("replacement with wrong source");

    let error = child_state
        .install_materialization_prepared_output(&request, &prepared, vec![wrong_replacement])
        .expect_err("wrong replacement source rejected");
    assert_eq!(
        error,
        BranchRuntimeError::InvalidInheritedLayer {
            reason: "materialization replacement tables must match prepared output",
        },
    );
    assert_eq!(child_state.inherited_layer_count(), 1);
    assert_eq!(child_state.owned_table_count(), 0);
}

#[test]
fn branch_materialization_rejects_output_identity_collision_without_mutation() {
    let source = branch_id(101);
    let other_source = branch_id(102);
    let child = branch_id(103);
    let materialized_layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materialize-collision-source",
            vec![storage_row_with(
                source,
                b"materialize-collision-a".to_vec(),
                5,
                50,
                Timestamp::EPOCH,
                b"source".to_vec(),
            )],
        )]],
    );
    let colliding_layer = branch_inherited_layer(
        other_source,
        CommitVersion::new(4),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            other_source,
            BranchLevel::ZERO,
            "materialize-collision-layer-0-table-0",
            vec![storage_row_with(
                other_source,
                b"materialize-collision-b".to_vec(),
                4,
                40,
                Timestamp::EPOCH,
                b"other".to_vec(),
            )],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![materialized_layer, colliding_layer])
        .expect("attach collision layers");
    let before = child_state.clone();

    let error = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialize-collision")
                .expect("collision request"),
        )
        .expect_err("materialization identity collision rejected");
    assert_eq!(
        error,
        BranchRuntimeError::InvalidInheritedLayer {
            reason:
                "materialization output identity must not collide with existing reachable table",
        }
    );
    assert_eq!(child_state, before);
}

fn visible_rows(rows: &[BranchVisibleRow]) -> Vec<StorageRow> {
    rows.iter().map(|row| row.row().clone()).collect()
}
