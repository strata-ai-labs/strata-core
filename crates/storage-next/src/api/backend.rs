//! Opaque storage backend handles.

use crate::backend::{memory::MemoryBackend, Backend, BackendHandle};

#[cfg(feature = "localfs")]
use crate::backend::local_fs::LocalFsBackend;
#[cfg(all(test, unix, feature = "localfs"))]
use crate::layout::ObjectLayout;
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
        }
    }

    pub(crate) fn as_backend(&self) -> &dyn Backend {
        match &self.inner {
            StorageBackendInner::Memory(backend) => backend,
            #[cfg(feature = "localfs")]
            StorageBackendInner::LocalFs(backend) => backend,
        }
    }

    pub(crate) fn as_backend_handle(&self) -> BackendHandle<'_> {
        BackendHandle::borrowed(self.as_backend())
    }

    #[cfg(feature = "localfs")]
    pub(crate) fn to_owned_backend_handle(&self) -> Option<BackendHandle<'static>> {
        match &self.inner {
            StorageBackendInner::Memory(_) => None,
            StorageBackendInner::LocalFs(backend) => Some(BackendHandle::owned(backend.clone())),
        }
    }

    #[cfg(feature = "localfs")]
    pub(crate) fn into_backend_handle(self) -> BackendHandle<'static> {
        match self.inner {
            StorageBackendInner::Memory(backend) => BackendHandle::owned(backend),
            #[cfg(feature = "localfs")]
            StorageBackendInner::LocalFs(backend) => BackendHandle::owned(backend),
        }
    }

    /// Arm a targeted publish fault on `object` so its next durable publish fails at the object
    /// fsync. `before_visibility = true` faults before the object becomes visible (the temp-file
    /// sync); `false` faults after it is visible but before its durability is confirmed (the
    /// parent-directory sync). A memory backend is a no-op (it has no fault hook).
    #[cfg(all(test, unix, feature = "localfs"))]
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
    pub(crate) fn inject_manifest_publish_fault(
        &self,
        branch_id: strata_core_next::BranchId,
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
    pub(crate) fn inject_snapshot_publish_fault(&self, snapshot_id: u64, before_visibility: bool) {
        let object = ObjectLayout::snapshot(snapshot_id)
            .expect("snapshot object name")
            .as_str()
            .to_owned();
        self.inject_targeted_publish_fault(object, before_visibility);
    }
}
