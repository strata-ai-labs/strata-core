use crate::backend::memory::MemoryBackend;
use crate::backend::Backend;
use crate::format::{decode_immutable_table, encode_internal_key, FormatError, TableCompression};
use crate::layout::ObjectLayout;
use crate::row::{InternalKey, PhysicalKey, StorageRow, StorageSpaceId};
use crate::service::TableObjectByteSource;
use crate::table::{
    sort_table_rows_by_key, validate_strictly_sorted_unique_rows, BoundedTableCursor,
    BuiltTableArtifact, BytesTableSource, CacheInsert, CursorMergePath, FrozenTable,
    ImmutableTableBuilder, ImmutableTableReader, KeepAllTableCompactionPolicy, MergeTableCursor,
    MutableTable, TableBlockAddress, TableBlockCache, TableBlockCacheKey, TableBlockCacheKind,
    TableBlockCacheStats, TableBloomFilter, TableBloomProbe, TableBuilderConfig, TableByteSource,
    TableCacheConfig, TableCacheTableId, TableCommitRange, TableCompactionConfig,
    TableCompactionDecision, TableCompactionDropReason, TableCompactionSource,
    TableCompactionSourceId, TableCompactor, TableCursor, TableIdentity, TableInternalKeyBytes,
    TableKeyBound, TableKeyBounds, TableKeyRange, TableMemoryFacts, TablePhysicalKeyBytes,
    TableReaderConfig, TableRow, TableRuntimeConfig, TableRuntimeError, TableRuntimeFacts,
    TableRuntimeResult, TableRuntimeStats, MERGE_HEAP_THRESHOLD,
};
use std::collections::{btree_map::Entry, BTreeMap, VecDeque};
use std::error::Error;
use std::sync::Arc;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

use super::TestkitError;

type CursorKeyValue = (Vec<u8>, Vec<u8>);
type MergeModelItem = (Vec<u8>, usize, Vec<u8>);

const GENERATED_COMPACTION_MAX_SOURCES: u8 = 16;
const GENERATED_COMPACTION_MAX_ROWS: usize = 4096;

/// Summary of one generated table-runtime scaffold contract check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TableRuntimeScaffoldOutcome {
    valid_config: usize,
    invalid_config: usize,
    valid_facts: usize,
    invalid_facts: usize,
    row_key_adapters: usize,
    invalid_row_key_sequences: usize,
    key_bounds: usize,
    size_accounting: usize,
    mutable_frozen_tables: usize,
    raw_cursors: usize,
    immutable_builder_artifacts: usize,
    immutable_table_readers: usize,
    object_backed_table_readers: usize,
    table_block_caches: usize,
    table_bloom_filters: usize,
    table_compactions: usize,
    error_sources: usize,
    stats: usize,
}

impl TableRuntimeScaffoldOutcome {
    /// Number of valid configuration cases exercised.
    pub const fn valid_config_cases(self) -> usize {
        self.valid_config
    }

    /// Number of invalid configuration cases exercised.
    pub const fn invalid_config_cases(self) -> usize {
        self.invalid_config
    }

    /// Number of valid table-fact cases exercised.
    pub const fn valid_fact_cases(self) -> usize {
        self.valid_facts
    }

    /// Number of invalid table-fact cases exercised.
    pub const fn invalid_fact_cases(self) -> usize {
        self.invalid_facts
    }

    /// Number of row/key adapter cases exercised.
    pub const fn row_key_adapter_cases(self) -> usize {
        self.row_key_adapters
    }

    /// Number of invalid row/key sequence cases exercised.
    pub const fn invalid_row_key_sequence_cases(self) -> usize {
        self.invalid_row_key_sequences
    }

    /// Number of key-bound cases exercised.
    pub const fn key_bound_cases(self) -> usize {
        self.key_bounds
    }

    /// Number of size-accounting cases exercised.
    pub const fn size_accounting_cases(self) -> usize {
        self.size_accounting
    }

    /// Number of mutable/frozen table cases exercised.
    pub const fn mutable_frozen_table_cases(self) -> usize {
        self.mutable_frozen_tables
    }

    /// Number of raw cursor and merge cursor cases exercised.
    pub const fn raw_cursor_cases(self) -> usize {
        self.raw_cursors
    }

    /// Number of immutable table builder artifact cases exercised.
    pub const fn immutable_builder_artifact_cases(self) -> usize {
        self.immutable_builder_artifacts
    }

    /// Number of immutable table reader cases exercised.
    pub const fn immutable_table_reader_cases(self) -> usize {
        self.immutable_table_readers
    }

    /// Number of object-backed immutable table reader cases exercised.
    pub const fn object_backed_table_reader_cases(self) -> usize {
        self.object_backed_table_readers
    }

    /// Number of table block-cache cases exercised.
    pub const fn table_block_cache_cases(self) -> usize {
        self.table_block_caches
    }

    /// Number of table bloom/filter accelerator cases exercised.
    pub const fn table_bloom_filter_cases(self) -> usize {
        self.table_bloom_filters
    }

    /// Number of generic table compaction cases exercised.
    pub const fn table_compaction_cases(self) -> usize {
        self.table_compactions
    }

    /// Number of error source-chain cases exercised.
    pub const fn error_source_cases(self) -> usize {
        self.error_sources
    }

    /// Number of stats construction cases exercised.
    pub const fn stats_cases(self) -> usize {
        self.stats
    }
}

/// Runs one deterministic generated scaffold contract case for the L5 table runtime.
///
/// The input is a bounded script. It is not durable table bytes; it exercises
/// deterministic table-runtime contracts that each L5 slice adds to this
/// single generated route instead of scattering unrelated property harnesses.
pub fn check_table_runtime_scaffold_contract(
    script: &[u8],
) -> Result<TableRuntimeScaffoldOutcome, TestkitError> {
    let mut outcome = TableRuntimeScaffoldOutcome {
        valid_config: 0,
        invalid_config: 0,
        valid_facts: 0,
        invalid_facts: 0,
        row_key_adapters: 0,
        invalid_row_key_sequences: 0,
        key_bounds: 0,
        size_accounting: 0,
        mutable_frozen_tables: 0,
        raw_cursors: 0,
        immutable_builder_artifacts: 0,
        immutable_table_readers: 0,
        object_backed_table_readers: 0,
        table_block_caches: 0,
        table_bloom_filters: 0,
        table_compactions: 0,
        error_sources: 0,
        stats: 0,
    };

    check_valid_config(script)?;
    outcome.valid_config += 1;
    outcome.invalid_config += check_invalid_configs()?;

    check_valid_facts(script)?;
    outcome.valid_facts += 1;
    outcome.invalid_facts += check_invalid_facts()?;

    check_row_key_adapters(script)?;
    outcome.row_key_adapters += 1;
    outcome.invalid_row_key_sequences += check_invalid_row_key_sequences(script)?;
    check_key_bounds(script)?;
    outcome.key_bounds += 1;
    check_size_accounting(script)?;
    outcome.size_accounting += 1;
    check_mutable_frozen_tables(script)?;
    outcome.mutable_frozen_tables += 1;
    check_raw_cursors(script)?;
    outcome.raw_cursors += 1;
    check_immutable_table_builder(script)?;
    outcome.immutable_builder_artifacts += 1;
    check_immutable_table_reader(script)?;
    outcome.immutable_table_readers += 1;
    check_object_backed_table_reader(script)?;
    outcome.object_backed_table_readers += 1;
    check_table_block_cache(script)?;
    outcome.table_block_caches += 1;
    check_table_bloom_filter(script)?;
    outcome.table_bloom_filters += 1;
    check_table_compaction(script)?;
    outcome.table_compactions += 1;

    check_error_source_chain()?;
    outcome.error_sources += 1;

    check_stats(script)?;
    outcome.stats += 1;

    Ok(outcome)
}

/// Runs the reader-specific fuzz contract over arbitrary table bytes.
pub fn check_table_runtime_reader_contract(bytes: &[u8]) -> Result<(), TestkitError> {
    let identity = TableIdentity::new("fuzz-reader")
        .map_err(|err| TestkitError::new(format!("reader identity setup failed: {err}")))?;
    match ImmutableTableReader::open_bytes(identity, bytes.to_vec(), TableReaderConfig::default()) {
        Ok(reader) => assert_reader_matches_decode("reader fuzz", &reader, bytes),
        Err(
            TableRuntimeError::DecodeFormat { .. }
            | TableRuntimeError::InvalidRowOrder { .. }
            | TableRuntimeError::DuplicateInternalKey { .. }
            | TableRuntimeError::InvalidRange { .. },
        ) => Ok(()),
        Err(err) => Err(TestkitError::new(format!(
            "reader fuzz returned unexpected error: {err}"
        ))),
    }
}

/// Runs the cursor-specific fuzz contract over generated valid table sources.
pub fn check_table_runtime_cursor_contract(script: &[u8]) -> Result<(), TestkitError> {
    let row_count = 1 + usize::from(script_byte(script, 0) % 32);
    let table = generated_cursor_table(script, 0, row_count)?;
    let model_keys = table
        .iter()
        .map(|row| row.key().clone())
        .collect::<Vec<_>>();
    let mut cursor = table.cursor();
    let mut expected_position: Option<usize> = None;

    for (step, op) in script.iter().copied().take(128).enumerate() {
        match op % 4 {
            0 => {
                cursor
                    .seek_to_first()
                    .map_err(|err| TestkitError::new(format!("cursor seek-first failed: {err}")))?;
                expected_position = (!model_keys.is_empty()).then_some(0);
            }
            1 => {
                cursor
                    .advance()
                    .map_err(|err| TestkitError::new(format!("cursor advance failed: {err}")))?;
                expected_position = expected_position.and_then(|position| {
                    let next = position.saturating_add(1);
                    (next < model_keys.len()).then_some(next)
                });
            }
            2 => {
                let target_index = usize::from(script_byte(script, step + 1)) % model_keys.len();
                let target = model_keys[target_index].clone();
                cursor
                    .seek(&target)
                    .map_err(|err| TestkitError::new(format!("cursor exact seek failed: {err}")))?;
                expected_position = Some(target_index);
            }
            _ => {
                let target = TableInternalKeyBytes::from_row(&generated_model_row(
                    script,
                    step.saturating_add(33),
                )?);
                cursor.seek(&target).map_err(|err| {
                    TestkitError::new(format!("cursor generated seek failed: {err}"))
                })?;
                expected_position = model_keys.iter().position(|key| key >= &target);
            }
        }

        let expected_key = expected_position.map(|position| &model_keys[position]);
        if cursor.current_key() != expected_key {
            return Err(TestkitError::new(
                "cursor fuzz current key did not match generated model",
            ));
        }
    }

    assert_bounded_cursors_match_model("cursor fuzz bounded", &table)?;
    let source_count = 1 + usize::from(script_byte(script, 2) % GENERATED_COMPACTION_MAX_SOURCES);
    let merge_tables = generated_cursor_tables(script, source_count, 2)?;
    let expected_path = match source_count {
        0 => CursorMergePath::Empty,
        1 => CursorMergePath::Single,
        2..=MERGE_HEAP_THRESHOLD => CursorMergePath::Linear,
        _ => CursorMergePath::Heap,
    };
    assert_merge_cursor_matches_model("cursor fuzz merge", &merge_tables, expected_path)
}

/// Runs the compaction-specific fuzz contract over generated sorted sources.
pub fn check_table_runtime_compaction_contract(script: &[u8]) -> Result<(), TestkitError> {
    check_table_compaction(script)
}

fn check_valid_config(script: &[u8]) -> Result<(), TestkitError> {
    let target_data_block_size = 1 + u32::from(script_byte(script, 0));
    let rows_per_block = 1 + usize::from(script_byte(script, 1));
    let compression = if script_byte(script, 2) & 1 == 0 {
        TableCompression::Uncompressed
    } else {
        TableCompression::Zstd
    };
    let cache_enabled = script_byte(script, 3) & 1 == 0;
    let cache_capacity = if cache_enabled {
        1 + usize::from(script_byte(script, 4))
    } else {
        usize::from(script_byte(script, 4))
    };
    let target_output_bytes = 1 + u64::from(script_byte(script, 5));
    let max_output_tables = 1 + usize::from(script_byte(script, 6));

    let builder = TableBuilderConfig::new(target_data_block_size, rows_per_block, compression)
        .map_err(|err| TestkitError::new(format!("valid builder config rejected: {err}")))?;
    let reader = TableReaderConfig::new();
    let cache = TableCacheConfig::new(cache_enabled, cache_capacity)
        .map_err(|err| TestkitError::new(format!("valid cache config rejected: {err}")))?;
    let compaction = TableCompactionConfig::new(target_output_bytes, max_output_tables)
        .map_err(|err| TestkitError::new(format!("valid compaction config rejected: {err}")))?;
    let runtime = TableRuntimeConfig::new(builder, reader, cache, compaction)
        .map_err(|err| TestkitError::new(format!("valid runtime config rejected: {err}")))?;

    if runtime.builder().target_data_block_size() != target_data_block_size
        || runtime.builder().rows_per_block() != rows_per_block
        || runtime.builder().compression() != compression
        || *runtime.reader() != TableReaderConfig::default()
        || runtime.cache().enabled() != cache_enabled
        || runtime.cache().capacity_bytes() != cache_capacity
        || runtime.compaction().target_output_bytes() != target_output_bytes
        || runtime.compaction().max_output_tables() != max_output_tables
    {
        return Err(TestkitError::new("valid runtime config facts drifted"));
    }

    Ok(())
}

fn check_invalid_configs() -> Result<usize, TestkitError> {
    let cases = [
        TableBuilderConfig::new(0, 1, TableCompression::Uncompressed),
        TableBuilderConfig::new(1, 0, TableCompression::Uncompressed),
    ];
    for case in cases {
        expect_invalid_config(case)?;
    }

    expect_invalid_config(TableCacheConfig::new(true, 0))?;
    expect_invalid_config(TableCompactionConfig::new(0, 1))?;
    expect_invalid_config(TableCompactionConfig::new(1, 0))?;
    Ok(5)
}

fn check_valid_facts(script: &[u8]) -> Result<(), TestkitError> {
    let row_count = 1 + u64::from(script_byte(script, 8) % 32);
    let data_block_count = 1 + u32::from(script_byte(script, 9) % u8::try_from(row_count).unwrap());
    let byte_count = 1 + u64::from(script_byte(script, 10));
    let commit_min = CommitVersion::new(u64::from(script_byte(script, 11)));
    let commit_max =
        CommitVersion::new(commit_min.as_u64() + u64::from(script_byte(script, 12) % 32));
    let identity = TableIdentity::new(format!("table-{:02x}", script_byte(script, 13)))
        .map_err(|err| TestkitError::new(format!("valid table identity rejected: {err}")))?;
    let first_key = vec![script_byte(script, 14)];
    let mut last_key = first_key.clone();
    last_key.push(script_byte(script, 15));
    let key_range = TableKeyRange::new(first_key.as_slice(), last_key.as_slice())
        .map_err(|err| TestkitError::new(format!("valid key range rejected: {err}")))?;
    let commit_range = TableCommitRange::new(commit_min, commit_max)
        .map_err(|err| TestkitError::new(format!("valid commit range rejected: {err}")))?;
    let facts = TableRuntimeFacts::new(
        identity.clone(),
        row_count,
        data_block_count,
        key_range,
        commit_range,
        byte_count,
    )
    .map_err(|err| TestkitError::new(format!("valid table facts rejected: {err}")))?;

    if facts.identity() != &identity
        || facts.row_count() != row_count
        || facts.data_block_count() != data_block_count
        || facts.key_range().first_key() != first_key.as_slice()
        || facts.key_range().last_key() != last_key.as_slice()
        || facts.commit_range().min() != commit_min
        || facts.commit_range().max() != commit_max
        || facts.byte_count() != byte_count
    {
        return Err(TestkitError::new("valid table facts drifted"));
    }

    Ok(())
}

fn check_invalid_facts() -> Result<usize, TestkitError> {
    expect_invalid_config(TableIdentity::new(""))?;
    expect_invalid_config(TableIdentity::new("table/1"))?;
    expect_invalid_range(TableKeyRange::new(Vec::<u8>::new(), vec![1]))?;
    expect_invalid_range(TableKeyRange::new(vec![2], vec![1]))?;
    expect_invalid_range(TableCommitRange::new(
        CommitVersion::new(2),
        CommitVersion::new(1),
    ))?;

    let identity = TableIdentity::new("table-01")
        .map_err(|err| TestkitError::new(format!("identity setup failed: {err}")))?;
    let key_range = TableKeyRange::new(vec![1], vec![2])
        .map_err(|err| TestkitError::new(format!("key range setup failed: {err}")))?;
    let commit_range = TableCommitRange::new(CommitVersion::new(1), CommitVersion::new(1))
        .map_err(|err| TestkitError::new(format!("commit range setup failed: {err}")))?;

    expect_invalid_range(TableRuntimeFacts::new(
        identity.clone(),
        0,
        1,
        key_range.clone(),
        commit_range,
        1,
    ))?;
    expect_invalid_range(TableRuntimeFacts::new(
        identity.clone(),
        1,
        0,
        key_range.clone(),
        commit_range,
        1,
    ))?;
    expect_invalid_range(TableRuntimeFacts::new(
        identity.clone(),
        1,
        2,
        key_range.clone(),
        commit_range,
        1,
    ))?;
    expect_invalid_range(TableRuntimeFacts::new(
        identity,
        1,
        1,
        key_range,
        commit_range,
        0,
    ))?;

    Ok(9)
}

fn check_error_source_chain() -> Result<(), TestkitError> {
    let source = FormatError::InvalidLength { field: "row_count" };
    let error = TableRuntimeError::BuildFormat {
        source: source.clone(),
    };
    if error.source().map(ToString::to_string) != Some(source.to_string()) {
        return Err(TestkitError::new(
            "table runtime error did not preserve format source",
        ));
    }
    Ok(())
}

fn check_stats(script: &[u8]) -> Result<(), TestkitError> {
    let rows = u64::from(script_byte(script, 16));
    let bytes = u64::from(script_byte(script, 17));
    let hits = u64::from(script_byte(script, 18));
    let misses = u64::from(script_byte(script, 19));
    let input = u64::from(script_byte(script, 20));
    let output = u64::from(script_byte(script, 21));
    let stats = TableRuntimeStats::new(rows, bytes, hits, misses, input, output);
    if stats.rows_read() != rows
        || stats.bytes_read() != bytes
        || stats.cache_hits() != hits
        || stats.cache_misses() != misses
        || stats.compaction_input_rows() != input
        || stats.compaction_output_rows() != output
    {
        return Err(TestkitError::new("table runtime stats drifted"));
    }
    Ok(())
}

fn check_row_key_adapters(script: &[u8]) -> Result<(), TestkitError> {
    let physical_key = generated_physical_key(script, 32, generated_user_key(script, 36))?;
    let row = StorageRow::put(
        physical_key.clone(),
        CommitVersion::new(10 + u64::from(script_byte(script, 40))),
        Timestamp::from_micros(u64::from(script_byte(script, 41))),
        Timestamp::from_micros(u64::from(script_byte(script, 42))),
        generated_value(script, 43),
    );
    let table_row = TableRow::new(row.clone());
    let key = TableInternalKeyBytes::from_row(&row);
    let decoded = key
        .decode()
        .map_err(|err| TestkitError::new(format!("generated key did not decode: {err}")))?;
    let prefix = TablePhysicalKeyBytes::from_physical_key(&physical_key);

    if table_row.row() != &row
        || table_row.key() != &key
        || table_row.encoded_key() != key.as_slice()
        || table_row.physical_key() != &physical_key
        || table_row.commit_version() != row.commit_version()
        || table_row.commit_timestamp() != row.commit_timestamp()
        || table_row.expires_at() != row.expires_at()
        || table_row.value() != row.value()
        || table_row.is_tombstone()
        || decoded.physical_key() != &physical_key
        || decoded.commit_version() != row.commit_version()
        || !prefix.is_prefix_of(&key)
    {
        return Err(TestkitError::new("generated row adapter facts drifted"));
    }

    let tombstone = TableRow::new(StorageRow::tombstone(
        physical_key.clone(),
        CommitVersion::new(row.commit_version().as_u64() + 1),
        Timestamp::from_micros(u64::from(script_byte(script, 44))),
    ));
    if !tombstone.is_tombstone() || !tombstone.value().is_empty() {
        return Err(TestkitError::new("generated tombstone facts drifted"));
    }

    let newer = TableRow::new(StorageRow::put(
        physical_key.clone(),
        CommitVersion::new(99),
        Timestamp::from_micros(1),
        Timestamp::EPOCH,
        Vec::new(),
    ));
    let older = TableRow::new(StorageRow::put(
        physical_key,
        CommitVersion::new(1),
        Timestamp::from_micros(2),
        Timestamp::EPOCH,
        Vec::new(),
    ));
    if newer.key() >= older.key() {
        return Err(TestkitError::new(
            "same physical key did not sort newest commit first",
        ));
    }

    check_generated_row_key_model(script)?;

    Ok(())
}

fn check_generated_row_key_model(script: &[u8]) -> Result<(), TestkitError> {
    let row_count = 1 + usize::from(script_byte(script, 96) % 64);
    let rows = (0..row_count)
        .map(|index| generated_model_row(script, index))
        .collect::<Result<Vec<_>, _>>()?;
    let mut adapted = rows.iter().cloned().map(TableRow::new).collect::<Vec<_>>();
    let mut expected_keys = rows
        .iter()
        .map(|row| {
            encode_internal_key(&InternalKey::new(
                row.physical_key().clone(),
                row.commit_version(),
            ))
        })
        .collect::<Vec<_>>();

    sort_table_rows_by_key(&mut adapted);
    expected_keys.sort();

    let actual_keys = adapted
        .iter()
        .map(|row| row.encoded_key().to_vec())
        .collect::<Vec<_>>();
    if actual_keys != expected_keys {
        return Err(TestkitError::new(
            "generated row ordering did not match V1 key oracle",
        ));
    }
    validate_strictly_sorted_unique_rows(&adapted).map_err(|err| {
        TestkitError::new(format!(
            "generated sorted unique row validation failed: {err}"
        ))
    })?;

    for table_row in adapted {
        let row = table_row.row();
        if table_row.commit_timestamp() != row.commit_timestamp()
            || table_row.expires_at() != row.expires_at()
            || table_row.value() != row.value()
            || table_row.is_tombstone() != row.is_tombstone()
        {
            return Err(TestkitError::new(
                "generated row metadata did not survive adaptation",
            ));
        }
    }

    Ok(())
}

fn check_invalid_row_key_sequences(script: &[u8]) -> Result<usize, TestkitError> {
    let physical_key = generated_physical_key(script, 48, generated_user_key(script, 52))?;
    let newer = TableRow::new(StorageRow::put(
        physical_key.clone(),
        CommitVersion::new(7),
        Timestamp::from_micros(1),
        Timestamp::EPOCH,
        Vec::new(),
    ));
    let older = TableRow::new(StorageRow::put(
        physical_key.clone(),
        CommitVersion::new(3),
        Timestamp::from_micros(2),
        Timestamp::EPOCH,
        Vec::new(),
    ));
    let mut sorted = vec![older.clone(), newer.clone()];
    sort_table_rows_by_key(&mut sorted);
    validate_strictly_sorted_unique_rows(&sorted)
        .map_err(|err| TestkitError::new(format!("distinct versions rejected: {err}")))?;

    match validate_strictly_sorted_unique_rows(&[older, newer.clone()]) {
        Err(TableRuntimeError::InvalidRowOrder { .. }) => {}
        Err(err) => {
            return Err(TestkitError::new(format!(
                "expected invalid row order; got {err}"
            )));
        }
        Ok(()) => return Err(TestkitError::new("unsorted generated rows were accepted")),
    }

    let duplicate = TableRow::new(StorageRow::put(
        physical_key,
        CommitVersion::new(7),
        Timestamp::from_micros(3),
        Timestamp::EPOCH,
        b"different".to_vec(),
    ));
    match validate_strictly_sorted_unique_rows(&[newer, duplicate]) {
        Err(TableRuntimeError::DuplicateInternalKey { .. }) => {}
        Err(err) => {
            return Err(TestkitError::new(format!(
                "expected duplicate internal key; got {err}"
            )));
        }
        Ok(()) => return Err(TestkitError::new("duplicate generated rows were accepted")),
    }

    Ok(3)
}

fn check_key_bounds(script: &[u8]) -> Result<(), TestkitError> {
    let row_count = 3 + usize::from(script_byte(script, 64) % 16);
    let mut rows = (0..row_count)
        .map(|index| generated_model_row(script, index))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(TableRow::new)
        .collect::<Vec<_>>();
    sort_table_rows_by_key(&mut rows);
    let first = rows[0].key().clone();
    let middle = rows[rows.len() / 2].key().clone();
    let last = rows[rows.len() - 1].key().clone();

    let unbounded = TableKeyBounds::unbounded();
    assert_bounds_match_filter("unbounded", &unbounded, &rows, |_| true)?;

    let exact = TableKeyBounds::exact(middle.clone());
    assert_bounds_match_filter("exact", &exact, &rows, |key| key == &middle)?;

    let closed = TableKeyBounds::closed(first.clone(), middle.clone())
        .map_err(|err| TestkitError::new(format!("valid closed range rejected: {err}")))?;
    assert_bounds_match_filter("closed", &closed, &rows, |key| {
        key >= &first && key <= &middle
    })?;

    let open = TableKeyBounds::open(first.clone(), last.clone())
        .map_err(|err| TestkitError::new(format!("valid open range rejected: {err}")))?;
    assert_bounds_match_filter("open", &open, &rows, |key| key > &first && key < &last)?;

    let lower_unbounded = TableKeyBounds::range(
        TableKeyBound::Unbounded,
        TableKeyBound::included(middle.clone()),
    )
    .map_err(|err| TestkitError::new(format!("valid lower-unbounded range rejected: {err}")))?;
    assert_bounds_match_filter("lower-unbounded", &lower_unbounded, &rows, |key| {
        key <= &middle
    })?;

    let upper_unbounded = TableKeyBounds::range(
        TableKeyBound::excluded(middle.clone()),
        TableKeyBound::Unbounded,
    )
    .map_err(|err| TestkitError::new(format!("valid upper-unbounded range rejected: {err}")))?;
    assert_bounds_match_filter("upper-unbounded", &upper_unbounded, &rows, |key| {
        key > &middle
    })?;

    let prefix_row_index = usize::from(script_byte(script, 65)) % rows.len();
    let prefix = TablePhysicalKeyBytes::from_row(rows[prefix_row_index].row());
    let prefix_bytes = prefix.as_slice().to_vec();
    let prefix_bounds = TableKeyBounds::prefix(prefix.as_slice().to_vec());
    assert_bounds_match_filter("prefix", &prefix_bounds, &rows, |key| {
        key.as_slice().starts_with(&prefix_bytes)
    })?;

    if !matches!(
        TableKeyBounds::closed(last.clone(), first.clone()),
        Err(TableRuntimeError::InvalidRange { .. })
    ) {
        return Err(TestkitError::new("invalid key bounds were accepted"));
    }

    Ok(())
}

fn assert_bounds_match_filter(
    label: &'static str,
    bounds: &TableKeyBounds,
    rows: &[TableRow],
    model: impl Fn(&TableInternalKeyBytes) -> bool,
) -> Result<(), TestkitError> {
    let actual = rows
        .iter()
        .map(|row| bounds.contains_key(row.key()))
        .collect::<Vec<_>>();
    let expected = rows.iter().map(|row| model(row.key())).collect::<Vec<_>>();
    if actual != expected {
        return Err(TestkitError::new(format!(
            "{label} key bounds did not match generated model"
        )));
    }
    Ok(())
}

fn check_size_accounting(script: &[u8]) -> Result<(), TestkitError> {
    let key = generated_physical_key(script, 80, b"size".to_vec())?;
    let small = TableRow::new(StorageRow::put(
        key.clone(),
        CommitVersion::new(1),
        Timestamp::from_micros(1),
        Timestamp::EPOCH,
        vec![1],
    ));
    let larger_value = TableRow::new(StorageRow::put(
        key,
        CommitVersion::new(1),
        Timestamp::from_micros(1),
        Timestamp::EPOCH,
        vec![1; 16],
    ));
    let longer_key = TableRow::new(StorageRow::put(
        generated_physical_key(script, 80, b"size-with-longer-key".to_vec())?,
        CommitVersion::new(1),
        Timestamp::from_micros(1),
        Timestamp::EPOCH,
        vec![1],
    ));
    let tombstone = TableRow::new(StorageRow::tombstone(
        generated_physical_key(script, 80, b"size-tombstone".to_vec())?,
        CommitVersion::new(1),
        Timestamp::from_micros(1),
    ));

    if small.approximate_size_bytes() == 0
        || tombstone.approximate_size_bytes() == 0
        || larger_value.approximate_size_bytes() <= small.approximate_size_bytes()
        || longer_key.approximate_size_bytes() <= small.approximate_size_bytes()
        || TableRow::new(small.row().clone()).approximate_size_bytes()
            != small.approximate_size_bytes()
    {
        return Err(TestkitError::new("generated size accounting drifted"));
    }

    Ok(())
}

fn check_mutable_frozen_tables(script: &[u8]) -> Result<(), TestkitError> {
    let mut table = MutableTable::new();
    let mut model = BTreeMap::<Vec<u8>, StorageRow>::new();
    assert_memory_facts_match_model("empty mutable", &table.facts(), &model, 0)?;

    let first_row = generated_model_row(script, 0)?;
    let related_row = StorageRow::put(
        first_row.physical_key().clone(),
        CommitVersion::new(first_row.commit_version().as_u64().saturating_add(1)),
        Timestamp::from_micros(first_row.commit_timestamp().as_micros().saturating_add(1)),
        Timestamp::EPOCH,
        generated_value(script, 224),
    );

    let generated_count = usize::from(script_byte(script, 240) % 121);
    let mut rows = deterministic_memory_table_edge_rows()?;
    rows.push(first_row.clone());
    rows.push(related_row);
    for index in 1..=generated_count {
        rows.push(generated_model_row(script, index)?);
    }
    let expected_bytes = insert_rows_into_memory_table(&mut table, &mut model, rows)?;

    let before_duplicate_facts = table.facts();
    let before_duplicate_keys = table.iter().map(table_row_key_bytes).collect::<Vec<_>>();
    match table.insert_row(first_row) {
        Err(TableRuntimeError::DuplicateInternalKey { .. }) => {}
        Err(err) => {
            return Err(TestkitError::new(format!(
                "expected forced duplicate key error; got {err}"
            )));
        }
        Ok(()) => return Err(TestkitError::new("forced duplicate key insert succeeded")),
    }
    if table.facts() != before_duplicate_facts
        || table.iter().map(table_row_key_bytes).collect::<Vec<_>>() != before_duplicate_keys
    {
        return Err(TestkitError::new(
            "duplicate insert mutated generated mutable table",
        ));
    }

    assert_memory_table_matches_model("mutable", table.iter(), &model, expected_bytes)?;
    assert_memory_facts_match_model("mutable", &table.facts(), &model, expected_bytes)?;
    assert_sampled_table_reads_match_model(&table, &model)?;

    let frozen = table.freeze();
    assert_frozen_table_matches_model(&frozen, &model, expected_bytes)?;

    Ok(())
}

fn deterministic_memory_table_edge_rows() -> Result<Vec<StorageRow>, TestkitError> {
    let edge_physical =
        deterministic_physical_key(0xe1, "edge", 0x20, b"edge\0physical\0key".to_vec())?;
    let edge_empty_value = StorageRow::put(
        edge_physical.clone(),
        CommitVersion::new(10),
        Timestamp::from_micros(10),
        Timestamp::EPOCH,
        Vec::new(),
    );

    Ok(vec![
        edge_empty_value.clone(),
        edge_empty_value,
        StorageRow::put(
            edge_physical,
            CommitVersion::new(3),
            Timestamp::from_micros(3),
            Timestamp::EPOCH,
            b"lower".to_vec(),
        ),
        StorageRow::tombstone(
            deterministic_physical_key(0xe2, "edge", 0x20, b"edge-tombstone".to_vec())?,
            CommitVersion::new(4),
            Timestamp::from_micros(4),
        ),
        StorageRow::put(
            deterministic_physical_key(0xe3, "edge", 0x20, b"edge-expired".to_vec())?,
            CommitVersion::new(5),
            Timestamp::from_micros(5),
            Timestamp::from_micros(1),
            b"expired".to_vec(),
        ),
        StorageRow::put(
            deterministic_physical_key(0xe4, "edge", 0x01, b"edge-storage-owned".to_vec())?,
            CommitVersion::new(6),
            Timestamp::from_micros(6),
            Timestamp::EPOCH,
            b"storage-owned".to_vec(),
        ),
    ])
}

fn insert_rows_into_memory_table(
    table: &mut MutableTable,
    model: &mut BTreeMap<Vec<u8>, StorageRow>,
    rows: Vec<StorageRow>,
) -> Result<usize, TestkitError> {
    let mut expected_bytes = 0usize;
    for row in rows {
        let key = table_key_bytes(&row);
        let result = table.insert_row(row.clone());
        match model.entry(key) {
            Entry::Occupied(_) => expect_duplicate_insert_result(result)?,
            Entry::Vacant(entry) => {
                result.map_err(|err| {
                    TestkitError::new(format!("generated unique row insert failed: {err}"))
                })?;
                expected_bytes = expected_bytes
                    .saturating_add(TableRow::new(row.clone()).approximate_size_bytes());
                entry.insert(row);
            }
        }
    }
    Ok(expected_bytes)
}

fn expect_duplicate_insert_result(
    result: Result<(), TableRuntimeError>,
) -> Result<(), TestkitError> {
    match result {
        Err(TableRuntimeError::DuplicateInternalKey { .. }) => Ok(()),
        Err(err) => Err(TestkitError::new(format!(
            "expected generated duplicate key error; got {err}"
        ))),
        Ok(()) => Err(TestkitError::new(
            "generated duplicate key insert succeeded",
        )),
    }
}

fn assert_sampled_table_reads_match_model(
    table: &MutableTable,
    model: &BTreeMap<Vec<u8>, StorageRow>,
) -> Result<(), TestkitError> {
    let first_key = model
        .keys()
        .next()
        .ok_or_else(|| TestkitError::new("generated mutable model was empty"))?;
    let first_table_key = TableInternalKeyBytes::from_canonical_bytes(first_key.clone())
        .map_err(|err| TestkitError::new(format!("model first key was not canonical: {err}")))?;
    if table.get(&first_table_key).map(TableRow::row) != model.get(first_key) {
        return Err(TestkitError::new("mutable exact lookup drifted"));
    }
    let absent_key = absent_internal_key_for_model(&first_table_key, model)?;
    if table.get(&absent_key).is_some() {
        return Err(TestkitError::new("mutable absent exact lookup hit a row"));
    }

    let bounds = TableKeyBounds::exact(first_table_key.clone());
    let bounded = table
        .rows_in_bounds(&bounds)
        .map(table_row_key_bytes)
        .collect::<Vec<_>>();
    if bounded != vec![first_key.clone()] {
        return Err(TestkitError::new("mutable exact bounds drifted"));
    }
    if table
        .rows_in_bounds(&TableKeyBounds::exact(absent_key))
        .next()
        .is_some()
    {
        return Err(TestkitError::new("mutable absent exact bounds hit a row"));
    }
    let (range, expected_range) = sample_closed_range_from_model(model)?;
    let actual_range = table
        .rows_in_bounds(&range)
        .map(table_row_key_bytes)
        .collect::<Vec<_>>();
    if actual_range != expected_range {
        return Err(TestkitError::new("mutable range bounds drifted"));
    }

    let prefix = TablePhysicalKeyBytes::from_row(table.get(&first_table_key).expect("first").row());
    let prefix_bytes = prefix.as_slice().to_vec();
    let actual_prefix = table
        .rows_with_physical_prefix(&prefix)
        .map(table_row_key_bytes)
        .collect::<Vec<_>>();
    let expected_prefix = model
        .keys()
        .filter(|key| key.starts_with(&prefix_bytes))
        .cloned()
        .collect::<Vec<_>>();
    if actual_prefix != expected_prefix {
        return Err(TestkitError::new("mutable physical-prefix lookup drifted"));
    }

    Ok(())
}

fn assert_frozen_table_matches_model(
    table: &FrozenTable,
    model: &BTreeMap<Vec<u8>, StorageRow>,
    expected_bytes: usize,
) -> Result<(), TestkitError> {
    assert_memory_table_matches_model("frozen", table.iter(), model, expected_bytes)?;
    assert_memory_facts_match_model("frozen", &table.facts(), model, expected_bytes)?;

    let first_key = model
        .keys()
        .next()
        .ok_or_else(|| TestkitError::new("generated frozen model was empty"))?;
    let first_table_key = TableInternalKeyBytes::from_canonical_bytes(first_key.clone())
        .map_err(|err| TestkitError::new(format!("model first key was not canonical: {err}")))?;
    if table.get(&first_table_key).map(TableRow::row) != model.get(first_key) {
        return Err(TestkitError::new("frozen exact lookup drifted"));
    }
    let absent_key = absent_internal_key_for_model(&first_table_key, model)?;
    if table.get(&absent_key).is_some() {
        return Err(TestkitError::new("frozen absent exact lookup hit a row"));
    }
    if table
        .rows_in_bounds(&TableKeyBounds::exact(first_table_key))
        .map(table_row_key_bytes)
        .collect::<Vec<_>>()
        != vec![first_key.clone()]
    {
        return Err(TestkitError::new("frozen exact bounds drifted"));
    }
    if table
        .rows_in_bounds(&TableKeyBounds::exact(absent_key))
        .next()
        .is_some()
    {
        return Err(TestkitError::new("frozen absent exact bounds hit a row"));
    }
    let (range, expected_range) = sample_closed_range_from_model(model)?;
    if table
        .rows_in_bounds(&range)
        .map(table_row_key_bytes)
        .collect::<Vec<_>>()
        != expected_range
    {
        return Err(TestkitError::new("frozen range bounds drifted"));
    }

    let prefix = TablePhysicalKeyBytes::from_row(table.iter().next().expect("first row").row());
    let prefix_bytes = prefix.as_slice().to_vec();
    let actual_prefix = table
        .rows_with_physical_prefix(&prefix)
        .map(table_row_key_bytes)
        .collect::<Vec<_>>();
    let expected_prefix = model
        .keys()
        .filter(|key| key.starts_with(&prefix_bytes))
        .cloned()
        .collect::<Vec<_>>();
    if actual_prefix != expected_prefix {
        return Err(TestkitError::new("frozen physical-prefix lookup drifted"));
    }
    Ok(())
}

fn assert_memory_table_matches_model<'a>(
    label: &'static str,
    rows: impl Iterator<Item = &'a TableRow>,
    model: &BTreeMap<Vec<u8>, StorageRow>,
    expected_bytes: usize,
) -> Result<(), TestkitError> {
    let rows = rows.collect::<Vec<_>>();
    let actual_keys = rows
        .iter()
        .map(|row| table_row_key_bytes(row))
        .collect::<Vec<_>>();
    let expected_keys = model.keys().cloned().collect::<Vec<_>>();
    if actual_keys != expected_keys {
        return Err(TestkitError::new(format!(
            "{label} table iteration did not match generated model"
        )));
    }
    for row in &rows {
        let key = table_row_key_bytes(row);
        if model.get(&key) != Some(row.row()) {
            return Err(TestkitError::new(format!(
                "{label} table row payload did not match generated model"
            )));
        }
    }
    let actual_bytes = rows.iter().fold(0usize, |total, row| {
        total.saturating_add(row.approximate_size_bytes())
    });
    if actual_bytes != expected_bytes {
        return Err(TestkitError::new(format!(
            "{label} table byte accounting drifted"
        )));
    }
    Ok(())
}

fn assert_memory_facts_match_model(
    label: &'static str,
    facts: &TableMemoryFacts,
    model: &BTreeMap<Vec<u8>, StorageRow>,
    expected_bytes: usize,
) -> Result<(), TestkitError> {
    if facts.row_count() != model.len()
        || facts.approximate_size_bytes() != expected_bytes
        || facts.first_key() != model.keys().next().map(Vec::as_slice)
        || facts.last_key() != model.keys().next_back().map(Vec::as_slice)
    {
        return Err(TestkitError::new(format!(
            "{label} table facts did not match generated model"
        )));
    }

    let expected_min = model.values().map(StorageRow::commit_version).min();
    let expected_max = model.values().map(StorageRow::commit_version).max();
    if facts.min_commit() != expected_min || facts.max_commit() != expected_max {
        return Err(TestkitError::new(format!(
            "{label} table commit facts drifted"
        )));
    }
    Ok(())
}

fn check_raw_cursors(script: &[u8]) -> Result<(), TestkitError> {
    let table = generated_cursor_table(script, 0, 16)?;
    assert_memory_cursors_match_model("generated memory cursor", &table)?;
    assert_bounded_cursors_match_model("generated bounded cursor", &table)?;

    let empty = MergeTableCursor::new(Vec::new());
    if empty.path() != CursorMergePath::Empty {
        return Err(TestkitError::new("empty merge selected the wrong path"));
    }

    let single_empty_tables = vec![MutableTable::new()];
    assert_merge_cursor_matches_model(
        "single empty merge",
        &single_empty_tables,
        CursorMergePath::Single,
    )?;

    let single_tables = vec![generated_cursor_table(script, 1, 8)?];
    assert_merge_cursor_matches_model("single merge", &single_tables, CursorMergePath::Single)?;

    let mixed_linear_tables = vec![
        MutableTable::new(),
        generated_cursor_table(script, 2, 0)?,
        MutableTable::new(),
    ];
    assert_merge_cursor_matches_model(
        "mixed linear merge",
        &mixed_linear_tables,
        CursorMergePath::Linear,
    )?;

    let linear_tables = generated_cursor_tables(script, MERGE_HEAP_THRESHOLD, 4)?;
    assert_merge_cursor_matches_model("linear merge", &linear_tables, CursorMergePath::Linear)?;

    let mut mixed_heap_tables = generated_cursor_tables(script, MERGE_HEAP_THRESHOLD + 1, 2)?;
    mixed_heap_tables[1] = MutableTable::new();
    mixed_heap_tables[3] = MutableTable::new();
    assert_merge_cursor_matches_model(
        "mixed heap merge",
        &mixed_heap_tables,
        CursorMergePath::Heap,
    )?;

    let heap_tables = generated_cursor_tables(script, MERGE_HEAP_THRESHOLD + 1, 4)?;
    assert_merge_cursor_matches_model("heap merge", &heap_tables, CursorMergePath::Heap)?;

    let stress_heap_tables = generated_cursor_tables(script, 16, 2)?;
    assert_merge_cursor_matches_model(
        "sixteen-source heap merge",
        &stress_heap_tables,
        CursorMergePath::Heap,
    )?;

    Ok(())
}

fn generated_cursor_tables(
    script: &[u8],
    source_count: usize,
    generated_rows_per_source: usize,
) -> Result<Vec<MutableTable>, TestkitError> {
    (0..source_count)
        .map(|source_index| generated_cursor_table(script, source_index, generated_rows_per_source))
        .collect()
}

fn generated_cursor_table(
    script: &[u8],
    source_index: usize,
    generated_row_count: usize,
) -> Result<MutableTable, TestkitError> {
    let mut table = MutableTable::new();
    let shared_key = deterministic_physical_key(0xc1, "cursor", 0x20, b"shared".to_vec())?;
    table
        .insert_row(StorageRow::put(
            shared_key,
            CommitVersion::new(99),
            Timestamp::from_micros(99),
            Timestamp::EPOCH,
            vec![u8::try_from(source_index % 256).expect("bounded source index")],
        ))
        .map_err(|err| TestkitError::new(format!("shared cursor row insert failed: {err}")))?;

    for row_index in 0..generated_row_count {
        table
            .insert_row(generated_cursor_row(script, source_index, row_index)?)
            .map_err(|err| TestkitError::new(format!("cursor row insert failed: {err}")))?;
    }
    Ok(table)
}

fn generated_cursor_row(
    script: &[u8],
    source_index: usize,
    row_index: usize,
) -> Result<StorageRow, TestkitError> {
    let mut user_key = vec![
        script_byte(script, 224 + (row_index % 16)),
        u8::try_from(source_index % 256).expect("bounded source index"),
        u8::try_from(row_index % 256).expect("bounded row index"),
    ];
    if row_index % 3 == 0 {
        user_key.push(0x00);
    }
    if row_index % 5 == 0 {
        user_key.extend_from_slice(&[0x00, 0xff]);
    }
    let raw_space_id = 0x20 + u8::try_from((source_index + row_index) % 16).unwrap_or(0);
    let branch_byte = script_byte(script, 240 + (source_index % 16))
        .wrapping_add(u8::try_from(row_index % 16).expect("bounded row index"));
    let physical_key = deterministic_physical_key(branch_byte, "cursor", raw_space_id, user_key)?;
    let version = CommitVersion::new(
        1 + u64::from(script_byte(script, 192 + ((source_index + row_index) % 16))),
    );
    let timestamp = Timestamp::from_micros(
        u64::from(script_byte(script, 208 + ((source_index + row_index) % 16)))
            + u64::try_from(row_index).unwrap_or(0),
    );

    if (source_index + row_index) % 7 == 0 {
        Ok(StorageRow::tombstone(physical_key, version, timestamp))
    } else {
        let expires_at = if (source_index + row_index) % 5 == 0 {
            Timestamp::from_micros(1)
        } else {
            Timestamp::EPOCH
        };
        Ok(StorageRow::put(
            physical_key,
            version,
            timestamp,
            expires_at,
            generated_value(script, 208 + (row_index % 16)),
        ))
    }
}

fn assert_memory_cursors_match_model(
    label: &'static str,
    table: &MutableTable,
) -> Result<(), TestkitError> {
    let expected = table.iter().map(table_row_key_bytes).collect::<Vec<_>>();

    let mut mutable_cursor = table.cursor();
    mutable_cursor
        .seek_to_first()
        .map_err(|err| TestkitError::new(format!("{label} mutable seek failed: {err}")))?;
    assert_cursor_keys(label, &mut mutable_cursor, &expected)?;

    let frozen = table.clone().freeze();
    let mut frozen_cursor = frozen.cursor();
    frozen_cursor
        .seek_to_first()
        .map_err(|err| TestkitError::new(format!("{label} frozen seek failed: {err}")))?;
    assert_cursor_keys(label, &mut frozen_cursor, &expected)?;

    let model_rows = table.iter().collect::<Vec<_>>();
    if let Some(first) = model_rows.first() {
        let targets = [
            first.key().clone(),
            model_rows[model_rows.len() / 2].key().clone(),
            TableInternalKeyBytes::from_row(&StorageRow::put(
                deterministic_physical_key(0xff, "cursor", 0x20, b"after".to_vec())?,
                CommitVersion::new(1),
                Timestamp::from_micros(1),
                Timestamp::EPOCH,
                Vec::new(),
            )),
        ];
        for target in targets {
            let expected_key = model_rows
                .iter()
                .find(|row| row.key() >= &target)
                .map(|row| row.key().clone());
            let mut cursor = table.cursor();
            cursor
                .seek(&target)
                .map_err(|err| TestkitError::new(format!("{label} seek failed: {err}")))?;
            if cursor.current_key().cloned() != expected_key {
                return Err(TestkitError::new(format!(
                    "{label} seek did not match generated model"
                )));
            }
        }
    }

    Ok(())
}

fn assert_bounded_cursors_match_model(
    label: &'static str,
    table: &MutableTable,
) -> Result<(), TestkitError> {
    let rows = table.iter().collect::<Vec<_>>();
    if rows.is_empty() {
        return Ok(());
    }

    let middle = rows[rows.len() / 2].key().clone();
    let exact = TableKeyBounds::exact(middle);
    assert_bounded_cursor_matches_model(label, table, exact)?;

    let lower = rows[rows.len() / 3].key().clone();
    let upper = rows[(rows.len() * 2) / 3].key().clone();
    let closed = TableKeyBounds::closed(lower, upper)
        .map_err(|err| TestkitError::new(format!("{label} range setup failed: {err}")))?;
    assert_bounded_cursor_matches_model(label, table, closed)?;

    let prefix = TablePhysicalKeyBytes::from_row(rows[0].row());
    assert_bounded_cursor_matches_model(
        label,
        table,
        TableKeyBounds::prefix(prefix.as_slice().to_vec()),
    )?;

    Ok(())
}

fn assert_bounded_cursor_matches_model(
    label: &'static str,
    table: &MutableTable,
    bounds: TableKeyBounds,
) -> Result<(), TestkitError> {
    let expected = table
        .iter()
        .filter(|row| bounds.contains_key(row.key()))
        .map(table_row_key_bytes)
        .collect::<Vec<_>>();
    let mut cursor = BoundedTableCursor::new(Box::new(table.cursor()), bounds);
    cursor
        .seek_to_first()
        .map_err(|err| TestkitError::new(format!("{label} bounded seek failed: {err}")))?;
    assert_cursor_keys(label, &mut cursor, &expected)
}

fn assert_merge_cursor_matches_model(
    label: &'static str,
    tables: &[MutableTable],
    expected_path: CursorMergePath,
) -> Result<(), TestkitError> {
    let mut expected = Vec::<MergeModelItem>::new();
    for (source_index, table) in tables.iter().enumerate() {
        for row in table.iter() {
            expected.push((
                row.encoded_key().to_vec(),
                source_index,
                row.value().to_vec(),
            ));
        }
    }
    expected.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

    let mut merge = MergeTableCursor::new(boxed_table_cursors(tables));
    if merge.path() != expected_path {
        return Err(TestkitError::new(format!(
            "{label} selected wrong merge path"
        )));
    }
    merge
        .seek_to_first()
        .map_err(|err| TestkitError::new(format!("{label} seek failed: {err}")))?;
    let actual = collect_cursor_key_values(&mut merge)?;
    let expected = expected
        .into_iter()
        .map(|(key, _, value)| (key, value))
        .collect::<Vec<_>>();
    if actual != expected {
        return Err(TestkitError::new(format!(
            "{label} output did not match generated merge model"
        )));
    }

    if let Some((seek_key, _)) = expected.get(expected.len() / 2) {
        let seek_key = TableInternalKeyBytes::from_canonical_bytes(seek_key.clone())
            .map_err(|err| TestkitError::new(format!("{label} seek key decode failed: {err}")))?;
        merge
            .seek(&seek_key)
            .map_err(|err| TestkitError::new(format!("{label} re-seek failed: {err}")))?;
        let actual_after_seek = collect_cursor_key_values(&mut merge)?;
        let expected_after_seek = expected
            .iter()
            .filter(|(key, _)| key.as_slice() >= seek_key.as_slice())
            .cloned()
            .collect::<Vec<_>>();
        if actual_after_seek != expected_after_seek {
            return Err(TestkitError::new(format!(
                "{label} re-seek output did not match generated model"
            )));
        }
    }

    Ok(())
}

fn boxed_table_cursors<'a>(tables: &'a [MutableTable]) -> Vec<Box<dyn TableCursor + 'a>> {
    tables
        .iter()
        .map(|table| Box::new(table.cursor()) as Box<dyn TableCursor + 'a>)
        .collect()
}

fn assert_cursor_keys(
    label: &'static str,
    cursor: &mut impl TableCursor,
    expected: &[Vec<u8>],
) -> Result<(), TestkitError> {
    let actual = collect_cursor_keys(cursor)?;
    if actual != expected {
        return Err(TestkitError::new(format!(
            "{label} cursor keys did not match generated model"
        )));
    }
    Ok(())
}

fn collect_cursor_keys(cursor: &mut impl TableCursor) -> Result<Vec<Vec<u8>>, TestkitError> {
    let mut keys = Vec::new();
    while let Some(row) = cursor.current() {
        keys.push(row.encoded_key().to_vec());
        cursor
            .advance()
            .map_err(|err| TestkitError::new(format!("cursor advance failed: {err}")))?;
    }
    Ok(keys)
}

fn collect_cursor_key_values(
    cursor: &mut impl TableCursor,
) -> Result<Vec<CursorKeyValue>, TestkitError> {
    let mut rows = Vec::new();
    while let Some(row) = cursor.current() {
        rows.push((row.encoded_key().to_vec(), row.value().to_vec()));
        cursor
            .advance()
            .map_err(|err| TestkitError::new(format!("cursor advance failed: {err}")))?;
    }
    Ok(rows)
}

fn check_immutable_table_builder(script: &[u8]) -> Result<(), TestkitError> {
    let rows = generated_builder_table_rows(script)?;
    let expected_rows = rows.iter().map(|row| row.row().clone()).collect::<Vec<_>>();
    let target_data_block_size = 1 + u32::from(script_byte(script, 24));
    let rows_per_block = 1 + usize::from(script_byte(script, 25) % 8);
    let builder = ImmutableTableBuilder::new(
        TableBuilderConfig::new(
            target_data_block_size,
            rows_per_block,
            TableCompression::Uncompressed,
        )
        .map_err(|err| TestkitError::new(format!("builder config setup failed: {err}")))?,
    )
    .map_err(|err| TestkitError::new(format!("builder setup failed: {err}")))?;
    let identity = TableIdentity::new(format!("builder-{:02x}", script_byte(script, 26)))
        .map_err(|err| TestkitError::new(format!("builder identity setup failed: {err}")))?;

    let artifact = builder
        .build_from_rows(identity.clone(), &rows)
        .map_err(|err| TestkitError::new(format!("generated table build failed: {err}")))?;
    assert_built_table_matches_model(
        "generated rows build",
        &artifact,
        &expected_rows,
        rows_per_block,
    )?;

    assert_builder_paths_match(&builder, &identity, &rows, &expected_rows, artifact.bytes())?;
    assert_builder_compression_matches(
        target_data_block_size,
        rows_per_block,
        &identity,
        &rows,
        &expected_rows,
    )?;
    assert_builder_block_shapes(target_data_block_size, &identity, &rows, &expected_rows)?;
    assert_builder_one_row_shape(target_data_block_size, &identity, &rows)?;
    assert_builder_rejects_invalid_inputs(&builder, identity, &rows)?;

    Ok(())
}

fn assert_builder_paths_match(
    builder: &ImmutableTableBuilder,
    identity: &TableIdentity,
    rows: &[TableRow],
    expected_rows: &[StorageRow],
    expected_bytes: &[u8],
) -> Result<(), TestkitError> {
    let repeat = builder
        .build_from_rows(identity.clone(), rows)
        .map_err(|err| TestkitError::new(format!("generated repeat build failed: {err}")))?;
    if expected_bytes != repeat.bytes() {
        return Err(TestkitError::new(
            "generated table build was not deterministic",
        ));
    }

    let mut mutable = MutableTable::new();
    for row in expected_rows {
        mutable
            .insert_row(row.clone())
            .map_err(|err| TestkitError::new(format!("builder mutable setup failed: {err}")))?;
    }
    let frozen = mutable.clone().freeze();
    let from_mutable = builder
        .build_from_mutable(identity.clone(), &mutable)
        .map_err(|err| TestkitError::new(format!("generated mutable build failed: {err}")))?;
    let from_frozen = builder
        .build_from_frozen(identity.clone(), &frozen)
        .map_err(|err| TestkitError::new(format!("generated frozen build failed: {err}")))?;
    if expected_bytes != from_mutable.bytes() || expected_bytes != from_frozen.bytes() {
        return Err(TestkitError::new(
            "builder rows, mutable, and frozen paths diverged",
        ));
    }

    let storage_rows = rows.iter().map(|row| row.row().clone()).collect::<Vec<_>>();
    let from_storage_rows = builder
        .build_from_storage_rows(identity.clone(), &storage_rows)
        .map_err(|err| TestkitError::new(format!("generated storage-row build failed: {err}")))?;
    if expected_bytes != from_storage_rows.bytes() {
        return Err(TestkitError::new(
            "builder table-row and storage-row paths diverged",
        ));
    }

    Ok(())
}

fn assert_builder_compression_matches(
    target_data_block_size: u32,
    rows_per_block: usize,
    identity: &TableIdentity,
    rows: &[TableRow],
    expected_rows: &[StorageRow],
) -> Result<(), TestkitError> {
    let zstd_builder = ImmutableTableBuilder::new(
        TableBuilderConfig::new(
            target_data_block_size,
            rows_per_block,
            TableCompression::Zstd,
        )
        .map_err(|err| TestkitError::new(format!("zstd config setup failed: {err}")))?,
    )
    .map_err(|err| TestkitError::new(format!("zstd builder setup failed: {err}")))?;
    let zstd = zstd_builder
        .build_from_rows(identity.clone(), rows)
        .map_err(|err| TestkitError::new(format!("generated zstd build failed: {err}")))?;
    assert_built_table_matches_model("generated zstd build", &zstd, expected_rows, rows_per_block)
}

fn assert_builder_block_shapes(
    target_data_block_size: u32,
    identity: &TableIdentity,
    rows: &[TableRow],
    expected_rows: &[StorageRow],
) -> Result<(), TestkitError> {
    let single_block_builder = ImmutableTableBuilder::new(
        TableBuilderConfig::new(
            target_data_block_size,
            rows.len(),
            TableCompression::Uncompressed,
        )
        .map_err(|err| TestkitError::new(format!("single-block config failed: {err}")))?,
    )
    .map_err(|err| TestkitError::new(format!("single-block builder failed: {err}")))?;
    let single_block = single_block_builder
        .build_from_rows(identity.clone(), rows)
        .map_err(|err| TestkitError::new(format!("single-block build failed: {err}")))?;
    assert_built_table_matches_model(
        "single-block build",
        &single_block,
        expected_rows,
        rows.len(),
    )?;

    if rows.len() > 1 {
        let multi_block_builder = ImmutableTableBuilder::new(
            TableBuilderConfig::new(target_data_block_size, 1, TableCompression::Uncompressed)
                .map_err(|err| TestkitError::new(format!("multi-block config failed: {err}")))?,
        )
        .map_err(|err| TestkitError::new(format!("multi-block builder failed: {err}")))?;
        let multi_block = multi_block_builder
            .build_from_rows(identity.clone(), rows)
            .map_err(|err| TestkitError::new(format!("multi-block build failed: {err}")))?;
        assert_built_table_matches_model("multi-block build", &multi_block, expected_rows, 1)?;
    }
    Ok(())
}

fn assert_builder_one_row_shape(
    target_data_block_size: u32,
    identity: &TableIdentity,
    rows: &[TableRow],
) -> Result<(), TestkitError> {
    let one_row = rows
        .first()
        .cloned()
        .ok_or_else(|| TestkitError::new("generated builder rows were empty"))?;
    let expected_rows = vec![one_row.row().clone()];
    let builder = ImmutableTableBuilder::new(
        TableBuilderConfig::new(target_data_block_size, 1, TableCompression::Uncompressed)
            .map_err(|err| TestkitError::new(format!("one-row config failed: {err}")))?,
    )
    .map_err(|err| TestkitError::new(format!("one-row builder failed: {err}")))?;
    let artifact = builder
        .build_from_rows(identity.clone(), &[one_row])
        .map_err(|err| TestkitError::new(format!("one-row build failed: {err}")))?;
    assert_built_table_matches_model("one-row build", &artifact, &expected_rows, 1)
}

fn assert_builder_rejects_invalid_inputs(
    builder: &ImmutableTableBuilder,
    identity: TableIdentity,
    rows: &[TableRow],
) -> Result<(), TestkitError> {
    match builder.build_from_rows(identity.clone(), &[]) {
        Err(TableRuntimeError::InvalidRange { .. }) => {}
        Err(err) => {
            return Err(TestkitError::new(format!(
                "expected empty builder input rejection; got {err}"
            )));
        }
        Ok(_) => return Err(TestkitError::new("empty builder input was accepted")),
    }

    if rows.len() > 1 {
        let mut unsorted = rows.to_vec();
        unsorted.swap(0, 1);
        match builder.build_from_rows(identity.clone(), &unsorted) {
            Err(TableRuntimeError::InvalidRowOrder { .. }) => {}
            Err(err) => {
                return Err(TestkitError::new(format!(
                    "expected unsorted builder input rejection; got {err}"
                )));
            }
            Ok(_) => return Err(TestkitError::new("unsorted builder input was accepted")),
        }
    }

    let duplicate = vec![rows[0].clone(), rows[0].clone()];
    match builder.build_from_rows(identity, &duplicate) {
        Err(TableRuntimeError::DuplicateInternalKey { .. }) => {}
        Err(err) => {
            return Err(TestkitError::new(format!(
                "expected duplicate builder input rejection; got {err}"
            )));
        }
        Ok(_) => return Err(TestkitError::new("duplicate builder input was accepted")),
    }

    Ok(())
}

fn generated_builder_table_rows(script: &[u8]) -> Result<Vec<TableRow>, TestkitError> {
    let shared_key =
        deterministic_physical_key(0xb1, "builder", 0x20, b"shared\0builder".to_vec())?;
    let mut model = BTreeMap::<Vec<u8>, StorageRow>::new();
    for row in [
        StorageRow::put(
            shared_key.clone(),
            CommitVersion::new(11),
            Timestamp::from_micros(11),
            Timestamp::EPOCH,
            b"newer".to_vec(),
        ),
        StorageRow::put(
            shared_key,
            CommitVersion::new(3),
            Timestamp::from_micros(3),
            Timestamp::EPOCH,
            b"older".to_vec(),
        ),
        StorageRow::tombstone(
            deterministic_physical_key(0xb2, "builder", 0x20, b"delete".to_vec())?,
            CommitVersion::new(7),
            Timestamp::from_micros(7),
        ),
        StorageRow::put(
            deterministic_physical_key(0xb3, "builder", 0x20, b"expired".to_vec())?,
            CommitVersion::new(5),
            Timestamp::from_micros(5),
            Timestamp::from_micros(1),
            b"expired".to_vec(),
        ),
        StorageRow::put(
            deterministic_physical_key(0xb4, "builder", 0x21, b"empty".to_vec())?,
            CommitVersion::new(13),
            Timestamp::from_micros(13),
            Timestamp::EPOCH,
            Vec::new(),
        ),
    ] {
        model.insert(table_key_bytes(&row), row);
    }

    let generated_count = usize::from(script_byte(script, 27).min(251));
    for index in 0..generated_count {
        let row = generated_model_row(script, 32 + index)?;
        model.insert(table_key_bytes(&row), row);
    }

    let rows = model.into_values().map(TableRow::new).collect::<Vec<_>>();
    if rows.len() > 256 {
        return Err(TestkitError::new(
            "generated builder model exceeded bounded row budget",
        ));
    }
    validate_strictly_sorted_unique_rows(&rows)
        .map_err(|err| TestkitError::new(format!("generated builder rows invalid: {err}")))?;
    Ok(rows)
}

fn assert_built_table_matches_model(
    label: &'static str,
    artifact: &BuiltTableArtifact,
    expected_rows: &[StorageRow],
    rows_per_block: usize,
) -> Result<(), TestkitError> {
    let decoded = decode_immutable_table(artifact.bytes())
        .map_err(|err| TestkitError::new(format!("{label} decode failed: {err}")))?;
    let properties = decoded.properties();
    if decoded.rows() != expected_rows {
        return Err(TestkitError::new(format!(
            "{label} decoded rows did not match generated model"
        )));
    }
    let expected_block_count = expected_rows.len().div_ceil(rows_per_block);
    let expected_block_count_u32 = u32::try_from(expected_block_count)
        .map_err(|_| TestkitError::new("expected block count exceeded u32"))?;
    if decoded.data_blocks().len() != expected_block_count
        || decoded.header().data_block_count() != expected_block_count_u32
        || properties.data_block_count() != expected_block_count_u32
        || properties.row_count() != expected_rows.len() as u64
        || artifact.facts().data_block_count() != properties.data_block_count()
        || artifact.facts().row_count() != properties.row_count()
        || artifact.byte_count() != artifact.bytes().len() as u64
    {
        return Err(TestkitError::new(format!(
            "{label} decoded table facts drifted"
        )));
    }

    let expected_first = expected_rows
        .first()
        .map(table_key_bytes)
        .ok_or_else(|| TestkitError::new("builder model rows were empty"))?;
    let expected_last = expected_rows
        .last()
        .map(table_key_bytes)
        .ok_or_else(|| TestkitError::new("builder model rows were empty"))?;
    if artifact.facts().key_range().first_key() != expected_first.as_slice()
        || artifact.facts().key_range().last_key() != expected_last.as_slice()
        || artifact.facts().key_range().first_key() != properties.min_key_bytes()
        || artifact.facts().key_range().last_key() != properties.max_key_bytes()
    {
        return Err(TestkitError::new(format!(
            "{label} key range facts drifted"
        )));
    }

    let expected_min = expected_rows
        .iter()
        .map(StorageRow::commit_version)
        .min()
        .ok_or_else(|| TestkitError::new("builder model rows were empty"))?;
    let expected_max = expected_rows
        .iter()
        .map(StorageRow::commit_version)
        .max()
        .ok_or_else(|| TestkitError::new("builder model rows were empty"))?;
    if artifact.facts().commit_range().min() != expected_min
        || artifact.facts().commit_range().max() != expected_max
        || artifact.facts().commit_range().min() != properties.commit_min()
        || artifact.facts().commit_range().max() != properties.commit_max()
    {
        return Err(TestkitError::new(format!(
            "{label} commit range facts drifted"
        )));
    }

    Ok(())
}

fn check_immutable_table_reader(script: &[u8]) -> Result<(), TestkitError> {
    let rows = generated_builder_table_rows(script)?;
    let expected_rows = rows.iter().map(|row| row.row().clone()).collect::<Vec<_>>();
    let target_data_block_size = 1 + u32::from(script_byte(script, 48));
    let rows_per_block = 1 + usize::from(script_byte(script, 49) % 8);
    let compression = if script_byte(script, 50) & 1 == 0 {
        TableCompression::Uncompressed
    } else {
        TableCompression::Zstd
    };
    let config = TableReaderConfig::new();
    let builder = ImmutableTableBuilder::new(
        TableBuilderConfig::new(target_data_block_size, rows_per_block, compression)
            .map_err(|err| TestkitError::new(format!("reader builder config failed: {err}")))?,
    )
    .map_err(|err| TestkitError::new(format!("reader builder setup failed: {err}")))?;
    let identity = TableIdentity::new(format!("reader-{:02x}", script_byte(script, 52)))
        .map_err(|err| TestkitError::new(format!("reader identity setup failed: {err}")))?;
    let artifact = builder
        .build_from_rows(identity.clone(), &rows)
        .map_err(|err| TestkitError::new(format!("reader artifact build failed: {err}")))?;

    let reader =
        ImmutableTableReader::open_bytes(identity.clone(), artifact.bytes().to_vec(), config)
            .map_err(|err| TestkitError::new(format!("reader bytes open failed: {err}")))?;
    assert_immutable_reader_matches_model(
        "generated byte reader",
        &reader,
        &artifact,
        &rows,
        &expected_rows,
        config,
    )?;

    let source_reader = ImmutableTableReader::open_source(
        identity.clone(),
        BytesTableSource::new(artifact.bytes().to_vec()),
        config,
    )
    .map_err(|err| TestkitError::new(format!("reader source open failed: {err}")))?;
    assert_immutable_reader_matches_model(
        "generated source reader",
        &source_reader,
        &artifact,
        &rows,
        &expected_rows,
        config,
    )?;

    let mut corrupt = artifact.bytes().to_vec();
    corrupt[0] = corrupt[0].wrapping_add(1);
    match ImmutableTableReader::open_bytes(identity.clone(), corrupt, config) {
        Err(TableRuntimeError::DecodeFormat { .. }) => {}
        Err(err) => {
            return Err(TestkitError::new(format!(
                "expected corrupt reader bytes to produce decode error; got {err}"
            )));
        }
        Ok(_) => return Err(TestkitError::new("corrupt reader bytes were accepted")),
    }

    match ImmutableTableReader::open_source(
        identity,
        ShortTableSource::new(artifact.into_bytes()),
        config,
    ) {
        Err(TableRuntimeError::SourceRead { .. }) => {}
        Err(err) => {
            return Err(TestkitError::new(format!(
                "expected short reader source to produce source error; got {err}"
            )));
        }
        Ok(_) => return Err(TestkitError::new("short reader source was accepted")),
    }

    Ok(())
}

fn check_object_backed_table_reader(script: &[u8]) -> Result<(), TestkitError> {
    let rows = generated_builder_table_rows(script)?;
    let expected_rows = rows.iter().map(|row| row.row().clone()).collect::<Vec<_>>();
    let target_data_block_size = 1 + u32::from(script_byte(script, 64));
    let rows_per_block = 1 + usize::from(script_byte(script, 65) % 8);
    let compression = if script_byte(script, 66) & 1 == 0 {
        TableCompression::Uncompressed
    } else {
        TableCompression::Zstd
    };
    let config = TableReaderConfig::new();
    let builder = ImmutableTableBuilder::new(
        TableBuilderConfig::new(target_data_block_size, rows_per_block, compression).map_err(
            |err| TestkitError::new(format!("object-backed builder config failed: {err}")),
        )?,
    )
    .map_err(|err| TestkitError::new(format!("object-backed builder setup failed: {err}")))?;
    let identity = TableIdentity::new(format!("object-reader-{:02x}", script_byte(script, 68)))
        .map_err(|err| TestkitError::new(format!("object reader identity setup failed: {err}")))?;
    let artifact = builder
        .build_from_rows(identity.clone(), &rows)
        .map_err(|err| TestkitError::new(format!("object-backed artifact build failed: {err}")))?;

    let branch = BranchId::from_bytes([script_byte(script, 69); BranchId::BYTE_LEN]).to_string();
    let level = u32::from(script_byte(script, 70) % 4);
    let table_id = format!("table{:04x}", script_byte(script, 71));
    let object = ObjectLayout::table_object(&branch, level, &table_id)
        .map_err(|err| TestkitError::new(format!("object-backed layout failed: {err}")))?;
    let backend = MemoryBackend::new();
    backend
        .write_object(&object, artifact.bytes())
        .map_err(|err| TestkitError::new(format!("object-backed seed write failed: {err}")))?;
    let source = TableObjectByteSource::new(&backend, object, artifact.byte_count())
        .map_err(|err| TestkitError::new(format!("object-backed source setup failed: {err}")))?;
    let object_reader = ImmutableTableReader::open_source(identity.clone(), &source, config)
        .map_err(|err| TestkitError::new(format!("object-backed reader open failed: {err}")))?;
    assert_immutable_reader_matches_model(
        "generated object-backed reader",
        &object_reader,
        &artifact,
        &rows,
        &expected_rows,
        config,
    )?;

    let byte_reader = ImmutableTableReader::open_bytes(identity, artifact.bytes().to_vec(), config)
        .map_err(|err| TestkitError::new(format!("object-backed byte model open failed: {err}")))?;
    if object_reader.facts() != byte_reader.facts()
        || object_reader.rows() != byte_reader.rows()
        || object_reader.config() != byte_reader.config()
    {
        return Err(TestkitError::new(
            "object-backed reader drifted from byte-backed reader",
        ));
    }

    Ok(())
}

fn check_table_block_cache(script: &[u8]) -> Result<(), TestkitError> {
    let cache = TableBlockCache::new(
        TableCacheConfig::new(true, 12)
            .map_err(|err| TestkitError::new(format!("cache config failed: {err}")))?,
    );
    let table_a = TableCacheTableId::new(vec![0xa0, script_byte(script, 56)])
        .map_err(|err| TestkitError::new(format!("cache table id failed: {err}")))?;
    let table_b = TableCacheTableId::new(vec![0xb0, script_byte(script, 57)])
        .map_err(|err| TestkitError::new(format!("cache table id failed: {err}")))?;
    let first = generated_cache_key(&table_a, TableBlockCacheKind::Data, 0, 4, Some(0))?;
    let second = generated_cache_key(&table_a, TableBlockCacheKind::Index, 4, 4, Some(1))?;
    let third = generated_cache_key(&table_a, TableBlockCacheKind::Data, 8, 8, Some(2))?;
    let other_table = generated_cache_key(&table_b, TableBlockCacheKind::Data, 0, 4, Some(0))?;

    expect_cache_inserted(cache.insert(first.clone(), arc_bytes(script_byte(script, 58), 4)))?;
    expect_cache_hit(&cache, &first, script_byte(script, 58), 4)?;
    expect_cache_duplicate(
        cache.insert(first.clone(), arc_bytes(script_byte(script, 59), 4)),
        script_byte(script, 58),
        4,
    )?;
    expect_cache_inserted(cache.insert(second.clone(), arc_bytes(script_byte(script, 60), 4)))?;
    expect_cache_hit(&cache, &first, script_byte(script, 58), 4)?;
    expect_cache_inserted(cache.insert(third.clone(), arc_bytes(script_byte(script, 61), 8)))?;
    if cache.stats().bytes() > cache.stats().capacity_bytes() {
        return Err(TestkitError::new("cache exceeded capacity"));
    }
    if cache.get(&second).is_some() {
        return Err(TestkitError::new(
            "least-recent cache entry survived pressure",
        ));
    }
    expect_cache_hit(&cache, &first, script_byte(script, 58), 4)?;
    expect_cache_hit(&cache, &third, script_byte(script, 61), 8)?;

    expect_cache_inserted(
        cache.insert(other_table.clone(), arc_bytes(script_byte(script, 62), 4)),
    )?;
    if cache.remove_table(&table_a) == 0 || cache.get(&first).is_some() {
        return Err(TestkitError::new("cache table removal failed"));
    }
    expect_cache_hit(&cache, &other_table, script_byte(script, 62), 4)?;

    check_disabled_and_oversized_cache(script)?;
    check_generated_cache_model(script)
}

fn check_disabled_and_oversized_cache(script: &[u8]) -> Result<(), TestkitError> {
    let disabled = TableBlockCache::disabled();
    let disabled_key = generated_cache_key(
        &TableCacheTableId::new(b"disabled".to_vec())
            .map_err(|err| TestkitError::new(format!("disabled cache id failed: {err}")))?,
        TableBlockCacheKind::Data,
        0,
        4,
        None,
    )?;
    match disabled
        .insert(disabled_key.clone(), arc_bytes(script_byte(script, 63), 4))
        .map_err(|err| TestkitError::new(format!("disabled cache insert failed: {err}")))?
    {
        CacheInsert::SkippedDisabled(_) => {}
        other => {
            return Err(TestkitError::new(format!(
                "disabled cache returned unexpected insert result: {other:?}"
            )));
        }
    }
    if disabled.get(&disabled_key).is_some() || disabled.stats().entries() != 0 {
        return Err(TestkitError::new("disabled cache stored an entry"));
    }

    let small = TableBlockCache::new(
        TableCacheConfig::new(true, 2)
            .map_err(|err| TestkitError::new(format!("small cache config failed: {err}")))?,
    );
    match small
        .insert(disabled_key, arc_bytes(script_byte(script, 64), 4))
        .map_err(|err| TestkitError::new(format!("oversized cache insert failed: {err}")))?
    {
        CacheInsert::SkippedOversized(_) => Ok(()),
        other => Err(TestkitError::new(format!(
            "oversized cache returned unexpected insert result: {other:?}"
        ))),
    }
}

fn check_generated_cache_model(script: &[u8]) -> Result<(), TestkitError> {
    let enabled = script_byte(script, 88) % 3 != 0;
    let initial_capacity = if enabled {
        1 + usize::from(script_byte(script, 89) % 64)
    } else {
        0
    };
    let cache = TableBlockCache::new(
        TableCacheConfig::new(enabled, initial_capacity)
            .map_err(|err| TestkitError::new(format!("generated cache config failed: {err}")))?,
    );
    let mut model = GeneratedCacheModel::new(enabled, initial_capacity);
    let operations = 32 + usize::from(script_byte(script, 90) % 32);

    for step in 0..operations {
        run_generated_cache_operation(script, step, &cache, &mut model)?;
        assert_generated_cache_stats(&cache.stats(), &model)?;
        let before = cache.stats();
        let after = cache.stats();
        if before != after {
            return Err(TestkitError::new("cache stats read mutated cache state"));
        }
    }

    Ok(())
}

fn run_generated_cache_operation(
    script: &[u8],
    step: usize,
    cache: &TableBlockCache,
    model: &mut GeneratedCacheModel,
) -> Result<(), TestkitError> {
    let key = generated_script_cache_key(script, step)?;
    match generated_script_cache_byte(script, step, 91) % 6 {
        0 => {
            let expected_bytes = model.get(&key);
            if cache.get(&key).map(|bytes| bytes.to_vec()) != expected_bytes {
                return Err(TestkitError::new("generated cache get drifted"));
            }
        }
        1 => {
            let value = generated_script_cache_value(script, step);
            let expected = model.insert(&key, value);
            let observed = cache
                .insert(key, Arc::<[u8]>::from(expected.input_bytes().to_vec()))
                .map_err(|err| {
                    TestkitError::new(format!("generated cache insert failed: {err}"))
                })?;
            assert_generated_cache_insert(observed, expected)?;
        }
        2 => {
            let expected_removed = model.remove(&key);
            if cache.remove(&key) != expected_removed {
                return Err(TestkitError::new("generated cache remove drifted"));
            }
        }
        3 => {
            let table = generated_script_cache_table_id(script, step)?;
            let expected_removed = model.remove_table(&table);
            if cache.remove_table(&table) != expected_removed {
                return Err(TestkitError::new("generated cache table removal drifted"));
            }
        }
        4 => {
            model.clear();
            cache.clear();
        }
        _ => {
            let capacity = generated_script_cache_capacity(script, step);
            model.resize(capacity);
            cache.resize(capacity);
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum GeneratedCacheInsert {
    Inserted(Vec<u8>),
    DuplicateExisting { stored: Vec<u8>, attempted: Vec<u8> },
    SkippedDisabled(Vec<u8>),
    SkippedOversized(Vec<u8>),
}

impl GeneratedCacheInsert {
    fn input_bytes(&self) -> &[u8] {
        match self {
            Self::Inserted(bytes)
            | Self::SkippedDisabled(bytes)
            | Self::SkippedOversized(bytes) => bytes,
            Self::DuplicateExisting { attempted, .. } => attempted,
        }
    }
}

struct GeneratedCacheModel {
    enabled: bool,
    capacity_bytes: usize,
    bytes: usize,
    entries: BTreeMap<TableBlockCacheKey, Vec<u8>>,
    recency: VecDeque<TableBlockCacheKey>,
    hits: u64,
    misses: u64,
    inserts: u64,
    duplicate_inserts: u64,
    evictions: u64,
    removes: u64,
    table_invalidations: u64,
    clears: u64,
    skipped_oversized: u64,
    skipped_disabled: u64,
}

impl GeneratedCacheModel {
    fn new(enabled: bool, capacity_bytes: usize) -> Self {
        Self {
            enabled,
            capacity_bytes,
            bytes: 0,
            entries: BTreeMap::new(),
            recency: VecDeque::new(),
            hits: 0,
            misses: 0,
            inserts: 0,
            duplicate_inserts: 0,
            evictions: 0,
            removes: 0,
            table_invalidations: 0,
            clears: 0,
            skipped_oversized: 0,
            skipped_disabled: 0,
        }
    }

    fn get(&mut self, key: &TableBlockCacheKey) -> Option<Vec<u8>> {
        if let Some(bytes) = self.entries.get(key).cloned() {
            self.hits = self.hits.saturating_add(1);
            model_touch_recency(&mut self.recency, key);
            Some(bytes)
        } else {
            self.misses = self.misses.saturating_add(1);
            None
        }
    }

    fn insert(&mut self, key: &TableBlockCacheKey, bytes: Vec<u8>) -> GeneratedCacheInsert {
        if !self.enabled {
            self.skipped_disabled = self.skipped_disabled.saturating_add(1);
            return GeneratedCacheInsert::SkippedDisabled(bytes);
        }
        if let Some(stored) = self.entries.get(key).cloned() {
            self.duplicate_inserts = self.duplicate_inserts.saturating_add(1);
            model_touch_recency(&mut self.recency, key);
            return GeneratedCacheInsert::DuplicateExisting {
                stored,
                attempted: bytes,
            };
        }
        if bytes.len() > self.capacity_bytes {
            self.skipped_oversized = self.skipped_oversized.saturating_add(1);
            return GeneratedCacheInsert::SkippedOversized(bytes);
        }
        self.evict_to_fit(bytes.len());
        if self.bytes.saturating_add(bytes.len()) > self.capacity_bytes {
            self.skipped_oversized = self.skipped_oversized.saturating_add(1);
            return GeneratedCacheInsert::SkippedOversized(bytes);
        }

        self.bytes = self.bytes.saturating_add(bytes.len());
        self.entries.insert(key.clone(), bytes.clone());
        model_touch_recency(&mut self.recency, key);
        self.inserts = self.inserts.saturating_add(1);
        GeneratedCacheInsert::Inserted(bytes)
    }

    fn remove(&mut self, key: &TableBlockCacheKey) -> bool {
        if let Some(bytes) = self.entries.remove(key) {
            self.bytes = self.bytes.saturating_sub(bytes.len());
            model_remove_from_recency(&mut self.recency, key);
            self.removes = self.removes.saturating_add(1);
            true
        } else {
            false
        }
    }

    fn remove_table(&mut self, table: &TableCacheTableId) -> usize {
        let keys = self
            .entries
            .keys()
            .filter(|key| key.table() == table)
            .cloned()
            .collect::<Vec<_>>();
        let removed = keys.len();
        for key in keys {
            if let Some(bytes) = self.entries.remove(&key) {
                self.bytes = self.bytes.saturating_sub(bytes.len());
            }
            model_remove_from_recency(&mut self.recency, &key);
        }
        if removed > 0 {
            self.table_invalidations = self.table_invalidations.saturating_add(1);
        }
        removed
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.recency.clear();
        self.bytes = 0;
        self.clears = self.clears.saturating_add(1);
    }

    fn resize(&mut self, capacity_bytes: usize) {
        self.capacity_bytes = capacity_bytes;
        while self.bytes > self.capacity_bytes {
            if !self.evict_one() {
                break;
            }
        }
    }

    fn evict_to_fit(&mut self, incoming_bytes: usize) {
        while self.bytes.saturating_add(incoming_bytes) > self.capacity_bytes {
            if !self.evict_one() {
                break;
            }
        }
    }

    fn evict_one(&mut self) -> bool {
        while let Some(key) = self.recency.pop_front() {
            if let Some(bytes) = self.entries.remove(&key) {
                self.bytes = self.bytes.saturating_sub(bytes.len());
                self.evictions = self.evictions.saturating_add(1);
                return true;
            }
        }
        false
    }
}

fn assert_generated_cache_insert(
    observed: CacheInsert,
    expected: GeneratedCacheInsert,
) -> Result<(), TestkitError> {
    match (observed, expected) {
        (CacheInsert::Inserted(bytes), GeneratedCacheInsert::Inserted(expected_bytes))
        | (
            CacheInsert::SkippedDisabled(bytes),
            GeneratedCacheInsert::SkippedDisabled(expected_bytes),
        )
        | (
            CacheInsert::SkippedOversized(bytes),
            GeneratedCacheInsert::SkippedOversized(expected_bytes),
        ) if bytes.as_ref() == expected_bytes.as_slice() => Ok(()),
        (
            CacheInsert::DuplicateExisting(bytes),
            GeneratedCacheInsert::DuplicateExisting { stored, .. },
        ) if bytes.as_ref() == stored.as_slice() => Ok(()),
        (observed, expected) => Err(TestkitError::new(format!(
            "generated cache insert drifted: observed {observed:?}, expected {expected:?}"
        ))),
    }
}

fn assert_generated_cache_stats(
    stats: &TableBlockCacheStats,
    model: &GeneratedCacheModel,
) -> Result<(), TestkitError> {
    if stats.entries() != model.entries.len()
        || stats.bytes() != model.bytes
        || stats.capacity_bytes() != model.capacity_bytes
        || stats.hits() != model.hits
        || stats.misses() != model.misses
        || stats.inserts() != model.inserts
        || stats.duplicate_inserts() != model.duplicate_inserts
        || stats.evictions() != model.evictions
        || stats.removes() != model.removes
        || stats.table_invalidations() != model.table_invalidations
        || stats.clears() != model.clears
        || stats.skipped_oversized() != model.skipped_oversized
        || stats.skipped_disabled() != model.skipped_disabled
        || stats.bytes() > stats.capacity_bytes()
    {
        return Err(TestkitError::new(
            "generated cache stats drifted from model",
        ));
    }
    Ok(())
}

fn model_touch_recency(recency: &mut VecDeque<TableBlockCacheKey>, key: &TableBlockCacheKey) {
    model_remove_from_recency(recency, key);
    recency.push_back(key.clone());
}

fn model_remove_from_recency(recency: &mut VecDeque<TableBlockCacheKey>, key: &TableBlockCacheKey) {
    if let Some(index) = recency.iter().position(|candidate| candidate == key) {
        recency.remove(index);
    }
}

fn generated_script_cache_key(
    script: &[u8],
    step: usize,
) -> Result<TableBlockCacheKey, TestkitError> {
    let table = generated_script_cache_table_id(script, step)?;
    let kind = match generated_script_cache_byte(script, step, 23) % 4 {
        0 => TableBlockCacheKind::Data,
        1 => TableBlockCacheKind::Index,
        2 => TableBlockCacheKind::Properties,
        _ => TableBlockCacheKind::Accelerator,
    };
    generated_cache_key(
        &table,
        kind,
        u64::from(generated_script_cache_byte(script, step, 37) % 16).saturating_mul(4),
        1 + u32::from(generated_script_cache_byte(script, step, 41) % 16),
        Some(u32::from(
            generated_script_cache_byte(script, step, 43) % 16,
        )),
    )
}

fn generated_script_cache_table_id(
    script: &[u8],
    step: usize,
) -> Result<TableCacheTableId, TestkitError> {
    TableCacheTableId::new(vec![
        b'g',
        generated_script_cache_byte(script, step, 11) % 4,
        generated_script_cache_byte(script, step, 17),
    ])
    .map_err(|err| TestkitError::new(format!("generated cache table id failed: {err}")))
}

fn generated_script_cache_value(script: &[u8], step: usize) -> Vec<u8> {
    let len = 1 + usize::from(generated_script_cache_byte(script, step, 53) % 96);
    vec![generated_script_cache_byte(script, step, 59); len]
}

fn generated_script_cache_capacity(script: &[u8], step: usize) -> usize {
    match generated_script_cache_byte(script, step, 67) % 6 {
        0 => 0,
        1 => 1,
        2 => 4,
        3 => 16,
        4 => 64,
        _ => 128,
    }
}

fn generated_script_cache_byte(script: &[u8], step: usize, salt: usize) -> u8 {
    let base = if script.is_empty() {
        0
    } else {
        script[(step.saturating_mul(37).saturating_add(salt)) % script.len()]
    };
    base.wrapping_add(
        u8::try_from(step % 251)
            .expect("step residue fits u8")
            .wrapping_mul(17),
    )
    .wrapping_add(u8::try_from(salt % 251).expect("salt residue fits u8"))
}

fn check_table_bloom_filter(script: &[u8]) -> Result<(), TestkitError> {
    let first = deterministic_physical_key(1, "cache", 0x20, generated_user_key(script, 65))?;
    let second = deterministic_physical_key(1, "cache", 0x21, generated_user_key(script, 73))?;
    let third = deterministic_physical_key(2, "cache", 0x20, generated_user_key(script, 81))?;
    let first_key = TablePhysicalKeyBytes::from_physical_key(&first);
    let second_key = TablePhysicalKeyBytes::from_physical_key(&second);
    let third_key = TablePhysicalKeyBytes::from_physical_key(&third);
    let keys = vec![
        first_key.as_slice(),
        second_key.as_slice(),
        third_key.as_slice(),
    ];
    let filter = TableBloomFilter::build(keys.clone(), 10)
        .map_err(|err| TestkitError::new(format!("bloom build failed: {err}")))?;

    for key in keys {
        if filter.might_contain(key) == TableBloomProbe::DefinitelyAbsent {
            return Err(TestkitError::new("bloom filter produced a false negative"));
        }
    }
    let absent = TablePhysicalKeyBytes::from_physical_key(&deterministic_physical_key(
        0xfe,
        "cache",
        0x20,
        b"absent".to_vec(),
    )?);
    if !matches!(
        filter.might_contain(absent.as_slice()),
        TableBloomProbe::DefinitelyAbsent | TableBloomProbe::MaybePresent
    ) {
        return Err(TestkitError::new(
            "bloom filter returned non-conservative absence",
        ));
    }

    let empty = TableBloomFilter::build(Vec::<&[u8]>::new(), 10)
        .map_err(|err| TestkitError::new(format!("empty bloom build failed: {err}")))?;
    if !empty.is_empty() || empty.might_contain(b"anything") != TableBloomProbe::DefinitelyAbsent {
        return Err(TestkitError::new("empty bloom filter facts drifted"));
    }
    expect_invalid_config(TableBloomFilter::build([b"key".as_slice()], 0))?;
    Ok(())
}

fn check_table_compaction(script: &[u8]) -> Result<(), TestkitError> {
    let rows = generated_compaction_table_rows(script)?;
    let expected_rows = rows.iter().map(|row| row.row().clone()).collect::<Vec<_>>();
    let source_count = 1 + usize::from(script_byte(script, 88) % GENERATED_COMPACTION_MAX_SOURCES);
    let sources = generated_compaction_sources(&rows, source_count, "generated")?;
    let compactor = generated_compactor(script, 89, GENERATED_COMPACTION_MAX_ROWS)?;

    check_keep_all_compaction(
        script,
        &rows,
        &expected_rows,
        &sources,
        source_count,
        &compactor,
    )?;
    check_policy_compaction(
        script,
        &rows,
        &expected_rows,
        &sources,
        source_count,
        &compactor,
    )?;
    assert_compaction_rejects_duplicate_and_output_limit(script)?;
    Ok(())
}

fn generated_compaction_table_rows(script: &[u8]) -> Result<Vec<TableRow>, TestkitError> {
    let requested_rows = generated_compaction_row_count(script);
    let mut model = BTreeMap::<Vec<u8>, StorageRow>::new();
    for row in generated_compaction_seed_rows()?
        .into_iter()
        .take(requested_rows)
    {
        model.insert(table_key_bytes(&row), row);
    }

    let mut generated_index = 0_usize;
    while model.len() < requested_rows {
        let row = generated_model_row(script, generated_index)?;
        model.insert(table_key_bytes(&row), row);
        generated_index = generated_index.saturating_add(1);
    }

    let rows = model.into_values().map(TableRow::new).collect::<Vec<_>>();
    validate_strictly_sorted_unique_rows(&rows)
        .map_err(|err| TestkitError::new(format!("generated compaction rows invalid: {err}")))?;
    Ok(rows)
}

fn generated_compaction_row_count(script: &[u8]) -> usize {
    let raw = u16::from_le_bytes([script_byte(script, 93), script_byte(script, 94)]);
    usize::from(raw) % (GENERATED_COMPACTION_MAX_ROWS + 1)
}

fn generated_compaction_seed_rows() -> Result<Vec<StorageRow>, TestkitError> {
    let shared_key =
        deterministic_physical_key(0xc1, "compaction", 0x20, b"shared\0compaction".to_vec())?;
    Ok(vec![
        StorageRow::put(
            shared_key.clone(),
            CommitVersion::new(11),
            Timestamp::from_micros(11),
            Timestamp::EPOCH,
            b"newer".to_vec(),
        ),
        StorageRow::put(
            shared_key,
            CommitVersion::new(3),
            Timestamp::from_micros(3),
            Timestamp::EPOCH,
            b"older".to_vec(),
        ),
        StorageRow::tombstone(
            deterministic_physical_key(0xc2, "compaction", 0x20, b"delete".to_vec())?,
            CommitVersion::new(7),
            Timestamp::from_micros(7),
        ),
        StorageRow::put(
            deterministic_physical_key(0xc3, "compaction", 0x20, b"expired".to_vec())?,
            CommitVersion::new(5),
            Timestamp::from_micros(5),
            Timestamp::from_micros(1),
            b"expired".to_vec(),
        ),
        StorageRow::put(
            deterministic_physical_key(0xc4, "compaction", 0x21, b"empty".to_vec())?,
            CommitVersion::new(13),
            Timestamp::from_micros(13),
            Timestamp::EPOCH,
            Vec::new(),
        ),
    ])
}

fn check_keep_all_compaction(
    script: &[u8],
    rows: &[TableRow],
    expected_rows: &[StorageRow],
    sources: &[TableCompactionSource],
    source_count: usize,
    compactor: &TableCompactor,
) -> Result<(), TestkitError> {
    let keep_identity = TableIdentity::new(format!("compact-keep-{:02x}", script_byte(script, 90)))
        .map_err(|err| TestkitError::new(format!("compaction identity failed: {err}")))?;
    let mut keep_all = KeepAllTableCompactionPolicy;
    let keep_all_output = compactor
        .compact(&keep_identity, sources, &mut keep_all)
        .map_err(|err| TestkitError::new(format!("keep-all compaction failed: {err}")))?;
    assert_compaction_output_matches_model(
        "keep-all compaction",
        &keep_all_output,
        expected_rows,
        source_count,
        rows.len() as u64,
        rows.len() as u64,
        0,
    )?;
    let mut repeat_keep_all = KeepAllTableCompactionPolicy;
    let repeated_output = compactor
        .compact(&keep_identity, sources, &mut repeat_keep_all)
        .map_err(|err| TestkitError::new(format!("repeat compaction failed: {err}")))?;
    assert_compaction_output_matches_model(
        "repeat keep-all compaction",
        &repeated_output,
        expected_rows,
        source_count,
        rows.len() as u64,
        rows.len() as u64,
        0,
    )?;
    assert_compaction_outputs_byte_identical(
        "repeat keep-all compaction",
        &keep_all_output,
        &repeated_output,
    )?;

    let regrouped_count = if source_count == 1 { 2 } else { 1 };
    let regrouped_sources = generated_compaction_sources(rows, regrouped_count, "regrouped")?;
    let mut regrouped_keep_all = KeepAllTableCompactionPolicy;
    let regrouped_output = compactor
        .compact(&keep_identity, &regrouped_sources, &mut regrouped_keep_all)
        .map_err(|err| TestkitError::new(format!("regrouped compaction failed: {err}")))?;
    assert_compaction_output_matches_model(
        "regrouped keep-all compaction",
        &regrouped_output,
        expected_rows,
        regrouped_count,
        rows.len() as u64,
        rows.len() as u64,
        0,
    )?;
    assert_compaction_outputs_byte_identical(
        "regrouped keep-all compaction",
        &keep_all_output,
        &regrouped_output,
    )?;
    Ok(())
}

fn check_policy_compaction(
    script: &[u8],
    rows: &[TableRow],
    expected_rows: &[StorageRow],
    sources: &[TableCompactionSource],
    source_count: usize,
    compactor: &TableCompactor,
) -> Result<(), TestkitError> {
    let drop_mod = 2 + u64::from(script_byte(script, 91) % 5);
    let mut drop_policy = |context: &crate::table::TableCompactionRowContext<'_>,
                           row: &TableRow| {
        if context.merged_row_index() % drop_mod == 0 {
            Ok(TableCompactionDecision::drop(compaction_drop_reason(row)))
        } else {
            Ok(TableCompactionDecision::Keep)
        }
    };
    let policy_output = compactor
        .compact(
            &TableIdentity::new(format!("compact-policy-{:02x}", script_byte(script, 92)))
                .map_err(|err| TestkitError::new(format!("policy identity failed: {err}")))?,
            sources,
            &mut drop_policy,
        )
        .map_err(|err| TestkitError::new(format!("policy compaction failed: {err}")))?;
    let expected_kept = expected_rows
        .iter()
        .enumerate()
        .filter(|(index, _)| (u64::try_from(*index).expect("bounded index") % drop_mod) != 0)
        .map(|(_, row)| row.clone())
        .collect::<Vec<_>>();
    let expected_dropped = expected_rows.len().saturating_sub(expected_kept.len()) as u64;
    assert_compaction_output_matches_model(
        "policy compaction",
        &policy_output,
        &expected_kept,
        source_count,
        rows.len() as u64,
        expected_kept.len() as u64,
        expected_dropped,
    )?;
    if policy_output
        .report()
        .drop_summaries()
        .iter()
        .map(|summary| summary.rows())
        .sum::<u64>()
        != expected_dropped
    {
        return Err(TestkitError::new(
            "generated compaction drop summaries did not add up",
        ));
    }

    Ok(())
}

fn generated_compactor(
    script: &[u8],
    offset: usize,
    max_output_tables: usize,
) -> Result<TableCompactor, TestkitError> {
    let target_output_bytes = 1 + u64::from(script_byte(script, offset));
    let rows_per_block = 1 + usize::from(script_byte(script, offset + 1) % 8);
    let compression = if script_byte(script, offset + 2) & 1 == 0 {
        TableCompression::Uncompressed
    } else {
        TableCompression::Zstd
    };
    TableCompactor::new(
        TableCompactionConfig::new(target_output_bytes, max_output_tables)
            .map_err(|err| TestkitError::new(format!("compaction config failed: {err}")))?,
        TableBuilderConfig::new(
            1 + u32::from(script_byte(script, offset + 3)),
            rows_per_block,
            compression,
        )
        .map_err(|err| TestkitError::new(format!("compaction builder config failed: {err}")))?,
    )
    .map_err(|err| TestkitError::new(format!("compactor setup failed: {err}")))
}

fn generated_compaction_sources(
    rows: &[TableRow],
    source_count: usize,
    label: &'static str,
) -> Result<Vec<TableCompactionSource>, TestkitError> {
    let mut buckets = vec![Vec::<TableRow>::new(); source_count];
    for (index, row) in rows.iter().cloned().enumerate() {
        buckets[index % source_count].push(row);
    }
    buckets
        .into_iter()
        .enumerate()
        .map(|(index, bucket)| {
            let id = TableCompactionSourceId::new(format!("{label}-{index}"))
                .map_err(|err| TestkitError::new(format!("source id setup failed: {err}")))?;
            TableCompactionSource::from_rows(id, bucket)
                .map_err(|err| TestkitError::new(format!("source setup failed: {err}")))
        })
        .collect()
}

fn compaction_drop_reason(row: &TableRow) -> TableCompactionDropReason {
    if row.is_tombstone() {
        TableCompactionDropReason::TombstoneElided
    } else if row.expires_at() != Timestamp::EPOCH {
        TableCompactionDropReason::Expired
    } else if row.commit_version().as_u64() % 2 == 0 {
        TableCompactionDropReason::OlderVersion
    } else {
        TableCompactionDropReason::CallerSelected
    }
}

fn assert_compaction_output_matches_model(
    label: &'static str,
    output: &crate::table::TableCompactionOutput,
    expected_rows: &[StorageRow],
    expected_input_sources: usize,
    expected_input_rows: u64,
    expected_kept_rows: u64,
    expected_dropped_rows: u64,
) -> Result<(), TestkitError> {
    let actual_rows = compaction_output_rows(output)?;
    if actual_rows != expected_rows {
        return Err(TestkitError::new(format!(
            "{label} rows did not match generated model"
        )));
    }
    let actual_table_rows = actual_rows
        .iter()
        .cloned()
        .map(TableRow::new)
        .collect::<Vec<_>>();
    validate_strictly_sorted_unique_rows(&actual_table_rows).map_err(|err| {
        TestkitError::new(format!("{label} concatenated output rows invalid: {err}"))
    })?;
    if output.report().input_rows() != expected_input_rows
        || output.report().input_sources() != expected_input_sources
        || output.report().kept_rows() != expected_kept_rows
        || output.report().dropped_rows() != expected_dropped_rows
        || output.report().output_tables() != output.artifacts().len()
    {
        return Err(TestkitError::new(format!("{label} report drifted")));
    }
    let output_bytes = output
        .artifacts()
        .iter()
        .map(BuiltTableArtifact::byte_count)
        .sum::<u64>();
    if output.report().output_bytes() != output_bytes {
        return Err(TestkitError::new(format!(
            "{label} output byte report drifted"
        )));
    }
    if output.artifacts().is_empty() {
        if output.report().split_count() != 0 {
            return Err(TestkitError::new(format!(
                "{label} empty split count drifted"
            )));
        }
    } else if output.report().split_count() != output.artifacts().len().saturating_sub(1) as u64 {
        return Err(TestkitError::new(format!("{label} split count drifted")));
    }

    assert_compaction_artifacts_match_model(label, output)?;
    Ok(())
}

fn assert_compaction_artifacts_match_model(
    label: &'static str,
    output: &crate::table::TableCompactionOutput,
) -> Result<(), TestkitError> {
    let mut identities = Vec::new();
    for artifact in output.artifacts() {
        identities.push(artifact.facts().identity().as_str().to_owned());
        assert_compaction_artifact_matches_model(label, artifact)?;
    }
    identities.sort();
    identities.dedup();
    if identities.len() != output.artifacts().len() {
        return Err(TestkitError::new(format!(
            "{label} output identities were not distinct"
        )));
    }
    Ok(())
}

fn assert_compaction_artifact_matches_model(
    label: &'static str,
    artifact: &BuiltTableArtifact,
) -> Result<(), TestkitError> {
    let decoded = decode_immutable_table(artifact.bytes())
        .map_err(|err| TestkitError::new(format!("{label} decode failed: {err}")))?;
    let decoded_rows = decoded
        .rows()
        .iter()
        .cloned()
        .map(TableRow::new)
        .collect::<Vec<_>>();
    validate_strictly_sorted_unique_rows(&decoded_rows)
        .map_err(|err| TestkitError::new(format!("{label} artifact rows invalid: {err}")))?;
    let reader = ImmutableTableReader::open_bytes(
        artifact.facts().identity().clone(),
        artifact.bytes().to_vec(),
        TableReaderConfig::default(),
    )
    .map_err(|err| TestkitError::new(format!("{label} reader open failed: {err}")))?;
    let reader_rows = reader
        .rows()
        .iter()
        .map(|row| row.row().clone())
        .collect::<Vec<_>>();
    if decoded.rows().is_empty()
        || artifact.facts().row_count() != decoded.rows().len() as u64
        || artifact.facts().byte_count() != artifact.bytes().len() as u64
        || reader_rows.as_slice() != decoded.rows()
    {
        return Err(TestkitError::new(format!("{label} artifact facts drifted")));
    }
    assert_compaction_artifact_ranges_match_model(label, artifact, &decoded_rows, decoded.rows())
}

fn assert_compaction_artifact_ranges_match_model(
    label: &'static str,
    artifact: &BuiltTableArtifact,
    decoded_rows: &[TableRow],
    storage_rows: &[StorageRow],
) -> Result<(), TestkitError> {
    let first_row = decoded_rows
        .first()
        .ok_or_else(|| TestkitError::new(format!("{label} artifact had no first row")))?;
    let last_row = decoded_rows
        .last()
        .ok_or_else(|| TestkitError::new(format!("{label} artifact had no last row")))?;
    if artifact.facts().key_range().first_key() != first_row.encoded_key()
        || artifact.facts().key_range().last_key() != last_row.encoded_key()
    {
        return Err(TestkitError::new(format!("{label} key range drifted")));
    }
    let commit_min = storage_rows
        .iter()
        .map(StorageRow::commit_version)
        .min()
        .ok_or_else(|| TestkitError::new(format!("{label} commit min missing")))?;
    let commit_max = storage_rows
        .iter()
        .map(StorageRow::commit_version)
        .max()
        .ok_or_else(|| TestkitError::new(format!("{label} commit max missing")))?;
    if artifact.facts().commit_range().min() != commit_min
        || artifact.facts().commit_range().max() != commit_max
    {
        return Err(TestkitError::new(format!("{label} commit range drifted")));
    }
    Ok(())
}

fn assert_compaction_outputs_byte_identical(
    label: &'static str,
    left: &crate::table::TableCompactionOutput,
    right: &crate::table::TableCompactionOutput,
) -> Result<(), TestkitError> {
    let left_bytes = left
        .artifacts()
        .iter()
        .map(BuiltTableArtifact::bytes)
        .collect::<Vec<_>>();
    let right_bytes = right
        .artifacts()
        .iter()
        .map(BuiltTableArtifact::bytes)
        .collect::<Vec<_>>();
    if left_bytes != right_bytes {
        return Err(TestkitError::new(format!(
            "{label} did not produce byte-identical artifacts"
        )));
    }
    Ok(())
}

fn compaction_output_rows(
    output: &crate::table::TableCompactionOutput,
) -> Result<Vec<StorageRow>, TestkitError> {
    let mut rows = Vec::new();
    for artifact in output.artifacts() {
        let decoded = decode_immutable_table(artifact.bytes()).map_err(|err| {
            TestkitError::new(format!("compaction artifact decode failed: {err}"))
        })?;
        rows.extend_from_slice(decoded.rows());
    }
    Ok(rows)
}

fn assert_compaction_rejects_duplicate_and_output_limit(script: &[u8]) -> Result<(), TestkitError> {
    let duplicate = TableRow::new(StorageRow::put(
        deterministic_physical_key(0xd1, "compact", 0x20, b"duplicate".to_vec())?,
        CommitVersion::new(1),
        Timestamp::from_micros(1),
        Timestamp::EPOCH,
        vec![1],
    ));
    let sources = vec![
        TableCompactionSource::from_rows(
            TableCompactionSourceId::new("left")
                .map_err(|err| TestkitError::new(format!("left id failed: {err}")))?,
            vec![duplicate.clone()],
        )
        .map_err(|err| TestkitError::new(format!("left duplicate setup failed: {err}")))?,
        TableCompactionSource::from_rows(
            TableCompactionSourceId::new("right")
                .map_err(|err| TestkitError::new(format!("right id failed: {err}")))?,
            vec![duplicate],
        )
        .map_err(|err| TestkitError::new(format!("right duplicate setup failed: {err}")))?,
    ];
    let mut keep_all = KeepAllTableCompactionPolicy;
    match generated_compactor(script, 96, 4)?.compact(
        &TableIdentity::new("compact-duplicate")
            .map_err(|err| TestkitError::new(format!("duplicate identity failed: {err}")))?,
        &sources,
        &mut keep_all,
    ) {
        Err(TableRuntimeError::DuplicateInternalKey { .. }) => {}
        Err(err) => {
            return Err(TestkitError::new(format!(
                "expected duplicate compaction error; got {err}"
            )));
        }
        Ok(_) => return Err(TestkitError::new("duplicate compaction input was accepted")),
    }

    let rows = [
        TableRow::new(StorageRow::put(
            deterministic_physical_key(0xd2, "compact", 0x20, b"alpha".to_vec())?,
            CommitVersion::new(1),
            Timestamp::from_micros(1),
            Timestamp::EPOCH,
            vec![1; 64],
        )),
        TableRow::new(StorageRow::put(
            deterministic_physical_key(0xd3, "compact", 0x20, b"bravo".to_vec())?,
            CommitVersion::new(1),
            Timestamp::from_micros(1),
            Timestamp::EPOCH,
            vec![2; 64],
        )),
    ];
    let source = TableCompactionSource::from_rows(
        TableCompactionSourceId::new("limit")
            .map_err(|err| TestkitError::new(format!("limit id failed: {err}")))?,
        rows.to_vec(),
    )
    .map_err(|err| TestkitError::new(format!("limit source setup failed: {err}")))?;
    let limited = TableCompactor::new(
        TableCompactionConfig::new(1, 1)
            .map_err(|err| TestkitError::new(format!("limit config failed: {err}")))?,
        TableBuilderConfig::new(1, 1, TableCompression::Uncompressed)
            .map_err(|err| TestkitError::new(format!("limit builder config failed: {err}")))?,
    )
    .map_err(|err| TestkitError::new(format!("limit compactor failed: {err}")))?;
    match limited.compact(
        &TableIdentity::new("compact-limit")
            .map_err(|err| TestkitError::new(format!("limit identity failed: {err}")))?,
        &[source],
        &mut keep_all,
    ) {
        Err(TableRuntimeError::InvalidRange {
            field: "max_output_tables",
        }) => {}
        Err(err) => {
            return Err(TestkitError::new(format!(
                "expected max-output compaction error; got {err}"
            )));
        }
        Ok(_) => return Err(TestkitError::new("compaction output limit was accepted")),
    }

    Ok(())
}

fn generated_cache_key(
    table: &TableCacheTableId,
    kind: TableBlockCacheKind,
    offset: u64,
    length: u32,
    ordinal: Option<u32>,
) -> Result<TableBlockCacheKey, TestkitError> {
    let address = TableBlockAddress::new(kind, offset, length, ordinal)
        .map_err(|err| TestkitError::new(format!("cache address setup failed: {err}")))?;
    Ok(TableBlockCacheKey::new(table.clone(), address))
}

fn arc_bytes(byte: u8, len: usize) -> Arc<[u8]> {
    Arc::<[u8]>::from(vec![byte; len])
}

fn expect_cache_inserted(result: TableRuntimeResult<CacheInsert>) -> Result<(), TestkitError> {
    match result.map_err(|err| TestkitError::new(format!("cache insert failed: {err}")))? {
        CacheInsert::Inserted(_) => Ok(()),
        other => Err(TestkitError::new(format!(
            "expected cache insert; got {other:?}"
        ))),
    }
}

fn expect_cache_duplicate(
    result: TableRuntimeResult<CacheInsert>,
    byte: u8,
    len: usize,
) -> Result<(), TestkitError> {
    match result.map_err(|err| TestkitError::new(format!("cache duplicate failed: {err}")))? {
        CacheInsert::DuplicateExisting(bytes) if bytes.as_ref() == vec![byte; len].as_slice() => {
            Ok(())
        }
        other => Err(TestkitError::new(format!(
            "expected cache duplicate; got {other:?}"
        ))),
    }
}

fn expect_cache_hit(
    cache: &TableBlockCache,
    key: &TableBlockCacheKey,
    byte: u8,
    len: usize,
) -> Result<(), TestkitError> {
    match cache.get(key) {
        Some(bytes) if bytes.as_ref() == vec![byte; len].as_slice() => Ok(()),
        Some(_) => Err(TestkitError::new("cache hit returned wrong bytes")),
        None => Err(TestkitError::new("expected cache hit")),
    }
}

fn assert_immutable_reader_matches_model(
    label: &'static str,
    reader: &ImmutableTableReader,
    artifact: &BuiltTableArtifact,
    rows: &[TableRow],
    expected_rows: &[StorageRow],
    expected_config: TableReaderConfig,
) -> Result<(), TestkitError> {
    if reader.config() != expected_config
        || reader.facts() != artifact.facts()
        || reader.byte_count() != artifact.byte_count()
    {
        return Err(TestkitError::new(format!("{label} reader facts drifted")));
    }

    let actual_rows = reader
        .rows()
        .iter()
        .map(|row| row.row().clone())
        .collect::<Vec<_>>();
    if actual_rows != expected_rows {
        return Err(TestkitError::new(format!(
            "{label} rows did not match generated model"
        )));
    }

    let expected_keys = rows.iter().map(table_row_key_bytes).collect::<Vec<_>>();
    let mut cursor = reader.cursor();
    cursor
        .seek_to_first()
        .map_err(|err| TestkitError::new(format!("{label} cursor seek failed: {err}")))?;
    assert_cursor_keys(label, &mut cursor, &expected_keys)?;

    for row in [rows.first(), rows.get(rows.len() / 2), rows.last()]
        .into_iter()
        .flatten()
    {
        let exact = reader.get_exact(row.key());
        if exact.as_ref() != Some(row) {
            return Err(TestkitError::new(format!(
                "{label} exact lookup missed generated row"
            )));
        }
    }

    let absent = TableInternalKeyBytes::from_row(&StorageRow::put(
        deterministic_physical_key(0xfe, "reader", 0x20, b"absent".to_vec())?,
        CommitVersion::new(u64::MAX),
        Timestamp::from_micros(u64::MAX),
        Timestamp::EPOCH,
        Vec::new(),
    ));
    let missing = reader.get_exact(&absent);
    if missing.is_some() {
        return Err(TestkitError::new(format!(
            "{label} returned a row for an absent key"
        )));
    }

    let target = rows[rows.len() / 2].key().clone();
    let mut cursor = reader.cursor();
    cursor
        .seek(&target)
        .map_err(|err| TestkitError::new(format!("{label} cursor target seek failed: {err}")))?;
    let expected_seek = rows
        .iter()
        .find(|row| row.key() >= &target)
        .map(table_row_key_bytes);
    if cursor.current_key().map(|key| key.as_slice().to_vec()) != expected_seek {
        return Err(TestkitError::new(format!(
            "{label} target seek did not match generated model"
        )));
    }

    let lower = rows[rows.len() / 3].key().clone();
    let upper = rows[(rows.len() * 2) / 3].key().clone();
    let bounds = TableKeyBounds::closed(lower, upper)
        .map_err(|err| TestkitError::new(format!("{label} bounds setup failed: {err}")))?;
    assert_reader_bounds_match_model(label, reader, bounds, rows)?;

    let prefix = TablePhysicalKeyBytes::from_row(rows[0].row());
    assert_reader_bounds_match_model(
        label,
        reader,
        TableKeyBounds::prefix(prefix.as_slice().to_vec()),
        rows,
    )?;

    Ok(())
}

fn assert_reader_matches_decode(
    label: &'static str,
    reader: &ImmutableTableReader,
    bytes: &[u8],
) -> Result<(), TestkitError> {
    let decoded = decode_immutable_table(bytes)
        .map_err(|err| TestkitError::new(format!("{label} decode failed after open: {err}")))?;
    let actual_rows = reader
        .rows()
        .iter()
        .map(|row| row.row().clone())
        .collect::<Vec<_>>();
    if actual_rows.as_slice() != decoded.rows() {
        return Err(TestkitError::new(format!(
            "{label} rows did not match decoded table"
        )));
    }

    let properties = decoded.properties();
    let facts = reader.facts();
    let byte_count =
        u64::try_from(bytes.len()).expect("table fuzz input length fits in u64 on this platform");
    if facts.byte_count() != byte_count
        || facts.row_count() != properties.row_count()
        || facts.data_block_count() != decoded.header().data_block_count()
        || facts.key_range().first_key() != properties.min_key_bytes()
        || facts.key_range().last_key() != properties.max_key_bytes()
        || facts.commit_range().min() != properties.commit_min()
        || facts.commit_range().max() != properties.commit_max()
    {
        return Err(TestkitError::new(format!(
            "{label} trusted facts did not match decoded table"
        )));
    }

    Ok(())
}

fn assert_reader_bounds_match_model(
    label: &'static str,
    reader: &ImmutableTableReader,
    bounds: TableKeyBounds,
    rows: &[TableRow],
) -> Result<(), TestkitError> {
    let expected = rows
        .iter()
        .filter(|row| bounds.contains_key(row.key()))
        .map(table_row_key_bytes)
        .collect::<Vec<_>>();
    let mut cursor = reader.bounded_cursor(bounds);
    cursor
        .seek_to_first()
        .map_err(|err| TestkitError::new(format!("{label} bounded reader seek failed: {err}")))?;
    assert_cursor_keys(label, &mut cursor, &expected)
}

struct ShortTableSource {
    bytes: Vec<u8>,
}

impl ShortTableSource {
    fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}

impl TableByteSource for ShortTableSource {
    fn byte_count(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&self, offset: u64, len: usize) -> TableRuntimeResult<Vec<u8>> {
        let start = usize::try_from(offset).map_err(|_| TableRuntimeError::InvalidRange {
            field: "byte_offset",
        })?;
        let end = start
            .checked_add(len)
            .ok_or(TableRuntimeError::InvalidRange {
                field: "byte_range",
            })?
            .min(self.bytes.len());
        if end == start {
            return Ok(Vec::new());
        }
        Ok(self.bytes[start..end - 1].to_vec())
    }
}

fn expect_invalid_config<T>(result: Result<T, TableRuntimeError>) -> Result<(), TestkitError> {
    match result {
        Err(TableRuntimeError::InvalidConfig { .. }) => Ok(()),
        Err(err) => Err(TestkitError::new(format!(
            "expected invalid config error; got {err}"
        ))),
        Ok(_) => Err(TestkitError::new("invalid config was accepted")),
    }
}

fn expect_invalid_range<T>(result: Result<T, TableRuntimeError>) -> Result<(), TestkitError> {
    match result {
        Err(TableRuntimeError::InvalidRange { .. }) => Ok(()),
        Err(err) => Err(TestkitError::new(format!(
            "expected invalid range error; got {err}"
        ))),
        Ok(_) => Err(TestkitError::new(
            "invalid table range or facts were accepted",
        )),
    }
}

fn script_byte(script: &[u8], index: usize) -> u8 {
    script.get(index).copied().unwrap_or(0)
}

fn generated_physical_key(
    script: &[u8],
    offset: usize,
    user_key: Vec<u8>,
) -> Result<PhysicalKey, TestkitError> {
    let mut branch = [script_byte(script, offset); BranchId::BYTE_LEN];
    branch[BranchId::BYTE_LEN - 1] = script_byte(script, offset + 1);
    let raw_space_id = (script_byte(script, offset + 2) % 0xff) + 1;
    let storage_space_id = StorageSpaceId::from_raw(raw_space_id)
        .map_err(|err| TestkitError::new(format!("storage space id setup failed: {err}")))?;
    PhysicalKey::new(
        BranchId::from_bytes(branch),
        format!("space-{:02x}", script_byte(script, offset + 3)),
        storage_space_id,
        user_key,
    )
    .map_err(|err| TestkitError::new(format!("physical key setup failed: {err}")))
}

fn deterministic_physical_key(
    branch_byte: u8,
    space: &'static str,
    raw_space_id: u8,
    user_key: Vec<u8>,
) -> Result<PhysicalKey, TestkitError> {
    let storage_space_id = StorageSpaceId::from_raw(raw_space_id)
        .map_err(|err| TestkitError::new(format!("storage space id setup failed: {err}")))?;
    PhysicalKey::new(
        BranchId::from_bytes([branch_byte; BranchId::BYTE_LEN]),
        space,
        storage_space_id,
        user_key,
    )
    .map_err(|err| TestkitError::new(format!("physical key setup failed: {err}")))
}

fn generated_user_key(script: &[u8], offset: usize) -> Vec<u8> {
    let len = usize::from(script_byte(script, offset) % 8);
    (0..len)
        .map(|index| script_byte(script, offset + 1 + index))
        .collect()
}

fn generated_value(script: &[u8], offset: usize) -> Vec<u8> {
    let len = usize::from(script_byte(script, offset) % 16);
    (0..len)
        .map(|index| script_byte(script, offset + 1 + index))
        .collect()
}

fn table_key_bytes(row: &StorageRow) -> Vec<u8> {
    encode_internal_key(&InternalKey::new(
        row.physical_key().clone(),
        row.commit_version(),
    ))
}

fn table_row_key_bytes(row: &TableRow) -> Vec<u8> {
    row.encoded_key().to_vec()
}

fn absent_internal_key_for_model(
    present_key: &TableInternalKeyBytes,
    model: &BTreeMap<Vec<u8>, StorageRow>,
) -> Result<TableInternalKeyBytes, TestkitError> {
    let decoded = present_key
        .decode()
        .map_err(|err| TestkitError::new(format!("present key decode failed: {err}")))?;
    let versions = [
        CommitVersion::new(0),
        CommitVersion::new(decoded.commit_version().as_u64().saturating_add(10_000)),
        CommitVersion::MAX,
    ];

    for version in versions {
        let candidate =
            encode_internal_key(&InternalKey::new(decoded.physical_key().clone(), version));
        if !model.contains_key(&candidate) {
            return TableInternalKeyBytes::from_canonical_bytes(candidate).map_err(|err| {
                TestkitError::new(format!("absent model key was not canonical: {err}"))
            });
        }
    }

    Err(TestkitError::new(
        "generated model exhausted absent key candidates",
    ))
}

fn sample_closed_range_from_model(
    model: &BTreeMap<Vec<u8>, StorageRow>,
) -> Result<(TableKeyBounds, Vec<Vec<u8>>), TestkitError> {
    let keys = model.keys().cloned().collect::<Vec<_>>();
    if keys.is_empty() {
        return Err(TestkitError::new("generated model was empty"));
    }

    let lower_key = keys[keys.len() / 3].clone();
    let upper_key = keys[(keys.len() * 2) / 3].clone();
    let lower = TableInternalKeyBytes::from_canonical_bytes(lower_key.clone())
        .map_err(|err| TestkitError::new(format!("range lower key was not canonical: {err}")))?;
    let upper = TableInternalKeyBytes::from_canonical_bytes(upper_key.clone())
        .map_err(|err| TestkitError::new(format!("range upper key was not canonical: {err}")))?;
    let bounds = TableKeyBounds::closed(lower, upper)
        .map_err(|err| TestkitError::new(format!("generated range was rejected: {err}")))?;
    let expected = keys
        .into_iter()
        .filter(|key| key >= &lower_key && key <= &upper_key)
        .collect::<Vec<_>>();
    Ok((bounds, expected))
}

fn generated_model_row(script: &[u8], index: usize) -> Result<StorageRow, TestkitError> {
    let mut user_key = generated_user_key(script, 112 + (index % 16));
    user_key.extend_from_slice(&(index as u64).to_le_bytes());
    if index % 5 == 0 {
        user_key.push(0x00);
    }
    if index % 7 == 0 {
        user_key.extend_from_slice(&[0x00, 0x00, 0xff]);
    }

    let physical_key = generated_physical_key(script, 128 + (index % 16), user_key)?;
    let version = CommitVersion::new(u64::from(script_byte(script, 144 + (index % 16))) + 1);
    let timestamp = Timestamp::from_micros(
        u64::from(script_byte(script, 160 + (index % 16))) + u64::try_from(index).unwrap_or(0),
    );
    if script_byte(script, 176 + (index % 16)) % 5 == 0 {
        Ok(StorageRow::tombstone(physical_key, version, timestamp))
    } else {
        Ok(StorageRow::put(
            physical_key,
            version,
            timestamp,
            Timestamp::from_micros(u64::from(script_byte(script, 192 + (index % 16)))),
            generated_value(script, 208 + (index % 16)),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        check_table_runtime_compaction_contract, check_table_runtime_cursor_contract,
        check_table_runtime_reader_contract, check_table_runtime_scaffold_contract,
        generated_builder_table_rows, generated_compaction_row_count, generated_compaction_sources,
        generated_compaction_table_rows, ImmutableTableBuilder, TableBuilderConfig, TableIdentity,
        GENERATED_COMPACTION_MAX_ROWS, GENERATED_COMPACTION_MAX_SOURCES,
    };

    #[test]
    fn table_runtime_scaffold_contract_checks_generated_scripts() {
        let outcome = check_table_runtime_scaffold_contract(&[
            64, 16, 1, 0, 32, 128, 4, 1, 5, 2, 200, 7, 3, 0xaa, 0x10, 0x20, 1, 2, 3, 4, 5, 6,
        ])
        .expect("scaffold contract");

        assert_eq!(outcome.valid_config_cases(), 1);
        assert_eq!(outcome.invalid_config_cases(), 5);
        assert_eq!(outcome.valid_fact_cases(), 1);
        assert_eq!(outcome.invalid_fact_cases(), 9);
        assert_eq!(outcome.row_key_adapter_cases(), 1);
        assert_eq!(outcome.invalid_row_key_sequence_cases(), 3);
        assert_eq!(outcome.key_bound_cases(), 1);
        assert_eq!(outcome.size_accounting_cases(), 1);
        assert_eq!(outcome.mutable_frozen_table_cases(), 1);
        assert_eq!(outcome.raw_cursor_cases(), 1);
        assert_eq!(outcome.immutable_builder_artifact_cases(), 1);
        assert_eq!(outcome.immutable_table_reader_cases(), 1);
        assert_eq!(outcome.object_backed_table_reader_cases(), 1);
        assert_eq!(outcome.table_block_cache_cases(), 1);
        assert_eq!(outcome.table_bloom_filter_cases(), 1);
        assert_eq!(outcome.table_compaction_cases(), 1);
        assert_eq!(outcome.error_source_cases(), 1);
        assert_eq!(outcome.stats_cases(), 1);
    }

    #[test]
    fn table_runtime_compaction_generator_covers_planned_source_and_row_limits() {
        let mut script = vec![0_u8; 256];
        script[88] = GENERATED_COMPACTION_MAX_SOURCES - 1;
        let max_rows = u16::try_from(GENERATED_COMPACTION_MAX_ROWS)
            .expect("generated compaction max rows fits u16");
        script[93..95].copy_from_slice(&max_rows.to_le_bytes());

        assert_eq!(generated_compaction_row_count(&[]), 0);
        assert_eq!(
            generated_compaction_row_count(&script),
            GENERATED_COMPACTION_MAX_ROWS
        );

        let rows = generated_compaction_table_rows(&script).expect("max generated rows");
        assert_eq!(rows.len(), GENERATED_COMPACTION_MAX_ROWS);
        let sources = generated_compaction_sources(
            &rows,
            usize::from(GENERATED_COMPACTION_MAX_SOURCES),
            "max",
        )
        .expect("max generated sources");
        assert_eq!(sources.len(), usize::from(GENERATED_COMPACTION_MAX_SOURCES));
        assert_eq!(
            sources
                .iter()
                .map(super::TableCompactionSource::len)
                .sum::<usize>(),
            GENERATED_COMPACTION_MAX_ROWS
        );
    }

    #[test]
    fn dedicated_table_runtime_fuzz_contracts_exercise_their_surfaces() {
        let script = [
            64, 16, 1, 0, 32, 128, 4, 1, 5, 2, 200, 7, 3, 0xaa, 0x10, 0x20, 1, 2, 3, 4, 5, 6,
        ];

        let rows = generated_builder_table_rows(&script).expect("reader rows");
        let builder =
            ImmutableTableBuilder::new(TableBuilderConfig::default()).expect("reader builder");
        let artifact = builder
            .build_from_rows(
                TableIdentity::new("reader-contract").expect("reader identity"),
                &rows,
            )
            .expect("reader artifact");
        check_table_runtime_reader_contract(artifact.bytes()).expect("reader contract");
        check_table_runtime_reader_contract(b"not an immutable table").expect("reader rejection");

        check_table_runtime_cursor_contract(&script).expect("cursor contract");
        check_table_runtime_compaction_contract(&script).expect("compaction contract");
    }
}
