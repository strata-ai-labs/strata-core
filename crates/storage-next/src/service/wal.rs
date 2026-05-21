//! WAL service mechanics over storage-next backend objects.

#![cfg_attr(
    any(not(test), all(test, not(feature = "localfs"))),
    expect(
        dead_code,
        reason = "WAL service is consumed by later commit and lifecycle layers; no-localfs test builds only exercise unsupported durable construction"
    )
)]

use crate::backend::{
    Backend, BackendCapability, BackendError, BackendErrorKind, BackendRange, PublishError,
};
use crate::config::mode::DurabilityPolicy;
use crate::format::{
    decode_wal_record, decode_wal_record_envelope, decode_wal_segment_header, encode_wal_record,
    encode_wal_record_envelope, encode_wal_segment_header, FormatError, SegmentMetadata, WalRecord,
    WalRecordEnvelope, WalSegmentHeader, WAL_SEGMENT_HEADER_SIZE,
};
use crate::layout::{LayoutError, ObjectFamily, ObjectLayout};
use crate::object::ObjectName;
use crate::service::{validate_publish_outcome, ObjectPublisher};
use std::borrow::Cow;
use std::fmt;
use strata_core_next::CommitVersion;

const DEFAULT_SEGMENT_SIZE: u64 = 64 * 1024 * 1024;
const MIN_SEGMENT_SIZE: u64 = 1024;
const WAL_SEGMENT_COMPONENT_LEN: usize = 16;
const IDENTITY_CODEC_ID: &str = "identity";

pub(crate) type WalServiceResult<T> = Result<T, WalServiceError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WalServiceConfig {
    segment_size: u64,
    codec_id: &'static str,
}

impl WalServiceConfig {
    pub(crate) const fn new(segment_size: u64) -> Self {
        Self {
            segment_size,
            codec_id: IDENTITY_CODEC_ID,
        }
    }

    pub(crate) const fn with_codec(segment_size: u64, codec_id: &'static str) -> Self {
        Self {
            segment_size,
            codec_id,
        }
    }

    pub(crate) const fn segment_size(self) -> u64 {
        self.segment_size
    }

    pub(crate) const fn codec_id(self) -> &'static str {
        self.codec_id
    }

    fn validate(self) -> WalServiceResult<()> {
        if self.segment_size < MIN_SEGMENT_SIZE {
            Err(WalServiceError::InvalidConfig {
                field: "segment_size",
            })
        } else if self.codec_id != IDENTITY_CODEC_ID {
            Err(WalServiceError::InvalidConfig { field: "codec_id" })
        } else {
            Ok(())
        }
    }
}

impl Default for WalServiceConfig {
    fn default() -> Self {
        Self {
            segment_size: DEFAULT_SEGMENT_SIZE,
            codec_id: IDENTITY_CODEC_ID,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WalRetentionProof {
    covered_through: CommitVersion,
    source: WalRetentionProofSource,
}

impl WalRetentionProof {
    pub(crate) const fn snapshot_watermark(covered_through: CommitVersion) -> Self {
        Self {
            covered_through,
            source: WalRetentionProofSource::SnapshotWatermark,
        }
    }

    pub(crate) const fn flush_watermark(covered_through: CommitVersion) -> Self {
        Self {
            covered_through,
            source: WalRetentionProofSource::FlushWatermark,
        }
    }

    pub(crate) const fn covered_through(self) -> CommitVersion {
        self.covered_through
    }

    pub(crate) const fn source(self) -> WalRetentionProofSource {
        self.source
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WalRetentionProofSource {
    SnapshotWatermark,
    FlushWatermark,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WalOperation {
    Open,
    CreateSegment,
    Append,
    Sync,
    Repair,
    List,
    Read,
}

impl WalOperation {
    const fn name(self) -> &'static str {
        match self {
            Self::Open => "open WAL",
            Self::CreateSegment => "create WAL segment",
            Self::Append => "append WAL record",
            Self::Sync => "sync WAL segment",
            Self::Repair => "repair WAL segment",
            Self::List => "list WAL segments",
            Self::Read => "read WAL segment",
        }
    }
}

impl fmt::Display for WalOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WalServiceError {
    InvalidConfig {
        field: &'static str,
    },
    UnsupportedCapability {
        capability: BackendCapability,
    },
    InvalidSegmentId {
        segment_id: u64,
    },
    Layout {
        source: LayoutError,
    },
    Backend {
        operation: WalOperation,
        object: ObjectName,
        source: BackendError,
    },
    List {
        source: BackendError,
    },
    Publish {
        operation: WalOperation,
        source: PublishError,
    },
    Format {
        operation: WalOperation,
        object: ObjectName,
        source: FormatError,
    },
    DatabaseMismatch {
        object: ObjectName,
        segment_id: u64,
    },
    RecordTooLarge {
        bytes: u64,
        segment_size: u64,
    },
    SegmentIdOverflow {
        segment_id: u64,
    },
    TruncationSegmentMismatch {
        segment_id: u64,
        active_segment_id: u64,
    },
    InvalidTruncation {
        segment_id: u64,
        valid_end_offset: u64,
        object_size: u64,
    },
    RepairUncertain {
        segment_id: u64,
    },
    UnexpectedAppendOffset {
        object: ObjectName,
        expected: u64,
        actual: u64,
    },
    UnexpectedAppendLength {
        object: ObjectName,
        expected: u64,
        actual: u64,
    },
    UnexpectedObjectSize {
        object: ObjectName,
        expected: u64,
        actual: u64,
    },
}

impl fmt::Display for WalServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig { field } => write!(formatter, "invalid WAL config field {field}"),
            Self::UnsupportedCapability { capability } => {
                write!(
                    formatter,
                    "backend does not support WAL capability {capability}"
                )
            }
            Self::InvalidSegmentId { segment_id } => {
                write!(formatter, "WAL segment id {segment_id} is invalid")
            }
            Self::Layout { source } => {
                write!(formatter, "failed to build WAL object name: {source}")
            }
            Self::Backend {
                operation,
                object,
                source,
            } => write!(formatter, "failed to {operation} at {object}: {source}"),
            Self::List { source } => write!(formatter, "failed to list WAL segments: {source}"),
            Self::Publish { operation, source } => {
                write!(formatter, "failed to {operation}: {source}")
            }
            Self::Format {
                operation,
                object,
                source,
            } => write!(
                formatter,
                "failed to {operation} bytes at {object}: {source}"
            ),
            Self::DatabaseMismatch { object, segment_id } => write!(
                formatter,
                "WAL segment {segment_id} at {object} belongs to a different database"
            ),
            Self::RecordTooLarge {
                bytes,
                segment_size,
            } => write!(
                formatter,
                "WAL record frame of {bytes} bytes exceeds segment size {segment_size}"
            ),
            Self::SegmentIdOverflow { segment_id } => {
                write!(formatter, "WAL segment id overflow after {segment_id}")
            }
            Self::TruncationSegmentMismatch {
                segment_id,
                active_segment_id,
            } => write!(
                formatter,
                "WAL truncation fact for segment {segment_id} does not match active segment {active_segment_id}"
            ),
            Self::InvalidTruncation {
                segment_id,
                valid_end_offset,
                object_size,
            } => write!(
                formatter,
                "invalid WAL truncation fact for segment {segment_id}: valid_end_offset {valid_end_offset} must be strictly less than object_size {object_size}"
            ),
            Self::RepairUncertain { segment_id } => write!(
                formatter,
                "WAL segment {segment_id} repair durability is uncertain; reopen before appending"
            ),
            Self::UnexpectedAppendOffset {
                object,
                expected,
                actual,
            } => write!(
                formatter,
                "WAL append at {object} started at offset {actual}, expected {expected}"
            ),
            Self::UnexpectedAppendLength {
                object,
                expected,
                actual,
            } => write!(
                formatter,
                "WAL append at {object} wrote {actual} bytes, expected {expected}"
            ),
            Self::UnexpectedObjectSize {
                object,
                expected,
                actual,
            } => write!(
                formatter,
                "WAL object {object} has size {actual}, expected {expected}"
            ),
        }
    }
}

impl std::error::Error for WalServiceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Layout { source } => Some(source),
            Self::Backend { source, .. } | Self::List { source } => Some(source),
            Self::Publish { source, .. } => Some(source),
            Self::Format { source, .. } => Some(source),
            Self::InvalidConfig { .. }
            | Self::UnsupportedCapability { .. }
            | Self::InvalidSegmentId { .. }
            | Self::DatabaseMismatch { .. }
            | Self::RecordTooLarge { .. }
            | Self::SegmentIdOverflow { .. }
            | Self::TruncationSegmentMismatch { .. }
            | Self::InvalidTruncation { .. }
            | Self::RepairUncertain { .. }
            | Self::UnexpectedAppendOffset { .. }
            | Self::UnexpectedAppendLength { .. }
            | Self::UnexpectedObjectSize { .. } => None,
        }
    }
}

impl WalServiceError {
    pub(crate) const fn is_writer_halted_append_failure(&self) -> bool {
        matches!(self, Self::RepairUncertain { .. })
    }

    pub(crate) const fn is_durability_uncertain_append_failure(&self) -> bool {
        matches!(
            self,
            Self::Backend {
                operation: WalOperation::Sync,
                ..
            } | Self::UnexpectedAppendOffset { .. }
                | Self::UnexpectedAppendLength { .. }
                | Self::UnexpectedObjectSize { .. }
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WalAppend {
    segment_id: u64,
    start_offset: u64,
    bytes_written: u64,
    dirty_bytes: u64,
    forced_durable: bool,
}

impl WalAppend {
    const fn new(
        segment_id: u64,
        start_offset: u64,
        bytes_written: u64,
        dirty_bytes: u64,
        forced_durable: bool,
    ) -> Self {
        Self {
            segment_id,
            start_offset,
            bytes_written,
            dirty_bytes,
            forced_durable,
        }
    }

    pub(crate) const fn segment_id(&self) -> u64 {
        self.segment_id
    }

    pub(crate) const fn start_offset(&self) -> u64 {
        self.start_offset
    }

    pub(crate) const fn bytes_written(&self) -> u64 {
        self.bytes_written
    }

    pub(crate) const fn dirty_bytes(&self) -> u64 {
        self.dirty_bytes
    }

    pub(crate) const fn forced_durable(&self) -> bool {
        self.forced_durable
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WalTruncation {
    segment_id: u64,
    valid_end_offset: u64,
    object_size: u64,
}

impl WalTruncation {
    const fn new(segment_id: u64, valid_end_offset: u64, object_size: u64) -> Self {
        Self {
            segment_id,
            valid_end_offset,
            object_size,
        }
    }

    pub(crate) const fn segment_id(&self) -> u64 {
        self.segment_id
    }

    pub(crate) const fn valid_end_offset(&self) -> u64 {
        self.valid_end_offset
    }

    pub(crate) const fn object_size(&self) -> u64 {
        self.object_size
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WalRead {
    records: Vec<WalRecord>,
    truncation: Option<WalTruncation>,
}

impl WalRead {
    fn new(records: Vec<WalRecord>, truncation: Option<WalTruncation>) -> Self {
        Self {
            records,
            truncation,
        }
    }

    pub(crate) fn records(&self) -> &[WalRecord] {
        &self.records
    }

    pub(crate) const fn truncation(&self) -> Option<&WalTruncation> {
        self.truncation.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WalRepair {
    segment_id: u64,
    valid_end_offset: u64,
    removed_bytes: u64,
}

impl WalRepair {
    const fn new(segment_id: u64, valid_end_offset: u64, removed_bytes: u64) -> Self {
        Self {
            segment_id,
            valid_end_offset,
            removed_bytes,
        }
    }

    pub(crate) const fn segment_id(&self) -> u64 {
        self.segment_id
    }

    pub(crate) const fn valid_end_offset(&self) -> u64 {
        self.valid_end_offset
    }

    pub(crate) const fn removed_bytes(&self) -> u64 {
        self.removed_bytes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WalDeleteReport {
    deleted: Vec<u64>,
    protected: Vec<u64>,
    failed: Vec<u64>,
}

impl WalDeleteReport {
    fn new() -> Self {
        Self {
            deleted: Vec::new(),
            protected: Vec::new(),
            failed: Vec::new(),
        }
    }

    pub(crate) fn deleted_segments(&self) -> &[u64] {
        &self.deleted
    }

    pub(crate) fn protected_segments(&self) -> &[u64] {
        &self.protected
    }

    pub(crate) fn failed_segments(&self) -> &[u64] {
        &self.failed
    }
}

pub(crate) struct WalService<'a> {
    backend: &'a dyn Backend,
    database_id: [u8; 16],
    active_segment_id: u64,
    active_object: ObjectName,
    active_segment_size: u64,
    segment_size: u64,
    codec_id: &'static str,
    durability_policy: DurabilityPolicy,
    dirty_bytes: u64,
    dirty_records: u64,
    active_metadata: SegmentMetadata,
    repair_uncertain: bool,
}

impl<'a> WalService<'a> {
    pub(crate) fn open(
        backend: &'a dyn Backend,
        database_id: [u8; 16],
        active_segment_id: u64,
        durability_policy: DurabilityPolicy,
        config: WalServiceConfig,
    ) -> WalServiceResult<Self> {
        config.validate()?;
        validate_segment_id(active_segment_id)?;
        require_capabilities(backend, WAL_REQUIRED_CAPABILITIES)?;
        let (active_object, active_segment_size, active_metadata) =
            open_or_create_segment(backend, database_id, active_segment_id, config.codec_id())?;

        Ok(Self {
            backend,
            database_id,
            active_segment_id,
            active_object,
            active_segment_size,
            segment_size: config.segment_size(),
            codec_id: config.codec_id(),
            durability_policy,
            dirty_bytes: 0,
            dirty_records: 0,
            active_metadata,
            repair_uncertain: false,
        })
    }

    pub(crate) const fn active_segment_id(&self) -> u64 {
        self.active_segment_id
    }

    pub(crate) const fn dirty_bytes(&self) -> u64 {
        self.dirty_bytes
    }

    pub(crate) const fn dirty_records(&self) -> u64 {
        self.dirty_records
    }

    pub(crate) const fn active_metadata(&self) -> &SegmentMetadata {
        &self.active_metadata
    }

    pub(crate) const fn durability_policy(&self) -> DurabilityPolicy {
        self.durability_policy
    }

    pub(crate) fn append(&mut self, record: &WalRecord) -> WalServiceResult<WalAppend> {
        if self.repair_uncertain {
            return Err(WalServiceError::RepairUncertain {
                segment_id: self.active_segment_id,
            });
        }

        let frame = encode_record_frame(record, &self.active_object, self.codec_id)?;
        let frame_len = frame.len() as u64;
        // The segment header consumes part of the configured segment budget, so
        // a single record frame must fit in the remaining capacity before any
        // backend append is attempted.
        let max_record_bytes = self
            .segment_size
            .checked_sub(WAL_SEGMENT_HEADER_SIZE as u64)
            .ok_or(WalServiceError::InvalidConfig {
                field: "segment_size",
            })?;
        if frame_len > max_record_bytes {
            return Err(WalServiceError::RecordTooLarge {
                bytes: frame_len,
                segment_size: self.segment_size,
            });
        }

        self.validate_active_object_size(WalOperation::Append)?;

        // Rotation is decided against service state that was just reconciled
        // with backend metadata. That prevents appending after an unrepaired
        // partial tail or externally-mutated active segment.
        let projected_size = self.active_segment_size.checked_add(frame_len).ok_or(
            WalServiceError::RecordTooLarge {
                bytes: frame_len,
                segment_size: self.segment_size,
            },
        )?;
        if projected_size > self.segment_size {
            self.rotate_segment()?;
            self.validate_active_object_size(WalOperation::Append)?;
        }

        let expected_offset = self.active_segment_size;
        let append = self
            .backend
            .append_object(&self.active_object, &frame)
            .map_err(|source| WalServiceError::Backend {
                operation: WalOperation::Append,
                object: self.active_object.clone(),
                source,
            })?;
        if append.start_offset() != expected_offset {
            return Err(WalServiceError::UnexpectedAppendOffset {
                object: self.active_object.clone(),
                expected: expected_offset,
                actual: append.start_offset(),
            });
        }
        if append.bytes_written() != frame_len {
            return Err(WalServiceError::UnexpectedAppendLength {
                object: self.active_object.clone(),
                expected: frame_len,
                actual: append.bytes_written(),
            });
        }
        let expected_size =
            expected_offset
                .checked_add(frame_len)
                .ok_or(WalServiceError::RecordTooLarge {
                    bytes: frame_len,
                    segment_size: self.segment_size,
                })?;
        if append.metadata().size_bytes() != expected_size {
            return Err(WalServiceError::UnexpectedObjectSize {
                object: self.active_object.clone(),
                expected: expected_size,
                actual: append.metadata().size_bytes(),
            });
        }

        self.active_segment_size = append.metadata().size_bytes();
        self.dirty_bytes = self.dirty_bytes.saturating_add(frame_len);
        self.dirty_records = self.dirty_records.saturating_add(1);
        self.active_metadata
            .track_record(record.commit_version(), record.commit_timestamp());

        // In always mode the append is already visible when sync runs. If sync
        // fails, dirty facts intentionally remain advanced so lifecycle can
        // classify the durability-uncertain window.
        let forced_durable = if self.durability_policy == DurabilityPolicy::Always {
            self.force_durable()?;
            true
        } else {
            false
        };

        Ok(WalAppend::new(
            self.active_segment_id,
            append.start_offset(),
            append.bytes_written(),
            self.dirty_bytes,
            forced_durable,
        ))
    }

    pub(crate) fn force_durable(&mut self) -> WalServiceResult<()> {
        self.backend
            .sync_object(&self.active_object)
            .map_err(|source| WalServiceError::Backend {
                operation: WalOperation::Sync,
                object: self.active_object.clone(),
                source,
            })?;
        self.dirty_bytes = 0;
        self.dirty_records = 0;
        Ok(())
    }

    pub(crate) fn close(&mut self) -> WalServiceResult<()> {
        if self.dirty_bytes > 0 {
            self.force_durable()?;
        }
        Ok(())
    }

    pub(crate) fn read_all(&self) -> WalServiceResult<WalRead> {
        let segments = list_segments(self.backend)?;
        let latest_segment_id = segments.last().map(|segment| segment.segment_id);
        let mut records = Vec::new();
        let mut truncation = None;

        for segment in segments {
            let is_latest = latest_segment_id == Some(segment.segment_id);
            let read = read_segment(
                self.backend,
                self.database_id,
                segment.segment_id,
                &segment.object,
                is_latest,
                self.codec_id,
            )?;
            records.extend(read.records);
            if read.truncation.is_some() {
                truncation = read.truncation;
                break;
            }
        }

        Ok(WalRead::new(records, truncation))
    }

    pub(crate) fn read_after_commit_version(
        &self,
        watermark: CommitVersion,
    ) -> WalServiceResult<WalRead> {
        let read = self.read_all()?;
        let records = read
            .records
            .into_iter()
            .filter(|record| record.commit_version() > watermark)
            .collect();
        Ok(WalRead::new(records, read.truncation))
    }

    pub(crate) fn repair_latest_tail(
        &mut self,
        truncation: &WalTruncation,
    ) -> WalServiceResult<WalRepair> {
        if truncation.segment_id() != self.active_segment_id {
            return Err(WalServiceError::TruncationSegmentMismatch {
                segment_id: truncation.segment_id(),
                active_segment_id: self.active_segment_id,
            });
        }
        if truncation.valid_end_offset() >= truncation.object_size() {
            return Err(WalServiceError::InvalidTruncation {
                segment_id: truncation.segment_id(),
                valid_end_offset: truncation.valid_end_offset(),
                object_size: truncation.object_size(),
            });
        }

        self.validate_active_object_size_against(WalOperation::Repair, truncation.object_size())?;
        let prefix = self
            .backend
            .read_range(
                &self.active_object,
                BackendRange::new(0, truncation.valid_end_offset()),
            )
            .map_err(|source| WalServiceError::Backend {
                operation: WalOperation::Repair,
                object: self.active_object.clone(),
                source,
            })?;
        let prefix_len = prefix.len() as u64;
        if prefix_len != truncation.valid_end_offset() {
            return Err(WalServiceError::UnexpectedObjectSize {
                object: self.active_object.clone(),
                expected: truncation.valid_end_offset(),
                actual: prefix_len,
            });
        }

        let repaired_read = decode_segment_bytes(
            self.database_id,
            self.active_segment_id,
            &self.active_object,
            &prefix,
            false,
            self.codec_id,
            WalOperation::Repair,
        )?;
        let outcome = match ObjectPublisher::new(self.backend)
            .publish_durable_replace(&self.active_object, &prefix)
        {
            Ok(outcome) => outcome,
            Err(source) => {
                self.repair_uncertain = true;
                return Err(WalServiceError::Publish {
                    operation: WalOperation::Repair,
                    source,
                });
            }
        };
        if let Err(error) = validate_wal_publish_outcome(
            WalOperation::Repair,
            &self.active_object,
            truncation.valid_end_offset(),
            &outcome,
        ) {
            self.repair_uncertain = true;
            return Err(error);
        }

        self.active_segment_size = truncation.valid_end_offset();
        self.active_metadata =
            segment_metadata_from_records(self.active_segment_id, repaired_read.records());
        self.dirty_bytes = 0;
        self.dirty_records = 0;
        self.repair_uncertain = false;
        Ok(WalRepair::new(
            truncation.segment_id(),
            truncation.valid_end_offset(),
            truncation.object_size() - truncation.valid_end_offset(),
        ))
    }

    pub(crate) fn delete_covered_segments(
        &self,
        retention_proof: WalRetentionProof,
    ) -> WalServiceResult<WalDeleteReport> {
        require_capability(self.backend, BackendCapability::DeleteObject)?;
        let mut report = WalDeleteReport::new();
        let covered_through = retention_proof.covered_through();

        for segment in list_segments(self.backend)? {
            if segment.segment_id >= self.active_segment_id {
                report.protected.push(segment.segment_id);
                continue;
            }

            let read = read_segment(
                self.backend,
                self.database_id,
                segment.segment_id,
                &segment.object,
                false,
                self.codec_id,
            )?;
            if read
                .records
                .iter()
                .all(|record| record.commit_version() <= covered_through)
            {
                match self.backend.delete_object(&segment.object) {
                    Ok(()) => {
                        report.deleted.push(segment.segment_id);
                        self.delete_segment_sidecar_best_effort(segment.segment_id);
                    }
                    Err(source) if source.kind() == BackendErrorKind::NotFound => {
                        report.deleted.push(segment.segment_id);
                        self.delete_segment_sidecar_best_effort(segment.segment_id);
                    }
                    Err(_) => report.failed.push(segment.segment_id),
                }
            } else {
                report.protected.push(segment.segment_id);
            }
        }

        Ok(report)
    }

    fn delete_segment_sidecar_best_effort(&self, segment_id: u64) {
        // Segment metadata sidecars are optional accelerators. Once a WAL
        // segment is pruned, its sidecar is unreachable recovery state and can
        // be removed, but sidecar deletion must not turn authoritative WAL
        // retention into a failure.
        let Ok(sidecar) = ObjectLayout::wal_segment_metadata(segment_id) else {
            return;
        };
        let _ = self.backend.delete_object(&sidecar);
    }

    fn rotate_segment(&mut self) -> WalServiceResult<()> {
        // Old segment bytes must be durable before the active pointer advances
        // to a freshly created segment. Recovery can then replay all complete
        // segments up to the active segment without losing the rotation record.
        if self.dirty_bytes > 0 {
            self.force_durable()?;
        }

        let next_segment_id =
            self.active_segment_id
                .checked_add(1)
                .ok_or(WalServiceError::SegmentIdOverflow {
                    segment_id: self.active_segment_id,
                })?;
        let (next_object, next_size, next_metadata) =
            create_segment(self.backend, self.database_id, next_segment_id)?;

        self.active_segment_id = next_segment_id;
        self.active_object = next_object;
        self.active_segment_size = next_size;
        self.active_metadata = next_metadata;
        Ok(())
    }

    fn validate_active_object_size(&self, operation: WalOperation) -> WalServiceResult<()> {
        self.validate_active_object_size_against(operation, self.active_segment_size)
    }

    fn validate_active_object_size_against(
        &self,
        operation: WalOperation,
        expected_size: u64,
    ) -> WalServiceResult<()> {
        let actual_size = self
            .backend
            .object_metadata(&self.active_object)
            .map_err(|source| WalServiceError::Backend {
                operation,
                object: self.active_object.clone(),
                source,
            })?
            .size_bytes();
        if actual_size == expected_size {
            Ok(())
        } else {
            Err(WalServiceError::UnexpectedAppendOffset {
                object: self.active_object.clone(),
                expected: expected_size,
                actual: actual_size,
            })
        }
    }
}

const WAL_REQUIRED_CAPABILITIES: &[BackendCapability] = &[
    BackendCapability::ReadObject,
    BackendCapability::ReadRange,
    BackendCapability::ListPrefix,
    BackendCapability::ObjectMetadata,
    BackendCapability::AppendObject,
    BackendCapability::DurablePublish,
    BackendCapability::DurableSync,
];

#[derive(Clone, Debug, Eq, PartialEq)]
struct WalSegmentObject {
    segment_id: u64,
    object: ObjectName,
}

fn validate_segment_id(segment_id: u64) -> WalServiceResult<()> {
    if segment_id == 0 {
        Err(WalServiceError::InvalidSegmentId { segment_id })
    } else {
        Ok(())
    }
}

fn require_capabilities(
    backend: &dyn Backend,
    capabilities: &[BackendCapability],
) -> WalServiceResult<()> {
    for capability in capabilities {
        require_capability(backend, *capability)?;
    }
    Ok(())
}

fn require_capability(
    backend: &dyn Backend,
    capability: BackendCapability,
) -> WalServiceResult<()> {
    if backend.capabilities().contains(capability) {
        Ok(())
    } else {
        Err(WalServiceError::UnsupportedCapability { capability })
    }
}

fn segment_object(segment_id: u64) -> WalServiceResult<ObjectName> {
    ObjectLayout::wal_segment(segment_id).map_err(|source| WalServiceError::Layout { source })
}

fn open_or_create_segment(
    backend: &dyn Backend,
    database_id: [u8; 16],
    segment_id: u64,
    codec_id: &str,
) -> WalServiceResult<(ObjectName, u64, SegmentMetadata)> {
    let object = segment_object(segment_id)?;
    match backend.object_metadata(&object) {
        Ok(metadata) => {
            let read = read_segment(backend, database_id, segment_id, &object, true, codec_id)?;
            let segment_size = read
                .truncation
                .as_ref()
                .map_or(metadata.size_bytes(), WalTruncation::valid_end_offset);
            let active_metadata = segment_metadata_from_records(segment_id, &read.records);
            Ok((object, segment_size, active_metadata))
        }
        Err(source) if source.kind() == BackendErrorKind::NotFound => {
            create_segment(backend, database_id, segment_id)
        }
        Err(source) => Err(WalServiceError::Backend {
            operation: WalOperation::Open,
            object,
            source,
        }),
    }
}

fn create_segment(
    backend: &dyn Backend,
    database_id: [u8; 16],
    segment_id: u64,
) -> WalServiceResult<(ObjectName, u64, SegmentMetadata)> {
    let object = segment_object(segment_id)?;
    let header = WalSegmentHeader::new(segment_id, database_id);
    let bytes = encode_wal_segment_header(&header);
    let outcome = ObjectPublisher::new(backend)
        .publish_durable_create(&object, &bytes)
        .map_err(|source| WalServiceError::Publish {
            operation: WalOperation::CreateSegment,
            source,
        })?;
    validate_wal_publish_outcome(
        WalOperation::CreateSegment,
        &object,
        bytes.len() as u64,
        &outcome,
    )?;
    Ok((
        object,
        outcome.metadata().size_bytes(),
        SegmentMetadata::empty(segment_id),
    ))
}

fn validate_wal_publish_outcome(
    operation: WalOperation,
    object: &ObjectName,
    byte_count: u64,
    outcome: &crate::backend::PublishOutcome,
) -> WalServiceResult<()> {
    validate_publish_outcome(object, byte_count, outcome).map_err(|mismatch| {
        WalServiceError::Backend {
            operation,
            object: mismatch.object().clone(),
            source: BackendError::new(
                BackendErrorKind::MetadataMismatch,
                format!("WAL publish returned invalid {} metadata", mismatch.field()),
            ),
        }
    })
}

fn encode_record_frame(
    record: &WalRecord,
    object: &ObjectName,
    codec_id: &str,
) -> WalServiceResult<Vec<u8>> {
    let record_bytes = encode_wal_record(record).map_err(|source| WalServiceError::Format {
        operation: WalOperation::Append,
        object: object.clone(),
        source,
    })?;
    let record_bytes = encode_wal_codec_bytes(codec_id, record_bytes)?;
    let envelope =
        WalRecordEnvelope::new(record_bytes).map_err(|source| WalServiceError::Format {
            operation: WalOperation::Append,
            object: object.clone(),
            source,
        })?;
    encode_wal_record_envelope(&envelope).map_err(|source| WalServiceError::Format {
        operation: WalOperation::Append,
        object: object.clone(),
        source,
    })
}

fn encode_wal_codec_bytes(codec_id: &str, bytes: Vec<u8>) -> WalServiceResult<Vec<u8>> {
    match codec_id {
        IDENTITY_CODEC_ID => Ok(bytes),
        _ => Err(WalServiceError::InvalidConfig { field: "codec_id" }),
    }
}

fn decode_wal_codec_bytes<'a>(codec_id: &str, bytes: &'a [u8]) -> WalServiceResult<Cow<'a, [u8]>> {
    match codec_id {
        IDENTITY_CODEC_ID => Ok(Cow::Borrowed(bytes)),
        _ => Err(WalServiceError::InvalidConfig { field: "codec_id" }),
    }
}

fn list_segments(backend: &dyn Backend) -> WalServiceResult<Vec<WalSegmentObject>> {
    let prefix = ObjectLayout::wal_prefix().map_err(|source| WalServiceError::Layout { source })?;
    let mut segments = backend
        .list_prefix(&prefix)
        .map_err(|source| WalServiceError::List { source })?
        .into_iter()
        .map(parse_segment_object)
        .collect::<WalServiceResult<Vec<_>>>()?;
    // Valid segment names are fixed-width hex, but sorting by parsed id keeps
    // ordering correct even if a backend returns objects in arbitrary order.
    segments.sort_by_key(|segment| segment.segment_id);
    Ok(segments)
}

fn parse_segment_object(object: ObjectName) -> WalServiceResult<WalSegmentObject> {
    let raw = object.as_str();
    let mut parts = raw.split('/');
    let family = parts.next();
    let component = parts.next();
    if parts.next().is_some() || family != Some(ObjectFamily::Wal.as_str()) {
        return Err(WalServiceError::Backend {
            operation: WalOperation::List,
            object,
            source: BackendError::new(BackendErrorKind::InvalidObjectName, "not a WAL object"),
        });
    }
    let Some(component) = component else {
        return Err(WalServiceError::Backend {
            operation: WalOperation::List,
            object,
            source: BackendError::new(
                BackendErrorKind::InvalidObjectName,
                "WAL segment object is missing segment id",
            ),
        });
    };
    if component.len() != WAL_SEGMENT_COMPONENT_LEN
        || !component
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(WalServiceError::Backend {
            operation: WalOperation::List,
            object,
            source: BackendError::new(
                BackendErrorKind::InvalidObjectName,
                "WAL segment object has invalid component",
            ),
        });
    }
    // The alphabet and fixed-width checks above should make parsing
    // infallible. Keep the fallible branch explicit so later layout changes
    // fail closed instead of introducing a production panic.
    let segment_id = u64::from_str_radix(component, 16).map_err(|_| WalServiceError::Backend {
        operation: WalOperation::List,
        object: object.clone(),
        source: BackendError::new(
            BackendErrorKind::InvalidObjectName,
            "WAL segment object id is not fixed-width hex",
        ),
    })?;
    if segment_id == 0 {
        return Err(WalServiceError::Backend {
            operation: WalOperation::List,
            object,
            source: BackendError::new(
                BackendErrorKind::InvalidObjectName,
                "WAL segment object id must be nonzero",
            ),
        });
    }
    Ok(WalSegmentObject { segment_id, object })
}

fn read_segment(
    backend: &dyn Backend,
    database_id: [u8; 16],
    segment_id: u64,
    object: &ObjectName,
    is_latest: bool,
    codec_id: &str,
) -> WalServiceResult<WalRead> {
    let bytes = backend
        .read_object(object)
        .map_err(|source| WalServiceError::Backend {
            operation: WalOperation::Read,
            object: object.clone(),
            source,
        })?;
    decode_segment_bytes(
        database_id,
        segment_id,
        object,
        &bytes,
        is_latest,
        codec_id,
        WalOperation::Read,
    )
}

fn decode_segment_bytes(
    database_id: [u8; 16],
    segment_id: u64,
    object: &ObjectName,
    bytes: &[u8],
    is_latest: bool,
    codec_id: &str,
    operation: WalOperation,
) -> WalServiceResult<WalRead> {
    let (header, mut offset) =
        decode_wal_segment_header(bytes, Some(segment_id)).map_err(|source| {
            WalServiceError::Format {
                operation,
                object: object.clone(),
                source,
            }
        })?;
    if header.database_id() != &database_id {
        return Err(WalServiceError::DatabaseMismatch {
            object: object.clone(),
            segment_id,
        });
    }

    let mut records = Vec::new();
    while offset < bytes.len() {
        let (envelope, envelope_len) = match decode_wal_record_envelope(&bytes[offset..]) {
            Ok(decoded) => decoded,
            Err(FormatError::InsufficientBytes { .. }) if is_latest => {
                // A short final envelope on the latest segment is a repairable
                // tail fact. The same byte shape in an older segment is hard
                // corruption because later durable records depend on it.
                return Ok(WalRead::new(
                    records,
                    Some(WalTruncation::new(
                        segment_id,
                        offset as u64,
                        bytes.len() as u64,
                    )),
                ));
            }
            Err(source) => {
                return Err(WalServiceError::Format {
                    operation,
                    object: object.clone(),
                    source,
                });
            }
        };
        let decoded_record = decode_wal_codec_bytes(codec_id, envelope.encoded_record())?;
        let (record, consumed) = decode_wal_record(decoded_record.as_ref()).map_err(|source| {
            WalServiceError::Format {
                operation,
                object: object.clone(),
                source,
            }
        })?;
        if consumed != decoded_record.len() {
            return Err(WalServiceError::Format {
                operation,
                object: object.clone(),
                source: FormatError::TrailingData {
                    format: "wal_record",
                    remaining: decoded_record.len() - consumed,
                },
            });
        }
        records.push(record);
        offset += envelope_len;
    }

    Ok(WalRead::new(records, None))
}

fn segment_metadata_from_records(segment_id: u64, records: &[WalRecord]) -> SegmentMetadata {
    let mut metadata = SegmentMetadata::empty(segment_id);
    for record in records {
        metadata.track_record(record.commit_version(), record.commit_timestamp());
    }
    metadata
}

#[cfg(test)]
mod tests;
