//! Resolver for Strata's thin V1 IDL overlay.

#![deny(unsafe_code)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

use crate::{public_error_code_entries, Command, Output};

const IDL_DIR: &str = "crates/executor-next/idl/v1";
const FIXTURE_ROOT: &str = "crates/executor-next/tests/fixtures";
const COMMAND_INDEX_FILE: &str = "command-index.json";
const CLI_COMMAND_INDEX_FILE: &str = "cli-command-index.json";
const SUPPORTED_COMMAND_SCHEMA_VERSION: &str = "strata.idl.v1";
const SUPPORTED_COMMAND_GENERATOR_VERSION: &str = "strata-executor-idl.1";
const CLI_SCHEMA_VERSION: &str = "strata.cli.v1";
const CLI_GENERATOR_VERSION: &str = "strata-executor-cli-idl.1";
const COMMAND_SOURCE_FIELDS: &[&str] = &[
    "id",
    "kind",
    "title",
    "input",
    "output",
    "outputs",
    "result",
    "prose",
    "wire_status",
    "docs",
    "cli_path",
    "mcp_name",
    "feature",
    "access",
    "commit",
    "pagination",
    "batch",
    "response_model",
    "snippets",
    "errors+",
    "errors-",
    "fixtures",
];

/// IDL resolver result type.
pub type Result<T> = std::result::Result<T, IdlError>;

/// IDL resolver error.
#[derive(Debug, Error)]
pub enum IdlError {
    /// A required file could not be read.
    #[error("failed to read {path}: {source}")]
    Read {
        /// File path.
        path: PathBuf,
        /// I/O error.
        source: std::io::Error,
    },
    /// A required file could not be written.
    #[error("failed to write {path}: {source}")]
    Write {
        /// File path.
        path: PathBuf,
        /// I/O error.
        source: std::io::Error,
    },
    /// A YAML source file is invalid.
    #[error("failed to parse YAML {path}: {source}")]
    Yaml {
        /// File path.
        path: PathBuf,
        /// YAML parse error.
        source: serde_yaml::Error,
    },
    /// JSON could not be serialized or deserialized.
    #[error("failed to process JSON {path}: {source}")]
    Json {
        /// File path.
        path: PathBuf,
        /// JSON error.
        source: serde_json::Error,
    },
    /// Authored IDL is invalid.
    #[error("{0}")]
    Invalid(String),
    /// Generated output is stale.
    #[error(
        "{path} is stale; run `cargo run -p strata-executor-next --features idl-tooling --bin strata-idl -- generate`"
    )]
    Stale {
        /// Generated file path.
        path: PathBuf,
    },
    /// Generated CLI output is stale.
    #[error(
        "{path} is stale; run `cargo run -p strata-executor-next --features idl-tooling --bin strata-idl -- generate-cli`"
    )]
    CliStale {
        /// Generated file path.
        path: PathBuf,
    },
}

/// Resolved command index.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandIndex {
    /// Generated-file marker.
    pub generated: bool,
    /// Source schema version.
    pub schema_version: String,
    /// Generator version.
    pub generator_version: String,
    /// Resolved commands sorted by stable command id.
    pub commands: Vec<ResolvedCommand>,
}

/// One resolved command entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedCommand {
    /// Stable command id.
    pub id: String,
    /// Command family.
    pub family: String,
    /// Operation id within the family.
    pub op: String,
    /// Operation kind id.
    pub kind: String,
    /// User-facing title.
    pub title: String,
    /// Short summary from prose frontmatter.
    pub summary: String,
    /// Long Markdown description.
    pub description: String,
    /// Documentation path.
    pub docs: String,
    /// Future CLI routing facts.
    pub cli: CliInfo,
    /// Future MCP metadata.
    pub mcp: McpInfo,
    /// Product feature.
    pub feature: String,
    /// Access mode.
    pub access: String,
    /// Executor command variant reference.
    pub input: String,
    /// Executor output variant reference.
    pub output: String,
    /// All executor output variants this command can produce on the current wire.
    pub outputs: Vec<String>,
    /// Stability marker for the current executor wire shape.
    pub wire_status: String,
    /// Shared response concept.
    pub response_model: String,
    /// Commit behavior.
    pub commit: String,
    /// Pagination behavior.
    pub pagination: String,
    /// Batch behavior.
    pub batch: String,
    /// Registered public errors that can be surfaced for this command.
    pub errors: Vec<ErrorRef>,
    /// Checked-in fixture references.
    pub fixtures: FixtureRefs,
    /// Authored source locations.
    pub source: SourceInfo,
}

/// Future CLI routing facts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CliInfo {
    /// CLI path segments.
    pub path: Vec<String>,
}

/// Future MCP metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct McpInfo {
    /// MCP tool name.
    pub name: String,
    /// MCP tool description.
    pub description: String,
}

/// Registered public error reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ErrorRef {
    /// Public error code.
    pub code: String,
    /// Stable docs URL.
    pub docs: String,
}

/// Request/response fixture references.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureRefs {
    /// Request fixture path relative to `crates/executor-next/tests/fixtures`.
    pub request: String,
    /// Primary response fixture path relative to `crates/executor-next/tests/fixtures`.
    pub response: String,
    /// Alternate response fixtures for commands with multiple current wire outputs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub responses: Vec<String>,
}

/// Authored source locations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Command YAML path relative to the repo root.
    pub command: String,
    /// Prose Markdown path relative to `crates/executor-next/idl/v1/prose`.
    pub prose: String,
}

/// Generated CLI command metadata index.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CliCommandIndex {
    /// Generated-file marker.
    pub generated: bool,
    /// CLI artifact schema version.
    pub schema_version: String,
    /// CLI artifact generator version.
    pub generator_version: String,
    /// Source command-index facts.
    pub source: CliIndexSourceInfo,
    /// Number of commands included.
    pub command_count: usize,
    /// Command families sorted by family id.
    pub families: Vec<CliFamilyGroup>,
    /// Commands sorted by CLI path.
    pub commands: Vec<CliCommandEntry>,
    /// Lookup tables for runtime command resolution.
    pub lookup: CliLookupTables,
}

/// Source command-index facts for a generated CLI index.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CliIndexSourceInfo {
    /// Source command-index path relative to the repo root.
    pub path: String,
    /// SHA-256 checksum of the source command-index JSON bytes.
    pub checksum_sha256: String,
    /// Source command-index schema version.
    pub schema_version: String,
    /// Source command-index generator version.
    pub generator_version: String,
}

/// CLI command family group.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CliFamilyGroup {
    /// Family id.
    pub id: String,
    /// Number of commands in the family.
    pub command_count: usize,
    /// Command ids in CLI listing order.
    pub commands: Vec<String>,
}

/// CLI command entry optimized for command discovery and explanation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CliCommandEntry {
    /// Stable command id.
    pub id: String,
    /// CLI path segments.
    pub path: Vec<String>,
    /// Display form of the CLI path.
    pub path_display: String,
    /// Command family.
    pub family: String,
    /// Operation id within the family.
    pub op: String,
    /// Operation kind id.
    pub kind: String,
    /// User-facing title.
    pub title: String,
    /// One-line summary.
    pub summary: String,
    /// Long Markdown description.
    pub description: String,
    /// Documentation path.
    pub docs: String,
    /// Product feature.
    pub feature: String,
    /// Access mode.
    pub access: String,
    /// Commit behavior.
    pub commit: String,
    /// Pagination behavior.
    pub pagination: String,
    /// Batch behavior.
    pub batch: String,
    /// Executor command variant reference.
    pub input: String,
    /// All executor output variants this command can produce on the current wire.
    pub outputs: Vec<String>,
    /// Shared response concept.
    pub response_model: String,
    /// Registered public errors that can be surfaced for this command.
    pub errors: Vec<ErrorRef>,
    /// Checked-in fixture references.
    pub fixtures: FixtureRefs,
    /// Stability marker for the current executor wire shape.
    pub wire_status: String,
}

/// CLI runtime lookup tables.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CliLookupTables {
    /// Command id to entry offset.
    pub by_id: BTreeMap<String, usize>,
    /// CLI path display string to command id.
    pub by_path: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestSource {
    schema_version: String,
    generator_version: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerFields {
    #[serde(default)]
    docs: Option<String>,
    #[serde(default)]
    cli_path: Option<Vec<String>>,
    #[serde(default)]
    mcp_name: Option<String>,
    #[serde(default)]
    feature: Option<String>,
    #[serde(default)]
    access: Option<String>,
    #[serde(default)]
    commit: Option<String>,
    #[serde(default)]
    pagination: Option<String>,
    #[serde(default)]
    batch: Option<String>,
    #[serde(default)]
    response_model: Option<String>,
    #[serde(default)]
    errors: Vec<String>,
    #[serde(default)]
    snippets: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DefaultsSource {
    #[serde(flatten)]
    fields: LayerFields,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FamiliesSource {
    families: Vec<NamedLayerSource>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct KindsSource {
    kinds: Vec<NamedLayerSource>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NamedLayerSource {
    id: String,
    #[serde(flatten)]
    fields: LayerFields,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ErrorsSource {
    errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DtoInventorySource {
    response_models: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandsFileSource {
    commands: Vec<CommandSource>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandSource {
    id: String,
    kind: String,
    title: String,
    input: String,
    output: String,
    #[serde(default)]
    outputs: Vec<String>,
    result: String,
    prose: String,
    #[serde(default)]
    wire_status: Option<String>,
    #[serde(default)]
    docs: Option<String>,
    #[serde(default)]
    cli_path: Option<Vec<String>>,
    #[serde(default)]
    mcp_name: Option<String>,
    #[serde(default)]
    feature: Option<String>,
    #[serde(default)]
    access: Option<String>,
    #[serde(default)]
    commit: Option<String>,
    #[serde(default)]
    pagination: Option<String>,
    #[serde(default)]
    batch: Option<String>,
    #[serde(default)]
    response_model: Option<String>,
    #[serde(default)]
    snippets: Vec<String>,
    #[serde(default, rename = "errors+")]
    errors_add: Vec<String>,
    #[serde(default, rename = "errors-")]
    errors_remove: Vec<String>,
    fixtures: FixtureRefs,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProseFrontmatter {
    summary: String,
    #[serde(default)]
    mcp_description: Option<String>,
}

struct Prose {
    summary: String,
    mcp_description: Option<String>,
    body: String,
}

#[derive(Clone, Debug, Default)]
struct ResolvedLayer {
    docs: Option<String>,
    cli_path: Option<Vec<String>>,
    mcp_name: Option<String>,
    feature: Option<String>,
    access: Option<String>,
    commit: Option<String>,
    pagination: Option<String>,
    batch: Option<String>,
    response_model: Option<String>,
    errors: Vec<String>,
    snippets: Vec<String>,
}

impl ResolvedLayer {
    fn apply(&mut self, fields: &LayerFields) {
        if fields.docs.is_some() {
            self.docs.clone_from(&fields.docs);
        }
        if fields.cli_path.is_some() {
            self.cli_path.clone_from(&fields.cli_path);
        }
        if fields.mcp_name.is_some() {
            self.mcp_name.clone_from(&fields.mcp_name);
        }
        if fields.feature.is_some() {
            self.feature.clone_from(&fields.feature);
        }
        if fields.access.is_some() {
            self.access.clone_from(&fields.access);
        }
        if fields.commit.is_some() {
            self.commit.clone_from(&fields.commit);
        }
        if fields.pagination.is_some() {
            self.pagination.clone_from(&fields.pagination);
        }
        if fields.batch.is_some() {
            self.batch.clone_from(&fields.batch);
        }
        if fields.response_model.is_some() {
            self.response_model.clone_from(&fields.response_model);
        }
        append_unique(&mut self.errors, &fields.errors);
        append_unique(&mut self.snippets, &fields.snippets);
    }

    fn apply_command(&mut self, command: &CommandSource) {
        if command.docs.is_some() {
            self.docs.clone_from(&command.docs);
        }
        if command.cli_path.is_some() {
            self.cli_path.clone_from(&command.cli_path);
        }
        if command.mcp_name.is_some() {
            self.mcp_name.clone_from(&command.mcp_name);
        }
        if command.feature.is_some() {
            self.feature.clone_from(&command.feature);
        }
        if command.access.is_some() {
            self.access.clone_from(&command.access);
        }
        if command.commit.is_some() {
            self.commit.clone_from(&command.commit);
        }
        if command.pagination.is_some() {
            self.pagination.clone_from(&command.pagination);
        }
        if command.batch.is_some() {
            self.batch.clone_from(&command.batch);
        }
        if command.response_model.is_some() {
            self.response_model.clone_from(&command.response_model);
        }
        append_unique(&mut self.errors, &command.errors_add);
        append_unique(&mut self.snippets, &command.snippets);
        if !command.errors_remove.is_empty() {
            self.errors
                .retain(|code| !command.errors_remove.iter().any(|removed| removed == code));
        }
    }
}

/// Returns the repository root inferred from this crate's manifest directory.
#[must_use]
pub fn default_repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("executor-next lives under crates/")
        .to_path_buf()
}

/// Resolves the default IDL source tree.
pub fn resolve_default_index() -> Result<CommandIndex> {
    resolve_index(&default_repo_root())
}

/// Resolves the IDL source tree under `repo_root`.
pub fn resolve_index(repo_root: &Path) -> Result<CommandIndex> {
    let idl_root = repo_root.join(IDL_DIR);
    let manifest: ManifestSource = read_yaml(&idl_root.join("manifest.yaml"))?;
    let defaults: DefaultsSource = read_yaml(&idl_root.join("defaults.yaml"))?;
    let families: FamiliesSource = read_yaml(&idl_root.join("families.yaml"))?;
    let kinds: KindsSource = read_yaml(&idl_root.join("kinds.yaml"))?;
    let overlay_errors: ErrorsSource = read_yaml(&idl_root.join("errors.yaml"))?;
    let dto_inventory: DtoInventorySource = read_yaml(&idl_root.join("dto-inventory.yaml"))?;

    let family_layers = named_layers(families.families, "family")?;
    let kind_layers = named_layers(kinds.kinds, "kind")?;
    let registered_errors = registered_error_map();
    validate_error_overlay(&overlay_errors.errors, &registered_errors)?;

    let command_refs = enum_variants(&repo_root.join("crates/executor-next/src/command.rs"))?;
    let output_refs = enum_variants(&repo_root.join("crates/executor-next/src/output.rs"))?;
    let response_models: BTreeSet<String> = dto_inventory.response_models.into_iter().collect();

    let mut command_entries = Vec::new();
    for file_name in ["kv.yaml", "vector.yaml"] {
        let command_path = idl_root.join("commands").join(file_name);
        validate_command_source_text(&command_path)?;
        let command_file: CommandsFileSource = read_yaml(&command_path)?;
        for command in command_file.commands {
            command_entries.push((command_path.clone(), command));
        }
    }

    let mut seen_ids = BTreeSet::new();
    let mut seen_cli_paths = BTreeMap::new();
    let mut seen_mcp_names = BTreeMap::new();
    let mut resolved = Vec::new();

    for (command_path, command) in command_entries {
        if !seen_ids.insert(command.id.clone()) {
            return Err(invalid(format!("duplicate command id `{}`", command.id)));
        }
        let entry = resolve_command(
            repo_root,
            &idl_root,
            &manifest,
            &defaults.fields,
            &family_layers,
            &kind_layers,
            &overlay_errors.errors,
            &registered_errors,
            &command_refs,
            &output_refs,
            &response_models,
            &command_path,
            &command,
        )?;

        let cli_key = entry.cli.path.join(" ");
        if let Some(existing) = seen_cli_paths.insert(cli_key.clone(), entry.id.clone()) {
            return Err(invalid(format!(
                "duplicate cli path `{cli_key}` for `{existing}` and `{}`",
                entry.id
            )));
        }
        if let Some(existing) = seen_mcp_names.insert(entry.mcp.name.clone(), entry.id.clone()) {
            return Err(invalid(format!(
                "duplicate mcp name `{}` for `{existing}` and `{}`",
                entry.mcp.name, entry.id
            )));
        }
        resolved.push(entry);
    }

    resolved.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(CommandIndex {
        generated: true,
        schema_version: manifest.schema_version,
        generator_version: manifest.generator_version,
        commands: resolved,
    })
}

/// Serializes a resolved index with stable formatting.
pub fn to_generated_json(index: &CommandIndex) -> Result<String> {
    let mut json = serde_json::to_string_pretty(index).map_err(|source| IdlError::Json {
        path: PathBuf::from("command-index.json"),
        source,
    })?;
    json.push('\n');
    Ok(json)
}

/// Generates `crates/executor-next/idl/v1/generated/command-index.json`.
pub fn generate(repo_root: &Path) -> Result<()> {
    let index = resolve_index(repo_root)?;
    let json = to_generated_json(&index)?;
    let path = command_index_path(repo_root);
    fs::write(&path, json).map_err(|source| IdlError::Write { path, source })
}

/// Checks whether the generated command index is fresh.
pub fn check(repo_root: &Path) -> Result<()> {
    let index = resolve_index(repo_root)?;
    let expected = to_generated_json(&index)?;
    let path = command_index_path(repo_root);
    let actual = fs::read_to_string(&path).map_err(|source| IdlError::Read {
        path: path.clone(),
        source,
    })?;
    if actual == expected {
        Ok(())
    } else {
        Err(IdlError::Stale { path })
    }
}

/// Resolves generated CLI command metadata from the checked-in command index.
pub fn resolve_default_cli_index() -> Result<CliCommandIndex> {
    resolve_cli_index(&default_repo_root())
}

/// Resolves generated CLI command metadata under `repo_root`.
pub fn resolve_cli_index(repo_root: &Path) -> Result<CliCommandIndex> {
    let path = command_index_path(repo_root);
    let text = fs::read_to_string(&path).map_err(|source| IdlError::Read {
        path: path.clone(),
        source,
    })?;
    let source_checksum = checksum_sha256(text.as_bytes());
    let command_index: CommandIndex =
        serde_json::from_str(&text).map_err(|source| IdlError::Json {
            path: path.clone(),
            source,
        })?;
    cli_index_from_command_index(repo_root, &path, source_checksum, command_index)
}

/// Serializes generated CLI command metadata with stable formatting.
pub fn to_generated_cli_json(index: &CliCommandIndex) -> Result<String> {
    let mut json = serde_json::to_string_pretty(index).map_err(|source| IdlError::Json {
        path: PathBuf::from(CLI_COMMAND_INDEX_FILE),
        source,
    })?;
    json.push('\n');
    Ok(json)
}

/// Generates `crates/executor-next/idl/v1/generated/cli-command-index.json`.
pub fn generate_cli(repo_root: &Path) -> Result<()> {
    let index = resolve_cli_index(repo_root)?;
    let json = to_generated_cli_json(&index)?;
    let path = cli_command_index_path(repo_root);
    fs::write(&path, json).map_err(|source| IdlError::Write { path, source })
}

/// Checks whether the generated CLI command metadata is fresh.
pub fn check_cli(repo_root: &Path) -> Result<()> {
    let index = resolve_cli_index(repo_root)?;
    let expected = to_generated_cli_json(&index)?;
    let path = cli_command_index_path(repo_root);
    let actual = fs::read_to_string(&path).map_err(|source| IdlError::Read {
        path: path.clone(),
        source,
    })?;
    if actual == expected {
        Ok(())
    } else {
        Err(IdlError::CliStale { path })
    }
}

fn cli_index_from_command_index(
    repo_root: &Path,
    source_path: &Path,
    source_checksum: String,
    command_index: CommandIndex,
) -> Result<CliCommandIndex> {
    validate_cli_source_index(&command_index)?;
    let source = CliIndexSourceInfo {
        path: relative_to(source_path, repo_root),
        checksum_sha256: source_checksum,
        schema_version: command_index.schema_version.clone(),
        generator_version: command_index.generator_version.clone(),
    };

    let mut seen_ids = BTreeSet::new();
    let mut seen_paths = BTreeMap::new();
    let mut commands = Vec::with_capacity(command_index.commands.len());
    for command in command_index.commands {
        let entry = cli_entry_from_resolved(command)?;
        if !seen_ids.insert(entry.id.clone()) {
            return Err(invalid(format!(
                "duplicate command id `{}` in command index",
                entry.id
            )));
        }
        let path_key = cli_path_key(&entry.path);
        if let Some(existing) = seen_paths.insert(path_key.clone(), entry.id.clone()) {
            return Err(invalid(format!(
                "duplicate cli path `{path_key}` for `{existing}` and `{}`",
                entry.id
            )));
        }
        commands.push(entry);
    }

    commands.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut by_id = BTreeMap::new();
    let mut by_path = BTreeMap::new();
    let mut family_commands: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (index, command) in commands.iter().enumerate() {
        by_id.insert(command.id.clone(), index);
        by_path.insert(command.path_display.clone(), command.id.clone());
        family_commands
            .entry(command.family.clone())
            .or_default()
            .push(command.id.clone());
    }

    let families = family_commands
        .into_iter()
        .map(|(id, commands)| CliFamilyGroup {
            command_count: commands.len(),
            id,
            commands,
        })
        .collect::<Vec<_>>();

    Ok(CliCommandIndex {
        generated: true,
        schema_version: CLI_SCHEMA_VERSION.to_owned(),
        generator_version: CLI_GENERATOR_VERSION.to_owned(),
        source,
        command_count: commands.len(),
        families,
        commands,
        lookup: CliLookupTables { by_id, by_path },
    })
}

fn validate_cli_source_index(index: &CommandIndex) -> Result<()> {
    if !index.generated {
        return Err(invalid("command index must be marked generated"));
    }
    if index.schema_version != SUPPORTED_COMMAND_SCHEMA_VERSION {
        return Err(invalid(format!(
            "unsupported command index schema `{}`",
            index.schema_version
        )));
    }
    if index.generator_version != SUPPORTED_COMMAND_GENERATOR_VERSION {
        return Err(invalid(format!(
            "unsupported command index generator `{}`",
            index.generator_version
        )));
    }
    if index.commands.is_empty() {
        return Err(invalid("command index must contain at least one command"));
    }
    Ok(())
}

fn cli_entry_from_resolved(command: ResolvedCommand) -> Result<CliCommandEntry> {
    let entry = CliCommandEntry {
        id: command.id,
        path_display: cli_path_key(&command.cli.path),
        path: command.cli.path,
        family: command.family,
        op: command.op,
        kind: command.kind,
        title: command.title,
        summary: command.summary,
        description: command.description,
        docs: command.docs,
        feature: command.feature,
        access: command.access,
        commit: command.commit,
        pagination: command.pagination,
        batch: command.batch,
        input: command.input,
        outputs: command.outputs,
        response_model: command.response_model,
        errors: command.errors,
        fixtures: command.fixtures,
        wire_status: command.wire_status,
    };
    validate_cli_entry(&entry)?;
    Ok(entry)
}

fn validate_cli_entry(entry: &CliCommandEntry) -> Result<()> {
    validate_command_id(&entry.id)?;
    validate_cli_path(&entry.path, &entry.id)?;
    let expected_path_display = cli_path_key(&entry.path);
    if entry.path_display != expected_path_display {
        return Err(invalid(format!(
            "command `{}` has mismatched path_display `{}`",
            entry.id, entry.path_display
        )));
    }
    for (field, value) in [
        ("family", entry.family.as_str()),
        ("op", entry.op.as_str()),
        ("kind", entry.kind.as_str()),
        ("title", entry.title.as_str()),
        ("summary", entry.summary.as_str()),
        ("description", entry.description.as_str()),
        ("docs", entry.docs.as_str()),
        ("feature", entry.feature.as_str()),
        ("access", entry.access.as_str()),
        ("commit", entry.commit.as_str()),
        ("pagination", entry.pagination.as_str()),
        ("batch", entry.batch.as_str()),
        ("input", entry.input.as_str()),
        ("response_model", entry.response_model.as_str()),
        ("wire_status", entry.wire_status.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(invalid(format!(
                "command `{}` has empty `{field}`",
                entry.id
            )));
        }
        if contains_placeholder(value) {
            return Err(invalid(format!(
                "command `{}` has unresolved placeholder in `{field}`",
                entry.id
            )));
        }
    }
    validate_docs_path(&entry.docs, &entry.id)?;
    validate_executor_ref_prefix(&entry.input, "Command")?;
    if entry.outputs.is_empty() {
        return Err(invalid(format!(
            "command `{}` has no output references",
            entry.id
        )));
    }
    for output in &entry.outputs {
        validate_executor_ref_prefix(output, "Output")?;
    }
    if entry.errors.is_empty() {
        return Err(invalid(format!(
            "command `{}` has no public error references",
            entry.id
        )));
    }
    for error in &entry.errors {
        if error.code.trim().is_empty() || error.docs.trim().is_empty() {
            return Err(invalid(format!(
                "command `{}` has incomplete public error reference",
                entry.id
            )));
        }
    }
    validate_wire_status(&entry.id, &entry.wire_status)?;
    Ok(())
}

fn validate_executor_ref_prefix(reference: &str, prefix: &str) -> Result<()> {
    variant_name(reference, prefix).map(|_| ())
}

fn cli_path_key(path: &[String]) -> String {
    path.join(" ")
}

fn default_cli_path_for_command_id(command_id: &str) -> Vec<String> {
    command_id
        .split('.')
        .map(|segment| segment.replace('_', "-"))
        .collect()
}

fn command_index_path(repo_root: &Path) -> PathBuf {
    repo_root
        .join(IDL_DIR)
        .join("generated")
        .join(COMMAND_INDEX_FILE)
}

fn cli_command_index_path(repo_root: &Path) -> PathBuf {
    repo_root
        .join(IDL_DIR)
        .join("generated")
        .join(CLI_COMMAND_INDEX_FILE)
}

fn checksum_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn resolve_command(
    repo_root: &Path,
    idl_root: &Path,
    manifest: &ManifestSource,
    defaults: &LayerFields,
    family_layers: &BTreeMap<String, LayerFields>,
    kind_layers: &BTreeMap<String, LayerFields>,
    overlay_errors: &[String],
    registered_errors: &BTreeMap<String, String>,
    command_refs: &BTreeSet<String>,
    output_refs: &BTreeSet<String>,
    response_models: &BTreeSet<String>,
    command_path: &Path,
    command: &CommandSource,
) -> Result<ResolvedCommand> {
    validate_command_id(&command.id)?;
    let (family, op) = split_command_id(&command.id)?;
    let family_layer = family_layers.get(family).ok_or_else(|| {
        invalid(format!(
            "command `{}` references unknown family `{family}`",
            command.id
        ))
    })?;
    let kind_layer = kind_layers.get(&command.kind).ok_or_else(|| {
        invalid(format!(
            "command `{}` references unknown kind `{}`",
            command.id, command.kind
        ))
    })?;

    validate_executor_ref(&command.input, "Command", command_refs)?;
    validate_executor_ref(&command.output, "Output", output_refs)?;
    let outputs = resolve_output_refs(command, output_refs)?;
    validate_fixture_refs(repo_root, &command.fixtures, &command.input, &outputs)?;
    let wire_status = command
        .wire_status
        .clone()
        .unwrap_or_else(|| "stable".to_owned());
    validate_wire_status(&command.id, &wire_status)?;

    let mut layer = ResolvedLayer::default();
    layer.apply(defaults);
    layer.apply(family_layer);
    layer.apply(kind_layer);
    layer.apply_command(command);

    let context = PlaceholderContext::new(family, op, &command.result);
    let docs = expand_required(
        "docs",
        &required(layer.docs, "docs", &command.id)?,
        &context,
    )?;
    validate_docs_path(&docs, &command.id)?;

    let cli_path = match layer.cli_path {
        Some(path) => expand_cli_path(path, &context, &command.id)?,
        None => default_cli_path_for_command_id(&command.id),
    };
    validate_cli_path(&cli_path, &command.id)?;

    let mcp_name_template = layer
        .mcp_name
        .unwrap_or_else(|| "strata_{family}_{op_slug}".to_owned());
    let mcp_name = expand_required("mcp_name", &mcp_name_template, &context)?;
    validate_mcp_name(&mcp_name, &command.id)?;

    let response_model = expand_required(
        "response_model",
        &required(layer.response_model, "response_model", &command.id)?,
        &context,
    )?;
    if !response_models.contains(&response_model) {
        return Err(invalid(format!(
            "command `{}` references unknown response model `{response_model}`",
            command.id
        )));
    }

    let prose = load_prose(idl_root, &command.prose, &layer.snippets)?;
    let errors = resolve_errors(
        &command.id,
        &layer.errors,
        overlay_errors,
        registered_errors,
    )?;

    Ok(ResolvedCommand {
        id: command.id.clone(),
        family: family.to_owned(),
        op: op.to_owned(),
        kind: command.kind.clone(),
        title: command.title.clone(),
        summary: prose.summary,
        description: prose.body,
        docs,
        cli: CliInfo { path: cli_path },
        mcp: McpInfo {
            name: mcp_name,
            description: prose
                .mcp_description
                .unwrap_or_else(|| format!("Run the {} command.", command.id)),
        },
        feature: required(layer.feature, "feature", &command.id)?,
        access: required(layer.access, "access", &command.id)?,
        input: command.input.clone(),
        output: command.output.clone(),
        outputs,
        wire_status,
        response_model,
        commit: required(layer.commit, "commit", &command.id)?,
        pagination: required(layer.pagination, "pagination", &command.id)?,
        batch: required(layer.batch, "batch", &command.id)?,
        errors,
        fixtures: command.fixtures.clone(),
        source: SourceInfo {
            command: relative_to(command_path, repo_root),
            prose: command.prose.clone(),
        },
    })
    .and_then(|entry| {
        validate_generated_entry(&entry, manifest)?;
        Ok(entry)
    })
}

fn validate_generated_entry(entry: &ResolvedCommand, _manifest: &ManifestSource) -> Result<()> {
    for (field, value) in [
        ("title", entry.title.as_str()),
        ("summary", entry.summary.as_str()),
        ("description", entry.description.as_str()),
        ("feature", entry.feature.as_str()),
        ("access", entry.access.as_str()),
        ("wire_status", entry.wire_status.as_str()),
        ("commit", entry.commit.as_str()),
        ("pagination", entry.pagination.as_str()),
        ("batch", entry.batch.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(invalid(format!(
                "command `{}` has empty `{field}`",
                entry.id
            )));
        }
        if contains_placeholder(value) {
            return Err(invalid(format!(
                "command `{}` has unresolved placeholder in `{field}`",
                entry.id
            )));
        }
    }
    Ok(())
}

fn read_yaml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).map_err(|source| IdlError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_yaml::from_str(&text).map_err(|source| IdlError::Yaml {
        path: path.to_path_buf(),
        source,
    })
}

fn named_layers(
    sources: Vec<NamedLayerSource>,
    noun: &str,
) -> Result<BTreeMap<String, LayerFields>> {
    let mut layers = BTreeMap::new();
    for source in sources {
        if layers.insert(source.id.clone(), source.fields).is_some() {
            return Err(invalid(format!("duplicate {noun} id `{}`", source.id)));
        }
    }
    Ok(layers)
}

fn validate_command_source_text(path: &Path) -> Result<()> {
    let text = fs::read_to_string(path).map_err(|source| IdlError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    for forbidden in [
        "fields:",
        "schema:",
        "properties:",
        "request_schema:",
        "response_schema:",
    ] {
        if text.contains(forbidden) {
            return Err(invalid(format!(
                "{} defines DTO fields via `{forbidden}`; command YAML may only reference executor DTOs",
                path.display()
            )));
        }
    }
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('&') || trimmed.contains(": &") || trimmed.contains("<<:") {
            return Err(invalid(format!(
                "{} uses YAML anchors or merge keys; use explicit IDL layers instead",
                path.display()
            )));
        }
    }
    for field in extract_command_fields(&text) {
        if !COMMAND_SOURCE_FIELDS.contains(&field.as_str()) {
            return Err(invalid(format!(
                "{} contains unknown command field `{field}`",
                path.display()
            )));
        }
    }
    Ok(())
}

fn extract_command_fields(text: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut in_command = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("- id:") {
            in_command = true;
        }
        if !in_command {
            continue;
        }
        let indent = line.len() - trimmed.len();
        if indent == 4 {
            if let Some((field, _)) = trimmed.split_once(':') {
                let normalized = field.trim_start_matches("- ").trim();
                fields.push(normalized.to_owned());
            }
        }
    }
    fields
}

fn registered_error_map() -> BTreeMap<String, String> {
    public_error_code_entries()
        .map(|entry| {
            (
                entry.code.to_owned(),
                format!(
                    "https://strata.dev/docs/errors/registry#{}",
                    entry.docs_slug
                ),
            )
        })
        .collect()
}

fn validate_error_overlay(
    overlay_errors: &[String],
    registered_errors: &BTreeMap<String, String>,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for code in overlay_errors {
        if !seen.insert(code) {
            return Err(invalid(format!(
                "duplicate error code `{code}` in errors.yaml"
            )));
        }
        if !registered_errors.contains_key(code) {
            return Err(invalid(format!(
                "unregistered error code `{code}` in errors.yaml"
            )));
        }
    }
    Ok(())
}

fn resolve_errors(
    command_id: &str,
    errors: &[String],
    overlay_errors: &[String],
    registered_errors: &BTreeMap<String, String>,
) -> Result<Vec<ErrorRef>> {
    let allowed: BTreeSet<&str> = overlay_errors.iter().map(String::as_str).collect();
    let mut seen = BTreeSet::new();
    let mut resolved = Vec::new();
    for code in errors {
        if !allowed.contains(code.as_str()) {
            return Err(invalid(format!(
                "command `{command_id}` references error `{code}` not listed in errors.yaml"
            )));
        }
        if seen.insert(code) {
            let docs = registered_errors.get(code).ok_or_else(|| {
                invalid(format!(
                    "command `{command_id}` references unregistered error `{code}`"
                ))
            })?;
            resolved.push(ErrorRef {
                code: code.clone(),
                docs: docs.clone(),
            });
        }
    }
    Ok(resolved)
}

fn enum_variants(path: &Path) -> Result<BTreeSet<String>> {
    let text = fs::read_to_string(path).map_err(|source| IdlError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let mut variants = BTreeSet::new();
    for line in text.lines() {
        if !line.starts_with("    ") || line.starts_with("        ") {
            continue;
        }
        let trimmed = line.trim_start();
        let Some(first) = trimmed.chars().next() else {
            continue;
        };
        if !first.is_ascii_uppercase() {
            continue;
        }
        let name = trimmed
            .split([' ', '{', '(', ','])
            .next()
            .unwrap_or_default();
        if !name.is_empty() {
            variants.insert(name.to_owned());
        }
    }
    Ok(variants)
}

fn validate_executor_ref(reference: &str, prefix: &str, variants: &BTreeSet<String>) -> Result<()> {
    let name = variant_name(reference, prefix)?;
    if variants.contains(name) {
        Ok(())
    } else {
        Err(invalid(format!("unknown DTO reference `{reference}`")))
    }
}

fn variant_name<'a>(reference: &'a str, prefix: &str) -> Result<&'a str> {
    let expected_prefix = format!("{prefix}::");
    let Some(name) = reference.strip_prefix(&expected_prefix) else {
        return Err(invalid(format!(
            "DTO reference `{reference}` must start with `{expected_prefix}`"
        )));
    };
    Ok(name)
}

fn resolve_output_refs(
    command: &CommandSource,
    output_refs: &BTreeSet<String>,
) -> Result<Vec<String>> {
    let outputs = if command.outputs.is_empty() {
        vec![command.output.clone()]
    } else {
        command.outputs.clone()
    };
    if !outputs.iter().any(|output| output == &command.output) {
        return Err(invalid(format!(
            "command `{}` declares primary output `{}` but does not include it in `outputs`",
            command.id, command.output
        )));
    }
    let mut seen = BTreeSet::new();
    for output in &outputs {
        validate_executor_ref(output, "Output", output_refs)?;
        if !seen.insert(output) {
            return Err(invalid(format!(
                "command `{}` declares duplicate output `{output}`",
                command.id
            )));
        }
    }
    Ok(outputs)
}

fn validate_wire_status(command_id: &str, wire_status: &str) -> Result<()> {
    match wire_status {
        "stable" | "transitional" => Ok(()),
        _ => Err(invalid(format!(
            "command `{command_id}` has invalid wire_status `{wire_status}`"
        ))),
    }
}

fn validate_fixture_refs(
    repo_root: &Path,
    fixtures: &FixtureRefs,
    input: &str,
    outputs: &[String],
) -> Result<()> {
    let request_tag = variant_wire_tag(input, "Command")?;
    let output_tags = outputs
        .iter()
        .map(|output| variant_wire_tag(output, "Output"))
        .collect::<Result<Vec<_>>>()?;
    validate_fixture_ref(
        repo_root,
        &fixtures.request,
        "requests/v1",
        true,
        &[request_tag],
    )?;
    let mut response_tags = BTreeSet::new();
    response_tags.insert(validate_fixture_ref(
        repo_root,
        &fixtures.response,
        "responses/v1",
        false,
        &output_tags,
    )?);
    for response in &fixtures.responses {
        response_tags.insert(validate_fixture_ref(
            repo_root,
            response,
            "responses/v1",
            false,
            &output_tags,
        )?);
    }
    for output_tag in output_tags {
        if !response_tags.contains(&output_tag) {
            return Err(invalid(format!(
                "fixtures for `{input}` do not cover output type `{output_tag}`"
            )));
        }
    }
    Ok(())
}

fn validate_fixture_ref(
    repo_root: &Path,
    fixture: &str,
    prefix: &str,
    request: bool,
    expected_tags: &[String],
) -> Result<String> {
    let path = Path::new(fixture);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(invalid(format!(
            "fixture path `{fixture}` must stay under {FIXTURE_ROOT}"
        )));
    }
    if !fixture.starts_with(prefix) {
        return Err(invalid(format!(
            "fixture path `{fixture}` must start with `{prefix}`"
        )));
    }
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        return Err(invalid(format!(
            "fixture path `{fixture}` must end with .json"
        )));
    }
    let full_path = repo_root.join(FIXTURE_ROOT).join(fixture);
    let text = fs::read_to_string(&full_path).map_err(|source| IdlError::Read {
        path: full_path.clone(),
        source,
    })?;
    let actual_tag = fixture_type_tag(&full_path, &text)?;
    if !expected_tags.iter().any(|tag| tag == &actual_tag) {
        return Err(invalid(format!(
            "fixture `{fixture}` has type `{actual_tag}` but expected one of `{}`",
            expected_tags.join("`, `")
        )));
    }
    if request {
        let _: Command = serde_json::from_str(&text).map_err(|source| IdlError::Json {
            path: full_path.clone(),
            source,
        })?;
    } else {
        let _: Output = serde_json::from_str(&text).map_err(|source| IdlError::Json {
            path: full_path.clone(),
            source,
        })?;
    }
    Ok(actual_tag)
}

fn fixture_type_tag(path: &Path, text: &str) -> Result<String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|source| IdlError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    value
        .get("type")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            invalid(format!(
                "fixture `{}` is missing string field `type`",
                path.display()
            ))
        })
}

fn variant_wire_tag(reference: &str, prefix: &str) -> Result<String> {
    Ok(pascal_to_snake(variant_name(reference, prefix)?))
}

fn pascal_to_snake(name: &str) -> String {
    let mut output = String::with_capacity(name.len());
    for (index, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index != 0 {
                output.push('_');
            }
            output.push(ch.to_ascii_lowercase());
        } else {
            output.push(ch);
        }
    }
    output
}

fn load_prose(idl_root: &Path, prose_path: &str, snippets: &[String]) -> Result<Prose> {
    let path = idl_root.join("prose").join(prose_path);
    let text = fs::read_to_string(&path).map_err(|source| IdlError::Read {
        path: path.clone(),
        source,
    })?;
    let (frontmatter, body) = parse_frontmatter(&path, &text)?;
    if frontmatter.summary.trim().is_empty() {
        return Err(invalid(format!(
            "prose `{}` has empty summary",
            path.display()
        )));
    }
    if body.trim().is_empty() {
        return Err(invalid(format!(
            "prose `{}` has empty body",
            path.display()
        )));
    }

    let mut description = body.trim().to_owned();
    for snippet in snippets {
        let snippet_path = idl_root.join("prose/snippets").join(snippet);
        let snippet_text = fs::read_to_string(&snippet_path).map_err(|source| IdlError::Read {
            path: snippet_path.clone(),
            source,
        })?;
        if snippet_text.trim().is_empty() {
            return Err(invalid(format!(
                "snippet `{}` is empty",
                snippet_path.display()
            )));
        }
        description.push_str("\n\n");
        description.push_str(snippet_text.trim());
    }

    Ok(Prose {
        summary: frontmatter.summary,
        mcp_description: frontmatter.mcp_description,
        body: description,
    })
}

fn parse_frontmatter(path: &Path, text: &str) -> Result<(ProseFrontmatter, String)> {
    let Some(stripped) = text.strip_prefix("---\n") else {
        return Err(invalid(format!(
            "prose `{}` must start with YAML frontmatter",
            path.display()
        )));
    };
    let Some((frontmatter, body)) = stripped.split_once("\n---\n") else {
        return Err(invalid(format!(
            "prose `{}` is missing closing frontmatter marker",
            path.display()
        )));
    };
    let parsed = serde_yaml::from_str(frontmatter).map_err(|source| IdlError::Yaml {
        path: path.to_path_buf(),
        source,
    })?;
    Ok((parsed, body.to_owned()))
}

struct PlaceholderContext<'a> {
    family: &'a str,
    op: &'a str,
    op_path: String,
    op_slug: String,
    result: &'a str,
}

impl<'a> PlaceholderContext<'a> {
    fn new(family: &'a str, op: &'a str, result: &'a str) -> Self {
        Self {
            family,
            op,
            op_path: op.replace('.', "/"),
            op_slug: op.replace('.', "_"),
            result,
        }
    }

    fn value(&self, key: &str) -> Option<&str> {
        match key {
            "family" => Some(self.family),
            "op" => Some(self.op),
            "op_path" => Some(&self.op_path),
            "op_slug" => Some(&self.op_slug),
            "result" => Some(self.result),
            _ => None,
        }
    }
}

fn expand_required(
    field: &str,
    template: &str,
    context: &PlaceholderContext<'_>,
) -> Result<String> {
    let mut output = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let (head, after_head) = rest.split_at(start);
        output.push_str(head);
        let Some(end) = after_head.find('}') else {
            return Err(invalid(format!(
                "unclosed placeholder in `{field}` template `{template}`"
            )));
        };
        let placeholder = &after_head[1..end];
        let value = context.value(placeholder).ok_or_else(|| {
            invalid(format!(
                "unknown placeholder `{{{placeholder}}}` in `{field}` template `{template}`"
            ))
        })?;
        output.push_str(value);
        rest = &after_head[end + 1..];
    }
    output.push_str(rest);
    if contains_placeholder(&output) {
        return Err(invalid(format!(
            "unresolved placeholder in `{field}` template `{template}`"
        )));
    }
    Ok(output)
}

fn expand_cli_path(
    path: Vec<String>,
    context: &PlaceholderContext<'_>,
    command_id: &str,
) -> Result<Vec<String>> {
    path.into_iter()
        .map(|segment| expand_required("cli_path", &segment, context))
        .collect::<Result<Vec<_>>>()
        .and_then(|expanded| {
            validate_cli_path(&expanded, command_id)?;
            Ok(expanded)
        })
}

fn contains_placeholder(value: &str) -> bool {
    value.contains('{') || value.contains('}')
}

fn split_command_id(id: &str) -> Result<(&str, &str)> {
    id.split_once('.')
        .ok_or_else(|| invalid(format!("command id `{id}` must contain a family and op")))
}

fn validate_command_id(id: &str) -> Result<()> {
    let (family, op) = split_command_id(id)?;
    if family.is_empty() || op.is_empty() {
        return Err(invalid(format!("command id `{id}` is malformed")));
    }
    for part in id.split('.') {
        if part.is_empty()
            || !part
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
        {
            return Err(invalid(format!(
                "command id `{id}` must use lower snake-case dot segments"
            )));
        }
    }
    Ok(())
}

fn validate_docs_path(docs: &str, command_id: &str) -> Result<()> {
    if docs.starts_with("/docs/") && !contains_placeholder(docs) {
        Ok(())
    } else {
        Err(invalid(format!(
            "command `{command_id}` resolved invalid docs path `{docs}`"
        )))
    }
}

fn validate_cli_path(path: &[String], command_id: &str) -> Result<()> {
    if path.is_empty() {
        return Err(invalid(format!(
            "command `{command_id}` has empty cli path"
        )));
    }
    for segment in path {
        if segment.is_empty()
            || !segment
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
        {
            return Err(invalid(format!(
                "command `{command_id}` has invalid cli path segment `{segment}`"
            )));
        }
    }
    Ok(())
}

fn validate_mcp_name(name: &str, command_id: &str) -> Result<()> {
    if name.starts_with("strata_")
        && name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
    {
        Ok(())
    } else {
        Err(invalid(format!(
            "command `{command_id}` resolved invalid mcp name `{name}`"
        )))
    }
}

fn required(value: Option<String>, field: &str, command_id: &str) -> Result<String> {
    value.ok_or_else(|| invalid(format!("command `{command_id}` is missing `{field}`")))
}

fn append_unique(target: &mut Vec<String>, values: &[String]) {
    for value in values {
        if !target.contains(value) {
            target.push(value.clone());
        }
    }
}

fn relative_to(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn invalid(message: impl Into<String>) -> IdlError {
    IdlError::Invalid(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_expansion_rejects_unknown_placeholders() {
        let context = PlaceholderContext::new("kv", "put", "KvWrite");
        let error = expand_required("docs", "/docs/{unknown}", &context)
            .expect_err("unknown placeholder should fail");
        assert!(error.to_string().contains("unknown placeholder"));
    }

    #[test]
    fn executor_variant_refs_resolve_to_wire_tags() {
        assert_eq!(
            variant_wire_tag("Command::KvGetv", "Command").expect("command tag resolves"),
            "kv_getv"
        );
        assert_eq!(
            variant_wire_tag("Output::KeysPage", "Output").expect("output tag resolves"),
            "keys_page"
        );
        assert_eq!(
            variant_wire_tag("Output::VectorIndexQuery", "Output").expect("output tag resolves"),
            "vector_index_query"
        );
    }

    #[test]
    fn wire_status_allows_only_stable_or_transitional() {
        validate_wire_status("kv.put", "stable").expect("stable is valid");
        validate_wire_status("vector.collection.create", "transitional")
            .expect("transitional is valid");
        let error = validate_wire_status("kv.put", "experimental")
            .expect_err("unknown wire status should fail");
        assert!(error.to_string().contains("invalid wire_status"));
    }
}
