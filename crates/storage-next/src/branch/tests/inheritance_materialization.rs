use super::*;

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
            reason: "compaction",
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
    assert!(matches!(
        unavailable_child.attach_inherited_layers(vec![unavailable]),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. })
    ));

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
fn branch_fork_preserves_layer_order_and_resets_readable_inherited_statuses() {
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
        InheritedLayerStatus::Active,
        "copied materializing inherited layers reset to active"
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
    assert!(matches!(
        source_state.fork_into_empty_child(branch),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. })
    ));
    assert_eq!(source_state, source_before);

    let unavailable = branch_inherited_layer(
        other,
        CommitVersion::new(1),
        InheritedLayerStatus::Unavailable,
        Vec::new(),
    );
    assert!(matches!(
        unavailable.clone_active_for_fork(),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. })
    ));
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
    let table = branch_owned_table(
        source,
        BranchLevel::ZERO,
        "fork-source-owned",
        vec![inherited_row.clone()],
    );
    source_state
        .install_l0_table(table)
        .expect("source install");
    let (child_state, outcome): (BranchLocalState, BranchForkOutcome) = source_state
        .fork_into_empty_child(child)
        .expect("fork child");
    assert_eq!(outcome.source_branch_id(), source);
    assert_eq!(outcome.destination_branch_id(), child);
    assert_eq!(outcome.fork_version(), CommitVersion::new(5));
    assert_eq!(outcome.inherited_layer_count(), 1);
    assert_eq!(outcome.inherited_table_count(), 1);
    assert!(child_state.active().is_empty());
    assert!(child_state.frozen().is_empty());
    assert_eq!(child_state.owned_table_count(), 0);
    assert_eq!(child_state.inherited_layer_count(), 1);
    assert_eq!(child_state.inherited_table_count(), 1);
    assert_eq!(child_state.inherited_layers()[0].source_branch_id(), source);
    assert_eq!(
        child_state.max_commit_version(),
        Some(CommitVersion::new(5))
    );
    assert_eq!(child_state.put_rows(), 0);

    let view = child_state.capture_read_view().expect("child read view");
    assert_eq!(view.inherited_layer_count(), 1);
    assert_eq!(
        view.inherited_layers()[0].fork_version(),
        CommitVersion::new(5)
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
    assert!(matches!(
        source_state.fork_into_empty_child(child),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. })
    ));

    source_state.rotate_active();
    assert!(matches!(
        source_state.fork_into_empty_child(child),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. })
    ));
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
fn branch_fork_rejects_inherited_only_source_without_own_applied_version() {
    let source = branch_id(83);
    let inherited_only = branch_id(84);
    let child = branch_id(85);
    let mut source_state = BranchLocalState::empty(source);
    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "fork-own-source",
            vec![storage_row_with(
                source,
                b"fork-own-source".to_vec(),
                3,
                30,
                Timestamp::EPOCH,
                b"source".to_vec(),
            )],
        ))
        .expect("install source table");
    let (inherited_only_state, _) = source_state
        .fork_into_empty_child(inherited_only)
        .expect("fork inherited-only parent");

    let error = inherited_only_state
        .fork_into_empty_child(child)
        .expect_err("inherited-only source cannot mint synthetic fork version");

    assert_eq!(
        error,
        BranchRuntimeError::InvalidInheritedLayer {
            reason: "fork source must contain at least one retained row",
        }
    );
}

#[test]
fn branch_inherited_reads_apply_fork_gate_and_child_tombstone_shadowing() {
    let source = branch_id(74);
    let child = branch_id(75);
    let visible_source = storage_row_with(
        source,
        b"gate".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"visible".to_vec(),
    );
    let post_fork_source = storage_row_with(
        source,
        b"gate".to_vec(),
        7,
        70,
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
            "fork-gated-source",
            vec![visible_source.clone(), post_fork_source],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");

    let child_key = physical_key(child, b"gate".to_vec());
    let view = child_state
        .capture_read_view()
        .expect("view")
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    let expected = rewrite_row_branch(&visible_source, source, child).expect("rewrite");
    assert_visible_row(
        view.latest(&child_key).expect("latest").as_ref(),
        &expected,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    assert_visible_row(
        view.at_version(&child_key, CommitVersion::new(7))
            .expect("bounded")
            .as_ref(),
        &expected,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );

    child_state
        .append_committed_row(tombstone_row(child, b"gate".to_vec(), 6, 60))
        .expect("child tombstone");
    let shadowed = child_state.capture_read_view().expect("shadowed view");
    assert!(
        shadowed.latest(&child_key).expect("latest").is_none(),
        "child tombstone must shadow inherited put"
    );
    assert_visible_row(
        shadowed
            .at_version(&child_key, CommitVersion::new(4))
            .expect("before tombstone")
            .as_ref(),
        &expected,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
}

#[test]
fn branch_inherited_timestamp_reads_apply_timestamp_and_fork_gates() {
    let source = branch_id(61);
    let child = branch_id(62);
    let visible_source = storage_row_with(
        source,
        b"time-gate".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"visible".to_vec(),
    );
    let source_future_timestamp = storage_row_with(
        source,
        b"time-gate".to_vec(),
        4,
        70,
        Timestamp::EPOCH,
        b"future-time".to_vec(),
    );
    let source_after_fork_with_old_timestamp = storage_row_with(
        source,
        b"time-gate".to_vec(),
        7,
        20,
        Timestamp::EPOCH,
        b"after-fork".to_vec(),
    );
    let layer = branch_inherited_layer_unchecked_for_fork_gate_tests(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "inherited-time-gate",
            vec![
                visible_source.clone(),
                source_future_timestamp,
                source_after_fork_with_old_timestamp,
            ],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let child_key = physical_key(child, b"time-gate".to_vec());
    let view = child_state
        .capture_read_view()
        .expect("view")
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    assert_visible_row(
        view.read_point(
            &child_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .expect("timestamp inherited read")
        .as_ref(),
        &rewrite_row_branch(&visible_source, source, child).expect("visible rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    assert_eq!(
        view.read_point(
            &child_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
        )
        .expect("below visible timestamp"),
        None,
        "post-fork source row with an old timestamp remains hidden by fork version",
    );

    let child_expired = storage_row_with(
        child,
        b"time-gate".to_vec(),
        6,
        35,
        Timestamp::from_micros(39),
        b"child-expired".to_vec(),
    );
    child_state
        .append_committed_row(child_expired)
        .expect("append child expired row");
    let shadowed = child_state.capture_read_view().expect("shadowed view");
    assert_eq!(
        shadowed
            .read_point(
                &child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            )
            .expect("timestamp expired child read"),
        None,
        "selected child-local expired row suppresses inherited fallback",
    );
}

#[test]
fn branch_inherited_timestamp_reads_pick_nearest_layer_for_exact_ties() {
    let (child_state, fixture) = inherited_timestamp_shadow_fixture();
    let inherited_view = child_state.capture_read_view().expect("view");
    assert_visible_row(
        inherited_view
            .read_point(
                &fixture.child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            )
            .expect("nearest inherited timestamp read")
            .as_ref(),
        &fixture.expected_nearest,
        BranchRowSource::Inherited {
            source_branch_id: fixture.nearest_source,
            layer_index: 0,
        },
    );
}

#[test]
fn branch_inherited_timestamp_reads_apply_local_put_and_tombstone_shadows() {
    let (mut child_state, fixture) = inherited_timestamp_shadow_fixture();
    let child_put = storage_row_with(
        fixture.child,
        fixture.key.clone(),
        4,
        35,
        Timestamp::EPOCH,
        b"child-put".to_vec(),
    );
    child_state
        .append_committed_row(child_put.clone())
        .expect("append child put");
    assert_visible_row(
        child_state
            .capture_read_view()
            .expect("put view")
            .read_point(
                &fixture.child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            )
            .expect("child put timestamp read")
            .as_ref(),
        &child_put,
        BranchRowSource::Active,
    );

    child_state
        .append_committed_row(tombstone_row(fixture.child, fixture.key, 5, 45))
        .expect("append child tombstone");
    assert_eq!(
        child_state
            .capture_read_view()
            .expect("tombstone view")
            .read_point(
                &fixture.child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(50)),
            )
            .expect("child tombstone timestamp read"),
        None,
        "child-local tombstone at timestamp shadows inherited puts",
    );
}

struct InheritedTimestampShadowFixture {
    nearest_source: BranchId,
    child: BranchId,
    key: Vec<u8>,
    child_key: PhysicalKey,
    expected_nearest: StorageRow,
}

fn inherited_timestamp_shadow_fixture() -> (BranchLocalState, InheritedTimestampShadowFixture) {
    let nearest_source = branch_id(63);
    let farther_source = branch_id(64);
    let child = branch_id(65);
    let key = b"time-shadow".to_vec();
    let nearest_row = storage_row_with(
        nearest_source,
        key.clone(),
        3,
        30,
        Timestamp::EPOCH,
        b"nearest".to_vec(),
    );
    let farther_row = storage_row_with(
        farther_source,
        key.clone(),
        3,
        30,
        Timestamp::EPOCH,
        b"farther".to_vec(),
    );
    let nearest_layer = branch_inherited_layer(
        nearest_source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            nearest_source,
            BranchLevel::ZERO,
            "nearest-timestamp-tie",
            vec![nearest_row.clone()],
        )]],
    );
    let farther_layer = branch_inherited_layer(
        farther_source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            farther_source,
            BranchLevel::ZERO,
            "farther-timestamp-tie",
            vec![farther_row],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![nearest_layer, farther_layer])
        .expect("attach inherited layers");
    let child_key = physical_key(child, key.clone());
    let expected_nearest =
        rewrite_row_branch(&nearest_row, nearest_source, child).expect("nearest rewrite");
    (
        child_state,
        InheritedTimestampShadowFixture {
            nearest_source,
            child,
            key,
            child_key,
            expected_nearest,
        },
    )
}

#[test]
fn branch_inherited_timestamp_view_is_pinned_after_source_mutation() {
    let source = branch_id(66);
    let child = branch_id(67);
    let mut source_state = BranchLocalState::empty(source);
    let inherited = storage_row_with(
        source,
        b"source-pinned-ts".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"inherited".to_vec(),
    );
    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "source-pinned-ts-base",
            vec![inherited.clone()],
        ))
        .expect("install base source table");
    let (child_state, _) = source_state
        .fork_into_empty_child(child)
        .expect("fork child");
    let pinned = child_state.capture_read_view().expect("pinned child view");

    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "source-pinned-ts-later",
            vec![storage_row_with(
                source,
                b"source-pinned-ts-later".to_vec(),
                4,
                40,
                Timestamp::EPOCH,
                b"later".to_vec(),
            )],
        ))
        .expect("install later source table");

    let expected = rewrite_row_branch(&inherited, source, child).expect("rewrite inherited");
    assert_visible_row(
        pinned
            .read_point(
                &physical_key(child, b"source-pinned-ts".to_vec()),
                BranchReadBound::at_timestamp(Timestamp::from_micros(50)),
            )
            .expect("pinned inherited timestamp read")
            .as_ref(),
        &expected,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    assert_eq!(
        pinned
            .read_point(
                &physical_key(child, b"source-pinned-ts-later".to_vec()),
                BranchReadBound::at_timestamp(Timestamp::from_micros(50)),
            )
            .expect("pinned inherited later read"),
        None,
        "captured child timestamp view must not observe later source mutation",
    );
}

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

#[test]
fn branch_materialization_retry_removes_layer_when_replacements_are_already_visible() {
    let source = branch_id(111);
    let child = branch_id(112);
    let source_row = storage_row_with(
        source,
        b"materialize-retry-visible".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"source".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(4),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materialize-retry-source",
            vec![source_row.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach retry layer");
    let rewritten = rewrite_row_branch(&source_row, source, child).expect("rewrite retry row");
    let reader = immutable_reader("materialize-retry-layer-0-table-0", vec![rewritten.clone()]);
    let descriptor = branch_table_descriptor(BranchLevel::ZERO, &reader);
    let replacement = BranchOwnedTable::new_materialization_replacement(
        child,
        descriptor,
        reader,
        BranchMaterializationSource::new(source, CommitVersion::new(4)),
    )
    .expect("replacement table");
    child_state
        .install_l0_table(replacement)
        .expect("preinstall visible replacement");

    let outcome = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialize-retry").expect("request"),
        )
        .expect("retry materialization");

    assert_eq!(
        outcome.recovery(),
        BranchMaterializationRecovery::ReplacementAlreadyVisibleLayerRemoved,
    );
    assert_eq!(outcome.rows_materialized(), 1);
    assert_eq!(outcome.tables_created(), 0);
    assert_eq!(outcome.skipped_exact_duplicate_rows(), 1);
    assert_eq!(outcome.inherited_layers_remaining(), 0);
    assert_eq!(outcome.replacement_owned_table_count(), 1);
    assert_eq!(child_state.inherited_layer_count(), 0);
    assert_visible_row(
        child_state
            .capture_read_view()
            .expect("retry view")
            .latest(&physical_key(child, b"materialize-retry-visible".to_vec()))
            .expect("retry latest")
            .as_ref(),
        &rewritten,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
}

#[test]
fn branch_materialization_rejects_bad_request_without_mutation() {
    let source = branch_id(103);
    let child = branch_id(104);
    let active_layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        Vec::new(),
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![active_layer])
        .expect("attach active layer");
    let before = child_state.clone();
    assert!(matches!(
        child_state.materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 1, "materialized-missing")
                .expect("missing request"),
        ),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. })
    ));
    assert_eq!(
        child_state, before,
        "missing layer materialization must not mutate state",
    );
    assert!(matches!(
        child_state.materialize_inherited_layer(
            &BranchMaterializationRequest::new(source, 0, "materialized-wrong-branch")
                .expect("wrong branch request"),
        ),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
    assert_eq!(
        child_state, before,
        "wrong-branch materialization must not mutate state",
    );
}

#[test]
fn branch_materialization_accepts_materializing_layer_status() {
    let source = branch_id(103);
    let child = branch_id(104);
    let materializing_row = storage_row_with(
        source,
        b"materializing-status".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"status".to_vec(),
    );
    let materializing_layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Materializing,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materializing-status-source",
            vec![materializing_row.clone()],
        )]],
    );
    let mut materializing_child = BranchLocalState::empty(child);
    materializing_child
        .attach_inherited_layers(vec![materializing_layer])
        .expect("attach materializing layer");
    let materializing_outcome = materializing_child
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-status")
                .expect("materializing request"),
        )
        .expect("materialize materializing layer");
    assert_eq!(
        materializing_outcome.recovery(),
        BranchMaterializationRecovery::ReplacementVisibleLayerRemoved,
    );
    assert_eq!(materializing_outcome.rows_materialized(), 1);
    assert_eq!(materializing_outcome.tables_created(), 1);
    assert_eq!(materializing_child.inherited_layer_count(), 0);
    let materialized_row =
        rewrite_row_branch(&materializing_row, source, child).expect("materializing rewrite");
    assert_visible_row(
        materializing_child
            .capture_read_view()
            .expect("materializing view")
            .latest(materialized_row.physical_key())
            .expect("materializing latest")
            .as_ref(),
        &materialized_row,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
}

#[test]
fn branch_materialization_intent_marks_source_reachability_before_replacement() {
    let source = branch_id(103);
    let child = branch_id(104);
    let row = storage_row_with(
        source,
        b"materialization-intent".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"source".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materialization-intent-source",
            vec![row],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach active inherited layer");

    let snapshot = child_state
        .mark_inherited_layer_materializing(0)
        .expect("mark materializing");
    assert_eq!(
        child_state.inherited_layers()[0].status(),
        InheritedLayerStatus::Materializing,
    );
    assert_eq!(snapshot.table_refs().len(), 1);
    assert!(matches!(
        snapshot.table_refs()[0].reference_kind(),
        BranchTableReferenceKind::MaterializingSource {
            source_branch_id,
            fork_version,
            layer_index,
        } if source_branch_id == source
            && fork_version == CommitVersion::new(5)
            && layer_index == 0
    ));

    let retry = child_state
        .mark_inherited_layer_materializing(0)
        .expect("materializing retry");
    assert_eq!(retry, snapshot);
    assert!(matches!(
        child_state.mark_inherited_layer_materializing(1),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. })
    ));
}

#[test]
fn branch_materialization_rejects_unavailable_same_source_and_invalid_descriptors() {
    let source = branch_id(103);
    let child = branch_id(104);
    let unavailable = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Unavailable,
        Vec::new(),
    );
    let mut unavailable_child = BranchLocalState::empty(child);
    assert!(matches!(
        unavailable_child.attach_inherited_layers(vec![unavailable]),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. }),
    ));
    let same_branch_layer = branch_inherited_layer(
        child,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        Vec::new(),
    );
    let mut same_branch_child = BranchLocalState::empty(child);
    assert!(matches!(
        same_branch_child.attach_inherited_layers(vec![same_branch_layer]),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. }),
    ));

    let wrong_source_table = branch_owned_table(
        branch_id(105),
        BranchLevel::ZERO,
        "materialize-wrong-source-table",
        vec![storage_row_with(
            branch_id(105),
            b"wrong-source".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"wrong".to_vec(),
        )],
    );
    assert!(matches!(
        BranchInheritedLayer::new(
            InheritedLayerDescriptor::new(
                source,
                CommitVersion::new(5),
                InheritedLayerStatus::Active,
                1,
            ),
            vec![vec![wrong_source_table]],
        ),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. }),
    ));
    assert!(matches!(
        BranchInheritedLayer::new(
            InheritedLayerDescriptor::new(
                source,
                CommitVersion::new(5),
                InheritedLayerStatus::Active,
                1,
            ),
            Vec::new(),
        ),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. }),
    ));
}

#[test]
fn branch_materialization_preserves_edge_row_facts_and_table_facts() {
    let source = branch_id(106);
    let child = branch_id(107);
    let system_space = StorageSpaceId::engine(0x21).expect("system storage space");
    let empty_system_key = StorageRow::put(
        physical_key_with(source, "system", system_space, Vec::new()),
        CommitVersion::new(5),
        Timestamp::from_micros(55),
        Timestamp::MAX,
        Vec::new(),
    );
    let binary_key = StorageRow::put(
        physical_key(source, vec![0x00, 0x80, b'L', b'6', b'H']),
        CommitVersion::new(4),
        Timestamp::from_micros(44),
        Timestamp::from_micros(144),
        vec![0x00, 0xff],
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materialize-edge-source",
            vec![empty_system_key.clone(), binary_key.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach edge layer");
    let outcome = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-edge")
                .expect("edge request"),
        )
        .expect("materialize edge rows");
    assert_eq!(outcome.rows_materialized(), 2);
    assert_eq!(outcome.tables_created(), 1);
    let table = &child_state.owned_levels()[0][0];
    assert_eq!(table.facts().row_count(), 2);
    assert_eq!(table.facts().commit_range().min(), CommitVersion::new(4),);
    assert_eq!(table.facts().commit_range().max(), CommitVersion::new(5),);
    assert!(!table.descriptor().identity().as_str().contains('/'));

    let system_rewritten =
        rewrite_row_branch(&empty_system_key, source, child).expect("system rewrite");
    let binary_rewritten = rewrite_row_branch(&binary_key, source, child).expect("binary rewrite");
    let view = child_state.capture_read_view().expect("edge view");
    assert_visible_row(
        view.latest(system_rewritten.physical_key())
            .expect("system latest")
            .as_ref(),
        &system_rewritten,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_visible_row(
        view.latest(binary_rewritten.physical_key())
            .expect("binary latest")
            .as_ref(),
        &binary_rewritten,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_eq!(system_rewritten.physical_key().space(), "system");
    assert_eq!(
        system_rewritten.physical_key().storage_space_id(),
        system_space
    );
    assert!(system_rewritten.physical_key().user_key().is_empty());
    assert_eq!(system_rewritten.expires_at(), Timestamp::MAX);
    assert!(system_rewritten.value().is_empty());
    assert_eq!(
        binary_rewritten.physical_key().user_key(),
        &[0x00, 0x80, b'L', b'6', b'H']
    );
    assert_eq!(binary_rewritten.value(), &[0x00, 0xff]);
    assert_eq!(binary_rewritten.expires_at(), Timestamp::from_micros(144));
}

struct LayerOrderMaterializationFixture {
    nearest_source: BranchId,
    farther_source: BranchId,
    child: BranchId,
    child_key: PhysicalKey,
    nearest_duplicate: StorageRow,
    farther_history: StorageRow,
    child_state: BranchLocalState,
}

fn layer_order_materialization_fixture() -> LayerOrderMaterializationFixture {
    let nearest_source = branch_id(108);
    let farther_source = branch_id(109);
    let child = branch_id(110);
    let shared_key = b"materialized-layer-order".to_vec();
    let nearest_duplicate = storage_row_with(
        nearest_source,
        shared_key.clone(),
        4,
        40,
        Timestamp::EPOCH,
        b"shared".to_vec(),
    );
    let farther_duplicate = storage_row_with(
        farther_source,
        shared_key.clone(),
        4,
        40,
        Timestamp::EPOCH,
        b"shared".to_vec(),
    );
    let farther_history = storage_row_with(
        farther_source,
        shared_key.clone(),
        2,
        20,
        Timestamp::EPOCH,
        b"farther-history".to_vec(),
    );
    let nearest_layer = branch_inherited_layer(
        nearest_source,
        CommitVersion::new(4),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            nearest_source,
            BranchLevel::ZERO,
            "materialize-nearest-source",
            vec![nearest_duplicate.clone()],
        )]],
    );
    let farther_layer = branch_inherited_layer(
        farther_source,
        CommitVersion::new(4),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            farther_source,
            BranchLevel::ZERO,
            "materialize-farther-source",
            vec![farther_duplicate, farther_history.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![nearest_layer, farther_layer])
        .expect("attach ordered layers");
    LayerOrderMaterializationFixture {
        nearest_source,
        farther_source,
        child,
        child_key: physical_key(child, shared_key),
        nearest_duplicate,
        farther_history,
        child_state,
    }
}

#[test]
fn branch_materialization_preserves_layer_order_when_deep_layer_materialized_first() {
    let mut fixture = layer_order_materialization_fixture();
    let before = fixture
        .child_state
        .capture_read_view()
        .expect("before order view");
    assert_visible_row(
        before
            .latest(&fixture.child_key)
            .expect("before latest")
            .as_ref(),
        &rewrite_row_branch(
            &fixture.nearest_duplicate,
            fixture.nearest_source,
            fixture.child,
        )
        .expect("nearest rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: fixture.nearest_source,
            layer_index: 0,
        },
    );
    assert_visible_row(
        before
            .at_version(&fixture.child_key, CommitVersion::new(3))
            .expect("before historical")
            .as_ref(),
        &rewrite_row_branch(
            &fixture.farther_history,
            fixture.farther_source,
            fixture.child,
        )
        .expect("farther rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: fixture.farther_source,
            layer_index: 1,
        },
    );

    let farther_outcome = fixture
        .child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(fixture.child, 1, "materialized-farther")
                .expect("farther request"),
        )
        .expect("materialize farther first");
    assert_eq!(farther_outcome.rows_materialized(), 1);
    assert_eq!(farther_outcome.skipped_exact_duplicate_rows(), 1);
    assert_eq!(
        fixture.child_state.inherited_layers()[0].source_branch_id(),
        fixture.nearest_source
    );

    let after_farther = fixture
        .child_state
        .capture_read_view()
        .expect("after farther view");
    assert_visible_row(
        after_farther
            .latest(&fixture.child_key)
            .expect("after farther latest")
            .as_ref(),
        &rewrite_row_branch(
            &fixture.nearest_duplicate,
            fixture.nearest_source,
            fixture.child,
        )
        .expect("nearest rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: fixture.nearest_source,
            layer_index: 0,
        },
    );
    assert_visible_row(
        after_farther
            .at_version(&fixture.child_key, CommitVersion::new(3))
            .expect("after farther historical")
            .as_ref(),
        &rewrite_row_branch(
            &fixture.farther_history,
            fixture.farther_source,
            fixture.child,
        )
        .expect("farther rewrite"),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
}

#[test]
fn branch_materialization_preserves_nearest_and_history_after_all_layers_materialize() {
    let mut fixture = layer_order_materialization_fixture();
    fixture
        .child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(fixture.child, 1, "materialized-farther")
                .expect("farther request"),
        )
        .expect("materialize farther first");
    let nearest_outcome = fixture
        .child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(fixture.child, 0, "materialized-nearest")
                .expect("nearest request"),
        )
        .expect("materialize nearest");
    assert_eq!(nearest_outcome.rows_materialized(), 1);
    assert_eq!(nearest_outcome.skipped_exact_duplicate_rows(), 0);
    assert_eq!(fixture.child_state.inherited_layer_count(), 0);
    let after_all = fixture
        .child_state
        .capture_read_view()
        .expect("after all view");
    assert_visible_row(
        after_all
            .latest(&fixture.child_key)
            .expect("after all latest")
            .as_ref(),
        &rewrite_row_branch(
            &fixture.nearest_duplicate,
            fixture.nearest_source,
            fixture.child,
        )
        .expect("nearest rewrite"),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 1,
        },
    );
    assert_visible_row(
        after_all
            .at_version(&fixture.child_key, CommitVersion::new(3))
            .expect("after all historical")
            .as_ref(),
        &rewrite_row_branch(
            &fixture.farther_history,
            fixture.farther_source,
            fixture.child,
        )
        .expect("farther rewrite"),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
}

#[test]
fn branch_materialization_handles_empty_and_already_materialized_layers() {
    let source = branch_id(95);
    let child = branch_id(96);
    let empty_layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        Vec::new(),
    );
    let mut empty_child = BranchLocalState::empty(child);
    empty_child
        .attach_inherited_layers(vec![empty_layer])
        .expect("attach empty inherited layer");
    let empty_outcome = empty_child
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-empty")
                .expect("empty request"),
        )
        .expect("materialize empty layer");
    assert_eq!(empty_outcome.rows_materialized(), 0);
    assert_eq!(empty_outcome.tables_created(), 0);
    assert_eq!(empty_outcome.inherited_layers_remaining(), 0);
    assert_eq!(empty_child.inherited_layer_count(), 0);
    assert_eq!(empty_child.owned_table_count(), 0);
    assert_eq!(empty_child.max_commit_version(), None);
    assert_eq!(empty_child.timestamp_min(), None);
    assert_eq!(empty_child.timestamp_max(), None);
    assert_eq!(
        empty_child.facts().expect("empty inherited facts"),
        BranchStateFacts::empty(child),
    );

    let materialized_layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Materialized,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "already-materialized-source",
            vec![storage_row_with(
                source,
                b"stale".to_vec(),
                5,
                50,
                Timestamp::EPOCH,
                b"stale".to_vec(),
            )],
        )]],
    );
    let mut materialized_child = BranchLocalState::empty(child);
    materialized_child
        .attach_inherited_layers(vec![materialized_layer])
        .expect("attach materialized layer");
    let before = materialized_child.clone();
    let materialized_outcome = materialized_child
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-stale")
                .expect("materialized request"),
        )
        .expect("already materialized no-op");
    assert_eq!(
        materialized_outcome.recovery(),
        BranchMaterializationRecovery::LayerAlreadyMaterialized,
    );
    assert_eq!(materialized_outcome.rows_materialized(), 0);
    assert_eq!(materialized_outcome.tables_created(), 0);
    assert_eq!(materialized_child, before);

    assert!(matches!(
        BranchMaterializationRequest::new(child, 0, ""),
        Err(BranchRuntimeError::InvalidConfig {
            field: "output_identity_prefix",
            ..
        })
    ));
}

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
