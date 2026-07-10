fn check_branch_compaction(script: &[u8]) -> Result<CompactionOutcome, TestkitError> {
    let mut outcome = CompactionOutcome::default();
    check_compaction_noops_and_invalid_requests(script, &mut outcome)?;
    check_compaction_pruning_rejections(script, &mut outcome)?;
    check_l0_keep_all_compaction_parity_and_release(script, &mut outcome)?;
    check_compaction_output_splitting(script, &mut outcome)?;
    check_l0_to_l1_compaction_candidate(script, &mut outcome)?;
    check_nonzero_level_compaction_candidate(script, &mut outcome)?;
    check_stale_compaction_plan_rejection(script, &mut outcome)?;
    Ok(outcome)
}

fn check_compaction_noops_and_invalid_requests(
    script: &[u8],
    outcome: &mut CompactionOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 160));
    let mut state = BranchLocalState::new(branch, compaction_runtime_config()?)
        .map_err(|err| TestkitError::new(format!("compaction empty state failed: {err}")))?;
    let request = branch_compaction_request(
        branch,
        BranchCompactionKind::CompactL0,
        "generated-compact-empty",
    )?;
    let no_candidate = state
        .compact_branch_owned_tables(&request)
        .map_err(|err| TestkitError::new(format!("empty compaction failed: {err}")))?;
    if no_candidate.noop_reason() != Some(BranchCompactionNoopReason::EmptyInputLevel)
        || !no_candidate.output_refs().is_empty()
        || !no_candidate.removed_refs().is_empty()
    {
        return Err(TestkitError::new("empty compaction noop facts drifted"));
    }
    outcome.compaction_noop_cases += 1;

    let wrong_branch = branch_id(script_byte(script, 160).wrapping_add(1));
    expect_invalid_compaction(state.plan_branch_compaction(&branch_compaction_request(
        wrong_branch,
        BranchCompactionKind::CompactL0,
        "generated-compact-wrong-branch",
    )?))?;
    outcome.invalid_compaction_request_rejection_cases += 1;
    Ok(())
}

fn check_compaction_pruning_rejections(
    script: &[u8],
    outcome: &mut CompactionOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 161));
    let mut state = compaction_state_with_l0_tables(
        branch,
        "generated-prune",
        vec![
            vec![storage_row_with(
                branch,
                b"generated-prune-a".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                vec![script_byte(script, 162)],
            )?],
            vec![tombstone_row(branch, b"generated-prune-b".to_vec(), 3, 30)?],
        ],
    )?;
    let before = state.clone();

    for (policy, counter) in [
        (
            BranchCompactionRetentionPolicy::DropOlderVersions,
            &mut outcome.unsafe_old_version_pruning_rejection_cases,
        ),
        (
            BranchCompactionRetentionPolicy::DropTombstones,
            &mut outcome.unsafe_tombstone_pruning_rejection_cases,
        ),
        (
            BranchCompactionRetentionPolicy::DropExpired,
            &mut outcome.unsafe_ttl_pruning_rejection_cases,
        ),
    ] {
        let request = branch_compaction_request(
            branch,
            BranchCompactionKind::CompactL0,
            format!("generated-prune-{policy:?}").to_ascii_lowercase(),
        )?
        .with_retention_policy(policy);
        expect_invalid_compaction(state.compact_branch_owned_tables(&request))?;
        if state != before {
            return Err(TestkitError::new(
                "unsafe compaction pruning rejection mutated state",
            ));
        }
        *counter += 1;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn check_l0_keep_all_compaction_parity_and_release(
    script: &[u8],
    outcome: &mut CompactionOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 163));
    let read_key = physical_key(branch, b"generated-compact-live".to_vec())?;
    let deleted_key = physical_key(branch, b"generated-compact-delete".to_vec())?;
    let prefix = physical_key(branch, b"generated-compact-scan-".to_vec())?;
    let lower = physical_key(branch, b"generated-compact-live".to_vec())?;
    let upper = physical_key(branch, b"generated-compact-scan-z".to_vec())?;

    let mut state = compaction_state_with_l0_tables(
        branch,
        "generated-keep",
        vec![
            vec![
                storage_row_with(
                    branch,
                    b"generated-compact-live".to_vec(),
                    6,
                    60,
                    Timestamp::EPOCH,
                    b"new".to_vec(),
                )?,
                tombstone_row(branch, b"generated-compact-delete".to_vec(), 7, 70)?,
                storage_row_with(
                    branch,
                    b"generated-compact-scan-a".to_vec(),
                    4,
                    40,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 164)],
                )?,
            ],
            vec![
                storage_row_with(
                    branch,
                    b"generated-compact-live".to_vec(),
                    2,
                    20,
                    Timestamp::EPOCH,
                    b"old".to_vec(),
                )?,
                storage_row_with(
                    branch,
                    b"generated-compact-delete".to_vec(),
                    3,
                    30,
                    Timestamp::EPOCH,
                    b"deleted".to_vec(),
                )?,
                storage_row_with(
                    branch,
                    b"generated-compact-scan-z".to_vec(),
                    5,
                    50,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 165)],
                )?,
            ],
        ],
    )?;
    let before_snapshot = state
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("pre-compact reachability failed: {err}")))?;
    let before_view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("pre-compact view failed: {err}")))?;
    let before_latest = visible_row(&before_view, &read_key, BranchReadBound::latest())?;
    let before_version = visible_row(
        &before_view,
        &read_key,
        BranchReadBound::at_version(CommitVersion::new(2)),
    )?;
    let before_timestamp = visible_row(
        &before_view,
        &read_key,
        BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
    )?;
    let before_history = history_versions(
        &before_view
            .history(&read_key, BranchHistoryOptions::all())
            .map_err(|err| TestkitError::new(format!("pre-compact history failed: {err}")))?,
    );
    let before_prefix = visible_user_keys(
        &before_view
            .scan_prefix(
                &BranchScanBounds::prefix(&prefix),
                BranchReadBound::latest(),
            )
            .map_err(|err| TestkitError::new(format!("pre-compact prefix failed: {err}")))?,
    );
    let before_range = visible_user_keys(
        &before_view
            .scan_range(
                &BranchScanBounds::closed(&lower, &upper).map_err(|err| {
                    TestkitError::new(format!("compact range bound failed: {err}"))
                })?,
                BranchReadBound::latest(),
            )
            .map_err(|err| TestkitError::new(format!("pre-compact range failed: {err}")))?,
    );
    if before_view
        .latest(&deleted_key)
        .map_err(|err| TestkitError::new(format!("pre-compact tombstone failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new("pre-compact tombstone did not shadow"));
    }

    let request = branch_compaction_request(
        branch,
        BranchCompactionKind::CompactL0,
        "generated-compact-keep",
    )?;
    let plan = state
        .plan_branch_compaction(&request)
        .map_err(|err| TestkitError::new(format!("L0 compaction plan failed: {err}")))?;
    let candidate = plan
        .candidate()
        .ok_or_else(|| TestkitError::new("L0 compaction missed candidate"))?;
    if candidate.input_refs().len() != 2
        || !candidate.overlap_refs().is_empty()
        || candidate.output_level() != BranchLevel::ZERO
    {
        return Err(TestkitError::new("L0 compaction candidate facts drifted"));
    }
    outcome.l0_compaction_candidate_cases += 1;

    let compaction = state
        .install_branch_compaction_plan(&request, &plan)
        .map_err(|err| TestkitError::new(format!("L0 compaction install failed: {err}")))?;
    if !compaction.installed_replacement_tables()
        || compaction.output_refs().is_empty()
        || compaction.removed_refs().len() != 2
    {
        return Err(TestkitError::new("L0 compaction outcome facts drifted"));
    }
    let report = compaction
        .table_report()
        .ok_or_else(|| TestkitError::new("installed compaction missed table report"))?;
    if report.input_sources() != 2
        || report.input_rows() != 6
        || report.kept_rows() != 6
        || report.dropped_rows() != 0
        || report.output_tables() != compaction.output_refs().len()
    {
        return Err(TestkitError::new("keep-all compaction report drifted"));
    }
    outcome.keep_all_compaction_cases += 1;
    outcome.compaction_output_install_cases += 1;

    let after_snapshot = state
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("post-compact reachability failed: {err}")))?;
    let after_view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("post-compact view failed: {err}")))?;
    if visible_row(&after_view, &read_key, BranchReadBound::latest())? != before_latest {
        return Err(TestkitError::new("compaction latest read parity drifted"));
    }
    outcome.compaction_latest_parity_cases += 1;
    if visible_row(
        &after_view,
        &read_key,
        BranchReadBound::at_version(CommitVersion::new(2)),
    )? != before_version
    {
        return Err(TestkitError::new("compaction version read parity drifted"));
    }
    outcome.compaction_version_parity_cases += 1;
    if visible_row(
        &after_view,
        &read_key,
        BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
    )? != before_timestamp
    {
        return Err(TestkitError::new(
            "compaction timestamp read parity drifted",
        ));
    }
    outcome.compaction_timestamp_parity_cases += 1;
    let after_history = history_versions(
        &after_view
            .history(&read_key, BranchHistoryOptions::all())
            .map_err(|err| TestkitError::new(format!("post-compact history failed: {err}")))?,
    );
    if after_history != before_history {
        return Err(TestkitError::new("compaction history parity drifted"));
    }
    outcome.compaction_history_parity_cases += 1;
    let after_prefix = visible_user_keys(
        &after_view
            .scan_prefix(
                &BranchScanBounds::prefix(&prefix),
                BranchReadBound::latest(),
            )
            .map_err(|err| TestkitError::new(format!("post-compact prefix failed: {err}")))?,
    );
    if after_prefix != before_prefix {
        return Err(TestkitError::new("compaction prefix scan parity drifted"));
    }
    outcome.compaction_prefix_scan_parity_cases += 1;
    let after_range = visible_user_keys(
        &after_view
            .scan_range(
                &BranchScanBounds::closed(&lower, &upper).map_err(|err| {
                    TestkitError::new(format!("post-compact range bound failed: {err}"))
                })?,
                BranchReadBound::latest(),
            )
            .map_err(|err| TestkitError::new(format!("post-compact range failed: {err}")))?,
    );
    if after_range != before_range {
        return Err(TestkitError::new("compaction range scan parity drifted"));
    }
    outcome.compaction_range_scan_parity_cases += 1;
    if visible_row(&before_view, &read_key, BranchReadBound::latest())? != before_latest {
        return Err(TestkitError::new(
            "pinned pre-compact view drifted after install",
        ));
    }
    outcome.compaction_pinned_view_isolation_cases += 1;

    let aggregate_after =
        BranchReachabilityAggregate::from_snapshots(std::slice::from_ref(&after_snapshot))
            .map_err(|err| TestkitError::new(format!("post-compact aggregate failed: {err}")))?;
    let release = BranchReleasePlan::from_removed_refs(
        branch,
        compaction.removed_refs().to_vec(),
        &aggregate_after,
        Some(&SharedTableRegistry::new()),
    )
    .map_err(|err| TestkitError::new(format!("compaction release failed: {err}")))?;
    if release.releasable_tables().len() != compaction.removed_refs().len()
        || !release.protected_tables().is_empty()
    {
        return Err(TestkitError::new("compaction release candidate drifted"));
    }
    outcome.compaction_release_candidate_cases += 1;

    let mut runtime_registry = SharedTableRegistry::new();
    runtime_registry
        .register_snapshot(&before_snapshot)
        .map_err(|err| TestkitError::new(format!("protected registry failed: {err}")))?;
    let protected = BranchReleasePlan::from_removed_refs(
        branch,
        compaction.removed_refs().to_vec(),
        &aggregate_after,
        Some(&runtime_registry),
    )
    .map_err(|err| TestkitError::new(format!("protected compaction release failed: {err}")))?;
    if protected.protected_tables().len() != compaction.removed_refs().len()
        || !protected.releasable_tables().is_empty()
    {
        return Err(TestkitError::new("compaction protected release drifted"));
    }
    outcome.compaction_protected_release_cases += 1;
    Ok(())
}

fn check_compaction_output_splitting(
    script: &[u8],
    outcome: &mut CompactionOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 166));
    let mut state = compaction_state_with_l0_tables(
        branch,
        "generated-split",
        vec![
            vec![storage_row_with(
                branch,
                b"generated-split-a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                vec![script_byte(script, 167), 0x01],
            )?],
            vec![storage_row_with(
                branch,
                b"generated-split-b".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                vec![script_byte(script, 168), 0x02],
            )?],
        ],
    )?;
    let config = TableCompactionConfig::new(1, 8)
        .map_err(|err| TestkitError::new(format!("split compaction config failed: {err}")))?;
    let request = branch_compaction_request(
        branch,
        BranchCompactionKind::CompactL0,
        "generated-compact-split",
    )?
    .with_table_compaction_config(config);
    let compaction = state
        .compact_branch_owned_tables(&request)
        .map_err(|err| TestkitError::new(format!("split compaction failed: {err}")))?;
    let report = compaction
        .table_report()
        .ok_or_else(|| TestkitError::new("split compaction missed report"))?;
    if compaction.output_refs().len() < 2 || report.split_count() == 0 {
        return Err(TestkitError::new(
            "compaction output split was not exercised",
        ));
    }
    outcome.compaction_output_split_cases += 1;
    Ok(())
}

fn check_l0_to_l1_compaction_candidate(
    script: &[u8],
    outcome: &mut CompactionOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 169));
    let mut state = BranchLocalState::new(branch, compaction_runtime_config()?)
        .map_err(|err| TestkitError::new(format!("L0-to-L1 state failed: {err}")))?;
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "generated-l0l1-overlap",
                vec![storage_row_with(
                    branch,
                    b"generated-l0l1-key".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 170)],
                )?],
            )?,
        )
        .map_err(|err| TestkitError::new(format!("L1 overlap install failed: {err}")))?;
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "generated-l0l1-input",
            vec![storage_row_with(
                branch,
                b"generated-l0l1-key".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                vec![script_byte(script, 171)],
            )?],
        )?)
        .map_err(|err| TestkitError::new(format!("L0 input install failed: {err}")))?;
    let request = branch_compaction_request(
        branch,
        BranchCompactionKind::CompactL0ToLevelOne,
        "generated-compact-l0l1",
    )?;
    let plan = state
        .plan_branch_compaction(&request)
        .map_err(|err| TestkitError::new(format!("L0-to-L1 plan failed: {err}")))?;
    let candidate = plan
        .candidate()
        .ok_or_else(|| TestkitError::new("L0-to-L1 compaction missed candidate"))?;
    if candidate.input_refs().len() != 1
        || candidate.overlap_refs().len() != 1
        || candidate.output_level() != BranchLevel::new(1)
    {
        return Err(TestkitError::new("L0-to-L1 candidate facts drifted"));
    }
    outcome.l0_to_l1_compaction_candidate_cases += 1;
    Ok(())
}

fn check_nonzero_level_compaction_candidate(
    script: &[u8],
    outcome: &mut CompactionOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 172));
    let mut state = BranchLocalState::new(branch, compaction_runtime_config()?)
        .map_err(|err| TestkitError::new(format!("nonzero compaction state failed: {err}")))?;
    state
        .install_owned_table_at_level(
            BranchLevel::new(2),
            branch_owned_table(
                branch,
                BranchLevel::new(2),
                "generated-nonzero-overlap",
                vec![storage_row_with(
                    branch,
                    b"generated-nonzero-key".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 173)],
                )?],
            )?,
        )
        .map_err(|err| TestkitError::new(format!("L2 overlap install failed: {err}")))?;
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "generated-nonzero-input",
                vec![storage_row_with(
                    branch,
                    b"generated-nonzero-key".to_vec(),
                    2,
                    20,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 174)],
                )?],
            )?,
        )
        .map_err(|err| TestkitError::new(format!("L1 input install failed: {err}")))?;
    let request = branch_compaction_request(
        branch,
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::new(1),
            table_index: 0,
        },
        "generated-compact-nonzero",
    )?;
    let plan = state
        .plan_branch_compaction(&request)
        .map_err(|err| TestkitError::new(format!("nonzero compaction plan failed: {err}")))?;
    let candidate = plan
        .candidate()
        .ok_or_else(|| TestkitError::new("nonzero compaction missed candidate"))?;
    if candidate.input_refs().len() != 1
        || candidate.overlap_refs().len() != 1
        || candidate.output_level() != BranchLevel::new(2)
    {
        return Err(TestkitError::new(
            "nonzero compaction candidate facts drifted",
        ));
    }
    outcome.nonzero_level_compaction_candidate_cases += 1;
    Ok(())
}

fn check_stale_compaction_plan_rejection(
    script: &[u8],
    outcome: &mut CompactionOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 175));
    let mut state = compaction_state_with_l0_tables(
        branch,
        "generated-stale",
        vec![
            vec![storage_row_with(
                branch,
                b"generated-stale-a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                vec![script_byte(script, 176)],
            )?],
            vec![storage_row_with(
                branch,
                b"generated-stale-b".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                vec![script_byte(script, 177)],
            )?],
        ],
    )?;
    let request = branch_compaction_request(
        branch,
        BranchCompactionKind::CompactL0,
        "generated-compact-stale",
    )?;
    let plan = state
        .plan_branch_compaction(&request)
        .map_err(|err| TestkitError::new(format!("stale compaction plan failed: {err}")))?;
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "generated-stale-newer",
            vec![storage_row_with(
                branch,
                b"generated-stale-c".to_vec(),
                3,
                30,
                Timestamp::EPOCH,
                vec![script_byte(script, 178)],
            )?],
        )?)
        .map_err(|err| TestkitError::new(format!("stale plan mutation failed: {err}")))?;
    let before_install = state.clone();
    expect_invalid_compaction(state.install_branch_compaction_plan(&request, &plan))?;
    if state != before_install {
        return Err(TestkitError::new(
            "stale compaction rejection mutated state",
        ));
    }
    outcome.stale_candidate_rejection_cases += 1;
    Ok(())
}
