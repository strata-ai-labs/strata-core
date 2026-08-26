//! Branch promotion (merge) conformance tests (M12D1, conformance #6–8).

mod common;

use strata_engine::{
    ComparedCapability, ConflictKind, ConflictStrategyResult, Database, EngineErrorClass,
    PromotionOutcome, PromotionStrategy,
};

use common::{assert_branch_value, branch, key, open_cache_database, space, value};

fn sorted(mut ids: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    ids.sort();
    ids
}

fn applied(outcome: &PromotionOutcome) -> Vec<Vec<u8>> {
    sorted(
        outcome
            .applied()
            .iter()
            .map(|entity| entity.identity().to_vec())
            .collect(),
    )
}

fn deleted(outcome: &PromotionOutcome) -> Vec<Vec<u8>> {
    sorted(
        outcome
            .deleted()
            .iter()
            .map(|entity| entity.identity().to_vec())
            .collect(),
    )
}

fn conflicts(outcome: &PromotionOutcome) -> Vec<Vec<u8>> {
    sorted(
        outcome
            .conflicts()
            .iter()
            .map(|conflict| conflict.identity().to_vec())
            .collect(),
    )
}

fn assert_absent(database: &mut Database, branch_name: &str, space_name: &str, key_bytes: &[u8]) {
    let mut kv = database
        .kv(branch(branch_name), space(space_name))
        .expect("KV opens");
    assert!(
        kv.get(&key(key_bytes)).expect("KV read succeeds").is_none(),
        "expected `{}` to be absent",
        String::from_utf8_lossy(key_bytes)
    );
}

/// Seeds `default` with `shared`/`md`, forks `feature`, then diverges both
/// branches on `shared` (different values) and `md` (feature deletes, default
/// modifies), each adding a private key. This is the canonical conflict setup.
fn diverged_database() -> Database {
    let mut database = open_cache_database().expect("cache open succeeds");
    {
        let mut kv = database
            .kv(branch("default"), space("default"))
            .expect("KV opens");
        kv.put(key(b"shared"), value(b"base")).expect("shared base");
        kv.put(key(b"md"), value(b"one")).expect("md base");
    }
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    {
        let mut kv = database
            .kv(branch("feature"), space("default"))
            .expect("KV opens");
        kv.put(key(b"shared"), value(b"feature_change"))
            .expect("feature shared");
        kv.delete(key(b"md")).expect("feature deletes md");
        kv.put(key(b"new_feature"), value(b"y"))
            .expect("feature add");
    }
    {
        let mut kv = database
            .kv(branch("default"), space("default"))
            .expect("KV opens");
        kv.put(key(b"shared"), value(b"default_change"))
            .expect("default shared");
        kv.put(key(b"md"), value(b"two")).expect("default md");
        kv.put(key(b"new_default"), value(b"x"))
            .expect("default add");
    }
    database
}

#[test]
fn promote_source_wins_applies_source_changes_and_reports_them() {
    let mut database = diverged_database();

    let outcome = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::SourceWins,
        )
        .expect("source-wins promote succeeds");

    assert_eq!(outcome.source().as_str(), "feature");
    assert_eq!(outcome.target().as_str(), "default");
    assert_eq!(outcome.strategy(), PromotionStrategy::SourceWins);
    assert!(!outcome.is_noop());
    assert!(outcome.target_version().is_some());

    // The branch point is the source's recorded fork version.
    let fork_version = database
        .branches()
        .expect("branch service opens")
        .get(&branch("feature"))
        .expect("feature summary")
        .parent()
        .expect("feature was forked")
        .fork_version();
    assert_eq!(outcome.branch_point(), fork_version);

    // shared (conflict, present source) and new_feature (clean) are applied;
    // md (conflict, source deleted) is deleted; both conflicts are reported.
    assert_eq!(
        applied(&outcome),
        vec![b"new_feature".to_vec(), b"shared".to_vec()]
    );
    assert_eq!(deleted(&outcome), vec![b"md".to_vec()]);
    assert_eq!(
        conflicts(&outcome),
        vec![b"md".to_vec(), b"shared".to_vec()]
    );

    let shared = outcome
        .conflicts()
        .iter()
        .find(|conflict| conflict.identity() == b"shared")
        .expect("shared conflict present");
    assert_eq!(shared.kind(), ConflictKind::ValueDivergence);
    assert_eq!(shared.strategy_result(), ConflictStrategyResult::SourceWins);
    assert_eq!(shared.source_value(), Some(&b"feature_change"[..]));
    assert_eq!(shared.target_value(), Some(&b"default_change"[..]));

    let md = outcome
        .conflicts()
        .iter()
        .find(|conflict| conflict.identity() == b"md")
        .expect("md conflict present");
    assert_eq!(md.kind(), ConflictKind::ModifyDeleteDivergence);
    assert_eq!(md.source_value(), None);

    // The applied entity carries the source value, capability, and space; the
    // deleted entity carries no value.
    let shared_applied = outcome
        .applied()
        .iter()
        .find(|entity| entity.identity() == b"shared")
        .expect("shared applied");
    assert_eq!(shared_applied.capability(), ComparedCapability::KeyValue);
    assert_eq!(shared_applied.space().as_str(), "default");
    assert_eq!(shared_applied.value(), Some(&b"feature_change"[..]));
    let md_deleted = outcome
        .deleted()
        .iter()
        .find(|entity| entity.identity() == b"md")
        .expect("md deleted");
    assert_eq!(md_deleted.value(), None);

    // Target now reflects the source-wins resolution.
    assert_branch_value(
        &mut database,
        "default",
        "default",
        b"shared",
        b"feature_change",
    );
    assert_branch_value(&mut database, "default", "default", b"new_feature", b"y");
    // Target-only change is preserved; source deletion is propagated.
    assert_branch_value(&mut database, "default", "default", b"new_default", b"x");
    assert_absent(&mut database, "default", "default", b"md");

    // Source branch is never modified.
    assert_branch_value(
        &mut database,
        "feature",
        "default",
        b"shared",
        b"feature_change",
    );
    assert_absent(&mut database, "feature", "default", b"new_default");
}

#[test]
fn promote_records_authoritative_merge_lineage_on_the_target() {
    let mut database = open_cache_database().expect("cache open succeeds");
    {
        let mut kv = database
            .kv(branch("default"), space("default"))
            .expect("KV opens");
        kv.put(key(b"k"), value(b"base")).expect("base");
    }
    // Recreate `feature` so its generation is 2, not 1 — the lineage edge must
    // record the source's actual generation, not a constant.
    {
        let mut branches = database.branches().expect("branch service opens");
        branches
            .fork_current(&branch("default"), branch("feature"))
            .expect("first fork (generation 1)");
        branches.delete(&branch("feature")).expect("delete feature");
        branches
            .fork_current(&branch("default"), branch("feature"))
            .expect("second fork (generation 2)");
    }
    {
        let mut kv = database
            .kv(branch("feature"), space("default"))
            .expect("KV opens");
        kv.put(key(b"k"), value(b"changed"))
            .expect("feature change");
    }

    let outcome = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::SourceWins,
        )
        .expect("promote succeeds");
    let target_version = outcome.target_version().expect("promotion wrote a commit");

    let branches = database.branches().expect("branch service opens");
    let source = branches.get(&branch("feature")).expect("feature summary");
    let source_branch_id = source.branch_id();
    assert_eq!(source.generation(), 2);
    let summary = branches.get(&branch("default")).expect("target summary");
    let merge = summary
        .merge_parent()
        .expect("promotion recorded a merge edge");
    assert_eq!(merge.source_name().as_str(), "feature");
    assert_eq!(merge.source_branch_id(), source_branch_id);
    assert_eq!(merge.source_generation(), 2);
    assert_eq!(merge.merged_at(), target_version);
    assert!(merge.merged_timestamp().is_some());
}

#[test]
fn promote_strict_refuses_on_conflict_with_zero_target_mutation() {
    let mut database = diverged_database();

    let error = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect_err("strict promotion refuses a conflict");
    assert_eq!(error.class(), EngineErrorClass::Conflict);
    assert_eq!(error.code(), "conflict.engine.promotion");

    // The target is untouched: no partial application, no merge edge.
    assert_branch_value(
        &mut database,
        "default",
        "default",
        b"shared",
        b"default_change",
    );
    assert_branch_value(&mut database, "default", "default", b"md", b"two");
    assert_absent(&mut database, "default", "default", b"new_feature");
    let summary = database
        .branches()
        .expect("branch service opens")
        .get(&branch("default"))
        .expect("target summary");
    assert!(summary.merge_parent().is_none());
}

#[test]
fn promote_of_a_one_sided_change_applies_cleanly_under_strict() {
    let mut database = open_cache_database().expect("cache open succeeds");
    {
        let mut kv = database
            .kv(branch("default"), space("default"))
            .expect("KV opens");
        kv.put(key(b"k"), value(b"base")).expect("base");
    }
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    {
        let mut kv = database
            .kv(branch("feature"), space("default"))
            .expect("KV opens");
        kv.put(key(b"k"), value(b"changed"))
            .expect("feature change");
    }

    let outcome = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect("clean promote succeeds under strict");
    assert!(outcome.conflicts().is_empty());
    assert_eq!(applied(&outcome), vec![b"k".to_vec()]);
    assert!(outcome.deleted().is_empty());
    assert!(outcome.target_version().is_some());

    assert_branch_value(&mut database, "default", "default", b"k", b"changed");
}

#[test]
fn promote_with_no_changes_is_a_noop_and_writes_no_commit() {
    let mut database = open_cache_database().expect("cache open succeeds");
    {
        let mut kv = database
            .kv(branch("default"), space("default"))
            .expect("KV opens");
        kv.put(key(b"k"), value(b"base")).expect("base");
    }
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");

    let outcome = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect("no-op promote succeeds");
    assert!(outcome.is_noop());
    assert!(outcome.target_version().is_none());
    assert!(outcome.applied().is_empty());
    assert!(outcome.deleted().is_empty());
    assert!(outcome.conflicts().is_empty());

    // No merge edge is recorded for a no-op promotion.
    let summary = database
        .branches()
        .expect("branch service opens")
        .get(&branch("default"))
        .expect("target summary");
    assert!(summary.merge_parent().is_none());
}

#[test]
fn promote_of_unrelated_branches_is_rejected() {
    let mut database = open_cache_database().expect("cache open succeeds");
    database
        .branches()
        .expect("branch service opens")
        .create(branch("other"))
        .expect("empty root branch");

    let error = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("default"),
            &branch("other"),
            PromotionStrategy::Strict,
        )
        .expect_err("unrelated branches have no branch point");
    assert_eq!(error.class(), EngineErrorClass::InvalidInput);
    assert_eq!(error.code(), "invalid_argument.engine.branch_point");
}

#[test]
fn test_repeated_promotion_incorporates_prior_merge() {
    let mut database = open_cache_database().expect("cache open succeeds");
    {
        let mut kv = database
            .kv(branch("default"), space("default"))
            .expect("KV opens");
        kv.put(key(b"k"), value(b"base")).expect("default base");
    }
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    {
        let mut kv = database
            .kv(branch("feature"), space("default"))
            .expect("KV opens");
        kv.put(key(b"k"), value(b"v1")).expect("feature v1");
    }

    // First promotion incorporates feature@v1 into default.
    let first = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect("first strict promote succeeds");
    assert!(first.conflicts().is_empty());
    let first_version = first.target_version().expect("first promote committed");
    assert_branch_value(&mut database, "default", "default", b"k", b"v1");

    // Feature advances past the merge.
    {
        let mut kv = database
            .kv(branch("feature"), space("default"))
            .expect("KV opens");
        kv.put(key(b"k"), value(b"v2")).expect("feature v2");
    }

    // The second promotion must diff against the recorded merge edge, not the
    // original fork point: default's k=v1 was itself incorporated from feature,
    // so it is not a conflicting target-side change.
    let second = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect("second strict promote must not false-conflict");
    assert!(
        second.conflicts().is_empty(),
        "repeated promotion must not conflict on already-merged changes"
    );
    // The branch point advanced to the first merge's target commit.
    assert_eq!(second.branch_point(), first_version);
    assert_branch_value(&mut database, "default", "default", b"k", b"v2");
}

#[test]
fn test_prior_merge_from_another_source_does_not_shift_the_branch_point() {
    let mut database = open_cache_database().expect("cache open succeeds");
    {
        let mut kv = database
            .kv(branch("default"), space("default"))
            .expect("KV opens");
        kv.put(key(b"k"), value(b"base")).expect("default base");
    }
    // Two features fork from the same base state (default@k=base).
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature_a"))
        .expect("fork a succeeds");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature_b"))
        .expect("fork b succeeds");
    {
        let mut kv = database
            .kv(branch("feature_a"), space("default"))
            .expect("KV opens");
        kv.put(key(b"k"), value(b"a1")).expect("feature_a a1");
    }
    {
        let mut kv = database
            .kv(branch("feature_b"), space("default"))
            .expect("KV opens");
        kv.put(key(b"k"), value(b"b1")).expect("feature_b b1");
    }

    // Promote feature_a: default now holds a1, with a merge edge from feature_a.
    database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature_a"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect("promote a succeeds");
    assert_branch_value(&mut database, "default", "default", b"k", b"a1");

    // Promoting feature_b must use feature_b's OWN fork base (k=base), not the
    // merge edge recorded for feature_a. feature_b changed base->b1 while default
    // changed base->a1, so this is a genuine conflict — not a clean re-apply.
    let error = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature_b"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect_err("a merge edge from another source is a genuine conflict");
    assert_eq!(error.class(), EngineErrorClass::Conflict);
    assert_eq!(error.code(), "conflict.engine.promotion");
}

#[test]
fn test_recreated_source_after_merge_uses_its_own_fork_base() {
    let mut database = open_cache_database().expect("cache open succeeds");
    {
        let mut kv = database
            .kv(branch("default"), space("default"))
            .expect("KV opens");
        kv.put(key(b"k"), value(b"base")).expect("default base");
    }
    // First incarnation of `feature`: fork, diverge, promote into default.
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork gen1 succeeds");
    {
        let mut kv = database
            .kv(branch("feature"), space("default"))
            .expect("KV opens");
        kv.put(key(b"k"), value(b"v1")).expect("feature v1");
    }
    database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect("promote gen1 succeeds");
    // default now holds v1, with a merge edge recorded from feature@gen1.

    // default advances locally AFTER the merge, so the stale merge state (v1) and
    // the state the re-fork will inherit (default_after) genuinely differ.
    {
        let mut kv = database
            .kv(branch("default"), space("default"))
            .expect("KV opens");
        kv.put(key(b"k"), value(b"default_after"))
            .expect("default advances after merge");
    }

    // Delete `feature` and re-fork it — a new generation reusing the same name
    // (and thus the same branch id) — from default's new head.
    database
        .branches()
        .expect("branch service opens")
        .delete(&branch("feature"))
        .expect("delete gen1 succeeds");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("re-fork gen2 succeeds");
    {
        let mut kv = database
            .kv(branch("feature"), space("default"))
            .expect("KV opens");
        kv.put(key(b"k"), value(b"w1")).expect("feature gen2 w1");
    }

    // Promoting the re-forked feature must use ITS OWN fork base (default_after),
    // not the stale merge edge from the deleted gen1 incarnation. Against its own
    // fork base, default is unchanged, so this is a clean apply — matching the
    // stale merge state (v1) instead would falsely conflict on default_after.
    let outcome = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect("re-forked feature promotes cleanly against its own fork base");
    assert!(
        outcome.conflicts().is_empty(),
        "a re-forked source must diff against its own fork, not a stale merge edge"
    );
    assert_branch_value(&mut database, "default", "default", b"k", b"w1");
}
