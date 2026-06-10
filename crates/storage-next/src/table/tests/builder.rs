use crate::format::{
    decode_immutable_table, encode_immutable_table, TableCompression, MAX_TABLE_KEY_BYTES,
};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    sort_table_rows_by_key, BuiltTableArtifact, ImmutableTableBuilder, ImmutableTableReader,
    MutableTable, TableBuilderConfig, TableCacheConfig, TableCompactionConfig, TableIdentity,
    TableReaderConfig, TableRow, TableRuntimeConfig, TableRuntimeError,
};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

fn branch(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn physical_key(branch_byte: u8, space_id: u8, user_key: impl Into<Vec<u8>>) -> PhysicalKey {
    PhysicalKey::new(
        branch(branch_byte),
        "builder",
        StorageSpaceId::from_raw(space_id).expect("storage space id"),
        user_key,
    )
    .expect("physical key")
}

fn put_row(user_key: impl Into<Vec<u8>>, version: u64) -> StorageRow {
    put_row_for_key(
        physical_key(1, 0x20, user_key),
        version,
        version.to_le_bytes().to_vec(),
    )
}

fn put_row_for_key(key: PhysicalKey, version: u64, value: Vec<u8>) -> StorageRow {
    StorageRow::put(
        key,
        CommitVersion::new(version),
        Timestamp::from_micros(version.saturating_add(100)),
        Timestamp::EPOCH,
        value,
    )
}

fn expired_row(user_key: impl Into<Vec<u8>>, version: u64) -> StorageRow {
    StorageRow::put(
        physical_key(1, 0x20, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(version.saturating_add(100)),
        Timestamp::from_micros(1),
        b"expired".to_vec(),
    )
}

fn tombstone_row(user_key: impl Into<Vec<u8>>, version: u64) -> StorageRow {
    StorageRow::tombstone(
        physical_key(1, 0x20, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(version.saturating_add(100)),
    )
}

fn sorted_table_rows(rows: &[StorageRow]) -> Vec<TableRow> {
    let mut rows = rows.iter().cloned().map(TableRow::new).collect::<Vec<_>>();
    sort_table_rows_by_key(&mut rows);
    rows
}

fn sorted_storage_rows(rows: &[StorageRow]) -> Vec<StorageRow> {
    sorted_table_rows(rows)
        .into_iter()
        .map(TableRow::into_row)
        .collect()
}

fn identity(name: &'static str) -> TableIdentity {
    TableIdentity::new(name).expect("valid identity")
}

fn builder(rows_per_block: usize, compression: TableCompression) -> ImmutableTableBuilder {
    ImmutableTableBuilder::new(
        TableBuilderConfig::new(1024, rows_per_block, compression).expect("builder config"),
    )
    .expect("builder")
}

fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("u32 bytes"))
}

fn read_u64_le(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("u64 bytes"))
}

#[test]
fn immutable_builder_constructs_from_builder_and_runtime_config() {
    ImmutableTableBuilder::new(TableBuilderConfig::default()).expect("default builder config");
    assert_eq!(
        TableBuilderConfig::new(0, 1, TableCompression::Uncompressed),
        Err(TableRuntimeError::InvalidConfig {
            field: "target_data_block_size",
            reason: "must be nonzero",
        })
    );
    assert_eq!(
        TableBuilderConfig::new(1024, 0, TableCompression::Uncompressed),
        Err(TableRuntimeError::InvalidConfig {
            field: "rows_per_block",
            reason: "must be nonzero",
        })
    );
    assert!(matches!(
        TableIdentity::new(""),
        Err(TableRuntimeError::InvalidConfig {
            field: "table_identity",
            ..
        })
    ));
    assert!(matches!(
        TableIdentity::new("bad/name"),
        Err(TableRuntimeError::InvalidConfig {
            field: "table_identity",
            ..
        })
    ));

    let config = TableBuilderConfig::new(2048, 3, TableCompression::Uncompressed).expect("config");
    let builder = ImmutableTableBuilder::new(config).expect("builder");
    assert_eq!(builder.config(), config);

    let runtime = TableRuntimeConfig::new(
        config,
        TableReaderConfig::default(),
        TableCacheConfig::default(),
        TableCompactionConfig::default(),
    )
    .expect("runtime config");
    let from_runtime =
        ImmutableTableBuilder::from_runtime_config(&runtime).expect("builder from runtime config");
    assert_eq!(from_runtime.config(), config);

    let rows = sorted_table_rows(&[put_row(b"alpha".to_vec(), 1)]);
    let artifact: BuiltTableArtifact = builder
        .build_from_rows(identity("builder-config"), &rows)
        .expect("build artifact");
    let decoded = decode_immutable_table(artifact.bytes()).expect("decode artifact");
    assert_eq!(decoded.header().target_data_block_size(), 2048);
    assert_eq!(decoded.header().data_block_count(), 1);
    assert_eq!(decoded.header().row_count(), 1);
    assert_eq!(artifact.facts().row_count(), 1);
    assert_eq!(
        artifact.facts().key_range().first_key(),
        artifact.facts().key_range().last_key()
    );
    assert_eq!(
        artifact.facts().commit_range().min(),
        artifact.facts().commit_range().max()
    );
}

#[test]
fn immutable_builder_rejects_empty_unsorted_and_duplicate_inputs() {
    let builder = builder(2, TableCompression::Uncompressed);
    assert_eq!(
        builder.build_from_rows(identity("empty"), &[]),
        Err(TableRuntimeError::InvalidRange { field: "row_count" })
    );
    assert_eq!(
        builder.build_from_mutable(identity("empty-mutable"), &MutableTable::new()),
        Err(TableRuntimeError::InvalidRange { field: "row_count" })
    );
    assert_eq!(
        builder.build_from_frozen(identity("empty-frozen"), &MutableTable::new().freeze()),
        Err(TableRuntimeError::InvalidRange { field: "row_count" })
    );

    let rows = sorted_table_rows(&[put_row(b"alpha".to_vec(), 1), put_row(b"bravo".to_vec(), 1)]);
    let unsorted = vec![rows[1].clone(), rows[0].clone()];
    assert!(matches!(
        builder.build_from_rows(identity("unsorted"), &unsorted),
        Err(TableRuntimeError::InvalidRowOrder { .. })
    ));

    let duplicate = vec![rows[0].clone(), rows[0].clone()];
    assert!(matches!(
        builder.build_from_rows(identity("duplicate"), &duplicate),
        Err(TableRuntimeError::DuplicateInternalKey { .. })
    ));
}

#[test]
fn immutable_builder_rejects_oversized_keys_before_format_encoding() {
    let builder = builder(1, TableCompression::Uncompressed);
    let oversized_key = put_row(vec![0x61; MAX_TABLE_KEY_BYTES], 1);
    let rows = sorted_table_rows(&[oversized_key]);

    assert_eq!(
        builder.build_from_rows(identity("oversized-key"), &rows),
        Err(TableRuntimeError::InvalidRange {
            field: "internal_key_len"
        })
    );
}

#[test]
fn immutable_builder_outputs_table_format_one_block_and_multi_block_bytes() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
    ];
    let table_rows = sorted_table_rows(&rows);
    let expected_rows = sorted_storage_rows(&rows);

    let one_block = builder(8, TableCompression::Uncompressed)
        .build_from_rows(identity("one-block"), &table_rows)
        .expect("one block build");
    assert_eq!(&one_block.bytes()[..4], b"STTB");
    assert_ne!(&one_block.bytes()[..6], b"STRAKV");
    let decoded_one = decode_immutable_table(one_block.bytes()).expect("decode one block");
    assert_eq!(decoded_one.header().data_block_count(), 1);
    assert_eq!(decoded_one.data_blocks().len(), 1);
    assert_eq!(decoded_one.index().entry_count(), 1);
    assert_eq!(decoded_one.rows(), expected_rows.as_slice());

    let multi_block = builder(1, TableCompression::Uncompressed)
        .build_from_rows(identity("multi-block"), &table_rows)
        .expect("multi block build");
    let decoded_multi = decode_immutable_table(multi_block.bytes()).expect("decode multi block");
    assert_eq!(decoded_multi.header().data_block_count(), 3);
    assert_eq!(decoded_multi.data_blocks().len(), 3);
    assert_eq!(decoded_multi.index().entry_count(), 3);
    assert_eq!(decoded_multi.rows(), expected_rows.as_slice());
    for (index_entry, data_block) in decoded_multi
        .index()
        .entries()
        .iter()
        .zip(decoded_multi.data_blocks())
    {
        assert_eq!(index_entry.first_key_bytes(), data_block.first_key_bytes());
        assert_eq!(index_entry.last_key_bytes(), data_block.last_key_bytes());
        assert_eq!(index_entry.row_count() as usize, data_block.row_count());
    }
}

#[test]
fn immutable_builder_emits_canonical_table_header_footer_bytes() {
    const TABLE_HEADER_SIZE: usize = 64;
    const TABLE_FOOTER_SIZE: usize = 64;

    let rows = sorted_table_rows(&[
        put_row(b"alpha".to_vec(), 2),
        tombstone_row(b"bravo".to_vec(), 5),
        expired_row(b"charlie".to_vec(), 3),
    ]);
    let config = TableBuilderConfig::new(4096, 2, TableCompression::Uncompressed).expect("config");
    let artifact = ImmutableTableBuilder::new(config)
        .expect("builder")
        .build_from_rows(identity("wire-shape"), &rows)
        .expect("build artifact");
    let bytes = artifact.bytes();
    let decoded = decode_immutable_table(bytes).expect("decode artifact");

    assert!(bytes.len() > TABLE_HEADER_SIZE + TABLE_FOOTER_SIZE);
    assert_eq!(&bytes[..4], b"STTB");
    assert_eq!(read_u32_le(bytes, 4), 1);
    assert_eq!(read_u32_le(bytes, 8), 64);
    assert_eq!(read_u32_le(bytes, 12), 0);
    assert_eq!(read_u32_le(bytes, 16), config.target_data_block_size());
    assert_eq!(read_u32_le(bytes, 20), decoded.header().data_block_count());
    assert_eq!(read_u64_le(bytes, 24), decoded.header().row_count());
    assert_eq!(
        read_u64_le(bytes, 32),
        decoded.header().commit_min().as_u64()
    );
    assert_eq!(
        read_u64_le(bytes, 40),
        decoded.header().commit_max().as_u64()
    );
    assert!(bytes[48..TABLE_HEADER_SIZE].iter().all(|byte| *byte == 0));

    let footer_start = bytes.len() - TABLE_FOOTER_SIZE;
    let index_offset =
        usize::try_from(read_u64_le(bytes, footer_start)).expect("index offset fits usize");
    let index_len =
        usize::try_from(read_u32_le(bytes, footer_start + 8)).expect("index len fits usize");
    let filter_offset = read_u64_le(bytes, footer_start + 12);
    let filter_len = read_u32_le(bytes, footer_start + 20);
    let properties_offset = usize::try_from(read_u64_le(bytes, footer_start + 24))
        .expect("properties offset fits usize");
    let properties_len =
        usize::try_from(read_u32_le(bytes, footer_start + 32)).expect("properties len fits usize");

    assert_eq!(filter_offset, 0);
    assert_eq!(filter_len, 0);
    assert_eq!(&bytes[footer_start + 36..footer_start + 40], b"STTF");
    assert!(bytes[footer_start + 40..footer_start + 60]
        .iter()
        .all(|byte| *byte == 0));
    assert!(index_offset >= TABLE_HEADER_SIZE);
    assert!(index_len > 0);
    assert!(properties_len > 0);
    assert_eq!(properties_offset, index_offset + index_len);
    assert_eq!(footer_start, properties_offset + properties_len);
    assert!(!decoded.data_blocks().is_empty());
}

#[test]
fn immutable_builder_preserves_raw_row_facts_and_derives_decoded_facts() {
    let versioned_key = physical_key(1, 0x20, b"versioned\0key".to_vec());
    let rows = vec![
        put_row_for_key(versioned_key.clone(), 9, b"newer".to_vec()),
        put_row_for_key(versioned_key, 2, b"older".to_vec()),
        tombstone_row(b"delete".to_vec(), 7),
        expired_row(b"expired".to_vec(), 5),
        put_row_for_key(
            physical_key(2, 0x21, b"other-branch-and-space".to_vec()),
            11,
            Vec::new(),
        ),
        put_row_for_key(
            physical_key(3, 0x22, b"large-value".to_vec()),
            12,
            vec![0x7a; 16 * 1024],
        ),
    ];
    let table_rows = sorted_table_rows(&rows);
    let expected_rows = sorted_storage_rows(&rows);
    let artifact = builder(2, TableCompression::Uncompressed)
        .build_from_rows(identity("raw-facts"), &table_rows)
        .expect("build raw facts");
    let decoded = decode_immutable_table(artifact.bytes()).expect("decode raw facts");
    let properties = decoded.properties();

    assert_eq!(decoded.rows(), expected_rows.as_slice());
    assert_eq!(artifact.byte_count(), artifact.bytes().len() as u64);
    assert_eq!(artifact.facts().identity().as_str(), "raw-facts");
    assert_eq!(artifact.facts().row_count(), properties.row_count());
    assert_eq!(
        artifact.facts().data_block_count(),
        properties.data_block_count()
    );
    assert_eq!(
        artifact.facts().key_range().first_key(),
        properties.min_key_bytes()
    );
    assert_eq!(
        artifact.facts().key_range().last_key(),
        properties.max_key_bytes()
    );
    assert_eq!(
        artifact.facts().key_range().first_key(),
        table_rows.first().expect("first").encoded_key()
    );
    assert_eq!(
        artifact.facts().key_range().last_key(),
        table_rows.last().expect("last").encoded_key()
    );
    assert_eq!(artifact.facts().commit_range().min(), CommitVersion::new(2));
    assert_eq!(
        artifact.facts().commit_range().max(),
        CommitVersion::new(12)
    );
    assert_eq!(
        artifact.facts().commit_range().min(),
        properties.commit_min()
    );
    assert_eq!(
        artifact.facts().commit_range().max(),
        properties.commit_max()
    );
    assert!(decoded.rows().iter().any(StorageRow::is_tombstone));
    assert!(decoded
        .rows()
        .iter()
        .any(|row| row.expires_at() == Timestamp::from_micros(1)));
}

#[test]
fn immutable_builder_is_deterministic_across_rows_mutable_and_frozen_sources() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        tombstone_row(b"bravo".to_vec(), 2),
        expired_row(b"charlie".to_vec(), 3),
        put_row(b"delta".to_vec(), 4),
    ];
    let table_rows = sorted_table_rows(&rows);
    let mut table = MutableTable::new();
    for row in rows {
        table.insert_row(row).expect("insert row");
    }
    let frozen = table.clone().freeze();
    let builder = builder(2, TableCompression::Uncompressed);

    let from_rows_a = builder
        .build_from_rows(identity("deterministic"), &table_rows)
        .expect("build from rows a");
    let from_rows_b = builder
        .build_from_rows(identity("deterministic"), &table_rows)
        .expect("build from rows b");
    let from_mutable = builder
        .build_from_mutable(identity("deterministic"), &table)
        .expect("build from mutable");
    let from_frozen = builder
        .build_from_frozen(identity("deterministic"), &frozen)
        .expect("build from frozen");

    assert_eq!(from_rows_a.bytes(), from_rows_b.bytes());
    assert_eq!(from_rows_a.bytes(), from_mutable.bytes());
    assert_eq!(from_rows_a.bytes(), from_frozen.bytes());
    assert_eq!(from_rows_a.facts(), from_frozen.facts());

    let (bytes, facts) = from_rows_a.clone().into_parts();
    assert_eq!(bytes, from_rows_a.bytes());
    assert_eq!(&facts, from_rows_a.facts());
    assert_eq!(from_rows_a.into_bytes(), from_rows_b.bytes());
}

#[test]
fn immutable_builder_streaming_matches_batch_builder_with_block_bounded_buffer() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        tombstone_row(b"bravo".to_vec(), 2),
        expired_row(b"charlie".to_vec(), 3),
        put_row(b"delta".to_vec(), 4),
        put_row(b"echo".to_vec(), 5),
    ];
    let table_rows = sorted_table_rows(&rows);
    let storage_rows = sorted_storage_rows(&rows);
    let builder = builder(2, TableCompression::Uncompressed);
    let batch = builder
        .build_from_rows(identity("streaming-equivalent"), &table_rows)
        .expect("batch build");
    let encoded = encode_immutable_table(&storage_rows, 1024, 2, TableCompression::Uncompressed)
        .expect("format encode");

    let mut streaming = builder
        .begin_streaming(identity("streaming-equivalent"))
        .expect("begin streaming build");
    assert_eq!(streaming.buffered_rows(), 0);
    for row in &table_rows {
        streaming.append(row).expect("append streaming row");
        assert!(streaming.buffered_rows() <= 2);
    }
    assert_eq!(streaming.peak_buffered_rows(), 2);
    let streamed = streaming.finish().expect("finish streaming build");
    let reader = ImmutableTableReader::open_bytes(
        identity("streaming-equivalent"),
        streamed.bytes().to_vec(),
        TableReaderConfig::default(),
    )
    .expect("open streamed artifact");

    assert_eq!(streamed.bytes(), encoded.as_slice());
    assert_eq!(streamed.bytes(), batch.bytes());
    assert_eq!(streamed.facts(), batch.facts());
    assert_eq!(reader.rows(), table_rows.as_slice());
    for row in &table_rows {
        assert_eq!(reader.get_exact(row.key()), Some(row.clone()));
    }
}

#[test]
fn immutable_builder_streaming_preserves_validation_errors() {
    let builder = builder(2, TableCompression::Uncompressed);
    let empty = builder
        .begin_streaming(identity("streaming-empty"))
        .expect("begin empty streaming build");
    assert_eq!(
        empty.finish(),
        Err(TableRuntimeError::InvalidRange { field: "row_count" })
    );

    let rows = sorted_table_rows(&[put_row(b"alpha".to_vec(), 1), put_row(b"bravo".to_vec(), 2)]);
    let mut unsorted = builder
        .begin_streaming(identity("streaming-unsorted"))
        .expect("begin unsorted streaming build");
    unsorted
        .append(&rows[1])
        .expect("append first unsorted row");
    assert!(matches!(
        unsorted.append(&rows[0]),
        Err(TableRuntimeError::InvalidRowOrder { .. })
    ));

    let mut duplicate = builder
        .begin_streaming(identity("streaming-duplicate"))
        .expect("begin duplicate streaming build");
    duplicate
        .append(&rows[0])
        .expect("append first duplicate row");
    assert!(matches!(
        duplicate.append(&rows[0]),
        Err(TableRuntimeError::DuplicateInternalKey { .. })
    ));
}

#[test]
fn immutable_builder_compression_paths_decode_to_same_rows() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
        put_row(b"delta".to_vec(), 4),
    ];
    let table_rows = sorted_table_rows(&rows);
    let expected_rows = sorted_storage_rows(&rows);

    let uncompressed = builder(2, TableCompression::Uncompressed)
        .build_from_rows(identity("uncompressed"), &table_rows)
        .expect("uncompressed build");
    let zstd = builder(2, TableCompression::Zstd)
        .build_from_rows(identity("zstd"), &table_rows)
        .expect("zstd build");

    let decoded_uncompressed =
        decode_immutable_table(uncompressed.bytes()).expect("decode uncompressed");
    let decoded_zstd = decode_immutable_table(zstd.bytes()).expect("decode zstd");
    assert_eq!(decoded_uncompressed.rows(), expected_rows.as_slice());
    assert_eq!(decoded_zstd.rows(), expected_rows.as_slice());
    assert_eq!(
        decoded_uncompressed.header().data_block_count(),
        decoded_zstd.header().data_block_count()
    );
}

#[test]
fn immutable_builder_target_size_only_changes_header_fact_not_rows() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        tombstone_row(b"charlie".to_vec(), 3),
    ];
    let table_rows = sorted_table_rows(&rows);
    let expected_rows = sorted_storage_rows(&rows);
    let small_target = ImmutableTableBuilder::new(
        TableBuilderConfig::new(128, 2, TableCompression::Uncompressed).expect("small config"),
    )
    .expect("small builder")
    .build_from_rows(identity("small-target"), &table_rows)
    .expect("small target build");
    let large_target = ImmutableTableBuilder::new(
        TableBuilderConfig::new(8192, 2, TableCompression::Uncompressed).expect("large config"),
    )
    .expect("large builder")
    .build_from_rows(identity("large-target"), &table_rows)
    .expect("large target build");

    let decoded_small = decode_immutable_table(small_target.bytes()).expect("decode small");
    let decoded_large = decode_immutable_table(large_target.bytes()).expect("decode large");
    assert_ne!(small_target.bytes(), large_target.bytes());
    assert_eq!(decoded_small.header().target_data_block_size(), 128);
    assert_eq!(decoded_large.header().target_data_block_size(), 8192);
    assert_eq!(
        decoded_small.header().data_block_count(),
        decoded_large.header().data_block_count()
    );
    assert_eq!(decoded_small.rows(), expected_rows.as_slice());
    assert_eq!(decoded_large.rows(), expected_rows.as_slice());
}

#[test]
fn immutable_builder_storage_row_path_uses_same_sorted_validation() {
    let rows = vec![put_row(b"alpha".to_vec(), 1), put_row(b"bravo".to_vec(), 2)];
    let sorted = sorted_storage_rows(&rows);
    let builder = builder(2, TableCompression::Uncompressed);
    let artifact = builder
        .build_from_storage_rows(identity("storage-rows"), &sorted)
        .expect("build sorted storage rows");
    assert_eq!(
        decode_immutable_table(artifact.bytes())
            .expect("decode")
            .rows(),
        sorted.as_slice()
    );

    let unsorted = vec![sorted[1].clone(), sorted[0].clone()];
    assert!(matches!(
        builder.build_from_storage_rows(identity("storage-unsorted"), &unsorted),
        Err(TableRuntimeError::InvalidRowOrder { .. })
    ));
}

#[test]
fn immutable_builder_validation_errors_keep_display_bounded() {
    let builder = builder(2, TableCompression::Uncompressed);
    let long_a = put_row(vec![b'a'; 8 * 1024], 1);
    let long_b = put_row(vec![b'b'; 8 * 1024], 1);
    let sorted = sorted_table_rows(&[long_a, long_b]);
    let unsorted = vec![sorted[1].clone(), sorted[0].clone()];
    let order_error = builder
        .build_from_rows(identity("long-unsorted"), &unsorted)
        .expect_err("long unsorted keys should fail validation");
    let order_display = format!("{order_error}");
    assert!(matches!(
        &order_error,
        TableRuntimeError::InvalidRowOrder { .. }
    ));
    assert!(order_display.contains("...("));
    assert!(order_display.len() < 256);

    let duplicate = vec![sorted[0].clone(), sorted[0].clone()];
    let duplicate_error = builder
        .build_from_rows(identity("long-duplicate"), &duplicate)
        .expect_err("long duplicate keys should fail validation");
    let duplicate_display = format!("{duplicate_error}");
    assert!(matches!(
        &duplicate_error,
        TableRuntimeError::DuplicateInternalKey { .. }
    ));
    assert!(duplicate_display.contains("...("));
    assert!(duplicate_display.len() < 128);
}
