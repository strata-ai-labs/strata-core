//! Source guards for the immutable table byte-format boundary.

#![deny(unsafe_code)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn table_format_surfaces_do_not_use_product_payload_vocabulary() {
    let root = common::crate_root();
    let forbidden_words = [
        "primitive",
        "transaction",
        "entity",
        "json",
        "graph",
        "vector",
        "search",
        "product",
    ];
    let forbidden_phrases = [
        "bincode value",
        "valid kv",
        "kv payload",
        "kv value",
        "engine payload",
        "valid engine",
    ];

    for file in table_format_surface_files(&root) {
        let text = fs::read_to_string(&file).expect("read table format source surface");
        for (line_number, line) in text.lines().enumerate() {
            if allowed_table_format_line(line) {
                continue;
            }
            let normalized = line.to_ascii_lowercase();
            for word in forbidden_words {
                assert!(
                    !contains_ascii_word(&normalized, word),
                    "{}:{} uses product table vocabulary {word:?}: {line}",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    line_number + 1
                );
            }
            for phrase in forbidden_phrases {
                assert!(
                    !normalized.contains(phrase),
                    "{}:{} uses product table vocabulary {phrase:?}: {line}",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    line_number + 1
                );
            }
        }
    }
}

#[test]
fn table_format_source_does_not_import_engine_crates() {
    let root = common::crate_root();
    let mut files = vec![root.join("Cargo.toml")];
    files.extend(rust_files(&root.join("src/format/table")));
    files.push(root.join("src/format/table_manifest.rs"));

    for file in files {
        let text = fs::read_to_string(&file).expect("read table format source");
        let normalized = text.to_ascii_lowercase();
        for needle in [
            "strata-engine",
            "strata_engine",
            "use engine",
            "use crate::engine",
            "use super::engine",
            "engine::primitives",
        ] {
            assert!(
                !normalized.contains(needle),
                "{} imports or depends on engine payload code via {needle:?}",
                file.strip_prefix(&root).unwrap_or(&file).display()
            );
        }
    }
}

#[test]
fn table_manifest_format_source_stays_below_service_and_lifecycle_layers() {
    assert_table_manifest_source_excludes(&[
        "std::fs",
        "std::path",
        "std::env",
        "OpenOptions",
        "File::",
        "crate::service",
        "crate::lifecycle",
        "strata_engine",
        "stratahub",
        "graph",
        "vector",
        "json",
        "merge",
        "cherry",
        "revert",
    ]);
}

#[test]
fn table_manifest_format_does_not_import_raw_io() {
    assert_table_manifest_source_excludes(&[
        "std::fs",
        "std::path",
        "std::env",
        "OpenOptions",
        "File::",
    ]);
}

#[test]
fn table_manifest_format_does_not_import_backend_services() {
    assert_table_manifest_source_excludes(&["crate::backend", "crate::service"]);
}

#[test]
fn table_manifest_format_uses_layout_table_object_classification() {
    let root = common::crate_root();
    let file = root.join("src/format/table_manifest.rs");
    let text = fs::read_to_string(&file).expect("read table manifest format source");

    assert!(
        text.contains("ObjectLayout::classify_table_object"),
        "src/format/table_manifest.rs must consume the layout-owned table object classifier"
    );
}

#[test]
fn table_manifest_format_does_not_import_lifecycle_execution() {
    assert_table_manifest_source_excludes(&["crate::lifecycle"]);
}

#[test]
fn table_manifest_format_does_not_import_engine_or_product_crates() {
    assert_table_manifest_source_excludes(&[
        "strata_engine",
        "strata-engine",
        "crate::engine",
        "primitive",
        "product",
    ]);
}

#[test]
fn table_manifest_format_does_not_import_stratahub() {
    assert_table_manifest_source_excludes(&["stratahub", "strata_hub"]);
}

#[test]
fn table_manifest_format_does_not_import_primitive_modules() {
    assert_table_manifest_source_excludes(&["primitive", "crate::primitive"]);
}

#[test]
fn table_manifest_format_does_not_use_product_workflow_words() {
    assert_table_manifest_source_excludes(&[
        "graph", "vector", "json", "merge", "cherry", "revert",
    ]);
}

#[test]
fn lower_layers_do_not_import_lifecycle_table_manifest_policy() {
    let root = common::crate_root();
    let mut files = Vec::new();
    files.extend(rust_files(&root.join("src/format")));
    files.extend(rust_files(&root.join("src/table")));
    files.extend(rust_files(&root.join("src/branch")));

    for file in files {
        let text = fs::read_to_string(&file).expect("read lower-layer source");
        let compact = text.split_whitespace().collect::<String>();
        for forbidden in [
            "crate::lifecycle",
            "LifecycleTableManifest",
            "table_manifest_policy",
        ] {
            assert!(
                !compact.contains(forbidden),
                "{} imports lifecycle table-manifest policy via {forbidden:?}",
                file.strip_prefix(&root).unwrap_or(&file).display()
            );
        }
    }
}

#[test]
fn table_manifest_fuzz_target_is_registered() {
    let root = common::crate_root();
    let manifest = fs::read_to_string(root.join("fuzz/Cargo.toml")).expect("read fuzz manifest");
    let target = fs::read_to_string(root.join("fuzz/fuzz_targets/format_table_manifest.rs"))
        .expect("read table manifest fuzz target");

    assert!(manifest.contains("name = \"format_table_manifest\""));
    assert!(manifest.contains("fuzz_targets/format_table_manifest.rs"));
    assert!(target.contains("FormatDecoder::TableManifest"));
}

fn table_format_surface_files(root: &Path) -> Vec<PathBuf> {
    let mut files = vec![
        root.join("src/format/fuzzing.rs"),
        root.join("src/testkit/format_fuzz.rs"),
        root.join("tests/format_golden.rs"),
        root.join("tests/table_properties.rs"),
        root.join("fuzz/fuzz_targets/format_table_artifact.rs"),
        root.join("fuzz/fuzz_targets/format_table_block.rs"),
        root.join("fuzz/fuzz_targets/format_table_manifest.rs"),
        root.join("src/format/table_manifest.rs"),
    ];
    files.extend(rust_files(&root.join("src/format/table")));
    files.sort();
    files
}

fn assert_table_manifest_source_excludes(forbidden: &[&str]) {
    let root = common::crate_root();
    let file = root.join("src/format/table_manifest.rs");
    let text = fs::read_to_string(&file).expect("read table manifest format source");

    for (line_number, line) in text.lines().enumerate() {
        let compact = line.split_whitespace().collect::<String>();
        for needle in forbidden {
            let line_to_scan = if needle
                .bytes()
                .any(|byte| byte.is_ascii_uppercase() || matches!(byte, b'_' | b'-'))
            {
                compact.clone()
            } else {
                compact.to_ascii_lowercase()
            };
            assert!(
                !line_to_scan.contains(needle),
                "{}:{} imports forbidden table-manifest dependency {needle:?}: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
        }
    }
}

fn allowed_table_format_line(line: &str) -> bool {
    line.contains("StorageSpaceId::engine")
        || line.contains("engine id")
        || line.contains("engine storage space")
        || line.contains("STRAKV")
        || line.contains("old storage table")
        || line.contains("golden vector")
        || line.contains("golden-vector")
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
