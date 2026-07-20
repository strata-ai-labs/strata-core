//! Handwritten CLI layer over `strata-executor`.

#![deny(unsafe_code)]
#![allow(clippy::missing_const_for_fn)]

use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::PathBuf;

use clap::Parser;
use serde_json::Value;
use strata_executor::{Command, Executor, ExecutorError, GraphPropertyDef};

mod agents;
#[cfg(test)]
mod catalog_guard;
mod context;
mod doctor;
mod guidance;
mod init;
mod input;
mod mcp;
mod open;
mod options;
mod render;
mod repl;

use context::{CommandContext, Scope};
use input::{
    bytes_argument, cursor_argument, parse_filter_argument, parse_json_argument,
    parse_optional_filter_argument, parse_optional_json_argument, parse_query_vector_argument,
    parse_relaxed_json_argument, parse_vector_argument,
};
use options::{
    ArrowCommand, BranchCommand, Cli, CommandCommand, ConfigCommand, EventCommand, GraphCommand,
    GraphOntologyCommand, JsonCommand, KvCommand, SpaceCommand, VectorCollectionCommand,
    VectorCommand,
};
use render::{render_error, render_output, render_value};

/// Runs the CLI and returns a process exit code.
pub fn run<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    // Capture the executor's boundary error logs (reference_id + code + source
    // chain) to stderr so the reference id shown in an error message correlates
    // to a real, inspectable line (ERR-2). stdout stays clean for command
    // output. Ignored if a subscriber is already installed (e.g. in tests).
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(tracing::Level::ERROR)
        .try_init();
    match Cli::try_parse_from(args) {
        Ok(cli) => {
            let format = cli.output_format();
            match execute(cli) {
                Ok(exit_code) => exit_code,
                Err(CliError::Executor(error)) => {
                    render_error(error.status(), format);
                    1
                }
                Err(error) => {
                    eprintln!("error: {error}");
                    2
                }
            }
        }
        Err(error) => {
            // clap routes --help/--version to stdout with exit 0 and genuine
            // parse errors to stderr with exit 2; error.print() honors that
            // split. Install scripts verify with `strata --version`, so the
            // success path must be a success.
            let exit = if error.use_stderr() { 2 } else { 0 };
            // A failure to print has nowhere left to report.
            let _ = error.print();
            exit
        }
    }
}

fn execute(cli: Cli) -> Result<i32, CliError> {
    let format = cli.output_format();
    let command = cli.command;
    let mut context = CommandContext::new(cli.branch, cli.space);

    if let Some(command) = command {
        if let Some(name) = deferred_top_command(&command) {
            return Err(deferred_command(name));
        }
        if matches!(command, options::TopCommand::Doctor) {
            // Doctor takes an *optional* database target, so it resolves the
            // target itself instead of going through open_executor's refusal.
            let (report, healthy) = doctor::run_doctor(cli.cache, cli.db, cli.db_path)?;
            render_value(&report, format)?;
            return Ok(i32::from(!healthy));
        }
        let command = match command {
            options::TopCommand::Agents(args) => return agents::run(&args.command, format),
            other => other,
        };
        if matches!(
            command,
            options::TopCommand::Mcp(options::McpArgs {
                command: options::McpCommand::Serve,
            })
        ) {
            // The MCP server owns the process: it opens the target (explicit
            // path / STRATA_DB / --cache; refusal otherwise, like any
            // one-shot command), applies the session scope, and serves stdio
            // until the client closes stdin.
            let opened =
                open::open_executor(cli.cache, cli.db, cli.db_path, open::OpenIntent::OneShot)?;
            let mut executor = opened.executor;
            let scope = context.scope_with_overrides(None, None);
            if let Some(branch) = scope.branch.as_deref() {
                executor = executor.with_default_branch(branch)?;
            }
            if let Some(space) = scope.space.as_deref() {
                executor.set_default_space(space.to_owned())?;
            }
            let exit = mcp::serve(&mut executor)?;
            executor.close()?;
            return Ok(exit);
        }
        if let TopLevelAction::NoDatabase(value) = top_level_without_database(&command)? {
            render_value(&value, format)?;
            return Ok(0);
        }

        if let options::TopCommand::Clone(args) = command {
            // Clone creates a NEW database; it never touches a session
            // database, so it runs on an ephemeral cache executor.
            let mut executor = Executor::open_cache()?;
            let dest = args
                .dest
                .unwrap_or_else(|| PathBuf::from(format!("{}.strata", args.dataset)));
            let output = executor.execute(Command::HubClone {
                dataset: args.dataset,
                branch: args.branch,
                dest: dest.display().to_string(),
                hub_url: args.hub,
            })?;
            render::render_output(&output, format)?;
            executor.close()?;
            return Ok(0);
        }

        let opened =
            open::open_executor(cli.cache, cli.db, cli.db_path, open::OpenIntent::OneShot)?;
        let mut executor = opened.executor;
        if let Some(branch) = context.scope_with_overrides(None, None).branch.as_deref() {
            executor = executor.with_default_branch(branch)?;
        }

        let scope = context.scope_with_overrides(None, None);
        execute_parsed_command(&mut executor, command, &scope, format)?;
        executor.close()?;
        return Ok(0);
    }

    let interactive = std::io::stdin().is_terminal();
    let intent = if interactive {
        open::OpenIntent::Interactive
    } else {
        open::OpenIntent::Pipe
    };
    let opened = open::open_executor(cli.cache, cli.db, cli.db_path, intent)?;
    if opened.implicit_cache {
        // Bare interactive invocation: an ephemeral session, stated plainly
        // so nobody discovers volatility after typing data in (first-run D2).
        eprintln!(
            "strata {} — in-memory session (nothing persisted; run with a path to keep data)",
            env!("CARGO_PKG_VERSION")
        );
        eprintln!("type `help` for commands  |  agents: run `strata agents guide`");
    }
    let mut executor = opened.executor;
    if let Some(branch) = context.scope_with_overrides(None, None).branch.as_deref() {
        executor = executor.with_default_branch(branch)?;
    }

    let saw_pipe_error = if interactive {
        repl::run_repl(&mut executor, &mut context, format)?;
        false
    } else {
        repl::run_pipe(&mut executor, &mut context, format)?
    };
    executor.close()?;
    Ok(i32::from(saw_pipe_error))
}

pub(crate) fn execute_parsed_command(
    executor: &mut Executor,
    command: options::TopCommand,
    scope: &Scope,
    format: options::Format,
) -> Result<(), CliError> {
    if let Some(name) = deferred_top_command(&command) {
        return Err(deferred_command(name));
    }
    // The executor owns branch and space session context (CLI-4). Resolve the
    // current scope onto the executor so every path — including `command run`,
    // which executes raw JSON without per-command scope injection — honors
    // --branch/--space and the REPL `use` context uniformly.
    let executor_branch = executor.default_branch().to_owned();
    let branch = scope.branch.clone().unwrap_or(executor_branch);
    executor.set_default_branch(branch)?;
    let space = scope
        .space
        .clone()
        .unwrap_or_else(|| strata_executor::DEFAULT_SPACE.to_owned());
    executor.set_default_space(space)?;
    let output = match command {
        options::TopCommand::Ping => executor.execute(Command::Ping)?,
        options::TopCommand::Remote => executor.execute(Command::RemoteGet)?,
        options::TopCommand::Clone(_) => {
            unreachable!("clone is dispatched before a session database opens")
        }
        options::TopCommand::Init => {
            let value = init::run_init()?;
            render_value(&value, format)?;
            return Ok(());
        }
        options::TopCommand::Doctor => {
            // Inside a session the database is already open and evidently
            // working, so report installation checks only; the process exit
            // code is unaffected — the session stays alive either way.
            let (report, _healthy) = doctor::run_doctor(false, None, None)?;
            render_value(&report, format)?;
            return Ok(());
        }
        options::TopCommand::Agents(args) => {
            // Exit code is only meaningful for the one-shot path; agents
            // commands never fail healthily inside a session.
            let _exit = agents::run(&args.command, format)?;
            return Ok(());
        }
        options::TopCommand::Mcp(_) => {
            return Err(CliError::usage(
                "`mcp serve` runs as a one-shot command (it owns stdio), not inside a session",
            ));
        }
        options::TopCommand::Info => executor.execute(Command::Info {
            branch: scope.branch.clone(),
        })?,
        options::TopCommand::Health => executor.execute(Command::Health {
            branch: scope.branch.clone(),
        })?,
        options::TopCommand::Metrics => executor.execute(Command::Metrics {
            branch: scope.branch.clone(),
        })?,
        options::TopCommand::Describe => executor.execute(Command::Describe {
            branch: scope.branch.clone(),
        })?,
        options::TopCommand::Config(args) => executor.execute(config_command(args.command))?,
        options::TopCommand::Branch(args) => executor.execute(branch_command(args.command)?)?,
        options::TopCommand::Space(args) => executor.execute(space_command(args.command, scope))?,
        options::TopCommand::Kv(args) => executor.execute(kv_command(args.command, scope)?)?,
        options::TopCommand::Json(args) => executor.execute(json_command(args.command, scope)?)?,
        options::TopCommand::Vector(command) => {
            executor.execute(vector_command(command.command, scope)?)?
        }
        options::TopCommand::Event(args) => {
            executor.execute(event_command(args.command, scope)?)?
        }
        options::TopCommand::Graph(args) => {
            executor.execute(graph_command(args.command, scope)?)?
        }
        options::TopCommand::Arrow(args) => executor.execute(arrow_command(args.command, scope))?,
        #[cfg(feature = "inference")]
        options::TopCommand::Inference(args) => {
            // Make config-file provider keys visible to the runtime (which reads
            // the environment). Env vars already set win — this only fills gaps.
            load_provider_keys_into_env();
            executor.execute(inference_command(args.command)?)?
        }
        options::TopCommand::Command(args) => executor.execute(raw_command(args.command)?)?,
        options::TopCommand::Search(_)
        | options::TopCommand::Recipe(_)
        | options::TopCommand::Txn(_)
        | options::TopCommand::Begin
        | options::TopCommand::Commit
        | options::TopCommand::Rollback
        | options::TopCommand::Flush
        | options::TopCommand::Compact
        | options::TopCommand::Up(_)
        | options::TopCommand::Down(_)
        | options::TopCommand::Uninstall(_) => unreachable!("deferred top commands handled above"),
    };

    render_output(&output, format)?;
    Ok(())
}

fn deferred_top_command(command: &options::TopCommand) -> Option<&'static str> {
    match command {
        options::TopCommand::Search(_) => Some("search"),
        options::TopCommand::Recipe(_) => Some("recipe"),
        options::TopCommand::Txn(_) => Some("txn"),
        options::TopCommand::Begin => Some("begin"),
        options::TopCommand::Commit => Some("commit"),
        options::TopCommand::Rollback => Some("rollback"),
        options::TopCommand::Flush => Some("flush"),
        options::TopCommand::Compact => Some("compact"),
        options::TopCommand::Up(_) => Some("up"),
        options::TopCommand::Down(_) => Some("down"),
        options::TopCommand::Uninstall(_) => Some("uninstall"),
        _ => None,
    }
}

fn deferred_command(name: &str) -> CliError {
    CliError::usage(format!(
        "`{name}` is recognized from the old CLI, but is not available in the V1 CLI surface yet"
    ))
}

enum TopLevelAction {
    NeedsDatabase,
    NoDatabase(Value),
}

fn top_level_without_database(command: &options::TopCommand) -> Result<TopLevelAction, CliError> {
    match command {
        options::TopCommand::Init => Ok(TopLevelAction::NoDatabase(init::run_init()?)),
        options::TopCommand::Config(args) => match &args.command {
            ConfigCommand::Set { key, value } => {
                Ok(TopLevelAction::NoDatabase(user_config_set(key, value)?))
            }
            ConfigCommand::Unset { key } => Ok(TopLevelAction::NoDatabase(user_config_unset(key)?)),
            ConfigCommand::Path => Ok(TopLevelAction::NoDatabase(user_config_path()?)),
            ConfigCommand::Show => Ok(TopLevelAction::NoDatabase(user_config_show())),
            ConfigCommand::GetKey { key } if is_user_config_key(key) => {
                Ok(TopLevelAction::NoDatabase(user_config_get(key)?))
            }
            _ => Ok(TopLevelAction::NeedsDatabase),
        },
        options::TopCommand::Command(args) => match &args.command {
            CommandCommand::Print { json, file } => {
                let command = raw_command_from_sources(json.as_deref(), file.as_ref())?;
                let value = serde_json::to_value(command)?;
                Ok(TopLevelAction::NoDatabase(value))
            }
            CommandCommand::Run { .. } => Ok(TopLevelAction::NeedsDatabase),
        },
        _ => Ok(TopLevelAction::NeedsDatabase),
    }
}

fn config_command(command: ConfigCommand) -> Command {
    match command {
        ConfigCommand::Get => Command::ConfigGet,
        ConfigCommand::GetKey { key } => Command::ConfigureGetKey { key },
        // User-config subcommands are handled before a database opens
        // (top_level_without_database); reaching here is a dispatch bug.
        ConfigCommand::Set { .. }
        | ConfigCommand::Unset { .. }
        | ConfigCommand::Path
        | ConfigCommand::Show => unreachable!("user-config subcommands run without a database"),
    }
}

/// The canonical provider name behind a `<provider>.api_key` config key,
/// validated against the known cloud providers, or `None`.
#[cfg(feature = "inference")]
fn provider_api_key_target(key: &str) -> Option<&'static str> {
    let provider = key.strip_suffix(".api_key")?;
    strata_executor::inference_provider_key_info(provider).map(|info| info.provider)
}

#[cfg(not(feature = "inference"))]
fn provider_api_key_target(_key: &str) -> Option<&'static str> {
    None
}

/// Whether `key` is a user-config key handled without opening a database
/// (`hub.url` or a `<provider>.api_key`).
fn is_user_config_key(key: &str) -> bool {
    key == "hub.url" || provider_api_key_target(key).is_some()
}

/// Mask a secret to a short non-secret prefix, e.g. `sk-ant-****`.
fn redact_key(value: &str) -> String {
    let prefix: String = value.chars().take(7).collect();
    format!("{prefix}****")
}

fn unknown_config_key(key: &str) -> CliError {
    CliError::usage(format!(
        "unknown config key `{key}`; settable keys: hub.url, openai.api_key, \
         anthropic.api_key, google.api_key"
    ))
}

/// `strata config set <key> <value>` — writes the global user config.
/// `hub.url` sets the hub; `<provider>.api_key` stores a cloud API key (0600,
/// never echoed back in plaintext).
fn user_config_set(key: &str, value: &str) -> Result<serde_json::Value, CliError> {
    if key == "hub.url" {
        let path = strata_hub::write_global_hub_url(value)
            .map_err(|error| CliError::usage(error.to_string()))?;
        return Ok(serde_json::json!({
            "key": key,
            "value": value,
            "path": path.display().to_string(),
        }));
    }
    if let Some(provider) = provider_api_key_target(key) {
        let path = strata_hub::write_global_provider_key(provider, value)
            .map_err(|error| CliError::usage(error.to_string()))?;
        return Ok(serde_json::json!({
            "key": key,
            "value": redact_key(value),
            "path": path.display().to_string(),
            "note": "API key stored (env var overrides it)",
        }));
    }
    Err(unknown_config_key(key))
}

fn user_config_unset(key: &str) -> Result<serde_json::Value, CliError> {
    if key == "hub.url" {
        let path = strata_hub::unset_global_hub_url()
            .map_err(|error| CliError::usage(error.to_string()))?;
        return Ok(serde_json::json!({
            "key": key,
            "unset": true,
            "path": path.map(|path| path.display().to_string()),
        }));
    }
    if let Some(provider) = provider_api_key_target(key) {
        let path = strata_hub::unset_global_provider_key(provider)
            .map_err(|error| CliError::usage(error.to_string()))?;
        return Ok(serde_json::json!({
            "key": key,
            "unset": true,
            "path": path.map(|path| path.display().to_string()),
        }));
    }
    Err(unknown_config_key(key))
}

fn user_config_get(key: &str) -> Result<serde_json::Value, CliError> {
    if key == "hub.url" {
        let value = strata_hub::read_global_hub_url()
            .map_err(|error| CliError::usage(error.to_string()))?;
        return Ok(serde_json::json!({
            "key": "hub.url",
            "value": value.map(|url| url.to_string()),
        }));
    }
    if let Some(provider) = provider_api_key_target(key) {
        // Never surface the raw key — report set/unset with a redacted preview.
        let value = strata_hub::read_global_provider_key(provider)
            .map_err(|error| CliError::usage(error.to_string()))?;
        return Ok(serde_json::json!({
            "key": key,
            "set": value.is_some(),
            "value": value.as_deref().map(redact_key),
        }));
    }
    Err(unknown_config_key(key))
}

/// Populate provider API-key environment variables from the global config for
/// any that are not already set (the environment always wins). The inference
/// runtime reads these variables, so this bridges `strata config set
/// <provider>.api_key` to the runtime without the inference layer needing to
/// know about `~/.strata`.
#[cfg(feature = "inference")]
fn load_provider_keys_into_env() {
    for info in strata_executor::INFERENCE_CLOUD_PROVIDER_KEYS {
        if std::env::var_os(info.env_var).is_some() {
            continue;
        }
        if let Ok(Some(key)) = strata_hub::read_global_provider_key(info.provider) {
            std::env::set_var(info.env_var, key);
        }
    }
}

fn user_config_path() -> Result<serde_json::Value, CliError> {
    let path = strata_hub::global_config_path()
        .ok_or_else(|| CliError::usage("the platform exposes no user config directory"))?;
    Ok(serde_json::json!({ "path": path.display().to_string() }))
}

/// `strata config show` — the resolved hub URL and which layer supplied
/// it, the first thing to ask for when strata talks to the wrong hub.
fn user_config_show() -> serde_json::Value {
    match strata_hub::resolve_hub_url(&strata_hub::HubUrlInputs::from_process(None)) {
        Ok(resolved) => serde_json::json!({
            "hub.url": resolved.url.to_string(),
            "source": resolved.source.to_string(),
        }),
        Err(error) => serde_json::json!({
            "hub.url": serde_json::Value::Null,
            "detail": error.to_string(),
        }),
    }
}

fn branch_command(command: BranchCommand) -> Result<Command, CliError> {
    Ok(match command {
        BranchCommand::List => Command::BranchList,
        BranchCommand::Get { branch } => Command::BranchGet { branch },
        BranchCommand::Create { branch } => Command::BranchCreate { branch },
        BranchCommand::Fork {
            source,
            branch,
            version,
            timestamp,
        } => match (version, timestamp) {
            (Some(version), None) => Command::BranchForkAtVersion {
                source,
                branch,
                version,
            },
            (None, Some(timestamp)) => Command::BranchForkAtTimestamp {
                source,
                branch,
                timestamp,
            },
            (None, None) | (Some(_), Some(_)) => Command::BranchForkCurrent { source, branch },
        },
        BranchCommand::Delete { branch } => Command::BranchDelete { branch },
        BranchCommand::Diff(_) => return Err(deferred_command("branch diff")),
        BranchCommand::Merge(_) => return Err(deferred_command("branch merge")),
        BranchCommand::Tag(_) => return Err(deferred_command("branch tag")),
        BranchCommand::Note(_) => return Err(deferred_command("branch note")),
    })
}

fn space_command(command: SpaceCommand, scope: &Scope) -> Command {
    match command {
        SpaceCommand::List => Command::SpaceList {
            branch: scope.branch.clone(),
        },
        SpaceCommand::Create { space } => Command::SpaceCreate {
            branch: scope.branch.clone(),
            space,
        },
        SpaceCommand::Exists { space } => Command::SpaceExists {
            branch: scope.branch.clone(),
            space,
        },
        SpaceCommand::Delete { space, force } => Command::SpaceDelete {
            branch: scope.branch.clone(),
            space,
            force,
        },
    }
}

fn kv_command(command: KvCommand, scope: &Scope) -> Result<Command, CliError> {
    Ok(match command {
        KvCommand::Put { key, value, file } => Command::KvPut {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            key: bytes(key),
            value: bytes_argument(value.as_deref(), file.as_ref())?,
        },
        KvCommand::Get { key, as_of } => Command::KvGet {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            key: bytes(key),
            as_of,
        },
        KvCommand::Delete { key } => Command::KvDelete {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            key: bytes(key),
        },
        KvCommand::List {
            prefix,
            cursor,
            limit,
            as_of,
        } => Command::KvList {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            prefix: prefix.map(bytes),
            cursor: cursor.as_deref().map(cursor_argument).transpose()?,
            limit,
            as_of,
        },
        KvCommand::Scan {
            start,
            cursor,
            limit,
        } => Command::KvScan {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            // A cursor continues from the first unreturned row, so it maps to
            // the inclusive scan start (clap rejects --start with --cursor).
            start: match cursor {
                Some(cursor) => Some(cursor_argument(&cursor)?),
                None => start.map(bytes),
            },
            limit,
        },
        KvCommand::Exists { key } => Command::KvExists {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            key: bytes(key),
        },
        KvCommand::History { key } => Command::KvHistory {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            key: bytes(key),
        },
        KvCommand::Count { prefix } => Command::KvCount {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            prefix: prefix.map(bytes),
            as_of: None,
        },
        KvCommand::Sample { prefix, count } => Command::KvSample {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            prefix: prefix.map(bytes),
            count,
        },
    })
}

fn json_command(command: JsonCommand, scope: &Scope) -> Result<Command, CliError> {
    Ok(match command {
        JsonCommand::Set {
            key,
            path,
            value,
            file,
        } => Command::JsonSet {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            key,
            path,
            value: parse_relaxed_json_argument(value.as_deref(), file.as_ref(), "json value")?,
        },
        JsonCommand::Get { key, path, as_of } => Command::JsonGet {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            key,
            path,
            as_of,
        },
        JsonCommand::Delete { key, path } => Command::JsonDelete {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            key,
            path,
        },
        JsonCommand::List {
            prefix,
            cursor,
            limit,
            as_of,
        } => Command::JsonList {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            prefix,
            cursor,
            limit,
            as_of,
        },
        JsonCommand::Scan {
            start,
            cursor,
            limit,
        } => Command::JsonScan {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            // A cursor continues from the first unreturned document, so it maps
            // to the inclusive scan start (clap rejects --start with --cursor).
            start: cursor.or(start),
            limit,
        },
        JsonCommand::Exists { key } => Command::JsonExists {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            key,
        },
        JsonCommand::History { key } => Command::JsonHistory {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            key,
        },
        JsonCommand::Count { prefix } => Command::JsonCount {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            prefix,
            as_of: None,
        },
        JsonCommand::Sample { prefix, count } => Command::JsonSample {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            prefix,
            count,
        },
        JsonCommand::Index {
            command:
                options::JsonIndexCommand::Create {
                    name,
                    field_path,
                    index_type,
                },
        } => Command::JsonCreateIndex {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            name,
            field_path,
            index_type: index_type.into(),
        },
        JsonCommand::Index {
            command: options::JsonIndexCommand::Drop { name },
        } => Command::JsonDropIndex {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            name,
        },
        JsonCommand::Index {
            command: options::JsonIndexCommand::List,
        } => Command::JsonListIndexes {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
        },
    })
}

#[allow(clippy::too_many_lines)]
fn vector_command(command: VectorCommand, scope: &Scope) -> Result<Command, CliError> {
    Ok(match command {
        VectorCommand::Collection { command } => vector_collection_command(command, scope),
        VectorCommand::Upsert {
            collection,
            key,
            vector,
            metadata,
            file,
            metadata_file,
        } => Command::VectorUpsert {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            collection,
            key,
            vector: parse_vector_argument(vector.as_deref(), file.as_ref(), "vector")?,
            metadata: parse_optional_json_argument(
                metadata.as_deref(),
                metadata_file.as_ref(),
                "vector metadata",
            )?,
        },
        VectorCommand::Get {
            collection,
            key,
            as_of,
        } => Command::VectorGet {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            collection,
            key,
            as_of,
        },
        VectorCommand::History { collection, key } => Command::VectorHistory {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            collection,
            key,
        },
        VectorCommand::Exists { collection, key } => Command::VectorExists {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            collection,
            key,
        },
        VectorCommand::Keys {
            collection,
            prefix,
            cursor,
            limit,
        } => Command::VectorListKeys {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            collection,
            prefix,
            cursor,
            limit,
            as_of: None,
        },
        VectorCommand::Scan {
            collection,
            start,
            cursor,
            limit,
        } => Command::VectorScan {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            collection,
            // A cursor continues from the first unreturned key, so it maps to
            // the inclusive scan start (clap rejects --start with --cursor).
            start: cursor.or(start),
            limit,
        },
        VectorCommand::UpdateMetadata {
            collection,
            key,
            patch,
            file,
        } => Command::VectorUpdateMetadata {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            collection,
            key,
            patch: parse_json_argument(patch.as_deref(), file.as_ref(), "metadata patch")?,
        },
        VectorCommand::Delete { collection, key } => Command::VectorDelete {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            collection,
            key,
        },
        VectorCommand::DeleteAll { collection } => Command::VectorDeleteAll {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            collection,
        },
        VectorCommand::DeleteByFilter {
            collection,
            filter,
            filter_file,
        } => Command::VectorDeleteByFilter {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            collection,
            filter: parse_filter_argument(filter.as_deref(), filter_file.as_ref())?,
        },
        VectorCommand::Query {
            collection,
            query,
            file,
            k,
            filter,
            filter_file,
            as_of,
            diagnostics,
        } => {
            let command_filter =
                parse_optional_filter_argument(filter.as_deref(), filter_file.as_ref())?;
            if diagnostics {
                Command::VectorIndexQuery {
                    branch: scope.branch.clone(),
                    space: scope.space.clone(),
                    collection,
                    query: parse_query_vector_argument(
                        query.as_deref(),
                        file.as_ref(),
                        "query vector",
                    )?,
                    k,
                    filter: command_filter,
                    as_of,
                }
            } else {
                Command::VectorQuery {
                    branch: scope.branch.clone(),
                    space: scope.space.clone(),
                    collection,
                    query: parse_query_vector_argument(
                        query.as_deref(),
                        file.as_ref(),
                        "query vector",
                    )?,
                    k,
                    filter: command_filter,
                    as_of,
                }
            }
        }
        VectorCommand::Count { collection } => Command::VectorCount {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            collection,
            as_of: None,
        },
        VectorCommand::Sample { collection, count } => Command::VectorSample {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            collection,
            count,
        },
    })
}

fn vector_collection_command(command: VectorCollectionCommand, scope: &Scope) -> Command {
    match command {
        VectorCollectionCommand::Create {
            collection,
            dimension,
            metric,
        } => Command::VectorCreateCollection {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            collection,
            dimension,
            metric: metric.into(),
        },
        VectorCollectionCommand::Delete { collection } => Command::VectorDeleteCollection {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            collection,
        },
        VectorCollectionCommand::List => Command::VectorListCollections {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
        },
        VectorCollectionCommand::Stats { collection } => Command::VectorCollectionStats {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            collection,
        },
    }
}

fn event_command(command: EventCommand, scope: &Scope) -> Result<Command, CliError> {
    Ok(match command {
        EventCommand::Append {
            event_type,
            payload,
            file,
        } => Command::EventAppend {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            event_type,
            payload: parse_json_argument(payload.as_deref(), file.as_ref(), "event payload")?,
        },
        EventCommand::Get { sequence, as_of } => Command::EventGet {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            sequence,
            as_of,
        },
        EventCommand::Exists { sequence } => Command::EventExists {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            sequence,
        },
        EventCommand::Count { as_of } => Command::EventCount {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            as_of,
        },
        EventCommand::List {
            event_type,
            limit,
            after_sequence,
            as_of,
        } => Command::EventList {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            event_type,
            limit,
            after_sequence,
            as_of,
        },
        EventCommand::Types { as_of } => Command::EventListTypes {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            as_of,
        },
        EventCommand::ByType {
            event_type,
            limit,
            after_sequence,
            as_of,
        } => Command::EventList {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            event_type: Some(event_type),
            limit,
            after_sequence,
            as_of,
        },
        EventCommand::Range {
            start_seq,
            end_seq,
            limit,
            direction,
            event_type,
        } => Command::EventRange {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            start_seq,
            end_seq,
            limit,
            direction: direction.into(),
            event_type,
        },
        EventCommand::RangeTime {
            start_ts,
            end_ts,
            limit,
            direction,
            event_type,
        } => Command::EventRangeByTime {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            start_ts,
            end_ts,
            limit,
            direction: direction.into(),
            event_type,
        },
        EventCommand::VerifyChain => Command::EventVerifyChain {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
        },
    })
}

#[allow(clippy::too_many_lines)]
fn graph_command(command: GraphCommand, scope: &Scope) -> Result<Command, CliError> {
    Ok(match command {
        GraphCommand::Create { graph } => Command::GraphCreate {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
        },
        GraphCommand::Delete { graph } => Command::GraphDelete {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
        },
        GraphCommand::List {
            cursor,
            limit,
            as_of,
        } => Command::GraphList {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            cursor,
            limit,
            as_of,
        },
        GraphCommand::Meta { graph, as_of } => Command::GraphGetMeta {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            as_of,
        },
        GraphCommand::AddNode {
            graph,
            node_id,
            properties,
            properties_file,
            object_type,
        } => Command::GraphAddNode {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            node_id,
            properties: parse_optional_json_argument(
                properties.as_deref(),
                properties_file.as_ref(),
                "node properties",
            )?,
            binding: None,
            object_type,
        },
        GraphCommand::GetNode {
            graph,
            node_id,
            as_of,
        } => Command::GraphGetNode {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            node_id,
            as_of,
        },
        GraphCommand::RemoveNode { graph, node_id } => Command::GraphRemoveNode {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            node_id,
        },
        GraphCommand::ListNodes {
            graph,
            prefix,
            cursor,
            limit,
            as_of,
        } => Command::GraphListNodes {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            prefix,
            cursor,
            limit,
            as_of,
        },
        GraphCommand::Sample { graph, count } => Command::GraphSample {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            count,
        },
        GraphCommand::AddEdge {
            graph,
            src,
            edge_type,
            dst,
            weight,
            properties,
            properties_file,
        } => Command::GraphAddEdge {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            src,
            edge_type,
            dst,
            weight,
            properties: parse_optional_json_argument(
                properties.as_deref(),
                properties_file.as_ref(),
                "edge properties",
            )?,
        },
        GraphCommand::GetEdge {
            graph,
            src,
            edge_type,
            dst,
            as_of,
        } => Command::GraphGetEdge {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            src,
            edge_type,
            dst,
            as_of,
        },
        GraphCommand::RemoveEdge {
            graph,
            src,
            edge_type,
            dst,
        } => Command::GraphRemoveEdge {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            src,
            edge_type,
            dst,
        },
        GraphCommand::Neighbors {
            graph,
            node_id,
            direction,
            edge_type,
            cursor,
            limit,
            as_of,
        } => Command::GraphNeighbors {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            node_id,
            direction: direction.into(),
            edge_type,
            cursor,
            limit,
            as_of,
        },
        GraphCommand::NodesByType {
            graph,
            object_type,
            cursor,
            limit,
            as_of,
        } => Command::GraphNodesByType {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            object_type,
            cursor,
            limit,
            as_of,
        },
        GraphCommand::Ontology(args) => graph_ontology_command(args.command, scope)?,
        GraphCommand::Wcc { graph, as_of } => Command::GraphWcc {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            budget: None,
            as_of,
        },
        GraphCommand::Lcc { graph, as_of } => Command::GraphLcc {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            budget: None,
            as_of,
        },
        GraphCommand::Sssp {
            graph,
            source,
            direction,
            as_of,
        } => Command::GraphSssp {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            source,
            direction: Some(direction.into()),
            budget: None,
            as_of,
        },
        GraphCommand::Pagerank {
            graph,
            damping,
            max_iterations,
            tolerance,
            personalization,
            as_of,
        } => Command::GraphPagerank {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            damping,
            max_iterations,
            tolerance,
            personalization: personalization
                .as_deref()
                .map(parse_personalization)
                .transpose()?,
            budget: None,
            as_of,
        },
        GraphCommand::Cdlp {
            graph,
            max_iterations,
            direction,
            as_of,
        } => Command::GraphCdlp {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            max_iterations,
            direction: Some(direction.into()),
            budget: None,
            as_of,
        },
        GraphCommand::BulkInsert {
            graph,
            data,
            file,
            chunk_size,
        } => {
            let payload =
                parse_json_argument(data.as_deref(), file.as_ref(), "bulk-insert payload")?;
            let payload: BulkInsertPayload = serde_json::from_value(payload).map_err(|error| {
                CliError::usage(format!(
                    "bulk-insert payload must be {{\"nodes\": [...], \"edges\": [...]}}: {error}"
                ))
            })?;
            Command::GraphBulkInsert {
                branch: scope.branch.clone(),
                space: scope.space.clone(),
                graph,
                nodes: payload.nodes,
                edges: payload.edges,
                chunk_size,
            }
        }
        GraphCommand::Bfs {
            graph,
            start,
            max_depth,
            max_nodes,
            edge_types,
            direction,
            as_of,
        } => Command::GraphBfs {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            start,
            max_depth,
            max_nodes,
            edge_types: if edge_types.is_empty() {
                None
            } else {
                Some(edge_types)
            },
            direction: Some(direction.into()),
            budget: None,
            as_of,
        },
    })
}

#[derive(serde::Deserialize)]
struct BulkInsertPayload {
    #[serde(default)]
    nodes: Vec<strata_executor::GraphBulkNode>,
    #[serde(default)]
    edges: Vec<strata_executor::GraphBulkEdge>,
}

fn parse_personalization(raw: &str) -> Result<std::collections::BTreeMap<String, f64>, CliError> {
    serde_json::from_str(raw).map_err(|error| {
        CliError::usage(format!(
            "personalization must be a JSON object mapping node ids to weights: {error}"
        ))
    })
}

fn graph_ontology_command(
    command: GraphOntologyCommand,
    scope: &Scope,
) -> Result<Command, CliError> {
    Ok(match command {
        GraphOntologyCommand::DefineObjectType {
            graph,
            name,
            properties,
            properties_file,
        } => Command::GraphDefineObjectType {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            name,
            properties: parse_type_properties(properties.as_deref(), properties_file.as_ref())?,
        },
        GraphOntologyCommand::DefineLinkType {
            graph,
            name,
            source,
            target,
            cardinality,
            properties,
            properties_file,
        } => Command::GraphDefineLinkType {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            name,
            source,
            target,
            cardinality,
            properties: parse_type_properties(properties.as_deref(), properties_file.as_ref())?,
        },
        GraphOntologyCommand::DeleteObjectType { graph, name } => Command::GraphDeleteObjectType {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            name,
        },
        GraphOntologyCommand::DeleteLinkType { graph, name } => Command::GraphDeleteLinkType {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            name,
        },
        GraphOntologyCommand::Freeze { graph } => Command::GraphFreezeOntology {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
        },
        GraphOntologyCommand::Get { graph, as_of } => Command::GraphGetOntology {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            as_of,
        },
        GraphOntologyCommand::Summary { graph, as_of } => Command::GraphOntologySummary {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            as_of,
        },
    })
}

/// Parses the ontology type-properties JSON argument into the wire map.
fn parse_type_properties(
    properties: Option<&str>,
    properties_file: Option<&std::path::PathBuf>,
) -> Result<std::collections::BTreeMap<String, GraphPropertyDef>, CliError> {
    let Some(value) = parse_optional_json_argument(properties, properties_file, "type properties")?
    else {
        return Ok(std::collections::BTreeMap::new());
    };
    serde_json::from_value(value).map_err(|error| {
        CliError::usage(format!(
            "type properties must map names to {{value_type, required}}: {error}"
        ))
    })
}

fn arrow_command(command: ArrowCommand, scope: &Scope) -> Command {
    match command {
        ArrowCommand::Import {
            file_path,
            format,
            target,
            key_column,
            value_column,
            collection,
        } => Command::ArrowImport {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            file_path,
            format: format.map(Into::into),
            target: target.into(),
            key_column,
            value_column,
            collection,
        },
        ArrowCommand::Export {
            primitive,
            format,
            path,
            prefix,
            limit,
            collection,
            graph,
            event_type,
        } => Command::ArrowExport {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            primitive: primitive.into(),
            format: format.into(),
            path,
            prefix,
            limit,
            collection,
            graph,
            event_type,
        },
    }
}

/// Builds executor commands for the inference family. Inference is
/// database-independent (models are process state), so no scope is injected.
#[cfg(feature = "inference")]
#[allow(
    clippy::too_many_lines,
    reason = "one flat match over the inference verbs; the Generate arm assembles the chat body inline"
)]
fn inference_command(command: options::InferenceCommand) -> Result<Command, CliError> {
    use options::{InferenceCommand as Inf, InferenceModelsCommand as Models};
    Ok(match command {
        Inf::Models(args) => match args.command {
            Models::List => Command::InferenceModelsList,
            Models::Local => Command::InferenceModelsLocal,
            Models::Pull { model } => Command::InferenceModelsPull { model },
        },
        Inf::Capability { model } => Command::InferenceModelCapability { model },
        Inf::Generate {
            model,
            prompt,
            system,
            messages,
            messages_json,
            json_body,
            max_tokens,
            temperature,
            top_k,
            top_p,
            min_p,
            repeat_penalty,
            frequency_penalty,
            presence_penalty,
            seed,
            stop_sequences,
            stop_tokens,
            response_format,
            grammar,
            n_ctx,
            n_gpu_layers,
            chat_format,
            tools_json,
            tool_choice,
            response_schema,
            response_schema_name,
            logprobs,
            top_logprobs,
        } => {
            let request = if let Some(body) = json_body {
                serde_json::from_str::<strata_executor::InferenceChatRequest>(&body)
                    .map_err(|error| CliError::usage(format!("invalid --json-body: {error}")))?
            } else {
                let mut turns: Vec<strata_executor::InferenceChatMessage> = Vec::new();
                if let Some(system) = system {
                    turns.push(strata_executor::InferenceChatMessage::new(
                        strata_executor::InferenceRole::System,
                        system,
                    ));
                }
                for spec in &messages {
                    turns.push(parse_chat_message(spec)?);
                }
                if let Some(json) = messages_json {
                    let parsed: Vec<strata_executor::InferenceChatMessage> =
                        serde_json::from_str(&json).map_err(|error| {
                            CliError::usage(format!("invalid --messages-json: {error}"))
                        })?;
                    turns.extend(parsed);
                }

                let mut request = strata_executor::InferenceChatRequest::default();
                if turns.is_empty() {
                    match prompt {
                        Some(prompt) => request.prompt = Some(prompt),
                        None => {
                            return Err(CliError::usage(
                                "provide a prompt, --system/--message, or --json-body".to_owned(),
                            ))
                        }
                    }
                } else {
                    if let Some(prompt) = prompt {
                        turns.push(strata_executor::InferenceChatMessage::new(
                            strata_executor::InferenceRole::User,
                            prompt,
                        ));
                    }
                    request.messages = Some(turns);
                }

                request.max_tokens = max_tokens;
                request.temperature = temperature;
                request.top_p = top_p;
                request.top_k = top_k;
                request.min_p = min_p;
                request.repeat_penalty = repeat_penalty;
                request.frequency_penalty = frequency_penalty;
                request.presence_penalty = presence_penalty;
                request.seed = seed;
                request.stop = (!stop_sequences.is_empty()).then_some(stop_sequences);
                request.stop_token_ids = (!stop_tokens.is_empty()).then_some(stop_tokens);
                request.grammar = grammar;
                request.response_format = response_format.map(|format| match format {
                    options::ResponseFormatArg::Text => {
                        strata_executor::InferenceResponseFormat::Text
                    }
                    options::ResponseFormatArg::JsonObject => {
                        strata_executor::InferenceResponseFormat::JsonObject
                    }
                });
                // --response-schema takes precedence over --response-format.
                if let Some(schema_json) = response_schema {
                    let schema: serde_json::Value = serde_json::from_str(&schema_json)
                        .map_err(|e| CliError::usage(format!("invalid --response-schema: {e}")))?;
                    request.response_format =
                        Some(strata_executor::InferenceResponseFormat::JsonSchema {
                            json_schema: strata_executor::InferenceJsonSchemaSpec {
                                name: response_schema_name.unwrap_or_else(|| "response".to_owned()),
                                description: None,
                                schema,
                                strict: None,
                            },
                        });
                }
                if let Some(tools_json) = tools_json {
                    let tools: Vec<strata_executor::InferenceTool> =
                        serde_json::from_str(&tools_json)
                            .map_err(|e| CliError::usage(format!("invalid --tools-json: {e}")))?;
                    request.tools = (!tools.is_empty()).then_some(tools);
                }
                if let Some(choice) = tool_choice {
                    request.tool_choice = Some(parse_tool_choice(&choice));
                }
                if logprobs || top_logprobs.is_some() {
                    request.logprobs = Some(true);
                    request.top_logprobs = top_logprobs;
                }
                if n_ctx.is_some() || n_gpu_layers.is_some() || chat_format.is_some() {
                    request.model_config = Some(strata_executor::InferenceModelConfig {
                        n_ctx,
                        n_gpu_layers,
                        chat_format,
                        ..Default::default()
                    });
                }
                request
            };
            Command::InferenceGenerate { model, request }
        }
        Inf::Tokenize {
            model,
            text,
            special,
        } => Command::InferenceTokenize {
            model,
            text,
            add_special: special,
        },
        Inf::Detokenize { model, ids } => Command::InferenceDetokenize { model, ids },
        Inf::Embed {
            model,
            inputs,
            dimensions,
            normalize,
            input_type,
        } => {
            let input = if inputs.len() == 1 {
                strata_executor::InferenceEmbedInput::One(
                    inputs.into_iter().next().expect("one input present"),
                )
            } else {
                strata_executor::InferenceEmbedInput::Many(inputs)
            };
            Command::InferenceEmbed {
                model,
                request: strata_executor::InferenceEmbeddingsRequest {
                    input,
                    dimensions,
                    normalize: normalize.then_some(true),
                    input_type: input_type.map(|kind| match kind {
                        options::InputTypeArg::Query => strata_executor::InferenceInputType::Query,
                        options::InputTypeArg::Document => {
                            strata_executor::InferenceInputType::Document
                        }
                    }),
                    instruction: None,
                },
            }
        }
        Inf::Rank {
            model,
            query,
            passages,
        } => Command::InferenceRank {
            model,
            request: strata_executor::InferenceRankRequest { query, passages },
        },
        Inf::Unload { model } => Command::InferenceUnload { model },
        Inf::CacheStatus => Command::InferenceCacheStatus,
    })
}

/// Parses a `--message "role:content"` spec into a chat message.
#[cfg(feature = "inference")]
fn parse_chat_message(spec: &str) -> Result<strata_executor::InferenceChatMessage, CliError> {
    let (role, content) = spec.split_once(':').ok_or_else(|| {
        CliError::usage(format!("--message must be \"role:content\", got {spec:?}"))
    })?;
    let role = match role.trim() {
        "system" => strata_executor::InferenceRole::System,
        "user" => strata_executor::InferenceRole::User,
        "assistant" => strata_executor::InferenceRole::Assistant,
        "tool" => strata_executor::InferenceRole::Tool,
        other => {
            return Err(CliError::usage(format!(
                "unknown message role {other:?} (expected system|user|assistant|tool)"
            )))
        }
    };
    Ok(strata_executor::InferenceChatMessage::new(
        role,
        content.to_owned(),
    ))
}

/// Parse a `--tool-choice` value: `auto` | `none` | `required` (case-insensitive)
/// select a mode; anything else names a function to force.
#[cfg(feature = "inference")]
fn parse_tool_choice(value: &str) -> strata_executor::InferenceToolChoice {
    use strata_executor::{
        InferenceNamedToolChoice, InferenceToolChoice, InferenceToolChoiceFunction,
        InferenceToolChoiceMode,
    };
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => InferenceToolChoice::Mode(InferenceToolChoiceMode::Auto),
        "none" => InferenceToolChoice::Mode(InferenceToolChoiceMode::None),
        "required" => InferenceToolChoice::Mode(InferenceToolChoiceMode::Required),
        _ => InferenceToolChoice::Named(InferenceNamedToolChoice::Function {
            function: InferenceToolChoiceFunction {
                name: value.trim().to_owned(),
            },
        }),
    }
}

#[cfg(all(test, feature = "inference"))]
mod config_key_tests {
    use super::{is_user_config_key, provider_api_key_target, redact_key};

    #[test]
    fn redacts_to_a_non_secret_prefix() {
        // Real keys are long, so only a public-ish prefix is ever revealed.
        assert_eq!(
            redact_key("sk-ant-api03-averylongsecretvalue"),
            "sk-ant-****"
        );
        assert_eq!(redact_key("AIzaSyD-averylongsecretvalue"), "AIzaSyD****");
    }

    #[test]
    fn provider_api_key_targets_are_validated() {
        assert_eq!(provider_api_key_target("openai.api_key"), Some("openai"));
        assert_eq!(
            provider_api_key_target("anthropic.api_key"),
            Some("anthropic")
        );
        assert_eq!(provider_api_key_target("google.api_key"), Some("google"));
        assert_eq!(provider_api_key_target("bogus.api_key"), None);
        assert_eq!(provider_api_key_target("openai.base_url"), None);
    }

    #[test]
    fn user_config_keys_recognized() {
        assert!(is_user_config_key("hub.url"));
        assert!(is_user_config_key("openai.api_key"));
        assert!(!is_user_config_key("nonsense"));
    }
}

#[cfg(all(test, feature = "inference"))]
mod inference_command_tests {
    use super::{inference_command, options, parse_chat_message};
    use serde_json::json;

    fn generate(
        prompt: Option<&str>,
        system: Option<&str>,
        messages: Vec<&str>,
        json_body: Option<&str>,
    ) -> options::InferenceCommand {
        options::InferenceCommand::Generate {
            model: "m".to_owned(),
            prompt: prompt.map(str::to_owned),
            system: system.map(str::to_owned),
            messages: messages.into_iter().map(str::to_owned).collect(),
            messages_json: None,
            json_body: json_body.map(str::to_owned),
            max_tokens: None,
            temperature: None,
            top_k: None,
            top_p: None,
            min_p: None,
            repeat_penalty: None,
            frequency_penalty: None,
            presence_penalty: None,
            seed: None,
            stop_sequences: vec![],
            stop_tokens: vec![],
            response_format: None,
            grammar: None,
            n_ctx: None,
            n_gpu_layers: None,
            chat_format: None,
            tools_json: None,
            tool_choice: None,
            response_schema: None,
            response_schema_name: None,
            logprobs: false,
            top_logprobs: None,
        }
    }

    fn body(command: options::InferenceCommand) -> serde_json::Value {
        let command = inference_command(command).expect("builds");
        serde_json::to_value(&command).expect("serializes")["request"].clone()
    }

    #[test]
    fn system_plus_prompt_becomes_messages() {
        let req = body(generate(Some("hello"), Some("be terse"), vec![], None));
        assert_eq!(
            req["messages"][0],
            json!({"role": "system", "content": "be terse"})
        );
        assert_eq!(
            req["messages"][1],
            json!({"role": "user", "content": "hello"})
        );
        assert!(req.get("prompt").is_none());
    }

    #[test]
    fn prompt_only_is_raw_completion() {
        let req = body(generate(Some("once upon"), None, vec![], None));
        assert_eq!(req["prompt"], "once upon");
        assert!(req.get("messages").is_none());
    }

    #[test]
    fn json_body_overrides_everything() {
        let req = body(generate(
            None,
            None,
            vec![],
            Some(r#"{"messages":[{"role":"user","content":"hi"}],"max_tokens":5}"#),
        ));
        assert_eq!(req["max_tokens"], 5);
        assert_eq!(req["messages"][0]["content"], "hi");
    }

    #[test]
    fn no_input_is_a_usage_error() {
        let err = inference_command(generate(None, None, vec![], None)).unwrap_err();
        assert!(err.to_string().contains("prompt"), "got: {err}");
    }

    fn generate_with(f: impl FnOnce(&mut options::InferenceCommand)) -> options::InferenceCommand {
        let mut cmd = generate(Some("hi"), None, vec![], None);
        f(&mut cmd);
        cmd
    }

    #[test]
    fn tools_and_tool_choice_are_built() {
        let cmd = generate_with(|c| {
            if let options::InferenceCommand::Generate {
                tools_json,
                tool_choice,
                ..
            } = c
            {
                *tools_json = Some(
                    r#"[{"type":"function","function":{"name":"get_weather","parameters":{"type":"object"}}}]"#
                        .to_owned(),
                );
                *tool_choice = Some("required".to_owned());
            }
        });
        let req = body(cmd);
        assert_eq!(req["tools"][0]["type"], "function");
        assert_eq!(req["tools"][0]["function"]["name"], "get_weather");
        assert_eq!(req["tool_choice"], "required");
    }

    #[test]
    fn named_tool_choice_forces_function() {
        let cmd = generate_with(|c| {
            if let options::InferenceCommand::Generate { tool_choice, .. } = c {
                *tool_choice = Some("get_weather".to_owned());
            }
        });
        let req = body(cmd);
        assert_eq!(req["tool_choice"]["type"], "function");
        assert_eq!(req["tool_choice"]["function"]["name"], "get_weather");
    }

    #[test]
    fn response_schema_sets_json_schema_format() {
        let cmd = generate_with(|c| {
            if let options::InferenceCommand::Generate {
                response_schema,
                response_schema_name,
                ..
            } = c
            {
                *response_schema = Some(r#"{"type":"object"}"#.to_owned());
                *response_schema_name = Some("person".to_owned());
            }
        });
        let req = body(cmd);
        assert_eq!(req["response_format"]["type"], "json_schema");
        assert_eq!(req["response_format"]["json_schema"]["name"], "person");
        assert_eq!(
            req["response_format"]["json_schema"]["schema"]["type"],
            "object"
        );
    }

    #[test]
    fn logprobs_flag_is_built() {
        let cmd = generate_with(|c| {
            if let options::InferenceCommand::Generate {
                logprobs,
                top_logprobs,
                ..
            } = c
            {
                *logprobs = true;
                *top_logprobs = Some(3);
            }
        });
        let req = body(cmd);
        assert_eq!(req["logprobs"], true);
        assert_eq!(req["top_logprobs"], 3);
    }

    #[test]
    fn message_roles_parse_and_reject() {
        assert!(parse_chat_message("user:hi").is_ok());
        assert!(parse_chat_message("assistant:sure").is_ok());
        assert!(parse_chat_message("no-colon").is_err());
        assert!(parse_chat_message("wizard:hi").is_err());
    }

    #[test]
    fn embed_single_and_batch() {
        let one = body(options::InferenceCommand::Embed {
            model: "m".to_owned(),
            inputs: vec!["a".to_owned()],
            dimensions: None,
            normalize: false,
            input_type: None,
        });
        assert_eq!(one["input"], "a");

        let many = body(options::InferenceCommand::Embed {
            model: "m".to_owned(),
            inputs: vec!["a".to_owned(), "b".to_owned()],
            dimensions: Some(256),
            normalize: true,
            input_type: Some(options::InputTypeArg::Query),
        });
        assert_eq!(many["input"], json!(["a", "b"]));
        assert_eq!(many["dimensions"], 256);
        assert_eq!(many["normalize"], true);
        assert_eq!(many["input_type"], "query");
    }
}

fn raw_command(command: CommandCommand) -> Result<Command, CliError> {
    match command {
        CommandCommand::Run { json, file } => {
            raw_command_from_sources(json.as_deref(), file.as_ref())
        }
        CommandCommand::Print { .. } => Err(CliError::usage(
            "`command print` validates a command without executing it and is handled before open",
        )),
    }
}

fn raw_command_from_sources(
    json: Option<&str>,
    file: Option<&std::path::PathBuf>,
) -> Result<Command, CliError> {
    let text = match (json, file) {
        (Some(_), Some(_)) => {
            return Err(CliError::usage(
                "provide either `--command-json <json>` or `--file <path>`, not both",
            ));
        }
        (Some(json), None) => json.to_owned(),
        (None, Some(path)) => input::read_text_file(path)?,
        (None, None) => {
            return Err(CliError::usage(
                "raw command execution requires `--command-json <json>` or `--file <path>`",
            ));
        }
    };
    Ok(serde_json::from_str(&text)?)
}

fn bytes(value: String) -> strata_executor::Bytes {
    strata_executor::Bytes::new(value.into_bytes())
}

/// CLI error.
#[derive(Debug)]
enum CliError {
    Usage(String),
    Io(std::io::Error),
    Json(serde_json::Error),
    Executor(Box<ExecutorError>),
}

impl CliError {
    fn usage(message: impl Into<String>) -> Self {
        Self::Usage(message.into())
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(message) => formatter.write_str(message),
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Json(error) => write!(formatter, "{error}"),
            Self::Executor(error) => write!(formatter, "{}", error.message()),
        }
    }
}

impl std::error::Error for CliError {}

impl From<std::io::Error> for CliError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for CliError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<ExecutorError> for CliError {
    fn from(value: ExecutorError) -> Self {
        Self::Executor(Box::new(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use options::{
        ArrowCommand, CliArrowExportPrimitive, CliArrowFormat, CommandCommand, KvCommand,
        TopCommand,
    };
    use std::path::PathBuf;

    #[test]
    fn parses_direct_database_path_before_subcommand() {
        let cli = Cli::parse_from(["strata", "./db", "kv", "put", "hello", "world"]);
        assert_eq!(cli.db_path, Some(PathBuf::from("./db")));
        assert!(matches!(
            cli.command,
            Some(TopCommand::Kv(options::KvArgs {
                command: KvCommand::Put { key, value: Some(value), file: None },
            })) if key == "hello" && value == "world"
        ));
    }

    #[test]
    fn parses_db_flag_and_scope() {
        let cli = Cli::parse_from([
            "strata", "--db", "./db", "--branch", "feature", "--space", "app", "kv", "get", "hello",
        ]);
        assert_eq!(cli.db, Some(PathBuf::from("./db")));
        assert_eq!(cli.branch.as_deref(), Some("feature"));
        assert_eq!(cli.space.as_deref(), Some("app"));
    }

    #[test]
    fn parses_no_command_for_shell_mode() {
        let cli = Cli::parse_from(["strata", "--cache"]);
        assert!(cli.cache);
        assert!(cli.command.is_none());
    }

    #[test]
    fn parses_output_flags() {
        let cli = Cli::parse_from(["strata", "--json", "--cache", "ping"]);
        assert_eq!(cli.output_format(), options::Format::Json);

        let cli = Cli::parse_from(["strata", "--raw", "--cache", "ping"]);
        assert_eq!(cli.output_format(), options::Format::Raw);
    }

    #[test]
    fn parses_arrow_file_format_without_colliding_with_output_format() {
        let cli = Cli::parse_from([
            "strata",
            "--json",
            "--cache",
            "arrow",
            "export",
            "--primitive",
            "kv",
            "--format",
            "jsonl",
            "out.jsonl",
        ]);
        assert_eq!(cli.output_format(), options::Format::Json);
        assert!(matches!(
            cli.command,
            Some(TopCommand::Arrow(options::ArrowArgs {
                command: ArrowCommand::Export {
                    primitive: CliArrowExportPrimitive::Kv,
                    format: CliArrowFormat::Jsonl,
                    path,
                    ..
                },
            })) if path == "out.jsonl"
        ));
    }

    #[test]
    fn parses_raw_command_json_without_colliding_with_output_json() {
        let cli = Cli::parse_from([
            "strata",
            "--json",
            "--cache",
            "command",
            "run",
            "--command-json",
            r#"{"type":"ping"}"#,
        ]);
        assert_eq!(cli.output_format(), options::Format::Json);
        assert!(matches!(
            cli.command,
            Some(TopCommand::Command(options::CommandArgs {
                command: CommandCommand::Run {
                    json: Some(json),
                    file: None,
                },
            })) if json == r#"{"type":"ping"}"#
        ));
    }

    #[test]
    fn parses_delete_alias() {
        let cli = Cli::parse_from(["strata", "--cache", "kv", "del", "hello"]);
        assert!(matches!(
            cli.command,
            Some(TopCommand::Kv(options::KvArgs {
                command: KvCommand::Delete { key },
            })) if key == "hello"
        ));
    }

    #[test]
    fn kv_put_reads_file_value() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let path = temp.path().join("value.bin");
        std::fs::write(&path, b"from-file").expect("write value");
        let command = kv_command(
            KvCommand::Put {
                key: "hello".to_owned(),
                value: None,
                file: Some(path),
            },
            &Scope::default(),
        )
        .expect("kv command");

        let Command::KvPut { value, .. } = command else {
            panic!("expected kv put");
        };
        assert_eq!(value.as_slice(), b"from-file");
    }

    #[test]
    fn deferred_top_level_command_returns_usage_error() {
        assert_eq!(run(["strata", "--cache", "search"]), 2);
    }

    #[test]
    fn run_executes_durable_kv_round_trip() {
        let temp = tempfile::tempdir().expect("create temp dir");
        let db = temp.path().to_string_lossy().to_string();

        assert_eq!(
            run(["strata", "--db", db.as_str(), "kv", "put", "hello", "world"]),
            0
        );
        assert_eq!(
            run(["strata", "--db", db.as_str(), "kv", "get", "hello"]),
            0
        );
    }
}
