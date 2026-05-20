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
