//! Object layout property harness entry point.

#![deny(unsafe_code)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn object_layout_property_harness_has_crate_root() {
    assert!(common::crate_root().join("src/object/mod.rs").is_file());
    assert!(common::crate_root().join("src/layout/mod.rs").is_file());
}

#[test]
fn reserved_layout_names_stay_owned_by_layout_module() {
    let root = common::crate_root();
    let src = root.join("src");
    let allowed_files = [
        src.join("layout/mod.rs"),
        src.join("layout/tests.rs"),
        src.join("format/tests.rs"),
    ];
    let reserved_fragments = [
        "\"manifest\"",
        "\"manifest/",
        "\"wal\"",
        "\"wal/",
        "\"tables\"",
        "\"tables/",
        "\"snapshots\"",
        "\"snapshots/",
        "\"tmp\"",
        "\"tmp/",
        "\"quarantine\"",
        "\"quarantine/",
        "\"locks\"",
        "\"locks/",
        "\"meta\"",
        "\"meta/",
        "MANIFEST",
        "wal-",
        "snap-",
        "segments.manifest",
        "quarantine.manifest",
        "__quarantine__",
        "follower_state",
        "follower_audit",
    ];

    for file in rust_files(&src) {
        if allowed_files.iter().any(|allowed| allowed == &file) {
            continue;
        }

        let text = fs::read_to_string(&file).expect("read source file");
        for (line_number, line) in production_source_lines(&text) {
            for fragment in reserved_fragments {
                assert!(
                    !line.contains(fragment),
                    "{}:{} contains reserved layout fragment {fragment:?}",
                    file.strip_prefix(&root).unwrap_or(&file).display(),
                    line_number
                );
            }
        }
    }
}

#[test]
fn production_source_scan_skips_test_cfg_items_only() {
    let source = r#"
fn before() {
    let _ = "manifest/current";
}

#[cfg(test)]
mod tests {
    fn inside_test_module() {
        let _ = "tables/test-only";
    }
}

fn after() {
    let _ = "wal/after-test-module";
}

#[cfg(test)]
const TEST_ONLY: &str = "snapshots/test-only";

#[cfg(test)]
fn inline_test_only() {
    let _ = "tmp/inline-test-only";
}

fn final_item() {
    let _ = "locks/final";
}
"#;

    let production = production_source_lines(source);
    let production: Vec<_> = production.into_iter().map(|(_, line)| line).collect();
    let production = production.join("\n");

    assert!(production.contains("\"manifest/current\""));
    assert!(production.contains("\"wal/after-test-module\""));
    assert!(production.contains("\"locks/final\""));
    assert!(!production.contains("\"tables/test-only\""));
    assert!(!production.contains("\"snapshots/test-only\""));
    assert!(!production.contains("\"tmp/inline-test-only\""));
}

fn production_source_lines(source: &str) -> Vec<(usize, String)> {
    let mut lines = Vec::new();
    let mut pending_test_item = false;
    let mut skipping_header = false;
    let mut skipping_body_depth = None;

    for (index, line) in source.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim_start();

        if let Some(depth) = skipping_body_depth {
            let next_depth = depth + brace_delta(line);
            skipping_body_depth = (next_depth > 0).then_some(next_depth);
            continue;
        }

        if skipping_header {
            if trimmed.ends_with(';') {
                skipping_header = false;
                continue;
            }

            let depth = brace_delta(line);
            if depth > 0 {
                skipping_header = false;
                skipping_body_depth = Some(depth);
            } else if line.contains('{') {
                skipping_header = false;
            }
            continue;
        }

        if pending_test_item {
            if trimmed.is_empty() || trimmed.starts_with("#[") {
                continue;
            }

            pending_test_item = false;
            if trimmed.ends_with(';') {
                continue;
            }

            let depth = brace_delta(line);
            if depth > 0 {
                skipping_body_depth = Some(depth);
            } else if line.contains('{') {
                continue;
            } else {
                skipping_header = true;
            }
            continue;
        }

        if trimmed.starts_with("#[cfg(test)]") {
            pending_test_item = true;
            continue;
        }

        lines.push((line_number, line.to_owned()));
    }

    lines
}

fn brace_delta(line: &str) -> i32 {
    line.chars().fold(0, |depth, ch| match ch {
        '{' => depth + 1,
        '}' => depth - 1,
        _ => depth,
    })
}

fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();

    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path).expect("read source directory") {
            let entry = entry.expect("read source entry");
            let path = entry.path();
            if path.is_dir() {
                // `src/<layer>/tests/*.rs` are test files by convention —
                // the parent `mod.rs` gates them with `#[cfg(test)] mod
                // tests;`, so they hold test code rather than production
                // code. Likewise `src/testkit/*.rs` is gated by
                // `#[cfg(any(test, feature = "testkit"))]` in `lib.rs`
                // and exists only for test harnesses. The line-scanner
                // can't see the parent-mod attribute (it walks one
                // file at a time), so we skip these directories here
                // to keep test fixtures from tripping the
                // reserved-name guard.
                let dir_name = path.file_name().and_then(|name| name.to_str());
                if matches!(dir_name, Some("tests" | "testkit")) {
                    continue;
                }
                pending.push(path);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    files.sort();
    files
}
