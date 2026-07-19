//! CLI `doctor` behavior (TCP3.16).
//!
//! `crates/cli/src/doctor.rs` had zero tests, so its four environment/database
//! precondition codes were carried on the error-code guard's allowlist as
//! "deferred". The Phase 3 exit-gate audit's honest re-examination showed that
//! was wrong: all four are reachable hermetically by perturbing one environment
//! axis (`HOME`/`STRATA_HOME`/`PATH`/`--db`) with the same real-binary pattern
//! the 3.11 CLI family tests use. This closes that gap and drops the four codes
//! off the allowlist.

#![deny(unsafe_code)]

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_strata")
}

fn bin_dir() -> PathBuf {
    PathBuf::from(bin())
        .parent()
        .expect("binary directory")
        .to_path_buf()
}

/// Runs `strata --json [--db <db>] doctor` with a controlled environment,
/// returning the parsed report and exit code. The base environment is healthy —
/// the binary's own directory on `PATH`, no stray `STRATA_HOME`/`STRATA_DB` —
/// so each test perturbs exactly one axis to trigger exactly one issue.
fn run_doctor(env: &[(&str, Option<&OsStr>)], db: Option<&Path>) -> (Value, i32) {
    let mut cmd = Command::new(bin());
    cmd.arg("--json");
    if let Some(db) = db {
        // `--db` is a global flag, parsed before the subcommand.
        cmd.arg("--db").arg(db);
    }
    cmd.arg("doctor")
        .env("PATH", bin_dir())
        .env_remove("STRATA_HOME")
        .env_remove("STRATA_DB")
        .env_remove("XDG_CONFIG_HOME");
    for (key, value) in env {
        match value {
            Some(value) => cmd.env(key, value),
            None => cmd.env_remove(key),
        };
    }
    let output = cmd.output().expect("run strata binary");
    let report = serde_json::from_slice(&output.stdout).expect("doctor report is JSON on stdout");
    (report, output.status.code().expect("exit code"))
}

fn issue_codes(report: &Value) -> Vec<String> {
    report["data"]["issues"]
        .as_array()
        .expect("issues array")
        .iter()
        .map(|issue| issue["code"].as_str().expect("issue code").to_owned())
        .collect()
}

#[test]
fn a_healthy_environment_reports_no_issues_and_exits_zero() {
    let home = tempfile::tempdir().expect("temp home");
    let (report, code) = run_doctor(&[("HOME", Some(home.path().as_os_str()))], None);
    assert_eq!(report["type"], "doctor");
    assert_eq!(report["data"]["path_ok"], true);
    assert!(issue_codes(&report).is_empty(), "healthy env has no issues");
    assert_eq!(code, 0);
}

#[test]
fn a_missing_database_target_reports_the_database_path_code() {
    let home = tempfile::tempdir().expect("temp home");
    let missing = home.path().join("no-such-db");
    let (report, code) = run_doctor(&[("HOME", Some(home.path().as_os_str()))], Some(&missing));
    assert!(issue_codes(&report).contains(&"not_found.cli.database_path".to_owned()));
    assert_eq!(report["data"]["database"]["exists"], false);
    assert_eq!(code, 1);
}

#[test]
fn a_non_directory_strata_home_reports_the_home_not_directory_code() {
    let home = tempfile::tempdir().expect("temp home");
    let file = home.path().join("strata-home-is-a-file");
    std::fs::write(&file, b"not a directory").expect("write file");
    let (report, code) = run_doctor(
        &[
            ("HOME", Some(home.path().as_os_str())),
            ("STRATA_HOME", Some(file.as_os_str())),
        ],
        None,
    );
    assert!(issue_codes(&report).contains(&"failed_precondition.cli.home_not_directory".to_owned()));
    assert_eq!(code, 1);
}

#[test]
fn an_unresolvable_home_reports_the_home_unresolved_code() {
    // Neither STRATA_HOME nor HOME set: `strata_home()` cannot resolve.
    let (report, code) = run_doctor(&[("HOME", None), ("STRATA_HOME", None)], None);
    assert!(issue_codes(&report).contains(&"failed_precondition.cli.home_unresolved".to_owned()));
    assert_eq!(report["data"]["home"], Value::Null);
    assert_eq!(code, 1);
}

#[test]
fn a_binary_off_path_reports_the_binary_not_on_path_code() {
    let home = tempfile::tempdir().expect("temp home");
    // A PATH pointing at a directory that does not contain the strata binary.
    let empty = tempfile::tempdir().expect("empty path dir");
    let (report, code) = run_doctor(
        &[
            ("HOME", Some(home.path().as_os_str())),
            ("PATH", Some(empty.path().as_os_str())),
        ],
        None,
    );
    assert!(issue_codes(&report).contains(&"failed_precondition.cli.binary_not_on_path".to_owned()));
    assert_eq!(report["data"]["path_ok"], false);
    assert_eq!(code, 1);
}
