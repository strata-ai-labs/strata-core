//! Generated lifecycle flush contract helpers.

use super::{ensure, script_byte, testkit_error};
use crate::backend::{
    Backend, BackendCapabilities, BackendCapability, BackendError, BackendErrorKind,
    BackendMetadata, BackendRange, BackendResult, PublishDurability, PublishError,
    PublishFailureKind, PublishMode, PublishOutcome, PublishResult,
};
use crate::branch::state::BranchLocalState;
use crate::lifecycle::{
    flush_cache_branch, flush_durable_branch, FlushFrozenRequest, FlushTableIdentitySeed,
    FlushTableObjectId,
};
use crate::object::{ObjectName, ObjectPrefix};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::service::{TableObjectReaderService, TableObjectService};
use crate::testkit::TestkitError;
use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Mutex;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LifecycleFlushContractOutcome {
    cache_success: usize,
    durable_success: usize,
    deferred: usize,
    publish_failures: usize,
    reopen_failures: usize,
    retries: usize,
    read_parity: usize,
}

pub fn check_lifecycle_flush_contract(
    script: &[u8],
) -> Result<LifecycleFlushContractOutcome, TestkitError> {
    let mut outcome = LifecycleFlushContractOutcome::default();
    check_cache_success(script, &mut outcome)?;
    check_durable_success_and_retry(script, &mut outcome)?;
    check_deferred(script, &mut outcome)?;
    check_publish_failure(script, &mut outcome)?;
    check_reopen_failure(script, &mut outcome)?;
    Ok(outcome)
}

impl LifecycleFlushContractOutcome {
    pub const fn cache_success_cases(&self) -> usize {
        self.cache_success
    }

    pub const fn durable_success_cases(&self) -> usize {
        self.durable_success
    }

    pub const fn deferred_cases(&self) -> usize {
        self.deferred
    }

    pub const fn publish_failure_cases(&self) -> usize {
        self.publish_failures
    }

    pub const fn reopen_failure_cases(&self) -> usize {
        self.reopen_failures
    }

    pub const fn retry_cases(&self) -> usize {
        self.retries
    }

    pub const fn read_parity_cases(&self) -> usize {
        self.read_parity
    }
}

fn check_cache_success(
    script: &[u8],
    outcome: &mut LifecycleFlushContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 0));
    let key = physical_key(branch, b"cache");
    let row = put_row(
        branch,
        b"cache",
        version(script_byte(script, 1)),
        10_000,
        b"value",
    );
    let mut state = frozen_branch(branch, row);
    let before = latest_value(&state, &key)?;
    let flush = flush_cache_branch(&mut state, &request(branch, script, 2))
        .map_err(|error| testkit_error(&error))?;
    let after = latest_value(&state, &key)?;

    ensure(flush.completed(), "cache flush did not complete")?;
    ensure(before == after, "cache flush changed latest read result")?;
    ensure(
        state.frozen_table_count() == 0 && state.owned_table_count() == 1,
        "cache flush did not replace frozen state with owned table",
    )?;
    ensure(
        flush.table_object().is_none(),
        "cache flush reported durable table object",
    )?;
    outcome.cache_success += 1;
    outcome.read_parity += 1;
    Ok(())
}

fn check_durable_success_and_retry(
    script: &[u8],
    outcome: &mut LifecycleFlushContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 3));
    let row = put_row(
        branch,
        b"durable",
        version(script_byte(script, 4)),
        20_000,
        b"value",
    );
    let backend: &'static FlushContractBackend = Box::leak(Box::new(FlushContractBackend::new()));
    let flush_request = request(branch, script, 5);
    let mut first = frozen_branch(branch, row.clone());
    let first_outcome = flush_durable_branch(
        &mut first,
        &TableObjectService::new(backend),
        &TableObjectReaderService::new(backend),
        &flush_request,
    )
    .map_err(|error| testkit_error(&error))?;

    ensure(first_outcome.completed(), "durable flush did not complete")?;
    ensure(
        backend.object_count() == 1,
        "durable flush did not publish exactly one object",
    )?;
    ensure(
        first.frozen_table_count() == 0 && first.owned_table_count() == 1,
        "durable flush did not install owned table",
    )?;

    let mut retry = frozen_branch(branch, row);
    let retry_outcome = flush_durable_branch(
        &mut retry,
        &TableObjectService::new(backend),
        &TableObjectReaderService::new(backend),
        &flush_request,
    )
    .map_err(|error| testkit_error(&error))?;
    ensure(
        retry_outcome.completed(),
        "matching durable retry did not complete",
    )?;
    ensure(
        retry_outcome.table_object() == first_outcome.table_object(),
        "matching durable retry changed object identity",
    )?;

    outcome.durable_success += 1;
    outcome.retries += 1;
    Ok(())
}

fn check_deferred(
    script: &[u8],
    outcome: &mut LifecycleFlushContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 6));
    let mut state = BranchLocalState::empty(branch);
    let flush = flush_cache_branch(&mut state, &request(branch, script, 7))
        .map_err(|error| testkit_error(&error))?;
    ensure(
        flush.deferred_no_frozen_state(),
        "empty flush did not defer",
    )?;
    ensure(state.is_empty(), "empty flush mutated branch state")?;
    outcome.deferred += 1;
    Ok(())
}

fn check_publish_failure(
    script: &[u8],
    outcome: &mut LifecycleFlushContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 8));
    let row = put_row(
        branch,
        b"publish-failure",
        version(script_byte(script, 9)),
        30_000,
        b"value",
    );
    let mut state = frozen_branch(branch, row);
    let before = state.clone();
    let backend: &'static FlushContractBackend =
        Box::leak(Box::new(FlushContractBackend::with_publish_failure()));
    let error = flush_durable_branch(
        &mut state,
        &TableObjectService::new(backend),
        &TableObjectReaderService::new(backend),
        &request(branch, script, 10),
    )
    .expect_err("publish failure");

    ensure(
        error.source().is_some(),
        "publish failure did not preserve source chain",
    )?;
    ensure(state == before, "publish failure mutated frozen state")?;
    outcome.publish_failures += 1;
    Ok(())
}

fn check_reopen_failure(
    script: &[u8],
    outcome: &mut LifecycleFlushContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 11));
    let row = put_row(
        branch,
        b"reopen-failure",
        version(script_byte(script, 12)),
        40_000,
        b"value",
    );
    let mut state = frozen_branch(branch, row);
    let before = state.clone();
    let backend: &'static FlushContractBackend =
        Box::leak(Box::new(FlushContractBackend::with_range_failure()));
    let flush = flush_durable_branch(
        &mut state,
        &TableObjectService::new(backend),
        &TableObjectReaderService::new(backend),
        &request(branch, script, 13),
    )
    .map_err(|error| testkit_error(&error))?;

    ensure(
        flush.published_not_installed(),
        "reopen failure did not report partial publication",
    )?;
    ensure(
        flush.failure().is_some(),
        "reopen failure did not preserve typed failure",
    )?;
    ensure(state == before, "reopen failure removed frozen state")?;
    outcome.reopen_failures += 1;
    Ok(())
}

fn request(branch: BranchId, script: &[u8], offset: usize) -> FlushFrozenRequest {
    FlushFrozenRequest::new(
        branch,
        None,
        FlushTableIdentitySeed::new(format!("flush-seed-{}", script_byte(script, offset)))
            .expect("seed"),
        FlushTableObjectId::new(format!("flush-object-{}", script_byte(script, offset + 1)))
            .expect("object id"),
    )
    .expect("flush request")
}

fn frozen_branch(branch: BranchId, row: StorageRow) -> BranchLocalState {
    let mut state = BranchLocalState::empty(branch);
    state.append_committed_row(row).expect("append row");
    state.rotate_active();
    state
}

fn latest_value(
    state: &BranchLocalState,
    key: &PhysicalKey,
) -> Result<Option<Vec<u8>>, TestkitError> {
    Ok(state
        .capture_read_view()
        .map_err(|error| TestkitError::new(format!("branch read failed: {error}")))?
        .latest(key)
        .map_err(|error| TestkitError::new(format!("branch read failed: {error}")))?
        .map(|row| row.row().value().to_vec()))
}

fn put_row(
    branch: BranchId,
    user_key: &[u8],
    commit_version: CommitVersion,
    commit_timestamp: u64,
    value: &[u8],
) -> StorageRow {
    StorageRow::put(
        physical_key(branch, user_key),
        commit_version,
        Timestamp::from_micros(commit_timestamp),
        Timestamp::EPOCH,
        value.to_vec(),
    )
}

fn physical_key(branch: BranchId, user_key: &[u8]) -> PhysicalKey {
    PhysicalKey::new(
        branch,
        "flush",
        StorageSpaceId::engine(0x55).expect("engine space"),
        user_key.to_vec(),
    )
    .expect("physical key")
}

fn branch_id(seed: u8) -> BranchId {
    BranchId::from_bytes([seed.max(1); BranchId::BYTE_LEN])
}

fn version(seed: u8) -> CommitVersion {
    CommitVersion::new(u64::from(seed) + 1)
}

#[derive(Debug)]
struct FlushContractBackend {
    objects: Mutex<BTreeMap<ObjectName, Vec<u8>>>,
    publish_failure: bool,
    range_failure: bool,
}

impl FlushContractBackend {
    fn new() -> Self {
        Self {
            objects: Mutex::new(BTreeMap::new()),
            publish_failure: false,
            range_failure: false,
        }
    }

    fn with_publish_failure() -> Self {
        Self {
            publish_failure: true,
            ..Self::new()
        }
    }

    fn with_range_failure() -> Self {
        Self {
            range_failure: true,
            ..Self::new()
        }
    }

    fn object_count(&self) -> usize {
        self.objects.lock().expect("objects").len()
    }
}

impl Backend for FlushContractBackend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::from_slice(&[
            BackendCapability::ReadRange,
            BackendCapability::ObjectMetadata,
            BackendCapability::DurablePublish,
            BackendCapability::DurableSync,
        ])
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
        if self.range_failure {
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "range read failed",
            ));
        }
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

    fn delete_object(&self, name: &ObjectName) -> crate::backend::DeleteResult {
        let removed = self.objects.lock().expect("objects").remove(name).is_some();
        crate::backend::durable_delete_result(name, removed)
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
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

    fn publish_object(
        &self,
        name: &ObjectName,
        bytes: &[u8],
        mode: PublishMode,
    ) -> PublishResult<PublishOutcome> {
        if self.publish_failure {
            return Err(PublishError::new(
                name.clone(),
                PublishFailureKind::FailedBeforeVisibility,
                BackendError::new(BackendErrorKind::Unavailable, "publish failed"),
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
