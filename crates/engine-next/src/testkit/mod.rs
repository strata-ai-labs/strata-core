//! Optional test kit exports.

pub use crate::test_support::*;

/// Returns the storage-space byte for a symbolic row class used in tests.
pub fn row_class_storage_id_for_test(class: &str) -> Option<u8> {
    let row_class = match class {
        "kv" => crate::persistence::RowClass::Kv,
        "json" => crate::persistence::RowClass::Json,
        "json-index" => crate::persistence::RowClass::JsonIndex,
        "branch" => crate::persistence::RowClass::BranchControl,
        "registry" => crate::persistence::RowClass::Registry,
        "identity" => crate::persistence::RowClass::DatasetIdentity,
        _ => return None,
    };
    Some(crate::persistence::row_class_storage_id_for_test(row_class))
}
