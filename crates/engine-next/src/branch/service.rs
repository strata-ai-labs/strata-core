//! Product branch service.

use crate::branch::catalog::BranchCatalogRecord;
use crate::control::ControlPlane;
use crate::diagnostics::{EngineError, EngineResult};
use crate::persistence::StoragePersistence;

use super::BranchName;
use crate::api::{BranchCreateOutcome, BranchSummary};

/// Service for product branch operations.
pub struct BranchService<'a> {
    persistence: &'a mut StoragePersistence,
    control: &'a mut ControlPlane,
}

impl<'a> BranchService<'a> {
    pub(crate) const fn new(
        persistence: &'a mut StoragePersistence,
        control: &'a mut ControlPlane,
    ) -> Self {
        Self {
            persistence,
            control,
        }
    }

    /// Lists active product branches.
    pub fn list(&self) -> Vec<BranchSummary> {
        self.control
            .list_branches()
            .into_iter()
            .map(|record| BranchSummary::new(record.name().clone(), record.generation()))
            .collect()
    }

    /// Looks up an active product branch by name.
    pub fn get(&self, name: &BranchName) -> EngineResult<BranchSummary> {
        let record = self.control.lookup_branch(name).ok_or_else(|| {
            EngineError::not_found(
                "not_found.engine.branch",
                format!("branch `{name}` does not exist"),
            )
        })?;
        Ok(BranchSummary::new(
            record.name().clone(),
            record.generation(),
        ))
    }

    /// Creates a product branch from the current source branch head.
    pub fn create_from_head(
        &mut self,
        source: &BranchName,
        name: BranchName,
    ) -> EngineResult<BranchCreateOutcome> {
        if self.control.contains_branch(&name) {
            return Err(EngineError::conflict(
                "already_exists.engine.branch",
                format!("branch `{name}` already exists"),
            ));
        }
        let source_record = self.control.lookup_branch(source).cloned().ok_or_else(|| {
            EngineError::not_found(
                "not_found.engine.branch",
                format!("source branch `{source}` does not exist"),
            )
        })?;
        let record = BranchCatalogRecord::derived(name, source_record.branch_id());

        ControlPlane::begin_branch_create(self.persistence, &record)?;
        if let Err(error) = self.persistence.fork_branch_current(
            record.branch_id(),
            source_record.branch_id(),
            record.generation(),
        ) {
            if error.code() == "not_found.engine.persistence_history" {
                self.persistence
                    .ensure_branch_created(record.branch_id(), record.generation())?;
                self.control
                    .activate_branch(self.persistence, record.clone())?;
                return Ok(BranchCreateOutcome::new(BranchSummary::new(
                    record.name().clone(),
                    record.generation(),
                )));
            }
            let _ = ControlPlane::clear_pending_branch_create(self.persistence, &record);
            return Err(error);
        }
        self.control
            .activate_branch(self.persistence, record.clone())?;

        Ok(BranchCreateOutcome::new(BranchSummary::new(
            record.name().clone(),
            record.generation(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::BranchService;
    use crate::branch::catalog::{BranchCatalogRecord, DEFAULT_BRANCH_GENERATION};
    use crate::branch::BranchName;
    use crate::control::bootstrap_or_load;
    use crate::diagnostics::EngineErrorClass;
    use crate::persistence::{PersistenceOpenTarget, StoragePersistence};
    use strata_core_next::BranchId;

    #[test]
    fn branch_create_failure_after_pending_does_not_activate_catalog_entry() {
        let (mut persistence, summary) =
            StoragePersistence::open(PersistenceOpenTarget::Cache).expect("cache opens");
        let mut control =
            bootstrap_or_load(&mut persistence, summary.created()).expect("bootstrap succeeds");
        let feature = BranchName::new("feature").expect("valid branch");
        control.insert_branch(BranchCatalogRecord::new(
            BranchName::default_branch(),
            BranchId::from_bytes([0x44; BranchId::BYTE_LEN]),
            DEFAULT_BRANCH_GENERATION,
            None,
        ));

        let error = BranchService::new(&mut persistence, &mut control)
            .create_from_head(&BranchName::default_branch(), feature.clone())
            .expect_err("preexisting lower branch must fail");
        assert_eq!(error.class(), EngineErrorClass::NotFound);
        assert_eq!(error.code(), "not_found.engine.persistence");
        assert!(!control.contains_branch(&feature));

        let loaded =
            bootstrap_or_load(&mut persistence, false).expect("control plane reloads cleanly");
        assert!(!loaded.contains_branch(&feature));
    }
}
