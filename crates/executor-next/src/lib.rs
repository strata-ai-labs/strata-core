//! Serializable command boundary for the rebuilt engine.

#![deny(unsafe_code)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::struct_field_names)]
#![allow(clippy::too_many_lines)]

pub mod command;
pub mod error;
pub mod executor;
pub mod output;
pub mod types;

pub use command::Command;
pub use error::{ExecutorError, ExecutorErrorClass, ExecutorResult};
pub use executor::Executor;
pub use output::Output;
pub use types::{
    BatchGetItemResult, BatchItemResult, BatchKvEntry, BranchCleanupItem, BranchItem,
    BranchParentItem, BranchStatus, Bytes, HistoryItem, SampleItem, ScanItem, VersionedValue,
    DEFAULT_BRANCH, DEFAULT_SPACE,
};
