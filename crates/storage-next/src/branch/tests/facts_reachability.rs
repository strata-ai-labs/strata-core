use super::*;

#[test]
fn branch_runtime_config_rejects_unusable_zero_limits() {
    let default_config = BranchRuntimeConfig::default();
    assert_eq!(default_config.max_level_count(), 8);
    assert_eq!(default_config.max_inherited_layers(), 64);
    assert_eq!(default_config.max_frozen_tables(), 32);

    let explicit = BranchRuntimeConfig::new(3, 2, 8).expect("valid branch config");
    assert_eq!(explicit.max_level_count(), 3);
    assert_eq!(explicit.max_inherited_layers(), 2);
    assert_eq!(explicit.max_frozen_tables(), 8);

    assert_invalid_config_field(BranchRuntimeConfig::new(0, 2, 8), "max_level_count");
    assert_invalid_config_field(
        BranchRuntimeConfig::new(usize::from(u8::MAX) + 2, 2, 8),
        "max_level_count",
    );
    assert_invalid_config_field(BranchRuntimeConfig::new(3, 0, 8), "max_inherited_layers");
    assert_invalid_config_field(BranchRuntimeConfig::new(3, 2, 0), "max_frozen_tables");
}

#[test]
fn branch_runtime_config_default_allocates_eight_lsm_levels() {
    let branch = branch_id(2);
    let state = BranchLocalState::empty(branch);
    let view = state.capture_read_view().expect("read view");

    assert_eq!(view.owned_levels().len(), 8);
}

#[test]
fn branch_runtime_config_default_accepts_terminal_lsm_level_tables() {
    let branch = branch_id(3);
    let mut state = BranchLocalState::empty(branch);
    let terminal_level = BranchLevel::new(7);
    let row = storage_row_with(
        branch,
        b"terminal-level".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"value".to_vec(),
    );

    let outcome = state
        .install_owned_table_at_level(
            terminal_level,
            branch_owned_table(
                branch,
                terminal_level,
                "terminal-level-table",
                vec![row.clone()],
            ),
        )
        .expect("install terminal-level table");

    assert_eq!(outcome.level(), terminal_level);
    assert_eq!(outcome.table_index(), 0);
    let view = state.capture_read_view().expect("read view");
    assert_eq!(view.owned_levels().len(), 8);
    assert_eq!(
        view.owned_levels()[usize::from(terminal_level.raw())].len(),
        1
    );
    assert_eq!(
        view.source_layout().owned_nonzero_level_table_counts(),
        &[BranchLevelTableCount::new(terminal_level, 1)]
    );
    assert_eq!(
        view.latest(&physical_key(branch, b"terminal-level".to_vec()))
            .expect("latest")
            .expect("visible")
            .row(),
        &row
    );
}

#[test]
fn branch_runtime_config_default_recovers_terminal_lsm_level_tables() {
    let branch = branch_id(4);
    let terminal_level = BranchLevel::new(7);
    let row = storage_row_with(
        branch,
        b"recovered-terminal-level".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"value".to_vec(),
    );
    let mut owned_levels = vec![Vec::new(); 8];
    owned_levels[usize::from(terminal_level.raw())].push(branch_owned_table(
        branch,
        terminal_level,
        "recovered-terminal-level-table",
        vec![row.clone()],
    ));
    let request = BranchTableManifestRecoveryRequest::new(branch, owned_levels, Vec::new())
        .expect("recovery request");
    let mut state = BranchLocalState::empty(branch);

    let outcome = state
        .install_table_manifest_recovery(request)
        .expect("manifest recovery install");

    assert_eq!(outcome.total_table_count(), 1);
    let view = state.capture_read_view().expect("read view");
    assert_eq!(view.owned_levels().len(), 8);
    assert_eq!(
        view.owned_levels()[usize::from(terminal_level.raw())].len(),
        1
    );
    assert_eq!(
        view.source_layout().owned_nonzero_level_table_counts(),
        &[BranchLevelTableCount::new(terminal_level, 1)]
    );
    assert_eq!(
        view.latest(&physical_key(branch, b"recovered-terminal-level".to_vec()))
            .expect("latest")
            .expect("visible")
            .row(),
        &row
    );
}

#[test]
fn branch_runtime_config_preserves_custom_compaction_bounds() {
    let branch = branch_id(5);
    let two_level_config = BranchRuntimeConfig::new(2, 64, 32).expect("two-level config");
    let two_level_state = BranchLocalState::new(branch, two_level_config).expect("state");
    let terminal_plan = two_level_state
        .plan_branch_compaction(
            &BranchCompactionRequest::new(
                branch,
                BranchCompactionKind::CompactLevel {
                    level: BranchLevel::new(1),
                    table_index: 0,
                },
                "two-level-terminal",
            )
            .expect("request"),
        )
        .expect("terminal plan");
    assert_eq!(
        terminal_plan.noop_reason(),
        Some(BranchCompactionNoopReason::LastLevel)
    );
    assert!(matches!(
        two_level_state.plan_branch_compaction(
            &BranchCompactionRequest::new(
                branch,
                BranchCompactionKind::CompactLevel {
                    level: BranchLevel::new(2),
                    table_index: 0,
                },
                "two-level-outside",
            )
            .expect("request"),
        ),
        Err(BranchRuntimeError::InvalidCompaction { .. })
    ));

    let three_level_branch = branch_id(6);
    let three_level_config = BranchRuntimeConfig::new(3, 64, 32).expect("three-level config");
    let three_level_state =
        BranchLocalState::new(three_level_branch, three_level_config).expect("state");
    let compactable_empty_plan = three_level_state
        .plan_branch_compaction(
            &BranchCompactionRequest::new(
                three_level_branch,
                BranchCompactionKind::CompactLevel {
                    level: BranchLevel::new(1),
                    table_index: 0,
                },
                "three-level-empty",
            )
            .expect("request"),
        )
        .expect("compactable empty plan");
    assert_eq!(
        compactable_empty_plan.noop_reason(),
        Some(BranchCompactionNoopReason::EmptyInputLevel)
    );
    let terminal_plan = three_level_state
        .plan_branch_compaction(
            &BranchCompactionRequest::new(
                three_level_branch,
                BranchCompactionKind::CompactLevel {
                    level: BranchLevel::new(2),
                    table_index: 0,
                },
                "three-level-terminal",
            )
            .expect("request"),
        )
        .expect("terminal plan");
    assert_eq!(
        terminal_plan.noop_reason(),
        Some(BranchCompactionNoopReason::LastLevel)
    );
}

#[test]
fn branch_read_bound_preserves_requested_bound() {
    let version = CommitVersion::new(42);
    let timestamp = Timestamp::from_micros(1_000);

    assert_eq!(BranchReadBound::latest(), BranchReadBound::Latest);
    assert_eq!(
        BranchReadBound::at_version(version),
        BranchReadBound::AtVersion(version)
    );
    assert_eq!(
        BranchReadBound::at_timestamp(timestamp),
        BranchReadBound::AtTimestamp(timestamp)
    );
}

#[test]
fn branch_state_facts_accept_empty_shape_and_reject_impossible_shapes() {
    let test_branch_id = branch_id(1);
    let empty = BranchStateFacts::empty(test_branch_id);

    assert_eq!(empty.branch_id(), test_branch_id);
    assert_eq!(empty.active_rows(), 0);
    assert_eq!(empty.frozen_table_count(), 0);
    assert_eq!(empty.owned_table_count(), 0);
    assert_eq!(empty.inherited_layer_count(), 0);
    assert_eq!(empty.max_commit_version(), None);
    assert_eq!(empty.timestamp_min(), None);
    assert_eq!(empty.timestamp_max(), None);

    let populated = BranchStateFacts::new(
        test_branch_id,
        1,
        2,
        3,
        4,
        Some(CommitVersion::new(9)),
        Some(Timestamp::from_micros(10)),
        Some(Timestamp::from_micros(11)),
    )
    .expect("populated branch facts");
    assert_eq!(populated.active_rows(), 1);
    assert_eq!(populated.max_commit_version(), Some(CommitVersion::new(9)));

    assert!(matches!(
        BranchStateFacts::new(
            test_branch_id,
            0,
            0,
            0,
            0,
            Some(CommitVersion::new(1)),
            None,
            None,
        ),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
    assert!(matches!(
        BranchStateFacts::new(
            test_branch_id,
            0,
            0,
            0,
            0,
            None,
            Some(Timestamp::from_micros(5)),
            Some(Timestamp::from_micros(5)),
        ),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
    assert!(matches!(
        BranchStateFacts::new(
            test_branch_id,
            1,
            0,
            0,
            0,
            Some(CommitVersion::new(1)),
            Some(Timestamp::from_micros(5)),
            Some(Timestamp::from_micros(4)),
        ),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
    assert!(matches!(
        BranchStateFacts::new(
            test_branch_id,
            1,
            0,
            0,
            0,
            Some(CommitVersion::new(1)),
            Some(Timestamp::from_micros(5)),
            None,
        ),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
}

#[test]
fn branch_descriptors_preserve_storage_owned_facts() {
    let test_branch_id = branch_id(2);
    let state_facts = BranchStateFacts::empty(test_branch_id);
    let state = BranchStateDescriptor::new(test_branch_id, state_facts).expect("state descriptor");
    let view = BranchViewDescriptor::new(test_branch_id, state_facts).expect("view descriptor");

    assert_eq!(state.branch_id(), test_branch_id);
    assert_eq!(state.facts(), state_facts);
    assert_eq!(view.branch_id(), test_branch_id);
    assert_eq!(view.facts(), state_facts);

    let table_facts = table_facts("branch-table");
    let table = BranchTableDescriptor::new(
        TableIdentity::new("branch-table").expect("identity"),
        table_facts.clone(),
        BranchLevel::new(2),
    )
    .expect("table descriptor");
    assert_eq!(table.identity().as_str(), "branch-table");
    assert_eq!(table.facts(), &table_facts);
    assert_eq!(table.level().raw(), 2);
    assert_eq!(table, table.clone());

    let inherited = InheritedLayerDescriptor::new(
        branch_id(3),
        CommitVersion::new(11),
        InheritedLayerStatus::Active,
        4,
    );
    assert_eq!(inherited.source_branch_id(), branch_id(3));
    assert_eq!(inherited.fork_version(), CommitVersion::new(11));
    assert_eq!(inherited.status(), InheritedLayerStatus::Active);
    assert_eq!(inherited.table_count(), 4);
    let statuses = [
        InheritedLayerStatus::Active,
        InheritedLayerStatus::Materializing,
        InheritedLayerStatus::Materialized,
        InheritedLayerStatus::Unavailable,
    ];
    assert!(statuses.contains(&InheritedLayerStatus::Materializing));
    assert!(statuses.contains(&InheritedLayerStatus::Materialized));
    assert!(statuses.contains(&InheritedLayerStatus::Unavailable));

    let reachability = BranchReachabilityFacts::new(test_branch_id, 3, 4);
    assert_eq!(reachability.branch_id(), test_branch_id);
    assert_eq!(reachability.owned_table_count(), 3);
    assert_eq!(reachability.inherited_table_count(), 4);
    assert_eq!(reachability.reachable_table_count(), 7);
    assert_eq!(
        reachability,
        BranchReachabilityFacts::new(test_branch_id, 3, 4)
    );
    assert_ne!(
        reachability,
        BranchReachabilityFacts::new(test_branch_id, 4, 3)
    );

    for debug_text in [
        format!("{state:?}"),
        format!("{view:?}"),
        format!("{table:?}"),
        format!("{inherited:?}"),
        format!("{reachability:?}"),
    ] {
        assert!(!debug_text.contains("secret-payload"));
    }

    assert!(matches!(
        BranchStateDescriptor::new(branch_id(42), state_facts),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
    assert!(matches!(
        BranchViewDescriptor::new(branch_id(42), state_facts),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
    assert!(matches!(
        BranchTableDescriptor::new(
            TableIdentity::new("wrong-table").expect("identity"),
            table_facts,
            BranchLevel::new(2),
        ),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
}

#[test]
#[allow(clippy::similar_names, clippy::too_many_lines)]
fn branch_reachability_fact_types_are_deterministic_and_validated() {
    let owner = branch_id(111);
    let source = branch_id(112);
    let owned = BranchTableRef::owned(
        owner,
        BranchLevel::new(1),
        2,
        TableIdentity::new("reach-owned").expect("identity"),
    )
    .expect("owned ref");
    let inherited = BranchTableRef::inherited(
        owner,
        source,
        CommitVersion::new(9),
        3,
        BranchLevel::ZERO,
        0,
        TableIdentity::new("reach-inherited").expect("identity"),
    )
    .expect("inherited ref");
    let materializing = BranchTableRef::materializing_source(
        owner,
        source,
        CommitVersion::new(9),
        4,
        BranchLevel::ZERO,
        1,
        TableIdentity::new("reach-materializing").expect("identity"),
    )
    .expect("materializing ref");
    let replacement = BranchTableRef::replacement(
        owner,
        source,
        CommitVersion::new(9),
        BranchLevel::ZERO,
        2,
        TableIdentity::new("reach-replacement").expect("identity"),
    )
    .expect("replacement ref");

    assert_eq!(owned.owner_branch_id(), owner);
    assert_eq!(owned.table_branch_id(), owner);
    assert_eq!(owned.level(), BranchLevel::new(1));
    assert_eq!(owned.table_index(), 2);
    assert_eq!(owned.reference_kind(), BranchTableReferenceKind::Owned);
    assert!(inherited.reference_kind().is_inherited_like());
    assert_eq!(
        materializing.reference_kind().source_branch_id(),
        Some(source)
    );
    assert!(replacement.reference_kind().is_owned_like());
    assert!(matches!(
        replacement.reference_kind(),
        BranchTableReferenceKind::Replacement {
            source_branch_id,
            fork_version,
        } if source_branch_id == source && fork_version == CommitVersion::new(9)
    ));

    let snapshot = BranchReachabilitySnapshot::new(
        owner,
        vec![
            materializing.clone(),
            replacement.clone(),
            inherited.clone(),
            owned.clone(),
        ],
    )
    .expect("snapshot");
    assert_eq!(snapshot.branch_id(), owner);
    assert_eq!(snapshot.facts().owned_table_count(), 2);
    assert_eq!(snapshot.facts().inherited_table_count(), 2);
    assert_eq!(snapshot.protected_table_count(), 4);
    assert_eq!(
        snapshot
            .table_refs()
            .iter()
            .map(|table_ref| table_ref.table_identity().as_str())
            .collect::<Vec<_>>(),
        vec![
            "reach-inherited",
            "reach-materializing",
            "reach-owned",
            "reach-replacement",
        ],
        "snapshot sorting is stable and identity-first",
    );
    assert_eq!(BranchReachabilitySnapshot::empty(owner).table_refs(), &[]);

    assert!(BranchTableRef::owned(
        owner,
        BranchLevel::ZERO,
        0,
        TableIdentity::new("valid").expect("identity"),
    )
    .is_ok());
    assert!(matches!(
        BranchTableRef::inherited(
            owner,
            owner,
            CommitVersion::new(1),
            0,
            BranchLevel::ZERO,
            0,
            TableIdentity::new("bad-same-branch").expect("identity"),
        ),
        Err(BranchRuntimeError::InvalidReachability { .. })
    ));
    let wrong_owner_error = BranchReachabilitySnapshot::new(branch_id(113), vec![owned.clone()])
        .expect_err("wrong owner rejected");
    assert!(matches!(
        wrong_owner_error,
        BranchRuntimeError::InvalidReachability { .. }
    ));
    assert!(wrong_owner_error
        .to_string()
        .contains("owner branch must match snapshot branch"));
    let wrong_owner_duplicate_error =
        BranchReachabilitySnapshot::new(branch_id(113), vec![owned.clone(), owned])
            .expect_err("wrong owner duplicate rejected");
    assert!(wrong_owner_duplicate_error
        .to_string()
        .contains("owner branch must match snapshot branch"));
    assert!(matches!(
        BranchReachabilitySnapshot::new(owner, vec![inherited.clone(), inherited]),
        Err(BranchRuntimeError::InvalidReachability { .. })
    ));
}

#[test]
#[allow(clippy::too_many_lines)]
fn branch_reachability_snapshot_tracks_owned_and_inherited_tables_only() {
    let source = branch_id(114);
    let child = branch_id(115);
    let mut source_state = BranchLocalState::empty(source);
    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "reach-source-owned",
            vec![storage_row_with(
                source,
                b"source-reach".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"source".to_vec(),
            )],
        ))
        .expect("install source owned table");
    let (mut child_state, _) = source_state
        .fork_into_empty_child(child)
        .expect("fork reachability child");
    child_state
        .append_committed_row(storage_row_with(
            child,
            b"active-not-durable".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"active".to_vec(),
        ))
        .expect("append active row");
    assert!(matches!(
        child_state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    child_state
        .install_l0_table(branch_owned_table(
            child,
            BranchLevel::ZERO,
            "reach-child-owned",
            vec![storage_row_with(
                child,
                b"child-owned".to_vec(),
                3,
                30,
                Timestamp::EPOCH,
                b"child".to_vec(),
            )],
        ))
        .expect("install child owned table");

    let snapshot = child_state
        .reachability_snapshot()
        .expect("child reachability snapshot");
    assert_eq!(snapshot.facts().owned_table_count(), 1);
    assert_eq!(snapshot.facts().inherited_table_count(), 1);
    assert_eq!(snapshot.protected_table_count(), 2);
    assert_eq!(
        snapshot
            .table_refs()
            .iter()
            .map(|table_ref| {
                (
                    table_ref.table_identity().as_str(),
                    table_ref.reference_kind(),
                    table_ref.owner_branch_id(),
                    table_ref.table_branch_id(),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            (
                "reach-child-owned",
                BranchTableReferenceKind::Owned,
                child,
                child,
            ),
            (
                "reach-source-owned",
                BranchTableReferenceKind::Inherited {
                    source_branch_id: source,
                    fork_version: CommitVersion::new(1),
                    layer_index: 0,
                },
                child,
                source,
            ),
        ],
    );
    assert!(snapshot
        .table_refs()
        .iter()
        .all(|table_ref| !table_ref.table_identity().as_str().contains("active")));

    let materializing = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Materializing,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "reach-materializing-source",
            vec![storage_row_with(
                source,
                b"materializing".to_vec(),
                5,
                50,
                Timestamp::EPOCH,
                b"materializing".to_vec(),
            )],
        )]],
    );
    let mut materializing_child = BranchLocalState::empty(child);
    materializing_child
        .attach_inherited_layers(vec![materializing])
        .expect("attach materializing layer");
    let materializing_snapshot = materializing_child
        .reachability_snapshot()
        .expect("materializing snapshot");
    assert!(matches!(
        materializing_snapshot.table_refs()[0].reference_kind(),
        BranchTableReferenceKind::MaterializingSource { .. }
    ));
}

#[test]
#[allow(clippy::similar_names, clippy::too_many_lines)]
fn branch_reachability_aggregate_registry_and_release_plans_are_safe() {
    let parent = branch_id(116);
    let child_a = branch_id(117);
    let child_b = branch_id(118);
    let mut parent_state = BranchLocalState::empty(parent);
    parent_state
        .install_l0_table(branch_owned_table(
            parent,
            BranchLevel::ZERO,
            "reach-shared-parent",
            vec![storage_row_with(
                parent,
                b"shared".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"parent".to_vec(),
            )],
        ))
        .expect("install shared parent table");
    let (child_a_state, _) = parent_state
        .fork_into_empty_child(child_a)
        .expect("fork child a");
    let (child_b_state, _) = parent_state
        .fork_into_empty_child(child_b)
        .expect("fork child b");
    let parent_snapshot = parent_state
        .reachability_snapshot()
        .expect("parent snapshot");
    let child_a_snapshot = child_a_state
        .reachability_snapshot()
        .expect("child a snapshot");
    let child_b_snapshot = child_b_state
        .reachability_snapshot()
        .expect("child b snapshot");
    let table_identity = parent_snapshot.table_refs()[0].table_identity().clone();
    let aggregate = BranchReachabilityAggregate::from_snapshots(&[
        parent_snapshot.clone(),
        child_a_snapshot.clone(),
        child_b_snapshot.clone(),
    ])
    .expect("aggregate");
    assert_eq!(aggregate.branch_count(), 3);
    assert_eq!(aggregate.table_count(), 1);
    assert_eq!(aggregate.reference_count(), 3);
    assert_eq!(aggregate.reference_count_for(&table_identity), 3);
    assert!(aggregate.is_reachable(&table_identity));
    assert!(aggregate.is_shared(&table_identity));
    assert_eq!(aggregate.table_protections()[0].reference_count(), 3);

    let mut registry = SharedTableRegistry::rebuild_from_snapshots(&[
        parent_snapshot.clone(),
        child_a_snapshot.clone(),
        child_b_snapshot.clone(),
    ])
    .expect("registry rebuild");
    assert_eq!(registry.table_count(), 1);
    assert_eq!(registry.reference_count(&table_identity), 3);
    registry
        .unregister_snapshot(&child_a_snapshot)
        .expect("unregister child a");
    assert_eq!(registry.reference_count(&table_identity), 2);

    let aggregate_after_child_a = BranchReachabilityAggregate::from_snapshots(&[
        parent_snapshot.clone(),
        child_b_snapshot.clone(),
    ])
    .expect("aggregate after child a");
    let child_a_release = BranchReleasePlan::from_removed_refs(
        child_a,
        child_a_snapshot.table_refs().to_vec(),
        &aggregate_after_child_a,
        Some(&registry),
    )
    .expect("child a release plan");
    assert_eq!(child_a_release.released_branch_id(), child_a);
    assert_eq!(child_a_release.removed_refs().len(), 1);
    assert!(child_a_release.releasable_tables().is_empty());
    assert_eq!(
        child_a_release.protected_tables()[0].reason(),
        BranchProtectionReason::StillReachable
    );
    let durable_only_protected = BranchReleasePlan::from_removed_refs(
        child_b,
        child_b_snapshot.table_refs().to_vec(),
        &aggregate_after_child_a,
        None,
    )
    .expect("durable-only release plan");
    assert_eq!(
        durable_only_protected.protected_tables()[0].reason(),
        BranchProtectionReason::StillReachable
    );

    let empty_aggregate = BranchReachabilityAggregate::empty();
    let empty_registry = SharedTableRegistry::new();
    let final_release = BranchReleasePlan::from_removed_refs(
        child_b,
        child_b_snapshot.table_refs().to_vec(),
        &empty_aggregate,
        Some(&empty_registry),
    )
    .expect("final release plan");
    assert_eq!(
        final_release
            .releasable_tables()
            .iter()
            .map(TableIdentity::as_str)
            .collect::<Vec<_>>(),
        vec!["reach-shared-parent"],
    );
    assert!(final_release.protected_tables().is_empty());

    let runtime_protected = BranchReleasePlan::from_removed_refs(
        child_b,
        child_b_snapshot.table_refs().to_vec(),
        &empty_aggregate,
        Some(&registry),
    )
    .expect("runtime protected release plan");
    assert_eq!(
        runtime_protected.protected_tables()[0].reason(),
        BranchProtectionReason::RuntimeReferenced
    );

    let registry_disagreement = BranchReleasePlan::from_removed_refs(
        child_b,
        child_b_snapshot.table_refs().to_vec(),
        &aggregate_after_child_a,
        Some(&empty_registry),
    )
    .expect("registry disagreement release plan");
    assert_eq!(
        registry_disagreement.protected_tables()[0].reason(),
        BranchProtectionReason::RegistryDisagreement
    );
    let mut count_mismatch_registry = SharedTableRegistry::new();
    count_mismatch_registry
        .register_snapshot(&child_b_snapshot)
        .expect("register only child b");
    let count_mismatch = BranchReleasePlan::from_removed_refs(
        child_a,
        child_a_snapshot.table_refs().to_vec(),
        &aggregate_after_child_a,
        Some(&count_mismatch_registry),
    )
    .expect("count mismatch release plan");
    assert_eq!(
        count_mismatch.protected_tables()[0].reason(),
        BranchProtectionReason::RegistryDisagreement
    );

    let mut replacement_registry = SharedTableRegistry::rebuild_from_snapshots(&[
        parent_snapshot.clone(),
        child_b_snapshot.clone(),
    ])
    .expect("replacement registry rebuild");
    let empty_child_b = BranchReachabilitySnapshot::empty(child_b);
    replacement_registry
        .replace_snapshot(&empty_child_b)
        .expect("replace child b snapshot");
    assert_eq!(replacement_registry.reference_count(&table_identity), 1);
    assert!(matches!(
        replacement_registry.replace_snapshot(&child_a_snapshot),
        Err(BranchRuntimeError::InvalidReachability { .. })
    ));

    assert!(matches!(
        registry.unregister_snapshot(&child_a_snapshot),
        Err(BranchRuntimeError::InvalidReachability { .. })
    ));
    registry.clear();
    assert_eq!(registry.reference_count(&table_identity), 0);
}

#[test]
fn branch_reachability_registry_snapshot_replacement_updates_counts_atomically() {
    let branch = branch_id(119);
    let old_identity = TableIdentity::new("reach-registry-old").expect("old identity");
    let new_identity = TableIdentity::new("reach-registry-new").expect("new identity");
    let old_snapshot = BranchReachabilitySnapshot::new(
        branch,
        vec![
            BranchTableRef::owned(branch, BranchLevel::ZERO, 0, old_identity.clone())
                .expect("old ref"),
        ],
    )
    .expect("old snapshot");
    let new_snapshot = BranchReachabilitySnapshot::new(
        branch,
        vec![
            BranchTableRef::owned(branch, BranchLevel::ZERO, 0, new_identity.clone())
                .expect("new ref"),
        ],
    )
    .expect("new snapshot");
    let mut registry = SharedTableRegistry::new();

    registry
        .register_snapshot(&old_snapshot)
        .expect("register old snapshot");
    assert_eq!(registry.reference_count(&old_identity), 1);
    assert!(matches!(
        registry.register_snapshot(&old_snapshot),
        Err(BranchRuntimeError::InvalidReachability { .. })
    ));
    assert_eq!(registry.reference_count(&old_identity), 1);

    registry
        .replace_snapshot(&new_snapshot)
        .expect("replace old snapshot with new snapshot");
    assert_eq!(registry.reference_count(&old_identity), 0);
    assert_eq!(registry.reference_count(&new_identity), 1);
    assert_eq!(registry.table_count(), 1);

    assert!(matches!(
        registry.unregister_snapshot(&old_snapshot),
        Err(BranchRuntimeError::InvalidReachability { .. })
    ));
    assert_eq!(registry.reference_count(&old_identity), 0);
    assert_eq!(registry.reference_count(&new_identity), 1);

    registry
        .unregister_snapshot(&new_snapshot)
        .expect("unregister new snapshot");
    assert_eq!(registry.reference_count(&new_identity), 0);
    assert_eq!(registry.table_count(), 0);
    assert!(matches!(
        registry.replace_snapshot(&new_snapshot),
        Err(BranchRuntimeError::InvalidReachability { .. })
    ));
}

#[test]
#[allow(clippy::too_many_lines)]
fn branch_reachability_release_plans_cover_empty_clear_and_inherited_refs() {
    let empty_branch = branch_id(120);
    let empty_snapshot = BranchReachabilitySnapshot::empty(empty_branch);
    let empty_release = BranchReleasePlan::from_removed_refs(
        empty_branch,
        empty_snapshot.table_refs().to_vec(),
        &BranchReachabilityAggregate::empty(),
        Some(&SharedTableRegistry::new()),
    )
    .expect("empty branch release");
    assert!(empty_release.removed_refs().is_empty());
    assert!(empty_release.releasable_tables().is_empty());
    assert!(empty_release.protected_tables().is_empty());

    let parent = branch_id(121);
    let child = branch_id(122);
    let mut parent_state = BranchLocalState::empty(parent);
    parent_state
        .install_l0_table(branch_owned_table(
            parent,
            BranchLevel::ZERO,
            "reach-clear-parent-owned",
            vec![storage_row_with(
                parent,
                b"clear-shared".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"parent".to_vec(),
            )],
        ))
        .expect("install parent table");
    let (child_state, _) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    let parent_snapshot = parent_state
        .reachability_snapshot()
        .expect("parent snapshot");
    let child_snapshot = child_state.reachability_snapshot().expect("child snapshot");
    let aggregate_child_only =
        BranchReachabilityAggregate::from_snapshots(std::slice::from_ref(&child_snapshot))
            .expect("child aggregate");
    let parent_clear = BranchReleasePlan::from_removed_refs(
        parent,
        parent_snapshot.table_refs().to_vec(),
        &aggregate_child_only,
        None,
    )
    .expect("parent clear release");
    assert!(parent_clear.releasable_tables().is_empty());
    assert_eq!(
        parent_clear.protected_tables()[0].reason(),
        BranchProtectionReason::StillReachable
    );

    let child_clear = BranchReleasePlan::from_removed_refs(
        child,
        child_snapshot.table_refs().to_vec(),
        &BranchReachabilityAggregate::empty(),
        Some(&SharedTableRegistry::new()),
    )
    .expect("child clear release");
    assert_eq!(
        child_clear
            .releasable_tables()
            .iter()
            .map(TableIdentity::as_str)
            .collect::<Vec<_>>(),
        vec!["reach-clear-parent-owned"],
    );
    assert!(child_clear.protected_tables().is_empty());
    assert!(matches!(
        child_clear.removed_refs()[0].reference_kind(),
        BranchTableReferenceKind::Inherited { .. }
    ));

    let mutable_branch = branch_id(123);
    let mut mutable_state = BranchLocalState::empty(mutable_branch);
    mutable_state
        .append_committed_row(storage_row_with(
            mutable_branch,
            b"active-only".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"secret-active-payload".to_vec(),
        ))
        .expect("append active row");
    assert!(matches!(
        mutable_state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    let mutable_snapshot = mutable_state
        .reachability_snapshot()
        .expect("mutable-only reachability snapshot");
    assert!(mutable_snapshot.table_refs().is_empty());
    assert!(!format!("{mutable_snapshot:?}").contains("secret-active-payload"));
}

#[test]
#[allow(clippy::too_many_lines)]
fn branch_reachability_materialization_release_is_limited_to_removed_layer() {
    let near_source = branch_id(124);
    let deep_source = branch_id(125);
    let child = branch_id(126);
    let near_identity = "reach-materialize-near-source";
    let deep_identity = "reach-materialize-deep-source";
    let near_layer = branch_inherited_layer(
        near_source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            near_source,
            BranchLevel::ZERO,
            near_identity,
            vec![storage_row_with(
                near_source,
                b"near-materialize".to_vec(),
                4,
                40,
                Timestamp::EPOCH,
                b"near".to_vec(),
            )],
        )]],
    );
    let deep_layer = branch_inherited_layer(
        deep_source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            deep_source,
            BranchLevel::ZERO,
            deep_identity,
            vec![storage_row_with(
                deep_source,
                b"deep-materialize".to_vec(),
                5,
                50,
                Timestamp::EPOCH,
                b"deep".to_vec(),
            )],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![near_layer, deep_layer])
        .expect("attach inherited layers");
    let before = child_state
        .reachability_snapshot()
        .expect("pre-materialization snapshot");
    assert_eq!(before.facts().owned_table_count(), 0);
    assert_eq!(before.facts().inherited_table_count(), 2);
    let removed_deep_refs = before
        .table_refs()
        .iter()
        .filter(|table_ref| {
            matches!(
                table_ref.reference_kind(),
                BranchTableReferenceKind::Inherited {
                    source_branch_id,
                    layer_index: 1,
                    ..
                } if source_branch_id == deep_source
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(removed_deep_refs.len(), 1);

    child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 1, "reach-materialized-deep")
                .expect("materialization request"),
        )
        .expect("materialize deep layer");
    let after = child_state
        .reachability_snapshot()
        .expect("post-materialization snapshot");
    assert_eq!(after.facts().owned_table_count(), 1);
    assert_eq!(after.facts().inherited_table_count(), 1);
    assert!(after.table_refs().iter().any(|table_ref| matches!(
        table_ref.reference_kind(),
        BranchTableReferenceKind::Inherited {
            source_branch_id,
            layer_index: 0,
            ..
        } if source_branch_id == near_source
    )));
    assert!(after.table_refs().iter().any(|table_ref| matches!(
        table_ref.reference_kind(),
        BranchTableReferenceKind::Replacement {
            source_branch_id,
            fork_version,
        } if source_branch_id == deep_source && fork_version == CommitVersion::new(5)
    )));

    let aggregate_after = BranchReachabilityAggregate::from_snapshots(std::slice::from_ref(&after))
        .expect("aggregate after materialization");
    let release = BranchReleasePlan::from_removed_refs(
        child,
        removed_deep_refs,
        &aggregate_after,
        Some(&SharedTableRegistry::new()),
    )
    .expect("deep materialization release");
    assert_eq!(
        release
            .releasable_tables()
            .iter()
            .map(TableIdentity::as_str)
            .collect::<Vec<_>>(),
        vec![deep_identity],
    );
    assert!(release.protected_tables().is_empty());
    assert_eq!(
        aggregate_after.reference_count_for(&TableIdentity::new(near_identity).expect("near")),
        1,
    );
    assert_eq!(
        aggregate_after.reference_count_for(&TableIdentity::new(deep_identity).expect("deep")),
        0,
    );
}

#[test]
fn branch_reachability_rebuild_from_decoded_refs_is_deterministic_and_blocks_mismatches() {
    let parent = branch_id(127);
    let child = branch_id(128);
    let shared_identity = TableIdentity::new("reach-decoded-shared").expect("shared identity");
    let parent_ref = BranchTableRef::owned(parent, BranchLevel::ZERO, 0, shared_identity.clone())
        .expect("parent ref");
    let child_ref = BranchTableRef::inherited(
        child,
        parent,
        CommitVersion::new(7),
        0,
        BranchLevel::ZERO,
        0,
        shared_identity.clone(),
    )
    .expect("child ref");
    let parent_snapshot =
        BranchReachabilitySnapshot::new(parent, vec![parent_ref]).expect("parent snapshot");
    let child_snapshot =
        BranchReachabilitySnapshot::new(child, vec![child_ref]).expect("child snapshot");
    let aggregate_one = BranchReachabilityAggregate::from_snapshots(&[
        child_snapshot.clone(),
        parent_snapshot.clone(),
    ])
    .expect("aggregate one");
    let aggregate_two = BranchReachabilityAggregate::from_snapshots(&[
        child_snapshot.clone(),
        parent_snapshot.clone(),
    ])
    .expect("aggregate two");
    assert_eq!(aggregate_one, aggregate_two);
    assert_eq!(aggregate_one.reference_count_for(&shared_identity), 2);
    assert!(aggregate_one.is_shared(&shared_identity));
    assert!(matches!(
        BranchReachabilityAggregate::from_snapshots(&[
            parent_snapshot.clone(),
            parent_snapshot.clone(),
        ]),
        Err(BranchRuntimeError::InvalidReachability { .. })
    ));

    let mut registry = SharedTableRegistry::new();
    registry
        .register_snapshot(&child_snapshot)
        .expect("register child only");
    let release = BranchReleasePlan::from_removed_refs(
        parent,
        parent_snapshot.table_refs().to_vec(),
        &aggregate_one,
        Some(&registry),
    )
    .expect("mismatch release plan");
    assert_eq!(
        release.protected_tables()[0].reason(),
        BranchProtectionReason::RegistryDisagreement
    );
}

#[test]
fn branch_reachability_marks_materialized_tables_as_replacements() {
    let source = branch_id(129);
    let child = branch_id(130);
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![branch_inherited_layer(
            source,
            CommitVersion::new(4),
            InheritedLayerStatus::Active,
            vec![vec![branch_owned_table(
                source,
                BranchLevel::ZERO,
                "reach-materialize-source",
                vec![storage_row_with(
                    source,
                    b"materialized-replacement".to_vec(),
                    4,
                    40,
                    Timestamp::EPOCH,
                    b"source".to_vec(),
                )],
            )]],
        )])
        .expect("attach inherited layer");

    child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "reach-materialized")
                .expect("materialization request"),
        )
        .expect("materialize inherited layer");
    let snapshot = child_state
        .reachability_snapshot()
        .expect("replacement reachability snapshot");

    assert_eq!(snapshot.facts().owned_table_count(), 1);
    assert_eq!(snapshot.facts().inherited_table_count(), 0);
    assert!(matches!(
        snapshot.table_refs()[0].reference_kind(),
        BranchTableReferenceKind::Replacement {
            source_branch_id,
            fork_version,
        } if source_branch_id == source && fork_version == CommitVersion::new(4)
    ));
}
