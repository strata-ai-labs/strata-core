use super::*;
use crate::layout::ObjectFamily;

#[derive(Clone, Debug, Eq, PartialEq)]
enum ReconcileOperation {
    Read(ObjectName),
    List(ObjectPrefix),
    Write(ObjectName),
    Publish(ObjectName),
    Delete(ObjectName),
    Metadata(ObjectName),
}

#[derive(Debug, Default)]
struct ReconcileBackend {
    objects: Mutex<BTreeMap<ObjectName, Vec<u8>>>,
    operations: Mutex<Vec<ReconcileOperation>>,
    read_failures: Mutex<BTreeMap<ObjectName, BackendErrorKind>>,
    list_failure: Mutex<Option<BackendErrorKind>>,
    weak_listing: bool,
}

impl ReconcileBackend {
    fn new() -> Self {
        Self::default()
    }

    fn weak_listing() -> Self {
        Self {
            weak_listing: true,
            ..Self::default()
        }
    }

    fn with_object(self, object: ObjectName, bytes: &[u8]) -> Self {
        self.objects
            .lock()
            .expect("reconcile backend lock")
            .insert(object, bytes.to_vec());
        self
    }

    fn fail_read(&self, object: ObjectName, kind: BackendErrorKind) {
        self.read_failures
            .lock()
            .expect("reconcile backend lock")
            .insert(object, kind);
    }

    fn fail_list(&self, kind: BackendErrorKind) {
        *self.list_failure.lock().expect("reconcile backend lock") = Some(kind);
    }

    fn operations(&self) -> Vec<ReconcileOperation> {
        self.operations
            .lock()
            .expect("reconcile backend lock")
            .clone()
    }
}

impl Backend for ReconcileBackend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::from_slice(&[
            BackendCapability::ReadObject,
            BackendCapability::ListPrefix,
            BackendCapability::WriteObject,
            BackendCapability::DeleteObject,
            BackendCapability::ObjectMetadata,
            BackendCapability::DurablePublish,
            BackendCapability::DurableSync,
        ])
    }

    fn read_object(&self, name: &ObjectName) -> BackendResult<Vec<u8>> {
        self.operations
            .lock()
            .expect("reconcile backend lock")
            .push(ReconcileOperation::Read(name.clone()));
        if let Some(kind) = self
            .read_failures
            .lock()
            .expect("reconcile backend lock")
            .get(name)
            .copied()
        {
            return Err(BackendError::new(kind, "read failed"));
        }
        self.objects
            .lock()
            .expect("reconcile backend lock")
            .get(name)
            .cloned()
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "not found"))
    }

    fn read_range(&self, name: &ObjectName, range: BackendRange) -> BackendResult<Vec<u8>> {
        let bytes = self.read_object(name)?;
        let start = usize::try_from(range.offset())
            .map_err(|_| BackendError::new(BackendErrorKind::InvalidRange, "range overflow"))?;
        let end = range
            .end_offset()
            .ok_or_else(|| BackendError::new(BackendErrorKind::InvalidRange, "range overflow"))?;
        let end = usize::try_from(end)
            .map_err(|_| BackendError::new(BackendErrorKind::InvalidRange, "range overflow"))?;
        if start > bytes.len() {
            return Ok(Vec::new());
        }
        Ok(bytes[start..bytes.len().min(end)].to_vec())
    }

    fn write_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendMetadata> {
        self.operations
            .lock()
            .expect("reconcile backend lock")
            .push(ReconcileOperation::Write(name.clone()));
        self.objects
            .lock()
            .expect("reconcile backend lock")
            .insert(name.clone(), bytes.to_vec());
        Ok(BackendMetadata::new(bytes.len() as u64, None))
    }

    fn delete_object(&self, name: &ObjectName) -> crate::backend::DeleteResult {
        self.operations
            .lock()
            .expect("reconcile backend lock")
            .push(ReconcileOperation::Delete(name.clone()));
        let removed = self
            .objects
            .lock()
            .expect("reconcile backend lock")
            .remove(name)
            .is_some();
        crate::backend::durable_delete_result(name, removed)
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        self.operations
            .lock()
            .expect("reconcile backend lock")
            .push(ReconcileOperation::List(prefix.clone()));
        if let Some(kind) = *self.list_failure.lock().expect("reconcile backend lock") {
            return Err(BackendError::new(kind, "list failed"));
        }

        let mut objects: Vec<_> = self
            .objects
            .lock()
            .expect("reconcile backend lock")
            .keys()
            .filter(|object| self.weak_listing || object.as_str().starts_with(prefix.as_str()))
            .cloned()
            .collect();
        objects.sort();
        Ok(objects)
    }

    fn object_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
        self.operations
            .lock()
            .expect("reconcile backend lock")
            .push(ReconcileOperation::Metadata(name.clone()));
        self.objects
            .lock()
            .expect("reconcile backend lock")
            .get(name)
            .map(|bytes| BackendMetadata::new(bytes.len() as u64, None))
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "not found"))
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        bytes: &[u8],
        _mode: PublishMode,
    ) -> Result<PublishOutcome, PublishError> {
        self.operations
            .lock()
            .expect("reconcile backend lock")
            .push(ReconcileOperation::Publish(name.clone()));
        Ok(PublishOutcome::new(
            name.clone(),
            BackendMetadata::new(bytes.len() as u64, None),
            PublishDurability::Durable,
        ))
    }
}

fn quarantine_object(branch_id: BranchId, object_id: &str) -> ObjectName {
    ObjectLayout::quarantine_object(&branch_id.to_string(), object_id).expect("quarantine object")
}

fn malformed_branch_object() -> ObjectName {
    ObjectName::new(format!(
        "{}/{}/table0001",
        ObjectFamily::Quarantine.as_str(),
        "not-a-branch"
    ))
    .expect("malformed branch object")
}

fn malformed_object_id(branch_id: BranchId) -> ObjectName {
    ObjectName::new(format!(
        "{}/{}/table0001/extra",
        ObjectFamily::Quarantine.as_str(),
        branch_id
    ))
    .expect("malformed object id")
}

fn adjacent_family_object(branch_id: BranchId) -> ObjectName {
    ObjectName::new(format!(
        "{}x/{branch_id}/table0001",
        ObjectFamily::Quarantine.as_str()
    ))
    .expect("adjacent family object")
}

fn assert_read_only(backend: &ReconcileBackend) {
    assert!(backend.operations().iter().all(|operation| matches!(
        operation,
        ReconcileOperation::Read(_) | ReconcileOperation::List(_)
    )));
}

#[test]
fn reconcile_absent_inventory_and_no_objects_is_clean_empty() {
    let branch_id = branch_id();
    let backend = ReconcileBackend::new();
    let service = QuarantineService::new(&backend);

    let report = service
        .reconcile_branch_quarantine(branch_id, DATABASE_ID, CODEC_ID)
        .expect("reconcile branch");

    assert_eq!(report.kind(), QuarantineReconciliationKind::CleanEmpty);
    assert_eq!(report.branch_id(), branch_id);
    assert_eq!(report.inventory_object(), &inventory_object(branch_id));
    assert!(!report.inventory_present());
    assert!(report.listed_objects().is_empty());
    assert!(report.missing_objects().is_empty());
    assert!(report.unlisted_objects().is_empty());
    assert_read_only(&backend);
}

#[test]
fn reconcile_empty_inventory_and_no_objects_is_clean_inventory() {
    let branch_id = branch_id();
    let inventory = inventory(branch_id, Vec::new());
    let backend = ReconcileBackend::new()
        .with_object(inventory_object(branch_id), &encode_inventory(&inventory));
    let service = QuarantineService::new(&backend);

    let report = service
        .reconcile_branch_quarantine(branch_id, DATABASE_ID, CODEC_ID)
        .expect("reconcile branch");

    assert_eq!(report.kind(), QuarantineReconciliationKind::CleanInventory);
    assert!(report.inventory_present());
    assert!(report.corrupt_inventory().is_none());
    assert_read_only(&backend);
}

#[test]
fn reconcile_matching_inventory_and_objects_is_clean_inventory() {
    let branch_id = branch_id();
    let source_object = table_source_object("table0001");
    let quarantine_object = quarantine_object(branch_id, "table0001");
    let entry = QuarantineEntry::new(
        "table0001",
        source_object.clone(),
        5,
        Timestamp::from_micros(2),
    )
    .expect("entry");
    let inventory = inventory(branch_id, vec![entry]);
    let backend = ReconcileBackend::new()
        .with_object(inventory_object(branch_id), &encode_inventory(&inventory))
        .with_object(quarantine_object.clone(), b"table");
    let service = QuarantineService::new(&backend);

    let report = service
        .reconcile_branch_quarantine(branch_id, DATABASE_ID, CODEC_ID)
        .expect("reconcile branch");

    assert_eq!(report.kind(), QuarantineReconciliationKind::CleanInventory);
    assert_eq!(report.listed_objects().len(), 1);
    assert_eq!(report.listed_objects()[0].object_id(), "table0001");
    assert_eq!(report.listed_objects()[0].object(), &quarantine_object);
    assert_eq!(report.listed_objects()[0].source_object(), &source_object);
    assert_eq!(report.listed_objects()[0].byte_count(), 5);
    assert!(report.missing_objects().is_empty());
    assert!(report.unlisted_objects().is_empty());
    assert_read_only(&backend);
}

#[test]
fn reconcile_corrupt_inventory_is_policy_downgrade_with_source_fact() {
    let branch_id = branch_id();
    let object = inventory_object(branch_id);
    let orphan = quarantine_object(branch_id, "table0001");
    let backend = ReconcileBackend::new()
        .with_object(object.clone(), b"not inventory")
        .with_object(orphan.clone(), b"orphan");
    let service = QuarantineService::new(&backend);

    let report = service
        .reconcile_branch_quarantine(branch_id, DATABASE_ID, CODEC_ID)
        .expect("reconcile branch");

    assert_eq!(
        report.kind(),
        QuarantineReconciliationKind::CorruptInventory
    );
    let corrupt = report.corrupt_inventory().expect("corrupt inventory");
    assert_eq!(corrupt.object(), &object);
    assert!(matches!(
        corrupt.source(),
        QuarantineInventoryCorruption::Decode(FormatError::InsufficientBytes { .. })
    ));
    assert_eq!(report.unlisted_objects().len(), 1);
    assert_eq!(report.unlisted_objects()[0].object_id(), "table0001");
    assert_eq!(report.unlisted_objects()[0].object(), &orphan);
    assert_read_only(&backend);
}

#[test]
fn reconcile_identity_mismatch_is_corrupt_inventory_source_fact() {
    let branch_id = branch_id();
    let object = inventory_object(branch_id);
    let wrong_database_inventory =
        QuarantineInventory::new(OTHER_DATABASE_ID, branch_id, CODEC_ID, Vec::new())
            .expect("wrong database inventory");
    let backend = ReconcileBackend::new()
        .with_object(object.clone(), &encode_inventory(&wrong_database_inventory));
    let service = QuarantineService::new(&backend);

    let report = service
        .reconcile_branch_quarantine(branch_id, DATABASE_ID, CODEC_ID)
        .expect("reconcile branch");

    assert_eq!(
        report.kind(),
        QuarantineReconciliationKind::CorruptInventory
    );
    let corrupt = report.corrupt_inventory().expect("corrupt inventory");
    assert_eq!(corrupt.object(), &object);
    assert!(matches!(
        corrupt.source(),
        QuarantineInventoryCorruption::DatabaseMismatch {
            expected: DATABASE_ID,
            actual: OTHER_DATABASE_ID,
        }
    ));
    assert_read_only(&backend);
}

#[test]
fn reconcile_branch_identity_mismatch_is_corrupt_inventory_source_fact() {
    let branch_id = branch_id();
    let object = inventory_object(branch_id);
    let wrong_branch_inventory =
        QuarantineInventory::new(DATABASE_ID, other_branch_id(), CODEC_ID, Vec::new())
            .expect("wrong branch inventory");
    let backend = ReconcileBackend::new()
        .with_object(object.clone(), &encode_inventory(&wrong_branch_inventory));
    let service = QuarantineService::new(&backend);

    let report = service
        .reconcile_branch_quarantine(branch_id, DATABASE_ID, CODEC_ID)
        .expect("reconcile branch");

    assert_eq!(
        report.kind(),
        QuarantineReconciliationKind::CorruptInventory
    );
    let corrupt = report.corrupt_inventory().expect("corrupt inventory");
    assert_eq!(corrupt.object(), &object);
    assert!(matches!(
        corrupt.source(),
        QuarantineInventoryCorruption::BranchMismatch {
            expected,
            actual,
        } if *expected == branch_id && *actual == other_branch_id()
    ));
    assert_read_only(&backend);
}

#[test]
fn reconcile_codec_mismatch_is_corrupt_inventory_source_fact() {
    let branch_id = branch_id();
    let object = inventory_object(branch_id);
    let wrong_codec_inventory =
        QuarantineInventory::new(DATABASE_ID, branch_id, "other-codec", Vec::new())
            .expect("wrong codec inventory");
    let backend = ReconcileBackend::new()
        .with_object(object.clone(), &encode_inventory(&wrong_codec_inventory));
    let service = QuarantineService::new(&backend);

    let report = service
        .reconcile_branch_quarantine(branch_id, DATABASE_ID, CODEC_ID)
        .expect("reconcile branch");

    assert_eq!(
        report.kind(),
        QuarantineReconciliationKind::CorruptInventory
    );
    let corrupt = report.corrupt_inventory().expect("corrupt inventory");
    assert_eq!(corrupt.object(), &object);
    assert!(matches!(
        corrupt.source(),
        QuarantineInventoryCorruption::CodecMismatch {
            expected,
            actual,
        } if expected == CODEC_ID && actual == "other-codec"
    ));
    assert_read_only(&backend);
}

#[test]
fn reconcile_missing_inventory_object_is_policy_downgrade() {
    let branch_id = branch_id();
    let source_object = table_source_object("table0001");
    let quarantine_object = quarantine_object(branch_id, "table0001");
    let entry = QuarantineEntry::new(
        "table0001",
        source_object.clone(),
        5,
        Timestamp::from_micros(2),
    )
    .expect("entry");
    let inventory = inventory(branch_id, vec![entry]);
    let backend = ReconcileBackend::new()
        .with_object(inventory_object(branch_id), &encode_inventory(&inventory));
    let service = QuarantineService::new(&backend);

    let report = service
        .reconcile_branch_quarantine(branch_id, DATABASE_ID, CODEC_ID)
        .expect("reconcile branch");

    assert_eq!(
        report.kind(),
        QuarantineReconciliationKind::MissingQuarantineObject
    );
    assert_eq!(report.missing_objects().len(), 1);
    assert_eq!(report.missing_objects()[0].object_id(), "table0001");
    assert_eq!(report.missing_objects()[0].object(), &quarantine_object);
    assert_eq!(report.missing_objects()[0].source_object(), &source_object);
    assert_read_only(&backend);
}

#[test]
fn reconcile_unlisted_quarantine_object_is_policy_downgrade() {
    let branch_id = branch_id();
    let inventory = inventory(branch_id, Vec::new());
    let quarantine_object = quarantine_object(branch_id, "table0001");
    let backend = ReconcileBackend::new()
        .with_object(inventory_object(branch_id), &encode_inventory(&inventory))
        .with_object(quarantine_object.clone(), b"table");
    let service = QuarantineService::new(&backend);

    let report = service
        .reconcile_branch_quarantine(branch_id, DATABASE_ID, CODEC_ID)
        .expect("reconcile branch");

    assert_eq!(
        report.kind(),
        QuarantineReconciliationKind::UnlistedQuarantineObject
    );
    assert_eq!(report.unlisted_objects().len(), 1);
    assert_eq!(report.unlisted_objects()[0].object_id(), "table0001");
    assert_eq!(report.unlisted_objects()[0].object(), &quarantine_object);
    assert_read_only(&backend);
}

#[test]
fn reconcile_absent_inventory_with_quarantine_object_is_unlisted() {
    let branch_id = branch_id();
    let quarantine_object = quarantine_object(branch_id, "table0001");
    let backend = ReconcileBackend::new().with_object(quarantine_object.clone(), b"table");
    let service = QuarantineService::new(&backend);

    let report = service
        .reconcile_branch_quarantine(branch_id, DATABASE_ID, CODEC_ID)
        .expect("reconcile branch");

    assert_eq!(
        report.kind(),
        QuarantineReconciliationKind::UnlistedQuarantineObject
    );
    assert!(!report.inventory_present());
    assert_eq!(report.unlisted_objects().len(), 1);
    assert_eq!(report.unlisted_objects()[0].object_id(), "table0001");
    assert_eq!(report.unlisted_objects()[0].object(), &quarantine_object);
    assert_read_only(&backend);
}

#[test]
fn reconcile_malformed_object_id_is_policy_downgrade() {
    let branch_id = branch_id();
    let object = malformed_object_id(branch_id);
    let inventory = inventory(branch_id, Vec::new());
    let backend = ReconcileBackend::new()
        .with_object(inventory_object(branch_id), &encode_inventory(&inventory))
        .with_object(object.clone(), b"bad");
    let service = QuarantineService::new(&backend);

    let report = service
        .reconcile_branch_quarantine(branch_id, DATABASE_ID, CODEC_ID)
        .expect("reconcile branch");

    assert_eq!(
        report.kind(),
        QuarantineReconciliationKind::MalformedListedObject
    );
    assert_eq!(report.malformed_objects().len(), 1);
    assert_eq!(report.malformed_objects()[0].object(), &object);
    assert_eq!(report.malformed_objects()[0].object_id(), Some("table0001"));
    assert_eq!(report.malformed_objects()[0].reason(), "object_id");
    assert_read_only(&backend);
}

#[test]
fn family_reconcile_reports_malformed_branch_ids() {
    let object = malformed_branch_object();
    let backend = ReconcileBackend::new().with_object(object.clone(), b"bad");
    let service = QuarantineService::new(&backend);

    let family = service
        .reconcile_quarantine_family(DATABASE_ID, CODEC_ID)
        .expect("reconcile family");

    assert_eq!(
        family.kind(),
        QuarantineReconciliationKind::MalformedListedObject
    );
    assert!(family.branch_reports().is_empty());
    assert_eq!(family.malformed_objects().len(), 1);
    assert_eq!(family.malformed_objects()[0].object(), &object);
    assert_eq!(family.malformed_objects()[0].reason(), "branch");
    assert_read_only(&backend);
}

#[test]
fn family_reconcile_routes_valid_branch_malformed_objects_to_branch_report() {
    let branch_id = branch_id();
    let object = malformed_object_id(branch_id);
    let backend = ReconcileBackend::new().with_object(object.clone(), b"bad");
    let service = QuarantineService::new(&backend);

    let family = service
        .reconcile_quarantine_family(DATABASE_ID, CODEC_ID)
        .expect("reconcile family");

    assert_eq!(
        family.kind(),
        QuarantineReconciliationKind::MalformedListedObject
    );
    assert!(family.malformed_objects().is_empty());
    assert_eq!(family.branch_reports().len(), 1);
    let report = &family.branch_reports()[0];
    assert_eq!(report.branch_id(), branch_id);
    assert_eq!(
        report.kind(),
        QuarantineReconciliationKind::MalformedListedObject
    );
    assert_eq!(report.malformed_objects().len(), 1);
    assert_eq!(report.malformed_objects()[0].object(), &object);
    assert_read_only(&backend);
}

#[test]
fn family_reconcile_reports_highest_policy_severity_not_first_branch() {
    let first_branch = branch_id();
    let second_branch = other_branch_id();
    let first_branch_orphan = quarantine_object(first_branch, "table0001");
    let second_inventory_object = inventory_object(second_branch);
    let backend = ReconcileBackend::new()
        .with_object(first_branch_orphan, b"orphan")
        .with_object(second_inventory_object, b"not inventory");
    let service = QuarantineService::new(&backend);

    let family = service
        .reconcile_quarantine_family(DATABASE_ID, CODEC_ID)
        .expect("reconcile family");

    assert_eq!(family.branch_reports().len(), 2);
    assert_eq!(
        family.branch_reports()[0].kind(),
        QuarantineReconciliationKind::UnlistedQuarantineObject
    );
    assert_eq!(
        family.branch_reports()[1].kind(),
        QuarantineReconciliationKind::CorruptInventory
    );
    assert_eq!(
        family.kind(),
        QuarantineReconciliationKind::CorruptInventory
    );
    assert_read_only(&backend);
}

#[test]
fn reconcile_ignores_adjacent_family_objects_under_weak_listing() {
    let branch_id = branch_id();
    let adjacent = adjacent_family_object(branch_id);
    let backend = ReconcileBackend::weak_listing().with_object(adjacent, b"adjacent");
    let service = QuarantineService::new(&backend);

    let report = service
        .reconcile_branch_quarantine(branch_id, DATABASE_ID, CODEC_ID)
        .expect("reconcile branch");
    let family = service
        .reconcile_quarantine_family(DATABASE_ID, CODEC_ID)
        .expect("reconcile family");

    assert_eq!(report.kind(), QuarantineReconciliationKind::CleanEmpty);
    assert_eq!(family.kind(), QuarantineReconciliationKind::CleanEmpty);
    assert!(family.branch_reports().is_empty());
    assert_read_only(&backend);
}

#[test]
fn family_reconcile_list_failure_is_unavailable() {
    let backend = ReconcileBackend::new();
    backend.fail_list(BackendErrorKind::Unavailable);
    let service = QuarantineService::new(&backend);

    let family = service
        .reconcile_quarantine_family(DATABASE_ID, CODEC_ID)
        .expect("reconcile family");

    assert_eq!(
        family.kind(),
        QuarantineReconciliationKind::BackendUnavailable
    );
    assert!(family.branch_reports().is_empty());
    assert!(family.malformed_objects().is_empty());
    let unavailable = family.unavailable().expect("unavailable fact");
    assert_eq!(unavailable.operation(), "list_family");
    assert!(unavailable.object().is_none());
    assert_eq!(unavailable.source().kind(), BackendErrorKind::Unavailable);
    assert_read_only(&backend);
}

#[test]
fn reconcile_backend_list_failure_is_unavailable_without_mutation() {
    let branch_id = branch_id();
    let backend = ReconcileBackend::new();
    backend.fail_list(BackendErrorKind::Unavailable);
    let service = QuarantineService::new(&backend);

    let report = service
        .reconcile_branch_quarantine(branch_id, DATABASE_ID, CODEC_ID)
        .expect("reconcile branch");

    assert_eq!(
        report.kind(),
        QuarantineReconciliationKind::BackendUnavailable
    );
    let unavailable = report.unavailable().expect("unavailable fact");
    assert_eq!(unavailable.operation(), "list_branch");
    assert!(unavailable.object().is_none());
    assert_eq!(unavailable.source().kind(), BackendErrorKind::Unavailable);
    assert_read_only(&backend);
}

#[test]
fn reconcile_inventory_read_failure_is_unavailable_but_not_found_is_absent() {
    let branch_id = branch_id();
    let object = inventory_object(branch_id);
    let backend = ReconcileBackend::new().with_object(object.clone(), b"not read");
    backend.fail_read(object.clone(), BackendErrorKind::Unavailable);
    let service = QuarantineService::new(&backend);

    let report = service
        .reconcile_branch_quarantine(branch_id, DATABASE_ID, CODEC_ID)
        .expect("reconcile branch");

    assert_eq!(
        report.kind(),
        QuarantineReconciliationKind::BackendUnavailable
    );
    let unavailable = report.unavailable().expect("unavailable fact");
    assert_eq!(unavailable.operation(), "read_inventory");
    assert_eq!(unavailable.object(), Some(&object));
    assert_eq!(unavailable.source().kind(), BackendErrorKind::Unavailable);
    assert_read_only(&backend);

    let absent_backend = ReconcileBackend::new();
    let absent_service = QuarantineService::new(&absent_backend);
    let absent = absent_service
        .reconcile_branch_quarantine(branch_id, DATABASE_ID, CODEC_ID)
        .expect("reconcile absent branch");
    assert_eq!(absent.kind(), QuarantineReconciliationKind::CleanEmpty);
}
