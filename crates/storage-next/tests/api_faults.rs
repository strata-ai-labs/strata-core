//! API boundary fault classification harness.

#![cfg(all(feature = "fault-injection", feature = "testkit"))]
#![deny(unsafe_code)]

use strata_storage_next::testkit::check_storage_api_commit_fault_contract;

#[test]
fn api_fault_validation_failure_is_classified() {
    let outcome =
        check_storage_api_commit_fault_contract(b"api-fault-validation").expect("fault contract");

    assert_eq!(outcome.validation_failures(), 1);
    assert_eq!(outcome.total_routes(), 1);
}

#[test]
fn api_fault_conflict_is_classified() {
    let outcome =
        check_storage_api_commit_fault_contract(b"api-fault-conflict").expect("fault contract");

    assert_eq!(outcome.conflicts(), 1);
    assert_eq!(outcome.total_routes(), 1);
}

#[test]
fn api_fault_before_allocation_failure_is_classified() {
    let outcome =
        check_storage_api_commit_fault_contract(b"api-fault-before").expect("fault contract");

    assert_eq!(outcome.before_allocation_failures(), 1);
    assert_eq!(outcome.total_routes(), 1);
}

#[test]
fn api_fault_after_allocation_failure_is_classified() {
    let outcome =
        check_storage_api_commit_fault_contract(b"api-fault-after").expect("fault contract");

    assert_eq!(outcome.after_allocation_failures(), 1);
    assert_eq!(outcome.total_routes(), 1);
}

#[test]
fn api_fault_wal_append_failure_is_classified() {
    let outcome =
        check_storage_api_commit_fault_contract(b"api-fault-wal").expect("fault contract");

    assert_eq!(outcome.wal_append_failures(), 1);
    assert_eq!(outcome.total_routes(), 1);
}

#[test]
fn api_fault_forced_durability_failure_is_classified() {
    let outcome =
        check_storage_api_commit_fault_contract(b"api-fault-forced").expect("fault contract");

    assert_eq!(outcome.forced_durability_uncertainties(), 1);
    assert_eq!(outcome.total_routes(), 1);
}

#[test]
fn api_fault_branch_apply_failure_is_classified() {
    let outcome =
        check_storage_api_commit_fault_contract(b"api-fault-branch").expect("fault contract");

    assert_eq!(outcome.branch_apply_failures(), 1);
    assert_eq!(outcome.total_routes(), 1);
}

#[test]
fn api_fault_visibility_publication_failure_is_classified() {
    let outcome =
        check_storage_api_commit_fault_contract(b"api-fault-visibility").expect("fault contract");

    assert_eq!(outcome.visibility_publication_failures(), 1);
    assert_eq!(outcome.total_routes(), 1);
}
