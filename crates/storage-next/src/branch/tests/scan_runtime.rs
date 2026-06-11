#![allow(clippy::too_many_lines)]

use super::*;

fn scan_put(branch: BranchId, user_key: &str, version: u64, value: &str) -> StorageRow {
    storage_row_with(
        branch,
        user_key.as_bytes().to_vec(),
        version,
        version.saturating_mul(10),
        Timestamp::EPOCH,
        value.as_bytes().to_vec(),
    )
}

fn expiring_scan_put(
    branch: BranchId,
    user_key: &str,
    version: u64,
    timestamp: u64,
    expires_at: u64,
    value: &str,
) -> StorageRow {
    storage_row_with(
        branch,
        user_key.as_bytes().to_vec(),
        version,
        timestamp,
        Timestamp::from_micros(expires_at),
        value.as_bytes().to_vec(),
    )
}

fn scan_table(
    branch: BranchId,
    level: BranchLevel,
    identity: &str,
    rows: Vec<StorageRow>,
) -> BranchOwnedTable {
    branch_owned_table(branch, level, identity, rows)
}

fn prefix_bounds(branch: BranchId, prefix: &str) -> BranchScanBounds {
    BranchScanBounds::prefix(&physical_key(branch, prefix.as_bytes().to_vec()))
}

fn closed_bounds(branch: BranchId, lower: &str, upper: &str) -> BranchScanBounds {
    BranchScanBounds::closed(
        &physical_key(branch, lower.as_bytes().to_vec()),
        &physical_key(branch, upper.as_bytes().to_vec()),
    )
    .expect("closed scan bounds")
}

fn empty_range(branch: BranchId, key: &str) -> BranchScanBounds {
    BranchScanBounds::range(
        branch,
        "default",
        StorageSpaceId::engine(0x20).expect("engine storage space"),
        BranchUserKeyBound::included(key.as_bytes()),
        BranchUserKeyBound::excluded(key.as_bytes()),
    )
    .expect("empty scan range")
}

fn visible_values(rows: &[BranchVisibleRow]) -> Vec<Vec<u8>> {
    rows.iter().map(|row| row.row().value().to_vec()).collect()
}

fn history_values(rows: &[BranchHistoryRow]) -> Vec<Vec<u8>> {
    rows.iter().map(|row| row.row().value().to_vec()).collect()
}

#[test]
fn branch_prefix_and_range_scans_merge_all_source_families_in_key_order() {
    let branch = branch_id(48);
    let source = branch_id(49);
    let mut state = BranchLocalState::empty(branch);

    let inherited = branch_inherited_layer(
        source,
        CommitVersion::new(20),
        InheritedLayerStatus::Active,
        vec![
            Vec::new(),
            vec![scan_table(
                source,
                BranchLevel::new(1),
                "runtime-scan-inherited-nonzero",
                vec![
                    scan_put(source, "mix-a", 10, "inherited-a"),
                    scan_put(source, "mix-b", 10, "inherited-b"),
                    scan_put(source, "mix-c", 10, "inherited-c"),
                    scan_put(source, "mix-d", 10, "inherited-d"),
                    scan_put(source, "mix-e", 3, "inherited-e"),
                ],
            )],
        ],
    );
    state
        .attach_inherited_layers(vec![inherited])
        .expect("attach inherited layer");

    let frozen = scan_put(branch, "mix-b", 10, "frozen-b");
    state
        .append_committed_row(frozen.clone())
        .expect("append frozen row");
    assert!(matches!(
        state.rotate_active(),
        BranchRotationOutcome::Rotated { .. }
    ));

    let active = scan_put(branch, "mix-a", 10, "active-a");
    state
        .append_committed_row(active.clone())
        .expect("append active row");

    let l0 = scan_put(branch, "mix-c", 10, "l0-c");
    state
        .install_l0_table(scan_table(
            branch,
            BranchLevel::ZERO,
            "runtime-scan-l0",
            vec![l0.clone()],
        ))
        .expect("install L0 table");

    let nonzero_d = scan_put(branch, "mix-d", 10, "nonzero-d");
    let nonzero_f = scan_put(branch, "mix-f", 1, "nonzero-f");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            scan_table(
                branch,
                BranchLevel::new(1),
                "runtime-scan-owned-nonzero",
                vec![nonzero_d.clone(), nonzero_f.clone()],
            ),
        )
        .expect("install nonzero table");

    let view = state.capture_read_view().expect("capture read view");
    let prefix = view
        .scan_prefix(&prefix_bounds(branch, "mix-"), BranchReadBound::latest())
        .expect("prefix scan");

    assert_eq!(
        scan_user_keys(&prefix),
        vec![
            b"mix-a".to_vec(),
            b"mix-b".to_vec(),
            b"mix-c".to_vec(),
            b"mix-d".to_vec(),
            b"mix-e".to_vec(),
            b"mix-f".to_vec(),
        ]
    );
    assert_eq!(
        visible_values(&prefix),
        vec![
            b"active-a".to_vec(),
            b"frozen-b".to_vec(),
            b"l0-c".to_vec(),
            b"nonzero-d".to_vec(),
            b"inherited-e".to_vec(),
            b"nonzero-f".to_vec(),
        ]
    );
    assert_eq!(prefix[0].source(), BranchRowSource::Active);
    assert_eq!(prefix[1].source(), BranchRowSource::Frozen { index: 0 });
    assert_eq!(
        prefix[2].source(),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            table_index: 0,
        }
    );
    assert_eq!(
        prefix[3].source(),
        BranchRowSource::OwnedTable {
            level: BranchLevel::new(1),
            table_index: 0,
        }
    );
    assert_eq!(
        prefix[4].source(),
        BranchRowSource::Inherited {
            source_branch_id: source,
            layer_index: 0,
        }
    );
    assert_eq!(prefix[4].row().physical_key().branch_id(), branch);

    let range = view
        .scan_range(
            &closed_bounds(branch, "mix-b", "mix-e"),
            BranchReadBound::latest(),
        )
        .expect("range scan");
    assert_eq!(
        scan_user_keys(&range),
        vec![
            b"mix-b".to_vec(),
            b"mix-c".to_vec(),
            b"mix-d".to_vec(),
            b"mix-e".to_vec(),
        ]
    );
    assert_eq!(
        visible_values(&range),
        vec![
            b"frozen-b".to_vec(),
            b"l0-c".to_vec(),
            b"nonzero-d".to_vec(),
            b"inherited-e".to_vec(),
        ]
    );
}

#[test]
fn branch_scans_apply_fork_caps_tombstones_ttl_and_visible_limits() {
    let branch = branch_id(50);
    let source = branch_id(51);
    let mut state = BranchLocalState::empty(branch);

    let inherited = branch_inherited_layer_unchecked_for_fork_gate_tests(
        source,
        CommitVersion::new(4),
        InheritedLayerStatus::Active,
        vec![
            Vec::new(),
            vec![scan_table(
                source,
                BranchLevel::new(1),
                "runtime-scan-rules-inherited",
                vec![
                    scan_put(source, "rule-deleted", 2, "inherited-deleted"),
                    scan_put(source, "rule-fork", 6, "post-fork"),
                    scan_put(source, "rule-fork", 4, "at-fork"),
                ],
            )],
        ],
    );
    state
        .attach_inherited_layers(vec![inherited])
        .expect("attach inherited layer");

    let deleted = tombstone_row(branch, b"rule-deleted".to_vec(), 5, 20);
    state
        .append_committed_row(deleted.clone())
        .expect("append tombstone");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            scan_table(
                branch,
                BranchLevel::new(1),
                "runtime-scan-rules-owned",
                vec![
                    expiring_scan_put(branch, "rule-expired", 3, 15, 25, "expired"),
                    scan_put(branch, "rule-visible-a", 1, "visible-a"),
                    scan_put(branch, "rule-visible-b", 2, "visible-b"),
                    scan_put(branch, "rule-visible-c", 3, "visible-c"),
                ],
            ),
        )
        .expect("install owned table");

    let view = state.capture_read_view().expect("capture read view");
    let bounds = prefix_bounds(branch, "rule-");
    let latest = view
        .scan_prefix(&bounds, BranchReadBound::latest())
        .expect("latest scan");

    assert_eq!(
        scan_user_keys(&latest),
        vec![
            b"rule-expired".to_vec(),
            b"rule-fork".to_vec(),
            b"rule-visible-a".to_vec(),
            b"rule-visible-b".to_vec(),
            b"rule-visible-c".to_vec(),
        ]
    );
    assert_eq!(
        visible_values(&latest),
        vec![
            b"expired".to_vec(),
            b"at-fork".to_vec(),
            b"visible-a".to_vec(),
            b"visible-b".to_vec(),
            b"visible-c".to_vec(),
        ]
    );

    let timestamp_rows = view
        .scan_prefix(
            &bounds,
            BranchReadBound::at_timestamp(Timestamp::from_micros(30)),
        )
        .expect("timestamp scan");
    assert_eq!(
        scan_user_keys(&timestamp_rows),
        vec![
            b"rule-visible-a".to_vec(),
            b"rule-visible-b".to_vec(),
            b"rule-visible-c".to_vec(),
        ]
    );

    let limited = state
        .scan_including_tombstones_borrowed(
            &bounds,
            BranchReadBound::latest(),
            Some(2),
            Some(Timestamp::from_micros(30)),
        )
        .expect("limited borrowed scan");
    assert_eq!(
        history_user_keys(&limited),
        vec![
            b"rule-deleted".to_vec(),
            b"rule-expired".to_vec(),
            b"rule-fork".to_vec(),
            b"rule-visible-a".to_vec(),
        ]
    );
    assert!(limited[0].row().is_tombstone());
    assert_eq!(
        history_values(&limited),
        vec![
            Vec::<u8>::new(),
            b"expired".to_vec(),
            b"at-fork".to_vec(),
            b"visible-a".to_vec(),
        ]
    );
}

#[test]
fn branch_nonzero_scan_bounds_cover_empty_single_first_last_and_prefix_edges() {
    let branch = branch_id(52);
    let mut state = BranchLocalState::empty(branch);
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            scan_table(
                branch,
                BranchLevel::new(1),
                "runtime-scan-edge-a",
                vec![
                    scan_put(branch, "edge-000", 1, "first"),
                    scan_put(branch, "edge-001", 2, "second"),
                ],
            ),
        )
        .expect("install first edge table");
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            scan_table(
                branch,
                BranchLevel::new(1),
                "runtime-scan-edge-b",
                vec![
                    scan_put(branch, "edge-010", 3, "prefix-boundary"),
                    scan_put(branch, "edge-999", 4, "last"),
                ],
            ),
        )
        .expect("install last edge table");

    let view = state.capture_read_view().expect("capture read view");
    let edge_prefix = view
        .scan_prefix(&prefix_bounds(branch, "edge-00"), BranchReadBound::latest())
        .expect("edge prefix scan");
    assert_eq!(
        scan_user_keys(&edge_prefix),
        vec![b"edge-000".to_vec(), b"edge-001".to_vec()]
    );

    let boundary_prefix = view
        .scan_prefix(&prefix_bounds(branch, "edge-01"), BranchReadBound::latest())
        .expect("boundary prefix scan");
    assert_eq!(scan_user_keys(&boundary_prefix), vec![b"edge-010".to_vec()]);

    let single = view
        .scan_range(
            &closed_bounds(branch, "edge-001", "edge-001"),
            BranchReadBound::latest(),
        )
        .expect("single-key range");
    assert_eq!(scan_user_keys(&single), vec![b"edge-001".to_vec()]);

    let first = view
        .scan_range(
            &closed_bounds(branch, "edge-000", "edge-000"),
            BranchReadBound::latest(),
        )
        .expect("first-key range");
    assert_eq!(scan_user_keys(&first), vec![b"edge-000".to_vec()]);

    let last = view
        .scan_range(
            &closed_bounds(branch, "edge-999", "edge-999"),
            BranchReadBound::latest(),
        )
        .expect("last-key range");
    assert_eq!(scan_user_keys(&last), vec![b"edge-999".to_vec()]);

    let empty = view
        .scan_range(&empty_range(branch, "edge-002"), BranchReadBound::latest())
        .expect("empty range");
    assert!(empty.is_empty());
}
