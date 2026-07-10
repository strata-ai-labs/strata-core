fn check_branch_materialization(script: &[u8]) -> Result<MaterializationOutcome, TestkitError> {
    let mut outcome = MaterializationOutcome::default();
    check_materialization_read_parity(script, &mut outcome)?;
    check_materialization_child_owned_immutable_collision(script, &mut outcome)?;
    check_materialization_tombstone_and_ttl(script, &mut outcome)?;
    check_materialization_empty_and_idempotent(script, &mut outcome)?;
    check_invalid_materialization_requests(script, &mut outcome)?;
    Ok(outcome)
}

#[allow(clippy::too_many_lines)]
fn check_materialization_read_parity(
    script: &[u8],
    outcome: &mut MaterializationOutcome,
) -> Result<(), TestkitError> {
    let source = branch_id(script_byte(script, 140));
    let child = branch_id(script_byte(script, 140).wrapping_add(1));
    let visible = storage_row_with(
        source,
        b"generated-materialize-a".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        vec![script_byte(script, 141)],
    )?;
    let historical = storage_row_with(
        source,
        b"generated-materialize-history".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 142)],
    )?;
    let post_fork = storage_row_with(
        source,
        b"generated-materialize-post-fork".to_vec(),
        9,
        15,
        Timestamp::EPOCH,
        b"post-fork".to_vec(),
    )?;
    let exact_duplicate_source = storage_row_with(
        source,
        b"generated-materialize-duplicate".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"duplicate".to_vec(),
    )?;
    let same_internal_key_different_timestamp = storage_row_with(
        source,
        b"generated-materialize-same-key".to_vec(),
        4,
        30,
        Timestamp::EPOCH,
        b"inherited-timestamp".to_vec(),
    )?;
    let layer = branch_inherited_layer_unchecked_for_fork_gate_checks(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "generated-materialize-source",
            vec![
                visible.clone(),
                historical.clone(),
                post_fork,
                exact_duplicate_source.clone(),
                same_internal_key_different_timestamp.clone(),
            ],
        )?]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .map_err(|err| TestkitError::new(format!("materialization attach failed: {err}")))?;
    let child_newer = storage_row_with(
        child,
        b"generated-materialize-history".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        b"child-newer".to_vec(),
    )?;
    child_state
        .append_committed_row(child_newer.clone())
        .map_err(|err| TestkitError::new(format!("materialization child append failed: {err}")))?;
    let exact_duplicate_child = rewrite_row_branch(&exact_duplicate_source, source, child)
        .map_err(|err| {
            TestkitError::new(format!("materialization duplicate rewrite failed: {err}"))
        })?;
    child_state
        .append_committed_row(exact_duplicate_child.clone())
        .map_err(|err| {
            TestkitError::new(format!("materialization duplicate append failed: {err}"))
        })?;
    let child_same_internal_key_later_timestamp = storage_row_with(
        child,
        b"generated-materialize-same-key".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"child-timestamp".to_vec(),
    )?;
    child_state
        .append_committed_row(child_same_internal_key_later_timestamp.clone())
        .map_err(|err| {
            TestkitError::new(format!("materialization timestamp append failed: {err}"))
        })?;
    let visible_rewritten = rewrite_row_branch(&visible, source, child).map_err(|err| {
        TestkitError::new(format!("materialization visible rewrite failed: {err}"))
    })?;
    let historical_rewritten = rewrite_row_branch(&historical, source, child).map_err(|err| {
        TestkitError::new(format!("materialization historical rewrite failed: {err}"))
    })?;
    let same_key_rewritten =
        rewrite_row_branch(&same_internal_key_different_timestamp, source, child).map_err(
            |err| TestkitError::new(format!("materialization same-key rewrite failed: {err}")),
        )?;

    let before = child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("materialization before view failed: {err}")))?;
    let pinned = before.clone();
    let visible_key = physical_key(child, b"generated-materialize-a".to_vec())?;
    let history_key = physical_key(child, b"generated-materialize-history".to_vec())?;
    let timestamp_key = physical_key(child, b"generated-materialize-same-key".to_vec())?;
    let prefix =
        BranchScanBounds::prefix(&physical_key(child, b"generated-materialize-".to_vec())?);
    let range = BranchScanBounds::closed(
        &physical_key(child, b"generated-materialize-a".to_vec())?,
        &physical_key(child, b"generated-materialize-history".to_vec())?,
    )
    .map_err(|err| TestkitError::new(format!("materialization range bounds failed: {err}")))?;

    let before_latest = before
        .latest(&visible_key)
        .map_err(|err| TestkitError::new(format!("materialization before latest failed: {err}")))?
        .map(|row| row.row().clone());
    let before_version = before
        .at_version(&history_key, CommitVersion::new(2))
        .map_err(|err| TestkitError::new(format!("materialization before getv failed: {err}")))?
        .map(|row| row.row().clone());
    let before_timestamp = before
        .read_point(
            &timestamp_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("materialization before as-of failed: {err}")))?
        .map(|row| row.row().clone());
    let before_history_rows = before
        .history(
            &history_key,
            BranchHistoryOptions::all().include_tombstones(true),
        )
        .map_err(|err| TestkitError::new(format!("materialization before history failed: {err}")))?
        .into_iter()
        .map(|row| row.row().clone())
        .collect::<Vec<_>>();
    let before_prefix_keys = visible_user_keys(
        &before
            .scan_prefix(&prefix, BranchReadBound::latest())
            .map_err(|err| {
                TestkitError::new(format!("materialization before prefix failed: {err}"))
            })?,
    );
    let before_range_keys = visible_user_keys(
        &before
            .scan_range(&range, BranchReadBound::latest())
            .map_err(|err| {
                TestkitError::new(format!("materialization before range failed: {err}"))
            })?,
    );

    let materialization: BranchMaterializationOutcome = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "generated-materialize").map_err(
                |err| TestkitError::new(format!("materialization request failed: {err}")),
            )?,
        )
        .map_err(|err| TestkitError::new(format!("materialization failed: {err}")))?;
    outcome.materialization_attempts += 1;
    outcome.successful_materializations += 1;
    outcome.materialized_rows += usize::try_from(materialization.rows_materialized())
        .map_err(|_| TestkitError::new("materialized row count did not fit usize"))?;
    outcome.materialized_tables += materialization.tables_created();
    outcome.skipped_materialization_post_fork_rows +=
        usize::try_from(materialization.skipped_post_fork_rows())
            .map_err(|_| TestkitError::new("skipped post-fork count did not fit usize"))?;
    outcome.skipped_materialization_exact_duplicates +=
        usize::try_from(materialization.skipped_exact_duplicate_rows())
            .map_err(|_| TestkitError::new("skipped duplicate count did not fit usize"))?;
    if materialization.recovery() != BranchMaterializationRecovery::ReplacementVisibleLayerRemoved
        || materialization.rows_materialized() != 3
        || materialization.skipped_post_fork_rows() != 1
        || materialization.skipped_exact_duplicate_rows() != 1
        || child_state.inherited_layer_count() != 0
    {
        return Err(TestkitError::new("materialization outcome facts drifted"));
    }

    let after = child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("materialization after view failed: {err}")))?;
    let mut model = ModelBranch::new(child);
    for row in [
        visible_rewritten,
        historical_rewritten,
        exact_duplicate_child,
        child_newer,
        same_key_rewritten,
        child_same_internal_key_later_timestamp,
    ] {
        model.push(row)?;
    }
    assert_materialization_model_matches(&after, &model, child)?;
    if after
        .latest(&visible_key)
        .map_err(|err| TestkitError::new(format!("materialization after latest failed: {err}")))?
        .map(|row| row.row().clone())
        != before_latest
    {
        return Err(TestkitError::new("materialization latest parity failed"));
    }
    outcome.materialization_latest_read_parity += 1;
    if after
        .at_version(&history_key, CommitVersion::new(2))
        .map_err(|err| TestkitError::new(format!("materialization after getv failed: {err}")))?
        .map(|row| row.row().clone())
        != before_version
    {
        return Err(TestkitError::new("materialization getv parity failed"));
    }
    outcome.materialization_version_read_parity += 1;
    if after
        .read_point(
            &timestamp_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("materialization after as-of failed: {err}")))?
        .map(|row| row.row().clone())
        != before_timestamp
    {
        return Err(TestkitError::new("materialization as-of parity failed"));
    }
    outcome.materialization_timestamp_read_parity += 1;
    let after_history_rows = after
        .history(
            &history_key,
            BranchHistoryOptions::all().include_tombstones(true),
        )
        .map_err(|err| TestkitError::new(format!("materialization after history failed: {err}")))?
        .into_iter()
        .map(|row| row.row().clone())
        .collect::<Vec<_>>();
    if after_history_rows != before_history_rows {
        return Err(TestkitError::new("materialization history parity failed"));
    }
    outcome.materialization_history_read_parity += 1;
    let after_prefix_keys = visible_user_keys(
        &after
            .scan_prefix(&prefix, BranchReadBound::latest())
            .map_err(|err| {
                TestkitError::new(format!("materialization after prefix failed: {err}"))
            })?,
    );
    if after_prefix_keys != before_prefix_keys {
        return Err(TestkitError::new("materialization prefix parity failed"));
    }
    outcome.materialization_prefix_scan_parity += 1;
    let after_range_keys = visible_user_keys(
        &after
            .scan_range(&range, BranchReadBound::latest())
            .map_err(|err| {
                TestkitError::new(format!("materialization after range failed: {err}"))
            })?,
    );
    if after_range_keys != before_range_keys {
        return Err(TestkitError::new("materialization range parity failed"));
    }
    outcome.materialization_range_scan_parity += 1;

    let pinned_row = pinned
        .latest(&visible_key)
        .map_err(|err| TestkitError::new(format!("materialization pinned read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("materialization pinned read missed row"))?;
    if !matches!(pinned_row.source(), BranchRowSource::Inherited { .. }) {
        return Err(TestkitError::new(
            "materialization pinned view lost inherited source",
        ));
    }
    outcome.materialization_pinned_view_isolations += 1;
    Ok(())
}

fn check_materialization_child_owned_immutable_collision(
    script: &[u8],
    outcome: &mut MaterializationOutcome,
) -> Result<(), TestkitError> {
    let source = branch_id(script_byte(script, 146));
    let child = branch_id(script_byte(script, 146).wrapping_add(1));
    let inherited = storage_row_with(
        source,
        b"generated-materialize-owned-collision".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"inherited".to_vec(),
    )?;
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(4),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "generated-materialize-owned-collision-source",
            vec![inherited],
        )?]],
    )?;
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .map_err(|err| {
            TestkitError::new(format!(
                "materialization owned collision attach failed: {err}"
            ))
        })?;
    child_state
        .install_l0_table(branch_owned_table(
            child,
            BranchLevel::ZERO,
            "generated-materialize-owned-collision-child",
            vec![storage_row_with(
                child,
                b"generated-materialize-owned-collision".to_vec(),
                4,
                45,
                Timestamp::EPOCH,
                b"child".to_vec(),
            )?],
        )?)
        .map_err(|err| {
            TestkitError::new(format!(
                "materialization owned collision child install failed: {err}"
            ))
        })?;
    let before = child_state.clone();

    expect_invalid_inherited_layer(
        child_state.materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "generated-materialize-owned-collision")
                .map_err(|err| {
                TestkitError::new(format!(
                    "materialization owned collision request failed: {err}"
                ))
            })?,
        ),
    )?;
    if child_state != before {
        return Err(TestkitError::new(
            "materialization owned collision mutated state",
        ));
    }
    outcome.invalid_materialization_rejections += 1;
    Ok(())
}

fn assert_materialization_model_matches(
    view: &BranchReadView,
    model: &ModelBranch,
    branch: BranchId,
) -> Result<(), TestkitError> {
    for (key, bound, label) in [
        (
            physical_key(branch, b"generated-materialize-a".to_vec())?,
            BranchReadBound::latest(),
            "latest",
        ),
        (
            physical_key(branch, b"generated-materialize-history".to_vec())?,
            BranchReadBound::latest(),
            "history latest",
        ),
        (
            physical_key(branch, b"generated-materialize-history".to_vec())?,
            BranchReadBound::at_version(CommitVersion::new(2)),
            "history version",
        ),
        (
            physical_key(branch, b"generated-materialize-same-key".to_vec())?,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            "timestamp",
        ),
    ] {
        assert_model_point(view, model, &key, bound, label)?;
    }

    let history_key = physical_key(branch, b"generated-materialize-history".to_vec())?;
    let actual_history = view
        .history(
            &history_key,
            BranchHistoryOptions::all().include_tombstones(true),
        )
        .map_err(|err| TestkitError::new(format!("materialization model history failed: {err}")))?
        .into_iter()
        .map(|row| row.row().clone())
        .collect::<Vec<_>>();
    if actual_history != model.history(&history_key) {
        return Err(TestkitError::new("materialization model history mismatch"));
    }

    let prefix =
        BranchScanBounds::prefix(&physical_key(branch, b"generated-materialize-".to_vec())?);
    let actual_prefix = view
        .scan_prefix(&prefix, BranchReadBound::latest())
        .map_err(|err| TestkitError::new(format!("materialization model prefix failed: {err}")))?
        .into_iter()
        .map(|row| row.row().clone())
        .collect::<Vec<_>>();
    let expected_prefix = materialization_model_scan(
        model,
        b"generated-materialize-",
        None,
        BranchReadBound::latest(),
    );
    if actual_prefix != expected_prefix {
        return Err(TestkitError::new("materialization model prefix mismatch"));
    }

    let lower = b"generated-materialize-a".as_slice();
    let upper = b"generated-materialize-history".as_slice();
    let range = BranchScanBounds::closed(
        &physical_key(branch, lower.to_vec())?,
        &physical_key(branch, upper.to_vec())?,
    )
    .map_err(|err| {
        TestkitError::new(format!("materialization model range bounds failed: {err}"))
    })?;
    let actual_range = view
        .scan_range(&range, BranchReadBound::latest())
        .map_err(|err| TestkitError::new(format!("materialization model range failed: {err}")))?
        .into_iter()
        .map(|row| row.row().clone())
        .collect::<Vec<_>>();
    let expected_range =
        materialization_model_scan(model, lower, Some(upper), BranchReadBound::latest());
    if actual_range != expected_range {
        return Err(TestkitError::new("materialization model range mismatch"));
    }
    Ok(())
}

fn materialization_model_scan(
    model: &ModelBranch,
    lower_or_prefix: &[u8],
    upper: Option<&[u8]>,
    bound: BranchReadBound,
) -> Vec<StorageRow> {
    let mut keys = Vec::<PhysicalKey>::new();
    for row in &model.rows {
        let user_key = row.physical_key().user_key();
        let matches = if let Some(upper) = upper {
            lower_or_prefix <= user_key && user_key <= upper
        } else {
            user_key.starts_with(lower_or_prefix)
        };
        if matches && !keys.iter().any(|key| key == row.physical_key()) {
            keys.push(row.physical_key().clone());
        }
    }
    keys.sort_by(|left, right| left.user_key().cmp(right.user_key()));

    let mut rows = Vec::new();
    for key in keys {
        if let Some(row) = model.visible(&key, bound) {
            rows.push(row);
        }
    }
    rows
}

fn check_materialization_tombstone_and_ttl(
    script: &[u8],
    outcome: &mut MaterializationOutcome,
) -> Result<(), TestkitError> {
    let source = branch_id(script_byte(script, 148));
    let child = branch_id(script_byte(script, 148).wrapping_add(1));
    let expired = storage_row_with(
        source,
        b"generated-materialize-expired".to_vec(),
        2,
        20,
        Timestamp::from_micros(25),
        vec![script_byte(script, 149)],
    )?;
    let deleted_put = storage_row_with(
        source,
        b"generated-materialize-deleted".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 150)],
    )?;
    let deleting_tombstone =
        tombstone_row(source, b"generated-materialize-deleted".to_vec(), 3, 30)?;
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(3),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "generated-materialize-ttl-source",
            vec![expired.clone(), deleted_put, deleting_tombstone],
        )?]],
    )?;
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .map_err(|err| TestkitError::new(format!("materialization ttl attach failed: {err}")))?;
    let materialization = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "generated-materialize-ttl").map_err(
                |err| TestkitError::new(format!("materialization ttl request failed: {err}")),
            )?,
        )
        .map_err(|err| TestkitError::new(format!("materialization ttl failed: {err}")))?;
    outcome.materialization_attempts += 1;
    outcome.successful_materializations += 1;
    outcome.materialized_rows += usize::try_from(materialization.rows_materialized())
        .map_err(|_| TestkitError::new("ttl materialized rows did not fit usize"))?;
    outcome.materialized_tables += materialization.tables_created();

    let view = child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("materialization ttl view failed: {err}")))?;
    check_materialized_ttl_preserved(&view, &expired, source, child, outcome)?;
    check_materialized_tombstone_preserved(&view, child, outcome)
}

fn check_materialized_ttl_preserved(
    view: &BranchReadView,
    expired: &StorageRow,
    source: BranchId,
    child: BranchId,
    outcome: &mut MaterializationOutcome,
) -> Result<(), TestkitError> {
    let expired_key = physical_key(child, b"generated-materialize-expired".to_vec())?;
    let before_expiry = view
        .read_point(
            &expired_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(24)),
        )
        .map_err(|err| TestkitError::new(format!("materialization ttl before read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("materialization ttl row missing before expiry"))?;
    let expected_expired = rewrite_row_branch(expired, source, child)
        .map_err(|err| TestkitError::new(format!("materialization ttl rewrite failed: {err}")))?;
    if before_expiry.row() != &expected_expired {
        return Err(TestkitError::new(
            "materialization ttl changed expired row facts",
        ));
    }
    if !matches!(
        before_expiry.source(),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            ..
        }
    ) {
        return Err(TestkitError::new(
            "materialization ttl row did not move to owned table",
        ));
    }
    if view
        .read_point(
            &expired_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
        )
        .map_err(|err| TestkitError::new(format!("materialization ttl exact read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "materialization ttl failed to suppress at expiry",
        ));
    }
    outcome.materialization_ttl_preservations += 1;
    Ok(())
}

fn check_materialized_tombstone_preserved(
    view: &BranchReadView,
    child: BranchId,
    outcome: &mut MaterializationOutcome,
) -> Result<(), TestkitError> {
    let deleted_key = physical_key(child, b"generated-materialize-deleted".to_vec())?;
    if view
        .read_point(
            &deleted_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("materialization tombstone read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "materialization tombstone failed to suppress put",
        ));
    }
    let history_rows = view
        .history(
            &deleted_key,
            BranchHistoryOptions::all().include_tombstones(true),
        )
        .map_err(|err| {
            TestkitError::new(format!("materialization tombstone history failed: {err}"))
        })?;
    if history_versions(&history_rows) != vec![3, 1] {
        return Err(TestkitError::new(
            "materialization tombstone history drifted",
        ));
    }
    if history_rows.iter().any(|row| {
        !matches!(
            row.source(),
            BranchRowSource::OwnedTable {
                level: BranchLevel::ZERO,
                ..
            }
        )
    }) {
        return Err(TestkitError::new(
            "materialization tombstone history did not move to owned table",
        ));
    }
    outcome.materialization_tombstone_preservations += 1;
    Ok(())
}

fn check_materialization_empty_and_idempotent(
    script: &[u8],
    outcome: &mut MaterializationOutcome,
) -> Result<(), TestkitError> {
    let source = branch_id(script_byte(script, 145));
    let child = branch_id(script_byte(script, 145).wrapping_add(1));
    let empty_layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        Vec::new(),
    )?;
    let mut empty_state = BranchLocalState::empty(child);
    empty_state
        .attach_inherited_layers(vec![empty_layer])
        .map_err(|err| TestkitError::new(format!("empty materialization attach failed: {err}")))?;
    let empty = empty_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "generated-materialize-empty").map_err(
                |err| TestkitError::new(format!("empty materialization request failed: {err}")),
            )?,
        )
        .map_err(|err| TestkitError::new(format!("empty materialization failed: {err}")))?;
    outcome.materialization_attempts += 1;
    outcome.successful_materializations += 1;
    if empty.rows_materialized() != 0
        || empty.tables_created() != 0
        || empty_state.inherited_layer_count() != 0
    {
        return Err(TestkitError::new("empty materialization drifted"));
    }
    outcome.empty_materializations += 1;

    let materialized_layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Materialized,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "generated-materialize-already",
            vec![storage_row_with(
                source,
                b"generated-materialized-stale".to_vec(),
                5,
                50,
                Timestamp::EPOCH,
                vec![script_byte(script, 146)],
            )?],
        )?]],
    )?;
    let mut materialized_state = BranchLocalState::empty(child);
    materialized_state
        .attach_inherited_layers(vec![materialized_layer])
        .map_err(|err| {
            TestkitError::new(format!("idempotent materialization attach failed: {err}"))
        })?;
    let before = materialized_state.clone();
    let retry = materialized_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "generated-materialize-retry").map_err(
                |err| TestkitError::new(format!("retry materialization request failed: {err}")),
            )?,
        )
        .map_err(|err| TestkitError::new(format!("retry materialization failed: {err}")))?;
    outcome.materialization_attempts += 1;
    if retry.recovery() != BranchMaterializationRecovery::LayerAlreadyMaterialized
        || retry.rows_materialized() != 0
        || materialized_state != before
    {
        return Err(TestkitError::new("idempotent materialization drifted"));
    }
    outcome.idempotent_materialization_retries += 1;
    Ok(())
}

fn check_invalid_materialization_requests(
    script: &[u8],
    outcome: &mut MaterializationOutcome,
) -> Result<(), TestkitError> {
    let source = branch_id(script_byte(script, 147));
    let child = branch_id(script_byte(script, 147).wrapping_add(1));
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        Vec::new(),
    )?;
    let mut state = BranchLocalState::empty(child);
    state.attach_inherited_layers(vec![layer]).map_err(|err| {
        TestkitError::new(format!("invalid materialization attach failed: {err}"))
    })?;
    match state.materialize_inherited_layer(
        &BranchMaterializationRequest::new(source, 0, "generated-materialize-wrong-branch")
            .map_err(|err| TestkitError::new(format!("wrong-branch request failed: {err}")))?,
    ) {
        Err(BranchRuntimeError::InvalidBranchState { .. }) => {}
        Err(err) => {
            return Err(TestkitError::new(format!(
                "wrong-branch materialization returned wrong error: {err}",
            )))
        }
        Ok(_) => return Err(TestkitError::new("wrong-branch materialization succeeded")),
    }
    outcome.materialization_attempts += 1;
    outcome.invalid_materialization_rejections += 1;

    expect_invalid_config(BranchMaterializationRequest::new(
        child,
        0,
        "generated/materialize",
    ))?;
    outcome.invalid_materialization_rejections += 1;
    Ok(())
}
