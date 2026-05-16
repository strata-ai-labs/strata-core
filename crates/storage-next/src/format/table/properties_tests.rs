use super::super::{key::encode_internal_key, FormatError};
use super::data::TableDataBlock;
use super::properties::{
    decode_table_properties, encode_table_properties, validate_table_properties_against_header,
    TableProperties,
};
use super::test_support::{internal_key_for_row, put, tombstone};
use super::{MAX_TABLE_DATA_BLOCKS, MAX_TABLE_KEY_BYTES, MAX_TABLE_ROWS};
use strata_core_next::CommitVersion;

fn key_for(user_key: &[u8], version: u64) -> crate::row::InternalKey {
    internal_key_for_row(&put(user_key.to_vec(), version))
}

fn properties_payload(
    row_count: u64,
    data_block_count: u32,
    commit_min: u64,
    commit_max: u64,
    min_key: &[u8],
    max_key: &[u8],
) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&row_count.to_le_bytes());
    bytes.extend_from_slice(&data_block_count.to_le_bytes());
    bytes.extend_from_slice(&commit_min.to_le_bytes());
    bytes.extend_from_slice(&commit_max.to_le_bytes());
    bytes.extend_from_slice(&u32_len(min_key.len()).to_le_bytes());
    bytes.extend_from_slice(min_key);
    bytes.extend_from_slice(&u32_len(max_key.len()).to_le_bytes());
    bytes.extend_from_slice(max_key);
    bytes
}

fn u32_len(len: usize) -> u32 {
    u32::try_from(len).expect("test length fits u32")
}

#[test]
fn table_properties_round_trip_and_derive_from_data_blocks() {
    let first = TableDataBlock::from_rows(&[put(b"alpha".to_vec(), 9)]).expect("first block");
    let second =
        TableDataBlock::from_rows(&[put(b"beta".to_vec(), 7), tombstone(b"gamma".to_vec(), 5)])
            .expect("second block");
    let properties =
        TableProperties::from_data_blocks(&[first, second]).expect("derive properties");

    assert_eq!(properties.row_count(), 3);
    assert_eq!(properties.data_block_count(), 2);
    assert_eq!(properties.commit_min(), CommitVersion::new(5));
    assert_eq!(properties.commit_max(), CommitVersion::new(9));
    assert_eq!(properties.min_key(), &key_for(b"alpha", 9));
    assert_eq!(properties.max_key(), &key_for(b"gamma", 5));
    assert_eq!(
        decode_table_properties(&encode_table_properties(&properties).expect("encode props")),
        Ok(properties)
    );
}

#[test]
fn table_properties_from_data_blocks_rejects_overlapping_block_ranges() {
    let first = TableDataBlock::from_rows(&[put(b"alpha".to_vec(), 9), put(b"delta".to_vec(), 8)])
        .expect("first block");
    let overlapping =
        TableDataBlock::from_rows(&[put(b"beta".to_vec(), 7)]).expect("overlapping block");

    assert_eq!(
        TableProperties::from_data_blocks(&[first, overlapping]),
        Err(FormatError::InvalidValue {
            field: "table_block_key_overlap"
        })
    );
}

#[test]
fn table_properties_rejects_pre_v1_and_future_versions() {
    let properties = TableProperties::new(
        1,
        1,
        CommitVersion::new(7),
        CommitVersion::new(7),
        &key_for(b"alpha", 7),
        &key_for(b"alpha", 7),
    )
    .expect("properties");
    let valid = encode_table_properties(&properties).expect("encode props");

    let mut pre_v1 = valid.clone();
    pre_v1[..4].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        decode_table_properties(&pre_v1),
        Err(FormatError::PreV1Format {
            format: "table_properties_block",
            version: 0
        })
    );

    let mut future = valid;
    future[..4].copy_from_slice(&2u32.to_le_bytes());
    assert_eq!(
        decode_table_properties(&future),
        Err(FormatError::FutureFormat {
            format: "table_properties_block",
            version: 2,
            max_supported: 1
        })
    );
}

#[test]
fn table_properties_rejects_invalid_counts_and_commit_range() {
    let min_key = encode_internal_key(&key_for(b"alpha", 7));
    let max_key = min_key.clone();

    assert_eq!(
        decode_table_properties(&properties_payload(0, 1, 7, 7, &min_key, &max_key)),
        Err(FormatError::InvalidLength { field: "row_count" })
    );
    assert_eq!(
        decode_table_properties(&properties_payload(
            MAX_TABLE_ROWS + 1,
            1,
            7,
            7,
            &min_key,
            &max_key
        )),
        Err(FormatError::InvalidLength { field: "row_count" })
    );
    assert_eq!(
        decode_table_properties(&properties_payload(1, 0, 7, 7, &min_key, &max_key)),
        Err(FormatError::InvalidLength {
            field: "data_block_count"
        })
    );
    assert_eq!(
        decode_table_properties(&properties_payload(
            1,
            MAX_TABLE_DATA_BLOCKS + 1,
            7,
            7,
            &min_key,
            &max_key
        )),
        Err(FormatError::InvalidLength {
            field: "data_block_count"
        })
    );
    assert_eq!(
        decode_table_properties(&properties_payload(1, 1, 8, 7, &min_key, &max_key)),
        Err(FormatError::InvalidValue {
            field: "commit_range"
        })
    );
}

#[test]
fn table_properties_rejects_bad_key_lengths_and_nested_key_bytes() {
    let min_key = encode_internal_key(&key_for(b"alpha", 7));
    let max_key = encode_internal_key(&key_for(b"beta", 7));

    assert_eq!(
        decode_table_properties(&properties_payload(1, 1, 7, 7, &[], &max_key)),
        Err(FormatError::InvalidLength {
            field: "min_key_len"
        })
    );
    assert_eq!(
        decode_table_properties(&properties_payload(1, 1, 7, 7, &min_key, &[])),
        Err(FormatError::InvalidLength {
            field: "max_key_len"
        })
    );

    let mut oversized = Vec::new();
    oversized.extend_from_slice(&1u32.to_le_bytes());
    oversized.extend_from_slice(&1u64.to_le_bytes());
    oversized.extend_from_slice(&1u32.to_le_bytes());
    oversized.extend_from_slice(&7u64.to_le_bytes());
    oversized.extend_from_slice(&7u64.to_le_bytes());
    oversized.extend_from_slice(&(u32_len(MAX_TABLE_KEY_BYTES) + 1).to_le_bytes());
    assert_eq!(
        decode_table_properties(&oversized),
        Err(FormatError::InvalidLength {
            field: "min_key_len"
        })
    );

    assert!(matches!(
        decode_table_properties(&properties_payload(1, 1, 7, 7, &[0xff], &max_key)),
        Err(FormatError::InsufficientBytes {
            format: "internal_key",
            ..
        })
    ));
    assert!(matches!(
        decode_table_properties(&properties_payload(1, 1, 7, 7, &min_key, &[0xff])),
        Err(FormatError::InsufficientBytes {
            format: "internal_key",
            ..
        })
    ));
}

#[test]
fn table_properties_rejects_min_key_after_max_key() {
    assert_eq!(
        TableProperties::new(
            1,
            1,
            CommitVersion::new(7),
            CommitVersion::new(7),
            &key_for(b"beta", 7),
            &key_for(b"alpha", 7),
        ),
        Err(FormatError::InvalidValue {
            field: "table_key_range"
        })
    );
}

#[test]
fn table_properties_validate_header_facts_when_supplied() {
    let properties = TableProperties::new(
        1,
        1,
        CommitVersion::new(7),
        CommitVersion::new(7),
        &key_for(b"alpha", 7),
        &key_for(b"alpha", 7),
    )
    .expect("properties");

    assert_eq!(
        validate_table_properties_against_header(
            &properties,
            2,
            1,
            CommitVersion::new(7),
            CommitVersion::new(7)
        ),
        Err(FormatError::InvalidValue {
            field: "table_properties_facts"
        })
    );
    assert_eq!(
        validate_table_properties_against_header(
            &properties,
            1,
            1,
            CommitVersion::new(7),
            CommitVersion::new(7)
        ),
        Ok(())
    );
}

#[test]
fn table_properties_rejects_trailing_bytes() {
    let properties = TableProperties::new(
        1,
        1,
        CommitVersion::new(7),
        CommitVersion::new(7),
        &key_for(b"alpha", 7),
        &key_for(b"alpha", 7),
    )
    .expect("properties");
    let mut bytes = encode_table_properties(&properties).expect("encode props");
    bytes.push(0);

    assert_eq!(
        decode_table_properties(&bytes),
        Err(FormatError::TrailingData {
            format: "table_properties_block",
            remaining: 1
        })
    );
}
