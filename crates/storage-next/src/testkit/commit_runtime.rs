//! Generated commit-runtime scaffold contract helpers.

use crate::commit::{
    CommitDurabilityClass, CommitLowerLayer, CommitPhase, CommitReadOnlyDiagnostics,
    CommitRuntimeConfig, CommitRuntimeError, CommitRuntimeStats, CommitVisibilityFacts,
};
use std::error::Error;
use std::fmt;
use strata_core_next::CommitVersion;

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

fn script_byte(script: &[u8], index: usize) -> u8 {
    script.get(index).copied().unwrap_or(0)
}

#[derive(Debug)]
struct ScaffoldSource;

impl fmt::Display for ScaffoldSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("scaffold source")
    }
}

impl Error for ScaffoldSource {}
