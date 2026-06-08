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
    let dense = TableBloomFilter::build(
        [
            b"false-positive-source-a".as_slice(),
            b"false-positive-source-b".as_slice(),
            b"false-positive-source-c".as_slice(),
        ],
        1,
    )
    .map_err(|err| TestkitError::new(format!("dense bloom build failed: {err}")))?;
    let false_positive_found = (0u8..=u8::MAX).any(|byte| {
        let candidate = [byte];
        dense.might_contain(candidate.as_slice()) == TableBloomProbe::MaybePresent
    });
    if !false_positive_found {
        return Err(TestkitError::new(
            "dense bloom filter did not expose a deterministic false-positive path",
        ));
    }
    expect_invalid_config(TableBloomFilter::build([b"key".as_slice()], 0))?;
    Ok(())
}

fn keep_all_policy() -> impl crate::table::TableCompactionPolicy {
    |_: &crate::table::TableCompactionRowContext<'_>, _: &TableRow| {
        Ok(TableCompactionDecision::Keep)
    }
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
    let mut keep_all = keep_all_policy();
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
    let mut repeat_keep_all = keep_all_policy();
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
    let mut regrouped_keep_all = keep_all_policy();
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
    let mut keep_all = keep_all_policy();
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
