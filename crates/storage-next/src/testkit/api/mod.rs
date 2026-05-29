//! Storage API boundary testkit contracts.

mod commit;
mod model;

pub use commit::{
    check_storage_api_commit_fault_contract, check_storage_api_commit_model_contract,
    StorageApiCommitFaultOutcome, StorageApiCommitModelOutcome,
};
pub use model::{check_storage_api_read_model_contract, StorageApiReadModelOutcome};
