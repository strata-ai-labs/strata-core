//! First-run setup helpers.

use std::fs;
use std::path::PathBuf;

use serde_json::json;
use serde_json::Value;

use crate::{guidance, CliError};

pub(crate) fn run_init() -> Result<Value, CliError> {
    let home = strata_home()?;
    let existed = home.exists();
    fs::create_dir_all(&home)?;

    Ok(json!({
        "type": "init",
        "data": {
            "home": home,
            "created": !existed,
            "next_steps": guidance::NEXT_STEPS,
        }
    }))
}

pub(crate) fn strata_home() -> Result<PathBuf, CliError> {
    if let Some(path) = std::env::var_os("STRATA_HOME") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| CliError::usage("HOME is not set; set STRATA_HOME explicitly"))?;
    Ok(home.join(".strata"))
}
