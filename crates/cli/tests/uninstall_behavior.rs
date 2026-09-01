//! `strata uninstall` behavior (#2995): real binary against a hermetic HOME.
//!
//! The V1 port of the pre-V1 command: inventory + confirmation, removal of the
//! Strata home / legacy dirs / REPL history, PATH-block cleanup matching what
//! `install.sh` appends, `~/.config/strata/` and per-project databases kept,
//! and the usage refusals (database targets; in-session invocation).

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use tempfile::TempDir;

fn strata(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_strata"));
    command
        .env("HOME", home)
        .env_remove("STRATA_HOME")
        .env_remove("STRATA_INSTALL_DIR")
        .env_remove("STRATA_DB");
    command
}

/// The exact block `install.sh` appends (expanded install dir, bash form).
fn strata_path_block(home: &Path) -> String {
    format!(
        "\n# Strata\nexport PATH=\"{}/.strata/bin:$PATH\"\n",
        home.display()
    )
}

#[test]
fn uninstall_refuses_a_database_target() {
    let home = TempDir::new().expect("home");
    for args in [
        vec!["--db", "./some-db", "uninstall"],
        vec!["--cache", "uninstall"],
    ] {
        let output = strata(home.path())
            .args(&args)
            .output()
            .expect("binary runs");
        assert_eq!(
            output.status.code(),
            Some(2),
            "database-targeted uninstall must be a usage refusal: {args:?}"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("does not take a database target"),
            "unexpected stderr for {args:?}: {stderr}"
        );
    }
}

#[test]
fn uninstall_without_confirmation_aborts_and_removes_nothing() {
    let home = TempDir::new().expect("home");
    let strata_dir = home.path().join(".strata");
    fs::create_dir_all(strata_dir.join("bin")).expect("bin dir");
    fs::write(strata_dir.join("bin/strata"), b"binary").expect("binary");

    // Empty stdin: read_line sees EOF, which must abort, never proceed.
    let output = strata(home.path())
        .args(["uninstall", "--json"])
        .stdin(Stdio::null())
        .output()
        .expect("binary runs");
    assert_eq!(output.status.code(), Some(0), "abort is a healthy exit");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"aborted\": true") || stdout.contains("\"aborted\":true"),
        "abort must be reported in the summary: {stdout}"
    );
    assert!(
        strata_dir.exists(),
        "an aborted uninstall must remove nothing"
    );
}

#[test]
fn uninstall_with_yes_removes_installation_and_cleans_path_block() {
    let home = TempDir::new().expect("home");
    let strata_dir = home.path().join(".strata");
    fs::create_dir_all(strata_dir.join("bin")).expect("bin dir");
    fs::write(strata_dir.join("bin/strata"), b"binary").expect("binary");
    fs::write(home.path().join(".strata_history"), b"kv get x\n").expect("history");

    // A bashrc carrying unrelated lines plus the installer's block.
    let bashrc = home.path().join(".bashrc");
    let mut contents = String::from("# my stuff\nexport FOO=bar\n");
    contents.push_str(&strata_path_block(home.path()));
    fs::write(&bashrc, &contents).expect("bashrc");

    // A zshrc with no marker: must not be rewritten at all.
    let zshrc = home.path().join(".zshrc");
    fs::write(&zshrc, "# untouched\n").expect("zshrc");

    // User config survives (install.sh documents it is never touched).
    let config_dir = home.path().join(".config/strata");
    fs::create_dir_all(&config_dir).expect("config dir");
    fs::write(config_dir.join("config.toml"), b"[hub]\n").expect("config");

    let output = strata(home.path())
        .args(["uninstall", "--yes", "--json"])
        .output()
        .expect("binary runs");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(!strata_dir.exists(), "the Strata home must be removed");
    assert!(
        !home.path().join(".strata_history").exists(),
        "REPL history must be removed"
    );
    assert_eq!(
        fs::read_to_string(&bashrc).expect("bashrc survives"),
        "# my stuff\nexport FOO=bar\n",
        "only the Strata block leaves the bashrc"
    );
    assert_eq!(
        fs::read_to_string(&zshrc).expect("zshrc survives"),
        "# untouched\n",
        "a marker-free config must not be rewritten"
    );
    assert!(
        config_dir.join("config.toml").exists(),
        "~/.config/strata is kept across uninstall"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(".bashrc"),
        "the summary names the cleaned config: {stdout}"
    );
}

#[test]
fn uninstall_removes_an_external_install_dir() {
    // Kills the filter-negation mutant on the STRATA_INSTALL_DIR handling: a
    // dir OUTSIDE the Strata home must be part of the removal set.
    let home = TempDir::new().expect("home");
    let external = TempDir::new().expect("external root");
    let bin_dir = external.path().join("strata-bin");
    fs::create_dir_all(&bin_dir).expect("bin dir");
    fs::write(bin_dir.join("strata"), b"binary").expect("binary");

    let output = strata(home.path())
        .env("STRATA_INSTALL_DIR", &bin_dir)
        .args(["uninstall", "--yes", "--json"])
        .output()
        .expect("binary runs");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !bin_dir.exists(),
        "a STRATA_INSTALL_DIR outside the Strata home is removed"
    );
}

#[test]
fn uninstall_inside_a_session_is_refused() {
    let home = TempDir::new().expect("home");
    let mut child = strata(home.path())
        .arg("--cache")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("REPL spawns");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"uninstall\nquit\n")
        .expect("write lines");
    let output = child.wait_with_output().expect("REPL exits");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("outside of a database session"),
        "in-session uninstall must be refused: {stderr}"
    );
    // Piped sessions propagate a failed line as a failing exit (script
    // semantics); the refusal above is the contract under test.
    assert_eq!(output.status.code(), Some(1));
}
