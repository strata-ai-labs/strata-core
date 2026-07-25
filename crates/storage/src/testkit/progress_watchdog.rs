//! TCP5.1 progress watchdog: converts a hung or livelocked test lane into a
//! fast, diagnosable failure.
//!
//! The seed incident is #2792: the nightly stress lane livelocked and the
//! only detector was the 4-hour CI job timeout — hours of silence, no
//! diagnosis, the whole job budget burned. This watchdog is the in-test
//! liveness budget: workers tick a shared counter as they make progress; a
//! monitor thread samples it, and when a full stall window passes without a
//! single tick it prints a diagnosis and **aborts the process**. Abort, not
//! panic — a panicking monitor thread cannot fail a test whose main thread
//! is hung; killing the process is the only signal that always escapes.
//!
//! What the diagnosis contains: the stall duration, total ticks before the
//! stall, the lane's own context dump (per-session counters and whatever
//! else the lane closes over), and the process CPU-time delta across the
//! stall window — the single most useful discriminator, because a dead hang
//! shows flat CPU while a livelock burns it (#2792's signature was ~3,000
//! background task completions/s on an idle store).
//!
//! What this does NOT catch: a *crawl* that still ticks occasionally. A
//! tick-rate floor would be hardware-dependent and flap; the crawl class is
//! bounded by the CI step budget (`timeout-minutes` per step), which is the
//! other half of TCP5.1.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Environment override for the stall window, in seconds.
const STALL_SECONDS_ENV: &str = "STRATA_PROGRESS_STALL_SECONDS";

/// Cloneable handle a worker uses to report liveness.
#[derive(Clone, Debug)]
pub struct ProgressTicker {
    progress: Arc<AtomicU64>,
}

impl ProgressTicker {
    pub fn tick(&self) {
        self.progress.fetch_add(1, Ordering::Relaxed);
    }
}

/// Armed monitor; dropping it disarms the monitor thread.
#[derive(Debug)]
pub struct ProgressWatchdog {
    progress: Arc<AtomicU64>,
    disarmed: Arc<AtomicBool>,
}

impl ProgressWatchdog {
    /// Arm a watchdog for `label` with the given stall window (overridable
    /// via `STRATA_PROGRESS_STALL_SECONDS`). `context` runs on the monitor
    /// thread at stall time — close over `Arc`'d counters, never borrows.
    pub fn arm(
        label: &'static str,
        stall_window: Duration,
        context: impl Fn() -> String + Send + 'static,
    ) -> Self {
        let stall_window = std::env::var(STALL_SECONDS_ENV)
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .map_or(stall_window, Duration::from_secs);
        let progress = Arc::new(AtomicU64::new(0));
        let disarmed = Arc::new(AtomicBool::new(false));
        let monitor_progress = Arc::clone(&progress);
        let monitor_disarmed = Arc::clone(&disarmed);
        std::thread::Builder::new()
            .name(format!("progress-watchdog-{label}"))
            .spawn(move || {
                monitor(
                    label,
                    stall_window,
                    &monitor_progress,
                    &monitor_disarmed,
                    &context,
                );
            })
            .expect("spawn watchdog monitor thread");
        Self { progress, disarmed }
    }

    #[must_use]
    pub fn ticker(&self) -> ProgressTicker {
        ProgressTicker {
            progress: Arc::clone(&self.progress),
        }
    }
}

impl Drop for ProgressWatchdog {
    fn drop(&mut self) {
        self.disarmed.store(true, Ordering::Release);
    }
}

/// Sampling cadence for a stall window: a quarter-window, floored so tiny
/// test windows cannot busy-spin the monitor.
fn stall_poll_interval(stall_window: Duration) -> Duration {
    (stall_window / 4).max(Duration::from_millis(10))
}

/// The stall decision, pure so the boundary is pinned by a truth table: a
/// lane is stalled once a full window has elapsed with no tick.
fn stall_window_elapsed(since_last_tick: Duration, stall_window: Duration) -> bool {
    since_last_tick >= stall_window
}

fn monitor(
    label: &'static str,
    stall_window: Duration,
    progress: &AtomicU64,
    disarmed: &AtomicBool,
    context: &(impl Fn() -> String + Send),
) {
    let poll = stall_poll_interval(stall_window);
    let mut last_count = progress.load(Ordering::Relaxed);
    let mut last_change = Instant::now();
    let mut cpu_at_last_change = process_cpu_ticks();
    loop {
        std::thread::sleep(poll);
        if disarmed.load(Ordering::Acquire) {
            return;
        }
        let count = progress.load(Ordering::Relaxed);
        if count != last_count {
            last_count = count;
            last_change = Instant::now();
            cpu_at_last_change = process_cpu_ticks();
            continue;
        }
        if !stall_window_elapsed(last_change.elapsed(), stall_window) {
            continue;
        }
        let cpu_delta = process_cpu_ticks().saturating_sub(cpu_at_last_change);
        eprintln!(
            "\n==== PROGRESS WATCHDOG: lane '{label}' stalled ====\n\
             no progress tick for {:?} (window {:?}); {last_count} ticks before the stall\n\
             process CPU during the stall: {cpu_delta} ticks — flat CPU means a dead hang \
             (lock/IO wait), burning CPU means a livelock (the #2792 class)\n\
             {}\n\
             ==== aborting so the lane fails now instead of at the job timeout ====",
            last_change.elapsed(),
            stall_window,
            context(),
        );
        std::process::abort();
    }
}

/// Process-wide utime+stime in clock ticks from `/proc/self/stat`; zero on
/// non-Linux or on any parse failure (the diagnosis degrades, never panics).
fn process_cpu_ticks() -> u64 {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/self/stat")
            .ok()
            .and_then(|stat| parse_proc_stat_cpu_ticks(&stat))
            .unwrap_or(0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

/// utime + stime (fields 14/15, 1-indexed) from a `/proc/<pid>/stat` line.
/// The parenthesized comm may itself contain spaces and parens — fields are
/// counted after the LAST `") "`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_proc_stat_cpu_ticks(stat: &str) -> Option<u64> {
    let after_comm = stat.rsplit(") ").next()?;
    let mut fields = after_comm.split_whitespace();
    let utime = fields.nth(11)?.parse::<u64>().ok()?;
    let stime = fields.next()?.parse::<u64>().ok()?;
    Some(utime + stime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stall_decision_boundary_truth_table() {
        let window = Duration::from_secs(1);
        // Below the window: not stalled (kills the inverted comparisons).
        assert!(!stall_window_elapsed(Duration::from_millis(500), window));
        // Exactly the window: stalled (kills >).
        assert!(stall_window_elapsed(window, window));
        // Past the window: stalled (kills ==).
        assert!(stall_window_elapsed(Duration::from_secs(2), window));
    }

    #[test]
    fn poll_interval_is_a_floored_quarter_window() {
        assert_eq!(
            stall_poll_interval(Duration::from_secs(120)),
            Duration::from_secs(30)
        );
        // The floor keeps tiny test windows from busy-spinning the monitor.
        assert_eq!(
            stall_poll_interval(Duration::from_millis(20)),
            Duration::from_millis(10)
        );
    }

    #[test]
    fn proc_stat_parser_sums_utime_and_stime_after_a_hostile_comm() {
        // comm contains spaces and a closing paren; utime=1714 stime=286.
        let stat = "12345 (a (weird) name) S 1 12345 12345 0 -1 4194304 \
                    100 0 0 0 1714 286 0 0 20 0 8 0 555 100000 200 400 300";
        assert_eq!(parse_proc_stat_cpu_ticks(stat), Some(2000));
        assert_eq!(parse_proc_stat_cpu_ticks("garbage"), None);
    }

    #[test]
    fn process_cpu_ticks_grow_when_the_process_burns_cpu() {
        let before = process_cpu_ticks();
        // Burn well over a scheduler tick (10ms at 100Hz) of pure CPU.
        let start = Instant::now();
        let mut sink = 0u64;
        while start.elapsed() < Duration::from_millis(150) {
            sink = sink.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        }
        std::hint::black_box(sink);
        let after = process_cpu_ticks();
        // Kills the constant-return mutants: the counter must move, and by
        // more than one tick. Other tests' CPU only pushes it further up.
        assert!(
            after >= before + 2,
            "cpu ticks did not grow: before={before} after={after}"
        );
    }

    #[test]
    fn a_quiet_gap_shorter_than_the_window_does_not_fire() {
        // One tick, then silence for several polls but less than the window:
        // the monitor must keep waiting, not abort (an abort here kills the
        // whole test process, so a miswired decision cannot pass unnoticed).
        let watchdog = ProgressWatchdog::arm("short-gap", Duration::from_millis(500), String::new);
        watchdog.ticker().tick();
        std::thread::sleep(Duration::from_millis(300));
        drop(watchdog);
    }

    #[test]
    fn ticking_keeps_the_watchdog_quiet_and_drop_disarms() {
        let watchdog = ProgressWatchdog::arm("quiet", Duration::from_millis(80), String::new);
        let ticker = watchdog.ticker();
        let start = Instant::now();
        // Tick well past several stall windows: liveness must keep it quiet.
        while start.elapsed() < Duration::from_millis(300) {
            ticker.tick();
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(watchdog);
        // A disarmed monitor must not fire even after a silent window.
        std::thread::sleep(Duration::from_millis(200));
    }

    /// Child half of the abort test: arms a watchdog and never ticks.
    /// Only meaningful when re-invoked by `stalled_lane_aborts_with_a_diagnosis`.
    #[test]
    #[ignore = "re-invoked as a subprocess by the abort test"]
    fn helper_stalled_lane_never_ticks() {
        if std::env::var("STRATA_WATCHDOG_ABORT_CHILD").is_err() {
            return;
        }
        let _watchdog = ProgressWatchdog::arm("stalled", Duration::from_millis(100), || {
            "context: 3 sessions at ops [7, 7, 7]".to_string()
        });
        std::thread::sleep(Duration::from_secs(60));
    }

    #[test]
    fn stalled_lane_aborts_with_a_diagnosis() {
        let exe = std::env::current_exe().expect("current_exe");
        let output = std::process::Command::new(exe)
            .args([
                "--exact",
                "testkit::progress_watchdog::tests::helper_stalled_lane_never_ticks",
                "--ignored",
                "--nocapture",
            ])
            .env("STRATA_WATCHDOG_ABORT_CHILD", "1")
            .output()
            .expect("run stalled child");
        assert!(
            !output.status.success(),
            "a stalled lane must not exit clean: {output:?}"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("PROGRESS WATCHDOG: lane 'stalled' stalled"),
            "missing stall banner:\n{stderr}"
        );
        assert!(
            stderr.contains("sessions at ops [7, 7, 7]"),
            "missing lane context dump:\n{stderr}"
        );
    }
}
