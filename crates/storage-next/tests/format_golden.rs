//! Durable format golden-vector harness entry point.

#![deny(unsafe_code)]

mod common;

use std::fs;

#[test]
fn format_golden_harness_has_storage_format_directory() {
    let dir = common::storage_format_goldens_dir();

    assert!(dir.is_dir());
    for file in [
        "internal-key-ordinary.hex",
        "internal-key-zero-user-byte.hex",
        "manifest-identity.hex",
        "quarantine-inventory-empty.hex",
        "quarantine-inventory-multi-entry.hex",
        "segment-metadata-sidecar.hex",
        "snapshot-container-single-section.hex",
        "snapshot-header-identity.hex",
        "snapshot-section-empty.hex",
        "snapshot-watermark-empty.hex",
        "snapshot-watermark-present.hex",
        "storage-row-put.hex",
        "storage-row-tombstone.hex",
        "table-manifest-empty.hex",
        "table-manifest-owned-levels.hex",
        "table-manifest-inherited-layers.hex",
        "table-manifest-materialization-provenance.hex",
        "table-manifest-extension-section.hex",
        "immutable-table-one-block.hex",
        "immutable-table-two-block.hex",
        "table-data-block-one-put-uncompressed-frame.hex",
        "table-data-block-put-tombstone-uncompressed-frame.hex",
        "table-data-block-zstd-frame.hex",
        "table-index-block.hex",
        "table-properties-block.hex",
        "wal-commit-payload-one-put.hex",
        "wal-commit-payload-put-tombstone.hex",
        "wal-record-empty-pre-m3f.hex",
        "wal-record-envelope.hex",
        "wal-record-payload.hex",
        "wal-segment-header.hex",
    ] {
        assert!(dir.join(file).is_file(), "missing golden vector {file}");
    }
}

#[test]
fn format_fuzz_corpus_matches_golden_vectors() {
    let cases = [
        (
            "quarantine-inventory-empty.hex",
            "format_quarantine",
            "quarantine-inventory-empty",
        ),
        (
            "quarantine-inventory-multi-entry.hex",
            "format_quarantine",
            "quarantine-inventory-multi-entry",
        ),
        (
            "wal-commit-payload-one-put.hex",
            "format_wal_commit_payload",
            "one-put",
        ),
        (
            "wal-commit-payload-put-tombstone.hex",
            "format_wal_commit_payload",
            "put-tombstone",
        ),
        (
            "table-data-block-one-put-uncompressed-frame.hex",
            "format_table_block",
            "one-put-uncompressed",
        ),
        (
            "table-data-block-zstd-frame.hex",
            "format_table_block",
            "zstd",
        ),
        (
            "immutable-table-one-block.hex",
            "format_table_artifact",
            "one-block",
        ),
        (
            "immutable-table-two-block.hex",
            "format_table_artifact",
            "two-block",
        ),
        (
            "table-manifest-empty.hex",
            "format_table_manifest",
            "valid-empty",
        ),
        (
            "table-manifest-owned-levels.hex",
            "format_table_manifest",
            "valid-owned-levels",
        ),
        (
            "table-manifest-inherited-layers.hex",
            "format_table_manifest",
            "valid-inherited-layer",
        ),
        (
            "table-manifest-materialization-provenance.hex",
            "format_table_manifest",
            "valid-materialization-provenance",
        ),
        (
            "table-manifest-extension-section.hex",
            "format_table_manifest",
            "valid-unknown-optional-extension",
        ),
    ];

    let goldens = common::storage_format_goldens_dir();
    let corpus = common::crate_root().join("fuzz/corpus");

    for (golden_name, corpus_dir, corpus_name) in cases {
        let golden = parse_hex_fixture(&fs::read_to_string(goldens.join(golden_name)).unwrap());
        let seed = fs::read(corpus.join(corpus_dir).join(corpus_name)).unwrap();

        assert_eq!(seed, golden, "fuzz corpus seed drifted from {golden_name}");
    }
}

#[test]
fn format_table_manifest_fuzz_corpus_has_required_semantic_seeds() {
    let corpus = common::crate_root().join("fuzz/corpus/format_table_manifest");
    let seeds = [
        "valid-empty",
        "valid-owned-levels",
        "valid-inherited-layer",
        "valid-materialization-provenance",
        "valid-unknown-optional-extension",
        "bad-checksum",
        "future-version",
        "truncated-table-entry",
    ];

    for seed in seeds {
        let path = corpus.join(seed);
        let bytes = fs::read(&path).unwrap_or_else(|err| {
            panic!("missing or unreadable table-manifest corpus seed {seed}: {err}")
        });
        assert!(!bytes.is_empty(), "empty table-manifest corpus seed {seed}");
    }
}

fn parse_hex_fixture(text: &str) -> Vec<u8> {
    let hex: String = text
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .flat_map(str::chars)
        .filter(|ch| !ch.is_whitespace())
        .collect();

    assert_eq!(hex.len() % 2, 0, "hex fixture has odd byte count");
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
        .collect()
}
