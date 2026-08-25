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

use crate::branch::adapter::{
    CapabilityBranchAdapter, ComparableEntity, DerivedDisposition, EntitySummary,
};
use crate::data::kv::ProductSpace;
use crate::diagnostics::{EngineError, EngineResult};
use crate::persistence::{
    decode_vector_key, encode_vector_space_prefix, PersistenceReadRow, RowClass,
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
