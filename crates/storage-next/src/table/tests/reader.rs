use crate::format::{
    decode_immutable_table_metadata, decode_table_footer_metadata, MAX_TABLE_FOOTER_SIZE,
};
use crate::format::{TableCompression, MAX_TABLE_HEADER_SIZE};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    sort_table_rows_by_key, BuiltTableArtifact, BytesTableSource, ImmutableTableBuilder,
    ImmutableTableReader, TableBlockAddress, TableBlockCache, TableBlockCacheKey,
    TableBlockCacheKind, TableBloomProbe, TableBuilderConfig, TableByteSource, TableCacheConfig,
    TableCacheTableId, TableCompactionConfig, TableCursor, TableIdentity, TableInternalKeyBytes,
    TableKeyBound, TableKeyBounds, TablePhysicalKeyBound, TablePhysicalKeyBytes, TableReaderConfig,
    TableReaderFilter, TableReaderOpenMode, TableRow, TableRuntimeConfig, TableRuntimeError,
    TableRuntimeResult,
};
use sha2::{Digest, Sha256};
use std::error::Error as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

fn branch(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn physical_key(
    branch_byte: u8,
    space: &'static str,
    space_id: u8,
    user_key: impl Into<Vec<u8>>,
) -> PhysicalKey {
    PhysicalKey::new(
        branch(branch_byte),
        space,
        StorageSpaceId::from_raw(space_id).expect("storage space id"),
        user_key,
    )
    .expect("physical key")
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

fn put_row(user_key: impl Into<Vec<u8>>, version: u64) -> StorageRow {
    put_row_for_key(
        physical_key(1, "reader", 0x20, user_key),
        version,
        version.to_le_bytes().to_vec(),
    )
}

fn expired_row(user_key: impl Into<Vec<u8>>, version: u64) -> StorageRow {
    StorageRow::put(
        physical_key(1, "reader", 0x20, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(version.saturating_add(100)),
        Timestamp::from_micros(1),
        b"expired".to_vec(),
    )
}

fn tombstone_row(user_key: impl Into<Vec<u8>>, version: u64) -> StorageRow {
    StorageRow::tombstone(
        physical_key(1, "reader", 0x20, user_key),
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

fn build_artifact(
    name: &'static str,
    rows: &[StorageRow],
    rows_per_block: usize,
    compression: TableCompression,
) -> (BuiltTableArtifact, Vec<TableRow>) {
    let table_rows = sorted_table_rows(rows);
    let artifact = builder(rows_per_block, compression)
        .build_from_rows(identity(name), &table_rows)
        .expect("build immutable table artifact");
    (artifact, table_rows)
}

fn reader_filter_for_table_bytes(
    identity_name: &'static str,
    bytes: &[u8],
    bits_per_key: usize,
) -> TableReaderFilter {
    TableReaderFilter::from_table_bytes(identity(identity_name), bytes, bits_per_key)
        .expect("reader filter")
}

fn filter_probe_for_key(filter: &TableReaderFilter, key: &PhysicalKey) -> TableBloomProbe {
    let key = TablePhysicalKeyBytes::from_physical_key(key);
    filter.probe_physical_key(&key)
}

#[cfg(feature = "perf-trace")]
fn find_absent_physical_key_with_probe(
    filter: &TableReaderFilter,
    candidates: impl IntoIterator<Item = Vec<u8>>,
    expected: TableBloomProbe,
) -> PhysicalKey {
    for candidate in candidates {
        let key = physical_key(1, "reader", 0x20, candidate);
        if filter_probe_for_key(filter, &key) == expected {
            return key;
        }
    }
    panic!("no generated absent physical key produced the requested probe result");
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TableMetadataRanges {
    index_offset: u64,
    index_len: u32,
    properties_offset: u64,
    properties_len: u32,
}

fn table_metadata_ranges(bytes: &[u8]) -> TableMetadataRanges {
    let footer_start = bytes.len() - MAX_TABLE_FOOTER_SIZE;
    let footer = decode_table_footer_metadata(&bytes[footer_start..], footer_start)
        .expect("decode table footer metadata");
    TableMetadataRanges {
        index_offset: footer.index_block_offset(),
        index_len: footer.index_block_frame_len(),
        properties_offset: footer.properties_block_offset(),
        properties_len: footer.properties_block_frame_len(),
    }
}

fn decode_table_metadata(bytes: &[u8]) -> crate::format::ImmutableTableMetadata {
    let ranges = table_metadata_ranges(bytes);
    let footer_start = bytes.len() - MAX_TABLE_FOOTER_SIZE;
    let index_start = checked_table_offset(ranges.index_offset);
    let index_end =
        index_start + usize::try_from(ranges.index_len).expect("index length fits usize");
    let properties_start = checked_table_offset(ranges.properties_offset);
    let properties_end = properties_start
        + usize::try_from(ranges.properties_len).expect("properties length fits usize");

    decode_immutable_table_metadata(
        bytes.len() as u64,
        &bytes[..MAX_TABLE_HEADER_SIZE],
        &bytes[footer_start..],
        &bytes[index_start..index_end],
        &bytes[properties_start..properties_end],
    )
    .expect("decode table metadata")
}

fn refresh_table_crc(bytes: &mut [u8]) {
    let crc_offset = bytes.len() - 4;
    let crc = crc32fast::hash(&bytes[..crc_offset]);
    bytes[crc_offset..].copy_from_slice(&crc.to_le_bytes());
}

fn set_footer_filter_block_fields(bytes: &mut [u8], filter_offset: u64, filter_len: u32) {
    let footer_start = bytes.len() - MAX_TABLE_FOOTER_SIZE;
    bytes[footer_start + 12..footer_start + 20].copy_from_slice(&filter_offset.to_le_bytes());
    bytes[footer_start + 20..footer_start + 24].copy_from_slice(&filter_len.to_le_bytes());
    refresh_table_crc(bytes);
}

fn corrupt_data_block_payload(bytes: &mut [u8], block_index: usize) {
    let metadata = decode_table_metadata(bytes);
    let entry = metadata
        .index()
        .entries()
        .get(block_index)
        .expect("data block entry");
    let payload_offset = checked_table_offset(entry.block_offset()) + 12;
    bytes[payload_offset] ^= 0xff;
}

fn checked_table_offset(offset: u64) -> usize {
    usize::try_from(offset).expect("table fixture offset fits usize")
}

fn enabled_block_cache(capacity_bytes: usize) -> Arc<TableBlockCache> {
    Arc::new(TableBlockCache::new(
        TableCacheConfig::new(true, capacity_bytes).expect("cache config"),
    ))
}

fn collect_keys(cursor: &mut impl TableCursor) -> Vec<Vec<u8>> {
    let mut keys = Vec::new();
    while let Some(row) = cursor.current() {
        keys.push(row.encoded_key().to_vec());
        cursor.advance().expect("advance cursor");
    }
    keys
}

fn expected_seek_key(rows: &[TableRow], target: &TableInternalKeyBytes) -> Option<Vec<u8>> {
    rows.iter()
        .find(|row| row.key() >= target)
        .map(|row| row.encoded_key().to_vec())
}

fn all_reader_keys(reader: &ImmutableTableReader) -> Vec<Vec<u8>> {
    let mut cursor = reader.cursor();
    cursor.seek_to_first().expect("seek first");
    collect_keys(&mut cursor)
}

fn bounded_reader_keys(reader: &ImmutableTableReader, bounds: TableKeyBounds) -> Vec<Vec<u8>> {
    let mut cursor = reader.bounded_cursor(bounds);
    cursor.seek_to_first().expect("seek bounded first");
    collect_keys(&mut cursor)
}

fn encoded_row_keys(rows: &[TableRow]) -> Vec<Vec<u8>> {
    rows.iter()
        .map(|row| row.encoded_key().to_vec())
        .collect::<Vec<_>>()
}

fn optional_table_row_key(row: Option<TableRow>) -> Option<Vec<u8>> {
    row.map(|row| row.encoded_key().to_vec())
}

fn assert_decode_format(bytes: Vec<u8>, name: &'static str) {
    let error =
        ImmutableTableReader::open_bytes(identity(name), bytes, TableReaderConfig::default())
            .expect_err("invalid table bytes should fail through decode wrapper");

    assert!(matches!(error, TableRuntimeError::DecodeFormat { .. }));
    assert!(error.source().is_some());
}

#[derive(Clone)]
struct TestSource {
    bytes: Vec<u8>,
    advertised_len: u64,
    short_read: bool,
    long_read: bool,
    fail_read: bool,
    calls: Arc<AtomicUsize>,
}

impl TestSource {
    fn exact(bytes: Vec<u8>) -> Self {
        Self {
            advertised_len: bytes.len() as u64,
            bytes,
            short_read: false,
            long_read: false,
            fail_read: false,
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn short(bytes: Vec<u8>) -> Self {
        Self {
            advertised_len: bytes.len() as u64,
            bytes,
            short_read: true,
            long_read: false,
            fail_read: false,
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn long(bytes: Vec<u8>) -> Self {
        Self {
            advertised_len: bytes.len() as u64,
            bytes,
            short_read: false,
            long_read: true,
            fail_read: false,
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn failing(bytes: Vec<u8>) -> Self {
        Self {
            advertised_len: bytes.len() as u64,
            bytes,
            short_read: false,
            long_read: false,
            fail_read: true,
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn with_advertised_len(bytes: Vec<u8>, advertised_len: u64) -> Self {
        Self {
            advertised_len,
            bytes,
            short_read: false,
            long_read: false,
            fail_read: false,
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

impl TableByteSource for TestSource {
    fn byte_count(&self) -> u64 {
        self.advertised_len
    }

    fn read_at(&self, offset: u64, len: usize) -> TableRuntimeResult<Vec<u8>> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if self.fail_read {
            return Err(TableRuntimeError::source_read(
                "injected table source failure",
            ));
        }

        let start = usize::try_from(offset).map_err(|_| TableRuntimeError::InvalidRange {
            field: "byte_offset",
        })?;
        let mut end = start
            .checked_add(len)
            .ok_or(TableRuntimeError::InvalidRange {
                field: "byte_range",
            })?;
        if start > self.bytes.len() {
            return Err(TableRuntimeError::source_read(
                "byte range exceeds source length",
            ));
        }
        end = end.min(self.bytes.len());
        if self.short_read && end > start {
            end -= 1;
        }
        let mut bytes = self.bytes[start..end].to_vec();
        if self.long_read {
            bytes.push(0);
        }
        Ok(bytes)
    }

    fn exact_content_digest(&self) -> TableRuntimeResult<Option<[u8; 32]>> {
        if self.advertised_len != self.bytes.len() as u64
            || self.short_read
            || self.long_read
            || self.fail_read
        {
            return Ok(None);
        }

        let digest = Sha256::digest(&self.bytes);
        let mut output = [0; 32];
        output.copy_from_slice(&digest);
        Ok(Some(output))
    }
}

#[test]
fn immutable_reader_opens_bytes_and_exposes_facts_and_exact_lookup() {
    let same_key = physical_key(1, "reader", 0x20, b"same\0key".to_vec());
    let rows = vec![
        put_row_for_key(same_key.clone(), 9, b"newer".to_vec()),
        put_row_for_key(same_key, 2, b"older".to_vec()),
        tombstone_row(b"delete".to_vec(), 7),
        expired_row(b"expired".to_vec(), 5),
        put_row_for_key(
            physical_key(2, "reader", 0x21, b"other-branch".to_vec()),
            11,
            Vec::new(),
        ),
        put_row_for_key(
            physical_key(3, "reader", 0x22, b"large-value".to_vec()),
            12,
            vec![0x7a; 16 * 1024],
        ),
    ];
    let table_rows = sorted_table_rows(&rows);
    let artifact = builder(2, TableCompression::Uncompressed)
        .build_from_rows(identity("reader-bytes"), &table_rows)
        .expect("build reader bytes");
    let config = TableReaderConfig::new();

    let reader = ImmutableTableReader::open_bytes(
        identity("reader-bytes"),
        artifact.bytes().to_vec(),
        config,
    )
    .expect("open reader");

    assert_eq!(reader.config(), config);
    assert_eq!(reader.facts(), artifact.facts());
    assert_eq!(
        reader.runtime_facts().open_mode(),
        TableReaderOpenMode::EagerBytes
    );
    assert!(reader.runtime_facts().metadata_loaded());
    assert!(reader.runtime_facts().index_loaded());
    assert_eq!(
        reader.runtime_facts().data_blocks_loaded(),
        artifact.facts().data_block_count()
    );
    assert_eq!(
        reader.runtime_facts().rows_materialized(),
        artifact.facts().row_count()
    );
    assert!(!reader.runtime_facts().filter_available());
    assert!(!reader.runtime_facts().cache_enabled());
    assert_eq!(reader.byte_count(), artifact.byte_count());
    assert_eq!(reader.rows(), table_rows.as_slice());

    for row in &table_rows {
        assert_eq!(reader.get_exact(row.key()), Some(row.clone()));
    }

    let missing = TableInternalKeyBytes::from_row(&put_row(b"missing".to_vec(), 99));
    assert_eq!(reader.get_exact(&missing), None);
}

#[test]
fn immutable_reader_into_materialized_preserves_eager_byte_runtime_facts() {
    let rows = sorted_table_rows(&[put_row(b"alpha".to_vec(), 1), put_row(b"bravo".to_vec(), 2)]);
    let artifact = builder(1, TableCompression::Uncompressed)
        .build_from_rows(identity("reader-materialized-bytes"), &rows)
        .expect("build reader bytes");
    let reader = ImmutableTableReader::open_bytes(
        identity("reader-materialized-bytes"),
        artifact.bytes().to_vec(),
        TableReaderConfig::default(),
    )
    .expect("open byte reader");
    let row_backing = reader.rows().as_ptr();

    let materialized = reader
        .into_materialized()
        .expect("materialize eager reader");

    assert_eq!(
        materialized.runtime_facts().open_mode(),
        TableReaderOpenMode::EagerBytes
    );
    assert_eq!(
        materialized.runtime_facts().data_blocks_loaded(),
        artifact.facts().data_block_count()
    );
    assert_eq!(
        materialized.runtime_facts().rows_materialized(),
        artifact.facts().row_count()
    );
    assert_eq!(materialized.rows(), rows.as_slice());
    assert_eq!(materialized.rows().as_ptr(), row_backing);
}

#[test]
fn immutable_reader_into_materialized_converts_lazy_source_runtime_facts() {
    let rows = sorted_table_rows(&[put_row(b"alpha".to_vec(), 1), put_row(b"bravo".to_vec(), 2)]);
    let artifact = builder(1, TableCompression::Uncompressed)
        .build_from_rows(identity("reader-materialized-source"), &rows)
        .expect("build reader source");
    let reader = ImmutableTableReader::open_source(
        identity("reader-materialized-source"),
        BytesTableSource::new(artifact.bytes().to_vec()),
        TableReaderConfig::default(),
    )
    .expect("open source reader");
    assert_eq!(
        reader.runtime_facts().open_mode(),
        TableReaderOpenMode::LazySource
    );

    let materialized = reader
        .into_materialized()
        .expect("materialize source reader");

    assert_eq!(
        materialized.runtime_facts().open_mode(),
        TableReaderOpenMode::EagerSource
    );
    assert_eq!(
        materialized.runtime_facts().data_blocks_loaded(),
        artifact.facts().data_block_count()
    );
    assert_eq!(
        materialized.runtime_facts().rows_materialized(),
        artifact.facts().row_count()
    );
    assert_eq!(materialized.rows(), rows.as_slice());
}

#[test]
fn immutable_reader_equality_remains_reflexive_when_lazy_rows_fail() {
    let rows = sorted_table_rows(&[put_row(b"alpha".to_vec(), 1), put_row(b"bravo".to_vec(), 2)]);
    let artifact = builder(1, TableCompression::Uncompressed)
        .build_from_rows(identity("reader-corrupt-lazy-reflexive"), &rows)
        .expect("build reader source");
    let mut bytes = artifact.bytes().to_vec();
    let first_data_payload_offset = MAX_TABLE_HEADER_SIZE + 12;
    bytes[first_data_payload_offset] ^= 0xff;

    let reader = ImmutableTableReader::open_source(
        identity("reader-corrupt-lazy-reflexive"),
        BytesTableSource::new(bytes),
        TableReaderConfig::default(),
    )
    .expect("lazy metadata open should not decode corrupt data block");
    assert!(matches!(
        reader.try_rows(),
        Err(TableRuntimeError::DecodeFormat { .. })
    ));
    assert_eq!(reader, reader.clone());
}

#[test]
fn immutable_reader_opens_table_source_and_maps_source_failures() {
    let rows = sorted_table_rows(&[put_row(b"alpha".to_vec(), 1), put_row(b"bravo".to_vec(), 2)]);
    let artifact = builder(1, TableCompression::Uncompressed)
        .build_from_rows(identity("reader-source"), &rows)
        .expect("build reader source");

    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let reader = ImmutableTableReader::open_source(
        identity("reader-source"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open source");
    assert_eq!(source_probe.calls(), 4);
    assert_eq!(
        reader.runtime_facts().open_mode(),
        TableReaderOpenMode::LazySource
    );
    assert!(reader.runtime_facts().metadata_loaded());
    assert!(reader.runtime_facts().index_loaded());
    assert_eq!(reader.runtime_facts().data_blocks_loaded(), 0);
    assert_eq!(reader.runtime_facts().rows_materialized(), 0);
    assert!(!reader.runtime_facts().filter_available());
    assert!(!reader.runtime_facts().cache_enabled());
    assert_eq!(reader.facts(), artifact.facts());
    assert_eq!(reader.rows(), rows.as_slice());
    assert_eq!(
        source_probe.calls(),
        usize::try_from(artifact.facts().data_block_count()).expect("block count fits") + 4
    );

    let failing = ImmutableTableReader::open_source(
        identity("reader-source-fail"),
        TestSource::failing(artifact.bytes().to_vec()),
        TableReaderConfig::default(),
    )
    .expect_err("source failure should propagate");
    assert!(matches!(failing, TableRuntimeError::SourceRead { .. }));

    let short = ImmutableTableReader::open_source(
        identity("reader-source-short"),
        TestSource::short(artifact.bytes().to_vec()),
        TableReaderConfig::default(),
    )
    .expect_err("short source read should fail");
    assert_eq!(
        short,
        TableRuntimeError::source_read("short table header read")
    );

    let long = ImmutableTableReader::open_source(
        identity("reader-source-long"),
        TestSource::long(artifact.bytes().to_vec()),
        TableReaderConfig::default(),
    )
    .expect_err("long source read should fail");
    assert_eq!(
        long,
        TableRuntimeError::source_read("long table source range read")
    );

    let short_advertised = TestSource::with_advertised_len(
        artifact.bytes().to_vec(),
        artifact.byte_count().saturating_sub(1),
    );
    let short_advertised_probe = short_advertised.clone();
    let error = ImmutableTableReader::open_source(
        identity("reader-source-advertised-short"),
        short_advertised,
        TableReaderConfig::default(),
    )
    .expect_err("advertised prefix should decode as a partial table");
    assert_eq!(short_advertised_probe.calls(), 2);
    assert!(matches!(error, TableRuntimeError::DecodeFormat { .. }));

    let long_advertised = ImmutableTableReader::open_source(
        identity("reader-source-advertised-long"),
        TestSource::with_advertised_len(
            artifact.bytes().to_vec(),
            artifact.byte_count().saturating_add(1),
        ),
        TableReaderConfig::default(),
    )
    .expect_err("advertised length beyond source should be a short read");
    assert_eq!(
        long_advertised,
        TableRuntimeError::source_read("short table footer read")
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn immutable_reader_source_open_perf_counters_prove_lazy_metadata_path() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
        put_row(b"delta".to_vec(), 4),
        put_row(b"echo".to_vec(), 5),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-lazy-open-lazy-proof",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    assert_eq!(artifact.facts().data_block_count(), 3);
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let zero = crate::observability::perf_trace::snapshot();
    assert_eq!(zero.table_reader_opens(), 0);
    assert_eq!(zero.table_data_block_reads(), 0);
    assert_eq!(zero.table_data_block_decodes(), 0);
    assert_eq!(zero.table_rows_decoded(), 0);

    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-open-lazy-proof"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy source reader");

    assert_eq!(
        reader.runtime_facts().open_mode(),
        TableReaderOpenMode::LazySource
    );
    assert_eq!(reader.runtime_facts().data_blocks_loaded(), 0);
    assert_eq!(reader.runtime_facts().rows_materialized(), 0);
    assert_eq!(source_probe.calls(), 4);

    let open_perf = crate::observability::perf_trace::snapshot();
    assert_eq!(open_perf.table_reader_opens(), 1);
    assert_eq!(
        open_perf.table_metadata_read_bytes(),
        u64::try_from(MAX_TABLE_HEADER_SIZE + MAX_TABLE_FOOTER_SIZE).expect("metadata bytes fit")
    );
    assert!(open_perf.table_index_read_bytes() > 0);
    assert!(open_perf.table_properties_read_bytes() > 0);
    assert_eq!(open_perf.table_data_block_reads(), 0);
    assert_eq!(open_perf.table_data_block_read_bytes(), 0);
    assert_eq!(open_perf.table_data_block_decodes(), 0);
    assert_eq!(open_perf.table_rows_decoded(), 0);

    assert_eq!(reader.rows(), table_rows.as_slice());
    assert_eq!(
        source_probe.calls(),
        usize::try_from(artifact.facts().data_block_count()).expect("block count fits") + 4
    );

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.table_reader_opens(), 1);
    assert_eq!(
        perf.table_metadata_read_bytes(),
        open_perf.table_metadata_read_bytes()
    );
    assert_eq!(
        perf.table_index_read_bytes(),
        open_perf.table_index_read_bytes()
    );
    assert_eq!(
        perf.table_properties_read_bytes(),
        open_perf.table_properties_read_bytes()
    );
    assert_eq!(
        perf.table_data_block_reads(),
        u64::from(artifact.facts().data_block_count())
    );
    assert!(perf.table_data_block_read_bytes() > 0);
    assert_eq!(
        perf.table_data_block_decodes(),
        u64::from(artifact.facts().data_block_count())
    );
    assert_eq!(perf.table_rows_decoded(), artifact.facts().row_count());
}

#[cfg(feature = "perf-trace")]
#[test]
fn immutable_reader_lazy_source_one_block_open_reads_metadata_only() {
    let rows = vec![put_row(b"alpha".to_vec(), 1), put_row(b"bravo".to_vec(), 2)];
    let (artifact, _) = build_artifact(
        "reader-lazy-open-one-block",
        &rows,
        16,
        TableCompression::Uncompressed,
    );
    assert_eq!(artifact.facts().data_block_count(), 1);
    let eager = ImmutableTableReader::open_bytes(
        identity("reader-lazy-open-one-block"),
        artifact.bytes().to_vec(),
        TableReaderConfig::default(),
    )
    .expect("open eager byte reader");
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-open-one-block"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy one-block source reader");

    assert_eq!(reader.facts(), eager.facts());
    assert_eq!(source_probe.calls(), 4);
    assert_lazy_open_runtime_facts(&reader);
    assert_lazy_open_perf_snapshot(&crate::observability::perf_trace::snapshot(), true);
}

#[cfg(feature = "perf-trace")]
#[test]
fn immutable_reader_lazy_source_multi_block_open_reads_metadata_only() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
        put_row(b"delta".to_vec(), 4),
    ];
    let (artifact, _) = build_artifact(
        "reader-lazy-open-multi-block",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    assert!(artifact.facts().data_block_count() > 1);
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-open-multi-block"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy multi-block source reader");

    assert_eq!(reader.facts(), artifact.facts());
    assert_eq!(source_probe.calls(), 4);
    assert_lazy_open_runtime_facts(&reader);
    assert_lazy_open_perf_snapshot(&crate::observability::perf_trace::snapshot(), true);
}

#[cfg(feature = "perf-trace")]
#[test]
fn immutable_reader_lazy_source_corrupt_header_and_footer_fail_before_index_or_data_reads() {
    let (artifact, _) = build_artifact(
        "reader-lazy-open-header-footer-corrupt",
        &[put_row(b"alpha".to_vec(), 1), put_row(b"bravo".to_vec(), 2)],
        1,
        TableCompression::Uncompressed,
    );

    let mut corrupt_header = artifact.bytes().to_vec();
    corrupt_header[0] ^= 0xff;
    assert_lazy_decode_error_before_data_reads(
        "header-corrupt",
        corrupt_header,
        LazyOpenExpectation {
            source_calls: 2,
            metadata_frames_read: false,
        },
    );

    let mut corrupt_footer = artifact.bytes().to_vec();
    let footer_magic = corrupt_footer.len() - MAX_TABLE_FOOTER_SIZE + 36;
    corrupt_footer[footer_magic] ^= 0xff;
    assert_lazy_decode_error_before_data_reads(
        "footer-corrupt",
        corrupt_footer,
        LazyOpenExpectation {
            source_calls: 2,
            metadata_frames_read: false,
        },
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn immutable_reader_lazy_source_corrupt_index_and_properties_fail_before_data_reads() {
    let (artifact, _) = build_artifact(
        "reader-lazy-open-index-properties-corrupt",
        &[put_row(b"alpha".to_vec(), 1), put_row(b"bravo".to_vec(), 2)],
        1,
        TableCompression::Uncompressed,
    );
    let ranges = table_metadata_ranges(artifact.bytes());

    let mut corrupt_index = artifact.bytes().to_vec();
    corrupt_index[checked_table_offset(ranges.index_offset)] ^= 0xff;
    assert_lazy_decode_error_before_data_reads(
        "index-corrupt",
        corrupt_index,
        LazyOpenExpectation {
            source_calls: 4,
            metadata_frames_read: true,
        },
    );

    let mut corrupt_properties = artifact.bytes().to_vec();
    corrupt_properties[checked_table_offset(ranges.properties_offset)] ^= 0xff;
    assert_lazy_decode_error_before_data_reads(
        "properties-corrupt",
        corrupt_properties,
        LazyOpenExpectation {
            source_calls: 4,
            metadata_frames_read: true,
        },
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn immutable_reader_lazy_source_index_fact_drift_fails_before_data_reads() {
    let (artifact, _) = build_artifact(
        "reader-lazy-open-index-fact-drift",
        &[
            put_row(b"alpha".to_vec(), 1),
            put_row(b"bravo".to_vec(), 2),
            put_row(b"charlie".to_vec(), 3),
        ],
        1,
        TableCompression::Uncompressed,
    );
    let mut drifted = artifact.bytes().to_vec();
    increment_first_index_entry_row_count(&mut drifted);

    assert_lazy_decode_error_before_data_reads(
        "index-row-count-drift",
        drifted,
        LazyOpenExpectation {
            source_calls: 4,
            metadata_frames_read: true,
        },
    );
}

#[test]
fn immutable_reader_lazy_block_cache_exact_lookup_hits_cache_after_cold_block_read() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-lazy-block-cache-cache-hit",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let cache = enabled_block_cache(4096);
    #[cfg(feature = "perf-trace")]
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-block-cache-cache-hit"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy reader")
    .with_block_cache(Arc::clone(&cache))
    .expect("attach block cache");
    assert!(reader.runtime_facts().cache_enabled());
    assert_eq!(source_probe.calls(), 4);

    let first = reader
        .try_get_exact(table_rows[1].key())
        .expect("cold exact lookup")
        .expect("row present");
    assert_eq!(first, table_rows[1]);
    assert_eq!(source_probe.calls(), 5);
    let cold_stats = cache.stats();
    assert_eq!(cold_stats.misses(), 1);
    assert_eq!(cold_stats.inserts(), 1);
    assert_eq!(cold_stats.hits(), 0);
    assert_eq!(cold_stats.entries(), 1);

    let second = reader
        .try_get_exact(table_rows[1].key())
        .expect("warm exact lookup")
        .expect("row present");
    assert_eq!(second, table_rows[1]);
    assert_eq!(source_probe.calls(), 5);
    let warm_stats = cache.stats();
    assert_eq!(warm_stats.misses(), 1);
    assert_eq!(warm_stats.inserts(), 1);
    assert_eq!(warm_stats.hits(), 1);

    #[cfg(feature = "perf-trace")]
    {
        let perf = crate::observability::perf_trace::snapshot();
        assert_eq!(perf.table_data_block_reads(), 1);
        assert_eq!(perf.table_cache_misses(), 1);
        assert_eq!(perf.table_cache_inserts(), 1);
        assert_eq!(perf.table_cache_hits(), 1);
        assert_eq!(perf.table_data_block_decodes(), 2);
        assert_eq!(perf.table_rows_decoded(), 2);
    }
}

#[test]
fn immutable_reader_lazy_block_cache_exact_lookup_reads_distinct_blocks_only() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-lazy-block-cache-distinct-blocks",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let cache = enabled_block_cache(4096);
    #[cfg(feature = "perf-trace")]
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-block-cache-distinct-blocks"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy reader")
    .with_block_cache(Arc::clone(&cache))
    .expect("attach block cache");

    assert_eq!(
        reader
            .try_get_exact(table_rows[0].key())
            .expect("first lookup"),
        Some(table_rows[0].clone())
    );
    assert_eq!(
        reader
            .try_get_exact(table_rows[2].key())
            .expect("third lookup"),
        Some(table_rows[2].clone())
    );
    assert_eq!(source_probe.calls(), 6);
    let stats = cache.stats();
    assert_eq!(stats.misses(), 2);
    assert_eq!(stats.inserts(), 2);
    assert_eq!(stats.hits(), 0);
    assert_eq!(stats.entries(), 2);
    #[cfg(feature = "perf-trace")]
    {
        let perf = crate::observability::perf_trace::snapshot();
        assert_eq!(perf.table_data_block_reads(), 2);
        assert_eq!(perf.table_data_block_decodes(), 2);
        assert_eq!(perf.table_rows_decoded(), 2);
    }
}

#[test]
fn immutable_reader_lazy_block_cache_exact_lookup_uses_already_materialized_rows() {
    let rows = vec![put_row(b"alpha".to_vec(), 1), put_row(b"bravo".to_vec(), 2)];
    let (artifact, table_rows) = build_artifact(
        "reader-lazy-block-cache-materialized-exact",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let cache = enabled_block_cache(4096);
    #[cfg(feature = "perf-trace")]
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-block-cache-materialized-exact"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy reader")
    .with_block_cache(Arc::clone(&cache))
    .expect("attach block cache");

    assert_eq!(reader.rows(), table_rows.as_slice());
    assert_eq!(source_probe.calls(), 6);
    #[cfg(feature = "perf-trace")]
    {
        let perf_after_rows = crate::observability::perf_trace::snapshot();
        assert_eq!(perf_after_rows.table_data_block_reads(), 2);
        assert_eq!(perf_after_rows.table_cache_inserts(), 2);
    }

    assert_eq!(
        reader
            .try_get_exact(table_rows[1].key())
            .expect("exact lookup after materialization"),
        Some(table_rows[1].clone())
    );
    assert_eq!(source_probe.calls(), 6);
    #[cfg(feature = "perf-trace")]
    {
        let perf_after_lookup = crate::observability::perf_trace::snapshot();
        assert_eq!(perf_after_lookup.table_data_block_reads(), 2);
        assert_eq!(perf_after_lookup.table_cache_hits(), 0);
        assert_eq!(perf_after_lookup.table_data_block_decodes(), 2);
    }
}

#[test]
fn immutable_reader_disabled_block_cache_preserves_results_without_storing() {
    let rows = vec![put_row(b"alpha".to_vec(), 1), put_row(b"bravo".to_vec(), 2)];
    let (artifact, table_rows) = build_artifact(
        "reader-lazy-block-cache-disabled-cache",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let cache = Arc::new(TableBlockCache::disabled());
    #[cfg(feature = "perf-trace")]
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-block-cache-disabled-cache"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy reader")
    .with_block_cache(Arc::clone(&cache))
    .expect("attach disabled cache");
    assert!(!reader.runtime_facts().cache_enabled());

    assert_eq!(
        reader
            .try_get_exact(table_rows[0].key())
            .expect("first lookup"),
        Some(table_rows[0].clone())
    );
    assert_eq!(
        reader
            .try_get_exact(table_rows[0].key())
            .expect("second lookup"),
        Some(table_rows[0].clone())
    );
    assert_eq!(source_probe.calls(), 6);
    let stats = cache.stats();
    assert_eq!(stats.misses(), 2);
    assert_eq!(stats.skipped_disabled(), 2);
    assert_eq!(stats.entries(), 0);
    #[cfg(feature = "perf-trace")]
    {
        let perf = crate::observability::perf_trace::snapshot();
        assert_eq!(perf.table_data_block_reads(), 2);
        assert_eq!(perf.table_cache_misses(), 2);
        assert_eq!(perf.table_cache_skipped_inserts(), 2);
    }
}

#[test]
fn immutable_reader_lazy_block_cache_oversized_block_is_not_cached_but_reads_correctly() {
    let rows = vec![put_row(b"alpha".to_vec(), 1), put_row(b"bravo".to_vec(), 2)];
    let (artifact, table_rows) = build_artifact(
        "reader-lazy-block-cache-oversized-cache",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let cache = enabled_block_cache(1);
    #[cfg(feature = "perf-trace")]
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-block-cache-oversized-cache"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy reader")
    .with_block_cache(Arc::clone(&cache))
    .expect("attach small cache");

    assert_eq!(
        reader
            .try_get_exact(table_rows[0].key())
            .expect("first lookup"),
        Some(table_rows[0].clone())
    );
    assert_eq!(
        reader
            .try_get_exact(table_rows[0].key())
            .expect("second lookup"),
        Some(table_rows[0].clone())
    );
    assert_eq!(source_probe.calls(), 6);
    let stats = cache.stats();
    assert_eq!(stats.misses(), 2);
    assert_eq!(stats.skipped_oversized(), 2);
    assert_eq!(stats.entries(), 0);
}

#[test]
fn immutable_reader_block_cache_is_scoped_by_table_identity() {
    let rows = vec![put_row(b"alpha".to_vec(), 1)];
    let (artifact, table_rows) = build_artifact(
        "reader-lazy-block-cache-cache-table-a",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let left_source = TestSource::exact(artifact.bytes().to_vec());
    let left_probe = left_source.clone();
    let right_source = TestSource::exact(artifact.bytes().to_vec());
    let right_probe = right_source.clone();
    let cache = enabled_block_cache(4096);
    #[cfg(feature = "perf-trace")]
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let left = ImmutableTableReader::open_source(
        identity("reader-lazy-block-cache-cache-table-a"),
        left_source,
        TableReaderConfig::default(),
    )
    .expect("open left")
    .with_block_cache(Arc::clone(&cache))
    .expect("attach left cache");
    let right = ImmutableTableReader::open_source(
        identity("reader-lazy-block-cache-cache-table-b"),
        right_source,
        TableReaderConfig::default(),
    )
    .expect("open right")
    .with_block_cache(Arc::clone(&cache))
    .expect("attach right cache");

    assert_eq!(
        left.try_get_exact(table_rows[0].key())
            .expect("left lookup"),
        Some(table_rows[0].clone())
    );
    assert_eq!(
        right
            .try_get_exact(table_rows[0].key())
            .expect("right lookup"),
        Some(table_rows[0].clone())
    );
    assert_eq!(left_probe.calls(), 5);
    assert_eq!(right_probe.calls(), 5);
    let stats = cache.stats();
    assert_eq!(stats.misses(), 2);
    assert_eq!(stats.hits(), 0);
    assert_eq!(stats.inserts(), 2);
    assert_eq!(stats.entries(), 2);
}

#[test]
fn immutable_reader_table_cache_invalidation_forces_source_read() {
    let rows = vec![put_row(b"alpha".to_vec(), 1)];
    let (artifact, table_rows) = build_artifact(
        "reader-lazy-block-cache-invalidate",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let cache = enabled_block_cache(4096);
    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-block-cache-invalidate"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open reader")
    .with_block_cache(Arc::clone(&cache))
    .expect("attach cache");

    assert_eq!(
        reader.try_get_exact(table_rows[0].key()).expect("lookup"),
        Some(table_rows[0].clone())
    );
    assert_eq!(source_probe.calls(), 5);
    let table = TableCacheTableId::new(b"reader-lazy-block-cache-invalidate".as_slice())
        .expect("cache table id");
    assert_eq!(cache.remove_table(&table), 1);
    assert_eq!(
        reader
            .try_get_exact(table_rows[0].key())
            .expect("lookup after invalidation"),
        Some(table_rows[0].clone())
    );
    assert_eq!(source_probe.calls(), 6);
    let stats = cache.stats();
    assert_eq!(stats.table_invalidations(), 1);
    assert_eq!(stats.misses(), 2);
    assert_eq!(stats.hits(), 0);
    assert_eq!(stats.inserts(), 2);
}

#[test]
fn immutable_reader_cached_block_is_validated_before_rows_are_yielded() {
    let rows = vec![put_row(b"alpha".to_vec(), 1)];
    let (artifact, table_rows) = build_artifact(
        "reader-lazy-block-cache-corrupt-cache-hit",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let ranges = table_metadata_ranges(artifact.bytes());
    let data_offset = MAX_TABLE_HEADER_SIZE as u64;
    let data_len = u32::try_from(
        ranges
            .index_offset
            .checked_sub(data_offset)
            .expect("first data block precedes index"),
    )
    .expect("first data block length fits u32");
    let mut cached_block =
        artifact.bytes()[MAX_TABLE_HEADER_SIZE..checked_table_offset(ranges.index_offset)].to_vec();
    cached_block[12] ^= 0xff;

    let cache = enabled_block_cache(4096);
    let cache_key = TableBlockCacheKey::new(
        TableCacheTableId::new(b"reader-lazy-block-cache-corrupt-cache-hit".as_slice())
            .expect("cache table id"),
        TableBlockAddress::new(TableBlockCacheKind::Data, data_offset, data_len, Some(0))
            .expect("data block cache address"),
    );
    cache
        .insert(cache_key, Arc::<[u8]>::from(cached_block))
        .expect("seed corrupt cache block");

    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    #[cfg(feature = "perf-trace")]
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-block-cache-corrupt-cache-hit"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open reader")
    .with_block_cache(Arc::clone(&cache))
    .expect("attach cache");

    assert_eq!(source_probe.calls(), 4);
    assert!(matches!(
        reader.try_get_exact(table_rows[0].key()),
        Err(TableRuntimeError::DecodeFormat { .. })
    ));
    assert_eq!(source_probe.calls(), 4);
    let stats = cache.stats();
    assert_eq!(stats.hits(), 1);
    assert_eq!(stats.misses(), 0);
    assert_eq!(stats.entries(), 1);
    #[cfg(feature = "perf-trace")]
    {
        let perf = crate::observability::perf_trace::snapshot();
        assert_eq!(perf.table_cache_hits(), 1);
        assert_eq!(perf.table_cache_misses(), 0);
        assert_eq!(perf.table_cache_inserts(), 0);
        assert_eq!(perf.table_data_block_reads(), 0);
        assert_eq!(perf.table_data_block_decodes(), 0);
    }
}

#[test]
fn immutable_reader_corrupt_block_is_not_inserted_after_decode_failure() {
    let rows = vec![put_row(b"alpha".to_vec(), 1)];
    let (artifact, table_rows) = build_artifact(
        "reader-lazy-block-cache-corrupt-uncached",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let mut bytes = artifact.bytes().to_vec();
    let first_data_payload_offset = MAX_TABLE_HEADER_SIZE + 12;
    bytes[first_data_payload_offset] ^= 0xff;
    let source = TestSource::exact(bytes);
    let cache = enabled_block_cache(4096);
    #[cfg(feature = "perf-trace")]
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-block-cache-corrupt-uncached"),
        source,
        TableReaderConfig::default(),
    )
    .expect("lazy metadata open should not decode corrupt data block")
    .with_block_cache(Arc::clone(&cache))
    .expect("attach cache");

    assert!(matches!(
        reader.try_get_exact(table_rows[0].key()),
        Err(TableRuntimeError::DecodeFormat { .. })
    ));
    let stats = cache.stats();
    assert_eq!(stats.misses(), 1);
    assert_eq!(stats.inserts(), 0);
    assert_eq!(stats.entries(), 0);
    #[cfg(feature = "perf-trace")]
    {
        let perf = crate::observability::perf_trace::snapshot();
        assert_eq!(perf.table_cache_misses(), 1);
        assert_eq!(perf.table_cache_inserts(), 0);
        assert_eq!(perf.table_data_block_reads(), 1);
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn immutable_reader_indexed_point_lookup_matches_eager_for_required_cases() {
    let multi_key = physical_key(1, "reader", 0x20, b"multi".to_vec());
    let high_key = physical_key(1, "reader", 0x20, vec![0x00, b'h', 0xff, b'z']);
    let rows = vec![
        put_row_for_key(high_key.clone(), 4, b"high".to_vec()),
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        tombstone_row(b"deleted".to_vec(), 6),
        expired_row(b"expired".to_vec(), 7),
        put_row_for_key(multi_key.clone(), 9, b"newer".to_vec()),
        put_row_for_key(multi_key.clone(), 5, b"middle".to_vec()),
        put_row_for_key(multi_key.clone(), 1, b"older".to_vec()),
        put_row(b"zulu".to_vec(), 10),
        put_row_for_key(
            physical_key(2, "reader", 0x20, b"other-branch".to_vec()),
            11,
            b"last".to_vec(),
        ),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-indexed-point-point-parity",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    let bytes = artifact.bytes().to_vec();
    let eager = ImmutableTableReader::open_bytes(
        identity("reader-indexed-point-point-parity"),
        bytes.clone(),
        TableReaderConfig::default(),
    )
    .expect("open eager point reader");
    let bytes_lazy = ImmutableTableReader::open_source(
        identity("reader-indexed-point-point-parity"),
        BytesTableSource::new(bytes.clone()),
        TableReaderConfig::default(),
    )
    .expect("open bytes-backed lazy point reader");
    let object_lazy = ImmutableTableReader::open_source(
        identity("reader-indexed-point-point-parity"),
        TestSource::exact(bytes),
        TableReaderConfig::default(),
    )
    .expect("open object-backed lazy point reader");

    let cases = vec![
        (
            "before-table-range",
            physical_key(0, "reader", 0x20, b"before".to_vec()),
            None,
            None,
        ),
        (
            "after-table-range",
            physical_key(9, "reader", 0x20, b"after".to_vec()),
            None,
            None,
        ),
        (
            "absent-inside-candidate-block",
            physical_key(1, "reader", 0x20, b"charlie".to_vec()),
            None,
            None,
        ),
        (
            "present-once",
            physical_key(1, "reader", 0x20, b"bravo".to_vec()),
            None,
            None,
        ),
        ("present-multiple-versions", multi_key.clone(), None, None),
        (
            "version-bound",
            multi_key.clone(),
            Some(CommitVersion::new(5)),
            None,
        ),
        (
            "timestamp-bound",
            multi_key.clone(),
            None,
            Some(Timestamp::from_micros(106)),
        ),
        (
            "tombstone-row",
            physical_key(1, "reader", 0x20, b"deleted".to_vec()),
            None,
            None,
        ),
        (
            "expired-raw-row",
            physical_key(1, "reader", 0x20, b"expired".to_vec()),
            None,
            None,
        ),
        (
            "first-key-in-table",
            table_rows
                .first()
                .expect("first row")
                .physical_key()
                .clone(),
            None,
            None,
        ),
        (
            "last-key-in-table",
            table_rows.last().expect("last row").physical_key().clone(),
            None,
            None,
        ),
        (
            "first-key-in-non-first-block",
            table_rows[2].physical_key().clone(),
            None,
            None,
        ),
        ("high-bit-and-embedded-zero-user-key", high_key, None, None),
    ];

    for (case, key, max_version, max_timestamp) in cases {
        let (expected, _) = eager.seek_physical_key(&key, max_version, max_timestamp);
        let expected_key = optional_table_row_key(expected);
        for (reader_name, reader) in [
            ("bytes-backed", &bytes_lazy),
            ("object-backed", &object_lazy),
        ] {
            let (actual, visited) = reader.seek_physical_key(&key, max_version, max_timestamp);
            assert_eq!(
                optional_table_row_key(actual),
                expected_key,
                "{reader_name} lazy point case {case} diverged from eager reader"
            );
            if expected_key.is_some() {
                assert!(
                    visited > 0,
                    "{reader_name} lazy point case {case} found a row without visiting it"
                );
            }
        }
    }
}

#[test]
fn immutable_reader_indexed_point_between_block_absent_matches_eager_without_materializing() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"charlie".to_vec(), 2),
        put_row(b"echo".to_vec(), 3),
    ];
    let (artifact, _) = build_artifact(
        "reader-indexed-point-between-blocks",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let bytes = artifact.bytes().to_vec();
    let eager = ImmutableTableReader::open_bytes(
        identity("reader-indexed-point-between-blocks"),
        bytes.clone(),
        TableReaderConfig::default(),
    )
    .expect("open eager reader");
    let source = TestSource::exact(bytes);
    let source_probe = source.clone();
    let lazy = ImmutableTableReader::open_source(
        identity("reader-indexed-point-between-blocks"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy reader");
    assert_eq!(source_probe.calls(), 4);

    let missing = physical_key(1, "reader", 0x20, b"bravo".to_vec());
    let (expected, _) = eager.seek_physical_key(&missing, None, None);
    let (actual, visited) = lazy
        .try_seek_physical_key(&missing, None, None)
        .expect("lazy point lookup");

    assert_eq!(
        optional_table_row_key(actual),
        optional_table_row_key(expected)
    );
    assert_eq!(visited, 0);
    assert_eq!(source_probe.calls(), 4);
}

#[cfg(feature = "perf-trace")]
#[test]
fn immutable_reader_indexed_point_out_of_range_miss_reads_zero_data_blocks() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
    ];
    let (artifact, _) = build_artifact(
        "reader-indexed-point-out-of-range",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let reader = ImmutableTableReader::open_source(
        identity("reader-indexed-point-out-of-range"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy reader");
    assert_eq!(source_probe.calls(), 4);

    let _capture = crate::observability::perf_trace::begin_test_capture();
    for key in [
        physical_key(0, "reader", 0x20, b"before".to_vec()),
        physical_key(9, "reader", 0x20, b"after".to_vec()),
    ] {
        let (row, visited) = reader.seek_physical_key(&key, None, None);
        assert!(row.is_none());
        assert_eq!(visited, 0);
    }

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(source_probe.calls(), 4);
    assert_eq!(perf.table_seeks(), 2);
    assert_eq!(perf.table_data_block_reads(), 0);
    assert_eq!(perf.table_data_block_decodes(), 0);
    assert_eq!(perf.table_rows_decoded(), 0);
    assert_eq!(perf.table_point_rows_visited(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn immutable_reader_indexed_point_present_hit_reads_one_block_then_uses_cache() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-indexed-point-present-cache",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let cache = enabled_block_cache(4096);
    let reader = ImmutableTableReader::open_source(
        identity("reader-indexed-point-present-cache"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy reader")
    .with_block_cache(Arc::clone(&cache))
    .expect("attach cache");
    let target = table_rows[1].physical_key().clone();

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let (cold, cold_visited) = reader.seek_physical_key(&target, None, None);
    assert_eq!(cold, Some(table_rows[1].clone()));
    assert_eq!(cold_visited, 1);
    assert_eq!(source_probe.calls(), 5);
    let (warm, warm_visited) = reader.seek_physical_key(&target, None, None);
    assert_eq!(warm, Some(table_rows[1].clone()));
    assert_eq!(warm_visited, 1);
    assert_eq!(source_probe.calls(), 5);

    let stats = cache.stats();
    assert_eq!(stats.misses(), 1);
    assert_eq!(stats.inserts(), 1);
    assert_eq!(stats.hits(), 1);
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.table_seeks(), 2);
    assert_eq!(perf.table_data_block_reads(), 1);
    assert_eq!(perf.table_cache_misses(), 1);
    assert_eq!(perf.table_cache_inserts(), 1);
    assert_eq!(perf.table_cache_hits(), 1);
    assert_eq!(perf.table_data_block_decodes(), 2);
    assert_eq!(perf.table_rows_decoded(), 2);
    assert_eq!(perf.table_point_rows_visited(), 2);
}

#[cfg(feature = "perf-trace")]
#[test]
fn immutable_reader_indexed_point_absent_inside_candidate_block_reads_one_block_only() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"charlie".to_vec(), 2),
        put_row(b"delta".to_vec(), 3),
        put_row(b"echo".to_vec(), 4),
    ];
    let (artifact, _) = build_artifact(
        "reader-indexed-point-absent-candidate",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let reader = ImmutableTableReader::open_source(
        identity("reader-indexed-point-absent-candidate"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy reader");
    assert_eq!(source_probe.calls(), 4);
    let missing = physical_key(1, "reader", 0x20, b"bravo".to_vec());

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let (row, visited) = reader.seek_physical_key(&missing, None, None);

    assert!(row.is_none());
    assert_eq!(visited, 1);
    assert_eq!(source_probe.calls(), 5);
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.table_data_block_reads(), 1);
    assert_eq!(perf.table_data_block_decodes(), 1);
    assert_eq!(perf.table_rows_decoded(), 2);
    assert_eq!(perf.table_point_rows_visited(), 1);
}

#[test]
fn immutable_reader_supplied_filter_is_conservative_for_physical_keys() {
    let rows = (0..128)
        .map(|index| {
            put_row(
                format!("key-{index:04}").into_bytes(),
                u64::try_from(index + 1).expect("version fits u64"),
            )
        })
        .collect::<Vec<_>>();
    let (artifact, table_rows) = build_artifact(
        "reader-filter-conservative",
        &rows,
        8,
        TableCompression::Uncompressed,
    );
    let filter = reader_filter_for_table_bytes("reader-filter-conservative", artifact.bytes(), 10);

    assert_eq!(
        TableReaderFilter::unavailable().probe_physical_key(
            &TablePhysicalKeyBytes::from_physical_key(table_rows[0].physical_key())
        ),
        TableBloomProbe::Unavailable
    );
    for row in &table_rows {
        assert_eq!(
            filter_probe_for_key(&filter, row.physical_key()),
            TableBloomProbe::MaybePresent
        );
    }

    let absent = physical_key(1, "reader", 0x20, b"key-0128".to_vec());
    assert!(matches!(
        filter_probe_for_key(&filter, &absent),
        TableBloomProbe::DefinitelyAbsent | TableBloomProbe::MaybePresent
    ));
}

#[test]
fn immutable_reader_rejects_supplied_filter_when_table_content_drift() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"charlie".to_vec(), 2),
        put_row(b"delta".to_vec(), 3),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-filter-facts",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    assert!(matches!(
        TableReaderFilter::from_table_bytes(
            identity("reader-filter-facts"),
            &artifact.bytes()[..artifact.bytes().len() - 1],
            10
        ),
        Err(TableRuntimeError::DecodeFormat { .. })
    ));

    let same_facts_rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bzzzzzz".to_vec(), 2),
        put_row(b"delta".to_vec(), 3),
    ];
    let (same_facts_artifact, same_facts_table_rows) = build_artifact(
        "reader-filter-facts",
        &same_facts_rows,
        2,
        TableCompression::Uncompressed,
    );
    assert_ne!(same_facts_table_rows, table_rows);
    assert_eq!(same_facts_artifact.facts(), artifact.facts());
    let mismatched_filter =
        reader_filter_for_table_bytes("reader-filter-facts", same_facts_artifact.bytes(), 10);
    let reader = ImmutableTableReader::open_source(
        identity("reader-filter-facts"),
        BytesTableSource::new(artifact.bytes().to_vec()),
        TableReaderConfig::default(),
    )
    .expect("open lazy reader");
    assert!(matches!(
        reader.with_table_filter(mismatched_filter),
        Err(TableRuntimeError::InvalidConfig {
            field: "table_filter",
            reason: "must match table bytes"
        })
    ));
}

#[test]
fn immutable_reader_rejects_available_filter_without_exact_table_proof() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"charlie".to_vec(), 2),
    ];
    let (artifact, _) = build_artifact(
        "reader-filter-unproven",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    let filter = reader_filter_for_table_bytes("reader-filter-unproven", artifact.bytes(), 10);
    let reader = ImmutableTableReader::open_source(
        identity("reader-filter-unproven"),
        BytesTableSource::new(artifact.bytes().to_vec()),
        TableReaderConfig::default(),
    )
    .expect("open lazy reader");

    assert!(matches!(
        reader.with_table_filter(filter),
        Err(TableRuntimeError::InvalidConfig {
            field: "table_filter",
            reason: "must match table bytes"
        })
    ));
}

#[test]
fn immutable_reader_rejects_deferred_durable_filter_block_fields() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"charlie".to_vec(), 2),
    ];
    let (artifact, _) = build_artifact(
        "reader-filter-deferred-durable",
        &rows,
        2,
        TableCompression::Uncompressed,
    );

    for (filter_offset, filter_len) in [(1u64, 0u32), (0, 1), (1, 1)] {
        let mut bytes = artifact.bytes().to_vec();
        set_footer_filter_block_fields(&mut bytes, filter_offset, filter_len);

        assert!(matches!(
            ImmutableTableReader::open_bytes(
                identity("reader-filter-deferred-durable"),
                bytes.clone(),
                TableReaderConfig::default()
            ),
            Err(TableRuntimeError::DecodeFormat { .. })
        ));
        assert!(matches!(
            ImmutableTableReader::open_source(
                identity("reader-filter-deferred-durable"),
                BytesTableSource::new(bytes),
                TableReaderConfig::default()
            ),
            Err(TableRuntimeError::DecodeFormat { .. })
        ));
    }
}

#[test]
fn immutable_reader_unavailable_filter_keeps_point_lookup_on_standard_path() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"charlie".to_vec(), 2),
    ];
    let (artifact, _) = build_artifact(
        "reader-filter-unavailable",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    let reader = ImmutableTableReader::open_source(
        identity("reader-filter-unavailable"),
        BytesTableSource::new(artifact.bytes().to_vec()),
        TableReaderConfig::default(),
    )
    .expect("open lazy reader")
    .with_table_filter(TableReaderFilter::unavailable())
    .expect("attach unavailable filter");
    assert!(!reader.runtime_facts().filter_available());

    let missing = physical_key(1, "reader", 0x20, b"bravo".to_vec());
    let (row, visited) = reader.seek_physical_key(&missing, None, None);
    assert!(row.is_none());
    assert_eq!(visited, 1);
}

#[cfg(feature = "perf-trace")]
#[test]
fn immutable_reader_negative_filter_skips_candidate_data_block_for_point_miss() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"charlie".to_vec(), 2),
        put_row(b"delta".to_vec(), 3),
        put_row(b"echo".to_vec(), 4),
    ];
    let (artifact, _) = build_artifact(
        "reader-filter-negative-point",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    let filter =
        reader_filter_for_table_bytes("reader-filter-negative-point", artifact.bytes(), 12);
    let missing = find_absent_physical_key_with_probe(
        &filter,
        (0..4096).map(|index| format!("bravo-{index:04}").into_bytes()),
        TableBloomProbe::DefinitelyAbsent,
    );

    let unfiltered_source = TestSource::exact(artifact.bytes().to_vec());
    let unfiltered_probe = unfiltered_source.clone();
    let unfiltered = ImmutableTableReader::open_source(
        identity("reader-filter-negative-point"),
        unfiltered_source,
        TableReaderConfig::default(),
    )
    .expect("open unfiltered lazy reader");
    let unfiltered_reads = {
        let _capture = crate::observability::perf_trace::begin_test_capture();
        let (row, visited) = unfiltered.seek_physical_key(&missing, None, None);
        assert!(row.is_none());
        assert_eq!(visited, 1);
        crate::observability::perf_trace::snapshot().table_data_block_reads()
    };
    assert_eq!(unfiltered_reads, 1);
    assert_eq!(unfiltered_probe.calls(), 5);

    let filtered_source = TestSource::exact(artifact.bytes().to_vec());
    let filtered_probe = filtered_source.clone();
    let filtered = ImmutableTableReader::open_source(
        identity("reader-filter-negative-point"),
        filtered_source,
        TableReaderConfig::default(),
    )
    .expect("open filtered lazy reader")
    .with_table_filter(filter)
    .expect("attach reader filter");
    assert!(filtered.runtime_facts().filter_available());

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let (row, visited) = filtered.seek_physical_key(&missing, None, None);
    assert!(row.is_none());
    assert_eq!(visited, 0);
    assert_eq!(filtered_probe.calls(), 4);
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.table_data_block_reads(), 0);
    assert_eq!(perf.table_data_block_decodes(), 0);
    assert_eq!(perf.table_point_rows_visited(), 0);
    assert_eq!(perf.table_filter_negative_probes(), 1);
}

#[cfg(feature = "perf-trace")]
#[test]
fn immutable_reader_disabled_filter_matches_point_result_with_more_block_reads() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"charlie".to_vec(), 2),
        put_row(b"delta".to_vec(), 3),
        put_row(b"echo".to_vec(), 4),
    ];
    let (artifact, _) = build_artifact(
        "reader-filter-disabled-point",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    let filter =
        reader_filter_for_table_bytes("reader-filter-disabled-point", artifact.bytes(), 12);
    let missing = find_absent_physical_key_with_probe(
        &filter,
        (0..4096).map(|index| format!("bravo-{index:04}").into_bytes()),
        TableBloomProbe::DefinitelyAbsent,
    );

    let disabled = ImmutableTableReader::open_source(
        identity("reader-filter-disabled-point"),
        TestSource::exact(artifact.bytes().to_vec()),
        TableReaderConfig::default(),
    )
    .expect("open disabled-filter lazy reader")
    .with_table_filter(TableReaderFilter::unavailable())
    .expect("attach unavailable filter");
    assert!(!disabled.runtime_facts().filter_available());
    let disabled_perf = {
        let _capture = crate::observability::perf_trace::begin_test_capture();
        let (row, _) = disabled.seek_physical_key(&missing, None, None);
        assert!(row.is_none());
        crate::observability::perf_trace::snapshot()
    };

    let enabled = ImmutableTableReader::open_source(
        identity("reader-filter-disabled-point"),
        TestSource::exact(artifact.bytes().to_vec()),
        TableReaderConfig::default(),
    )
    .expect("open enabled-filter lazy reader")
    .with_table_filter(filter)
    .expect("attach available filter");
    assert!(enabled.runtime_facts().filter_available());
    let enabled_perf = {
        let _capture = crate::observability::perf_trace::begin_test_capture();
        let (row, _) = enabled.seek_physical_key(&missing, None, None);
        assert!(row.is_none());
        crate::observability::perf_trace::snapshot()
    };

    assert!(disabled_perf.table_data_block_reads() > enabled_perf.table_data_block_reads());
    assert_eq!(enabled_perf.table_data_block_reads(), 0);
    assert_eq!(enabled_perf.table_filter_negative_probes(), 1);
}

#[cfg(feature = "perf-trace")]
#[test]
fn immutable_reader_positive_filter_probe_still_validates_data_block() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"charlie".to_vec(), 2),
        put_row(b"delta".to_vec(), 3),
        put_row(b"echo".to_vec(), 4),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-filter-positive-point",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    let filter =
        reader_filter_for_table_bytes("reader-filter-positive-point", artifact.bytes(), 12);
    let target = table_rows[1].physical_key().clone();
    assert_eq!(
        filter_probe_for_key(&filter, &target),
        TableBloomProbe::MaybePresent
    );
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let reader = ImmutableTableReader::open_source(
        identity("reader-filter-positive-point"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open filtered lazy reader")
    .with_table_filter(filter)
    .expect("attach reader filter");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let (row, visited) = reader.seek_physical_key(&target, None, None);
    assert_eq!(row, Some(table_rows[1].clone()));
    assert_eq!(visited, 1);
    assert_eq!(source_probe.calls(), 5);
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.table_filter_positive_probes(), 1);
    assert_eq!(perf.table_data_block_reads(), 1);
    assert_eq!(perf.table_data_block_decodes(), 1);
    assert_eq!(perf.table_rows_decoded(), 2);
}

#[cfg(feature = "perf-trace")]
#[test]
fn immutable_reader_false_positive_filter_keeps_point_miss_correct() {
    let rows = (0..96)
        .map(|index| {
            put_row(
                format!("key-{:04}", index * 100).into_bytes(),
                u64::try_from(index + 1).expect("version fits u64"),
            )
        })
        .collect::<Vec<_>>();
    let (artifact, _) = build_artifact(
        "reader-filter-false-positive-point",
        &rows,
        4,
        TableCompression::Uncompressed,
    );
    let filter =
        reader_filter_for_table_bytes("reader-filter-false-positive-point", artifact.bytes(), 1);
    let missing = find_absent_physical_key_with_probe(
        &filter,
        (0..96).map(|index| format!("key-{:04}", index * 100 + 1).into_bytes()),
        TableBloomProbe::MaybePresent,
    );
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let reader = ImmutableTableReader::open_source(
        identity("reader-filter-false-positive-point"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open filtered lazy reader")
    .with_table_filter(filter)
    .expect("attach reader filter");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let (row, visited) = reader.seek_physical_key(&missing, None, None);
    assert!(row.is_none());
    assert!(visited > 0);
    assert!(source_probe.calls() > 4);
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.table_filter_positive_probes(), 1);
    assert!(perf.table_data_block_reads() > 0);
    assert!(perf.table_data_block_decodes() > 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn immutable_reader_indexed_point_rows_visited_are_bounded_to_candidate_key_chain() {
    let rows = (0..100)
        .map(|index| {
            put_row(
                format!("key-{index:03}").into_bytes(),
                u64::try_from(index + 1).expect("version fits u64"),
            )
        })
        .collect::<Vec<_>>();
    let (artifact, table_rows) = build_artifact(
        "reader-indexed-point-bounded-visits",
        &rows,
        4,
        TableCompression::Uncompressed,
    );
    let source = TestSource::exact(artifact.bytes().to_vec());
    let reader = ImmutableTableReader::open_source(
        identity("reader-indexed-point-bounded-visits"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy reader");
    let target = table_rows[50].physical_key().clone();

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let (row, visited) = reader.seek_physical_key(&target, None, None);
    assert_eq!(row, Some(table_rows[50].clone()));
    assert_eq!(visited, 1);

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.table_data_block_reads(), 1);
    assert_eq!(perf.table_data_block_decodes(), 1);
    assert_eq!(perf.table_rows_decoded(), 4);
    assert_eq!(perf.table_point_rows_visited(), 1);
}

#[test]
fn immutable_reader_indexed_point_physical_key_chain_can_cross_block_boundaries() {
    let split_key = physical_key(1, "reader", 0x20, b"split".to_vec());
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row_for_key(split_key.clone(), 9, b"v9".to_vec()),
        put_row_for_key(split_key.clone(), 7, b"v7".to_vec()),
        put_row_for_key(split_key.clone(), 5, b"v5".to_vec()),
        put_row_for_key(split_key.clone(), 3, b"v3".to_vec()),
        put_row(b"zulu".to_vec(), 10),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-indexed-point-cross-block-chain",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    let bytes = artifact.bytes().to_vec();
    let eager = ImmutableTableReader::open_bytes(
        identity("reader-indexed-point-cross-block-chain"),
        bytes.clone(),
        TableReaderConfig::default(),
    )
    .expect("open eager reader");
    let lazy = ImmutableTableReader::open_source(
        identity("reader-indexed-point-cross-block-chain"),
        BytesTableSource::new(bytes),
        TableReaderConfig::default(),
    )
    .expect("open lazy reader");
    let version_five = table_rows
        .iter()
        .find(|row| row.physical_key() == &split_key && row.commit_version().as_u64() == 5)
        .expect("version five row")
        .clone();
    let version_three = table_rows
        .iter()
        .find(|row| row.physical_key() == &split_key && row.commit_version().as_u64() == 3)
        .expect("version three row")
        .clone();

    for (max_version, max_timestamp, expected) in [
        (
            Some(CommitVersion::new(6)),
            None,
            Some(version_five.clone()),
        ),
        (None, Some(Timestamp::from_micros(104)), Some(version_three)),
        (Some(CommitVersion::new(2)), None, None),
    ] {
        let (eager_row, _) = eager.seek_physical_key(&split_key, max_version, max_timestamp);
        let (lazy_row, _) = lazy.seek_physical_key(&split_key, max_version, max_timestamp);
        assert_eq!(eager_row, expected);
        assert_eq!(lazy_row, expected);
    }
}

#[cfg(feature = "perf-trace")]
#[test]
fn immutable_reader_physical_key_rows_reads_split_chain_without_materializing() {
    let split_key = physical_key(1, "reader", 0x20, b"history-split".to_vec());
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row_for_key(split_key.clone(), 9, b"v9".to_vec()),
        put_row_for_key(split_key.clone(), 7, b"v7".to_vec()),
        put_row_for_key(split_key.clone(), 5, b"v5".to_vec()),
        put_row_for_key(split_key.clone(), 3, b"v3".to_vec()),
        put_row(b"zulu".to_vec(), 10),
    ];
    let (artifact, _) = build_artifact(
        "reader-physical-key-rows-split-chain",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let reader = ImmutableTableReader::open_source(
        identity("reader-physical-key-rows-split-chain"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy reader");
    let metadata_calls = source_probe.calls();

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let (history_rows, visited) = reader
        .try_physical_key_rows(&split_key)
        .expect("physical key rows");
    let perf = crate::observability::perf_trace::snapshot();

    assert_eq!(
        history_rows
            .iter()
            .map(|row| row.commit_version().as_u64())
            .collect::<Vec<_>>(),
        vec![9, 7, 5, 3]
    );
    assert_eq!(visited, 4);
    assert_eq!(source_probe.calls(), metadata_calls + 3);
    assert_eq!(perf.table_data_block_reads(), 3);
    assert_eq!(perf.table_data_block_decodes(), 3);
    assert_eq!(perf.table_rows_decoded(), 6);
    assert_eq!(perf.table_point_rows_visited(), 4);
}

#[cfg(feature = "perf-trace")]
#[test]
fn immutable_reader_indexed_point_timestamp_bound_walks_only_target_chain_blocks() {
    let split_key = physical_key(1, "reader", 0x20, b"split".to_vec());
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row_for_key(split_key.clone(), 9, b"v9".to_vec()),
        put_row_for_key(split_key.clone(), 7, b"v7".to_vec()),
        put_row_for_key(split_key.clone(), 5, b"v5".to_vec()),
        put_row_for_key(split_key.clone(), 3, b"v3".to_vec()),
        put_row(b"zulu".to_vec(), 10),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-indexed-point-cross-block-timestamp",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    let reader = ImmutableTableReader::open_source(
        identity("reader-indexed-point-cross-block-timestamp"),
        BytesTableSource::new(artifact.bytes().to_vec()),
        TableReaderConfig::default(),
    )
    .expect("open lazy reader");
    let expected = table_rows
        .iter()
        .find(|row| row.physical_key() == &split_key && row.commit_version().as_u64() == 3)
        .expect("version three row")
        .clone();

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let (row, visited) =
        reader.seek_physical_key(&split_key, None, Some(Timestamp::from_micros(104)));

    assert_eq!(row, Some(expected));
    assert_eq!(visited, 4);
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.table_data_block_reads(), 3);
    assert_eq!(perf.table_data_block_decodes(), 3);
    assert_eq!(perf.table_rows_decoded(), 6);
    assert_eq!(perf.table_point_rows_visited(), 4);
}

#[cfg(feature = "perf-trace")]
#[test]
fn immutable_reader_indexed_point_materialized_lazy_rows_are_reused_for_physical_seek() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-indexed-point-materialized",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let reader = ImmutableTableReader::open_source(
        identity("reader-indexed-point-materialized"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy reader");
    assert_eq!(reader.rows(), table_rows.as_slice());
    assert_eq!(source_probe.calls(), 7);
    let target = table_rows[1].physical_key().clone();

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let (row, visited) = reader.seek_physical_key(&target, None, None);

    assert_eq!(row, Some(table_rows[1].clone()));
    assert_eq!(visited, 1);
    assert_eq!(source_probe.calls(), 7);
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.table_seeks(), 1);
    assert_eq!(perf.table_data_block_reads(), 0);
    assert_eq!(perf.table_data_block_decodes(), 0);
    assert_eq!(perf.table_rows_decoded(), 0);
    assert_eq!(perf.table_point_rows_visited(), 1);
}

#[test]
fn immutable_reader_indexed_point_corrupt_candidate_block_errors_but_range_miss_does_not_read() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
    ];
    let (artifact, _) = build_artifact(
        "reader-indexed-point-corrupt-candidate",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let mut bytes = artifact.bytes().to_vec();
    let first_data_payload_offset = MAX_TABLE_HEADER_SIZE + 12;
    bytes[first_data_payload_offset] ^= 0xff;
    let source = TestSource::exact(bytes);
    let source_probe = source.clone();
    let reader = ImmutableTableReader::open_source(
        identity("reader-indexed-point-corrupt-candidate"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy reader with corrupt data block");
    assert_eq!(source_probe.calls(), 4);

    let out_of_range = physical_key(0, "reader", 0x20, b"before".to_vec());
    assert_eq!(
        reader.try_seek_physical_key(&out_of_range, None, None),
        Ok((None, 0))
    );
    assert_eq!(source_probe.calls(), 4);

    let target = physical_key(1, "reader", 0x20, b"alpha".to_vec());
    assert!(matches!(
        reader.try_seek_physical_key(&target, None, None),
        Err(TableRuntimeError::DecodeFormat { .. })
    ));
    assert_eq!(source_probe.calls(), 5);
}

#[test]
fn immutable_reader_indexed_point_exact_encoded_key_lookup_stays_separate_from_physical_seek() {
    let versioned_key = physical_key(1, "reader", 0x20, b"versioned".to_vec());
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row_for_key(versioned_key.clone(), 9, b"newer".to_vec()),
        put_row_for_key(versioned_key.clone(), 2, b"older".to_vec()),
        put_row(b"zulu".to_vec(), 10),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-indexed-point-exact-vs-physical",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let reader = ImmutableTableReader::open_source(
        identity("reader-indexed-point-exact-vs-physical"),
        BytesTableSource::new(artifact.bytes().to_vec()),
        TableReaderConfig::default(),
    )
    .expect("open lazy reader");
    let newer = table_rows
        .iter()
        .find(|row| row.physical_key() == &versioned_key && row.commit_version().as_u64() == 9)
        .expect("newer row")
        .clone();
    let older = table_rows
        .iter()
        .find(|row| row.physical_key() == &versioned_key && row.commit_version().as_u64() == 2)
        .expect("older row")
        .clone();

    assert_eq!(
        reader.try_get_exact(older.key()).expect("exact lookup"),
        Some(older.clone())
    );
    assert_eq!(
        reader.seek_physical_key(&versioned_key, None, None).0,
        Some(newer)
    );
    assert_eq!(
        reader
            .seek_physical_key(&versioned_key, Some(CommitVersion::new(2)), None)
            .0,
        Some(older)
    );
}

#[test]
fn immutable_reader_lazy_range_cursor_reads_only_overlapping_block() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
        put_row(b"delta".to_vec(), 4),
        put_row(b"echo".to_vec(), 5),
        put_row(b"foxtrot".to_vec(), 6),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-lazy-range-single-block",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    assert_eq!(artifact.facts().data_block_count(), 3);
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-range-single-block"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy range reader");
    assert_eq!(source_probe.calls(), 4);

    let lower = table_rows[2].key().clone();
    let upper = table_rows[3].key().clone();
    let bounds = TableKeyBounds::closed(lower, upper).expect("closed bounds");
    let expected = table_rows[2..4]
        .iter()
        .map(|row| row.encoded_key().to_vec())
        .collect::<Vec<_>>();

    #[cfg(feature = "perf-trace")]
    let _capture = crate::observability::perf_trace::begin_test_capture();
    assert_eq!(bounded_reader_keys(&reader, bounds), expected);

    assert_eq!(source_probe.calls(), 5);
    #[cfg(feature = "perf-trace")]
    {
        let perf = crate::observability::perf_trace::snapshot();
        assert_eq!(perf.table_data_block_reads(), 1);
        assert_eq!(perf.table_data_block_decodes(), 1);
        assert_eq!(perf.table_rows_decoded(), 2);
        assert_eq!(perf.table_cursor_rows_visited(), 2);
        assert_eq!(perf.scan_cursor_rows_yielded(), 2);
    }
}

#[test]
fn immutable_reader_lazy_cursor_can_stop_before_next_block_is_decoded() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
        put_row(b"delta".to_vec(), 4),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-lazy-range-limit-stop",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-range-limit-stop"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy cursor reader");
    assert_eq!(source_probe.calls(), 4);

    #[cfg(feature = "perf-trace")]
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let mut cursor = reader.cursor();
    cursor.seek_to_first().expect("seek first");
    assert_eq!(cursor.current(), table_rows.first());
    cursor.advance().expect("advance within first block");
    assert_eq!(cursor.current(), table_rows.get(1));

    assert_eq!(source_probe.calls(), 5);
    #[cfg(feature = "perf-trace")]
    {
        let perf = crate::observability::perf_trace::snapshot();
        assert_eq!(perf.table_data_block_reads(), 1);
        assert_eq!(perf.table_data_block_decodes(), 1);
        assert_eq!(perf.table_rows_decoded(), 2);
        assert_eq!(perf.table_cursor_rows_visited(), 2);
    }
}

#[test]
fn immutable_reader_lazy_cursor_preserves_current_row_when_next_block_errors() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-lazy-range-advance-error",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let mut bytes = artifact.bytes().to_vec();
    corrupt_data_block_payload(&mut bytes, 1);
    let source = TestSource::exact(bytes);
    let source_probe = source.clone();
    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-range-advance-error"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy cursor reader with corrupt later block");
    assert_eq!(source_probe.calls(), 4);

    let mut cursor = reader.cursor();
    cursor.seek_to_first().expect("first block should decode");
    assert_eq!(cursor.current(), table_rows.first());
    assert_eq!(source_probe.calls(), 5);

    let error = cursor
        .advance()
        .expect_err("second block corruption should surface on advance");
    assert!(matches!(error, TableRuntimeError::DecodeFormat { .. }));
    assert_eq!(cursor.current(), table_rows.first());
    assert_eq!(source_probe.calls(), 6);
}

#[test]
fn immutable_reader_lazy_prefix_cursor_skips_nonmatching_blocks_and_stops_before_neighbor() {
    let prefix_key = physical_key(1, "reader", 0x20, b"target".to_vec());
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row_for_key(prefix_key.clone(), 9, b"newer".to_vec()),
        put_row_for_key(prefix_key.clone(), 3, b"older".to_vec()),
        put_row(b"zulu".to_vec(), 10),
        put_row_for_key(
            physical_key(2, "reader", 0x20, b"target".to_vec()),
            11,
            b"other branch".to_vec(),
        ),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-lazy-prefix-single-block",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    assert!(artifact.facts().data_block_count() >= 3);
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-prefix-single-block"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy prefix reader");
    assert_eq!(source_probe.calls(), 4);

    let prefix = TablePhysicalKeyBytes::from_physical_key(&prefix_key);
    let expected = table_rows
        .iter()
        .filter(|row| row.physical_key() == &prefix_key)
        .map(|row| row.encoded_key().to_vec())
        .collect::<Vec<_>>();
    assert_eq!(expected.len(), 2);

    #[cfg(feature = "perf-trace")]
    let _capture = crate::observability::perf_trace::begin_test_capture();
    assert_eq!(
        bounded_reader_keys(&reader, TableKeyBounds::prefix(prefix.as_slice())),
        expected
    );

    assert_eq!(source_probe.calls(), 5);
    #[cfg(feature = "perf-trace")]
    {
        let perf = crate::observability::perf_trace::snapshot();
        assert_eq!(perf.table_data_block_reads(), 1);
        assert_eq!(perf.table_data_block_decodes(), 1);
        assert_eq!(perf.table_rows_decoded(), 2);
        assert_eq!(perf.table_cursor_rows_visited(), 2);
        assert_eq!(perf.scan_cursor_rows_yielded(), 2);
    }
}

#[test]
fn immutable_reader_lazy_cursor_scans_all_blocks_without_materializing_rows() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
        put_row(b"delta".to_vec(), 4),
        put_row(b"echo".to_vec(), 5),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-lazy-cursor-full-scan",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    assert_eq!(artifact.facts().data_block_count(), 3);
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-cursor-full-scan"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy cursor reader");
    assert_eq!(source_probe.calls(), 4);
    assert_eq!(reader.runtime_facts().rows_materialized(), 0);

    #[cfg(feature = "perf-trace")]
    let _capture = crate::observability::perf_trace::begin_test_capture();
    assert_eq!(all_reader_keys(&reader), encoded_row_keys(&table_rows));

    assert_eq!(reader.runtime_facts().rows_materialized(), 0);
    assert_eq!(
        source_probe.calls(),
        4 + usize::try_from(artifact.facts().data_block_count()).expect("block count fits")
    );
    #[cfg(feature = "perf-trace")]
    {
        let perf = crate::observability::perf_trace::snapshot();
        assert_eq!(
            perf.table_data_block_reads(),
            u64::from(artifact.facts().data_block_count())
        );
        assert_eq!(
            perf.table_data_block_decodes(),
            u64::from(artifact.facts().data_block_count())
        );
        assert_eq!(perf.table_rows_decoded(), artifact.facts().row_count());
        assert_eq!(
            perf.table_cursor_rows_visited(),
            artifact.facts().row_count()
        );
    }
}

#[test]
fn immutable_reader_lazy_cursor_seek_and_reseek_use_index_without_materializing_rows() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
        put_row(b"delta".to_vec(), 4),
        put_row(b"echo".to_vec(), 5),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-lazy-cursor-seek-reseek",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    assert_eq!(artifact.facts().data_block_count(), 3);
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-cursor-seek-reseek"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy cursor seek reader");
    assert_eq!(source_probe.calls(), 4);
    assert_eq!(reader.runtime_facts().rows_materialized(), 0);

    #[cfg(feature = "perf-trace")]
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let mut cursor = reader.cursor();
    cursor
        .seek(table_rows[3].key())
        .expect("seek into later block");
    assert_eq!(cursor.current(), table_rows.get(3));
    assert_eq!(source_probe.calls(), 5);

    cursor
        .seek(table_rows[1].key())
        .expect("reseek earlier block");
    assert_eq!(cursor.current(), table_rows.get(1));
    assert_eq!(source_probe.calls(), 6);

    let after_last = TableInternalKeyBytes::from_row(&put_row(b"zulu".to_vec(), 99));
    cursor.seek(&after_last).expect("seek after table");
    assert!(cursor.current().is_none());
    assert_eq!(source_probe.calls(), 6);
    assert_eq!(reader.runtime_facts().rows_materialized(), 0);

    #[cfg(feature = "perf-trace")]
    {
        let perf = crate::observability::perf_trace::snapshot();
        assert_eq!(perf.table_data_block_reads(), 2);
        assert_eq!(perf.table_data_block_decodes(), 2);
        assert_eq!(perf.table_rows_decoded(), 4);
        assert_eq!(perf.table_cursor_rows_visited(), 2);
    }
}

#[test]
fn immutable_reader_lazy_open_range_cursor_skips_excluded_endpoints_without_upper_block_read() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
        put_row(b"delta".to_vec(), 4),
        put_row(b"echo".to_vec(), 5),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-lazy-cursor-open-range",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-cursor-open-range"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy open-range cursor reader");
    assert_eq!(source_probe.calls(), 4);

    let lower = table_rows[1].key().clone();
    let selected = table_rows[2].encoded_key().to_vec();
    let upper = table_rows[3].key().clone();
    let bounds = TableKeyBounds::open(lower, upper).expect("open bounds");

    #[cfg(feature = "perf-trace")]
    let _capture = crate::observability::perf_trace::begin_test_capture();
    assert_eq!(bounded_reader_keys(&reader, bounds), vec![selected]);

    assert_eq!(
        source_probe.calls(),
        6,
        "lazy open-range cursor should read only the excluded lower block and selected block"
    );
    #[cfg(feature = "perf-trace")]
    {
        let perf = crate::observability::perf_trace::snapshot();
        assert_eq!(perf.table_data_block_reads(), 2);
        assert_eq!(perf.table_data_block_decodes(), 2);
        assert_eq!(perf.table_rows_decoded(), 2);
        assert_eq!(perf.scan_cursor_rows_yielded(), 1);
    }
}

#[test]
fn immutable_reader_lazy_exact_cursor_miss_after_table_reads_no_data_blocks() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
    ];
    let (artifact, _) = build_artifact(
        "reader-lazy-cursor-after-table-miss",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-cursor-after-table-miss"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy exact-miss cursor reader");
    assert_eq!(source_probe.calls(), 4);
    let after_last = TableInternalKeyBytes::from_row(&put_row(b"zulu".to_vec(), 99));

    #[cfg(feature = "perf-trace")]
    let _capture = crate::observability::perf_trace::begin_test_capture();
    assert_eq!(
        bounded_reader_keys(&reader, TableKeyBounds::exact(after_last)),
        Vec::<Vec<u8>>::new()
    );

    assert_eq!(source_probe.calls(), 4);
    #[cfg(feature = "perf-trace")]
    {
        let perf = crate::observability::perf_trace::snapshot();
        assert_eq!(perf.table_data_block_reads(), 0);
        assert_eq!(perf.table_data_block_decodes(), 0);
        assert_eq!(perf.table_rows_decoded(), 0);
        assert_eq!(perf.scan_cursor_rows_yielded(), 0);
    }
}

#[test]
fn immutable_reader_lazy_physical_range_cursor_reads_only_matching_key_chain() {
    let target = physical_key(1, "reader", 0x20, b"target".to_vec());
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row_for_key(target.clone(), 9, b"newer".to_vec()),
        put_row_for_key(target.clone(), 3, b"older".to_vec()),
        put_row(b"zulu".to_vec(), 10),
        put_row_for_key(
            physical_key(2, "reader", 0x20, b"target".to_vec()),
            11,
            b"other branch".to_vec(),
        ),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-lazy-cursor-physical-range",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let source = TestSource::exact(artifact.bytes().to_vec());
    let source_probe = source.clone();
    let reader = ImmutableTableReader::open_source(
        identity("reader-lazy-cursor-physical-range"),
        source,
        TableReaderConfig::default(),
    )
    .expect("open lazy physical-range cursor reader");
    assert_eq!(source_probe.calls(), 4);

    let physical_prefix = TablePhysicalKeyBytes::from_physical_key(&target);
    let bounds = TableKeyBounds::physical_range(
        &physical_prefix,
        TablePhysicalKeyBound::Unbounded,
        TablePhysicalKeyBound::Unbounded,
    )
    .expect("physical range bounds");
    let expected = table_rows
        .iter()
        .filter(|row| row.physical_key() == &target)
        .map(|row| row.encoded_key().to_vec())
        .collect::<Vec<_>>();
    assert_eq!(expected.len(), 2);

    #[cfg(feature = "perf-trace")]
    let _capture = crate::observability::perf_trace::begin_test_capture();
    assert_eq!(bounded_reader_keys(&reader, bounds), expected);

    assert_eq!(
        source_probe.calls(),
        6,
        "lazy physical-range cursor should read only the matching key-chain blocks"
    );
    #[cfg(feature = "perf-trace")]
    {
        let perf = crate::observability::perf_trace::snapshot();
        assert_eq!(perf.table_data_block_reads(), 2);
        assert_eq!(perf.table_data_block_decodes(), 2);
        assert_eq!(perf.table_rows_decoded(), 2);
        assert_eq!(perf.scan_cursor_rows_yielded(), 2);
    }
}

#[cfg(feature = "perf-trace")]
#[derive(Clone, Copy)]
struct LazyOpenExpectation {
    source_calls: usize,
    metadata_frames_read: bool,
}

#[cfg(feature = "perf-trace")]
fn assert_lazy_open_runtime_facts(reader: &ImmutableTableReader<'_>) {
    assert_eq!(
        reader.runtime_facts().open_mode(),
        TableReaderOpenMode::LazySource
    );
    assert!(reader.runtime_facts().metadata_loaded());
    assert!(reader.runtime_facts().index_loaded());
    assert_eq!(reader.runtime_facts().data_blocks_loaded(), 0);
    assert_eq!(reader.runtime_facts().rows_materialized(), 0);
    assert!(!reader.runtime_facts().filter_available());
    assert!(!reader.runtime_facts().cache_enabled());
}

#[cfg(feature = "perf-trace")]
fn assert_lazy_open_perf_snapshot(
    snapshot: &crate::observability::perf_trace::StoragePerfSnapshot,
    metadata_frames_read: bool,
) {
    assert_eq!(snapshot.table_reader_opens(), 1);
    assert_eq!(
        snapshot.table_metadata_read_bytes(),
        u64::try_from(MAX_TABLE_HEADER_SIZE + MAX_TABLE_FOOTER_SIZE).expect("metadata bytes fit")
    );
    if metadata_frames_read {
        assert!(snapshot.table_index_read_bytes() > 0);
        assert!(snapshot.table_properties_read_bytes() > 0);
    } else {
        assert_eq!(snapshot.table_index_read_bytes(), 0);
        assert_eq!(snapshot.table_properties_read_bytes(), 0);
    }
    assert_eq!(snapshot.table_data_block_reads(), 0);
    assert_eq!(snapshot.table_data_block_read_bytes(), 0);
    assert_eq!(snapshot.table_data_block_decodes(), 0);
    assert_eq!(snapshot.table_rows_decoded(), 0);
}

#[cfg(feature = "perf-trace")]
fn assert_lazy_decode_error_before_data_reads(
    label: &'static str,
    bytes: Vec<u8>,
    expectation: LazyOpenExpectation,
) {
    let source = TestSource::exact(bytes);
    let source_probe = source.clone();
    let _capture = crate::observability::perf_trace::begin_test_capture();

    let error = ImmutableTableReader::open_source(
        TableIdentity::new(format!("reader-lazy-open-{label}")).expect("identity"),
        source,
        TableReaderConfig::default(),
    )
    .expect_err("lazy metadata open should reject malformed metadata");

    assert!(matches!(error, TableRuntimeError::DecodeFormat { .. }));
    assert_eq!(source_probe.calls(), expectation.source_calls);
    assert_lazy_open_perf_snapshot(
        &crate::observability::perf_trace::snapshot(),
        expectation.metadata_frames_read,
    );
}

#[cfg(feature = "perf-trace")]
fn increment_first_index_entry_row_count(bytes: &mut [u8]) {
    let ranges = table_metadata_ranges(bytes);
    let frame_start = checked_table_offset(ranges.index_offset);
    let frame_len = usize::try_from(ranges.index_len).expect("index frame len fits usize");
    assert_eq!(bytes[frame_start], 2);
    assert_eq!(bytes[frame_start + 1], 0);
    let encoded_len = read_u32_le_at(bytes, frame_start + 4) as usize;
    let decoded_len = read_u32_le_at(bytes, frame_start + 8) as usize;
    assert_eq!(encoded_len, decoded_len);
    let payload_start = frame_start + 12;
    let payload_end = payload_start + encoded_len;
    assert_eq!(payload_end + 4, frame_start + frame_len);

    let first_key_len = read_u32_le_at(bytes, payload_start + 8) as usize;
    let last_key_len_offset = payload_start + 12 + first_key_len;
    let last_key_len = read_u32_le_at(bytes, last_key_len_offset) as usize;
    let row_count_offset = last_key_len_offset + 4 + last_key_len + 8 + 4;
    let row_count = read_u32_le_at(bytes, row_count_offset);
    bytes[row_count_offset..row_count_offset + 4]
        .copy_from_slice(&row_count.saturating_add(1).to_le_bytes());

    let crc_offset = frame_start + frame_len - 4;
    let crc = crc32fast::hash(&bytes[frame_start..crc_offset]);
    bytes[crc_offset..crc_offset + 4].copy_from_slice(&crc.to_le_bytes());
}

#[cfg(feature = "perf-trace")]
fn read_u32_le_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("u32 fixture bytes"),
    )
}

#[cfg(feature = "perf-trace")]
#[test]
fn immutable_reader_query_perf_counters_record_seek_and_cursor_work() {
    let versioned_key = physical_key(2, "reader", 0x20, b"counter-key".to_vec());
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row_for_key(versioned_key.clone(), 9, b"newer".to_vec()),
        put_row_for_key(versioned_key.clone(), 3, b"older".to_vec()),
        put_row(b"zulu".to_vec(), 10),
    ];
    let (artifact, _) = build_artifact(
        "reader-query-query-counters",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    let reader = ImmutableTableReader::open_bytes(
        identity("reader-query-query-counters"),
        artifact.into_bytes(),
        TableReaderConfig::default(),
    )
    .expect("open reader");

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let (row, visited) = reader.seek_physical_key(&versioned_key, None, None);
    assert!(row.is_some());
    assert!(visited >= 1);
    let mut cursor = reader.cursor();
    cursor.seek_to_first().expect("seek first");
    cursor.advance().expect("advance");

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.table_seeks(), 1);
    assert_eq!(
        perf.table_point_rows_visited(),
        u64::try_from(visited).expect("visited rows fit")
    );
    assert_eq!(perf.table_cursor_rows_visited(), 2);
}

#[test]
fn immutable_reader_one_row_facts_and_runtime_config_are_preserved() {
    let runtime = TableRuntimeConfig::new(
        TableBuilderConfig::new(256, 4, TableCompression::Uncompressed).expect("builder config"),
        TableReaderConfig::new(),
        TableCacheConfig::new(false, 0).expect("cache config"),
        TableCompactionConfig::new(4096, 8).expect("compaction config"),
    )
    .expect("runtime config");
    let row = put_row_for_key(
        physical_key(7, "reader", 0x25, b"single".to_vec()),
        42,
        b"one".to_vec(),
    );
    let table_row = TableRow::new(row.clone());
    let artifact = ImmutableTableBuilder::from_runtime_config(&runtime)
        .expect("builder from runtime")
        .build_from_storage_rows(identity("reader-one-row"), &[row])
        .expect("build one-row table");

    let reader = ImmutableTableReader::open_bytes(
        identity("reader-one-row"),
        artifact.bytes().to_vec(),
        *runtime.reader(),
    )
    .expect("open one-row reader");

    assert_eq!(reader.config(), *runtime.reader());
    assert_eq!(reader.facts().identity(), &identity("reader-one-row"));
    assert_eq!(reader.facts().row_count(), 1);
    assert_eq!(reader.facts().data_block_count(), 1);
    assert_eq!(reader.facts().commit_range().min(), CommitVersion::new(42));
    assert_eq!(reader.facts().commit_range().max(), CommitVersion::new(42));
    assert_eq!(
        reader.facts().key_range().first_key(),
        table_row.encoded_key()
    );
    assert_eq!(
        reader.facts().key_range().last_key(),
        table_row.encoded_key()
    );
    assert_eq!(reader.byte_count(), artifact.byte_count());
    assert_eq!(reader.rows(), std::slice::from_ref(&table_row));

    let mut cursor = reader.cursor();
    cursor.seek_to_first().expect("seek one-row reader");
    assert_eq!(
        collect_keys(&mut cursor),
        vec![table_row.encoded_key().to_vec()]
    );
}

#[test]
fn immutable_reader_rejects_identity_and_partial_or_legacy_table_bytes() {
    assert_eq!(
        TableIdentity::new("table/with/slash"),
        Err(TableRuntimeError::InvalidConfig {
            field: "table_identity",
            reason: "must be an opaque single component",
        })
    );

    let (artifact, _) = build_artifact(
        "reader-invalid-bytes",
        &[put_row(b"alpha".to_vec(), 1), put_row(b"bravo".to_vec(), 2)],
        1,
        TableCompression::Uncompressed,
    );

    assert_decode_format(Vec::new(), "reader-empty-bytes");
    assert_decode_format(artifact.bytes()[..64].to_vec(), "reader-header-only");
    assert_decode_format(
        artifact.bytes()[artifact.bytes().len() - 64..].to_vec(),
        "reader-footer-only",
    );

    let mut old_magic = artifact.bytes().to_vec();
    old_magic[..4].copy_from_slice(b"STRM");
    assert_decode_format(old_magic, "reader-old-magic");

    let mut pre_v1 = artifact.bytes().to_vec();
    pre_v1[4..8].copy_from_slice(&0u32.to_le_bytes());
    assert_decode_format(pre_v1, "reader-pre-v1");

    let mut future = artifact.bytes().to_vec();
    future[4..8].copy_from_slice(&2u32.to_le_bytes());
    assert_decode_format(future, "reader-future-version");
}

#[test]
fn immutable_reader_cursor_seek_and_bounds_match_sorted_model() {
    let prefix_key = physical_key(1, "reader", 0x20, b"prefix".to_vec());
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        tombstone_row(b"bravo".to_vec(), 2),
        expired_row(b"charlie".to_vec(), 3),
        put_row_for_key(prefix_key.clone(), 9, b"newer".to_vec()),
        put_row_for_key(prefix_key.clone(), 3, b"older".to_vec()),
        put_row_for_key(
            physical_key(2, "reader", 0x20, b"prefix".to_vec()),
            1,
            b"other".to_vec(),
        ),
    ];
    let table_rows = sorted_table_rows(&rows);
    let artifact = builder(1, TableCompression::Uncompressed)
        .build_from_rows(identity("reader-cursor"), &table_rows)
        .expect("build reader cursor");
    assert!(artifact.facts().data_block_count() > 1);
    let reader = ImmutableTableReader::open_bytes(
        identity("reader-cursor"),
        artifact.into_bytes(),
        TableReaderConfig::default(),
    )
    .expect("open reader cursor");

    let expected_keys = table_rows
        .iter()
        .map(|row| row.encoded_key().to_vec())
        .collect::<Vec<_>>();
    let mut cursor = reader.cursor();
    assert!(cursor.current().is_none());
    cursor.seek_to_first().expect("seek first");
    assert_eq!(collect_keys(&mut cursor), expected_keys);

    for target in [
        table_rows[0].key().clone(),
        table_rows[table_rows.len() / 2].key().clone(),
        TableInternalKeyBytes::from_row(&put_row(b"after".to_vec(), 99)),
    ] {
        let mut cursor = reader.cursor();
        cursor.seek(&target).expect("seek target");
        assert_eq!(
            cursor.current_key().map(|key| key.as_slice().to_vec()),
            expected_seek_key(&table_rows, &target)
        );
    }

    let lower = table_rows[1].key().clone();
    let upper = table_rows[table_rows.len() - 2].key().clone();
    let mut closed = reader.bounded_cursor(
        TableKeyBounds::closed(lower.clone(), upper.clone()).expect("closed bounds"),
    );
    closed.seek_to_first().expect("seek closed bounds");
    let expected_closed = table_rows
        .iter()
        .filter(|row| row.key() >= &lower && row.key() <= &upper)
        .map(|row| row.encoded_key().to_vec())
        .collect::<Vec<_>>();
    assert_eq!(collect_keys(&mut closed), expected_closed);

    let prefix = TablePhysicalKeyBytes::from_physical_key(&prefix_key);
    let mut bounded_prefix = reader.bounded_cursor(TableKeyBounds::prefix(prefix.as_slice()));
    bounded_prefix.seek_to_first().expect("seek prefix bounds");
    let expected_prefix = table_rows
        .iter()
        .filter(|row| prefix.is_prefix_of(row.key()))
        .map(|row| row.encoded_key().to_vec())
        .collect::<Vec<_>>();
    assert_eq!(collect_keys(&mut bounded_prefix), expected_prefix);
}

#[test]
fn immutable_reader_exact_lookup_covers_boundaries_versions_and_misses() {
    let versioned_key = physical_key(2, "reader", 0x20, b"multi-version\0key".to_vec());
    let rows = vec![
        put_row_for_key(
            physical_key(1, "reader", 0x20, b"first".to_vec()),
            1,
            b"first".to_vec(),
        ),
        put_row_for_key(versioned_key.clone(), 9, b"newer".to_vec()),
        put_row_for_key(versioned_key.clone(), 2, b"older".to_vec()),
        tombstone_row(b"deleted".to_vec(), 6),
        expired_row(b"expired".to_vec(), 5),
        put_row_for_key(
            physical_key(8, "reader", 0x25, b"last".to_vec()),
            99,
            Vec::new(),
        ),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-exact-lookup",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    let reader = ImmutableTableReader::open_bytes(
        identity("reader-exact-lookup"),
        artifact.into_bytes(),
        TableReaderConfig::default(),
    )
    .expect("open exact lookup reader");

    for index in [0, table_rows.len() / 2, table_rows.len() - 1] {
        let row = &table_rows[index];
        assert_eq!(reader.get_exact(row.key()), Some(row.clone()));
    }
    for row in table_rows
        .iter()
        .filter(|row| row.physical_key() == &versioned_key)
    {
        assert_eq!(reader.get_exact(row.key()), Some(row.clone()));
    }

    let absent_version = TableInternalKeyBytes::from_row(&put_row_for_key(
        versioned_key,
        5,
        b"absent-version".to_vec(),
    ));
    let before_first = TableInternalKeyBytes::from_row(&put_row_for_key(
        physical_key(0, "reader", 0x20, b"before".to_vec()),
        1,
        Vec::new(),
    ));
    let after_last = TableInternalKeyBytes::from_row(&put_row_for_key(
        physical_key(9, "reader", 0x20, b"after".to_vec()),
        1,
        Vec::new(),
    ));

    assert_eq!(reader.get_exact(&absent_version), None);
    assert_eq!(reader.get_exact(&before_first), None);
    assert_eq!(reader.get_exact(&after_last), None);
}

#[test]
fn immutable_reader_cursor_state_reseek_and_exhaustion_are_stable() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row(b"charlie".to_vec(), 3),
        put_row(b"delta".to_vec(), 4),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-cursor-state",
        &rows,
        1,
        TableCompression::Uncompressed,
    );
    let reader = ImmutableTableReader::open_bytes(
        identity("reader-cursor-state"),
        artifact.into_bytes(),
        TableReaderConfig::default(),
    )
    .expect("open cursor state reader");

    let before_first = TableInternalKeyBytes::from_row(&put_row_for_key(
        physical_key(0, "reader", 0x20, b"before".to_vec()),
        1,
        Vec::new(),
    ));
    let after_last = TableInternalKeyBytes::from_row(&put_row_for_key(
        physical_key(9, "reader", 0x20, b"after".to_vec()),
        1,
        Vec::new(),
    ));

    let mut cursor = reader.cursor();
    assert!(cursor.current().is_none());
    cursor.seek(&before_first).expect("seek before first");
    assert_eq!(cursor.current(), table_rows.first());
    assert_eq!(cursor.current_key(), table_rows.first().map(TableRow::key));
    assert_eq!(cursor.current(), table_rows.first());

    cursor.advance().expect("advance");
    assert_eq!(cursor.current(), table_rows.get(1));
    cursor.seek(table_rows[2].key()).expect("reseek forward");
    assert_eq!(cursor.current(), table_rows.get(2));
    cursor
        .seek(table_rows[2].key())
        .expect("repeat same target seek");
    assert_eq!(cursor.current(), table_rows.get(2));

    cursor.seek(&after_last).expect("seek after last");
    assert!(cursor.current().is_none());
    assert!(cursor.current_key().is_none());
    cursor.advance().expect("advance exhausted");
    assert!(cursor.current().is_none());

    cursor
        .seek_to_first()
        .expect("reseek first after exhaustion");
    assert_eq!(cursor.current(), table_rows.first());

    let mut exhaust = reader.cursor();
    exhaust
        .seek(table_rows.last().expect("last row").key())
        .expect("seek last");
    assert_eq!(exhaust.current(), table_rows.last());
    exhaust.advance().expect("advance past last");
    assert!(exhaust.current().is_none());
    exhaust.advance().expect("second exhausted advance");
    assert!(exhaust.current().is_none());
}

fn bound_shape_reader() -> (ImmutableTableReader<'static>, Vec<TableRow>, PhysicalKey) {
    let prefix_key = physical_key(1, "reader", 0x20, b"prefix".to_vec());
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row_for_key(prefix_key.clone(), 9, b"newer".to_vec()),
        put_row_for_key(prefix_key.clone(), 3, b"older".to_vec()),
        put_row_for_key(
            physical_key(1, "reader", 0x21, b"prefix".to_vec()),
            8,
            b"different storage space".to_vec(),
        ),
        put_row_for_key(
            physical_key(1, "reader-alt", 0x20, b"prefix".to_vec()),
            7,
            b"different logical space".to_vec(),
        ),
        put_row_for_key(
            physical_key(1, "reader", 0x20, b"prefix-extra".to_vec()),
            6,
            b"prefix-like neighbor".to_vec(),
        ),
        put_row(b"zulu".to_vec(), 10),
        put_row_for_key(
            physical_key(2, "reader", 0x20, b"prefix".to_vec()),
            1,
            b"other branch".to_vec(),
        ),
    ];
    let (artifact, table_rows) = build_artifact(
        "reader-bound-shapes",
        &rows,
        2,
        TableCompression::Uncompressed,
    );
    let reader = ImmutableTableReader::open_bytes(
        identity("reader-bound-shapes"),
        artifact.into_bytes(),
        TableReaderConfig::default(),
    )
    .expect("open bound shapes reader");

    (reader, table_rows, prefix_key)
}

#[test]
fn immutable_reader_range_bounds_cover_inclusive_exclusive_shapes() {
    let (reader, table_rows, prefix_key) = bound_shape_reader();

    let all_keys = encoded_row_keys(&table_rows);
    assert_eq!(
        bounded_reader_keys(&reader, TableKeyBounds::unbounded()),
        all_keys
    );

    let present = table_rows[2].key().clone();
    assert_eq!(
        bounded_reader_keys(&reader, TableKeyBounds::exact(present.clone())),
        vec![present.as_slice().to_vec()]
    );

    let absent = TableInternalKeyBytes::from_row(&put_row_for_key(
        prefix_key.clone(),
        5,
        b"missing version".to_vec(),
    ));
    assert!(bounded_reader_keys(&reader, TableKeyBounds::exact(absent)).is_empty());

    let lower = table_rows[1].key().clone();
    let upper = table_rows[table_rows.len() - 2].key().clone();
    assert_eq!(
        bounded_reader_keys(
            &reader,
            TableKeyBounds::closed(lower.clone(), upper.clone()).expect("closed bounds"),
        ),
        table_rows
            .iter()
            .filter(|row| row.key() >= &lower && row.key() <= &upper)
            .map(|row| row.encoded_key().to_vec())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        bounded_reader_keys(
            &reader,
            TableKeyBounds::open(lower.clone(), upper.clone()).expect("open bounds"),
        ),
        table_rows
            .iter()
            .filter(|row| row.key() > &lower && row.key() < &upper)
            .map(|row| row.encoded_key().to_vec())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        bounded_reader_keys(
            &reader,
            TableKeyBounds::range(
                TableKeyBound::included(lower.clone()),
                TableKeyBound::excluded(upper.clone()),
            )
            .expect("half-open bounds"),
        ),
        table_rows
            .iter()
            .filter(|row| row.key() >= &lower && row.key() < &upper)
            .map(|row| row.encoded_key().to_vec())
            .collect::<Vec<_>>()
    );
}

#[test]
fn immutable_reader_range_bounds_cover_unbounded_and_degenerate_shapes() {
    let (reader, table_rows, _) = bound_shape_reader();

    let lower = table_rows[1].key().clone();
    let upper = table_rows[table_rows.len() - 2].key().clone();
    assert_eq!(
        bounded_reader_keys(
            &reader,
            TableKeyBounds::range(
                TableKeyBound::Unbounded,
                TableKeyBound::included(upper.clone())
            )
            .expect("lower-unbounded bounds"),
        ),
        table_rows
            .iter()
            .filter(|row| row.key() <= &upper)
            .map(|row| row.encoded_key().to_vec())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        bounded_reader_keys(
            &reader,
            TableKeyBounds::range(
                TableKeyBound::excluded(lower.clone()),
                TableKeyBound::Unbounded
            )
            .expect("upper-unbounded bounds"),
        ),
        table_rows
            .iter()
            .filter(|row| row.key() > &lower)
            .map(|row| row.encoded_key().to_vec())
            .collect::<Vec<_>>()
    );

    let singleton = table_rows[3].key().clone();
    assert_eq!(
        bounded_reader_keys(
            &reader,
            TableKeyBounds::closed(singleton.clone(), singleton.clone()).expect("singleton bounds"),
        ),
        vec![singleton.as_slice().to_vec()]
    );
    assert!(bounded_reader_keys(
        &reader,
        TableKeyBounds::open(singleton.clone(), singleton).expect("empty open singleton"),
    )
    .is_empty());
    assert!(matches!(
        TableKeyBounds::closed(upper.clone(), lower.clone()),
        Err(TableRuntimeError::InvalidRange {
            field: "key_bounds"
        })
    ));
}

#[test]
fn immutable_reader_prefix_bounds_do_not_cross_physical_key() {
    let (reader, table_rows, prefix_key) = bound_shape_reader();

    let prefix = TablePhysicalKeyBytes::from_physical_key(&prefix_key);
    assert_eq!(
        bounded_reader_keys(&reader, TableKeyBounds::prefix(prefix.as_slice())),
        table_rows
            .iter()
            .filter(|row| row.physical_key() == &prefix_key)
            .map(|row| row.encoded_key().to_vec())
            .collect::<Vec<_>>()
    );
}

#[test]
fn immutable_reader_preserves_zstd_and_row_shapes() {
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row_for_key(
            physical_key(1, "reader", 0x01, b"storage-owned".to_vec()),
            8,
            Vec::new(),
        ),
        tombstone_row(b"delete".to_vec(), 3),
        expired_row(b"expired".to_vec(), 4),
    ];
    let table_rows = sorted_table_rows(&rows);
    let expected_rows = sorted_storage_rows(&rows);
    let artifact = builder(2, TableCompression::Zstd)
        .build_from_rows(identity("reader-zstd"), &table_rows)
        .expect("build zstd reader");

    let reader = ImmutableTableReader::open_bytes(
        identity("reader-zstd"),
        artifact.into_bytes(),
        TableReaderConfig::default(),
    )
    .expect("open zstd reader");

    assert_eq!(
        reader
            .rows()
            .iter()
            .map(|row| row.row().clone())
            .collect::<Vec<_>>(),
        expected_rows
    );
    assert!(reader.rows().iter().any(TableRow::is_tombstone));
    assert!(reader
        .rows()
        .iter()
        .any(|row| row.expires_at() == Timestamp::from_micros(1)));

    let mut cursor = reader.cursor();
    cursor.seek_to_first().expect("seek zstd reader");
    assert_eq!(collect_keys(&mut cursor), encoded_row_keys(&table_rows));
}

#[test]
fn immutable_reader_bytes_and_source_paths_are_identical_for_queries() {
    let prefix_key = physical_key(1, "reader", 0x20, b"shared-prefix".to_vec());
    let rows = vec![
        put_row(b"alpha".to_vec(), 1),
        put_row(b"bravo".to_vec(), 2),
        put_row_for_key(prefix_key.clone(), 9, b"newer".to_vec()),
        put_row_for_key(prefix_key.clone(), 3, b"older".to_vec()),
        tombstone_row(b"deleted".to_vec(), 4),
        expired_row(b"expired".to_vec(), 5),
        put_row_for_key(
            physical_key(2, "reader", 0x21, b"other-space".to_vec()),
            6,
            vec![0x5a; 4096],
        ),
    ];
    let (artifact, table_rows) = build_artifact("reader-parity", &rows, 2, TableCompression::Zstd);
    let config = TableReaderConfig::new();
    let bytes = artifact.bytes().to_vec();
    let source = TestSource::exact(bytes.clone());
    let source_probe = source.clone();

    let byte_reader = ImmutableTableReader::open_bytes(identity("reader-parity"), bytes, config)
        .expect("open byte reader");
    let source_reader =
        ImmutableTableReader::open_source(identity("reader-parity"), source, config)
            .expect("open source reader");

    assert_eq!(source_probe.calls(), 4);
    assert_eq!(byte_reader.config(), source_reader.config());
    assert_eq!(byte_reader.facts(), source_reader.facts());
    assert_eq!(
        byte_reader.runtime_facts().open_mode(),
        TableReaderOpenMode::EagerBytes
    );
    assert_eq!(
        source_reader.runtime_facts().open_mode(),
        TableReaderOpenMode::LazySource
    );
    assert_eq!(source_reader.runtime_facts().data_blocks_loaded(), 0);
    assert_eq!(source_reader.runtime_facts().rows_materialized(), 0);
    assert_eq!(byte_reader.rows(), source_reader.rows());
    assert_eq!(byte_reader, source_reader);
    assert_eq!(
        source_probe.calls(),
        usize::try_from(artifact.facts().data_block_count()).expect("block count fits") + 4
    );
    assert_eq!(
        all_reader_keys(&byte_reader),
        all_reader_keys(&source_reader)
    );

    for row in &table_rows {
        assert_eq!(
            byte_reader.get_exact(row.key()),
            source_reader.get_exact(row.key())
        );
    }
    let missing = TableInternalKeyBytes::from_row(&put_row(b"missing".to_vec(), 999));
    assert_eq!(
        byte_reader.get_exact(&missing),
        source_reader.get_exact(&missing)
    );

    let lower = table_rows[1].key().clone();
    let upper = table_rows[table_rows.len() - 2].key().clone();
    let closed = TableKeyBounds::closed(lower, upper).expect("closed parity bounds");
    assert_eq!(
        bounded_reader_keys(&byte_reader, closed.clone()),
        bounded_reader_keys(&source_reader, closed)
    );

    let prefix = TablePhysicalKeyBytes::from_physical_key(&prefix_key);
    assert_eq!(
        bounded_reader_keys(&byte_reader, TableKeyBounds::prefix(prefix.as_slice())),
        bounded_reader_keys(&source_reader, TableKeyBounds::prefix(prefix.as_slice()))
    );
}

#[test]
fn immutable_reader_rejects_corrupt_table_bytes_as_decode_format() {
    let rows = sorted_table_rows(&[put_row(b"alpha".to_vec(), 1)]);
    let artifact = builder(1, TableCompression::Uncompressed)
        .build_from_rows(identity("reader-corrupt"), &rows)
        .expect("build corrupt source");
    let mut bytes = artifact.bytes().to_vec();
    bytes[0] = b'X';

    let error = ImmutableTableReader::open_bytes(
        identity("reader-corrupt"),
        bytes,
        TableReaderConfig::default(),
    )
    .expect_err("corrupt bytes should fail");

    assert!(matches!(error, TableRuntimeError::DecodeFormat { .. }));
    assert!(error.source().is_some());

    let mut source_corrupt = artifact.bytes().to_vec();
    source_corrupt[0] = b'X';
    let error = ImmutableTableReader::open_source(
        identity("reader-source-corrupt"),
        BytesTableSource::new(source_corrupt),
        TableReaderConfig::default(),
    )
    .expect_err("corrupt source bytes should fail");
    assert!(matches!(error, TableRuntimeError::DecodeFormat { .. }));
    assert!(error.source().is_some());

    let mut truncated = artifact.bytes().to_vec();
    truncated.pop();
    let error = ImmutableTableReader::open_bytes(
        identity("reader-truncated"),
        truncated,
        TableReaderConfig::default(),
    )
    .expect_err("truncated bytes should fail");

    assert!(matches!(error, TableRuntimeError::DecodeFormat { .. }));
    assert!(error.source().is_some());
}

#[test]
fn bytes_table_source_enforces_exact_ranges() {
    let source = BytesTableSource::new(vec![1, 2, 3, 4]);

    assert_eq!(source.byte_count(), 4);
    assert_eq!(source.read_at(1, 2).expect("range"), vec![2, 3]);
    assert_eq!(
        source.read_at(4, 0).expect("empty end range"),
        Vec::<u8>::new()
    );
    assert_eq!(
        source.read_at(3, 2),
        Err(TableRuntimeError::source_read(
            "byte range exceeds source length"
        ))
    );
    assert!(matches!(
        source.read_at(3, usize::MAX),
        Err(TableRuntimeError::InvalidRange {
            field: "byte_range"
        })
    ));
}
