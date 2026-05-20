fn check_branch_snapshot_install(script: &[u8]) -> Result<SnapshotInstallOutcome, TestkitError> {
    let mut outcome = SnapshotInstallOutcome::default();
    check_snapshot_empty_install(&mut outcome)?;
    check_snapshot_single_branch_install(script, &mut outcome)?;
    check_snapshot_multi_branch_install(script, &mut outcome)?;
    check_snapshot_invalid_requests(script, &mut outcome)?;
    check_snapshot_table_build_failure(script, &mut outcome)?;
    Ok(outcome)
}

fn check_snapshot_empty_install(outcome: &mut SnapshotInstallOutcome) -> Result<(), TestkitError> {
    let branch = branch_id(180);
    let mut branches = vec![BranchLocalState::empty(branch)];
    let before = branches.clone();
    let request =
        BranchSnapshotInstallRequest::from_rows("generated-snapshot-empty", Vec::new())
            .map_err(|err| TestkitError::new(format!("empty snapshot request failed: {err}")))?;
    let install = install_snapshot_rows_into_branches(&mut branches, &request)
        .map_err(|err| TestkitError::new(format!("empty snapshot install failed: {err}")))?;
    if install.recovery() != BranchSnapshotInstallRecovery::EmptyPlanNoop
        || install.rows_installed() != 0
        || install.tables_created() != 0
        || !install.branch_outcomes().is_empty()
        || branches != before
    {
        return Err(TestkitError::new("empty snapshot install facts drifted"));
    }
    outcome.snapshot_empty_install_noop_cases += 1;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn check_snapshot_single_branch_install(
    script: &[u8],
    outcome: &mut SnapshotInstallOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(181);
    let empty_key = physical_key(branch, Vec::new())?;
    let live_key = physical_key(branch, b"generated-snapshot-live".to_vec())?;
    let tombstone_key = physical_key(branch, b"generated-snapshot-deleted".to_vec())?;
    let ttl_key = physical_key(branch, b"generated-snapshot-ttl".to_vec())?;
    let large_key = physical_key(branch, b"generated-snapshot-large".to_vec())?;
    let max_timestamp_key = physical_key(branch, b"generated-snapshot-max-timestamp".to_vec())?;
    let lower = physical_key(branch, b"generated-snapshot-scan-a".to_vec())?;
    let upper = physical_key(branch, b"generated-snapshot-scan-z".to_vec())?;
    let alt_space = StorageSpaceId::engine(0x21)
        .map_err(|err| TestkitError::new(format!("snapshot alternate space failed: {err}")))?;
    let alt_key = physical_key_with_space(
        branch,
        "alternate",
        alt_space,
        vec![0xff, script_byte(script, 179), 0x00],
    )?;
    let live_old = storage_row_with(
        branch,
        b"generated-snapshot-live".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 180), 0x01],
    )?;
    let live_new = storage_row_with(
        branch,
        b"generated-snapshot-live".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        vec![script_byte(script, 181), 0x03],
    )?;
    let tombstone_put = storage_row_with(
        branch,
        b"generated-snapshot-deleted".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"will-delete".to_vec(),
    )?;
    let tombstone = tombstone_row(branch, b"generated-snapshot-deleted".to_vec(), 4, 40)?;
    let ttl_row = storage_row_with(
        branch,
        b"generated-snapshot-ttl".to_vec(),
        5,
        35,
        Timestamp::from_micros(50),
        b"ttl".to_vec(),
    )?;
    let empty_key_row = storage_row_with(branch, Vec::new(), 10, 5, Timestamp::EPOCH, Vec::new())?;
    let large_value_row = storage_row_with(
        branch,
        b"generated-snapshot-large".to_vec(),
        11,
        110,
        Timestamp::EPOCH,
        vec![script_byte(script, 186); 8 * 1024],
    )?;
    let max_timestamp_row = storage_row_with(
        branch,
        b"generated-snapshot-max-timestamp".to_vec(),
        12,
        u64::MAX,
        Timestamp::MAX,
        vec![script_byte(script, 187), 0xff],
    )?;
    let scan_a = storage_row_with(
        branch,
        b"generated-snapshot-scan-a".to_vec(),
        6,
        60,
        Timestamp::EPOCH,
        Vec::new(),
    )?;
    let scan_b = storage_row_with(
        branch,
        b"generated-snapshot-scan-b".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        vec![script_byte(script, 182)],
    )?;
    let scan_z = storage_row_with(
        branch,
        b"generated-snapshot-scan-z".to_vec(),
        13,
        130,
        Timestamp::EPOCH,
        vec![script_byte(script, 188)],
    )?;
    let high_bit = storage_row_with(
        branch,
        vec![0xff, script_byte(script, 183), 0x00],
        8,
        80,
        Timestamp::EPOCH,
        vec![0x80, script_byte(script, 184)],
    )?;
    let alt_row = storage_row_with_space(
        branch,
        "alternate",
        alt_space,
        alt_key.user_key().to_vec(),
        9,
        90,
        Timestamp::EPOCH,
        vec![script_byte(script, 185), 0x09],
    )?;
    let rows = sorted_snapshot_rows(vec![
        live_old.clone(),
        live_new.clone(),
        tombstone_put,
        tombstone.clone(),
        ttl_row.clone(),
        empty_key_row.clone(),
        large_value_row.clone(),
        max_timestamp_row.clone(),
        scan_a.clone(),
        scan_b.clone(),
        scan_z.clone(),
        high_bit.clone(),
        alt_row.clone(),
    ]);
    let request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-single",
        vec![BranchSnapshotInstallGroup::new(branch, rows)],
    )
    .map_err(|err| TestkitError::new(format!("single snapshot request failed: {err}")))?
    .with_max_rows_per_table(3)
    .map_err(|err| TestkitError::new(format!("single snapshot chunk config failed: {err}")))?;
    let mut branches = vec![BranchLocalState::empty(branch)];
    let pinned_before = branches[0]
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("pre-snapshot read view failed: {err}")))?;
    let install = install_snapshot_rows_into_branches(&mut branches, &request)
        .map_err(|err| TestkitError::new(format!("single snapshot install failed: {err}")))?;
    let installed = branch_state_by_id(&branches, branch)?;
    if install.recovery() != BranchSnapshotInstallRecovery::Installed
        || install.rows_installed() != 13
        || install.tables_created() != 5
        || install.branches_replaced() != 1
        || install.branches_created() != 0
        || installed.active_row_count() != 0
        || installed.owned_table_count() != 5
    {
        return Err(TestkitError::new("single snapshot install facts drifted"));
    }
    outcome.snapshot_single_branch_install_cases += 1;

    let view = installed
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("post-snapshot read view failed: {err}")))?;
    if visible_row(&view, &live_key, BranchReadBound::latest())? != Some(live_new.clone()) {
        return Err(TestkitError::new("snapshot latest read parity drifted"));
    }
    outcome.snapshot_latest_parity_cases += 1;
    if visible_row(
        &view,
        &live_key,
        BranchReadBound::at_version(CommitVersion::new(1)),
    )? != Some(live_old)
    {
        return Err(TestkitError::new("snapshot version read parity drifted"));
    }
    outcome.snapshot_version_parity_cases += 1;
    if visible_row(
        &view,
        &live_key,
        BranchReadBound::at_timestamp(Timestamp::from_micros(20)),
    )? != Some(storage_row_with(
        branch,
        b"generated-snapshot-live".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 180), 0x01],
    )?) {
        return Err(TestkitError::new("snapshot timestamp read parity drifted"));
    }
    outcome.snapshot_timestamp_parity_cases += 1;
    if history_versions(
        &view
            .history(&live_key, BranchHistoryOptions::all())
            .map_err(|err| TestkitError::new(format!("snapshot history failed: {err}")))?,
    ) != vec![3, 1]
    {
        return Err(TestkitError::new("snapshot history parity drifted"));
    }
    outcome.snapshot_history_parity_cases += 1;

    let prefix =
        BranchScanBounds::prefix(&physical_key(branch, b"generated-snapshot-scan-".to_vec())?);
    if visible_user_keys(
        &view
            .scan_prefix(&prefix, BranchReadBound::latest())
            .map_err(|err| TestkitError::new(format!("snapshot prefix scan failed: {err}")))?,
    ) != vec![
        b"generated-snapshot-scan-a".to_vec(),
        b"generated-snapshot-scan-b".to_vec(),
        b"generated-snapshot-scan-z".to_vec(),
    ] {
        return Err(TestkitError::new("snapshot prefix scan parity drifted"));
    }
    outcome.snapshot_prefix_scan_parity_cases += 1;
    let range = BranchScanBounds::closed(&lower, &upper)
        .map_err(|err| TestkitError::new(format!("snapshot range bounds failed: {err}")))?;
    if visible_user_keys(
        &view
            .scan_range(&range, BranchReadBound::latest())
            .map_err(|err| TestkitError::new(format!("snapshot range scan failed: {err}")))?,
    ) != vec![
        b"generated-snapshot-scan-a".to_vec(),
        b"generated-snapshot-scan-b".to_vec(),
        b"generated-snapshot-scan-z".to_vec(),
    ] {
        return Err(TestkitError::new("snapshot range scan parity drifted"));
    }
    outcome.snapshot_range_scan_parity_cases += 1;

    if visible_row(&view, &tombstone_key, BranchReadBound::latest())?.is_some()
        || history_versions(
            &view
                .history(&tombstone_key, BranchHistoryOptions::all())
                .map_err(|err| {
                    TestkitError::new(format!("snapshot tombstone history failed: {err}"))
                })?,
        ) != vec![4, 2]
    {
        return Err(TestkitError::new("snapshot tombstone preservation drifted"));
    }
    outcome.snapshot_tombstone_preservation_cases += 1;
    if visible_row(
        &view,
        &ttl_key,
        BranchReadBound::at_timestamp(Timestamp::from_micros(49)),
    )? != Some(ttl_row)
        || visible_row(
            &view,
            &ttl_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(50)),
        )?
        .is_some()
    {
        return Err(TestkitError::new("snapshot TTL preservation drifted"));
    }
    outcome.snapshot_ttl_preservation_cases += 1;
    if visible_row(&pinned_before, &live_key, BranchReadBound::latest())?.is_some()
        || visible_row(&pinned_before, &alt_key, BranchReadBound::latest())?.is_some()
    {
        return Err(TestkitError::new(
            "pre-install pinned view observed snapshot rows",
        ));
    }
    outcome.snapshot_pinned_view_isolation_cases += 1;
    let reachability = installed
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("snapshot reachability failed: {err}")))?;
    if reachability.facts().owned_table_count() != 5
        || reachability.table_refs().len() != 5
        || reachability
            .table_refs()
            .iter()
            .any(|table_ref| table_ref.reference_kind() != BranchTableReferenceKind::Owned)
    {
        return Err(TestkitError::new("snapshot reachability facts drifted"));
    }
    outcome.snapshot_reachability_cases += 1;
    if visible_row(&view, &alt_key, BranchReadBound::latest())? != Some(alt_row.clone())
        || visible_row(&view, &empty_key, BranchReadBound::latest())? != Some(empty_key_row)
        || visible_row(&view, &large_key, BranchReadBound::latest())? != Some(large_value_row)
        || visible_row(&view, high_bit.physical_key(), BranchReadBound::latest())?
            != Some(high_bit.clone())
        || visible_row(&view, &max_timestamp_key, BranchReadBound::latest())?
            != Some(max_timestamp_row)
    {
        return Err(TestkitError::new(
            "snapshot row-native boundary facts drifted",
        ));
    }
    outcome.snapshot_source_boundary_guard_cases += 1;
    Ok(())
}

fn check_snapshot_multi_branch_install(
    script: &[u8],
    outcome: &mut SnapshotInstallOutcome,
) -> Result<(), TestkitError> {
    let existing = branch_id(182);
    let created = branch_id(183);
    let untouched = branch_id(184);
    let existing_row = storage_row_with(
        existing,
        b"generated-snapshot-shared-key".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 189)],
    )?;
    let created_row = storage_row_with(
        created,
        b"generated-snapshot-shared-key".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        vec![script_byte(script, 190)],
    )?;
    let request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-multi",
        vec![
            BranchSnapshotInstallGroup::new(existing, sorted_snapshot_rows(vec![existing_row])),
            BranchSnapshotInstallGroup::new(created, sorted_snapshot_rows(vec![created_row])),
        ],
    )
    .map_err(|err| TestkitError::new(format!("multi snapshot request failed: {err}")))?
    .with_missing_branch_policy(BranchSnapshotMissingBranchPolicy::Create {
        config: BranchRuntimeConfig::default(),
    });
    let mut branches = vec![
        BranchLocalState::empty(untouched),
        BranchLocalState::empty(existing),
    ];
    let install = install_snapshot_rows_into_branches(&mut branches, &request)
        .map_err(|err| TestkitError::new(format!("multi snapshot install failed: {err}")))?;
    if install.recovery() != BranchSnapshotInstallRecovery::Installed
        || install.rows_installed() != 2
        || install.tables_created() != 2
        || install.branches_replaced() != 1
        || install.branches_created() != 1
        || branches
            .iter()
            .map(BranchLocalState::branch_id)
            .collect::<Vec<_>>()
            != vec![untouched, existing, created]
    {
        return Err(TestkitError::new("multi snapshot install facts drifted"));
    }
    outcome.snapshot_multi_branch_install_cases += 1;
    outcome.snapshot_missing_branch_create_cases += 1;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn check_snapshot_invalid_requests(
    script: &[u8],
    outcome: &mut SnapshotInstallOutcome,
) -> Result<(), TestkitError> {
    let target = branch_id(185);
    let other = branch_id(186);

    let mut missing_branches = vec![BranchLocalState::empty(other)];
    let missing_before = missing_branches.clone();
    let missing_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-missing",
        vec![BranchSnapshotInstallGroup::new(
            target,
            sorted_snapshot_rows(vec![storage_row(target, 1)?]),
        )],
    )
    .map_err(|err| TestkitError::new(format!("missing snapshot request failed: {err}")))?;
    expect_missing_snapshot_branch(
        install_snapshot_rows_into_branches(&mut missing_branches, &missing_request),
        target,
    )?;
    if missing_branches != missing_before {
        return Err(TestkitError::new(
            "missing snapshot rejection mutated state",
        ));
    }
    outcome.snapshot_missing_branch_rejection_cases += 1;

    let mut non_empty = vec![BranchLocalState::empty(target)];
    non_empty[0]
        .append_committed_row(storage_row_with(
            target,
            b"generated-snapshot-existing".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"existing".to_vec(),
        )?)
        .map_err(|err| TestkitError::new(format!("non-empty snapshot seed failed: {err}")))?;
    let non_empty_before = non_empty.clone();
    let non_empty_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-non-empty",
        vec![BranchSnapshotInstallGroup::new(
            target,
            sorted_snapshot_rows(vec![storage_row_with(
                target,
                b"generated-snapshot-new".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                vec![script_byte(script, 188)],
            )?]),
        )],
    )
    .map_err(|err| TestkitError::new(format!("non-empty snapshot request failed: {err}")))?;
    expect_invalid_snapshot_install(
        install_snapshot_rows_into_branches(&mut non_empty, &non_empty_request),
        "snapshot install target branch must be empty",
    )?;
    if non_empty != non_empty_before {
        return Err(TestkitError::new(
            "non-empty snapshot rejection mutated state",
        ));
    }
    outcome.snapshot_non_empty_target_rejection_cases += 1;

    let mut branches = vec![
        BranchLocalState::empty(target),
        BranchLocalState::empty(other),
    ];
    let before = branches.clone();

    let empty_group_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-empty-group",
        vec![BranchSnapshotInstallGroup::new(target, Vec::new())],
    )
    .map_err(|err| TestkitError::new(format!("empty-group snapshot request failed: {err}")))?;
    expect_invalid_snapshot_install(
        install_snapshot_rows_into_branches(&mut branches, &empty_group_request),
        "snapshot install branch groups must not be empty",
    )?;
    if branches != before {
        return Err(TestkitError::new(
            "empty-group snapshot rejection mutated state",
        ));
    }
    outcome.snapshot_empty_group_rejection_cases += 1;

    let duplicate_group_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-duplicate-group",
        vec![
            BranchSnapshotInstallGroup::new(
                target,
                sorted_snapshot_rows(vec![storage_row_with(
                    target,
                    b"generated-snapshot-group-a".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 189)],
                )?]),
            ),
            BranchSnapshotInstallGroup::new(
                target,
                sorted_snapshot_rows(vec![storage_row_with(
                    target,
                    b"generated-snapshot-group-b".to_vec(),
                    2,
                    20,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 190)],
                )?]),
            ),
        ],
    )
    .map_err(|err| TestkitError::new(format!("duplicate-group snapshot request failed: {err}")))?;
    expect_invalid_snapshot_install(
        install_snapshot_rows_into_branches(&mut branches, &duplicate_group_request),
        "snapshot install branch groups must be unique",
    )?;
    if branches != before {
        return Err(TestkitError::new(
            "duplicate-group snapshot rejection mutated state",
        ));
    }
    outcome.snapshot_duplicate_branch_group_rejection_cases += 1;

    let mismatch_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-mismatch",
        vec![BranchSnapshotInstallGroup::new(
            target,
            sorted_snapshot_rows(vec![storage_row(other, 1)?]),
        )],
    )
    .map_err(|err| TestkitError::new(format!("mismatch snapshot request failed: {err}")))?;
    expect_invalid_branch_row(install_snapshot_rows_into_branches(
        &mut branches,
        &mismatch_request,
    ))?;
    if branches != before {
        return Err(TestkitError::new(
            "branch-mismatch snapshot rejection mutated state",
        ));
    }
    outcome.snapshot_branch_mismatch_rejection_cases += 1;

    let duplicate = storage_row(target, 1)?;
    let duplicate_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-duplicate",
        vec![BranchSnapshotInstallGroup::new(
            target,
            vec![duplicate.clone(), duplicate],
        )],
    )
    .map_err(|err| TestkitError::new(format!("duplicate snapshot request failed: {err}")))?;
    expect_duplicate_internal_key(install_snapshot_rows_into_branches(
        &mut branches,
        &duplicate_request,
    ))?;
    if branches != before {
        return Err(TestkitError::new(
            "duplicate snapshot rejection mutated state",
        ));
    }
    outcome.snapshot_duplicate_row_rejection_cases += 1;

    let unsorted_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-unsorted",
        vec![BranchSnapshotInstallGroup::new(
            target,
            vec![
                storage_row_with(
                    target,
                    b"z-generated-snapshot".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 191)],
                )?,
                storage_row_with(
                    target,
                    b"a-generated-snapshot".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 192)],
                )?,
            ],
        )],
    )
    .map_err(|err| TestkitError::new(format!("unsorted snapshot request failed: {err}")))?;
    expect_invalid_snapshot_install(
        install_snapshot_rows_into_branches(&mut branches, &unsorted_request),
        "snapshot install rows must be strictly sorted by internal key",
    )?;
    if branches != before {
        return Err(TestkitError::new(
            "unsorted snapshot rejection mutated state",
        ));
    }
    outcome.snapshot_unsorted_row_rejection_cases += 1;

    let group_order_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-group-order",
        vec![
            BranchSnapshotInstallGroup::new(
                other,
                sorted_snapshot_rows(vec![storage_row(other, 1)?]),
            ),
            BranchSnapshotInstallGroup::new(
                target,
                sorted_snapshot_rows(vec![storage_row(target, 1)?]),
            ),
        ],
    )
    .map_err(|err| TestkitError::new(format!("group-order snapshot request failed: {err}")))?;
    expect_invalid_snapshot_install(
        install_snapshot_rows_into_branches(&mut branches, &group_order_request),
        "snapshot install branch groups must be sorted by branch id",
    )?;
    if branches != before {
        return Err(TestkitError::new(
            "group-order snapshot rejection mutated state",
        ));
    }

    let collision_existing = branch_id(189);
    let collision_target = branch_id(190);
    let collision_rows = sorted_snapshot_rows(vec![storage_row_with(
        collision_target,
        b"generated-snapshot-collision".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 193)],
    )?]);
    let collision_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-collision",
        vec![BranchSnapshotInstallGroup::new(
            collision_target,
            collision_rows.clone(),
        )],
    )
    .map_err(|err| TestkitError::new(format!("collision snapshot request failed: {err}")))?
    .with_missing_branch_policy(BranchSnapshotMissingBranchPolicy::Create {
        config: BranchRuntimeConfig::default(),
    });
    let mut dry_run = Vec::new();
    let dry_run_outcome = install_snapshot_rows_into_branches(&mut dry_run, &collision_request)
        .map_err(|err| TestkitError::new(format!("collision dry run failed: {err}")))?;
    let collision_identity = dry_run_outcome.branch_outcomes()[0].table_identities()[0].clone();
    let mut collision_branches = vec![
        BranchLocalState::empty(collision_existing),
        BranchLocalState::empty(collision_target),
    ];
    collision_branches[0]
        .install_l0_table(branch_owned_table(
            collision_existing,
            BranchLevel::ZERO,
            collision_identity.as_str(),
            vec![storage_row(collision_existing, 1)?],
        )?)
        .map_err(|err| TestkitError::new(format!("collision table seed failed: {err}")))?;
    let collision_before = collision_branches.clone();
    let collision_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-collision",
        vec![BranchSnapshotInstallGroup::new(
            collision_target,
            collision_rows,
        )],
    )
    .map_err(|err| TestkitError::new(format!("collision snapshot request failed: {err}")))?;
    expect_invalid_snapshot_install(
        install_snapshot_rows_into_branches(&mut collision_branches, &collision_request),
        "snapshot output identity must not collide with existing reachable table",
    )?;
    if collision_branches != collision_before {
        return Err(TestkitError::new(
            "identity-collision snapshot rejection mutated state",
        ));
    }
    outcome.snapshot_output_identity_collision_rejection_cases += 1;
    Ok(())
}

fn check_snapshot_table_build_failure(
    script: &[u8],
    outcome: &mut SnapshotInstallOutcome,
) -> Result<(), TestkitError> {
    let lower = branch_id(187);
    let higher = branch_id(188);
    let huge_key = vec![script_byte(script, 191).max(1); 70 * 1024];
    let request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-build-failure",
        vec![
            BranchSnapshotInstallGroup::new(
                lower,
                sorted_snapshot_rows(vec![storage_row(lower, 1)?]),
            ),
            BranchSnapshotInstallGroup::new(
                higher,
                sorted_snapshot_rows(vec![storage_row_with(
                    higher,
                    huge_key,
                    1,
                    10,
                    Timestamp::EPOCH,
                    b"secret-payload".to_vec(),
                )?]),
            ),
        ],
    )
    .map_err(|err| TestkitError::new(format!("build-failure snapshot request failed: {err}")))?;
    let debug_text = format!("{request:?}");
    if debug_text.contains("secret-payload") {
        return Err(TestkitError::new("snapshot request debug leaked row bytes"));
    }
    let mut branches = vec![
        BranchLocalState::empty(lower),
        BranchLocalState::empty(higher),
    ];
    let before = branches.clone();
    let Err(error) = install_snapshot_rows_into_branches(&mut branches, &request) else {
        return Err(TestkitError::new("oversized snapshot key was accepted"));
    };
    match error {
        BranchRuntimeError::TableRuntime { .. } => {}
        other => {
            return Err(TestkitError::new(format!(
                "snapshot table build returned wrong error: {other}"
            )))
        }
    }
    if error.to_string().contains("secret-payload") {
        return Err(TestkitError::new(
            "snapshot table build error leaked row bytes",
        ));
    }
    if branches != before {
        return Err(TestkitError::new(
            "snapshot table build failure mutated state",
        ));
    }
    outcome.snapshot_table_build_failure_atomicity_cases += 1;
    Ok(())
}

