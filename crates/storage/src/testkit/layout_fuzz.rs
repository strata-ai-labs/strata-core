//! L2 object-layout classification fuzzing (TCP3.3).
//!
//! L2 is a decode surface: object names are read back from the backend during
//! recovery listing and routed through the `classify_*` family to recover
//! their family, branch, segment/snapshot ids, and levels. Every other decoder
//! layer (L3 format, L5 table, L4 service) has a fuzz target; L2 did not, so a
//! malformed or adversarial object name discovered during a `list` could panic
//! or misroute recovery with no coverage.
//!
//! This routes arbitrary text through all L2 classifiers and asserts the
//! invariant that matters: **classification never panics** on any input, and
//! every canonical name a constructor produces round-trips back to the same
//! identity. Exposed only through the hidden testkit feature.

use crate::layout::ObjectLayout;
use crate::object::ObjectName;

/// How arbitrary bytes classified through the L2 object-layout surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutClassifyOutcome {
    /// The bytes were not a valid `ObjectName` (rejected before classify).
    InvalidName,
    /// A valid name that no family claimed (an unrecognised prefix).
    Unclassified,
    /// A valid name a family accepted as one of its objects.
    Classified,
    /// A valid name a family recognised by prefix but rejected as malformed
    /// (e.g. a non-canonical or wrongly-shaped object under a known family).
    RejectedShape,
}

/// Route arbitrary text through every L2 classifier. Returns how it landed and,
/// by completing without panicking, proves the classifiers tolerate arbitrary
/// input. This is the body a fuzz target drives with `String::from_utf8_lossy`.
pub fn classify_object_text(text: &str) -> LayoutClassifyOutcome {
    let Ok(name) = ObjectName::new(text) else {
        return LayoutClassifyOutcome::InvalidName;
    };

    // Each classifier returns Ok(Some(_)) when it claims the name, Ok(None)
    // when the name is not in its family, and Err when the name is in its
    // family but malformed. A name belongs to at most one family, so we fold
    // across all of them and report the strongest signal seen.
    let mut outcome = LayoutClassifyOutcome::Unclassified;
    let mut record = |result: crate::layout::LayoutResult<bool>| match result {
        Ok(true) => outcome = LayoutClassifyOutcome::Classified,
        Err(_) => {
            if outcome != LayoutClassifyOutcome::Classified {
                outcome = LayoutClassifyOutcome::RejectedShape;
            }
        }
        Ok(false) => {}
    };

    record(
        ObjectLayout::classify_manifest_object(&name)
            .map(|classification| classification.is_some()),
    );
    record(ObjectLayout::classify_wal_object(&name).map(|classification| classification.is_some()));
    record(
        ObjectLayout::classify_table_object(&name).map(|classification| classification.is_some()),
    );
    record(
        ObjectLayout::classify_snapshot_object(&name)
            .map(|classification| classification.is_some()),
    );
    record(
        ObjectLayout::classify_quarantine_object(&name)
            .map(|classification| classification.is_some()),
    );

    // Prefix predicates must also tolerate arbitrary names without panicking.
    let _ = ObjectLayout::has_wal_segment_prefix(&name);
    let _ = ObjectLayout::has_wal_segment_metadata_prefix(&name);

    outcome
}

/// Assert that every canonical name a constructor emits round-trips: the
/// classifier recovers the exact id/branch the constructor was given, for the
/// `u64`-id families (WAL segments, snapshots) where a decode bug would
/// silently misroute recovery to the wrong object. Returns the number of ids
/// checked so a caller can assert coverage was non-trivial.
pub fn assert_u64_id_roundtrips(ids: &[u64]) -> usize {
    let mut checked = 0;
    for &id in ids {
        let segment = ObjectLayout::wal_segment(id).expect("canonical wal segment name");
        match ObjectLayout::classify_wal_object(&segment).expect("wal classify") {
            Some(crate::layout::WalObjectClassification::Segment { segment_id }) => {
                assert_eq!(segment_id, id, "wal segment id did not round-trip");
            }
            other => panic!("canonical wal segment misclassified: {other:?}"),
        }

        let snapshot = ObjectLayout::snapshot(id).expect("canonical snapshot name");
        match ObjectLayout::classify_snapshot_object(&snapshot).expect("snapshot classify") {
            Some(crate::layout::SnapshotObjectClassification::Snapshot { snapshot_id }) => {
                assert_eq!(snapshot_id, id, "snapshot id did not round-trip");
            }
            other => panic!("canonical snapshot misclassified: {other:?}"),
        }
        checked += 2;
    }
    checked
}

#[cfg(test)]
mod tests {
    use super::{assert_u64_id_roundtrips, classify_object_text, LayoutClassifyOutcome};
    use crate::layout::ObjectLayout;

    #[test]
    fn canonical_names_classify_as_expected() {
        let snapshot = ObjectLayout::snapshot(7).expect("snapshot name");
        assert_eq!(
            classify_object_text(snapshot.as_str()),
            LayoutClassifyOutcome::Classified
        );
        let wal = ObjectLayout::wal_segment(3).expect("wal name");
        assert_eq!(
            classify_object_text(wal.as_str()),
            LayoutClassifyOutcome::Classified
        );
    }

    #[test]
    fn malformed_names_never_panic_and_report_their_shape() {
        // Empty / trailing-slash / oversized names are rejected before classify.
        assert_eq!(classify_object_text(""), LayoutClassifyOutcome::InvalidName);
        // A name under a known family prefix but wrongly shaped is a rejected
        // shape, not a classified object and not a crash.
        assert_eq!(
            classify_object_text("snapshots/not-a-u64"),
            LayoutClassifyOutcome::RejectedShape
        );
        // A name in no family is simply unclassified.
        assert_eq!(
            classify_object_text("totally/unknown/prefix"),
            LayoutClassifyOutcome::Unclassified
        );
        // Adversarial control/unicode bytes must not panic.
        for probe in [
            "\u{0}",
            "wal/\u{202e}",
            "tables/../escape",
            "snapshots/99999999999999999999999999",
        ] {
            let _ = classify_object_text(probe);
        }
    }

    #[test]
    fn ids_round_trip_through_the_classifiers() {
        let checked = assert_u64_id_roundtrips(&[0, 1, 42, u64::MAX, u64::MAX - 1]);
        assert_eq!(checked, 10, "5 ids x 2 families");
    }
}
