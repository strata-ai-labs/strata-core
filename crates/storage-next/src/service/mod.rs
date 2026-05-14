//! Lower storage services for manifests, WAL, snapshots, and publication.

mod checkpoint;
mod manifest;
mod publish;
mod sidecar;
mod snapshot;
mod wal;

#[expect(
    unused_imports,
    reason = "checkpoint sequencing is consumed by lifecycle work added later"
)]
pub(crate) use checkpoint::{
    CheckpointManifestOperation, CheckpointRequest, CheckpointService, CheckpointServiceError,
    CheckpointSnapshot, CheckpointWrite,
};

#[expect(
    unused_imports,
    reason = "manifest services are consumed by lifecycle and table services added later"
)]
pub(crate) use manifest::{
    DatabaseManifestService, DatabaseManifestWrite, ManifestRole, ManifestServiceError,
    TableManifestService,
};

pub(crate) use publish::ObjectPublisher;

#[expect(
    unused_imports,
    reason = "sidecar services are consumed by lifecycle and recovery services added later"
)]
pub(crate) use sidecar::{
    WalSegmentMetadataSidecar, WalSegmentMetadataSidecarDelete, WalSegmentMetadataSidecarError,
    WalSegmentMetadataSidecarLoad, WalSegmentMetadataSidecarService,
    WalSegmentMetadataSidecarWrite,
};

#[expect(
    unused_imports,
    reason = "snapshot service is consumed by lifecycle and recovery services added later"
)]
pub(crate) use snapshot::{
    SnapshotDeleteFailure, SnapshotDeleteReport, SnapshotObject, SnapshotPublishRequest,
    SnapshotService, SnapshotServiceError, SnapshotWrite,
};

#[expect(
    unused_imports,
    reason = "WAL service is consumed by commit and lifecycle services added later"
)]
pub(crate) use wal::{
    WalAppend, WalDeleteReport, WalOperation, WalRead, WalService, WalServiceConfig,
    WalServiceError, WalTruncation,
};
