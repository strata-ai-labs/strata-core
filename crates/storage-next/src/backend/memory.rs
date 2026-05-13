//! In-memory backend for cache-mode storage.

use super::{
    Backend, BackendCapabilities, BackendError, BackendErrorKind, BackendMetadata, BackendRange,
    BackendResult, PublishDurability, PublishError, PublishFailureKind, PublishMode,
    PublishOutcome, PublishResult, BASIC_OBJECT_BACKEND_CAPABILITIES,
};
use crate::object::{ObjectName, ObjectPrefix};
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Debug, Default)]
pub(crate) struct MemoryBackend {
    objects: RwLock<HashMap<ObjectName, Vec<u8>>>,
}

impl MemoryBackend {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn read_objects(
        &self,
    ) -> BackendResult<std::sync::RwLockReadGuard<'_, HashMap<ObjectName, Vec<u8>>>> {
        self.objects
            .read()
            .map_err(|_| BackendError::new(BackendErrorKind::Unknown, "memory backend poisoned"))
    }

    fn write_objects(
        &self,
    ) -> BackendResult<std::sync::RwLockWriteGuard<'_, HashMap<ObjectName, Vec<u8>>>> {
        self.objects
            .write()
            .map_err(|_| BackendError::new(BackendErrorKind::Unknown, "memory backend poisoned"))
    }
}

impl Backend for MemoryBackend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::from_slice(BASIC_OBJECT_BACKEND_CAPABILITIES)
    }

    fn read_object(&self, name: &ObjectName) -> BackendResult<Vec<u8>> {
        let objects = self.read_objects()?;
        objects.get(name).cloned().ok_or_else(|| {
            BackendError::new(
                BackendErrorKind::NotFound,
                format!("object {name} was not found"),
            )
        })
    }

    fn read_range(&self, name: &ObjectName, range: BackendRange) -> BackendResult<Vec<u8>> {
        let bytes = self.read_object(name)?;
        let Some(end_offset) = range.end_offset() else {
            return Err(BackendError::new(
                BackendErrorKind::InvalidRange,
                format!("range {}.. overflows for object {name}", range.offset()),
            ));
        };

        let start = usize::try_from(range.offset()).unwrap_or(usize::MAX);
        let end = usize::try_from(end_offset).unwrap_or(usize::MAX);
        if start >= bytes.len() {
            return Ok(Vec::new());
        }

        Ok(bytes[start..end.min(bytes.len())].to_vec())
    }

    fn write_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendMetadata> {
        let mut objects = self.write_objects()?;
        objects.insert(name.clone(), bytes.to_vec());
        Ok(BackendMetadata::new(bytes.len() as u64, None))
    }

    fn delete_object(&self, name: &ObjectName) -> BackendResult<()> {
        let mut objects = self.write_objects()?;
        objects.remove(name).map_or_else(
            || {
                Err(BackendError::new(
                    BackendErrorKind::NotFound,
                    format!("object {name} was not found"),
                ))
            },
            |_| Ok(()),
        )
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        let objects = self.read_objects()?;
        let mut names: Vec<_> = objects
            .keys()
            .filter(|name| name.as_str().starts_with(prefix.as_str()))
            .cloned()
            .collect();
        names.sort();
        Ok(names)
    }

    fn object_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
        let objects = self.read_objects()?;
        objects.get(name).map_or_else(
            || {
                Err(BackendError::new(
                    BackendErrorKind::NotFound,
                    format!("object {name} was not found"),
                ))
            },
            |bytes| Ok(BackendMetadata::new(bytes.len() as u64, None)),
        )
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        bytes: &[u8],
        mode: PublishMode,
    ) -> PublishResult<PublishOutcome> {
        if mode != PublishMode::NonDurableReplace {
            return Err(PublishError::unsupported(
                name,
                BackendError::unsupported(super::BackendCapability::DurablePublish),
            ));
        }

        let metadata = self.write_object(name, bytes).map_err(|error| {
            PublishError::new(
                name.clone(),
                PublishFailureKind::FailedBeforeVisibility,
                error,
            )
        })?;
        Ok(PublishOutcome::new(
            name.clone(),
            metadata,
            PublishDurability::NonDurable,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::MemoryBackend;
    use crate::backend::{
        Backend, BackendCapability, BackendErrorKind, BackendRange, PublishDurability,
        PublishFailureKind, PublishMode, BASIC_OBJECT_BACKEND_CAPABILITIES,
        CACHE_MODE_REQUIREMENTS,
    };
    use crate::object::{ObjectName, ObjectPrefix};

    #[test]
    fn memory_backend_reports_cache_mode_capabilities() {
        let backend = MemoryBackend::new();
        let capabilities = backend.capabilities();

        assert!(capabilities.supports(CACHE_MODE_REQUIREMENTS));
        assert!(capabilities.supports(BASIC_OBJECT_BACKEND_CAPABILITIES));
        assert!(!capabilities.contains(BackendCapability::DurablePublish));
        assert!(!capabilities.contains(BackendCapability::DurableSync));
        assert!(!capabilities.contains(BackendCapability::SingleWriterLock));
    }

    #[test]
    fn memory_backend_round_trips_object_bytes_and_metadata() {
        let backend = MemoryBackend::new();
        let name = ObjectName::new("tables/main/object").expect("valid object name");

        let metadata = backend
            .write_object(&name, b"abcdef")
            .expect("write should succeed");

        assert_eq!(metadata.size_bytes(), 6);
        assert_eq!(backend.read_object(&name).expect("read object"), b"abcdef");
        assert_eq!(
            backend
                .read_range(&name, BackendRange::new(2, 3))
                .expect("read range"),
            b"cde"
        );
        assert_eq!(
            backend
                .object_metadata(&name)
                .expect("metadata")
                .size_bytes(),
            6
        );
    }

    #[test]
    fn memory_backend_range_reads_are_bounded() {
        let backend = MemoryBackend::new();
        let name = ObjectName::new("tables/main/object").expect("valid object name");
        backend.write_object(&name, b"abc").expect("write");

        assert_eq!(
            backend
                .read_range(&name, BackendRange::new(2, 20))
                .expect("range truncates"),
            b"c"
        );
        assert_eq!(
            backend
                .read_range(&name, BackendRange::new(3, 1))
                .expect("range at end"),
            b""
        );
        assert_eq!(
            backend
                .read_range(&name, BackendRange::new(u64::MAX, 1))
                .expect_err("overflow rejected")
                .kind(),
            BackendErrorKind::InvalidRange
        );
    }

    #[test]
    fn memory_backend_lists_prefixes_in_order() {
        let backend = MemoryBackend::new();
        let names = [
            ObjectName::new("tables/a/002").expect("name"),
            ObjectName::new("tables/a/001").expect("name"),
            ObjectName::new("tables/b/001").expect("name"),
        ];
        for name in &names {
            backend
                .write_object(name, name.as_str().as_bytes())
                .expect("write");
        }

        let prefix = ObjectPrefix::new("tables/a/").expect("prefix");
        let listed = backend.list_prefix(&prefix).expect("list prefix");
        let listed: Vec<_> = listed.iter().map(ObjectName::as_str).collect();

        assert_eq!(listed, vec!["tables/a/001", "tables/a/002"]);
    }

    #[test]
    fn memory_backend_delete_removes_object() {
        let backend = MemoryBackend::new();
        let name = ObjectName::new("manifest/current").expect("name");

        assert_eq!(
            backend.delete_object(&name).expect_err("missing").kind(),
            BackendErrorKind::NotFound
        );

        backend.write_object(&name, b"manifest").expect("write");
        backend.delete_object(&name).expect("delete");

        assert_eq!(
            backend.read_object(&name).expect_err("deleted").kind(),
            BackendErrorKind::NotFound
        );
    }

    #[test]
    fn memory_backend_publishes_non_durable_objects() {
        let backend = MemoryBackend::new();
        let name = ObjectName::new("manifest/current").expect("name");

        let outcome = backend
            .publish_object(&name, b"manifest", PublishMode::NonDurableReplace)
            .expect("publish");

        assert_eq!(outcome.object(), &name);
        assert_eq!(outcome.metadata().size_bytes(), 8);
        assert_eq!(outcome.durability(), PublishDurability::NonDurable);
        assert_eq!(backend.read_object(&name).expect("read"), b"manifest");
    }

    #[test]
    fn memory_backend_rejects_durable_publish_modes() {
        let backend = MemoryBackend::new();
        let name = ObjectName::new("manifest/current").expect("name");

        for mode in [PublishMode::Create, PublishMode::Replace] {
            let error = backend
                .publish_object(&name, b"manifest", mode)
                .expect_err("durable publish should be unsupported");

            assert_eq!(error.kind(), PublishFailureKind::Unsupported);
            assert_eq!(
                error.source_error().kind(),
                BackendErrorKind::UnsupportedOperation
            );
        }
    }
}
