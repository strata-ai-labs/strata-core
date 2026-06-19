//! Optional test kit exports.

pub use crate::test_support::*;

/// Returns the storage-space byte for a symbolic row class used in tests.
pub fn row_class_storage_id_for_test(class: &str) -> Option<u8> {
    let row_class = match class {
        "kv" => crate::persistence::RowClass::Kv,
        "json" => crate::persistence::RowClass::Json,
        "json-index" => crate::persistence::RowClass::JsonIndex,
        "graph-metadata" => crate::persistence::RowClass::GraphMetadata,
        "graph-node" => crate::persistence::RowClass::GraphNode,
        "graph-edge" => crate::persistence::RowClass::GraphEdge,
        "graph-reverse-edge" => crate::persistence::RowClass::GraphReverseEdge,
        "graph-binding" => crate::persistence::RowClass::GraphBindingIndex,
        "branch" => crate::persistence::RowClass::BranchControl,
        "space-control" => crate::persistence::RowClass::SpaceControl,
        "registry" => crate::persistence::RowClass::Registry,
        "identity" => crate::persistence::RowClass::DatasetIdentity,
        _ => return None,
    };
    Some(crate::persistence::row_class_storage_id_for_test(row_class))
}
