//! Swappable synchronization seam for concurrency-schedule exploration
//! (TCP4.3).
//!
//! Modules whose interleavings are model-checked import their primitives
//! from here instead of `std::sync`. A normal build re-exports std — the
//! seam is zero-cost. Under `RUSTFLAGS="--cfg loom"` the same names
//! resolve to [loom](https://docs.rs/loom) primitives, so the in-crate
//! `#[cfg(all(loom, test))]` models explore every schedule of the real
//! protocol code, not a hand-copied abstraction.
//!
//! Two deliberate semantic notes for the loom side:
//!
//! - loom's `Condvar::wait_timeout` never times out (upstream TODO): timed
//!   fallback branches are unreachable under exploration. That is the
//!   correct posture for verification — a schedule that NEEDS a timeout to
//!   make progress is a lost-wakeup bug, and loom reports it as a detected
//!   deadlock instead of letting the fallback mask it.
//! - [`beat_pause`] (a pure batching heuristic in the product) becomes a
//!   scheduler yield: wall-clock pauses are schedule-irrelevant under
//!   model checking.

#[cfg(not(loom))]
pub(crate) use std::sync::{Arc, Condvar, Mutex};

#[cfg(loom)]
pub(crate) use loom::sync::{Arc, Condvar, Mutex};

use std::time::Duration;

/// Batching-beat pause: real sleep in production, scheduler yield under
/// loom (time is not modeled; the beat is a throughput heuristic with no
/// correctness role).
pub(crate) fn beat_pause(duration: Duration) {
    #[cfg(not(loom))]
    std::thread::sleep(duration);
    #[cfg(loom)]
    {
        let _ = duration;
        loom::thread::yield_now();
    }
}

/// Bounded deadline for lost-wakeup safety nets. Real clock in production;
/// under loom it never expires — a schedule that cannot finish without the
/// deadline is a liveness bug loom must surface as a deadlock, not one the
/// net may absorb.
#[derive(Debug)]
pub(crate) struct Deadline {
    #[cfg(not(loom))]
    at: std::time::Instant,
}

impl Deadline {
    pub(crate) fn after(duration: Duration) -> Self {
        #[cfg(not(loom))]
        {
            Self {
                at: std::time::Instant::now() + duration,
            }
        }
        #[cfg(loom)]
        {
            let _ = duration;
            Self {}
        }
    }

    #[cfg_attr(
        loom,
        allow(clippy::unused_self, reason = "loom has no clock to consult")
    )]
    pub(crate) fn expired(&self) -> bool {
        #[cfg(not(loom))]
        {
            std::time::Instant::now() >= self.at
        }
        #[cfg(loom)]
        false
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::Deadline;
    use std::time::Duration;

    #[test]
    fn deadline_expiry_truth_table() {
        assert!(Deadline::after(Duration::ZERO).expired());
        assert!(!Deadline::after(Duration::from_secs(3600)).expired());
    }
}
