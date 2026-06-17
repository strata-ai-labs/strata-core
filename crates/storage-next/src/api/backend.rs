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

    /// Arm a targeted publish fault on the branch's table manifest object so the next durable
    /// manifest publish fails at the manifest fsync. `before_visibility = true` faults before the
    /// manifest becomes visible (the temp-file sync); `false` faults after it is visible but before
    /// its durability is confirmed (the parent-directory sync). A memory backend is a no-op (it has
    /// no fault hook). Used by the off-lock publish durability suite to drive manifest-debt recovery.
    #[cfg(all(test, unix, feature = "localfs"))]
    pub(crate) fn inject_manifest_publish_fault(
        &self,
        branch_id: strata_core_next::BranchId,
        before_visibility: bool,
    ) {
        let StorageBackendInner::LocalFs(backend) = &self.inner else {
            return;
        };
        let name = ObjectLayout::branch_table_manifest(&branch_id.to_string())
            .expect("manifest object name")
            .as_str()
            .to_owned();
        if before_visibility {
            backend
                .inject_targeted_manifest_fault_before_visibility(name)
                .expect("arm manifest publish fault");
        } else {
            backend
                .inject_targeted_manifest_fault_visible_unconfirmed(name)
                .expect("arm manifest publish fault");
        }
    }
}
