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
fn product_scope_stays_limited_to_branch_and_kv() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let forbidden = [
        "Json",
        "Event",
        "Vector",
        "Graph",
        "Retrieval",
        "Search",
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
