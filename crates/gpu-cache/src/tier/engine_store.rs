//! [`PageStore`] over the store of record: Strata's engine (design §10).
//!
//! Pages live as KV rows in a dedicated product space of a durable-local
//! database, through engine's public surface only (the tier is a
//! consumer like the executor — never a storage importer):
//!
//! ```text
//! manifest              -> geometry (page_bytes, summary_bytes; LE u64 x2)
//! watermark             -> highest durably committed page id (BE u64)
//! page/<page_id BE u64> -> page blob bytes
//! meta/<page_id BE u64> -> summary blob bytes
//! ```
//!
//! `commit_batch` lands a whole batch — page rows, meta rows, and the
//! watermark — in **one engine commit** via `put_batch`; the returned
//! `CommitOutcome` is the tier's durability receipt. Crash semantics are
//! therefore exactly the engine's: everything up to the last receipt is
//! durable, later appends are not (bounded by the tier's backlog cap).

use std::sync::{Arc, Mutex};

use strata_engine::{
    BranchName, Database, DurableLocalOpenOptions, KvKey, KvService, KvValue, ProductSpace,
};

use crate::tier::page_table::PageId;
use crate::tier::store::{CommitReceipt, PageBlob, PageStore, TierManifest};
use crate::GpuError;

const MANIFEST_KEY: &[u8] = b"manifest";
const WATERMARK_KEY: &[u8] = b"watermark";
const PAGE_PREFIX: &[u8] = b"page/";
const META_PREFIX: &[u8] = b"meta/";
const MANIFEST_BYTES: usize = 16;

/// The engine-backed store of record.
///
/// The database handle is shared across a fork family (HT-11): forked
/// stores are branch-scoped views over one engine instance, so it sits
/// behind a lock with the tier's guard discipline — locked per operation,
/// one thread drives a family.
pub struct EnginePageStore {
    database: Arc<Mutex<Database>>,
    branch: BranchName,
    space: ProductSpace,
}

impl EnginePageStore {
    /// Opens (or creates) a durable-local database at `path` and binds the
    /// tier's rows to `space` on the default branch.
    pub fn open(path: impl Into<std::path::PathBuf>, space: &str) -> Result<Self, GpuError> {
        let outcome = Database::open_local(path, DurableLocalOpenOptions::new())
            .map_err(store_error("open_local"))?;
        Self::from_database(outcome.into_database(), space)
    }

    /// Opens the database and binds the tier's rows to `space` on a named
    /// branch — reopening a forked branch of record (HT-11c). The branch
    /// must exist; the first read (the tier's manifest check at open)
    /// refuses otherwise.
    pub fn open_on_branch(
        path: impl Into<std::path::PathBuf>,
        space: &str,
        branch: &str,
    ) -> Result<Self, GpuError> {
        let mut store = Self::open(path, space)?;
        store.branch = BranchName::new(branch).map_err(store_error("branch_name"))?;
        Ok(store)
    }

    /// Wraps an already-open database handle (the embedding case: the tier
    /// shares the application's database).
    pub fn from_database(database: Database, space: &str) -> Result<Self, GpuError> {
        let branch = database.default_branch().clone();
        let space = ProductSpace::new(space).map_err(store_error("space"))?;
        Ok(Self {
            database: Arc::new(Mutex::new(database)),
            branch,
            space,
        })
    }

    /// Forks the branch of record (HT-11c): creates `branch` from this
    /// branch's current head, so the child sees every page durable here at
    /// the fork point — manifest, page rows, and watermark all travel with
    /// the branch — and diverges after. Pass the returned store to
    /// `Tier::fork`; the tier's flushed-parent refusal guarantees the fork
    /// point covers the whole working set.
    pub fn fork(&self, branch: &str) -> Result<Self, GpuError> {
        let name = BranchName::new(branch).map_err(store_error("fork_branch_name"))?;
        {
            let mut database = self.database.lock().expect("database lock poisoned");
            let mut branches = database.branches().map_err(store_error("branches"))?;
            branches
                .fork_current(&self.branch, name.clone())
                .map_err(store_error("fork_current"))?;
        }
        Ok(Self {
            database: Arc::clone(&self.database),
            branch: name,
            space: self.space.clone(),
        })
    }

    /// The branch of record this store reads and writes.
    #[must_use]
    pub fn branch(&self) -> &BranchName {
        &self.branch
    }

    /// Closes the underlying database (flushes engine state). The handle
    /// is family-shared: closing through any store closes them all;
    /// repeat closes are idempotent.
    pub fn close(&mut self) -> Result<(), GpuError> {
        let mut database = self.database.lock().expect("database lock poisoned");
        database.close().map_err(store_error("close"))?;
        Ok(())
    }

    /// Runs one KV operation on this store's branch and space, holding the
    /// family database lock for exactly that operation.
    fn with_kv<T>(
        &mut self,
        f: impl FnOnce(&mut KvService<'_>) -> Result<T, GpuError>,
    ) -> Result<T, GpuError> {
        let mut database = self.database.lock().expect("database lock poisoned");
        let mut kv = database
            .kv(self.branch.clone(), self.space.clone())
            .map_err(store_error("kv_service"))?;
        f(&mut kv)
    }

    fn get_row(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, GpuError> {
        let key = kv_key(key)?;
        self.with_kv(|kv| {
            let value = kv.get(&key).map_err(store_error("get"))?;
            Ok(value.map(KvValue::into_bytes))
        })
    }
}

impl<B: crate::tier::backend::DeviceBackend> crate::tier::tier::Tier<B, EnginePageStore> {
    /// The canonical fork call for engine-backed tiers: forks the handle
    /// and its branch of record in one step. Refuses an unflushed parent
    /// *before* creating the branch, so a refusal leaves no orphaned
    /// branch behind — the two-step form (`tier.fork(store.fork(..))`)
    /// creates the branch first and would strand it on refusal. The
    /// generic `Tier::fork(store)` remains the machinery seam for test
    /// backends.
    pub fn fork_branch(&self, branch: &str) -> Result<Self, GpuError> {
        if self.write_backlog() > 0 {
            return Err(GpuError::ForkUnflushed {
                queued: self.write_backlog(),
            });
        }
        self.fork(self.store().fork(branch)?)
    }
}

fn store_error(operation: &'static str) -> impl FnOnce(strata_engine::EngineError) -> GpuError {
    move |error| GpuError::Store {
        operation,
        detail: error.to_string(),
    }
}

/// Meta row layout (self-describing):
/// `summary_len (u32 LE) || summary || tags (4 x u64 LE) ||
/// edge_count (u16 LE) || edges (u64 LE each)`.
fn encode_meta(blob: &PageBlob) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(4 + blob.summary.len() + 32 + 2 + blob.edges.len() * 8);
    bytes.extend_from_slice(
        &u32::try_from(blob.summary.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    bytes.extend_from_slice(&blob.summary);
    for tag in &blob.tags {
        bytes.extend_from_slice(&tag.to_le_bytes());
    }
    bytes.extend_from_slice(
        &u16::try_from(blob.edges.len())
            .unwrap_or(u16::MAX)
            .to_le_bytes(),
    );
    for edge in &blob.edges {
        bytes.extend_from_slice(&edge.0.to_le_bytes());
    }
    bytes
}

/// Inverse of [`encode_meta`]; `None` on any structural inconsistency (the
/// caller treats it as a miss and degrades rather than serving half a page).
fn decode_meta(bytes: &[u8]) -> Option<(Vec<u8>, [u64; 4], Vec<PageId>)> {
    let summary_len =
        usize::try_from(u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?)).ok()?;
    let mut at = 4;
    let summary = bytes.get(at..at + summary_len)?.to_vec();
    at += summary_len;
    let mut tags = [0u64; 4];
    for tag in &mut tags {
        *tag = u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?);
        at += 8;
    }
    let edge_count = usize::from(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
    at += 2;
    let mut edges = Vec::with_capacity(edge_count);
    for _ in 0..edge_count {
        edges.push(PageId(u64::from_le_bytes(
            bytes.get(at..at + 8)?.try_into().ok()?,
        )));
        at += 8;
    }
    (at == bytes.len()).then_some((summary, tags, edges))
}

fn kv_key(bytes: &[u8]) -> Result<KvKey, GpuError> {
    KvKey::new(bytes).map_err(store_error("key"))
}

fn page_key(id: PageId) -> Result<KvKey, GpuError> {
    let mut bytes = Vec::with_capacity(PAGE_PREFIX.len() + 8);
    bytes.extend_from_slice(PAGE_PREFIX);
    bytes.extend_from_slice(&id.0.to_be_bytes());
    kv_key(&bytes)
}

fn meta_key(id: PageId) -> Result<KvKey, GpuError> {
    let mut bytes = Vec::with_capacity(META_PREFIX.len() + 8);
    bytes.extend_from_slice(META_PREFIX);
    bytes.extend_from_slice(&id.0.to_be_bytes());
    kv_key(&bytes)
}

impl PageStore for EnginePageStore {
    fn read_pages(&mut self, ids: &[PageId]) -> Result<Vec<Option<PageBlob>>, GpuError> {
        // One engine batch read for pages and summaries together.
        let mut keys = Vec::with_capacity(ids.len() * 2);
        for id in ids {
            keys.push(page_key(*id)?);
            keys.push(meta_key(*id)?);
        }
        let rows = self.with_kv(|kv| kv.batch_get(&keys).map_err(store_error("batch_get")))?;
        let mut blobs = Vec::with_capacity(ids.len());
        for pair in rows.chunks_exact(2) {
            let blob = match (&pair[0], &pair[1]) {
                (Some(page), Some(meta)) => {
                    decode_meta(meta.value().as_bytes()).map(|(summary, tags, edges)| PageBlob {
                        bytes: page.value().as_bytes().to_vec(),
                        summary,
                        tags,
                        edges,
                    })
                }
                // A page without its meta row (or vice versa) cannot happen
                // through commit_batch; treat any asymmetry as a miss and
                // let the caller degrade rather than serve half a page.
                _ => None,
            };
            blobs.push(blob);
        }
        Ok(blobs)
    }

    fn commit_batch(
        &mut self,
        entries: &[(PageId, PageBlob)],
        watermark: PageId,
    ) -> Result<CommitReceipt, GpuError> {
        let mut rows: Vec<(KvKey, KvValue)> = Vec::with_capacity(entries.len() * 2 + 1);
        for (id, blob) in entries {
            rows.push((page_key(*id)?, KvValue::new(blob.bytes.clone())));
            rows.push((meta_key(*id)?, KvValue::new(encode_meta(blob))));
        }
        rows.push((
            kv_key(WATERMARK_KEY)?,
            KvValue::new(watermark.0.to_be_bytes().to_vec()),
        ));
        let outcome = self.with_kv(|kv| kv.put_batch(rows).map_err(store_error("put_batch")))?;
        Ok(CommitReceipt {
            version: outcome.version().as_u64(),
            timestamp: outcome.timestamp().as_micros(),
        })
    }

    fn load_manifest(&mut self) -> Result<Option<TierManifest>, GpuError> {
        let Some(bytes) = self.get_row(MANIFEST_KEY)? else {
            return Ok(None);
        };
        if bytes.len() != MANIFEST_BYTES {
            return Err(GpuError::Store {
                operation: "load_manifest",
                detail: format!(
                    "manifest row has {} bytes, expected {MANIFEST_BYTES}",
                    bytes.len()
                ),
            });
        }
        let page_bytes = u64::from_le_bytes(bytes[0..8].try_into().expect("checked length"));
        let summary_bytes = u64::from_le_bytes(bytes[8..16].try_into().expect("checked length"));
        Ok(Some(TierManifest {
            page_bytes,
            summary_bytes,
        }))
    }

    fn write_manifest(&mut self, manifest: TierManifest) -> Result<(), GpuError> {
        let mut bytes = Vec::with_capacity(MANIFEST_BYTES);
        bytes.extend_from_slice(&manifest.page_bytes.to_le_bytes());
        bytes.extend_from_slice(&manifest.summary_bytes.to_le_bytes());
        let key = kv_key(MANIFEST_KEY)?;
        self.with_kv(|kv| {
            kv.put(key, KvValue::new(bytes))
                .map_err(store_error("write_manifest"))?;
            Ok(())
        })
    }

    fn watermark(&mut self) -> Result<Option<PageId>, GpuError> {
        let Some(bytes) = self.get_row(WATERMARK_KEY)? else {
            return Ok(None);
        };
        let raw: [u8; 8] = bytes.as_slice().try_into().map_err(|_| GpuError::Store {
            operation: "watermark",
            detail: format!("watermark row has {} bytes, expected 8", bytes.len()),
        })?;
        Ok(Some(PageId(u64::from_be_bytes(raw))))
    }
}
