//! Vector metadata filter scalar-matching edge cases.
//!
//! Pins the `Eq` filter semantics across scalar types: bit-exact number
//! comparison (so `-0.0` does not match a stored `+0.0`), integer/float
//! normalization through `as_f64`, null matching, and typed mismatches.

mod common;

use common::{branch, open_cache_database, space};
use serde_json::json;
use strata_engine::{
    VectorCollectionName, VectorConfig, VectorDistanceMetric, VectorEmbedding, VectorFilter,
    VectorKey, VectorMetadata, VectorScalar,
};

#[test]
fn vector_metadata_filter_scalar_matching_is_bit_exact_and_typed() {
    let mut db = open_cache_database().expect("cache database opens");
    let mut vectors = db
        .vector(branch("default"), space("default"))
        .expect("vector service opens");
    let collection = VectorCollectionName::new("docs").expect("collection name");
    vectors
        .create_collection(
            collection.clone(),
            VectorConfig::new(2, VectorDistanceMetric::Cosine).expect("config"),
        )
        .expect("create collection");

    for (key, metadata) in [
        ("k-null", json!({ "tag": null })),
        ("k-zero", json!({ "x": 0.0 })),
        ("k-five-int", json!({ "n": 5 })),
        ("k-five-float", json!({ "n": 5.0 })),
        ("k-true", json!({ "active": true })),
        ("k-string", json!({ "label": "a" })),
    ] {
        vectors
            .upsert(
                collection.clone(),
                VectorKey::new(key).expect("vector key"),
                VectorEmbedding::new(vec![1.0, 0.0]).expect("embedding"),
                Some(VectorMetadata::new(metadata).expect("metadata")),
            )
            .expect("upsert");
    }

    let query_embedding = VectorEmbedding::new(vec![1.0, 0.0]).expect("query embedding");
    let mut matched = |filter: VectorFilter| -> Vec<String> {
        let mut keys: Vec<String> = vectors
            .query(&collection, &query_embedding, 16, Some(&filter))
            .expect("query succeeds")
            .matches()
            .iter()
            .map(|hit| hit.entry().key().as_str().to_owned())
            .collect();
        keys.sort();
        keys
    };

    // Null matches a JSON null.
    assert_eq!(
        matched(
            VectorFilter::new()
                .eq("tag", VectorScalar::Null)
                .expect("filter")
        ),
        vec!["k-null".to_owned()]
    );
    // Integer and float metadata normalize through as_f64, so `5` matches both.
    assert_eq!(
        matched(VectorFilter::new().eq("n", 5).expect("filter")),
        vec!["k-five-float".to_owned(), "k-five-int".to_owned()]
    );
    // Number comparison is bit-exact: `-0.0` does not match a stored `+0.0`.
    assert!(matched(VectorFilter::new().eq("x", -0.0_f64).expect("filter")).is_empty());
    assert_eq!(
        matched(VectorFilter::new().eq("x", 0.0_f64).expect("filter")),
        vec!["k-zero".to_owned()]
    );
    // A string filter does not match a numeric value (type mismatch).
    assert!(matched(VectorFilter::new().eq("n", "5").expect("filter")).is_empty());
    // Boolean equality is exact.
    assert_eq!(
        matched(VectorFilter::new().eq("active", true).expect("filter")),
        vec!["k-true".to_owned()]
    );
    assert!(matched(VectorFilter::new().eq("active", false).expect("filter")).is_empty());
}
