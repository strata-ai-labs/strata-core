//! Vector capability branch compare + promote conformance (M12G-vector).

mod common;

use strata_engine::{
    BranchStateSelector, ComparedCapability, Database, PromotionStrategy, VectorCollectionName,
    VectorConfig, VectorDistanceMetric, VectorEmbedding, VectorKey,
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
