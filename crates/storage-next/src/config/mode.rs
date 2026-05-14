#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "storage mode validation is consumed by lifecycle open paths added after durable services"
    )
)]

use crate::backend::{
    BackendCapabilities, BackendCapability, BackendError, BackendErrorKind, BackendResult,
    CACHE_MODE_REQUIREMENTS, DURABLE_LOCAL_MODE_REQUIREMENTS,
    OBJECT_DURABLE_CANDIDATE_BASE_REQUIREMENTS,
};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DurabilityPolicy {
    Standard,
    Always,
}

impl DurabilityPolicy {
    const fn name(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Always => "always",
        }
    }
}

impl fmt::Display for DurabilityPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StorageMode {
    Cache,
    Durable,
    ObjectDurableCandidate,
}

impl StorageMode {
    const fn name(self) -> &'static str {
        match self {
            Self::Cache => "cache",
            Self::Durable => "durable",
            Self::ObjectDurableCandidate => "object-durable-candidate",
        }
    }
}

impl fmt::Display for StorageMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StorageModeRequest {
    mode: StorageMode,
    durability_policy: Option<DurabilityPolicy>,
}

impl StorageModeRequest {
    pub(crate) const fn cache() -> Self {
        Self {
            mode: StorageMode::Cache,
            durability_policy: None,
        }
    }

    pub(crate) const fn durable_local(policy: DurabilityPolicy) -> Self {
        Self {
            mode: StorageMode::Durable,
            durability_policy: Some(policy),
        }
    }

    pub(crate) const fn object_durable_candidate() -> Self {
        Self {
            mode: StorageMode::ObjectDurableCandidate,
            durability_policy: None,
        }
    }

    pub(crate) const fn mode(self) -> StorageMode {
        self.mode
    }

    pub(crate) const fn durability_policy(self) -> Option<DurabilityPolicy> {
        self.durability_policy
    }

    pub(crate) fn validate_backend(self, capabilities: BackendCapabilities) -> BackendResult<()> {
        let missing = self.missing_capabilities(capabilities);
        if missing.is_empty() {
            Ok(())
        } else {
            Err(BackendError::new(
                BackendErrorKind::CapabilityMismatch,
                format!(
                    "backend cannot satisfy {} storage mode; missing {}",
                    self,
                    format_capabilities(&missing)
                ),
            ))
        }
    }

    pub(crate) fn missing_capabilities(
        self,
        capabilities: BackendCapabilities,
    ) -> Vec<BackendCapability> {
        match self.mode {
            StorageMode::Cache => capabilities.missing(CACHE_MODE_REQUIREMENTS),
            StorageMode::Durable => capabilities.missing(DURABLE_LOCAL_MODE_REQUIREMENTS),
            StorageMode::ObjectDurableCandidate => {
                missing_object_candidate_capabilities(capabilities)
            }
        }
    }
}

impl fmt::Display for StorageModeRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.mode, self.durability_policy) {
            (StorageMode::Cache, None) => formatter.write_str("cache"),
            (StorageMode::Durable, Some(policy)) => write!(formatter, "durable/{policy}"),
            (StorageMode::ObjectDurableCandidate, None) => {
                formatter.write_str("object-durable-candidate")
            }
            _ => formatter.write_str(self.mode.name()),
        }
    }
}

fn missing_object_candidate_capabilities(
    capabilities: BackendCapabilities,
) -> Vec<BackendCapability> {
    let mut missing = capabilities.missing(OBJECT_DURABLE_CANDIDATE_BASE_REQUIREMENTS);

    let has_fenced_publish = capabilities.contains(BackendCapability::ConditionalPublish);
    let has_fenced_update = capabilities.contains(BackendCapability::ConditionalCreate)
        && capabilities.contains(BackendCapability::ConditionalUpdate);

    // Object-durable backends may offer either a single conditional publish
    // primitive or a create/update fence pair. Reporting the missing side of a
    // partial pair gives backend authors a targeted capability error.
    if !has_fenced_publish && !has_fenced_update {
        missing.push(BackendCapability::ConditionalPublish);
        if capabilities.contains(BackendCapability::ConditionalCreate) {
            missing.push(BackendCapability::ConditionalUpdate);
        } else if capabilities.contains(BackendCapability::ConditionalUpdate) {
            missing.push(BackendCapability::ConditionalCreate);
        }
    }

    missing
}

fn format_capabilities(capabilities: &[BackendCapability]) -> String {
    capabilities
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::{DurabilityPolicy, StorageMode, StorageModeRequest};
    use crate::backend::{
        BackendCapabilities, BackendCapability, BackendErrorKind, BASIC_OBJECT_BACKEND_CAPABILITIES,
    };

    #[test]
    fn cache_mode_accepts_basic_object_backend() {
        let capabilities = BackendCapabilities::from_slice(BASIC_OBJECT_BACKEND_CAPABILITIES);
        let request = StorageModeRequest::cache();

        request
            .validate_backend(capabilities)
            .expect("basic object backend can satisfy cache mode");
        assert_eq!(request.mode(), StorageMode::Cache);
        assert_eq!(request.durability_policy(), None);
        assert_eq!(request.missing_capabilities(capabilities), Vec::new());
    }

    #[test]
    fn durable_local_modes_require_publish_sync_and_writer_guard() {
        let capabilities = BackendCapabilities::from_slice(BASIC_OBJECT_BACKEND_CAPABILITIES);

        for request in [
            StorageModeRequest::durable_local(DurabilityPolicy::Standard),
            StorageModeRequest::durable_local(DurabilityPolicy::Always),
        ] {
            let error = request
                .validate_backend(capabilities)
                .expect_err("basic object backend cannot satisfy durable local mode");

            assert_eq!(request.mode(), StorageMode::Durable);
            assert!(matches!(
                request.durability_policy(),
                Some(DurabilityPolicy::Standard | DurabilityPolicy::Always)
            ));
            assert_eq!(error.kind(), BackendErrorKind::CapabilityMismatch);
            assert_eq!(
                request.missing_capabilities(capabilities),
                vec![
                    BackendCapability::AppendObject,
                    BackendCapability::DurablePublish,
                    BackendCapability::DurableSync,
                    BackendCapability::SingleWriterLock,
                ]
            );
            assert!(error.to_string().contains(&request.to_string()));
        }
    }

    #[test]
    fn object_candidate_accepts_conditional_publish_as_fence() {
        let capabilities = BackendCapabilities::from_slice(&[
            BackendCapability::ReadObject,
            BackendCapability::ReadRange,
            BackendCapability::WriteObject,
            BackendCapability::DeleteObject,
            BackendCapability::ListPrefix,
            BackendCapability::ObjectMetadata,
            BackendCapability::ConditionalPublish,
            BackendCapability::ConsistentList,
            BackendCapability::MonotonicMetadata,
        ]);

        StorageModeRequest::object_durable_candidate()
            .validate_backend(capabilities)
            .expect("conditional publish can satisfy object durable fencing");
    }

    #[test]
    fn object_candidate_accepts_create_update_pair_as_fence() {
        let capabilities = BackendCapabilities::from_slice(&[
            BackendCapability::ReadObject,
            BackendCapability::ReadRange,
            BackendCapability::WriteObject,
            BackendCapability::DeleteObject,
            BackendCapability::ListPrefix,
            BackendCapability::ObjectMetadata,
            BackendCapability::ConditionalCreate,
            BackendCapability::ConditionalUpdate,
            BackendCapability::ConsistentList,
            BackendCapability::MonotonicMetadata,
        ]);

        StorageModeRequest::object_durable_candidate()
            .validate_backend(capabilities)
            .expect("create/update pair can satisfy object durable fencing");
    }

    #[test]
    fn object_candidate_reports_missing_fence_as_conditional_publish_need() {
        let capabilities = BackendCapabilities::from_slice(BASIC_OBJECT_BACKEND_CAPABILITIES);
        let request = StorageModeRequest::object_durable_candidate();

        let error = request
            .validate_backend(capabilities)
            .expect_err("basic object backend lacks object durable fencing");

        assert_eq!(error.kind(), BackendErrorKind::CapabilityMismatch);
        assert_eq!(
            request.missing_capabilities(capabilities),
            vec![
                BackendCapability::ConsistentList,
                BackendCapability::MonotonicMetadata,
                BackendCapability::ConditionalPublish,
            ]
        );
    }

    #[test]
    fn object_candidate_reports_incomplete_create_update_pair() {
        let capabilities = BackendCapabilities::from_slice(&[
            BackendCapability::ReadObject,
            BackendCapability::ReadRange,
            BackendCapability::WriteObject,
            BackendCapability::DeleteObject,
            BackendCapability::ListPrefix,
            BackendCapability::ObjectMetadata,
            BackendCapability::ConditionalCreate,
            BackendCapability::ConsistentList,
            BackendCapability::MonotonicMetadata,
        ]);

        assert_eq!(
            StorageModeRequest::object_durable_candidate().missing_capabilities(capabilities),
            vec![
                BackendCapability::ConditionalPublish,
                BackendCapability::ConditionalUpdate,
            ]
        );
    }
}
