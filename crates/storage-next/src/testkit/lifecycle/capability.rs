//! Generated lifecycle capability validation checks.

use super::{ensure, script_byte, LifecycleScaffoldOutcome};
use crate::backend::{
    Backend, BackendAppend, BackendCapabilities, BackendCapability, BackendError, BackendFence,
    BackendMetadata, BackendRange, BackendResult, BackendWriterGuard, PublishError, PublishMode,
    PublishOutcome, PublishResult, CACHE_MODE_REQUIREMENTS, DURABLE_LOCAL_MODE_REQUIREMENTS,
    OBJECT_DURABLE_CANDIDATE_BASE_REQUIREMENTS,
};
use crate::lifecycle::{
    validate_backend_capabilities_for_open, validate_storage_mode_capabilities, LifecycleCodecId,
    LifecycleConfig, LifecycleError, ObjectDurableFenceMode, RecoveryStrictness, StorageMode,
    StorageOpenPlan,
};
use crate::object::{ObjectName, ObjectPrefix};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::super::TestkitError;

const ALL_CAPABILITIES: [BackendCapability; 16] = [
    BackendCapability::ReadObject,
    BackendCapability::ReadRange,
    BackendCapability::WriteObject,
    BackendCapability::DeleteObject,
    BackendCapability::ListPrefix,
    BackendCapability::ObjectMetadata,
    BackendCapability::AppendObject,
    BackendCapability::ConditionalCreate,
    BackendCapability::ConditionalUpdate,
    BackendCapability::DurablePublish,
    BackendCapability::ConditionalPublish,
    BackendCapability::DurableSync,
    BackendCapability::SingleWriterLock,
    BackendCapability::Lease,
    BackendCapability::ConsistentList,
    BackendCapability::MonotonicMetadata,
];

pub(super) fn check_lifecycle_capability_contract(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    check_cache_capabilities(outcome)?;
    check_durable_capabilities(outcome)?;
    check_object_candidate_capabilities(outcome)?;
    check_backend_preflight(outcome)?;
    check_input_derived_capabilities(script, outcome)?;
    Ok(())
}

fn check_cache_capabilities(outcome: &mut LifecycleScaffoldOutcome) -> Result<(), TestkitError> {
    let accepted = BackendCapabilities::from_slice(CACHE_MODE_REQUIREMENTS);
    let validated = validate_storage_mode_capabilities(&plan(StorageMode::Cache), accepted)
        .map_err(|error| TestkitError::new(error.to_string()))?;
    ensure(
        validated.required_capabilities() == CACHE_MODE_REQUIREMENTS,
        "cache capability validation reported wrong required set",
    )?;
    record_accept(
        outcome,
        StorageMode::Cache,
        validated.object_candidate_fence(),
    );

    for missing in CACHE_MODE_REQUIREMENTS {
        let available = capabilities_except(CACHE_MODE_REQUIREMENTS, *missing);
        assert_missing(outcome, StorageMode::Cache, available, &[*missing])?;
    }
    Ok(())
}

fn check_durable_capabilities(outcome: &mut LifecycleScaffoldOutcome) -> Result<(), TestkitError> {
    for mode in [
        StorageMode::DurableLocalStandard,
        StorageMode::DurableLocalAlways,
    ] {
        let accepted = BackendCapabilities::from_slice(DURABLE_LOCAL_MODE_REQUIREMENTS);
        let validated = validate_storage_mode_capabilities(&plan(mode), accepted)
            .map_err(|error| TestkitError::new(error.to_string()))?;
        ensure(
            validated.required_capabilities() == DURABLE_LOCAL_MODE_REQUIREMENTS,
            "durable capability validation reported wrong required set",
        )?;
        record_accept(outcome, mode, validated.object_candidate_fence());

        let missing_append = capabilities_except(
            DURABLE_LOCAL_MODE_REQUIREMENTS,
            BackendCapability::AppendObject,
        );
        assert_missing(
            outcome,
            mode,
            missing_append,
            &[BackendCapability::AppendObject],
        )?;
    }
    Ok(())
}

fn check_object_candidate_capabilities(
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let conditional_publish =
        object_candidate_capabilities(&[BackendCapability::ConditionalPublish]);
    let accepted = validate_storage_mode_capabilities(
        &plan(StorageMode::ObjectDurableCandidate),
        conditional_publish,
    )
    .map_err(|error| TestkitError::new(error.to_string()))?;
    ensure(
        accepted.object_candidate_fence() == Some(ObjectDurableFenceMode::ConditionalPublish),
        "object candidate did not select conditional publish fence",
    )?;
    record_accept(
        outcome,
        StorageMode::ObjectDurableCandidate,
        accepted.object_candidate_fence(),
    );

    let create_update = object_candidate_capabilities(&[
        BackendCapability::ConditionalCreate,
        BackendCapability::ConditionalUpdate,
    ]);
    let accepted = validate_storage_mode_capabilities(
        &plan(StorageMode::ObjectDurableCandidate),
        create_update,
    )
    .map_err(|error| TestkitError::new(error.to_string()))?;
    ensure(
        accepted.object_candidate_fence() == Some(ObjectDurableFenceMode::ConditionalCreateUpdate),
        "object candidate did not select create/update fence pair",
    )?;
    record_accept(
        outcome,
        StorageMode::ObjectDurableCandidate,
        accepted.object_candidate_fence(),
    );

    assert_missing(
        outcome,
        StorageMode::ObjectDurableCandidate,
        object_candidate_capabilities_except(
            &[BackendCapability::ConditionalPublish],
            BackendCapability::ObjectMetadata,
        ),
        &[BackendCapability::ObjectMetadata],
    )?;
    assert_missing(
        outcome,
        StorageMode::ObjectDurableCandidate,
        object_candidate_capabilities_except(
            &[BackendCapability::ConditionalPublish],
            BackendCapability::ConsistentList,
        ),
        &[BackendCapability::ConsistentList],
    )?;
    assert_missing(
        outcome,
        StorageMode::ObjectDurableCandidate,
        object_candidate_capabilities_except(
            &[BackendCapability::ConditionalPublish],
            BackendCapability::MonotonicMetadata,
        ),
        &[BackendCapability::MonotonicMetadata],
    )?;
    assert_missing(
        outcome,
        StorageMode::ObjectDurableCandidate,
        BackendCapabilities::from_slice(OBJECT_DURABLE_CANDIDATE_BASE_REQUIREMENTS),
        &[BackendCapability::ConditionalPublish],
    )?;
    assert_missing(
        outcome,
        StorageMode::ObjectDurableCandidate,
        object_candidate_capabilities(&[BackendCapability::ConditionalCreate]),
        &[
            BackendCapability::ConditionalPublish,
            BackendCapability::ConditionalUpdate,
        ],
    )?;
    assert_missing(
        outcome,
        StorageMode::ObjectDurableCandidate,
        BackendCapabilities::from_slice(DURABLE_LOCAL_MODE_REQUIREMENTS),
        &[
            BackendCapability::ConsistentList,
            BackendCapability::MonotonicMetadata,
            BackendCapability::ConditionalPublish,
        ],
    )?;
    Ok(())
}

fn check_backend_preflight(outcome: &mut LifecycleScaffoldOutcome) -> Result<(), TestkitError> {
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
        assert_preflight_calls_only_capabilities(outcome, mode, accepted_capabilities, true)?;
        assert_preflight_calls_only_capabilities(
            outcome,
            mode,
            BackendCapabilities::empty(),
            false,
        )?;
    }
    Ok(())
}

fn check_input_derived_capabilities(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let mode = storage_mode_from_byte(script_byte(script, 13));
    let capabilities = capabilities_from_script(script);
    match validate_storage_mode_capabilities(&plan(mode), capabilities) {
        Ok(validated) => {
            record_accept(outcome, mode, validated.object_candidate_fence());
        }
        Err(LifecycleError::CapabilityMismatch {
            storage_mode,
            missing,
            ..
        }) => {
            ensure(storage_mode == mode, "capability error lost storage mode")?;
            record_reject(outcome, mode, missing.len());
        }
        Err(error) => return Err(TestkitError::new(error.to_string())),
    }
    outcome.input_derived_capability_cases += 1;
    Ok(())
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
    outcome: &mut LifecycleScaffoldOutcome,
    mode: StorageMode,
    capabilities: BackendCapabilities,
    expected: &[BackendCapability],
) -> Result<(), TestkitError> {
    match validate_storage_mode_capabilities(&plan(mode), capabilities) {
        Err(LifecycleError::CapabilityMismatch {
            storage_mode,
            missing,
            ..
        }) => {
            ensure(storage_mode == mode, "capability error lost storage mode")?;
            ensure(
                missing == expected,
                "capability error returned wrong missing set",
            )?;
            record_reject(outcome, mode, missing.len());
            Ok(())
        }
        Ok(_) => Err(TestkitError::new("missing capabilities were accepted")),
        Err(error) => Err(TestkitError::new(error.to_string())),
    }
}

fn assert_preflight_calls_only_capabilities(
    outcome: &mut LifecycleScaffoldOutcome,
    mode: StorageMode,
    capabilities: BackendCapabilities,
    should_accept: bool,
) -> Result<(), TestkitError> {
    let backend = CountingBackend::new(capabilities);
    let result = validate_backend_capabilities_for_open(&plan(mode), &backend);
    if should_accept {
        result.map_err(|error| TestkitError::new(error.to_string()))?;
    } else {
        ensure(result.is_err(), "preflight accepted missing capabilities")?;
    }
    ensure(
        backend.capability_calls() == 1 && backend.side_effect_calls() == 0,
        "capability preflight touched backend side effects",
    )?;
    outcome.capability_preflight_cases += 1;
    Ok(())
}

fn record_accept(
    outcome: &mut LifecycleScaffoldOutcome,
    mode: StorageMode,
    fence: Option<ObjectDurableFenceMode>,
) {
    outcome.accepted_capability_cases += 1;
    record_mode(outcome, mode);
    match fence {
        Some(ObjectDurableFenceMode::ConditionalPublish) => {
            outcome.object_candidate_conditional_publish_cases += 1;
        }
        Some(ObjectDurableFenceMode::ConditionalCreateUpdate) => {
            outcome.object_candidate_create_update_cases += 1;
        }
        None => {}
    }
}

fn record_reject(outcome: &mut LifecycleScaffoldOutcome, mode: StorageMode, missing_count: usize) {
    outcome.rejected_capability_cases += 1;
    outcome.missing_capability_cases += missing_count;
    record_mode(outcome, mode);
}

fn record_mode(outcome: &mut LifecycleScaffoldOutcome, mode: StorageMode) {
    match mode {
        StorageMode::Cache => outcome.cache_capability_cases += 1,
        StorageMode::DurableLocalStandard => outcome.durable_standard_capability_cases += 1,
        StorageMode::DurableLocalAlways => outcome.durable_always_capability_cases += 1,
        StorageMode::ObjectDurableCandidate => outcome.object_candidate_capability_cases += 1,
    }
}

fn storage_mode_from_byte(byte: u8) -> StorageMode {
    match byte % 4 {
        0 => StorageMode::Cache,
        1 => StorageMode::DurableLocalStandard,
        2 => StorageMode::DurableLocalAlways,
        _ => StorageMode::ObjectDurableCandidate,
    }
}

fn capabilities_from_script(script: &[u8]) -> BackendCapabilities {
    let mut capabilities = BackendCapabilities::empty();
    let low = script_byte(script, 14);
    let high = script_byte(script, 15);
    let bits = u16::from(low) | (u16::from(high) << 8);
    for (index, capability) in ALL_CAPABILITIES.iter().enumerate() {
        if bits & (1_u16 << index) != 0 {
            capabilities.insert(*capability);
        }
    }
    capabilities
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
