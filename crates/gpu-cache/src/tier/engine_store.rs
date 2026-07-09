//! [`PageStore`] over the store of record: Strata's engine (design §10).
//!
//! Pages live as KV rows in a dedicated product space of a durable-local
//! database, through engine-next's public surface only (the tier is a
//! consumer like the executor — never a storage-next importer):
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

use strata_engine_next::{
    BranchName, Database, DurableLocalOpenOptions, KvKey, KvValue, ProductSpace,
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
pub struct EnginePageStore {
    database: Database,
    branch: BranchName,
    space: ProductSpace,
}

impl EnginePageStore {
    /// Opens (or creates) a durable-local database at `path` and binds the
    /// tier's rows to `space` on the default branch.
    pub fn open(path: impl Into<std::path::PathBuf>, space: &str) -> Result<Self, GpuError> {
        let outcome = Database::open_local(path, DurableLocalOpenOptions::new())
            .map_err(store_error("open_local"))?;
        let database = outcome.into_database();
        let branch = database.default_branch().clone();
        let space = ProductSpace::new(space).map_err(store_error("space"))?;
        Ok(Self {
            database,
            branch,
            space,
        })
    }

    /// Wraps an already-open database handle (the embedding case: the tier
    /// shares the application's database).
    pub fn from_database(database: Database, space: &str) -> Result<Self, GpuError> {
        let branch = database.default_branch().clone();
        let space = ProductSpace::new(space).map_err(store_error("space"))?;
        Ok(Self {
            database,
            branch,
            space,
        })
    }

    /// Closes the underlying database (flushes engine state).
    pub fn close(&mut self) -> Result<(), GpuError> {
        self.database.close().map_err(store_error("close"))?;
        Ok(())
    }

    fn kv(&mut self) -> Result<strata_engine_next::KvService<'_>, GpuError> {
        self.database
            .kv(self.branch.clone(), self.space.clone())
            .map_err(store_error("kv_service"))
    }

    fn get_row(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, GpuError> {
        let key = kv_key(key)?;
        let mut kv = self.kv()?;
        let value = kv.get(&key).map_err(store_error("get"))?;
        Ok(value.map(KvValue::into_bytes))
    }
}

fn store_error(
    operation: &'static str,
) -> impl FnOnce(strata_engine_next::EngineError) -> GpuError {
    move |error| GpuError::Store {
        operation,
        detail: error.to_string(),
    }
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
        let mut kv = self.kv()?;
        let rows = kv.batch_get(&keys).map_err(store_error("batch_get"))?;
        let mut blobs = Vec::with_capacity(ids.len());
        for pair in rows.chunks_exact(2) {
            let blob = match (&pair[0], &pair[1]) {
                (Some(page), Some(meta)) => Some(PageBlob {
                    bytes: page.value().as_bytes().to_vec(),
                    summary: meta.value().as_bytes().to_vec(),
                }),
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
            rows.push((meta_key(*id)?, KvValue::new(blob.summary.clone())));
        }
        rows.push((
            kv_key(WATERMARK_KEY)?,
            KvValue::new(watermark.0.to_be_bytes().to_vec()),
        ));
        let mut kv = self.kv()?;
        let outcome = kv.put_batch(rows).map_err(store_error("put_batch"))?;
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
        let mut kv = self.kv()?;
        kv.put(key, KvValue::new(bytes))
            .map_err(store_error("write_manifest"))?;
        Ok(())
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
