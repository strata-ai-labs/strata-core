//! Deterministic branch artifact export (dataset clone artifacts).
//!
//! This module serializes a branch's *logical* content — never storage
//! files — into portable payload sections, per the dataset clone artifact
//! contract (`docs/architecture/engine/dataset-clone-artifact-contract.md`)
//! and the slice plan in `docs/design/hub-bundle-adapter-plan.md` (HB2).
//! Consumers (the `strata-hub` adapter) package sections into transport
//! objects; the engine knows nothing about hub wire formats.
//!
//! # Determinism
//!
//! Export bytes are a pure function of the branch's logical content at the
//! moment of export: identical logical databases produce identical bytes.
//! This property is what makes content-addressed bundles reproducible and
//! is pinned by tests. One caveat follows from content itself: event
//! records carry the wall-clock timestamp stamped at append, so two
//! databases populated at different instants differ *in content* on their
//! event logs — reproducible dataset builds that include events need an
//! explicit-timestamp append surface (planned with the import path).
//! Determinism holds because:
//!
//! - every enumeration is deterministically ordered (keys and document ids
//!   byte-lexicographic, events by sequence, collections/graphs by name,
//!   spaces by name, nodes by id, edges by (src, edge type, dst) within
//!   the per-node neighbor order);
//! - JSON payloads serialize through `serde_json` with sorted object keys;
//! - the exporter holds `&mut Database` for the whole export, so no
//!   concurrent writer can interleave (consistency by exclusivity).
//!
//! # Payload format (SAP1)
//!
//! Sections carry a flat record stream. Every record is framed as a
//! `u32` little-endian byte length followed by the record body. Within a
//! body: integers and floats are little-endian; strings and byte arrays
//! are `u32` length-prefixed; optional fields are a `u8` presence tag
//! (0 or 1) followed by the value when present. Commit timestamps ride
//! along as `u64` microseconds so imports can preserve temporal facts.
//! The format version is [`ARTIFACT_FORMAT_VERSION`]; the framing is
//! frozen once the hub adapter's golden fixtures land (HB3).

mod decode;
mod export;
mod import;

pub use decode::{decode_section, ArtifactRecord, ArtifactRecordIter};
pub use export::export_branch;
pub use import::{import_branch, BranchImportSummary};

use strata_core::Timestamp;

use crate::api::{BranchName, ProductSpace};

/// Version tag for the SAP1 payload framing described at module level.
pub const ARTIFACT_FORMAT_VERSION: u16 = 1;

/// Which data model a payload section carries.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum ArtifactModel {
    /// Key-value rows.
    Kv,
    /// JSON documents.
    Json,
    /// Event-log records.
    Event,
    /// One vector collection (config + entries).
    Vector,
    /// One property graph (meta + nodes + edges).
    Graph,
}

impl ArtifactModel {
    /// Stable lowercase label, usable in transport paths.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Kv => "kv",
            Self::Json => "json",
            Self::Event => "event",
            Self::Vector => "vector",
            Self::Graph => "graph",
        }
    }
}

/// One deterministic payload section: a single data model within a single
/// product space (qualified by collection or graph name for vector/graph).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactSection {
    space: ProductSpace,
    model: ArtifactModel,
    qualifier: Option<String>,
    record_count: u64,
    bytes: Vec<u8>,
}

impl ArtifactSection {
    /// Reassembles a section from transported parts (bundle import).
    ///
    /// `bytes` must be the concatenation of the section's transport chunks
    /// in order; malformed streams surface
    /// `corruption.engine.artifact_payload` during import.
    #[must_use]
    pub fn from_parts(
        space: ProductSpace,
        model: ArtifactModel,
        qualifier: Option<String>,
        record_count: u64,
        bytes: Vec<u8>,
    ) -> Self {
        Self::new(space, model, qualifier, record_count, bytes)
    }

    pub(crate) fn new(
        space: ProductSpace,
        model: ArtifactModel,
        qualifier: Option<String>,
        record_count: u64,
        bytes: Vec<u8>,
    ) -> Self {
        Self {
            space,
            model,
            qualifier,
            record_count,
            bytes,
        }
    }

    /// Product space this section belongs to.
    #[must_use]
    pub const fn space(&self) -> &ProductSpace {
        &self.space
    }

    /// Data model this section carries.
    #[must_use]
    pub const fn model(&self) -> ArtifactModel {
        self.model
    }

    /// Collection or graph name for vector/graph sections.
    #[must_use]
    pub fn qualifier(&self) -> Option<&str> {
        self.qualifier.as_deref()
    }

    /// Number of framed records in `bytes`.
    #[must_use]
    pub const fn record_count(&self) -> u64 {
        self.record_count
    }

    /// The SAP1 record stream.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// A branch's full logical content as deterministic payload sections.
///
/// V1 buffers sections in memory; the exporter is sized for dataset-scale
/// artifacts (curated datasets, clones), not multi-gigabyte backups.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchArtifact {
    branch: BranchName,
    spaces: Vec<ProductSpace>,
    sections: Vec<ArtifactSection>,
    max_row_timestamp: Option<Timestamp>,
}

impl BranchArtifact {
    /// Reassembles an artifact from transported parts (bundle import).
    #[must_use]
    pub fn from_parts(
        branch: BranchName,
        spaces: Vec<ProductSpace>,
        sections: Vec<ArtifactSection>,
        max_row_timestamp: Option<Timestamp>,
    ) -> Self {
        Self::new(branch, spaces, sections, max_row_timestamp)
    }

    pub(crate) fn new(
        branch: BranchName,
        spaces: Vec<ProductSpace>,
        sections: Vec<ArtifactSection>,
        max_row_timestamp: Option<Timestamp>,
    ) -> Self {
        Self {
            branch,
            spaces,
            sections,
            max_row_timestamp,
        }
    }

    /// The exported branch.
    #[must_use]
    pub const fn branch(&self) -> &BranchName {
        &self.branch
    }

    /// Every product space on the branch (including empty ones), sorted by
    /// name — space existence is content and survives clone.
    #[must_use]
    pub fn spaces(&self) -> &[ProductSpace] {
        &self.spaces
    }

    /// Payload sections in deterministic order (spaces sorted by name;
    /// kv, json, event, then vector collections and graphs by name).
    #[must_use]
    pub fn sections(&self) -> &[ArtifactSection] {
        &self.sections
    }

    /// Maximum commit timestamp observed across exported rows; `None` for
    /// an empty branch. Content-derived — the adapter uses it for
    /// manifest fields that must not depend on the wall clock.
    #[must_use]
    pub const fn max_row_timestamp(&self) -> Option<Timestamp> {
        self.max_row_timestamp
    }
}
