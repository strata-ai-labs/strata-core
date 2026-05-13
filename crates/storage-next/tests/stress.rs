//! Long-running storage stress harness entry point.

#![deny(unsafe_code)]

mod common;

#[test]
#[ignore = "stress testing requires durable services that are not implemented yet"]
fn stress_harness_reads_local_options() {
    let _ = common::stress_duration()
        .unwrap_or_else(|error| panic!("invalid stress harness environment: {error}"));
    let _ = common::stress_seed()
        .unwrap_or_else(|error| panic!("invalid stress harness environment: {error}"));

    assert!(common::crate_root().join("src").is_dir());
}
