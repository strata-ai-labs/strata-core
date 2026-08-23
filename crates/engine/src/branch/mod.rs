//! Product branch names, records, and services.

mod name;
mod service;

pub(crate) mod adapter;
pub(crate) mod catalog;
pub(crate) mod compare;
pub(crate) mod preview;

pub use name::BranchName;
pub(crate) use name::SYSTEM_BRANCH;
pub use service::BranchService;
