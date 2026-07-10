//! StrataHub adapter: bundle export/import and hub orchestration over the
//! Strata engine.
//!
//! This crate is the single home for hub-facing core features: the bundle
//! export adapter consumed by StrataHub's ingest pipeline, bundle import
//! for clone reconstitution, remote-tracking metadata, and (behind the
//! executor `hub.*` commands) clone orchestration. The engine never sees
//! StrataHub wire types; this crate maps engine-owned artifact payloads to
//! the StrataHub transport contract.
//!
//! Contract sources: stratahub `strata-core-requirements-for-stratahub-v1.md`
//! §3 and `stratahub-v1-bundle-format.md` §3-§5; strata-core
//! `docs/architecture/engine/dataset-clone-artifact-contract.md`; the local
//! slice plan in `docs/design/hub-bundle-adapter-plan.md`.
//!
//! The M8E2 `Engine` trait is not yet published by `stratahub-ingest`; the
//! types here follow the M8E2 shapes exactly so the eventual trait impl is
//! pure delegation (slice HB5).

#![deny(unsafe_code)]

mod error;
mod export;
mod import;
mod info;
mod remote;
mod schema_preview;

pub use error::BundleExportError;
pub use export::{
    AuxiliaryHashes, BundleObject, EngineExportOptions, EngineExportOutput, StrataCoreEngine,
};
pub use import::{import_bundle, BundleImportError};
pub use info::{engine_info, EngineInfo, CAPABILITY_REGISTRY_VERSION};
pub use remote::{
    read_remote_tracking_ref, write_remote_tracking_ref, RemoteRefError, RemoteTrackingRef,
};
