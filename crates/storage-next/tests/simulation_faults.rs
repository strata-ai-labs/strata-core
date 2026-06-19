//! Fault-combination simulation (STH-4 slice 4c): the interleaving × fault/crash
//! dimension. Each seed runs a backend-op fault and a power-loss crash at a
//! seed-chosen point of a seeded interleaving (client commits interspersed with
//! maintenance drained at a sim-chosen cadence), verified against the STH-1 recovery
//! oracle: a backend-op fault loses no acknowledged commit (the faulted commit is
//! in-doubt); a power-loss crash holds the durability contract (`Always` loses
//! nothing, `Standard` a clean prefix; a torn tail is fail-loud or a clean prefix).
//! This crosses the STH-2 / STH-3 fault and crash substrates with *varied* schedules.

#![deny(unsafe_code)]

mod common;

#[cfg(all(
    feature = "fault-injection",
    feature = "localfs",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_simulation_holds_oracle_across_seeds() {
    let keep_test_dir = common::keep_test_dir()
        .unwrap_or_else(|error| panic!("invalid fault-simulation environment: {error}"));
    let test_root_override = common::test_root_override()
        .unwrap_or_else(|error| panic!("invalid fault-simulation environment: {error}"));
    let case_limit = common::fault_case_limit()
        .unwrap_or_else(|error| panic!("invalid fault-simulation environment: {error}"));

    let tempdir = test_root_override
        .is_none()
        .then(|| tempfile::tempdir().expect("temp fault-simulation root"));
    let root = test_root_override.unwrap_or_else(|| {
        tempdir
            .as_ref()
            .expect("tempdir exists without override")
            .path()
            .to_path_buf()
    });

    // Cap the seed set for the CI-fast lane; STRATA_STORAGE_FAULT_CASES overrides.
    let outcome =
        strata_storage_next::testkit::run_fault_simulation_harness(&root, case_limit.or(Some(16)))
            .expect("fault-simulation harness");

    // A budget of 0 is an explicit "run nothing"; otherwise the sweep must run real
    // fault + crash cases, with at least one fault firing and one crash perturbing
    // the disk (never a vacuous green).
    assert!(
        (outcome.fault_cases() > 0 && outcome.crash_cases() > 0) || case_limit == Some(0),
        "no fault-simulation cases run"
    );
    assert!(
        outcome.fault_fired_cases() > 0 || case_limit == Some(0),
        "no scheduled backend fault fired"
    );
    assert!(
        outcome.crash_perturbed_cases() > 0 || case_limit == Some(0),
        "no power-loss crash perturbed the on-disk state"
    );

    if keep_test_dir {
        if let Some(tempdir) = tempdir {
            eprintln!("keeping fault-simulation root at {}", root.display());
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
#[ignore = "soak: deep multi-seed fault simulation; run with --ignored (raise STRATA_STORAGE_FAULT_CASES)"]
#[test]
fn fault_simulation_soak_deepens_across_many_seeds() {
    let case_limit = common::fault_case_limit()
        .unwrap_or_else(|error| panic!("invalid fault-simulation environment: {error}"));
    let tempdir = tempfile::tempdir().expect("temp fault-simulation soak root");

    let outcome = strata_storage_next::testkit::run_fault_simulation_harness(
        tempdir.path(),
        case_limit.or(Some(2_000)),
    )
    .expect("fault-simulation soak");

    assert!(
        outcome.fault_fired_cases() > 0 && outcome.crash_perturbed_cases() > 0,
        "soak was vacuous: {outcome:?}"
    );
    // The point of seed scaling: the soak must explore beyond the CI seed budget.
    assert!(
        outcome.seeds_executed() > 4,
        "soak did not deepen beyond the default seed budget (got {} seeds)",
        outcome.seeds_executed()
    );
}
