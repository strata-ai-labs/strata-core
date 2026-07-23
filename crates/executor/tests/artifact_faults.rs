//! TCP4.9a — artifact-level fault sweep + health-vs-truth oracle.
//!
//! The recovery fault seam covers byte-level damage ({corrupt, truncate,
//! io-fail}); #2690 proved *absence* is a distinct, undertested axis — a
//! deleted WAL segment silently erases the database while every health
//! surface stays green. This harness extends the taxonomy to {delete,
//! missing, reorder} at the artifact level, driven coarse-first (whole files
//! and directories, per the `ShardStore` evidence) through the public wire
//! surface: seed a durable database, close it cleanly, sabotage a copy on
//! disk, reopen, and judge.
//!
//! The oracle is **health versus truth**: the harness knows exactly what it
//! wrote (the ground truth), reads it back, and diffs the result against the
//! database's self-reported health. A database that lost data must never
//! self-report healthy — a fault must either leave the truth intact, or
//! refuse/degrade loudly with a permanent, non-retryable classification.
//!
//! Seed (gate 7): #2690 is re-found by this lane — the sole-segment deletion
//! and whole-directory variants both silently wipe while reporting healthy.
//! Both are pinned shrink-only (`pin_2690_*`): the pins assert today's broken
//! behavior exactly, so the watermark fix breaks them and forces their
//! deletion. First-run catch: #2754 (missing current-manifest / snapshot
//! objects refuse open as retryable `unavailable` — permanent damage with
//! retry-forever advice, the #2699 classification pattern on the absence
//! axis), pinned the same way. Positive controls: an unfaulted copy and a
//! deleted writer lock must both open complete and healthy, so the lane
//! cannot drift into treating every removal as fatal.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use strata_executor::{Command, Executor};

// ---------------------------------------------------------------------------
// Ground truth: what the seeded database must contain.
// ---------------------------------------------------------------------------

const KV_KEYS: u64 = 200;
const EVENTS: u64 = 50;
/// `default` plus one created side branch.
const BRANCHES: u64 = 2;

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Minimal standard base64 for wire KV keys/values (no dependency).
fn b64(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let mut buf = [0u8; 3];
        buf[..chunk.len()].copy_from_slice(chunk);
        let n = u32::from(buf[0]) << 16 | u32::from(buf[1]) << 8 | u32::from(buf[2]);
        let chars = [
            B64[(n >> 18) as usize & 63],
            B64[(n >> 12) as usize & 63],
            B64[(n >> 6) as usize & 63],
            B64[n as usize & 63],
        ];
        let keep = chunk.len() + 1;
        for (index, ch) in chars.iter().enumerate() {
            out.push(if index < keep { char::from(*ch) } else { '=' });
        }
    }
    out
}

fn kv_key(index: u64) -> String {
    b64(format!("truth-{index:04}").as_bytes())
}

fn kv_value(index: u64) -> String {
    b64(format!("value-{index:04}-{}", "x".repeat(100)).as_bytes())
}

fn run(executor: &mut Executor, wire: &Value) -> Result<Value, Value> {
    let command: Command =
        serde_json::from_value(wire.clone()).expect("wire JSON parses as a command");
    match executor.execute(command) {
        Ok(output) => Ok(serde_json::to_value(&output).expect("serialize output")),
        Err(error) => Err(serde_json::to_value(error.status()).expect("serialize error status")),
    }
}

fn ok(executor: &mut Executor, wire: &Value) -> Value {
    run(executor, wire).unwrap_or_else(|error| panic!("command must succeed ({wire}): {error}"))
}

/// Seeds the ground truth through the wire and closes cleanly on drop.
fn seed(root: &Path) {
    let mut executor = Executor::open_durable_local(root).expect("durable executor opens");
    for index in 0..KV_KEYS {
        ok(
            &mut executor,
            &json!({"type": "kv_put", "key": kv_key(index), "value": kv_value(index)}),
        );
    }
    for index in 0..EVENTS {
        ok(
            &mut executor,
            &json!({"type": "event_append", "event_type": "truth.tick", "payload": {"i": index}}),
        );
    }
    ok(
        &mut executor,
        &json!({"type": "branch_create", "branch": "side"}),
    );
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create copy dir");
    for entry in std::fs::read_dir(src).expect("read staged dir").flatten() {
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to);
        } else {
            std::fs::copy(&from, &to).expect("copy staged file");
        }
    }
}

/// Stages the seeded database once and hands each fault a fresh byte-copy.
struct Stage {
    #[allow(dead_code)] // owns the tempdir lifetime
    dir: tempfile::TempDir,
    staged: PathBuf,
    copies: u32,
}

impl Stage {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tmp");
        let staged = dir.path().join("staged");
        seed(&staged);
        Self {
            dir,
            staged,
            copies: 0,
        }
    }

    fn copy(&mut self) -> PathBuf {
        self.copies += 1;
        let copy = self.dir.path().join(format!("copy-{}", self.copies));
        copy_dir(&self.staged, &copy);
        copy
    }
}

/// The single WAL segment the CI-sized stage produces.
fn sole_wal_segment(root: &Path) -> PathBuf {
    let mut segments: Vec<PathBuf> = std::fs::read_dir(root.join("wal"))
        .expect("wal dir")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".object@"))
        })
        .collect();
    segments.sort();
    assert_eq!(
        segments.len(),
        1,
        "the CI-sized stage produces one WAL segment; multi-segment faults are the \
         storage-seam increment's territory"
    );
    segments.remove(0)
}

// ---------------------------------------------------------------------------
// The oracle: self-reported health versus read-back truth.
// ---------------------------------------------------------------------------

struct Truth {
    kv_count: u64,
    first_key_intact: bool,
    last_key_intact: bool,
    event_count: u64,
    branch_count: u64,
}

/// Reads the ground truth back through the wire. Every read must succeed
/// (gate 8: no unexpected error on an operation expected to succeed) —
/// what it *returns* is judged by the caller.
fn read_truth(executor: &mut Executor) -> Truth {
    let count = ok(executor, &json!({"type": "kv_count"}));
    let kv_count = count["data"]
        .as_u64()
        .unwrap_or_else(|| panic!("kv_count carries a bare uint in data: {count}"));

    let key_intact = |executor: &mut Executor, index: u64| {
        let got = ok(executor, &json!({"type": "kv_get", "key": kv_key(index)}));
        got["data"]["found"].as_bool() == Some(true)
            && got["data"]["value"]["value"].as_str() == Some(kv_value(index).as_str())
    };
    let first_key_intact = key_intact(executor, 0);
    let last_key_intact = key_intact(executor, KV_KEYS - 1);

    let events = ok(
        executor,
        &json!({"type": "event_range", "start_seq": 0, "direction": "forward", "limit": 1000}),
    );
    let event_count = events["data"]["items"]
        .as_array()
        .map_or(0, |items| items.len() as u64);

    let branches = ok(executor, &json!({"type": "branch_list"}));
    let branch_count = branches["data"]["items"]
        .as_array()
        .map_or(0, |items| items.len() as u64);

    Truth {
        kv_count,
        first_key_intact,
        last_key_intact,
        event_count,
        branch_count,
    }
}

fn assert_truth_complete(truth: &Truth, label: &str) {
    assert_eq!(truth.kv_count, KV_KEYS, "{label}: kv rows lost");
    assert!(truth.first_key_intact, "{label}: first key lost or damaged");
    assert!(truth.last_key_intact, "{label}: last key lost or damaged");
    assert_eq!(truth.event_count, EVENTS, "{label}: events lost");
    assert_eq!(truth.branch_count, BRANCHES, "{label}: branches lost");
}

/// The health command's data object.
fn health(executor: &mut Executor) -> Value {
    ok(executor, &json!({"type": "health"}))["data"].clone()
}

fn health_is_all_green(health: &Value) -> bool {
    health["status"].as_str() == Some("healthy")
        && ["identity", "registry", "branch_catalog", "space_catalog"]
            .iter()
            .all(|surface| health[surface].as_str() == Some("healthy"))
}

/// A refused open must classify the damage as permanent: `corruption` class,
/// not retryable. (#2754 pins the two paths that currently violate this.)
fn assert_permanent_refusal(root: &Path, label: &str) {
    let error = Executor::open_durable_local(root)
        .err()
        .unwrap_or_else(|| panic!("{label}: open must refuse"));
    let status = serde_json::to_value(error.status()).expect("serialize error status");
    assert_eq!(
        status["code"].as_str(),
        Some("corruption.engine.persistence_recovery"),
        "{label}: refusal code diverges: {status}"
    );
    assert_eq!(
        status["retryable"].as_bool(),
        Some(false),
        "{label}: permanent damage must not advise retry: {status}"
    );
}

// ---------------------------------------------------------------------------
// Positive controls
// ---------------------------------------------------------------------------

/// The copy mechanism itself preserves everything: an unfaulted byte-copy
/// opens complete and healthy. Every fault verdict below rests on this.
#[test]
fn control_copy_opens_complete_and_healthy() {
    let mut stage = Stage::new();
    let copy = stage.copy();
    let mut executor = Executor::open_durable_local(&copy).expect("control copy opens");
    assert_truth_complete(&read_truth(&mut executor), "control copy");
    assert!(
        health_is_all_green(&health(&mut executor)),
        "control copy must self-report healthy"
    );
}

/// Not every removal is damage: the writer lock is a runtime artifact,
/// recreated on open. A lane that treats all absence as fatal would be
/// overfit — benign removals must stay benign.
#[test]
fn deleting_the_writer_lock_is_benign() {
    let mut stage = Stage::new();
    let copy = stage.copy();
    std::fs::remove_file(copy.join("locks").join("writer.object@")).expect("delete writer lock");
    let mut executor = Executor::open_durable_local(&copy).expect("open succeeds without the lock");
    assert_truth_complete(&read_truth(&mut executor), "deleted writer lock");
    assert!(
        health_is_all_green(&health(&mut executor)),
        "deleted writer lock must not degrade health"
    );
}

// ---------------------------------------------------------------------------
// The #2690 pins: absence of the WAL is invisible (shrink-only)
// ---------------------------------------------------------------------------

/// Asserts today's #2690 behavior exactly: the database opens, every health
/// surface reports green, and the entire ground truth is gone. This IS the
/// health-vs-truth violation the oracle exists to forbid — pinned until the
/// version-watermark fix lands, at which point this pin breaks (the open
/// must refuse, or health must degrade) and must be deleted.
fn assert_pinned_silent_wipe(root: &Path, label: &str) {
    let mut executor = Executor::open_durable_local(root)
        .unwrap_or_else(|error| panic!("#2690 pin ({label}): today the open succeeds: {error}"));
    let truth = read_truth(&mut executor);
    assert_eq!(truth.kv_count, 0, "#2690 pin ({label}): total kv wipe");
    assert!(
        !truth.first_key_intact && !truth.last_key_intact,
        "#2690 pin ({label}): no key survives"
    );
    assert_eq!(truth.event_count, 0, "#2690 pin ({label}): event log wiped");
    assert_eq!(
        truth.branch_count, 1,
        "#2690 pin ({label}): the side branch is gone too"
    );
    assert!(
        health_is_all_green(&health(&mut executor)),
        "#2690 pin ({label}): the wipe self-reports healthy — the violation this \
         lane exists to catch. If health now degrades, the fix landed: delete this pin"
    );
}

#[test]
fn pin_2690_deleting_the_sole_wal_segment_silently_wipes_while_healthy() {
    let mut stage = Stage::new();
    let copy = stage.copy();
    std::fs::remove_file(sole_wal_segment(&copy)).expect("delete wal segment");
    assert_pinned_silent_wipe(&copy, "sole segment deleted");
}

#[test]
fn pin_2690_missing_wal_directory_silently_wipes_while_healthy() {
    let mut stage = Stage::new();
    let copy = stage.copy();
    std::fs::remove_dir_all(copy.join("wal")).expect("remove wal dir");
    assert_pinned_silent_wipe(&copy, "wal directory missing");
}

// ---------------------------------------------------------------------------
// Absence faults that refuse correctly (the contract to hold)
// ---------------------------------------------------------------------------

/// A missing branch catalog is detected and classified as permanent
/// corruption — proof the artifact-absence detection machinery exists.
#[test]
fn missing_branch_catalog_refuses_open_as_permanent_corruption() {
    let mut stage = Stage::new();
    let copy = stage.copy();
    std::fs::remove_file(copy.join("manifest").join("branch-catalog.object@"))
        .expect("delete branch catalog");
    assert_permanent_refusal(&copy, "missing branch catalog");
}

/// Reorder axis: a WAL segment whose file name disagrees with its content
/// is detected and classified as permanent corruption.
#[test]
fn wal_segment_renamed_to_a_wrong_id_refuses_open_as_permanent_corruption() {
    let mut stage = Stage::new();
    let copy = stage.copy();
    let segment = sole_wal_segment(&copy);
    let renamed = segment.with_file_name("00000000000000ff.object@");
    std::fs::rename(&segment, &renamed).expect("rename wal segment");
    assert_permanent_refusal(&copy, "renamed wal segment");
}

/// Emptying the whole manifest directory refuses as permanent corruption.
#[test]
fn emptying_the_manifest_directory_refuses_open_as_permanent_corruption() {
    let mut stage = Stage::new();
    let copy = stage.copy();
    for entry in std::fs::read_dir(copy.join("manifest"))
        .expect("manifest dir")
        .flatten()
    {
        std::fs::remove_file(entry.path()).expect("delete manifest object");
    }
    assert_permanent_refusal(&copy, "emptied manifest directory");
}

// ---------------------------------------------------------------------------
// The #2754 pins: absence misclassified as transient (shrink-only)
// ---------------------------------------------------------------------------

/// Asserts today's #2754 behavior exactly: the open refuses (loud — good)
/// but classifies permanent artifact loss as a retryable outage, so a caller
/// obeying the advice retries forever. When #2754 lands these refusals
/// become non-retryable corruption: this pin breaks — delete it and move the
/// condition up to `assert_permanent_refusal`.
fn assert_pinned_transient_misclassification(root: &Path, label: &str) {
    let error = Executor::open_durable_local(root)
        .err()
        .unwrap_or_else(|| panic!("#2754 pin ({label}): the open refuses today"));
    let status = serde_json::to_value(error.status()).expect("serialize error status");
    assert_eq!(
        status["code"].as_str(),
        Some("unavailable.engine.persistence"),
        "#2754 pin ({label}): envelope changed — if now corruption/non-retryable, \
         the fix landed: delete this pin and promote to assert_permanent_refusal: {status}"
    );
    assert_eq!(
        status["retryable"].as_bool(),
        Some(true),
        "#2754 pin ({label}): the retry-forever advice is the defect being pinned: {status}"
    );
}

#[test]
fn pin_2754_missing_current_manifest_reports_transient_unavailable() {
    let mut stage = Stage::new();
    let copy = stage.copy();
    std::fs::remove_file(copy.join("manifest").join("current.object@"))
        .expect("delete current manifest");
    assert_pinned_transient_misclassification(&copy, "missing current manifest");
}

#[test]
fn pin_2754_missing_snapshot_objects_report_transient_unavailable() {
    let mut stage = Stage::new();

    let contents_gone = stage.copy();
    for entry in std::fs::read_dir(contents_gone.join("snapshots"))
        .expect("snapshots dir")
        .flatten()
    {
        std::fs::remove_file(entry.path()).expect("delete snapshot object");
    }
    assert_pinned_transient_misclassification(&contents_gone, "snapshot objects deleted");

    let dir_gone = stage.copy();
    std::fs::remove_dir_all(dir_gone.join("snapshots")).expect("remove snapshots dir");
    assert_pinned_transient_misclassification(&dir_gone, "snapshots directory missing");
}
