//! Executor-facing engine contract built over the storage persistence boundary.

#![deny(unsafe_code)]

pub mod api;

mod branch;
mod commit;
mod config;
mod control;
mod data;
mod diagnostics;
mod persistence;
mod runtime;

#[cfg(any(test, feature = "testkit"))]
pub mod test_support;

#[cfg(any(test, feature = "testkit"))]
pub mod testkit;

pub use api::{
    BranchCreateOutcome, BranchName, BranchService, BranchSummary, CacheOpenOptions, CloseOutcome,
    Database, DatabaseOpenOutcome, DatabaseOpenSummary, DatabaseOpenTarget,
    DurableLocalOpenOptions, EngineError, EngineErrorClass, EngineResult, KvHistory, KvHistoryRow,
    KvKey, KvListPage, KvSample, KvScanRow, KvService, KvValue, KvVersionedValue, ProductSpace,
};
