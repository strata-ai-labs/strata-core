use super::{
    require_capability, QuarantineService, QuarantineServiceError, QuarantineServiceResult,
};
use crate::backend::{
    Backend, BackendCapability, BackendErrorKind, DeleteStatus, PublishFailureKind,
};
use crate::format::quarantine::{QuarantineEntry, QuarantineInventory};
use crate::layout::{ObjectFamily, ObjectLayout};
use crate::object::ObjectName;
use strata_core_next::{BranchId, Timestamp};

mod types;

pub(crate) use types::{
    QuarantineDeleteOutcome, QuarantineGate, QuarantineObjectReport, QuarantineObjectRequest,
    QuarantineObjectStatus, QuarantinePublishFailure, QuarantinePurgeReport,
    QuarantinePurgeRequest,
};

impl QuarantineService<'_> {
    pub(crate) fn quarantine_object(
        &self,
        request: &QuarantineObjectRequest,
    ) -> QuarantineServiceResult<QuarantineObjectReport> {
        validate_gate(request.gate)?;
        validate_quarantine_request(request)?;
        require_capabilities(
            &self.backend,
            &[
                BackendCapability::ReadObject,
                BackendCapability::ObjectMetadata,
                BackendCapability::DurablePublish,
                BackendCapability::DurableSync,
                BackendCapability::DeleteObject,
            ],
        )?;

        let quarantine_object = quarantine_object_name(request.branch_id, &request.object_id)?;
        let inventory_load =
            self.load_inventory(request.branch_id, request.database_id, &request.codec_id)?;
        let entry = inventory_load
            .inventory()
            .entries()
            .iter()
            .find(|entry| entry.object_id() == request.object_id);

        if let Some(entry) = entry {
            let quarantine_bytes = read_object_optional(&self.backend, &quarantine_object)?;
            let source_bytes = read_object_optional(&self.backend, &request.source_object)?;
            return self.handle_existing_quarantine_entry(
                request,
                &quarantine_object,
                entry,
                quarantine_bytes,
                source_bytes,
                inventory_load.inventory().entries().len(),
            );
        }

        let quarantine_object_exists = object_exists(&self.backend, &quarantine_object)?;
        if quarantine_object_exists {
            return Err(QuarantineServiceError::InventoryMismatch {
                object_id: request.object_id.clone(),
                quarantine_object,
                source_object: request.source_object.clone(),
                reason: "quarantine object is not listed in inventory",
            });
        }
        let source_bytes = read_object_optional(&self.backend, &request.source_object)?;
        self.handle_new_quarantine_entry(
            request,
            quarantine_object,
            inventory_load.inventory(),
            source_bytes,
        )
    }

    fn handle_new_quarantine_entry(
        &self,
        request: &QuarantineObjectRequest,
        quarantine_object: ObjectName,
        current_inventory: &QuarantineInventory,
        source_bytes: Option<Vec<u8>>,
    ) -> QuarantineServiceResult<QuarantineObjectReport> {
        let Some(source_bytes) = source_bytes else {
            return Err(QuarantineServiceError::Missing {
                object: request.source_object.clone(),
            });
        };
        let source_metadata = self
            .backend
            .object_metadata(&request.source_object)
            .map_err(|source| QuarantineServiceError::Metadata {
                object: request.source_object.clone(),
                source,
            })?;
        let byte_count = source_bytes.len() as u64;
        if source_metadata.size_bytes() != byte_count {
            return Err(QuarantineServiceError::BackendState {
                object: request.source_object.clone(),
                expected_size: byte_count,
                actual_size: source_metadata.size_bytes(),
            });
        }

        let updated_inventory =
            updated_inventory_with_entry(request, current_inventory, byte_count)?;

        self.publish_inventory_then_copy_source(
            request,
            quarantine_object,
            &source_bytes,
            byte_count,
            &updated_inventory,
        )
    }

    fn publish_inventory_then_copy_source(
        &self,
        request: &QuarantineObjectRequest,
        quarantine_object: ObjectName,
        source_bytes: &[u8],
        byte_count: u64,
        updated_inventory: &QuarantineInventory,
    ) -> QuarantineServiceResult<QuarantineObjectReport> {
        // The inventory is published before object movement so reconciliation
        // can classify any later copy failure as an inventory/object mismatch
        // instead of losing track of an in-flight quarantine request.
        let inventory_write = match self.publish_inventory_replace(updated_inventory) {
            Ok(write) => write,
            Err(QuarantineServiceError::Publish { object, source }) => {
                let status = if publish_failure_is_uncertain(source.kind()) {
                    QuarantineObjectStatus::InventoryPublishUncertain
                } else {
                    QuarantineObjectStatus::InventoryPublishFailed
                };
                let mut report = QuarantineObjectReport::new(
                    status,
                    request,
                    quarantine_object,
                    byte_count,
                    updated_inventory.entries().len(),
                );
                report.inventory_publish_failure =
                    Some(QuarantinePublishFailure::new(object, source));
                return Ok(report);
            }
            Err(source) => return Err(source),
        };

        // Source bytes are deleted only after the quarantine copy has been
        // durably created. This preserves at least one recoverable copy across
        // every publish and delete fault window.
        match self
            .publisher
            .publish_durable_create(&quarantine_object, source_bytes)
        {
            Ok(outcome) => {
                super::validate_publish_outcome(&quarantine_object, byte_count, &outcome).map_err(
                    |mismatch| QuarantineServiceError::InvalidPublishMetadata {
                        object: mismatch.object().clone(),
                        field: mismatch.field(),
                    },
                )?;
                let source_delete = delete_source(&self.backend, &request.source_object);
                let status = status_after_source_delete(&source_delete);
                let mut report = QuarantineObjectReport::new(
                    status,
                    request,
                    quarantine_object,
                    byte_count,
                    inventory_write.inventory().entries().len(),
                );
                report.inventory_write = Some(inventory_write);
                report.quarantine_publish_outcome = Some(outcome);
                report.source_delete = Some(source_delete);
                Ok(report)
            }
            Err(source) => {
                let status = if publish_failure_is_uncertain(source.kind()) {
                    // Both visibility-unknown and visible-but-unconfirmed
                    // publish windows share this report status. Callers that
                    // need the exact split must inspect the retained publish
                    // failure kind.
                    QuarantineObjectStatus::QuarantinePublishUncertain
                } else {
                    QuarantineObjectStatus::QuarantinePublishFailed
                };
                let mut report = QuarantineObjectReport::new(
                    status,
                    request,
                    quarantine_object.clone(),
                    byte_count,
                    inventory_write.inventory().entries().len(),
                );
                report.inventory_write = Some(inventory_write);
                report.quarantine_publish_failure =
                    Some(QuarantinePublishFailure::new(quarantine_object, source));
                Ok(report)
            }
        }
    }

    pub(crate) fn purge_quarantine(
        &self,
        request: QuarantinePurgeRequest,
    ) -> QuarantineServiceResult<QuarantinePurgeReport> {
        validate_gate(request.gate)?;
        let inventory =
            self.load_inventory(request.branch_id, request.database_id, &request.codec_id)?;
        validate_purge_inventory_token(&request, &inventory)?;
        let inventory_object = inventory.object().clone();
        let mut report = QuarantinePurgeReport::new(request.branch_id, inventory_object);
        if inventory.inventory().entries().is_empty() {
            return Ok(report);
        }

        require_capabilities(
            &self.backend,
            &[
                BackendCapability::DeleteObject,
                BackendCapability::DurablePublish,
                BackendCapability::DurableSync,
            ],
        )?;

        // Purge deletes only inventory-listed quarantine objects and rewrites
        // the inventory after attempting every delete. Delete failures are
        // represented as retained entries so callers can retry without
        // reconstructing reachability facts.
        for entry in inventory.inventory().entries() {
            let object = quarantine_object_name(request.branch_id, entry.object_id())?;
            match self.backend.delete_object(&object) {
                Ok(outcome) if outcome.status() == DeleteStatus::Deleted => {
                    report.reclaimed_bytes =
                        report.reclaimed_bytes.saturating_add(entry.byte_count());
                    report
                        .deleted
                        .push(QuarantineDeleteOutcome::deleted(object));
                }
                Ok(_) => {
                    report.reclaimed_bytes =
                        report.reclaimed_bytes.saturating_add(entry.byte_count());
                    report
                        .already_missing
                        .push(QuarantineDeleteOutcome::missing(object));
                }
                Err(source) => {
                    report.retained_entries.push(entry.clone());
                    report
                        .failed
                        .push(QuarantineDeleteOutcome::failed(object, source.into()));
                }
            }
        }

        let rewritten = QuarantineInventory::new(
            request.database_id,
            request.branch_id,
            request.codec_id,
            report.retained_entries.clone(),
        )
        .map_err(|_| QuarantineServiceError::InvalidRequest { field: "inventory" })?;
        match self.publish_inventory_replace(&rewritten) {
            Ok(write) => report.inventory_write = Some(write),
            Err(QuarantineServiceError::Publish { object, source }) => {
                report.inventory_publish_failure =
                    Some(QuarantinePublishFailure::new(object, source));
            }
            Err(source) => return Err(source),
        }
        Ok(report)
    }

    fn handle_existing_quarantine_entry(
        &self,
        request: &QuarantineObjectRequest,
        quarantine_object: &ObjectName,
        entry: &QuarantineEntry,
        quarantine_bytes: Option<Vec<u8>>,
        source_bytes: Option<Vec<u8>>,
        entry_count: usize,
    ) -> QuarantineServiceResult<QuarantineObjectReport> {
        if entry.source_object() != &request.source_object {
            return Err(QuarantineServiceError::InventoryMismatch {
                object_id: request.object_id.clone(),
                quarantine_object: quarantine_object.clone(),
                source_object: request.source_object.clone(),
                reason: "inventory source object differs from request",
            });
        }

        let Some(quarantine_bytes) = quarantine_bytes else {
            return Err(QuarantineServiceError::InventoryMismatch {
                object_id: request.object_id.clone(),
                quarantine_object: quarantine_object.clone(),
                source_object: request.source_object.clone(),
                reason: "inventory entry has no quarantine object",
            });
        };

        let byte_count = entry.byte_count();
        if byte_count != quarantine_bytes.len() as u64 {
            return Err(QuarantineServiceError::InventoryMismatch {
                object_id: request.object_id.clone(),
                quarantine_object: quarantine_object.clone(),
                source_object: request.source_object.clone(),
                reason: "quarantine byte count differs from inventory",
            });
        }

        let Some(source_bytes) = source_bytes else {
            return Ok(QuarantineObjectReport::new(
                QuarantineObjectStatus::AlreadyQuarantined,
                request,
                quarantine_object.clone(),
                byte_count,
                entry_count,
            ));
        };

        if source_bytes != quarantine_bytes {
            return Err(QuarantineServiceError::InventoryMismatch {
                object_id: request.object_id.clone(),
                quarantine_object: quarantine_object.clone(),
                source_object: request.source_object.clone(),
                reason: "source and quarantine bytes differ",
            });
        }

        let source_delete = delete_source(&self.backend, &request.source_object);
        let status = status_after_retry_source_delete(&source_delete);
        let mut report = QuarantineObjectReport::new(
            status,
            request,
            quarantine_object.clone(),
            byte_count,
            entry_count,
        );
        report.source_delete = Some(source_delete);
        Ok(report)
    }
}

fn validate_purge_inventory_token(
    request: &QuarantinePurgeRequest,
    inventory: &super::QuarantineInventoryLoad,
) -> QuarantineServiceResult<()> {
    let Some(expected) = request.expected_inventory_token else {
        return Err(QuarantineServiceError::InvalidRequest {
            field: "inventory_token",
        });
    };
    if inventory.token() == expected {
        return Ok(());
    }
    Err(QuarantineServiceError::InventoryMismatch {
        object_id: ObjectLayout::quarantine_inventory_object_id().to_owned(),
        quarantine_object: inventory.object().clone(),
        source_object: inventory.object().clone(),
        reason: "inventory token differs from purge proof",
    })
}

fn validate_gate(gate: QuarantineGate) -> QuarantineServiceResult<()> {
    if gate == QuarantineGate::Safe {
        return Ok(());
    }
    Err(QuarantineServiceError::UnsafeGate { gate })
}

fn validate_quarantine_request(request: &QuarantineObjectRequest) -> QuarantineServiceResult<()> {
    if request.object_id.is_empty() {
        return Err(QuarantineServiceError::InvalidRequest { field: "object_id" });
    }
    if request.object_id == ObjectLayout::quarantine_inventory_object_id() {
        return Err(QuarantineServiceError::InvalidRequest { field: "object_id" });
    }
    quarantine_object_name(request.branch_id, &request.object_id)?;
    if request.quarantined_at == Timestamp::EPOCH && !request.allow_epoch_timestamp {
        return Err(QuarantineServiceError::InvalidRequest {
            field: "quarantined_at",
        });
    }
    match ObjectFamily::from_object_name(&request.source_object) {
        Some(ObjectFamily::Quarantine) | None => Err(QuarantineServiceError::InvalidRequest {
            field: "source_object",
        }),
        Some(_) => Ok(()),
    }
}

fn updated_inventory_with_entry(
    request: &QuarantineObjectRequest,
    current_inventory: &QuarantineInventory,
    byte_count: u64,
) -> QuarantineServiceResult<QuarantineInventory> {
    let new_entry = QuarantineEntry::new(
        request.object_id.clone(),
        request.source_object.clone(),
        byte_count,
        request.quarantined_at,
    )
    .map_err(|_| QuarantineServiceError::InvalidRequest { field: "entry" })?;
    let mut entries = current_inventory.entries().to_vec();
    entries.push(new_entry);
    QuarantineInventory::new(
        request.database_id,
        request.branch_id,
        request.codec_id.clone(),
        entries,
    )
    .map_err(|_| QuarantineServiceError::InvalidRequest { field: "inventory" })
}

fn require_capabilities(
    backend: &dyn crate::backend::Backend,
    capabilities: &[BackendCapability],
) -> QuarantineServiceResult<()> {
    for capability in capabilities {
        require_capability(backend, *capability)?;
    }
    Ok(())
}

fn read_object_optional(
    backend: &dyn crate::backend::Backend,
    object: &ObjectName,
) -> QuarantineServiceResult<Option<Vec<u8>>> {
    match backend.read_object(object) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(source) if source.kind() == BackendErrorKind::NotFound => Ok(None),
        Err(source) => Err(QuarantineServiceError::Read {
            object: object.clone(),
            source,
        }),
    }
}

fn object_exists(
    backend: &dyn crate::backend::Backend,
    object: &ObjectName,
) -> QuarantineServiceResult<bool> {
    match backend.object_metadata(object) {
        Ok(_) => Ok(true),
        Err(source) if source.kind() == BackendErrorKind::NotFound => Ok(false),
        Err(source) => Err(QuarantineServiceError::Metadata {
            object: object.clone(),
            source,
        }),
    }
}

fn quarantine_object_name(
    branch_id: BranchId,
    object_id: &str,
) -> QuarantineServiceResult<ObjectName> {
    ObjectLayout::quarantine_object(&branch_id.to_string(), object_id)
        .map_err(|source| QuarantineServiceError::Layout { source })
}

fn delete_source(
    backend: &dyn crate::backend::Backend,
    source_object: &ObjectName,
) -> QuarantineDeleteOutcome {
    match backend.delete_object(source_object) {
        Ok(outcome) if outcome.status() == DeleteStatus::Deleted => {
            QuarantineDeleteOutcome::deleted(source_object.clone())
        }
        Ok(_) => QuarantineDeleteOutcome::missing(source_object.clone()),
        Err(source) if source.source_error().kind() == BackendErrorKind::NotFound => {
            QuarantineDeleteOutcome::missing(source_object.clone())
        }
        Err(source) => QuarantineDeleteOutcome::failed(source_object.clone(), source.into()),
    }
}

fn status_after_source_delete(source_delete: &QuarantineDeleteOutcome) -> QuarantineObjectStatus {
    if source_delete.deleted {
        QuarantineObjectStatus::QuarantinedSourceDeleted
    } else if source_delete.already_missing {
        QuarantineObjectStatus::SourceAlreadyMissingAfterPublish
    } else {
        QuarantineObjectStatus::QuarantinedSourceDeleteFailed
    }
}

fn status_after_retry_source_delete(
    source_delete: &QuarantineDeleteOutcome,
) -> QuarantineObjectStatus {
    if source_delete.deleted {
        QuarantineObjectStatus::SourceDeleteRetried
    } else if source_delete.already_missing {
        QuarantineObjectStatus::SourceAlreadyMissingAfterPublish
    } else {
        QuarantineObjectStatus::QuarantinedSourceDeleteFailed
    }
}

const fn publish_failure_is_uncertain(kind: PublishFailureKind) -> bool {
    matches!(
        kind,
        PublishFailureKind::VisibilityUnknown | PublishFailureKind::VisibleDurabilityUnconfirmed
    )
}
