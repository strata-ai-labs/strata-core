//! Engine row classes and storage-space assignments.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RowClass {
    Kv,
    Json,
    JsonIndex,
    VectorCollection,
    Vector,
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
            Self::BranchControl => 0x30,
            Self::Registry => 0x32,
            Self::DatasetIdentity => 0x34,
        }
    }
}

#[cfg(any(test, feature = "testkit"))]
pub(crate) const fn row_class_storage_id_for_test(class: RowClass) -> u8 {
    class.storage_space_id()
}
