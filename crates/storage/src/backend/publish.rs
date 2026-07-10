use super::{BackendError, BackendErrorKind, BackendMetadata};
use crate::object::ObjectName;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PublishMode {
    Create,
    Replace,
    NonDurableReplace,
}

impl PublishMode {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Replace => "replace",
            Self::NonDurableReplace => "non_durable_replace",
        }
    }
}

impl fmt::Display for PublishMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PublishDurability {
    Durable,
    NonDurable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PublishOutcome {
    object: ObjectName,
    metadata: BackendMetadata,
    durability: PublishDurability,
}

impl PublishOutcome {
    pub(crate) fn new(
        object: ObjectName,
        metadata: BackendMetadata,
        durability: PublishDurability,
    ) -> Self {
        Self {
            object,
            metadata,
            durability,
        }
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }

    pub(crate) const fn metadata(&self) -> &BackendMetadata {
        &self.metadata
    }

    pub(crate) const fn durability(&self) -> PublishDurability {
        self.durability
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PublishFailureKind {
    /// Backend cannot provide the requested publish contract at all.
    Unsupported,
    /// Create-only publish observed an existing object before any replacement
    /// attempt.
    PreconditionFailed,
    /// Failure occurred before the new bytes could become the visible object.
    FailedBeforeVisibility,
    /// Failure occurred during the visibility change, so callers must inspect
    /// durable state before retrying blindly.
    VisibilityUnknown,
    /// New bytes became visible, but the parent durability barrier did not
    /// complete.
    VisibleDurabilityUnconfirmed,
}

impl PublishFailureKind {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::PreconditionFailed => "precondition_failed",
            Self::FailedBeforeVisibility => "failed_before_visibility",
            Self::VisibilityUnknown => "visibility_unknown",
            Self::VisibleDurabilityUnconfirmed => "visible_durability_unconfirmed",
        }
    }
}

impl fmt::Display for PublishFailureKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PublishError {
    object: ObjectName,
    kind: PublishFailureKind,
    source: BackendError,
}

impl PublishError {
    pub(crate) fn new(object: ObjectName, kind: PublishFailureKind, source: BackendError) -> Self {
        Self {
            object,
            kind,
            source,
        }
    }

    pub(crate) fn unsupported(object: &ObjectName, source: BackendError) -> Self {
        Self::new(object.clone(), PublishFailureKind::Unsupported, source)
    }

    pub(crate) fn precondition_failed(object: &ObjectName, message: impl Into<String>) -> Self {
        Self::new(
            object.clone(),
            PublishFailureKind::PreconditionFailed,
            BackendError::new(BackendErrorKind::AlreadyExists, message),
        )
    }

    pub(crate) const fn object(&self) -> &ObjectName {
        &self.object
    }

    pub(crate) const fn kind(&self) -> PublishFailureKind {
        self.kind
    }

    pub(crate) const fn source_error(&self) -> &BackendError {
        &self.source
    }
}

impl fmt::Display for PublishError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "publish {} failed for {}: {}",
            self.kind, self.object, self.source
        )
    }
}

impl std::error::Error for PublishError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

pub(crate) type PublishResult<T> = Result<T, PublishError>;
