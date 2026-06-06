use super::*;
use crate::service::wal::WalOperation;
use std::collections::VecDeque;

struct SyncFaultBackend {
    inner: StoredWalBackend,
    failures: Mutex<VecDeque<BackendErrorKind>>,
    sync_calls: Mutex<Vec<ObjectName>>,
}

impl SyncFaultBackend {
    fn new(failures: impl IntoIterator<Item = BackendErrorKind>) -> Self {
        Self {
            inner: StoredWalBackend::new(),
            failures: Mutex::new(failures.into_iter().collect()),
            sync_calls: Mutex::new(Vec::new()),
        }
    }

    fn sync_calls(&self) -> Vec<ObjectName> {
        self.sync_calls.lock().expect("sync calls lock").clone()
    }
}

impl Backend for SyncFaultBackend {
    fn capabilities(&self) -> BackendCapabilities {
        self.inner.capabilities()
    }

    fn read_object(&self, name: &ObjectName) -> BackendResult<Vec<u8>> {
        self.inner.read_object(name)
    }

    fn read_range(&self, name: &ObjectName, range: BackendRange) -> BackendResult<Vec<u8>> {
        self.inner.read_range(name, range)
    }

    fn write_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendMetadata> {
        self.inner.write_object(name, bytes)
    }

    fn delete_object(&self, name: &ObjectName) -> crate::backend::DeleteResult {
        self.inner.delete_object(name)
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        self.inner.list_prefix(prefix)
    }

    fn object_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
        self.inner.object_metadata(name)
    }

    fn append_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendAppend> {
        self.inner.append_object(name, bytes)
    }

    fn sync_object(&self, name: &ObjectName) -> crate::backend::BackendResult<()> {
        // Calls are recorded before the injected result so tests can prove a
        // failed durability barrier was actually attempted.
        self.sync_calls
            .lock()
            .expect("sync calls lock")
            .push(name.clone());
        if let Some(kind) = self
            .failures
            .lock()
            .expect("sync failures lock")
            .pop_front()
        {
            Err(BackendError::new(kind, "injected sync failure"))
        } else {
            Ok(())
        }
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        bytes: &[u8],
        mode: PublishMode,
    ) -> PublishResult<PublishOutcome> {
        self.inner.publish_object(name, bytes, mode)
    }
}

fn assert_sync_error(error: WalServiceError, object: &ObjectName, kind: BackendErrorKind) {
    match error {
        WalServiceError::Backend {
            operation,
            object: actual_object,
            source,
        } => {
            assert_eq!(operation, WalOperation::Sync);
            assert_eq!(actual_object, *object);
            assert_eq!(source.kind(), kind);
        }
        other => panic!("expected WAL sync backend error, got {other:?}"),
    }
}

fn assert_metadata_matches_records(
    service: &WalService<'_>,
    segment_id: u64,
    records: &[WalRecord],
) {
    let metadata = service.active_metadata();
    assert_eq!(metadata.segment_id(), segment_id);
    assert_eq!(metadata.record_count(), records.len() as u64);
    assert_eq!(
        metadata.min_commit_version(),
        records
            .iter()
            .map(WalRecord::commit_version)
            .min()
            .expect("metadata records")
    );
    assert_eq!(
        metadata.max_commit_version(),
        records
            .iter()
            .map(WalRecord::commit_version)
            .max()
            .expect("metadata records")
    );
    assert_eq!(
        metadata.min_timestamp(),
        records
            .iter()
            .map(WalRecord::commit_timestamp)
            .min()
            .expect("metadata records")
    );
    assert_eq!(
        metadata.max_timestamp(),
        records
            .iter()
            .map(WalRecord::commit_timestamp)
            .max()
            .expect("metadata records")
    );
}

#[test]
fn standard_append_tracks_dirty_state_until_forced_durable() {
    let backend = SyncFaultBackend::new([]);
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");
    let first = record(1, b"standard dirty".to_vec());
    let frame_len = record_frame_len(&first);
    let segment_one = ObjectLayout::wal_segment(1).expect("segment one");

    let append = service.append(&first).expect("append standard record");

    assert_eq!(append.segment_id(), 1);
    assert_eq!(append.start_offset(), WAL_SEGMENT_HEADER_SIZE as u64);
    assert_eq!(append.bytes_written(), frame_len);
    assert_eq!(append.dirty_bytes(), frame_len);
    assert!(!append.forced_durable());
    assert_eq!(service.dirty_bytes(), frame_len);
    assert_eq!(service.dirty_records(), 1);
    assert!(backend.sync_calls().is_empty());

    service.force_durable().expect("force WAL durable");

    assert_eq!(backend.sync_calls(), vec![segment_one]);
    assert_eq!(service.dirty_bytes(), 0);
    assert_eq!(service.dirty_records(), 0);
}

#[test]
fn standard_close_syncs_dirty_state_and_clears_counters() {
    let backend = SyncFaultBackend::new([]);
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");
    let first = record(1, b"standard close".to_vec());
    let frame_len = record_frame_len(&first);
    service.append(&first).expect("append before close");
    assert_eq!(service.dirty_bytes(), frame_len);
    assert_eq!(service.dirty_records(), 1);

    service.close().expect("close WAL");

    assert_eq!(
        backend.sync_calls(),
        vec![ObjectLayout::wal_segment(1).expect("segment one")]
    );
    assert_eq!(service.dirty_bytes(), 0);
    assert_eq!(service.dirty_records(), 0);
    assert_ne!(frame_len, 0);
}

#[test]
fn standard_close_always_issues_one_sync_even_when_clean() {
    // Close must issue a SyncObject on every call, regardless of dirty
    // state. This guarantees a partial-close retry (first close cleared
    // dirty bytes but failed downstream) still produces an observable
    // sync on the retry attempt, so the close outcome's
    // `durable_synced=true` is backed by a fresh syscall every time.
    let backend = SyncFaultBackend::new([]);
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");

    service.close().expect("clean close issues sync");

    assert_eq!(service.dirty_bytes(), 0);
    assert_eq!(service.dirty_records(), 0);
    assert_eq!(
        backend.sync_calls().len(),
        1,
        "clean close must still issue exactly one SyncObject for retry observability"
    );
}

#[test]
fn standard_close_sync_failure_surfaces_typed_error() {
    // Replaces the prior "skip sync" expectation. With the always-sync
    // contract, an injected sync fault on close MUST surface as an
    // `Err(WalServiceError::Backend { operation: Sync, .. })` so the
    // lifecycle close path can route the close-time WAL sync failure
    // back to retry pending.
    let backend = SyncFaultBackend::new([BackendErrorKind::Unavailable]);
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");

    let error = service
        .close()
        .expect_err("close must propagate the sync fault");
    assert!(matches!(
        error,
        WalServiceError::Backend {
            operation: WalOperation::Sync,
            ..
        }
    ));
}

#[test]
fn standard_force_durable_sync_failure_leaves_dirty_state_intact() {
    let backend = SyncFaultBackend::new([BackendErrorKind::Unavailable]);
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");
    let first = record(1, b"force failure".to_vec());
    let frame_len = record_frame_len(&first);
    let segment_one = ObjectLayout::wal_segment(1).expect("segment one");
    service.append(&first).expect("append before force");

    let error = service
        .force_durable()
        .expect_err("sync failure should keep dirty state");

    assert_sync_error(error, &segment_one, BackendErrorKind::Unavailable);
    assert_eq!(service.dirty_bytes(), frame_len);
    assert_eq!(service.dirty_records(), 1);
    assert_eq!(backend.sync_calls(), vec![segment_one.clone()]);
    let read = service.read_all().expect("read after failed sync");
    assert_eq!(read.records(), std::slice::from_ref(&first));
    assert_eq!(read.truncation(), None);

    service
        .force_durable()
        .expect("retry force durable after transient failure");

    assert_eq!(backend.sync_calls(), vec![segment_one.clone(), segment_one]);
    assert_eq!(service.dirty_bytes(), 0);
    assert_eq!(service.dirty_records(), 0);
}

#[test]
fn standard_close_sync_failure_returns_typed_error_and_leaves_dirty_state_intact() {
    let backend = SyncFaultBackend::new([BackendErrorKind::Interrupted]);
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");
    let first = record(1, b"close failure".to_vec());
    let frame_len = record_frame_len(&first);
    service.append(&first).expect("append before close");
    let segment_one = ObjectLayout::wal_segment(1).expect("segment one");

    let error = service
        .close()
        .expect_err("close should return sync failure");

    assert_sync_error(error, &segment_one, BackendErrorKind::Interrupted);
    assert_eq!(service.dirty_bytes(), frame_len);
    assert_eq!(service.dirty_records(), 1);

    service.close().expect("close retry");

    assert_eq!(service.dirty_bytes(), 0);
    assert_eq!(service.dirty_records(), 0);
}

#[test]
fn always_append_forces_sync_and_reports_clean_durable_state() {
    let backend = SyncFaultBackend::new([]);
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Always,
        WalServiceConfig::default(),
    )
    .expect("open WAL");
    let first = record(1, b"always success".to_vec());
    let frame_len = record_frame_len(&first);
    let segment_one = ObjectLayout::wal_segment(1).expect("segment one");

    let append = service.append(&first).expect("append always record");

    assert_eq!(append.segment_id(), 1);
    assert_eq!(append.start_offset(), WAL_SEGMENT_HEADER_SIZE as u64);
    assert_eq!(append.bytes_written(), frame_len);
    assert_eq!(append.dirty_bytes(), 0);
    assert!(append.forced_durable());
    assert_eq!(service.dirty_bytes(), 0);
    assert_eq!(service.dirty_records(), 0);
    assert_eq!(backend.sync_calls(), vec![segment_one]);
}

#[test]
fn always_append_visible_durability_unconfirmed_after_sync_failure() {
    let backend = SyncFaultBackend::new([BackendErrorKind::Unavailable]);
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Always,
        WalServiceConfig::default(),
    )
    .expect("open WAL");
    let first = record(1, b"visible but not confirmed".to_vec());
    let first_len = record_frame_len(&first);
    let second = record(2, b"retry after sync fault".to_vec());
    let second_len = record_frame_len(&second);
    let segment_one = ObjectLayout::wal_segment(1).expect("segment one");

    let error = service
        .append(&first)
        .expect_err("always sync failure should reject append result");

    assert_sync_error(error, &segment_one, BackendErrorKind::Unavailable);
    assert_eq!(service.active_segment_id(), 1);
    assert_eq!(service.dirty_bytes(), first_len);
    assert_eq!(service.dirty_records(), 1);
    assert_eq!(
        backend
            .object_metadata(&segment_one)
            .expect("segment metadata after failed sync")
            .size_bytes(),
        WAL_SEGMENT_HEADER_SIZE as u64 + first_len
    );
    assert_metadata_matches_records(&service, 1, std::slice::from_ref(&first));
    let read = service.read_all().expect("read visible unconfirmed record");
    assert_eq!(read.records(), std::slice::from_ref(&first));
    assert_eq!(read.truncation(), None);

    let append = service
        .append(&second)
        .expect("append after transient sync fault");

    assert_eq!(append.segment_id(), 1);
    assert_eq!(
        append.start_offset(),
        WAL_SEGMENT_HEADER_SIZE as u64 + first_len
    );
    assert_eq!(append.bytes_written(), second_len);
    assert_eq!(append.dirty_bytes(), 0);
    assert!(append.forced_durable());
    assert_eq!(service.dirty_bytes(), 0);
    assert_eq!(service.dirty_records(), 0);
    assert_metadata_matches_records(&service, 1, &[first.clone(), second.clone()]);
    let read = service
        .read_all()
        .expect("read visible records after retry");
    assert_eq!(read.records(), &[first, second]);
    assert_eq!(read.truncation(), None);
    assert_eq!(backend.sync_calls(), vec![segment_one.clone(), segment_one]);
}

#[test]
fn sync_backend_error_kind_is_preserved_for_required_routing_cases() {
    for kind in [
        BackendErrorKind::NotFound,
        BackendErrorKind::Interrupted,
        BackendErrorKind::Unavailable,
    ] {
        let backend = SyncFaultBackend::new([kind]);
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Always,
            WalServiceConfig::default(),
        )
        .expect("open WAL");
        let segment_one = ObjectLayout::wal_segment(1).expect("segment one");

        let error = service
            .append(&record(1, format!("sync kind {kind:?}").into_bytes()))
            .expect_err("sync failure should preserve backend kind");

        assert_sync_error(error, &segment_one, kind);
    }
}
