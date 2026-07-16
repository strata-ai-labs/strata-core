//! In-process crash recovery harness entry point: deterministic simulated
//! interruptions with a same-process reopen (fast, exhaustive-friendly).
//! Real process-level kill/reopen coverage lives in
//! `src/testkit/process_crash.rs` (SIGKILL + reopen through the recovery
//! oracle), which runs per-PR with a deep nightly soak.

#![deny(unsafe_code)]

mod common;

#[cfg(all(feature = "testkit", feature = "localfs", not(target_arch = "wasm32")))]
#[test]
fn crash_recovery_harness_reopens_durable_local_objects() {
    let case_limit = common::crash_case_limit()
        .unwrap_or_else(|error| panic!("invalid crash harness environment: {error}"));
    let keep_test_dir = common::keep_test_dir()
        .unwrap_or_else(|error| panic!("invalid crash harness environment: {error}"));
    let test_root_override = common::test_root_override()
        .unwrap_or_else(|error| panic!("invalid crash harness environment: {error}"));

    let tempdir = test_root_override
        .is_none()
        .then(|| tempfile::tempdir().expect("temp crash harness root"));
    let root = test_root_override.unwrap_or_else(|| {
        tempdir
            .as_ref()
            .expect("tempdir exists without override")
            .path()
            .to_path_buf()
    });

    let outcome = strata_storage::testkit::run_localfs_crash_recovery_harness(&root, case_limit)
        .expect("crash recovery harness");

    assert!(outcome.cases_executed() > 0 || case_limit == Some(0));
    if keep_test_dir {
        if let Some(tempdir) = tempdir {
            eprintln!("keeping crash recovery harness root at {}", root.display());
            let _ = tempdir.keep();
        }
    }
}

#[cfg(all(feature = "testkit", feature = "localfs", not(target_arch = "wasm32")))]
fn crash_outcome() -> strata_storage::testkit::CrashRecoveryHarnessOutcome {
    let tempdir = tempfile::tempdir().expect("temp crash harness root");
    strata_storage::testkit::run_localfs_crash_recovery_harness(tempdir.path(), None)
        .expect("crash recovery harness")
}

#[cfg(all(feature = "testkit", feature = "localfs", not(target_arch = "wasm32")))]
#[test]
fn crash_after_wal_append_before_visibility_replays_record() {
    assert!(crash_outcome().log_append_replay_cases() > 0);
}

#[cfg(all(feature = "testkit", feature = "localfs", not(target_arch = "wasm32")))]
#[test]
fn crash_after_wal_append_with_unresolved_gate_reconciles_on_reopen() {
    assert!(crash_outcome().unresolved_gate_reconcile_cases() > 0);
}

#[cfg(all(feature = "testkit", feature = "localfs", not(target_arch = "wasm32")))]
#[test]
fn crash_after_snapshot_publish_before_manifest_update_ignores_orphan_snapshot() {
    assert!(crash_outcome().orphan_snapshot_ignored_cases() > 0);
}

#[cfg(all(feature = "testkit", feature = "localfs", not(target_arch = "wasm32")))]
#[test]
fn crash_after_manifest_update_before_wal_truncation_recovers_checkpoint_and_tail() {
    assert!(crash_outcome().checkpoint_tail_recovered_cases() > 0);
}

#[cfg(all(feature = "testkit", feature = "localfs", not(target_arch = "wasm32")))]
#[test]
fn crash_after_table_publish_before_branch_install_reports_orphan_table() {
    assert!(crash_outcome().orphan_table_reported_cases() > 0);
}

#[cfg(all(feature = "testkit", feature = "localfs", not(target_arch = "wasm32")))]
#[test]
fn crash_after_quarantine_inventory_publish_before_object_move_reports_debt() {
    assert!(crash_outcome().quarantine_inventory_debt_cases() > 0);
}

#[cfg(all(feature = "testkit", feature = "localfs", not(target_arch = "wasm32")))]
#[test]
fn crash_after_object_quarantine_before_purge_preserves_quarantine_entry() {
    assert!(crash_outcome().object_quarantine_preserved_cases() > 0);
}

#[cfg(all(feature = "testkit", feature = "localfs", not(target_arch = "wasm32")))]
#[test]
fn crash_after_close_wal_sync_before_guard_release_reopens_consistently() {
    assert!(crash_outcome().close_reopen_consistent_cases() > 0);
}

#[cfg(all(feature = "testkit", feature = "localfs", not(target_arch = "wasm32")))]
#[test]
fn crash_after_wal_rolls_before_checkpoint_resumes_writer_at_tail() {
    assert!(crash_outcome().stale_wal_pointer_resumed_cases() > 0);
}

#[cfg(all(feature = "testkit", feature = "localfs", not(target_arch = "wasm32")))]
#[test]
fn crash_harness_ignored_cases_have_nonignored_phase_equivalents() {
    assert!(crash_outcome().ignored_case_equivalent_cases() > 0);
}

#[cfg(all(feature = "testkit", feature = "localfs", not(target_arch = "wasm32")))]
#[test]
fn crash_harness_respects_case_limit_and_keep_root_environment() {
    assert!(crash_outcome().harness_environment_cases() > 0);
}
