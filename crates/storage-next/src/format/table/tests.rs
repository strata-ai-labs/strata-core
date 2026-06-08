use super::{
    decode_table_block_frame, decode_table_block_frame_as, decode_table_footer,
    decode_table_header, encode_table_block_frame, encode_table_footer, encode_table_header,
    FormatError, TableBlockFrame, TableBlockKind, TableCompression, TableFooter, TableHeader,
    MAX_TABLE_DATA_BLOCKS, MAX_TABLE_ROWS, TABLE_BLOCK_FRAME_FORMAT, TABLE_BLOCK_FRAME_HEADER_SIZE,
    TABLE_FOOTER_FORMAT, TABLE_FOOTER_SIZE, TABLE_HEADER_FORMAT, TABLE_HEADER_SIZE,
};
use strata_core_next::CommitVersion;

const INDEX_OFFSET: u64 = TABLE_HEADER_SIZE as u64;
const INDEX_LEN: u32 = 16;
const PROPERTIES_OFFSET: u64 = INDEX_OFFSET + INDEX_LEN as u64;
const PROPERTIES_LEN: u32 = 16;
const FOOTER_START: usize = TABLE_HEADER_SIZE + INDEX_LEN as usize + PROPERTIES_LEN as usize;

fn header() -> TableHeader {
    TableHeader::new(4096, 2, 10, CommitVersion::new(3), CommitVersion::new(9))
        .expect("table header")
}

fn footer() -> TableFooter {
    TableFooter::new(INDEX_OFFSET, INDEX_LEN, PROPERTIES_OFFSET, PROPERTIES_LEN)
        .expect("table footer")
}

fn table_prefix() -> Vec<u8> {
    let mut prefix = vec![0x5a; FOOTER_START];
    prefix[..4].copy_from_slice(b"STTB");
    prefix
}

fn table_with_footer(footer: &TableFooter) -> Vec<u8> {
    let mut table = table_prefix();
    table.extend_from_slice(&encode_table_footer(footer, &table).expect("encode footer"));
    table
}

fn refresh_table_crc(table: &mut [u8]) {
    let crc = crc32fast::hash(&table[..table.len() - 4]);
    let crc_offset = table.len() - 4;
    table[crc_offset..].copy_from_slice(&crc.to_le_bytes());
}

fn frame(kind: TableBlockKind, compression: TableCompression) -> TableBlockFrame {
    TableBlockFrame::new(kind, compression, b"table payload".to_vec()).expect("block frame")
}

fn refresh_frame_crc(bytes: &mut [u8]) {
    let encoded_len = u32::from_le_bytes(bytes[4..8].try_into().expect("encoded len")) as usize;
    let crc_offset = TABLE_BLOCK_FRAME_HEADER_SIZE + encoded_len;
    let crc = crc32fast::hash(&bytes[..crc_offset]);
    bytes[crc_offset..crc_offset + 4].copy_from_slice(&crc.to_le_bytes());
}

fn invalid_zstd_frame() -> Vec<u8> {
    let encoded_payload = b"not zstd";
    let mut bytes = Vec::new();
    bytes.push(1);
    bytes.push(1);
    bytes.extend_from_slice(&0u16.to_le_bytes());
    let encoded_len = u32::try_from(encoded_payload.len()).expect("test payload len fits");
    bytes.extend_from_slice(&encoded_len.to_le_bytes());
    bytes.extend_from_slice(&32u32.to_le_bytes());
    bytes.extend_from_slice(encoded_payload);
    let crc = crc32fast::hash(&bytes);
    bytes.extend_from_slice(&crc.to_le_bytes());
    bytes
}

#[test]
fn table_header_round_trips() {
    let header = header();
    let bytes = encode_table_header(&header);

    assert_eq!(bytes.len(), TABLE_HEADER_SIZE);
    assert_eq!(decode_table_header(&bytes), Ok((header, TABLE_HEADER_SIZE)));
}

#[test]
fn table_header_rejects_magic_versions_size_flags_and_reserved_bytes() {
    let valid = encode_table_header(&header());

    let mut bad_magic = valid.clone();
    // Old development table files start with STRAKV; stable V1 requires STTB.
    bad_magic[..4].copy_from_slice(b"STRA");
    assert_eq!(
        decode_table_header(&bad_magic),
        Err(FormatError::InvalidMagic {
            format: TABLE_HEADER_FORMAT
        })
    );

    let mut pre_v1 = valid.clone();
    pre_v1[4..8].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        decode_table_header(&pre_v1),
        Err(FormatError::PreV1Format {
            format: TABLE_HEADER_FORMAT,
            version: 0
        })
    );

    let mut future = valid.clone();
    future[4..8].copy_from_slice(&2u32.to_le_bytes());
    assert_eq!(
        decode_table_header(&future),
        Err(FormatError::FutureFormat {
            format: TABLE_HEADER_FORMAT,
            version: 2,
            max_supported: 1
        })
    );

    let mut bad_size = valid.clone();
    bad_size[8..12].copy_from_slice(&63u32.to_le_bytes());
    assert_eq!(
        decode_table_header(&bad_size),
        Err(FormatError::InvalidLength {
            field: "table_header_size"
        })
    );

    let mut flags = valid.clone();
    flags[12..16].copy_from_slice(&1u32.to_le_bytes());
    assert_eq!(
        decode_table_header(&flags),
        Err(FormatError::UnsupportedFlags {
            format: TABLE_HEADER_FORMAT,
            flags: 1
        })
    );

    let mut reserved = valid;
    reserved[48] = 1;
    assert_eq!(
        decode_table_header(&reserved),
        Err(FormatError::InvalidValue { field: "reserved" })
    );
}

#[test]
fn table_header_rejects_invalid_facts() {
    assert_eq!(
        TableHeader::new(0, 1, 1, CommitVersion::new(1), CommitVersion::new(1)),
        Err(FormatError::InvalidLength {
            field: "target_data_block_size"
        })
    );
    assert_eq!(
        TableHeader::new(4096, 0, 1, CommitVersion::new(1), CommitVersion::new(1)),
        Err(FormatError::InvalidLength {
            field: "data_block_count"
        })
    );
    assert_eq!(
        TableHeader::new(4096, 1, 0, CommitVersion::new(1), CommitVersion::new(1)),
        Err(FormatError::InvalidLength { field: "row_count" })
    );
    assert_eq!(
        TableHeader::new(4096, 1, 1, CommitVersion::new(2), CommitVersion::new(1)),
        Err(FormatError::InvalidValue {
            field: "commit_range"
        })
    );

    assert_eq!(
        TableHeader::new(
            4096,
            MAX_TABLE_DATA_BLOCKS + 1,
            1,
            CommitVersion::new(1),
            CommitVersion::new(1)
        ),
        Err(FormatError::InvalidLength {
            field: "data_block_count"
        })
    );
    assert_eq!(
        TableHeader::new(
            4096,
            1,
            MAX_TABLE_ROWS + 1,
            CommitVersion::new(1),
            CommitVersion::new(1)
        ),
        Err(FormatError::InvalidLength { field: "row_count" })
    );

    let valid = encode_table_header(&header());
    let mut too_many_blocks = valid.clone();
    too_many_blocks[20..24].copy_from_slice(&(MAX_TABLE_DATA_BLOCKS + 1).to_le_bytes());
    assert_eq!(
        decode_table_header(&too_many_blocks),
        Err(FormatError::InvalidLength {
            field: "data_block_count"
        })
    );

    let mut too_many_rows = valid;
    too_many_rows[24..32].copy_from_slice(&(MAX_TABLE_ROWS + 1).to_le_bytes());
    assert_eq!(
        decode_table_header(&too_many_rows),
        Err(FormatError::InvalidLength { field: "row_count" })
    );
}

#[test]
fn table_header_rejects_truncated_header() {
    let bytes = encode_table_header(&header());

    for len in [0, 1, 4, 8, 12, 16, 20, 24, 32, 40, 48, 63] {
        assert_eq!(
            decode_table_header(&bytes[..len]),
            Err(FormatError::InsufficientBytes {
                format: TABLE_HEADER_FORMAT,
                needed: TABLE_HEADER_SIZE,
                actual: len
            })
        );
    }
}

#[test]
fn table_footer_round_trips_with_absent_filter() {
    let footer = footer();
    let table = table_with_footer(&footer);

    assert_eq!(table.len(), FOOTER_START + TABLE_FOOTER_SIZE);
    assert_eq!(decode_table_footer(&table), Ok(footer));
}

#[test]
fn table_footer_rejects_checksum_before_trusting_offsets() {
    let footer = footer();
    let mut table = table_with_footer(&footer);
    table[0] ^= 0xff;

    assert!(matches!(
        decode_table_footer(&table),
        Err(FormatError::ChecksumMismatch {
            format: TABLE_FOOTER_FORMAT,
            ..
        })
    ));
}

#[test]
fn table_footer_rejects_magic_reserved_and_filter_fields_after_valid_crc() {
    let footer = footer();
    let mut bad_magic = table_with_footer(&footer);
    let footer_start = bad_magic.len() - TABLE_FOOTER_SIZE;
    bad_magic[footer_start + 36..footer_start + 40].copy_from_slice(b"NOPE");
    refresh_table_crc(&mut bad_magic);
    assert_eq!(
        decode_table_footer(&bad_magic),
        Err(FormatError::InvalidMagic {
            format: TABLE_FOOTER_FORMAT
        })
    );

    let mut reserved = table_with_footer(&footer);
    let footer_start = reserved.len() - TABLE_FOOTER_SIZE;
    reserved[footer_start + 40] = 1;
    refresh_table_crc(&mut reserved);
    assert_eq!(
        decode_table_footer(&reserved),
        Err(FormatError::InvalidValue { field: "reserved" })
    );

    for (filter_offset, filter_len) in [(1u64, 0u32), (0, 1), (1, 1)] {
        let mut filter = table_with_footer(&footer);
        let footer_start = filter.len() - TABLE_FOOTER_SIZE;
        filter[footer_start + 12..footer_start + 20].copy_from_slice(&filter_offset.to_le_bytes());
        filter[footer_start + 20..footer_start + 24].copy_from_slice(&filter_len.to_le_bytes());
        refresh_table_crc(&mut filter);
        assert_eq!(
            decode_table_footer(&filter),
            Err(FormatError::InvalidValue {
                field: "filter_block"
            })
        );
    }
}

#[test]
fn table_footer_rejects_offsets_before_canonical_body_and_zero_lengths() {
    let prefix = table_prefix();

    assert_eq!(
        encode_table_footer(
            &TableFooter::new(63, INDEX_LEN, PROPERTIES_OFFSET, PROPERTIES_LEN).expect("footer"),
            &prefix
        ),
        Err(FormatError::InvalidLength {
            field: "index_block_offset"
        })
    );
    assert_eq!(
        TableFooter::new(INDEX_OFFSET, 0, PROPERTIES_OFFSET, PROPERTIES_LEN),
        Err(FormatError::InvalidLength {
            field: "index_block_frame_len"
        })
    );
    assert_eq!(
        TableFooter::new(INDEX_OFFSET, INDEX_LEN, PROPERTIES_OFFSET, 0),
        Err(FormatError::InvalidLength {
            field: "properties_block_frame_len"
        })
    );
}

#[test]
fn table_footer_rejects_overlaps_and_hidden_bytes() {
    let prefix = table_prefix();

    assert_eq!(
        encode_table_footer(
            &TableFooter::new(INDEX_OFFSET, 32, PROPERTIES_OFFSET, PROPERTIES_LEN).expect("footer"),
            &prefix
        ),
        Err(FormatError::InvalidLength {
            field: "properties_block_offset"
        })
    );
    assert_eq!(
        encode_table_footer(
            &TableFooter::new(
                INDEX_OFFSET,
                INDEX_LEN,
                PROPERTIES_OFFSET + 1,
                PROPERTIES_LEN
            )
            .expect("footer"),
            &[0; FOOTER_START + 1]
        ),
        Err(FormatError::InvalidLength {
            field: "properties_block_offset"
        })
    );
    assert_eq!(
        encode_table_footer(
            &TableFooter::new(
                INDEX_OFFSET,
                INDEX_LEN,
                PROPERTIES_OFFSET,
                PROPERTIES_LEN - 1
            )
            .expect("footer"),
            &prefix
        ),
        Err(FormatError::InvalidLength {
            field: "properties_block_range"
        })
    );
}

#[test]
fn table_footer_rejects_offset_overflow_and_ranges_past_footer() {
    let prefix = table_prefix();
    let footer_start = u32::try_from(FOOTER_START).expect("footer start fits in u32");

    assert_eq!(
        encode_table_footer(
            &TableFooter::new(u64::MAX, INDEX_LEN, PROPERTIES_OFFSET, PROPERTIES_LEN)
                .expect("footer"),
            &prefix
        ),
        Err(FormatError::InvalidLength {
            field: "index_block"
        })
    );
    assert_eq!(
        encode_table_footer(
            &TableFooter::new(
                INDEX_OFFSET,
                footer_start,
                PROPERTIES_OFFSET,
                PROPERTIES_LEN
            )
            .expect("footer"),
            &prefix
        ),
        Err(FormatError::InvalidLength {
            field: "index_block_range"
        })
    );
    assert_eq!(
        encode_table_footer(
            &TableFooter::new(INDEX_OFFSET, INDEX_LEN, u64::MAX, PROPERTIES_LEN).expect("footer"),
            &prefix
        ),
        Err(FormatError::InvalidLength {
            field: "properties_block"
        })
    );
    assert_eq!(
        encode_table_footer(
            &TableFooter::new(
                INDEX_OFFSET,
                INDEX_LEN,
                PROPERTIES_OFFSET,
                PROPERTIES_LEN + 1
            )
            .expect("footer"),
            &prefix
        ),
        Err(FormatError::InvalidLength {
            field: "properties_block_range"
        })
    );
}

#[test]
fn table_footer_rejects_truncated_footer() {
    let table = table_with_footer(&footer());

    for len in [0, 1, 8, 12, 20, 24, 32, 36, 40, 60, TABLE_FOOTER_SIZE - 1] {
        assert_eq!(
            decode_table_footer(&table[..len]),
            Err(FormatError::InsufficientBytes {
                format: TABLE_FOOTER_FORMAT,
                needed: TABLE_FOOTER_SIZE,
                actual: len
            })
        );
    }
}

#[test]
fn table_block_frame_round_trips_uncompressed_and_zstd() {
    for compression in [TableCompression::Uncompressed, TableCompression::Zstd] {
        let frame = frame(TableBlockKind::Data, compression);
        let bytes = encode_table_block_frame(&frame).expect("encode frame");

        assert_eq!(decode_table_block_frame(&bytes), Ok((frame, bytes.len())));
    }
}

#[test]
fn table_block_frame_rejects_empty_payloads_for_all_table_block_kinds() {
    for kind in [
        TableBlockKind::Data,
        TableBlockKind::Index,
        TableBlockKind::Properties,
    ] {
        assert_eq!(
            TableBlockFrame::new(kind, TableCompression::Uncompressed, Vec::new()),
            Err(FormatError::InvalidLength {
                field: "decoded_len"
            })
        );
    }
}

#[test]
fn table_block_frame_rejects_reserved_or_unknown_block_type() {
    for block_type in [3u8, 99] {
        let frame = frame(TableBlockKind::Data, TableCompression::Uncompressed);
        let mut bytes = encode_table_block_frame(&frame).expect("encode frame");
        bytes[0] = block_type;
        refresh_frame_crc(&mut bytes);

        assert_eq!(
            decode_table_block_frame(&bytes),
            Err(FormatError::InvalidValue {
                field: "block_type"
            })
        );
    }
}

#[test]
fn table_block_frame_rejects_unknown_compression_flags_and_length_mismatch() {
    let frame = frame(TableBlockKind::Data, TableCompression::Uncompressed);
    let valid = encode_table_block_frame(&frame).expect("encode frame");

    let mut codec = valid.clone();
    codec[1] = 9;
    refresh_frame_crc(&mut codec);
    assert_eq!(
        decode_table_block_frame(&codec),
        Err(FormatError::UnsupportedCompression {
            format: TABLE_BLOCK_FRAME_FORMAT,
            codec: 9
        })
    );

    let mut flags = valid.clone();
    flags[2..4].copy_from_slice(&1u16.to_le_bytes());
    refresh_frame_crc(&mut flags);
    assert_eq!(
        decode_table_block_frame(&flags),
        Err(FormatError::UnsupportedFlags {
            format: TABLE_BLOCK_FRAME_FORMAT,
            flags: 1
        })
    );

    let mut decoded_len = valid;
    decoded_len[8..12].copy_from_slice(&1u32.to_le_bytes());
    refresh_frame_crc(&mut decoded_len);
    assert_eq!(
        decode_table_block_frame(&decoded_len),
        Err(FormatError::InvalidLength {
            field: "table_block_lengths"
        })
    );
}

#[test]
fn table_block_frame_rejects_invalid_zstd_payload_or_decoded_length() {
    let invalid = invalid_zstd_frame();
    assert_eq!(
        decode_table_block_frame(&invalid),
        Err(FormatError::DecompressionFailed {
            format: TABLE_BLOCK_FRAME_FORMAT
        })
    );

    let frame = frame(TableBlockKind::Data, TableCompression::Zstd);
    let mut bytes = encode_table_block_frame(&frame).expect("encode frame");
    let wrong_len =
        u32::try_from(frame.decoded_payload().len()).expect("test payload len fits") + 1;
    bytes[8..12].copy_from_slice(&wrong_len.to_le_bytes());
    refresh_frame_crc(&mut bytes);
    assert_eq!(
        decode_table_block_frame(&bytes),
        Err(FormatError::InvalidLength {
            field: "decoded_len"
        })
    );
}

#[test]
fn table_block_frame_rejects_checksum_and_truncation() {
    let frame = frame(TableBlockKind::Data, TableCompression::Uncompressed);
    let bytes = encode_table_block_frame(&frame).expect("encode frame");

    let mut checksum = bytes.clone();
    checksum[TABLE_BLOCK_FRAME_HEADER_SIZE] ^= 0xff;
    assert!(matches!(
        decode_table_block_frame(&checksum),
        Err(FormatError::ChecksumMismatch {
            format: TABLE_BLOCK_FRAME_FORMAT,
            ..
        })
    ));

    assert_eq!(
        decode_table_block_frame(&bytes[..TABLE_BLOCK_FRAME_HEADER_SIZE - 1]),
        Err(FormatError::InsufficientBytes {
            format: TABLE_BLOCK_FRAME_FORMAT,
            needed: TABLE_BLOCK_FRAME_HEADER_SIZE,
            actual: TABLE_BLOCK_FRAME_HEADER_SIZE - 1
        })
    );
    let encoded_len = u32::from_le_bytes(bytes[4..8].try_into().expect("encoded len")) as usize;
    let truncated_payload_len = TABLE_BLOCK_FRAME_HEADER_SIZE + encoded_len - 1;
    assert_eq!(
        decode_table_block_frame(&bytes[..truncated_payload_len]),
        Err(FormatError::InsufficientBytes {
            format: TABLE_BLOCK_FRAME_FORMAT,
            needed: bytes.len(),
            actual: truncated_payload_len
        })
    );
    assert_eq!(
        decode_table_block_frame(&bytes[..bytes.len() - 1]),
        Err(FormatError::InsufficientBytes {
            format: TABLE_BLOCK_FRAME_FORMAT,
            needed: bytes.len(),
            actual: bytes.len() - 1
        })
    );
}

#[test]
fn table_block_frame_rejects_oversized_lengths_before_payload_read() {
    for (offset, field) in [(4, "encoded_len"), (8, "decoded_len")] {
        let frame = frame(TableBlockKind::Data, TableCompression::Uncompressed);
        let mut bytes = encode_table_block_frame(&frame).expect("encode frame");
        bytes[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());

        assert_eq!(
            decode_table_block_frame(&bytes),
            Err(FormatError::InvalidLength { field })
        );
    }
}

#[test]
fn table_block_frame_consumes_one_frame_and_rejects_wrong_expected_kind() {
    let frame = frame(TableBlockKind::Index, TableCompression::Uncompressed);
    let bytes = encode_table_block_frame(&frame).expect("encode frame");
    let mut sequence = bytes.clone();
    sequence.extend_from_slice(&bytes);

    assert_eq!(
        decode_table_block_frame(&sequence),
        Ok((frame.clone(), bytes.len()))
    );
    assert_eq!(
        decode_table_block_frame_as(&sequence, TableBlockKind::Data),
        Err(FormatError::InvalidValue {
            field: "block_type"
        })
    );
    assert_eq!(
        decode_table_block_frame_as(&sequence, TableBlockKind::Index),
        Ok((frame, bytes.len()))
    );
}
