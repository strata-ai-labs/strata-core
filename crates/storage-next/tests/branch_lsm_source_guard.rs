//! Source guards for the branch-LSM runtime boundary.

#![deny(unsafe_code)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn branch_lsm_source_does_not_import_upper_layers_or_engines() {
    let root = common::crate_root();
    let forbidden = [
        "crate::commit",
        "crate::lifecycle",
        "crate::api",
        "strata_engine",
        "strata-engine",
        "crates/engine",
        "use engine",
        "use crate::engine",
        "use super::engine",
    ];

    for file in branch_lsm_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read branch LSM source");
        let normalized = text.to_ascii_lowercase();
        for needle in forbidden {
            assert!(
                !normalized.contains(needle),
                "{} imports an upper layer or engine surface via {needle:?}",
                file.strip_prefix(&root).unwrap_or(&file).display()
            );
        }
    }
}

#[test]
fn branch_lsm_source_does_not_use_product_dtos_or_payload_vocabulary() {
    let root = common::crate_root();

    for file in branch_lsm_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read branch LSM source");
        for (line_number, line) in text.lines().enumerate() {
            assert!(
                !contains_forbidden_product_vocabulary(line),
                "{}:{} uses product DTO or payload vocabulary: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
        }
    }
}

#[test]
fn branch_lsm_source_does_not_use_io_backend_or_lifecycle_apis() {
    let root = common::crate_root();
    let forbidden_substrings = [
        "std::fs",
        "std::path",
        "std::env",
        "env::var",
        "env::var_os",
        "OpenOptions",
        "pread",
        "mmap",
        "memmap",
        "crate::backend",
        "crate::format",
        "crate::layout",
        "crate::service",
        "crate::service::wal",
        "crate::service::checkpoint",
        "crate::testkit",
    ];

    for file in branch_lsm_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read branch LSM source");
        for (line_number, line) in text.lines().enumerate() {
            for needle in forbidden_substrings {
                assert!(
                    !line.contains(needle),
                    "{}:{} uses IO, backend, lifecycle, or testkit API {needle:?}: {line}",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    line_number + 1
                );
            }
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
fn branch_lsm_runtime_stays_crate_private() {
    let root = common::crate_root();
    let lib = fs::read_to_string(root.join("src/lib.rs")).expect("read lib.rs");
    assert!(lib.contains("mod branch;"));
    assert!(!lib.contains("pub mod branch;"));

    for file in branch_lsm_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read branch LSM source");
        for (line_number, line) in text.lines().enumerate() {
            assert!(
                !is_public_surface_leak(line),
                "{}:{} exposes branch LSM API publicly: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
        }
    }
}

#[test]
fn branch_lsm_source_has_no_premature_behavior_entrypoints() {
    let root = common::crate_root();

    // L6C owns branch-local committed-row append and active rotation. Later
    // behavior-owning L6 slices should narrow this guard as they add concrete
    // read, fork, install, or compaction entrypoints.
    for file in branch_lsm_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read branch LSM source");
        for (line_number, line) in text.lines().enumerate() {
            assert!(
                !contains_premature_behavior_entrypoint(line),
                "{}:{} implements behavior before its owning L6 slice: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
        }
    }
}

#[test]
fn branch_lsm_source_guard_catches_required_forbidden_terms() {
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
    assert!(contains_forbidden_product_vocabulary(
        "let _: TransactionContext;"
    ));

    assert!(contains_forbidden_substring(
        "use crate::commit;",
        "crate::commit"
    ));
    assert!(contains_forbidden_substring(
        "use crate::lifecycle;",
        "crate::lifecycle"
    ));
    assert!(contains_forbidden_substring(
        "use crate::api;",
        "crate::api"
    ));
    assert!(contains_forbidden_substring(
        "use strata_engine_next::BranchRuntime;",
        "strata_engine"
    ));
    assert!(contains_forbidden_substring(
        "std::fs::read(\"branch\")",
        "std::fs"
    ));
    assert!(contains_ascii_word("let _: PathBuf;", "PathBuf"));
    assert!(contains_forbidden_substring(
        "use crate::backend;",
        "crate::backend"
    ));
    assert!(contains_forbidden_substring(
        "use crate::service;",
        "crate::service"
    ));
    assert!(is_public_surface_leak("pub struct BranchRuntime;"));
    assert!(is_public_surface_leak("pub fn read_latest() {}"));
    assert!(contains_premature_behavior_entrypoint(
        "pub(crate) fn read_latest() {}"
    ));
    assert!(contains_premature_behavior_entrypoint(
        "pub(crate) fn materialize_inherited_layer() {}"
    ));
    assert!(contains_premature_behavior_entrypoint(
        "pub(crate) fn install_snapshot_rows() {}"
    ));
    assert!(contains_premature_behavior_entrypoint(
        "pub(crate) fn compact_branch() {}"
    ));
    assert!(contains_premature_behavior_entrypoint(
        "pub(crate) fn fork_branch() {}"
    ));

    assert!(!is_public_surface_leak("pub(crate) struct BranchRuntime;"));
    assert!(!contains_premature_behavior_entrypoint(
        "pub(crate) fn append_committed_row() {}"
    ));
    assert!(!contains_premature_behavior_entrypoint(
        "pub(crate) fn rotate_active() {}"
    ));
    assert!(!contains_premature_behavior_entrypoint(
        "pub(crate) const fn latest() -> Self"
    ));
    assert!(!contains_premature_behavior_entrypoint(
        "pub(crate) const fn fork_version(self) -> CommitVersion"
    ));
    assert!(!contains_forbidden_product_vocabulary(
        "BranchId CommitVersion Timestamp StorageRow TableRuntimeFacts"
    ));
}

#[test]
fn branch_lsm_source_guard_catches_backend_operation_call_forms() {
    assert!(contains_forbidden_backend_operation(
        "source.read_object(&object)",
    ));
    assert!(contains_forbidden_backend_operation(
        "read_object(source, object)",
    ));
    assert!(contains_forbidden_backend_operation(
        "source.read_range::<Bytes>(&range)",
    ));
    assert!(contains_forbidden_backend_operation(
        "backend::write_object(object, bytes)",
    ));
    assert!(contains_forbidden_backend_operation(
        "append_object(segment, bytes)",
    ));
    assert!(contains_forbidden_backend_operation(
        "delete_object(object)",
    ));
    assert!(contains_forbidden_backend_operation("list_prefix(prefix)",));
    assert!(contains_forbidden_backend_operation(
        "object_metadata(object)",
    ));
    assert!(contains_forbidden_backend_operation(
        "publish_object(object)",
    ));
    assert!(!contains_forbidden_backend_operation(
        "let read_object_id = object_id;"
    ));
    assert!(!contains_forbidden_backend_operation(
        "let object_metadata_version = version;"
    ));
}

fn branch_lsm_source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rs_files(&root.join("src/branch"), &mut files);
    files.sort();
    files
}

fn collect_rs_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read branch source dir") {
        let entry = entry.expect("read branch source entry");
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && path.file_name().is_some_and(|name| name != "tests.rs")
        {
            files.push(path);
        }
    }
}

fn contains_forbidden_product_vocabulary(line: &str) -> bool {
    [
        "VersionedValue",
        "Versioned<",
        "strata_core::Value",
        "strata_core::Key",
        "Namespace",
        "TypeTag",
        "EntityRef",
        "JsonValue",
        "Graph",
        "Vector",
        "Search",
        "TransactionContext",
    ]
    .iter()
    .any(|needle| line.contains(needle))
        || contains_ascii_word(line, "Value")
        || contains_ascii_word(line, "Key")
}

fn contains_forbidden_substring(line: &str, needle: &str) -> bool {
    line.contains(needle)
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

fn contains_premature_behavior_entrypoint(line: &str) -> bool {
    [
        "fn read_latest",
        "fn getv",
        "fn read_as_of",
        "fn as_of",
        "fn read_history",
        "fn prefix_scan",
        "fn range_scan",
        "fn rewrite_branch",
        "fn create_inherited",
        "fn materialize",
        "fn install_immutable",
        "fn install_snapshot",
        "fn compact",
        "fn fork_branch",
    ]
    .iter()
    .any(|needle| line.contains(needle))
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
