//! Bundle export error vocabulary (M8E2 `EngineError` shape).

use std::error::Error;
use std::fmt;
use std::path::PathBuf;

/// Failure modes of bundle export, mirroring the M8E2 spec's `EngineError`
/// so the eventual trait impl (HB5) maps variants one-to-one.
#[derive(Debug)]
#[non_exhaustive]
pub enum BundleExportError {
    /// The source directory is not a valid Strata database.
    NotAStrataDb(PathBuf),
    /// The source database is locked by another process.
    Locked {
        /// The locked database path.
        path: PathBuf,
    },
    /// A requested branch does not exist in the source.
    BranchNotFound(String),
    /// Internal engine failure.
    Internal {
        /// Human-readable failure detail.
        detail: String,
    },
    /// I/O failure while copying or reading the source.
    Io {
        /// The underlying I/O error.
        source: std::io::Error,
    },
}

impl fmt::Display for BundleExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAStrataDb(path) => write!(
                formatter,
                "source directory `{}` is not a valid Strata database",
                path.display()
            ),
            Self::Locked { path } => write!(
                formatter,
                "strata DB at `{}` is locked by another process",
                path.display()
            ),
            Self::BranchNotFound(branch) => write!(
                formatter,
                "requested branch `{branch}` does not exist in the source"
            ),
            Self::Internal { detail } => write!(formatter, "internal engine error: {detail}"),
            Self::Io { source } => write!(formatter, "I/O error in engine: {source}"),
        }
    }
}

impl Error for BundleExportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source } => Some(source),
            _ => None,
        }
    }
}

impl From<std::io::Error> for BundleExportError {
    fn from(source: std::io::Error) -> Self {
        Self::Io { source }
    }
}
