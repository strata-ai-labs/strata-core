//! Deterministic simulation (STH-4): a seeded explorer over the production
//! lifecycle path. Each seed drives client commits interleaved with maintenance
//! (drained at a sim-chosen cadence) and seeded clock advancement against the
//! inline executor on the real `Background` logic, asserting the STH-1 recovery
//! oracle (safety) and queue progress (liveness) every step. Every trajectory is a
//! pure function of its seed, so any failure replays bit-exact (the determinism
//! guard lives in the crate's `simulation` unit tests).

#![deny(unsafe_code)]

mod common;

#[cfg(all(
    feature = "fault-injection",
    feature = "localfs",
    not(target_arch = "wasm32")
))]
#[test]
fn simulation_holds_oracle_and_liveness_across_seeds() {
    let keep_test_dir = common::keep_test_dir()
        .unwrap_or_else(|error| panic!("invalid simulation environment: {error}"));
    let test_root_override = common::test_root_override()
        .unwrap_or_else(|error| panic!("invalid simulation environment: {error}"));
    let case_limit = common::fault_case_limit()
        .unwrap_or_else(|error| panic!("invalid simulation environment: {error}"));

    let tempdir = test_root_override
        .is_none()
        .then(|| tempfile::tempdir().expect("temp simulation root"));
    let root = test_root_override.unwrap_or_else(|| {
        tempdir
            .as_ref()
            .expect("tempdir exists without override")
            .path()
            .to_path_buf()
    });

    // Cap the seed set for the CI-fast lane; STRATA_STORAGE_FAULT_CASES overrides
    // (set it higher, or to the full grid, for a deeper local/soak run).
    let outcome =
        strata_storage_next::testkit::run_simulation_harness(&root, case_limit.or(Some(16)))
            .expect("simulation harness");

    // A budget of 0 is an explicit "run nothing"; otherwise the sweep must run real
    // trajectories that complete maintenance and advance the clock (never vacuous).
    assert!(
        outcome.seeds_executed() > 0 || case_limit == Some(0),
        "no simulation seeds executed"
    );
    assert!(
        outcome.maintenance_completed() > 0 || case_limit == Some(0),
        "no maintenance completed across the sweep"
    );
    assert!(
        outcome.clock_advances() > 0 || case_limit == Some(0),
        "manual maintenance clock never advanced"
    );

    if keep_test_dir {
        if let Some(tempdir) = tempdir {
            eprintln!("keeping simulation root at {}", root.display());
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
#[ignore = "soak: deep multi-seed simulation; run with --ignored (raise STRATA_STORAGE_FAULT_CASES)"]
#[test]
fn simulation_soak_deepens_across_many_seeds() {
    let case_limit = common::fault_case_limit()
        .unwrap_or_else(|error| panic!("invalid simulation environment: {error}"));
    let tempdir = tempfile::tempdir().expect("temp simulation soak root");

    let outcome = strata_storage_next::testkit::run_simulation_harness(
        tempdir.path(),
        case_limit.or(Some(2_000)),
    )
    .expect("simulation soak");

    assert!(
        outcome.maintenance_completed() > 0,
        "soak completed no maintenance: {outcome:?}"
    );
    // The point of seed scaling: the soak must explore beyond the CI seed budget.
    assert!(
        outcome.seeds_executed() > 8,
        "soak did not deepen beyond the default seed budget (got {} seeds)",
        outcome.seeds_executed()
    );
}
