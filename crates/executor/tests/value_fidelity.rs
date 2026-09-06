//! TCP4.6a — generative value-fidelity sweeps (round-trip / read==write).
//!
//! The 4.6 charter upgrade: oracles that fail on VALUE LOSS, not just panics.
//! Both seeds are fixed and point-pinned; this lane extends them to seeded
//! generative sweeps at the wire — the layer both bugs were observed through:
//!
//! - **#2689** (an f64 embedding whose f32 cast underflowed was silently
//!   stored as the zero vector): for arbitrary f64 vectors, upsert must either
//!   refuse with `invalid_argument.engine.vector_embedding` — exactly when a
//!   component is non-finite, overflows f32, or underflows to zero — or store
//!   values whose read-back equals the f32-narrowed input component-for
//!   component. Both directions: a refusal on a representable vector is as
//!   wrong as a silent store of an unrepresentable one. Point pin:
//!   `vector_behavior.rs` "a subnormal embedding is rejected".
//! - **#2688** (arrow vector export→import stringified metadata under a
//!   literal `metadata` key and leaked the internal `vector_revision` field):
//!   arbitrary metadata objects — nested, unicode, adversarial key names
//!   including `metadata` and `vector_revision` themselves — must round-trip
//!   through export→import byte-identically, with embeddings intact and no
//!   internal fields injected. Point pin: `arrow_behavior.rs`
//!   `vector_export_import_round_trip_preserves_metadata_without_leaking_internals`.
//!
//! Deterministic: seeded `SplitMix64` (the oracle-suite pattern); the domain is
//! adversarial-by-construction rather than volume-driven, so the sweep runs
//! per-PR; `STRATA_FIDELITY_CASES` scales the per-seed case count for manual
//! deep runs.

use serde_json::Value;
use strata_executor::{Command, Executor, Output, VectorDistanceMetric};

/// `SplitMix64` — tiny, seedable, deterministic (the oracle-suite pattern).
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn pick<'a, T>(&mut self, pool: &'a [T]) -> &'a T {
        &pool[usize::try_from(self.next_u64() % pool.len() as u64).expect("bounded")]
    }
}

fn env_cases(default: usize) -> usize {
    std::env::var("STRATA_FIDELITY_CASES")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(default)
}

fn create_collection(executor: &mut Executor, name: &str, dimension: u64) {
    executor
        .execute(Command::VectorCreateCollection {
            branch: None,
            space: None,
            collection: name.to_owned(),
            dimension,
            metric: VectorDistanceMetric::Cosine,
        })
        .expect("collection creates");
}

#[expect(
    clippy::result_large_err,
    reason = "thin wrapper mirroring Executor::execute's public signature"
)]
fn upsert(
    executor: &mut Executor,
    collection: &str,
    key: &str,
    vector: Vec<f64>,
    metadata: Option<Value>,
) -> strata_executor::ExecutorResult<Output> {
    executor.execute(Command::VectorUpsert {
        branch: None,
        space: None,
        collection: collection.to_owned(),
        key: key.to_owned(),
        vector,
        metadata,
    })
}

/// Read one vector back: `(embedding, metadata)`.
fn read_back(executor: &mut Executor, collection: &str, key: &str) -> (Vec<f32>, Option<Value>) {
    let output = executor
        .execute(Command::VectorGet {
            branch: None,
            space: None,
            collection: collection.to_owned(),
            key: key.to_owned(),
            as_of: None,
            as_of_time: None,
        })
        .expect("vector get succeeds");
    let Output::VectorData(value) = output else {
        panic!("unexpected vector get output");
    };
    let value = value.into_option().expect("stored vector present");
    (
        value.data().embedding().to_vec(),
        value.data().metadata().cloned(),
    )
}

// --- #2689 class: f64 -> f32 narrowing fidelity --------------------------

/// The adversarial component pool: representable values, f32 boundaries, and
/// every non-representable class (overflow, underflow, subnormal-of-f64,
/// non-finite). Random walks alone practically never land on these.
const COMPONENT_POOL: &[f64] = &[
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.25,
    -2.75,
    1e-30,
    -1e-30,
    1e30,
    3.402_823_466e38, // ~f32::MAX, representable
    3.5e38,           // overflows f32 -> inf
    -3.5e38,          // overflows negative
    1.175_494_35e-38, // ~f32::MIN_POSITIVE, representable
    1e-39,            // f32-subnormal but non-zero: representable, must survive
    1e-46,            // underflows f32 to zero: must refuse (the #2689 class)
    -1e-46,           // negative underflow
    1e-308,           // the literal #2689 seed component
    5e-324,           // smallest positive f64 (deep underflow)
    1e308,            // f64-huge overflow
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::NAN,
];

/// The fully-representable subset: half the sweep draws only from here so the
/// stored arm (the read==write oracle) gets dense coverage deterministically,
/// not by luck against a pool that refuses ~91% of 4-component draws.
const SAFE_POOL: &[f64] = &[
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.25,
    -2.75,
    1e-30,
    -1e-30,
    1e30,
    3.402_823_466e38, // rounds to f32::MAX, finite
    1.175_494_35e-38, // ~f32::MIN_POSITIVE
    1e-39,            // narrows to a non-zero f32 subnormal: must survive
];

/// Exact f32 equality IS the oracle — an epsilon would mask value drift. ±0.0
/// compare equal by design, and no stored NaN is reachable (refused at ingest).
#[expect(
    clippy::float_cmp,
    reason = "exact read-back equality is the fidelity oracle"
)]
fn stored_value_matches(got: f32, want: f32) -> bool {
    got == want
}

/// The fixed contract, both directions: refuse iff a component's f32 cast is
/// non-finite or collapses a non-zero value to zero.
fn must_refuse(vector: &[f64]) -> bool {
    vector.iter().any(|value| {
        #[allow(clippy::cast_possible_truncation, reason = "the narrowing under test")]
        let narrowed = *value as f32;
        !narrowed.is_finite() || (narrowed == 0.0 && *value != 0.0)
    })
}

#[test]
fn embedding_narrowing_never_loses_value_silently() {
    const DIMENSION: usize = 4;
    let cases = env_cases(64);
    let mut executor = Executor::open_cache().expect("cache executor opens");
    create_collection(&mut executor, "fidelity", DIMENSION as u64);

    let mut refused = 0_usize;
    let mut stored = 0_usize;
    for seed in [1_u64, 2, 3, 4] {
        let mut rng = Rng(seed);
        for case in 0..cases {
            let pool = if case % 2 == 0 {
                SAFE_POOL
            } else {
                COMPONENT_POOL
            };
            let vector: Vec<f64> = (0..DIMENSION).map(|_| *rng.pick(pool)).collect();
            let key = format!("s{seed}c{case}");
            let expect_refusal = must_refuse(&vector);
            match upsert(&mut executor, "fidelity", &key, vector.clone(), None) {
                Ok(_) => {
                    assert!(
                        !expect_refusal,
                        "[seed={seed} case={case}] {vector:?} contains a non-representable \
                         component but was ACCEPTED — the #2689 silent-loss class"
                    );
                    stored += 1;
                    let (embedding, _) = read_back(&mut executor, "fidelity", &key);
                    #[allow(clippy::cast_possible_truncation, reason = "the narrowing under test")]
                    let expected: Vec<f32> = vector.iter().map(|value| *value as f32).collect();
                    // Component-wise `==` (no stored NaN is possible: refused
                    // above), so any silent value change fails — the read==write
                    // oracle, at f32 precision by documented contract.
                    assert_eq!(
                        embedding.len(),
                        expected.len(),
                        "[seed={seed} case={case}] dimension changed on read-back"
                    );
                    for (index, (got, want)) in embedding.iter().zip(&expected).enumerate() {
                        assert!(
                            stored_value_matches(*got, *want),
                            "[seed={seed} case={case}] component {index} stored {want} \
                             but read back {got} (input {})",
                            vector[index]
                        );
                    }
                }
                Err(error) => {
                    assert!(
                        expect_refusal,
                        "[seed={seed} case={case}] {vector:?} is fully f32-representable \
                         but was refused: {} ({})",
                        error.code(),
                        error
                    );
                    assert_eq!(
                        error.code(),
                        "invalid_argument.engine.vector_embedding",
                        "[seed={seed} case={case}] wrong refusal code"
                    );
                    refused += 1;
                }
            }
        }
    }
    // Non-vacuity: the pool must drive BOTH arms, heavily.
    assert!(
        stored > 20,
        "sweep must store representable vectors (got {stored})"
    );
    assert!(
        refused > 20,
        "sweep must refuse unrepresentable vectors (got {refused})"
    );
}

#[test]
fn the_literal_2689_seed_input_is_refused() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    create_collection(&mut executor, "seed2689", 3);
    let error = upsert(
        &mut executor,
        "seed2689",
        "z",
        vec![1e-308, 1e-308, 1e-308],
        None,
    )
    .expect_err("the #2689 seed input must be refused, never silently zeroed");
    assert_eq!(error.code(), "invalid_argument.engine.vector_embedding");
}

// --- #2688 class: arrow export→import metadata fidelity -------------------

#[cfg(feature = "arrow")]
mod arrow_fidelity {
    use serde_json::json;
    use strata_executor::{ArrowExportPrimitive, ArrowFileFormat, ArrowImportTarget};
    use tempfile::TempDir;

    use super::*;

    const KEY_POOL: &[&str] = &[
        "kind",
        "rank",
        // Adversarial: user fields named like the wire wrapper and the internal
        // field #2688 leaked — they must round-trip as USER data.
        "metadata",
        "vector_revision",
        "名前",
        "empty string ok?",
        "n",
    ];

    fn value_for(rng: &mut Rng, depth: usize) -> Value {
        match rng.next_u64() % if depth == 0 { 8 } else { 6 } {
            0 => json!("note"),
            1 => json!("ünïcødé 🚀"),
            2 => json!(""),
            3 => json!(i64::try_from(rng.next_u64() % 1_000_000).expect("bounded")),
            4 => json!(0.5 + f64::from(u32::try_from(rng.next_u64() % 8).expect("bounded")) * 0.25), // f64-exact
            5 => json!(rng.next_u64() % 2 == 0),
            6 => {
                let mut object = serde_json::Map::new();
                object.insert("nested".to_owned(), value_for(rng, depth + 1));
                Value::Object(object)
            }
            _ => json!([1, "two", null, true]),
        }
    }

    fn metadata_for(rng: &mut Rng) -> Value {
        let mut object = serde_json::Map::new();
        let fields = rng.next_u64() % 4; // 0..=3 fields; 0 = empty object
        for _ in 0..fields {
            let key = (*rng.pick(KEY_POOL)).to_owned();
            object.insert(key, value_for(rng, 0));
        }
        Value::Object(object)
    }

    fn exact_embedding(rng: &mut Rng, dimension: usize) -> Vec<f64> {
        // Multiples of 0.25 are exact in f32, so the parquet round-trip must be
        // bit-clean (the 4.2e domain trick).
        (0..dimension)
            .map(|_| f64::from(u32::try_from(rng.next_u64() % 32).expect("bounded")) * 0.25 - 4.0)
            .collect()
    }

    #[test]
    fn arrow_vector_round_trip_preserves_arbitrary_metadata_and_embeddings() {
        const DIMENSION: usize = 4;
        let rows = env_cases(24);
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("fidelity.parquet");

        let mut executor = Executor::open_cache().expect("cache executor opens");
        create_collection(&mut executor, "src", DIMENSION as u64);
        create_collection(&mut executor, "dst", DIMENSION as u64);

        // Seed `src` with generated rows, plus the literal #2688 shape and a
        // metadata-free row.
        let mut expected: Vec<(String, Vec<f64>, Option<Value>)> = Vec::new();
        let mut rng = Rng(0x2688);
        for row in 0..rows {
            let key = format!("r{row:03}");
            let embedding = exact_embedding(&mut rng, DIMENSION);
            let metadata = metadata_for(&mut rng);
            expected.push((key, embedding, Some(metadata)));
        }
        expected.push((
            "seed-2688".to_owned(),
            vec![0.25, 0.5, 0.75, 1.0],
            Some(json!({"kind": "note", "rank": 1})),
        ));
        expected.push(("bare".to_owned(), vec![1.0, 0.0, 0.0, 0.0], None));
        for (key, embedding, metadata) in &expected {
            upsert(
                &mut executor,
                "src",
                key,
                embedding.clone(),
                metadata.clone(),
            )
            .expect("seed upsert succeeds");
        }

        executor
            .execute(Command::ArrowExport {
                branch: None,
                space: None,
                primitive: ArrowExportPrimitive::Vector,
                format: ArrowFileFormat::Parquet,
                path: path.to_string_lossy().into_owned(),
                prefix: None,
                limit: None,
                collection: Some("src".to_owned()),
                graph: None,
                event_type: None,
            })
            .expect("export succeeds");
        executor
            .execute(Command::ArrowImport {
                branch: None,
                space: None,
                file_path: path.to_string_lossy().into_owned(),
                format: Some(ArrowFileFormat::Parquet),
                target: ArrowImportTarget::Vector,
                key_column: None,
                value_column: None,
                collection: Some("dst".to_owned()),
                graph: None,
            })
            .expect("import succeeds");

        for (key, embedding, metadata) in &expected {
            let (got_embedding, got_metadata) = read_back(&mut executor, "dst", key);
            #[allow(clippy::cast_possible_truncation, reason = "wire f64 -> stored f32")]
            let want_embedding: Vec<f32> = embedding.iter().map(|value| *value as f32).collect();
            assert_eq!(
                got_embedding, want_embedding,
                "embedding for `{key}` changed across the arrow round trip"
            );
            // Exact JSON equality: catches stringified wrapping, dropped or
            // mutated fields, AND injected internals (`vector_revision`) in one
            // comparison — the #2688 oracle on arbitrary shapes.
            assert_eq!(
                &got_metadata, metadata,
                "metadata for `{key}` changed across the arrow round trip"
            );
        }
    }
}
