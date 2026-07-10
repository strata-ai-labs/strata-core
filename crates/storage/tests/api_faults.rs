//! API boundary fault classification harness.

#![cfg(all(feature = "fault-injection", feature = "testkit"))]
#![deny(unsafe_code)]

use strata_storage::testkit::{
    check_storage_api_commit_fault_contract, check_storage_api_maintenance_fault_contract,
};

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

#[test]
fn api_fault_snapshot_publish_failure_is_classified() {
    let outcome = check_storage_api_maintenance_fault_contract(b"api-fault-snapshot")
        .expect("maintenance fault contract");

    assert_eq!(outcome.snapshot_publish_failures(), 1);
    assert_eq!(outcome.total_routes(), 1);
}

#[test]
fn api_fault_manifest_publish_failure_is_classified() {
    let outcome = check_storage_api_maintenance_fault_contract(b"api-fault-manifest")
        .expect("maintenance fault contract");

    assert_eq!(outcome.manifest_publish_failures(), 1);
    assert_eq!(outcome.total_routes(), 1);
}

#[test]
fn api_fault_table_publish_failure_is_classified() {
    let outcome = check_storage_api_maintenance_fault_contract(b"api-fault-table")
        .expect("maintenance fault contract");

    assert_eq!(outcome.table_publish_failures(), 1);
    assert_eq!(outcome.total_routes(), 1);
}

#[test]
fn api_fault_compaction_failure_is_classified() {
    let outcome = check_storage_api_maintenance_fault_contract(b"api-fault-compaction")
        .expect("maintenance fault contract");

    assert_eq!(outcome.compaction_failures(), 1);
    assert_eq!(outcome.total_routes(), 1);
}

#[test]
fn api_fault_materialization_failure_is_classified() {
    let outcome = check_storage_api_maintenance_fault_contract(b"api-fault-materialization")
        .expect("maintenance fault contract");

    assert_eq!(outcome.materialization_failures(), 1);
    assert_eq!(outcome.total_routes(), 1);
}

#[test]
fn api_fault_retention_proof_failure_is_classified() {
    let outcome = check_storage_api_maintenance_fault_contract(b"api-fault-retention")
        .expect("maintenance fault contract");

    assert_eq!(outcome.retention_proof_failures(), 1);
    assert_eq!(outcome.total_routes(), 1);
}

#[test]
fn api_fault_quarantine_inventory_mismatch_is_classified() {
    let outcome = check_storage_api_maintenance_fault_contract(b"api-fault-inventory")
        .expect("maintenance fault contract");

    assert_eq!(outcome.quarantine_inventory_mismatches(), 1);
    assert_eq!(outcome.total_routes(), 1);
}

#[test]
fn api_fault_purge_publish_uncertainty_is_classified() {
    let outcome = check_storage_api_maintenance_fault_contract(b"api-fault-purge")
        .expect("maintenance fault contract");

    assert_eq!(outcome.purge_publish_uncertainties(), 1);
    assert_eq!(outcome.total_routes(), 1);
}

#[test]
fn api_fault_repair_failure_is_classified() {
    let outcome = check_storage_api_maintenance_fault_contract(b"api-fault-repair")
        .expect("maintenance fault contract");

    assert_eq!(outcome.repair_failures(), 1);
    assert_eq!(outcome.total_routes(), 1);
}
