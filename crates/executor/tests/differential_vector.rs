//! TCP4.2e — vector differential harness against an in-process exact
//! k-NN oracle, on the 4.2b corpus rails (`tests/corpus/README.md`).
//!
//! Unlike 4.2a-d there is no external reference engine: external vector
//! databases are *approximate* by design, so the reference is a
//! brute-force f64 scorer over a shadow copy of every live vector — the
//! one oracle that is exact by construction. Because the oracle is
//! in-process, this lane needs no service container and no feature gate:
//! it runs on every PR (env-scaled up for the nightly soak).
//!
//! A seeded generator drives upsert/delete/query workloads over three
//! collections, one per wire metric (`cosine`, `euclidean`,
//! `dot_product`), and checks every query result against the oracle:
//!
//! - **Per query** (both `vector_query` and `vector_index_query`): result
//!   length, descending score order, per-match score against the oracle
//!   formula (within epsilon), per-match metadata equality, and exact
//!   top-k membership with tie tolerance — every vector scoring strictly
//!   above the k-th oracle score must be returned, and nothing scoring
//!   strictly below it may be.
//! - **Tier A (every step)**: `vector_count` vs shadow size.
//! - **Tier B (every 16 steps)**: `vector_list_keys` vs shadow keys plus
//!   a full-ordering sweep (k = whole collection) per collection.
//!
//! Score formulas pinned by probing: cosine = normalized similarity
//! (0.0 when either norm is zero — no NaN), euclidean = 1/(1+d),
//! `dot_product` = raw dot; matches sort descending; filters are
//! AND-composed top-level `eq` conditions applied BEFORE top-k selection
//! (a k=1 query with a filter matching only the globally-worst vector
//! returns that vector). The generated domain uses quantized components
//! (multiples of 0.25, exactly representable in f32) so exact score ties
//! occur on purpose and drift stays inside epsilon; the key pool (96) is
//! larger than `collection_exact_threshold` (64) so accumulation crosses
//! the engine's exact→flat index boundary mid-run. Both query paths must
//! be exact at this scale — the resolved index kinds are exhaustive scans.

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

const DIMENSION: usize = 8;
const KEY_POOL: u64 = 96;
const METRICS: [&str; 3] = ["cosine", "euclidean", "dot_product"];
const TAGS: [&str; 4] = ["alpha", "bëta", "gamma", "delta"];
/// Score tolerance: engine scores are f32, the oracle is f64; on the
/// quantized domain dots are exact and only sqrt/division drift.
const EPSILON: f64 = 1e-4;

fn score(metric: &str, query: &[f64], vector: &[f64]) -> f64 {
    let dot: f64 = query.iter().zip(vector).map(|(a, b)| a * b).sum();
    match metric {
        "cosine" => {
            let nq = query.iter().map(|a| a * a).sum::<f64>().sqrt();
            let nv = vector.iter().map(|a| a * a).sum::<f64>().sqrt();
            if nq == 0.0 || nv == 0.0 {
                0.0
            } else {
                dot / (nq * nv)
            }
        }
        "euclidean" => {
            let d2: f64 = query
                .iter()
                .zip(vector)
                .map(|(a, b)| (a - b) * (a - b))
                .sum();
            1.0 / (1.0 + d2.sqrt())
        }
        "dot_product" => dot,
        other => panic!("unknown metric {other}"),
    }
}

/// Shadow copy of one collection: key → (vector, metadata-or-Null).
type Shadow = std::collections::BTreeMap<String, (Vec<f64>, Value)>;

/// One parsed wire match.
struct Match {
    key: String,
    score: f64,
    metadata: Value,
}

fn parse_matches(items: &Value) -> Vec<Match> {
    items
        .as_array()
        .unwrap_or_else(|| panic!("matches are an array: {items}"))
        .iter()
        .map(|item| Match {
            key: item["key"].as_str().expect("match key").to_owned(),
            score: item["score"].as_f64().expect("match score"),
            metadata: item.get("metadata").cloned().unwrap_or(Value::Null),
        })
        .collect()
}

/// The exact-top-k oracle check with tie tolerance.
fn check_matches(
    context: &str,
    metric: &str,
    shadow: &Shadow,
    filter: Option<(&str, &str)>,
    query: &[f64],
    k: usize,
    matches: &[Match],
) {
    let subset: Vec<(&String, f64)> = shadow
        .iter()
        .filter(|(_, (_, metadata))| match filter {
            Some((field, value)) => metadata.get(field) == Some(&json!(value)),
            None => true,
        })
        .map(|(key, (vector, _))| (key, score(metric, query, vector)))
        .collect();

    let expected_len = k.min(subset.len());
    assert_eq!(
        matches.len(),
        expected_len,
        "{context}: expected {expected_len} matches, got {}",
        matches.len()
    );
    for window in matches.windows(2) {
        assert!(
            window[0].score >= window[1].score - 1e-6,
            "{context}: matches are not sorted descending ({} then {})",
            window[0].score,
            window[1].score
        );
    }

    let mut oracle_sorted: Vec<f64> = subset.iter().map(|(_, s)| *s).collect();
    oracle_sorted.sort_by(|a, b| b.partial_cmp(a).expect("finite scores"));
    // The k-th best oracle score bounds membership: strictly-better must
    // be present, strictly-worse must not be.
    let boundary = oracle_sorted.get(expected_len.saturating_sub(1)).copied();

    let returned: std::collections::BTreeSet<&str> =
        matches.iter().map(|m| m.key.as_str()).collect();
    for matched in matches {
        let (_, expected_score) = subset
            .iter()
            .find(|(key, _)| **key == matched.key)
            .unwrap_or_else(|| {
                panic!(
                    "{context}: returned key {} is not in the (filtered) shadow",
                    matched.key
                )
            });
        assert!(
            (matched.score - expected_score).abs() <= EPSILON,
            "{context}: score for {} diverged: engine {} vs oracle {}",
            matched.key,
            matched.score,
            expected_score
        );
        let (_, expected_metadata) = &shadow[&matched.key];
        assert_eq!(
            &matched.metadata, expected_metadata,
            "{context}: metadata for {} diverged",
            matched.key
        );
        if let Some(bound) = boundary {
            assert!(
                subset
                    .iter()
                    .find(|(key, _)| **key == matched.key)
                    .is_some_and(|(_, s)| *s >= bound - EPSILON),
                "{context}: {} returned but scores below the top-{expected_len} boundary",
                matched.key
            );
        }
    }
    if let Some(bound) = boundary {
        for (key, oracle_score) in &subset {
            assert!(
                *oracle_score <= bound + EPSILON || returned.contains(key.as_str()),
                "{context}: {key} scores {oracle_score} (above boundary {bound}) but was \
                 not returned"
            );
        }
    }
}

fn generated_vector(rng: &mut Rng) -> Vec<f64> {
    // Quantized components: multiples of 0.25 in [-2, 2], exactly
    // representable in f32 — exact ties happen on purpose. Zero vectors
    // are legal domain (probed: cosine guards them to score 0.0).
    (0..DIMENSION)
        .map(|_| {
            let quantum = i32::try_from(rng.below(17)).expect("bounded") - 8;
            f64::from(quantum) * 0.25
        })
        .collect()
}

fn generated_metadata(rng: &mut Rng) -> Value {
    let tag = TAGS[usize::try_from(rng.below(TAGS.len() as u64)).expect("bounded")];
    match rng.below(3) {
        0 => Value::Null,
        1 => json!({"tag": tag}),
        _ => json!({"tag": tag, "n": rng.below(100)}),
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn collection_name(metric: &str) -> String {
    format!("diff_{metric}")
}

#[expect(
    clippy::too_many_lines,
    reason = "single narrative op-loop; the checker and adapters are already extracted"
)]
fn run_differential(seed: u64, ops: u64, mut recorder: Option<&mut Vec<corpus::CorpusCase>>) {
    let mut executor = support::executor();
    let mut shadows: std::collections::BTreeMap<String, Shadow> = std::collections::BTreeMap::new();
    let mut rng = Rng(seed);

    let record = |executor: &mut strata_executor::Executor,
                  recorder: &mut Option<&mut Vec<corpus::CorpusCase>>,
                  op: Value| {
        let observed = corpus::execute_canonicalized(executor, &op);
        if let Some(cases) = recorder.as_deref_mut() {
            cases.push(corpus::CorpusCase {
                op,
                expect: observed.clone(),
            });
        }
        observed
    };

    for metric in METRICS {
        let collection = collection_name(metric);
        record(
            &mut executor,
            &mut recorder,
            json!({"type": "vector_create_collection", "collection": collection,
                   "dimension": DIMENSION, "metric": metric}),
        )
        .expect("collection create succeeds");
        shadows.insert(collection, Shadow::new());
    }

    for step in 0..ops {
        let metric = METRICS[usize::try_from(rng.below(METRICS.len() as u64)).expect("bounded")];
        let collection = collection_name(metric);
        let key = format!("v{:03}", rng.below(KEY_POOL));
        match rng.below(100) {
            0..=54 => {
                let vector = generated_vector(&mut rng);
                let metadata = generated_metadata(&mut rng);
                let mut op = json!({"type": "vector_upsert", "collection": collection,
                                    "key": key, "vector": vector});
                if !metadata.is_null() {
                    op["metadata"] = metadata.clone();
                }
                record(&mut executor, &mut recorder, op).expect("upsert succeeds");
                shadows
                    .get_mut(&collection)
                    .expect("shadow exists")
                    .insert(key, (vector, metadata));
            }
            55..=69 => {
                let outcome = record(
                    &mut executor,
                    &mut recorder,
                    json!({"type": "vector_delete", "collection": collection, "key": key}),
                )
                .expect("delete succeeds");
                let removed = shadows
                    .get_mut(&collection)
                    .expect("shadow exists")
                    .remove(&key)
                    .is_some();
                assert_eq!(
                    outcome["data"]["effect"]["affected_count"].as_u64(),
                    Some(u64::from(removed)),
                    "seed={seed} step={step}: delete effect diverged for {key}"
                );
            }
            _ => {
                let query = generated_vector(&mut rng);
                let k = [1_usize, 3, 8, 17][usize::try_from(rng.below(4)).expect("bounded")];
                let filter = (rng.below(10) < 3).then(|| {
                    (
                        "tag",
                        TAGS[usize::try_from(rng.below(TAGS.len() as u64)).expect("bounded")],
                    )
                });
                let mut op = json!({"type": "vector_query", "collection": collection,
                                    "query": query, "k": k});
                if let Some((field, value)) = filter {
                    op["filter"] = json!({"conditions": [{"field": field, "op": "eq",
                        "value": {"type": "string", "value": value}}]});
                }
                let mut index_op = op.clone();
                index_op["type"] = json!("vector_index_query");

                let shadow = &shadows[&collection];
                let plain = record(&mut executor, &mut recorder, op).expect("query succeeds");
                check_matches(
                    &format!("seed={seed} step={step} {metric} vector_query"),
                    metric,
                    shadow,
                    filter,
                    &query,
                    k,
                    &parse_matches(&plain["data"]),
                );
                let indexed =
                    record(&mut executor, &mut recorder, index_op).expect("index query succeeds");
                check_matches(
                    &format!("seed={seed} step={step} {metric} vector_index_query"),
                    metric,
                    shadow,
                    filter,
                    &query,
                    k,
                    &parse_matches(&indexed["data"]["matches"]),
                );
            }
        }

        // Tier A: count vs shadow size for the touched collection.
        let count = record(
            &mut executor,
            &mut recorder,
            json!({"type": "vector_count", "collection": collection}),
        )
        .expect("count succeeds");
        assert_eq!(
            count["data"].as_u64(),
            Some(shadows[&collection].len() as u64),
            "seed={seed} step={step}: {collection} count diverged"
        );

        // Tier B: full key listing + full-ordering sweep, every collection.
        if step % 16 == 15 {
            for metric in METRICS {
                let collection = collection_name(metric);
                let shadow = &shadows[&collection];
                let listing = record(
                    &mut executor,
                    &mut recorder,
                    json!({"type": "vector_list_keys", "collection": collection, "limit": 256}),
                )
                .expect("list succeeds");
                let listed: Vec<&str> = listing["data"]["items"]
                    .as_array()
                    .expect("key items")
                    .iter()
                    .map(|item| item.as_str().expect("key string"))
                    .collect();
                let expected: Vec<&str> = shadow.keys().map(String::as_str).collect();
                assert_eq!(
                    listed, expected,
                    "seed={seed} step={step}: {collection} key listing diverged"
                );

                let sweep_query = generated_vector(&mut rng);
                let sweep = record(
                    &mut executor,
                    &mut recorder,
                    json!({"type": "vector_query", "collection": collection,
                           "query": sweep_query, "k": 256}),
                )
                .expect("sweep succeeds");
                check_matches(
                    &format!("seed={seed} step={step} {metric} full sweep"),
                    metric,
                    shadow,
                    None,
                    &sweep_query,
                    256,
                    &parse_matches(&sweep["data"]),
                );
            }
        }
    }

    // Strata-side regression riders for the corpus: dimension mismatch and
    // the k=0 empty page.
    let mismatch = record(
        &mut executor,
        &mut recorder,
        json!({"type": "vector_upsert", "collection": collection_name("cosine"),
               "key": "bad", "vector": [1.0, 2.0]}),
    );
    assert_eq!(
        mismatch,
        Err("invalid_argument.engine.vector_dimension".to_owned()),
        "dimension mismatch must be rejected"
    );
    let empty = record(
        &mut executor,
        &mut recorder,
        json!({"type": "vector_query", "collection": collection_name("cosine"),
               "query": generated_vector(&mut rng), "k": 0}),
    )
    .expect("k=0 succeeds");
    assert_eq!(
        empty["data"].as_array().map(Vec::len),
        Some(0),
        "k=0 returns an empty page"
    );
}

#[test]
fn vector_differential_vs_exact_knn_oracle() {
    let seeds = env_u64("STRATA_VECTOR_DIFF_SEEDS", 2);
    let ops = env_u64("STRATA_VECTOR_DIFF_OPS", 150);
    for seed in 1..=seeds {
        run_differential(seed, ops, None);
    }
}

/// Positive control (the #2746 lesson): known geometry must come back with
/// the exact expected keys and scores through the same parse path the
/// differential uses — proving the adapter reads real data.
#[test]
fn vector_adapter_passes_positive_controls() {
    let mut executor = support::executor();
    support::run(
        &mut executor,
        &json!({"type": "vector_create_collection", "collection": "ctl",
                "dimension": 3, "metric": "cosine"}),
    );
    for (key, vector) in [("ux", [1.0, 0.0, 0.0]), ("uy", [0.0, 1.0, 0.0])] {
        support::run(
            &mut executor,
            &json!({"type": "vector_upsert", "collection": "ctl", "key": key, "vector": vector}),
        );
    }
    let page = support::run(
        &mut executor,
        &json!({"type": "vector_query", "collection": "ctl", "query": [1.0, 0.0, 0.0], "k": 2}),
    );
    let matches = parse_matches(&page["data"]);
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].key, "ux");
    assert!((matches[0].score - 1.0).abs() <= EPSILON);
    assert_eq!(matches[1].key, "uy");
    assert!(matches[1].score.abs() <= EPSILON);
}

/// Sabotage: a phantom high-scoring shadow entry must make the oracle
/// check panic — proving the differential is non-vacuous.
#[test]
fn oracle_check_detects_a_planted_divergence() {
    let mut shadow = Shadow::new();
    shadow.insert("real".to_owned(), (vec![1.0, 0.0, 0.0], Value::Null));
    shadow.insert("phantom".to_owned(), (vec![1.0, 0.0, 0.0], Value::Null));
    let matches = [Match {
        key: "real".to_owned(),
        score: 1.0,
        metadata: Value::Null,
    }];
    let caught = std::panic::catch_unwind(|| {
        check_matches(
            "sabotage",
            "cosine",
            &shadow,
            None,
            &[1.0, 0.0, 0.0],
            2,
            &matches,
        );
    });
    assert!(
        caught.is_err(),
        "a match list missing a top-k member was invisible to the oracle check"
    );
}

/// Corpus recorder — `STRATA_CORPUS_RECORD=1` (local, never CI). The
/// oracle is in-process, so every recorded expectation is
/// reference-validated by construction.
#[test]
fn record_vector_corpus() {
    const RECORD_SEEDS: [u64; 4] = [41, 42, 43, 44];
    const RECORD_OPS: u64 = 250;
    if std::env::var("STRATA_CORPUS_RECORD").is_err() {
        return;
    }
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
        let name = format!("vector-{seed:04}");
        let contents = corpus::corpus_file_contents(
            &name,
            "vector",
            seed,
            "differential_vector v1",
            "in-process exact k-NN oracle (record time)",
            &first,
        );
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/corpus")
            .join(format!("{name}.jsonl"));
        std::fs::write(&path, contents).expect("write corpus file");
        eprintln!("recorded {} cases to {}", first.len(), path.display());
    }
}
