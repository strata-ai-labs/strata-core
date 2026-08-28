//! Event capability branch compare (diff-only) + promotion exclusion (M12G-event).

mod common;

use serde_json::json;
use strata_engine::{
    BranchComparison, BranchStateSelector, ComparedCapability, Database, EventPayload, EventType,
    PromotionStrategy, SpaceComparison,
};

use common::{branch, key, open_cache_database, space, value};

fn append_event(database: &mut Database, branch_name: &str, tool: &str) {
    database
        .event(branch(branch_name), space("default"))
        .expect("event service opens")
        .append(
            EventType::new("tool_call").expect("event type"),
            EventPayload::new(json!({ "tool": tool })).expect("payload"),
        )
        .expect("event append");
}

fn event_diff(comparison: &BranchComparison) -> Option<&SpaceComparison> {
    comparison
        .comparisons()
        .iter()
        .find(|space| space.capability() == ComparedCapability::Event)
}

#[test]
fn test_promotion_keeps_a_deleted_space_with_target_only_event_rows() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let evspace = space("events");
    // The `events` space exists on both branches at the fork (part of the base).
    database
        .spaces(branch("default"))
        .expect("space service opens")
        .create(evspace.clone())
        .expect("create space");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    // The source deletes the space; the target appends a (target-only) event into
    // it — event rows are not promotable, so the space-deletion retain guard must
    // still see them.
    database
        .spaces(branch("feature"))
        .expect("space service opens")
        .delete(&evspace, true)
        .expect("delete space");
    database
        .event(branch("default"), evspace.clone())
        .expect("event service opens")
        .append(
            EventType::new("tool_call").expect("event type"),
            EventPayload::new(json!({ "tool": "x" })).expect("payload"),
        )
        .expect("event append");

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

    // Deregistering the space would orphan the target-only event, so a space the
    // target still holds event rows in stays registered.
    assert!(
        database
            .spaces(branch("default"))
            .expect("space service opens")
            .exists(&evspace)
            .expect("exists succeeds"),
        "a space with target-only event rows must stay registered"
    );
}

#[test]
fn events_are_compared_but_never_promoted() {
    let mut database = open_cache_database().expect("cache open succeeds");
    append_event(&mut database, "default", "seed");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");

    // Feature appends two events and changes a KV key; default is unchanged.
    append_event(&mut database, "feature", "a");
    append_event(&mut database, "feature", "b");
    database
        .kv(branch("feature"), space("default"))
        .expect("kv service opens")
        .put(key(b"k"), value(b"v"))
        .expect("kv put");

    // Compare reports the two feature-appended events as added on the event
    // capability; events are immutable, so nothing is modified or removed.
    let before = database
        .branches()
        .expect("branch service opens")
        .compare(
            &branch("default"),
            &branch("feature"),
            BranchStateSelector::Current,
        )
        .expect("compare succeeds");
    let events = event_diff(&before).expect("an event diff is present");
    assert_eq!(events.added().len(), 2, "feature appended two events");
    assert!(events.modified().is_empty() && events.removed().is_empty());

    // Promote feature → default applies the KV change but never touches events.
    let outcome = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect("promote succeeds");
    assert!(!outcome.is_noop(), "the KV change is promoted");
    assert!(
        outcome
            .applied()
            .iter()
            .all(|entity| entity.capability() != ComparedCapability::Event),
        "events are never promoted",
    );

    // After the promote the KV is in sync, but the events still differ — proof
    // the promotion left the event streams untouched.
    let after = database
        .branches()
        .expect("branch service opens")
        .compare(
            &branch("default"),
            &branch("feature"),
            BranchStateSelector::Current,
        )
        .expect("compare after promote");
    let events_after = event_diff(&after).expect("events still differ after promote");
    assert_eq!(events_after.added().len(), 2, "events were not promoted");
}
