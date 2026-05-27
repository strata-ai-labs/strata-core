//! Integration harness for lifecycle branch lifecycle contracts.

#![deny(unsafe_code)]

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_branch_lifecycle_cache_smoke() {
    let outcome = strata_storage_next::testkit::check_lifecycle_branch_lifecycle_contract(
        b"branch-cache-smoke\x01\x02\x03\x04",
    )
    .expect("branch lifecycle contract");

    assert!(outcome.catalog_cases() > 0);
    assert!(outcome.clear_delete_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_branch_lifecycle_fork_at_history() {
    let outcome = strata_storage_next::testkit::check_lifecycle_branch_lifecycle_contract(
        b"branch-fork-history\x10\x20\x30\x40",
    )
    .expect("branch lifecycle contract");

    assert!(outcome.fork_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_branch_lifecycle_clear_delete_no_resurrection() {
    let outcome = strata_storage_next::testkit::check_lifecycle_branch_lifecycle_contract(
        b"branch-clear-delete\x55\x55\x66\x77",
    )
    .expect("branch lifecycle contract");

    assert!(outcome.stale_work_cases() > 0);
    assert!(outcome.generation_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_branch_lifecycle_pinned_view_retention() {
    let outcome = strata_storage_next::testkit::check_lifecycle_branch_lifecycle_contract(
        b"branch-pinned-view\x99\x88\x77\x66",
    )
    .expect("branch lifecycle contract");

    assert!(outcome.pinned_view_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_branch_lifecycle_generation_reuse() {
    let outcome = strata_storage_next::testkit::check_lifecycle_branch_lifecycle_contract(
        b"branch-generation-reuse\xaa\xbb\xcc\xdd",
    )
    .expect("branch lifecycle contract");

    assert!(outcome.generation_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_branch_lifecycle_stale_maintenance_rejection() {
    let outcome = strata_storage_next::testkit::check_lifecycle_branch_lifecycle_contract(
        b"branch-stale-maintenance\x11\x22\x33\x44",
    )
    .expect("branch lifecycle contract");

    assert!(outcome.stale_work_cases() > 0);
}
