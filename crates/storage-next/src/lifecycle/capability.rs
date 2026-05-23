//! Storage-mode capability validation.

use super::{LifecycleError, LifecycleResult, StorageMode, StorageOpenPlan};
use crate::backend::{
    Backend, BackendCapabilities, BackendCapability, CACHE_MODE_REQUIREMENTS,
    DURABLE_LOCAL_MODE_REQUIREMENTS, OBJECT_DURABLE_CANDIDATE_BASE_REQUIREMENTS,
};
use crate::config::mode::{DurabilityPolicy, StorageModeRequest};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ObjectDurableFenceMode {
    ConditionalPublish,
    ConditionalCreateUpdate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleCapabilityOutcome {
    storage_mode: StorageMode,
    request: StorageModeRequest,
    capabilities: BackendCapabilities,
    required: Vec<BackendCapability>,
    missing: Vec<BackendCapability>,
    durability_policy: Option<DurabilityPolicy>,
    object_candidate_fence: Option<ObjectDurableFenceMode>,
}

pub(crate) fn validate_storage_mode_capabilities(
    plan: &StorageOpenPlan,
    capabilities: BackendCapabilities,
) -> LifecycleResult<LifecycleCapabilityOutcome> {
    plan.validate()?;
    let request = request_for_storage_mode(plan.storage_mode());
    let required = required_capabilities(plan.storage_mode(), capabilities);
    let missing = request.missing_capabilities(capabilities);
    if !missing.is_empty() {
        return Err(LifecycleError::CapabilityMismatch {
            storage_mode: plan.storage_mode(),
            required,
            missing,
        });
    }

    Ok(LifecycleCapabilityOutcome {
        storage_mode: plan.storage_mode(),
        request,
        capabilities,
        required,
        missing: Vec::new(),
        durability_policy: request.durability_policy(),
        object_candidate_fence: object_candidate_fence(plan.storage_mode(), capabilities),
    })
}

pub(crate) fn validate_backend_capabilities_for_open(
    plan: &StorageOpenPlan,
    backend: &dyn Backend,
) -> LifecycleResult<LifecycleCapabilityOutcome> {
    validate_storage_mode_capabilities(plan, backend.capabilities())
}

pub(crate) const fn request_for_storage_mode(storage_mode: StorageMode) -> StorageModeRequest {
    match storage_mode {
        StorageMode::Cache => StorageModeRequest::cache(),
        StorageMode::DurableLocalStandard => {
            StorageModeRequest::durable_local(DurabilityPolicy::Standard)
        }
        StorageMode::DurableLocalAlways => {
            StorageModeRequest::durable_local(DurabilityPolicy::Always)
        }
        StorageMode::ObjectDurableCandidate => StorageModeRequest::object_durable_candidate(),
    }
}

impl LifecycleCapabilityOutcome {
    pub(crate) const fn storage_mode(&self) -> StorageMode {
        self.storage_mode
    }

    pub(crate) const fn request(&self) -> StorageModeRequest {
        self.request
    }

    pub(crate) const fn capabilities(&self) -> BackendCapabilities {
        self.capabilities
    }

    pub(crate) fn required_capabilities(&self) -> &[BackendCapability] {
        &self.required
    }

    pub(crate) fn missing_capabilities(&self) -> &[BackendCapability] {
        &self.missing
    }

    pub(crate) const fn durability_policy(&self) -> Option<DurabilityPolicy> {
        self.durability_policy
    }

    pub(crate) const fn object_candidate_fence(&self) -> Option<ObjectDurableFenceMode> {
        self.object_candidate_fence
    }
}

fn required_capabilities(
    storage_mode: StorageMode,
    capabilities: BackendCapabilities,
) -> Vec<BackendCapability> {
    match storage_mode {
        StorageMode::Cache => CACHE_MODE_REQUIREMENTS.to_vec(),
        StorageMode::DurableLocalStandard | StorageMode::DurableLocalAlways => {
            DURABLE_LOCAL_MODE_REQUIREMENTS.to_vec()
        }
        StorageMode::ObjectDurableCandidate => {
            let mut required = OBJECT_DURABLE_CANDIDATE_BASE_REQUIREMENTS.to_vec();
            if capabilities.contains(BackendCapability::ConditionalPublish) {
                required.push(BackendCapability::ConditionalPublish);
            }
            if capabilities.contains(BackendCapability::ConditionalCreate)
                && capabilities.contains(BackendCapability::ConditionalUpdate)
            {
                required.push(BackendCapability::ConditionalCreate);
                required.push(BackendCapability::ConditionalUpdate);
            }
            required
        }
    }
}

fn object_candidate_fence(
    storage_mode: StorageMode,
    capabilities: BackendCapabilities,
) -> Option<ObjectDurableFenceMode> {
    if storage_mode != StorageMode::ObjectDurableCandidate {
        return None;
    }
    if capabilities.contains(BackendCapability::ConditionalPublish) {
        Some(ObjectDurableFenceMode::ConditionalPublish)
    } else if capabilities.contains(BackendCapability::ConditionalCreate)
        && capabilities.contains(BackendCapability::ConditionalUpdate)
    {
        Some(ObjectDurableFenceMode::ConditionalCreateUpdate)
    } else {
        None
    }
}
