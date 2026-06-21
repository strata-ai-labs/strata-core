//! Engine admin and space service behavior tests.

mod common;

use strata_engine_next::{AdminHealthStatus, DatabaseOpenTarget, EngineErrorClass, ProductSpace};
#[cfg(feature = "testkit")]
use strata_engine_next::{
    VectorCollectionName, VectorConfig, VectorDistanceMetric, VectorEmbedding, VectorKey,
};

use common::{branch, key, open_cache_database, open_durable_database, space, value};

#[test]
fn admin_reports_sanitized_database_and_primitive_facts() {
    let mut database = open_cache_database().expect("cache database opens");
    database
        .kv(branch("default"), space("default"))
        .expect("KV service opens")
        .put(key(b"alpha"), value(b"one"))
        .expect("kv put succeeds");
    database
        .spaces(branch("default"))
        .expect("space service opens")
        .create(space("tenant_a"))
        .expect("space create succeeds");

    let mut admin = database.admin().expect("admin service opens");
    let ping = admin.ping();
    assert!(!ping.version.is_empty());

    let info = admin.info(Some(&branch("default"))).expect("info succeeds");
    assert_eq!(info.target, DatabaseOpenTarget::Cache);
    assert!(info.created);
    assert!(!info.durable);
    assert!(info.open);
    assert_eq!(info.default_branch.as_str(), "default");
    assert_eq!(info.branch_count, 1);
    assert_eq!(info.space_count, 2);

    let health = admin.health(Some(&branch("default")));
    assert_eq!(health.status, AdminHealthStatus::Healthy);
    assert_eq!(health.default_branch.as_str(), "default");
    assert_eq!(health.branch_count, 1);

    let metrics = admin
        .metrics(Some(&branch("default")))
        .expect("metrics succeeds");
    assert_eq!(metrics.target, DatabaseOpenTarget::Cache);
    assert_eq!(metrics.control_status, AdminHealthStatus::Healthy);
    assert_eq!(metrics.space_count, 2);

    let describe = admin
        .describe(Some(&branch("default")))
        .expect("describe succeeds");
    assert_eq!(describe.branch.as_str(), "default");
    assert_eq!(
        describe
            .spaces
            .iter()
            .map(ProductSpace::as_str)
            .collect::<Vec<_>>(),
        vec!["default", "tenant_a"]
    );
    assert_eq!(describe.primitives.kv_count, 1);
    assert!(describe.capabilities.kv);
    assert!(describe.capabilities.json);
    assert!(describe.capabilities.event);
    assert!(describe.capabilities.vector);
    assert!(describe.capabilities.vector_index);
    assert!(describe.capabilities.graph_core);

    let config = admin.config();
    assert_eq!(config.target, DatabaseOpenTarget::Cache);
    assert_eq!(config.default_branch.as_str(), "default");
    assert_eq!(
        admin
            .config_value("default_branch")
            .expect("config key succeeds"),
        Some("default".to_owned())
    );
    assert_eq!(
        admin
            .config_value("openai_api_key")
            .expect("secret-like unknown key is hidden"),
        None
    );
}

#[test]
fn space_delete_force_tombstones_visible_kv_and_catalog_atomically() {
    let mut database = open_cache_database().expect("cache database opens");
    database
        .spaces(branch("default"))
        .expect("space service opens")
        .create(space("tenant_a"))
        .expect("space create succeeds");
    database
        .kv(branch("default"), space("tenant_a"))
        .expect("tenant KV service opens")
        .put(key(b"alpha"), value(b"one"))
        .expect("kv put succeeds");

    let error = database
        .spaces(branch("default"))
        .expect("space service opens")
        .delete(&space("tenant_a"), false)
        .expect_err("non-empty delete without force fails");
    assert_eq!(error.class(), EngineErrorClass::Conflict);
    assert_eq!(error.code(), "failed_precondition.engine.space_not_empty");
    assert!(database
        .spaces(branch("default"))
        .expect("space service opens")
        .exists(&space("tenant_a"))
        .expect("space exists succeeds"));
    assert_eq!(
        database
            .kv(branch("default"), space("tenant_a"))
            .expect("tenant KV service opens")
            .count(None)
            .expect("count succeeds"),
        1
    );

    let outcome = database
        .spaces(branch("default"))
        .expect("space service opens")
        .delete(&space("tenant_a"), true)
        .expect("forced delete succeeds");
    assert!(outcome.deleted());
    assert!(outcome.force());
    assert!(outcome.deleted_rows() >= 1);
    assert!(outcome.version().is_some());
    assert!(outcome.timestamp().is_some());
    assert!(!database
        .spaces(branch("default"))
        .expect("space service opens")
        .exists(&space("tenant_a"))
        .expect("space exists succeeds"));
    assert_eq!(
        database
            .kv(branch("default"), space("tenant_a"))
            .expect("tenant KV service opens")
            .count(None)
            .expect("count succeeds"),
        0
    );
}

#[cfg(feature = "testkit")]
#[test]
fn space_delete_force_removes_vector_index_manifest_control_rows() {
    let mut database = open_cache_database().expect("cache database opens");
    let tenant = space("tenant_a");
    let docs = VectorCollectionName::new("docs").expect("valid collection name");

    database
        .spaces(branch("default"))
        .expect("space service opens")
        .create(tenant.clone())
        .expect("space create succeeds");
    {
        let mut vectors = database
            .vector(branch("default"), tenant.clone())
            .expect("vector service opens");
        vectors
            .create_collection(
                docs.clone(),
                VectorConfig::new(2, VectorDistanceMetric::Cosine).expect("valid config"),
            )
            .expect("collection create succeeds");
        for (key, embedding) in [
            ("doc-0", [1.0_f32, 0.0]),
            ("doc-1", [2.0_f32, 0.0]),
            ("doc-2", [3.0_f32, 0.0]),
        ] {
            vectors
                .upsert(
                    docs.clone(),
                    VectorKey::new(key).expect("valid vector key"),
                    VectorEmbedding::new(embedding).expect("valid embedding"),
                    None,
                )
                .expect("vector upsert succeeds");
        }
        vectors
            .seed_flat_index_manifest_from_visible_rows_for_test(&docs, "source-a")
            .expect("manifest seed succeeds");
        assert!(vectors
            .index_manifest_byte_len_for_test(&docs)
            .expect("manifest lookup succeeds")
            .is_some());
    }

    database
        .spaces(branch("default"))
        .expect("space service opens")
        .delete(&tenant, true)
        .expect("forced delete succeeds");
    database
        .spaces(branch("default"))
        .expect("space service opens")
        .create(tenant.clone())
        .expect("space recreate succeeds");

    let mut recreated = database
        .vector(branch("default"), tenant)
        .expect("vector service opens after recreate");
    recreated
        .create_collection(
            docs.clone(),
            VectorConfig::new(2, VectorDistanceMetric::Cosine).expect("valid config"),
        )
        .expect("collection recreate succeeds");
    assert_eq!(
        recreated
            .index_manifest_byte_len_for_test(&docs)
            .expect("manifest lookup succeeds"),
        None
    );
}

#[cfg(feature = "localfs")]
#[test]
fn admin_and_spaces_survive_durable_reopen() {
    let temp = tempfile::tempdir().expect("temp dir");
    let path = temp.path().join("db");

    {
        let mut database = open_durable_database(&path).expect("durable database opens");
        database
            .spaces(branch("default"))
            .expect("space service opens")
            .create(space("tenant_a"))
            .expect("space create succeeds");
        let info = database
            .admin()
            .expect("admin service opens")
            .info(Some(&branch("default")))
            .expect("info succeeds");
        assert_eq!(info.target, DatabaseOpenTarget::DurableLocal);
        assert!(info.created);
        assert!(info.durable);
        assert_eq!(info.space_count, 2);
        database.close().expect("durable close succeeds");
    }

    let mut reopened = open_durable_database(&path).expect("durable database reopens");
    let info = reopened
        .admin()
        .expect("admin service opens")
        .info(Some(&branch("default")))
        .expect("info succeeds");
    assert_eq!(info.target, DatabaseOpenTarget::DurableLocal);
    assert!(!info.created);
    assert!(info.durable);
    assert_eq!(info.space_count, 2);
    assert!(reopened
        .spaces(branch("default"))
        .expect("space service opens")
        .exists(&space("tenant_a"))
        .expect("space exists succeeds"));
}
