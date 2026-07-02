//! IDL-driven command discovery helpers for the Strata V1 CLI.

#![deny(unsafe_code)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::result_large_err)]

use std::ffi::OsString;
use std::path::PathBuf;

use serde::Serialize;
use strata_executor_next::cli_metadata::{
    CliCommandCatalog, CliCommandEntry, CliCommandSuggestions, CliFamilyGroup, CliMetadataError,
};
use strata_executor_next::{ErrorStatus, ExecutorError};

mod execution;
mod kv_execution;

/// Production CLI name used in user-facing help text.
pub const PRODUCTION_COMMAND_NAME: &str = "strata";

const OUTPUT_SCHEMA_VERSION: &str = "strata.cli.output.v1";
const COMMAND_DISCOVERY_DOCS: &str = "/docs/cli/commands";
const UNKNOWN_COMMAND_CODE: &str = "invalid_argument.cli.command_unknown";
const UNKNOWN_FAMILY_CODE: &str = "invalid_argument.cli.family_unknown";

/// Captured CLI process output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CliProcessOutput {
    /// Process exit code.
    pub exit_code: i32,
    /// Text that should be written to standard output.
    pub stdout: String,
    /// Text that should be written to standard error.
    pub stderr: String,
}

/// Run the Strata CLI command-discovery entry point with arbitrary OS args.
pub fn run_args<I, T>(args: I) -> CliProcessOutput
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let mut args = args
        .into_iter()
        .map(Into::into)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    if !args.is_empty() {
        args.remove(0);
    }

    match run_inner(args) {
        Ok(stdout) => CliProcessOutput {
            exit_code: 0,
            stdout: ensure_trailing_newline(stdout),
            stderr: String::new(),
        },
        Err(error) => error.into_output(),
    }
}

/// Render top-level help from generated CLI metadata.
pub fn render_top_level_help(catalog: &CliCommandCatalog) -> String {
    let mut lines = vec![
        "Strata".to_string(),
        String::new(),
        "Usage: strata <command> [options]".to_string(),
        String::new(),
        "Commands:".to_string(),
        "  commands    List generated command metadata.".to_string(),
        "  explain     Explain a command from generated metadata.".to_string(),
        "  kv          Execute KV commands against a database.".to_string(),
        String::new(),
        "Families:".to_string(),
    ];
    for family in catalog.families() {
        lines.push(format!(
            "  {:<8} {} commands",
            family.id, family.command_count
        ));
    }
    lines.push(String::new());
    lines.push("Examples:".to_string());
    lines.push("  strata commands --family kv".to_string());
    lines.push("  strata explain kv.put".to_string());
    lines.push("  strata --db ./my-db kv put user Claude".to_string());
    lines.push("  strata explain vector query".to_string());
    join_lines(lines)
}

/// Render family help from generated CLI metadata.
pub fn render_family_help(catalog: &CliCommandCatalog, family: &CliFamilyGroup) -> String {
    let mut lines = vec![
        format!("{} commands", family.id),
        String::new(),
        "Usage: strata commands --family <family>".to_string(),
        String::new(),
    ];
    for command in commands_for_group(catalog, family) {
        lines.push(format!(
            "  {:<32} {}",
            command.path_display, command.summary
        ));
    }
    join_lines(lines)
}

/// Render command help from generated CLI metadata.
pub fn render_command_help(command: &CliCommandEntry) -> String {
    render_explain_human(command)
}

fn run_inner(mut args: Vec<String>) -> Result<String, CliError> {
    let format = extract_format(&mut args)?;
    let db = extract_db(&mut args, format)?;
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        args.retain(|arg| arg != "--help" && arg != "-h");
        return render_help_for(&args, format);
    }

    let Some(command) = args.first().map(String::as_str) else {
        let catalog = load_catalog(format)?;
        return Ok(match format {
            OutputFormat::Human => render_top_level_help(&catalog),
            OutputFormat::Json => json_output(&TopLevelHelpJson::from_catalog(&catalog)),
        });
    };

    match command {
        "commands" => {
            args.remove(0);
            run_commands(args, format)
        }
        "explain" => {
            args.remove(0);
            run_explain(&args, format)
        }
        "kv" => {
            args.remove(0);
            kv_execution::run_kv(args, format, db)
        }
        _ => Err(CliError::unknown_command(args.join(" "), format)),
    }
}

fn render_help_for(args: &[String], format: OutputFormat) -> Result<String, CliError> {
    let catalog = load_catalog(format)?;
    if args.is_empty() {
        return Ok(match format {
            OutputFormat::Human => render_top_level_help(&catalog),
            OutputFormat::Json => json_output(&TopLevelHelpJson::from_catalog(&catalog)),
        });
    }

    match args.first().map(String::as_str) {
        Some("commands") => Ok(match format {
            OutputFormat::Human => render_commands_help(&catalog),
            OutputFormat::Json => json_output(&TopLevelHelpJson::from_catalog(&catalog)),
        }),
        Some("explain") => Ok(match format {
            OutputFormat::Human => render_explain_help(),
            OutputFormat::Json => json_output(&TopLevelHelpJson::from_catalog(&catalog)),
        }),
        Some("kv") => {
            if args.len() == 1 {
                return Ok(match format {
                    OutputFormat::Human => render_kv_help(&catalog),
                    OutputFormat::Json => json_output(&TopLevelHelpJson::from_catalog(&catalog)),
                });
            }
            let selector = args.join(" ");
            let Some(command) = catalog.command(&selector) else {
                return Err(CliError::unknown_command(selector, format));
            };
            Ok(match format {
                OutputFormat::Human => render_command_help(command),
                OutputFormat::Json => json_output(&ExplainJson::new(&catalog, command)),
            })
        }
        Some(selector) => {
            if let Some(family) = catalog.family(selector) {
                Ok(render_family_help(&catalog, family))
            } else if let Some(command) = catalog.command(selector) {
                Ok(render_command_help(command))
            } else {
                Err(CliError::unknown_command(args.join(" "), format))
            }
        }
        None => unreachable!("empty args handled above"),
    }
}

fn run_commands(mut args: Vec<String>, format: OutputFormat) -> Result<String, CliError> {
    let family = extract_family(&mut args, format)?;
    if !args.is_empty() {
        return Err(CliError::usage(
            format!("unexpected arguments for commands: {}", args.join(" ")),
            format,
        ));
    }

    let catalog = load_catalog(format)?;
    if let Some(family_id) = family {
        let Some(group) = catalog.family(&family_id) else {
            return Err(CliError::unknown_family(family_id, format));
        };
        let commands = commands_for_group(&catalog, group);
        return Ok(match format {
            OutputFormat::Human => render_commands_human(&catalog, Some(group)),
            OutputFormat::Json => json_output(&CommandsJson::family(&catalog, group, commands)),
        });
    }

    Ok(match format {
        OutputFormat::Human => render_commands_human(&catalog, None),
        OutputFormat::Json => json_output(&CommandsJson::all(&catalog)),
    })
}

fn run_explain(args: &[String], format: OutputFormat) -> Result<String, CliError> {
    if args.is_empty() {
        return Err(CliError::usage(
            "missing command selector for explain".to_string(),
            format,
        ));
    }

    let selector = args.join(" ");
    let catalog = load_catalog(format)?;
    let Some(command) = catalog.command(&selector) else {
        return Err(CliError::unknown_command(selector, format));
    };

    Ok(match format {
        OutputFormat::Human => render_explain_human(command),
        OutputFormat::Json => json_output(&ExplainJson::new(&catalog, command)),
    })
}

fn load_catalog(format: OutputFormat) -> Result<CliCommandCatalog, CliError> {
    CliCommandCatalog::embedded().map_err(|error| CliError::metadata(&error, format))
}

fn extract_format(args: &mut Vec<String>) -> Result<OutputFormat, CliError> {
    let mut format = OutputFormat::Human;
    let mut offset = 0;
    while offset < args.len() {
        match args[offset].as_str() {
            execution::ARGUMENT_DELIMITER => break,
            "--json" => {
                format = OutputFormat::Json;
                args.remove(offset);
            }
            "--format" => {
                args.remove(offset);
                let Some(value) = args.get(offset).cloned() else {
                    return Err(CliError::usage(
                        "missing value for --format".to_string(),
                        format,
                    ));
                };
                args.remove(offset);
                format = OutputFormat::parse(&value)?;
            }
            _ => offset += 1,
        }
    }
    Ok(format)
}

fn extract_db(args: &mut Vec<String>, format: OutputFormat) -> Result<Option<PathBuf>, CliError> {
    let mut db = None;
    let mut offset = 0;
    while offset < args.len() {
        match args[offset].as_str() {
            execution::ARGUMENT_DELIMITER => break,
            "--db" | "--database" => {
                let flag = args.remove(offset);
                if db.is_some() {
                    return Err(CliError::usage(format!("duplicate {flag}"), format));
                }
                let Some(value) = args.get(offset).cloned() else {
                    return Err(CliError::usage(format!("missing value for {flag}"), format));
                };
                if value.starts_with("--") {
                    return Err(CliError::usage(format!("missing value for {flag}"), format));
                }
                args.remove(offset);
                db = Some(PathBuf::from(value));
            }
            _ => offset += 1,
        }
    }
    Ok(db)
}

fn extract_family(
    args: &mut Vec<String>,
    format: OutputFormat,
) -> Result<Option<String>, CliError> {
    let mut family = None;
    let mut offset = 0;
    while offset < args.len() {
        if args[offset] == "--family" {
            args.remove(offset);
            if family.is_some() {
                return Err(CliError::usage("duplicate --family".to_string(), format));
            }
            let Some(value) = args.get(offset).cloned() else {
                return Err(CliError::usage(
                    "missing value for --family".to_string(),
                    format,
                ));
            };
            if value.starts_with("--") {
                return Err(CliError::usage(
                    "missing value for --family".to_string(),
                    format,
                ));
            }
            args.remove(offset);
            family = Some(value);
        } else {
            offset += 1;
        }
    }
    Ok(family)
}

fn render_commands_human(catalog: &CliCommandCatalog, family: Option<&CliFamilyGroup>) -> String {
    let groups = match family {
        Some(group) => vec![group],
        None => catalog.families().iter().collect::<Vec<_>>(),
    };
    let mut lines = vec!["Strata commands".to_string(), String::new()];
    for group in groups {
        lines.push(group.id.clone());
        for command in commands_for_group(catalog, group) {
            let marker = if command.wire_status == "transitional" {
                " [transitional]"
            } else {
                ""
            };
            lines.push(format!(
                "  {:<32} {}{}",
                command.path_display, command.summary, marker
            ));
        }
        lines.push(String::new());
    }
    lines.push("Use `strata explain <command>` for details.".to_string());
    join_lines(lines)
}

fn render_commands_help(catalog: &CliCommandCatalog) -> String {
    let mut lines = vec![
        "List Strata commands".to_string(),
        String::new(),
        "Usage: strata commands [--family <family>] [--format json]".to_string(),
        String::new(),
        "Families:".to_string(),
    ];
    for family in catalog.families() {
        lines.push(format!(
            "  {:<8} {} commands",
            family.id, family.command_count
        ));
    }
    join_lines(lines)
}

fn render_kv_help(catalog: &CliCommandCatalog) -> String {
    let family = catalog
        .family("kv")
        .expect("embedded metadata has KV family");
    let mut lines = vec![
        "KV commands".to_string(),
        String::new(),
        "Usage: strata --db <path> kv <operation> [options]".to_string(),
        String::new(),
        "Operations:".to_string(),
    ];
    for command in commands_for_group(catalog, family) {
        lines.push(format!(
            "  {:<24} {}",
            command.path_display, command.summary
        ));
    }
    lines.push(String::new());
    lines.push("Common options:".to_string());
    lines.push("  --branch <name>       Target branch.".to_string());
    lines.push("  --space <name>        Target product space.".to_string());
    lines.push("  --format human|json   Select output format.".to_string());
    lines.push("  --                    Treat following tokens as KV operands.".to_string());
    lines.push(String::new());
    lines.push("Examples:".to_string());
    lines.push("  strata --db ./my-db kv put user Claude".to_string());
    lines.push("  strata --db ./my-db kv put flag -- --json".to_string());
    lines.push("  strata --db ./my-db kv get user --format json".to_string());
    lines.push("  strata kv put --help".to_string());
    join_lines(lines)
}

fn render_explain_help() -> String {
    join_lines(vec![
        "Explain a Strata command".to_string(),
        String::new(),
        "Usage: strata explain <command-id|command path> [--format json]".to_string(),
        String::new(),
        "Examples:".to_string(),
        "  strata explain kv.put".to_string(),
        "  strata explain kv put".to_string(),
        "  strata explain vector.collection.create".to_string(),
        "  strata explain vector collection create".to_string(),
    ])
}

fn render_explain_human(command: &CliCommandEntry) -> String {
    let mut lines = vec![
        command.path_display.clone(),
        command.title.clone(),
        String::new(),
        command.summary.clone(),
        String::new(),
        command.description.clone(),
        String::new(),
        "Facts:".to_string(),
        format!("  ID: {}", command.id),
        format!("  Docs: {}", command.docs),
        format!("  Access: {}", command.access),
        format!("  Commit: {}", command.commit),
        format!("  Pagination: {}", command.pagination),
        format!("  Batch: {}", command.batch),
        format!("  Input: {}", command.input),
        format!("  Outputs: {}", command.outputs.join(", ")),
        format!("  Response: {}", command.response_model),
        format!("  Wire: {}", command.wire_status),
        String::new(),
        "Errors:".to_string(),
    ];
    for error in &command.errors {
        lines.push(format!("  {}", error.code));
    }
    join_lines(lines)
}

fn commands_for_group<'a>(
    catalog: &'a CliCommandCatalog,
    group: &CliFamilyGroup,
) -> Vec<&'a CliCommandEntry> {
    group
        .commands
        .iter()
        .filter_map(|id| catalog.command_by_id(id))
        .collect()
}

pub(crate) fn json_output<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).expect("CLI JSON output must be serializable")
}

fn ensure_trailing_newline(mut text: String) -> String {
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn join_lines(lines: impl IntoIterator<Item = String>) -> String {
    lines.into_iter().collect::<Vec<_>>().join("\n")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputFormat {
    Human,
    Json,
}

impl OutputFormat {
    fn parse(value: &str) -> Result<Self, CliError> {
        match value {
            "human" => Ok(Self::Human),
            "json" => Ok(Self::Json),
            _ => Err(CliError::usage(
                format!("unsupported output format `{value}`"),
                Self::Human,
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CliError {
    code: String,
    message: String,
    docs: String,
    exit_code: i32,
    format: OutputFormat,
    details: CliErrorDetails,
}

impl CliError {
    pub(crate) fn usage(message: String, format: OutputFormat) -> Self {
        Self {
            code: "invalid_argument.cli.usage".to_string(),
            message,
            docs: COMMAND_DISCOVERY_DOCS.to_string(),
            exit_code: 2,
            format,
            details: CliErrorDetails::Usage,
        }
    }

    pub(crate) fn executor(error: &ExecutorError, format: OutputFormat) -> Self {
        let status = error.status().clone();
        Self {
            code: status.code().to_owned(),
            message: status.message().to_owned(),
            docs: status.docs_url().to_owned(),
            exit_code: 1,
            format,
            details: CliErrorDetails::Executor(status),
        }
    }

    fn unknown_command(selector: String, format: OutputFormat) -> Self {
        let suggestions = CliCommandCatalog::embedded()
            .ok()
            .map_or_else(CliCommandSuggestions::default, |catalog| {
                catalog.suggestions(&selector, 3)
            });
        Self {
            code: UNKNOWN_COMMAND_CODE.to_string(),
            message: format!("unknown Strata command `{selector}`"),
            docs: COMMAND_DISCOVERY_DOCS.to_string(),
            exit_code: 2,
            format,
            details: CliErrorDetails::UnknownCommand {
                selector,
                suggestions,
            },
        }
    }

    fn unknown_family(family: String, format: OutputFormat) -> Self {
        let suggestions = CliCommandCatalog::embedded()
            .ok()
            .map(|catalog| {
                catalog
                    .families()
                    .iter()
                    .map(|group| group.id.clone())
                    .filter(|id| id.starts_with(family.chars().next().unwrap_or_default()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Self {
            code: UNKNOWN_FAMILY_CODE.to_string(),
            message: format!("unknown Strata command family `{family}`"),
            docs: COMMAND_DISCOVERY_DOCS.to_string(),
            exit_code: 2,
            format,
            details: CliErrorDetails::UnknownFamily {
                family,
                suggestions,
            },
        }
    }

    fn metadata(error: &CliMetadataError, format: OutputFormat) -> Self {
        Self {
            code: "internal.cli.metadata_invalid".to_string(),
            message: error.to_string(),
            docs: COMMAND_DISCOVERY_DOCS.to_string(),
            exit_code: 1,
            format,
            details: CliErrorDetails::Metadata,
        }
    }

    fn into_output(self) -> CliProcessOutput {
        let exit_code = self.exit_code;
        let stderr = match self.format {
            OutputFormat::Human => format!(
                "error: {}\ncode: {}\ndocs: {}\n",
                self.message, self.code, self.docs
            ),
            OutputFormat::Json => match &self.details {
                CliErrorDetails::Executor(status) => {
                    ensure_trailing_newline(json_output(&ExecutorErrorJson {
                        schema_version: OUTPUT_SCHEMA_VERSION,
                        kind: "error",
                        error: status,
                    }))
                }
                _ => ensure_trailing_newline(json_output(&CliErrorJson::from(self))),
            },
        };
        CliProcessOutput {
            exit_code,
            stdout: String::new(),
            stderr,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CliErrorDetails {
    Usage,
    UnknownCommand {
        selector: String,
        suggestions: CliCommandSuggestions,
    },
    UnknownFamily {
        family: String,
        suggestions: Vec<String>,
    },
    Executor(ErrorStatus),
    Metadata,
}

#[derive(Serialize)]
struct CliErrorJson {
    schema_version: &'static str,
    kind: &'static str,
    error: CliErrorJsonBody,
}

impl From<CliError> for CliErrorJson {
    fn from(error: CliError) -> Self {
        Self {
            schema_version: OUTPUT_SCHEMA_VERSION,
            kind: "error",
            error: CliErrorJsonBody {
                code: error.code,
                message: error.message,
                docs: error.docs,
                details: ErrorDetailsJson::from(error.details),
            },
        }
    }
}

#[derive(Serialize)]
struct CliErrorJsonBody {
    code: String,
    message: String,
    docs: String,
    details: ErrorDetailsJson,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ErrorDetailsJson {
    Usage,
    UnknownCommand {
        selector: String,
        suggestions: CliCommandSuggestions,
    },
    UnknownFamily {
        family: String,
        suggestions: Vec<String>,
    },
    Metadata,
}

impl From<CliErrorDetails> for ErrorDetailsJson {
    fn from(details: CliErrorDetails) -> Self {
        match details {
            CliErrorDetails::Usage => Self::Usage,
            CliErrorDetails::UnknownCommand {
                selector,
                suggestions,
            } => Self::UnknownCommand {
                selector,
                suggestions,
            },
            CliErrorDetails::UnknownFamily {
                family,
                suggestions,
            } => Self::UnknownFamily {
                family,
                suggestions,
            },
            CliErrorDetails::Executor(_) | CliErrorDetails::Metadata => Self::Metadata,
        }
    }
}

#[derive(Serialize)]
struct ExecutorErrorJson<'a> {
    schema_version: &'static str,
    kind: &'static str,
    error: &'a ErrorStatus,
}

#[derive(Serialize)]
struct TopLevelHelpJson<'a> {
    schema_version: &'static str,
    kind: &'static str,
    command: &'static str,
    generator_version: &'a str,
    families: &'a [CliFamilyGroup],
}

impl<'a> TopLevelHelpJson<'a> {
    fn from_catalog(catalog: &'a CliCommandCatalog) -> Self {
        Self {
            schema_version: OUTPUT_SCHEMA_VERSION,
            kind: "help",
            command: PRODUCTION_COMMAND_NAME,
            generator_version: &catalog.index().generator_version,
            families: catalog.families(),
        }
    }
}

#[derive(Serialize)]
struct CommandsJson<'a> {
    schema_version: &'static str,
    kind: &'static str,
    generator_version: &'a str,
    source_checksum_sha256: &'a str,
    families: Vec<FamilyCommandsJson<'a>>,
}

impl<'a> CommandsJson<'a> {
    fn all(catalog: &'a CliCommandCatalog) -> Self {
        let families = catalog
            .families()
            .iter()
            .map(|family| Self::family_entry(catalog, family))
            .collect();
        Self::new(catalog, families)
    }

    fn family(
        catalog: &'a CliCommandCatalog,
        family: &'a CliFamilyGroup,
        commands: Vec<&'a CliCommandEntry>,
    ) -> Self {
        Self::new(
            catalog,
            vec![FamilyCommandsJson {
                id: &family.id,
                command_count: commands.len(),
                commands,
            }],
        )
    }

    fn family_entry(
        catalog: &'a CliCommandCatalog,
        family: &'a CliFamilyGroup,
    ) -> FamilyCommandsJson<'a> {
        FamilyCommandsJson {
            id: &family.id,
            command_count: family.command_count,
            commands: commands_for_group(catalog, family),
        }
    }

    fn new(catalog: &'a CliCommandCatalog, families: Vec<FamilyCommandsJson<'a>>) -> Self {
        Self {
            schema_version: OUTPUT_SCHEMA_VERSION,
            kind: "commands",
            generator_version: &catalog.index().generator_version,
            source_checksum_sha256: &catalog.index().source.checksum_sha256,
            families,
        }
    }
}

#[derive(Serialize)]
struct FamilyCommandsJson<'a> {
    id: &'a str,
    command_count: usize,
    commands: Vec<&'a CliCommandEntry>,
}

#[derive(Serialize)]
struct ExplainJson<'a> {
    schema_version: &'static str,
    kind: &'static str,
    generator_version: &'a str,
    source_checksum_sha256: &'a str,
    command: &'a CliCommandEntry,
}

impl<'a> ExplainJson<'a> {
    fn new(catalog: &'a CliCommandCatalog, command: &'a CliCommandEntry) -> Self {
        Self {
            schema_version: OUTPUT_SCHEMA_VERSION,
            kind: "explain",
            generator_version: &catalog.index().generator_version,
            source_checksum_sha256: &catalog.index().source.checksum_sha256,
            command,
        }
    }
}
