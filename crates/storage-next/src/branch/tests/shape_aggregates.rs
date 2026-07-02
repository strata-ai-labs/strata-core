//! BS1.1 — cached branch shape-aggregate correctness.
//!
//! Every accessor asserts `cached == fresh fold` internally in debug builds (the
//! oracle), so these tests both (a) trip that oracle by exercising each mutation class
//! and (b) assert the returned values equal an independent reference fold — catching a
//! stale cache (oracle) and a wrong-but-consistent recompute (reference fold).

use super::*;

/// Independent reference fold over the branch's tables, asserted against every cached
/// accessor. Deliberately recomputes from the raw tables (not `recompute_shape_aggregates`)
/// so it is a true reference. Calling the accessors also fires the internal debug oracle.
fn assert_shape_matches_fold(state: &BranchLocalState) {
    let owned_resident: u64 = state
        .owned_levels()
        .iter()
        .flatten()
        .map(BranchOwnedTable::approximate_size_bytes)
        .fold(0u64, u64::saturating_add);
    assert_eq!(
        state.owned_table_byte_count(),
        owned_resident,
        "owned_bytes"
    );

    let owned_tables: usize = state.owned_levels().iter().map(Vec::len).sum();
    assert_eq!(state.owned_table_count(), owned_tables, "owned_tables");

    let frozen_resident: u64 = state
        .frozen()
        .iter()
        .map(|table| u64::try_from(table.approximate_size_bytes()).unwrap_or(u64::MAX))
        .fold(0u64, u64::saturating_add);
    assert_eq!(state.frozen_byte_count(), frozen_resident, "frozen_bytes");

    let per_level = state.per_level_bytes();
    assert_eq!(
        per_level.len(),
        state.owned_levels().len(),
        "per_level_bytes length"
    );
    for (level_index, tables) in state.owned_levels().iter().enumerate() {
        let logical: u64 = tables
            .iter()
            .map(|table| table.facts().byte_count())
            .fold(0u64, u64::saturating_add);
        assert_eq!(
            per_level[level_index], logical,
            "per_level_bytes[{level_index}]"
        );
    }

    let inherited: usize = state
        .inherited_layers()
        .iter()
        .map(BranchInheritedLayer::table_count)
        .sum();
    assert_eq!(state.inherited_table_count(), inherited, "inherited_tables");
}

#[test]
fn shape_empty_and_append_leave_owned_and_frozen_zero() {
    let branch = branch_id(210);
    let mut state = BranchLocalState::empty(branch);
    assert_shape_matches_fold(&state);
    assert_eq!(state.owned_table_count(), 0);
    assert_eq!(state.frozen_byte_count(), 0);

    // The active memtable is not part of the shape aggregates, so appends leave it unchanged.
    state
        .append_committed_row(storage_row(branch, 1))
        .expect("append");
    assert_shape_matches_fold(&state);
    assert_eq!(state.frozen_byte_count(), 0);
    assert_eq!(state.owned_table_count(), 0);
}

#[test]
fn shape_tracks_rotation_delta() {
    let branch = branch_id(211);
    let mut state = BranchLocalState::empty(branch);
    state
        .append_committed_row(storage_row(branch, 1))
        .expect("append");
    assert_eq!(state.frozen_byte_count(), 0);

    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    assert_shape_matches_fold(&state);
    assert_eq!(state.frozen_table_count(), 1);
    assert!(state.frozen_byte_count() > 0);
    assert_eq!(state.owned_table_count(), 0);
}

#[test]
fn shape_tracks_flush_install_delta() {
    let branch = branch_id(212);
    let mut state = BranchLocalState::empty(branch);
    let row = storage_row_with(
        branch,
        b"flush".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"flush".to_vec(),
    );
    state.append_committed_row(row.clone()).expect("append");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    assert!(state.frozen_byte_count() > 0);

    let table = branch_owned_table(branch, BranchLevel::ZERO, "flush-l0", vec![row]);
    state
        .replace_frozen_with_l0_table(0, table)
        .expect("flush install");

    // frozen -1, owned L0 +1: the delta must move bytes from frozen to owned/level-0.
    assert_shape_matches_fold(&state);
    assert_eq!(state.frozen_table_count(), 0);
    assert_eq!(state.frozen_byte_count(), 0);
    assert_eq!(state.owned_table_count(), 1);
    assert!(state.owned_table_byte_count() > 0);
    assert!(state.per_level_bytes()[0] > 0);
}

#[test]
fn shape_tracks_hook_installs_at_l0_and_l1() {
    let branch = branch_id(213);
    let mut state = BranchLocalState::empty(branch);

    let l0 = branch_owned_table(
        branch,
        BranchLevel::ZERO,
        "hook-l0",
        vec![storage_row_with(
            branch,
            b"l0".to_vec(),
            1,
            1,
            Timestamp::EPOCH,
            b"v".to_vec(),
        )],
    );
    state.install_l0_table(l0).expect("install l0");
    assert_shape_matches_fold(&state);
    assert_eq!(state.owned_table_count(), 1);
    let l0_bytes = state.per_level_bytes()[0];
    assert!(l0_bytes > 0);

    let l1 = branch_owned_table(
        branch,
        BranchLevel::new(1),
        "hook-l1",
        vec![storage_row_with(
            branch,
            b"l1".to_vec(),
            2,
            2,
            Timestamp::EPOCH,
            b"v".to_vec(),
        )],
    );
    state
        .install_owned_table_at_level(BranchLevel::new(1), l1)
        .expect("install l1");
    assert_shape_matches_fold(&state);
    assert_eq!(state.owned_table_count(), 2);
    assert_eq!(
        state.per_level_bytes()[0],
        l0_bytes,
        "L0 sum unchanged by L1 install"
    );
    assert!(state.per_level_bytes()[1] > 0);
}

#[test]
fn shape_tracks_inherited_layers() {
    let child = branch_id(214);
    let source = branch_id(215);
    let mut state = BranchLocalState::empty(child);
    assert_eq!(state.inherited_table_count(), 0);

    let inherited_table = branch_owned_table(
        source,
        BranchLevel::ZERO,
        "inherited-l0",
        vec![storage_row_with(
            source,
            b"ik".to_vec(),
            1,
            1,
            Timestamp::EPOCH,
            b"v".to_vec(),
        )],
    );
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(10),
        InheritedLayerStatus::Active,
        vec![vec![inherited_table]],
    );
    state
        .attach_inherited_layers(vec![layer])
        .expect("attach inherited layer");

    assert_shape_matches_fold(&state);
    assert_eq!(state.inherited_table_count(), 1);
    assert_eq!(
        state.owned_table_count(),
        0,
        "inherited tables are not owned"
    );
}

#[test]
fn shape_stays_consistent_across_operation_sequence() {
    let branch = branch_id(216);
    let mut state = BranchLocalState::empty(branch);
    let mut counter = 0u64;

    // A deterministic mix of shape-affecting operations. The internal debug oracle plus the
    // reference fold assert consistency after every step, covering both the delta mutators
    // (rotate) and the hook mutators (install) interleaved.
    for step in 0..40u64 {
        match step % 5 {
            0 | 1 => {
                counter += 1;
                state
                    .append_committed_row(storage_row_with(
                        branch,
                        format!("k{counter}").into_bytes(),
                        counter,
                        counter,
                        Timestamp::EPOCH,
                        b"v".to_vec(),
                    ))
                    .expect("append");
            }
            2 => {
                // Rotation may be Skipped (empty active / frozen limit); shape holds either way.
                let _rotation = state.rotate_active();
            }
            3 => {
                counter += 1;
                let table = branch_owned_table(
                    branch,
                    BranchLevel::ZERO,
                    &format!("seq-l0-{counter}"),
                    vec![storage_row_with(
                        branch,
                        format!("l0k{counter}").into_bytes(),
                        counter,
                        counter,
                        Timestamp::EPOCH,
                        b"v".to_vec(),
                    )],
                );
                state.install_l0_table(table).expect("install l0");
            }
            _ => {
                counter += 1;
                let table = branch_owned_table(
                    branch,
                    BranchLevel::new(1),
                    &format!("seq-l1-{counter}"),
                    vec![storage_row_with(
                        branch,
                        format!("l1k{counter}").into_bytes(),
                        counter,
                        counter,
                        Timestamp::EPOCH,
                        b"v".to_vec(),
                    )],
                );
                state
                    .install_owned_table_at_level(BranchLevel::new(1), table)
                    .expect("install l1");
            }
        }
        assert_shape_matches_fold(&state);
    }

    assert!(state.owned_table_count() > 0, "sequence built owned tables");
}
