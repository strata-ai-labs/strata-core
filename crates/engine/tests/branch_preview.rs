//! Branch preview-promotion conformance tests (M12C1, conformance #5).

mod common;

use serde_json::json;
use strata_engine::{
    BranchPreview, ComparedCapability, ConflictKind, ConflictStrategyResult,
    DerivedStateDisposition, JsonDocumentId, JsonPath, JsonValue, PreviewConflict,
    PromotionStrategy,
};

use common::{assert_branch_value, branch, key, open_cache_database, space, value};

#[test]
fn test_preview_reports_capability_coverage_spaces_and_derived_state() {
    let mut database = open_cache_database().expect("cache open succeeds");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    // Source change: feature adds a JSON document.
    database
        .json(branch("feature"), space("default"))
        .expect("json opens")
        .set_or_create(
            JsonDocumentId::new("doc").expect("id"),
            &JsonPath::root(),
            JsonValue::new(json!({"a": 1})).expect("value"),
        )
        .expect("json set");
    // Target-only space: default registers a space feature does not have, so the
    // reported spaces must be the union of both sides — not just the source's.
    database
        .spaces(branch("default"))
        .expect("space service opens")
        .create(space("target_only"))
        .expect("space create");

    let preview = database
        .branches()
        .expect("branch service opens")
        .preview(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect("preview succeeds");

    // Rule 4: the preview reports which capabilities a promotion covers and which
    // it does not.
    assert!(preview
        .capabilities_covered()
        .contains(&ComparedCapability::KeyValue));
    assert!(preview
        .capabilities_unsupported()
        .contains(&ComparedCapability::GraphNode));
    // The reported spaces are the source ∪ target union, including the
    // target-only space.
    assert!(preview
        .spaces_covered()
        .iter()
        .any(|space| space.as_str() == "target_only"));
    // Rule 5: JSON is the source-changed capability, so a promotion would leave
    // its secondary index rebuild-required — and it is the only report.
    assert_eq!(preview.derived_state().len(), 1);
    assert_eq!(
        preview.derived_state()[0].capability(),
        ComparedCapability::Json
    );
    assert_eq!(
        preview.derived_state()[0].disposition(),
        DerivedStateDisposition::RebuildRequired
    );
}

fn conflict_for<'a>(preview: &'a BranchPreview, identity: &[u8]) -> Option<&'a PreviewConflict> {
    preview
        .conflicts()
        .iter()
        .find(|conflict| conflict.identity() == identity)
}

#[test]
fn preview_reports_conflicts_and_mutates_neither_branch() {
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
    // default: modifies shared and md, adds a key of its own.
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
    // feature: modifies shared differently, deletes md, adds a key of its own.
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

    let preview = database
        .branches()
        .expect("branch service opens")
        .preview(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect("preview succeeds");

    assert_eq!(preview.source().as_str(), "feature");
    assert_eq!(preview.target().as_str(), "default");
    assert!(!preview.is_clean());
    assert_eq!(preview.conflicts().len(), 2);

    let shared = conflict_for(&preview, b"shared").expect("shared conflict present");
    assert_eq!(shared.kind(), ConflictKind::ValueDivergence);
    assert_eq!(shared.source_value(), Some(&b"feature_change"[..]));
    assert_eq!(shared.target_value(), Some(&b"default_change"[..]));
    assert_eq!(shared.strategy_result(), ConflictStrategyResult::Refused);

    let md = conflict_for(&preview, b"md").expect("md conflict present");
    assert_eq!(md.kind(), ConflictKind::ModifyDeleteDivergence);
    assert_eq!(md.source_value(), None); // feature deleted it
    assert_eq!(md.target_value(), Some(&b"two"[..]));

    // One-sided changes are not conflicts.
    assert!(conflict_for(&preview, b"new_default").is_none());
    assert!(conflict_for(&preview, b"new_feature").is_none());

    // Preview is read-only: neither branch changed.
    assert_branch_value(
        &mut database,
        "default",
        "default",
        b"shared",
        b"default_change",
    );
    assert_branch_value(
        &mut database,
        "feature",
        "default",
        b"shared",
        b"feature_change",
    );
    assert_branch_value(&mut database, "default", "default", b"md", b"two");
}

#[test]
fn preview_of_a_one_sided_change_is_clean() {
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

    let preview = database
        .branches()
        .expect("branch service opens")
        .preview(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect("preview succeeds");
    assert!(preview.is_clean());
    assert!(preview.conflicts().is_empty());
}

#[test]
fn preview_of_unrelated_branches_is_rejected() {
    let mut database = open_cache_database().expect("cache open succeeds");
    database
        .branches()
        .expect("branch service opens")
        .create(branch("other"))
        .expect("empty root branch");

    let error = database
        .branches()
        .expect("branch service opens")
        .preview(
            &branch("default"),
            &branch("other"),
            PromotionStrategy::Strict,
        )
        .expect_err("unrelated branches have no branch point");
    assert_eq!(error.code(), "invalid_argument.engine.branch_point");
}

#[test]
fn preview_source_wins_reports_the_resolution_and_resolves_lineage_both_ways() {
    let mut database = open_cache_database().expect("cache open succeeds");
    {
        let mut kv = database
            .kv(branch("default"), space("default"))
            .expect("KV opens");
        kv.put(key(b"shared"), value(b"base")).expect("base");
    }
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    {
        let mut kv = database
            .kv(branch("default"), space("default"))
            .expect("KV opens");
        kv.put(key(b"shared"), value(b"default_change"))
            .expect("default change");
    }
    {
        let mut kv = database
            .kv(branch("feature"), space("default"))
            .expect("KV opens");
        kv.put(key(b"shared"), value(b"feature_change"))
            .expect("feature change");
    }

    // source = default, target = feature: the lineage edge is target.parent == source.
    let preview = database
        .branches()
        .expect("branch service opens")
        .preview(
            &branch("default"),
            &branch("feature"),
            PromotionStrategy::SourceWins,
        )
        .expect("preview succeeds");
    assert_eq!(preview.source().as_str(), "default");
    let shared = conflict_for(&preview, b"shared").expect("shared conflict present");
    assert_eq!(shared.strategy_result(), ConflictStrategyResult::SourceWins);
    assert_eq!(shared.source_value(), Some(&b"default_change"[..]));
    assert_eq!(shared.target_value(), Some(&b"feature_change"[..]));
}

#[test]
fn preview_rejects_a_stale_lineage_edge() {
    let mut database = open_cache_database().expect("cache open succeeds");
    database
        .branches()
        .expect("branch service opens")
        .create(branch("origin"))
        .expect("origin branch");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("origin"), branch("feature"))
        .expect("fork succeeds");
    // Delete and recreate the parent: same name and id, a newer generation, so
    // the fork edge recorded on `feature` no longer matches the live parent.
    database
        .branches()
        .expect("branch service opens")
        .delete(&branch("origin"))
        .expect("delete origin");
    database
        .branches()
        .expect("branch service opens")
        .create(branch("origin"))
        .expect("recreate origin");

    // Both directions must reject: the generation guard, not just the branch id,
    // decides whether the lineage edge is intact.
    let forward = database
        .branches()
        .expect("branch service opens")
        .preview(
            &branch("origin"),
            &branch("feature"),
            PromotionStrategy::Strict,
        )
        .expect_err("stale edge (origin as source) is rejected");
    assert_eq!(forward.code(), "invalid_argument.engine.branch_point");

    let reverse = database
        .branches()
        .expect("branch service opens")
        .preview(
            &branch("feature"),
            &branch("origin"),
            PromotionStrategy::Strict,
        )
        .expect_err("stale edge (origin as target) is rejected");
    assert_eq!(reverse.code(), "invalid_argument.engine.branch_point");
}
