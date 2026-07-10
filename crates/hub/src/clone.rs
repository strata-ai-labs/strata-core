//! Clone orchestration (coordination doc §3.6, `hub.clone`): the
//! headline operation — resolve a ref, fetch and verify the bundle,
//! reconstitute the database, record its origin.
//!
//! Orchestration lives here, once, behind a transport seam: the
//! [`HubTransport`] trait abstracts the wire (stratahub-client binds it
//! for real hubs; tests bind an in-process fake). Frontends reach this
//! only through the executor's `hub.*` commands.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;

use stratahub_protocol::{BranchName, DatasetName, Hash, Manifest};
use time::OffsetDateTime;

use crate::import::{import_bundle, BundleImportError};
use crate::remote::{write_remote_tracking_ref, RemoteRefError, RemoteTrackingRef};

/// The wire surface clone consumes; stratahub-client binds it for real
/// hubs (slice `HB7b`), tests bind an in-process fake. Deliberately
/// narrow: exactly the calls the §3.6 clone flow makes, in its order.
pub trait HubTransport {
    /// The transport's base URL, recorded into the tracking ref.
    fn hub_url(&self) -> String;

    /// The dataset's default branch (`GET /v1/datasets/{name}`).
    fn default_branch(&self, dataset: &DatasetName) -> Result<BranchName, CloneError>;

    /// Resolves a branch to its current manifest hash
    /// (`GET /v1/refs/{dataset}/{branch}`).
    fn resolve_ref(&self, dataset: &DatasetName, branch: &BranchName) -> Result<Hash, CloneError>;

    /// Fetches and integrity-verifies a manifest
    /// (`GET /v1/manifests/{hash}`).
    fn get_manifest(&self, hash: &Hash) -> Result<Manifest, CloneError>;

    /// Fetches one object body (`GET /v1/objects/{hash}`); transports
    /// resume partial downloads internally.
    fn get_object(&self, hash: &Hash) -> Result<Vec<u8>, CloneError>;
}

/// A clone request.
#[derive(Clone, Debug)]
pub struct CloneRequest {
    /// The dataset to clone.
    pub dataset: DatasetName,
    /// The branch to fetch; `None` means the dataset's default branch.
    pub branch: Option<BranchName>,
    /// Destination directory (must not exist, or be empty).
    pub dest: PathBuf,
}

/// Progress events, emitted in order; frontends render them (progress
/// bars, spinners) without re-implementing orchestration.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CloneProgress {
    /// The branch resolved to a manifest hash.
    Resolved {
        /// The branch being fetched.
        branch: String,
        /// The bundle's manifest hash.
        manifest_hash: String,
    },
    /// The manifest arrived; downloads are about to start.
    ManifestFetched {
        /// Number of objects to fetch.
        object_count: u64,
        /// Total bytes across all objects.
        total_bytes: u64,
    },
    /// One object landed.
    ObjectFetched {
        /// 1-based index of this object.
        index: u64,
        /// Number of objects to fetch.
        object_count: u64,
        /// This object's size.
        bytes: u64,
    },
    /// All objects verified; reconstitution started.
    Importing,
    /// The clone is complete and its origin is recorded.
    Done,
}

/// Facts about a completed clone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloneOutcome {
    /// The fetched branch.
    pub branch: BranchName,
    /// The bundle's manifest hash.
    pub manifest_hash: Hash,
    /// Number of objects fetched.
    pub object_count: u64,
    /// Total bytes fetched.
    pub total_bytes: u64,
}

/// Clone failure modes.
#[derive(Debug)]
#[non_exhaustive]
pub enum CloneError {
    /// The local engine cannot reconstitute this bundle.
    EngineIncompatible {
        /// The manifest's required version range.
        required: String,
        /// The local engine version.
        current: String,
    },
    /// A transport-level failure (wire, protocol, not-found).
    Transport {
        /// Human-readable failure detail.
        detail: String,
    },
    /// Reconstitution failed (see [`BundleImportError`]).
    Import(BundleImportError),
    /// The tracking-ref write failed after reconstitution.
    TrackingRef(RemoteRefError),
}

impl fmt::Display for CloneError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EngineIncompatible { required, current } => write!(
                formatter,
                "bundle requires engine {required}; this engine is {current}"
            ),
            Self::Transport { detail } => write!(formatter, "hub transport failed: {detail}"),
            Self::Import(error) => write!(formatter, "reconstitution failed: {error}"),
            Self::TrackingRef(error) => {
                write!(formatter, "origin recording failed: {error}")
            }
        }
    }
}

impl Error for CloneError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Import(error) => Some(error),
            Self::TrackingRef(error) => Some(error),
            _ => None,
        }
    }
}

impl From<BundleImportError> for CloneError {
    fn from(error: BundleImportError) -> Self {
        Self::Import(error)
    }
}

/// Clones a dataset: resolve → manifest → compatibility gate → objects →
/// [`import_bundle`] → tracking ref. The engine-compatibility check runs
/// BEFORE any object download, so incompatible bundles cost one manifest
/// fetch, not bandwidth (§3.6 invariant).
///
/// # Errors
///
/// See [`CloneError`]; import's all-or-nothing guarantee holds — a
/// failed clone leaves no destination state.
pub fn clone_dataset<T: HubTransport>(
    transport: &T,
    request: &CloneRequest,
    progress: &mut dyn FnMut(CloneProgress),
) -> Result<CloneOutcome, CloneError> {
    let branch = match &request.branch {
        Some(branch) => branch.clone(),
        None => transport.default_branch(&request.dataset)?,
    };
    let manifest_hash = transport.resolve_ref(&request.dataset, &branch)?;
    progress(CloneProgress::Resolved {
        branch: branch.as_str().to_owned(),
        manifest_hash: manifest_hash.as_str().to_owned(),
    });

    let manifest = transport.get_manifest(&manifest_hash)?;
    check_engine_compatibility(&manifest)?;
    progress(CloneProgress::ManifestFetched {
        object_count: manifest.object_count,
        total_bytes: manifest.total_size_bytes,
    });

    let mut objects = HashMap::with_capacity(manifest.objects.len());
    let object_count = manifest.objects.len() as u64;
    for (index, descriptor) in manifest.objects.iter().enumerate() {
        let bytes = transport.get_object(&descriptor.hash)?;
        progress(CloneProgress::ObjectFetched {
            index: index as u64 + 1,
            object_count,
            bytes: bytes.len() as u64,
        });
        objects.insert(descriptor.hash.clone(), bytes);
    }

    progress(CloneProgress::Importing);
    import_bundle(&request.dest, &manifest, &objects)?;

    let tracking_ref = RemoteTrackingRef::for_clone(
        transport.hub_url(),
        request.dataset.clone(),
        branch.clone(),
        &manifest,
        manifest_hash.clone(),
        OffsetDateTime::now_utc(),
    );
    write_remote_tracking_ref(&request.dest, &tracking_ref).map_err(CloneError::TrackingRef)?;
    progress(CloneProgress::Done);

    Ok(CloneOutcome {
        branch,
        manifest_hash,
        object_count,
        total_bytes: manifest.total_size_bytes,
    })
}

/// The pre-download gate: the manifest's semver range against the local
/// engine version.
fn check_engine_compatibility(manifest: &Manifest) -> Result<(), CloneError> {
    let requirement = manifest
        .engine_compatibility
        .required_engine_version
        .parse::<semver::VersionReq>()
        .map_err(|error| CloneError::Transport {
            detail: format!("manifest carries a malformed engine requirement: {error}"),
        })?;
    let info = crate::engine_info();
    let current = info
        .version
        .parse::<semver::Version>()
        .expect("engine version is semver");
    if !requirement.matches(&current) {
        return Err(CloneError::EngineIncompatible {
            required: manifest
                .engine_compatibility
                .required_engine_version
                .clone(),
            current: info.version,
        });
    }
    Ok(())
}
