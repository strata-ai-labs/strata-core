//! Lower storage services for manifests, WAL, snapshots, tables, quarantine, and publication.

mod checkpoint;
mod manifest;
mod publish;
mod quarantine;
mod sidecar;
mod snapshot;
mod table;
mod wal;

#[cfg(test)]
mod cache_mode_absence_tests;

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

pub(crate) use publish::{validate_publish_outcome, ObjectPublisher};

#[expect(
    unused_imports,
    reason = "quarantine service is consumed by lifecycle and recovery services added later"
)]
pub(crate) use quarantine::{
    MalformedQuarantineObjectReason, QuarantineBackendOperation, QuarantineBackendUnavailable,
    QuarantineCorruptInventory, QuarantineDeleteOutcome, QuarantineFamilyReconciliation,
    QuarantineGate, QuarantineInventoryCorruption, QuarantineInventoryLoad,
    QuarantineInventoryWrite, QuarantineListedObject, QuarantineMalformedObject,
    QuarantineMissingObject, QuarantineObjectReport, QuarantineObjectRequest,
    QuarantineObjectStatus, QuarantinePublishFailure, QuarantinePurgeReport,
    QuarantinePurgeRequest, QuarantineReconciliationKind, QuarantineReconciliationReport,
    QuarantineRecoveryClass, QuarantineService, QuarantineServiceError, QuarantineUnlistedObject,
};

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
    reason = "table object service is consumed by branch table integration added later"
)]
pub(crate) use table::{
    TableObjectByteSource, TableObjectFacts, TableObjectReadError, TableObjectReaderService,
    TableObjectService, TableObjectServiceError, TableObjectWrite,
};

#[expect(
    unused_imports,
    reason = "WAL service is consumed by commit and lifecycle services added later"
)]
pub(crate) use wal::{
    WalAppend, WalDeleteReport, WalOperation, WalRead, WalRepair, WalRetentionProof,
    WalRetentionProofSource, WalService, WalServiceConfig, WalServiceError, WalTruncation,
};
