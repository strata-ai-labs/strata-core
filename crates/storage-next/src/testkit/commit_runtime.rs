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
use super::commit_runtime_outcome::check_commit_runtime_outcome_contract;
use super::TestkitError;

/// Summary of one generated commit-runtime scaffold contract check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
}

/// Runs one deterministic generated scaffold contract case for the commit runtime.
pub fn check_commit_runtime_scaffold_contract(
    script: &[u8],
) -> Result<CommitRuntimeScaffoldOutcome, TestkitError> {
    let mut outcome = CommitRuntimeScaffoldOutcome {
        valid_config: 0,
        invalid_config: 0,
        phase_facts: 0,
        visibility_facts: 0,
        invalid_visibility_facts: 0,
        error_displays: 0,
        error_sources: 0,
        stats: 0,
        source_guard_fixtures: 0,
        valid_batches: 0,
        invalid_batches: 0,
        duplicate_mutations: 0,
        branch_mismatches: 0,
        storage_owned_spaces: 0,
        invalid_fact_cases: 0,
        stamping_cases: 0,
        expiry_rejections: 0,
        stamping_rejections: 0,
        version_allocations: 0,
        version_catch_ups: 0,
        version_overflows: 0,
        generated_timestamps: 0,
        clamped_timestamps: 0,
        explicit_timestamps: 0,
        invalid_explicit_timestamps: 0,
        timestamp_source_failures: 0,
        read_only_no_allocations: 0,
        no_transaction_id_checks: 0,
        read_only_outcomes: 0,
        read_only_disabled_rejections: 0,
        visible_tracker_initializations: 0,
        visible_tracker_monotonic_publishes: 0,
        visible_tracker_regression_rejections: 0,
        outcome_invalid_visibility_facts: 0,
        outcome_constructor_rejections: 0,
        mutation_count_facts: 0,
        cross_branch_read_only_facts: 0,
        read_only_outcome_no_allocations: 0,
    };

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

    Ok(outcome)
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
