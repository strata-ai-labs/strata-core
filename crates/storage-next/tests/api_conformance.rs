//! Engine-facing API conformance harness entry point.

#![cfg(feature = "testkit")]
#![deny(unsafe_code)]

mod common;

use strata_storage_next::testkit::TestBackendKind;

#[test]
fn api_conformance_harness_can_use_test_backend_selection() {
    let backend = TestBackendKind::parse("memory").expect("memory backend should be supported");

    assert_eq!(backend.name(), "memory");
    assert!(common::crate_root().join("src/api/mod.rs").is_file());
}
