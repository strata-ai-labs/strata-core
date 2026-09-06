//! Engine-owned commit outcome vocabulary.

use strata_core::{CommitVersion, Timestamp};

/// Per-commit durability, as storage attested it at acknowledgement time.
///
/// `Standard` is an admission fact, not a survival guarantee: the commit
/// rides the standard durability policy (synced by close, threshold, or
/// rotation) and can be lost to process kill or power failure until then.
/// Only `Always` attests the commit was synced before acknowledgement
/// (#2756: folding `Standard` into a `durable` boolean told SDK callers
/// their unsynced commits would survive a crash).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitDurability {
    /// Volatile commit (cache mode): gone when the process exits.
    NotDurable,
    /// Admitted under the standard policy: durable after the next sync
    /// point, lost with the process until then.
    Standard,
    /// Synced to durable storage before acknowledgement.
    Always,
    /// Storage could not attest this commit's durability.
    Uncertain,
}

/// Summary returned by a committed engine write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitOutcome {
    version: CommitVersion,
    timestamp: Timestamp,
    committed_at: Option<Timestamp>,
    put_count: usize,
    delete_count: usize,
    durability: CommitDurability,
}

impl CommitOutcome {
    pub(crate) const fn new(
        version: CommitVersion,
        timestamp: Timestamp,
        put_count: usize,
        delete_count: usize,
        durability: CommitDurability,
    ) -> Self {
        Self {
            version,
            timestamp,
            committed_at: None,
            put_count,
            delete_count,
            durability,
        }
    }

    /// Attaches the commit's wall-clock instant (UTC epoch micros). Distinct
    /// from `timestamp`, the logical commit-timeline clock. `None` when unknown
    /// (a replayed/imported commit, or a caller that supplied none) (#3112).
    pub(crate) const fn with_committed_at(mut self, committed_at: Option<Timestamp>) -> Self {
        self.committed_at = committed_at;
        self
    }

    pub(crate) const fn with_counts(self, put_count: usize, delete_count: usize) -> Self {
        Self {
            version: self.version,
            timestamp: self.timestamp,
            committed_at: self.committed_at,
            put_count,
            delete_count,
            durability: self.durability,
        }
    }

    #[must_use]
    /// Returns the committed version.
    pub const fn version(self) -> CommitVersion {
        self.version
    }

    #[must_use]
    /// Returns the commit's logical commit-timeline position. Not a wall-clock
    /// time — see [`committed_at`](Self::committed_at) for that.
    pub const fn timestamp(self) -> Timestamp {
        self.timestamp
    }

    #[must_use]
    /// Returns the commit's wall-clock instant (UTC epoch micros), or `None`
    /// when unknown. Distinct from the logical `timestamp` (#3112).
    pub const fn committed_at(self) -> Option<Timestamp> {
        self.committed_at
    }

    #[must_use]
    /// Returns the number of put rows.
    pub const fn put_count(self) -> usize {
        self.put_count
    }

    #[must_use]
    /// Returns the number of deleted rows.
    pub const fn delete_count(self) -> usize {
        self.delete_count
    }

    #[must_use]
    /// Returns the durability storage attested for this commit.
    pub const fn durability(self) -> CommitDurability {
        self.durability
    }
}
