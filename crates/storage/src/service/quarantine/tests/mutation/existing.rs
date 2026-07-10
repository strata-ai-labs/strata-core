use super::*;

#[test]
fn quarantine_retries_source_delete_for_matching_existing_copy_without_republish() {
    let branch_id = branch_id();
    let source_object = source_object();
    let quarantine_object = quarantine_object(branch_id, "table0002");
    let entry = QuarantineEntry::new(
        "table0002",
        source_object.clone(),
        5,
        Timestamp::from_micros(2),
    )
    .expect("entry");
    let inventory = inventory(branch_id, vec![entry]);
    let backend = MutationBackend::durable()
        .with_object(inventory_object(branch_id), &encode_inventory(&inventory))
        .with_object(source_object.clone(), b"table")
        .with_object(quarantine_object.clone(), b"table");
    let service = QuarantineService::new(&backend);

    let report = service
        .quarantine_object(&request(branch_id, "table0002", source_object.clone()))
        .expect("retry source delete");

    assert_eq!(report.status(), QuarantineObjectStatus::SourceDeleteRetried);
    assert!(report.inventory_write().is_none());
    assert!(report.quarantine_publish_outcome().is_none());
    assert!(!backend.contains(&source_object));
    assert_eq!(backend.operations(), vec![Operation::Delete(source_object)]);
}

#[test]
fn quarantine_existing_entry_with_missing_source_is_already_quarantined() {
    let branch_id = branch_id();
    let source_object = source_object();
    let quarantine_object = quarantine_object(branch_id, "table0002");
    let entry = QuarantineEntry::new(
        "table0002",
        source_object.clone(),
        5,
        Timestamp::from_micros(2),
    )
    .expect("entry");
    let inventory = inventory(branch_id, vec![entry]);
    let backend = MutationBackend::durable()
        .with_object(inventory_object(branch_id), &encode_inventory(&inventory))
        .with_object(quarantine_object.clone(), b"table");
    let service = QuarantineService::new(&backend);

    let report = service
        .quarantine_object(&request(branch_id, "table0002", source_object))
        .expect("already quarantined");

    assert_eq!(report.status(), QuarantineObjectStatus::AlreadyQuarantined);
    assert_eq!(report.byte_count(), 5);
    assert!(report.inventory_write().is_none());
    assert!(report.source_delete().is_none());
    assert!(backend.operations().is_empty());
}

#[test]
fn quarantine_existing_entry_rejects_source_drift_before_delete() {
    let branch_id = branch_id();
    let inventory_source = source_object();
    let request_source = table_source_object("table0003");
    let quarantine_object = quarantine_object(branch_id, "table0002");
    let entry = QuarantineEntry::new("table0002", inventory_source, 5, Timestamp::from_micros(2))
        .expect("entry");
    let inventory = inventory(branch_id, vec![entry]);
    let backend = MutationBackend::durable()
        .with_object(inventory_object(branch_id), &encode_inventory(&inventory))
        .with_object(request_source.clone(), b"table")
        .with_object(quarantine_object.clone(), b"table");
    let service = QuarantineService::new(&backend);

    assert!(matches!(
        service.quarantine_object(&request(branch_id, "table0002", request_source.clone())),
        Err(QuarantineServiceError::InventoryMismatch {
            object_id,
            quarantine_object: object,
            source_object,
            reason: "inventory source object differs from request",
        }) if object_id == "table0002" && object == quarantine_object && source_object == request_source
    ));
    assert!(backend.contains(&request_source));
    assert!(backend.contains(&quarantine_object));
    assert!(backend.operations().is_empty());
}

#[test]
fn quarantine_existing_entry_rejects_missing_or_size_drifted_copy() {
    let branch_id = branch_id();
    let source_object = source_object();
    let quarantine_object = quarantine_object(branch_id, "table0002");
    for (stored_copy, reason) in [
        (None, "inventory entry has no quarantine object"),
        (
            Some(b"wrong-size".as_slice()),
            "quarantine byte count differs from inventory",
        ),
    ] {
        let entry = QuarantineEntry::new(
            "table0002",
            source_object.clone(),
            5,
            Timestamp::from_micros(2),
        )
        .expect("entry");
        let inventory = inventory(branch_id, vec![entry]);
        let mut backend = MutationBackend::durable()
            .with_object(inventory_object(branch_id), &encode_inventory(&inventory));
        if let Some(bytes) = stored_copy {
            backend = backend.with_object(quarantine_object.clone(), bytes);
        }
        let service = QuarantineService::new(&backend);

        assert!(matches!(
            service.quarantine_object(&request(branch_id, "table0002", source_object.clone())),
            Err(QuarantineServiceError::InventoryMismatch {
                object_id,
                quarantine_object: object,
                reason: actual_reason,
                ..
            }) if object_id == "table0002" && object == quarantine_object && actual_reason == reason
        ));
        assert!(backend.operations().is_empty());
    }
}

#[test]
fn quarantine_existing_entry_rejects_different_source_and_copy_bytes() {
    let branch_id = branch_id();
    let source_object = source_object();
    let quarantine_object = quarantine_object(branch_id, "table0002");
    let entry = QuarantineEntry::new(
        "table0002",
        source_object.clone(),
        5,
        Timestamp::from_micros(2),
    )
    .expect("entry");
    let inventory = inventory(branch_id, vec![entry]);
    let backend = MutationBackend::durable()
        .with_object(inventory_object(branch_id), &encode_inventory(&inventory))
        .with_object(source_object.clone(), b"table")
        .with_object(quarantine_object.clone(), b"other");
    let service = QuarantineService::new(&backend);

    assert!(matches!(
        service.quarantine_object(&request(branch_id, "table0002", source_object.clone())),
        Err(QuarantineServiceError::InventoryMismatch {
            reason: "source and quarantine bytes differ",
            ..
        })
    ));
    assert!(backend.contains(&source_object));
    assert!(backend.contains(&quarantine_object));
    assert!(backend.operations().is_empty());
}

#[test]
fn quarantine_source_delete_not_found_after_copy_is_reported() {
    let branch_id = branch_id();
    let source_object = source_object();
    let quarantine_object = quarantine_object(branch_id, "table0002");
    let backend = MutationBackend::durable().with_object(source_object.clone(), b"table");
    backend.fail_delete_with(source_object.clone(), BackendErrorKind::NotFound);
    let service = QuarantineService::new(&backend);

    let report = service
        .quarantine_object(&request(branch_id, "table0002", source_object.clone()))
        .expect("delete not found report");

    assert_eq!(
        report.status(),
        QuarantineObjectStatus::SourceAlreadyMissingAfterPublish
    );
    assert!(report.source_delete().expect("delete").already_missing());
    assert!(!backend.contains(&source_object));
    assert!(backend.contains(&quarantine_object));
}

#[test]
fn quarantine_source_delete_failure_reports_partial_state() {
    let branch_id = branch_id();
    let source_object = source_object();
    let quarantine_object = quarantine_object(branch_id, "table0002");
    let backend = MutationBackend::durable().with_object(source_object.clone(), b"table");
    backend.fail_delete(source_object.clone());
    let service = QuarantineService::new(&backend);

    let report = service
        .quarantine_object(&request(branch_id, "table0002", source_object.clone()))
        .expect("source delete failure report");

    assert_eq!(
        report.status(),
        QuarantineObjectStatus::QuarantinedSourceDeleteFailed
    );
    assert!(report.source_delete().expect("delete").failure().is_some());
    assert!(backend.contains(&source_object));
    assert!(backend.contains(&quarantine_object));
}

#[test]
fn quarantine_rejects_metadata_size_mismatch_before_mutation() {
    let branch_id = branch_id();
    let source_object = source_object();
    let quarantine_object = quarantine_object(branch_id, "table0002");
    let inventory_object = inventory_object(branch_id);
    let backend = MutationBackend::durable().with_object(source_object.clone(), b"table");
    backend.override_metadata_size(source_object.clone(), 99);
    let service = QuarantineService::new(&backend);

    assert!(matches!(
        service.quarantine_object(&request(branch_id, "table0002", source_object.clone())),
        Err(QuarantineServiceError::BackendState {
            object,
            expected_size: 5,
            actual_size: 99,
        }) if object == source_object
    ));
    assert!(!backend.contains(&quarantine_object));
    assert!(!backend.contains(&inventory_object));
}
