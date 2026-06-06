use super::*;

#[derive(Debug)]
struct PublishPreflightBackend {
    capabilities: BackendCapabilities,
}

impl PublishPreflightBackend {
    const fn new(capabilities: BackendCapabilities) -> Self {
        Self { capabilities }
    }
}

impl Backend for PublishPreflightBackend {
    fn capabilities(&self) -> BackendCapabilities {
        self.capabilities
    }

    fn read_object(&self, _name: &ObjectName) -> BackendResult<Vec<u8>> {
        panic!("read_object should not be called by inventory publish preflight")
    }

    fn read_range(&self, _name: &ObjectName, _range: BackendRange) -> BackendResult<Vec<u8>> {
        panic!("read_range should not be called by inventory publish preflight")
    }

    fn write_object(&self, _name: &ObjectName, _bytes: &[u8]) -> BackendResult<BackendMetadata> {
        panic!("write_object should not be called by inventory publish preflight")
    }

    fn delete_object(&self, _name: &ObjectName) -> crate::backend::DeleteResult {
        panic!("delete_object should not be called by inventory publish preflight")
    }

    fn list_prefix(&self, _prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        panic!("list_prefix should not be called by inventory publish preflight")
    }

    fn object_metadata(&self, _name: &ObjectName) -> BackendResult<BackendMetadata> {
        panic!("object_metadata should not be called by inventory publish preflight")
    }

    fn publish_object(
        &self,
        _name: &ObjectName,
        _bytes: &[u8],
        _mode: PublishMode,
    ) -> Result<PublishOutcome, PublishError> {
        panic!("publish_object should not be called when durable capability preflight fails")
    }
}

fn assert_decode_error(error: QuarantineServiceError, object: &ObjectName) {
    match error {
        QuarantineServiceError::Decode {
            object: actual,
            source,
        } => {
            assert_eq!(&actual, object);
            assert!(matches!(source, FormatError::InsufficientBytes { .. }));
        }
        other => panic!("expected decode error, got {other:?}"),
    }
}

fn assert_decode_invalid_value(
    error: QuarantineServiceError,
    object: &ObjectName,
    expected_field: &'static str,
) {
    match error {
        QuarantineServiceError::Decode {
            object: actual,
            source: FormatError::InvalidValue { field },
        } => {
            assert_eq!(&actual, object);
            assert_eq!(field, expected_field);
        }
        other => panic!("expected invalid-value decode error, got {other:?}"),
    }
}

fn inventory_with_entry(
    branch_id: BranchId,
    object_id: impl Into<String>,
    source_object: ObjectName,
) -> QuarantineInventory {
    inventory(
        branch_id,
        vec![QuarantineEntry::new(
            object_id,
            source_object,
            128,
            Timestamp::from_micros(1_700_000_000_000_000),
        )
        .expect("quarantine entry")],
    )
}

fn assert_unsupported_publish_capability(
    error: QuarantineServiceError,
    object: &ObjectName,
    capability: BackendCapability,
) {
    match error {
        QuarantineServiceError::Publish {
            object: actual,
            source,
        } => {
            assert_eq!(&actual, object);
            assert_eq!(source.object(), object);
            assert_eq!(source.kind(), PublishFailureKind::Unsupported);
            assert_eq!(
                source.source_error().kind(),
                BackendErrorKind::UnsupportedOperation
            );
            assert!(source
                .source_error()
                .to_string()
                .contains(&capability.to_string()));
        }
        other => panic!("expected publish error, got {other:?}"),
    }
}

#[test]
fn optional_inventory_load_distinguishes_absent_from_synthesized_empty() {
    let backend = MemoryBackend::new();
    let service = QuarantineService::new(&backend);
    let branch_id = branch_id();
    let object = inventory_object(branch_id);

    let optional = service
        .load_optional_inventory(branch_id, DATABASE_ID, CODEC_ID)
        .expect("optional load");
    assert!(optional.is_none());

    assert_eq!(
        service.load_required_inventory(branch_id, DATABASE_ID, CODEC_ID),
        Err(QuarantineServiceError::Missing {
            object: object.clone()
        })
    );

    let load = service
        .load_inventory(branch_id, DATABASE_ID, CODEC_ID)
        .expect("absent inventory loads as empty");
    assert_empty_load(&load, branch_id, &object);
}

#[test]
fn corrupt_inventory_bytes_are_never_treated_as_empty() {
    let branch_id = branch_id();
    let object = inventory_object(branch_id);
    let backend = RecordingBackend::with_object(object.clone(), b"not-an-inventory".to_vec());
    let service = QuarantineService::new(&backend);

    assert_decode_error(
        service
            .load_optional_inventory(branch_id, DATABASE_ID, CODEC_ID)
            .expect_err("optional load must fail closed"),
        &object,
    );
    assert_decode_error(
        service
            .load_inventory(branch_id, DATABASE_ID, CODEC_ID)
            .expect_err("default load must not synthesize empty on corrupt bytes"),
        &object,
    );
    assert_decode_error(
        service
            .load_required_inventory(branch_id, DATABASE_ID, CODEC_ID)
            .expect_err("required load must preserve corruption"),
        &object,
    );
}

#[test]
fn inventory_load_rejects_layout_invalid_entries_after_decode() {
    let branch_id = branch_id();
    let object = inventory_object(branch_id);

    let cases = [
        (
            "overlong assembled quarantine path",
            inventory_with_entry(branch_id, "a".repeat(980), table_source_object("table0001")),
            "object_id",
        ),
        (
            "quarantine source family",
            inventory_with_entry(
                branch_id,
                "table0001",
                ObjectLayout::quarantine_object(&branch_id.to_string(), "table0001")
                    .expect("quarantine source object"),
            ),
            "source_object",
        ),
        (
            "unknown source family",
            inventory_with_entry(
                branch_id,
                "table0001",
                ObjectName::new("unknown/table0001").expect("unknown source object"),
            ),
            "source_object",
        ),
    ];

    for (name, inventory, field) in cases {
        let backend = RecordingBackend::with_object(object.clone(), encode_inventory(&inventory));
        let service = QuarantineService::new(&backend);
        let error = service
            .load_required_inventory(branch_id, DATABASE_ID, CODEC_ID)
            .expect_err(name);

        assert_decode_invalid_value(error, &object, field);
    }
}

#[test]
fn inventory_publish_rejects_layout_invalid_entries_before_backend_publish() {
    let branch_id = branch_id();
    let object = inventory_object(branch_id);
    let backend = RecordingBackend::new();
    let service = QuarantineService::new(&backend);
    let inventory = inventory_with_entry(
        branch_id,
        "table0001",
        ObjectName::new("unknown/table0001").expect("unknown source object"),
    );

    match service
        .publish_inventory_replace(&inventory)
        .expect_err("layout-invalid inventory should fail before publish")
    {
        QuarantineServiceError::Encode {
            object: actual,
            source: FormatError::InvalidValue { field },
        } => {
            assert_eq!(actual, object);
            assert_eq!(field, "source_object");
        }
        other => panic!("expected encode invalid-value error, got {other:?}"),
    }
    assert!(backend.objects.lock().expect("objects").is_empty());
}

#[test]
fn inventory_publish_requires_each_durable_capability_before_backend_publish() {
    let branch_id = branch_id();
    let inventory = inventory(branch_id, Vec::new());
    let object = inventory_object(branch_id);

    for (capability, capabilities) in [
        (
            BackendCapability::DurablePublish,
            BackendCapabilities::from_slice(&[BackendCapability::DurableSync]),
        ),
        (
            BackendCapability::DurableSync,
            BackendCapabilities::from_slice(&[BackendCapability::DurablePublish]),
        ),
    ] {
        let backend = PublishPreflightBackend::new(capabilities);
        let service = QuarantineService::new(&backend);

        let error = service
            .publish_inventory_replace(&inventory)
            .expect_err("durable capability preflight should fail");

        assert_unsupported_publish_capability(error, &object, capability);
    }
}

#[test]
fn visibility_unknown_inventory_publish_returns_error_without_visible_replacement() {
    let branch_id = branch_id();
    let object = inventory_object(branch_id);
    let old_inventory = inventory(branch_id, Vec::new());
    let old_bytes = encode_inventory(&old_inventory);
    let new_inventory = one_entry_inventory(branch_id);
    let backend = PublishFailureBackend::with_object(
        PublishFailureKind::VisibilityUnknown,
        object.clone(),
        &old_bytes,
    );
    let service = QuarantineService::new(&backend);

    let error = service
        .publish_inventory_replace(&new_inventory)
        .expect_err("visibility-unknown publish must not return write facts");

    assert_publish_error(error, &object, PublishFailureKind::VisibilityUnknown);
    assert_eq!(backend.stored_bytes(&object), old_bytes);
    assert_eq!(
        service
            .load_required_inventory(branch_id, DATABASE_ID, CODEC_ID)
            .expect("load still-visible old inventory")
            .inventory(),
        &old_inventory
    );
}

#[test]
fn visibility_unknown_inventory_publish_may_leave_replacement_visible_without_write_facts() {
    let branch_id = branch_id();
    let object = inventory_object(branch_id);
    let old_inventory = inventory(branch_id, Vec::new());
    let old_bytes = encode_inventory(&old_inventory);
    let new_inventory = one_entry_inventory(branch_id);
    let new_bytes = encode_inventory(&new_inventory);
    let backend = PublishFailureBackend::visible_after_replace(
        PublishFailureKind::VisibilityUnknown,
        object.clone(),
        &old_bytes,
    );
    let service = QuarantineService::new(&backend);

    let error = service
        .publish_inventory_replace(&new_inventory)
        .expect_err("visibility-unknown publish must not return write facts");

    assert_publish_error(error, &object, PublishFailureKind::VisibilityUnknown);
    assert_eq!(backend.stored_bytes(&object), new_bytes);
    assert_eq!(
        service
            .load_required_inventory(branch_id, DATABASE_ID, CODEC_ID)
            .expect("load visible replacement inventory")
            .inventory(),
        &new_inventory
    );
}
