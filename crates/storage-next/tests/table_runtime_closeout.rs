//! Closeout guards for the M4-L5 table-runtime milestone.

#![deny(unsafe_code)]

mod common;

use std::fs;
use std::path::Path;

#[test]
fn table_runtime_closeout_generated_harness_exposes_every_counter() {
    let crate_root = common::crate_root();
    let testkit = read_file(&crate_root.join("src/testkit/table_runtime.rs"));
    let properties = read_file(&crate_root.join("tests/table_runtime_properties.rs"));

    let counter_methods = [
        "valid_config_cases",
        "invalid_config_cases",
        "valid_fact_cases",
        "invalid_fact_cases",
        "row_key_adapter_cases",
        "invalid_row_key_sequence_cases",
        "key_bound_cases",
        "size_accounting_cases",
        "mutable_frozen_table_cases",
        "raw_cursor_cases",
        "immutable_builder_artifact_cases",
        "immutable_table_reader_cases",
        "object_backed_table_reader_cases",
        "table_block_cache_cases",
        "table_bloom_filter_cases",
        "table_compaction_cases",
        "error_source_cases",
        "stats_cases",
    ];
    assert_contains_all("src/testkit/table_runtime.rs", &testkit, &counter_methods);
    assert_contains_all(
        "tests/table_runtime_properties.rs",
        &properties,
        &counter_methods,
    );
}

#[test]
fn table_runtime_closeout_source_guard_suite_covers_required_boundary_categories() {
    let crate_root = common::crate_root();
    let source_guard = read_file(&crate_root.join("tests/table_runtime_source_guard.rs"));

    assert_contains_all(
        "tests/table_runtime_source_guard.rs",
        &source_guard,
        &[
            "table_runtime_source_does_not_import_upper_layers_or_engines",
            "table_runtime_source_does_not_use_product_payload_vocabulary",
            "table_runtime_source_does_not_use_cursor_policy_vocabulary",
            "table_runtime_source_does_not_use_filesystem_backend_service_or_env_apis",
            "table_runtime_source_does_not_use_object_layout_literals",
            "table_runtime_source_does_not_use_old_table_builder_vocabulary",
            "table_runtime_source_does_not_create_process_global_cache_state",
            "table_runtime_source_does_not_use_unsafe_or_old_cache_identity",
            "table_runtime_compaction_source_does_not_embed_retention_policy_terms",
            "table_runtime_stays_crate_private",
        ],
    );

    assert_contains_all(
        "tests/table_runtime_source_guard.rs",
        &source_guard,
        &[
            "crate::backend",
            "crate::layout",
            "crate::object",
            "crate::service",
            "crate::branch",
            "crate::commit",
            "crate::lifecycle",
            "crate::testkit",
            "std::fs",
            "std::path::Path",
            "std::path::PathBuf",
            "std::fs::File",
            "pread",
            "rename(",
            "remove_file(",
            "mmap",
            "memmap",
            "tables/",
            "wal/",
            "snapshots/",
            "manifest/current",
            "STRAKV",
            "SegmentBuilder",
            "global_cache",
            "branch_retention",
            "inherited_table",
            "install_manifest",
            "lifecycle_cleanup",
            "garbage_collect",
            "testkit::",
            "pub mod table;",
        ],
    );
}

#[test]
fn table_runtime_closeout_fuzz_inventory_matches_existing_table_targets() {
    let crate_root = common::crate_root();
    let fuzz_manifest = read_file(&crate_root.join("fuzz/Cargo.toml"));

    for target in ["format_table_artifact", "format_table_block"] {
        let target_path = crate_root.join(format!("fuzz/fuzz_targets/{target}.rs"));
        assert!(
            target_path.is_file(),
            "missing documented table fuzz target {}",
            target_path.display()
        );
        let manifest_name = format!("name = \"{target}\"");
        let manifest_path = format!("path = \"fuzz_targets/{target}.rs\"");
        assert_contains_all(
            "fuzz/Cargo.toml",
            &fuzz_manifest,
            &[manifest_name.as_str(), manifest_path.as_str()],
        );
    }
}

fn read_file(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn assert_contains_all(label: &str, text: &str, needles: &[&str]) {
    for needle in needles {
        assert!(text.contains(needle), "{label} should contain {needle:?}");
    }
}
