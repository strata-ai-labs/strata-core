//! Executor error-boundary and source-guard tests.

use std::fs;
use std::path::{Path, PathBuf};

use strata_executor_next::{Bytes, Command, Executor, ExecutorError, ExecutorErrorClass};

#[test]
fn executor_errors_have_stable_public_shape() {
    let mut executor = Executor::open_cache().expect("cache executor opens");

    let invalid_key = executor
        .execute(Command::KvGet {
            branch: None,
            space: None,
            key: Bytes::new(Vec::new()),
            as_of: None,
        })
        .expect_err("empty key fails");
    assert_eq!(invalid_key.class(), ExecutorErrorClass::InvalidInput);
    assert!(invalid_key.code().contains(".executor."));

    let invalid_space = executor
        .execute(Command::KvPut {
            branch: None,
            space: Some("_system_".to_owned()),
            key: Bytes::from("key"),
            value: Bytes::from("value"),
        })
        .expect_err("reserved space fails");
    assert_eq!(invalid_space.class(), ExecutorErrorClass::InvalidInput);

    let missing_branch = executor
        .execute(Command::KvPut {
            branch: Some("missing".to_owned()),
            space: None,
            key: Bytes::from("key"),
            value: Bytes::from("value"),
        })
        .expect_err("missing branch fails");
    assert_eq!(missing_branch.class(), ExecutorErrorClass::NotFound);

    executor.close().expect("close succeeds");
    let closed = executor
        .execute(Command::KvExists {
            branch: None,
            space: None,
            key: Bytes::from("key"),
        })
        .expect_err("closed executor fails");
    assert_eq!(closed.class(), ExecutorErrorClass::ClosedHandle);
}

#[test]
fn serialized_errors_do_not_expose_lower_layer_terms() {
    let error = ExecutorError::new(
        ExecutorErrorClass::Internal,
        "internal.executor.test",
        false,
        "public message",
    );
    let encoded = serde_json::to_string(&error).expect("error serializes");

    for forbidden in forbidden_lower_layer_terms() {
        assert!(
            !encoded.contains(forbidden),
            "serialized error leaked forbidden term `{forbidden}`: {encoded}"
        );
    }
}

#[test]
fn executor_crate_does_not_depend_on_storage_crates() {
    let manifest = fs::read_to_string(crate_root().join("Cargo.toml")).expect("manifest reads");
    assert!(!manifest.contains("strata-storage-next"));
    assert!(!manifest.contains("strata_storage_next"));
}

#[test]
fn executor_sources_do_not_name_lower_layer_types() {
    for file in source_files(&crate_root().join("src")) {
        let text = fs::read_to_string(&file).expect("source reads");
        for forbidden in forbidden_lower_layer_terms() {
            assert!(
                !text.contains(forbidden),
                "{} leaked forbidden term `{forbidden}`",
                file.display()
            );
        }
    }
}

#[test]
fn command_and_output_are_serde_serializable() {
    let command_source =
        fs::read_to_string(crate_root().join("src/command.rs")).expect("command reads");
    let output_source =
        fs::read_to_string(crate_root().join("src/output.rs")).expect("output reads");

    assert!(command_source.contains("Serialize"));
    assert!(command_source.contains("Deserialize"));
    assert!(output_source.contains("Serialize"));
    assert!(output_source.contains("Deserialize"));
}

#[test]
fn convenience_facade_stays_command_shaped() {
    let source = fs::read_to_string(crate_root().join("src/executor.rs")).expect("executor reads");
    let facade = source
        .split("impl Executor {")
        .nth(2)
        .expect("convenience impl is present");

    assert!(facade.contains("self.execute(Command::KvPut"));
    assert!(facade.contains("self.execute(Command::KvBatchPut"));
    assert!(!facade.contains(".kv("));
    assert!(!facade.contains(".put("));
    assert!(!facade.contains(".put_batch("));
    assert!(!facade.contains(".delete("));
}

#[test]
fn source_contract_uses_kv_specific_value_outputs() {
    let output_source =
        fs::read_to_string(crate_root().join("src/output.rs")).expect("output reads");
    let tests_source =
        fs::read_to_string(crate_root().join("tests/command_contract.rs")).expect("tests read");
    let generic_optional = ["May", "be"].concat();
    let generic_versioned = ["May", "beVersioned"].concat();

    assert!(output_source.contains("KvValue"));
    assert!(output_source.contains("KvVersionedValue"));
    assert!(!output_source.contains(&generic_optional));
    assert!(!output_source.contains(&generic_versioned));
    assert!(!tests_source.contains(&generic_optional));
    assert!(!tests_source.contains(&generic_versioned));
}

#[test]
fn executor_benchmarks_do_not_bypass_commands() {
    let benchmark_root = workspace_root().join("benchmarks/src/bin");
    if !benchmark_root.exists() {
        return;
    }

    for file in source_files(&benchmark_root) {
        let name = file
            .file_name()
            .and_then(|name| name.to_str())
            .expect("benchmark source has a file name");
        if !name.contains("executor") {
            continue;
        }

        let text = fs::read_to_string(&file).expect("benchmark source reads");
        assert!(
            text.contains("Command::KvBatchPut"),
            "{} must use the serialized batch-put command",
            file.display()
        );
        for forbidden in [
            "strata_storage_next",
            "StorageRuntime",
            "CommitBatch",
            ".put_batch(",
            ".commit(",
        ] {
            assert!(
                !text.contains(forbidden),
                "{} bypassed executor commands with `{forbidden}`",
                file.display()
            );
        }
    }
}

fn forbidden_lower_layer_terms() -> &'static [&'static str] {
    &[
        "strata-storage-next",
        "strata_storage_next",
        "StorageRuntime",
        "CommitBatch",
        "CommitMutation",
        "StorageSpaceId",
        "StorageKey",
        "StorageValue",
        "BranchRequest",
        "Wal",
        "TableRuntime",
        "Lifecycle",
        "Compaction",
        "storage_api",
    ]
}

fn source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_source_files(root, &mut files);
    files
}

fn collect_source_files(root: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).expect("directory reads") {
        let entry = entry.expect("directory entry reads");
        let path = entry.path();
        if path.is_dir() {
            collect_source_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_root() -> PathBuf {
    crate_root()
        .parent()
        .and_then(Path::parent)
        .expect("crate is under workspace crates directory")
        .to_path_buf()
}
