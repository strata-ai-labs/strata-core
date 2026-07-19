//! CLI json + space family behavior (TCP3.11a).
//!
//! Ports the space and json workflows from the shell scenario corpus
//! (`scripts/cli-corpus/01`/`02`) into real-binary integration tests. These
//! families had no Rust integration coverage, yet the corpus that first
//! exercised them surfaced several behavior fixes now pinned here: a stored
//! JSON `null` is a live document (present in get/list) distinct from a missing
//! one, and json list paginates by cursor.

#![deny(unsafe_code)]

use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

/// Runs `strata --db <db> --json [scope] <args>` and returns parsed stdout.
fn strata(db: &Path, args: &[&str]) -> Value {
    let output = run(db, args);
    assert!(
        output.status.success(),
        "command {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

fn run(db: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_strata"))
        .arg("--db")
        .arg(db)
        .arg("--json")
        .args(args)
        .env_remove("STRATA_DB")
        .output()
        .expect("run strata binary")
}

fn db(home: &TempDir) -> std::path::PathBuf {
    home.path().join("db")
}

#[test]
fn space_create_list_and_exists() {
    let home = tempfile::tempdir().expect("temp home");
    let db = db(&home);

    let created = strata(&db, &["space", "create", "docs"]);
    assert_eq!(created["type"], "space_create_result");
    assert_eq!(created["data"]["space"], "docs");
    strata(&db, &["space", "create", "cache"]);

    let list = strata(&db, &["space", "list"]);
    let names: Vec<&str> = list["data"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(names.contains(&"default") && names.contains(&"docs") && names.contains(&"cache"));

    assert_eq!(strata(&db, &["space", "exists", "docs"])["data"], true);
    assert_eq!(strata(&db, &["space", "exists", "ghost"])["data"], false);
}

#[test]
fn spaces_isolate_the_same_key() {
    let home = tempfile::tempdir().expect("temp home");
    let db = db(&home);
    strata(&db, &["space", "create", "docs"]);

    strata(&db, &["kv", "put", "shared", "default-val"]);
    strata(&db, &["--space", "docs", "kv", "put", "shared", "docs-val"]);

    // Human/raw modes decode kv bytes to text; assert the decoded value per space.
    let default = String::from_utf8_lossy(&run_raw(&db, &["kv", "get", "shared"]).stdout)
        .trim()
        .to_owned();
    let docs =
        String::from_utf8_lossy(&run_raw(&db, &["--space", "docs", "kv", "get", "shared"]).stdout)
            .trim()
            .to_owned();
    assert_eq!(default, "default-val");
    assert_eq!(docs, "docs-val");
}

fn run_raw(db: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_strata"))
        .arg("--db")
        .arg(db)
        .arg("--raw")
        .args(args)
        .env_remove("STRATA_DB")
        .output()
        .expect("run strata binary")
}

#[test]
fn space_delete_removes_it() {
    let home = tempfile::tempdir().expect("temp home");
    let db = db(&home);
    strata(&db, &["space", "create", "scratch"]);
    assert_eq!(strata(&db, &["space", "exists", "scratch"])["data"], true);

    strata(&db, &["space", "delete", "scratch"]);
    assert_eq!(strata(&db, &["space", "exists", "scratch"])["data"], false);
}

#[test]
fn json_stored_null_is_present_and_missing_is_distinct() {
    let home = tempfile::tempdir().expect("temp home");
    let db = db(&home);

    strata(&db, &["json", "set", "doc-null", "$", "null"]);

    // A stored JSON null is a live document: found, with an inner null value.
    let stored = strata(&db, &["json", "get", "doc-null", "$"]);
    assert_eq!(stored["type"], "json_versioned_value");
    assert_eq!(stored["data"]["found"], true);
    assert_eq!(stored["data"]["value"]["value"], Value::Null);

    // A never-written document is distinctly missing.
    let missing = strata(&db, &["json", "get", "missing", "$"]);
    assert_eq!(missing["data"]["found"], false);
}

#[test]
fn json_list_paginates_by_cursor_and_includes_stored_null() {
    let home = tempfile::tempdir().expect("temp home");
    let db = db(&home);
    strata(&db, &["json", "set", "doc-01", "$", r#"{"rank":1}"#]);
    strata(&db, &["json", "set", "doc-02", "$", r#"{"rank":2}"#]);
    strata(&db, &["json", "set", "doc-03", "$", r#"{"rank":3}"#]);
    strata(&db, &["json", "set", "doc-null", "$", "null"]);

    let first = strata(&db, &["json", "list", "--prefix", "doc-", "--limit", "2"]);
    assert_eq!(
        first["data"]["items"],
        serde_json::json!(["doc-01", "doc-02"])
    );
    assert_eq!(first["data"]["has_more"], true);

    let second = strata(
        &db,
        &[
            "json", "list", "--prefix", "doc-", "--limit", "2", "--cursor", "doc-02",
        ],
    );
    // The stored-null document lists like any other live document.
    assert_eq!(
        second["data"]["items"],
        serde_json::json!(["doc-03", "doc-null"])
    );
    assert_eq!(second["data"]["has_more"], false);

    assert_eq!(strata(&db, &["json", "count"])["data"], 4);
}

#[test]
fn json_index_create_list_and_drop() {
    let home = tempfile::tempdir().expect("temp home");
    let db = db(&home);

    let by_rank = strata(
        &db,
        &[
            "json",
            "index",
            "create",
            "by-rank",
            "$.rank",
            "--index-type",
            "numeric",
        ],
    );
    assert_eq!(by_rank["type"], "json_index_definition");
    assert_eq!(by_rank["data"]["name"], "by-rank");
    strata(
        &db,
        &[
            "json",
            "index",
            "create",
            "by-name",
            "$.name",
            "--index-type",
            "text",
        ],
    );

    let listed = strata(&db, &["json", "index", "list"]);
    let mut names: Vec<&str> = listed["data"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["by-name", "by-rank"]);

    strata(&db, &["json", "index", "drop", "by-name"]);
    let after = strata(&db, &["json", "index", "list"]);
    let remaining: Vec<&str> = after["data"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect();
    assert_eq!(remaining, ["by-rank"]);
}

#[test]
fn json_history_is_newest_first() {
    let home = tempfile::tempdir().expect("temp home");
    let db = db(&home);
    strata(&db, &["json", "set", "doc", "$", r#"{"n":1}"#]);
    strata(&db, &["json", "set", "doc", "$", r#"{"n":2}"#]);

    let history = strata(&db, &["json", "history", "doc"]);
    assert_eq!(history["type"], "json_version_history");
    let items = history["data"].as_array().expect("history array");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["value"], serde_json::json!({"n": 2}));
    assert_eq!(items[1]["value"], serde_json::json!({"n": 1}));
}
