//! Shared helpers for source-guard integration tests.

use std::fs;
use std::path::{Path, PathBuf};

/// Return true when the line carries a V1 architecture-layer or
/// milestone slice label that should not appear in production source,
/// test names, or closeout docstrings.
///
/// Flagged tokens:
/// - standalone milestone-layer labels.
/// - `L[4-9]` followed by a short uppercase suffix.
/// - standalone milestone labels.
/// - milestone labels followed by a short uppercase suffix.
///
/// Allowed tokens (not flagged):
/// - `L[0-3]` (LSM-level references such as `L0`, `L1`, `L2`, `L3`).
/// - `PascalCase` identifiers with architecture-layer digits in the name.
///   These are heuristically detected by the presence of any lowercase letter
///   in the four characters following the digit.
pub(crate) fn contains_milestone_label(line: &str) -> bool {
    let bytes = line.as_bytes();
    if bytes.len() < 2 {
        return false;
    }

    let mut i = 0;
    while i + 1 < bytes.len() {
        let c0 = bytes[i];
        let c1 = bytes[i + 1];

        let is_prefix = matches!((c0, c1), (b'L', b'4'..=b'9') | (b'M', b'0'..=b'9'));
        if !is_prefix {
            i += 1;
            continue;
        }

        // Word boundary before the prefix.
        if i > 0 && bytes[i - 1].is_ascii_alphanumeric() {
            i += 1;
            continue;
        }

        let after_idx = i + 2;
        if after_idx >= bytes.len() {
            return true;
        }
        let next = bytes[after_idx];
        if !next.is_ascii_alphabetic() {
            return true;
        }

        let lookahead_end = (after_idx + 4).min(bytes.len());
        let has_lowercase = bytes[after_idx..lookahead_end]
            .iter()
            .any(u8::is_ascii_lowercase);
        if !has_lowercase {
            return true;
        }

        i += 2;
    }
    false
}

/// Recursively collect every `*.rs` file under `dir`, skipping any
/// directory named `tests` and the file `tests.rs`. Matches the
/// existing recursive-collect pattern used by the lifecycle source
/// guard so behavior stays uniform across surface-specific guards.
pub(crate) fn collect_rs_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
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

/// Recursively collect every `*.rs` file under `dir` including
/// `tests` subdirectories. Use this when scanning test source roots
/// for label-absence (rather than scanning production source where
/// `tests` subdirs are excluded).
pub(crate) fn collect_rs_files_including_tests(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let entry = entry.expect("read source entry");
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files_including_tests(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
