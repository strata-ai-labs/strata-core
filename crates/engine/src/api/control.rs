//! Core control-plane diagnostics.

use super::BranchName;

/// Health status for a core control-plane source area.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlHealthStatus {
    /// Required control-plane facts were loaded or validated.
    Healthy,
    /// The requested control-plane facts do not exist.
    Missing,
    /// Required control-plane facts exist but violate engine invariants.
    Corrupt,
    /// The control plane is unavailable after a prior operation failure.
    Unavailable,
}

/// Branch-local space catalog diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpaceCatalogDiagnostics {
    branch: BranchName,
    status: ControlHealthStatus,
    space_count: Option<usize>,
}

impl SpaceCatalogDiagnostics {
    pub(crate) const fn new(
        branch: BranchName,
        status: ControlHealthStatus,
        space_count: Option<usize>,
    ) -> Self {
        Self {
            branch,
            status,
            space_count,
        }
    }

    #[must_use]
    /// Returns the requested branch.
    pub const fn branch(&self) -> &BranchName {
        &self.branch
    }

    #[must_use]
    /// Returns the branch-local space catalog status.
    pub const fn status(&self) -> ControlHealthStatus {
        self.status
    }

    #[must_use]
    /// Returns the number of registered user spaces when the catalog is healthy.
    pub const fn space_count(&self) -> Option<usize> {
        self.space_count
    }
}

/// Core control-plane diagnostics for an open database handle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlDiagnostics {
    identity_status: ControlHealthStatus,
    registry_status: ControlHealthStatus,
    branch_catalog_status: ControlHealthStatus,
    default_branch: BranchName,
    active_branch_count: usize,
    space_catalog: Option<SpaceCatalogDiagnostics>,
}

impl ControlDiagnostics {
    pub(crate) const fn new(
        identity_status: ControlHealthStatus,
        registry_status: ControlHealthStatus,
        branch_catalog_status: ControlHealthStatus,
        default_branch: BranchName,
        active_branch_count: usize,
        space_catalog: Option<SpaceCatalogDiagnostics>,
    ) -> Self {
        Self {
            identity_status,
            registry_status,
            branch_catalog_status,
            default_branch,
            active_branch_count,
            space_catalog,
        }
    }

    #[must_use]
    /// Returns the database identity status.
    pub const fn identity_status(&self) -> ControlHealthStatus {
        self.identity_status
    }

    #[must_use]
    /// Returns the registry status.
    pub const fn registry_status(&self) -> ControlHealthStatus {
        self.registry_status
    }

    #[must_use]
    /// Returns the branch catalog status.
    pub const fn branch_catalog_status(&self) -> ControlHealthStatus {
        self.branch_catalog_status
    }

    #[must_use]
    /// Returns the configured default product branch.
    pub const fn default_branch(&self) -> &BranchName {
        &self.default_branch
    }

    #[must_use]
    /// Returns the number of active product branches.
    pub const fn active_branch_count(&self) -> usize {
        self.active_branch_count
    }

    #[must_use]
    /// Returns diagnostics for the requested branch-local space catalog.
    pub const fn space_catalog(&self) -> Option<&SpaceCatalogDiagnostics> {
        self.space_catalog.as_ref()
    }
}
