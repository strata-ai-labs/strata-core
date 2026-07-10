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
    let before_duplicate_keys = table
        .iter()
        .map(|row| table_row_key_bytes(row.as_ref()))
        .collect::<Vec<_>>();
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
        || table
            .iter()
            .map(|row| table_row_key_bytes(row.as_ref()))
            .collect::<Vec<_>>()
            != before_duplicate_keys
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
    if table.get(&first_table_key).as_deref().map(TableRow::row) != model.get(first_key) {
        return Err(TestkitError::new("mutable exact lookup drifted"));
    }
    let absent_key = absent_internal_key_for_model(&first_table_key, model)?;
    if table.get(&absent_key).is_some() {
        return Err(TestkitError::new("mutable absent exact lookup hit a row"));
    }

    let bounds = TableKeyBounds::exact(first_table_key.clone());
    let bounded = table
        .rows_in_bounds(&bounds)
        .map(|row| table_row_key_bytes(row.as_ref()))
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
        .map(|row| table_row_key_bytes(row.as_ref()))
        .collect::<Vec<_>>();
    if actual_range != expected_range {
        return Err(TestkitError::new("mutable range bounds drifted"));
    }

    let prefix = TablePhysicalKeyBytes::from_row(table.get(&first_table_key).expect("first").row());
    let prefix_bytes = prefix.as_slice().to_vec();
    let actual_prefix = table
        .rows_with_physical_prefix(&prefix)
        .map(|row| table_row_key_bytes(row.as_ref()))
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
    if table.get(&first_table_key).as_deref().map(TableRow::row) != model.get(first_key) {
        return Err(TestkitError::new("frozen exact lookup drifted"));
    }
    let absent_key = absent_internal_key_for_model(&first_table_key, model)?;
    if table.get(&absent_key).is_some() {
        return Err(TestkitError::new("frozen absent exact lookup hit a row"));
    }
    if table
        .rows_in_bounds(&TableKeyBounds::exact(first_table_key))
        .map(|row| table_row_key_bytes(row.as_ref()))
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
        .map(|row| table_row_key_bytes(row.as_ref()))
        .collect::<Vec<_>>()
        != expected_range
    {
        return Err(TestkitError::new("frozen range bounds drifted"));
    }

    let prefix = TablePhysicalKeyBytes::from_row(table.iter().next().expect("first row").row());
    let prefix_bytes = prefix.as_slice().to_vec();
    let actual_prefix = table
        .rows_with_physical_prefix(&prefix)
        .map(|row| table_row_key_bytes(row.as_ref()))
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

fn assert_memory_table_matches_model(
    label: &'static str,
    rows: impl Iterator<Item = Arc<TableRow>>,
    model: &BTreeMap<Vec<u8>, StorageRow>,
    expected_bytes: usize,
) -> Result<(), TestkitError> {
    let rows = rows.collect::<Vec<_>>();
    let actual_keys = rows
        .iter()
        .map(|row| table_row_key_bytes(row.as_ref()))
        .collect::<Vec<_>>();
    let expected_keys = model.keys().cloned().collect::<Vec<_>>();
    if actual_keys != expected_keys {
        return Err(TestkitError::new(format!(
            "{label} table iteration did not match generated model"
        )));
    }
    for row in &rows {
        let key = table_row_key_bytes(row.as_ref());
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
    let expected = table
        .iter()
        .map(|row| table_row_key_bytes(row.as_ref()))
        .collect::<Vec<_>>();

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
        .map(|row| table_row_key_bytes(row.as_ref()))
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
