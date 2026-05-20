//! Generated commit-runtime scaffold property harness.

#![deny(unsafe_code)]

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn commit_runtime_property_harness_runs_scaffold_contract() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::TestCaseError;
    use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
    use strata_storage_next::testkit::check_commit_runtime_scaffold_contract;

    let mut runner = TestRunner::new(Config {
        cases: 32,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/commit_runtime.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 0..=64), |script| {
            let outcome = check_commit_runtime_scaffold_contract(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if outcome.valid_config_cases() == 0
                || outcome.invalid_config_cases() == 0
                || outcome.phase_fact_cases() == 0
                || outcome.visibility_fact_cases() == 0
                || outcome.invalid_visibility_fact_cases() == 0
                || outcome.error_display_cases() == 0
                || outcome.error_source_cases() == 0
                || outcome.stats_cases() == 0
                || outcome.source_guard_fixture_cases() == 0
                || outcome.valid_batch_cases() == 0
                || outcome.invalid_batch_cases() == 0
                || outcome.duplicate_mutation_cases() == 0
                || outcome.branch_mismatch_cases() == 0
                || outcome.storage_owned_space_cases() == 0
                || outcome.invalid_fact_cases() == 0
                || outcome.stamping_cases() == 0
                || outcome.expiry_rejection_cases() == 0
                || outcome.stamping_rejection_cases() == 0
                || outcome.version_allocation_cases() == 0
                || outcome.version_catch_up_cases() == 0
                || outcome.version_overflow_cases() == 0
                || outcome.generated_timestamp_cases() == 0
                || outcome.clamped_timestamp_cases() == 0
                || outcome.explicit_timestamp_cases() == 0
                || outcome.invalid_explicit_timestamp_cases() == 0
                || outcome.timestamp_source_failure_cases() == 0
                || outcome.read_only_no_allocation_cases() == 0
                || outcome.no_transaction_id_check_cases() == 0
            {
                return Err(TestCaseError::fail(
                    "commit runtime scaffold contract did not exercise all categories",
                ));
            }
            Ok(())
        })
        .expect("generated commit runtime scaffold property");
}
