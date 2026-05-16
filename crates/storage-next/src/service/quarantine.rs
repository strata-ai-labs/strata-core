//! Quarantine inventory service mechanics.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "quarantine service is consumed by lifecycle and recovery work added later"
    )
)]

use crate::backend::{
    Backend, BackendCapability, BackendError, BackendErrorKind, PublishError, PublishOutcome,
};
use crate::format::quarantine::{
    decode_quarantine_inventory, encode_quarantine_inventory, QuarantineInventory,
};
use crate::format::FormatError;
use crate::layout::{LayoutError, ObjectFamily, ObjectLayout};
use crate::object::ObjectName;
use crate::service::{validate_publish_outcome, ObjectPublisher};
use std::fmt;
use strata_core_next::BranchId;

pub(crate) type QuarantineServiceResult<T> = Result<T, QuarantineServiceError>;

mod mutation;
mod reconcile;

pub(crate) use mutation::{
    QuarantineDeleteOutcome, QuarantineGate, QuarantineObjectReport, QuarantineObjectRequest,
    QuarantineObjectStatus, QuarantinePublishFailure, QuarantinePurgeReport,
    QuarantinePurgeRequest,
};
pub(crate) use reconcile::{
    MalformedQuarantineObjectReason, QuarantineBackendOperation, QuarantineBackendUnavailable,
    QuarantineCorruptInventory, QuarantineFamilyReconciliation, QuarantineInventoryCorruption,
    QuarantineListedObject, QuarantineMalformedObject, QuarantineMissingObject,
    QuarantineReconciliationKind, QuarantineReconciliationReport, QuarantineRecoveryClass,
    QuarantineUnlistedObject,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum QuarantineServiceError {
    UnsupportedCapability {
        capability: BackendCapability,
    },
    Layout {
        source: LayoutError,
    },
    Missing {
        object: ObjectName,
    },
    Read {
        object: ObjectName,
        source: BackendError,
    },
    Decode {
        object: ObjectName,
        source: FormatError,
    },
    Encode {
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
    DatabaseMismatch {
        object: ObjectName,
        expected: [u8; 16],
        actual: [u8; 16],
    },
    BranchMismatch {
        object: ObjectName,
        expected: BranchId,
        actual: BranchId,
    },
    CodecMismatch {
        object: ObjectName,
        expected: String,
        actual: String,
    },
    InvalidRequest {
        field: &'static str,
    },
    UnsafeGate {
        gate: QuarantineGate,
    },
    Metadata {
        object: ObjectName,
        source: BackendError,
    },
    BackendState {
        object: ObjectName,
        expected_size: u64,
        actual_size: u64,
    },
    InventoryMismatch {
        object_id: String,
        quarantine_object: ObjectName,
        source_object: ObjectName,
        reason: &'static str,
    },
}

impl fmt::Display for QuarantineServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedCapability { capability } => {
                write!(formatter, "backend does not support {capability}")
            }
            Self::Layout { source } => {
                write!(
                    formatter,
                    "failed to build quarantine object name: {source}"
                )
            }
            Self::Missing { object } => {
                write!(formatter, "required object {object} is missing")
            }
            Self::Read { object, source } => {
                write!(formatter, "failed to read object {object}: {source}")
            }
            Self::Decode { object, source } => {
                write!(
                    formatter,
                    "failed to decode quarantine inventory {object}: {source}"
                )
            }
            Self::Encode { object, source } => {
                write!(
                    formatter,
                    "failed to encode quarantine inventory {object}: {source}"
                )
            }
            Self::Publish { object, source } => {
                write!(
                    formatter,
                    "failed to publish quarantine inventory {object}: {source}"
                )
            }
            Self::InvalidPublishMetadata { object, field } => write!(
                formatter,
                "quarantine inventory {object} has invalid publish metadata {field}"
            ),
            Self::DatabaseMismatch {
                object,
                expected,
                actual,
            } => write!(
                formatter,
                "quarantine inventory {object} database mismatch: expected {expected:?}, found {actual:?}"
            ),
            Self::BranchMismatch {
                object,
                expected,
                actual,
            } => write!(
                formatter,
                "quarantine inventory {object} branch mismatch: expected {expected}, found {actual}"
            ),
            Self::CodecMismatch {
                object,
                expected,
                actual,
            } => write!(
                formatter,
                "quarantine inventory {object} codec mismatch: expected {expected}, found {actual}"
            ),
            Self::InvalidRequest { field } => {
                write!(formatter, "quarantine request field {field} is invalid")
            }
            Self::UnsafeGate { gate } => {
                write!(formatter, "quarantine request is blocked by {gate}")
            }
            Self::Metadata { object, source } => {
                write!(
                    formatter,
                    "failed to read metadata for object {object}: {source}"
                )
            }
            Self::BackendState {
                object,
                expected_size,
                actual_size,
            } => write!(
                formatter,
                "object {object} metadata size mismatch: expected {expected_size}, found {actual_size}"
            ),
            Self::InventoryMismatch {
                object_id,
                quarantine_object,
                source_object,
                reason,
            } => write!(
                formatter,
                "quarantine inventory entry {object_id} for {source_object} disagrees with {quarantine_object}: {reason}"
            ),
        }
    }
}

impl std::error::Error for QuarantineServiceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Layout { source } => Some(source),
            Self::Read { source, .. } | Self::Metadata { source, .. } => Some(source),
            Self::Decode { source, .. } | Self::Encode { source, .. } => Some(source),
            Self::Publish { source, .. } => Some(source),
            Self::UnsupportedCapability { .. }
            | Self::Missing { .. }
            | Self::InvalidPublishMetadata { .. }
            | Self::DatabaseMismatch { .. }
            | Self::BranchMismatch { .. }
            | Self::CodecMismatch { .. }
            | Self::InvalidRequest { .. }
            | Self::UnsafeGate { .. }
            | Self::BackendState { .. }
            | Self::InventoryMismatch { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuarantineInventoryLoad {
    object: ObjectName,
    inventory: QuarantineInventory,
    byte_count: u64,
    present: bool,
}

impl QuarantineInventoryLoad {
    fn new(
        object: ObjectName,
        inventory: QuarantineInventory,
        byte_count: u64,
        present: bool,
    ) -> Self {
        Self {
            object,
            inventory,
            byte_count,
            present,
        }
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }

    pub(crate) const fn inventory(&self) -> &QuarantineInventory {
        &self.inventory
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.inventory.branch_id()
    }

    pub(crate) fn entry_count(&self) -> usize {
        self.inventory.entries().len()
    }

    pub(crate) const fn byte_count(&self) -> u64 {
        self.byte_count
    }

    pub(crate) const fn is_present(&self) -> bool {
        self.present
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuarantineInventoryWrite {
    object: ObjectName,
    inventory: QuarantineInventory,
    byte_count: u64,
    outcome: PublishOutcome,
}

impl QuarantineInventoryWrite {
    fn new(
        object: ObjectName,
        inventory: QuarantineInventory,
        byte_count: u64,
        outcome: PublishOutcome,
    ) -> Self {
        Self {
            object,
            inventory,
            byte_count,
            outcome,
        }
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }

    pub(crate) const fn inventory(&self) -> &QuarantineInventory {
        &self.inventory
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.inventory.branch_id()
    }

    pub(crate) fn entry_count(&self) -> usize {
        self.inventory.entries().len()
    }

    pub(crate) const fn byte_count(&self) -> u64 {
        self.byte_count
    }

    pub(crate) const fn outcome(&self) -> &PublishOutcome {
        &self.outcome
    }
}

pub(crate) struct QuarantineService<'a> {
    backend: &'a dyn Backend,
    publisher: ObjectPublisher<'a>,
}

impl<'a> QuarantineService<'a> {
    pub(crate) const fn new(backend: &'a dyn Backend) -> Self {
        Self {
            backend,
            publisher: ObjectPublisher::new(backend),
        }
    }

    pub(crate) fn load_inventory(
        &self,
        branch_id: BranchId,
        expected_database_id: [u8; 16],
        expected_codec_id: &str,
    ) -> QuarantineServiceResult<QuarantineInventoryLoad> {
        let object = inventory_object(branch_id)?;
        if let Some(load) =
            self.load_optional_inventory(branch_id, expected_database_id, expected_codec_id)?
        {
            return Ok(load);
        }

        // Missing inventory is the healthy empty state only for the inventory
        // service. Later reconciliation must also inspect quarantine objects
        // before treating a branch as clean.
        let inventory =
            empty_inventory(&object, branch_id, expected_database_id, expected_codec_id)?;
        Ok(QuarantineInventoryLoad::new(object, inventory, 0, false))
    }

    pub(crate) fn load_required_inventory(
        &self,
        branch_id: BranchId,
        expected_database_id: [u8; 16],
        expected_codec_id: &str,
    ) -> QuarantineServiceResult<QuarantineInventoryLoad> {
        let object = inventory_object(branch_id)?;
        self.load_optional_inventory(branch_id, expected_database_id, expected_codec_id)?
            .ok_or(QuarantineServiceError::Missing { object })
    }

    pub(crate) fn load_optional_inventory(
        &self,
        branch_id: BranchId,
        expected_database_id: [u8; 16],
        expected_codec_id: &str,
    ) -> QuarantineServiceResult<Option<QuarantineInventoryLoad>> {
        let object = inventory_object(branch_id)?;
        let Some(bytes) = read_inventory_optional(self.backend, &object)? else {
            return Ok(None);
        };
        let byte_count = bytes.len() as u64;
        let inventory = decode_inventory(&object, &bytes)?;
        let inventory = validate_inventory_identity(
            &object,
            branch_id,
            expected_database_id,
            expected_codec_id,
            inventory,
        )?;
        Ok(Some(QuarantineInventoryLoad::new(
            object, inventory, byte_count, true,
        )))
    }

    pub(crate) fn publish_inventory_replace(
        &self,
        inventory: &QuarantineInventory,
    ) -> QuarantineServiceResult<QuarantineInventoryWrite> {
        let object = inventory_object(inventory.branch_id())?;
        validate_inventory_layout(inventory).map_err(|source| QuarantineServiceError::Encode {
            object: object.clone(),
            source,
        })?;
        let bytes = encode_quarantine_inventory(inventory).map_err(|source| {
            QuarantineServiceError::Encode {
                object: object.clone(),
                source,
            }
        })?;
        let decoded = decode_inventory(&object, &bytes)?;
        validate_inventory_layout(&decoded).map_err(|source| QuarantineServiceError::Decode {
            object: object.clone(),
            source,
        })?;
        let outcome = self
            .publisher
            .publish_durable_replace(&object, &bytes)
            .map_err(|source| QuarantineServiceError::Publish {
                object: object.clone(),
                source,
            })?;
        validate_publish_outcome(&object, bytes.len() as u64, &outcome).map_err(|mismatch| {
            QuarantineServiceError::InvalidPublishMetadata {
                object: mismatch.object().clone(),
                field: mismatch.field(),
            }
        })?;
        Ok(QuarantineInventoryWrite::new(
            object,
            decoded,
            bytes.len() as u64,
            outcome,
        ))
    }
}

fn inventory_object(branch_id: BranchId) -> QuarantineServiceResult<ObjectName> {
    ObjectLayout::quarantine_manifest(&branch_id.to_string())
        .map_err(|source| QuarantineServiceError::Layout { source })
}

fn empty_inventory(
    object: &ObjectName,
    branch_id: BranchId,
    database_id: [u8; 16],
    codec_id: &str,
) -> QuarantineServiceResult<QuarantineInventory> {
    QuarantineInventory::new(database_id, branch_id, codec_id, Vec::new()).map_err(|source| {
        QuarantineServiceError::Encode {
            object: object.clone(),
            source,
        }
    })
}

fn read_inventory_optional(
    backend: &dyn Backend,
    object: &ObjectName,
) -> QuarantineServiceResult<Option<Vec<u8>>> {
    require_capability(backend, BackendCapability::ReadObject)?;
    match backend.read_object(object) {
        Ok(bytes) => Ok(Some(bytes)),
        // Absence is different from corruption. Optional load can synthesize an
        // empty inventory, but malformed bytes must remain visible to recovery.
        Err(source) if source.kind() == BackendErrorKind::NotFound => Ok(None),
        Err(source) => Err(QuarantineServiceError::Read {
            object: object.clone(),
            source,
        }),
    }
}

fn require_capability(
    backend: &dyn Backend,
    capability: BackendCapability,
) -> QuarantineServiceResult<()> {
    if backend.capabilities().contains(capability) {
        return Ok(());
    }
    Err(QuarantineServiceError::UnsupportedCapability { capability })
}

fn decode_inventory(
    object: &ObjectName,
    bytes: &[u8],
) -> QuarantineServiceResult<QuarantineInventory> {
    decode_quarantine_inventory(bytes).map_err(|source| QuarantineServiceError::Decode {
        object: object.clone(),
        source,
    })
}

fn validate_inventory_identity(
    object: &ObjectName,
    expected_branch_id: BranchId,
    expected_database_id: [u8; 16],
    expected_codec_id: &str,
    inventory: QuarantineInventory,
) -> QuarantineServiceResult<QuarantineInventory> {
    if inventory.database_id() != &expected_database_id {
        return Err(QuarantineServiceError::DatabaseMismatch {
            object: object.clone(),
            expected: expected_database_id,
            actual: *inventory.database_id(),
        });
    }
    if inventory.branch_id() != expected_branch_id {
        return Err(QuarantineServiceError::BranchMismatch {
            object: object.clone(),
            expected: expected_branch_id,
            actual: inventory.branch_id(),
        });
    }
    if inventory.codec_id() != expected_codec_id {
        return Err(QuarantineServiceError::CodecMismatch {
            object: object.clone(),
            expected: expected_codec_id.to_owned(),
            actual: inventory.codec_id().to_owned(),
        });
    }
    validate_inventory_layout(&inventory).map_err(|source| QuarantineServiceError::Decode {
        object: object.clone(),
        source,
    })?;
    Ok(inventory)
}

fn validate_inventory_layout(inventory: &QuarantineInventory) -> Result<(), FormatError> {
    for entry in inventory.entries() {
        validate_inventory_object_id(inventory.branch_id(), entry.object_id())?;
        validate_inventory_source_object(entry.source_object())?;
    }
    Ok(())
}

fn validate_inventory_object_id(branch_id: BranchId, object_id: &str) -> Result<(), FormatError> {
    if object_id == quarantine_inventory_object_id(branch_id)? {
        return Err(FormatError::InvalidValue { field: "object_id" });
    }
    ObjectLayout::quarantine_object(&branch_id.to_string(), object_id)
        .map(|_| ())
        .map_err(|_| FormatError::InvalidValue { field: "object_id" })
}

fn quarantine_inventory_object_id(branch_id: BranchId) -> Result<String, FormatError> {
    let object = ObjectLayout::quarantine_manifest(&branch_id.to_string())
        .map_err(|_| FormatError::InvalidValue { field: "branch_id" })?;
    object
        .as_str()
        .rsplit('/')
        .next()
        .map(str::to_owned)
        .ok_or(FormatError::InvalidValue { field: "object_id" })
}

fn validate_inventory_source_object(source_object: &ObjectName) -> Result<(), FormatError> {
    match ObjectFamily::from_object_name(source_object) {
        Some(ObjectFamily::Quarantine) | None => Err(FormatError::InvalidValue {
            field: "source_object",
        }),
        Some(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests;
