//! Long-running storage stress harness entry point.

#![deny(unsafe_code)]

mod common;

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn stress_harness_runs_bounded_storage_scripts() {
    let duration = common::stress_duration()
        .unwrap_or_else(|error| panic!("invalid stress harness environment: {error}"));
    let seed = common::stress_seed()
        .unwrap_or_else(|error| panic!("invalid stress harness environment: {error}"))
        .unwrap_or(0x51c0_ffef_00d5_eed5);

    let outcome = strata_storage::testkit::run_storage_stress_harness(seed, duration)
        .expect("storage stress harness");

    assert!(outcome.iterations() > 0);
    assert_eq!(outcome.scripts_executed(), outcome.iterations() * 3);
}
