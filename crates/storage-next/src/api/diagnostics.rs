//! API diagnostics request shells.

use strata_core_next::BranchId;

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticsScope {
    Global,
    Branch(BranchId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticsRequest {
    scope: DiagnosticsScope,
}

impl DiagnosticsRequest {
    #[must_use]
    pub const fn new(scope: DiagnosticsScope) -> Self {
        Self { scope }
    }

    #[must_use]
    pub const fn scope(self) -> DiagnosticsScope {
        self.scope
    }
}
