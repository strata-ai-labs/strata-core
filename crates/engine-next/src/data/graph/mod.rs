//! Graph core capability.

mod outcome;
mod record;
mod service;
mod types;

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

pub(crate) use record::{
    decode_graph_binding_record, decode_graph_edge_record, decode_graph_metadata_record,
    decode_graph_node_record, encode_graph_binding_record, encode_graph_edge_record,
    encode_graph_metadata_record, encode_graph_node_record, GraphBindingRecord, GraphEdgeRecord,
    GraphMetadataRecord, GraphNodeRecord,
};
