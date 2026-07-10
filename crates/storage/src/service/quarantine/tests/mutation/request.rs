use super::*;
use crate::layout::ObjectFamily;

#[test]
fn safe_gate_allows_backend_access_for_valid_request() {
    let branch_id = branch_id();
    let source_object = source_object();
    let backend = MutationBackend::durable().with_object(source_object.clone(), b"table");
    let service = QuarantineService::new(&backend);

    service
        .quarantine_object(&request(branch_id, "table0002", source_object.clone()))
        .expect("safe request reaches backend");

    assert_eq!(backend.read_count(&source_object), 1);
    assert!(!backend.operations().is_empty());
}

#[test]
fn quarantine_rejects_empty_object_id_before_backend_access() {
    let branch_id = branch_id();
    let source_object = source_object();
    let backend = MutationBackend::durable().with_object(source_object.clone(), b"table");
    let service = QuarantineService::new(&backend);

    assert_eq!(
        service.quarantine_object(&request(branch_id, "", source_object.clone())),
        Err(QuarantineServiceError::InvalidRequest { field: "object_id" })
    );
    assert_eq!(backend.read_count(&source_object), 0);
    assert!(backend.operations().is_empty());
}

#[test]
fn quarantine_rejects_invalid_source_family_before_backend_access() {
    let branch_id = branch_id();
    let quarantine_source = quarantine_object(branch_id, "already-held");
    let unknown_source = ObjectName::new("unknown/table0002").expect("unknown source object");

    for source_object in [quarantine_source, unknown_source] {
        let backend = MutationBackend::durable().with_object(source_object.clone(), b"table");
        let service = QuarantineService::new(&backend);

        assert_eq!(
            service.quarantine_object(&request(branch_id, "table0002", source_object.clone())),
            Err(QuarantineServiceError::InvalidRequest {
                field: "source_object",
            })
        );
        assert_eq!(backend.read_count(&source_object), 0);
        assert!(backend.operations().is_empty());
    }
}

#[test]
fn quarantine_rejects_existing_copy_without_inventory_entry_before_mutation() {
    let branch_id = branch_id();
    let source_object = source_object();
    let quarantine_object = quarantine_object(branch_id, "table0002");
    let backend = MutationBackend::durable()
        .with_object(source_object.clone(), b"table")
        .with_object(quarantine_object.clone(), b"table");
    let service = QuarantineService::new(&backend);

    assert!(matches!(
        service.quarantine_object(&request(branch_id, "table0002", source_object.clone())),
        Err(QuarantineServiceError::InventoryMismatch {
            object_id,
            quarantine_object: object,
            source_object: reported_source,
            reason: "quarantine object is not listed in inventory",
        }) if object_id == "table0002" && object == quarantine_object && reported_source == source_object
    ));
    assert!(backend.contains(&source_object));
    assert!(backend.contains(&quarantine_object));
    assert_eq!(backend.read_count(&source_object), 0);
    assert_eq!(backend.read_count(&quarantine_object), 0);
    assert_eq!(
        backend.operations(),
        vec![Operation::Metadata(quarantine_object)]
    );
}

#[test]
fn branch_id_request_validation_is_owned_by_core_atom_and_layout_parsing() {
    let branch_id = branch_id();
    let source_object = source_object();
    let backend = MutationBackend::durable().with_object(source_object.clone(), b"table");
    let service = QuarantineService::new(&backend);

    let report = service
        .quarantine_object(&request(branch_id, "table0002", source_object))
        .expect("typed branch id request");

    assert_eq!(report.branch_id(), branch_id);
    assert_eq!(
        ObjectFamily::from_object_name(report.quarantine_object()),
        Some(ObjectFamily::Quarantine)
    );
}
