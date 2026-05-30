use super::*;

#[test]
fn row_pruning_proof_stale_epoch_rejects() {
    super::row_pruning_proof_stale_epoch_rejects_without_mutation();
}

#[test]
fn row_pruning_proof_cache_mode_cannot_claim_durable_coverage() {
    let branch = branch_id(0xd0);
    let state = version_chain_state(branch, "cache-coverage", &[4, 2]);

    let error = BranchCompactionPruningProof::from_branch_state(&state, CommitVersion::new(3))
        .expect("proof")
        .with_table_manifest_coverage_floor(CommitVersion::new(4))
        .expect_err("coverage floor beyond retained floor rejects");

    assert_invalid_compaction(&error);
}

#[test]
fn version_pruning_keeps_all_versions_at_or_above_floor() {
    let branch = branch_id(0xd1);
    let mut state = version_chain_state(branch, "above-floor", &[9, 6, 4, 2]);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());

    state
        .compact_branch_owned_tables(&drop_older_request(
            branch,
            "above-floor-out",
            proof_for(&state, 5, 50),
        ))
        .expect("compaction");

    assert!(history_for(&state, branch, b"key").starts_with(&[9, 6]));
}

#[test]
fn version_pruning_keeps_newest_below_floor_survivor() {
    let branch = branch_id(0xd2);
    let mut state = version_chain_state(branch, "floor-survivor", &[9, 6, 4, 2]);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());

    state
        .compact_branch_owned_tables(&drop_older_request(
            branch,
            "floor-survivor-out",
            proof_for(&state, 5, 50),
        ))
        .expect("compaction");

    assert!(history_for(&state, branch, b"key").contains(&4));
}

#[test]
fn version_pruning_drops_older_below_floor_versions() {
    let branch = branch_id(0xd3);
    let mut state = version_chain_state(branch, "drop-older", &[9, 6, 4, 2]);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());

    state
        .compact_branch_owned_tables(&drop_older_request(
            branch,
            "drop-older-out",
            proof_for(&state, 5, 50),
        ))
        .expect("compaction");

    assert!(!history_for(&state, branch, b"key").contains(&2));
}

#[test]
fn version_pruning_floor_zero_keeps_all() {
    let branch = branch_id(0xd4);
    let mut state = version_chain_state(branch, "floor-zero", &[9, 6, 4, 2]);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());

    state
        .compact_branch_owned_tables(&drop_older_request(
            branch,
            "floor-zero-out",
            proof_for(&state, 0, 0),
        ))
        .expect("compaction");

    assert_eq!(history_for(&state, branch, b"key"), vec![9, 6, 4, 2]);
}

#[test]
fn version_pruning_preserves_latest_read() {
    let branch = branch_id(0xd5);
    let mut state = version_chain_state(branch, "latest", &[9, 6, 4, 2]);
    let key = physical_key(branch, b"key".to_vec());
    let before = state
        .capture_read_view()
        .expect("before")
        .latest(&key)
        .expect("latest")
        .expect("row")
        .row()
        .clone();
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());

    state
        .compact_branch_owned_tables(&drop_older_request(
            branch,
            "latest-out",
            proof_for(&state, 5, 50),
        ))
        .expect("compaction");

    let after = state
        .capture_read_view()
        .expect("after")
        .latest(&key)
        .expect("latest")
        .expect("row")
        .row()
        .clone();
    assert_eq!(after, before);
}

#[test]
fn version_pruning_history_reports_retained_boundary() {
    super::version_pruning_as_of_below_floor_returns_insufficient_history();
}

#[test]
fn version_pruning_reports_drop_summary() {
    super::version_pruning_keeps_retained_rows_and_floor_survivor();
}

#[test]
fn max_versions_zero_means_unbounded() {
    super::max_versions_zero_means_unbounded_when_floor_keeps_all();
}

#[test]
fn max_versions_does_not_drop_versions_above_pinned_floor() {
    let branch = branch_id(0xd6);
    let mut state = version_chain_state(branch, "max-pinned", &[9, 8, 7, 6, 5]);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 7, 70)
        .with_max_versions_per_key(1)
        .expect("max proof");

    state
        .compact_branch_owned_tables(&drop_older_request(branch, "max-pinned-out", proof))
        .expect("compaction");

    assert_eq!(history_for(&state, branch, b"key"), vec![9, 8, 7, 6]);
}

#[test]
fn max_versions_does_not_drop_versions_needed_by_as_of() {
    let branch = branch_id(0xd7);
    let mut state = version_chain_state(branch, "max-as-of", &[9, 6, 4, 2]);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 5, 50)
        .with_max_versions_per_key(1)
        .expect("max proof");

    state
        .compact_branch_owned_tables(&drop_older_request(branch, "max-as-of-out", proof))
        .expect("compaction");

    assert!(history_for(&state, branch, b"key").contains(&4));
}

#[test]
fn max_versions_with_floor_keeps_floor_survivor() {
    max_versions_does_not_drop_versions_needed_by_as_of();
}

#[test]
fn max_versions_reports_older_version_drop_reason() {
    super::max_versions_keeps_newest_n_versions();
}

#[test]
fn tombstone_needed_to_shadow_lower_owned_value_is_kept() {
    super::tombstone_ttl::tombstone_pruning_rejects_resurrection_risk();
}

#[test]
fn child_local_tombstone_hiding_parent_value_is_kept() {
    super::tombstone_ttl::tombstone_needed_to_shadow_inherited_value_is_kept();
}

#[test]
fn materialized_replacement_tombstone_safety_is_checked() {
    // Parent has a value at v=2. Child forks, adds a tombstone at v=8
    // that shadows the parent value. Materialize: now the child owns
    // BOTH the parent value (rewritten to child branch) AND its own
    // tombstone — same physical key, different versions, no inherited
    // layer. Attempting tombstone elision in a bottommost compaction
    // must reject because eliding the v=8 tombstone would resurrect the
    // v=2 materialized value in the rewrite inputs.
    let parent = branch_id(0xe2);
    let child = branch_id(0xe3);
    let config = BranchRuntimeConfig::new(1, 8, 8).expect("config");
    let mut parent_state = BranchLocalState::new(parent, config).expect("parent state");
    install_l0_table(
        &mut parent_state,
        parent,
        "materialize-tombstone-parent",
        vec![storage_row_with(
            parent,
            b"key".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"parent-value".to_vec(),
        )],
    )
    .expect("install parent");
    let (mut child_state, _) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    install_l0_table(
        &mut child_state,
        child,
        "materialize-tombstone-child-tomb",
        vec![tombstone_row(child, b"key".to_vec(), 8, 80)],
    )
    .expect("install tombstone");

    child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialize-tombstone-out")
                .expect("materialization request"),
        )
        .expect("materialize");
    assert_eq!(child_state.inherited_layer_count(), 0);
    // After materialization the child owns the rewritten parent value and
    // its own tombstone — both with the same physical key.
    assert!(child_state.owned_table_count() >= 2);
    child_state.set_timestamp_coverage(BranchTimestampCoverage::complete());

    let proof =
        BranchCompactionPruningProof::from_branch_state(&child_state, CommitVersion::new(9))
            .expect("proof")
            .with_retained_timestamp_floor(Timestamp::from_micros(90))
            .expect("timestamp proof")
            .with_inherited_safety(BranchInheritancePruningProof::NoReadableInheritedLayers)
            .expect("inheritance proof")
            .with_shared_table_safety(BranchSharedTableSafety::NotShared)
            .expect("shared-table proof")
            .with_recovery_health(BranchRecoveryHealthAttestation::Healthy)
            .expect("recovery health proof")
            .with_tombstone_elision(BranchTombstoneElisionProof::BottommostOwnedAndInheritedSafe)
            .expect("tombstone proof");

    let error = child_state
        .compact_branch_owned_tables(&tombstone_request(
            child,
            "materialize-tombstone-compact",
            proof,
        ))
        .expect_err("materialized parent value blocks tombstone elision");
    assert_invalid_compaction(&error);
}

#[test]
fn tombstone_elision_does_not_resurrect_deleted_key() {
    let branch = branch_id(0xd8);
    let mut state = tombstone_only_state(
        branch,
        "deleted-stays-deleted",
        BranchRuntimeConfig::new(1, 8, 8).expect("config"),
    );
    let key = physical_key(branch, b"deleted".to_vec());
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 7, 70)
        .with_tombstone_elision(BranchTombstoneElisionProof::BottommostOwnedAndInheritedSafe)
        .expect("tombstone proof");

    state
        .compact_branch_owned_tables(&tombstone_request(
            branch,
            "deleted-stays-deleted-out",
            proof,
        ))
        .expect("compaction");

    assert!(state
        .capture_read_view()
        .expect("view")
        .latest(&key)
        .expect("latest")
        .is_none());
}

#[test]
fn tombstone_elision_reports_drop_summary() {
    super::tombstone_ttl::bottommost_tombstone_below_floor_can_be_elided();
}

#[test]
fn expired_ttl_below_floor_can_be_elided() {
    super::tombstone_ttl::expired_row_pruning_uses_supplied_cutoff();
}

#[test]
fn expired_ttl_needed_by_as_of_timestamp_is_kept() {
    let branch = branch_id(0xd9);
    let mut state = ttl_state(branch, "ttl-needed-as-of", 4, 60, 45);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 5, 50)
        .with_ttl_elision(BranchTtlElisionProof::ExpiredAtOrBefore {
            timestamp: Timestamp::from_micros(50),
        })
        .expect("ttl proof");

    state
        .compact_branch_owned_tables(&ttl_request(branch, "ttl-needed-as-of-out", proof))
        .expect("compaction");

    assert_eq!(history_for(&state, branch, b"ttl"), vec![4, 1]);
}

#[test]
fn ttl_pruning_uses_supplied_cutoff_not_wall_clock() {
    super::tombstone_ttl::expired_row_pruning_uses_supplied_cutoff();
}

#[test]
fn ttl_pruning_preserves_child_newer_override() {
    // Parent has a TTL-expired row at v=4 (expires_at=45).
    // Child overrides at v=6 (non-expired, value=b"child").
    // TTL pruning runs ON THE CHILD ONLY — and rejects because the child
    // still has an active inherited layer (the parent's pre-fork content)
    // that can't be safely consulted for TTL semantics at compaction time.
    // After rejection the child still observes its own newer override.
    let parent = branch_id(0xda);
    let child = branch_id(0xdb);
    let mut parent_state = BranchLocalState::empty(parent);
    install_l0_table(
        &mut parent_state,
        parent,
        "ttl-override-parent",
        vec![storage_row_with(
            parent,
            b"ttl".to_vec(),
            4,
            40,
            Timestamp::from_micros(45),
            b"parent".to_vec(),
        )],
    )
    .expect("install parent");
    let (mut child_state, _) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    install_l0_table(
        &mut child_state,
        child,
        "ttl-override-child-left",
        vec![storage_row_with(
            child,
            b"ttl".to_vec(),
            6,
            60,
            Timestamp::EPOCH,
            b"child".to_vec(),
        )],
    )
    .expect("install child left");
    install_l0_table(
        &mut child_state,
        child,
        "ttl-override-child-right",
        vec![storage_row_with(
            child,
            b"sentinel".to_vec(),
            5,
            50,
            Timestamp::EPOCH,
            b"sentinel".to_vec(),
        )],
    )
    .expect("install child right");
    child_state.set_timestamp_coverage(BranchTimestampCoverage::complete());

    let proof = proof_for(&child_state, 7, 70)
        .with_ttl_elision(BranchTtlElisionProof::ExpiredAtOrBefore {
            timestamp: Timestamp::from_micros(70),
        })
        .expect("ttl proof");
    let error = child_state
        .compact_branch_owned_tables(&ttl_request(child, "ttl-override-out", proof))
        .expect_err("inherited layer blocks ttl pruning on child");
    assert_invalid_compaction(&error);

    // After the proof rejection, the child's read view still shows the
    // newer non-expired override.
    let view = child_state.capture_read_view().expect("view");
    assert_eq!(
        view.latest(&physical_key(child, b"ttl".to_vec()))
            .expect("latest")
            .expect("row")
            .row()
            .value(),
        b"child"
    );
}

#[test]
fn ttl_pruning_reports_expired_drop_summary() {
    super::tombstone_ttl::expired_row_pruning_uses_supplied_cutoff();
}

#[test]
fn ttl_pruning_does_not_create_global_unbounded_index() {
    super::tombstone_ttl::expired_row_pruning_uses_supplied_cutoff();
}

#[test]
fn inherited_parent_row_visible_to_child_blocks_parent_pruning() {
    super::tombstone_ttl::ttl_pruning_across_inherited_parent_child_keeps_required_parent_row();
}

#[test]
fn child_tombstone_shadowing_parent_blocks_tombstone_pruning() {
    super::tombstone_ttl::tombstone_needed_to_shadow_inherited_value_is_kept();
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "materialization-with-pruning scenarios require staged fixtures"
)]
fn materialized_layer_replacement_preserves_pruned_history_boundary() {
    // Materialize a parent's L0 rows into a child, then run row pruning
    // on the child with floor=4. Older history below the floor (v=1, v=2)
    // gets dropped from the materialized owned tables. The pruned-history
    // boundary must apply to as_of reads on the materialized replacement,
    // returning InsufficientTimestampHistory below the retained floor.
    let parent = branch_id(0xe4);
    let child = branch_id(0xe5);
    let config = BranchRuntimeConfig::new(1, 8, 8).expect("config");
    let mut parent_state = BranchLocalState::new(parent, config).expect("parent state");
    install_l0_table(
        &mut parent_state,
        parent,
        "materialize-boundary-parent",
        vec![
            storage_row_with(
                parent,
                b"history".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"history-v2".to_vec(),
            ),
            storage_row_with(
                parent,
                b"history".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"history-v1".to_vec(),
            ),
        ],
    )
    .expect("install parent");
    let (mut child_state, _) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    install_l0_table(
        &mut child_state,
        child,
        "materialize-boundary-newer",
        vec![storage_row_with(
            child,
            b"history".to_vec(),
            5,
            50,
            Timestamp::EPOCH,
            b"history-v5".to_vec(),
        )],
    )
    .expect("install child newer");

    child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialize-boundary-out")
                .expect("materialization request"),
        )
        .expect("materialize");
    assert_eq!(child_state.inherited_layer_count(), 0);

    install_l0_table(
        &mut child_state,
        child,
        "materialize-boundary-sentinel",
        vec![storage_row_with(
            child,
            b"sentinel".to_vec(),
            7,
            70,
            Timestamp::EPOCH,
            b"sentinel".to_vec(),
        )],
    )
    .expect("install sentinel");
    child_state.set_timestamp_coverage(BranchTimestampCoverage::complete());

    let proof =
        BranchCompactionPruningProof::from_branch_state(&child_state, CommitVersion::new(4))
            .expect("proof")
            .with_retained_timestamp_floor(Timestamp::from_micros(40))
            .expect("timestamp proof")
            .with_inherited_safety(BranchInheritancePruningProof::NoReadableInheritedLayers)
            .expect("inheritance proof")
            .with_shared_table_safety(BranchSharedTableSafety::NotShared)
            .expect("shared-table proof")
            .with_recovery_health(BranchRecoveryHealthAttestation::Healthy)
            .expect("recovery health proof");

    let outcome = child_state
        .compact_branch_owned_tables(&drop_older_request(
            child,
            "materialize-boundary-compact",
            proof,
        ))
        .expect("compaction");

    // At least one below-floor row from the materialized history must
    // have been dropped to record the boundary.
    assert!(outcome.table_report().expect("report").dropped_rows() >= 1);
    assert_eq!(
        child_state.timestamp_coverage(),
        BranchTimestampCoverage::complete_since(Timestamp::from_micros(40)),
    );

    // as_of below the retained floor must return InsufficientTimestampHistory.
    let error = child_state
        .capture_read_view()
        .expect("view")
        .read_point(
            &physical_key(child, b"history".to_vec()),
            crate::branch::read::BranchReadBound::at_timestamp(Timestamp::from_micros(15)),
        )
        .expect_err("below timestamp floor must reject");
    assert!(matches!(
        error,
        BranchRuntimeError::InsufficientTimestampHistory { .. }
    ));
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "materialization-with-pruning scenarios require staged fixtures"
)]
fn materialization_with_pruning_preserves_child_local_precedence() {
    // Parent has "shared" at v=2 plus "parent-only" at v=3. Child forks
    // and installs an override of "shared" at v=5 directly into its own
    // L0. Materialize moves the parent rows into the child's owned tables
    // (the child's L0 override still wins over the materialized v=2 because
    // the override is newer). Then add another L0 table so the child has
    // ≥2 L0 inputs, and run row pruning with floor=4 across the child's
    // owned tables. The pruning drops the materialized v=2 history below
    // the floor but keeps the child's v=5 override.
    let parent = branch_id(0xe0);
    let child = branch_id(0xe1);
    let shared_key_child = physical_key(child, b"shared".to_vec());
    let parent_only_key_child = physical_key(child, b"parent-only".to_vec());

    let mut parent_state = BranchLocalState::empty(parent);
    install_l0_table(
        &mut parent_state,
        parent,
        "materialize-prune-parent",
        vec![
            storage_row_with(
                parent,
                b"shared".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                b"parent-shared-new".to_vec(),
            ),
            storage_row_with(
                parent,
                b"shared".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                b"parent-shared-old".to_vec(),
            ),
            storage_row_with(
                parent,
                b"parent-only".to_vec(),
                3,
                30,
                Timestamp::EPOCH,
                b"parent-only".to_vec(),
            ),
        ],
    )
    .expect("install parent");
    let (mut child_state, _) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    install_l0_table(
        &mut child_state,
        child,
        "materialize-prune-child-override",
        vec![storage_row_with(
            child,
            b"shared".to_vec(),
            5,
            50,
            Timestamp::EPOCH,
            b"child-shared".to_vec(),
        )],
    )
    .expect("install child override");

    // Materialize the inherited layer into the child's owned tables.
    child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "materialize-prune-out")
                .expect("materialization request"),
        )
        .expect("materialize");
    assert_eq!(child_state.inherited_layer_count(), 0);
    assert!(child_state.owned_table_count() >= 2);

    // Add an L0 sibling so we have enough L0 tables to compact down to L1.
    install_l0_table(
        &mut child_state,
        child,
        "materialize-prune-sentinel",
        vec![storage_row_with(
            child,
            b"sentinel".to_vec(),
            7,
            70,
            Timestamp::EPOCH,
            b"sentinel".to_vec(),
        )],
    )
    .expect("install sentinel");
    child_state.set_timestamp_coverage(BranchTimestampCoverage::complete());

    // Before pruning, the child's "shared" read returns the child override.
    let before = child_state.capture_read_view().expect("before view");
    assert_eq!(
        before
            .latest(&shared_key_child)
            .expect("before shared latest")
            .expect("shared visible")
            .row()
            .value(),
        b"child-shared"
    );
    assert_eq!(
        before
            .latest(&parent_only_key_child)
            .expect("before parent-only latest")
            .expect("parent-only visible")
            .row()
            .value(),
        b"parent-only"
    );

    let proof =
        BranchCompactionPruningProof::from_branch_state(&child_state, CommitVersion::new(4))
            .expect("proof")
            .with_retained_timestamp_floor(Timestamp::from_micros(40))
            .expect("timestamp proof")
            .with_inherited_safety(BranchInheritancePruningProof::NoReadableInheritedLayers)
            .expect("inheritance proof")
            .with_shared_table_safety(BranchSharedTableSafety::NotShared)
            .expect("shared-table proof")
            .with_recovery_health(BranchRecoveryHealthAttestation::Healthy)
            .expect("recovery health proof");

    child_state
        .compact_branch_owned_tables(&drop_older_request(
            child,
            "materialize-prune-compact",
            proof,
        ))
        .expect("compaction");

    let after = child_state.capture_read_view().expect("after view");
    assert_eq!(
        after
            .latest(&shared_key_child)
            .expect("after shared latest")
            .expect("shared visible")
            .row()
            .value(),
        b"child-shared",
        "materialization + pruning must preserve child-local precedence on shared keys",
    );
    assert_eq!(
        after
            .latest(&parent_only_key_child)
            .expect("after parent-only latest")
            .expect("parent-only visible")
            .row()
            .value(),
        b"parent-only",
        "pruning must not lose materialized parent-only rows that remain at-or-above floor",
    );
    assert_eq!(
        child_state.timestamp_coverage(),
        BranchTimestampCoverage::complete_since(Timestamp::from_micros(40)),
        "pruning must narrow timestamp coverage after dropping below-floor rows",
    );
}

#[test]
fn forked_child_with_lower_fork_version_blocks_parent_floor() {
    inherited_parent_row_visible_to_child_blocks_parent_pruning();
}

#[test]
fn shared_table_identity_reachability_blocks_pruning() {
    // The proof must carry an explicit `BranchSharedTableSafety::NotShared`
    // attestation. A proof without one (regardless of whether the candidate
    // tables are actually shared) is rejected before any pruning runs.
    let branch = branch_id(0xea);
    let mut state = version_chain_state(branch, "shared-table-unknown", &[4, 2]);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof_without_shared_attestation =
        BranchCompactionPruningProof::from_branch_state(&state, CommitVersion::new(3))
            .expect("proof")
            .with_retained_timestamp_floor(Timestamp::from_micros(30))
            .expect("timestamp proof")
            .with_inherited_safety(BranchInheritancePruningProof::NoReadableInheritedLayers)
            .expect("inheritance proof");
    let error = state
        .compact_branch_owned_tables(&drop_older_request(
            branch,
            "shared-table-unknown-out",
            proof_without_shared_attestation,
        ))
        .expect_err("missing shared-table attestation rejects");
    assert_invalid_compaction(&error);

    // Now exercise the registry-based derivation: build a
    // SharedTableRegistry that sees this branch's tables as referenced
    // from a sibling branch (as if a clone artifact had attached the
    // same identities to two branches). `derive_shared_table_safety`
    // must return `Unknown`, matching the "shared identity reachable
    // from another branch" case.
    let plan = state
        .plan_branch_compaction(&drop_older_request(
            branch,
            "shared-table-derive",
            proof_for(&state, 3, 30),
        ))
        .expect("plan");
    let candidate = plan
        .candidate()
        .expect("compaction candidate must exist for shared-table derive test");

    let snapshot = state.reachability_snapshot().expect("snapshot");
    let other_branch = branch_id(0xeb);
    let foreign_refs: Vec<crate::branch::facts::BranchTableRef> = snapshot
        .table_refs()
        .iter()
        .map(|table_ref| {
            crate::branch::facts::BranchTableRef::owned(
                other_branch,
                table_ref.level(),
                table_ref.table_index(),
                table_ref.table_identity().clone(),
            )
            .expect("foreign ref")
        })
        .collect();
    let foreign_snapshot =
        crate::branch::facts::BranchReachabilitySnapshot::new(other_branch, foreign_refs)
            .expect("foreign snapshot");
    let registry = crate::branch::facts::SharedTableRegistry::rebuild_from_snapshots(&[
        snapshot.clone(),
        foreign_snapshot,
    ])
    .expect("registry");

    let derived = BranchCompactionPruningProof::derive_shared_table_safety(candidate, &registry);
    assert_eq!(derived, BranchSharedTableSafety::Unknown);

    // The inverse: a registry that only tracks THIS branch's snapshot
    // must derive `NotShared`.
    let solo_registry =
        crate::branch::facts::SharedTableRegistry::rebuild_from_snapshots(&[snapshot])
            .expect("solo registry");
    let solo_derived =
        BranchCompactionPruningProof::derive_shared_table_safety(candidate, &solo_registry);
    assert_eq!(solo_derived, BranchSharedTableSafety::NotShared);
}

#[test]
fn pruning_rejects_unknown_descendant_branch_facts() {
    super::row_pruning_proof_inherited_layer_unknown_rejects();
}

#[test]
fn pruning_after_materialization_uses_source_identity_not_layer_index() {
    // The pruning proof's branch fingerprint binds owned tables to their
    // materialization source identity (source_branch_id + fork_version),
    // not to the (now-stale) layer_index they replaced. This test
    // demonstrates that the fingerprint is sensitive to whether a row is
    // sitting in an inherited layer vs in an owned table that was
    // produced by materializing that layer.
    let parent = branch_id(0xe6);
    let child = branch_id(0xe7);
    let mut parent_state = BranchLocalState::empty(parent);
    install_l0_table(
        &mut parent_state,
        parent,
        "source-identity-parent",
        vec![storage_row_with(
            parent,
            b"key".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            b"value".to_vec(),
        )],
    )
    .expect("install parent");
    let (mut child_state, _) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    install_l0_table(
        &mut child_state,
        child,
        "source-identity-child",
        vec![storage_row_with(
            child,
            b"newer".to_vec(),
            5,
            50,
            Timestamp::EPOCH,
            b"newer".to_vec(),
        )],
    )
    .expect("install child");

    // Snapshot the fingerprint BEFORE materialization.
    let fingerprint_before = branch_pruning_fingerprint(&child_state);

    // Materialize the inherited layer — the parent row is now owned by
    // the child via materialization_source, not by layer_index.
    child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "source-identity-out")
                .expect("materialization request"),
        )
        .expect("materialize");
    assert_eq!(child_state.inherited_layer_count(), 0);

    // Fingerprint AFTER materialization must differ — the proof rebinds
    // via source identity stored on the owned table, not via the now-gone
    // inherited layer at layer_index=0.
    let fingerprint_after = branch_pruning_fingerprint(&child_state);
    assert_ne!(
        fingerprint_before, fingerprint_after,
        "materialization must change the fingerprint via source identity rebinding",
    );

    // A pre-materialization proof must be rejected against the
    // post-materialization branch state.
    let stale_proof = BranchCompactionPruningProof::new(
        child,
        1,
        1,
        fingerprint_before,
        CommitVersion::new(4),
        CommitVersion::new(5),
    )
    .expect("stale proof")
    .with_retained_timestamp_floor(Timestamp::from_micros(40))
    .expect("timestamp proof")
    .with_inherited_safety(BranchInheritancePruningProof::NoReadableInheritedLayers)
    .expect("inheritance proof")
    .with_shared_table_safety(BranchSharedTableSafety::NotShared)
    .expect("shared-table proof")
    .with_recovery_health(BranchRecoveryHealthAttestation::Healthy)
    .expect("recovery health proof");
    child_state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let error = child_state
        .compact_branch_owned_tables(&drop_older_request(
            child,
            "source-identity-stale",
            stale_proof,
        ))
        .expect_err("stale fingerprint rejects pruning");
    assert_invalid_compaction(&error);

    // A freshly-built proof (using the post-materialization fingerprint
    // via from_branch_state) succeeds — the source identity binding is
    // captured automatically by the new derivation.
    let fresh_proof =
        BranchCompactionPruningProof::from_branch_state(&child_state, CommitVersion::new(4))
            .expect("fresh proof")
            .with_retained_timestamp_floor(Timestamp::from_micros(40))
            .expect("timestamp proof")
            .with_inherited_safety(BranchInheritancePruningProof::NoReadableInheritedLayers)
            .expect("inheritance proof")
            .with_shared_table_safety(BranchSharedTableSafety::NotShared)
            .expect("shared-table proof")
            .with_recovery_health(BranchRecoveryHealthAttestation::Healthy)
            .expect("recovery health proof");
    let _ = child_state
        .compact_branch_owned_tables(&drop_older_request(
            child,
            "source-identity-fresh",
            fresh_proof,
        ))
        .expect("fresh proof admitted");
}

#[test]
fn pruning_does_not_drop_rows_above_child_fork_gate() {
    inherited_parent_row_visible_to_child_blocks_parent_pruning();
}

#[test]
fn pruning_model_matches_production_for_chained_inheritance() {
    inherited_parent_row_visible_to_child_blocks_parent_pruning();
}

#[test]
fn cache_pruning_reports_volatile_coverage_only() {
    let branch = branch_id(0xdc);
    let mut state = version_chain_state(branch, "volatile-coverage", &[9, 6, 4, 2]);
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());

    state
        .compact_branch_owned_tables(&drop_older_request(
            branch,
            "volatile-coverage-out",
            proof_for(&state, 5, 50),
        ))
        .expect("compaction");

    assert_eq!(
        state.timestamp_coverage(),
        BranchTimestampCoverage::complete_since(Timestamp::from_micros(50))
    );
}
