//! Fix #2756 — per-commit durability attestation and the `Always` knob.
//!
//! The old receipt folded storage's four-state durability answer into a
//! `durable` boolean that reported unsynced Standard-mode commits as
//! durable, so SDK callers treated acknowledgements as crash-safe and
//! SIGKILL silently erased them. The contract now: every commit outcome
//! carries the durability storage actually attested, and
//! `DurabilityMode::Always` (newly reachable through the public open
//! options) syncs before acknowledging — an `Always` attestation is a
//! survival guarantee.
//!
//! The SIGKILL tests are the issue's acceptance criteria, subprocess-driven
//! (the TCP2.1 re-invoke pattern): a child writes and parks without
//! closing; the parent SIGKILLs it mid-life and reopens the store.
//! `Always`: every acknowledged commit survives. `Standard`: loss of the
//! unsynced tail is contract-conformant *because the receipts said
//! `Standard`* — and the survivors must form a prefix of the acknowledged
//! order (the WAL flushes sequentially; a gap would be corruption, not the
//! documented window).
#![cfg(feature = "localfs")]

use std::io::BufRead;
use std::path::Path;

use strata_engine::{
    BranchName, CacheOpenOptions, CommitDurability, Database, DatabaseOpenOutcome, DurabilityMode,
    DurableLocalOpenOptions, KvKey, KvValue, ProductSpace,
};

const KEYS: u32 = 50;
const DIR_ENV: &str = "STRATA_COMMIT_DURABILITY_DIR";
const MODE_ENV: &str = "STRATA_COMMIT_DURABILITY_MODE";

fn branch() -> BranchName {
    BranchName::new("default").expect("valid branch name")
}

fn space() -> ProductSpace {
    ProductSpace::new("default").expect("valid product space")
}

fn kv_key(index: u32) -> KvKey {
    KvKey::new(format!("ack-{index:04}").as_bytes()).expect("valid key")
}

fn kv_value(index: u32) -> KvValue {
    KvValue::new(format!("value-{index:04}").into_bytes())
}

fn open_with_mode(root: &Path, mode: DurabilityMode) -> Database {
    Database::open_local(root, DurableLocalOpenOptions::new().with_durability(mode))
        .map(DatabaseOpenOutcome::into_database)
        .expect("durable open")
}

// ---------------------------------------------------------------------------
// Attestation per mode
// ---------------------------------------------------------------------------

#[test]
fn cache_commits_attest_not_durable() {
    let mut db = Database::open_cache(CacheOpenOptions::new())
        .map(DatabaseOpenOutcome::into_database)
        .expect("cache open");
    let mut kv = db.kv(branch(), space()).expect("kv opens");
    let outcome = kv.put(kv_key(0), kv_value(0)).expect("put succeeds");
    assert_eq!(outcome.commit().durability(), CommitDurability::NotDurable);
}

#[test]
fn standard_commits_attest_standard_not_durable_now() {
    let dir = tempfile::tempdir().expect("tmp");
    let mut db = open_with_mode(&dir.path().join("db"), DurabilityMode::Standard);
    let mut kv = db.kv(branch(), space()).expect("kv opens");
    let outcome = kv.put(kv_key(0), kv_value(0)).expect("put succeeds");
    // The #2756 defect: this attested `durable` before the sync point.
    assert_eq!(outcome.commit().durability(), CommitDurability::Standard);
}

#[test]
fn always_commits_attest_always() {
    let dir = tempfile::tempdir().expect("tmp");
    let mut db = open_with_mode(&dir.path().join("db"), DurabilityMode::Always);
    let mut kv = db.kv(branch(), space()).expect("kv opens");
    let outcome = kv.put(kv_key(0), kv_value(0)).expect("put succeeds");
    assert_eq!(outcome.commit().durability(), CommitDurability::Always);
}

// ---------------------------------------------------------------------------
// SIGKILL acceptance (subprocess phases)
// ---------------------------------------------------------------------------

/// Child: write every key in the selected mode, assert each receipt's
/// attestation, report acknowledgement, then park without closing so the
/// parent can SIGKILL a live process.
#[test]
#[ignore = "subprocess phase: re-invoked by the SIGKILL tests with STRATA_COMMIT_DURABILITY_DIR"]
fn phase_write_and_park() {
    let Ok(dir) = std::env::var(DIR_ENV) else {
        return;
    };
    let mode = match std::env::var(MODE_ENV).as_deref() {
        Ok("always") => DurabilityMode::Always,
        _ => DurabilityMode::Standard,
    };
    let expected = match mode {
        DurabilityMode::Always => CommitDurability::Always,
        _ => CommitDurability::Standard,
    };
    let root = Path::new(&dir).join("db");
    let mut db = open_with_mode(&root, mode);
    {
        let mut kv = db.kv(branch(), space()).expect("kv opens");
        for index in 0..KEYS {
            let outcome = kv
                .put(kv_key(index), kv_value(index))
                .expect("put succeeds");
            assert_eq!(
                outcome.commit().durability(),
                expected,
                "receipt must attest the open mode's durability"
            );
        }
    }
    println!("ACKED {KEYS}");
    // Park un-closed; the parent SIGKILLs here. Only an `Always` receipt
    // promises these acknowledgements outlive the kill.
    std::thread::sleep(std::time::Duration::from_secs(300));
}

/// Spawns the write child, SIGKILLs it after acknowledgement, and returns
/// how many acknowledged keys survive reopen — verifying the survivors are
/// an exact prefix with intact values as it counts.
fn acked_survivors_after_sigkill(root: &Path, mode: &str) -> u32 {
    let exe = std::env::current_exe().expect("current test binary");
    let mut child = std::process::Command::new(exe)
        .args([
            "phase_write_and_park",
            "--exact",
            "--ignored",
            "--nocapture",
        ])
        .env(DIR_ENV, root)
        .env(MODE_ENV, mode)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn write child");
    let stdout = child.stdout.take().expect("child stdout");
    let mut lines = std::io::BufReader::new(stdout).lines();
    loop {
        let line = lines
            .next()
            .expect("child exited before acknowledging")
            .expect("read child stdout");
        if line.starts_with("ACKED") {
            break;
        }
    }
    child.kill().expect("SIGKILL child");
    child.wait().expect("reap child");

    let mut db = open_with_mode(&root.join("db"), DurabilityMode::Standard);
    let mut kv = db.kv(branch(), space()).expect("kv opens after recovery");
    let mut survivors = 0u32;
    let mut lost_seen = false;
    for index in 0..KEYS {
        match kv.get(&kv_key(index)).expect("read succeeds") {
            Some(value) => {
                assert!(
                    !lost_seen,
                    "key {index} survived after an earlier key was lost — survivors \
                     must be a prefix of the acknowledged order"
                );
                assert_eq!(
                    value.as_bytes(),
                    kv_value(index).as_bytes(),
                    "surviving key {index} must be undamaged"
                );
                survivors += 1;
            }
            None => lost_seen = true,
        }
    }
    survivors
}

/// The `Always` contract (#2756 acceptance): an acknowledged commit whose
/// receipt attests `always` survives SIGKILL — all of them, every time.
#[test]
fn always_mode_acknowledgements_survive_sigkill() {
    let dir = tempfile::tempdir().expect("tmp");
    let survivors = acked_survivors_after_sigkill(dir.path(), "always");
    assert_eq!(
        survivors, KEYS,
        "a commit attested `always` must survive process kill"
    );
}

/// The `Standard` contract, now honest: unsynced acknowledgements may die
/// with the process — permitted precisely because every receipt attested
/// `standard`, not `always`. The prefix and value checks inside the helper
/// keep the permitted loss window from hiding corruption.
#[test]
fn standard_mode_loss_window_is_honestly_attested() {
    let dir = tempfile::tempdir().expect("tmp");
    let survivors = acked_survivors_after_sigkill(dir.path(), "standard");
    assert!(
        survivors <= KEYS,
        "recovery cannot invent commits ({survivors} > {KEYS})"
    );
}

/// Clean-close control: without a kill, Standard mode recovers everything —
/// the loss window is the kill window, nothing else.
#[test]
fn standard_mode_clean_close_recovers_all_acknowledgements() {
    let dir = tempfile::tempdir().expect("tmp");
    let root = dir.path().join("db");
    {
        let mut db = open_with_mode(&root, DurabilityMode::Standard);
        let mut kv = db.kv(branch(), space()).expect("kv opens");
        for index in 0..KEYS {
            kv.put(kv_key(index), kv_value(index))
                .expect("put succeeds");
        }
        db.close().expect("clean close");
    }
    let mut db = open_with_mode(&root, DurabilityMode::Standard);
    let mut kv = db.kv(branch(), space()).expect("kv opens after reopen");
    for index in 0..KEYS {
        assert!(
            kv.get(&kv_key(index)).expect("read succeeds").is_some(),
            "key {index} lost across a clean close"
        );
    }
}
