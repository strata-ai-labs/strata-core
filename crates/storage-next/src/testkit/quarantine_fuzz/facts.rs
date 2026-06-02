use super::{
    branch_key, quarantine_object_name, FuzzResult, InventoryState, QuarantineScriptModel,
};
use crate::format::quarantine::QuarantineInventory;
use crate::object::ObjectName;
use crate::service::{QuarantineReconciliationKind, QuarantineReconciliationReport};
use strata_core_next::{BranchId, Timestamp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ExpectedInventoryEntry {
    object_id: String,
    source_object: ObjectName,
    byte_count: u64,
    quarantined_at: Timestamp,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct ExpectedReconciliationFacts {
    pub(super) listed: Vec<ExpectedListedObject>,
    pub(super) missing: Vec<ExpectedMissingObject>,
    pub(super) unlisted: Vec<ExpectedUnlistedObject>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ExpectedListedObject {
    object_id: String,
    object: ObjectName,
    source_object: ObjectName,
    byte_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ExpectedMissingObject {
    object_id: String,
    object: ObjectName,
    source_object: ObjectName,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ExpectedUnlistedObject {
    object_id: String,
    object: ObjectName,
}

pub(super) fn expected_inventory_entries(
    model: &QuarantineScriptModel,
    branch_id: BranchId,
) -> Vec<ExpectedInventoryEntry> {
    match model.inventories.get(&branch_key(branch_id)) {
        Some(InventoryState::Present(entries)) => entries
            .iter()
            .map(|(object_id, entry)| ExpectedInventoryEntry {
                object_id: object_id.clone(),
                source_object: entry.source_object.clone(),
                byte_count: entry.bytes.len() as u64,
                quarantined_at: entry.quarantined_at,
            })
            .collect(),
        Some(InventoryState::Corrupt) | None => Vec::new(),
    }
}

pub(super) fn expected_reconciliation_facts(
    model: &QuarantineScriptModel,
    branch_id: BranchId,
) -> FuzzResult<ExpectedReconciliationFacts> {
    let key = branch_key(branch_id);
    let mut facts = ExpectedReconciliationFacts::default();
    match model.inventories.get(&key) {
        Some(InventoryState::Present(entries)) => {
            for (object_id, entry) in entries {
                let object = quarantine_object_name(branch_id, object_id)?;
                if model
                    .quarantine_objects
                    .contains_key(&(key, object_id.clone()))
                {
                    facts.listed.push(ExpectedListedObject {
                        object_id: object_id.clone(),
                        object,
                        source_object: entry.source_object.clone(),
                        byte_count: entry.bytes.len() as u64,
                    });
                } else {
                    facts.missing.push(ExpectedMissingObject {
                        object_id: object_id.clone(),
                        object,
                        source_object: entry.source_object.clone(),
                    });
                }
            }
            push_unlisted_quarantine_objects(model, branch_id, &mut facts, |object_id| {
                !entries.contains_key(object_id)
            })?;
        }
        Some(InventoryState::Corrupt) | None => {
            push_unlisted_quarantine_objects(model, branch_id, &mut facts, |_| true)?;
        }
    }
    Ok(facts)
}

pub(super) fn expected_kind(
    model: &QuarantineScriptModel,
    branch_id: BranchId,
) -> QuarantineReconciliationKind {
    if matches!(
        model.inventories.get(&branch_key(branch_id)),
        Some(InventoryState::Corrupt)
    ) {
        return QuarantineReconciliationKind::CorruptInventory;
    }
    let Some(InventoryState::Present(entries)) = model.inventories.get(&branch_key(branch_id))
    else {
        return empty_inventory_kind(model, branch_id);
    };
    if entries.keys().any(|object_id| {
        !model
            .quarantine_objects
            .contains_key(&(branch_key(branch_id), object_id.clone()))
    }) {
        // Missing listed objects outrank unlisted objects because inventory
        // entries prove an interrupted quarantine that needs operator action.
        return QuarantineReconciliationKind::MissingQuarantineObject;
    }
    if has_unlisted_quarantine_object(model, branch_id, |object_id| {
        !entries.contains_key(object_id)
    }) {
        return QuarantineReconciliationKind::UnlistedQuarantineObject;
    }
    QuarantineReconciliationKind::CleanInventory
}

fn empty_inventory_kind(
    model: &QuarantineScriptModel,
    branch_id: BranchId,
) -> QuarantineReconciliationKind {
    if has_unlisted_quarantine_object(model, branch_id, |_| true) {
        QuarantineReconciliationKind::UnlistedQuarantineObject
    } else {
        QuarantineReconciliationKind::CleanEmpty
    }
}

fn has_unlisted_quarantine_object(
    model: &QuarantineScriptModel,
    branch_id: BranchId,
    mut include: impl FnMut(&str) -> bool,
) -> bool {
    let key = branch_key(branch_id);
    model
        .quarantine_objects
        .keys()
        .any(|(object_branch, object_id)| *object_branch == key && include(object_id))
}

fn push_unlisted_quarantine_objects(
    model: &QuarantineScriptModel,
    branch_id: BranchId,
    facts: &mut ExpectedReconciliationFacts,
    mut include: impl FnMut(&str) -> bool,
) -> FuzzResult<()> {
    let key = branch_key(branch_id);
    for quarantine_key in model.quarantine_objects.keys() {
        let (object_branch, object_id) = quarantine_key;
        if *object_branch == key && include(object_id) {
            facts.unlisted.push(ExpectedUnlistedObject {
                object_id: object_id.clone(),
                object: quarantine_object_name(branch_id, object_id)?,
            });
        }
    }
    Ok(())
}

pub(super) fn actual_inventory_entries(
    inventory: &QuarantineInventory,
) -> Vec<ExpectedInventoryEntry> {
    inventory
        .entries()
        .iter()
        .map(|entry| ExpectedInventoryEntry {
            object_id: entry.object_id().to_owned(),
            source_object: entry.source_object().clone(),
            byte_count: entry.byte_count(),
            quarantined_at: entry.quarantined_at(),
        })
        .collect()
}

pub(super) fn actual_listed_objects(
    report: &QuarantineReconciliationReport,
) -> Vec<ExpectedListedObject> {
    report
        .listed_objects()
        .iter()
        .map(|object| ExpectedListedObject {
            object_id: object.object_id().to_owned(),
            object: object.object().clone(),
            source_object: object.source_object().clone(),
            byte_count: object.byte_count(),
        })
        .collect()
}

pub(super) fn actual_missing_objects(
    report: &QuarantineReconciliationReport,
) -> Vec<ExpectedMissingObject> {
    report
        .missing_objects()
        .iter()
        .map(|object| ExpectedMissingObject {
            object_id: object.object_id().to_owned(),
            object: object.object().clone(),
            source_object: object.source_object().clone(),
        })
        .collect()
}

pub(super) fn actual_unlisted_objects(
    report: &QuarantineReconciliationReport,
) -> Vec<ExpectedUnlistedObject> {
    report
        .unlisted_objects()
        .iter()
        .map(|object| ExpectedUnlistedObject {
            object_id: object.object_id().to_owned(),
            object: object.object().clone(),
        })
        .collect()
}
