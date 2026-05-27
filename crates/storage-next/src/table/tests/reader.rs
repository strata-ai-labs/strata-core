use crate::format::TableCompression;
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    sort_table_rows_by_key, BuiltTableArtifact, BytesTableSource, ImmutableTableBuilder,
    ImmutableTableCursor, ImmutableTableReader, TableBuilderConfig, TableByteSource,
    TableCacheConfig, TableCompactionConfig, TableCursor, TableIdentity, TableInternalKeyBytes,
    TableKeyBound, TableKeyBounds, TablePhysicalKeyBytes, TableReaderConfig, TableRow,
    TableRuntimeConfig, TableRuntimeError, TableRuntimeResult,
};
use std::cell::Cell;
use std::error::Error as _;
use std::rc::Rc;
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
    calls: Rc<Cell<usize>>,
}

impl TestSource {
    fn exact(bytes: Vec<u8>) -> Self {
        Self {
            advertised_len: bytes.len() as u64,
            bytes,
            short_read: false,
            long_read: false,
            fail_read: false,
            calls: Rc::new(Cell::new(0)),
        }
    }

    fn short(bytes: Vec<u8>) -> Self {
        Self {
            advertised_len: bytes.len() as u64,
            bytes,
            short_read: true,
            long_read: false,
            fail_read: false,
            calls: Rc::new(Cell::new(0)),
        }
    }

    fn long(bytes: Vec<u8>) -> Self {
        Self {
            advertised_len: bytes.len() as u64,
            bytes,
            short_read: false,
            long_read: true,
            fail_read: false,
            calls: Rc::new(Cell::new(0)),
        }
    }

    fn failing(bytes: Vec<u8>) -> Self {
        Self {
            advertised_len: bytes.len() as u64,
            bytes,
            short_read: false,
            long_read: false,
            fail_read: true,
            calls: Rc::new(Cell::new(0)),
        }
    }

    fn with_advertised_len(bytes: Vec<u8>, advertised_len: u64) -> Self {
        Self {
            advertised_len,
            bytes,
            short_read: false,
            long_read: false,
            fail_read: false,
            calls: Rc::new(Cell::new(0)),
        }
    }

    fn calls(&self) -> usize {
        self.calls.get()
    }
}

impl TableByteSource for TestSource {
    fn byte_count(&self) -> u64 {
        self.advertised_len
    }

    fn read_at(&self, offset: u64, len: usize) -> TableRuntimeResult<Vec<u8>> {
        self.calls.set(self.calls.get().saturating_add(1));
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
    assert_eq!(reader.byte_count(), artifact.byte_count());
    assert_eq!(reader.rows(), table_rows.as_slice());

    for row in &table_rows {
        assert_eq!(reader.get_exact(row.key()), Some(row.clone()));
    }

    let missing = TableInternalKeyBytes::from_row(&put_row(b"missing".to_vec(), 99));
    assert_eq!(reader.get_exact(&missing), None);
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
    assert!(source_probe.calls() > 1);
    assert_eq!(reader.facts(), artifact.facts());
    assert_eq!(reader.rows(), rows.as_slice());

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
    let mut cursor: ImmutableTableCursor<'_> = reader.cursor();
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

fn bound_shape_reader() -> (ImmutableTableReader, Vec<TableRow>, PhysicalKey) {
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

    assert!(source_probe.calls() > 1);
    assert_eq!(byte_reader, source_reader);
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
