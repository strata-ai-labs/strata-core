use super::*;
use crate::service::QuarantineDeleteOutcome;

fn purge_request(
    service: &QuarantineService<'_>,
    branch_id: BranchId,
    gate: QuarantineGate,
) -> QuarantinePurgeRequest {
    let token = if gate == QuarantineGate::Safe {
        Some(
            service
                .load_inventory(branch_id, DATABASE_ID, CODEC_ID)
                .expect("inventory token")
                .token(),
        )
    } else {
        None
    };
    QuarantinePurgeRequest::new(branch_id, DATABASE_ID, CODEC_ID, gate, token)
}

#[test]
fn purge_deletes_listed_objects_only_and_rewrites_empty_inventory() {
    let branch_id = branch_id();
    let source_object = source_object();
    let listed_object = quarantine_object(branch_id, "table0002");
    let unlisted_object = quarantine_object(branch_id, "table0003");
    let adjacent_family_object =
        ObjectName::new(format!("quarantinex/{branch_id}/table0004")).expect("adjacent object");
    let entry = QuarantineEntry::new("table0002", source_object, 5, Timestamp::from_micros(2))
        .expect("entry");
    let inventory = inventory(branch_id, vec![entry]);
    let backend = MutationBackend::durable()
        .with_object(inventory_object(branch_id), &encode_inventory(&inventory))
        .with_object(listed_object.clone(), b"table")
        .with_object(unlisted_object.clone(), b"other")
        .with_object(adjacent_family_object.clone(), b"adjacent");
    let service = QuarantineService::new(&backend);

    let report = service
        .purge_quarantine(purge_request(&service, branch_id, QuarantineGate::Safe))
        .expect("purge");

    assert_eq!(report.deleted().len(), 1);
    assert_eq!(report.branch_id(), branch_id);
    assert_eq!(report.inventory_object(), &inventory_object(branch_id));
    assert!(report.failed().is_empty());
    assert!(report.retained_entries().is_empty());
    assert!(!backend.contains(&listed_object));
    assert!(backend.contains(&unlisted_object));
    assert!(backend.contains(&adjacent_family_object));
    assert!(report
        .inventory_write()
        .expect("inventory rewrite")
        .inventory()
        .is_empty());
    assert!(service
        .load_required_inventory(branch_id, DATABASE_ID, CODEC_ID)
        .expect("load rewritten inventory")
        .inventory()
        .is_empty());
}

#[test]
fn purge_rejects_stale_inventory_token_before_delete() {
    let branch_id = branch_id();
    let source_object = source_object();
    let listed_object = quarantine_object(branch_id, "table0002");
    let entry = QuarantineEntry::new("table0002", source_object, 5, Timestamp::from_micros(2))
        .expect("entry");
    let inventory = inventory(branch_id, vec![entry]);
    let later_source = table_source_object("table0004");
    let later_quarantine = quarantine_object(branch_id, "table0004");
    let backend = MutationBackend::durable()
        .with_object(inventory_object(branch_id), &encode_inventory(&inventory))
        .with_object(listed_object.clone(), b"table")
        .with_object(later_source.clone(), b"later");
    let service = QuarantineService::new(&backend);
    let stale_token = service
        .load_inventory(branch_id, DATABASE_ID, CODEC_ID)
        .expect("stale token")
        .token();

    let mutation = service
        .quarantine_object(&request(branch_id, "table0004", later_source))
        .expect("inventory mutation");
    assert_eq!(
        mutation.status(),
        QuarantineObjectStatus::QuarantinedSourceDeleted
    );

    let purge = service
        .purge_quarantine(QuarantinePurgeRequest::new(
            branch_id,
            DATABASE_ID,
            CODEC_ID,
            QuarantineGate::Safe,
            Some(stale_token),
        ))
        .expect_err("stale proof rejected");

    assert!(matches!(
        purge,
        QuarantineServiceError::InventoryMismatch { .. }
    ));
    assert!(backend.contains(&listed_object));
    assert!(backend.contains(&later_quarantine));
}

#[test]
fn purge_unsafe_gate_fails_before_delete_or_rewrite() {
    let branch_id = branch_id();
    let source_object = source_object();
    let listed_object = quarantine_object(branch_id, "table0002");
    let entry = QuarantineEntry::new("table0002", source_object, 5, Timestamp::from_micros(2))
        .expect("entry");
    let inventory = inventory(branch_id, vec![entry]);
    for gate in [
        QuarantineGate::Referenced,
        QuarantineGate::UnsafeRecovery,
        QuarantineGate::ProofIncomplete,
    ] {
        let backend = MutationBackend::durable()
            .with_object(inventory_object(branch_id), &encode_inventory(&inventory))
            .with_object(listed_object.clone(), b"table");
        let service = QuarantineService::new(&backend);

        assert_eq!(
            service.purge_quarantine(purge_request(&service, branch_id, gate)),
            Err(QuarantineServiceError::UnsafeGate { gate })
        );
        assert!(backend.contains(&listed_object));
        assert!(backend.operations().is_empty());
    }
}

#[test]
fn purge_empty_inventory_reports_no_work_without_delete_capability() {
    let branch_id = branch_id();
    let inventory = inventory(branch_id, Vec::new());
    let mut backend = MutationBackend::durable()
        .with_object(inventory_object(branch_id), &encode_inventory(&inventory));
    backend.capabilities = BackendCapabilities::from_slice(&[
        BackendCapability::ReadObject,
        BackendCapability::DurablePublish,
        BackendCapability::DurableSync,
    ]);
    let service = QuarantineService::new(&backend);

    let report = service
        .purge_quarantine(purge_request(&service, branch_id, QuarantineGate::Safe))
        .expect("empty purge");

    assert!(report.deleted().is_empty());
    assert!(report.already_missing().is_empty());
    assert!(report.failed().is_empty());
    assert!(report.inventory_write().is_none());
    assert!(backend.operations().is_empty());
}

#[test]
fn purge_non_empty_inventory_rejects_each_missing_mutation_capability_before_mutation() {
    let branch_id = branch_id();
    let source_object = source_object();
    let listed_object = quarantine_object(branch_id, "table0002");
    let entry = QuarantineEntry::new("table0002", source_object, 5, Timestamp::from_micros(2))
        .expect("entry");
    let inventory = inventory(branch_id, vec![entry]);
    for missing in [
        BackendCapability::DeleteObject,
        BackendCapability::DurablePublish,
        BackendCapability::DurableSync,
    ] {
        let mut backend = MutationBackend::durable()
            .with_object(inventory_object(branch_id), &encode_inventory(&inventory))
            .with_object(listed_object.clone(), b"table");
        backend.capabilities = mutation_capabilities_without(missing);
        let service = QuarantineService::new(&backend);

        assert_eq!(
            service.purge_quarantine(purge_request(&service, branch_id, QuarantineGate::Safe)),
            Err(QuarantineServiceError::UnsupportedCapability {
                capability: missing,
            })
        );
        assert!(backend.contains(&listed_object));
        assert!(backend.operations().is_empty());
    }
}

#[test]
fn purge_delete_failure_keeps_failed_entry_in_rewritten_inventory() {
    let branch_id = branch_id();
    let source_object = source_object();
    let listed_object = quarantine_object(branch_id, "table0002");
    let entry = QuarantineEntry::new("table0002", source_object, 5, Timestamp::from_micros(2))
        .expect("entry");
    let inventory = inventory(branch_id, vec![entry]);
    let backend = MutationBackend::durable()
        .with_object(inventory_object(branch_id), &encode_inventory(&inventory))
        .with_object(listed_object.clone(), b"table");
    backend.fail_delete(listed_object.clone());
    let service = QuarantineService::new(&backend);

    let report = service
        .purge_quarantine(purge_request(&service, branch_id, QuarantineGate::Safe))
        .expect("purge report");

    assert!(report.deleted().is_empty());
    assert_eq!(report.failed().len(), 1);
    assert_eq!(report.retained_entries().len(), 1);
    assert!(backend.contains(&listed_object));
    assert_eq!(
        service
            .load_required_inventory(branch_id, DATABASE_ID, CODEC_ID)
            .expect("load rewritten inventory")
            .inventory()
            .entries()
            .len(),
        1
    );
}

#[test]
fn purge_multiple_delete_failures_are_sorted_and_keep_only_failed_entries() {
    let branch_id = branch_id();
    let object1 = quarantine_object(branch_id, "table0001");
    let object2 = quarantine_object(branch_id, "table0002");
    let object3 = quarantine_object(branch_id, "table0003");
    let entry1 = QuarantineEntry::new(
        "table0001",
        table_source_object("table0001"),
        1,
        Timestamp::from_micros(1),
    )
    .expect("entry 1");
    let entry2 = QuarantineEntry::new(
        "table0002",
        table_source_object("table0002"),
        2,
        Timestamp::from_micros(2),
    )
    .expect("entry 2");
    let entry3 = QuarantineEntry::new(
        "table0003",
        table_source_object("table0003"),
        3,
        Timestamp::from_micros(3),
    )
    .expect("entry 3");
    let inventory = inventory(branch_id, vec![entry3, entry1, entry2]);
    let backend = MutationBackend::durable()
        .with_object(inventory_object(branch_id), &encode_inventory(&inventory))
        .with_object(object1.clone(), b"one")
        .with_object(object2.clone(), b"two")
        .with_object(object3.clone(), b"three");
    backend.fail_delete(object2.clone());
    backend.fail_delete(object3.clone());
    let service = QuarantineService::new(&backend);

    let report = service
        .purge_quarantine(purge_request(&service, branch_id, QuarantineGate::Safe))
        .expect("purge report");

    assert_eq!(
        report
            .deleted()
            .iter()
            .map(QuarantineDeleteOutcome::object)
            .collect::<Vec<_>>(),
        vec![&object1]
    );
    assert_eq!(
        report
            .failed()
            .iter()
            .map(QuarantineDeleteOutcome::object)
            .collect::<Vec<_>>(),
        vec![&object2, &object3]
    );
    assert_eq!(
        report
            .retained_entries()
            .iter()
            .map(QuarantineEntry::object_id)
            .collect::<Vec<_>>(),
        vec!["table0002", "table0003"]
    );
    assert!(!backend.contains(&object1));
    assert!(backend.contains(&object2));
    assert!(backend.contains(&object3));
    assert_eq!(
        service
            .load_required_inventory(branch_id, DATABASE_ID, CODEC_ID)
            .expect("load rewritten inventory")
            .inventory()
            .entries()
            .iter()
            .map(QuarantineEntry::object_id)
            .collect::<Vec<_>>(),
        vec!["table0002", "table0003"]
    );
}

#[test]
fn purge_missing_object_is_reported_and_removed_from_inventory() {
    let branch_id = branch_id();
    let source_object = source_object();
    let listed_object = quarantine_object(branch_id, "table0002");
    let entry = QuarantineEntry::new("table0002", source_object, 5, Timestamp::from_micros(2))
        .expect("entry");
    let inventory = inventory(branch_id, vec![entry]);
    let backend = MutationBackend::durable()
        .with_object(inventory_object(branch_id), &encode_inventory(&inventory));
    let service = QuarantineService::new(&backend);

    let report = service
        .purge_quarantine(purge_request(&service, branch_id, QuarantineGate::Safe))
        .expect("purge report");

    assert!(report.deleted().is_empty());
    assert_eq!(report.already_missing().len(), 1);
    assert_eq!(report.already_missing()[0].object(), &listed_object);
    assert!(report.already_missing()[0].already_missing());
    assert!(report.retained_entries().is_empty());
    assert!(report
        .inventory_write()
        .expect("inventory rewrite")
        .inventory()
        .is_empty());
    assert!(service
        .load_required_inventory(branch_id, DATABASE_ID, CODEC_ID)
        .expect("load rewritten inventory")
        .inventory()
        .is_empty());
}

#[test]
fn purge_multiple_missing_objects_are_sorted_and_removed_from_inventory() {
    let branch_id = branch_id();
    let object1 = quarantine_object(branch_id, "table0001");
    let object2 = quarantine_object(branch_id, "table0002");
    let object3 = quarantine_object(branch_id, "table0003");
    let entry1 = QuarantineEntry::new(
        "table0001",
        table_source_object("table0001"),
        1,
        Timestamp::from_micros(1),
    )
    .expect("entry 1");
    let entry2 = QuarantineEntry::new(
        "table0002",
        table_source_object("table0002"),
        2,
        Timestamp::from_micros(2),
    )
    .expect("entry 2");
    let entry3 = QuarantineEntry::new(
        "table0003",
        table_source_object("table0003"),
        3,
        Timestamp::from_micros(3),
    )
    .expect("entry 3");
    let inventory = inventory(branch_id, vec![entry3, entry1, entry2]);
    let backend = MutationBackend::durable()
        .with_object(inventory_object(branch_id), &encode_inventory(&inventory));
    let service = QuarantineService::new(&backend);

    let report = service
        .purge_quarantine(purge_request(&service, branch_id, QuarantineGate::Safe))
        .expect("purge report");

    assert!(report.deleted().is_empty());
    assert!(report.failed().is_empty());
    assert_eq!(
        report
            .already_missing()
            .iter()
            .map(QuarantineDeleteOutcome::object)
            .collect::<Vec<_>>(),
        vec![&object1, &object2, &object3]
    );
    assert!(report.retained_entries().is_empty());
    assert!(service
        .load_required_inventory(branch_id, DATABASE_ID, CODEC_ID)
        .expect("load rewritten inventory")
        .inventory()
        .is_empty());
}

#[test]
fn purge_inventory_rewrite_failure_preserves_delete_report() {
    let branch_id = branch_id();
    for (kind, visible, expected_visible_entries) in [
        (PublishFailureKind::Unsupported, false, 1),
        (PublishFailureKind::PreconditionFailed, false, 1),
        (PublishFailureKind::FailedBeforeVisibility, false, 1),
        (PublishFailureKind::VisibilityUnknown, false, 1),
        (PublishFailureKind::VisibilityUnknown, true, 0),
        (PublishFailureKind::VisibleDurabilityUnconfirmed, true, 0),
    ] {
        let source_object = source_object();
        let listed_object = quarantine_object(branch_id, "table0002");
        let inventory_object = inventory_object(branch_id);
        let entry = QuarantineEntry::new("table0002", source_object, 5, Timestamp::from_micros(2))
            .expect("entry");
        let inventory = inventory(branch_id, vec![entry]);
        let backend = MutationBackend::durable()
            .with_object(inventory_object.clone(), &encode_inventory(&inventory))
            .with_object(listed_object.clone(), b"table");
        backend.fail_publish(inventory_object.clone(), kind, visible);
        let service = QuarantineService::new(&backend);

        let report = service
            .purge_quarantine(purge_request(&service, branch_id, QuarantineGate::Safe))
            .expect("purge report");

        assert_eq!(report.deleted().len(), 1);
        assert_eq!(report.deleted()[0].object(), &listed_object);
        let failure = report.inventory_publish_failure().expect("publish failure");
        assert_eq!(failure.object(), &inventory_object);
        assert_eq!(failure.source().kind(), kind);
        assert!(!backend.contains(&listed_object));
        assert_eq!(
            service
                .load_required_inventory(branch_id, DATABASE_ID, CODEC_ID)
                .expect("load inventory after failed rewrite")
                .inventory()
                .entries()
                .len(),
            expected_visible_entries
        );
    }
}
