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

use crate::tier::backend::{
    CopyFence, DeviceBackend, Region, RegionBytes, TagFilter, TopkReadback,
};
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
    validity: Vec<u8>,
    tags: Vec<u8>,
    scratch: Vec<u8>,
    materialize: Vec<u8>,
    /// The most recent selection (the sim's "device scratch").
    last_topk: Option<TopkReadback>,
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
            Region::Validity => &mut self.validity,
            Region::Tags => &mut self.tags,
            Region::Scratch => &mut self.scratch,
            Region::Materialize => &mut self.materialize,
        }
    }

    fn region_ref(&self, region: Region) -> &[u8] {
        match region {
            Region::Pages => &self.pages,
            Region::Summaries => &self.summaries,
            Region::Adjacency => &self.adjacency,
            Region::Validity => &self.validity,
            Region::Tags => &self.tags,
            Region::Scratch => &self.scratch,
            Region::Materialize => &self.materialize,
        }
    }

    /// Slot capacity, derived from the validity region (one byte per slot).
    fn capacity(&self) -> usize {
        self.validity.len()
    }

    fn is_valid(&self, slot: usize) -> bool {
        self.validity.get(slot).copied() == Some(1)
    }

    fn tag_of(&self, slot: usize, index: usize) -> u64 {
        let start = slot * 32 + index * 8;
        u64::from_le_bytes(self.tags[start..start + 8].try_into().expect("tag bytes"))
    }

    fn adjacency_entry(&self, slot: usize, j: usize, degree: usize) -> u32 {
        let start = (slot * degree + j) * 4;
        u32::from_le_bytes(
            self.adjacency[start..start + 4]
                .try_into()
                .expect("adj bytes"),
        )
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
        self.validity = alloc(bytes.validity)?;
        self.tags = alloc(bytes.tags)?;
        self.scratch = alloc(bytes.scratch)?;
        self.materialize = alloc(bytes.materialize)?;
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

    /// The reference semantics the CUDA kernels are proven against
    /// (GT3 exit gate): dot-product scores over selectable+filtered slots,
    /// top-k with score-descending order and lower-slot tie-break, one-hop
    /// bounded deduplicated expansion.
    ///
    /// Reads the *applied* region bytes; on real hardware the same-lane
    /// stream order applies pending copies first — tests mixing
    /// `hold_completions` with `topk` must release before selecting.
    fn topk(
        &mut self,
        query: &[f32],
        k: u16,
        expand_budget: Option<u16>,
        filter: Option<TagFilter>,
    ) -> Result<Self::Fence, GpuError> {
        let capacity = self.capacity();
        if capacity == 0 {
            return Err(GpuError::InvalidConfig {
                detail: "topk before reserve".to_owned(),
            });
        }
        let dim = self.summaries.len() / capacity / 4;
        if query.len() != dim {
            return Err(GpuError::InvalidConfig {
                detail: format!("query has {} dims, summaries have {dim}", query.len()),
            });
        }
        let degree = self.adjacency.len() / capacity / 4;

        let mut scored: Vec<(u32, f32)> = Vec::new();
        for slot in 0..capacity {
            if !self.is_valid(slot) {
                continue;
            }
            if let Some(filter) = filter {
                if self.tag_of(slot, usize::from(filter.index)) != filter.value {
                    continue;
                }
            }
            let base = slot * dim * 4;
            let mut score = 0.0f32;
            for (i, q) in query.iter().enumerate() {
                let start = base + i * 4;
                let s =
                    f32::from_le_bytes(self.summaries[start..start + 4].try_into().expect("f32"));
                score = q.mul_add(s, score);
            }
            scored.push((u32::try_from(slot).expect("capacity fits u32"), score));
        }
        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        scored.truncate(usize::from(k));
        let selected = scored;

        let mut expanded = Vec::new();
        if let Some(budget) = expand_budget {
            let mut seen: std::collections::HashSet<u32> =
                selected.iter().map(|(slot, _)| *slot).collect();
            'outer: for &(slot, _) in &selected {
                for j in 0..degree {
                    let neighbor = self.adjacency_entry(slot as usize, j, degree);
                    if neighbor == u32::MAX || !self.is_valid(neighbor as usize) {
                        continue;
                    }
                    if seen.insert(neighbor) {
                        expanded.push(neighbor);
                        if expanded.len() >= usize::from(budget) {
                            break 'outer;
                        }
                    }
                }
            }
        }

        self.last_topk = Some(TopkReadback { selected, expanded });
        self.fence_now()
    }

    fn read_topk(&mut self) -> Result<TopkReadback, GpuError> {
        self.last_topk
            .clone()
            .ok_or_else(|| GpuError::InvalidConfig {
                detail: "read_topk before any topk".to_owned(),
            })
    }

    fn materialize_topk(&mut self) -> Result<Self::Fence, GpuError> {
        let selection = self
            .last_topk
            .clone()
            .ok_or_else(|| GpuError::InvalidConfig {
                detail: "materialize before any topk".to_owned(),
            })?;
        let capacity = self.capacity();
        let page_bytes = self.pages.len() / capacity;
        self.materialize.fill(0);
        for (i, (slot, _)) in selection.selected.iter().enumerate() {
            let src = (*slot as usize) * page_bytes;
            let dst = i * page_bytes;
            let page = self.pages[src..src + page_bytes].to_vec();
            self.materialize[dst..dst + page_bytes].copy_from_slice(&page);
        }
        self.fence_now()
    }
}
