//! Executor open helpers.

use std::path::PathBuf;

use strata_executor_next::Executor;

use crate::CliError;

/// How the invocation intends to use the database (first-run D2).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OpenIntent {
    /// A single command: refuses to run without an explicit target.
    OneShot,
    /// An interactive TTY session: falls back to an ephemeral cache session.
    Interactive,
    /// A piped (non-TTY) session: refuses like one-shot commands — an agent
    /// streaming commands must never write to an implicit location.
    Pipe,
}

/// An opened executor plus how its target was chosen.
pub(crate) struct OpenedExecutor {
    pub(crate) executor: Executor,
    /// True when a bare interactive invocation fell back to cache mode; the
    /// caller prints the nothing-is-persisted banner.
    pub(crate) implicit_cache: bool,
}

/// Resolves the database target: explicit flag/path, then the `STRATA_DB`
/// environment variable, then intent-specific fallback. A one-shot or piped
/// invocation with no target refuses with a teaching error instead of opening
/// the current directory implicitly — agents run commands from arbitrary
/// directories, and an accidental durable database in cwd is data loss
/// waiting to happen.
pub(crate) fn open_executor(
    cache: bool,
    db_flag: Option<PathBuf>,
    db_path: Option<PathBuf>,
    intent: OpenIntent,
) -> Result<OpenedExecutor, CliError> {
    if cache {
        if db_flag.is_some() || db_path.is_some() {
            return Err(CliError::usage(
                "`--cache` cannot be combined with `--db` or a database path",
            ));
        }
        return Ok(OpenedExecutor {
            executor: Executor::open_cache()?,
            implicit_cache: false,
        });
    }

    let path = match (db_flag, db_path) {
        (Some(_), Some(_)) => {
            return Err(CliError::usage(
                "provide either `--db <path>` or positional database path, not both",
            ));
        }
        (Some(path), None) | (None, Some(path)) => Some(path),
        (None, None) => env_database_path(),
    };

    if let Some(path) = path {
        return Ok(OpenedExecutor {
            executor: Executor::open_durable_local(path)?,
            implicit_cache: false,
        });
    }

    match intent {
        OpenIntent::Interactive => Ok(OpenedExecutor {
            executor: Executor::open_cache()?,
            implicit_cache: true,
        }),
        OpenIntent::OneShot | OpenIntent::Pipe => Err(CliError::usage(
            "[invalid_argument.cli.no_database]: no database specified\n  hint: pass a path (strata ./mydb kv put …), set STRATA_DB, or use --cache for ephemeral",
        )),
    }
}

fn env_database_path() -> Option<PathBuf> {
    std::env::var_os("STRATA_DB")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
