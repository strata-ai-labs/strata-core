fn check_materialization_fault_window(
    script: &[u8],
    source: BranchId,
    child: BranchId,
) -> Result<(), TestkitError> {
    let other_source = distinct_branch_id(script_byte(script, 231), &[source, child]);
    let materialized_layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "fault-materialize-source",
            vec![storage_row_with(
                source,
                b"fault-materialize-a".to_vec(),
                5,
                50,
                Timestamp::EPOCH,
                vec![script_byte(script, 232)],
            )?],
        )?]],
    )?;
    let colliding_layer = branch_inherited_layer(
        other_source,
        CommitVersion::new(4),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            other_source,
            BranchLevel::ZERO,
            "fault-materialize-layer-0-table-0",
            vec![storage_row_with(
                other_source,
                b"fault-materialize-b".to_vec(),
                4,
                40,
                Timestamp::EPOCH,
                vec![script_byte(script, 233)],
            )?],
        )?]],
    )?;
    let mut state = BranchLocalState::empty(child);
    state
        .attach_inherited_layers(vec![materialized_layer, colliding_layer])
        .map_err(|err| TestkitError::new(format!("fault materialization attach failed: {err}")))?;
    let before = state.clone();
    expect_invalid_inherited_layer(state.materialize_inherited_layer(
        &BranchMaterializationRequest::new(child, 0, "fault-materialize").map_err(|err| {
            TestkitError::new(format!("fault materialization request failed: {err}"))
        })?,
    ))?;
    if state != before {
        return Err(TestkitError::new("materialization fault mutated state"));
    }
    Ok(())
}

fn check_snapshot_fault_window(
    script: &[u8],
    lower: BranchId,
    higher: BranchId,
) -> Result<(), TestkitError> {
    let request = BranchSnapshotInstallRequest::new(
        "fault-snapshot",
        vec![
            BranchSnapshotInstallGroup::new(
                lower,
                sorted_snapshot_rows(vec![storage_row_with(
                    lower,
                    b"fault-snapshot-ok".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 234)],
                )?]),
            ),
            BranchSnapshotInstallGroup::new(
                higher,
                sorted_snapshot_rows(vec![storage_row_with(
                    higher,
                    vec![script_byte(script, 235).max(1); 70 * 1024],
                    1,
                    10,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 236)],
                )?]),
            ),
        ],
    )
    .map_err(|err| TestkitError::new(format!("fault snapshot request failed: {err}")))?;
    let mut branches = vec![
        BranchLocalState::empty(lower),
        BranchLocalState::empty(higher),
    ];
    let before = branches.clone();
    if install_snapshot_rows_into_branches(&mut branches, &request).is_ok() {
        return Err(TestkitError::new("snapshot build fault was accepted"));
    }
    if branches != before {
        return Err(TestkitError::new("snapshot build fault mutated state"));
    }
    Ok(())
}

fn compaction_state_with_l0_tables(
    branch: BranchId,
    identity_prefix: &str,
    tables: Vec<Vec<StorageRow>>,
) -> Result<BranchLocalState, TestkitError> {
    let mut state = BranchLocalState::new(branch, compaction_runtime_config()?)
        .map_err(|err| TestkitError::new(format!("compaction state failed: {err}")))?;
    for (index, rows) in tables.into_iter().enumerate() {
        state
            .install_l0_table(branch_owned_table(
                branch,
                BranchLevel::ZERO,
                &format!("{identity_prefix}-l0-{index}"),
                rows,
            )?)
            .map_err(|err| TestkitError::new(format!("compaction L0 install failed: {err}")))?;
    }
    Ok(state)
}

fn compaction_runtime_config() -> Result<BranchRuntimeConfig, TestkitError> {
    BranchRuntimeConfig::new(3, 64, 32)
        .map_err(|err| TestkitError::new(format!("compaction config failed: {err}")))
}

fn branch_compaction_request(
    branch: BranchId,
    kind: BranchCompactionKind,
    output_identity_seed: impl Into<String>,
) -> Result<BranchCompactionRequest, TestkitError> {
    BranchCompactionRequest::new(branch, kind, output_identity_seed)
        .map_err(|err| TestkitError::new(format!("compaction request failed: {err}")))
}

fn visible_row(
    view: &BranchReadView,
    key: &PhysicalKey,
    bound: BranchReadBound,
) -> Result<Option<StorageRow>, TestkitError> {
    view.read_point(key, bound)
        .map(|row| row.map(|row| row.row().clone()))
        .map_err(|err| TestkitError::new(format!("compaction read failed: {err}")))
}

fn check_branch_local_append_path(
    script: &[u8],
    branch: BranchId,
    state: &mut BranchLocalState,
    outcome: &mut StateOutcome,
) -> Result<StorageRow, TestkitError> {
    let put = storage_row_with(
        branch,
        user_key(script, 56),
        5,
        50,
        Timestamp::from_micros(60),
        vec![script_byte(script, 60), 0x00],
    )?;
    append_expect_put(state, &put)?;
    outcome.committed_put_appends += 1;
    outcome.active_only_facts += 1;

    let wrong_branch_row = storage_row_with(
        branch_id(script_byte(script, 55).wrapping_add(1)),
        user_key(script, 61),
        9,
        90,
        Timestamp::EPOCH,
        b"secret-payload".to_vec(),
    )?;
    let facts_before_wrong_branch = branch_state_facts(state)?;
    expect_invalid_branch_row(state.append_committed_row(wrong_branch_row))?;
    if branch_state_facts(state)? != facts_before_wrong_branch {
        return Err(TestkitError::new(
            "wrong-branch append changed branch-local facts",
        ));
    }
    outcome.wrong_branch_append_rejections += 1;

    let facts_before_duplicate = branch_state_facts(state)?;
    expect_duplicate_internal_key(state.append_committed_row(put.clone()))?;
    if branch_state_facts(state)? != facts_before_duplicate {
        return Err(TestkitError::new(
            "active duplicate append changed branch-local facts",
        ));
    }
    outcome.active_duplicate_rejections += 1;

    let same_physical_key_older = storage_row_with(
        branch,
        put.physical_key().user_key().to_vec(),
        4,
        40,
        Timestamp::from_micros(40),
        Vec::new(),
    )?;
    append_expect_put(state, &same_physical_key_older)?;
    outcome.same_key_version_appends += 1;

    let mut other_key = user_key(script, 65);
    other_key.push(0x01);
    let same_version_other_key = storage_row_with(
        branch,
        other_key,
        5,
        70,
        Timestamp::EPOCH,
        vec![script_byte(script, 69)],
    )?;
    append_expect_put(state, &same_version_other_key)?;
    outcome.same_version_key_appends += 1;

    let tombstone = tombstone_row(branch, user_key(script, 70), 11, 30)?;
    let tombstone_key = TableInternalKeyBytes::from_row(&tombstone);
    let tombstone_outcome = state
        .append_committed_row(tombstone.clone())
        .map_err(|err| TestkitError::new(format!("tombstone append failed: {err}")))?;
    if !tombstone_outcome.is_tombstone()
        || state.tombstone_rows() != 1
        || state
            .active()
            .get(&tombstone_key)
            .is_none_or(|stored| stored.row() != &tombstone)
    {
        return Err(TestkitError::new("tombstone append facts drifted"));
    }
    outcome.committed_tombstone_appends += 1;

    let mixed_facts = branch_state_facts(state)?;
    if mixed_facts.active_rows() != 4
        || mixed_facts.frozen_table_count() != 0
        || mixed_facts.max_commit_version() != Some(CommitVersion::new(11))
        || mixed_facts.timestamp_min() != Some(Timestamp::from_micros(30))
        || mixed_facts.timestamp_max() != Some(Timestamp::from_micros(70))
    {
        return Err(TestkitError::new("active branch-local facts drifted"));
    }
    Ok(put)
}

fn check_branch_local_rotation_path(
    script: &[u8],
    branch: BranchId,
    state: &mut BranchLocalState,
    outcome: &mut StateOutcome,
    put: &StorageRow,
) -> Result<(), TestkitError> {
    outcome.active_rotations += check_successful_rotation(state, 4, 1)?;
    let frozen_only = branch_state_facts(state)?;
    if frozen_only.active_rows() != 0 || frozen_only.frozen_table_count() != 1 {
        return Err(TestkitError::new("frozen-only branch facts drifted"));
    }
    outcome.frozen_only_facts += 1;

    let duplicate_frozen = storage_row_with(
        branch,
        put.physical_key().user_key().to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"duplicate".to_vec(),
    )?;
    let facts_before_frozen_duplicate = branch_state_facts(state)?;
    expect_duplicate_internal_key(state.append_committed_row(duplicate_frozen))?;
    if branch_state_facts(state)? != facts_before_frozen_duplicate {
        return Err(TestkitError::new(
            "frozen duplicate append changed branch-local facts",
        ));
    }
    outcome.frozen_duplicate_rejections += 1;

    let later = storage_row_with(
        branch,
        user_key(script, 74),
        12,
        120,
        Timestamp::EPOCH,
        vec![script_byte(script, 78)],
    )?;
    state
        .append_committed_row(later)
        .map_err(|err| TestkitError::new(format!("mixed append failed: {err}")))?;
    let mixed = branch_state_facts(state)?;
    if mixed.active_rows() != 1
        || mixed.frozen_table_count() != 1
        || mixed.max_commit_version() != Some(CommitVersion::new(12))
        || mixed.timestamp_max() != Some(Timestamp::from_micros(120))
    {
        return Err(TestkitError::new("mixed active/frozen facts drifted"));
    }
    outcome.mixed_active_frozen_facts += 1;
    Ok(())
}

fn check_branch_local_edge_facts(
    branch: BranchId,
    outcome: &mut StateOutcome,
) -> Result<(), TestkitError> {
    let mut state = BranchLocalState::empty(branch);
    let zero = storage_row_with(
        branch,
        b"generated-zero-edge".to_vec(),
        0,
        0,
        Timestamp::EPOCH,
        Vec::new(),
    )?;
    let max = storage_row_with(
        branch,
        b"generated-max-edge".to_vec(),
        u64::MAX,
        u64::MAX,
        Timestamp::MAX,
        b"max".to_vec(),
    )?;

    append_expect_put(&mut state, &zero)?;
    let zero_facts = branch_state_facts(&state)?;
    if zero_facts.max_commit_version() != Some(CommitVersion::ZERO)
        || zero_facts.timestamp_min() != Some(Timestamp::EPOCH)
        || zero_facts.timestamp_max() != Some(Timestamp::EPOCH)
    {
        return Err(TestkitError::new(
            "zero version/timestamp branch-local edge facts drifted",
        ));
    }

    append_expect_put(&mut state, &max)?;
    let max_facts = branch_state_facts(&state)?;
    if max_facts.max_commit_version() != Some(CommitVersion::MAX)
        || max_facts.timestamp_min() != Some(Timestamp::EPOCH)
        || max_facts.timestamp_max() != Some(Timestamp::MAX)
    {
        return Err(TestkitError::new(
            "max version/timestamp branch-local edge facts drifted",
        ));
    }

    check_successful_rotation(&mut state, 2, 1)?;
    let rotated = branch_state_facts(&state)?;
    if rotated.active_rows() != 0
        || rotated.frozen_table_count() != 1
        || rotated.max_commit_version() != Some(CommitVersion::MAX)
        || rotated.timestamp_min() != Some(Timestamp::EPOCH)
        || rotated.timestamp_max() != Some(Timestamp::MAX)
    {
        return Err(TestkitError::new("rotated branch-local edge facts drifted"));
    }

    outcome.timestamp_edge_facts += 1;
    outcome.max_commit_edge_facts += 1;
    Ok(())
}

fn check_empty_rotation(state: &mut BranchLocalState) -> Result<usize, TestkitError> {
    match state.rotate_active() {
        BranchRotationOutcome::Skipped {
            reason: BranchRotationSkipReason::EmptyActive,
        } if state.frozen().is_empty() => Ok(1),
        other => Err(TestkitError::new(format!(
            "empty rotation returned unexpected outcome: {other:?}"
        ))),
    }
}

fn check_successful_rotation(
    state: &mut BranchLocalState,
    expected_rows: usize,
    expected_tables: usize,
) -> Result<usize, TestkitError> {
    match state.rotate_active() {
        BranchRotationOutcome::Rotated {
            frozen_index: 0,
            frozen_rows,
            frozen_tables,
        } if frozen_rows == expected_rows
            && frozen_tables == expected_tables
            && state.active().is_empty()
            && state.frozen_table_count() == expected_tables =>
        {
            Ok(1)
        }
        other => Err(TestkitError::new(format!(
            "active rotation returned unexpected outcome: {other:?}"
        ))),
    }
}

fn check_frozen_limit_skip(branch: BranchId) -> Result<usize, TestkitError> {
    let config = BranchRuntimeConfig::new(7, 64, 1)
        .map_err(|err| TestkitError::new(format!("limit config failed: {err}")))?;
    let mut state = BranchLocalState::new(branch, config)
        .map_err(|err| TestkitError::new(format!("limit state failed: {err}")))?;
    let first = storage_row_with(
        branch,
        b"limit-first".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"first".to_vec(),
    )?;
    let second = storage_row_with(
        branch,
        b"limit-second".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"second".to_vec(),
    )?;
    state
        .append_committed_row(first.clone())
        .map_err(|err| TestkitError::new(format!("limit first append failed: {err}")))?;
    check_successful_rotation(&mut state, 1, 1)?;
    state
        .append_committed_row(second.clone())
        .map_err(|err| TestkitError::new(format!("limit second append failed: {err}")))?;
    let before_skip = branch_state_facts(&state)?;

    match state.rotate_active() {
        BranchRotationOutcome::Skipped {
            reason: BranchRotationSkipReason::FrozenLimitReached,
        } if branch_state_facts(&state)? == before_skip
            && state
                .active()
                .get(&TableInternalKeyBytes::from_row(&second))
                .is_some()
            && state.frozen()[0]
                .get(&TableInternalKeyBytes::from_row(&first))
                .is_some() => {}
        other => {
            return Err(TestkitError::new(format!(
                "frozen-limit rotation returned unexpected outcome: {other:?}"
            )))
        }
    }

    let third = storage_row_with(
        branch,
        b"limit-third".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"third".to_vec(),
    )?;
    state
        .append_committed_row(third.clone())
        .map_err(|err| TestkitError::new(format!("limit third append failed: {err}")))?;
    if state.active_row_count() != 2
        || state.frozen_table_count() != 1
        || state
            .active()
            .get(&TableInternalKeyBytes::from_row(&third))
            .is_none_or(|stored| stored.row() != &third)
    {
        return Err(TestkitError::new(
            "append after frozen-limit skip did not preserve active state",
        ));
    }
    Ok(1)
}

fn append_expect_put(state: &mut BranchLocalState, row: &StorageRow) -> Result<(), TestkitError> {
    let key = TableInternalKeyBytes::from_row(row);
    let outcome = state
        .append_committed_row(row.clone())
        .map_err(|err| TestkitError::new(format!("put append failed: {err}")))?;
    if outcome.is_tombstone()
        || outcome.commit_version() != row.commit_version()
        || outcome.commit_timestamp() != row.commit_timestamp()
        || state
            .active()
            .get(&key)
            .is_none_or(|stored| stored.row() != row)
    {
        return Err(TestkitError::new("put append facts drifted"));
    }
    Ok(())
}

fn expect_invalid_branch_row<T>(result: Result<T, BranchRuntimeError>) -> Result<(), TestkitError> {
    match result {
        Err(BranchRuntimeError::InvalidBranchRow { .. }) => Ok(()),
        Err(err) => Err(TestkitError::new(format!(
            "wrong-branch append returned wrong error: {err}"
        ))),
        Ok(_) => Err(TestkitError::new("wrong-branch append succeeded")),
    }
}

fn expect_duplicate_internal_key<T>(
    result: Result<T, BranchRuntimeError>,
) -> Result<(), TestkitError> {
    match result {
        Err(BranchRuntimeError::TableRuntime {
            source: crate::table::TableRuntimeError::DuplicateInternalKey { .. },
        }) => Ok(()),
        Err(err) => Err(TestkitError::new(format!(
            "duplicate append returned wrong error: {err}"
        ))),
        Ok(_) => Err(TestkitError::new("duplicate append succeeded")),
    }
}

fn expect_invalid_inherited_layer<T>(
    result: Result<T, BranchRuntimeError>,
) -> Result<(), TestkitError> {
    match result {
        Err(BranchRuntimeError::InvalidInheritedLayer { .. }) => Ok(()),
        Err(err) => Err(TestkitError::new(format!(
            "invalid inherited layer returned wrong error: {err}"
        ))),
        Ok(_) => Err(TestkitError::new("invalid inherited layer was accepted")),
    }
}

fn branch_state_facts(state: &BranchLocalState) -> Result<BranchStateFacts, TestkitError> {
    state.facts().map_err(|error| state_fact_error(&error))
}

fn state_fact_error(error: &BranchRuntimeError) -> TestkitError {
    TestkitError::new(format!("branch-local facts failed: {error}"))
}

fn expect_invalid_config<T>(result: Result<T, BranchRuntimeError>) -> Result<(), TestkitError> {
    match result {
        Err(BranchRuntimeError::InvalidConfig { .. }) => Ok(()),
        Err(err) => Err(TestkitError::new(format!(
            "invalid config returned wrong error: {err}"
        ))),
        Ok(_) => Err(TestkitError::new("invalid branch config was accepted")),
    }
}

fn expect_invalid_state<T>(result: Result<T, BranchRuntimeError>) -> Result<(), TestkitError> {
    match result {
        Err(BranchRuntimeError::InvalidBranchState { .. }) => Ok(()),
        Err(err) => Err(TestkitError::new(format!(
            "invalid branch facts returned wrong error: {err}"
        ))),
        Ok(_) => Err(TestkitError::new("invalid branch facts were accepted")),
    }
}

fn expect_invalid_reachability<T>(
    result: Result<T, BranchRuntimeError>,
) -> Result<(), TestkitError> {
    match result {
        Err(BranchRuntimeError::InvalidReachability { .. }) => Ok(()),
        Err(err) => Err(TestkitError::new(format!(
            "invalid reachability returned wrong error: {err}"
        ))),
        Ok(_) => Err(TestkitError::new("invalid reachability was accepted")),
    }
}

fn expect_invalid_compaction<T>(result: Result<T, BranchRuntimeError>) -> Result<(), TestkitError> {
    match result {
        Err(BranchRuntimeError::InvalidCompaction { .. }) => Ok(()),
        Err(err) => Err(TestkitError::new(format!(
            "invalid compaction returned wrong error: {err}"
        ))),
        Ok(_) => Err(TestkitError::new("invalid compaction was accepted")),
    }
}

fn expect_invalid_snapshot_install<T>(
    result: Result<T, BranchRuntimeError>,
    expected_reason: &'static str,
) -> Result<(), TestkitError> {
    match result {
        Err(BranchRuntimeError::InvalidSnapshotInstall { reason }) if reason == expected_reason => {
            Ok(())
        }
        Err(err) => Err(TestkitError::new(format!(
            "invalid snapshot install returned wrong error: {err}"
        ))),
        Ok(_) => Err(TestkitError::new("invalid snapshot install was accepted")),
    }
}

fn expect_missing_snapshot_branch<T>(
    result: Result<T, BranchRuntimeError>,
    expected_branch: BranchId,
) -> Result<(), TestkitError> {
    match result {
        Err(BranchRuntimeError::BranchNotFound { branch_id }) if branch_id == expected_branch => {
            Ok(())
        }
        Err(err) => Err(TestkitError::new(format!(
            "missing snapshot branch returned wrong error: {err}"
        ))),
        Ok(_) => Err(TestkitError::new("missing snapshot branch was accepted")),
    }
}

fn branch_state_by_id(
    branches: &[BranchLocalState],
    branch_id: BranchId,
) -> Result<&BranchLocalState, TestkitError> {
    branches
        .iter()
        .find(|state| state.branch_id() == branch_id)
        .ok_or_else(|| TestkitError::new("snapshot branch state missing after install"))
}

fn sorted_snapshot_rows(rows: Vec<StorageRow>) -> Vec<StorageRow> {
    let mut rows = rows.into_iter().map(TableRow::new).collect::<Vec<_>>();
    sort_table_rows_by_key(&mut rows);
    rows.into_iter().map(TableRow::into_row).collect()
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn distinct_branch_id(seed: u8, excluded: &[BranchId]) -> BranchId {
    for offset in 0..=u8::MAX {
        let candidate = branch_id(seed.wrapping_add(offset));
        if !excluded.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!("more branch-id exclusions than byte-backed test ids");
}

fn table_identity(identity: &str) -> Result<TableIdentity, TestkitError> {
    TableIdentity::new(identity)
        .map_err(|err| TestkitError::new(format!("table identity failed: {err}")))
}

fn branch_owned_table(
    branch_id: BranchId,
    level: BranchLevel,
    identity: &str,
    rows: Vec<StorageRow>,
) -> Result<BranchOwnedTable, TestkitError> {
    let reader = immutable_reader(identity, rows)?;
    let descriptor = branch_table_descriptor(level, &reader)?;
    BranchOwnedTable::new(branch_id, descriptor, reader)
        .map_err(|err| TestkitError::new(format!("branch-owned table failed: {err}")))
}

fn branch_inherited_layer(
    source_branch_id: BranchId,
    fork_version: CommitVersion,
    status: InheritedLayerStatus,
    owned_levels: Vec<Vec<BranchOwnedTable>>,
) -> Result<BranchInheritedLayer, TestkitError> {
    let table_count = owned_levels.iter().map(Vec::len).sum();
    let descriptor =
        InheritedLayerDescriptor::new(source_branch_id, fork_version, status, table_count);
    BranchInheritedLayer::new(descriptor, owned_levels)
        .map_err(|err| TestkitError::new(format!("branch inherited layer failed: {err}")))
}

fn branch_inherited_layer_unchecked_for_fork_gate_checks(
    source_branch_id: BranchId,
    fork_version: CommitVersion,
    status: InheritedLayerStatus,
    owned_levels: Vec<Vec<BranchOwnedTable>>,
) -> BranchInheritedLayer {
    let table_count = owned_levels.iter().map(Vec::len).sum();
    let descriptor =
        InheritedLayerDescriptor::new(source_branch_id, fork_version, status, table_count);
    BranchInheritedLayer::new_unchecked_for_test(descriptor, owned_levels)
}

fn branch_table_descriptor(
    level: BranchLevel,
    reader: &ImmutableTableReader,
) -> Result<BranchTableDescriptor, TestkitError> {
    BranchTableDescriptor::new(
        reader.facts().identity().clone(),
        reader.facts().clone(),
        level,
    )
    .map_err(|err| TestkitError::new(format!("branch table descriptor failed: {err}")))
}

fn immutable_reader(
    identity: &str,
    rows: Vec<StorageRow>,
) -> Result<ImmutableTableReader, TestkitError> {
    let mut rows = rows.into_iter().map(TableRow::new).collect::<Vec<_>>();
    sort_table_rows_by_key(&mut rows);
    let identity = TableIdentity::new(identity)
        .map_err(|err| TestkitError::new(format!("table identity failed: {err}")))?;
    let builder = ImmutableTableBuilder::new(TableBuilderConfig::default())
        .map_err(|err| TestkitError::new(format!("table builder failed: {err}")))?;
    let artifact = builder
        .build_from_rows(identity.clone(), &rows)
        .map_err(|err| TestkitError::new(format!("immutable table build failed: {err}")))?;
    ImmutableTableReader::open_bytes(
        identity,
        artifact.into_bytes(),
        TableReaderConfig::default(),
    )
    .map_err(|err| TestkitError::new(format!("immutable table reader failed: {err}")))
}

fn table_facts(identity: &str) -> Result<TableRuntimeFacts, TestkitError> {
    TableRuntimeFacts::new(
        TableIdentity::new(identity)
            .map_err(|err| TestkitError::new(format!("table identity failed: {err}")))?,
        1,
        1,
        TableKeyRange::new(vec![0x01], vec![0x02])
            .map_err(|err| TestkitError::new(format!("table key range failed: {err}")))?,
        TableCommitRange::new(CommitVersion::new(1), CommitVersion::new(1))
            .map_err(|err| TestkitError::new(format!("table commit range failed: {err}")))?,
        128,
    )
    .map_err(|err| TestkitError::new(format!("table facts failed: {err}")))
}

fn row_versions(rows: &[TableRow]) -> Vec<u64> {
    rows.iter()
        .map(|row| row.commit_version().as_u64())
        .collect()
}

fn matching_versions(rows: &[TableRow], bound: BranchEffectiveReadBound) -> Vec<u64> {
    rows.iter()
        .filter(|row| bound.matches_row(row.row()))
        .map(|row| row.commit_version().as_u64())
        .collect()
}

fn history_versions(rows: &[BranchHistoryRow]) -> Vec<u64> {
    rows.iter()
        .map(|row| row.row().commit_version().as_u64())
        .collect()
}

fn visible_user_keys(rows: &[BranchVisibleRow]) -> Vec<Vec<u8>> {
    rows.iter()
        .map(|row| row.row().physical_key().user_key().to_vec())
        .collect()
}

fn storage_row(branch_id: BranchId, version: u64) -> Result<StorageRow, TestkitError> {
    storage_row_with(
        branch_id,
        b"key".to_vec(),
        version,
        version,
        Timestamp::EPOCH,
        b"row-bytes".to_vec(),
    )
}

fn storage_row_with(
    branch_id: BranchId,
    user_key: Vec<u8>,
    version: u64,
    timestamp: u64,
    expires_at: Timestamp,
    value: Vec<u8>,
) -> Result<StorageRow, TestkitError> {
    storage_row_with_space(
        branch_id,
        "default",
        StorageSpaceId::engine(0x20)
            .map_err(|err| TestkitError::new(format!("storage space failed: {err}")))?,
        user_key,
        version,
        timestamp,
        expires_at,
        value,
    )
}

fn storage_row_with_space(
    branch_id: BranchId,
    space_name: &str,
    space: StorageSpaceId,
    user_key: Vec<u8>,
    version: u64,
    timestamp: u64,
    expires_at: Timestamp,
    value: Vec<u8>,
) -> Result<StorageRow, TestkitError> {
    Ok(StorageRow::put(
        physical_key_with_space(branch_id, space_name, space, user_key)?,
        CommitVersion::new(version),
        Timestamp::from_micros(timestamp),
        expires_at,
        value,
    ))
}

fn tombstone_row(
    branch_id: BranchId,
    user_key: Vec<u8>,
    version: u64,
    timestamp: u64,
) -> Result<StorageRow, TestkitError> {
    Ok(StorageRow::tombstone(
        physical_key(branch_id, user_key)?,
        CommitVersion::new(version),
        Timestamp::from_micros(timestamp),
    ))
}

fn physical_key(branch_id: BranchId, user_key: Vec<u8>) -> Result<PhysicalKey, TestkitError> {
    let space = StorageSpaceId::engine(0x20)
        .map_err(|err| TestkitError::new(format!("storage space failed: {err}")))?;
    physical_key_with_space(branch_id, "default", space, user_key)
}

fn physical_key_with_space(
    branch_id: BranchId,
    space_name: &str,
    space: StorageSpaceId,
    user_key: Vec<u8>,
) -> Result<PhysicalKey, TestkitError> {
    PhysicalKey::new(branch_id, space_name, space, user_key)
        .map_err(|err| TestkitError::new(format!("physical key failed: {err}")))
}

fn user_key(script: &[u8], start: usize) -> Vec<u8> {
    vec![
        script_byte(script, start),
        0x00,
        script_byte(script, start + 1),
        script_byte(script, start + 2),
    ]
}

fn script_byte(script: &[u8], index: usize) -> u8 {
    script.get(index).copied().unwrap_or(0)
}

#[derive(Debug)]
struct LeafError;

impl fmt::Display for LeafError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("leaf source")
    }
}

impl Error for LeafError {}
