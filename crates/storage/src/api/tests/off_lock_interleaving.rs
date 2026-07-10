//! BS2.4 off-lock read protocol — single-threaded interleaving / determinism tests.
//!
//! The multi-threaded concurrency-stress gate (readers racing a committing writer) is BS2.4b.
//! These tests deterministically pin the protocol's key guarantee via direct calls plus the
//! `applied_not_visible` seam (`append_unacked_row_for_test`): a row applied **above** the visible
//! frontier — a commit whose visible publish failed after apply — must be hidden from every
//! off-lock read verb, which bounds by the visible version `V`. This exercises the product path
//! (the off-lock API), complementing the runtime-bridge lifecycle tests.

use super::*;

fn open() -> StorageRuntime<'static> {
    StorageRuntime::open_ephemeral()
        .expect("open ephemeral runtime")
        .into_runtime()
}

/// Seed an acknowledged row at v1 (advances the visible frontier to 1) and a newer row at v5
/// applied above it (does **not** advance the frontier — the `applied_not_visible` shape). Both
/// land in the (Model 2, live) active memtable for key `k`.
fn seed_acked_and_unacked(runtime: &mut StorageRuntime<'static>) {
    runtime
        .append_raw_row_for_test(background_raw_row_with_value(b"k", 1, b"acked".to_vec()))
        .expect("seed acked row (advances visible to 1)");
    runtime
        .append_unacked_row_for_test(background_raw_row_with_value(b"k", 5, b"unacked".to_vec()))
        .expect("apply un-acked row above the frontier");
}

#[test]
fn off_lock_latest_point_hides_applied_not_visible() {
    let mut runtime = open();
    let branch = StorageRuntime::default_branch_id_for_test();
    seed_acked_and_unacked(&mut runtime);

    let outcome = runtime
        .read_point(&PointReadRequest::new(
            branch,
            background_space(),
            key(b"k"),
            ReadBound::Latest,
        ))
        .expect("off-lock latest point read");
    let row = outcome.row().expect("acked row visible");
    assert_eq!(row.value().expect("put row").as_bytes(), b"acked");
    assert_eq!(row.commit_version(), CommitVersion::new(1));
}

#[test]
fn off_lock_latest_scan_hides_applied_not_visible() {
    let mut runtime = open();
    let branch = StorageRuntime::default_branch_id_for_test();
    seed_acked_and_unacked(&mut runtime);

    let scan = runtime
        .scan_prefix(&PrefixScanReadRequest::new(
            branch,
            background_space(),
            key(b"k"),
            ReadBound::Latest,
            None,
        ))
        .expect("off-lock latest prefix scan");
    assert_eq!(scan.rows().len(), 1);
    assert_eq!(
        scan.rows()[0].value().expect("put row").as_bytes(),
        b"acked"
    );
}

#[test]
fn off_lock_history_hides_applied_not_visible() {
    let mut runtime = open();
    let branch = StorageRuntime::default_branch_id_for_test();
    seed_acked_and_unacked(&mut runtime);

    let history = runtime
        .read_history(&HistoryReadRequest::new(
            branch,
            background_space(),
            key(b"k"),
        ))
        .expect("off-lock history read");
    // Only the acknowledged v1 row is visible; the un-acked v5 row is above the visibility bound.
    assert_eq!(history.rows().len(), 1);
    assert_eq!(
        history.rows()[0].value().expect("put row").as_bytes(),
        b"acked"
    );
}

/// BS2.4b snapshot lifetime: a loaded snapshot keeps a table alive after the runtime retires it
/// (flush frozen → owned); the runtime drops its own reference (`RocksDB` `Version::Unref`), and the
/// table dies by `Arc` drop once the snapshot is dropped. Proven via `Arc::strong_count` on the
/// frozen table's backing memtable.
#[test]
fn held_snapshot_keeps_a_retired_table_alive() {
    let mut runtime = StorageRuntime::open_ephemeral()
        .expect("open ephemeral runtime")
        .into_runtime();
    let branch = StorageRuntime::default_branch_id_for_test();
    // Seed a row into the active memtable, then rotate it to a frozen table.
    runtime
        .append_raw_row_for_test(background_raw_row_with_value(b"lifetime", 1, b"v".to_vec()))
        .expect("seed active row");
    runtime
        .rotate_default_branch_for_test()
        .expect("rotate active -> frozen");

    // Hold the published snapshot; it references the frozen table alongside the runtime.
    let held = runtime
        .load_snapshot_for_test(branch)
        .expect("published snapshot");
    assert_eq!(held.frozen_table_count(), 1, "expected one frozen table");
    let before = held
        .frozen_table_strong_count(0)
        .expect("frozen strong count");
    assert_eq!(
        before, 2,
        "frozen table held by both the runtime and the snapshot"
    );

    // Flush retires the frozen table (frozen -> owned) in the runtime; the held snapshot keeps it.
    runtime
        .flush_default_branch_for_test()
        .expect("flush frozen -> owned");
    let after = held
        .frozen_table_strong_count(0)
        .expect("frozen strong count");
    assert_eq!(
        after, 1,
        "the runtime released the frozen table; only the held snapshot keeps it alive"
    );

    drop(held);
    runtime.close().expect("close runtime");
}
