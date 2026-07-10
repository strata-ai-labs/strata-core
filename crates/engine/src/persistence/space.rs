//! Engine row classes and storage-space assignments.

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum RowClass {
    Kv,
    Json,
    JsonIndex,
    VectorCollection,
    Vector,
    Event,
    EventMetadata,
    EventIndex,
    GraphMetadata,
    GraphNode,
    GraphEdge,
    GraphReverseEdge,
    GraphBindingIndex,
    GraphOntology,
    GraphTypeIndex,
    BranchControl,
    SpaceControl,
    Registry,
    DatasetIdentity,
}

impl RowClass {
    pub(crate) const fn storage_space_id(self) -> u8 {
        match self {
            Self::Kv => 0x20,
            Self::Json => 0x22,
            Self::JsonIndex => 0x24,
            Self::VectorCollection => 0x26,
            Self::Vector => 0x28,
            Self::Event => 0x2a,
            Self::EventMetadata => 0x2c,
            Self::EventIndex => 0x2e,
            Self::BranchControl => 0x30,
            Self::SpaceControl => 0x31,
            Self::Registry => 0x32,
            Self::DatasetIdentity => 0x34,
            Self::GraphMetadata => 0x36,
            Self::GraphNode => 0x38,
            Self::GraphEdge => 0x3a,
            Self::GraphReverseEdge => 0x3c,
            Self::GraphBindingIndex => 0x3e,
            // Authored ontology rows: the first source-row byte past the
            // 0x40..=0x45 derived-row families reserved by the storage-space
            // registry.
            Self::GraphOntology => 0x46,
            // Rebuildable node-type index rows: the storage-space
            // registry's graph-index family byte ("graph reverse maps,
            // relationship lookup rows, traversal accelerators").
            Self::GraphTypeIndex => 0x43,
        }
    }
}

#[cfg(any(test, feature = "testkit"))]
pub(crate) const fn row_class_storage_id_for_test(class: RowClass) -> u8 {
    class.storage_space_id()
}

#[cfg(test)]
mod tests {
    use super::RowClass;

    #[test]
    fn core_control_row_ids_are_locked_and_leave_deferred_gaps_unused() {
        assert_eq!(RowClass::BranchControl.storage_space_id(), 0x30);
        assert_eq!(RowClass::SpaceControl.storage_space_id(), 0x31);
        assert_eq!(RowClass::Registry.storage_space_id(), 0x32);
        assert_eq!(RowClass::DatasetIdentity.storage_space_id(), 0x34);

        let all_ids = [
            RowClass::Kv.storage_space_id(),
            RowClass::Json.storage_space_id(),
            RowClass::JsonIndex.storage_space_id(),
            RowClass::VectorCollection.storage_space_id(),
            RowClass::Vector.storage_space_id(),
            RowClass::Event.storage_space_id(),
            RowClass::EventMetadata.storage_space_id(),
            RowClass::EventIndex.storage_space_id(),
            RowClass::GraphMetadata.storage_space_id(),
            RowClass::GraphNode.storage_space_id(),
            RowClass::GraphEdge.storage_space_id(),
            RowClass::GraphReverseEdge.storage_space_id(),
            RowClass::GraphBindingIndex.storage_space_id(),
            RowClass::GraphOntology.storage_space_id(),
            RowClass::GraphTypeIndex.storage_space_id(),
            RowClass::BranchControl.storage_space_id(),
            RowClass::SpaceControl.storage_space_id(),
            RowClass::Registry.storage_space_id(),
            RowClass::DatasetIdentity.storage_space_id(),
        ];
        let mut unique = all_ids.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), all_ids.len());
        assert!(!unique.contains(&0x33));
        // 0x43 is claimed by GraphTypeIndex — the registry's graph-index
        // derived family; the other derived-family bytes stay reserved.
        assert_eq!(RowClass::GraphTypeIndex.storage_space_id(), 0x43);
        assert!(unique
            .iter()
            .all(|id| !(0x40..=0x45).contains(id) || *id == 0x43));
    }
}
