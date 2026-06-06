//! Source guards for the storage backend IO boundary.

#![deny(unsafe_code)]

mod common;

use std::path::Path;

#[test]
fn production_filesystem_io_stays_inside_localfs_backend() {
    let root = common::crate_root();
    let src = root.join("src");
    let allowed = root.join("src/backend/local_fs.rs");
    let mut files = Vec::new();
    common::source_guard_helpers::collect_rs_files(&src, &mut files);

    let violations: Vec<_> = files
        .into_iter()
        .filter(|file| should_scan_for_filesystem_io(file, &allowed))
        .flat_map(|file| filesystem_marker_violations(&root, &file))
        .collect();

    assert!(
        violations.is_empty(),
        "direct filesystem IO must stay in src/backend/local_fs.rs:\n{}",
        violations.join("\n")
    );
}

#[test]
fn production_delete_contract_exposes_no_parallel_delete_api() {
    let root = common::crate_root();
    let src = root.join("src");
    let mut files = Vec::new();
    common::source_guard_helpers::collect_rs_files(&src, &mut files);

    let violations: Vec<_> = files
        .into_iter()
        .filter(|file| is_production_source(file))
        .flat_map(|file| delete_contract_marker_violations(&root, &file))
        .collect();

    assert!(
        violations.is_empty(),
        "backend delete must remain a single delete_object method:\n{}",
        violations.join("\n")
    );
}

fn should_scan_for_filesystem_io(file: &Path, allowed: &Path) -> bool {
    is_production_source(file) && file != allowed
}

fn is_production_source(file: &Path) -> bool {
    !file.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        name == "testkit" || name == "tests" || name == "test_support" || name.ends_with("_tests")
    }) && file.file_name().is_some_and(|file_name| {
        let file_name = file_name.to_string_lossy();
        file_name != "tests.rs"
            && file_name != "test_support.rs"
            && !file_name.ends_with("_tests.rs")
    })
}

fn filesystem_marker_violations(root: &Path, file: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(file).expect("read source file");
    let relative = display_relative(root, file);
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            forbidden_filesystem_marker(line)
                .map(|marker| format!("{relative}:{} contains `{marker}`", index + 1))
        })
        .collect()
}

fn delete_contract_marker_violations(root: &Path, file: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(file).expect("read source file");
    let relative = display_relative(root, file);
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            forbidden_delete_contract_marker(line)
                .map(|marker| format!("{relative}:{} contains `{marker}`", index + 1))
        })
        .collect()
}

fn forbidden_filesystem_marker(line: &str) -> Option<&'static str> {
    if line.contains("std::fs") {
        return Some("std::fs");
    }
    if line.contains("use std::{") && line.contains("fs") {
        return Some("use std::{..., fs, ...}");
    }
    if contains_standalone_fs_path(line) {
        return Some("fs::");
    }
    if line.contains("File::open") {
        return Some("File::open");
    }
    if line.contains("File::create") {
        return Some("File::create");
    }
    if line.contains("OpenOptions::new") {
        return Some("OpenOptions::new");
    }
    if line.contains(".sync_all(") {
        return Some(".sync_all(");
    }
    None
}

fn forbidden_delete_contract_marker(line: &str) -> Option<&'static str> {
    for marker in ["delete_object_durable", "DeleteOptions", "DeleteMode"] {
        if line.contains(marker) {
            return Some(marker);
        }
    }
    None
}

fn contains_standalone_fs_path(line: &str) -> bool {
    line.match_indices("fs::").any(|(index, _)| {
        line[..index]
            .chars()
            .next_back()
            .is_none_or(|previous| !previous.is_ascii_alphanumeric() && previous != '_')
    })
}

fn display_relative(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn filesystem_boundary_scan_skips_localfs_backend_and_test_sources() {
    let root = Path::new("/crate");
    let allowed = root.join("src/backend/local_fs.rs");

    assert!(!should_scan_for_filesystem_io(&allowed, &allowed));
    assert!(!should_scan_for_filesystem_io(
        &root.join("src/testkit/backend.rs"),
        &allowed
    ));
    assert!(!should_scan_for_filesystem_io(
        &root.join("src/backend/tests/helpers.rs"),
        &allowed
    ));
    assert!(!should_scan_for_filesystem_io(
        &root.join("src/backend/tests.rs"),
        &allowed
    ));
    assert!(!should_scan_for_filesystem_io(
        &root.join("src/service/snapshot/publish_fault_tests.rs"),
        &allowed
    ));
    assert!(!should_scan_for_filesystem_io(
        &root.join("src/service/cache_mode_absence_tests/support.rs"),
        &allowed
    ));
    assert!(!should_scan_for_filesystem_io(
        &root.join("src/format/table/test_support.rs"),
        &allowed
    ));
    assert!(should_scan_for_filesystem_io(
        &root.join("src/service/wal.rs"),
        &allowed
    ));
}

#[test]
fn filesystem_marker_detection_ignores_backend_type_names() {
    assert_eq!(
        forbidden_filesystem_marker("Self::open(StorageOpenOptions::cache())"),
        None
    );
    assert_eq!(
        forbidden_filesystem_marker("crate::backend::local_fs::LocalFsBackend::new(root)"),
        None
    );
}

#[test]
fn filesystem_marker_detection_catches_direct_filesystem_io() {
    assert!(forbidden_filesystem_marker("std::fs::read(path)").is_some());
    assert!(forbidden_filesystem_marker("use std::{fs, path::Path};").is_some());
    assert!(forbidden_filesystem_marker("fs::remove_file(path)").is_some());
    assert!(forbidden_filesystem_marker("File::open(path)").is_some());
    assert!(forbidden_filesystem_marker("File::create(path)").is_some());
    assert!(forbidden_filesystem_marker("OpenOptions::new().read(true)").is_some());
    assert!(forbidden_filesystem_marker("file.sync_all()").is_some());
}

#[test]
fn delete_contract_marker_detection_catches_parallel_delete_api_shapes() {
    assert!(forbidden_delete_contract_marker("fn delete_object_durable(&self) {}").is_some());
    assert!(forbidden_delete_contract_marker("struct DeleteOptions;").is_some());
    assert!(forbidden_delete_contract_marker("enum DeleteMode { Durable }").is_some());
    assert_eq!(
        forbidden_delete_contract_marker("fn delete_object(&self) -> DeleteResult"),
        None
    );
}
