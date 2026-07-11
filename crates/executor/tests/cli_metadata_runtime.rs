//! Runtime CLI metadata conformance tests.

use std::fs;

use serde_json::Value;
use strata_executor::cli_metadata::{
    CliCommandCatalog, CliMetadataError, EMBEDDED_CLI_COMMAND_INDEX_JSON,
};

#[test]
fn embedded_cli_metadata_loads_without_generator_feature() {
    let catalog = CliCommandCatalog::embedded().expect("embedded CLI metadata loads");

    assert_eq!(catalog.index().schema_version, "strata.cli.v1");
    assert_eq!(
        catalog.index().generator_version,
        "strata-executor-cli-idl.1"
    );
    assert_eq!(catalog.index().command_count, 56);
    assert_eq!(catalog.commands().len(), 56);
    assert_eq!(catalog.families().len(), 4);
}

#[test]
fn command_lookup_supports_ids_and_cli_paths() {
    let catalog = CliCommandCatalog::embedded().expect("embedded CLI metadata loads");

    let kv_put = catalog.command("kv.put").expect("kv.put resolves by id");
    assert_eq!(kv_put.path_display, "kv put");
    assert_eq!(kv_put.access, "write");
    assert_eq!(kv_put.commit, "commits_on_success");

    assert_eq!(
        catalog
            .command("kv put")
            .expect("kv put resolves by path")
            .id,
        "kv.put"
    );
    assert_eq!(
        catalog
            .command("kv batch-get")
            .expect("hyphenated batch path resolves")
            .id,
        "kv.batch_get"
    );
    assert_eq!(
        catalog
            .command_by_path(["vector", "collection", "create"])
            .expect("nested vector path resolves")
            .id,
        "vector.collection.create"
    );
    assert_eq!(
        catalog
            .command_by_path(["vector", "delete-by-filter"])
            .expect("hyphenated vector path resolves")
            .id,
        "vector.delete_by_filter"
    );
    assert_eq!(
        catalog
            .command_by_path_display("vector query")
            .expect("vector query resolves")
            .id,
        "vector.query"
    );
    assert!(catalog.command("kv missing").is_none());
}

#[test]
fn command_listing_is_grouped_and_sorted() {
    let catalog = CliCommandCatalog::embedded().expect("embedded CLI metadata loads");

    let families = catalog.families();
    assert_eq!(families[0].id, "event");
    assert_eq!(families[1].id, "json");
    assert_eq!(families[2].id, "kv");
    assert_eq!(families[3].id, "vector");

    let kv_commands = catalog
        .commands_for_family("kv")
        .expect("KV family commands exist");
    assert_eq!(kv_commands.len(), 13);
    assert_eq!(
        kv_commands
            .iter()
            .map(|command| command.id.as_str())
            .collect::<Vec<_>>(),
        families[2]
            .commands
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );

    let mut sorted_paths = catalog
        .commands()
        .iter()
        .map(|command| command.path.clone())
        .collect::<Vec<_>>();
    let actual_paths = sorted_paths.clone();
    sorted_paths.sort();
    assert_eq!(actual_paths, sorted_paths);
    assert!(catalog.commands_for_family("admin").is_none());
}

#[test]
fn unknown_command_suggestions_are_deterministic() {
    let catalog = CliCommandCatalog::embedded().expect("embedded CLI metadata loads");

    let suggestions = catalog.suggestions("kv ptu", 3);
    assert!(suggestions.command_ids.contains(&"kv.put".to_owned()));
    assert!(suggestions.paths.contains(&"kv put".to_owned()));
    assert_eq!(suggestions.command_ids.len(), 3);
    assert_eq!(suggestions.paths.len(), 3);
    assert_eq!(suggestions, catalog.suggestions("kv ptu", 3));

    let empty = catalog.suggestions("kv ptu", 0);
    assert!(empty.command_ids.is_empty());
    assert!(empty.paths.is_empty());
}

#[test]
fn runtime_validation_rejects_malformed_metadata() {
    let mut value: Value =
        serde_json::from_str(EMBEDDED_CLI_COMMAND_INDEX_JSON).expect("metadata parses as JSON");
    value["command_count"] = Value::from(999);
    assert_parse_invalid(&value, "command_count");

    let mut value: Value =
        serde_json::from_str(EMBEDDED_CLI_COMMAND_INDEX_JSON).expect("metadata parses as JSON");
    value["schema_version"] = Value::from("wrong.schema");
    assert_parse_invalid(&value, "unsupported CLI command index schema");
}

#[test]
fn runtime_validation_rejects_bad_source_versions() {
    let mut value: Value =
        serde_json::from_str(EMBEDDED_CLI_COMMAND_INDEX_JSON).expect("metadata parses as JSON");
    value["source"]["schema_version"] = Value::from("wrong.source.schema");
    assert_parse_invalid(&value, "unsupported source command index schema");

    let mut value: Value =
        serde_json::from_str(EMBEDDED_CLI_COMMAND_INDEX_JSON).expect("metadata parses as JSON");
    value["source"]["generator_version"] = Value::from("wrong.source.generator");
    assert_parse_invalid(&value, "unsupported source command index generator");
}

#[test]
fn runtime_validation_rejects_command_identity_mismatch() {
    let mut value: Value =
        serde_json::from_str(EMBEDDED_CLI_COMMAND_INDEX_JSON).expect("metadata parses as JSON");
    value["commands"][0]["family"] = Value::from("vector");
    assert_parse_invalid(&value, "identity does not match family/op");
}

#[test]
fn runtime_validation_rejects_bad_family_group_membership() {
    let mut value: Value =
        serde_json::from_str(EMBEDDED_CLI_COMMAND_INDEX_JSON).expect("metadata parses as JSON");
    value["families"][0]["commands"][0] = Value::from("vector.query");
    assert_parse_invalid(&value, "owned by family");

    let mut value: Value =
        serde_json::from_str(EMBEDDED_CLI_COMMAND_INDEX_JSON).expect("metadata parses as JSON");
    value["families"][0]["commands"]
        .as_array_mut()
        .expect("family commands is array")
        .pop()
        .expect("family has command to remove");
    value["families"][0]["command_count"] = Value::from(
        value["families"][0]["commands"]
            .as_array()
            .expect("family commands is array")
            .len(),
    );
    assert_parse_invalid(&value, "family groups do not cover every command");
}

#[test]
fn runtime_metadata_source_does_not_use_authoring_inputs() {
    let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = fs::read_to_string(crate_root.join("src/cli_metadata.rs")).expect("source reads");
    for forbidden in [
        "serde_yaml",
        "frontmatter",
        "commands/*.yaml",
        "prose/**/*.md",
        "idl_tooling",
    ] {
        assert!(
            !source.contains(forbidden),
            "runtime metadata source must not reference `{forbidden}`"
        );
    }

    let manifest = fs::read_to_string(crate_root.join("Cargo.toml")).expect("manifest reads");
    let default_feature_section = manifest
        .split("[features]")
        .nth(1)
        .expect("features section exists")
        .split("[dependencies]")
        .next()
        .expect("dependencies section follows features");
    assert!(!default_feature_section.contains("idl-tooling = [\"default\""));
}

fn assert_invalid(error: CliMetadataError, expected: &str) {
    match error {
        CliMetadataError::Invalid(message) => assert!(
            message.contains(expected),
            "expected `{message}` to contain `{expected}`"
        ),
        CliMetadataError::Json(error) => {
            panic!("expected validation error, got JSON error {error}")
        }
    }
}

fn assert_parse_invalid(value: &Value, expected: &str) {
    let json = serde_json::to_string(&value).expect("metadata serializes");
    let error = CliCommandCatalog::parse(&json).expect_err("invalid metadata fails");
    assert_invalid(error, expected);
}
