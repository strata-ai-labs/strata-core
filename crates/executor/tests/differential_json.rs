//! TCP4.2b — JSON differential harness against `MongoDB`, plus the corpus
//! recorder (the SLT completion-mode format; see `tests/corpus/README.md`).
//!
//! A seeded generator drives the same document workload into Strata (the
//! executor wire surface) and `MongoDB`, then diffs the tiers both engines
//! provably share:
//!
//! - **Tier A (every step)**: full-document get by key, existence, and the
//!   prefix count.
//! - **Tier B (every 16 steps)**: ordered key listing under the generator
//!   prefix, and a full per-document state compare.
//!
//! The generated domain is restricted to the provable intersection:
//! printable keys, `$`-free top-level field names, i64 integers, fractional
//! doubles (so `1` vs `1.0` type identity survives the BSON round-trip),
//! unicode strings, booleans, nulls, arrays, and nested documents. Path
//! reads (Strata bracket-notation semantics, #2731) and dotted field names
//! are OUR-side semantics: they ride the corpus as self-regression cases
//! but are never diffed against `MongoDB`.
//!
//! The `MongoDB` oracle is a dependency-free `OP_MSG` client over the `bson`
//! codec (the Redis RESP-client precedent): connects to
//! `STRATA_MONGO_URL`, drops the scratch database, and skips loudly when
//! the env var is absent. Positive controls prove both adapters read their
//! own writes before any divergence is trusted (the #2746 lesson).

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

/// All generated documents live under this key prefix so prefix count and
/// listing have a stable anchor on both engines.
const KEY_PREFIX: &str = "d";

fn generated_key(rng: &mut Rng) -> String {
    format!("{KEY_PREFIX}{:04}", rng.below(400))
}

/// Shared-domain field names: no `$` prefix, no dots (dotted names are
/// Strata bracket-notation territory, kept out of the diffed intersection).
const FIELD_NAMES: [&str; 6] = ["alpha", "beta", "näme", "n0", "deep", "flag"];

/// Shared-domain scalar: i64 (full range), fractional double, unicode
/// string, bool, null.
fn generated_scalar(rng: &mut Rng) -> Value {
    match rng.below(5) {
        0 => {
            let magnitude = rng.next();
            #[allow(clippy::cast_possible_wrap)] // full-range i64 is the point
            let integer = magnitude as i64;
            json!(integer)
        }
        1 => {
            #[allow(clippy::cast_precision_loss)] // small domain
            let double = rng.below(1_000_000) as f64 + 0.5;
            json!(double)
        }
        2 => {
            let choices = ["", "plain", "uni•code ✓", "\u{1F5C3} astral", "line\nbreak"];
            #[allow(clippy::cast_possible_truncation)] // tiny bound
            let slot = rng.below(choices.len() as u64) as usize;
            json!(choices[slot])
        }
        3 => json!(rng.below(2) == 0),
        _ => Value::Null,
    }
}

fn generated_document(rng: &mut Rng, depth: u64) -> Value {
    let mut map = serde_json::Map::new();
    let fields = 1 + rng.below(4);
    for _ in 0..fields {
        #[allow(clippy::cast_possible_truncation)] // tiny bound
        let slot = rng.below(FIELD_NAMES.len() as u64) as usize;
        let name = FIELD_NAMES[slot].to_owned();
        let value = match rng.below(4) {
            0 if depth < 2 => generated_document(rng, depth + 1),
            1 if depth < 2 => {
                let items = (0..=rng.below(3))
                    .map(|_| generated_scalar(rng))
                    .collect::<Vec<_>>();
                Value::Array(items)
            }
            _ => generated_scalar(rng),
        };
        map.insert(name, value);
    }
    Value::Object(map)
}

/// The shared JSON tier both engines implement.
trait JsonOracle {
    fn set(&mut self, key: &str, document: &Value);
    fn delete(&mut self, key: &str);
    fn get_document(&mut self, key: &str) -> Option<Value>;
    fn count_prefixed(&mut self) -> u64;
    fn keys_ordered(&mut self) -> Vec<String>;
}

/// Strata through the executor wire surface (cache mode).
struct StrataJson(strata_executor::Executor);

impl StrataJson {
    fn new() -> Self {
        Self(support::executor())
    }
}

impl JsonOracle for StrataJson {
    fn set(&mut self, key: &str, document: &Value) {
        support::run(
            &mut self.0,
            &json!({"type": "json_set", "key": key, "path": "$", "value": document}),
        );
    }
    fn delete(&mut self, key: &str) {
        support::run(
            &mut self.0,
            &json!({"type": "json_delete", "key": key, "path": "$"}),
        );
    }
    fn get_document(&mut self, key: &str) -> Option<Value> {
        let output = support::run(
            &mut self.0,
            &json!({"type": "json_get", "key": key, "path": "$"}),
        );
        if output["data"]["found"].as_bool() == Some(false) {
            return None;
        }
        let value = &output["data"]["value"]["value"];
        assert!(
            !value.is_null() || output["data"]["found"].as_bool() == Some(true),
            "json_get envelope carries the document: {output}"
        );
        Some(value.clone())
    }
    fn count_prefixed(&mut self) -> u64 {
        support::run(
            &mut self.0,
            &json!({"type": "json_count", "prefix": KEY_PREFIX}),
        )["data"]
            .as_u64()
            .expect("json_count returns a count")
    }
    fn keys_ordered(&mut self) -> Vec<String> {
        let page = support::run(
            &mut self.0,
            &json!({"type": "json_list", "prefix": KEY_PREFIX, "limit": 4096}),
        );
        page["data"]["items"]
            .as_array()
            .unwrap_or_else(|| panic!("json_list carries items: {page}"))
            .iter()
            .map(|item| item.as_str().expect("list items are keys").to_owned())
            .collect()
    }
}

/// `MongoDB` over a minimal `OP_MSG` wire client (no driver): one scratch
/// database, `_id` = Strata key.
struct MongoJson {
    stream: std::net::TcpStream,
    request_id: i32,
    /// Per-connection collection: tests run in parallel against one shared
    /// server, so each harness namespaces its own collection and drops it
    /// at connect (cross-test contamination showed up as a step-0 count
    /// divergence on the first live run).
    collection: String,
}

const MONGO_DB: &str = "strata_diff";

impl MongoJson {
    fn connect(collection: &str) -> Option<Self> {
        let address = std::env::var("STRATA_MONGO_URL").ok()?;
        let stream = std::net::TcpStream::connect(&address)
            .unwrap_or_else(|err| panic!("STRATA_MONGO_URL={address} set but unreachable: {err}"));
        let mut mongo = Self {
            stream,
            request_id: 1,
            collection: collection.to_owned(),
        };
        mongo.drop_collection();
        Some(mongo)
    }

    /// Drops this harness's collection, tolerating absence.
    fn drop_collection(&mut self) {
        let reply = self.raw_command(bson::doc! {"drop": self.collection.clone()});
        if !reply_ok(&reply) {
            let code_name = reply.get_str("codeName").unwrap_or("");
            assert_eq!(
                code_name, "NamespaceNotFound",
                "collection drop failed: {reply:?}"
            );
        }
    }

    /// Sends one `OP_MSG` command document (section kind 0) and returns the
    /// reply document, asserting `ok: 1`.
    fn command(&mut self, command: bson::Document) -> bson::Document {
        let sent = command.clone();
        let document = self.raw_command(command);
        assert!(
            reply_ok(&document),
            "mongo command failed: {document:?} (command: {sent:?})"
        );
        document
    }

    /// Sends one `OP_MSG` command without asserting success.
    fn raw_command(&mut self, mut command: bson::Document) -> bson::Document {
        use std::io::{Read, Write};
        command.insert("$db", MONGO_DB);
        let body = bson::to_vec(&command).expect("encode command");
        let message_length = 16 + 4 + 1 + body.len();
        let mut message = Vec::with_capacity(message_length);
        message.extend_from_slice(
            &i32::try_from(message_length)
                .expect("message fits i32")
                .to_le_bytes(),
        );
        message.extend_from_slice(&self.request_id.to_le_bytes());
        self.request_id += 1;
        message.extend_from_slice(&0_i32.to_le_bytes()); // responseTo
        message.extend_from_slice(&2013_i32.to_le_bytes()); // OP_MSG
        message.extend_from_slice(&0_u32.to_le_bytes()); // flagBits
        message.push(0); // section kind 0
        message.extend_from_slice(&body);
        self.stream.write_all(&message).expect("mongo write");

        let mut header = [0_u8; 16];
        self.stream.read_exact(&mut header).expect("mongo header");
        let reply_length = usize::try_from(i32::from_le_bytes(
            header[0..4].try_into().expect("length bytes"),
        ))
        .expect("reply length is non-negative");
        let mut reply = vec![0_u8; reply_length - 16];
        self.stream.read_exact(&mut reply).expect("mongo body");
        // flagBits (4) + section kind (1) precede the reply document.
        let document: bson::Document =
            bson::from_slice(&reply[5..]).expect("decode mongo reply document");
        document
    }
}

/// Whether a mongo reply's `ok` field signals success (servers send it as
/// a double `1.0`; integer forms are accepted defensively).
fn reply_ok(reply: &bson::Document) -> bool {
    match reply.get("ok") {
        Some(bson::Bson::Double(ok)) => (ok - 1.0).abs() < f64::EPSILON,
        Some(bson::Bson::Int32(ok)) => *ok == 1,
        Some(bson::Bson::Int64(ok)) => *ok == 1,
        _ => false,
    }
}

/// Explicit JSON→BSON conversion — no extended-JSON surprises: i64 stays
/// integer, f64 stays double.
fn json_to_bson(value: &Value) -> bson::Bson {
    match value {
        Value::Null => bson::Bson::Null,
        Value::Bool(flag) => bson::Bson::Boolean(*flag),
        Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                bson::Bson::Int64(integer)
            } else {
                bson::Bson::Double(number.as_f64().expect("numeric value"))
            }
        }
        Value::String(text) => bson::Bson::String(text.clone()),
        Value::Array(items) => bson::Bson::Array(items.iter().map(json_to_bson).collect()),
        Value::Object(map) => {
            let mut document = bson::Document::new();
            for (key, child) in map {
                document.insert(key.clone(), json_to_bson(child));
            }
            bson::Bson::Document(document)
        }
    }
}

/// Explicit BSON→JSON conversion mirroring `json_to_bson`.
fn bson_to_json(value: &bson::Bson) -> Value {
    match value {
        bson::Bson::Null => Value::Null,
        bson::Bson::Boolean(flag) => json!(flag),
        bson::Bson::Int32(integer) => json!(i64::from(*integer)),
        bson::Bson::Int64(integer) => json!(integer),
        bson::Bson::Double(double) => json!(double),
        bson::Bson::String(text) => json!(text),
        bson::Bson::Array(items) => Value::Array(items.iter().map(bson_to_json).collect()),
        bson::Bson::Document(document) => {
            let mut map = serde_json::Map::new();
            for (key, child) in document {
                map.insert(key.clone(), bson_to_json(child));
            }
            Value::Object(map)
        }
        other => panic!("unexpected BSON type from the shared domain: {other:?}"),
    }
}

impl JsonOracle for MongoJson {
    fn set(&mut self, key: &str, document: &Value) {
        let bson::Bson::Document(mut replacement) = json_to_bson(document) else {
            panic!("generated documents are objects");
        };
        replacement.insert("_id", key);
        self.command(bson::doc! {
            "update": self.collection.clone(),
            "updates": [{"q": {"_id": key}, "u": replacement, "upsert": true}],
        });
    }
    fn delete(&mut self, key: &str) {
        self.command(bson::doc! {
            "delete": self.collection.clone(),
            "deletes": [{"q": {"_id": key}, "limit": 1}],
        });
    }
    fn get_document(&mut self, key: &str) -> Option<Value> {
        let reply = self.command(bson::doc! {
            "find": self.collection.clone(),
            "filter": {"_id": key},
            "limit": 1,
        });
        let batch = reply
            .get_document("cursor")
            .expect("find reply carries a cursor")
            .get_array("firstBatch")
            .expect("cursor carries firstBatch");
        let first = batch.first()?;
        let bson::Bson::Document(found) = first else {
            panic!("find returns documents: {first:?}");
        };
        let mut found = found.clone();
        found.remove("_id");
        Some(bson_to_json(&bson::Bson::Document(found)))
    }
    fn count_prefixed(&mut self) -> u64 {
        let reply = self.command(bson::doc! {
            "count": self.collection.clone(),
            "query": {"_id": {"$regex": format!("^{KEY_PREFIX}")}},
        });
        u64::try_from(
            reply
                .get_i32("n")
                .map(i64::from)
                .or_else(|_| reply.get_i64("n"))
                .expect("count reply carries n"),
        )
        .expect("count is non-negative")
    }
    fn keys_ordered(&mut self) -> Vec<String> {
        let reply = self.command(bson::doc! {
            "find": self.collection.clone(),
            "filter": {"_id": {"$regex": format!("^{KEY_PREFIX}")}},
            "sort": {"_id": 1},
            "projection": {"_id": 1},
            "batchSize": 4096,
        });
        reply
            .get_document("cursor")
            .expect("find reply carries a cursor")
            .get_array("firstBatch")
            .expect("cursor carries firstBatch")
            .iter()
            .map(|item| {
                let bson::Bson::Document(document) = item else {
                    panic!("projection returns documents: {item:?}");
                };
                document.get_str("_id").expect("_id is a string").to_owned()
            })
            .collect()
    }
}

/// One generated step: the shared-domain mutation plus our-side extras for
/// the corpus (path reads over the live document set).
#[expect(
    clippy::too_many_lines,
    reason = "one deliberately linear generated workload, like the stress lane"
)]
fn run_differential(seed: u64, ops: u64, recorder: Option<&mut Vec<corpus::CorpusCase>>) {
    let mut rng = Rng(seed);
    let mut strata = StrataJson::new();
    let mut mongo = MongoJson::connect(&format!("diff-{seed}"));
    if mongo.is_none() {
        assert!(
            std::env::var("CI").is_err(),
            "CI must provide STRATA_MONGO_URL for the JSON differential lane"
        );
        eprintln!("STRATA_MONGO_URL not set; JSON differential skipped (Strata-only run)");
    }
    let mut recorder = recorder;
    let mut live: std::collections::BTreeMap<String, Value> = std::collections::BTreeMap::new();

    let mut record = |strata: &mut StrataJson, op: Value| {
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
        let choice = rng.below(100);
        if choice < 45 || live.is_empty() {
            let key = generated_key(&mut rng);
            let document = generated_document(&mut rng, 0);
            record(
                &mut strata,
                json!({"type": "json_set", "key": key, "path": "$", "value": document}),
            )
            .expect("shared-domain set succeeds");
            if let Some(mongo) = mongo.as_mut() {
                mongo.set(&key, &document);
            }
            live.insert(key, document);
        } else if choice < 60 {
            // Overwrite an existing document.
            #[allow(clippy::cast_possible_truncation)] // live set is tiny
            let index = rng.below(live.len() as u64) as usize;
            let key = live.keys().nth(index).expect("live key").clone();
            let document = generated_document(&mut rng, 0);
            record(
                &mut strata,
                json!({"type": "json_set", "key": key, "path": "$", "value": document}),
            )
            .expect("shared-domain overwrite succeeds");
            if let Some(mongo) = mongo.as_mut() {
                mongo.set(&key, &document);
            }
            live.insert(key, document);
        } else if choice < 75 {
            // Delete an existing document.
            #[allow(clippy::cast_possible_truncation)] // live set is tiny
            let index = rng.below(live.len() as u64) as usize;
            let key = live.keys().nth(index).expect("live key").clone();
            record(
                &mut strata,
                json!({"type": "json_delete", "key": key, "path": "$"}),
            )
            .expect("delete of a live document succeeds");
            if let Some(mongo) = mongo.as_mut() {
                mongo.delete(&key);
            }
            live.remove(&key);
        } else if choice < 90 {
            // Full-document read of a live or absent key — recorded, and the
            // primary Tier A probe below diffs it against MongoDB.
            let key = generated_key(&mut rng);
            let _ = record(
                &mut strata,
                json!({"type": "json_get", "key": key, "path": "$"}),
            );
        } else {
            // OUR-side semantics for the corpus only: a path read against a
            // live document (bracket-notation surface, #2731). Never diffed
            // against MongoDB — path semantics deliberately diverge.
            if let Some((key, _)) = live.iter().nth(
                #[allow(clippy::cast_possible_truncation)] // live set is tiny
                {
                    rng.below(live.len().max(1) as u64) as usize
                },
            ) {
                let key = key.clone();
                let _ = record(
                    &mut strata,
                    json!({"type": "json_get", "key": key, "path": "$.alpha"}),
                );
            }
        }

        // Tier A: sampled full-document read + prefix count, every step.
        if let Some(mongo) = mongo.as_mut() {
            let probe = generated_key(&mut rng);
            assert_eq!(
                strata.get_document(&probe),
                mongo.get_document(&probe),
                "[seed={seed} step={step}] full-document read diverges on {probe}"
            );
            assert_eq!(
                strata.count_prefixed(),
                mongo.count_prefixed(),
                "[seed={seed} step={step}] prefix count diverges"
            );

            // Tier B: ordered key listing + full state, every 16 steps.
            if step % 16 == 15 {
                assert_eq!(
                    strata.keys_ordered(),
                    mongo.keys_ordered(),
                    "[seed={seed} step={step}] ordered key listing diverges"
                );
                for key in live.keys() {
                    assert_eq!(
                        strata.get_document(key),
                        mongo.get_document(key),
                        "[seed={seed} step={step}] stored document diverges on {key}"
                    );
                }
            }
        }
    }

    // Terminal full-state agreement.
    if let Some(mongo) = mongo.as_mut() {
        assert_eq!(
            strata.keys_ordered(),
            mongo.keys_ordered(),
            "[seed={seed}] terminal key set diverges"
        );
        for key in live.keys() {
            assert_eq!(
                strata.get_document(key),
                mongo.get_document(key),
                "[seed={seed}] terminal document diverges on {key}"
            );
        }
    }
}

/// Default lane: fixed seeds, bounded ops. `STRATA_JSON_DIFF_SEEDS` /
/// `STRATA_JSON_DIFF_OPS` scale the nightly soak without a code change.
#[test]
fn strata_json_agrees_with_mongodb_on_shared_tiers() {
    let seeds: u64 = std::env::var("STRATA_JSON_DIFF_SEEDS")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(4);
    let ops: u64 = std::env::var("STRATA_JSON_DIFF_OPS")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(120);
    for seed in 0..seeds {
        run_differential(seed, ops, None);
    }
}

/// Sabotage + positive controls: both adapters read their own writes, and a
/// fabricated divergence is detected (non-vacuousness).
#[test]
fn the_harness_detects_a_fabricated_divergence() {
    let Some(mut mongo) = MongoJson::connect("sabotage") else {
        assert!(
            std::env::var("CI").is_err(),
            "CI must provide STRATA_MONGO_URL for the JSON differential lane"
        );
        eprintln!("STRATA_MONGO_URL not set; sabotage check skipped");
        return;
    };
    let mut strata = StrataJson::new();
    let document = json!({"alpha": 1_i64, "deep": {"flag": true}, "näme": "uni•code ✓"});
    strata.set("d0001", &document);
    mongo.set("d0001", &document);
    // Positive control: a written document must read back through BOTH
    // adapters (the #2746 lesson).
    assert_eq!(strata.get_document("d0001"), Some(document.clone()));
    assert_eq!(mongo.get_document("d0001"), Some(document));
    // Fabricate the divergence.
    mongo.set("d0002", &json!({"phantom": true}));
    assert_ne!(strata.count_prefixed(), mongo.count_prefixed());
    assert_ne!(strata.keys_ordered(), mongo.keys_ordered());
}

/// Corpus recorder — `STRATA_CORPUS_RECORD=1` (local, never CI). Records
/// each seed twice and requires canonically identical outputs (a missed
/// volatile field fails recording, not replay), validates the shared tiers
/// against `MongoDB` live, then writes `tests/corpus/json-<seed>.jsonl`.
#[test]
fn record_json_corpus() {
    const RECORD_SEEDS: [u64; 4] = [1, 2, 3, 4];
    const RECORD_OPS: u64 = 250;
    if std::env::var("STRATA_CORPUS_RECORD").is_err() {
        return;
    }
    assert!(
        std::env::var("STRATA_MONGO_URL").is_ok(),
        "corpus recording requires live MongoDB validation (corpus/README.md)"
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
        let name = format!("json-{seed:04}");
        let contents = corpus::corpus_file_contents(
            &name,
            "json",
            seed,
            "differential_json v1",
            "mongodb (record time)",
            &first,
        );
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/corpus")
            .join(format!("{name}.jsonl"));
        std::fs::write(&path, contents).expect("write corpus file");
        eprintln!("recorded {} cases to {}", first.len(), path.display());
    }
}
