//! Byte-oriented KV capability.

mod outcome;
mod service;
mod types;

pub use outcome::{
    KvBatchDeleteOutcome, KvDeleteOutcome, KvHistory, KvHistoryRow, KvListPage, KvSample,
    KvScanRow, KvVersionedValue,
};
pub use service::KvService;
pub use types::{KvKey, KvValue, ProductSpace};
