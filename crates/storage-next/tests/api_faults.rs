//! API boundary fault classification harness.

#![cfg(all(feature = "fault-injection", feature = "testkit"))]
#![deny(unsafe_code)]

use strata_storage_next::testkit::check_storage_api_commit_fault_contract;

fn assert_commit_fault_contract(script: &[u8]) {
    let outcome = check_storage_api_commit_fault_contract(script).expect("commit fault contract");
    assert!(outcome.validation_failures() > 0);
    assert!(outcome.conflicts() > 0);
    assert!(outcome.unsupported_durability() > 0);
    assert!(outcome.closed_runtime_rejections() > 0);
    assert!(outcome.ambiguous_commit_examples() > 0);
}

#[test]
fn api_fault_validation_failure_is_classified() {
    assert_commit_fault_contract(b"api-fault-validation");
}

#[test]
fn api_fault_conflict_is_classified() {
    assert_commit_fault_contract(b"api-fault-conflict");
}

#[test]
fn api_fault_unsupported_durability_is_classified() {
    assert_commit_fault_contract(b"api-fault-durability");
}

#[test]
fn api_fault_closed_runtime_is_classified() {
    assert_commit_fault_contract(b"api-fault-closed");
}

#[test]
fn api_fault_ambiguous_commit_class_is_distinct() {
    assert_commit_fault_contract(b"api-fault-ambiguous");
}
