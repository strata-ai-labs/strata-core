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

