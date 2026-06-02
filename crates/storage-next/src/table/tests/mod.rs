use super::{
    TableBuilderConfig, TableCacheConfig, TableCommitRange, TableCompactionConfig, TableIdentity,
    TableKeyRange, TableReaderConfig, TableRuntimeConfig, TableRuntimeError, TableRuntimeFacts,
};
use crate::format::{FormatError, TableCompression};
use std::error::Error;
use strata_core_next::CommitVersion;

mod builder;
mod cache;
mod compaction;
mod cursor;
mod key;
mod mutable;
mod reader;

#[test]
fn table_runtime_default_config_constructs() {
    let config = TableRuntimeConfig::default();

    assert_eq!(config.builder().target_data_block_size(), 64 * 1024);
    assert_eq!(config.builder().rows_per_block(), 256);
    assert_eq!(
        config.builder().compression(),
        TableCompression::Uncompressed
    );
    assert_eq!(*config.reader(), TableReaderConfig::default());
    assert!(config.cache().enabled());
    assert_eq!(config.cache().capacity_bytes(), 64 * 1024 * 1024);
    assert_eq!(config.compaction().target_output_bytes(), 64 * 1024 * 1024);
    assert_eq!(config.compaction().max_output_tables(), 1_024);
}

#[test]
fn table_runtime_config_rejects_invalid_values() {
    assert_eq!(
        TableBuilderConfig::new(0, 1, TableCompression::Uncompressed),
        Err(TableRuntimeError::InvalidConfig {
            field: "target_data_block_size",
            reason: "must be nonzero",
        })
    );
    assert_eq!(
        TableBuilderConfig::new(1, 0, TableCompression::Uncompressed),
        Err(TableRuntimeError::InvalidConfig {
            field: "rows_per_block",
            reason: "must be nonzero",
        })
    );
    assert_eq!(
        TableCacheConfig::new(true, 0),
        Err(TableRuntimeError::InvalidConfig {
            field: "cache_capacity_bytes",
            reason: "enabled cache must have positive capacity",
        })
    );
    assert_eq!(
        TableCompactionConfig::new(0, 1),
        Err(TableRuntimeError::InvalidConfig {
            field: "target_output_bytes",
            reason: "must be nonzero",
        })
    );
    assert_eq!(
        TableCompactionConfig::new(1, 0),
        Err(TableRuntimeError::InvalidConfig {
            field: "max_output_tables",
            reason: "must be nonzero",
        })
    );
}

#[test]
fn table_runtime_config_accepts_explicit_valid_values() {
    let builder =
        TableBuilderConfig::new(1024, 16, TableCompression::Zstd).expect("valid builder config");
    let reader = TableReaderConfig::new();
    let cache = TableCacheConfig::new(false, 0).expect("disabled cache can have zero capacity");
    let compaction = TableCompactionConfig::new(4096, 4).expect("valid compaction config");

    let config =
        TableRuntimeConfig::new(builder, reader, cache, compaction).expect("valid table config");

    assert_eq!(config.builder().compression(), TableCompression::Zstd);
    assert_eq!(*config.reader(), TableReaderConfig::default());
    assert!(!config.cache().enabled());
}

#[test]
fn table_runtime_facts_construct_and_expose_fields() {
    let identity = TableIdentity::new("table-0001").expect("valid identity");
    let key_range = TableKeyRange::new([0x10], [0x20]).expect("valid key range");
    let commit_range = TableCommitRange::new(CommitVersion::new(7), CommitVersion::new(9))
        .expect("valid commit range");

    let facts =
        TableRuntimeFacts::new(identity.clone(), 3, 1, key_range.clone(), commit_range, 512)
            .expect("valid table facts");

    assert_eq!(facts.identity(), &identity);
    assert_eq!(facts.identity().as_str(), "table-0001");
    assert_eq!(facts.row_count(), 3);
    assert_eq!(facts.data_block_count(), 1);
    assert_eq!(facts.key_range(), &key_range);
    assert_eq!(facts.key_range().first_key(), &[0x10]);
    assert_eq!(facts.key_range().last_key(), &[0x20]);
    assert_eq!(facts.commit_range().min(), CommitVersion::new(7));
    assert_eq!(facts.commit_range().max(), CommitVersion::new(9));
    assert_eq!(facts.byte_count(), 512);
}

#[test]
fn table_runtime_facts_reject_impossible_values() {
    let identity = TableIdentity::new("table-0001").expect("valid identity");
    let key_range = TableKeyRange::new([0x10], [0x20]).expect("valid key range");
    let commit_range =
        TableCommitRange::new(CommitVersion::new(1), CommitVersion::new(1)).expect("range");

    assert_eq!(
        TableIdentity::new(""),
        Err(TableRuntimeError::InvalidConfig {
            field: "table_identity",
            reason: "must not be empty",
        })
    );
    assert_eq!(
        TableIdentity::new("table/0001"),
        Err(TableRuntimeError::InvalidConfig {
            field: "table_identity",
            reason: "must be an opaque single component",
        })
    );
    assert_eq!(
        TableKeyRange::new([0x20], [0x10]),
        Err(TableRuntimeError::InvalidRange { field: "key_range" })
    );
    assert_eq!(
        TableCommitRange::new(CommitVersion::new(2), CommitVersion::new(1)),
        Err(TableRuntimeError::InvalidRange {
            field: "commit_range",
        })
    );
    assert_eq!(
        TableRuntimeFacts::new(identity.clone(), 0, 1, key_range.clone(), commit_range, 1),
        Err(TableRuntimeError::InvalidRange { field: "row_count" })
    );
    assert_eq!(
        TableRuntimeFacts::new(identity.clone(), 1, 0, key_range.clone(), commit_range, 1),
        Err(TableRuntimeError::InvalidRange {
            field: "data_block_count",
        })
    );
    assert_eq!(
        TableRuntimeFacts::new(identity.clone(), 1, 2, key_range.clone(), commit_range, 1),
        Err(TableRuntimeError::InvalidRange {
            field: "data_block_count",
        })
    );
    assert_eq!(
        TableRuntimeFacts::new(identity, 1, 1, key_range, commit_range, 0),
        Err(TableRuntimeError::InvalidRange {
            field: "byte_count",
        })
    );
}

#[test]
fn table_runtime_error_display_and_sources_are_typed() {
    let format = FormatError::InvalidLength { field: "row_count" };
    let build = TableRuntimeError::BuildFormat {
        source: format.clone(),
    };
    let decode = TableRuntimeError::DecodeFormat {
        source: format.clone(),
    };
    let duplicate = TableRuntimeError::DuplicateInternalKey {
        key: vec![0xab, 0xcd],
    };
    let order = TableRuntimeError::InvalidRowOrder {
        previous: vec![0x02],
        current: vec![0x01],
    };
    let source_read = TableRuntimeError::source_read("range unavailable");
    let cache = TableRuntimeError::Cache {
        reason: "capacity exceeded",
    };
    let compaction = TableRuntimeError::CompactionPolicy {
        reason: "policy rejected row",
    };

    assert_eq!(
        build.source().map(ToString::to_string),
        Some(format.to_string())
    );
    assert_eq!(
        decode.source().map(ToString::to_string),
        Some(format.to_string())
    );
    assert!(build
        .to_string()
        .contains("failed to build immutable table"));
    assert!(decode
        .to_string()
        .contains("failed to decode immutable table"));
    assert!(duplicate.to_string().contains("abcd"));
    assert!(order.to_string().contains("02"));
    let long_duplicate = TableRuntimeError::DuplicateInternalKey {
        key: vec![0xab; 128],
    }
    .to_string();
    assert!(long_duplicate.contains("128 bytes"));
    assert!(long_duplicate.len() < 128);
    assert!(!long_duplicate.contains(&"ab".repeat(64)));
    assert!(source_read.to_string().contains("range unavailable"));
    assert!(cache.to_string().contains("capacity exceeded"));
    assert!(compaction.to_string().contains("policy rejected row"));
}
