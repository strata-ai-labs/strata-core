//! Durable format golden-vector harness entry point.

#![deny(unsafe_code)]

mod common;

#[test]
fn format_golden_harness_has_storage_format_directory() {
    assert!(common::storage_format_goldens_dir().is_dir());
}
