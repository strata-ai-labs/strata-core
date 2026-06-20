use super::*;

#[cfg(feature = "testkit")]
#[test]
fn vector_hnsw_artifact_query_matches_exact_above_threshold() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let docs = collection("hnsw-artifact-query");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    let entries = (0_u16..320)
        .map(|index| descending_fixture_upsert(index, json!({"kind": "doc"})))
        .collect::<Vec<_>>();
    vectors
        .batch_upsert(&docs, &entries)
        .expect("batch upsert succeeds");
    vectors
        .seed_flat_hnsw_index_manifest_from_visible_rows_for_test(&docs, "source-a")
        .expect("artifact seed succeeds");
    assert_eq!(
        vectors
            .hnsw_artifact_load_status_for_test(&docs, "source-a", None)
            .expect("HNSW artifact status checks"),
        "loaded"
    );

    let exact = vectors
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 8, None)
        .expect("exact query succeeds");
    let (indexed, diagnostics) = vectors
        .query_with_index_policy_for_test(
            &docs,
            &embedding([1.0, 0.0]),
            8,
            None,
            VectorIndexPolicy::hnsw_only_for_test(),
        )
        .expect("diagnostic query succeeds");

    assert_search_results_match(&indexed, &exact);
    assert_eq!(
        match_key_strings(&indexed),
        vec![
            "doc-000", "doc-001", "doc-002", "doc-003", "doc-004", "doc-005", "doc-006", "doc-007"
        ]
    );
    assert_eq!(diagnostics.policy_mode(), "hnsw_only");
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.manifest_ref_count(), 2);
    assert_eq!(diagnostics.source_hnsw_threshold(), 0);
    assert_eq!(diagnostics.hnsw_source_count(), 1);
    assert_eq!(diagnostics.flat_source_count(), 0);
    assert_eq!(diagnostics.active_delta_source_count(), 1);
    assert_eq!(diagnostics.exact_source_count(), 0);
    assert_eq!(diagnostics.indexed_source_count(), 1);
    assert_eq!(diagnostics.indexed_vector_count(), 320);
    assert!(diagnostics.last_query_used_index());
    assert_eq!(diagnostics.last_query_fallback_reason(), None);
    assert_eq!(diagnostics.artifact_sources().len(), 2);
    assert!(diagnostics.artifact_sources().iter().any(|source| {
        source.artifact_id().starts_with("hnsw:")
            && source.status() == "loaded"
            && source.searched()
    }));
    assert!(diagnostics.artifact_sources().iter().any(|source| {
        source.artifact_id().starts_with("flat:")
            && source.status() == "loaded"
            && !source.searched()
    }));
}

#[cfg(feature = "testkit")]
#[test]
fn vector_hnsw_recall_gates_pass_for_random_and_clustered_datasets() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let fixtures = [
        HnswRecallFixture::random("hnsw-recall-random", 320, 16, 0x11),
        HnswRecallFixture::clustered("hnsw-recall-clustered", 320, 16, 0x23),
    ];

    for fixture in fixtures {
        let mut vectors = vector_service(&mut database, "default", "default");
        fixture.seed_collection(&mut vectors);
        vectors
            .seed_flat_hnsw_index_manifest_from_visible_rows_for_test(
                fixture.collection(),
                "source-a",
            )
            .expect("HNSW artifact seed succeeds");

        let stats = assert_hnsw_recall_gates(
            &mut vectors,
            "default",
            fixture.collection(),
            &fixture.queries(),
        );
        assert_eq!(
            stats.hnsw_queries,
            stats.total_queries,
            "all recall queries should use the HNSW source for {}",
            fixture.collection().as_str()
        );
    }
}

#[cfg(feature = "testkit")]
#[test]
fn vector_hnsw_recall_gates_cover_update_delete_and_branch_fork_fixtures() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let fixture = HnswRecallFixture::random("hnsw-recall-mutations", 320, 16, 0x41);
    {
        let mut vectors = vector_service(&mut database, "default", "default");
        fixture.seed_collection(&mut vectors);
        vectors
            .seed_flat_hnsw_index_manifest_from_visible_rows_for_test(
                fixture.collection(),
                "source-a",
            )
            .expect("HNSW artifact seed succeeds");
        for index in 0..24 {
            vectors
                .upsert(
                    fixture.collection().clone(),
                    vector_key(&format!("updated-{index:03}")),
                    hnsw_recall_embedding(index + 9_000, fixture.dimension(), 0x59),
                    Some(metadata(json!({"fixture": "updated", "bucket": index % 8}))),
                )
                .expect("update upsert succeeds");
        }
        for index in 0..24 {
            vectors
                .delete(fixture.collection(), vector_key(&format!("vec-{index:04}")))
                .expect("delete succeeds");
        }

        let mut queries = fixture.queries();
        queries.extend(
            (0..12).map(|index| hnsw_recall_embedding(index + 9_000, fixture.dimension(), 0x59)),
        );
        let stats =
            assert_hnsw_recall_gates(&mut vectors, "default", fixture.collection(), &queries);
        assert_eq!(
            stats.hnsw_queries, stats.total_queries,
            "mutation fixture should still use HNSW plus active delta"
        );
    }

    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("branch fork succeeds");

    let mut feature = vector_service(&mut database, "feature", "default");
    for index in 0..32 {
        feature
            .upsert(
                fixture.collection().clone(),
                vector_key(&format!("feature-{index:03}")),
                hnsw_recall_embedding(index + 12_000, fixture.dimension(), 0x67),
                Some(metadata(json!({"fixture": "feature", "bucket": index % 8}))),
            )
            .expect("feature upsert succeeds");
    }
    for index in 24..32 {
        feature
            .delete(fixture.collection(), vector_key(&format!("vec-{index:04}")))
            .expect("feature delete succeeds");
    }
    let mut queries = fixture.queries();
    queries.extend(
        (0..12).map(|index| hnsw_recall_embedding(index + 12_000, fixture.dimension(), 0x67)),
    );
    let stats = assert_hnsw_recall_gates(&mut feature, "feature", fixture.collection(), &queries);
    assert_eq!(
        stats.hnsw_queries, stats.total_queries,
        "fork fixture should keep inherited HNSW refs searchable"
    );
}

#[cfg(feature = "testkit")]
#[test]
fn vector_hnsw_memory_budget_falls_back_to_flat_artifact() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let docs = collection("hnsw-budget-fallback");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    let entries = (0_u16..320)
        .map(|index| descending_fixture_upsert(index, json!({"kind": "doc"})))
        .collect::<Vec<_>>();
    vectors
        .batch_upsert(&docs, &entries)
        .expect("batch upsert succeeds");
    vectors
        .seed_flat_hnsw_index_manifest_from_visible_rows_for_test(&docs, "source-a")
        .expect("artifact seed succeeds");

    let exact = vectors
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 8, None)
        .expect("exact query succeeds");
    let (indexed, diagnostics) = vectors
        .query_with_index_policy_for_test(
            &docs,
            &embedding([1.0, 0.0]),
            8,
            None,
            VectorIndexPolicy::hnsw_only_for_test().with_hnsw_memory_budget_for_test(1),
        )
        .expect("diagnostic query succeeds");

    assert_search_results_match(&indexed, &exact);
    assert_eq!(diagnostics.policy_mode(), "hnsw_only");
    assert_eq!(diagnostics.hnsw_memory_budget_bytes(), 1);
    assert_eq!(diagnostics.hnsw_source_count(), 0);
    assert_eq!(diagnostics.flat_source_count(), 1);
    assert_eq!(diagnostics.active_delta_source_count(), 1);
    assert_eq!(diagnostics.exact_source_count(), 0);
    assert_eq!(
        diagnostics.last_query_fallback_reason(),
        Some("hnsw_memory_budget_exceeded")
    );
    assert!(diagnostics.artifact_sources().iter().any(|source| {
        source.artifact_id().starts_with("flat:")
            && source.status() == "loaded"
            && source.searched()
    }));
    assert!(diagnostics.artifact_sources().iter().any(|source| {
        source.artifact_id().starts_with("hnsw:")
            && source.status() == "loaded"
            && !source.searched()
    }));
}

#[cfg(feature = "testkit")]
#[test]
fn vector_hnsw_artifact_loss_or_corruption_falls_back_to_flat_artifact() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let docs = collection("hnsw-loss-fallback");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    let entries = (0_u16..320)
        .map(|index| descending_fixture_upsert(index, json!({"kind": "doc"})))
        .collect::<Vec<_>>();
    vectors
        .batch_upsert(&docs, &entries)
        .expect("batch upsert succeeds");
    vectors
        .seed_flat_hnsw_index_manifest_from_visible_rows_for_test(&docs, "source-a")
        .expect("artifact seed succeeds");

    let exact = vectors
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 8, None)
        .expect("exact query succeeds");
    assert_eq!(
        vectors
            .remove_manifest_hnsw_artifacts_for_test(&docs)
            .expect("HNSW artifacts remove"),
        1
    );
    let (missing_indexed, missing_diagnostics) = vectors
        .query_with_index_policy_for_test(
            &docs,
            &embedding([1.0, 0.0]),
            8,
            None,
            VectorIndexPolicy::hnsw_only_for_test(),
        )
        .expect("diagnostic query succeeds");

    assert_search_results_match(&missing_indexed, &exact);
    assert_eq!(missing_diagnostics.hnsw_source_count(), 0);
    assert_eq!(missing_diagnostics.flat_source_count(), 1);
    assert_eq!(
        missing_diagnostics.last_query_fallback_reason(),
        Some("artifact_unavailable")
    );
    assert!(missing_diagnostics.artifact_sources().iter().any(|source| {
        source.artifact_id().starts_with("hnsw:")
            && source.status() == "missing"
            && !source.searched()
    }));

    vectors
        .seed_flat_hnsw_index_manifest_from_visible_rows_for_test(&docs, "source-a")
        .expect("artifact seed succeeds");
    assert_eq!(
        vectors
            .corrupt_manifest_hnsw_artifacts_for_test(&docs)
            .expect("HNSW artifacts corrupt"),
        1
    );
    let (corrupt_indexed, corrupt_diagnostics) = vectors
        .query_with_index_policy_for_test(
            &docs,
            &embedding([1.0, 0.0]),
            8,
            None,
            VectorIndexPolicy::hnsw_only_for_test(),
        )
        .expect("diagnostic query succeeds");

    assert_search_results_match(&corrupt_indexed, &exact);
    assert_eq!(corrupt_diagnostics.hnsw_source_count(), 0);
    assert_eq!(corrupt_diagnostics.flat_source_count(), 1);
    assert_eq!(
        corrupt_diagnostics.last_query_fallback_reason(),
        Some("artifact_unavailable")
    );
    assert!(corrupt_diagnostics.artifact_sources().iter().any(|source| {
        source.artifact_id().starts_with("hnsw:")
            && source.status() == "corrupt"
            && !source.searched()
    }));
}
