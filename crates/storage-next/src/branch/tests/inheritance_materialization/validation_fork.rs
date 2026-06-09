#[test]
fn branch_runtime_errors_are_typed_and_preserve_sources() {
    let test_branch_id = branch_id(6);
    let invalid_config = BranchRuntimeConfig::new(0, 1, 1).expect_err("invalid config");
    let table_error = BranchRuntimeError::TableRuntime {
        source: crate::table::TableRuntimeError::Cache {
            reason: "cache unavailable",
        },
    };
    let variants = [
        invalid_config,
        BranchRuntimeError::InvalidBranchState { reason: "state" },
        BranchRuntimeError::BranchNotFound {
            branch_id: test_branch_id,
        },
        BranchRuntimeError::BranchAlreadyExists {
            branch_id: test_branch_id,
        },
        BranchRuntimeError::InvalidBranchRow { reason: "row" },
        BranchRuntimeError::InvalidReadBound { reason: "bound" },
        BranchRuntimeError::InvalidInheritedLayer { reason: "layer" },
        BranchRuntimeError::InvalidReachability {
            reason: "reachability",
        },
        BranchRuntimeError::InvalidCompaction {
            reason: BranchCompactionInvalidity::Generic("compaction"),
        },
        BranchRuntimeError::InvalidSnapshotInstall { reason: "snapshot" },
        table_error.clone(),
        BranchRuntimeError::publish("publish"),
    ];

    for error in variants {
        let text = error.to_string();
        assert!(!text.is_empty());
        assert!(!text.contains("secret-payload"));
    }

    let alias_result: BranchRuntimeResult<()> =
        Err(BranchRuntimeError::InvalidReadBound { reason: "alias" });
    assert!(matches!(
        alias_result,
        Err(BranchRuntimeError::InvalidReadBound { reason: "alias" })
    ));

    let source = table_error.source().expect("table source");
    assert!(source.to_string().contains("table cache operation failed"));

    let publish_error = BranchRuntimeError::publish_with("ambiguous", LeafError);
    let source = publish_error.source().expect("publish source");
    assert_eq!(source.to_string(), "leaf source");
}

#[test]
#[allow(clippy::too_many_lines)]
fn branch_inherited_layer_constructor_rejects_count_and_source_mismatches() {
    let source = branch_id(70);
    let child = branch_id(71);
    let table = branch_owned_table(
        source,
        BranchLevel::ZERO,
        "inherited-valid",
        vec![storage_row_with(
            source,
            b"inherited".to_vec(),
            3,
            30,
            Timestamp::EPOCH,
            b"secret-payload".to_vec(),
        )],
    );

    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(3),
        InheritedLayerStatus::Active,
        vec![vec![table.clone()]],
    );
    assert_eq!(layer.source_branch_id(), source);
    assert_eq!(layer.fork_version(), CommitVersion::new(3));
    assert_eq!(layer.table_count(), 1);

    let duplicate_identity = BranchInheritedLayer::new(
        InheritedLayerDescriptor::new(
            source,
            CommitVersion::new(3),
            InheritedLayerStatus::Active,
            2,
        ),
        vec![vec![
            branch_owned_table(
                source,
                BranchLevel::ZERO,
                "inherited-duplicate-identity",
                vec![storage_row_with(
                    source,
                    b"inherited-duplicate-a".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    b"a".to_vec(),
                )],
            ),
            branch_owned_table(
                source,
                BranchLevel::ZERO,
                "inherited-duplicate-identity",
                vec![storage_row_with(
                    source,
                    b"inherited-duplicate-b".to_vec(),
                    2,
                    20,
                    Timestamp::EPOCH,
                    b"b".to_vec(),
                )],
            ),
        ]],
    )
    .expect_err("duplicate inherited table identities rejected");
    assert_eq!(
        duplicate_identity,
        BranchRuntimeError::InvalidInheritedLayer {
            reason: "inherited layer tables must not contain duplicate table identities",
        }
    );

    let stale_count = BranchInheritedLayer::new(
        InheritedLayerDescriptor::new(
            source,
            CommitVersion::new(3),
            InheritedLayerStatus::Active,
            2,
        ),
        vec![vec![table.clone()]],
    )
    .expect_err("stale inherited table count rejected");
    assert!(matches!(
        stale_count,
        BranchRuntimeError::InvalidInheritedLayer { .. }
    ));
    assert!(!stale_count.to_string().contains("secret-payload"));

    let wrong_table = branch_owned_table(
        child,
        BranchLevel::ZERO,
        "inherited-wrong-source",
        vec![storage_row_with(
            child,
            b"inherited".to_vec(),
            3,
            30,
            Timestamp::EPOCH,
            b"secret-payload".to_vec(),
        )],
    );
    let wrong_source = BranchInheritedLayer::new(
        InheritedLayerDescriptor::new(
            source,
            CommitVersion::new(3),
            InheritedLayerStatus::Active,
            1,
        ),
        vec![vec![wrong_table]],
    )
    .expect_err("wrong source table rejected");
    assert!(matches!(
        wrong_source,
        BranchRuntimeError::InvalidInheritedLayer { .. }
    ));
    assert!(!wrong_source.to_string().contains("secret-payload"));

    let post_fork_table = branch_owned_table(
        source,
        BranchLevel::ZERO,
        "inherited-post-fork-source",
        vec![storage_row_with(
            source,
            b"inherited-post-fork".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            b"secret-payload".to_vec(),
        )],
    );
    let post_fork = BranchInheritedLayer::new(
        InheritedLayerDescriptor::new(
            source,
            CommitVersion::new(3),
            InheritedLayerStatus::Active,
            1,
        ),
        vec![vec![post_fork_table]],
    )
    .expect_err("post-fork inherited rows rejected");
    assert!(matches!(
        post_fork,
        BranchRuntimeError::InvalidInheritedLayer { .. }
    ));
    assert!(!post_fork.to_string().contains("secret-payload"));

    let view_error = BranchReadView::new_with_inherited(
        child,
        MutableTable::new(),
        Vec::new(),
        Vec::new(),
        vec![branch_inherited_layer(
            child,
            CommitVersion::new(3),
            InheritedLayerStatus::Active,
            Vec::new(),
        )],
        BranchStateFacts::new(child, 0, 0, 0, 1, None, None, None).expect("self inherited facts"),
    )
    .expect_err("self inheritance rejected");
    assert!(matches!(
        view_error,
        BranchRuntimeError::InvalidInheritedLayer { .. }
    ));
}

#[test]
fn branch_inherited_layer_rejects_duplicate_internal_keys_across_tables() {
    let source = branch_id(71);
    let duplicate_left = branch_owned_table(
        source,
        BranchLevel::ZERO,
        "inherited-duplicate-left",
        vec![storage_row_with(
            source,
            b"duplicate".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            b"secret-payload-left".to_vec(),
        )],
    );
    let duplicate_right = branch_owned_table(
        source,
        BranchLevel::ZERO,
        "inherited-duplicate-right",
        vec![storage_row_with(
            source,
            b"duplicate".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            b"secret-payload-right".to_vec(),
        )],
    );

    let error = BranchInheritedLayer::new(
        InheritedLayerDescriptor::new(
            source,
            CommitVersion::new(4),
            InheritedLayerStatus::Active,
            2,
        ),
        vec![vec![duplicate_left, duplicate_right]],
    )
    .expect_err("duplicate inherited internal keys rejected");
    assert!(matches!(
        error,
        BranchRuntimeError::InvalidInheritedLayer { .. }
    ));
    assert!(!error.to_string().contains("secret-payload"));
}

#[test]
fn branch_inherited_layer_status_and_count_edges_are_enforced() {
    let source = branch_id(72);
    let child = branch_id(73);
    let row = storage_row_with(
        source,
        b"status".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"source".to_vec(),
    );
    let materializing = branch_inherited_layer(
        source,
        CommitVersion::new(3),
        InheritedLayerStatus::Materializing,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "inherited-materializing",
            vec![row.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![materializing])
        .expect("materializing layer attaches");
    let visible = child_state
        .capture_read_view()
        .expect("materializing view")
        .latest(&physical_key(child, b"status".to_vec()))
        .expect("materializing latest")
        .expect("materializing inherited row");
    assert_eq!(
        visible.row(),
        &rewrite_row_branch(&row, source, child).expect("rewrite")
    );

    let materialized = branch_inherited_layer(
        source,
        CommitVersion::new(3),
        InheritedLayerStatus::Materialized,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "inherited-materialized",
            vec![row],
        )]],
    );
    let mut materialized_child = BranchLocalState::empty(child);
    materialized_child
        .attach_inherited_layers(vec![materialized])
        .expect("materialized layer attaches for diagnostic state");
    assert!(materialized_child
        .capture_read_view()
        .expect("materialized view")
        .latest(&physical_key(child, b"status".to_vec()))
        .expect("materialized latest")
        .is_none());

    let unavailable = branch_inherited_layer(
        source,
        CommitVersion::new(3),
        InheritedLayerStatus::Unavailable,
        Vec::new(),
    );
    let mut unavailable_child = BranchLocalState::empty(child);
    let unavailable_child_before = unavailable_child.clone();
    assert_eq!(
        unavailable_child
            .attach_inherited_layers(vec![unavailable])
            .expect_err("unavailable layer attach rejected"),
        BranchRuntimeError::InvalidInheritedLayer {
            reason: "unavailable inherited layers cannot attach",
        },
    );
    assert_eq!(unavailable_child, unavailable_child_before);

    let config = BranchRuntimeConfig::new(7, 1, 32).expect("one inherited layer config");
    let mut limited_child = BranchLocalState::new(child, config).expect("limited child");
    let first = branch_inherited_layer(
        source,
        CommitVersion::new(3),
        InheritedLayerStatus::Active,
        Vec::new(),
    );
    let second_source = branch_id(74);
    let second = branch_inherited_layer(
        second_source,
        CommitVersion::new(3),
        InheritedLayerStatus::Active,
        Vec::new(),
    );
    assert!(matches!(
        limited_child.attach_inherited_layers(vec![first, second]),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. })
    ));

    let mut empty_attach_child = BranchLocalState::empty(child);
    assert!(matches!(
        empty_attach_child.attach_inherited_layers(Vec::<BranchInheritedLayer>::new()),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. })
    ));
    assert!(empty_attach_child.inherited_layers().is_empty());
}

#[test]
fn branch_attach_rejects_reversed_inherited_layer_order() {
    let child = branch_id(74);
    let nearest_source = branch_id(75);
    let farther_source = branch_id(76);
    let nearest = branch_inherited_layer(
        nearest_source,
        CommitVersion::new(8),
        InheritedLayerStatus::Active,
        Vec::new(),
    );
    let farther = branch_inherited_layer(
        farther_source,
        CommitVersion::new(3),
        InheritedLayerStatus::Active,
        Vec::new(),
    );
    let mut child_state = BranchLocalState::empty(child);
    let error = child_state
        .attach_inherited_layers(vec![farther, nearest])
        .expect_err("reversed inherited stack rejected");
    assert_eq!(
        error,
        BranchRuntimeError::InvalidInheritedLayer {
            reason: "inherited layers must be ordered nearest-first by fork version",
        }
    );
    assert!(child_state.inherited_layers().is_empty());
}

#[test]
fn branch_attach_reports_nearest_inherited_layer_fork_version() {
    let child = branch_id(77);
    let nearest_source = branch_id(78);
    let farther_source = branch_id(79);
    let nearest = branch_inherited_layer(
        nearest_source,
        CommitVersion::new(8),
        InheritedLayerStatus::Active,
        Vec::new(),
    );
    let farther = branch_inherited_layer(
        farther_source,
        CommitVersion::new(3),
        InheritedLayerStatus::Active,
        Vec::new(),
    );
    let mut child_state = BranchLocalState::empty(child);

    let outcome = child_state
        .attach_inherited_layers(vec![nearest, farther])
        .expect("attach ordered inherited layers");

    assert_eq!(outcome.source_branch_id(), nearest_source);
    assert_eq!(outcome.destination_branch_id(), child);
    assert_eq!(outcome.fork_version(), CommitVersion::new(8));
    assert_eq!(outcome.inherited_layer_count(), 2);
}

#[test]
fn branch_fork_preserves_layer_order_and_readable_inherited_statuses() {
    let fixture = fork_status_fixture();
    assert_eq!(fixture.outcome.fork_version(), CommitVersion::new(6));
    assert_eq!(fixture.outcome.inherited_layer_count(), 2);
    assert_eq!(fixture.outcome.inherited_table_count(), 2);
    assert_eq!(
        fixture.child_state.inherited_layers()[0].source_branch_id(),
        fixture.source
    );
    assert_eq!(
        fixture.child_state.inherited_layers()[0].status(),
        InheritedLayerStatus::Active
    );
    assert_eq!(
        fixture.child_state.inherited_layers()[1].source_branch_id(),
        fixture.grandparent
    );
    assert_eq!(
        fixture.child_state.inherited_layers()[1].status(),
        InheritedLayerStatus::Materializing,
        "copied materializing inherited layers retain their recovery status"
    );

    let view = fixture.child_state.capture_read_view().expect("child view");
    assert_visible_row(
        view.latest(&physical_key(fixture.child, b"source-owned".to_vec()))
            .expect("source-owned latest")
            .as_ref(),
        &rewrite_row_branch(&fixture.source_owned, fixture.source, fixture.child)
            .expect("source-owned rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: fixture.source,
            layer_index: 0,
        },
    );
    assert_visible_row(
        view.latest(&physical_key(fixture.child, b"materializing".to_vec()))
            .expect("materializing latest")
            .as_ref(),
        &rewrite_row_branch(&fixture.inherited_row, fixture.grandparent, fixture.child)
            .expect("inherited rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: fixture.grandparent,
            layer_index: 1,
        },
    );
    assert!(
        view.latest(&physical_key(fixture.child, b"materialized".to_vec()))
            .expect("materialized latest")
            .is_none(),
        "materialized source layers are skipped when forking"
    );
}

#[test]
fn branch_fork_and_attach_rejections_do_not_mutate_state() {
    let branch = branch_id(87);
    let other = branch_id(88);
    let mut child_state = BranchLocalState::empty(branch);
    let original = child_state.clone();
    child_state
        .append_committed_row(storage_row_with(
            branch,
            b"owned-before-attach".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"owned".to_vec(),
        ))
        .expect("append own row");
    let non_empty = child_state.clone();
    let layer = branch_inherited_layer_unchecked_for_fork_gate_tests(
        other,
        CommitVersion::new(1),
        InheritedLayerStatus::Active,
        Vec::new(),
    );

    assert!(matches!(
        child_state.attach_inherited_layers(vec![layer]),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
    assert_eq!(child_state, non_empty);
    assert_ne!(child_state, original);

    let source_state = BranchLocalState::empty(branch);
    let source_before = source_state.clone();
    assert_eq!(
        source_state
            .fork_into_empty_child(branch)
            .expect_err("self-fork rejected"),
        BranchRuntimeError::InvalidInheritedLayer {
            reason: "fork source and destination branches must differ",
        },
    );
    assert_eq!(source_state, source_before);

    let unavailable = branch_inherited_layer(
        other,
        CommitVersion::new(1),
        InheritedLayerStatus::Unavailable,
        Vec::new(),
    );
    assert_eq!(
        unavailable
            .clone_active_for_fork()
            .expect_err("unavailable layer cannot be forked"),
        BranchRuntimeError::InvalidInheritedLayer {
            reason: "unavailable inherited layers cannot be forked",
        },
    );
}

#[test]
fn branch_fork_into_empty_child_captures_inherited_layers_without_copying_rows() {
    let source = branch_id(72);
    let child = branch_id(73);
    let mut source_state = BranchLocalState::empty(source);
    let inherited_row = storage_row_with(
        source,
        b"shared".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"parent".to_vec(),
    );
    let l1_row = storage_row_with(
        source,
        b"shared-l1".to_vec(),
        6,
        60,
        Timestamp::EPOCH,
        b"parent-l1".to_vec(),
    );
    let table = branch_owned_table(
        source,
        BranchLevel::ZERO,
        "fork-source-owned",
        vec![inherited_row.clone()],
    );
    source_state
        .install_l0_table(table)
        .expect("source install");
    source_state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                source,
                BranchLevel::new(1),
                "fork-source-owned-l1",
                vec![l1_row.clone()],
            ),
        )
        .expect("source install L1");
    let (child_state, outcome): (BranchLocalState, BranchForkOutcome) = source_state
        .fork_into_empty_child(child)
        .expect("fork child");
    assert_eq!(outcome.source_branch_id(), source);
    assert_eq!(outcome.destination_branch_id(), child);
    assert_eq!(outcome.fork_version(), CommitVersion::new(6));
    assert_eq!(outcome.inherited_layer_count(), 1);
    assert_eq!(outcome.inherited_table_count(), 2);
    assert!(child_state.active().is_empty());
    assert!(child_state.frozen().is_empty());
    assert_eq!(child_state.owned_table_count(), 0);
    assert_eq!(child_state.inherited_layer_count(), 1);
    assert_eq!(child_state.inherited_table_count(), 2);
    assert_eq!(child_state.inherited_layers()[0].source_branch_id(), source);
    assert_eq!(
        child_state.max_commit_version(),
        Some(CommitVersion::new(6))
    );
    assert_eq!(child_state.put_rows(), 0);

    let view = child_state.capture_read_view().expect("child read view");
    assert_eq!(view.inherited_layer_count(), 1);
    assert_eq!(
        view.inherited_layers()[0].fork_version(),
        CommitVersion::new(6)
    );
    let layout = view.source_layout();
    assert_eq!(layout.active_rows(), 0);
    assert_eq!(layout.frozen_table_count(), 0);
    assert_eq!(layout.owned_total_tables(), 0);
    assert_eq!(layout.inherited_layers(), 1);
    assert_eq!(layout.inherited_readable_layers(), 1);
    assert_eq!(layout.inherited_active_layers(), 1);
    assert_eq!(layout.inherited_l0_tables(), 1);
    assert_eq!(layout.inherited_total_tables(), 2);
    assert_eq!(
        layout.inherited_nonzero_level_table_counts(),
        &[super::super::facts::BranchLevelTableCount::new(
            BranchLevel::new(1),
            1
        )]
    );
    let expected = rewrite_row_branch(&inherited_row, source, child).expect("expected rewrite");
    let visible = view
        .latest(&physical_key(child, b"shared".to_vec()))
        .expect("latest")
        .expect("inherited row");
    assert_eq!(visible.row(), &expected);
    assert_eq!(
        visible.source(),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        }
    );
    assert_visible_row(
        view.latest(&physical_key(child, b"shared-l1".to_vec()))
            .expect("L1 latest")
            .as_ref(),
        &rewrite_row_branch(&l1_row, source, child).expect("expected L1 rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
}

#[test]
fn branch_fork_rejects_unflushed_active_and_frozen_source_rows() {
    let source = branch_id(79);
    let child = branch_id(80);
    let mut source_state = BranchLocalState::empty(source);
    source_state
        .append_committed_row(storage_row_with(
            source,
            b"active-only".to_vec(),
            8,
            80,
            Timestamp::EPOCH,
            b"must-flush".to_vec(),
        ))
        .expect("source active append");
    let before_active_rejection = source_state.clone();
    assert_eq!(
        source_state
            .fork_into_empty_child(child)
            .expect_err("active rows block fork capture"),
        BranchRuntimeError::InvalidInheritedLayer {
            reason: "fork source must flush active and frozen rows before inheritance capture",
        },
    );
    assert_eq!(source_state, before_active_rejection);

    source_state.rotate_active();
    let before_frozen_rejection = source_state.clone();
    assert_eq!(
        source_state
            .fork_into_empty_child(child)
            .expect_err("frozen rows block fork capture"),
        BranchRuntimeError::InvalidInheritedLayer {
            reason: "fork source must flush active and frozen rows before inheritance capture",
        },
    );
    assert_eq!(source_state, before_frozen_rejection);
}

#[test]
fn branch_fork_rejects_empty_source_without_ambiguous_zero_version() {
    let source = branch_id(81);
    let child = branch_id(82);
    let source_state = BranchLocalState::empty(source);
    let error = source_state
        .fork_into_empty_child(child)
        .expect_err("empty source fork rejected");
    assert_eq!(
        error,
        BranchRuntimeError::InvalidInheritedLayer {
            reason: "fork source must contain at least one retained row",
        }
    );
}

#[test]
fn branch_fork_uses_inherited_rows_when_source_has_no_own_rows() {
    let source = branch_id(83);
    let inherited_only = branch_id(84);
    let child = branch_id(85);
    let row = storage_row_with(
        source,
        b"fork-own-source".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"source".to_vec(),
    );
    let mut source_state = BranchLocalState::empty(source);
    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "fork-own-source",
            vec![row.clone()],
        ))
        .expect("install source table");
    let (inherited_only_state, _) = source_state
        .fork_into_empty_child(inherited_only)
        .expect("fork inherited-only parent");

    let (child_state, outcome) = inherited_only_state
        .fork_into_empty_child(child)
        .expect("inherited-only source can fork from retained inherited rows");

    assert_eq!(outcome.fork_version(), CommitVersion::new(3));
    assert_eq!(outcome.inherited_layer_count(), 2);
    assert_eq!(
        child_state.inherited_layers()[0].source_branch_id(),
        inherited_only
    );
    assert_eq!(child_state.inherited_layers()[0].table_count(), 0);
    assert_eq!(child_state.inherited_layers()[1].source_branch_id(), source);
    assert_visible_row(
        child_state
            .capture_read_view()
            .expect("child view")
            .latest(&physical_key(child, b"fork-own-source".to_vec()))
            .expect("latest")
            .as_ref(),
        &rewrite_row_branch(&row, source, child).expect("rewrite inherited row"),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 1,
        },
    );
}

#[test]
fn branch_fork_child_local_rows_shadow_inherited_parent_rows() {
    let source = branch_id(91);
    let child_with_put = branch_id(92);
    let child_with_tombstone = branch_id(93);
    let parent_row = storage_row_with(
        source,
        b"fork-shadow".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"parent".to_vec(),
    );
    let mut source_state = BranchLocalState::empty(source);
    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "fork-shadow-source",
            vec![parent_row.clone()],
        ))
        .expect("install source row");

    let (mut put_child_state, _) = source_state
        .fork_into_empty_child(child_with_put)
        .expect("fork child for put shadow");
    let child_put = storage_row_with(
        child_with_put,
        b"fork-shadow".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"child".to_vec(),
    );
    put_child_state
        .append_committed_row(child_put.clone())
        .expect("append child put");
    let put_child_key = physical_key(child_with_put, b"fork-shadow".to_vec());
    let put_view = put_child_state
        .capture_read_view()
        .expect("put child view");
    assert_visible_row(
        put_view
            .latest(&put_child_key)
            .expect("put child latest")
            .as_ref(),
        &child_put,
        BranchRowSource::Active,
    );
    assert_visible_row(
        put_view
            .at_version(&put_child_key, CommitVersion::new(4))
            .expect("put child inherited version")
            .as_ref(),
        &rewrite_row_branch(&parent_row, source, child_with_put).expect("rewrite parent row"),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    let put_history = put_view
        .history(
            &put_child_key,
            BranchHistoryOptions::all().include_tombstones(true),
        )
        .expect("put child history");
    assert_eq!(history_versions(&put_history), vec![5, 4]);
    assert_eq!(put_history[0].source(), BranchRowSource::Active);
    assert_eq!(
        put_history[1].source(),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        }
    );

    let (mut tombstone_child_state, _) = source_state
        .fork_into_empty_child(child_with_tombstone)
        .expect("fork child for tombstone shadow");
    let child_tombstone = tombstone_row(child_with_tombstone, b"fork-shadow".to_vec(), 5, 50);
    tombstone_child_state
        .append_committed_row(child_tombstone)
        .expect("append child tombstone");
    let tombstone_child_key = physical_key(child_with_tombstone, b"fork-shadow".to_vec());
    let tombstone_view = tombstone_child_state
        .capture_read_view()
        .expect("tombstone child view");
    assert!(
        tombstone_view
            .latest(&tombstone_child_key)
            .expect("tombstone child latest")
            .is_none(),
        "child tombstone must hide inherited parent row"
    );
    assert_visible_row(
        tombstone_view
            .at_version(&tombstone_child_key, CommitVersion::new(4))
            .expect("tombstone child inherited version")
            .as_ref(),
        &rewrite_row_branch(&parent_row, source, child_with_tombstone)
            .expect("rewrite parent row for tombstone child"),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    assert_eq!(
        history_versions(
            &tombstone_view
                .history(
                    &tombstone_child_key,
                    BranchHistoryOptions::all().include_tombstones(true),
                )
                .expect("tombstone child history"),
        ),
        vec![5, 4],
    );
    assert_eq!(
        history_versions(
            &tombstone_view
                .history(
                    &tombstone_child_key,
                    BranchHistoryOptions::all().include_tombstones(false),
                )
                .expect("tombstone child visible history"),
        ),
        vec![4],
    );
}

#[test]
fn branch_fork_frontier_survives_parent_compaction() {
    let source = branch_id(86);
    let child = branch_id(87);
    let compacted_new = storage_row_with(
        source,
        b"fork-parent-compact".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"new".to_vec(),
    );
    let compacted_old = storage_row_with(
        source,
        b"fork-parent-compact".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"old".to_vec(),
    );
    let mut source_state = BranchLocalState::empty(source);
    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "fork-parent-compact-new",
            vec![compacted_new.clone()],
        ))
        .expect("install new L0");
    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "fork-parent-compact-old",
            vec![compacted_old.clone()],
        ))
        .expect("install old L0");

    let (child_state, outcome) = source_state
        .fork_into_empty_child(child)
        .expect("fork before parent compaction");
    assert_eq!(outcome.fork_version(), CommitVersion::new(5));
    let child_key = physical_key(child, b"fork-parent-compact".to_vec());
    assert_visible_row(
        child_state
            .capture_read_view()
            .expect("child before parent compaction")
            .latest(&child_key)
            .expect("child latest before parent compaction")
            .as_ref(),
        &rewrite_row_branch(&compacted_new, source, child).expect("rewrite compacted row"),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );

    let request = BranchCompactionRequest::new(
        source,
        BranchCompactionKind::CompactL0,
        "fork-parent-compact-output",
    )
    .expect("parent compaction request");
    source_state
        .compact_branch_owned_tables(&request)
        .expect("compact parent after fork");
    assert_eq!(source_state.owned_levels()[0].len(), 1);

    assert_visible_row(
        child_state
            .capture_read_view()
            .expect("child after parent compaction")
            .latest(&child_key)
            .expect("child latest after parent compaction")
            .as_ref(),
        &rewrite_row_branch(&compacted_new, source, child).expect("rewrite compacted row"),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    assert_eq!(
        history_versions(
            &child_state
                .capture_read_view()
                .expect("child history after parent compaction")
                .history(
                    &child_key,
                    BranchHistoryOptions::all().include_tombstones(true),
                )
                .expect("child history after parent compaction"),
        ),
        vec![5, 2],
    );
}

#[test]
fn branch_fork_frontier_survives_parent_materialization() {
    let grandparent = branch_id(88);
    let parent = branch_id(89);
    let child = branch_id(90);
    let row = storage_row_with(
        grandparent,
        b"fork-parent-materialize".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"grandparent".to_vec(),
    );
    let old_row = storage_row_with(
        grandparent,
        b"fork-parent-materialize".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"grandparent-old".to_vec(),
    );
    let mut grandparent_state = BranchLocalState::empty(grandparent);
    grandparent_state
        .install_l0_table(branch_owned_table(
            grandparent,
            BranchLevel::ZERO,
            "fork-parent-materialize-grandparent",
            vec![old_row.clone(), row.clone()],
        ))
        .expect("install grandparent row");
    let (mut parent_state, _) = grandparent_state
        .fork_into_empty_child(parent)
        .expect("fork parent");
    let (child_state, child_outcome) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    assert_eq!(child_outcome.inherited_layer_count(), 2);
    let child_key = physical_key(child, b"fork-parent-materialize".to_vec());
    assert_visible_row(
        child_state
            .capture_read_view()
            .expect("child before parent materialization")
            .latest(&child_key)
            .expect("child latest before parent materialization")
            .as_ref(),
        &rewrite_row_branch(&row, grandparent, child).expect("rewrite grandparent row"),
        BranchRowSource::Inherited {
            source_branch_id: grandparent,
            layer_index: 1,
        },
    );
    assert_eq!(
        history_versions(
            &child_state
                .capture_read_view()
                .expect("child history before parent materialization")
                .history(
                    &child_key,
                    BranchHistoryOptions::all().include_tombstones(true),
                )
                .expect("child history before parent materialization"),
        ),
        vec![4, 1],
    );

    parent_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(parent, 0, "fork-parent-materialize")
                .expect("parent materialization request"),
        )
        .expect("parent materializes inherited layer");
    assert_eq!(parent_state.inherited_layer_count(), 0);
    assert_eq!(parent_state.owned_table_count(), 1);

    assert_visible_row(
        child_state
            .capture_read_view()
            .expect("child after parent materialization")
            .latest(&child_key)
            .expect("child latest after parent materialization")
            .as_ref(),
        &rewrite_row_branch(&row, grandparent, child).expect("rewrite grandparent row"),
        BranchRowSource::Inherited {
            source_branch_id: grandparent,
            layer_index: 1,
        },
    );
    let after_materialization_view = child_state
        .capture_read_view()
        .expect("child after parent materialization history view");
    assert_eq!(
        history_versions(
            &after_materialization_view
                .history(
                    &child_key,
                    BranchHistoryOptions::all().include_tombstones(true),
                )
                .expect("child history after parent materialization"),
        ),
        vec![4, 1],
    );
    assert_visible_row(
        after_materialization_view
            .at_version(&child_key, CommitVersion::new(1))
            .expect("child old version after parent materialization")
            .as_ref(),
        &rewrite_row_branch(&old_row, grandparent, child).expect("rewrite old grandparent row"),
        BranchRowSource::Inherited {
            source_branch_id: grandparent,
            layer_index: 1,
        },
    );
    assert_eq!(child_state.inherited_layer_count(), 2);
    assert_eq!(child_state.owned_table_count(), 0);
}
