use super::*;
use crate::backend::memory::MemoryBackend;
use crate::backend::{
    Backend, BackendAppend, BackendCapabilities, BackendCapability, BackendError, BackendFence,
    BackendMetadata, BackendRange, BackendResult, BackendWriterGuard, PublishError, PublishMode,
    PublishOutcome, PublishResult, BASIC_OBJECT_BACKEND_CAPABILITIES, CACHE_MODE_REQUIREMENTS,
    DURABLE_LOCAL_MODE_REQUIREMENTS, OBJECT_DURABLE_CANDIDATE_BASE_REQUIREMENTS,
};
use crate::config::mode::DurabilityPolicy;
use crate::object::{ObjectName, ObjectPrefix};
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn capability_validation_maps_lifecycle_modes_to_storage_mode_requests() {
    let cases = [
        (
            StorageMode::Cache,
            BackendCapabilities::from_slice(CACHE_MODE_REQUIREMENTS),
            None,
            None,
        ),
        (
            StorageMode::DurableLocalStandard,
            BackendCapabilities::from_slice(DURABLE_LOCAL_MODE_REQUIREMENTS),
            Some(DurabilityPolicy::Standard),
            None,
        ),
        (
            StorageMode::DurableLocalAlways,
            BackendCapabilities::from_slice(DURABLE_LOCAL_MODE_REQUIREMENTS),
            Some(DurabilityPolicy::Always),
            None,
        ),
        (
            StorageMode::ObjectDurableCandidate,
            object_candidate_capabilities(&[BackendCapability::ConditionalPublish]),
            None,
            Some(ObjectDurableFenceMode::ConditionalPublish),
        ),
        (
            StorageMode::ObjectDurableCandidate,
            object_candidate_capabilities(&[
                BackendCapability::ConditionalCreate,
                BackendCapability::ConditionalUpdate,
            ]),
            None,
            Some(ObjectDurableFenceMode::ConditionalCreateUpdate),
        ),
    ];

    for (mode, capabilities, expected_policy, expected_fence) in cases {
        let outcome = validate_storage_mode_capabilities(&plan(mode), capabilities)
            .expect("capabilities should satisfy lifecycle storage mode");

        assert_eq!(outcome.storage_mode(), mode);
        assert_eq!(outcome.capabilities(), capabilities);
        assert_eq!(outcome.durability_policy(), expected_policy);
        assert_eq!(outcome.request().durability_policy(), expected_policy);
        assert_eq!(outcome.object_candidate_fence(), expected_fence);
        assert!(outcome.missing_capabilities().is_empty());
        assert!(!outcome.required_capabilities().is_empty());
    }
}

#[test]
fn cache_capability_validation_accepts_browser_like_backend_without_metadata() {
    validate_storage_mode_capabilities(
        &plan(StorageMode::Cache),
        BackendCapabilities::from_slice(CACHE_MODE_REQUIREMENTS),
    )
    .expect("cache mode must not require object metadata");

    validate_storage_mode_capabilities(
        &plan(StorageMode::Cache),
        BackendCapabilities::from_slice(BASIC_OBJECT_BACKEND_CAPABILITIES),
    )
    .expect("cache mode accepts basic object backend capabilities");

    validate_storage_mode_capabilities(
        &plan(StorageMode::Cache),
        MemoryBackend::new().capabilities(),
    )
    .expect("cache mode accepts in-memory backend capabilities");

    for missing in CACHE_MODE_REQUIREMENTS {
        let available = capabilities_except(CACHE_MODE_REQUIREMENTS, *missing);
        assert_missing(StorageMode::Cache, available, &[*missing]);
    }
}

#[test]
fn durable_local_modes_reject_each_missing_durable_capability() {
    for mode in [
        StorageMode::DurableLocalStandard,
        StorageMode::DurableLocalAlways,
    ] {
        validate_storage_mode_capabilities(
            &plan(mode),
            BackendCapabilities::from_slice(DURABLE_LOCAL_MODE_REQUIREMENTS),
        )
        .expect("full durable local capability set accepted");

        assert_missing(
            mode,
            MemoryBackend::new().capabilities(),
            &[
                BackendCapability::AppendObject,
                BackendCapability::DurablePublish,
                BackendCapability::DurableSync,
                BackendCapability::SingleWriterLock,
            ],
        );

        for missing in DURABLE_LOCAL_MODE_REQUIREMENTS {
            let available = capabilities_except(DURABLE_LOCAL_MODE_REQUIREMENTS, *missing);
            assert_missing(mode, available, &[*missing]);
        }
    }
}

#[test]
fn object_candidate_accepts_either_publish_fence_or_create_update_pair() {
    let publish = object_candidate_capabilities(&[BackendCapability::ConditionalPublish]);
    let create_update = object_candidate_capabilities(&[
        BackendCapability::ConditionalCreate,
        BackendCapability::ConditionalUpdate,
    ]);
    let all_fences = object_candidate_capabilities(&[
        BackendCapability::ConditionalPublish,
        BackendCapability::ConditionalCreate,
        BackendCapability::ConditionalUpdate,
    ]);

    assert_eq!(
        validate_storage_mode_capabilities(&plan(StorageMode::ObjectDurableCandidate), publish)
            .expect("conditional publish accepted")
            .object_candidate_fence(),
        Some(ObjectDurableFenceMode::ConditionalPublish)
    );
    assert_eq!(
        validate_storage_mode_capabilities(
            &plan(StorageMode::ObjectDurableCandidate),
            create_update,
        )
        .expect("conditional create/update pair accepted")
        .object_candidate_fence(),
        Some(ObjectDurableFenceMode::ConditionalCreateUpdate)
    );
    assert_eq!(
        validate_storage_mode_capabilities(&plan(StorageMode::ObjectDurableCandidate), all_fences)
            .expect("all fences accepted")
            .object_candidate_fence(),
        Some(ObjectDurableFenceMode::ConditionalPublish)
    );
}

#[test]
fn object_candidate_reports_base_and_partial_fence_missing_capabilities() {
    assert_missing(
        StorageMode::ObjectDurableCandidate,
        object_candidate_capabilities_except(
            &[BackendCapability::ConditionalPublish],
            BackendCapability::ObjectMetadata,
        ),
        &[BackendCapability::ObjectMetadata],
    );
    assert_missing(
        StorageMode::ObjectDurableCandidate,
        object_candidate_capabilities_except(
            &[BackendCapability::ConditionalPublish],
            BackendCapability::ConsistentList,
        ),
        &[BackendCapability::ConsistentList],
    );
    assert_missing(
        StorageMode::ObjectDurableCandidate,
        object_candidate_capabilities_except(
            &[BackendCapability::ConditionalPublish],
            BackendCapability::MonotonicMetadata,
        ),
        &[BackendCapability::MonotonicMetadata],
    );
    assert_missing(
        StorageMode::ObjectDurableCandidate,
        BackendCapabilities::from_slice(BASIC_OBJECT_BACKEND_CAPABILITIES),
        &[
            BackendCapability::ConsistentList,
            BackendCapability::MonotonicMetadata,
            BackendCapability::ConditionalPublish,
        ],
    );

    assert_missing(
        StorageMode::ObjectDurableCandidate,
        object_candidate_capabilities(&[BackendCapability::ConditionalCreate]),
        &[
            BackendCapability::ConditionalPublish,
            BackendCapability::ConditionalUpdate,
        ],
    );

    assert_missing(
        StorageMode::ObjectDurableCandidate,
        object_candidate_capabilities(&[BackendCapability::ConditionalUpdate]),
        &[
            BackendCapability::ConditionalPublish,
            BackendCapability::ConditionalCreate,
        ],
    );
    assert_missing(
        StorageMode::ObjectDurableCandidate,
        BackendCapabilities::from_slice(DURABLE_LOCAL_MODE_REQUIREMENTS),
        &[
            BackendCapability::ConsistentList,
            BackendCapability::MonotonicMetadata,
            BackendCapability::ConditionalPublish,
        ],
    );
}

#[test]
fn backend_capability_preflight_reads_only_capabilities() {
    for (mode, accepted_capabilities) in [
        (
            StorageMode::Cache,
            BackendCapabilities::from_slice(CACHE_MODE_REQUIREMENTS),
        ),
        (
            StorageMode::DurableLocalStandard,
            BackendCapabilities::from_slice(DURABLE_LOCAL_MODE_REQUIREMENTS),
        ),
        (
            StorageMode::DurableLocalAlways,
            BackendCapabilities::from_slice(DURABLE_LOCAL_MODE_REQUIREMENTS),
        ),
        (
            StorageMode::ObjectDurableCandidate,
            object_candidate_capabilities(&[BackendCapability::ConditionalPublish]),
        ),
    ] {
        assert_preflight_calls_only_capabilities(mode, accepted_capabilities, true);
        assert_preflight_calls_only_capabilities(mode, BackendCapabilities::empty(), false);
    }
}

fn plan(mode: StorageMode) -> StorageOpenPlan {
    StorageOpenPlan::new(
        mode,
        LifecycleCodecId::identity(),
        RecoveryStrictness::Strict,
        LifecycleConfig::default(),
    )
    .expect("valid lifecycle open plan")
}

fn object_candidate_capabilities(extra: &[BackendCapability]) -> BackendCapabilities {
    let mut capabilities = OBJECT_DURABLE_CANDIDATE_BASE_REQUIREMENTS.to_vec();
    capabilities.extend_from_slice(extra);
    BackendCapabilities::from_slice(&capabilities)
}

fn object_candidate_capabilities_except(
    extra: &[BackendCapability],
    excluded: BackendCapability,
) -> BackendCapabilities {
    let mut capabilities = OBJECT_DURABLE_CANDIDATE_BASE_REQUIREMENTS
        .iter()
        .copied()
        .filter(|capability| *capability != excluded)
        .collect::<Vec<_>>();
    capabilities.extend_from_slice(extra);
    BackendCapabilities::from_slice(&capabilities)
}

fn capabilities_except(
    required: &[BackendCapability],
    excluded: BackendCapability,
) -> BackendCapabilities {
    let capabilities = required
        .iter()
        .copied()
        .filter(|capability| *capability != excluded)
        .collect::<Vec<_>>();
    BackendCapabilities::from_slice(&capabilities)
}

fn assert_missing(
    mode: StorageMode,
    capabilities: BackendCapabilities,
    expected: &[BackendCapability],
) {
    match validate_storage_mode_capabilities(&plan(mode), capabilities) {
        Err(LifecycleError::CapabilityMismatch {
            storage_mode,
            missing,
        }) => {
            assert_eq!(storage_mode, mode);
            assert_eq!(missing, expected);
        }
        result => panic!("expected missing capabilities {expected:?}, got {result:?}"),
    }
}

fn assert_preflight_calls_only_capabilities(
    mode: StorageMode,
    capabilities: BackendCapabilities,
    should_accept: bool,
) {
    let backend = CountingBackend::new(capabilities);
    let result = validate_backend_capabilities_for_open(&plan(mode), &backend);
    if should_accept {
        result.expect("preflight should accept matching capabilities");
    } else {
        assert!(matches!(
            result,
            Err(LifecycleError::CapabilityMismatch { .. })
        ));
    }
    assert_eq!(backend.capability_calls(), 1);
    assert_eq!(backend.side_effect_calls(), 0);
}

struct CountingBackend {
    capabilities: BackendCapabilities,
    capability_calls: AtomicUsize,
    side_effect_calls: AtomicUsize,
}

impl CountingBackend {
    const fn new(capabilities: BackendCapabilities) -> Self {
        Self {
            capabilities,
            capability_calls: AtomicUsize::new(0),
            side_effect_calls: AtomicUsize::new(0),
        }
    }

    fn capability_calls(&self) -> usize {
        self.capability_calls.load(Ordering::SeqCst)
    }

    fn side_effect_calls(&self) -> usize {
        self.side_effect_calls.load(Ordering::SeqCst)
    }

    fn record_side_effect(&self) {
        self.side_effect_calls.fetch_add(1, Ordering::SeqCst);
    }
}

impl Backend for CountingBackend {
    fn capabilities(&self) -> BackendCapabilities {
        self.capability_calls.fetch_add(1, Ordering::SeqCst);
        self.capabilities
    }

    fn read_object(&self, _name: &ObjectName) -> BackendResult<Vec<u8>> {
        self.record_side_effect();
        Err(BackendError::unsupported(BackendCapability::ReadObject))
    }

    fn read_range(&self, _name: &ObjectName, _range: BackendRange) -> BackendResult<Vec<u8>> {
        self.record_side_effect();
        Err(BackendError::unsupported(BackendCapability::ReadRange))
    }

    fn write_object(&self, _name: &ObjectName, _bytes: &[u8]) -> BackendResult<BackendMetadata> {
        self.record_side_effect();
        Err(BackendError::unsupported(BackendCapability::WriteObject))
    }

    fn delete_object(&self, _name: &ObjectName) -> BackendResult<()> {
        self.record_side_effect();
        Err(BackendError::unsupported(BackendCapability::DeleteObject))
    }

    fn list_prefix(&self, _prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        self.record_side_effect();
        Err(BackendError::unsupported(BackendCapability::ListPrefix))
    }

    fn object_metadata(&self, _name: &ObjectName) -> BackendResult<BackendMetadata> {
        self.record_side_effect();
        Err(BackendError::unsupported(BackendCapability::ObjectMetadata))
    }

    fn acquire_writer_lock(&self, _name: &ObjectName) -> BackendResult<BackendWriterGuard> {
        self.record_side_effect();
        Err(BackendError::unsupported(
            BackendCapability::SingleWriterLock,
        ))
    }

    fn append_object(&self, _name: &ObjectName, _bytes: &[u8]) -> BackendResult<BackendAppend> {
        self.record_side_effect();
        Err(BackendError::unsupported(BackendCapability::AppendObject))
    }

    fn sync_object(&self, _name: &ObjectName) -> BackendResult<()> {
        self.record_side_effect();
        Err(BackendError::unsupported(BackendCapability::DurableSync))
    }

    fn conditional_create(
        &self,
        _name: &ObjectName,
        _bytes: &[u8],
    ) -> BackendResult<BackendMetadata> {
        self.record_side_effect();
        Err(BackendError::unsupported(
            BackendCapability::ConditionalCreate,
        ))
    }

    fn conditional_update(
        &self,
        _name: &ObjectName,
        _expected: &BackendFence,
        _bytes: &[u8],
    ) -> BackendResult<BackendMetadata> {
        self.record_side_effect();
        Err(BackendError::unsupported(
            BackendCapability::ConditionalUpdate,
        ))
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        _bytes: &[u8],
        _mode: PublishMode,
    ) -> PublishResult<PublishOutcome> {
        self.record_side_effect();
        Err(PublishError::unsupported(
            name,
            BackendError::unsupported(BackendCapability::DurablePublish),
        ))
    }
}
