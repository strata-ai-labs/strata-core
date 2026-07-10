//! Generated durable lifecycle assembly checks.

use super::super::TestkitError;
use super::{ensure, script_byte, LifecycleScaffoldOutcome};
use crate::backend::{
    Backend, BackendAppend, BackendCapabilities, BackendError, BackendErrorKind, BackendMetadata,
    BackendRange, BackendResult, BackendWriterGuard, PublishDurability, PublishError,
    PublishFailureKind, PublishMode, PublishOutcome, PublishResult,
    DURABLE_LOCAL_MODE_REQUIREMENTS,
};
use crate::branch::config::BranchRuntimeConfig;
use crate::commit::{CommitBranchGeneration, CommitManualTimestampSource, CommitRuntimeConfig};
use crate::config::mode::DurabilityPolicy;
use crate::format::{encode_manifest, DatabaseManifest};
use crate::layout::ObjectLayout;
use crate::lifecycle::{
    LifecycleCodecId, LifecycleConfig, LifecycleDurableLocalOpenRequest,
    LifecycleDurableLocalShell, LifecycleError, LifecycleState, RecoveryStrictness, StorageMode,
    StorageOpenDisposition, StorageOpenPlan,
};
use crate::object::{ObjectName, ObjectPrefix};
use crate::service::WalServiceConfig;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

const DATABASE_ID: [u8; 16] = [0x73; 16];
const OTHER_DATABASE_ID: [u8; 16] = [0x74; 16];

pub(super) fn check_lifecycle_durable_contract(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    check_durable_standard_create(script, outcome)?;
    check_durable_always_existing(script, outcome)?;
    check_durable_rejections(script, outcome)?;
    check_durable_lock_failure(script, outcome)?;
    check_durable_identity_mismatch(script, outcome)?;
    check_durable_create_race(script, outcome)?;
    check_durable_publish_fault(script, outcome)?;
    check_durable_wal_failure(script, outcome)?;
    check_input_derived_durable(script, outcome)?;
    Ok(())
}

fn check_durable_standard_create(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let backend: &'static DurableScriptBackend = Box::leak(Box::new(DurableScriptBackend::new()));
    let shell = assemble(
        StorageMode::DurableLocalStandard,
        script_byte(script, 40),
        backend,
    )?;
    ensure(
        shell.state() == LifecycleState::Recovering,
        "durable standard shell did not enter recovering state",
    )?;
    ensure(
        shell.assembly_facts().disposition() == StorageOpenDisposition::Created,
        "durable standard did not create missing manifest",
    )?;
    ensure(
        shell.assembly_facts().durability_policy() == DurabilityPolicy::Standard,
        "durable standard policy drifted",
    )?;
    ensure(
        shell.assembly_facts().active_wal_segment() == 1,
        "new durable database did not open WAL segment one",
    )?;
    ensure(
        shell.admit_recovery_step().is_ok()
            && shell.admit_ordinary_read().is_err()
            && shell.admit_commit().is_err()
            && shell.admit_ordinary_maintenance().is_err()
            && shell.admit_health_query().is_ok(),
        "durable shell admitted wrong operation before recovery",
    )?;
    // Assembly's only directory listing is the WAL-prefix scan that reconciles
    // the writer's resume segment against the on-disk tail (#2555). Anything
    // else listing objects during assembly is still a side-effect violation.
    ensure(
        backend.listed_prefixes()
            == vec![ObjectLayout::wal_prefix()
                .map_err(testkit_error)?
                .as_str()
                .to_owned()],
        "durable assembly listed objects beyond the single WAL resume scan",
    )?;
    outcome.durable_assembly_standard_cases += 1;
    outcome.durable_manifest_create_cases += 1;
    outcome.durable_recovering_admission_cases += 1;
    outcome.durable_no_recovery_side_effect_cases += 1;
    Ok(())
}

fn check_durable_always_existing(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let backend: &'static DurableScriptBackend = Box::leak(Box::new(DurableScriptBackend::new()));
    let active_segment = u64::from(script_byte(script, 41) % 8) + 2;
    let manifest = DatabaseManifest::new(DATABASE_ID, "identity")
        .map_err(testkit_error)?
        .with_recovery_facts(
            active_segment,
            Some(CommitVersion::new(8).as_u64()),
            Some(2),
            Some(CommitVersion::new(7)),
        )
        .map_err(testkit_error)?;
    backend.write_raw(
        ObjectLayout::database_manifest().map_err(testkit_error)?,
        encode_manifest(&manifest).map_err(testkit_error)?,
    );
    let shell = assemble(
        StorageMode::DurableLocalAlways,
        script_byte(script, 42),
        backend,
    )?;
    ensure(
        shell.assembly_facts().disposition() == StorageOpenDisposition::OpenedExisting,
        "durable always did not open existing manifest",
    )?;
    ensure(
        shell.assembly_facts().durability_policy() == DurabilityPolicy::Always,
        "durable always policy drifted",
    )?;
    ensure(
        shell.assembly_facts().active_wal_segment() == active_segment,
        "durable always ignored manifest active WAL segment",
    )?;
    outcome.durable_assembly_always_cases += 1;
    outcome.durable_manifest_existing_cases += 1;
    Ok(())
}

fn check_durable_rejections(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let rejected = LifecycleDurableLocalOpenRequest::new(
        open_plan(StorageMode::Cache),
        DATABASE_ID,
        branch_id(script_byte(script, 43)),
        CommitBranchGeneration::new(1).map_err(testkit_error)?,
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        WalServiceConfig::default()
            .with_append_buffer_bytes(crate::service::DEFAULT_WAL_APPEND_BUFFER_BYTES),
    );
    ensure(
        matches!(rejected, Err(LifecycleError::InvalidOpenPlan { .. })),
        "durable request accepted cache mode",
    )?;

    let backend: &'static DurableScriptBackend = Box::leak(Box::new(
        DurableScriptBackend::with_capabilities(BackendCapabilities::empty()),
    ));
    let capability_rejected = LifecycleDurableLocalShell::assemble(
        request(StorageMode::DurableLocalStandard, script_byte(script, 44))?,
        backend,
        timestamp_source(),
    );
    ensure(
        matches!(
            capability_rejected,
            Err(LifecycleError::CapabilityMismatch { .. })
        ),
        "durable shell accepted missing capabilities",
    )?;
    ensure(
        backend.operation_count() == 1 && backend.saw_capabilities(),
        "capability rejection performed durable side effects",
    )?;
    outcome.durable_assembly_rejected_cases += 1;
    Ok(())
}

fn check_durable_lock_failure(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let backend: &'static DurableScriptBackend =
        Box::leak(Box::new(DurableScriptBackend::with_lock_failure()));
    let rejected = LifecycleDurableLocalShell::assemble(
        request(StorageMode::DurableLocalStandard, script_byte(script, 45))?,
        backend,
        timestamp_source(),
    );
    ensure(
        matches!(rejected, Err(LifecycleError::LowerLayer { .. })),
        "writer lock failure was not typed",
    )?;
    ensure(
        backend.saw_writer_lock() && !backend.saw_read_object(),
        "writer lock failure did not stop before manifest read",
    )?;
    outcome.durable_writer_lock_failure_cases += 1;
    Ok(())
}

fn check_durable_identity_mismatch(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let backend: &'static DurableScriptBackend = Box::leak(Box::new(DurableScriptBackend::new()));
    let manifest = DatabaseManifest::new(OTHER_DATABASE_ID, "identity").map_err(testkit_error)?;
    backend.write_raw(
        ObjectLayout::database_manifest().map_err(testkit_error)?,
        encode_manifest(&manifest).map_err(testkit_error)?,
    );
    let rejected = LifecycleDurableLocalShell::assemble(
        request(StorageMode::DurableLocalStandard, script_byte(script, 46))?,
        backend,
        timestamp_source(),
    );
    ensure(
        matches!(rejected, Err(LifecycleError::InvalidOpenPlan { .. })),
        "manifest identity mismatch did not reject",
    )?;
    ensure(
        !backend.saw_wal_metadata(),
        "manifest identity mismatch still opened WAL",
    )?;
    outcome.durable_manifest_identity_mismatch_cases += 1;
    Ok(())
}

fn check_durable_create_race(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let active_segment = u64::from(script_byte(script, 47) % 8) + 2;
    let race_manifest = DatabaseManifest::new(DATABASE_ID, "identity")
        .map_err(testkit_error)?
        .with_recovery_facts(active_segment, None, None, None)
        .map_err(testkit_error)?;
    let backend: &'static DurableScriptBackend = Box::leak(Box::new(
        DurableScriptBackend::with_create_race(race_manifest),
    ));
    let shell = assemble(
        StorageMode::DurableLocalStandard,
        script_byte(script, 48),
        backend,
    )?;
    ensure(
        shell.assembly_facts().disposition() == StorageOpenDisposition::OpenedExisting,
        "durable create race did not reload existing database object",
    )?;
    ensure(
        shell.assembly_facts().active_wal_segment() == active_segment,
        "durable create race ignored reloaded active segment",
    )?;
    outcome.durable_manifest_create_race_cases += 1;
    Ok(())
}

fn check_durable_publish_fault(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let backend: &'static DurableScriptBackend = Box::leak(Box::new(
        DurableScriptBackend::with_publish_failure(PublishFailureKind::VisibilityUnknown),
    ));
    let rejected = LifecycleDurableLocalShell::assemble(
        request(StorageMode::DurableLocalStandard, script_byte(script, 49))?,
        backend,
        timestamp_source(),
    );
    ensure(
        matches!(rejected, Err(LifecycleError::LowerLayer { .. })),
        "durable manifest publish fault was not typed",
    )?;
    outcome.durable_manifest_publish_fault_cases += 1;
    Ok(())
}

fn check_durable_wal_failure(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let backend: &'static DurableScriptBackend =
        Box::leak(Box::new(DurableScriptBackend::with_metadata_failure()));
    let manifest = DatabaseManifest::new(DATABASE_ID, "identity")
        .map_err(testkit_error)?
        .with_recovery_facts(u64::from(script_byte(script, 50) % 8) + 2, None, None, None)
        .map_err(testkit_error)?;
    backend.write_raw(
        ObjectLayout::database_manifest().map_err(testkit_error)?,
        encode_manifest(&manifest).map_err(testkit_error)?,
    );
    let rejected = LifecycleDurableLocalShell::assemble(
        request(StorageMode::DurableLocalStandard, script_byte(script, 51))?,
        backend,
        timestamp_source(),
    );
    ensure(
        matches!(rejected, Err(LifecycleError::LowerLayer { .. })),
        "durable WAL open failure was not typed",
    )?;
    ensure(
        backend.saw_wal_metadata(),
        "durable WAL failure did not reach active-segment metadata",
    )?;
    outcome.durable_wal_open_failure_cases += 1;
    Ok(())
}

fn check_input_derived_durable(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let mode = if script_byte(script, 52).is_multiple_of(2) {
        StorageMode::DurableLocalStandard
    } else {
        StorageMode::DurableLocalAlways
    };
    let backend: &'static DurableScriptBackend = Box::leak(Box::new(DurableScriptBackend::new()));
    let shell = assemble(mode, script_byte(script, 53), backend)?;
    ensure(
        shell.assembly_facts().mode() == mode,
        "input-derived durable mode was not preserved",
    )?;
    outcome.input_derived_durable_cases += 1;
    Ok(())
}

fn assemble(
    mode: StorageMode,
    branch_byte: u8,
    backend: &'static DurableScriptBackend,
) -> Result<LifecycleDurableLocalShell<'static>, TestkitError> {
    LifecycleDurableLocalShell::assemble(request(mode, branch_byte)?, backend, timestamp_source())
        .map_err(testkit_error)
}

fn request(
    mode: StorageMode,
    branch_byte: u8,
) -> Result<LifecycleDurableLocalOpenRequest, TestkitError> {
    LifecycleDurableLocalOpenRequest::new(
        open_plan(mode),
        DATABASE_ID,
        branch_id(branch_byte),
        CommitBranchGeneration::new(1).map_err(testkit_error)?,
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        WalServiceConfig::default()
            .with_append_buffer_bytes(crate::service::DEFAULT_WAL_APPEND_BUFFER_BYTES),
    )
    .map_err(testkit_error)
}

fn open_plan(mode: StorageMode) -> StorageOpenPlan {
    StorageOpenPlan::new(
        mode,
        LifecycleCodecId::identity(),
        RecoveryStrictness::Strict,
        LifecycleConfig::default(),
    )
    .expect("open plan")
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; 16])
}

fn timestamp_source() -> CommitManualTimestampSource {
    CommitManualTimestampSource::new(Timestamp::from_micros(9_000))
}

fn testkit_error(error: impl std::error::Error) -> TestkitError {
    TestkitError::new(error.to_string())
}

#[derive(Debug)]
struct DurableScriptBackend {
    capabilities: BackendCapabilities,
    objects: Mutex<BTreeMap<ObjectName, Vec<u8>>>,
    operations: Mutex<Vec<ScriptOperation>>,
    listed_prefixes: Mutex<Vec<String>>,
    lock_held: Arc<AtomicBool>,
    fail_lock: bool,
    publish_failure: Option<PublishFailureKind>,
    create_race_manifest: Mutex<Option<DatabaseManifest>>,
    fail_metadata: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScriptOperation {
    Capabilities,
    ReadObject,
    ObjectMetadata,
    WriterLock,
    Publish,
    ListPrefix,
    Other,
}

impl DurableScriptBackend {
    fn new() -> Self {
        Self::with_capabilities(BackendCapabilities::from_slice(
            DURABLE_LOCAL_MODE_REQUIREMENTS,
        ))
    }

    fn with_capabilities(capabilities: BackendCapabilities) -> Self {
        Self {
            capabilities,
            objects: Mutex::new(BTreeMap::new()),
            operations: Mutex::new(Vec::new()),
            listed_prefixes: Mutex::new(Vec::new()),
            lock_held: Arc::new(AtomicBool::new(false)),
            fail_lock: false,
            publish_failure: None,
            create_race_manifest: Mutex::new(None),
            fail_metadata: false,
        }
    }

    fn with_lock_failure() -> Self {
        Self {
            fail_lock: true,
            ..Self::new()
        }
    }

    fn with_publish_failure(kind: PublishFailureKind) -> Self {
        Self {
            publish_failure: Some(kind),
            ..Self::new()
        }
    }

    fn with_create_race(manifest: DatabaseManifest) -> Self {
        Self {
            create_race_manifest: Mutex::new(Some(manifest)),
            ..Self::new()
        }
    }

    fn with_metadata_failure() -> Self {
        Self {
            fail_metadata: true,
            ..Self::new()
        }
    }

    fn write_raw(&self, object: ObjectName, bytes: Vec<u8>) {
        self.objects.lock().expect("objects").insert(object, bytes);
    }

    fn operation_count(&self) -> usize {
        self.operations.lock().expect("operations").len()
    }

    fn saw_capabilities(&self) -> bool {
        self.saw(ScriptOperation::Capabilities)
    }

    fn saw_writer_lock(&self) -> bool {
        self.saw(ScriptOperation::WriterLock)
    }

    fn saw_read_object(&self) -> bool {
        self.saw(ScriptOperation::ReadObject)
    }

    fn listed_prefixes(&self) -> Vec<String> {
        self.listed_prefixes
            .lock()
            .expect("listed prefixes")
            .clone()
    }

    fn saw_wal_metadata(&self) -> bool {
        self.saw(ScriptOperation::ObjectMetadata)
    }

    fn saw(&self, operation: ScriptOperation) -> bool {
        self.operations
            .lock()
            .expect("operations")
            .contains(&operation)
    }

    fn record(&self, operation: ScriptOperation) {
        self.operations.lock().expect("operations").push(operation);
    }
}

impl Backend for DurableScriptBackend {
    fn capabilities(&self) -> BackendCapabilities {
        self.record(ScriptOperation::Capabilities);
        self.capabilities
    }

    fn read_object(&self, name: &ObjectName) -> BackendResult<Vec<u8>> {
        self.record(ScriptOperation::ReadObject);
        self.objects
            .lock()
            .expect("objects")
            .get(name)
            .cloned()
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "object not found"))
    }

    fn read_range(&self, name: &ObjectName, range: BackendRange) -> BackendResult<Vec<u8>> {
        self.record(ScriptOperation::Other);
        let bytes = self.read_object(name)?;
        let start = usize::try_from(range.offset()).unwrap_or(usize::MAX);
        let end = usize::try_from(range.end_offset().unwrap_or(u64::MAX)).unwrap_or(usize::MAX);
        Ok(bytes[start.min(bytes.len())..end.min(bytes.len())].to_vec())
    }

    fn write_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendMetadata> {
        self.record(ScriptOperation::Other);
        self.objects
            .lock()
            .expect("objects")
            .insert(name.clone(), bytes.to_vec());
        Ok(BackendMetadata::new(bytes.len() as u64, None))
    }

    fn delete_object(&self, name: &ObjectName) -> crate::backend::DeleteResult {
        self.record(ScriptOperation::Other);
        let removed = self.objects.lock().expect("objects").remove(name).is_some();
        crate::backend::durable_delete_result(name, removed)
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        self.record(ScriptOperation::ListPrefix);
        self.listed_prefixes
            .lock()
            .expect("listed prefixes")
            .push(prefix.as_str().to_owned());
        let mut names: Vec<_> = self
            .objects
            .lock()
            .expect("objects")
            .keys()
            .filter(|name| name.as_str().starts_with(prefix.as_str()))
            .cloned()
            .collect();
        names.sort();
        Ok(names)
    }

    fn object_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
        self.record(ScriptOperation::ObjectMetadata);
        if self.fail_metadata {
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "metadata unavailable",
            ));
        }
        self.objects
            .lock()
            .expect("objects")
            .get(name)
            .map(|bytes| BackendMetadata::new(bytes.len() as u64, None))
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "object not found"))
    }

    fn acquire_writer_lock(&self, name: &ObjectName) -> BackendResult<BackendWriterGuard> {
        self.record(ScriptOperation::WriterLock);
        if self.fail_lock || self.lock_held.swap(true, Ordering::SeqCst) {
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "writer lock unavailable",
            ));
        }
        Ok(BackendWriterGuard::new(
            name.clone(),
            ScriptWriterLock {
                locked: Arc::clone(&self.lock_held),
            },
        ))
    }

    fn append_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendAppend> {
        self.record(ScriptOperation::Other);
        let mut objects = self.objects.lock().expect("objects");
        let object = objects.entry(name.clone()).or_default();
        let start_offset = object.len() as u64;
        object.extend_from_slice(bytes);
        Ok(BackendAppend::new(
            start_offset,
            bytes.len() as u64,
            BackendMetadata::new(object.len() as u64, None),
        ))
    }

    fn sync_object(&self, _name: &ObjectName) -> crate::backend::BackendResult<()> {
        self.record(ScriptOperation::Other);
        Ok(())
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        bytes: &[u8],
        mode: PublishMode,
    ) -> PublishResult<PublishOutcome> {
        self.record(ScriptOperation::Publish);
        if mode == PublishMode::Create
            && name == &ObjectLayout::database_manifest().expect("database object")
        {
            if let Some(manifest) = self.create_race_manifest.lock().expect("race").take() {
                self.objects.lock().expect("objects").insert(
                    name.clone(),
                    encode_manifest(&manifest).expect("encoded race object"),
                );
                return Err(PublishError::precondition_failed(
                    name,
                    "object already exists",
                ));
            }
        }
        if let Some(kind) = self.publish_failure {
            return Err(PublishError::new(
                name.clone(),
                kind,
                BackendError::new(BackendErrorKind::Unavailable, "injected publish failure"),
            ));
        }
        let mut objects = self.objects.lock().expect("objects");
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

struct ScriptWriterLock {
    locked: Arc<AtomicBool>,
}

impl Drop for ScriptWriterLock {
    fn drop(&mut self) {
        self.locked.store(false, Ordering::SeqCst);
    }
}
