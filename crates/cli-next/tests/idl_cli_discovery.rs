//! Integration coverage for IDL-driven command discovery in the V1 CLI.

use std::process::{Command, Output};

use serde_json::Value;
use strata_executor_next::cli_metadata::CliCommandCatalog;
use tempfile::TempDir;

fn strata_next() -> Command {
    Command::new(env!("CARGO_BIN_EXE_strata-next"))
}

fn run_success(args: &[&str]) -> Output {
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

fn run_failure(args: &[&str]) -> Output {
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

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr should be UTF-8")
}

#[test]
fn commands_lists_kv_from_generated_metadata() {
    let output = run_success(&["commands", "--family", "kv"]);

    assert_eq!(
        stdout(&output),
        include_str!("fixtures/commands_family_kv.txt")
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn commands_json_is_structured_and_family_scoped() {
    let output = run_success(&["commands", "--family", "vector", "--format", "json"]);
    let value: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");

    assert_eq!(value["schema_version"], "strata.cli.output.v1");
    assert_eq!(value["kind"], "commands");
    assert_eq!(value["families"].as_array().expect("families").len(), 1);
    assert_eq!(value["families"][0]["id"], "vector");
    assert_eq!(value["families"][0]["command_count"], 19);
    assert_eq!(
        value["families"][0]["commands"][0]["id"],
        "vector.batch_delete"
    );
    assert_eq!(
        value["families"][0]["commands"][0]["path_display"],
        "vector batch-delete"
    );
    assert_eq!(value["families"][0]["commands"][18]["id"], "vector.upsert");
    let path_displays = value["families"][0]["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .map(|command| command["path_display"].as_str().expect("path display"))
        .collect::<Vec<_>>();
    assert!(
        path_displays.iter().all(|path| !path.contains('_')),
        "generated CLI paths should not leak command-id underscores"
    );
}

#[test]
fn explain_accepts_command_id_and_cli_path() {
    let by_id = run_success(&["explain", "kv.put", "--format", "json"]);
    let by_path = run_success(&["explain", "kv", "put", "--format", "json"]);

    let by_id_json: Value = serde_json::from_slice(&by_id.stdout).expect("id output JSON");
    let by_path_json: Value = serde_json::from_slice(&by_path.stdout).expect("path output JSON");

    assert_eq!(by_id_json["command"], by_path_json["command"]);
    assert_eq!(by_id_json["command"]["id"], "kv.put");
    assert_eq!(by_id_json["command"]["path_display"], "kv put");
    assert_eq!(
        by_id_json["command"]["response_model"],
        "MutationAck<KvWrite>"
    );
}

#[test]
fn explain_accepts_hyphenated_and_nested_cli_paths() {
    let batch_get = run_success(&["explain", "kv", "batch-get", "--format", "json"]);
    let delete_by_filter =
        run_success(&["explain", "vector", "delete-by-filter", "--format", "json"]);
    let collection_create = run_success(&[
        "explain",
        "vector",
        "collection",
        "create",
        "--format",
        "json",
    ]);

    let batch_get: Value = serde_json::from_slice(&batch_get.stdout).expect("batch get JSON");
    let delete_by_filter: Value =
        serde_json::from_slice(&delete_by_filter.stdout).expect("delete by filter JSON");
    let collection_create: Value =
        serde_json::from_slice(&collection_create.stdout).expect("collection create JSON");

    assert_eq!(batch_get["command"]["id"], "kv.batch_get");
    assert_eq!(batch_get["command"]["path_display"], "kv batch-get");
    assert_eq!(delete_by_filter["command"]["id"], "vector.delete_by_filter");
    assert_eq!(
        delete_by_filter["command"]["path_display"],
        "vector delete-by-filter"
    );
    assert_eq!(
        collection_create["command"]["id"],
        "vector.collection.create"
    );
    assert_eq!(
        collection_create["command"]["path_display"],
        "vector collection create"
    );
}

#[test]
fn explain_human_output_matches_fixture() {
    let output = run_success(&["explain", "kv.put"]);

    assert_eq!(stdout(&output), include_str!("fixtures/explain_kv_put.txt"));
    assert!(output.stderr.is_empty());
}

#[test]
fn explain_json_output_matches_fixture() {
    let output = run_success(&["explain", "kv.put", "--format", "json"]);

    assert_eq!(
        stdout(&output),
        include_str!("fixtures/explain_kv_put.json")
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn vector_explain_outputs_match_fixtures() {
    let human = run_success(&["explain", "vector.query"]);
    let json = run_success(&["explain", "vector", "query", "--format", "json"]);

    assert_eq!(
        stdout(&human),
        include_str!("fixtures/explain_vector_query.txt")
    );
    assert_eq!(
        stdout(&json),
        include_str!("fixtures/explain_vector_query.json")
    );
}

#[test]
fn unknown_command_returns_structured_error_with_suggestions() {
    let output = run_failure(&["explain", "nope", "--format", "json"]);
    let error = stderr(&output);
    let value: Value = serde_json::from_str(&error).expect("stderr should be JSON");

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(error, include_str!("fixtures/unknown_command_error.json"));
    assert_eq!(
        value["error"]["code"],
        "invalid_argument.cli.command_unknown"
    );
    assert_eq!(value["error"]["details"]["selector"], "nope");
    assert!(value["error"]["details"]["suggestions"]["command_ids"]
        .as_array()
        .expect("command suggestions")
        .iter()
        .any(|suggestion| suggestion == "kv.get"));
}

#[test]
fn unknown_family_and_duplicate_family_are_structured_errors() {
    let unknown = run_failure(&["commands", "--family", "missing", "--format", "json"]);
    let duplicate = run_failure(&[
        "commands", "--family", "kv", "--family", "vector", "--format", "json",
    ]);

    assert_eq!(
        stderr(&unknown),
        include_str!("fixtures/unknown_family_error.json")
    );
    let duplicate: Value =
        serde_json::from_slice(&duplicate.stderr).expect("duplicate error should be JSON");
    assert_eq!(duplicate["error"]["code"], "invalid_argument.cli.usage");
    assert_eq!(duplicate["error"]["message"], "duplicate --family");
}

#[test]
fn commands_and_explain_do_not_touch_user_home_or_database() {
    let home = TempDir::new().expect("temp home");
    let cwd = TempDir::new().expect("temp cwd");
    let output = strata_next()
        .args(["commands", "--family", "kv"])
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .current_dir(cwd.path())
        .output()
        .expect("strata-next command should run");

    assert!(output.status.success());
    assert!(!home.path().join(".strata").exists());
    assert!(!cwd.path().join(".strata").exists());
}

#[test]
fn help_adapter_renders_groups_family_and_command_from_metadata() {
    let catalog = CliCommandCatalog::embedded().expect("embedded catalog");
    let top_level = strata_cli_next::render_top_level_help(&catalog);
    let kv_family = catalog.family("kv").expect("kv family");
    let kv_help = strata_cli_next::render_family_help(&catalog, kv_family);
    let kv_put = catalog.command("kv.put").expect("kv.put");
    let command_help = strata_cli_next::render_command_help(kv_put);

    assert!(top_level.contains("strata explain kv.put"));
    assert!(top_level.contains("strata --db ./my-db kv put user Claude"));
    assert!(top_level.contains("kv          Execute KV commands against a database."));
    assert!(top_level.contains("kv       13 commands"));
    assert!(kv_help.contains("kv put"));
    assert!(kv_help.contains("Store or replace a KV value by key."));
    assert!(command_help.contains("MutationAck<KvWrite>"));
    assert!(command_help.contains("/docs/kv/put"));
}

#[test]
fn executable_kv_help_uses_runtime_cli_shape() {
    let kv_help = run_success(&["kv", "--help"]);
    let kv_put_help = run_success(&["kv", "put", "--help"]);

    let kv_help = stdout(&kv_help);
    assert!(kv_help.contains("Usage: strata --db <path> kv <operation> [options]"));
    assert!(kv_help.contains("--                    Treat following tokens as KV operands."));
    assert!(kv_help.contains("strata --db ./my-db kv put flag -- --json"));

    let kv_put_help = stdout(&kv_put_help);
    assert!(kv_put_help.contains("kv put"));
    assert!(kv_put_help.contains("Put KV value"));
    assert!(kv_put_help.contains("MutationAck<KvWrite>"));
}

#[test]
fn runtime_crate_does_not_depend_on_authoring_sources_or_old_stack() {
    let manifest = include_str!("../Cargo.toml");
    for forbidden in [
        "serde_yaml",
        "frontmatter",
        "strata-executor =",
        "strata-engine =",
        "strata-storage =",
    ] {
        assert!(
            !manifest.contains(forbidden),
            "cli-next manifest must not contain {forbidden}"
        );
    }

    let lib = include_str!("../src/lib.rs");
    for forbidden in [".yaml", "commands/", "prose/", "frontmatter", "idl_tooling"] {
        assert!(
            !lib.contains(forbidden),
            "cli-next runtime source must not contain {forbidden}"
        );
    }
}
