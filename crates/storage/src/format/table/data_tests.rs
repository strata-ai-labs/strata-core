use super::super::{key::encode_internal_key, storage_row::encode_storage_row, FormatError};
use super::data::{decode_table_data_block, encode_table_data_block, TableDataBlock};
use super::test_support::{
    alternate_key, data_payload, encoded_internal_key_for_row, encoded_row, internal_key_for_row,
    put, put_for_key, tombstone,
};
use super::{MAX_TABLE_BLOCK_ENTRIES, MAX_TABLE_KEY_BYTES, MAX_TABLE_ROW_BYTES};
use crate::row::InternalKey;
use strata_core::CommitVersion;

#[test]
fn table_data_block_round_trips_one_put_row() {
    let block = TableDataBlock::from_rows(&[put(b"alpha".to_vec(), 7)]).expect("data block");
    let bytes = encode_table_data_block(&block).expect("encode data block");
    assert_eq!(
        block.entries()[0].internal_key(),
        &internal_key_for_row(block.entries()[0].row())
    );

    assert_eq!(decode_table_data_block(&bytes), Ok(block));
}

#[test]
fn table_data_block_preserves_put_and_tombstone_order() {
    let rows = vec![put(b"alpha".to_vec(), 8), tombstone(b"alpha".to_vec(), 7)];
    let block = TableDataBlock::from_rows(&rows).expect("data block");
    let decoded =
        decode_table_data_block(&encode_table_data_block(&block).expect("encode data block"))
            .expect("decode data block");

    assert_eq!(decoded.rows().cloned().collect::<Vec<_>>(), rows);
}

#[test]
fn table_data_block_allows_same_physical_key_at_different_versions() {
    let block = TableDataBlock::from_rows(&[
        put(b"alpha".to_vec(), 9),
        put(b"alpha".to_vec(), 8),
        tombstone(b"alpha".to_vec(), 7),
    ]);

    assert!(block.is_ok());
}

#[test]
fn table_data_block_rejects_empty_or_oversized_entry_count() {
    assert_eq!(
        TableDataBlock::from_rows(&[]),
        Err(FormatError::InvalidLength {
            field: "entry_count"
        })
    );

    assert_eq!(
        decode_table_data_block(&0u32.to_le_bytes()),
        Err(FormatError::InvalidLength {
            field: "entry_count"
        })
    );

    let mut bytes = (MAX_TABLE_BLOCK_ENTRIES + 1).to_le_bytes().to_vec();
    bytes.extend_from_slice(&[0; 8]);
    assert_eq!(
        decode_table_data_block(&bytes),
        Err(FormatError::InvalidLength {
            field: "entry_count"
        })
    );

    let truncated_at_limit = MAX_TABLE_BLOCK_ENTRIES.to_le_bytes();
    assert_eq!(
        decode_table_data_block(&truncated_at_limit),
        Err(FormatError::InsufficientBytes {
            format: "table_data_block",
            needed: 8,
            actual: 4
        })
    );
}

#[test]
fn table_data_block_rejects_bad_key_lengths_before_allocation() {
    let row = put(b"alpha".to_vec(), 7);
    let row_bytes = encoded_row(&row);

    assert_eq!(
        decode_table_data_block(&data_payload(&[(Vec::new(), row_bytes.clone())])),
        Err(FormatError::InvalidLength {
            field: "internal_key_len"
        })
    );

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&(u32_len(MAX_TABLE_KEY_BYTES) + 1).to_le_bytes());
    assert_eq!(
        decode_table_data_block(&bytes),
        Err(FormatError::InvalidLength {
            field: "internal_key_len"
        })
    );
}

#[test]
fn table_data_block_rejects_bad_row_lengths_before_allocation() {
    let row = put(b"alpha".to_vec(), 7);
    let key_bytes = encoded_internal_key_for_row(&row);

    assert_eq!(
        decode_table_data_block(&data_payload(&[(key_bytes.clone(), Vec::new())])),
        Err(FormatError::InvalidLength { field: "row_len" })
    );

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&u32_len(key_bytes.len()).to_le_bytes());
    bytes.extend_from_slice(&key_bytes);
    bytes.extend_from_slice(&(u32_len(MAX_TABLE_ROW_BYTES) + 1).to_le_bytes());
    assert_eq!(
        decode_table_data_block(&bytes),
        Err(FormatError::InvalidLength { field: "row_len" })
    );
}

fn u32_len(len: usize) -> u32 {
    u32::try_from(len).expect("test length fits u32")
}

#[test]
fn table_data_block_rejects_invalid_nested_key_or_row_bytes() {
    let row = put(b"alpha".to_vec(), 7);

    assert!(matches!(
        decode_table_data_block(&data_payload(&[(vec![0xff], encoded_row(&row))])),
        Err(FormatError::InsufficientBytes {
            format: "internal_key",
            ..
        })
    ));

    assert_eq!(
        decode_table_data_block(&data_payload(&[(
            encoded_internal_key_for_row(&row),
            vec![2]
        )])),
        Err(FormatError::InvalidVersion {
            format: "storage_row",
            version: 2
        })
    );
}

#[test]
fn table_data_block_rejects_key_row_fact_mismatches() {
    let alpha = put(b"alpha".to_vec(), 7);
    let beta = put_for_key(alternate_key(b"beta".to_vec()), 7);
    assert_eq!(
        decode_table_data_block(&data_payload(&[(
            encoded_internal_key_for_row(&alpha),
            encoded_row(&beta)
        )])),
        Err(FormatError::InvalidValue {
            field: "physical_key"
        })
    );

    let wrong_version_key = encode_internal_key(&InternalKey::new(
        alpha.physical_key().clone(),
        CommitVersion::new(8),
    ));
    assert_eq!(
        decode_table_data_block(&data_payload(&[(wrong_version_key, encoded_row(&alpha))])),
        Err(FormatError::InvalidValue {
            field: "commit_version"
        })
    );
}

#[test]
fn table_data_block_rejects_unsorted_or_duplicate_internal_keys() {
    assert_eq!(
        TableDataBlock::from_rows(&[put(b"beta".to_vec(), 7), put(b"alpha".to_vec(), 7)]),
        Err(FormatError::InvalidValue {
            field: "internal_key_order"
        })
    );

    let row = put(b"alpha".to_vec(), 7);
    assert_eq!(
        TableDataBlock::from_rows(&[row.clone(), row]),
        Err(FormatError::InvalidValue {
            field: "internal_key"
        })
    );

    let alpha = put(b"alpha".to_vec(), 7);
    let beta = put(b"beta".to_vec(), 7);
    let duplicate_bytes = data_payload(&[
        (encoded_internal_key_for_row(&alpha), encoded_row(&alpha)),
        (encoded_internal_key_for_row(&alpha), encoded_row(&alpha)),
    ]);
    assert_eq!(
        decode_table_data_block(&duplicate_bytes),
        Err(FormatError::InvalidValue {
            field: "internal_key"
        })
    );

    let bytes = data_payload(&[
        (encoded_internal_key_for_row(&beta), encoded_row(&beta)),
        (encoded_internal_key_for_row(&alpha), encoded_row(&alpha)),
    ]);
    assert_eq!(
        decode_table_data_block(&bytes),
        Err(FormatError::InvalidValue {
            field: "internal_key_order"
        })
    );
}

#[test]
fn table_data_block_rejects_trailing_bytes() {
    let block = TableDataBlock::from_rows(&[put(b"alpha".to_vec(), 7)]).expect("data block");
    let mut bytes = encode_table_data_block(&block).expect("encode data block");
    bytes.push(0);

    assert_eq!(
        decode_table_data_block(&bytes),
        Err(FormatError::TrailingData {
            format: "table_data_block",
            remaining: 1
        })
    );
}

#[test]
fn table_data_block_surfaces_nested_tombstone_payload_errors() {
    let row = tombstone(b"alpha".to_vec(), 7);
    let mut row_bytes = encode_storage_row(&row).expect("encode row");
    let key_len = u32::from_le_bytes(row_bytes[1..5].try_into().expect("key len")) as usize;
    let value_len_offset = 1 + 4 + key_len + 8 + 8 + 8 + 4 + 1;
    row_bytes[value_len_offset..value_len_offset + 4].copy_from_slice(&1u32.to_le_bytes());

    assert_eq!(
        decode_table_data_block(&data_payload(&[(
            encoded_internal_key_for_row(&row),
            row_bytes
        )])),
        Err(FormatError::InvalidTombstonePayload { field: "value" })
    );
}

#[test]
fn table_data_block_surfaces_nested_storage_row_flag_errors() {
    let row = put(b"alpha".to_vec(), 7);
    let mut row_bytes = encode_storage_row(&row).expect("encode row");
    let key_len = u32::from_le_bytes(row_bytes[1..5].try_into().expect("key len")) as usize;
    let flags_offset = 1 + 4 + key_len + 8 + 8 + 8;
    row_bytes[flags_offset..flags_offset + 4].copy_from_slice(&1u32.to_le_bytes());

    assert_eq!(
        decode_table_data_block(&data_payload(&[(
            encoded_internal_key_for_row(&row),
            row_bytes
        )])),
        Err(FormatError::UnsupportedFlags {
            format: "storage_row",
            flags: 1
        })
    );
}

#[test]
fn table_data_block_encoding_is_deterministic() {
    let rows = vec![put(b"alpha".to_vec(), 8), tombstone(b"alpha".to_vec(), 7)];
    let block = TableDataBlock::from_rows(&rows).expect("data block");

    assert_eq!(
        encode_table_data_block(&block).expect("first encode"),
        encode_table_data_block(&block).expect("second encode")
    );
}
