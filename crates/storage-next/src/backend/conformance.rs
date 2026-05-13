use super::{
    Backend, BackendErrorKind, BASIC_OBJECT_BACKEND_CAPABILITIES, CACHE_MODE_REQUIREMENTS,
};
use crate::config::mode::{DurabilityPolicy, StorageModeRequest};
use crate::test_support::{
    assert_backend_error_kind, assert_backend_list, object_name as name, range,
};

fn assert_basic_object_conformance(backend: &dyn Backend) {
    assert_basic_capabilities(backend);
    assert_storage_mode_validation(backend);
    assert_missing_object_behavior(backend);
    assert_write_read_metadata_and_ranges(backend);
    assert_prefix_listing_and_delete(backend);
    assert_conditional_operations_are_unsupported(backend);
}

fn assert_basic_capabilities(backend: &dyn Backend) {
    let capabilities = backend.capabilities();

    assert!(capabilities.supports(CACHE_MODE_REQUIREMENTS));
    assert!(capabilities.supports(BASIC_OBJECT_BACKEND_CAPABILITIES));
}

fn assert_storage_mode_validation(backend: &dyn Backend) {
    let capabilities = backend.capabilities();

    StorageModeRequest::cache()
        .validate_backend(capabilities)
        .expect("basic object backend should satisfy cache mode");

    for request in [
        StorageModeRequest::durable_local(DurabilityPolicy::Standard),
        StorageModeRequest::durable_local(DurabilityPolicy::Always),
        StorageModeRequest::object_durable_candidate(),
    ] {
        let error = request
            .validate_backend(capabilities)
            .expect_err("basic object backend should not satisfy durable modes");

        assert_eq!(error.kind(), BackendErrorKind::CapabilityMismatch);
        assert!(
            !request.missing_capabilities(capabilities).is_empty(),
            "capability mismatch should report at least one missing capability"
        );
    }
}

fn assert_missing_object_behavior(backend: &dyn Backend) {
    let missing = name("objects/missing");

    assert_backend_error_kind(backend.read_object(&missing), BackendErrorKind::NotFound);
    assert_backend_error_kind(
        backend.read_range(&missing, range(0, 1)),
        BackendErrorKind::NotFound,
    );
    assert_backend_error_kind(
        backend.object_metadata(&missing),
        BackendErrorKind::NotFound,
    );
    assert_backend_error_kind(backend.delete_object(&missing), BackendErrorKind::NotFound);
}

fn assert_write_read_metadata_and_ranges(backend: &dyn Backend) {
    let item = name("range/item");

    let metadata = backend
        .write_object(&item, b"abcdef")
        .expect("write should succeed");
    assert_eq!(metadata.size_bytes(), 6);
    assert_eq!(metadata.fence(), None);

    assert_eq!(
        backend.read_object(&item).expect("read should succeed"),
        b"abcdef"
    );
    assert_eq!(
        backend
            .object_metadata(&item)
            .expect("metadata should exist")
            .size_bytes(),
        6
    );
    assert_eq!(
        backend
            .read_range(&item, range(2, 3))
            .expect("range should succeed"),
        b"cde"
    );
    assert_eq!(
        backend
            .read_range(&item, range(2, 20))
            .expect("range should truncate at object length"),
        b"cdef"
    );
    assert_eq!(
        backend
            .read_range(&item, range(3, 0))
            .expect("zero-length range should succeed"),
        b""
    );
    assert_eq!(
        backend
            .read_range(&item, range(99, 1))
            .expect("range past end should return empty bytes"),
        b""
    );
    assert_backend_error_kind(
        backend.read_range(&item, range(u64::MAX, 1)),
        BackendErrorKind::InvalidRange,
    );

    let replacement = backend
        .write_object(&item, b"xy")
        .expect("overwrite should succeed");
    assert_eq!(replacement.size_bytes(), 2);
    assert_eq!(backend.read_object(&item).expect("read replacement"), b"xy");
}

fn assert_prefix_listing_and_delete(backend: &dyn Backend) {
    let parent = name("listing/tree");
    let child = name("listing/tree/child");
    let sibling = name("listing/tree/sibling");
    let other = name("listing/other");

    backend
        .write_object(&parent, b"parent")
        .expect("parent write");
    backend.write_object(&child, b"child").expect("child write");
    backend
        .write_object(&sibling, b"sibling")
        .expect("sibling write");
    backend.write_object(&other, b"other").expect("other write");

    assert_backend_list(
        backend,
        "listing/",
        &[
            "listing/other",
            "listing/tree",
            "listing/tree/child",
            "listing/tree/sibling",
        ],
    );
    assert_backend_list(
        backend,
        "listing/tree/",
        &["listing/tree/child", "listing/tree/sibling"],
    );

    backend.delete_object(&child).expect("delete child");
    assert_backend_error_kind(backend.read_object(&child), BackendErrorKind::NotFound);
    assert_backend_list(backend, "listing/tree/", &["listing/tree/sibling"]);
}

fn assert_conditional_operations_are_unsupported(backend: &dyn Backend) {
    let name = name("objects/conditional");
    let fence = super::BackendFence::new([1, 2, 3]);

    assert_backend_error_kind(
        backend.conditional_create(&name, b"bytes"),
        BackendErrorKind::UnsupportedOperation,
    );
    assert_backend_error_kind(
        backend.conditional_update(&name, &fence, b"bytes"),
        BackendErrorKind::UnsupportedOperation,
    );
}

#[cfg(test)]
mod tests {
    use super::assert_basic_object_conformance;
    use crate::backend::memory::MemoryBackend;

    #[test]
    fn memory_backend_satisfies_basic_object_conformance() {
        let backend = MemoryBackend::new();

        assert_basic_object_conformance(&backend);
    }

    #[cfg(feature = "localfs")]
    #[test]
    fn localfs_backend_satisfies_basic_object_conformance() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = crate::backend::local_fs::LocalFsBackend::new(dir.path());

        assert_basic_object_conformance(&backend);
    }
}
