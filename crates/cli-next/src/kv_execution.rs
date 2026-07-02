use std::fmt::Write as _;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use strata_executor_next::{
    BatchGetItemResult, BatchItemResult, BatchKvEntry, BatchResult, Bytes, Command, HistoryItem,
    Output, SampleItem,
};

use crate::execution::{
    bytes, execute_durable, reject_unknown_flags, require_positional_len, strip_argument_delimiter,
    take_string, take_u64, CommandScope,
};
use crate::{json_output, CliError, OutputFormat};

pub(crate) fn run_kv(
    mut args: Vec<String>,
    format: OutputFormat,
    db: Option<PathBuf>,
) -> Result<String, CliError> {
    let Some(db) = db else {
        return Err(CliError::usage(
            "missing --db <path> for kv command".to_string(),
            format,
        ));
    };
    let command = parse_kv_command(&mut args, format)?;
    let output = execute_durable(db, command, format)?;
    Ok(render_output(&output, format))
}

fn parse_kv_command(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let Some(op) = args.first().cloned() else {
        return Err(CliError::usage("missing kv operation".to_string(), format));
    };
    args.remove(0);
    match op.as_str() {
        "put" => parse_put(args, format),
        "get" => parse_get(args, format),
        "delete" => parse_delete(args, format),
        "batch-put" => parse_batch_put(args, format),
        "batch-get" => parse_batch_get(args, format),
        "batch-delete" => parse_batch_delete(args, format),
        "batch-exists" => parse_batch_exists(args, format),
        "list" => parse_list(args, format),
        "scan" => parse_scan(args, format),
        "exists" => parse_exists(args, format),
        "history" => parse_history(args, format),
        "count" => parse_count(args, format),
        "sample" => parse_sample(args, format),
        _ => Err(CliError::usage(
            format!("unknown kv operation `{op}`"),
            format,
        )),
    }
}

fn parse_put(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 2, "kv put <key> <value>", format)?;
    Ok(Command::KvPut {
        branch: scope.branch,
        space: scope.space,
        key: bytes(&args[0]),
        value: bytes(&args[1]),
    })
}

fn parse_get(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    let as_of = take_u64(args, "--as-of", format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 1, "kv get <key>", format)?;
    Ok(Command::KvGet {
        branch: scope.branch,
        space: scope.space,
        key: bytes(&args[0]),
        as_of,
    })
}

fn parse_delete(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 1, "kv delete <key>", format)?;
    Ok(Command::KvDelete {
        branch: scope.branch,
        space: scope.space,
        key: bytes(&args[0]),
    })
}

fn parse_batch_put(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    let entries =
        parse_kv_batch_entries(&take_required_string(args, "--entries", format)?, format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 0, "kv batch-put --entries <json>", format)?;
    Ok(Command::KvBatchPut {
        branch: scope.branch,
        space: scope.space,
        entries,
    })
}

fn parse_batch_get(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    let keys = parse_string_array(
        &take_required_string(args, "--keys", format)?,
        "--keys",
        format,
    )?
    .into_iter()
    .map(|key| bytes(&key))
    .collect();
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 0, "kv batch-get --keys <json>", format)?;
    Ok(Command::KvBatchGet {
        branch: scope.branch,
        space: scope.space,
        keys,
    })
}

fn parse_batch_delete(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    let keys = parse_string_array(
        &take_required_string(args, "--keys", format)?,
        "--keys",
        format,
    )?
    .into_iter()
    .map(|key| bytes(&key))
    .collect();
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 0, "kv batch-delete --keys <json>", format)?;
    Ok(Command::KvBatchDelete {
        branch: scope.branch,
        space: scope.space,
        keys,
    })
}

fn parse_batch_exists(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    let keys = parse_string_array(
        &take_required_string(args, "--keys", format)?,
        "--keys",
        format,
    )?
    .into_iter()
    .map(|key| bytes(&key))
    .collect();
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 0, "kv batch-exists --keys <json>", format)?;
    Ok(Command::KvBatchExists {
        branch: scope.branch,
        space: scope.space,
        keys,
    })
}

fn parse_list(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    let prefix = take_string(args, "--prefix", format)?.map(|value| bytes(&value));
    let cursor = take_string(args, "--cursor", format)?.map(|value| bytes(&value));
    let limit = take_u64(args, "--limit", format)?;
    let as_of = take_u64(args, "--as-of", format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 0, "kv list", format)?;
    Ok(Command::KvList {
        branch: scope.branch,
        space: scope.space,
        prefix,
        cursor,
        limit,
        as_of,
    })
}

fn parse_scan(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    let start = take_string(args, "--start", format)?.map(|value| bytes(&value));
    let limit = take_u64(args, "--limit", format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 0, "kv scan", format)?;
    Ok(Command::KvScan {
        branch: scope.branch,
        space: scope.space,
        start,
        limit,
    })
}

fn parse_exists(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 1, "kv exists <key>", format)?;
    Ok(Command::KvExists {
        branch: scope.branch,
        space: scope.space,
        key: bytes(&args[0]),
    })
}

fn parse_history(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 1, "kv history <key>", format)?;
    Ok(Command::KvGetv {
        branch: scope.branch,
        space: scope.space,
        key: bytes(&args[0]),
    })
}

fn parse_count(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    let prefix = take_string(args, "--prefix", format)?.map(|value| bytes(&value));
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 0, "kv count", format)?;
    Ok(Command::KvCount {
        branch: scope.branch,
        space: scope.space,
        prefix,
    })
}

fn parse_sample(args: &mut Vec<String>, format: OutputFormat) -> Result<Command, CliError> {
    let scope = CommandScope::extract(args, format)?;
    let prefix = take_string(args, "--prefix", format)?.map(|value| bytes(&value));
    let count = take_u64(args, "--count", format)?;
    reject_unknown_flags(args, format)?;
    strip_argument_delimiter(args);
    require_positional_len(args, 0, "kv sample", format)?;
    Ok(Command::KvSample {
        branch: scope.branch,
        space: scope.space,
        prefix,
        count,
    })
}

fn take_required_string(
    args: &mut Vec<String>,
    flag: &'static str,
    format: OutputFormat,
) -> Result<String, CliError> {
    take_string(args, flag, format)?
        .ok_or_else(|| CliError::usage(format!("missing {flag}"), format))
}

fn parse_string_array(
    value: &str,
    flag: &'static str,
    format: OutputFormat,
) -> Result<Vec<String>, CliError> {
    serde_json::from_str(value)
        .map_err(|_| CliError::usage(format!("{flag} must be a JSON array of strings"), format))
}

fn parse_kv_batch_entries(
    value: &str,
    format: OutputFormat,
) -> Result<Vec<BatchKvEntry>, CliError> {
    let entries = serde_json::from_str::<Vec<KvBatchEntryArg>>(value).map_err(|_| {
        CliError::usage(
            "--entries must be a JSON array of {\"key\",\"value\"} objects".to_string(),
            format,
        )
    })?;
    Ok(entries
        .into_iter()
        .map(|entry| BatchKvEntry::new(bytes(&entry.key), bytes(&entry.value)))
        .collect())
}

fn render_output(output: &Output, format: OutputFormat) -> String {
    match format {
        OutputFormat::Json => json_output(output),
        OutputFormat::Human => render_human(output),
    }
}

fn render_human(output: &Output) -> String {
    match output {
        Output::WriteResult {
            key,
            effect,
            commit,
        } => format!(
            "ok\nkey: {}\neffect: {}\nversion: {}\ntimestamp: {}",
            display_bytes(key),
            effect_label(effect),
            commit.version(),
            commit.timestamp()
        ),
        Output::DeleteResult {
            key,
            effect,
            commit,
        } => {
            let mut lines = vec![
                "ok".to_string(),
                format!("key: {}", display_bytes(key)),
                format!("effect: {}", effect_label(effect)),
            ];
            if let Some(commit) = commit {
                lines.push(format!("version: {}", commit.version()));
                lines.push(format!("timestamp: {}", commit.timestamp()));
            }
            lines.join("\n")
        }
        Output::KvValue(value) => render_optional_bytes(value.as_ref(), None, None),
        Output::KvVersionedValue(value) => match value {
            Some(value) => render_optional_bytes(
                Some(value.value()),
                Some(value.version()),
                Some(value.timestamp()),
            ),
            None => "missing".to_string(),
        },
        Output::Keys { items, page } | Output::KeysPage { items, page } => render_page(
            items.iter().map(display_bytes),
            page.has_more(),
            page.cursor(),
        ),
        Output::VersionHistory(Some(items)) => render_history(items),
        Output::VersionHistory(None) => "missing".to_string(),
        Output::KvScanResult { items, page } => {
            let rows = items.iter().map(|item| {
                format!(
                    "{}\t{}\t{}\t{}",
                    display_bytes(item.key()),
                    display_bytes(item.value()),
                    item.version(),
                    item.timestamp()
                )
            });
            render_page(rows, page.has_more(), page.cursor())
        }
        Output::BatchResults(results) => render_batch_results(results),
        Output::BatchGetResults(results) => render_batch_get_results(results),
        Output::Bool(value) => value.to_string(),
        Output::BoolList(values) => render_page(values.iter().map(bool::to_string), false, None),
        Output::Uint(value) => value.to_string(),
        Output::SampleResult {
            total_count,
            items,
            page,
        } => render_sample(*total_count, items, page.has_more(), page.cursor()),
        other => json_output(&StableDebugFallback { output: other }),
    }
}

fn render_optional_bytes(
    value: Option<&Bytes>,
    version: Option<u64>,
    timestamp: Option<u64>,
) -> String {
    let Some(value) = value else {
        return "missing".to_string();
    };
    let mut lines = vec![
        "found".to_string(),
        format!("value: {}", display_bytes(value)),
    ];
    if let Some(version) = version {
        lines.push(format!("version: {version}"));
    }
    if let Some(timestamp) = timestamp {
        lines.push(format!("timestamp: {timestamp}"));
    }
    lines.join("\n")
}

fn render_history(items: &[HistoryItem]) -> String {
    render_page(
        items.iter().map(|item| {
            let value = item.value().map_or("null".to_string(), display_bytes);
            format!(
                "{value}\tversion={}\ttimestamp={}\ttombstone={}",
                item.version(),
                item.timestamp(),
                item.is_tombstone()
            )
        }),
        false,
        None,
    )
}

fn render_batch_results(results: &BatchResult<BatchItemResult>) -> String {
    let mut lines = batch_header(results);
    lines.extend(results.items().iter().map(|item| {
        let Some(result) = item.result() else {
            return format!("{}\tstatus={:?}", item.index(), item.status());
        };
        let mut line = format!(
            "{}\t{}\tstatus={:?}\tapplied={}",
            item.index(),
            display_bytes(result.key()),
            item.status(),
            item.applied()
        );
        if let Some(effect) = result.effect() {
            write!(&mut line, "\teffect={}", effect_label(effect))
                .expect("writing to String should not fail");
        }
        if let Some(version) = result.version() {
            write!(&mut line, "\tversion={version}").expect("writing to String should not fail");
        }
        if let Some(error) = item.error() {
            write!(&mut line, "\terror={error}").expect("writing to String should not fail");
        }
        line
    }));
    lines.join("\n")
}

fn render_batch_get_results(results: &BatchResult<BatchGetItemResult>) -> String {
    let mut lines = batch_header(results);
    lines.extend(results.items().iter().map(|item| {
        let Some(result) = item.result() else {
            return format!("{}\tstatus={:?}", item.index(), item.status());
        };
        let mut line = format!(
            "{}\t{}\tstatus={:?}\tfound={}",
            item.index(),
            display_bytes(result.key()),
            item.status(),
            result.found()
        );
        if let Some(value) = result.value() {
            write!(&mut line, "\tvalue={}", display_bytes(value))
                .expect("writing to String should not fail");
        }
        if let Some(error) = item.error() {
            write!(&mut line, "\terror={error}").expect("writing to String should not fail");
        }
        line
    }));
    lines.join("\n")
}

fn render_sample(
    total_count: u64,
    items: &[SampleItem],
    has_more: bool,
    cursor: Option<&Bytes>,
) -> String {
    let rows = items.iter().map(|item| {
        format!(
            "{}\t{}\t{}\t{}",
            display_bytes(item.key()),
            display_bytes(item.value()),
            item.version(),
            item.timestamp()
        )
    });
    let mut lines = vec![format!("total_count: {total_count}")];
    lines.push(render_page(rows, has_more, cursor));
    lines.join("\n")
}

fn batch_header<T>(results: &BatchResult<T>) -> Vec<String> {
    let mut lines = vec![
        format!("mode: {:?}", results.mode()).to_ascii_lowercase(),
        format!("status: {:?}", results.status()).to_ascii_lowercase(),
        format!("applied: {}", results.applied()),
    ];
    if let Some(commit) = results.commit() {
        lines.push(format!("version: {}", commit.version()));
        lines.push(format!("timestamp: {}", commit.timestamp()));
    }
    lines
}

fn render_page(
    rows: impl IntoIterator<Item = String>,
    has_more: bool,
    cursor: Option<&Bytes>,
) -> String {
    let mut lines = rows.into_iter().collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push("(empty)".to_string());
    }
    lines.push(format!("has_more: {has_more}"));
    lines.push(format!(
        "cursor: {}",
        cursor.map_or_else(|| "null".to_string(), display_bytes)
    ));
    lines.join("\n")
}

fn display_bytes(bytes: &Bytes) -> String {
    if let Ok(value) = std::str::from_utf8(bytes.as_slice()) {
        return value.to_owned();
    }
    let mut hex = String::with_capacity(bytes.as_slice().len() * 2);
    for byte in bytes.as_slice() {
        write!(&mut hex, "{byte:02x}").expect("writing to String should not fail");
    }
    format!("0x{hex}")
}

fn effect_label(effect: &strata_executor_next::MutationEffect) -> String {
    format!("{:?}", effect.kind()).to_ascii_lowercase()
}

#[derive(Serialize)]
struct StableDebugFallback<'a> {
    output: &'a Output,
}

#[derive(Deserialize)]
struct KvBatchEntryArg {
    key: String,
    value: String,
}
