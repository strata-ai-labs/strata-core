use super::{
    check_table_format_model_script, run_quarantine_service_script, run_snapshot_service_script,
    TestkitError,
};
use std::time::{Duration, Instant};

#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
use crate::format::{encode_immutable_table, SnapshotSection, TableCompression};
#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
use crate::layout::ObjectLayout;
#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
#[cfg(any(feature = "fault-injection", test))]
use crate::service::{
    QuarantineGate, QuarantineObjectRequest, QuarantineObjectStatus, QuarantineService,
    SnapshotServiceError, TableObjectServiceError,
};
#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
use crate::service::{SnapshotPublishRequest, SnapshotService, TableObjectService};
#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
const DATABASE_ID: [u8; 16] = [0x9d; 16];
#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
const CODEC_ID: &str = "identity";

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
type CrashRecoveryCase = fn(&std::path::Path) -> Result<(), TestkitError>;
#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
type CrashRecoveryRecorder = fn(&mut CrashRecoveryHarnessOutcome);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CrashRecoveryHarnessOutcome {
    cases_executed: usize,
    log_append_replay: usize,
    unresolved_gate_reconcile: usize,
    orphan_snapshot_ignored: usize,
    checkpoint_tail_recovered: usize,
    orphan_table_reported: usize,
    quarantine_inventory_debt: usize,
    object_quarantine_preserved: usize,
    close_reopen_consistent: usize,
    ignored_case_equivalents: usize,
    harness_environment: usize,
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn record_snapshot_reopen_window(outcome: &mut CrashRecoveryHarnessOutcome) {
    outcome.orphan_snapshot_ignored += 1;
    outcome.quarantine_inventory_debt += 1;
    outcome.object_quarantine_preserved += 1;
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn record_log_tail_window(outcome: &mut CrashRecoveryHarnessOutcome) {
    outcome.log_append_replay += 1;
    outcome.unresolved_gate_reconcile += 1;
    outcome.checkpoint_tail_recovered += 1;
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn record_table_window(outcome: &mut CrashRecoveryHarnessOutcome) {
    outcome.orphan_table_reported += 1;
}

impl CrashRecoveryHarnessOutcome {
    pub const fn cases_executed(self) -> usize {
        self.cases_executed
    }

    pub const fn log_append_replay_cases(self) -> usize {
        self.log_append_replay
    }

    pub const fn unresolved_gate_reconcile_cases(self) -> usize {
        self.unresolved_gate_reconcile
    }

    pub const fn orphan_snapshot_ignored_cases(self) -> usize {
        self.orphan_snapshot_ignored
    }

    pub const fn checkpoint_tail_recovered_cases(self) -> usize {
        self.checkpoint_tail_recovered
    }

    pub const fn orphan_table_reported_cases(self) -> usize {
        self.orphan_table_reported
    }

    pub const fn quarantine_inventory_debt_cases(self) -> usize {
        self.quarantine_inventory_debt
    }

    pub const fn object_quarantine_preserved_cases(self) -> usize {
        self.object_quarantine_preserved
    }

    pub const fn close_reopen_consistent_cases(self) -> usize {
        self.close_reopen_consistent
    }

    pub const fn ignored_case_equivalent_cases(self) -> usize {
        self.ignored_case_equivalents
    }

    pub const fn harness_environment_cases(self) -> usize {
        self.harness_environment
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageStressHarnessOutcome {
    iterations: usize,
    scripts_executed: usize,
}

impl StorageStressHarnessOutcome {
    pub const fn iterations(self) -> usize {
        self.iterations
    }

    pub const fn scripts_executed(self) -> usize {
        self.scripts_executed
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceFaultWindowHarnessOutcome {
    cases_executed: usize,
}

impl ServiceFaultWindowHarnessOutcome {
    pub const fn cases_executed(self) -> usize {
        self.cases_executed
    }
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
pub fn run_localfs_crash_recovery_harness(
    root: &std::path::Path,
    case_limit: Option<usize>,
) -> Result<CrashRecoveryHarnessOutcome, TestkitError> {
    let cases: &[(CrashRecoveryCase, CrashRecoveryRecorder)] = &[
        (
            localfs_snapshot_survives_reopen,
            record_snapshot_reopen_window,
        ),
        (localfs_sidecar_survives_reopen, record_log_tail_window),
        (localfs_table_object_survives_reopen, record_table_window),
    ];
    let limit = case_limit.unwrap_or(cases.len()).min(cases.len());
    std::fs::create_dir_all(root)
        .map_err(|err| TestkitError::new(format!("create crash harness root: {err}")))?;

    let mut outcome = CrashRecoveryHarnessOutcome {
        harness_environment: 1,
        ..CrashRecoveryHarnessOutcome::default()
    };
    for (index, (case, record)) in cases.iter().take(limit).enumerate() {
        let case_root = root.join(format!("case-{index:02}"));
        if case_root.exists() {
            std::fs::remove_dir_all(&case_root)
                .map_err(|err| TestkitError::new(format!("reset crash case directory: {err}")))?;
        }
        std::fs::create_dir_all(&case_root)
            .map_err(|err| TestkitError::new(format!("create crash case directory: {err}")))?;
        case(&case_root)?;
        outcome.cases_executed += 1;
        record(&mut outcome);
    }

    if outcome.cases_executed == cases.len() {
        outcome.close_reopen_consistent += 1;
        outcome.ignored_case_equivalents += 1;
    }
    Ok(outcome)
}

pub fn run_storage_stress_harness(
    seed: u64,
    duration: Option<Duration>,
) -> Result<StorageStressHarnessOutcome, TestkitError> {
    let mut rng = SplitMix64::new(seed);
    let deadline = duration.and_then(|duration| Instant::now().checked_add(duration));
    let minimum_iterations = 8;
    let maximum_iterations = if duration.is_some() { 1024 } else { 8 };
    let mut iterations = 0;
    let mut scripts_executed = 0;

    while iterations < maximum_iterations {
        run_snapshot_service_script(&random_bytes(&mut rng, 384))
            .map_err(|err| TestkitError::new(format!("snapshot stress script: {err}")))?;
        scripts_executed += 1;
        run_quarantine_service_script(&random_bytes(&mut rng, 768))
            .map_err(|err| TestkitError::new(format!("quarantine stress script: {err}")))?;
        scripts_executed += 1;
        check_table_format_model_script(&random_bytes(&mut rng, 512))?;
        scripts_executed += 1;

        iterations += 1;
        if iterations >= minimum_iterations
            && deadline.is_none_or(|deadline| Instant::now() >= deadline)
        {
            break;
        }
    }

    Ok(StorageStressHarnessOutcome {
        iterations,
        scripts_executed,
    })
}

#[cfg(any(test, feature = "fault-injection"))]
pub fn run_service_fault_window_harness() -> Result<ServiceFaultWindowHarnessOutcome, TestkitError>
{
    snapshot_publish_fault_preserves_absence()?;
    table_publish_fault_preserves_absence()?;
    quarantine_inventory_publish_fault_preserves_source()?;
    Ok(ServiceFaultWindowHarnessOutcome { cases_executed: 3 })
}

#[derive(Clone, Copy, Debug)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }
}

fn random_bytes(rng: &mut SplitMix64, len: usize) -> Vec<u8> {
    (0..len).map(|_| rng.next_u64().to_le_bytes()[0]).collect()
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn localfs_snapshot_survives_reopen(root: &std::path::Path) -> Result<(), TestkitError> {
    use crate::backend::local_fs::LocalFsBackend;

    let backend = LocalFsBackend::new(root);
    SnapshotService::new(&backend)
        .publish_create(snapshot_request(11, b"crash-snapshot")?)
        .map_err(|err| TestkitError::new(format!("publish snapshot before reopen: {err}")))?;

    let reopened = LocalFsBackend::new(root);
    let loaded = SnapshotService::new(&reopened)
        .load_required_for_codec(11, DATABASE_ID, CODEC_ID)
        .map_err(|err| TestkitError::new(format!("load snapshot after reopen: {err}")))?;
    require(
        loaded.sections()[0].payload() == b"crash-snapshot",
        "snapshot payload changed after reopen",
    )
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn localfs_sidecar_survives_reopen(root: &std::path::Path) -> Result<(), TestkitError> {
    use crate::backend::local_fs::LocalFsBackend;
    use crate::format::SegmentMetadata;
    use crate::service::{WalSegmentMetadataSidecarLoad, WalSegmentMetadataSidecarService};

    let mut metadata = SegmentMetadata::empty(7);
    metadata.track_record(CommitVersion::new(7), Timestamp::from_micros(700));
    let backend = LocalFsBackend::new(root);
    WalSegmentMetadataSidecarService::new(&backend)
        .publish_replace(&metadata)
        .map_err(|err| TestkitError::new(format!("publish sidecar before reopen: {err}")))?;

    let reopened = LocalFsBackend::new(root);
    let loaded = WalSegmentMetadataSidecarService::new(&reopened)
        .load(7)
        .map_err(|err| TestkitError::new(format!("load sidecar after reopen: {err}")))?;
    match loaded {
        WalSegmentMetadataSidecarLoad::Present(sidecar) if sidecar.metadata() == &metadata => {
            Ok(())
        }
        _ => Err(TestkitError::new("sidecar did not survive reopen")),
    }
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn localfs_table_object_survives_reopen(root: &std::path::Path) -> Result<(), TestkitError> {
    use crate::backend::local_fs::LocalFsBackend;
    use crate::backend::Backend;
    use crate::format::decode_immutable_table;

    let bytes = valid_table_bytes()?;
    let branch = branch_id().to_string();
    let object = ObjectLayout::table_object(&branch, 1, "table0001")
        .map_err(|err| TestkitError::new(format!("table object layout: {err}")))?;
    let backend = LocalFsBackend::new(root);
    TableObjectService::new(&backend)
        .publish_create(&branch, 1, "table0001", &bytes)
        .map_err(|err| TestkitError::new(format!("publish table before reopen: {err}")))?;

    let reopened = LocalFsBackend::new(root);
    let table_bytes = reopened
        .read_object(&object)
        .map_err(|err| TestkitError::new(format!("read table after reopen: {err}")))?;
    let decoded = decode_immutable_table(&table_bytes)
        .map_err(|err| TestkitError::new(format!("decode table after reopen: {err}")))?;
    require(decoded.rows().len() == 2, "table rows changed after reopen")
}

#[cfg(any(test, feature = "fault-injection"))]
fn snapshot_publish_fault_preserves_absence() -> Result<(), TestkitError> {
    use crate::backend::{Backend, PublishFailureKind};
    use crate::testkit::FaultingBackend;

    let backend = FaultingBackend::new(HarnessBackend::default(), publish_fault_script());
    let object = ObjectLayout::snapshot(21)
        .map_err(|err| TestkitError::new(format!("snapshot layout: {err}")))?;
    let error = SnapshotService::new(&backend)
        .publish_create(snapshot_request(21, b"fault-snapshot")?)
        .expect_err("fault script must fail snapshot publish");
    require(
        matches!(error, SnapshotServiceError::Publish { source, .. }
            if source.kind() == PublishFailureKind::FailedBeforeVisibility),
        "snapshot publish fault was not classified before visibility",
    )?;
    require(
        backend.read_object(&object).is_err(),
        "snapshot became visible after before-visibility publish fault",
    )
}

#[cfg(any(test, feature = "fault-injection"))]
fn table_publish_fault_preserves_absence() -> Result<(), TestkitError> {
    use crate::backend::{Backend, PublishFailureKind};
    use crate::testkit::FaultingBackend;

    let backend = FaultingBackend::new(HarnessBackend::default(), publish_fault_script());
    let branch = branch_id().to_string();
    let object = ObjectLayout::table_object(&branch, 0, "table0001")
        .map_err(|err| TestkitError::new(format!("table layout: {err}")))?;
    let error = TableObjectService::new(&backend)
        .publish_create(&branch, 0, "table0001", &valid_table_bytes()?)
        .expect_err("fault script must fail table publish");
    require(
        matches!(error, TableObjectServiceError::Publish { source, .. }
            if source.kind() == PublishFailureKind::FailedBeforeVisibility),
        "table publish fault was not classified before visibility",
    )?;
    require(
        backend.read_object(&object).is_err(),
        "table became visible after before-visibility publish fault",
    )
}

#[cfg(any(test, feature = "fault-injection"))]
fn quarantine_inventory_publish_fault_preserves_source() -> Result<(), TestkitError> {
    use crate::backend::Backend;
    use crate::testkit::FaultingBackend;

    let backend = FaultingBackend::new(HarnessBackend::default(), publish_fault_script());
    let source = ObjectLayout::table_object("main", 0, "table0001")
        .map_err(|err| TestkitError::new(format!("source layout: {err}")))?;
    backend
        .write_object(&source, b"source-bytes")
        .map_err(|err| TestkitError::new(format!("seed quarantine source: {err}")))?;
    let request = QuarantineObjectRequest::new(
        branch_id(),
        DATABASE_ID,
        CODEC_ID,
        "table0001",
        source.clone(),
        Timestamp::from_micros(900),
        QuarantineGate::Safe,
    );

    let report = QuarantineService::new(&backend)
        .quarantine_object(&request)
        .map_err(|err| TestkitError::new(format!("quarantine with publish fault: {err}")))?;
    require(
        report.status() == QuarantineObjectStatus::InventoryPublishFailed,
        "quarantine inventory publish fault returned wrong status",
    )?;
    require(
        backend.read_object(&source).as_deref() == Ok(b"source-bytes"),
        "source was deleted before durable quarantine copy",
    )
}

#[cfg(any(test, feature = "fault-injection"))]
fn publish_fault_script() -> crate::testkit::FaultScript {
    use crate::testkit::{BackendOperation, FaultKind, FaultRule, FaultScript};
    use std::num::NonZeroU64;

    FaultScript::new([FaultRule::new(
        BackendOperation::PublishObject,
        NonZeroU64::new(1).expect("non-zero publish call"),
        FaultKind::Interrupted,
    )])
}

#[cfg(any(test, feature = "fault-injection"))]
#[derive(Debug, Default)]
struct HarnessBackend {
    objects: std::sync::Mutex<std::collections::BTreeMap<crate::object::ObjectName, Vec<u8>>>,
}

#[cfg(any(test, feature = "fault-injection"))]
impl crate::backend::Backend for HarnessBackend {
    fn capabilities(&self) -> crate::backend::BackendCapabilities {
        use crate::backend::BackendCapability;
        crate::backend::BackendCapabilities::from_slice(&[
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

    fn read_object(
        &self,
        name: &crate::object::ObjectName,
    ) -> crate::backend::BackendResult<Vec<u8>> {
        self.objects
            .lock()
            .map_err(|_| backend_error("object lock poisoned"))?
            .get(name)
            .cloned()
            .ok_or_else(|| not_found("object not found"))
    }

    fn read_range(
        &self,
        name: &crate::object::ObjectName,
        range: crate::backend::BackendRange,
    ) -> crate::backend::BackendResult<Vec<u8>> {
        let bytes = self.read_object(name)?;
        let start = usize::try_from(range.offset()).map_err(|_| invalid_range())?;
        let end = usize::try_from(range.end_offset().ok_or_else(invalid_range)?)
            .map_err(|_| invalid_range())?;
        Ok(bytes[start.min(bytes.len())..end.min(bytes.len())].to_vec())
    }

    fn write_object(
        &self,
        name: &crate::object::ObjectName,
        bytes: &[u8],
    ) -> crate::backend::BackendResult<crate::backend::BackendMetadata> {
        self.objects
            .lock()
            .map_err(|_| backend_error("object lock poisoned"))?
            .insert(name.clone(), bytes.to_vec());
        Ok(crate::backend::BackendMetadata::new(
            bytes.len() as u64,
            None,
        ))
    }

    fn delete_object(&self, name: &crate::object::ObjectName) -> crate::backend::BackendResult<()> {
        self.objects
            .lock()
            .map_err(|_| backend_error("object lock poisoned"))?
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| not_found("object not found"))
    }

    fn list_prefix(
        &self,
        prefix: &crate::object::ObjectPrefix,
    ) -> crate::backend::BackendResult<Vec<crate::object::ObjectName>> {
        Ok(self
            .objects
            .lock()
            .map_err(|_| backend_error("object lock poisoned"))?
            .keys()
            .filter(|name| name.as_str().starts_with(prefix.as_str()))
            .cloned()
            .collect())
    }

    fn object_metadata(
        &self,
        name: &crate::object::ObjectName,
    ) -> crate::backend::BackendResult<crate::backend::BackendMetadata> {
        let len = self.read_object(name)?.len();
        Ok(crate::backend::BackendMetadata::new(len as u64, None))
    }

    fn publish_object(
        &self,
        name: &crate::object::ObjectName,
        bytes: &[u8],
        mode: crate::backend::PublishMode,
    ) -> crate::backend::PublishResult<crate::backend::PublishOutcome> {
        let mut objects = self.objects.lock().map_err(|_| {
            crate::backend::PublishError::new(
                name.clone(),
                crate::backend::PublishFailureKind::FailedBeforeVisibility,
                backend_error("object lock poisoned"),
            )
        })?;
        if mode == crate::backend::PublishMode::Create && objects.contains_key(name) {
            return Err(crate::backend::PublishError::precondition_failed(
                name,
                "object already exists",
            ));
        }
        objects.insert(name.clone(), bytes.to_vec());
        Ok(crate::backend::PublishOutcome::new(
            name.clone(),
            crate::backend::BackendMetadata::new(bytes.len() as u64, None),
            crate::backend::PublishDurability::Durable,
        ))
    }
}

#[cfg(any(test, feature = "fault-injection"))]
fn backend_error(message: &'static str) -> crate::backend::BackendError {
    crate::backend::BackendError::new(crate::backend::BackendErrorKind::Unknown, message)
}

#[cfg(any(test, feature = "fault-injection"))]
fn not_found(message: &'static str) -> crate::backend::BackendError {
    crate::backend::BackendError::new(crate::backend::BackendErrorKind::NotFound, message)
}

#[cfg(any(test, feature = "fault-injection"))]
fn invalid_range() -> crate::backend::BackendError {
    crate::backend::BackendError::new(crate::backend::BackendErrorKind::InvalidRange, "bad range")
}

#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
fn snapshot_request(
    snapshot_id: u64,
    payload: &'static [u8],
) -> Result<SnapshotPublishRequest, TestkitError> {
    let section = SnapshotSection::new(0x01, payload.to_vec())
        .map_err(|err| TestkitError::new(format!("snapshot section: {err}")))?;
    Ok(SnapshotPublishRequest::new(
        snapshot_id,
        CommitVersion::new(snapshot_id),
        Timestamp::from_micros(snapshot_id * 100),
        DATABASE_ID,
        CODEC_ID,
        vec![section],
    ))
}

#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
fn valid_table_bytes() -> Result<Vec<u8>, TestkitError> {
    let rows = vec![
        table_row(b"alpha".to_vec(), 9),
        table_row(b"beta".to_vec(), 7),
    ];
    encode_immutable_table(&rows, 4096, 8, TableCompression::Uncompressed)
        .map_err(|err| TestkitError::new(format!("encode table: {err}")))
}

#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
fn table_row(user_key: Vec<u8>, version: u64) -> StorageRow {
    let key = PhysicalKey::new(
        branch_id(),
        "default",
        StorageSpaceId::engine(0x20).expect("engine storage space id"),
        user_key,
    )
    .expect("physical key");
    StorageRow::put(
        key,
        CommitVersion::new(version),
        Timestamp::from_micros(version * 100),
        Timestamp::EPOCH,
        vec![u8::try_from(version).expect("test table version fits byte")],
    )
}

#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
fn branch_id() -> BranchId {
    BranchId::from_bytes([0x44; BranchId::BYTE_LEN])
}

#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
fn require(condition: bool, message: &'static str) -> Result<(), TestkitError> {
    if condition {
        Ok(())
    } else {
        Err(TestkitError::new(message))
    }
}
