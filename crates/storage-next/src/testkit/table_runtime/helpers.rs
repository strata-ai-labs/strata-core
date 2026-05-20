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

