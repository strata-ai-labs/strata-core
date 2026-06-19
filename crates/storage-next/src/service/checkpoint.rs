//! Mechanical checkpoint sequencing over manifests and snapshots.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "checkpoint sequencing is consumed by lifecycle work added later"
    )
)]

use crate::backend::{BackendHandle, PublishFailureKind, PublishOutcome};
use crate::format::{DatabaseManifest, SnapshotSection};
use crate::layout::{LayoutError, ObjectLayout};
use crate::object::ObjectName;
use crate::service::{
    DatabaseManifestService, ManifestRole, ManifestServiceError, SnapshotPublishRequest,
    SnapshotService, SnapshotServiceError,
};
use std::fmt;
use strata_core_next::{CommitVersion, Timestamp};

pub(crate) type CheckpointServiceResult<T> = Result<T, CheckpointServiceError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CheckpointManifestOperation {
    LoadCurrent,
    PersistActiveWalSegment,
}

impl CheckpointManifestOperation {
    const fn name(self) -> &'static str {
        match self {
            Self::LoadCurrent => "load current manifest",
            Self::PersistActiveWalSegment => "persist active WAL segment",
        }
    }
}

impl fmt::Display for CheckpointManifestOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CheckpointServiceError {
    Manifest {
        operation: CheckpointManifestOperation,
        source: Box<ManifestServiceError>,
    },
    Snapshot {
        source: Box<SnapshotServiceError>,
    },
    DatabaseMismatch {
        expected: [u8; 16],
        actual: [u8; 16],
    },
    InvalidCheckpointFact {
        field: &'static str,
        value: u64,
    },
    OrphanSnapshot {
        snapshot: Box<CheckpointSnapshot>,
        source: Box<ManifestServiceError>,
    },
    FinalManifestUncertain {
        snapshot: Box<CheckpointSnapshot>,
        source: Box<ManifestServiceError>,
    },
}

impl fmt::Display for CheckpointServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manifest { operation, source } => {
                write!(formatter, "checkpoint failed to {operation}: {source}")
            }
            Self::Snapshot { source } => {
                write!(formatter, "checkpoint failed to publish snapshot: {source}")
            }
            Self::DatabaseMismatch { expected, actual } => write!(
                formatter,
                "checkpoint database mismatch: expected {expected:?}, found {actual:?}"
            ),
            Self::InvalidCheckpointFact { field, value } => {
                write!(
                    formatter,
                    "checkpoint fact {field} has invalid value {value}"
                )
            }
            Self::OrphanSnapshot { snapshot, source } => write!(
                formatter,
                "checkpoint published orphan snapshot {} before manifest update failed: {source}",
                snapshot.snapshot_id()
            ),
            Self::FinalManifestUncertain { snapshot, source } => write!(
                formatter,
                "checkpoint published snapshot {} before final manifest visibility became uncertain: {source}",
                snapshot.snapshot_id()
            ),
        }
    }
}

impl std::error::Error for CheckpointServiceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Manifest { source, .. }
            | Self::OrphanSnapshot { source, .. }
            | Self::FinalManifestUncertain { source, .. } => Some(source.as_ref()),
            Self::Snapshot { source } => Some(source.as_ref()),
            Self::DatabaseMismatch { .. } | Self::InvalidCheckpointFact { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckpointRequest {
    database_id: [u8; 16],
    codec_id: String,
    active_wal_segment: u64,
    snapshot_id: u64,
    snapshot_watermark: CommitVersion,
    created_at: Timestamp,
    sections: Vec<SnapshotSection>,
    flushed_through_base: Option<CommitVersion>,
}

impl CheckpointRequest {
    pub(crate) fn new(
        database_id: [u8; 16],
        codec_id: impl Into<String>,
        active_wal_segment: u64,
        snapshot_id: u64,
        snapshot_watermark: CommitVersion,
        created_at: Timestamp,
        sections: Vec<SnapshotSection>,
    ) -> Self {
        Self {
            database_id,
            codec_id: codec_id.into(),
            active_wal_segment,
            snapshot_id,
            snapshot_watermark,
            created_at,
            sections,
            flushed_through_base: None,
        }
    }

    /// The base floor (highest durably-flushed commit) the checkpoint snapshot is a delta
    /// over, when the snapshot carries only rows above a durable table-manifest base; `None`
    /// for a self-contained full snapshot. Recorded with the snapshot facts so recovery can
    /// require the base.
    #[must_use]
    pub(crate) const fn with_flushed_through_base(
        mut self,
        flushed_through_base: Option<CommitVersion>,
    ) -> Self {
        self.flushed_through_base = flushed_through_base;
        self
    }

    pub(crate) const fn flushed_through_base(&self) -> Option<CommitVersion> {
        self.flushed_through_base
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckpointSnapshot {
    snapshot_id: u64,
    snapshot_watermark: CommitVersion,
    created_at: Timestamp,
    object: ObjectName,
    section_count: usize,
    byte_count: u64,
    publish_outcome: PublishOutcome,
}

impl CheckpointSnapshot {
    fn new(
        snapshot_id: u64,
        snapshot_watermark: CommitVersion,
        created_at: Timestamp,
        object: ObjectName,
        section_count: usize,
        byte_count: u64,
        publish_outcome: PublishOutcome,
    ) -> Self {
        Self {
            snapshot_id,
            snapshot_watermark,
            created_at,
            object,
            section_count,
            byte_count,
            publish_outcome,
        }
    }

    pub(crate) const fn snapshot_id(&self) -> u64 {
        self.snapshot_id
    }

    pub(crate) const fn snapshot_watermark(&self) -> CommitVersion {
        self.snapshot_watermark
    }

    pub(crate) const fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }

    pub(crate) const fn section_count(&self) -> usize {
        self.section_count
    }

    pub(crate) const fn byte_count(&self) -> u64 {
        self.byte_count
    }

    pub(crate) const fn publish_outcome(&self) -> &PublishOutcome {
        &self.publish_outcome
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckpointWrite {
    active_wal_segment: u64,
    snapshot: CheckpointSnapshot,
}

impl CheckpointWrite {
    fn new(active_wal_segment: u64, snapshot: CheckpointSnapshot) -> Self {
        Self {
            active_wal_segment,
            snapshot,
        }
    }

    pub(crate) const fn active_wal_segment(&self) -> u64 {
        self.active_wal_segment
    }

    pub(crate) const fn snapshot(&self) -> &CheckpointSnapshot {
        &self.snapshot
    }
}

#[derive(Clone)]
pub(crate) struct CheckpointService<'a> {
    manifest: DatabaseManifestService<'a>,
    snapshot: SnapshotService<'a>,
}

impl<'a> CheckpointService<'a> {
    pub(crate) fn new(backend: impl Into<BackendHandle<'a>>) -> Self {
        let backend = backend.into();
        Self {
            manifest: DatabaseManifestService::new(backend.clone()),
            snapshot: SnapshotService::new(backend),
        }
    }

    pub(crate) fn checkpoint(
        &self,
        request: CheckpointRequest,
    ) -> CheckpointServiceResult<CheckpointWrite> {
        validate_request(&request)?;
        let manifest = self.validate_manifest_identity(&request)?;
        // Capture the delta base floor before the snapshot publish consumes `request`'s
        // owned fields; it is recorded with the snapshot facts below.
        let flushed_through_base = request.flushed_through_base();

        // Recovery must know the WAL segment that remains active before any
        // snapshot can shorten replay. A failure here leaves the published
        // recovery facts at the previous replay window with no snapshot bytes.
        self.manifest
            .persist_active_wal_segment_from_current(manifest, request.active_wal_segment)
            .map_err(|source| CheckpointServiceError::Manifest {
                operation: CheckpointManifestOperation::PersistActiveWalSegment,
                source: Box::new(source),
            })?;

        // Published recovery facts still lack the new snapshot at this point.
        // Any snapshot publish failure therefore leaves recovery on WAL replay
        // instead of pointing at an incomplete or unconfirmed snapshot.
        let snapshot_write = self
            .snapshot
            .publish_create(SnapshotPublishRequest::new(
                request.snapshot_id,
                request.snapshot_watermark,
                request.created_at,
                request.database_id,
                request.codec_id,
                request.sections,
            ))
            .map_err(|source| CheckpointServiceError::Snapshot {
                source: Box::new(source),
            })?;
        let published_snapshot = checkpoint_snapshot_from_write(&snapshot_write);

        // This is the only step that makes the snapshot live for recovery. A
        // no-visible failure leaves an orphan snapshot; an uncertain publish
        // requires callers to reload recovery facts before classifying state.
        self.manifest
            .persist_snapshot_facts_with_flush_boundary(
                published_snapshot.snapshot_id(),
                published_snapshot.snapshot_watermark(),
                flushed_through_base,
            )
            .map_err(|source| final_manifest_failure(published_snapshot.clone(), source))?;

        Ok(CheckpointWrite::new(
            request.active_wal_segment,
            published_snapshot,
        ))
    }

    fn validate_manifest_identity(
        &self,
        request: &CheckpointRequest,
    ) -> CheckpointServiceResult<DatabaseManifest> {
        let manifest =
            self.manifest
                .load_required()
                .map_err(|source| CheckpointServiceError::Manifest {
                    operation: CheckpointManifestOperation::LoadCurrent,
                    source: Box::new(source),
                })?;
        if manifest.codec_id() != request.codec_id {
            let object = database_manifest_object()?;
            return Err(CheckpointServiceError::Manifest {
                operation: CheckpointManifestOperation::LoadCurrent,
                source: Box::new(ManifestServiceError::CodecMismatch {
                    object,
                    expected: request.codec_id.clone(),
                    actual: manifest.codec_id().to_owned(),
                }),
            });
        }
        if manifest.database_id() != &request.database_id {
            return Err(CheckpointServiceError::DatabaseMismatch {
                expected: request.database_id,
                actual: *manifest.database_id(),
            });
        }
        Ok(manifest)
    }
}

fn validate_request(request: &CheckpointRequest) -> CheckpointServiceResult<()> {
    if request.active_wal_segment == 0 {
        return Err(CheckpointServiceError::InvalidCheckpointFact {
            field: "active_wal_segment",
            value: request.active_wal_segment,
        });
    }
    if request.snapshot_id == 0 {
        return Err(CheckpointServiceError::InvalidCheckpointFact {
            field: "snapshot_id",
            value: request.snapshot_id,
        });
    }
    if request.snapshot_watermark.as_u64() == 0 {
        return Err(CheckpointServiceError::InvalidCheckpointFact {
            field: "snapshot_watermark",
            value: request.snapshot_watermark.as_u64(),
        });
    }
    Ok(())
}

fn checkpoint_snapshot_from_write(write: &super::snapshot::SnapshotWrite) -> CheckpointSnapshot {
    let (container, outcome) = write;
    let header = container.header();
    CheckpointSnapshot::new(
        header.snapshot_id(),
        header.watermark_commit_version(),
        header.created_at(),
        outcome.object().clone(),
        container.sections().len(),
        outcome.metadata().size_bytes(),
        outcome.clone(),
    )
}

fn final_manifest_failure(
    snapshot: CheckpointSnapshot,
    source: ManifestServiceError,
) -> CheckpointServiceError {
    if final_manifest_publish_is_uncertain(&source) {
        CheckpointServiceError::FinalManifestUncertain {
            snapshot: Box::new(snapshot),
            source: Box::new(source),
        }
    } else {
        CheckpointServiceError::OrphanSnapshot {
            snapshot: Box::new(snapshot),
            source: Box::new(source),
        }
    }
}

fn final_manifest_publish_is_uncertain(source: &ManifestServiceError) -> bool {
    matches!(
        source,
        ManifestServiceError::Publish {
            source,
            ..
        } if matches!(
            source.kind(),
            PublishFailureKind::VisibilityUnknown | PublishFailureKind::VisibleDurabilityUnconfirmed
        )
    )
}

fn database_manifest_object() -> CheckpointServiceResult<ObjectName> {
    ObjectLayout::database_manifest().map_err(|source: LayoutError| {
        CheckpointServiceError::Manifest {
            operation: CheckpointManifestOperation::LoadCurrent,
            source: Box::new(ManifestServiceError::Layout {
                role: ManifestRole::Database,
                source,
            }),
        }
    })
}

#[cfg(test)]
mod tests;
