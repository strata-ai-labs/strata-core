use super::*;
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    sort_table_rows_by_key, ImmutableTableBuilder, ImmutableTableReader, MutableTable,
    TableBuilderConfig, TableCommitRange, TableIdentity, TableInternalKeyBytes, TableKeyRange,
    TablePhysicalKeyBytes, TableReaderConfig, TableRow, TableRuntimeFacts,
};
use std::error::Error;
use std::fmt;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[test]
fn branch_runtime_config_rejects_unusable_zero_limits() {
    let default_config = BranchRuntimeConfig::default();
    assert_eq!(default_config.max_level_count(), 7);
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
        4,
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
    assert!(matches!(
        BranchReachabilitySnapshot::new(branch_id(113), vec![owned]),
        Err(BranchRuntimeError::InvalidReachability { .. })
    ));
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
        CommitVersion::new(4),
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
            materialization_layer_index: 1,
        }
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
            materialization_layer_index: 0,
        }
    ));
}

#[test]
fn branch_row_result_shells_preserve_row_and_source() {
    let row = storage_row(branch_id(4), 17);
    let visible = BranchVisibleRow::new(row.clone(), BranchRowSource::Active);
    assert_eq!(visible.row(), &row);
    assert_eq!(visible.source(), BranchRowSource::Active);

    let source = BranchRowSource::OwnedTable {
        level: BranchLevel::ZERO,
        table_index: 2,
    };
    let history = BranchHistoryRow::new(row.clone(), source);
    assert_eq!(history.row(), &row);
    assert_eq!(history.source(), source);

    let inherited_source = BranchRowSource::Inherited {
        source_branch_id: branch_id(5),
        layer_index: 1,
    };
    let inherited = BranchVisibleRow::new(row, inherited_source);
    assert_eq!(inherited.source(), inherited_source);

    let frozen_source = BranchRowSource::Frozen { index: 3 };
    let frozen = BranchHistoryRow::new(storage_row(branch_id(4), 18), frozen_source);
    assert_eq!(frozen.source(), frozen_source);
}

#[test]
fn branch_row_identity_accepts_matching_rows_and_rejects_mismatches() {
    let expected = branch_id(10);
    let other = branch_id(11);
    let row = storage_row(expected, 21);

    assert!(row_matches_branch(expected, &row));
    assert!(!row_matches_branch(other, &row));
    require_physical_key_branch(expected, row.physical_key()).expect("matching key");

    let identity: BranchRowIdentity =
        require_row_branch(expected, &row).expect("matching row identity");
    assert_eq!(identity.branch_id(), expected);
    assert_eq!(identity.physical_key(), row.physical_key());
    assert_eq!(identity.commit_version(), CommitVersion::new(21));
    assert_eq!(identity.commit_timestamp(), Timestamp::from_micros(21));

    let mismatch = require_row_branch(other, &row).expect_err("branch mismatch");
    assert!(matches!(
        mismatch,
        BranchRuntimeError::InvalidBranchRow { .. }
    ));
    assert!(!mismatch.to_string().contains("row-bytes"));
}

#[test]
fn branch_physical_key_validation_accepts_opaque_edge_key_shapes() {
    let zero_branch = BranchId::from_bytes([0x00; BranchId::BYTE_LEN]);
    let high_branch = BranchId::from_bytes([0xff; BranchId::BYTE_LEN]);
    let mixed_branch = BranchId::from_bytes([
        0x00, 0x01, 0x02, 0x03, 0x80, 0x81, 0xfe, 0xff, 0x10, 0x20, 0x30, 0x40, 0x55, 0xaa, 0x7e,
        0xe7,
    ]);
    let keys = [
        physical_key_with(
            zero_branch,
            "system",
            StorageSpaceId::COMMIT_TIMELINE,
            Vec::new(),
        ),
        physical_key_with(
            high_branch,
            "default-prefix",
            StorageSpaceId::engine(0xff).expect("engine storage space"),
            vec![0x00, 0x00, 0xff],
        ),
        physical_key_with(
            mixed_branch,
            "default",
            StorageSpaceId::engine(0x20).expect("engine storage space"),
            vec![0x80, 0x00, 0x81, 0xfe],
        ),
    ];

    for key in &keys {
        require_physical_key_branch(key.branch_id(), key).expect("opaque branch key accepted");
        let same_branch = rewrite_physical_key_branch(key, key.branch_id())
            .expect("same-branch physical key rewrite");
        assert_eq!(same_branch, *key);
    }

    let rewritten =
        rewrite_physical_key_branch(&keys[1], mixed_branch).expect("physical key rewrite");
    assert_eq!(rewritten.branch_id(), mixed_branch);
    assert_eq!(rewritten.space(), "default-prefix");
    assert_eq!(
        rewritten.storage_space_id(),
        StorageSpaceId::engine(0xff).expect("engine storage space")
    );
    assert_eq!(rewritten.user_key(), &[0x00, 0x00, 0xff]);
    assert_eq!(
        rewrite_physical_key_branch(&rewritten, high_branch).expect("round trip key"),
        keys[1],
    );
    assert_eq!(
        &TablePhysicalKeyBytes::from_physical_key(&rewritten).as_slice()[..BranchId::BYTE_LEN],
        mixed_branch.as_bytes()
    );

    let mismatch =
        require_physical_key_branch(high_branch, &keys[0]).expect_err("wrong branch key");
    assert!(matches!(
        mismatch,
        BranchRuntimeError::InvalidBranchRow { .. }
    ));
    let text = mismatch.to_string();
    assert!(!text.contains("main"));
    assert!(!text.contains("default-prefix"));
    assert!(!text.contains("row-bytes"));
}

#[test]
fn branch_row_validation_accepts_put_tombstone_and_edge_rows_without_policy() {
    let branch = branch_id(20);
    let other = branch_id(21);
    let put = StorageRow::put(
        physical_key_with(
            branch,
            "system",
            StorageSpaceId::COMMIT_TIMELINE,
            Vec::new(),
        ),
        CommitVersion::ZERO,
        Timestamp::EPOCH,
        Timestamp::from_micros(1),
        Vec::new(),
    );
    let put_identity = require_row_branch(branch, &put).expect("edge put row");
    assert_eq!(put_identity.branch_id(), branch);
    assert_eq!(put_identity.physical_key(), put.physical_key());
    assert_eq!(put_identity.commit_version(), CommitVersion::ZERO);
    assert_eq!(put_identity.commit_timestamp(), Timestamp::EPOCH);
    assert_eq!(put.expires_at(), Timestamp::from_micros(1));
    assert!(put.value().is_empty());
    assert!(!put.is_tombstone());

    let tombstone = StorageRow::tombstone(
        physical_key_with(
            branch,
            "default",
            StorageSpaceId::engine(0x21).expect("engine storage space"),
            vec![0x00, 0xff],
        ),
        CommitVersion::MAX,
        Timestamp::MAX,
    );
    let tombstone_identity = require_row_branch(branch, &tombstone).expect("tombstone row");
    assert_eq!(tombstone_identity.physical_key(), tombstone.physical_key());
    assert_eq!(tombstone_identity.commit_version(), CommitVersion::MAX);
    assert_eq!(tombstone_identity.commit_timestamp(), Timestamp::MAX);
    assert!(tombstone.is_tombstone());
    assert!(tombstone.value().is_empty());

    let wrong_put = StorageRow::put(
        physical_key(other, b"wrong-put".to_vec()),
        CommitVersion::new(1),
        Timestamp::from_micros(1),
        Timestamp::EPOCH,
        b"secret-payload".to_vec(),
    );
    let wrong_tombstone = tombstone_row(other, b"wrong-delete".to_vec(), 2, 2);
    for row in [&wrong_put, &wrong_tombstone] {
        let error = require_row_branch(branch, row).expect_err("wrong-branch row rejected");
        assert!(matches!(error, BranchRuntimeError::InvalidBranchRow { .. }));
        assert!(!error.to_string().contains("secret-payload"));
    }
}

#[test]
fn branch_rewrite_preserves_put_and_tombstone_row_facts() {
    let source = branch_id(12);
    let target = branch_id(13);
    let put = storage_row_with(
        source,
        b"\x00user\x00\x00key".to_vec(),
        31,
        310,
        Timestamp::from_micros(999),
        b"secret-payload".to_vec(),
    );

    let rewritten_key =
        rewrite_physical_key_branch(put.physical_key(), target).expect("rewrite key");
    assert_eq!(rewritten_key.branch_id(), target);
    assert_eq!(rewritten_key.space(), put.physical_key().space());
    assert_eq!(
        rewritten_key.storage_space_id(),
        put.physical_key().storage_space_id()
    );
    assert_eq!(rewritten_key.user_key(), put.physical_key().user_key());

    let rewritten = rewrite_row_branch(&put, source, target).expect("rewrite put");
    assert_eq!(rewritten.physical_key().branch_id(), target);
    assert_eq!(rewritten.physical_key().space(), put.physical_key().space());
    assert_eq!(
        rewritten.physical_key().user_key(),
        put.physical_key().user_key()
    );
    assert_eq!(rewritten.commit_version(), put.commit_version());
    assert_eq!(rewritten.commit_timestamp(), put.commit_timestamp());
    assert_eq!(rewritten.expires_at(), put.expires_at());
    assert_eq!(rewritten.value(), put.value());
    assert!(!rewritten.is_tombstone());

    let round_trip = rewrite_row_branch(&rewritten, target, source).expect("round trip");
    assert_eq!(round_trip, put);
    assert_eq!(
        rewrite_row_branch(&put, source, source).expect("same branch rewrite"),
        put
    );
    assert!(matches!(
        rewrite_row_branch(&put, target, source),
        Err(BranchRuntimeError::InvalidBranchRow { .. })
    ));

    let tombstone = tombstone_row(source, b"deleted".to_vec(), 44, 440);
    let rewritten_tombstone =
        rewrite_row_branch(&tombstone, source, target).expect("rewrite tombstone");
    assert_eq!(rewritten_tombstone.physical_key().branch_id(), target);
    assert_eq!(
        rewritten_tombstone.physical_key().user_key(),
        tombstone.physical_key().user_key()
    );
    assert_eq!(rewritten_tombstone.commit_version(), CommitVersion::new(44));
    assert_eq!(
        rewritten_tombstone.commit_timestamp(),
        Timestamp::from_micros(440)
    );
    assert!(rewritten_tombstone.is_tombstone());
    assert!(rewritten_tombstone.value().is_empty());
}

#[test]
fn branch_rewrite_preserves_empty_put_values_and_storage_owned_keys() {
    let source = branch_id(22);
    let target = branch_id(23);
    let put = StorageRow::put(
        physical_key_with(
            source,
            "system",
            StorageSpaceId::COMMIT_TIMELINE,
            Vec::new(),
        ),
        CommitVersion::new(9),
        Timestamp::from_micros(90),
        Timestamp::from_micros(100),
        Vec::new(),
    );

    let rewritten = rewrite_row_branch(&put, source, target).expect("rewrite empty put");
    assert_eq!(rewritten.physical_key().branch_id(), target);
    assert_eq!(rewritten.physical_key().space(), "system");
    assert_eq!(
        rewritten.physical_key().storage_space_id(),
        StorageSpaceId::COMMIT_TIMELINE
    );
    assert!(rewritten.physical_key().user_key().is_empty());
    assert_eq!(rewritten.commit_version(), put.commit_version());
    assert_eq!(rewritten.commit_timestamp(), put.commit_timestamp());
    assert_eq!(rewritten.expires_at(), put.expires_at());
    assert!(rewritten.value().is_empty());
    assert!(!rewritten.is_tombstone());
}

#[test]
fn branch_rewrite_groups_inherited_rows_with_child_local_encoded_keys() {
    let source = branch_id(16);
    let target = branch_id(17);
    let user_key = b"shared-logical-key".to_vec();
    let inherited = storage_row_with(
        source,
        user_key.clone(),
        7,
        70,
        Timestamp::EPOCH,
        b"inherited".to_vec(),
    );
    let child_local =
        storage_row_with(target, user_key, 5, 50, Timestamp::EPOCH, b"child".to_vec());

    let rewritten = rewrite_row_branch(&inherited, source, target).expect("rewrite inherited row");
    let rewritten_prefix = TablePhysicalKeyBytes::from_row(&rewritten);
    let child_prefix = TablePhysicalKeyBytes::from_row(&child_local);
    assert_eq!(rewritten_prefix.as_slice(), child_prefix.as_slice());
    assert_eq!(
        &rewritten_prefix.as_slice()[..BranchId::BYTE_LEN],
        target.as_bytes()
    );

    let mut rows = vec![TableRow::new(child_local), TableRow::new(rewritten)];
    sort_table_rows_by_key(&mut rows);
    assert_eq!(rows[0].commit_version(), CommitVersion::new(7));
    assert_eq!(rows[1].commit_version(), CommitVersion::new(5));
    assert_eq!(
        TablePhysicalKeyBytes::from_row(rows[0].row()).as_slice(),
        TablePhysicalKeyBytes::from_row(rows[1].row()).as_slice()
    );
}

#[test]
fn branch_effective_read_bounds_apply_inclusive_own_and_inherited_caps() {
    let row = storage_row_with(
        branch_id(14),
        b"bounded".to_vec(),
        50,
        500,
        Timestamp::EPOCH,
        b"value".to_vec(),
    );

    let latest = BranchEffectiveReadBound::for_own_branch(BranchReadBound::latest());
    assert_eq!(latest.max_commit_version(), None);
    assert_eq!(latest.max_commit_timestamp(), None);
    assert!(latest.matches_row(&row).matches_effective_bound());

    let exact_version = BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_version(
        CommitVersion::new(50),
    ));
    assert!(exact_version.matches_row(&row).matches_effective_bound());
    let before_version = BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_version(
        CommitVersion::new(49),
    ));
    assert!(!before_version.matches_row(&row).version_in_bound());

    let exact_timestamp = BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_timestamp(
        Timestamp::from_micros(500),
    ));
    assert!(exact_timestamp.matches_row(&row).matches_effective_bound());
    let before_timestamp = BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_timestamp(
        Timestamp::from_micros(499),
    ));
    assert!(!before_timestamp.matches_row(&row).timestamp_in_bound());

    let inherited_latest = BranchEffectiveReadBound::for_inherited_layer(
        BranchReadBound::latest(),
        CommitVersion::new(50),
    );
    assert_eq!(
        inherited_latest.max_commit_version(),
        Some(CommitVersion::new(50))
    );
    assert!(inherited_latest.matches_row(&row).matches_effective_bound());

    let inherited_timestamp = BranchEffectiveReadBound::for_inherited_layer(
        BranchReadBound::at_timestamp(Timestamp::from_micros(500)),
        CommitVersion::new(49),
    );
    let inherited_match: BranchRowBoundMatch = inherited_timestamp.matches_row(&row);
    assert_eq!(
        inherited_timestamp.max_commit_version(),
        Some(CommitVersion::new(49))
    );
    assert_eq!(
        inherited_timestamp.max_commit_timestamp(),
        Some(Timestamp::from_micros(500))
    );
    assert!(!inherited_match.version_in_bound());
    assert!(inherited_match.timestamp_in_bound());
    assert!(!inherited_match.matches_effective_bound());

    let inherited_version = BranchEffectiveReadBound::for_inherited_layer(
        BranchReadBound::at_version(CommitVersion::new(70)),
        CommitVersion::new(55),
    );
    assert_eq!(
        inherited_version.max_commit_version(),
        Some(CommitVersion::new(55))
    );
}

#[test]
fn branch_own_bounds_cover_zero_epoch_and_below_equal_above_edges() {
    let branch = branch_id(24);
    let zero_row = storage_row_with(
        branch,
        b"zero".to_vec(),
        0,
        0,
        Timestamp::EPOCH,
        b"zero".to_vec(),
    );
    let later_row = storage_row_with(
        branch,
        b"later".to_vec(),
        1,
        1,
        Timestamp::EPOCH,
        b"later".to_vec(),
    );

    let latest = BranchEffectiveReadBound::for_own_branch(BranchReadBound::latest());
    assert!(latest.matches_row(&zero_row).matches_effective_bound());
    assert!(latest.matches_row(&later_row).matches_effective_bound());

    let version_zero =
        BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_version(CommitVersion::ZERO));
    assert_eq!(version_zero.max_commit_version(), Some(CommitVersion::ZERO));
    assert_eq!(version_zero.max_commit_timestamp(), None);
    assert!(version_zero
        .matches_row(&zero_row)
        .matches_effective_bound());
    assert!(!version_zero.matches_row(&later_row).version_in_bound());

    let version_one = BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_version(
        CommitVersion::new(1),
    ));
    assert!(version_one.matches_row(&zero_row).version_in_bound());
    assert!(version_one.matches_row(&later_row).version_in_bound());

    let timestamp_epoch =
        BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_timestamp(Timestamp::EPOCH));
    assert_eq!(timestamp_epoch.max_commit_version(), None);
    assert_eq!(
        timestamp_epoch.max_commit_timestamp(),
        Some(Timestamp::EPOCH)
    );
    assert!(timestamp_epoch
        .matches_row(&zero_row)
        .matches_effective_bound());
    assert!(!timestamp_epoch.matches_row(&later_row).timestamp_in_bound());

    let timestamp_one = BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_timestamp(
        Timestamp::from_micros(1),
    ));
    assert!(timestamp_one.matches_row(&zero_row).timestamp_in_bound());
    assert!(timestamp_one.matches_row(&later_row).timestamp_in_bound());
}

#[test]
fn branch_inherited_bounds_cover_fork_edges_and_combined_timestamp_match() {
    let branch = branch_id(25);
    let fork_version = CommitVersion::new(4);
    let row_at_fork = storage_row_with(
        branch,
        b"at-fork".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"at".to_vec(),
    );
    let row_after_fork = storage_row_with(
        branch,
        b"after-fork".to_vec(),
        5,
        40,
        Timestamp::EPOCH,
        b"after".to_vec(),
    );
    let row_after_timestamp = storage_row_with(
        branch,
        b"after-time".to_vec(),
        3,
        41,
        Timestamp::EPOCH,
        b"after-time".to_vec(),
    );

    let latest =
        BranchEffectiveReadBound::for_inherited_layer(BranchReadBound::latest(), fork_version);
    assert_eq!(latest.max_commit_version(), Some(fork_version));
    assert!(latest.matches_row(&row_at_fork).matches_effective_bound());
    assert!(!latest.matches_row(&row_after_fork).version_in_bound());

    for (requested, expected) in [
        (CommitVersion::new(3), CommitVersion::new(3)),
        (CommitVersion::new(4), CommitVersion::new(4)),
        (CommitVersion::new(5), CommitVersion::new(4)),
    ] {
        let bound = BranchEffectiveReadBound::for_inherited_layer(
            BranchReadBound::at_version(requested),
            fork_version,
        );
        assert_eq!(bound.max_commit_version(), Some(expected));
        assert_eq!(bound.max_commit_timestamp(), None);
    }

    let timestamp_bound = BranchEffectiveReadBound::for_inherited_layer(
        BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        fork_version,
    );
    assert_eq!(timestamp_bound.max_commit_version(), Some(fork_version));
    assert_eq!(
        timestamp_bound.max_commit_timestamp(),
        Some(Timestamp::from_micros(40))
    );
    assert!(timestamp_bound
        .matches_row(&row_at_fork)
        .matches_effective_bound());

    let after_fork_match = timestamp_bound.matches_row(&row_after_fork);
    assert!(!after_fork_match.version_in_bound());
    assert!(after_fork_match.timestamp_in_bound());
    assert!(!after_fork_match.matches_effective_bound());

    let after_timestamp_match = timestamp_bound.matches_row(&row_after_timestamp);
    assert!(after_timestamp_match.version_in_bound());
    assert!(!after_timestamp_match.timestamp_in_bound());
    assert!(!after_timestamp_match.matches_effective_bound());
}

#[test]
fn branch_effective_bounds_filter_row_chains_without_collapsing_versions() {
    let branch = branch_id(18);
    let user_key = b"row-chain".to_vec();
    let mut rows = vec![
        TableRow::new(storage_row_with(
            branch,
            user_key.clone(),
            3,
            30,
            Timestamp::from_micros(25),
            b"expired-looking".to_vec(),
        )),
        TableRow::new(storage_row_with(
            branch,
            user_key.clone(),
            5,
            50,
            Timestamp::EPOCH,
            b"newest".to_vec(),
        )),
        TableRow::new(tombstone_row(branch, user_key.clone(), 4, 40)),
        TableRow::new(storage_row_with(
            branch,
            user_key,
            2,
            60,
            Timestamp::EPOCH,
            b"timestamp-late".to_vec(),
        )),
    ];
    sort_table_rows_by_key(&mut rows);
    assert_eq!(row_versions(&rows), vec![5, 4, 3, 2]);

    let version_bound = BranchEffectiveReadBound::new(Some(CommitVersion::new(4)), None);
    assert_eq!(matching_versions(&rows, version_bound), vec![4, 3, 2]);

    let timestamp_bound = BranchEffectiveReadBound::new(None, Some(Timestamp::from_micros(40)));
    assert_eq!(matching_versions(&rows, timestamp_bound), vec![4, 3]);

    let combined_bound = BranchEffectiveReadBound::new(
        Some(CommitVersion::new(4)),
        Some(Timestamp::from_micros(40)),
    );
    let combined = matching_versions(&rows, combined_bound);
    assert_eq!(combined, vec![4, 3]);
    assert!(
        combined.len() > 1,
        "L6B candidate filtering must not collapse row history into one visible row",
    );

    let candidates = rows
        .iter()
        .map(|row| {
            BranchRowCandidateFacts::from_row(row.row(), BranchRowSource::Active, combined_bound)
        })
        .filter(|candidate| candidate.bound_match().matches_effective_bound())
        .collect::<Vec<_>>();
    assert!(candidates.iter().any(BranchRowCandidateFacts::is_tombstone));
    assert!(candidates
        .iter()
        .any(|candidate| candidate.expires_at() == Timestamp::from_micros(25)));

    let wrong_branch = storage_row(branch_id(19), 4);
    assert!(matches!(
        require_row_branch(branch, &wrong_branch),
        Err(BranchRuntimeError::InvalidBranchRow { .. })
    ));
}

#[test]
fn branch_candidate_facts_preserve_tombstone_and_expiry_without_visibility_policy() {
    let branch = branch_id(15);
    let expired_looking = storage_row_with(
        branch,
        b"expired".to_vec(),
        10,
        100,
        Timestamp::from_micros(90),
        b"value".to_vec(),
    );
    let bound = BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_timestamp(
        Timestamp::from_micros(100),
    ));
    let facts = BranchRowCandidateFacts::from_row(&expired_looking, BranchRowSource::Active, bound);
    assert_eq!(facts.source(), BranchRowSource::Active);
    assert_eq!(facts.physical_key(), expired_looking.physical_key());
    assert_eq!(facts.commit_version(), CommitVersion::new(10));
    assert_eq!(facts.commit_timestamp(), Timestamp::from_micros(100));
    assert_eq!(facts.expires_at(), Timestamp::from_micros(90));
    assert!(!facts.is_tombstone());
    assert!(facts.bound_match().matches_effective_bound());

    let tombstone = tombstone_row(branch, b"deleted".to_vec(), 11, 100);
    let tombstone_facts =
        BranchRowCandidateFacts::from_row(&tombstone, BranchRowSource::Frozen { index: 0 }, bound);
    assert_eq!(
        tombstone_facts.source(),
        BranchRowSource::Frozen { index: 0 }
    );
    assert!(tombstone_facts.is_tombstone());
    assert!(tombstone_facts.bound_match().matches_effective_bound());
}

#[test]
fn branch_candidate_bound_match_records_each_axis_independently() {
    let row = storage_row_with(
        branch_id(26),
        b"axis".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"value".to_vec(),
    );

    let version_miss = BranchRowCandidateFacts::from_row(
        &row,
        BranchRowSource::Active,
        BranchEffectiveReadBound::new(
            Some(CommitVersion::new(4)),
            Some(Timestamp::from_micros(50)),
        ),
    );
    assert!(!version_miss.bound_match().version_in_bound());
    assert!(version_miss.bound_match().timestamp_in_bound());
    assert!(!version_miss.bound_match().matches_effective_bound());

    let timestamp_miss = BranchRowCandidateFacts::from_row(
        &row,
        BranchRowSource::Active,
        BranchEffectiveReadBound::new(
            Some(CommitVersion::new(5)),
            Some(Timestamp::from_micros(49)),
        ),
    );
    assert!(timestamp_miss.bound_match().version_in_bound());
    assert!(!timestamp_miss.bound_match().timestamp_in_bound());
    assert!(!timestamp_miss.bound_match().matches_effective_bound());

    let both_miss = BranchRowCandidateFacts::from_row(
        &row,
        BranchRowSource::Active,
        BranchEffectiveReadBound::new(
            Some(CommitVersion::new(4)),
            Some(Timestamp::from_micros(49)),
        ),
    );
    assert!(!both_miss.bound_match().version_in_bound());
    assert!(!both_miss.bound_match().timestamp_in_bound());
    assert!(!both_miss.bound_match().matches_effective_bound());

    let latest = BranchRowCandidateFacts::from_row(
        &row,
        BranchRowSource::Active,
        BranchEffectiveReadBound::for_own_branch(BranchReadBound::latest()),
    );
    assert!(latest.bound_match().matches_effective_bound());
}

#[test]
fn branch_local_state_constructs_empty_active_state() {
    let branch = branch_id(30);
    let config = BranchRuntimeConfig::new(3, 4, 2).expect("valid config");
    let state = BranchLocalState::new(branch, config).expect("branch-local state");

    assert_eq!(state.branch_id(), branch);
    assert_eq!(state.config(), config);
    assert!(state.is_empty());
    assert_eq!(state.active_row_count(), 0);
    assert!(state.active().is_empty());
    assert!(state.frozen().is_empty());
    assert_eq!(state.frozen_table_count(), 0);
    assert_eq!(state.max_commit_version(), None);
    assert_eq!(state.timestamp_min(), None);
    assert_eq!(state.timestamp_max(), None);
    assert_eq!(state.put_rows(), 0);
    assert_eq!(state.tombstone_rows(), 0);
    assert_eq!(
        state.facts().expect("facts"),
        BranchStateFacts::empty(branch)
    );

    let default_state = BranchLocalState::empty(branch);
    assert_eq!(default_state.branch_id(), branch);
    assert!(default_state.is_empty());
}

#[test]
fn branch_local_state_appends_puts_tombstones_and_preserves_row_facts() {
    let branch = branch_id(31);
    let mut state = BranchLocalState::empty(branch);
    let put = storage_row_with(
        branch,
        b"same-logical-key".to_vec(),
        5,
        50,
        Timestamp::from_micros(500),
        b"secret-payload".to_vec(),
    );
    let older_same_key = storage_row_with(
        branch,
        b"same-logical-key".to_vec(),
        4,
        40,
        Timestamp::from_micros(40),
        Vec::new(),
    );
    let same_version_other_key = storage_row_with(
        branch,
        b"other-logical-key".to_vec(),
        5,
        60,
        Timestamp::EPOCH,
        b"other".to_vec(),
    );
    let tombstone = tombstone_row(branch, b"deleted".to_vec(), 9, 30);

    let put_outcome: BranchAppendOutcome = state
        .append_committed_row(put.clone())
        .expect("append put row");
    assert_eq!(put_outcome.branch_id(), branch);
    assert_eq!(put_outcome.commit_version(), CommitVersion::new(5));
    assert_eq!(put_outcome.commit_timestamp(), Timestamp::from_micros(50));
    assert!(!put_outcome.is_tombstone());
    assert_eq!(put_outcome.active_rows(), 1);
    assert!(put_outcome.approximate_active_bytes() > 0);
    assert_eq!(
        state
            .active()
            .get(&TableInternalKeyBytes::from_row(&put))
            .expect("stored put")
            .row(),
        &put
    );

    state
        .append_committed_row(older_same_key.clone())
        .expect("same physical key at different version");
    state
        .append_committed_row(same_version_other_key.clone())
        .expect("different physical key at same version");
    let tombstone_outcome = state
        .append_committed_row(tombstone.clone())
        .expect("append tombstone row");
    assert!(tombstone_outcome.is_tombstone());
    assert_eq!(state.active_row_count(), 4);
    assert_eq!(state.put_rows(), 3);
    assert_eq!(state.tombstone_rows(), 1);
    for row in [&older_same_key, &same_version_other_key, &tombstone] {
        assert_eq!(
            state
                .active()
                .get(&TableInternalKeyBytes::from_row(row))
                .expect("stored row")
                .row(),
            row
        );
    }

    let facts = state.facts().expect("facts");
    assert_eq!(facts.active_rows(), 4);
    assert_eq!(facts.frozen_table_count(), 0);
    assert_eq!(facts.owned_table_count(), 0);
    assert_eq!(facts.inherited_layer_count(), 0);
    assert_eq!(facts.max_commit_version(), Some(CommitVersion::new(9)));
    assert_eq!(facts.timestamp_min(), Some(Timestamp::from_micros(30)));
    assert_eq!(facts.timestamp_max(), Some(Timestamp::from_micros(60)));
}

#[test]
fn branch_local_state_tracks_zero_max_version_and_timestamp_edges() {
    let branch = branch_id(37);
    let mut state = BranchLocalState::empty(branch);
    let zero = storage_row_with(branch, b"zero".to_vec(), 0, 0, Timestamp::EPOCH, Vec::new());
    let max = storage_row_with(
        branch,
        b"max".to_vec(),
        u64::MAX,
        u64::MAX,
        Timestamp::MAX,
        b"max".to_vec(),
    );

    state
        .append_committed_row(zero)
        .expect("append zero edge row");
    let zero_facts = state.facts().expect("zero facts");
    assert_eq!(zero_facts.max_commit_version(), Some(CommitVersion::ZERO));
    assert_eq!(zero_facts.timestamp_min(), Some(Timestamp::EPOCH));
    assert_eq!(zero_facts.timestamp_max(), Some(Timestamp::EPOCH));

    state
        .append_committed_row(max)
        .expect("append max edge row");
    let max_facts = state.facts().expect("max facts");
    assert_eq!(max_facts.max_commit_version(), Some(CommitVersion::MAX));
    assert_eq!(max_facts.timestamp_min(), Some(Timestamp::EPOCH));
    assert_eq!(max_facts.timestamp_max(), Some(Timestamp::MAX));

    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated {
            frozen_index: 0,
            frozen_rows: 2,
            frozen_tables: 1,
        }
    ));
    let rotated_facts = state.facts().expect("rotated facts");
    assert_eq!(rotated_facts.active_rows(), 0);
    assert_eq!(rotated_facts.frozen_table_count(), 1);
    assert_eq!(rotated_facts.max_commit_version(), Some(CommitVersion::MAX));
    assert_eq!(rotated_facts.timestamp_min(), Some(Timestamp::EPOCH));
    assert_eq!(rotated_facts.timestamp_max(), Some(Timestamp::MAX));
}

#[test]
fn branch_local_state_accepts_opaque_branch_ids_and_key_shapes() {
    let branch_ids = [
        BranchId::from_bytes([0x00; BranchId::BYTE_LEN]),
        BranchId::from_bytes([0xff; BranchId::BYTE_LEN]),
        BranchId::from_bytes([
            0x00, 0x01, 0x02, 0x03, 0x80, 0x81, 0xfe, 0xff, 0x10, 0x20, 0x30, 0x40, 0x55, 0xaa,
            0x7e, 0xe7,
        ]),
    ];

    for branch in branch_ids {
        let mut state = BranchLocalState::empty(branch);
        let rows = [
            StorageRow::put(
                physical_key_with(
                    branch,
                    "system",
                    StorageSpaceId::COMMIT_TIMELINE,
                    Vec::new(),
                ),
                CommitVersion::new(1),
                Timestamp::from_micros(100),
                Timestamp::EPOCH,
                Vec::new(),
            ),
            storage_row_with(
                branch,
                vec![0x00, 0x00, 0xff],
                2,
                100,
                Timestamp::from_micros(1),
                b"nul-key".to_vec(),
            ),
            storage_row_with(
                branch,
                vec![0x80, 0x00, 0xfe, 0xff],
                3,
                90,
                Timestamp::EPOCH,
                b"high-bit".to_vec(),
            ),
            StorageRow::put(
                physical_key_with(
                    branch,
                    "default-prefix",
                    StorageSpaceId::engine(0xff).expect("engine storage space"),
                    b"shared-prefix-key".to_vec(),
                ),
                CommitVersion::new(4),
                Timestamp::from_micros(110),
                Timestamp::EPOCH,
                b"prefixed-space".to_vec(),
            ),
        ];

        for row in &rows {
            state
                .append_committed_row(row.clone())
                .expect("append opaque branch/key row");
            assert_eq!(
                state
                    .active()
                    .get(&TableInternalKeyBytes::from_row(row))
                    .expect("stored opaque row")
                    .row(),
                row
            );
        }

        let facts = state.facts().expect("opaque facts");
        assert_eq!(
            facts.active_rows(),
            u64::try_from(rows.len()).expect("row count fits in u64")
        );
        assert_eq!(facts.frozen_table_count(), 0);
        assert_eq!(facts.max_commit_version(), Some(CommitVersion::new(4)));
        assert_eq!(facts.timestamp_min(), Some(Timestamp::from_micros(90)));
        assert_eq!(facts.timestamp_max(), Some(Timestamp::from_micros(110)));
    }
}

#[test]
fn branch_local_state_rejects_wrong_branch_rows_without_mutation() {
    let branch = branch_id(32);
    let wrong_branch = branch_id(33);
    let mut state = BranchLocalState::empty(branch);
    let baseline_facts = state.facts().expect("baseline facts");
    let wrong_put = storage_row_with(
        wrong_branch,
        b"wrong".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"secret-payload".to_vec(),
    );
    let wrong_tombstone = tombstone_row(wrong_branch, b"wrong-delete".to_vec(), 2, 20);

    for row in [wrong_put, wrong_tombstone] {
        let error = state
            .append_committed_row(row)
            .expect_err("wrong branch rejected");
        assert!(matches!(error, BranchRuntimeError::InvalidBranchRow { .. }));
        assert!(!error.to_string().contains("secret-payload"));
        assert_eq!(
            state.facts().expect("facts after rejection"),
            baseline_facts
        );
        assert!(state.active().is_empty());
        assert!(state.frozen().is_empty());
    }
}

#[test]
fn branch_local_state_rejects_active_and_frozen_duplicates_without_mutation() {
    let branch = branch_id(34);
    let mut state = BranchLocalState::empty(branch);
    let row = storage_row_with(
        branch,
        b"duplicate".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        b"secret-payload".to_vec(),
    );

    state
        .append_committed_row(row.clone())
        .expect("initial append");
    let facts_after_append = state.facts().expect("append facts");
    let active_duplicate = state
        .append_committed_row(row.clone())
        .expect_err("active duplicate rejected");
    assert_duplicate_internal_key(&active_duplicate);
    assert_eq!(
        state.facts().expect("after active duplicate"),
        facts_after_append
    );
    assert_eq!(state.active_row_count(), 1);

    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated {
            frozen_index: 0,
            frozen_rows: 1,
            frozen_tables: 1,
        }
    ));
    let facts_after_rotation = state.facts().expect("rotation facts");
    let frozen_duplicate = state
        .append_committed_row(row)
        .expect_err("frozen duplicate rejected");
    assert_duplicate_internal_key(&frozen_duplicate);
    assert_eq!(
        state.facts().expect("after frozen duplicate"),
        facts_after_rotation
    );
    assert_eq!(state.active_row_count(), 0);
    assert_eq!(state.frozen_table_count(), 1);
}

#[test]
fn branch_local_state_rotation_preserves_rows_and_newest_first_order() {
    let branch = branch_id(35);
    let mut state =
        BranchLocalState::new(branch, BranchRuntimeConfig::new(7, 64, 4).expect("config"))
            .expect("state");

    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Skipped {
            reason: BranchRotationSkipReason::EmptyActive,
        }
    ));
    assert!(state.frozen().is_empty());

    let first = storage_row_with(
        branch,
        b"first".to_vec(),
        1,
        100,
        Timestamp::EPOCH,
        b"first".to_vec(),
    );
    state
        .append_committed_row(first.clone())
        .expect("append first");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated {
            frozen_index: 0,
            frozen_rows: 1,
            frozen_tables: 1,
        }
    ));
    assert!(state.active().is_empty());
    assert_eq!(
        state.frozen()[0]
            .get(&TableInternalKeyBytes::from_row(&first))
            .expect("first frozen row")
            .row(),
        &first
    );

    let second = storage_row_with(
        branch,
        b"second".to_vec(),
        2,
        50,
        Timestamp::EPOCH,
        b"second".to_vec(),
    );
    state
        .append_committed_row(second.clone())
        .expect("append second");
    assert_eq!(
        state.frozen()[0]
            .get(&TableInternalKeyBytes::from_row(&first))
            .expect("frozen row unchanged after active append")
            .row(),
        &first
    );
    assert_eq!(
        state
            .active()
            .get(&TableInternalKeyBytes::from_row(&second))
            .expect("second active row")
            .row(),
        &second
    );
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated {
            frozen_index: 0,
            frozen_rows: 1,
            frozen_tables: 2,
        }
    ));

    assert_eq!(
        state.frozen()[0]
            .get(&TableInternalKeyBytes::from_row(&second))
            .expect("newest frozen row")
            .row(),
        &second
    );
    assert_eq!(
        state.frozen()[1]
            .get(&TableInternalKeyBytes::from_row(&first))
            .expect("older frozen row")
            .row(),
        &first
    );
    let facts = state.facts().expect("facts");
    assert_eq!(facts.active_rows(), 0);
    assert_eq!(facts.frozen_table_count(), 2);
    assert_eq!(facts.max_commit_version(), Some(CommitVersion::new(2)));
    assert_eq!(facts.timestamp_min(), Some(Timestamp::from_micros(50)));
    assert_eq!(facts.timestamp_max(), Some(Timestamp::from_micros(100)));
}

#[test]
fn branch_local_state_respects_frozen_limit_without_dropping_active_rows() {
    let branch = branch_id(36);
    let config = BranchRuntimeConfig::new(7, 64, 1).expect("config");
    let mut state = BranchLocalState::new(branch, config).expect("state");
    let frozen_row = storage_row(branch, 1);
    let active_row = storage_row_with(
        branch,
        b"still-active".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"active".to_vec(),
    );
    let additional_active_row = storage_row_with(
        branch,
        b"still-active-too".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"active-too".to_vec(),
    );

    state
        .append_committed_row(frozen_row.clone())
        .expect("append frozen row");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .append_committed_row(active_row.clone())
        .expect("append active row");
    let before_skip = state.facts().expect("before skip facts");

    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Skipped {
            reason: BranchRotationSkipReason::FrozenLimitReached,
        }
    ));
    assert_eq!(state.facts().expect("after skip facts"), before_skip);
    assert_eq!(state.active_row_count(), 1);
    assert_eq!(state.frozen_table_count(), 1);
    assert!(state
        .active()
        .get(&TableInternalKeyBytes::from_row(&active_row))
        .is_some());
    assert!(state.frozen()[0]
        .get(&TableInternalKeyBytes::from_row(&frozen_row))
        .is_some());

    state
        .append_committed_row(additional_active_row.clone())
        .expect("append after frozen-limit skip");
    assert_eq!(state.active_row_count(), 2);
    assert_eq!(state.frozen_table_count(), 1);
    assert_eq!(
        state
            .active()
            .get(&TableInternalKeyBytes::from_row(&additional_active_row))
            .expect("additional active row")
            .row(),
        &additional_active_row
    );
    let after_append = state.facts().expect("after append facts");
    assert_eq!(after_append.active_rows(), 2);
    assert_eq!(after_append.frozen_table_count(), 1);
    assert_eq!(
        after_append.max_commit_version(),
        Some(CommitVersion::new(3))
    );
}

#[test]
fn branch_read_view_is_pinned_across_append_and_rotation() {
    let branch = branch_id(38);
    let mut state = BranchLocalState::empty(branch);
    let first = storage_row_with(
        branch,
        b"pinned".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"first".to_vec(),
    );
    let tombstone = tombstone_row(branch, b"pinned".to_vec(), 2, 20);

    state
        .append_committed_row(first.clone())
        .expect("append first");
    let view = state.capture_read_view().expect("capture read view");
    let captured_facts = view.facts();

    state
        .append_committed_row(tombstone.clone())
        .expect("append tombstone after capture");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    let after = state.capture_read_view().expect("capture after mutation");

    let key = physical_key(branch, b"pinned".to_vec());
    let visible = view.latest(&key).expect("pinned latest").expect("row");
    assert_eq!(visible.row(), &first);
    assert_eq!(visible.source(), BranchRowSource::Active);
    assert_eq!(view.facts(), captured_facts);
    assert_eq!(view.active_row_count(), 1);
    assert_eq!(view.frozen_table_count(), 0);

    assert_eq!(after.latest(&key).expect("after latest"), None);
    assert_eq!(after.active_row_count(), 0);
    assert_eq!(after.frozen_table_count(), 1);
    assert_eq!(
        after
            .history(&key, BranchHistoryOptions::all())
            .expect("after history")
            .iter()
            .map(|row| row.row().commit_version().as_u64())
            .collect::<Vec<_>>(),
        vec![2, 1],
    );
}

#[test]
fn branch_read_view_constructor_rejects_stale_facts_and_wrong_branch_sources() {
    let branch = branch_id(43);
    let other = branch_id(44);
    let row = storage_row_with(
        branch,
        b"constructor".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"value".to_vec(),
    );
    let mut active = MutableTable::new();
    active.insert_row(row.clone()).expect("insert row");
    let valid_facts = BranchStateFacts::new(
        branch,
        1,
        0,
        0,
        0,
        Some(CommitVersion::new(3)),
        Some(Timestamp::from_micros(30)),
        Some(Timestamp::from_micros(30)),
    )
    .expect("valid facts");
    BranchReadView::new(branch, active.clone(), Vec::new(), Vec::new(), valid_facts)
        .expect("valid read view");

    assert!(matches!(
        BranchReadView::new(
            branch,
            active.clone(),
            Vec::new(),
            Vec::new(),
            BranchStateFacts::empty(branch)
        ),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));

    let stale_facts = BranchStateFacts::new(
        branch,
        1,
        0,
        0,
        0,
        Some(CommitVersion::new(2)),
        Some(Timestamp::from_micros(30)),
        Some(Timestamp::from_micros(30)),
    )
    .expect("stale facts shape");
    assert!(matches!(
        BranchReadView::new(branch, active.clone(), Vec::new(), Vec::new(), stale_facts),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));

    let unsupported_inherited_facts =
        BranchStateFacts::new(branch, 0, 0, 0, 1, None, None, None).expect("inherited facts shape");
    assert!(matches!(
        BranchReadView::new(
            branch,
            MutableTable::new(),
            Vec::new(),
            Vec::new(),
            unsupported_inherited_facts
        ),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));

    let wrong_branch_row = storage_row_with(
        other,
        b"constructor".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"secret-payload".to_vec(),
    );
    let mut wrong_active = MutableTable::new();
    wrong_active
        .insert_row(wrong_branch_row)
        .expect("insert wrong row");
    let wrong_error =
        BranchReadView::new(branch, wrong_active, Vec::new(), Vec::new(), valid_facts)
            .expect_err("wrong-branch source rejected");
    assert!(matches!(
        wrong_error,
        BranchRuntimeError::InvalidBranchState { .. }
    ));
    assert!(!wrong_error.to_string().contains("secret-payload"));
}

#[test]
fn branch_read_view_constructor_rejects_frozen_source_and_fact_mismatches() {
    let branch = branch_id(45);
    let other = branch_id(46);
    let row = storage_row_with(
        branch,
        b"frozen-constructor".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"value".to_vec(),
    );
    let mut frozen_source = MutableTable::new();
    frozen_source.insert_row(row).expect("insert frozen row");
    let valid_facts = BranchStateFacts::new(
        branch,
        0,
        1,
        0,
        0,
        Some(CommitVersion::new(3)),
        Some(Timestamp::from_micros(30)),
        Some(Timestamp::from_micros(30)),
    )
    .expect("valid frozen facts");
    let frozen = frozen_source.freeze();
    BranchReadView::new(
        branch,
        MutableTable::new(),
        vec![frozen.clone()],
        Vec::new(),
        valid_facts,
    )
    .expect("valid frozen read view");

    let stale_count = BranchStateFacts::new(
        branch,
        0,
        2,
        0,
        0,
        Some(CommitVersion::new(3)),
        Some(Timestamp::from_micros(30)),
        Some(Timestamp::from_micros(30)),
    )
    .expect("stale frozen count facts");
    assert!(matches!(
        BranchReadView::new(
            branch,
            MutableTable::new(),
            vec![frozen.clone()],
            Vec::new(),
            stale_count
        ),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));

    let stale_timestamps = BranchStateFacts::new(
        branch,
        0,
        1,
        0,
        0,
        Some(CommitVersion::new(3)),
        Some(Timestamp::from_micros(29)),
        Some(Timestamp::from_micros(30)),
    )
    .expect("stale timestamp facts");
    assert!(matches!(
        BranchReadView::new(
            branch,
            MutableTable::new(),
            vec![frozen.clone()],
            Vec::new(),
            stale_timestamps,
        ),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));

    let wrong_row = storage_row_with(
        other,
        b"frozen-constructor".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"secret-payload".to_vec(),
    );
    let mut wrong_frozen = MutableTable::new();
    wrong_frozen
        .insert_row(wrong_row)
        .expect("insert wrong frozen row");
    let wrong_error = BranchReadView::new(
        branch,
        MutableTable::new(),
        vec![wrong_frozen.freeze()],
        Vec::new(),
        valid_facts,
    )
    .expect_err("wrong frozen row rejected");
    assert!(matches!(
        wrong_error,
        BranchRuntimeError::InvalidBranchState { .. }
    ));
    assert!(!wrong_error.to_string().contains("secret-payload"));
}

#[test]
fn branch_read_view_empty_and_single_row_cases_are_stable() {
    let branch = branch_id(45);
    let empty_state = BranchLocalState::empty(branch);
    let empty_view = empty_state.capture_read_view().expect("empty view");
    let key = physical_key(branch, b"single".to_vec());
    assert_eq!(empty_view.latest(&key).expect("empty latest"), None);
    assert!(empty_view
        .history(&key, BranchHistoryOptions::all())
        .expect("empty history")
        .is_empty());
    let empty_prefix = BranchScanBounds::prefix(&physical_key(branch, Vec::new()));
    assert!(empty_view
        .scan_prefix(&empty_prefix, BranchReadBound::latest())
        .expect("empty prefix")
        .is_empty());
    let empty_range = BranchScanBounds::unbounded(
        branch,
        "default",
        StorageSpaceId::engine(0x20).expect("engine space"),
    )
    .expect("empty range");
    assert!(empty_view
        .scan_range(&empty_range, BranchReadBound::latest())
        .expect("empty range")
        .is_empty());

    let mut single_state = BranchLocalState::empty(branch);
    let expired_looking = storage_row_with(
        branch,
        b"single".to_vec(),
        1,
        10,
        Timestamp::from_micros(5),
        Vec::new(),
    );
    single_state
        .append_committed_row(expired_looking.clone())
        .expect("append expired-looking row");
    let single_view = single_state.capture_read_view().expect("single view");
    let latest = single_view
        .latest(&key)
        .expect("latest")
        .expect("single row");
    assert_eq!(latest.row(), &expired_looking);
    assert_eq!(latest.source(), BranchRowSource::Active);
    assert_eq!(latest.row().value(), b"");
    assert_eq!(latest.row().expires_at(), Timestamp::from_micros(5));
    assert_eq!(
        single_view
            .at_version(&key, CommitVersion::ZERO)
            .expect("below single row"),
        None
    );
    assert_eq!(
        single_view
            .at_version(&key, CommitVersion::MAX)
            .expect("max bound")
            .expect("max row")
            .row(),
        &expired_looking
    );
}

#[test]
fn branch_read_view_frozen_limit_skip_does_not_mutate_captured_view() {
    let branch = branch_id(45);
    let config = BranchRuntimeConfig::new(7, 64, 1).expect("config");
    let mut limited_state = BranchLocalState::new(branch, config).expect("limited state");
    let frozen = storage_row_with(
        branch,
        b"limited".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"frozen".to_vec(),
    );
    let active = storage_row_with(
        branch,
        b"limited".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"active".to_vec(),
    );
    limited_state
        .append_committed_row(frozen.clone())
        .expect("append frozen row");
    assert!(matches!(
        limited_state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    let pinned = limited_state.capture_read_view().expect("pinned view");
    let pinned_facts = pinned.facts();
    limited_state
        .append_committed_row(active.clone())
        .expect("append active row");
    assert_eq!(
        limited_state.rotate_active(),
        BranchRotationOutcome::Skipped {
            reason: BranchRotationSkipReason::FrozenLimitReached,
        }
    );
    let limited_key = physical_key(branch, b"limited".to_vec());
    assert_eq!(
        pinned
            .latest(&limited_key)
            .expect("pinned latest")
            .expect("pinned row")
            .row(),
        &frozen
    );
    assert_eq!(pinned.facts(), pinned_facts);
    assert_eq!(
        limited_state
            .capture_read_view()
            .expect("after skip view")
            .latest(&limited_key)
            .expect("after skip latest")
            .expect("active row")
            .row(),
        &active
    );
}

#[test]
fn branch_read_view_latest_and_version_reads_follow_row_chain_not_source_order() {
    let branch = branch_id(39);
    let mut state = BranchLocalState::empty(branch);
    let key = physical_key(branch, b"versioned".to_vec());
    let frozen_newer = storage_row_with(
        branch,
        b"versioned".to_vec(),
        10,
        100,
        Timestamp::EPOCH,
        b"frozen-newer".to_vec(),
    );
    let active_older = storage_row_with(
        branch,
        b"versioned".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        b"active-older".to_vec(),
    );
    let hidden_by_tombstone = storage_row_with(
        branch,
        b"hidden".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"hidden".to_vec(),
    );
    let tombstone = tombstone_row(branch, b"hidden".to_vec(), 5, 50);

    state
        .append_committed_row(frozen_newer.clone())
        .expect("append frozen newer");
    state
        .append_committed_row(hidden_by_tombstone)
        .expect("append hidden");
    state
        .append_committed_row(tombstone)
        .expect("append tombstone");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .append_committed_row(active_older.clone())
        .expect("append active older");

    let view = state.capture_read_view().expect("view");
    let latest = view.latest(&key).expect("latest").expect("latest row");
    assert_eq!(latest.row(), &frozen_newer);
    assert_eq!(latest.source(), BranchRowSource::Frozen { index: 0 });

    let at_seven = view
        .at_version(&key, CommitVersion::new(7))
        .expect("at version")
        .expect("older row");
    assert_eq!(at_seven.row(), &active_older);
    assert_eq!(at_seven.source(), BranchRowSource::Active);
    assert_eq!(
        view.at_version(&key, CommitVersion::new(6))
            .expect("below all"),
        None
    );
    assert_eq!(
        view.latest(&physical_key(branch, b"hidden".to_vec()))
            .expect("tombstone shadows"),
        None
    );
}

#[test]
fn branch_read_view_version_bounds_respect_tombstone_edges_and_extremes() {
    let branch = branch_id(46);
    let mut state = BranchLocalState::empty(branch);
    let live_before_tombstone = storage_row_with(
        branch,
        b"deleted".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"live".to_vec(),
    );
    let deleting_tombstone = tombstone_row(branch, b"deleted".to_vec(), 3, 30);
    let zero_row = storage_row_with(
        branch,
        b"zero".to_vec(),
        0,
        0,
        Timestamp::EPOCH,
        b"zero".to_vec(),
    );
    let max_row = storage_row_with(
        branch,
        b"max".to_vec(),
        u64::MAX,
        u64::MAX,
        Timestamp::EPOCH,
        b"max".to_vec(),
    );
    for row in [
        live_before_tombstone.clone(),
        deleting_tombstone,
        zero_row.clone(),
        max_row.clone(),
    ] {
        state.append_committed_row(row).expect("append version row");
    }
    let view = state.capture_read_view().expect("view");
    let deleted_key = physical_key(branch, b"deleted".to_vec());
    assert_eq!(view.latest(&deleted_key).expect("latest deleted"), None);
    assert_eq!(
        view.at_version(&deleted_key, CommitVersion::new(2))
            .expect("before tombstone")
            .expect("live row")
            .row(),
        &live_before_tombstone
    );
    assert_eq!(
        view.at_version(&deleted_key, CommitVersion::new(3))
            .expect("at tombstone"),
        None
    );
    assert_eq!(
        view.at_version(&physical_key(branch, b"zero".to_vec()), CommitVersion::ZERO)
            .expect("zero bound")
            .expect("zero row")
            .row(),
        &zero_row
    );
    assert_eq!(
        view.at_version(&physical_key(branch, b"max".to_vec()), CommitVersion::MAX)
            .expect("max bound")
            .expect("max row")
            .row(),
        &max_row
    );
}

#[test]
fn branch_read_view_timestamp_reads_filter_by_timestamp_then_commit_version() {
    let branch = branch_id(57);
    let mut state = BranchLocalState::empty(branch);
    let key = physical_key(branch, b"as-of".to_vec());
    let older = storage_row_with(
        branch,
        b"as-of".to_vec(),
        7,
        80,
        Timestamp::EPOCH,
        b"older".to_vec(),
    );
    let highest_version = storage_row_with(
        branch,
        b"as-of".to_vec(),
        10,
        100,
        Timestamp::EPOCH,
        b"highest-version".to_vec(),
    );
    let lower_version_later_timestamp = storage_row_with(
        branch,
        b"as-of".to_vec(),
        8,
        120,
        Timestamp::EPOCH,
        b"later-timestamp".to_vec(),
    );
    for row in [
        older.clone(),
        highest_version.clone(),
        lower_version_later_timestamp,
    ] {
        state
            .append_committed_row(row)
            .expect("append timestamp row");
    }

    let view = state.capture_read_view().expect("view");
    assert_eq!(
        view.read_point(
            &key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(79))
        )
        .expect("before all"),
        None
    );
    assert_eq!(
        view.read_point(
            &key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(80))
        )
        .expect("at older")
        .expect("older row")
        .row(),
        &older
    );
    assert_eq!(
        view.read_point(
            &key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(119))
        )
        .expect("before lower version later timestamp")
        .expect("highest version row")
        .row(),
        &highest_version
    );
    assert_eq!(
        view.read_point(
            &key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(130))
        )
        .expect("after all")
        .expect("highest version row")
        .row(),
        &highest_version,
        "timestamp bounds filter eligibility, then row chains still select newest commit version",
    );
}

#[test]
fn branch_read_view_timestamp_reads_cover_frozen_and_owned_sources() {
    let branch = branch_id(57);
    let mut state = BranchLocalState::empty(branch);
    let frozen_key = physical_key(branch, b"as-of-frozen".to_vec());
    let frozen_visible = storage_row_with(
        branch,
        b"as-of-frozen".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"frozen-visible".to_vec(),
    );
    let frozen_future = storage_row_with(
        branch,
        b"as-of-frozen".to_vec(),
        3,
        50,
        Timestamp::EPOCH,
        b"frozen-future".to_vec(),
    );
    state
        .append_committed_row(frozen_visible.clone())
        .expect("append frozen visible");
    state
        .append_committed_row(frozen_future)
        .expect("append frozen future");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));

    let owned_key = physical_key(branch, b"as-of-owned".to_vec());
    let owned_visible = storage_row_with(
        branch,
        b"as-of-owned".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"owned-visible".to_vec(),
    );
    let owned_future = storage_row_with(
        branch,
        b"as-of-owned".to_vec(),
        6,
        80,
        Timestamp::EPOCH,
        b"owned-future".to_vec(),
    );
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "timestamp-owned-source",
            vec![owned_visible.clone(), owned_future],
        ))
        .expect("install owned timestamp table");

    let view = state.capture_read_view().expect("view");
    assert_visible_row(
        view.read_point(
            &frozen_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(30)),
        )
        .expect("frozen timestamp read")
        .as_ref(),
        &frozen_visible,
        BranchRowSource::Frozen { index: 0 },
    );
    assert_eq!(
        view.read_point(
            &owned_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(39)),
        )
        .expect("before owned timestamp"),
        None
    );
    assert_visible_row(
        view.read_point(
            &owned_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .expect("owned timestamp read")
        .as_ref(),
        &owned_visible,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
}

#[test]
fn branch_read_view_timestamp_tombstones_suppress_fallthrough() {
    let branch = branch_id(58);
    let mut state = BranchLocalState::empty(branch);
    let deleted_key = physical_key(branch, b"deleted-at-time".to_vec());
    let deleted_put = storage_row_with(
        branch,
        b"deleted-at-time".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"deleted-put".to_vec(),
    );
    let deleted_tombstone = tombstone_row(branch, b"deleted-at-time".to_vec(), 3, 30);
    for row in [deleted_put.clone(), deleted_tombstone] {
        state
            .append_committed_row(row)
            .expect("append visibility row");
    }
    let view = state.capture_read_view().expect("view");

    assert_eq!(
        view.read_point(
            &deleted_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(29)),
        )
        .expect("before tombstone")
        .expect("deleted put")
        .row(),
        &deleted_put
    );
    assert_eq!(
        view.read_point(
            &deleted_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(30)),
        )
        .expect("at tombstone"),
        None,
        "tombstone exactly at read timestamp shadows older puts",
    );
}

#[test]
fn branch_read_view_timestamp_ttl_boundaries_suppress_fallthrough() {
    let branch = branch_id(58);
    let mut state = BranchLocalState::empty(branch);
    let ttl_key = physical_key(branch, b"ttl".to_vec());
    let never_expires_key = physical_key(branch, b"ttl-epoch".to_vec());
    let ttl_old = storage_row_with(
        branch,
        b"ttl".to_vec(),
        1,
        5,
        Timestamp::EPOCH,
        b"old".to_vec(),
    );
    let ttl_new = storage_row_with(
        branch,
        b"ttl".to_vec(),
        2,
        10,
        Timestamp::from_micros(20),
        b"new".to_vec(),
    );
    let never_expires = storage_row_with(
        branch,
        b"ttl-epoch".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        Vec::new(),
    );
    for row in [ttl_old, ttl_new.clone(), never_expires.clone()] {
        state.append_committed_row(row).expect("append ttl row");
    }
    let view = state.capture_read_view().expect("view");

    assert_eq!(
        view.read_point(
            &ttl_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(19)),
        )
        .expect("before expiry")
        .expect("ttl row")
        .row(),
        &ttl_new
    );
    assert_eq!(
        view.read_point(
            &ttl_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(20)),
        )
        .expect("exact expiry"),
        None,
        "selected expired rows suppress the key instead of falling through",
    );
    assert_eq!(
        view.read_point(
            &ttl_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(21)),
        )
        .expect("after expiry"),
        None
    );
    assert_eq!(
        view.latest(&ttl_key)
            .expect("latest ignores wall clock")
            .expect("latest ttl row")
            .row(),
        &ttl_new,
        "latest reads do not invent a wall-clock timestamp",
    );
    assert_eq!(
        view.read_point(
            &never_expires_key,
            BranchReadBound::at_timestamp(Timestamp::MAX),
        )
        .expect("max timestamp")
        .expect("epoch-expiry row")
        .row(),
        &never_expires
    );
}

#[test]
fn branch_read_view_timestamp_max_expiry_is_far_future_not_no_expiry() {
    let branch = branch_id(58);
    let mut state = BranchLocalState::empty(branch);
    let max_expiry_key = physical_key(branch, b"ttl-max".to_vec());
    let max_expiry = storage_row_with(
        branch,
        b"ttl-max".to_vec(),
        1,
        10,
        Timestamp::MAX,
        b"far-future".to_vec(),
    );
    state
        .append_committed_row(max_expiry.clone())
        .expect("append max-expiry row");
    let view = state.capture_read_view().expect("view");

    assert_eq!(
        view.read_point(
            &max_expiry_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(u64::MAX - 1)),
        )
        .expect("before max expiry")
        .expect("visible before max expiry")
        .row(),
        &max_expiry
    );
    assert_eq!(
        view.read_point(
            &max_expiry_key,
            BranchReadBound::at_timestamp(Timestamp::MAX),
        )
        .expect("at max expiry"),
        None,
        "Timestamp::MAX expiry is an actual far-future expiry, not the no-expiry sentinel",
    );
}

#[test]
fn branch_read_view_timestamp_scans_apply_tombstone_and_ttl_per_key() {
    let branch = branch_id(59);
    let mut state = BranchLocalState::empty(branch);
    let visible = storage_row_with(
        branch,
        b"ts-scan-a".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"a".to_vec(),
    );
    let future = storage_row_with(
        branch,
        b"ts-scan-b".to_vec(),
        2,
        50,
        Timestamp::EPOCH,
        b"future".to_vec(),
    );
    let expired_old = storage_row_with(
        branch,
        b"ts-scan-c".to_vec(),
        1,
        5,
        Timestamp::EPOCH,
        b"old".to_vec(),
    );
    let expired_new = storage_row_with(
        branch,
        b"ts-scan-c".to_vec(),
        3,
        30,
        Timestamp::from_micros(35),
        b"expired".to_vec(),
    );
    let deleted_old = storage_row_with(
        branch,
        b"ts-scan-d".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"deleted".to_vec(),
    );
    let deleted_tombstone = tombstone_row(branch, b"ts-scan-d".to_vec(), 4, 40);
    for row in [
        visible.clone(),
        future,
        expired_old,
        expired_new,
        deleted_old,
        deleted_tombstone,
    ] {
        state.append_committed_row(row).expect("append scan row");
    }
    let view = state.capture_read_view().expect("view");
    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"ts-scan-".to_vec()));
    let prefix_rows = view
        .scan_prefix(
            &prefix,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .expect("timestamp prefix scan");
    assert_eq!(scan_user_keys(&prefix_rows), vec![b"ts-scan-a".to_vec()]);
    assert_eq!(prefix_rows[0].row(), &visible);

    let range = BranchScanBounds::closed(
        &physical_key(branch, b"ts-scan-a".to_vec()),
        &physical_key(branch, b"ts-scan-d".to_vec()),
    )
    .expect("closed range");
    let range_rows = view
        .scan_range(
            &range,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .expect("timestamp range scan");
    assert_eq!(scan_user_keys(&range_rows), vec![b"ts-scan-a".to_vec()]);
}

#[test]
fn branch_read_view_timestamp_scans_preserve_bounds_and_empty_results() {
    let branch = branch_id(59);
    let mut state = BranchLocalState::empty(branch);
    let scan_a = storage_row_with(
        branch,
        b"ts-bound-a".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"a".to_vec(),
    );
    let scan_b = storage_row_with(
        branch,
        b"ts-bound-b".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"b".to_vec(),
    );
    let scan_c_future = storage_row_with(
        branch,
        b"ts-bound-c".to_vec(),
        3,
        50,
        Timestamp::EPOCH,
        b"c".to_vec(),
    );
    for row in [scan_a.clone(), scan_b.clone(), scan_c_future] {
        state.append_committed_row(row).expect("append scan row");
    }
    let view = state.capture_read_view().expect("view");
    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"ts-bound-".to_vec()));

    assert!(
        view.scan_prefix(
            &prefix,
            BranchReadBound::at_timestamp(Timestamp::from_micros(9)),
        )
        .expect("before all timestamp scan")
        .is_empty(),
        "timestamp scan with no eligible rows should return an empty result",
    );
    let closed = BranchScanBounds::closed(
        &physical_key(branch, b"ts-bound-a".to_vec()),
        &physical_key(branch, b"ts-bound-c".to_vec()),
    )
    .expect("closed bounds");
    let closed_rows = view
        .scan_range(
            &closed,
            BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
        )
        .expect("closed timestamp range");
    assert_eq!(
        scan_user_keys(&closed_rows),
        vec![b"ts-bound-a".to_vec(), b"ts-bound-b".to_vec()],
        "timestamp scan remains sorted and preserves inclusive range edges",
    );
    let open = BranchScanBounds::open(
        &physical_key(branch, b"ts-bound-a".to_vec()),
        &physical_key(branch, b"ts-bound-c".to_vec()),
    )
    .expect("open bounds");
    let open_rows = view
        .scan_range(
            &open,
            BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
        )
        .expect("open timestamp range");
    assert_eq!(scan_user_keys(&open_rows), vec![b"ts-bound-b".to_vec()]);
}

#[test]
fn branch_read_view_timestamp_scans_preserve_key_spaces() {
    let branch = branch_id(59);
    let mut state = BranchLocalState::empty(branch);
    let engine_space = StorageSpaceId::engine(0x20).expect("engine space");
    let other_space = StorageSpaceId::engine(0x21).expect("other space");
    let default_row = storage_row_with(
        branch,
        b"ts-space-a".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"default".to_vec(),
    );
    let system_row = storage_row_with_named_space(
        branch,
        "system",
        engine_space,
        b"ts-space-a".to_vec(),
        2,
        10,
        b"system".to_vec(),
    );
    let other_storage_space_row = StorageRow::put(
        physical_key_with(branch, "default", other_space, b"ts-space-a".to_vec()),
        CommitVersion::new(3),
        Timestamp::from_micros(10),
        Timestamp::EPOCH,
        b"other-space".to_vec(),
    );
    for row in [
        default_row.clone(),
        system_row.clone(),
        other_storage_space_row.clone(),
    ] {
        state.append_committed_row(row).expect("append scan row");
    }
    let view = state.capture_read_view().expect("view");
    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"ts-space-".to_vec()));

    let default_rows = view
        .scan_prefix(
            &prefix,
            BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
        )
        .expect("default prefix timestamp scan");
    assert_eq!(
        scan_user_keys(&default_rows),
        vec![b"ts-space-a".to_vec()],
        "default-space scan must not leak named-space or storage-space rows",
    );
    assert_eq!(default_rows[0].row(), &default_row);
    let system_rows = view
        .scan_prefix(
            &BranchScanBounds::prefix(&physical_key_with(
                branch,
                "system",
                engine_space,
                b"ts-space-".to_vec(),
            )),
            BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
        )
        .expect("system prefix timestamp scan");
    assert_eq!(system_rows.len(), 1);
    assert_eq!(system_rows[0].row(), &system_row);
    assert_eq!(
        view.scan_prefix(
            &BranchScanBounds::prefix(&physical_key_with(
                branch,
                "default",
                other_space,
                b"ts-space-".to_vec(),
            )),
            BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
        )
        .expect("other storage-space timestamp scan")
        .first()
        .expect("other storage-space row")
        .row(),
        &other_storage_space_row,
    );
}

#[test]
fn branch_inherited_timestamp_scans_rewrite_source_keys_before_grouping() {
    let source = branch_id(63);
    let child = branch_id(64);
    let source_visible = storage_row_with(
        source,
        b"ts-inherited-scan-a".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"visible".to_vec(),
    );
    let source_future = storage_row_with(
        source,
        b"ts-inherited-scan-b".to_vec(),
        4,
        70,
        Timestamp::EPOCH,
        b"future".to_vec(),
    );
    let source_after_fork = storage_row_with(
        source,
        b"ts-inherited-scan-c".to_vec(),
        7,
        20,
        Timestamp::EPOCH,
        b"post-fork".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "timestamp-inherited-scan",
            vec![source_visible.clone(), source_future, source_after_fork],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let view = child_state.capture_read_view().expect("view");
    let expected = rewrite_row_branch(&source_visible, source, child).expect("rewrite expected");
    let prefix = BranchScanBounds::prefix(&physical_key(child, b"ts-inherited-scan-".to_vec()));
    let prefix_rows = view
        .scan_prefix(
            &prefix,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .expect("timestamp inherited prefix scan");
    assert_eq!(
        scan_user_keys(&prefix_rows),
        vec![b"ts-inherited-scan-a".to_vec()]
    );
    assert_visible_row(
        prefix_rows.first(),
        &expected,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );

    let range = BranchScanBounds::closed(
        &physical_key(child, b"ts-inherited-scan-a".to_vec()),
        &physical_key(child, b"ts-inherited-scan-c".to_vec()),
    )
    .expect("closed inherited range");
    let range_rows = view
        .scan_range(
            &range,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .expect("timestamp inherited range scan");
    assert_eq!(range_rows.len(), 1);
    assert_eq!(range_rows[0].row(), &expected);
}

#[test]
fn branch_read_view_timestamp_views_are_pinned_across_later_mutations() {
    let branch = branch_id(65);
    let mut state = BranchLocalState::empty(branch);
    let point = storage_row_with(
        branch,
        b"pinned-ts".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"point".to_vec(),
    );
    let scan = storage_row_with(
        branch,
        b"pinned-scan-a".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"scan".to_vec(),
    );
    for row in [point.clone(), scan.clone()] {
        state.append_committed_row(row).expect("append pinned row");
    }
    let pinned = state.capture_read_view().expect("pinned view");

    state
        .append_committed_row(storage_row_with(
            branch,
            b"pinned-ts".to_vec(),
            3,
            30,
            Timestamp::EPOCH,
            b"later-point".to_vec(),
        ))
        .expect("append later point");
    state
        .append_committed_row(storage_row_with(
            branch,
            b"pinned-scan-b".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            b"later-scan".to_vec(),
        ))
        .expect("append later scan");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "pinned-timestamp-owned",
            vec![storage_row_with(
                branch,
                b"pinned-scan-c".to_vec(),
                5,
                50,
                Timestamp::EPOCH,
                b"owned".to_vec(),
            )],
        ))
        .expect("install later owned table");

    let point_key = physical_key(branch, b"pinned-ts".to_vec());
    assert_visible_row(
        pinned
            .read_point(
                &point_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(35)),
            )
            .expect("pinned point read")
            .as_ref(),
        &point,
        BranchRowSource::Active,
    );
    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"pinned-scan-".to_vec()));
    let pinned_scan = pinned
        .scan_prefix(
            &prefix,
            BranchReadBound::at_timestamp(Timestamp::from_micros(60)),
        )
        .expect("pinned timestamp scan");
    assert_eq!(
        scan_user_keys(&pinned_scan),
        vec![b"pinned-scan-a".to_vec()]
    );
    assert_eq!(pinned_scan[0].row(), &scan);

    let current = state.capture_read_view().expect("current view");
    assert_eq!(
        current
            .scan_prefix(
                &prefix,
                BranchReadBound::at_timestamp(Timestamp::from_micros(60)),
            )
            .expect("current timestamp scan")
            .len(),
        3
    );
}

#[test]
fn branch_timestamp_coverage_rejects_only_proven_insufficient_history() {
    let branch = branch_id(60);
    let mut state = BranchLocalState::empty(branch);
    let row = storage_row_with(
        branch,
        b"coverage".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"coverage".to_vec(),
    );
    state.append_committed_row(row.clone()).expect("append row");
    let key = physical_key(branch, b"coverage".to_vec());
    let unknown = state.capture_read_view().expect("unknown coverage");
    assert_eq!(
        unknown.timestamp_coverage(),
        BranchTimestampCoverage::unknown()
    );
    assert_eq!(
        unknown
            .read_point(
                &key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(49))
            )
            .expect("unknown coverage permits best effort"),
        None,
        "observed timestamp_min alone is not an insufficient-history proof",
    );

    let complete_since = state
        .capture_read_view()
        .expect("coverage view")
        .with_timestamp_coverage(BranchTimestampCoverage::complete_since(
            Timestamp::from_micros(50),
        ));
    let error = complete_since
        .read_point(
            &key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(49)),
        )
        .expect_err("known insufficient history");
    assert_eq!(
        error,
        BranchRuntimeError::InsufficientTimestampHistory {
            branch_id: branch,
            requested_timestamp: Timestamp::from_micros(49),
            earliest_available_timestamp: Some(Timestamp::from_micros(50)),
            source: BranchTimestampHistorySource::Combined,
        }
    );
    assert!(!error.to_string().contains("coverage"));
    assert_eq!(
        complete_since
            .read_point(
                &key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(50))
            )
            .expect("at coverage floor")
            .expect("row")
            .row(),
        &row
    );
    assert_eq!(
        state
            .capture_read_view()
            .expect("complete coverage")
            .with_timestamp_coverage(BranchTimestampCoverage::complete())
            .read_point(&key, BranchReadBound::at_timestamp(Timestamp::EPOCH))
            .expect("complete coverage permits timestamp read"),
        None
    );
}

#[test]
fn branch_read_view_multiple_frozen_tables_preserve_source_facts() {
    let branch = branch_id(47);
    let mut state = BranchLocalState::empty(branch);
    let old_frozen = storage_row_with(
        branch,
        b"multi-frozen".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"old".to_vec(),
    );
    let new_frozen = storage_row_with(
        branch,
        b"multi-frozen".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"new".to_vec(),
    );
    let active_middle = storage_row_with(
        branch,
        b"multi-frozen".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"active".to_vec(),
    );

    state
        .append_committed_row(old_frozen.clone())
        .expect("append old frozen");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .append_committed_row(new_frozen.clone())
        .expect("append new frozen");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .append_committed_row(active_middle.clone())
        .expect("append active middle");

    let view = state.capture_read_view().expect("view");
    assert_eq!(view.frozen_table_count(), 2);
    let key = physical_key(branch, b"multi-frozen".to_vec());
    let latest = view.latest(&key).expect("latest").expect("new frozen");
    assert_eq!(latest.row(), &new_frozen);
    assert_eq!(latest.source(), BranchRowSource::Frozen { index: 0 });
    let at_two = view
        .at_version(&key, CommitVersion::new(2))
        .expect("at two")
        .expect("active middle");
    assert_eq!(at_two.row(), &active_middle);
    assert_eq!(at_two.source(), BranchRowSource::Active);
    let at_one = view
        .at_version(&key, CommitVersion::new(1))
        .expect("at one")
        .expect("old frozen");
    assert_eq!(at_one.row(), &old_frozen);
    assert_eq!(at_one.source(), BranchRowSource::Frozen { index: 1 });
}

#[test]
fn branch_read_view_history_preserves_tombstones_limits_and_before_version() {
    let branch = branch_id(40);
    let mut state = BranchLocalState::empty(branch);
    let key = physical_key(branch, b"history".to_vec());
    let rows = [
        storage_row_with(
            branch,
            b"history".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"one".to_vec(),
        ),
        tombstone_row(branch, b"history".to_vec(), 2, 20),
        storage_row_with(
            branch,
            b"history".to_vec(),
            3,
            30,
            Timestamp::from_micros(25),
            Vec::new(),
        ),
    ];

    for row in rows {
        state.append_committed_row(row).expect("append history row");
    }
    let view = state.capture_read_view().expect("view");

    let all = view
        .history(&key, BranchHistoryOptions::all())
        .expect("all history");
    assert_eq!(history_versions(&all), vec![3, 2, 1]);
    assert!(all.iter().any(|row| row.row().is_tombstone()));
    assert_eq!(all[0].row().value(), b"");
    assert_eq!(all[0].row().expires_at(), Timestamp::from_micros(25));

    let before_three = view
        .history(
            &key,
            BranchHistoryOptions::all().before_version(CommitVersion::new(3)),
        )
        .expect("before");
    assert_eq!(history_versions(&before_three), vec![2, 1]);

    let one = view
        .history(&key, BranchHistoryOptions::all().limit(1))
        .expect("limited");
    assert_eq!(history_versions(&one), vec![3]);

    let zero = view
        .history(&key, BranchHistoryOptions::all().limit(0))
        .expect("zero limit");
    assert!(zero.is_empty());

    let without_tombstones = view
        .history(&key, BranchHistoryOptions::all().include_tombstones(false))
        .expect("without tombstones");
    assert_eq!(history_versions(&without_tombstones), vec![3, 1]);

    let before_zero = view
        .history(
            &key,
            BranchHistoryOptions::all().before_version(CommitVersion::ZERO),
        )
        .expect("before zero");
    assert!(before_zero.is_empty());
}

#[test]
fn branch_read_view_prefix_and_range_scans_group_by_physical_key() {
    let branch = branch_id(41);
    let view = branch_read_view_with_scan_rows(branch);
    assert_eq!(view.branch_id(), branch);

    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"ap".to_vec()));
    let prefix_rows = view
        .scan_prefix(&prefix, BranchReadBound::latest())
        .expect("prefix scan");
    assert_eq!(
        scan_user_keys(&prefix_rows),
        vec![b"apple".to_vec(), b"apricot".to_vec()]
    );
    assert_eq!(prefix_rows[0].row().value(), b"new-apple");

    let closed = BranchScanBounds::closed(
        &physical_key(branch, b"apple".to_vec()),
        &physical_key(branch, b"banana".to_vec()),
    )
    .expect("closed range");
    let range_rows = view
        .scan_range(&closed, BranchReadBound::at_version(CommitVersion::new(4)))
        .expect("range scan");
    assert_eq!(
        scan_user_keys(&range_rows),
        vec![b"apple".to_vec(), b"apricot".to_vec(), b"banana".to_vec()]
    );

    let open = BranchScanBounds::open(
        &physical_key(branch, b"apple".to_vec()),
        &physical_key(branch, b"banana".to_vec()),
    )
    .expect("open range");
    let open_rows = view
        .scan_range(&open, BranchReadBound::latest())
        .expect("open range scan");
    assert_eq!(scan_user_keys(&open_rows), vec![b"apricot".to_vec()]);

    let bounded = BranchScanBounds::range(
        branch,
        "default",
        StorageSpaceId::engine(0x20).expect("engine space"),
        BranchUserKeyBound::included(b"apple".to_vec()),
        BranchUserKeyBound::excluded(b"banana".to_vec()),
    )
    .expect("manual range");
    let bounded_rows = view
        .scan_range(&bounded, BranchReadBound::latest())
        .expect("manual range scan");
    assert_eq!(
        scan_user_keys(&bounded_rows),
        vec![b"apple".to_vec(), b"apricot".to_vec()]
    );

    let unbounded = BranchScanBounds::unbounded(
        branch,
        "default",
        StorageSpaceId::engine(0x20).expect("engine space"),
    )
    .expect("unbounded scan");
    let unbounded_rows = view
        .scan_range(
            &unbounded,
            BranchReadBound::at_version(CommitVersion::new(6)),
        )
        .expect("unbounded scan");
    assert_eq!(
        scan_user_keys(&unbounded_rows),
        vec![
            b"apple".to_vec(),
            b"apricot".to_vec(),
            b"banana".to_vec(),
            vec![0x80, 0x00, 0xff],
        ]
    );
}

#[test]
fn branch_read_view_scans_cover_empty_prefix_zero_bytes_and_degenerate_ranges() {
    let branch = branch_id(48);
    let mut state = BranchLocalState::empty(branch);
    let empty_key_row = storage_row_with(branch, Vec::new(), 1, 10, Timestamp::EPOCH, Vec::new());
    let nul_row = storage_row_with(
        branch,
        b"nul\0a".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"nul-a".to_vec(),
    );
    let nul_tombstone = tombstone_row(branch, b"nul\0b".to_vec(), 3, 30);
    let other_storage_space = StorageRow::put(
        physical_key_with(
            branch,
            "default",
            StorageSpaceId::engine(0x21).expect("engine space"),
            b"nul\0a".to_vec(),
        ),
        CommitVersion::new(4),
        Timestamp::from_micros(40),
        Timestamp::EPOCH,
        b"other-storage-space".to_vec(),
    );
    for row in [
        empty_key_row.clone(),
        nul_row.clone(),
        nul_tombstone,
        other_storage_space,
    ] {
        state
            .append_committed_row(row)
            .expect("append scan edge row");
    }
    let view = state.capture_read_view().expect("view");

    let empty_prefix = BranchScanBounds::prefix(&physical_key(branch, Vec::new()));
    assert_eq!(
        scan_user_keys(
            &view
                .scan_prefix(&empty_prefix, BranchReadBound::latest())
                .expect("empty prefix scan")
        ),
        vec![Vec::new(), b"nul\0a".to_vec()]
    );

    let nul_prefix = BranchScanBounds::prefix(&physical_key(branch, b"nul\0".to_vec()));
    let nul_rows = view
        .scan_prefix(&nul_prefix, BranchReadBound::latest())
        .expect("nul prefix scan");
    assert_eq!(scan_user_keys(&nul_rows), vec![b"nul\0a".to_vec()]);
    assert_eq!(nul_rows[0].row(), &nul_row);

    let lower = physical_key(branch, b"nul\0a".to_vec());
    let open_degenerate = BranchScanBounds::open(&lower, &lower).expect("open degenerate");
    assert!(view
        .scan_range(&open_degenerate, BranchReadBound::latest())
        .expect("open degenerate scan")
        .is_empty());
    let closed_degenerate = BranchScanBounds::closed(&lower, &lower).expect("closed degenerate");
    let closed_degenerate_rows = view
        .scan_range(&closed_degenerate, BranchReadBound::latest())
        .expect("closed degenerate scan");
    assert_eq!(closed_degenerate_rows.len(), 1);
    assert_eq!(closed_degenerate_rows[0].row(), &nul_row);
}

fn branch_read_view_with_scan_rows(branch: BranchId) -> BranchReadView {
    let mut state = BranchLocalState::empty(branch);
    let rows = [
        storage_row_with(
            branch,
            b"apple".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"old-apple".to_vec(),
        ),
        storage_row_with(
            branch,
            b"apple".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"new-apple".to_vec(),
        ),
        storage_row_with(
            branch,
            b"apricot".to_vec(),
            3,
            30,
            Timestamp::EPOCH,
            b"apricot".to_vec(),
        ),
        storage_row_with(
            branch,
            b"banana".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            b"banana".to_vec(),
        ),
        tombstone_row(branch, b"apex".to_vec(), 5, 50),
        storage_row_with(
            branch,
            vec![0x80, 0x00, 0xff],
            6,
            60,
            Timestamp::EPOCH,
            b"high".to_vec(),
        ),
        StorageRow::put(
            physical_key_with(
                branch,
                "other-space",
                StorageSpaceId::engine(0x20).expect("engine space"),
                b"apple".to_vec(),
            ),
            CommitVersion::new(7),
            Timestamp::from_micros(70),
            Timestamp::EPOCH,
            b"other-space".to_vec(),
        ),
        StorageRow::put(
            physical_key_with(
                branch,
                "default",
                StorageSpaceId::engine(0x21).expect("engine space"),
                b"apple".to_vec(),
            ),
            CommitVersion::new(8),
            Timestamp::from_micros(80),
            Timestamp::EPOCH,
            b"other-storage-space".to_vec(),
        ),
    ];
    for row in rows {
        state.append_committed_row(row).expect("append scan row");
    }
    state.capture_read_view().expect("view")
}

#[test]
fn branch_read_view_rejects_wrong_branch_before_timestamp_reads_without_payload() {
    let branch = branch_id(42);
    let other = branch_id(43);
    let mut state = BranchLocalState::empty(branch);
    let row = storage_row_with(
        branch,
        b"payload".to_vec(),
        1,
        10,
        Timestamp::from_micros(20),
        b"secret-payload".to_vec(),
    );
    state.append_committed_row(row).expect("append row");
    let view = state.capture_read_view().expect("view");

    let wrong_branch_error = view
        .latest(&physical_key(other, b"payload".to_vec()))
        .expect_err("wrong branch rejected");
    assert!(matches!(
        wrong_branch_error,
        BranchRuntimeError::InvalidBranchRow { .. }
    ));
    assert!(!wrong_branch_error.to_string().contains("secret-payload"));

    let timestamp_row = view
        .read_point(
            &physical_key(branch, b"payload".to_vec()),
            BranchReadBound::at_timestamp(Timestamp::from_micros(19)),
        )
        .expect("timestamp read")
        .expect("timestamp row");
    assert_eq!(timestamp_row.row().value(), b"secret-payload");

    let scan_rows = view
        .scan_prefix(
            &BranchScanBounds::prefix(&physical_key(branch, b"payload".to_vec())),
            BranchReadBound::at_timestamp(Timestamp::from_micros(19)),
        )
        .expect("timestamp scan");
    assert_eq!(scan_user_keys(&scan_rows), vec![b"payload".to_vec()]);

    let wrong_branch_scan = view
        .scan_prefix(
            &BranchScanBounds::prefix(&physical_key(other, b"payload".to_vec())),
            BranchReadBound::latest(),
        )
        .expect_err("wrong branch scan rejected");
    assert!(matches!(
        wrong_branch_scan,
        BranchRuntimeError::InvalidBranchRow { .. }
    ));

    assert!(matches!(
        BranchScanBounds::closed(
            &physical_key(branch, b"z".to_vec()),
            &physical_key(branch, b"a".to_vec()),
        ),
        Err(BranchRuntimeError::InvalidReadBound { .. })
    ));
    assert!(matches!(
        BranchScanBounds::unbounded(
            branch,
            "",
            StorageSpaceId::engine(0x20).expect("engine space"),
        ),
        Err(BranchRuntimeError::InvalidReadBound { .. })
    ));
    assert!(matches!(
        BranchScanBounds::range(
            branch,
            "bad\0space",
            StorageSpaceId::engine(0x20).expect("engine space"),
            BranchUserKeyBound::Unbounded,
            BranchUserKeyBound::Unbounded,
        ),
        Err(BranchRuntimeError::InvalidReadBound { .. })
    ));
}

#[test]
fn branch_owned_table_constructor_rejects_descriptor_and_branch_mismatches() {
    let branch = branch_id(49);
    let row = storage_row_with(
        branch,
        b"owned-constructor".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"secret-payload".to_vec(),
    );
    let reader = immutable_reader("owned-constructor", vec![row]);
    let descriptor = branch_table_descriptor(BranchLevel::ZERO, &reader);
    let owned =
        BranchOwnedTable::new(branch, descriptor.clone(), reader.clone()).expect("owned table");
    assert_eq!(owned.branch_id(), branch);
    assert_eq!(owned.descriptor(), &descriptor);
    assert_eq!(owned.facts(), reader.facts());
    assert_eq!(owned.level(), BranchLevel::ZERO);
    assert_eq!(owned.rows().len(), 1);

    let other_reader = immutable_reader(
        "owned-constructor-other",
        vec![storage_row_with(
            branch,
            b"other".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"other".to_vec(),
        )],
    );
    assert!(matches!(
        BranchOwnedTable::new(branch, descriptor, other_reader),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));

    let wrong_branch_reader = immutable_reader(
        "owned-constructor-wrong",
        vec![storage_row_with(
            branch_id(50),
            b"wrong".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"secret-payload".to_vec(),
        )],
    );
    let wrong_branch_descriptor = branch_table_descriptor(BranchLevel::ZERO, &wrong_branch_reader);
    let error = BranchOwnedTable::new(branch, wrong_branch_descriptor, wrong_branch_reader)
        .expect_err("wrong branch table rejected");
    assert!(matches!(error, BranchRuntimeError::InvalidBranchRow { .. }));
    assert!(!error.to_string().contains("secret-payload"));
}

#[test]
fn branch_owned_table_empty_immutable_input_is_rejected_before_install() {
    let branch = branch_id(68);
    let state = BranchLocalState::empty(branch);
    let builder = ImmutableTableBuilder::new(TableBuilderConfig::default()).expect("builder");
    let rows = Vec::<TableRow>::new();
    let error = builder
        .build_from_rows(
            TableIdentity::new("empty-owned-table").expect("identity"),
            &rows,
        )
        .expect_err("empty immutable table rejected");

    assert!(matches!(
        error,
        crate::table::TableRuntimeError::InvalidRange { field: "row_count" }
    ));
    assert_eq!(state.owned_table_count(), 0);
    assert!(state.is_empty());
}

#[test]
fn branch_local_state_installs_l0_table_and_reads_owned_sources() {
    let branch = branch_id(51);
    let mut state = BranchLocalState::empty(branch);
    let row = storage_row_with(
        branch,
        b"owned-l0".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"owned".to_vec(),
    );
    let table = branch_owned_table(branch, BranchLevel::ZERO, "owned-l0", vec![row.clone()]);

    let outcome: BranchImmutableInstallOutcome = state.install_l0_table(table).expect("install l0");
    assert_eq!(outcome.branch_id(), branch);
    assert_eq!(outcome.level(), BranchLevel::ZERO);
    assert_eq!(outcome.table_index(), 0);
    assert_eq!(outcome.level_table_count(), 1);
    assert_eq!(outcome.owned_table_count(), 1);
    assert_eq!(outcome.replaced_frozen_index(), None);
    assert_eq!(state.owned_table_count(), 1);
    assert_eq!(state.max_commit_version(), Some(CommitVersion::new(5)));
    assert_eq!(state.put_rows(), 1);

    let facts = state.facts().expect("facts");
    assert_eq!(facts.owned_table_count(), 1);
    assert_eq!(facts.max_commit_version(), Some(CommitVersion::new(5)));
    let view = state.capture_read_view().expect("view");
    assert_eq!(view.owned_table_count(), 1);
    assert_eq!(view.owned_levels()[0].len(), 1);
    let visible = view
        .latest(&physical_key(branch, b"owned-l0".to_vec()))
        .expect("latest")
        .expect("owned row");
    assert_eq!(visible.row(), &row);
    assert_eq!(
        visible.source(),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        }
    );
}

#[test]
fn branch_local_state_rejects_owned_table_for_other_branch_without_mutation() {
    let branch = branch_id(56);
    let other = branch_id(57);
    let mut state = BranchLocalState::empty(branch);
    let table = branch_owned_table(
        other,
        BranchLevel::ZERO,
        "owned-other-branch",
        vec![storage_row_with(
            other,
            b"wrong-branch-owned".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"secret-payload".to_vec(),
        )],
    );
    let before = state.clone();

    let error = state
        .install_l0_table(table)
        .expect_err("other branch table rejected");
    assert!(matches!(error, BranchRuntimeError::InvalidBranchRow { .. }));
    assert!(!error.to_string().contains("secret-payload"));
    assert_eq!(state, before);
}

#[test]
fn branch_local_state_replaces_frozen_with_l0_without_mutating_pinned_views() {
    let branch = branch_id(52);
    let mut state = BranchLocalState::empty(branch);
    let row = storage_row_with(
        branch,
        b"flush".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"flush".to_vec(),
    );
    state.append_committed_row(row.clone()).expect("append row");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    let pinned = state.capture_read_view().expect("pinned view");
    let table = branch_owned_table(branch, BranchLevel::ZERO, "flush-l0", vec![row.clone()]);

    let outcome: BranchImmutableInstallOutcome = state
        .replace_frozen_with_l0_table(0, table)
        .expect("replace frozen");
    assert_eq!(outcome.replaced_frozen_index(), Some(0));
    assert_eq!(state.frozen_table_count(), 0);
    assert_eq!(state.owned_table_count(), 1);

    let key = physical_key(branch, b"flush".to_vec());
    let before = pinned.latest(&key).expect("pinned latest").expect("row");
    assert_eq!(before.row(), &row);
    assert_eq!(before.source(), BranchRowSource::Frozen { index: 0 });

    let after_view = state.capture_read_view().expect("after view");
    let after = after_view.latest(&key).expect("after latest").expect("row");
    assert_eq!(after.row(), &row);
    assert_eq!(
        after.source(),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        }
    );
}

#[test]
fn branch_frozen_replacement_rejects_mismatches_without_mutation() {
    let branch = branch_id(55);
    let mut state = BranchLocalState::empty(branch);
    let row = storage_row_with(
        branch,
        b"flush-mismatch".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"frozen".to_vec(),
    );
    state.append_committed_row(row.clone()).expect("append row");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    let before = state.clone();

    let mismatched_value = storage_row_with(
        branch,
        b"flush-mismatch".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"different".to_vec(),
    );
    let mismatch = branch_owned_table(
        branch,
        BranchLevel::ZERO,
        "flush-mismatch",
        vec![mismatched_value],
    );
    assert!(matches!(
        state.replace_frozen_with_l0_table(0, mismatch),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
    assert_eq!(state, before);

    let replacement =
        branch_owned_table(branch, BranchLevel::ZERO, "flush-out-of-range", vec![row]);
    assert!(matches!(
        state.replace_frozen_with_l0_table(1, replacement),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
    assert_eq!(state, before);
}

#[test]
fn branch_read_view_scans_owned_immutable_tables_and_pins_before_l0_install() {
    let branch = branch_id(58);
    let mut state = BranchLocalState::empty(branch);
    let pinned_before = state.capture_read_view().expect("pre-install view");
    let live = storage_row_with(
        branch,
        b"scan-owned-a".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"live".to_vec(),
    );
    let old_deleted = storage_row_with(
        branch,
        b"scan-owned-b".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"old".to_vec(),
    );
    let deleting_tombstone = tombstone_row(branch, b"scan-owned-b".to_vec(), 4, 40);
    let high = storage_row_with(
        branch,
        vec![b's', b'c', b'a', b'n', 0x80],
        2,
        20,
        Timestamp::EPOCH,
        b"high".to_vec(),
    );
    let table = branch_owned_table(
        branch,
        BranchLevel::ZERO,
        "owned-scan",
        vec![live.clone(), old_deleted, deleting_tombstone, high.clone()],
    );
    state.install_l0_table(table).expect("install scan table");

    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"scan-owned".to_vec()));
    assert!(pinned_before
        .scan_prefix(&prefix, BranchReadBound::latest())
        .expect("pinned prefix")
        .is_empty());

    let view = state.capture_read_view().expect("post-install view");
    let prefix_rows = view
        .scan_prefix(&prefix, BranchReadBound::latest())
        .expect("prefix scan");
    assert_eq!(scan_user_keys(&prefix_rows), vec![b"scan-owned-a".to_vec()]);
    assert_eq!(prefix_rows[0].row(), &live);
    assert_eq!(
        prefix_rows[0].source(),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        }
    );

    let range = BranchScanBounds::closed(
        &physical_key(branch, b"scan-owned-a".to_vec()),
        &physical_key(branch, vec![b's', b'c', b'a', b'n', 0x80]),
    )
    .expect("closed owned range");
    let range_rows = view
        .scan_range(&range, BranchReadBound::latest())
        .expect("range scan");
    assert_eq!(
        scan_user_keys(&range_rows),
        vec![b"scan-owned-a".to_vec(), vec![b's', b'c', b'a', b'n', 0x80]]
    );
    assert_eq!(range_rows[1].row(), &high);
}

#[test]
fn branch_owned_l0_tables_accept_overlaps_and_select_by_version_not_index() {
    let branch = branch_id(59);
    let mut state = BranchLocalState::empty(branch);
    let key = physical_key(branch, b"l0-overlap".to_vec());
    let newer = storage_row_with(
        branch,
        b"l0-overlap".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"newer".to_vec(),
    );
    let older = storage_row_with(
        branch,
        b"l0-overlap".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"older".to_vec(),
    );

    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "l0-overlap-newer",
            vec![newer.clone()],
        ))
        .expect("install newer L0");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "l0-overlap-older",
            vec![older.clone()],
        ))
        .expect("install overlapping older L0");
    assert_eq!(state.owned_levels()[0].len(), 2);
    assert_eq!(
        state.owned_levels()[0][0].descriptor().identity().as_str(),
        "l0-overlap-older"
    );

    let view = state.capture_read_view().expect("view");
    assert_visible_row(
        view.latest(&key).expect("latest").as_ref(),
        &newer,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 1,
        },
    );
    assert_visible_row(
        view.at_version(&key, CommitVersion::new(2))
            .expect("bounded")
            .as_ref(),
        &older,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
}

#[test]
fn branch_frozen_replacement_targets_named_frozen_table_and_keeps_l0_front() {
    let branch = branch_id(60);
    let mut state = BranchLocalState::empty(branch);
    let older_frozen = storage_row_with(
        branch,
        b"replace-old".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"old".to_vec(),
    );
    let newer_frozen = storage_row_with(
        branch,
        b"replace-new".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"new".to_vec(),
    );
    state
        .append_committed_row(older_frozen.clone())
        .expect("append older");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .append_committed_row(newer_frozen.clone())
        .expect("append newer");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "preexisting-l0",
            vec![storage_row_with(
                branch,
                b"preexisting".to_vec(),
                3,
                30,
                Timestamp::EPOCH,
                b"preexisting".to_vec(),
            )],
        ))
        .expect("install preexisting L0");
    let pinned = state.capture_read_view().expect("pinned");

    let outcome = state
        .replace_frozen_with_l0_table(
            1,
            branch_owned_table(
                branch,
                BranchLevel::ZERO,
                "replace-old-l0",
                vec![older_frozen.clone()],
            ),
        )
        .expect("replace older frozen");
    assert_eq!(outcome.replaced_frozen_index(), Some(1));
    assert_eq!(state.frozen_table_count(), 1);
    assert_eq!(state.owned_levels()[0].len(), 2);
    assert_eq!(
        state.owned_levels()[0][0].descriptor().identity().as_str(),
        "replace-old-l0"
    );
    assert_eq!(
        state.owned_levels()[0][1].descriptor().identity().as_str(),
        "preexisting-l0"
    );

    let old_key = physical_key(branch, b"replace-old".to_vec());
    let new_key = physical_key(branch, b"replace-new".to_vec());
    assert_visible_row(
        pinned.latest(&old_key).expect("pinned old").as_ref(),
        &older_frozen,
        BranchRowSource::Frozen { index: 1 },
    );
    let after = state.capture_read_view().expect("after");
    assert_visible_row(
        after.latest(&old_key).expect("after old").as_ref(),
        &older_frozen,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_visible_row(
        after.latest(&new_key).expect("after new").as_ref(),
        &newer_frozen,
        BranchRowSource::Frozen { index: 0 },
    );
}

#[test]
fn branch_owned_nonzero_levels_are_sorted_and_reject_overlaps_without_mutation() {
    let branch = branch_id(53);
    let config = BranchRuntimeConfig::new(3, 64, 32).expect("config");
    let mut state = BranchLocalState::new(branch, config).expect("state");
    let level = BranchLevel::new(1);
    let z_table = branch_owned_table(
        branch,
        level,
        "level-one-z",
        vec![storage_row_with(
            branch,
            b"z".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"z".to_vec(),
        )],
    );
    let ac_table = branch_owned_table(
        branch,
        level,
        "level-one-ac",
        vec![
            storage_row_with(
                branch,
                b"a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"a".to_vec(),
            ),
            storage_row_with(
                branch,
                b"c".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"c".to_vec(),
            ),
        ],
    );

    assert_eq!(
        state
            .install_owned_table_at_level(level, z_table)
            .expect("install z")
            .table_index(),
        0
    );
    assert_eq!(
        state
            .install_owned_table_at_level(level, ac_table)
            .expect("install ac")
            .table_index(),
        0
    );
    assert_eq!(
        state.owned_levels()[1][0].descriptor().identity().as_str(),
        "level-one-ac"
    );
    assert_eq!(
        state.owned_levels()[1][1].descriptor().identity().as_str(),
        "level-one-z"
    );

    let before = state.clone();
    let overlap = branch_owned_table(
        branch,
        level,
        "level-one-overlap",
        vec![storage_row_with(
            branch,
            b"b".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"b".to_vec(),
        )],
    );
    assert!(matches!(
        state.install_owned_table_at_level(level, overlap),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
    assert_eq!(state, before);

    let wrong_level_table = branch_owned_table(
        branch,
        BranchLevel::new(2),
        "wrong-level",
        vec![storage_row(branch, 9)],
    );
    assert!(matches!(
        state.install_owned_table_at_level(level, wrong_level_table),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
}

#[test]
fn branch_owned_level_outside_configured_count_is_rejected_without_mutation() {
    let branch = branch_id(70);
    let config = BranchRuntimeConfig::new(3, 64, 32).expect("config");
    let mut state = BranchLocalState::new(branch, config).expect("state");
    let outside = BranchLevel::new(3);
    let outside_level_table = branch_owned_table(
        branch,
        outside,
        "outside-level",
        vec![storage_row(branch, 10)],
    );
    let before_outside = state.clone();
    assert!(matches!(
        state.install_owned_table_at_level(outside, outside_level_table),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
    assert_eq!(state, before_outside);
}

#[test]
fn branch_read_view_merges_owned_tables_with_active_and_frozen_by_commit_version() {
    let branch = branch_id(54);
    let mut state = BranchLocalState::empty(branch);
    let key = physical_key(branch, b"owned-chain".to_vec());
    let frozen_newer = storage_row_with(
        branch,
        b"owned-chain".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        b"frozen".to_vec(),
    );
    let active_older = storage_row_with(
        branch,
        b"owned-chain".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"active".to_vec(),
    );
    let owned_middle = storage_row_with(
        branch,
        b"owned-chain".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"owned".to_vec(),
    );

    state
        .append_committed_row(frozen_newer.clone())
        .expect("append frozen");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .append_committed_row(active_older.clone())
        .expect("append active");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "owned-chain",
            vec![owned_middle.clone()],
        ))
        .expect("install owned");

    let view = state.capture_read_view().expect("view");
    assert_eq!(
        view.latest(&key).expect("latest").expect("newest").source(),
        BranchRowSource::Frozen { index: 0 }
    );
    assert_eq!(
        view.at_version(&key, CommitVersion::new(5))
            .expect("at five")
            .expect("owned")
            .source(),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        }
    );
    assert_eq!(
        view.at_version(&key, CommitVersion::new(2))
            .expect("at two")
            .expect("active")
            .row(),
        &active_older
    );
    assert_eq!(
        history_versions(
            &view
                .history(&key, BranchHistoryOptions::all())
                .expect("history")
        ),
        vec![7, 5, 2]
    );
}

#[test]
fn branch_immutable_point_reads_choose_newer_between_active_and_l0() {
    let branch = branch_id(61);
    let mut state = BranchLocalState::empty(branch);
    let active_wins = storage_row_with(
        branch,
        b"active-wins".to_vec(),
        9,
        90,
        Timestamp::EPOCH,
        b"active".to_vec(),
    );
    let owned_beats_active_old = storage_row_with(
        branch,
        b"owned-beats-active".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"old-active".to_vec(),
    );
    state
        .append_committed_row(active_wins.clone())
        .expect("append active winner");
    state
        .append_committed_row(owned_beats_active_old)
        .expect("append active loser");
    let owned_rows = vec![
        storage_row_with(
            branch,
            b"active-wins".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            b"old-owned".to_vec(),
        ),
        storage_row_with(
            branch,
            b"owned-beats-active".to_vec(),
            7,
            70,
            Timestamp::EPOCH,
            b"owned-active".to_vec(),
        ),
    ];
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "active-l0-precedence",
            owned_rows.clone(),
        ))
        .expect("install L0");

    let view = state.capture_read_view().expect("view");
    assert_visible_row(
        view.latest(&physical_key(branch, b"active-wins".to_vec()))
            .expect("active wins")
            .as_ref(),
        &active_wins,
        BranchRowSource::Active,
    );
    assert_visible_row(
        view.latest(&physical_key(branch, b"owned-beats-active".to_vec()))
            .expect("owned active")
            .as_ref(),
        &owned_rows[1],
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
}

#[test]
fn branch_immutable_point_reads_choose_newer_between_frozen_l0_and_l1() {
    let branch = branch_id(62);
    let mut state = BranchLocalState::empty(branch);
    let frozen_wins = storage_row_with(
        branch,
        b"frozen-wins".to_vec(),
        8,
        80,
        Timestamp::EPOCH,
        b"frozen".to_vec(),
    );
    state
        .append_committed_row(frozen_wins.clone())
        .expect("append frozen winner");
    state
        .append_committed_row(storage_row_with(
            branch,
            b"owned-beats-frozen".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"old-frozen".to_vec(),
        ))
        .expect("append frozen loser");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));

    let owned_rows = vec![
        storage_row_with(
            branch,
            b"frozen-wins".to_vec(),
            5,
            50,
            Timestamp::EPOCH,
            b"old-owned".to_vec(),
        ),
        storage_row_with(
            branch,
            b"owned-beats-frozen".to_vec(),
            6,
            60,
            Timestamp::EPOCH,
            b"owned-frozen".to_vec(),
        ),
    ];
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "frozen-l0-precedence",
            owned_rows.clone(),
        ))
        .expect("install L0");
    let l1_only = storage_row_with(
        branch,
        b"l1-only".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"l1".to_vec(),
    );
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "source-precedence-l1",
                vec![l1_only.clone()],
            ),
        )
        .expect("install L1");

    let view = state.capture_read_view().expect("view");
    assert_visible_row(
        view.latest(&physical_key(branch, b"frozen-wins".to_vec()))
            .expect("frozen wins")
            .as_ref(),
        &frozen_wins,
        BranchRowSource::Frozen { index: 0 },
    );
    assert_visible_row(
        view.latest(&physical_key(branch, b"owned-beats-frozen".to_vec()))
            .expect("owned frozen")
            .as_ref(),
        &owned_rows[1],
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_visible_row(
        view.latest(&physical_key(branch, b"l1-only".to_vec()))
            .expect("l1 only")
            .as_ref(),
        &l1_only,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );
}

#[test]
fn branch_immutable_version_reads_cover_tombstone_bounds() {
    let branch = branch_id(63);
    let mut state = BranchLocalState::empty(branch);
    let deleted_put = storage_row_with(
        branch,
        b"deleted".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"before-delete".to_vec(),
    );
    let tombstone_above_put = storage_row_with(
        branch,
        b"tombstone-above".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"visible-before-tombstone".to_vec(),
    );
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "version-tombstone-l0",
            vec![
                deleted_put.clone(),
                tombstone_row(branch, b"deleted".to_vec(), 3, 30),
                tombstone_above_put.clone(),
                tombstone_row(branch, b"tombstone-above".to_vec(), 5, 50),
            ],
        ))
        .expect("install version table");
    let view = state.capture_read_view().expect("view");

    assert_visible_row(
        view.at_version(
            &physical_key(branch, b"deleted".to_vec()),
            CommitVersion::new(2),
        )
        .expect("before delete")
        .as_ref(),
        &deleted_put,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert!(view
        .at_version(
            &physical_key(branch, b"deleted".to_vec()),
            CommitVersion::new(3),
        )
        .expect("at delete")
        .is_none());
    assert_visible_row(
        view.at_version(
            &physical_key(branch, b"tombstone-above".to_vec()),
            CommitVersion::new(4),
        )
        .expect("below tombstone")
        .as_ref(),
        &tombstone_above_put,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert!(view
        .at_version(
            &physical_key(branch, b"tombstone-above".to_vec()),
            CommitVersion::new(5),
        )
        .expect("at tombstone")
        .is_none());
}

#[test]
fn branch_immutable_version_reads_cover_zero_and_max_commit_bounds() {
    let branch = branch_id(64);
    let mut state = BranchLocalState::empty(branch);
    let zero = storage_row_with(
        branch,
        b"zero-owned".to_vec(),
        0,
        0,
        Timestamp::EPOCH,
        b"zero".to_vec(),
    );
    let max = storage_row_with(
        branch,
        b"max-owned".to_vec(),
        u64::MAX,
        90,
        Timestamp::EPOCH,
        b"max".to_vec(),
    );
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "version-extremes-l0",
            vec![zero.clone(), max.clone()],
        ))
        .expect("install version extremes");
    let view = state.capture_read_view().expect("view");

    assert_visible_row(
        view.at_version(
            &physical_key(branch, b"zero-owned".to_vec()),
            CommitVersion::ZERO,
        )
        .expect("zero")
        .as_ref(),
        &zero,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_visible_row(
        view.at_version(
            &physical_key(branch, b"max-owned".to_vec()),
            CommitVersion::MAX,
        )
        .expect("max")
        .as_ref(),
        &max,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
}

#[test]
fn branch_immutable_history_filters_tombstones_limits_and_cross_level_versions() {
    let branch = branch_id(65);
    let mut state = BranchLocalState::empty(branch);
    let key = physical_key(branch, b"owned-history".to_vec());
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "history-l1",
                vec![storage_row_with(
                    branch,
                    b"owned-history".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    b"old".to_vec(),
                )],
            ),
        )
        .expect("install history L1");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "history-l0",
            vec![
                storage_row_with(
                    branch,
                    b"owned-history".to_vec(),
                    3,
                    30,
                    Timestamp::EPOCH,
                    b"new".to_vec(),
                ),
                tombstone_row(branch, b"owned-history".to_vec(), 2, 20),
            ],
        ))
        .expect("install history L0");
    let view = state.capture_read_view().expect("view");

    assert_eq!(
        history_versions(
            &view
                .history(&key, BranchHistoryOptions::all())
                .expect("all")
        ),
        vec![3, 2, 1]
    );
    assert_eq!(
        history_versions(
            &view
                .history(&key, BranchHistoryOptions::all().include_tombstones(false))
                .expect("without tombstones")
        ),
        vec![3, 1]
    );
    assert_eq!(
        history_versions(
            &view
                .history(
                    &key,
                    BranchHistoryOptions::all().before_version(CommitVersion::new(3)),
                )
                .expect("before three")
        ),
        vec![2, 1]
    );
    assert!(view
        .history(&key, BranchHistoryOptions::all().limit(0))
        .expect("limit zero")
        .is_empty());
    assert_eq!(
        history_versions(
            &view
                .history(
                    &key,
                    BranchHistoryOptions::all()
                        .include_tombstones(false)
                        .limit(1),
                )
                .expect("filtered limit")
        ),
        vec![3]
    );
}

#[test]
fn branch_immutable_prefix_scans_merge_sources_and_respect_spaces() {
    let branch = branch_id(66);
    let mut state = BranchLocalState::empty(branch);
    let frozen = storage_row_with(
        branch,
        b"scan-b".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"frozen".to_vec(),
    );
    let active = storage_row_with(
        branch,
        b"scan-a".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"active".to_vec(),
    );
    state
        .append_committed_row(frozen.clone())
        .expect("append frozen");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
        .append_committed_row(active.clone())
        .expect("append active");
    let scan_c_new = storage_row_with(
        branch,
        b"scan-c".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"new-c".to_vec(),
    );
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "scan-l0-new",
            vec![
                scan_c_new.clone(),
                tombstone_row(branch, b"scan-d".to_vec(), 6, 60),
            ],
        ))
        .expect("install scan L0 new");
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "scan-l0-old",
            vec![
                storage_row_with(
                    branch,
                    b"scan-c".to_vec(),
                    2,
                    20,
                    Timestamp::EPOCH,
                    b"old-c".to_vec(),
                ),
                storage_row_with(
                    branch,
                    b"scan-d".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    b"old-d".to_vec(),
                ),
                storage_row_with_named_space(
                    branch,
                    "other-space",
                    StorageSpaceId::engine(0x20).expect("engine space"),
                    b"scan-a".to_vec(),
                    9,
                    90,
                    b"other-space".to_vec(),
                ),
            ],
        ))
        .expect("install scan L0 old");

    let view = state.capture_read_view().expect("view");
    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"scan-".to_vec()));
    let prefix_rows = view
        .scan_prefix(&prefix, BranchReadBound::latest())
        .expect("prefix scan");
    assert_eq!(
        scan_user_keys(&prefix_rows),
        vec![b"scan-a".to_vec(), b"scan-b".to_vec(), b"scan-c".to_vec(),]
    );
    assert_eq!(prefix_rows[0].row(), &active);
    assert_eq!(prefix_rows[1].row(), &frozen);
    assert_eq!(prefix_rows[2].row(), &scan_c_new);
}

#[test]
fn branch_immutable_prefix_scan_includes_l1_and_excludes_storage_space_id() {
    let branch = branch_id(69);
    let mut state = BranchLocalState::empty(branch);
    let l1_row = storage_row_with(
        branch,
        b"scan-l1".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"l1".to_vec(),
    );
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "scan-other-storage-space",
            vec![storage_row_with_named_space(
                branch,
                "default",
                StorageSpaceId::engine(0x21).expect("engine space"),
                b"scan-l1".to_vec(),
                2,
                20,
                b"other-storage-space".to_vec(),
            )],
        ))
        .expect("install other storage-space L0");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "scan-l1-default-space",
                vec![l1_row.clone()],
            ),
        )
        .expect("install L1");

    let view = state.capture_read_view().expect("view");
    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"scan-".to_vec()));
    let rows = view
        .scan_prefix(&prefix, BranchReadBound::latest())
        .expect("prefix scan");
    assert_eq!(scan_user_keys(&rows), vec![b"scan-l1".to_vec()]);
    assert_visible_row(
        rows.first(),
        &l1_row,
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        },
    );
}

#[test]
fn branch_immutable_range_scans_cover_l1_edge_and_degenerate_bounds() {
    let branch = branch_id(67);
    let mut state = BranchLocalState::empty(branch);
    let scan_e = storage_row_with(
        branch,
        b"scan-e".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"e".to_vec(),
    );
    let scan_g = storage_row_with(
        branch,
        b"scan-g".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"g".to_vec(),
    );
    let scan_h = storage_row_with(
        branch,
        b"scan-h".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"h".to_vec(),
    );
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "scan-l1-e-g",
                vec![scan_e, scan_g],
            ),
        )
        .expect("install scan L1 e-g");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(branch, BranchLevel::new(1), "scan-l1-h", vec![scan_h]),
        )
        .expect("install scan L1 h");

    let view = state.capture_read_view().expect("view");
    let closed = BranchScanBounds::closed(
        &physical_key(branch, b"scan-e".to_vec()),
        &physical_key(branch, b"scan-g".to_vec()),
    )
    .expect("closed range");
    assert_eq!(
        scan_user_keys(
            &view
                .scan_range(&closed, BranchReadBound::latest())
                .expect("closed range")
        ),
        vec![b"scan-e".to_vec(), b"scan-g".to_vec()]
    );
    let open = BranchScanBounds::open(
        &physical_key(branch, b"scan-e".to_vec()),
        &physical_key(branch, b"scan-g".to_vec()),
    )
    .expect("open range");
    assert!(view
        .scan_range(&open, BranchReadBound::latest())
        .expect("open range")
        .is_empty());
    let degenerate = BranchScanBounds::closed(
        &physical_key(branch, b"scan-h".to_vec()),
        &physical_key(branch, b"scan-h".to_vec()),
    )
    .expect("degenerate range");
    assert_eq!(
        scan_user_keys(
            &view
                .scan_range(&degenerate, BranchReadBound::latest())
                .expect("degenerate range")
        ),
        vec![b"scan-h".to_vec()]
    );
}

#[test]
fn branch_runtime_stats_default_and_accessors_are_stable() {
    let empty = BranchRuntimeStats::default();
    assert_eq!(empty.latest_reads(), 0);
    assert_eq!(empty.bounded_reads(), 0);
    assert_eq!(empty.history_reads(), 0);
    assert_eq!(empty.inherited_layers_examined(), 0);

    let stats = BranchRuntimeStats::new(1, 2, 3, 4);
    assert_eq!(stats.latest_reads(), 1);
    assert_eq!(stats.bounded_reads(), 2);
    assert_eq!(stats.history_reads(), 3);
    assert_eq!(stats.inherited_layers_examined(), 4);
}

#[test]
fn branch_runtime_errors_are_typed_and_preserve_sources() {
    let test_branch_id = branch_id(6);
    let invalid_config = BranchRuntimeConfig::new(0, 1, 1).expect_err("invalid config");
    let table_error = BranchRuntimeError::TableRuntime {
        source: crate::table::TableRuntimeError::Cache {
            reason: "cache unavailable",
        },
    };
    let variants = [
        invalid_config,
        BranchRuntimeError::InvalidBranchState { reason: "state" },
        BranchRuntimeError::BranchNotFound {
            branch_id: test_branch_id,
        },
        BranchRuntimeError::BranchAlreadyExists {
            branch_id: test_branch_id,
        },
        BranchRuntimeError::InvalidBranchRow { reason: "row" },
        BranchRuntimeError::InvalidReadBound { reason: "bound" },
        BranchRuntimeError::InvalidInheritedLayer { reason: "layer" },
        BranchRuntimeError::InvalidReachability {
            reason: "reachability",
        },
        table_error.clone(),
        BranchRuntimeError::publish("publish"),
    ];

    for error in variants {
        let text = error.to_string();
        assert!(!text.is_empty());
        assert!(!text.contains("secret-payload"));
    }

    let alias_result: BranchRuntimeResult<()> =
        Err(BranchRuntimeError::InvalidReadBound { reason: "alias" });
    assert!(matches!(
        alias_result,
        Err(BranchRuntimeError::InvalidReadBound { reason: "alias" })
    ));

    let source = table_error.source().expect("table source");
    assert!(source.to_string().contains("table cache operation failed"));

    let publish_error = BranchRuntimeError::publish_with("ambiguous", LeafError);
    let source = publish_error.source().expect("publish source");
    assert_eq!(source.to_string(), "leaf source");
}

fn assert_invalid_config_field(
    result: BranchRuntimeResult<BranchRuntimeConfig>,
    expected_field: &'static str,
) {
    match result {
        Err(BranchRuntimeError::InvalidConfig { field, .. }) => {
            assert_eq!(field, expected_field);
        }
        other => panic!("expected invalid config field {expected_field}, got {other:?}"),
    }
}

#[test]
fn branch_inherited_layer_constructor_rejects_count_and_source_mismatches() {
    let source = branch_id(70);
    let child = branch_id(71);
    let table = branch_owned_table(
        source,
        BranchLevel::ZERO,
        "inherited-valid",
        vec![storage_row_with(
            source,
            b"inherited".to_vec(),
            3,
            30,
            Timestamp::EPOCH,
            b"secret-payload".to_vec(),
        )],
    );

    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(3),
        InheritedLayerStatus::Active,
        vec![vec![table.clone()]],
    );
    assert_eq!(layer.source_branch_id(), source);
    assert_eq!(layer.fork_version(), CommitVersion::new(3));
    assert_eq!(layer.table_count(), 1);

    let stale_count = BranchInheritedLayer::new(
        InheritedLayerDescriptor::new(
            source,
            CommitVersion::new(3),
            InheritedLayerStatus::Active,
            2,
        ),
        vec![vec![table.clone()]],
    )
    .expect_err("stale inherited table count rejected");
    assert!(matches!(
        stale_count,
        BranchRuntimeError::InvalidInheritedLayer { .. }
    ));
    assert!(!stale_count.to_string().contains("secret-payload"));

    let wrong_table = branch_owned_table(
        child,
        BranchLevel::ZERO,
        "inherited-wrong-source",
        vec![storage_row_with(
            child,
            b"inherited".to_vec(),
            3,
            30,
            Timestamp::EPOCH,
            b"secret-payload".to_vec(),
        )],
    );
    let wrong_source = BranchInheritedLayer::new(
        InheritedLayerDescriptor::new(
            source,
            CommitVersion::new(3),
            InheritedLayerStatus::Active,
            1,
        ),
        vec![vec![wrong_table]],
    )
    .expect_err("wrong source table rejected");
    assert!(matches!(
        wrong_source,
        BranchRuntimeError::InvalidInheritedLayer { .. }
    ));
    assert!(!wrong_source.to_string().contains("secret-payload"));

    let view_error = BranchReadView::new_with_inherited(
        child,
        MutableTable::new(),
        Vec::new(),
        Vec::new(),
        vec![branch_inherited_layer(
            child,
            CommitVersion::new(3),
            InheritedLayerStatus::Active,
            Vec::new(),
        )],
        BranchStateFacts::new(child, 0, 0, 0, 1, Some(CommitVersion::new(3)), None, None)
            .expect("self inherited facts"),
    )
    .expect_err("self inheritance rejected");
    assert!(matches!(
        view_error,
        BranchRuntimeError::InvalidInheritedLayer { .. }
    ));
}

#[test]
fn branch_inherited_layer_rejects_duplicate_internal_keys_across_tables() {
    let source = branch_id(71);
    let duplicate_left = branch_owned_table(
        source,
        BranchLevel::ZERO,
        "inherited-duplicate-left",
        vec![storage_row_with(
            source,
            b"duplicate".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            b"secret-payload-left".to_vec(),
        )],
    );
    let duplicate_right = branch_owned_table(
        source,
        BranchLevel::ZERO,
        "inherited-duplicate-right",
        vec![storage_row_with(
            source,
            b"duplicate".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            b"secret-payload-right".to_vec(),
        )],
    );

    let error = BranchInheritedLayer::new(
        InheritedLayerDescriptor::new(
            source,
            CommitVersion::new(4),
            InheritedLayerStatus::Active,
            2,
        ),
        vec![vec![duplicate_left, duplicate_right]],
    )
    .expect_err("duplicate inherited internal keys rejected");
    assert!(matches!(
        error,
        BranchRuntimeError::InvalidInheritedLayer { .. }
    ));
    assert!(!error.to_string().contains("secret-payload"));
}

#[test]
fn branch_inherited_layer_status_and_count_edges_are_enforced() {
    let source = branch_id(72);
    let child = branch_id(73);
    let row = storage_row_with(
        source,
        b"status".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"source".to_vec(),
    );
    let materializing = branch_inherited_layer(
        source,
        CommitVersion::new(3),
        InheritedLayerStatus::Materializing,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "inherited-materializing",
            vec![row.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![materializing])
        .expect("materializing layer attaches");
    let visible = child_state
        .capture_read_view()
        .expect("materializing view")
        .latest(&physical_key(child, b"status".to_vec()))
        .expect("materializing latest")
        .expect("materializing inherited row");
    assert_eq!(
        visible.row(),
        &rewrite_row_branch(&row, source, child).expect("rewrite")
    );

    let materialized = branch_inherited_layer(
        source,
        CommitVersion::new(3),
        InheritedLayerStatus::Materialized,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "inherited-materialized",
            vec![row],
        )]],
    );
    let mut materialized_child = BranchLocalState::empty(child);
    materialized_child
        .attach_inherited_layers(vec![materialized])
        .expect("materialized layer attaches for diagnostic state");
    assert!(materialized_child
        .capture_read_view()
        .expect("materialized view")
        .latest(&physical_key(child, b"status".to_vec()))
        .expect("materialized latest")
        .is_none());

    let unavailable = branch_inherited_layer(
        source,
        CommitVersion::new(3),
        InheritedLayerStatus::Unavailable,
        Vec::new(),
    );
    let mut unavailable_child = BranchLocalState::empty(child);
    assert!(matches!(
        unavailable_child.attach_inherited_layers(vec![unavailable]),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. })
    ));

    let config = BranchRuntimeConfig::new(7, 1, 32).expect("one inherited layer config");
    let mut limited_child = BranchLocalState::new(child, config).expect("limited child");
    let first = branch_inherited_layer(
        source,
        CommitVersion::new(3),
        InheritedLayerStatus::Active,
        Vec::new(),
    );
    let second_source = branch_id(74);
    let second = branch_inherited_layer(
        second_source,
        CommitVersion::new(3),
        InheritedLayerStatus::Active,
        Vec::new(),
    );
    assert!(matches!(
        limited_child.attach_inherited_layers(vec![first, second]),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. })
    ));
}

#[test]
fn branch_fork_preserves_layer_order_and_resets_readable_inherited_statuses() {
    let fixture = fork_status_fixture();
    assert_eq!(fixture.outcome.fork_version(), CommitVersion::new(6));
    assert_eq!(fixture.outcome.inherited_layer_count(), 2);
    assert_eq!(fixture.outcome.inherited_table_count(), 2);
    assert_eq!(
        fixture.child_state.inherited_layers()[0].source_branch_id(),
        fixture.source
    );
    assert_eq!(
        fixture.child_state.inherited_layers()[0].status(),
        InheritedLayerStatus::Active
    );
    assert_eq!(
        fixture.child_state.inherited_layers()[1].source_branch_id(),
        fixture.grandparent
    );
    assert_eq!(
        fixture.child_state.inherited_layers()[1].status(),
        InheritedLayerStatus::Active,
        "copied materializing inherited layers reset to active"
    );

    let view = fixture.child_state.capture_read_view().expect("child view");
    assert_visible_row(
        view.latest(&physical_key(fixture.child, b"source-owned".to_vec()))
            .expect("source-owned latest")
            .as_ref(),
        &rewrite_row_branch(&fixture.source_owned, fixture.source, fixture.child)
            .expect("source-owned rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: fixture.source,
            layer_index: 0,
        },
    );
    assert_visible_row(
        view.latest(&physical_key(fixture.child, b"materializing".to_vec()))
            .expect("materializing latest")
            .as_ref(),
        &rewrite_row_branch(&fixture.inherited_row, fixture.grandparent, fixture.child)
            .expect("inherited rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: fixture.grandparent,
            layer_index: 1,
        },
    );
    assert!(
        view.latest(&physical_key(fixture.child, b"materialized".to_vec()))
            .expect("materialized latest")
            .is_none(),
        "materialized source layers are skipped when forking"
    );
}

#[test]
fn branch_fork_and_attach_rejections_do_not_mutate_state() {
    let branch = branch_id(87);
    let other = branch_id(88);
    let mut child_state = BranchLocalState::empty(branch);
    let original = child_state.clone();
    child_state
        .append_committed_row(storage_row_with(
            branch,
            b"owned-before-attach".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"owned".to_vec(),
        ))
        .expect("append own row");
    let non_empty = child_state.clone();
    let layer = branch_inherited_layer(
        other,
        CommitVersion::new(1),
        InheritedLayerStatus::Active,
        Vec::new(),
    );

    assert!(matches!(
        child_state.attach_inherited_layers(vec![layer]),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
    assert_eq!(child_state, non_empty);
    assert_ne!(child_state, original);

    let source_state = BranchLocalState::empty(branch);
    let source_before = source_state.clone();
    assert!(matches!(
        source_state.fork_into_empty_child(branch),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. })
    ));
    assert_eq!(source_state, source_before);

    let unavailable = branch_inherited_layer(
        other,
        CommitVersion::new(1),
        InheritedLayerStatus::Unavailable,
        Vec::new(),
    );
    assert!(matches!(
        unavailable.clone_active_for_fork(),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. })
    ));
}

#[test]
fn branch_fork_into_empty_child_captures_inherited_layers_without_copying_rows() {
    let source = branch_id(72);
    let child = branch_id(73);
    let mut source_state = BranchLocalState::empty(source);
    let inherited_row = storage_row_with(
        source,
        b"shared".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"parent".to_vec(),
    );
    let table = branch_owned_table(
        source,
        BranchLevel::ZERO,
        "fork-source-owned",
        vec![inherited_row.clone()],
    );
    source_state
        .install_l0_table(table)
        .expect("source install");
    source_state
        .append_committed_row(storage_row_with(
            source,
            b"active-only".to_vec(),
            8,
            80,
            Timestamp::EPOCH,
            b"not-inherited-by-l6f".to_vec(),
        ))
        .expect("source active append");

    let (child_state, outcome): (BranchLocalState, BranchForkOutcome) = source_state
        .fork_into_empty_child(child)
        .expect("fork child");
    assert_eq!(outcome.source_branch_id(), source);
    assert_eq!(outcome.destination_branch_id(), child);
    assert_eq!(outcome.fork_version(), CommitVersion::new(8));
    assert_eq!(outcome.inherited_layer_count(), 1);
    assert_eq!(outcome.inherited_table_count(), 1);
    assert!(child_state.active().is_empty());
    assert!(child_state.frozen().is_empty());
    assert_eq!(child_state.owned_table_count(), 0);
    assert_eq!(child_state.inherited_layer_count(), 1);
    assert_eq!(child_state.inherited_table_count(), 1);
    assert_eq!(child_state.inherited_layers()[0].source_branch_id(), source);
    assert_eq!(
        child_state.max_commit_version(),
        Some(CommitVersion::new(8))
    );
    assert_eq!(child_state.put_rows(), 0);

    let view = child_state.capture_read_view().expect("child read view");
    assert_eq!(view.inherited_layer_count(), 1);
    assert_eq!(
        view.inherited_layers()[0].fork_version(),
        CommitVersion::new(8)
    );
    let expected = rewrite_row_branch(&inherited_row, source, child).expect("expected rewrite");
    let visible = view
        .latest(&physical_key(child, b"shared".to_vec()))
        .expect("latest")
        .expect("inherited row");
    assert_eq!(visible.row(), &expected);
    assert_eq!(
        visible.source(),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        }
    );
    assert!(
        view.latest(&physical_key(child, b"active-only".to_vec()))
            .expect("active-only latest")
            .is_none(),
        "L6F must not silently inherit source active/frozen rows"
    );
}

#[test]
fn branch_inherited_reads_apply_fork_gate_and_child_tombstone_shadowing() {
    let source = branch_id(74);
    let child = branch_id(75);
    let visible_source = storage_row_with(
        source,
        b"gate".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"visible".to_vec(),
    );
    let post_fork_source = storage_row_with(
        source,
        b"gate".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        b"post-fork".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "fork-gated-source",
            vec![visible_source.clone(), post_fork_source],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");

    let child_key = physical_key(child, b"gate".to_vec());
    let view = child_state.capture_read_view().expect("view");
    let expected = rewrite_row_branch(&visible_source, source, child).expect("rewrite");
    assert_visible_row(
        view.latest(&child_key).expect("latest").as_ref(),
        &expected,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    assert_visible_row(
        view.at_version(&child_key, CommitVersion::new(7))
            .expect("bounded")
            .as_ref(),
        &expected,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );

    child_state
        .append_committed_row(tombstone_row(child, b"gate".to_vec(), 6, 60))
        .expect("child tombstone");
    let shadowed = child_state.capture_read_view().expect("shadowed view");
    assert!(
        shadowed.latest(&child_key).expect("latest").is_none(),
        "child tombstone must shadow inherited put"
    );
    assert_visible_row(
        shadowed
            .at_version(&child_key, CommitVersion::new(4))
            .expect("before tombstone")
            .as_ref(),
        &expected,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
}

#[test]
fn branch_inherited_timestamp_reads_apply_timestamp_and_fork_gates() {
    let source = branch_id(61);
    let child = branch_id(62);
    let visible_source = storage_row_with(
        source,
        b"time-gate".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"visible".to_vec(),
    );
    let source_future_timestamp = storage_row_with(
        source,
        b"time-gate".to_vec(),
        4,
        70,
        Timestamp::EPOCH,
        b"future-time".to_vec(),
    );
    let source_after_fork_with_old_timestamp = storage_row_with(
        source,
        b"time-gate".to_vec(),
        7,
        20,
        Timestamp::EPOCH,
        b"after-fork".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "inherited-time-gate",
            vec![
                visible_source.clone(),
                source_future_timestamp,
                source_after_fork_with_old_timestamp,
            ],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let child_key = physical_key(child, b"time-gate".to_vec());
    let view = child_state.capture_read_view().expect("view");
    assert_visible_row(
        view.read_point(
            &child_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .expect("timestamp inherited read")
        .as_ref(),
        &rewrite_row_branch(&visible_source, source, child).expect("visible rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    assert_eq!(
        view.read_point(
            &child_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
        )
        .expect("below visible timestamp"),
        None,
        "post-fork source row with an old timestamp remains hidden by fork version",
    );

    let child_expired = storage_row_with(
        child,
        b"time-gate".to_vec(),
        6,
        35,
        Timestamp::from_micros(39),
        b"child-expired".to_vec(),
    );
    child_state
        .append_committed_row(child_expired)
        .expect("append child expired row");
    let shadowed = child_state.capture_read_view().expect("shadowed view");
    assert_eq!(
        shadowed
            .read_point(
                &child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            )
            .expect("timestamp expired child read"),
        None,
        "selected child-local expired row suppresses inherited fallback",
    );
}

#[test]
fn branch_inherited_timestamp_reads_pick_nearest_layer_for_exact_ties() {
    let (child_state, fixture) = inherited_timestamp_shadow_fixture();
    let inherited_view = child_state.capture_read_view().expect("view");
    assert_visible_row(
        inherited_view
            .read_point(
                &fixture.child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            )
            .expect("nearest inherited timestamp read")
            .as_ref(),
        &fixture.expected_nearest,
        BranchRowSource::Inherited {
            source_branch_id: fixture.nearest_source,
            layer_index: 0,
        },
    );
}

#[test]
fn branch_inherited_timestamp_reads_apply_local_put_and_tombstone_shadows() {
    let (mut child_state, fixture) = inherited_timestamp_shadow_fixture();
    let child_put = storage_row_with(
        fixture.child,
        fixture.key.clone(),
        4,
        35,
        Timestamp::EPOCH,
        b"child-put".to_vec(),
    );
    child_state
        .append_committed_row(child_put.clone())
        .expect("append child put");
    assert_visible_row(
        child_state
            .capture_read_view()
            .expect("put view")
            .read_point(
                &fixture.child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            )
            .expect("child put timestamp read")
            .as_ref(),
        &child_put,
        BranchRowSource::Active,
    );

    child_state
        .append_committed_row(tombstone_row(fixture.child, fixture.key, 5, 45))
        .expect("append child tombstone");
    assert_eq!(
        child_state
            .capture_read_view()
            .expect("tombstone view")
            .read_point(
                &fixture.child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(50)),
            )
            .expect("child tombstone timestamp read"),
        None,
        "child-local tombstone at timestamp shadows inherited puts",
    );
}

struct InheritedTimestampShadowFixture {
    nearest_source: BranchId,
    child: BranchId,
    key: Vec<u8>,
    child_key: PhysicalKey,
    expected_nearest: StorageRow,
}

fn inherited_timestamp_shadow_fixture() -> (BranchLocalState, InheritedTimestampShadowFixture) {
    let nearest_source = branch_id(63);
    let farther_source = branch_id(64);
    let child = branch_id(65);
    let key = b"time-shadow".to_vec();
    let nearest_row = storage_row_with(
        nearest_source,
        key.clone(),
        3,
        30,
        Timestamp::EPOCH,
        b"nearest".to_vec(),
    );
    let farther_row = storage_row_with(
        farther_source,
        key.clone(),
        3,
        30,
        Timestamp::EPOCH,
        b"farther".to_vec(),
    );
    let nearest_layer = branch_inherited_layer(
        nearest_source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            nearest_source,
            BranchLevel::ZERO,
            "nearest-timestamp-tie",
            vec![nearest_row.clone()],
        )]],
    );
    let farther_layer = branch_inherited_layer(
        farther_source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            farther_source,
            BranchLevel::ZERO,
            "farther-timestamp-tie",
            vec![farther_row],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![nearest_layer, farther_layer])
        .expect("attach inherited layers");
    let child_key = physical_key(child, key.clone());
    let expected_nearest =
        rewrite_row_branch(&nearest_row, nearest_source, child).expect("nearest rewrite");
    (
        child_state,
        InheritedTimestampShadowFixture {
            nearest_source,
            child,
            key,
            child_key,
            expected_nearest,
        },
    )
}

#[test]
fn branch_inherited_timestamp_view_is_pinned_after_source_mutation() {
    let source = branch_id(66);
    let child = branch_id(67);
    let mut source_state = BranchLocalState::empty(source);
    let inherited = storage_row_with(
        source,
        b"source-pinned-ts".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"inherited".to_vec(),
    );
    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "source-pinned-ts-base",
            vec![inherited.clone()],
        ))
        .expect("install base source table");
    let (child_state, _) = source_state
        .fork_into_empty_child(child)
        .expect("fork child");
    let pinned = child_state.capture_read_view().expect("pinned child view");

    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "source-pinned-ts-later",
            vec![storage_row_with(
                source,
                b"source-pinned-ts-later".to_vec(),
                4,
                40,
                Timestamp::EPOCH,
                b"later".to_vec(),
            )],
        ))
        .expect("install later source table");

    let expected = rewrite_row_branch(&inherited, source, child).expect("rewrite inherited");
    assert_visible_row(
        pinned
            .read_point(
                &physical_key(child, b"source-pinned-ts".to_vec()),
                BranchReadBound::at_timestamp(Timestamp::from_micros(50)),
            )
            .expect("pinned inherited timestamp read")
            .as_ref(),
        &expected,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    assert_eq!(
        pinned
            .read_point(
                &physical_key(child, b"source-pinned-ts-later".to_vec()),
                BranchReadBound::at_timestamp(Timestamp::from_micros(50)),
            )
            .expect("pinned inherited later read"),
        None,
        "captured child timestamp view must not observe later source mutation",
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn branch_materialization_rewrites_retained_rows_without_cleanup() {
    let source = branch_id(91);
    let child = branch_id(92);
    let key = b"materialized-history".to_vec();
    let old_source = storage_row_with(
        source,
        key.clone(),
        1,
        10,
        Timestamp::EPOCH,
        b"old".to_vec(),
    );
    let mid_source = storage_row_with(
        source,
        key.clone(),
        3,
        30,
        Timestamp::EPOCH,
        b"mid".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(3),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materialize-source-history",
            vec![old_source.clone(), mid_source.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let child_newer = storage_row_with(
        child,
        key.clone(),
        5,
        50,
        Timestamp::EPOCH,
        b"child-newer".to_vec(),
    );
    child_state
        .append_committed_row(child_newer.clone())
        .expect("append child newer row");

    let child_key = physical_key(child, key);
    let before = child_state.capture_read_view().expect("before view");
    assert_visible_row(
        before.latest(&child_key).expect("before latest").as_ref(),
        &child_newer,
        BranchRowSource::Active,
    );
    assert_visible_row(
        before
            .at_version(&child_key, CommitVersion::new(2))
            .expect("before getv")
            .as_ref(),
        &rewrite_row_branch(&old_source, source, child).expect("old rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    assert_visible_row(
        before
            .read_point(
                &child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(35)),
            )
            .expect("before as-of")
            .as_ref(),
        &rewrite_row_branch(&mid_source, source, child).expect("mid rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );

    let outcome: BranchMaterializationOutcome = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-history")
                .expect("materialization request"),
        )
        .expect("materialize inherited layer");
    assert_eq!(outcome.child_branch_id(), child);
    assert_eq!(outcome.source_branch_id(), source);
    assert_eq!(outcome.fork_version(), CommitVersion::new(3));
    assert_eq!(outcome.layer_index(), 0);
    assert_eq!(outcome.rows_materialized(), 2);
    assert_eq!(outcome.tables_created(), 1);
    assert_eq!(outcome.skipped_post_fork_rows(), 0);
    assert_eq!(outcome.skipped_exact_duplicate_rows(), 0);
    assert_eq!(outcome.inherited_layers_remaining(), 0);
    assert_eq!(outcome.replacement_owned_table_count(), 1);
    assert_eq!(
        outcome.recovery(),
        BranchMaterializationRecovery::ReplacementVisibleLayerRemoved
    );
    assert_eq!(child_state.inherited_layer_count(), 0);
    assert_eq!(child_state.owned_table_count(), 1);

    let after = child_state.capture_read_view().expect("after view");
    assert_visible_row(
        after.latest(&child_key).expect("after latest").as_ref(),
        &child_newer,
        BranchRowSource::Active,
    );
    assert_visible_row(
        after
            .at_version(&child_key, CommitVersion::new(2))
            .expect("after getv")
            .as_ref(),
        &rewrite_row_branch(&old_source, source, child).expect("old rewrite"),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_visible_row(
        after
            .read_point(
                &child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(35)),
            )
            .expect("after as-of")
            .as_ref(),
        &rewrite_row_branch(&mid_source, source, child).expect("mid rewrite"),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_eq!(
        history_versions(
            &after
                .history(
                    &child_key,
                    BranchHistoryOptions::all().include_tombstones(true)
                )
                .expect("after history"),
        ),
        vec![5, 3, 1],
        "materialization must preserve retained row history",
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn branch_materialization_skips_post_fork_rows_and_exact_duplicates_only() {
    let source = branch_id(93);
    let child = branch_id(94);
    let post_fork = storage_row_with(
        source,
        b"post-fork".to_vec(),
        8,
        80,
        Timestamp::EPOCH,
        b"post".to_vec(),
    );
    let exact_duplicate = storage_row_with(
        source,
        b"exact".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"source-exact".to_vec(),
    );
    let retained_history = storage_row_with(
        source,
        b"history".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"source-history".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materialize-filter-source",
            vec![post_fork, exact_duplicate.clone(), retained_history.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let child_exact_duplicate = storage_row_with(
        child,
        b"exact".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"source-exact".to_vec(),
    );
    child_state
        .append_committed_row(child_exact_duplicate.clone())
        .expect("append exact duplicate");
    let child_newer_history = storage_row_with(
        child,
        b"history".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        b"child-newer-history".to_vec(),
    );
    child_state
        .append_committed_row(child_newer_history.clone())
        .expect("append newer history");

    let outcome = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-filter")
                .expect("materialization request"),
        )
        .expect("materialize inherited layer");
    assert_eq!(outcome.rows_materialized(), 1);
    assert_eq!(outcome.tables_created(), 1);
    assert_eq!(outcome.skipped_post_fork_rows(), 1);
    assert_eq!(outcome.skipped_exact_duplicate_rows(), 1);

    let view = child_state.capture_read_view().expect("after view");
    assert!(
        view.latest(&physical_key(child, b"post-fork".to_vec()))
            .expect("post-fork latest")
            .is_none(),
        "post-fork inherited rows must not be materialized",
    );
    assert_visible_row(
        view.latest(&physical_key(child, b"exact".to_vec()))
            .expect("exact latest")
            .as_ref(),
        &child_exact_duplicate,
        BranchRowSource::Active,
    );
    assert_eq!(
        history_versions(
            &view
                .history(
                    &physical_key(child, b"exact".to_vec()),
                    BranchHistoryOptions::all().include_tombstones(true),
                )
                .expect("exact history"),
        ),
        vec![4],
        "exact duplicate inherited row should be suppressed",
    );
    let history_key = physical_key(child, b"history".to_vec());
    assert_visible_row(
        view.latest(&history_key).expect("history latest").as_ref(),
        &child_newer_history,
        BranchRowSource::Active,
    );
    assert_visible_row(
        view.at_version(&history_key, CommitVersion::new(3))
            .expect("history getv")
            .as_ref(),
        &rewrite_row_branch(&retained_history, source, child).expect("history rewrite"),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn branch_materialization_retains_same_internal_key_when_row_facts_differ() {
    let source = branch_id(97);
    let child = branch_id(98);
    let key = b"materialized-same-version-timestamp".to_vec();
    let inherited_visible_at_timestamp = storage_row_with(
        source,
        key.clone(),
        4,
        30,
        Timestamp::EPOCH,
        b"inherited-visible-at-40".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materialize-same-version-source",
            vec![inherited_visible_at_timestamp.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let child_same_internal_key_later_timestamp = storage_row_with(
        child,
        key.clone(),
        4,
        50,
        Timestamp::EPOCH,
        b"child-hidden-at-40".to_vec(),
    );
    child_state
        .append_committed_row(child_same_internal_key_later_timestamp.clone())
        .expect("append child same internal key");

    let child_key = physical_key(child, key);
    let rewritten =
        rewrite_row_branch(&inherited_visible_at_timestamp, source, child).expect("rewrite");
    let before = child_state.capture_read_view().expect("before view");
    assert_visible_row(
        before
            .read_point(
                &child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            )
            .expect("before as-of")
            .as_ref(),
        &rewritten,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    let before_history = before
        .history(
            &child_key,
            BranchHistoryOptions::all().include_tombstones(true),
        )
        .expect("before history");
    assert_eq!(history_versions(&before_history), vec![4, 4]);
    assert_eq!(
        before_history[0].row(),
        &child_same_internal_key_later_timestamp
    );
    assert_eq!(before_history[1].row(), &rewritten);

    let outcome = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-same-version")
                .expect("request"),
        )
        .expect("materialize inherited layer");
    assert_eq!(outcome.rows_materialized(), 1);
    assert_eq!(outcome.skipped_exact_duplicate_rows(), 0);

    let after = child_state.capture_read_view().expect("after view");
    assert_visible_row(
        after
            .read_point(
                &child_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            )
            .expect("after as-of")
            .as_ref(),
        &rewritten,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    let after_history = after
        .history(
            &child_key,
            BranchHistoryOptions::all().include_tombstones(true),
        )
        .expect("after history");
    assert_eq!(history_versions(&after_history), vec![4, 4]);
    assert_eq!(
        after_history[0].row(),
        &child_same_internal_key_later_timestamp
    );
    assert_eq!(after_history[1].row(), &rewritten);
}

#[test]
#[allow(clippy::too_many_lines)]
fn branch_materialization_preserves_scans_tombstones_ttl_and_pinned_views() {
    let source = branch_id(99);
    let child = branch_id(100);
    let visible = storage_row_with(
        source,
        b"materialized-scan-a".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"visible".to_vec(),
    );
    let expired = storage_row_with(
        source,
        b"materialized-scan-expired".to_vec(),
        2,
        20,
        Timestamp::from_micros(25),
        b"expired".to_vec(),
    );
    let deleted_put = storage_row_with(
        source,
        b"materialized-scan-deleted".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"deleted-put".to_vec(),
    );
    let deleting_tombstone = tombstone_row(source, b"materialized-scan-deleted".to_vec(), 3, 30);
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(3),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materialize-scan-source",
            vec![
                visible.clone(),
                expired.clone(),
                deleted_put.clone(),
                deleting_tombstone.clone(),
            ],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let pinned = child_state.capture_read_view().expect("pinned before view");
    let visible_key = physical_key(child, b"materialized-scan-a".to_vec());
    let expired_key = physical_key(child, b"materialized-scan-expired".to_vec());
    let deleted_key = physical_key(child, b"materialized-scan-deleted".to_vec());
    let visible_rewritten = rewrite_row_branch(&visible, source, child).expect("visible rewrite");
    let expired_rewritten = rewrite_row_branch(&expired, source, child).expect("expired rewrite");

    let prefix = BranchScanBounds::prefix(&physical_key(child, b"materialized-scan-".to_vec()));
    let range = BranchScanBounds::closed(
        &physical_key(child, b"materialized-scan-a".to_vec()),
        &physical_key(child, b"materialized-scan-expired".to_vec()),
    )
    .expect("closed materialization range");
    assert_eq!(
        scan_user_keys(
            &pinned
                .scan_prefix(
                    &prefix,
                    BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
                )
                .expect("before timestamp prefix scan"),
        ),
        vec![b"materialized-scan-a".to_vec()]
    );
    assert_eq!(
        scan_user_keys(
            &pinned
                .scan_range(
                    &range,
                    BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
                )
                .expect("before timestamp range scan"),
        ),
        vec![b"materialized-scan-a".to_vec()]
    );

    let outcome = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-scan").expect("request"),
        )
        .expect("materialize inherited layer");
    assert_eq!(outcome.rows_materialized(), 4);
    assert_eq!(outcome.tables_created(), 1);
    assert_eq!(child_state.inherited_layer_count(), 0);

    assert_visible_row(
        pinned
            .read_point(
                &visible_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            )
            .expect("pinned visible")
            .as_ref(),
        &visible_rewritten,
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );

    let after = child_state.capture_read_view().expect("after view");
    assert_visible_row(
        after
            .read_point(
                &visible_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            )
            .expect("after visible")
            .as_ref(),
        &visible_rewritten,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_visible_row(
        after
            .read_point(
                &expired_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(24)),
            )
            .expect("expired before expiry")
            .as_ref(),
        &expired_rewritten,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert!(
        after
            .read_point(
                &expired_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
            )
            .expect("expired at expiry")
            .is_none(),
        "materialization must preserve TTL visibility without cleanup",
    );
    assert!(
        after
            .read_point(
                &deleted_key,
                BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            )
            .expect("deleted at timestamp")
            .is_none(),
        "materialized tombstone must keep suppressing older puts",
    );
    assert_eq!(
        history_versions(
            &after
                .history(
                    &deleted_key,
                    BranchHistoryOptions::all().include_tombstones(true),
                )
                .expect("deleted history"),
        ),
        vec![3, 1],
    );
    assert_eq!(
        scan_user_keys(
            &after
                .scan_prefix(
                    &prefix,
                    BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
                )
                .expect("after timestamp prefix scan"),
        ),
        vec![b"materialized-scan-a".to_vec()]
    );
    assert_eq!(
        scan_user_keys(
            &after
                .scan_range(
                    &range,
                    BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
                )
                .expect("after timestamp range scan"),
        ),
        vec![b"materialized-scan-a".to_vec()]
    );
}

#[test]
fn branch_materialization_splits_large_outputs_and_validates_identity_prefixes() {
    let source = branch_id(101);
    let child = branch_id(102);
    let rows = (0_u64..4_097)
        .map(|index| {
            storage_row_with(
                source,
                format!("materialized-split-{index:04}").into_bytes(),
                1,
                1,
                Timestamp::EPOCH,
                index.to_le_bytes().to_vec(),
            )
        })
        .collect::<Vec<_>>();
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(1),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materialize-split-source",
            rows,
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let outcome = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-split").expect("request"),
        )
        .expect("materialize split layer");
    assert_eq!(outcome.rows_materialized(), 4_097);
    assert_eq!(outcome.tables_created(), 2);
    assert_eq!(child_state.owned_levels()[0].len(), 2);
    assert_eq!(
        child_state.owned_levels()[0][0]
            .descriptor()
            .identity()
            .as_str(),
        "materialized-split-layer-0-table-0",
    );
    assert_eq!(
        child_state.owned_levels()[0][1]
            .descriptor()
            .identity()
            .as_str(),
        "materialized-split-layer-0-table-1",
    );

    assert!(matches!(
        BranchMaterializationRequest::new(child, 0, "bad/path"),
        Err(BranchRuntimeError::InvalidConfig {
            field: "output_identity_prefix",
            ..
        }),
    ));
    assert!(matches!(
        BranchMaterializationRequest::new(child, 0, "bad\0prefix"),
        Err(BranchRuntimeError::InvalidConfig {
            field: "output_identity_prefix",
            ..
        }),
    ));
}

#[test]
fn branch_materialization_rejects_bad_request_without_mutation() {
    let source = branch_id(103);
    let child = branch_id(104);
    let active_layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        Vec::new(),
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![active_layer])
        .expect("attach active layer");
    let before = child_state.clone();
    assert!(matches!(
        child_state.materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 1, "materialized-missing")
                .expect("missing request"),
        ),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. })
    ));
    assert_eq!(
        child_state, before,
        "missing layer materialization must not mutate state",
    );
    assert!(matches!(
        child_state.materialize_inherited_layer(
            &BranchMaterializationRequest::new(source, 0, "materialized-wrong-branch")
                .expect("wrong branch request"),
        ),
        Err(BranchRuntimeError::InvalidBranchState { .. })
    ));
    assert_eq!(
        child_state, before,
        "wrong-branch materialization must not mutate state",
    );
}

#[test]
fn branch_materialization_accepts_materializing_layer_status() {
    let source = branch_id(103);
    let child = branch_id(104);
    let materializing_row = storage_row_with(
        source,
        b"materializing-status".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"status".to_vec(),
    );
    let materializing_layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Materializing,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materializing-status-source",
            vec![materializing_row.clone()],
        )]],
    );
    let mut materializing_child = BranchLocalState::empty(child);
    materializing_child
        .attach_inherited_layers(vec![materializing_layer])
        .expect("attach materializing layer");
    let materializing_outcome = materializing_child
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-status")
                .expect("materializing request"),
        )
        .expect("materialize materializing layer");
    assert_eq!(
        materializing_outcome.recovery(),
        BranchMaterializationRecovery::ReplacementVisibleLayerRemoved,
    );
    assert_eq!(materializing_outcome.rows_materialized(), 1);
    assert_eq!(materializing_outcome.tables_created(), 1);
    assert_eq!(materializing_child.inherited_layer_count(), 0);
    let materialized_row =
        rewrite_row_branch(&materializing_row, source, child).expect("materializing rewrite");
    assert_visible_row(
        materializing_child
            .capture_read_view()
            .expect("materializing view")
            .latest(materialized_row.physical_key())
            .expect("materializing latest")
            .as_ref(),
        &materialized_row,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
}

#[test]
fn branch_materialization_rejects_unavailable_same_source_and_invalid_descriptors() {
    let source = branch_id(103);
    let child = branch_id(104);
    let unavailable = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Unavailable,
        Vec::new(),
    );
    let mut unavailable_child = BranchLocalState::empty(child);
    assert!(matches!(
        unavailable_child.attach_inherited_layers(vec![unavailable]),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. }),
    ));
    let same_branch_layer = branch_inherited_layer(
        child,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        Vec::new(),
    );
    let mut same_branch_child = BranchLocalState::empty(child);
    assert!(matches!(
        same_branch_child.attach_inherited_layers(vec![same_branch_layer]),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. }),
    ));

    let wrong_source_table = branch_owned_table(
        branch_id(105),
        BranchLevel::ZERO,
        "materialize-wrong-source-table",
        vec![storage_row_with(
            branch_id(105),
            b"wrong-source".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"wrong".to_vec(),
        )],
    );
    assert!(matches!(
        BranchInheritedLayer::new(
            InheritedLayerDescriptor::new(
                source,
                CommitVersion::new(5),
                InheritedLayerStatus::Active,
                1,
            ),
            vec![vec![wrong_source_table]],
        ),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. }),
    ));
    assert!(matches!(
        BranchInheritedLayer::new(
            InheritedLayerDescriptor::new(
                source,
                CommitVersion::new(5),
                InheritedLayerStatus::Active,
                1,
            ),
            Vec::new(),
        ),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. }),
    ));
}

#[test]
fn branch_materialization_preserves_edge_row_facts_and_table_facts() {
    let source = branch_id(106);
    let child = branch_id(107);
    let system_space = StorageSpaceId::engine(0x21).expect("system storage space");
    let empty_system_key = StorageRow::put(
        physical_key_with(source, "system", system_space, Vec::new()),
        CommitVersion::new(5),
        Timestamp::from_micros(55),
        Timestamp::MAX,
        Vec::new(),
    );
    let binary_key = StorageRow::put(
        physical_key(source, vec![0x00, 0x80, b'L', b'6', b'H']),
        CommitVersion::new(4),
        Timestamp::from_micros(44),
        Timestamp::from_micros(144),
        vec![0x00, 0xff],
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "materialize-edge-source",
            vec![empty_system_key.clone(), binary_key.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach edge layer");
    let outcome = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-edge")
                .expect("edge request"),
        )
        .expect("materialize edge rows");
    assert_eq!(outcome.rows_materialized(), 2);
    assert_eq!(outcome.tables_created(), 1);
    let table = &child_state.owned_levels()[0][0];
    assert_eq!(table.facts().row_count(), 2);
    assert_eq!(table.facts().commit_range().min(), CommitVersion::new(4),);
    assert_eq!(table.facts().commit_range().max(), CommitVersion::new(5),);
    assert!(!table.descriptor().identity().as_str().contains('/'));

    let system_rewritten =
        rewrite_row_branch(&empty_system_key, source, child).expect("system rewrite");
    let binary_rewritten = rewrite_row_branch(&binary_key, source, child).expect("binary rewrite");
    let view = child_state.capture_read_view().expect("edge view");
    assert_visible_row(
        view.latest(system_rewritten.physical_key())
            .expect("system latest")
            .as_ref(),
        &system_rewritten,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_visible_row(
        view.latest(binary_rewritten.physical_key())
            .expect("binary latest")
            .as_ref(),
        &binary_rewritten,
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_eq!(system_rewritten.physical_key().space(), "system");
    assert_eq!(
        system_rewritten.physical_key().storage_space_id(),
        system_space
    );
    assert!(system_rewritten.physical_key().user_key().is_empty());
    assert_eq!(system_rewritten.expires_at(), Timestamp::MAX);
    assert!(system_rewritten.value().is_empty());
    assert_eq!(
        binary_rewritten.physical_key().user_key(),
        &[0x00, 0x80, b'L', b'6', b'H']
    );
    assert_eq!(binary_rewritten.value(), &[0x00, 0xff]);
    assert_eq!(binary_rewritten.expires_at(), Timestamp::from_micros(144));
}

struct LayerOrderMaterializationFixture {
    nearest_source: BranchId,
    farther_source: BranchId,
    child: BranchId,
    child_key: PhysicalKey,
    nearest_duplicate: StorageRow,
    farther_history: StorageRow,
    child_state: BranchLocalState,
}

fn layer_order_materialization_fixture() -> LayerOrderMaterializationFixture {
    let nearest_source = branch_id(108);
    let farther_source = branch_id(109);
    let child = branch_id(110);
    let shared_key = b"materialized-layer-order".to_vec();
    let nearest_duplicate = storage_row_with(
        nearest_source,
        shared_key.clone(),
        4,
        40,
        Timestamp::EPOCH,
        b"shared".to_vec(),
    );
    let farther_duplicate = storage_row_with(
        farther_source,
        shared_key.clone(),
        4,
        40,
        Timestamp::EPOCH,
        b"shared".to_vec(),
    );
    let farther_history = storage_row_with(
        farther_source,
        shared_key.clone(),
        2,
        20,
        Timestamp::EPOCH,
        b"farther-history".to_vec(),
    );
    let nearest_layer = branch_inherited_layer(
        nearest_source,
        CommitVersion::new(4),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            nearest_source,
            BranchLevel::ZERO,
            "materialize-nearest-source",
            vec![nearest_duplicate.clone()],
        )]],
    );
    let farther_layer = branch_inherited_layer(
        farther_source,
        CommitVersion::new(4),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            farther_source,
            BranchLevel::ZERO,
            "materialize-farther-source",
            vec![farther_duplicate, farther_history.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![nearest_layer, farther_layer])
        .expect("attach ordered layers");
    LayerOrderMaterializationFixture {
        nearest_source,
        farther_source,
        child,
        child_key: physical_key(child, shared_key),
        nearest_duplicate,
        farther_history,
        child_state,
    }
}

#[test]
fn branch_materialization_preserves_layer_order_when_deep_layer_materialized_first() {
    let mut fixture = layer_order_materialization_fixture();
    let before = fixture
        .child_state
        .capture_read_view()
        .expect("before order view");
    assert_visible_row(
        before
            .latest(&fixture.child_key)
            .expect("before latest")
            .as_ref(),
        &rewrite_row_branch(
            &fixture.nearest_duplicate,
            fixture.nearest_source,
            fixture.child,
        )
        .expect("nearest rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: fixture.nearest_source,
            layer_index: 0,
        },
    );
    assert_visible_row(
        before
            .at_version(&fixture.child_key, CommitVersion::new(3))
            .expect("before historical")
            .as_ref(),
        &rewrite_row_branch(
            &fixture.farther_history,
            fixture.farther_source,
            fixture.child,
        )
        .expect("farther rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: fixture.farther_source,
            layer_index: 1,
        },
    );

    let farther_outcome = fixture
        .child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(fixture.child, 1, "materialized-farther")
                .expect("farther request"),
        )
        .expect("materialize farther first");
    assert_eq!(farther_outcome.rows_materialized(), 1);
    assert_eq!(farther_outcome.skipped_exact_duplicate_rows(), 1);
    assert_eq!(
        fixture.child_state.inherited_layers()[0].source_branch_id(),
        fixture.nearest_source
    );

    let after_farther = fixture
        .child_state
        .capture_read_view()
        .expect("after farther view");
    assert_visible_row(
        after_farther
            .latest(&fixture.child_key)
            .expect("after farther latest")
            .as_ref(),
        &rewrite_row_branch(
            &fixture.nearest_duplicate,
            fixture.nearest_source,
            fixture.child,
        )
        .expect("nearest rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: fixture.nearest_source,
            layer_index: 0,
        },
    );
    assert_visible_row(
        after_farther
            .at_version(&fixture.child_key, CommitVersion::new(3))
            .expect("after farther historical")
            .as_ref(),
        &rewrite_row_branch(
            &fixture.farther_history,
            fixture.farther_source,
            fixture.child,
        )
        .expect("farther rewrite"),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
}

#[test]
fn branch_materialization_preserves_nearest_and_history_after_all_layers_materialize() {
    let mut fixture = layer_order_materialization_fixture();
    fixture
        .child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(fixture.child, 1, "materialized-farther")
                .expect("farther request"),
        )
        .expect("materialize farther first");
    let nearest_outcome = fixture
        .child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(fixture.child, 0, "materialized-nearest")
                .expect("nearest request"),
        )
        .expect("materialize nearest");
    assert_eq!(nearest_outcome.rows_materialized(), 1);
    assert_eq!(nearest_outcome.skipped_exact_duplicate_rows(), 0);
    assert_eq!(fixture.child_state.inherited_layer_count(), 0);
    let after_all = fixture
        .child_state
        .capture_read_view()
        .expect("after all view");
    assert_visible_row(
        after_all
            .latest(&fixture.child_key)
            .expect("after all latest")
            .as_ref(),
        &rewrite_row_branch(
            &fixture.nearest_duplicate,
            fixture.nearest_source,
            fixture.child,
        )
        .expect("nearest rewrite"),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        },
    );
    assert_visible_row(
        after_all
            .at_version(&fixture.child_key, CommitVersion::new(3))
            .expect("after all historical")
            .as_ref(),
        &rewrite_row_branch(
            &fixture.farther_history,
            fixture.farther_source,
            fixture.child,
        )
        .expect("farther rewrite"),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 1,
        },
    );
}

#[test]
fn branch_materialization_handles_empty_and_already_materialized_layers() {
    let source = branch_id(95);
    let child = branch_id(96);
    let empty_layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        Vec::new(),
    );
    let mut empty_child = BranchLocalState::empty(child);
    empty_child
        .attach_inherited_layers(vec![empty_layer])
        .expect("attach empty inherited layer");
    let empty_outcome = empty_child
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-empty")
                .expect("empty request"),
        )
        .expect("materialize empty layer");
    assert_eq!(empty_outcome.rows_materialized(), 0);
    assert_eq!(empty_outcome.tables_created(), 0);
    assert_eq!(empty_outcome.inherited_layers_remaining(), 0);
    assert_eq!(empty_child.inherited_layer_count(), 0);
    assert_eq!(empty_child.owned_table_count(), 0);

    let materialized_layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Materialized,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "already-materialized-source",
            vec![storage_row_with(
                source,
                b"stale".to_vec(),
                5,
                50,
                Timestamp::EPOCH,
                b"stale".to_vec(),
            )],
        )]],
    );
    let mut materialized_child = BranchLocalState::empty(child);
    materialized_child
        .attach_inherited_layers(vec![materialized_layer])
        .expect("attach materialized layer");
    let before = materialized_child.clone();
    let materialized_outcome = materialized_child
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialized-stale")
                .expect("materialized request"),
        )
        .expect("already materialized no-op");
    assert_eq!(
        materialized_outcome.recovery(),
        BranchMaterializationRecovery::LayerAlreadyMaterialized,
    );
    assert_eq!(materialized_outcome.rows_materialized(), 0);
    assert_eq!(materialized_outcome.tables_created(), 0);
    assert_eq!(materialized_child, before);

    assert!(matches!(
        BranchMaterializationRequest::new(child, 0, ""),
        Err(BranchRuntimeError::InvalidConfig {
            field: "output_identity_prefix",
            ..
        })
    ));
}

#[test]
fn branch_inherited_history_filters_tombstones_limits_and_fork_gates() {
    let source = branch_id(89);
    let child = branch_id(90);
    let key = b"history-inherited".to_vec();
    let post_fork = storage_row_with(
        source,
        key.clone(),
        5,
        50,
        Timestamp::EPOCH,
        b"post-fork-secret".to_vec(),
    );
    let tombstone = tombstone_row(source, key.clone(), 4, 40);
    let visible = storage_row_with(
        source,
        key.clone(),
        3,
        30,
        Timestamp::from_micros(300),
        b"visible".to_vec(),
    );
    let older = storage_row_with(
        source,
        key.clone(),
        1,
        10,
        Timestamp::EPOCH,
        b"older".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(4),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "inherited-history",
            vec![post_fork, tombstone.clone(), visible.clone(), older.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited history layer");
    let view = child_state.capture_read_view().expect("view");
    let child_key = physical_key(child, key);

    assert!(
        view.latest(&child_key).expect("latest").is_none(),
        "selected inherited tombstone shadows older inherited puts"
    );

    let all = view
        .history(&child_key, BranchHistoryOptions::all())
        .expect("all inherited history");
    assert_eq!(history_versions(&all), vec![4, 3, 1]);
    assert!(all[0].row().is_tombstone());
    assert_eq!(
        all[1].source(),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        }
    );
    assert_eq!(
        all[1].row(),
        &rewrite_row_branch(&visible, source, child).expect("visible rewrite")
    );

    let without_tombstones = view
        .history(
            &child_key,
            BranchHistoryOptions::all().include_tombstones(false),
        )
        .expect("history without tombstones");
    assert_eq!(history_versions(&without_tombstones), vec![3, 1]);

    let before_fork = view
        .history(
            &child_key,
            BranchHistoryOptions::all().before_version(CommitVersion::new(4)),
        )
        .expect("history before tombstone");
    assert_eq!(history_versions(&before_fork), vec![3, 1]);

    let limited_after_filter = view
        .history(
            &child_key,
            BranchHistoryOptions::all()
                .include_tombstones(false)
                .limit(1),
        )
        .expect("limited inherited history");
    assert_eq!(history_versions(&limited_after_filter), vec![3]);
    assert_eq!(
        limited_after_filter[0].row(),
        &rewrite_row_branch(&visible, source, child).expect("limited rewrite")
    );
    assert!(
        history_versions(&all).iter().all(|version| *version <= 4),
        "rows above the fork version must stay out of history"
    );
}

#[test]
fn branch_inherited_l0_overlap_and_l1_tables_participate_in_point_reads() {
    let source = branch_id(91);
    let child = branch_id(92);
    let overlapping_key = b"overlap".to_vec();
    let old_l0 = storage_row_with(
        source,
        overlapping_key.clone(),
        2,
        20,
        Timestamp::EPOCH,
        b"old-l0".to_vec(),
    );
    let new_l0 = storage_row_with(
        source,
        overlapping_key.clone(),
        5,
        50,
        Timestamp::EPOCH,
        b"new-l0".to_vec(),
    );
    let l1_row = storage_row_with(
        source,
        b"from-l1".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"l1".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![
            vec![
                branch_owned_table(
                    source,
                    BranchLevel::ZERO,
                    "inherited-overlap-l0-new",
                    vec![new_l0.clone()],
                ),
                branch_owned_table(
                    source,
                    BranchLevel::ZERO,
                    "inherited-overlap-l0-old",
                    vec![old_l0],
                ),
            ],
            vec![branch_owned_table(
                source,
                BranchLevel::new(1),
                "inherited-l1-point",
                vec![l1_row.clone()],
            )],
        ],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited overlap layer");
    let view = child_state.capture_read_view().expect("view");

    assert_visible_row(
        view.latest(&physical_key(child, overlapping_key))
            .expect("overlap latest")
            .as_ref(),
        &rewrite_row_branch(&new_l0, source, child).expect("new L0 rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
    assert_visible_row(
        view.latest(&physical_key(child, b"from-l1".to_vec()))
            .expect("L1 latest")
            .as_ref(),
        &rewrite_row_branch(&l1_row, source, child).expect("L1 rewrite"),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        },
    );
}

#[test]
fn branch_child_owned_table_shadows_inherited_exact_duplicate_key() {
    let source = branch_id(81);
    let child = branch_id(82);
    let source_row = storage_row_with(
        source,
        b"exact-duplicate".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"source".to_vec(),
    );
    let child_row = storage_row_with(
        child,
        b"exact-duplicate".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"child-owned".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "exact-duplicate-inherited",
            vec![source_row],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited");
    child_state
        .install_l0_table(branch_owned_table(
            child,
            BranchLevel::ZERO,
            "exact-duplicate-owned",
            vec![child_row.clone()],
        ))
        .expect("install child-owned shadow");

    let visible = child_state
        .capture_read_view()
        .expect("view")
        .latest(&physical_key(child, b"exact-duplicate".to_vec()))
        .expect("latest")
        .expect("child-owned row");
    assert_eq!(visible.row(), &child_row);
    assert_eq!(
        visible.source(),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        }
    );
}

#[test]
fn branch_inherited_scans_and_history_rewrite_before_grouping() {
    let source = branch_id(76);
    let child = branch_id(77);
    let source_put = storage_row_with(
        source,
        b"scan-a".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"source".to_vec(),
    );
    let source_tombstone = StorageRow::tombstone(
        physical_key(source, b"scan-b".to_vec()),
        CommitVersion::new(4),
        Timestamp::from_micros(40),
    );
    let child_put = storage_row_with(
        child,
        b"scan-a".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"child".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(4),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "scan-inherited",
            vec![source_put.clone(), source_tombstone.clone()],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited");
    child_state
        .append_committed_row(child_put.clone())
        .expect("child put");

    let view = child_state.capture_read_view().expect("view");
    let bounds = BranchScanBounds::prefix(&physical_key(child, b"scan-".to_vec()));
    let scan = view
        .scan_prefix(&bounds, BranchReadBound::latest())
        .expect("scan");
    assert_eq!(scan_user_keys(&scan), vec![b"scan-a".to_vec()]);
    assert_eq!(scan[0].row(), &child_put);
    assert_eq!(scan[0].source(), BranchRowSource::Active);

    let history = view
        .history(
            &physical_key(child, b"scan-a".to_vec()),
            BranchHistoryOptions::all(),
        )
        .expect("history");
    assert_eq!(history_versions(&history), vec![5, 2]);
    assert_eq!(history[0].source(), BranchRowSource::Active);
    assert_eq!(
        history[1].source(),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        }
    );
    assert_eq!(
        history[1].row(),
        &rewrite_row_branch(&source_put, source, child).expect("source rewrite")
    );
}

#[test]
fn branch_inherited_scans_preserve_space_boundaries() {
    let fixture = inherited_scan_boundary_fixture();
    let closed = fixture
        .view
        .scan_range(
            &BranchScanBounds::closed(&fixture.lower, &fixture.upper).expect("closed bounds"),
            BranchReadBound::latest(),
        )
        .expect("closed inherited scan");
    assert_eq!(
        scan_user_keys(&closed),
        vec![b"scan-a".to_vec(), b"scan-b".to_vec(), b"scan-c".to_vec()]
    );
    assert!(closed.iter().all(|row| {
        row.row().physical_key().space() == "default"
            && row.row().physical_key().storage_space_id() == fixture.engine_space
    }));
    let system_rows = fixture
        .view
        .scan_prefix(
            &BranchScanBounds::prefix(&physical_key_with(
                fixture.child,
                "system",
                fixture.engine_space,
                b"scan-".to_vec(),
            )),
            BranchReadBound::latest(),
        )
        .expect("system prefix scan");
    assert_eq!(system_rows.len(), 1);
    assert_eq!(system_rows[0].row().physical_key().space(), "system");
    assert_eq!(
        system_rows[0].row().physical_key().storage_space_id(),
        fixture.engine_space
    );
    assert_eq!(system_rows[0].row().physical_key().user_key(), b"scan-b");
}

#[test]
fn branch_inherited_scans_preserve_range_edges() {
    let fixture = inherited_scan_boundary_fixture();
    let open = fixture
        .view
        .scan_range(
            &BranchScanBounds::open(&fixture.lower, &fixture.upper).expect("open bounds"),
            BranchReadBound::latest(),
        )
        .expect("open inherited scan");
    assert_eq!(scan_user_keys(&open), vec![b"scan-b".to_vec()]);
    assert_eq!(
        open[0].row(),
        &rewrite_row_branch(&fixture.scan_b, fixture.source, fixture.child)
            .expect("middle rewrite")
    );

    let closed_degenerate = fixture
        .view
        .scan_range(
            &BranchScanBounds::closed(&fixture.middle, &fixture.middle).expect("closed degenerate"),
            BranchReadBound::latest(),
        )
        .expect("closed degenerate scan");
    assert_eq!(scan_user_keys(&closed_degenerate), vec![b"scan-b".to_vec()]);

    let open_degenerate = fixture
        .view
        .scan_range(
            &BranchScanBounds::open(&fixture.middle, &fixture.middle).expect("open degenerate"),
            BranchReadBound::latest(),
        )
        .expect("open degenerate scan");
    assert!(open_degenerate.is_empty());
}

#[test]
fn branch_inherited_rejects_wrong_branch_before_timestamp_reads_without_payload() {
    let source = branch_id(95);
    let child = branch_id(96);
    let row = storage_row_with(
        source,
        b"reject".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"secret-payload".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(2),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "inherited-reject-secret",
            vec![row],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");
    let view = child_state.capture_read_view().expect("view");

    let wrong_branch_error = view
        .latest(&physical_key(branch_id(97), b"reject".to_vec()))
        .expect_err("wrong branch rejected before inherited lookup");
    assert!(matches!(
        wrong_branch_error,
        BranchRuntimeError::InvalidBranchRow { .. }
    ));
    assert!(!wrong_branch_error.to_string().contains("secret-payload"));

    let timestamp_row = view
        .read_point(
            &physical_key(child, b"reject".to_vec()),
            BranchReadBound::at_timestamp(Timestamp::from_micros(20)),
        )
        .expect("timestamp inherited read")
        .expect("inherited timestamp row");
    assert_eq!(
        timestamp_row.row(),
        &rewrite_row_branch(
            &storage_row_with(
                source,
                b"reject".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"secret-payload".to_vec(),
            ),
            source,
            child,
        )
        .expect("expected rewrite")
    );
}

#[test]
fn branch_chained_fork_prefers_nearest_inherited_layer_for_exact_ties() {
    let grandparent = branch_id(78);
    let parent = branch_id(79);
    let child = branch_id(80);
    let key = b"tie".to_vec();
    let grandparent_row = storage_row_with(
        grandparent,
        key.clone(),
        5,
        50,
        Timestamp::EPOCH,
        b"grandparent".to_vec(),
    );
    let parent_row = storage_row_with(
        parent,
        key.clone(),
        5,
        50,
        Timestamp::EPOCH,
        b"parent".to_vec(),
    );

    let mut grandparent_state = BranchLocalState::empty(grandparent);
    grandparent_state
        .install_l0_table(branch_owned_table(
            grandparent,
            BranchLevel::ZERO,
            "grandparent-tie",
            vec![grandparent_row],
        ))
        .expect("grandparent install");
    let (mut parent_state, _) = grandparent_state
        .fork_into_empty_child(parent)
        .expect("fork parent");
    parent_state
        .install_l0_table(branch_owned_table(
            parent,
            BranchLevel::ZERO,
            "parent-tie",
            vec![parent_row.clone()],
        ))
        .expect("parent install");

    let (child_state, outcome) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    assert_eq!(outcome.inherited_layer_count(), 2);
    let view = child_state.capture_read_view().expect("view");
    let visible = view
        .latest(&physical_key(child, key))
        .expect("latest")
        .expect("nearest inherited row");
    assert_eq!(
        visible.row(),
        &rewrite_row_branch(&parent_row, parent, child).expect("parent rewrite")
    );
    assert_eq!(
        visible.source(),
        BranchRowSource::Inherited {
            source_branch_id: parent,
            layer_index: 0,
        }
    );
}

struct ForkStatusFixture {
    grandparent: BranchId,
    source: BranchId,
    child: BranchId,
    source_owned: StorageRow,
    inherited_row: StorageRow,
    child_state: BranchLocalState,
    outcome: BranchForkOutcome,
}

fn fork_status_fixture() -> ForkStatusFixture {
    let grandparent = branch_id(83);
    let materialized_source = branch_id(84);
    let source = branch_id(85);
    let child = branch_id(86);
    let inherited_row = storage_row_with(
        grandparent,
        b"materializing".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"grandparent".to_vec(),
    );
    let source_owned = storage_row_with(
        source,
        b"source-owned".to_vec(),
        6,
        60,
        Timestamp::EPOCH,
        b"source".to_vec(),
    );
    let materializing = branch_inherited_layer(
        grandparent,
        CommitVersion::new(3),
        InheritedLayerStatus::Materializing,
        vec![vec![branch_owned_table(
            grandparent,
            BranchLevel::ZERO,
            "fork-materializing",
            vec![inherited_row.clone()],
        )]],
    );
    let materialized = branch_inherited_layer(
        materialized_source,
        CommitVersion::new(2),
        InheritedLayerStatus::Materialized,
        vec![vec![branch_owned_table(
            materialized_source,
            BranchLevel::ZERO,
            "fork-materialized",
            vec![storage_row_with(
                materialized_source,
                b"materialized".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"old-materialized".to_vec(),
            )],
        )]],
    );
    let mut source_state = BranchLocalState::empty(source);
    source_state
        .attach_inherited_layers(vec![materializing, materialized])
        .expect("attach source inherited layers");
    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "fork-source-owned-status",
            vec![source_owned.clone()],
        ))
        .expect("install source-owned table after inherited attach");
    let (child_state, outcome) = source_state
        .fork_into_empty_child(child)
        .expect("fork child with inherited layers");
    ForkStatusFixture {
        grandparent,
        source,
        child,
        source_owned,
        inherited_row,
        child_state,
        outcome,
    }
}

struct InheritedScanBoundaryFixture {
    source: BranchId,
    child: BranchId,
    engine_space: StorageSpaceId,
    view: BranchReadView,
    lower: PhysicalKey,
    middle: PhysicalKey,
    upper: PhysicalKey,
    scan_b: StorageRow,
}

fn inherited_scan_boundary_fixture() -> InheritedScanBoundaryFixture {
    let source = branch_id(93);
    let child = branch_id(94);
    let engine_space = StorageSpaceId::engine(0x21).expect("engine space");
    let system_space = StorageSpaceId::COMMIT_TIMELINE;
    let scan_a = storage_row_with_named_space(
        source,
        "default",
        engine_space,
        b"scan-a".to_vec(),
        2,
        20,
        b"a".to_vec(),
    );
    let scan_b = storage_row_with_named_space(
        source,
        "default",
        engine_space,
        b"scan-b".to_vec(),
        3,
        30,
        b"b".to_vec(),
    );
    let scan_c = storage_row_with_named_space(
        source,
        "default",
        engine_space,
        b"scan-c".to_vec(),
        4,
        40,
        b"c".to_vec(),
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(6),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "inherited-scan-boundaries",
            vec![
                scan_a,
                scan_b.clone(),
                scan_c,
                storage_row_with_named_space(
                    source,
                    "system",
                    engine_space,
                    b"scan-b".to_vec(),
                    5,
                    50,
                    b"wrong-name".to_vec(),
                ),
                storage_row_with_named_space(
                    source,
                    "default",
                    system_space,
                    b"scan-b".to_vec(),
                    6,
                    60,
                    b"wrong-storage".to_vec(),
                ),
            ],
        )]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited scan layer");
    InheritedScanBoundaryFixture {
        source,
        child,
        engine_space,
        view: child_state.capture_read_view().expect("view"),
        lower: physical_key_with(child, "default", engine_space, b"scan-a".to_vec()),
        middle: physical_key_with(child, "default", engine_space, b"scan-b".to_vec()),
        upper: physical_key_with(child, "default", engine_space, b"scan-c".to_vec()),
        scan_b,
    }
}

fn assert_duplicate_internal_key(error: &BranchRuntimeError) {
    assert!(matches!(
        error,
        BranchRuntimeError::TableRuntime {
            source: crate::table::TableRuntimeError::DuplicateInternalKey { .. },
        }
    ));
    assert!(error.source().is_some());
    assert!(!error.to_string().contains("secret-payload"));
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn table_facts(identity: &str) -> TableRuntimeFacts {
    TableRuntimeFacts::new(
        TableIdentity::new(identity).expect("identity"),
        2,
        1,
        TableKeyRange::new(vec![0x01], vec![0x02]).expect("key range"),
        TableCommitRange::new(CommitVersion::new(1), CommitVersion::new(2)).expect("commit range"),
        128,
    )
    .expect("table facts")
}

fn branch_owned_table(
    branch: BranchId,
    level: BranchLevel,
    identity: &str,
    rows: Vec<StorageRow>,
) -> BranchOwnedTable {
    let reader = immutable_reader(identity, rows);
    let descriptor = branch_table_descriptor(level, &reader);
    BranchOwnedTable::new(branch, descriptor, reader).expect("branch-owned table")
}

fn branch_inherited_layer(
    source: BranchId,
    fork_version: CommitVersion,
    status: InheritedLayerStatus,
    owned_levels: Vec<Vec<BranchOwnedTable>>,
) -> BranchInheritedLayer {
    let table_count = owned_levels.iter().map(Vec::len).sum();
    BranchInheritedLayer::new(
        InheritedLayerDescriptor::new(source, fork_version, status, table_count),
        owned_levels,
    )
    .expect("branch inherited layer")
}

fn branch_table_descriptor(
    level: BranchLevel,
    reader: &ImmutableTableReader,
) -> BranchTableDescriptor {
    BranchTableDescriptor::new(
        reader.facts().identity().clone(),
        reader.facts().clone(),
        level,
    )
    .expect("branch table descriptor")
}

fn immutable_reader(identity: &str, rows: Vec<StorageRow>) -> ImmutableTableReader {
    let mut rows = rows.into_iter().map(TableRow::new).collect::<Vec<_>>();
    sort_table_rows_by_key(&mut rows);
    let identity = TableIdentity::new(identity).expect("identity");
    let builder = ImmutableTableBuilder::new(TableBuilderConfig::default()).expect("builder");
    let artifact = builder
        .build_from_rows(identity.clone(), &rows)
        .expect("built table");
    ImmutableTableReader::open_bytes(
        identity,
        artifact.into_bytes(),
        TableReaderConfig::default(),
    )
    .expect("immutable table reader")
}

fn row_versions(rows: &[TableRow]) -> Vec<u64> {
    rows.iter()
        .map(|row| row.commit_version().as_u64())
        .collect()
}

fn matching_versions(rows: &[TableRow], bound: BranchEffectiveReadBound) -> Vec<u64> {
    rows.iter()
        .filter(|row| bound.matches_row(row.row()).matches_effective_bound())
        .map(|row| row.commit_version().as_u64())
        .collect()
}

fn history_versions(rows: &[BranchHistoryRow]) -> Vec<u64> {
    rows.iter()
        .map(|row| row.row().commit_version().as_u64())
        .collect()
}

fn scan_user_keys(rows: &[BranchVisibleRow]) -> Vec<Vec<u8>> {
    rows.iter()
        .map(|row| row.row().physical_key().user_key().to_vec())
        .collect()
}

fn assert_visible_row(
    actual: Option<&BranchVisibleRow>,
    expected_row: &StorageRow,
    expected_source: BranchRowSource,
) {
    let actual = actual.expect("visible row");
    assert_eq!(actual.row(), expected_row);
    assert_eq!(actual.source(), expected_source);
}

fn storage_row(branch_id: BranchId, version: u64) -> StorageRow {
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
) -> StorageRow {
    StorageRow::put(
        physical_key(branch_id, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(timestamp),
        expires_at,
        value,
    )
}

fn storage_row_with_named_space(
    branch_id: BranchId,
    space_name: &str,
    storage_space_id: StorageSpaceId,
    user_key: Vec<u8>,
    version: u64,
    timestamp: u64,
    value: Vec<u8>,
) -> StorageRow {
    StorageRow::put(
        physical_key_with(branch_id, space_name, storage_space_id, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(timestamp),
        Timestamp::EPOCH,
        value,
    )
}

fn tombstone_row(
    branch_id: BranchId,
    user_key: Vec<u8>,
    version: u64,
    timestamp: u64,
) -> StorageRow {
    StorageRow::tombstone(
        physical_key(branch_id, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(timestamp),
    )
}

fn physical_key(branch_id: BranchId, user_key: Vec<u8>) -> PhysicalKey {
    let space = StorageSpaceId::engine(0x20).expect("engine storage space");
    physical_key_with(branch_id, "default", space, user_key)
}

fn physical_key_with(
    branch_id: BranchId,
    space_name: &str,
    storage_space_id: StorageSpaceId,
    user_key: Vec<u8>,
) -> PhysicalKey {
    PhysicalKey::new(branch_id, space_name, storage_space_id, user_key).expect("physical key")
}

#[derive(Debug)]
struct LeafError;

impl fmt::Display for LeafError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("leaf source")
    }
}

impl Error for LeafError {}
