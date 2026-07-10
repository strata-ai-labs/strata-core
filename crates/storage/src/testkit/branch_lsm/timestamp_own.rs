fn check_branch_timestamp_visibility(script: &[u8]) -> Result<TimestampOutcome, TestkitError> {
    let mut outcome = TimestampOutcome::default();
    check_timestamp_point_reads(script, &mut outcome)?;
    check_timestamp_frozen_and_owned_point_reads(script, &mut outcome)?;
    check_timestamp_ttl(script, &mut outcome)?;
    check_timestamp_tombstones(script, &mut outcome)?;
    check_timestamp_scans(script, &mut outcome)?;
    check_inherited_timestamp_reads(script, &mut outcome)?;
    check_inherited_timestamp_scans(script, &mut outcome)?;
    check_inherited_timestamp_local_shadows_and_ties(script, &mut outcome)?;
    check_pinned_timestamp_views(script, &mut outcome)?;
    check_timestamp_coverage(script, &mut outcome)?;
    Ok(outcome)
}

fn check_timestamp_point_reads(
    script: &[u8],
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 110));
    let mut state = BranchLocalState::empty(branch);
    let key = b"generated-as-of".to_vec();
    let older = storage_row_with(
        branch,
        key.clone(),
        7,
        80,
        Timestamp::EPOCH,
        vec![script_byte(script, 111)],
    )?;
    let highest_version = storage_row_with(
        branch,
        key.clone(),
        10,
        100,
        Timestamp::EPOCH,
        vec![script_byte(script, 112)],
    )?;
    let lower_version_later_timestamp = storage_row_with(
        branch,
        key.clone(),
        8,
        120,
        Timestamp::EPOCH,
        vec![script_byte(script, 113)],
    )?;
    for row in [
        older.clone(),
        highest_version.clone(),
        lower_version_later_timestamp,
    ] {
        append_expect_put(&mut state, &row)?;
    }
    let read_key = physical_key(branch, key)?;
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp point view failed: {err}")))?
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    if view
        .read_point(
            &read_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(79)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp below read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "timestamp point read returned row before every eligible timestamp",
        ));
    }
    let at_older = view
        .read_point(
            &read_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(80)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp exact read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("timestamp exact read missed older row"))?;
    if at_older.row() != &older {
        return Err(TestkitError::new("timestamp exact read selected wrong row"));
    }
    let after_all = view
        .read_point(
            &read_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(130)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp nonmonotonic read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("timestamp after-all read missed row"))?;
    if after_all.row() != &highest_version {
        return Err(TestkitError::new(
            "timestamp read sorted by timestamp instead of commit version",
        ));
    }
    outcome.timestamp_point_reads += 2;
    outcome.active_timestamp_point_reads += 2;
    outcome.non_monotonic_timestamp_reads += 1;
    Ok(())
}

fn check_timestamp_frozen_and_owned_point_reads(
    script: &[u8],
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 125));
    let mut state = BranchLocalState::empty(branch);
    let frozen_key = b"generated-frozen-as-of".to_vec();
    let frozen_visible = storage_row_with(
        branch,
        frozen_key.clone(),
        2,
        20,
        Timestamp::EPOCH,
        vec![script_byte(script, 126)],
    )?;
    let frozen_future = storage_row_with(
        branch,
        frozen_key.clone(),
        3,
        50,
        Timestamp::EPOCH,
        b"frozen-future".to_vec(),
    )?;
    for row in [frozen_visible.clone(), frozen_future] {
        state
            .append_committed_row(row)
            .map_err(|err| TestkitError::new(format!("timestamp frozen append failed: {err}")))?;
    }
    match state.rotate_active() {
        BranchRotationOutcome::Rotated { .. } => {}
        outcome @ BranchRotationOutcome::Skipped { .. } => {
            return Err(TestkitError::new(format!(
                "timestamp frozen rotation skipped: {outcome:?}",
            )))
        }
    }

    let owned_key = b"generated-owned-as-of".to_vec();
    let owned_visible = storage_row_with(
        branch,
        owned_key.clone(),
        4,
        40,
        Timestamp::EPOCH,
        vec![script_byte(script, 127)],
    )?;
    let owned_future = storage_row_with(
        branch,
        owned_key.clone(),
        6,
        80,
        Timestamp::EPOCH,
        b"owned-future".to_vec(),
    )?;
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "generated-owned-as-of",
            vec![owned_visible.clone(), owned_future],
        )?)
        .map_err(|err| TestkitError::new(format!("timestamp owned install failed: {err}")))?;

    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp source view failed: {err}")))?;
    let frozen = view
        .read_point(
            &physical_key(branch, frozen_key)?,
            BranchReadBound::at_timestamp(Timestamp::from_micros(30)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp frozen read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("timestamp frozen read missed row"))?;
    if frozen.row() != &frozen_visible || frozen.source() != (BranchRowSource::Frozen { index: 0 })
    {
        return Err(TestkitError::new("timestamp frozen read/source drifted"));
    }
    outcome.timestamp_point_reads += 1;
    outcome.frozen_timestamp_point_reads += 1;

    let owned = view
        .read_point(
            &physical_key(branch, owned_key)?,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp owned read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("timestamp owned read missed row"))?;
    if owned.row() != &owned_visible
        || owned.source()
            != (BranchRowSource::OwnedTable {
                level: BranchLevel::ZERO,
                table_index: 0,
            })
    {
        return Err(TestkitError::new("timestamp owned read/source drifted"));
    }
    outcome.timestamp_point_reads += 1;
    outcome.owned_timestamp_point_reads += 1;
    Ok(())
}

fn check_timestamp_ttl(script: &[u8], outcome: &mut TimestampOutcome) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 114));
    let mut state = BranchLocalState::empty(branch);
    let ttl_key = b"generated-ttl".to_vec();
    let old = storage_row_with(
        branch,
        ttl_key.clone(),
        1,
        5,
        Timestamp::EPOCH,
        vec![script_byte(script, 115)],
    )?;
    let expiring = storage_row_with(
        branch,
        ttl_key.clone(),
        2,
        10,
        Timestamp::from_micros(20),
        vec![script_byte(script, 116)],
    )?;
    let epoch_key = b"generated-epoch-expiry".to_vec();
    let epoch_expiry = storage_row_with(
        branch,
        epoch_key.clone(),
        4,
        40,
        Timestamp::EPOCH,
        Vec::new(),
    )?;
    for row in [old, expiring.clone(), epoch_expiry.clone()] {
        state
            .append_committed_row(row)
            .map_err(|err| TestkitError::new(format!("timestamp ttl append failed: {err}")))?;
    }
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp ttl view failed: {err}")))?;
    let ttl_physical_key = physical_key(branch, ttl_key)?;
    let before_expiry = view
        .read_point(
            &ttl_physical_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(19)),
        )
        .map_err(|err| TestkitError::new(format!("ttl before read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("ttl before expiry missed row"))?;
    if before_expiry.row() != &expiring {
        return Err(TestkitError::new("ttl before expiry selected wrong row"));
    }
    outcome.ttl_before_expiry_reads += 1;

    if view
        .read_point(
            &ttl_physical_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(20)),
        )
        .map_err(|err| TestkitError::new(format!("ttl exact read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "ttl exact expiry remained visible or fell through",
        ));
    }
    outcome.ttl_exact_expiry_suppressions += 1;

    if view
        .read_point(
            &ttl_physical_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(21)),
        )
        .map_err(|err| TestkitError::new(format!("ttl after read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "ttl after expiry remained visible or fell through",
        ));
    }
    outcome.ttl_after_expiry_suppressions += 1;

    let epoch_read = view
        .read_point(
            &physical_key(branch, epoch_key)?,
            BranchReadBound::at_timestamp(Timestamp::MAX),
        )
        .map_err(|err| TestkitError::new(format!("epoch expiry read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("epoch expiry read missed row"))?;
    if epoch_read.row() != &epoch_expiry {
        return Err(TestkitError::new(
            "epoch expiry sentinel did not behave as no expiry",
        ));
    }
    outcome.timestamp_point_reads += 1;

    check_timestamp_max_expiry(script, branch, outcome)?;
    Ok(())
}

fn check_timestamp_max_expiry(
    script: &[u8],
    branch: BranchId,
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let mut state = BranchLocalState::empty(branch);
    let max_key = b"generated-max-expiry".to_vec();
    let max_expiry = storage_row_with(
        branch,
        max_key.clone(),
        5,
        50,
        Timestamp::MAX,
        vec![script_byte(script, 133)],
    )?;
    state
        .append_committed_row(max_expiry.clone())
        .map_err(|err| TestkitError::new(format!("timestamp max expiry append failed: {err}")))?;
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp max expiry view failed: {err}")))?;
    let max_before = view
        .read_point(
            &physical_key(branch, max_key.clone())?,
            BranchReadBound::at_timestamp(Timestamp::from_micros(u64::MAX - 1)),
        )
        .map_err(|err| TestkitError::new(format!("max expiry before read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("max expiry before read missed row"))?;
    if max_before.row() != &max_expiry {
        return Err(TestkitError::new("max expiry selected wrong row"));
    }
    if view
        .read_point(
            &physical_key(branch, max_key)?,
            BranchReadBound::at_timestamp(Timestamp::MAX),
        )
        .map_err(|err| TestkitError::new(format!("max expiry exact read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "Timestamp::MAX expiry behaved as no-expiry sentinel",
        ));
    }
    outcome.ttl_max_expiry_reads += 1;
    Ok(())
}

fn check_timestamp_tombstones(
    script: &[u8],
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 114));
    let mut state = BranchLocalState::empty(branch);
    let deleted_key = b"generated-ts-delete".to_vec();
    let deleted_put = storage_row_with(
        branch,
        deleted_key.clone(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 117)],
    )?;
    let deleted = tombstone_row(branch, deleted_key.clone(), 3, 30)?;
    for row in [deleted_put.clone(), deleted] {
        state.append_committed_row(row).map_err(|err| {
            TestkitError::new(format!("timestamp tombstone append failed: {err}"))
        })?;
    }
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp tombstone view failed: {err}")))?;
    let deleted_physical_key = physical_key(branch, deleted_key)?;
    let before_tombstone = view
        .read_point(
            &deleted_physical_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(29)),
        )
        .map_err(|err| TestkitError::new(format!("pre-tombstone read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("pre-tombstone read missed put"))?;
    if before_tombstone.row() != &deleted_put {
        return Err(TestkitError::new("pre-tombstone read selected wrong row"));
    }
    outcome.timestamp_tombstone_after_non_shadows += 1;
    if view
        .read_point(
            &deleted_physical_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(30)),
        )
        .map_err(|err| TestkitError::new(format!("tombstone timestamp read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new("timestamp tombstone fell through"));
    }
    outcome.timestamp_tombstone_shadows += 1;
    Ok(())
}

fn check_timestamp_scans(
    script: &[u8],
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 118));
    let mut state = BranchLocalState::empty(branch);
    let fixture = timestamp_scan_fixture(script, branch, &mut state)?;
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp scan view failed: {err}")))?
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    check_timestamp_basic_scans(branch, &view, &fixture, outcome)?;
    check_timestamp_scan_edges(branch, &view, outcome)?;
    check_timestamp_scan_space_isolation(branch, &view, &fixture, outcome)?;
    Ok(())
}

struct TimestampScanFixture {
    visible: StorageRow,
    system_row: StorageRow,
    other_space_row: StorageRow,
    engine_space: StorageSpaceId,
    other_space: StorageSpaceId,
}

fn timestamp_scan_fixture(
    script: &[u8],
    branch: BranchId,
    state: &mut BranchLocalState,
) -> Result<TimestampScanFixture, TestkitError> {
    let engine_space = StorageSpaceId::engine(0x20)
        .map_err(|err| TestkitError::new(format!("storage space failed: {err}")))?;
    let other_space = StorageSpaceId::engine(0x21)
        .map_err(|err| TestkitError::new(format!("other storage space failed: {err}")))?;
    let visible = storage_row_with(
        branch,
        b"generated-ts-scan-a".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 119)],
    )?;
    let future = storage_row_with(
        branch,
        b"generated-ts-scan-b".to_vec(),
        2,
        50,
        Timestamp::EPOCH,
        vec![script_byte(script, 120)],
    )?;
    let expired_old = storage_row_with(
        branch,
        b"generated-ts-scan-c".to_vec(),
        1,
        5,
        Timestamp::EPOCH,
        b"old".to_vec(),
    )?;
    let expired_new = storage_row_with(
        branch,
        b"generated-ts-scan-c".to_vec(),
        3,
        30,
        Timestamp::from_micros(35),
        b"expired".to_vec(),
    )?;
    let deleted_old = storage_row_with(
        branch,
        b"generated-ts-scan-d".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"deleted".to_vec(),
    )?;
    let deleted = tombstone_row(branch, b"generated-ts-scan-d".to_vec(), 4, 40)?;
    let system_row = storage_row_with_space(
        branch,
        "system",
        engine_space,
        b"generated-ts-scan-a".to_vec(),
        5,
        15,
        Timestamp::EPOCH,
        b"system-space".to_vec(),
    )?;
    let other_space_row = storage_row_with_space(
        branch,
        "default",
        other_space,
        b"generated-ts-scan-a".to_vec(),
        6,
        15,
        Timestamp::EPOCH,
        b"other-storage-space".to_vec(),
    )?;
    for row in [
        visible.clone(),
        future,
        expired_old,
        expired_new,
        deleted_old,
        deleted,
        system_row.clone(),
        other_space_row.clone(),
    ] {
        state
            .append_committed_row(row)
            .map_err(|err| TestkitError::new(format!("timestamp scan append failed: {err}")))?;
    }
    Ok(TimestampScanFixture {
        visible,
        system_row,
        other_space_row,
        engine_space,
        other_space,
    })
}

fn check_timestamp_basic_scans(
    branch: BranchId,
    view: &crate::branch::read::BranchReadView,
    fixture: &TimestampScanFixture,
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"generated-ts-scan-".to_vec())?);
    let prefix_rows = view
        .scan_prefix(
            &prefix,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp prefix scan failed: {err}")))?;
    if visible_user_keys(&prefix_rows) != vec![fixture.visible.physical_key().user_key().to_vec()] {
        return Err(TestkitError::new("timestamp prefix scan drifted"));
    }
    outcome.timestamp_scan_reads += 1;
    outcome.timestamp_prefix_scans += 1;

    let range = BranchScanBounds::closed(
        &physical_key(branch, b"generated-ts-scan-a".to_vec())?,
        &physical_key(branch, b"generated-ts-scan-d".to_vec())?,
    )
    .map_err(|err| TestkitError::new(format!("timestamp scan bounds failed: {err}")))?;
    let range_rows = view
        .scan_range(
            &range,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp range scan failed: {err}")))?;
    if visible_user_keys(&range_rows) != vec![fixture.visible.physical_key().user_key().to_vec()] {
        return Err(TestkitError::new("timestamp range scan drifted"));
    }
    outcome.timestamp_scan_reads += 1;
    outcome.timestamp_range_scans += 1;
    Ok(())
}

fn check_timestamp_scan_edges(
    branch: BranchId,
    view: &crate::branch::read::BranchReadView,
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"generated-ts-scan-".to_vec())?);
    let before_all_rows = view
        .scan_prefix(
            &prefix,
            BranchReadBound::at_timestamp(Timestamp::from_micros(4)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp empty prefix failed: {err}")))?;
    if !before_all_rows.is_empty() {
        return Err(TestkitError::new(
            "timestamp scan returned rows before every eligible timestamp",
        ));
    }
    outcome.timestamp_scan_reads += 1;
    outcome.timestamp_empty_scans += 1;

    let open = BranchScanBounds::open(
        &physical_key(branch, b"generated-ts-scan-a".to_vec())?,
        &physical_key(branch, b"generated-ts-scan-d".to_vec())?,
    )
    .map_err(|err| TestkitError::new(format!("timestamp open bounds failed: {err}")))?;
    let open_rows = view
        .scan_range(
            &open,
            BranchReadBound::at_timestamp(Timestamp::from_micros(60)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp open range failed: {err}")))?;
    if visible_user_keys(&open_rows) != vec![b"generated-ts-scan-b".to_vec()] {
        return Err(TestkitError::new(
            "timestamp open range failed to preserve exclusive edges",
        ));
    }
    outcome.timestamp_scan_reads += 1;
    outcome.timestamp_range_scans += 1;
    outcome.timestamp_scan_boundary_reads += 1;
    Ok(())
}

fn check_timestamp_scan_space_isolation(
    branch: BranchId,
    view: &crate::branch::read::BranchReadView,
    fixture: &TimestampScanFixture,
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let system_rows = view
        .scan_prefix(
            &BranchScanBounds::prefix(&physical_key_with_space(
                branch,
                "system",
                fixture.engine_space,
                b"generated-ts-scan-".to_vec(),
            )?),
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp system scan failed: {err}")))?;
    let other_space_rows = view
        .scan_prefix(
            &BranchScanBounds::prefix(&physical_key_with_space(
                branch,
                "default",
                fixture.other_space,
                b"generated-ts-scan-".to_vec(),
            )?),
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp other-space scan failed: {err}")))?;
    if system_rows.len() != 1
        || system_rows[0].row() != &fixture.system_row
        || other_space_rows.len() != 1
        || other_space_rows[0].row() != &fixture.other_space_row
    {
        return Err(TestkitError::new(
            "timestamp scans leaked across key-space boundaries",
        ));
    }
    outcome.timestamp_scan_reads += 2;
    outcome.timestamp_prefix_scans += 2;
    outcome.timestamp_scan_space_isolations += 1;
    Ok(())
}

