//! Strata: an embedded, branchable, time-traveling database.
//!
//! This crate is the embedded-library facade — the `stratadb` name on
//! crates.io — re-exporting the engine's public surface. Open a database,
//! pick a branch and space, and use the six data services:
//!
//! ```
//! use stratadb::{BranchName, CacheOpenOptions, Database, KvKey, KvValue, ProductSpace};
//!
//! let mut db = Database::open_cache(CacheOpenOptions::new())?.into_database();
//! let mut kv = db.kv(
//!     BranchName::new("default")?,
//!     ProductSpace::new("default")?,
//! )?;
//! kv.put(KvKey::new("greeting")?, KvValue::new(b"hello".to_vec()))?;
//! assert!(kv.get(&KvKey::new("greeting")?)?.is_some());
//! # Ok::<(), stratadb::EngineError>(())
//! ```
//!
//! Durable databases live at a path you name (`Database::open_local`);
//! cache mode keeps everything in memory. The command-level surface
//! (CLI, MCP, SDK wire) is the `strata` binary from `strata-cli`.

#![deny(unsafe_code)]

pub use strata_engine::*;
