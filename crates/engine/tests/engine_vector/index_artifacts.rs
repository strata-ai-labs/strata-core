use super::*;

#[cfg(feature = "testkit")]
#[test]
fn vector_flat_artifact_query_uses_loaded_artifact_above_threshold() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let docs = collection("flat-artifact-query");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    let entries = (0_u16..80)
        .map(|index| {
            descending_fixture_upsert(
                index,
                json!({"kind": if index % 2 == 0 { "doc" } else { "note" }}),
            )
        })
        .collect::<Vec<_>>();
    vectors
        .batch_upsert(&docs, &entries)
        .expect("batch upsert succeeds");
    vectors
        .seed_flat_index_manifest_from_visible_rows_for_test(&docs, "source-a")
        .expect("flat artifact seed succeeds");

    let filter = filter_eq("kind", "doc");
    let public = vectors
        .query(&docs, &embedding([1.0, 0.0]), 5, Some(&filter))
        .expect("public query succeeds");
    let exact = vectors
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 5, Some(&filter))
        .expect("exact query succeeds");
    let (_, diagnostics) = vectors
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 5, Some(&filter))
        .expect("diagnostic query succeeds");

    assert_search_results_match(&public, &exact);
    assert_eq!(
        match_key_strings(&public),
        vec!["doc-000", "doc-002", "doc-004", "doc-006", "doc-008"]
    );
    assert_eq!(diagnostics.policy_mode(), "auto");
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.flat_source_count(), 1);
    assert_eq!(diagnostics.active_delta_source_count(), 1);
    assert_eq!(diagnostics.exact_source_count(), 0);
    assert_eq!(diagnostics.indexed_source_count(), 1);
    assert!(diagnostics.last_query_used_index());
    assert_eq!(diagnostics.last_query_fallback_reason(), None);
    assert_eq!(diagnostics.artifact_sources().len(), 1);
    assert_eq!(diagnostics.artifact_sources()[0].status(), "loaded");
    assert!(diagnostics.artifact_sources()[0].searched());
}

#[cfg(feature = "testkit")]
#[test]
fn vector_flat_artifact_query_falls_back_when_artifact_is_missing_or_corrupt() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let docs = collection("flat-artifact-fallback");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    let entries = (0_u16..72)
        .map(|index| descending_fixture_upsert(index, json!({"kind": "doc"})))
        .collect::<Vec<_>>();
    vectors
        .batch_upsert(&docs, &entries)
        .expect("batch upsert succeeds");
    vectors
        .seed_flat_index_manifest_from_visible_rows_for_test(&docs, "source-a")
        .expect("flat artifact seed succeeds");
    let snapshot = vectors
        .upsert(
            docs.clone(),
            vector_key("fallback-marker"),
            embedding([0.0, 1.0]),
            None,
        )
        .expect("marker upsert succeeds")
        .commit()
        .timestamp();

    let exact = vectors
        .query_at_exact_for_test(&docs, &embedding([1.0, 0.0]), 6, None, snapshot)
        .expect("exact query succeeds");
    vectors
        .remove_flat_artifact_for_test(&docs, "source-a")
        .expect("artifact remove succeeds");
    let public_missing = vectors
        .query_at(&docs, &embedding([1.0, 0.0]), 6, None, snapshot)
        .expect("public query succeeds");
    let (_, missing_diagnostics) = vectors
        .query_at_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 6, None, snapshot)
        .expect("missing diagnostic query succeeds");

    assert_search_results_match(&public_missing, &exact);
    assert_eq!(missing_diagnostics.flat_source_count(), 0);
    assert_eq!(missing_diagnostics.exact_source_count(), 1);
    assert_eq!(
        missing_diagnostics.last_query_fallback_reason(),
        Some("artifact_unavailable")
    );
    assert_eq!(
        missing_diagnostics.artifact_sources()[0].status(),
        "missing"
    );
    assert!(!missing_diagnostics.artifact_sources()[0].searched());

    vectors
        .seed_flat_index_manifest_from_visible_rows_for_test(&docs, "source-a")
        .expect("flat artifact seed succeeds");
    vectors
        .corrupt_flat_artifact_for_test(&docs, "source-a")
        .expect("artifact corruption succeeds");
    let public_corrupt = vectors
        .query_at(&docs, &embedding([1.0, 0.0]), 6, None, snapshot)
        .expect("public query succeeds");
    let (_, corrupt_diagnostics) = vectors
        .query_at_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 6, None, snapshot)
        .expect("corrupt diagnostic query succeeds");

    assert_search_results_match(&public_corrupt, &exact);
    assert_eq!(corrupt_diagnostics.flat_source_count(), 0);
    assert_eq!(
        corrupt_diagnostics.last_query_fallback_reason(),
        Some("artifact_unavailable")
    );
    assert_eq!(
        corrupt_diagnostics.artifact_sources()[0].status(),
        "corrupt"
    );
    assert!(!corrupt_diagnostics.artifact_sources()[0].searched());
}

#[cfg(feature = "testkit")]
#[test]
fn vector_index_manifest_hnsw_only_below_threshold_falls_back_to_exact_results() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let docs = collection("hnsw-below-threshold");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    let entries = (0_u16..72)
        .map(|index| descending_fixture_upsert(index, json!({"kind": "doc"})))
        .collect::<Vec<_>>();
    vectors
        .batch_upsert(&docs, &entries)
        .expect("batch upsert succeeds");
    vectors
        .seed_synthetic_hnsw_index_manifest_for_test(&docs)
        .expect("HNSW manifest seed succeeds");
    let snapshot = vectors
        .upsert(
            docs.clone(),
            vector_key("hnsw-marker"),
            embedding([0.0, 1.0]),
            None,
        )
        .expect("marker upsert succeeds")
        .commit()
        .timestamp();

    let exact = vectors
        .query_at_exact_for_test(&docs, &embedding([1.0, 0.0]), 6, None, snapshot)
        .expect("exact query succeeds");
    let public = vectors
        .query_at(&docs, &embedding([1.0, 0.0]), 6, None, snapshot)
        .expect("public query succeeds");
    let (_, diagnostics) = vectors
        .query_at_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 6, None, snapshot)
        .expect("diagnostic query succeeds");

    assert_search_results_match(&public, &exact);
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.manifest_ref_count(), 1);
    assert_eq!(diagnostics.flat_source_count(), 0);
    assert_eq!(diagnostics.exact_source_count(), 1);
    assert_eq!(
        diagnostics.last_query_fallback_reason(),
        Some("manifest_no_flat_artifacts")
    );
    assert_eq!(diagnostics.artifact_sources().len(), 1);
    assert_eq!(diagnostics.artifact_sources()[0].status(), "not_selected");
    assert!(!diagnostics.artifact_sources()[0].searched());
}

#[cfg(feature = "testkit")]
#[test]
fn vector_query_does_not_seal_active_delta_after_threshold() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let docs = collection("active-delta-seal");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    let entries = (0_u16..72)
        .map(|index| descending_fixture_upsert(index, json!({"kind": "doc"})))
        .collect::<Vec<_>>();
    vectors
        .batch_upsert(&docs, &entries)
        .expect("batch upsert succeeds");

    let exact = vectors
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("exact query succeeds");
    let (diagnostic_result, diagnostics) = vectors
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("diagnostic query succeeds");
    let public = vectors
        .query(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("public query succeeds");

    assert_search_results_match(&diagnostic_result, &exact);
    assert_search_results_match(&public, &exact);
    assert_eq!(diagnostics.manifest_status(), "missing");
    assert_eq!(diagnostics.manifest_ref_count(), 0);
    assert_eq!(diagnostics.flat_source_count(), 0);
    assert_eq!(diagnostics.active_delta_source_count(), 0);
    assert_eq!(diagnostics.exact_source_count(), 1);
    assert_eq!(diagnostics.indexed_vector_count(), 0);
    assert!(!diagnostics.last_query_used_index());
    assert_eq!(diagnostics.last_query_fallback_reason(), Some("missing"));
    assert!(diagnostics.artifact_sources().is_empty());
}
