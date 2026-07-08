//! BS3.4b — `RocksDB` `SetupDelay` port: debt-adaptive write-rate ramp + token bucket.
//!
//! The pure mechanism behind graded write admission — the DEFAULT since the BS3.4c bake-off
//! (2026-07-07: graded won every cell; `STRATA_ADMISSION=legacy` is the escape hatch until the
//! readiness-hardening milestone removes the legacy path). No I/O and no clock-of-record — callers pass the branch shape, the previous
//! rate/debt, and a `MaintenanceInstant`, so the whole thing is deterministically unit-testable.
//!
//! Reference: `docs/architecture/storage-next/durable-write-pipeline-scaling.md:175-188`
//! (`db/column_family.cc` `SetupDelay`, `db/write_controller.cc` token bucket).

use std::sync::OnceLock;
use std::time::Duration;

use super::background::MaintenanceInstant;

/// Which write-admission delay-band mechanism the durable runtime uses. `Legacy` is the quadratic
/// P-controller; `Graded` is BS3.4b's debt-adaptive rate ramp — the default since the BS3.4c
/// bake-off (see `billion-scale-ledger.md` § Next-levers: graded won every cell, stall-wall
/// preserved, small-budget improved).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LifecycleAdmissionMode {
    Legacy,
    Graded,
}

/// The admission mode seeded once from the `STRATA_ADMISSION` env var. `Graded` is the default
/// (BS3.4c decision); `legacy` selects the quadratic P-controller as the escape hatch until the
/// readiness-hardening milestone retires it. Cached in a `OnceLock` so it is read exactly once per process; tests select the
/// mode via a runtime override (`with_admission_mode_for_test`) rather than this global, to avoid
/// cross-test coupling.
pub(crate) fn admission_mode_from_env() -> LifecycleAdmissionMode {
    static MODE: OnceLock<LifecycleAdmissionMode> = OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("STRATA_ADMISSION").ok().as_deref() {
        Some("legacy") => LifecycleAdmissionMode::Legacy,
        _ => LifecycleAdmissionMode::Graded,
    })
}

/// The compaction debt that drives the ramp: bytes still needing compaction. L0 has no byte target
/// (`targets[0] == 0`, per `nonzero_level_targets_from_level_bytes`), so all L0 bytes count; non-zero
/// levels contribute only bytes over their target. This mirrors `RocksDB`'s
/// `estimated_compaction_needed_bytes` — the signal that grows when compaction falls behind.
pub(crate) fn compaction_debt(per_level_bytes: &[u64], targets: &[u64]) -> u64 {
    per_level_bytes
        .iter()
        .zip(targets)
        .map(|(&bytes, &target)| bytes.saturating_sub(target))
        .fold(0u64, u64::saturating_add)
}

// SetupDelay rate multipliers (numerator/denominator). Adopted verbatim from `RocksDB` (decade-tuned).
const RATE_GROWING_NUM: u64 = 8; // ×0.8 when debt is flat or growing
const RATE_GROWING_DEN: u64 = 10;
const RATE_SHRINKING_NUM: u64 = 5; // ×1.25 when debt is shrinking (compaction catching up)
const RATE_SHRINKING_DEN: u64 = 4;
const RATE_NEAR_STOP_NUM: u64 = 3; // ×0.6 near the hard stop (brake hardest)
const RATE_NEAR_STOP_DEN: u64 = 5;
const RATE_RECOVER_NUM: u64 = 7; // ×1.4 on return to normal (accelerate back toward max)
const RATE_RECOVER_DEN: u64 = 5;
/// Within this many L0 tables of the hard stop, brake hardest (×0.6).
const NEAR_STOP_MARGIN: usize = 2;

fn scale(rate: u64, num: u64, den: u64) -> u64 {
    u64::try_from(u128::from(rate) * u128::from(num) / u128::from(den)).unwrap_or(u64::MAX)
}

/// One `SetupDelay` recomputation: the new write rate (bytes/sec) from the current rate, the debt now
/// vs. at the previous recompute, the L0 count, and the hard-stop threshold. Clamped to
/// `[floor_rate, max_rate]`. Called only at install events (event cadence), never per commit.
///
/// Precedence: near-stop braking (×0.6) overrides everything; then return-to-normal (×1.4) when the
/// debt has cleared; otherwise ×0.8 while debt is flat/growing, ×1.25 while it shrinks.
pub(crate) fn next_write_rate(
    current_rate: u64,
    debt: u64,
    last_debt: u64,
    l0_count: usize,
    stop_threshold: usize,
    max_rate: u64,
    floor_rate: u64,
) -> u64 {
    let near_stop = l0_count.saturating_add(NEAR_STOP_MARGIN) >= stop_threshold;
    let next = if near_stop {
        scale(current_rate, RATE_NEAR_STOP_NUM, RATE_NEAR_STOP_DEN)
    } else if debt == 0 {
        scale(current_rate, RATE_RECOVER_NUM, RATE_RECOVER_DEN)
    } else if debt >= last_debt {
        scale(current_rate, RATE_GROWING_NUM, RATE_GROWING_DEN)
    } else {
        scale(current_rate, RATE_SHRINKING_NUM, RATE_SHRINKING_DEN)
    };
    next.clamp(floor_rate.min(max_rate), max_rate)
}

const NANOS_PER_SEC: u128 = 1_000_000_000;
/// A charged commit never sleeps for less than this (`RocksDB`'s 1 ms floor keeps the pacing coarse
/// enough not to spin).
const MIN_DELAY: Duration = Duration::from_millis(1);

/// Token bucket enforcing the write rate per commit (`RocksDB` `db/write_controller.cc`): credit
/// accrues at `rate` bytes/sec for elapsed time; a commit that exceeds its credit sleeps
/// `bytes_over_credit / rate` (min 1 ms), pacing the aggregate write rate to `rate`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WriteRateBucket {
    credit_bytes: u64,
    last_refill: MaintenanceInstant,
}

impl WriteRateBucket {
    pub(crate) fn new(now: MaintenanceInstant) -> Self {
        Self {
            credit_bytes: 0,
            last_refill: now,
        }
    }

    /// Charge `bytes` at `rate`, first refilling credit for the time elapsed since the last charge.
    /// Returns the delay to pace this commit (`ZERO` when the commit fits within accrued credit).
    pub(crate) fn charge(&mut self, bytes: u64, rate: u64, now: MaintenanceInstant) -> Duration {
        let rate = rate.max(1);
        let elapsed = now.saturating_duration_since(self.last_refill).as_nanos();
        let refilled =
            u64::try_from(u128::from(rate) * elapsed / NANOS_PER_SEC).unwrap_or(u64::MAX);
        // Credit caps at one second of the current rate (a 1 s burst = `rate` bytes) so an idle gap
        // can't buy an unbounded unpaced burst; sustained over-rate writing is still paced to `rate`.
        let cap = rate;
        self.credit_bytes = self.credit_bytes.saturating_add(refilled).min(cap.max(1));
        self.last_refill = now;
        if bytes <= self.credit_bytes {
            self.credit_bytes -= bytes;
            return Duration::ZERO;
        }
        let over = bytes - self.credit_bytes;
        self.credit_bytes = 0;
        let nanos =
            u64::try_from(u128::from(over) * NANOS_PER_SEC / u128::from(rate)).unwrap_or(u64::MAX);
        Duration::from_nanos(nanos).max(MIN_DELAY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;
    const MAX_RATE: u64 = 16 * MIB; // `RocksDB`'s default max_delayed_write_rate
    const FLOOR: u64 = 16 * 1024; // 16 KiB/s

    fn at(millis: u64) -> MaintenanceInstant {
        MaintenanceInstant::from_elapsed(Duration::from_millis(millis))
    }

    #[test]
    fn debt_counts_all_l0_bytes_and_nonzero_over_target() {
        // targets[0] == 0 (L0), so all 5000 L0 bytes count; L1 is 30 over its 100 target; L2 (50 vs
        // 200) is under target and contributes nothing.
        let per_level = [5_000_u64, 130, 50];
        let targets = [0_u64, 100, 200];
        assert_eq!(compaction_debt(&per_level, &targets), 5_000 + 30);
    }

    #[test]
    fn debt_is_zero_when_all_levels_at_or_below_target_and_l0_empty() {
        assert_eq!(compaction_debt(&[0, 50, 50], &[0, 100, 100]), 0);
    }

    #[test]
    fn ramp_decays_geometrically_while_debt_grows() {
        // debt >= last_debt (growing): ×0.8 each recompute.
        let mut rate = MAX_RATE;
        rate = next_write_rate(rate, 100, 50, 21, 36, MAX_RATE, FLOOR);
        assert_eq!(rate, MAX_RATE * 8 / 10);
        rate = next_write_rate(rate, 200, 100, 21, 36, MAX_RATE, FLOOR);
        assert_eq!(rate, MAX_RATE * 8 / 10 * 8 / 10);
    }

    #[test]
    fn ramp_recovers_capped_at_max_while_debt_shrinks() {
        let low = MAX_RATE / 2;
        // debt shrinking (10 < 20): ×1.25, but never above max.
        assert_eq!(
            next_write_rate(low, 10, 20, 21, 36, MAX_RATE, FLOOR),
            low * 5 / 4
        );
        // near max, ×1.25 clamps to max.
        assert_eq!(
            next_write_rate(MAX_RATE * 9 / 10, 10, 20, 21, 36, MAX_RATE, FLOOR),
            MAX_RATE
        );
    }

    #[test]
    fn ramp_brakes_hardest_near_stop_regardless_of_debt_direction() {
        // l0 = stop-2 -> near-stop -> ×0.6 even though debt is shrinking.
        assert_eq!(
            next_write_rate(MAX_RATE, 10, 20, 34, 36, MAX_RATE, FLOOR),
            MAX_RATE * 3 / 5
        );
    }

    #[test]
    fn ramp_returns_to_normal_when_debt_clears() {
        // debt == 0, not near stop -> ×1.4 toward max.
        assert_eq!(
            next_write_rate(MAX_RATE / 2, 0, 100, 5, 36, MAX_RATE, FLOOR),
            MAX_RATE / 2 * 7 / 5
        );
    }

    #[test]
    fn ramp_clamps_at_floor() {
        // A tiny rate braked near stop never drops below the floor.
        assert_eq!(
            next_write_rate(FLOOR, 100, 50, 34, 36, MAX_RATE, FLOOR),
            FLOOR
        );
    }

    #[test]
    fn bucket_lets_within_rate_writes_pass_undelayed() {
        let mut bucket = WriteRateBucket::new(at(0));
        // At MAX_RATE, ~16 KiB accrues per ms. After 10 ms, ~160 KiB credit; a 100 KiB write fits.
        assert_eq!(bucket.charge(100 * 1024, MAX_RATE, at(10)), Duration::ZERO);
    }

    #[test]
    fn bucket_paces_over_credit_writes_by_overage_over_rate() {
        let mut bucket = WriteRateBucket::new(at(0));
        // No elapsed time -> no credit; a 1 MiB write at 1 MiB/s pays ~1 s (over/rate).
        let delay = bucket.charge(MIB, MIB, at(0));
        assert_eq!(delay, Duration::from_secs(1));
    }

    #[test]
    fn bucket_enforces_min_delay_floor() {
        let mut bucket = WriteRateBucket::new(at(0));
        // A 1-byte overage at MAX_RATE would be sub-ms; the 1 ms floor applies.
        let delay = bucket.charge(1, MAX_RATE, at(0));
        assert_eq!(delay, MIN_DELAY);
    }

    #[test]
    fn bucket_credit_is_capped_so_idle_cannot_buy_an_unbounded_burst() {
        let mut bucket = WriteRateBucket::new(at(0));
        // 100 s idle at 1 MiB/s would accrue 100 MiB uncapped; capped at 1 s = 1 MiB.
        // A 2 MiB write then pays for the 1 MiB overage -> ~1 s, not free.
        let delay = bucket.charge(2 * MIB, MIB, at(100_000));
        assert_eq!(delay, Duration::from_secs(1));
    }
}
