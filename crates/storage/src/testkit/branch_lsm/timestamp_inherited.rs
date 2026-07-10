fn check_inherited_timestamp_reads(
    script: &[u8],
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let source = branch_id(script_byte(script, 121));
    let child = branch_id(script_byte(script, 121).wrapping_add(1));
    let visible = storage_row_with(
        source,
        b"generated-inherited-time".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        vec![script_byte(script, 122)],
    )?;
    let future_timestamp = storage_row_with(
        source,
        b"generated-inherited-time".to_vec(),
        4,
        70,
        Timestamp::EPOCH,
        b"future".to_vec(),
    )?;
    let after_fork_old_timestamp = storage_row_with(
        source,
        b"generated-inherited-time".to_vec(),
        7,
        20,
        Timestamp::EPOCH,
        b"post-fork".to_vec(),
    )?;
    let layer = branch_inherited_layer_unchecked_for_fork_gate_checks(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "generated-inherited-time",
            vec![visible.clone(), future_timestamp, after_fork_old_timestamp],
        )?]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .map_err(|err| TestkitError::new(format!("timestamp inherited attach failed: {err}")))?;
    let view = child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp inherited view failed: {err}")))?
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    let child_key = physical_key(child, b"generated-inherited-time".to_vec())?;
    let inherited = view
        .read_point(
            &child_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp inherited read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("timestamp inherited read missed row"))?;
    let expected = rewrite_row_branch(&visible, source, child)
        .map_err(|err| TestkitError::new(format!("timestamp inherited rewrite failed: {err}")))?;
    if inherited.row() != &expected {
        return Err(TestkitError::new("timestamp inherited row drifted"));
    }
    outcome.inherited_timestamp_reads += 1;
    outcome.inherited_timestamp_point_reads += 1;

    if view
        .read_point(
            &child_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp fork gate read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "timestamp inherited read exposed post-fork row with old timestamp",
        ));
    }
    outcome.inherited_timestamp_fork_gates += 1;
    Ok(())
}

fn check_inherited_timestamp_scans(
    script: &[u8],
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let source = branch_id(script_byte(script, 128));
    let child = branch_id(script_byte(script, 128).wrapping_add(1));
    let visible = storage_row_with(
        source,
        b"generated-inherited-scan-a".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        vec![script_byte(script, 129)],
    )?;
    let future_timestamp = storage_row_with(
        source,
        b"generated-inherited-scan-b".to_vec(),
        4,
        70,
        Timestamp::EPOCH,
        b"future".to_vec(),
    )?;
    let after_fork_old_timestamp = storage_row_with(
        source,
        b"generated-inherited-scan-c".to_vec(),
        7,
        20,
        Timestamp::EPOCH,
        b"post-fork".to_vec(),
    )?;
    let layer = branch_inherited_layer_unchecked_for_fork_gate_checks(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "generated-inherited-scan",
            vec![visible.clone(), future_timestamp, after_fork_old_timestamp],
        )?]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .map_err(|err| {
            TestkitError::new(format!("timestamp inherited scan attach failed: {err}"))
        })?;
    let view = child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp inherited scan view failed: {err}")))?
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    let expected = rewrite_row_branch(&visible, source, child)
        .map_err(|err| TestkitError::new(format!("timestamp scan rewrite failed: {err}")))?;
    let prefix =
        BranchScanBounds::prefix(&physical_key(child, b"generated-inherited-scan-".to_vec())?);
    let prefix_rows = view
        .scan_prefix(
            &prefix,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp inherited prefix failed: {err}")))?;
    if visible_user_keys(&prefix_rows) != vec![expected.physical_key().user_key().to_vec()]
        || prefix_rows.first().map(BranchVisibleRow::row) != Some(&expected)
    {
        return Err(TestkitError::new(
            "timestamp inherited prefix scan did not rewrite before grouping",
        ));
    }
    outcome.timestamp_scan_reads += 1;
    outcome.timestamp_prefix_scans += 1;
    outcome.inherited_timestamp_scan_reads += 1;

    let range = BranchScanBounds::closed(
        &physical_key(child, b"generated-inherited-scan-a".to_vec())?,
        &physical_key(child, b"generated-inherited-scan-c".to_vec())?,
    )
    .map_err(|err| TestkitError::new(format!("timestamp inherited bounds failed: {err}")))?;
    let range_rows = view
        .scan_range(
            &range,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp inherited range failed: {err}")))?;
    if range_rows.len() != 1 || range_rows[0].row() != &expected {
        return Err(TestkitError::new("timestamp inherited range scan drifted"));
    }
    outcome.timestamp_scan_reads += 1;
    outcome.timestamp_range_scans += 1;
    outcome.inherited_timestamp_scan_reads += 1;
    Ok(())
}

fn check_inherited_timestamp_local_shadows_and_ties(
    script: &[u8],
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let (mut child_state, fixture) = inherited_timestamp_shadow_fixture(script)?;
    check_inherited_timestamp_nearest_tie(&child_state, &fixture, outcome)?;
    check_inherited_timestamp_child_put_shadow(&mut child_state, &fixture, outcome)?;
    check_inherited_timestamp_child_tombstone_shadow(&mut child_state, &fixture, outcome)?;
    Ok(())
}

struct InheritedTimestampShadowFixture {
    child: BranchId,
    nearest_source: BranchId,
    key: Vec<u8>,
    child_key: PhysicalKey,
    expected_nearest: StorageRow,
}

fn inherited_timestamp_shadow_fixture(
    script: &[u8],
) -> Result<(BranchLocalState, InheritedTimestampShadowFixture), TestkitError> {
    let nearest_source = branch_id(script_byte(script, 133));
    let farther_source = branch_id(script_byte(script, 133).wrapping_add(1));
    let child = branch_id(script_byte(script, 133).wrapping_add(2));
    let key = b"generated-inherited-shadow".to_vec();
    let nearest = storage_row_with(
        nearest_source,
        key.clone(),
        3,
        30,
        Timestamp::EPOCH,
        vec![script_byte(script, 134)],
    )?;
    let farther = storage_row_with(
        farther_source,
        key.clone(),
        3,
        30,
        Timestamp::EPOCH,
        b"farther".to_vec(),
    )?;
    let nearest_layer = branch_inherited_layer(
        nearest_source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            nearest_source,
            BranchLevel::ZERO,
            "generated-nearest-time-tie",
            vec![nearest.clone()],
        )?]],
    )?;
    let farther_layer = branch_inherited_layer(
        farther_source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            farther_source,
            BranchLevel::ZERO,
            "generated-farther-time-tie",
            vec![farther],
        )?]],
    )?;
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![nearest_layer, farther_layer])
        .map_err(|err| {
            TestkitError::new(format!("timestamp inherited shadow attach failed: {err}"))
        })?;
    let expected_nearest = rewrite_row_branch(&nearest, nearest_source, child)
        .map_err(|err| TestkitError::new(format!("nearest inherited rewrite failed: {err}")))?;
    Ok((
        child_state,
        InheritedTimestampShadowFixture {
            child,
            nearest_source,
            key: key.clone(),
            child_key: physical_key(child, key)?,
            expected_nearest,
        },
    ))
}

fn check_inherited_timestamp_nearest_tie(
    child_state: &BranchLocalState,
    fixture: &InheritedTimestampShadowFixture,
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let inherited = child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp inherited tie view failed: {err}")))?
        .read_point(
            &fixture.child_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp inherited tie read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("timestamp inherited tie missed row"))?;
    if inherited.row() != &fixture.expected_nearest
        || inherited.source()
            != (BranchRowSource::Inherited {
                source_branch_id: fixture.nearest_source,
                layer_index: 0,
            })
    {
        return Err(TestkitError::new(
            "nearest inherited timestamp layer did not win exact tie",
        ));
    }
    outcome.inherited_timestamp_reads += 1;
    outcome.inherited_timestamp_point_reads += 1;
    outcome.inherited_timestamp_nearest_ties += 1;
    Ok(())
}

fn check_inherited_timestamp_child_put_shadow(
    child_state: &mut BranchLocalState,
    fixture: &InheritedTimestampShadowFixture,
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let child_put = storage_row_with(
        fixture.child,
        fixture.key.clone(),
        4,
        35,
        Timestamp::EPOCH,
        b"child-put".to_vec(),
    )?;
    child_state
        .append_committed_row(child_put.clone())
        .map_err(|err| TestkitError::new(format!("timestamp child put failed: {err}")))?;
    let put_read = child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp child put view failed: {err}")))?
        .read_point(
            &fixture.child_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp child put read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("timestamp child put missed row"))?;
    if put_read.row() != &child_put || put_read.source() != BranchRowSource::Active {
        return Err(TestkitError::new(
            "child-local put did not shadow inherited timestamp row",
        ));
    }
    outcome.inherited_timestamp_reads += 1;
    outcome.inherited_timestamp_point_reads += 1;
    outcome.inherited_timestamp_child_put_shadows += 1;
    Ok(())
}

fn check_inherited_timestamp_child_tombstone_shadow(
    child_state: &mut BranchLocalState,
    fixture: &InheritedTimestampShadowFixture,
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    child_state
        .append_committed_row(tombstone_row(fixture.child, fixture.key.clone(), 5, 45)?)
        .map_err(|err| TestkitError::new(format!("timestamp child tombstone failed: {err}")))?;
    if child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp child tombstone view failed: {err}")))?
        .read_point(
            &fixture.child_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(50)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp child tombstone read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "child-local tombstone did not shadow inherited timestamp row",
        ));
    }
    outcome.inherited_timestamp_reads += 1;
    outcome.inherited_timestamp_point_reads += 1;
    outcome.inherited_timestamp_child_tombstone_shadows += 1;
    Ok(())
}

fn check_pinned_timestamp_views(
    script: &[u8],
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 130));
    let mut state = BranchLocalState::empty(branch);
    let point = storage_row_with(
        branch,
        b"generated-pinned-ts".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 131)],
    )?;
    let scan = storage_row_with(
        branch,
        b"generated-pinned-scan-a".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        vec![script_byte(script, 132)],
    )?;
    for row in [point.clone(), scan.clone()] {
        state
            .append_committed_row(row)
            .map_err(|err| TestkitError::new(format!("pinned timestamp append failed: {err}")))?;
    }
    let pinned = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("pinned timestamp view failed: {err}")))?;
    state
        .append_committed_row(storage_row_with(
            branch,
            b"generated-pinned-ts".to_vec(),
            3,
            30,
            Timestamp::EPOCH,
            b"later-point".to_vec(),
        )?)
        .map_err(|err| TestkitError::new(format!("later timestamp append failed: {err}")))?;
    state
        .append_committed_row(storage_row_with(
            branch,
            b"generated-pinned-scan-b".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            b"later-scan".to_vec(),
        )?)
        .map_err(|err| TestkitError::new(format!("later scan append failed: {err}")))?;
    match state.rotate_active() {
        BranchRotationOutcome::Rotated { .. } => {}
        outcome @ BranchRotationOutcome::Skipped { .. } => {
            return Err(TestkitError::new(format!(
                "pinned timestamp rotation skipped: {outcome:?}",
            )))
        }
    }
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "generated-pinned-owned",
            vec![storage_row_with(
                branch,
                b"generated-pinned-scan-c".to_vec(),
                5,
                50,
                Timestamp::EPOCH,
                b"owned".to_vec(),
            )?],
        )?)
        .map_err(|err| TestkitError::new(format!("pinned owned install failed: {err}")))?;

    let point_row = pinned
        .read_point(
            &physical_key(branch, b"generated-pinned-ts".to_vec())?,
            BranchReadBound::at_timestamp(Timestamp::from_micros(35)),
        )
        .map_err(|err| TestkitError::new(format!("pinned timestamp point failed: {err}")))?
        .ok_or_else(|| TestkitError::new("pinned timestamp point missed row"))?;
    if point_row.row() != &point {
        return Err(TestkitError::new(
            "pinned timestamp point saw later mutation",
        ));
    }
    let scan_rows = pinned
        .scan_prefix(
            &BranchScanBounds::prefix(&physical_key(branch, b"generated-pinned-scan-".to_vec())?),
            BranchReadBound::at_timestamp(Timestamp::from_micros(60)),
        )
        .map_err(|err| TestkitError::new(format!("pinned timestamp scan failed: {err}")))?;
    if visible_user_keys(&scan_rows) != vec![scan.physical_key().user_key().to_vec()] {
        return Err(TestkitError::new(
            "pinned timestamp scan saw later mutation",
        ));
    }
    outcome.pinned_timestamp_view_isolations += 1;
    Ok(())
}

fn check_timestamp_coverage(
    script: &[u8],
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 123));
    let mut state = BranchLocalState::empty(branch);
    let row = storage_row_with(
        branch,
        b"generated-coverage".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        vec![script_byte(script, 124)],
    )?;
    state
        .append_committed_row(row.clone())
        .map_err(|err| TestkitError::new(format!("coverage append failed: {err}")))?;
    let key = physical_key(branch, b"generated-coverage".to_vec())?;
    let canonical = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("coverage canonical view failed: {err}")))?;
    match canonical.read_point(
        &key,
        BranchReadBound::at_timestamp(Timestamp::from_micros(49)),
    ) {
        Err(err) => {
            return Err(TestkitError::new(format!(
                "unknown coverage returned unexpected error: {err}",
            )))
        }
        Ok(Some(_)) => {
            return Err(TestkitError::new(
                "unknown coverage returned a row outside the timestamp bound",
            ))
        }
        Ok(None) => {}
    }
    outcome.unknown_timestamp_coverage_reads += 1;

    let complete_since = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("coverage proof view failed: {err}")))?
        .with_timestamp_coverage(BranchTimestampCoverage::complete_since(
            Timestamp::from_micros(50),
        ));
    match complete_since.read_point(
        &key,
        BranchReadBound::at_timestamp(Timestamp::from_micros(49)),
    ) {
        Err(BranchRuntimeError::InsufficientTimestampHistory {
            branch_id,
            requested_timestamp,
            earliest_available_timestamp: Some(earliest),
            source: BranchTimestampHistorySource::Combined,
        }) if branch_id == branch
            && requested_timestamp == Timestamp::from_micros(49)
            && earliest == Timestamp::from_micros(50) => {}
        Err(err) => {
            return Err(TestkitError::new(format!(
                "coverage proof returned wrong error: {err}",
            )))
        }
        Ok(_) => {
            return Err(TestkitError::new(
                "coverage proof accepted insufficient timestamp",
            ))
        }
    }
    outcome.insufficient_timestamp_history_rejections += 1;

    let at_floor = complete_since
        .read_point(
            &key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(50)),
        )
        .map_err(|err| TestkitError::new(format!("coverage floor read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("coverage floor read missed row"))?;
    if at_floor.row() != &row {
        return Err(TestkitError::new("coverage floor read selected wrong row"));
    }
    outcome.timestamp_point_reads += 1;
    Ok(())
}
