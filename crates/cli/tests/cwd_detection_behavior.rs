//! Bare-invocation cwd awareness (#3000): real binary, controlled cwd.
//!
//! One-shot and piped invocations with no target keep REFUSING (agents never
//! get an implicit write target) — but the refusal now names the dataset the
//! caller is standing in, or the datasets the directory contains. The
//! interactive open-the-cwd path is unit-tested in `open.rs`
//! (`implicit_interactive_target`) and smoke-verified on a pty; these tests
//! pin the non-interactive contract.

use std::path::Path;
use std::process::{Command, Stdio};

use tempfile::TempDir;

fn strata() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_strata"));
    command.env_remove("STRATA_DB");
    command
}

/// Creates a real durable dataset at `path` by writing to it.
fn make_dataset(path: &Path) {
    let status = strata()
        .arg(path)
        .args(["kv", "put", "greeting", "hello"])
        .status()
        .expect("seed write runs");
    assert!(status.success(), "dataset seed write succeeds");
}

#[test]
fn bare_one_shot_inside_a_dataset_still_refuses_but_names_it() {
    let root = TempDir::new().expect("tmp");
    let dataset = root.path().join("sales");
    make_dataset(&dataset);

    let output = strata()
        .current_dir(&dataset)
        .args(["kv", "list"])
        .output()
        .expect("binary runs");
    assert_eq!(
        output.status.code(),
        Some(2),
        "agents keep the refusal: no implicit target"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("current directory is a Strata database"),
        "the refusal must name the dataset underfoot: {stderr}"
    );
    assert!(
        stderr.contains("strata . <command>"),
        "and say how to use it: {stderr}"
    );
}

#[test]
fn bare_one_shot_above_datasets_lists_them() {
    let root = TempDir::new().expect("tmp");
    make_dataset(&root.path().join("alpha"));
    make_dataset(&root.path().join("zeta"));

    let output = strata()
        .current_dir(root.path())
        .args(["kv", "list"])
        .output()
        .expect("binary runs");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Strata datasets here: alpha, zeta"),
        "the refusal must list what the directory contains: {stderr}"
    );
}

#[test]
fn piped_bare_session_inside_a_dataset_keeps_refusing() {
    // The pipe intent is the agent surface: even standing inside a dataset it
    // must never open an implicit target (#3000 keeps the documented posture).
    let root = TempDir::new().expect("tmp");
    let dataset = root.path().join("sales");
    make_dataset(&dataset);

    let output = strata()
        .current_dir(&dataset)
        .stdin(Stdio::piped())
        .output()
        .expect("binary runs");
    assert_ne!(
        output.status.code(),
        Some(0),
        "a bare piped session must refuse"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no database specified"),
        "the refusal stands: {stderr}"
    );
}
