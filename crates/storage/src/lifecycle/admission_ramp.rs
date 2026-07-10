//! BS3.4b — `RocksDB` `SetupDelay` port: debt-adaptive write-rate ramp + token bucket.
//!
//! The pure mechanism behind graded write admission — the DEFAULT since the BS3.4c bake-off
//! (2026-07-07: graded won every cell; `STRATA_ADMISSION=legacy` is the escape hatch until the
//! readiness-hardening milestone removes the legacy path). No I/O and no clock-of-record — callers pass the branch shape, the previous
//! rate/debt, and a `MaintenanceInstant`, so the whole thing is deterministically unit-testable.
//!
//! Reference: `docs/architecture/storage/durable-write-pipeline-scaling.md:175-188`
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
/// W1.4b: the debt the RAMP paces on. L0 bytes always count — writers grow
/// L0 directly and one bounded pass directly drains it, so pacing writers to
/// L0 debt is causally sound. Nonzero-level overage is STRUCTURAL debt (the
/// level shape converging at compaction's own pace); it counts only beyond
/// `soft_nonzero_debt_limit`, because braking writers cannot drain it faster —
/// measured: the ramp held workload A at ~150KB/s for a 942MB post-load
/// overhang, and pacing sleeps were 76% of the run's wall clock (ledger
/// § W1.4a). `RocksDB` analog: `soft_pending_compaction_bytes_limit` (64GB
/// default) — modest pending bytes never pace there either.
pub(crate) fn pacing_debt(
    per_level_bytes: &[u64],
    targets: &[u64],
    soft_nonzero_debt_limit: u64,
) -> u64 {
    let l0_bytes = per_level_bytes.first().copied().unwrap_or(0);
    let nonzero_over_target: u64 = per_level_bytes
        .iter()
        .zip(targets.iter())
        .skip(1)
        .map(|(bytes, target)| bytes.saturating_sub(*target))
        .sum();
    l0_bytes.saturating_add(nonzero_over_target.saturating_sub(soft_nonzero_debt_limit))
}

pub(crate) fn compaction_debt(per_level_bytes: &[u64], targets: &[u64]) -> u64 {
    per_level_bytes
        .iter()
        .zip(targets)
        .map(|(&bytes, &target)| bytes.saturating_sub(target))
        .fold(0u64, u64::saturating_add)
}

/// Within this many L0 tables of the hard stop, pace at the floor.
const NEAR_STOP_MARGIN: usize = 2;

/// W1.4b: deterministic proportional-quadratic rate from L0 depth. Below the
/// urgent count: no pacing (max rate). From urgent toward the near-stop point:
/// `min + (max-min)·h²` where `h` is the remaining headroom fraction — full
/// rate on entering the band, quadratic brake into the floor as L0 approaches
/// the blocking wall (which stays the hard backstop, unchanged).
///
/// Replaces the `SetupDelay`-style multiplicative walk (×0.8/×1.25 per install
/// event): at our recompute cadence (~1/s) and with post-load STRUCTURAL debt
/// flat by nature, the walk decayed to the floor and held a ~1KB/commit writer
/// at ~150KB/s — pacing sleeps were 76% of workload A's wall clock (ledger
/// § W1.4a); the proportional form cannot pin low while L0 has headroom.
/// Byte debt no longer drives the rate: below the soft structural limit it is
/// L0-count-correlated anyway, and beyond it compaction pressure deepens L0,
/// which this controller sees directly (`pacing_debt` stays as telemetry).
pub(crate) fn next_write_rate(
    l0_count: usize,
    urgent_threshold: usize,
    stop_threshold: usize,
    max_rate: u64,
    floor_rate: u64,
) -> u64 {
    let floor_rate = floor_rate.min(max_rate);
    if l0_count < urgent_threshold {
        return max_rate;
    }
    let stop_point = stop_threshold.saturating_sub(NEAR_STOP_MARGIN);
    if l0_count >= stop_point {
        return floor_rate;
    }
    let span = stop_point.saturating_sub(urgent_threshold).max(1) as u64;
    let headroom = stop_point.saturating_sub(l0_count) as u64; // in (0, span]
    let scaled = u128::from(max_rate - floor_rate) * u128::from(headroom) * u128::from(headroom)
        / (u128::from(span) * u128::from(span));
    floor_rate.saturating_add(u64::try_from(scaled).unwrap_or(u64::MAX))
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
    fn pacing_debt_counts_l0_fully_and_nonzero_overage_only_beyond_soft_limit() {
        // L0 = 5,000; nonzero overage = 30: under a 100-byte soft limit the
        // overage is structural noise — only L0 paces.
        let per_level = [5_000u64, 130, 0];
        let targets = [0u64, 100, 100];
        assert_eq!(pacing_debt(&per_level, &targets, 100), 5_000);
        // Overage beyond the soft limit paces by its EXCESS only.
        let per_level = [5_000u64, 700, 0];
        assert_eq!(pacing_debt(&per_level, &targets, 100), 5_000 + 500);
        // Zero soft limit degenerates to the full debt signal.
        assert_eq!(
            pacing_debt(&per_level, &targets, 0),
            compaction_debt(&per_level, &targets)
        );
    }

    #[test]
    fn debt_is_zero_when_all_levels_at_or_below_target_and_l0_empty() {
        assert_eq!(compaction_debt(&[0, 50, 50], &[0, 100, 100]), 0);
    }

    #[test]
    fn rate_is_max_below_the_urgent_band() {
        assert_eq!(next_write_rate(0, 8, 16, MAX_RATE, FLOOR), MAX_RATE);
        assert_eq!(next_write_rate(7, 8, 16, MAX_RATE, FLOOR), MAX_RATE);
    }

    #[test]
    fn rate_brakes_quadratically_through_the_band() {
        // Band: urgent=8 .. stop_point=14 (blocking 16 - margin 2), span 6.
        let at = |l0: usize| next_write_rate(l0, 8, 16, MAX_RATE, FLOOR);
        assert_eq!(at(8), MAX_RATE); // full headroom on entering the band
        let mut previous = at(8);
        for l0 in 9..14 {
            let rate = at(l0);
            assert!(rate < previous, "rate must fall monotonically: l0={l0}");
            assert!(
                rate > FLOOR,
                "rate holds above floor inside the band: l0={l0}"
            );
            previous = rate;
        }
        // Quadratic: half headroom (l0=11) => quarter of the range above floor.
        let expected = FLOOR + (MAX_RATE - FLOOR) / 4;
        let half = at(11);
        assert!(
            half.abs_diff(expected) <= (MAX_RATE - FLOOR) / 100,
            "half-headroom rate {half} must be ~quarter-range {expected}"
        );
    }

    #[test]
    fn rate_is_floor_at_and_beyond_the_near_stop_point() {
        assert_eq!(next_write_rate(14, 8, 16, MAX_RATE, FLOOR), FLOOR);
        assert_eq!(next_write_rate(15, 8, 16, MAX_RATE, FLOOR), FLOOR);
        assert_eq!(next_write_rate(40, 8, 16, MAX_RATE, FLOOR), FLOOR);
    }

    #[test]
    fn rate_clamps_floor_to_max_and_survives_degenerate_thresholds() {
        // floor > max clamps to max.
        assert_eq!(next_write_rate(20, 8, 16, FLOOR, MAX_RATE), FLOOR);
        // urgent == stop point: any in-band count is already at the floor.
        assert_eq!(next_write_rate(14, 14, 16, MAX_RATE, FLOOR), FLOOR);
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
