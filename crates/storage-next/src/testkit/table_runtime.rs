use crate::backend::memory::MemoryBackend;
use crate::backend::Backend;
use crate::format::{decode_immutable_table, encode_internal_key, FormatError, TableCompression};
use crate::layout::ObjectLayout;
use crate::row::{InternalKey, PhysicalKey, StorageRow, StorageSpaceId};
use crate::service::{TableObjectFacts, TableObjectReaderService};
use crate::table::{
    sort_table_rows_by_key, validate_strictly_sorted_unique_rows, BoundedTableCursor,
    BuiltTableArtifact, BytesTableSource, CacheInsert, CursorMergePath, FrozenTable,
    ImmutableTableBuilder, ImmutableTableReader, MergeTableCursor, MutableTable, TableBlockAddress,
    TableBlockCache, TableBlockCacheKey, TableBlockCacheKind, TableBlockCacheStats,
    TableBloomFilter, TableBloomProbe, TableBuilderConfig, TableByteSource, TableCacheConfig,
    TableCacheTableId, TableCommitRange, TableCompactionConfig, TableCompactionDecision,
    TableCompactionDropReason, TableCompactionSource, TableCompactionSourceId, TableCompactor,
    TableCursor, TableIdentity, TableInternalKeyBytes, TableKeyBound, TableKeyBounds,
    TableKeyRange, TableMemoryFacts, TablePhysicalKeyBytes, TableReaderConfig, TableRow,
    TableRuntimeConfig, TableRuntimeError, TableRuntimeFacts, TableRuntimeResult,
    MERGE_HEAP_THRESHOLD,
};
use std::collections::{btree_map::Entry, BTreeMap, VecDeque};
use std::error::Error;
use std::sync::Arc;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

use super::TestkitError;

type CursorKeyValue = (Vec<u8>, Vec<u8>);
type MergeModelItem = (Vec<u8>, usize, Vec<u8>);

const GENERATED_COMPACTION_MAX_SOURCES: u8 = 16;
const GENERATED_COMPACTION_MAX_ROWS: usize = 4096;

include!("table_runtime/outcome_contracts.rs");
include!("table_runtime/config_keys.rs");
include!("table_runtime/mutable_cursor.rs");
include!("table_runtime/builder_reader.rs");
include!("table_runtime/cache.rs");
include!("table_runtime/bloom_compaction.rs");
include!("table_runtime/perf_trace_contracts.rs");
include!("table_runtime/helpers.rs");
include!("table_runtime/tests.rs");
