use super::*;

#[test]
fn commit_to_branch_a_does_not_change_branch_b() {
    let branch_a = branch_id(14);
    let branch_b = branch_id(15);
    let mut catalog = LifecycleBranchCatalog::new(BranchRuntimeConfig::default()).expect("catalog");
    catalog
        .create_branch(branch_a, generation(1), None)
        .expect("create a");
    catalog
        .create_branch(branch_b, generation(1), None)
        .expect("create b");
    catalog
        .branch_state_mut(branch_a, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("state a")
        .append_committed_row(put_row(branch_a, 1, b"isolated", b"a"))
        .expect("append a");

    let key_a = physical_key(branch_a, b"isolated");
    let key_b = physical_key(branch_b, b"isolated");
    assert!(catalog
        .capture_read_view(branch_a)
        .expect("view a")
        .latest(&key_a)
        .expect("read a")
        .is_some());
    assert!(catalog
        .capture_read_view(branch_b)
        .expect("view b")
        .latest(&key_b)
        .expect("read b")
        .is_none());
}

#[test]
fn prefix_scan_branch_a_does_not_emit_branch_b_rows() {
    let branch_a = branch_id(16);
    let branch_b = branch_id(17);
    let mut catalog = LifecycleBranchCatalog::new(BranchRuntimeConfig::default()).expect("catalog");
    catalog
        .create_branch(branch_a, generation(1), None)
        .expect("create a");
    catalog
        .create_branch(branch_b, generation(1), None)
        .expect("create b");
    catalog
        .branch_state_mut(branch_a, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("state a")
        .append_committed_row(put_row(branch_a, 1, b"scan-key", b"a"))
        .expect("append a");
    catalog
        .branch_state_mut(branch_b, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("state b")
        .append_committed_row(put_row(branch_b, 1, b"scan-key", b"b"))
        .expect("append b");

    let bounds = BranchScanBounds::prefix(&physical_key(branch_a, b"scan"));
    let rows = catalog
        .capture_read_view(branch_a)
        .expect("view a")
        .scan_prefix(&bounds, crate::branch::read::BranchReadBound::latest())
        .expect("scan a");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].row().physical_key().branch_id(), branch_a);
    assert_eq!(rows[0].row().value(), b"a");
}

#[test]
fn clear_branch_a_does_not_change_branch_b() {
    let branch_a = branch_id(51);
    let branch_b = branch_id(52);
    let key_b = physical_key(branch_b, b"branch-b");
    let mut catalog = LifecycleBranchCatalog::new(BranchRuntimeConfig::default()).expect("catalog");
    catalog
        .create_branch(branch_a, generation(1), None)
        .expect("create a");
    catalog
        .create_branch(branch_b, generation(1), None)
        .expect("create b");
    catalog
        .branch_state_mut(branch_b, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("state b")
        .append_committed_row(put_row(branch_b, 2, b"branch-b", b"b"))
        .expect("append b");

    catalog
        .clear_branch(branch_a, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("clear a");

    assert_eq!(
        catalog
            .capture_read_view(branch_b)
            .expect("view b")
            .latest(&key_b)
            .expect("read b")
            .expect("row b")
            .row()
            .value(),
        b"b"
    );
}

#[test]
fn delete_branch_a_does_not_change_branch_b() {
    let branch_a = branch_id(77);
    let branch_b = branch_id(78);
    let key_b = physical_key(branch_b, b"delete-isolation");
    let mut catalog = LifecycleBranchCatalog::new(BranchRuntimeConfig::default()).expect("catalog");
    catalog
        .create_branch(branch_a, generation(1), None)
        .expect("create a");
    catalog
        .create_branch(branch_b, generation(1), None)
        .expect("create b");
    catalog
        .branch_state_mut(branch_b, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("state b")
        .append_committed_row(put_row(branch_b, 2, b"delete-isolation", b"b"))
        .expect("append b");

    catalog
        .delete_branch(
            branch_a,
            CommitBranchGenerationGuard::exact(generation(1)),
            Some(CommitVersion::new(3)),
        )
        .expect("delete a");

    assert_eq!(
        catalog
            .capture_read_view(branch_b)
            .expect("view b")
            .latest(&key_b)
            .expect("read b")
            .expect("row b")
            .row()
            .value(),
        b"b"
    );
}

#[test]
fn fork_branch_a_to_branch_c_does_not_change_branch_b() {
    let branch_a = branch_id(79);
    let branch_b = branch_id(80);
    let branch_c = branch_id(81);
    let key_b = physical_key(branch_b, b"fork-isolation");
    let mut catalog = LifecycleBranchCatalog::new(BranchRuntimeConfig::default()).expect("catalog");
    catalog
        .create_branch(branch_a, generation(1), None)
        .expect("create a");
    catalog
        .create_branch(branch_b, generation(1), None)
        .expect("create b");
    catalog
        .replace_active_branch_state(
            branch_a,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(branch_a, &[put_row(branch_a, 2, b"fork-isolation", b"a")]),
        )
        .expect("replace a");
    catalog
        .branch_state_mut(branch_b, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("state b")
        .append_committed_row(put_row(branch_b, 2, b"fork-isolation", b"b"))
        .expect("append b");

    catalog
        .fork_current(branch_a, branch_c, generation(1))
        .expect("fork c");

    assert_eq!(
        catalog
            .capture_read_view(branch_b)
            .expect("view b")
            .latest(&key_b)
            .expect("read b")
            .expect("row b")
            .row()
            .value(),
        b"b"
    );
}

#[test]
fn row_with_wrong_branch_id_rejects_install() {
    let branch_a = branch_id(82);
    let branch_b = branch_id(83);
    let mut catalog = catalog_with_branch(branch_a, generation(1));

    assert!(catalog
        .branch_state_mut(branch_a, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("state a")
        .append_committed_row(put_row(branch_b, 2, b"wrong-branch", b"b"))
        .is_err());
}

#[test]
fn as_of_branch_a_does_not_use_branch_b_timeline() {
    let branch_a = branch_id(120);
    let branch_b = branch_id(121);
    let mut catalog = LifecycleBranchCatalog::new(BranchRuntimeConfig::default()).expect("catalog");
    catalog
        .create_branch(branch_a, generation(1), None)
        .expect("create a");
    catalog
        .create_branch(branch_b, generation(1), None)
        .expect("create b");
    // Branch A has a row at version 3. Branch B has a row at version 5
    // (later in absolute time). An as-of read on A at version 4 must NOT
    // see B's row — branches own their own timelines.
    catalog
        .branch_state_mut(branch_a, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("state a")
        .append_committed_row(put_row(branch_a, 3, b"timeline", b"a"))
        .expect("append a");
    catalog
        .branch_state_mut(branch_b, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("state b")
        .append_committed_row(put_row(branch_b, 5, b"timeline", b"b"))
        .expect("append b");

    let key_a = physical_key(branch_a, b"timeline");
    let view_a = catalog.capture_read_view(branch_a).expect("view a");
    let row = view_a
        .read_point(&key_a, BranchReadBound::at_version(CommitVersion::new(4)))
        .expect("as-of read")
        .expect("row");
    assert_eq!(row.row().physical_key().branch_id(), branch_a);
    assert_eq!(row.row().value(), b"a");
}

#[test]
fn history_branch_a_does_not_use_branch_b_rows() {
    let branch_a = branch_id(122);
    let branch_b = branch_id(123);
    let mut catalog = LifecycleBranchCatalog::new(BranchRuntimeConfig::default()).expect("catalog");
    catalog
        .create_branch(branch_a, generation(1), None)
        .expect("create a");
    catalog
        .create_branch(branch_b, generation(1), None)
        .expect("create b");
    catalog
        .branch_state_mut(branch_a, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("state a")
        .append_committed_row(put_row(branch_a, 2, b"hist", b"a-v2"))
        .expect("append a");
    catalog
        .branch_state_mut(branch_b, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("state b")
        .append_committed_row(put_row(branch_b, 3, b"hist", b"b-v3"))
        .expect("append b");
    catalog
        .branch_state_mut(branch_a, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("state a")
        .append_committed_row(put_row(branch_a, 4, b"hist", b"a-v4"))
        .expect("append a");

    let key_a = physical_key(branch_a, b"hist");
    let history = catalog
        .capture_read_view(branch_a)
        .expect("view a")
        .history(&key_a, BranchHistoryOptions::all())
        .expect("history");

    assert_eq!(history.len(), 2);
    for entry in &history {
        assert_eq!(entry.row().physical_key().branch_id(), branch_a);
    }
}

#[test]
fn materialize_branch_c_does_not_change_branch_a() {
    let branch_a = branch_id(124);
    let branch_c = branch_id(125);
    let key_a = physical_key(branch_a, b"materialize-iso");
    let mut catalog = catalog_with_branch(branch_a, generation(1));
    catalog
        .replace_active_branch_state(
            branch_a,
            CommitBranchGenerationGuard::exact(generation(1)),
            owned_state(branch_a, &[put_row(branch_a, 2, b"materialize-iso", b"a")]),
        )
        .expect("replace a");
    catalog
        .fork_current(branch_a, branch_c, generation(1))
        .expect("fork c");
    let view_a_before = catalog
        .capture_read_view(branch_a)
        .expect("view a before")
        .latest(&key_a)
        .expect("read")
        .expect("row")
        .row()
        .value()
        .to_vec();
    // Materialize branch C's inherited layer. Branch A must remain unchanged.
    catalog
        .branch_state_mut(branch_c, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("state c")
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(branch_c, 0, "branch-lifecycle-iso-mat")
                .expect("request"),
        )
        .expect("materialize");

    let view_a_after = catalog
        .capture_read_view(branch_a)
        .expect("view a after")
        .latest(&key_a)
        .expect("read")
        .expect("row")
        .row()
        .value()
        .to_vec();
    assert_eq!(view_a_before, view_a_after);
}
