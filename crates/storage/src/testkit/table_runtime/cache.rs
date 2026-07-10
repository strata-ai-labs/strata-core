fn check_table_block_cache(script: &[u8]) -> Result<(), TestkitError> {
    let cache = TableBlockCache::new(
        TableCacheConfig::new(true, 12)
            .map_err(|err| TestkitError::new(format!("cache config failed: {err}")))?,
    );
    let table_a = TableCacheTableId::new(vec![0xa0, script_byte(script, 56)])
        .map_err(|err| TestkitError::new(format!("cache table id failed: {err}")))?;
    let table_b = TableCacheTableId::new(vec![0xb0, script_byte(script, 57)])
        .map_err(|err| TestkitError::new(format!("cache table id failed: {err}")))?;
    let first = generated_cache_key(&table_a, TableBlockCacheKind::Data, 0, 4, Some(0))?;
    let second = generated_cache_key(&table_a, TableBlockCacheKind::Index, 4, 4, Some(1))?;
    let third = generated_cache_key(&table_a, TableBlockCacheKind::Data, 8, 8, Some(2))?;
    let other_table = generated_cache_key(&table_b, TableBlockCacheKind::Data, 0, 4, Some(0))?;

    expect_cache_inserted(cache.insert(first.clone(), arc_bytes(script_byte(script, 58), 4)))?;
    expect_cache_hit(&cache, &first, script_byte(script, 58), 4)?;
    expect_cache_duplicate(
        cache.insert(first.clone(), arc_bytes(script_byte(script, 59), 4)),
        script_byte(script, 58),
        4,
    )?;
    expect_cache_inserted(cache.insert(second.clone(), arc_bytes(script_byte(script, 60), 4)))?;
    expect_cache_hit(&cache, &first, script_byte(script, 58), 4)?;
    expect_cache_inserted(cache.insert(third.clone(), arc_bytes(script_byte(script, 61), 8)))?;
    if cache.stats().bytes() > cache.stats().capacity_bytes() {
        return Err(TestkitError::new("cache exceeded capacity"));
    }
    if cache.get(&second).is_some() {
        return Err(TestkitError::new(
            "least-recent cache entry survived pressure",
        ));
    }
    expect_cache_hit(&cache, &first, script_byte(script, 58), 4)?;
    expect_cache_hit(&cache, &third, script_byte(script, 61), 8)?;

    expect_cache_inserted(
        cache.insert(other_table.clone(), arc_bytes(script_byte(script, 62), 4)),
    )?;
    if cache.remove_table(&table_a) == 0 || cache.get(&first).is_some() {
        return Err(TestkitError::new("cache table removal failed"));
    }
    expect_cache_hit(&cache, &other_table, script_byte(script, 62), 4)?;

    check_disabled_and_oversized_cache(script)?;
    check_generated_cache_model(script)
}

fn check_disabled_and_oversized_cache(script: &[u8]) -> Result<(), TestkitError> {
    let disabled = TableBlockCache::disabled();
    let disabled_key = generated_cache_key(
        &TableCacheTableId::new(b"disabled".to_vec())
            .map_err(|err| TestkitError::new(format!("disabled cache id failed: {err}")))?,
        TableBlockCacheKind::Data,
        0,
        4,
        None,
    )?;
    match disabled
        .insert(disabled_key.clone(), arc_bytes(script_byte(script, 63), 4))
        .map_err(|err| TestkitError::new(format!("disabled cache insert failed: {err}")))?
    {
        CacheInsert::SkippedDisabled(_) => {}
        other => {
            return Err(TestkitError::new(format!(
                "disabled cache returned unexpected insert result: {other:?}"
            )));
        }
    }
    if disabled.get(&disabled_key).is_some() || disabled.stats().entries() != 0 {
        return Err(TestkitError::new("disabled cache stored an entry"));
    }

    let small = TableBlockCache::new(
        TableCacheConfig::new(true, 2)
            .map_err(|err| TestkitError::new(format!("small cache config failed: {err}")))?,
    );
    match small
        .insert(disabled_key, arc_bytes(script_byte(script, 64), 4))
        .map_err(|err| TestkitError::new(format!("oversized cache insert failed: {err}")))?
    {
        CacheInsert::SkippedOversized(_) => Ok(()),
        other => Err(TestkitError::new(format!(
            "oversized cache returned unexpected insert result: {other:?}"
        ))),
    }
}

fn check_generated_cache_model(script: &[u8]) -> Result<(), TestkitError> {
    let enabled = script_byte(script, 88) % 3 != 0;
    let initial_capacity = if enabled {
        1 + usize::from(script_byte(script, 89) % 64)
    } else {
        0
    };
    let cache = TableBlockCache::new(
        TableCacheConfig::new(enabled, initial_capacity)
            .map_err(|err| TestkitError::new(format!("generated cache config failed: {err}")))?,
    );
    let mut model = GeneratedCacheModel::new(enabled, initial_capacity);
    let operations = 32 + usize::from(script_byte(script, 90) % 32);

    for step in 0..operations {
        run_generated_cache_operation(script, step, &cache, &mut model)?;
        assert_generated_cache_stats(&cache.stats(), &model)?;
        let before = cache.stats();
        let after = cache.stats();
        if before != after {
            return Err(TestkitError::new("cache stats read mutated cache state"));
        }
    }

    Ok(())
}

fn run_generated_cache_operation(
    script: &[u8],
    step: usize,
    cache: &TableBlockCache,
    model: &mut GeneratedCacheModel,
) -> Result<(), TestkitError> {
    let key = generated_script_cache_key(script, step)?;
    match generated_script_cache_byte(script, step, 91) % 6 {
        0 => {
            let expected_bytes = model.get(&key);
            if cache.get(&key).map(|bytes| bytes.to_vec()) != expected_bytes {
                return Err(TestkitError::new("generated cache get drifted"));
            }
        }
        1 => {
            let value = generated_script_cache_value(script, step);
            let expected = model.insert(&key, value);
            let observed = cache
                .insert(key, Arc::<[u8]>::from(expected.input_bytes().to_vec()))
                .map_err(|err| {
                    TestkitError::new(format!("generated cache insert failed: {err}"))
                })?;
            assert_generated_cache_insert(observed, expected)?;
        }
        2 => {
            let expected_removed = model.remove(&key);
            if cache.remove(&key) != expected_removed {
                return Err(TestkitError::new("generated cache remove drifted"));
            }
        }
        3 => {
            let table = generated_script_cache_table_id(script, step)?;
            let expected_removed = model.remove_table(&table);
            if cache.remove_table(&table) != expected_removed {
                return Err(TestkitError::new("generated cache table removal drifted"));
            }
        }
        4 => {
            model.clear();
            cache.clear();
        }
        _ => {
            let capacity = generated_script_cache_capacity(script, step);
            model.resize(capacity);
            cache.resize(capacity);
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum GeneratedCacheInsert {
    Inserted(Vec<u8>),
    DuplicateExisting { stored: Vec<u8>, attempted: Vec<u8> },
    SkippedDisabled(Vec<u8>),
    SkippedOversized(Vec<u8>),
}

impl GeneratedCacheInsert {
    fn input_bytes(&self) -> &[u8] {
        match self {
            Self::Inserted(bytes)
            | Self::SkippedDisabled(bytes)
            | Self::SkippedOversized(bytes) => bytes,
            Self::DuplicateExisting { attempted, .. } => attempted,
        }
    }
}

struct GeneratedCacheModel {
    enabled: bool,
    capacity_bytes: usize,
    bytes: usize,
    entries: BTreeMap<TableBlockCacheKey, Vec<u8>>,
    recency: VecDeque<TableBlockCacheKey>,
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
}

impl GeneratedCacheModel {
    fn new(enabled: bool, capacity_bytes: usize) -> Self {
        Self {
            enabled,
            capacity_bytes,
            bytes: 0,
            entries: BTreeMap::new(),
            recency: VecDeque::new(),
            hits: 0,
            misses: 0,
            inserts: 0,
            duplicate_inserts: 0,
            evictions: 0,
            removes: 0,
            table_invalidations: 0,
            clears: 0,
            skipped_oversized: 0,
            skipped_disabled: 0,
        }
    }

    fn get(&mut self, key: &TableBlockCacheKey) -> Option<Vec<u8>> {
        if let Some(bytes) = self.entries.get(key).cloned() {
            self.hits = self.hits.saturating_add(1);
            model_touch_recency(&mut self.recency, key);
            Some(bytes)
        } else {
            self.misses = self.misses.saturating_add(1);
            None
        }
    }

    fn insert(&mut self, key: &TableBlockCacheKey, bytes: Vec<u8>) -> GeneratedCacheInsert {
        if !self.enabled {
            self.skipped_disabled = self.skipped_disabled.saturating_add(1);
            return GeneratedCacheInsert::SkippedDisabled(bytes);
        }
        if let Some(stored) = self.entries.get(key).cloned() {
            self.duplicate_inserts = self.duplicate_inserts.saturating_add(1);
            model_touch_recency(&mut self.recency, key);
            return GeneratedCacheInsert::DuplicateExisting {
                stored,
                attempted: bytes,
            };
        }
        if bytes.len() > self.capacity_bytes {
            self.skipped_oversized = self.skipped_oversized.saturating_add(1);
            return GeneratedCacheInsert::SkippedOversized(bytes);
        }
        self.evict_to_fit(bytes.len());
        if self.bytes.saturating_add(bytes.len()) > self.capacity_bytes {
            self.skipped_oversized = self.skipped_oversized.saturating_add(1);
            return GeneratedCacheInsert::SkippedOversized(bytes);
        }

        self.bytes = self.bytes.saturating_add(bytes.len());
        self.entries.insert(key.clone(), bytes.clone());
        model_touch_recency(&mut self.recency, key);
        self.inserts = self.inserts.saturating_add(1);
        GeneratedCacheInsert::Inserted(bytes)
    }

    fn remove(&mut self, key: &TableBlockCacheKey) -> bool {
        if let Some(bytes) = self.entries.remove(key) {
            self.bytes = self.bytes.saturating_sub(bytes.len());
            model_remove_from_recency(&mut self.recency, key);
            self.removes = self.removes.saturating_add(1);
            true
        } else {
            false
        }
    }

    fn remove_table(&mut self, table: &TableCacheTableId) -> usize {
        let keys = self
            .entries
            .keys()
            .filter(|key| key.table() == table)
            .cloned()
            .collect::<Vec<_>>();
        let removed = keys.len();
        for key in keys {
            if let Some(bytes) = self.entries.remove(&key) {
                self.bytes = self.bytes.saturating_sub(bytes.len());
            }
            model_remove_from_recency(&mut self.recency, &key);
        }
        if removed > 0 {
            self.table_invalidations = self.table_invalidations.saturating_add(1);
        }
        removed
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.recency.clear();
        self.bytes = 0;
        self.clears = self.clears.saturating_add(1);
    }

    fn resize(&mut self, capacity_bytes: usize) {
        self.capacity_bytes = capacity_bytes;
        while self.bytes > self.capacity_bytes {
            if !self.evict_one() {
                break;
            }
        }
    }

    fn evict_to_fit(&mut self, incoming_bytes: usize) {
        while self.bytes.saturating_add(incoming_bytes) > self.capacity_bytes {
            if !self.evict_one() {
                break;
            }
        }
    }

    fn evict_one(&mut self) -> bool {
        while let Some(key) = self.recency.pop_front() {
            if let Some(bytes) = self.entries.remove(&key) {
                self.bytes = self.bytes.saturating_sub(bytes.len());
                self.evictions = self.evictions.saturating_add(1);
                return true;
            }
        }
        false
    }
}

fn assert_generated_cache_insert(
    observed: CacheInsert,
    expected: GeneratedCacheInsert,
) -> Result<(), TestkitError> {
    match (observed, expected) {
        (CacheInsert::Inserted(bytes), GeneratedCacheInsert::Inserted(expected_bytes))
        | (
            CacheInsert::SkippedDisabled(bytes),
            GeneratedCacheInsert::SkippedDisabled(expected_bytes),
        )
        | (
            CacheInsert::SkippedOversized(bytes),
            GeneratedCacheInsert::SkippedOversized(expected_bytes),
        ) if bytes.as_ref() == expected_bytes.as_slice() => Ok(()),
        (
            CacheInsert::DuplicateExisting(bytes),
            GeneratedCacheInsert::DuplicateExisting { stored, .. },
        ) if bytes.as_ref() == stored.as_slice() => Ok(()),
        (observed, expected) => Err(TestkitError::new(format!(
            "generated cache insert drifted: observed {observed:?}, expected {expected:?}"
        ))),
    }
}

fn assert_generated_cache_stats(
    stats: &TableBlockCacheStats,
    model: &GeneratedCacheModel,
) -> Result<(), TestkitError> {
    if stats.entries() != model.entries.len()
        || stats.bytes() != model.bytes
        || stats.capacity_bytes() != model.capacity_bytes
        || stats.hits() != model.hits
        || stats.misses() != model.misses
        || stats.inserts() != model.inserts
        || stats.duplicate_inserts() != model.duplicate_inserts
        || stats.evictions() != model.evictions
        || stats.removes() != model.removes
        || stats.table_invalidations() != model.table_invalidations
        || stats.clears() != model.clears
        || stats.skipped_oversized() != model.skipped_oversized
        || stats.skipped_disabled() != model.skipped_disabled
        || stats.bytes() > stats.capacity_bytes()
    {
        return Err(TestkitError::new(
            "generated cache stats drifted from model",
        ));
    }
    Ok(())
}

fn model_touch_recency(recency: &mut VecDeque<TableBlockCacheKey>, key: &TableBlockCacheKey) {
    model_remove_from_recency(recency, key);
    recency.push_back(key.clone());
}

fn model_remove_from_recency(recency: &mut VecDeque<TableBlockCacheKey>, key: &TableBlockCacheKey) {
    if let Some(index) = recency.iter().position(|candidate| candidate == key) {
        recency.remove(index);
    }
}

fn generated_script_cache_key(
    script: &[u8],
    step: usize,
) -> Result<TableBlockCacheKey, TestkitError> {
    let table = generated_script_cache_table_id(script, step)?;
    let kind = match generated_script_cache_byte(script, step, 23) % 4 {
        0 => TableBlockCacheKind::Data,
        1 => TableBlockCacheKind::Index,
        2 => TableBlockCacheKind::Properties,
        _ => TableBlockCacheKind::Accelerator,
    };
    generated_cache_key(
        &table,
        kind,
        u64::from(generated_script_cache_byte(script, step, 37) % 16).saturating_mul(4),
        1 + u32::from(generated_script_cache_byte(script, step, 41) % 16),
        Some(u32::from(
            generated_script_cache_byte(script, step, 43) % 16,
        )),
    )
}

fn generated_script_cache_table_id(
    script: &[u8],
    step: usize,
) -> Result<TableCacheTableId, TestkitError> {
    TableCacheTableId::new(vec![
        b'g',
        generated_script_cache_byte(script, step, 11) % 4,
        generated_script_cache_byte(script, step, 17),
    ])
    .map_err(|err| TestkitError::new(format!("generated cache table id failed: {err}")))
}

fn generated_script_cache_value(script: &[u8], step: usize) -> Vec<u8> {
    let len = 1 + usize::from(generated_script_cache_byte(script, step, 53) % 96);
    vec![generated_script_cache_byte(script, step, 59); len]
}

fn generated_script_cache_capacity(script: &[u8], step: usize) -> usize {
    match generated_script_cache_byte(script, step, 67) % 6 {
        0 => 0,
        1 => 1,
        2 => 4,
        3 => 16,
        4 => 64,
        _ => 128,
    }
}

fn generated_script_cache_byte(script: &[u8], step: usize, salt: usize) -> u8 {
    let base = if script.is_empty() {
        0
    } else {
        script[(step.saturating_mul(37).saturating_add(salt)) % script.len()]
    };
    base.wrapping_add(
        u8::try_from(step % 251)
            .expect("step residue fits u8")
            .wrapping_mul(17),
    )
    .wrapping_add(u8::try_from(salt % 251).expect("salt residue fits u8"))
}
