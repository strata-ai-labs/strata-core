use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    FrozenTable, MutableTable, TableInternalKeyBytes, TableKeyBound, TableKeyBounds,
    TablePhysicalKeyBytes, TableRow, TableRuntimeError,
};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

fn branch(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn physical_key(branch_byte: u8, space_id: u8, user_key: impl Into<Vec<u8>>) -> PhysicalKey {
    physical_key_in_space(branch_byte, "default", space_id, user_key)
}

fn physical_key_in_space(
    branch_byte: u8,
    space: &str,
    space_id: u8,
    user_key: impl Into<Vec<u8>>,
) -> PhysicalKey {
    PhysicalKey::new(
        branch(branch_byte),
        space.to_owned(),
        StorageSpaceId::from_raw(space_id).expect("storage space id"),
        user_key,
    )
    .expect("physical key")
}

fn put_row(user_key: impl Into<Vec<u8>>, version: u64) -> StorageRow {
    StorageRow::put(
        physical_key(1, 0x20, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(version.saturating_add(10)),
        Timestamp::EPOCH,
        version.to_le_bytes().to_vec(),
    )
}

fn put_row_for_key(key: PhysicalKey, version: u64, value: Vec<u8>) -> StorageRow {
    StorageRow::put(
        key,
        CommitVersion::new(version),
        Timestamp::from_micros(version.saturating_add(10)),
        Timestamp::from_micros(version.saturating_add(100)),
        value,
    )
}

fn table_keys(table: &MutableTable) -> Vec<Vec<u8>> {
    table.iter().map(|row| row.encoded_key().to_vec()).collect()
}

fn frozen_keys(table: &FrozenTable) -> Vec<Vec<u8>> {
    table.iter().map(|row| row.encoded_key().to_vec()).collect()
}

fn mutable_bound_keys(table: &MutableTable, bounds: &TableKeyBounds) -> Vec<Vec<u8>> {
    table
        .rows_in_bounds(bounds)
        .map(|row| row.encoded_key().to_vec())
        .collect()
}

fn frozen_bound_keys(table: &FrozenTable, bounds: &TableKeyBounds) -> Vec<Vec<u8>> {
    table
        .rows_in_bounds(bounds)
        .map(|row| row.encoded_key().to_vec())
        .collect()
}

#[test]
fn mutable_table_empty_facts_and_empty_freeze_are_valid() {
    let table = MutableTable::new();
    assert_eq!(table.len(), 0);
    assert!(table.is_empty());
    assert_eq!(table.approximate_size_bytes(), 0);
    assert!(table.iter().next().is_none());
    assert!(table.first_key().is_none());
    assert!(table.last_key().is_none());
    assert!(table
        .get(&TableInternalKeyBytes::from_row(&put_row(
            b"missing".to_vec(),
            1
        )))
        .is_none());
    assert!(table
        .rows_in_bounds(&TableKeyBounds::unbounded())
        .next()
        .is_none());
    assert!(table
        .rows_with_physical_prefix(&TablePhysicalKeyBytes::from_physical_key(&physical_key(
            1,
            0x20,
            b"missing".to_vec()
        )))
        .next()
        .is_none());

    let facts = table.facts();
    assert_eq!(facts.row_count(), 0);
    assert_eq!(facts.approximate_size_bytes(), 0);
    assert!(facts.first_key().is_none());
    assert!(facts.last_key().is_none());
    assert!(facts.min_commit().is_none());
    assert!(facts.max_commit().is_none());

    let frozen = table.freeze();
    assert_eq!(frozen.len(), 0);
    assert!(frozen.is_empty());
    assert_eq!(frozen.facts(), facts);
    assert!(frozen.iter().next().is_none());
}

#[test]
fn mutable_table_insert_preserves_row_facts_and_updates_memory_facts() {
    let key = physical_key(2, 0x01, b"fact".to_vec());
    let row = StorageRow::put(
        key.clone(),
        CommitVersion::new(11),
        Timestamp::from_micros(70),
        Timestamp::from_micros(90),
        Vec::new(),
    );
    let table_row = TableRow::new(row.clone());
    let expected_size = table_row.approximate_size_bytes();

    let mut table = MutableTable::new();
    table.insert_row(row.clone()).expect("insert row");

    assert_eq!(table.len(), 1);
    assert!(!table.is_empty());
    assert_eq!(table.approximate_size_bytes(), expected_size);
    assert_eq!(table.first_key(), Some(table_row.key().clone()));
    assert_eq!(table.last_key(), Some(table_row.key().clone()));
    assert_eq!(table.get(table_row.key()).expect("stored row").row(), &row);

    let facts = table.facts();
    assert_eq!(facts.row_count(), 1);
    assert_eq!(facts.approximate_size_bytes(), expected_size);
    assert_eq!(facts.first_key(), Some(table_row.encoded_key()));
    assert_eq!(facts.last_key(), Some(table_row.encoded_key()));
    assert_eq!(facts.min_commit(), Some(CommitVersion::new(11)));
    assert_eq!(facts.max_commit(), Some(CommitVersion::new(11)));
}

#[test]
fn mutable_table_accepts_supported_row_shapes_and_tracks_facts() {
    let rows = vec![
        StorageRow::put(
            physical_key(1, 0x20, b"empty-value".to_vec()),
            CommitVersion::new(40),
            Timestamp::from_micros(400),
            Timestamp::EPOCH,
            Vec::new(),
        ),
        StorageRow::tombstone(
            physical_key(1, 0x20, b"tombstone".to_vec()),
            CommitVersion::new(7),
            Timestamp::from_micros(70),
        ),
        StorageRow::put(
            physical_key(1, 0x01, b"storage-owned".to_vec()),
            CommitVersion::new(99),
            Timestamp::from_micros(990),
            Timestamp::EPOCH,
            b"storage-owned".to_vec(),
        ),
        StorageRow::put(
            physical_key(1, 0x21, b"expired-looking".to_vec()),
            CommitVersion::new(2),
            Timestamp::from_micros(200),
            Timestamp::from_micros(1),
            b"expired".to_vec(),
        ),
    ];

    let mut table = MutableTable::new();
    let mut expected_size = 0usize;
    for row in &rows {
        let table_row = TableRow::new(row.clone());
        expected_size = expected_size.saturating_add(table_row.approximate_size_bytes());

        table.insert_row(row.clone()).expect("insert row shape");

        assert_eq!(
            table.get(table_row.key()).as_deref().map(TableRow::row),
            Some(row),
            "insert must preserve row facts exactly"
        );
        assert_eq!(table.approximate_size_bytes(), expected_size);
    }

    let facts = table.facts();
    assert_eq!(facts.row_count(), rows.len());
    assert_eq!(facts.approximate_size_bytes(), expected_size);
    assert_eq!(facts.min_commit(), Some(CommitVersion::new(2)));
    assert_eq!(facts.max_commit(), Some(CommitVersion::new(99)));
    assert!(table.iter().any(|row| row.is_tombstone()));
    assert!(table
        .iter()
        .any(|row| row.expires_at() == Timestamp::from_micros(1)));
}

#[test]
fn mutable_table_iterates_by_encoded_internal_key_order() {
    let same_key = physical_key(1, 0x20, b"same".to_vec());
    let rows = vec![
        put_row(b"charlie".to_vec(), 2),
        put_row_for_key(same_key.clone(), 3, b"older".to_vec()),
        put_row(b"alpha".to_vec(), 2),
        put_row_for_key(same_key, 9, b"newer".to_vec()),
        StorageRow::tombstone(
            physical_key(3, 0x20, [0x00, 0x41]),
            CommitVersion::new(4),
            Timestamp::from_micros(1),
        ),
        put_row_for_key(physical_key(2, 0x01, b"owned".to_vec()), 1, Vec::new()),
    ];
    let mut expected = rows.iter().cloned().map(TableRow::new).collect::<Vec<_>>();
    expected.sort();

    let mut table = MutableTable::new();
    for row in rows {
        table.insert_row(row).expect("insert generated row");
    }

    let actual = table_keys(&table);
    assert_eq!(
        actual,
        expected
            .iter()
            .map(|row| row.encoded_key().to_vec())
            .collect::<Vec<_>>()
    );

    let same_prefix =
        TablePhysicalKeyBytes::from_physical_key(&physical_key(1, 0x20, b"same".to_vec()));
    let versions = table
        .rows_with_physical_prefix(&same_prefix)
        .map(|row| row.commit_version())
        .collect::<Vec<_>>();
    assert_eq!(versions, vec![CommitVersion::new(9), CommitVersion::new(3)]);
}

#[test]
fn mutable_table_order_uses_encoded_key_not_timestamps_or_expiry() {
    let rows = vec![
        StorageRow::put(
            physical_key_in_space(2, "z-space", 0x20, b"late-branch".to_vec()),
            CommitVersion::new(1),
            Timestamp::from_micros(1),
            Timestamp::from_micros(1),
            b"a".to_vec(),
        ),
        StorageRow::put(
            physical_key_in_space(1, "b-space", 0x20, b"middle-space".to_vec()),
            CommitVersion::new(50),
            Timestamp::from_micros(9_000),
            Timestamp::from_micros(1),
            b"b".to_vec(),
        ),
        StorageRow::put(
            physical_key_in_space(1, "a-space", 0x21, b"later-storage-id".to_vec()),
            CommitVersion::new(7),
            Timestamp::from_micros(8_000),
            Timestamp::from_micros(100_000),
            b"c".to_vec(),
        ),
        StorageRow::put(
            physical_key_in_space(1, "a-space", 0x20, [0x00, 0x41]),
            CommitVersion::new(3),
            Timestamp::from_micros(7_000),
            Timestamp::from_micros(50_000),
            b"d".to_vec(),
        ),
    ];
    let mut expected = rows.iter().cloned().map(TableRow::new).collect::<Vec<_>>();
    expected.sort();

    let mut table = MutableTable::new();
    for row in rows {
        table.insert_row(row).expect("insert row");
    }

    assert_eq!(
        table_keys(&table),
        expected
            .iter()
            .map(|row| row.encoded_key().to_vec())
            .collect::<Vec<_>>()
    );
    assert_eq!(table.facts().min_commit(), Some(CommitVersion::new(1)));
    assert_eq!(table.facts().max_commit(), Some(CommitVersion::new(50)));
}

#[test]
fn mutable_table_duplicate_insert_is_non_mutating() {
    let key = physical_key(1, 0x20, b"dupe".to_vec());
    let original = put_row_for_key(key.clone(), 7, b"original".to_vec());
    let duplicate = put_row_for_key(key, 7, b"different".to_vec());
    let original_table_row = TableRow::new(original.clone());

    let mut table = MutableTable::new();
    table.insert_row(original).expect("first insert");
    let before_facts = table.facts();
    let before_size = table.approximate_size_bytes();
    let before_keys = table_keys(&table);

    let error = table
        .insert_row(duplicate)
        .expect_err("duplicate internal key should fail");
    assert!(matches!(
        error,
        TableRuntimeError::DuplicateInternalKey { .. }
    ));
    assert_eq!(table.facts(), before_facts);
    assert_eq!(table.approximate_size_bytes(), before_size);
    assert_eq!(table_keys(&table), before_keys);
    assert_eq!(
        table
            .get(original_table_row.key())
            .expect("original still present")
            .value(),
        b"original"
    );
}

#[test]
fn mutable_table_exact_lookup_covers_present_and_absent_keys() {
    let physical = physical_key(1, 0x20, b"versioned".to_vec());
    let rows = vec![
        put_row(b"alpha".to_vec(), 11),
        StorageRow::tombstone(
            physical.clone(),
            CommitVersion::new(9),
            Timestamp::from_micros(9),
        ),
        put_row_for_key(
            physical_key(1, 0x20, b"expired".to_vec()),
            7,
            b"expired".to_vec(),
        ),
        put_row(b"omega".to_vec(), 3),
    ];
    let mut expected = rows.iter().cloned().map(TableRow::new).collect::<Vec<_>>();
    expected.sort();

    let mut table = MutableTable::new();
    for row in rows {
        table.insert_row(row).expect("insert row");
    }

    for table_row in [
        expected.first().expect("first"),
        &expected[expected.len() / 2],
        expected.last().expect("last"),
    ] {
        assert_eq!(
            table.get(table_row.key()).as_deref().map(TableRow::row),
            Some(table_row.row())
        );
    }

    let before_first = TableInternalKeyBytes::from_row(&put_row_for_key(
        physical_key(0, 0x20, b"before".to_vec()),
        1,
        Vec::new(),
    ));
    let after_last = TableInternalKeyBytes::from_row(&put_row_for_key(
        physical_key(9, 0x20, b"after".to_vec()),
        1,
        Vec::new(),
    ));
    let absent_same_physical =
        TableInternalKeyBytes::from_row(&put_row_for_key(physical, 42, Vec::new()));
    assert!(table.get(&before_first).is_none());
    assert!(table.get(&after_last).is_none());
    assert!(table.get(&absent_same_physical).is_none());

    assert!(table
        .get(
            expected
                .iter()
                .find(|row| row.is_tombstone())
                .expect("tombstone")
                .key()
        )
        .expect("tombstone row")
        .is_tombstone());
    assert_eq!(
        table
            .get(
                TableRow::new(put_row_for_key(
                    physical_key(1, 0x20, b"expired".to_vec()),
                    7,
                    Vec::new()
                ))
                .key()
            )
            .expect("expired-looking row")
            .expires_at(),
        Timestamp::from_micros(107)
    );
}

#[test]
fn mutable_table_key_bound_lookup_covers_bound_variants() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        StorageRow::tombstone(
            physical_key(1, 0x20, b"bravo".to_vec()),
            CommitVersion::new(2),
            Timestamp::from_micros(2),
        ),
        StorageRow::put(
            physical_key(1, 0x20, b"charlie".to_vec()),
            CommitVersion::new(3),
            Timestamp::from_micros(3),
            Timestamp::from_micros(1),
            b"expired".to_vec(),
        ),
        put_row(b"delta".to_vec(), 4),
    ];
    let mut expected = rows.iter().cloned().map(TableRow::new).collect::<Vec<_>>();
    expected.sort();
    let expected_keys = expected
        .iter()
        .map(|row| row.encoded_key().to_vec())
        .collect::<Vec<_>>();

    let mut table = MutableTable::new();
    for row in rows {
        table.insert_row(row).expect("insert row");
    }

    let keys = expected
        .iter()
        .map(|row| row.key().clone())
        .collect::<Vec<_>>();
    let absent = TableInternalKeyBytes::from_row(&put_row(b"absent".to_vec(), 88));

    assert_eq!(
        mutable_bound_keys(&table, &TableKeyBounds::unbounded()),
        expected_keys
    );
    assert_eq!(
        mutable_bound_keys(&table, &TableKeyBounds::exact(keys[1].clone())),
        vec![expected_keys[1].clone()]
    );
    assert!(mutable_bound_keys(&table, &TableKeyBounds::exact(absent)).is_empty());
    assert_eq!(
        mutable_bound_keys(
            &table,
            &TableKeyBounds::closed(keys[1].clone(), keys[2].clone()).expect("closed range")
        ),
        expected_keys[1..=2].to_vec()
    );
    assert_eq!(
        mutable_bound_keys(
            &table,
            &TableKeyBounds::open(keys[0].clone(), keys[3].clone()).expect("open range")
        ),
        expected_keys[1..=2].to_vec()
    );
    assert_eq!(
        mutable_bound_keys(
            &table,
            &TableKeyBounds::range(
                TableKeyBound::Unbounded,
                TableKeyBound::included(keys[1].clone())
            )
            .expect("lower-unbounded range")
        ),
        expected_keys[..=1].to_vec()
    );
    assert_eq!(
        mutable_bound_keys(
            &table,
            &TableKeyBounds::range(
                TableKeyBound::excluded(keys[1].clone()),
                TableKeyBound::Unbounded
            )
            .expect("upper-unbounded range")
        ),
        expected_keys[2..].to_vec()
    );
    assert_eq!(
        mutable_bound_keys(
            &table,
            &TableKeyBounds::closed(keys[2].clone(), keys[2].clone()).expect("equal closed")
        ),
        vec![expected_keys[2].clone()]
    );
    assert!(mutable_bound_keys(
        &table,
        &TableKeyBounds::open(keys[2].clone(), keys[2].clone()).expect("equal open")
    )
    .is_empty());
    assert!(table
        .rows_in_bounds(&TableKeyBounds::unbounded())
        .any(|row| row.is_tombstone()));
    assert!(table
        .rows_in_bounds(&TableKeyBounds::unbounded())
        .any(|row| row.expires_at() == Timestamp::from_micros(1)));
}

#[test]
fn mutable_table_physical_prefix_lookup_is_exact_and_mechanical() {
    let physical = physical_key(1, 0x20, b"subject".to_vec());
    let rows = [
        put_row_for_key(physical.clone(), 9, b"new".to_vec()),
        StorageRow::tombstone(
            physical.clone(),
            CommitVersion::new(5),
            Timestamp::from_micros(5),
        ),
        StorageRow::put(
            physical.clone(),
            CommitVersion::new(1),
            Timestamp::from_micros(1),
            Timestamp::from_micros(1),
            b"expired".to_vec(),
        ),
        put_row_for_key(
            physical_key(1, 0x20, b"subject-adjacent".to_vec()),
            8,
            Vec::new(),
        ),
        put_row_for_key(physical_key(2, 0x20, b"subject".to_vec()), 8, Vec::new()),
        put_row_for_key(
            physical_key_in_space(1, "other", 0x20, b"subject".to_vec()),
            8,
            Vec::new(),
        ),
        put_row_for_key(physical_key(1, 0x21, b"subject".to_vec()), 8, Vec::new()),
    ];
    let mut table = MutableTable::new();
    for row in rows {
        table.insert_row(row).expect("insert row");
    }

    let prefix = TablePhysicalKeyBytes::from_physical_key(&physical);
    let prefix_rows = table.rows_with_physical_prefix(&prefix).collect::<Vec<_>>();
    assert_eq!(
        prefix_rows
            .iter()
            .map(|row| row.commit_version())
            .collect::<Vec<_>>(),
        vec![
            CommitVersion::new(9),
            CommitVersion::new(5),
            CommitVersion::new(1)
        ]
    );
    assert!(prefix_rows.iter().any(|row| row.is_tombstone()));
    assert!(prefix_rows
        .iter()
        .any(|row| row.expires_at() == Timestamp::from_micros(1)));
    assert_eq!(
        prefix_rows
            .iter()
            .filter(|row| row.physical_key() == &physical)
            .count(),
        3
    );

    assert!(table
        .rows_with_physical_prefix(&TablePhysicalKeyBytes::from_physical_key(&physical_key(
            1,
            0x20,
            b"missing".to_vec()
        )))
        .next()
        .is_none());
}

#[test]
fn mutable_table_exact_bounds_and_prefix_reads_are_mechanical() {
    let physical = physical_key(1, 0x20, b"subject".to_vec());
    let rows = [
        put_row_for_key(physical.clone(), 9, b"new".to_vec()),
        StorageRow::tombstone(
            physical.clone(),
            CommitVersion::new(5),
            Timestamp::from_micros(5),
        ),
        put_row_for_key(physical.clone(), 1, b"old".to_vec()),
        put_row_for_key(
            physical_key(1, 0x20, b"subject-adjacent".to_vec()),
            8,
            Vec::new(),
        ),
        put_row_for_key(physical_key(2, 0x20, b"subject".to_vec()), 8, Vec::new()),
    ];
    let mut table = MutableTable::new();
    for row in rows {
        table.insert_row(row).expect("insert row");
    }

    let prefix = TablePhysicalKeyBytes::from_physical_key(&physical);
    let prefix_rows = table.rows_with_physical_prefix(&prefix).collect::<Vec<_>>();
    assert_eq!(prefix_rows.len(), 3);
    assert_eq!(
        prefix_rows
            .iter()
            .map(|row| row.commit_version())
            .collect::<Vec<_>>(),
        vec![
            CommitVersion::new(9),
            CommitVersion::new(5),
            CommitVersion::new(1)
        ]
    );
    assert!(prefix_rows.iter().any(|row| row.is_tombstone()));

    let first = table.first_key().expect("first").clone();
    let last = table.last_key().expect("last").clone();
    let bounded = table
        .rows_in_bounds(
            &TableKeyBounds::range(
                TableKeyBound::included(first.clone()),
                TableKeyBound::excluded(last.clone()),
            )
            .expect("range"),
        )
        .map(|row| row.encoded_key().to_vec())
        .collect::<Vec<_>>();
    let expected = table
        .iter()
        .filter(|row| row.key() >= &first && row.key() < &last)
        .map(|row| row.encoded_key().to_vec())
        .collect::<Vec<_>>();
    assert_eq!(bounded, expected);

    let missing_version =
        TableInternalKeyBytes::from_row(&put_row_for_key(physical, 42, Vec::new()));
    assert!(table.get(&missing_version).is_none());
}

#[test]
fn freeze_preserves_rows_facts_and_read_helpers() {
    let mut table = MutableTable::new();
    for row in [
        put_row(b"bravo".to_vec(), 2),
        put_row(b"alpha".to_vec(), 7),
        StorageRow::tombstone(
            physical_key(1, 0x20, b"charlie".to_vec()),
            CommitVersion::new(4),
            Timestamp::from_micros(44),
        ),
    ] {
        table.insert_row(row).expect("insert row");
    }

    let facts = table.facts();
    let keys = table_keys(&table);
    let first = table.first_key().expect("first").clone();
    let prefix = TablePhysicalKeyBytes::from_row(table.iter().next().expect("row").row());
    let prefix_count = table.rows_with_physical_prefix(&prefix).count();
    let bounded_count = table
        .rows_in_bounds(&TableKeyBounds::exact(first.clone()))
        .count();

    let frozen = table.freeze();
    assert_eq!(frozen.facts(), facts);
    assert_eq!(
        frozen.approximate_size_bytes(),
        facts.approximate_size_bytes()
    );
    assert_eq!(frozen_keys(&frozen), keys);
    assert_eq!(frozen.first_key(), Some(first.clone()));
    let frozen_last_key = frozen.last_key();
    assert_eq!(
        frozen_last_key
            .as_ref()
            .map(TableInternalKeyBytes::as_slice),
        facts.last_key()
    );
    assert_eq!(
        frozen.rows_with_physical_prefix(&prefix).count(),
        prefix_count
    );
    assert_eq!(
        frozen.rows_in_bounds(&TableKeyBounds::exact(first)).count(),
        bounded_count
    );
    for key in &keys {
        let key = TableInternalKeyBytes::from_canonical_bytes(key.clone()).expect("canonical key");
        assert!(frozen.get(&key).is_some());
    }
    assert_eq!(
        frozen_bound_keys(&frozen, &TableKeyBounds::unbounded()),
        keys
    );
    assert!(format!("{frozen:?}").contains("approximate_size_bytes"));
    assert!(!format!("{frozen:?}").contains("value:"));
}
