//! Durable immutable-table object publication service.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "table object publication is consumed by L5 table runtime added later"
    )
)]

use crate::backend::{
    Backend, BackendCapability, BackendError, PublishError, PublishFailureKind, PublishOutcome,
};
use crate::format::{decode_immutable_table, FormatError, ImmutableTable};
use crate::layout::{LayoutError, ObjectLayout};
use crate::object::ObjectName;
use crate::service::{validate_publish_outcome, ObjectPublisher};
use std::fmt;
use strata_core_next::CommitVersion;

pub(crate) type TableObjectServiceResult<T> = Result<T, TableObjectServiceError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TableObjectServiceError {
    Layout {
        source: LayoutError,
    },
    Decode {
        object: ObjectName,
        source: FormatError,
    },
    Publish {
        object: ObjectName,
        source: PublishError,
    },
    InvalidPublishMetadata {
        object: ObjectName,
        field: &'static str,
    },
}

impl fmt::Display for TableObjectServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Layout { source } => {
                write!(formatter, "failed to build table object name: {source}")
            }
            Self::Decode { object, source } => {
                write!(
                    formatter,
                    "failed to decode immutable table object {object}: {source}"
                )
            }
            Self::Publish { object, source } => {
                write!(
                    formatter,
                    "failed to publish immutable table object {object}: {source}"
                )
            }
            Self::InvalidPublishMetadata { object, field } => write!(
                formatter,
                "immutable table object {object} has invalid publish metadata {field}"
            ),
        }
    }
}

impl std::error::Error for TableObjectServiceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Layout { source } => Some(source),
            Self::Decode { source, .. } => Some(source),
            Self::Publish { source, .. } => Some(source),
            Self::InvalidPublishMetadata { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableObjectFacts {
    object: ObjectName,
    byte_count: u64,
    row_count: u64,
    data_block_count: u32,
    commit_min: CommitVersion,
    commit_max: CommitVersion,
}

impl TableObjectFacts {
    fn from_table(
        object: ObjectName,
        bytes: &[u8],
        table: &ImmutableTable,
    ) -> TableObjectServiceResult<Self> {
        let byte_count = u64::try_from(bytes.len()).map_err(|_| {
            TableObjectServiceError::InvalidPublishMetadata {
                object: object.clone(),
                field: "byte_count",
            }
        })?;
        let header = table.header();
        Ok(Self {
            object,
            byte_count,
            row_count: header.row_count(),
            data_block_count: header.data_block_count(),
            commit_min: header.commit_min(),
            commit_max: header.commit_max(),
        })
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }

    pub(crate) const fn byte_count(&self) -> u64 {
        self.byte_count
    }

    pub(crate) const fn row_count(&self) -> u64 {
        self.row_count
    }

    pub(crate) const fn data_block_count(&self) -> u32 {
        self.data_block_count
    }

    pub(crate) const fn commit_min(&self) -> CommitVersion {
        self.commit_min
    }

    pub(crate) const fn commit_max(&self) -> CommitVersion {
        self.commit_max
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableObjectWrite {
    facts: TableObjectFacts,
    outcome: PublishOutcome,
}

impl TableObjectWrite {
    fn new(facts: TableObjectFacts, outcome: PublishOutcome) -> Self {
        Self { facts, outcome }
    }

    pub(crate) const fn facts(&self) -> &TableObjectFacts {
        &self.facts
    }

    pub(crate) const fn outcome(&self) -> &PublishOutcome {
        &self.outcome
    }
}

pub(crate) struct TableObjectService<'a> {
    backend: &'a dyn Backend,
}

impl<'a> TableObjectService<'a> {
    pub(crate) const fn new(backend: &'a dyn Backend) -> Self {
        Self { backend }
    }

    pub(crate) fn publish_create(
        &self,
        branch_id: &str,
        level: u32,
        table_id: &str,
        bytes: &[u8],
    ) -> TableObjectServiceResult<TableObjectWrite> {
        let object = table_object(branch_id, level, table_id)?;
        // Keep capability preflight ahead of table decode. ObjectPublisher also
        // checks before backend mutation; this earlier check preserves the
        // service contract that unsupported durable publication does not spend
        // work decoding caller-supplied table bytes.
        require_durable_publish_capabilities(self.backend, &object)?;
        let table =
            decode_immutable_table(bytes).map_err(|source| TableObjectServiceError::Decode {
                object: object.clone(),
                source,
            })?;
        let facts = TableObjectFacts::from_table(object.clone(), bytes, &table)?;
        let outcome = ObjectPublisher::new(self.backend)
            .publish_durable_create(&object, bytes)
            .map_err(|source| TableObjectServiceError::Publish {
                object: object.clone(),
                source,
            })?;
        validate_publish_outcome(&object, facts.byte_count(), &outcome).map_err(|mismatch| {
            TableObjectServiceError::InvalidPublishMetadata {
                object: mismatch.object().clone(),
                field: mismatch.field(),
            }
        })?;
        Ok(TableObjectWrite::new(facts, outcome))
    }
}

fn table_object(
    branch_id: &str,
    level: u32,
    table_id: &str,
) -> TableObjectServiceResult<ObjectName> {
    ObjectLayout::table_object(branch_id, level, table_id)
        .map_err(|source| TableObjectServiceError::Layout { source })
}

fn require_durable_publish_capabilities(
    backend: &dyn Backend,
    object: &ObjectName,
) -> TableObjectServiceResult<()> {
    let capabilities = backend.capabilities();
    for capability in [
        BackendCapability::DurablePublish,
        BackendCapability::DurableSync,
    ] {
        if !capabilities.contains(capability) {
            return Err(TableObjectServiceError::Publish {
                object: object.clone(),
                source: PublishError::new(
                    object.clone(),
                    PublishFailureKind::Unsupported,
                    BackendError::unsupported(capability),
                ),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{TableObjectService, TableObjectServiceError};
    use crate::backend::{
        Backend, BackendCapabilities, BackendCapability, BackendError, BackendErrorKind,
        BackendMetadata, BackendRange, BackendResult, PublishDurability, PublishError,
        PublishFailureKind, PublishMode, PublishOutcome, PublishResult,
    };
    use crate::format::{encode_immutable_table, FormatError, TableCompression};
    use crate::layout::{LayoutError, ObjectLayout};
    use crate::object::{ObjectName, ObjectPrefix};
    use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use strata_core_next::{BranchId, CommitVersion, Timestamp};

    const RECORDING_DURABLE_CAPABILITIES: &[BackendCapability] = &[
        BackendCapability::ReadObject,
        BackendCapability::ReadRange,
        BackendCapability::WriteObject,
        BackendCapability::DeleteObject,
        BackendCapability::ListPrefix,
        BackendCapability::ObjectMetadata,
        BackendCapability::DurablePublish,
        BackendCapability::DurableSync,
    ];

    #[derive(Debug)]
    struct RecordingBackend {
        capabilities: BackendCapabilities,
        objects: Mutex<BTreeMap<ObjectName, Vec<u8>>>,
        operations: Mutex<Vec<(ObjectName, PublishMode)>>,
        publish_failure: Option<PublishFailureKind>,
        metadata_size_override: Option<u64>,
        outcome_object_override: Option<ObjectName>,
        durability_override: Option<PublishDurability>,
    }

    impl RecordingBackend {
        fn durable() -> Self {
            Self {
                capabilities: BackendCapabilities::from_slice(RECORDING_DURABLE_CAPABILITIES),
                objects: Mutex::new(BTreeMap::new()),
                operations: Mutex::new(Vec::new()),
                publish_failure: None,
                metadata_size_override: None,
                outcome_object_override: None,
                durability_override: None,
            }
        }

        fn with_publish_failure(mut self, failure: PublishFailureKind) -> Self {
            self.publish_failure = Some(failure);
            self
        }

        fn with_metadata_size_override(mut self, size: u64) -> Self {
            self.metadata_size_override = Some(size);
            self
        }

        fn with_outcome_object_override(mut self, object: ObjectName) -> Self {
            self.outcome_object_override = Some(object);
            self
        }

        fn with_durability_override(mut self, durability: PublishDurability) -> Self {
            self.durability_override = Some(durability);
            self
        }

        fn without_capability(mut self, capability: BackendCapability) -> Self {
            let capabilities = RECORDING_DURABLE_CAPABILITIES
                .iter()
                .copied()
                .filter(|candidate| *candidate != capability)
                .collect::<Vec<_>>();
            self.capabilities = BackendCapabilities::from_slice(&capabilities);
            self
        }

        fn seed(&self, object: ObjectName, bytes: &[u8]) {
            self.objects
                .lock()
                .expect("objects lock")
                .insert(object, bytes.to_vec());
        }

        fn read_stored(&self, object: &ObjectName) -> Vec<u8> {
            self.objects
                .lock()
                .expect("objects lock")
                .get(object)
                .expect("stored object")
                .clone()
        }

        fn operations(&self) -> Vec<(ObjectName, PublishMode)> {
            self.operations.lock().expect("operations lock").clone()
        }
    }

    impl Backend for RecordingBackend {
        fn capabilities(&self) -> BackendCapabilities {
            self.capabilities
        }

        fn read_object(&self, name: &ObjectName) -> BackendResult<Vec<u8>> {
            self.objects
                .lock()
                .expect("objects lock")
                .get(name)
                .cloned()
                .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "not found"))
        }

        fn read_range(&self, name: &ObjectName, range: BackendRange) -> BackendResult<Vec<u8>> {
            let bytes = self.read_object(name)?;
            let end = range.end_offset().ok_or_else(|| {
                BackendError::new(BackendErrorKind::InvalidRange, "range overflow")
            })?;
            let start = usize::try_from(range.offset()).unwrap_or(usize::MAX);
            let end = usize::try_from(end).unwrap_or(usize::MAX);
            if start >= bytes.len() {
                return Ok(Vec::new());
            }
            Ok(bytes[start..end.min(bytes.len())].to_vec())
        }

        fn write_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendMetadata> {
            self.objects
                .lock()
                .expect("objects lock")
                .insert(name.clone(), bytes.to_vec());
            Ok(BackendMetadata::new(bytes.len() as u64, None))
        }

        fn delete_object(&self, name: &ObjectName) -> BackendResult<()> {
            self.objects
                .lock()
                .expect("objects lock")
                .remove(name)
                .map_or_else(
                    || Err(BackendError::new(BackendErrorKind::NotFound, "not found")),
                    |_| Ok(()),
                )
        }

        fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
            let mut objects = self
                .objects
                .lock()
                .expect("objects lock")
                .keys()
                .filter(|object| object.as_str().starts_with(prefix.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            objects.sort();
            Ok(objects)
        }

        fn object_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
            self.objects
                .lock()
                .expect("objects lock")
                .get(name)
                .map_or_else(
                    || Err(BackendError::new(BackendErrorKind::NotFound, "not found")),
                    |bytes| Ok(BackendMetadata::new(bytes.len() as u64, None)),
                )
        }

        fn publish_object(
            &self,
            name: &ObjectName,
            bytes: &[u8],
            mode: PublishMode,
        ) -> PublishResult<PublishOutcome> {
            self.operations
                .lock()
                .expect("operations lock")
                .push((name.clone(), mode));
            if let Some(kind) = self.publish_failure {
                return Err(PublishError::new(
                    name.clone(),
                    kind,
                    BackendError::new(BackendErrorKind::Interrupted, "injected publish failure"),
                ));
            }

            let mut objects = self.objects.lock().expect("objects lock");
            if mode == PublishMode::Create && objects.contains_key(name) {
                return Err(PublishError::precondition_failed(name, "object exists"));
            }
            objects.insert(name.clone(), bytes.to_vec());
            let metadata_size = self.metadata_size_override.unwrap_or(bytes.len() as u64);
            let outcome_object = self
                .outcome_object_override
                .clone()
                .unwrap_or_else(|| name.clone());
            Ok(PublishOutcome::new(
                outcome_object,
                BackendMetadata::new(metadata_size, None),
                self.durability_override
                    .unwrap_or(PublishDurability::Durable),
            ))
        }
    }

    #[test]
    fn table_object_publish_create_writes_valid_table_to_layout_object() {
        let backend = RecordingBackend::durable();
        let bytes = valid_table_bytes();
        let branch = branch_id().to_string();
        let object = ObjectLayout::table_object(&branch, 2, "table0001").expect("table object");

        let write = TableObjectService::new(&backend)
            .publish_create(&branch, 2, "table0001", &bytes)
            .expect("publish table object");

        assert_eq!(write.facts().object(), &object);
        assert_eq!(write.facts().byte_count(), bytes.len() as u64);
        assert_eq!(write.facts().row_count(), 2);
        assert_eq!(write.facts().data_block_count(), 1);
        assert_eq!(write.facts().commit_min(), CommitVersion::new(7));
        assert_eq!(write.facts().commit_max(), CommitVersion::new(9));
        assert_eq!(write.outcome().object(), &object);
        assert_eq!(write.outcome().durability(), PublishDurability::Durable);
        assert_eq!(backend.read_stored(&object), bytes);
        assert_eq!(backend.operations(), vec![(object, PublishMode::Create)]);
    }

    #[test]
    fn table_object_publish_rejects_bad_layout_before_decode_or_publish() {
        let backend = RecordingBackend::durable();
        let bytes = valid_table_bytes();

        assert!(matches!(
            TableObjectService::new(&backend).publish_create("bad/branch", 0, "table0001", &bytes),
            Err(TableObjectServiceError::Layout {
                source: LayoutError::ComponentContainsSeparator { role: "branch" }
            })
        ));
        assert!(backend.operations().is_empty());
    }

    #[test]
    fn table_object_publish_rejects_invalid_table_bytes_before_publish() {
        let backend = RecordingBackend::durable();
        let branch = branch_id().to_string();
        let object = ObjectLayout::table_object(&branch, 0, "table0001").expect("table object");

        assert_eq!(
            TableObjectService::new(&backend).publish_create(&branch, 0, "table0001", b"not table"),
            Err(TableObjectServiceError::Decode {
                object,
                source: FormatError::InsufficientBytes {
                    format: "immutable_table",
                    needed: 128,
                    actual: 9
                }
            })
        );
        assert!(backend.operations().is_empty());
    }

    #[test]
    fn table_object_publish_requires_durable_capabilities_before_decode() {
        for capability in [
            BackendCapability::DurablePublish,
            BackendCapability::DurableSync,
        ] {
            let backend = RecordingBackend::durable().without_capability(capability);
            let branch = branch_id().to_string();
            let object = ObjectLayout::table_object(&branch, 0, "table0001").expect("table object");

            let error = TableObjectService::new(&backend)
                .publish_create(&branch, 0, "table0001", b"not table")
                .expect_err("missing durable capability should fail before table decode");

            match error {
                TableObjectServiceError::Publish {
                    object: actual,
                    source,
                } => {
                    assert_eq!(actual, object);
                    assert_eq!(source.kind(), PublishFailureKind::Unsupported);
                    assert!(
                        source
                            .source_error()
                            .to_string()
                            .contains(capability.name()),
                        "publish error did not name missing capability {capability:?}: {source}"
                    );
                }
                other => panic!("expected capability publish error, got {other:?}"),
            }
            assert!(backend.operations().is_empty());
        }
    }

    #[test]
    fn table_object_publish_create_refuses_existing_object_and_preserves_bytes() {
        let backend = RecordingBackend::durable();
        let branch = branch_id().to_string();
        let object = ObjectLayout::table_object(&branch, 0, "table0001").expect("table object");
        backend.seed(object.clone(), b"old table bytes");

        let error = TableObjectService::new(&backend)
            .publish_create(&branch, 0, "table0001", &valid_table_bytes())
            .expect_err("create must not replace immutable table object");

        match error {
            TableObjectServiceError::Publish {
                object: actual,
                source,
            } => {
                assert_eq!(actual, object);
                assert_eq!(source.kind(), PublishFailureKind::PreconditionFailed);
            }
            other => panic!("expected publish error, got {other:?}"),
        }
        assert_eq!(backend.read_stored(&object), b"old table bytes");
    }

    #[test]
    fn table_object_publish_preserves_publish_failure_kind() {
        for kind in [
            PublishFailureKind::Unsupported,
            PublishFailureKind::PreconditionFailed,
            PublishFailureKind::FailedBeforeVisibility,
            PublishFailureKind::VisibilityUnknown,
            PublishFailureKind::VisibleDurabilityUnconfirmed,
        ] {
            let backend = RecordingBackend::durable().with_publish_failure(kind);
            let branch = branch_id().to_string();
            let object = ObjectLayout::table_object(&branch, 0, "table0001").expect("table object");

            let error = TableObjectService::new(&backend)
                .publish_create(&branch, 0, "table0001", &valid_table_bytes())
                .expect_err("publish failure should propagate");

            match error {
                TableObjectServiceError::Publish {
                    object: actual,
                    source,
                } => {
                    assert_eq!(actual, object);
                    assert_eq!(source.kind(), kind);
                }
                other => panic!("expected publish error, got {other:?}"),
            }
        }
    }

    #[test]
    fn table_object_publish_rejects_wrong_publish_metadata() {
        let branch = branch_id().to_string();
        let bytes = valid_table_bytes();
        let object = ObjectLayout::table_object(&branch, 0, "table0001").expect("table object");
        let wrong_object = ObjectLayout::table_object(&branch, 0, "table0002").expect("table two");
        let backend =
            RecordingBackend::durable().with_outcome_object_override(wrong_object.clone());

        assert_eq!(
            TableObjectService::new(&backend).publish_create(&branch, 0, "table0001", &bytes),
            Err(TableObjectServiceError::InvalidPublishMetadata {
                object: object.clone(),
                field: "object"
            })
        );

        let backend = RecordingBackend::durable().with_metadata_size_override(1);
        assert_eq!(
            TableObjectService::new(&backend).publish_create(&branch, 0, "table0001", &bytes),
            Err(TableObjectServiceError::InvalidPublishMetadata {
                object,
                field: "size_bytes"
            })
        );

        let object = ObjectLayout::table_object(&branch, 0, "table0001").expect("table object");
        let backend =
            RecordingBackend::durable().with_durability_override(PublishDurability::NonDurable);
        assert_eq!(
            TableObjectService::new(&backend).publish_create(&branch, 0, "table0001", &bytes),
            Err(TableObjectServiceError::InvalidPublishMetadata {
                object,
                field: "durability"
            })
        );
    }

    #[cfg(all(feature = "localfs", unix))]
    #[test]
    fn table_object_publish_create_round_trips_on_localfs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = crate::backend::local_fs::LocalFsBackend::new(dir.path());
        let bytes = valid_table_bytes();
        let branch = branch_id().to_string();
        let object = ObjectLayout::table_object(&branch, 1, "table0001").expect("table object");

        let write = TableObjectService::new(&backend)
            .publish_create(&branch, 1, "table0001", &bytes)
            .expect("publish table object on localfs");

        assert_eq!(write.facts().object(), &object);
        assert_eq!(write.facts().byte_count(), bytes.len() as u64);
        assert_eq!(write.outcome().object(), &object);
        assert_eq!(write.outcome().durability(), PublishDurability::Durable);
        assert_eq!(
            backend.read_object(&object).expect("read table object"),
            bytes
        );
    }

    fn valid_table_bytes() -> Vec<u8> {
        let rows = vec![row(b"alpha".to_vec(), 9), row(b"beta".to_vec(), 7)];
        encode_immutable_table(&rows, 4096, 8, TableCompression::Uncompressed)
            .expect("encode immutable table")
    }

    fn row(user_key: Vec<u8>, version: u64) -> StorageRow {
        let key = PhysicalKey::new(
            branch_id(),
            "default",
            StorageSpaceId::engine(0x20).expect("engine storage space"),
            user_key,
        )
        .expect("physical key");
        let version = CommitVersion::new(version);
        StorageRow::put(
            key,
            version,
            Timestamp::from_micros(1_700_000_000_000_000 + version.as_u64()),
            Timestamp::EPOCH,
            b"table row".to_vec(),
        )
    }

    fn branch_id() -> BranchId {
        BranchId::from_bytes([0x77; BranchId::BYTE_LEN])
    }
}
