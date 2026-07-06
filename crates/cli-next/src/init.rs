//! First-run setup helpers.

use std::fs;
use std::path::PathBuf;

use serde_json::json;
use serde_json::Value;

use crate::CliError;

pub(crate) fn run_init() -> Result<Value, CliError> {
    let home = strata_home()?;
    let existed = home.exists();
    fs::create_dir_all(&home)?;

    Ok(json!({
        "type": "init",
        "data": {
            "home": home,
            "created": !existed,
            "database_created": false,
            "next_steps": [
                "strata ./my-db",
                "strata --db ./my-db kv put key value",
                "strata --cache"
            ]
        }
    }))
}

fn strata_home() -> Result<PathBuf, CliError> {
    if let Some(path) = std::env::var_os("STRATA_HOME") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| CliError::usage("HOME is not set; set STRATA_HOME explicitly"))?;
    Ok(home.join(".strata"))
}
