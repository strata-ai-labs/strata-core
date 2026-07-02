//! Integration coverage for executor-backed KV commands in the V1 CLI.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{json, Value};
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

fn put(db: &Path, key: &str, value: &str) {
    run_success([
        OsStr::new("--db"),
        db.as_os_str(),
        OsStr::new("kv"),
        OsStr::new("put"),
        OsStr::new(key),
        OsStr::new(value),
    ]);
}

fn args_with_db(db: &Path, parts: &[&str]) -> Vec<OsString> {
    let mut args = vec![OsString::from("--db"), db.as_os_str().to_owned()];
    args.extend(parts.iter().map(OsString::from));
    args
}

fn assert_usage_error(args: Vec<OsString>, expected_message: &str) {
    let output = run_failure(args);
    let error = stderr_json(&output);
    assert_eq!(error["error"]["code"], "invalid_argument.cli.usage");
    assert_eq!(error["error"]["message"], expected_message);
}

#[test]
fn kv_put_get_delete_roundtrip_uses_durable_database() {
    let temp = TempDir::new().expect("temp db parent");
    let db = db_path(&temp);

    let put = run_success([
        OsStr::new("--db"),
        db.as_os_str(),
        OsStr::new("kv"),
        OsStr::new("put"),
        OsStr::new("user"),
        OsStr::new("Claude"),
    ]);
    let put = stdout(&put);
    assert!(put.contains("ok\n"));
    assert!(put.contains("key: user"));
    assert!(put.contains("effect: created"));
    assert!(put.contains("version: "));

    let get = run_success([
        OsStr::new("--db"),
        db.as_os_str(),
        OsStr::new("kv"),
        OsStr::new("get"),
        OsStr::new("user"),
    ]);
    let get = stdout(&get);
    assert!(get.contains("found\n"));
    assert!(get.contains("value: Claude"));

    let exists = run_success([
        OsStr::new("--db"),
        db.as_os_str(),
        OsStr::new("kv"),
        OsStr::new("exists"),
        OsStr::new("user"),
        OsStr::new("--format"),
        OsStr::new("json"),
    ]);
    let exists = stdout_json(&exists);
    assert_eq!(exists["type"], "bool");
    assert_eq!(exists["data"], true);

    let delete = run_success([
        OsStr::new("--db"),
        db.as_os_str(),
        OsStr::new("kv"),
        OsStr::new("delete"),
        OsStr::new("user"),
    ]);
    let delete = stdout(&delete);
    assert!(delete.contains("ok\n"));
    assert!(delete.contains("effect: deleted"));

    let missing = run_success([
        OsStr::new("--db"),
        db.as_os_str(),
        OsStr::new("kv"),
        OsStr::new("get"),
        OsStr::new("user"),
    ]);
    assert_eq!(stdout(&missing), "missing\n");
}

#[test]
fn kv_list_scan_and_count_return_executor_json_shapes() {
    let temp = TempDir::new().expect("temp db parent");
    let db = db_path(&temp);

    put(&db, "user:1", "Ada");
    put(&db, "user:2", "Grace");
    put(&db, "other", "Ignored");

    let list = run_success([
        OsStr::new("--db"),
        db.as_os_str(),
        OsStr::new("kv"),
        OsStr::new("list"),
        OsStr::new("--prefix"),
        OsStr::new("user"),
        OsStr::new("--limit"),
        OsStr::new("1"),
        OsStr::new("--format"),
        OsStr::new("json"),
    ]);
    let list = stdout_json(&list);
    assert_eq!(list["type"], "keys_page");
    assert_eq!(list["data"]["items"].as_array().expect("keys").len(), 1);
    assert_eq!(list["data"]["has_more"], true);
    assert!(list["data"]["cursor"].is_array());

    let scan = run_success([
        OsStr::new("--db"),
        db.as_os_str(),
        OsStr::new("kv"),
        OsStr::new("scan"),
        OsStr::new("--start"),
        OsStr::new("user"),
        OsStr::new("--limit"),
        OsStr::new("2"),
        OsStr::new("--format"),
        OsStr::new("json"),
    ]);
    let scan = stdout_json(&scan);
    assert_eq!(scan["type"], "kv_scan_result");
    assert_eq!(scan["data"]["items"].as_array().expect("rows").len(), 2);
    assert_eq!(
        scan["data"]["items"][0]["key"],
        json!([117, 115, 101, 114, 58, 49])
    );
    assert_eq!(scan["data"]["items"][0]["value"], json!([65, 100, 97]));

    let count = run_success([
        OsStr::new("--db"),
        db.as_os_str(),
        OsStr::new("kv"),
        OsStr::new("count"),
        OsStr::new("--prefix"),
        OsStr::new("user"),
        OsStr::new("--format"),
        OsStr::new("json"),
    ]);
    let count = stdout_json(&count);
    assert_eq!(count["type"], "uint");
    assert_eq!(count["data"], 2);
}

#[test]
fn kv_parser_errors_are_structured_cli_errors() {
    let no_db = run_failure([
        OsStr::new("kv"),
        OsStr::new("get"),
        OsStr::new("user"),
        OsStr::new("--format"),
        OsStr::new("json"),
    ]);
    let no_db = stderr_json(&no_db);
    assert_eq!(no_db["error"]["code"], "invalid_argument.cli.usage");
    assert_eq!(
        no_db["error"]["message"],
        "missing --db <path> for kv command"
    );

    let duplicate_db = run_failure([
        OsStr::new("--db"),
        OsStr::new("a"),
        OsStr::new("--db"),
        OsStr::new("b"),
        OsStr::new("kv"),
        OsStr::new("get"),
        OsStr::new("user"),
        OsStr::new("--format"),
        OsStr::new("json"),
    ]);
    let duplicate_db = stderr_json(&duplicate_db);
    assert_eq!(duplicate_db["error"]["code"], "invalid_argument.cli.usage");
    assert_eq!(duplicate_db["error"]["message"], "duplicate --db");

    let temp = TempDir::new().expect("temp db parent");
    let db = db_path(&temp);
    let invalid_limit = run_failure([
        OsStr::new("--db"),
        db.as_os_str(),
        OsStr::new("kv"),
        OsStr::new("list"),
        OsStr::new("--limit"),
        OsStr::new("nope"),
        OsStr::new("--format"),
        OsStr::new("json"),
    ]);
    let invalid_limit = stderr_json(&invalid_limit);
    assert_eq!(invalid_limit["error"]["code"], "invalid_argument.cli.usage");
    assert_eq!(
        invalid_limit["error"]["message"],
        "invalid integer value `nope` for --limit"
    );
    assert_usage_error(
        args_with_db(&db, &["kv", "nope", "--format", "json"]),
        "unknown kv operation `nope`",
    );
}

#[test]
fn kv_duplicate_parser_flags_are_structured_cli_errors() {
    let temp = TempDir::new().expect("temp db parent");
    let db = db_path(&temp);

    assert_usage_error(
        args_with_db(
            &db,
            &[
                "kv", "get", "user", "--branch", "a", "--branch", "b", "--format", "json",
            ],
        ),
        "duplicate --branch",
    );
    assert_usage_error(
        args_with_db(
            &db,
            &[
                "kv", "get", "user", "--space", "a", "--space", "b", "--format", "json",
            ],
        ),
        "duplicate --space",
    );
    assert_usage_error(
        args_with_db(
            &db,
            &[
                "kv", "list", "--prefix", "a", "--prefix", "b", "--format", "json",
            ],
        ),
        "duplicate --prefix",
    );
    assert_usage_error(
        args_with_db(
            &db,
            &[
                "kv", "list", "--cursor", "a", "--cursor", "b", "--format", "json",
            ],
        ),
        "duplicate --cursor",
    );
    assert_usage_error(
        args_with_db(
            &db,
            &[
                "kv", "scan", "--start", "a", "--start", "b", "--format", "json",
            ],
        ),
        "duplicate --start",
    );
    assert_usage_error(
        args_with_db(
            &db,
            &[
                "kv", "list", "--limit", "1", "--limit", "2", "--format", "json",
            ],
        ),
        "duplicate --limit",
    );
    assert_usage_error(
        args_with_db(
            &db,
            &[
                "kv", "get", "user", "--as-of", "1", "--as-of", "2", "--format", "json",
            ],
        ),
        "duplicate --as-of",
    );
}

#[test]
fn kv_executor_errors_are_rendered_as_public_status() {
    let temp = TempDir::new().expect("temp db parent");
    let db = db_path(&temp);

    let empty_key = run_failure([
        OsStr::new("--db"),
        db.as_os_str(),
        OsStr::new("kv"),
        OsStr::new("get"),
        OsStr::new(""),
        OsStr::new("--format"),
        OsStr::new("json"),
    ]);
    let empty_key = stderr_json(&empty_key);
    assert_eq!(empty_key["schema_version"], "strata.cli.output.v1");
    assert_eq!(empty_key["kind"], "error");
    assert_eq!(empty_key["error"]["code"], "invalid_argument.engine.kv_key");
    assert_eq!(empty_key["error"]["retry_policy"], "never");
    assert!(empty_key["error"]["reference_id"].is_string());
}

#[test]
fn kv_accepts_flag_like_operands_after_argument_delimiter() {
    let temp = TempDir::new().expect("temp db parent");
    let db = db_path(&temp);

    run_success(args_with_db(&db, &["kv", "put", "flag", "--", "--json"]));
    let flag_value = run_success(args_with_db(&db, &["kv", "get", "flag"]));
    assert!(stdout(&flag_value).contains("value: --json"));

    run_success(args_with_db(&db, &["kv", "put", "--", "--db", "literal"]));
    let db_key = run_success(args_with_db(&db, &["kv", "get", "--", "--db"]));
    assert!(stdout(&db_key).contains("value: literal"));
}
