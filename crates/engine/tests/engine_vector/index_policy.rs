use super::*;

#[cfg(feature = "testkit")]
#[test]
fn vector_exact_baseline_guards_top_k_filters_ties_and_deletes() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let docs = collection("exact-guard");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::DotProduct))
        .expect("collection create succeeds");
    vectors
        .batch_upsert(
            &docs,
            &[
                upsert("deleted", [10.0, 0.0], json!({"keep": true})),
                upsert("filtered", [4.0, 0.0], json!({"keep": false})),
                upsert("winner", [3.0, 0.0], json!({"keep": true})),
                upsert("tie-b", [2.0, 0.0], json!({"keep": true})),
                upsert("tie-a", [2.0, 0.0], json!({"keep": true})),
                upsert("low", [1.0, 0.0], json!({"keep": true})),
            ],
        )
        .expect("batch upsert succeeds");
    vectors
        .delete(&docs, vector_key("deleted"))
        .expect("delete succeeds");

    let filter = filter_eq("keep", true);
    let public = vectors
        .query(&docs, &embedding([1.0, 0.0]), 3, Some(&filter))
        .expect("public query succeeds");
    let exact = vectors
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 3, Some(&filter))
        .expect("exact query succeeds");

    assert_search_results_match(&public, &exact);
    assert_eq!(match_key_strings(&exact), vec!["winner", "tie-a", "tie-b"]);
}

#[cfg(feature = "testkit")]
#[test]
fn vector_exact_baseline_guards_timestamp_visibility() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let docs = collection("exact-history");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    vectors
        .upsert(
            docs.clone(),
            vector_key("a"),
            embedding([1.0, 0.0]),
            Some(metadata(json!({"keep": true}))),
        )
        .expect("first upsert succeeds");
    let snapshot = vectors
        .upsert(
            docs.clone(),
            vector_key("b"),
            embedding([0.8, 0.2]),
            Some(metadata(json!({"keep": true}))),
        )
        .expect("second upsert succeeds")
        .commit()
        .timestamp();
    vectors
        .upsert(
            docs.clone(),
            vector_key("a"),
            embedding([0.0, 1.0]),
            Some(metadata(json!({"keep": false}))),
        )
        .expect("update succeeds");
    vectors
        .delete(&docs, vector_key("b"))
        .expect("delete succeeds");

    let filter = filter_eq("keep", true);
    let public = vectors
        .query_at(&docs, &embedding([1.0, 0.0]), 10, Some(&filter), snapshot)
        .expect("public timestamp query succeeds");
    let exact = vectors
        .query_at_exact_for_test(&docs, &embedding([1.0, 0.0]), 10, Some(&filter), snapshot)
        .expect("exact timestamp query succeeds");

    assert_search_results_match(&public, &exact);
    assert_eq!(match_key_strings(&exact), vec!["a", "b"]);
}

#[cfg(feature = "testkit")]
#[test]
fn vector_exact_baseline_guards_branch_visibility() {
    let mut database = open_cache_database().expect("cache open succeeds");
    {
        let mut vectors = vector_service(&mut database, "default", "default");
        vectors
            .create_collection(
                collection("branch-guard"),
                config(2, VectorDistanceMetric::Cosine),
            )
            .expect("collection create succeeds");
        vectors
            .upsert(
                collection("branch-guard"),
                vector_key("shared"),
                embedding([1.0, 0.0]),
                Some(metadata(json!({"keep": true, "branch": "default"}))),
            )
            .expect("default upsert succeeds");
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
                collection("branch-guard"),
                vector_key("shared"),
                embedding([0.0, 1.0]),
                Some(metadata(json!({"keep": true, "branch": "feature"}))),
            )
            .expect("feature override succeeds");
        feature
            .upsert(
                collection("branch-guard"),
                vector_key("feature-only"),
                embedding([0.9, 0.1]),
                Some(metadata(json!({"keep": true, "branch": "feature"}))),
            )
            .expect("feature upsert succeeds");
    }

    let filter = filter_eq("keep", true);
    {
        let mut default_vectors = vector_service(&mut database, "default", "default");
        let public = default_vectors
            .query(
                &collection("branch-guard"),
                &embedding([1.0, 0.0]),
                10,
                Some(&filter),
            )
            .expect("default public query succeeds");
        let exact = default_vectors
            .query_exact_for_test(
                &collection("branch-guard"),
                &embedding([1.0, 0.0]),
                10,
                Some(&filter),
            )
            .expect("default exact query succeeds");
        assert_search_results_match(&public, &exact);
        assert_eq!(match_key_strings(&exact), vec!["shared"]);
    }

    let mut feature_vectors = vector_service(&mut database, "feature", "default");
    let public = feature_vectors
        .query(
            &collection("branch-guard"),
            &embedding([1.0, 0.0]),
            10,
            Some(&filter),
        )
        .expect("feature public query succeeds");
    let exact = feature_vectors
        .query_exact_for_test(
            &collection("branch-guard"),
            &embedding([1.0, 0.0]),
            10,
            Some(&filter),
        )
        .expect("feature exact query succeeds");
    assert_search_results_match(&public, &exact);
    assert_eq!(match_key_strings(&exact), vec!["feature-only", "shared"]);
}

#[cfg(feature = "testkit")]
#[test]
fn vector_default_index_policy_reports_exact_fallback_for_latest_query() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let docs = collection("default-index-docs");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    vectors
        .batch_upsert(
            &docs,
            &[
                upsert("a", [1.0, 0.0], json!({"kind": "doc"})),
                upsert("b", [0.8, 0.2], json!({"kind": "doc"})),
                upsert("c", [0.2, 0.8], json!({"kind": "note"})),
            ],
        )
        .expect("batch upsert succeeds");

    let filter = filter_eq("kind", "doc");
    let public = vectors
        .query(&docs, &embedding([1.0, 0.0]), 2, Some(&filter))
        .expect("public query succeeds");
    let (diagnostic_result, diagnostics) = vectors
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 2, Some(&filter))
        .expect("diagnostic query succeeds");

    assert_search_results_match(&diagnostic_result, &public);
    assert_eq!(match_key_strings(&diagnostic_result), vec!["a", "b"]);
    assert_default_exact_policy_diagnostics(&diagnostics, "default-index-docs", usize::MAX);

    let (zero_limit_result, zero_limit_diagnostics) = vectors
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 0, Some(&filter))
        .expect("zero-limit diagnostic query succeeds");

    assert!(zero_limit_result.matches().is_empty());
    assert_default_exact_policy_core(&zero_limit_diagnostics, "default-index-docs", 0);
    assert_eq!(zero_limit_diagnostics.manifest_status(), "not_checked");
    assert_eq!(zero_limit_diagnostics.manifest_ref_count(), 0);
}

#[cfg(feature = "testkit")]
#[test]
fn vector_default_index_policy_reports_exact_fallback_for_timestamp_query() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let docs = collection("default-index-history");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    vectors
        .upsert(
            docs.clone(),
            vector_key("a"),
            embedding([1.0, 0.0]),
            Some(metadata(json!({"keep": true}))),
        )
        .expect("first upsert succeeds");
    let snapshot = vectors
        .upsert(
            docs.clone(),
            vector_key("b"),
            embedding([0.8, 0.2]),
            Some(metadata(json!({"keep": true}))),
        )
        .expect("second upsert succeeds")
        .commit()
        .timestamp();
    vectors
        .upsert(
            docs.clone(),
            vector_key("a"),
            embedding([0.0, 1.0]),
            Some(metadata(json!({"keep": false}))),
        )
        .expect("update succeeds");
    vectors
        .delete(&docs, vector_key("b"))
        .expect("delete succeeds");

    let filter = filter_eq("keep", true);
    let public = vectors
        .query_at(&docs, &embedding([1.0, 0.0]), 10, Some(&filter), snapshot)
        .expect("public timestamp query succeeds");
    let (diagnostic_result, diagnostics) = vectors
        .query_at_with_index_diagnostics_for_test(
            &docs,
            &embedding([1.0, 0.0]),
            10,
            Some(&filter),
            snapshot,
        )
        .expect("diagnostic timestamp query succeeds");

    assert_search_results_match(&diagnostic_result, &public);
    assert_eq!(match_key_strings(&diagnostic_result), vec!["a", "b"]);
    assert_default_exact_policy_diagnostics(&diagnostics, "default-index-history", usize::MAX);
}

#[cfg(feature = "testkit")]
#[test]
fn vector_index_manifest_loaded_empty_manifest_reports_refs_without_changing_results() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let docs = collection("manifest-docs");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    vectors
        .batch_upsert(
            &docs,
            &[
                upsert("a", [1.0, 0.0], json!({})),
                upsert("b", [0.0, 1.0], json!({})),
            ],
        )
        .expect("batch upsert succeeds");
    vectors
        .seed_empty_index_manifest_for_test(&docs)
        .expect("manifest seed succeeds");

    let public = vectors
        .query(&docs, &embedding([1.0, 0.0]), 2, None)
        .expect("public query succeeds");
    let (diagnostic_result, diagnostics) = vectors
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 2, None)
        .expect("diagnostic query succeeds");

    assert_search_results_match(&diagnostic_result, &public);
    assert_default_exact_policy_core(&diagnostics, "manifest-docs", usize::MAX);
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.manifest_generation(), Some(1));
    assert_eq!(diagnostics.manifest_ref_count(), 0);
    assert_eq!(diagnostics.manifest_inherited_ref_count(), 0);
    assert_eq!(diagnostics.manifest_owned_ref_count(), 0);
    assert_eq!(diagnostics.manifest_active_delta_count(), 0);
}

#[cfg(feature = "testkit")]
#[test]
fn vector_index_manifest_loaded_non_empty_manifest_reports_ref_shape() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let docs = collection("manifest-shape");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    vectors
        .batch_upsert(
            &docs,
            &[
                upsert("a", [1.0, 0.0], json!({})),
                upsert("b", [0.0, 1.0], json!({})),
            ],
        )
        .expect("batch upsert succeeds");
    vectors
        .seed_synthetic_index_manifest_for_test(
            &docs,
            &[
                (storage_branch_id(0x01), 2, VectorDistanceMetric::Cosine),
                (storage_branch_id(0x02), 2, VectorDistanceMetric::Cosine),
            ],
            7,
        )
        .expect("manifest seed succeeds");

    let public = vectors
        .query(&docs, &embedding([1.0, 0.0]), 2, None)
        .expect("public query succeeds");
    let (diagnostic_result, diagnostics) = vectors
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 2, None)
        .expect("diagnostic query succeeds");

    assert_search_results_match(&diagnostic_result, &public);
    assert_default_exact_policy_core(&diagnostics, "manifest-shape", usize::MAX);
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.manifest_generation(), Some(1));
    assert_eq!(diagnostics.manifest_ref_count(), 2);
    assert_eq!(diagnostics.manifest_owned_ref_count(), 1);
    assert_eq!(diagnostics.manifest_inherited_ref_count(), 1);
    assert_eq!(diagnostics.manifest_active_delta_count(), 7);
}

#[cfg(feature = "testkit")]
#[test]
fn vector_flat_artifact_builder_reports_load_statuses_without_changing_queries() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let docs = collection("flat-artifact-docs");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    vectors
        .batch_upsert(
            &docs,
            &[
                upsert("a", [1.0, 0.0], json!({"kind": "doc"})),
                upsert("b", [0.9, 0.1], json!({"kind": "doc"})),
                upsert("c", [0.0, 1.0], json!({"kind": "note"})),
            ],
        )
        .expect("batch upsert succeeds");
    vectors
        .seed_flat_index_manifest_from_visible_rows_for_test(&docs, "source-a")
        .expect("flat artifact seed succeeds");

    let byte_len = vectors
        .flat_artifact_byte_len_for_test(&docs, "source-a")
        .expect("artifact length checks")
        .expect("artifact exists");
    let manifest_bytes = vectors
        .index_manifest_byte_len_for_test(&docs)
        .expect("manifest length reads")
        .expect("manifest exists");
    assert!(byte_len > 0);
    assert!(manifest_bytes < 16 * 1024);
    assert_eq!(
        vectors
            .flat_artifact_load_status_for_test(&docs, "source-a", None)
            .expect("artifact status checks"),
        "loaded"
    );
    assert_eq!(
        vectors
            .flat_artifact_load_status_for_test(
                &docs,
                "source-a",
                Some(usize::try_from(byte_len - 1).expect("byte length fits")),
            )
            .expect("artifact status checks"),
        "over_budget"
    );

    let filter = filter_eq("kind", "doc");
    let exact = vectors
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 10, Some(&filter))
        .expect("exact query succeeds");
    let (diagnostic_result, diagnostics) = vectors
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 10, Some(&filter))
        .expect("diagnostic query succeeds");

    assert_search_results_match(&diagnostic_result, &exact);
    assert_eq!(match_key_strings(&diagnostic_result), vec!["a", "b"]);
    assert_default_exact_policy_core(&diagnostics, "flat-artifact-docs", usize::MAX);
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.manifest_ref_count(), 1);

    vectors
        .remove_flat_artifact_for_test(&docs, "source-a")
        .expect("artifact remove succeeds");
    assert_eq!(
        vectors
            .flat_artifact_load_status_for_test(&docs, "source-a", None)
            .expect("artifact status checks"),
        "missing"
    );

    vectors
        .seed_flat_index_manifest_from_visible_rows_for_test(&docs, "source-a")
        .expect("flat artifact seed succeeds");
    vectors
        .corrupt_flat_artifact_for_test(&docs, "source-a")
        .expect("artifact corruption succeeds");
    assert_eq!(
        vectors
            .flat_artifact_load_status_for_test(&docs, "source-a", None)
            .expect("artifact status checks"),
        "corrupt"
    );

    vectors
        .seed_flat_index_manifest_from_visible_rows_for_test(&docs, "source-a")
        .expect("flat artifact seed succeeds");
    vectors
        .make_flat_artifact_stale_for_test(&docs, "source-a")
        .expect("stale artifact write succeeds");
    assert_eq!(
        vectors
            .flat_artifact_load_status_for_test(&docs, "source-a", None)
            .expect("artifact status checks"),
        "stale"
    );
}
