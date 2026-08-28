//! Vector capability branch adapter.
//!
//! A vector entry is a single authored [`RowClass::Vector`] MVCC row: the key is
//! the space prefix followed by the length-prefixed collection name and the user
//! key, and the value is the encoded record. The comparable identity is that
//! collection-qualified, space-relative suffix, so the same key in two
//! collections compares as two distinct entities. Decoding the key through
//! [`decode_vector_key`] inherits the vector capability's foreign-space and
//! malformed-byte diagnostics for free.
//!
//! The derived vector index needs no branch-workflow handling: the query path is
//! exact-correct via its full-collection-scan fallback, and promoted rows land
//! past any index watermark, so a promote that writes authored vector rows keeps
//! search correct without touching the manifest.

use std::collections::{BTreeMap, BTreeSet};

use strata_core::BranchId;

use crate::api::{ComparedCapability, ConflictKind, ConflictStrategyResult, PreviewConflict};
use crate::branch::adapter::{
    CapabilityBranchAdapter, ComparableEntity, DerivedDisposition, EntitySummary,
};
use crate::branch::catalog::BranchCatalogRecord;
use crate::branch::preview::base_point_for;
use crate::data::kv::ProductSpace;
use crate::data::vector::VectorCollectionName;
use crate::diagnostics::{EngineError, EngineResult};
use crate::persistence::{
    decode_vector_collection_name, decode_vector_key, encode_vector_collection_entry_prefix,
    encode_vector_collection_prefix, encode_vector_space_prefix, PersistenceReadRow, ReadSelector,
    RowAddress, RowClass, RowMutation, StoragePersistence,
};

/// The vector capability's branch adapter.
pub(crate) struct VectorBranchAdapter;

impl CapabilityBranchAdapter for VectorBranchAdapter {
    fn row_class(&self) -> RowClass {
        RowClass::Vector
    }

    fn derived_disposition(&self) -> DerivedDisposition {
        DerivedDisposition::Authored
    }

    fn space_prefix(&self, space: &ProductSpace) -> Vec<u8> {
        encode_vector_space_prefix(space)
    }

    fn interpret_row(
        &self,
        space: &ProductSpace,
        row: &PersistenceReadRow,
    ) -> EngineResult<ComparableEntity> {
        // Validate the row is a well-formed vector key in this space, rejecting
        // foreign-space and malformed keys with the vector capability's
        // structured diagnostic.
        decode_vector_key(space, row.key())?;
        let identity = row
            .key()
            .strip_prefix(encode_vector_space_prefix(space).as_slice())
            .ok_or_else(|| {
                EngineError::corruption(
                    "data_loss.engine.vector_key",
                    "stored vector row key is outside the requested space",
                )
            })?
            .to_vec();
        let summary = if row.is_tombstone() {
            EntitySummary::Absent
        } else {
            let value = row.value().ok_or_else(|| {
                EngineError::corruption(
                    "data_loss.engine.vector_record",
                    "stored vector row is present but carries no value",
                )
            })?;
            EntitySummary::Present(value.to_vec())
        };
        Ok(ComparableEntity::new(
            identity,
            summary,
            row.commit_version(),
        ))
    }
}

/// Plans carrying `source`'s vector collection **configs** into `target` during a
/// promotion, per space. Collection config rows (`RowClass::VectorCollection`)
/// are not authored vector data, so the capability adapter above never carries
/// them; without this, promoted vectors land behind a missing config and reads
/// fail `not_found.engine.vector_collection`.
///
/// A collection's entire config is `(dimension, metric)` — all structural — and
/// is encoded deterministically, so the stored bytes are a faithful identity:
/// identical bytes are the same collection; any difference is an incompatible
/// dimension or metric change (contract Vector minimum: conflict on
/// metric/dimension). A source-only collection is carried; a collection present
/// on both with a divergent config is reported as a `ValueDivergence` conflict.
/// Like any conflicting entity, the source config is queued as a mutation and
/// only lands when the caller commits — strict refuses on the conflict, source-
/// wins overwrites.
pub(crate) fn plan_collection_promotion(
    persistence: &mut StoragePersistence,
    source: &BranchCatalogRecord,
    target: &BranchCatalogRecord,
    spaces: &[ProductSpace],
    strategy_result: ConflictStrategyResult,
) -> EngineResult<(Vec<RowMutation>, Vec<PreviewConflict>)> {
    let (base_branch, base_selector) = base_point_for(source, target)?;
    let mut mutations = Vec::new();
    let mut conflicts = Vec::new();
    for space in spaces {
        let prefix = encode_vector_collection_prefix(space);
        let target_configs = collection_config_rows(persistence, target, &prefix)?;
        let source_configs = collection_config_rows(persistence, source, &prefix)?;
        for (key, source_value) in &source_configs {
            match target_configs.get(key) {
                // Identical config: nothing to carry.
                Some(target_value) if target_value == source_value => {}
                // Divergent config = incompatible dimension/metric (the whole
                // config is structural). No strategy can merge it: record a
                // structural conflict and do NOT carry the config, so the service
                // refuses under every strategy instead of overwriting the target's
                // collection and mixing vector shapes.
                Some(target_value) => {
                    let identity = key
                        .strip_prefix(prefix.as_slice())
                        .unwrap_or(key.as_slice())
                        .to_vec();
                    conflicts.push(PreviewConflict::new(
                        ComparedCapability::Vector,
                        space.clone(),
                        identity,
                        Some(source_value.clone()),
                        Some(target_value.clone()),
                        ConflictKind::IncompatibleCollection,
                        ConflictStrategyResult::Refused,
                    ));
                }
                // Source-only collection: carry its config onto the target.
                None => mutations.push(RowMutation::put(
                    RowAddress::new(
                        target.storage_branch_id(),
                        RowClass::VectorCollection,
                        key.clone(),
                    ),
                    source_value.clone(),
                )),
            }
        }

        // Deletions: a collection config present in the base but gone from the
        // source was deleted there. Remove its now-stale config from the target,
        // unless the target still holds a live vector the promotion does not
        // delete (deregistering would orphan it behind a missing config).
        let base_configs =
            collection_config_rows_at(persistence, base_branch, &prefix, base_selector)?;
        for (key, base_value) in &base_configs {
            if source_configs.contains_key(key) {
                continue;
            }
            let Some(target_value) = target_configs.get(key) else {
                continue;
            };
            let collection = decode_vector_collection_name(space, key)?;
            // The retain guard takes precedence over the SourceWins deletion
            // below: an in-use collection is never orphaned, even under SourceWins.
            if target_retains_vectors(
                persistence,
                base_branch,
                base_selector,
                source,
                target,
                space,
                &collection,
            )? {
                continue;
            }
            let address = RowAddress::new(
                target.storage_branch_id(),
                RowClass::VectorCollection,
                key.clone(),
            );
            if target_value == base_value {
                // Clean one-sided deletion (the target never changed the config):
                // applies under both strategies, like a data-row deletion.
                mutations.push(RowMutation::delete(address));
            } else {
                // The source deleted the collection while the target independently
                // changed its config — a modify/delete divergence. Strict refuses;
                // SourceWins applies the deletion.
                let identity = key
                    .strip_prefix(prefix.as_slice())
                    .unwrap_or(key.as_slice())
                    .to_vec();
                conflicts.push(PreviewConflict::new(
                    ComparedCapability::Vector,
                    space.clone(),
                    identity,
                    None,
                    Some(target_value.clone()),
                    ConflictKind::ModifyDeleteDivergence,
                    strategy_result,
                ));
                if strategy_result == ConflictStrategyResult::SourceWins {
                    mutations.push(RowMutation::delete(address));
                }
            }
        }
    }
    Ok((mutations, conflicts))
}

/// Whether the `target` still holds a live vector in `collection` that this
/// promotion will NOT delete — used to avoid orphaning vectors behind a config
/// the deletion pass would otherwise remove.
///
/// The promotion deletes exactly the vectors the data three-way removes: a
/// base-inherited vector the source deleted (base-live, source-absent). A
/// target-live key that is either absent from the base (target-only, or one the
/// source created-and-deleted post-fork — a net no-op the three-way skips) or
/// still live on the source therefore survives. Vector keys are branch-
/// independent, so the three branches' keys compare directly.
#[allow(clippy::too_many_arguments)]
fn target_retains_vectors(
    persistence: &mut StoragePersistence,
    base_branch: BranchId,
    base_selector: ReadSelector,
    source: &BranchCatalogRecord,
    target: &BranchCatalogRecord,
    space: &ProductSpace,
    collection: &VectorCollectionName,
) -> EngineResult<bool> {
    let entry_prefix = encode_vector_collection_entry_prefix(space, collection);
    let target_live = live_vector_keys(
        persistence,
        target.storage_branch_id(),
        &entry_prefix,
        ReadSelector::Latest,
    )?;
    if target_live.is_empty() {
        return Ok(false);
    }
    let base_live = live_vector_keys(persistence, base_branch, &entry_prefix, base_selector)?;
    let source_live = live_vector_keys(
        persistence,
        source.storage_branch_id(),
        &entry_prefix,
        ReadSelector::Latest,
    )?;
    // A target-live vector survives unless it is base-inherited AND the source
    // deleted it (the only case the data three-way propagates as a deletion).
    Ok(target_live
        .iter()
        .any(|key| !base_live.contains(key) || source_live.contains(key)))
}

/// The set of live (non-tombstoned) vector row keys under `entry_prefix` on
/// `storage_branch` at `selector`.
fn live_vector_keys(
    persistence: &mut StoragePersistence,
    storage_branch: BranchId,
    entry_prefix: &[u8],
    selector: ReadSelector,
) -> EngineResult<BTreeSet<Vec<u8>>> {
    Ok(persistence
        .scan_prefix(
            storage_branch,
            RowClass::Vector,
            entry_prefix.to_vec(),
            selector,
            None,
        )?
        .into_iter()
        .filter(|row| !row.is_tombstone())
        .map(|row| row.key().to_vec())
        .collect())
}

/// Every visible collection config row for `record` under `prefix`, keyed by the
/// full storage key so source and target rows for the same collection align.
fn collection_config_rows(
    persistence: &mut StoragePersistence,
    record: &BranchCatalogRecord,
    prefix: &[u8],
) -> EngineResult<BTreeMap<Vec<u8>, Vec<u8>>> {
    collection_config_rows_at(
        persistence,
        record.storage_branch_id(),
        prefix,
        ReadSelector::Latest,
    )
}

/// Reads the live vector-collection config rows of `storage_branch` under `prefix`
/// at `selector` (e.g. a promotion base point). Tombstoned (deleted) collections
/// are skipped; a base→source diff over the returned keys detects source-side
/// deletions.
fn collection_config_rows_at(
    persistence: &mut StoragePersistence,
    storage_branch: BranchId,
    prefix: &[u8],
    selector: ReadSelector,
) -> EngineResult<BTreeMap<Vec<u8>, Vec<u8>>> {
    let rows = persistence.scan_prefix(
        storage_branch,
        RowClass::VectorCollection,
        prefix.to_vec(),
        selector,
        None,
    )?;
    let mut configs = BTreeMap::new();
    for row in &rows {
        if row.is_tombstone() {
            continue;
        }
        let value = row.value().ok_or_else(|| {
            EngineError::corruption(
                "data_loss.engine.vector_collection",
                "stored vector collection row is present but carries no value",
            )
        })?;
        configs.insert(row.key().to_vec(), value.to_vec());
    }
    Ok(configs)
}

#[cfg(test)]
mod tests {
    use super::VectorBranchAdapter;

    use strata_core::CommitVersion;

    use crate::branch::adapter::{CapabilityBranchAdapter, DerivedDisposition, EntitySummary};
    use crate::data::kv::ProductSpace;
    use crate::data::vector::{VectorCollectionName, VectorKey};
    use crate::diagnostics::EngineErrorClass;
    use crate::persistence::{
        encode_vector_key, encode_vector_space_prefix, PersistenceReadRow, RowClass,
    };

    fn space() -> ProductSpace {
        ProductSpace::new("default").expect("default is a valid space")
    }

    fn vector_row(
        space: &ProductSpace,
        collection: &str,
        key: &str,
        value: Option<&[u8]>,
        tombstone: bool,
    ) -> PersistenceReadRow {
        let encoded = encode_vector_key(
            space,
            &VectorCollectionName::new(collection).expect("valid collection"),
            &VectorKey::new(key).expect("valid key"),
        );
        PersistenceReadRow::for_test(encoded, value.map(<[u8]>::to_vec), tombstone)
    }

    #[test]
    fn interpret_row_decodes_a_present_vector_row() {
        let space = space();
        let row = vector_row(&space, "emb", "alpha", Some(b"record-bytes"), false);
        let entity = VectorBranchAdapter
            .interpret_row(&space, &row)
            .expect("decodes a present vector row");
        assert_eq!(
            entity.summary(),
            &EntitySummary::Present(b"record-bytes".to_vec())
        );
        assert_eq!(entity.version(), CommitVersion::new(1));
        assert!(!entity.is_tombstone());
    }

    #[test]
    fn interpret_row_maps_a_vector_tombstone_to_absent() {
        let space = space();
        let row = vector_row(&space, "emb", "gone", None, true);
        let entity = VectorBranchAdapter
            .interpret_row(&space, &row)
            .expect("decodes a tombstone");
        assert!(entity.is_tombstone());
        assert_eq!(entity.summary(), &EntitySummary::Absent);
    }

    #[test]
    fn identity_distinguishes_the_same_key_in_different_collections() {
        let space = space();
        let a = VectorBranchAdapter
            .interpret_row(&space, &vector_row(&space, "one", "k", Some(b"x"), false))
            .expect("decodes");
        let b = VectorBranchAdapter
            .interpret_row(&space, &vector_row(&space, "two", "k", Some(b"x"), false))
            .expect("decodes");
        assert_ne!(
            a.identity(),
            b.identity(),
            "the collection is part of the comparable identity"
        );
    }

    #[test]
    fn interpret_row_rejects_a_key_from_another_space() {
        let other = ProductSpace::new("other").expect("other is a valid space");
        let row = vector_row(&other, "emb", "alpha", Some(b"x"), false);
        let error = VectorBranchAdapter
            .interpret_row(&space(), &row)
            .expect_err("a key encoded for another space is rejected");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.vector_key");
    }

    #[test]
    fn interpret_row_rejects_a_present_row_without_a_value() {
        let space = space();
        let row = vector_row(&space, "emb", "beta", None, false);
        let error = VectorBranchAdapter
            .interpret_row(&space, &row)
            .expect_err("a present row without a value is rejected");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.vector_record");
    }

    #[test]
    fn space_prefix_matches_the_vector_encoding() {
        let space = space();
        assert_eq!(
            VectorBranchAdapter.space_prefix(&space),
            encode_vector_space_prefix(&space)
        );
    }

    #[test]
    fn row_class_and_disposition_are_reported() {
        assert_eq!(VectorBranchAdapter.row_class(), RowClass::Vector);
        assert_eq!(
            VectorBranchAdapter.derived_disposition(),
            DerivedDisposition::Authored
        );
    }
}
