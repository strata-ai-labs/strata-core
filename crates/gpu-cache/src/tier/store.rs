//! The store-of-record seam (T2).
//!
//! GT1 abstracts T2 behind [`PageStore`] so the machinery is testable
//! standalone; GT2 implements it over engine-next's public surface (pages as
//! rows, edges through the graph capability). The trait is deliberately
//! batch-shaped: promotion reads in batches, and the engine's batch APIs are
//! the intended implementation.

use std::cell::Cell;
use std::collections::HashMap;

use crate::tier::page_table::PageId;
use crate::GpuError;

/// One page's durable content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageBlob {
    /// The opaque page bytes (fixed size per tier instance).
    pub bytes: Vec<u8>,
    /// The page's summary blob (fixed size per tier instance).
    pub summary: Vec<u8>,
}

/// What the tier requires of the store of record.
pub trait PageStore {
    /// Reads a batch of pages. Absent ids yield `None` in place — a miss is
    /// a degradation signal, never an error.
    fn read_pages(&self, ids: &[PageId]) -> Result<Vec<Option<PageBlob>>, GpuError>;

    /// Persists a new page, assigning its stable id.
    fn append_page(&mut self, blob: PageBlob) -> Result<PageId, GpuError>;
}

/// In-memory store fake for GT1 machinery tests.
#[derive(Debug, Default)]
pub struct InMemoryStore {
    pages: HashMap<PageId, PageBlob>,
    next_id: u64,
    /// Fault knob: fail the next N read batches outright.
    fail_reads: Cell<u32>,
}

impl InMemoryStore {
    /// Creates an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seeds a page with a chosen id (test convenience).
    pub fn seed(&mut self, id: PageId, blob: PageBlob) {
        self.next_id = self.next_id.max(id.0 + 1);
        self.pages.insert(id, blob);
    }

    /// Makes the next `count` read batches fail.
    pub fn fail_next_reads(&mut self, count: u32) {
        self.fail_reads.set(count);
    }

    /// Number of stored pages.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pages.len()
    }

    /// True when the store holds no pages.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }
}

impl PageStore for InMemoryStore {
    fn read_pages(&self, ids: &[PageId]) -> Result<Vec<Option<PageBlob>>, GpuError> {
        let remaining = self.fail_reads.get();
        if remaining > 0 {
            self.fail_reads.set(remaining - 1);
            return Err(GpuError::DriverCall {
                call: "store.read_pages",
                code: -1,
                detail: "injected store read failure".to_owned(),
            });
        }
        Ok(ids.iter().map(|id| self.pages.get(id).cloned()).collect())
    }

    fn append_page(&mut self, blob: PageBlob) -> Result<PageId, GpuError> {
        let id = PageId(self.next_id);
        self.next_id += 1;
        self.pages.insert(id, blob);
        Ok(id)
    }
}
