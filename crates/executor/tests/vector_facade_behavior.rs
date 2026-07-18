//! Coverage for the `Executor` vector convenience facade (`facade/vector.rs`).
//!
//! Each facade method is a thin wrapper that fills `branch`/`space` with `None`
//! (the default branch and space) and forwards to `execute`. The risk in such a
//! layer is a transcription bug — a swapped argument, a wrong `as_of` default,
//! a mis-named field. So every method is checked against its explicit
//! `Command` with `branch: None, space: None`: the same lifecycle runs on two
//! fresh cache executors, one driven through the facade and one through raw
//! commands, and the outputs must be identical at every step. Cache mode is
//! deterministic, so two executors given the same operations produce equal
//! `Output`s (which derive `PartialEq`).

use serde_json::json;
use strata_executor::{
    BatchVectorEntry, Command, Executor, VectorDistanceMetric, VectorFilterCondition,
    VectorMetadataFilter,
};

/// Runs a facade call and its equivalent explicit command on the two lockstep
/// executors and asserts the outputs match.
macro_rules! assert_facade_matches {
    ($direct:ident, $facade_call:expr, $command:expr) => {{
        let facade_output = $facade_call.expect("facade call succeeds");
        let direct_output = $direct
            .execute($command)
            .expect("explicit command succeeds");
        assert_eq!(
            facade_output, direct_output,
            "facade output must equal the explicit command output"
        );
    }};
}

// One coherent lifecycle exercising all 19 facade methods against their
// explicit commands; kept as a single test so the two executors stay in
// lockstep across the whole sequence.
#[allow(clippy::too_many_lines)]
#[test]
fn vector_facade_matches_explicit_commands() {
    let mut facade = Executor::open_cache().expect("facade executor opens");
    let mut direct = Executor::open_cache().expect("direct executor opens");

    let metadata = json!({ "kind": "doc" });
    let filter = VectorMetadataFilter::new(vec![VectorFilterCondition::eq("kind", "note")]);

    assert_facade_matches!(
        direct,
        facade.vector_create_collection("docs", 2, VectorDistanceMetric::Cosine),
        Command::VectorCreateCollection {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            dimension: 2,
            metric: VectorDistanceMetric::Cosine,
        }
    );
    assert_facade_matches!(
        direct,
        facade.vector_list_collections(),
        Command::VectorListCollections {
            branch: None,
            space: None,
        }
    );
    assert_facade_matches!(
        direct,
        facade.vector_collection_stats("docs"),
        Command::VectorCollectionStats {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
        }
    );
    assert_facade_matches!(
        direct,
        facade.vector_upsert("docs", "a", vec![1.0, 0.0], Some(metadata.clone())),
        Command::VectorUpsert {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            key: "a".to_owned(),
            vector: vec![1.0, 0.0],
            metadata: Some(metadata.clone()),
        }
    );
    assert_facade_matches!(
        direct,
        facade.vector_get("docs", "a"),
        Command::VectorGet {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            key: "a".to_owned(),
            as_of: None,
        }
    );
    assert_facade_matches!(
        direct,
        facade.vector_exists("docs", "a"),
        Command::VectorExists {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            key: "a".to_owned(),
        }
    );
    assert_facade_matches!(
        direct,
        facade.vector_count("docs"),
        Command::VectorCount {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            as_of: None,
        }
    );
    assert_facade_matches!(
        direct,
        facade.vector_list_keys("docs", None, None, None),
        Command::VectorListKeys {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            prefix: None,
            cursor: None,
            limit: None,
            as_of: None,
        }
    );
    assert_facade_matches!(
        direct,
        facade.vector_query("docs", vec![1.0, 0.0], 5, None),
        Command::VectorQuery {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            query: vec![1.0, 0.0],
            k: 5,
            filter: None,
            as_of: None,
        }
    );
    assert_facade_matches!(
        direct,
        facade.vector_index_query("docs", vec![1.0, 0.0], 5, None),
        Command::VectorIndexQuery {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            query: vec![1.0, 0.0],
            k: 5,
            filter: None,
            as_of: None,
        }
    );
    assert_facade_matches!(
        direct,
        facade.vector_update_metadata("docs", "a", json!({ "kind": "note" })),
        Command::VectorUpdateMetadata {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            key: "a".to_owned(),
            patch: json!({ "kind": "note" }),
        }
    );
    assert_facade_matches!(
        direct,
        facade.vector_history("docs", "a"),
        Command::VectorHistory {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            key: "a".to_owned(),
        }
    );
    assert_facade_matches!(
        direct,
        facade.vector_batch_upsert(
            "docs",
            vec![
                BatchVectorEntry::new("b", vec![0.0, 1.0], Some(json!({ "kind": "doc" }))),
                BatchVectorEntry::new("c", vec![0.5, 0.5], Some(json!({ "kind": "doc" }))),
            ],
        ),
        Command::VectorBatchUpsert {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            entries: vec![
                BatchVectorEntry::new("b", vec![0.0, 1.0], Some(json!({ "kind": "doc" }))),
                BatchVectorEntry::new("c", vec![0.5, 0.5], Some(json!({ "kind": "doc" }))),
            ],
        }
    );
    assert_facade_matches!(
        direct,
        facade.vector_batch_get("docs", vec!["a".to_owned(), "b".to_owned()]),
        Command::VectorBatchGet {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            keys: vec!["a".to_owned(), "b".to_owned()],
        }
    );
    assert_facade_matches!(
        direct,
        facade.vector_batch_delete("docs", vec!["b".to_owned()]),
        Command::VectorBatchDelete {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            keys: vec!["b".to_owned()],
        }
    );
    assert_facade_matches!(
        direct,
        facade.vector_delete_by_filter("docs", filter.clone()),
        Command::VectorDeleteByFilter {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            filter: filter.clone(),
        }
    );
    assert_facade_matches!(
        direct,
        facade.vector_delete("docs", "c"),
        Command::VectorDelete {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            key: "c".to_owned(),
        }
    );
    assert_facade_matches!(
        direct,
        facade.vector_delete_all("docs"),
        Command::VectorDeleteAll {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
        }
    );
    assert_facade_matches!(
        direct,
        facade.vector_delete_collection("docs"),
        Command::VectorDeleteCollection {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
        }
    );
}
