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

    let full_filter = run_success(args_with_db(
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
            r#"{"conditions":[{"field":"kind","op":"eq","value":{"type":"string","value":"doc"}}]}"#,
            "--format",
            "json",
        ],
    ));
    let full_filter = stdout_json(&full_filter);
    assert_eq!(full_filter["type"], "vector_matches");
    assert_eq!(full_filter["data"].as_array().expect("matches").len(), 1);
    assert_eq!(full_filter["data"][0]["key"], "a");
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
fn vector_collection_history_delete_all_and_batch_commands_execute() {
    let temp = TempDir::new().expect("temp db parent");
    let db = db_path(&temp);
    create_collection(&db);
    upsert(&db, "a", "1,0", r#"{"kind":"doc"}"#);
    upsert(&db, "b", "0,1", r#"{"kind":"note"}"#);

    let list = run_success(args_with_db(
        &db,
        &["vector", "collection", "list", "--format", "json"],
    ));
    let list = stdout_json(&list);
    assert_eq!(list["type"], "vector_collection_list");
    assert_eq!(list["data"]["items"][0]["name"], "docs");

    let stats = run_success(args_with_db(
        &db,
        &["vector", "collection", "stats", "docs", "--format", "json"],
    ));
    let stats = stdout_json(&stats);
    assert_eq!(stats["type"], "vector_collection_list");
    assert_eq!(stats["data"]["items"][0]["count"], 2);

    let history = run_success(args_with_db(
        &db,
        &["vector", "history", "docs", "a", "--format", "json"],
    ));
    let history = stdout_json(&history);
    assert_eq!(history["type"], "vector_version_history");
    assert_eq!(history["data"].as_array().expect("history").len(), 1);

    let batch_upsert = run_success(args_with_db(
        &db,
        &[
            "vector",
            "batch-upsert",
            "docs",
            "--entries",
            r#"[{"key":"c","vector":[1,1],"metadata":{"kind":"doc"}},{"key":"d","vector":"0.5,0.5"}]"#,
            "--format",
            "json",
        ],
    ));
    let batch_upsert = stdout_json(&batch_upsert);
    assert_eq!(batch_upsert["type"], "vector_batch_upsert_results");
    assert_eq!(batch_upsert["data"]["mode"], "itemwise");
    assert_eq!(
        batch_upsert["data"]["items"]
            .as_array()
            .expect("batch items")
            .len(),
        2
    );

    let batch_get = run_success(args_with_db(
        &db,
        &[
            "vector",
            "batch-get",
            "docs",
            "--keys",
            r#"["c","missing"]"#,
            "--format",
            "json",
        ],
    ));
    let batch_get = stdout_json(&batch_get);
    assert_eq!(batch_get["type"], "vector_batch_get_results");
    assert_eq!(batch_get["data"]["items"][0]["result"]["found"], true);
    assert_eq!(batch_get["data"]["items"][1]["result"]["found"], false);

    let batch_delete = run_success(args_with_db(
        &db,
        &[
            "vector",
            "batch-delete",
            "docs",
            "--keys",
            r#"["c","d"]"#,
            "--format",
            "json",
        ],
    ));
    let batch_delete = stdout_json(&batch_delete);
    assert_eq!(batch_delete["type"], "vector_batch_delete_results");
    assert_eq!(batch_delete["data"]["applied"], true);

    let delete_all = run_success(args_with_db(
        &db,
        &["vector", "delete-all", "docs", "--format", "json"],
    ));
    let delete_all = stdout_json(&delete_all);
    assert_eq!(delete_all["type"], "vector_bulk_delete_result");
    assert_eq!(delete_all["data"]["deleted_count"], 2);

    let delete_collection = run_success(args_with_db(
        &db,
        &["vector", "collection", "delete", "docs", "--format", "json"],
    ));
    let delete_collection = stdout_json(&delete_collection);
    assert_eq!(delete_collection["type"], "bool");
    assert_eq!(delete_collection["data"], true);
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

    let missing_batch_keys = run_failure(args_with_db(
        &db,
        &["vector", "batch-get", "docs", "--format", "json"],
    ));
    let missing_batch_keys = stderr_json(&missing_batch_keys);
    assert_eq!(missing_batch_keys["error"]["message"], "missing --keys");

    let invalid_batch_entries = run_failure(args_with_db(
        &db,
        &[
            "vector",
            "batch-upsert",
            "docs",
            "--entries",
            r#"[{"key":"a","vector":{}}]"#,
            "--format",
            "json",
        ],
    ));
    let invalid_batch_entries = stderr_json(&invalid_batch_entries);
    assert_eq!(
        invalid_batch_entries["error"]["message"],
        "vector value for --entries.vector must be an array or string literal"
    );

    create_collection(&db);
    let empty_filter = run_failure(args_with_db(
        &db,
        &[
            "vector",
            "delete-by-filter",
            "docs",
            "--filter",
            "{}",
            "--format",
            "json",
        ],
    ));
    let empty_filter = stderr_json(&empty_filter);
    assert_eq!(
        empty_filter["error"]["code"],
        "invalid_argument.engine.vector_filter"
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
