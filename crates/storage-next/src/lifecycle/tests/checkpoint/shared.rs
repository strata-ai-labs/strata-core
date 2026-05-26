use super::*;

pub(in crate::lifecycle::tests) fn open_runtime(
    branch: BranchId,
    backend: &CheckpointTestBackend,
) -> LifecycleDurableLocalRuntime<'_, CommitManualTimestampSource> {
    let mut shell = assemble_shell(branch, backend).expect("shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("recovery outcome");
    shell.complete_recovery(&outcome).expect("open runtime")
}

pub(super) fn open_runtime_with_wal_segment_size(
    branch: BranchId,
    backend: &CheckpointTestBackend,
    segment_size: u64,
) -> LifecycleDurableLocalRuntime<'_, CommitManualTimestampSource> {
    let mut shell =
        assemble_shell_with_wal_segment_size(branch, backend, segment_size).expect("shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("recovery outcome");
    shell.complete_recovery(&outcome).expect("open runtime")
}

pub(in crate::lifecycle::tests) fn assemble_shell(
    branch: BranchId,
    backend: &CheckpointTestBackend,
) -> LifecycleResult<LifecycleDurableLocalShell<'_>> {
    assemble_shell_with_wal_segment_size(
        branch,
        backend,
        crate::service::WalServiceConfig::default().segment_size(),
    )
}

pub(super) fn assemble_shell_with_wal_segment_size(
    branch: BranchId,
    backend: &CheckpointTestBackend,
    segment_size: u64,
) -> LifecycleResult<LifecycleDurableLocalShell<'_>> {
    LifecycleDurableLocalShell::assemble(
        LifecycleDurableLocalOpenRequest::new(
            StorageOpenPlan::new(
                StorageMode::DurableLocalStandard,
                LifecycleCodecId::identity(),
                RecoveryStrictness::Strict,
                LifecycleConfig::default(),
            )
            .expect("open plan"),
            DATABASE_ID,
            branch,
            CommitBranchGeneration::new(1).expect("generation"),
            BranchRuntimeConfig::default(),
            CommitRuntimeConfig::default(),
            crate::service::WalServiceConfig::new(segment_size),
        )?,
        backend,
        CommitManualTimestampSource::new(Timestamp::from_micros(9_000)),
    )
}

pub(in crate::lifecycle::tests) fn durable_batch(
    branch: BranchId,
    user_key: &'static [u8],
    value: &'static [u8],
) -> CommitBatch {
    CommitBatch::mutating(
        branch,
        vec![CommitMutation::put(
            physical_key(branch, user_key),
            value.to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::new(
            CommitDurabilityMode::Standard,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::RuntimeGenerated,
            CommitOrigin::StorageRuntime,
        ),
    )
}

pub(in crate::lifecycle::tests) fn generation_guard() -> CommitBranchGenerationGuard {
    CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation"))
}

pub(super) fn flush_request(branch: BranchId) -> FlushFrozenRequest {
    FlushFrozenRequest::new(
        branch,
        None,
        FlushTableIdentitySeed::new(format!("checkpoint-flush-{branch}")).expect("seed"),
        FlushTableObjectId::new(format!("checkpoint-object-{branch}")).expect("object"),
    )
    .expect("flush request")
}

pub(super) fn put_row(
    branch: BranchId,
    version: u64,
    user_key: &'static [u8],
    value: &'static [u8],
) -> StorageRow {
    StorageRow::put(
        physical_key(branch, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(version * 100),
        Timestamp::EPOCH,
        value.to_vec(),
    )
}

pub(in crate::lifecycle::tests) fn physical_key(
    branch: BranchId,
    user_key: &'static [u8],
) -> PhysicalKey {
    PhysicalKey::new(
        branch,
        "checkpoint",
        StorageSpaceId::engine(0x30).expect("space"),
        user_key.to_vec(),
    )
    .expect("physical key")
}

pub(in crate::lifecycle::tests) fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; 16])
}

pub(super) fn maintenance_task_for_test(
    id: u64,
    request: MaintenanceTaskRequest,
) -> MaintenanceTask {
    MaintenanceTask::new_for_test(id, request).expect("maintenance task")
}

#[derive(Debug)]
pub(in crate::lifecycle::tests) struct CheckpointTestBackend {
    objects: Mutex<BTreeMap<ObjectName, Vec<u8>>>,
    events: Mutex<Vec<CheckpointBackendEvent>>,
    fail_list: AtomicBool,
    fail_snapshot_publish: AtomicBool,
    fail_delete: AtomicBool,
    fail_delete_call: AtomicUsize,
    delete_calls: AtomicUsize,
    fail_manifest_replace_call: AtomicUsize,
    fail_table_manifest_replace_call: AtomicUsize,
    uncertain_table_manifest_replace_call: AtomicUsize,
    fail_table_object_create_call: AtomicUsize,
    uncertain_table_object_create_call: AtomicUsize,
    corrupt_table_object_create_call: AtomicUsize,
    uncertain_manifest_replace_call: AtomicUsize,
    manifest_replace_calls: AtomicUsize,
    table_manifest_replace_calls: AtomicUsize,
    table_object_create_calls: AtomicUsize,
    lock_held: Arc<AtomicBool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::lifecycle::tests) enum CheckpointBackendEvent {
    DatabaseRecordReplace,
    SnapshotCreate,
    TableObjectCreate,
    TableManifestReplace,
    ObjectDelete,
    ObjectList,
}

impl CheckpointTestBackend {
    pub(in crate::lifecycle::tests) fn new() -> Self {
        Self {
            objects: Mutex::new(BTreeMap::new()),
            events: Mutex::new(Vec::new()),
            fail_list: AtomicBool::new(false),
            fail_snapshot_publish: AtomicBool::new(false),
            fail_delete: AtomicBool::new(false),
            fail_delete_call: AtomicUsize::new(0),
            delete_calls: AtomicUsize::new(0),
            fail_manifest_replace_call: AtomicUsize::new(0),
            fail_table_manifest_replace_call: AtomicUsize::new(0),
            uncertain_table_manifest_replace_call: AtomicUsize::new(0),
            fail_table_object_create_call: AtomicUsize::new(0),
            uncertain_table_object_create_call: AtomicUsize::new(0),
            corrupt_table_object_create_call: AtomicUsize::new(0),
            uncertain_manifest_replace_call: AtomicUsize::new(0),
            manifest_replace_calls: AtomicUsize::new(0),
            table_manifest_replace_calls: AtomicUsize::new(0),
            table_object_create_calls: AtomicUsize::new(0),
            lock_held: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(super) fn fail_wal_listing(&self) {
        self.fail_list.store(true, Ordering::SeqCst);
    }

    pub(super) fn fail_snapshot_publish(&self) {
        self.fail_snapshot_publish.store(true, Ordering::SeqCst);
    }

    pub(super) fn fail_delete(&self) {
        self.fail_delete.store(true, Ordering::SeqCst);
    }

    pub(super) fn fail_delete_on_call(&self, call: usize) {
        self.fail_delete_call.store(call, Ordering::SeqCst);
    }

    pub(in crate::lifecycle::tests) fn fail_manifest_replacement_on_call(&self, call: usize) {
        self.fail_manifest_replace_call
            .store(call, Ordering::SeqCst);
    }

    pub(in crate::lifecycle::tests) fn fail_table_manifest_replacement_on_call(&self, call: usize) {
        self.fail_table_manifest_replace_call
            .store(call, Ordering::SeqCst);
    }

    pub(in crate::lifecycle::tests) fn uncertain_table_manifest_replacement_on_call(
        &self,
        call: usize,
    ) {
        self.uncertain_table_manifest_replace_call
            .store(call, Ordering::SeqCst);
    }

    pub(in crate::lifecycle::tests) fn fail_table_object_create_on_call(&self, call: usize) {
        self.fail_table_object_create_call
            .store(call, Ordering::SeqCst);
    }

    pub(in crate::lifecycle::tests) fn uncertain_table_object_create_on_call(&self, call: usize) {
        self.uncertain_table_object_create_call
            .store(call, Ordering::SeqCst);
    }

    pub(in crate::lifecycle::tests) fn corrupt_table_object_create_on_call(&self, call: usize) {
        self.corrupt_table_object_create_call
            .store(call, Ordering::SeqCst);
    }

    pub(in crate::lifecycle::tests) fn uncertain_manifest_replacement_on_call(&self, call: usize) {
        self.uncertain_manifest_replace_call
            .store(call, Ordering::SeqCst);
    }

    pub(super) fn event_count(&self) -> usize {
        self.events.lock().expect("events").len()
    }

    pub(super) fn checkpoint_events(&self) -> Vec<CheckpointBackendEvent> {
        self.events
            .lock()
            .expect("events")
            .iter()
            .copied()
            .filter(|event| {
                matches!(
                    event,
                    CheckpointBackendEvent::DatabaseRecordReplace
                        | CheckpointBackendEvent::SnapshotCreate
                )
            })
            .collect()
    }

    pub(in crate::lifecycle::tests) fn events(&self) -> Vec<CheckpointBackendEvent> {
        self.events.lock().expect("events").clone()
    }

    pub(super) fn list_calls(&self) -> usize {
        self.events
            .lock()
            .expect("events")
            .iter()
            .filter(|event| matches!(event, CheckpointBackendEvent::ObjectList))
            .count()
    }

    pub(in crate::lifecycle::tests) fn delete_calls(&self) -> usize {
        self.delete_calls.load(Ordering::SeqCst)
    }

    pub(in crate::lifecycle::tests) fn table_object_create_calls(&self) -> usize {
        self.table_object_create_calls.load(Ordering::SeqCst)
    }

    pub(in crate::lifecycle::tests) fn table_manifest_replace_calls(&self) -> usize {
        self.table_manifest_replace_calls.load(Ordering::SeqCst)
    }

    pub(in crate::lifecycle::tests) fn table_object_names(&self) -> Vec<ObjectName> {
        let objects = self.objects.lock().expect("objects");
        let mut names = objects
            .keys()
            .filter(|name| {
                name.as_str().starts_with("tables/") && !name.as_str().ends_with("/manifest")
            })
            .cloned()
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    pub(in crate::lifecycle::tests) fn snapshot_objects(&self) -> Vec<ObjectName> {
        let objects = self.objects.lock().expect("objects");
        let mut snapshots = objects
            .keys()
            .filter(|name| decode_snapshot_object(objects.get(*name).expect("bytes")))
            .cloned()
            .collect::<Vec<_>>();
        snapshots.sort();
        snapshots
    }

    pub(super) fn snapshot_created_at(&self) -> Vec<Timestamp> {
        let objects = self.objects.lock().expect("objects");
        let mut timestamps = objects
            .values()
            .filter_map(|bytes| {
                decode_snapshot_container(bytes)
                    .ok()
                    .map(|container| container.header().created_at())
            })
            .collect::<Vec<_>>();
        timestamps.sort();
        timestamps
    }

    fn classify_publish(
        name: &ObjectName,
        bytes: &[u8],
        mode: PublishMode,
    ) -> CheckpointPublishKind {
        if mode == PublishMode::Replace
            && *name == ObjectLayout::database_manifest().expect("current database object")
        {
            CheckpointPublishKind::DatabaseRecord
        } else if mode == PublishMode::Replace
            && name.as_str().starts_with("tables/")
            && name.as_str().ends_with("/manifest")
        {
            CheckpointPublishKind::TableManifestRecord
        } else if mode == PublishMode::Create
            && name.as_str().starts_with("tables/")
            && !name.as_str().ends_with("/manifest")
        {
            CheckpointPublishKind::TableObjectCreate
        } else if mode == PublishMode::Create && decode_snapshot_container(bytes).is_ok() {
            CheckpointPublishKind::SnapshotCreate
        } else {
            CheckpointPublishKind::Other
        }
    }

    fn record_publish_event(&self, kind: CheckpointPublishKind) {
        let event = match kind {
            CheckpointPublishKind::DatabaseRecord => {
                Some(CheckpointBackendEvent::DatabaseRecordReplace)
            }
            CheckpointPublishKind::TableManifestRecord => {
                Some(CheckpointBackendEvent::TableManifestReplace)
            }
            CheckpointPublishKind::TableObjectCreate => {
                Some(CheckpointBackendEvent::TableObjectCreate)
            }
            CheckpointPublishKind::SnapshotCreate => Some(CheckpointBackendEvent::SnapshotCreate),
            CheckpointPublishKind::Other => None,
        };
        if let Some(event) = event {
            self.events.lock().expect("events").push(event);
        }
    }

    fn maybe_fail_publish(
        &self,
        name: &ObjectName,
        kind: CheckpointPublishKind,
    ) -> PublishResult<()> {
        match kind {
            CheckpointPublishKind::SnapshotCreate => {
                if self.fail_snapshot_publish.load(Ordering::SeqCst) {
                    return Err(PublishError::new(
                        name.clone(),
                        PublishFailureKind::FailedBeforeVisibility,
                        BackendError::new(
                            BackendErrorKind::Unavailable,
                            "injected snapshot create failure",
                        ),
                    ));
                }
            }
            CheckpointPublishKind::DatabaseRecord => {
                self.maybe_fail_database_record_publish(name)?;
            }
            CheckpointPublishKind::TableManifestRecord => {
                self.maybe_fail_table_manifest_publish(name)?;
            }
            CheckpointPublishKind::TableObjectCreate => {
                self.maybe_fail_table_object_publish(name)?;
            }
            CheckpointPublishKind::Other => {}
        }
        Ok(())
    }

    fn maybe_fail_database_record_publish(&self, name: &ObjectName) -> PublishResult<()> {
        let call = self
            .manifest_replace_calls
            .fetch_add(1, Ordering::SeqCst)
            .saturating_add(1);
        if self.fail_manifest_replace_call.load(Ordering::SeqCst) == call {
            return Err(PublishError::precondition_failed(
                name,
                "injected current record replace failure",
            ));
        }
        if self.uncertain_manifest_replace_call.load(Ordering::SeqCst) == call {
            return Err(PublishError::new(
                name.clone(),
                PublishFailureKind::VisibilityUnknown,
                BackendError::new(
                    BackendErrorKind::Unavailable,
                    "injected current record visibility uncertainty",
                ),
            ));
        }
        Ok(())
    }

    fn maybe_fail_table_manifest_publish(&self, name: &ObjectName) -> PublishResult<()> {
        let call = self
            .table_manifest_replace_calls
            .fetch_add(1, Ordering::SeqCst)
            .saturating_add(1);
        if self.fail_table_manifest_replace_call.load(Ordering::SeqCst) == call {
            return Err(PublishError::precondition_failed(
                name,
                "injected table manifest replace failure",
            ));
        }
        if self
            .uncertain_table_manifest_replace_call
            .load(Ordering::SeqCst)
            == call
        {
            return Err(PublishError::new(
                name.clone(),
                PublishFailureKind::VisibilityUnknown,
                BackendError::new(
                    BackendErrorKind::Unavailable,
                    "injected table manifest visibility uncertainty",
                ),
            ));
        }
        Ok(())
    }

    fn maybe_fail_table_object_publish(&self, name: &ObjectName) -> PublishResult<()> {
        let call = self
            .table_object_create_calls
            .fetch_add(1, Ordering::SeqCst)
            .saturating_add(1);
        if self.fail_table_object_create_call.load(Ordering::SeqCst) == call {
            return Err(PublishError::new(
                name.clone(),
                PublishFailureKind::FailedBeforeVisibility,
                BackendError::new(
                    BackendErrorKind::Unavailable,
                    "injected table object create failure",
                ),
            ));
        }
        if self
            .uncertain_table_object_create_call
            .load(Ordering::SeqCst)
            == call
        {
            return Err(PublishError::new(
                name.clone(),
                PublishFailureKind::VisibilityUnknown,
                BackendError::new(
                    BackendErrorKind::Unavailable,
                    "injected table object visibility uncertainty",
                ),
            ));
        }
        Ok(())
    }

    fn published_bytes(&self, kind: CheckpointPublishKind, bytes: &[u8]) -> Vec<u8> {
        if kind == CheckpointPublishKind::TableObjectCreate
            && self.corrupt_table_object_create_call.load(Ordering::SeqCst)
                == self.table_object_create_calls.load(Ordering::SeqCst)
        {
            let mut corrupted = bytes.to_vec();
            if let Some(first) = corrupted.first_mut() {
                *first ^= 0xff;
            }
            return corrupted;
        }
        bytes.to_vec()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CheckpointPublishKind {
    DatabaseRecord,
    TableManifestRecord,
    TableObjectCreate,
    SnapshotCreate,
    Other,
}

impl Backend for CheckpointTestBackend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::from_slice(DURABLE_LOCAL_MODE_REQUIREMENTS)
    }

    fn read_object(&self, name: &ObjectName) -> BackendResult<Vec<u8>> {
        self.objects
            .lock()
            .expect("objects")
            .get(name)
            .cloned()
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "object not found"))
    }

    fn read_range(&self, name: &ObjectName, range: BackendRange) -> BackendResult<Vec<u8>> {
        let bytes = self.read_object(name)?;
        let start = usize::try_from(range.offset()).unwrap_or(usize::MAX);
        let end = usize::try_from(range.end_offset().unwrap_or(u64::MAX)).unwrap_or(usize::MAX);
        Ok(bytes[start.min(bytes.len())..end.min(bytes.len())].to_vec())
    }

    fn write_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendMetadata> {
        self.objects
            .lock()
            .expect("objects")
            .insert(name.clone(), bytes.to_vec());
        Ok(BackendMetadata::new(bytes.len() as u64, None))
    }

    fn delete_object(&self, name: &ObjectName) -> BackendResult<()> {
        self.events
            .lock()
            .expect("events")
            .push(CheckpointBackendEvent::ObjectDelete);
        let object_exists = self.objects.lock().expect("objects").contains_key(name);
        let call = if object_exists {
            self.delete_calls
                .fetch_add(1, Ordering::SeqCst)
                .saturating_add(1)
        } else {
            0
        };
        if object_exists
            && (self.fail_delete.load(Ordering::SeqCst)
                || self.fail_delete_call.load(Ordering::SeqCst) == call)
        {
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "injected delete failure",
            ));
        }
        self.objects.lock().expect("objects").remove(name);
        Ok(())
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        self.events
            .lock()
            .expect("events")
            .push(CheckpointBackendEvent::ObjectList);
        if self.fail_list.load(Ordering::SeqCst) {
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "injected list failure",
            ));
        }
        let mut names = self
            .objects
            .lock()
            .expect("objects")
            .keys()
            .filter(|name| name.as_str().starts_with(prefix.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        names.sort();
        Ok(names)
    }

    fn object_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
        self.objects
            .lock()
            .expect("objects")
            .get(name)
            .map(|bytes| BackendMetadata::new(bytes.len() as u64, None))
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "object not found"))
    }

    fn acquire_writer_lock(&self, name: &ObjectName) -> BackendResult<BackendWriterGuard> {
        if self.lock_held.swap(true, Ordering::SeqCst) {
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "writer lock already held",
            ));
        }
        Ok(BackendWriterGuard::new(
            name.clone(),
            HeldWriterLock {
                locked: Arc::clone(&self.lock_held),
            },
        ))
    }

    fn append_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendAppend> {
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

    fn sync_object(&self, _name: &ObjectName) -> BackendResult<()> {
        Ok(())
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        bytes: &[u8],
        mode: PublishMode,
    ) -> PublishResult<PublishOutcome> {
        let kind = Self::classify_publish(name, bytes, mode);
        self.record_publish_event(kind);
        self.maybe_fail_publish(name, kind)?;
        let mut objects = self.objects.lock().expect("objects");
        if mode == PublishMode::Create && objects.contains_key(name) {
            return Err(PublishError::precondition_failed(
                name,
                "object already exists",
            ));
        }
        let stored_bytes = self.published_bytes(kind, bytes);
        let byte_count = stored_bytes.len() as u64;
        objects.insert(name.clone(), stored_bytes);
        Ok(PublishOutcome::new(
            name.clone(),
            BackendMetadata::new(byte_count, None),
            PublishDurability::Durable,
        ))
    }
}

fn decode_snapshot_object(bytes: &[u8]) -> bool {
    decode_snapshot_container(bytes).is_ok()
}

struct HeldWriterLock {
    locked: Arc<AtomicBool>,
}

impl Drop for HeldWriterLock {
    fn drop(&mut self) {
        self.locked.store(false, Ordering::SeqCst);
    }
}
