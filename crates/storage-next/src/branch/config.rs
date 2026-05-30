//! Branch-runtime configuration shell.

use super::error::{BranchRuntimeError, BranchRuntimeResult};

const DEFAULT_MAX_LEVEL_COUNT: usize = 7;
const DEFAULT_MAX_INHERITED_LAYERS: usize = 64;
const DEFAULT_MAX_FROZEN_TABLES: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchRuntimeConfig {
    level_count: usize,
    inherited_layers: usize,
    frozen_tables: usize,
}

impl BranchRuntimeConfig {
    pub(crate) fn new(
        max_level_count: usize,
        max_inherited_layers: usize,
        max_frozen_tables: usize,
    ) -> BranchRuntimeResult<Self> {
        let config = Self {
            level_count: max_level_count,
            inherited_layers: max_inherited_layers,
            frozen_tables: max_frozen_tables,
        };
        config.validate()?;
        Ok(config)
    }

    pub(crate) const fn max_level_count(self) -> usize {
        self.level_count
    }

    pub(crate) const fn max_inherited_layers(self) -> usize {
        self.inherited_layers
    }

    pub(crate) const fn max_frozen_tables(self) -> usize {
        self.frozen_tables
    }

    pub(crate) fn validate(self) -> BranchRuntimeResult<()> {
        if self.level_count == 0 {
            return Err(BranchRuntimeError::InvalidConfig {
                field: "max_level_count",
                reason: "must be nonzero",
            });
        }
        if self.level_count > usize::from(u8::MAX) + 1 {
            return Err(BranchRuntimeError::InvalidConfig {
                field: "max_level_count",
                reason: "must fit in BranchLevel",
            });
        }
        if self.inherited_layers == 0 {
            return Err(BranchRuntimeError::InvalidConfig {
                field: "max_inherited_layers",
                reason: "must be nonzero",
            });
        }
        if self.frozen_tables == 0 {
            return Err(BranchRuntimeError::InvalidConfig {
                field: "max_frozen_tables",
                reason: "must be nonzero",
            });
        }
        Ok(())
    }
}

impl Default for BranchRuntimeConfig {
    fn default() -> Self {
        Self::new(
            DEFAULT_MAX_LEVEL_COUNT,
            DEFAULT_MAX_INHERITED_LAYERS,
            DEFAULT_MAX_FROZEN_TABLES,
        )
        .expect("default branch runtime configuration is valid")
    }
}
