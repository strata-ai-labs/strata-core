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
    parse_optional_filter_argument, parse_optional_json_argument, parse_relaxed_json_argument,
    parse_vector_argument,
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
            executor.execute(inference_command(args.command))?
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
            ConfigCommand::GetKey { key } if key == "hub.url" => {
                Ok(TopLevelAction::NoDatabase(user_config_get()?))
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

/// `strata config set hub.url <url>` — writes the global user config.
fn user_config_set(key: &str, value: &str) -> Result<serde_json::Value, CliError> {
    require_hub_url_key(key)?;
    let path = strata_hub::write_global_hub_url(value)
        .map_err(|error| CliError::usage(error.to_string()))?;
    Ok(serde_json::json!({
        "key": key,
        "value": value,
        "path": path.display().to_string(),
    }))
}

fn user_config_unset(key: &str) -> Result<serde_json::Value, CliError> {
    require_hub_url_key(key)?;
    let path =
        strata_hub::unset_global_hub_url().map_err(|error| CliError::usage(error.to_string()))?;
    Ok(serde_json::json!({
        "key": key,
        "unset": true,
        "path": path.map(|path| path.display().to_string()),
    }))
}

fn user_config_get() -> Result<serde_json::Value, CliError> {
    let value =
        strata_hub::read_global_hub_url().map_err(|error| CliError::usage(error.to_string()))?;
    Ok(serde_json::json!({
        "key": "hub.url",
        "value": value.map(|url| url.to_string()),
    }))
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

fn require_hub_url_key(key: &str) -> Result<(), CliError> {
    if key == "hub.url" {
        return Ok(());
    }
    Err(CliError::usage(
        "only `hub.url` is a settable user-config key in V1",
    ))
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
        KvCommand::History { key } => Command::KvGetv {
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
        JsonCommand::History { key } => Command::JsonGetv {
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
        VectorCommand::History { collection, key } => Command::VectorGetv {
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
                    query: parse_vector_argument(query.as_deref(), file.as_ref(), "query vector")?,
                    k,
                    filter: command_filter,
                    as_of,
                }
            } else {
                Command::VectorQuery {
                    branch: scope.branch.clone(),
                    space: scope.space.clone(),
                    collection,
                    query: parse_vector_argument(query.as_deref(), file.as_ref(), "query vector")?,
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
        EventCommand::Len { as_of } => Command::EventLen {
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
fn inference_command(command: options::InferenceCommand) -> Command {
    use options::{InferenceCommand as Inf, InferenceModelsCommand as Models};
    match command {
        Inf::Models(args) => match args.command {
            Models::List => Command::InferenceModelsList,
            Models::Local => Command::InferenceModelsLocal,
            Models::Pull { model } => Command::InferenceModelsPull { model },
        },
        Inf::Capability { model } => Command::InferenceModelCapability { model },
        Inf::Generate {
            model,
            prompt,
            max_tokens,
            temperature,
            top_k,
            top_p,
            seed,
            stop_sequences,
            stop_tokens,
            grammar,
        } => {
            let defaults = strata_executor::InferenceGenerateRequest::default();
            Command::InferenceGenerate {
                model,
                request: strata_executor::InferenceGenerateRequest {
                    prompt,
                    max_tokens: max_tokens.unwrap_or(defaults.max_tokens),
                    temperature: temperature.unwrap_or(defaults.temperature),
                    top_k: top_k.unwrap_or(defaults.top_k),
                    top_p: top_p.unwrap_or(defaults.top_p),
                    seed,
                    stop_sequences,
                    stop_tokens,
                    grammar,
                },
            }
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
        Inf::Embed { model, text } => Command::InferenceEmbed {
            model,
            request: strata_executor::InferenceEmbedRequest { text },
        },
        Inf::EmbedBatch { model, texts } => Command::InferenceEmbedBatch { model, texts },
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
