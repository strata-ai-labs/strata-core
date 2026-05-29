//! Storage API boundary testkit contracts.

mod branch;
mod commit;
mod maintenance;
mod model;

pub use branch::{check_storage_api_branch_model_contract, StorageApiBranchModelOutcome};
pub use commit::{
    check_storage_api_commit_fault_contract, check_storage_api_commit_model_contract,
    StorageApiCommitFaultOutcome, StorageApiCommitModelOutcome,
};
pub use maintenance::{
    check_storage_api_maintenance_fault_contract, check_storage_api_maintenance_model_contract,
    StorageApiMaintenanceFaultOutcome, StorageApiMaintenanceModelOutcome,
};
pub use model::{check_storage_api_read_model_contract, StorageApiReadModelOutcome};
