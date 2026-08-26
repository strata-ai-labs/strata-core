//! Graph capability branch adapters — comparison only.
//!
//! Graph is multi-row-class: nodes, edges, and the ontology are each an authored
//! [`RowClass`], one KV-shaped MVCC row per entity (space prefix + a length-
//! prefixed identity tuple → version byte + JSON). Each is compared by its own
//! thin adapter — a graph diff reports node, edge, and ontology changes
//! separately, which is what a reader wants. The derived rows (reverse-edge maps,
//! binding index, type index) are rebuildable and excluded.
//!
//! Graph is compared but never promoted:
//! [`CapabilityBranchAdapter::supports_promotion`] is `false` on every graph
//! adapter. A structural merge (preserving edge referential integrity, ontology
//! consistency, and branch-relative bindings, and rebuilding the derived rows)
//! needs machinery beyond the generic three-way and lands in a later slice.

use crate::branch::adapter::{
    CapabilityBranchAdapter, ComparableEntity, DerivedDisposition, EntitySummary,
};
use crate::data::kv::ProductSpace;
use crate::diagnostics::{EngineError, EngineResult};
use crate::persistence::{
    decode_graph_edge_key, decode_graph_node_key, encode_graph_edge_space_prefix,
    encode_graph_node_space_prefix, encode_graph_ontology_space_prefix, PersistenceReadRow,
    RowClass,
};

/// The space-relative identity of a graph row (its key with the space prefix
/// stripped) and the value summary, sharing the tombstone/no-value handling
/// across the three graph adapters.
fn interpret(
    row: &PersistenceReadRow,
    prefix: &[u8],
    key_code: &'static str,
    record_code: &'static str,
) -> EngineResult<ComparableEntity> {
    let identity = row
        .key()
        .strip_prefix(prefix)
        .ok_or_else(|| {
            EngineError::corruption(
                key_code,
                "stored graph row key is outside the requested space",
            )
        })?
        .to_vec();
    let summary = if row.is_tombstone() {
        EntitySummary::Absent
    } else {
        let value = row.value().ok_or_else(|| {
            EngineError::corruption(
                record_code,
                "stored graph row is present but carries no value",
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

/// The graph node capability's branch adapter (comparison only).
pub(crate) struct GraphNodeBranchAdapter;

impl CapabilityBranchAdapter for GraphNodeBranchAdapter {
    fn row_class(&self) -> RowClass {
        RowClass::GraphNode
    }

    fn derived_disposition(&self) -> DerivedDisposition {
        DerivedDisposition::Authored
    }

    fn supports_promotion(&self) -> bool {
        false
    }

    fn space_prefix(&self, space: &ProductSpace) -> Vec<u8> {
        encode_graph_node_space_prefix(space)
    }

    fn interpret_row(
        &self,
        space: &ProductSpace,
        row: &PersistenceReadRow,
    ) -> EngineResult<ComparableEntity> {
        decode_graph_node_key(space, row.key())?;
        interpret(
            row,
            &encode_graph_node_space_prefix(space),
            "data_loss.engine.graph_node_key",
            "data_loss.engine.graph_node_record",
        )
    }
}

/// The graph edge capability's branch adapter (comparison only).
pub(crate) struct GraphEdgeBranchAdapter;

impl CapabilityBranchAdapter for GraphEdgeBranchAdapter {
    fn row_class(&self) -> RowClass {
        RowClass::GraphEdge
    }

    fn derived_disposition(&self) -> DerivedDisposition {
        DerivedDisposition::Authored
    }

    fn supports_promotion(&self) -> bool {
        false
    }

    fn space_prefix(&self, space: &ProductSpace) -> Vec<u8> {
        encode_graph_edge_space_prefix(space)
    }

    fn interpret_row(
        &self,
        space: &ProductSpace,
        row: &PersistenceReadRow,
    ) -> EngineResult<ComparableEntity> {
        decode_graph_edge_key(space, row.key())?;
        interpret(
            row,
            &encode_graph_edge_space_prefix(space),
            "data_loss.engine.graph_edge_key",
            "data_loss.engine.graph_edge_record",
        )
    }
}

/// The graph ontology capability's branch adapter (comparison only). One row per
/// graph holds the whole type system, so any type or freeze-state change diffs
/// as a single modified entity.
pub(crate) struct GraphOntologyBranchAdapter;

impl CapabilityBranchAdapter for GraphOntologyBranchAdapter {
    fn row_class(&self) -> RowClass {
        RowClass::GraphOntology
    }

    fn derived_disposition(&self) -> DerivedDisposition {
        DerivedDisposition::Authored
    }

    fn supports_promotion(&self) -> bool {
        false
    }

    fn space_prefix(&self, space: &ProductSpace) -> Vec<u8> {
        encode_graph_ontology_space_prefix(space)
    }

    fn interpret_row(
        &self,
        space: &ProductSpace,
        row: &PersistenceReadRow,
    ) -> EngineResult<ComparableEntity> {
        // The ontology key has no standalone decoder; the prefix strip in
        // `interpret` validates space membership.
        interpret(
            row,
            &encode_graph_ontology_space_prefix(space),
            "data_loss.engine.graph_key",
            "data_loss.engine.graph_ontology_record",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{GraphEdgeBranchAdapter, GraphNodeBranchAdapter, GraphOntologyBranchAdapter};

    use crate::branch::adapter::{CapabilityBranchAdapter, DerivedDisposition, EntitySummary};
    use crate::data::graph::{GraphEdgeType, GraphName, GraphNodeId};
    use crate::data::kv::ProductSpace;
    use crate::diagnostics::EngineErrorClass;
    use crate::persistence::{
        encode_graph_edge_key, encode_graph_edge_space_prefix, encode_graph_node_key,
        encode_graph_node_space_prefix, encode_graph_ontology_space_prefix, PersistenceReadRow,
        RowClass,
    };

    fn space() -> ProductSpace {
        ProductSpace::new("default").expect("default is a valid space")
    }

    fn node_row(
        space: &ProductSpace,
        graph: &str,
        node: &str,
        value: Option<&[u8]>,
    ) -> PersistenceReadRow {
        let encoded = encode_graph_node_key(
            space,
            &GraphName::new(graph).expect("graph"),
            &GraphNodeId::new(node).expect("node"),
        );
        PersistenceReadRow::for_test(encoded, value.map(<[u8]>::to_vec), false)
    }

    #[test]
    fn node_adapter_decodes_a_present_row_and_reports_facets() {
        let entity = GraphNodeBranchAdapter
            .interpret_row(
                &space(),
                &node_row(&space(), "deps", "chunk", Some(b"node")),
            )
            .expect("decodes a present node row");
        assert_eq!(entity.summary(), &EntitySummary::Present(b"node".to_vec()));
        assert_eq!(GraphNodeBranchAdapter.row_class(), RowClass::GraphNode);
        assert_eq!(
            GraphNodeBranchAdapter.derived_disposition(),
            DerivedDisposition::Authored
        );
        assert!(!GraphNodeBranchAdapter.supports_promotion());
        assert_eq!(
            GraphNodeBranchAdapter.space_prefix(&space()),
            encode_graph_node_space_prefix(&space())
        );
    }

    #[test]
    fn node_identity_distinguishes_graphs_and_ids() {
        let a = GraphNodeBranchAdapter
            .interpret_row(&space(), &node_row(&space(), "one", "k", Some(b"x")))
            .expect("decodes");
        let b = GraphNodeBranchAdapter
            .interpret_row(&space(), &node_row(&space(), "two", "k", Some(b"x")))
            .expect("decodes");
        assert_ne!(
            a.identity(),
            b.identity(),
            "the graph is part of the identity"
        );
    }

    #[test]
    fn node_adapter_rejects_a_key_from_another_space() {
        let other = ProductSpace::new("other").expect("other is a valid space");
        let error = GraphNodeBranchAdapter
            .interpret_row(&space(), &node_row(&other, "deps", "chunk", Some(b"x")))
            .expect_err("a key encoded for another space is rejected");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.graph_node_key");
    }

    #[test]
    fn node_adapter_rejects_a_present_row_without_a_value() {
        let error = GraphNodeBranchAdapter
            .interpret_row(&space(), &node_row(&space(), "deps", "chunk", None))
            .expect_err("a present row without a value is rejected");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.graph_node_record");
    }

    #[test]
    fn edge_adapter_decodes_and_is_not_promotable() {
        let encoded = encode_graph_edge_key(
            &space(),
            &GraphName::new("deps").expect("graph"),
            &GraphNodeId::new("doc").expect("src"),
            &GraphEdgeType::new("contains").expect("type"),
            &GraphNodeId::new("chunk").expect("dst"),
        );
        let row = PersistenceReadRow::for_test(encoded, Some(b"edge".to_vec()), false);
        let entity = GraphEdgeBranchAdapter
            .interpret_row(&space(), &row)
            .expect("decodes a present edge row");
        assert_eq!(entity.summary(), &EntitySummary::Present(b"edge".to_vec()));
        assert_eq!(GraphEdgeBranchAdapter.row_class(), RowClass::GraphEdge);
        assert!(!GraphEdgeBranchAdapter.supports_promotion());
        assert_eq!(
            GraphEdgeBranchAdapter.space_prefix(&space()),
            encode_graph_edge_space_prefix(&space())
        );
    }

    #[test]
    fn ontology_adapter_is_authored_and_not_promotable() {
        assert_eq!(
            GraphOntologyBranchAdapter.derived_disposition(),
            DerivedDisposition::Authored
        );
        assert!(!GraphOntologyBranchAdapter.supports_promotion());
        assert_eq!(
            GraphOntologyBranchAdapter.space_prefix(&space()),
            encode_graph_ontology_space_prefix(&space())
        );
        // The ontology prefix is a distinct family from the node's.
        assert_ne!(
            GraphOntologyBranchAdapter.space_prefix(&space()),
            encode_graph_node_space_prefix(&space())
        );
    }
}
