//! Source and dependency boundary guards.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn storage_crate_imports_stay_inside_persistence_adapter() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let offenders: Vec<_> = rust_files(&root)
        .into_iter()
        .filter(|path| {
            !path
                .components()
                .any(|part| part.as_os_str() == "persistence")
        })
        .filter_map(|path| {
            let text = fs::read_to_string(&path).expect("read source file");
            let forbidden = [
                "strata_storage_next",
                "strata-storage-next",
                "StorageRuntime",
                "StorageOpenOptions",
                "CommitBatch",
                "CommitMutation",
                "StorageSpaceId",
                "StorageKey",
                "StorageValue",
                "BranchRequest",
            ];
            forbidden
                .iter()
                .any(|token| text.contains(token))
                .then_some(path)
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "storage crate names escaped persistence: {offenders:?}"
    );
}

#[test]
fn planning_vocabulary_stays_out_of_sources_and_tests() {
    let mut roots = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests"));
    let exact_token = ["Ne", "xt"].concat();
    let forbidden = [
        ["M", "5"].concat(),
        ["M", "5", "G"].concat(),
        ["M", "5", "T"].concat(),
        ["vertical", " ", "spine"].concat(),
        ["next", " ", "slice"].concat(),
    ];
    let offenders: Vec<_> = roots
        .into_iter()
        .flat_map(|root| rust_files(&root))
        .filter(|path| !path.ends_with("dependency_guards.rs"))
        .filter_map(|path| {
            let text = fs::read_to_string(&path).expect("read source file");
            (forbidden.iter().any(|token| text.contains(token.as_str()))
                || text.contains(&exact_token))
            .then_some(path)
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "planning vocabulary appeared outside docs: {offenders:?}"
    );
}

#[test]
fn executor_facing_api_does_not_expose_storage_types() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let roots = [
        root.join("api"),
        root.join("branch"),
        root.join("data").join("kv"),
        root.join("data").join("json"),
        root.join("data").join("vector"),
    ];
    let forbidden = [
        "strata_storage_next",
        "StorageRuntime",
        "StorageOpenOptions",
        "CommitBatch",
        "CommitMutation",
        "StorageSpaceId",
        "StorageKey",
        "StorageValue",
        "BranchRequest",
        "WalService",
        "WalFormat",
        "Manifest",
        "TableRuntime",
        "Lifecycle",
        "StorageBackend",
        "TransactionContext",
        "TransactionSession",
        "TxnContext",
    ];
    let offenders: Vec<_> = roots
        .into_iter()
        .flat_map(|root| rust_files(&root))
        .filter_map(|path| {
            let text = fs::read_to_string(&path).expect("read source file");
            forbidden
                .iter()
                .any(|token| text.contains(token))
                .then_some(path)
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "executor-facing API exposed lower-layer names: {offenders:?}"
    );
}

#[test]
fn kv_service_keeps_storage_requests_in_persistence_modules() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("data")
        .join("kv")
        .join("service.rs");
    let text = fs::read_to_string(path).expect("read KV service source");
    for forbidden in [
        "strata_storage_next",
        "StorageRuntime",
        "PointReadRequest",
        "PrefixScanReadRequest",
        "ScanReadRequest",
        "CommitBatch",
        "CommitMutation",
        "StorageSpaceId",
        "StorageKey",
        "StorageValue",
        "ReadBound",
        "ReadLimit",
        "BranchRequest",
    ] {
        assert!(
            !text.contains(forbidden),
            "KV service constructed storage-specific request details"
        );
    }
}

#[test]
fn json_service_keeps_storage_requests_in_persistence_modules() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("data")
        .join("json")
        .join("service.rs");
    let text = fs::read_to_string(path).expect("read JSON service source");
    for forbidden in [
        "strata_storage_next",
        "StorageRuntime",
        "PointReadRequest",
        "PrefixScanReadRequest",
        "ScanReadRequest",
        "CommitBatch",
        "CommitMutation",
        "StorageSpaceId",
        "StorageKey",
        "StorageValue",
        "ReadBound",
        "ReadLimit",
        "BranchRequest",
    ] {
        assert!(
            !text.contains(forbidden),
            "JSON service constructed storage-specific request details"
        );
    }
}

#[test]
fn vector_service_keeps_storage_requests_in_persistence_modules() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("data")
        .join("vector")
        .join("service.rs");
    let text = fs::read_to_string(path).expect("read vector service source");
    for forbidden in [
        "strata_storage_next",
        "StorageRuntime",
        "PointReadRequest",
        "PrefixScanReadRequest",
        "ScanReadRequest",
        "CommitBatch",
        "CommitMutation",
        "StorageSpaceId",
        "StorageKey",
        "StorageValue",
        "ReadBound",
        "ReadLimit",
        "BranchRequest",
    ] {
        assert!(
            !text.contains(forbidden),
            "vector service constructed storage-specific request details"
        );
    }
}

#[test]
fn branch_service_keeps_storage_branch_requests_in_persistence_modules() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("branch")
        .join("service.rs");
    let text = fs::read_to_string(path).expect("read branch service source");
    for forbidden in [
        "strata_storage_next",
        "BranchRequest",
        "BranchAction",
        "StorageRuntime",
        "StorageBranch",
    ] {
        assert!(
            !text.contains(forbidden),
            "branch service constructed storage-specific request details"
        );
    }
}

#[test]
fn branch_service_does_not_expose_deferred_workflows() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("branch")
        .join("service.rs");
    let text = fs::read_to_string(path).expect("read branch service source");
    for forbidden in [
        "pub fn merge",
        "pub fn publish",
        "pub fn review",
        "pub fn cherry",
        "pub fn revert",
        "pub fn restore",
    ] {
        assert!(
            !text.contains(forbidden),
            "branch service exposed deferred workflow `{forbidden}`"
        );
    }
}

#[test]
fn branch_catalog_preserves_product_default_id_separate_from_storage_id() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("branch")
        .join("catalog.rs");
    let text = fs::read_to_string(path).expect("read branch catalog source");
    assert!(text.contains("DEFAULT_BRANCH_ID: BranchId = BranchId::from_bytes([0;"));
    assert!(text.contains("DEFAULT_STORAGE_BRANCH_ID"));
    assert!(text.contains("storage_branch_id_for_product_branch_id"));
}

#[test]
fn persistence_adapter_owns_storage_request_construction_and_error_mapping() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("persistence")
        .join("adapter.rs");
    let text = fs::read_to_string(path).expect("read persistence adapter source");
    for required in [
        "PointReadRequest",
        "PrefixScanReadRequest",
        "ScanReadRequest",
        "CommitBatch",
        "CommitMutation",
        "StorageSpaceId",
        "StorageKey",
        "StorageValue",
        "ReadBound",
        "ReadLimit",
        "BranchRequest",
        "map_storage_error",
    ] {
        assert!(
            text.contains(required),
            "persistence adapter stopped owning `{required}`"
        );
    }
}

#[test]
fn kv_read_shapes_do_not_devolve_to_point_or_write_loops() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("data")
        .join("kv")
        .join("service.rs");
    let text = fs::read_to_string(path).expect("read KV service source");

    let batch_get = function_body(&text, "pub fn batch_get");
    assert!(batch_get.contains("let record = self.branch_record()?;"));
    assert!(batch_get.contains(".read_row("));
    assert!(!batch_get.contains("commit_batch"));

    let count = function_body(&text, "pub fn count");
    assert!(count.contains(".scan_prefix("));
    assert!(!count.contains(".read_row("));

    let sample = function_body(&text, "pub fn sample");
    assert!(sample.contains(".scan_prefix("));
    assert!(!sample.contains(".read_row("));

    let page = function_body(&text, "fn scan_keys_after_cursor");
    assert!(page.contains(".scan_range("));
    assert!(page.contains("saturating_add(1)"));
    assert!(!page.contains(".persistence.scan_prefix("));
}

#[test]
fn executor_crate_does_not_import_storage_crates() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let executor = manifest
        .parent()
        .expect("engine crate has crates parent")
        .join("executor-next");
    if !executor.exists() {
        return;
    }
    let mut files = rust_files(&executor.join("src"));
    files.push(executor.join("Cargo.toml"));
    let offenders: Vec<_> = files
        .into_iter()
        .filter_map(|path| {
            let text = fs::read_to_string(&path).expect("read executor source");
            [
                "strata_storage_next",
                "strata-storage-next",
                "StorageRuntime",
                "StorageOpenOptions",
                "CommitBatch",
                "CommitMutation",
                "StorageSpaceId",
                "StorageKey",
                "StorageValue",
                "BranchRequest",
            ]
            .iter()
            .any(|token| text.contains(token))
            .then_some(path)
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "executor crate imported storage behavior details: {offenders:?}"
    );
}

#[test]
fn benchmark_sources_do_not_use_engine_persistence_internals() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let benchmark_root = manifest
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .join("benchmarks")
        .join("src");
    if !benchmark_root.exists() {
        return;
    }
    let offenders: Vec<_> = rust_files(&benchmark_root)
        .into_iter()
        .filter_map(|path| {
            let text = fs::read_to_string(&path).expect("read benchmark source");
            [
                "strata_engine_next::persistence",
                "persistence::adapter",
                "StoragePersistence",
                "PersistenceReadRow",
                "CommitPlan",
                "RowAddress",
            ]
            .iter()
            .any(|token| text.contains(token))
            .then_some(path)
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "benchmark source used engine persistence internals: {offenders:?}"
    );
}

#[test]
fn product_scope_stays_limited_to_branch_kv_json_and_vector() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let forbidden = [
        "Event",
        "Graph",
        "Retrieval",
        "Ipc",
        "Export",
        "Merge",
        "Diff",
        "Restore",
        "Revert",
        "CherryPick",
        "TransactionSession",
        "begin_transaction",
        "transaction_session",
    ];
    let offenders: Vec<_> = rust_files(&root)
        .into_iter()
        .filter_map(|path| {
            let text = fs::read_to_string(&path).expect("read source file");
            forbidden
                .iter()
                .any(|token| text.contains(token))
                .then_some(path)
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "out-of-scope product behavior appeared in engine crate: {offenders:?}"
    );
}

#[test]
fn open_options_remain_explicit() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("api")
        .join("options.rs");
    let text = fs::read_to_string(path).expect("read options source");
    for forbidden in [
        "derive(Default)",
        "impl Default for CacheOpenOptions",
        "impl Default for DurableLocalOpenOptions",
    ] {
        assert!(
            !text.contains(forbidden),
            "open options gained implicit default mode"
        );
    }
}

fn rust_files(root: &Path) -> Vec<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new("rg")
        .arg("--files")
        .arg(root)
        .output()
        .expect("run rg source file listing");
    assert!(
        output.status.success(),
        "rg failed while listing source files: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("source listing is UTF-8")
        .lines()
        .filter(|line| {
            Path::new(line)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("rs"))
        })
        .map(|line| {
            let path = PathBuf::from(line);
            if path.is_absolute() {
                path
            } else {
                manifest.join(path)
            }
        })
        .collect()
}

fn function_body<'a>(source: &'a str, signature: &str) -> &'a str {
    let start = source
        .find(signature)
        .unwrap_or_else(|| panic!("missing function signature `{signature}`"));
    let open = source[start..].find('{').map_or_else(
        || panic!("missing function body `{signature}`"),
        |offset| start + offset,
    );
    let mut depth = 0usize;
    for (offset, byte) in source[open..].bytes().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return &source[open..=open + offset];
                }
            }
            _ => {}
        }
    }
    panic!("unterminated function body `{signature}`");
}
