use super::*;

#[test]
fn visible_version_tracker_starts_at_zero_and_publishes_monotonically() {
    let mut tracker = VisibleVersionTracker::default();

    assert_eq!(tracker.visible_version(), CommitVersion::ZERO);
    assert_eq!(
        tracker
            .publish_visible(CommitVersion::new(7))
            .expect("advance visible"),
        VisibleVersionPublish::Advanced {
            previous: CommitVersion::ZERO,
            current: CommitVersion::new(7),
        }
    );
    assert_eq!(tracker.visible_version(), CommitVersion::new(7));
    assert_eq!(
        tracker
            .publish_visible(CommitVersion::new(7))
            .expect("idempotent publish"),
        VisibleVersionPublish::Unchanged {
            current: CommitVersion::new(7),
        }
    );
    assert_eq!(
        tracker.publish_visible(CommitVersion::new(6)),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "visible version cannot regress",
        })
    );
    assert_eq!(tracker.visible_version(), CommitVersion::new(7));
    assert_eq!(
        tracker
            .publish_visible(CommitVersion::new(11))
            .expect("second advance"),
        VisibleVersionPublish::Advanced {
            previous: CommitVersion::new(7),
            current: CommitVersion::new(11),
        }
    );
    assert_eq!(tracker.visible_version(), CommitVersion::new(11));
    assert_eq!(
        tracker.publish_visible(CommitVersion::ZERO),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "visible version cannot regress",
        })
    );
    assert_eq!(tracker.visible_version(), CommitVersion::new(11));
}

#[test]
fn visible_version_tracker_initializes_from_recovered_visible_version() {
    let empty = VisibleVersionTracker::default();
    let tracker = VisibleVersionTracker::new(CommitVersion::MAX);

    assert_eq!(empty.visible_version(), CommitVersion::ZERO);
    assert_eq!(tracker.visible_version(), CommitVersion::MAX);
}

#[test]
fn commit_visibility_fact_validation_covers_ordering_edges() {
    let v1 = CommitVersion::new(1);
    let v2 = CommitVersion::new(2);

    assert!(CommitVisibilityFacts::new(Some(v1), None, None, None, None).is_ok());
    assert!(CommitVisibilityFacts::new(Some(v1), Some(v1), Some(v1), Some(v1), Some(v1)).is_ok());
    assert_eq!(
        CommitVisibilityFacts::new(Some(v1), Some(v2), None, None, None),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "durable version must not exceed allocated version",
        })
    );
    assert_eq!(
        CommitVisibilityFacts::new(Some(v1), None, Some(v2), None, None),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "applied version must not exceed allocated version",
        })
    );
    assert_eq!(
        CommitVisibilityFacts::new(Some(v2), None, Some(v1), Some(v2), Some(v2)),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "visible version must not exceed applied version",
        })
    );
    assert_eq!(
        CommitVisibilityFacts::new(Some(v2), None, Some(v1), None, Some(v2)),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "timeline version must not exceed applied version",
        })
    );
    assert_eq!(
        CommitVisibilityFacts::new(Some(v2), None, Some(v2), Some(v2), Some(v1)),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "visible version must not exceed timeline version",
        })
    );
    assert_eq!(
        CommitVisibilityFacts::new(None, Some(v1), None, None, None),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "durable version must not exceed allocated version",
        })
    );
    assert_eq!(
        CommitVisibilityFacts::new(None, None, Some(v1), None, None),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "applied version must not exceed allocated version",
        })
    );
}

#[test]
fn visible_version_tracker_publishes_from_valid_visibility_facts_only() {
    let mut tracker = VisibleVersionTracker::default();
    let version = CommitVersion::new(9);
    let facts = CommitVisibilityFacts::new(
        Some(version),
        None,
        Some(version),
        Some(version),
        Some(version),
    )
    .expect("valid cache-visible facts");

    assert_eq!(
        tracker.publish_from_facts(facts).expect("publish facts"),
        VisibleVersionPublish::Advanced {
            previous: CommitVersion::ZERO,
            current: version,
        }
    );
    assert_eq!(tracker.visible_version(), version);
    assert_eq!(
        tracker
            .publish_from_facts(facts)
            .expect("idempotent facts publish"),
        VisibleVersionPublish::Unchanged { current: version }
    );
    assert_eq!(
        tracker.publish_from_facts(
            CommitVisibilityFacts::new(
                Some(CommitVersion::new(10)),
                None,
                Some(CommitVersion::new(10)),
                None,
                None,
            )
            .expect("applied but not visible")
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "visibility facts must include visible version",
        })
    );
    assert_eq!(
        tracker.publish_from_facts(
            CommitVisibilityFacts::new(
                Some(CommitVersion::new(8)),
                None,
                Some(CommitVersion::new(8)),
                Some(CommitVersion::new(8)),
                Some(CommitVersion::new(8)),
            )
            .expect("lower visible facts")
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "visible version cannot regress",
        })
    );
    assert_eq!(tracker.visible_version(), version);
}

#[test]
fn visible_version_replay_catch_up_uses_same_monotonic_rules() {
    let mut tracker = VisibleVersionTracker::new(CommitVersion::new(20));

    assert_eq!(
        tracker
            .catch_up_visible_after_replay(CommitVersion::new(25))
            .expect("replay catch-up"),
        VisibleVersionPublish::Advanced {
            previous: CommitVersion::new(20),
            current: CommitVersion::new(25),
        }
    );
    assert_eq!(
        tracker.catch_up_visible_after_replay(CommitVersion::new(24)),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "visible version cannot regress",
        })
    );
}
