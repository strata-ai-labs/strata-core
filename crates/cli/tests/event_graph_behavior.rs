//! CLI event + graph family behavior (TCP3.11b).
//!
//! Ports the event and graph workflows from the shell scenario corpus
//! (`scripts/cli-corpus/04`) into real-binary integration tests. Neither family
//! had Rust integration coverage. The corpus that first exercised them surfaced
//! the event chain field rename (`valid`, not `is_valid`) and event
//! sequence-cursor pagination, both pinned here, plus graph neighbor direction/
//! edge-type filtering and per-branch divergence.

#![deny(unsafe_code)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

fn strata(db: &Path, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_strata"))
        .arg("--db")
        .arg(db)
        .arg("--json")
        .args(args)
        .env_remove("STRATA_DB")
        .output()
        .expect("run strata binary");
    assert!(
        output.status.success(),
        "command {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

fn db(home: &TempDir) -> std::path::PathBuf {
    home.path().join("db")
}

/// Sequence numbers of an `event_records`/`event_range_result` page, in order.
fn sequences(page: &Value) -> Vec<u64> {
    page["data"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["event"]["sequence"].as_u64().expect("sequence"))
        .collect()
}

/// Neighbor/node ids of a graph page, sorted.
fn sorted_node_ids(page: &Value) -> Vec<String> {
    let mut ids: Vec<String> = page["data"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["node_id"].as_str().expect("node_id").to_owned())
        .collect();
    ids.sort();
    ids
}

// --- events ---------------------------------------------------------------

#[test]
fn event_append_assigns_monotonic_sequences() {
    let home = tempfile::tempdir().expect("temp home");
    let db = db(&home);
    let first = strata(&db, &["event", "append", "user.created", r#"{"u":"ada"}"#]);
    assert_eq!(first["type"], "event_append_result");
    assert_eq!(first["data"]["sequence"], 0);
    strata(&db, &["event", "append", "user.updated", r#"{"u":"ada"}"#]);
    strata(&db, &["event", "append", "system.audit", r#"{"a":"x"}"#]);

    let all = strata(&db, &["event", "list", "--limit", "10"]);
    assert_eq!(sequences(&all), vec![0, 1, 2]);
}

#[test]
fn event_list_paginates_by_sequence_cursor() {
    let home = tempfile::tempdir().expect("temp home");
    let db = db(&home);
    for i in 0..3 {
        strata(
            &db,
            &["event", "append", "e.type", &format!(r#"{{"i":{i}}}"#)],
        );
    }

    let first = strata(&db, &["event", "list", "--limit", "2"]);
    assert_eq!(first["type"], "event_records");
    assert_eq!(sequences(&first), vec![0, 1]);
    assert_eq!(first["data"]["has_more"], true);
    assert_eq!(first["data"]["cursor"], 1);

    let second = strata(&db, &["event", "list", "--limit", "2", "--cursor", "1"]);
    assert_eq!(sequences(&second), vec![2]);
    assert_eq!(second["data"]["has_more"], false);
    assert_eq!(second["data"]["cursor"], Value::Null);
}

#[test]
fn event_by_type_filters_and_reverse_range_orders() {
    let home = tempfile::tempdir().expect("temp home");
    let db = db(&home);
    strata(&db, &["event", "append", "user.created", r#"{"i":0}"#]);
    strata(&db, &["event", "append", "user.updated", r#"{"i":1}"#]);
    strata(&db, &["event", "append", "system.audit", r#"{"i":2}"#]);

    let by_type = strata(&db, &["event", "by-type", "user.updated", "--limit", "5"]);
    let items = by_type["data"]["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["event"]["event_type"], "user.updated");

    let reverse = strata(
        &db,
        &[
            "event",
            "range",
            "2",
            "--direction",
            "reverse",
            "--limit",
            "2",
        ],
    );
    assert_eq!(reverse["type"], "event_range_result");
    assert_eq!(sequences(&reverse), vec![2, 1]);
}

#[test]
fn event_verify_chain_reports_valid_not_is_valid() {
    let home = tempfile::tempdir().expect("temp home");
    let db = db(&home);
    strata(&db, &["event", "append", "e.one", r#"{"i":0}"#]);
    strata(&db, &["event", "append", "e.two", r#"{"i":1}"#]);

    let verified = strata(&db, &["event", "verify-chain"]);
    assert_eq!(verified["type"], "event_chain_verification");
    assert_eq!(verified["data"]["valid"], true);
    // The field was renamed from `is_valid` to `valid`; the old key must be gone.
    assert!(verified["data"].get("is_valid").is_none());
}

#[test]
fn event_count_is_isolated_per_branch() {
    let home = tempfile::tempdir().expect("temp home");
    let db = db(&home);
    for i in 0..3 {
        strata(
            &db,
            &["event", "append", "e.type", &format!(r#"{{"i":{i}}}"#)],
        );
    }
    strata(&db, &["branch", "fork", "default", "event-child"]);
    strata(
        &db,
        &[
            "--branch",
            "event-child",
            "event",
            "append",
            "child.only",
            r#"{"b":"child"}"#,
        ],
    );

    assert_eq!(strata(&db, &["event", "count"])["data"]["count"], 3);
    assert_eq!(
        strata(&db, &["--branch", "event-child", "event", "count"])["data"]["count"],
        4
    );
}

// --- graph ----------------------------------------------------------------

fn seed_social(db: &Path) {
    strata(db, &["graph", "create", "social"]);
    for node in ["ada", "bob", "carol", "dave", "erin"] {
        strata(
            db,
            &[
                "graph",
                "add-node",
                "social",
                node,
                "--properties",
                r#"{"kind":"person"}"#,
            ],
        );
    }
    strata(
        db,
        &[
            "graph", "add-edge", "social", "ada", "follows", "bob", "--weight", "0.4",
        ],
    );
    strata(
        db,
        &[
            "graph", "add-edge", "social", "carol", "follows", "ada", "--weight", "0.9",
        ],
    );
    strata(
        db,
        &[
            "graph", "add-edge", "social", "ada", "mentors", "dave", "--weight", "1.0",
        ],
    );
}

#[test]
fn graph_create_node_and_edge_report_typed_results() {
    let home = tempfile::tempdir().expect("temp home");
    let db = db(&home);
    let created = strata(&db, &["graph", "create", "social"]);
    assert_eq!(created["type"], "graph_create_result");
    assert_eq!(created["data"]["info"]["graph"], "social");

    let node = strata(&db, &["graph", "add-node", "social", "ada"]);
    assert_eq!(node["data"]["node_id"], "ada");
    let edge = strata(&db, &["graph", "add-node", "social", "bob"]);
    assert_eq!(edge["data"]["node_id"], "bob");
    let added = strata(
        &db,
        &[
            "graph", "add-edge", "social", "ada", "follows", "bob", "--weight", "0.4",
        ],
    );
    assert_eq!(added["data"]["edge_type"], "follows");
    assert_eq!(added["data"]["dst"], "bob");
}

#[test]
fn graph_list_nodes_paginates_by_cursor() {
    let home = tempfile::tempdir().expect("temp home");
    let db = db(&home);
    seed_social(&db);

    let first = strata(&db, &["graph", "list-nodes", "social", "--limit", "2"]);
    assert_eq!(first["type"], "graph_node_page");
    assert_eq!(sorted_node_ids(&first), vec!["ada", "bob"]);
    assert_eq!(first["data"]["has_more"], true);

    let second = strata(
        &db,
        &[
            "graph",
            "list-nodes",
            "social",
            "--limit",
            "2",
            "--cursor",
            "bob",
        ],
    );
    assert_eq!(sorted_node_ids(&second), vec!["carol", "dave"]);
}

#[test]
fn graph_neighbors_filter_by_direction_and_edge_type() {
    let home = tempfile::tempdir().expect("temp home");
    let db = db(&home);
    seed_social(&db);

    let outgoing = strata(
        &db,
        &[
            "graph",
            "neighbors",
            "social",
            "ada",
            "--direction",
            "outgoing",
            "--limit",
            "10",
        ],
    );
    assert_eq!(outgoing["type"], "graph_neighbor_page");
    assert_eq!(sorted_node_ids(&outgoing), vec!["bob", "dave"]);

    let incoming = strata(
        &db,
        &[
            "graph",
            "neighbors",
            "social",
            "ada",
            "--direction",
            "incoming",
            "--limit",
            "10",
        ],
    );
    assert_eq!(sorted_node_ids(&incoming), vec!["carol"]);

    let mentored = strata(
        &db,
        &[
            "graph",
            "neighbors",
            "social",
            "ada",
            "--edge-type",
            "mentors",
            "--limit",
            "10",
        ],
    );
    assert_eq!(sorted_node_ids(&mentored), vec!["dave"]);
}

#[test]
fn graph_edits_on_a_fork_do_not_touch_the_parent() {
    let home = tempfile::tempdir().expect("temp home");
    let db = db(&home);
    seed_social(&db);

    strata(&db, &["branch", "fork", "default", "graph-child"]);
    let child = |args: &[&str]| {
        let mut full = vec!["--branch", "graph-child"];
        full.extend_from_slice(args);
        strata(&db, &full)
    };
    child(&["graph", "remove-edge", "social", "ada", "follows", "bob"]);
    child(&[
        "graph",
        "add-node",
        "social",
        "frank",
        "--properties",
        r#"{"kind":"person"}"#,
    ]);
    child(&[
        "graph", "add-edge", "social", "ada", "follows", "frank", "--weight", "0.8",
    ]);

    // The parent branch is unchanged; the child diverges (bob dropped, frank added).
    let parent = strata(
        &db,
        &[
            "graph",
            "neighbors",
            "social",
            "ada",
            "--direction",
            "outgoing",
            "--limit",
            "10",
        ],
    );
    assert_eq!(sorted_node_ids(&parent), vec!["bob", "dave"]);
    let forked = child(&[
        "graph",
        "neighbors",
        "social",
        "ada",
        "--direction",
        "outgoing",
        "--limit",
        "10",
    ]);
    assert_eq!(sorted_node_ids(&forked), vec!["dave", "frank"]);
}
