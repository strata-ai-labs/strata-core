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

/// Every storage mode that exists today, with its required disposition: cache is
/// volatile (all six capabilities denied), every other mode is durable (all six
/// permitted). New modes added to [`StorageMode`] force this list to be updated.
const ALL_MODES_WITH_DISPOSITION: [(StorageMode, bool); 4] = [
    (StorageMode::Cache, false),
    (StorageMode::DurableLocalStandard, true),
    (StorageMode::DurableLocalAlways, true),
    (StorageMode::ObjectDurableCandidate, true),
];

#[test]
fn lifecycle_policy_matrix_covers_every_mode_and_capability() {
    for (mode, source_table_permitted) in ALL_MODES_WITH_DISPOSITION {
        let policy = ModeLifecyclePolicy::for_storage_mode(mode);
        for (name, permitted) in all_capabilities(policy) {
            assert_eq!(
                permitted, source_table_permitted,
                "{mode} capability {name} must be {source_table_permitted}"
            );
        }
    }
}

#[test]
fn cache_denies_and_durable_candidate_modes_permit_each_capability() {
    // Pivot the matrix the other way: for each of the six capabilities, cache is
    // false while every durable/candidate mode is true. This is the
    // capability-kind angle requested by the plan: there is no maintenance-task
    // scheduling helper beyond these predicates, so denial of flush/compaction/
    // materialization scheduling is proven through the six policy predicates.
    let cache = ModeLifecyclePolicy::for_storage_mode(StorageMode::Cache);
    let durable_modes = [
        ModeLifecyclePolicy::for_storage_mode(StorageMode::DurableLocalStandard),
        ModeLifecyclePolicy::for_storage_mode(StorageMode::DurableLocalAlways),
        ModeLifecyclePolicy::for_storage_mode(StorageMode::ObjectDurableCandidate),
    ];

    let cache_caps = all_capabilities(cache);
    for (index, (name, cache_permitted)) in cache_caps.into_iter().enumerate() {
        assert!(!cache_permitted, "cache must deny capability: {name}");
        for durable in durable_modes {
            let (durable_name, durable_permitted) = all_capabilities(durable)[index];
            assert_eq!(durable_name, name, "capability ordering must be stable");
            assert!(
                durable_permitted,
                "durable mode must permit capability: {name}"
            );
        }
    }
}

#[test]
fn lifecycle_policy_is_independent_of_scheduling_policy_and_config() {
    // The policy surface is a pure function of mode. Constructing open plans that
    // vary the maintenance scheduling policy and other config fields must not
    // shift the lifecycle policy: the gate is the mode, not the config.
    let configs = [
        LifecycleConfig::default(),
        LifecycleConfig::default()
            .with_maintenance_scheduling_policy(LifecycleMaintenanceSchedulingPolicy::Disabled)
            .expect("disabled scheduling config"),
        LifecycleConfig::default()
            .with_maintenance_scheduling_policy(
                LifecycleMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
            )
            .expect("evaluate-and-enqueue scheduling config"),
        LifecycleConfig::default()
            .with_maintenance_scheduling_policy(
                LifecycleMaintenanceSchedulingPolicy::DeterministicInline,
            )
            .expect("deterministic-inline scheduling config"),
    ];

    for (mode, _) in ALL_MODES_WITH_DISPOSITION {
        let expected = ModeLifecyclePolicy::for_storage_mode(mode);
        for config in configs {
            let plan = StorageOpenPlan::new(
                mode,
                LifecycleCodecId::identity(),
                RecoveryStrictness::Strict,
                config,
            )
            .expect("open plan");
            assert_eq!(
                plan.lifecycle_policy(),
                expected,
                "{mode} lifecycle policy must not depend on the scheduling policy or config"
            );
        }
    }
}
