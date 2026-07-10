use super::*;

#[test]
fn fork_current_missing_source_rejects_before_destination_mutation() {
    let source = branch_id(26);
    let child = branch_id(27);
    let mut catalog = LifecycleBranchCatalog::new(BranchRuntimeConfig::default()).expect("catalog");

    assert_eq!(
        catalog.fork_current(source, child, generation(1)),
        Err(LifecycleError::BranchNotFound { branch_id: source })
    );
    assert_eq!(
        catalog.lookup(child),
        Err(LifecycleError::BranchNotFound { branch_id: child })
    );
}

#[test]
fn fork_current_existing_destination_rejects() {
    let source = branch_id(28);
    let child = branch_id(29);
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .create_branch(child, generation(1), None)
        .expect("create child");

    assert_eq!(
        catalog.fork_current(source, child, generation(1)),
        Err(LifecycleError::BranchAlreadyExists { branch_id: child })
    );
}

// The catalog API does not expose a "registered-but-empty" destination state:
// `create_branch` immediately registers the descriptor, and any subsequent fork
// attempt against that branch_id hits the "already exists" guard regardless of
// whether the destination has rows. This test exercises that the existing
// destination's rows are preserved when fork rejects — proving the rejection
// is non-destructive.
#[test]
fn fork_current_existing_destination_with_rows_rejects_without_overwrite() {
    let source = branch_id(64);
    let child = branch_id(65);
    let child_key = physical_key(child, b"nonempty-destination");
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .create_branch(child, generation(1), None)
        .expect("create child");
    catalog
        .branch_state_mut(child, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("child state")
        .append_committed_row(put_row(child, 2, b"nonempty-destination", b"row"))
        .expect("append");

    assert_eq!(
        catalog.fork_current(source, child, generation(1)),
        Err(LifecycleError::BranchAlreadyExists { branch_id: child })
    );
    assert_eq!(
        catalog
            .capture_read_view(child)
            .expect("child view")
            .latest(&child_key)
            .expect("child read")
            .expect("row preserved")
            .row()
            .value(),
        b"row"
    );
}

#[test]
fn fork_current_inherits_owned_tables_without_copying_objects() {
    let source = branch_id(6);
    let child = branch_id(7);
    let mut catalog = catalog_with_branch(source, generation(1));
    let source_state = owned_state(source, &[put_row(source, 4, b"fork-key", b"parent")]);
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            source_state,
        )
        .expect("replace source");

    let outcome = catalog
        .fork_current(source, child, generation(1))
        .expect("fork current");

    assert_eq!(outcome.source_branch_id(), source);
    assert_eq!(outcome.descriptor().branch_id(), child);
    assert_eq!(outcome.inherited_layer_count(), 1);
    assert_eq!(outcome.inherited_table_count(), 1);
    assert_eq!(
        outcome
            .descriptor()
            .parent()
            .expect("parent")
            .source_branch_id(),
        source
    );
    assert_eq!(
        outcome
            .descriptor()
            .parent()
            .expect("parent")
            .fork_version(),
        CommitVersion::new(4)
    );
    let child_key = physical_key(child, b"fork-key");
    assert_eq!(
        catalog
            .capture_read_view(child)
            .expect("child view")
            .latest(&child_key)
            .expect("child read")
            .expect("inherited row")
            .row()
            .value(),
        b"parent"
    );
}

#[test]
fn fork_current_inherited_rows_are_visible_in_child() {
    let source = branch_id(81);
    let child = branch_id(82);
    let child_key = physical_key(child, b"inherited-visible");
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(
                source,
                &[put_row(source, 4, b"inherited-visible", b"source")],
            ),
        )
        .expect("replace source");

    catalog
        .fork_current(source, child, generation(1))
        .expect("fork");

    assert_eq!(
        catalog
            .capture_read_view(child)
            .expect("child view")
            .latest(&child_key)
            .expect("child read")
            .expect("inherited row")
            .row()
            .value(),
        b"source"
    );
}

#[test]
fn fork_current_records_source_branch_and_fork_version() {
    let source = branch_id(83);
    let child = branch_id(84);
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(source, &[put_row(source, 11, b"fork-version", b"source")]),
        )
        .expect("replace source");

    let outcome = catalog
        .fork_current(source, child, generation(1))
        .expect("fork");

    assert_eq!(outcome.source_branch_id(), source);
    assert_eq!(outcome.fork_version(), CommitVersion::new(11));
    assert_eq!(
        outcome
            .descriptor()
            .parent()
            .expect("parent")
            .source_branch_id(),
        source
    );
    assert_eq!(
        outcome
            .descriptor()
            .parent()
            .expect("parent")
            .fork_version(),
        CommitVersion::new(11)
    );
}

#[test]
fn fork_current_reachability_facts_include_shared_tables() {
    let source = branch_id(66);
    let child = branch_id(67);
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(source, &[put_row(source, 4, b"shared-reach", b"parent")]),
        )
        .expect("replace source");
    catalog
        .fork_current(source, child, generation(1))
        .expect("fork");

    let child_snapshot = catalog
        .branch_state(child)
        .expect("child state")
        .reachability_snapshot()
        .expect("reachability");

    assert_eq!(child_snapshot.table_refs().len(), 1);
    let table_ref = &child_snapshot.table_refs()[0];
    assert_eq!(table_ref.owner_branch_id(), child);
    assert_eq!(table_ref.table_branch_id(), source);
    assert_eq!(
        table_ref.reference_kind(),
        BranchTableReferenceKind::Inherited {
            source_branch_id: source,
            fork_version: CommitVersion::new(4),
            layer_index: 0,
        }
    );
}

#[test]
fn fork_current_works_from_materialized_replacement_tables() {
    let source = branch_id(85);
    let child = branch_id(86);
    let grandchild = branch_id(87);
    let grandchild_key = physical_key(grandchild, b"materialized-source");
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(
                source,
                &[put_row(source, 4, b"materialized-source", b"source")],
            ),
        )
        .expect("replace source");
    catalog
        .fork_current(source, child, generation(1))
        .expect("fork child");
    catalog
        .branch_state_mut(child, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("child state")
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "branch-lifecycle-materialized-source")
                .expect("materialization request"),
        )
        .expect("materialize");

    let outcome = catalog
        .fork_current(child, grandchild, generation(1))
        .expect("fork grandchild");

    assert_eq!(outcome.source_branch_id(), child);
    assert_eq!(outcome.inherited_layer_count(), 1);
    assert_eq!(
        catalog
            .capture_read_view(grandchild)
            .expect("grandchild view")
            .latest(&grandchild_key)
            .expect("read")
            .expect("row")
            .row()
            .value(),
        b"source"
    );
}

#[test]
fn fork_current_preserves_inherited_chain_order() {
    let parent = branch_id(88);
    let child = branch_id(89);
    let grandchild = branch_id(90);
    let grandchild_key = physical_key(grandchild, b"chain-order");
    let mut catalog = catalog_with_branch(parent, generation(1));
    catalog
        .replace_active_branch_state(
            parent,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(parent, &[put_row(parent, 4, b"chain-order", b"parent")]),
        )
        .expect("replace parent");
    catalog
        .fork_current(parent, child, generation(1))
        .expect("fork child");
    catalog
        .fork_current(child, grandchild, generation(1))
        .expect("fork grandchild");

    let snapshot = catalog
        .branch_state(grandchild)
        .expect("grandchild state")
        .reachability_snapshot()
        .expect("reachability");
    assert_eq!(snapshot.table_refs().len(), 1);
    assert_eq!(
        catalog
            .capture_read_view(grandchild)
            .expect("view")
            .latest(&grandchild_key)
            .expect("read")
            .expect("row")
            .row()
            .value(),
        b"parent"
    );
}

#[test]
fn fork_current_source_with_active_rows_is_rejected_explicitly() {
    let source = branch_id(8);
    let child = branch_id(9);
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .branch_state_mut(source, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("state")
        .append_committed_row(put_row(source, 1, b"active", b"row"))
        .expect("append");

    assert_eq!(
        catalog.fork_current(source, child, generation(1)),
        Err(LifecycleError::SourceHasUnflushedRows { branch_id: source })
    );
    assert_eq!(
        catalog.lookup(child),
        Err(LifecycleError::BranchNotFound { branch_id: child })
    );
}

#[test]
fn fork_current_child_local_row_shadows_inherited_row() {
    let source = branch_id(30);
    let child = branch_id(31);
    let key = physical_key(child, b"shadow");
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(source, &[put_row(source, 4, b"shadow", b"parent")]),
        )
        .expect("replace source");
    catalog
        .fork_current(source, child, generation(1))
        .expect("fork");
    catalog
        .branch_state_mut(child, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("child state")
        .append_committed_row(put_row(child, 5, b"shadow", b"child"))
        .expect("append child");

    assert_eq!(
        catalog
            .capture_read_view(child)
            .expect("child view")
            .latest(&key)
            .expect("read")
            .expect("row")
            .row()
            .value(),
        b"child"
    );
}

#[test]
fn fork_current_source_later_write_does_not_change_child_view() {
    let source = branch_id(32);
    let child = branch_id(33);
    let child_key = physical_key(child, b"stable-child");
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(source, &[put_row(source, 4, b"stable-child", b"parent")]),
        )
        .expect("replace source");
    catalog
        .fork_current(source, child, generation(1))
        .expect("fork");
    catalog
        .branch_state_mut(source, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("source state")
        .append_committed_row(put_row(source, 5, b"stable-child", b"later"))
        .expect("append source");

    assert_eq!(
        catalog
            .capture_read_view(child)
            .expect("child view")
            .latest(&child_key)
            .expect("read")
            .expect("row")
            .row()
            .value(),
        b"parent"
    );
}

#[test]
fn fork_at_history_child_excludes_rows_after_requested_version() {
    let source = branch_id(10);
    let child = branch_id(11);
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(
                source,
                &[
                    put_row(source, 2, b"history-key", b"old"),
                    put_row(source, 5, b"history-key", b"new"),
                ],
            ),
        )
        .expect("replace source");

    let outcome = catalog
        .fork_at_retained_version(
            source,
            child,
            generation(1),
            CommitVersion::new(2),
            CommitVersion::new(1),
        )
        .expect("fork at retained version");

    assert_eq!(outcome.fork_version(), CommitVersion::new(2));
    // fork-cow.2: the source's rows are sealed in owned tables and V (= 2) is fully durable, so this
    // fork is copy-on-write — the child references the source's owned tables through one inherited layer
    // instead of materializing the `<= V` rows into its own tables.
    assert_eq!(outcome.inherited_layer_count(), 1);
    assert!(outcome.inherited_table_count() >= 1);
    assert_eq!(
        catalog
            .branch_state(child)
            .expect("child state")
            .owned_table_count(),
        0,
        "a copy-on-write fork child materializes no tables of its own",
    );
    let child_key = physical_key(child, b"history-key");
    let child_view = catalog.capture_read_view(child).expect("child view");
    assert_eq!(
        child_view
            .latest(&child_key)
            .expect("child read")
            .expect("old visible")
            .row()
            .value(),
        b"old"
    );
    let child_history = child_view
        .history(&child_key, BranchHistoryOptions::all())
        .expect("child history");
    assert_eq!(child_history.len(), 1);
    assert_eq!(
        child_history[0].row().commit_version(),
        CommitVersion::new(2)
    );
}

#[test]
fn fork_at_history_falls_back_to_materialization_for_unsealed_in_fork_rows() {
    // Gate: when a `<= V` row is still in the source's active memtable (not yet sealed into an owned
    // table), an inherited layer cannot reference it — so the fork must materialize instead of COW.
    let source = branch_id(12);
    let child = branch_id(13);
    let mut catalog = catalog_with_branch(source, generation(1));
    {
        let src = catalog
            .branch_state_mut(source, CommitBranchGenerationGuard::exact(generation(1)))
            .expect("source state");
        src.append_committed_row(put_row(source, 2, b"history-key", b"old"))
            .expect("append v2");
        src.append_committed_row(put_row(source, 5, b"history-key", b"new"))
            .expect("append v5");
    }

    let outcome = catalog
        .fork_at_retained_version(
            source,
            child,
            generation(1),
            CommitVersion::new(2),
            CommitVersion::new(1),
        )
        .expect("fork at retained version");

    // The in-fork row (v2) is unsealed, so the fork materializes: no inherited layer, and the child
    // owns its own tables.
    assert_eq!(outcome.inherited_layer_count(), 0);
    assert_eq!(outcome.inherited_table_count(), 0);
    assert!(
        catalog
            .branch_state(child)
            .expect("child state")
            .owned_table_count()
            > 0,
        "the materialization fallback gives the child its own tables",
    );
    // Read equivalence still holds on the fallback path: the child sees v2, not v5.
    let child_key = physical_key(child, b"history-key");
    let child_view = catalog.capture_read_view(child).expect("child view");
    assert_eq!(
        child_view
            .latest(&child_key)
            .expect("child read")
            .expect("old visible")
            .row()
            .value(),
        b"old"
    );
    let child_history = child_view
        .history(&child_key, BranchHistoryOptions::all())
        .expect("child history");
    assert_eq!(child_history.len(), 1);
    assert_eq!(
        child_history[0].row().commit_version(),
        CommitVersion::new(2)
    );
}

// Fork-timeline inheritance contract. The inherited-layer read path uses
// `BranchEffectiveReadBound::for_inherited_layer`, which caps
// `AtTimestamp(T)` reads at `(fork_version, T)`. Per-row commit_timestamp
// drives timestamp matching. The three tests below pin the contract:
// - reads at T < fork return the parent's row through the inherited layer,
// - reads at T inside the child's own commits return the child's row,
// - parent post-fork commits are invisible to the child.

#[test]
fn forked_branch_at_timestamp_before_fork_returns_parent_row() {
    let source = branch_id(120);
    let child = branch_id(121);
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state_with_timestamp_coverage(
                source,
                &[
                    put_row(source, 1, b"history-key", b"v1"),
                    put_row(source, 5, b"history-key", b"v5"),
                ],
                BranchTimestampCoverage::complete(),
            ),
        )
        .expect("replace source");

    // Fork at version 5 so the child inherits both v1 and v5 in the
    // parent's inherited layer.
    catalog
        .fork_at_retained_version(
            source,
            child,
            generation(1),
            CommitVersion::new(5),
            CommitVersion::new(1),
        )
        .expect("fork at retained version");

    // Give the child the same timestamp coverage as the parent so
    // `AtTimestamp` reads can resolve without an
    // InsufficientTimestampHistory error.
    catalog
        .branch_state_mut(child, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("child state")
        .set_timestamp_coverage(BranchTimestampCoverage::complete());

    let child_key = physical_key(child, b"history-key");
    let child_view = catalog.capture_read_view(child).expect("child view");
    // Pre-fork timestamp 300 micros sits between v1 (100) and v5 (500);
    // the inherited-layer read returns the v1 row.
    let row = child_view
        .read_point(
            &child_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(300)),
        )
        .expect("read")
        .expect("inherited v1 row");
    assert_eq!(row.row().commit_version(), CommitVersion::new(1));
    assert_eq!(row.row().value(), b"v1");
}

#[test]
fn forked_branch_at_timestamp_after_fork_returns_child_row() {
    let source = branch_id(122);
    let child = branch_id(123);
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state_with_timestamp_coverage(
                source,
                &[put_row(source, 5, b"history-key", b"parent-v5")],
                BranchTimestampCoverage::complete(),
            ),
        )
        .expect("replace source");

    catalog
        .fork_at_retained_version(
            source,
            child,
            generation(1),
            CommitVersion::new(5),
            CommitVersion::new(1),
        )
        .expect("fork at retained version");

    catalog
        .branch_state_mut(child, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("child state")
        .set_timestamp_coverage(BranchTimestampCoverage::complete());
    // Child commits its own row at version 12 (timestamp 1200 micros).
    catalog
        .branch_state_mut(child, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("child state")
        .append_committed_row(put_row(child, 12, b"history-key", b"child-v12"))
        .expect("append child");

    let child_key = physical_key(child, b"history-key");
    let child_view = catalog.capture_read_view(child).expect("child view");
    // At timestamp 1300 micros (post-child-commit), the child's own v12
    // row is the latest visible.
    let row = child_view
        .read_point(
            &child_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(1300)),
        )
        .expect("read")
        .expect("child v12 row");
    assert_eq!(row.row().commit_version(), CommitVersion::new(12));
    assert_eq!(row.row().value(), b"child-v12");
}

#[test]
fn forked_branch_isolated_from_parent_post_fork_commits() {
    let source = branch_id(124);
    let child = branch_id(125);
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state_with_timestamp_coverage(
                source,
                &[put_row(source, 5, b"history-key", b"parent-v5")],
                BranchTimestampCoverage::complete(),
            ),
        )
        .expect("replace source");

    catalog
        .fork_at_retained_version(
            source,
            child,
            generation(1),
            CommitVersion::new(5),
            CommitVersion::new(1),
        )
        .expect("fork at retained version");

    catalog
        .branch_state_mut(child, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("child state")
        .set_timestamp_coverage(BranchTimestampCoverage::complete());
    catalog
        .branch_state_mut(child, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("child state")
        .append_committed_row(put_row(child, 12, b"history-key", b"child-v12"))
        .expect("append child");

    // Parent commits a NEW row at version 15 (timestamp 1500 micros) AFTER
    // the fork. The fork_version (5) caps the child's view of the parent's
    // inherited layer; the parent's v15 row is post-fork and invisible.
    catalog
        .branch_state_mut(source, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("source state")
        .append_committed_row(put_row(source, 15, b"history-key", b"parent-v15"))
        .expect("append source");

    let child_key = physical_key(child, b"history-key");
    let child_view = catalog.capture_read_view(child).expect("child view");
    // Timestamp 1600 micros is post-parent-v15. The child's read still
    // returns the child's own v12 row because the parent's post-fork
    // commit cannot enter the child's view (the inherited-layer read is
    // capped at fork_version=5; the parent's v15 exceeds the cap).
    let row = child_view
        .read_point(
            &child_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(1600)),
        )
        .expect("read")
        .expect("child v12 row stays visible");
    assert_eq!(row.row().commit_version(), CommitVersion::new(12));
    assert_eq!(row.row().value(), b"child-v12");
}

#[test]
fn fork_at_history_child_includes_rows_at_requested_version() {
    let source = branch_id(91);
    let child = branch_id(92);
    let child_key = physical_key(child, b"boundary-include");
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(
                source,
                &[
                    put_row(source, 2, b"boundary-include", b"old"),
                    put_row(source, 5, b"boundary-include", b"boundary"),
                    put_row(source, 8, b"boundary-include", b"new"),
                ],
            ),
        )
        .expect("replace source");

    catalog
        .fork_at_retained_version(
            source,
            child,
            generation(1),
            CommitVersion::new(5),
            CommitVersion::new(1),
        )
        .expect("fork at boundary");

    assert_eq!(
        catalog
            .capture_read_view(child)
            .expect("child view")
            .latest(&child_key)
            .expect("read")
            .expect("row")
            .row()
            .value(),
        b"boundary"
    );
}

#[test]
fn fork_at_history_retained_version_succeeds() {
    let source = branch_id(68);
    let child = branch_id(69);
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(source, &[put_row(source, 7, b"retained", b"value")]),
        )
        .expect("replace source");

    let outcome = catalog
        .fork_at_retained_version(
            source,
            child,
            generation(1),
            CommitVersion::new(7),
            CommitVersion::new(7),
        )
        .expect("fork at retained floor");

    assert_eq!(outcome.fork_version(), CommitVersion::new(7));
}

#[test]
fn fork_at_history_visible_latest_matches_current_fork() {
    let source = branch_id(70);
    let current_child = branch_id(71);
    let history_child = branch_id(72);
    let current_key = physical_key(current_child, b"latest-match");
    let history_key = physical_key(history_child, b"latest-match");
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(source, &[put_row(source, 7, b"latest-match", b"value")]),
        )
        .expect("replace source");
    catalog
        .fork_current(source, current_child, generation(1))
        .expect("current fork");
    catalog
        .fork_at_retained_version(
            source,
            history_child,
            generation(1),
            CommitVersion::new(7),
            CommitVersion::new(1),
        )
        .expect("history fork");

    let current_value = catalog
        .capture_read_view(current_child)
        .expect("current view")
        .latest(&current_key)
        .expect("current read")
        .expect("current row")
        .row()
        .value()
        .to_vec();
    let history_value = catalog
        .capture_read_view(history_child)
        .expect("history view")
        .latest(&history_key)
        .expect("history read")
        .expect("history row")
        .row()
        .value()
        .to_vec();

    assert_eq!(current_value, history_value);
}

#[test]
fn fork_at_history_after_visible_version_rejects() {
    let source = branch_id(34);
    let child = branch_id(35);
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(source, &[put_row(source, 2, b"after-visible", b"value")]),
        )
        .expect("replace source");

    assert_eq!(
        catalog.fork_at_retained_version(
            source,
            child,
            generation(1),
            CommitVersion::new(3),
            CommitVersion::new(1),
        ),
        Err(LifecycleError::BranchHistoryUnavailable {
            branch_id: source,
            reason: "requested fork version is newer than visible branch version",
        })
    );
}

#[test]
fn fork_at_history_source_deleted_before_capture_rejects() {
    let source = branch_id(73);
    let child = branch_id(74);
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .delete_branch(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            Some(CommitVersion::new(1)),
        )
        .expect("delete source");

    assert_eq!(
        catalog.fork_at_retained_version(
            source,
            child,
            generation(1),
            CommitVersion::new(1),
            CommitVersion::new(1),
        ),
        Err(LifecycleError::BranchNotWritable {
            branch_id: source,
            state: "deleted",
        })
    );
}

#[test]
fn fork_at_history_destination_generation_guard_is_enforced() {
    let source = branch_id(75);
    let child = branch_id(76);
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(source, &[put_row(source, 2, b"dest-generation", b"value")]),
        )
        .expect("replace source");
    catalog
        .create_branch(child, generation(1), None)
        .expect("create destination");
    catalog
        .delete_branch(
            child,
            CommitBranchGenerationGuard::exact(generation(1)),
            Some(CommitVersion::new(3)),
        )
        .expect("delete destination");

    assert_eq!(
        catalog.fork_at_retained_version(
            source,
            child,
            generation(1),
            CommitVersion::new(2),
            CommitVersion::new(1),
        ),
        Err(LifecycleError::BranchGenerationMismatch {
            branch_id: child,
            expected: 2,
            actual: 1,
        })
    );
}

#[test]
fn fork_at_history_from_inherited_source_includes_visible_parent_row() {
    let parent = branch_id(36);
    let child = branch_id(37);
    let grandchild = branch_id(38);
    let grandchild_key = physical_key(grandchild, b"inherited-history");
    let mut catalog = catalog_with_branch(parent, generation(1));
    catalog
        .replace_active_branch_state(
            parent,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(
                parent,
                &[
                    put_row(parent, 2, b"inherited-history", b"old"),
                    put_row(parent, 5, b"inherited-history", b"new"),
                ],
            ),
        )
        .expect("replace parent");
    catalog
        .fork_current(parent, child, generation(1))
        .expect("fork child");

    catalog
        .fork_at_retained_version(
            child,
            grandchild,
            generation(1),
            CommitVersion::new(2),
            CommitVersion::new(1),
        )
        .expect("fork grandchild");

    let view = catalog.capture_read_view(grandchild).expect("view");
    assert_eq!(
        view.latest(&grandchild_key)
            .expect("read")
            .expect("row")
            .row()
            .value(),
        b"old"
    );
    assert_eq!(
        view.history(&grandchild_key, BranchHistoryOptions::all())
            .expect("history")
            .len(),
        1
    );
}

#[test]
fn fork_at_history_tombstone_at_boundary_is_preserved() {
    let source = branch_id(39);
    let child = branch_id(40);
    let child_key = physical_key(child, b"tombstone-history");
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(
                source,
                &[
                    put_row(source, 2, b"tombstone-history", b"value"),
                    tombstone_row(source, 4, b"tombstone-history"),
                    put_row(source, 6, b"tombstone-history", b"newer"),
                ],
            ),
        )
        .expect("replace source");

    catalog
        .fork_at_retained_version(
            source,
            child,
            generation(1),
            CommitVersion::new(4),
            CommitVersion::new(1),
        )
        .expect("fork");

    let view = catalog.capture_read_view(child).expect("view");
    assert!(view.latest(&child_key).expect("read").is_none());
    let history = view
        .history(&child_key, BranchHistoryOptions::all())
        .expect("history");
    assert_eq!(history.len(), 2);
    assert!(history[0].row().is_tombstone());
}

#[test]
fn fork_at_history_below_retained_floor_rejects() {
    let source = branch_id(12);
    let child = branch_id(13);
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(source, &[put_row(source, 2, b"history-floor", b"value")]),
        )
        .expect("replace source");

    assert_eq!(
        catalog.fork_at_retained_version(
            source,
            child,
            generation(1),
            CommitVersion::new(1),
            CommitVersion::new(2),
        ),
        Err(LifecycleError::BranchHistoryUnavailable {
            branch_id: source,
            reason: "requested fork version is below retained history",
        })
    );
    assert_eq!(
        catalog.lookup(child),
        Err(LifecycleError::BranchNotFound { branch_id: child })
    );
}

#[test]
fn fork_at_history_missing_timestamp_coverage_rejects() {
    let source = branch_id(100);
    let child = branch_id(101);
    let mut catalog = catalog_with_branch(source, generation(1));
    // Default state from owned_state has BranchTimestampCoverage::Unknown.
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(source, &[put_row(source, 5, b"ts-missing", b"value")]),
        )
        .expect("replace source");

    assert_eq!(
        catalog.fork_at_retained_timestamp(
            source,
            child,
            generation(1),
            Timestamp::from_micros(500),
            CommitVersion::new(1),
        ),
        Err(LifecycleError::InsufficientTimestampHistory {
            branch_id: source,
            reason: "branch has no retained timestamp coverage",
        })
    );
    assert_eq!(
        catalog.lookup(child),
        Err(LifecycleError::BranchNotFound { branch_id: child })
    );
}

#[test]
fn fork_at_history_below_timestamp_coverage_rejects() {
    let source = branch_id(102);
    let child = branch_id(103);
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state_with_timestamp_coverage(
                source,
                &[put_row(source, 5, b"ts-floor", b"value")],
                BranchTimestampCoverage::complete_since(Timestamp::from_micros(400)),
            ),
        )
        .expect("replace source");

    assert_eq!(
        catalog.fork_at_retained_timestamp(
            source,
            child,
            generation(1),
            Timestamp::from_micros(200),
            CommitVersion::new(1),
        ),
        Err(LifecycleError::InsufficientTimestampHistory {
            branch_id: source,
            reason: "requested timestamp is below retained timestamp coverage",
        })
    );
}

#[test]
fn fork_at_history_timestamp_lookup_uses_timeline_tiebreaker() {
    let source = branch_id(104);
    let child = branch_id(105);
    let child_key = physical_key(child, b"timeline-tiebreaker");
    let mut catalog = catalog_with_branch(source, generation(1));
    // Two commits share commit_timestamp by design (storage allows
    // colocated timestamps; commit version is the timeline tiebreaker).
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state_with_timestamp_coverage(
                source,
                &[
                    StorageRow::put(
                        physical_key(source, b"timeline-tiebreaker"),
                        CommitVersion::new(3),
                        Timestamp::from_micros(300),
                        Timestamp::EPOCH,
                        b"older".to_vec(),
                    ),
                    StorageRow::put(
                        physical_key(source, b"timeline-tiebreaker"),
                        CommitVersion::new(5),
                        Timestamp::from_micros(300),
                        Timestamp::EPOCH,
                        b"newer".to_vec(),
                    ),
                    // Row past the fork timestamp must not appear.
                    put_row(source, 9, b"timeline-tiebreaker", b"future"),
                ],
                BranchTimestampCoverage::complete(),
            ),
        )
        .expect("replace source");

    let outcome = catalog
        .fork_at_retained_timestamp(
            source,
            child,
            generation(1),
            Timestamp::from_micros(300),
            CommitVersion::new(1),
        )
        .expect("fork at timestamp");

    // Resolved fork version is the highest commit at or before the timestamp.
    assert_eq!(outcome.fork_version(), CommitVersion::new(5));
    assert_eq!(
        catalog
            .capture_read_view(child)
            .expect("child view")
            .latest(&child_key)
            .expect("read")
            .expect("row")
            .row()
            .value(),
        b"newer"
    );
}

#[test]
fn fork_at_history_no_rows_at_or_before_timestamp_rejects() {
    let source = branch_id(106);
    let child = branch_id(107);
    let mut catalog = catalog_with_branch(source, generation(1));
    catalog
        .replace_active_branch_state(
            source,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state_with_timestamp_coverage(
                source,
                &[StorageRow::put(
                    physical_key(source, b"future"),
                    CommitVersion::new(5),
                    Timestamp::from_micros(1_000),
                    Timestamp::EPOCH,
                    b"value".to_vec(),
                )],
                BranchTimestampCoverage::complete(),
            ),
        )
        .expect("replace source");

    assert_eq!(
        catalog.fork_at_retained_timestamp(
            source,
            child,
            generation(1),
            Timestamp::from_micros(500),
            CommitVersion::new(1),
        ),
        Err(LifecycleError::InsufficientTimestampHistory {
            branch_id: source,
            reason: "branch has no rows at or before requested timestamp",
        })
    );
}

#[test]
fn pinned_view_survives_source_branch_delete_after_fork() {
    let parent = branch_id(108);
    let child = branch_id(109);
    let child_key = physical_key(child, b"survives-source-delete");
    let mut catalog = catalog_with_branch(parent, generation(1));
    catalog
        .replace_active_branch_state(
            parent,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(
                parent,
                &[put_row(parent, 3, b"survives-source-delete", b"parent")],
            ),
        )
        .expect("replace parent");
    catalog
        .fork_current(parent, child, generation(1))
        .expect("fork child");
    let view = catalog.capture_read_view(child).expect("child view");
    // Delete the source (parent) branch — child's pinned view must keep
    // serving inherited rows because the inherited layer is retained via
    // child reachability, not parent reachability.
    catalog
        .delete_branch(
            parent,
            CommitBranchGenerationGuard::exact(generation(1)),
            Some(CommitVersion::new(4)),
        )
        .expect("delete parent");

    assert_eq!(
        view.latest(&child_key)
            .expect("read")
            .expect("row")
            .row()
            .value(),
        b"parent"
    );
}
