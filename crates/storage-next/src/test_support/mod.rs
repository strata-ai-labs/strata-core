//! Crate-private test support.

use crate::backend::{Backend, BackendErrorKind, BackendRange, BackendResult};
use crate::object::{ObjectName, ObjectPrefix};

pub(crate) fn object_name(raw: &str) -> ObjectName {
    ObjectName::new(raw).expect("test object name should be valid")
}

pub(crate) fn object_prefix(raw: &str) -> ObjectPrefix {
    ObjectPrefix::new(raw).expect("test object prefix should be valid")
}

pub(crate) fn assert_backend_error_kind<T>(result: BackendResult<T>, expected: BackendErrorKind) {
    match result {
        Ok(_) => panic!("expected {expected:?} error"),
        Err(error) => assert_eq!(error.kind(), expected),
    }
}

pub(crate) fn assert_backend_list(backend: &dyn Backend, raw_prefix: &str, expected: &[&str]) {
    let prefix = object_prefix(raw_prefix);
    let listed = backend.list_prefix(&prefix).expect("list should succeed");
    let listed: Vec<_> = listed.iter().map(ObjectName::as_str).collect();

    assert_eq!(listed, expected);
}

pub(crate) fn range(offset: u64, length: u64) -> BackendRange {
    BackendRange::new(offset, length)
}
