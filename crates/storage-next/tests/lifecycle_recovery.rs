//! Lifecycle and recovery harness entry point.

#![deny(unsafe_code)]

mod common;

#[test]
fn lifecycle_recovery_harness_has_lifecycle_module() {
    assert!(common::crate_root().join("src/lifecycle/mod.rs").is_file());
}
