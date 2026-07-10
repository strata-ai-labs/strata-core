//! Closeout guards for the table-runtime implementation.

#![deny(unsafe_code)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn table_runtime_closeout_generated_harness_exposes_every_counter() {
    let crate_root = common::crate_root();
    let testkit = read_table_runtime_testkit_sources(&crate_root);
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
        "lazy_reader_open_cases",
        "lazy_point_hit_cases",
        "lazy_point_miss_cases",
        "lazy_range_cursor_cases",
        "object_backed_reader_parity_cases",
        "table_block_cache_cases",
        "cache_hit_cases",
        "cache_miss_cases",
        "table_bloom_filter_cases",
        "filter_available_cases",
        "filter_absent_cases",
        "filter_negative_probe_cases",
        "filter_false_positive_cases",
        "table_compaction_cases",
        "streaming_compaction_output_cases",
        "table_perf_trace_cases",
        "error_source_cases",
    ];
    assert_contains_all(
        "src/testkit/table_runtime module",
        &testkit,
        &counter_methods,
    );
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
            "KVSegment",
            "SegmentBuilder",
            "SegmentId",
            "path_hash",
            "global_cache",
            "materialization",
            "retention",
            "quarantine",
            "checkpoint",
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
fn table_runtime_closeout_porting_log_records_required_evidence() {
    let crate_root = common::crate_root();
    let repo_root = repository_root(&crate_root);
    let porting_log_path = repo_root
        .join("docs/architecture/implementation-plans")
        .join(milestone(4))
        .join(storage_next_layer(5))
        .join(porting_log_file_name(4, 5));
    let porting_log = read_file(&porting_log_path);
    let closeout_heading = format!(
        "## {}P-{}: Table Runtime Parity Closeout",
        milestone(4),
        storage_next_layer(5)
    );
    let source_ownership = format!("{} still owns", storage_next_layer(6));
    let maintenance_ownership = format!("{} still owns", storage_next_layer(8));

    assert_contains_all_strings(
        "table runtime porting log",
        &porting_log,
        &[
            closeout_heading,
            table_runtime_file_name("implementation-plan"),
            table_runtime_file_name("test-plan"),
            "crates/storage/src/{segment,index,bloom,block_cache,merge_iter,seekable,compaction}.rs"
                .to_owned(),
            "Lazy table open".to_owned(),
            "Point lookup".to_owned(),
            "Range and prefix".to_owned(),
            "Bloom/filter".to_owned(),
            "Table compaction".to_owned(),
            "Object-backed reader".to_owned(),
            "Durable filter blocks remain deferred".to_owned(),
            source_ownership,
            maintenance_ownership,
            "Closeout report:".to_owned(),
            table_runtime_file_name("closeout"),
        ],
    );
}

#[test]
fn table_runtime_closeout_fuzz_inventory_matches_existing_table_targets() {
    let crate_root = common::crate_root();
    let fuzz_manifest = read_file(&crate_root.join("fuzz/Cargo.toml"));

    for target in [
        "format_table_artifact",
        "format_table_block",
        "format_table_block_trusted",
        "format_table_block_indexed_seek",
    ] {
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
        assert_nonempty_corpus(&crate_root, target);
    }

    for (target, contract) in [
        (
            "table_runtime_reader",
            "check_table_runtime_reader_contract",
        ),
        (
            "table_runtime_cursor",
            "check_table_runtime_cursor_contract",
        ),
        (
            "table_runtime_compaction",
            "check_table_runtime_compaction_contract",
        ),
    ] {
        let target_path = crate_root.join(format!("fuzz/fuzz_targets/{target}.rs"));
        assert!(
            target_path.is_file(),
            "missing documented table fuzz target {}",
            target_path.display()
        );
        let target_text = read_file(&target_path);
        let manifest_name = format!("name = \"{target}\"");
        let manifest_path = format!("path = \"fuzz_targets/{target}.rs\"");
        assert_contains_all(
            "fuzz/Cargo.toml",
            &fuzz_manifest,
            &[manifest_name.as_str(), manifest_path.as_str()],
        );
        assert_contains_all(target, &target_text, &[contract]);
        assert!(
            !target_text.contains("check_table_runtime_scaffold_contract"),
            "{target} must exercise its dedicated runtime contract"
        );
        assert_nonempty_corpus(&crate_root, target);
    }
}

fn read_file(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn repository_root(crate_root: &Path) -> PathBuf {
    crate_root
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("derive repository root from {}", crate_root.display()))
        .to_path_buf()
}

fn milestone(digit: u8) -> String {
    format!("M{digit}")
}

fn storage_next_layer(digit: u8) -> String {
    format!("L{digit}")
}

fn table_runtime_file_name(suffix: &str) -> String {
    format!(
        "{}p-{}-table-runtime-parity-{suffix}.md",
        milestone(4).to_ascii_lowercase(),
        storage_next_layer(5).to_ascii_lowercase()
    )
}

fn porting_log_file_name(milestone_digit: u8, layer_digit: u8) -> String {
    format!(
        "{}-{}-porting-log.md",
        milestone(milestone_digit).to_ascii_lowercase(),
        storage_next_layer(layer_digit).to_ascii_lowercase()
    )
}

fn read_table_runtime_testkit_sources(crate_root: &Path) -> String {
    let mut text = read_file(&crate_root.join("src/testkit/table_runtime.rs"));
    let module_dir = crate_root.join("src/testkit/table_runtime");
    let mut entries = fs::read_dir(&module_dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", module_dir.display()))
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| panic!("read entries in {}: {error}", module_dir.display()));
    entries.sort_by_key(std::fs::DirEntry::path);
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            text.push('\n');
            text.push_str(&read_file(&path));
        }
    }
    text
}

fn assert_contains_all(label: &str, text: &str, needles: &[&str]) {
    for needle in needles {
        assert!(text.contains(needle), "{label} should contain {needle:?}");
    }
}

fn assert_contains_all_strings(label: &str, text: &str, needles: &[String]) {
    for needle in needles {
        assert!(text.contains(needle), "{label} should contain {needle:?}");
    }
}

fn assert_nonempty_corpus(crate_root: &Path, target: &str) {
    let corpus = crate_root.join(format!("fuzz/corpus/{target}"));
    assert!(
        corpus.is_dir(),
        "missing checked-in fuzz corpus directory {}",
        corpus.display()
    );
    let has_seed = fs::read_dir(&corpus)
        .unwrap_or_else(|error| panic!("read {}: {error}", corpus.display()))
        .any(|entry| entry.map(|entry| entry.path().is_file()).unwrap_or(false));
    assert!(
        has_seed,
        "fuzz corpus directory {} should contain at least one seed",
        corpus.display()
    );
}
