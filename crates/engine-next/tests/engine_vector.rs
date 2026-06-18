//! Vector conformance tests.

mod common;

use serde_json::{json, Map, Value};
use strata_engine_next::{
    Database, EngineErrorClass, VectorCollectionName, VectorConfig, VectorDistanceMetric,
    VectorEmbedding, VectorFilter, VectorFilterCondition, VectorKey, VectorMetadata,
    VectorMetadataPatch, VectorScalar, VectorUpsertEntry,
};

use common::{branch, open_cache_database, open_durable_database, space};

#[test]
fn vector_contract_runs_in_cache_and_durable_modes() {
    run_database_modes(exercise_vector_contract);
}

#[test]
fn vector_collection_lifecycle_and_counts_run_in_cache_and_durable_modes() {
    run_database_modes(exercise_vector_collection_lifecycle);
}

#[test]
fn vector_metadata_patch_contract_runs_in_cache_and_durable_modes() {
    run_database_modes(exercise_vector_metadata_patch_contract);
}

#[test]
fn vector_batch_contracts_run_in_cache_and_durable_modes() {
    run_database_modes(exercise_vector_batch_contracts);
}

#[test]
fn vector_bulk_delete_contracts_run_in_cache_and_durable_modes() {
    run_database_modes(exercise_vector_bulk_delete_contracts);
}

#[test]
fn vector_exact_search_metrics_run_in_cache_and_durable_modes() {
    run_database_modes(exercise_vector_exact_search_metrics);
}

#[test]
fn vector_timestamp_reads_track_overwrite_filter_and_delete() {
    run_database_modes(exercise_vector_timestamp_reads);
}

#[test]
fn vector_branch_and_space_isolation_match_other_primitives() {
    let mut database = open_cache_database().expect("cache open succeeds");
    {
        let mut vectors = vector_service(&mut database, "default", "default");
        vectors
            .create_collection(collection("docs"), config(2, VectorDistanceMetric::Cosine))
            .expect("collection create succeeds");
        vectors
            .upsert(
                collection("docs"),
                vector_key("shared"),
                embedding([1.0, 0.0]),
                Some(metadata(json!({"branch": "default"}))),
            )
            .expect("base upsert succeeds");
    }

    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("branch fork succeeds");

    {
        let mut feature = vector_service(&mut database, "feature", "default");
        let inherited = feature
            .get(&collection("docs"), &vector_key("shared"))
            .expect("read inherited vector succeeds")
            .expect("inherited vector exists");
        assert_eq!(
            inherited.metadata().expect("metadata").as_inner(),
            &json!({"branch": "default"})
        );
        feature
            .upsert(
                collection("docs"),
                vector_key("shared"),
                embedding([0.0, 1.0]),
                Some(metadata(json!({"branch": "feature"}))),
            )
            .expect("feature upsert succeeds");
    }

    let mut default_vectors = vector_service(&mut database, "default", "default");
    assert_eq!(
        default_vectors
            .get(&collection("docs"), &vector_key("shared"))
            .expect("default read succeeds")
            .expect("default vector exists")
            .metadata()
            .expect("metadata")
            .as_inner(),
        &json!({"branch": "default"})
    );
    drop(default_vectors);

    let mut feature_vectors = vector_service(&mut database, "feature", "default");
    assert_eq!(
        feature_vectors
            .get(&collection("docs"), &vector_key("shared"))
            .expect("feature read succeeds")
            .expect("feature vector exists")
            .metadata()
            .expect("metadata")
            .as_inner(),
        &json!({"branch": "feature"})
    );
    drop(feature_vectors);

    let mut other_space = vector_service(&mut database, "default", "other");
    other_space
        .create_collection(
            collection("docs"),
            config(2, VectorDistanceMetric::DotProduct),
        )
        .expect("other-space collection create succeeds");
    assert_eq!(
        other_space
            .collection_info(&collection("docs"))
            .expect("info succeeds")
            .expect("info exists")
            .config()
            .metric(),
        VectorDistanceMetric::DotProduct
    );
}

#[test]
fn vector_branch_destructive_operations_stay_isolated() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let collection = collection("docs");
    {
        let mut vectors = vector_service(&mut database, "default", "default");
        vectors
            .create_collection(collection.clone(), config(2, VectorDistanceMetric::Cosine))
            .expect("collection create succeeds");
        vectors
            .batch_upsert(
                &collection,
                &[
                    upsert("shared", [1.0, 0.0], json!({"kind": "keep"})),
                    upsert("delete", [0.0, 1.0], json!({"kind": "keep"})),
                    upsert("filter", [0.5, 0.5], json!({"kind": "remove"})),
                    upsert("all", [0.25, 0.75], json!({"kind": "keep"})),
                ],
            )
            .expect("batch upsert succeeds");
    }
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("branch fork succeeds");

    {
        let mut feature = vector_service(&mut database, "feature", "default");
        feature
            .update_metadata(
                &collection,
                vector_key("shared"),
                &patch(json!({"patched": true})),
            )
            .expect("metadata patch succeeds");
        assert!(feature
            .delete(&collection, vector_key("delete"))
            .expect("delete succeeds")
            .deleted());
        assert_eq!(
            feature
                .delete_by_filter(&collection, &filter_eq("kind", "remove"))
                .expect("filtered delete succeeds")
                .deleted_count(),
            1
        );
        assert_eq!(feature.count(&collection).expect("count succeeds"), 2);
        assert_eq!(
            feature
                .delete_all(&collection)
                .expect("delete all succeeds")
                .deleted_count(),
            2
        );
        assert_eq!(feature.count(&collection).expect("count succeeds"), 0);
        assert!(feature
            .delete_collection(&collection)
            .expect("collection delete succeeds"));
        assert!(feature
            .collection_info(&collection)
            .expect("info succeeds")
            .is_none());
    }

    let mut parent = vector_service(&mut database, "default", "default");
    assert_eq!(parent.count(&collection).expect("parent count succeeds"), 4);
    assert_eq!(
        parent
            .get(&collection, &vector_key("shared"))
            .expect("parent read succeeds")
            .expect("parent value exists")
            .metadata()
            .expect("metadata")
            .as_inner(),
        &json!({"kind": "keep"})
    );
    assert!(parent
        .get(&collection, &vector_key("delete"))
        .expect("parent read succeeds")
        .is_some());
    assert!(parent
        .get(&collection, &vector_key("filter"))
        .expect("parent read succeeds")
        .is_some());
}

#[test]
fn vector_space_destructive_operations_stay_isolated() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let collection = collection("docs");
    {
        let mut default_space = vector_service(&mut database, "default", "default");
        default_space
            .create_collection(collection.clone(), config(2, VectorDistanceMetric::Cosine))
            .expect("collection create succeeds");
        default_space
            .batch_upsert(
                &collection,
                &[
                    upsert("shared", [1.0, 0.0], json!({"space": "default"})),
                    upsert("keep", [0.0, 1.0], json!({"kind": "keep"})),
                ],
            )
            .expect("batch upsert succeeds");
    }
    {
        let mut other_space = vector_service(&mut database, "default", "other");
        other_space
            .create_collection(
                collection.clone(),
                config(2, VectorDistanceMetric::DotProduct),
            )
            .expect("collection create succeeds");
        other_space
            .batch_upsert(
                &collection,
                &[
                    upsert("shared", [0.0, 1.0], json!({"space": "other"})),
                    upsert("remove", [1.0, 0.0], json!({"kind": "remove"})),
                ],
            )
            .expect("batch upsert succeeds");
        assert_eq!(
            other_space
                .delete_by_filter(&collection, &filter_eq("kind", "remove"))
                .expect("filtered delete succeeds")
                .deleted_count(),
            1
        );
        assert!(other_space
            .delete_collection(&collection)
            .expect("collection delete succeeds"));
    }

    let mut default_space = vector_service(&mut database, "default", "default");
    assert_eq!(default_space.count(&collection).expect("count succeeds"), 2);
    assert_eq!(
        default_space
            .collection_info(&collection)
            .expect("info succeeds")
            .expect("collection exists")
            .config()
            .metric(),
        VectorDistanceMetric::Cosine
    );
    assert_eq!(
        default_space
            .get(&collection, &vector_key("shared"))
            .expect("read succeeds")
            .expect("value exists")
            .metadata()
            .expect("metadata")
            .as_inner(),
        &json!({"space": "default"})
    );
}

#[test]
fn vector_durable_reopen_preserves_collections_entries_and_history() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let collection = collection("durable");
    let key = vector_key("doc-1");
    let delete_timestamp;

    {
        let mut database = open_durable_database(tempdir.path()).expect("durable open succeeds");
        let mut vectors = database
            .vector(branch("default"), space("default"))
            .expect("vector service opens");
        vectors
            .create_collection(
                collection.clone(),
                config(2, VectorDistanceMetric::Euclidean),
            )
            .expect("collection create succeeds");
        vectors
            .upsert(
                collection.clone(),
                key.clone(),
                embedding([1.0, 0.0]),
                Some(metadata(json!({"stage": "created"}))),
            )
            .expect("first upsert succeeds");
        let updated = vectors
            .upsert(
                collection.clone(),
                key.clone(),
                embedding([0.0, 1.0]),
                Some(metadata(json!({"stage": "updated"}))),
            )
            .expect("second upsert succeeds");
        assert_eq!(updated.vector_revision(), 2);
        delete_timestamp = vectors
            .delete(&collection, vector_key("missing"))
            .expect("missing delete succeeds")
            .commit()
            .unwrap_or_else(|| updated.commit())
            .timestamp();
        drop(vectors);
        database.close().expect("close succeeds");
    }

    let mut reopened = open_durable_database(tempdir.path()).expect("reopen succeeds");
    let mut vectors = reopened
        .vector(branch("default"), space("default"))
        .expect("vector service opens");
    let info = vectors
        .collection_info(&collection)
        .expect("info succeeds")
        .expect("collection exists");
    assert_eq!(info.config().metric(), VectorDistanceMetric::Euclidean);
    assert_eq!(info.count(), 1);
    assert_eq!(
        vectors
            .get(&collection, &key)
            .expect("read succeeds")
            .expect("entry exists")
            .embedding()
            .as_slice(),
        &[0.0, 1.0]
    );
    let history = vectors
        .history(&collection, &key)
        .expect("history succeeds")
        .expect("history exists");
    assert_eq!(history.rows().len(), 2);
    assert_eq!(history.rows()[0].vector_revision(), Some(2));
    assert_eq!(history.rows()[1].vector_revision(), Some(1));
    assert_eq!(
        vectors
            .query_at(
                &collection,
                &embedding([0.0, 1.0]),
                10,
                None,
                delete_timestamp
            )
            .expect("timestamp query succeeds")
            .matches()
            .len(),
        1
    );
}

#[test]
fn vector_durable_reopen_preserves_collection_delete() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let collection = collection("deleted");
    let key = vector_key("doc-1");

    {
        let mut database = open_durable_database(tempdir.path()).expect("durable open succeeds");
        let mut vectors = vector_service(&mut database, "default", "default");
        vectors
            .create_collection(collection.clone(), config(2, VectorDistanceMetric::Cosine))
            .expect("collection create succeeds");
        vectors
            .upsert(
                collection.clone(),
                key.clone(),
                embedding([1.0, 0.0]),
                Some(metadata(json!({"stage": "before-delete"}))),
            )
            .expect("upsert succeeds");
        assert!(vectors
            .delete_collection(&collection)
            .expect("collection delete succeeds"));
        assert!(vectors
            .collection_info(&collection)
            .expect("latest info succeeds")
            .is_none());
        drop(vectors);
        database.close().expect("close succeeds");
    }

    let mut reopened = open_durable_database(tempdir.path()).expect("reopen succeeds");
    let mut vectors = vector_service(&mut reopened, "default", "default");
    assert!(vectors
        .collection_info(&collection)
        .expect("info succeeds")
        .is_none());
    assert_eq!(
        vectors
            .count(&collection)
            .expect_err("deleted collection count rejected")
            .code(),
        "not_found.engine.vector_collection"
    );
    assert_eq!(
        vectors
            .query(&collection, &embedding([1.0, 0.0]), 1, None)
            .expect_err("deleted collection query rejected")
            .code(),
        "not_found.engine.vector_collection"
    );
    let history = vectors
        .history(&collection, &key)
        .expect("history succeeds")
        .expect("history exists");
    assert!(history.rows()[0].is_tombstone());
    assert_eq!(history.rows()[1].vector_revision(), Some(1));
    drop(vectors);
    reopened.close().expect("close succeeds");
}

#[test]
fn vector_durable_reopen_preserves_branch_and_space_isolation() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let collection = collection("docs");
    {
        let mut database = open_durable_database(tempdir.path()).expect("durable open succeeds");
        {
            let mut vectors = vector_service(&mut database, "default", "default");
            vectors
                .create_collection(collection.clone(), config(2, VectorDistanceMetric::Cosine))
                .expect("collection create succeeds");
            vectors
                .upsert(
                    collection.clone(),
                    vector_key("shared"),
                    embedding([1.0, 0.0]),
                    Some(metadata(json!({"branch": "default"}))),
                )
                .expect("upsert succeeds");
        }
        database
            .branches()
            .expect("branch service opens")
            .fork_current(&branch("default"), branch("feature"))
            .expect("branch fork succeeds");
        {
            let mut feature = vector_service(&mut database, "feature", "default");
            feature
                .upsert(
                    collection.clone(),
                    vector_key("feature-only"),
                    embedding([0.0, 1.0]),
                    Some(metadata(json!({"branch": "feature"}))),
                )
                .expect("feature upsert succeeds");
        }
        {
            let mut other_space = vector_service(&mut database, "default", "other");
            other_space
                .create_collection(
                    collection.clone(),
                    config(2, VectorDistanceMetric::Euclidean),
                )
                .expect("space collection create succeeds");
            other_space
                .upsert(
                    collection.clone(),
                    vector_key("shared"),
                    embedding([0.0, 1.0]),
                    Some(metadata(json!({"space": "other"}))),
                )
                .expect("space upsert succeeds");
        }
        database.close().expect("close succeeds");
    }

    let mut reopened = open_durable_database(tempdir.path()).expect("reopen succeeds");
    let mut default_vectors = vector_service(&mut reopened, "default", "default");
    assert_eq!(
        default_vectors
            .get(&collection, &vector_key("shared"))
            .expect("default read succeeds")
            .expect("default vector exists")
            .metadata()
            .expect("metadata")
            .as_inner(),
        &json!({"branch": "default"})
    );
    assert!(default_vectors
        .get(&collection, &vector_key("feature-only"))
        .expect("default read succeeds")
        .is_none());
    drop(default_vectors);

    let mut feature_vectors = vector_service(&mut reopened, "feature", "default");
    assert!(feature_vectors
        .get(&collection, &vector_key("shared"))
        .expect("feature inherited read succeeds")
        .is_some());
    assert!(feature_vectors
        .get(&collection, &vector_key("feature-only"))
        .expect("feature read succeeds")
        .is_some());
    drop(feature_vectors);

    let mut other_space = vector_service(&mut reopened, "default", "other");
    assert_eq!(
        other_space
            .collection_info(&collection)
            .expect("space info succeeds")
            .expect("space collection exists")
            .config()
            .metric(),
        VectorDistanceMetric::Euclidean
    );
    assert_eq!(
        other_space
            .get(&collection, &vector_key("shared"))
            .expect("space read succeeds")
            .expect("space value exists")
            .metadata()
            .expect("metadata")
            .as_inner(),
        &json!({"space": "other"})
    );
}

#[test]
fn vector_invalid_inputs_are_engine_errors() {
    let error = VectorCollectionName::new("_internal").expect_err("reserved collection rejected");
    assert_eq!(error.class(), EngineErrorClass::InvalidInput);
    assert_eq!(
        error.code(),
        "invalid_argument.engine.vector_collection_reserved"
    );

    let error =
        VectorEmbedding::new([1.0, f32::INFINITY]).expect_err("non-finite embedding rejected");
    assert_eq!(error.class(), EngineErrorClass::InvalidInput);
    assert_eq!(error.code(), "invalid_argument.engine.vector_embedding");

    let error = VectorMetadataPatch::new(json!("not-object")).expect_err("patch rejected");
    assert_eq!(error.class(), EngineErrorClass::InvalidInput);
    assert_eq!(
        error.code(),
        "invalid_argument.engine.vector_metadata_patch"
    );

    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    vectors
        .create_collection(collection("docs"), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    let error = vectors
        .upsert(
            collection("docs"),
            vector_key("wrong-dim"),
            embedding([1.0, 2.0, 3.0]),
            None,
        )
        .expect_err("dimension mismatch rejected");
    assert_eq!(error.class(), EngineErrorClass::InvalidInput);
    assert_eq!(error.code(), "invalid_argument.engine.vector_dimension");

    let error = vectors
        .delete_by_filter(&collection("docs"), &VectorFilter::new())
        .expect_err("empty filtered delete rejected");
    assert_eq!(error.class(), EngineErrorClass::InvalidInput);
    assert_eq!(error.code(), "invalid_argument.engine.vector_filter");
}

#[test]
fn vector_missing_branch_errors_are_engine_owned() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "missing", "default");

    let error = vectors
        .create_collection(collection("docs"), config(2, VectorDistanceMetric::Cosine))
        .expect_err("create in missing branch rejected");
    assert_eq!(error.class(), EngineErrorClass::NotFound);
    assert_eq!(error.code(), "not_found.engine.branch");

    let error = vectors
        .upsert(
            collection("docs"),
            vector_key("a"),
            embedding([1.0, 0.0]),
            None,
        )
        .expect_err("upsert in missing branch rejected");
    assert_eq!(error.class(), EngineErrorClass::NotFound);
    assert_eq!(error.code(), "not_found.engine.branch");

    let error = vectors
        .get(&collection("docs"), &vector_key("a"))
        .expect_err("get in missing branch rejected");
    assert_eq!(error.class(), EngineErrorClass::NotFound);
    assert_eq!(error.code(), "not_found.engine.branch");

    let error = vectors
        .query(&collection("docs"), &embedding([1.0, 0.0]), 0, None)
        .expect_err("zero-limit query in missing branch rejected");
    assert_eq!(error.class(), EngineErrorClass::NotFound);
    assert_eq!(error.code(), "not_found.engine.branch");

    let error = vectors
        .batch_upsert(&collection("docs"), &[])
        .expect_err("empty batch upsert in missing branch rejected");
    assert_eq!(error.class(), EngineErrorClass::NotFound);
    assert_eq!(error.code(), "not_found.engine.branch");

    let error = vectors
        .batch_delete(&collection("docs"), &[])
        .expect_err("empty batch delete in missing branch rejected");
    assert_eq!(error.class(), EngineErrorClass::NotFound);
    assert_eq!(error.code(), "not_found.engine.branch");
}

#[test]
fn vector_noop_operations_validate_collection_and_query_shape() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let docs = collection("docs");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");

    let empty_keys = vectors
        .list_keys(&docs, None, None, 0)
        .expect("zero-limit key list succeeds");
    assert!(empty_keys.keys().is_empty());
    assert!(!empty_keys.has_more());
    assert_eq!(
        vectors
            .batch_upsert(&docs, &[])
            .expect("empty batch upsert succeeds")
            .commit(),
        None
    );
    assert_eq!(
        vectors
            .batch_delete(&docs, &[])
            .expect("empty batch delete succeeds")
            .commit(),
        None
    );
    assert!(vectors
        .query(&docs, &embedding([1.0, 0.0]), 0, None)
        .expect("zero-limit query succeeds")
        .matches()
        .is_empty());

    let missing = collection("missing");
    assert_eq!(
        vectors
            .list_keys(&missing, None, None, 0)
            .expect_err("zero-limit key list validates collection")
            .code(),
        "not_found.engine.vector_collection"
    );
    assert_eq!(
        vectors
            .batch_upsert(&missing, &[])
            .expect_err("empty batch upsert validates collection")
            .code(),
        "not_found.engine.vector_collection"
    );
    assert_eq!(
        vectors
            .batch_delete(&missing, &[])
            .expect_err("empty batch delete validates collection")
            .code(),
        "not_found.engine.vector_collection"
    );
    assert_eq!(
        vectors
            .query(&missing, &embedding([1.0, 0.0]), 0, None)
            .expect_err("zero-limit query validates collection")
            .code(),
        "not_found.engine.vector_collection"
    );
    assert_eq!(
        vectors
            .query(&docs, &embedding([1.0, 0.0, 0.0]), 0, None)
            .expect_err("zero-limit query validates dimension")
            .code(),
        "invalid_argument.engine.vector_dimension"
    );
}

#[test]
fn vector_exact_metric_ordering_is_deterministic() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    vectors
        .create_collection(
            collection("dots"),
            config(2, VectorDistanceMetric::DotProduct),
        )
        .expect("collection create succeeds");
    vectors
        .batch_upsert(
            &collection("dots"),
            &[
                upsert("b", [1.0, 0.0], json!({"keep": true})),
                upsert("a", [1.0, 0.0], json!({"keep": true})),
                upsert("c", [0.0, 1.0], json!({"keep": false})),
            ],
        )
        .expect("batch upsert succeeds");

    let matches = vectors
        .query(
            &collection("dots"),
            &embedding([1.0, 0.0]),
            3,
            Some(&filter_eq("keep", true)),
        )
        .expect("query succeeds");
    assert_eq!(
        matches
            .matches()
            .iter()
            .map(|row| row.entry().key().as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert!((matches.matches()[0].score() - 1.0).abs() < f32::EPSILON);
    assert!((matches.matches()[1].score() - 1.0).abs() < f32::EPSILON);
}

#[test]
fn vector_collection_delete_tombstones_visible_rows() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    vectors
        .create_collection(
            collection("scratch"),
            config(2, VectorDistanceMetric::Cosine),
        )
        .expect("collection create succeeds");
    vectors
        .batch_upsert(
            &collection("scratch"),
            &[
                upsert("a", [1.0, 0.0], json!({})),
                upsert("b", [0.0, 1.0], json!({})),
            ],
        )
        .expect("batch upsert succeeds");
    assert_eq!(
        vectors
            .count(&collection("scratch"))
            .expect("count succeeds"),
        2
    );
    assert!(vectors
        .delete_collection(&collection("scratch"))
        .expect("collection delete succeeds"));
    assert!(vectors
        .collection_info(&collection("scratch"))
        .expect("info succeeds")
        .is_none());
    assert!(!vectors
        .delete_collection(&collection("scratch"))
        .expect("missing collection delete succeeds"));
    let error = vectors
        .query(&collection("scratch"), &embedding([1.0, 0.0]), 1, None)
        .expect_err("query missing collection rejected");
    assert_eq!(error.class(), EngineErrorClass::NotFound);
    assert_eq!(error.code(), "not_found.engine.vector_collection");
}

#[test]
fn vector_historical_reads_use_historical_collection_config_after_delete() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let collection = collection("historical");
    let key = vector_key("doc");
    vectors
        .create_collection(
            collection.clone(),
            config(2, VectorDistanceMetric::Euclidean),
        )
        .expect("collection create succeeds");
    let written = vectors
        .upsert(
            collection.clone(),
            key.clone(),
            embedding([1.0, 0.0]),
            Some(metadata(json!({"kind": "doc"}))),
        )
        .expect("upsert succeeds");
    let version = written.commit().version();
    let timestamp = written.commit().timestamp();

    assert!(vectors
        .delete_collection(&collection)
        .expect("collection delete succeeds"));
    assert!(vectors
        .collection_info(&collection)
        .expect("latest info succeeds")
        .is_none());

    assert_eq!(
        vectors
            .get_at_version(&collection, &key, version)
            .expect("historical version read succeeds")
            .expect("historical vector exists")
            .entry()
            .embedding()
            .as_slice(),
        &[1.0, 0.0]
    );
    assert_eq!(
        vectors
            .get_at(&collection, &key, timestamp)
            .expect("historical timestamp read succeeds")
            .expect("historical vector exists")
            .entry()
            .embedding()
            .as_slice(),
        &[1.0, 0.0]
    );
    assert_eq!(
        vectors
            .query_at(&collection, &embedding([1.0, 0.0]), 10, None, timestamp)
            .expect("historical query succeeds")
            .matches()
            .iter()
            .map(|row| row.entry().key().as_str())
            .collect::<Vec<_>>(),
        vec!["doc"]
    );
    let history = vectors
        .history(&collection, &key)
        .expect("history succeeds")
        .expect("history exists");
    assert_eq!(history.rows().len(), 2);
    assert!(history.rows()[0].is_tombstone());
    assert_eq!(history.rows()[1].vector_revision(), Some(1));
}

#[test]
fn vector_list_keys_pages_in_public_key_order() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let collection = collection("paged");
    vectors
        .create_collection(collection.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    vectors
        .batch_upsert(
            &collection,
            &[
                upsert("b", [1.0, 0.0], json!({})),
                upsert("aa", [1.0, 0.0], json!({})),
                upsert("ba", [1.0, 0.0], json!({})),
                upsert("a", [1.0, 0.0], json!({})),
                upsert("ab", [1.0, 0.0], json!({})),
            ],
        )
        .expect("batch upsert succeeds");

    let first = vectors
        .list_keys(&collection, None, None, 2)
        .expect("first page succeeds");
    assert_eq!(key_strings(first.keys()), vec!["a", "aa"]);
    assert!(first.has_more());

    let second = vectors
        .list_keys(&collection, None, first.cursor(), 2)
        .expect("second page succeeds");
    assert_eq!(key_strings(second.keys()), vec!["ab", "b"]);
    assert!(second.has_more());

    let third = vectors
        .list_keys(&collection, None, second.cursor(), 2)
        .expect("third page succeeds");
    assert_eq!(key_strings(third.keys()), vec!["ba"]);
    assert!(!third.has_more());
}

#[test]
fn vector_serde_and_filter_builder_reject_invalid_inputs() {
    assert!(serde_json::from_value::<VectorEmbedding>(json!([])).is_err());
    assert!(serde_json::from_value::<VectorConfig>(json!({
        "dimension": 0,
        "metric": "cosine"
    }))
    .is_err());
    assert!(serde_json::from_value::<VectorFilterCondition>(json!({
        "field": "nested/path",
        "op": "eq",
        "value": {
            "type": "bool",
            "value": true
        }
    }))
    .is_err());

    let error = VectorFilter::new()
        .eq("nested/path", true)
        .expect_err("invalid filter field rejected");
    assert_eq!(error.class(), EngineErrorClass::InvalidInput);
    assert_eq!(
        error.code(),
        "invalid_argument.engine.vector_metadata_field"
    );
}

fn exercise_vector_collection_lifecycle(database: &mut Database) {
    let mut vectors = vector_service(database, "default", "default");
    let docs = create_collection_lifecycle_fixture(&mut vectors);
    assert_collection_count_updates(&mut vectors, &docs);
    assert_collection_delete_hides_rows(&mut vectors, &docs);
}

fn create_collection_lifecycle_fixture(
    vectors: &mut strata_engine_next::VectorService<'_>,
) -> VectorCollectionName {
    assert!(vectors
        .list_collections()
        .expect("empty collection list succeeds")
        .is_empty());
    let docs = collection("docs");
    let alpha = collection("alpha");
    let zeta = collection("zeta");
    let info = vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    assert_eq!(info.name(), &docs);
    assert_eq!(info.count(), 0);
    assert_eq!(info.config().dimension(), 2);
    assert!(info.created_version().as_u64() > 0);
    assert!(vectors
        .create_collection(docs.clone(), config(3, VectorDistanceMetric::DotProduct))
        .expect_err("duplicate collection rejected")
        .code()
        .contains("vector_collection"));

    vectors
        .create_collection(alpha.clone(), config(2, VectorDistanceMetric::Euclidean))
        .expect("alpha create succeeds");
    vectors
        .create_collection(zeta.clone(), config(2, VectorDistanceMetric::DotProduct))
        .expect("zeta create succeeds");
    assert_eq!(
        vectors
            .list_collections()
            .expect("collection list succeeds")
            .iter()
            .map(|info| info.name().as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "docs", "zeta"]
    );
    docs
}

fn assert_collection_count_updates(
    vectors: &mut strata_engine_next::VectorService<'_>,
    docs: &VectorCollectionName,
) {
    vectors
        .batch_upsert(
            docs,
            &[
                upsert("a", [1.0, 0.0], json!({})),
                upsert("b", [0.0, 1.0], json!({})),
            ],
        )
        .expect("batch upsert succeeds");
    assert_eq!(vectors.count(docs).expect("count succeeds"), 2);
    vectors
        .upsert(
            docs.clone(),
            vector_key("a"),
            embedding([0.5, 0.5]),
            Some(metadata(json!({}))),
        )
        .expect("overwrite succeeds");
    assert_eq!(vectors.count(docs).expect("count succeeds"), 2);

    assert!(vectors
        .delete(docs, vector_key("a"))
        .expect("delete succeeds")
        .deleted());
    assert!(!vectors
        .delete(docs, vector_key("a"))
        .expect("second delete succeeds")
        .deleted());
    assert_eq!(vectors.count(docs).expect("count succeeds"), 1);
    assert_eq!(
        key_strings(
            vectors
                .list_keys(docs, None, None, 10)
                .expect("key list succeeds")
                .keys()
        ),
        vec!["b"]
    );
}

fn assert_collection_delete_hides_rows(
    vectors: &mut strata_engine_next::VectorService<'_>,
    docs: &VectorCollectionName,
) {
    assert!(vectors
        .delete_collection(docs)
        .expect("collection delete succeeds"));
    assert!(vectors
        .collection_info(docs)
        .expect("info succeeds")
        .is_none());
    assert_eq!(
        vectors
            .count(docs)
            .expect_err("deleted collection count rejected")
            .code(),
        "not_found.engine.vector_collection"
    );
    assert_eq!(
        vectors
            .upsert(docs.clone(), vector_key("c"), embedding([1.0, 0.0]), None)
            .expect_err("upsert into deleted collection rejected")
            .code(),
        "not_found.engine.vector_collection"
    );
    assert_eq!(
        vectors
            .list_collections()
            .expect("collection list succeeds")
            .iter()
            .map(|info| info.name().as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "zeta"]
    );
}

fn exercise_vector_metadata_patch_contract(database: &mut Database) {
    let mut vectors = vector_service(database, "default", "default");
    let docs = collection("metadata");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    vectors
        .upsert(
            docs.clone(),
            vector_key("empty"),
            embedding([1.0, 0.0]),
            None,
        )
        .expect("empty metadata upsert succeeds");
    vectors
        .upsert(
            docs.clone(),
            vector_key("object"),
            embedding([0.0, 1.0]),
            Some(metadata(json!({"kind": "doc", "rank": 1}))),
        )
        .expect("object metadata upsert succeeds");
    vectors
        .upsert(
            docs.clone(),
            vector_key("scalar"),
            embedding([0.5, 0.5]),
            Some(metadata(json!("scalar"))),
        )
        .expect("scalar metadata upsert succeeds");

    let empty_patch = vectors
        .update_metadata(&docs, vector_key("empty"), &patch(json!({"added": true})))
        .expect("metadata patch succeeds");
    assert!(empty_patch.updated());
    assert_eq!(empty_patch.vector_revision(), Some(2));
    assert!(empty_patch.commit().is_some());
    assert_eq!(
        vectors
            .get(&docs, &vector_key("empty"))
            .expect("read succeeds")
            .expect("entry exists")
            .metadata()
            .expect("metadata")
            .as_inner(),
        &json!({"added": true})
    );

    vectors
        .update_metadata(
            &docs,
            vector_key("object"),
            &patch(json!({"rank": 2, "nullable": null})),
        )
        .expect("object patch succeeds");
    let patched = vectors
        .get(&docs, &vector_key("object"))
        .expect("read succeeds")
        .expect("entry exists");
    assert_eq!(patched.embedding().as_slice(), &[0.0, 1.0]);
    assert_eq!(
        patched.metadata().expect("metadata").as_inner(),
        &json!({"kind": "doc", "rank": 2, "nullable": null})
    );

    let error = vectors
        .update_metadata(&docs, vector_key("scalar"), &patch(json!({"x": 1})))
        .expect_err("non-object metadata patch rejected");
    assert_eq!(error.class(), EngineErrorClass::InvalidInput);
    assert_eq!(
        error.code(),
        "invalid_argument.engine.vector_metadata_patch"
    );
    assert_eq!(
        vectors
            .get(&docs, &vector_key("scalar"))
            .expect("read succeeds")
            .expect("entry exists")
            .metadata()
            .expect("metadata")
            .as_inner(),
        &json!("scalar")
    );

    let missing = vectors
        .update_metadata(&docs, vector_key("missing"), &patch(json!({"x": 1})))
        .expect("missing metadata patch succeeds");
    assert!(!missing.updated());
    assert_eq!(missing.vector_revision(), None);
    assert_eq!(missing.commit(), None);
    assert_eq!(
        vectors
            .update_metadata(
                &collection("missing"),
                vector_key("missing"),
                &patch(json!({"x": 1}))
            )
            .expect_err("missing collection rejected")
            .code(),
        "not_found.engine.vector_collection"
    );
}

fn exercise_vector_batch_contracts(database: &mut Database) {
    let mut vectors = vector_service(database, "default", "default");
    let collection = collection("batch");
    vectors
        .create_collection(collection.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");

    let empty_upsert = vectors
        .batch_upsert(&collection, &[])
        .expect("empty batch upsert succeeds");
    assert!(empty_upsert.vector_revisions().is_empty());
    assert_eq!(empty_upsert.commit(), None);
    assert!(vectors
        .batch_get(&collection, &[])
        .expect("empty batch get succeeds")
        .entries()
        .is_empty());
    let empty_delete = vectors
        .batch_delete(&collection, &[])
        .expect("empty batch delete succeeds");
    assert!(empty_delete.deleted().is_empty());
    assert_eq!(empty_delete.commit(), None);

    let upserted = vectors
        .batch_upsert(
            &collection,
            &[
                upsert("a", [1.0, 0.0], json!({"round": 1})),
                upsert("b", [0.0, 1.0], json!({"round": 1})),
                upsert("a", [0.5, 0.5], json!({"round": 2})),
            ],
        )
        .expect("batch upsert succeeds");
    assert_eq!(upserted.vector_revisions(), &[1, 1, 2]);
    assert!(upserted.commit().is_some());
    assert_eq!(vectors.count(&collection).expect("count succeeds"), 2);
    assert_eq!(
        vectors
            .get(&collection, &vector_key("a"))
            .expect("read succeeds")
            .expect("entry exists")
            .embedding()
            .as_slice(),
        &[0.5, 0.5]
    );

    let batch_get = vectors
        .batch_get(
            &collection,
            &[vector_key("a"), vector_key("missing"), vector_key("a")],
        )
        .expect("batch get succeeds");
    assert!(batch_get.entries()[0].is_some());
    assert!(batch_get.entries()[1].is_none());
    assert!(batch_get.entries()[2].is_some());
    assert_eq!(
        batch_get.entries()[0]
            .as_ref()
            .expect("entry")
            .vector_revision(),
        2
    );

    let error = vectors
        .batch_upsert(
            &collection,
            &[
                upsert("c", [1.0, 0.0], json!({})),
                upsert("bad", [1.0, 0.0, 0.0], json!({})),
            ],
        )
        .expect_err("dimension mismatch rejected");
    assert_eq!(error.class(), EngineErrorClass::InvalidInput);
    assert_eq!(error.code(), "invalid_argument.engine.vector_dimension");
    assert!(!vectors
        .exists(&collection, &vector_key("c"))
        .expect("exists succeeds"));

    let deleted = vectors
        .batch_delete(
            &collection,
            &[vector_key("a"), vector_key("a"), vector_key("missing")],
        )
        .expect("batch delete succeeds");
    assert_eq!(deleted.deleted(), &[true, false, false]);
    assert!(deleted.commit().is_some());
    assert!(vectors
        .get(&collection, &vector_key("a"))
        .expect("read succeeds")
        .is_none());
    assert_eq!(
        vectors
            .query(&collection, &embedding([0.5, 0.5]), 10, None)
            .expect("search succeeds")
            .matches()
            .iter()
            .map(|row| row.entry().key().as_str())
            .collect::<Vec<_>>(),
        vec!["b"]
    );
}

fn exercise_vector_bulk_delete_contracts(database: &mut Database) {
    let mut vectors = vector_service(database, "default", "default");
    let docs = collection("docs");
    let other = collection("other");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("docs create succeeds");
    vectors
        .create_collection(other.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("other create succeeds");
    vectors
        .batch_upsert(
            &docs,
            &[
                upsert("a", [1.0, 0.0], json!({"kind": "doc", "active": true})),
                upsert("b", [0.0, 1.0], json!({"kind": "doc", "active": false})),
                upsert("c", [0.5, 0.5], json!({"kind": "note", "active": true})),
                VectorUpsertEntry::new(vector_key("d"), embedding([0.25, 0.75]), None),
            ],
        )
        .expect("docs batch upsert succeeds");
    vectors
        .upsert(
            other.clone(),
            vector_key("o"),
            embedding([1.0, 0.0]),
            Some(metadata(json!({"kind": "doc"}))),
        )
        .expect("other upsert succeeds");

    let deleted = vectors
        .delete_by_filter(&docs, &filter_and([("kind", "doc"), ("active", "true")]))
        .expect("filtered delete succeeds");
    assert_eq!(deleted.deleted_count(), 0);
    assert_eq!(deleted.commit(), None);
    let deleted = vectors
        .delete_by_filter(
            &docs,
            &filter_eq("kind", "doc").eq("active", true).expect("filter"),
        )
        .expect("filtered delete succeeds");
    assert_eq!(deleted.deleted_count(), 1);
    assert!(deleted.commit().is_some());
    assert!(vectors
        .get(&docs, &vector_key("a"))
        .expect("read succeeds")
        .is_none());
    assert_eq!(vectors.count(&docs).expect("count succeeds"), 3);
    assert_eq!(
        key_strings(
            vectors
                .list_keys(&docs, None, None, 10)
                .expect("key list succeeds")
                .keys()
        ),
        vec!["b", "c", "d"]
    );
    assert!(vectors
        .history(&docs, &vector_key("a"))
        .expect("history succeeds")
        .expect("history exists")
        .rows()[0]
        .is_tombstone());
    let rerun = vectors
        .delete_by_filter(
            &docs,
            &filter_eq("kind", "doc").eq("active", true).expect("filter"),
        )
        .expect("filtered delete rerun succeeds");
    assert_eq!(rerun.deleted_count(), 0);
    assert_eq!(rerun.commit(), None);

    let all_deleted = vectors.delete_all(&docs).expect("delete all succeeds");
    assert_eq!(all_deleted.deleted_count(), 3);
    assert!(all_deleted.commit().is_some());
    assert_eq!(vectors.count(&docs).expect("count succeeds"), 0);
    assert!(vectors
        .collection_info(&docs)
        .expect("info succeeds")
        .is_some());
    assert!(vectors
        .list_keys(&docs, None, None, 10)
        .expect("key list succeeds")
        .keys()
        .is_empty());
    assert!(vectors
        .query(&docs, &embedding([1.0, 0.0]), 10, None)
        .expect("search succeeds")
        .matches()
        .is_empty());
    let idempotent = vectors
        .delete_all(&docs)
        .expect("delete all rerun succeeds");
    assert_eq!(idempotent.deleted_count(), 0);
    assert_eq!(idempotent.commit(), None);
    assert_eq!(vectors.count(&other).expect("other count succeeds"), 1);
}

fn exercise_vector_exact_search_metrics(database: &mut Database) {
    let mut vectors = vector_service(database, "default", "default");
    assert_cosine_search_fixture(&mut vectors);
    assert_euclidean_search_fixture(&mut vectors);
    assert_dot_product_search_fixture(&mut vectors);
}

fn assert_cosine_search_fixture(vectors: &mut strata_engine_next::VectorService<'_>) {
    let cosine = collection("cosine");
    vectors
        .create_collection(cosine.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("cosine create succeeds");
    vectors
        .batch_upsert(
            &cosine,
            &[
                upsert("same", [1.0, 0.0], json!({})),
                upsert("orthogonal", [0.0, 1.0], json!({})),
                upsert("zero", [0.0, 0.0], json!({})),
                upsert("opposite", [-1.0, 0.0], json!({})),
            ],
        )
        .expect("cosine upsert succeeds");
    let cosine_scores = score_pairs(
        vectors
            .query(&cosine, &embedding([1.0, 0.0]), 10, None)
            .expect("cosine query succeeds")
            .matches(),
    );
    assert_eq!(
        cosine_scores
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>(),
        vec!["same", "orthogonal", "zero", "opposite"]
    );
    assert_close(cosine_scores[0].1, 1.0);
    assert_close(cosine_scores[1].1, 0.0);
    assert_close(cosine_scores[2].1, 0.0);
    assert_close(cosine_scores[3].1, -1.0);
}

fn assert_euclidean_search_fixture(vectors: &mut strata_engine_next::VectorService<'_>) {
    let euclidean = collection("euclidean");
    vectors
        .create_collection(
            euclidean.clone(),
            config(2, VectorDistanceMetric::Euclidean),
        )
        .expect("euclidean create succeeds");
    vectors
        .batch_upsert(
            &euclidean,
            &[
                upsert("same", [0.0, 0.0], json!({})),
                upsert("near", [0.5, 0.0], json!({})),
                upsert("far", [2.0, 0.0], json!({})),
            ],
        )
        .expect("euclidean upsert succeeds");
    let euclidean_scores = score_pairs(
        vectors
            .query(&euclidean, &embedding([0.0, 0.0]), 10, None)
            .expect("euclidean query succeeds")
            .matches(),
    );
    assert_eq!(
        euclidean_scores
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>(),
        vec!["same", "near", "far"]
    );
    assert_close(euclidean_scores[0].1, 1.0);
    assert_close(euclidean_scores[1].1, 1.0 / 1.5);
    assert_close(euclidean_scores[2].1, 1.0 / 3.0);
}

fn assert_dot_product_search_fixture(vectors: &mut strata_engine_next::VectorService<'_>) {
    let dot = collection("dot");
    vectors
        .create_collection(dot.clone(), config(2, VectorDistanceMetric::DotProduct))
        .expect("dot create succeeds");
    vectors
        .batch_upsert(
            &dot,
            &[
                upsert("high", [3.0, 0.0], json!({})),
                upsert("tie-a", [2.0, 0.0], json!({})),
                upsert("tie-b", [2.0, 0.0], json!({})),
                upsert("negative", [-1.0, 0.0], json!({})),
            ],
        )
        .expect("dot upsert succeeds");
    assert_eq!(
        vectors
            .query(&dot, &embedding([1.0, 0.0]), 10, None)
            .expect("dot query succeeds")
            .matches()
            .iter()
            .map(|row| row.entry().key().as_str())
            .collect::<Vec<_>>(),
        vec!["high", "tie-a", "tie-b", "negative"]
    );
    assert!(vectors
        .query(&dot, &embedding([1.0, 0.0]), 0, None)
        .expect("zero limit query succeeds")
        .matches()
        .is_empty());
    assert_eq!(
        vectors
            .query(&dot, &embedding([1.0, 0.0, 0.0]), 1, None)
            .expect_err("dimension mismatch rejected")
            .code(),
        "invalid_argument.engine.vector_dimension"
    );
}

fn exercise_vector_timestamp_reads(database: &mut Database) {
    let mut vectors = vector_service(database, "default", "default");
    let collection = collection("history");
    let key = vector_key("doc");
    assert_eq!(
        vectors
            .query_at(
                &collection,
                &embedding([1.0, 0.0]),
                10,
                None,
                strata_core_next::Timestamp::EPOCH
            )
            .expect_err("pre-create query rejected")
            .code(),
        "not_found.engine.vector_collection"
    );
    vectors
        .create_collection(collection.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    let first = vectors
        .upsert(
            collection.clone(),
            key.clone(),
            embedding([1.0, 0.0]),
            Some(metadata(json!({"kind": "doc"}))),
        )
        .expect("first upsert succeeds");
    let second = vectors
        .upsert(
            collection.clone(),
            key.clone(),
            embedding([0.0, 1.0]),
            Some(metadata(json!({"kind": "note"}))),
        )
        .expect("second upsert succeeds");
    let deleted = vectors
        .delete(&collection, key.clone())
        .expect("delete succeeds");

    assert_eq!(
        vectors
            .get_at(&collection, &key, first.commit().timestamp())
            .expect("first timestamp read succeeds")
            .expect("first value exists")
            .embedding()
            .as_slice(),
        &[1.0, 0.0]
    );
    assert_eq!(
        vectors
            .query_at(
                &collection,
                &embedding([1.0, 0.0]),
                10,
                Some(&filter_eq("kind", "doc")),
                first.commit().timestamp()
            )
            .expect("first timestamp query succeeds")
            .matches()
            .len(),
        1
    );
    assert_eq!(
        vectors
            .get_at(&collection, &key, second.commit().timestamp())
            .expect("second timestamp read succeeds")
            .expect("second value exists")
            .embedding()
            .as_slice(),
        &[0.0, 1.0]
    );
    assert!(vectors
        .query_at(
            &collection,
            &embedding([1.0, 0.0]),
            10,
            Some(&filter_eq("kind", "doc")),
            second.commit().timestamp()
        )
        .expect("second timestamp query succeeds")
        .matches()
        .is_empty());
    assert!(vectors
        .get_at(
            &collection,
            &key,
            deleted.commit().expect("delete commit").timestamp()
        )
        .expect("delete timestamp read succeeds")
        .is_none());
    let history = vectors
        .history(&collection, &key)
        .expect("history succeeds")
        .expect("history exists");
    assert_eq!(history.rows().len(), 3);
    assert!(history.rows()[0].is_tombstone());
    assert_eq!(history.rows()[1].vector_revision(), Some(2));
    assert_eq!(history.rows()[2].vector_revision(), Some(1));
}

fn exercise_vector_contract(database: &mut Database) {
    let collection = collection("docs");
    let key_a = vector_key("doc/a");
    let key_b = vector_key("doc/b");
    let key_c = vector_key("note/c");
    let mut vectors = vector_service(database, "default", "default");

    let first = create_contract_vectors(&mut vectors, &collection, &key_a, &key_b, &key_c);
    assert_contract_reads(
        &mut vectors,
        &collection,
        &key_a,
        &key_b,
        first.commit().version(),
    );
    assert_contract_patch_and_search(&mut vectors, &collection, &key_a);
    assert_contract_batches_and_deletes(&mut vectors, &collection, &key_a, &key_b);
}

fn create_contract_vectors(
    vectors: &mut strata_engine_next::VectorService<'_>,
    collection: &VectorCollectionName,
    key_a: &VectorKey,
    key_b: &VectorKey,
    key_c: &VectorKey,
) -> strata_engine_next::VectorWriteOutcome {
    let info = vectors
        .create_collection(collection.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    assert_eq!(info.name(), collection);
    assert_eq!(info.count(), 0);

    let duplicate = vectors
        .create_collection(collection.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect_err("duplicate collection rejected");
    assert_eq!(duplicate.class(), EngineErrorClass::Conflict);
    assert_eq!(duplicate.code(), "already_exists.engine.vector_collection");

    let first = vectors
        .upsert(
            collection.clone(),
            key_a.clone(),
            embedding([1.0, 0.0]),
            Some(metadata(json!({"kind": "doc", "rank": 1}))),
        )
        .expect("first upsert succeeds");
    assert_eq!(first.vector_revision(), 1);
    let second = vectors
        .upsert(
            collection.clone(),
            key_a.clone(),
            embedding([0.5, 0.5]),
            Some(metadata(json!({"kind": "doc", "rank": 2}))),
        )
        .expect("second upsert succeeds");
    assert_eq!(second.vector_revision(), 2);

    let batch = vectors
        .batch_upsert(
            collection,
            &[
                VectorUpsertEntry::new(
                    key_b.clone(),
                    embedding([0.0, 1.0]),
                    Some(metadata(json!({"kind": "doc", "rank": 3}))),
                ),
                VectorUpsertEntry::new(
                    key_c.clone(),
                    embedding([0.25, 0.25]),
                    Some(metadata(json!({"kind": "note", "rank": 4}))),
                ),
                VectorUpsertEntry::new(
                    key_b.clone(),
                    embedding([1.0, 0.0]),
                    Some(metadata(json!({"kind": "doc", "rank": 5}))),
                ),
            ],
        )
        .expect("batch upsert succeeds");
    assert_eq!(batch.vector_revisions(), &[1, 1, 2]);
    assert!(batch.commit().is_some());
    first
}

fn assert_contract_reads(
    vectors: &mut strata_engine_next::VectorService<'_>,
    collection: &VectorCollectionName,
    key_a: &VectorKey,
    key_b: &VectorKey,
    first_version: strata_core_next::CommitVersion,
) {
    assert_eq!(vectors.count(collection).expect("count succeeds"), 3);
    assert!(vectors.exists(collection, key_a).expect("exists succeeds"));
    assert_eq!(
        vectors
            .get_versioned(collection, key_a)
            .expect("read succeeds")
            .expect("entry exists")
            .vector_revision(),
        2
    );
    assert!(vectors
        .get_at_version(collection, key_a, first_version)
        .expect("version read succeeds")
        .is_some());

    let page = vectors
        .list_keys(collection, Some(&vector_key("doc/")), None, 1)
        .expect("first key page succeeds");
    assert_eq!(page.keys(), std::slice::from_ref(key_a));
    assert!(page.has_more());
    let next = vectors
        .list_keys(collection, Some(&vector_key("doc/")), page.cursor(), 10)
        .expect("second key page succeeds");
    assert_eq!(next.keys(), std::slice::from_ref(key_b));
    assert!(!next.has_more());
}

fn assert_contract_patch_and_search(
    vectors: &mut strata_engine_next::VectorService<'_>,
    collection: &VectorCollectionName,
    key_a: &VectorKey,
) {
    let mut patch = Map::new();
    patch.insert("patched".to_owned(), json!(true));
    let patched = vectors
        .update_metadata(
            collection,
            key_a.clone(),
            &VectorMetadataPatch::from_map(patch).expect("valid patch"),
        )
        .expect("metadata patch succeeds");
    assert!(patched.updated());
    assert_eq!(patched.vector_revision(), Some(3));
    assert_eq!(
        vectors
            .get(collection, key_a)
            .expect("read succeeds")
            .expect("entry exists")
            .metadata()
            .expect("metadata")
            .as_inner(),
        &json!({"kind": "doc", "rank": 2, "patched": true})
    );

    let search = vectors
        .query(
            collection,
            &embedding([1.0, 0.0]),
            10,
            Some(&filter_eq("kind", "doc")),
        )
        .expect("query succeeds");
    assert_eq!(
        search
            .matches()
            .iter()
            .map(|row| row.entry().key().as_str())
            .collect::<Vec<_>>(),
        vec!["doc/b", "doc/a"]
    );
}

fn assert_contract_batches_and_deletes(
    vectors: &mut strata_engine_next::VectorService<'_>,
    collection: &VectorCollectionName,
    key_a: &VectorKey,
    key_b: &VectorKey,
) {
    let batch_get = vectors
        .batch_get(
            collection,
            &[key_a.clone(), vector_key("missing"), key_b.clone()],
        )
        .expect("batch get succeeds");
    assert!(batch_get.entries()[0].is_some());
    assert!(batch_get.entries()[1].is_none());
    assert!(batch_get.entries()[2].is_some());

    let batch_delete = vectors
        .batch_delete(
            collection,
            &[key_b.clone(), key_b.clone(), vector_key("missing")],
        )
        .expect("batch delete succeeds");
    assert_eq!(batch_delete.deleted(), &[true, false, false]);
    assert_eq!(vectors.count(collection).expect("count succeeds"), 2);

    let deleted = vectors
        .delete_by_filter(collection, &filter_eq("kind", "note"))
        .expect("filtered delete succeeds");
    assert_eq!(deleted.deleted_count(), 1);
    assert_eq!(vectors.count(collection).expect("count succeeds"), 1);

    let history = vectors
        .history(collection, key_b)
        .expect("history succeeds")
        .expect("history exists");
    assert!(history.rows()[0].is_tombstone());
    assert_eq!(history.rows()[1].vector_revision(), Some(2));

    let all_deleted = vectors.delete_all(collection).expect("delete all succeeds");
    assert_eq!(all_deleted.deleted_count(), 1);
    assert_eq!(vectors.count(collection).expect("count succeeds"), 0);
}

fn run_database_modes(exercise: fn(&mut Database)) {
    let mut cache = open_cache_database().expect("cache open succeeds");
    exercise(&mut cache);

    let tempdir = tempfile::tempdir().expect("tempdir");
    let mut durable = open_durable_database(tempdir.path()).expect("durable open succeeds");
    exercise(&mut durable);
    durable.close().expect("durable close succeeds");
}

fn vector_service<'a>(
    database: &'a mut Database,
    branch_name: &str,
    space_name: &str,
) -> strata_engine_next::VectorService<'a> {
    database
        .vector(branch(branch_name), space(space_name))
        .expect("vector service opens")
}

fn collection(name: &str) -> VectorCollectionName {
    VectorCollectionName::new(name).expect("valid collection")
}

fn vector_key(key: &str) -> VectorKey {
    VectorKey::new(key).expect("valid vector key")
}

fn embedding<const N: usize>(values: [f32; N]) -> VectorEmbedding {
    VectorEmbedding::new(values).expect("valid embedding")
}

fn metadata(value: Value) -> VectorMetadata {
    VectorMetadata::new(value).expect("valid metadata")
}

fn patch(value: Value) -> VectorMetadataPatch {
    VectorMetadataPatch::new(value).expect("valid metadata patch")
}

fn filter_eq(field: &str, value: impl Into<VectorScalar>) -> VectorFilter {
    VectorFilter::new()
        .eq(field, value)
        .expect("valid vector filter")
}

fn filter_and<const N: usize>(conditions: [(&str, &str); N]) -> VectorFilter {
    conditions
        .into_iter()
        .fold(VectorFilter::new(), |filter, (field, value)| {
            filter.eq(field, value).expect("valid vector filter")
        })
}

fn key_strings(keys: &[VectorKey]) -> Vec<&str> {
    keys.iter().map(VectorKey::as_str).collect()
}

fn score_pairs(matches: &[strata_engine_next::VectorSearchMatch]) -> Vec<(String, f32)> {
    matches
        .iter()
        .map(|row| (row.entry().key().as_str().to_owned(), row.score()))
        .collect()
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 0.000_001,
        "expected {expected}, got {actual}"
    );
}

fn config(dimension: usize, metric: VectorDistanceMetric) -> VectorConfig {
    VectorConfig::new(dimension, metric).expect("valid config")
}

fn upsert<const N: usize>(key: &str, values: [f32; N], metadata_value: Value) -> VectorUpsertEntry {
    VectorUpsertEntry::new(
        vector_key(key),
        embedding(values),
        Some(metadata(metadata_value)),
    )
}
