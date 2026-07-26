//! Opaque storage backend handles.

#[cfg(feature = "localfs")]
use crate::backend::BackendHandle;
use crate::backend::{memory::MemoryBackend, Backend};

#[cfg(feature = "localfs")]
use crate::backend::local_fs::LocalFsBackend;
#[cfg(all(test, unix, feature = "localfs"))]
use crate::layout::ObjectLayout;
#[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
use crate::testkit::{
    BackendCall, FaultScript, FaultingBackend, FsModel, ReorderingBackend, TestkitError,
    WriteOrderingReport, WriteOrderingWatchdog,
};
#[cfg(feature = "localfs")]
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct StorageBackend {
    inner: StorageBackendInner,
}

#[derive(Debug)]
enum StorageBackendInner {
    Memory(MemoryBackend),
    #[cfg(feature = "localfs")]
    LocalFs(LocalFsBackend),
    #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
    Fault(FaultingBackend<LocalFsBackend>),
    #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
    Reordering(ReorderingBackend),
    #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
    WriteOrdering(WriteOrderingWatchdog<LocalFsBackend>),
    /// TCP4.11a: the ordering watchdog stacked over the fault injector, so the
    /// continuous ordering oracle observes every faulted trajectory.
    #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
    WriteOrderingFault(WriteOrderingWatchdog<FaultingBackend<LocalFsBackend>>),
    /// TCP4.11a: the ordering watchdog stacked over the reordering (crash
    /// materialization) backend.
    #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
    WriteOrderingReordering(WriteOrderingWatchdog<ReorderingBackend>),
}

impl StorageBackend {
    #[must_use]
    pub fn memory() -> Self {
        Self {
            inner: StorageBackendInner::Memory(MemoryBackend::new()),
        }
    }

    #[cfg(feature = "localfs")]
    #[must_use]
    pub fn local_fs(root: impl Into<PathBuf>) -> Self {
        Self {
            inner: StorageBackendInner::LocalFs(LocalFsBackend::new(root)),
        }
    }

    #[cfg(feature = "localfs")]
    #[must_use]
    pub fn local_fs_root(&self) -> Option<&Path> {
        match &self.inner {
            StorageBackendInner::Memory(_) => None,
            StorageBackendInner::LocalFs(backend) => Some(backend.root()),
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::Fault(backend) => Some(backend.inner().root()),
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::Reordering(backend) => Some(backend.inner().root()),
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::WriteOrdering(backend) => Some(backend.inner().root()),
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::WriteOrderingFault(backend) => {
                Some(backend.inner().inner().root())
            }
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::WriteOrderingReordering(backend) => {
                Some(backend.inner().inner().root())
            }
        }
    }

    /// Open a local-filesystem backend wrapped in a deterministic fault injector.
    /// The runtime opens on this exactly as it would on [`Self::local_fs`], but the
    /// `script` fails chosen backend operations — the substrate for systematic
    /// fault-injection sweeps. Test / `fault-injection`-only.
    #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
    #[must_use]
    pub fn faulting_local_fs(root: impl Into<PathBuf>, script: FaultScript) -> Self {
        Self {
            inner: StorageBackendInner::Fault(FaultingBackend::new(
                LocalFsBackend::new(root),
                script,
            )),
        }
    }

    /// Open a local-filesystem backend with a cumulative write-byte quota: once
    /// exceeded, every write returns `NoSpace` — a deterministic disk-full
    /// (ENOSPC) simulation. Test / `fault-injection`-only.
    #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
    #[must_use]
    pub fn faulting_local_fs_with_quota(root: impl Into<PathBuf>, byte_quota: u64) -> Self {
        Self {
            inner: StorageBackendInner::Fault(
                FaultingBackend::new(LocalFsBackend::new(root), FaultScript::empty())
                    .with_byte_quota(byte_quota),
            ),
        }
    }

    /// Open a local-filesystem backend that records each object's unsynced
    /// boundary, so [`Self::reordering_crash`] can materialize a filesystem
    /// persistence-model crash state. Test / `fault-injection`-only.
    #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
    #[must_use]
    pub fn reordering_local_fs(root: impl Into<PathBuf>) -> Self {
        Self {
            inner: StorageBackendInner::Reordering(ReorderingBackend::local_fs(root)),
        }
    }

    /// Materialize a crash state per the filesystem persistence `model` (seeded) on
    /// a reordering backend; a no-op (`false`) on other backends. Returns whether
    /// the on-disk state was actually perturbed.
    #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
    pub fn reordering_crash(&self, model: FsModel, seed: u64) -> Result<bool, TestkitError> {
        match &self.inner {
            StorageBackendInner::Reordering(backend) => backend.crash(model, seed),
            StorageBackendInner::WriteOrderingReordering(watchdog) => {
                let reordering = watchdog.inner();
                reordering.crash(model, seed)
            }
            _ => Ok(false),
        }
    }

    /// Open a local-filesystem backend under the write-ordering watchdog: a
    /// pure observer that records append/sync/publish order and files a typed
    /// violation whenever a manifest, snapshot, or table publish happens while
    /// a WAL segment holds unsynced bytes. Test / `fault-injection`-only.
    #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
    #[must_use]
    pub fn write_ordering_local_fs(root: impl Into<PathBuf>) -> Self {
        Self {
            inner: StorageBackendInner::WriteOrdering(WriteOrderingWatchdog::new(
                LocalFsBackend::new(root),
            )),
        }
    }

    /// The ordering watchdog stacked over a fault injector (TCP4.11a): the
    /// simulation's continuous ordering oracle observes the same op stream the
    /// fault script perturbs. Test / `fault-injection`-only.
    #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
    #[must_use]
    pub fn write_ordering_faulting_local_fs(root: impl Into<PathBuf>, script: FaultScript) -> Self {
        Self {
            inner: StorageBackendInner::WriteOrderingFault(WriteOrderingWatchdog::new(
                FaultingBackend::new(LocalFsBackend::new(root), script),
            )),
        }
    }

    /// The ordering watchdog stacked over the reordering (crash) backend
    /// (TCP4.11a): ordering observed continuously while unsynced boundaries are
    /// recorded for crash materialization. Test / `fault-injection`-only.
    #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
    #[must_use]
    pub fn write_ordering_reordering_local_fs(root: impl Into<PathBuf>) -> Self {
        Self {
            inner: StorageBackendInner::WriteOrderingReordering(WriteOrderingWatchdog::new(
                ReorderingBackend::local_fs(root),
            )),
        }
    }

    /// The write-ordering watchdog's report (aggregate facts + violations);
    /// `None` for backends without the watchdog.
    #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
    #[must_use]
    pub fn write_ordering_report(&self) -> Option<WriteOrderingReport> {
        match &self.inner {
            StorageBackendInner::WriteOrdering(backend) => Some(backend.report()),
            StorageBackendInner::WriteOrderingFault(backend) => Some(backend.report()),
            StorageBackendInner::WriteOrderingReordering(backend) => Some(backend.report()),
            _ => None,
        }
    }

    /// CONFIRMED-only write-ordering violations (safe mid-stream and on
    /// faulted/crashed runs that never quiesce; see
    /// [`WriteOrderingWatchdog::confirmed_report`]). `None` without a watchdog.
    #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
    #[must_use]
    pub fn write_ordering_confirmed_report(&self) -> Option<WriteOrderingReport> {
        match &self.inner {
            StorageBackendInner::WriteOrdering(backend) => Some(backend.confirmed_report()),
            StorageBackendInner::WriteOrderingFault(backend) => Some(backend.confirmed_report()),
            StorageBackendInner::WriteOrderingReordering(backend) => {
                Some(backend.confirmed_report())
            }
            _ => None,
        }
    }

    /// Backend operations observed by a faulting backend (empty for non-faulting
    /// backends), so a sweep can see which operations fired and stop once a target
    /// operation no longer occurs in the workload.
    #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
    #[must_use]
    pub fn fault_calls(&self) -> Vec<BackendCall> {
        match &self.inner {
            StorageBackendInner::Fault(backend) => backend.calls(),
            StorageBackendInner::WriteOrderingFault(backend) => backend.inner().calls(),
            _ => Vec::new(),
        }
    }

    pub(crate) fn as_backend(&self) -> &dyn Backend {
        match &self.inner {
            StorageBackendInner::Memory(backend) => backend,
            #[cfg(feature = "localfs")]
            StorageBackendInner::LocalFs(backend) => backend,
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::Fault(backend) => backend,
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::Reordering(backend) => backend,
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::WriteOrdering(backend) => backend,
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::WriteOrderingFault(backend) => backend,
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::WriteOrderingReordering(backend) => backend,
        }
    }

    #[cfg(feature = "localfs")]
    pub(crate) fn to_owned_backend_handle(&self) -> Option<BackendHandle<'static>> {
        match &self.inner {
            // Cache-only, never opened durable, so it needs no owned durable handle.
            StorageBackendInner::Memory(_) => None,
            StorageBackendInner::LocalFs(backend) => Some(BackendHandle::owned(backend.clone())),
            // BS4.4h: the fault/reordering backends `Arc`-share their state and are now `Clone`, so
            // a clone yields an owned handle over the same fault state — durable opens are uniformly
            // owned/`'static`, including the soak tests (which retain their own clone to inspect after).
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::Fault(backend) => Some(BackendHandle::owned(backend.clone())),
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::Reordering(backend) => Some(BackendHandle::owned(backend.clone())),
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::WriteOrdering(backend) => {
                Some(BackendHandle::owned(backend.clone()))
            }
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::WriteOrderingFault(backend) => {
                Some(BackendHandle::owned(backend.clone()))
            }
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::WriteOrderingReordering(backend) => {
                Some(BackendHandle::owned(backend.clone()))
            }
        }
    }

    #[cfg(feature = "localfs")]
    pub(crate) fn into_backend_handle(self) -> BackendHandle<'static> {
        match self.inner {
            StorageBackendInner::Memory(backend) => BackendHandle::owned(backend),
            #[cfg(feature = "localfs")]
            StorageBackendInner::LocalFs(backend) => BackendHandle::owned(backend),
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::Fault(backend) => BackendHandle::owned(backend),
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::Reordering(backend) => BackendHandle::owned(backend),
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::WriteOrdering(backend) => BackendHandle::owned(backend),
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::WriteOrderingFault(backend) => BackendHandle::owned(backend),
            #[cfg(all(any(test, feature = "fault-injection"), feature = "localfs"))]
            StorageBackendInner::WriteOrderingReordering(backend) => BackendHandle::owned(backend),
        }
    }

    /// Arm a targeted publish fault on `object` so its next durable publish fails at the object
    /// fsync. `before_visibility = true` faults before the object becomes visible (the temp-file
    /// sync); `false` faults after it is visible but before its durability is confirmed (the
    /// parent-directory sync). A memory backend is a no-op (it has no fault hook).
    #[cfg(all(test, unix, feature = "localfs"))]
    #[cfg_attr(not(feature = "perf-trace"), allow(dead_code))]
    fn inject_targeted_publish_fault(&self, object: String, before_visibility: bool) {
        let StorageBackendInner::LocalFs(backend) = &self.inner else {
            return;
        };
        if before_visibility {
            backend
                .inject_targeted_publish_fault_before_visibility(object)
                .expect("arm publish fault");
        } else {
            backend
                .inject_targeted_publish_fault_visible_unconfirmed(object)
                .expect("arm publish fault");
        }
    }

    /// Arm a publish fault on the branch's table manifest object. Used by the off-lock publish
    /// durability suite to drive manifest-debt recovery.
    #[cfg(all(test, unix, feature = "localfs"))]
    #[cfg_attr(not(feature = "perf-trace"), allow(dead_code))]
    pub(crate) fn inject_manifest_publish_fault(
        &self,
        branch_id: strata_core::BranchId,
        before_visibility: bool,
    ) {
        let object = ObjectLayout::branch_table_manifest(&branch_id.to_string())
            .expect("manifest object name")
            .as_str()
            .to_owned();
        self.inject_targeted_publish_fault(object, before_visibility);
    }

    /// Arm a publish fault on the checkpoint snapshot object for `snapshot_id`. Used by the
    /// delta-checkpoint crash-consistency suite to fail the snapshot fsync during a checkpoint.
    #[cfg(all(test, unix, feature = "localfs"))]
    #[cfg_attr(not(feature = "perf-trace"), allow(dead_code))]
    pub(crate) fn inject_snapshot_publish_fault(&self, snapshot_id: u64, before_visibility: bool) {
        let object = ObjectLayout::snapshot(snapshot_id)
            .expect("snapshot object name")
            .as_str()
            .to_owned();
        self.inject_targeted_publish_fault(object, before_visibility);
    }
}
