//! Executor Layer Tests
//!
//! Tests for the strata-executor crate which provides:
//! - Command enum (114 variants) - the instruction set
//! - Output enum - typed results
//! - Executor - stateless command dispatch
//! - Session - stateful transaction support
//! - Strata - high-level typed facade behavior

mod common;

mod adversarial;
mod branch_invariants;
mod branch_metadata_commands;
mod command_dispatch;
mod error_handling;
mod indexing_auto_embed_commands;
mod product_surface_commands;
mod serialization;
mod session_transactions;
mod storage_boundary_characterization;
