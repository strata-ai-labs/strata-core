//! CLI integration suite: real `strata` binary invocations, each command in
//! its own process, against real cache and durable-local databases. This is
//! the cross-process execution coverage the CLI test plans call for — the
//! durability claims below only mean something because every step is a
//! separate OS process.

#![deny(unsafe_code)]

use std::path::Path;
use std::process::{Command, Output};

fn strata(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_strata"))
        .args(args)
        .env_remove("STRATA_DB")
        .output()
        .expect("run strata binary")
}

fn strata_env(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_strata"));
    command.args(args).env_remove("STRATA_DB");
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("run strata binary")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_ok(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{context} failed (status {:?})\nstdout: {}\nstderr: {}",
        output.status.code(),
        stdout(output),
        stderr(output)
    );
}

fn db_arg(dir: &Path) -> String {
    dir.join("db").to_string_lossy().into_owned()
}

// --- durable cross-process execution -----------------------------------

#[test]
fn kv_round_trip_survives_across_processes() {
    let dir = tempfile::tempdir().expect("tmp");
    let db = db_arg(dir.path());
    assert_ok(&strata(&["--db", &db, "kv", "put", "alpha", "1"]), "put");

    let get = strata(&["--db", &db, "kv", "get", "alpha"]);
    assert_ok(&get, "get in a second process");
    assert_eq!(
        stdout(&get).trim(),
        "1",
        "durable value must survive the process boundary"
    );

    let exists = strata(&["--db", &db, "kv", "exists", "alpha"]);
    assert_ok(&exists, "exists");
    assert!(
        stdout(&exists).contains("true"),
        "exists: {}",
        stdout(&exists)
    );

    assert_ok(
        &strata(&["--db", &db, "kv", "delete", "alpha"]),
        "delete in a third process",
    );
    let gone = strata(&["--db", &db, "kv", "get", "alpha"]);
    assert_ok(&gone, "get after delete");
    assert_eq!(
        stdout(&gone).trim(),
        "(nil)",
        "deleted key must read nil in a fourth process"
    );
}

#[test]
fn kv_list_and_count_agree_across_processes() {
    let dir = tempfile::tempdir().expect("tmp");
    let db = db_arg(dir.path());
    for (key, value) in [("user:a", "1"), ("user:b", "2"), ("other", "3")] {
        assert_ok(&strata(&["--db", &db, "kv", "put", key, value]), "seed put");
    }
    let count = strata(&["--db", &db, "--json", "kv", "count"]);
    assert_ok(&count, "count");
    let parsed: serde_json::Value =
        serde_json::from_str(stdout(&count).trim()).expect("count emits valid JSON");
    let rendered = parsed.to_string();
    assert!(
        rendered.contains('3'),
        "count JSON should report 3: {parsed}"
    );
    let list = strata(&["--db", &db, "kv", "list"]);
    assert_ok(&list, "list");
    for key in ["user:a", "user:b", "other"] {
        assert!(
            stdout(&list).contains(key),
            "list missing {key}: {}",
            stdout(&list)
        );
    }
}

#[test]
fn json_output_is_machine_parseable_and_typed() {
    let dir = tempfile::tempdir().expect("tmp");
    let db = db_arg(dir.path());
    assert_ok(&strata(&["--db", &db, "kv", "put", "k", "v"]), "put");
    let get = strata(&["--db", &db, "--json", "kv", "get", "k"]);
    assert_ok(&get, "json get");
    let parsed: serde_json::Value =
        serde_json::from_str(stdout(&get).trim()).expect("kv get --json emits valid JSON");
    assert_eq!(
        parsed["type"], "kv_versioned_value",
        "typed envelope: {parsed}"
    );
    assert_eq!(parsed["data"]["found"], true, "found flag: {parsed}");
}

#[test]
fn raw_output_is_script_friendly() {
    let dir = tempfile::tempdir().expect("tmp");
    let db = db_arg(dir.path());
    assert_ok(
        &strata(&["--db", &db, "--raw", "kv", "put", "r", "v"]),
        "raw put",
    );
    let get = strata(&["--db", &db, "--raw", "kv", "get", "r"]);
    assert_ok(&get, "raw get");
    assert_eq!(stdout(&get), "v\n", "raw get must emit exactly the value");
}

#[test]
fn missing_key_reads_nil_with_success_exit() {
    let dir = tempfile::tempdir().expect("tmp");
    let db = db_arg(dir.path());
    assert_ok(&strata(&["--db", &db, "kv", "put", "seed", "x"]), "seed");
    let get = strata(&["--db", &db, "kv", "get", "absent"]);
    assert_ok(&get, "missing-key get");
    assert_eq!(stdout(&get).trim(), "(nil)");
}

// --- targeting and refusal contracts ------------------------------------

#[test]
fn no_database_refuses_with_typed_error_and_exit_two() {
    let output = strata(&["kv", "get", "k"]);
    assert_eq!(output.status.code(), Some(2), "refusal must exit 2");
    let err = stderr(&output);
    assert!(
        err.contains("invalid_argument.cli.no_database"),
        "typed error code expected: {err}"
    );
    assert!(
        err.contains("--cache"),
        "hint should mention --cache: {err}"
    );
}

#[test]
fn conflicting_database_targets_refuse() {
    let dir = tempfile::tempdir().expect("tmp");
    let db = db_arg(dir.path());
    let both = strata(&["--db", &db, "--cache", "kv", "put", "k", "v"]);
    assert_eq!(both.status.code(), Some(2), "--db with --cache must refuse");
    let positional = strata(&[&db, "--db", &db, "info"]);
    assert_eq!(
        positional.status.code(),
        Some(2),
        "positional DB with --db must refuse: {}",
        stderr(&positional)
    );
}

#[test]
fn env_var_targets_the_database() {
    let dir = tempfile::tempdir().expect("tmp");
    let db = db_arg(dir.path());
    let put = strata_env(&["kv", "put", "envk", "envv"], &[("STRATA_DB", &db)]);
    assert_ok(&put, "put via STRATA_DB");
    let get = strata_env(&["kv", "get", "envk"], &[("STRATA_DB", &db)]);
    assert_ok(&get, "get via STRATA_DB");
    assert_eq!(stdout(&get).trim(), "envv");
}

#[test]
fn cache_mode_is_ephemeral_per_process() {
    let put = strata(&["--cache", "kv", "put", "ghost", "1"]);
    assert_ok(&put, "cache put");
    let get = strata(&["--cache", "kv", "get", "ghost"]);
    assert_ok(&get, "cache get in a new process");
    assert_eq!(
        stdout(&get).trim(),
        "(nil)",
        "cache data must not survive the process"
    );
}

// --- branches ------------------------------------------------------------

#[test]
fn branch_scoped_writes_stay_isolated_across_processes() {
    let dir = tempfile::tempdir().expect("tmp");
    let db = db_arg(dir.path());
    assert_ok(
        &strata(&["--db", &db, "branch", "create", "dev"]),
        "branch create",
    );
    assert_ok(
        &strata(&["--db", &db, "--branch", "dev", "kv", "put", "flag", "on"]),
        "branch-scoped put",
    );
    let default_read = strata(&["--db", &db, "kv", "get", "flag"]);
    assert_ok(&default_read, "default-branch read");
    assert_eq!(
        stdout(&default_read).trim(),
        "(nil)",
        "default branch must not see dev writes"
    );
    let dev_read = strata(&["--db", &db, "--branch", "dev", "kv", "get", "flag"]);
    assert_ok(&dev_read, "dev-branch read");
    assert_eq!(stdout(&dev_read).trim(), "on");
    let list = strata(&["--db", &db, "branch", "list"]);
    assert_ok(&list, "branch list");
    assert!(stdout(&list).contains("dev") && stdout(&list).contains("default"));
}

// --- vectors ---------------------------------------------------------------

#[test]
fn vector_upsert_and_query_survive_across_processes() {
    let dir = tempfile::tempdir().expect("tmp");
    let db = db_arg(dir.path());
    assert_ok(
        &strata(&["--db", &db, "vector", "collection", "create", "emb", "4"]),
        "collection create",
    );
    assert_ok(
        &strata(&["--db", &db, "vector", "upsert", "emb", "k1", "1,0,0,0"]),
        "upsert k1",
    );
    assert_ok(
        &strata(&["--db", &db, "vector", "upsert", "emb", "k2", "0,1,0,0"]),
        "upsert k2",
    );
    let query = strata(&[
        "--db", &db, "--json", "vector", "query", "emb", "1,0,0,0", "-k", "1",
    ]);
    assert_ok(&query, "query in a separate process");
    let parsed: serde_json::Value =
        serde_json::from_str(stdout(&query).trim()).expect("vector query --json is valid JSON");
    let rendered = parsed.to_string();
    assert!(
        rendered.contains("k1") && !rendered.contains("k2"),
        "nearest neighbor must be k1 alone: {rendered}"
    );
    let get = strata(&["--db", &db, "vector", "get", "emb", "k1"]);
    assert_ok(&get, "vector get");
    assert!(
        stdout(&get).contains('1'),
        "vector get output: {}",
        stdout(&get)
    );
}

// --- REPL / pipe -----------------------------------------------------------

#[test]
fn piped_repl_commands_execute_and_persist_durably() {
    use std::io::Write;
    let dir = tempfile::tempdir().expect("tmp");
    let db = db_arg(dir.path());
    let mut child = Command::new(env!("CARGO_BIN_EXE_strata"))
        .args(["--db", &db])
        .env_remove("STRATA_DB")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn repl");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"kv put a 42\nkv get a\n")
        .expect("pipe commands");
    let output = child.wait_with_output().expect("repl completes on EOF");
    assert!(output.status.success(), "repl exit: {output:?}");
    let out = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(out.contains("42"), "repl session must echo the read: {out}");

    // The REPL's writes are durable: a fresh process sees them.
    let get = strata(&["--db", &db, "kv", "get", "a"]);
    assert_ok(&get, "post-repl get");
    assert_eq!(stdout(&get).trim(), "42");
}

// --- init / observability ---------------------------------------------------

#[test]
fn init_prepares_home_and_is_idempotent() {
    let home = tempfile::tempdir().expect("tmp");
    let home_str = home.path().to_string_lossy().into_owned();
    let first = strata_env(&["--json", "init"], &[("STRATA_HOME", &home_str)]);
    assert_ok(&first, "init");
    let parsed: serde_json::Value =
        serde_json::from_str(stdout(&first).trim()).expect("init --json is valid JSON");
    assert_eq!(parsed["type"], "init", "typed envelope: {parsed}");
    assert_eq!(
        parsed["data"]["home"].as_str(),
        Some(home_str.as_str()),
        "init JSON names the prepared home: {parsed}"
    );
    let second = strata_env(&["init"], &[("STRATA_HOME", &home_str)]);
    assert_ok(&second, "second init must be idempotent");
}

#[test]
fn info_and_health_emit_valid_json_for_a_durable_database() {
    let dir = tempfile::tempdir().expect("tmp");
    let db = db_arg(dir.path());
    assert_ok(&strata(&["--db", &db, "kv", "put", "seed", "1"]), "seed");
    for command in ["info", "health", "describe"] {
        let output = strata(&["--db", &db, "--json", command]);
        assert_ok(&output, command);
        serde_json::from_str::<serde_json::Value>(stdout(&output).trim()).unwrap_or_else(|err| {
            panic!("{command} --json must parse: {err}\n{}", stdout(&output))
        });
    }
}
