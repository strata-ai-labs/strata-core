fn check_branch_local_state(script: &[u8]) -> Result<StateOutcome, TestkitError> {
    let mut outcome = StateOutcome::default();
    let branch = branch_id(script_byte(script, 55));
    let config = BranchRuntimeConfig::new(7, 64, 2)
        .map_err(|err| TestkitError::new(format!("state config failed: {err}")))?;
    let mut state = BranchLocalState::new(branch, config)
        .map_err(|err| TestkitError::new(format!("state construction failed: {err}")))?;
    if state.branch_id() != branch
        || state.config() != config
        || !state.is_empty()
        || branch_state_facts(&state)? != BranchStateFacts::empty(branch)
    {
        return Err(TestkitError::new("empty branch-local state drifted"));
    }
    outcome.state_construction += 1;

    outcome.empty_rotation_skips += check_empty_rotation(&mut state)?;
    let put = check_branch_local_append_path(script, branch, &mut state, &mut outcome)?;
    check_branch_local_rotation_path(script, branch, &mut state, &mut outcome, &put)?;
    check_branch_local_edge_facts(branch, &mut outcome)?;
    outcome.frozen_limit_skips += check_frozen_limit_skip(branch)?;
    Ok(outcome)
}

fn check_branch_read_view(script: &[u8]) -> Result<ReadOutcome, TestkitError> {
    let mut outcome = ReadOutcome::default();
    let branch = branch_id(script_byte(script, 79));
    let mut state = BranchLocalState::empty(branch);
    let seed = seed_read_view_state(script, branch, &mut state)?;

    check_read_view_point_reads(&state, branch, &seed, &mut outcome)?;
    check_read_view_history(&state, &seed, &mut outcome)?;
    check_read_view_scans_and_pinning(script, branch, &mut state, &mut outcome)?;
    check_read_view_rejections(script, &seed.read_key, &state, &mut outcome)?;

    Ok(outcome)
}

struct ReadSeed {
    read_key: PhysicalKey,
    tombstone_key: PhysicalKey,
    newer: StorageRow,
    active_lower: StorageRow,
}

fn seed_read_view_state(
    script: &[u8],
    branch: BranchId,
    state: &mut BranchLocalState,
) -> Result<ReadSeed, TestkitError> {
    let mut key = user_key(script, 80);
    key.push(0x01);
    let older = storage_row_with(
        branch,
        key.clone(),
        1,
        10,
        Timestamp::from_micros(100),
        vec![script_byte(script, 84)],
    )?;
    let newer = storage_row_with(
        branch,
        key.clone(),
        3,
        30,
        Timestamp::from_micros(90),
        vec![script_byte(script, 85), 0x00],
    )?;
    let active_lower = storage_row_with(
        branch,
        key.clone(),
        2,
        20,
        Timestamp::EPOCH,
        vec![script_byte(script, 86)],
    )?;
    let mut tombstone_key = user_key(script, 87);
    tombstone_key.push(0x02);
    let tombstone_old = storage_row_with(
        branch,
        tombstone_key.clone(),
        4,
        40,
        Timestamp::EPOCH,
        b"shadowed".to_vec(),
    )?;
    let tombstone = tombstone_row(branch, tombstone_key.clone(), 5, 50)?;

    state
        .append_committed_row(older.clone())
        .map_err(|err| TestkitError::new(format!("read older append failed: {err}")))?;
    state
        .append_committed_row(newer.clone())
        .map_err(|err| TestkitError::new(format!("read newer append failed: {err}")))?;
    state
        .append_committed_row(tombstone_old)
        .map_err(|err| TestkitError::new(format!("read shadowed append failed: {err}")))?;
    state
        .append_committed_row(tombstone.clone())
        .map_err(|err| TestkitError::new(format!("read tombstone append failed: {err}")))?;
    check_successful_rotation(state, 4, 1)?;
    state
        .append_committed_row(active_lower.clone())
        .map_err(|err| TestkitError::new(format!("read active append failed: {err}")))?;

    Ok(ReadSeed {
        read_key: physical_key(branch, key)?,
        tombstone_key: physical_key(branch, tombstone_key)?,
        newer,
        active_lower,
    })
}

fn check_read_view_point_reads(
    state: &BranchLocalState,
    branch: BranchId,
    seed: &ReadSeed,
    outcome: &mut ReadOutcome,
) -> Result<(), TestkitError> {
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("read view capture failed: {err}")))?;
    if view.branch_id() != branch || view.active_row_count() != 1 || view.frozen_table_count() != 1
    {
        return Err(TestkitError::new("read view capture facts drifted"));
    }
    outcome.read_view_captures += 1;

    let latest = view
        .latest(&seed.read_key)
        .map_err(|err| TestkitError::new(format!("latest read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("latest read missed row"))?;
    if latest.row() != &seed.newer || latest.source() != (BranchRowSource::Frozen { index: 0 }) {
        return Err(TestkitError::new(
            "latest read did not select newest version across active/frozen",
        ));
    }
    outcome.latest_point_reads += 1;
    outcome.active_frozen_merge_reads += 1;

    let bounded = view
        .at_version(&seed.read_key, CommitVersion::new(2))
        .map_err(|err| TestkitError::new(format!("version read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("version read missed row"))?;
    if bounded.row() != &seed.active_lower || bounded.source() != BranchRowSource::Active {
        return Err(TestkitError::new("version read selected wrong row"));
    }
    outcome.version_bounded_point_reads += 1;

    let tombstone_read = view
        .latest(&seed.tombstone_key)
        .map_err(|err| TestkitError::new(format!("tombstone read failed: {err}")))?;
    if tombstone_read.is_some() {
        return Err(TestkitError::new(
            "selected tombstone fell through to older put",
        ));
    }
    outcome.tombstone_shadow_reads += 1;
    Ok(())
}

fn check_read_view_history(
    state: &BranchLocalState,
    seed: &ReadSeed,
    outcome: &mut ReadOutcome,
) -> Result<(), TestkitError> {
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("history view capture failed: {err}")))?;
    let history = view
        .history(&seed.read_key, BranchHistoryOptions::all())
        .map_err(|err| TestkitError::new(format!("history read failed: {err}")))?;
    if history_versions(&history) != vec![3, 2, 1] {
        return Err(TestkitError::new("history read order drifted"));
    }
    outcome.history_reads += 1;

    let tombstone_history = view
        .history(&seed.tombstone_key, BranchHistoryOptions::all())
        .map_err(|err| TestkitError::new(format!("tombstone history failed: {err}")))?;
    if !tombstone_history.iter().any(|row| row.row().is_tombstone()) {
        return Err(TestkitError::new("history dropped tombstone row"));
    }
    outcome.history_tombstones += 1;

    let limited = view
        .history(
            &seed.read_key,
            BranchHistoryOptions::all().before_version(CommitVersion::new(3)),
        )
        .map_err(|err| TestkitError::new(format!("bounded history failed: {err}")))?;
    if history_versions(&limited) != vec![2, 1] {
        return Err(TestkitError::new("bounded history drifted"));
    }
    let limited_one = view
        .history(&seed.read_key, BranchHistoryOptions::all().limit(1))
        .map_err(|err| TestkitError::new(format!("limited history failed: {err}")))?;
    if history_versions(&limited_one) != vec![3] {
        return Err(TestkitError::new("one-row history limit drifted"));
    }
    let limited_zero = view
        .history(&seed.read_key, BranchHistoryOptions::all().limit(0))
        .map_err(|err| TestkitError::new(format!("zero history limit failed: {err}")))?;
    if !limited_zero.is_empty() {
        return Err(TestkitError::new("zero history limit returned rows"));
    }
    outcome.history_limits += 1;
    Ok(())
}

fn check_read_view_scans_and_pinning(
    script: &[u8],
    branch: BranchId,
    state: &mut BranchLocalState,
    outcome: &mut ReadOutcome,
) -> Result<(), TestkitError> {
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("pre-prefix capture failed: {err}")))?;
    let mut prefix_key = user_key(script, 91);
    prefix_key.push(0x03);
    let prefix_a = storage_row_with(
        branch,
        [prefix_key.clone(), b"a".to_vec()].concat(),
        6,
        60,
        Timestamp::EPOCH,
        b"prefix-a".to_vec(),
    )?;
    let prefix_b = tombstone_row(branch, [prefix_key.clone(), b"b".to_vec()].concat(), 7, 70)?;
    state
        .append_committed_row(prefix_a.clone())
        .map_err(|err| TestkitError::new(format!("prefix append failed: {err}")))?;
    state
        .append_committed_row(prefix_b.clone())
        .map_err(|err| TestkitError::new(format!("prefix tombstone append failed: {err}")))?;
    let after_prefix = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("post-prefix capture failed: {err}")))?;
    let prefix_rows = after_prefix
        .scan_prefix(
            &BranchScanBounds::prefix(&physical_key(branch, prefix_key.clone())?),
            BranchReadBound::latest(),
        )
        .map_err(|err| TestkitError::new(format!("prefix scan failed: {err}")))?;
    if visible_user_keys(&prefix_rows) != vec![prefix_a.physical_key().user_key().to_vec()] {
        return Err(TestkitError::new("prefix scan result drifted"));
    }
    outcome.prefix_scans += 1;
    outcome.scan_tombstone_suppressions += 1;

    let range_rows = after_prefix
        .scan_range(
            &BranchScanBounds::range(
                branch,
                "default",
                StorageSpaceId::engine(0x20)
                    .map_err(|err| TestkitError::new(format!("storage space failed: {err}")))?,
                BranchUserKeyBound::included(prefix_key.clone()),
                BranchUserKeyBound::excluded([prefix_key.clone(), b"z".to_vec()].concat()),
            )
            .map_err(|err| TestkitError::new(format!("range bounds failed: {err}")))?,
            BranchReadBound::latest(),
        )
        .map_err(|err| TestkitError::new(format!("range scan failed: {err}")))?;
    if visible_user_keys(&range_rows) != vec![prefix_a.physical_key().user_key().to_vec()] {
        return Err(TestkitError::new("range scan result drifted"));
    }
    outcome.range_scans += 1;

    let pinned_before_append = view
        .latest(&physical_key(branch, prefix_key.clone())?)
        .map_err(|err| TestkitError::new(format!("pinned prefix read failed: {err}")))?;
    if pinned_before_append.is_some() {
        return Err(TestkitError::new("pinned view saw append after capture"));
    }
    outcome.pinned_append_isolations += 1;

    let before_rotation_view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("pre-rotation capture failed: {err}")))?;
    check_successful_rotation(state, 3, 2)?;
    let pinned_after_rotation = before_rotation_view
        .latest(prefix_a.physical_key())
        .map_err(|err| TestkitError::new(format!("pinned rotation read failed: {err}")))?;
    if pinned_after_rotation
        .as_ref()
        .is_none_or(|row| row.row() != &prefix_a)
    {
        return Err(TestkitError::new(
            "pinned view lost active row after rotation",
        ));
    }
    outcome.pinned_rotation_isolations += 1;
    Ok(())
}

fn check_read_view_rejections(
    script: &[u8],
    read_key: &PhysicalKey,
    state: &BranchLocalState,
    outcome: &mut ReadOutcome,
) -> Result<(), TestkitError> {
    let after_prefix = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("rejection view capture failed: {err}")))?;
    expect_invalid_branch_row(after_prefix.latest(&physical_key(
        branch_id(script_byte(script, 79).wrapping_add(1)),
        read_key.user_key().to_vec(),
    )?))?;
    outcome.wrong_branch_read_rejections += 1;

    Ok(())
}
