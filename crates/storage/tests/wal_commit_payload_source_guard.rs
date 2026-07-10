//! Source guards for the WAL commit-payload boundary.

#![deny(unsafe_code)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn wal_commit_payload_surfaces_do_not_use_legacy_payload_vocabulary() {
    let root = common::crate_root();
    let forbidden_substrings = ["entityref", "messagepack", "msgpack", "rmp_serde"];
    let forbidden_words = [
        "primitive",
        "transaction",
        "entity",
        "json",
        "graph",
        "search",
    ];
    let forbidden_phrases = [
        "vector payload",
        "vector wal",
        "wal vector",
        "payload vector",
        "valid vector",
        "engine payload",
        "engine operation",
        "valid engine",
        "engine wal",
        "wal engine",
    ];

    for file in wal_commit_payload_surface_files(&root) {
        let text = fs::read_to_string(&file).expect("read WAL commit-payload source surface");
        for (line_number, line) in text.lines().enumerate() {
            let normalized = line.to_ascii_lowercase();
            for term in forbidden_substrings {
                assert!(
                    !normalized.contains(term),
                    "{}:{} uses legacy WAL-payload vocabulary {term:?}: {line}",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    line_number + 1
                );
            }
            for word in forbidden_words {
                assert!(
                    !contains_ascii_word(&normalized, word),
                    "{}:{} uses legacy WAL-payload vocabulary {word:?}: {line}",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    line_number + 1
                );
            }
            for phrase in forbidden_phrases {
                assert!(
                    !normalized.contains(phrase),
                    "{}:{} uses legacy WAL-payload vocabulary {phrase:?}: {line}",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    line_number + 1
                );
            }
        }
    }
}

fn contains_ascii_word(haystack: &str, needle: &str) -> bool {
    haystack.match_indices(needle).any(|(start, _)| {
        let end = start + needle.len();
        !is_ascii_word_byte(haystack.as_bytes().get(start.wrapping_sub(1)).copied())
            && !is_ascii_word_byte(haystack.as_bytes().get(end).copied())
    })
}

fn is_ascii_word_byte(byte: Option<u8>) -> bool {
    byte.is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[test]
fn wal_format_source_does_not_import_engine_crates() {
    let root = common::crate_root();
    let files = [
        root.join("Cargo.toml"),
        root.join("src/format/wal.rs"),
        root.join("src/format/wal/commit_payload.rs"),
    ];
    let forbidden_imports = [
        "strata-engine",
        "strata_engine",
        "use engine",
        "use crate::engine",
        "use super::engine",
        "engine::primitives",
    ];

    for file in files {
        let text = fs::read_to_string(&file).expect("read WAL format source");
        let normalized = text.to_ascii_lowercase();
        for needle in forbidden_imports {
            assert!(
                !normalized.contains(needle),
                "{} imports or depends on engine payload code via {needle:?}",
                file.strip_prefix(&root).unwrap_or(&file).display()
            );
        }
    }
}

fn wal_commit_payload_surface_files(root: &Path) -> Vec<PathBuf> {
    let mut files = vec![
        root.join("src/format/wal.rs"),
        root.join("src/format/wal/commit_payload.rs"),
        root.join("src/service/wal.rs"),
        root.join("src/testkit/format_fuzz.rs"),
        root.join("tests/format_golden.rs"),
        root.join("fuzz/fuzz_targets/format_wal_record.rs"),
        root.join("fuzz/fuzz_targets/format_wal_commit_payload.rs"),
    ];
    files.extend(rust_files(&root.join("src/service/wal/tests")));
    files.sort();
    files
}

fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();

    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path).expect("read source directory") {
            let entry = entry.expect("read source entry");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    files
}
