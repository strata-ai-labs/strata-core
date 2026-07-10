use super::super::{QuarantineInventoryToken, QuarantineInventoryWrite};
use crate::backend::{
    BackendError, DeleteError, DeleteOutcome, DeleteStatus, PublishError, PublishOutcome,
};
use crate::format::quarantine::QuarantineEntry;
use crate::object::ObjectName;
use std::fmt;
use strata_core::{BranchId, Timestamp};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuarantineGate {
    Safe,
    Referenced,
    UnsafeRecovery,
    ProofIncomplete,
}

impl QuarantineGate {
    const fn name(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Referenced => "referenced",
            Self::UnsafeRecovery => "unsafe recovery",
            Self::ProofIncomplete => "proof incomplete",
        }
    }
}

impl fmt::Display for QuarantineGate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuarantineObjectRequest {
    pub(super) branch_id: BranchId,
    pub(super) database_id: [u8; 16],
    pub(super) codec_id: String,
    pub(super) object_id: String,
    pub(super) source_object: ObjectName,
    pub(super) quarantined_at: Timestamp,
    pub(super) gate: QuarantineGate,
    pub(super) allow_epoch_timestamp: bool,
}

impl QuarantineObjectRequest {
    pub(crate) fn new(
        branch_id: BranchId,
        database_id: [u8; 16],
        codec_id: impl Into<String>,
        object_id: impl Into<String>,
        source_object: ObjectName,
        quarantined_at: Timestamp,
        gate: QuarantineGate,
    ) -> Self {
        Self {
            branch_id,
            database_id,
            codec_id: codec_id.into(),
            object_id: object_id.into(),
            source_object,
            quarantined_at,
            gate,
            allow_epoch_timestamp: false,
        }
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) fn object_id(&self) -> &str {
        &self.object_id
    }

    pub(crate) const fn source_object(&self) -> &ObjectName {
        &self.source_object
    }

    pub(crate) fn allow_epoch_timestamp(mut self) -> Self {
        self.allow_epoch_timestamp = true;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuarantineObjectStatus {
    QuarantinedSourceDeleted,
    AlreadyQuarantined,
    SourceDeleteRetried,
    SourceAlreadyMissingAfterPublish,
    QuarantinedSourceDeleteFailed,
    InventoryPublishFailed,
    InventoryPublishUncertain,
    QuarantinePublishFailed,
    QuarantinePublishUncertain,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuarantinePublishFailure {
    object: ObjectName,
    source: PublishError,
}

impl QuarantinePublishFailure {
    pub(super) fn new(object: ObjectName, source: PublishError) -> Self {
        Self { object, source }
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }

    pub(crate) const fn source(&self) -> &PublishError {
        &self.source
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuarantineDeleteOutcome {
    object: ObjectName,
    pub(super) deleted: bool,
    pub(super) already_missing: bool,
    pub(super) outcome: Option<DeleteOutcome>,
    pub(super) failure: Option<DeleteError>,
}

impl QuarantineDeleteOutcome {
    pub(super) fn from_outcome(outcome: DeleteOutcome) -> Self {
        let deleted = outcome.status() == DeleteStatus::Deleted;
        let already_missing = outcome.status() == DeleteStatus::AlreadyMissing;
        let object = outcome.object().clone();
        Self {
            object,
            deleted,
            already_missing,
            outcome: Some(outcome),
            failure: None,
        }
    }

    pub(super) fn failed(object: ObjectName, failure: DeleteError) -> Self {
        Self {
            object,
            deleted: false,
            already_missing: false,
            outcome: None,
            failure: Some(failure),
        }
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }

    pub(crate) const fn deleted_flag(&self) -> bool {
        self.deleted
    }

    pub(crate) const fn already_missing(&self) -> bool {
        self.already_missing
    }

    pub(crate) const fn outcome(&self) -> Option<&DeleteOutcome> {
        self.outcome.as_ref()
    }

    pub(crate) const fn failure(&self) -> Option<&BackendError> {
        match self.failure.as_ref() {
            Some(failure) => Some(failure.source_error()),
            None => None,
        }
    }

    pub(crate) const fn delete_error(&self) -> Option<&DeleteError> {
        self.failure.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuarantineObjectReport {
    status: QuarantineObjectStatus,
    branch_id: BranchId,
    object_id: String,
    source_object: ObjectName,
    quarantine_object: ObjectName,
    byte_count: u64,
    entry_count: usize,
    pub(super) inventory_write: Option<QuarantineInventoryWrite>,
    pub(super) inventory_publish_failure: Option<QuarantinePublishFailure>,
    pub(super) quarantine_publish_outcome: Option<PublishOutcome>,
    pub(super) quarantine_publish_failure: Option<QuarantinePublishFailure>,
    pub(super) source_delete: Option<QuarantineDeleteOutcome>,
}

impl QuarantineObjectReport {
    pub(super) fn new(
        status: QuarantineObjectStatus,
        request: &QuarantineObjectRequest,
        quarantine_object: ObjectName,
        byte_count: u64,
        entry_count: usize,
    ) -> Self {
        Self {
            status,
            branch_id: request.branch_id,
            object_id: request.object_id.clone(),
            source_object: request.source_object.clone(),
            quarantine_object,
            byte_count,
            entry_count,
            inventory_write: None,
            inventory_publish_failure: None,
            quarantine_publish_outcome: None,
            quarantine_publish_failure: None,
            source_delete: None,
        }
    }

    pub(crate) const fn status(&self) -> QuarantineObjectStatus {
        self.status
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) fn object_id(&self) -> &str {
        &self.object_id
    }

    pub(crate) const fn source_object(&self) -> &ObjectName {
        &self.source_object
    }

    pub(crate) const fn quarantine_object(&self) -> &ObjectName {
        &self.quarantine_object
    }

    pub(crate) const fn byte_count(&self) -> u64 {
        self.byte_count
    }

    pub(crate) const fn entry_count(&self) -> usize {
        self.entry_count
    }

    pub(crate) const fn inventory_write(&self) -> Option<&QuarantineInventoryWrite> {
        self.inventory_write.as_ref()
    }

    pub(crate) const fn inventory_publish_failure(&self) -> Option<&QuarantinePublishFailure> {
        self.inventory_publish_failure.as_ref()
    }

    pub(crate) const fn quarantine_publish_failure(&self) -> Option<&QuarantinePublishFailure> {
        self.quarantine_publish_failure.as_ref()
    }

    pub(crate) const fn quarantine_publish_outcome(&self) -> Option<&PublishOutcome> {
        self.quarantine_publish_outcome.as_ref()
    }

    pub(crate) const fn source_delete(&self) -> Option<&QuarantineDeleteOutcome> {
        self.source_delete.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuarantinePurgeRequest {
    pub(super) branch_id: BranchId,
    pub(super) database_id: [u8; 16],
    pub(super) codec_id: String,
    pub(super) gate: QuarantineGate,
    pub(super) expected_inventory_token: Option<QuarantineInventoryToken>,
}

impl QuarantinePurgeRequest {
    pub(crate) fn new(
        branch_id: BranchId,
        database_id: [u8; 16],
        codec_id: impl Into<String>,
        gate: QuarantineGate,
        expected_inventory_token: Option<QuarantineInventoryToken>,
    ) -> Self {
        Self {
            branch_id,
            database_id,
            codec_id: codec_id.into(),
            gate,
            expected_inventory_token,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuarantinePurgeReport {
    branch_id: BranchId,
    inventory_object: ObjectName,
    pub(super) deleted: Vec<QuarantineDeleteOutcome>,
    pub(super) already_missing: Vec<QuarantineDeleteOutcome>,
    pub(super) failed: Vec<QuarantineDeleteOutcome>,
    pub(super) retained_entries: Vec<QuarantineEntry>,
    pub(super) reclaimed_bytes: u64,
    pub(super) inventory_write: Option<QuarantineInventoryWrite>,
    pub(super) inventory_publish_failure: Option<QuarantinePublishFailure>,
}

impl QuarantinePurgeReport {
    pub(super) fn new(branch_id: BranchId, inventory_object: ObjectName) -> Self {
        Self {
            branch_id,
            inventory_object,
            deleted: Vec::new(),
            already_missing: Vec::new(),
            failed: Vec::new(),
            retained_entries: Vec::new(),
            reclaimed_bytes: 0,
            inventory_write: None,
            inventory_publish_failure: None,
        }
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn inventory_object(&self) -> &ObjectName {
        &self.inventory_object
    }

    pub(crate) fn deleted(&self) -> &[QuarantineDeleteOutcome] {
        &self.deleted
    }

    pub(crate) fn already_missing(&self) -> &[QuarantineDeleteOutcome] {
        &self.already_missing
    }

    pub(crate) fn failed(&self) -> &[QuarantineDeleteOutcome] {
        &self.failed
    }

    pub(crate) fn retained_entries(&self) -> &[QuarantineEntry] {
        &self.retained_entries
    }

    pub(crate) const fn reclaimed_bytes(&self) -> u64 {
        self.reclaimed_bytes
    }

    pub(crate) const fn inventory_write(&self) -> Option<&QuarantineInventoryWrite> {
        self.inventory_write.as_ref()
    }

    pub(crate) const fn inventory_publish_failure(&self) -> Option<&QuarantinePublishFailure> {
        self.inventory_publish_failure.as_ref()
    }
}
