//! Installation and database diagnostics (first-run D5).
//!
//! `strata doctor` is the one command a human or agent runs when anything is
//! off: it reports the binary, platform, Strata home, PATH visibility, and —
//! when a database is targeted via path, `--db`, `STRATA_DB`, or `--cache` —
//! a health summary. Every finding carries a stable code and an actionable
//! hint; the process exits non-zero when any issue is found, so install
//! scripts can end with `strata doctor` as their verification step.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use strata_executor::ipc::Connection;
use strata_executor::{Command, DurableLocalOpenOptions, Executor, ExecutorError, IpcMode};

use crate::{init, open, CliError};

/// Runs the doctor checks. Returns the report and whether it found no issues.
pub(crate) fn run_doctor(
    cache: bool,
    db_flag: Option<PathBuf>,
    db_path: Option<PathBuf>,
) -> Result<(Value, bool), CliError> {
    let mut issues: Vec<Value> = Vec::new();

    let home = if let Ok(path) = init::strata_home() {
        if path.exists() && !path.is_dir() {
            issues.push(issue(
                "failed_precondition.cli.home_not_directory",
                "the Strata home exists but is not a directory; move it or point STRATA_HOME elsewhere",
            ));
        }
        Value::String(path.display().to_string())
    } else {
        issues.push(issue(
            "failed_precondition.cli.home_unresolved",
            "HOME is not set; set STRATA_HOME explicitly",
        ));
        Value::Null
    };

    let path_ok = binary_on_path();
    if !path_ok {
        issues.push(issue(
            "failed_precondition.cli.binary_not_on_path",
            "add the strata binary's directory to PATH so tools and agents can invoke it",
        ));
    }

    let database = database_report(cache, db_flag, db_path, &mut issues)?;

    let healthy = issues.is_empty();
    let report = json!({
        "type": "doctor",
        "data": {
            "binary": env!("CARGO_PKG_VERSION"),
            "platform": format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            "home": home,
            "path_ok": path_ok,
            "database": database,
            "issues": issues,
        }
    });
    Ok((report, healthy))
}

fn database_report(
    cache: bool,
    db_flag: Option<PathBuf>,
    db_path: Option<PathBuf>,
    issues: &mut Vec<Value>,
) -> Result<Value, CliError> {
    if cache {
        if db_flag.is_some() || db_path.is_some() {
            return Err(CliError::usage(
                "`--cache` cannot be combined with `--db` or a database path",
            ));
        }
        // A cache database is created fresh per process, so probing one
        // proves the engine opens in this environment — nothing more.
        return Ok(match Executor::open_cache() {
            Ok(mut executor) => {
                let open_ok = executor.execute(Command::Info { branch: None }).is_ok();
                if let Err(error) = executor.close() {
                    push_executor_issue(issues, &error);
                }
                json!({"mode": "cache", "open_ok": open_ok})
            }
            Err(error) => {
                push_executor_issue(issues, &error);
                json!({"mode": "cache", "open_ok": false})
            }
        });
    }

    let target = match (db_flag, db_path) {
        (Some(_), Some(_)) => {
            return Err(CliError::usage(
                "provide either `--db <path>` or positional database path, not both",
            ));
        }
        (Some(path), None) | (None, Some(path)) => Some(path),
        (None, None) => open::env_database_path(),
    };
    let Some(path) = target else {
        // No database targeted: doctor checks the installation alone.
        return Ok(Value::Null);
    };
    if !path.exists() {
        issues.push(issue(
            "not_found.cli.database_path",
            "the target database path does not exist; doctor does not create databases",
        ));
        return Ok(json!({
            "path": path.display().to_string(),
            "exists": false,
        }));
    }
    Ok(probe_database(&path, issues))
}

fn probe_database(path: &Path, issues: &mut Vec<Value>) -> Value {
    let display = path.display().to_string();
    // Broker as a client: if the store is owned by a live process, probe it
    // through that owner's socket instead of failing on the writer lock — so
    // `strata doctor` can inspect a database while an app or REPL holds it.
    match Connection::open_durable_local_brokered(
        path,
        DurableLocalOpenOptions::new(),
        IpcMode::Client,
    ) {
        Ok(connection) => {
            let info = match connection.execute(Command::Info { branch: None }) {
                Ok(output) => serde_json::to_value(&output)
                    .ok()
                    .and_then(|mut value| value.get_mut("data").map(Value::take)),
                Err(error) => {
                    push_executor_issue(issues, &error);
                    None
                }
            };
            if let Err(error) = connection.close() {
                push_executor_issue(issues, &error);
            }
            json!({
                "path": display,
                "exists": true,
                "open_ok": info.is_some(),
                "info": info,
            })
        }
        Err(error) => {
            push_executor_issue(issues, &error);
            json!({
                "path": display,
                "exists": true,
                "open_ok": false,
            })
        }
    }
}

fn push_executor_issue(issues: &mut Vec<Value>, error: &ExecutorError) {
    issues.push(issue(error.code(), error.suggested_fix()));
}

fn issue(code: &str, hint: &str) -> Value {
    json!({"code": code, "hint": hint})
}

/// True when a binary with this executable's file name is reachable on PATH.
fn binary_on_path() -> bool {
    let Some(name) = std::env::current_exe()
        .ok()
        .and_then(|path| path.file_name().map(std::ffi::OsStr::to_os_string))
    else {
        return false;
    };
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths)
        .any(|dir| !dir.as_os_str().is_empty() && dir.join(&name).is_file())
}
