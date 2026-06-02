//! Table-local cache and read accelerators.

use super::{TableCacheConfig, TableRuntimeError, TableRuntimeResult};
use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::{Arc, Mutex};

const MAX_TABLE_CACHE_ID_BYTES: usize = 512;
const MAX_BLOOM_BYTES: usize = 16 * 1024 * 1024;
const MAX_BLOOM_PROBES: u8 = 30;
const MAX_DEBUG_BYTES: usize = 16;

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
        Self {
            state: Mutex::new(CacheState {
                enabled: config.enabled(),
                capacity_bytes: config.capacity_bytes(),
                bytes: 0,
                entries: BTreeMap::new(),
                recency: VecDeque::new(),
                stats: TableBlockCacheStats {
                    capacity_bytes: config.capacity_bytes(),
                    ..TableBlockCacheStats::default()
                },
            }),
        }
    }

    pub(crate) fn disabled() -> Self {
        let config = TableCacheConfig::new(false, 0)
            .expect("disabled table cache with zero capacity is valid");
        Self::new(config)
    }

    pub(crate) fn get(&self, key: &TableBlockCacheKey) -> Option<Arc<[u8]>> {
        let mut state = self.lock_state();
        let bytes = state.entries.get(key).cloned();
        if let Some(bytes) = bytes {
            state.stats.hits = state.stats.hits.saturating_add(1);
            touch_recency(&mut state.recency, key);
            Some(bytes)
        } else {
            state.stats.misses = state.stats.misses.saturating_add(1);
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

        let mut state = self.lock_state();
        if !state.enabled {
            state.stats.skipped_disabled = state.stats.skipped_disabled.saturating_add(1);
            return Ok(CacheInsert::SkippedDisabled(bytes));
        }

        if let Some(existing) = state.entries.get(&key).cloned() {
            state.stats.duplicate_inserts = state.stats.duplicate_inserts.saturating_add(1);
            touch_recency(&mut state.recency, &key);
            return Ok(CacheInsert::DuplicateExisting(existing));
        }

        if bytes.len() > state.capacity_bytes {
            state.stats.skipped_oversized = state.stats.skipped_oversized.saturating_add(1);
            return Ok(CacheInsert::SkippedOversized(bytes));
        }

        evict_to_fit(&mut state, bytes.len());
        if state.bytes.saturating_add(bytes.len()) > state.capacity_bytes {
            state.stats.skipped_oversized = state.stats.skipped_oversized.saturating_add(1);
            return Ok(CacheInsert::SkippedOversized(bytes));
        }

        state.bytes = state.bytes.saturating_add(bytes.len());
        let recency_key = key.clone();
        state.entries.insert(key, Arc::clone(&bytes));
        touch_recency(&mut state.recency, &recency_key);
        state.stats.inserts = state.stats.inserts.saturating_add(1);
        refresh_gauges(&mut state);
        Ok(CacheInsert::Inserted(bytes))
    }

    pub(crate) fn remove(&self, key: &TableBlockCacheKey) -> bool {
        let mut state = self.lock_state();
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
        let mut state = self.lock_state();
        let keys = state
            .entries
            .keys()
            .filter(|key| key.table() == table)
            .cloned()
            .collect::<Vec<_>>();
        let removed = keys.len();
        for key in keys {
            if let Some(bytes) = state.entries.remove(&key) {
                state.bytes = state.bytes.saturating_sub(bytes.len());
            }
            remove_from_recency(&mut state.recency, &key);
        }
        if removed > 0 {
            state.stats.table_invalidations = state.stats.table_invalidations.saturating_add(1);
        }
        refresh_gauges(&mut state);
        removed
    }

    pub(crate) fn clear(&self) {
        let mut state = self.lock_state();
        state.entries.clear();
        state.recency.clear();
        state.bytes = 0;
        state.stats.clears = state.stats.clears.saturating_add(1);
        refresh_gauges(&mut state);
    }

    pub(crate) fn resize(&self, capacity_bytes: usize) {
        let mut state = self.lock_state();
        state.capacity_bytes = capacity_bytes;
        state.stats.capacity_bytes = capacity_bytes;
        evict_to_capacity(&mut state);
        refresh_gauges(&mut state);
    }

    pub(crate) fn stats(&self) -> TableBlockCacheStats {
        let state = self.lock_state();
        let mut view = state.stats;
        view.entries = state.entries.len();
        view.bytes = state.bytes;
        view.capacity_bytes = state.capacity_bytes;
        view
    }

    #[cfg(test)]
    pub(crate) fn poison_for_test(&self) {
        let _ = std::panic::catch_unwind(|| {
            let _guard = self
                .state
                .lock()
                .expect("test lock should not already be poisoned");
            panic!("poison table cache mutex for recovery test");
        });
    }

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
            return TableBloomProbe::DefinitelyAbsent;
        }
        if self.bits.is_empty() || self.bit_count == 0 || self.probes == 0 {
            return TableBloomProbe::Unavailable;
        }
        for probe in 0..self.probes {
            let bit = bloom_bit(key, probe, self.bit_count);
            if self.bits[bit / 8] & (1 << (bit % 8)) == 0 {
                return TableBloomProbe::DefinitelyAbsent;
            }
        }
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
