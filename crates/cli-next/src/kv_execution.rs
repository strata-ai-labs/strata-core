use std::fmt::Write as _;
use std::path::PathBuf;

use serde::Serialize;
use strata_executor_next::{Bytes, Command, Output};

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
        "list" => parse_list(args, format),
        "scan" => parse_scan(args, format),
        "exists" => parse_exists(args, format),
        "count" => parse_count(args, format),
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
        Output::Bool(value) => value.to_string(),
        Output::Uint(value) => value.to_string(),
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
