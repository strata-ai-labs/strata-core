//! KV/vector IDL overlay conformance tests.

#![cfg(feature = "idl-tooling")]

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use strata_executor_next::idl_tooling::{
    check, default_repo_root, resolve_default_index, to_generated_json,
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
                .starts_with("https://strata.dev/docs/errors/registry#")),
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

    assert_eq!(model("kv.get"), "Maybe<Bytes>");
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
fn slice_one_does_not_add_downstream_generators() {
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
            "Slice 1 must not add downstream generator code for {forbidden}"
        );
    }
}

#[test]
fn slice_one_b_packaging_is_executor_owned() {
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
    assert!(executor_toml.contains("idl-tooling = [\"dep:serde_yaml\"]"));
    assert!(executor_toml.contains("name = \"strata-idl\""));
    assert!(executor_toml.contains("required-features = [\"idl-tooling\"]"));
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
