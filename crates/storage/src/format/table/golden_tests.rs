use super::artifact::{
    decode_immutable_table, encode_immutable_table, encode_immutable_table_with_block_compressions,
};
use super::data::{decode_table_data_block, encode_table_data_block, TableDataBlock};
use super::index::{decode_table_index_block, encode_table_index_block, TableIndexBlock};
use super::properties::{decode_table_properties, encode_table_properties, TableProperties};
use super::test_support::{put, tombstone};
use super::{
    decode_filter_frame, decode_table_block_frame, encode_filter_frame, encode_table_block_frame,
    TableBlockFrame, TableBlockKind, TableCompression, TableFilterFrame,
};

const TABLE_DATA_BLOCK_ONE_PUT_UNCOMPRESSED_FRAME: &str = include_str!(
    "../../../testdata/goldens/storage-format-v1/table-data-block-one-put-uncompressed-frame.hex"
);
const TABLE_DATA_BLOCK_PUT_TOMBSTONE_UNCOMPRESSED_FRAME: &str = include_str!(
    "../../../testdata/goldens/storage-format-v1/table-data-block-put-tombstone-uncompressed-frame.hex"
);
const TABLE_DATA_BLOCK_ZSTD_FRAME: &str =
    include_str!("../../../testdata/goldens/storage-format-v1/table-data-block-zstd-frame.hex");
const TABLE_INDEX_BLOCK: &str =
    include_str!("../../../testdata/goldens/storage-format-v1/table-index-block.hex");
const TABLE_PROPERTIES_BLOCK: &str =
    include_str!("../../../testdata/goldens/storage-format-v1/table-properties-block.hex");
const IMMUTABLE_TABLE_ONE_BLOCK: &str =
    include_str!("../../../testdata/goldens/storage-format-v1/immutable-table-one-block.hex");
const IMMUTABLE_TABLE_TWO_BLOCK: &str =
    include_str!("../../../testdata/goldens/storage-format-v1/immutable-table-two-block.hex");
const TABLE_FILTER_FRAME: &str =
    include_str!("../../../testdata/goldens/storage-format-v1/table-filter-frame.hex");
const IMMUTABLE_TABLE_FILTERED: &str =
    include_str!("../../../testdata/goldens/storage-format-v1/immutable-table-filtered.hex");
const IMMUTABLE_TABLE_FILTERED_MULTI_BLOCK: &str = include_str!(
    "../../../testdata/goldens/storage-format-v1/immutable-table-filtered-multi-block.hex"
);

fn golden_filter_frame() -> TableFilterFrame {
    TableFilterFrame {
        probes: 7,
        key_count: 3,
        bit_count: 64,
        bits: vec![0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef],
    }
}

#[test]
fn table_filter_frame_matches_golden_vector() {
    let frame = golden_filter_frame();
    let golden = parse_hex(TABLE_FILTER_FRAME);
    assert_eq!(
        encode_filter_frame(&frame).expect("encode filter frame"),
        golden
    );
    let (decoded, consumed) = decode_filter_frame(&golden).expect("decode filter frame");
    assert_eq!(consumed, golden.len());
    assert_eq!(decoded, frame);
}

#[test]
fn immutable_table_filtered_matches_golden_vector() {
    let rows = vec![put(b"alpha".to_vec(), 7), put(b"beta".to_vec(), 6)];
    let frame = golden_filter_frame();
    let golden = parse_hex(IMMUTABLE_TABLE_FILTERED);
    assert_eq!(
        encode_immutable_table_with_block_compressions(
            &rows,
            4096,
            2,
            &[TableCompression::Uncompressed],
            Some(&frame),
        )
        .expect("encode filtered table"),
        golden
    );
    let decoded = decode_immutable_table(&golden).expect("decode filtered table");
    assert_eq!(decoded.rows(), rows.as_slice());
    assert_eq!(decoded.filter(), Some(&frame));
}

#[test]
fn immutable_table_filtered_multi_block_matches_golden_vector() {
    let rows = two_block_rows();
    let frame = golden_filter_frame();
    let golden = parse_hex(IMMUTABLE_TABLE_FILTERED_MULTI_BLOCK);
    assert_eq!(
        encode_immutable_table_with_block_compressions(
            &rows,
            1024,
            2,
            &[TableCompression::Uncompressed; 2],
            Some(&frame),
        )
        .expect("encode multi-block filtered table"),
        golden
    );
    let decoded = decode_immutable_table(&golden).expect("decode multi-block filtered table");
    assert_eq!(decoded.rows(), rows.as_slice());
    assert_eq!(decoded.filter(), Some(&frame));
    // The filter must sit after BOTH data blocks — this is the multi-block layout the single-block
    // golden does not exercise.
    assert_eq!(decoded.data_blocks().len(), 2);
}

#[test]
fn table_data_block_one_put_frame_matches_golden_vector() {
    let block = TableDataBlock::from_rows(&[put(b"alpha".to_vec(), 9)]).expect("data block");
    assert_data_frame_matches_golden(
        &block,
        TableCompression::Uncompressed,
        TABLE_DATA_BLOCK_ONE_PUT_UNCOMPRESSED_FRAME,
    );
}

#[test]
fn table_data_block_put_tombstone_frame_matches_golden_vector() {
    let block = TableDataBlock::from_rows(&[put(b"alpha".to_vec(), 9), tombstone(b"alpha", 7)])
        .expect("data block");
    assert_data_frame_matches_golden(
        &block,
        TableCompression::Uncompressed,
        TABLE_DATA_BLOCK_PUT_TOMBSTONE_UNCOMPRESSED_FRAME,
    );
}

#[test]
#[cfg_attr(
    miri,
    ignore = "zstd C FFI is unsupported under Miri; ASAN lane covers compression paths"
)]
fn table_data_block_zstd_frame_matches_golden_vector() {
    let block = TableDataBlock::from_rows(&[put(b"alpha".to_vec(), 9), tombstone(b"alpha", 7)])
        .expect("data block");
    assert_data_frame_matches_golden(&block, TableCompression::Zstd, TABLE_DATA_BLOCK_ZSTD_FRAME);
}

#[test]
fn table_index_block_matches_golden_vector() {
    let table = decode_immutable_table(&parse_hex(IMMUTABLE_TABLE_TWO_BLOCK)).expect("table");
    let golden = parse_hex(TABLE_INDEX_BLOCK);

    assert_eq!(
        encode_table_index_block(table.index()).expect("encode index"),
        golden
    );
    assert_eq!(decode_table_index_block(&golden), Ok(table.index().clone()));
}

#[test]
fn table_properties_block_matches_golden_vector() {
    let table = decode_immutable_table(&parse_hex(IMMUTABLE_TABLE_TWO_BLOCK)).expect("table");
    let golden = parse_hex(TABLE_PROPERTIES_BLOCK);

    assert_eq!(
        encode_table_properties(table.properties()).expect("encode properties"),
        golden
    );
    assert_eq!(
        decode_table_properties(&golden),
        Ok(table.properties().clone())
    );
}

#[test]
fn immutable_table_one_block_matches_golden_vector() {
    let rows = vec![put(b"alpha".to_vec(), 9), tombstone(b"alpha", 7)];
    let golden = parse_hex(IMMUTABLE_TABLE_ONE_BLOCK);

    assert_eq!(
        encode_immutable_table(&rows, 4096, 8, TableCompression::Uncompressed)
            .expect("encode table"),
        golden
    );
    assert_eq!(
        decode_immutable_table(&golden)
            .expect("decode table")
            .rows(),
        rows.as_slice()
    );
}

#[test]
fn immutable_table_two_block_matches_golden_vector() {
    let rows = two_block_rows();
    let golden = parse_hex(IMMUTABLE_TABLE_TWO_BLOCK);

    assert_eq!(
        encode_immutable_table(&rows, 1024, 2, TableCompression::Uncompressed)
            .expect("encode table"),
        golden
    );
    let decoded = decode_immutable_table(&golden).expect("decode table");
    assert_eq!(decoded.rows(), rows.as_slice());
    assert_eq!(
        decoded.index(),
        &expected_two_block_index(decoded.data_blocks())
    );
    assert_eq!(
        decoded.properties(),
        &TableProperties::from_data_blocks(decoded.data_blocks()).expect("properties")
    );
}

fn assert_data_frame_matches_golden(
    block: &TableDataBlock,
    compression: TableCompression,
    golden_text: &str,
) {
    let payload = encode_table_data_block(block).expect("encode data block");
    let frame = TableBlockFrame::new(TableBlockKind::Data, compression, payload).expect("frame");
    let golden = parse_hex(golden_text);

    assert_eq!(
        encode_table_block_frame(&frame).expect("encode frame"),
        golden
    );
    let (decoded_frame, consumed) = decode_table_block_frame(&golden).expect("decode frame");
    assert_eq!(consumed, golden.len());
    assert_eq!(decoded_frame.kind(), TableBlockKind::Data);
    assert_eq!(decoded_frame.compression(), compression);
    assert_eq!(
        decode_table_data_block(decoded_frame.decoded_payload()).expect("decode data block"),
        *block
    );
}

fn two_block_rows() -> Vec<crate::row::StorageRow> {
    vec![
        put(b"alpha".to_vec(), 9),
        tombstone(b"alpha", 7),
        put(b"beta".to_vec(), 6),
        put(b"gamma".to_vec(), 5),
    ]
}

fn expected_two_block_index(blocks: &[TableDataBlock]) -> TableIndexBlock {
    let first_frame = data_frame_len(&blocks[0]);
    let first_offset = super::TABLE_HEADER_SIZE;
    let second_offset = first_offset + first_frame;
    let second_frame = data_frame_len(&blocks[1]);

    TableIndexBlock::new(vec![
        super::index::TableIndexEntry::new(
            blocks[0].entries()[0].internal_key(),
            blocks[0].entries()[blocks[0].entries().len() - 1].internal_key(),
            u64::try_from(first_offset).expect("first offset"),
            u32::try_from(first_frame).expect("first frame"),
            u32::try_from(blocks[0].row_count()).expect("first rows"),
        )
        .expect("first index"),
        super::index::TableIndexEntry::new(
            blocks[1].entries()[0].internal_key(),
            blocks[1].entries()[blocks[1].entries().len() - 1].internal_key(),
            u64::try_from(second_offset).expect("second offset"),
            u32::try_from(second_frame).expect("second frame"),
            u32::try_from(blocks[1].row_count()).expect("second rows"),
        )
        .expect("second index"),
    ])
    .expect("index")
}

fn data_frame_len(block: &TableDataBlock) -> usize {
    let payload = encode_table_data_block(block).expect("data payload");
    let frame = TableBlockFrame::new(
        TableBlockKind::Data,
        TableCompression::Uncompressed,
        payload,
    )
    .expect("data frame");
    encode_table_block_frame(&frame)
        .expect("encode data frame")
        .len()
}

fn parse_hex(text: &str) -> Vec<u8> {
    let hex: String = text
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .flat_map(str::chars)
        .filter(|ch| !ch.is_whitespace())
        .collect();

    assert_eq!(hex.len() % 2, 0, "hex fixture has odd byte count");
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).expect("hex byte"))
        .collect()
}
