mod backend;
mod facts;
mod operation;

use super::service_fuzz::ServiceFuzzViolation;
use crate::backend::{BackendErrorKind, PublishFailureKind};
use crate::layout::ObjectLayout;
use crate::object::ObjectName;
use crate::service::{
    QuarantineGate, QuarantineObjectRequest, QuarantineObjectStatus, QuarantinePurgeRequest,
    QuarantineReconciliationKind, QuarantineService, QuarantineServiceError,
};
use backend::{BackendAccess, QuarantineScriptBackend};
use facts::{
    actual_inventory_entries, actual_listed_objects, actual_missing_objects,
    actual_unlisted_objects, expected_inventory_entries, expected_kind,
    expected_reconciliation_facts,
};
use operation::{QuarantineFault, QuarantineOperation};
use std::collections::{BTreeMap, BTreeSet};
use strata_core::{BranchId, Timestamp};

// Stateful quarantine fuzzing uses one bytecode interpreter for both proptest
// and cargo-fuzz, so the deterministic property and the mutational fuzz target
// are forced to agree on the same recovery model.
const DATABASE_ID: [u8; 16] = [0x71; 16];
const CODEC_ID: &str = "identity";
const BYTES_PER_OPERATION: usize = 8;
const MAX_OPERATIONS: usize = 96;
const MAX_TOTAL_PAYLOAD_BYTES: usize = 64 * 1024;

type FuzzResult<T> = Result<T, ServiceFuzzViolation>;
type BranchKey = [u8; BranchId::BYTE_LEN];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuarantineServiceFuzzOutcome {
    steps_executed: usize,
}

impl QuarantineServiceFuzzOutcome {
    pub const fn steps_executed(self) -> usize {
        self.steps_executed
    }
}

pub fn run_quarantine_service_script(bytes: &[u8]) -> FuzzResult<QuarantineServiceFuzzOutcome> {
    let backend = QuarantineScriptBackend::default();
    let mut model = QuarantineScriptModel::default();
    let mut steps_executed = 0;

    for chunk in bytes.chunks_exact(BYTES_PER_OPERATION).take(MAX_OPERATIONS) {
        let operation = QuarantineOperation::from_chunk(chunk);
        apply_operation(&backend, &mut model, operation)?;
        // Every operation is followed by a full invariant check so shrinking
        // finds the first operation that diverges from the reference model.
        assert_model_matches_backend(&backend, &model)?;
        steps_executed += 1;
    }

    Ok(QuarantineServiceFuzzOutcome { steps_executed })
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct QuarantineScriptModel {
    sources: BTreeMap<ObjectName, Vec<u8>>,
    // BranchId intentionally has no Ord; the model uses canonical bytes so
    // invariant checks remain deterministic without changing the public atom.
    inventories: BTreeMap<BranchKey, InventoryState>,
    quarantine_objects: BTreeMap<(BranchKey, String), Vec<u8>>,
    touched_sources: BTreeSet<ObjectName>,
    touched_quarantine_objects: BTreeSet<(BranchKey, String)>,
    touched_branches: BTreeSet<BranchKey>,
    total_payload_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum InventoryState {
    Present(BTreeMap<String, InventoryEntryState>),
    Corrupt,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InventoryEntryState {
    source_object: ObjectName,
    bytes: Vec<u8>,
    quarantined_at: Timestamp,
}

fn apply_operation(
    backend: &QuarantineScriptBackend,
    model: &mut QuarantineScriptModel,
    operation: QuarantineOperation,
) -> FuzzResult<()> {
    match operation {
        QuarantineOperation::SeedSource {
            branch_id,
            object_id,
            payload_len,
            payload_seed,
        } => seed_source(
            backend,
            model,
            branch_id,
            &object_id,
            payload_len,
            payload_seed,
        ),
        QuarantineOperation::QuarantineObject {
            branch_id,
            object_id,
            gate,
            fault,
            quarantined_at,
            allow_epoch_timestamp,
        } => quarantine_object(
            backend,
            model,
            branch_id,
            object_id,
            gate,
            fault,
            quarantined_at,
            allow_epoch_timestamp,
        ),
        QuarantineOperation::PurgeBranch {
            branch_id,
            object_id,
            gate,
            fail_delete,
        } => purge_branch(backend, model, branch_id, &object_id, gate, fail_delete),
        QuarantineOperation::CorruptInventory { branch_id } => {
            corrupt_inventory(backend, model, branch_id)
        }
        QuarantineOperation::InsertUnlistedObject {
            branch_id,
            object_id,
            payload_len,
            payload_seed,
        } => insert_unlisted_object(
            backend,
            model,
            branch_id,
            object_id,
            payload_len,
            payload_seed,
        ),
        QuarantineOperation::DeleteQuarantineObject {
            branch_id,
            object_id,
        } => delete_quarantine_object(backend, model, branch_id, object_id),
        QuarantineOperation::ReconcileBranch { branch_id } => {
            model.touched_branches.insert(branch_key(branch_id));
            assert_reconcile_matches_model(backend, model, branch_id)
        }
        QuarantineOperation::LoadInventory { branch_id } => {
            model.touched_branches.insert(branch_key(branch_id));
            assert_load_matches_model(backend, model, branch_id)
        }
    }
}

fn seed_source(
    backend: &QuarantineScriptBackend,
    model: &mut QuarantineScriptModel,
    branch_id: BranchId,
    object_id: &str,
    payload_len: usize,
    payload_seed: u8,
) -> FuzzResult<()> {
    model.touched_branches.insert(branch_key(branch_id));
    let source_object = source_object(object_id)?;
    let payload = bounded_payload(model, payload_len, payload_seed);
    backend.write_visible(source_object.clone(), payload.clone())?;
    model.touched_sources.insert(source_object.clone());
    model.sources.insert(source_object, payload);
    Ok(())
}

fn quarantine_object(
    backend: &QuarantineScriptBackend,
    model: &mut QuarantineScriptModel,
    branch_id: BranchId,
    object_id: String,
    gate: QuarantineGate,
    fault: QuarantineFault,
    quarantined_at: Timestamp,
    allow_epoch_timestamp: bool,
) -> FuzzResult<()> {
    model.touched_branches.insert(branch_key(branch_id));
    model
        .touched_quarantine_objects
        .insert((branch_key(branch_id), object_id.clone()));
    let source_object = source_object(&object_id)?;
    let quarantine_object = quarantine_object_name(branch_id, &object_id)?;
    let inventory_object = inventory_object(branch_id)?;

    if gate == QuarantineGate::Safe {
        match fault {
            QuarantineFault::InventoryNoVisible => backend.fail_publish_once(
                inventory_object,
                PublishFailureKind::FailedBeforeVisibility,
                false,
            )?,
            QuarantineFault::InventoryVisibilityUnknownInvisible => backend.fail_publish_once(
                inventory_object,
                PublishFailureKind::VisibilityUnknown,
                false,
            )?,
            QuarantineFault::InventoryVisibilityUnknownVisible => backend.fail_publish_once(
                inventory_object,
                PublishFailureKind::VisibilityUnknown,
                true,
            )?,
            QuarantineFault::CopyNoVisible => backend.fail_publish_once(
                quarantine_object.clone(),
                PublishFailureKind::FailedBeforeVisibility,
                false,
            )?,
            QuarantineFault::CopyVisibilityUnknownVisible => backend.fail_publish_once(
                quarantine_object.clone(),
                PublishFailureKind::VisibilityUnknown,
                true,
            )?,
            QuarantineFault::CopyDurabilityUnconfirmed => backend.fail_publish_once(
                quarantine_object.clone(),
                PublishFailureKind::VisibleDurabilityUnconfirmed,
                true,
            )?,
            QuarantineFault::SourceDeleteFailure => {
                backend.fail_delete_once(source_object.clone(), BackendErrorKind::Interrupted)?;
            }
            QuarantineFault::None => {}
        }
    }

    let mut request = QuarantineObjectRequest::new(
        branch_id,
        DATABASE_ID,
        CODEC_ID,
        object_id.clone(),
        source_object.clone(),
        quarantined_at,
        gate,
    );
    if allow_epoch_timestamp {
        request = request.allow_epoch_timestamp();
    }
    let result = QuarantineService::new(backend).quarantine_object(&request);
    update_model_after_quarantine(
        model,
        branch_id,
        object_id,
        source_object,
        quarantined_at,
        allow_epoch_timestamp,
        fault,
        gate,
        &result,
    )?;
    backend.clear_faults()?;
    Ok(())
}

fn update_model_after_quarantine(
    model: &mut QuarantineScriptModel,
    branch_id: BranchId,
    object_id: String,
    source_object: ObjectName,
    quarantined_at: Timestamp,
    allow_epoch_timestamp: bool,
    fault: QuarantineFault,
    gate: QuarantineGate,
    result: &Result<crate::service::QuarantineObjectReport, QuarantineServiceError>,
) -> FuzzResult<()> {
    let key = (branch_key(branch_id), object_id.clone());
    if gate != QuarantineGate::Safe {
        return require(
            matches!(result, Err(QuarantineServiceError::UnsafeGate { gate: actual }) if *actual == gate),
            "non-safe quarantine gate mutated or returned wrong error",
        );
    }
    if epoch_timestamp_rejected(quarantined_at, allow_epoch_timestamp, result)? {
        return Ok(());
    }
    if matches!(
        model.inventories.get(&branch_key(branch_id)),
        Some(InventoryState::Corrupt)
    ) {
        return require(
            matches!(result, Err(QuarantineServiceError::Decode { .. })),
            "corrupt inventory did not stop quarantine request",
        );
    }

    let entry = model_entry(model, branch_id, &object_id).cloned();
    if let Some(entry) = entry {
        return update_model_after_existing_entry(
            model,
            &key,
            &source_object,
            fault,
            result,
            &entry,
        );
    }

    if model.quarantine_objects.contains_key(&key) {
        return require(
            matches!(
                result,
                Err(QuarantineServiceError::InventoryMismatch { .. })
            ),
            "unlisted existing quarantine object was not rejected",
        );
    }
    let Some(source_bytes) = model.sources.get(&source_object).cloned() else {
        return require(
            matches!(result, Err(QuarantineServiceError::Missing { object }) if object == &source_object),
            "missing source did not return missing error",
        );
    };

    apply_new_entry_fault_outcome(
        model,
        branch_id,
        object_id,
        source_object,
        source_bytes,
        quarantined_at,
        key,
        fault,
        result,
    )
}

fn apply_new_entry_fault_outcome(
    model: &mut QuarantineScriptModel,
    branch_id: BranchId,
    object_id: String,
    source_object: ObjectName,
    source_bytes: Vec<u8>,
    quarantined_at: Timestamp,
    key: (BranchKey, String),
    fault: QuarantineFault,
    result: &Result<crate::service::QuarantineObjectReport, QuarantineServiceError>,
) -> FuzzResult<()> {
    match fault {
        QuarantineFault::InventoryNoVisible => {
            require_status(result, QuarantineObjectStatus::InventoryPublishFailed)
        }
        QuarantineFault::InventoryVisibilityUnknownInvisible => {
            require_status(result, QuarantineObjectStatus::InventoryPublishUncertain)
        }
        QuarantineFault::InventoryVisibilityUnknownVisible => {
            // The inventory publish may already be visible, so reconciliation
            // must treat the entry as durable-enough evidence of uncertainty.
            insert_inventory_entry(
                model,
                branch_id,
                object_id,
                source_object,
                source_bytes,
                quarantined_at,
            )?;
            require_status(result, QuarantineObjectStatus::InventoryPublishUncertain)
        }
        QuarantineFault::CopyNoVisible => {
            insert_inventory_entry(
                model,
                branch_id,
                object_id,
                source_object,
                source_bytes,
                quarantined_at,
            )?;
            require_status(result, QuarantineObjectStatus::QuarantinePublishFailed)
        }
        QuarantineFault::CopyVisibilityUnknownVisible
        | QuarantineFault::CopyDurabilityUnconfirmed => {
            // Both uncertain publish outcomes leave copied bytes visible in
            // this backend, but callers still see a single uncertain status.
            insert_inventory_entry(
                model,
                branch_id,
                object_id.clone(),
                source_object,
                source_bytes.clone(),
                quarantined_at,
            )?;
            model.quarantine_objects.insert(key, source_bytes);
            require_status(result, QuarantineObjectStatus::QuarantinePublishUncertain)
        }
        QuarantineFault::SourceDeleteFailure => {
            insert_inventory_entry(
                model,
                branch_id,
                object_id.clone(),
                source_object,
                source_bytes.clone(),
                quarantined_at,
            )?;
            model.quarantine_objects.insert(key, source_bytes);
            require_status(
                result,
                QuarantineObjectStatus::QuarantinedSourceDeleteFailed,
            )
        }
        QuarantineFault::None => {
            insert_inventory_entry(
                model,
                branch_id,
                object_id.clone(),
                source_object.clone(),
                source_bytes.clone(),
                quarantined_at,
            )?;
            model.quarantine_objects.insert(key, source_bytes);
            model.sources.remove(&source_object);
            require_status(result, QuarantineObjectStatus::QuarantinedSourceDeleted)
        }
    }
}

fn update_model_after_existing_entry(
    model: &mut QuarantineScriptModel,
    key: &(BranchKey, String),
    source_object: &ObjectName,
    fault: QuarantineFault,
    result: &Result<crate::service::QuarantineObjectReport, QuarantineServiceError>,
    entry: &InventoryEntryState,
) -> FuzzResult<()> {
    let Some(quarantine_bytes) = model.quarantine_objects.get(key).cloned() else {
        return require_inventory_mismatch(
            result,
            "inventory entry without quarantine object was not rejected",
        );
    };
    if quarantine_bytes.len() as u64 != entry.bytes.len() as u64 {
        return require_inventory_mismatch(result, "quarantine byte-count drift was not rejected");
    }
    let Some(source_bytes) = model.sources.get(source_object).cloned() else {
        return require_status(result, QuarantineObjectStatus::AlreadyQuarantined);
    };
    if source_bytes != quarantine_bytes {
        return require_inventory_mismatch(result, "source/quarantine byte drift was not rejected");
    }
    if fault == QuarantineFault::SourceDeleteFailure {
        return require_status(
            result,
            QuarantineObjectStatus::QuarantinedSourceDeleteFailed,
        );
    }
    require_status(result, QuarantineObjectStatus::SourceDeleteRetried)?;
    model.sources.remove(source_object);
    Ok(())
}

fn require_inventory_mismatch(
    result: &Result<crate::service::QuarantineObjectReport, QuarantineServiceError>,
    message: &'static str,
) -> FuzzResult<()> {
    require(
        matches!(
            result,
            Err(QuarantineServiceError::InventoryMismatch { .. })
        ),
        message,
    )
}

fn purge_branch(
    backend: &QuarantineScriptBackend,
    model: &mut QuarantineScriptModel,
    branch_id: BranchId,
    object_id: &str,
    gate: QuarantineGate,
    fail_delete: bool,
) -> FuzzResult<()> {
    model.touched_branches.insert(branch_key(branch_id));
    if gate == QuarantineGate::Safe && fail_delete {
        backend.fail_delete_once(
            quarantine_object_name(branch_id, object_id)?,
            BackendErrorKind::Interrupted,
        )?;
    }
    let service = QuarantineService::new(backend);
    let token = if gate == QuarantineGate::Safe {
        service
            .load_inventory(branch_id, DATABASE_ID, CODEC_ID)
            .ok()
            .map(|inventory| inventory.token())
    } else {
        None
    };
    let request = QuarantinePurgeRequest::new(branch_id, DATABASE_ID, CODEC_ID, gate, token);
    let result = service.purge_quarantine(request);
    update_model_after_purge(model, branch_id, object_id, gate, fail_delete, &result)?;
    backend.clear_faults()?;
    Ok(())
}

fn update_model_after_purge(
    model: &mut QuarantineScriptModel,
    branch_id: BranchId,
    object_id: &str,
    gate: QuarantineGate,
    fail_delete: bool,
    result: &Result<crate::service::QuarantinePurgeReport, QuarantineServiceError>,
) -> FuzzResult<()> {
    if gate != QuarantineGate::Safe {
        return require(
            matches!(result, Err(QuarantineServiceError::UnsafeGate { gate: actual }) if *actual == gate),
            "non-safe purge gate mutated or returned wrong error",
        );
    }
    if matches!(
        model.inventories.get(&branch_key(branch_id)),
        Some(InventoryState::Corrupt)
    ) {
        return require(
            matches!(result, Err(QuarantineServiceError::Decode { .. })),
            "corrupt inventory did not stop purge",
        );
    }
    let Some(InventoryState::Present(entries)) =
        model.inventories.get(&branch_key(branch_id)).cloned()
    else {
        return require(result.is_ok(), "empty purge failed");
    };
    if entries.is_empty() {
        return require(result.is_ok(), "empty-present purge failed");
    }

    let mut retained = BTreeMap::new();
    for (entry_object_id, entry) in entries {
        let key = (branch_key(branch_id), entry_object_id.clone());
        let should_fail = fail_delete && entry_object_id == object_id;
        if should_fail {
            retained.insert(entry_object_id, entry);
        } else {
            model.quarantine_objects.remove(&key);
        }
    }
    model
        .inventories
        .insert(branch_key(branch_id), InventoryState::Present(retained));
    require(result.is_ok(), "purge report unexpectedly failed")
}

fn corrupt_inventory(
    backend: &QuarantineScriptBackend,
    model: &mut QuarantineScriptModel,
    branch_id: BranchId,
) -> FuzzResult<()> {
    model.touched_branches.insert(branch_key(branch_id));
    backend.write_visible(inventory_object(branch_id)?, b"corrupt inventory".to_vec())?;
    model
        .inventories
        .insert(branch_key(branch_id), InventoryState::Corrupt);
    Ok(())
}

fn insert_unlisted_object(
    backend: &QuarantineScriptBackend,
    model: &mut QuarantineScriptModel,
    branch_id: BranchId,
    object_id: String,
    payload_len: usize,
    payload_seed: u8,
) -> FuzzResult<()> {
    model.touched_branches.insert(branch_key(branch_id));
    model
        .touched_quarantine_objects
        .insert((branch_key(branch_id), object_id.clone()));
    let payload = bounded_payload(model, payload_len, payload_seed);
    backend.write_visible(
        quarantine_object_name(branch_id, &object_id)?,
        payload.clone(),
    )?;
    model
        .quarantine_objects
        .insert((branch_key(branch_id), object_id), payload);
    Ok(())
}

fn delete_quarantine_object(
    backend: &QuarantineScriptBackend,
    model: &mut QuarantineScriptModel,
    branch_id: BranchId,
    object_id: String,
) -> FuzzResult<()> {
    model.touched_branches.insert(branch_key(branch_id));
    model
        .touched_quarantine_objects
        .insert((branch_key(branch_id), object_id.clone()));
    let object = quarantine_object_name(branch_id, &object_id)?;
    backend.remove_visible(&object)?;
    model
        .quarantine_objects
        .remove(&(branch_key(branch_id), object_id));
    Ok(())
}

fn assert_model_matches_backend(
    backend: &QuarantineScriptBackend,
    model: &QuarantineScriptModel,
) -> FuzzResult<()> {
    for source in &model.touched_sources {
        require(
            backend.visible_bytes(source)? == model.sources.get(source).cloned(),
            "source visibility drifted from quarantine model",
        )?;
    }
    for (branch_key, object_id) in &model.touched_quarantine_objects {
        let branch_id = BranchId::from_bytes(*branch_key);
        let object = quarantine_object_name(branch_id, object_id)?;
        require(
            backend.visible_bytes(&object)?
                == model
                    .quarantine_objects
                    .get(&(*branch_key, object_id.clone()))
                    .cloned(),
            "quarantine object visibility drifted from model",
        )?;
    }
    for branch_key in &model.touched_branches {
        assert_reconcile_matches_model(backend, model, BranchId::from_bytes(*branch_key))?;
    }
    Ok(())
}

fn assert_reconcile_matches_model(
    backend: &QuarantineScriptBackend,
    model: &QuarantineScriptModel,
    branch_id: BranchId,
) -> FuzzResult<()> {
    let start = backend.access_len()?;
    let report = QuarantineService::new(backend)
        .reconcile_branch_quarantine(branch_id, DATABASE_ID, CODEC_ID)
        .map_err(|_| ServiceFuzzViolation::new("quarantine reconcile returned service error"))?;
    // Reconciliation is a diagnostic pass. The property treats any write,
    // publish, metadata rewrite, or delete as a recovery safety violation.
    require(
        backend
            .access_since(start)?
            .iter()
            .all(|access| matches!(access, BackendAccess::Read | BackendAccess::List)),
        "quarantine reconcile mutated backend state",
    )?;

    let expected = expected_kind(model, branch_id);
    require(
        report.kind() == expected,
        "quarantine reconcile kind did not match model",
    )?;
    match expected {
        QuarantineReconciliationKind::CleanEmpty => {
            require(
                !report.inventory_present(),
                "clean-empty inventory was present",
            )?;
            require(
                report.listed_objects().is_empty(),
                "clean-empty had listed objects",
            )?;
        }
        QuarantineReconciliationKind::CleanInventory => {
            let expected_facts = expected_reconciliation_facts(model, branch_id)?;
            require(
                actual_listed_objects(&report) == expected_facts.listed,
                "clean inventory listed facts did not match model",
            )?;
        }
        QuarantineReconciliationKind::CorruptInventory => {
            require(
                report.corrupt_inventory().is_some(),
                "corrupt inventory fact missing",
            )?;
        }
        QuarantineReconciliationKind::MissingQuarantineObject => {
            require(
                !report.missing_objects().is_empty(),
                "missing quarantine fact missing",
            )?;
        }
        QuarantineReconciliationKind::UnlistedQuarantineObject => {
            require(
                !report.unlisted_objects().is_empty(),
                "unlisted quarantine fact missing",
            )?;
        }
        QuarantineReconciliationKind::MalformedListedObject
        | QuarantineReconciliationKind::BackendUnavailable => {
            return Err(ServiceFuzzViolation::new(
                "quarantine model generated unreachable reconcile kind",
            ));
        }
    }
    let expected_facts = expected_reconciliation_facts(model, branch_id)?;
    require(
        actual_listed_objects(&report) == expected_facts.listed,
        "reconcile listed facts did not match model",
    )?;
    require(
        actual_missing_objects(&report) == expected_facts.missing,
        "reconcile missing facts did not match model",
    )?;
    require(
        actual_unlisted_objects(&report) == expected_facts.unlisted,
        "reconcile unlisted facts did not match model",
    )?;
    Ok(())
}

fn assert_load_matches_model(
    backend: &QuarantineScriptBackend,
    model: &QuarantineScriptModel,
    branch_id: BranchId,
) -> FuzzResult<()> {
    let result = QuarantineService::new(backend).load_inventory(branch_id, DATABASE_ID, CODEC_ID);
    match model.inventories.get(&branch_key(branch_id)) {
        Some(InventoryState::Corrupt) => require(
            matches!(result, Err(QuarantineServiceError::Decode { .. })),
            "corrupt inventory load did not return decode error",
        ),
        Some(InventoryState::Present(_entries)) => {
            let load = result.map_err(|_| {
                ServiceFuzzViolation::new("present quarantine inventory failed load")
            })?;
            require(load.is_present(), "present inventory loaded as absent")?;
            require(
                actual_inventory_entries(load.inventory())
                    == expected_inventory_entries(model, branch_id),
                "loaded inventory entries did not match model",
            )
        }
        None => {
            let load = result
                .map_err(|_| ServiceFuzzViolation::new("absent inventory failed empty load"))?;
            require(!load.is_present(), "absent inventory loaded as present")?;
            require(
                load.inventory().is_empty(),
                "absent inventory was not empty",
            )
        }
    }
}

fn insert_inventory_entry(
    model: &mut QuarantineScriptModel,
    branch_id: BranchId,
    object_id: String,
    source_object: ObjectName,
    bytes: Vec<u8>,
    quarantined_at: Timestamp,
) -> FuzzResult<()> {
    let entry = InventoryEntryState {
        source_object,
        bytes,
        quarantined_at,
    };
    let state = model
        .inventories
        .entry(branch_key(branch_id))
        .or_insert_with(|| InventoryState::Present(BTreeMap::new()));
    match state {
        InventoryState::Present(entries) => {
            entries.insert(object_id, entry);
            Ok(())
        }
        InventoryState::Corrupt => Err(ServiceFuzzViolation::new(
            "corrupt inventory accepted model entry insertion",
        )),
    }
}

fn model_entry<'a>(
    model: &'a QuarantineScriptModel,
    branch_id: BranchId,
    object_id: &str,
) -> Option<&'a InventoryEntryState> {
    match model.inventories.get(&branch_key(branch_id)) {
        Some(InventoryState::Present(entries)) => entries.get(object_id),
        _ => None,
    }
}

fn require_status(
    result: &Result<crate::service::QuarantineObjectReport, QuarantineServiceError>,
    status: QuarantineObjectStatus,
) -> FuzzResult<()> {
    require(
        matches!(result, Ok(report) if report.status() == status),
        "quarantine status did not match model",
    )
}

fn epoch_timestamp_rejected(
    quarantined_at: Timestamp,
    allow_epoch_timestamp: bool,
    result: &Result<crate::service::QuarantineObjectReport, QuarantineServiceError>,
) -> FuzzResult<bool> {
    if quarantined_at != Timestamp::EPOCH || allow_epoch_timestamp {
        return Ok(false);
    }
    require(
        matches!(result, Err(QuarantineServiceError::InvalidRequest { field }) if *field == "quarantined_at"),
        "disallowed epoch timestamp did not fail before backend access",
    )?;
    Ok(true)
}

fn bounded_payload(
    model: &mut QuarantineScriptModel,
    requested_len: usize,
    payload_seed: u8,
) -> Vec<u8> {
    let remaining = MAX_TOTAL_PAYLOAD_BYTES.saturating_sub(model.total_payload_bytes);
    let len = requested_len.min(remaining);
    model.total_payload_bytes += len;
    vec![payload_seed; len]
}

fn branch_key(branch_id: BranchId) -> BranchKey {
    *branch_id.as_bytes()
}

fn source_object(object_id: &str) -> FuzzResult<ObjectName> {
    ObjectLayout::table_object("main", 0, object_id)
        .map_err(|_| ServiceFuzzViolation::new("source object layout rejected valid id"))
}

fn inventory_object(branch_id: BranchId) -> FuzzResult<ObjectName> {
    ObjectLayout::quarantine_manifest(&branch_id.to_string())
        .map_err(|_| ServiceFuzzViolation::new("inventory object layout rejected valid branch"))
}

fn quarantine_object_name(branch_id: BranchId, object_id: &str) -> FuzzResult<ObjectName> {
    ObjectLayout::quarantine_object(&branch_id.to_string(), object_id)
        .map_err(|_| ServiceFuzzViolation::new("quarantine object layout rejected valid id"))
}

fn require(condition: bool, message: &'static str) -> FuzzResult<()> {
    if condition {
        Ok(())
    } else {
        Err(ServiceFuzzViolation::new(message))
    }
}

#[cfg(test)]
mod tests;
