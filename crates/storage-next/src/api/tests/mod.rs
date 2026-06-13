use std::error::Error;
use std::fmt;
#[cfg(feature = "localfs")]
use std::path::PathBuf;

use super::*;

mod branch;
mod commit;
mod diagnostics;
mod maintenance;
mod read;

fn assert_result_type<T>(result: StorageApiResult<T>) -> StorageApiResult<T> {
    result
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn space() -> StorageSpaceId {
    StorageSpaceId::new(b"space".to_vec()).expect("valid storage space")
}

fn key(name: &[u8]) -> StorageKey {
    StorageKey::new(name.to_vec()).expect("valid key")
}

fn background_space() -> StorageSpaceId {
    StorageSpaceId::new(vec![0x20]).expect("engine storage space")
}

fn background_put_batch(name: &[u8], value: Vec<u8>) -> CommitBatch {
    CommitBatch::new(
        StorageRuntime::default_branch_id_for_test(),
        vec![CommitMutation::Put {
            storage_space: background_space(),
            key: key(name),
            value: StorageValue::new(value),
            ttl: None,
        }],
        CommitOptions::default(),
    )
    .expect("valid background put batch")
}

fn background_raw_row(name: &[u8], version: u64) -> crate::row::StorageRow {
    let physical_key = crate::row::PhysicalKey::new(
        StorageRuntime::default_branch_id_for_test(),
        "api",
        crate::row::StorageSpaceId::engine(0x20).expect("engine-owned row storage space"),
        name,
    )
    .expect("valid raw row key");
    crate::row::StorageRow::put(
        physical_key,
        CommitVersion::new(version),
        Timestamp::from_micros(version),
        Timestamp::EPOCH,
        b"value",
    )
}

fn background_owned_table_count_at(
    layout: &crate::branch::facts::BranchSourceLayout,
    level: u8,
) -> usize {
    if level == 0 {
        return layout.owned_l0_tables();
    }
    layout
        .owned_nonzero_level_table_counts()
        .iter()
        .find(|count| count.level().raw() == level)
        .map_or(0, |count| count.table_count())
}

#[cfg(feature = "localfs")]
fn temp_dir_for_api_test(name: &str) -> PathBuf {
    static NEXT_TEMP_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "strata-storage-api-{name}-{}-{}",
        std::process::id(),
        NEXT_TEMP_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    if path.exists() {
        std::fs::remove_dir_all(&path).expect("clear old temp dir");
    }
    path
}

#[derive(Debug)]
struct SourceError;

impl fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("source failure")
    }
}

impl Error for SourceError {}

#[derive(Debug)]
struct PayloadSourceError;

impl fmt::Display for PayloadSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("source leaked secret-payload [1, 2, 3]")
    }
}

impl Error for PayloadSourceError {}

#[test]
fn api_module_exports_storage_runtime_shell() {
    assert_eq!(
        StorageRuntime::closed().state(),
        StorageRuntimeState::Closed
    );
}

#[test]
fn open_options_do_not_have_implicit_default_mode() {
    let options_source = include_str!("../options.rs");

    assert_storage_open_options_has_no_default(options_source);
}

fn assert_storage_open_options_has_no_default(source: &str) {
    let compact_source = source.split_whitespace().collect::<String>();
    for forbidden_impl in [
        "implDefaultforStorageOpenOptions",
        "implstd::default::DefaultforStorageOpenOptions",
        "implcore::default::DefaultforStorageOpenOptions",
        "impl::std::default::DefaultforStorageOpenOptions",
        "impl::core::default::DefaultforStorageOpenOptions",
    ] {
        assert!(
            !compact_source.contains(forbidden_impl),
            "StorageOpenOptions must not implement Default via {forbidden_impl}"
        );
    }

    assert!(!source.contains("StorageOpenOptions::default"));
    assert_storage_open_options_derive_excludes_default(&compact_source);
}

fn assert_storage_open_options_derive_excludes_default(compact_source: &str) {
    let prefix = compact_source
        .split("pubstructStorageOpenOptions")
        .next()
        .expect("StorageOpenOptions declaration is present");
    let derive_args = prefix
        .rsplit("#[derive(")
        .next()
        .unwrap_or_default()
        .split(")]")
        .next()
        .unwrap_or_default();

    assert!(
        !derive_args
            .split(',')
            .any(|trait_path| trait_path.ends_with("Default")),
        "StorageOpenOptions must not derive Default"
    );
}

#[test]
fn api_module_exports_storage_result() {
    let result = assert_result_type::<()>(Ok(()));
    assert!(result.is_ok());
}

#[test]
fn api_module_exports_storage_error() {
    let error = StorageApiError::InvalidRuntimeState {
        reason: "runtime is closed",
    };
    assert_eq!(error.class(), StorageApiErrorClass::FailedPrecondition);
}

#[test]
fn api_module_exports_open_options_shell() {
    let options = StorageOpenOptions::durable_local(StorageDurabilityPolicy::Always)
        .with_strict_recovery(false);
    assert_eq!(
        options.mode(),
        StorageMode::DurableLocal {
            policy: StorageDurabilityPolicy::Always,
        }
    );
    assert!(!options.strict_recovery());
}

#[test]
fn api_module_exports_read_selector_shell() {
    let request = PointReadRequest::new(
        branch_id(1),
        space(),
        key(b"read-key"),
        ReadBound::AtTimestamp(Timestamp::from_micros(99)),
    );
    assert_eq!(
        request.bound(),
        ReadBound::AtTimestamp(Timestamp::from_micros(99))
    );
    assert_eq!(request.branch_id(), branch_id(1));
    assert_eq!(request.storage_space(), &space());
    assert_eq!(request.key(), &key(b"read-key"));
}

#[test]
fn api_module_exports_commit_batch_shell() {
    let batch = CommitBatch::new(
        branch_id(2),
        vec![CommitMutation::Put {
            storage_space: space(),
            key: key(b"commit-key"),
            value: StorageValue::new(b"value".to_vec()),
            ttl: None,
        }],
        CommitOptions::default().require_conflict_check(true),
    )
    .expect("valid batch");
    assert_eq!(batch.branch_id(), branch_id(2));
    assert!(batch.options().conflict_check_required());
    assert_eq!(batch.mutations().len(), 1);
}

#[test]
fn api_module_exports_branch_request_shell() {
    let request = BranchRequest::new(
        branch_id(3),
        BranchAction::ForkAtVersion {
            source: branch_id(4),
            version: CommitVersion::new(12),
        },
        Some(BranchGeneration::new(2)),
    );
    assert!(matches!(
        request.action(),
        BranchAction::ForkAtVersion { .. }
    ));
    assert_eq!(request.branch_id(), branch_id(3));
    assert_eq!(
        request.expected_generation(),
        Some(BranchGeneration::new(2))
    );
}

#[test]
fn api_module_exports_maintenance_request_shell() {
    let request = MaintenanceRequest::new(
        MaintenanceTask::Repair,
        MaintenanceScope::Branch(branch_id(5)),
    );
    assert_eq!(request.task(), MaintenanceTask::Repair);
    assert_eq!(request.scope(), MaintenanceScope::Branch(branch_id(5)));
}

#[test]
fn api_module_exports_diagnostics_shell() {
    let request = DiagnosticsRequest::new(DiagnosticsScope::Global);
    assert_eq!(request.scope(), DiagnosticsScope::Global);
}

#[test]
fn storage_api_error_codes_are_stable() {
    let branch = branch_id(7);
    let cases = [
        (
            StorageApiError::InvalidArgument {
                field: "key",
                reason: "bad key",
            },
            "invalid_argument.storage_api.argument",
        ),
        (
            StorageApiError::UnsupportedCapability {
                capability: "object_durable",
                reason: "unsupported",
            },
            "unsupported.storage_api.capability",
        ),
        (
            StorageApiError::BranchNotFound { branch_id: branch },
            "not_found.storage_api.branch",
        ),
        (
            StorageApiError::RetainedHistoryUnavailable {
                branch_id: branch,
                reason: "pruned",
            },
            "history_unavailable.storage_api.retained",
        ),
        (
            StorageApiError::TimestampHistoryUnavailable {
                branch_id: branch,
                reason: "pruned",
            },
            "history_unavailable.storage_api.timestamp",
        ),
        (
            StorageApiError::durable_uncertain("sync uncertain"),
            "ambiguous_commit.storage_api.durable_uncertain",
        ),
        (
            StorageApiError::RecoveryDegraded {
                reason: "recovery has storage health debt",
            },
            "failed_precondition.storage_api.recovery_degraded",
        ),
    ];

    for (error, code) in cases {
        assert_eq!(error.code(), code);
    }
}

#[test]
fn storage_api_error_display_is_not_empty() {
    let error = StorageApiError::InvalidRuntimeState {
        reason: "runtime is closed",
    };
    assert!(!error.to_string().is_empty());
}

#[test]
fn storage_api_error_source_chain_is_preserved() {
    let error = StorageApiError::lower_layer_with(
        StorageApiLowerLayer::Service,
        "service failed",
        SourceError,
    );

    assert_eq!(error.code(), "internal.storage_api.lower_layer");
    assert!(error.source().is_some());
}

#[test]
fn storage_api_error_invalid_argument_has_structured_field() {
    let error = StorageApiError::InvalidArgument {
        field: "key",
        reason: "empty",
    };
    match error {
        StorageApiError::InvalidArgument { field, reason } => {
            assert_eq!(field, "key");
            assert_eq!(reason, "empty");
        }
        _ => panic!("expected invalid argument"),
    }
}

#[test]
fn storage_api_error_unsupported_capability_has_structured_field() {
    let error = StorageApiError::UnsupportedCapability {
        capability: "object_durable",
        reason: "unsupported",
    };
    assert_eq!(error.class(), StorageApiErrorClass::Unsupported);
    match error {
        StorageApiError::UnsupportedCapability { capability, reason } => {
            assert_eq!(capability, "object_durable");
            assert_eq!(reason, "unsupported");
        }
        _ => panic!("expected unsupported capability"),
    }
}

#[test]
fn storage_api_error_history_unavailable_is_distinct_from_not_found() {
    let branch = branch_id(8);
    let history = StorageApiError::RetainedHistoryUnavailable {
        branch_id: branch,
        reason: "pruned",
    };
    let not_found = StorageApiError::BranchNotFound { branch_id: branch };

    assert_ne!(history.code(), not_found.code());
    assert_eq!(history.class(), StorageApiErrorClass::HistoryUnavailable);
    assert_eq!(not_found.class(), StorageApiErrorClass::NotFound);
}

#[test]
fn storage_api_error_durable_uncertain_is_distinct_from_lower_layer_failure() {
    let uncertain = StorageApiError::durable_uncertain("sync uncertain");
    let lower = StorageApiError::lower_layer_with(
        StorageApiLowerLayer::Service,
        "service failed",
        SourceError,
    );

    assert_ne!(uncertain.code(), lower.code());
    assert_eq!(uncertain.class(), StorageApiErrorClass::AmbiguousCommit);
    assert_eq!(lower.class(), StorageApiErrorClass::Internal);
}

#[test]
fn storage_api_error_display_does_not_include_payload_bytes() {
    let error = StorageApiError::lower_layer_with(
        StorageApiLowerLayer::Service,
        "service failed",
        PayloadSourceError,
    );
    let display = error.to_string();

    assert!(!display.contains("secret-payload"));
    assert!(!display.contains("[1, 2, 3]"));
}

#[test]
fn storage_api_error_classes_do_not_overclaim_corruption() {
    assert_eq!(
        StorageApiError::RecoveryDegraded {
            reason: "policy downgrade",
        }
        .class(),
        StorageApiErrorClass::FailedPrecondition
    );
    assert_eq!(
        StorageApiError::durable_uncertain("sync uncertain").class(),
        StorageApiErrorClass::AmbiguousCommit
    );
}

#[test]
fn storage_key_rejects_empty_when_required() {
    assert!(StorageKey::new(Vec::<u8>::new()).is_err());
}

#[test]
fn storage_value_accepts_opaque_bytes() {
    let bytes = vec![0, 159, 255, b'\n'];
    let value = StorageValue::new(bytes.clone());
    assert_eq!(value.as_bytes(), bytes.as_slice());
}

#[test]
fn read_limit_rejects_zero_when_zero_is_invalid() {
    assert!(ReadLimit::new(0).is_err());
    assert_eq!(ReadLimit::new(3).expect("valid limit").get(), 3);
}

#[test]
fn scan_bound_order_is_validated() {
    let start = key(b"a");
    let end = key(b"z");
    let valid = ScanRange::new(Some(start.clone()), Some(end)).expect("valid range");
    assert_eq!(valid.start(), Some(&start));

    let invalid = ScanRange::new(Some(key(b"z")), Some(key(b"a"))).expect_err("invalid range");
    assert_eq!(invalid.code(), "invalid_argument.storage_api.argument");
}

#[test]
fn branch_generation_zero_policy_is_explicit() {
    assert_eq!(BranchGeneration::ZERO.as_u64(), 0);
    assert_eq!(BranchGeneration::new(0), BranchGeneration::ZERO);
}

#[test]
fn maintenance_request_kind_is_constructible() {
    let kinds = [
        MaintenanceTask::Checkpoint,
        MaintenanceTask::Flush,
        MaintenanceTask::Compact,
        MaintenanceTask::Materialize,
        MaintenanceTask::Retain,
        MaintenanceTask::SnapshotPruning,
        MaintenanceTask::Reclaim,
        MaintenanceTask::Quarantine,
        MaintenanceTask::Purge,
        MaintenanceTask::Repair,
        MaintenanceTask::WalGrowth,
    ];

    for kind in kinds {
        assert_eq!(
            MaintenanceRequest::new(kind, MaintenanceScope::Global).task(),
            kind
        );
    }
}

#[test]
fn diagnostics_request_kind_is_constructible() {
    let global = DiagnosticsRequest::new(DiagnosticsScope::Global);
    let branch = DiagnosticsRequest::new(DiagnosticsScope::Branch(branch_id(9)));

    assert_eq!(global.scope(), DiagnosticsScope::Global);
    assert_eq!(branch.scope(), DiagnosticsScope::Branch(branch_id(9)));
}

#[test]
fn open_options_reject_unsupported_modes() {
    assert!(StorageOpenOptions::cache().validate().is_ok());
    assert!(StorageOpenOptions::ephemeral().validate().is_ok());
    assert!(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
            .validate()
            .is_ok()
    );

    assert_eq!(
        StorageOpenOptions::object_durable_candidate()
            .validate()
            .expect_err("unsupported")
            .code(),
        "unsupported.storage_api.capability"
    );
    assert_eq!(
        StorageOpenOptions::distributed_candidate()
            .validate()
            .expect_err("unsupported")
            .code(),
        "unsupported.storage_api.capability"
    );
}

#[test]
fn open_options_ephemeral_is_explicit_cache_mode() {
    let options = StorageOpenOptions::ephemeral();

    assert_eq!(options.mode(), StorageMode::Cache);
    assert!(!options.requires_backend());
    assert!(options.validate().is_ok());
}

#[test]
fn open_options_rejects_zero_limits() {
    for policy in [
        StorageWalGrowthPolicy::thresholds(0, 1, 1),
        StorageWalGrowthPolicy::thresholds(1, 0, 1),
        StorageWalGrowthPolicy::thresholds(1, 1, 0),
    ] {
        let error = StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
            .with_wal_growth_policy(policy)
            .validate()
            .expect_err("zero WAL growth limits are rejected");
        assert_eq!(error.code(), "invalid_argument.storage_api.argument");
    }
}

#[test]
fn open_rejects_zero_limits_before_lifecycle_mapping() {
    let error = StorageRuntime::open(
        StorageOpenOptions::cache()
            .with_wal_growth_policy(StorageWalGrowthPolicy::thresholds(0, 1, 1)),
    )
    .expect_err("zero WAL growth byte limit is rejected before open");

    match error {
        StorageApiError::InvalidArgument { field, reason } => {
            assert_eq!(field, "max_retained_wal_bytes");
            assert_eq!(reason, "WAL growth byte limit must be greater than zero");
        }
        _ => panic!("expected invalid argument"),
    }
}

#[test]
fn open_options_rejects_cache_with_durable_path_requirement() {
    let options = StorageOpenOptions::cache().with_strict_recovery(false);
    let error = options
        .validate()
        .expect_err("cache cannot request durable recovery fallback");

    assert_eq!(error.code(), "invalid_argument.storage_api.argument");
    assert!(!options.requires_backend());
}

#[test]
fn open_options_rejects_durable_without_local_path() {
    let error = StorageRuntime::open(StorageOpenOptions::durable_local(
        StorageDurabilityPolicy::Standard,
    ))
    .expect_err("durable local open requires explicit backend");

    assert_eq!(error.code(), "invalid_argument.storage_api.argument");
}

#[test]
fn open_options_rejects_object_durable_candidate() {
    let error = StorageOpenOptions::object_durable_candidate()
        .validate()
        .expect_err("object durable mode is unsupported");

    assert_eq!(error.code(), "unsupported.storage_api.capability");
    assert_eq!(error.class(), StorageApiErrorClass::Unsupported);
}

#[test]
fn open_options_rejects_distributed_writer_mode() {
    let error = StorageOpenOptions::distributed_candidate()
        .validate()
        .expect_err("distributed writer mode is unsupported");

    assert_eq!(error.code(), "unsupported.storage_api.capability");
    assert_eq!(error.class(), StorageApiErrorClass::Unsupported);
}

#[test]
fn open_options_preserves_budget_policy() {
    let options = StorageOpenOptions::durable_local(StorageDurabilityPolicy::Always)
        .with_budget_policy(StorageBudgetPolicy::LowMemory)
        .with_wal_growth_policy(StorageWalGrowthPolicy::Disabled);

    assert_eq!(options.budget_policy(), StorageBudgetPolicy::LowMemory);
    assert_eq!(
        options.wal_growth_policy(),
        StorageWalGrowthPolicy::Disabled
    );
    assert!(options.validate().is_ok());
}

#[test]
fn open_options_default_to_background_maintenance_policy() {
    for options in [
        StorageOpenOptions::cache(),
        StorageOpenOptions::ephemeral(),
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        StorageOpenOptions::object_durable_candidate(),
        StorageOpenOptions::distributed_candidate(),
    ] {
        assert_eq!(
            options.maintenance_scheduling_policy(),
            StorageMaintenanceSchedulingPolicy::Background
        );
    }
}

#[test]
fn open_options_preserves_explicit_maintenance_policy() {
    let options = StorageOpenOptions::cache()
        .with_maintenance_scheduling_policy(StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue);

    assert_eq!(
        options.maintenance_scheduling_policy(),
        StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue
    );
    assert!(options.validate().is_ok());
}

#[test]
fn source_guard_public_open_options_do_not_default_to_deterministic_inline() {
    let options_source = include_str!("../options.rs");

    assert!(
        !options_source
            .contains("maintenance_scheduling_policy: StorageMaintenanceSchedulingPolicy::DeterministicInline"),
        "public open option constructors must not default to deterministic inline maintenance"
    );
}

#[test]
fn source_guard_api_does_not_export_background_scheduler_internals() {
    let api_mod_source = include_str!("../mod.rs");

    for private_symbol in [
        "BackgroundScheduler",
        "BackgroundSchedulerStats",
        "BackgroundTaskPriority",
        "BackgroundBackpressureError",
    ] {
        assert!(
            !api_mod_source.contains(private_symbol),
            "public API module must not expose old-engine/background scheduler internals: {private_symbol}"
        );
    }
}

#[test]
fn source_guard_background_scheduler_is_local_storage_next_port() {
    let background_source = include_str!("../../lifecycle/background.rs");

    assert!(
        background_source.contains("storage-next port of `crates/engine/src/background.rs`"),
        "background scheduler must document the old-engine source it ports"
    );
    assert!(
        !background_source.contains("strata_engine")
            && !background_source.contains("strata-engine"),
        "storage-next background scheduler must be a local port, not a strata-engine dependency"
    );
}

#[test]
fn source_guard_background_priority_maps_lifecycle_work_by_pressure_cost() {
    let runtime_source = include_str!("../runtime.rs");
    let priority_mapping = runtime_source
        .split("const fn background_priority_for_task_request")
        .nth(1)
        .expect("background priority mapping function is present")
        .split("fn drain_cache_background_round")
        .next()
        .expect("priority mapping precedes background drain");
    let compact_source = priority_mapping.split_whitespace().collect::<String>();

    assert!(
        compact_source.contains(
            "LifecycleMaintenanceTaskKind::Flush|LifecycleMaintenanceTaskKind::Checkpoint|LifecycleMaintenanceTaskKind::FlushWatermark|LifecycleMaintenanceTaskKind::WalTruncation=>BackgroundTaskPriority::High"
        ),
        "flush/checkpoint/WAL-retention work must wake the background worker at high priority"
    );
    assert!(
        compact_source.contains(
            "LifecycleMaintenanceTaskKind::Compaction|LifecycleMaintenanceTaskKind::Materialization=>BackgroundTaskPriority::Normal"
        ),
        "compaction and materialization must use normal background priority"
    );
    assert!(
        compact_source.contains(
            "LifecycleMaintenanceTaskKind::HealthCollection|LifecycleMaintenanceTaskKind::Retention|LifecycleMaintenanceTaskKind::SnapshotPruning|LifecycleMaintenanceTaskKind::Quarantine|LifecycleMaintenanceTaskKind::Purge|LifecycleMaintenanceTaskKind::Repair=>BackgroundTaskPriority::Low"
        ),
        "health, retention, pruning, quarantine, purge, and repair work must stay low priority"
    );
}

#[test]
fn open_options_reject_cache_lossy_recovery() {
    let options = StorageOpenOptions::cache().with_strict_recovery(false);
    let validation = options
        .validate()
        .expect_err("cache lossy recovery rejected");
    let open = StorageRuntime::open(options).expect_err("cache lossy recovery rejected");

    assert_eq!(validation.code(), "invalid_argument.storage_api.argument");
    assert_eq!(open.code(), "invalid_argument.storage_api.argument");
}

#[test]
fn open_options_rejects_durable_without_local_backend() {
    let error = StorageRuntime::open(StorageOpenOptions::durable_local(
        StorageDurabilityPolicy::Standard,
    ))
    .expect_err("durable local open requires explicit backend");

    assert_eq!(error.code(), "invalid_argument.storage_api.argument");
}

#[test]
fn open_options_preserves_recovery_strictness() {
    let strict = StorageOpenOptions::cache();
    let lossy = StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
        .with_strict_recovery(false);

    assert!(strict.strict_recovery());
    assert!(!lossy.strict_recovery());
}

#[test]
fn open_cache_returns_open_runtime_and_cache_summary() {
    let outcome =
        StorageRuntime::open(StorageOpenOptions::cache()).expect("cache open should succeed");
    let summary = outcome.summary();
    let runtime = outcome.into_runtime();

    assert_eq!(runtime.state(), StorageRuntimeState::Open);
    assert!(runtime.is_open());
    assert_eq!(summary.mode(), StorageMode::Cache);
    assert_eq!(summary.disposition(), StorageOpenDisposition::Created);
    assert_eq!(summary.recovery_health(), RecoveryHealthSummary::Healthy);
    assert_eq!(summary.recovered_visible_version(), None);
    assert!(summary.maintenance_ready());
    assert_eq!(
        summary.maintenance_scheduling_policy(),
        StorageMaintenanceSchedulingPolicy::Background
    );
    assert!(!summary.has_durable_recovery_facts());
    assert!(summary.backend_capabilities_used());
    assert_eq!(
        runtime.maintenance_scheduling_policy_for_test(),
        crate::lifecycle::LifecycleMaintenanceSchedulingPolicy::Background
    );
    let queue = runtime.maintenance_status().expect("maintenance status");
    assert_eq!(queue.background_worker_count(), 1);
    assert_eq!(queue.background_queue_depth(), 0);
    assert_eq!(queue.background_active_tasks(), 0);
}

#[test]
fn open_ephemeral_returns_open_runtime_and_cache_summary() {
    let outcome = StorageRuntime::open_ephemeral().expect("ephemeral open");
    let summary = outcome.summary();
    let runtime = outcome.into_runtime();

    assert_eq!(runtime.state(), StorageRuntimeState::Open);
    assert_eq!(summary.mode(), StorageMode::Cache);
    assert_eq!(summary.disposition(), StorageOpenDisposition::Created);
    assert_eq!(summary.recovery_health(), RecoveryHealthSummary::Healthy);
    assert_eq!(
        summary.maintenance_scheduling_policy(),
        StorageMaintenanceSchedulingPolicy::Background
    );
    assert!(!summary.has_durable_recovery_facts());
    assert_eq!(
        runtime.maintenance_scheduling_policy_for_test(),
        crate::lifecycle::LifecycleMaintenanceSchedulingPolicy::Background
    );
    assert_eq!(
        runtime
            .maintenance_status()
            .expect("maintenance status")
            .background_worker_count(),
        1
    );
}

#[test]
fn open_cache_helper_returns_open_runtime_and_cache_summary() {
    let outcome = StorageRuntime::open_cache().expect("cache helper open");
    let summary = outcome.summary();
    let runtime = outcome.into_runtime();

    assert_eq!(runtime.state(), StorageRuntimeState::Open);
    assert_eq!(summary.mode(), StorageMode::Cache);
    assert_eq!(summary.disposition(), StorageOpenDisposition::Created);
    assert_eq!(
        summary.maintenance_scheduling_policy(),
        StorageMaintenanceSchedulingPolicy::Background
    );
    assert_eq!(
        runtime
            .maintenance_status()
            .expect("maintenance status")
            .background_worker_count(),
        1
    );
}

#[test]
fn open_cache_can_select_non_background_maintenance_policies_for_tests() {
    for (api_policy, lifecycle_policy) in [
        (
            StorageMaintenanceSchedulingPolicy::DeterministicInline,
            crate::lifecycle::LifecycleMaintenanceSchedulingPolicy::DeterministicInline,
        ),
        (
            StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
            crate::lifecycle::LifecycleMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
        ),
        (
            StorageMaintenanceSchedulingPolicy::Disabled,
            crate::lifecycle::LifecycleMaintenanceSchedulingPolicy::Disabled,
        ),
    ] {
        let outcome = StorageRuntime::open(
            StorageOpenOptions::cache().with_maintenance_scheduling_policy(api_policy),
        )
        .expect("cache open should preserve explicit maintenance policy");
        let summary = outcome.summary();
        let runtime = outcome.into_runtime();

        assert_eq!(summary.maintenance_scheduling_policy(), api_policy);
        assert_eq!(
            runtime.maintenance_scheduling_policy_for_test(),
            lifecycle_policy
        );
        assert_eq!(
            runtime
                .maintenance_status()
                .expect("maintenance status")
                .background_worker_count(),
            0
        );
    }
}

#[cfg(feature = "perf-trace")]
#[test]
fn disabled_maintenance_policy_skips_api_post_commit_enqueue_and_worker_wake() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let mut runtime = StorageRuntime::open(
        StorageOpenOptions::cache()
            .with_maintenance_scheduling_policy(StorageMaintenanceSchedulingPolicy::Disabled),
    )
    .expect("disabled cache open")
    .into_runtime();
    let batch = CommitBatch::new(
        StorageRuntime::default_branch_id_for_test(),
        vec![CommitMutation::Put {
            storage_space: StorageSpaceId::new(vec![0x20]).expect("engine storage space"),
            key: key(b"disabled-maintenance"),
            value: StorageValue::new(b"value".to_vec()),
            ttl: None,
        }],
        CommitOptions::default(),
    )
    .expect("valid commit batch");

    runtime.commit(&batch).expect("disabled commit");

    let queue = runtime.maintenance_status().expect("maintenance status");
    assert_eq!(queue.pending_tasks(), 0);
    assert_eq!(queue.background_worker_count(), 0);

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_post_commit_maintenance_evaluations(), 1);
    assert_eq!(perf.lifecycle_post_commit_maintenance_disabled(), 1);
    assert_eq!(perf.lifecycle_post_commit_maintenance_tasks_enqueued(), 0);
    assert_eq!(perf.lifecycle_background_runtimes_created(), 0);
}

#[test]
fn maintenance_status_reports_queue_state_without_background_worker_in_manual_mode() {
    let mut runtime = StorageRuntime::open(
        StorageOpenOptions::cache().with_maintenance_scheduling_policy(
            StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
        ),
    )
    .expect("manual maintenance cache open")
    .into_runtime();
    let branch = StorageRuntime::default_branch_id_for_test();

    let queue = runtime
        .enqueue_maintenance(&MaintenanceRequest::new(
            MaintenanceTask::Flush,
            MaintenanceScope::Branch(branch),
        ))
        .expect("enqueue maintenance");

    assert_eq!(queue.pending_tasks(), 1);
    assert_eq!(queue.enqueued(), 1);
    assert_eq!(queue.background_worker_count(), 0);
    assert_eq!(queue.background_queue_depth(), 0);
    assert_eq!(queue.background_active_tasks(), 0);

    let status = runtime.maintenance_status().expect("maintenance status");
    assert_eq!(status.pending_tasks(), 1);
    assert_eq!(status.background_worker_count(), 0);
}

#[test]
fn background_manual_mode_run_next_maintenance_drains_explicit_work_without_worker() {
    let mut runtime = StorageRuntime::open(
        StorageOpenOptions::cache().with_maintenance_scheduling_policy(
            StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
        ),
    )
    .expect("manual maintenance cache open")
    .into_runtime();
    let branch = StorageRuntime::default_branch_id_for_test();

    runtime
        .commit(&background_put_batch(b"manual-run-next", b"value".to_vec()))
        .expect("seed active table data");
    runtime
        .rotate_default_branch_for_test()
        .expect("rotate active table into frozen source");

    let queue = runtime
        .enqueue_maintenance(&MaintenanceRequest::new(
            MaintenanceTask::Flush,
            MaintenanceScope::Branch(branch),
        ))
        .expect("enqueue manual flush maintenance");
    assert_eq!(queue.pending_tasks(), 1);
    assert_eq!(queue.background_worker_count(), 0);

    let outcome = runtime
        .run_next_maintenance()
        .expect("manual run-next maintenance")
        .expect("queued flush maintenance outcome");
    assert_eq!(outcome.task(), MaintenanceTask::Flush);
    assert_eq!(outcome.scope(), MaintenanceScope::Branch(branch));
    assert_eq!(outcome.status(), MaintenanceSummaryStatus::Completed);

    let status = runtime.maintenance_status().expect("maintenance status");
    assert_eq!(status.pending_tasks(), 0);
    assert_eq!(status.background_worker_count(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn background_explicit_enqueue_wakes_and_drains_queue() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let mut runtime = StorageRuntime::open_cache()
        .expect("background cache open")
        .into_runtime();
    let branch = StorageRuntime::default_branch_id_for_test();

    let queue = runtime
        .enqueue_maintenance(&MaintenanceRequest::new(
            MaintenanceTask::Flush,
            MaintenanceScope::Branch(branch),
        ))
        .expect("enqueue background maintenance");
    assert_eq!(queue.pending_tasks(), 1);

    runtime.wait_background_idle_for_test();

    let status = runtime.maintenance_status().expect("maintenance status");
    assert_eq!(status.pending_tasks(), 0);
    assert_eq!(status.background_worker_count(), 1);

    let perf = crate::observability::perf_trace::snapshot();
    assert!(perf.lifecycle_background_wake_submitted() >= 1);
    assert!(perf.lifecycle_background_drain_rounds() >= 1);
    assert!(perf.lifecycle_background_tasks_completed() >= 1);
}

#[cfg(feature = "perf-trace")]
#[test]
fn background_compaction_enqueue_wakes_and_drains_table_rewrite() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let mut runtime = StorageRuntime::open_cache()
        .expect("background cache open")
        .into_runtime();
    let branch = StorageRuntime::default_branch_id_for_test();

    for index in 0..2 {
        let key = format!("background-compact-{index}");
        runtime
            .append_raw_row_for_test(background_raw_row(
                key.as_bytes(),
                u64::try_from(index + 1).expect("index fits"),
            ))
            .expect("seed raw active row");
        runtime
            .flush_default_branch_for_test()
            .expect("flush raw row into L0 table");
    }
    let before = runtime
        .branch_source_layout_for_test(branch)
        .expect("pre-compaction source layout");
    assert_eq!(before.owned_l0_tables(), 2);

    runtime
        .enqueue_maintenance(&MaintenanceRequest::new(
            MaintenanceTask::Compact,
            MaintenanceScope::Branch(branch),
        ))
        .expect("enqueue background compaction");
    runtime.wait_background_idle_for_test();

    let after = runtime
        .branch_source_layout_for_test(branch)
        .expect("post-compaction source layout");
    assert_eq!(after.owned_l0_tables(), 0);
    assert_eq!(background_owned_table_count_at(&after, 1), 1);
    assert_eq!(
        runtime
            .maintenance_status()
            .expect("maintenance status")
            .pending_tasks(),
        0
    );

    let perf = crate::observability::perf_trace::snapshot();
    assert!(perf.lifecycle_background_wake_submitted() >= 1);
    assert!(perf.lifecycle_background_tasks_completed() >= 1);
    assert!(perf.lifecycle_compaction_operations_completed() >= 1);
}

#[cfg(feature = "perf-trace")]
#[test]
fn background_materialization_enqueue_wakes_and_drains_table_rewrite() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let mut runtime = StorageRuntime::open_cache()
        .expect("background cache open")
        .into_runtime();
    let parent = StorageRuntime::default_branch_id_for_test();
    let child = branch_id(0x91);

    runtime
        .append_raw_row_for_test(background_raw_row(b"background-materialize", 1))
        .expect("seed parent row");
    runtime
        .flush_default_branch_for_test()
        .expect("flush parent row into an inheritable table");
    runtime
        .fork_default_branch_for_test(child)
        .expect("fork child from parent");

    let before = runtime
        .branch_source_layout_for_test(child)
        .expect("pre-materialization child layout");
    assert_eq!(before.inherited_layers(), 1);
    assert_eq!(before.inherited_total_tables(), 1);

    runtime
        .enqueue_maintenance(&MaintenanceRequest::new(
            MaintenanceTask::Materialize,
            MaintenanceScope::Branch(child),
        ))
        .expect("enqueue background materialization");
    runtime.wait_background_idle_for_test();

    let after = runtime
        .branch_source_layout_for_test(child)
        .expect("post-materialization child layout");
    assert_eq!(after.inherited_total_tables(), 0);
    assert_eq!(after.owned_total_tables(), 1);
    assert_eq!(
        runtime
            .maintenance_status()
            .expect("maintenance status")
            .pending_tasks(),
        0
    );
    let read = runtime
        .read_point(&PointReadRequest::new(
            child,
            background_space(),
            key(b"background-materialize"),
            ReadBound::Latest,
        ))
        .expect("read materialized child row");
    assert_eq!(
        read.row()
            .expect("materialized row")
            .value()
            .expect("value")
            .as_bytes(),
        b"value"
    );

    let perf = crate::observability::perf_trace::snapshot();
    assert!(perf.lifecycle_background_wake_submitted() >= 1);
    assert!(perf.lifecycle_background_tasks_completed() >= 1);
    assert!(perf.branch_materialization_output_tables() >= 1);

    let parent_layout = runtime
        .branch_source_layout_for_test(parent)
        .expect("parent layout");
    assert_eq!(parent_layout.owned_total_tables(), 1);
}

#[cfg(feature = "perf-trace")]
#[test]
fn background_duplicate_enqueue_coalesces_wake_while_worker_busy() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Barrier};

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let mut runtime = StorageRuntime::open_cache()
        .expect("background cache open")
        .into_runtime();
    let ready = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let observed_open = Arc::new(AtomicBool::new(false));
    assert!(
        runtime.submit_runtime_state_background_probe_for_test(
            Arc::clone(&ready),
            Arc::clone(&release),
            Arc::clone(&observed_open),
        ),
        "background runtime should accept probe"
    );
    ready.wait();

    let branch = StorageRuntime::default_branch_id_for_test();
    for _ in 0..2 {
        runtime
            .enqueue_maintenance(&MaintenanceRequest::new(
                MaintenanceTask::Flush,
                MaintenanceScope::Branch(branch),
            ))
            .expect("enqueue duplicate background maintenance");
    }

    let perf = crate::observability::perf_trace::snapshot();
    assert!(perf.lifecycle_background_wake_submitted() >= 1);
    assert!(perf.lifecycle_background_wake_coalesced() >= 1);

    release.wait();
    runtime.wait_background_idle_for_test();
    assert!(observed_open.load(Ordering::Acquire));
    assert_eq!(
        runtime
            .maintenance_status()
            .expect("maintenance status")
            .pending_tasks(),
        0
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn background_stale_wake_records_noop_without_pending_task() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let runtime = StorageRuntime::open_cache()
        .expect("background cache open")
        .into_runtime();

    runtime.submit_stale_background_wake_for_test();
    runtime.wait_background_idle_for_test();

    let status = runtime.maintenance_status().expect("maintenance status");
    assert_eq!(status.pending_tasks(), 0);

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_background_stale_wake_noop(), 1);
    assert_eq!(perf.lifecycle_background_drain_rounds(), 1);
}

#[cfg(feature = "perf-trace")]
#[test]
fn background_wake_after_shutdown_records_rejected_without_running_task() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let runtime = StorageRuntime::open_cache()
        .expect("background cache open")
        .into_runtime();

    runtime.shutdown_background_for_test();
    runtime.submit_stale_background_wake_for_test();

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_background_wake_rejected(), 1);
    assert_eq!(perf.lifecycle_background_stale_wake_noop(), 0);
}

#[cfg(all(feature = "localfs", feature = "perf-trace"))]
#[test]
fn background_wal_growth_checkpoint_wakes_and_drains() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let backend = Box::leak(Box::new(StorageBackend::local_fs(temp_dir_for_api_test(
        "background-wal-growth",
    ))));
    let mut runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
            .with_wal_growth_policy(StorageWalGrowthPolicy::thresholds(u64::MAX, usize::MAX, 1)),
        backend,
    )
    .expect("durable background open")
    .into_runtime();

    runtime
        .commit(&background_put_batch(b"wal-growth", b"value".to_vec()))
        .expect("commit below WAL growth threshold");
    let commit = runtime
        .commit(&background_put_batch(b"wal-growth-next", b"value".to_vec()))
        .expect("commit crossing WAL growth threshold");
    runtime.wait_background_idle_for_test();

    let status = runtime.maintenance_status().expect("maintenance status");
    assert_eq!(status.pending_tasks(), 0);
    let report = runtime
        .diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Global))
        .expect("diagnostics");
    assert_eq!(report.wal_growth().state(), DiagnosticsFactState::Known);
    assert!(matches!(
        report
            .wal_growth()
            .last_status()
            .map(|summary| summary.status()),
        Some(
            MaintenanceWalGrowthStatus::CheckpointEnqueued
                | MaintenanceWalGrowthStatus::CheckpointCoalesced
        )
    ));
    assert_eq!(report.checkpoint().state(), DiagnosticsFactState::Known);
    assert!(
        report
            .checkpoint()
            .checkpoint_watermark()
            .is_some_and(|watermark| watermark >= commit.commit_version()),
        "background checkpoint did not advance to the WAL-growth commit"
    );

    let perf = crate::observability::perf_trace::snapshot();
    assert!(perf.lifecycle_background_wake_submitted() >= 1);
    assert!(perf.lifecycle_background_tasks_completed() >= 1);
}

#[cfg(feature = "perf-trace")]
#[test]
fn background_urgent_pressure_records_slowdown_without_inline_maintenance() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let mut runtime = StorageRuntime::open(
        StorageOpenOptions::cache().with_budget_policy(StorageBudgetPolicy::LowMemory),
    )
    .expect("low-memory background cache open")
    .into_runtime();

    runtime
        .commit(&background_put_batch(b"urgent-seed", b"value".to_vec()))
        .expect("seed active row");
    runtime
        .rotate_default_branch_for_test()
        .expect("create frozen pressure");
    runtime
        .commit(&background_put_batch(b"urgent-followup", b"value".to_vec()))
        .expect("urgent commit should be accepted");
    runtime.wait_background_idle_for_test();

    let perf = crate::observability::perf_trace::snapshot();
    assert!(perf.lifecycle_write_admission_slowdown_attempts() >= 1);
    assert!(perf.lifecycle_write_admission_slowdown_ns() > 0);
    assert_eq!(perf.lifecycle_inline_maintenance_attempts(), 0);
    assert!(perf.lifecycle_background_tasks_completed() >= 1);
    assert_eq!(
        runtime
            .maintenance_status()
            .expect("maintenance status")
            .pending_tasks(),
        0
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn background_block_pressure_waits_for_flush_progress_before_retry() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let mut runtime = StorageRuntime::open_cache()
        .expect("background cache open")
        .into_runtime();

    for index in 0..4 {
        let key = format!("block-seed-{index}");
        runtime
            .append_raw_row_for_test(background_raw_row(key.as_bytes(), 0))
            .expect("seed raw active row before rotation");
        runtime
            .rotate_default_branch_for_test()
            .expect("create frozen table");
    }
    assert_eq!(
        runtime
            .maintenance_status()
            .expect("maintenance status before block")
            .pending_tasks(),
        0
    );

    runtime
        .commit(&background_put_batch(b"block-followup", b"value".to_vec()))
        .expect("block pressure should wait and retry after background flush");
    runtime.wait_background_idle_for_test();

    let perf = crate::observability::perf_trace::snapshot();
    assert!(perf.lifecycle_write_admission_wait_attempts() >= 1);
    assert_eq!(perf.lifecycle_write_admission_wait_timeouts(), 0);
    assert!(perf.lifecycle_write_admission_block_wait_ns() > 0);
    assert!(perf.lifecycle_background_tasks_completed() >= 1);
    assert!(perf.lifecycle_pressure_clear_wakes() >= 1);
    assert_eq!(
        runtime
            .maintenance_status()
            .expect("maintenance status")
            .pending_tasks(),
        0
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn background_block_pressure_wait_has_deadline_when_worker_is_busy() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Barrier};
    use std::time::{Duration, Instant};

    let _capture = crate::observability::perf_trace::begin_test_capture();
    let mut runtime = StorageRuntime::open_cache()
        .expect("background cache open")
        .into_runtime();

    let ready = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let observed_open = Arc::new(AtomicBool::new(false));
    assert!(
        runtime.submit_runtime_state_background_probe_for_test(
            Arc::clone(&ready),
            Arc::clone(&release),
            Arc::clone(&observed_open),
        ),
        "background runtime should accept probe"
    );
    ready.wait();

    for index in 0..4 {
        let key = format!("block-timeout-seed-{index}");
        runtime
            .append_raw_row_for_test(background_raw_row(key.as_bytes(), 0))
            .expect("seed raw active row before rotation");
        runtime
            .rotate_default_branch_for_test()
            .expect("create frozen table");
    }

    let start = Instant::now();
    let error = runtime
        .commit(&background_put_batch(b"block-timeout", b"value".to_vec()))
        .expect_err("blocked background worker should leave pressure rejected");
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "pressure wait ignored its bounded deadline"
    );
    assert!(matches!(
        error,
        StorageApiError::StoragePressure {
            severity: CommitAdmissionPressureSeverity::Blocking,
            retryable: true,
            ..
        }
    ));

    let perf = crate::observability::perf_trace::snapshot();
    assert!(perf.lifecycle_write_admission_wait_attempts() >= 1);
    assert_eq!(perf.lifecycle_write_admission_wait_timeouts(), 1);
    assert!(perf.lifecycle_write_admission_block_wait_ns() > 0);

    release.wait();
    runtime.wait_background_idle_for_test();
    assert!(observed_open.load(Ordering::Acquire));
}

#[cfg(feature = "perf-trace")]
#[test]
fn background_runtime_creation_perf_trace_records_only_background_opens() {
    let _capture = crate::observability::perf_trace::begin_test_capture();

    let background = StorageRuntime::open_cache().expect("background cache open");
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_background_runtimes_created(), 1);
    assert_eq!(perf.lifecycle_background_runtime_workers_created(), 1);

    let enqueue = StorageRuntime::open(
        StorageOpenOptions::cache().with_maintenance_scheduling_policy(
            StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
        ),
    )
    .expect("explicit enqueue cache open");
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_background_runtimes_created(), 1);
    assert_eq!(perf.lifecycle_background_runtime_workers_created(), 1);

    drop(enqueue);
    drop(background);
}

#[test]
fn background_close_drains_queued_work_before_lifecycle_close() {
    let runtime = StorageRuntime::open_cache()
        .expect("background cache open")
        .into_runtime();

    assert_background_close_drains_queued_work_before_lifecycle_close(runtime);
}

#[cfg(feature = "localfs")]
#[test]
fn durable_background_close_drains_queued_work_before_lifecycle_close() {
    let runtime = StorageRuntime::open_local(temp_dir_for_api_test("durable-background-close"))
        .expect("durable background open")
        .into_runtime();

    assert_background_close_drains_queued_work_before_lifecycle_close(runtime);
}

fn assert_background_close_drains_queued_work_before_lifecycle_close(
    mut runtime: StorageRuntime<'static>,
) {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Barrier};
    use std::time::{Duration, Instant};

    let shutdown_requested = runtime
        .background_shutdown_requested_flag_for_test()
        .expect("background runtime should expose shutdown flag");
    let ready = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let observed_open = Arc::new(AtomicBool::new(false));
    let close_returned = Arc::new(AtomicBool::new(false));

    assert!(
        runtime.submit_runtime_state_background_probe_for_test(
            Arc::clone(&ready),
            Arc::clone(&release),
            Arc::clone(&observed_open),
        ),
        "background runtime should accept the probe task"
    );
    ready.wait();

    std::thread::scope(|scope| {
        let close_returned_worker = Arc::clone(&close_returned);
        let runtime_ref = &mut runtime;
        let close_handle = scope.spawn(move || {
            let summary = runtime_ref.close().expect("close background runtime");
            close_returned_worker.store(true, Ordering::Release);
            summary
        });

        let deadline = Instant::now() + Duration::from_secs(5);
        while !shutdown_requested.load(Ordering::Acquire) {
            assert!(
                Instant::now() < deadline,
                "background close did not request scheduler shutdown"
            );
            std::thread::yield_now();
        }
        assert!(
            !close_returned.load(Ordering::Acquire),
            "close returned before the accepted background task was released"
        );

        release.wait();
        let summary = close_handle.join().expect("join close thread");
        assert_eq!(summary.state(), StorageRuntimeState::Closed);
    });

    assert!(
        observed_open.load(Ordering::Acquire),
        "queued background task must run before lifecycle close transitions the runtime"
    );
    assert_eq!(runtime.state(), StorageRuntimeState::Closed);
}

#[test]
#[cfg(feature = "localfs")]
fn open_local_returns_durable_standard_runtime() {
    let outcome = StorageRuntime::open_local(temp_dir_for_api_test("open-local"))
        .expect("local durable open should succeed");
    let summary = outcome.summary();
    let mut runtime = outcome.into_runtime();

    assert_eq!(
        summary.mode(),
        StorageMode::DurableLocal {
            policy: StorageDurabilityPolicy::Standard,
        }
    );
    assert_eq!(summary.disposition(), StorageOpenDisposition::Created);
    assert_eq!(summary.recovery_health(), RecoveryHealthSummary::Healthy);
    assert!(summary.recovered_visible_version().is_some());
    assert!(summary.has_durable_recovery_facts());
    assert!(summary.backend_capabilities_used());
    assert_eq!(
        summary.maintenance_scheduling_policy(),
        StorageMaintenanceSchedulingPolicy::Background
    );
    assert_eq!(runtime.state(), StorageRuntimeState::Open);
    assert_eq!(
        runtime.maintenance_scheduling_policy_for_test(),
        crate::lifecycle::LifecycleMaintenanceSchedulingPolicy::Background
    );
    assert_eq!(
        runtime
            .maintenance_status()
            .expect("maintenance status")
            .background_worker_count(),
        1
    );

    let close = runtime.close().expect("local durable close");
    assert!(close.durable_synced());
}

#[test]
#[cfg(feature = "localfs")]
fn open_local_reopens_persisted_commits_from_same_root() {
    let root = temp_dir_for_api_test("open-local-reopen");
    let branch = StorageRuntime::default_branch_id_for_test();
    let storage_space = StorageSpaceId::new(vec![0x20]).expect("valid engine storage space");
    let storage_key = key(b"persisted");
    let storage_value = StorageValue::new(b"value".to_vec());
    let batch = CommitBatch::new(
        branch,
        vec![CommitMutation::Put {
            storage_space: storage_space.clone(),
            key: storage_key.clone(),
            value: storage_value.clone(),
            ttl: None,
        }],
        CommitOptions::default(),
    )
    .expect("valid put batch");

    let mut first = StorageRuntime::open_local(root.clone())
        .expect("first local durable open")
        .into_runtime();
    first.commit(&batch).expect("persisted commit");
    first.close().expect("first local durable close");
    drop(first);

    let second = StorageRuntime::open_local(root).expect("second local durable open");
    let second_summary = second.summary();
    let second = second.into_runtime();
    let read = second
        .read_point(&PointReadRequest::new(
            branch,
            storage_space,
            storage_key,
            ReadBound::Latest,
        ))
        .expect("read persisted value");
    let row = read.row().expect("persisted row");

    assert_eq!(
        second_summary.disposition(),
        StorageOpenDisposition::OpenedExisting
    );
    assert_eq!(row.value(), Some(&storage_value));
}

#[test]
#[cfg(feature = "localfs")]
fn open_durable_local_returns_requested_policy() {
    let outcome = StorageRuntime::open_durable_local(
        temp_dir_for_api_test("open-durable-local-always"),
        StorageDurabilityPolicy::Always,
    )
    .expect("local durable open should succeed");
    let summary = outcome.summary();
    let mut runtime = outcome.into_runtime();

    assert_eq!(
        summary.mode(),
        StorageMode::DurableLocal {
            policy: StorageDurabilityPolicy::Always,
        }
    );
    assert!(summary.has_durable_recovery_facts());

    let close = runtime.close().expect("local durable close");
    assert!(close.durable_synced());
}

#[test]
#[cfg(not(feature = "localfs"))]
fn open_local_without_localfs_rejects_without_cache_fallback() {
    let outcome = StorageRuntime::open_local(std::path::PathBuf::from("no-localfs"));

    match outcome {
        Ok(open) => {
            let summary = open.summary();
            panic!(
                "open_local unexpectedly succeeded in mode {:?}",
                summary.mode()
            );
        }
        Err(error) => {
            assert_eq!(error.class(), StorageApiErrorClass::Unsupported);
            assert_eq!(error.code(), "unsupported.storage_api.capability");
        }
    }
}

#[test]
fn open_cache_returns_open_runtime() {
    let outcome = StorageRuntime::open(StorageOpenOptions::cache()).expect("cache open");
    let runtime = outcome.into_runtime();

    assert_eq!(runtime.state(), StorageRuntimeState::Open);
    assert!(runtime.is_open());
}

#[test]
fn open_cache_reports_cache_mode() {
    let outcome = StorageRuntime::open(StorageOpenOptions::cache()).expect("cache open");

    assert_eq!(outcome.summary().mode(), StorageMode::Cache);
}

#[test]
fn open_cache_reports_no_durable_recovery_facts() {
    let outcome = StorageRuntime::open(StorageOpenOptions::cache()).expect("cache open");

    assert!(!outcome.summary().has_durable_recovery_facts());
}

#[test]
fn open_cache_does_not_construct_wal_or_manifest_services() {
    let outcome = StorageRuntime::open(StorageOpenOptions::cache()).expect("cache open");
    let mut runtime = outcome.into_runtime();
    let close = runtime.close().expect("cache close");

    assert!(!close.durable_synced());
}

#[test]
fn open_cache_close_is_idempotent() {
    let outcome =
        StorageRuntime::open(StorageOpenOptions::cache()).expect("cache open should succeed");
    let mut runtime = outcome.into_runtime();

    let first = runtime.close().expect("first close");
    assert_eq!(first.state(), StorageRuntimeState::Closed);
    assert!(!first.idempotent());
    assert!(first.commits_quiesced());
    assert!(first.maintenance_drained());
    assert!(!first.durable_synced());
    assert!(first.guards_released());

    let second = runtime.close().expect("second close");
    assert_eq!(second.state(), StorageRuntimeState::Closed);
    assert!(second.idempotent());
}

#[test]
fn close_open_cache_returns_final_facts() {
    let outcome = StorageRuntime::open(StorageOpenOptions::cache()).expect("cache open");
    let mut runtime = outcome.into_runtime();
    let close = runtime.close().expect("cache close");

    assert_eq!(close.state(), StorageRuntimeState::Closed);
    assert!(close.commits_quiesced());
    assert!(close.maintenance_drained());
    assert!(!close.durable_synced());
    assert!(close.guards_released());
}

#[test]
fn close_twice_returns_idempotent_outcome() {
    let outcome = StorageRuntime::open(StorageOpenOptions::cache()).expect("cache open");
    let mut runtime = outcome.into_runtime();

    let first = runtime.close().expect("first close");
    let second = runtime.close().expect("second close");

    assert!(!first.idempotent());
    assert!(second.idempotent());
    assert_eq!(second.state(), StorageRuntimeState::Closed);
}

#[test]
#[cfg(feature = "localfs")]
fn close_failure_preserves_source_chain() {
    let backend = StorageBackend::local_fs(temp_dir_for_api_test("durable-close-failure"));
    let outcome = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("durable open should succeed");
    let mut runtime = outcome.into_runtime();
    assert!(runtime.release_writer_guard_for_test());

    let error = runtime
        .close()
        .expect_err("missing writer guard fails close");

    assert_eq!(error.code(), "internal.storage_api.lower_layer");
    assert_eq!(error.class(), StorageApiErrorClass::Internal);
    let source = error.source().expect("lifecycle source is preserved");
    assert!(source.is::<crate::lifecycle::LifecycleError>());
}

#[test]
fn close_then_read_rejects_closed_runtime() {
    let outcome = StorageRuntime::open(StorageOpenOptions::cache()).expect("cache open");
    let mut runtime = outcome.into_runtime();
    runtime.close().expect("close");

    let error = runtime
        .require_open("read requires an open storage runtime")
        .expect_err("closed runtime rejects read");

    assert_eq!(error.code(), "failed_precondition.storage_api.state");
}

#[test]
fn close_then_commit_rejects_closed_runtime() {
    let outcome = StorageRuntime::open(StorageOpenOptions::cache()).expect("cache open");
    let mut runtime = outcome.into_runtime();
    runtime.close().expect("close");

    let error = runtime
        .require_open("commit requires an open storage runtime")
        .expect_err("closed runtime rejects commit");

    assert_eq!(error.code(), "failed_precondition.storage_api.state");
}

#[test]
fn close_then_maintenance_rejects_closed_runtime() {
    let outcome = StorageRuntime::open(StorageOpenOptions::cache()).expect("cache open");
    let mut runtime = outcome.into_runtime();
    runtime.close().expect("close");

    let error = runtime
        .require_open("maintenance requires an open storage runtime")
        .expect_err("closed runtime rejects maintenance");

    assert_eq!(error.code(), "failed_precondition.storage_api.state");
}

#[test]
fn open_cache_operation_after_close_rejects() {
    let outcome =
        StorageRuntime::open(StorageOpenOptions::cache()).expect("cache open should succeed");
    let mut runtime = outcome.into_runtime();

    runtime.close().expect("close");
    let error = runtime
        .require_open("read requires an open storage runtime")
        .expect_err("closed runtime rejects operation");

    assert_eq!(error.code(), "failed_precondition.storage_api.state");
}

#[test]
#[cfg(feature = "localfs")]
fn open_durable_modes_return_open_runtime() {
    for policy in [
        StorageDurabilityPolicy::Standard,
        StorageDurabilityPolicy::Always,
    ] {
        let backend = StorageBackend::local_fs(temp_dir_for_api_test("durable-mode"));
        let outcome =
            StorageRuntime::open_with_backend(StorageOpenOptions::durable_local(policy), &backend)
                .expect("durable open should succeed");
        let summary = outcome.summary();
        let mut runtime = outcome.into_runtime();

        assert_eq!(summary.mode(), StorageMode::DurableLocal { policy });
        assert_eq!(summary.disposition(), StorageOpenDisposition::Created);
        assert_eq!(summary.recovery_health(), RecoveryHealthSummary::Healthy);
        assert!(summary.recovered_visible_version().is_some());
        assert!(summary.has_durable_recovery_facts());
        assert!(summary.backend_capabilities_used());
        assert_eq!(runtime.state(), StorageRuntimeState::Open);

        let close = runtime.close().expect("durable close");
        assert_eq!(close.state(), StorageRuntimeState::Closed);
        assert!(close.durable_synced());
    }
}

#[test]
#[cfg(feature = "localfs")]
fn open_durable_local_with_backend_returns_open_runtime() {
    let backend = StorageBackend::local_fs(temp_dir_for_api_test("durable-helper"));
    let outcome = StorageRuntime::open_durable_local_with_backend(
        StorageDurabilityPolicy::Standard,
        &backend,
    )
    .expect("durable helper open should succeed");
    let summary = outcome.summary();
    let runtime = outcome.into_runtime();

    assert_eq!(
        summary.mode(),
        StorageMode::DurableLocal {
            policy: StorageDurabilityPolicy::Standard
        }
    );
    assert!(summary.has_durable_recovery_facts());
    assert!(summary.backend_capabilities_used());
    assert_eq!(
        summary.maintenance_scheduling_policy(),
        StorageMaintenanceSchedulingPolicy::Background
    );
    assert_eq!(runtime.state(), StorageRuntimeState::Open);
    assert_eq!(
        runtime
            .maintenance_status()
            .expect("maintenance status")
            .background_worker_count(),
        1
    );
}

#[test]
#[cfg(feature = "localfs")]
fn open_durable_standard_returns_open_runtime() {
    let backend = StorageBackend::local_fs(temp_dir_for_api_test("durable-standard"));
    let outcome = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("standard durable open should succeed");
    let runtime = outcome.into_runtime();

    assert_eq!(runtime.state(), StorageRuntimeState::Open);
    assert!(runtime.is_open());
}

#[test]
#[cfg(feature = "localfs")]
fn open_durable_always_returns_open_runtime() {
    let backend = StorageBackend::local_fs(temp_dir_for_api_test("durable-always"));
    let outcome = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Always),
        &backend,
    )
    .expect("always durable open should succeed");
    let runtime = outcome.into_runtime();

    assert_eq!(runtime.state(), StorageRuntimeState::Open);
    assert!(runtime.is_open());
}

#[test]
#[cfg(feature = "localfs")]
fn create_durable_local_returns_created_disposition() {
    let backend = StorageBackend::local_fs(temp_dir_for_api_test("durable-created"));
    let outcome = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("durable create should succeed");

    assert_eq!(
        outcome.summary().disposition(),
        StorageOpenDisposition::Created
    );
}

#[test]
#[cfg(feature = "localfs")]
fn durable_open_reports_backend_capabilities_used() {
    let backend = StorageBackend::local_fs(temp_dir_for_api_test("durable-capabilities"));
    let outcome = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("durable open should succeed");

    assert!(outcome.summary().backend_capabilities_used());
}

#[test]
#[cfg(feature = "localfs")]
fn durable_open_reports_recovery_health() {
    let backend = StorageBackend::local_fs(temp_dir_for_api_test("durable-health"));
    let outcome = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("durable open should succeed");

    assert_eq!(
        outcome.summary().recovery_health(),
        RecoveryHealthSummary::Healthy
    );
}

#[test]
fn durable_open_degraded_health_survives_boundary_mapping() {
    let summary = StorageOpenSummary::with_open_facts(
        StorageMode::DurableLocal {
            policy: StorageDurabilityPolicy::Standard,
        },
        StorageOpenDisposition::OpenedExisting,
        RecoveryHealthSummary::Degraded,
        Some(CommitVersion::new(3)),
        true,
        StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
        true,
        true,
    );

    assert_eq!(summary.recovery_health(), RecoveryHealthSummary::Degraded);
    assert!(summary.has_durable_recovery_facts());
}

#[test]
fn borrowed_memory_durable_background_open_rejects_with_policy_error() {
    let backend = StorageBackend::memory();
    let error = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect_err("background durable borrowed memory backend cannot be promoted");

    match error {
        StorageApiError::InvalidArgument { field, reason } => {
            assert_eq!(field, "maintenance_scheduling_policy");
            assert!(reason.contains("background durable opens with borrowed backend handles"));
        }
        _ => panic!("expected invalid maintenance scheduling policy argument"),
    }
}

#[test]
fn durable_open_failure_returns_storage_api_error() {
    let backend = StorageBackend::memory();
    let error = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
            .with_maintenance_scheduling_policy(
                StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
            ),
        &backend,
    )
    .expect_err("memory backend cannot satisfy durable local mode");

    assert_eq!(error.class(), StorageApiErrorClass::Unsupported);
}

#[test]
#[cfg(feature = "localfs")]
fn close_open_durable_returns_final_facts() {
    let backend = StorageBackend::local_fs(temp_dir_for_api_test("durable-close"));
    let outcome = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Always),
        &backend,
    )
    .expect("durable open should succeed");
    let mut runtime = outcome.into_runtime();
    let close = runtime.close().expect("durable close");

    assert_eq!(close.state(), StorageRuntimeState::Closed);
    assert!(close.commits_quiesced());
    assert!(close.maintenance_drained());
    assert!(close.durable_synced());
    assert!(close.guards_released());
}

#[test]
#[cfg(feature = "localfs")]
fn open_existing_durable_local_returns_opened_disposition() {
    let root = temp_dir_for_api_test("durable-reopen");
    let first_backend = StorageBackend::local_fs(root.clone());
    let first = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &first_backend,
    )
    .expect("first durable open");
    let mut first_runtime = first.into_runtime();
    first_runtime.close().expect("first close");
    drop(first_runtime);
    drop(first_backend);

    let second_backend = StorageBackend::local_fs(root);
    let second = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &second_backend,
    )
    .expect("second durable open");

    assert_eq!(
        second.summary().disposition(),
        StorageOpenDisposition::OpenedExisting
    );
}

#[test]
fn durable_open_with_memory_backend_returns_storage_api_error() {
    let backend = StorageBackend::memory();
    let error = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
            .with_maintenance_scheduling_policy(
                StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
            ),
        &backend,
    )
    .expect_err("memory backend cannot satisfy durable local mode");

    assert_eq!(error.class(), StorageApiErrorClass::Unsupported);
    assert!(error.source().is_none());
}

#[test]
fn commit_batch_rejects_empty_and_duplicate_mutations() {
    let branch = branch_id(1);
    let empty = CommitBatch::new(branch, Vec::new(), CommitOptions::default())
        .expect_err("empty batch should fail");
    assert_eq!(empty.code(), "invalid_argument.storage_api.argument");

    let mutation = CommitMutation::Delete {
        storage_space: space(),
        key: key(b"k"),
    };
    let duplicate = CommitBatch::new(
        branch,
        vec![mutation.clone(), mutation],
        CommitOptions::default(),
    )
    .expect_err("duplicate batch should fail");
    assert_eq!(duplicate.code(), "invalid_argument.storage_api.argument");
}

#[test]
fn request_shells_are_constructible() {
    let branch = branch_id(2);
    let point = PointReadRequest::new(branch, space(), key(b"k"), ReadBound::Latest);
    assert_eq!(point.bound(), ReadBound::Latest);
    assert_eq!(point.branch_id(), branch);
    assert_eq!(point.storage_space(), &space());
    assert_eq!(point.key(), &key(b"k"));

    let scan = ScanReadRequest::new(
        branch,
        space(),
        ScanRange::new(None, None).expect("unbounded range"),
        ReadBound::AtVersion(CommitVersion::new(5)),
        Some(ReadLimit::new(10).expect("valid limit")),
    );
    assert_eq!(scan.branch_id(), branch);
    assert_eq!(scan.storage_space(), &space());
    assert_eq!(
        scan.range(),
        &ScanRange::new(None, None).expect("unbounded range")
    );
    assert_eq!(scan.bound(), ReadBound::AtVersion(CommitVersion::new(5)));
    assert_eq!(scan.limit(), Some(ReadLimit::new(10).expect("valid limit")));

    let branch_request = BranchRequest::new(
        branch,
        BranchAction::ForkAtTimestamp {
            source: branch_id(3),
            timestamp: Timestamp::from_micros(10),
        },
        Some(BranchGeneration::new(1)),
    );
    assert!(matches!(
        branch_request.action(),
        BranchAction::ForkAtTimestamp { .. }
    ));
    assert_eq!(branch_request.branch_id(), branch);
    assert_eq!(
        branch_request.expected_generation(),
        Some(BranchGeneration::new(1))
    );

    assert_eq!(
        MaintenanceRequest::new(MaintenanceTask::Checkpoint, MaintenanceScope::Global).task(),
        MaintenanceTask::Checkpoint
    );
    assert_eq!(
        MaintenanceRequest::new(MaintenanceTask::Checkpoint, MaintenanceScope::Global).scope(),
        MaintenanceScope::Global
    );
    assert_eq!(
        DiagnosticsRequest::new(DiagnosticsScope::Branch(branch)).scope(),
        DiagnosticsScope::Branch(branch)
    );
}

#[test]
fn outcome_summaries_expose_stored_fields() {
    let open = StorageOpenSummary::new(
        StorageOpenDisposition::OpenedExisting,
        RecoveryHealthSummary::Degraded,
        Some(CommitVersion::new(42)),
    );
    assert_eq!(open.disposition(), StorageOpenDisposition::OpenedExisting);
    assert_eq!(open.recovery_health(), RecoveryHealthSummary::Degraded);
    assert_eq!(
        open.recovered_visible_version(),
        Some(CommitVersion::new(42))
    );
    assert_eq!(
        open.maintenance_scheduling_policy(),
        StorageMaintenanceSchedulingPolicy::Background
    );

    let close = StorageCloseSummary::new(StorageRuntimeState::Closed, true);
    assert_eq!(close.state(), StorageRuntimeState::Closed);
    assert!(close.idempotent());

    let commit = CommitSummary::new(
        branch_id(6),
        CommitVersion::new(7),
        Timestamp::from_micros(8),
    );
    assert_eq!(commit.branch_id(), branch_id(6));
    assert_eq!(commit.commit_version(), CommitVersion::new(7));
    assert_eq!(commit.commit_timestamp(), Timestamp::from_micros(8));
}
