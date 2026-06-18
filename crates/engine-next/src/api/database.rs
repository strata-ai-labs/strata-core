//! Database handle and open/close outcomes.

use std::path::PathBuf;

use crate::branch::BranchService;
use crate::control::{bootstrap_or_load, ControlPlane};
use crate::data::event::EventService;
#[cfg(any(test, feature = "testkit"))]
use crate::data::json::JsonIndexName;
use crate::data::json::JsonService;
use crate::data::kv::{KvService, ProductSpace};
use crate::data::vector::VectorService;
use crate::diagnostics::{EngineError, EngineResult};
use crate::persistence::{
    close_summary_is_durable, PersistenceOpenSummary, PersistenceOpenTarget, StoragePersistence,
};
#[cfg(any(test, feature = "testkit"))]
use crate::persistence::{encode_json_index_entry_prefix, ReadSelector, RowClass};

use super::{BranchName, CacheOpenOptions, DurableLocalOpenOptions};

/// Explicit storage target used to open a database.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DatabaseOpenTarget {
    /// Volatile cache-backed database.
    Cache,
    /// Durable local filesystem-backed database.
    DurableLocal,
}

/// Summary of an engine database open.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DatabaseOpenSummary {
    target: DatabaseOpenTarget,
    created: bool,
    durable: bool,
}

impl DatabaseOpenSummary {
    pub(crate) const fn new(
        target: DatabaseOpenTarget,
        persistence: PersistenceOpenSummary,
    ) -> Self {
        Self {
            target,
            created: persistence.created(),
            durable: persistence.durable(),
        }
    }

    #[must_use]
    /// Returns the explicit open target.
    pub const fn target(self) -> DatabaseOpenTarget {
        self.target
    }

    #[must_use]
    /// Returns true when the backing database was newly initialized.
    pub const fn created(self) -> bool {
        self.created
    }

    #[must_use]
    /// Returns true when the target is durable.
    pub const fn durable(self) -> bool {
        self.durable
    }
}

/// Database open result containing the handle and open facts.
pub struct DatabaseOpenOutcome {
    database: Database,
    summary: DatabaseOpenSummary,
}

impl DatabaseOpenOutcome {
    pub(crate) const fn new(database: Database, summary: DatabaseOpenSummary) -> Self {
        Self { database, summary }
    }

    #[must_use]
    /// Returns the open summary.
    pub const fn summary(&self) -> DatabaseOpenSummary {
        self.summary
    }

    #[must_use]
    /// Consumes the outcome and returns the database handle.
    pub fn into_database(self) -> Database {
        self.database
    }
}

/// Database close facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CloseOutcome {
    durable: bool,
    durable_synced: bool,
    idempotent: bool,
}

impl CloseOutcome {
    pub(crate) const fn new(durable: bool, durable_synced: bool, idempotent: bool) -> Self {
        Self {
            durable,
            durable_synced,
            idempotent,
        }
    }

    #[must_use]
    /// Returns true when the closed database used durable storage.
    pub const fn durable(self) -> bool {
        self.durable
    }

    #[must_use]
    /// Returns true when durable storage reported synced close effects.
    pub const fn durable_synced(self) -> bool {
        self.durable_synced
    }

    #[must_use]
    /// Returns true when this close result came from an already closed handle.
    pub const fn idempotent(self) -> bool {
        self.idempotent
    }
}

/// Executor-facing database handle.
pub struct Database {
    persistence: StoragePersistence,
    control: ControlPlane,
    summary: DatabaseOpenSummary,
    last_close: Option<CloseOutcome>,
    open: bool,
}

impl Database {
    /// Opens an explicit cache database.
    pub fn open_cache(options: CacheOpenOptions) -> EngineResult<DatabaseOpenOutcome> {
        Self::open(
            PersistenceOpenTarget::Cache,
            DatabaseOpenTarget::Cache,
            options.into_default_branch(),
        )
    }

    /// Opens an explicit durable local database.
    pub fn open_local(
        path: impl Into<PathBuf>,
        options: DurableLocalOpenOptions,
    ) -> EngineResult<DatabaseOpenOutcome> {
        Self::open(
            PersistenceOpenTarget::DurableLocal(path.into()),
            DatabaseOpenTarget::DurableLocal,
            options.into_default_branch(),
        )
    }

    /// Returns a branch service for this database.
    pub fn branches(&mut self) -> EngineResult<BranchService<'_>> {
        self.require_open()?;
        self.control.require_healthy()?;
        Ok(BranchService::new(&mut self.persistence, &mut self.control))
    }

    /// Returns a byte-oriented KV service for the selected branch and space.
    pub fn kv(&mut self, branch: BranchName, space: ProductSpace) -> EngineResult<KvService<'_>> {
        self.require_open()?;
        self.control.require_healthy()?;
        Ok(KvService::new(
            &mut self.persistence,
            &mut self.control,
            branch,
            space,
        ))
    }

    /// Returns a JSON document service for the selected branch and space.
    pub fn json(
        &mut self,
        branch: BranchName,
        space: ProductSpace,
    ) -> EngineResult<JsonService<'_>> {
        self.require_open()?;
        self.control.require_healthy()?;
        Ok(JsonService::new(
            &mut self.persistence,
            &mut self.control,
            branch,
            space,
        ))
    }

    /// Returns a vector service for the selected branch and space.
    pub fn vector(
        &mut self,
        branch: BranchName,
        space: ProductSpace,
    ) -> EngineResult<VectorService<'_>> {
        self.require_open()?;
        self.control.require_healthy()?;
        Ok(VectorService::new(
            &mut self.persistence,
            &mut self.control,
            branch,
            space,
        ))
    }

    /// Returns an event log service for the selected branch and space.
    pub fn event(
        &mut self,
        branch: BranchName,
        space: ProductSpace,
    ) -> EngineResult<EventService<'_>> {
        self.require_open()?;
        self.control.require_healthy()?;
        Ok(EventService::new(
            &mut self.persistence,
            &mut self.control,
            branch,
            space,
        ))
    }

    /// Counts visible JSON secondary-index entries for conformance tests.
    #[cfg(any(test, feature = "testkit"))]
    pub fn json_index_entry_count_for_test(
        &mut self,
        branch: &BranchName,
        space: &ProductSpace,
        index: &JsonIndexName,
    ) -> EngineResult<u64> {
        self.require_open()?;
        self.control.require_healthy()?;
        let record = self.control.lookup_branch(branch).cloned().ok_or_else(|| {
            EngineError::not_found(
                "not_found.engine.branch",
                format!("branch `{branch}` does not exist"),
            )
        })?;
        let count = self
            .persistence
            .scan_prefix(
                record.storage_branch_id(),
                RowClass::JsonIndex,
                encode_json_index_entry_prefix(space, index),
                ReadSelector::Latest,
                None,
            )?
            .into_iter()
            .filter(|row| !row.is_tombstone())
            .count();
        Ok(u64::try_from(count).unwrap_or(u64::MAX))
    }

    /// Closes the database handle.
    pub fn close(&mut self) -> EngineResult<CloseOutcome> {
        if let Some(close) = self.last_close {
            return Ok(CloseOutcome::new(
                close.durable(),
                close.durable_synced(),
                true,
            ));
        }
        let durable = self.persistence.durable();
        let close = self.persistence.close()?;
        let outcome =
            CloseOutcome::new(durable, close_summary_is_durable(close), close.idempotent());
        self.open = false;
        self.last_close = Some(outcome);
        Ok(outcome)
    }

    #[must_use]
    /// Returns the open facts captured when this handle was created.
    pub const fn open_summary(&self) -> DatabaseOpenSummary {
        self.summary
    }

    #[must_use]
    /// Returns the configured default product branch.
    pub fn default_branch(&self) -> &BranchName {
        self.control.default_branch()
    }

    fn open(
        target: PersistenceOpenTarget,
        open_target: DatabaseOpenTarget,
        default_branch: Option<BranchName>,
    ) -> EngineResult<DatabaseOpenOutcome> {
        let (mut persistence, persistence_summary) = StoragePersistence::open(target)?;
        let control = bootstrap_or_load(
            &mut persistence,
            persistence_summary.created(),
            default_branch,
        )?;
        let summary = DatabaseOpenSummary::new(open_target, persistence_summary);
        Ok(DatabaseOpenOutcome::new(
            Self {
                persistence,
                control,
                summary,
                last_close: None,
                open: true,
            },
            summary,
        ))
    }

    fn require_open(&self) -> EngineResult<()> {
        if self.open {
            Ok(())
        } else {
            Err(EngineError::closed_runtime(
                "database handle is closed and cannot accept operations",
            ))
        }
    }
}
