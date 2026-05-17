use crate::format::{encode_internal_key, FormatError, TableCompression};
use crate::row::{InternalKey, PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    sort_table_rows_by_key, validate_strictly_sorted_unique_rows, BoundedTableCursor,
    CursorMergePath, FrozenTable, MergeTableCursor, MutableTable, TableBuilderConfig,
    TableCacheConfig, TableCommitRange, TableCompactionConfig, TableCursor, TableIdentity,
    TableInternalKeyBytes, TableKeyBound, TableKeyBounds, TableKeyRange, TableMemoryFacts,
    TablePhysicalKeyBytes, TableReaderConfig, TableRow, TableRuntimeConfig, TableRuntimeError,
    TableRuntimeFacts, TableRuntimeStats, MERGE_HEAP_THRESHOLD,
};
use std::collections::{btree_map::Entry, BTreeMap};
use std::error::Error;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

use super::TestkitError;

type CursorKeyValue = (Vec<u8>, Vec<u8>);
type MergeModelItem = (Vec<u8>, usize, Vec<u8>);

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

    check_error_source_chain()?;
    outcome.error_sources += 1;

    check_stats(script)?;
    outcome.stats += 1;

    Ok(outcome)
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
    let reader = TableReaderConfig::new(cache_enabled, script_byte(script, 7) & 1 == 0);
    let cache = TableCacheConfig::new(cache_enabled, cache_capacity)
        .map_err(|err| TestkitError::new(format!("valid cache config rejected: {err}")))?;
    let compaction = TableCompactionConfig::new(target_output_bytes, max_output_tables)
        .map_err(|err| TestkitError::new(format!("valid compaction config rejected: {err}")))?;
    let runtime = TableRuntimeConfig::new(builder, reader, cache, compaction)
        .map_err(|err| TestkitError::new(format!("valid runtime config rejected: {err}")))?;

    if runtime.builder().target_data_block_size() != target_data_block_size
        || runtime.builder().rows_per_block() != rows_per_block
        || runtime.builder().compression() != compression
        || runtime.reader().cache_enabled() != cache_enabled
        || runtime.reader().validate_on_open() != (script_byte(script, 7) & 1 == 0)
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
    use super::check_table_runtime_scaffold_contract;

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
        assert_eq!(outcome.error_source_cases(), 1);
        assert_eq!(outcome.stats_cases(), 1);
    }
}
