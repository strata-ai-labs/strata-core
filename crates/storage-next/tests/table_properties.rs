//! Table property harness entry point.

#![deny(unsafe_code)]

mod common;

#[test]
fn table_property_harness_has_table_module() {
    assert!(common::crate_root().join("src/table/mod.rs").is_file());
}
