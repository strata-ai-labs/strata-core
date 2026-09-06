//! TCP4.4c — exact-kNN ground-truth conformance for vector search.
//!
//! A 2,500-vector subsample of the TEXMEX `siftsmall` corpus (the dataset
//! lineage behind ANN-Benchmarks) with top-10 ground truth for 100 queries
//! under all three shipped metrics, computed in pure-Python double
//! precision — an independent toolchain — and validated against the corpus
//! authors' own ground truth before subsampling (see the vendored README).
//!
//! Strata's V1 vector search is an EXACT scan, so this is a conformance
//! contract, not a benchmark: the returned top-10 must match the expected
//! ids in the exact expected order (the vendored rankings have zero
//! boundary ties), and every returned score must match the independently
//! computed value — dot products exactly (SIFT components are integers, so
//! every dot fits f32 losslessly below 2^24), euclidean/cosine to f32
//! tolerance. This is the first external oracle for the hand-rolled
//! distance implementations in `engine/src/data/vector/distance.rs`.
//!
//! When a real ANN index lands, this harness becomes its recall-regression
//! gate: recall drops below 1.0 by design, and the assert changes from
//! exact-match to a thresholded recall measured against this ground truth.

use std::path::PathBuf;

use strata_executor::{Command, Executor, Output, VectorDistanceMetric};

const BASE_COUNT: usize = 2_500;
const QUERY_COUNT: usize = 100;
const DIMENSION: usize = 128;
const K: usize = 10;

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/conformance/sift")
}

fn load_u8_matrix(name: &str, rows: usize) -> Vec<Vec<f64>> {
    let bytes = std::fs::read(data_dir().join(name)).expect("vendored vectors present");
    assert_eq!(bytes.len(), rows * DIMENSION, "{name}: unexpected size");
    bytes
        .chunks_exact(DIMENSION)
        .map(|row| row.iter().map(|&component| f64::from(component)).collect())
        .collect()
}

struct Expected {
    ids: Vec<usize>,
    scores: Vec<f64>,
}

fn load_ground_truth(metric: &str) -> Vec<Expected> {
    let text = std::fs::read_to_string(data_dir().join(format!("gt_{metric}.tsv")))
        .expect("vendored ground truth present");
    let mut expected = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
        let (query, entries) = line.split_once('\t').expect("query column");
        assert_eq!(
            query.parse::<usize>().expect("query index"),
            line_no,
            "ground truth rows are ordered by query"
        );
        let mut ids = Vec::new();
        let mut scores = Vec::new();
        for entry in entries.split(',') {
            let (id, score) = entry.split_once(':').expect("id:score entry");
            ids.push(id.parse::<usize>().expect("neighbor id"));
            scores.push(score.parse::<f64>().expect("neighbor score"));
        }
        assert_eq!(ids.len(), K, "zero boundary ties: exactly K entries");
        expected.push(Expected { ids, scores });
    }
    assert_eq!(expected.len(), QUERY_COUNT);
    expected
}

fn run_metric(
    executor: &mut Executor,
    collection: &str,
    metric: VectorDistanceMetric,
    metric_name: &str,
    base: &[Vec<f64>],
    queries: &[Vec<f64>],
) {
    executor
        .execute(Command::VectorCreateCollection {
            branch: None,
            space: None,
            collection: collection.to_owned(),
            dimension: DIMENSION as u64,
            metric,
        })
        .expect("collection creates");
    for (index, vector) in base.iter().enumerate() {
        executor
            .execute(Command::VectorUpsert {
                branch: None,
                space: None,
                collection: collection.to_owned(),
                key: format!("v{index}"),
                vector: vector.clone(),
                metadata: None,
            })
            .expect("vector upserts");
    }

    let expected = load_ground_truth(metric_name);
    for (query_index, (query, truth)) in queries.iter().zip(&expected).enumerate() {
        let Output::VectorMatches(result) = executor
            .execute(Command::VectorQuery {
                branch: None,
                space: None,
                collection: collection.to_owned(),
                query: query.clone(),
                k: K as u64,
                filter: None,
                as_of: None,
                as_of_time: None,
            })
            .expect("query succeeds")
        else {
            panic!("unexpected vector query output");
        };
        let matches = &result;
        assert_eq!(
            matches.len(),
            K,
            "{metric_name} query {query_index}: wrong match count"
        );
        for (rank, matched) in matches.iter().enumerate() {
            let expected_key = format!("v{}", truth.ids[rank]);
            assert_eq!(
                matched.key(),
                expected_key,
                "{metric_name} query {query_index} rank {rank}: neighbor diverges \
                 from independent exact ground truth"
            );
            let observed = f64::from(matched.score());
            let reference = truth.scores[rank];
            // Dot products of integer vectors are exact in f32 below 2^24;
            // euclidean/cosine narrow a f64 accumulation to f32 at the end.
            let tolerance = if metric == VectorDistanceMetric::DotProduct {
                0.0
            } else {
                1e-6 * reference.abs().max(1.0)
            };
            assert!(
                (observed - reference).abs() <= tolerance,
                "{metric_name} query {query_index} rank {rank}: score {observed} \
                 diverges from independent value {reference}"
            );
        }
    }
}

#[test]
fn exact_search_matches_independent_ground_truth_for_every_metric() {
    let base = load_u8_matrix("base-2500x128.u8bin", BASE_COUNT);
    let queries = load_u8_matrix("queries-100x128.u8bin", QUERY_COUNT);

    let mut executor = Executor::open_cache().expect("cache executor opens");
    for (collection, metric, name) in [
        ("sift-l2", VectorDistanceMetric::Euclidean, "euclidean"),
        ("sift-dot", VectorDistanceMetric::DotProduct, "dot"),
        ("sift-cos", VectorDistanceMetric::Cosine, "cosine"),
    ] {
        run_metric(&mut executor, collection, metric, name, &base, &queries);
    }
}
