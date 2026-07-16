//! Failure-during-failure (STH-5): stage a crash whose checkpoint publish
//! already failed, then inject a second fault into the reopen's recovery path;
//! separately, fault every write position inside the maintenance publish
//! transitions. Every case must end oracle-valid (an exact prefix of
//! acknowledged history), the intermediate failure must be typed — a drain
//! error, a failed summary, or a recorded source error on a completed pass
//! (manifest publish debt) — and the store must accept writes afterwards.

#![deny(unsafe_code)]

mod common;

#[cfg(all(
    feature = "fault-injection",
    feature = "localfs",
    not(target_arch = "wasm32")
))]
#[test]
fn second_fault_during_recovery_stays_oracle_valid_and_resumable() {
    let case_limit = common::compound_case_limit()
        .unwrap_or_else(|error| panic!("invalid compound fault environment: {error}"));
    let tempdir = tempfile::tempdir().expect("temp compound fault root");

    // Cap the grid for the CI-fast lane; STRATA_STORAGE_COMPOUND_CASES
    // overrides for deeper local/soak runs.
    let outcome = strata_storage::testkit::run_compound_fault_recovery_sweep(
        tempdir.path(),
        case_limit.or(Some(48)),
    )
    .expect("compound fault recovery sweep");

    assert!(
        outcome.staged_crashes() > 0 || case_limit == Some(0),
        "no first-failure crashes staged"
    );
    assert!(
        outcome.positions_swept() > 0 || case_limit == Some(0),
        "no second-fault positions swept"
    );
    assert!(
        outcome.faulted_opens_failed_typed() > 0 || case_limit == Some(0),
        "no recovery fault ever failed an open; the sweep is vacuous"
    );
    assert_eq!(
        outcome.resumes_verified(),
        outcome.positions_swept(),
        "every compound case must verify resumability"
    );
}

#[cfg(all(
    feature = "fault-injection",
    feature = "localfs",
    not(target_arch = "wasm32")
))]
#[test]
fn maintenance_publish_faults_stay_typed_resumable_and_oracle_valid() {
    let tempdir = tempfile::tempdir().expect("temp compound maintenance root");

    let outcome = strata_storage::testkit::run_compound_fault_maintenance_cases(tempdir.path(), 17)
        .expect("compound maintenance cases");

    assert!(outcome.positions_swept() > 0, "no positions swept");
    assert_eq!(
        outcome.faults_surfaced_typed(),
        outcome.positions_swept(),
        "every maintenance fault must leave a typed trace"
    );
    assert_eq!(
        outcome.in_session_resumes() + outcome.reopen_resumes(),
        outcome.positions_swept(),
        "every maintenance case must resume"
    );
}

/// Soak: many seeds × the full second-fault grid — the genuine compound
/// bug-hunt. `#[ignore]` by default; `STRATA_STORAGE_COMPOUND_CASES` sets the
/// depth.
#[cfg(all(
    feature = "fault-injection",
    feature = "localfs",
    not(target_arch = "wasm32")
))]
#[ignore = "soak: deep multi-seed compound fault sweep; run with --ignored (raise STRATA_STORAGE_COMPOUND_CASES)"]
#[test]
fn compound_fault_soak_deepens_across_many_seeds() {
    let case_limit = common::compound_case_limit()
        .unwrap_or_else(|error| panic!("invalid compound fault environment: {error}"));
    let tempdir = tempfile::tempdir().expect("temp compound soak root");

    let outcome = strata_storage::testkit::run_compound_fault_recovery_sweep(
        tempdir.path(),
        case_limit.or(Some(2_000)),
    )
    .expect("compound fault soak");

    assert!(outcome.positions_swept() > 0, "soak swept no positions");
    assert!(
        outcome.seeds_executed() > 2,
        "soak did not deepen beyond the default seed budget (got {} seeds)",
        outcome.seeds_executed()
    );
}
