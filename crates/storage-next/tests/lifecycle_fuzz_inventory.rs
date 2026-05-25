//! Lifecycle fuzz target and corpus inventory.

#![deny(unsafe_code)]

mod common;

use std::fs;

#[test]
fn lifecycle_fuzz_targets_are_registered() {
    let manifest = fs::read_to_string(common::crate_root().join("fuzz/Cargo.toml"))
        .expect("read fuzz manifest");

    for target in [
        "lifecycle_recovery",
        "lifecycle_maintenance",
        "lifecycle_retention",
    ] {
        assert!(manifest.contains(&format!("name = \"{target}\"")));
        assert!(manifest.contains(&format!("fuzz_targets/{target}.rs")));
    }
}

#[test]
fn lifecycle_fuzz_targets_call_distinct_contracts() {
    let root = common::crate_root();
    for (target, contract) in [
        (
            "lifecycle_recovery",
            "check_lifecycle_recovery_fuzz_contract",
        ),
        (
            "lifecycle_maintenance",
            "check_lifecycle_maintenance_fuzz_contract",
        ),
        (
            "lifecycle_retention",
            "check_lifecycle_retention_fuzz_contract",
        ),
    ] {
        let text = fs::read_to_string(root.join(format!("fuzz/fuzz_targets/{target}.rs")))
            .expect("read fuzz target");
        assert!(text.contains(contract), "{target} does not call {contract}");
        assert!(!text.contains("check_lifecycle_scaffold_contract"));
    }
}

#[test]
fn lifecycle_fuzz_corpora_have_non_empty_seed_files() {
    let root = common::crate_root();
    for (target, seeds) in [
        (
            "lifecycle_recovery",
            ["valid_seed", "corrupt_seed", "mixed_seed"],
        ),
        (
            "lifecycle_maintenance",
            ["valid_seed", "fault_seed", "close_seed"],
        ),
        (
            "lifecycle_retention",
            ["valid_seed", "blocked_seed", "purge_seed"],
        ),
    ] {
        let dir = root.join(format!("fuzz/corpus/{target}"));
        for seed in seeds {
            let path = dir.join(seed);
            assert!(path.is_file(), "{} is missing", path.display());
            assert!(fs::metadata(&path).expect("seed metadata").len() > 0);
        }
    }
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_recovery_fuzz_seed_hits_valid_and_corrupt_routes() {
    let outcome = strata_storage_next::testkit::check_lifecycle_recovery_fuzz_contract(
        &seed_bytes("lifecycle_recovery", "mixed_seed"),
    )
    .expect("recovery fuzz contract");

    assert!(outcome.recovered_visibility_match_cases() > 0);
    assert!(outcome.lossy_degraded_health_check_cases() > 0);
    assert_eq!(outcome.input_maintenance_route_cases(), 0);
    assert_eq!(outcome.input_reclaim_route_cases(), 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_maintenance_fuzz_seed_hits_task_and_close_routes() {
    let outcome = strata_storage_next::testkit::check_lifecycle_maintenance_fuzz_contract(
        &seed_bytes("lifecycle_maintenance", "close_seed"),
    )
    .expect("maintenance fuzz contract");

    assert!(outcome.input_maintenance_route_cases() > 0);
    assert!(outcome.close_idempotence_check_cases() > 0);
    assert_eq!(outcome.input_reclaim_route_cases(), 0);
    assert_eq!(outcome.recovered_visibility_match_cases(), 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_maintenance_fuzz_regression_keeps_materialization_branches_distinct() {
    let outcome = strata_storage_next::testkit::check_lifecycle_maintenance_fuzz_contract(
        b"lifecycle-maintenance-valid-eeed\n",
    )
    .expect("maintenance fuzz contract");

    assert!(outcome.input_maintenance_route_cases() > 0);
    assert!(outcome.close_idempotence_check_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_retention_fuzz_seed_hits_delete_and_defer_routes() {
    let outcome = strata_storage_next::testkit::check_lifecycle_retention_fuzz_contract(
        &seed_bytes("lifecycle_retention", "purge_seed"),
    )
    .expect("retention fuzz contract");

    assert!(outcome.input_reclaim_route_cases() > 0);
    assert!(outcome.deletion_subset_check_cases() > 0);
    assert_eq!(outcome.input_maintenance_route_cases(), 0);
    assert_eq!(outcome.recovered_visibility_match_cases(), 0);
}

#[cfg(not(all(feature = "testkit", not(target_arch = "wasm32"))))]
#[test]
fn lifecycle_fuzz_seed_contracts_require_testkit() {}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
fn seed_bytes(target: &str, name: &str) -> Vec<u8> {
    fs::read(common::crate_root().join(format!("fuzz/corpus/{target}/{name}")))
        .expect("read lifecycle fuzz seed")
}
