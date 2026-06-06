use super::{FuzzResult, ServiceFuzzViolation};
use crate::backend::{
    Backend, BackendCapabilities, BackendCapability, BackendError, BackendErrorKind,
    BackendMetadata, BackendRange, BackendResult, PublishDurability, PublishError,
    PublishFailureKind, PublishMode, PublishOutcome,
};
use crate::object::{ObjectName, ObjectPrefix};
use std::collections::BTreeMap;
use std::sync::Mutex;

// The fuzz backend is intentionally simple but not permissive: it records every
// access so service reconciliation can be asserted read-only after each step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BackendAccess {
    Read,
    List,
    Write,
    Publish,
    Delete,
    Metadata,
}

#[derive(Default)]
pub(super) struct QuarantineScriptBackend {
    objects: Mutex<BTreeMap<ObjectName, Vec<u8>>>,
    publish_failures: Mutex<BTreeMap<ObjectName, (PublishFailureKind, bool)>>,
    delete_failures: Mutex<BTreeMap<ObjectName, BackendErrorKind>>,
    access_log: Mutex<Vec<BackendAccess>>,
}

impl QuarantineScriptBackend {
    pub(super) fn write_visible(&self, object: ObjectName, bytes: Vec<u8>) -> FuzzResult<()> {
        self.objects
            .lock()
            .map_err(|_| ServiceFuzzViolation::new("quarantine object lock poisoned"))?
            .insert(object, bytes);
        Ok(())
    }

    pub(super) fn remove_visible(&self, object: &ObjectName) -> FuzzResult<()> {
        self.objects
            .lock()
            .map_err(|_| ServiceFuzzViolation::new("quarantine object lock poisoned"))?
            .remove(object);
        Ok(())
    }

    pub(super) fn visible_bytes(&self, object: &ObjectName) -> FuzzResult<Option<Vec<u8>>> {
        Ok(self
            .objects
            .lock()
            .map_err(|_| ServiceFuzzViolation::new("quarantine object lock poisoned"))?
            .get(object)
            .cloned())
    }

    pub(super) fn fail_publish_once(
        &self,
        object: ObjectName,
        kind: PublishFailureKind,
        visible: bool,
    ) -> FuzzResult<()> {
        // The `visible` flag models ambiguous durable-publish windows where
        // callers receive an error but the object may already be readable.
        self.publish_failures
            .lock()
            .map_err(|_| ServiceFuzzViolation::new("quarantine publish fault lock poisoned"))?
            .insert(object, (kind, visible));
        Ok(())
    }

    pub(super) fn fail_delete_once(
        &self,
        object: ObjectName,
        kind: BackendErrorKind,
    ) -> FuzzResult<()> {
        self.delete_failures
            .lock()
            .map_err(|_| ServiceFuzzViolation::new("quarantine delete fault lock poisoned"))?
            .insert(object, kind);
        Ok(())
    }

    pub(super) fn clear_faults(&self) -> FuzzResult<()> {
        self.publish_failures
            .lock()
            .map_err(|_| ServiceFuzzViolation::new("quarantine publish fault lock poisoned"))?
            .clear();
        self.delete_failures
            .lock()
            .map_err(|_| ServiceFuzzViolation::new("quarantine delete fault lock poisoned"))?
            .clear();
        Ok(())
    }

    pub(super) fn access_len(&self) -> FuzzResult<usize> {
        Ok(self
            .access_log
            .lock()
            .map_err(|_| ServiceFuzzViolation::new("quarantine access log poisoned"))?
            .len())
    }

    pub(super) fn access_since(&self, start: usize) -> FuzzResult<Vec<BackendAccess>> {
        Ok(self
            .access_log
            .lock()
            .map_err(|_| ServiceFuzzViolation::new("quarantine access log poisoned"))?
            .iter()
            .skip(start)
            .copied()
            .collect())
    }

    fn record(&self, access: BackendAccess) -> BackendResult<()> {
        self.access_log
            .lock()
            .map_err(|_| BackendError::new(BackendErrorKind::Unknown, "access log poisoned"))?
            .push(access);
        Ok(())
    }
}

impl Backend for QuarantineScriptBackend {
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
        self.record(BackendAccess::Read)?;
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
        self.record(BackendAccess::Write)?;
        self.objects
            .lock()
            .map_err(|_| BackendError::new(BackendErrorKind::Unknown, "object lock poisoned"))?
            .insert(name.clone(), bytes.to_vec());
        Ok(BackendMetadata::new(bytes.len() as u64, None))
    }

    fn delete_object(&self, name: &ObjectName) -> crate::backend::DeleteResult {
        self.record(BackendAccess::Delete)
            .map_err(|error| crate::backend::DeleteError::failed_before_removal(name, error))?;
        if let Some(kind) = self
            .delete_failures
            .lock()
            .map_err(|_| {
                crate::backend::DeleteError::failed_before_removal(
                    name,
                    BackendError::new(BackendErrorKind::Unknown, "delete lock poisoned"),
                )
            })?
            .remove(name)
        {
            return crate::backend::failed_delete_result(
                name,
                BackendError::new(kind, "delete failed"),
            );
        }
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
        self.record(BackendAccess::List)?;
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
        self.record(BackendAccess::Metadata)?;
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
        self.record(BackendAccess::Publish).map_err(|source| {
            PublishError::new(
                name.clone(),
                PublishFailureKind::FailedBeforeVisibility,
                source,
            )
        })?;
        if let Some((kind, visible)) = self
            .publish_failures
            .lock()
            .map_err(|_| {
                PublishError::new(
                    name.clone(),
                    PublishFailureKind::FailedBeforeVisibility,
                    BackendError::new(BackendErrorKind::Unknown, "publish lock poisoned"),
                )
            })?
            .remove(name)
        {
            if visible {
                // Visibility-before-error is the crash window that higher
                // layers must reconcile instead of treating as clean failure.
                self.objects
                    .lock()
                    .map_err(|_| {
                        PublishError::new(
                            name.clone(),
                            PublishFailureKind::FailedBeforeVisibility,
                            BackendError::new(BackendErrorKind::Unknown, "object lock poisoned"),
                        )
                    })?
                    .insert(name.clone(), bytes.to_vec());
            }
            return Err(PublishError::new(
                name.clone(),
                kind,
                BackendError::new(BackendErrorKind::Interrupted, "publish failed"),
            ));
        }

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
