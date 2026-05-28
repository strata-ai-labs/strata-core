//! Source guards for the storage API boundary.

#![deny(unsafe_code)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn api_is_the_only_public_storage_next_production_module() {
    let root = common::crate_root();
    let lib = fs::read_to_string(root.join("src/lib.rs")).expect("read lib.rs");

    assert!(lib.contains("pub mod api;"));
    for module in [
        "backend",
        "branch",
        "commit",
        "config",
        "error",
        "format",
        "layout",
        "lifecycle",
        "object",
        "observability",
        "row",
        "service",
        "table",
    ] {
        assert!(
            !lib.contains(&format!("pub mod {module};")),
            "src/lib.rs publicly exposes lower storage module {module}"
        );
    }
}

#[test]
fn lower_modules_are_not_public_api() {
    let root = common::crate_root();
    let lib = fs::read_to_string(root.join("src/lib.rs")).expect("read lib.rs");
    for module in [
        "backend",
        "branch",
        "commit",
        "config",
        "error",
        "format",
        "layout",
        "lifecycle",
        "object",
        "observability",
        "row",
        "service",
        "table",
    ] {
        assert!(
            !lib.contains(&format!("pub mod {module};")),
            "src/lib.rs publicly exposes lower storage module {module}"
        );
    }
}

#[test]
fn api_public_signatures_do_not_expose_lower_layer_concrete_types() {
    let root = common::crate_root();
    let forbidden = [
        "BackendCapabilities",
        "BackendError",
        "BackendResult",
        "LifecycleError",
        "StorageOpenOutcome",
        "MaintenanceOutcome",
        "CloseOutcome",
        "CommitOutcome",
        "CommitReplayRuntime",
        "BranchLocalState",
        "BranchReadView",
        "BranchSnapshot",
        "BranchInherited",
        "BranchOwnedTable",
        "BranchInheritedLayer",
        "CommitRuntime",
        "TableObjectService",
        "TableObject",
        "TableObjectRef",
        "TableManifest",
        "WalService",
        "WalRecord",
        "WalRecordEnvelope",
        "ManifestService",
        "ManifestSnapshot",
        "SnapshotService",
        "SnapshotEnvelope",
        "FormatError",
        "FormatResult",
        "Codec",
        "LayoutId",
        "ServiceState",
        "LifecycleResult",
    ];

    for file in api_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read API source");
        for (line_number, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("pub ") {
                continue;
            }
            for type_name in forbidden {
                assert!(
                    !trimmed.contains(type_name),
                    "{}:{} exposes lower-layer concrete type {type_name}: {line}",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    line_number + 1
                );
            }
        }
    }
}

#[test]
fn api_source_avoids_engine_product_and_runtime_dependencies() {
    let root = common::crate_root();
    for file in api_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read API source");
        for (line_number, line) in text.lines().enumerate() {
            assert!(
                !contains_forbidden_api_dependency(line),
                "{}:{} uses forbidden API dependency: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
            assert!(
                !contains_forbidden_runtime_dependency(line),
                "{}:{} uses forbidden runtime dependency: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
            assert!(
                !contains_forbidden_product_vocabulary(line),
                "{}:{} uses forbidden product vocabulary: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
        }
    }
}

#[test]
fn lower_layers_do_not_import_api_upward() {
    let root = common::crate_root();
    for source_dir in [
        "src/backend",
        "src/config",
        "src/error",
        "src/layout",
        "src/object",
        "src/format",
        "src/observability",
        "src/service",
        "src/table",
        "src/branch",
        "src/commit",
        "src/row",
        "src/lifecycle",
    ] {
        let mut files = Vec::new();
        common::source_guard_helpers::collect_rs_files(&root.join(source_dir), &mut files);
        for file in files {
            let text = fs::read_to_string(&file).expect("read lower source");
            for (line_number, line) in text.lines().enumerate() {
                assert!(
                    !imports_api(line),
                    "{}:{} imports upward into API: {line}",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    line_number + 1
                );
            }
        }
    }
}

#[test]
fn api_implementation_avoids_architecture_labels() {
    let root = common::crate_root();
    for file in api_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read API source");
        for (line_number, line) in text.lines().enumerate() {
            assert!(
                !common::source_guard_helpers::contains_milestone_label(line),
                "{}:{} contains architecture label: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
        }
    }
}

fn api_source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    common::source_guard_helpers::collect_rs_files(&root.join("src/api"), &mut files);
    files
}

fn contains_forbidden_api_dependency(line: &str) -> bool {
    let compact = compact_line(line);
    let forbidden_modules = [
        "backend",
        "layout",
        "format",
        "service",
        "table",
        "branch",
        "commit",
        "lifecycle",
    ];
    if forbidden_modules
        .iter()
        .any(|module| imports_crate_module(&compact, module))
    {
        return true;
    }

    [
        "strata_engine",
        "strata_intelligence",
        "strata_inference",
        "strata_executor",
    ]
    .iter()
    .any(|needle| compact.contains(needle))
}

fn contains_forbidden_runtime_dependency(line: &str) -> bool {
    let compact = compact_line(line);
    ["async", "future", "tokio", "async_std", "spawn"]
        .iter()
        .any(|needle| compact.contains(needle))
}

fn contains_forbidden_product_vocabulary(line: &str) -> bool {
    let compact = compact_line(line);
    [
        "json",
        "event",
        "embedding",
        "prompt",
        "model",
        "chat",
        "vector",
        "graph",
        "search",
        "stratahub",
        "primitive",
    ]
    .iter()
    .any(|needle| compact.contains(needle))
}

fn imports_api(line: &str) -> bool {
    let compact = compact_line(line);
    imports_crate_module(&compact, "api")
        || compact.contains("super::api")
        || compact.contains("super::{api")
}

fn imports_crate_module(compact: &str, module: &str) -> bool {
    compact.contains(&format!("crate::{module}"))
        || compact.contains(&format!("crate::{{{module}"))
        || compact.contains(&format!(",{module}::"))
        || compact.contains(&format!("{{{module}::"))
}

fn compact_line(line: &str) -> String {
    line.split("//")
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect::<String>()
        .to_ascii_lowercase()
}

#[test]
fn api_dependency_guard_catches_grouped_lower_layer_imports() {
    assert!(contains_forbidden_api_dependency(
        "use crate::{backend::BackendCapabilities, lifecycle::StorageMode};"
    ));
    assert!(contains_forbidden_api_dependency(
        "use crate::service::WalService;"
    ));
    assert!(!contains_forbidden_api_dependency(
        "use super::{StorageApiError, StorageApiResult};"
    ));
}

#[test]
fn upward_api_guard_catches_grouped_api_imports() {
    assert!(imports_api(
        "use crate::{api::StorageRuntime, lifecycle::LifecycleError};"
    ));
    assert!(imports_api(
        "use super::{api::StorageRuntime, cache::CacheRuntime};"
    ));
    assert!(!imports_api("use crate::lifecycle::LifecycleError;"));
}

#[test]
fn api_runtime_guard_catches_future_after_lowercasing() {
    assert!(contains_forbidden_runtime_dependency(
        "pub fn returns_future() -> impl Future<Output = ()>"
    ));
}

#[test]
fn api_product_guard_catches_required_product_terms() {
    assert!(contains_forbidden_product_vocabulary(
        "pub struct VectorSearch"
    ));
    assert!(contains_forbidden_product_vocabulary(
        "pub struct GraphQuery"
    ));
}
