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
                if needle == "crate::branch" && is_allowed_branch_read_view_line(&file, line) {
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
        "use strata_engine_next::CommitRuntime;",
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
    assert!(contains_forbidden_storage_path_for_file(
        "use crate::branch::BranchLocalState;",
        Path::new("src/commit/conflict.rs")
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

    assert!(text.contains("use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};"));
    assert!(text.contains("StorageSpaceId::COMMIT_TIMELINE"));
    assert!(!contains_forbidden_storage_path_text_for_file(
        &text, &timeline
    ));
    for line in text.lines() {
        assert!(!contains_forbidden_product_vocabulary(line));
        assert!(!contains_forbidden_backend_operation(line));
    }
}

fn commit_runtime_source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rs_files(&root.join("src/commit"), &mut files);
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
        .filter(|line| !is_allowed_branch_read_view_line(file, line))
        .collect::<String>();
    contains_forbidden_storage_path_for_file(&filtered, file)
}

fn contains_forbidden_storage_path_for_file(line: &str, file: &Path) -> bool {
    if is_allowed_branch_read_view_line(file, line) {
        let compact: String = line
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        let without_allowed = compact
            .replace("usecrate::branch::BranchReadView;", "")
            .replace("crate::branch::BranchReadView", "");
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

fn is_allowed_branch_read_view_line(file: &Path, line: &str) -> bool {
    file.file_name().is_some_and(|name| name == "conflict.rs")
        && line.trim() == "use crate::branch::BranchReadView;"
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
