//! Graph core capability.

mod adjacency;
mod ontology;
mod outcome;
mod record;
mod service;
mod types;

pub(crate) use adjacency::GraphAdjacencyIndexBuilder;
pub use adjacency::{GraphAdjacencyEdge, GraphAdjacencyIndex, GraphAnalyticsBudget};
pub use ontology::{
    GraphLinkTypeDef, GraphLinkTypeSummary, GraphObjectTypeDef, GraphObjectTypeSummary,
    GraphOntology, GraphOntologyFreezeOutcome, GraphOntologyStatus, GraphOntologySummary,
    GraphOntologyWriteOutcome, GraphPropertyDef, GraphTypeName,
};
pub use outcome::{
    GraphBatchOpOutcome, GraphBatchWriteOutcome, GraphBinding, GraphBindingPage,
    GraphDeleteOutcome, GraphEdge, GraphEdgeWriteOutcome, GraphInfo, GraphNamePage, GraphNeighbor,
    GraphNeighborPage, GraphNode, GraphNodePage, GraphWriteOutcome,
};
pub use service::GraphService;
pub use types::{
    GraphBatchOperation, GraphBatchWrite, GraphBindingPrimitive, GraphBindingTarget,
    GraphDirection, GraphEdgeData, GraphEdgeType, GraphEntityBinding, GraphName, GraphNodeData,
    GraphNodeId, GraphProperties,
};

pub(crate) use ontology::{
    decode_graph_ontology_record, encode_graph_ontology_record, GraphOntologyRecord,
};
pub(crate) use record::{
    decode_graph_binding_record, decode_graph_edge_record, decode_graph_metadata_record,
    decode_graph_node_record, decode_graph_type_index_record, encode_graph_binding_record,
    encode_graph_edge_record, encode_graph_metadata_record, encode_graph_node_record,
    encode_graph_type_index_record, GraphBindingRecord, GraphEdgeRecord, GraphMetadataRecord,
    GraphNodeRecord, GraphTypeIndexRecord,
};
