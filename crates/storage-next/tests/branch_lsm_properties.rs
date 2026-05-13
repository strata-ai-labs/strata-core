//! Branch-local storage property harness entry point.

#![deny(unsafe_code)]

mod common;

#[test]
fn branch_storage_property_harness_has_branch_module() {
    assert!(common::crate_root().join("src/branch/mod.rs").is_file());
}
