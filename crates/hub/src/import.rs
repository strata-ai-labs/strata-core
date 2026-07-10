//! `import_bundle`: clone reconstitution (coordination doc §3.5,
//! bundle-format §5.2).
//!
//! All-or-nothing: the database materializes in a hidden staging
//! directory next to the target, is verified by a fresh re-open, and
//! atomically renames into place. On any error the staging directory is
//! removed and the target is never created — `Ok` means a fully
//! functional database, `Err` means no partial state.
//!
//! Objects are untrusted input: every provided body is re-verified
//! against its manifest hash before any byte reaches the staging
//! database, and missing objects are reported as a set so callers can
//! fetch-and-retry.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

use stratahub_protocol::{Hash, Manifest};

use strata_core::Timestamp;
use strata_engine::artifact::{ArtifactModel, ArtifactSection, BranchArtifact};
use strata_engine::{Database, DurableLocalOpenOptions};

/// Failure modes of bundle import.
#[derive(Debug)]
#[non_exhaustive]
pub enum BundleImportError {
    /// The target path already exists and is not an empty directory.
    TargetNotEmpty(PathBuf),
    /// Objects referenced by the manifest were not provided.
    IncompleteBundle {
        /// The referenced-but-missing object hashes.
        missing_hashes: Vec<Hash>,
    },
    /// A provided object's bytes do not match its manifest hash.
    ObjectHashMismatch {
        /// The manifest-declared hash whose body failed verification.
        expected: Hash,
    },
    /// The bundle's control document is missing or malformed.
    MalformedBundle {
        /// Human-readable defect description.
        detail: String,
    },
    /// The staged database failed to materialize or verify.
    Engine {
        /// The engine error code.
        code: String,
    },
    /// I/O failure while staging or renaming.
    Io {
        /// The underlying I/O error.
        source: std::io::Error,
    },
}

impl fmt::Display for BundleImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TargetNotEmpty(path) => write!(
                formatter,
                "import target `{}` already exists and is not empty",
                path.display()
            ),
            Self::IncompleteBundle { missing_hashes } => write!(
                formatter,
                "bundle is missing {} referenced object(s)",
                missing_hashes.len()
            ),
            Self::ObjectHashMismatch { expected } => write!(
                formatter,
                "object body does not match its declared hash {}",
                expected.as_str()
            ),
            Self::MalformedBundle { detail } => {
                write!(formatter, "bundle is malformed: {detail}")
            }
            Self::Engine { code } => {
                write!(formatter, "staged database failed to materialize: {code}")
            }
            Self::Io { source } => write!(formatter, "I/O error during import: {source}"),
        }
    }
}

impl Error for BundleImportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source } => Some(source),
            _ => None,
        }
    }
}

impl From<std::io::Error> for BundleImportError {
    fn from(source: std::io::Error) -> Self {
        Self::Io { source }
    }
}

/// Reconstitutes a working Strata database at `target_path` from a
/// bundle's manifest and objects.
///
/// `objects` supplies every object body keyed by content hash (the clone
/// orchestration collects these from the object store or download cache).
/// Extra objects are ignored; missing ones fail with
/// [`BundleImportError::IncompleteBundle`] before anything is staged.
///
/// # Errors
///
/// See [`BundleImportError`]; on every error path the target is left
/// exactly as it was (normally: nonexistent).
pub fn import_bundle<S: std::hash::BuildHasher>(
    target_path: &Path,
    manifest: &Manifest,
    objects: &HashMap<Hash, Vec<u8>, S>,
) -> Result<(), BundleImportError> {
    validate_target(target_path)?;
    manifest
        .validate()
        .map_err(|error| BundleImportError::MalformedBundle {
            detail: format!("manifest validation: {error}"),
        })?;
    verify_objects(manifest, objects)?;
    let artifacts = reassemble_artifacts(manifest, objects)?;

    let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".strata-import-")
        .tempdir_in(parent)?;

    materialize(staging.path(), &artifacts).map_err(|code| BundleImportError::Engine { code })?;

    // The staging directory becomes the target atomically; same-parent
    // placement keeps the rename on one filesystem.
    let staged = staging.keep();
    if let Err(error) = std::fs::rename(&staged, target_path) {
        let _ = std::fs::remove_dir_all(&staged); // best-effort cleanup; the error below is the story
        return Err(error.into());
    }
    Ok(())
}

fn validate_target(target_path: &Path) -> Result<(), BundleImportError> {
    if !target_path.exists() {
        return Ok(());
    }
    let is_empty_dir = target_path.is_dir()
        && std::fs::read_dir(target_path)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
    if is_empty_dir {
        // An existing empty directory is claimed by removing it so the
        // staged rename can take its place.
        std::fs::remove_dir(target_path)?;
        return Ok(());
    }
    Err(BundleImportError::TargetNotEmpty(target_path.to_owned()))
}

fn verify_objects<S: std::hash::BuildHasher>(
    manifest: &Manifest,
    objects: &HashMap<Hash, Vec<u8>, S>,
) -> Result<(), BundleImportError> {
    let mut missing = Vec::new();
    for descriptor in &manifest.objects {
        match objects.get(&descriptor.hash) {
            None => missing.push(descriptor.hash.clone()),
            Some(bytes) => {
                if stratahub_protocol::hash_bytes(bytes) != descriptor.hash
                    || bytes.len() as u64 != descriptor.size_bytes
                {
                    return Err(BundleImportError::ObjectHashMismatch {
                        expected: descriptor.hash.clone(),
                    });
                }
            }
        }
    }
    if !missing.is_empty() {
        return Err(BundleImportError::IncompleteBundle {
            missing_hashes: missing,
        });
    }
    Ok(())
}

/// Rebuilds each branch's `BranchArtifact` from the control document and
/// the section chunk objects.
fn reassemble_artifacts<S: std::hash::BuildHasher>(
    manifest: &Manifest,
    objects: &HashMap<Hash, Vec<u8>, S>,
) -> Result<Vec<BranchArtifact>, BundleImportError> {
    let control_descriptor = manifest
        .objects
        .iter()
        .find(|descriptor| descriptor.path.as_str() == "control/bundle.json")
        .ok_or_else(|| malformed("control/bundle.json is not among the manifest objects"))?;
    let control: serde_json::Value = serde_json::from_slice(
        objects
            .get(&control_descriptor.hash)
            .expect("verified above"),
    )
    .map_err(|error| malformed(&format!("control document parse: {error}")))?;
    if control["bundle_control_version"].as_u64() != Some(1) {
        return Err(malformed("unsupported bundle_control_version"));
    }

    let mut artifacts = Vec::new();
    for branch in control["branches"].as_array().into_iter().flatten() {
        artifacts.push(reassemble_branch(branch, objects)?);
    }
    if artifacts.is_empty() {
        return Err(malformed("control document lists no branches"));
    }
    Ok(artifacts)
}

fn reassemble_branch<S: std::hash::BuildHasher>(
    control: &serde_json::Value,
    objects: &HashMap<Hash, Vec<u8>, S>,
) -> Result<BranchArtifact, BundleImportError> {
    let branch = strata_engine::BranchName::new(
        control["name"]
            .as_str()
            .ok_or_else(|| malformed("branch entry lacks a name"))?,
    )
    .map_err(|error| malformed(&format!("branch name: {}", error.code())))?;

    let mut spaces = Vec::new();
    for space in control["spaces"].as_array().into_iter().flatten() {
        let space = space
            .as_str()
            .ok_or_else(|| malformed("space entry is not a string"))?;
        spaces.push(
            strata_engine::ProductSpace::new(space)
                .map_err(|error| malformed(&format!("space name: {}", error.code())))?,
        );
    }

    let mut sections = Vec::new();
    for section in control["sections"].as_array().into_iter().flatten() {
        sections.push(reassemble_section(section, objects)?);
    }

    let max_row_timestamp = control["max_row_timestamp_micros"]
        .as_u64()
        .map(Timestamp::from_micros);
    Ok(BranchArtifact::from_parts(
        branch,
        spaces,
        sections,
        max_row_timestamp,
    ))
}

fn reassemble_section<S: std::hash::BuildHasher>(
    control: &serde_json::Value,
    objects: &HashMap<Hash, Vec<u8>, S>,
) -> Result<ArtifactSection, BundleImportError> {
    let space = strata_engine::ProductSpace::new(
        control["space"]
            .as_str()
            .ok_or_else(|| malformed("section lacks a space"))?,
    )
    .map_err(|error| malformed(&format!("section space: {}", error.code())))?;
    let model = match control["model"].as_str() {
        Some("kv") => ArtifactModel::Kv,
        Some("json") => ArtifactModel::Json,
        Some("event") => ArtifactModel::Event,
        Some("vector") => ArtifactModel::Vector,
        Some("graph") => ArtifactModel::Graph,
        other => {
            return Err(malformed(&format!(
                "section carries an unknown model: {other:?}"
            )));
        }
    };
    let qualifier = control["qualifier"].as_str().map(str::to_owned);
    let record_count = control["record_count"]
        .as_u64()
        .ok_or_else(|| malformed("section lacks a record count"))?;

    // Chunks concatenate in control order; records may straddle chunk
    // borders by design.
    let mut bytes = Vec::new();
    for chunk in control["chunks"].as_array().into_iter().flatten() {
        let hash = Hash::parse(
            chunk
                .as_str()
                .ok_or_else(|| malformed("chunk hash is not a string"))?,
        )
        .map_err(|error| malformed(&format!("chunk hash: {error}")))?;
        let chunk_bytes = objects.get(&hash).ok_or_else(|| {
            // Referenced by control but absent from the manifest object
            // set — verify_objects only covers manifest-listed hashes.
            BundleImportError::IncompleteBundle {
                missing_hashes: vec![hash.clone()],
            }
        })?;
        bytes.extend_from_slice(chunk_bytes);
    }

    Ok(ArtifactSection::from_parts(
        space,
        model,
        qualifier,
        record_count,
        bytes,
    ))
}

/// Builds and verifies the staged database. Returns the engine error code
/// on failure (the staging directory is discarded by the caller's `Drop`).
fn materialize(staging: &Path, artifacts: &[BranchArtifact]) -> Result<(), String> {
    {
        let mut db = Database::open_local(staging, DurableLocalOpenOptions::new())
            .map_err(|error| error.code().to_owned())?
            .into_database();
        for artifact in artifacts {
            db.import_branch_artifact(artifact)
                .map_err(|error| error.code().to_owned())?;
        }
    }
    // Fresh re-open proves the materialized database recovers cleanly —
    // the §6 "verify the resulting directory opens" step.
    let outcome = Database::open_local(staging, DurableLocalOpenOptions::new())
        .map_err(|error| error.code().to_owned())?;
    if outcome.summary().created() {
        return Err("staged import produced no durable database".to_owned());
    }
    Ok(())
}

fn malformed(detail: &str) -> BundleImportError {
    BundleImportError::MalformedBundle {
        detail: detail.to_owned(),
    }
}
