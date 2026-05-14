use super::{SnapshotObject, SnapshotService, SnapshotServiceError};
use crate::backend::{
    memory::MemoryBackend, Backend, BackendCapabilities, BackendCapability, BackendError,
    BackendErrorKind, BackendMetadata, BackendRange, BackendResult,
    BASIC_OBJECT_BACKEND_CAPABILITIES,
};
use crate::layout::{ObjectFamily, ObjectLayout};
use crate::object::{ObjectName, ObjectPrefix};
use std::collections::BTreeSet;
use std::sync::Mutex;

#[test]
fn snapshot_list_returns_empty_when_family_is_absent() {
    let backend = MemoryBackend::new();
    let service = SnapshotService::new(&backend);

    let snapshots = service.list_snapshots().expect("list snapshots");

    assert!(snapshots.is_empty());
}

#[test]
fn snapshot_list_sorts_by_numeric_snapshot_id() {
    let backend = MemoryBackend::new();
    write_placeholder_snapshot(&backend, 10);
    write_placeholder_snapshot(&backend, 1);
    write_placeholder_snapshot(&backend, 3);
    let service = SnapshotService::new(&backend);

    let snapshots = service.list_snapshots().expect("list snapshots");

    assert_snapshot_ids(&snapshots, &[1, 3, 10]);
}

#[test]
fn latest_snapshot_returns_highest_listed_snapshot_id() {
    let backend = MemoryBackend::new();
    write_placeholder_snapshot(&backend, 2);
    write_placeholder_snapshot(&backend, 9);
    write_placeholder_snapshot(&backend, 4);
    let service = SnapshotService::new(&backend);

    let latest = service
        .latest_snapshot()
        .expect("latest snapshot")
        .expect("snapshot exists");

    assert_eq!(latest.snapshot_id(), 9);
    assert_eq!(latest.object(), &snapshot_object(9));
}

#[test]
fn latest_snapshot_returns_none_when_family_is_absent() {
    let backend = MemoryBackend::new();
    let service = SnapshotService::new(&backend);

    let latest = service.latest_snapshot().expect("latest snapshot");

    assert!(latest.is_none());
}

#[test]
fn snapshot_list_rejects_malformed_snapshot_family_objects() {
    let malformed = [
        snapshot_raw("000000000000000A"),
        snapshot_raw("0001"),
        snapshot_raw("00000000000000001"),
        snapshot_raw("000000000000000g"),
        format!(
            "{}/{}/part",
            ObjectFamily::Snapshots.as_str(),
            "0000000000000001"
        ),
        snapshot_raw("0000000000000000"),
    ];

    for raw in malformed {
        let object = object_name(raw);
        let backend = ListingBackend::with_names(vec![object.clone()]);
        let service = SnapshotService::new(&backend);

        let error = service
            .list_snapshots()
            .expect_err("malformed snapshot object rejected");

        match error {
            SnapshotServiceError::InvalidListedObject {
                object: actual,
                source,
            } => {
                assert_eq!(actual, object);
                assert_eq!(source.kind(), BackendErrorKind::InvalidObjectName);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}

#[test]
fn snapshot_list_ignores_objects_outside_snapshot_family() {
    let backend = ListingBackend::with_names(vec![
        object_name(format!(
            "{}2/{}",
            ObjectFamily::Snapshots.as_str(),
            "0000000000000001"
        )),
        family_object(ObjectFamily::Wal, "0000000000000001"),
        snapshot_object(2),
    ]);
    let service = SnapshotService::new(&backend);

    let snapshots = service.list_snapshots().expect("list snapshots");

    assert_snapshot_ids(&snapshots, &[2]);
}

#[test]
fn snapshot_list_failure_returns_typed_error_without_reads() {
    let backend = ListingBackend::with_list_error(BackendError::new(
        BackendErrorKind::Unavailable,
        "list failed",
    ));
    let service = SnapshotService::new(&backend);

    let error = service.list_snapshots().expect_err("list failure");

    match error {
        SnapshotServiceError::List { source } => {
            assert_eq!(source.kind(), BackendErrorKind::Unavailable);
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(backend.read_count(), 0);
}

#[test]
fn prune_snapshots_list_failure_returns_typed_error_without_deletes() {
    let backend = ListingBackend::with_list_error(BackendError::new(
        BackendErrorKind::Unavailable,
        "list failed",
    ));
    let service = SnapshotService::new(&backend);

    let error = service.prune_snapshots(None, 1).expect_err("list failure");

    match error {
        SnapshotServiceError::List { source } => {
            assert_eq!(source.kind(), BackendErrorKind::Unavailable);
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert!(backend.deleted_objects().is_empty());
}

#[test]
fn prune_snapshots_rejects_malformed_snapshot_family_object_before_delete() {
    let malformed = object_name(snapshot_raw("000000000000000g"));
    let backend = ListingBackend::with_names(vec![snapshot_object(1), malformed.clone()]);
    let service = SnapshotService::new(&backend);

    let error = service
        .prune_snapshots(None, 1)
        .expect_err("malformed snapshot object rejected");

    match error {
        SnapshotServiceError::InvalidListedObject { object, source } => {
            assert_eq!(object, malformed);
            assert_eq!(source.kind(), BackendErrorKind::InvalidObjectName);
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert!(backend.deleted_objects().is_empty());
}

#[test]
fn prune_snapshots_protects_live_snapshot_and_newest_retained() {
    let backend = MemoryBackend::new();
    for snapshot_id in 1..=5 {
        write_placeholder_snapshot(&backend, snapshot_id);
    }
    let service = SnapshotService::new(&backend);

    let report = service
        .prune_snapshots(Some(2), 2)
        .expect("prune snapshots");

    assert_snapshot_ids(report.deleted(), &[1, 3]);
    assert_snapshot_ids(report.protected(), &[2, 4, 5]);
    assert!(report.failed().is_empty());
    assert_missing(&backend, 1);
    assert_missing(&backend, 3);
    assert_present(&backend, 2);
    assert_present(&backend, 4);
    assert_present(&backend, 5);
}

#[test]
fn prune_snapshots_clamps_retain_newest_to_one() {
    let backend = MemoryBackend::new();
    write_placeholder_snapshot(&backend, 1);
    write_placeholder_snapshot(&backend, 2);
    let service = SnapshotService::new(&backend);

    let report = service.prune_snapshots(None, 0).expect("prune snapshots");

    assert_snapshot_ids(report.deleted(), &[1]);
    assert_snapshot_ids(report.protected(), &[2]);
    assert!(report.failed().is_empty());
}

#[test]
fn prune_snapshots_reports_delete_failures_without_hiding_successes() {
    let failing = snapshot_object(1);
    let backend = ListingBackend::with_delete_failures(
        vec![failing.clone(), snapshot_object(2), snapshot_object(3)],
        vec![failing],
    );
    let service = SnapshotService::new(&backend);

    let report = service.prune_snapshots(None, 1).expect("prune snapshots");

    assert_snapshot_ids(report.deleted(), &[2]);
    assert_snapshot_ids(report.protected(), &[3]);
    assert_eq!(report.failed().len(), 1);
    assert_eq!(report.failed()[0].snapshot().snapshot_id(), 1);
    assert_eq!(
        report.failed()[0].source().kind(),
        BackendErrorKind::Unavailable
    );
    assert_eq!(backend.deleted_objects(), vec![snapshot_object(2)]);
}

#[test]
fn prune_snapshots_rejects_zero_live_snapshot_before_listing() {
    let backend = ListingBackend::with_names(vec![snapshot_object(1)]);
    let service = SnapshotService::new(&backend);

    let error = service
        .prune_snapshots(Some(0), 1)
        .expect_err("zero live snapshot rejected");

    match error {
        SnapshotServiceError::InvalidSnapshotFact { snapshot_id, field } => {
            assert_eq!(snapshot_id, 0);
            assert_eq!(field, "snapshot_id");
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(backend.list_count(), 0);
}

#[test]
fn prune_snapshots_requires_delete_capability_before_listing() {
    let backend = ListingBackend::without_delete_capability(vec![snapshot_object(1)]);
    let service = SnapshotService::new(&backend);

    let error = service
        .prune_snapshots(None, 1)
        .expect_err("missing delete capability");

    match error {
        SnapshotServiceError::UnsupportedCapability { capability } => {
            assert_eq!(capability, BackendCapability::DeleteObject);
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(backend.list_count(), 0);
}

#[derive(Debug)]
struct ListingBackend {
    names: Vec<ObjectName>,
    list_error: Option<BackendError>,
    delete_failures: BTreeSet<ObjectName>,
    deleted: Mutex<Vec<ObjectName>>,
    reads: Mutex<u64>,
    lists: Mutex<u64>,
    capabilities: BackendCapabilities,
}

impl ListingBackend {
    fn with_names(names: Vec<ObjectName>) -> Self {
        Self::new(
            names,
            None,
            Vec::new(),
            BackendCapabilities::from_slice(BASIC_OBJECT_BACKEND_CAPABILITIES),
        )
    }

    fn with_list_error(source: BackendError) -> Self {
        Self::new(
            Vec::new(),
            Some(source),
            Vec::new(),
            BackendCapabilities::from_slice(BASIC_OBJECT_BACKEND_CAPABILITIES),
        )
    }

    fn with_delete_failures(names: Vec<ObjectName>, delete_failures: Vec<ObjectName>) -> Self {
        Self::new(
            names,
            None,
            delete_failures,
            BackendCapabilities::from_slice(BASIC_OBJECT_BACKEND_CAPABILITIES),
        )
    }

    fn without_delete_capability(names: Vec<ObjectName>) -> Self {
        let capabilities = BackendCapabilities::from_slice(&[
            BackendCapability::ReadObject,
            BackendCapability::ReadRange,
            BackendCapability::WriteObject,
            BackendCapability::ListPrefix,
            BackendCapability::ObjectMetadata,
        ]);
        Self::new(names, None, Vec::new(), capabilities)
    }

    fn new(
        names: Vec<ObjectName>,
        list_error: Option<BackendError>,
        delete_failures: Vec<ObjectName>,
        capabilities: BackendCapabilities,
    ) -> Self {
        Self {
            names,
            list_error,
            delete_failures: delete_failures.into_iter().collect(),
            deleted: Mutex::new(Vec::new()),
            reads: Mutex::new(0),
            lists: Mutex::new(0),
            capabilities,
        }
    }

    fn deleted_objects(&self) -> Vec<ObjectName> {
        self.deleted.lock().expect("deleted lock").clone()
    }

    fn read_count(&self) -> u64 {
        *self.reads.lock().expect("reads lock")
    }

    fn list_count(&self) -> u64 {
        *self.lists.lock().expect("lists lock")
    }
}

impl Backend for ListingBackend {
    fn capabilities(&self) -> BackendCapabilities {
        self.capabilities
    }

    fn read_object(&self, _name: &ObjectName) -> BackendResult<Vec<u8>> {
        *self.reads.lock().expect("reads lock") += 1;
        Err(BackendError::new(BackendErrorKind::NotFound, "not found"))
    }

    fn read_range(&self, _name: &ObjectName, _range: BackendRange) -> BackendResult<Vec<u8>> {
        Err(BackendError::new(
            BackendErrorKind::UnsupportedOperation,
            "read range not implemented",
        ))
    }

    fn write_object(&self, _name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendMetadata> {
        Ok(BackendMetadata::new(bytes.len() as u64, None))
    }

    fn delete_object(&self, name: &ObjectName) -> BackendResult<()> {
        if self.delete_failures.contains(name) {
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "delete failed",
            ));
        }
        self.deleted
            .lock()
            .expect("deleted lock")
            .push(name.clone());
        Ok(())
    }

    fn list_prefix(&self, _prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        *self.lists.lock().expect("lists lock") += 1;
        match &self.list_error {
            Some(source) => Err(source.clone()),
            None => Ok(self.names.clone()),
        }
    }

    fn object_metadata(&self, _name: &ObjectName) -> BackendResult<BackendMetadata> {
        Err(BackendError::new(BackendErrorKind::NotFound, "not found"))
    }
}

fn write_placeholder_snapshot(backend: &MemoryBackend, snapshot_id: u64) {
    backend
        .write_object(&snapshot_object(snapshot_id), b"snapshot")
        .expect("write snapshot placeholder");
}

fn assert_present(backend: &MemoryBackend, snapshot_id: u64) {
    backend
        .read_object(&snapshot_object(snapshot_id))
        .expect("snapshot should remain");
}

fn assert_missing(backend: &MemoryBackend, snapshot_id: u64) {
    let error = backend
        .read_object(&snapshot_object(snapshot_id))
        .expect_err("snapshot should be deleted");
    assert_eq!(error.kind(), BackendErrorKind::NotFound);
}

fn assert_snapshot_ids(snapshots: &[SnapshotObject], expected: &[u64]) {
    let actual: Vec<u64> = snapshots.iter().map(SnapshotObject::snapshot_id).collect();
    assert_eq!(actual, expected);
}

fn snapshot_object(snapshot_id: u64) -> ObjectName {
    ObjectLayout::snapshot(snapshot_id).expect("snapshot object")
}

fn object_name(raw: impl Into<String>) -> ObjectName {
    ObjectName::new(raw).expect("object name")
}

fn snapshot_raw(component: &str) -> String {
    format!("{}/{}", ObjectFamily::Snapshots.as_str(), component)
}

fn family_object(family: ObjectFamily, component: &str) -> ObjectName {
    object_name(format!("{}/{}", family.as_str(), component))
}
