use super::{SnapshotDeleteReport, SnapshotObject, SnapshotService, SnapshotServiceError};
use crate::backend::{
    Backend, BackendCapabilities, BackendCapability, BackendError, BackendErrorKind,
    BackendMetadata, BackendRange, BackendResult, PublishDurability, PublishError,
    PublishFailureKind, PublishMode, PublishOutcome,
};
use crate::format::{SegmentMetadata, SnapshotSection};
use crate::layout::{ObjectFamily, ObjectLayout};
use crate::object::{ObjectName, ObjectPrefix};
use crate::service::{WalSegmentMetadataSidecarLoad, WalSegmentMetadataSidecarService};
use proptest::collection::vec;
use proptest::prelude::{Just, Strategy};
use proptest::test_runner::{
    Config, FileFailurePersistence, TestCaseError, TestCaseResult, TestRunner,
};
use proptest::{prop_assert, prop_assert_eq, prop_oneof};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use strata_core_next::{CommitVersion, Timestamp};

const DATABASE_ID: [u8; 16] = [0x7a; 16];
const CODEC_ID: &str = "identity";

#[derive(Clone, Copy, Debug)]
enum SnapshotOperation {
    CreateSnapshot {
        snapshot_id: u64,
    },
    CreateAdjacentFamilyObject {
        suffix: u64,
    },
    LoadSnapshot {
        snapshot_id: u64,
    },
    ListSnapshots,
    LatestSnapshot,
    PruneSnapshots {
        live_snapshot_id: Option<u64>,
        retain_newest: usize,
    },
    PublishSidecar {
        segment_id: u64,
    },
    CorruptSidecar {
        segment_id: u64,
    },
    LoadSidecar {
        segment_id: u64,
    },
}

#[derive(Debug, Default)]
struct SnapshotListingModel {
    visible_snapshots: BTreeSet<u64>,
    adjacent_objects: BTreeSet<ObjectName>,
    sidecars: BTreeMap<u64, SidecarState>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SidecarState {
    Present(SegmentMetadata),
    Corrupt,
}

#[derive(Debug, Default)]
struct StoredObjectBackend {
    objects: Mutex<BTreeMap<ObjectName, Vec<u8>>>,
}

impl StoredObjectBackend {
    fn insert(&self, name: ObjectName, bytes: Vec<u8>) {
        self.objects
            .lock()
            .expect("object lock")
            .insert(name, bytes);
    }
}

impl Backend for StoredObjectBackend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::from_slice(&[
            BackendCapability::ReadObject,
            BackendCapability::ReadRange,
            BackendCapability::WriteObject,
            BackendCapability::DeleteObject,
            BackendCapability::ListPrefix,
            BackendCapability::ObjectMetadata,
            BackendCapability::DurablePublish,
            BackendCapability::DurableSync,
        ])
    }

    fn read_object(&self, name: &ObjectName) -> BackendResult<Vec<u8>> {
        self.objects
            .lock()
            .expect("object lock")
            .get(name)
            .cloned()
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "not found"))
    }

    fn read_range(&self, name: &ObjectName, range: BackendRange) -> BackendResult<Vec<u8>> {
        let bytes = self.read_object(name)?;
        let start = usize::try_from(range.offset())
            .map_err(|_| BackendError::new(BackendErrorKind::InvalidRange, "range overflow"))?;
        let end =
            usize::try_from(range.end_offset().ok_or_else(|| {
                BackendError::new(BackendErrorKind::InvalidRange, "range overflow")
            })?)
            .map_err(|_| BackendError::new(BackendErrorKind::InvalidRange, "range overflow"))?;
        if start > bytes.len() {
            return Ok(Vec::new());
        }
        Ok(bytes[start..bytes.len().min(end)].to_vec())
    }

    fn write_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendMetadata> {
        self.insert(name.clone(), bytes.to_vec());
        Ok(BackendMetadata::new(bytes.len() as u64, None))
    }

    fn delete_object(&self, name: &ObjectName) -> BackendResult<()> {
        self.objects
            .lock()
            .expect("object lock")
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "not found"))
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        Ok(self
            .objects
            .lock()
            .expect("object lock")
            .keys()
            .filter(|name| name.as_str().starts_with(prefix.as_str()))
            .cloned()
            .collect())
    }

    fn object_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
        self.objects
            .lock()
            .expect("object lock")
            .get(name)
            .map(|bytes| BackendMetadata::new(bytes.len() as u64, None))
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "not found"))
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        bytes: &[u8],
        mode: PublishMode,
    ) -> Result<PublishOutcome, PublishError> {
        let mut objects = self.objects.lock().expect("object lock");
        if mode == PublishMode::Create && objects.contains_key(name) {
            return Err(PublishError::precondition_failed(
                name,
                "object already exists",
            ));
        }
        objects.insert(name.clone(), bytes.to_vec());
        Ok(PublishOutcome::new(
            name.clone(),
            BackendMetadata::new(bytes.len() as u64, None),
            PublishDurability::Durable,
        ))
    }
}

#[test]
fn snapshot_service_state_machine_matches_model() {
    let mut runner = TestRunner::new(Config {
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/snapshot_service_state_machine.txt",
        ))),
        ..Config::default()
    });
    let strategy = vec(snapshot_operation_strategy(), 1..=96);

    runner
        .run(&strategy, run_snapshot_listing_sequence)
        .expect("snapshot service state machine should match model");
}

fn snapshot_operation_strategy() -> impl Strategy<Value = SnapshotOperation> {
    prop_oneof![
        (1_u64..=16).prop_map(|snapshot_id| SnapshotOperation::CreateSnapshot { snapshot_id }),
        (1_u64..=8).prop_map(|suffix| SnapshotOperation::CreateAdjacentFamilyObject { suffix }),
        (1_u64..=16).prop_map(|snapshot_id| SnapshotOperation::LoadSnapshot { snapshot_id }),
        Just(SnapshotOperation::ListSnapshots),
        Just(SnapshotOperation::LatestSnapshot),
        (proptest::option::of(0_u64..=16), 0_usize..=5).prop_map(
            |(live_snapshot_id, retain_newest)| SnapshotOperation::PruneSnapshots {
                live_snapshot_id,
                retain_newest,
            },
        ),
        (1_u64..=8).prop_map(|segment_id| SnapshotOperation::PublishSidecar { segment_id }),
        (1_u64..=8).prop_map(|segment_id| SnapshotOperation::CorruptSidecar { segment_id }),
        (1_u64..=8).prop_map(|segment_id| SnapshotOperation::LoadSidecar { segment_id }),
    ]
}

fn run_snapshot_listing_sequence(operations: Vec<SnapshotOperation>) -> TestCaseResult {
    let backend = StoredObjectBackend::default();
    let mut model = SnapshotListingModel::default();

    for operation in operations {
        apply_operation(&backend, &mut model, operation)?;
        assert_model_matches_service(&backend, &model)?;
        assert_adjacent_objects_remain(&backend, &model)?;
    }

    Ok(())
}

fn apply_operation(
    backend: &StoredObjectBackend,
    model: &mut SnapshotListingModel,
    operation: SnapshotOperation,
) -> TestCaseResult {
    match operation {
        SnapshotOperation::CreateSnapshot { snapshot_id } => {
            create_snapshot_and_update_model(backend, model, snapshot_id)?;
        }
        SnapshotOperation::CreateAdjacentFamilyObject { suffix } => {
            let object = adjacent_family_object(suffix);
            backend
                .write_object(&object, b"adjacent")
                .expect("write adjacent object");
            model.adjacent_objects.insert(object);
        }
        SnapshotOperation::ListSnapshots => {
            assert_model_matches_service(backend, model)?;
        }
        SnapshotOperation::LatestSnapshot => {
            assert_latest_matches_model(backend, model)?;
        }
        SnapshotOperation::LoadSnapshot { snapshot_id } => {
            assert_snapshot_load_matches_model(backend, model, snapshot_id)?;
        }
        SnapshotOperation::PruneSnapshots {
            live_snapshot_id,
            retain_newest,
        } => {
            prune_and_update_model(backend, model, live_snapshot_id, retain_newest)?;
        }
        SnapshotOperation::PublishSidecar { segment_id } => {
            publish_sidecar_and_update_model(backend, model, segment_id);
        }
        SnapshotOperation::CorruptSidecar { segment_id } => {
            corrupt_sidecar_and_update_model(backend, model, segment_id);
        }
        SnapshotOperation::LoadSidecar { segment_id } => {
            assert_sidecar_load_matches_model(backend, model, segment_id)?;
        }
    }
    Ok(())
}

fn prune_and_update_model(
    backend: &StoredObjectBackend,
    model: &mut SnapshotListingModel,
    live_snapshot_id: Option<u64>,
    retain_newest: usize,
) -> TestCaseResult {
    let service = SnapshotService::new(backend);
    if live_snapshot_id == Some(0) {
        let error = service
            .prune_snapshots(live_snapshot_id, retain_newest)
            .expect_err("zero live snapshot should be rejected before listing");
        let rejected_zero_live_snapshot = matches!(
            error,
            SnapshotServiceError::InvalidSnapshotFact {
                snapshot_id: 0,
                field: "snapshot_id"
            }
        );
        prop_assert!(rejected_zero_live_snapshot);
        return Ok(());
    }

    let expected = expected_prune_report(&model.visible_snapshots, live_snapshot_id, retain_newest);
    let report = service
        .prune_snapshots(live_snapshot_id, retain_newest)
        .expect("prune snapshots");

    assert_report_matches(&report, &expected)?;
    model.visible_snapshots = expected.protected.into_iter().collect();
    Ok(())
}

fn create_snapshot_and_update_model(
    backend: &StoredObjectBackend,
    model: &mut SnapshotListingModel,
    snapshot_id: u64,
) -> TestCaseResult {
    let service = SnapshotService::new(backend);
    let result = service.publish_create(snapshot_request(snapshot_id));
    if model.visible_snapshots.contains(&snapshot_id) {
        let error = result.expect_err("duplicate snapshot create should fail");
        let duplicate_create_rejected = matches!(
            error,
            SnapshotServiceError::Publish { source, .. }
                if source.kind() == PublishFailureKind::PreconditionFailed
        );
        prop_assert!(duplicate_create_rejected);
    } else {
        result.expect("snapshot create");
        model.visible_snapshots.insert(snapshot_id);
    }
    assert_snapshot_load_matches_model(backend, model, snapshot_id)
}

fn publish_sidecar_and_update_model(
    backend: &StoredObjectBackend,
    model: &mut SnapshotListingModel,
    segment_id: u64,
) {
    let metadata = metadata_for_segment(segment_id);
    WalSegmentMetadataSidecarService::new(backend)
        .publish_replace(&metadata)
        .expect("publish sidecar");
    model
        .sidecars
        .insert(segment_id, SidecarState::Present(metadata));
}

fn corrupt_sidecar_and_update_model(
    backend: &StoredObjectBackend,
    model: &mut SnapshotListingModel,
    segment_id: u64,
) {
    let object = ObjectLayout::wal_segment_metadata(segment_id).expect("sidecar object");
    // Sidecars are cache facts. Corrupt bytes must stay recoverable and must not
    // change the authoritative snapshot listing model.
    backend
        .write_object(&object, b"corrupt sidecar")
        .expect("write corrupt sidecar");
    model.sidecars.insert(segment_id, SidecarState::Corrupt);
}

#[derive(Debug)]
struct ExpectedPruneReport {
    deleted: Vec<u64>,
    protected: Vec<u64>,
}

fn expected_prune_report(
    visible_snapshots: &BTreeSet<u64>,
    live_snapshot_id: Option<u64>,
    retain_newest: usize,
) -> ExpectedPruneReport {
    let snapshot_ids: Vec<u64> = visible_snapshots.iter().copied().collect();
    let retain_newest = retain_newest.max(1);
    let retain_start = snapshot_ids.len().saturating_sub(retain_newest);
    let mut deleted = Vec::new();
    let mut protected = Vec::new();

    for (index, snapshot_id) in snapshot_ids.iter().copied().enumerate() {
        let newest_retained = index >= retain_start;
        let live = live_snapshot_id == Some(snapshot_id);
        if newest_retained || live {
            protected.push(snapshot_id);
        } else {
            deleted.push(snapshot_id);
        }
    }

    ExpectedPruneReport { deleted, protected }
}

fn assert_report_matches(
    report: &SnapshotDeleteReport,
    expected: &ExpectedPruneReport,
) -> TestCaseResult {
    prop_assert_eq!(snapshot_ids(report.deleted()), expected.deleted.clone());
    prop_assert_eq!(snapshot_ids(report.protected()), expected.protected.clone());
    prop_assert!(report.failed().is_empty());
    Ok(())
}

fn assert_model_matches_service(
    backend: &StoredObjectBackend,
    model: &SnapshotListingModel,
) -> TestCaseResult {
    let service = SnapshotService::new(backend);
    let snapshots = service.list_snapshots().expect("list snapshots");
    let expected: Vec<u64> = model.visible_snapshots.iter().copied().collect();

    prop_assert_eq!(snapshot_ids(&snapshots), expected);
    assert_latest_matches_model(backend, model)?;
    Ok(())
}

fn assert_latest_matches_model(
    backend: &StoredObjectBackend,
    model: &SnapshotListingModel,
) -> TestCaseResult {
    let service = SnapshotService::new(backend);
    let latest = service
        .latest_snapshot()
        .expect("latest snapshot")
        .map(|snapshot| snapshot.snapshot_id());
    let expected = model.visible_snapshots.iter().next_back().copied();

    prop_assert_eq!(latest, expected);
    Ok(())
}

fn assert_adjacent_objects_remain(
    backend: &StoredObjectBackend,
    model: &SnapshotListingModel,
) -> TestCaseResult {
    for object in &model.adjacent_objects {
        let bytes = backend
            .read_object(object)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(bytes.as_slice(), b"adjacent");
    }
    Ok(())
}

fn assert_snapshot_load_matches_model(
    backend: &StoredObjectBackend,
    model: &SnapshotListingModel,
    snapshot_id: u64,
) -> TestCaseResult {
    let service = SnapshotService::new(backend);
    let result = service.load_required_for_codec(snapshot_id, DATABASE_ID, CODEC_ID);
    if model.visible_snapshots.contains(&snapshot_id) {
        let snapshot = result.expect("visible snapshot should load");
        let expected_payload = payload_for_snapshot(snapshot_id);
        prop_assert_eq!(snapshot.header().snapshot_id(), snapshot_id);
        prop_assert_eq!(
            snapshot.sections()[0].payload(),
            expected_payload.as_slice()
        );
    } else {
        let error = result.expect_err("missing snapshot should not load");
        let missing_snapshot_reported = matches!(
            error,
            SnapshotServiceError::Missing {
                snapshot_id: missing_id,
                ..
            } if missing_id == snapshot_id
        );
        prop_assert!(missing_snapshot_reported);
    }
    Ok(())
}

fn assert_sidecar_load_matches_model(
    backend: &StoredObjectBackend,
    model: &SnapshotListingModel,
    segment_id: u64,
) -> TestCaseResult {
    let loaded = WalSegmentMetadataSidecarService::new(backend)
        .load(segment_id)
        .expect("sidecar load should be recoverable");
    match model.sidecars.get(&segment_id) {
        Some(SidecarState::Present(expected)) => {
            let WalSegmentMetadataSidecarLoad::Present(sidecar) = loaded else {
                return Err(TestCaseError::fail("expected present sidecar"));
            };
            prop_assert_eq!(sidecar.metadata(), expected);
        }
        Some(SidecarState::Corrupt) => {
            let corrupt_sidecar_reported =
                matches!(loaded, WalSegmentMetadataSidecarLoad::Corrupt { .. });
            prop_assert!(corrupt_sidecar_reported);
        }
        None => {
            let missing_sidecar_reported =
                matches!(loaded, WalSegmentMetadataSidecarLoad::Missing { .. });
            prop_assert!(missing_sidecar_reported);
        }
    }
    Ok(())
}

fn snapshot_request(snapshot_id: u64) -> super::SnapshotPublishRequest {
    super::SnapshotPublishRequest::new(
        snapshot_id,
        CommitVersion::new(snapshot_id),
        Timestamp::from_micros(snapshot_id * 100),
        DATABASE_ID,
        CODEC_ID,
        vec![SnapshotSection::new(0x01, payload_for_snapshot(snapshot_id)).expect("section")],
    )
}

fn payload_for_snapshot(snapshot_id: u64) -> Vec<u8> {
    snapshot_id.to_le_bytes().to_vec()
}

fn adjacent_family_object(suffix: u64) -> ObjectName {
    let adjacent_family = format!("{}2", ObjectFamily::Snapshots.as_str());
    ObjectName::new(format!("{adjacent_family}/{suffix:016x}")).expect("adjacent family object")
}

fn metadata_for_segment(segment_id: u64) -> SegmentMetadata {
    let mut metadata = SegmentMetadata::empty(segment_id);
    metadata.track_record(
        CommitVersion::new(segment_id),
        Timestamp::from_micros(segment_id * 100),
    );
    metadata
}

fn snapshot_ids(snapshots: &[SnapshotObject]) -> Vec<u64> {
    snapshots.iter().map(SnapshotObject::snapshot_id).collect()
}

#[test]
fn zero_live_snapshot_does_not_mutate_visible_snapshots() {
    let backend = StoredObjectBackend::default();
    let mut model = SnapshotListingModel::default();
    create_snapshot_and_update_model(&backend, &mut model, 1).expect("create snapshot");
    create_snapshot_and_update_model(&backend, &mut model, 2).expect("create snapshot");

    prune_and_update_model(&backend, &mut model, Some(0), 0).expect("zero live snapshot rejected");

    assert_eq!(model.visible_snapshots, BTreeSet::from([1, 2]));
    let error = SnapshotService::new(&backend)
        .prune_snapshots(Some(0), 0)
        .expect_err("zero live snapshot rejected");
    assert!(matches!(
        error,
        SnapshotServiceError::InvalidSnapshotFact {
            snapshot_id: 0,
            field: "snapshot_id"
        }
    ));
}

#[test]
fn missing_snapshot_after_model_delete_reports_not_found() {
    let backend = StoredObjectBackend::default();
    let mut model = SnapshotListingModel::default();
    create_snapshot_and_update_model(&backend, &mut model, 1).expect("create snapshot");
    create_snapshot_and_update_model(&backend, &mut model, 2).expect("create snapshot");
    let service = SnapshotService::new(&backend);
    service.prune_snapshots(None, 0).expect("delete snapshot");

    let error = backend
        .read_object(&ObjectLayout::snapshot(1).expect("snapshot object"))
        .expect_err("snapshot should be deleted");

    assert_eq!(error.kind(), BackendErrorKind::NotFound);
}
