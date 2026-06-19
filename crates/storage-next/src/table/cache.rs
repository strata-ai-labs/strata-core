//! Table-local cache and read accelerators.

use super::{TableCacheConfig, TableRuntimeError, TableRuntimeResult};
use crate::observability::perf_trace;
use std::collections::{hash_map::DefaultHasher, BTreeMap, VecDeque};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, MutexGuard};

const MAX_TABLE_CACHE_ID_BYTES: usize = 512;
const MAX_BLOOM_BYTES: usize = 16 * 1024 * 1024;
const MAX_BLOOM_PROBES: u8 = 30;
const MAX_DEBUG_BYTES: usize = 16;
const TABLE_BLOCK_CACHE_MAX_SHARDS: usize = 16;
const TABLE_BLOCK_CACHE_TARGET_SHARD_BYTES: usize = 64 * 1024;

#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct TableCacheTableId {
    bytes: Vec<u8>,
}

impl TableCacheTableId {
    pub(crate) fn new(bytes: impl Into<Vec<u8>>) -> TableRuntimeResult<Self> {
        let bytes = bytes.into();
        if bytes.is_empty() {
            return Err(TableRuntimeError::InvalidConfig {
                field: "table_cache_id",
                reason: "must not be empty",
            });
        }
        if bytes.len() > MAX_TABLE_CACHE_ID_BYTES {
            return Err(TableRuntimeError::InvalidConfig {
                field: "table_cache_id",
                reason: "is too large",
            });
        }
        Ok(Self { bytes })
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Debug for TableCacheTableId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TableCacheTableId(")?;
        fmt_bounded_bytes(formatter, self.as_slice())?;
        formatter.write_str(")")
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum TableBlockCacheKind {
    Data,
    Index,
    Properties,
    Accelerator,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct TableBlockAddress {
    kind: TableBlockCacheKind,
    offset: u64,
    length: u32,
    ordinal: Option<u32>,
}

impl TableBlockAddress {
    pub(crate) fn new(
        kind: TableBlockCacheKind,
        offset: u64,
        length: u32,
        ordinal: Option<u32>,
    ) -> TableRuntimeResult<Self> {
        if length == 0 {
            return Err(TableRuntimeError::InvalidRange {
                field: "block_length",
            });
        }
        offset
            .checked_add(u64::from(length))
            .ok_or(TableRuntimeError::InvalidRange {
                field: "block_range",
            })?;
        Ok(Self {
            kind,
            offset,
            length,
            ordinal,
        })
    }

    pub(crate) const fn kind(&self) -> TableBlockCacheKind {
        self.kind
    }

    pub(crate) const fn offset(&self) -> u64 {
        self.offset
    }

    pub(crate) const fn length(&self) -> u32 {
        self.length
    }

    pub(crate) const fn ordinal(&self) -> Option<u32> {
        self.ordinal
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct TableBlockCacheKey {
    table: TableCacheTableId,
    address: TableBlockAddress,
}

impl TableBlockCacheKey {
    pub(crate) fn new(table: TableCacheTableId, address: TableBlockAddress) -> Self {
        Self { table, address }
    }

    pub(crate) const fn table(&self) -> &TableCacheTableId {
        &self.table
    }

    pub(crate) const fn address(&self) -> &TableBlockAddress {
        &self.address
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TableBlockCacheStats {
    hits: u64,
    misses: u64,
    inserts: u64,
    duplicate_inserts: u64,
    evictions: u64,
    removes: u64,
    table_invalidations: u64,
    clears: u64,
    skipped_oversized: u64,
    skipped_disabled: u64,
    entries: usize,
    bytes: usize,
    capacity_bytes: usize,
}

impl TableBlockCacheStats {
    pub(crate) const fn hits(self) -> u64 {
        self.hits
    }

    pub(crate) const fn misses(self) -> u64 {
        self.misses
    }

    pub(crate) const fn inserts(self) -> u64 {
        self.inserts
    }

    pub(crate) const fn duplicate_inserts(self) -> u64 {
        self.duplicate_inserts
    }

    pub(crate) const fn evictions(self) -> u64 {
        self.evictions
    }

    pub(crate) const fn removes(self) -> u64 {
        self.removes
    }

    pub(crate) const fn table_invalidations(self) -> u64 {
        self.table_invalidations
    }

    pub(crate) const fn clears(self) -> u64 {
        self.clears
    }

    pub(crate) const fn skipped_oversized(self) -> u64 {
        self.skipped_oversized
    }

    pub(crate) const fn skipped_disabled(self) -> u64 {
        self.skipped_disabled
    }

    pub(crate) const fn entries(self) -> usize {
        self.entries
    }

    pub(crate) const fn bytes(self) -> usize {
        self.bytes
    }

    pub(crate) const fn capacity_bytes(self) -> usize {
        self.capacity_bytes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CacheInsert {
    Inserted(Arc<[u8]>),
    DuplicateExisting(Arc<[u8]>),
    SkippedDisabled(Arc<[u8]>),
    SkippedOversized(Arc<[u8]>),
}

impl CacheInsert {
    pub(crate) fn bytes(&self) -> Arc<[u8]> {
        match self {
            Self::Inserted(bytes)
            | Self::DuplicateExisting(bytes)
            | Self::SkippedDisabled(bytes)
            | Self::SkippedOversized(bytes) => Arc::clone(bytes),
        }
    }
}

#[derive(Debug)]
pub(crate) struct TableBlockCache {
    shards: Vec<CacheShard>,
}

#[derive(Debug)]
struct CacheShard {
    state: Mutex<CacheState>,
}

#[derive(Debug)]
struct CacheState {
    enabled: bool,
    capacity_bytes: usize,
    bytes: usize,
    entries: BTreeMap<TableBlockCacheKey, Arc<[u8]>>,
    recency: VecDeque<TableBlockCacheKey>,
    stats: TableBlockCacheStats,
}

impl TableBlockCache {
    pub(crate) fn new(config: TableCacheConfig) -> Self {
        let capacities = shard_capacities(config.capacity_bytes(), cache_shard_count(config));
        Self {
            shards: capacities
                .into_iter()
                .map(|capacity_bytes| CacheShard {
                    state: Mutex::new(CacheState {
                        enabled: config.enabled(),
                        capacity_bytes,
                        bytes: 0,
                        entries: BTreeMap::new(),
                        recency: VecDeque::new(),
                        stats: TableBlockCacheStats {
                            capacity_bytes,
                            ..TableBlockCacheStats::default()
                        },
                    }),
                })
                .collect(),
        }
    }

    pub(crate) fn disabled() -> Self {
        let config = TableCacheConfig::new(false, 0)
            .expect("disabled table cache with zero capacity is valid");
        Self::new(config)
    }

    pub(crate) fn enabled(&self) -> bool {
        self.shards
            .first()
            .expect("table block cache always has at least one shard")
            .lock_state()
            .enabled
    }

    /// Total bytes currently resident across all shards. Folded into the database-wide memory
    /// total so cached blocks count against the budget.
    pub(crate) fn current_bytes(&self) -> u64 {
        self.shards.iter().fold(0u64, |total, shard| {
            total.saturating_add(u64::try_from(shard.lock_state().bytes).unwrap_or(u64::MAX))
        })
    }

    pub(crate) fn get(&self, key: &TableBlockCacheKey) -> Option<Arc<[u8]>> {
        let mut state = self.shard_for_key(key).lock_state();
        let bytes = state.entries.get(key).cloned();
        if let Some(bytes) = bytes {
            state.stats.hits = state.stats.hits.saturating_add(1);
            perf_trace::record_table_cache_hit();
            touch_recency(&mut state.recency, key);
            Some(bytes)
        } else {
            state.stats.misses = state.stats.misses.saturating_add(1);
            perf_trace::record_table_cache_miss();
            None
        }
    }

    pub(crate) fn insert(
        &self,
        key: TableBlockCacheKey,
        bytes: Arc<[u8]>,
    ) -> TableRuntimeResult<CacheInsert> {
        if bytes.is_empty() {
            return Err(TableRuntimeError::InvalidRange {
                field: "cache_charge",
            });
        }

        let mut state = self.shard_for_key(&key).lock_state();
        if !state.enabled {
            state.stats.skipped_disabled = state.stats.skipped_disabled.saturating_add(1);
            perf_trace::record_table_cache_skipped_insert();
            return Ok(CacheInsert::SkippedDisabled(bytes));
        }

        if let Some(existing) = state.entries.get(&key).cloned() {
            state.stats.duplicate_inserts = state.stats.duplicate_inserts.saturating_add(1);
            touch_recency(&mut state.recency, &key);
            return Ok(CacheInsert::DuplicateExisting(existing));
        }

        if bytes.len() > state.capacity_bytes {
            state.stats.skipped_oversized = state.stats.skipped_oversized.saturating_add(1);
            perf_trace::record_table_cache_skipped_insert();
            return Ok(CacheInsert::SkippedOversized(bytes));
        }

        evict_to_fit(&mut state, bytes.len());
        if state.bytes.saturating_add(bytes.len()) > state.capacity_bytes {
            state.stats.skipped_oversized = state.stats.skipped_oversized.saturating_add(1);
            perf_trace::record_table_cache_skipped_insert();
            return Ok(CacheInsert::SkippedOversized(bytes));
        }

        state.bytes = state.bytes.saturating_add(bytes.len());
        let recency_key = key.clone();
        state.entries.insert(key, Arc::clone(&bytes));
        touch_recency(&mut state.recency, &recency_key);
        state.stats.inserts = state.stats.inserts.saturating_add(1);
        perf_trace::record_table_cache_insert();
        refresh_gauges(&mut state);
        Ok(CacheInsert::Inserted(bytes))
    }

    pub(crate) fn remove(&self, key: &TableBlockCacheKey) -> bool {
        let mut state = self.shard_for_key(key).lock_state();
        if let Some(bytes) = state.entries.remove(key) {
            state.bytes = state.bytes.saturating_sub(bytes.len());
            remove_from_recency(&mut state.recency, key);
            state.stats.removes = state.stats.removes.saturating_add(1);
            refresh_gauges(&mut state);
            true
        } else {
            false
        }
    }

    pub(crate) fn remove_table(&self, table: &TableCacheTableId) -> usize {
        let mut removed = 0usize;
        let mut states = self.lock_all_states();
        for state in &mut states {
            let keys = state
                .entries
                .keys()
                .filter(|key| key.table() == table)
                .cloned()
                .collect::<Vec<_>>();
            removed = removed.saturating_add(keys.len());
            for key in keys {
                if let Some(bytes) = state.entries.remove(&key) {
                    state.bytes = state.bytes.saturating_sub(bytes.len());
                }
                remove_from_recency(&mut state.recency, &key);
            }
            refresh_gauges(state);
        }
        if removed > 0 {
            let first_state = states
                .first_mut()
                .expect("table block cache always has at least one shard");
            first_state.stats.table_invalidations =
                first_state.stats.table_invalidations.saturating_add(1);
            refresh_gauges(first_state);
        }
        removed
    }

    pub(crate) fn clear(&self) {
        let mut states = self.lock_all_states();
        for state in &mut states {
            state.entries.clear();
            state.recency.clear();
            state.bytes = 0;
            refresh_gauges(state);
        }
        let first_state = states
            .first_mut()
            .expect("table block cache always has at least one shard");
        first_state.stats.clears = first_state.stats.clears.saturating_add(1);
        refresh_gauges(first_state);
    }

    pub(crate) fn resize(&self, capacity_bytes: usize) {
        let capacities = shard_capacities(capacity_bytes, self.shards.len());
        let mut states = self.lock_all_states();
        for (state, capacity_bytes) in states.iter_mut().zip(capacities) {
            state.capacity_bytes = capacity_bytes;
            state.stats.capacity_bytes = capacity_bytes;
            evict_to_capacity(state);
            refresh_gauges(state);
        }
    }

    pub(crate) fn stats(&self) -> TableBlockCacheStats {
        let mut stats = TableBlockCacheStats::default();
        let shard_guards = self.lock_all_states();
        for cache_state in &shard_guards {
            let mut shard_stats = cache_state.stats;
            shard_stats.entries = cache_state.entries.len();
            shard_stats.bytes = cache_state.bytes;
            shard_stats.capacity_bytes = cache_state.capacity_bytes;
            stats = merge_cache_stats(stats, shard_stats);
        }
        stats
    }

    #[cfg(test)]
    pub(crate) fn poison_for_test(&self) {
        for shard in &self.shards {
            let _ = std::panic::catch_unwind(|| {
                let _guard = shard
                    .state
                    .lock()
                    .expect("test lock should not already be poisoned");
                panic!("poison table cache mutex for recovery test");
            });
        }
    }

    #[cfg(test)]
    pub(crate) fn shard_count_for_test(&self) -> usize {
        self.shards.len()
    }

    #[cfg(test)]
    pub(crate) fn shard_index_for_test(&self, key: &TableBlockCacheKey) -> usize {
        shard_index(key, self.shards.len())
    }

    fn shard_for_key(&self, key: &TableBlockCacheKey) -> &CacheShard {
        &self.shards[shard_index(key, self.shards.len())]
    }

    fn lock_all_states(&self) -> Vec<MutexGuard<'_, CacheState>> {
        self.shards.iter().map(CacheShard::lock_state).collect()
    }
}

impl CacheShard {
    fn lock_state(&self) -> std::sync::MutexGuard<'_, CacheState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TableBloomProbe {
    DefinitelyAbsent,
    MaybePresent,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableBloomFilter {
    bits: Vec<u8>,
    bit_count: usize,
    probes: u8,
    key_count: usize,
}

impl TableBloomFilter {
    pub(crate) fn build<'a>(
        keys: impl IntoIterator<Item = &'a [u8]>,
        bits_per_key: usize,
    ) -> TableRuntimeResult<Self> {
        if bits_per_key == 0 {
            return Err(TableRuntimeError::InvalidConfig {
                field: "bits_per_key",
                reason: "must be nonzero",
            });
        }

        let keys = keys.into_iter().collect::<Vec<_>>();
        if keys.is_empty() {
            return Ok(Self {
                bits: Vec::new(),
                bit_count: 0,
                probes: 1,
                key_count: 0,
            });
        }

        let bit_count = keys
            .len()
            .checked_mul(bits_per_key)
            .ok_or(TableRuntimeError::InvalidRange {
                field: "bloom_bits",
            })?
            .max(8);
        let byte_count = bit_count.div_ceil(8);
        if byte_count > MAX_BLOOM_BYTES {
            return Err(TableRuntimeError::InvalidRange {
                field: "bloom_bytes",
            });
        }

        let probes = optimal_probe_count(bits_per_key);
        let mut filter = Self {
            bits: vec![0; byte_count],
            bit_count,
            probes,
            key_count: keys.len(),
        };
        for key in keys {
            filter.insert_key(key);
        }
        Ok(filter)
    }

    pub(crate) fn might_contain(&self, key: &[u8]) -> TableBloomProbe {
        if self.key_count == 0 {
            perf_trace::record_table_filter_negative_probe();
            return TableBloomProbe::DefinitelyAbsent;
        }
        if self.bits.is_empty() || self.bit_count == 0 || self.probes == 0 {
            perf_trace::record_table_filter_absent_probe();
            return TableBloomProbe::Unavailable;
        }
        for probe in 0..self.probes {
            let bit = bloom_bit(key, probe, self.bit_count);
            if self.bits[bit / 8] & (1 << (bit % 8)) == 0 {
                perf_trace::record_table_filter_negative_probe();
                return TableBloomProbe::DefinitelyAbsent;
            }
        }
        perf_trace::record_table_filter_positive_probe();
        TableBloomProbe::MaybePresent
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.key_count == 0
    }

    pub(crate) fn approximate_size_bytes(&self) -> usize {
        self.bits.len()
    }

    pub(crate) const fn key_count(&self) -> usize {
        self.key_count
    }

    pub(crate) const fn probes(&self) -> u8 {
        self.probes
    }

    fn insert_key(&mut self, key: &[u8]) {
        for probe in 0..self.probes {
            let bit = bloom_bit(key, probe, self.bit_count);
            self.bits[bit / 8] |= 1 << (bit % 8);
        }
    }
}

fn evict_to_fit(state: &mut CacheState, incoming_bytes: usize) {
    while state.bytes.saturating_add(incoming_bytes) > state.capacity_bytes {
        if !evict_one(state) {
            break;
        }
    }
}

fn evict_to_capacity(state: &mut CacheState) {
    while state.bytes > state.capacity_bytes {
        if !evict_one(state) {
            break;
        }
    }
}

fn evict_one(state: &mut CacheState) -> bool {
    while let Some(key) = state.recency.pop_front() {
        if let Some(bytes) = state.entries.remove(&key) {
            state.bytes = state.bytes.saturating_sub(bytes.len());
            state.stats.evictions = state.stats.evictions.saturating_add(1);
            refresh_gauges(state);
            return true;
        }
    }
    false
}

fn touch_recency(recency: &mut VecDeque<TableBlockCacheKey>, key: &TableBlockCacheKey) {
    remove_from_recency(recency, key);
    recency.push_back(key.clone());
}

fn remove_from_recency(recency: &mut VecDeque<TableBlockCacheKey>, key: &TableBlockCacheKey) {
    if let Some(index) = recency.iter().position(|candidate| candidate == key) {
        recency.remove(index);
    }
}

fn refresh_gauges(state: &mut CacheState) {
    state.stats.entries = state.entries.len();
    state.stats.bytes = state.bytes;
    state.stats.capacity_bytes = state.capacity_bytes;
}

fn cache_shard_count(config: TableCacheConfig) -> usize {
    if !config.enabled() || config.capacity_bytes() == 0 {
        return 1;
    }
    (config.capacity_bytes() / TABLE_BLOCK_CACHE_TARGET_SHARD_BYTES)
        .clamp(1, TABLE_BLOCK_CACHE_MAX_SHARDS)
}

fn shard_capacities(capacity_bytes: usize, shard_count: usize) -> Vec<usize> {
    let shard_count = shard_count.max(1);
    let base = capacity_bytes / shard_count;
    let remainder = capacity_bytes % shard_count;
    (0..shard_count)
        .map(|index| base + usize::from(index < remainder))
        .collect()
}

fn shard_index(key: &TableBlockCacheKey, shard_count: usize) -> usize {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    let shard_count = u64::try_from(shard_count.max(1)).expect("shard count fits in u64");
    usize::try_from(hasher.finish() % shard_count).expect("shard index fits in usize")
}

fn merge_cache_stats(
    mut left: TableBlockCacheStats,
    right: TableBlockCacheStats,
) -> TableBlockCacheStats {
    left.hits = left.hits.saturating_add(right.hits);
    left.misses = left.misses.saturating_add(right.misses);
    left.inserts = left.inserts.saturating_add(right.inserts);
    left.duplicate_inserts = left
        .duplicate_inserts
        .saturating_add(right.duplicate_inserts);
    left.evictions = left.evictions.saturating_add(right.evictions);
    left.removes = left.removes.saturating_add(right.removes);
    left.table_invalidations = left
        .table_invalidations
        .saturating_add(right.table_invalidations);
    left.clears = left.clears.saturating_add(right.clears);
    left.skipped_oversized = left
        .skipped_oversized
        .saturating_add(right.skipped_oversized);
    left.skipped_disabled = left.skipped_disabled.saturating_add(right.skipped_disabled);
    left.entries = left.entries.saturating_add(right.entries);
    left.bytes = left.bytes.saturating_add(right.bytes);
    left.capacity_bytes = left.capacity_bytes.saturating_add(right.capacity_bytes);
    left
}

fn optimal_probe_count(bits_per_key: usize) -> u8 {
    let probes = bits_per_key.saturating_mul(693).saturating_add(999) / 1000;
    u8::try_from(probes.clamp(1, usize::from(MAX_BLOOM_PROBES)))
        .expect("clamped bloom probe count fits in u8")
}

fn bloom_bit(key: &[u8], probe: u8, bit_count: usize) -> usize {
    let mixed = fnv1a64(key, 0xcbf2_9ce4_8422_2325 ^ u64::from(probe))
        .wrapping_add(u64::from(probe).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    let bit_count = u64::try_from(bit_count).expect("bloom bit count fits in u64");
    usize::try_from(mixed % bit_count).expect("bloom bit index fits in usize")
}

fn fnv1a64(bytes: &[u8], seed: u64) -> u64 {
    let mut hash = seed;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

fn fmt_bounded_bytes(formatter: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let shown = bytes.len().min(MAX_DEBUG_BYTES);
    for byte in &bytes[..shown] {
        formatter.write_str(char::from(HEX[usize::from(byte >> 4)]).encode_utf8(&mut [0; 4]))?;
        formatter.write_str(char::from(HEX[usize::from(byte & 0x0f)]).encode_utf8(&mut [0; 4]))?;
    }
    if bytes.len() > shown {
        write!(formatter, "...({} bytes)", bytes.len())?;
    }
    Ok(())
}
