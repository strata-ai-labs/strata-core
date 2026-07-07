//! KV/vector IDL overlay conformance tests.

#![cfg(feature = "idl-tooling")]

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use strata_executor_next::idl_tooling::{
    check, check_cli, default_repo_root, resolve_cli_index, resolve_default_cli_index,
    resolve_default_index, to_generated_cli_json, to_generated_json,
};

const REQUIRED_KV: &[&str] = &[
    "kv.put",
    "kv.get",
    "kv.delete",
    "kv.list",
    "kv.scan",
    "kv.batch_put",
    "kv.batch_get",
    "kv.batch_delete",
    "kv.batch_exists",
    "kv.exists",
    "kv.history",
    "kv.count",
    "kv.sample",
];

const REQUIRED_VECTOR: &[&str] = &[
    "vector.collection.create",
    "vector.collection.delete",
    "vector.collection.list",
    "vector.collection.stats",
    "vector.count",
    "vector.upsert",
    "vector.get",
    "vector.history",
    "vector.exists",
    "vector.keys",
    "vector.metadata.update",
    "vector.delete",
    "vector.delete_by_filter",
    "vector.delete_all",
    "vector.query",
    "vector.index.query",
    "vector.batch_upsert",
    "vector.batch_get",
    "vector.batch_delete",
];

#[test]
fn kv_and_vector_overlay_has_required_command_coverage() {
    let index = resolve_default_index().expect("IDL resolves");
    let ids: BTreeSet<&str> = index
        .commands
        .iter()
        .map(|command| command.id.as_str())
        .collect();

    for id in REQUIRED_KV.iter().chain(REQUIRED_VECTOR.iter()) {
        assert!(ids.contains(id), "missing required command `{id}`");
    }
    assert_eq!(ids.len(), REQUIRED_KV.len() + REQUIRED_VECTOR.len());
}

#[test]
fn generated_command_index_is_fresh_and_deterministic() {
    let root = default_repo_root();
    check(&root).expect("generated IDL is fresh");

    let first = resolve_default_index().expect("first resolve succeeds");
    let second = resolve_default_index().expect("second resolve succeeds");
    assert_eq!(first, second);

    let generated = to_generated_json(&first).expect("index serializes");
    let path = root
        .join("crates/executor-next/idl/v1/generated")
        .join("command-index.json");
    let checked_in = fs::read_to_string(path).expect("generated file is readable");
    assert_eq!(generated, checked_in);
}

#[test]
fn resolved_commands_are_explain_ready() {
    let index = resolve_default_index().expect("IDL resolves");
    let mut sorted_ids = Vec::new();
    for command in &index.commands {
        sorted_ids.push(command.id.as_str());
        assert_eq!(command.generated_family_and_op_id(), command.id.as_str());
        assert!(!command.title.trim().is_empty());
        assert!(!command.summary.trim().is_empty());
        assert!(!command.description.trim().is_empty());
        assert!(command.docs.starts_with("/docs/"));
        assert!(!command.cli.path.is_empty());
        assert!(command.mcp.name.starts_with("strata_"));
        assert!(command.input.starts_with("Command::"));
        assert!(command.output.starts_with("Output::"));
        assert!(!command.outputs.is_empty());
        assert!(
            command
                .outputs
                .iter()
                .any(|output| output == &command.output),
            "primary output must be listed in outputs for `{}`",
            command.id
        );
        assert!(
            matches!(command.wire_status.as_str(), "stable" | "transitional"),
            "unexpected wire status for `{}`",
            command.id
        );
        assert!(!command.response_model.trim().is_empty());
        assert!(!command.commit.trim().is_empty());
        assert!(!command.pagination.trim().is_empty());
        assert!(!command.batch.trim().is_empty());
        assert!(has_extension(&command.source.command, "yaml"));
        assert!(
            command
                .source
                .command
                .starts_with("crates/executor-next/idl/v1/commands/"),
            "command source should be executor-owned for `{}`",
            command.id
        );
        assert!(has_extension(&command.source.prose, "md"));
        assert!(command.fixtures.request.starts_with("requests/v1/"));
        assert!(command.fixtures.response.starts_with("responses/v1/"));
        assert!(
            command.errors.iter().all(|error| error
                .docs
                .starts_with("https://stratadb.org/e/")),
            "all command errors should include docs URLs"
        );
    }

    let mut expected = sorted_ids.clone();
    expected.sort_unstable();
    assert_eq!(sorted_ids, expected, "commands must be sorted by id");
}

#[test]
fn kv_vector_concepts_resolve_to_expected_shared_models() {
    let index = resolve_default_index().expect("IDL resolves");
    let model = |id: &str| {
        index
            .commands
            .iter()
            .find(|command| command.id == id)
            .map(|command| command.response_model.as_str())
            .expect("command exists")
    };

    assert_eq!(model("kv.get"), "Maybe<VersionedValue>");
    assert_eq!(model("kv.list"), "Page<Bytes, Bytes>");
    assert_eq!(model("kv.scan"), "Page<ScanItem, Bytes>");
    assert_eq!(model("kv.batch_get"), "BatchResult<Maybe<Bytes>>");
    assert_eq!(
        model("vector.collection.create"),
        "MutationAck<VectorCollectionCreate>"
    );
    assert_eq!(
        model("vector.collection.stats"),
        "StatusResponse<VectorCollectionInfo>"
    );
    assert_eq!(
        model("vector.collection.list"),
        "Page<VectorCollectionInfo, String>"
    );
    assert_eq!(model("vector.keys"), "Page<String, String>");
    assert_eq!(model("vector.query"), "SearchResult<VectorMatch>");
    assert_eq!(
        model("vector.index.query"),
        "SearchResult<VectorMatch> + IndexDiagnostics"
    );
}

#[test]
fn kv_list_declares_both_current_wire_outputs() {
    let index = resolve_default_index().expect("IDL resolves");
    let command = index
        .commands
        .iter()
        .find(|command| command.id == "kv.list")
        .expect("kv.list exists");

    assert_eq!(command.output, "Output::KeysPage");
    assert_eq!(
        command.outputs,
        vec!["Output::Keys".to_owned(), "Output::KeysPage".to_owned()]
    );
    assert_eq!(command.fixtures.response, "responses/v1/kv/list_page.json");
    assert_eq!(
        command.fixtures.responses,
        vec!["responses/v1/kv/list_keys.json".to_owned()]
    );
}

#[test]
fn transitional_vector_collection_wire_shapes_are_explicit() {
    let index = resolve_default_index().expect("IDL resolves");
    let transitional: BTreeSet<&str> = index
        .commands
        .iter()
        .filter(|command| command.wire_status == "transitional")
        .map(|command| command.id.as_str())
        .collect();

    assert_eq!(
        transitional,
        BTreeSet::from([
            "vector.collection.create",
            "vector.collection.delete",
            "vector.collection.stats"
        ])
    );
}

#[test]
fn idl_tooling_does_not_add_downstream_generators() {
    let root = default_repo_root();
    let mut source = String::new();
    for path in [
        root.join("crates/executor-next/src/idl_tooling.rs"),
        root.join("crates/executor-next/src/bin/strata-idl/main.rs"),
    ] {
        source.push_str(&fs::read_to_string(path).expect("IDL tooling source is readable"));
    }

    for forbidden in ["OpenAPI", "TypeScript", "Python SDK", "MCP server"] {
        assert!(
            !source.contains(forbidden),
            "IDL tooling must not add downstream generator code for {forbidden}"
        );
    }
}

#[test]
fn idl_packaging_is_executor_owned() {
    let root = default_repo_root();
    assert!(
        !root.join("crates/idl-next").exists(),
        "standalone IDL crate should be removed"
    );
    assert!(
        root.join("crates/executor-next/idl/v1/manifest.yaml")
            .is_file(),
        "authored IDL should live under executor-next"
    );
    assert!(
        root.join("crates/executor-next/idl/v1/generated/command-index.json")
            .is_file(),
        "generated IDL should live under executor-next"
    );
    assert!(
        root.join("crates/executor-next/src/bin/strata-idl/main.rs")
            .is_file(),
        "executor-next should own the strata-idl dev binary"
    );

    let workspace_toml =
        fs::read_to_string(root.join("Cargo.toml")).expect("workspace Cargo.toml reads");
    assert!(
        !workspace_toml.contains("\"crates/idl-next\""),
        "workspace should not list the old standalone IDL crate"
    );

    let executor_toml = fs::read_to_string(root.join("crates/executor-next/Cargo.toml"))
        .expect("executor Cargo.toml reads");
    assert!(executor_toml.contains("idl-tooling = ["));
    assert!(executor_toml.contains("\"dep:serde_yaml\""));
    assert!(executor_toml.contains("\"dep:sha2\""));
    assert!(executor_toml.contains("name = \"strata-idl\""));
    assert!(executor_toml.contains("required-features = [\"idl-tooling\"]"));
}

#[test]
fn generated_cli_command_index_is_fresh_and_deterministic() {
    let root = default_repo_root();
    check_cli(&root).expect("generated CLI IDL is fresh");

    let first = resolve_default_cli_index().expect("first CLI resolve succeeds");
    let second = resolve_default_cli_index().expect("second CLI resolve succeeds");
    assert_eq!(first, second);
    assert!(first.generated);
    assert_eq!(first.schema_version, "strata.cli.v1");
    assert_eq!(first.generator_version, "strata-executor-cli-idl.1");
    assert_eq!(first.source.schema_version, "strata.idl.v1");
    assert_eq!(first.source.generator_version, "strata-executor-idl.1");
    assert_eq!(first.source.checksum_sha256.len(), 64);
    assert!(first
        .source
        .checksum_sha256
        .chars()
        .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()));
    assert_eq!(
        first.command_count,
        REQUIRED_KV.len() + REQUIRED_VECTOR.len()
    );
    assert_eq!(first.command_count, first.commands.len());

    let generated = to_generated_cli_json(&first).expect("CLI index serializes");
    let path = root
        .join("crates/executor-next/idl/v1/generated")
        .join("cli-command-index.json");
    let checked_in = fs::read_to_string(path).expect("generated CLI file is readable");
    assert_eq!(generated, checked_in);
}

#[test]
fn cli_command_index_has_required_coverage_and_lookup_tables() {
    let index = resolve_default_cli_index().expect("CLI IDL resolves");
    let ids: BTreeSet<&str> = index
        .commands
        .iter()
        .map(|command| command.id.as_str())
        .collect();
    for id in REQUIRED_KV.iter().chain(REQUIRED_VECTOR.iter()) {
        assert!(ids.contains(id), "missing required CLI command `{id}`");
    }

    let mut sorted_paths = index
        .commands
        .iter()
        .map(|command| command.path.clone())
        .collect::<Vec<_>>();
    let actual_paths = sorted_paths.clone();
    sorted_paths.sort();
    assert_eq!(actual_paths, sorted_paths, "commands must sort by CLI path");

    for (offset, command) in index.commands.iter().enumerate() {
        assert_eq!(command.path_display, command.path.join(" "));
        assert!(
            command.path.iter().all(|segment| !segment.contains('_')),
            "generated CLI path for `{}` should not leak command-id underscores",
            command.id
        );
        assert_eq!(index.lookup.by_id.get(&command.id), Some(&offset));
        assert_eq!(
            index.lookup.by_path.get(&command.path_display),
            Some(&command.id)
        );
        assert!(!command.title.trim().is_empty());
        assert!(!command.summary.trim().is_empty());
        assert!(!command.description.trim().is_empty());
        assert!(command.docs.starts_with("/docs/"));
        assert!(command.input.starts_with("Command::"));
        assert!(!command.outputs.is_empty());
        assert!(command
            .outputs
            .iter()
            .all(|output| output.starts_with("Output::")));
        assert!(!command.errors.is_empty());
    }

    let kv = index
        .families
        .iter()
        .find(|family| family.id == "kv")
        .expect("KV family exists");
    assert_eq!(kv.command_count, REQUIRED_KV.len());
    let vector = index
        .families
        .iter()
        .find(|family| family.id == "vector")
        .expect("vector family exists");
    assert_eq!(vector.command_count, REQUIRED_VECTOR.len());

    let path_for = |id: &str| {
        index
            .commands
            .iter()
            .find(|command| command.id == id)
            .map(|command| command.path_display.as_str())
            .expect("command exists")
    };
    assert_eq!(path_for("kv.batch_get"), "kv batch-get");
    assert_eq!(
        path_for("vector.delete_by_filter"),
        "vector delete-by-filter"
    );
    assert_eq!(
        path_for("vector.collection.create"),
        "vector collection create"
    );
}

#[test]
fn cli_generation_reads_resolved_index_not_authored_yaml_or_prose() {
    let root = default_repo_root();
    let temp = tempfile::tempdir().expect("tempdir creates");
    let generated_dir = temp.path().join("crates/executor-next/idl/v1/generated");
    fs::create_dir_all(&generated_dir).expect("generated dir creates");
    fs::copy(
        root.join("crates/executor-next/idl/v1/generated/command-index.json"),
        generated_dir.join("command-index.json"),
    )
    .expect("command index copies");

    let index =
        resolve_cli_index(temp.path()).expect("CLI index resolves from generated JSON only");
    assert_eq!(
        index.command_count,
        REQUIRED_KV.len() + REQUIRED_VECTOR.len()
    );
    assert_eq!(
        index.source.path,
        "crates/executor-next/idl/v1/generated/command-index.json"
    );
    assert!(
        !temp
            .path()
            .join("crates/executor-next/idl/v1/commands/kv.yaml")
            .exists(),
        "test fixture intentionally excludes authored YAML"
    );
    assert!(
        !temp
            .path()
            .join("crates/executor-next/idl/v1/prose/commands/kv.put.md")
            .exists(),
        "test fixture intentionally excludes authored prose"
    );
}

#[test]
fn strata_idl_generates_cli_artifacts_without_user_explain() {
    let root = default_repo_root();
    let source = fs::read_to_string(root.join("crates/executor-next/src/bin/strata-idl/main.rs"))
        .expect("strata-idl source reads");

    assert!(source.contains("\"generate-cli\""));
    assert!(source.contains("\"check-cli\""));
    assert!(
        !source.contains("\"explain\""),
        "strata-idl must not introduce explain; user explain belongs to strata"
    );
}

trait ResolvedCommandExt {
    fn generated_family_and_op_id(&self) -> String;
}

impl ResolvedCommandExt for strata_executor_next::idl_tooling::ResolvedCommand {
    fn generated_family_and_op_id(&self) -> String {
        format!("{}.{}", self.family, self.op)
    }
}

fn has_extension(path: &str, extension: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|actual| actual.eq_ignore_ascii_case(extension))
}
