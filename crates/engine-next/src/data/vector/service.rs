//! Vector service.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use strata_core_next::{CommitVersion, Timestamp};

use crate::branch::catalog::BranchCatalogRecord;
use crate::branch::BranchName;
use crate::commit::CommitOutcome;
use crate::control::ControlPlane;
use crate::data::kv::ProductSpace;
use crate::diagnostics::{EngineError, EngineResult};
use crate::persistence::{
    decode_vector_collection_name, decode_vector_key, encode_vector_collection_entry_prefix,
    encode_vector_collection_key, encode_vector_collection_prefix, encode_vector_key, CommitPlan,
    PersistenceReadRow, ReadSelector, RowAddress, RowClass, RowMutation, StoragePersistence,
};

use super::{
    decode_collection_config, decode_vector_record, encode_collection_config, encode_vector_record,
    vector_score, VectorBatchDeleteOutcome, VectorBatchGetOutcome, VectorBatchUpsertOutcome,
    VectorBulkDeleteOutcome, VectorCollectionInfo, VectorCollectionName, VectorConfig,
    VectorDeleteOutcome, VectorEmbedding, VectorEntry, VectorFilter, VectorHistory,
    VectorHistoryRow, VectorKey, VectorKeyPage, VectorMetadata, VectorMetadataPatch,
    VectorMetadataUpdateOutcome, VectorRecord, VectorSearchMatch, VectorSearchResult,
    VectorUpsertEntry, VectorVersionedEntry, VectorWriteOutcome,
};

const VECTOR_LIST_RAW_PAGE_MIN: usize = 64;
const VECTOR_LIST_RAW_PAGE_MAX: usize = 4096;

/// Service for vector collection and exact search operations.
pub struct VectorService<'a> {
    persistence: &'a mut StoragePersistence,
    control: &'a mut ControlPlane,
    branch: BranchName,
    space: ProductSpace,
}

impl<'a> VectorService<'a> {
    pub(crate) const fn new(
        persistence: &'a mut StoragePersistence,
        control: &'a mut ControlPlane,
        branch: BranchName,
        space: ProductSpace,
    ) -> Self {
        Self {
            persistence,
            control,
            branch,
            space,
        }
    }

    /// Creates a vector collection.
    pub fn create_collection(
        &mut self,
        name: VectorCollectionName,
        config: VectorConfig,
    ) -> EngineResult<VectorCollectionInfo> {
        let record = self.branch_record()?;
        let address = self.collection_address(&record, &name);
        if self
            .persistence
            .read_row(&address, ReadSelector::Latest)?
            .is_some_and(|row| !row.is_tombstone())
        {
            return Err(EngineError::conflict(
                "already_exists.engine.vector_collection",
                "vector collection already exists",
            ));
        }
        let commit = self.commit_batch(
            &record,
            vec![RowMutation::put(
                address,
                encode_collection_config(&name, config)?,
            )],
        )?;
        Ok(VectorCollectionInfo::new(
            name,
            config,
            0,
            commit.version(),
            commit.timestamp(),
        ))
    }

    /// Deletes a vector collection and all currently visible vectors in it.
    pub fn delete_collection(&mut self, name: &VectorCollectionName) -> EngineResult<bool> {
        let record = self.branch_record()?;
        let Some(_) = self.collection_config_row(&record, name, ReadSelector::Latest)? else {
            return Ok(false);
        };
        let mut mutations = vec![RowMutation::delete(self.collection_address(&record, name))];
        for row in self.vector_rows(&record, name, ReadSelector::Latest)? {
            if row.is_tombstone() {
                continue;
            }
            mutations.push(RowMutation::delete(RowAddress::new(
                record.storage_branch_id(),
                RowClass::Vector,
                row.key().to_vec(),
            )));
        }
        self.commit_batch(&record, mutations)?;
        Ok(true)
    }

    /// Lists visible vector collections.
    pub fn list_collections(&mut self) -> EngineResult<Vec<VectorCollectionInfo>> {
        let record = self.branch_record()?;
        let mut collections = self
            .persistence
            .scan_prefix(
                record.storage_branch_id(),
                RowClass::VectorCollection,
                encode_vector_collection_prefix(&self.space),
                ReadSelector::Latest,
                None,
            )?
            .into_iter()
            .filter(|row| !row.is_tombstone())
            .map(|row| self.collection_info_from_row(&record, &row))
            .collect::<EngineResult<Vec<_>>>()?;
        collections.sort_by(|left, right| left.name().cmp(right.name()));
        Ok(collections)
    }

    /// Returns visible collection info.
    pub fn collection_info(
        &mut self,
        name: &VectorCollectionName,
    ) -> EngineResult<Option<VectorCollectionInfo>> {
        let record = self.branch_record()?;
        self.collection_config_row(&record, name, ReadSelector::Latest)?
            .map(|row| self.collection_info_from_row(&record, &row))
            .transpose()
    }

    /// Counts visible vectors in a collection.
    pub fn count(&mut self, name: &VectorCollectionName) -> EngineResult<u64> {
        let record = self.branch_record()?;
        self.require_collection_config(&record, name)?;
        self.count_with_record(&record, name)
    }

    /// Upserts one vector entry.
    pub fn upsert(
        &mut self,
        collection: VectorCollectionName,
        key: VectorKey,
        embedding: VectorEmbedding,
        metadata: Option<VectorMetadata>,
    ) -> EngineResult<VectorWriteOutcome> {
        let record = self.branch_record()?;
        let config = self.require_collection_config(&record, &collection)?;
        embedding.validate_dimension(config.dimension())?;
        let revision = self
            .latest_vector_revision(&record, &collection, &key)?
            .unwrap_or(0)
            .saturating_add(1);
        let address = self.vector_address(&record, &collection, &key);
        let vector = VectorRecord::new(collection, key, embedding, metadata, revision);
        let commit = self.commit_batch(
            &record,
            vec![RowMutation::put(address, encode_vector_record(&vector)?)],
        )?;
        Ok(VectorWriteOutcome::new(commit, revision))
    }

    /// Reads the latest visible vector entry.
    pub fn get(
        &mut self,
        collection: &VectorCollectionName,
        key: &VectorKey,
    ) -> EngineResult<Option<VectorEntry>> {
        Ok(self
            .get_versioned(collection, key)?
            .map(|entry| entry.entry().clone()))
    }

    /// Reads the latest visible vector entry with commit metadata.
    pub fn get_versioned(
        &mut self,
        collection: &VectorCollectionName,
        key: &VectorKey,
    ) -> EngineResult<Option<VectorVersionedEntry>> {
        let record = self.branch_record()?;
        self.require_collection_config(&record, collection)?;
        let address = self.vector_address(&record, collection, key);
        let Some(row) = self.persistence.read_row(&address, ReadSelector::Latest)? else {
            return Ok(None);
        };
        Self::versioned_entry_from_row(collection, key, &row)
    }

    /// Reads a vector entry at a commit version.
    pub fn get_at_version(
        &mut self,
        collection: &VectorCollectionName,
        key: &VectorKey,
        version: CommitVersion,
    ) -> EngineResult<Option<VectorVersionedEntry>> {
        let record = self.branch_record()?;
        self.require_collection_config_with_selector(
            &record,
            collection,
            ReadSelector::AtVersion(version),
        )?;
        let address = self.vector_address(&record, collection, key);
        let Some(row) = self
            .persistence
            .read_row(&address, ReadSelector::AtVersion(version))?
        else {
            return Ok(None);
        };
        Self::versioned_entry_from_row(collection, key, &row)
    }

    /// Reads a vector entry at a commit timestamp.
    pub fn get_at(
        &mut self,
        collection: &VectorCollectionName,
        key: &VectorKey,
        timestamp: Timestamp,
    ) -> EngineResult<Option<VectorVersionedEntry>> {
        let record = self.branch_record()?;
        self.require_collection_config_with_selector(
            &record,
            collection,
            ReadSelector::AtTimestamp(timestamp),
        )?;
        let address = self.vector_address(&record, collection, key);
        let Some(row) = self
            .persistence
            .read_row(&address, ReadSelector::AtTimestamp(timestamp))?
        else {
            return Ok(None);
        };
        Self::versioned_entry_from_row(collection, key, &row)
    }

    /// Reads full vector history newest-first.
    pub fn history(
        &mut self,
        collection: &VectorCollectionName,
        key: &VectorKey,
    ) -> EngineResult<Option<VectorHistory>> {
        let record = self.branch_record()?;
        self.require_collection_config_history(&record, collection)?;
        let address = self.vector_address(&record, collection, key);
        let rows = self
            .persistence
            .read_history(&address, true)?
            .into_iter()
            .map(|row| Self::history_row_from_row(collection, key, &row))
            .collect::<EngineResult<Vec<_>>>()?;
        Ok((!rows.is_empty()).then(|| VectorHistory::new(rows)))
    }

    /// Returns true when the vector key has a latest visible value.
    pub fn exists(
        &mut self,
        collection: &VectorCollectionName,
        key: &VectorKey,
    ) -> EngineResult<bool> {
        Ok(self.get_versioned(collection, key)?.is_some())
    }

    /// Lists visible vector keys with cursor pagination.
    pub fn list_keys(
        &mut self,
        collection: &VectorCollectionName,
        prefix: Option<&VectorKey>,
        cursor: Option<&VectorKey>,
        limit: usize,
    ) -> EngineResult<VectorKeyPage> {
        let record = self.branch_record()?;
        self.require_collection_config(&record, collection)?;
        if limit == 0 {
            return Ok(VectorKeyPage::new(Vec::new(), false, None));
        }
        let mut keys = self.keys_after_cursor(
            &record,
            collection,
            prefix,
            cursor,
            limit.saturating_add(1),
            ReadSelector::Latest,
        )?;
        let has_more = keys.len() > limit;
        if has_more {
            keys.truncate(limit);
        }
        let cursor = has_more.then(|| keys.last().expect("non-empty page").clone());
        Ok(VectorKeyPage::new(keys, has_more, cursor))
    }

    /// Updates top-level vector metadata fields.
    pub fn update_metadata(
        &mut self,
        collection: &VectorCollectionName,
        key: VectorKey,
        patch: &VectorMetadataPatch,
    ) -> EngineResult<VectorMetadataUpdateOutcome> {
        let record = self.branch_record()?;
        self.require_collection_config(&record, collection)?;
        let address = self.vector_address(&record, collection, &key);
        let Some(row) = self.persistence.read_row(&address, ReadSelector::Latest)? else {
            return Ok(VectorMetadataUpdateOutcome::new(key, false, None, None));
        };
        if row.is_tombstone() {
            return Ok(VectorMetadataUpdateOutcome::new(key, false, None, None));
        }
        let current = Self::vector_record_from_row(collection, &key, &row)?;
        let mut object = match current.metadata().map(VectorMetadata::as_inner) {
            None => serde_json::Map::new(),
            Some(Value::Object(object)) => object.clone(),
            Some(_) => {
                return Err(EngineError::invalid_input(
                    "invalid_argument.engine.vector_metadata_patch",
                    "vector metadata patch requires existing object metadata",
                ));
            }
        };
        for (field, value) in patch.fields() {
            object.insert(field.clone(), value.clone());
        }
        let metadata = VectorMetadata::new(Value::Object(object))?;
        let revision = current.vector_revision().saturating_add(1);
        let vector = VectorRecord::new(
            collection.clone(),
            key.clone(),
            current.embedding().clone(),
            Some(metadata),
            revision,
        );
        let commit = self.commit_batch(
            &record,
            vec![RowMutation::put(address, encode_vector_record(&vector)?)],
        )?;
        Ok(VectorMetadataUpdateOutcome::new(
            key,
            true,
            Some(revision),
            Some(commit),
        ))
    }

    /// Deletes one vector if present.
    pub fn delete(
        &mut self,
        collection: &VectorCollectionName,
        key: VectorKey,
    ) -> EngineResult<VectorDeleteOutcome> {
        let outcome = self.batch_delete(collection, std::slice::from_ref(&key))?;
        let deleted = outcome.deleted().first().copied().unwrap_or(false);
        Ok(VectorDeleteOutcome::new(key, deleted, outcome.commit()))
    }

    /// Deletes visible vectors matching a non-empty metadata filter.
    pub fn delete_by_filter(
        &mut self,
        collection: &VectorCollectionName,
        filter: &VectorFilter,
    ) -> EngineResult<VectorBulkDeleteOutcome> {
        if filter.is_empty() {
            return Err(EngineError::invalid_input(
                "invalid_argument.engine.vector_filter",
                "vector metadata filter must not be empty for filtered delete",
            ));
        }
        self.delete_matching(collection, |entry| filter.matches(entry.metadata()))
    }

    /// Deletes every visible vector in a collection.
    pub fn delete_all(
        &mut self,
        collection: &VectorCollectionName,
    ) -> EngineResult<VectorBulkDeleteOutcome> {
        self.delete_matching(collection, |_| true)
    }

    /// Upserts multiple vector entries in one commit.
    pub fn batch_upsert(
        &mut self,
        collection: &VectorCollectionName,
        entries: &[VectorUpsertEntry],
    ) -> EngineResult<VectorBatchUpsertOutcome> {
        let record = self.branch_record()?;
        let config = self.require_collection_config(&record, collection)?;
        if entries.is_empty() {
            return Ok(VectorBatchUpsertOutcome::new(Vec::new(), None));
        }
        for entry in entries {
            entry.embedding().validate_dimension(config.dimension())?;
        }

        let mut revisions_by_key = BTreeMap::<VectorKey, u64>::new();
        let mut final_records = BTreeMap::<VectorKey, VectorRecord>::new();
        let mut revisions = Vec::with_capacity(entries.len());
        for entry in entries {
            let previous_revision = if let Some(revision) = revisions_by_key.get(entry.key()) {
                *revision
            } else {
                self.latest_vector_revision(&record, collection, entry.key())?
                    .unwrap_or(0)
            };
            let revision = previous_revision.saturating_add(1);
            revisions_by_key.insert(entry.key().clone(), revision);
            revisions.push(revision);
            final_records.insert(
                entry.key().clone(),
                VectorRecord::new(
                    collection.clone(),
                    entry.key().clone(),
                    entry.embedding().clone(),
                    entry.metadata().cloned(),
                    revision,
                ),
            );
        }
        let mutations = final_records
            .into_iter()
            .map(|(key, vector)| {
                Ok(RowMutation::put(
                    self.vector_address(&record, collection, &key),
                    encode_vector_record(&vector)?,
                ))
            })
            .collect::<EngineResult<Vec<_>>>()?;
        let commit = self.commit_batch(&record, mutations)?;
        Ok(VectorBatchUpsertOutcome::new(revisions, Some(commit)))
    }

    /// Reads multiple latest visible vector entries with metadata.
    pub fn batch_get(
        &mut self,
        collection: &VectorCollectionName,
        keys: &[VectorKey],
    ) -> EngineResult<VectorBatchGetOutcome> {
        let record = self.branch_record()?;
        self.require_collection_config(&record, collection)?;
        let mut entries = Vec::with_capacity(keys.len());
        for key in keys {
            let address = self.vector_address(&record, collection, key);
            let entry = match self.persistence.read_row(&address, ReadSelector::Latest)? {
                Some(row) => Self::versioned_entry_from_row(collection, key, &row)?,
                None => None,
            };
            entries.push(entry);
        }
        Ok(VectorBatchGetOutcome::new(entries))
    }

    /// Deletes multiple vector keys in one commit.
    pub fn batch_delete(
        &mut self,
        collection: &VectorCollectionName,
        keys: &[VectorKey],
    ) -> EngineResult<VectorBatchDeleteOutcome> {
        let record = self.branch_record()?;
        self.require_collection_config(&record, collection)?;
        if keys.is_empty() {
            return Ok(VectorBatchDeleteOutcome::new(Vec::new(), None));
        }
        let mut seen = BTreeSet::<VectorKey>::new();
        let mut mutations = Vec::new();
        let mut deleted = Vec::with_capacity(keys.len());
        for key in keys {
            if !seen.insert(key.clone()) {
                deleted.push(false);
                continue;
            }
            let address = self.vector_address(&record, collection, key);
            let exists = self
                .persistence
                .read_row(&address, ReadSelector::Latest)?
                .is_some_and(|row| !row.is_tombstone());
            if exists {
                mutations.push(RowMutation::delete(address));
            }
            deleted.push(exists);
        }
        if mutations.is_empty() {
            return Ok(VectorBatchDeleteOutcome::new(deleted, None));
        }
        let commit = self.commit_batch(&record, mutations)?;
        Ok(VectorBatchDeleteOutcome::new(deleted, Some(commit)))
    }

    /// Runs exact latest nearest-neighbor search.
    pub fn query(
        &mut self,
        collection: &VectorCollectionName,
        query: &VectorEmbedding,
        k: usize,
        filter: Option<&VectorFilter>,
    ) -> EngineResult<VectorSearchResult> {
        self.query_with_selector(collection, query, k, filter, ReadSelector::Latest)
    }

    /// Runs exact nearest-neighbor search at a timestamp.
    pub fn query_at(
        &mut self,
        collection: &VectorCollectionName,
        query: &VectorEmbedding,
        k: usize,
        filter: Option<&VectorFilter>,
        timestamp: Timestamp,
    ) -> EngineResult<VectorSearchResult> {
        self.query_with_selector(
            collection,
            query,
            k,
            filter,
            ReadSelector::AtTimestamp(timestamp),
        )
    }

    fn delete_matching(
        &mut self,
        collection: &VectorCollectionName,
        predicate: impl Fn(&VectorEntry) -> bool,
    ) -> EngineResult<VectorBulkDeleteOutcome> {
        let record = self.branch_record()?;
        self.require_collection_config(&record, collection)?;
        let rows = self.visible_vector_entries(&record, collection, ReadSelector::Latest)?;
        let mut mutations = Vec::new();
        for (row, entry) in rows {
            if predicate(&entry) {
                mutations.push(RowMutation::delete(RowAddress::new(
                    record.storage_branch_id(),
                    RowClass::Vector,
                    row.key().to_vec(),
                )));
            }
        }
        if mutations.is_empty() {
            return Ok(VectorBulkDeleteOutcome::new(0, None));
        }
        let deleted_count = u64::try_from(mutations.len()).unwrap_or(u64::MAX);
        let commit = self.commit_batch(&record, mutations)?;
        Ok(VectorBulkDeleteOutcome::new(deleted_count, Some(commit)))
    }

    fn query_with_selector(
        &mut self,
        collection: &VectorCollectionName,
        query: &VectorEmbedding,
        k: usize,
        filter: Option<&VectorFilter>,
        selector: ReadSelector,
    ) -> EngineResult<VectorSearchResult> {
        let record = self.branch_record()?;
        let config = self.require_collection_config_with_selector(&record, collection, selector)?;
        query.validate_dimension(config.dimension())?;
        if k == 0 {
            return Ok(VectorSearchResult::new(Vec::new()));
        }
        let mut matches = Vec::new();
        for (row, entry) in self.visible_vector_entries(&record, collection, selector)? {
            if filter.is_some_and(|filter| !filter.matches(entry.metadata())) {
                continue;
            }
            let score = vector_score(query, entry.embedding(), config.metric())?;
            matches.push(VectorSearchMatch::new(
                entry,
                score,
                row.commit_version(),
                row.commit_timestamp(),
            ));
        }
        matches.sort_by(|left, right| {
            right
                .score()
                .total_cmp(&left.score())
                .then_with(|| left.entry().key().cmp(right.entry().key()))
        });
        if matches.len() > k {
            matches.truncate(k);
        }
        Ok(VectorSearchResult::new(matches))
    }

    fn branch_record(&self) -> EngineResult<BranchCatalogRecord> {
        self.control.require_healthy()?;
        self.control
            .lookup_branch(&self.branch)
            .cloned()
            .ok_or_else(|| {
                EngineError::not_found(
                    "not_found.engine.branch",
                    format!("branch `{}` does not exist", self.branch),
                )
            })
    }

    fn collection_address(
        &self,
        record: &BranchCatalogRecord,
        collection: &VectorCollectionName,
    ) -> RowAddress {
        RowAddress::new(
            record.storage_branch_id(),
            RowClass::VectorCollection,
            encode_vector_collection_key(&self.space, collection),
        )
    }

    fn vector_address(
        &self,
        record: &BranchCatalogRecord,
        collection: &VectorCollectionName,
        key: &VectorKey,
    ) -> RowAddress {
        RowAddress::new(
            record.storage_branch_id(),
            RowClass::Vector,
            encode_vector_key(&self.space, collection, key),
        )
    }

    fn collection_config_row(
        &mut self,
        record: &BranchCatalogRecord,
        collection: &VectorCollectionName,
        selector: ReadSelector,
    ) -> EngineResult<Option<PersistenceReadRow>> {
        let address = self.collection_address(record, collection);
        Ok(self
            .persistence
            .read_row(&address, selector)?
            .filter(|row| !row.is_tombstone()))
    }

    fn require_collection_config(
        &mut self,
        record: &BranchCatalogRecord,
        collection: &VectorCollectionName,
    ) -> EngineResult<VectorConfig> {
        self.require_collection_config_with_selector(record, collection, ReadSelector::Latest)
    }

    fn require_collection_config_with_selector(
        &mut self,
        record: &BranchCatalogRecord,
        collection: &VectorCollectionName,
        selector: ReadSelector,
    ) -> EngineResult<VectorConfig> {
        let Some(row) = self.collection_config_row(record, collection, selector)? else {
            return Err(EngineError::not_found(
                "not_found.engine.vector_collection",
                "vector collection does not exist",
            ));
        };
        let value = row.value().ok_or_else(|| {
            EngineError::corruption(
                "data_loss.engine.vector_collection",
                "stored vector collection row is missing a value",
            )
        })?;
        decode_collection_config(collection, value)
    }

    fn require_collection_config_history(
        &mut self,
        record: &BranchCatalogRecord,
        collection: &VectorCollectionName,
    ) -> EngineResult<()> {
        if let Some(row) = self.collection_config_row(record, collection, ReadSelector::Latest)? {
            let value = row.value().ok_or_else(|| {
                EngineError::corruption(
                    "data_loss.engine.vector_collection",
                    "stored vector collection row is missing a value",
                )
            })?;
            let _ = decode_collection_config(collection, value)?;
            return Ok(());
        }

        let address = self.collection_address(record, collection);
        for row in self.persistence.read_history(&address, true)? {
            if row.is_tombstone() {
                continue;
            }
            let value = row.value().ok_or_else(|| {
                EngineError::corruption(
                    "data_loss.engine.vector_collection",
                    "stored vector collection row is missing a value",
                )
            })?;
            let _ = decode_collection_config(collection, value)?;
            return Ok(());
        }

        Err(EngineError::not_found(
            "not_found.engine.vector_collection",
            "vector collection does not exist",
        ))
    }

    fn collection_info_from_row(
        &mut self,
        record: &BranchCatalogRecord,
        row: &PersistenceReadRow,
    ) -> EngineResult<VectorCollectionInfo> {
        let name = decode_vector_collection_name(&self.space, row.key())?;
        let value = row.value().ok_or_else(|| {
            EngineError::corruption(
                "data_loss.engine.vector_collection",
                "stored vector collection row is missing a value",
            )
        })?;
        let config = decode_collection_config(&name, value)?;
        let count = self.count_with_record(record, &name)?;
        Ok(VectorCollectionInfo::new(
            name,
            config,
            count,
            row.commit_version(),
            row.commit_timestamp(),
        ))
    }

    fn count_with_record(
        &mut self,
        record: &BranchCatalogRecord,
        collection: &VectorCollectionName,
    ) -> EngineResult<u64> {
        let count = self
            .vector_rows(record, collection, ReadSelector::Latest)?
            .into_iter()
            .filter(|row| !row.is_tombstone())
            .count();
        Ok(u64::try_from(count).unwrap_or(u64::MAX))
    }

    fn vector_rows(
        &mut self,
        record: &BranchCatalogRecord,
        collection: &VectorCollectionName,
        selector: ReadSelector,
    ) -> EngineResult<Vec<PersistenceReadRow>> {
        self.persistence.scan_prefix(
            record.storage_branch_id(),
            RowClass::Vector,
            encode_vector_collection_entry_prefix(&self.space, collection),
            selector,
            None,
        )
    }

    fn visible_vector_entries(
        &mut self,
        record: &BranchCatalogRecord,
        collection: &VectorCollectionName,
        selector: ReadSelector,
    ) -> EngineResult<Vec<(PersistenceReadRow, VectorEntry)>> {
        self.vector_rows(record, collection, selector)?
            .into_iter()
            .filter(|row| !row.is_tombstone())
            .map(|row| {
                let (_, key) = decode_vector_key(&self.space, row.key())?;
                let entry = Self::entry_from_row(collection, &key, &row)?;
                Ok((row, entry))
            })
            .collect()
    }

    fn keys_after_cursor(
        &mut self,
        record: &BranchCatalogRecord,
        collection: &VectorCollectionName,
        prefix: Option<&VectorKey>,
        cursor: Option<&VectorKey>,
        limit: usize,
        selector: ReadSelector,
    ) -> EngineResult<Vec<VectorKey>> {
        let prefix_start = encode_vector_collection_entry_prefix(&self.space, collection);
        let prefix_end = next_prefix(&prefix_start);
        let mut start = prefix_start.clone();
        let mut keys = Vec::with_capacity(limit);
        while keys.len() < limit && start < prefix_end {
            let remaining = limit.saturating_sub(keys.len());
            let raw_limit = remaining
                .saturating_add(1)
                .clamp(VECTOR_LIST_RAW_PAGE_MIN, VECTOR_LIST_RAW_PAGE_MAX);
            let rows = self.persistence.scan_range(
                record.storage_branch_id(),
                RowClass::Vector,
                Some(start.clone()),
                Some(prefix_end.clone()),
                selector,
                Some(raw_limit),
            )?;
            if rows.is_empty() {
                break;
            }
            for row in &rows {
                if row.is_tombstone() {
                    continue;
                }
                let (_, key) = decode_vector_key(&self.space, row.key())?;
                if cursor.is_some_and(|cursor| key.as_str() <= cursor.as_str()) {
                    continue;
                }
                if let Some(prefix) = prefix {
                    if !key.as_str().starts_with(prefix.as_str()) {
                        continue;
                    }
                }
                keys.push(key);
                if keys.len() >= limit {
                    break;
                }
            }
            let last_raw_key = rows.last().expect("non-empty raw page").key();
            start = exclusive_after_key(last_raw_key);
        }
        keys.sort();
        Ok(keys)
    }

    fn latest_vector_revision(
        &mut self,
        record: &BranchCatalogRecord,
        collection: &VectorCollectionName,
        key: &VectorKey,
    ) -> EngineResult<Option<u64>> {
        let address = self.vector_address(record, collection, key);
        if let Some(row) = self.persistence.read_row(&address, ReadSelector::Latest)? {
            if !row.is_tombstone() {
                return Ok(Some(
                    Self::vector_record_from_row(collection, key, &row)?.vector_revision(),
                ));
            }
        }
        for row in self.persistence.read_history(&address, false)? {
            if row.is_tombstone() {
                continue;
            }
            return Ok(Some(
                Self::vector_record_from_row(collection, key, &row)?.vector_revision(),
            ));
        }
        Ok(None)
    }

    fn versioned_entry_from_row(
        collection: &VectorCollectionName,
        key: &VectorKey,
        row: &PersistenceReadRow,
    ) -> EngineResult<Option<VectorVersionedEntry>> {
        if row.is_tombstone() {
            return Ok(None);
        }
        Ok(Some(VectorVersionedEntry::new(
            Self::entry_from_row(collection, key, row)?,
            row.commit_version(),
            row.commit_timestamp(),
        )))
    }

    fn history_row_from_row(
        collection: &VectorCollectionName,
        key: &VectorKey,
        row: &PersistenceReadRow,
    ) -> EngineResult<VectorHistoryRow> {
        if row.is_tombstone() {
            return Ok(VectorHistoryRow::new(
                None,
                true,
                row.commit_version(),
                row.commit_timestamp(),
                None,
            ));
        }
        let entry = Self::entry_from_row(collection, key, row)?;
        let revision = entry.vector_revision();
        Ok(VectorHistoryRow::new(
            Some(entry),
            false,
            row.commit_version(),
            row.commit_timestamp(),
            Some(revision),
        ))
    }

    fn entry_from_row(
        collection: &VectorCollectionName,
        key: &VectorKey,
        row: &PersistenceReadRow,
    ) -> EngineResult<VectorEntry> {
        let vector = Self::vector_record_from_row(collection, key, row)?;
        Ok(VectorEntry::new(
            vector.key().clone(),
            vector.embedding().clone(),
            vector.metadata().cloned(),
            vector.vector_revision(),
        ))
    }

    fn vector_record_from_row(
        collection: &VectorCollectionName,
        key: &VectorKey,
        row: &PersistenceReadRow,
    ) -> EngineResult<VectorRecord> {
        let value = row.value().ok_or_else(|| {
            EngineError::corruption(
                "data_loss.engine.vector_record",
                "stored vector row is missing a value",
            )
        })?;
        decode_vector_record(collection, key, value)
    }

    fn commit_batch(
        &mut self,
        record: &BranchCatalogRecord,
        mutations: Vec<RowMutation>,
    ) -> EngineResult<CommitOutcome> {
        if mutations.is_empty() {
            return Err(EngineError::invalid_input(
                "invalid_argument.engine.vector_batch",
                "vector batch must contain at least one mutation",
            ));
        }
        let plan = CommitPlan::new(
            record.storage_branch_id(),
            mutations,
            Some(record.generation()),
        );
        self.persistence.commit(&plan)
    }
}

fn next_prefix(prefix: &[u8]) -> Vec<u8> {
    let mut upper = prefix.to_vec();
    for index in (0..upper.len()).rev() {
        if upper[index] != u8::MAX {
            upper[index] += 1;
            upper.truncate(index + 1);
            return upper;
        }
    }
    vec![u8::MAX]
}

fn exclusive_after_key(key: &[u8]) -> Vec<u8> {
    let mut next = key.to_vec();
    next.push(0);
    next
}
