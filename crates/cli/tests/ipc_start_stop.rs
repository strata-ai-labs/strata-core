//! `strata start` / `strata stop`: the headless broker-owner lifecycle pair.
//!
//! `strata start <db>` opens a durable database as a broker owner and blocks,
//! keeping the socket alive so other processes can attach, until `strata stop`
//! (or `ipc stop`) stops the hosting and it exits cleanly. These are real,
//! cross-process invocations of the `strata` binary — the multi-process claims
//! only mean something because every actor is a separate OS process.

#![deny(unsafe_code)]

use std::io::BufRead;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_strata")
}

fn strata(args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .env_remove("STRATA_DB")
        .output()
        .expect("run strata binary")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn db_arg(dir: &Path) -> String {
    dir.join("db").to_string_lossy().into_owned()
}

/// Spawns `strata --db <db> --json start` and blocks until it prints its
/// readiness report, returning the running child and the parsed report. The
/// child is now hosting a broker socket and blocking until stopped.
fn spawn_start_host(db: &str) -> (Child, serde_json::Value) {
    let mut child = Command::new(bin())
        .args(["--db", db, "--json", "start"])
        .env_remove("STRATA_DB")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn strata start");
    let mut reader = std::io::BufReader::new(child.stdout.take().expect("start stdout"));
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .expect("start prints a readiness report before it blocks");
    let report: serde_json::Value = serde_json::from_str(line.trim())
        .unwrap_or_else(|error| panic!("start readiness line is JSON ({error}): {line:?}"));
    (child, report)
}

#[test]
fn start_hosts_a_database_and_stop_stops_it_and_it_exits() {
    let dir = tempfile::tempdir().expect("tmp");
    let db = db_arg(dir.path());
    // Seed the store so it exists durably before the host opens it.
    assert!(
        strata(&["--db", &db, "kv", "put", "seed", "1"])
            .status
            .success(),
        "seed"
    );

    // `strata start` becomes the broker owner and reports it is hosting.
    let (mut host, report) = spawn_start_host(&db);
    assert_eq!(
        report["type"], "ipc_started",
        "readiness envelope: {report}"
    );
    assert_eq!(
        report["data"]["hosting"], true,
        "start reports it is hosting: {report}"
    );
    assert_eq!(
        report["data"]["is_owner"], true,
        "start owns the store it hosts: {report}"
    );
    assert!(
        report["data"]["socket_path"].is_string(),
        "start reports its socket: {report}"
    );

    // A separate one-shot process now BROKERS to the host instead of being
    // refused on the writer lock — one store, two processes.
    let brokered = strata(&["--db", &db, "kv", "put", "contender", "1"]);
    assert!(
        brokered.status.success(),
        "a second process brokers to the start host: stderr={}",
        stderr(&brokered)
    );

    // `strata stop` brokers to the host and tells it to stop hosting.
    let stop = strata(&["--db", &db, "--json", "stop"]);
    assert!(stop.status.success(), "stop: stderr={}", stderr(&stop));
    let parsed: serde_json::Value =
        serde_json::from_str(stdout(&stop).trim()).expect("stop --json parses");
    assert_eq!(parsed["type"], "ipc_stop", "typed envelope: {parsed}");
    assert_eq!(
        parsed["data"]["stopped"], true,
        "stop halted the running host: {parsed}"
    );

    // Once its hosting is stopped, the start process exits cleanly on its own.
    let status = wait_with_timeout(&mut host).expect("start exits after being stopped");
    assert_eq!(status.code(), Some(0), "a stopped host exits cleanly");

    // With the owner gone, the store is free: a fresh process opens it directly
    // and the pre-stop durable data survived.
    let read = strata(&["--db", &db, "kv", "get", "contender"]);
    assert!(read.status.success(), "post-stop read: {}", stderr(&read));
    assert_eq!(stdout(&read).trim(), "1", "the brokered write survived");
}

#[test]
fn stop_on_a_missing_database_reports_not_stopped_and_creates_nothing() {
    let dir = tempfile::tempdir().expect("tmp");
    let missing = dir.path().join("no-such-db");
    let stop = strata(&["--db", &missing.to_string_lossy(), "--json", "stop"]);
    assert!(
        stop.status.success(),
        "stop is forgiving: {}",
        stderr(&stop)
    );
    let parsed: serde_json::Value =
        serde_json::from_str(stdout(&stop).trim()).expect("stop --json parses");
    assert_eq!(parsed["type"], "ipc_stop");
    assert_eq!(
        parsed["data"]["stopped"], false,
        "there was no owner to stop: {parsed}"
    );
    assert!(
        !missing.exists(),
        "stop must not create a database as a side effect"
    );
}

#[test]
fn start_refuses_cache_mode() {
    // A cache database is single-process by construction and hosts nothing, so
    // there is no owner to keep alive.
    let refused = strata(&["--cache", "start"]);
    assert_eq!(
        refused.status.code(),
        Some(2),
        "start --cache is a usage error: {}",
        stderr(&refused)
    );
}

#[test]
fn start_refuses_an_incompatible_ipc_mode() {
    let dir = tempfile::tempdir().expect("tmp");
    let db = db_arg(dir.path());
    // start exists to host; both non-hosting modes contradict that outright.
    for mode in ["client", "off"] {
        let refused = strata(&["--db", &db, "--ipc", mode, "start"]);
        assert_eq!(
            refused.status.code(),
            Some(2),
            "start --ipc {mode} is a usage error: {}",
            stderr(&refused)
        );
    }
}

#[test]
fn stop_refuses_cache_mode() {
    // A cache database has no broker owner to stop.
    let refused = strata(&["--cache", "stop"]);
    assert_eq!(
        refused.status.code(),
        Some(2),
        "stop --cache is a usage error: {}",
        stderr(&refused)
    );
}

#[test]
fn start_and_stop_require_a_database_target() {
    // Neither a positional path, `--db`, nor STRATA_DB: both refuse rather than
    // acting on an implicit current directory.
    for verb in ["start", "stop"] {
        let refused = strata(&[verb]);
        assert_eq!(
            refused.status.code(),
            Some(2),
            "`strata {verb}` with no target is a usage error: {}",
            stderr(&refused)
        );
    }
}

#[test]
fn start_refuses_when_another_process_already_owns_the_store() {
    let dir = tempfile::tempdir().expect("tmp");
    let db = db_arg(dir.path());
    assert!(
        strata(&["--db", &db, "kv", "put", "seed", "1"])
            .status
            .success(),
        "seed"
    );

    // A first host owns the store; a second `strata start` cannot become a
    // second owner — Host mode brokers it in as a client, so it refuses.
    let (mut host, _report) = spawn_start_host(&db);
    let refused = strata(&["--db", &db, "start"]);
    assert_eq!(
        refused.status.code(),
        Some(2),
        "a second start refuses an already-owned store: {}",
        stderr(&refused)
    );
    assert!(
        stderr(&refused).contains("already owns"),
        "the refusal names the already-owned cause (not the bind-failure one): {}",
        stderr(&refused)
    );

    // Tear the host down for a clean exit.
    let _ = strata(&["--db", &db, "stop"]);
    let _ = wait_with_timeout(&mut host);
}

/// Waits up to a few seconds for `child` to exit, returning its status. Kills it
/// on timeout so a hung host never wedges the suite.
fn wait_with_timeout(child: &mut Child) -> Option<std::process::ExitStatus> {
    for _ in 0..300 {
        match child.try_wait().expect("try_wait") {
            Some(status) => return Some(status),
            None => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    None
}
