//! Database handle and open/close outcomes.

use std::path::PathBuf;

use crate::branch::BranchService;
use crate::control::{bootstrap_or_load, ControlPlane};
use crate::data::kv::{KvService, ProductSpace};
use crate::diagnostics::{EngineError, EngineResult};
use crate::persistence::{
    close_summary_is_durable, PersistenceOpenSummary, PersistenceOpenTarget, StoragePersistence,
};

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
    pub fn open_cache(_options: CacheOpenOptions) -> EngineResult<DatabaseOpenOutcome> {
        Self::open(PersistenceOpenTarget::Cache, DatabaseOpenTarget::Cache)
    }

    /// Opens an explicit durable local database.
    pub fn open_local(
        path: impl Into<PathBuf>,
        _options: DurableLocalOpenOptions,
    ) -> EngineResult<DatabaseOpenOutcome> {
        Self::open(
            PersistenceOpenTarget::DurableLocal(path.into()),
            DatabaseOpenTarget::DurableLocal,
        )
    }

    /// Returns a branch service for this database.
    pub fn branches(&mut self) -> EngineResult<BranchService<'_>> {
        self.require_open()?;
        Ok(BranchService::new(&mut self.persistence, &mut self.control))
    }

    /// Returns a byte-oriented KV service for the selected branch and space.
    pub fn kv(&mut self, branch: BranchName, space: ProductSpace) -> EngineResult<KvService<'_>> {
        self.require_open()?;
        Ok(KvService::new(
            &mut self.persistence,
            &mut self.control,
            branch,
            space,
        ))
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

    fn open(
        target: PersistenceOpenTarget,
        open_target: DatabaseOpenTarget,
    ) -> EngineResult<DatabaseOpenOutcome> {
        let (mut persistence, persistence_summary) = StoragePersistence::open(target)?;
        let control = bootstrap_or_load(&mut persistence, persistence_summary.created())?;
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
