//! Storage persistence adapter boundary.

mod adapter;
mod key;
mod plan;
mod row;
mod space;

pub(crate) use adapter::{
    close_summary_is_durable, PersistenceOpenSummary, PersistenceOpenTarget, PersistenceReadRow,
    StoragePersistence,
};
pub(crate) use key::{
    branch_catalog_key, branch_index_key, branch_pending_index_key, branch_pending_key,
    capability_registry_key, database_identity_key, decode_kv_key, encode_kv_key,
    encode_kv_key_bytes, encode_kv_space_prefix, storage_registry_key,
};
pub(crate) use plan::CommitPlan;
pub(crate) use row::{ReadSelector, RowAddress, RowMutation};
pub(crate) use space::RowClass;

#[cfg(any(test, feature = "testkit"))]
pub(crate) use space::row_class_storage_id_for_test;
