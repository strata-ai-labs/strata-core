//! Object layout property harness entry point.

#![deny(unsafe_code)]

mod common;

#[test]
fn object_layout_property_harness_has_crate_root() {
    assert!(common::crate_root().join("src/object/mod.rs").is_file());
}
