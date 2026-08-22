//! Capability branch adapter — the engine-side contract that lets a data
//! capability participate in branch workflows.
//!
//! See `docs/architecture/engine/branch-operation-and-capability-adapter-contract.md`
//! (§Capability Branch Adapter, §Capability Branch Adapter Requirements).
//!
//! An adapter is a pure interpreter of one capability's rows: it names the row
//! class its authored rows live in, the storage-key prefix that scopes them to a
//! space, how each row decodes into a comparable entity, and how its derived
//! rows are disposed across branch workflows. Enumeration (choosing a branch, a
//! read selector, and running the scan) is workflow orchestration and stays out
//! of the adapter — the workflow feeds scanned rows to [`CapabilityBranchAdapter::interpret_row`].
//! Adapters return facts; they never commit.
//!
//! M12A1 defines the comparison-serving surface. Promotion, selected-copy, and
//! undo methods extend this trait at later M12 slices, where the post-V1
//! branch-operation absence guard is retired. The trait and its types are the
//! milestone foundation; their first non-test consumer is the M12B compare
//! workflow, so until that lands they are otherwise unreferenced.
#![allow(
    dead_code,
    reason = "M12A1 foundation: the trait and its types are consumed by the M12B \
              compare workflow; defined here as the milestone foundation"
)]

use strata_core::CommitVersion;

use crate::data::kv::ProductSpace;
use crate::diagnostics::EngineResult;
use crate::persistence::{PersistenceReadRow, RowClass};

/// How a capability's rows are treated across branch workflows.
///
/// Authored rows are user data that participates in comparison and promotion;
/// the remaining variants classify derived rows per the contract's
/// derived-state disposition rules.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DerivedDisposition {
    /// Authored user data — participates in comparison and promotion.
    Authored,
    /// Derived data rebuilt from authored rows after a branch workflow.
    Rebuildable,
    /// Derived data marked stale for a later rebuild.
    Staleable,
    /// Derived data dropped rather than carried across a workflow.
    Droppable,
    /// Derived data an owning contract has made authoritative for a workflow.
    Authoritative,
}

/// A capability's value at a comparable entity, summarized for comparison and
/// conflict reporting. A tombstoned (deleted) entity is [`EntitySummary::Absent`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EntitySummary {
    /// The entity is present with these authored value bytes.
    Present(Vec<u8>),
    /// The entity is absent at this branch state (deleted or tombstoned).
    Absent,
}

/// One comparable logical entity of a capability at a branch state.
///
/// `identity` is the capability's logical key with its space prefix stripped, so
/// the same entity compares equal across branches regardless of storage branch
/// id. `summary` carries the value used for equality and conflict reporting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ComparableEntity {
    identity: Vec<u8>,
    space: ProductSpace,
    summary: EntitySummary,
    version: CommitVersion,
}

impl ComparableEntity {
    pub(crate) fn new(
        identity: Vec<u8>,
        space: ProductSpace,
        summary: EntitySummary,
        version: CommitVersion,
    ) -> Self {
        Self {
            identity,
            space,
            summary,
            version,
        }
    }

    /// The capability's space-relative logical key.
    pub(crate) fn identity(&self) -> &[u8] {
        &self.identity
    }

    /// The space the entity belongs to.
    pub(crate) fn space(&self) -> &ProductSpace {
        &self.space
    }

    /// The value summary at this branch state.
    pub(crate) fn summary(&self) -> &EntitySummary {
        &self.summary
    }

    /// The commit version at which this entity's value was observed.
    pub(crate) const fn version(&self) -> CommitVersion {
        self.version
    }

    /// Whether the entity is absent (deleted or tombstoned) at this branch state.
    pub(crate) fn is_tombstone(&self) -> bool {
        matches!(self.summary, EntitySummary::Absent)
    }
}

/// The engine-side contract a data capability implements to participate in
/// branch workflows.
///
/// Adapters interpret capability rows only. They do not orchestrate workflows,
/// read storage directly, or commit; they name their row class and space
/// scoping and decode rows into comparable entities, rejecting malformed
/// capability bytes with structured diagnostics.
pub(crate) trait CapabilityBranchAdapter {
    /// The storage row class holding this capability's authored rows.
    fn row_class(&self) -> RowClass;

    /// How this capability's rows are disposed across branch workflows.
    fn derived_disposition(&self) -> DerivedDisposition;

    /// The storage-key prefix that scopes this capability's rows to one space,
    /// so a branch workflow can enumerate them with a prefix scan.
    fn space_prefix(&self, space: &ProductSpace) -> Vec<u8>;

    /// Decode one persisted row into a comparable entity, rejecting malformed
    /// capability bytes with a structured diagnostic. The `space` is the space
    /// the row was enumerated under.
    fn interpret_row(
        &self,
        space: &ProductSpace,
        row: &PersistenceReadRow,
    ) -> EngineResult<ComparableEntity>;
}

#[cfg(test)]
mod tests {
    use super::{CapabilityBranchAdapter, ComparableEntity, DerivedDisposition, EntitySummary};

    use crate::data::kv::ProductSpace;
    use crate::diagnostics::{EngineError, EngineErrorClass, EngineResult};
    use crate::persistence::{PersistenceReadRow, RowClass};

    const FAKE_PREFIX: &[u8] = b"fake/";

    /// A minimal capability used to pin the adapter trait's shape without a real
    /// capability or storage. Its rows are `fake/<space>/<identity> => value`.
    struct FakeAdapter;

    impl CapabilityBranchAdapter for FakeAdapter {
        fn row_class(&self) -> RowClass {
            RowClass::Kv
        }

        fn derived_disposition(&self) -> DerivedDisposition {
            DerivedDisposition::Authored
        }

        fn space_prefix(&self, space: &ProductSpace) -> Vec<u8> {
            let mut prefix = FAKE_PREFIX.to_vec();
            prefix.extend_from_slice(space.as_str().as_bytes());
            prefix.push(b'/');
            prefix
        }

        fn interpret_row(
            &self,
            space: &ProductSpace,
            row: &PersistenceReadRow,
        ) -> EngineResult<ComparableEntity> {
            let prefix = self.space_prefix(space);
            let identity = row
                .key()
                .strip_prefix(prefix.as_slice())
                .ok_or_else(|| {
                    EngineError::corruption(
                        "data_loss.engine.kv_key",
                        "capability row key is outside the requested space",
                    )
                })?
                .to_vec();
            let summary = if row.is_tombstone() {
                EntitySummary::Absent
            } else {
                let value = row.value().ok_or_else(|| {
                    EngineError::corruption(
                        "data_loss.engine.kv_value",
                        "capability row is present but carries no value",
                    )
                })?;
                EntitySummary::Present(value.to_vec())
            };
            Ok(ComparableEntity::new(
                identity,
                space.clone(),
                summary,
                row.commit_version(),
            ))
        }
    }

    fn space() -> ProductSpace {
        ProductSpace::new("default").expect("default is a valid space")
    }

    fn keyed(space: &ProductSpace, identity: &[u8]) -> Vec<u8> {
        let mut key = FakeAdapter.space_prefix(space);
        key.extend_from_slice(identity);
        key
    }

    #[test]
    fn interpret_row_decodes_a_present_value() {
        let space = space();
        let row =
            PersistenceReadRow::for_test(keyed(&space, b"alpha"), Some(b"one".to_vec()), false);
        let entity = FakeAdapter
            .interpret_row(&space, &row)
            .expect("decodes a present row");
        assert_eq!(entity.identity(), b"alpha");
        assert_eq!(entity.space(), &space);
        assert_eq!(entity.summary(), &EntitySummary::Present(b"one".to_vec()));
        assert!(!entity.is_tombstone());
    }

    #[test]
    fn interpret_row_maps_a_tombstone_to_absent() {
        let space = space();
        let row = PersistenceReadRow::for_test(keyed(&space, b"gone"), None, true);
        let entity = FakeAdapter
            .interpret_row(&space, &row)
            .expect("decodes a tombstone");
        assert!(entity.is_tombstone());
        assert_eq!(entity.summary(), &EntitySummary::Absent);
    }

    #[test]
    fn interpret_row_rejects_a_malformed_key_with_a_structured_diagnostic() {
        let space = space();
        let row =
            PersistenceReadRow::for_test(b"unscoped-key".to_vec(), Some(b"x".to_vec()), false);
        let error = FakeAdapter
            .interpret_row(&space, &row)
            .expect_err("a key outside the space is rejected");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.kv_key");
    }

    #[test]
    fn interpret_row_rejects_a_present_row_without_a_value() {
        let space = space();
        let row = PersistenceReadRow::for_test(keyed(&space, b"beta"), None, false);
        let error = FakeAdapter
            .interpret_row(&space, &row)
            .expect_err("a present row without a value is rejected");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.kv_value");
    }

    #[test]
    fn row_class_and_disposition_are_reported() {
        assert_eq!(FakeAdapter.row_class(), RowClass::Kv);
        assert_eq!(
            FakeAdapter.derived_disposition(),
            DerivedDisposition::Authored
        );
    }
}
