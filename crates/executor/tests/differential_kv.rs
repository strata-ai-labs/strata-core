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

/// Tier A — the universal surface every oracle shares (Redis included).
trait KvTierA {
    fn put(&mut self, key: &[u8], value: &[u8]);
    fn delete(&mut self, key: &[u8]);
    fn get(&mut self, key: &[u8]) -> Option<Vec<u8>>;
    fn count(&mut self) -> u64;
}

/// Tier B — the ordered-keyspace surface (`RocksDB`, not Redis).
trait KvOracle: KvTierA {
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

impl KvTierA for StrataKv {
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
}

impl StrataKv {
    /// Put returning the commit receipt's logical timestamp (Tier C anchor).
    fn put_ts(&mut self, key: &[u8], value: &[u8]) -> u64 {
        support::run(
            &mut self.0,
            &json!({"type": "kv_put", "key": support::base64_encode(key),
                    "value": support::base64_encode(value)}),
        )["data"]["commit"]["timestamp"]
            .as_u64()
            .expect("write receipt carries a commit timestamp")
    }
    fn get_at(&mut self, key: &[u8], as_of: u64) -> Option<Vec<u8>> {
        let output = support::run(
            &mut self.0,
            &json!({"type": "kv_get", "key": support::base64_encode(key), "as_of": as_of}),
        );
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
    fn count_at(&mut self, as_of: u64) -> u64 {
        support::run(&mut self.0, &json!({"type": "kv_count", "as_of": as_of}))["data"]
            .as_u64()
            .expect("kv_count returns a count")
    }
    fn keys_with_prefix_at(&mut self, prefix: &[u8], as_of: u64) -> Vec<Vec<u8>> {
        let page = support::run(
            &mut self.0,
            &json!({"type": "kv_list", "prefix": support::base64_encode(prefix),
                    "as_of": as_of, "limit": 4096}),
        );
        page["data"]["items"]
            .as_array()
            .unwrap_or_else(|| panic!("kv_list carries items: {page}"))
            .iter()
            .map(|item| support::base64_decode(item.as_str().expect("list items are keys")))
            .collect()
    }
}

impl KvOracle for StrataKv {
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

impl KvTierA for RocksKv {
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
}

impl KvOracle for RocksKv {
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

/// Tier C: a `RocksDB` snapshot taken after op N is the oracle for Strata's
/// `as_of` at commit N — point reads, count, and prefix listings must agree
/// at every checkpoint even as both engines keep mutating past it.
fn run_tier_c(seed: u64, ops: u64) {
    let mut rng = Rng(seed ^ 0x00C0_FFEE);
    let mut strata = StrataKv::new();
    let dir = tempfile::tempdir().expect("rocksdb tempdir");
    let db = rocksdb::DB::open_default(dir.path()).expect("open rocksdb");
    let mut live: std::collections::BTreeSet<Vec<u8>> = std::collections::BTreeSet::new();
    let mut touched: std::collections::BTreeSet<Vec<u8>> = std::collections::BTreeSet::new();
    let mut checkpoints: Vec<(u64, rocksdb::Snapshot<'_>)> = Vec::new();

    for step in 0..ops {
        let choice = rng.below(100);
        if choice < 60 || live.is_empty() {
            let key = generated_key(&mut rng);
            let value = generated_value(&mut rng);
            strata.put_ts(&key, &value);
            db.put(&key, &value).expect("rocksdb put");
            touched.insert(key.clone());
            live.insert(key);
        } else if choice < 85 {
            #[allow(clippy::cast_possible_truncation)] // live set is tiny
            let index = rng.below(live.len() as u64) as usize;
            let key = live.iter().nth(index).expect("live key").clone();
            let value = generated_value(&mut rng);
            strata.put_ts(&key, &value);
            db.put(&key, &value).expect("rocksdb put");
        } else {
            #[allow(clippy::cast_possible_truncation)] // live set is tiny
            let index = rng.below(live.len() as u64) as usize;
            let key = live.iter().nth(index).expect("live key").clone();
            strata.delete(&key);
            db.delete(&key).expect("rocksdb delete");
            live.remove(&key);
        }

        // Checkpoint on a fixed cadence, anchored by a fresh put so the
        // Strata timestamp is a real commit both engines have applied.
        if step % 20 == 19 {
            let key = generated_key(&mut rng);
            let value = generated_value(&mut rng);
            let timestamp = strata.put_ts(&key, &value);
            db.put(&key, &value).expect("rocksdb put");
            touched.insert(key.clone());
            live.insert(key);
            checkpoints.push((timestamp, db.snapshot()));
        }
    }

    assert!(
        !checkpoints.is_empty(),
        "[seed={seed}] tier C swept no checkpoints"
    );
    for (position, (timestamp, snapshot)) in checkpoints.iter().enumerate() {
        // Point reads across every key the run ever touched.
        for key in &touched {
            let mut read = rocksdb::ReadOptions::default();
            read.set_snapshot(snapshot);
            let rocks = db.get_opt(key, &read).expect("rocksdb snapshot get");
            let strata_read = strata.get_at(key, *timestamp);
            assert_eq!(
                strata_read, rocks,
                "[seed={seed} checkpoint={position}] as-of point read diverges on {key:x?}"
            );
        }
        // Count.
        let mut read = rocksdb::ReadOptions::default();
        read.set_snapshot(snapshot);
        let rocks_count: u64 = db
            .iterator_opt(rocksdb::IteratorMode::Start, read)
            .count()
            .try_into()
            .expect("count fits");
        assert_eq!(
            strata.count_at(*timestamp),
            rocks_count,
            "[seed={seed} checkpoint={position}] as-of count diverges"
        );
        // Prefix listings.
        for prefix_byte in ALPHABET {
            let mut read = rocksdb::ReadOptions::default();
            read.set_snapshot(snapshot);
            let rocks_keys: Vec<Vec<u8>> = db
                .iterator_opt(
                    rocksdb::IteratorMode::From(&[prefix_byte], rocksdb::Direction::Forward),
                    read,
                )
                .map(|entry| entry.expect("rocksdb iterate").0.to_vec())
                .take_while(|key| key.starts_with(&[prefix_byte]))
                .collect();
            assert_eq!(
                strata.keys_with_prefix_at(&[prefix_byte], *timestamp),
                rocks_keys,
                "[seed={seed} checkpoint={position}] as-of prefix {prefix_byte:#04x} diverges"
            );
        }
    }
}

/// Tier C lane: fixed seeds; `STRATA_KV_DIFF_SEEDS` scales the soak.
#[test]
fn strata_as_of_agrees_with_rocksdb_snapshots() {
    let seeds: u64 = std::env::var("STRATA_KV_DIFF_SEEDS")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(3);
    for seed in 0..seeds {
        run_tier_c(seed, 120);
    }
}

/// Minimal RESP client — Tier A needs six commands, so no dependency: Redis
/// bulk strings are binary-safe, which is exactly what the edge-byte domain
/// requires of the diversity oracle.
struct RedisKv {
    stream: std::net::TcpStream,
    reader: std::io::BufReader<std::net::TcpStream>,
}

impl RedisKv {
    /// Connects to `STRATA_REDIS_URL`, selects the scratch database 15, and
    /// flushes it. Returns None (skip) when the env var is absent.
    fn connect() -> Option<Self> {
        let address = std::env::var("STRATA_REDIS_URL").ok()?;
        let stream = std::net::TcpStream::connect(&address)
            .unwrap_or_else(|err| panic!("STRATA_REDIS_URL={address} set but unreachable: {err}"));
        let reader = std::io::BufReader::new(stream.try_clone().expect("clone stream"));
        let mut redis = Self { stream, reader };
        redis.command(&[b"SELECT", b"15"]);
        redis.command(&[b"FLUSHDB"]);
        Some(redis)
    }

    fn command(&mut self, parts: &[&[u8]]) -> Option<Vec<u8>> {
        use std::io::{BufRead, Read, Write};
        let mut request = format!("*{}\r\n", parts.len()).into_bytes();
        for part in parts {
            request.extend_from_slice(format!("${}\r\n", part.len()).as_bytes());
            request.extend_from_slice(part);
            request.extend_from_slice(b"\r\n");
        }
        self.stream.write_all(&request).expect("redis write");
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("redis read");
        let (kind, rest) = line.split_at(1);
        let rest = rest.trim_end();
        match kind {
            "+" | ":" => Some(rest.as_bytes().to_vec()),
            "$" => {
                let length: i64 = rest.parse().expect("bulk length");
                if length < 0 {
                    return None;
                }
                let mut buffer = vec![0_u8; usize::try_from(length).expect("len") + 2];
                self.reader.read_exact(&mut buffer).expect("bulk body");
                buffer.truncate(buffer.len() - 2);
                Some(buffer)
            }
            "-" => panic!("redis error: {rest}"),
            other => panic!("unexpected RESP kind {other:?}"),
        }
    }
}

impl KvTierA for RedisKv {
    fn put(&mut self, key: &[u8], value: &[u8]) {
        self.command(&[b"SET", key, value]);
    }
    fn delete(&mut self, key: &[u8]) {
        self.command(&[b"DEL", key]);
    }
    fn get(&mut self, key: &[u8]) -> Option<Vec<u8>> {
        self.command(&[b"GET", key])
    }
    fn count(&mut self) -> u64 {
        let raw = self.command(&[b"DBSIZE"]).expect("dbsize reply");
        String::from_utf8(raw)
            .expect("integer")
            .parse()
            .expect("count")
    }
}

/// Tier A vs the architecturally-unlike oracle: identical op-sequences,
/// point reads + count checked continuously. Skips (loudly) when no Redis
/// is configured — the nightly job provides the service container.
#[test]
fn strata_kv_agrees_with_redis_on_tier_a() {
    let Some(mut redis) = RedisKv::connect() else {
        eprintln!("SKIP: STRATA_REDIS_URL not set; the nightly lane runs this against a service container");
        return;
    };
    // Positive controls (the #2746 lesson) before any differential claim.
    redis.put(b"probe\x00\xff", b"\x00value\xff");
    assert_eq!(redis.get(b"probe\x00\xff"), Some(b"\x00value\xff".to_vec()));
    redis.delete(b"probe\x00\xff");

    let seeds: u64 = std::env::var("STRATA_KV_DIFF_SEEDS")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(4);
    let ops: u64 = std::env::var("STRATA_KV_DIFF_OPS")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(160);
    for seed in 0..seeds {
        let mut rng = Rng(seed ^ 0x0BAD_CAFE);
        let mut strata = StrataKv::new();
        redis.command(&[b"FLUSHDB"]);
        let mut live: std::collections::BTreeSet<Vec<u8>> = std::collections::BTreeSet::new();
        for step in 0..ops {
            let choice = rng.below(100);
            if choice < 60 || live.is_empty() {
                let key = generated_key(&mut rng);
                let value = generated_value(&mut rng);
                strata.put(&key, &value);
                redis.put(&key, &value);
                live.insert(key);
            } else {
                #[allow(clippy::cast_possible_truncation)] // live set is tiny
                let index = rng.below(live.len() as u64) as usize;
                let key = live.iter().nth(index).expect("live key").clone();
                strata.delete(&key);
                redis.delete(&key);
                live.remove(&key);
            }
            let probe = generated_key(&mut rng);
            assert_eq!(
                strata.get(&probe),
                redis.get(&probe),
                "[seed={seed} step={step}] redis point read diverges on {probe:x?}"
            );
            assert_eq!(
                strata.count(),
                redis.count(),
                "[seed={seed} step={step}] redis count diverges"
            );
        }
    }
}
