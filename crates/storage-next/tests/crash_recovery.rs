//! Process-level crash recovery harness entry point.

#![deny(unsafe_code)]

mod common;

#[test]
#[ignore = "crash recovery requires durable services that are not implemented yet"]
fn crash_recovery_harness_reads_local_options() {
    let _ = common::crash_case_limit()
        .unwrap_or_else(|error| panic!("invalid crash harness environment: {error}"));
    let _ = common::keep_test_dir()
        .unwrap_or_else(|error| panic!("invalid crash harness environment: {error}"));
    let _ = common::test_root_override()
        .unwrap_or_else(|error| panic!("invalid crash harness environment: {error}"));

    assert!(common::crate_root().join("src/lifecycle/mod.rs").is_file());
}
