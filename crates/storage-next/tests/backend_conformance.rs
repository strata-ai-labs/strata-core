//! Backend conformance harness entry point.

#![cfg(feature = "testkit")]
#![deny(unsafe_code)]

mod common;

use strata_storage_next::testkit::TestBackendKind;

#[test]
fn backend_conformance_harness_selects_memory_backend() {
    let selected = common::selected_backend()
        .unwrap_or_else(|error| panic!("invalid backend harness environment: {error}"))
        .unwrap_or_else(|| "memory".to_owned());
    let backend = TestBackendKind::parse(&selected).unwrap_or_else(|error| {
        panic!("unsupported {}={selected:?}: {error}", common::BACKEND_ENV)
    });

    assert_eq!(backend.name(), selected);
}

#[cfg(not(feature = "localfs"))]
#[test]
fn localfs_selection_requires_localfs_feature() {
    let error =
        TestBackendKind::parse("localfs").expect_err("localfs should require localfs feature");

    assert_eq!(
        error.to_string(),
        "test backend \"localfs\" requires the localfs feature"
    );
}
