//! TCP4.2d — event-log differential harness against Redis Streams, on the
//! 4.2b corpus rails (`tests/corpus/README.md`).
//!
//! A seeded generator appends the same typed-event workload into Strata
//! (the executor wire surface) and a Redis stream, then diffs the tiers
//! both engines provably share:
//!
//! - **Tier A (every step)**: total count, plus the ordered
//!   `(event_type, payload)` sequence of one sampled type.
//! - **Tier B (every 16 steps)**: the full global ordered sequence and the
//!   type set.
//!
//! The shared abstraction is an append-only totally-ordered log of
//! `(event_type, payload)` — Strata's global `sequence` corresponds to the
//! stream position. Kept OUT of the diff: wall-clock time ranges (Redis IDs
//! are server clocks, Strata event timestamps are its own wall clock —
//! neither comparable nor deterministic) and the hash chain (Strata-only;
//! its integrity is pinned by `event_verify_chain`, and the hashes derive
//! from wall-clock micros so the corpus scrubs them).
//!
//! The Redis oracle extends the 4.2a RESP client with array replies
//! (`XRANGE` returns nested arrays); each instance uses its own stream key
//! in scratch DB 15 (the 4.2c per-instance-namespace lesson, applied
//! before it bites). Positive controls prove both adapters read their own
//! writes before any divergence is trusted (the #2746 lesson).

use serde_json::{json, Value};

#[path = "parity/support.rs"]
mod support;

#[path = "parity/corpus.rs"]
mod corpus;

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

const EVENT_TYPES: [&str; 5] = [
    "user.created",
    "user.updated",
    "order.placed",
    "täsk.done",
    "ping",
];

fn generated_payload(rng: &mut Rng) -> Value {
    match rng.below(4) {
        0 => json!({"id": rng.below(10_000), "live": rng.below(2) == 0}),
        1 => json!({"näme": format!("s{}", rng.below(1_000))}),
        2 => json!({"nested": {"depth": rng.below(100)}, "tags": ["a", "b"]}),
        _ => json!({}),
    }
}

/// The shared append-only-log tier both engines implement.
trait EventOracle {
    fn append(&mut self, event_type: &str, payload: &Value);
    /// The full global order as `(event_type, payload)`.
    fn events_ordered(&mut self) -> Vec<(String, Value)>;
    /// The ordered events of one type.
    fn events_of_type(&mut self, event_type: &str) -> Vec<(String, Value)>;
    fn count(&mut self) -> u64;
    /// Sorted distinct type set.
    fn types(&mut self) -> Vec<String>;
}

/// Strata through the executor wire surface (cache mode).
struct StrataEvents(strata_executor::Executor);

impl StrataEvents {
    fn new() -> Self {
        Self(support::executor())
    }

    fn range(&mut self, filter: Option<&str>) -> Vec<(String, Value)> {
        let mut request = json!({
            "type": "event_range", "start_seq": 0,
            "direction": "forward", "limit": 8192,
        });
        if let Some(event_type) = filter {
            request["event_type"] = json!(event_type);
        }
        let page = support::run(&mut self.0, &request);
        assert_eq!(
            page["data"]["has_more"], false,
            "domain outgrew the range limit: {page}"
        );
        page["data"]["items"]
            .as_array()
            .unwrap_or_else(|| panic!("range carries items: {page}"))
            .iter()
            .map(|item| {
                (
                    item["event"]["event_type"]
                        .as_str()
                        .expect("event type")
                        .to_owned(),
                    item["event"]["payload"].clone(),
                )
            })
            .collect()
    }
}

impl EventOracle for StrataEvents {
    fn append(&mut self, event_type: &str, payload: &Value) {
        support::run(
            &mut self.0,
            &json!({"type": "event_append", "event_type": event_type, "payload": payload}),
        );
    }
    fn events_ordered(&mut self) -> Vec<(String, Value)> {
        self.range(None)
    }
    fn events_of_type(&mut self, event_type: &str) -> Vec<(String, Value)> {
        self.range(Some(event_type))
    }
    fn count(&mut self) -> u64 {
        support::run(&mut self.0, &json!({"type": "event_count"}))["data"]["count"]
            .as_u64()
            .expect("event count")
    }
    fn types(&mut self) -> Vec<String> {
        let page = support::run(&mut self.0, &json!({"type": "event_list_types"}));
        let mut types: Vec<String> = page["data"]["items"]
            .as_array()
            .unwrap_or_else(|| panic!("type list carries items: {page}"))
            .iter()
            .map(|item| item.as_str().expect("type string").to_owned())
            .collect();
        types.sort();
        types
    }
}

/// A RESP value: everything the stream commands reply with.
#[derive(Debug)]
enum Resp {
    Simple(Vec<u8>),
    Bulk(Option<Vec<u8>>),
    Integer(i64),
    Array(Vec<Resp>),
}

/// Redis Streams over the 4.2a RESP client extended with array replies;
/// one stream key per instance in scratch DB 15.
struct RedisEvents {
    stream: std::net::TcpStream,
    reader: std::io::BufReader<std::net::TcpStream>,
    key: String,
}

static REDIS_INSTANCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl RedisEvents {
    fn connect() -> Option<Self> {
        let address = std::env::var("STRATA_REDIS_URL").ok()?;
        let stream = std::net::TcpStream::connect(&address)
            .unwrap_or_else(|err| panic!("STRATA_REDIS_URL={address} set but unreachable: {err}"));
        let reader = std::io::BufReader::new(stream.try_clone().expect("clone stream"));
        let instance = REDIS_INSTANCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let key = format!("events-{}-{instance}", std::process::id());
        let mut redis = Self {
            stream,
            reader,
            key,
        };
        match redis.command(&[b"SELECT", b"15"]) {
            Resp::Simple(ok) if ok == b"OK" => {}
            other => panic!("SELECT 15 must acknowledge: {other:?}"),
        }
        let key_bytes = redis.key.clone();
        redis.command(&[b"DEL", key_bytes.as_bytes()]);
        Some(redis)
    }

    fn command(&mut self, parts: &[&[u8]]) -> Resp {
        use std::io::Write;
        let mut request = format!("*{}\r\n", parts.len()).into_bytes();
        for part in parts {
            request.extend_from_slice(format!("${}\r\n", part.len()).as_bytes());
            request.extend_from_slice(part);
            request.extend_from_slice(b"\r\n");
        }
        self.stream.write_all(&request).expect("redis write");
        self.read_value()
    }

    fn read_value(&mut self) -> Resp {
        use std::io::{BufRead, Read};
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("redis read");
        let (kind, rest) = line.split_at(1);
        let rest = rest.trim_end();
        match kind {
            "+" => Resp::Simple(rest.as_bytes().to_vec()),
            ":" => Resp::Integer(rest.parse().expect("integer reply")),
            "$" => {
                let length: i64 = rest.parse().expect("bulk length");
                if length < 0 {
                    return Resp::Bulk(None);
                }
                let mut buffer = vec![0_u8; usize::try_from(length).expect("len") + 2];
                self.reader.read_exact(&mut buffer).expect("bulk body");
                buffer.truncate(buffer.len() - 2);
                Resp::Bulk(Some(buffer))
            }
            "*" => {
                let length: i64 = rest.parse().expect("array length");
                let entries = (0..length.max(0)).map(|_| self.read_value()).collect();
                Resp::Array(entries)
            }
            "-" => panic!("redis error: {rest}"),
            other => panic!("unexpected RESP kind {other:?}"),
        }
    }

    /// Every `(event_type, payload)` in stream order.
    fn entries(&mut self) -> Vec<(String, Value)> {
        let key = self.key.clone();
        let reply = self.command(&[b"XRANGE", key.as_bytes(), b"-", b"+"]);
        let Resp::Array(items) = reply else {
            panic!("XRANGE returns an array: {reply:?}");
        };
        items
            .iter()
            .map(|entry| {
                // Each entry: [id, [field, value, field, value, ...]].
                let Resp::Array(pair) = entry else {
                    panic!("stream entry shape: {entry:?}");
                };
                let Resp::Array(fields) = &pair[1] else {
                    panic!("stream fields shape: {pair:?}");
                };
                let mut event_type = None;
                let mut payload = None;
                for chunk in fields.chunks(2) {
                    let (Resp::Bulk(Some(name)), Resp::Bulk(Some(value))) = (&chunk[0], &chunk[1])
                    else {
                        panic!("stream field shape: {chunk:?}");
                    };
                    match name.as_slice() {
                        b"t" => event_type = Some(String::from_utf8(value.clone()).expect("type")),
                        b"p" => {
                            payload =
                                Some(serde_json::from_slice(value).expect("payload round-trips"));
                        }
                        other => panic!("unexpected stream field {other:?}"),
                    }
                }
                (
                    event_type.expect("type field"),
                    payload.expect("payload field"),
                )
            })
            .collect()
    }
}

impl EventOracle for RedisEvents {
    fn append(&mut self, event_type: &str, payload: &Value) {
        let key = self.key.clone();
        let body = payload.to_string();
        self.command(&[
            b"XADD",
            key.as_bytes(),
            b"*",
            b"t",
            event_type.as_bytes(),
            b"p",
            body.as_bytes(),
        ]);
    }
    fn events_ordered(&mut self) -> Vec<(String, Value)> {
        self.entries()
    }
    fn events_of_type(&mut self, event_type: &str) -> Vec<(String, Value)> {
        self.entries()
            .into_iter()
            .filter(|(entry_type, _)| entry_type == event_type)
            .collect()
    }
    fn count(&mut self) -> u64 {
        let key = self.key.clone();
        match self.command(&[b"XLEN", key.as_bytes()]) {
            Resp::Integer(count) => u64::try_from(count).expect("non-negative"),
            other => panic!("XLEN returns an integer: {other:?}"),
        }
    }
    fn types(&mut self) -> Vec<String> {
        let mut types: Vec<String> = self
            .entries()
            .into_iter()
            .map(|(event_type, _)| event_type)
            .collect();
        types.sort();
        types.dedup();
        types
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn run_differential(seed: u64, ops: u64, mut recorder: Option<&mut Vec<corpus::CorpusCase>>) {
    let Some(mut redis) = RedisEvents::connect() else {
        eprintln!("SKIP: STRATA_REDIS_URL not set; event differential needs a live Redis");
        return;
    };
    let mut strata = StrataEvents::new();
    let mut rng = Rng(seed);

    let mut record = |strata: &mut StrataEvents, op: Value| {
        let observed = corpus::execute_canonicalized(&mut strata.0, &op);
        if let Some(cases) = recorder.as_deref_mut() {
            cases.push(corpus::CorpusCase {
                op,
                expect: observed.clone(),
            });
        }
        observed
    };

    for step in 0..ops {
        let sampled =
            EVENT_TYPES[usize::try_from(rng.below(EVENT_TYPES.len() as u64)).expect("bounded")];
        match rng.below(100) {
            // Append — the log's only mutation.
            0..=69 => {
                let payload = generated_payload(&mut rng);
                let _ = record(
                    &mut strata,
                    json!({"type": "event_append", "event_type": sampled, "payload": payload}),
                );
                redis.append(sampled, &payload);
            }
            // Read probes: recorded (and the chain check rides along); the
            // tier diffs below are the cross-engine oracle.
            70..=84 => {
                let _ = record(&mut strata, json!({"type": "event_count"}));
                let _ = record(&mut strata, json!({"type": "event_list_types"}));
            }
            _ => {
                let _ = record(&mut strata, json!({"type": "event_verify_chain"}));
                let _ = record(
                    &mut strata,
                    json!({
                        "type": "event_range", "start_seq": 0,
                        "direction": "forward", "limit": 8192, "event_type": sampled,
                    }),
                );
            }
        }

        // Tier A: total count + one sampled type's ordered events.
        assert_eq!(
            strata.count(),
            redis.count(),
            "seed={seed} step={step}: event counts diverged"
        );
        assert_eq!(
            strata.events_of_type(sampled),
            redis.events_of_type(sampled),
            "seed={seed} step={step}: ordered events of type {sampled} diverged"
        );

        // Tier B: the full global order + the type set, every 16 steps.
        if step % 16 == 15 {
            assert_eq!(
                strata.events_ordered(),
                redis.events_ordered(),
                "seed={seed} step={step}: global event order diverged"
            );
            assert_eq!(
                strata.types(),
                redis.types(),
                "seed={seed} step={step}: type sets diverged"
            );
        }
    }
}

#[test]
fn event_differential_vs_redis_streams() {
    let seeds = env_u64("STRATA_EVENT_DIFF_SEEDS", 2);
    let ops = env_u64("STRATA_EVENT_DIFF_OPS", 200);
    for seed in 1..=seeds {
        run_differential(seed, ops, None);
    }
}

/// Sabotage + positive controls (the #2746 lesson): both adapters read
/// their own writes, and a deliberately skewed append is caught.
#[test]
fn event_adapters_pass_positive_controls() {
    let Some(mut redis) = RedisEvents::connect() else {
        eprintln!("SKIP: STRATA_REDIS_URL not set");
        return;
    };
    let mut strata = StrataEvents::new();
    let payload = json!({"id": 7, "nested": {"ok": true}});
    strata.append("ctl.event", &payload);
    redis.append("ctl.event", &payload);

    assert_eq!(strata.count(), 1);
    assert_eq!(redis.count(), 1);
    assert_eq!(
        strata.events_ordered(),
        vec![("ctl.event".to_owned(), payload.clone())]
    );
    assert_eq!(
        redis.events_ordered(),
        vec![("ctl.event".to_owned(), payload)]
    );

    // Sabotage: skew Redis only; the diff MUST see it.
    redis.append("ctl.skew", &json!({"id": 8}));
    assert_ne!(
        strata.events_ordered(),
        redis.events_ordered(),
        "sabotaged append was invisible to the diff"
    );
}

/// Corpus recorder — `STRATA_CORPUS_RECORD=1` (local, never CI); requires a
/// live Redis so every recorded expectation was reference-validated.
#[test]
fn record_event_corpus() {
    const RECORD_SEEDS: [u64; 4] = [31, 32, 33, 34];
    const RECORD_OPS: u64 = 250;
    if std::env::var("STRATA_CORPUS_RECORD").is_err() {
        return;
    }
    assert!(
        std::env::var("STRATA_REDIS_URL").is_ok(),
        "corpus recording requires live Redis validation (corpus/README.md)"
    );
    for seed in RECORD_SEEDS {
        let mut first = Vec::new();
        run_differential(seed, RECORD_OPS, Some(&mut first));
        let mut second = Vec::new();
        run_differential(seed, RECORD_OPS, Some(&mut second));
        assert_eq!(first.len(), second.len(), "seed {seed}: op count differs");
        for (index, (a, b)) in first.iter().zip(&second).enumerate() {
            assert_eq!(
                a.op, b.op,
                "seed {seed} case {index}: generator is not deterministic"
            );
            assert_eq!(
                a.expect, b.expect,
                "seed {seed} case {index}: output is not deterministic after \
                 canonicalization — a volatile field is missing from the scrub list"
            );
        }
        let name = format!("event-{seed:04}");
        let contents = corpus::corpus_file_contents(
            &name,
            "event",
            seed,
            "differential_event v1",
            "redis-streams (record time)",
            &first,
        );
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/corpus")
            .join(format!("{name}.jsonl"));
        std::fs::write(&path, contents).expect("write corpus file");
        eprintln!("recorded {} cases to {}", first.len(), path.display());
    }
}
