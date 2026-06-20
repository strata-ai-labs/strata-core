use super::*;

#[test]
fn vector_active_delta_does_not_seal_below_flat_policy_threshold() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let mut vectors = vector_service(&mut database, "default", "default");
    let docs = collection("active-delta-small");
    vectors
        .create_collection(docs.clone(), config(2, VectorDistanceMetric::Cosine))
        .expect("collection create succeeds");
    let entries = (0_u16..20)
        .map(|index| descending_fixture_upsert(index, json!({"kind": "doc"})))
        .collect::<Vec<_>>();
    vectors
        .batch_upsert(&docs, &entries)
        .expect("batch upsert succeeds");

    let (result, diagnostics) = vectors
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 5, None)
        .expect("diagnostic query succeeds");

    assert_eq!(
        match_key_strings(&result),
        vec!["doc-000", "doc-001", "doc-002", "doc-003", "doc-004"]
    );
    assert_default_exact_policy_core(&diagnostics, "active-delta-small", usize::MAX);
    assert_eq!(diagnostics.manifest_status(), "missing");
}
#[test]
fn vector_source_owned_flat_artifacts_run_in_cache_and_durable_modes() {
    run_database_modes(exercise_vector_source_owned_flat_artifacts);
}
fn exercise_vector_source_owned_flat_artifacts(database: &mut Database) {
    let docs = collection("source-owned-flat");
    {
        let mut vectors = vector_service(database, "default", "default");
        vectors
            .create_collection(docs.clone(), config(2, VectorDistanceMetric::DotProduct))
            .expect("collection create succeeds");
        let entries = (0_u16..72)
            .map(|index| descending_fixture_upsert(index, json!({"kind": "doc"})))
            .collect::<Vec<_>>();
        vectors
            .batch_upsert(&docs, &entries)
            .expect("first batch upsert succeeds");
    }
    database
        .flush_storage_branch_for_test(&branch("default"))
        .expect("first flush succeeds");
    {
        let mut vectors = vector_service(database, "default", "default");
        let entries = (72_u16..144)
            .map(|index| descending_fixture_upsert(index, json!({"kind": "doc"})))
            .collect::<Vec<_>>();
        vectors
            .batch_upsert(&docs, &entries)
            .expect("second batch upsert succeeds");
    }
    database
        .flush_storage_branch_for_test(&branch("default"))
        .expect("second flush succeeds");

    let mut vectors = vector_service(database, "default", "default");
    vectors
        .seal_index_artifacts_for_test(&docs, VectorIndexPolicy::flat_only_for_test())
        .expect("flat artifacts seal succeeds");
    let exact = vectors
        .query_exact_for_test(
            &docs,
            &embedding([1.0, 0.0]),
            6,
            Some(&filter_eq("kind", "doc")),
        )
        .expect("exact query succeeds");
    let (indexed, diagnostics) = vectors
        .query_with_index_diagnostics_for_test(
            &docs,
            &embedding([1.0, 0.0]),
            6,
            Some(&filter_eq("kind", "doc")),
        )
        .expect("diagnostic query succeeds");
    let public = vectors
        .query(
            &docs,
            &embedding([1.0, 0.0]),
            6,
            Some(&filter_eq("kind", "doc")),
        )
        .expect("public query succeeds");

    assert_search_results_match(&indexed, &exact);
    assert_search_results_match(&public, &exact);
    assert_eq!(
        match_key_strings(&indexed),
        vec!["doc-000", "doc-001", "doc-002", "doc-003", "doc-004", "doc-005"]
    );
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert!(diagnostics.manifest_ref_count() >= 2);
    assert_eq!(
        diagnostics.manifest_ref_count(),
        diagnostics.manifest_owned_ref_count()
    );
    assert_eq!(diagnostics.manifest_inherited_ref_count(), 0);
    assert_eq!(
        diagnostics.flat_source_count(),
        diagnostics.manifest_ref_count()
    );
    assert_eq!(diagnostics.exact_source_count(), 0);
    assert_eq!(diagnostics.indexed_vector_count(), 144);
    assert!(diagnostics.last_query_used_index());
    assert_eq!(diagnostics.last_query_fallback_reason(), None);
    assert_eq!(
        diagnostics.artifact_sources().len(),
        diagnostics.manifest_ref_count()
    );
    assert!(diagnostics
        .artifact_sources()
        .iter()
        .all(|source| source.artifact_id().contains(":source:")));
    assert!(diagnostics
        .artifact_sources()
        .iter()
        .all(|source| source.status() == "loaded" && source.searched()));
}

#[cfg(feature = "testkit")]
#[test]
fn vector_durable_flat_artifacts_are_reused_after_reopen() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let docs = collection("durable-flat-reuse");
    {
        let mut database = open_durable_database(tempdir.path()).expect("durable open succeeds");
        create_source_owned_parent_artifacts(&mut database, &docs);
        database.close().expect("durable close succeeds");
    }

    let mut reopened = open_durable_database(tempdir.path()).expect("reopen succeeds");
    let mut vectors = vector_service(&mut reopened, "default", "default");
    let exact = vectors
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("exact query succeeds");
    let (indexed, diagnostics) = vectors
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("diagnostic query succeeds");
    let manifest_bytes = vectors
        .index_manifest_byte_len_for_test(&docs)
        .expect("manifest length reads")
        .expect("manifest exists");

    assert_search_results_match(&indexed, &exact);
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert!(diagnostics.manifest_ref_count() >= 2);
    assert_eq!(
        diagnostics.flat_source_count(),
        diagnostics.manifest_ref_count()
    );
    assert_eq!(diagnostics.exact_source_count(), 0);
    assert!(diagnostics.last_query_used_index());
    assert_eq!(diagnostics.last_query_fallback_reason(), None);
    assert!(diagnostics.derived_bytes() > u64::try_from(manifest_bytes).unwrap_or(u64::MAX));
    assert!(manifest_bytes < 16 * 1024);
    assert!(diagnostics
        .artifact_sources()
        .iter()
        .all(|source| source.status() == "loaded" && source.searched()));
}

#[cfg(feature = "testkit")]
#[test]
fn vector_durable_hnsw_artifacts_are_reused_after_reopen() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let docs = collection("durable-hnsw-reuse");
    {
        let mut database = open_durable_database(tempdir.path()).expect("durable open succeeds");
        create_source_owned_hnsw_artifacts(&mut database, &docs);
        database.close().expect("durable close succeeds");
    }

    let mut reopened = open_durable_database(tempdir.path()).expect("reopen succeeds");
    let mut vectors = vector_service(&mut reopened, "default", "default");
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
    let manifest_bytes = vectors
        .index_manifest_byte_len_for_test(&docs)
        .expect("manifest length reads")
        .expect("manifest exists");

    assert_search_results_match(&indexed, &exact);
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.manifest_ref_count(), 2);
    assert_eq!(diagnostics.hnsw_source_count(), 1);
    assert_eq!(diagnostics.flat_source_count(), 0);
    assert_eq!(diagnostics.exact_source_count(), 0);
    assert!(diagnostics.last_query_used_index());
    assert_eq!(diagnostics.last_query_fallback_reason(), None);
    assert!(manifest_bytes < 16 * 1024);
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
fn vector_durable_missing_hnsw_artifacts_fall_back_to_flat_without_rebuild() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let docs = collection("durable-hnsw-missing");
    {
        let mut database = open_durable_database(tempdir.path()).expect("durable open succeeds");
        create_source_owned_hnsw_artifacts(&mut database, &docs);
        database.close().expect("durable close succeeds");
    }

    let mut reopened = open_durable_database(tempdir.path()).expect("reopen succeeds");
    let mut vectors = vector_service(&mut reopened, "default", "default");
    let removed = vectors
        .remove_manifest_hnsw_artifacts_for_test(&docs)
        .expect("HNSW artifact files are removed");
    assert_eq!(removed, 1);
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
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.hnsw_source_count(), 0);
    assert_eq!(diagnostics.flat_source_count(), 1);
    assert_eq!(diagnostics.exact_source_count(), 0);
    assert_eq!(
        diagnostics.last_query_fallback_reason(),
        Some("artifact_unavailable")
    );
    assert!(diagnostics.artifact_sources().iter().any(|source| {
        source.artifact_id().starts_with("hnsw:")
            && source.status() == "missing"
            && !source.searched()
    }));
    assert!(diagnostics.artifact_sources().iter().any(|source| {
        source.artifact_id().starts_with("flat:")
            && source.status() == "loaded"
            && source.searched()
    }));
}

#[cfg(feature = "testkit")]
#[test]
fn vector_durable_hnsw_load_budget_falls_back_to_flat_after_reopen() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let docs = collection("durable-hnsw-load-budget");
    {
        let mut database = open_durable_database(tempdir.path()).expect("durable open succeeds");
        create_source_owned_hnsw_artifacts(&mut database, &docs);
        database.close().expect("durable close succeeds");
    }

    let mut reopened = open_durable_database(tempdir.path()).expect("reopen succeeds");
    let mut vectors = vector_service(&mut reopened, "default", "default");
    let exact = vectors
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 8, None)
        .expect("exact query succeeds");
    let (indexed, diagnostics) = vectors
        .query_with_index_policy_for_test(
            &docs,
            &embedding([1.0, 0.0]),
            8,
            None,
            VectorIndexPolicy::hnsw_only_for_test().with_hnsw_artifact_load_budget_for_test(1),
        )
        .expect("diagnostic query succeeds");

    assert_search_results_match(&indexed, &exact);
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.hnsw_source_count(), 0);
    assert_eq!(diagnostics.flat_source_count(), 1);
    assert_eq!(diagnostics.exact_source_count(), 0);
    assert_eq!(diagnostics.exact_fallback_count(), 0);
    assert_eq!(
        diagnostics.last_query_fallback_reason(),
        Some("artifact_unavailable")
    );
    assert!(diagnostics.artifact_sources().iter().any(|source| {
        source.artifact_id().starts_with("hnsw:")
            && source.status() == "over_budget"
            && !source.searched()
    }));
    assert!(diagnostics.artifact_sources().iter().any(|source| {
        source.artifact_id().starts_with("flat:")
            && source.status() == "loaded"
            && source.searched()
    }));
}

#[cfg(feature = "testkit")]
#[test]
fn vector_durable_hnsw_memory_budget_falls_back_to_flat_after_reopen() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let docs = collection("durable-hnsw-memory-budget");
    {
        let mut database = open_durable_database(tempdir.path()).expect("durable open succeeds");
        create_source_owned_hnsw_artifacts(&mut database, &docs);
        database.close().expect("durable close succeeds");
    }

    let mut reopened = open_durable_database(tempdir.path()).expect("reopen succeeds");
    let mut vectors = vector_service(&mut reopened, "default", "default");
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
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.hnsw_memory_budget_bytes(), 1);
    assert_eq!(diagnostics.hnsw_source_count(), 0);
    assert_eq!(diagnostics.flat_source_count(), 1);
    assert_eq!(diagnostics.exact_source_count(), 0);
    assert_eq!(diagnostics.exact_fallback_count(), 0);
    assert_eq!(
        diagnostics.last_query_fallback_reason(),
        Some("hnsw_memory_budget_exceeded")
    );
    assert!(diagnostics.artifact_sources().iter().any(|source| {
        source.artifact_id().starts_with("hnsw:")
            && source.status() == "loaded"
            && !source.searched()
    }));
    assert!(diagnostics.artifact_sources().iter().any(|source| {
        source.artifact_id().starts_with("flat:")
            && source.status() == "loaded"
            && source.searched()
    }));
}

#[cfg(feature = "testkit")]
#[test]
fn vector_durable_corrupt_hnsw_artifacts_fall_back_to_flat_without_rebuild() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let docs = collection("durable-hnsw-corrupt");
    {
        let mut database = open_durable_database(tempdir.path()).expect("durable open succeeds");
        create_source_owned_hnsw_artifacts(&mut database, &docs);
        database.close().expect("durable close succeeds");
    }

    let mut reopened = open_durable_database(tempdir.path()).expect("reopen succeeds");
    let mut vectors = vector_service(&mut reopened, "default", "default");
    let corrupted = vectors
        .corrupt_manifest_hnsw_artifacts_for_test(&docs)
        .expect("HNSW artifact files are corrupted");
    assert_eq!(corrupted, 1);
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
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.hnsw_source_count(), 0);
    assert_eq!(diagnostics.flat_source_count(), 1);
    assert_eq!(diagnostics.exact_source_count(), 0);
    assert_eq!(
        diagnostics.last_query_fallback_reason(),
        Some("artifact_unavailable")
    );
    assert!(diagnostics.artifact_sources().iter().any(|source| {
        source.artifact_id().starts_with("hnsw:")
            && source.status() == "corrupt"
            && !source.searched()
    }));
    assert!(diagnostics.artifact_sources().iter().any(|source| {
        source.artifact_id().starts_with("flat:")
            && source.status() == "loaded"
            && source.searched()
    }));
}

#[cfg(feature = "testkit")]
#[test]
fn vector_durable_missing_flat_artifacts_fall_back_to_exact_without_rebuild() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let docs = collection("durable-flat-missing");
    {
        let mut database = open_durable_database(tempdir.path()).expect("durable open succeeds");
        create_source_owned_parent_artifacts(&mut database, &docs);
        database.close().expect("durable close succeeds");
    }

    let mut reopened = open_durable_database(tempdir.path()).expect("reopen succeeds");
    let mut vectors = vector_service(&mut reopened, "default", "default");
    let removed = vectors
        .remove_manifest_flat_artifacts_for_test(&docs)
        .expect("artifact files are removed");
    assert!(removed >= 2);
    let exact = vectors
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("exact query succeeds");
    let (indexed, diagnostics) = vectors
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("diagnostic query succeeds");

    assert_search_results_match(&indexed, &exact);
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.flat_source_count(), 0);
    assert_eq!(diagnostics.exact_source_count(), 1);
    assert_eq!(
        diagnostics.last_query_fallback_reason(),
        Some("artifact_unavailable")
    );
    assert!(diagnostics
        .artifact_sources()
        .iter()
        .all(|source| source.status() == "missing" && !source.searched()));

    vectors
        .seal_index_artifacts_for_test(&docs, VectorIndexPolicy::flat_only_for_test())
        .expect("explicit maintenance seal succeeds");
    let (resealed, resealed_diagnostics) = vectors
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("diagnostic query succeeds after explicit seal");
    assert_search_results_match(&resealed, &exact);
    assert_eq!(
        resealed_diagnostics.flat_source_count(),
        resealed_diagnostics.manifest_ref_count()
    );
    assert_eq!(resealed_diagnostics.exact_source_count(), 0);
    assert!(resealed_diagnostics.last_query_used_index());
    assert_eq!(resealed_diagnostics.last_query_fallback_reason(), None);
    assert!(resealed_diagnostics
        .artifact_sources()
        .iter()
        .all(|source| source.status() == "loaded" && source.searched()));
}

#[cfg(feature = "testkit")]
#[test]
fn vector_durable_flat_load_budget_falls_back_to_exact_after_reopen() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let docs = collection("durable-flat-load-budget");
    {
        let mut database = open_durable_database(tempdir.path()).expect("durable open succeeds");
        create_source_owned_parent_artifacts(&mut database, &docs);
        database.close().expect("durable close succeeds");
    }

    let mut reopened = open_durable_database(tempdir.path()).expect("reopen succeeds");
    let mut vectors = vector_service(&mut reopened, "default", "default");
    let exact = vectors
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("exact query succeeds");
    let (indexed, diagnostics) = vectors
        .query_with_index_policy_for_test(
            &docs,
            &embedding([1.0, 0.0]),
            6,
            None,
            VectorIndexPolicy::flat_only_for_test().with_flat_artifact_load_budget_for_test(1),
        )
        .expect("diagnostic query succeeds");

    assert_search_results_match(&indexed, &exact);
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.flat_source_count(), 0);
    assert_eq!(diagnostics.exact_source_count(), 1);
    assert_eq!(diagnostics.exact_fallback_count(), 0);
    assert_eq!(
        diagnostics.last_query_fallback_reason(),
        Some("artifact_unavailable")
    );
    assert!(diagnostics
        .artifact_sources()
        .iter()
        .all(|source| source.status() == "over_budget" && !source.searched()));
}

#[cfg(feature = "testkit")]
#[test]
fn vector_durable_corrupt_flat_artifacts_fall_back_to_exact_without_rebuild() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let docs = collection("durable-flat-corrupt");
    {
        let mut database = open_durable_database(tempdir.path()).expect("durable open succeeds");
        create_source_owned_parent_artifacts(&mut database, &docs);
        database.close().expect("durable close succeeds");
    }

    let mut reopened = open_durable_database(tempdir.path()).expect("reopen succeeds");
    let mut vectors = vector_service(&mut reopened, "default", "default");
    let corrupted = vectors
        .corrupt_manifest_flat_artifacts_for_test(&docs)
        .expect("artifact files are corrupted");
    assert!(corrupted >= 2);
    let exact = vectors
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("exact query succeeds");
    let (indexed, diagnostics) = vectors
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("diagnostic query succeeds");

    assert_search_results_match(&indexed, &exact);
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.flat_source_count(), 0);
    assert_eq!(diagnostics.exact_source_count(), 1);
    assert_eq!(
        diagnostics.last_query_fallback_reason(),
        Some("artifact_unavailable")
    );
    assert!(diagnostics
        .artifact_sources()
        .iter()
        .all(|source| source.status() == "corrupt" && !source.searched()));
}

#[cfg(feature = "testkit")]
#[test]
fn vector_durable_partially_written_flat_artifacts_on_reopen_fall_back_to_exact() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let docs = collection("durable-flat-partial-reopen");
    let artifact_ids = {
        let mut database = open_durable_database(tempdir.path()).expect("durable open succeeds");
        let artifact_ids = create_source_owned_parent_artifacts(&mut database, &docs);
        database.close().expect("durable close succeeds");
        artifact_ids
    };
    assert!(artifact_ids.len() >= 2);

    for artifact_id in &artifact_ids {
        let path = durable_flat_artifact_file(tempdir.path(), artifact_id);
        let bytes = std::fs::read(&path).expect("durable flat artifact exists before truncation");
        assert!(bytes.len() > 8);
        std::fs::write(&path, &bytes[..8]).expect("durable flat artifact truncates");
    }

    let mut reopened = open_durable_database(tempdir.path()).expect("reopen succeeds");
    let mut vectors = vector_service(&mut reopened, "default", "default");
    let exact = vectors
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("exact query succeeds");
    let (indexed, diagnostics) = vectors
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("diagnostic query succeeds");

    assert_search_results_match(&indexed, &exact);
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.flat_source_count(), 0);
    assert_eq!(diagnostics.exact_source_count(), 1);
    assert_eq!(diagnostics.exact_fallback_count(), 0);
    assert_eq!(
        diagnostics.last_query_fallback_reason(),
        Some("artifact_unavailable")
    );
    assert!(diagnostics
        .artifact_sources()
        .iter()
        .all(|source| source.status() == "corrupt" && !source.searched()));
    for artifact_id in &artifact_ids {
        let path = durable_flat_artifact_file(tempdir.path(), artifact_id);
        assert_eq!(
            std::fs::metadata(path)
                .expect("truncated flat artifact still exists")
                .len(),
            8
        );
    }
}

#[cfg(feature = "testkit")]
#[test]
fn vector_durable_stale_flat_artifacts_fall_back_to_exact_without_rebuild() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let docs = collection("durable-flat-stale");
    {
        let mut database = open_durable_database(tempdir.path()).expect("durable open succeeds");
        create_source_owned_parent_artifacts(&mut database, &docs);
        database.close().expect("durable close succeeds");
    }

    let mut reopened = open_durable_database(tempdir.path()).expect("reopen succeeds");
    let mut vectors = vector_service(&mut reopened, "default", "default");
    let stale = vectors
        .make_manifest_flat_artifacts_stale_for_test(&docs)
        .expect("artifact files are made stale");
    assert!(stale >= 2);
    let exact = vectors
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("exact query succeeds");
    let (indexed, diagnostics) = vectors
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("diagnostic query succeeds");

    assert_search_results_match(&indexed, &exact);
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.flat_source_count(), 0);
    assert_eq!(diagnostics.exact_source_count(), 1);
    assert_eq!(
        diagnostics.last_query_fallback_reason(),
        Some("artifact_unavailable")
    );
    assert!(diagnostics
        .artifact_sources()
        .iter()
        .all(|source| source.status() == "stale" && !source.searched()));
}

#[cfg(feature = "testkit")]
#[test]
fn vector_durable_partially_written_hnsw_artifacts_on_reopen_fall_back_to_flat() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let docs = collection("durable-hnsw-partial-reopen");
    let (flat_ids, hnsw_ids) = {
        let mut database = open_durable_database(tempdir.path()).expect("durable open succeeds");
        create_source_owned_hnsw_artifacts(&mut database, &docs);
        let mut vectors = vector_service(&mut database, "default", "default");
        let artifact_ids = vectors
            .index_manifest_artifact_ids_for_test(&docs)
            .expect("manifest artifact IDs read");
        database.close().expect("durable close succeeds");
        let flat_ids = artifact_ids
            .iter()
            .filter(|artifact_id| artifact_id.starts_with("flat:"))
            .cloned()
            .collect::<Vec<_>>();
        let hnsw_ids = artifact_ids
            .into_iter()
            .filter(|artifact_id| artifact_id.starts_with("hnsw:"))
            .collect::<Vec<_>>();
        (flat_ids, hnsw_ids)
    };
    assert_eq!(flat_ids.len(), 1);
    assert_eq!(hnsw_ids.len(), 1);

    for artifact_id in &hnsw_ids {
        let path = durable_hnsw_artifact_file(tempdir.path(), artifact_id);
        let bytes = std::fs::read(&path).expect("durable HNSW artifact exists before truncation");
        assert!(bytes.len() > 8);
        std::fs::write(&path, &bytes[..8]).expect("durable HNSW artifact truncates");
    }

    let mut reopened = open_durable_database(tempdir.path()).expect("reopen succeeds");
    let mut vectors = vector_service(&mut reopened, "default", "default");
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
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.hnsw_source_count(), 0);
    assert_eq!(diagnostics.flat_source_count(), 1);
    assert_eq!(diagnostics.exact_source_count(), 0);
    assert_eq!(diagnostics.exact_fallback_count(), 0);
    assert_eq!(
        diagnostics.last_query_fallback_reason(),
        Some("artifact_unavailable")
    );
    assert!(diagnostics.artifact_sources().iter().any(|source| {
        source.artifact_id().starts_with("hnsw:")
            && source.status() == "corrupt"
            && !source.searched()
    }));
    assert!(diagnostics.artifact_sources().iter().any(|source| {
        source.artifact_id().starts_with("flat:")
            && source.status() == "loaded"
            && source.searched()
    }));
    for artifact_id in &hnsw_ids {
        let path = durable_hnsw_artifact_file(tempdir.path(), artifact_id);
        assert_eq!(
            std::fs::metadata(path)
                .expect("truncated HNSW artifact still exists")
                .len(),
            8
        );
    }
    for artifact_id in &flat_ids {
        let path = durable_flat_artifact_file(tempdir.path(), artifact_id);
        assert!(
            std::fs::metadata(path)
                .expect("flat fallback artifact remains durable")
                .len()
                > 8
        );
    }
}

#[cfg(feature = "testkit")]
#[test]
fn vector_durable_artifact_io_failure_does_not_fail_query() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let docs = collection("durable-flat-io-failure");
    {
        let mut database = open_durable_database(tempdir.path()).expect("durable open succeeds");
        create_source_owned_parent_artifacts(&mut database, &docs);
        database.close().expect("durable close succeeds");
    }
    let flat_dir = durable_flat_artifact_dir(tempdir.path());
    std::fs::remove_dir_all(&flat_dir).expect("artifact directory removes");
    std::fs::write(&flat_dir, b"not a directory").expect("artifact directory obstruction writes");

    let mut reopened = open_durable_database(tempdir.path()).expect("reopen succeeds");
    let mut vectors = vector_service(&mut reopened, "default", "default");
    let exact = vectors
        .query_exact_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("exact query succeeds");
    let (indexed, diagnostics) = vectors
        .query_with_index_diagnostics_for_test(&docs, &embedding([1.0, 0.0]), 6, None)
        .expect("diagnostic query succeeds despite artifact IO failure");

    assert_search_results_match(&indexed, &exact);
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.flat_source_count(), 0);
    assert_eq!(diagnostics.exact_source_count(), 1);
    assert_eq!(
        diagnostics.last_query_fallback_reason(),
        Some("artifact_unavailable")
    );
    assert!(diagnostics
        .artifact_sources()
        .iter()
        .all(|source| source.status() == "corrupt" && !source.searched()));
}

#[cfg(feature = "testkit")]
#[test]
fn vector_source_owned_flat_artifacts_are_inherited_in_cache_and_durable_modes() {
    run_database_modes(exercise_vector_source_owned_flat_artifact_inheritance);
}

#[cfg(feature = "testkit")]
fn exercise_vector_source_owned_flat_artifact_inheritance(database: &mut Database) {
    let docs = collection("source-owned-inherit");
    let parent_artifact_ids = create_source_owned_parent_artifacts(database, &docs);

    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("branch fork succeeds");

    assert_feature_inherits_source_artifact_refs(database, &docs, &parent_artifact_ids);
    upsert_feature_vector_and_assert_manifest_refs(database, &docs, &parent_artifact_ids);
    assert_feature_query_uses_inherited_source_artifacts(database, &docs, &parent_artifact_ids);
}

#[cfg(feature = "testkit")]
fn create_source_owned_parent_artifacts(
    database: &mut Database,
    docs: &VectorCollectionName,
) -> Vec<String> {
    {
        let mut parent = vector_service(database, "default", "default");
        parent
            .create_collection(docs.to_owned(), config(2, VectorDistanceMetric::DotProduct))
            .expect("collection create succeeds");
        let entries = (0_u16..72)
            .map(|index| descending_fixture_upsert(index, json!({"kind": "doc"})))
            .collect::<Vec<_>>();
        parent
            .batch_upsert(docs, &entries)
            .expect("first batch upsert succeeds");
    }
    database
        .flush_storage_branch_for_test(&branch("default"))
        .expect("first flush succeeds");
    {
        let mut parent = vector_service(database, "default", "default");
        let entries = (72_u16..144)
            .map(|index| descending_fixture_upsert(index, json!({"kind": "doc"})))
            .collect::<Vec<_>>();
        parent
            .batch_upsert(docs, &entries)
            .expect("second batch upsert succeeds");
    }
    database
        .flush_storage_branch_for_test(&branch("default"))
        .expect("second flush succeeds");

    let mut parent = vector_service(database, "default", "default");
    parent
        .seal_index_artifacts_for_test(docs, VectorIndexPolicy::flat_only_for_test())
        .expect("flat artifacts seal succeeds");
    let (indexed, diagnostics) = parent
        .query_with_index_diagnostics_for_test(docs, &embedding([1.0, 0.0]), 5, None)
        .expect("parent diagnostic query succeeds");
    let exact = parent
        .query_exact_for_test(docs, &embedding([1.0, 0.0]), 5, None)
        .expect("parent exact query succeeds");
    assert_search_results_match(&indexed, &exact);
    assert!(diagnostics.manifest_ref_count() >= 2);
    assert_eq!(diagnostics.manifest_inherited_ref_count(), 0);
    assert_eq!(
        diagnostics.manifest_ref_count(),
        diagnostics.manifest_owned_ref_count()
    );
    diagnostics
        .artifact_sources()
        .iter()
        .map(|source| source.artifact_id().to_owned())
        .collect::<Vec<_>>()
}

#[cfg(feature = "testkit")]
fn create_source_owned_hnsw_artifacts(database: &mut Database, docs: &VectorCollectionName) {
    {
        let mut vectors = vector_service(database, "default", "default");
        vectors
            .create_collection(docs.to_owned(), config(2, VectorDistanceMetric::Cosine))
            .expect("collection create succeeds");
        let entries = (0_u16..320)
            .map(|index| descending_fixture_upsert(index, json!({"kind": "doc"})))
            .collect::<Vec<_>>();
        vectors
            .batch_upsert(docs, &entries)
            .expect("batch upsert succeeds");
    }
    database
        .flush_storage_branch_for_test(&branch("default"))
        .expect("flush succeeds");

    let mut vectors = vector_service(database, "default", "default");
    vectors
        .seal_index_artifacts_for_test(docs, VectorIndexPolicy::hnsw_only_for_test())
        .expect("HNSW artifacts seal succeeds");
    let exact = vectors
        .query_exact_for_test(docs, &embedding([1.0, 0.0]), 8, None)
        .expect("exact query succeeds");
    let (indexed, diagnostics) = vectors
        .query_with_index_policy_for_test(
            docs,
            &embedding([1.0, 0.0]),
            8,
            None,
            VectorIndexPolicy::hnsw_only_for_test(),
        )
        .expect("diagnostic query succeeds");

    assert_search_results_match(&indexed, &exact);
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.manifest_ref_count(), 2);
    assert_eq!(diagnostics.hnsw_source_count(), 1);
    assert_eq!(diagnostics.flat_source_count(), 0);
    assert_eq!(diagnostics.exact_source_count(), 0);
    assert!(diagnostics.last_query_used_index());
    assert_eq!(diagnostics.last_query_fallback_reason(), None);
    assert!(diagnostics.artifact_sources().iter().any(|source| {
        source.artifact_id().starts_with("hnsw:")
            && source.status() == "loaded"
            && source.searched()
    }));
}

#[cfg(feature = "testkit")]
fn assert_feature_inherits_source_artifact_refs(
    database: &mut Database,
    docs: &VectorCollectionName,
    parent_artifact_ids: &[String],
) {
    let mut feature = vector_service(database, "feature", "default");
    let ids = feature
        .index_manifest_artifact_ids_for_test(docs)
        .expect("feature manifest IDs are readable after fork");
    assert_eq!(ids, parent_artifact_ids);
}

#[cfg(feature = "testkit")]
fn upsert_feature_vector_and_assert_manifest_refs(
    database: &mut Database,
    docs: &VectorCollectionName,
    parent_artifact_ids: &[String],
) {
    let mut feature = vector_service(database, "feature", "default");
    feature
        .upsert(
            docs.to_owned(),
            vector_key("feature-late"),
            embedding([200.0, 0.0]),
            Some(metadata(json!({"kind": "doc"}))),
        )
        .expect("feature upsert succeeds");
    let ids = feature
        .index_manifest_artifact_ids_for_test(docs)
        .expect("feature manifest IDs are readable after upsert");
    assert_eq!(ids, parent_artifact_ids);
}

#[cfg(feature = "testkit")]
fn assert_feature_query_uses_inherited_source_artifacts(
    database: &mut Database,
    docs: &VectorCollectionName,
    parent_artifact_ids: &[String],
) {
    let mut feature = vector_service(database, "feature", "default");
    let exact = feature
        .query_exact_for_test(docs, &embedding([1.0, 0.0]), 5, None)
        .expect("feature exact query succeeds");
    let (indexed, diagnostics) = feature
        .query_with_index_diagnostics_for_test(docs, &embedding([1.0, 0.0]), 5, None)
        .expect("feature diagnostic query succeeds");
    let child_artifact_ids = diagnostics
        .artifact_sources()
        .iter()
        .map(|source| source.artifact_id().to_owned())
        .collect::<Vec<_>>();

    assert_search_results_match(&indexed, &exact);
    assert_eq!(
        match_key_strings(&indexed),
        vec!["feature-late", "doc-000", "doc-001", "doc-002", "doc-003"]
    );
    assert_eq!(diagnostics.manifest_status(), "loaded");
    assert_eq!(diagnostics.manifest_ref_count(), parent_artifact_ids.len());
    assert_eq!(
        diagnostics.manifest_ref_count(),
        diagnostics.manifest_inherited_ref_count()
    );
    assert_eq!(diagnostics.manifest_owned_ref_count(), 0);
    assert!(diagnostics.flat_source_count() >= 1);
    assert_eq!(diagnostics.active_delta_source_count(), 1);
    assert_eq!(diagnostics.exact_source_count(), 0);
    assert_eq!(diagnostics.last_query_fallback_reason(), None);
    assert!(child_artifact_ids
        .iter()
        .all(|artifact_id| parent_artifact_ids.contains(artifact_id)));
}
