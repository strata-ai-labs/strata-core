use super::artifact::{
    decode_immutable_table, encode_immutable_table, encode_immutable_table_with_block_compressions,
};
use super::data::TableDataBlock;
use super::index::{TableIndexBlock, TableIndexEntry};
use super::properties::TableProperties;
use super::test_support::{branch, encoded_internal_key_for_row, put, timestamp, tombstone};
use super::{
    decode_table_footer, encode_table_block_frame, encode_table_footer, encode_table_header,
    FormatError, TableBlockFrame, TableBlockKind, TableCompression, TableFilterFrame, TableFooter,
    TableHeader, TABLE_BLOCK_FRAME_HEADER_SIZE, TABLE_FOOTER_FORMAT, TABLE_FOOTER_SIZE,
    TABLE_HEADER_SIZE,
};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use std::ops::Range;
use strata_core_next::{CommitVersion, Timestamp};

fn sample_rows() -> Vec<StorageRow> {
    vec![
        put(b"alpha".to_vec(), 9),
        tombstone(b"alpha".to_vec(), 7),
        put(b"beta".to_vec(), 6),
        put(b"gamma".to_vec(), 5),
    ]
}

fn generated_rows(row_count: usize) -> Vec<StorageRow> {
    (0..row_count)
        .map(|index| {
            put(
                format!("row-{index:03}").into_bytes(),
                row_count as u64 - index as u64,
            )
        })
        .collect()
}

fn refresh_table_crc(table: &mut [u8]) {
    let crc = crc32fast::hash(&table[..table.len() - 4]);
    let crc_offset = table.len() - 4;
    table[crc_offset..].copy_from_slice(&crc.to_le_bytes());
}

fn refresh_frame_crc(frame: &mut [u8]) {
    let encoded_len = u32::from_le_bytes(frame[4..8].try_into().expect("encoded len")) as usize;
    let crc_offset = TABLE_BLOCK_FRAME_HEADER_SIZE + encoded_len;
    let crc = crc32fast::hash(&frame[..crc_offset]);
    frame[crc_offset..crc_offset + 4].copy_from_slice(&crc.to_le_bytes());
}

fn footer_start(table: &[u8]) -> usize {
    table.len() - TABLE_FOOTER_SIZE
}

fn footer_ranges(table: &[u8]) -> (Range<usize>, Range<usize>) {
    let footer = decode_table_footer(table).expect("footer");
    let index_start = usize::try_from(footer.index_block_offset()).expect("index offset");
    let index_len = usize::try_from(footer.index_block_frame_len()).expect("index len");
    let properties_start =
        usize::try_from(footer.properties_block_offset()).expect("properties offset");
    let properties_len =
        usize::try_from(footer.properties_block_frame_len()).expect("properties len");

    (
        index_start..index_start + index_len,
        properties_start..properties_start + properties_len,
    )
}

fn assemble_table(
    header: TableHeader,
    data_blocks: &[TableDataBlock],
    data_compressions: &[TableCompression],
    index: &TableIndexBlock,
    properties: &TableProperties,
) -> Vec<u8> {
    let mut table = encode_table_header(&header);
    for (block, compression) in data_blocks.iter().zip(data_compressions.iter().copied()) {
        let payload = super::data::encode_table_data_block(block).expect("data payload");
        let frame =
            TableBlockFrame::new(TableBlockKind::Data, compression, payload).expect("data frame");
        table.extend_from_slice(&encode_table_block_frame(&frame).expect("encode data frame"));
    }

    let index_offset = u64::try_from(table.len()).expect("index offset");
    let index_payload = super::index::encode_table_index_block(index).expect("index payload");
    let index_frame = TableBlockFrame::new(
        TableBlockKind::Index,
        TableCompression::Uncompressed,
        index_payload,
    )
    .expect("index frame");
    let index_frame = encode_table_block_frame(&index_frame).expect("encode index frame");
    let index_len = u32::try_from(index_frame.len()).expect("index len");
    table.extend_from_slice(&index_frame);

    let properties_offset = u64::try_from(table.len()).expect("properties offset");
    let properties_payload =
        super::properties::encode_table_properties(properties).expect("properties payload");
    let properties_frame = TableBlockFrame::new(
        TableBlockKind::Properties,
        TableCompression::Uncompressed,
        properties_payload,
    )
    .expect("properties frame");
    let properties_frame =
        encode_table_block_frame(&properties_frame).expect("encode properties frame");
    let properties_len = u32::try_from(properties_frame.len()).expect("properties len");
    table.extend_from_slice(&properties_frame);

    let footer = TableFooter::new(index_offset, index_len, properties_offset, properties_len)
        .expect("footer");
    table.extend_from_slice(&encode_table_footer(&footer, &table).expect("encode footer"));
    table
}

fn index_entry_for_block(
    block: &TableDataBlock,
    offset: usize,
    frame_len: usize,
) -> TableIndexEntry {
    TableIndexEntry::new(
        block.entries()[0].internal_key(),
        block.entries()[block.entries().len() - 1].internal_key(),
        u64::try_from(offset).expect("offset"),
        u32::try_from(frame_len).expect("frame len"),
        u32::try_from(block.row_count()).expect("row count"),
    )
    .expect("index entry")
}

struct IndexEntryFields {
    first_key: Range<usize>,
    last_key: Range<usize>,
    block_offset: usize,
    block_frame_len: usize,
    row_count: usize,
}

fn first_index_entry_fields(table: &[u8]) -> IndexEntryFields {
    let (index_range, _) = footer_ranges(table);
    let payload_start = index_range.start + TABLE_BLOCK_FRAME_HEADER_SIZE;
    let mut cursor = payload_start + 8;
    let first_len =
        u32::from_le_bytes(table[cursor..cursor + 4].try_into().expect("first len")) as usize;
    let first_key = cursor + 4..cursor + 4 + first_len;
    cursor += 4 + first_len;
    let last_len =
        u32::from_le_bytes(table[cursor..cursor + 4].try_into().expect("last len")) as usize;
    let last_key = cursor + 4..cursor + 4 + last_len;
    cursor += 4 + last_len;
    IndexEntryFields {
        first_key,
        last_key,
        block_offset: cursor,
        block_frame_len: cursor + 8,
        row_count: cursor + 12,
    }
}

fn refresh_index_frame_and_table_crc(table: &mut [u8]) {
    let index_range = raw_index_range(table);
    refresh_frame_crc(&mut table[index_range]);
    refresh_table_crc(table);
}

fn refresh_properties_frame_and_table_crc(table: &mut [u8]) {
    let properties_range = raw_properties_range(table);
    refresh_frame_crc(&mut table[properties_range]);
    refresh_table_crc(table);
}

fn raw_index_range(table: &[u8]) -> Range<usize> {
    let footer_start = footer_start(table);
    let index_start = usize::try_from(u64::from_le_bytes(
        table[footer_start..footer_start + 8]
            .try_into()
            .expect("index"),
    ))
    .expect("index offset fits usize");
    let index_len = u32::from_le_bytes(
        table[footer_start + 8..footer_start + 12]
            .try_into()
            .expect("index len"),
    ) as usize;
    index_start..index_start + index_len
}

fn raw_properties_range(table: &[u8]) -> Range<usize> {
    let footer_start = footer_start(table);
    let properties_start = usize::try_from(u64::from_le_bytes(
        table[footer_start + 24..footer_start + 32]
            .try_into()
            .expect("properties"),
    ))
    .expect("properties offset fits usize");
    let properties_len = u32::from_le_bytes(
        table[footer_start + 32..footer_start + 36]
            .try_into()
            .expect("properties len"),
    ) as usize;
    properties_start..properties_start + properties_len
}

struct PropertiesFields {
    row_count: usize,
    data_block_count: usize,
    commit_min: usize,
    min_key: Range<usize>,
    max_key: Range<usize>,
}

fn properties_fields(table: &[u8]) -> PropertiesFields {
    let (_, properties_range) = footer_ranges(table);
    let payload_start = properties_range.start + TABLE_BLOCK_FRAME_HEADER_SIZE;
    let row_count = payload_start + 4;
    let data_block_count = payload_start + 12;
    let commit_min = payload_start + 16;
    let mut cursor = payload_start + 32;
    let min_len =
        u32::from_le_bytes(table[cursor..cursor + 4].try_into().expect("min len")) as usize;
    let min_key = cursor + 4..cursor + 4 + min_len;
    cursor += 4 + min_len;
    let max_len =
        u32::from_le_bytes(table[cursor..cursor + 4].try_into().expect("max len")) as usize;
    let max_key = cursor + 4..cursor + 4 + max_len;
    PropertiesFields {
        row_count,
        data_block_count,
        commit_min,
        min_key,
        max_key,
    }
}

#[test]
fn immutable_table_round_trips_one_block_and_table_facts() {
    let rows = vec![put(b"alpha".to_vec(), 9), tombstone(b"alpha".to_vec(), 7)];
    let bytes = encode_immutable_table(&rows, 4096, 16, TableCompression::Uncompressed)
        .expect("encode table");
    let decoded = decode_immutable_table(&bytes).expect("decode table");

    assert_eq!(decoded.rows(), rows.as_slice());
    assert_eq!(decoded.header().row_count(), 2);
    assert_eq!(decoded.header().data_block_count(), 1);
    assert_eq!(decoded.header().commit_min(), CommitVersion::new(7));
    assert_eq!(decoded.header().commit_max(), CommitVersion::new(9));
    assert_eq!(decoded.data_blocks().len(), 1);
    assert_eq!(decoded.index().entry_count(), 1);
    assert_eq!(decoded.properties().row_count(), 2);
}

#[test]
fn immutable_table_round_trips_two_blocks_in_table_order() {
    let rows = sample_rows();
    let bytes = encode_immutable_table(&rows, 1024, 2, TableCompression::Uncompressed)
        .expect("encode table");
    let decoded = decode_immutable_table(&bytes).expect("decode table");

    assert_eq!(decoded.rows(), rows.as_slice());
    assert_eq!(decoded.header().data_block_count(), 2);
    assert_eq!(decoded.data_blocks().len(), 2);
    assert_eq!(decoded.index().entry_count(), 2);
}

#[test]
fn immutable_table_round_trips_zstd_and_mixed_data_compression() {
    let rows = sample_rows();
    let zstd = encode_immutable_table(&rows[..2], 4096, 8, TableCompression::Zstd)
        .expect("encode zstd table");
    assert_eq!(
        decode_immutable_table(&zstd).expect("decode zstd").rows(),
        &rows[..2]
    );

    let mixed = encode_immutable_table_with_block_compressions(
        &rows,
        4096,
        2,
        &[TableCompression::Uncompressed, TableCompression::Zstd],
        None,
    )
    .expect("encode mixed table");
    assert_eq!(
        decode_immutable_table(&mixed).expect("decode mixed").rows(),
        rows.as_slice()
    );
}

#[test]
fn immutable_table_decodes_property_budget_row_counts() {
    let rows = generated_rows(128);
    let bytes = encode_immutable_table(&rows, 4096, 32, TableCompression::Uncompressed)
        .expect("encode budget-sized table");
    let decoded = decode_immutable_table(&bytes).expect("decode budget-sized table");

    assert_eq!(decoded.rows(), rows.as_slice());
    assert_eq!(decoded.header().row_count(), 128);
    assert_eq!(decoded.header().data_block_count(), 4);
    assert_eq!(decoded.data_blocks().len(), 4);
    assert_eq!(decoded.index().entry_count(), 4);

    let above_property_budget = generated_rows(129);
    let bytes = encode_immutable_table(
        &above_property_budget,
        4096,
        32,
        TableCompression::Uncompressed,
    )
    .expect("the property budget is not a durable format limit");
    assert_eq!(
        decode_immutable_table(&bytes)
            .expect("decode above-budget valid table")
            .rows(),
        above_property_budget.as_slice()
    );
}

#[test]
fn immutable_table_rejects_empty_unsorted_duplicate_or_bad_split_inputs() {
    assert_eq!(
        encode_immutable_table(&[], 4096, 1, TableCompression::Uncompressed),
        Err(FormatError::InvalidLength { field: "row_count" })
    );
    assert_eq!(
        encode_immutable_table(
            &[put(b"alpha".to_vec(), 1)],
            4096,
            0,
            TableCompression::Uncompressed
        ),
        Err(FormatError::InvalidLength {
            field: "rows_per_block"
        })
    );
    assert_eq!(
        encode_immutable_table(
            &[put(b"alpha".to_vec(), 1)],
            0,
            1,
            TableCompression::Uncompressed
        ),
        Err(FormatError::InvalidLength {
            field: "target_data_block_size"
        })
    );

    assert_eq!(
        encode_immutable_table(
            &[put(b"beta".to_vec(), 7), put(b"alpha".to_vec(), 7)],
            4096,
            8,
            TableCompression::Uncompressed
        ),
        Err(FormatError::InvalidValue {
            field: "internal_key_order"
        })
    );

    let duplicate = put(b"alpha".to_vec(), 7);
    assert_eq!(
        encode_immutable_table(
            &[duplicate.clone(), duplicate],
            4096,
            8,
            TableCompression::Uncompressed
        ),
        Err(FormatError::InvalidValue {
            field: "internal_key"
        })
    );

    assert_eq!(
        encode_immutable_table_with_block_compressions(
            &[put(b"alpha".to_vec(), 7), put(b"beta".to_vec(), 6)],
            4096,
            1,
            &[TableCompression::Uncompressed],
            None
        ),
        Err(FormatError::InvalidValue {
            field: "data_compressions"
        })
    );
}

#[test]
fn immutable_table_rejects_header_fact_drift_after_valid_footer_crc() {
    let rows = sample_rows();
    let valid = encode_immutable_table(&rows, 4096, 2, TableCompression::Uncompressed)
        .expect("encode table");

    let mut row_count = valid.clone();
    row_count[24..32].copy_from_slice(&3u64.to_le_bytes());
    refresh_table_crc(&mut row_count);
    assert_eq!(
        decode_immutable_table(&row_count),
        Err(FormatError::InvalidValue {
            field: "table_properties_facts"
        })
    );

    let mut data_block_count = valid.clone();
    data_block_count[20..24].copy_from_slice(&1u32.to_le_bytes());
    refresh_table_crc(&mut data_block_count);
    assert_eq!(
        decode_immutable_table(&data_block_count),
        Err(FormatError::InvalidValue {
            field: "index_entry_count"
        })
    );

    let mut commit_range = valid;
    commit_range[32..40].copy_from_slice(&6u64.to_le_bytes());
    refresh_table_crc(&mut commit_range);
    assert_eq!(
        decode_immutable_table(&commit_range),
        Err(FormatError::InvalidValue {
            field: "table_properties_facts"
        })
    );
}

#[test]
fn immutable_table_rejects_properties_block_fact_drift_after_valid_crcs() {
    let rows = sample_rows();
    let table = encode_immutable_table(&rows, 4096, 2, TableCompression::Uncompressed)
        .expect("encode table");
    let fields = properties_fields(&table);

    let mut row_count = table.clone();
    row_count[fields.row_count..fields.row_count + 8].copy_from_slice(&3u64.to_le_bytes());
    refresh_properties_frame_and_table_crc(&mut row_count);
    assert_eq!(
        decode_immutable_table(&row_count),
        Err(FormatError::InvalidValue {
            field: "table_properties_facts"
        })
    );

    let mut data_block_count = table.clone();
    data_block_count[fields.data_block_count..fields.data_block_count + 4]
        .copy_from_slice(&1u32.to_le_bytes());
    refresh_properties_frame_and_table_crc(&mut data_block_count);
    assert_eq!(
        decode_immutable_table(&data_block_count),
        Err(FormatError::InvalidValue {
            field: "table_properties_facts"
        })
    );

    let mut commit_min = table.clone();
    commit_min[fields.commit_min..fields.commit_min + 8].copy_from_slice(&6u64.to_le_bytes());
    refresh_properties_frame_and_table_crc(&mut commit_min);
    assert_eq!(
        decode_immutable_table(&commit_min),
        Err(FormatError::InvalidValue {
            field: "table_properties_facts"
        })
    );

    let mut min_key = table.clone();
    let max_key_bytes = min_key[fields.max_key.clone()].to_vec();
    min_key[fields.min_key.clone()].copy_from_slice(&max_key_bytes);
    refresh_properties_frame_and_table_crc(&mut min_key);
    assert_eq!(
        decode_immutable_table(&min_key),
        Err(FormatError::InvalidValue {
            field: "table_properties_facts"
        })
    );

    let mut max_key = table;
    let first_row_key = encoded_internal_key_for_row(&rows[0]);
    max_key[fields.max_key].copy_from_slice(&first_row_key);
    refresh_properties_frame_and_table_crc(&mut max_key);
    assert_eq!(
        decode_immutable_table(&max_key),
        Err(FormatError::InvalidValue {
            field: "table_properties_facts"
        })
    );
}

#[test]
fn immutable_table_rejects_footer_crc_before_trusting_block_facts() {
    let rows = sample_rows();
    let mut table = encode_immutable_table(&rows, 4096, 2, TableCompression::Uncompressed)
        .expect("encode table");
    let (index_range, _) = footer_ranges(&table);
    table[index_range.start] = TableBlockKind::Data as u8;

    assert!(matches!(
        decode_immutable_table(&table),
        Err(FormatError::ChecksumMismatch {
            format: TABLE_FOOTER_FORMAT,
            ..
        })
    ));
}

#[test]
fn immutable_table_rejects_wrong_index_or_properties_frame_type() {
    let rows = sample_rows();
    let table = encode_immutable_table(&rows, 4096, 2, TableCompression::Uncompressed)
        .expect("encode table");
    let (index_range, properties_range) = footer_ranges(&table);

    let mut wrong_index = table.clone();
    wrong_index[index_range.start] = 1;
    refresh_frame_crc(&mut wrong_index[index_range]);
    refresh_table_crc(&mut wrong_index);
    assert_eq!(
        decode_immutable_table(&wrong_index),
        Err(FormatError::InvalidValue {
            field: "block_type"
        })
    );

    let mut wrong_properties = table;
    wrong_properties[properties_range.start] = 2;
    refresh_frame_crc(&mut wrong_properties[properties_range]);
    refresh_table_crc(&mut wrong_properties);
    assert_eq!(
        decode_immutable_table(&wrong_properties),
        Err(FormatError::InvalidValue {
            field: "block_type"
        })
    );
}

#[test]
fn immutable_table_rejects_index_entries_that_do_not_match_actual_data_frames() {
    let rows = sample_rows();
    let mut table = encode_immutable_table(&rows, 4096, 2, TableCompression::Uncompressed)
        .expect("encode table");
    let fields = first_index_entry_fields(&table);
    table[fields.block_offset..fields.block_offset + 8]
        .copy_from_slice(&(TABLE_HEADER_SIZE as u64 + 1).to_le_bytes());
    refresh_index_frame_and_table_crc(&mut table);

    assert_eq!(
        decode_immutable_table(&table),
        Err(FormatError::InvalidValue {
            field: "index_block_offset"
        })
    );
}

#[test]
fn immutable_table_rejects_index_ranges_that_cannot_point_to_data_frames() {
    let rows = sample_rows();
    let table = encode_immutable_table(&rows, 4096, 2, TableCompression::Uncompressed)
        .expect("encode table");
    let fields = first_index_entry_fields(&table);

    let mut before_header = table.clone();
    before_header[fields.block_offset..fields.block_offset + 8]
        .copy_from_slice(&0u64.to_le_bytes());
    refresh_index_frame_and_table_crc(&mut before_header);
    assert_eq!(
        decode_immutable_table(&before_header),
        Err(FormatError::InvalidLength {
            field: "index_block_range"
        })
    );

    let mut overflow = table;
    overflow[fields.block_offset..fields.block_offset + 8].copy_from_slice(&u64::MAX.to_le_bytes());
    refresh_index_frame_and_table_crc(&mut overflow);
    assert!(matches!(
        decode_immutable_table(&overflow),
        Err(FormatError::InvalidLength {
            field: "index_block_offset" | "index_block_range"
        })
    ));
}

#[test]
fn immutable_table_rejects_index_frame_length_row_count_and_key_drift() {
    let rows = sample_rows();
    let table = encode_immutable_table(&rows, 4096, 2, TableCompression::Uncompressed)
        .expect("encode table");
    let fields = first_index_entry_fields(&table);

    let mut frame_len = table.clone();
    let actual_len = u32::from_le_bytes(
        frame_len[fields.block_frame_len..fields.block_frame_len + 4]
            .try_into()
            .expect("frame len"),
    );
    frame_len[fields.block_frame_len..fields.block_frame_len + 4]
        .copy_from_slice(&(actual_len - 1).to_le_bytes());
    refresh_index_frame_and_table_crc(&mut frame_len);
    assert_eq!(
        decode_immutable_table(&frame_len),
        Err(FormatError::InvalidValue {
            field: "index_block_frame_len"
        })
    );

    let mut row_count = table.clone();
    row_count[fields.row_count..fields.row_count + 4].copy_from_slice(&1u32.to_le_bytes());
    refresh_index_frame_and_table_crc(&mut row_count);
    assert_eq!(
        decode_immutable_table(&row_count),
        Err(FormatError::InvalidValue {
            field: "index_row_count"
        })
    );

    let mut first_key = table.clone();
    let last_key = first_key[fields.last_key.clone()].to_vec();
    first_key[fields.first_key.clone()].copy_from_slice(&last_key);
    refresh_index_frame_and_table_crc(&mut first_key);
    assert_eq!(
        decode_immutable_table(&first_key),
        Err(FormatError::InvalidValue {
            field: "index_first_key"
        })
    );

    let mut last_key_table = table;
    let first_key_bytes = last_key_table[fields.first_key].to_vec();
    last_key_table[fields.last_key].copy_from_slice(&first_key_bytes);
    refresh_index_frame_and_table_crc(&mut last_key_table);
    assert_eq!(
        decode_immutable_table(&last_key_table),
        Err(FormatError::InvalidValue {
            field: "index_last_key"
        })
    );
}

#[test]
fn immutable_table_rejects_index_reference_to_non_data_frame() {
    let rows = sample_rows();
    let mut table = encode_immutable_table(&rows, 4096, 2, TableCompression::Uncompressed)
        .expect("encode table");
    let fields = first_index_entry_fields(&table);
    let block_offset = usize::try_from(u64::from_le_bytes(
        table[fields.block_offset..fields.block_offset + 8]
            .try_into()
            .expect("block offset"),
    ))
    .expect("block offset fits usize");
    let block_len = u32::from_le_bytes(
        table[fields.block_frame_len..fields.block_frame_len + 4]
            .try_into()
            .expect("block len"),
    ) as usize;

    table[block_offset] = 2;
    refresh_frame_crc(&mut table[block_offset..block_offset + block_len]);
    refresh_table_crc(&mut table);

    assert_eq!(
        decode_immutable_table(&table),
        Err(FormatError::InvalidValue {
            field: "block_type"
        })
    );
}

#[test]
fn immutable_table_rejects_missing_or_extra_data_blocks() {
    let rows = sample_rows();
    let first = TableDataBlock::from_rows(&rows[..2]).expect("first block");
    let second = TableDataBlock::from_rows(&rows[2..]).expect("second block");
    let first_frame = encoded_data_frame(&first);
    let first_index = TableIndexBlock::new(vec![index_entry_for_block(
        &first,
        TABLE_HEADER_SIZE,
        first_frame.len(),
    )])
    .expect("index");
    let first_properties =
        TableProperties::from_data_blocks(std::slice::from_ref(&first)).expect("properties");
    let header = TableHeader::new(
        4096,
        1,
        first_properties.row_count(),
        first_properties.commit_min(),
        first_properties.commit_max(),
    )
    .expect("header");

    let extra = assemble_table(
        header,
        &[first.clone(), second.clone()],
        &[
            TableCompression::Uncompressed,
            TableCompression::Uncompressed,
        ],
        &first_index,
        &first_properties,
    );
    assert_eq!(
        decode_immutable_table(&extra),
        Err(FormatError::InvalidValue {
            field: "data_block_count"
        })
    );

    let second_offset = TABLE_HEADER_SIZE + first_frame.len();
    let second_frame = encoded_data_frame(&second);
    let two_index = TableIndexBlock::new(vec![
        index_entry_for_block(&first, TABLE_HEADER_SIZE, first_frame.len()),
        index_entry_for_block(&second, second_offset, second_frame.len()),
    ])
    .expect("two-entry index");
    let two_properties =
        TableProperties::from_data_blocks(&[first.clone(), second]).expect("properties");
    let two_header = TableHeader::new(
        4096,
        2,
        two_properties.row_count(),
        two_properties.commit_min(),
        two_properties.commit_max(),
    )
    .expect("header");
    let missing = assemble_table(
        two_header,
        &[first],
        &[TableCompression::Uncompressed],
        &two_index,
        &two_properties,
    );
    assert_eq!(
        decode_immutable_table(&missing),
        Err(FormatError::InvalidValue {
            field: "data_block_count"
        })
    );
}

#[test]
fn immutable_table_rejects_hidden_bytes_between_regions() {
    let rows = sample_rows();
    let table = encode_immutable_table(&rows, 4096, 2, TableCompression::Uncompressed)
        .expect("encode table");
    let (index_range, properties_range) = footer_ranges(&table);

    let mut between_data_and_index = table.clone();
    between_data_and_index.insert(index_range.start, 0xff);
    shift_footer_offsets(&mut between_data_and_index, 1, 1);
    refresh_table_crc(&mut between_data_and_index);
    assert!(matches!(
        decode_immutable_table(&between_data_and_index),
        Err(FormatError::InsufficientBytes {
            format: "table_block_frame",
            ..
        } | FormatError::InvalidValue {
            field: "block_type",
        })
    ));

    let mut between_index_and_properties = table.clone();
    between_index_and_properties.insert(properties_range.start, 0xff);
    shift_footer_offsets(&mut between_index_and_properties, 0, 1);
    refresh_table_crc(&mut between_index_and_properties);
    assert_eq!(
        decode_immutable_table(&between_index_and_properties),
        Err(FormatError::InvalidLength {
            field: "properties_block_offset"
        })
    );

    let mut between_properties_and_footer = table;
    let footer_start = footer_start(&between_properties_and_footer);
    between_properties_and_footer.insert(footer_start, 0xff);
    refresh_table_crc(&mut between_properties_and_footer);
    assert_eq!(
        decode_immutable_table(&between_properties_and_footer),
        Err(FormatError::InvalidLength {
            field: "properties_block_range"
        })
    );
}

#[test]
fn immutable_table_rejects_trailing_bytes_after_footer() {
    let rows = sample_rows();
    let mut table = encode_immutable_table(&rows, 4096, 2, TableCompression::Uncompressed)
        .expect("encode table");
    table.extend_from_slice(b"trailing");

    assert!(matches!(
        decode_immutable_table(&table),
        Err(FormatError::ChecksumMismatch {
            format: TABLE_FOOTER_FORMAT,
            ..
        })
    ));
}

#[test]
fn immutable_table_rejects_short_tables_and_impossible_footer_offsets() {
    assert_eq!(
        decode_immutable_table(&[0; TABLE_HEADER_SIZE + TABLE_FOOTER_SIZE - 1]),
        Err(FormatError::InsufficientBytes {
            format: "immutable_table",
            needed: TABLE_HEADER_SIZE + TABLE_FOOTER_SIZE,
            actual: TABLE_HEADER_SIZE + TABLE_FOOTER_SIZE - 1
        })
    );

    let rows = sample_rows();
    let mut table = encode_immutable_table(&rows, 4096, 2, TableCompression::Uncompressed)
        .expect("encode table");
    let footer_start = footer_start(&table);
    table[footer_start..footer_start + 8].copy_from_slice(&u64::MAX.to_le_bytes());
    refresh_table_crc(&mut table);

    assert_eq!(
        decode_immutable_table(&table),
        Err(FormatError::InvalidLength {
            field: "index_block"
        })
    );
}

#[test]
fn immutable_table_rejects_header_from_different_table_after_valid_crc() {
    let first = encode_immutable_table(
        &[put(b"alpha".to_vec(), 9), tombstone(b"alpha".to_vec(), 7)],
        4096,
        8,
        TableCompression::Uncompressed,
    )
    .expect("first table");
    let second = encode_immutable_table(
        &[put(b"beta".to_vec(), 6), put(b"gamma".to_vec(), 5)],
        4096,
        8,
        TableCompression::Uncompressed,
    )
    .expect("second table");

    let mut mixed = first[..TABLE_HEADER_SIZE].to_vec();
    mixed.extend_from_slice(&second[TABLE_HEADER_SIZE..]);
    refresh_table_crc(&mut mixed);

    assert_eq!(
        decode_immutable_table(&mixed),
        Err(FormatError::InvalidValue {
            field: "table_properties_facts"
        })
    );
}

#[test]
fn immutable_table_preserves_storage_space_ids_as_opaque_row_facts() {
    let physical_key = PhysicalKey::new(
        branch(3),
        "default",
        StorageSpaceId::COMMIT_TIMELINE,
        b"timeline-row".to_vec(),
    )
    .expect("physical key");
    let row = StorageRow::put(
        physical_key,
        CommitVersion::new(11),
        timestamp(11),
        Timestamp::EPOCH,
        b"timeline".to_vec(),
    );

    let bytes = encode_immutable_table(
        std::slice::from_ref(&row),
        4096,
        8,
        TableCompression::Uncompressed,
    )
    .expect("encode table");
    let decoded = decode_immutable_table(&bytes).expect("decode table");

    assert_eq!(decoded.rows(), &[row]);
    assert_eq!(
        decoded.rows()[0].physical_key().storage_space_id(),
        StorageSpaceId::COMMIT_TIMELINE
    );
}

fn encoded_data_frame(block: &TableDataBlock) -> Vec<u8> {
    let payload = super::data::encode_table_data_block(block).expect("data payload");
    let frame = TableBlockFrame::new(
        TableBlockKind::Data,
        TableCompression::Uncompressed,
        payload,
    )
    .expect("data frame");
    encode_table_block_frame(&frame).expect("encode frame")
}

fn shift_footer_offsets(table: &mut [u8], index_delta: u64, properties_delta: u64) {
    let footer_start = footer_start(table);
    let index_offset = u64::from_le_bytes(
        table[footer_start..footer_start + 8]
            .try_into()
            .expect("index"),
    );
    let properties_offset = u64::from_le_bytes(
        table[footer_start + 24..footer_start + 32]
            .try_into()
            .expect("properties"),
    );
    table[footer_start..footer_start + 8]
        .copy_from_slice(&(index_offset + index_delta).to_le_bytes());
    table[footer_start + 24..footer_start + 32]
        .copy_from_slice(&(properties_offset + properties_delta).to_le_bytes());
}

// BS4.2: a filtered table (nonzero footer slot, filter frame between data and index) round-trips and
// carries its filter; malformed / corrupt filter frames are rejected before a reader could trust them.

fn filtered_table(bits: [u8; 8]) -> (Vec<StorageRow>, TableFilterFrame, Vec<u8>) {
    let rows = vec![put(b"alpha".to_vec(), 7), put(b"beta".to_vec(), 6)];
    let filter = TableFilterFrame {
        probes: 3,
        key_count: 2,
        bit_count: 64,
        bits: bits.to_vec(),
    };
    let table = encode_immutable_table_with_block_compressions(
        &rows,
        4096,
        2,
        &[TableCompression::Uncompressed],
        Some(&filter),
    )
    .expect("encode filtered table");
    (rows, filter, table)
}

#[test]
fn filtered_table_round_trips_and_carries_the_filter() {
    let (rows, filter, table) = filtered_table([0xFF; 8]);
    let decoded = decode_immutable_table(&table).expect("decode filtered table");
    assert_eq!(decoded.rows(), rows.as_slice());
    assert_eq!(decoded.filter(), Some(&filter));
    assert!(decode_table_footer(&table).expect("footer").has_filter());
}

#[test]
fn filtered_table_rejects_filter_slot_outside_the_data_index_gap() {
    let (_, _, mut table) = filtered_table([0xFF; 8]);
    let footer_start = table.len() - TABLE_FOOTER_SIZE;
    // Grow the recorded filter length so filter_end no longer meets index_start.
    let filter_len = u32::from_le_bytes(
        table[footer_start + 20..footer_start + 24]
            .try_into()
            .expect("filter len bytes"),
    );
    table[footer_start + 20..footer_start + 24].copy_from_slice(&(filter_len + 1).to_le_bytes());
    refresh_table_crc(&mut table);
    assert!(matches!(
        decode_immutable_table(&table),
        Err(FormatError::InvalidLength { .. })
    ));
}

#[test]
fn filtered_table_rejects_flipped_filter_frame_bit() {
    let (_, _, mut table) = filtered_table([0xAB; 8]);
    let footer_start = table.len() - TABLE_FOOTER_SIZE;
    let filter_offset = usize::try_from(u64::from_le_bytes(
        table[footer_start + 12..footer_start + 20]
            .try_into()
            .expect("filter offset bytes"),
    ))
    .expect("filter offset fits usize");
    // Flip a byte inside the filter frame payload; refresh the whole-table CRC so only the filter
    // frame's own CRC is left broken.
    table[filter_offset + TABLE_BLOCK_FRAME_HEADER_SIZE] ^= 0x01;
    refresh_table_crc(&mut table);
    assert!(matches!(
        decode_immutable_table(&table),
        Err(FormatError::ChecksumMismatch { .. })
    ));
}
