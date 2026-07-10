//! Commit-runtime configuration shell.

use super::{CommitAdmissionPressureThresholds, CommitRuntimeError, CommitRuntimeResult};

const DEFAULT_MAX_MUTATIONS_PER_BATCH: usize = 4096;
const DEFAULT_MAX_VALIDATION_FACTS_PER_BATCH: usize = 8192;
const DEFAULT_MAX_COMMIT_ROWS_PER_BATCH: usize = 8192;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommitRuntimeConfig {
    max_mutations_per_batch: usize,
    max_validation_facts_per_batch: usize,
    max_commit_rows_per_batch: usize,
    read_only_diagnostics: CommitReadOnlyDiagnostics,
    admission_pressure_thresholds: CommitAdmissionPressureThresholds,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitReadOnlyDiagnostics {
    Enabled,
    Disabled,
}

impl CommitRuntimeConfig {
    pub(crate) fn new(
        max_mutations_per_batch: usize,
        max_validation_facts_per_batch: usize,
        max_commit_rows_per_batch: usize,
        read_only_diagnostics: CommitReadOnlyDiagnostics,
    ) -> CommitRuntimeResult<Self> {
        let config = Self {
            max_mutations_per_batch,
            max_validation_facts_per_batch,
            max_commit_rows_per_batch,
            read_only_diagnostics,
            admission_pressure_thresholds: CommitAdmissionPressureThresholds::disabled(),
        };
        config.validate()?;
        Ok(config)
    }

    pub(crate) fn with_admission_pressure_thresholds(
        mut self,
        thresholds: CommitAdmissionPressureThresholds,
    ) -> CommitRuntimeResult<Self> {
        thresholds.validate()?;
        self.admission_pressure_thresholds = thresholds;
        self.validate()?;
        Ok(self)
    }

    pub(crate) const fn max_mutations_per_batch(self) -> usize {
        self.max_mutations_per_batch
    }

    pub(crate) const fn max_validation_facts_per_batch(self) -> usize {
        self.max_validation_facts_per_batch
    }

    pub(crate) const fn max_commit_rows_per_batch(self) -> usize {
        self.max_commit_rows_per_batch
    }

    pub(crate) const fn read_only_diagnostics(self) -> CommitReadOnlyDiagnostics {
        self.read_only_diagnostics
    }

    pub(crate) const fn admission_pressure_thresholds(self) -> CommitAdmissionPressureThresholds {
        self.admission_pressure_thresholds
    }

    pub(crate) fn validate(self) -> CommitRuntimeResult<()> {
        if self.max_mutations_per_batch == 0 {
            return Err(CommitRuntimeError::InvalidConfig {
                field: "max_mutations_per_batch",
                reason: "must be nonzero",
            });
        }
        if self.max_validation_facts_per_batch == 0 {
            return Err(CommitRuntimeError::InvalidConfig {
                field: "max_validation_facts_per_batch",
                reason: "must be nonzero",
            });
        }
        if self.max_commit_rows_per_batch == 0 {
            return Err(CommitRuntimeError::InvalidConfig {
                field: "max_commit_rows_per_batch",
                reason: "must be nonzero",
            });
        }
        if self.max_commit_rows_per_batch < self.max_mutations_per_batch {
            return Err(CommitRuntimeError::InvalidConfig {
                field: "max_commit_rows_per_batch",
                reason: "must be greater than or equal to max_mutations_per_batch",
            });
        }
        self.admission_pressure_thresholds.validate()?;
        Ok(())
    }
}

impl Default for CommitRuntimeConfig {
    fn default() -> Self {
        Self::new(
            DEFAULT_MAX_MUTATIONS_PER_BATCH,
            DEFAULT_MAX_VALIDATION_FACTS_PER_BATCH,
            DEFAULT_MAX_COMMIT_ROWS_PER_BATCH,
            CommitReadOnlyDiagnostics::Enabled,
        )
        .expect("default commit runtime configuration is valid")
    }
}
