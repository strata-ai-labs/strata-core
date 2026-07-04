use crate::format::TableCompression;
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    CacheInsert, TableBlockAddress, TableBlockCache, TableBlockCacheKey, TableBlockCacheKind,
    TableBlockCacheStats, TableBloomFilter, TableBloomProbe, TableBuilderConfig, TableCacheConfig,
    TableCacheTableId, TableIdentity, TablePhysicalKeyBytes, TableRow, TableRuntimeError,
};
use std::sync::Arc;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

use crate::table::ImmutableTableBuilder;

fn cache_id(name: &str) -> TableCacheTableId {
    TableCacheTableId::new(name.as_bytes()).expect("cache id")
}

fn address(kind: TableBlockCacheKind, offset: u64, length: u32) -> TableBlockAddress {
    TableBlockAddress::new(
        kind,
        offset,
        length,
        Some(u32::try_from(offset).expect("test offset fits in u32")),
    )
    .expect("block address")
}

fn key(table: &str, kind: TableBlockCacheKind, offset: u64, length: u32) -> TableBlockCacheKey {
    TableBlockCacheKey::new(cache_id(table), address(kind, offset, length))
}

fn bytes(byte: u8, len: usize) -> Arc<[u8]> {
    Arc::<[u8]>::from(vec![byte; len])
}

fn enabled_cache(capacity: usize) -> TableBlockCache {
    TableBlockCache::new(TableCacheConfig::new(true, capacity).expect("cache config"))
}

fn key_on_new_shard(
    cache: &TableBlockCache,
    table: &str,
    existing_shards: &[usize],
) -> Option<TableBlockCacheKey> {
    (0..10_000u64)
        .map(|index| key(table, TableBlockCacheKind::Data, index.saturating_mul(4), 4))
        .find(|candidate| !existing_shards.contains(&cache.shard_index_for_test(candidate)))
}

fn physical_key(branch_byte: u8, space_id: u8, user_key: impl Into<Vec<u8>>) -> PhysicalKey {
    PhysicalKey::new(
        BranchId::from_bytes([branch_byte; BranchId::BYTE_LEN]),
        "cache",
        StorageSpaceId::from_raw(space_id).expect("storage space id"),
        user_key,
    )
    .expect("physical key")
}

#[test]
fn cache_key_and_address_validation_is_structural() {
    assert_eq!(
        TableCacheTableId::new(Vec::<u8>::new()),
        Err(TableRuntimeError::InvalidConfig {
            field: "table_cache_id",
            reason: "must not be empty",
        })
    );
    assert_eq!(
        TableCacheTableId::new(vec![0xaa; 513]),
        Err(TableRuntimeError::InvalidConfig {
            field: "table_cache_id",
            reason: "is too large",
        })
    );
    assert_eq!(
        TableBlockAddress::new(TableBlockCacheKind::Data, 0, 0, None),
        Err(TableRuntimeError::InvalidRange {
            field: "block_length",
        })
    );
    assert_eq!(
        TableBlockAddress::new(TableBlockCacheKind::Data, u64::MAX, 1, None),
        Err(TableRuntimeError::InvalidRange {
            field: "block_range",
        })
    );

    assert_eq!(
        TableCacheTableId::new(b"cache-table".as_slice())
            .expect("id from explicit bytes")
            .as_slice(),
        b"cache-table"
    );
    assert_eq!(
        TableCacheTableId::new(b"opaque/id/with/slashes".as_slice())
            .expect("cache ids do not impose path syntax")
            .as_slice(),
        b"opaque/id/with/slashes"
    );

    let table = cache_id("table-a");
    let data = TableBlockCacheKey::new(table.clone(), address(TableBlockCacheKind::Data, 8, 4));
    let index = TableBlockCacheKey::new(table.clone(), address(TableBlockCacheKind::Index, 8, 4));
    let accelerator = TableBlockCacheKey::new(
        table.clone(),
        address(TableBlockCacheKind::Accelerator, 8, 4),
    );
    let different_length =
        TableBlockCacheKey::new(table.clone(), address(TableBlockCacheKind::Data, 8, 5));
    let different_offset_same_ordinal = TableBlockCacheKey::new(
        table,
        TableBlockAddress::new(TableBlockCacheKind::Data, 9, 4, Some(8)).expect("address"),
    );
    let same_offset_same_length_different_ordinal = TableBlockCacheKey::new(
        cache_id("table-a"),
        TableBlockAddress::new(TableBlockCacheKind::Data, 8, 4, Some(9)).expect("address"),
    );
    assert_ne!(data, index);
    assert_ne!(data, accelerator);
    assert_ne!(data, different_length);
    assert_ne!(data, different_offset_same_ordinal);
    assert_ne!(data, same_offset_same_length_different_ordinal);
    assert_ne!(index, accelerator);
    assert_eq!(data.address().kind(), TableBlockCacheKind::Data);
    assert_eq!(data.address().offset(), 8);
    assert_eq!(data.address().length(), 4);
    assert_eq!(data.address().ordinal(), Some(8));

    let long = TableCacheTableId::new(vec![0xab; 32]).expect("long id");
    let rendered = format!("{long:?}");
    assert!(rendered.contains("32 bytes"));
    assert!(rendered.len() < 96);
}

#[test]
fn disabled_cache_stores_nothing_and_reports_skips() {
    let cache = TableBlockCache::disabled();
    let key = key("disabled", TableBlockCacheKind::Data, 0, 4);

    assert!(cache.get(&key).is_none());
    let inserted = cache
        .insert(key.clone(), bytes(1, 4))
        .expect("disabled insert");
    assert!(matches!(inserted, CacheInsert::SkippedDisabled(_)));
    assert_eq!(inserted.bytes().as_ref(), &[1, 1, 1, 1]);
    assert!(cache.get(&key).is_none());

    let stats: TableBlockCacheStats = cache.stats();
    assert_eq!(stats.entries(), 0);
    assert_eq!(stats.bytes(), 0);
    assert_eq!(stats.misses(), 2);
    assert_eq!(stats.skipped_disabled(), 1);

    cache.clear();
    cache.resize(128);
    let inserted = cache
        .insert(key.clone(), bytes(2, 4))
        .expect("disabled insert after resize");
    assert!(matches!(inserted, CacheInsert::SkippedDisabled(_)));
    assert!(cache.get(&key).is_none());
    assert_eq!(cache.stats().entries(), 0);
    assert_eq!(cache.stats().bytes(), 0);
    assert_eq!(cache.stats().capacity_bytes(), 128);
}

#[test]
fn zero_capacity_table_cache_does_not_store() {
    let cache = TableBlockCache::new(TableCacheConfig::new(false, 0).expect("zero cache"));
    let first = key("zero-capacity", TableBlockCacheKind::Data, 0, 4);

    assert!(matches!(
        cache.insert(first.clone(), bytes(1, 4)).expect("insert"),
        CacheInsert::SkippedDisabled(_)
    ));
    assert!(cache.get(&first).is_none());
    assert_eq!(cache.stats().entries(), 0);
    assert_eq!(cache.stats().bytes(), 0);
    assert_eq!(cache.stats().capacity_bytes(), 0);
}

#[test]
fn small_cache_serves_oversized_block_uncached() {
    let cache = enabled_cache(4);
    let large = key("small-cache", TableBlockCacheKind::Data, 0, 8);

    let inserted = cache.insert(large.clone(), bytes(2, 8)).expect("insert");

    assert!(matches!(inserted, CacheInsert::SkippedOversized(_)));
    assert_eq!(inserted.bytes().len(), 8);
    assert!(cache.get(&large).is_none());
    assert_eq!(cache.stats().skipped_oversized(), 1);
    assert_eq!(cache.stats().bytes(), 0);
}

#[test]
fn table_cache_respects_capacity_after_insert() {
    let cache = enabled_cache(8);
    for index in 0..4u64 {
        cache
            .insert(
                key("capacity", TableBlockCacheKind::Data, index * 4, 4),
                bytes(u8::try_from(index).expect("byte"), 4),
            )
            .expect("insert");
        assert!(cache.stats().bytes() <= cache.stats().capacity_bytes());
    }

    assert_eq!(cache.stats().entries(), 2);
    assert_eq!(cache.stats().bytes(), 8);
    assert!(cache.stats().evictions() >= 2);
}

#[test]
fn table_cache_eviction_effort_is_bounded() {
    let cache = enabled_cache(16);
    for index in 0..64u64 {
        cache
            .insert(
                key("bounded-evict", TableBlockCacheKind::Data, index * 4, 4),
                bytes(u8::try_from(index % 251).expect("byte"), 4),
            )
            .expect("insert");
    }

    let stats = cache.stats();
    assert_eq!(stats.entries(), 4);
    assert_eq!(stats.bytes(), 16);
    assert_eq!(stats.evictions(), 60);
}

#[test]
fn table_cache_shrink_records_pressure() {
    let cache = enabled_cache(16);
    for index in 0..4u64 {
        cache
            .insert(
                key("shrink", TableBlockCacheKind::Data, index * 4, 4),
                bytes(u8::try_from(index).expect("byte"), 4),
            )
            .expect("insert");
    }

    cache.resize(4);

    let stats = cache.stats();
    assert_eq!(stats.capacity_bytes(), 4);
    assert_eq!(stats.bytes(), 4);
    assert_eq!(stats.entries(), 1);
    assert_eq!(stats.evictions(), 3);
}

#[test]
fn table_cache_stats_include_hits_misses_entries_bytes() {
    let cache = enabled_cache(8);
    let first = key("stats", TableBlockCacheKind::Data, 0, 4);
    assert!(cache.get(&first).is_none());
    cache.insert(first.clone(), bytes(3, 4)).expect("insert");
    assert!(cache.get(&first).is_some());

    let stats = cache.stats();
    assert_eq!(stats.misses(), 1);
    assert_eq!(stats.hits(), 1);
    assert_eq!(stats.entries(), 1);
    assert_eq!(stats.bytes(), 4);
    assert_eq!(stats.capacity_bytes(), 8);
}

#[test]
fn sharded_cache_aggregates_capacity_stats_and_table_invalidation() {
    let cache = enabled_cache((2 * 64 * 1024) + 3);
    assert!(cache.shard_count_for_test() > 1);
    assert_eq!(cache.stats().capacity_bytes(), (2 * 64 * 1024) + 3);

    let table = cache_id("sharded-table");
    let first = key_on_new_shard(&cache, "sharded-table", &[]).expect("first shard key");
    let second = key_on_new_shard(
        &cache,
        "sharded-table",
        &[cache.shard_index_for_test(&first)],
    )
    .expect("second shard key");
    assert_ne!(
        cache.shard_index_for_test(&first),
        cache.shard_index_for_test(&second)
    );

    cache.insert(first.clone(), bytes(0x41, 4)).expect("first");
    cache
        .insert(second.clone(), bytes(0x42, 4))
        .expect("second");
    assert_eq!(cache.stats().entries(), 2);
    assert_eq!(cache.stats().bytes(), 8);

    assert_eq!(cache.remove_table(&table), 2);
    let stats = cache.stats();
    assert_eq!(stats.entries(), 0);
    assert_eq!(stats.bytes(), 0);
    assert_eq!(stats.table_invalidations(), 1);
}

#[test]
fn sharded_cache_caps_default_budget_without_tiny_shards() {
    let capacity = 64 * 1024 * 1024;
    let cache = enabled_cache(capacity);
    // BS4.1: shard count is CPU-derived (clamped [4,64]) and capped by capacity so no shard is
    // smaller than one 64 KiB block. Assert machine-independently: the band, and no tiny shards.
    let shard_count = cache.shard_count_for_test();
    assert!(
        (4..=64).contains(&shard_count),
        "shard count {shard_count} outside [4,64]"
    );
    assert!(
        capacity / shard_count >= 64 * 1024,
        "per-shard capacity {} is below one block",
        capacity / shard_count
    );
    assert_eq!(cache.stats().capacity_bytes(), 64 * 1024 * 1024);

    let block = bytes(0x55, 154);
    let key = key("default-budget-table", TableBlockCacheKind::Data, 0, 154);
    assert!(matches!(
        cache.insert(key, block).expect("insert small block"),
        CacheInsert::Inserted(_)
    ));
    let stats = cache.stats();
    assert_eq!(stats.inserts(), 1);
    assert_eq!(stats.skipped_oversized(), 0);
}

#[test]
fn shard_count_follows_cpu_band_and_caps_by_capacity() {
    // Disabled (zero-capacity) collapses to a single shard.
    assert_eq!(TableBlockCache::disabled().shard_count_for_test(), 1);
    // A sub-block cache never gets more shards than blocks it can hold (no tiny shards).
    assert_eq!(enabled_cache(4).shard_count_for_test(), 1);
    // A large cache is CPU-derived: within [4,64], and never sub-block per shard. Machine-independent.
    let capacity = 64 * 1024 * 1024;
    let shard_count = enabled_cache(capacity).shard_count_for_test();
    assert!(
        (4..=64).contains(&shard_count),
        "shard count {shard_count} outside [4,64]"
    );
    assert!(
        capacity / shard_count >= 64 * 1024,
        "shard smaller than one block"
    );
}

#[test]
fn concurrent_hammer_stays_within_capacity_and_never_panics() {
    let capacity = 256 * 1024;
    let cache = Arc::new(enabled_cache(capacity));
    let workers: Vec<_> = (0..8u8)
        .map(|worker| {
            let cache = Arc::clone(&cache);
            std::thread::spawn(move || {
                for round in 0..4_000u64 {
                    let offset = u64::from(worker) * 128 + (round % 64);
                    let entry = key("hammer", TableBlockCacheKind::Data, offset, 4);
                    // Outcomes are intentionally discarded: this is a race / panic / deadlock smoke
                    // under concurrency, not a correctness assertion (that is the property test).
                    match round % 3 {
                        0 => {
                            let _ = cache.insert(entry, bytes(worker, 4 * 1024));
                        }
                        1 => {
                            let _ = cache.get(&entry);
                        }
                        _ => {
                            let _ = cache.remove(&entry);
                        }
                    }
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().expect("hammer worker joins cleanly");
    }
    assert!(
        cache.current_bytes() <= u64::try_from(capacity).expect("capacity fits u64"),
        "cache exceeded capacity: {} > {capacity}",
        cache.current_bytes()
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn table_cache_perf_counters_distinguish_hits_misses_inserts_and_skips() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let zero = crate::observability::perf_trace::snapshot();
    assert_eq!(zero.table_cache_hits(), 0);
    assert_eq!(zero.table_cache_misses(), 0);
    assert_eq!(zero.table_cache_inserts(), 0);
    assert_eq!(zero.table_cache_skipped_inserts(), 0);

    let cache = enabled_cache(4);
    let first = key("perf-cache", TableBlockCacheKind::Data, 0, 4);
    let oversized = key("perf-cache", TableBlockCacheKind::Data, 8, 8);

    assert!(cache.get(&first).is_none());
    assert!(matches!(
        cache.insert(first.clone(), bytes(7, 4)).expect("insert"),
        CacheInsert::Inserted(_)
    ));
    assert!(cache.get(&first).is_some());
    assert!(matches!(
        cache
            .insert(oversized, bytes(8, 8))
            .expect("oversized insert"),
        CacheInsert::SkippedOversized(_)
    ));

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.table_cache_misses(), 1);
    assert_eq!(perf.table_cache_inserts(), 1);
    assert_eq!(perf.table_cache_hits(), 1);
    assert_eq!(perf.table_cache_skipped_inserts(), 1);
}

#[test]
fn table_cache_keys_use_table_identity_not_path() {
    let cache = enabled_cache(16);
    let path_like = key("dir/table", TableBlockCacheKind::Data, 0, 4);
    let plain = key("table", TableBlockCacheKind::Data, 0, 4);

    cache
        .insert(path_like.clone(), bytes(4, 4))
        .expect("path-like id");
    cache.insert(plain.clone(), bytes(5, 4)).expect("plain id");

    assert_eq!(
        cache.get(&path_like).expect("path-like hit").as_ref(),
        &[4; 4]
    );
    assert_eq!(cache.get(&plain).expect("plain hit").as_ref(), &[5; 4]);
}

#[test]
fn two_runtime_caches_are_isolated() {
    let left = enabled_cache(16);
    let right = enabled_cache(16);
    let shared = key("isolated", TableBlockCacheKind::Data, 0, 4);

    left.insert(shared.clone(), bytes(6, 4))
        .expect("left insert");

    assert!(right.get(&shared).is_none());
    assert_eq!(left.stats().entries(), 1);
    assert_eq!(right.stats().entries(), 0);
}

#[test]
fn cache_insert_get_duplicate_remove_clear_and_stats_are_deterministic() {
    let cache = enabled_cache(64);
    let first = key("table-a", TableBlockCacheKind::Data, 0, 4);
    let second = key("table-a", TableBlockCacheKind::Properties, 4, 3);

    assert!(cache.get(&first).is_none());
    assert!(matches!(
        cache.insert(first.clone(), bytes(0x11, 4)).expect("insert"),
        CacheInsert::Inserted(_)
    ));
    assert_eq!(cache.get(&first).expect("cache hit").as_ref(), &[0x11; 4]);

    let duplicate = cache
        .insert(first.clone(), bytes(0x22, 4))
        .expect("duplicate insert");
    assert!(matches!(duplicate, CacheInsert::DuplicateExisting(_)));
    assert_eq!(duplicate.bytes().as_ref(), &[0x11; 4]);
    assert_eq!(
        cache.get(&first).expect("existing survives").as_ref(),
        &[0x11; 4]
    );

    cache
        .insert(second.clone(), bytes(0x33, 3))
        .expect("second insert");
    assert_eq!(cache.stats().entries(), 2);
    assert_eq!(cache.stats().bytes(), 7);
    assert!(cache.remove(&first));
    assert!(!cache.remove(&first));
    assert!(cache.get(&first).is_none());
    assert_eq!(
        cache.get(&second).expect("second remains").as_ref(),
        &[0x33; 3]
    );

    cache.clear();
    let stats = cache.stats();
    assert_eq!(stats.entries(), 0);
    assert_eq!(stats.bytes(), 0);
    assert_eq!(stats.inserts(), 2);
    assert_eq!(stats.duplicate_inserts(), 1);
    assert_eq!(stats.removes(), 1);
    assert_eq!(stats.table_invalidations(), 0);
    assert_eq!(stats.clears(), 1);
    assert!(stats.hits() >= 3);
    assert!(stats.misses() >= 2);
}

#[test]
fn cache_capacity_eviction_resize_and_table_removal_are_coherent() {
    let cache = enabled_cache(10);
    let first = key("table-a", TableBlockCacheKind::Data, 0, 4);
    let second = key("table-a", TableBlockCacheKind::Data, 4, 4);
    let third = key("table-a", TableBlockCacheKind::Data, 8, 4);
    let other_table = key("table-b", TableBlockCacheKind::Data, 0, 4);

    cache.insert(first.clone(), bytes(1, 4)).expect("first");
    cache.insert(second.clone(), bytes(2, 4)).expect("second");
    assert_eq!(cache.get(&first).expect("touch first").as_ref(), &[1; 4]);
    cache.insert(third.clone(), bytes(3, 4)).expect("third");

    assert_eq!(cache.stats().bytes(), 8);
    assert_eq!(cache.stats().entries(), 2);
    assert_eq!(cache.stats().evictions(), 1);
    assert!(cache.get(&second).is_none());
    assert_eq!(cache.get(&first).expect("recent first").as_ref(), &[1; 4]);
    assert_eq!(cache.get(&third).expect("new third").as_ref(), &[3; 4]);

    let oversized = cache
        .insert(
            key("table-a", TableBlockCacheKind::Data, 12, 20),
            bytes(9, 20),
        )
        .expect("oversized insert");
    assert!(matches!(oversized, CacheInsert::SkippedOversized(_)));
    assert_eq!(cache.stats().skipped_oversized(), 1);

    cache.resize(16);
    cache
        .insert(other_table.clone(), bytes(4, 4))
        .expect("other table");
    let removes_before_table_invalidation = cache.stats().removes();
    assert_eq!(cache.remove_table(&cache_id("table-a")), 2);
    assert_eq!(cache.stats().removes(), removes_before_table_invalidation);
    assert_eq!(cache.stats().table_invalidations(), 1);
    assert!(cache.get(&first).is_none());
    assert_eq!(
        cache
            .get(&other_table)
            .expect("other table remains")
            .as_ref(),
        &[4; 4]
    );

    cache.resize(0);
    assert_eq!(cache.stats().capacity_bytes(), 0);
    assert_eq!(cache.stats().entries(), 0);
    assert!(cache.get(&other_table).is_none());
}

#[test]
fn cache_zero_charge_exact_fit_resize_and_stats_read_edges_are_coherent() {
    let cache = enabled_cache(4);
    let first = key("table-a", TableBlockCacheKind::Data, 0, 4);
    let second = key("table-a", TableBlockCacheKind::Data, 4, 4);

    assert_eq!(
        cache.insert(first.clone(), Arc::<[u8]>::from(Vec::<u8>::new())),
        Err(TableRuntimeError::InvalidRange {
            field: "cache_charge",
        })
    );
    assert!(matches!(
        cache.insert(first.clone(), bytes(1, 4)).expect("exact fit"),
        CacheInsert::Inserted(_)
    ));
    let stats_before = cache.stats();
    let stats_after = cache.stats();
    assert_eq!(stats_before, stats_after);
    assert_eq!(stats_after.entries(), 1);
    assert_eq!(stats_after.bytes(), 4);
    assert_eq!(stats_after.capacity_bytes(), 4);

    assert!(matches!(
        cache
            .insert(second.clone(), bytes(2, 4))
            .expect("second exact fit"),
        CacheInsert::Inserted(_)
    ));
    assert_eq!(cache.stats().evictions(), 1);
    assert!(cache.get(&first).is_none());
    assert_eq!(
        cache.get(&second).expect("second remains").as_ref(),
        &[2; 4]
    );

    cache.resize(8);
    assert!(matches!(
        cache
            .insert(first.clone(), bytes(3, 4))
            .expect("insert after resize up"),
        CacheInsert::Inserted(_)
    ));
    assert_eq!(cache.stats().entries(), 2);
    assert_eq!(cache.stats().bytes(), 8);

    cache.resize(0);
    assert_eq!(cache.stats().entries(), 0);
    assert!(matches!(
        cache
            .insert(first, bytes(4, 4))
            .expect("insert after resize to zero"),
        CacheInsert::SkippedOversized(_)
    ));
    assert_eq!(cache.stats().entries(), 0);
}

#[test]
fn cache_resize_downward_and_reinsert_after_eviction_follow_lru_model() {
    let cache = enabled_cache(12);
    let first = key("table-a", TableBlockCacheKind::Data, 0, 4);
    let second = key("table-a", TableBlockCacheKind::Index, 4, 4);
    let third = key("table-a", TableBlockCacheKind::Properties, 8, 4);

    cache.insert(first.clone(), bytes(1, 4)).expect("first");
    cache.insert(second.clone(), bytes(2, 4)).expect("second");
    cache.insert(third.clone(), bytes(3, 4)).expect("third");
    assert_eq!(cache.stats().entries(), 3);

    assert_eq!(cache.get(&first).expect("touch first").as_ref(), &[1; 4]);
    cache.resize(8);
    assert_eq!(cache.stats().entries(), 2);
    assert_eq!(cache.stats().bytes(), 8);
    assert!(cache.get(&second).is_none());
    assert_eq!(cache.get(&first).expect("first retained").as_ref(), &[1; 4]);
    assert_eq!(cache.get(&third).expect("third retained").as_ref(), &[3; 4]);

    cache.resize(4);
    assert_eq!(cache.stats().entries(), 1);
    assert_eq!(cache.stats().bytes(), 4);
    assert!(cache.get(&first).is_none());
    assert_eq!(
        cache.get(&third).expect("recent third retained").as_ref(),
        &[3; 4]
    );

    assert!(matches!(
        cache
            .insert(first.clone(), bytes(4, 4))
            .expect("reinsert evicted key"),
        CacheInsert::Inserted(_)
    ));
    assert_eq!(
        cache.get(&first).expect("reinserted first").as_ref(),
        &[4; 4]
    );
    assert!(cache.get(&third).is_none());
    assert!(cache.stats().evictions() >= 3);
}

#[test]
fn cache_instances_do_not_share_entries_or_stats() {
    let left = enabled_cache(32);
    let right = enabled_cache(32);
    let shared_key = key("shared", TableBlockCacheKind::Data, 0, 4);

    left.insert(shared_key.clone(), bytes(0xaa, 4))
        .expect("left insert");
    assert_eq!(
        left.get(&shared_key).expect("left hit").as_ref(),
        &[0xaa; 4]
    );
    assert!(right.get(&shared_key).is_none());

    left.clear();
    assert_eq!(left.stats().entries(), 0);
    assert_eq!(right.stats().entries(), 0);
    assert!(left.stats().hits() > right.stats().hits());
    assert!(right.stats().misses() > 0);
}

#[test]
fn cache_recovers_from_mutex_poison_without_panicking() {
    let cache = enabled_cache(16);
    let first = key("poison", TableBlockCacheKind::Data, 0, 4);
    cache.insert(first.clone(), bytes(1, 4)).expect("insert");

    cache.poison_for_test();

    assert_eq!(
        cache.get(&first).expect("hit after poison").as_ref(),
        &[1; 4]
    );
    assert_eq!(cache.stats().entries(), 1);
    assert!(cache.remove(&first));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn cache_concurrent_duplicate_inserts_leave_one_logical_entry() {
    let cache = Arc::new(enabled_cache(64));
    let shared_key = key("shared", TableBlockCacheKind::Data, 0, 4);
    let handles = (0..8u8)
        .map(|worker| {
            let cache = Arc::clone(&cache);
            let shared_key = shared_key.clone();
            std::thread::spawn(move || {
                cache
                    .insert(shared_key.clone(), bytes(worker, 4))
                    .expect("concurrent insert");
                cache.get(&shared_key).expect("concurrent hit")
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        assert_eq!(handle.join().expect("thread joined").len(), 4);
    }

    let stats = cache.stats();
    assert_eq!(stats.entries(), 1);
    assert_eq!(stats.bytes(), 4);
    assert_eq!(stats.inserts(), 1);
    assert_eq!(stats.duplicate_inserts(), 7);
    assert!(stats.bytes() <= stats.capacity_bytes());
    assert_eq!(cache.get(&shared_key).expect("shared survives").len(), 4);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn cache_concurrent_disjoint_inserts_removes_and_clear_keep_gauges_coherent() {
    let cache = Arc::new(enabled_cache(128));
    let handles = (0..16u8)
        .map(|worker| {
            let cache = Arc::clone(&cache);
            std::thread::spawn(move || {
                let key = key(
                    "concurrent",
                    TableBlockCacheKind::Data,
                    u64::from(worker) * 4,
                    4,
                );
                cache.insert(key.clone(), bytes(worker, 4)).expect("insert");
                assert_eq!(cache.get(&key).expect("hit").as_ref(), &[worker; 4]);
                if worker % 3 == 0 {
                    cache.remove(&key);
                }
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.join().expect("thread joined");
    }

    let stats = cache.stats();
    assert!(stats.bytes() <= stats.capacity_bytes());
    assert!(stats.entries() <= 16);
    assert!(stats.inserts() >= stats.entries() as u64);

    cache.clear();
    let cleared = cache.stats();
    assert_eq!(cleared.entries(), 0);
    assert_eq!(cleared.bytes(), 0);
    assert!(cleared.clears() >= 1);
}

#[test]
fn bloom_filter_has_no_false_negatives_and_is_conservative_for_absence() {
    let keys = [
        b"alpha".as_slice(),
        b"bravo".as_slice(),
        b"embedded\0zero".as_slice(),
        b"alpha".as_slice(),
    ];
    let filter = TableBloomFilter::build(keys, 10).expect("bloom filter");

    assert!(!filter.is_empty());
    assert_eq!(filter.key_count(), keys.len());
    assert!((1..=30).contains(&filter.probes()));
    assert!(filter.approximate_size_bytes() > 0);
    for key in keys {
        assert_eq!(filter.might_contain(key), TableBloomProbe::MaybePresent);
    }
    assert!(matches!(
        filter.might_contain(b"absent"),
        TableBloomProbe::DefinitelyAbsent | TableBloomProbe::MaybePresent
    ));

    let empty = TableBloomFilter::build(Vec::<&[u8]>::new(), 10).expect("empty filter");
    assert!(empty.is_empty());
    assert_eq!(
        empty.might_contain(b"anything"),
        TableBloomProbe::DefinitelyAbsent
    );
    assert_eq!(
        TableBloomFilter::build([b"key".as_slice()], 0),
        Err(TableRuntimeError::InvalidConfig {
            field: "bits_per_key",
            reason: "must be nonzero",
        })
    );
    assert_eq!(
        TableBloomFilter::build([b"key".as_slice()], usize::MAX),
        Err(TableRuntimeError::InvalidRange {
            field: "bloom_bytes",
        })
    );
    assert_eq!(
        TableBloomFilter::build([b"first".as_slice(), b"second".as_slice()], usize::MAX),
        Err(TableRuntimeError::InvalidRange {
            field: "bloom_bits",
        })
    );
    assert_eq!(
        TableBloomFilter::build(keys, 10).expect("same bloom filter"),
        filter
    );
    assert_eq!(TableBloomProbe::Unavailable, TableBloomProbe::Unavailable);
}

#[cfg(feature = "perf-trace")]
#[test]
fn bloom_filter_perf_counters_distinguish_negative_and_positive_probes() {
    let filter =
        TableBloomFilter::build([b"alpha".as_slice(), b"bravo".as_slice()], 10).expect("filter");
    let _capture = crate::observability::perf_trace::begin_test_capture();

    assert_eq!(
        filter.might_contain(b"alpha"),
        TableBloomProbe::MaybePresent
    );
    assert_eq!(
        filter.might_contain(b"definitely-absent"),
        TableBloomProbe::DefinitelyAbsent
    );

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.table_filter_probes(), 2);
    assert_eq!(perf.table_filter_positive_probes(), 1);
    assert_eq!(perf.table_filter_negative_probes(), 1);
    assert_eq!(perf.table_filter_absent_probes(), 0);
}

#[test]
fn bloom_filter_many_long_keys_and_probe_cap_stay_bounded() {
    let keys = (0..128usize)
        .map(|index| {
            let mut key = vec![u8::try_from(index % 251).expect("byte"); 128 + (index % 17)];
            key.extend_from_slice(&index.to_le_bytes());
            key
        })
        .collect::<Vec<_>>();
    let key_refs = keys.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let filter = TableBloomFilter::build(key_refs.clone(), 14).expect("many-key bloom filter");

    assert_eq!(filter.key_count(), keys.len());
    assert!(filter.approximate_size_bytes() <= 16 * 1024 * 1024);
    for key in &key_refs {
        assert_eq!(filter.might_contain(key), TableBloomProbe::MaybePresent);
    }

    let high_probe_filter =
        TableBloomFilter::build([b"cap-probes".as_slice()], 10_000).expect("probe-capped filter");
    assert_eq!(high_probe_filter.probes(), 30);
    assert!(high_probe_filter.approximate_size_bytes() > 0);
    assert_eq!(
        high_probe_filter.might_contain(b"cap-probes"),
        TableBloomProbe::MaybePresent
    );
}

#[test]
fn bloom_filter_over_physical_key_bytes_preserves_key_boundaries() {
    let present = physical_key(1, 0x20, b"prefix".to_vec());
    let other_branch = physical_key(2, 0x20, b"prefix".to_vec());
    let other_space = physical_key(1, 0x21, b"prefix".to_vec());
    let neighbor = physical_key(1, 0x20, b"prefix-extra".to_vec());
    let present_bytes = TablePhysicalKeyBytes::from_physical_key(&present);
    let other_branch_bytes = TablePhysicalKeyBytes::from_physical_key(&other_branch);
    let other_space_bytes = TablePhysicalKeyBytes::from_physical_key(&other_space);
    let neighbor_bytes = TablePhysicalKeyBytes::from_physical_key(&neighbor);

    let filter = TableBloomFilter::build([present_bytes.as_slice()], 12).expect("physical filter");
    assert_eq!(
        filter.might_contain(present_bytes.as_slice()),
        TableBloomProbe::MaybePresent
    );
    for candidate in [other_branch_bytes, other_space_bytes, neighbor_bytes] {
        assert!(matches!(
            filter.might_contain(candidate.as_slice()),
            TableBloomProbe::DefinitelyAbsent | TableBloomProbe::MaybePresent
        ));
    }
}

#[test]
fn accelerators_do_not_change_table_footer_filter_fields_or_table_bytes() {
    const TABLE_FOOTER_SIZE: usize = 64;

    let row = StorageRow::put(
        physical_key(1, 0x20, b"durable-format".to_vec()),
        CommitVersion::new(7),
        Timestamp::from_micros(8),
        Timestamp::EPOCH,
        b"value".to_vec(),
    );
    let builder = ImmutableTableBuilder::new(
        TableBuilderConfig::new(1024, 1, TableCompression::Uncompressed).expect("config"),
    )
    .expect("builder");
    let artifact = builder
        .build_from_rows(
            TableIdentity::new("filter-format").expect("identity"),
            &[TableRow::new(row.clone())],
        )
        .expect("table artifact");
    let original = artifact.bytes().to_vec();
    let footer_start = original.len() - TABLE_FOOTER_SIZE;

    assert_eq!(read_u64_le(&original, footer_start + 12), 0);
    assert_eq!(read_u32_le(&original, footer_start + 20), 0);

    let cache = enabled_cache(1024);
    let cache_key = key("format-table-generation", TableBlockCacheKind::Data, 64, 32);
    cache
        .insert(cache_key, Arc::<[u8]>::from(original[64..96].to_vec()))
        .expect("cache insert");
    let physical = TablePhysicalKeyBytes::from_physical_key(row.physical_key());
    let filter = TableBloomFilter::build([physical.as_slice()], 10).expect("filter");
    assert_eq!(
        filter.might_contain(physical.as_slice()),
        TableBloomProbe::MaybePresent
    );

    assert_eq!(artifact.bytes(), original.as_slice());
    assert_eq!(read_u64_le(artifact.bytes(), footer_start + 12), 0);
    assert_eq!(read_u32_le(artifact.bytes(), footer_start + 20), 0);
}

fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("u32 bytes"))
}

fn read_u64_le(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("u64 bytes"))
}
