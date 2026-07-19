//! CLI user-config write-path behavior (TCP3.10c).
//!
//! `strata config set/unset/path/show` run before any database opens and write
//! the global user config (`hub.url`, `<provider>.api_key`). These drive the
//! real binary against a hermetic `HOME` and assert the write path: the file is
//! created 0600, secrets are redacted and never echoed, the environment wins
//! over the stored value, and unset falls back to the built-in default.

#![deny(unsafe_code)]

use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

/// Runs the `strata` binary with a hermetic config home: `HOME` points at a
/// temp dir and every config/env override that could leak from the developer's
/// machine is stripped.
fn config_cli(home: &TempDir, args: &[&str], extra_env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_strata"));
    command
        .args(args)
        .env("HOME", home.path())
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("STRATA_HUB_URL")
        .env_remove("STRATA_DB");
    for (key, value) in extra_env {
        command.env(key, value);
    }
    command.output().expect("run strata binary")
}

fn json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

#[test]
fn hub_url_set_show_unset_roundtrip() {
    let home = tempfile::tempdir().expect("temp home");

    let set = json(&config_cli(
        &home,
        &["--json", "config", "set", "hub.url", "https://hub.example"],
        &[],
    ));
    assert_eq!(set["value"], "https://hub.example");

    // `show` resolves the hub URL and reports which layer supplied it.
    let show = json(&config_cli(&home, &["--json", "config", "show"], &[]));
    assert_eq!(show["hub.url"], "https://hub.example/");
    assert!(
        show["source"]
            .as_str()
            .expect("source string")
            .ends_with("config.toml"),
        "hub.url must be sourced from the config file: {show}"
    );

    config_cli(&home, &["config", "unset", "hub.url"], &[]);
    let after = json(&config_cli(&home, &["--json", "config", "show"], &[]));
    assert_eq!(
        after["source"], "built-in default",
        "after unset, the built-in default supplies the hub URL"
    );
}

#[test]
fn env_var_overrides_the_configured_hub_url() {
    let home = tempfile::tempdir().expect("temp home");
    config_cli(
        &home,
        &["config", "set", "hub.url", "https://config.example"],
        &[],
    );

    let show = json(&config_cli(
        &home,
        &["--json", "config", "show"],
        &[("STRATA_HUB_URL", "https://env.example")],
    ));
    assert_eq!(show["hub.url"], "https://env.example/");
    assert_eq!(
        show["source"], "STRATA_HUB_URL",
        "the environment wins over the stored config"
    );
}

#[cfg(unix)]
#[test]
fn config_file_is_written_0600() {
    use std::os::unix::fs::PermissionsExt;

    let home = tempfile::tempdir().expect("temp home");
    let set = json(&config_cli(
        &home,
        &["--json", "config", "set", "hub.url", "https://hub.example"],
        &[],
    ));
    let path = set["path"].as_str().expect("config path");
    let mode = std::fs::metadata(path)
        .expect("stat config file")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode, 0o600,
        "the user config may hold secrets and must be 0600"
    );
}

#[cfg(feature = "inference")]
#[test]
fn provider_api_key_is_redacted_and_never_echoed() {
    let home = tempfile::tempdir().expect("temp home");
    let output = config_cli(
        &home,
        &[
            "--json",
            "config",
            "set",
            "openai.api_key",
            "sk-topsecret-xyz",
        ],
        &[],
    );
    let rendered = String::from_utf8_lossy(&output.stdout);
    assert!(
        !rendered.contains("sk-topsecret-xyz"),
        "the raw API key must never be echoed back: {rendered}"
    );
    let value = json(&output);
    // Redaction keeps a short non-secret prefix (first 7 chars) plus `****`.
    assert_eq!(value["value"], "sk-tops****");
}

#[test]
fn config_path_reports_the_config_file() {
    let home = tempfile::tempdir().expect("temp home");
    let path = json(&config_cli(&home, &["--json", "config", "path"], &[]));
    assert!(
        path["path"]
            .as_str()
            .expect("path string")
            .ends_with("strata/config.toml"),
        "config path must point at the user config file: {path}"
    );
}

#[test]
fn unknown_config_key_is_rejected() {
    let home = tempfile::tempdir().expect("temp home");
    let output = config_cli(&home, &["config", "set", "bogus.key", "x"], &[]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "an unknown config key is a usage error (exit 2)"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown config key"),
        "error names the bad key: {stderr}"
    );
}
