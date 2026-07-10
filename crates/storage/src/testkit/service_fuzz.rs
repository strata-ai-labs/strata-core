use crate::backend::{
    Backend, BackendCapabilities, BackendCapability, BackendError, BackendErrorKind,
    BackendMetadata, BackendRange, BackendResult, PublishDurability, PublishError,
    PublishFailureKind, PublishMode, PublishOutcome,
};
use crate::format::{SegmentMetadata, SnapshotSection};
use crate::layout::ObjectLayout;
use crate::object::{ObjectName, ObjectPrefix};
use crate::service::{
    SnapshotPublishRequest, SnapshotService, SnapshotServiceError, WalSegmentMetadataSidecarLoad,
    WalSegmentMetadataSidecarService,
};
use std::collections::{btree_map::Entry, BTreeMap};
use std::fmt;
use std::sync::Mutex;
use strata_core::{CommitVersion, Timestamp};

const DATABASE_ID: [u8; 16] = [0x5a; 16];
const CODEC_ID: &str = "identity";
const BYTES_PER_OPERATION: usize = 4;
const MAX_OPERATIONS: usize = 96;

type FuzzResult<T> = Result<T, ServiceFuzzViolation>;

/// Summary returned after a service fuzz script completes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotServiceFuzzOutcome {
    steps_executed: usize,
}

impl SnapshotServiceFuzzOutcome {
    pub const fn steps_executed(self) -> usize {
        self.steps_executed
    }
}

/// Invariant failure found while executing a generated service script.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceFuzzViolation {
    message: &'static str,
}

impl ServiceFuzzViolation {
    pub(crate) const fn new(message: &'static str) -> Self {
        Self { message }
    }
}

impl fmt::Display for ServiceFuzzViolation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message)
    }
}

impl std::error::Error for ServiceFuzzViolation {}

/// Runs a bounded snapshot/sidecar service script generated from arbitrary bytes.
///
/// The runner lives behind the hidden testkit feature so fuzz targets can reach
/// crate-private services without turning those services into product API.
pub fn run_snapshot_service_script(bytes: &[u8]) -> FuzzResult<SnapshotServiceFuzzOutcome> {
    let backend = DurableScriptBackend::default();
    let mut model = ServiceModel::default();
    let mut steps_executed = 0;

    for chunk in bytes.chunks_exact(BYTES_PER_OPERATION).take(MAX_OPERATIONS) {
        let operation = ScriptOperation::from_chunk(chunk);
        apply_operation(&backend, &mut model, operation)?;
        assert_snapshot_index_matches(&backend, &model)?;
        assert_latest_matches(&backend, &model)?;
        steps_executed += 1;
    }

    Ok(SnapshotServiceFuzzOutcome { steps_executed })
}

#[derive(Clone, Copy, Debug)]
enum ScriptOperation {
    CreateSnapshot {
        snapshot_id: u64,
        payload_seed: u8,
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

impl ScriptOperation {
    fn from_chunk(chunk: &[u8]) -> Self {
        match chunk[0] % 8 {
            0 => Self::CreateSnapshot {
                snapshot_id: snapshot_id(chunk[1]),
                payload_seed: chunk[2],
            },
            1 => Self::LoadSnapshot {
                snapshot_id: snapshot_id(chunk[1]),
            },
            2 => Self::ListSnapshots,
            3 => Self::LatestSnapshot,
            4 => Self::PruneSnapshots {
                live_snapshot_id: live_snapshot_id(chunk[1]),
                retain_newest: usize::from(chunk[2] % 6),
            },
            5 => Self::PublishSidecar {
                segment_id: segment_id(chunk[1]),
            },
            6 => Self::CorruptSidecar {
                segment_id: segment_id(chunk[1]),
            },
            _ => Self::LoadSidecar {
                segment_id: segment_id(chunk[1]),
            },
        }
    }
}

#[derive(Default)]
struct ServiceModel {
    snapshots: BTreeMap<u64, Vec<u8>>,
    sidecars: BTreeMap<u64, SidecarState>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SidecarState {
    Present(SegmentMetadata),
    Corrupt,
}

#[derive(Debug, Default)]
struct DurableScriptBackend {
    objects: Mutex<BTreeMap<ObjectName, Vec<u8>>>,
}

impl DurableScriptBackend {
    fn insert(&self, name: ObjectName, bytes: Vec<u8>) -> BackendResult<()> {
        let mut objects = self
            .objects
            .lock()
            .map_err(|_| BackendError::new(BackendErrorKind::Unknown, "object lock poisoned"))?;
        objects.insert(name, bytes);
        Ok(())
    }
}

impl Backend for DurableScriptBackend {
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
            .map_err(|_| BackendError::new(BackendErrorKind::Unknown, "object lock poisoned"))?
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
        self.insert(name.clone(), bytes.to_vec())?;
        Ok(BackendMetadata::new(bytes.len() as u64, None))
    }

    fn delete_object(&self, name: &ObjectName) -> crate::backend::DeleteResult {
        let removed = self
            .objects
            .lock()
            .map_err(|_| {
                crate::backend::DeleteError::failed_before_removal(
                    name,
                    BackendError::new(BackendErrorKind::Unknown, "object lock poisoned"),
                )
            })?
            .remove(name)
            .is_some();
        crate::backend::durable_delete_result(name, removed)
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        Ok(self
            .objects
            .lock()
            .map_err(|_| BackendError::new(BackendErrorKind::Unknown, "object lock poisoned"))?
            .keys()
            .filter(|name| name.as_str().starts_with(prefix.as_str()))
            .cloned()
            .collect())
    }

    fn object_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
        self.objects
            .lock()
            .map_err(|_| BackendError::new(BackendErrorKind::Unknown, "object lock poisoned"))?
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
        let mut objects = self.objects.lock().map_err(|_| {
            PublishError::new(
                name.clone(),
                PublishFailureKind::FailedBeforeVisibility,
                BackendError::new(BackendErrorKind::Unknown, "object lock poisoned"),
            )
        })?;
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

fn apply_operation(
    backend: &DurableScriptBackend,
    model: &mut ServiceModel,
    operation: ScriptOperation,
) -> FuzzResult<()> {
    match operation {
        ScriptOperation::CreateSnapshot {
            snapshot_id,
            payload_seed,
        } => create_snapshot(backend, model, snapshot_id, payload_seed),
        ScriptOperation::LoadSnapshot { snapshot_id } => {
            assert_snapshot_load_matches(backend, model, snapshot_id)
        }
        ScriptOperation::ListSnapshots => assert_snapshot_index_matches(backend, model),
        ScriptOperation::LatestSnapshot => assert_latest_matches(backend, model),
        ScriptOperation::PruneSnapshots {
            live_snapshot_id,
            retain_newest,
        } => prune_snapshots(backend, model, live_snapshot_id, retain_newest),
        ScriptOperation::PublishSidecar { segment_id } => {
            publish_sidecar(backend, model, segment_id)
        }
        ScriptOperation::CorruptSidecar { segment_id } => {
            corrupt_sidecar(backend, model, segment_id)
        }
        ScriptOperation::LoadSidecar { segment_id } => {
            assert_sidecar_load_matches(backend, model, segment_id)
        }
    }
}

fn create_snapshot(
    backend: &DurableScriptBackend,
    model: &mut ServiceModel,
    snapshot_id: u64,
    payload_seed: u8,
) -> FuzzResult<()> {
    let payload = snapshot_payload(snapshot_id, payload_seed)?;
    let object = ObjectLayout::snapshot(snapshot_id)
        .map_err(|_| ServiceFuzzViolation::new("snapshot object layout rejected valid id"))?;
    let before = backend.read_object(&object).ok();
    let result = SnapshotService::new(backend)
        .publish_create(snapshot_request(snapshot_id, payload.clone())?);

    match model.snapshots.entry(snapshot_id) {
        Entry::Occupied(_) => {
            require(
                matches!(
                    result,
                    Err(SnapshotServiceError::Publish { ref source, .. })
                        if source.kind() == PublishFailureKind::PreconditionFailed
                ),
                "duplicate snapshot create did not return precondition failure",
            )?;
            require(
                before == backend.read_object(&object).ok(),
                "duplicate snapshot create changed visible bytes",
            )?;
        }
        Entry::Vacant(entry) => {
            require(result.is_ok(), "new snapshot create failed")?;
            entry.insert(payload);
        }
    }

    assert_snapshot_load_matches(backend, model, snapshot_id)
}

fn prune_snapshots(
    backend: &DurableScriptBackend,
    model: &mut ServiceModel,
    live_snapshot_id: Option<u64>,
    retain_newest: usize,
) -> FuzzResult<()> {
    let result = SnapshotService::new(backend).prune_snapshots(live_snapshot_id, retain_newest);
    if live_snapshot_id == Some(0) {
        require(
            matches!(
                result,
                Err(SnapshotServiceError::InvalidSnapshotFact {
                    snapshot_id: 0,
                    field: "snapshot_id"
                })
            ),
            "zero live snapshot id was not rejected",
        )?;
        return Ok(());
    }

    let report = result.map_err(|_| ServiceFuzzViolation::new("snapshot prune failed"))?;
    let (deleted, protected) = expected_prune(&model.snapshots, live_snapshot_id, retain_newest);
    require(
        snapshot_ids(report.deleted()) == deleted,
        "snapshot prune deleted set did not match model",
    )?;
    require(
        snapshot_ids(report.protected()) == protected,
        "snapshot prune protected set did not match model",
    )?;
    require(
        report.failed().is_empty(),
        "snapshot prune reported delete failure",
    )?;
    model
        .snapshots
        .retain(|snapshot_id, _| protected.contains(snapshot_id));
    Ok(())
}

fn publish_sidecar(
    backend: &DurableScriptBackend,
    model: &mut ServiceModel,
    segment_id: u64,
) -> FuzzResult<()> {
    let metadata = metadata_for_segment(segment_id);
    WalSegmentMetadataSidecarService::new(backend)
        .publish_replace(&metadata)
        .map_err(|_| ServiceFuzzViolation::new("sidecar publish failed"))?;
    model
        .sidecars
        .insert(segment_id, SidecarState::Present(metadata));
    Ok(())
}

fn corrupt_sidecar(
    backend: &DurableScriptBackend,
    model: &mut ServiceModel,
    segment_id: u64,
) -> FuzzResult<()> {
    let object = ObjectLayout::wal_segment_metadata(segment_id)
        .map_err(|_| ServiceFuzzViolation::new("sidecar object layout rejected valid id"))?;
    // Sidecar bytes are cache facts. Corruption must remain recoverable and must
    // not change authoritative snapshot state.
    backend
        .write_object(&object, b"corrupt sidecar")
        .map_err(|_| ServiceFuzzViolation::new("sidecar corruption write failed"))?;
    model.sidecars.insert(segment_id, SidecarState::Corrupt);
    Ok(())
}

fn assert_snapshot_index_matches(
    backend: &DurableScriptBackend,
    model: &ServiceModel,
) -> FuzzResult<()> {
    let snapshots = SnapshotService::new(backend)
        .list_snapshots()
        .map_err(|_| ServiceFuzzViolation::new("snapshot list failed"))?;
    let expected: Vec<u64> = model.snapshots.keys().copied().collect();
    require(
        snapshot_ids(&snapshots) == expected,
        "snapshot list did not match model",
    )
}

fn assert_latest_matches(backend: &DurableScriptBackend, model: &ServiceModel) -> FuzzResult<()> {
    let latest = SnapshotService::new(backend)
        .latest_snapshot()
        .map_err(|_| ServiceFuzzViolation::new("latest snapshot failed"))?
        .map(|snapshot| snapshot.snapshot_id());
    require(
        latest == model.snapshots.keys().next_back().copied(),
        "latest snapshot did not match model",
    )
}

fn assert_snapshot_load_matches(
    backend: &DurableScriptBackend,
    model: &ServiceModel,
    snapshot_id: u64,
) -> FuzzResult<()> {
    let result =
        SnapshotService::new(backend).load_required_for_codec(snapshot_id, DATABASE_ID, CODEC_ID);
    if let Some(expected_payload) = model.snapshots.get(&snapshot_id) {
        let snapshot =
            result.map_err(|_| ServiceFuzzViolation::new("visible snapshot failed load"))?;
        require(
            snapshot.header().snapshot_id() == snapshot_id,
            "loaded snapshot id did not match request",
        )?;
        require(
            snapshot.sections().len() == 1,
            "loaded snapshot section count changed",
        )?;
        require(
            snapshot.sections()[0].payload() == expected_payload.as_slice(),
            "loaded snapshot payload did not match model",
        )
    } else {
        require(
            matches!(
                result,
                Err(SnapshotServiceError::Missing {
                    snapshot_id: missing_id,
                    ..
                }) if missing_id == snapshot_id
            ),
            "missing snapshot load did not report missing",
        )
    }
}

fn assert_sidecar_load_matches(
    backend: &DurableScriptBackend,
    model: &ServiceModel,
    segment_id: u64,
) -> FuzzResult<()> {
    let loaded = WalSegmentMetadataSidecarService::new(backend)
        .load(segment_id)
        .map_err(|_| ServiceFuzzViolation::new("sidecar load returned fatal error"))?;
    match model.sidecars.get(&segment_id) {
        Some(SidecarState::Present(expected)) => {
            let WalSegmentMetadataSidecarLoad::Present(sidecar) = loaded else {
                return Err(ServiceFuzzViolation::new("present sidecar did not load"));
            };
            require(sidecar.metadata() == expected, "sidecar metadata changed")
        }
        Some(SidecarState::Corrupt) => require(
            matches!(loaded, WalSegmentMetadataSidecarLoad::Corrupt { .. }),
            "corrupt sidecar did not load as recoverable corruption",
        ),
        None => require(
            matches!(loaded, WalSegmentMetadataSidecarLoad::Missing { .. }),
            "missing sidecar did not load as missing",
        ),
    }
}

fn snapshot_request(snapshot_id: u64, payload: Vec<u8>) -> FuzzResult<SnapshotPublishRequest> {
    let section = SnapshotSection::new(0x01, payload)
        .map_err(|_| ServiceFuzzViolation::new("fixed non-zero section kind was rejected"))?;
    Ok(SnapshotPublishRequest::new(
        snapshot_id,
        CommitVersion::new(snapshot_id),
        Timestamp::from_micros(snapshot_id * 100),
        DATABASE_ID,
        CODEC_ID,
        vec![section],
    ))
}

fn expected_prune(
    snapshots: &BTreeMap<u64, Vec<u8>>,
    live_snapshot_id: Option<u64>,
    retain_newest: usize,
) -> (Vec<u64>, Vec<u64>) {
    let snapshot_ids: Vec<u64> = snapshots.keys().copied().collect();
    let retain_newest = retain_newest.max(1);
    let retain_start = snapshot_ids.len().saturating_sub(retain_newest);
    let mut deleted = Vec::new();
    let mut protected = Vec::new();

    for (index, snapshot_id) in snapshot_ids.iter().copied().enumerate() {
        if index >= retain_start || live_snapshot_id == Some(snapshot_id) {
            protected.push(snapshot_id);
        } else {
            deleted.push(snapshot_id);
        }
    }

    (deleted, protected)
}

fn snapshot_ids(snapshots: &[crate::service::SnapshotObject]) -> Vec<u64> {
    snapshots
        .iter()
        .map(crate::service::SnapshotObject::snapshot_id)
        .collect()
}

fn snapshot_id(byte: u8) -> u64 {
    u64::from(byte % 16) + 1
}

fn live_snapshot_id(byte: u8) -> Option<u64> {
    match byte % 18 {
        0 => None,
        1 => Some(0),
        value => Some(u64::from((value - 2) % 16) + 1),
    }
}

fn segment_id(byte: u8) -> u64 {
    u64::from(byte % 8) + 1
}

fn snapshot_payload(snapshot_id: u64, seed: u8) -> FuzzResult<Vec<u8>> {
    let snapshot_byte = u8::try_from(snapshot_id)
        .map_err(|_| ServiceFuzzViolation::new("snapshot id exceeded script byte range"))?;
    Ok(vec![snapshot_byte, seed])
}

fn metadata_for_segment(segment_id: u64) -> SegmentMetadata {
    let mut metadata = SegmentMetadata::empty(segment_id);
    metadata.track_record(
        CommitVersion::new(segment_id),
        Timestamp::from_micros(segment_id * 100),
    );
    metadata
}

fn require(condition: bool, message: &'static str) -> FuzzResult<()> {
    if condition {
        Ok(())
    } else {
        Err(ServiceFuzzViolation::new(message))
    }
}

#[cfg(test)]
mod tests {
    use super::run_snapshot_service_script;

    #[test]
    fn snapshot_service_script_accepts_empty_input() {
        let outcome = run_snapshot_service_script(&[]).expect("empty script");

        assert_eq!(outcome.steps_executed(), 0);
    }

    #[test]
    fn snapshot_service_script_exercises_snapshot_and_sidecar_paths() {
        let script = [
            0, 1, 7, 0, // create snapshot
            1, 1, 0, 0, // load snapshot
            5, 2, 0, 0, // publish sidecar
            7, 2, 0, 0, // load sidecar
            6, 2, 0, 0, // corrupt sidecar
            7, 2, 0, 0, // load corrupt sidecar
            4, 0, 0, 0, // prune with no live snapshot
        ];

        let outcome = run_snapshot_service_script(&script).expect("script");

        assert_eq!(outcome.steps_executed(), script.len() / 4);
    }
}
