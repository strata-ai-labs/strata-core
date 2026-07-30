//! A1 (#2524): plural flush install + partition verify.
//!
//! A frozen memtable flushes to N key-disjoint L0 tables (one until A2's
//! zone cuts land). The install is atomic — every table validates before
//! anything mutates — and `frozen_rows_match_tables` checks that the
//! outputs' concatenation partitions the frozen rows exactly.

use super::*;
use crate::branch::state::frozen_rows_match_tables;

fn frozen_state_with_rows(branch: BranchId, rows: &[StorageRow]) -> BranchLocalState {
    let mut state = BranchLocalState::empty(branch);
    for row in rows {
        state.append_committed_row(row.clone()).expect("append");
    }
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    state
}

fn two_disjoint_rows(branch: BranchId) -> (StorageRow, StorageRow) {
    (
        storage_row_with(
            branch,
            b"aa-low".to_vec(),
            3,
            30,
            Timestamp::EPOCH,
            b"low".to_vec(),
        ),
        storage_row_with(
            branch,
            b"zz-high".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            b"high".to_vec(),
        ),
    )
}

#[test]
fn install_n_replaces_one_frozen_table_with_key_disjoint_siblings() {
    let branch = branch_id(230);
    let (low, high) = two_disjoint_rows(branch);
    let mut state = frozen_state_with_rows(branch, &[low.clone(), high.clone()]);
    let identity = state.frozen()[0].memory_state_identity();

    let tables = vec![
        branch_owned_table(branch, BranchLevel::ZERO, "flush-cut-low", vec![low]),
        branch_owned_table(branch, BranchLevel::ZERO, "flush-cut-high", vec![high]),
    ];
    state
        .replace_frozen_with_level_zero_tables_by_identity(identity, tables)
        .expect("install two key-disjoint outputs");

    assert_eq!(
        state.frozen_table_count(),
        0,
        "the frozen input is consumed"
    );
    assert_eq!(state.owned_table_count(), 2, "both outputs land at L0");
    // Shape aggregates took the +N/-1 delta (the debug oracle inside the
    // delta already asserts cache==fold; this pins the public counts).
    assert_eq!(state.frozen_byte_count(), 0);
    assert!(state.per_level_bytes()[0] > 0);
}

#[test]
fn install_n_validates_every_table_before_mutating() {
    // A duplicate identity in the batch must reject WITHOUT consuming the
    // frozen table or installing the valid sibling (atomicity).
    let branch = branch_id(231);
    let (low, high) = two_disjoint_rows(branch);
    let mut state = frozen_state_with_rows(branch, &[low.clone(), high.clone()]);
    let identity = state.frozen()[0].memory_state_identity();

    let tables = vec![
        branch_owned_table(branch, BranchLevel::ZERO, "flush-cut-dup", vec![low]),
        branch_owned_table(branch, BranchLevel::ZERO, "flush-cut-dup", vec![high]),
    ];
    let error = state
        .replace_frozen_with_level_zero_tables_by_identity(identity, tables)
        .expect_err("duplicate identities must reject");
    assert!(matches!(
        error,
        BranchRuntimeError::InvalidBranchState { .. }
    ));

    assert_eq!(state.frozen_table_count(), 1, "frozen input untouched");
    assert_eq!(state.owned_table_count(), 0, "no partial install");
}

#[test]
fn install_n_requires_the_outputs_to_cover_the_frozen_rows() {
    let branch = branch_id(232);
    let (low, high) = two_disjoint_rows(branch);
    let mut state = frozen_state_with_rows(branch, &[low.clone(), high]);
    let identity = state.frozen()[0].memory_state_identity();

    // One output covering only half the frozen rows: row-count mismatch.
    let tables = vec![branch_owned_table(
        branch,
        BranchLevel::ZERO,
        "flush-cut-partial",
        vec![low],
    )];
    let error = state
        .replace_frozen_with_level_zero_tables_by_identity(identity, tables)
        .expect_err("partial coverage must reject");
    assert!(matches!(
        error,
        BranchRuntimeError::InvalidBranchState { .. }
    ));
    assert_eq!(state.frozen_table_count(), 1);
    assert_eq!(state.owned_table_count(), 0);
}

#[test]
fn frozen_rows_match_tables_accepts_an_exact_ordered_partition() {
    let branch = branch_id(233);
    let (low, high) = two_disjoint_rows(branch);
    let state = frozen_state_with_rows(branch, &[low.clone(), high.clone()]);
    let frozen = &state.frozen()[0];

    let table_low = branch_owned_table(branch, BranchLevel::ZERO, "verify-low", vec![low]);
    let table_high = branch_owned_table(branch, BranchLevel::ZERO, "verify-high", vec![high]);

    assert!(frozen_rows_match_tables(&[&table_low, &table_high], frozen));
    // Misordered segments are NOT a partition in key order.
    assert!(!frozen_rows_match_tables(
        &[&table_high, &table_low],
        frozen
    ));
    // A missing segment is a shortfall.
    assert!(!frozen_rows_match_tables(&[&table_low], frozen));
}

#[test]
fn frozen_rows_match_tables_rejects_mutated_and_duplicated_rows() {
    let branch = branch_id(234);
    let (low, high) = two_disjoint_rows(branch);
    let state = frozen_state_with_rows(branch, &[low.clone(), high.clone()]);
    let frozen = &state.frozen()[0];

    // Same row COUNT but a mutated value: per-row equality must fail.
    let mutated = storage_row_with(
        branch,
        b"zz-high".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"tampered".to_vec(),
    );
    let table_low = branch_owned_table(branch, BranchLevel::ZERO, "verify-low2", vec![low.clone()]);
    let table_mutated = branch_owned_table(branch, BranchLevel::ZERO, "verify-mut", vec![mutated]);
    assert!(!frozen_rows_match_tables(
        &[&table_low, &table_mutated],
        frozen
    ));

    // Same COUNT via duplication: totality-by-count alone must not pass.
    let table_dup_a =
        branch_owned_table(branch, BranchLevel::ZERO, "verify-dup-a", vec![low.clone()]);
    let table_dup_b = branch_owned_table(branch, BranchLevel::ZERO, "verify-dup-b", vec![low]);
    assert!(!frozen_rows_match_tables(
        &[&table_dup_a, &table_dup_b],
        frozen
    ));
}

/// Truth table for `contains_internal_key` — the probe behind both install
/// validation and the recovery fork-rebuild elision. One quadrant per home the
/// key can live in (active, frozen, owned) plus the absent case, and an
/// absent-key control against a populated stack.
#[test]
fn contains_internal_key_probes_the_whole_local_stack() {
    let branch = branch_id(235);
    let (low, high) = two_disjoint_rows(branch);
    let low_key = crate::table::TableInternalKeyBytes::from_row(&low);
    let high_key = crate::table::TableInternalKeyBytes::from_row(&high);

    let mut state = BranchLocalState::empty(branch);
    assert!(
        !state.contains_internal_key(&low_key).expect("probe empty"),
        "an empty stack holds nothing",
    );

    state.append_committed_row(low.clone()).expect("append");
    assert!(
        state.contains_internal_key(&low_key).expect("probe active"),
        "the active memtable hit must be seen",
    );

    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));
    assert!(
        state.contains_internal_key(&low_key).expect("probe frozen"),
        "the frozen memtable hit must be seen",
    );

    let identity = state.frozen()[0].memory_state_identity();
    state
        .replace_frozen_with_level_zero_tables_by_identity(
            identity,
            vec![branch_owned_table(
                branch,
                BranchLevel::ZERO,
                "probe-owned",
                vec![low],
            )],
        )
        .expect("install the owned table");
    assert!(
        state.contains_internal_key(&low_key).expect("probe owned"),
        "the owned-table hit must be seen",
    );
    assert!(
        !state
            .contains_internal_key(&high_key)
            .expect("probe absent"),
        "a key absent from every home must probe false",
    );
}
