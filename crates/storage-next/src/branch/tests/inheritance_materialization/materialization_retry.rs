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
    let extras =
        crate::table::TableSummaryExtras::from_rows(reader.rows()).expect("table summary extras");
    let replacement = BranchOwnedTable::new_materialization_replacement(
        child,
        descriptor,
        reader,
        extras,
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
fn branch_materialization_retry_continues_after_partial_replacement_install() {
    let source = branch_id(113);
    let child = branch_id(114);
    let first_chunk_len = 4_096usize;
    let source_rows = (0..=first_chunk_len)
        .map(|index| {
            storage_row_with(
                source,
                format!("materialize-partial-{index:04}").into_bytes(),
                u64::try_from(index + 1).expect("version fits"),
                u64::try_from(index + 1).expect("timestamp fits"),
                Timestamp::EPOCH,
                format!("source-{index}").into_bytes(),
            )
        })
        .collect::<Vec<_>>();
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(u64::try_from(source_rows.len()).expect("fork fits")),
        InheritedLayerStatus::Active,
        vec![vec![
            branch_owned_table(
                source,
                BranchLevel::ZERO,
                "materialize-partial-source-a",
                source_rows[..first_chunk_len].to_vec(),
            ),
            branch_owned_table(
                source,
                BranchLevel::ZERO,
                "materialize-partial-source-b",
                source_rows[first_chunk_len..].to_vec(),
            ),
        ]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach partial retry layer");
    let rewritten_first_chunk = source_rows[..first_chunk_len]
        .iter()
        .map(|row| rewrite_row_branch(row, source, child).expect("rewrite first chunk"))
        .collect::<Vec<_>>();
    let reader = immutable_reader("materialize-partial-layer-0-table-0", rewritten_first_chunk);
    let descriptor = branch_table_descriptor(BranchLevel::ZERO, &reader);
    let extras =
        crate::table::TableSummaryExtras::from_rows(reader.rows()).expect("table summary extras");
    let replacement = BranchOwnedTable::new_materialization_replacement(
        child,
        descriptor,
        reader,
        extras,
        BranchMaterializationSource::new(
            source,
            CommitVersion::new(u64::try_from(source_rows.len()).expect("fork fits")),
        ),
    )
    .expect("partial replacement table");
    child_state
        .install_l0_table(replacement)
        .expect("preinstall first partial replacement");

    let outcome = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialize-partial")
                .expect("partial request"),
        )
        .expect("retry partial materialization");

    assert_eq!(
        outcome.recovery(),
        BranchMaterializationRecovery::ReplacementVisibleLayerRemoved,
    );
    assert_eq!(
        outcome.rows_materialized(),
        u64::try_from(source_rows.len()).expect("rows fit")
    );
    assert_eq!(outcome.tables_created(), 1);
    assert_eq!(outcome.skipped_exact_duplicate_rows(), 4_096);
    assert_eq!(outcome.inherited_layers_remaining(), 0);
    assert_eq!(outcome.replacement_owned_table_count(), 2);
    assert_eq!(child_state.inherited_layer_count(), 0);

    let last_row = rewrite_row_branch(source_rows.last().expect("last source row"), source, child)
        .expect("rewrite last row");
    assert_visible_row(
        child_state
            .capture_read_view()
            .expect("partial retry view")
            .latest(last_row.physical_key())
            .expect("partial retry latest")
            .as_ref(),
        &last_row,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 1,
        },
    );
}

fn materialization_index_gap_rows(source: BranchId) -> Vec<StorageRow> {
    (0..4_097usize)
        .map(|index| {
            storage_row_with(
                source,
                format!("materialize-index-gap-{index:04}").into_bytes(),
                u64::try_from(index + 1).expect("version fits"),
                u64::try_from(index + 1).expect("timestamp fits"),
                Timestamp::EPOCH,
                format!("source-{index}").into_bytes(),
            )
        })
        .collect()
}

fn preinstall_materialization_replacement(
    child_state: &mut BranchLocalState,
    child: BranchId,
    identity: &str,
    materialization_source: BranchMaterializationSource,
    rows: Vec<StorageRow>,
) {
    let reader = immutable_reader(identity, rows);
    let descriptor = branch_table_descriptor(BranchLevel::ZERO, &reader);
    let extras =
        crate::table::TableSummaryExtras::from_rows(reader.rows()).expect("table summary extras");
    child_state
        .install_l0_table(
            BranchOwnedTable::new_materialization_replacement(
                child,
                descriptor,
                reader,
                extras,
                materialization_source,
            )
            .expect("materialization replacement"),
        )
        .expect("preinstall replacement");
}

#[test]
fn branch_materialization_retry_skips_over_noncontiguous_replacement_indices() {
    let source = branch_id(117);
    let child = branch_id(118);
    let source_rows = materialization_index_gap_rows(source);
    let fork_version = CommitVersion::new(u64::try_from(source_rows.len()).expect("fork fits"));
    let layer = branch_inherited_layer(
        source,
        fork_version,
        InheritedLayerStatus::Active,
        vec![vec![
            branch_owned_table(
                source,
                BranchLevel::ZERO,
                "materialize-index-gap-source-a",
                source_rows[..4_096].to_vec(),
            ),
            branch_owned_table(
                source,
                BranchLevel::ZERO,
                "materialize-index-gap-source-b",
                source_rows[4_096..].to_vec(),
            ),
        ]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach gapped retry layer");
    let materialization_source = BranchMaterializationSource::new(source, fork_version);
    let first_rewritten =
        rewrite_row_branch(&source_rows[0], source, child).expect("rewrite first row");
    preinstall_materialization_replacement(
        &mut child_state,
        child,
        "materialize-gap-compacted",
        materialization_source,
        vec![first_rewritten],
    );

    let second_rewritten =
        rewrite_row_branch(&source_rows[1], source, child).expect("rewrite second row");
    preinstall_materialization_replacement(
        &mut child_state,
        child,
        "materialize-gap-layer-0-table-2",
        materialization_source,
        vec![second_rewritten],
    );

    let outcome = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialize-gap")
                .expect("gapped request"),
        )
        .expect("gapped retry materialization");

    assert_eq!(
        outcome.recovery(),
        BranchMaterializationRecovery::ReplacementVisibleLayerRemoved,
    );
    assert_eq!(
        outcome.rows_materialized(),
        u64::try_from(source_rows.len()).expect("rows fit"),
    );
    assert_eq!(outcome.tables_created(), 1);
    assert_eq!(outcome.skipped_exact_duplicate_rows(), 2);
    assert_eq!(outcome.replacement_owned_table_count(), 3);
    assert_eq!(child_state.inherited_layer_count(), 0);

    let last_row = rewrite_row_branch(source_rows.last().expect("last source row"), source, child)
        .expect("rewrite last row");
    assert_visible_row(
        child_state
            .capture_read_view()
            .expect("gapped retry view")
            .latest(last_row.physical_key())
            .expect("gapped retry latest")
            .as_ref(),
        &last_row,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 2,
        },
    );
}

#[test]
fn branch_materialization_stable_source_retry_is_idempotent_after_layer_removal() {
    let source = branch_id(115);
    let child = branch_id(116);
    let fork_version = CommitVersion::new(4);
    let source_row = storage_row_with(
        source,
        b"materialize-stable-retry".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"source".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        fork_version,
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materialize-stable-retry-source",
            vec![source_row.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach stable retry layer");
    child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialize-stable-retry")
                .expect("initial request"),
        )
        .expect("initial materialization");

    let before_retry = child_state.clone();
    let retry = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new_for_source(
                child,
                0,
                source,
                fork_version,
                "materialize-stable-retry",
            )
            .expect("stable retry request"),
        )
        .expect("stable retry after layer removal");

    assert_eq!(
        retry.recovery(),
        BranchMaterializationRecovery::LayerAlreadyMaterialized,
    );
    assert_eq!(retry.rows_materialized(), 1);
    assert_eq!(retry.tables_created(), 0);
    assert_eq!(retry.inherited_layers_remaining(), 0);
    assert_eq!(retry.replacement_owned_table_count(), 1);
    assert_eq!(child_state, before_retry);
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
    assert!(matches!(
        child_state.materialize_inherited_layer(
            &BranchMaterializationRequest::new_for_source(
                child,
                0,
                branch_id(105),
                CommitVersion::new(5),
                "materialized-wrong-source",
            )
            .expect("wrong source request"),
        ),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. })
    ));
    assert_eq!(
        child_state, before,
        "wrong-source materialization must not mutate state",
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
fn branch_materialization_handle_marks_source_reachability_before_replacement() {
    let source = branch_id(103);
    let child = branch_id(104);
    let row = storage_row_with(
        source,
        b"materialization-handle".to_vec(),
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
            "materialization-handle-source",
            vec![row],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach active inherited layer");

    let (handle, snapshot) = child_state
        .mark_inherited_layer_materializing(0)
        .expect("mark materializing");
    assert_eq!(handle.child_branch_id(), child);
    assert_eq!(handle.source_branch_id(), source);
    assert_eq!(handle.fork_version(), CommitVersion::new(5));
    assert_eq!(handle.layer_index(), 0);
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
    assert_eq!(retry, (handle, snapshot.clone()));
    let request = BranchMaterializationRequest::from_handle(handle, "materialization-handle")
        .expect("request from handle");
    assert_eq!(request.child_branch_id(), child);
    assert_eq!(request.layer_index(), 0);
    assert_eq!(
        request.materialization_source(),
        Some(BranchMaterializationSource::new(
            source,
            CommitVersion::new(5)
        )),
    );
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
