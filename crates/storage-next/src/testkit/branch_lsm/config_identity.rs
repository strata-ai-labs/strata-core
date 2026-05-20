fn check_valid_config(script: &[u8]) -> Result<(), TestkitError> {
    let levels = 1 + usize::from(script_byte(script, 0) % 8);
    let inherited = 1 + usize::from(script_byte(script, 1) % 16);
    let frozen = 1 + usize::from(script_byte(script, 2) % 16);
    let config = BranchRuntimeConfig::new(levels, inherited, frozen)
        .map_err(|err| TestkitError::new(format!("valid branch config rejected: {err}")))?;
    if config.max_level_count() != levels
        || config.max_inherited_layers() != inherited
        || config.max_frozen_tables() != frozen
    {
        return Err(TestkitError::new("valid branch config facts drifted"));
    }
    Ok(())
}

fn check_invalid_configs() -> Result<usize, TestkitError> {
    expect_invalid_config(BranchRuntimeConfig::new(0, 1, 1))?;
    expect_invalid_config(BranchRuntimeConfig::new(1, 0, 1))?;
    expect_invalid_config(BranchRuntimeConfig::new(1, 1, 0))?;
    Ok(3)
}

fn check_read_bounds(script: &[u8]) -> Result<(), TestkitError> {
    let version = CommitVersion::new(u64::from(script_byte(script, 3)));
    let timestamp = Timestamp::from_micros(u64::from(script_byte(script, 4)));
    if BranchReadBound::latest() != BranchReadBound::Latest {
        return Err(TestkitError::new("latest read bound drifted"));
    }
    if BranchReadBound::at_version(version) != BranchReadBound::AtVersion(version) {
        return Err(TestkitError::new("version read bound drifted"));
    }
    if BranchReadBound::at_timestamp(timestamp) != BranchReadBound::AtTimestamp(timestamp) {
        return Err(TestkitError::new("timestamp read bound drifted"));
    }
    Ok(())
}

fn check_valid_facts(script: &[u8]) -> Result<(), TestkitError> {
    let branch_id = branch_id(script_byte(script, 5));
    let facts = BranchStateFacts::new(
        branch_id,
        1,
        usize::from(script_byte(script, 6) % 4),
        usize::from(script_byte(script, 7) % 4),
        usize::from(script_byte(script, 8) % 4),
        Some(CommitVersion::new(10)),
        Some(Timestamp::from_micros(1)),
        Some(Timestamp::from_micros(2)),
    )
    .map_err(|err| TestkitError::new(format!("valid branch facts rejected: {err}")))?;
    if facts.branch_id() != branch_id
        || facts.active_rows() != 1
        || facts.max_commit_version() != Some(CommitVersion::new(10))
        || facts.timestamp_min() != Some(Timestamp::from_micros(1))
        || facts.timestamp_max() != Some(Timestamp::from_micros(2))
    {
        return Err(TestkitError::new("valid branch facts drifted"));
    }

    let empty = BranchStateFacts::empty(branch_id);
    if empty.max_commit_version().is_some() || empty.timestamp_min().is_some() {
        return Err(TestkitError::new("empty branch facts drifted"));
    }
    Ok(())
}

fn check_invalid_facts(script: &[u8]) -> Result<usize, TestkitError> {
    let branch_id = branch_id(script_byte(script, 9));
    expect_invalid_state(BranchStateFacts::new(
        branch_id,
        0,
        0,
        0,
        0,
        Some(CommitVersion::new(1)),
        None,
        None,
    ))?;
    expect_invalid_state(BranchStateFacts::new(
        branch_id,
        0,
        0,
        0,
        0,
        None,
        Some(Timestamp::from_micros(1)),
        Some(Timestamp::from_micros(1)),
    ))?;
    expect_invalid_state(BranchStateFacts::new(
        branch_id,
        1,
        0,
        0,
        0,
        Some(CommitVersion::new(1)),
        Some(Timestamp::from_micros(2)),
        Some(Timestamp::from_micros(1)),
    ))?;
    expect_invalid_state(BranchStateFacts::new(
        branch_id,
        1,
        0,
        0,
        0,
        Some(CommitVersion::new(1)),
        Some(Timestamp::from_micros(1)),
        None,
    ))?;
    Ok(4)
}

fn check_descriptors(script: &[u8]) -> Result<(), TestkitError> {
    let test_branch_id = branch_id(script_byte(script, 10));
    let facts = BranchStateFacts::empty(test_branch_id);
    let state = BranchStateDescriptor::new(test_branch_id, facts)
        .map_err(|err| TestkitError::new(format!("state descriptor failed: {err}")))?;
    let view = BranchViewDescriptor::new(test_branch_id, facts)
        .map_err(|err| TestkitError::new(format!("view descriptor failed: {err}")))?;
    if state.branch_id() != test_branch_id || view.facts() != facts {
        return Err(TestkitError::new("branch state descriptors drifted"));
    }

    let table_facts = table_facts("branch-scaffold")?;
    let table = BranchTableDescriptor::new(
        TableIdentity::new("branch-scaffold")
            .map_err(|err| TestkitError::new(format!("table identity failed: {err}")))?,
        table_facts.clone(),
        BranchLevel::new(script_byte(script, 11) % 4),
    )
    .map_err(|err| TestkitError::new(format!("branch table descriptor failed: {err}")))?;
    if table.facts() != &table_facts || table.identity().as_str() != "branch-scaffold" {
        return Err(TestkitError::new("branch table descriptor drifted"));
    }

    let inherited = InheritedLayerDescriptor::new(
        branch_id(99),
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        2,
    );
    if inherited.source_branch_id() != branch_id(99)
        || inherited.fork_version() != CommitVersion::new(5)
        || inherited.table_count() != 2
    {
        return Err(TestkitError::new("inherited layer descriptor drifted"));
    }

    let reachability = BranchReachabilityFacts::new(test_branch_id, 1, 2);
    if reachability.owned_table_count() != 1 || reachability.inherited_table_count() != 2 {
        return Err(TestkitError::new("branch reachability facts drifted"));
    }

    let row = storage_row(test_branch_id, 7)?;
    let source = BranchRowSource::OwnedTable {
        level: BranchLevel::ZERO,
        table_index: 0,
    };
    let visible = BranchVisibleRow::new(row.clone(), source);
    let history = BranchHistoryRow::new(row, source);
    if visible.source() != source || history.source() != source {
        return Err(TestkitError::new("branch row result source drifted"));
    }
    Ok(())
}

fn check_error_sources() -> Result<(), TestkitError> {
    let table_error = BranchRuntimeError::TableRuntime {
        source: crate::table::TableRuntimeError::Cache {
            reason: "scaffold cache",
        },
    };
    if table_error.source().is_none() {
        return Err(TestkitError::new("branch table error source missing"));
    }

    let publish_error = BranchRuntimeError::publish_with("scaffold publish", LeafError);
    match publish_error.source() {
        Some(source) if source.to_string() == "leaf source" => {}
        _ => return Err(TestkitError::new("branch publish error source missing")),
    }

    if BranchRuntimeError::publish("scaffold publish")
        .to_string()
        .contains("secret-payload")
    {
        return Err(TestkitError::new("branch error leaked payload text"));
    }
    Ok(())
}

fn check_stats(script: &[u8]) -> Result<(), TestkitError> {
    let empty = BranchRuntimeStats::default();
    if empty.latest_reads() != 0
        || empty.bounded_reads() != 0
        || empty.history_reads() != 0
        || empty.inherited_layers_examined() != 0
    {
        return Err(TestkitError::new("default branch stats drifted"));
    }

    let stats = BranchRuntimeStats::new(
        u64::from(script_byte(script, 12)),
        u64::from(script_byte(script, 13)),
        u64::from(script_byte(script, 14)),
        u64::from(script_byte(script, 15)),
    );
    if stats.latest_reads() != u64::from(script_byte(script, 12))
        || stats.bounded_reads() != u64::from(script_byte(script, 13))
        || stats.history_reads() != u64::from(script_byte(script, 14))
        || stats.inherited_layers_examined() != u64::from(script_byte(script, 15))
    {
        return Err(TestkitError::new("branch stats drifted"));
    }
    Ok(())
}

fn check_row_identity_and_rewrites(script: &[u8]) -> Result<IdentityOutcome, TestkitError> {
    let source = branch_id(script_byte(script, 16));
    let target = branch_id(script_byte(script, 16).wrapping_add(1));
    let row = storage_row_with(
        source,
        user_key(script, 18),
        u64::from(script_byte(script, 22)),
        u64::from(script_byte(script, 23)),
        Timestamp::from_micros(u64::from(script_byte(script, 24))),
        vec![script_byte(script, 25), 0x00, script_byte(script, 26)],
    )?;
    let tombstone = tombstone_row(
        source,
        user_key(script, 27),
        u64::from(script_byte(script, 31)),
        u64::from(script_byte(script, 32)),
    )?;

    require_row_branch(source, &row)
        .map_err(|err| TestkitError::new(format!("matching row rejected: {err}")))?;
    if !row_matches_branch(source, &row) {
        return Err(TestkitError::new("matching row predicate returned false"));
    }
    if row_matches_branch(target, &row) {
        return Err(TestkitError::new("mismatching row predicate returned true"));
    }
    if !matches!(
        require_row_branch(target, &row),
        Err(BranchRuntimeError::InvalidBranchRow { .. })
    ) {
        return Err(TestkitError::new("mismatching row was not rejected"));
    }

    let rewritten_key = rewrite_physical_key_branch(row.physical_key(), target)
        .map_err(|err| TestkitError::new(format!("physical key rewrite failed: {err}")))?;
    if rewritten_key.branch_id() != target
        || rewritten_key.space() != row.physical_key().space()
        || rewritten_key.storage_space_id() != row.physical_key().storage_space_id()
        || rewritten_key.user_key() != row.physical_key().user_key()
    {
        return Err(TestkitError::new(
            "physical key rewrite changed non-branch facts",
        ));
    }

    let rewritten = rewrite_row_branch(&row, source, target)
        .map_err(|err| TestkitError::new(format!("row rewrite failed: {err}")))?;
    if rewritten.physical_key().branch_id() != target
        || rewritten.commit_version() != row.commit_version()
        || rewritten.commit_timestamp() != row.commit_timestamp()
        || rewritten.expires_at() != row.expires_at()
        || rewritten.value() != row.value()
        || rewritten.is_tombstone()
    {
        return Err(TestkitError::new("put row rewrite changed row facts"));
    }
    let rewritten_tombstone = rewrite_row_branch(&tombstone, source, target)
        .map_err(|err| TestkitError::new(format!("tombstone rewrite failed: {err}")))?;
    if !rewritten_tombstone.is_tombstone()
        || !rewritten_tombstone.value().is_empty()
        || rewritten_tombstone.commit_version() != tombstone.commit_version()
    {
        return Err(TestkitError::new("tombstone rewrite changed row shape"));
    }
    if !matches!(
        rewrite_row_branch(&row, target, source),
        Err(BranchRuntimeError::InvalidBranchRow { .. })
    ) {
        return Err(TestkitError::new("row rewrite skipped source preflight"));
    }

    Ok(IdentityOutcome {
        matching_rows: 1,
        mismatching_rows: 1,
        physical_key_rewrites: 1,
        row_rewrites: 2,
    })
}

fn check_effective_bounds_and_candidates(script: &[u8]) -> Result<BoundsOutcome, TestkitError> {
    let branch = branch_id(script_byte(script, 33));
    let version = CommitVersion::new(1 + u64::from(script_byte(script, 34)));
    let timestamp = Timestamp::from_micros(u64::from(script_byte(script, 35)));
    let row = storage_row_with(
        branch,
        user_key(script, 36),
        version.as_u64(),
        timestamp.as_micros(),
        Timestamp::from_micros(timestamp.as_micros().saturating_sub(1)),
        vec![script_byte(script, 40)],
    )?;
    let tombstone = tombstone_row(
        branch,
        user_key(script, 41),
        version.as_u64(),
        timestamp.as_micros(),
    )?;

    let own_latest = BranchEffectiveReadBound::for_own_branch(BranchReadBound::latest());
    if own_latest.max_commit_version().is_some()
        || own_latest.max_commit_timestamp().is_some()
        || !own_latest.matches_row(&row).matches_effective_bound()
    {
        return Err(TestkitError::new("own latest bound drifted"));
    }
    let own_version =
        BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_version(version));
    if !own_version.matches_row(&row).matches_effective_bound() {
        return Err(TestkitError::new("own version bound is not inclusive"));
    }
    let own_timestamp =
        BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_timestamp(timestamp));
    if !own_timestamp.matches_row(&row).matches_effective_bound() {
        return Err(TestkitError::new("own timestamp bound is not inclusive"));
    }

    let fork_version = CommitVersion::new(version.as_u64().saturating_sub(1));
    let inherited_latest =
        BranchEffectiveReadBound::for_inherited_layer(BranchReadBound::latest(), fork_version);
    if inherited_latest.max_commit_version() != Some(fork_version) {
        return Err(TestkitError::new("inherited latest lost fork cap"));
    }
    let inherited_version = BranchEffectiveReadBound::for_inherited_layer(
        BranchReadBound::at_version(CommitVersion::MAX),
        fork_version,
    );
    if inherited_version.max_commit_version() != Some(fork_version) {
        return Err(TestkitError::new("inherited version did not cap at fork"));
    }
    let inherited_timestamp = BranchEffectiveReadBound::for_inherited_layer(
        BranchReadBound::at_timestamp(timestamp),
        fork_version,
    );
    let inherited_match = inherited_timestamp.matches_row(&row);
    if inherited_timestamp.max_commit_version() != Some(fork_version)
        || inherited_timestamp.max_commit_timestamp() != Some(timestamp)
        || inherited_match.matches_effective_bound()
        || !inherited_match.timestamp_in_bound()
    {
        return Err(TestkitError::new(
            "inherited timestamp bound did not combine timestamp and fork caps",
        ));
    }

    let put_candidate =
        BranchRowCandidateFacts::from_row(&row, BranchRowSource::Active, own_timestamp);
    if put_candidate.is_tombstone()
        || put_candidate.expires_at() != row.expires_at()
        || !put_candidate.bound_match().matches_effective_bound()
    {
        return Err(TestkitError::new("put candidate facts drifted"));
    }
    let tombstone_candidate = BranchRowCandidateFacts::from_row(
        &tombstone,
        BranchRowSource::Frozen { index: 0 },
        own_timestamp,
    );
    if !tombstone_candidate.is_tombstone()
        || tombstone_candidate.source() != (BranchRowSource::Frozen { index: 0 })
        || !tombstone_candidate.bound_match().matches_effective_bound()
    {
        return Err(TestkitError::new("tombstone candidate facts drifted"));
    }

    Ok(BoundsOutcome {
        own_bounds: 3,
        inherited_bounds: 3,
        candidate_puts: 1,
        candidate_tombstones: 1,
    })
}

fn check_edge_rows_and_encoded_grouping(script: &[u8]) -> Result<EdgeOutcome, TestkitError> {
    let source = branch_id(script_byte(script, 44));
    let target = branch_id(script_byte(script, 44).wrapping_add(1));
    let storage_owned = StorageRow::put(
        physical_key_with_space(
            source,
            "system",
            StorageSpaceId::COMMIT_TIMELINE,
            Vec::new(),
        )?,
        CommitVersion::MAX,
        Timestamp::MAX,
        Timestamp::MAX,
        Vec::new(),
    );
    let rewritten_storage_owned = rewrite_row_branch(&storage_owned, source, target)
        .map_err(|err| TestkitError::new(format!("storage-owned row rewrite failed: {err}")))?;
    if rewritten_storage_owned.physical_key().branch_id() != target
        || rewritten_storage_owned.physical_key().space() != "system"
        || rewritten_storage_owned.physical_key().storage_space_id()
            != StorageSpaceId::COMMIT_TIMELINE
        || !rewritten_storage_owned.physical_key().user_key().is_empty()
        || rewritten_storage_owned.commit_version() != CommitVersion::MAX
        || rewritten_storage_owned.commit_timestamp() != Timestamp::MAX
        || rewritten_storage_owned.expires_at() != Timestamp::MAX
        || !rewritten_storage_owned.value().is_empty()
    {
        return Err(TestkitError::new(
            "storage-owned empty-key row rewrite changed edge facts",
        ));
    }

    let shared_key = vec![script_byte(script, 45), 0x00, script_byte(script, 46)];
    let storage_space = StorageSpaceId::engine(0x21)
        .map_err(|err| TestkitError::new(format!("storage space failed: {err}")))?;
    let inherited = storage_row_with_space(
        source,
        "default",
        storage_space,
        shared_key.clone(),
        7,
        70,
        Timestamp::EPOCH,
        vec![script_byte(script, 47)],
    )?;
    let child_local = storage_row_with_space(
        target,
        "default",
        storage_space,
        shared_key,
        5,
        50,
        Timestamp::EPOCH,
        vec![script_byte(script, 48)],
    )?;
    let rewritten = rewrite_row_branch(&inherited, source, target)
        .map_err(|err| TestkitError::new(format!("inherited row rewrite failed: {err}")))?;

    let rewritten_prefix = TablePhysicalKeyBytes::from_row(&rewritten);
    let child_prefix = TablePhysicalKeyBytes::from_row(&child_local);
    if rewritten_prefix.as_slice() != child_prefix.as_slice() {
        return Err(TestkitError::new(
            "rewritten inherited row did not group with child-local physical key",
        ));
    }

    let mut rows = vec![TableRow::new(child_local), TableRow::new(rewritten)];
    sort_table_rows_by_key(&mut rows);
    if row_versions(&rows) != vec![7, 5] {
        return Err(TestkitError::new(
            "rewritten inherited row did not sort as newest version in child group",
        ));
    }

    Ok(EdgeOutcome {
        edge_rows: 1,
        encoded_grouping: 1,
    })
}

fn check_row_chains_and_fork_edges(script: &[u8]) -> Result<ChainOutcome, TestkitError> {
    let branch = branch_id(script_byte(script, 49));
    let wrong_branch = branch_id(script_byte(script, 49).wrapping_add(1));
    let key = vec![script_byte(script, 50), 0x00, script_byte(script, 51)];
    let mut rows = vec![
        TableRow::new(storage_row_with(
            branch,
            key.clone(),
            3,
            30,
            Timestamp::from_micros(25),
            vec![script_byte(script, 52)],
        )?),
        TableRow::new(storage_row_with(
            branch,
            key.clone(),
            5,
            50,
            Timestamp::EPOCH,
            vec![script_byte(script, 53)],
        )?),
        TableRow::new(tombstone_row(branch, key.clone(), 4, 40)?),
        TableRow::new(storage_row_with(
            branch,
            key,
            2,
            60,
            Timestamp::EPOCH,
            vec![script_byte(script, 54)],
        )?),
    ];
    sort_table_rows_by_key(&mut rows);
    if row_versions(&rows) != vec![5, 4, 3, 2] {
        return Err(TestkitError::new(
            "row chain did not preserve descending version order",
        ));
    }

    let version_bound = BranchEffectiveReadBound::new(Some(CommitVersion::new(4)), None);
    if matching_versions(&rows, version_bound) != vec![4, 3, 2] {
        return Err(TestkitError::new(
            "version bound did not filter row chain inclusively",
        ));
    }
    let timestamp_bound = BranchEffectiveReadBound::new(None, Some(Timestamp::from_micros(40)));
    if matching_versions(&rows, timestamp_bound) != vec![4, 3] {
        return Err(TestkitError::new(
            "timestamp bound did not filter row chain inclusively",
        ));
    }
    let combined_bound = BranchEffectiveReadBound::new(
        Some(CommitVersion::new(4)),
        Some(Timestamp::from_micros(40)),
    );
    let combined = matching_versions(&rows, combined_bound);
    if combined != vec![4, 3] {
        return Err(TestkitError::new(
            "combined row-chain bound did not intersect version and timestamp caps",
        ));
    }

    let candidates = rows
        .iter()
        .map(|row| {
            BranchRowCandidateFacts::from_row(row.row(), BranchRowSource::Active, combined_bound)
        })
        .filter(|candidate| candidate.bound_match().matches_effective_bound())
        .collect::<Vec<_>>();
    if candidates.len() != 2
        || !candidates.iter().any(BranchRowCandidateFacts::is_tombstone)
        || !candidates
            .iter()
            .any(|candidate| candidate.expires_at() == Timestamp::from_micros(25))
    {
        return Err(TestkitError::new(
            "row-chain candidates collapsed tombstone or expiry facts",
        ));
    }

    let wrong_row = storage_row(wrong_branch, 4)?;
    if !matches!(
        require_row_branch(branch, &wrong_row),
        Err(BranchRuntimeError::InvalidBranchRow { .. })
    ) {
        return Err(TestkitError::new(
            "row-chain branch preflight accepted a wrong-branch row",
        ));
    }

    check_fork_edge_bounds()?;
    Ok(ChainOutcome {
        row_chains: 1,
        fork_edges: 4,
    })
}

fn check_fork_edge_bounds() -> Result<(), TestkitError> {
    let fork_version = CommitVersion::new(4);
    let before_fork = BranchEffectiveReadBound::for_inherited_layer(
        BranchReadBound::at_version(CommitVersion::new(3)),
        fork_version,
    );
    let at_fork = BranchEffectiveReadBound::for_inherited_layer(
        BranchReadBound::at_version(CommitVersion::new(4)),
        fork_version,
    );
    let after_fork = BranchEffectiveReadBound::for_inherited_layer(
        BranchReadBound::at_version(CommitVersion::new(5)),
        fork_version,
    );
    let latest =
        BranchEffectiveReadBound::for_inherited_layer(BranchReadBound::latest(), fork_version);
    if before_fork.max_commit_version() != Some(CommitVersion::new(3))
        || at_fork.max_commit_version() != Some(fork_version)
        || after_fork.max_commit_version() != Some(fork_version)
        || latest.max_commit_version() != Some(fork_version)
    {
        return Err(TestkitError::new(
            "inherited fork edge bounds did not cap requested versions correctly",
        ));
    }
    Ok(())
}
