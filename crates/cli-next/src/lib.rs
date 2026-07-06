//! Handwritten CLI layer over `strata-executor-next`.

#![deny(unsafe_code)]
#![allow(clippy::missing_const_for_fn)]

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use clap::Parser;
use serde::Serialize;
use serde_json::Value;
use strata_executor_next::{Bytes, Command, Executor, ExecutorError, Output, VectorMetadataFilter};

mod options;

use options::{
    ArrowCommand, BranchCommand, Cli, CommandCommand, ConfigCommand, EventCommand, Format,
    GraphCommand, JsonCommand, KvCommand, SpaceCommand, VectorCollectionCommand, VectorCommand,
};

/// Runs the CLI and returns a process exit code.
pub fn run<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    match Cli::try_parse_from(args) {
        Ok(cli) => {
            let format = cli.format;
            match execute(cli) {
                Ok(()) => 0,
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
            eprint!("{error}");
            2
        }
    }
}

fn execute(cli: Cli) -> Result<(), CliError> {
    let format = cli.format;
    let command = cli.command;
    let scope = Scope {
        branch: cli.branch,
        space: cli.space,
    };

    if let TopLevelAction::NoDatabase(output) = top_level_without_database(&command)? {
        render_output(&output, format)?;
        return Ok(());
    }

    let mut executor = open_executor(cli.cache, cli.db, cli.db_path)?;
    if let Some(branch) = scope.branch.as_deref() {
        executor = executor.with_default_branch(branch)?;
    }

    let output = match command {
        options::TopCommand::Ping => executor.execute(Command::Ping)?,
        options::TopCommand::Init | options::TopCommand::Info => {
            executor.execute(Command::Info {
                branch: scope.branch,
            })?
        }
        options::TopCommand::Health => executor.execute(Command::Health {
            branch: scope.branch,
        })?,
        options::TopCommand::Metrics => executor.execute(Command::Metrics {
            branch: scope.branch,
        })?,
        options::TopCommand::Describe => executor.execute(Command::Describe {
            branch: scope.branch,
        })?,
        options::TopCommand::Config(args) => executor.execute(config_command(args.command))?,
        options::TopCommand::Branch(args) => executor.execute(branch_command(args.command))?,
        options::TopCommand::Space(args) => {
            executor.execute(space_command(args.command, &scope))?
        }
        options::TopCommand::Kv(args) => executor.execute(kv_command(args.command, &scope))?,
        options::TopCommand::Json(args) => executor.execute(json_command(args.command, &scope))?,
        options::TopCommand::Vector(command) => {
            executor.execute(vector_command(command.command, &scope)?)?
        }
        options::TopCommand::Event(args) => {
            executor.execute(event_command(args.command, &scope)?)?
        }
        options::TopCommand::Graph(args) => {
            executor.execute(graph_command(args.command, &scope)?)?
        }
        options::TopCommand::Arrow(args) => {
            executor.execute(arrow_command(args.command, &scope))?
        }
        options::TopCommand::Command(args) => executor.execute(raw_command(args.command)?)?,
    };

    render_output(&output, format)?;
    executor.close()?;
    Ok(())
}

enum TopLevelAction {
    NeedsDatabase,
    NoDatabase(Box<Output>),
}

fn top_level_without_database(command: &options::TopCommand) -> Result<TopLevelAction, CliError> {
    match command {
        options::TopCommand::Command(args) => match &args.command {
            CommandCommand::Print { json, file } => {
                let command = raw_command_from_sources(json.as_deref(), file.as_ref())?;
                let value = serde_json::to_value(command)?;
                Ok(TopLevelAction::NoDatabase(Box::new(Output::JsonValue(
                    strata_executor_next::MaybeJsonValue::found(value),
                ))))
            }
            CommandCommand::Run { .. } => Ok(TopLevelAction::NeedsDatabase),
        },
        _ => Ok(TopLevelAction::NeedsDatabase),
    }
}

fn open_executor(
    cache: bool,
    db_flag: Option<PathBuf>,
    db_path: Option<PathBuf>,
) -> Result<Executor, CliError> {
    if cache {
        if db_flag.is_some() || db_path.is_some() {
            return Err(CliError::usage(
                "`--cache` cannot be combined with `--db` or a database path",
            ));
        }
        return Ok(Executor::open_cache()?);
    }
    let path = match (db_flag, db_path) {
        (Some(_), Some(_)) => {
            return Err(CliError::usage(
                "provide either `--db <path>` or positional database path, not both",
            ));
        }
        (Some(path), None) | (None, Some(path)) => path,
        (None, None) => std::env::current_dir()?,
    };
    Ok(Executor::open_durable_local(path)?)
}

#[derive(Clone, Debug)]
struct Scope {
    branch: Option<String>,
    space: Option<String>,
}

fn config_command(command: ConfigCommand) -> Command {
    match command {
        ConfigCommand::Get => Command::ConfigGet,
        ConfigCommand::GetKey { key } => Command::ConfigureGetKey { key },
    }
}

fn branch_command(command: BranchCommand) -> Command {
    match command {
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
    }
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

fn kv_command(command: KvCommand, scope: &Scope) -> Command {
    match command {
        KvCommand::Put { key, value } => Command::KvPut {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            key: bytes(key),
            value: bytes(value),
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
            cursor: cursor.map(bytes),
            limit,
            as_of,
        },
        KvCommand::Scan { start, limit } => Command::KvScan {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            start: start.map(bytes),
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
        },
        KvCommand::Sample { prefix, count } => Command::KvSample {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            prefix: prefix.map(bytes),
            count,
        },
    }
}

fn json_command(command: JsonCommand, scope: &Scope) -> Command {
    match command {
        JsonCommand::Set { key, path, value } => Command::JsonSet {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            key,
            path,
            value: parse_relaxed_json(&value),
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
    }
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
        } => Command::VectorUpsert {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            collection,
            key,
            vector: parse_vector(&vector)?,
            metadata: parse_optional_json(metadata.as_deref())?,
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
        },
        VectorCommand::UpdateMetadata {
            collection,
            key,
            patch,
        } => Command::VectorUpdateMetadata {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            collection,
            key,
            patch: parse_json(&patch)?,
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
        VectorCommand::DeleteByFilter { collection, filter } => Command::VectorDeleteByFilter {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            collection,
            filter: parse_filter(&filter)?,
        },
        VectorCommand::Query {
            collection,
            query,
            k,
            filter,
            as_of,
            diagnostics,
        } => {
            let command_filter = parse_optional_filter(filter.as_deref())?;
            if diagnostics {
                Command::VectorIndexQuery {
                    branch: scope.branch.clone(),
                    space: scope.space.clone(),
                    collection,
                    query: parse_vector(&query)?,
                    k,
                    filter: command_filter,
                    as_of,
                }
            } else {
                Command::VectorQuery {
                    branch: scope.branch.clone(),
                    space: scope.space.clone(),
                    collection,
                    query: parse_vector(&query)?,
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
        } => Command::EventAppend {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            event_type,
            payload: parse_json(&payload)?,
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
            as_of,
        } => Command::EventList {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            event_type,
            limit,
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
        } => Command::EventGetByType {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            event_type,
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
        GraphCommand::List { cursor, limit } => Command::GraphList {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            cursor,
            limit,
        },
        GraphCommand::Meta { graph } => Command::GraphGetMeta {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
        },
        GraphCommand::AddNode {
            graph,
            node_id,
            properties,
        } => Command::GraphAddNode {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            node_id,
            properties: parse_optional_json(properties.as_deref())?,
            binding: None,
        },
        GraphCommand::GetNode { graph, node_id } => Command::GraphGetNode {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            node_id,
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
        } => Command::GraphListNodes {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            prefix,
            cursor,
            limit,
        },
        GraphCommand::AddEdge {
            graph,
            src,
            edge_type,
            dst,
            weight,
            properties,
        } => Command::GraphAddEdge {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            src,
            edge_type,
            dst,
            weight,
            properties: parse_optional_json(properties.as_deref())?,
        },
        GraphCommand::GetEdge {
            graph,
            src,
            edge_type,
            dst,
        } => Command::GraphGetEdge {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            src,
            edge_type,
            dst,
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
        } => Command::GraphNeighbors {
            branch: scope.branch.clone(),
            space: scope.space.clone(),
            graph,
            node_id,
            direction: direction.into(),
            edge_type,
            cursor,
            limit,
        },
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
    file: Option<&PathBuf>,
) -> Result<Command, CliError> {
    let text = match (json, file) {
        (Some(_), Some(_)) => {
            return Err(CliError::usage(
                "provide either `--json <json>` or `--file <path>`, not both",
            ));
        }
        (Some(json), None) => json.to_owned(),
        (None, Some(path)) => fs::read_to_string(path)?,
        (None, None) => {
            return Err(CliError::usage(
                "raw command execution requires `--json <json>` or `--file <path>`",
            ));
        }
    };
    Ok(serde_json::from_str(&text)?)
}

fn bytes(value: String) -> Bytes {
    Bytes::new(value.into_bytes())
}

fn parse_relaxed_json(value: &str) -> Value {
    serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_owned()))
}

fn parse_json(value: &str) -> Result<Value, CliError> {
    serde_json::from_str(value).map_err(CliError::from)
}

fn parse_optional_json(value: Option<&str>) -> Result<Option<Value>, CliError> {
    value.map(parse_json).transpose()
}

fn parse_vector(value: &str) -> Result<Vec<f32>, CliError> {
    if value.trim_start().starts_with('[') {
        return serde_json::from_str(value).map_err(CliError::from);
    }
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<f32>().map_err(|error| {
                CliError::usage(format!("invalid vector element `{part}`: {error}"))
            })
        })
        .collect()
}

fn parse_filter(value: &str) -> Result<VectorMetadataFilter, CliError> {
    serde_json::from_str(value).map_err(CliError::from)
}

fn parse_optional_filter(value: Option<&str>) -> Result<Option<VectorMetadataFilter>, CliError> {
    value.map(parse_filter).transpose()
}

fn render_output(output: &Output, format: Format) -> Result<(), CliError> {
    match format {
        Format::Json => {
            println!("{}", serde_json::to_string(output)?);
        }
        Format::Pretty => {
            println!("{}", serde_json::to_string_pretty(output)?);
        }
    }
    Ok(())
}

fn render_error(status: &impl Serialize, format: Format) {
    #[derive(Serialize)]
    struct ErrorEnvelope<'a, T: Serialize + ?Sized> {
        error: &'a T,
    }

    let envelope = ErrorEnvelope { error: status };
    let rendered = match format {
        Format::Json => serde_json::to_string(&envelope),
        Format::Pretty => serde_json::to_string_pretty(&envelope),
    };
    match rendered {
        Ok(text) => eprintln!("{text}"),
        Err(error) => eprintln!("error: failed to render executor error: {error}"),
    }
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
    use options::{KvCommand, TopCommand};

    #[test]
    fn parses_direct_database_path_before_subcommand() {
        let cli = Cli::parse_from(["strata", "./db", "kv", "put", "hello", "world"]);
        assert_eq!(cli.db_path, Some(PathBuf::from("./db")));
        assert!(matches!(
            cli.command,
            TopCommand::Kv(options::KvArgs {
                command: KvCommand::Put { key, value },
            }) if key == "hello" && value == "world"
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
    fn parses_comma_vector() {
        assert_eq!(
            parse_vector("1, 2.5,3").expect("parse vector"),
            vec![1.0, 2.5, 3.0]
        );
    }

    #[test]
    fn parses_json_vector() {
        assert_eq!(
            parse_vector("[1,2,3]").expect("parse vector"),
            vec![1.0, 2.0, 3.0]
        );
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
