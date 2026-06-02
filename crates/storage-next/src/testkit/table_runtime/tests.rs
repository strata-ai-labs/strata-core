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
