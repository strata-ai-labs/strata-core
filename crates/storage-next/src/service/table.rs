//! Durable immutable-table object publication service.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "table object publication and reads are consumed by later branch table integration"
    )
)]

use crate::backend::{
    Backend, BackendCapability, BackendError, BackendRange, PublishError, PublishFailureKind,
    PublishOutcome,
};
use crate::format::{decode_immutable_table, FormatError, ImmutableTable};
use crate::layout::{LayoutError, ObjectLayout};
use crate::object::ObjectName;
use crate::service::{validate_publish_outcome, ObjectPublisher};
use crate::table::{
    ImmutableTableReader, TableByteSource, TableIdentity, TableReaderConfig, TableRuntimeError,
    TableRuntimeResult,
};
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TableObjectReadError {
    UnsupportedCapability {
        object: ObjectName,
        capability: BackendCapability,
    },
    Backend {
        object: ObjectName,
        source: BackendError,
    },
    Source {
        object: ObjectName,
        reason: &'static str,
    },
    Table {
        object: ObjectName,
        source: TableRuntimeError,
    },
    FactMismatch {
        object: ObjectName,
        field: &'static str,
    },
}

impl TableObjectReadError {
    const fn source_read_reason(&self) -> &'static str {
        match self {
            Self::UnsupportedCapability { .. } => "backend lacks table object read capability",
            Self::Backend { .. } => "backend table object range read failed",
            Self::Source { reason, .. } => reason,
            Self::Table { .. } => "table object bytes failed table validation",
            Self::FactMismatch { .. } => "table object facts do not match decoded table",
        }
    }
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

impl fmt::Display for TableObjectReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedCapability { object, capability } => write!(
                formatter,
                "cannot read immutable table object {object}: backend lacks {capability}"
            ),
            Self::Backend { object, source } => write!(
                formatter,
                "failed to read immutable table object {object}: {source}"
            ),
            Self::Source { object, reason } => write!(
                formatter,
                "immutable table object {object} source is invalid: {reason}"
            ),
            Self::Table { object, source } => write!(
                formatter,
                "failed to open immutable table object {object}: {source}"
            ),
            Self::FactMismatch { object, field } => write!(
                formatter,
                "immutable table object {object} decoded facts mismatch field {field}"
            ),
        }
    }
}

impl std::error::Error for TableObjectReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Backend { source, .. } => Some(source),
            Self::Table { source, .. } => Some(source),
            Self::UnsupportedCapability { .. }
            | Self::Source { .. }
            | Self::FactMismatch { .. } => None,
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

pub(crate) struct TableObjectByteSource<'a> {
    backend: &'a dyn Backend,
    object: ObjectName,
    byte_count: u64,
}

impl<'a> TableObjectByteSource<'a> {
    pub(crate) fn new(
        backend: &'a dyn Backend,
        object: ObjectName,
        byte_count: u64,
    ) -> Result<Self, TableObjectReadError> {
        if !backend
            .capabilities()
            .contains(BackendCapability::ReadRange)
        {
            return Err(TableObjectReadError::UnsupportedCapability {
                object,
                capability: BackendCapability::ReadRange,
            });
        }
        if byte_count == 0 {
            return Err(TableObjectReadError::Source {
                object,
                reason: "table object byte count is zero",
            });
        }
        let source = Self {
            backend,
            object,
            byte_count,
        };
        source.validate_metadata_if_available()?;
        Ok(source)
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }

    fn validate_metadata_if_available(&self) -> Result<(), TableObjectReadError> {
        if !self
            .backend
            .capabilities()
            .contains(BackendCapability::ObjectMetadata)
        {
            return Ok(());
        }

        let metadata = self
            .backend
            .object_metadata(&self.object)
            .map_err(|source| TableObjectReadError::Backend {
                object: self.object.clone(),
                source,
            })?;
        if metadata.size_bytes() != self.byte_count {
            return Err(TableObjectReadError::FactMismatch {
                object: self.object.clone(),
                field: "byte_count",
            });
        }
        Ok(())
    }

    pub(crate) fn read_exact_at(
        &self,
        offset: u64,
        len: usize,
    ) -> Result<Vec<u8>, TableObjectReadError> {
        if len == 0 {
            if offset > self.byte_count {
                return Err(TableObjectReadError::Source {
                    object: self.object.clone(),
                    reason: "range exceeds table object byte count",
                });
            }
            return Ok(Vec::new());
        }

        let len_u64 = u64::try_from(len).map_err(|_| TableObjectReadError::Source {
            object: self.object.clone(),
            reason: "range length is too large",
        })?;
        let end = offset
            .checked_add(len_u64)
            .ok_or_else(|| TableObjectReadError::Source {
                object: self.object.clone(),
                reason: "range end overflows",
            })?;
        if end > self.byte_count {
            return Err(TableObjectReadError::Source {
                object: self.object.clone(),
                reason: "range exceeds table object byte count",
            });
        }

        let bytes = self
            .backend
            .read_range(&self.object, BackendRange::new(offset, len_u64))
            .map_err(|source| TableObjectReadError::Backend {
                object: self.object.clone(),
                source,
            })?;
        if bytes.len() != len {
            let reason = if bytes.len() < len {
                "short table object range read"
            } else {
                "long table object range read"
            };
            return Err(TableObjectReadError::Source {
                object: self.object.clone(),
                reason,
            });
        }
        Ok(bytes)
    }

    fn read_full(&self) -> Result<Vec<u8>, TableObjectReadError> {
        let len = usize::try_from(self.byte_count).map_err(|_| TableObjectReadError::Source {
            object: self.object.clone(),
            reason: "table object byte count is too large",
        })?;
        self.read_exact_at(0, len)
    }
}

impl TableByteSource for TableObjectByteSource<'_> {
    fn byte_count(&self) -> u64 {
        self.byte_count
    }

    fn read_at(&self, offset: u64, len: usize) -> TableRuntimeResult<Vec<u8>> {
        self.read_exact_at(offset, len)
            .map_err(|error| TableRuntimeError::source_read_with(error.source_read_reason(), error))
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

pub(crate) struct TableObjectReaderService<'a> {
    backend: &'a dyn Backend,
}

impl<'a> TableObjectReaderService<'a> {
    pub(crate) const fn new(backend: &'a dyn Backend) -> Self {
        Self { backend }
    }

    pub(crate) fn open_reader(
        &self,
        identity: TableIdentity,
        object_facts: &TableObjectFacts,
        config: TableReaderConfig,
    ) -> Result<ImmutableTableReader, TableObjectReadError> {
        let source = TableObjectByteSource::new(
            self.backend,
            object_facts.object().clone(),
            object_facts.byte_count(),
        )?;
        // Keep the backend read at the L4/L5 boundary so object diagnostics
        // retain the original backend error instead of collapsing into L5's
        // object-neutral source-read vocabulary.
        let bytes = source.read_full()?;
        let reader =
            ImmutableTableReader::open_bytes(identity, bytes, config).map_err(|source| {
                TableObjectReadError::Table {
                    object: object_facts.object().clone(),
                    source,
                }
            })?;
        validate_reader_facts(object_facts, &reader)?;
        Ok(reader)
    }
}

fn validate_reader_facts(
    object_facts: &TableObjectFacts,
    reader: &ImmutableTableReader,
) -> Result<(), TableObjectReadError> {
    let reader_facts = reader.facts();
    if reader_facts.byte_count() != object_facts.byte_count() {
        return Err(TableObjectReadError::FactMismatch {
            object: object_facts.object().clone(),
            field: "byte_count",
        });
    }
    if reader_facts.row_count() != object_facts.row_count() {
        return Err(TableObjectReadError::FactMismatch {
            object: object_facts.object().clone(),
            field: "row_count",
        });
    }
    if reader_facts.data_block_count() != object_facts.data_block_count() {
        return Err(TableObjectReadError::FactMismatch {
            object: object_facts.object().clone(),
            field: "data_block_count",
        });
    }
    if reader_facts.commit_range().min() != object_facts.commit_min() {
        return Err(TableObjectReadError::FactMismatch {
            object: object_facts.object().clone(),
            field: "commit_min",
        });
    }
    if reader_facts.commit_range().max() != object_facts.commit_max() {
        return Err(TableObjectReadError::FactMismatch {
            object: object_facts.object().clone(),
            field: "commit_max",
        });
    }
    Ok(())
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
    use super::{
        TableObjectByteSource, TableObjectFacts, TableObjectReadError, TableObjectReaderService,
        TableObjectService, TableObjectServiceError,
    };
    use crate::backend::memory::MemoryBackend;
    use crate::backend::{
        Backend, BackendCapabilities, BackendCapability, BackendError, BackendErrorKind,
        BackendMetadata, BackendRange, BackendResult, PublishDurability, PublishError,
        PublishFailureKind, PublishMode, PublishOutcome, PublishResult,
    };
    use crate::format::{
        decode_immutable_table, encode_immutable_table, FormatError, TableCompression,
    };
    use crate::layout::{LayoutError, ObjectLayout};
    use crate::object::{ObjectName, ObjectPrefix};
    use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
    use crate::table::{
        sort_table_rows_by_key, ImmutableTableBuilder, ImmutableTableReader, TableBuilderConfig,
        TableByteSource, TableCursor, TableIdentity, TableInternalKeyBytes, TableKeyBounds,
        TablePhysicalKeyBytes, TableReaderConfig, TableRow, TableRuntimeError,
    };
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
        range_reads: Mutex<Vec<(ObjectName, BackendRange)>>,
        metadata_calls: Mutex<usize>,
        list_calls: Mutex<usize>,
        write_calls: Mutex<usize>,
        delete_calls: Mutex<usize>,
        publish_failure: Option<PublishFailureKind>,
        read_range_failure: Option<BackendError>,
        short_read: bool,
        long_read: bool,
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
                range_reads: Mutex::new(Vec::new()),
                metadata_calls: Mutex::new(0),
                list_calls: Mutex::new(0),
                write_calls: Mutex::new(0),
                delete_calls: Mutex::new(0),
                publish_failure: None,
                read_range_failure: None,
                short_read: false,
                long_read: false,
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

        fn with_read_range_failure(mut self, error: BackendError) -> Self {
            self.read_range_failure = Some(error);
            self
        }

        fn with_short_read(mut self) -> Self {
            self.short_read = true;
            self
        }

        fn with_long_read(mut self) -> Self {
            self.long_read = true;
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

        fn range_reads(&self) -> Vec<(ObjectName, BackendRange)> {
            self.range_reads.lock().expect("range reads lock").clone()
        }

        fn list_calls(&self) -> usize {
            *self.list_calls.lock().expect("list calls lock")
        }

        fn metadata_calls(&self) -> usize {
            *self.metadata_calls.lock().expect("metadata calls lock")
        }

        fn write_calls(&self) -> usize {
            *self.write_calls.lock().expect("write calls lock")
        }

        fn delete_calls(&self) -> usize {
            *self.delete_calls.lock().expect("delete calls lock")
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
            self.range_reads
                .lock()
                .expect("range reads lock")
                .push((name.clone(), range));
            if let Some(error) = &self.read_range_failure {
                return Err(error.clone());
            }
            let bytes = self.read_object(name)?;
            let end = range.end_offset().ok_or_else(|| {
                BackendError::new(BackendErrorKind::InvalidRange, "range overflow")
            })?;
            let start = usize::try_from(range.offset()).unwrap_or(usize::MAX);
            let end = usize::try_from(end).unwrap_or(usize::MAX);
            if start >= bytes.len() {
                return Ok(Vec::new());
            }
            let mut bytes = bytes[start..end.min(bytes.len())].to_vec();
            if self.short_read && !bytes.is_empty() {
                bytes.pop();
            }
            if self.long_read {
                bytes.push(0);
            }
            Ok(bytes)
        }

        fn write_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendMetadata> {
            *self.write_calls.lock().expect("write calls lock") += 1;
            self.objects
                .lock()
                .expect("objects lock")
                .insert(name.clone(), bytes.to_vec());
            Ok(BackendMetadata::new(bytes.len() as u64, None))
        }

        fn delete_object(&self, name: &ObjectName) -> BackendResult<()> {
            *self.delete_calls.lock().expect("delete calls lock") += 1;
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
            *self.list_calls.lock().expect("list calls lock") += 1;
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
            *self.metadata_calls.lock().expect("metadata calls lock") += 1;
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

    #[test]
    fn table_object_reader_opens_published_object_through_range_source() {
        let backend = RecordingBackend::durable();
        let bytes = valid_table_bytes();
        let branch = branch_id().to_string();
        let write = TableObjectService::new(&backend)
            .publish_create(&branch, 2, "table0001", &bytes)
            .expect("publish table object");
        let identity = TableIdentity::new("published-table").expect("identity");

        let object_reader = TableObjectReaderService::new(&backend)
            .open_reader(
                identity.clone(),
                write.facts(),
                TableReaderConfig::default(),
            )
            .expect("open object-backed reader");
        let byte_reader =
            ImmutableTableReader::open_bytes(identity, bytes.clone(), TableReaderConfig::default())
                .expect("open byte reader");

        assert_reader_matches(&object_reader, &byte_reader);
        assert_eq!(
            backend.range_reads(),
            vec![(
                write.facts().object().clone(),
                BackendRange::new(0, bytes.len() as u64)
            )]
        );
        assert_eq!(backend.metadata_calls(), 1);
        assert_eq!(
            backend.operations(),
            vec![(write.facts().object().clone(), PublishMode::Create)]
        );
        assert_eq!(backend.list_calls(), 0);
        assert_eq!(backend.write_calls(), 0);
        assert_eq!(backend.delete_calls(), 0);
    }

    #[test]
    fn table_object_reader_accepts_memory_backend_without_durable_capabilities() {
        let backend = MemoryBackend::new();
        let bytes = valid_table_bytes();
        let branch = branch_id().to_string();
        let object = ObjectLayout::table_object(&branch, 0, "table0001").expect("table object");
        backend.write_object(&object, &bytes).expect("seed object");
        let facts = facts_from_bytes(object, &bytes);
        let identity = TableIdentity::new("memory-table").expect("identity");

        let object_reader = TableObjectReaderService::new(&backend)
            .open_reader(identity.clone(), &facts, TableReaderConfig::default())
            .expect("open memory object-backed reader");
        let byte_reader =
            ImmutableTableReader::open_bytes(identity, bytes, TableReaderConfig::default())
                .expect("open byte reader");

        assert_reader_matches(&object_reader, &byte_reader);
        assert!(!backend
            .capabilities()
            .contains(BackendCapability::DurablePublish));
        assert!(!backend
            .capabilities()
            .contains(BackendCapability::DurableSync));
    }

    #[test]
    fn table_object_reader_allows_missing_metadata_capability() {
        let backend =
            RecordingBackend::durable().without_capability(BackendCapability::ObjectMetadata);
        let bytes = valid_table_bytes();
        let branch = branch_id().to_string();
        let object = ObjectLayout::table_object(&branch, 0, "table0001").expect("table object");
        backend.seed(object.clone(), &bytes);
        let facts = facts_from_bytes(object, &bytes);
        let identity = TableIdentity::new("no-metadata-table").expect("identity");

        let object_reader = TableObjectReaderService::new(&backend)
            .open_reader(identity.clone(), &facts, TableReaderConfig::default())
            .expect("open object-backed reader without metadata");
        let byte_reader =
            ImmutableTableReader::open_bytes(identity, bytes, TableReaderConfig::default())
                .expect("open byte reader");

        assert_reader_matches(&object_reader, &byte_reader);
        assert_eq!(backend.metadata_calls(), 0);
        assert_eq!(
            backend.range_reads(),
            vec![(
                facts.object().clone(),
                BackendRange::new(0, facts.byte_count())
            )]
        );
    }

    #[test]
    fn table_object_byte_source_enforces_capabilities_and_exact_ranges() {
        let object = ObjectLayout::table_object(&branch_id().to_string(), 0, "table0001")
            .expect("table object");
        let backend = RecordingBackend::durable().without_capability(BackendCapability::ReadRange);
        assert_eq!(
            TableObjectByteSource::new(&backend, object.clone(), 3)
                .err()
                .expect("read range needed"),
            TableObjectReadError::UnsupportedCapability {
                object: object.clone(),
                capability: BackendCapability::ReadRange,
            }
        );

        let backend = RecordingBackend::durable();
        backend.seed(object.clone(), b"abcdef");
        assert_eq!(
            TableObjectByteSource::new(&backend, object.clone(), 0)
                .err()
                .expect("zero byte count rejected"),
            TableObjectReadError::Source {
                object: object.clone(),
                reason: "table object byte count is zero",
            }
        );
        let source = TableObjectByteSource::new(&backend, object.clone(), 6).expect("source");
        assert_eq!(source.object(), &object);
        assert_eq!(source.byte_count(), 6);
        assert_eq!(source.read_exact_at(0, 0).expect("zero read"), b"");
        assert_eq!(source.read_exact_at(6, 0).expect("end zero read"), b"");
        assert!(backend.range_reads().is_empty());
        assert_eq!(source.read_exact_at(2, 3).expect("range read"), b"cde");
        assert_eq!(
            backend.range_reads(),
            vec![(object.clone(), BackendRange::new(2, 3))]
        );
        assert_eq!(
            source.read_exact_at(5, 2),
            Err(TableObjectReadError::Source {
                object: object.clone(),
                reason: "range exceeds table object byte count",
            })
        );
        assert_eq!(
            source.read_exact_at(u64::MAX, 1),
            Err(TableObjectReadError::Source {
                object,
                reason: "range end overflows",
            })
        );

        let object = ObjectLayout::table_object(&branch_id().to_string(), 0, "table0002")
            .expect("table object");
        let backend = RecordingBackend::durable().with_long_read();
        backend.seed(object.clone(), b"abcdef");
        let source = TableObjectByteSource::new(&backend, object.clone(), 6).expect("source");
        assert_eq!(
            source.read_exact_at(0, 3),
            Err(TableObjectReadError::Source {
                object,
                reason: "long table object range read",
            })
        );
    }

    #[test]
    fn table_object_reader_distinguishes_read_decode_and_fact_errors() {
        let branch = branch_id().to_string();
        let object = ObjectLayout::table_object(&branch, 0, "table0001").expect("table object");
        let bytes = valid_table_bytes();
        let facts = facts_from_bytes(object.clone(), &bytes);
        let identity = TableIdentity::new("fault-table").expect("identity");

        let missing = RecordingBackend::durable();
        assert!(matches!(
            TableObjectReaderService::new(&missing).open_reader(
                identity.clone(),
                &facts,
                TableReaderConfig::default()
            ),
            Err(TableObjectReadError::Backend {
                source,
                ..
            }) if source.kind() == BackendErrorKind::NotFound
        ));

        let interrupted = RecordingBackend::durable().with_read_range_failure(BackendError::new(
            BackendErrorKind::Interrupted,
            "injected read failure",
        ));
        interrupted.seed(object.clone(), &bytes);
        assert!(matches!(
            TableObjectReaderService::new(&interrupted).open_reader(
                identity.clone(),
                &facts,
                TableReaderConfig::default()
            ),
            Err(TableObjectReadError::Backend {
                source,
                ..
            }) if source.kind() == BackendErrorKind::Interrupted
        ));

        assert_reader_source_chain_preserves_backend_error(
            &interrupted,
            object.clone(),
            &facts,
            identity.clone(),
        );

        let short = RecordingBackend::durable().with_short_read();
        short.seed(object.clone(), &bytes);
        assert_eq!(
            TableObjectReaderService::new(&short).open_reader(
                identity.clone(),
                &facts,
                TableReaderConfig::default()
            ),
            Err(TableObjectReadError::Source {
                object: object.clone(),
                reason: "short table object range read",
            })
        );

        let corrupt = RecordingBackend::durable();
        corrupt.seed(object.clone(), b"not table");
        let corrupt_facts = TableObjectFacts {
            object: object.clone(),
            byte_count: 9,
            row_count: facts.row_count(),
            data_block_count: facts.data_block_count(),
            commit_min: facts.commit_min(),
            commit_max: facts.commit_max(),
        };
        assert!(matches!(
            TableObjectReaderService::new(&corrupt).open_reader(
                identity.clone(),
                &corrupt_facts,
                TableReaderConfig::default()
            ),
            Err(TableObjectReadError::Table {
                source: TableRuntimeError::DecodeFormat { .. },
                ..
            })
        ));

        let stale = RecordingBackend::durable();
        stale.seed(object.clone(), &bytes);
        let stale_facts = TableObjectFacts {
            object: object.clone(),
            row_count: facts.row_count() + 1,
            ..facts.clone()
        };
        assert_eq!(
            TableObjectReaderService::new(&stale).open_reader(
                identity,
                &stale_facts,
                TableReaderConfig::default()
            ),
            Err(TableObjectReadError::FactMismatch {
                object,
                field: "row_count",
            })
        );
    }

    #[test]
    fn table_object_reader_rejects_all_stale_l4_facts() {
        let branch = branch_id().to_string();
        let object = ObjectLayout::table_object(&branch, 0, "table0001").expect("table object");
        let bytes = valid_table_bytes();
        let facts = facts_from_bytes(object.clone(), &bytes);

        for (field, stale_facts) in [
            (
                "row_count",
                TableObjectFacts {
                    row_count: facts.row_count() + 1,
                    ..facts.clone()
                },
            ),
            (
                "data_block_count",
                TableObjectFacts {
                    data_block_count: facts.data_block_count() + 1,
                    ..facts.clone()
                },
            ),
            (
                "commit_min",
                TableObjectFacts {
                    commit_min: CommitVersion::new(facts.commit_min().as_u64() + 1),
                    ..facts.clone()
                },
            ),
            (
                "commit_max",
                TableObjectFacts {
                    commit_max: CommitVersion::new(facts.commit_max().as_u64() + 1),
                    ..facts.clone()
                },
            ),
        ] {
            let backend = RecordingBackend::durable();
            backend.seed(object.clone(), &bytes);
            let identity = TableIdentity::new(format!("stale-{field}")).expect("stale identity");

            assert_eq!(
                TableObjectReaderService::new(&backend).open_reader(
                    identity,
                    &stale_facts,
                    TableReaderConfig::default()
                ),
                Err(TableObjectReadError::FactMismatch {
                    object: object.clone(),
                    field,
                })
            );
        }
    }

    #[test]
    fn table_object_reader_rejects_stale_byte_count_when_metadata_is_available() {
        let branch = branch_id().to_string();
        let object = ObjectLayout::table_object(&branch, 0, "table0001").expect("table object");
        let bytes = valid_table_bytes();
        let facts = facts_from_bytes(object.clone(), &bytes);
        let identity = TableIdentity::new("stale-byte-count").expect("identity");

        let smaller_than_object = RecordingBackend::durable();
        let mut grown_bytes = bytes.clone();
        grown_bytes.extend_from_slice(b"stale-extra-bytes");
        smaller_than_object.seed(object.clone(), &grown_bytes);
        assert_eq!(
            TableObjectReaderService::new(&smaller_than_object).open_reader(
                identity.clone(),
                &facts,
                TableReaderConfig::default()
            ),
            Err(TableObjectReadError::FactMismatch {
                object: object.clone(),
                field: "byte_count",
            })
        );
        assert_eq!(smaller_than_object.metadata_calls(), 1);
        assert!(smaller_than_object.range_reads().is_empty());

        let larger_than_object = RecordingBackend::durable();
        larger_than_object.seed(object.clone(), &bytes);
        let stale_facts = TableObjectFacts {
            byte_count: facts.byte_count() + 1,
            ..facts
        };
        assert_eq!(
            TableObjectReaderService::new(&larger_than_object).open_reader(
                identity,
                &stale_facts,
                TableReaderConfig::default()
            ),
            Err(TableObjectReadError::FactMismatch {
                object,
                field: "byte_count",
            })
        );
        assert_eq!(larger_than_object.metadata_calls(), 1);
        assert!(larger_than_object.range_reads().is_empty());
    }

    #[test]
    fn table_object_reader_matches_byte_reader_for_queries_and_row_shapes() {
        let duplicate_key = physical_key_for(0x41, "object-reader", 0x20, b"multi\0version");
        let rows = vec![
            put_row_for_key(duplicate_key.clone(), 13, b"newer".to_vec()),
            put_row_for_key(duplicate_key.clone(), 4, b"older".to_vec()),
            tombstone_row_for_key(physical_key_for(0x41, "object-reader", 0x20, b"deleted"), 8),
            expired_row_for_key(physical_key_for(0x41, "object-reader", 0x21, b"expired"), 6),
            put_row_for_key(
                physical_key_for(0x42, "object-reader", 0x22, b"empty-value"),
                9,
                Vec::new(),
            ),
            put_row_for_key(
                physical_key_for(0x43, "object-reader", 0x23, b"large-value"),
                10,
                vec![0x7a; 12 * 1024],
            ),
        ];
        let (bytes, table_rows) = built_table_bytes(
            "object-query-source",
            &rows,
            2,
            TableCompression::Uncompressed,
        );
        let backend = RecordingBackend::durable();
        let branch = branch_id().to_string();
        let write = TableObjectService::new(&backend)
            .publish_create(&branch, 0, "table0001", &bytes)
            .expect("publish query table");
        let identity = TableIdentity::new("object-query-reader").expect("identity");

        let object_reader = TableObjectReaderService::new(&backend)
            .open_reader(
                identity.clone(),
                write.facts(),
                TableReaderConfig::default(),
            )
            .expect("open object-backed query reader");
        let byte_reader =
            ImmutableTableReader::open_bytes(identity, bytes, TableReaderConfig::default())
                .expect("open byte query reader");

        assert!(
            object_reader.facts().data_block_count() > 1,
            "test table should cover multiple blocks"
        );
        assert_reader_matches(&object_reader, &byte_reader);
        assert_reader_query_parity(&object_reader, &byte_reader, &table_rows, &duplicate_key);
        assert_eq!(backend.list_calls(), 0);
        assert_eq!(backend.write_calls(), 0);
        assert_eq!(backend.delete_calls(), 0);
    }

    #[test]
    fn table_object_reader_matches_byte_reader_for_zstd_and_cache_modes() {
        let rows = diverse_rows();
        let (bytes, table_rows) =
            built_table_bytes("object-zstd-source", &rows, 1, TableCompression::Zstd);
        let backend = RecordingBackend::durable();
        let branch = branch_id().to_string();
        let write = TableObjectService::new(&backend)
            .publish_create(&branch, 1, "table0002", &bytes)
            .expect("publish zstd table");
        let prefix_key = table_rows
            .first()
            .expect("at least one row")
            .physical_key()
            .clone();

        for (identity_text, config) in [
            ("object-zstd-cache-disabled", TableReaderConfig::new()),
            ("object-zstd-cache-enabled", TableReaderConfig::new()),
        ] {
            let identity = TableIdentity::new(identity_text).expect("identity");
            let object_reader = TableObjectReaderService::new(&backend)
                .open_reader(identity.clone(), write.facts(), config)
                .expect("open object-backed zstd reader");
            let byte_reader = ImmutableTableReader::open_bytes(identity, bytes.clone(), config)
                .expect("open byte zstd reader");

            assert_eq!(object_reader.config(), config);
            assert_reader_matches(&object_reader, &byte_reader);
            assert_reader_query_parity(&object_reader, &byte_reader, &table_rows, &prefix_key);
        }
    }

    #[test]
    fn table_object_reader_preserves_caller_identity_without_parsing_object_name() {
        let row = put_row_for_key(
            physical_key_for(0x44, "object-reader", 0x24, b"single"),
            42,
            b"one".to_vec(),
        );
        let (bytes, table_rows) = built_table_bytes(
            "object-identity-source",
            std::slice::from_ref(&row),
            8,
            TableCompression::Uncompressed,
        );
        let backend = RecordingBackend::durable();
        let branch = branch_id().to_string();
        let write = TableObjectService::new(&backend)
            .publish_create(&branch, 2, "table0003", &bytes)
            .expect("publish identity table");

        let first_identity = TableIdentity::new("opaque-reader-a").expect("identity a");
        let second_identity = TableIdentity::new("opaque-reader-b").expect("identity b");
        let first_reader = TableObjectReaderService::new(&backend)
            .open_reader(
                first_identity.clone(),
                write.facts(),
                TableReaderConfig::default(),
            )
            .expect("open first identity reader");
        let second_reader = TableObjectReaderService::new(&backend)
            .open_reader(
                second_identity.clone(),
                write.facts(),
                TableReaderConfig::default(),
            )
            .expect("open second identity reader");

        assert_eq!(first_reader.facts().identity(), &first_identity);
        assert_eq!(second_reader.facts().identity(), &second_identity);
        assert_ne!(
            first_reader.facts().identity(),
            second_reader.facts().identity()
        );
        assert_eq!(first_reader.rows(), table_rows.as_slice());
        assert_eq!(second_reader.rows(), table_rows.as_slice());
        assert_eq!(
            first_reader
                .rows()
                .iter()
                .map(TableRow::row)
                .collect::<Vec<_>>(),
            second_reader
                .rows()
                .iter()
                .map(TableRow::row)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn table_object_reader_routes_corruption_to_table_errors() {
        let rows = diverse_rows();
        let (bytes, _) = built_table_bytes(
            "object-corruption-source",
            &rows,
            2,
            TableCompression::Uncompressed,
        );
        let branch = branch_id().to_string();
        let object = ObjectLayout::table_object(&branch, 0, "table0004").expect("table object");
        let facts = facts_from_bytes(object.clone(), &bytes);

        for (label, corrupt_bytes) in [
            ("bad-magic", {
                let mut corrupt = bytes.clone();
                corrupt[0] ^= 0xff;
                corrupt
            }),
            ("bad-footer-crc", {
                let mut corrupt = bytes.clone();
                let last = corrupt.len() - 1;
                corrupt[last] ^= 0xff;
                corrupt
            }),
            ("legacy-magic", {
                let mut corrupt = bytes.clone();
                corrupt[..6].copy_from_slice(b"STRAKV");
                corrupt
            }),
        ] {
            let backend = RecordingBackend::durable();
            backend.seed(object.clone(), &corrupt_bytes);
            let identity = TableIdentity::new(format!("object-corrupt-{label}")).expect("identity");

            assert!(matches!(
                TableObjectReaderService::new(&backend).open_reader(
                    identity,
                    &facts,
                    TableReaderConfig::default()
                ),
                Err(TableObjectReadError::Table {
                    source: TableRuntimeError::DecodeFormat { .. },
                    ..
                })
            ));
        }
    }

    #[test]
    fn invalid_table_identity_is_rejected_before_object_read() {
        let backend = RecordingBackend::durable();
        assert!(matches!(
            TableIdentity::new("tables/branch/0000/table"),
            Err(TableRuntimeError::InvalidConfig {
                field: "table_identity",
                ..
            })
        ));
        assert!(backend.range_reads().is_empty());
    }

    #[cfg(all(feature = "localfs", unix))]
    #[test]
    fn table_object_publish_create_round_trips_and_reads_on_localfs() {
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
            bytes.clone()
        );

        let identity = TableIdentity::new("localfs-table").expect("identity");
        let object_reader = TableObjectReaderService::new(&backend)
            .open_reader(
                identity.clone(),
                write.facts(),
                TableReaderConfig::default(),
            )
            .expect("open localfs object-backed reader");
        let byte_reader =
            ImmutableTableReader::open_bytes(identity, bytes, TableReaderConfig::default())
                .expect("open byte reader");
        assert_reader_matches(&object_reader, &byte_reader);
    }

    fn assert_reader_matches(actual: &ImmutableTableReader, expected: &ImmutableTableReader) {
        assert_eq!(actual.facts(), expected.facts());
        assert_eq!(actual.byte_count(), expected.byte_count());
        assert_eq!(
            actual
                .rows()
                .iter()
                .map(|row| row.row().clone())
                .collect::<Vec<_>>(),
            expected
                .rows()
                .iter()
                .map(|row| row.row().clone())
                .collect::<Vec<_>>()
        );
    }

    fn assert_reader_query_parity(
        object_reader: &ImmutableTableReader,
        byte_reader: &ImmutableTableReader,
        table_rows: &[TableRow],
        prefix_key: &PhysicalKey,
    ) {
        assert_eq!(all_reader_rows(object_reader), all_reader_rows(byte_reader));

        for row in table_rows {
            assert_eq!(object_reader.get_exact(row.key()), Some(row.clone()));
            assert_eq!(
                object_reader.get_exact(row.key()),
                byte_reader.get_exact(row.key())
            );
        }

        let missing = TableInternalKeyBytes::from_row(&put_row_for_key(
            physical_key_for(0x7f, "object-reader", 0x2f, b"missing"),
            99,
            b"missing".to_vec(),
        ));
        assert_eq!(object_reader.get_exact(&missing), None);
        assert_eq!(
            object_reader.get_exact(&missing),
            byte_reader.get_exact(&missing)
        );

        let lower = table_rows[1].key().clone();
        let upper = table_rows[table_rows.len() - 2].key().clone();
        let closed = TableKeyBounds::closed(lower, upper).expect("closed bounds");
        assert_eq!(
            bounded_reader_rows(object_reader, closed.clone()),
            bounded_reader_rows(byte_reader, closed)
        );

        let prefix = TablePhysicalKeyBytes::from_physical_key(prefix_key);
        let prefix_bounds = TableKeyBounds::prefix(prefix.as_slice());
        assert_eq!(
            bounded_reader_rows(object_reader, prefix_bounds.clone()),
            bounded_reader_rows(byte_reader, prefix_bounds)
        );
    }

    fn all_reader_rows(reader: &ImmutableTableReader) -> Vec<StorageRow> {
        let mut cursor = reader.cursor();
        cursor.seek_to_first().expect("seek first");
        collect_cursor_rows(&mut cursor)
    }

    fn bounded_reader_rows(
        reader: &ImmutableTableReader,
        bounds: TableKeyBounds,
    ) -> Vec<StorageRow> {
        let mut cursor = reader.bounded_cursor(bounds);
        cursor.seek_to_first().expect("seek bounded");
        collect_cursor_rows(&mut cursor)
    }

    fn collect_cursor_rows(cursor: &mut impl TableCursor) -> Vec<StorageRow> {
        let mut rows = Vec::new();
        while let Some(row) = cursor.current() {
            rows.push(row.row().clone());
            cursor.advance().expect("advance cursor");
        }
        rows
    }

    fn valid_table_bytes() -> Vec<u8> {
        let rows = vec![row(b"alpha".to_vec(), 9), row(b"beta".to_vec(), 7)];
        encode_immutable_table(&rows, 4096, 8, TableCompression::Uncompressed)
            .expect("encode immutable table")
    }

    fn built_table_bytes(
        identity_text: &'static str,
        rows: &[StorageRow],
        rows_per_block: usize,
        compression: TableCompression,
    ) -> (Vec<u8>, Vec<TableRow>) {
        let mut table_rows = rows.iter().cloned().map(TableRow::new).collect::<Vec<_>>();
        sort_table_rows_by_key(&mut table_rows);
        let builder = ImmutableTableBuilder::new(
            TableBuilderConfig::new(1024, rows_per_block, compression).expect("builder config"),
        )
        .expect("builder");
        let artifact = builder
            .build_from_rows(
                TableIdentity::new(identity_text).expect("artifact identity"),
                &table_rows,
            )
            .expect("build table artifact");
        (artifact.into_bytes(), table_rows)
    }

    fn facts_from_bytes(object: ObjectName, bytes: &[u8]) -> TableObjectFacts {
        let table = decode_immutable_table(bytes).expect("decode table");
        TableObjectFacts::from_table(object, bytes, &table).expect("table facts")
    }

    fn assert_reader_source_chain_preserves_backend_error(
        backend: &RecordingBackend,
        object: ObjectName,
        facts: &TableObjectFacts,
        identity: TableIdentity,
    ) {
        let source = TableObjectByteSource::new(backend, object, facts.byte_count())
            .expect("table object source");
        let runtime_error =
            ImmutableTableReader::open_source(identity, &source, TableReaderConfig::default())
                .expect_err("object source failure should preserve source chain");
        let table_source = std::error::Error::source(&runtime_error)
            .expect("table runtime source read error should expose object read source");
        assert!(table_source
            .to_string()
            .contains("failed to read immutable table object"));
        let backend_source = std::error::Error::source(table_source)
            .expect("object read source should expose backend source");
        assert!(backend_source.to_string().contains("injected read failure"));
    }

    fn diverse_rows() -> Vec<StorageRow> {
        let duplicate_key = physical_key_for(0x55, "object-reader", 0x20, b"duplicate\0key");
        vec![
            put_row_for_key(duplicate_key.clone(), 12, b"newer".to_vec()),
            put_row_for_key(duplicate_key, 2, b"older".to_vec()),
            tombstone_row_for_key(physical_key_for(0x55, "object-reader", 0x21, b"deleted"), 7),
            expired_row_for_key(physical_key_for(0x56, "object-reader", 0x22, b"expired"), 5),
            put_row_for_key(
                physical_key_for(0x57, "object-reader", 0x23, b"empty"),
                9,
                Vec::new(),
            ),
            put_row_for_key(
                physical_key_for(0x58, "object-reader", 0x24, b"tail"),
                10,
                vec![0x11, 0x22, 0x33],
            ),
        ]
    }

    fn physical_key_for(
        branch_byte: u8,
        storage_space: &'static str,
        storage_space_id: u8,
        user_key: impl Into<Vec<u8>>,
    ) -> PhysicalKey {
        PhysicalKey::new(
            BranchId::from_bytes([branch_byte; BranchId::BYTE_LEN]),
            storage_space,
            StorageSpaceId::from_raw(storage_space_id).expect("storage space id"),
            user_key,
        )
        .expect("physical key")
    }

    fn put_row_for_key(key: PhysicalKey, version: u64, value: Vec<u8>) -> StorageRow {
        StorageRow::put(
            key,
            CommitVersion::new(version),
            Timestamp::from_micros(1_700_000_000_000_000 + version),
            Timestamp::EPOCH,
            value,
        )
    }

    fn expired_row_for_key(key: PhysicalKey, version: u64) -> StorageRow {
        StorageRow::put(
            key,
            CommitVersion::new(version),
            Timestamp::from_micros(1_700_000_000_000_000 + version),
            Timestamp::from_micros(1),
            b"expired".to_vec(),
        )
    }

    fn tombstone_row_for_key(key: PhysicalKey, version: u64) -> StorageRow {
        StorageRow::tombstone(
            key,
            CommitVersion::new(version),
            Timestamp::from_micros(1_700_000_000_000_000 + version),
        )
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
