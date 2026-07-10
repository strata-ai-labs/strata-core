//! Lifecycle scaffold coverage counters.

/// Coverage counters returned by the lifecycle scaffold contract.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[expect(
    clippy::struct_field_names,
    reason = "counter suffix keeps the public testkit getter vocabulary explicit"
)]
pub struct LifecycleScaffoldOutcome {
    pub(super) valid_config_cases: usize,
    pub(super) invalid_config_cases: usize,
    pub(super) lifecycle_state_cases: usize,
    pub(super) storage_mode_cases: usize,
    pub(super) valid_transition_cases: usize,
    pub(super) invalid_transition_cases: usize,
    pub(super) operation_admission_accept_cases: usize,
    pub(super) operation_admission_reject_cases: usize,
    pub(super) close_retry_cases: usize,
    pub(super) closed_idempotence_cases: usize,
    pub(super) failed_state_sticky_cases: usize,
    pub(super) input_derived_state_cases: usize,
    pub(super) open_plan_cases: usize,
    pub(super) open_outcome_cases: usize,
    pub(super) recovery_health_cases: usize,
    pub(super) maintenance_task_cases: usize,
    pub(super) reclaim_fact_cases: usize,
    pub(super) error_display_cases: usize,
    pub(super) error_source_cases: usize,
    pub(super) stats_cases: usize,
    pub(super) source_guard_fixture_cases: usize,
    pub(super) accepted_capability_cases: usize,
    pub(super) rejected_capability_cases: usize,
    pub(super) cache_capability_cases: usize,
    pub(super) durable_standard_capability_cases: usize,
    pub(super) durable_always_capability_cases: usize,
    pub(super) object_candidate_capability_cases: usize,
    pub(super) missing_capability_cases: usize,
    pub(super) object_candidate_conditional_publish_cases: usize,
    pub(super) object_candidate_create_update_cases: usize,
    pub(super) capability_preflight_cases: usize,
    pub(super) input_derived_capability_cases: usize,
    pub(super) cache_open_accepted_cases: usize,
    pub(super) cache_open_rejected_cases: usize,
    pub(super) cache_baseline_cases: usize,
    pub(super) cache_durable_absence_cases: usize,
    pub(super) cache_commit_read_cases: usize,
    pub(super) cache_close_cases: usize,
    pub(super) cache_close_idempotence_cases: usize,
    pub(super) cache_commit_after_close_rejected_cases: usize,
    pub(super) cache_reopen_empty_cases: usize,
    pub(super) input_derived_cache_cases: usize,
    pub(super) durable_assembly_standard_cases: usize,
    pub(super) durable_assembly_always_cases: usize,
    pub(super) durable_assembly_rejected_cases: usize,
    pub(super) durable_manifest_create_cases: usize,
    pub(super) durable_manifest_existing_cases: usize,
    pub(super) durable_writer_lock_failure_cases: usize,
    pub(super) durable_manifest_identity_mismatch_cases: usize,
    pub(super) durable_manifest_create_race_cases: usize,
    pub(super) durable_manifest_publish_fault_cases: usize,
    pub(super) durable_wal_open_failure_cases: usize,
    pub(super) durable_recovering_admission_cases: usize,
    pub(super) durable_no_recovery_side_effect_cases: usize,
    pub(super) input_derived_durable_cases: usize,
}

impl LifecycleScaffoldOutcome {
    /// Number of valid config cases exercised.
    pub const fn valid_config_cases(&self) -> usize {
        self.valid_config_cases
    }

    /// Number of invalid config cases exercised.
    pub const fn invalid_config_cases(&self) -> usize {
        self.invalid_config_cases
    }

    /// Number of lifecycle state cases exercised.
    pub const fn lifecycle_state_cases(&self) -> usize {
        self.lifecycle_state_cases
    }

    /// Number of storage mode cases exercised.
    pub const fn storage_mode_cases(&self) -> usize {
        self.storage_mode_cases
    }

    /// Number of valid lifecycle transition cases exercised.
    pub const fn valid_transition_cases(&self) -> usize {
        self.valid_transition_cases
    }

    /// Number of invalid lifecycle transition cases exercised.
    pub const fn invalid_transition_cases(&self) -> usize {
        self.invalid_transition_cases
    }

    /// Number of accepted operation-admission cases exercised.
    pub const fn operation_admission_accept_cases(&self) -> usize {
        self.operation_admission_accept_cases
    }

    /// Number of rejected operation-admission cases exercised.
    pub const fn operation_admission_reject_cases(&self) -> usize {
        self.operation_admission_reject_cases
    }

    /// Number of close retry cases exercised.
    pub const fn close_retry_cases(&self) -> usize {
        self.close_retry_cases
    }

    /// Number of closed-state idempotence cases exercised.
    pub const fn closed_idempotence_cases(&self) -> usize {
        self.closed_idempotence_cases
    }

    /// Number of failed-state stickiness cases exercised.
    pub const fn failed_state_sticky_cases(&self) -> usize {
        self.failed_state_sticky_cases
    }

    /// Number of input-derived state machine routes exercised.
    pub const fn input_derived_state_cases(&self) -> usize {
        self.input_derived_state_cases
    }

    /// Number of open plan cases exercised.
    pub const fn open_plan_cases(&self) -> usize {
        self.open_plan_cases
    }

    /// Number of open outcome cases exercised.
    pub const fn open_outcome_cases(&self) -> usize {
        self.open_outcome_cases
    }

    /// Number of recovery health cases exercised.
    pub const fn recovery_health_cases(&self) -> usize {
        self.recovery_health_cases
    }

    /// Number of maintenance task cases exercised.
    pub const fn maintenance_task_cases(&self) -> usize {
        self.maintenance_task_cases
    }

    /// Number of retention, quarantine, and close fact cases exercised.
    pub const fn reclaim_fact_cases(&self) -> usize {
        self.reclaim_fact_cases
    }

    /// Number of error display cases exercised.
    pub const fn error_display_cases(&self) -> usize {
        self.error_display_cases
    }

    /// Number of error source-chain cases exercised.
    pub const fn error_source_cases(&self) -> usize {
        self.error_source_cases
    }

    /// Number of stats cases exercised.
    pub const fn stats_cases(&self) -> usize {
        self.stats_cases
    }

    /// Number of source-guard fixture cases exercised.
    pub const fn source_guard_fixture_cases(&self) -> usize {
        self.source_guard_fixture_cases
    }

    /// Number of accepted lifecycle capability validation cases exercised.
    pub const fn accepted_capability_cases(&self) -> usize {
        self.accepted_capability_cases
    }

    /// Number of rejected lifecycle capability validation cases exercised.
    pub const fn rejected_capability_cases(&self) -> usize {
        self.rejected_capability_cases
    }

    /// Number of cache-mode lifecycle capability cases exercised.
    pub const fn cache_capability_cases(&self) -> usize {
        self.cache_capability_cases
    }

    /// Number of durable-local standard capability cases exercised.
    pub const fn durable_standard_capability_cases(&self) -> usize {
        self.durable_standard_capability_cases
    }

    /// Number of durable-local always capability cases exercised.
    pub const fn durable_always_capability_cases(&self) -> usize {
        self.durable_always_capability_cases
    }

    /// Number of object-durable candidate capability cases exercised.
    pub const fn object_candidate_capability_cases(&self) -> usize {
        self.object_candidate_capability_cases
    }

    /// Number of missing-capability reporting cases exercised.
    pub const fn missing_capability_cases(&self) -> usize {
        self.missing_capability_cases
    }

    /// Number of object-candidate conditional-publish fence cases exercised.
    pub const fn object_candidate_conditional_publish_cases(&self) -> usize {
        self.object_candidate_conditional_publish_cases
    }

    /// Number of object-candidate create/update fence cases exercised.
    pub const fn object_candidate_create_update_cases(&self) -> usize {
        self.object_candidate_create_update_cases
    }

    /// Number of backend preflight cases that read only capabilities.
    pub const fn capability_preflight_cases(&self) -> usize {
        self.capability_preflight_cases
    }

    /// Number of input-derived capability validation cases exercised.
    pub const fn input_derived_capability_cases(&self) -> usize {
        self.input_derived_capability_cases
    }

    /// Number of accepted cache lifecycle open cases exercised.
    pub const fn cache_open_accepted_cases(&self) -> usize {
        self.cache_open_accepted_cases
    }

    /// Number of rejected cache lifecycle open cases exercised.
    pub const fn cache_open_rejected_cases(&self) -> usize {
        self.cache_open_rejected_cases
    }

    /// Number of cache runtime baseline fact cases exercised.
    pub const fn cache_baseline_cases(&self) -> usize {
        self.cache_baseline_cases
    }

    /// Number of cache durable-service absence cases exercised.
    pub const fn cache_durable_absence_cases(&self) -> usize {
        self.cache_durable_absence_cases
    }

    /// Number of cache commit/read smoke cases exercised.
    pub const fn cache_commit_read_cases(&self) -> usize {
        self.cache_commit_read_cases
    }

    /// Number of cache close cases exercised.
    pub const fn cache_close_cases(&self) -> usize {
        self.cache_close_cases
    }

    /// Number of cache close idempotence cases exercised.
    pub const fn cache_close_idempotence_cases(&self) -> usize {
        self.cache_close_idempotence_cases
    }

    /// Number of cache commit-after-close rejection cases exercised.
    pub const fn cache_commit_after_close_rejected_cases(&self) -> usize {
        self.cache_commit_after_close_rejected_cases
    }

    /// Number of cache reopen-empty cases exercised.
    pub const fn cache_reopen_empty_cases(&self) -> usize {
        self.cache_reopen_empty_cases
    }

    /// Number of input-derived cache operation cases exercised.
    pub const fn input_derived_cache_cases(&self) -> usize {
        self.input_derived_cache_cases
    }

    /// Number of durable-standard service assembly cases exercised.
    pub const fn durable_assembly_standard_cases(&self) -> usize {
        self.durable_assembly_standard_cases
    }

    /// Number of durable-always service assembly cases exercised.
    pub const fn durable_assembly_always_cases(&self) -> usize {
        self.durable_assembly_always_cases
    }

    /// Number of rejected durable assembly cases exercised.
    pub const fn durable_assembly_rejected_cases(&self) -> usize {
        self.durable_assembly_rejected_cases
    }

    /// Number of durable manifest create cases exercised.
    pub const fn durable_manifest_create_cases(&self) -> usize {
        self.durable_manifest_create_cases
    }

    /// Number of durable existing-manifest cases exercised.
    pub const fn durable_manifest_existing_cases(&self) -> usize {
        self.durable_manifest_existing_cases
    }

    /// Number of durable writer-lock failure cases exercised.
    pub const fn durable_writer_lock_failure_cases(&self) -> usize {
        self.durable_writer_lock_failure_cases
    }

    /// Number of durable manifest identity mismatch cases exercised.
    pub const fn durable_manifest_identity_mismatch_cases(&self) -> usize {
        self.durable_manifest_identity_mismatch_cases
    }

    /// Number of durable manifest create-race cases exercised.
    pub const fn durable_manifest_create_race_cases(&self) -> usize {
        self.durable_manifest_create_race_cases
    }

    /// Number of durable manifest publish-fault cases exercised.
    pub const fn durable_manifest_publish_fault_cases(&self) -> usize {
        self.durable_manifest_publish_fault_cases
    }

    /// Number of durable WAL open-failure cases exercised.
    pub const fn durable_wal_open_failure_cases(&self) -> usize {
        self.durable_wal_open_failure_cases
    }

    /// Number of durable recovering-state admission cases exercised.
    pub const fn durable_recovering_admission_cases(&self) -> usize {
        self.durable_recovering_admission_cases
    }

    /// Number of durable no-recovery-side-effect cases exercised.
    pub const fn durable_no_recovery_side_effect_cases(&self) -> usize {
        self.durable_no_recovery_side_effect_cases
    }

    /// Number of input-derived durable assembly cases exercised.
    pub const fn input_derived_durable_cases(&self) -> usize {
        self.input_derived_durable_cases
    }
}
