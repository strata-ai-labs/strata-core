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
fn table_runtime_source_does_not_use_cursor_policy_vocabulary() {
    let root = common::crate_root();

    for file in table_runtime_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read table runtime source");
        for (line_number, line) in text.lines().enumerate() {
            assert!(
                !contains_forbidden_cursor_policy_vocabulary(line),
                "{}:{} uses cursor policy vocabulary: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
        }
    }
}

#[test]
fn table_runtime_source_does_not_use_filesystem_backend_service_or_env_apis() {
    let root = common::crate_root();
    let forbidden_substrings = [
        "std::fs",
        "std::path::Path",
        "std::path::PathBuf",
        "std::os::unix::fs::FileExt",
        "std::fs::File",
        "pread",
        "rename(",
        "remove_file(",
        "mmap",
        "memmap",
        "crate::backend",
        "crate::service",
        "super::service",
        "service::table",
        "testkit::",
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
                    "{}:{} uses filesystem, backend, service, or env API {needle:?}: {line}",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    line_number + 1
                );
            }
            for word in forbidden_words {
                assert!(
                    !contains_ascii_word(line, word),
                    "{}:{} uses filesystem, backend, service, or env API word {word:?}: {line}",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    line_number + 1
                );
            }
        }
    }
}

#[test]
fn table_runtime_source_does_not_use_object_layout_literals() {
    let root = common::crate_root();

    for file in table_runtime_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read table runtime source");
        for (line_number, line) in text.lines().enumerate() {
            assert!(
                !contains_forbidden_object_layout_literal(line),
                "{}:{} uses object layout vocabulary: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
        }
    }
}

#[test]
fn table_runtime_source_does_not_use_old_table_builder_vocabulary() {
    let root = common::crate_root();

    for file in table_runtime_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read table runtime source");
        for (line_number, line) in text.lines().enumerate() {
            assert!(
                !contains_forbidden_old_table_builder_vocabulary(line),
                "{}:{} uses old table builder vocabulary: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
        }
    }
}

#[test]
fn table_runtime_source_does_not_create_process_global_cache_state() {
    let root = common::crate_root();

    for file in table_runtime_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read table runtime source");
        let lines = text.lines().collect::<Vec<_>>();
        for (line_number, line) in lines.iter().enumerate() {
            assert!(
                !contains_process_global_cache_state(line),
                "{}:{} creates process-global table cache state: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
            assert!(
                !contains_static_cache_declaration(&lines, line_number),
                "{}:{} creates process-global table cache declaration: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
        }
    }
}

#[test]
fn table_runtime_source_does_not_use_unsafe_or_old_cache_identity() {
    let root = common::crate_root();

    for file in table_runtime_source_files(&root) {
        let text = fs::read_to_string(&file).expect("read table runtime source");
        for (line_number, line) in text.lines().enumerate() {
            assert!(
                !contains_forbidden_unsafe_or_old_cache_identity(line),
                "{}:{} uses unsafe code or old cache identity vocabulary: {line}",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                line_number + 1
            );
        }
    }
}

#[test]
fn table_runtime_compaction_source_does_not_embed_retention_policy_terms() {
    let root = common::crate_root();
    let file = root.join("src/table/compaction.rs");
    let text = fs::read_to_string(&file).expect("read table compaction source");

    for (line_number, line) in text.lines().enumerate() {
        assert!(
            !contains_forbidden_compaction_policy_vocabulary(line),
            "{}:{} embeds higher-layer compaction policy vocabulary: {line}",
            file.strip_prefix(&root).unwrap_or(&file).display(),
            line_number + 1
        );
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
#[allow(clippy::too_many_lines)]
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
        "let _: MvccCursor;",
        "let _: SnapshotCursor;",
        "cursor.as_of(version);",
        "let _: fork_version;",
        "let _: inherited_layer;",
        "rewrite_branch_id();",
        "visible_at(version);",
        "latest_row();",
        "ttl_filter(row);",
        "live_only();",
        "let _: MemtableEntry;",
        "let _: VersionedValue;",
    ] {
        assert!(
            contains_forbidden_cursor_policy_vocabulary(line),
            "guard should reject {line:?}"
        );
    }

    for line in [
        "use std::fs;",
        "use std::path::Path;",
        "use std::path::PathBuf;",
        "use std::os::unix::fs::FileExt;",
        "use std::fs::File;",
        "let _: File;",
        "let _: PathBuf;",
        "pread(fd, buf, len, offset);",
        "rename(source, target);",
        "remove_file(path);",
        "memmap2::Mmap::map(&file);",
        "source.read_range();",
        "backend.object_metadata();",
        "std::env::var(\"STRATA\");",
        "env::var_os(\"STRATA\");",
    ] {
        assert!(
            contains_forbidden_filesystem_backend_service_or_env(line),
            "guard should reject {line:?}"
        );
    }
    for line in [
        "use crate::service::table;",
        "use super::service;",
        "testkit::check_table_runtime_scaffold_contract();",
    ] {
        assert!(
            contains_forbidden_filesystem_backend_service_or_env(line),
            "guard should reject {line:?}"
        );
    }

    for line in [
        "let _ = b\"STRAKV\";",
        "let _: KVSegment;",
        "let _: SegmentBuilder;",
        "let _: SegmentId;",
        "use crate::segment_builder;",
        "use crate::layout::ObjectLayout;",
        "service::table::TableObjectService::new();",
        "let _: TableObjectName;",
        "publish_table_object();",
    ] {
        assert!(
            contains_forbidden_old_table_builder_vocabulary(line),
            "guard should reject {line:?}"
        );
    }
}

#[test]
fn table_runtime_dependency_guard_catches_object_layout_literals() {
    for line in [
        "let _ = \"tables/branch/0000/table\";",
        "let _ = \"wal/0000000000000001\";",
        "let _ = \"snapshots/0000000000000001\";",
        "let _ = \"manifest/current\";",
        "let _ = \"manifest\";",
        "use crate::object::ObjectName;",
        "use super::object::ObjectName;",
    ] {
        assert!(
            contains_forbidden_object_layout_literal(line),
            "guard should reject {line:?}"
        );
    }
}

#[test]
fn table_runtime_dependency_guard_catches_cache_identity_terms() {
    for line in [
        "unsafe { do_work(); }",
        "unsafe fn open() {}",
        "let _ = file_path_hash(path);",
        "let _ = path_hash(path);",
        "let file_id = 7;",
        "global_cache().get();",
    ] {
        assert!(
            contains_forbidden_unsafe_or_old_cache_identity(line),
            "guard should reject {line:?}"
        );
    }

    for line in [
        "lazy_static! { static ref CACHE: TableBlockCache = new_cache(); }",
        "static TABLE_CACHE: OnceLock<TableBlockCache> = OnceLock::new();",
        "static TABLE_CACHE: LazyLock<TableBlockCache> = LazyLock::new(new_cache);",
        "static mut GLOBAL_CACHE: Option<TableBlockCache> = None;",
        "let _ = GLOBAL_CACHE;",
        "let _ = PROCESS_CACHE;",
    ] {
        assert!(
            contains_process_global_cache_state(line),
            "guard should reject {line:?}"
        );
    }

    for line in [
        "use std::sync::{Arc, OnceLock};",
        "rows: OnceLock<TableRuntimeResult<Vec<TableRow>>>,",
        "let local = OnceLock::new();",
    ] {
        assert!(
            !contains_process_global_cache_state(line),
            "guard should allow reader-local memoization {line:?}"
        );
    }

    let split_static_once_lock = [
        "static TABLE_CACHE:",
        "    OnceLock<TableBlockCache> = OnceLock::new();",
    ];
    assert!(
        contains_static_cache_declaration(&split_static_once_lock, 0),
        "guard should reject split static OnceLock cache declarations"
    );

    let split_reader_field = ["rows:", "    OnceLock<TableRuntimeResult<Vec<TableRow>>>,"];
    assert!(
        !contains_static_cache_declaration(&split_reader_field, 0),
        "guard should allow split reader-local OnceLock fields"
    );
}

#[test]
fn table_runtime_dependency_guard_catches_compaction_policy_terms() {
    for line in [
        "let prune_floor = version;",
        "let snapshot_floor = version;",
        "let max_versions = 3;",
        "let bottommost = true;",
        "drop_expired(row);",
        "let branch_retention = policy;",
        "let inherited_table = table;",
        "let fork_gate = version;",
        "let materialization_policy = policy;",
        "retention_policy.apply();",
        "quarantine_table(table);",
        "checkpoint_table(table);",
        "let visible_row = row;",
        "install_manifest();",
        "lifecycle_cleanup();",
        "garbage_collect_tables();",
        "gc_table_object();",
    ] {
        assert!(
            contains_forbidden_compaction_policy_vocabulary(line),
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

fn contains_forbidden_cursor_policy_vocabulary(line: &str) -> bool {
    let normalized = line.to_ascii_lowercase();
    [
        "mvcc",
        "snapshot",
        "as_of",
        "fork",
        "inherit",
        "rewrite",
        "visible_at",
        "latest",
        "ttl_filter",
        "live_only",
        "memtableentry",
        "versionedvalue",
    ]
    .iter()
    .any(|term| normalized.contains(term))
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

fn contains_forbidden_filesystem_backend_service_or_env(line: &str) -> bool {
    [
        "std::fs",
        "std::path::Path",
        "std::path::PathBuf",
        "std::os::unix::fs::FileExt",
        "std::fs::File",
        "pread",
        "rename(",
        "remove_file(",
        "mmap",
        "memmap",
        "crate::backend",
        "crate::service",
        "super::service",
        "service::table",
        "testkit::",
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

fn contains_forbidden_old_table_builder_vocabulary(line: &str) -> bool {
    let normalized = line.to_ascii_lowercase();
    [
        "strakv",
        "kvsegment",
        "segmentbuilder",
        "segmentid",
        "segment_builder",
        "crate::layout",
        "objectlayout",
        "tableobjectservice",
        "table_object",
        "tableobjectname",
        "publish_table",
    ]
    .iter()
    .any(|term| normalized.contains(term))
}

fn contains_forbidden_object_layout_literal(line: &str) -> bool {
    let normalized = line.to_ascii_lowercase();
    [
        "tables/",
        "wal/",
        "snapshots/",
        "manifest/current",
        "crate::object",
        "super::object",
    ]
    .iter()
    .any(|term| normalized.contains(term))
        || contains_ascii_word(&normalized, "manifest")
}

fn contains_forbidden_unsafe_or_old_cache_identity(line: &str) -> bool {
    let normalized = line.to_ascii_lowercase();
    [
        "unsafe",
        "file_path_hash",
        "path_hash",
        "file_id",
        "global_cache",
    ]
    .iter()
    .any(|term| normalized.contains(term))
}

fn contains_process_global_cache_state(line: &str) -> bool {
    let normalized = line.to_ascii_lowercase();
    normalized.contains("lazy_static")
        || normalized.contains("once_cell")
        || normalized.contains("static mut")
        || normalized.contains("global_cache")
        || normalized.contains("process_cache")
        || (starts_static_declaration(&normalized)
            && (normalized.contains("oncelock") || normalized.contains("lazylock")))
}

fn contains_static_cache_declaration(lines: &[&str], line_number: usize) -> bool {
    let mut declaration = lines[line_number].to_ascii_lowercase();
    if !starts_static_declaration(&declaration) {
        return false;
    }

    for next in lines.iter().skip(line_number + 1).take(4) {
        declaration.push(' ');
        declaration.push_str(&next.to_ascii_lowercase());
        if next.contains(';') || next.contains('{') {
            break;
        }
    }

    declaration.contains("oncelock")
        || declaration.contains("lazylock")
        || declaration.contains("once_cell")
        || declaration.contains("tableblockcache")
        || declaration.contains("cache")
}

fn starts_static_declaration(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("static ")
        || trimmed.starts_with("pub static ")
        || trimmed.starts_with("pub(crate) static ")
        || trimmed.starts_with("pub(super) static ")
        || (trimmed.starts_with("pub(in ") && trimmed.contains(") static "))
}

fn contains_forbidden_compaction_policy_vocabulary(line: &str) -> bool {
    let normalized = line.to_ascii_lowercase();
    [
        "prune_floor",
        "snapshot_floor",
        "max_versions",
        "bottommost",
        "drop_expired",
        "branch_retention",
        "inherited_table",
        "fork_gate",
        "materialization",
        "retention",
        "quarantine",
        "checkpoint",
        "visible_row",
        "install_manifest",
        "lifecycle_cleanup",
        "garbage_collect",
        "gc_table_object",
    ]
    .iter()
    .any(|term| normalized.contains(term))
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
