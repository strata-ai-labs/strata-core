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

#[cfg(test)]
pub(crate) use checkpoint::CheckpointManifestOperation;
pub(crate) use checkpoint::{
    CheckpointRequest, CheckpointService, CheckpointServiceError, CheckpointSnapshot,
    CheckpointWrite,
};

pub(crate) use manifest::{
    BranchCatalogManifestService, DatabaseManifestService, ManifestRole, ManifestServiceError,
    PendingReleasesManifestService, TableManifestService, TableManifestWrite,
};

pub(crate) use publish::{validate_publish_outcome, ObjectPublisher};

pub(crate) use quarantine::{
    QuarantineDeleteOutcome, QuarantineFamilyReconciliation, QuarantineGate,
    QuarantineInventoryToken, QuarantineObjectReport, QuarantineObjectRequest,
    QuarantineObjectStatus, QuarantinePurgeReport, QuarantinePurgeRequest,
    QuarantineReconciliationKind, QuarantineReconciliationReport, QuarantineService,
    QuarantineServiceError,
};

#[cfg_attr(
    all(not(test), not(feature = "testkit")),
    expect(
        unused_imports,
        reason = "sidecar services are consumed by lifecycle and recovery services added later"
    )
)]
pub(crate) use sidecar::{WalSegmentMetadataSidecarLoad, WalSegmentMetadataSidecarService};

pub(crate) use snapshot::{
    SnapshotDeleteFailure, SnapshotObject, SnapshotPublishRequest, SnapshotService,
    SnapshotServiceError,
};

pub(crate) use table::{
    TableObjectFacts, TableObjectReadError, TableObjectReaderService, TableObjectService,
    TableObjectServiceError,
};

#[cfg_attr(
    all(not(test), not(feature = "testkit")),
    expect(
        unused_imports,
        reason = "WAL service is consumed by commit and lifecycle services added later"
    )
)]
pub(crate) use wal::{
    WalAppend, WalDeleteReport, WalGrowthFacts, WalRepair, WalRetentionProof,
    WalRetentionProofSource, WalService, WalServiceConfig, WalServiceError, WalTruncation,
};
