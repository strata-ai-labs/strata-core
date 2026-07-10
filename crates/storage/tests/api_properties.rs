//! API boundary property harness.

#![deny(unsafe_code)]

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn api_property_harness_checks_empty_runtime_reads_are_deterministic() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::TestCaseError;
    use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
    use strata_storage::api::{
        BranchId, PointReadRequest, ReadBound, StorageKey, StorageRuntime, StorageSpaceId,
    };

    let mut runner = TestRunner::new(Config {
        cases: 32,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/storage_api.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 1..=32), |bytes| {
            let outcome = StorageRuntime::open_ephemeral()
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            let runtime = outcome.summary();
            if !runtime.maintenance_ready() {
                return Err(TestCaseError::fail(
                    "opened cache runtime did not report maintenance readiness",
                ));
            }
            let branch = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);
            let request = PointReadRequest::new(
                branch,
                StorageSpaceId::new(vec![0x20])
                    .map_err(|error| TestCaseError::fail(error.to_string()))?,
                StorageKey::new(bytes).map_err(|error| TestCaseError::fail(error.to_string()))?,
                ReadBound::Latest,
            );
            let read = outcome
                .into_runtime()
                .read_point(&request)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if read.row().is_some() {
                return Err(TestCaseError::fail(
                    "empty runtime returned a row for generated key",
                ));
            }
            Ok(())
        })
        .expect("generated API read property");
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn api_property_harness_rejects_closed_runtime_reads() {
    use strata_storage::api::{
        BranchId, PointReadRequest, ReadBound, StorageApiErrorClass, StorageKey, StorageRuntime,
        StorageSpaceId,
    };

    let mut runtime = StorageRuntime::open_ephemeral()
        .expect("open runtime")
        .into_runtime();
    runtime.close().expect("close runtime");
    let branch = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);
    let error = runtime
        .read_point(&PointReadRequest::new(
            branch,
            StorageSpaceId::new(vec![0x20]).expect("engine space"),
            StorageKey::new(b"closed".to_vec()).expect("key"),
            ReadBound::Latest,
        ))
        .expect_err("closed runtime must reject reads");
    assert_eq!(error.class(), StorageApiErrorClass::FailedPrecondition);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn api_property_harness_matches_generated_read_model() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::{Config, FileFailurePersistence, TestCaseError, TestRunner};
    use strata_storage::testkit::check_storage_api_read_model_contract;

    let mut runner = TestRunner::new(Config {
        cases: 32,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/storage_api_read_model.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 1..=96), |script| {
            let outcome = check_storage_api_read_model_contract(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if outcome.puts() == 0
                || outcome.deletes() == 0
                || outcome.point_reads() == 0
                || outcome.history_reads() == 0
                || outcome.prefix_scans() == 0
                || outcome.range_scans() == 0
                || outcome.timestamp_lookups() == 0
                || outcome.retained_history_misses() == 0
            {
                return Err(TestCaseError::fail(
                    "generated read script did not exercise every required route",
                ));
            }
            Ok(())
        })
        .expect("generated API read model property");
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn api_property_harness_matches_generated_commit_model() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::{Config, FileFailurePersistence, TestCaseError, TestRunner};
    use strata_storage::testkit::check_storage_api_commit_model_contract;

    let mut runner = TestRunner::new(Config {
        cases: 32,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/storage_api_commit_model.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 1..=64), |script| {
            let outcome = check_storage_api_commit_model_contract(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if outcome.commits() == 0
                || outcome.puts() == 0
                || outcome.deletes() == 0
                || outcome.conditions() == 0
                || outcome.conflicts() == 0
                || outcome.ttl_roundtrips() == 0
            {
                return Err(TestCaseError::fail(
                    "generated commit script did not exercise every required route",
                ));
            }
            Ok(())
        })
        .expect("generated API commit model property");
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn api_property_harness_matches_generated_branch_model() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::{Config, FileFailurePersistence, TestCaseError, TestRunner};
    use strata_storage::testkit::check_storage_api_branch_model_contract;

    let mut runner = TestRunner::new(Config {
        cases: 32,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/storage_api_branch_model.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 1..=64), |script| {
            let outcome = check_storage_api_branch_model_contract(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if outcome.creates() == 0
                || outcome.describes() == 0
                || outcome.lists() == 0
                || outcome.fork_current() == 0
                || outcome.fork_at_version() == 0
                || outcome.fork_at_timestamp() == 0
                || outcome.clears() == 0
                || outcome.deletes() == 0
                || outcome.recreate_transitions() == 0
                || outcome.invalid_source_rejections() == 0
                || outcome.read_checks() == 0
            {
                return Err(TestCaseError::fail(
                    "generated branch script did not exercise every required route",
                ));
            }
            Ok(())
        })
        .expect("generated API branch model property");
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn api_property_harness_matches_generated_maintenance_model() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::{Config, FileFailurePersistence, TestCaseError, TestRunner};
    use strata_storage::testkit::check_storage_api_maintenance_model_contract;

    let mut runner = TestRunner::new(Config {
        cases: 32,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/storage_api_maintenance_model.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 1..=64), |script| {
            let outcome = check_storage_api_maintenance_model_contract(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if outcome.checkpoints() == 0
                || outcome.flushes() == 0
                || outcome.rewrites() == 0
                || outcome.retention() == 0
                || outcome.quarantine() == 0
                || outcome.wal_growth() == 0
                || outcome.queue_drains() == 0
                || outcome.scope_rejections() == 0
            {
                return Err(TestCaseError::fail(
                    "generated maintenance script did not exercise every required route",
                ));
            }
            Ok(())
        })
        .expect("generated API maintenance model property");
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn api_property_harness_matches_generated_diagnostics_model() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::{Config, FileFailurePersistence, TestCaseError, TestRunner};
    use strata_storage::testkit::check_storage_api_diagnostics_model_contract;

    let mut runner = TestRunner::new(Config {
        cases: 32,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/storage_api_diagnostics_model.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 1..=64), |script| {
            let outcome = check_storage_api_diagnostics_model_contract(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if outcome.health_reports() == 0
                || outcome.resource_reports() == 0
                || outcome.object_reports() == 0
                || outcome.branch_reports() == 0
                || outcome.timeline_reports() == 0
                || outcome.unsupported_durable_reports() == 0
                || outcome.closed_reports() == 0
            {
                return Err(TestCaseError::fail(
                    "generated diagnostics script did not exercise every required route",
                ));
            }
            Ok(())
        })
        .expect("generated API diagnostics model property");
}
