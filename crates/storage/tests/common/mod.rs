//! Shared integration-test support for storage harnesses.

#![deny(unsafe_code)]
#![expect(
    dead_code,
    reason = "shared integration support is consumed by different harness targets"
)]

pub(crate) mod source_guard_helpers;

use std::env;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

pub(crate) const BACKEND_ENV: &str = "STRATA_STORAGE_TEST_BACKEND";
pub(crate) const CRASH_CASES_ENV: &str = "STRATA_STORAGE_CRASH_CASES";
pub(crate) const FAULT_CASES_ENV: &str = "STRATA_STORAGE_FAULT_CASES";
pub(crate) const KEEP_TEST_DIR_ENV: &str = "STRATA_STORAGE_KEEP_TEST_DIR";
pub(crate) const STRESS_SECONDS_ENV: &str = "STRATA_STORAGE_STRESS_SECONDS";
pub(crate) const STRESS_SEED_ENV: &str = "STRATA_STORAGE_STRESS_SEED";
pub(crate) const TEST_ROOT_ENV: &str = "STRATA_STORAGE_TEST_ROOT";

pub(crate) fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub(crate) fn storage_format_goldens_dir() -> PathBuf {
    crate_root().join("testdata/goldens/storage-format-v1")
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct HarnessConfigError {
    variable: &'static str,
    reason: &'static str,
}

impl HarnessConfigError {
    const fn new(variable: &'static str, reason: &'static str) -> Self {
        Self { variable, reason }
    }
}

impl fmt::Display for HarnessConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid {}: {}", self.variable, self.reason)
    }
}

impl std::error::Error for HarnessConfigError {}

pub(crate) fn selected_backend() -> Result<Option<String>, HarnessConfigError> {
    parse_string_env(BACKEND_ENV)
}

pub(crate) fn keep_test_dir() -> Result<bool, HarnessConfigError> {
    env_flag(KEEP_TEST_DIR_ENV)
}

pub(crate) fn test_root_override() -> Result<Option<PathBuf>, HarnessConfigError> {
    match env::var_os(TEST_ROOT_ENV) {
        None => Ok(None),
        Some(value) if value.is_empty() => Err(HarnessConfigError::new(
            TEST_ROOT_ENV,
            "path override must not be empty",
        )),
        Some(value) => Ok(Some(PathBuf::from(value))),
    }
}

pub(crate) fn crash_case_limit() -> Result<Option<usize>, HarnessConfigError> {
    parse_env_usize(CRASH_CASES_ENV)
}

pub(crate) fn fault_case_limit() -> Result<Option<usize>, HarnessConfigError> {
    parse_env_usize(FAULT_CASES_ENV)
}

pub(crate) fn stress_seed() -> Result<Option<u64>, HarnessConfigError> {
    parse_env_u64(STRESS_SEED_ENV)
}

pub(crate) fn stress_duration() -> Result<Option<Duration>, HarnessConfigError> {
    parse_env_u64(STRESS_SECONDS_ENV).map(|seconds| seconds.map(Duration::from_secs))
}

fn env_flag(name: &'static str) -> Result<bool, HarnessConfigError> {
    let Some(value) = parse_string_env(name)? else {
        return Ok(false);
    };

    if value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes") {
        Ok(true)
    } else if value == "0"
        || value.eq_ignore_ascii_case("false")
        || value.eq_ignore_ascii_case("no")
    {
        Ok(false)
    } else {
        Err(HarnessConfigError::new(
            name,
            "expected one of 1, 0, true, false, yes, or no",
        ))
    }
}

fn parse_env_usize(name: &'static str) -> Result<Option<usize>, HarnessConfigError> {
    let Some(value) = parse_string_env(name)? else {
        return Ok(None);
    };

    value
        .parse::<usize>()
        .map(Some)
        .map_err(|_| HarnessConfigError::new(name, "expected an unsigned integer"))
}

fn parse_env_u64(name: &'static str) -> Result<Option<u64>, HarnessConfigError> {
    let Some(value) = parse_string_env(name)? else {
        return Ok(None);
    };

    value
        .parse::<u64>()
        .map(Some)
        .map_err(|_| HarnessConfigError::new(name, "expected an unsigned integer"))
}

fn parse_string_env(name: &'static str) -> Result<Option<String>, HarnessConfigError> {
    match env::var(name) {
        Ok(value) if value.trim().is_empty() => Err(HarnessConfigError::new(
            name,
            "value must not be empty or whitespace",
        )),
        Ok(value) => Ok(Some(value)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => {
            Err(HarnessConfigError::new(name, "value must be valid unicode"))
        }
    }
}
