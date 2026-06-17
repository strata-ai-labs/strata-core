use super::*;

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
fn open_options_reject_invalid_test_wal_segment_size() {
    let error = StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
        .with_wal_segment_size_for_test(1)
        .validate()
        .expect_err("invalid test WAL segment size is rejected");

    match error {
        StorageApiError::InvalidArgument { field, reason } => {
            assert_eq!(field, "wal_segment_size");
            assert_eq!(reason, "test WAL segment size is invalid");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn open_options_preserve_background_maintenance_knobs() {
    let background = StorageBackgroundMaintenanceOptions::product_default()
        .with_worker_count(2)
        .with_scheduler_queue_depth(3)
        .with_max_tasks_per_wake(4)
        .with_max_runtime_per_wake(std::time::Duration::from_millis(5));
    let options = StorageOpenOptions::cache()
        .with_background_maintenance(background)
        .with_background_worker_count(6)
        .with_background_scheduler_queue_depth(7)
        .with_background_max_tasks_per_wake(8)
        .with_background_max_runtime_per_wake(std::time::Duration::from_millis(9));

    assert_eq!(background.worker_count(), 2);
    assert_eq!(background.scheduler_queue_depth(), 3);
    assert_eq!(background.max_tasks_per_wake(), 4);
    assert_eq!(
        background.max_runtime_per_wake(),
        std::time::Duration::from_millis(5)
    );
    assert_eq!(options.background_maintenance().worker_count(), 6);
    assert_eq!(options.background_maintenance().scheduler_queue_depth(), 7);
    assert_eq!(options.background_maintenance().max_tasks_per_wake(), 8);
    assert_eq!(
        options.background_maintenance().max_runtime_per_wake(),
        std::time::Duration::from_millis(9)
    );
    assert!(options.validate().is_ok());
}

#[test]
fn open_options_reject_zero_background_maintenance_knobs() {
    for (options, expected_field) in [
        (
            StorageOpenOptions::cache().with_background_worker_count(0),
            "background_worker_count",
        ),
        (
            StorageOpenOptions::cache().with_background_scheduler_queue_depth(0),
            "background_scheduler_queue_depth",
        ),
        (
            StorageOpenOptions::cache().with_background_max_tasks_per_wake(0),
            "background_max_tasks_per_wake",
        ),
        (
            StorageOpenOptions::cache()
                .with_background_max_runtime_per_wake(std::time::Duration::ZERO),
            "background_max_runtime_per_wake",
        ),
    ] {
        let error = options
            .validate()
            .expect_err("zero background maintenance knob is rejected");
        match error {
            StorageApiError::InvalidArgument { field, .. } => assert_eq!(field, expected_field),
            other => panic!("expected invalid argument for {expected_field}, got {other:?}"),
        }
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
