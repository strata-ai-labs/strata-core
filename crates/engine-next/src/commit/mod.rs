//! Engine-owned commit outcome vocabulary.

use strata_core_next::{CommitVersion, Timestamp};

/// Summary returned by a committed engine write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitOutcome {
    version: CommitVersion,
    timestamp: Timestamp,
    put_count: usize,
    delete_count: usize,
    durable: bool,
}

impl CommitOutcome {
    pub(crate) const fn new(
        version: CommitVersion,
        timestamp: Timestamp,
        put_count: usize,
        delete_count: usize,
        durable: bool,
    ) -> Self {
        Self {
            version,
            timestamp,
            put_count,
            delete_count,
            durable,
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
    /// Returns true when storage reported a durable commit.
    pub const fn durable(self) -> bool {
        self.durable
    }
}
