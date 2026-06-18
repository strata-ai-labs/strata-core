//! Vector API type re-exports.

pub use crate::data::vector::{
    VectorBatchDeleteOutcome, VectorBatchGetOutcome, VectorBatchUpsertOutcome,
    VectorBulkDeleteOutcome, VectorCollectionInfo, VectorCollectionName, VectorConfig,
    VectorDeleteOutcome, VectorDistanceMetric, VectorEmbedding, VectorEntry, VectorFilter,
    VectorFilterCondition, VectorFilterOp, VectorHistory, VectorHistoryRow, VectorKey,
    VectorKeyPage, VectorMetadata, VectorMetadataPatch, VectorMetadataUpdateOutcome, VectorScalar,
    VectorSearchMatch, VectorSearchResult, VectorService, VectorUpsertEntry, VectorVersionedEntry,
    VectorWriteOutcome,
};
