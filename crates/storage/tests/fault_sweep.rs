//! Fault-injection sweeps (STH-2): fail the Nth backend operation a durable
//! workload invokes — once and continuously, across every traced position — and
//! the reopened database must be a prefix of acknowledged commit history (the
//! STH-1 recovery oracle). No acknowledged commit is lost; the faulted commit is
//! present-or-absent atomically; maintenance faults are non-destructive.

#![deny(unsafe_code)]

mod common;

#[cfg(all(
    feature = "fault-injection",
    feature = "localfs",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_sweep_recovers_prefix_of_acknowledged_history() {
    let keep_test_dir = common::keep_test_dir()
        .unwrap_or_else(|error| panic!("invalid fault sweep environment: {error}"));
    let test_root_override = common::test_root_override()
        .unwrap_or_else(|error| panic!("invalid fault sweep environment: {error}"));
    let case_limit = common::fault_case_limit()
        .unwrap_or_else(|error| panic!("invalid fault sweep environment: {error}"));

    let tempdir = test_root_override
        .is_none()
        .then(|| tempfile::tempdir().expect("temp fault sweep root"));
    let root = test_root_override.unwrap_or_else(|| {
        tempdir
            .as_ref()
            .expect("tempdir exists without override")
            .path()
            .to_path_buf()
    });

    // Cap the grid for the CI-fast lane; STRATA_STORAGE_FAULT_CASES overrides
    // (set it higher, or to the full grid, for a deeper local/soak run).
    let outcome = strata_storage::testkit::run_fault_sweep_harness(&root, case_limit.or(Some(96)))
        .expect("fault sweep harness");

    // A budget of 0 is an explicit "run nothing"; otherwise the sweep must trace
    // real backend ops and fail real positions (never a vacuous green).
    assert!(
        outcome.baseline_ops_traced() > 0 || case_limit == Some(0),
        "no backend operations traced to fault"
    );
    assert!(
        outcome.positions_swept() > 0 || case_limit == Some(0),
        "no fault positions swept"
    );
    assert!(
        outcome.budget_pressure_cases() > 0 || case_limit == Some(0),
        "budget-exhaustion case did not apply back-pressure"
    );

    if keep_test_dir {
        if let Some(tempdir) = tempdir {
            eprintln!("keeping fault sweep root at {}", root.display());
            let _ = tempdir.keep();
        }
    }
}

/// Soak: a deep sweep over many seeds — the genuine bug-hunt run. `#[ignore]` by
/// default; `STRATA_STORAGE_FAULT_CASES` sets the depth.
#[cfg(all(
    feature = "fault-injection",
    feature = "localfs",
    not(target_arch = "wasm32")
))]
#[ignore = "soak: deep multi-seed fault sweep; run with --ignored (raise STRATA_STORAGE_FAULT_CASES)"]
#[test]
fn fault_sweep_soak_deepens_across_many_seeds() {
    let case_limit = common::fault_case_limit()
        .unwrap_or_else(|error| panic!("invalid fault sweep environment: {error}"));
    let tempdir = tempfile::tempdir().expect("temp fault sweep soak root");

    let outcome = strata_storage::testkit::run_fault_sweep_harness(
        tempdir.path(),
        case_limit.or(Some(20_000)),
    )
    .expect("fault sweep soak");

    assert!(outcome.positions_swept() > 0, "soak swept no positions");
    // The point of seed scaling: the soak must explore beyond the CI seed budget.
    assert!(
        outcome.seeds_executed() > 4,
        "soak did not deepen beyond the default seed budget (got {} seeds)",
        outcome.seeds_executed()
    );
}
