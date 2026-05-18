use crate::format::{decode_internal_key, encode_internal_key, encode_physical_key};
use crate::row::{InternalKey, PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    first_table_key, last_table_key, sort_table_rows_by_key, validate_strictly_sorted_unique_keys,
    validate_strictly_sorted_unique_rows, TableInternalKeyBytes, TableKeyBound, TableKeyBounds,
    TablePhysicalKeyBytes, TableRow, TableRuntimeError,
};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

fn branch(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn branch_from_bytes(bytes: [u8; BranchId::BYTE_LEN]) -> BranchId {
    BranchId::from_bytes(bytes)
}

fn physical_key(
    branch_byte: u8,
    space: &str,
    storage_space_id: u8,
    user_key: impl Into<Vec<u8>>,
) -> PhysicalKey {
    physical_key_for_branch(branch(branch_byte), space, storage_space_id, user_key)
}

fn physical_key_for_branch(
    branch_id: BranchId,
    space: &str,
    storage_space_id: u8,
    user_key: impl Into<Vec<u8>>,
) -> PhysicalKey {
    PhysicalKey::new(
        branch_id,
        space,
        StorageSpaceId::from_raw(storage_space_id).expect("storage space id"),
        user_key,
    )
    .expect("physical key")
}

fn put_row(user_key: impl Into<Vec<u8>>, version: u64) -> StorageRow {
    StorageRow::put(
        physical_key(1, "default", 0x20, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(version.saturating_add(10)),
        Timestamp::EPOCH,
        b"value".to_vec(),
    )
}

fn put_row_for_key(physical_key: PhysicalKey, version: u64, value: Vec<u8>) -> StorageRow {
    StorageRow::put(
        physical_key,
        CommitVersion::new(version),
        Timestamp::from_micros(version.saturating_add(10)),
        Timestamp::from_micros(version.saturating_add(100)),
        value,
    )
}

fn table_key_for_branch(
    branch_id: BranchId,
    space: &str,
    storage_space_id: u8,
    user_key: impl Into<Vec<u8>>,
    version: u64,
) -> TableInternalKeyBytes {
    TableInternalKeyBytes::from_row(&put_row_for_key(
        physical_key_for_branch(branch_id, space, storage_space_id, user_key),
        version,
        Vec::new(),
    ))
}

fn table_key(
    branch_byte: u8,
    space: &str,
    storage_space_id: u8,
    user_key: impl Into<Vec<u8>>,
    version: u64,
) -> TableInternalKeyBytes {
    table_key_for_branch(
        branch(branch_byte),
        space,
        storage_space_id,
        user_key,
        version,
    )
}

#[test]
fn table_internal_key_bytes_match_v1_format_for_put_and_tombstone() {
    let put = put_row(b"alpha".to_vec(), 7);
    let tombstone = StorageRow::tombstone(
        physical_key(1, "default", 0x20, b"beta".to_vec()),
        CommitVersion::new(8),
        Timestamp::from_micros(18),
    );

    for row in [&put, &tombstone] {
        let expected = encode_internal_key(&InternalKey::new(
            row.physical_key().clone(),
            row.commit_version(),
        ));
        let key = TableInternalKeyBytes::from_row(row);

        assert_eq!(key.as_slice(), expected.as_slice());
        assert_eq!(
            decode_internal_key(key.as_slice()).expect("decode internal key"),
            InternalKey::new(row.physical_key().clone(), row.commit_version())
        );
        assert_eq!(
            TableInternalKeyBytes::from_canonical_bytes(key.as_slice().to_vec()),
            Ok(key)
        );
    }
}

#[test]
fn table_internal_key_bytes_reject_invalid_raw_bytes() {
    assert!(matches!(
        TableInternalKeyBytes::from_canonical_bytes([0x01, 0x02, 0x03]),
        Err(TableRuntimeError::DecodeFormat { .. })
    ));

    let mut invalid_storage_space = TableInternalKeyBytes::from_row(&put_row(b"raw".to_vec(), 1))
        .as_slice()
        .to_vec();
    let storage_space_offset = BranchId::BYTE_LEN + "default".len() + 1;
    invalid_storage_space[storage_space_offset] = 0x00;
    assert!(matches!(
        TableInternalKeyBytes::from_canonical_bytes(invalid_storage_space),
        Err(TableRuntimeError::DecodeFormat { .. })
    ));

    let mut trailing_before_suffix = TableInternalKeyBytes::from_row(&put_row(b"raw".to_vec(), 1))
        .as_slice()
        .to_vec();
    let suffix_offset = trailing_before_suffix.len() - 8;
    trailing_before_suffix.insert(suffix_offset, 0xff);
    assert!(matches!(
        TableInternalKeyBytes::from_canonical_bytes(trailing_before_suffix),
        Err(TableRuntimeError::DecodeFormat { .. })
    ));
}

#[test]
fn table_internal_key_ordering_uses_branch_bytes() {
    let min_branch = TableInternalKeyBytes::from_row(&put_row_for_key(
        physical_key_for_branch(
            branch_from_bytes([0x00; BranchId::BYTE_LEN]),
            "default",
            0x20,
            b"k",
        ),
        1,
        Vec::new(),
    ));
    let max_branch = TableInternalKeyBytes::from_row(&put_row_for_key(
        physical_key_for_branch(
            branch_from_bytes([0xff; BranchId::BYTE_LEN]),
            "default",
            0x20,
            b"k",
        ),
        1,
        Vec::new(),
    ));
    assert!(min_branch < max_branch);

    let first_byte_low = [0x01; BranchId::BYTE_LEN];
    let mut first_byte_high = first_byte_low;
    first_byte_high[0] = 0x02;
    let first_byte_low_key = TableInternalKeyBytes::from_row(&put_row_for_key(
        physical_key_for_branch(branch_from_bytes(first_byte_low), "default", 0x20, b"k"),
        1,
        Vec::new(),
    ));
    let first_byte_high_key = TableInternalKeyBytes::from_row(&put_row_for_key(
        physical_key_for_branch(branch_from_bytes(first_byte_high), "default", 0x20, b"k"),
        1,
        Vec::new(),
    ));
    assert!(first_byte_low_key < first_byte_high_key);

    let last_byte_low = [0x01; BranchId::BYTE_LEN];
    let mut last_byte_high = last_byte_low;
    last_byte_high[BranchId::BYTE_LEN - 1] = 0x02;
    let last_byte_low_key = TableInternalKeyBytes::from_row(&put_row_for_key(
        physical_key_for_branch(branch_from_bytes(last_byte_low), "default", 0x20, b"k"),
        1,
        Vec::new(),
    ));
    let last_byte_high_key = TableInternalKeyBytes::from_row(&put_row_for_key(
        physical_key_for_branch(branch_from_bytes(last_byte_high), "default", 0x20, b"k"),
        1,
        Vec::new(),
    ));
    assert!(last_byte_low_key < last_byte_high_key);

    let lower_branch = table_key(1, "default", 0x20, b"same".to_vec(), 1);
    let higher_branch = table_key(2, "default", 0x20, b"same".to_vec(), 1);
    assert!(lower_branch < higher_branch);
}

#[test]
fn table_internal_key_ordering_uses_space_bytes() {
    let alpha_space = table_key(1, "alpha", 0x20, b"same".to_vec(), 1);
    let beta_space = table_key(1, "beta", 0x20, b"same".to_vec(), 1);
    assert!(alpha_space < beta_space);

    let short_space = table_key(1, "a", 0x20, b"same".to_vec(), 1);
    let longer_space = table_key(1, "aa", 0x20, b"same".to_vec(), 1);
    let unicode_space = table_key(1, "space-\u{03bb}", 0x20, b"same".to_vec(), 1);
    assert!(short_space < longer_space);
    assert!(beta_space < unicode_space);
}

#[test]
fn table_internal_key_ordering_uses_storage_space_id_bytes() {
    let storage_owned = table_key(1, "default", 0x01, b"same".to_vec(), 1);
    let storage_reserved = table_key(1, "default", 0x02, b"same".to_vec(), 1);
    assert!(storage_owned < storage_reserved);

    let storage_reserved_high = table_key(1, "default", 0x1f, b"same".to_vec(), 1);
    let engine_low = table_key(1, "default", 0x20, b"same".to_vec(), 1);
    let engine_high = table_key(1, "default", 0xff, b"same".to_vec(), 1);
    assert!(storage_reserved < storage_reserved_high);
    assert!(storage_reserved_high < engine_low);
    assert!(engine_low < engine_high);
}

#[test]
fn table_internal_key_ordering_uses_escaped_user_key_bytes() {
    let empty_user_key = table_key(1, "default", 0x20, Vec::new(), 1);
    let zero_user_byte = table_key(1, "default", 0x20, [0x00, 0x41], 1);
    let one_user_byte = table_key(1, "default", 0x20, [0x01], 1);
    let double_zero_user_byte = table_key(1, "default", 0x20, [0x00, 0x00], 1);
    let high_user_byte = table_key(1, "default", 0x20, [0xff], 1);
    assert!(empty_user_key < zero_user_byte);
    assert!(zero_user_byte < one_user_byte);
    assert!(double_zero_user_byte < one_user_byte);
    assert!(one_user_byte < high_user_byte);
}

#[test]
fn table_internal_key_ordering_uses_descending_commit_version_suffix() {
    let same_key = physical_key(1, "default", 0x20, b"same".to_vec());
    let newer = TableInternalKeyBytes::from_row(&put_row_for_key(same_key.clone(), 42, Vec::new()));
    let older = TableInternalKeyBytes::from_row(&put_row_for_key(same_key, 7, Vec::new()));
    assert!(newer < older);

    let max = TableInternalKeyBytes::from_row(&put_row(b"version".to_vec(), u64::MAX));
    let zero = TableInternalKeyBytes::from_row(&put_row(b"version".to_vec(), 0));
    assert_eq!(max.commit_version(), Ok(CommitVersion::MAX));
    assert!(max < zero);
}

#[test]
fn table_physical_key_bytes_are_exact_internal_key_prefixes() {
    let source_key = physical_key(3, "default", 0x20, [0x00, 0x41, 0x00]);
    let prefix = TablePhysicalKeyBytes::from_physical_key(&source_key);
    let newer = TableInternalKeyBytes::from_internal_key(&InternalKey::new(
        source_key.clone(),
        CommitVersion::new(9),
    ));
    let older = TableInternalKeyBytes::from_internal_key(&InternalKey::new(
        source_key.clone(),
        CommitVersion::new(1),
    ));

    assert_eq!(prefix.as_slice(), encode_physical_key(&source_key));
    assert!(prefix.is_prefix_of(&newer));
    assert!(prefix.is_prefix_of(&older));
    assert!(newer.as_slice().starts_with(prefix.as_slice()));
    assert_eq!(prefix.len() + 8, newer.len());

    let adjacent = TableInternalKeyBytes::from_row(&put_row_for_key(
        physical_key(4, "default", 0x20, [0x00, 0x41, 0x00]),
        9,
        Vec::new(),
    ));
    assert!(!prefix.is_prefix_of(&adjacent));

    let different_storage_space = TableInternalKeyBytes::from_row(&put_row_for_key(
        physical_key(3, "default", 0x21, [0x00, 0x41, 0x00]),
        9,
        Vec::new(),
    ));
    assert!(!prefix.is_prefix_of(&different_storage_space));
}

#[test]
fn table_row_preserves_storage_row_facts_without_interpretation() {
    let source_key = physical_key(5, "default", 0x01, b"fact".to_vec());
    let row = StorageRow::put(
        source_key.clone(),
        CommitVersion::new(11),
        Timestamp::from_micros(70),
        Timestamp::from_micros(90),
        Vec::new(),
    );
    let table_row = TableRow::new(row.clone());

    assert_eq!(table_row.row(), &row);
    assert_eq!(table_row.physical_key(), &source_key);
    assert_eq!(table_row.commit_version(), CommitVersion::new(11));
    assert_eq!(table_row.commit_timestamp(), Timestamp::from_micros(70));
    assert_eq!(table_row.expires_at(), Timestamp::from_micros(90));
    assert_eq!(table_row.value(), b"");
    assert_eq!(
        table_row.key().physical_key().expect("decode table key"),
        source_key
    );
    assert_eq!(
        table_row.key().clone().into_vec(),
        TableInternalKeyBytes::from_row(&row).into_vec()
    );
    assert!(!table_row.is_tombstone());
    assert_eq!(
        table_row.encoded_key(),
        TableInternalKeyBytes::from_row(&row).as_slice()
    );
    assert!(table_row.approximate_size_bytes() > table_row.encoded_key().len());

    let tombstone = TableRow::new(StorageRow::tombstone(
        source_key,
        CommitVersion::new(12),
        Timestamp::from_micros(71),
    ));
    assert!(tombstone.is_tombstone());
    assert!(tombstone.value().is_empty());
    assert!(tombstone.approximate_size_bytes() > tombstone.encoded_key().len());
    assert_eq!(
        tombstone.clone().into_row().is_tombstone(),
        tombstone.is_tombstone()
    );

    let engine_row = TableRow::new(StorageRow::put(
        physical_key(5, "default", 0xff, b"engine".to_vec()),
        CommitVersion::new(13),
        Timestamp::from_micros(70),
        Timestamp::from_micros(90),
        b"engine bytes".to_vec(),
    ));
    assert!(engine_row
        .physical_key()
        .storage_space_id()
        .is_engine_owned());
    assert_eq!(engine_row.value(), b"engine bytes");

    let duplicate_version = TableRow::new(StorageRow::put(
        engine_row.physical_key().clone(),
        engine_row.commit_version(),
        Timestamp::from_micros(999),
        Timestamp::EPOCH,
        b"different value".to_vec(),
    ));
    assert_eq!(engine_row.key(), duplicate_version.key());
    assert_ne!(engine_row.row(), duplicate_version.row());

    let debug = format!("{engine_row:?}");
    assert!(debug.contains("value_len"));
    assert!(!debug.contains("value:"));
    assert!(!debug.contains("[101, 110, 103"));
    assert!(
        !debug.contains("engine bytes"),
        "debug output should not print payload text: {debug}"
    );
}

#[test]
fn sorted_unique_validation_rejects_unsorted_and_duplicate_internal_keys_only() {
    validate_strictly_sorted_unique_rows(&[]).expect("empty generic validation is valid");

    let single = vec![TableRow::new(put_row(b"single".to_vec(), 1))];
    validate_strictly_sorted_unique_rows(&single).expect("one row is sorted");

    let same_key = physical_key(1, "default", 0x20, b"same".to_vec());
    let newer = TableRow::new(put_row_for_key(same_key.clone(), 9, Vec::new()));
    let older = TableRow::new(put_row_for_key(same_key.clone(), 3, Vec::new()));
    let later_key = TableRow::new(put_row(b"zulu".to_vec(), 1));

    let mut sorted = vec![later_key.clone(), older.clone(), newer.clone()];
    sort_table_rows_by_key(&mut sorted);
    assert_eq!(first_table_key(&sorted), Some(newer.key()));
    assert_eq!(last_table_key(&sorted), Some(later_key.key()));
    validate_strictly_sorted_unique_rows(&sorted).expect("sorted unique rows");
    validate_strictly_sorted_unique_keys(sorted.iter().map(TableRow::key))
        .expect("sorted unique keys");

    let unsorted = vec![older.clone(), newer.clone()];
    assert!(matches!(
        validate_strictly_sorted_unique_rows(&unsorted),
        Err(TableRuntimeError::InvalidRowOrder { .. })
    ));

    let duplicate = TableRow::new(put_row_for_key(same_key, 9, b"different bytes".to_vec()));
    assert_eq!(newer.cmp(&duplicate), std::cmp::Ordering::Equal);
    assert_eq!(
        newer, duplicate,
        "TableRow Eq must match Ord and is therefore keyed by encoded internal key"
    );
    let duplicates = vec![newer, duplicate];
    assert!(matches!(
        validate_strictly_sorted_unique_rows(&duplicates),
        Err(TableRuntimeError::DuplicateInternalKey { .. })
    ));

    let non_adjacent_unsorted = vec![
        TableRow::new(put_row(b"alpha".to_vec(), 1)),
        TableRow::new(put_row(b"charlie".to_vec(), 1)),
        TableRow::new(put_row(b"bravo".to_vec(), 1)),
    ];
    let error = validate_strictly_sorted_unique_rows(&non_adjacent_unsorted)
        .expect_err("non-adjacent unsorted input should be rejected");
    assert!(matches!(error, TableRuntimeError::InvalidRowOrder { .. }));
    assert!(error.to_string().contains("out of order"));

    let duplicate_error = validate_strictly_sorted_unique_rows(&duplicates)
        .expect_err("duplicate input should be rejected");
    assert!(duplicate_error.to_string().contains("duplicated"));
}

#[test]
fn key_bounds_match_encoded_byte_filtering() {
    let mut rows = vec![
        TableRow::new(put_row(b"alpha".to_vec(), 2)),
        TableRow::new(put_row(b"bravo".to_vec(), 2)),
        TableRow::new(put_row(b"charlie".to_vec(), 2)),
    ];
    sort_table_rows_by_key(&mut rows);
    let alpha = rows[0].key().clone();
    let bravo = rows[1].key().clone();
    let charlie = rows[2].key().clone();

    let unbounded = TableKeyBounds::unbounded();
    assert!(rows.iter().all(|row| unbounded.contains_key(row.key())));

    let exact = TableKeyBounds::exact(bravo.clone());
    assert!(!exact.contains_key(&alpha));
    assert!(exact.contains_key(&bravo));
    assert!(!exact.contains_key(&charlie));

    let closed = TableKeyBounds::closed(alpha.clone(), bravo.clone()).expect("closed range");
    assert!(closed.contains_key(&alpha));
    assert!(closed.contains_key(&bravo));
    assert!(!closed.contains_key(&charlie));

    let open = TableKeyBounds::open(alpha.clone(), charlie.clone()).expect("open range");
    assert!(!open.contains_key(&alpha));
    assert!(open.contains_key(&bravo));
    assert!(!open.contains_key(&charlie));

    let empty = TableKeyBounds::open(bravo.clone(), bravo.clone()).expect("empty open range");
    assert!(!empty.contains_key(&bravo));

    let singleton = TableKeyBounds::closed(bravo.clone(), bravo.clone()).expect("singleton range");
    assert!(!singleton.contains_key(&alpha));
    assert!(singleton.contains_key(&bravo));
    assert!(!singleton.contains_key(&charlie));

    let lower_exclusive = TableKeyBounds::range(
        TableKeyBound::excluded(alpha.clone()),
        TableKeyBound::included(charlie.clone()),
    )
    .expect("lower exclusive");
    assert!(!lower_exclusive.contains_key(&alpha));
    assert!(lower_exclusive.contains_key(&bravo));
    assert!(lower_exclusive.contains_key(&charlie));

    let upper_exclusive = TableKeyBounds::range(
        TableKeyBound::included(alpha.clone()),
        TableKeyBound::excluded(charlie.clone()),
    )
    .expect("upper exclusive");
    assert!(upper_exclusive.contains_key(&alpha));
    assert!(upper_exclusive.contains_key(&bravo));
    assert!(!upper_exclusive.contains_key(&charlie));

    let prefix = TablePhysicalKeyBytes::from_row(rows[0].row());
    let prefix_bounds = TableKeyBounds::prefix(prefix.as_slice().to_vec());
    assert!(prefix_bounds.contains_key(&alpha));
    assert!(!prefix_bounds.contains_key(&bravo));

    let tombstone = TableRow::new(StorageRow::tombstone(
        rows[0].physical_key().clone(),
        CommitVersion::new(3),
        Timestamp::from_micros(1),
    ));
    let expired = TableRow::new(StorageRow::put(
        rows[0].physical_key().clone(),
        CommitVersion::new(1),
        Timestamp::from_micros(1),
        Timestamp::from_micros(2),
        Vec::new(),
    ));
    let same_physical_prefix = TableKeyBounds::prefix(prefix.as_slice().to_vec());
    assert!(same_physical_prefix.contains_key(tombstone.key()));
    assert!(same_physical_prefix.contains_key(expired.key()));

    assert_eq!(
        TableKeyBounds::range(
            TableKeyBound::included(charlie),
            TableKeyBound::excluded(alpha)
        ),
        Err(TableRuntimeError::InvalidRange {
            field: "key_bounds",
        })
    );
}

#[test]
fn approximate_row_size_is_deterministic_and_monotonic() {
    let key = physical_key(1, "default", 0x20, b"short".to_vec());
    let small = TableRow::new(put_row_for_key(key.clone(), 1, vec![1]));
    let larger_value = TableRow::new(put_row_for_key(key, 1, vec![1; 32]));
    assert!(larger_value.approximate_size_bytes() > small.approximate_size_bytes());

    let longer_key = TableRow::new(put_row_for_key(
        physical_key(1, "default", 0x20, b"short-with-more-key-bytes".to_vec()),
        1,
        vec![1],
    ));
    assert!(longer_key.approximate_size_bytes() > small.approximate_size_bytes());
    assert_eq!(
        small.approximate_size_bytes(),
        TableRow::new(small.row().clone()).approximate_size_bytes()
    );

    let different_timestamp = TableRow::new(StorageRow::put(
        small.physical_key().clone(),
        small.commit_version(),
        Timestamp::from_micros(999),
        Timestamp::EPOCH,
        small.value().to_vec(),
    ));
    let different_expiry = TableRow::new(StorageRow::put(
        small.physical_key().clone(),
        small.commit_version(),
        small.commit_timestamp(),
        Timestamp::from_micros(999),
        small.value().to_vec(),
    ));
    assert_eq!(
        small.approximate_size_bytes(),
        different_timestamp.approximate_size_bytes()
    );
    assert_eq!(
        small.approximate_size_bytes(),
        different_expiry.approximate_size_bytes()
    );

    let tombstone = TableRow::new(StorageRow::tombstone(
        small.physical_key().clone(),
        small.commit_version(),
        small.commit_timestamp(),
    ));
    assert!(tombstone.approximate_size_bytes() > tombstone.encoded_key().len());
}
