//! Vector capability branch compare + promote conformance (M12G-vector).

mod common;

use strata_engine::{
    BranchStateSelector, ComparedCapability, Database, EngineErrorClass, PromotionStrategy,
    VectorCollectionName, VectorConfig, VectorDistanceMetric, VectorEmbedding, VectorKey,
};

use common::{branch, open_cache_database, space};

fn collection() -> VectorCollectionName {
    VectorCollectionName::new("emb").expect("valid collection")
}

fn upsert(database: &mut Database, branch_name: &str, key: &str, embedding: Vec<f32>) {
    database
        .vector(branch(branch_name), space("default"))
        .expect("vector service opens")
        .upsert(
            collection(),
            VectorKey::new(key).expect("key"),
            VectorEmbedding::new(embedding).expect("embedding"),
            None,
        )
        .expect("upsert");
}

#[test]
fn vector_compare_and_promote_across_a_fork() {
    let mut database = open_cache_database().expect("cache open succeeds");

    // Seed `default` with two vectors, fork `feature`, then on `feature` change
    // one vector and add another (target left unchanged since the fork).
    database
        .vector(branch("default"), space("default"))
        .expect("vector service opens")
        .create_collection(
            collection(),
            VectorConfig::new(2, VectorDistanceMetric::Cosine).expect("valid config"),
        )
        .expect("create collection");
    upsert(&mut database, "default", "v1", vec![0.0, 1.0]);
    upsert(&mut database, "default", "v2", vec![0.0, 2.0]);
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    upsert(&mut database, "feature", "v1", vec![9.0, 9.0]);
    upsert(&mut database, "feature", "v3", vec![3.0, 3.0]);

    // Compare default → feature: the vector capability reports v1 modified and
    // v3 added; v2 is unchanged and absent from the diff.
    let comparison = database
        .branches()
        .expect("branch service opens")
        .compare(
            &branch("default"),
            &branch("feature"),
            BranchStateSelector::Current,
        )
        .expect("compare succeeds");
    let vector_space = comparison
        .comparisons()
        .iter()
        .find(|space| space.capability() == ComparedCapability::Vector)
        .expect("a vector comparison is present");
    assert_eq!(vector_space.added().len(), 1, "v3 is added on feature");
    assert_eq!(vector_space.modified().len(), 1, "v1 diverged");
    assert!(vector_space.removed().is_empty());

    // Promote feature → default (strict, no conflict): applies v1 and v3.
    let outcome = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect("strict promote succeeds");
    assert!(outcome.target_version().is_some());
    assert_eq!(outcome.applied().len(), 2, "v1 and v3 applied");
    assert!(
        outcome
            .applied()
            .iter()
            .all(|entity| entity.capability() == ComparedCapability::Vector),
        "the promoted entities are vectors",
    );

    // After the promote the two branches agree: no vector differences remain.
    let after = database
        .branches()
        .expect("branch service opens")
        .compare(
            &branch("default"),
            &branch("feature"),
            BranchStateSelector::Current,
        )
        .expect("compare after promote");
    let residual = after
        .comparisons()
        .iter()
        .find(|space| space.capability() == ComparedCapability::Vector);
    assert!(
        residual.is_none_or(|space| {
            space.added().is_empty() && space.removed().is_empty() && space.modified().is_empty()
        }),
        "vectors are in sync after promote",
    );
}

#[test]
fn test_promotion_carries_source_created_collection_config() {
    let mut database = open_cache_database().expect("cache open succeeds");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    // feature creates a brand-new collection (absent from default) and fills it.
    database
        .vector(branch("feature"), space("default"))
        .expect("vector service opens")
        .create_collection(
            collection(),
            VectorConfig::new(2, VectorDistanceMetric::Cosine).expect("valid config"),
        )
        .expect("create collection");
    upsert(&mut database, "feature", "v1", vec![0.0, 1.0]);

    // Precondition: the target has no such collection.
    assert!(database
        .vector(branch("default"), space("default"))
        .expect("vector service opens")
        .collection_info(&collection())
        .expect("info succeeds")
        .is_none());

    let outcome = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect("strict promote succeeds");
    assert!(outcome.conflicts().is_empty());

    // The collection config must be carried so the collection is usable on the
    // target, rather than the promoted vectors being orphaned behind a missing
    // config (reads would fail not_found.engine.vector_collection).
    let info = database
        .vector(branch("default"), space("default"))
        .expect("vector service opens")
        .collection_info(&collection())
        .expect("info succeeds")
        .expect("source-created collection must be registered on the target");
    assert_eq!(info.config().dimension(), 2);
    assert_eq!(info.config().metric(), VectorDistanceMetric::Cosine);
    // And the promoted vector is readable through the now-usable collection.
    assert_eq!(
        database
            .vector(branch("default"), space("default"))
            .expect("vector service opens")
            .count(&collection())
            .expect("count succeeds"),
        1,
        "the promoted vector is visible through the carried collection"
    );
}

#[test]
fn test_promotion_conflicts_on_incompatible_collection_config() {
    let mut database = open_cache_database().expect("cache open succeeds");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    // Both branches independently create the same-named collection with an
    // incompatible dimension — a structural conflict.
    database
        .vector(branch("default"), space("default"))
        .expect("vector service opens")
        .create_collection(
            collection(),
            VectorConfig::new(2, VectorDistanceMetric::Cosine).expect("valid config"),
        )
        .expect("create default collection");
    database
        .vector(branch("feature"), space("default"))
        .expect("vector service opens")
        .create_collection(
            collection(),
            VectorConfig::new(4, VectorDistanceMetric::Cosine).expect("valid config"),
        )
        .expect("create feature collection");

    let error = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect_err("incompatible collection config must conflict under strict");
    assert_eq!(error.class(), EngineErrorClass::Conflict);
    assert_eq!(error.code(), "conflict.engine.promotion");

    // Strict refused with zero target mutation: default keeps its own config.
    assert_eq!(
        database
            .vector(branch("default"), space("default"))
            .expect("vector service opens")
            .collection_info(&collection())
            .expect("info succeeds")
            .expect("default collection still present")
            .config()
            .dimension(),
        2,
        "the target's collection config is untouched by a refused promotion"
    );
}

#[test]
fn test_promotion_carries_collection_in_a_source_only_space() {
    let mut database = open_cache_database().expect("cache open succeeds");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    // feature creates a collection inside a brand-new space, then fills it — the
    // realistic "new namespace + new collection on a branch" flow. The space is
    // carried by the space-registration path and the collection by this one.
    database
        .spaces(branch("feature"))
        .expect("space service opens")
        .create(space("extra"))
        .expect("space create succeeds");
    database
        .vector(branch("feature"), space("extra"))
        .expect("vector service opens")
        .create_collection(
            collection(),
            VectorConfig::new(3, VectorDistanceMetric::Euclidean).expect("valid config"),
        )
        .expect("create collection");
    database
        .vector(branch("feature"), space("extra"))
        .expect("vector service opens")
        .upsert(
            collection(),
            VectorKey::new("v1").expect("key"),
            VectorEmbedding::new(vec![1.0, 2.0, 3.0]).expect("embedding"),
            None,
        )
        .expect("upsert");

    let outcome = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect("strict promote succeeds");
    assert!(outcome.conflicts().is_empty());

    // The new space is registered AND the collection in it is usable on target.
    let info = database
        .vector(branch("default"), space("extra"))
        .expect("vector service opens")
        .collection_info(&collection())
        .expect("info succeeds")
        .expect("collection carried into the newly-registered space");
    assert_eq!(info.config().dimension(), 3);
    assert_eq!(info.config().metric(), VectorDistanceMetric::Euclidean);
    assert_eq!(
        database
            .vector(branch("default"), space("extra"))
            .expect("vector service opens")
            .count(&collection())
            .expect("count succeeds"),
        1
    );
}
