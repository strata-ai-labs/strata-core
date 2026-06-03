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
        "LifecycleWalGrowthPolicy",
        "StorageRuntimeBudget",
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
        assert_no_forbidden_public_signature(&root, &file, &text, &forbidden);
    }
}

#[test]
fn api_open_signatures_do_not_expose_lifecycle_types() {
    assert_api_public_signatures_do_not_expose(&[
        "LifecycleCacheOpenRequest",
        "LifecycleDurableLocalOpenRequest",
        "LifecycleCacheRuntime",
        "LifecycleDurableLocalRuntime",
        "LifecycleStorageOpenOutcome",
        "LifecycleRecoveryRuntime",
        "StorageOpenPlan",
    ]);
}

#[test]
fn api_close_signatures_do_not_expose_lifecycle_types() {
    assert_api_public_signatures_do_not_expose(&[
        "CloseOutcome",
        "CloseOutcomeStatus",
        "LifecycleStateMachine",
        "LifecycleCloseFact",
        "LifecycleCloseTimeoutPolicy",
    ]);
}

#[test]
fn api_open_does_not_expose_backend_services() {
    assert_api_public_signatures_do_not_expose(&[
        "TableObjectService",
        "ManifestService",
        "SnapshotService",
        "WalService",
        "BackendCapabilities",
        "BackendError",
    ]);
}

#[test]
fn api_open_unsupported_modes_do_not_claim_production_support() {
    let root = common::crate_root();
    let options = fs::read_to_string(root.join("src/api/options.rs")).expect("read options");

    assert!(options.contains("object-durable storage is not a V1 production mode"));
    assert!(options.contains("distributed writer coordination is not a V1 production mode"));
}

#[test]
fn api_open_options_do_not_have_implicit_default_mode() {
    let root = common::crate_root();
    let options = fs::read_to_string(root.join("src/api/options.rs")).expect("read options");

    assert_storage_open_options_has_no_default(&options);
}

#[test]
fn production_open_paths_do_not_use_storage_open_options_default() {
    let root = common::crate_root();
    for file in production_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read production source");
        let compact = text.split_whitespace().collect::<String>();
        for forbidden in [
            "StorageOpenOptions::default",
            "StorageRuntime::open(StorageOpenOptions::default())",
            "StorageRuntime::open(Default::default())",
        ] {
            assert!(
                !compact.contains(forbidden),
                "{} uses implicit storage open default via {forbidden}",
                file.strip_prefix(&root).unwrap_or(&file).display()
            );
        }

        for (line_number, line) in text.lines().enumerate() {
            let compact_line = line.split_whitespace().collect::<String>();
            assert!(
                !(compact_line.contains("StorageOpenOptions")
                    && compact_line.contains("Default::default()")),
                "{}:{} uses implicit StorageOpenOptions default: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
        }
    }
}

#[test]
fn api_open_local_routes_to_durable_local_without_cache_fallback() {
    let root = common::crate_root();
    let runtime = fs::read_to_string(root.join("src/api/runtime.rs")).expect("read runtime");
    let compact_runtime = runtime.split_whitespace().collect::<String>();

    assert!(compact_runtime.contains("pubfnopen_local("));
    assert!(compact_runtime
        .contains("Self::open_durable_local(root,StorageDurabilityPolicy::Standard)"));
    assert!(compact_runtime.contains("StorageOpenOptions::durable_local(policy)"));
    assert!(compact_runtime.contains("capability:\"localfs\""));
    assert!(!compact_runtime.contains("open_local(root:implInto<std::path::PathBuf>)->StorageApiResult<StorageOpenOutcome<'static>>{Self::open_cache()"));
    assert!(!compact_runtime.contains("open_local(root:implInto<std::path::PathBuf>)->StorageApiResult<StorageOpenOutcome<'static>>{Self::open_ephemeral()"));
}

#[test]
fn api_source_rejects_silent_durable_to_cache_fallback_wording() {
    let root = common::crate_root();
    for file in api_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read API source");
        for (line_number, line) in text.lines().enumerate() {
            assert!(
                !looks_like_silent_durable_to_cache_fallback(line),
                "{}:{} suggests silent durable-to-cache fallback: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
        }
    }
}

#[test]
fn api_docs_show_native_durable_open_before_explicit_ephemeral_open() {
    let root = common::crate_root();
    let api_mod = fs::read_to_string(root.join("src/api/mod.rs")).expect("read api mod");
    let durable = api_mod
        .find("StorageRuntime::open_local(root)")
        .expect("API docs show native durable local open");
    let ephemeral = api_mod
        .find("StorageRuntime::open_ephemeral()")
        .expect("API docs show explicit ephemeral open");

    assert!(
        durable < ephemeral,
        "API docs must present native durable local open before ephemeral open"
    );
    assert!(api_mod.contains("Volatile storage must be requested explicitly"));
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

#[test]
fn api_branch_surface_excludes_product_workflow_terms() {
    let root = common::crate_root();
    for relative in ["src/api/branch.rs", "src/api/runtime.rs"] {
        let file = root.join(relative);
        let text = fs::read_to_string(&file)
            .expect("read branch API source")
            .to_ascii_lowercase();
        for term in ["merge", "cherry", "revert", "restore", "publish", "review"] {
            assert!(
                !text.contains(term),
                "{relative} exposes deferred branch workflow term {term}"
            );
        }
    }
}

#[test]
fn diagnostics_do_not_contain_product_vocabulary() {
    let root = common::crate_root();
    let file = root.join("src/api/diagnostics.rs");
    let text = fs::read_to_string(&file).expect("read diagnostics API source");
    for (line_number, line) in text.lines().enumerate() {
        assert!(
            !contains_forbidden_product_vocabulary(line),
            "src/api/diagnostics.rs:{} uses forbidden product vocabulary: {line}",
            line_number + 1
        );
    }
}

#[test]
fn diagnostics_do_not_contain_primitive_vocabulary() {
    let root = common::crate_root();
    let file = root.join("src/api/diagnostics.rs");
    let text = fs::read_to_string(&file).expect("read diagnostics API source");
    for (line_number, line) in text.lines().enumerate() {
        assert!(
            !compact_line(line).contains("primitive"),
            "src/api/diagnostics.rs:{} uses primitive vocabulary: {line}",
            line_number + 1
        );
    }
}

#[test]
fn diagnostics_do_not_contain_user_advice() {
    let root = common::crate_root();
    let file = root.join("src/api/diagnostics.rs");
    let text = fs::read_to_string(&file).expect("read diagnostics API source");
    for (line_number, line) in text.lines().enumerate() {
        let compact = compact_line(line);
        for term in [
            "recommend",
            "advice",
            "user",
            "workspace",
            "project",
            "account",
        ] {
            assert!(
                !compact.contains(term),
                "src/api/diagnostics.rs:{} uses user-facing advice term {term}: {line}",
                line_number + 1
            );
        }
    }
}

#[test]
fn diagnostics_do_not_import_engine_telemetry() {
    let root = common::crate_root();
    let file = root.join("src/api/diagnostics.rs");
    let text = fs::read_to_string(&file).expect("read diagnostics API source");
    for (line_number, line) in text.lines().enumerate() {
        let compact = compact_line(line);
        if compact.starts_with("use") {
            assert!(
                !contains_forbidden_api_dependency(line)
                    && !compact.contains("observability")
                    && !compact.contains("telemetry::"),
                "src/api/diagnostics.rs:{} imports engine telemetry: {line}",
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

fn production_source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    common::source_guard_helpers::collect_rs_files(&root.join("src"), &mut files);
    files
        .into_iter()
        .filter(|file| {
            let relative = file.strip_prefix(root).unwrap_or(file.as_path());
            let path = relative.to_string_lossy();
            !path.contains("/tests/")
                && !path.ends_with("/tests.rs")
                && !path.ends_with("_tests.rs")
                && !path.contains("/testkit/")
                && !path.contains("/test_support/")
                && !path.contains("/proptest-regressions/")
        })
        .collect()
}

fn contains_forbidden_api_dependency(line: &str) -> bool {
    let compact = compact_line(line);
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

fn assert_storage_open_options_has_no_default(source: &str) {
    let compact_source = source.split_whitespace().collect::<String>();
    for forbidden_impl in [
        "implDefaultforStorageOpenOptions",
        "implstd::default::DefaultforStorageOpenOptions",
        "implcore::default::DefaultforStorageOpenOptions",
        "impl::std::default::DefaultforStorageOpenOptions",
        "impl::core::default::DefaultforStorageOpenOptions",
    ] {
        assert!(
            !compact_source.contains(forbidden_impl),
            "StorageOpenOptions must not implement Default via {forbidden_impl}"
        );
    }

    assert!(!source.contains("StorageOpenOptions::default"));
    assert_storage_open_options_derive_excludes_default(&compact_source);
}

fn assert_storage_open_options_derive_excludes_default(compact_source: &str) {
    let prefix = compact_source
        .split("pubstructStorageOpenOptions")
        .next()
        .expect("StorageOpenOptions declaration is present");
    let derive_args = prefix
        .rsplit("#[derive(")
        .next()
        .unwrap_or_default()
        .split(")]")
        .next()
        .unwrap_or_default();

    assert!(
        !derive_args
            .split(',')
            .any(|trait_path| trait_path.ends_with("Default")),
        "StorageOpenOptions must not derive Default"
    );
}

fn looks_like_silent_durable_to_cache_fallback(line: &str) -> bool {
    let compact = compact_full_line(line);
    let mentions_fallback = compact.contains("fallback")
        || compact.contains("fallbacks")
        || compact.contains("fallsback")
        || compact.contains("fallingback");
    let explicitly_rejected = compact.contains("never")
        || compact.contains("without")
        || compact.contains("cannot")
        || compact.contains("reject")
        || compact.contains("mustnot")
        || compact.contains("doesnot");

    compact.contains("durable")
        && compact.contains("cache")
        && mentions_fallback
        && !explicitly_rejected
}

fn compact_full_line(line: &str) -> String {
    line.split_whitespace()
        .collect::<String>()
        .to_ascii_lowercase()
}

fn compact_line(line: &str) -> String {
    line.split("//")
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect::<String>()
        .to_ascii_lowercase()
}

fn assert_no_forbidden_public_signature(root: &Path, file: &Path, text: &str, forbidden: &[&str]) {
    let mut signature = String::new();
    let mut start_line = 0;
    let mut collecting = false;

    for (line_number, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if !collecting && trimmed.starts_with("pub ") {
            collecting = true;
            start_line = line_number + 1;
            signature.clear();
        }
        if collecting {
            signature.push_str(trimmed);
            signature.push(' ');
            if public_signature_is_complete(trimmed) {
                assert_signature_has_no_forbidden_type(
                    root, file, start_line, &signature, forbidden,
                );
                collecting = false;
            }
        }
    }
}

fn assert_api_public_signatures_do_not_expose(forbidden: &[&str]) {
    let root = common::crate_root();
    for file in api_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read API source");
        assert_no_forbidden_public_signature(&root, &file, &text, forbidden);
    }
}

fn public_signature_is_complete(line: &str) -> bool {
    line.ends_with(';') || line.ends_with('{') || line.ends_with('}')
}

fn assert_signature_has_no_forbidden_type(
    root: &Path,
    file: &Path,
    start_line: usize,
    signature: &str,
    forbidden: &[&str],
) {
    for type_name in forbidden {
        assert!(
            !signature.contains(type_name),
            "{}:{} exposes lower-layer concrete type {type_name}: {signature}",
            file.strip_prefix(root).unwrap_or(file).display(),
            start_line
        );
    }
}

#[test]
fn api_dependency_guard_catches_engine_product_imports() {
    assert!(contains_forbidden_api_dependency(
        "use strata_engine::{Database, Runtime};"
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
fn public_signature_guard_catches_multiline_lower_types() {
    let root = Path::new("/crate");
    let file = root.join("src/api/example.rs");
    let text = "\
pub fn leaky(
    error: LifecycleError,
) -> StorageApiResult<()> {
    Ok(())
}
";
    let result = std::panic::catch_unwind(|| {
        assert_no_forbidden_public_signature(root, &file, text, &["LifecycleError"]);
    });

    assert!(result.is_err());
}

#[test]
fn api_runtime_guard_catches_future_after_lowercasing() {
    assert!(contains_forbidden_runtime_dependency(
        "pub fn returns_future() -> impl Future<Output = ()>"
    ));
}

#[test]
fn durable_to_cache_fallback_guard_scans_rustdoc_comments() {
    assert!(looks_like_silent_durable_to_cache_fallback(
        "//! durable local falls back to cache mode"
    ));
    assert!(looks_like_silent_durable_to_cache_fallback(
        "/// durable local fallback to cache mode"
    ));
    assert!(!looks_like_silent_durable_to_cache_fallback(
        "//! durable local never falls back to cache mode"
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
