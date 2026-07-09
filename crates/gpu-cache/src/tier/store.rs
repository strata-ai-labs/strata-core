//! The store-of-record seam (T2).
//!
//! The tier owns page-id assignment; the store persists. Writes are
//! **batched commits** (design §6): the write-behind queue drains into
//! `commit_batch`, which lands the batch's pages *and* the watermark row in
//! one atomic commit and returns the receipt — `flush()`'s durability point
//! is exactly the last such receipt. Geometry lives in a manifest row,
//! validated on reopen (design §10).
//!
//! [`InMemoryStore`] is the machinery-test fake; `EnginePageStore` (GT2)
//! implements the same seam over engine-next's public surface.

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
    /// Metadata tags (layer, sequence, position range — the tier stores,
    /// never interprets; `topk` filters match them exactly).
    pub tags: [u64; 4],
    /// Graph neighbors (bounded by the tier's adjacency degree). Persisted
    /// with the page; mirrored into the device adjacency table while
    /// resident. (Mirroring into the engine graph capability for
    /// auditability is deferred — the meta row is the source of truth.)
    pub edges: Vec<PageId>,
}

/// Tier geometry as persisted in the store's manifest row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TierManifest {
    /// Page blob size in bytes.
    pub page_bytes: u64,
    /// Summary blob size in bytes.
    pub summary_bytes: u64,
}

/// Receipt of one durable batch commit (the flush durability point).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitReceipt {
    /// Store commit version.
    pub version: u64,
    /// Store commit timestamp (microseconds).
    pub timestamp: u64,
}

/// What the tier requires of the store of record.
pub trait PageStore {
    /// Reads a batch of pages. Absent ids yield `None` in place — a miss is
    /// a degradation signal, never an error.
    fn read_pages(&mut self, ids: &[PageId]) -> Result<Vec<Option<PageBlob>>, GpuError>;

    /// Persists a batch of pages plus the new watermark in **one atomic
    /// commit**, returning its receipt.
    fn commit_batch(
        &mut self,
        entries: &[(PageId, PageBlob)],
        watermark: PageId,
    ) -> Result<CommitReceipt, GpuError>;

    /// Reads the geometry manifest, if this store was initialized before.
    fn load_manifest(&mut self) -> Result<Option<TierManifest>, GpuError>;

    /// Writes the geometry manifest (first open).
    fn write_manifest(&mut self, manifest: TierManifest) -> Result<(), GpuError>;

    /// Highest durably committed page id, if any pages were ever committed.
    fn watermark(&mut self) -> Result<Option<PageId>, GpuError>;
}

/// In-memory store fake for machinery tests.
#[derive(Debug, Default)]
pub struct InMemoryStore {
    pages: HashMap<PageId, PageBlob>,
    manifest: Option<TierManifest>,
    watermark: Option<PageId>,
    commit_counter: u64,
    /// Fault knob: fail the next N read batches.
    fail_reads: Cell<u32>,
    /// Fault knob: fail the next N batch commits.
    fail_commits: u32,
}

impl InMemoryStore {
    /// Creates an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seeds a page with a chosen id (test convenience; also advances the
    /// watermark like a durable commit would have).
    pub fn seed(&mut self, id: PageId, blob: PageBlob) {
        self.watermark = Some(self.watermark.map_or(id, |w| PageId(w.0.max(id.0))));
        self.pages.insert(id, blob);
    }

    /// Makes the next `count` read batches fail.
    pub fn fail_next_reads(&mut self, count: u32) {
        self.fail_reads.set(count);
    }

    /// Makes the next `count` batch commits fail.
    pub fn fail_next_commits(&mut self, count: u32) {
        self.fail_commits = count;
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

    /// Durable batch commits so far (telemetry oracle).
    #[must_use]
    pub const fn commits(&self) -> u64 {
        self.commit_counter
    }
}

impl PageStore for InMemoryStore {
    fn read_pages(&mut self, ids: &[PageId]) -> Result<Vec<Option<PageBlob>>, GpuError> {
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

    fn commit_batch(
        &mut self,
        entries: &[(PageId, PageBlob)],
        watermark: PageId,
    ) -> Result<CommitReceipt, GpuError> {
        if self.fail_commits > 0 {
            self.fail_commits -= 1;
            return Err(GpuError::DriverCall {
                call: "store.commit_batch",
                code: -1,
                detail: "injected commit failure".to_owned(),
            });
        }
        for (id, blob) in entries {
            self.pages.insert(*id, blob.clone());
        }
        self.watermark = Some(
            self.watermark
                .map_or(watermark, |w| PageId(w.0.max(watermark.0))),
        );
        self.commit_counter += 1;
        Ok(CommitReceipt {
            version: self.commit_counter,
            timestamp: self.commit_counter * 1000,
        })
    }

    fn load_manifest(&mut self) -> Result<Option<TierManifest>, GpuError> {
        Ok(self.manifest)
    }

    fn write_manifest(&mut self, manifest: TierManifest) -> Result<(), GpuError> {
        self.manifest = Some(manifest);
        Ok(())
    }

    fn watermark(&mut self) -> Result<Option<PageId>, GpuError> {
        Ok(self.watermark)
    }
}
