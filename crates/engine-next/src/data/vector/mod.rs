//! Vector capability.

mod distance;
mod outcome;
mod record;
mod service;
mod types;

pub use outcome::{
    VectorBatchDeleteOutcome, VectorBatchGetOutcome, VectorBatchUpsertOutcome,
    VectorBulkDeleteOutcome, VectorCollectionInfo, VectorDeleteOutcome, VectorEntry, VectorHistory,
    VectorHistoryRow, VectorKeyPage, VectorMetadataUpdateOutcome, VectorSearchMatch,
    VectorSearchResult, VectorVersionedEntry, VectorWriteOutcome,
};
pub use service::VectorService;
pub use types::{
    VectorCollectionName, VectorConfig, VectorDistanceMetric, VectorEmbedding, VectorFilter,
    VectorFilterCondition, VectorFilterOp, VectorKey, VectorMetadata, VectorMetadataPatch,
    VectorScalar, VectorUpsertEntry,
};

pub(crate) use distance::vector_score;
pub(crate) use record::{
    decode_collection_config, decode_vector_record, encode_collection_config, encode_vector_record,
    VectorRecord,
};
