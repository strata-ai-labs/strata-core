//! Deterministic simulation (DST) harness.
//!
//! The seeded explorer over the production lifecycle path: under one seed it sweeps
//! client-op × maintenance-cadence × clock-advancement interleavings against the
//! faithful production scheduler (the inline executor on the real `Background`
//! logic), asserting the recovery oracle (safety) and queue progress (liveness)
//! every step. Every trajectory is a pure function of its seed, so any failure
//! replays bit-exact — the determinism guard lives in [`driver`].
//!
//! Interleaving comes from cadence (how much maintenance drains between client ops),
//! seeded clock advancement, and where each op lands — never from reordering the
//! production maintenance priority, so every schedule explored is one production can
//! actually emit. The fault/crash dimension is a separate slice.

mod driver;

use std::path::{Path, PathBuf};

use crate::testkit::TestkitError;
use driver::run_one_sim;

/// Default seed count with no case budget. A case budget lets seeds scale freely,
/// so a large soak explores many seeds, not just these.
const DEFAULT_SIM_SEEDS: u64 = 8;
/// Actions driven per seeded trajectory.
const STEP_BUDGET: usize = 48;

/// Counters describing a simulation sweep run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SimulationOutcome {
    seeds_executed: usize,
    steps_executed: usize,
    maintenance_completed: usize,
    pressure_retries_drained: usize,
    clock_advances: usize,
}

impl SimulationOutcome {
    #[must_use]
    pub const fn seeds_executed(&self) -> usize {
        self.seeds_executed
    }
    #[must_use]
    pub const fn steps_executed(&self) -> usize {
        self.steps_executed
    }
    /// Maintenance tasks the inline executor completed across the sweep
    /// (non-vacuousness: the interleaving actually ran production maintenance).
    #[must_use]
    pub const fn maintenance_completed(&self) -> usize {
        self.maintenance_completed
    }
    /// Commits that hit retryable back-pressure, drained, and resumed (the admission
    /// liveness path was exercised).
    #[must_use]
    pub const fn pressure_retries_drained(&self) -> usize {
        self.pressure_retries_drained
    }
    /// Manual-clock advances that reached the injected clock.
    #[must_use]
    pub const fn clock_advances(&self) -> usize {
        self.clock_advances
    }
}

fn case_dir(root: &Path, label: &str) -> Result<PathBuf, TestkitError> {
    let dir = root.join(label);
    std::fs::create_dir_all(&dir)
        .map_err(|err| TestkitError::new(format!("create case dir: {err}")))?;
    Ok(dir)
}

/// Sweep seeded simulation trajectories, each safety- and liveness-checked every
/// step against the recovery oracle. `case_limit` caps the number of trajectories
/// (for CI budgets); `None` runs the default seed set.
pub fn run_simulation_harness(
    root: &Path,
    case_limit: Option<usize>,
) -> Result<SimulationOutcome, TestkitError> {
    let mut outcome = SimulationOutcome::default();
    // Seeds scale with the case budget: an uncapped run covers a few seeds, a large
    // soak explores as many as the budget allows. Seeds stay `0..N` so any failure
    // replays from its printed seed.
    let seed_budget = if case_limit.is_some() {
        u64::MAX
    } else {
        DEFAULT_SIM_SEEDS
    };
    for (cases, seed) in (0..seed_budget).enumerate() {
        if case_limit.is_some_and(|limit| cases >= limit) {
            break;
        }
        let facts = run_one_sim(&case_dir(root, &format!("sim-{seed}"))?, seed, STEP_BUDGET)?;
        outcome.seeds_executed += 1;
        outcome.steps_executed += facts.steps();
        outcome.maintenance_completed += facts.maintenance_completed();
        outcome.pressure_retries_drained += facts.pressure_retries();
        outcome.clock_advances += facts.clock_advances();
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::run_simulation_harness;

    #[test]
    fn sweep_holds_and_is_non_vacuous() {
        let dir = tempfile::tempdir().expect("tmp");
        let outcome = run_simulation_harness(dir.path(), Some(12)).expect("simulation sweep");
        assert!(outcome.seeds_executed() > 0, "no seeds executed");
        assert!(outcome.steps_executed() > 0, "no steps executed");
        // Non-vacuousness: the interleaving actually completed production
        // maintenance, and the manual clock advanced.
        assert!(
            outcome.maintenance_completed() > 0,
            "no maintenance completed across the sweep: {outcome:?}"
        );
        assert!(
            outcome.clock_advances() > 0,
            "manual clock never advanced: {outcome:?}"
        );
    }
}
