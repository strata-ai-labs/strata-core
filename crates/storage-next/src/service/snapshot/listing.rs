use super::{
    require_capability, validate_snapshot_id, SnapshotService, SnapshotServiceError,
    SnapshotServiceResult,
};
use crate::backend::{Backend, BackendCapability, BackendError, BackendErrorKind};
use crate::layout::{ObjectLayout, SnapshotObjectClassification};
use crate::object::ObjectName;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SnapshotObject {
    snapshot_id: u64,
    object: ObjectName,
}

impl SnapshotObject {
    const fn new(snapshot_id: u64, object: ObjectName) -> Self {
        Self {
            snapshot_id,
            object,
        }
    }

    pub(crate) const fn snapshot_id(&self) -> u64 {
        self.snapshot_id
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SnapshotDeleteFailure {
    snapshot: SnapshotObject,
    source: BackendError,
}

impl SnapshotDeleteFailure {
    const fn new(snapshot: SnapshotObject, source: BackendError) -> Self {
        Self { snapshot, source }
    }

    pub(crate) const fn snapshot(&self) -> &SnapshotObject {
        &self.snapshot
    }

    pub(crate) const fn source(&self) -> &BackendError {
        &self.source
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct SnapshotDeleteReport {
    deleted: Vec<SnapshotObject>,
    protected: Vec<SnapshotObject>,
    failed: Vec<SnapshotDeleteFailure>,
}

impl SnapshotDeleteReport {
    fn record_deleted(&mut self, snapshot: SnapshotObject) {
        self.deleted.push(snapshot);
    }

    fn record_protected(&mut self, snapshot: SnapshotObject) {
        self.protected.push(snapshot);
    }

    fn record_failed(&mut self, snapshot: SnapshotObject, source: BackendError) {
        self.failed
            .push(SnapshotDeleteFailure::new(snapshot, source));
    }

    pub(crate) fn deleted(&self) -> &[SnapshotObject] {
        &self.deleted
    }

    pub(crate) fn protected(&self) -> &[SnapshotObject] {
        &self.protected
    }

    pub(crate) fn failed(&self) -> &[SnapshotDeleteFailure] {
        &self.failed
    }
}

impl SnapshotService<'_> {
    pub(crate) fn list_snapshots(&self) -> SnapshotServiceResult<Vec<SnapshotObject>> {
        list_snapshot_objects(&self.backend)
    }

    pub(crate) fn latest_snapshot(&self) -> SnapshotServiceResult<Option<SnapshotObject>> {
        Ok(self.list_snapshots()?.into_iter().next_back())
    }

    pub(crate) fn prune_snapshots(
        &self,
        live_snapshot_id: Option<u64>,
        retain_newest: usize,
    ) -> SnapshotServiceResult<SnapshotDeleteReport> {
        if let Some(snapshot_id) = live_snapshot_id {
            validate_snapshot_id(snapshot_id)?;
        }
        require_capability(&self.backend, BackendCapability::DeleteObject)?;

        let snapshots = self.list_snapshots()?;
        let retain_newest = retain_newest.max(1);
        let retain_start = snapshots.len().saturating_sub(retain_newest);
        let mut report = SnapshotDeleteReport::default();

        for (index, snapshot) in snapshots.into_iter().enumerate() {
            let newest_retained = index >= retain_start;
            let live = live_snapshot_id == Some(snapshot.snapshot_id());
            if newest_retained || live {
                report.record_protected(snapshot);
                continue;
            }

            // Pruning executes caller-supplied retention intent. A per-object
            // delete failure must not hide deletes that already succeeded or
            // snapshots that were explicitly protected.
            match self.backend.delete_object(snapshot.object()) {
                Ok(_) => report.record_deleted(snapshot),
                Err(source) => report.record_failed(snapshot, source.into()),
            }
        }

        Ok(report)
    }
}

fn snapshot_prefix() -> SnapshotServiceResult<crate::object::ObjectPrefix> {
    ObjectLayout::snapshot_prefix().map_err(|source| SnapshotServiceError::Layout { source })
}

fn list_snapshot_objects(backend: &dyn Backend) -> SnapshotServiceResult<Vec<SnapshotObject>> {
    require_capability(backend, BackendCapability::ListPrefix)?;
    let prefix = snapshot_prefix()?;
    let mut snapshots = Vec::new();
    for object in backend
        .list_prefix(&prefix)
        .map_err(|source| SnapshotServiceError::List { source })?
    {
        if let Some(snapshot) = parse_snapshot_object(object)? {
            snapshots.push(snapshot);
        }
    }
    snapshots.sort_by_key(SnapshotObject::snapshot_id);
    Ok(snapshots)
}

fn parse_snapshot_object(object: ObjectName) -> SnapshotServiceResult<Option<SnapshotObject>> {
    let snapshot_id = match ObjectLayout::classify_snapshot_object(&object) {
        Ok(Some(SnapshotObjectClassification::Snapshot { snapshot_id })) => snapshot_id,
        Ok(None) => {
            // Some object stores expose weak prefix behavior. Objects outside
            // the snapshot family are ignored, but malformed names inside the
            // family fail closed because they can represent ambiguous recovery
            // state.
            return Ok(None);
        }
        Err(_) => return Err(invalid_listed_snapshot_object(object)),
    };
    if snapshot_id == 0 {
        return Err(invalid_listed_snapshot_object(object));
    }
    Ok(Some(SnapshotObject::new(snapshot_id, object)))
}

fn invalid_listed_snapshot_object(object: ObjectName) -> SnapshotServiceError {
    SnapshotServiceError::InvalidListedObject {
        object,
        source: BackendError::new(
            BackendErrorKind::InvalidObjectName,
            "invalid snapshot object name in listing",
        ),
    }
}
