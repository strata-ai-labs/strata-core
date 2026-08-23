//! Byte-oriented KV capability.

mod adapter;
mod outcome;
mod service;
mod types;

pub use outcome::{
    KvBatchDeleteOutcome, KvBatchPutOutcome, KvDeleteOutcome, KvHistory, KvHistoryRow, KvListPage,
    KvSample, KvScanRow, KvVersionedValue, KvWriteOutcome,
};
pub use service::KvService;
pub use types::{KvKey, KvValue, ProductSpace};
