//! Vector capability branch compare + promote conformance (M12G-vector).

mod common;

use strata_engine::{
    BranchStateSelector, ComparedCapability, ConflictKind, Database, DerivedStateDisposition,
    EngineErrorClass, PromotionStrategy, VectorCollectionName, VectorConfig, VectorDistanceMetric,
    VectorEmbedding, VectorKey,
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
    // Promoting vectors keeps search correct via the query-time fallback, so the
    // derived vector index needs no rebuild (contract §Promotion rule 9).
    assert!(outcome.derived_state().iter().any(|report| {
        report.capability() == ComparedCapability::Vector
            && report.disposition() == DerivedStateDisposition::Current
    }));

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
fn test_promotion_removes_a_collection_deleted_on_source() {
    let mut database = open_cache_database().expect("cache open succeeds");
    // `emb` exists on both branches at the fork (part of the base).
    database
        .vector(branch("default"), space("default"))
        .expect("vector service opens")
        .create_collection(
            collection(),
            VectorConfig::new(2, VectorDistanceMetric::Cosine).expect("valid config"),
        )
        .expect("create collection");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    // The source deletes `emb`; the target independently creates a target-only
    // collection `keep`.
    let deleted = database
        .vector(branch("feature"), space("default"))
        .expect("vector service opens")
        .delete_collection(&collection())
        .expect("delete collection");
    assert!(deleted);
    let keep = VectorCollectionName::new("keep").expect("valid collection");
    database
        .vector(branch("default"), space("default"))
        .expect("vector service opens")
        .create_collection(
            keep.clone(),
            VectorConfig::new(3, VectorDistanceMetric::Cosine).expect("valid config"),
        )
        .expect("create keep collection");

    // Precondition: the target still registers `emb`.
    assert!(database
        .vector(branch("default"), space("default"))
        .expect("vector service opens")
        .collection_info(&collection())
        .expect("info succeeds")
        .is_some());

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

    // `emb` (deleted on source, present in the base) must be removed from the target.
    assert!(
        database
            .vector(branch("default"), space("default"))
            .expect("vector service opens")
            .collection_info(&collection())
            .expect("info succeeds")
            .is_none(),
        "promotion must remove a collection the source deleted"
    );
    // A target-only collection (absent from the base) must survive.
    assert!(
        database
            .vector(branch("default"), space("default"))
            .expect("vector service opens")
            .collection_info(&keep)
            .expect("info succeeds")
            .is_some(),
        "a target-only collection must not be deleted"
    );
}

#[test]
fn test_promotion_keeps_a_deleted_collection_the_target_still_uses() {
    let mut database = open_cache_database().expect("cache open succeeds");
    // `emb` exists on both branches at the fork.
    database
        .vector(branch("default"), space("default"))
        .expect("vector service opens")
        .create_collection(
            collection(),
            VectorConfig::new(2, VectorDistanceMetric::Cosine).expect("valid config"),
        )
        .expect("create collection");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    // The source deletes `emb`; the target writes a NEW (target-only) vector into it.
    let deleted = database
        .vector(branch("feature"), space("default"))
        .expect("vector service opens")
        .delete_collection(&collection())
        .expect("delete collection");
    assert!(deleted);
    upsert(&mut database, "default", "t1", vec![0.0, 5.0]);

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

    // Deregistering `emb` would orphan the target-only vector behind a missing
    // config, so a collection the target still holds a live vector in stays.
    assert!(
        database
            .vector(branch("default"), space("default"))
            .expect("vector service opens")
            .collection_info(&collection())
            .expect("info succeeds")
            .is_some(),
        "a collection the target still holds vectors in must stay registered"
    );
    assert_eq!(
        database
            .vector(branch("default"), space("default"))
            .expect("vector service opens")
            .count(&collection())
            .expect("count succeeds"),
        1,
        "the target-only vector remains readable"
    );
}

#[test]
fn test_promotion_keeps_a_collection_whose_key_the_source_created_and_deleted() {
    let mut database = open_cache_database().expect("cache open succeeds");
    // `emb` exists on both at the fork, empty (the base holds no vectors).
    database
        .vector(branch("default"), space("default"))
        .expect("vector service opens")
        .create_collection(
            collection(),
            VectorConfig::new(2, VectorDistanceMetric::Cosine).expect("valid config"),
        )
        .expect("create collection");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    // The source adds a vector `k`, then deletes the whole collection (tombstoning
    // `k` and the config). Relative to the base this is a net no-op for `k`.
    upsert(&mut database, "feature", "k", vec![1.0, 1.0]);
    database
        .vector(branch("feature"), space("default"))
        .expect("vector service opens")
        .delete_collection(&collection())
        .expect("delete collection");
    // The target independently adds the SAME key `k` (vector keys are branch-
    // independent).
    upsert(&mut database, "default", "k", vec![2.0, 2.0]);

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

    // `k` was never in the base, so the data three-way does not delete it; its
    // collection must NOT be deregistered, or `k` would be orphaned behind a
    // missing config.
    assert!(
        database
            .vector(branch("default"), space("default"))
            .expect("vector service opens")
            .collection_info(&collection())
            .expect("info succeeds")
            .is_some(),
        "a collection with a surviving target vector must stay registered"
    );
    assert_eq!(
        database
            .vector(branch("default"), space("default"))
            .expect("vector service opens")
            .count(&collection())
            .expect("count succeeds"),
        1,
        "the surviving target vector remains readable"
    );
}

#[test]
fn test_promotion_removes_collection_configs_in_a_source_deleted_space() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let with_coll = space("with_coll");
    // A space holding a collection exists on both branches at the fork (the base).
    database
        .spaces(branch("default"))
        .expect("space service opens")
        .create(with_coll.clone())
        .expect("create space");
    database
        .vector(branch("default"), with_coll.clone())
        .expect("vector service opens")
        .create_collection(
            collection(),
            VectorConfig::new(2, VectorDistanceMetric::Cosine).expect("valid config"),
        )
        .expect("create collection");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    // The source deletes the whole space (force: purges its collection config).
    database
        .spaces(branch("feature"))
        .expect("space service opens")
        .delete(&with_coll, true)
        .expect("delete space");

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
    // The space is deregistered on the target.
    assert!(!database
        .spaces(branch("default"))
        .expect("space service opens")
        .exists(&with_coll)
        .expect("exists succeeds"));

    // Re-creating the space must NOT resurface the stale collection config the
    // source deleted — the deregistered space's collection metadata must be gone.
    database
        .spaces(branch("default"))
        .expect("space service opens")
        .create(with_coll.clone())
        .expect("re-create space");
    assert!(
        database
            .vector(branch("default"), with_coll.clone())
            .expect("vector service opens")
            .collection_info(&collection())
            .expect("info succeeds")
            .is_none(),
        "a collection config in a source-deleted space must not linger and resurface"
    );
}

// A collection the source deleted while the target independently reshaped it
// (delete + recreate with a different config) is a modify/delete divergence.
fn reshaped_deletion_database() -> Database {
    let mut database = open_cache_database().expect("cache open succeeds");
    database
        .vector(branch("default"), space("default"))
        .expect("vector service opens")
        .create_collection(
            collection(),
            VectorConfig::new(2, VectorDistanceMetric::Cosine).expect("valid config"),
        )
        .expect("create collection");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    // Source deletes `emb`.
    database
        .vector(branch("feature"), space("default"))
        .expect("vector service opens")
        .delete_collection(&collection())
        .expect("delete collection");
    // Target deletes and recreates `emb` with a different (incompatible) config.
    database
        .vector(branch("default"), space("default"))
        .expect("vector service opens")
        .delete_collection(&collection())
        .expect("delete collection");
    database
        .vector(branch("default"), space("default"))
        .expect("vector service opens")
        .create_collection(
            collection(),
            VectorConfig::new(3, VectorDistanceMetric::Cosine).expect("valid config"),
        )
        .expect("recreate collection");
    database
}

#[test]
fn test_strict_refuses_when_source_deletes_a_collection_the_target_reshaped() {
    let mut database = reshaped_deletion_database();
    let error = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect_err("a modify/delete divergence refuses under Strict");
    assert_eq!(error.class(), EngineErrorClass::Conflict);
    assert_eq!(error.code(), "conflict.engine.promotion");
    // The target is untouched — `emb` keeps the target's reshaped config.
    let info = database
        .vector(branch("default"), space("default"))
        .expect("vector service opens")
        .collection_info(&collection())
        .expect("info succeeds")
        .expect("target collection retained on strict refusal");
    assert_eq!(info.config().dimension(), 3);
}

#[test]
fn test_source_wins_removes_a_collection_the_target_reshaped() {
    let mut database = reshaped_deletion_database();
    let outcome = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::SourceWins,
        )
        .expect("source-wins promote applies the deletion");
    assert!(!outcome.conflicts().is_empty());
    // Source wins: the collection the source deleted is removed from the target.
    assert!(
        database
            .vector(branch("default"), space("default"))
            .expect("vector service opens")
            .collection_info(&collection())
            .expect("info succeeds")
            .is_none(),
        "SourceWins applies the source-side collection deletion"
    );
}

#[test]
fn test_preview_reports_an_incompatible_collection_conflict() {
    let mut database = open_cache_database().expect("cache open succeeds");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    // Both branches independently create the same collection with incompatible dims.
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

    // Preview must surface the same conflict promote refuses on — not report clean.
    let preview = database
        .branches()
        .expect("branch service opens")
        .preview(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect("preview succeeds");
    assert!(
        preview
            .conflicts()
            .iter()
            .any(|conflict| conflict.kind() == ConflictKind::IncompatibleCollection),
        "preview must report the incompatible-collection conflict that promote refuses on"
    );

    // Parity: promote does refuse on exactly this conflict.
    let error = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect_err("incompatible collection config refuses");
    assert_eq!(error.code(), "conflict.engine.promotion");
}

#[test]
fn test_preview_of_a_compatible_source_collection_reports_no_conflict() {
    let mut database = open_cache_database().expect("cache open succeeds");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    // A source-only collection is carried cleanly — not a conflict.
    database
        .vector(branch("feature"), space("default"))
        .expect("vector service opens")
        .create_collection(
            collection(),
            VectorConfig::new(2, VectorDistanceMetric::Cosine).expect("valid config"),
        )
        .expect("create feature collection");

    let preview = database
        .branches()
        .expect("branch service opens")
        .preview(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect("preview succeeds");
    assert!(
        !preview
            .conflicts()
            .iter()
            .any(|conflict| conflict.kind() == ConflictKind::IncompatibleCollection),
        "a compatible source-only collection must not preview as a conflict"
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
fn test_source_wins_refuses_incompatible_collection() {
    let mut database = open_cache_database().expect("cache open succeeds");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    // Both branches independently create the same collection with incompatible
    // dimensions — structurally unmergeable.
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

    // Even SourceWins must refuse: incompatible shapes cannot be merged, and
    // forcing the source config would leave the target mixing dimensions.
    let error = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::SourceWins,
        )
        .expect_err("source-wins must refuse an incompatible collection");
    assert_eq!(error.class(), EngineErrorClass::Conflict);
    assert_eq!(error.code(), "conflict.engine.promotion");

    // Zero mutation on the structural refusal: the target keeps its own config.
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
        "the target's collection is untouched by the refused source-wins promotion"
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
