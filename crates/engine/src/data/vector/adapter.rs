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

use std::collections::BTreeMap;

use crate::api::{ComparedCapability, ConflictKind, ConflictStrategyResult, PreviewConflict};
use crate::branch::adapter::{
    CapabilityBranchAdapter, ComparableEntity, DerivedDisposition, EntitySummary,
};
use crate::branch::catalog::BranchCatalogRecord;
use crate::data::kv::ProductSpace;
use crate::diagnostics::{EngineError, EngineResult};
use crate::persistence::{
    decode_vector_key, encode_vector_collection_prefix, encode_vector_space_prefix,
    PersistenceReadRow, ReadSelector, RowAddress, RowClass, RowMutation, StoragePersistence,
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
) -> EngineResult<(Vec<RowMutation>, Vec<PreviewConflict>)> {
    let mut mutations = Vec::new();
    let mut conflicts = Vec::new();
    for space in spaces {
        let prefix = encode_vector_collection_prefix(space);
        let target_configs = collection_config_rows(persistence, target, &prefix)?;
        for (key, source_value) in collection_config_rows(persistence, source, &prefix)? {
            match target_configs.get(&key) {
                // Identical config: nothing to carry.
                Some(target_value) if *target_value == source_value => {}
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
                    RowAddress::new(target.storage_branch_id(), RowClass::VectorCollection, key),
                    source_value,
                )),
            }
        }
    }
    Ok((mutations, conflicts))
}

/// Every visible collection config row for `record` under `prefix`, keyed by the
/// full storage key so source and target rows for the same collection align.
fn collection_config_rows(
    persistence: &mut StoragePersistence,
    record: &BranchCatalogRecord,
    prefix: &[u8],
) -> EngineResult<BTreeMap<Vec<u8>, Vec<u8>>> {
    let rows = persistence.scan_prefix(
        record.storage_branch_id(),
        RowClass::VectorCollection,
        prefix.to_vec(),
        ReadSelector::Latest,
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
