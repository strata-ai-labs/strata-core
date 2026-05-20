//! Commit visible-version tracking.

use super::{CommitRuntimeError, CommitRuntimeResult, CommitVisibilityFacts};
use strata_core_next::CommitVersion;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VisibleVersionTracker {
    visible_version: CommitVersion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VisibleVersionPublish {
    Advanced {
        previous: CommitVersion,
        current: CommitVersion,
    },
    Unchanged {
        current: CommitVersion,
    },
}

impl VisibleVersionTracker {
    pub(crate) const fn new(visible_version: CommitVersion) -> Self {
        Self { visible_version }
    }

    pub(crate) const fn visible_version(self) -> CommitVersion {
        self.visible_version
    }

    pub(crate) fn publish_visible(
        &mut self,
        visible_version: CommitVersion,
    ) -> CommitRuntimeResult<VisibleVersionPublish> {
        if visible_version < self.visible_version {
            return Err(CommitRuntimeError::InvalidVisibilityFacts {
                reason: "visible version cannot regress",
            });
        }
        if visible_version == self.visible_version {
            return Ok(VisibleVersionPublish::Unchanged {
                current: self.visible_version,
            });
        }
        let previous = self.visible_version;
        self.visible_version = visible_version;
        Ok(VisibleVersionPublish::Advanced {
            previous,
            current: visible_version,
        })
    }

    pub(crate) fn publish_from_facts(
        &mut self,
        facts: CommitVisibilityFacts,
    ) -> CommitRuntimeResult<VisibleVersionPublish> {
        let Some(visible_version) = facts.visible_version() else {
            return Err(CommitRuntimeError::InvalidVisibilityFacts {
                reason: "visibility facts must include visible version",
            });
        };
        facts.validate()?;
        self.publish_visible(visible_version)
    }

    pub(crate) fn catch_up_visible_after_replay(
        &mut self,
        visible_version: CommitVersion,
    ) -> CommitRuntimeResult<VisibleVersionPublish> {
        self.publish_visible(visible_version)
    }
}

impl Default for VisibleVersionTracker {
    fn default() -> Self {
        Self::new(CommitVersion::ZERO)
    }
}
