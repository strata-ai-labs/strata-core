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
            put_count,
            delete_count,
            durability,
        }
    }

    pub(crate) const fn with_counts(self, put_count: usize, delete_count: usize) -> Self {
        Self {
            version: self.version,
            timestamp: self.timestamp,
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
    /// Returns the commit timestamp.
    pub const fn timestamp(self) -> Timestamp {
        self.timestamp
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
