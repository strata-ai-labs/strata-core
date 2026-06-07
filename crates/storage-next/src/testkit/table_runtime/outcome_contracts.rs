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
    l5a_perf_traces: usize,
    error_sources: usize,
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

    /// Number of M4P-L5A table-local perf-trace cases exercised.
    pub const fn l5a_perf_trace_cases(self) -> usize {
        self.l5a_perf_traces
    }

    /// Number of error source-chain cases exercised.
    pub const fn error_source_cases(self) -> usize {
        self.error_sources
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
        l5a_perf_traces: 0,
        error_sources: 0,
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
    #[cfg(feature = "perf-trace")]
    {
        check_l5a_perf_trace_contract(script)?;
        outcome.l5a_perf_traces += 1;
    }

    check_error_source_chain()?;
    outcome.error_sources += 1;

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
