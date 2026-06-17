//! Recovery oracle (STH-1): after a crash at each bounded-exhaustive point in a
//! seeded durable workload, recovered state must be a prefix of acknowledged
//! commit history — no acknowledged commit lost, no torn batch, gap, or phantom.

#![deny(unsafe_code)]

mod common;

#[cfg(all(feature = "testkit", feature = "localfs", not(target_arch = "wasm32")))]
#[test]
fn recovery_oracle_recovers_prefix_of_acknowledged_history() {
    let keep_test_dir = common::keep_test_dir()
        .unwrap_or_else(|error| panic!("invalid recovery oracle environment: {error}"));
    let test_root_override = common::test_root_override()
        .unwrap_or_else(|error| panic!("invalid recovery oracle environment: {error}"));
    let case_limit = common::crash_case_limit()
        .unwrap_or_else(|error| panic!("invalid recovery oracle environment: {error}"));

    let tempdir = test_root_override
        .is_none()
        .then(|| tempfile::tempdir().expect("temp recovery oracle root"));
    let root = test_root_override.unwrap_or_else(|| {
        tempdir
            .as_ref()
            .expect("tempdir exists without override")
            .path()
            .to_path_buf()
    });

    // Cap the grid for the CI-fast lane; STRATA_STORAGE_CRASH_CASES overrides
    // (set it higher, or to the full grid, for a deeper local/soak run).
    let outcome =
        strata_storage_next::testkit::run_recovery_oracle_harness(&root, case_limit.or(Some(96)))
            .expect("recovery oracle harness");

    assert_eq!(
        outcome.durability_modes_swept(),
        2,
        "both durability modes must be swept"
    );
    // A budget of 0 is an explicit "run nothing"; otherwise the sweep must execute.
    assert!(
        outcome.drop_cases() > 0 || case_limit == Some(0),
        "no drop cases executed"
    );

    if keep_test_dir {
        if let Some(tempdir) = tempdir {
            eprintln!("keeping recovery oracle root at {}", root.display());
            let _ = tempdir.keep();
        }
    }
}
