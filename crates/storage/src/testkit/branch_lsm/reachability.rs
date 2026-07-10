fn check_branch_reachability(script: &[u8]) -> Result<ReachabilityOutcome, TestkitError> {
    let mut outcome = ReachabilityOutcome::default();
    check_reachability_fact_model(script, &mut outcome)?;
    check_fork_reachability_registry_and_release(script, &mut outcome)?;
    check_materialization_reachability_release(script, &mut outcome)?;
    check_branch_clear_reachability_release(script, &mut outcome)?;
    Ok(outcome)
}

#[allow(clippy::similar_names, clippy::too_many_lines)]
fn check_reachability_fact_model(
    script: &[u8],
    outcome: &mut ReachabilityOutcome,
) -> Result<(), TestkitError> {
    let owner = branch_id(script_byte(script, 151));
    let source = branch_id(script_byte(script, 151).wrapping_add(1));
    let owned = BranchTableRef::owned(
        owner,
        BranchLevel::new(1),
        2,
        table_identity("generated-reach-owned")?,
    )
    .map_err(|err| TestkitError::new(format!("owned reachability ref failed: {err}")))?;
    let inherited = BranchTableRef::inherited(
        owner,
        source,
        CommitVersion::new(5),
        0,
        BranchLevel::ZERO,
        0,
        table_identity("generated-reach-inherited")?,
    )
    .map_err(|err| TestkitError::new(format!("inherited reachability ref failed: {err}")))?;
    let materializing = BranchTableRef::materializing_source(
        owner,
        source,
        CommitVersion::new(5),
        1,
        BranchLevel::ZERO,
        1,
        table_identity("generated-reach-materializing")?,
    )
    .map_err(|err| TestkitError::new(format!("materializing reachability ref failed: {err}")))?;
    let replacement = BranchTableRef::replacement(
        owner,
        source,
        CommitVersion::new(5),
        BranchLevel::ZERO,
        2,
        table_identity("generated-reach-replacement")?,
    )
    .map_err(|err| TestkitError::new(format!("replacement reachability ref failed: {err}")))?;
    let snapshot = BranchReachabilitySnapshot::new(
        owner,
        vec![
            replacement.clone(),
            materializing.clone(),
            inherited.clone(),
            owned.clone(),
        ],
    )
    .map_err(|err| TestkitError::new(format!("reachability snapshot failed: {err}")))?;
    if snapshot.facts().owned_table_count() != 2
        || snapshot.facts().inherited_table_count() != 2
        || snapshot.facts().reachable_table_count() != 4
        || snapshot.protected_table_count() != 4
    {
        return Err(TestkitError::new("reachability facts drifted"));
    }
    if snapshot
        .table_refs()
        .iter()
        .map(|table_ref| table_ref.table_identity().as_str())
        .collect::<Vec<_>>()
        != vec![
            "generated-reach-inherited",
            "generated-reach-materializing",
            "generated-reach-owned",
            "generated-reach-replacement",
        ]
    {
        return Err(TestkitError::new(
            "reachability snapshot order was nondeterministic",
        ));
    }
    outcome.reachability_snapshots += 1;
    outcome.reachability_owned_refs += 2;
    outcome.reachability_inherited_refs += 2;
    outcome.materializing_reachability_refs += 1;
    outcome.reachability_deterministic_orderings += 1;

    let aggregate = BranchReachabilityAggregate::from_snapshots(std::slice::from_ref(&snapshot))
        .map_err(|err| TestkitError::new(format!("reachability aggregate failed: {err}")))?;
    if aggregate.branch_count() != 1
        || aggregate.table_count() != 4
        || aggregate.reference_count_for(owned.table_identity()) != 1
    {
        return Err(TestkitError::new("single-branch aggregate facts drifted"));
    }
    if !aggregate
        .table_protections()
        .iter()
        .all(|protection| protection.reference_count() == 1 && protection.table_refs().len() == 1)
    {
        return Err(TestkitError::new("aggregate protection refs drifted"));
    }
    outcome.reachability_aggregate_rebuilds += 1;

    expect_invalid_reachability(BranchTableRef::inherited(
        owner,
        owner,
        CommitVersion::new(1),
        0,
        BranchLevel::ZERO,
        0,
        table_identity("generated-reach-invalid-same-branch")?,
    ))?;
    expect_invalid_reachability(BranchReachabilitySnapshot::new(
        owner,
        vec![inherited.clone(), inherited],
    ))?;
    expect_invalid_reachability(BranchReachabilityAggregate::from_snapshots(&[
        snapshot.clone(),
        snapshot,
    ]))?;
    outcome.invalid_reachability_rejections += 3;
    Ok(())
}

#[allow(clippy::similar_names, clippy::too_many_lines)]
fn check_fork_reachability_registry_and_release(
    script: &[u8],
    outcome: &mut ReachabilityOutcome,
) -> Result<(), TestkitError> {
    let parent = branch_id(script_byte(script, 152));
    let child_a = branch_id(script_byte(script, 152).wrapping_add(1));
    let child_b = branch_id(script_byte(script, 152).wrapping_add(2));
    let mut parent_state = BranchLocalState::empty(parent);
    parent_state
        .install_l0_table(branch_owned_table(
            parent,
            BranchLevel::ZERO,
            "generated-reach-shared-parent",
            vec![storage_row_with(
                parent,
                b"generated-reach-shared".to_vec(),
                3,
                30,
                Timestamp::EPOCH,
                vec![script_byte(script, 153)],
            )?],
        )?)
        .map_err(|err| TestkitError::new(format!("shared parent install failed: {err}")))?;
    let (child_a_state, _) = parent_state
        .fork_into_empty_child(child_a)
        .map_err(|err| TestkitError::new(format!("reachability fork a failed: {err}")))?;
    let (child_b_state, _) = parent_state
        .fork_into_empty_child(child_b)
        .map_err(|err| TestkitError::new(format!("reachability fork b failed: {err}")))?;
    outcome.fork_reachability_cases += 2;

    let parent_snapshot = parent_state
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("parent reachability failed: {err}")))?;
    let child_a_snapshot = child_a_state
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("child a reachability failed: {err}")))?;
    let child_b_snapshot = child_b_state
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("child b reachability failed: {err}")))?;
    let table_identity = parent_snapshot.table_refs()[0].table_identity().clone();
    outcome.reachability_snapshots += 3;
    outcome.reachability_owned_refs += parent_snapshot.facts().owned_table_count();
    outcome.reachability_inherited_refs += child_a_snapshot.facts().inherited_table_count()
        + child_b_snapshot.facts().inherited_table_count();

    let aggregate = BranchReachabilityAggregate::from_snapshots(&[
        parent_snapshot.clone(),
        child_a_snapshot.clone(),
        child_b_snapshot.clone(),
    ])
    .map_err(|err| TestkitError::new(format!("fork aggregate failed: {err}")))?;
    if aggregate.reference_count_for(&table_identity) != 3
        || !aggregate.is_reachable(&table_identity)
        || !aggregate.is_shared(&table_identity)
    {
        return Err(TestkitError::new("shared fork aggregate facts drifted"));
    }
    outcome.reachability_aggregate_rebuilds += 1;
    outcome.shared_table_detections += 1;

    let mut registry = SharedTableRegistry::rebuild_from_snapshots(&[
        parent_snapshot.clone(),
        child_a_snapshot.clone(),
        child_b_snapshot.clone(),
    ])
    .map_err(|err| TestkitError::new(format!("registry rebuild failed: {err}")))?;
    if registry.table_count() != 1
        || registry.reference_count(&table_identity) != 3
        || !registry.is_runtime_referenced(&table_identity)
    {
        return Err(TestkitError::new("registry rebuild facts drifted"));
    }
    outcome.registry_rebuilds += 1;

    let registry_before_failed_fork = registry.clone();
    if !matches!(
        parent_state.fork_into_empty_child(parent),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. })
    ) {
        return Err(TestkitError::new(
            "same-branch fork did not reject before reachability publication",
        ));
    }
    if registry != registry_before_failed_fork {
        return Err(TestkitError::new(
            "failed fork mutated reachability registry",
        ));
    }
    outcome.failed_fork_reachability_rollbacks += 1;

    registry
        .unregister_snapshot(&child_a_snapshot)
        .map_err(|err| TestkitError::new(format!("child a unregister failed: {err}")))?;
    if registry.reference_count(&table_identity) != 2 {
        return Err(TestkitError::new("registry unregister count drifted"));
    }
    outcome.registry_unregisters += 1;

    let aggregate_after_child_a = BranchReachabilityAggregate::from_snapshots(&[
        parent_snapshot.clone(),
        child_b_snapshot.clone(),
    ])
    .map_err(|err| TestkitError::new(format!("post-release aggregate failed: {err}")))?;
    outcome.reachability_aggregate_rebuilds += 1;
    let protected = BranchReleasePlan::from_removed_refs(
        child_a,
        child_a_snapshot.table_refs().to_vec(),
        &aggregate_after_child_a,
        Some(&registry),
    )
    .map_err(|err| TestkitError::new(format!("shared release plan failed: {err}")))?;
    if !protected.releasable_tables().is_empty()
        || protected.protected_tables().len() != 1
        || protected.protected_tables()[0].reason() != BranchProtectionReason::StillReachable
    {
        return Err(TestkitError::new("shared release protection drifted"));
    }
    outcome.protected_release_attempts += 1;

    let durable_only_protected = BranchReleasePlan::from_removed_refs(
        child_b,
        child_b_snapshot.table_refs().to_vec(),
        &aggregate_after_child_a,
        None,
    )
    .map_err(|err| TestkitError::new(format!("durable-only release plan failed: {err}")))?;
    if durable_only_protected.protected_tables()[0].reason()
        != BranchProtectionReason::StillReachable
    {
        return Err(TestkitError::new(
            "missing runtime registry was misclassified as disagreement",
        ));
    }
    outcome.protected_release_attempts += 1;

    let empty_registry = SharedTableRegistry::new();
    let releasable = BranchReleasePlan::from_removed_refs(
        child_b,
        child_b_snapshot.table_refs().to_vec(),
        &BranchReachabilityAggregate::empty(),
        Some(&empty_registry),
    )
    .map_err(|err| TestkitError::new(format!("final release plan failed: {err}")))?;
    if releasable.released_branch_id() != child_b
        || releasable.removed_refs().len() != 1
        || releasable.releasable_tables().len() != 1
        || !releasable.protected_tables().is_empty()
    {
        return Err(TestkitError::new("final release candidate drifted"));
    }
    outcome.reachability_release_candidates += 1;

    let runtime_protected = BranchReleasePlan::from_removed_refs(
        child_b,
        child_b_snapshot.table_refs().to_vec(),
        &BranchReachabilityAggregate::empty(),
        Some(&registry),
    )
    .map_err(|err| TestkitError::new(format!("runtime-protected plan failed: {err}")))?;
    if runtime_protected.protected_tables()[0].reason() != BranchProtectionReason::RuntimeReferenced
    {
        return Err(TestkitError::new(
            "runtime registry protection reason drifted",
        ));
    }
    outcome.protected_release_attempts += 1;

    let disagreement = BranchReleasePlan::from_removed_refs(
        child_b,
        child_b_snapshot.table_refs().to_vec(),
        &aggregate_after_child_a,
        Some(&empty_registry),
    )
    .map_err(|err| TestkitError::new(format!("disagreement plan failed: {err}")))?;
    if disagreement.protected_tables()[0].reason() != BranchProtectionReason::RegistryDisagreement {
        return Err(TestkitError::new("registry disagreement reason drifted"));
    }
    outcome.protected_release_attempts += 1;
    outcome.registry_disagreements += 1;

    let mut count_mismatch_registry = SharedTableRegistry::new();
    count_mismatch_registry
        .register_snapshot(&child_b_snapshot)
        .map_err(|err| TestkitError::new(format!("mismatch registry register failed: {err}")))?;
    let count_mismatch = BranchReleasePlan::from_removed_refs(
        child_a,
        child_a_snapshot.table_refs().to_vec(),
        &aggregate_after_child_a,
        Some(&count_mismatch_registry),
    )
    .map_err(|err| TestkitError::new(format!("count-mismatch release plan failed: {err}")))?;
    if count_mismatch.protected_tables()[0].reason() != BranchProtectionReason::RegistryDisagreement
    {
        return Err(TestkitError::new(
            "registry count mismatch was not reported as disagreement",
        ));
    }
    outcome.protected_release_attempts += 1;
    outcome.registry_disagreements += 1;

    let mut replacement_registry = SharedTableRegistry::rebuild_from_snapshots(&[
        parent_snapshot,
        child_b_snapshot.clone(),
    ])
    .map_err(|err| TestkitError::new(format!("replacement registry rebuild failed: {err}")))?;
    replacement_registry
        .replace_snapshot(&BranchReachabilitySnapshot::empty(child_b))
        .map_err(|err| TestkitError::new(format!("registry snapshot replacement failed: {err}")))?;
    if replacement_registry.reference_count(&table_identity) != 1 {
        return Err(TestkitError::new("registry replacement count drifted"));
    }
    expect_invalid_reachability(replacement_registry.replace_snapshot(&child_a_snapshot))?;

    expect_invalid_reachability(registry.unregister_snapshot(&child_a_snapshot))?;
    outcome.invalid_reachability_rejections += 2;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn check_materialization_reachability_release(
    script: &[u8],
    outcome: &mut ReachabilityOutcome,
) -> Result<(), TestkitError> {
    let source = branch_id(script_byte(script, 154));
    let child = branch_id(script_byte(script, 154).wrapping_add(1));
    let source_table = branch_owned_table(
        source,
        BranchLevel::ZERO,
        "generated-reach-materialize-source",
        vec![storage_row_with(
            source,
            b"generated-reach-materialize".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            vec![script_byte(script, 155)],
        )?],
    )?;
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![branch_inherited_layer(
            source,
            CommitVersion::new(4),
            InheritedLayerStatus::Active,
            vec![vec![source_table.clone()]],
        )?])
        .map_err(|err| {
            TestkitError::new(format!("reachability materialize attach failed: {err}"))
        })?;
    let before = child_state
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("pre-materialize reachability failed: {err}")))?;
    if before.facts().inherited_table_count() != 1 {
        return Err(TestkitError::new(
            "pre-materialization reachability missed source table",
        ));
    }
    outcome.reachability_snapshots += 1;
    outcome.reachability_inherited_refs += 1;

    let mut in_flight = BranchLocalState::empty(child);
    in_flight
        .attach_inherited_layers(vec![branch_inherited_layer(
            source,
            CommitVersion::new(4),
            InheritedLayerStatus::Materializing,
            vec![vec![source_table]],
        )?])
        .map_err(|err| TestkitError::new(format!("materializing attach failed: {err}")))?;
    let in_flight_snapshot = in_flight
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("materializing snapshot failed: {err}")))?;
    if !matches!(
        in_flight_snapshot.table_refs()[0].reference_kind(),
        BranchTableReferenceKind::MaterializingSource { .. }
    ) {
        return Err(TestkitError::new(
            "materializing layer did not retain source reachability",
        ));
    }
    outcome.reachability_snapshots += 1;
    outcome.reachability_inherited_refs += 1;
    outcome.materializing_reachability_refs += 1;

    child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "generated-reach-materialized").map_err(
                |err| TestkitError::new(format!("materialization request failed: {err}")),
            )?,
        )
        .map_err(|err| TestkitError::new(format!("reachability materialization failed: {err}")))?;
    let after = child_state
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("post-materialize reachability failed: {err}")))?;
    if after.facts().owned_table_count() != 1 || after.facts().inherited_table_count() != 0 {
        return Err(TestkitError::new(
            "post-materialization replacement reachability drifted",
        ));
    }
    if !matches!(
        after.table_refs()[0].reference_kind(),
        BranchTableReferenceKind::Replacement {
            source_branch_id,
            fork_version,
        } if source_branch_id == source && fork_version == CommitVersion::new(4)
    ) {
        return Err(TestkitError::new(
            "materialized table reachability did not preserve replacement provenance",
        ));
    }
    outcome.reachability_snapshots += 1;
    outcome.reachability_owned_refs += 1;

    let aggregate_after = BranchReachabilityAggregate::from_snapshots(&[after])
        .map_err(|err| TestkitError::new(format!("materialized aggregate failed: {err}")))?;
    let release = BranchReleasePlan::from_removed_refs(
        child,
        before.table_refs().to_vec(),
        &aggregate_after,
        Some(&SharedTableRegistry::new()),
    )
    .map_err(|err| TestkitError::new(format!("materialized release failed: {err}")))?;
    if release.releasable_tables().len() != 1 || !release.protected_tables().is_empty() {
        return Err(TestkitError::new(
            "materialization removed-source release facts drifted",
        ));
    }
    outcome.reachability_aggregate_rebuilds += 1;
    outcome.materialization_release_cases += 1;
    outcome.reachability_release_candidates += 1;
    Ok(())
}

fn check_branch_clear_reachability_release(
    script: &[u8],
    outcome: &mut ReachabilityOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 156));
    let mut state = BranchLocalState::empty(branch);
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "generated-reach-clear-a",
            vec![storage_row_with(
                branch,
                b"generated-reach-clear-a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                vec![script_byte(script, 157)],
            )?],
        )?)
        .map_err(|err| TestkitError::new(format!("clear table a failed: {err}")))?;
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "generated-reach-clear-b",
            vec![storage_row_with(
                branch,
                b"generated-reach-clear-b".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                vec![script_byte(script, 158)],
            )?],
        )?)
        .map_err(|err| TestkitError::new(format!("clear table b failed: {err}")))?;
    let snapshot = state
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("clear reachability failed: {err}")))?;
    if snapshot.facts().owned_table_count() != 2 || snapshot.protected_table_count() != 2 {
        return Err(TestkitError::new("clear snapshot reachability drifted"));
    }
    outcome.reachability_snapshots += 1;
    outcome.reachability_owned_refs += 2;

    let release = BranchReleasePlan::from_removed_refs(
        branch,
        snapshot.table_refs().to_vec(),
        &BranchReachabilityAggregate::empty(),
        Some(&SharedTableRegistry::new()),
    )
    .map_err(|err| TestkitError::new(format!("clear release failed: {err}")))?;
    if release.releasable_tables().len() != 2 || !release.protected_tables().is_empty() {
        return Err(TestkitError::new("clear release facts drifted"));
    }
    outcome.branch_clear_release_cases += 1;
    outcome.reachability_release_candidates += 2;
    Ok(())
}
