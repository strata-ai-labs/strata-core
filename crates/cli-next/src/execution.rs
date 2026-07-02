use std::path::PathBuf;

use strata_executor_next::{Bytes, Command, Executor, Output};

use crate::{CliError, OutputFormat};

pub(crate) const ARGUMENT_DELIMITER: &str = "--";

pub(crate) fn execute_durable(
    db: PathBuf,
    command: Command,
    format: OutputFormat,
) -> Result<Output, CliError> {
    let mut executor =
        Executor::open_durable_local(db).map_err(|error| CliError::executor(&error, format))?;
    let output = match executor.execute(command) {
        Ok(output) => output,
        Err(error) => {
            // Preserve the command failure, but do not leave embedded callers
            // with an explicitly opened durable handle.
            let _ = executor.close();
            return Err(CliError::executor(&error, format));
        }
    };
    executor
        .close()
        .map_err(|error| CliError::executor(&error, format))?;
    Ok(output)
}

#[derive(Default)]
pub(crate) struct CommandScope {
    pub(crate) branch: Option<String>,
    pub(crate) space: Option<String>,
}

impl CommandScope {
    pub(crate) fn extract(args: &mut Vec<String>, format: OutputFormat) -> Result<Self, CliError> {
        Ok(Self {
            branch: take_string(args, "--branch", format)?,
            space: take_string(args, "--space", format)?,
        })
    }
}

pub(crate) fn take_string(
    args: &mut Vec<String>,
    flag: &'static str,
    format: OutputFormat,
) -> Result<Option<String>, CliError> {
    let mut value = None;
    let mut offset = 0;
    while offset < args.len() {
        if args[offset] == ARGUMENT_DELIMITER {
            break;
        }
        if args[offset] == flag {
            args.remove(offset);
            if value.is_some() {
                return Err(CliError::usage(format!("duplicate {flag}"), format));
            }
            let Some(next) = args.get(offset).cloned() else {
                return Err(CliError::usage(format!("missing value for {flag}"), format));
            };
            if next.starts_with("--") {
                return Err(CliError::usage(format!("missing value for {flag}"), format));
            }
            args.remove(offset);
            value = Some(next);
        } else {
            offset += 1;
        }
    }
    Ok(value)
}

pub(crate) fn take_u64(
    args: &mut Vec<String>,
    flag: &'static str,
    format: OutputFormat,
) -> Result<Option<u64>, CliError> {
    let Some(value) = take_string(args, flag, format)? else {
        return Ok(None);
    };
    value.parse::<u64>().map(Some).map_err(|_| {
        CliError::usage(
            format!("invalid integer value `{value}` for {flag}"),
            format,
        )
    })
}

pub(crate) fn reject_unknown_flags(args: &[String], format: OutputFormat) -> Result<(), CliError> {
    let scan_len = args
        .iter()
        .position(|arg| arg == ARGUMENT_DELIMITER)
        .unwrap_or(args.len());
    if let Some(flag) = args[..scan_len].iter().find(|arg| arg.starts_with("--")) {
        return Err(CliError::usage(format!("unknown option `{flag}`"), format));
    }
    Ok(())
}

pub(crate) fn strip_argument_delimiter(args: &mut Vec<String>) {
    if let Some(offset) = args.iter().position(|arg| arg == ARGUMENT_DELIMITER) {
        args.remove(offset);
    }
}

pub(crate) fn require_positional_len(
    args: &[String],
    expected: usize,
    usage: &'static str,
    format: OutputFormat,
) -> Result<(), CliError> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(CliError::usage(format!("usage: strata {usage}"), format))
    }
}

pub(crate) fn bytes(value: &str) -> Bytes {
    Bytes::from(value)
}
