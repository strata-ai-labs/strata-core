//! Source guards for the commit-runtime boundary.

#![deny(unsafe_code)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn commit_runtime_source_does_not_import_upper_layers_engines_or_product_api() {
    let root = common::crate_root();
    let forbidden = [
        "strata_engine",
        "strata-engine",
        "crates/engine",
        "use engine",
        "use crate::engine",
        "use super::engine",
        "crate::api",
        "crate::lifecycle",
    ];

    for file in commit_runtime_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read commit runtime source");
        let normalized = text.to_ascii_lowercase();
        for needle in forbidden {
            assert!(
                !normalized.contains(needle),
                "{} imports an upper layer, lifecycle, or product API surface via {needle:?}",
                file.strip_prefix(&root).unwrap_or(&file).display()
            );
        }
    }
}

#[test]
fn commit_runtime_source_does_not_use_product_session_or_payload_vocabulary() {
    let root = common::crate_root();

    for file in commit_runtime_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read commit runtime source");
        for (line_number, line) in text.lines().enumerate() {
            assert!(
                !contains_forbidden_product_vocabulary(line),
                "{}:{} uses product/session vocabulary: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
        }
    }
}

#[test]
fn commit_runtime_source_does_not_use_table_backend_layout_or_io_apis() {
    let root = common::crate_root();
    let forbidden_substrings = [
        "std::fs",
        "std::path",
        "std::env",
        "env::var",
        "env::var_os",
        "OpenOptions",
        "pread",
        "rename(",
        "remove_file",
        "mmap",
        "memmap",
        "crate::table",
        "crate::backend",
        "crate::layout",
        "crate::object",
        "crate::service",
        "crate::service::wal",
        "crate::format::wal",
        "crate::testkit",
        "TableRuntime",
        "TableRow",
        "ObjectLayout",
        "ObjectName",
        "Backend",
        "SystemTime",
        "Instant::now",
        "Timestamp::now",
        "std::thread::sleep",
        "thread::sleep",
        "std::time",
        "std::time::Duration",
        "std::time::Instant",
        "tokio::",
        "async_std::",
        "smol::",
    ];

    for file in commit_runtime_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read commit runtime source");
        assert!(
            !contains_forbidden_storage_path_text_for_file(&text, &file),
            "{} uses forbidden lower-layer bypass or IO path",
            file.strip_prefix(&root).unwrap_or(&file).display()
        );
        for (line_number, line) in text.lines().enumerate() {
            for needle in forbidden_substrings {
                if is_allowed_commit_runtime_boundary_line(&file, line)
                    && matches!(
                        needle,
                        "crate::branch"
                            | "crate::service"
                            | "crate::service::wal"
                            | "crate::format::wal"
                    )
                {
                    continue;
                }
                assert!(
                    !line.contains(needle),
                    "{}:{} uses forbidden lower-layer bypass or IO API {needle:?}: {line}",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    line_number + 1
                );
            }
            assert!(
                !contains_forbidden_storage_path_for_file(line, &file),
                "{}:{} uses forbidden lower-layer bypass or IO path: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
            assert!(
                !contains_forbidden_backend_operation(line),
                "{}:{} uses backend operation vocabulary directly: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
            for word in ["Path", "PathBuf", "File"] {
                assert!(
                    !contains_ascii_word(line, word),
                    "{}:{} uses IO word {word:?}: {line}",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    line_number + 1
                );
            }
        }
    }
}

#[test]
fn commit_runtime_implementation_avoids_architecture_labels() {
    let root = common::crate_root();
    for file in commit_runtime_source_files(&root)
        .into_iter()
        .chain(commit_runtime_unit_test_files(&root).into_iter())
        .chain(commit_runtime_testkit_files(&root).into_iter())
        .chain(commit_runtime_integration_test_files(&root).into_iter())
    {
        let text = fs::read_to_string(&file).expect("read commit runtime source");
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
fn mark_deleting_is_only_called_from_delete_branch() {
    let root = common::crate_root();

    let mut production_files = Vec::new();
    common::source_guard_helpers::collect_rs_files(&root.join("src/commit"), &mut production_files);
    common::source_guard_helpers::collect_rs_files(&root.join("src/branch"), &mut production_files);
    common::source_guard_helpers::collect_rs_files(
        &root.join("src/lifecycle"),
        &mut production_files,
    );

    for file in production_files {
        let display = file
            .strip_prefix(&root)
            .unwrap_or(&file)
            .display()
            .to_string();
        let text = fs::read_to_string(&file).expect("read production source");
        for (line_number, line) in text.lines().enumerate() {
            if !line.contains("mark_deleting(") {
                continue;
            }
            assert!(
                mark_deleting_call_is_allowed(&display, &text, line_number),
                "{}:{} calls mark_deleting outside the allowed call sites \
                 (branch_registry definition or branch_lifecycle::delete_branch): {line}",
                display,
                line_number + 1
            );
        }
    }
}

#[test]
fn mark_deleting_call_classifier_accepts_definition_and_delete_branch_body() {
    let registry = "pub(crate) fn mark_deleting(&mut self, branch_id: BranchId) { }\n";
    assert!(mark_deleting_call_is_allowed(
        "src/commit/branch_registry.rs",
        registry,
        0,
    ));

    let in_delete_branch = "pub(crate) fn delete_branch(&mut self) -> Result<()> {\n    \
                            self.registry.mark_deleting(branch_id)?;\n    \
                            Ok(())\n}\n";
    assert!(mark_deleting_call_is_allowed(
        "src/lifecycle/branch_lifecycle.rs",
        in_delete_branch,
        1,
    ));

    let nested_brace = "pub(crate) fn delete_branch(&mut self) -> Result<()> {\n    \
                        if condition {\n        \
                            self.registry.mark_deleting(branch_id)?;\n    \
                        }\n    \
                        Ok(())\n}\n";
    assert!(mark_deleting_call_is_allowed(
        "src/lifecycle/branch_lifecycle.rs",
        nested_brace,
        2,
    ));
}

#[test]
fn mark_deleting_call_classifier_rejects_unrelated_callers() {
    let other_function = "pub(crate) fn other_method(&mut self) -> Result<()> {\n    \
                          self.registry.mark_deleting(branch_id)?;\n    \
                          Ok(())\n}\n";
    assert!(!mark_deleting_call_is_allowed(
        "src/lifecycle/branch_lifecycle.rs",
        other_function,
        1,
    ));

    let outside_any_function = "static FOO: () = ();\nlet _ = obj.mark_deleting(branch);\n";
    assert!(!mark_deleting_call_is_allowed(
        "src/lifecycle/branch_lifecycle.rs",
        outside_any_function,
        1,
    ));

    let wrong_file_path = "pub(crate) fn delete_branch(&mut self) {\n    \
                           self.registry.mark_deleting(branch_id)?;\n\
                           }\n";
    assert!(!mark_deleting_call_is_allowed(
        "src/commit/cache.rs",
        wrong_file_path,
        1,
    ));
}

fn mark_deleting_call_is_allowed(display_path: &str, source: &str, line_number: usize) -> bool {
    if display_path.ends_with("src/commit/branch_registry.rs") {
        return true;
    }
    if display_path.ends_with("src/lifecycle/branch_lifecycle.rs") {
        let Some(body_range) = function_body_byte_range(source, "fn delete_branch(") else {
            return false;
        };
        let byte_index = byte_index_of_line_start(source, line_number);
        return body_range.contains(&byte_index);
    }
    false
}

fn function_body_byte_range(source: &str, signature: &str) -> Option<std::ops::Range<usize>> {
    let sig_start = source.find(signature)?;
    let body_start_relative = source[sig_start..].find('{')?;
    let body_start = sig_start + body_start_relative;
    let bytes = source.as_bytes();
    let mut depth: i32 = 1;
    let mut index = body_start + 1;
    while index < bytes.len() && depth > 0 {
        match bytes[index] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        index += 1;
    }
    if depth == 0 {
        Some(body_start..index)
    } else {
        None
    }
}

fn byte_index_of_line_start(source: &str, line_number: usize) -> usize {
    source
        .lines()
        .take(line_number)
        .map(|line| line.len() + 1)
        .sum()
}

#[test]
fn commit_runtime_stays_crate_private() {
    let root = common::crate_root();
    let lib = fs::read_to_string(root.join("src/lib.rs")).expect("read lib.rs");
    assert!(lib.contains("mod commit;"));
    assert!(!lib.contains("pub mod commit;"));

    for file in commit_runtime_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read commit runtime source");
        for (line_number, line) in text.lines().enumerate() {
            assert!(
                !is_public_surface_leak(line),
                "{}:{} exposes commit runtime API publicly: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
        }
    }
}

#[test]
fn commit_runtime_source_guard_catches_product_vocabulary() {
    assert!(contains_forbidden_product_vocabulary(
        "let _: TransactionContext;"
    ));
    assert!(contains_forbidden_product_vocabulary(
        "let _: TransactionManager;"
    ));
    assert!(contains_forbidden_product_vocabulary(
        "begin_transaction();"
    ));
    assert!(contains_forbidden_product_vocabulary("rollback();"));
    assert!(contains_forbidden_product_vocabulary(
        "let _: VersionedValue;"
    ));
    assert!(contains_forbidden_product_vocabulary(
        "let _: Versioned<Row>;"
    ));
    assert!(contains_forbidden_product_vocabulary(
        "let _: strata_core::Value;"
    ));
    assert!(contains_forbidden_product_vocabulary("let _: Value;"));
    assert!(contains_forbidden_product_vocabulary("let _: Key;"));
    assert!(contains_forbidden_product_vocabulary("let _: EntityRef;"));
    assert!(contains_forbidden_product_vocabulary("let _: JsonValue;"));
    assert!(contains_forbidden_product_vocabulary(
        "let _: TransactionId;"
    ));
    assert!(contains_forbidden_product_vocabulary("let _: TxnId;"));
    assert!(contains_forbidden_product_vocabulary(
        "let _: TransactionID;"
    ));
    assert!(contains_forbidden_product_vocabulary("next_txn_id();"));
    assert!(contains_forbidden_product_vocabulary(
        "let transaction_id = 7;"
    ));
    assert!(contains_forbidden_product_vocabulary(
        "let transaction id = 7;"
    ));
    assert!(contains_forbidden_product_vocabulary(
        "let _: Transaction ID;"
    ));
    assert!(contains_forbidden_product_vocabulary(
        "recover transaction IDs;"
    ));
    assert!(contains_forbidden_product_vocabulary(
        "query as_of timestamp;"
    ));
    assert!(contains_forbidden_product_vocabulary(
        "let _: SessionContext;"
    ));
}

#[test]
fn commit_runtime_source_guard_catches_forbidden_imports_and_public_surface() {
    assert!(contains_forbidden_substring(
        "use strata_engine::CommitRuntime;",
        "strata_engine"
    ));
    assert!(contains_forbidden_substring(
        "use crate::api;",
        "crate::api"
    ));
    assert!(contains_forbidden_substring(
        "use crate::lifecycle;",
        "crate::lifecycle"
    ));
    assert!(contains_forbidden_substring(
        "use crate::table::TableRow;",
        "crate::table"
    ));
    assert!(contains_forbidden_substring(
        "use crate::branch::BranchState;",
        "crate::branch"
    ));
    assert!(contains_forbidden_substring(
        "use crate::backend;",
        "crate::backend"
    ));
    assert!(contains_forbidden_substring(
        "use crate::layout;",
        "crate::layout"
    ));
    assert!(contains_forbidden_substring(
        "use crate::service::wal::WalService;",
        "crate::service"
    ));
    assert!(contains_forbidden_substring(
        "use crate::service::manifest::DatabaseManifestService;",
        "crate::service"
    ));
    assert!(contains_forbidden_substring(
        "use crate::format::wal::WalRecord;",
        "crate::format::wal"
    ));
    assert!(contains_forbidden_storage_path(
        "use crate::{branch::BranchState};"
    ));
    assert!(!contains_forbidden_storage_path_for_file(
        "use crate::branch::BranchReadView;",
        Path::new("src/commit/conflict.rs")
    ));
    assert!(!contains_forbidden_storage_path_for_file(
        "use crate::branch::{BranchReadBound, BranchReadView};",
        Path::new("src/commit/conflict.rs")
    ));
    assert!(contains_forbidden_storage_path_for_file(
        "use crate::branch::BranchLocalState;",
        Path::new("src/commit/conflict.rs")
    ));
    assert!(!contains_forbidden_storage_path_for_file(
        "use crate::branch::BranchLocalState;",
        Path::new("src/commit/cache.rs")
    ));
    assert!(contains_forbidden_storage_path_for_file(
        "use crate::branch::BranchReadView;",
        Path::new("src/commit/cache.rs")
    ));
    assert!(contains_forbidden_storage_path_for_file(
        "use crate::branch::*;",
        Path::new("src/commit/conflict.rs")
    ));
    assert!(contains_forbidden_storage_path_for_file(
        "use crate::branch::BranchCompactionRequest;",
        Path::new("src/commit/conflict.rs")
    ));
    assert!(contains_forbidden_storage_path(
        "use crate::format::{wal::WalRecord};"
    ));
    assert!(contains_forbidden_storage_path(
        "use crate::service::{wal::WalService};"
    ));
    assert!(contains_forbidden_storage_path(
        "use crate::{format::{wal::WalRecord}};"
    ));
    assert!(contains_forbidden_storage_path(
        "use crate::{\n    format::{\n        wal::WalRecord,\n    },\n};"
    ));
    assert!(contains_forbidden_storage_path("use std::{fs, path};"));
    assert!(contains_forbidden_storage_path(
        "use std::{\n    fs,\n    path,\n};"
    ));
    assert!(is_public_surface_leak("pub struct CommitRuntime;"));
    assert!(is_public_surface_leak("pub enum CommitBatch {}"));
    assert!(is_public_surface_leak("pub fn commit() {}"));
    assert!(is_public_surface_leak("pub async fn commit() {}"));
    assert!(is_public_surface_leak("pub unsafe fn commit() {}"));
}

#[test]
fn commit_runtime_source_guard_allows_durable_wal_and_replay_boundaries() {
    assert!(!contains_forbidden_storage_path_for_file(
        "use crate::branch::BranchLocalState;",
        Path::new("src/commit/durable.rs")
    ));
    assert!(!contains_forbidden_storage_path_for_file(
        "use crate::service::{WalAppend, WalService, WalServiceError};",
        Path::new("src/commit/durable.rs")
    ));
    assert!(!contains_forbidden_storage_path_for_file(
        "use crate::format::{WalCommitPayload, WalRecord};",
        Path::new("src/commit/durable.rs")
    ));
    assert!(contains_forbidden_storage_path_for_file(
        "use crate::service::{WalAppend, WalService, WalServiceError};",
        Path::new("src/commit/cache.rs")
    ));
    assert!(!contains_forbidden_storage_path_for_file(
        "use crate::branch::{BranchHistoryOptions, BranchReadView, BranchRowSource};",
        Path::new("src/commit/replay.rs")
    ));
    assert!(!contains_forbidden_storage_path_for_file(
        "use crate::format::WalRecord;",
        Path::new("src/commit/replay.rs")
    ));
}

#[test]
fn commit_runtime_source_guard_catches_io_and_backend_terms() {
    assert!(contains_forbidden_substring(
        "std::fs::read(\"commit\")",
        "std::fs"
    ));
    assert!(contains_forbidden_substring(
        "SystemTime::now()",
        "SystemTime"
    ));
    assert!(contains_forbidden_substring(
        "Instant::now()",
        "Instant::now"
    ));
    assert!(contains_forbidden_substring(
        "Timestamp::now()",
        "Timestamp::now"
    ));
    assert!(contains_forbidden_substring(
        "std::thread::sleep(deadline)",
        "std::thread::sleep"
    ));
    assert!(contains_forbidden_substring(
        "std::time::Instant::now()",
        "std::time"
    ));
    assert!(contains_forbidden_substring(
        "tokio::spawn(task)",
        "tokio::"
    ));
    assert!(contains_forbidden_substring(
        "async_std::task::sleep(deadline)",
        "async_std::"
    ));
    assert!(contains_forbidden_substring(
        "smol::Timer::after(deadline)",
        "smol::"
    ));
    assert!(contains_ascii_word("let _: PathBuf;", "PathBuf"));
    assert!(contains_forbidden_backend_operation(
        "backend.read_object(object)",
    ));
    assert!(contains_forbidden_backend_operation(
        "publish_object(object, bytes)",
    ));
}

#[test]
fn commit_runtime_source_guard_allows_owned_terms() {
    assert!(!is_public_surface_leak("pub(crate) struct CommitRuntime;"));
    assert!(!contains_forbidden_substring(
        "use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};",
        "crate::branch"
    ));
    assert!(!contains_forbidden_storage_path(
        "use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};"
    ));
    assert!(!contains_forbidden_product_vocabulary(
        "BranchId CommitVersion Timestamp StorageRow WalRecord WalService"
    ));
    assert!(!contains_forbidden_backend_operation(
        "let read_object_id = object_id;"
    ));
}

#[test]
fn commit_runtime_source_guard_pins_timeline_module_boundary() {
    let root = common::crate_root();
    let timeline = root.join("src/commit/timeline.rs");
    let text = fs::read_to_string(&timeline).expect("read commit timeline source");

    // W3.2: `PhysicalKey` (legacy row-pair construction) is test/testkit-gated;
    // the production module keeps only the row types the validators read.
    assert!(text.contains("use crate::row::{StorageRow, StorageSpaceId};"));
    assert!(text.contains("use crate::row::PhysicalKey;"));
    assert!(text.contains("StorageSpaceId::COMMIT_TIMELINE"));
    assert!(!contains_forbidden_storage_path_text_for_file(
        &text, &timeline
    ));
    for line in text.lines() {
        assert!(!contains_forbidden_product_vocabulary(line));
        assert!(!contains_forbidden_backend_operation(line));
    }
}

#[test]
fn lower_storage_layers_do_not_import_commit_runtime_upward() {
    let root = common::crate_root();
    for source_dir in ["src/branch", "src/format", "src/service"] {
        let mut files = Vec::new();
        collect_rs_files(&root.join(source_dir), &mut files);
        for file in files {
            let text = fs::read_to_string(&file).expect("read lower-layer source");
            for (line_number, line) in text.lines().enumerate() {
                let compact: String = line
                    .chars()
                    .filter(|character| !character.is_whitespace())
                    .collect();
                assert!(
                    !compact.contains("crate::commit")
                        && !compact.contains("crate::{commit")
                        && !compact.contains("super::commit")
                        && !compact.contains("super::{commit"),
                    "{}:{} imports upward into commit runtime: {line}",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    line_number + 1
                );
            }
        }
    }
}

fn commit_runtime_source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rs_files(&root.join("src/commit"), &mut files);
    files.sort();
    files
}

fn commit_runtime_unit_test_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    common::source_guard_helpers::collect_rs_files_including_tests(
        &root.join("src/commit/tests"),
        &mut files,
    );
    files.sort();
    files
}

fn commit_runtime_testkit_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for candidate in [
        root.join("src/testkit/commit_runtime.rs"),
        root.join("src/testkit/commit_runtime_allocator.rs"),
        root.join("src/testkit/commit_runtime_model.rs"),
        root.join("src/testkit/commit_runtime_runner.rs"),
        root.join("src/testkit/commit_runtime_script.rs"),
    ] {
        if candidate.exists() {
            files.push(candidate);
        }
    }
    files.sort();
    files
}

fn commit_runtime_integration_test_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let tests_dir = root.join("tests");
    for entry in fs::read_dir(tests_dir).expect("read integration tests dir") {
        let path = entry.expect("read integration test entry").path();
        if path
            .file_stem()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("commit_runtime_"))
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("rs"))
        {
            files.push(path);
        }
    }
    files.sort();
    files
}

fn collect_rs_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read commit source dir") {
        let entry = entry.expect("read commit source entry");
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

fn contains_forbidden_product_vocabulary(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "TransactionContext",
        "TransactionManager",
        "begin_transaction",
        "rollback",
        "VersionedValue",
        "Versioned<",
        "strata_core::Value",
        "strata_core::Key",
        "Namespace",
        "EntityRef",
        "JsonValue",
        "Graph",
        "Vector",
        "Search",
        "TransactionId",
        "TxnId",
        "TransactionID",
        "next_txn_id",
        "transaction_id",
        "transaction id",
        "transaction-id",
    ]
    .iter()
    .any(|needle| line.contains(needle))
        || [
            "transactionid",
            "transaction id",
            "transaction ids",
            "transaction-id",
            "transaction-ids",
            "transaction_id",
            "transaction_ids",
            "as_of",
            "session",
        ]
        .iter()
        .any(|needle| lower.contains(needle))
        || contains_ascii_word(line, "Value")
        || contains_ascii_word(line, "Key")
}

fn contains_forbidden_substring(line: &str, needle: &str) -> bool {
    line.contains(needle)
}

fn contains_forbidden_storage_path(line: &str) -> bool {
    contains_forbidden_storage_path_for_file(line, Path::new("src/commit/other.rs"))
}

fn contains_forbidden_storage_path_text_for_file(text: &str, file: &Path) -> bool {
    let filtered = text
        .lines()
        .filter(|line| !is_allowed_commit_runtime_boundary_line(file, line))
        .collect::<String>();
    contains_forbidden_storage_path_for_file(&filtered, file)
}

fn contains_forbidden_storage_path_for_file(line: &str, file: &Path) -> bool {
    if is_allowed_commit_runtime_boundary_line(file, line) {
        let compact: String = line
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        let without_allowed = compact
            .replace("usecrate::branch::{BranchReadBound,BranchReadView};", "")
            .replace(
                "usecrate::branch::{BranchHistoryOptions,BranchReadView,BranchRowSource};",
                "",
            )
            .replace(
                "usecrate::branch::read::{BranchHistoryOptions,BranchReadView,BranchRowSource};",
                "",
            )
            .replace("usecrate::branch::{BranchLocalState,BranchReadView};", "")
            .replace(
                "usecrate::branch::read::{BranchReadBound,BranchReadView};",
                "",
            )
            .replace("usecrate::branch::read::BranchReadView;", "")
            .replace(
                "usecrate::branch::read::{BranchOwnRowMatch,BranchReadView};",
                "",
            )
            .replace("crate::branch::BranchOwnRowMatch", "")
            .replace("crate::branch::read::BranchOwnRowMatch", "")
            .replace("usecrate::branch::state::BranchLocalState;", "")
            .replace("usecrate::branch::BranchReadView;", "")
            .replace("crate::branch::BranchReadBound", "")
            .replace("crate::branch::BranchHistoryOptions", "")
            .replace("crate::branch::BranchRowSource", "")
            .replace("crate::branch::BranchReadView", "")
            .replace("usecrate::branch::BranchLocalState;", "")
            .replace("crate::branch::BranchLocalState", "")
            .replace(
                "usecrate::service::{WalAppend,WalService,WalServiceError};",
                "",
            )
            .replace("crate::service::WalAppend", "")
            .replace("crate::service::WalService", "")
            .replace("crate::service::WalServiceError", "")
            .replace("crate::service::wal::WalAppend", "")
            .replace("crate::service::wal::WalOperation", "")
            .replace("crate::service::wal::WalService", "")
            .replace("crate::service::wal::WalServiceError", "")
            .replace("usecrate::format::{WalCommitPayload,WalRecord};", "")
            .replace("usecrate::format::WalRecord;", "")
            .replace("crate::format::WalCommitPayload", "")
            .replace("crate::format::WalRecord", "");
        return contains_forbidden_storage_path(&without_allowed);
    }

    let compact: String = line
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    [
        "crate::table",
        "crate::{table",
        "crate::branch",
        "crate::{branch",
        "crate::backend",
        "crate::{backend",
        "crate::layout",
        "crate::{layout",
        "crate::object",
        "crate::{object",
        "crate::service",
        "crate::{service",
        "crate::service::wal",
        "crate::service::{wal",
        "crate::{service::wal",
        "crate::{service::{wal",
        "crate::format::wal",
        "crate::format::{wal",
        "crate::{format::wal",
        "crate::{format::{wal",
        "std::fs",
        "std::{fs",
        "std::path",
        "std::{path",
    ]
    .iter()
    .any(|needle| compact.contains(needle))
}

fn is_allowed_branch_runtime_line(file: &Path, line: &str) -> bool {
    let Some(file_name) = file.file_name() else {
        return false;
    };
    let trimmed = line.trim();
    if file_name == "conflict.rs" {
        return matches!(
            trimmed,
            "use crate::branch::BranchReadView;"
                | "use crate::branch::{BranchReadBound, BranchReadView};"
                | "use crate::branch::read::{BranchReadBound, BranchReadView};"
        );
    }

    file_name == "cache.rs" && trimmed == "use crate::branch::BranchLocalState;"
}

fn is_allowed_commit_runtime_boundary_line(file: &Path, line: &str) -> bool {
    if is_allowed_branch_runtime_line(file, line) {
        return true;
    }
    let Some(file_name) = file.file_name() else {
        return false;
    };
    let trimmed = line.trim();
    file_name == "durable.rs"
        && matches!(
            trimmed,
            "use crate::branch::BranchLocalState;"
                | "use crate::branch::{BranchLocalState, BranchReadView};"
                | "use crate::branch::read::BranchReadView;"
                | "use crate::branch::state::BranchLocalState;"
                | "use crate::format::{WalCommitPayload, WalRecord};"
                | "use crate::service::{WalAppend, WalService, WalServiceError};"
        )
        || file_name == "replay.rs"
            && matches!(
                trimmed,
                "use crate::branch::{BranchHistoryOptions, BranchReadView, BranchRowSource};"
                    | "use crate::branch::read::{BranchHistoryOptions, BranchReadView, BranchRowSource};"
                    | "use crate::branch::read::{BranchOwnRowMatch, BranchReadView};"
                    | "use crate::format::WalRecord;"
            )
}

fn contains_forbidden_backend_operation(line: &str) -> bool {
    [
        "read_object",
        "read_range",
        "write_object",
        "append_object",
        "delete_object",
        "list_prefix",
        "object_metadata",
        "publish_object",
    ]
    .iter()
    .any(|operation| contains_call_like_identifier(line, operation))
}

fn contains_call_like_identifier(line: &str, identifier: &str) -> bool {
    line.match_indices(identifier).any(|(index, _)| {
        let before = line[..index].chars().next_back();
        let rest = &line[index + identifier.len()..];
        !before.is_some_and(is_ascii_ident_char)
            && (rest.starts_with('(') || rest.starts_with("::<"))
    })
}

fn is_public_surface_leak(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("pub(crate)") || trimmed.starts_with("pub(super)") {
        return false;
    }
    [
        "pub struct ",
        "pub enum ",
        "pub trait ",
        "pub type ",
        "pub fn ",
        "pub async fn ",
        "pub unsafe fn ",
        "pub const ",
        "pub static ",
        "pub mod ",
        "pub use ",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(prefix))
}

fn contains_ascii_word(line: &str, word: &str) -> bool {
    line.match_indices(word).any(|(index, _)| {
        let before = line[..index].chars().next_back();
        let after = line[index + word.len()..].chars().next();
        !before.is_some_and(is_ascii_ident_char) && !after.is_some_and(is_ascii_ident_char)
    })
}

fn is_ascii_ident_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}
