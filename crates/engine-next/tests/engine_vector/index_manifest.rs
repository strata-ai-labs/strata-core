use super::*;

#[test]
fn vector_flat_artifact_query_matches_exact_for_timestamp_update_delete_and_branch() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let docs = collection("flat-artifact-branch");
    assert_flat_artifact_main_timestamp_update_delete(&mut database, &docs);
    assert_flat_artifact_feature_branch_override(&mut database, &docs);
}

#[cfg(feature = "testkit")]
fn assert_flat_artifact_main_timestamp_update_delete(
    database: &mut Database,
    docs: &VectorCollectionName,
) {
    let mut main = vector_service(database, "default", "default");
    main.create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    let entries = (0_u16..70)
        .map(|index| descending_fixture_upsert(index, json!({"keep": index % 2 == 0})))
        .collect::<Vec<_>>();
    main.batch_upsert(docs, &entries)
        .expect("batch upsert succeeds");
    main.seed_flat_index_manifest_from_visible_rows_for_test(docs, "source-a")
        .expect("flat artifact seed succeeds");
    let snapshot = main
        .upsert(
            docs.clone(),
            vector_key("fresh-main"),
            embedding([200.0, 0.0]),
            Some(metadata(json!({"keep": true}))),
        )
        .expect("fresh upsert succeeds")
        .commit()
        .timestamp();
    main.upsert(
        docs.clone(),
        vector_key("doc-000"),
        embedding([0.0, 200.0]),
        Some(metadata(json!({"keep": true}))),
    )
    .expect("update succeeds");
    main.delete(docs, vector_key("doc-002"))
        .expect("delete succeeds");

    let filter = filter_eq("keep", true);
    let public_latest = main
        .query(docs, &embedding([1.0, 0.0]), 5, Some(&filter))
        .expect("public latest query succeeds");
    let exact_latest = main
        .query_exact_for_test(docs, &embedding([1.0, 0.0]), 5, Some(&filter))
        .expect("exact latest query succeeds");
    let public_at = main
        .query_at(docs, &embedding([1.0, 0.0]), 5, Some(&filter), snapshot)
        .expect("public timestamp query succeeds");
    let exact_at = main
        .query_at_exact_for_test(docs, &embedding([1.0, 0.0]), 5, Some(&filter), snapshot)
        .expect("exact timestamp query succeeds");

    assert_search_results_match(&public_latest, &exact_latest);
    assert_search_results_match(&public_at, &exact_at);
    assert_eq!(
        match_key_strings(&public_latest),
        vec!["fresh-main", "doc-004", "doc-006", "doc-008", "doc-010"]
    );
    assert_eq!(
        match_key_strings(&public_at),
        vec!["doc-000", "fresh-main", "doc-002", "doc-004", "doc-006"]
    );
}

#[cfg(feature = "testkit")]
fn assert_flat_artifact_feature_branch_override(
    database: &mut Database,
    docs: &VectorCollectionName,
) {
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("branch fork succeeds");

    let mut feature = vector_service(database, "feature", "default");
    feature
        .seed_flat_index_manifest_from_visible_rows_for_test(docs, "source-feature")
        .expect("feature artifact seed succeeds");
    feature
        .upsert(
            docs.clone(),
            vector_key("feature-only"),
            embedding([300.0, 0.0]),
            Some(metadata(json!({"keep": true}))),
        )
        .expect("feature upsert succeeds");
    feature
        .delete(docs, vector_key("doc-004"))
        .expect("feature delete succeeds");

    let filter = filter_eq("keep", true);
    let public = feature
        .query(docs, &embedding([1.0, 0.0]), 5, Some(&filter))
        .expect("feature public query succeeds");
    let exact = feature
        .query_exact_for_test(docs, &embedding([1.0, 0.0]), 5, Some(&filter))
        .expect("feature exact query succeeds");
    let (_, diagnostics) = feature
        .query_with_index_diagnostics_for_test(docs, &embedding([1.0, 0.0]), 5, Some(&filter))
        .expect("feature diagnostic query succeeds");

    assert_search_results_match(&public, &exact);
    assert_eq!(
        match_key_strings(&public),
        vec![
            "feature-only",
            "fresh-main",
            "doc-006",
            "doc-008",
            "doc-010"
        ]
    );
    assert_eq!(diagnostics.flat_source_count(), 1);
    assert_eq!(diagnostics.active_delta_source_count(), 1);
    assert_eq!(diagnostics.exact_source_count(), 0);
    assert_eq!(diagnostics.artifact_sources()[0].status(), "loaded");
}

#[cfg(feature = "testkit")]
#[test]
fn vector_index_manifest_config_mismatch_falls_back_to_exact_results() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let docs = collection("manifest-config");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    vectors
        .upsert(docs.clone(), vector_key("a"), embedding([1.0, 0.0]), None)
        .expect("upsert succeeds");
    vectors
        .seed_synthetic_index_manifest_for_test(
            &docs,
            &[
                (storage_branch_id(0x01), 3, VectorDistanceMetric::Cosine),
                (storage_branch_id(0x01), 2, VectorDistanceMetric::DotProduct),
            ],
            0,
        )
        .expect("manifest seed succeeds");

    let exact = vectors
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 10, None)
        .expect("exact query succeeds");
    let (diagnostic_result, diagnostics) = vectors
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 10, None)
        .expect("diagnostic query succeeds");

    assert_search_results_match(&diagnostic_result, &exact);
    assert_default_exact_policy_core(&diagnostics, "manifest-config", usize::MAX);
    assert_eq!(diagnostics.manifest_status(), "stale");
    assert_eq!(diagnostics.manifest_ref_count(), 0);
}

#[cfg(feature = "testkit")]
#[test]
fn vector_index_manifest_timestamp_query_uses_timestamp_visible_manifest() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let docs = collection("manifest-time");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    let snapshot = vectors
        .upsert(docs.clone(), vector_key("a"), embedding([1.0, 0.0]), None)
        .expect("upsert succeeds")
        .commit()
        .timestamp();
    vectors
        .seed_synthetic_index_manifest_for_test(
            &docs,
            &[(storage_branch_id(0x01), 2, VectorDistanceMetric::Cosine)],
            0,
        )
        .expect("manifest seed succeeds");

    let (diagnostic_result, diagnostics) = vectors
        .query_at_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 10, None, snapshot)
        .expect("timestamp diagnostic query succeeds");

    assert_eq!(match_key_strings(&diagnostic_result), vec!["a"]);
    assert_default_exact_policy_diagnostics(&diagnostics, "manifest-time", usize::MAX);
}

#[cfg(feature = "testkit")]
#[test]
fn vector_index_manifest_corruption_falls_back_to_exact_results() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let docs = collection("manifest-corrupt");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    vectors
        .upsert(docs.clone(), vector_key("a"), embedding([1.0, 0.0]), None)
        .expect("upsert succeeds");
    vectors
        .put_raw_index_manifest_for_test(&docs, vec![1, b'{'])
        .expect("raw manifest write succeeds");

    let public = vectors
        .query(&docs, &embedding([1.0, 0.0]), 10, None)
        .expect("public query succeeds");
    let (diagnostic_result, diagnostics) = vectors
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 10, None)
        .expect("diagnostic query succeeds");

    assert_search_results_match(&diagnostic_result, &public);
    assert_default_exact_policy_core(&diagnostics, "manifest-corrupt", usize::MAX);
    assert_eq!(diagnostics.manifest_status(), "corrupt");
    assert_eq!(diagnostics.manifest_generation(), None);
    assert_eq!(diagnostics.manifest_ref_count(), 0);
}

#[cfg(feature = "testkit")]
#[test]
fn vector_index_manifest_materializes_inherited_flat_refs_on_fork() {
    let mut database = open_cache_database().expect("cache open succeeds");
    {
        let mut parent = vector_service(&mut database, "default", "default");
        let docs = collection("manifest-fork-materialize");
        parent
            .create_collection(docs.clone(), config(2, VectorDistanceMetric::DotProduct))
            .expect("collection create succeeds");
        let entries = (0_u16..72)
            .map(|index| descending_fixture_upsert(index, json!({"kind": "doc"})))
            .collect::<Vec<_>>();
        parent
            .batch_upsert(&docs, &entries)
            .expect("batch upsert succeeds");
        parent
            .seed_flat_index_manifest_from_visible_rows_for_test(&docs, "source-a")
            .expect("flat artifact seed succeeds");
    }
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("branch fork succeeds");

    let mut feature = vector_service(&mut database, "feature", "default");
    let docs = collection("manifest-fork-materialize");
    let exact = feature
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("exact query succeeds");
    let (diagnostic_result, diagnostics) = feature
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("diagnostic query succeeds");

    assert_search_results_match(&diagnostic_result, &exact);
    assert_eq!(
        match_key_strings(&diagnostic_result),
        vec!["doc-000", "doc-001", "doc-002", "doc-003", "doc-004", "doc-005"]
    );
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.manifest_ref_count(), 1);
    assert_eq!(diagnostics.manifest_inherited_ref_count(), 1);
    assert_eq!(diagnostics.manifest_owned_ref_count(), 0);
    assert_eq!(diagnostics.flat_source_count(), 1);
    assert_eq!(diagnostics.active_delta_source_count(), 1);
    assert_eq!(diagnostics.exact_source_count(), 0);
    assert_eq!(diagnostics.indexed_vector_count(), 72);
    assert!(diagnostics.last_query_used_index());
    assert_eq!(diagnostics.last_query_fallback_reason(), None);
    assert_eq!(diagnostics.artifact_sources().len(), 1);
    assert_eq!(
        diagnostics.artifact_sources()[0].artifact_id(),
        "flat:default:manifest-fork-materialize:source-a"
    );
    assert_eq!(diagnostics.artifact_sources()[0].status(), "loaded");
    assert!(diagnostics.artifact_sources()[0].searched());
}

#[cfg(feature = "testkit")]
#[test]
fn vector_index_manifest_forked_branches_diverge_with_inherited_refs() {
    let mut database = open_cache_database().expect("cache open succeeds");
    {
        let mut parent = vector_service(&mut database, "default", "default");
        let docs = collection("manifest-fork-diverge");
        parent
            .create_collection(docs.clone(), config(2, VectorDistanceMetric::DotProduct))
            .expect("collection create succeeds");
        let entries = (0_u16..72)
            .map(|index| descending_fixture_upsert(index, json!({"kind": "doc"})))
            .collect::<Vec<_>>();
        parent
            .batch_upsert(&docs, &entries)
            .expect("batch upsert succeeds");
        parent
            .seed_flat_index_manifest_from_visible_rows_for_test(&docs, "source-a")
            .expect("flat artifact seed succeeds");
    }
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("branch fork succeeds");
    {
        let mut parent = vector_service(&mut database, "default", "default");
        parent
            .upsert(
                collection("manifest-fork-diverge"),
                vector_key("parent-late"),
                embedding([200.0, 0.0]),
                None,
            )
            .expect("parent upsert succeeds");
    }
    {
        let mut feature = vector_service(&mut database, "feature", "default");
        let docs = collection("manifest-fork-diverge");
        feature
            .upsert(
                docs.clone(),
                vector_key("feature-late"),
                embedding([300.0, 0.0]),
                None,
            )
            .expect("feature upsert succeeds");
        assert!(feature
            .delete(&docs, vector_key("doc-000"))
            .expect("feature delete succeeds")
            .deleted());
    }

    let docs = collection("manifest-fork-diverge");
    let mut parent = vector_service(&mut database, "default", "default");
    let parent_exact = parent
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 3, None)
        .expect("parent exact query succeeds");
    let (parent_indexed, parent_diagnostics) = parent
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 3, None)
        .expect("parent diagnostic query succeeds");
    assert_search_results_match(&parent_indexed, &parent_exact);
    assert_eq!(
        match_key_strings(&parent_indexed),
        vec!["parent-late", "doc-000", "doc-001"]
    );
    assert_eq!(parent_diagnostics.manifest_inherited_ref_count(), 0);
    assert_eq!(parent_diagnostics.manifest_owned_ref_count(), 1);
    assert_eq!(parent_diagnostics.flat_source_count(), 1);
    drop(parent);

    let mut feature = vector_service(&mut database, "feature", "default");
    let feature_exact = feature
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 3, None)
        .expect("feature exact query succeeds");
    let (feature_indexed, feature_diagnostics) = feature
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 3, None)
        .expect("feature diagnostic query succeeds");
    assert_search_results_match(&feature_indexed, &feature_exact);
    assert_eq!(
        match_key_strings(&feature_indexed),
        vec!["feature-late", "doc-001", "doc-002"]
    );
    assert_eq!(feature_diagnostics.manifest_inherited_ref_count(), 1);
    assert_eq!(feature_diagnostics.manifest_owned_ref_count(), 0);
    assert_eq!(feature_diagnostics.flat_source_count(), 1);
    assert_eq!(feature_diagnostics.active_delta_source_count(), 1);
    assert_eq!(feature_diagnostics.exact_source_count(), 0);
    assert_eq!(feature_diagnostics.last_query_fallback_reason(), None);
}

#[cfg(feature = "testkit")]
#[test]
fn vector_index_manifest_retained_version_fork_materializes_capped_refs() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let fork_version = {
        let mut parent = vector_service(&mut database, "default", "default");
        let docs = collection("manifest-fork-version");
        parent
            .create_collection(docs.clone(), config(2, VectorDistanceMetric::DotProduct))
            .expect("collection create succeeds");
        let entries = (0_u16..72)
            .map(|index| descending_fixture_upsert(index, json!({"kind": "doc"})))
            .collect::<Vec<_>>();
        parent
            .batch_upsert(&docs, &entries)
            .expect("batch upsert succeeds");
        parent
            .seed_flat_index_manifest_from_visible_rows_for_test(&docs, "source-a")
            .expect("flat artifact seed succeeds");
        let marker = parent
            .upsert(
                docs.clone(),
                vector_key("fork-marker"),
                embedding([250.0, 0.0]),
                None,
            )
            .expect("marker upsert succeeds")
            .commit()
            .version();
        parent
            .upsert(
                docs,
                vector_key("parent-after-fork-version"),
                embedding([500.0, 0.0]),
                None,
            )
            .expect("post-fork-version parent upsert succeeds");
        marker
    };
    database
        .branches()
        .expect("branch service opens")
        .fork_at_version(&branch("default"), branch("by-version"), fork_version)
        .expect("retained version fork succeeds");

    let docs = collection("manifest-fork-version");
    let mut feature = vector_service(&mut database, "by-version", "default");
    let exact = feature
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 4, None)
        .expect("feature exact query succeeds");
    let (indexed, diagnostics) = feature
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 4, None)
        .expect("feature diagnostic query succeeds");

    assert_search_results_match(&indexed, &exact);
    assert_eq!(
        match_key_strings(&indexed),
        vec!["fork-marker", "doc-000", "doc-001", "doc-002"]
    );
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.manifest_ref_count(), 1);
    assert_eq!(diagnostics.manifest_inherited_ref_count(), 1);
    assert_eq!(diagnostics.manifest_owned_ref_count(), 0);
    assert_eq!(diagnostics.flat_source_count(), 1);
    assert_eq!(diagnostics.active_delta_source_count(), 1);
    assert_eq!(diagnostics.exact_source_count(), 0);
    assert_eq!(diagnostics.last_query_fallback_reason(), None);
}

#[cfg(feature = "testkit")]
#[test]
fn vector_index_manifest_inherited_missing_artifact_falls_back_to_exact_results() {
    let mut database = open_cache_database().expect("cache open succeeds");
    {
        let mut parent = vector_service(&mut database, "default", "default");
        let docs = collection("manifest-fork-missing-artifact");
        parent
            .create_collection(docs.clone(), config(2, VectorDistanceMetric::DotProduct))
            .expect("collection create succeeds");
        let entries = (0_u16..72)
            .map(|index| descending_fixture_upsert(index, json!({"kind": "doc"})))
            .collect::<Vec<_>>();
        parent
            .batch_upsert(&docs, &entries)
            .expect("batch upsert succeeds");
        parent
            .seed_flat_index_manifest_from_visible_rows_for_test(&docs, "source-a")
            .expect("flat artifact seed succeeds");
    }
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("branch fork succeeds");

    let mut feature = vector_service(&mut database, "feature", "default");
    let docs = collection("manifest-fork-missing-artifact");
    feature
        .remove_flat_artifact_for_test(&docs, "source-a")
        .expect("artifact remove succeeds");
    let exact = feature
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("exact query succeeds");
    let (indexed, diagnostics) = feature
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("diagnostic query succeeds");

    assert_search_results_match(&indexed, &exact);
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.manifest_ref_count(), 1);
    assert_eq!(diagnostics.manifest_inherited_ref_count(), 1);
    assert_eq!(diagnostics.manifest_owned_ref_count(), 0);
    assert_eq!(diagnostics.flat_source_count(), 0);
    assert_eq!(diagnostics.exact_source_count(), 1);
    assert_eq!(
        diagnostics.last_query_fallback_reason(),
        Some("artifact_unavailable")
    );
    assert_eq!(diagnostics.artifact_sources().len(), 1);
    assert_eq!(diagnostics.artifact_sources()[0].status(), "missing");
    assert!(!diagnostics.artifact_sources()[0].searched());
}
