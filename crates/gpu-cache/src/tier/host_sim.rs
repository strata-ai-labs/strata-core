//! Host-simulation backend: the tier's machinery on plain host memory.
//!
//! Everything the tier does — promotion, fencing, eviction, backpressure —
//! runs against this backend in ordinary CI with no GPU, deterministically:
//! copy completion is a counter the *test* advances, so interleavings that
//! would be racy timing on real hardware become explicit test steps
//! (mirroring storage-next's fault-injection discipline).
//!
//! Fault knobs:
//! - [`HostSimBackend::fail_next_copies`] — upcoming copies error at enqueue
//!   (a promotion batch that never starts).
//! - [`HostSimBackend::hold_completions`] — enqueued copies stay incomplete
//!   until [`HostSimBackend::complete_pending`] releases them; held copies'
//!   bytes are not visible early (models real staging).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::tier::backend::{CopyFence, DeviceBackend, Region, RegionBytes};
use crate::GpuError;

/// Shared completion state: a copy with ticket `t` is complete when
/// `released >= t`.
#[derive(Debug, Default)]
struct CompletionState {
    issued: u64,
    released: u64,
    hold: bool,
}

/// Fence for the host-sim backend: a ticket against the release counter.
#[derive(Clone, Debug)]
pub struct SimFence {
    ticket: u64,
    state: Rc<RefCell<CompletionState>>,
}

impl CopyFence for SimFence {
    fn is_complete(&self) -> bool {
        self.state.borrow().released >= self.ticket
    }
}

/// The host-sim device.
#[derive(Debug, Default)]
pub struct HostSimBackend {
    pages: Vec<u8>,
    summaries: Vec<u8>,
    adjacency: Vec<u8>,
    state: Rc<RefCell<CompletionState>>,
    fail_budget: u32,
    /// Ticket -> staged write; applied when the ticket is released so a held
    /// copy's bytes are not visible early.
    staged: BTreeMap<u64, (Region, u64, Vec<u8>)>,
    copies_enqueued: u64,
    copies_failed: u64,
}

impl HostSimBackend {
    /// Creates an empty backend; call [`DeviceBackend::reserve`] before use.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes the next `count` copies fail at enqueue.
    pub fn fail_next_copies(&mut self, count: u32) {
        self.fail_budget = count;
    }

    /// Stops auto-completing copies; they stay pending until released with
    /// [`Self::complete_pending`]. Turning hold off releases everything.
    pub fn hold_completions(&mut self, hold: bool) {
        {
            self.state.borrow_mut().hold = hold;
        }
        if !hold {
            let issued = self.state.borrow().issued;
            self.release_up_to(issued);
        }
    }

    /// Releases up to `count` held copies (oldest first), applying their
    /// bytes. Returns how many were released.
    pub fn complete_pending(&mut self, count: u64) -> u64 {
        let (before, target) = {
            let state = self.state.borrow();
            (state.released, state.issued.min(state.released + count))
        };
        self.release_up_to(target);
        target - before
    }

    fn release_up_to(&mut self, target: u64) {
        loop {
            let next = {
                let mut state = self.state.borrow_mut();
                if state.released >= target {
                    break;
                }
                state.released += 1;
                state.released
            };
            if let Some((region, offset, bytes)) = self.staged.remove(&next) {
                let start = usize::try_from(offset).expect("sim offsets fit usize");
                let buffer = self.region_mut(region);
                buffer[start..start + bytes.len()].copy_from_slice(&bytes);
            }
        }
    }

    fn region_mut(&mut self, region: Region) -> &mut Vec<u8> {
        match region {
            Region::Pages => &mut self.pages,
            Region::Summaries => &mut self.summaries,
            Region::Adjacency => &mut self.adjacency,
        }
    }

    fn region_ref(&self, region: Region) -> &[u8] {
        match region {
            Region::Pages => &self.pages,
            Region::Summaries => &self.summaries,
            Region::Adjacency => &self.adjacency,
        }
    }

    /// Copies enqueued so far (telemetry oracle).
    #[must_use]
    pub const fn copies_enqueued(&self) -> u64 {
        self.copies_enqueued
    }

    /// Copies rejected by the fault knob (telemetry oracle).
    #[must_use]
    pub const fn copies_failed(&self) -> u64 {
        self.copies_failed
    }
}

impl DeviceBackend for HostSimBackend {
    type Fence = SimFence;

    fn reserve(&mut self, bytes: RegionBytes) -> Result<(), GpuError> {
        let alloc = |len: u64| -> Result<Vec<u8>, GpuError> {
            usize::try_from(len)
                .map(|len| vec![0u8; len])
                .map_err(|_| GpuError::InvalidConfig {
                    detail: format!("sim region of {len} bytes exceeds host address width"),
                })
        };
        self.pages = alloc(bytes.pages)?;
        self.summaries = alloc(bytes.summaries)?;
        self.adjacency = alloc(bytes.adjacency)?;
        Ok(())
    }

    fn copy_in(
        &mut self,
        region: Region,
        offset: u64,
        bytes: &[u8],
    ) -> Result<Self::Fence, GpuError> {
        if self.fail_budget > 0 {
            self.fail_budget -= 1;
            self.copies_failed += 1;
            return Err(GpuError::DriverCall {
                call: "sim.copy_in",
                code: -1,
                detail: "injected copy failure".to_owned(),
            });
        }
        let end = offset + bytes.len() as u64;
        if end > self.region_ref(region).len() as u64 {
            return Err(GpuError::InvalidConfig {
                detail: format!(
                    "copy past region end ({end} > {})",
                    self.region_ref(region).len()
                ),
            });
        }
        self.copies_enqueued += 1;
        let (ticket, hold) = {
            let mut state = self.state.borrow_mut();
            state.issued += 1;
            (state.issued, state.hold)
        };
        self.staged.insert(ticket, (region, offset, bytes.to_vec()));
        if !hold {
            self.release_up_to(ticket);
        }
        Ok(SimFence {
            ticket,
            state: Rc::clone(&self.state),
        })
    }

    fn fence_now(&mut self) -> Result<Self::Fence, GpuError> {
        let ticket = self.state.borrow().issued;
        Ok(SimFence {
            ticket,
            state: Rc::clone(&self.state),
        })
    }

    fn read_back(&mut self, region: Region, offset: u64, len: usize) -> Result<Vec<u8>, GpuError> {
        let start = usize::try_from(offset).map_err(|_| GpuError::InvalidConfig {
            detail: "offset exceeds host address width".to_owned(),
        })?;
        let buffer = self.region_ref(region);
        if start + len > buffer.len() {
            return Err(GpuError::InvalidConfig {
                detail: format!("read past region end ({} > {})", start + len, buffer.len()),
            });
        }
        Ok(buffer[start..start + len].to_vec())
    }
}
