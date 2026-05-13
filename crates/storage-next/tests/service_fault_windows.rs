//! Durable service fault-window harness entry point.

#![cfg(feature = "fault-injection")]
#![deny(unsafe_code)]

mod common;

use strata_storage_next::testkit::{BackendOperation, FaultScript, FaultingBackend};

#[test]
fn service_fault_window_harness_can_record_backend_operations() {
    let backend = FaultingBackend::new((), FaultScript::empty());

    assert_eq!(
        backend.before_operation(BackendOperation::WriteObject),
        Ok(())
    );
    assert_eq!(backend.calls().len(), 1);
    assert!(common::crate_root().join("src/service/mod.rs").is_file());
}
