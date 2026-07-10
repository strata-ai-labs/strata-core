//! Commit timeline property harness entry point.

#![deny(unsafe_code)]

mod common;

#[test]
fn timeline_property_harness_has_commit_module() {
    assert!(common::crate_root().join("src/commit/mod.rs").is_file());
}
