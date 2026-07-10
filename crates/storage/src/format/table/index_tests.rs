use super::super::{key::encode_internal_key, FormatError};
use super::index::{
    decode_table_index_block, decode_table_index_block_for_data_blocks, encode_table_index_block,
    TableIndexBlock, TableIndexEntry,
};
use super::test_support::{internal_key_for_row, put};
use super::{MAX_TABLE_DATA_BLOCKS, MAX_TABLE_KEY_BYTES};

type RawIndexEntry = (Vec<u8>, Vec<u8>, u64, u32, u32);

fn key_for(user_key: &[u8], version: u64) -> crate::row::InternalKey {
    internal_key_for_row(&put(user_key.to_vec(), version))
}

fn entry(first: &[u8], last: &[u8], offset: u64, row_count: u32) -> TableIndexEntry {
    TableIndexEntry::new(&key_for(first, 7), &key_for(last, 7), offset, 48, row_count)
        .expect("index entry")
}

fn index_payload(entries: &[RawIndexEntry]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&u32_len(entries.len()).to_le_bytes());
    for (first_key, last_key, offset, frame_len, row_count) in entries {
        bytes.extend_from_slice(&u32_len(first_key.len()).to_le_bytes());
        bytes.extend_from_slice(first_key);
        bytes.extend_from_slice(&u32_len(last_key.len()).to_le_bytes());
        bytes.extend_from_slice(last_key);
        bytes.extend_from_slice(&offset.to_le_bytes());
        bytes.extend_from_slice(&frame_len.to_le_bytes());
        bytes.extend_from_slice(&row_count.to_le_bytes());
    }
    bytes
}

fn u32_len(len: usize) -> u32 {
    u32::try_from(len).expect("test length fits u32")
}

#[test]
fn table_index_block_round_trips_single_and_multi_entry_indexes() {
    let single =
        TableIndexBlock::new(vec![entry(b"alpha", b"beta", 64, 2)]).expect("single-entry index");
    assert_eq!(single.entries()[0].first_key(), &key_for(b"alpha", 7));
    assert_eq!(single.entries()[0].last_key(), &key_for(b"beta", 7));
    assert_eq!(
        decode_table_index_block(&encode_table_index_block(&single).expect("encode index")),
        Ok(single)
    );

    let multi = TableIndexBlock::new(vec![
        entry(b"alpha", b"beta", 64, 2),
        entry(b"delta", b"epsilon", 128, 3),
    ])
    .expect("multi-entry index");
    assert_eq!(
        decode_table_index_block(&encode_table_index_block(&multi).expect("encode index")),
        Ok(multi)
    );
}

#[test]
fn table_index_block_rejects_pre_v1_and_future_versions() {
    let block = TableIndexBlock::new(vec![entry(b"alpha", b"beta", 64, 2)]).expect("index");
    let valid = encode_table_index_block(&block).expect("encode index");

    let mut pre_v1 = valid.clone();
    pre_v1[..4].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        decode_table_index_block(&pre_v1),
        Err(FormatError::PreV1Format {
            format: "table_index_block",
            version: 0
        })
    );

    let mut future = valid;
    future[..4].copy_from_slice(&2u32.to_le_bytes());
    assert_eq!(
        decode_table_index_block(&future),
        Err(FormatError::FutureFormat {
            format: "table_index_block",
            version: 2,
            max_supported: 1
        })
    );
}

#[test]
fn table_index_block_rejects_zero_or_oversized_entry_count() {
    let mut zero = Vec::new();
    zero.extend_from_slice(&1u32.to_le_bytes());
    zero.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        decode_table_index_block(&zero),
        Err(FormatError::InvalidLength {
            field: "entry_count"
        })
    );

    let mut oversized = Vec::new();
    oversized.extend_from_slice(&1u32.to_le_bytes());
    oversized.extend_from_slice(&(MAX_TABLE_DATA_BLOCKS + 1).to_le_bytes());
    assert_eq!(
        decode_table_index_block(&oversized),
        Err(FormatError::InvalidLength {
            field: "entry_count"
        })
    );

    let mut truncated_at_limit = Vec::new();
    truncated_at_limit.extend_from_slice(&1u32.to_le_bytes());
    truncated_at_limit.extend_from_slice(&MAX_TABLE_DATA_BLOCKS.to_le_bytes());
    assert_eq!(
        decode_table_index_block(&truncated_at_limit),
        Err(FormatError::InsufficientBytes {
            format: "table_index_block",
            needed: 12,
            actual: 8
        })
    );
}

#[test]
fn table_index_block_rejects_bad_key_lengths_and_nested_key_bytes() {
    let first = encode_internal_key(&key_for(b"alpha", 7));
    let last = encode_internal_key(&key_for(b"beta", 7));

    assert_eq!(
        decode_table_index_block(&index_payload(&[(Vec::new(), last.clone(), 64, 48, 1)])),
        Err(FormatError::InvalidLength {
            field: "first_key_len"
        })
    );

    let mut oversized = Vec::new();
    oversized.extend_from_slice(&1u32.to_le_bytes());
    oversized.extend_from_slice(&1u32.to_le_bytes());
    oversized.extend_from_slice(&(u32_len(MAX_TABLE_KEY_BYTES) + 1).to_le_bytes());
    assert_eq!(
        decode_table_index_block(&oversized),
        Err(FormatError::InvalidLength {
            field: "first_key_len"
        })
    );

    assert!(matches!(
        decode_table_index_block(&index_payload(&[(vec![0xff], last, 64, 48, 1)])),
        Err(FormatError::InsufficientBytes {
            format: "internal_key",
            ..
        })
    ));

    assert!(matches!(
        decode_table_index_block(&index_payload(&[(first.clone(), vec![0xff], 64, 48, 1)])),
        Err(FormatError::InsufficientBytes {
            format: "internal_key",
            ..
        })
    ));

    assert_eq!(
        decode_table_index_block(&index_payload(&[(first, Vec::new(), 64, 48, 1)])),
        Err(FormatError::InvalidLength {
            field: "last_key_len"
        })
    );
}

#[test]
fn table_index_block_rejects_invalid_ranges_ordering_and_overlap() {
    assert_eq!(
        TableIndexEntry::new(&key_for(b"beta", 7), &key_for(b"alpha", 7), 64, 48, 1),
        Err(FormatError::InvalidValue {
            field: "index_key_range"
        })
    );

    assert_eq!(
        TableIndexBlock::new(vec![
            entry(b"delta", b"epsilon", 128, 1),
            entry(b"alpha", b"beta", 64, 1),
        ]),
        Err(FormatError::InvalidValue {
            field: "index_key_order"
        })
    );

    assert_eq!(
        TableIndexBlock::new(vec![
            entry(b"alpha", b"delta", 64, 1),
            entry(b"beta", b"epsilon", 128, 1),
        ]),
        Err(FormatError::InvalidValue {
            field: "index_key_overlap"
        })
    );
}

#[test]
fn table_index_block_rejects_zero_frame_len_or_row_count() {
    assert_eq!(
        TableIndexEntry::new(&key_for(b"alpha", 7), &key_for(b"beta", 7), 64, 0, 1),
        Err(FormatError::InvalidLength {
            field: "block_frame_len"
        })
    );
    assert_eq!(
        TableIndexEntry::new(&key_for(b"alpha", 7), &key_for(b"beta", 7), 64, 48, 0),
        Err(FormatError::InvalidLength { field: "row_count" })
    );
}

#[test]
fn table_index_block_checks_expected_data_block_count_when_supplied() {
    let block = TableIndexBlock::new(vec![entry(b"alpha", b"beta", 64, 1)]).expect("index");
    let bytes = encode_table_index_block(&block).expect("encode index");

    assert_eq!(
        decode_table_index_block_for_data_blocks(&bytes, 2),
        Err(FormatError::InvalidValue {
            field: "index_entry_count"
        })
    );
    assert_eq!(
        decode_table_index_block_for_data_blocks(&bytes, 1),
        Ok(block)
    );
}

#[test]
fn table_index_block_rejects_trailing_bytes() {
    let block = TableIndexBlock::new(vec![entry(b"alpha", b"beta", 64, 2)]).expect("index");
    let mut bytes = encode_table_index_block(&block).expect("encode index");
    bytes.push(0);

    assert_eq!(
        decode_table_index_block(&bytes),
        Err(FormatError::TrailingData {
            format: "table_index_block",
            remaining: 1
        })
    );
}
