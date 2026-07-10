#[cfg(feature = "perf-trace")]
fn check_table_perf_trace_contract(script: &[u8]) -> Result<(), TestkitError> {
    let rows = generated_builder_table_rows(script)?;
    let builder = ImmutableTableBuilder::new(
        TableBuilderConfig::new(128, 1, TableCompression::Uncompressed)
            .map_err(|err| TestkitError::new(format!("table perf-trace builder config failed: {err}")))?,
    )
    .map_err(|err| TestkitError::new(format!("table perf-trace builder setup failed: {err}")))?;
    let artifact = builder
        .build_from_rows(
            TableIdentity::new(format!("table-perf-{:02x}", script_byte(script, 216)))
                .map_err(|err| TestkitError::new(format!("table perf-trace identity setup failed: {err}")))?,
            &rows,
        )
        .map_err(|err| TestkitError::new(format!("table perf-trace table build failed: {err}")))?;

    #[cfg(test)]
    let _capture = crate::observability::perf_trace::begin_test_capture();
    #[cfg(not(test))]
    crate::observability::perf_trace::reset();

    let zero = crate::observability::perf_trace::snapshot();
    assert_table_zero_snapshot(&zero)?;

    let reader = ImmutableTableReader::open_source(
        TableIdentity::new(format!("table-reader-{:02x}", script_byte(script, 217)))
            .map_err(|err| TestkitError::new(format!("table perf-trace reader identity failed: {err}")))?,
        BytesTableSource::new(artifact.bytes().to_vec()),
        TableReaderConfig::default(),
    )
    .map_err(|err| TestkitError::new(format!("table perf-trace source reader open failed: {err}")))?;
    assert_table_reader_facts(&reader, &artifact)?;
    assert_table_open_snapshot(
        &crate::observability::perf_trace::snapshot(),
        &artifact,
    )?;

    let target = reader
        .rows()
        .get(reader.rows().len() / 2)
        .ok_or_else(|| TestkitError::new("table perf-trace reader had no generated target row"))?
        .physical_key()
        .clone();
    let (row, visited) = reader.seek_physical_key(&target, None, None);
    if row.is_none() || visited == 0 {
        return Err(TestkitError::new(
            "table perf-trace point lookup did not find the generated target row",
        ));
    }

    let mut cursor = reader.cursor();
    cursor
        .seek_to_first()
        .map_err(|err| TestkitError::new(format!("table perf-trace cursor seek failed: {err}")))?;
    cursor
        .advance()
        .map_err(|err| TestkitError::new(format!("table perf-trace cursor advance failed: {err}")))?;
    assert_table_query_snapshot(&crate::observability::perf_trace::snapshot())?;

    check_table_cache_counters(script)?;
    check_table_filter_counters(&reader)?;
    assert_table_accelerator_snapshot(&crate::observability::perf_trace::snapshot())
}

#[cfg(feature = "perf-trace")]
fn assert_table_zero_snapshot(
    snapshot: &crate::observability::perf_trace::StoragePerfSnapshot,
) -> Result<(), TestkitError> {
    if snapshot.table_reader_opens() != 0
        || snapshot.table_data_block_reads() != 0
        || snapshot.table_rows_decoded() != 0
        || snapshot.table_point_rows_visited() != 0
        || snapshot.table_cursor_rows_visited() != 0
        || snapshot.table_cache_hits() != 0
        || snapshot.table_cache_misses() != 0
        || snapshot.table_cache_inserts() != 0
        || snapshot.table_cache_skipped_inserts() != 0
        || snapshot.table_filter_probes() != 0
    {
        return Err(TestkitError::new("table perf-trace perf counters did not reset to zero"));
    }
    Ok(())
}

#[cfg(feature = "perf-trace")]
fn assert_table_reader_facts(
    reader: &ImmutableTableReader,
    _artifact: &BuiltTableArtifact,
) -> Result<(), TestkitError> {
    let runtime = reader.runtime_facts();
    if runtime.open_mode() != crate::table::TableReaderOpenMode::LazySource
        || !runtime.metadata_loaded()
        || !runtime.index_loaded()
        || runtime.filter_available()
        || runtime.cache_enabled()
        || runtime.data_blocks_loaded() != 0
        || runtime.rows_materialized() != 0
    {
        return Err(TestkitError::new(
            "table perf-trace reader runtime facts did not describe the lazy source open path",
        ));
    }
    Ok(())
}

#[cfg(feature = "perf-trace")]
fn assert_table_open_snapshot(
    snapshot: &crate::observability::perf_trace::StoragePerfSnapshot,
    _artifact: &BuiltTableArtifact,
) -> Result<(), TestkitError> {
    if snapshot.table_reader_opens() != 1
        || snapshot.table_metadata_read_bytes() == 0
        || snapshot.table_index_read_bytes() == 0
        || snapshot.table_properties_read_bytes() == 0
        || snapshot.table_data_block_reads() != 0
        || snapshot.table_data_block_read_bytes() != 0
        || snapshot.table_data_block_decodes() != 0
        || snapshot.table_rows_decoded() != 0
    {
        return Err(TestkitError::new(
            "table perf-trace source reader open counters did not stay lazy",
        ));
    }
    Ok(())
}

#[cfg(feature = "perf-trace")]
fn assert_table_query_snapshot(
    snapshot: &crate::observability::perf_trace::StoragePerfSnapshot,
) -> Result<(), TestkitError> {
    if snapshot.table_seeks() == 0
        || snapshot.table_point_rows_visited() == 0
        || snapshot.table_cursor_rows_visited() < 2
    {
        return Err(TestkitError::new(
            "table perf-trace query counters did not record seek and cursor work",
        ));
    }
    Ok(())
}

#[cfg(feature = "perf-trace")]
fn check_table_cache_counters(script: &[u8]) -> Result<(), TestkitError> {
    let cache = TableBlockCache::new(
        TableCacheConfig::new(true, 4)
            .map_err(|err| TestkitError::new(format!("table perf-trace cache config failed: {err}")))?,
    );
    let table = TableCacheTableId::new(vec![0x15, script_byte(script, 218)])
        .map_err(|err| TestkitError::new(format!("table perf-trace cache table id failed: {err}")))?;
    let key = generated_cache_key(&table, TableBlockCacheKind::Data, 0, 4, Some(0))?;
    if cache.get(&key).is_some() {
        return Err(TestkitError::new("table perf-trace cache miss unexpectedly hit"));
    }
    expect_cache_inserted(cache.insert(key.clone(), arc_bytes(script_byte(script, 219), 4)))?;
    expect_cache_hit(&cache, &key, script_byte(script, 219), 4)?;

    let oversized = generated_cache_key(&table, TableBlockCacheKind::Data, 4, 8, Some(1))?;
    match cache
        .insert(oversized, arc_bytes(script_byte(script, 220), 8))
        .map_err(|err| TestkitError::new(format!("table perf-trace oversized insert failed: {err}")))?
    {
        CacheInsert::SkippedOversized(_) => Ok(()),
        other => Err(TestkitError::new(format!(
            "table perf-trace oversized cache insert returned {other:?}"
        ))),
    }
}

#[cfg(feature = "perf-trace")]
fn check_table_filter_counters(reader: &ImmutableTableReader) -> Result<(), TestkitError> {
    let present = reader
        .rows()
        .first()
        .ok_or_else(|| TestkitError::new("table perf-trace reader had no bloom target row"))?
        .encoded_key();
    let filter = TableBloomFilter::build([present], 10)
        .map_err(|err| TestkitError::new(format!("table perf-trace bloom build failed: {err}")))?;
    if filter.might_contain(present) == TableBloomProbe::DefinitelyAbsent {
        return Err(TestkitError::new("table perf-trace bloom filter returned false negative"));
    }
    let empty = TableBloomFilter::build(Vec::<&[u8]>::new(), 10)
        .map_err(|err| TestkitError::new(format!("table perf-trace empty bloom build failed: {err}")))?;
    if empty.might_contain(b"definitely-absent") != TableBloomProbe::DefinitelyAbsent {
        return Err(TestkitError::new(
            "table perf-trace empty bloom filter did not return definitely absent",
        ));
    }
    Ok(())
}

#[cfg(feature = "perf-trace")]
fn assert_table_accelerator_snapshot(
    snapshot: &crate::observability::perf_trace::StoragePerfSnapshot,
) -> Result<(), TestkitError> {
    if snapshot.table_cache_misses() == 0
        || snapshot.table_cache_inserts() == 0
        || snapshot.table_cache_hits() == 0
        || snapshot.table_cache_skipped_inserts() == 0
        || snapshot.table_filter_probes() < 2
        || snapshot.table_filter_positive_probes() == 0
        || snapshot.table_filter_negative_probes() == 0
        || snapshot.table_filter_absent_probes() != 0
    {
        return Err(TestkitError::new(
            "table perf-trace cache/filter counters did not distinguish the expected outcomes",
        ));
    }
    Ok(())
}
