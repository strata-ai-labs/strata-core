//! Generated commit-runtime scaffold contract helpers.

use crate::commit::{
    CommitBatch, CommitBatchOptions, CommitCasFact, CommitConflictValidationMode,
    CommitDuplicateKeyPolicy, CommitDurabilityClass, CommitDurabilityMode, CommitExpiry,
    CommitLowerLayer, CommitMutation, CommitObservedVersion, CommitOrigin, CommitPhase,
    CommitReadFact, CommitReadOnlyDiagnostics, CommitRetentionHint, CommitRuntimeConfig,
    CommitRuntimeError, CommitRuntimeStats, CommitStamp, CommitTimestampPolicy,
    CommitValidationFacts, CommitVisibilityFacts,
};
use crate::row::{PhysicalKey, StorageSpaceId};
use std::error::Error;
use std::fmt;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

use super::commit_runtime_allocator::check_commit_runtime_allocator_contract;
use super::commit_runtime_branch_guards::check_commit_runtime_branch_guard_contract;
use super::commit_runtime_cache::check_commit_runtime_cache_contract;
use super::commit_runtime_conflicts::check_commit_runtime_conflict_contract;
use super::commit_runtime_durable::check_commit_runtime_durable_contract;
use super::commit_runtime_outcome::check_commit_runtime_outcome_contract;
use super::commit_runtime_timeline::check_commit_runtime_timeline_contract;
use super::TestkitError;

/// Summary of one generated commit-runtime scaffold contract check.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CommitRuntimeScaffoldOutcome {
    valid_config: usize,
    invalid_config: usize,
    phase_facts: usize,
    visibility_facts: usize,
    invalid_visibility_facts: usize,
    error_displays: usize,
    error_sources: usize,
    stats: usize,
    source_guard_fixtures: usize,
    valid_batches: usize,
    invalid_batches: usize,
    duplicate_mutations: usize,
    branch_mismatches: usize,
    storage_owned_spaces: usize,
    invalid_fact_cases: usize,
    stamping_cases: usize,
    expiry_rejections: usize,
    stamping_rejections: usize,
    version_allocations: usize,
    version_catch_ups: usize,
    version_overflows: usize,
    generated_timestamps: usize,
    clamped_timestamps: usize,
    explicit_timestamps: usize,
    invalid_explicit_timestamps: usize,
    timestamp_source_failures: usize,
    read_only_no_allocations: usize,
    no_transaction_id_checks: usize,
    read_only_outcomes: usize,
    read_only_disabled_rejections: usize,
    visible_tracker_initializations: usize,
    visible_tracker_monotonic_publishes: usize,
    visible_tracker_regression_rejections: usize,
    outcome_invalid_visibility_facts: usize,
    outcome_constructor_rejections: usize,
    mutation_count_facts: usize,
    cross_branch_read_only_facts: usize,
    read_only_outcome_no_allocations: usize,
    branch_registration_successes: usize,
    duplicate_registration_rejections: usize,
    missing_branch_rejections: usize,
    deleting_branch_rejections: usize,
    generation_exact_matches: usize,
    generation_mismatches: usize,
    generation_not_supplied: usize,
    stale_generation_after_recreate: usize,
    same_branch_guard_contentions: usize,
    different_branch_simultaneous_guards: usize,
    quiesce_start_successes: usize,
    quiesce_rejected_with_active_guards: usize,
    mutating_guard_rejected_during_quiesce: usize,
    read_only_allowed_during_quiesce: usize,
    guard_release_and_reacquire: usize,
    read_present_matches: usize,
    read_present_mismatches: usize,
    read_present_becomes_missing: usize,
    read_missing_matches: usize,
    read_missing_becomes_present: usize,
    cas_present_matches: usize,
    cas_present_mismatches: usize,
    cas_present_becomes_missing: usize,
    cas_missing_matches: usize,
    cas_missing_becomes_present: usize,
    combined_read_before_cas: usize,
    blind_put_no_conflicts: usize,
    blind_delete_no_conflicts: usize,
    skip_mode_no_reads: usize,
    lower_layer_read_failures: usize,
    conflict_error_vocabulary: usize,
    valid_timeline_entries: usize,
    zero_timeline_version_rejections: usize,
    timestamp_index_keys: usize,
    version_index_keys: usize,
    timeline_row_pairs: usize,
    timeline_shared_commit_facts: usize,
    timestamp_index_decodes: usize,
    version_index_decodes: usize,
    malformed_timeline_prefix_rejections: usize,
    malformed_timeline_key_length_rejections: usize,
    timeline_value_length_rejections: usize,
    timeline_key_value_mismatch_rejections: usize,
    timestamp_lookup_exact_matches: usize,
    timestamp_lookup_between_matches: usize,
    duplicate_timestamp_tiebreaks: usize,
    version_timestamp_lookups: usize,
    timeline_branch_isolations: usize,
    timeline_row_order_independence: usize,
    timeline_bounds_reports: usize,
    timeline_caller_rejections: usize,
    cache_put_commits: usize,
    cache_delete_commits: usize,
    cache_mixed_commits: usize,
    cache_one_version_per_batch: usize,
    cache_one_timestamp_per_batch: usize,
    cache_timeline_rows_installed: usize,
    cache_visible_publications: usize,
    cache_not_durable_outcomes: usize,
    cache_branch_admission_rejections: usize,
    cache_conflict_rejections: usize,
    cache_non_cache_rejections: usize,
    cache_apply_failure_atomicity: usize,
    cache_version_gap_after_failure: usize,
    cache_applied_above_visible_rejections: usize,
    cache_visible_allocator_mismatch_rejections: usize,
    cache_guard_release_after_failure: usize,
    durable_standard_commits: usize,
    durable_always_commits: usize,
    durable_wal_payload_parity: usize,
    durable_clean_wal_failures: usize,
    durable_uncertain_wal_failures: usize,
    durable_cache_mode_rejections: usize,
    durable_policy_mismatches: usize,
    durable_unforced_always_rejections: usize,
    durable_guard_release_after_failure: usize,
    durable_read_only_rejections: usize,
    durable_unresolved_fact_validations: usize,
    durable_unresolved_fact_rejections: usize,
    durable_unresolved_gate_records: usize,
    durable_unresolved_gate_idempotent_records: usize,
    durable_unresolved_gate_different_fact_rejections: usize,
    durable_unresolved_gate_exact_clears: usize,
    durable_not_applied_gates: usize,
    durable_applied_not_visible_gates: usize,
    durable_unresolved_gate_blocks: usize,
    durable_unresolved_gate_cache_blocks: usize,
    durable_unresolved_gate_read_only_diagnostics: usize,
    durable_clean_wal_no_gate: usize,
    durable_uncertain_wal_no_gate: usize,
}

impl CommitRuntimeScaffoldOutcome {
    /// Number of valid configuration cases exercised.
    pub const fn valid_config_cases(self) -> usize {
        self.valid_config
    }

    /// Number of invalid configuration cases exercised.
    pub const fn invalid_config_cases(self) -> usize {
        self.invalid_config
    }

    /// Number of phase and durability fact cases exercised.
    pub const fn phase_fact_cases(self) -> usize {
        self.phase_facts
    }

    /// Number of valid visibility fact cases exercised.
    pub const fn visibility_fact_cases(self) -> usize {
        self.visibility_facts
    }

    /// Number of invalid visibility fact cases exercised.
    pub const fn invalid_visibility_fact_cases(self) -> usize {
        self.invalid_visibility_facts
    }

    /// Number of error display cases exercised.
    pub const fn error_display_cases(self) -> usize {
        self.error_displays
    }

    /// Number of error source-chain cases exercised.
    pub const fn error_source_cases(self) -> usize {
        self.error_sources
    }

    /// Number of stats construction cases exercised.
    pub const fn stats_cases(self) -> usize {
        self.stats
    }

    /// Number of source-guard fixture cases exercised.
    pub const fn source_guard_fixture_cases(self) -> usize {
        self.source_guard_fixtures
    }

    /// Number of valid commit batch cases exercised.
    pub const fn valid_batch_cases(self) -> usize {
        self.valid_batches
    }

    /// Number of invalid commit batch cases exercised.
    pub const fn invalid_batch_cases(self) -> usize {
        self.invalid_batches
    }

    /// Number of duplicate mutation cases exercised.
    pub const fn duplicate_mutation_cases(self) -> usize {
        self.duplicate_mutations
    }

    /// Number of branch mismatch cases exercised.
    pub const fn branch_mismatch_cases(self) -> usize {
        self.branch_mismatches
    }

    /// Number of storage-owned caller input cases exercised.
    pub const fn storage_owned_space_cases(self) -> usize {
        self.storage_owned_spaces
    }

    /// Number of invalid validation fact cases exercised.
    pub const fn invalid_fact_cases(self) -> usize {
        self.invalid_fact_cases
    }

    /// Number of successful row stamping cases exercised.
    pub const fn stamping_cases(self) -> usize {
        self.stamping_cases
    }

    /// Number of expiry rejection cases exercised.
    pub const fn expiry_rejection_cases(self) -> usize {
        self.expiry_rejections
    }

    /// Number of stamping rejection cases exercised.
    pub const fn stamping_rejection_cases(self) -> usize {
        self.stamping_rejections
    }

    /// Number of version allocation cases exercised.
    pub const fn version_allocation_cases(self) -> usize {
        self.version_allocations
    }

    /// Number of version catch-up cases exercised.
    pub const fn version_catch_up_cases(self) -> usize {
        self.version_catch_ups
    }

    /// Number of version overflow cases exercised.
    pub const fn version_overflow_cases(self) -> usize {
        self.version_overflows
    }

    /// Number of runtime-generated timestamp cases exercised.
    pub const fn generated_timestamp_cases(self) -> usize {
        self.generated_timestamps
    }

    /// Number of clamped timestamp cases exercised.
    pub const fn clamped_timestamp_cases(self) -> usize {
        self.clamped_timestamps
    }

    /// Number of explicit timestamp cases exercised.
    pub const fn explicit_timestamp_cases(self) -> usize {
        self.explicit_timestamps
    }

    /// Number of invalid explicit timestamp cases exercised.
    pub const fn invalid_explicit_timestamp_cases(self) -> usize {
        self.invalid_explicit_timestamps
    }

    /// Number of timestamp source failure cases exercised.
    pub const fn timestamp_source_failure_cases(self) -> usize {
        self.timestamp_source_failures
    }

    /// Number of read-only no-allocation cases exercised.
    pub const fn read_only_no_allocation_cases(self) -> usize {
        self.read_only_no_allocations
    }

    /// Number of no transaction-id surface checks exercised.
    pub const fn no_transaction_id_check_cases(self) -> usize {
        self.no_transaction_id_checks
    }

    /// Number of read-only outcome success cases exercised.
    pub const fn read_only_outcome_cases(self) -> usize {
        self.read_only_outcomes
    }

    /// Number of disabled read-only rejection cases exercised.
    pub const fn read_only_disabled_rejection_cases(self) -> usize {
        self.read_only_disabled_rejections
    }

    /// Number of visible-version tracker initialization cases exercised.
    pub const fn visible_tracker_initialization_cases(self) -> usize {
        self.visible_tracker_initializations
    }

    /// Number of visible-version monotonic publish cases exercised.
    pub const fn visible_tracker_monotonic_publish_cases(self) -> usize {
        self.visible_tracker_monotonic_publishes
    }

    /// Number of visible-version regression rejection cases exercised.
    pub const fn visible_tracker_regression_rejection_cases(self) -> usize {
        self.visible_tracker_regression_rejections
    }

    /// Number of outcome invalid visibility fact cases exercised.
    pub const fn outcome_invalid_visibility_fact_cases(self) -> usize {
        self.outcome_invalid_visibility_facts
    }

    /// Number of outcome constructor rejection cases exercised.
    pub const fn outcome_constructor_rejection_cases(self) -> usize {
        self.outcome_constructor_rejections
    }

    /// Number of mutation count fact cases exercised.
    pub const fn mutation_count_fact_cases(self) -> usize {
        self.mutation_count_facts
    }

    /// Number of cross-branch read-only fact cases exercised.
    pub const fn cross_branch_read_only_fact_cases(self) -> usize {
        self.cross_branch_read_only_facts
    }

    /// Number of read-only outcome no-allocation cases exercised.
    pub const fn read_only_outcome_no_allocation_cases(self) -> usize {
        self.read_only_outcome_no_allocations
    }

    /// Number of branch registration success cases exercised.
    pub const fn branch_registration_success_cases(self) -> usize {
        self.branch_registration_successes
    }

    /// Number of duplicate branch registration rejection cases exercised.
    pub const fn duplicate_registration_rejection_cases(self) -> usize {
        self.duplicate_registration_rejections
    }

    /// Number of missing branch rejection cases exercised.
    pub const fn missing_branch_rejection_cases(self) -> usize {
        self.missing_branch_rejections
    }

    /// Number of deleting/deleted branch rejection cases exercised.
    pub const fn deleting_branch_rejection_cases(self) -> usize {
        self.deleting_branch_rejections
    }

    /// Number of exact generation match cases exercised.
    pub const fn generation_exact_match_cases(self) -> usize {
        self.generation_exact_matches
    }

    /// Number of generation mismatch cases exercised.
    pub const fn generation_mismatch_cases(self) -> usize {
        self.generation_mismatches
    }

    /// Number of not-supplied generation cases exercised.
    pub const fn generation_not_supplied_cases(self) -> usize {
        self.generation_not_supplied
    }

    /// Number of stale generation after recreate cases exercised.
    pub const fn stale_generation_after_recreate_cases(self) -> usize {
        self.stale_generation_after_recreate
    }

    /// Number of same-branch guard contention cases exercised.
    pub const fn same_branch_guard_contention_cases(self) -> usize {
        self.same_branch_guard_contentions
    }

    /// Number of simultaneous different-branch guard cases exercised.
    pub const fn different_branch_simultaneous_guard_cases(self) -> usize {
        self.different_branch_simultaneous_guards
    }

    /// Number of successful quiesce start cases exercised.
    pub const fn quiesce_start_success_cases(self) -> usize {
        self.quiesce_start_successes
    }

    /// Number of quiesce-with-active-guard rejection cases exercised.
    pub const fn quiesce_rejected_with_active_guard_cases(self) -> usize {
        self.quiesce_rejected_with_active_guards
    }

    /// Number of mutating guard rejected during quiesce cases exercised.
    pub const fn mutating_guard_rejected_during_quiesce_cases(self) -> usize {
        self.mutating_guard_rejected_during_quiesce
    }

    /// Number of read-only during quiesce cases exercised.
    pub const fn read_only_allowed_during_quiesce_cases(self) -> usize {
        self.read_only_allowed_during_quiesce
    }

    /// Number of guard release and reacquire cases exercised.
    pub const fn guard_release_and_reacquire_cases(self) -> usize {
        self.guard_release_and_reacquire
    }

    /// Number of read-set present match cases exercised.
    pub const fn read_present_match_cases(self) -> usize {
        self.read_present_matches
    }

    /// Number of read-set present mismatch cases exercised.
    pub const fn read_present_mismatch_cases(self) -> usize {
        self.read_present_mismatches
    }

    /// Number of read-set present-to-missing cases exercised.
    pub const fn read_present_becomes_missing_cases(self) -> usize {
        self.read_present_becomes_missing
    }

    /// Number of read-set missing match cases exercised.
    pub const fn read_missing_match_cases(self) -> usize {
        self.read_missing_matches
    }

    /// Number of read-set missing-to-present cases exercised.
    pub const fn read_missing_becomes_present_cases(self) -> usize {
        self.read_missing_becomes_present
    }

    /// Number of CAS present match cases exercised.
    pub const fn cas_present_match_cases(self) -> usize {
        self.cas_present_matches
    }

    /// Number of CAS present mismatch cases exercised.
    pub const fn cas_present_mismatch_cases(self) -> usize {
        self.cas_present_mismatches
    }

    /// Number of CAS present-to-missing cases exercised.
    pub const fn cas_present_becomes_missing_cases(self) -> usize {
        self.cas_present_becomes_missing
    }

    /// Number of CAS missing match cases exercised.
    pub const fn cas_missing_match_cases(self) -> usize {
        self.cas_missing_matches
    }

    /// Number of CAS missing-to-present cases exercised.
    pub const fn cas_missing_becomes_present_cases(self) -> usize {
        self.cas_missing_becomes_present
    }

    /// Number of combined read-before-CAS ordering cases exercised.
    pub const fn combined_read_before_cas_cases(self) -> usize {
        self.combined_read_before_cas
    }

    /// Number of blind put no-conflict cases exercised.
    pub const fn blind_put_no_conflict_cases(self) -> usize {
        self.blind_put_no_conflicts
    }

    /// Number of blind delete no-conflict cases exercised.
    pub const fn blind_delete_no_conflict_cases(self) -> usize {
        self.blind_delete_no_conflicts
    }

    /// Number of skip-mode no-read cases exercised.
    pub const fn skip_mode_no_read_cases(self) -> usize {
        self.skip_mode_no_reads
    }

    /// Number of lower-layer read failure cases exercised.
    pub const fn lower_layer_read_failure_cases(self) -> usize {
        self.lower_layer_read_failures
    }

    /// Number of conflict error vocabulary cases exercised.
    pub const fn conflict_error_vocabulary_cases(self) -> usize {
        self.conflict_error_vocabulary
    }

    /// Number of valid timeline entry cases exercised.
    pub const fn valid_timeline_entry_cases(self) -> usize {
        self.valid_timeline_entries
    }

    /// Number of zero-version timeline entry rejection cases exercised.
    pub const fn zero_timeline_version_rejection_cases(self) -> usize {
        self.zero_timeline_version_rejections
    }

    /// Number of timestamp-index key cases exercised.
    pub const fn timestamp_index_key_cases(self) -> usize {
        self.timestamp_index_keys
    }

    /// Number of version-index key cases exercised.
    pub const fn version_index_key_cases(self) -> usize {
        self.version_index_keys
    }

    /// Number of two-row timeline construction cases exercised.
    pub const fn timeline_row_pair_cases(self) -> usize {
        self.timeline_row_pairs
    }

    /// Number of timeline shared commit-fact cases exercised.
    pub const fn timeline_shared_commit_fact_cases(self) -> usize {
        self.timeline_shared_commit_facts
    }

    /// Number of timestamp-index decode cases exercised.
    pub const fn timestamp_index_decode_cases(self) -> usize {
        self.timestamp_index_decodes
    }

    /// Number of version-index decode cases exercised.
    pub const fn version_index_decode_cases(self) -> usize {
        self.version_index_decodes
    }

    /// Number of malformed timeline prefix rejection cases exercised.
    pub const fn malformed_timeline_prefix_rejection_cases(self) -> usize {
        self.malformed_timeline_prefix_rejections
    }

    /// Number of malformed timeline key-length rejection cases exercised.
    pub const fn malformed_timeline_key_length_rejection_cases(self) -> usize {
        self.malformed_timeline_key_length_rejections
    }

    /// Number of timeline value-length rejection cases exercised.
    pub const fn timeline_value_length_rejection_cases(self) -> usize {
        self.timeline_value_length_rejections
    }

    /// Number of timeline key/value mismatch rejection cases exercised.
    pub const fn timeline_key_value_mismatch_rejection_cases(self) -> usize {
        self.timeline_key_value_mismatch_rejections
    }

    /// Number of exact timestamp lookup cases exercised.
    pub const fn timestamp_lookup_exact_match_cases(self) -> usize {
        self.timestamp_lookup_exact_matches
    }

    /// Number of between-timestamp lookup cases exercised.
    pub const fn timestamp_lookup_between_match_cases(self) -> usize {
        self.timestamp_lookup_between_matches
    }

    /// Number of duplicate timestamp tiebreak cases exercised.
    pub const fn duplicate_timestamp_tiebreak_cases(self) -> usize {
        self.duplicate_timestamp_tiebreaks
    }

    /// Number of version-to-timestamp lookup cases exercised.
    pub const fn version_timestamp_lookup_cases(self) -> usize {
        self.version_timestamp_lookups
    }

    /// Number of timeline branch isolation cases exercised.
    pub const fn timeline_branch_isolation_cases(self) -> usize {
        self.timeline_branch_isolations
    }

    /// Number of timeline row-order independence cases exercised.
    pub const fn timeline_row_order_independence_cases(self) -> usize {
        self.timeline_row_order_independence
    }

    /// Number of timeline bounds report cases exercised.
    pub const fn timeline_bounds_report_cases(self) -> usize {
        self.timeline_bounds_reports
    }

    /// Number of caller storage-owned timeline rejection cases exercised.
    pub const fn timeline_caller_rejection_cases(self) -> usize {
        self.timeline_caller_rejections
    }

    /// Number of cache put commit cases exercised.
    pub const fn cache_put_commit_cases(self) -> usize {
        self.cache_put_commits
    }

    /// Number of cache delete commit cases exercised.
    pub const fn cache_delete_commit_cases(self) -> usize {
        self.cache_delete_commits
    }

    /// Number of mixed cache commit cases exercised.
    pub const fn cache_mixed_commit_cases(self) -> usize {
        self.cache_mixed_commits
    }

    /// Number of one-version-per-cache-batch cases exercised.
    pub const fn cache_one_version_per_batch_cases(self) -> usize {
        self.cache_one_version_per_batch
    }

    /// Number of one-timestamp-per-cache-batch cases exercised.
    pub const fn cache_one_timestamp_per_batch_cases(self) -> usize {
        self.cache_one_timestamp_per_batch
    }

    /// Number of cache timeline installation cases exercised.
    pub const fn cache_timeline_rows_installed_cases(self) -> usize {
        self.cache_timeline_rows_installed
    }

    /// Number of cache visible publication cases exercised.
    pub const fn cache_visible_publication_cases(self) -> usize {
        self.cache_visible_publications
    }

    /// Number of not-durable cache outcome cases exercised.
    pub const fn cache_not_durable_outcome_cases(self) -> usize {
        self.cache_not_durable_outcomes
    }

    /// Number of cache branch admission rejection cases exercised.
    pub const fn cache_branch_admission_rejection_cases(self) -> usize {
        self.cache_branch_admission_rejections
    }

    /// Number of cache conflict rejection cases exercised.
    pub const fn cache_conflict_rejection_cases(self) -> usize {
        self.cache_conflict_rejections
    }

    /// Number of non-cache durability rejection cases exercised by L7H.
    pub const fn cache_non_cache_rejection_cases(self) -> usize {
        self.cache_non_cache_rejections
    }

    /// Number of cache apply-failure atomicity cases exercised.
    pub const fn cache_apply_failure_atomicity_cases(self) -> usize {
        self.cache_apply_failure_atomicity
    }

    /// Number of post-allocation version-gap cases exercised.
    pub const fn cache_version_gap_after_failure_cases(self) -> usize {
        self.cache_version_gap_after_failure
    }

    /// Number of branch-applied-above-visible rejection cases exercised.
    pub const fn cache_applied_above_visible_rejection_cases(self) -> usize {
        self.cache_applied_above_visible_rejections
    }

    /// Number of allocator/visible mismatch rejection cases exercised.
    pub const fn cache_visible_allocator_mismatch_rejection_cases(self) -> usize {
        self.cache_visible_allocator_mismatch_rejections
    }

    /// Number of guard-release-after-cache-failure cases exercised.
    pub const fn cache_guard_release_after_failure_cases(self) -> usize {
        self.cache_guard_release_after_failure
    }

    /// Number of standard durable commit cases exercised.
    pub const fn durable_standard_commit_cases(self) -> usize {
        self.durable_standard_commits
    }

    /// Number of always-durable commit cases exercised.
    pub const fn durable_always_commit_cases(self) -> usize {
        self.durable_always_commits
    }

    /// Number of durable WAL payload parity cases exercised.
    pub const fn durable_wal_payload_parity_cases(self) -> usize {
        self.durable_wal_payload_parity
    }

    /// Number of clean WAL failure cases exercised.
    pub const fn durable_clean_wal_failure_cases(self) -> usize {
        self.durable_clean_wal_failures
    }

    /// Number of uncertain WAL failure cases exercised.
    pub const fn durable_uncertain_wal_failure_cases(self) -> usize {
        self.durable_uncertain_wal_failures
    }

    /// Number of durable-executor cache-mode rejection cases exercised.
    pub const fn durable_cache_mode_rejection_cases(self) -> usize {
        self.durable_cache_mode_rejections
    }

    /// Number of WAL durability policy mismatch cases exercised.
    pub const fn durable_policy_mismatch_cases(self) -> usize {
        self.durable_policy_mismatches
    }

    /// Number of always-durable unforced append rejection cases exercised.
    pub const fn durable_unforced_always_rejection_cases(self) -> usize {
        self.durable_unforced_always_rejections
    }

    /// Number of guard-release-after-durable-failure cases exercised.
    pub const fn durable_guard_release_after_failure_cases(self) -> usize {
        self.durable_guard_release_after_failure
    }

    /// Number of read-only durable executor rejection cases exercised.
    pub const fn durable_read_only_rejection_cases(self) -> usize {
        self.durable_read_only_rejections
    }

    /// Number of valid unresolved durable fact cases exercised.
    pub const fn durable_unresolved_fact_validation_cases(self) -> usize {
        self.durable_unresolved_fact_validations
    }

    /// Number of invalid unresolved durable fact cases exercised.
    pub const fn durable_unresolved_fact_rejection_cases(self) -> usize {
        self.durable_unresolved_fact_rejections
    }

    /// Number of first-record unresolved durable gate cases exercised.
    pub const fn durable_unresolved_gate_record_cases(self) -> usize {
        self.durable_unresolved_gate_records
    }

    /// Number of idempotent unresolved durable gate record cases exercised.
    pub const fn durable_unresolved_gate_idempotent_record_cases(self) -> usize {
        self.durable_unresolved_gate_idempotent_records
    }

    /// Number of different-fact unresolved durable gate rejection cases exercised.
    pub const fn durable_unresolved_gate_different_fact_rejection_cases(self) -> usize {
        self.durable_unresolved_gate_different_fact_rejections
    }

    /// Number of exact-clear unresolved durable gate cases exercised.
    pub const fn durable_unresolved_gate_exact_clear_cases(self) -> usize {
        self.durable_unresolved_gate_exact_clears
    }

    /// Number of durable-not-applied gate cases exercised.
    pub const fn durable_not_applied_gate_cases(self) -> usize {
        self.durable_not_applied_gates
    }

    /// Number of applied-not-visible gate cases exercised.
    pub const fn durable_applied_not_visible_gate_cases(self) -> usize {
        self.durable_applied_not_visible_gates
    }

    /// Number of unresolved durable gate blocking cases exercised.
    pub const fn durable_unresolved_gate_block_cases(self) -> usize {
        self.durable_unresolved_gate_blocks
    }

    /// Number of cache commits blocked by unresolved durable gates.
    pub const fn durable_unresolved_gate_cache_block_cases(self) -> usize {
        self.durable_unresolved_gate_cache_blocks
    }

    /// Number of read-only diagnostic cases allowed by unresolved durable gates.
    pub const fn durable_unresolved_gate_read_only_diagnostic_cases(self) -> usize {
        self.durable_unresolved_gate_read_only_diagnostics
    }

    /// Number of clean WAL failure cases that leave no unresolved durable gate.
    pub const fn durable_clean_wal_no_gate_cases(self) -> usize {
        self.durable_clean_wal_no_gate
    }

    /// Number of uncertain WAL failure cases that leave no durable-but-not-visible gate.
    pub const fn durable_uncertain_wal_no_gate_cases(self) -> usize {
        self.durable_uncertain_wal_no_gate
    }
}

/// Runs one deterministic generated scaffold contract case for the commit runtime.
pub fn check_commit_runtime_scaffold_contract(
    script: &[u8],
) -> Result<CommitRuntimeScaffoldOutcome, TestkitError> {
    let mut outcome = CommitRuntimeScaffoldOutcome::default();

    check_valid_config(script)?;
    outcome.valid_config += 1;
    outcome.invalid_config += check_invalid_configs()?;

    check_phase_facts(script)?;
    outcome.phase_facts += 1;

    check_visibility_facts(script)?;
    outcome.visibility_facts += 1;
    outcome.invalid_visibility_facts += check_invalid_visibility_facts()?;

    check_error_display()?;
    outcome.error_displays += 1;
    check_error_source()?;
    outcome.error_sources += 1;

    check_stats(script)?;
    outcome.stats += 1;

    check_source_guard_fixtures()?;
    outcome.source_guard_fixtures += 1;

    check_valid_batch(script)?;
    outcome.valid_batches += 1;
    outcome.invalid_batches += check_invalid_batches()?;
    outcome.duplicate_mutations += check_duplicate_mutations()?;
    outcome.branch_mismatches += check_branch_mismatches()?;
    outcome.storage_owned_spaces += check_storage_owned_spaces()?;
    outcome.invalid_fact_cases += check_invalid_fact_cases()?;
    outcome.expiry_rejections += check_expiry_rejections()?;
    check_stamping(script)?;
    outcome.stamping_cases += 1;
    outcome.stamping_rejections += check_stamping_rejections()?;

    absorb_allocator_contract(script, &mut outcome)?;
    absorb_outcome_contract(script, &mut outcome)?;
    absorb_branch_guard_contract(script, &mut outcome)?;
    absorb_conflict_contract(script, &mut outcome)?;
    absorb_timeline_contract(script, &mut outcome)?;
    absorb_cache_contract(script, &mut outcome)?;
    absorb_durable_contract(script, &mut outcome)?;

    Ok(outcome)
}

fn absorb_allocator_contract(
    script: &[u8],
    outcome: &mut CommitRuntimeScaffoldOutcome,
) -> Result<(), TestkitError> {
    let allocator_outcome = check_commit_runtime_allocator_contract(script)?;
    outcome.version_allocations += allocator_outcome.version_allocations;
    outcome.version_catch_ups += allocator_outcome.version_catch_ups;
    outcome.version_overflows += allocator_outcome.version_overflows;
    outcome.generated_timestamps += allocator_outcome.generated_timestamps;
    outcome.clamped_timestamps += allocator_outcome.clamped_timestamps;
    outcome.explicit_timestamps += allocator_outcome.explicit_timestamps;
    outcome.invalid_explicit_timestamps += allocator_outcome.invalid_explicit_timestamps;
    outcome.timestamp_source_failures += allocator_outcome.timestamp_source_failures;
    outcome.read_only_no_allocations += allocator_outcome.read_only_no_allocations;
    outcome.no_transaction_id_checks += allocator_outcome.no_transaction_id_checks;
    Ok(())
}

fn absorb_outcome_contract(
    script: &[u8],
    outcome: &mut CommitRuntimeScaffoldOutcome,
) -> Result<(), TestkitError> {
    let outcome_contract = check_commit_runtime_outcome_contract(script)?;
    outcome.read_only_outcomes += outcome_contract.read_only_outcomes;
    outcome.read_only_disabled_rejections += outcome_contract.read_only_disabled_rejections;
    outcome.visible_tracker_initializations += outcome_contract.visible_tracker_initializations;
    outcome.visible_tracker_monotonic_publishes +=
        outcome_contract.visible_tracker_monotonic_publishes;
    outcome.visible_tracker_regression_rejections +=
        outcome_contract.visible_tracker_regression_rejections;
    outcome.outcome_invalid_visibility_facts += outcome_contract.invalid_visibility_facts;
    outcome.outcome_constructor_rejections += outcome_contract.outcome_constructor_rejections;
    outcome.mutation_count_facts += outcome_contract.mutation_count_facts;
    outcome.cross_branch_read_only_facts += outcome_contract.cross_branch_read_only_facts;
    outcome.read_only_outcome_no_allocations += outcome_contract.read_only_no_allocation_proofs;
    Ok(())
}

fn absorb_branch_guard_contract(
    script: &[u8],
    outcome: &mut CommitRuntimeScaffoldOutcome,
) -> Result<(), TestkitError> {
    let branch_guard_contract = check_commit_runtime_branch_guard_contract(script)?;
    outcome.branch_registration_successes += branch_guard_contract.branch_registration_successes;
    outcome.duplicate_registration_rejections +=
        branch_guard_contract.duplicate_registration_rejections;
    outcome.missing_branch_rejections += branch_guard_contract.missing_branch_rejections;
    outcome.deleting_branch_rejections += branch_guard_contract.deleting_branch_rejections;
    outcome.generation_exact_matches += branch_guard_contract.generation_exact_matches;
    outcome.generation_mismatches += branch_guard_contract.generation_mismatches;
    outcome.generation_not_supplied += branch_guard_contract.generation_not_supplied;
    outcome.stale_generation_after_recreate +=
        branch_guard_contract.stale_generation_after_recreate;
    outcome.same_branch_guard_contentions += branch_guard_contract.same_branch_guard_contentions;
    outcome.different_branch_simultaneous_guards +=
        branch_guard_contract.different_branch_simultaneous_guards;
    outcome.quiesce_start_successes += branch_guard_contract.quiesce_start_successes;
    outcome.quiesce_rejected_with_active_guards +=
        branch_guard_contract.quiesce_rejected_with_active_guards;
    outcome.mutating_guard_rejected_during_quiesce +=
        branch_guard_contract.mutating_guard_rejected_during_quiesce;
    outcome.read_only_allowed_during_quiesce +=
        branch_guard_contract.read_only_allowed_during_quiesce;
    outcome.guard_release_and_reacquire += branch_guard_contract.guard_release_and_reacquire;
    Ok(())
}

fn absorb_conflict_contract(
    script: &[u8],
    outcome: &mut CommitRuntimeScaffoldOutcome,
) -> Result<(), TestkitError> {
    let conflict_contract = check_commit_runtime_conflict_contract(script)?;
    outcome.read_present_matches += conflict_contract.read_present_matches;
    outcome.read_present_mismatches += conflict_contract.read_present_mismatches;
    outcome.read_present_becomes_missing += conflict_contract.read_present_becomes_missing;
    outcome.read_missing_matches += conflict_contract.read_missing_matches;
    outcome.read_missing_becomes_present += conflict_contract.read_missing_becomes_present;
    outcome.cas_present_matches += conflict_contract.cas_present_matches;
    outcome.cas_present_mismatches += conflict_contract.cas_present_mismatches;
    outcome.cas_present_becomes_missing += conflict_contract.cas_present_becomes_missing;
    outcome.cas_missing_matches += conflict_contract.cas_missing_matches;
    outcome.cas_missing_becomes_present += conflict_contract.cas_missing_becomes_present;
    outcome.combined_read_before_cas += conflict_contract.combined_read_before_cas;
    outcome.blind_put_no_conflicts += conflict_contract.blind_put_no_conflicts;
    outcome.blind_delete_no_conflicts += conflict_contract.blind_delete_no_conflicts;
    outcome.skip_mode_no_reads += conflict_contract.skip_mode_no_reads;
    outcome.lower_layer_read_failures += conflict_contract.lower_layer_read_failures;
    outcome.conflict_error_vocabulary += conflict_contract.conflict_error_vocabulary;
    Ok(())
}

fn absorb_timeline_contract(
    script: &[u8],
    outcome: &mut CommitRuntimeScaffoldOutcome,
) -> Result<(), TestkitError> {
    let timeline_contract = check_commit_runtime_timeline_contract(script)?;
    outcome.valid_timeline_entries += timeline_contract.valid_entries;
    outcome.zero_timeline_version_rejections += timeline_contract.zero_version_rejections;
    outcome.timestamp_index_keys += timeline_contract.timestamp_index_keys;
    outcome.version_index_keys += timeline_contract.version_index_keys;
    outcome.timeline_row_pairs += timeline_contract.row_pairs;
    outcome.timeline_shared_commit_facts += timeline_contract.shared_commit_facts;
    outcome.timestamp_index_decodes += timeline_contract.timestamp_index_decodes;
    outcome.version_index_decodes += timeline_contract.version_index_decodes;
    outcome.malformed_timeline_prefix_rejections += timeline_contract.malformed_prefix_rejections;
    outcome.malformed_timeline_key_length_rejections +=
        timeline_contract.malformed_key_length_rejections;
    outcome.timeline_value_length_rejections += timeline_contract.value_length_rejections;
    outcome.timeline_key_value_mismatch_rejections +=
        timeline_contract.key_value_mismatch_rejections;
    outcome.timestamp_lookup_exact_matches += timeline_contract.timestamp_lookup_exact_matches;
    outcome.timestamp_lookup_between_matches += timeline_contract.timestamp_lookup_between_matches;
    outcome.duplicate_timestamp_tiebreaks += timeline_contract.duplicate_timestamp_tiebreaks;
    outcome.version_timestamp_lookups += timeline_contract.version_timestamp_lookups;
    outcome.timeline_branch_isolations += timeline_contract.branch_isolations;
    outcome.timeline_row_order_independence += timeline_contract.row_order_independence;
    outcome.timeline_bounds_reports += timeline_contract.bounds_reports;
    outcome.timeline_caller_rejections += timeline_contract.caller_rejections;
    Ok(())
}

fn absorb_cache_contract(
    script: &[u8],
    outcome: &mut CommitRuntimeScaffoldOutcome,
) -> Result<(), TestkitError> {
    let cache_contract = check_commit_runtime_cache_contract(script)?;
    outcome.cache_put_commits += cache_contract.put_commits;
    outcome.cache_delete_commits += cache_contract.delete_commits;
    outcome.cache_mixed_commits += cache_contract.mixed_commits;
    outcome.cache_one_version_per_batch += cache_contract.one_version_per_batch;
    outcome.cache_one_timestamp_per_batch += cache_contract.one_timestamp_per_batch;
    outcome.cache_timeline_rows_installed += cache_contract.timeline_rows_installed;
    outcome.cache_visible_publications += cache_contract.visible_publications;
    outcome.cache_not_durable_outcomes += cache_contract.not_durable_outcomes;
    outcome.cache_branch_admission_rejections += cache_contract.branch_admission_rejections;
    outcome.cache_conflict_rejections += cache_contract.conflict_rejections;
    outcome.cache_non_cache_rejections += cache_contract.non_cache_rejections;
    outcome.cache_apply_failure_atomicity += cache_contract.apply_failure_atomicity;
    outcome.cache_version_gap_after_failure += cache_contract.version_gap_after_failure;
    outcome.cache_applied_above_visible_rejections +=
        cache_contract.applied_above_visible_rejections;
    outcome.cache_visible_allocator_mismatch_rejections +=
        cache_contract.visible_allocator_mismatch_rejections;
    outcome.cache_guard_release_after_failure += cache_contract.guard_release_after_failure;
    Ok(())
}

fn absorb_durable_contract(
    script: &[u8],
    outcome: &mut CommitRuntimeScaffoldOutcome,
) -> Result<(), TestkitError> {
    let durable_contract = check_commit_runtime_durable_contract(script)?;
    outcome.durable_standard_commits += durable_contract.standard_commits;
    outcome.durable_always_commits += durable_contract.always_commits;
    outcome.durable_wal_payload_parity += durable_contract.wal_payload_parity;
    outcome.durable_clean_wal_failures += durable_contract.clean_wal_failures;
    outcome.durable_uncertain_wal_failures += durable_contract.uncertain_wal_failures;
    outcome.durable_cache_mode_rejections += durable_contract.cache_mode_rejections;
    outcome.durable_policy_mismatches += durable_contract.policy_mismatches;
    outcome.durable_unforced_always_rejections += durable_contract.unforced_always_rejections;
    outcome.durable_guard_release_after_failure += durable_contract.guard_release_after_failure;
    outcome.durable_read_only_rejections += durable_contract.read_only_rejections;
    outcome.durable_unresolved_fact_validations += durable_contract.unresolved_fact_validation;
    outcome.durable_unresolved_fact_rejections += durable_contract.unresolved_fact_rejections;
    outcome.durable_unresolved_gate_records += durable_contract.unresolved_gate_records;
    outcome.durable_unresolved_gate_idempotent_records +=
        durable_contract.unresolved_gate_idempotent_records;
    outcome.durable_unresolved_gate_different_fact_rejections +=
        durable_contract.unresolved_gate_different_fact_rejections;
    outcome.durable_unresolved_gate_exact_clears += durable_contract.unresolved_gate_exact_clears;
    outcome.durable_not_applied_gates += durable_contract.durable_not_applied_gates;
    outcome.durable_applied_not_visible_gates += durable_contract.applied_not_visible_gates;
    outcome.durable_unresolved_gate_blocks += durable_contract.unresolved_gate_blocks;
    outcome.durable_unresolved_gate_cache_blocks += durable_contract.unresolved_gate_cache_blocks;
    outcome.durable_unresolved_gate_read_only_diagnostics +=
        durable_contract.unresolved_gate_read_only_diagnostics;
    outcome.durable_clean_wal_no_gate += durable_contract.clean_wal_no_gate;
    outcome.durable_uncertain_wal_no_gate += durable_contract.uncertain_wal_no_gate;
    Ok(())
}

fn check_valid_config(script: &[u8]) -> Result<(), TestkitError> {
    let max_mutations = 1 + usize::from(script_byte(script, 0));
    let max_validation_facts = 1 + usize::from(script_byte(script, 1));
    let max_commit_rows = max_mutations + usize::from(script_byte(script, 2));
    let diagnostics = if script_byte(script, 3).is_multiple_of(2) {
        CommitReadOnlyDiagnostics::Enabled
    } else {
        CommitReadOnlyDiagnostics::Disabled
    };
    let config = CommitRuntimeConfig::new(
        max_mutations,
        max_validation_facts,
        max_commit_rows,
        diagnostics,
    )
    .map_err(|err| TestkitError::new(format!("valid config rejected: {err}")))?;

    if config.max_mutations_per_batch() == 0
        || config.max_validation_facts_per_batch() == 0
        || config.max_commit_rows_per_batch() < config.max_mutations_per_batch()
    {
        return Err(TestkitError::new("valid config produced impossible limits"));
    }
    Ok(())
}

fn check_invalid_configs() -> Result<usize, TestkitError> {
    let cases = [
        CommitRuntimeConfig::new(0, 1, 1, CommitReadOnlyDiagnostics::Enabled),
        CommitRuntimeConfig::new(1, 0, 1, CommitReadOnlyDiagnostics::Enabled),
        CommitRuntimeConfig::new(1, 1, 0, CommitReadOnlyDiagnostics::Enabled),
        CommitRuntimeConfig::new(2, 1, 1, CommitReadOnlyDiagnostics::Enabled),
    ];
    for case in &cases {
        if case.is_ok() {
            return Err(TestkitError::new("invalid config was accepted"));
        }
    }
    Ok(cases.len())
}

fn check_phase_facts(script: &[u8]) -> Result<(), TestkitError> {
    let phases = [
        CommitPhase::RejectedBeforeAllocation,
        CommitPhase::AllocatedNotDurable,
        CommitPhase::DurableNotApplied,
        CommitPhase::AppliedNotVisible,
        CommitPhase::Visible,
        CommitPhase::Replay,
    ];
    let durability = [
        CommitDurabilityClass::NotDurable,
        CommitDurabilityClass::Standard,
        CommitDurabilityClass::Always,
        CommitDurabilityClass::Uncertain,
    ];
    let phase = phases[usize::from(script_byte(script, 4)) % phases.len()];
    let class = durability[usize::from(script_byte(script, 5)) % durability.len()];

    if !phases.contains(&phase) || !durability.contains(&class) {
        return Err(TestkitError::new("phase or durability fact escaped enum"));
    }
    Ok(())
}

fn check_visibility_facts(script: &[u8]) -> Result<(), TestkitError> {
    let version = CommitVersion::new(u64::from(script_byte(script, 6)) + 1);
    let facts = CommitVisibilityFacts::new(
        Some(version),
        Some(version),
        Some(version),
        Some(version),
        Some(version),
    )
    .map_err(|err| TestkitError::new(format!("valid visibility facts rejected: {err}")))?;

    if facts.visible_version() != Some(version) || facts.timeline_version() != Some(version) {
        return Err(TestkitError::new(
            "visibility facts did not preserve visible/timeline versions",
        ));
    }
    Ok(())
}

fn check_invalid_visibility_facts() -> Result<usize, TestkitError> {
    let cases = [
        CommitVisibilityFacts::new(
            Some(CommitVersion::new(1)),
            Some(CommitVersion::new(2)),
            None,
            None,
            None,
        ),
        CommitVisibilityFacts::new(
            Some(CommitVersion::new(1)),
            None,
            Some(CommitVersion::new(2)),
            None,
            None,
        ),
        CommitVisibilityFacts::new(
            Some(CommitVersion::new(2)),
            None,
            Some(CommitVersion::new(1)),
            Some(CommitVersion::new(2)),
            Some(CommitVersion::new(2)),
        ),
        CommitVisibilityFacts::new(
            Some(CommitVersion::new(2)),
            None,
            Some(CommitVersion::new(2)),
            None,
            Some(CommitVersion::new(3)),
        ),
        CommitVisibilityFacts::new(
            Some(CommitVersion::new(2)),
            None,
            Some(CommitVersion::new(2)),
            Some(CommitVersion::new(2)),
            Some(CommitVersion::new(1)),
        ),
    ];
    for case in &cases {
        if case.is_ok() {
            return Err(TestkitError::new("invalid visibility facts were accepted"));
        }
    }
    Ok(cases.len())
}

fn check_error_display() -> Result<(), TestkitError> {
    let display = CommitRuntimeError::InvalidCommitState {
        reason: "visible before applied",
    }
    .to_string();

    if !display.contains("commit state") || display.contains("VersionedValue") {
        return Err(TestkitError::new(
            "commit error display used wrong vocabulary",
        ));
    }
    Ok(())
}

fn check_error_source() -> Result<(), TestkitError> {
    let err = CommitRuntimeError::lower_layer_with(
        CommitLowerLayer::WalService,
        "append failed before visibility",
        ScaffoldSource,
    );
    match err.source().map(ToString::to_string) {
        Some(source) if source == "scaffold source" => Ok(()),
        _ => Err(TestkitError::new(
            "commit error did not preserve source chain",
        )),
    }
}

fn check_stats(script: &[u8]) -> Result<(), TestkitError> {
    let stats = CommitRuntimeStats::new(
        u64::from(script_byte(script, 7)),
        u64::from(script_byte(script, 8)),
        u64::from(script_byte(script, 9)),
        u64::from(script_byte(script, 10)),
        u64::from(script_byte(script, 11)),
    );
    if stats.committed_batches() != u64::from(script_byte(script, 7))
        || stats.read_only_batches() != u64::from(script_byte(script, 8))
        || stats.rejected_batches() != u64::from(script_byte(script, 9))
        || stats.replayed_batches() != u64::from(script_byte(script, 10))
        || stats.durable_but_not_visible() != u64::from(script_byte(script, 11))
    {
        return Err(TestkitError::new("commit stats did not preserve counters"));
    }
    Ok(())
}

fn check_source_guard_fixtures() -> Result<(), TestkitError> {
    let allowed = [
        "pub(crate) struct CommitRuntime;",
        "BranchId CommitVersion Timestamp StorageRow WalRecord WalService",
    ];
    for fixture in allowed {
        if fixture.contains("VersionedValue") {
            return Err(TestkitError::new(
                "allowed fixture contains forbidden vocabulary",
            ));
        }
    }
    Ok(())
}

fn check_valid_batch(script: &[u8]) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 12));
    let config = CommitRuntimeConfig::default();
    let mutation_count = 1 + usize::from(script_byte(script, 16) % 32);
    let options = CommitBatchOptions::new(
        durability_mode(script_byte(script, 13)),
        conflict_mode(script_byte(script, 14)),
        CommitDuplicateKeyPolicy::Reject,
        timestamp_policy(script_byte(script, 15)),
        CommitOrigin::StorageRuntime,
    );
    let validation = CommitValidationFacts::new(
        vec![CommitReadFact::new(
            physical_key(branch, 0x20, b"read".to_vec()),
            CommitObservedVersion::Missing,
        )],
        vec![CommitCasFact::new(
            physical_key(branch, 0x20, b"cas".to_vec()),
            CommitObservedVersion::Present(CommitVersion::new(3)),
        )],
    );
    let mut mutations = Vec::with_capacity(mutation_count);
    for index in 0..mutation_count {
        let storage_space = 0x20 + u8::try_from(index % 8).expect("bounded index");
        let key = physical_key(
            branch,
            storage_space,
            vec![
                u8::try_from(index).expect("bounded mutation count"),
                script_byte(script, 17),
            ],
        );
        if (index + usize::from(script_byte(script, 18))).is_multiple_of(2) {
            mutations.push(CommitMutation::put(
                key,
                vec![
                    script_byte(script, 19),
                    u8::try_from(index).expect("bounded index"),
                ],
                expiry(script_byte(script, 20)),
                CommitRetentionHint::Append,
            ));
        } else {
            mutations.push(CommitMutation::delete(key));
        }
    }
    let batch = CommitBatch::mutating(branch, mutations, validation, options);

    let validated = batch
        .validate(&config)
        .map_err(|err| TestkitError::new(format!("valid batch rejected: {err}")))?;
    if validated.batch().mutations().len() != mutation_count
        || validated.batch().branch_id() != branch
        || validated.batch().options().duplicate_policy() != CommitDuplicateKeyPolicy::Reject
    {
        return Err(TestkitError::new("valid batch did not preserve facts"));
    }
    Ok(())
}

fn check_invalid_batches() -> Result<usize, TestkitError> {
    let branch = branch_id(20);
    let config = CommitRuntimeConfig::new(1, 1, 1, CommitReadOnlyDiagnostics::Enabled)
        .map_err(|err| TestkitError::new(format!("config rejected: {err}")))?;
    let cases = [
        CommitBatch::mutating(
            branch,
            Vec::new(),
            CommitValidationFacts::empty(),
            CommitBatchOptions::default(),
        )
        .validate(&config),
        CommitBatch::mutating(
            branch,
            vec![
                CommitMutation::delete(physical_key(branch, 0x20, b"a".to_vec())),
                CommitMutation::delete(physical_key(branch, 0x20, b"b".to_vec())),
            ],
            CommitValidationFacts::empty(),
            CommitBatchOptions::default(),
        )
        .validate(&config),
    ];
    for case in &cases {
        if case.is_ok() {
            return Err(TestkitError::new("invalid batch was accepted"));
        }
    }
    Ok(cases.len())
}

fn check_duplicate_mutations() -> Result<usize, TestkitError> {
    let branch = branch_id(21);
    let key = physical_key(branch, 0x20, b"dup".to_vec());
    let cases = [
        CommitBatch::mutating(
            branch,
            vec![
                CommitMutation::put(
                    key.clone(),
                    b"one".to_vec(),
                    CommitExpiry::None,
                    CommitRetentionHint::Append,
                ),
                CommitMutation::delete(key.clone()),
            ],
            CommitValidationFacts::empty(),
            CommitBatchOptions::default(),
        )
        .validate(&CommitRuntimeConfig::default()),
        CommitBatch::mutating(
            branch,
            vec![
                CommitMutation::delete(key.clone()),
                CommitMutation::delete(key),
            ],
            CommitValidationFacts::empty(),
            CommitBatchOptions::default(),
        )
        .validate(&CommitRuntimeConfig::default()),
    ];
    for case in &cases {
        if case.is_ok() {
            return Err(TestkitError::new("duplicate mutation was accepted"));
        }
    }
    Ok(cases.len())
}

fn check_branch_mismatches() -> Result<usize, TestkitError> {
    let branch = branch_id(22);
    let other = branch_id(23);
    let cases = [
        CommitBatch::mutating(
            branch,
            vec![CommitMutation::delete(physical_key(
                other,
                0x20,
                b"x".to_vec(),
            ))],
            CommitValidationFacts::empty(),
            CommitBatchOptions::default(),
        )
        .validate(&CommitRuntimeConfig::default()),
        CommitBatch::mutating(
            branch,
            vec![CommitMutation::delete(physical_key(
                branch,
                0x20,
                b"x".to_vec(),
            ))],
            CommitValidationFacts::new(
                vec![CommitReadFact::new(
                    physical_key(other, 0x20, b"x".to_vec()),
                    CommitObservedVersion::Missing,
                )],
                Vec::new(),
            ),
            CommitBatchOptions::default(),
        )
        .validate(&CommitRuntimeConfig::default()),
    ];
    for case in &cases {
        if case.is_ok() {
            return Err(TestkitError::new("branch mismatch was accepted"));
        }
    }
    Ok(cases.len())
}

fn check_storage_owned_spaces() -> Result<usize, TestkitError> {
    let branch = branch_id(24);
    let timeline = storage_owned_key(branch, b"timeline".to_vec());
    let cases = [
        CommitBatch::mutating(
            branch,
            vec![CommitMutation::delete(timeline.clone())],
            CommitValidationFacts::empty(),
            CommitBatchOptions::default(),
        )
        .validate(&CommitRuntimeConfig::default()),
        CommitBatch::mutating(
            branch,
            vec![CommitMutation::delete(physical_key(
                branch,
                0x20,
                b"x".to_vec(),
            ))],
            CommitValidationFacts::new(
                Vec::new(),
                vec![CommitCasFact::new(timeline, CommitObservedVersion::Missing)],
            ),
            CommitBatchOptions::default(),
        )
        .validate(&CommitRuntimeConfig::default()),
    ];
    for case in &cases {
        if case.is_ok() {
            return Err(TestkitError::new("storage-owned caller input was accepted"));
        }
    }
    Ok(cases.len())
}

fn check_invalid_fact_cases() -> Result<usize, TestkitError> {
    let branch = branch_id(25);
    let key = physical_key(branch, 0x20, b"fact".to_vec());
    let cases = [
        CommitBatch::mutating(
            branch,
            vec![CommitMutation::delete(physical_key(
                branch,
                0x20,
                b"x".to_vec(),
            ))],
            CommitValidationFacts::new(
                vec![CommitReadFact::new(
                    key.clone(),
                    CommitObservedVersion::Present(CommitVersion::ZERO),
                )],
                Vec::new(),
            ),
            CommitBatchOptions::default(),
        )
        .validate(&CommitRuntimeConfig::default()),
        CommitBatch::mutating(
            branch,
            vec![CommitMutation::delete(physical_key(
                branch,
                0x20,
                b"x".to_vec(),
            ))],
            CommitValidationFacts::new(
                vec![
                    CommitReadFact::new(key.clone(), CommitObservedVersion::Missing),
                    CommitReadFact::new(key, CommitObservedVersion::Missing),
                ],
                Vec::new(),
            ),
            CommitBatchOptions::default(),
        )
        .validate(&CommitRuntimeConfig::default()),
    ];
    for case in &cases {
        if case.is_ok() {
            return Err(TestkitError::new("invalid validation facts were accepted"));
        }
    }
    Ok(cases.len())
}

fn check_expiry_rejections() -> Result<usize, TestkitError> {
    let branch = branch_id(26);
    let case = CommitBatch::mutating(
        branch,
        vec![CommitMutation::put(
            physical_key(branch, 0x20, b"ttl".to_vec()),
            b"value".to_vec(),
            CommitExpiry::At(Timestamp::EPOCH),
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    )
    .validate(&CommitRuntimeConfig::default());
    if case.is_ok() {
        return Err(TestkitError::new("epoch expiry was accepted"));
    }
    Ok(1)
}

fn check_stamping(script: &[u8]) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 26));
    let value = vec![script_byte(script, 27), 0x00, 0xff];
    let commit_version = CommitVersion::new(u64::from(script_byte(script, 28)) + 1);
    let commit_timestamp = Timestamp::from_micros(u64::from(script_byte(script, 29)) + 1);
    let keep_last = std::num::NonZeroUsize::new(2).expect("nonzero");
    let batch = CommitBatch::mutating(
        branch,
        vec![
            CommitMutation::put(
                physical_key(branch, 0x20, b"put".to_vec()),
                value.clone(),
                CommitExpiry::None,
                CommitRetentionHint::KeepLastNonZero(keep_last),
            ),
            CommitMutation::delete(physical_key(branch, 0x20, b"delete".to_vec())),
        ],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    )
    .validate(&CommitRuntimeConfig::default())
    .map_err(|err| TestkitError::new(format!("stamp batch rejected: {err}")))?;
    let stamped = batch
        .stamp_user_rows(
            CommitStamp::new(branch, commit_version, commit_timestamp)
                .map_err(|err| TestkitError::new(format!("stamp rejected: {err}")))?,
        )
        .map_err(|err| TestkitError::new(format!("row stamping rejected: {err}")))?;

    if stamped.rows().len() != 2
        || stamped.rows()[0].value() != value.as_slice()
        || stamped.rows()[0].commit_version() != commit_version
        || !stamped.rows()[1].is_tombstone()
        || stamped.rows()[1].commit_timestamp() != commit_timestamp
        || stamped.retention_hints()
            != [Some(CommitRetentionHint::KeepLastNonZero(keep_last)), None].as_slice()
    {
        return Err(TestkitError::new("stamped rows did not preserve facts"));
    }
    Ok(())
}

fn check_stamping_rejections() -> Result<usize, TestkitError> {
    let branch = branch_id(30);
    let other = branch_id(31);
    let read_only = CommitBatch::read_only_diagnostic(
        branch,
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    )
    .validate(&CommitRuntimeConfig::default())
    .map_err(|err| TestkitError::new(format!("read-only batch rejected: {err}")))?;
    let mutating = CommitBatch::mutating(
        branch,
        vec![CommitMutation::delete(physical_key(
            branch,
            0x20,
            b"delete".to_vec(),
        ))],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    )
    .validate(&CommitRuntimeConfig::default())
    .map_err(|err| TestkitError::new(format!("mutating batch rejected: {err}")))?;
    let cases = [
        read_only.stamp_user_rows(
            CommitStamp::new(branch, CommitVersion::new(1), Timestamp::from_micros(1))
                .map_err(|err| TestkitError::new(format!("stamp rejected: {err}")))?,
        ),
        mutating.stamp_user_rows(
            CommitStamp::new(other, CommitVersion::new(1), Timestamp::from_micros(1))
                .map_err(|err| TestkitError::new(format!("stamp rejected: {err}")))?,
        ),
    ];
    for case in &cases {
        if case.is_ok() {
            return Err(TestkitError::new("invalid stamping was accepted"));
        }
    }
    Ok(cases.len())
}

fn script_byte(script: &[u8], index: usize) -> u8 {
    script.get(index).copied().unwrap_or(0)
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn physical_key(branch_id: BranchId, storage_space_id: u8, user_key: Vec<u8>) -> PhysicalKey {
    PhysicalKey::new(
        branch_id,
        "default",
        StorageSpaceId::engine(storage_space_id).expect("engine-owned space"),
        user_key,
    )
    .expect("physical key")
}

fn storage_owned_key(branch_id: BranchId, user_key: Vec<u8>) -> PhysicalKey {
    PhysicalKey::new(
        branch_id,
        "timeline",
        StorageSpaceId::COMMIT_TIMELINE,
        user_key,
    )
    .expect("storage-owned physical key")
}

fn durability_mode(byte: u8) -> CommitDurabilityMode {
    match byte % 3 {
        0 => CommitDurabilityMode::Cache,
        1 => CommitDurabilityMode::Standard,
        _ => CommitDurabilityMode::Always,
    }
}

fn conflict_mode(byte: u8) -> CommitConflictValidationMode {
    if byte.is_multiple_of(2) {
        CommitConflictValidationMode::Validate
    } else {
        CommitConflictValidationMode::Skip
    }
}

fn timestamp_policy(byte: u8) -> CommitTimestampPolicy {
    if byte.is_multiple_of(2) {
        CommitTimestampPolicy::RuntimeGenerated
    } else {
        CommitTimestampPolicy::Explicit(Timestamp::from_micros(u64::from(byte) + 1))
    }
}

fn expiry(byte: u8) -> CommitExpiry {
    if byte.is_multiple_of(2) {
        CommitExpiry::None
    } else {
        CommitExpiry::At(Timestamp::from_micros(u64::from(byte) + 1))
    }
}

#[derive(Debug)]
struct ScaffoldSource;

impl fmt::Display for ScaffoldSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("scaffold source")
    }
}

impl Error for ScaffoldSource {}
