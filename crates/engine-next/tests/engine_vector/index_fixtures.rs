use super::*;

#[test]
fn vector_small_2d_fixture_matches_exact_and_flat() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let fixture = Small2dFixture::seed(&mut vectors);

    fixture.assert_query(
        &mut vectors,
        embedding([1.0, 0.0]),
        4,
        &["a", "c", "b", "d"],
    );
    fixture.assert_query(&mut vectors, embedding([0.0, 1.0]), 2, &["b", "c"]);
    fixture.assert_query(&mut vectors, embedding([0.7, 0.7]), 3, &["c", "a", "b"]);

    let (empty, diagnostics) = assert_flat_policy_matches_exact(
        &mut vectors,
        fixture.collection(),
        &embedding([1.0, 0.0]),
        0,
        None,
    );
    assert!(empty.matches().is_empty());
    assert_eq!(diagnostics.manifest_status(), "not_checked");
    assert_eq!(diagnostics.flat_source_count(), 0);
    assert!(diagnostics.artifact_sources().is_empty());
}

#[test]
fn vector_duplicate_revision_fixture_matches_exact_and_flat() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let fixture = DuplicateRevisionFixture::seed(&mut vectors);

    let (latest, latest_diagnostics) = assert_flat_policy_matches_exact(
        &mut vectors,
        fixture.collection(),
        &embedding([1.0, 0.0]),
        2,
        None,
    );
    assert_eq!(match_key_strings(&latest), vec!["user-3", "user-1"]);
    assert_eq!(latest_diagnostics.flat_source_count(), 1);
    assert_eq!(latest_diagnostics.exact_source_count(), 0);
    assert!(latest_diagnostics.last_query_used_index());

    fixture.assert_timestamp_query(&mut vectors, fixture.after_user_1_insert(), &["user-1"]);
    fixture.assert_timestamp_query(
        &mut vectors,
        fixture.after_user_2_insert(),
        &["user-2", "user-1"],
    );
}

#[test]
fn vector_active_and_sealed_fixture_matches_exact_and_reports_sources() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let fixture = ActiveAndSealedFixture::seed(&mut vectors);

    let (indexed, diagnostics) = assert_flat_policy_matches_exact(
        &mut vectors,
        fixture.collection(),
        &embedding([1.0, 0.0]),
        5,
        Some(&filter_eq("keep", true)),
    );

    assert_eq!(
        match_key_strings(&indexed),
        vec!["fresh-active", "doc-000", "doc-002", "doc-003", "doc-004"]
    );
    assert_eq!(diagnostics.flat_source_count(), 1);
    assert_eq!(diagnostics.active_delta_source_count(), 1);
    assert_eq!(diagnostics.exact_source_count(), 0);
    assert!(diagnostics.last_query_used_index());
    assert_eq!(diagnostics.last_query_fallback_reason(), None);
}

#[test]
fn vector_metadata_fixture_filters_match_exact_and_flat() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let fixture = MetadataFixture::seed(&mut vectors);

    fixture.assert_filter(
        &mut vectors,
        filter_eq("tenant", "a"),
        2,
        &["doc-1", "doc-2"],
    );
    fixture.assert_filter(
        &mut vectors,
        filter_eq("kind", "note"),
        2,
        &["doc-1", "doc-3"],
    );
    fixture.assert_filter(&mut vectors, filter_eq("rank", 2_i32), 1, &["doc-2"]);
    fixture.assert_filter(&mut vectors, filter_eq("rank", 3_i32), 1, &["doc-3"]);
    fixture.assert_filter(&mut vectors, filter_eq("tenant", "missing"), 4, &[]);

    let tenant_filter = filter_eq("tenant", "a");
    let (underfilled, diagnostics) = assert_flat_policy_matches_exact(
        &mut vectors,
        fixture.collection(),
        &embedding([1.0, 0.0]),
        3,
        Some(&tenant_filter),
    );
    assert_eq!(match_key_strings(&underfilled), vec!["doc-1", "doc-2"]);
    assert_eq!(diagnostics.flat_source_count(), 1);
    assert_eq!(diagnostics.exact_source_count(), 1);
    assert_eq!(diagnostics.exact_fallback_count(), 1);
    assert_eq!(
        diagnostics.last_query_fallback_reason(),
        Some("indexed_underfill")
    );
}

struct Small2dFixture {
    collection: VectorCollectionName,
}

impl Small2dFixture {
    fn seed(vectors: &mut strata_engine_next::VectorService<'_>) -> Self {
        let collection = collection("fixture-small-2d");
        vectors
            .create_collection(collection.clone(), config(2, VectorDistanceMetric::Cosine))
            .expect("collection create succeeds");
        vectors
            .batch_upsert(
                &collection,
                &[
                    upsert("a", [1.0, 0.0], json!({})),
                    upsert("b", [0.0, 1.0], json!({})),
                    upsert("c", [0.7, 0.7], json!({})),
                    upsert("d", [-1.0, 0.0], json!({})),
                ],
            )
            .expect("batch upsert succeeds");
        vectors
            .seed_flat_index_manifest_from_visible_rows_for_test(&collection, "source-a")
            .expect("flat artifact seed succeeds");
        Self { collection }
    }

    const fn collection(&self) -> &VectorCollectionName {
        &self.collection
    }

    fn assert_query(
        &self,
        vectors: &mut strata_engine_next::VectorService<'_>,
        query: VectorEmbedding,
        k: usize,
        expected_keys: &[&str],
    ) {
        let (indexed, diagnostics) =
            assert_flat_policy_matches_exact(vectors, &self.collection, &query, k, None);
        assert_eq!(match_key_strings(&indexed), expected_keys);
        assert_eq!(diagnostics.flat_source_count(), 1);
        assert!(diagnostics.last_query_used_index());
    }
}

struct DuplicateRevisionFixture {
    collection: VectorCollectionName,
    after_user_1_insert: strata_core_next::Timestamp,
    after_user_2_insert: strata_core_next::Timestamp,
}

impl DuplicateRevisionFixture {
    fn seed(vectors: &mut strata_engine_next::VectorService<'_>) -> Self {
        let collection = collection("fixture-duplicate-revision");
        vectors
            .create_collection(collection.clone(), config(2, VectorDistanceMetric::Cosine))
            .expect("collection create succeeds");
        let after_user_1_insert = vectors
            .upsert(
                collection.clone(),
                vector_key("user-1"),
                embedding([1.0, 0.0]),
                None,
            )
            .expect("user-1 insert succeeds")
            .commit()
            .timestamp();
        vectors
            .upsert(
                collection.clone(),
                vector_key("user-1"),
                embedding([0.0, 1.0]),
                None,
            )
            .expect("user-1 update succeeds");
        let after_user_2_insert = vectors
            .upsert(
                collection.clone(),
                vector_key("user-2"),
                embedding([1.0, 0.0]),
                None,
            )
            .expect("user-2 insert succeeds")
            .commit()
            .timestamp();
        vectors
            .delete(&collection, vector_key("user-2"))
            .expect("user-2 delete succeeds");
        vectors
            .upsert(
                collection.clone(),
                vector_key("user-3"),
                embedding([0.8, 0.2]),
                None,
            )
            .expect("user-3 insert succeeds");
        vectors
            .seed_flat_index_manifest_from_visible_rows_for_test(&collection, "source-a")
            .expect("flat artifact seed succeeds");

        Self {
            collection,
            after_user_1_insert,
            after_user_2_insert,
        }
    }

    const fn collection(&self) -> &VectorCollectionName {
        &self.collection
    }

    const fn after_user_1_insert(&self) -> strata_core_next::Timestamp {
        self.after_user_1_insert
    }

    const fn after_user_2_insert(&self) -> strata_core_next::Timestamp {
        self.after_user_2_insert
    }

    fn assert_timestamp_query(
        &self,
        vectors: &mut strata_engine_next::VectorService<'_>,
        timestamp: strata_core_next::Timestamp,
        expected_keys: &[&str],
    ) {
        let exact = vectors
            .query_at_exact_for_test(&self.collection, &embedding([1.0, 0.0]), 5, None, timestamp)
            .expect("exact timestamp query succeeds");
        let (indexed, _diagnostics) = vectors
            .query_at_with_index_policy_for_test(
                &self.collection,
                &embedding([1.0, 0.0]),
                5,
                None,
                timestamp,
                VectorIndexPolicy::flat_only_for_test(),
            )
            .expect("indexed timestamp query succeeds");
        assert_search_results_match(&indexed, &exact);
        assert_eq!(match_key_strings(&indexed), expected_keys);
    }
}

struct ActiveAndSealedFixture {
    collection: VectorCollectionName,
}

impl ActiveAndSealedFixture {
    fn seed(vectors: &mut strata_engine_next::VectorService<'_>) -> Self {
        let collection = collection("fixture-active-and-sealed");
        vectors
            .create_collection(
                collection.clone(),
                config(2, VectorDistanceMetric::DotProduct),
            )
            .expect("collection create succeeds");
        let sealed_entries = (0_u16..72)
            .map(|index| descending_fixture_upsert(index, json!({"keep": true})))
            .collect::<Vec<_>>();
        vectors
            .batch_upsert(&collection, &sealed_entries)
            .expect("sealed batch upsert succeeds");
        vectors
            .seed_flat_index_manifest_from_visible_rows_for_test(&collection, "source-a")
            .expect("flat artifact seed succeeds");
        vectors
            .upsert(
                collection.clone(),
                vector_key("doc-000"),
                embedding([300.0, 0.0]),
                Some(metadata(json!({"keep": true}))),
            )
            .expect("sealed row update succeeds");
        vectors
            .delete(&collection, vector_key("doc-001"))
            .expect("sealed row delete succeeds");
        vectors
            .upsert(
                collection.clone(),
                vector_key("fresh-active"),
                embedding([400.0, 0.0]),
                Some(metadata(json!({"keep": true}))),
            )
            .expect("active row insert succeeds");

        Self { collection }
    }

    const fn collection(&self) -> &VectorCollectionName {
        &self.collection
    }
}

struct MetadataFixture {
    collection: VectorCollectionName,
}

impl MetadataFixture {
    fn seed(vectors: &mut strata_engine_next::VectorService<'_>) -> Self {
        let collection = collection("fixture-metadata");
        vectors
            .create_collection(collection.clone(), config(2, VectorDistanceMetric::Cosine))
            .expect("collection create succeeds");
        vectors
            .batch_upsert(
                &collection,
                &[
                    upsert(
                        "doc-1",
                        [1.0, 0.0],
                        json!({"tenant": "a", "kind": "note", "rank": 1}),
                    ),
                    upsert(
                        "doc-2",
                        [0.0, 1.0],
                        json!({"tenant": "a", "kind": "task", "rank": 2}),
                    ),
                    upsert(
                        "doc-3",
                        [1.0, 1.0],
                        json!({"tenant": "b", "kind": "note", "rank": 3}),
                    ),
                    VectorUpsertEntry::new(vector_key("doc-4"), embedding([1.0, 0.0]), None),
                ],
            )
            .expect("batch upsert succeeds");
        vectors
            .seed_flat_index_manifest_from_visible_rows_for_test(&collection, "source-a")
            .expect("flat artifact seed succeeds");
        Self { collection }
    }

    const fn collection(&self) -> &VectorCollectionName {
        &self.collection
    }

    fn assert_filter(
        &self,
        vectors: &mut strata_engine_next::VectorService<'_>,
        filter: VectorFilter,
        k: usize,
        expected_keys: &[&str],
    ) {
        let (indexed, _diagnostics) = assert_flat_policy_matches_exact(
            vectors,
            &self.collection,
            &embedding([1.0, 0.0]),
            k,
            Some(&filter),
        );
        assert_eq!(match_key_strings(&indexed), expected_keys);
    }
}
