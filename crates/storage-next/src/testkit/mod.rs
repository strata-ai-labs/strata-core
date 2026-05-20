//! Feature-gated storage conformance testkit.

#![doc(hidden)]

use std::fmt;

mod branch_lsm;
mod commit_runtime;
mod commit_runtime_allocator;
mod commit_runtime_branch_guards;
mod commit_runtime_outcome;
mod format_fuzz;
mod integration_harness;
mod quarantine_fuzz;
mod service_fuzz;
mod table_runtime;

pub use branch_lsm::{
    check_branch_lsm_fault_window_contract, check_branch_lsm_inheritance_contract,
    check_branch_lsm_install_contract, check_branch_lsm_reads_contract,
    check_branch_lsm_reference_model_contract, check_branch_lsm_scaffold_contract,
    BranchLsmScaffoldOutcome,
};
pub use commit_runtime::{check_commit_runtime_scaffold_contract, CommitRuntimeScaffoldOutcome};
pub use format_fuzz::{
    check_table_format_model_script, decode_format_bytes, FormatDecodeOutcome, FormatDecoder,
};
#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
pub use integration_harness::run_localfs_crash_recovery_harness;
#[cfg(any(test, feature = "fault-injection"))]
pub use integration_harness::run_service_fault_window_harness;
pub use integration_harness::{
    run_storage_stress_harness, CrashRecoveryHarnessOutcome, ServiceFaultWindowHarnessOutcome,
    StorageStressHarnessOutcome,
};
pub use quarantine_fuzz::{run_quarantine_service_script, QuarantineServiceFuzzOutcome};
pub use service_fuzz::{
    run_snapshot_service_script, ServiceFuzzViolation, SnapshotServiceFuzzOutcome,
};
pub use table_runtime::{
    check_table_runtime_compaction_contract, check_table_runtime_cursor_contract,
    check_table_runtime_reader_contract, check_table_runtime_scaffold_contract,
    TableRuntimeScaffoldOutcome,
};

/// Test-only backend selector used by external conformance tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestBackendKind {
    Memory,
    #[cfg(feature = "localfs")]
    LocalFilesystem,
}

impl TestBackendKind {
    /// Parses the backend name used by storage conformance test runners.
    pub fn parse(name: &str) -> Result<Self, TestkitError> {
        match name {
            "memory" => Ok(Self::Memory),
            #[cfg(feature = "localfs")]
            "localfs" => Ok(Self::LocalFilesystem),
            #[cfg(not(feature = "localfs"))]
            "localfs" => Err(TestkitError::new(
                "test backend \"localfs\" requires the localfs feature",
            )),
            _ => Err(TestkitError::new(format!(
                "unsupported test backend {name:?}"
            ))),
        }
    }

    /// Returns the stable name used in test logs and environment values.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            #[cfg(feature = "localfs")]
            Self::LocalFilesystem => "localfs",
        }
    }
}

/// Error returned by testkit selection and setup code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestkitError {
    message: String,
}

impl TestkitError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for TestkitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for TestkitError {}

#[cfg(any(test, feature = "fault-injection"))]
mod fault {
    use crate::backend::{
        Backend, BackendCapabilities, BackendError, BackendErrorKind, BackendFence,
        BackendMetadata, BackendRange, BackendResult, PublishError, PublishFailureKind,
        PublishMode, PublishOutcome, PublishResult,
    };
    use crate::object::{ObjectName, ObjectPrefix};
    use std::collections::HashMap;
    use std::fmt;
    use std::num::NonZeroU64;
    use std::sync::Mutex;

    /// Backend operation that can be targeted by deterministic test faults.
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub enum BackendOperation {
        ReadObject,
        ReadRange,
        WriteObject,
        DeleteObject,
        ListPrefix,
        ObjectMetadata,
        AppendObject,
        SyncObject,
        ConditionalCreate,
        ConditionalUpdate,
        PublishObject,
    }

    impl BackendOperation {
        pub const fn name(self) -> &'static str {
            match self {
                Self::ReadObject => "read_object",
                Self::ReadRange => "read_range",
                Self::WriteObject => "write_object",
                Self::DeleteObject => "delete_object",
                Self::ListPrefix => "list_prefix",
                Self::ObjectMetadata => "object_metadata",
                Self::AppendObject => "append_object",
                Self::SyncObject => "sync_object",
                Self::ConditionalCreate => "conditional_create",
                Self::ConditionalUpdate => "conditional_update",
                Self::PublishObject => "publish_object",
            }
        }
    }

    impl fmt::Display for BackendOperation {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(self.name())
        }
    }

    /// Error kind that the testkit can inject into backend operations.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum FaultKind {
        NotFound,
        AlreadyExists,
        PreconditionFailed,
        PermissionDenied,
        InvalidObjectName,
        InvalidRange,
        UnsupportedOperation,
        CapabilityMismatch,
        Unavailable,
        Interrupted,
        MetadataMismatch,
        Corruption,
        Unknown,
    }

    impl FaultKind {
        const fn backend_kind(self) -> BackendErrorKind {
            match self {
                Self::NotFound => BackendErrorKind::NotFound,
                Self::AlreadyExists => BackendErrorKind::AlreadyExists,
                Self::PreconditionFailed => BackendErrorKind::PreconditionFailed,
                Self::PermissionDenied => BackendErrorKind::PermissionDenied,
                Self::InvalidObjectName => BackendErrorKind::InvalidObjectName,
                Self::InvalidRange => BackendErrorKind::InvalidRange,
                Self::UnsupportedOperation => BackendErrorKind::UnsupportedOperation,
                Self::CapabilityMismatch => BackendErrorKind::CapabilityMismatch,
                Self::Unavailable => BackendErrorKind::Unavailable,
                Self::Interrupted => BackendErrorKind::Interrupted,
                Self::MetadataMismatch => BackendErrorKind::MetadataMismatch,
                Self::Corruption => BackendErrorKind::Corruption,
                Self::Unknown => BackendErrorKind::Unknown,
            }
        }
    }

    /// One deterministic backend fault.
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct FaultRule {
        operation: BackendOperation,
        call_number: NonZeroU64,
        kind: FaultKind,
    }

    impl FaultRule {
        pub const fn new(
            operation: BackendOperation,
            call_number: NonZeroU64,
            kind: FaultKind,
        ) -> Self {
            Self {
                operation,
                call_number,
                kind,
            }
        }

        pub const fn operation(&self) -> BackendOperation {
            self.operation
        }

        pub const fn call_number(&self) -> NonZeroU64 {
            self.call_number
        }

        pub const fn kind(&self) -> FaultKind {
            self.kind
        }
    }

    /// Deterministic fault script for a backend wrapper.
    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub struct FaultScript {
        rules: Vec<FaultRule>,
    }

    impl FaultScript {
        pub fn empty() -> Self {
            Self { rules: Vec::new() }
        }

        pub fn new(rules: impl IntoIterator<Item = FaultRule>) -> Self {
            Self {
                rules: rules.into_iter().collect(),
            }
        }

        fn fault_for(&self, operation: BackendOperation, call_number: u64) -> Option<FaultKind> {
            self.rules
                .iter()
                .find(|rule| rule.operation == operation && rule.call_number.get() == call_number)
                .map(FaultRule::kind)
        }
    }

    /// Observed backend operation call.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct BackendCall {
        operation: BackendOperation,
        call_number: u64,
    }

    impl BackendCall {
        pub const fn operation(self) -> BackendOperation {
            self.operation
        }

        pub const fn call_number(self) -> u64 {
            self.call_number
        }
    }

    #[derive(Debug)]
    struct FaultState {
        script: FaultScript,
        calls: Vec<BackendCall>,
        operation_counts: HashMap<BackendOperation, u64>,
    }

    impl FaultState {
        fn new(script: FaultScript) -> Self {
            Self {
                script,
                calls: Vec::new(),
                operation_counts: HashMap::new(),
            }
        }

        fn observe(&mut self, operation: BackendOperation) -> Option<FaultKind> {
            let count = self.operation_counts.entry(operation).or_insert(0);
            *count += 1;
            let call_number = *count;
            self.calls.push(BackendCall {
                operation,
                call_number,
            });
            self.script.fault_for(operation, call_number)
        }
    }

    /// Test-only backend wrapper that injects deterministic operation failures.
    #[derive(Debug)]
    pub struct FaultingBackend<B> {
        inner: B,
        state: Mutex<FaultState>,
    }

    impl<B> FaultingBackend<B> {
        /// Creates a test-only faulting wrapper around an arbitrary backend handle.
        pub fn new(inner: B, script: FaultScript) -> Self {
            Self {
                inner,
                state: Mutex::new(FaultState::new(script)),
            }
        }

        /// Returns the wrapped backend handle.
        pub fn inner(&self) -> &B {
            &self.inner
        }

        /// Returns the backend operations observed by this wrapper.
        pub fn calls(&self) -> Vec<BackendCall> {
            self.state.lock().map_or_else(
                |poisoned| poisoned.into_inner().calls.clone(),
                |state| state.calls.clone(),
            )
        }

        /// Records one operation and reports whether the script injects a fault.
        ///
        /// External conformance tests can call this before invoking the matching
        /// operation on `inner()`. The storage crate also uses it to implement
        /// the internal backend trait when `B` is a storage backend.
        pub fn before_operation(&self, operation: BackendOperation) -> Result<(), FaultKind> {
            let fault = self
                .state
                .lock()
                .map_or(Some(FaultKind::Unknown), |mut state| {
                    state.observe(operation)
                });

            fault.map_or(Ok(()), Err)
        }

        fn observe(&self, operation: BackendOperation) -> BackendResult<()> {
            self.before_operation(operation).map_err(|kind| {
                BackendError::new(
                    kind.backend_kind(),
                    format!("test fault injected for {operation}"),
                )
            })
        }
    }

    impl<B: Backend> Backend for FaultingBackend<B> {
        fn capabilities(&self) -> BackendCapabilities {
            self.inner.capabilities()
        }

        fn read_object(&self, name: &ObjectName) -> BackendResult<Vec<u8>> {
            self.observe(BackendOperation::ReadObject)?;
            self.inner.read_object(name)
        }

        fn read_range(&self, name: &ObjectName, range: BackendRange) -> BackendResult<Vec<u8>> {
            self.observe(BackendOperation::ReadRange)?;
            self.inner.read_range(name, range)
        }

        fn write_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendMetadata> {
            self.observe(BackendOperation::WriteObject)?;
            self.inner.write_object(name, bytes)
        }

        fn delete_object(&self, name: &ObjectName) -> BackendResult<()> {
            self.observe(BackendOperation::DeleteObject)?;
            self.inner.delete_object(name)
        }

        fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
            self.observe(BackendOperation::ListPrefix)?;
            self.inner.list_prefix(prefix)
        }

        fn object_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
            self.observe(BackendOperation::ObjectMetadata)?;
            self.inner.object_metadata(name)
        }

        fn append_object(
            &self,
            name: &ObjectName,
            bytes: &[u8],
        ) -> BackendResult<crate::backend::BackendAppend> {
            self.observe(BackendOperation::AppendObject)?;
            self.inner.append_object(name, bytes)
        }

        fn sync_object(&self, name: &ObjectName) -> BackendResult<()> {
            self.observe(BackendOperation::SyncObject)?;
            self.inner.sync_object(name)
        }

        fn conditional_create(
            &self,
            name: &ObjectName,
            bytes: &[u8],
        ) -> BackendResult<BackendMetadata> {
            self.observe(BackendOperation::ConditionalCreate)?;
            self.inner.conditional_create(name, bytes)
        }

        fn conditional_update(
            &self,
            name: &ObjectName,
            expected: &BackendFence,
            bytes: &[u8],
        ) -> BackendResult<BackendMetadata> {
            self.observe(BackendOperation::ConditionalUpdate)?;
            self.inner.conditional_update(name, expected, bytes)
        }

        fn publish_object(
            &self,
            name: &ObjectName,
            bytes: &[u8],
            mode: PublishMode,
        ) -> PublishResult<PublishOutcome> {
            self.before_operation(BackendOperation::PublishObject)
                .map_err(|kind| {
                    PublishError::new(
                        name.clone(),
                        PublishFailureKind::FailedBeforeVisibility,
                        BackendError::new(
                            kind.backend_kind(),
                            format!(
                                "test fault injected for {}",
                                BackendOperation::PublishObject
                            ),
                        ),
                    )
                })?;
            self.inner.publish_object(name, bytes, mode)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{BackendOperation, FaultKind, FaultRule, FaultScript, FaultingBackend};
        use crate::backend::memory::MemoryBackend;
        use crate::backend::{Backend, BackendErrorKind, PublishFailureKind, PublishMode};
        use crate::test_support::{assert_backend_error_kind, object_name};
        use std::num::NonZeroU64;

        #[test]
        fn faulting_backend_delegates_when_script_is_empty() {
            let backend = FaultingBackend::new(MemoryBackend::new(), FaultScript::empty());
            let name = object_name("fault/delegate");

            backend.write_object(&name, b"abc").expect("write");
            backend
                .publish_object(&name, b"publish", PublishMode::NonDurableReplace)
                .expect("publish");

            assert_eq!(backend.read_object(&name).expect("read"), b"publish");
            assert_eq!(
                backend
                    .calls()
                    .iter()
                    .map(|call| (call.operation(), call.call_number()))
                    .collect::<Vec<_>>(),
                vec![
                    (BackendOperation::WriteObject, 1),
                    (BackendOperation::PublishObject, 1),
                    (BackendOperation::ReadObject, 1),
                ]
            );
        }

        #[test]
        fn faulting_backend_injects_configured_operation_failure() {
            let script = FaultScript::new([FaultRule::new(
                BackendOperation::ReadObject,
                NonZeroU64::new(1).expect("non-zero"),
                FaultKind::Unavailable,
            )]);
            let backend = FaultingBackend::new(MemoryBackend::new(), script);
            let name = object_name("fault/read");

            backend.write_object(&name, b"abc").expect("write");
            assert_backend_error_kind(backend.read_object(&name), BackendErrorKind::Unavailable);
            assert_eq!(backend.read_object(&name).expect("second read"), b"abc");
        }

        #[test]
        fn faulting_backend_can_fail_conditional_operations_before_delegate() {
            let script = FaultScript::new([FaultRule::new(
                BackendOperation::ConditionalCreate,
                NonZeroU64::new(1).expect("non-zero"),
                FaultKind::PermissionDenied,
            )]);
            let backend = FaultingBackend::new(MemoryBackend::new(), script);
            let name = object_name("fault/conditional");

            assert_backend_error_kind(
                backend.conditional_create(&name, b"bytes"),
                BackendErrorKind::PermissionDenied,
            );
        }

        #[test]
        fn faulting_backend_can_fail_publish_before_delegate() {
            let script = FaultScript::new([FaultRule::new(
                BackendOperation::PublishObject,
                NonZeroU64::new(1).expect("non-zero"),
                FaultKind::Interrupted,
            )]);
            let backend = FaultingBackend::new(MemoryBackend::new(), script);
            let name = object_name("fault/publish");

            let error = backend
                .publish_object(&name, b"bytes", PublishMode::NonDurableReplace)
                .expect_err("publish fault");

            assert_eq!(error.kind(), PublishFailureKind::FailedBeforeVisibility);
            assert_eq!(error.source_error().kind(), BackendErrorKind::Interrupted);
            assert_eq!(
                backend
                    .read_object(&name)
                    .expect_err("not delegated")
                    .kind(),
                BackendErrorKind::NotFound
            );
        }

        #[test]
        fn faulting_backend_can_gate_non_backend_handles() {
            #[derive(Debug)]
            struct ExternalHandle {
                value: &'static str,
            }

            let script = FaultScript::new([FaultRule::new(
                BackendOperation::WriteObject,
                NonZeroU64::new(1).expect("non-zero"),
                FaultKind::Interrupted,
            )]);
            let backend = FaultingBackend::new(ExternalHandle { value: "ready" }, script);

            assert_eq!(
                backend.before_operation(BackendOperation::WriteObject),
                Err(FaultKind::Interrupted)
            );
            assert_eq!(backend.inner().value, "ready");
            assert_eq!(
                backend.before_operation(BackendOperation::WriteObject),
                Ok(())
            );
            assert_eq!(
                backend
                    .calls()
                    .iter()
                    .map(|call| (call.operation(), call.call_number()))
                    .collect::<Vec<_>>(),
                vec![
                    (BackendOperation::WriteObject, 1),
                    (BackendOperation::WriteObject, 2),
                ]
            );
        }
    }
}

#[cfg(any(test, feature = "fault-injection"))]
pub use fault::{
    BackendCall, BackendOperation, FaultKind, FaultRule, FaultScript, FaultingBackend,
};

#[cfg(test)]
mod tests {
    use super::{TestBackendKind, TestkitError};

    #[test]
    fn test_backend_kind_parses_memory() {
        let backend = TestBackendKind::parse("memory").expect("memory backend");

        assert_eq!(backend.name(), "memory");
    }

    #[cfg(feature = "localfs")]
    #[test]
    fn test_backend_kind_parses_localfs_when_feature_is_enabled() {
        let backend = TestBackendKind::parse("localfs").expect("localfs backend");

        assert_eq!(backend.name(), "localfs");
    }

    #[cfg(not(feature = "localfs"))]
    #[test]
    fn test_backend_kind_rejects_localfs_without_feature() {
        assert_eq!(
            TestBackendKind::parse("localfs"),
            Err(TestkitError::new(
                "test backend \"localfs\" requires the localfs feature"
            ))
        );
    }

    #[test]
    fn test_backend_kind_rejects_unknown_backend() {
        assert_eq!(
            TestBackendKind::parse("remote"),
            Err(TestkitError::new("unsupported test backend \"remote\""))
        );
    }
}
