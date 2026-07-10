fn check_branch_owned_immutable(script: &[u8]) -> Result<ImmutableOutcome, TestkitError> {
    let mut outcome = ImmutableOutcome::default();
    let branch = branch_id(script_byte(script, 100));
    check_immutable_l0_reads_and_scans(script, branch, &mut outcome)?;
    check_immutable_l1_and_invalid_installs(script, branch, &mut outcome)?;
    check_immutable_frozen_replacement(branch, &mut outcome)?;
    check_active_frozen_immutable_merge(branch, &mut outcome)?;
    Ok(outcome)
}

fn check_immutable_l0_reads_and_scans(
    script: &[u8],
    branch: BranchId,
    outcome: &mut ImmutableOutcome,
) -> Result<(), TestkitError> {
    let mut state = BranchLocalState::empty(branch);
    let pinned = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("immutable pinned capture failed: {err}")))?;
    let live = storage_row_with(
        branch,
        [b"owned-prefix-".to_vec(), user_key(script, 101)].concat(),
        3,
        30,
        Timestamp::EPOCH,
        vec![script_byte(script, 105)],
    )?;
    let old_deleted = storage_row_with(
        branch,
        b"owned-prefix-deleted".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"old".to_vec(),
    )?;
    let tombstone = tombstone_row(branch, b"owned-prefix-deleted".to_vec(), 4, 40)?;
    let high = storage_row_with(
        branch,
        vec![b'o', b'w', b'n', b'e', b'd', 0x80],
        2,
        20,
        Timestamp::EPOCH,
        b"high".to_vec(),
    )?;
    let table = branch_owned_table(
        branch,
        BranchLevel::ZERO,
        "generated-owned-l0",
        vec![live.clone(), old_deleted, tombstone, high.clone()],
    )?;
    outcome.immutable_descriptor_cases += 1;
    let install: BranchImmutableInstallOutcome = state
        .install_l0_table(table)
        .map_err(|err| TestkitError::new(format!("immutable L0 install failed: {err}")))?;
    if install.table_index() != 0 || install.owned_table_count() != 1 {
        return Err(TestkitError::new("immutable L0 install outcome drifted"));
    }
    outcome.immutable_l0_installs += 1;

    let live_key = live.physical_key().clone();
    if pinned
        .latest(&live_key)
        .map_err(|err| TestkitError::new(format!("pinned immutable read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "pinned view saw L0 install after capture",
        ));
    }
    outcome.pinned_immutable_install_isolations += 1;

    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("immutable view capture failed: {err}")))?;
    let latest = view
        .latest(&live_key)
        .map_err(|err| TestkitError::new(format!("immutable latest failed: {err}")))?
        .ok_or_else(|| TestkitError::new("immutable latest missed live row"))?;
    if latest.row() != &live || !matches!(latest.source(), BranchRowSource::OwnedTable { .. }) {
        return Err(TestkitError::new("immutable latest source drifted"));
    }
    outcome.immutable_latest_reads += 1;
    outcome.immutable_source_attributions += 1;

    if view
        .latest(&physical_key(branch, b"owned-prefix-deleted".to_vec())?)
        .map_err(|err| TestkitError::new(format!("immutable tombstone read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "immutable tombstone fell through to old put",
        ));
    }
    outcome.immutable_tombstone_shadows += 1;

    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"owned-prefix-".to_vec())?);
    let prefix_rows = view
        .scan_prefix(&prefix, BranchReadBound::latest())
        .map_err(|err| TestkitError::new(format!("immutable prefix scan failed: {err}")))?;
    if visible_user_keys(&prefix_rows) != vec![live.physical_key().user_key().to_vec()] {
        return Err(TestkitError::new("immutable prefix scan drifted"));
    }
    outcome.immutable_prefix_scans += 1;

    let range = BranchScanBounds::closed(&live_key, high.physical_key())
        .map_err(|err| TestkitError::new(format!("immutable range bounds failed: {err}")))?;
    let range_rows = view
        .scan_range(&range, BranchReadBound::latest())
        .map_err(|err| TestkitError::new(format!("immutable range scan failed: {err}")))?;
    if visible_user_keys(&range_rows)
        != vec![
            live.physical_key().user_key().to_vec(),
            high.physical_key().user_key().to_vec(),
        ]
    {
        return Err(TestkitError::new("immutable range scan drifted"));
    }
    outcome.immutable_range_scans += 1;
    Ok(())
}

fn check_immutable_l1_and_invalid_installs(
    script: &[u8],
    branch: BranchId,
    outcome: &mut ImmutableOutcome,
) -> Result<(), TestkitError> {
    let config = BranchRuntimeConfig::new(3, 64, 32)
        .map_err(|err| TestkitError::new(format!("immutable config failed: {err}")))?;
    let mut state = BranchLocalState::new(branch, config)
        .map_err(|err| TestkitError::new(format!("immutable state failed: {err}")))?;
    let level = BranchLevel::new(1);
    let first = branch_owned_table(
        branch,
        level,
        "generated-l1-a-c",
        vec![
            storage_row_with(
                branch,
                b"l1-a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                Vec::new(),
            )?,
            storage_row_with(
                branch,
                b"l1-c".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                vec![script_byte(script, 106)],
            )?,
        ],
    )?;
    let second = branch_owned_table(
        branch,
        level,
        "generated-l1-z",
        vec![storage_row_with(
            branch,
            b"l1-z".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            Vec::new(),
        )?],
    )?;
    state
        .install_owned_table_at_level(level, first)
        .map_err(|err| TestkitError::new(format!("immutable L1 first failed: {err}")))?;
    state
        .install_owned_table_at_level(level, second)
        .map_err(|err| TestkitError::new(format!("immutable L1 second failed: {err}")))?;
    outcome.immutable_l1_installs += 2;

    let before_overlap = state.clone();
    let overlap = branch_owned_table(
        branch,
        level,
        "generated-l1-overlap",
        vec![storage_row_with(
            branch,
            b"l1-b".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            Vec::new(),
        )?],
    )?;
    expect_invalid_state(state.install_owned_table_at_level(level, overlap))?;
    if state != before_overlap {
        return Err(TestkitError::new("overlapping L1 install mutated state"));
    }
    outcome.immutable_l1_overlap_rejections += 1;

    let wrong_level = branch_owned_table(
        branch,
        BranchLevel::new(2),
        "generated-wrong-level",
        vec![storage_row(branch, 8)?],
    )?;
    expect_invalid_state(state.install_owned_table_at_level(level, wrong_level))?;
    outcome.invalid_immutable_install_rejections += 1;

    let other = branch_id(script_byte(script, 100).wrapping_add(1));
    let wrong_branch = branch_owned_table(
        other,
        BranchLevel::ZERO,
        "generated-wrong-branch",
        vec![storage_row_with(
            other,
            b"wrong-branch-owned".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"secret-payload".to_vec(),
        )?],
    )?;
    expect_invalid_branch_row(state.install_l0_table(wrong_branch))?;
    outcome.invalid_immutable_install_rejections += 1;
    Ok(())
}

fn check_immutable_frozen_replacement(
    branch: BranchId,
    outcome: &mut ImmutableOutcome,
) -> Result<(), TestkitError> {
    let mut state = BranchLocalState::empty(branch);
    let row = storage_row_with(
        branch,
        b"generated-flush".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"flush".to_vec(),
    )?;
    state
        .append_committed_row(row.clone())
        .map_err(|err| TestkitError::new(format!("flush append failed: {err}")))?;
    check_successful_rotation(&mut state, 1, 1)?;
    let replacement = branch_owned_table(
        branch,
        BranchLevel::ZERO,
        "generated-flush-l0",
        vec![row.clone()],
    )?;
    let outcome_value = state
        .replace_frozen_with_l0_table(0, replacement)
        .map_err(|err| TestkitError::new(format!("frozen replacement failed: {err}")))?;
    if outcome_value.replaced_frozen_index() != Some(0)
        || state.frozen_table_count() != 0
        || state.owned_table_count() != 1
    {
        return Err(TestkitError::new("frozen replacement outcome drifted"));
    }
    outcome.frozen_replacements += 1;
    Ok(())
}

fn check_active_frozen_immutable_merge(
    branch: BranchId,
    outcome: &mut ImmutableOutcome,
) -> Result<(), TestkitError> {
    let mut state = BranchLocalState::empty(branch);
    let frozen_newer = storage_row_with(
        branch,
        b"merge-owned".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        b"frozen".to_vec(),
    )?;
    let active_older = storage_row_with(
        branch,
        b"merge-owned".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"active".to_vec(),
    )?;
    let owned_middle = storage_row_with(
        branch,
        b"merge-owned".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"owned".to_vec(),
    )?;
    state
        .append_committed_row(frozen_newer)
        .map_err(|err| TestkitError::new(format!("merge frozen append failed: {err}")))?;
    check_successful_rotation(&mut state, 1, 1)?;
    state
        .append_committed_row(active_older)
        .map_err(|err| TestkitError::new(format!("merge active append failed: {err}")))?;
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "generated-merge-owned",
            vec![owned_middle],
        )?)
        .map_err(|err| TestkitError::new(format!("merge owned install failed: {err}")))?;
    let key = physical_key(branch, b"merge-owned".to_vec())?;
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("merge view failed: {err}")))?;
    let bounded = view
        .at_version(&key, CommitVersion::new(5))
        .map_err(|err| TestkitError::new(format!("merge version read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("merge version read missed owned row"))?;
    if !matches!(bounded.source(), BranchRowSource::OwnedTable { .. }) {
        return Err(TestkitError::new(
            "merge version read did not select owned source",
        ));
    }
    if history_versions(
        &view
            .history(&key, BranchHistoryOptions::all())
            .map_err(|err| TestkitError::new(format!("merge history failed: {err}")))?,
    ) != vec![7, 5, 2]
    {
        return Err(TestkitError::new("active/frozen/immutable history drifted"));
    }
    outcome.immutable_version_bounded_reads += 1;
    outcome.immutable_history_reads += 1;
    outcome.active_frozen_immutable_merge_reads += 1;
    Ok(())
}

fn check_branch_inheritance(script: &[u8]) -> Result<InheritanceOutcome, TestkitError> {
    let mut outcome = InheritanceOutcome::default();
    let mut fixture = check_fork_capture_and_latest(script, &mut outcome)?;
    check_child_put_shadow_and_history(&mut fixture, &mut outcome)?;
    check_manual_inherited_tombstone_and_scans(&fixture, &mut outcome)?;
    check_chained_inheritance(script, &mut outcome)?;
    check_invalid_inherited_layers(fixture.child, &mut outcome)?;
    Ok(outcome)
}

struct DirectInheritanceFixture {
    source: BranchId,
    child: BranchId,
    child_key: PhysicalKey,
    rewritten_inherited: StorageRow,
    child_state: BranchLocalState,
}

struct SourceForkFixture {
    source: BranchId,
    child: BranchId,
    source_state: BranchLocalState,
    inherited: StorageRow,
}

fn build_inheritance_source(script: &[u8]) -> Result<SourceForkFixture, TestkitError> {
    let source = branch_id(script_byte(script, 108));
    let child = branch_id(script_byte(script, 108).wrapping_add(1));
    let mut source_state = BranchLocalState::empty(source);
    let inherited = storage_row_with(
        source,
        b"generated-inherited".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"source".to_vec(),
    )?;
    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "generated-inheritance-source",
            vec![inherited.clone()],
        )?)
        .map_err(|err| TestkitError::new(format!("inheritance source install failed: {err}")))?;
    Ok(SourceForkFixture {
        source,
        child,
        source_state,
        inherited,
    })
}

fn check_fork_capture_and_latest(
    script: &[u8],
    outcome: &mut InheritanceOutcome,
) -> Result<DirectInheritanceFixture, TestkitError> {
    let SourceForkFixture {
        source,
        child,
        mut source_state,
        inherited,
    } = build_inheritance_source(script)?;

    let (child_state, fork_outcome): (BranchLocalState, BranchForkOutcome) = source_state
        .fork_into_empty_child(child)
        .map_err(|err| TestkitError::new(format!("fork capture failed: {err}")))?;
    if fork_outcome.source_branch_id() != source
        || fork_outcome.destination_branch_id() != child
        || fork_outcome.fork_version() != CommitVersion::new(3)
        || fork_outcome.inherited_layer_count() != 1
        || fork_outcome.inherited_table_count() != 1
        || child_state.owned_table_count() != 0
        || child_state.inherited_layer_count() != 1
    {
        return Err(TestkitError::new("fork capture outcome drifted"));
    }
    outcome.inherited_fork_captures += 1;
    outcome.inherited_layer_validations += 1;

    let view = child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("child inherited view failed: {err}")))?;
    let child_key = physical_key(child, b"generated-inherited".to_vec())?;
    let rewritten = rewrite_row_branch(&inherited, source, child)
        .map_err(|err| TestkitError::new(format!("expected inherited rewrite failed: {err}")))?;
    let latest = view
        .latest(&child_key)
        .map_err(|err| TestkitError::new(format!("inherited latest failed: {err}")))?
        .ok_or_else(|| TestkitError::new("inherited latest missed row"))?;
    if latest.row() != &rewritten
        || latest.source()
            != (BranchRowSource::Inherited {
                source_branch_id: source,
                layer_index: 0,
            })
    {
        return Err(TestkitError::new("inherited latest/source rewrite drifted"));
    }
    outcome.inherited_latest_reads += 1;
    outcome.inherited_key_rewrites += 1;

    if view
        .latest(&physical_key(
            child,
            b"generated-source-active-only".to_vec(),
        )?)
        .map_err(|err| TestkitError::new(format!("source active-only read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "fork inherited source active row without flush",
        ));
    }
    outcome.inherited_post_fork_invisibility += 1;

    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "generated-inheritance-source-later",
            vec![storage_row_with(
                source,
                b"generated-source-later".to_vec(),
                9,
                90,
                Timestamp::EPOCH,
                b"later".to_vec(),
            )?],
        )?)
        .map_err(|err| TestkitError::new(format!("source later install failed: {err}")))?;
    if view
        .latest(&physical_key(child, b"generated-source-later".to_vec())?)
        .map_err(|err| TestkitError::new(format!("pinned inherited read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "pinned inherited view saw later source mutation",
        ));
    }
    outcome.pinned_inherited_view_isolations += 1;

    Ok(DirectInheritanceFixture {
        source,
        child,
        child_key,
        rewritten_inherited: rewritten,
        child_state,
    })
}

fn check_child_put_shadow_and_history(
    fixture: &mut DirectInheritanceFixture,
    outcome: &mut InheritanceOutcome,
) -> Result<(), TestkitError> {
    fixture
        .child_state
        .append_committed_row(storage_row_with(
            fixture.child,
            b"generated-inherited".to_vec(),
            7,
            70,
            Timestamp::EPOCH,
            b"child".to_vec(),
        )?)
        .map_err(|err| TestkitError::new(format!("child shadow put failed: {err}")))?;
    let shadow_view = fixture
        .child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("child shadow view failed: {err}")))?;
    let child_put = shadow_view
        .latest(&fixture.child_key)
        .map_err(|err| TestkitError::new(format!("child shadow latest failed: {err}")))?
        .ok_or_else(|| TestkitError::new("child shadow latest missed put"))?;
    if child_put.source() != BranchRowSource::Active {
        return Err(TestkitError::new("child put did not shadow inherited row"));
    }
    outcome.inherited_child_put_shadows += 1;
    let inherited_before_child = shadow_view
        .at_version(&fixture.child_key, CommitVersion::new(4))
        .map_err(|err| TestkitError::new(format!("inherited bounded read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("bounded inherited read missed row"))?;
    if inherited_before_child.row() != &fixture.rewritten_inherited {
        return Err(TestkitError::new("bounded inherited read drifted"));
    }
    outcome.inherited_version_bounded_reads += 1;

    let history = shadow_view
        .history(&fixture.child_key, BranchHistoryOptions::all())
        .map_err(|err| TestkitError::new(format!("inherited history failed: {err}")))?;
    if history_versions(&history) != vec![7, 3] {
        return Err(TestkitError::new("inherited history versions drifted"));
    }
    outcome.inherited_history_reads += 1;
    Ok(())
}

fn check_manual_inherited_tombstone_and_scans(
    fixture: &DirectInheritanceFixture,
    outcome: &mut InheritanceOutcome,
) -> Result<(), TestkitError> {
    let tombstone_key = physical_key(fixture.child, b"generated-delete-shadow".to_vec())?;
    let layer = branch_inherited_layer_unchecked_for_fork_gate_checks(
        fixture.source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            fixture.source,
            BranchLevel::ZERO,
            "generated-delete-shadow-source",
            vec![
                storage_row_with(
                    fixture.source,
                    b"generated-delete-shadow".to_vec(),
                    2,
                    20,
                    Timestamp::EPOCH,
                    b"source".to_vec(),
                )?,
                storage_row_with(
                    fixture.source,
                    b"generated-post-fork".to_vec(),
                    8,
                    80,
                    Timestamp::EPOCH,
                    b"post".to_vec(),
                )?,
                storage_row_with(
                    fixture.source,
                    b"generated-scan-visible".to_vec(),
                    3,
                    30,
                    Timestamp::EPOCH,
                    b"visible-scan".to_vec(),
                )?,
            ],
        )?]],
    );
    let mut tombstone_child = BranchLocalState::empty(fixture.child);
    tombstone_child
        .attach_inherited_layers(vec![layer])
        .map_err(|err| TestkitError::new(format!("manual inherited attach failed: {err}")))?;
    tombstone_child
        .append_committed_row(tombstone_row(
            fixture.child,
            b"generated-delete-shadow".to_vec(),
            6,
            60,
        )?)
        .map_err(|err| TestkitError::new(format!("child inherited tombstone failed: {err}")))?;
    let tombstone_view = tombstone_child
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("tombstone inherited view failed: {err}")))?;
    if tombstone_view
        .latest(&tombstone_key)
        .map_err(|err| TestkitError::new(format!("tombstone shadow read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "child tombstone fell through to inherited put",
        ));
    }
    outcome.inherited_child_tombstone_shadows += 1;
    if tombstone_view
        .latest(&physical_key(
            fixture.child,
            b"generated-post-fork".to_vec(),
        )?)
        .map_err(|err| TestkitError::new(format!("post-fork inherited read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new("manual fork gate exposed post-fork row"));
    }
    outcome.inherited_post_fork_invisibility += 1;

    let prefix = BranchScanBounds::prefix(&physical_key(fixture.child, b"generated-".to_vec())?);
    let prefix_rows = tombstone_view
        .scan_prefix(&prefix, BranchReadBound::latest())
        .map_err(|err| TestkitError::new(format!("inherited prefix scan failed: {err}")))?;
    if visible_user_keys(&prefix_rows) != vec![b"generated-scan-visible".to_vec()] {
        return Err(TestkitError::new(
            "inherited prefix scan did not rewrite and filter visible rows",
        ));
    }
    outcome.inherited_prefix_scans += 1;

    let range = BranchScanBounds::closed(
        &physical_key(fixture.child, b"generated-delete-shadow".to_vec())?,
        &physical_key(fixture.child, b"generated-scan-visible".to_vec())?,
    )
    .map_err(|err| TestkitError::new(format!("inherited range bounds failed: {err}")))?;
    let range_rows = tombstone_view
        .scan_range(&range, BranchReadBound::latest())
        .map_err(|err| TestkitError::new(format!("inherited range scan failed: {err}")))?;
    if visible_user_keys(&range_rows) != vec![b"generated-scan-visible".to_vec()] {
        return Err(TestkitError::new(
            "inherited range scan did not rewrite and filter visible rows",
        ));
    }
    outcome.inherited_range_scans += 1;
    Ok(())
}

fn check_chained_inheritance(
    script: &[u8],
    outcome: &mut InheritanceOutcome,
) -> Result<(), TestkitError> {
    let grandparent = branch_id(script_byte(script, 109));
    let parent = branch_id(script_byte(script, 109).wrapping_add(1));
    let child = branch_id(script_byte(script, 109).wrapping_add(2));
    let grandparent_row = storage_row_with(
        grandparent,
        b"generated-chain".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"grandparent".to_vec(),
    )?;
    let parent_row = storage_row_with(
        parent,
        b"generated-chain".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"parent".to_vec(),
    )?;
    let mut grandparent_state = BranchLocalState::empty(grandparent);
    grandparent_state
        .install_l0_table(branch_owned_table(
            grandparent,
            BranchLevel::ZERO,
            "generated-chain-grandparent",
            vec![grandparent_row],
        )?)
        .map_err(|err| TestkitError::new(format!("chain grandparent install failed: {err}")))?;
    let (mut parent_state, _) = grandparent_state
        .fork_into_empty_child(parent)
        .map_err(|err| TestkitError::new(format!("chain parent fork failed: {err}")))?;
    parent_state
        .install_l0_table(branch_owned_table(
            parent,
            BranchLevel::ZERO,
            "generated-chain-parent",
            vec![parent_row.clone()],
        )?)
        .map_err(|err| TestkitError::new(format!("chain parent install failed: {err}")))?;
    let (child_state, outcome_value) = parent_state
        .fork_into_empty_child(child)
        .map_err(|err| TestkitError::new(format!("chain child fork failed: {err}")))?;
    if outcome_value.inherited_layer_count() != 2 {
        return Err(TestkitError::new("chain inherited layer count drifted"));
    }
    let visible = child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("chain child view failed: {err}")))?
        .latest(&physical_key(child, b"generated-chain".to_vec())?)
        .map_err(|err| TestkitError::new(format!("chain latest failed: {err}")))?
        .ok_or_else(|| TestkitError::new("chain latest missed row"))?;
    if visible.source()
        != (BranchRowSource::Inherited {
            source_branch_id: parent,
            layer_index: 0,
        })
    {
        return Err(TestkitError::new("nearest inherited layer did not win"));
    }
    outcome.inherited_chained_ancestry += 1;
    Ok(())
}

fn check_invalid_inherited_layers(
    child: BranchId,
    outcome: &mut InheritanceOutcome,
) -> Result<(), TestkitError> {
    let source = branch_id(0xf1);
    let table = branch_owned_table(
        source,
        BranchLevel::ZERO,
        "generated-invalid-inherited-source",
        vec![storage_row_with(
            source,
            b"invalid-inherited".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"secret-payload".to_vec(),
        )?],
    )?;
    expect_invalid_inherited_layer(BranchInheritedLayer::new(
        InheritedLayerDescriptor::new(
            source,
            CommitVersion::new(1),
            InheritedLayerStatus::Active,
            2,
        ),
        vec![vec![table.clone()]],
    ))?;
    let mut child_state = BranchLocalState::empty(child);
    expect_invalid_inherited_layer(child_state.attach_inherited_layers(vec![
        branch_inherited_layer(
            child,
            CommitVersion::new(1),
            InheritedLayerStatus::Active,
            Vec::new(),
        )?,
    ]))?;
    outcome.invalid_inherited_layer_rejections += 2;
    Ok(())
}
