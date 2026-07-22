//! TCP4.2a — KV differential harness against the in-process `RocksDB` oracle
//! (the SQL Logic Test model, per capability).
//!
//! A seeded generator drives the same op-sequence into Strata (through the
//! executor wire surface — the real product path) and into `RocksDB` (an
//! independently built ordered store), then diffs the tiers both engines
//! provably share:
//!
//! - **Tier A (universal)**: put / overwrite / delete / get / exists / count.
//! - **Tier B (ordered keyspace)**: full ordered scan, byte-prefix listing,
//!   inclusive-start scan.
//!
//! The generated domain is restricted to the provable intersection: non-empty
//! keys (1–6 bytes over an edge-heavy alphabet including 0x00/0xff and
//! prefix-sharing structure) and non-empty values (1–8 bytes). Divergence on
//! a shared tier is a bug in one of the engines — and `RocksDB` is the more
//! battle-tested party. Tier C (snapshot ↔ as-of) and the Redis Tier-A
//! diversity oracle land in the next increment.
//!
//! Gated behind the `differential` feature (`required-features` on this
//! target): the `RocksDB` build is heavy C++, so this lane runs nightly, never
//! per-PR. Every failure message carries the seed; replays are exact.

use serde_json::json;

#[path = "parity/support.rs"]
mod support;

/// `SplitMix64` — deterministic, seed-replayable.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

/// Edge-heavy key alphabet: NUL, low, ASCII, high, 0xff — plus repetition to
/// force shared prefixes.
const ALPHABET: [u8; 6] = [0x00, 0x01, b'a', b'b', 0xfe, 0xff];

fn generated_key(rng: &mut Rng) -> Vec<u8> {
    // Bounds are tiny; the casts cannot truncate.
    #[allow(clippy::cast_possible_truncation)]
    let length = 1 + rng.below(6) as usize;
    (0..length)
        .map(|_| {
            #[allow(clippy::cast_possible_truncation)] // alphabet len 6
            let slot = rng.below(ALPHABET.len() as u64) as usize;
            ALPHABET[slot]
        })
        .collect()
}

#[allow(clippy::cast_possible_truncation)] // tiny bounds; byte cast intended
fn generated_value(rng: &mut Rng) -> Vec<u8> {
    let length = 1 + rng.below(8) as usize;
    (0..length).map(|_| rng.next() as u8).collect()
}

/// The shared-tier oracle surface both adapters implement.
trait KvOracle {
    fn put(&mut self, key: &[u8], value: &[u8]);
    fn delete(&mut self, key: &[u8]);
    fn get(&mut self, key: &[u8]) -> Option<Vec<u8>>;
    fn count(&mut self) -> u64;
    /// Full ordered (key, value) state — Tier B's strongest check.
    fn ordered_state(&mut self) -> Vec<(Vec<u8>, Vec<u8>)>;
    /// Keys under a byte prefix, in order.
    fn keys_with_prefix(&mut self, prefix: &[u8]) -> Vec<Vec<u8>>;
}

/// Strata through the executor wire surface (cache mode).
struct StrataKv(strata_executor::Executor);

impl StrataKv {
    fn new() -> Self {
        Self(support::executor())
    }
}

impl KvOracle for StrataKv {
    fn put(&mut self, key: &[u8], value: &[u8]) {
        support::run(
            &mut self.0,
            &json!({"type": "kv_put", "key": support::base64_encode(key),
                    "value": support::base64_encode(value)}),
        );
    }
    fn delete(&mut self, key: &[u8]) {
        support::run(
            &mut self.0,
            &json!({"type": "kv_delete", "key": support::base64_encode(key)}),
        );
    }
    fn get(&mut self, key: &[u8]) -> Option<Vec<u8>> {
        let output = support::run(
            &mut self.0,
            &json!({"type": "kv_get", "key": support::base64_encode(key)}),
        );
        // kv_get returns the versioned envelope: {found, value: {value, ...}}.
        if output["data"]["found"].as_bool() == Some(true) {
            Some(support::base64_decode(
                output["data"]["value"]["value"]
                    .as_str()
                    .unwrap_or_else(|| panic!("found row carries value bytes: {output}")),
            ))
        } else {
            None
        }
    }
    fn count(&mut self) -> u64 {
        support::run(&mut self.0, &json!({"type": "kv_count"}))["data"]
            .as_u64()
            .expect("kv_count returns a count")
    }
    fn ordered_state(&mut self) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut state = Vec::new();
        let mut start = support::base64_encode(&[0x00]);
        loop {
            let page = support::run(
                &mut self.0,
                &json!({"type": "kv_scan", "start": start, "limit": 512}),
            );
            let items = page["data"]["items"]
                .as_array()
                .unwrap_or_else(|| panic!("kv_scan carries items: {page}"));
            for item in items {
                state.push((
                    support::base64_decode(item["key"].as_str().expect("scan item key")),
                    support::base64_decode(item["value"].as_str().expect("scan item value")),
                ));
            }
            match page["data"]["cursor"].as_str() {
                Some(cursor) if page["data"]["has_more"].as_bool() == Some(true) => {
                    cursor.clone_into(&mut start);
                }
                _ => return state,
            }
        }
    }
    fn keys_with_prefix(&mut self, prefix: &[u8]) -> Vec<Vec<u8>> {
        let page = support::run(
            &mut self.0,
            &json!({"type": "kv_list", "prefix": support::base64_encode(prefix), "limit": 4096}),
        );
        page["data"]["items"]
            .as_array()
            .unwrap_or_else(|| panic!("kv_list carries items: {page}"))
            .iter()
            .map(|item| support::base64_decode(item.as_str().expect("list items are keys")))
            .collect()
    }
}

/// `RocksDB` in a tempdir — the independently built ordered oracle.
struct RocksKv {
    db: rocksdb::DB,
    _dir: tempfile::TempDir,
}

impl RocksKv {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("rocksdb tempdir");
        let db = rocksdb::DB::open_default(dir.path()).expect("open rocksdb");
        Self { db, _dir: dir }
    }
}

impl KvOracle for RocksKv {
    fn put(&mut self, key: &[u8], value: &[u8]) {
        self.db.put(key, value).expect("rocksdb put");
    }
    fn delete(&mut self, key: &[u8]) {
        self.db.delete(key).expect("rocksdb delete");
    }
    fn get(&mut self, key: &[u8]) -> Option<Vec<u8>> {
        self.db.get(key).expect("rocksdb get")
    }
    fn count(&mut self) -> u64 {
        self.db
            .iterator(rocksdb::IteratorMode::Start)
            .count()
            .try_into()
            .expect("count fits")
    }
    fn ordered_state(&mut self) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.db
            .iterator(rocksdb::IteratorMode::Start)
            .map(|entry| {
                let (key, value) = entry.expect("rocksdb iterate");
                (key.to_vec(), value.to_vec())
            })
            .collect()
    }
    fn keys_with_prefix(&mut self, prefix: &[u8]) -> Vec<Vec<u8>> {
        self.db
            .iterator(rocksdb::IteratorMode::From(
                prefix,
                rocksdb::Direction::Forward,
            ))
            .map(|entry| entry.expect("rocksdb iterate").0.to_vec())
            .take_while(|key| key.starts_with(prefix))
            .collect()
    }
}

/// One seeded differential run: identical op-sequence into both engines,
/// Tier A checks continuously, Tier B full-state checks periodically.
fn run_differential(seed: u64, ops: u64) {
    let mut rng = Rng(seed);
    let mut strata = StrataKv::new();
    let mut rocks = RocksKv::new();
    // The model tracks live keys so deletes/overwrites target real rows and
    // gets sample both present and absent keys.
    let mut live: std::collections::BTreeSet<Vec<u8>> = std::collections::BTreeSet::new();

    for step in 0..ops {
        let choice = rng.below(100);
        if choice < 55 || live.is_empty() {
            let key = generated_key(&mut rng);
            let value = generated_value(&mut rng);
            strata.put(&key, &value);
            rocks.put(&key, &value);
            live.insert(key);
        } else if choice < 70 {
            // Overwrite an existing key.
            #[allow(clippy::cast_possible_truncation)] // live set is tiny
            let index = rng.below(live.len() as u64) as usize;
            let key = live.iter().nth(index).expect("live key").clone();
            let value = generated_value(&mut rng);
            strata.put(&key, &value);
            rocks.put(&key, &value);
        } else if choice < 85 {
            // Delete an existing key.
            #[allow(clippy::cast_possible_truncation)] // live set is tiny
            let index = rng.below(live.len() as u64) as usize;
            let key = live.iter().nth(index).expect("live key").clone();
            strata.delete(&key);
            rocks.delete(&key);
            live.remove(&key);
        } else {
            // Delete a (likely) absent key — both must tolerate it identically.
            let key = generated_key(&mut rng);
            strata.delete(&key);
            rocks.delete(&key);
            live.remove(&key);
        }

        // Tier A: sampled point reads + count, every step.
        let probe = generated_key(&mut rng);
        assert_eq!(
            strata.get(&probe),
            rocks.get(&probe),
            "[seed={seed} step={step}] point read diverges on key {probe:x?}"
        );
        assert_eq!(
            strata.count(),
            rocks.count(),
            "[seed={seed} step={step}] count diverges"
        );

        // Tier B: full ordered state + prefix listings, every 16 steps.
        if step % 16 == 15 {
            assert_eq!(
                strata.ordered_state(),
                rocks.ordered_state(),
                "[seed={seed} step={step}] full ordered state diverges"
            );
            for prefix_byte in ALPHABET {
                assert_eq!(
                    strata.keys_with_prefix(&[prefix_byte]),
                    rocks.keys_with_prefix(&[prefix_byte]),
                    "[seed={seed} step={step}] prefix {prefix_byte:#04x} listing diverges"
                );
            }
        }
    }
    // Terminal full-state agreement.
    assert_eq!(
        strata.ordered_state(),
        rocks.ordered_state(),
        "[seed={seed}] terminal state diverges"
    );
}

/// Default lane: fixed seeds, bounded ops. `STRATA_KV_DIFF_SEEDS` /
/// `STRATA_KV_DIFF_OPS` scale the nightly soak without a code change.
#[test]
fn strata_kv_agrees_with_rocksdb_on_shared_tiers() {
    let seeds: u64 = std::env::var("STRATA_KV_DIFF_SEEDS")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(6);
    let ops: u64 = std::env::var("STRATA_KV_DIFF_OPS")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(160);
    for seed in 0..seeds {
        run_differential(seed, ops);
    }
}

/// Sabotage: the harness must detect a divergence. A run where one engine
/// silently drops a write must fail the differential — proving the oracle
/// actually bites (non-vacuousness, per program discipline).
#[test]
fn the_harness_detects_a_fabricated_divergence() {
    let mut strata = StrataKv::new();
    let mut rocks = RocksKv::new();
    strata.put(b"same", b"value");
    rocks.put(b"same", b"value");
    // Positive control: a written row must read back through BOTH adapters —
    // an adapter misreading its engine's response envelope fails here, not
    // as a phantom product bug (the #2746 lesson).
    assert_eq!(strata.get(b"same"), Some(b"value".to_vec()));
    assert_eq!(rocks.get(b"same"), Some(b"value".to_vec()));
    // Fabricate the divergence: RocksDB gets an extra row Strata never saw.
    rocks.put(b"phantom", b"row");
    assert_ne!(
        strata.ordered_state(),
        rocks.ordered_state(),
        "a fabricated extra row must diverge the full-state check"
    );
    assert_ne!(strata.count(), rocks.count());
}
