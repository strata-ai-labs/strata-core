//! Type-aware regeneration harness for the command-contract JSON fixtures.
//!
//! The wire representation of [`Bytes`](strata_executor::Bytes) changed
//! from an integer array to a base64 string (DSGN-5/DTO-2). Rather than editing
//! 80-plus fixtures by hand — and risking a genuine JSON array being mistaken
//! for a byte payload — this harness round-trips every fixture through the real
//! [`Command`]/[`Output`] types, whose deserializers accept the legacy
//! int-array form and whose serializers emit the canonical encoding. Only the
//! `Bytes` positions change; every other field is byte-identical because the
//! types decide what is bytes and what is a genuine array.
//!
//! It is `#[ignore]`d so it never runs in the normal suite. To regenerate:
//!
//! ```bash
//! STRATA_REGEN_FIXTURES=1 \
//!   cargo test -p strata-executor --test regenerate_fixtures -- --ignored --nocapture
//! ```
//!
//! Without `STRATA_REGEN_FIXTURES=1` the test is a no-op even under `--ignored`,
//! so an accidental `--ignored` run never rewrites checked-in fixtures.

use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;
use strata_executor::{
    Bytes, Command, HistoryItem, HistoryResult, Output, VectorData, VectorHistoryItem,
    VectorHistoryResult,
};

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current).expect("fixture dir is readable") {
            let path = entry.expect("dir entry is readable").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "json") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// Round-trips `text` through `T` and returns the canonical pretty-printed JSON
/// (trailing newline included). Returns `None` if the fixture is not a `T`.
fn regenerate_as<T: DeserializeOwned + Serialize>(text: &str) -> Option<String> {
    let value: T = serde_json::from_str(text).ok()?;
    Some(canonical_json(&value))
}

fn canonical_json<T: Serialize>(value: &T) -> String {
    let mut pretty = serde_json::to_string_pretty(value).expect("value serializes");
    pretty.push('\n');
    pretty
}

/// Doc-pointer wire examples referenced by the IDL metadata but asserted by no
/// golden test. They drifted from the current type shapes, so they cannot be
/// round-tripped from disk; regenerate them from the same canonical values the
/// output-case round-trip suite exercises.
fn orphan_wire_examples() -> Vec<(&'static str, String)> {
    let kv_history = Output::VersionHistory(Some(HistoryResult::new(vec![HistoryItem::new(
        Some(Bytes::from("one")),
        false,
        1,
        10,
    )])));
    let vector_history = Output::VectorVersionHistory(Some(VectorHistoryResult::new(vec![
        VectorHistoryItem::new(
            "doc-a".to_owned(),
            Some(VectorData::new(vec![1.0, 0.0], None)),
            1,
            10,
            Some(1),
            false,
        ),
        VectorHistoryItem::new("doc-a".to_owned(), None, 2, 20, None, true),
    ])));
    vec![
        (
            "responses/v1/kv/history_found.json",
            canonical_json(&kv_history),
        ),
        (
            "responses/v1/vector/history_found.json",
            canonical_json(&vector_history),
        ),
    ]
}

#[test]
#[ignore = "regeneration harness; run explicitly with STRATA_REGEN_FIXTURES=1"]
fn regenerate_command_contract_fixtures() {
    if std::env::var_os("STRATA_REGEN_FIXTURES").is_none() {
        eprintln!("STRATA_REGEN_FIXTURES not set — skipping fixture regeneration");
        return;
    }

    let root = fixtures_root();
    let mut rewritten = 0usize;
    let mut skipped = Vec::new();

    for path in json_files(&root) {
        let text = fs::read_to_string(&path).expect("fixture is readable");
        let relative = path.strip_prefix(&root).unwrap_or(&path);

        // Requests are always `Command`; responses are `Output` when tagged,
        // otherwise a bare shared sub-struct that carries no `Bytes` and is
        // already canonical — leave those untouched.
        let regenerated = if relative.starts_with("requests") {
            regenerate_as::<Command>(&text)
        } else {
            regenerate_as::<Output>(&text)
        };

        match regenerated {
            Some(canonical) if canonical != text => {
                fs::write(&path, canonical).expect("fixture is writable");
                rewritten += 1;
                eprintln!("rewrote {}", relative.display());
            }
            Some(_) => {}
            None => skipped.push(relative.display().to_string()),
        }
    }

    for (relative, canonical) in orphan_wire_examples() {
        let path = root.join(relative);
        let current = fs::read_to_string(&path).unwrap_or_default();
        if current != canonical {
            fs::write(&path, canonical).expect("orphan fixture is writable");
            rewritten += 1;
            eprintln!("rewrote {relative} (orphan wire example)");
        }
        skipped.retain(|entry| entry != relative);
    }

    eprintln!(
        "regeneration complete: {rewritten} rewritten, {} skipped",
        skipped.len()
    );
    for path in &skipped {
        eprintln!("  skipped (not a Command/Output): {path}");
    }
}
