//! Executor open helpers.

use std::path::PathBuf;

use strata_executor_next::Executor;

use crate::CliError;

pub(crate) fn open_executor(
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
