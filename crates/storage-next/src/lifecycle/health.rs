//! Lifecycle health facts.

use super::{LifecycleError, LifecycleResult};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryHealth {
    Healthy,
    Degraded {
        class: RecoveryDegradationClass,
        faults: Vec<RecoveryFault>,
    },
    Failed {
        fault: RecoveryFault,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryDegradationClass {
    DataLoss,
    PolicyDowngrade,
    Telemetry,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecoveryFault {
    kind: RecoveryFaultKind,
    reason: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryFaultKind {
    CorruptManifest,
    CorruptSnapshot,
    CorruptWal,
    MissingManifestObject,
    MissingTableObject,
    InheritedLayerLoss,
    NoManifestFallback,
    IoFailure,
    QuarantineInventoryMismatch,
    TimelineMismatch,
}

impl RecoveryHealth {
    pub(crate) fn degraded(
        class: RecoveryDegradationClass,
        faults: Vec<RecoveryFault>,
    ) -> LifecycleResult<Self> {
        if faults.is_empty() {
            return Err(LifecycleError::RecoveryFailed {
                reason: "degraded recovery health requires at least one fault",
            });
        }
        Ok(Self::Degraded { class, faults })
    }

    pub(crate) const fn failed(fault: RecoveryFault) -> Self {
        Self::Failed { fault }
    }

    pub(crate) const fn is_healthy(&self) -> bool {
        matches!(self, Self::Healthy)
    }
}

impl RecoveryFault {
    pub(crate) fn new(kind: RecoveryFaultKind, reason: &'static str) -> LifecycleResult<Self> {
        if reason.is_empty() {
            return Err(LifecycleError::RecoveryFailed {
                reason: "recovery fault reason must not be empty",
            });
        }
        Ok(Self { kind, reason })
    }

    pub(crate) const fn kind(&self) -> RecoveryFaultKind {
        self.kind
    }

    pub(crate) const fn reason(&self) -> &'static str {
        self.reason
    }
}
