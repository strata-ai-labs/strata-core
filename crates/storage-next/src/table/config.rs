//! Table-runtime configuration shells.

use super::{TableRuntimeError, TableRuntimeResult};
use crate::format::TableCompression;

const DEFAULT_TARGET_DATA_BLOCK_SIZE: u32 = 64 * 1024;
const DEFAULT_ROWS_PER_BLOCK: usize = 256;
const DEFAULT_CACHE_CAPACITY_BYTES: usize = 64 * 1024 * 1024;
const DEFAULT_COMPACTION_TARGET_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;
const DEFAULT_COMPACTION_MAX_OUTPUT_TABLES: usize = 1_024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableRuntimeConfig {
    builder: TableBuilderConfig,
    reader: TableReaderConfig,
    cache: TableCacheConfig,
    compaction: TableCompactionConfig,
}

impl TableRuntimeConfig {
    pub(crate) fn new(
        builder: TableBuilderConfig,
        reader: TableReaderConfig,
        cache: TableCacheConfig,
        compaction: TableCompactionConfig,
    ) -> TableRuntimeResult<Self> {
        builder.validate()?;
        cache.validate()?;
        compaction.validate()?;
        Ok(Self {
            builder,
            reader,
            cache,
            compaction,
        })
    }

    pub(crate) const fn builder(&self) -> &TableBuilderConfig {
        &self.builder
    }

    pub(crate) const fn reader(&self) -> &TableReaderConfig {
        &self.reader
    }

    pub(crate) const fn cache(&self) -> &TableCacheConfig {
        &self.cache
    }

    pub(crate) const fn compaction(&self) -> &TableCompactionConfig {
        &self.compaction
    }
}

impl Default for TableRuntimeConfig {
    fn default() -> Self {
        Self::new(
            TableBuilderConfig::default(),
            TableReaderConfig::default(),
            TableCacheConfig::default(),
            TableCompactionConfig::default(),
        )
        .expect("default table runtime configuration is valid")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TableBuilderConfig {
    target_data_block_size: u32,
    rows_per_block: usize,
    compression: TableCompression,
}

impl TableBuilderConfig {
    pub(crate) fn new(
        target_data_block_size: u32,
        rows_per_block: usize,
        compression: TableCompression,
    ) -> TableRuntimeResult<Self> {
        let config = Self {
            target_data_block_size,
            rows_per_block,
            compression,
        };
        config.validate()?;
        Ok(config)
    }

    pub(crate) const fn target_data_block_size(self) -> u32 {
        self.target_data_block_size
    }

    pub(crate) const fn rows_per_block(self) -> usize {
        self.rows_per_block
    }

    pub(crate) const fn compression(self) -> TableCompression {
        self.compression
    }

    pub(crate) fn validate(self) -> TableRuntimeResult<()> {
        if self.target_data_block_size == 0 {
            return Err(TableRuntimeError::InvalidConfig {
                field: "target_data_block_size",
                reason: "must be nonzero",
            });
        }
        if self.rows_per_block == 0 {
            return Err(TableRuntimeError::InvalidConfig {
                field: "rows_per_block",
                reason: "must be nonzero",
            });
        }
        Ok(())
    }
}

impl Default for TableBuilderConfig {
    fn default() -> Self {
        Self {
            target_data_block_size: DEFAULT_TARGET_DATA_BLOCK_SIZE,
            rows_per_block: DEFAULT_ROWS_PER_BLOCK,
            compression: TableCompression::Uncompressed,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TableReaderConfig {
    cache_enabled: bool,
    validate_on_open: bool,
}

impl TableReaderConfig {
    pub(crate) const fn new(cache_enabled: bool, validate_on_open: bool) -> Self {
        Self {
            cache_enabled,
            validate_on_open,
        }
    }

    pub(crate) const fn cache_enabled(self) -> bool {
        self.cache_enabled
    }

    pub(crate) const fn validate_on_open(self) -> bool {
        self.validate_on_open
    }
}

impl Default for TableReaderConfig {
    fn default() -> Self {
        Self {
            cache_enabled: true,
            validate_on_open: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TableCacheConfig {
    enabled: bool,
    capacity_bytes: usize,
}

impl TableCacheConfig {
    pub(crate) fn new(enabled: bool, capacity_bytes: usize) -> TableRuntimeResult<Self> {
        let config = Self {
            enabled,
            capacity_bytes,
        };
        config.validate()?;
        Ok(config)
    }

    pub(crate) const fn enabled(self) -> bool {
        self.enabled
    }

    pub(crate) const fn capacity_bytes(self) -> usize {
        self.capacity_bytes
    }

    pub(crate) fn validate(self) -> TableRuntimeResult<()> {
        if self.enabled && self.capacity_bytes == 0 {
            return Err(TableRuntimeError::InvalidConfig {
                field: "cache_capacity_bytes",
                reason: "enabled cache must have positive capacity",
            });
        }
        Ok(())
    }
}

impl Default for TableCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            capacity_bytes: DEFAULT_CACHE_CAPACITY_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TableCompactionConfig {
    target_output_bytes: u64,
    max_output_tables: usize,
}

impl TableCompactionConfig {
    pub(crate) fn new(
        target_output_bytes: u64,
        max_output_tables: usize,
    ) -> TableRuntimeResult<Self> {
        let config = Self {
            target_output_bytes,
            max_output_tables,
        };
        config.validate()?;
        Ok(config)
    }

    pub(crate) const fn target_output_bytes(self) -> u64 {
        self.target_output_bytes
    }

    pub(crate) const fn max_output_tables(self) -> usize {
        self.max_output_tables
    }

    pub(crate) fn validate(self) -> TableRuntimeResult<()> {
        if self.target_output_bytes == 0 {
            return Err(TableRuntimeError::InvalidConfig {
                field: "target_output_bytes",
                reason: "must be nonzero",
            });
        }
        if self.max_output_tables == 0 {
            return Err(TableRuntimeError::InvalidConfig {
                field: "max_output_tables",
                reason: "must be nonzero",
            });
        }
        Ok(())
    }
}

impl Default for TableCompactionConfig {
    fn default() -> Self {
        Self {
            target_output_bytes: DEFAULT_COMPACTION_TARGET_OUTPUT_BYTES,
            max_output_tables: DEFAULT_COMPACTION_MAX_OUTPUT_TABLES,
        }
    }
}
