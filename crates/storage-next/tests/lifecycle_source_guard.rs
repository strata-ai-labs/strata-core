//! Source guards for the lifecycle boundary.

#![deny(unsafe_code)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn lifecycle_source_does_not_import_engine_product_or_raw_io() {
    let root = common::crate_root();

    for file in lifecycle_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read lifecycle source");
        assert!(
            !contains_forbidden_import_or_io_text(&text),
            "{} uses forbidden lifecycle dependency or grouped IO surface",
            file.strip_prefix(&root).unwrap_or(&file).display()
        );
        for (line_number, line) in text.lines().enumerate() {
            assert!(
                !contains_forbidden_import_or_io(line),
                "{}:{} uses forbidden lifecycle dependency or IO surface: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
            assert!(
                !contains_forbidden_product_vocabulary(line),
                "{}:{} uses forbidden product lifecycle vocabulary: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
        }
    }
}

#[test]
fn lifecycle_stays_crate_private() {
    let root = common::crate_root();
    let lib = fs::read_to_string(root.join("src/lib.rs")).expect("read lib.rs");
    assert!(lib.contains("mod lifecycle;"));
    assert!(!lib.contains("pub mod lifecycle;"));

    for file in lifecycle_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read lifecycle source");
        for (line_number, line) in text.lines().enumerate() {
            assert!(
                !is_public_surface_leak(line),
                "{}:{} exposes lifecycle API publicly: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
        }
    }
}

#[test]
fn lower_layers_do_not_import_lifecycle_upward() {
    let root = common::crate_root();
    for source_dir in [
        "src/backend",
        "src/layout",
        "src/object",
        "src/format",
        "src/service",
        "src/table",
        "src/branch",
        "src/commit",
        "src/row",
    ] {
        let mut files = Vec::new();
        collect_rs_files(&root.join(source_dir), &mut files);
        for file in files {
            let text = fs::read_to_string(&file).expect("read lower layer source");
            assert!(
                !imports_lifecycle_text(&text),
                "{} imports upward into lifecycle",
                file.strip_prefix(&root).unwrap_or(&file).display()
            );
            for (line_number, line) in text.lines().enumerate() {
                assert!(
                    !imports_lifecycle(line),
                    "{}:{} imports upward into lifecycle: {line}",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    line_number + 1
                );
            }
        }
    }
}

#[test]
fn lifecycle_capability_validator_stays_preflight_only() {
    let root = common::crate_root();
    let path = root.join("src/lifecycle/capability.rs");
    let text = fs::read_to_string(&path).expect("read lifecycle capability source");
    assert!(
        !contains_forbidden_capability_preflight_dependency_text(&text),
        "lifecycle capability validator imports service/runtime assembly dependencies"
    );

    for (line_number, line) in text.lines().enumerate() {
        assert!(
            !contains_forbidden_capability_preflight_dependency(line),
            "src/lifecycle/capability.rs:{} imports or calls forbidden preflight dependency: {line}",
            line_number + 1
        );
    }
}

#[test]
fn lifecycle_cache_runtime_stays_cache_only() {
    let root = common::crate_root();
    let path = root.join("src/lifecycle/cache.rs");
    let text = fs::read_to_string(&path).expect("read lifecycle cache source");
    assert!(
        !contains_forbidden_cache_runtime_dependency_text(&text),
        "lifecycle cache runtime imports durable service or object-layout dependencies"
    );

    for (line_number, line) in text.lines().enumerate() {
        assert!(
            !contains_forbidden_cache_runtime_dependency(line),
            "src/lifecycle/cache.rs:{} imports or calls forbidden cache runtime dependency: {line}",
            line_number + 1
        );
    }
}

#[test]
fn lifecycle_durable_runtime_stays_bootstrap_only() {
    let root = common::crate_root();
    let path = root.join("src/lifecycle/durable.rs");
    let text = fs::read_to_string(&path).expect("read lifecycle durable source");

    for (line_number, line) in text.lines().enumerate() {
        assert!(
            !contains_forbidden_durable_runtime_dependency(line),
            "src/lifecycle/durable.rs:{} calls forbidden durable bootstrap dependency: {line}",
            line_number + 1
        );
    }
}

#[test]
fn lifecycle_recovery_runtime_does_not_call_commit_replay_or_product_hooks() {
    let root = common::crate_root();
    let path = root.join("src/lifecycle/recovery.rs");
    let text = fs::read_to_string(&path).expect("read lifecycle recovery source");

    for (line_number, line) in text.lines().enumerate() {
        assert!(
            !contains_forbidden_recovery_runtime_dependency(line),
            "src/lifecycle/recovery.rs:{} calls forbidden recovery dependency: {line}",
            line_number + 1
        );
    }
}

#[test]
fn lifecycle_source_guard_catches_fixture_violations() {
    assert_general_source_guard_fixtures();
    assert_capability_preflight_fixtures();
    assert_cache_runtime_fixtures();
    assert_durable_runtime_fixtures();
    assert_recovery_runtime_fixtures();
    assert_public_surface_fixtures();
}

fn assert_general_source_guard_fixtures() {
    assert!(contains_forbidden_import_or_io(
        "use strata_engine_next::StorageRuntime;"
    ));
    assert!(contains_forbidden_import_or_io("let _: OpenOptions;"));
    assert!(contains_forbidden_import_or_io(
        "use crate::testkit::LifecycleHarness;"
    ));
    assert!(contains_forbidden_import_or_io_text(
        "use crate::{\n    testkit::LifecycleHarness,\n};"
    ));
    assert!(contains_forbidden_import_or_io_text(
        "use crate::{\n    api::StorageApi,\n};"
    ));
    assert!(contains_forbidden_import_or_io(
        "let _ = std::fs::read(\"manifest\");"
    ));
    assert!(contains_forbidden_import_or_io_text(
        "use std::{\n    fs,\n    path,\n};"
    ));
    assert!(contains_forbidden_import_or_io("let _: PathBuf;"));
    assert!(contains_forbidden_import_or_io(
        "let _ = std::env::var(\"X\");"
    ));
    assert!(contains_forbidden_product_vocabulary(
        "Database::open with OpenOptions"
    ));
    assert!(contains_forbidden_product_vocabulary(
        "database::open with openoptions"
    ));
    assert!(contains_forbidden_product_vocabulary(
        "manual maintenance command"
    ));
    assert!(contains_forbidden_product_vocabulary(
        "let _: VersionedValue;"
    ));
    assert!(contains_forbidden_product_vocabulary("let _: EntityRef;"));
    assert!(contains_forbidden_product_vocabulary("let _: JsonValue;"));
    assert!(contains_forbidden_product_vocabulary("let _: Graph;"));
    assert!(contains_forbidden_product_vocabulary("let _: Vector;"));
    assert!(contains_forbidden_product_vocabulary("let _: Search;"));
    assert!(contains_forbidden_product_vocabulary(
        "let _: event module;"
    ));
    assert!(contains_forbidden_product_vocabulary("let _: Embedding;"));
    assert!(contains_forbidden_product_vocabulary("let _: Inference;"));
    assert!(contains_forbidden_product_vocabulary("use StrataHub;"));
    assert!(contains_forbidden_product_vocabulary("use stratahub;"));
    assert!(contains_forbidden_product_vocabulary("refresh follower"));
    assert!(contains_forbidden_product_vocabulary("follower mode"));
    assert!(contains_forbidden_product_vocabulary(
        "let _: TransactionContext;"
    ));
    assert!(contains_forbidden_product_vocabulary(
        "begin_transaction();"
    ));
    assert!(imports_lifecycle("use crate::lifecycle::LifecycleState;"));
    assert!(imports_lifecycle_text(
        "use crate::{\n    lifecycle::LifecycleState,\n};"
    ));
    assert!(!contains_forbidden_product_vocabulary(
        "BranchId CommitVersion StorageRow WalService RecoveryHealth MaintenanceTask"
    ));
    assert!(!contains_forbidden_import_or_io(
        "RecoveryHealth MaintenanceTask CloseOutcome"
    ));
}

fn assert_capability_preflight_fixtures() {
    assert!(contains_forbidden_capability_preflight_dependency(
        "use crate::service::WalService;"
    ));
    assert!(contains_forbidden_capability_preflight_dependency(
        "use crate::layout::ObjectLayout;"
    ));
    assert!(contains_forbidden_capability_preflight_dependency_text(
        "use crate::{\n    commit::CommitRuntime,\n};"
    ));
    assert!(contains_forbidden_capability_preflight_dependency(
        "backend.read_object(name)?;"
    ));
    assert!(contains_forbidden_capability_preflight_dependency(
        "backend.acquire_writer_lock(name)?;"
    ));
}

fn assert_cache_runtime_fixtures() {
    assert!(contains_forbidden_cache_runtime_dependency(
        "use crate::service::WalService;"
    ));
    assert!(contains_forbidden_cache_runtime_dependency_text(
        "use crate::{\n    layout::ObjectLayout,\n};"
    ));
    assert!(contains_forbidden_cache_runtime_dependency(
        "backend.read_object(name)?;"
    ));
    assert!(contains_forbidden_cache_runtime_dependency(
        "backend.read_range(name, range)?;"
    ));
    assert!(contains_forbidden_cache_runtime_dependency(
        "backend.write_object(name, bytes)?;"
    ));
    assert!(contains_forbidden_cache_runtime_dependency(
        "backend.delete_object(name)?;"
    ));
    assert!(contains_forbidden_cache_runtime_dependency(
        "backend.list_prefix(prefix)?;"
    ));
    assert!(contains_forbidden_cache_runtime_dependency(
        "backend.object_metadata(name)?;"
    ));
    assert!(contains_forbidden_cache_runtime_dependency(
        "backend.publish_object(name, bytes, mode)?;"
    ));
}

fn assert_durable_runtime_fixtures() {
    assert!(contains_forbidden_durable_runtime_dependency(
        "let name = \"locks/writer\";"
    ));
    assert!(!contains_forbidden_durable_runtime_dependency(
        "CommitReplayRuntime::new();"
    ));
    assert!(!contains_forbidden_durable_runtime_dependency(
        "let _: CommitReplayRequest;"
    ));
    assert!(contains_forbidden_durable_runtime_dependency(
        "WalRecord::new(version, branch, timestamp, payload);"
    ));
    assert!(contains_forbidden_durable_runtime_dependency(
        "checkpoint_service.checkpoint(request)?;"
    ));
    assert!(contains_forbidden_durable_runtime_dependency(
        "quarantine.load_inventory(branch, db, codec)?;"
    ));
}

fn assert_recovery_runtime_fixtures() {
    assert!(contains_forbidden_recovery_runtime_dependency(
        "CommitReplayRuntime::new();"
    ));
    assert!(contains_forbidden_recovery_runtime_dependency(
        "execute_durable_commit(request)?;"
    ));
    assert!(contains_forbidden_recovery_runtime_dependency(
        "visible.publish_from_facts(facts)?;"
    ));
    assert!(contains_forbidden_recovery_runtime_dependency(
        "primitive_registry.reconstruct(row)?;"
    ));
}

fn assert_public_surface_fixtures() {
    assert!(is_public_surface_leak("pub struct LifecycleRuntime;"));

    assert!(!is_public_surface_leak(
        "pub(crate) struct LifecycleRuntime;"
    ));
}

fn lifecycle_source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rs_files(&root.join("src/lifecycle"), &mut files);
    files.sort();
    files
}

fn collect_rs_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read source dir") {
        let entry = entry.expect("read source entry");
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_none_or(|name| name != "tests") {
                collect_rs_files(&path, files);
            }
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && path.file_name().is_some_and(|name| name != "tests.rs")
        {
            files.push(path);
        }
    }
}

fn contains_forbidden_import_or_io(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "strata_engine",
        "strata-engine",
        "crates/engine",
        "crate::api",
        "crate::testkit",
        "std::fs",
        "std::path",
        "std::env",
        "env::var",
        "mmap",
        "memmap",
        "openoptions",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
        || contains_ascii_word(line, "Path")
        || contains_ascii_word(line, "PathBuf")
        || contains_ascii_word(line, "File")
}

fn contains_forbidden_import_or_io_text(text: &str) -> bool {
    let compact: String = uncommented_text(text)
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    contains_forbidden_import_or_io(text)
        || [
            "usestd::{fs",
            "usestd::{path",
            "usestd::{env",
            "crate::{api",
            "crate::{testkit",
        ]
        .iter()
        .any(|needle| compact.contains(needle))
}

fn contains_forbidden_product_vocabulary(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "database::open",
        "openoptions",
        "productopen",
        "productrecovery",
        "versionedvalue",
        "entityref",
        "jsonvalue",
        "graph",
        "vector",
        "search",
        "event module",
        "embedding",
        "inference",
        "stratahub",
        "follower",
        "transactioncontext",
        "begin_transaction",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
        || [
            "public maintenance",
            "manual maintenance command",
            "refresh follower",
            "follower mode",
        ]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn imports_lifecycle(line: &str) -> bool {
    imports_lifecycle_text(line)
}

fn imports_lifecycle_text(text: &str) -> bool {
    let compact: String = uncommented_text(text)
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    compact.contains("crate::lifecycle")
        || (compact.contains("crate::{") && compact.contains("lifecycle::"))
        || compact.contains("super::lifecycle")
        || (compact.contains("super::{") && compact.contains("lifecycle::"))
}

fn contains_forbidden_capability_preflight_dependency(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "crate::service",
        "crate::layout",
        "crate::format",
        "crate::table",
        "crate::branch",
        "crate::commit",
        "objectlayout",
        "walservice",
        "databasemanifest",
        "snapshotservice",
        "tableobjectservice",
        "read_object(",
        "read_range(",
        "write_object(",
        "delete_object(",
        "list_prefix(",
        "object_metadata(",
        "append_object(",
        "sync_object(",
        "publish_object(",
        "conditional_create(",
        "conditional_update(",
        "acquire_writer_lock(",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn contains_forbidden_capability_preflight_dependency_text(text: &str) -> bool {
    let compact: String = uncommented_text(text)
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    contains_forbidden_capability_preflight_dependency(text)
        || [
            "crate::{service",
            "crate::{layout",
            "crate::{format",
            "crate::{table",
            "crate::{branch",
            "crate::{commit",
        ]
        .iter()
        .any(|needle| compact.contains(needle))
}

fn contains_forbidden_cache_runtime_dependency(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "crate::service",
        "crate::layout",
        "crate::format",
        "objectlayout",
        "walservice",
        "snapshotservice",
        "databasemanifestservice",
        "tablemanifestservice",
        "quarantineservice",
        "tableobjectservice",
        "read_object(",
        "read_range(",
        "write_object(",
        "delete_object(",
        "list_prefix(",
        "object_metadata(",
        "acquire_writer_lock(",
        "append_object(",
        "sync_object(",
        "publish_object(",
        "conditional_create(",
        "conditional_update(",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn contains_forbidden_cache_runtime_dependency_text(text: &str) -> bool {
    let compact: String = uncommented_text(text)
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    contains_forbidden_cache_runtime_dependency(text)
        || [
            "crate::{service",
            "crate::{layout",
            "crate::{format",
            "usecrate::service",
            "usecrate::layout",
            "usecrate::format",
        ]
        .iter()
        .any(|needle| compact.contains(needle))
        || contains_forbidden_cache_runtime_dependency(&compact)
}

fn contains_forbidden_durable_runtime_dependency(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "\"locks/writer\"",
        "walrecord::new",
        "checkpoint_service.checkpoint(",
        "load_inventory(",
        "read_after_commit_version(",
        "repair_latest_tail(",
        "delete_covered_segments(",
        "list_snapshots(",
        "open_reader(",
        "execute_cache_commit(",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn contains_forbidden_recovery_runtime_dependency(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "commitreplayruntime",
        "execute_durable_commit",
        "execute_cache_commit",
        "publish_from_facts(",
        "catch_up_to_recovered_version(",
        "catch_up_to_recovered_timestamp(",
        "primitive_registry",
        "reconstruct(",
        "stratahub",
        "follower",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn uncommented_text(text: &str) -> String {
    let mut uncommented = String::new();
    for line in text.lines() {
        uncommented.push_str(line.split("//").next().unwrap_or_default());
        uncommented.push('\n');
    }
    uncommented
}

fn is_public_surface_leak(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("pub(crate)") {
        return false;
    }
    [
        "pub struct ",
        "pub enum ",
        "pub trait ",
        "pub type ",
        "pub fn ",
        "pub const ",
        "pub static ",
        "pub mod ",
        "pub use ",
    ]
    .iter()
    .any(|needle| trimmed.starts_with(needle))
}

fn contains_ascii_word(line: &str, word: &str) -> bool {
    line.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|token| token == word)
}
