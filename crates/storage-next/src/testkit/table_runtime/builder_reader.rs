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
