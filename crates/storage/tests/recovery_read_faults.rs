//! Recovery-time read/list/metadata fault sweep (TCP3.3b).
//!
//! The write-path fault sweep excludes read/list/metadata ops. This sweep
//! covers the other place they matter: open recovery, where the runtime scans
//! the backend to decide what durably exists. A recovery-scan fault must never
//! yield a silently-`Healthy` open — the runtime either fails loudly or reports
//! degraded health. The invariant is asserted inside the harness; a violation
//! panics.

#![deny(unsafe_code)]

#[cfg(all(
    feature = "fault-injection",
    feature = "localfs",
    not(target_arch = "wasm32")
))]
#[test]
fn recovery_scan_faults_never_yield_a_silently_healthy_open() {
    let tempdir = tempfile::tempdir().expect("temp recovery-read-fault root");
    let outcome =
        strata_storage::testkit::run_recovery_read_fault_harness(&tempdir.path().join("db"), 0x51)
            .expect("recovery-read fault harness");

    // Never a vacuous green: the open path must have touched real read ops, the
    // sweep must have failed real positions, and injected faults must have
    // actually fired (otherwise the safety invariant was never exercised).
    assert!(
        outcome.op_kinds_swept() >= 2,
        "open recovery touched fewer than two read/list/metadata op kinds: {outcome:?}"
    );
    assert!(
        outcome.positions_swept() > 0,
        "no recovery-scan positions swept"
    );
    assert!(
        outcome.faults_fired() > 0,
        "no injected fault fired — invariant not exercised: {outcome:?}"
    );
}

/// Soak: the recovery-read sweep across many seeds — the genuine bug-hunt run.
/// `#[ignore]` by default.
#[cfg(all(
    feature = "fault-injection",
    feature = "localfs",
    not(target_arch = "wasm32")
))]
#[ignore = "soak: deep multi-seed recovery-read fault sweep; run with --ignored"]
#[test]
fn recovery_read_fault_soak_deepens_across_many_seeds() {
    let tempdir = tempfile::tempdir().expect("temp recovery-read soak root");
    let outcome = strata_storage::testkit::run_recovery_read_fault_soak(tempdir.path(), 64)
        .expect("recovery-read fault soak");

    assert!(outcome.positions_swept() > 0, "soak swept no positions");
    assert!(
        outcome.faults_fired() > 0,
        "soak fired no faults: {outcome:?}"
    );
}
