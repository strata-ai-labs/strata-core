use std::fmt::Write as _;
use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;
use strata_executor_next::{
    Command, Output, VectorDistanceMetric, VectorFilterCondition, VectorFilterOp, VectorMatch,
    VectorMetadataFilter, VectorScalar,
};

use crate::execution::{
    execute_durable, reject_unknown_flags, require_positional_len, strip_argument_delimiter,
    take_string, take_u64, CommandScope,
};
use crate::{json_output, CliError, OutputFormat};

pub(crate) fn run_vector(
    mut args: Vec<String>,
    format: OutputFormat,
    db: Option<PathBuf>,
) -> Result<String, CliError> {
    let Some(db) = db else {
        return Err(CliError::usage(
            "missing --db <path> for vector command".to_string(),
            format,
        ));
    };
    let command = parse_vector_command(&mut args, format)?;
    let output = execute_durable(db, command, format)?;
    Ok(render_output(&output, format))
}

fn parse_vector_command(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let Some(op) = args.first().cloned() else {
        return Err(CliError::usage(
            "missing vector operation".to_string(),
            format,
        ));
    };
    args.remove(0);
    match op.as_str() {
        "collection" => parse_collection(args, format),
        "upsert" => parse_upsert(args, format),
        "get" => parse_get(args, format),
        "history" => parse_history(args, format),
        "exists" => parse_exists(args, format),
        "keys" => parse_keys(args, format),
        "metadata" => parse_metadata(args, format),
        "delete" => parse_delete(args, format),
        "delete-all" => parse_delete_all(args, format),
        "delete-by-filter" => parse_delete_by_filter(args, format),
        "count" => parse_count(args, format),
        "query" => parse_query(args, format, false),
        "index" => parse_index(args, format),
        "batch-delete" | "batch-get" | "batch-upsert" => Err(CliError::usage(
            format!("vector operation `{op}` is not implemented by cli-next yet"),
            format,
        )),
        _ => Err(CliError::usage(
            format!("unknown vector operation `{op}`"),
            format,
        )),
    }
}

fn parse_collection(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let Some(op) = args.first().cloned() else {
        return Err(CliError::usage(
            "missing vector collection operation".to_string(),
            format,
        ));
    };
    args.remove(0);
    match op.as_str() {
        "create" => {
            let scope = CommandScope::extract(args, format)?;
            let dimension = take_required_u64(args, "--dimension", format)?;
            let metric = take_string(args, "--metric", format)?
                .as_deref()
                .map(|value| parse_metric(value, format))
                .transpose()?
                .unwrap_or_default();
            reject_unknown_flags(args, format)?;
            strip_argument_delimiter(args);
            require_positional_len(args, 1, "vector collection create <collection>", format)?;
            Ok(Command::VectorCreateCollection {
                branch: scope.branch,
                space: scope.space,
                collection: args[0].clone(),
                dimension,
                metric,
            })
        }
        "delete" => {
            let scope = CommandScope::extract(args, format)?;
            reject_unknown_flags(args, format)?;
            strip_argument_delimiter(args);
            require_positional_len(args, 1, "vector collection delete <collection>", format)?;
            Ok(Command::VectorDeleteCollection {
                branch: scope.branch,
                space: scope.space,
                collection: args[0].clone(),
            })
        }
        "list" => {
            let scope = CommandScope::extract(args, format)?;
            reject_unknown_flags(args, format)?;
            strip_argument_delimiter(args);
            require_positional_len(args, 0, "vector collection list", format)?;
            Ok(Command::VectorListCollections {
                branch: scope.branch,
                space: scope.space,
            })
        }
        "stats" => {
            let scope = CommandScope::extract(args, format)?;
            reject_unknown_flags(args, format)?;
            strip_argument_delimiter(args);
            require_positional_len(args, 1, "vector collection stats <collection>", format)?;
            Ok(Command::VectorCollectionStats {
                branch: scope.branch,
                space: scope.space,
                collection: args[0].clone(),
            })
        }
        _ => Err(CliError::usage(
            format!("unknown vector collection operation `{op}`"),
            format,
        )),
    }
}

fn parse_upsert(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    let vector = take_required_vector(args, "--vector", format)?;
    let metadata = take_optional_json(args, "--metadata", format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 2, "vector upsert <collection> <key>", format)?;
    Ok(Command::VectorUpsert {
        branch: scope.branch,
        space: scope.space,
        collection: args[0].clone(),
        key: args[1].clone(),
        vector,
        metadata,
    })
}

fn parse_get(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    let as_of = take_u64(args, "--as-of", format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 2, "vector get <collection> <key>", format)?;
    Ok(Command::VectorGet {
        branch: scope.branch,
        space: scope.space,
        collection: args[0].clone(),
        key: args[1].clone(),
        as_of,
    })
}

fn parse_history(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 2, "vector history <collection> <key>", format)?;
    Ok(Command::VectorGetv {
        branch: scope.branch,
        space: scope.space,
        collection: args[0].clone(),
        key: args[1].clone(),
    })
}

fn parse_exists(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 2, "vector exists <collection> <key>", format)?;
    Ok(Command::VectorExists {
        branch: scope.branch,
        space: scope.space,
        collection: args[0].clone(),
        key: args[1].clone(),
    })
}

fn parse_keys(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    let prefix = take_string(args, "--prefix", format)?;
    let cursor = take_string(args, "--cursor", format)?;
    let limit = take_u64(args, "--limit", format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 1, "vector keys <collection>", format)?;
    Ok(Command::VectorListKeys {
        branch: scope.branch,
        space: scope.space,
        collection: args[0].clone(),
        prefix,
        cursor,
        limit,
    })
}

fn parse_metadata(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let Some(op) = args.first().cloned() else {
        return Err(CliError::usage(
            "missing vector metadata operation".to_string(),
            format,
        ));
    };
    args.remove(0);
    if op != "update" {
        return Err(CliError::usage(
            format!("unknown vector metadata operation `{op}`"),
            format,
        ));
    }
    let scope = CommandScope::extract(args, format)?;
    let patch = take_required_json(args, "--patch", format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 2, "vector metadata update <collection> <key>", format)?;
    Ok(Command::VectorUpdateMetadata {
        branch: scope.branch,
        space: scope.space,
        collection: args[0].clone(),
        key: args[1].clone(),
        patch,
    })
}

fn parse_delete(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 2, "vector delete <collection> <key>", format)?;
    Ok(Command::VectorDelete {
        branch: scope.branch,
        space: scope.space,
        collection: args[0].clone(),
        key: args[1].clone(),
    })
}

fn parse_delete_all(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 1, "vector delete-all <collection>", format)?;
    Ok(Command::VectorDeleteAll {
        branch: scope.branch,
        space: scope.space,
        collection: args[0].clone(),
    })
}

fn parse_delete_by_filter(
    args: &mut Vec<String>,
    format: OutputFormat,
) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    let filter = take_required_filter(args, "--filter", format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 1, "vector delete-by-filter <collection>", format)?;
    Ok(Command::VectorDeleteByFilter {
        branch: scope.branch,
        space: scope.space,
        collection: args[0].clone(),
        filter,
    })
}

fn parse_count(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 1, "vector count <collection>", format)?;
    Ok(Command::VectorCount {
        branch: scope.branch,
        space: scope.space,
        collection: args[0].clone(),
    })
}

fn parse_query(
    args: &mut Vec<String>,
    format: OutputFormat,
    include_diagnostics: bool,
) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    let query = take_query_vector(args, format)?;
    let k = take_required_u64(args, "--k", format)?;
    let filter = take_optional_filter(args, "--filter", format)?;
    let as_of = take_u64(args, "--as-of", format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 1, "vector query <collection>", format)?;
    if include_diagnostics {
        Ok(Command::VectorIndexQuery {
            branch: scope.branch,
            space: scope.space,
            collection: args[0].clone(),
            query,
            k,
            filter,
            as_of,
        })
    } else {
        Ok(Command::VectorQuery {
            branch: scope.branch,
            space: scope.space,
            collection: args[0].clone(),
            query,
            k,
            filter,
            as_of,
        })
    }
}

fn parse_index(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let Some(op) = args.first().cloned() else {
        return Err(CliError::usage(
            "missing vector index operation".to_string(),
            format,
        ));
    };
    args.remove(0);
    if op == "query" {
        parse_query(args, format, true)
    } else {
        Err(CliError::usage(
            format!("unknown vector index operation `{op}`"),
            format,
        ))
    }
}

fn take_required_string(
    args: &mut Vec<String>,
    flag: &'static str,
    format: OutputFormat,
) -> Result<String, CliError> {
    take_string(args, flag, format)?
        .ok_or_else(|| CliError::usage(format!("missing {flag}"), format))
}

fn take_required_u64(
    args: &mut Vec<String>,
    flag: &'static str,
    format: OutputFormat,
) -> Result<u64, CliError> {
    take_u64(args, flag, format)?.ok_or_else(|| CliError::usage(format!("missing {flag}"), format))
}

fn take_required_vector(
    args: &mut Vec<String>,
    flag: &'static str,
    format: OutputFormat,
) -> Result<Vec<f32>, CliError> {
    parse_vector_literal(&take_required_string(args, flag, format)?, flag, format)
}

fn take_query_vector(args: &mut Vec<String>, format: OutputFormat) -> Result<Vec<f32>, CliError> {
    let query = take_string(args, "--query", format)?;
    let vector = take_string(args, "--vector", format)?;
    match (query, vector) {
        (Some(_), Some(_)) => Err(CliError::usage(
            "duplicate query vector; use only one of --query or --vector".to_string(),
            format,
        )),
        (Some(value), None) => parse_vector_literal(&value, "--query", format),
        (None, Some(value)) => parse_vector_literal(&value, "--vector", format),
        (None, None) => Err(CliError::usage(
            "missing --query or --vector".to_string(),
            format,
        )),
    }
}

fn take_required_json(
    args: &mut Vec<String>,
    flag: &'static str,
    format: OutputFormat,
) -> Result<Value, CliError> {
    let value = take_required_string(args, flag, format)?;
    parse_json_literal(&value, flag, format)
}

fn take_optional_json(
    args: &mut Vec<String>,
    flag: &'static str,
    format: OutputFormat,
) -> Result<Option<Value>, CliError> {
    take_string(args, flag, format)?
        .map(|value| parse_json_literal(&value, flag, format))
        .transpose()
}

fn take_required_filter(
    args: &mut Vec<String>,
    flag: &'static str,
    format: OutputFormat,
) -> Result<VectorMetadataFilter, CliError> {
    let value = take_required_json(args, flag, format)?;
    parse_filter_value(value, flag, format)
}

fn take_optional_filter(
    args: &mut Vec<String>,
    flag: &'static str,
    format: OutputFormat,
) -> Result<Option<VectorMetadataFilter>, CliError> {
    take_optional_json(args, flag, format)?
        .map(|value| parse_filter_value(value, flag, format))
        .transpose()
}

fn parse_metric(value: &str, format: OutputFormat) -> Result<VectorDistanceMetric, CliError> {
    match value {
        "cosine" => Ok(VectorDistanceMetric::Cosine),
        "euclidean" => Ok(VectorDistanceMetric::Euclidean),
        "dot_product" | "dot-product" => Ok(VectorDistanceMetric::DotProduct),
        _ => Err(CliError::usage(
            format!("unsupported vector metric `{value}`"),
            format,
        )),
    }
}

fn parse_vector_literal(
    value: &str,
    flag: &'static str,
    format: OutputFormat,
) -> Result<Vec<f32>, CliError> {
    let trimmed = value.trim();
    let vector = if trimmed.starts_with('[') {
        serde_json::from_str::<Vec<f32>>(trimmed)
            .map_err(|_| CliError::usage(format!("invalid vector literal for {flag}"), format))?
    } else {
        trimmed
            .split(',')
            .map(str::trim)
            .map(|part| {
                part.parse::<f32>().map_err(|_| {
                    CliError::usage(format!("invalid vector literal for {flag}"), format)
                })
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    if vector.is_empty() {
        return Err(CliError::usage(
            format!("empty vector literal for {flag}"),
            format,
        ));
    }
    Ok(vector)
}

fn parse_json_literal(
    value: &str,
    flag: &'static str,
    format: OutputFormat,
) -> Result<Value, CliError> {
    serde_json::from_str(value)
        .map_err(|_| CliError::usage(format!("invalid JSON value for {flag}"), format))
}

fn parse_filter_value(
    value: Value,
    flag: &'static str,
    format: OutputFormat,
) -> Result<VectorMetadataFilter, CliError> {
    if value.get("conditions").is_some() {
        return serde_json::from_value(value)
            .map_err(|_| CliError::usage(format!("invalid filter value for {flag}"), format));
    }
    let Value::Object(fields) = value else {
        return Err(CliError::usage(
            format!("filter shorthand for {flag} must be a JSON object"),
            format,
        ));
    };
    let conditions = fields
        .into_iter()
        .map(|(field, value)| {
            Ok(VectorFilterCondition::new(
                field,
                VectorFilterOp::Eq,
                scalar_from_json(value, flag, format)?,
            ))
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    Ok(VectorMetadataFilter::new(conditions))
}

fn scalar_from_json(
    value: Value,
    flag: &'static str,
    format: OutputFormat,
) -> Result<VectorScalar, CliError> {
    match value {
        Value::Null => Ok(VectorScalar::Null),
        Value::Bool(value) => Ok(VectorScalar::Bool(value)),
        Value::Number(value) => value.as_f64().map(VectorScalar::Number).ok_or_else(|| {
            CliError::usage(format!("filter value for {flag} must fit f64"), format)
        }),
        Value::String(value) => Ok(VectorScalar::String(value)),
        Value::Array(_) | Value::Object(_) => Err(CliError::usage(
            format!("filter values for {flag} must be scalar"),
            format,
        )),
    }
}

fn render_output(output: &Output, format: OutputFormat) -> String {
    match format {
        OutputFormat::Json => json_output(output),
        OutputFormat::Human => render_human(output),
    }
}

fn render_human(output: &Output) -> String {
    match output {
        Output::VectorCollectionList { items, page } => render_string_page(
            items.iter().map(|item| {
                format!(
                    "{}\tdimension={}\tmetric={:?}\tcount={}",
                    item.name(),
                    item.dimension(),
                    item.metric(),
                    item.count()
                )
            }),
            page.has_more(),
            page.cursor(),
        ),
        Output::VectorWriteResult {
            collection,
            key,
            effect,
            version,
            timestamp,
            vector_revision,
            ..
        } => format!(
            "ok\ncollection: {collection}\nkey: {key}\neffect: {}\nversion: {version}\ntimestamp: {timestamp}\nvector_revision: {vector_revision}",
            effect_label(effect)
        ),
        Output::VectorMetadataUpdateResult {
            collection,
            key,
            updated,
            effect,
            version,
            timestamp,
            vector_revision,
            ..
        } => render_optional_vector_mutation(
            "ok",
            collection,
            key,
            effect,
            *updated,
            *version,
            *timestamp,
            *vector_revision,
        ),
        Output::VectorDeleteResult {
            collection,
            key,
            deleted,
            effect,
            version,
            timestamp,
            ..
        } => render_optional_vector_mutation(
            "ok",
            collection,
            key,
            effect,
            *deleted,
            *version,
            *timestamp,
            None,
        ),
        Output::VectorBulkDeleteResult {
            collection,
            deleted_count,
            effect,
            version,
            timestamp,
            ..
        } => {
            let mut lines = vec![
                "ok".to_string(),
                format!("collection: {collection}"),
                format!("deleted_count: {deleted_count}"),
                format!("effect: {}", effect_label(effect)),
            ];
            push_optional_commit(&mut lines, *version, *timestamp, None);
            lines.join("\n")
        }
        Output::VectorData(Some(value)) => render_vector_data(value),
        Output::VectorData(None) | Output::VectorVersionHistory(None) => "missing".to_string(),
        Output::VectorVersionHistory(Some(items)) => render_vector_history(items),
        Output::VectorMatches(matches) => render_matches(matches),
        Output::VectorIndexQuery(result) => render_index_query(result),
        Output::VectorKeyPage { items, page } => {
            render_string_page(items.iter().cloned(), page.has_more(), page.cursor())
        }
        Output::Bool(value) => value.to_string(),
        Output::Uint(value) => value.to_string(),
        other => json_output(&StableDebugFallback { output: other }),
    }
}

fn render_vector_data(value: &strata_executor_next::VectorVersionedData) -> String {
    let mut lines = vec![
        "found".to_string(),
        format!("key: {}", value.key()),
        format!("dimension: {}", value.data().embedding().len()),
        format!("version: {}", value.version()),
        format!("timestamp: {}", value.timestamp()),
        format!("vector_revision: {}", value.vector_revision()),
    ];
    if let Some(metadata) = value.data().metadata() {
        lines.push(format!("metadata: {}", compact_json(metadata)));
    }
    lines.join("\n")
}

fn render_vector_history(items: &[strata_executor_next::VectorHistoryItem]) -> String {
    render_string_page(
        items.iter().map(|item| {
            format!(
                "{}\tversion={}\ttimestamp={}\ttombstone={}",
                item.key(),
                item.version(),
                item.timestamp(),
                item.is_tombstone()
            )
        }),
        false,
        None,
    )
}

fn render_index_query(result: &strata_executor_next::VectorIndexQueryResult) -> String {
    let mut text = render_matches(result.matches());
    text.push('\n');
    write!(
        &mut text,
        "diagnostics: manifest_status={} used_index={} fallback={}",
        result.diagnostics().manifest_status(),
        result.diagnostics().last_query_used_index(),
        result
            .diagnostics()
            .last_query_fallback_reason()
            .unwrap_or("none")
    )
    .expect("writing to String should not fail");
    text
}

#[allow(clippy::too_many_arguments)]
fn render_optional_vector_mutation(
    header: &str,
    collection: &str,
    key: &str,
    effect: &strata_executor_next::MutationEffect,
    matched: bool,
    version: Option<u64>,
    timestamp: Option<u64>,
    vector_revision: Option<u64>,
) -> String {
    let mut lines = vec![
        header.to_string(),
        format!("collection: {collection}"),
        format!("key: {key}"),
        format!("matched: {matched}"),
        format!("effect: {}", effect_label(effect)),
    ];
    push_optional_commit(&mut lines, version, timestamp, vector_revision);
    lines.join("\n")
}

fn push_optional_commit(
    lines: &mut Vec<String>,
    version: Option<u64>,
    timestamp: Option<u64>,
    vector_revision: Option<u64>,
) {
    if let Some(version) = version {
        lines.push(format!("version: {version}"));
    }
    if let Some(timestamp) = timestamp {
        lines.push(format!("timestamp: {timestamp}"));
    }
    if let Some(vector_revision) = vector_revision {
        lines.push(format!("vector_revision: {vector_revision}"));
    }
}

fn render_matches(matches: &[VectorMatch]) -> String {
    render_string_page(
        matches.iter().map(|match_| {
            let mut line = format!("{}\tscore={}", match_.key(), match_.score());
            if let Some(metadata) = match_.metadata() {
                write!(&mut line, "\tmetadata={}", compact_json(metadata))
                    .expect("writing to String should not fail");
            }
            line
        }),
        false,
        None,
    )
}

fn render_string_page(
    rows: impl IntoIterator<Item = String>,
    has_more: bool,
    cursor: Option<&String>,
) -> String {
    let mut lines = rows.into_iter().collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push("(empty)".to_string());
    }
    lines.push(format!("has_more: {has_more}"));
    lines.push(format!(
        "cursor: {}",
        cursor.map_or_else(|| "null".to_string(), Clone::clone)
    ));
    lines.join("\n")
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).expect("JSON value should serialize")
}

fn effect_label(effect: &strata_executor_next::MutationEffect) -> String {
    format!("{:?}", effect.kind()).to_ascii_lowercase()
}

#[derive(Serialize)]
struct StableDebugFallback<'a> {
    output: &'a Output,
}
