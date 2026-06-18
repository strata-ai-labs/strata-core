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
    BranchControl,
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
            Self::Registry => 0x32,
            Self::DatasetIdentity => 0x34,
            Self::GraphMetadata => 0x36,
            Self::GraphNode => 0x38,
            Self::GraphEdge => 0x3a,
            Self::GraphReverseEdge => 0x3c,
            Self::GraphBindingIndex => 0x3e,
        }
    }
}

#[cfg(any(test, feature = "testkit"))]
pub(crate) const fn row_class_storage_id_for_test(class: RowClass) -> u8 {
    class.storage_space_id()
}
