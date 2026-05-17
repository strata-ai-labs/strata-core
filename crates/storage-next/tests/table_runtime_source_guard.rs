//! Source guards for the table-runtime boundary.

#![deny(unsafe_code)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn table_runtime_source_does_not_import_upper_layers_or_engines() {
    let root = common::crate_root();
    let forbidden = [
        "crate::api",
        "crate::branch",
        "crate::commit",
        "crate::lifecycle",
        "crate::testkit",
        "strata-engine",
        "strata_engine",
        "engine::primitives",
        "use engine",
        "use crate::engine",
        "use super::engine",
    ];

    for file in table_runtime_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read table runtime source");
        for needle in forbidden {
            let normalized = text.to_ascii_lowercase();
            assert!(
                !normalized.contains(needle),
                "{} imports an upper layer or engine surface via {needle:?}",
                file.strip_prefix(&root).unwrap_or(&file).display()
            );
        }
    }
}

#[test]
fn table_runtime_source_does_not_use_product_payload_vocabulary() {
    let root = common::crate_root();

    for file in table_runtime_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read table runtime source");
        for (line_number, line) in text.lines().enumerate() {
            assert!(
                !contains_forbidden_product_payload_vocabulary(line),
                "{}:{} uses product payload vocabulary: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
        }
    }
}

#[test]
fn table_runtime_source_does_not_use_filesystem_or_backend_apis() {
    let root = common::crate_root();
    let forbidden_substrings = [
        "std::fs",
        "std::path::Path",
        "std::os::unix::fs::FileExt",
        "crate::backend",
        ".read_object(",
        ".read_range(",
        ".write_object(",
        ".append_object(",
        ".delete_object(",
        ".list_prefix(",
        ".object_metadata(",
        ".publish(",
        "std::env",
        "env::var",
        "env::var_os",
    ];
    let forbidden_words = ["Backend", "File", "PathBuf"];

    for file in table_runtime_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read table runtime source");
        for (line_number, line) in text.lines().enumerate() {
            for needle in forbidden_substrings {
                assert!(
                    !line.contains(needle),
                    "{}:{} uses filesystem or backend API {needle:?}: {line}",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    line_number + 1
                );
            }
            for word in forbidden_words {
                assert!(
                    !contains_ascii_word(line, word),
                    "{}:{} uses filesystem or backend API word {word:?}: {line}",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    line_number + 1
                );
            }
        }
    }
}

#[test]
fn table_runtime_source_does_not_create_process_global_cache_state() {
    let root = common::crate_root();
    let forbidden = [
        "lazy_static",
        "once_cell",
        "OnceLock",
        "static mut",
        "GLOBAL_CACHE",
        "PROCESS_CACHE",
    ];

    for file in table_runtime_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read table runtime source");
        for needle in forbidden {
            assert!(
                !text.contains(needle),
                "{} creates process-global table cache state via {needle:?}",
                file.strip_prefix(&root).unwrap_or(&file).display()
            );
        }
    }
}

#[test]
fn table_runtime_stays_crate_private() {
    let root = common::crate_root();
    let lib = fs::read_to_string(root.join("src/lib.rs")).expect("read lib.rs");
    assert!(
        lib.contains("mod table;"),
        "crate root should keep table as an internal module"
    );
    assert!(
        !lib.contains("pub mod table;"),
        "crate root must not expose the table module publicly"
    );

    for file in table_runtime_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read table runtime source");
        for (line_number, line) in text.lines().enumerate() {
            assert!(
                !is_public_surface_leak(line),
                "{}:{} exposes table runtime API publicly: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
        }
    }
}

#[test]
fn table_runtime_public_surface_guard_catches_bare_public_forms() {
    for line in [
        "pub async fn open() {}",
        "pub const VALUE: usize = 1;",
        "pub enum Value {}",
        "pub extern \"C\" fn open() {}",
        "pub fn open() {}",
        "pub macro m() {}",
        "pub mod leaked;",
        "pub static VALUE: usize = 1;",
        "pub struct Value;",
        "pub trait Value {}",
        "pub type Value = ();",
        "pub union Value { raw: u64 }",
        "pub unsafe fn open() {}",
        "pub use inner::Value;",
    ] {
        assert!(is_public_surface_leak(line), "guard should reject {line:?}");
    }

    for line in [
        "pub(crate) struct TableRuntimeConfig;",
        "pub(super) fn package_private() {}",
        "pub(in crate::table) type Scoped = ();",
        "    pub(crate) const VALUE: usize = 1;",
    ] {
        assert!(
            !is_public_surface_leak(line),
            "guard should allow scoped visibility {line:?}"
        );
    }
}

#[test]
fn table_runtime_dependency_guard_catches_required_forbidden_terms() {
    for line in [
        "use crate::api;",
        "use crate::branch;",
        "use crate::commit;",
        "use crate::lifecycle;",
        "use crate::testkit;",
        "use strata_engine::Value;",
        "use engine::primitives::Value;",
    ] {
        assert!(
            contains_forbidden_upper_layer_or_engine(line),
            "guard should reject {line:?}"
        );
    }

    for line in [
        "use crates::storage::key_encoding;",
        "use crate::key_encoding::InternalKey;",
        "let _: key_encoding::InternalKey;",
        "let _: TypeTag;",
        "let _: Namespace;",
        "let _: EntityRef;",
        "let _: stored_value::StoredValue;",
        "let _: graph::GraphRecord;",
        "let _: vector::VectorRecord;",
        "let _: json::JsonRecord;",
        "let _: search::SearchRecord;",
        "let _: event::EventRecord;",
        "let _: transaction::TransactionRecord;",
    ] {
        assert!(
            contains_forbidden_product_payload_vocabulary(line),
            "guard should reject {line:?}"
        );
    }

    for line in [
        "use std::fs;",
        "use std::path::Path;",
        "use std::os::unix::fs::FileExt;",
        "let _: File;",
        "let _: PathBuf;",
        "source.read_range();",
        "backend.object_metadata();",
        "std::env::var(\"STRATA\");",
        "env::var_os(\"STRATA\");",
    ] {
        assert!(
            contains_forbidden_filesystem_backend_or_env(line),
            "guard should reject {line:?}"
        );
    }
}

fn table_runtime_source_files(root: &Path) -> Vec<PathBuf> {
    rust_files(&root.join("src/table"))
        .into_iter()
        .filter(|file| {
            !file
                .components()
                .any(|component| component.as_os_str() == "tests")
        })
        .collect()
}

fn is_public_surface_leak(line: &str) -> bool {
    line.trim_start().starts_with("pub ")
}

fn contains_forbidden_product_payload_vocabulary(line: &str) -> bool {
    let normalized = line.to_ascii_lowercase();
    [
        "crates/storage",
        "crates::storage",
        "crate::key_encoding",
        "key_encoding::",
        "entityref",
        "messagepack",
        "msgpack",
        "rmp_serde",
        "typetag",
        "namespace",
        "stored_value",
        "storedvalue",
    ]
    .iter()
    .any(|term| normalized.contains(term))
        || [
            "primitive",
            "transaction",
            "entity",
            "json",
            "graph",
            "vector",
            "search",
            "event",
            "product",
        ]
        .iter()
        .any(|word| contains_ascii_word(&normalized, word))
        || [
            "bincode value",
            "engine payload",
            "engine operation",
            "valid engine",
            "payload value",
        ]
        .iter()
        .any(|phrase| normalized.contains(phrase))
}

fn contains_forbidden_upper_layer_or_engine(line: &str) -> bool {
    let normalized = line.to_ascii_lowercase();
    [
        "crate::api",
        "crate::branch",
        "crate::commit",
        "crate::lifecycle",
        "crate::testkit",
        "strata-engine",
        "strata_engine",
        "engine::primitives",
        "use engine",
        "use crate::engine",
        "use super::engine",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn contains_forbidden_filesystem_backend_or_env(line: &str) -> bool {
    [
        "std::fs",
        "std::path::Path",
        "std::os::unix::fs::FileExt",
        "crate::backend",
        ".read_object(",
        ".read_range(",
        ".write_object(",
        ".append_object(",
        ".delete_object(",
        ".list_prefix(",
        ".object_metadata(",
        ".publish(",
        "std::env",
        "env::var",
        "env::var_os",
    ]
    .iter()
    .any(|needle| line.contains(needle))
        || ["Backend", "File", "PathBuf"]
            .iter()
            .any(|word| contains_ascii_word(line, word))
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

    files.sort();
    files
}
