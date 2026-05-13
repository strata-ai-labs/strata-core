//! Commit runtime fault harness entry point.

#![cfg(feature = "fault-injection")]
#![deny(unsafe_code)]

mod common;

use std::num::NonZeroU64;
use strata_storage_next::testkit::{BackendOperation, FaultKind, FaultRule, FaultScript};

#[test]
fn commit_runtime_fault_harness_can_describe_fault_scripts() {
    let call = NonZeroU64::new(1).expect("non-zero call number");
    let script = FaultScript::new([FaultRule::new(
        BackendOperation::ReadObject,
        call,
        FaultKind::Unavailable,
    )]);

    assert_ne!(script, FaultScript::empty());
    assert!(common::crate_root().join("src/commit/mod.rs").is_file());
}
