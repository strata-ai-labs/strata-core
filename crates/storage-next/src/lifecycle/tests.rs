use super::*;
use std::error::Error;
use std::fmt;
use strata_core_next::CommitVersion;

const FORBIDDEN_DISPLAY_TERMS: [&str; 11] = [
    "Database::open",
    "OpenOptions",
    "ProductRecovery",
    "public maintenance",
    "manual maintenance command",
    "Follower",
    "StrataHub",
    "VersionedValue",
    "EntityRef",
    "JsonValue",
    "IPC",
];

#[test]
fn lifecycle_default_config_is_valid_and_lossless() {
    let config = LifecycleConfig::default();

    config.validate().expect("default config");
    assert!(config.max_maintenance_queue_depth() > 0);
    assert!(config.max_recovery_faults() > 0);
    assert_eq!(
        config.close_timeout_policy(),
        LifecycleCloseTimeoutPolicy::ReturnTypedTimeout
    );
    assert_eq!(
        config.lossy_recovery(),
        LifecycleLossyRecoveryPolicy::Disabled
    );
}

#[test]
fn lifecycle_explicit_config_preserves_fields() {
    let config = LifecycleConfig::new(
        7,
        3,
        LifecycleCloseTimeoutPolicy::WaitForStorageDrain,
        LifecycleLossyRecoveryPolicy::ExplicitlyAllowed,
    )
    .expect("valid config");

    assert_eq!(config.max_maintenance_queue_depth(), 7);
    assert_eq!(config.max_recovery_faults(), 3);
    assert_eq!(
        config.close_timeout_policy(),
        LifecycleCloseTimeoutPolicy::WaitForStorageDrain
    );
    assert_eq!(
        config.lossy_recovery(),
        LifecycleLossyRecoveryPolicy::ExplicitlyAllowed
    );
}

#[test]
fn lifecycle_config_rejects_zero_limits() {
    assert_eq!(
        LifecycleConfig::new(
            0,
            1,
            LifecycleCloseTimeoutPolicy::ReturnTypedTimeout,
            LifecycleLossyRecoveryPolicy::Disabled,
        ),
        Err(LifecycleError::InvalidConfig {
            field: "max_maintenance_queue_depth",
            reason: "must be nonzero",
        })
    );
    assert_eq!(
        LifecycleConfig::new(
            1,
            0,
            LifecycleCloseTimeoutPolicy::ReturnTypedTimeout,
            LifecycleLossyRecoveryPolicy::Disabled,
        ),
        Err(LifecycleError::InvalidConfig {
            field: "max_recovery_faults",
            reason: "must be nonzero",
        })
    );
}

#[test]
fn lifecycle_fact_variants_are_constructible() {
    assert_eq!(LifecycleState::New, LifecycleState::New);
    assert_eq!(LifecycleState::Opening, LifecycleState::Opening);
    assert_eq!(LifecycleState::Recovering, LifecycleState::Recovering);
    assert_eq!(LifecycleState::Open, LifecycleState::Open);
    assert_eq!(LifecycleState::Closing, LifecycleState::Closing);
    assert_eq!(LifecycleState::Closed, LifecycleState::Closed);
    assert_eq!(LifecycleState::Failed, LifecycleState::Failed);

    assert_eq!(StorageMode::Cache, StorageMode::Cache);
    assert_eq!(
        StorageMode::DurableLocalStandard,
        StorageMode::DurableLocalStandard
    );
    assert_eq!(
        StorageMode::DurableLocalAlways,
        StorageMode::DurableLocalAlways
    );
    assert_eq!(
        StorageMode::ObjectDurableCandidate,
        StorageMode::ObjectDurableCandidate
    );

    assert_eq!(LifecycleLowerLayer::Backend, LifecycleLowerLayer::Backend);
    assert_eq!(LifecycleLowerLayer::Layout, LifecycleLowerLayer::Layout);
    assert_eq!(LifecycleLowerLayer::Format, LifecycleLowerLayer::Format);
    assert_eq!(LifecycleLowerLayer::Service, LifecycleLowerLayer::Service);
    assert_eq!(
        LifecycleLowerLayer::TableRuntime,
        LifecycleLowerLayer::TableRuntime
    );
    assert_eq!(
        LifecycleLowerLayer::BranchRuntime,
        LifecycleLowerLayer::BranchRuntime
    );
    assert_eq!(
        LifecycleLowerLayer::CommitRuntime,
        LifecycleLowerLayer::CommitRuntime
    );

    assert_eq!(ClosePhase::QuiesceCommits, ClosePhase::QuiesceCommits);
    assert_eq!(ClosePhase::StopMaintenance, ClosePhase::StopMaintenance);
    assert_eq!(ClosePhase::DrainMaintenance, ClosePhase::DrainMaintenance);
    assert_eq!(ClosePhase::SyncDurableState, ClosePhase::SyncDurableState);
    assert_eq!(ClosePhase::ReleaseGuards, ClosePhase::ReleaseGuards);
    assert_eq!(ClosePhase::Closed, ClosePhase::Closed);
}

#[test]
fn storage_open_plan_validates_storage_mode_and_lossy_policy() {
    let cache = StorageOpenPlan::new(
        StorageMode::Cache,
        LifecycleCodecId::identity(),
        RecoveryStrictness::Strict,
        LifecycleConfig::default(),
    )
    .expect("cache plan");
    assert_eq!(cache.storage_mode(), StorageMode::Cache);
    assert_eq!(cache.codec_id().as_str(), "identity");
    assert_eq!(cache.recovery_policy(), RecoveryStrictness::Strict);
    assert_eq!(
        cache.lifecycle_config().lossy_recovery(),
        LifecycleLossyRecoveryPolicy::Disabled
    );

    let lossy_config = LifecycleConfig::new(
        8,
        8,
        LifecycleCloseTimeoutPolicy::ReturnTypedTimeout,
        LifecycleLossyRecoveryPolicy::ExplicitlyAllowed,
    )
    .expect("lossy config");
    let durable = StorageOpenPlan::new(
        StorageMode::DurableLocalStandard,
        LifecycleCodecId::new("identity-v2").expect("codec"),
        RecoveryStrictness::AllowExplicitLossyFallback,
        lossy_config,
    )
    .expect("durable lossy plan");
    assert_eq!(durable.storage_mode(), StorageMode::DurableLocalStandard);

    let durable_always = StorageOpenPlan::new(
        StorageMode::DurableLocalAlways,
        LifecycleCodecId::identity(),
        RecoveryStrictness::Strict,
        LifecycleConfig::default(),
    )
    .expect("durable always strict plan");
    assert_eq!(
        durable_always.storage_mode(),
        StorageMode::DurableLocalAlways
    );

    let object_candidate = StorageOpenPlan::new(
        StorageMode::ObjectDurableCandidate,
        LifecycleCodecId::identity(),
        RecoveryStrictness::Strict,
        LifecycleConfig::default(),
    )
    .expect("object durable candidate plan");
    assert_eq!(
        object_candidate.storage_mode(),
        StorageMode::ObjectDurableCandidate
    );

    assert_eq!(
        StorageOpenPlan::new(
            StorageMode::Cache,
            LifecycleCodecId::identity(),
            RecoveryStrictness::AllowExplicitLossyFallback,
            lossy_config,
        ),
        Err(LifecycleError::InvalidOpenPlan {
            reason: "cache mode cannot request durable recovery fallback",
        })
    );
    assert_eq!(
        StorageOpenPlan::new(
            StorageMode::DurableLocalAlways,
            LifecycleCodecId::identity(),
            RecoveryStrictness::AllowExplicitLossyFallback,
            LifecycleConfig::default(),
        ),
        Err(LifecycleError::InvalidOpenPlan {
            reason: "lossy recovery must be enabled explicitly",
        })
    );
}

#[test]
fn lifecycle_codec_id_rejects_invalid_values() {
    assert_eq!(
        LifecycleCodecId::new(""),
        Err(LifecycleError::InvalidOpenPlan {
            reason: "codec id must not be empty",
        })
    );
    assert_eq!(
        LifecycleCodecId::new("identity\0bad"),
        Err(LifecycleError::InvalidOpenPlan {
            reason: "codec id must not contain null bytes",
        })
    );
    assert_eq!(
        LifecycleCodecId::new("x".repeat(129)),
        Err(LifecycleError::InvalidOpenPlan {
            reason: "codec id is too long",
        })
    );
}

#[test]
fn storage_open_outcome_reports_storage_facts() {
    let outcome = StorageOpenOutcome::new(
        StorageMode::DurableLocalAlways,
        true,
        Some(CommitVersion::new(42)),
        RecoveryHealth::Healthy,
        true,
    )
    .expect("open outcome");

    assert_eq!(outcome.mode(), StorageMode::DurableLocalAlways);
    assert!(outcome.opened_existing());
    assert_eq!(
        outcome.recovered_visible_version(),
        Some(CommitVersion::new(42))
    );
    assert!(outcome.recovery_health().is_healthy());
    assert!(outcome.maintenance_ready());

    let cache_outcome = StorageOpenOutcome::new(
        StorageMode::Cache,
        false,
        None,
        RecoveryHealth::Healthy,
        false,
    )
    .expect("cache open outcome");
    assert_eq!(cache_outcome.mode(), StorageMode::Cache);
    assert!(!cache_outcome.opened_existing());
    assert_eq!(cache_outcome.recovered_visible_version(), None);
    assert!(!cache_outcome.maintenance_ready());

    assert_eq!(
        StorageOpenOutcome::new(
            StorageMode::Cache,
            false,
            Some(CommitVersion::new(1)),
            RecoveryHealth::Healthy,
            true,
        ),
        Err(LifecycleError::InvalidOpenPlan {
            reason: "cache mode cannot report recovered durable visibility",
        })
    );
}

#[test]
fn recovery_health_requires_degraded_faults() {
    let fault_kinds = [
        RecoveryFaultKind::CorruptManifest,
        RecoveryFaultKind::CorruptSnapshot,
        RecoveryFaultKind::CorruptWal,
        RecoveryFaultKind::MissingManifestObject,
        RecoveryFaultKind::MissingTableObject,
        RecoveryFaultKind::InheritedLayerLoss,
        RecoveryFaultKind::NoManifestFallback,
        RecoveryFaultKind::IoFailure,
        RecoveryFaultKind::QuarantineInventoryMismatch,
        RecoveryFaultKind::TimelineMismatch,
    ];
    assert_eq!(fault_kinds.len(), 10);

    let fault =
        RecoveryFault::new(RecoveryFaultKind::CorruptWal, "crc mismatch").expect("recovery fault");
    let degraded =
        RecoveryHealth::degraded(RecoveryDegradationClass::DataLoss, vec![fault.clone()])
            .expect("degraded health");

    assert_eq!(
        degraded,
        RecoveryHealth::Degraded {
            class: RecoveryDegradationClass::DataLoss,
            faults: vec![fault.clone()],
        }
    );
    assert_eq!(fault.kind(), RecoveryFaultKind::CorruptWal);
    assert_eq!(fault.reason(), "crc mismatch");
    assert_eq!(
        RecoveryHealth::failed(fault.clone()),
        RecoveryHealth::Failed { fault }
    );
    assert_eq!(
        RecoveryHealth::degraded(RecoveryDegradationClass::Telemetry, Vec::new()),
        Err(LifecycleError::RecoveryFailed {
            reason: "degraded recovery health requires at least one fault",
        })
    );
    for class in [
        RecoveryDegradationClass::DataLoss,
        RecoveryDegradationClass::PolicyDowngrade,
        RecoveryDegradationClass::Telemetry,
    ] {
        let health = RecoveryHealth::degraded(
            class,
            vec![RecoveryFault::new(RecoveryFaultKind::IoFailure, "io failed").expect("fault")],
        )
        .expect("degraded health class");
        assert!(matches!(health, RecoveryHealth::Degraded { .. }));
    }
    assert_eq!(
        RecoveryFault::new(RecoveryFaultKind::IoFailure, ""),
        Err(LifecycleError::RecoveryFailed {
            reason: "recovery fault reason must not be empty",
        })
    );
}

#[test]
fn maintenance_retention_quarantine_and_close_facts_are_constructible() {
    let tasks = [
        MaintenanceTaskKind::Flush,
        MaintenanceTaskKind::Checkpoint,
        MaintenanceTaskKind::WalTruncation,
        MaintenanceTaskKind::Compaction,
        MaintenanceTaskKind::Materialization,
        MaintenanceTaskKind::SnapshotPruning,
        MaintenanceTaskKind::Retention,
        MaintenanceTaskKind::Quarantine,
        MaintenanceTaskKind::Purge,
        MaintenanceTaskKind::Repair,
        MaintenanceTaskKind::HealthCollection,
    ];
    for task in tasks {
        assert_eq!(format!("{task:?}"), format!("{task:?}"));
    }

    let maintenance = MaintenanceOutcome::new(
        MaintenanceTaskKind::Checkpoint,
        MaintenanceOutcomeStatus::Deferred,
    );
    assert_eq!(maintenance.task_kind(), MaintenanceTaskKind::Checkpoint);
    assert_eq!(maintenance.status(), MaintenanceOutcomeStatus::Deferred);
    assert_eq!(
        MaintenanceOutcome::new(
            MaintenanceTaskKind::Repair,
            MaintenanceOutcomeStatus::Failed
        )
        .status(),
        MaintenanceOutcomeStatus::Failed
    );
    assert_eq!(
        MaintenanceOutcome::new(
            MaintenanceTaskKind::Flush,
            MaintenanceOutcomeStatus::Completed
        )
        .status(),
        MaintenanceOutcomeStatus::Completed
    );

    assert_eq!(RetentionDecision::Retain, RetentionDecision::Retain);
    assert_eq!(
        RetentionDecision::PruneCandidate,
        RetentionDecision::PruneCandidate
    );
    assert_eq!(
        RetentionDecision::QuarantineCandidate,
        RetentionDecision::QuarantineCandidate
    );
    assert_eq!(
        RetentionDecision::PurgeCandidate,
        RetentionDecision::PurgeCandidate
    );
    assert_eq!(
        RetentionDecision::SkipUntilProof,
        RetentionDecision::SkipUntilProof
    );
    assert_eq!(QuarantineStage::Candidate, QuarantineStage::Candidate);
    assert_eq!(
        QuarantineStage::InventoryPublished,
        QuarantineStage::InventoryPublished
    );
    assert_eq!(QuarantineStage::Quarantined, QuarantineStage::Quarantined);
    assert_eq!(
        QuarantineStage::PurgeEligible,
        QuarantineStage::PurgeEligible
    );

    let close = CloseOutcome::new(ClosePhase::DrainMaintenance, CloseOutcomeStatus::Timeout);
    assert_eq!(close.phase(), ClosePhase::DrainMaintenance);
    assert_eq!(close.status(), CloseOutcomeStatus::Timeout);
    assert_eq!(
        CloseOutcome::new(ClosePhase::Closed, CloseOutcomeStatus::Complete).status(),
        CloseOutcomeStatus::Complete
    );
    assert_eq!(
        CloseOutcome::new(ClosePhase::Closed, CloseOutcomeStatus::Failed).status(),
        CloseOutcomeStatus::Failed
    );
}

#[test]
fn lifecycle_stats_default_to_zero_and_preserve_explicit_counts() {
    let empty = LifecycleStats::default();
    assert_eq!(empty.open_attempts(), 0);
    assert_eq!(empty.recovery_faults(), 0);
    assert_eq!(empty.maintenance_tasks(), 0);
    assert_eq!(empty.retention_blocks(), 0);
    assert_eq!(empty.close_attempts(), 0);

    let stats = LifecycleStats::new(1, 2, 3, 4, 5);
    assert_eq!(stats.open_attempts(), 1);
    assert_eq!(stats.recovery_faults(), 2);
    assert_eq!(stats.maintenance_tasks(), 3);
    assert_eq!(stats.retention_blocks(), 4);
    assert_eq!(stats.close_attempts(), 5);
}

#[test]
fn lifecycle_error_display_and_source_chain_are_typed() {
    for (error, expected) in [
        (
            LifecycleError::InvalidConfig {
                field: "max_recovery_faults",
                reason: "must be nonzero",
            },
            "invalid lifecycle config max_recovery_faults: must be nonzero",
        ),
        (
            LifecycleError::InvalidLifecycleState {
                reason: "not ready",
            },
            "invalid lifecycle state: not ready",
        ),
        (
            LifecycleError::InvalidOpenPlan {
                reason: "mode rejected",
            },
            "invalid storage open plan: mode rejected",
        ),
        (
            LifecycleError::CapabilityMismatch {
                reason: "durable publish unavailable",
            },
            "storage capability mismatch: durable publish unavailable",
        ),
        (
            LifecycleError::RecoveryFailed {
                reason: "manifest missing",
            },
            "recovery failed: manifest missing",
        ),
        (
            LifecycleError::MaintenanceFailed {
                reason: "task rejected",
            },
            "maintenance failed: task rejected",
        ),
        (
            LifecycleError::RetentionBlocked {
                reason: "proof unavailable",
            },
            "retention blocked: proof unavailable",
        ),
        (
            LifecycleError::CloseFailed {
                reason: "durable sync failed",
            },
            "close failed: durable sync failed",
        ),
    ] {
        assert_eq!(error.to_string(), expected);
        assert_bounded_storage_display(&error.to_string());
    }

    let source = DummySource("backend unavailable");
    let error =
        LifecycleError::lower_layer_with(LifecycleLowerLayer::Backend, "read failed", source);
    assert_eq!(
        error.to_string(),
        "lifecycle lower layer backend failed: read failed"
    );
    assert_eq!(
        error.source().expect("source").to_string(),
        "backend unavailable"
    );

    assert_eq!(
        LifecycleError::lower_layer(LifecycleLowerLayer::Service, "publish rejected").to_string(),
        "lifecycle lower layer service failed: publish rejected"
    );
    assert_bounded_storage_display(
        &LifecycleError::lower_layer(LifecycleLowerLayer::Service, "publish rejected").to_string(),
    );
}

#[test]
fn lifecycle_debug_output_stays_storage_shaped() {
    let facts = [
        format!("{:?}", StorageMode::DurableLocalStandard),
        format!(
            "{:?}",
            StorageOpenPlan::new(
                StorageMode::Cache,
                LifecycleCodecId::identity(),
                RecoveryStrictness::Strict,
                LifecycleConfig::default(),
            )
            .expect("open plan")
        ),
        format!(
            "{:?}",
            StorageOpenOutcome::new(
                StorageMode::DurableLocalStandard,
                true,
                Some(CommitVersion::new(7)),
                RecoveryHealth::Healthy,
                true,
            )
            .expect("open outcome")
        ),
        format!("{:?}", RecoveryHealth::Healthy),
        format!("{:?}", MaintenanceTaskKind::Checkpoint),
        format!("{:?}", RetentionDecision::SkipUntilProof),
        format!("{:?}", QuarantineStage::PurgeEligible),
        format!(
            "{:?}",
            CloseOutcome::new(ClosePhase::ReleaseGuards, CloseOutcomeStatus::Complete)
        ),
    ];

    for fact in facts {
        assert_storage_vocabulary(&fact);
    }
}

#[test]
fn lifecycle_result_alias_uses_lifecycle_error() {
    fn returns_lifecycle_result() -> LifecycleResult<()> {
        Err(LifecycleError::RecoveryFailed {
            reason: "missing checkpoint",
        })
    }

    assert_eq!(
        returns_lifecycle_result(),
        Err(LifecycleError::RecoveryFailed {
            reason: "missing checkpoint",
        })
    );
}

#[derive(Debug)]
struct DummySource(&'static str);

impl fmt::Display for DummySource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for DummySource {}

fn assert_bounded_storage_display(text: &str) {
    assert!(text.len() <= 256, "display/debug text is too long: {text}");
    assert_storage_vocabulary(text);
}

fn assert_storage_vocabulary(text: &str) {
    for forbidden in FORBIDDEN_DISPLAY_TERMS {
        assert!(
            !text.contains(forbidden),
            "display/debug text contains forbidden term {forbidden:?}: {text}"
        );
    }
}
