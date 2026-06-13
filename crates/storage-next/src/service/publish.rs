//! Publication service for durable and cache-mode objects.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "publication service is consumed by manifest, WAL, and snapshot services added after the durable publisher"
    )
)]

use crate::backend::{
    Backend, BackendCapability, BackendError, BackendHandle, PublishDurability, PublishError,
    PublishFailureKind, PublishMode, PublishOutcome, PublishResult,
};
use crate::object::ObjectName;

#[derive(Clone)]
pub(crate) struct ObjectPublisher<'a> {
    backend: BackendHandle<'a>,
}

impl<'a> ObjectPublisher<'a> {
    pub(crate) fn new(backend: impl Into<BackendHandle<'a>>) -> Self {
        Self {
            backend: backend.into(),
        }
    }

    pub(crate) fn publish_durable_create(
        &self,
        name: &ObjectName,
        bytes: &[u8],
    ) -> PublishResult<PublishOutcome> {
        self.publish_durable(name, bytes, PublishMode::Create)
    }

    pub(crate) fn publish_durable_replace(
        &self,
        name: &ObjectName,
        bytes: &[u8],
    ) -> PublishResult<PublishOutcome> {
        self.publish_durable(name, bytes, PublishMode::Replace)
    }

    pub(crate) fn publish_non_durable_replace(
        &self,
        name: &ObjectName,
        bytes: &[u8],
    ) -> PublishResult<PublishOutcome> {
        self.backend
            .publish_object(name, bytes, PublishMode::NonDurableReplace)
    }

    fn publish_durable(
        &self,
        name: &ObjectName,
        bytes: &[u8],
        mode: PublishMode,
    ) -> PublishResult<PublishOutcome> {
        self.require_durable_capabilities(name)?;
        self.backend.publish_object(name, bytes, mode)
    }

    fn require_durable_capabilities(&self, name: &ObjectName) -> PublishResult<()> {
        let capabilities = self.backend.capabilities();
        for capability in [
            BackendCapability::DurablePublish,
            BackendCapability::DurableSync,
        ] {
            if !capabilities.contains(capability) {
                return Err(PublishError::new(
                    name.clone(),
                    PublishFailureKind::Unsupported,
                    BackendError::unsupported(capability),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PublishOutcomeMismatch {
    object: ObjectName,
    field: &'static str,
}

impl PublishOutcomeMismatch {
    const fn new(object: ObjectName, field: &'static str) -> Self {
        Self { object, field }
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }

    pub(crate) const fn field(&self) -> &'static str {
        self.field
    }
}

pub(crate) fn validate_publish_outcome(
    object: &ObjectName,
    byte_count: u64,
    outcome: &PublishOutcome,
) -> Result<(), PublishOutcomeMismatch> {
    if outcome.object() != object {
        return Err(PublishOutcomeMismatch::new(object.clone(), "object"));
    }
    if outcome.metadata().size_bytes() != byte_count {
        return Err(PublishOutcomeMismatch::new(object.clone(), "size_bytes"));
    }
    if outcome.durability() != PublishDurability::Durable {
        return Err(PublishOutcomeMismatch::new(object.clone(), "durability"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::backend::{
        memory::MemoryBackend, Backend, BackendErrorKind, PublishDurability, PublishFailureKind,
    };
    #[cfg(all(feature = "localfs", unix))]
    use crate::layout::ObjectLayout;
    use crate::object::ObjectName;
    use crate::service::ObjectPublisher;

    #[test]
    fn non_durable_publish_uses_cache_backend() {
        let backend = MemoryBackend::new();
        let name = ObjectName::new("manifest/current").expect("name");
        let publisher = ObjectPublisher::new(&backend);

        let outcome = publisher
            .publish_non_durable_replace(&name, b"manifest")
            .expect("publish");

        assert_eq!(outcome.object(), &name);
        assert_eq!(outcome.metadata().size_bytes(), 8);
        assert_eq!(outcome.durability(), PublishDurability::NonDurable);
        assert_eq!(backend.read_object(&name).expect("read"), b"manifest");
    }

    #[test]
    fn durable_publish_requires_durable_capabilities() {
        let backend = MemoryBackend::new();
        let name = ObjectName::new("manifest/current").expect("name");
        let publisher = ObjectPublisher::new(&backend);

        let error = publisher
            .publish_durable_replace(&name, b"manifest")
            .expect_err("memory backend cannot publish durably");

        assert_eq!(error.kind(), PublishFailureKind::Unsupported);
        assert_eq!(error.object(), &name);
        assert_eq!(
            error.source_error().kind(),
            BackendErrorKind::UnsupportedOperation
        );
    }

    #[cfg(all(feature = "localfs", unix))]
    #[test]
    fn durable_publish_uses_layout_and_local_filesystem_backend() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = crate::backend::local_fs::LocalFsBackend::new(dir.path());
        let name = ObjectLayout::database_manifest().expect("manifest name");
        let publisher = ObjectPublisher::new(&backend);

        let outcome = publisher
            .publish_durable_replace(&name, b"manifest")
            .expect("publish");

        assert_eq!(outcome.object(), &name);
        assert_eq!(outcome.metadata().size_bytes(), 8);
        assert_eq!(outcome.durability(), PublishDurability::Durable);
        assert_eq!(backend.read_object(&name).expect("read"), b"manifest");
    }

    #[cfg(all(feature = "localfs", unix))]
    #[test]
    fn durable_create_refuses_existing_local_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = crate::backend::local_fs::LocalFsBackend::new(dir.path());
        let name = ObjectLayout::database_manifest().expect("manifest name");
        let publisher = ObjectPublisher::new(&backend);

        publisher
            .publish_durable_create(&name, b"old")
            .expect("initial create");
        let error = publisher
            .publish_durable_create(&name, b"new")
            .expect_err("create should refuse existing object");

        assert_eq!(error.kind(), PublishFailureKind::PreconditionFailed);
        assert_eq!(error.object(), &name);
        assert_eq!(backend.read_object(&name).expect("read"), b"old");
    }
}
