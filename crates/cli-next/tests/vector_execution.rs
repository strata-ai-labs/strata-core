//! Integration coverage for executor-backed vector commands in the V1 CLI.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

fn strata_next() -> Command {
    Command::new(env!("CARGO_BIN_EXE_strata-next"))
}

fn run_success<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = strata_next()
        .args(args)
        .output()
        .expect("strata-next command should run");
    assert!(
        output.status.success(),
        "expected success; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn run_failure<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = strata_next()
        .args(args)
        .output()
        .expect("strata-next command should run");
    assert!(
        !output.status.success(),
        "expected failure; stdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    output
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout should be UTF-8")
}

fn stderr_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stderr).expect("stderr should be JSON")
}

fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout should be JSON")
}

fn db_path(temp: &TempDir) -> PathBuf {
    temp.path().join("db")
}

fn args_with_db(db: &Path, parts: &[&str]) -> Vec<OsString> {
    let mut args = vec![OsString::from("--db"), db.as_os_str().to_owned()];
    args.extend(parts.iter().map(OsString::from));
    args
}

fn create_collection(db: &Path) {
    run_success(args_with_db(
        db,
        &[
            "vector",
            "collection",
            "create",
            "docs",
            "--dimension",
            "2",
            "--metric",
            "cosine",
        ],
    ));
}

fn upsert(db: &Path, key: &str, vector: &str, metadata: &str) {
    run_success(args_with_db(
        db,
        &[
            "vector",
            "upsert",
            "docs",
            key,
            "--vector",
            vector,
            "--metadata",
            metadata,
        ],
    ));
}

#[test]
fn vector_collection_and_row_workflow_uses_durable_database() {
    let temp = TempDir::new().expect("temp db parent");
    let db = db_path(&temp);

    let created = run_success(args_with_db(
        &db,
        &["vector", "collection", "create", "docs", "--dimension", "2"],
    ));
    let created = stdout(&created);
    assert!(created.contains("docs\tdimension=2"));
    assert!(created.contains("count=0"));

    upsert(&db, "a", "1,0", r#"{"kind":"doc"}"#);
    upsert(&db, "b", "[0,1]", r#"{"kind":"note"}"#);

    let get = run_success(args_with_db(&db, &["vector", "get", "docs", "a"]));
    let get = stdout(&get);
    assert!(get.contains("found\n"));
    assert!(get.contains("key: a"));
    assert!(get.contains("dimension: 2"));
    assert!(get.contains(r#"metadata: {"kind":"doc"}"#));

    let exists = run_success(args_with_db(
        &db,
        &["vector", "exists", "docs", "a", "--format", "json"],
    ));
    let exists = stdout_json(&exists);
    assert_eq!(exists["type"], "bool");
    assert_eq!(exists["data"], true);

    let count = run_success(args_with_db(
        &db,
        &["vector", "count", "docs", "--format", "json"],
    ));
    let count = stdout_json(&count);
    assert_eq!(count["type"], "uint");
    assert_eq!(count["data"], 2);

    let keys = run_success(args_with_db(
        &db,
        &[
            "vector", "keys", "docs", "--prefix", "a", "--limit", "1", "--format", "json",
        ],
    ));
    let keys = stdout_json(&keys);
    assert_eq!(keys["type"], "vector_key_page");
    assert_eq!(keys["data"]["items"][0], "a");
}

#[test]
fn vector_query_and_index_query_return_executor_json_shapes() {
    let temp = TempDir::new().expect("temp db parent");
    let db = db_path(&temp);
    create_collection(&db);
    upsert(&db, "a", "1,0", r#"{"kind":"doc"}"#);
    upsert(&db, "b", "0,1", r#"{"kind":"note"}"#);

    let query = run_success(args_with_db(
        &db,
        &[
            "vector",
            "query",
            "docs",
            "--vector",
            "1,0",
            "--k",
            "2",
            "--filter",
            r#"{"kind":"doc"}"#,
            "--format",
            "json",
        ],
    ));
    let query = stdout_json(&query);
    assert_eq!(query["type"], "vector_matches");
    assert_eq!(query["data"].as_array().expect("matches").len(), 1);
    assert_eq!(query["data"][0]["key"], "a");

    let index_query = run_success(args_with_db(
        &db,
        &[
            "vector", "index", "query", "docs", "--query", "[1,0]", "--k", "2", "--format", "json",
        ],
    ));
    let index_query = stdout_json(&index_query);
    assert_eq!(index_query["type"], "vector_index_query");
    assert!(index_query["data"]["matches"].is_array());
    assert!(index_query["data"]["diagnostics"]["manifest_status"].is_string());
}

#[test]
fn vector_metadata_update_and_delete_paths_execute() {
    let temp = TempDir::new().expect("temp db parent");
    let db = db_path(&temp);
    create_collection(&db);
    upsert(&db, "a", "1,0", r#"{"kind":"doc"}"#);
    upsert(&db, "b", "0,1", r#"{"kind":"note"}"#);

    let update = run_success(args_with_db(
        &db,
        &[
            "vector",
            "metadata",
            "update",
            "docs",
            "a",
            "--patch",
            r#"{"tag":"updated"}"#,
        ],
    ));
    let update = stdout(&update);
    assert!(update.contains("matched: true"));

    let deleted = run_success(args_with_db(&db, &["vector", "delete", "docs", "a"]));
    let deleted = stdout(&deleted);
    assert!(deleted.contains("matched: true"));
    assert!(deleted.contains("effect: deleted"));

    let bulk = run_success(args_with_db(
        &db,
        &[
            "vector",
            "delete-by-filter",
            "docs",
            "--filter",
            r#"{"kind":"note"}"#,
        ],
    ));
    assert!(stdout(&bulk).contains("deleted_count: 1"));
}

#[test]
fn vector_parser_errors_are_structured_cli_errors() {
    let temp = TempDir::new().expect("temp db parent");
    let db = db_path(&temp);

    let no_db = run_failure(["vector", "collection", "list", "--format", "json"]);
    let no_db = stderr_json(&no_db);
    assert_eq!(no_db["error"]["code"], "invalid_argument.cli.usage");
    assert_eq!(
        no_db["error"]["message"],
        "missing --db <path> for vector command"
    );

    let duplicate_scope = run_failure(args_with_db(
        &db,
        &[
            "vector", "keys", "docs", "--branch", "a", "--branch", "b", "--format", "json",
        ],
    ));
    let duplicate_scope = stderr_json(&duplicate_scope);
    assert_eq!(duplicate_scope["error"]["message"], "duplicate --branch");

    let invalid_vector = run_failure(args_with_db(
        &db,
        &[
            "vector", "query", "docs", "--vector", "1,nope", "--k", "1", "--format", "json",
        ],
    ));
    let invalid_vector = stderr_json(&invalid_vector);
    assert_eq!(
        invalid_vector["error"]["message"],
        "invalid vector literal for --vector"
    );

    let unknown = run_failure(args_with_db(&db, &["vector", "nope", "--format", "json"]));
    let unknown = stderr_json(&unknown);
    assert_eq!(
        unknown["error"]["message"],
        "unknown vector operation `nope`"
    );
}

#[test]
fn vector_accepts_flag_like_operands_after_argument_delimiter() {
    let temp = TempDir::new().expect("temp db parent");
    let db = db_path(&temp);
    create_collection(&db);

    run_success(args_with_db(
        &db,
        &[
            "vector", "upsert", "docs", "--vector", "1,0", "--", "--json",
        ],
    ));
    let get = run_success(args_with_db(
        &db,
        &["vector", "get", "docs", "--", "--json"],
    ));
    assert!(stdout(&get).contains("key: --json"));
}
