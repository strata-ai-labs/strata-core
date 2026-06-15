//! Source guards for the per-mode lifecycle policy.
//!
//! These tests are mechanical: they assert that cache mode denies every
//! source-table lifecycle capability while durable modes permit them, and that
//! the policy is a pure function of [`StorageMode`] — never of scale, workload,
//! or configuration. They do not depend on benchmark output.

use super::*;

/// Collect every capability of a policy as `(name, permitted)` pairs so the
/// guards exercise the entire canonical surface.
fn all_capabilities(policy: ModeLifecyclePolicy) -> [(&'static str, bool); 6] {
    [
        (
            "schedule_post_commit_source_maintenance",
            policy.may_schedule_post_commit_source_maintenance(),
        ),
        (
            "apply_source_shape_admission_pressure",
            policy.may_apply_source_shape_admission_pressure(),
        ),
        (
            "run_background_maintenance",
            policy.may_run_background_maintenance(),
        ),
        ("flush_to_table_source", policy.may_flush_to_table_source()),
        (
            "rewrite_or_compact_tables",
            policy.may_rewrite_or_compact_tables(),
        ),
        (
            "checkpoint_or_truncate_wal",
            policy.may_checkpoint_or_truncate_wal(),
        ),
    ]
}

#[test]
fn cache_policy_denies_all_source_table_lifecycle_capabilities() {
    let policy = ModeLifecyclePolicy::for_storage_mode(StorageMode::Cache);
    for (name, permitted) in all_capabilities(policy) {
        assert!(!permitted, "cache mode must deny capability: {name}");
    }
}

#[test]
fn durable_policies_permit_all_source_table_lifecycle_capabilities() {
    for mode in [
        StorageMode::DurableLocalStandard,
        StorageMode::DurableLocalAlways,
    ] {
        let policy = ModeLifecyclePolicy::for_storage_mode(mode);
        for (name, permitted) in all_capabilities(policy) {
            assert!(permitted, "{mode} must permit capability: {name}");
        }
    }
}

#[test]
fn lifecycle_policy_is_a_pure_function_of_mode() {
    for mode in [
        StorageMode::Cache,
        StorageMode::DurableLocalStandard,
        StorageMode::DurableLocalAlways,
        StorageMode::ObjectDurableCandidate,
    ] {
        assert_eq!(
            ModeLifecyclePolicy::for_storage_mode(mode),
            ModeLifecyclePolicy::for_storage_mode(mode),
            "policy must be a pure function of the storage mode: {mode}"
        );
    }
}

#[test]
fn cache_and_durable_policies_are_distinct() {
    assert_ne!(
        ModeLifecyclePolicy::for_storage_mode(StorageMode::Cache),
        ModeLifecyclePolicy::for_storage_mode(StorageMode::DurableLocalStandard),
        "cache and durable lifecycle policies must be distinguishable"
    );
}
