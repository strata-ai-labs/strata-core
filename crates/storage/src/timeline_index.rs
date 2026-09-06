//! W3.1a: the per-branch retained commit timeline index.
//!
//! An append-only index over the same facts the commit-timeline ROWS
//! materialize (`commit/timeline.rs`): one `(commit_version,
//! commit_timestamp)` entry per commit. The rows keep being written — this
//! index is a cache with an exactness contract, and every lookup that cannot
//! PROVE equivalence with a timeline-space scan returns `None` so the caller
//! falls back to the scan (which then seeds the index). W3.1c retires the
//! rows once the index owns recovery.
//!
//! Exactness argument: timeline rows are never pruned by retention (unique
//! keys, no tombstones, no expiry), so a branch's timeline content is fully
//! determined by its commit set. `complete` entries in version order bounded
//! by a view's max commit version therefore reproduce a scan of that view
//! exactly; binary search by timestamp additionally requires
//! `timestamps_monotonic` (commit timestamps are normally non-decreasing in
//! version order, but external timestamp bases may violate — such branches
//! permanently fall back).

use parking_lot::RwLock;
use std::sync::Arc;
use strata_core::{CommitVersion, Timestamp};

/// One retained commit: the same fact a `ts-v1`/`ver-v1` timeline row pair
/// encodes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RetainedTimelineEntry {
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
    /// The commit's wall-clock instant (#3112 S2b). `None` means unknown — a
    /// first-class, documented state (storage-format spec §10 req 12), not a
    /// defect: legacy pre-elision timeline rows and pre-`committed_at`
    /// checkpoint sections never carried it. It is NOT part of the index's
    /// ordering or dedup key — `commit_version` orders, and
    /// `commit_version`/`commit_timestamp` decide entry sameness — so knowing
    /// or not knowing it never changes which commits the index reports.
    committed_at: Option<Timestamp>,
}

impl RetainedTimelineEntry {
    pub(crate) const fn new(commit_version: CommitVersion, commit_timestamp: Timestamp) -> Self {
        Self {
            commit_version,
            commit_timestamp,
            committed_at: None,
        }
    }

    /// Attaches the commit's wall-clock instant. Kept as a builder so `new`'s
    /// call sites stay put (#3112 S2b).
    #[must_use]
    pub(crate) const fn with_committed_at(mut self, committed_at: Option<Timestamp>) -> Self {
        self.committed_at = committed_at;
        self
    }

    pub(crate) const fn committed_at(self) -> Option<Timestamp> {
        self.committed_at
    }

    pub(crate) const fn commit_version(self) -> CommitVersion {
        self.commit_version
    }

    pub(crate) const fn commit_timestamp(self) -> Timestamp {
        self.commit_timestamp
    }
}

/// A proven-exact lookup answer. Mirrors the commit-timeline view's miss
/// vocabulary so the API layer maps arms one-to-one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RetainedTimelineLookup {
    Matched(RetainedTimelineEntry),
    BeforeRetainedHistory,
    /// Matched the latest retained entry, but the query timestamp lies beyond
    /// it (the scan path reports this distinctly).
    AfterLatestRetained(RetainedTimelineEntry),
    Empty,
}

/// The index's answer for a version→timestamp lookup. `Unproven` means the
/// index cannot prove equivalence with a scan — the caller falls back.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RetainedVersionLookup {
    Unproven,
    Absent,
    Found(Timestamp),
}

#[derive(Debug)]
struct RetainedTimelineState {
    /// Entries in commit-version order (observation order at apply).
    entries: Vec<RetainedTimelineEntry>,
    /// Timestamps non-decreasing in version order; cleared permanently if a
    /// commit ever violates it (timestamp binary search becomes unsound).
    timestamps_monotonic: bool,
    /// Whether `entries` cover ALL retained history for the branch. False at
    /// construction (pre-open commits are unknown); set by the first
    /// scan-backed seed. In-process observations keep it current after that.
    complete: bool,
}

/// The shared per-branch index: observed at row apply, seeded from the first
/// fallback scan, shared into read views as an `Arc`.
#[derive(Debug)]
pub(crate) struct RetainedCommitTimeline {
    inner: RwLock<RetainedTimelineState>,
}

/// The index is a CACHE over facts derivable from branch rows — it carries no
/// state identity of its own, so any two indexes compare equal. This keeps
/// `BranchLocalState`'s derived equality (used by state-comparison tests)
/// independent of cache warmth.
impl PartialEq for RetainedCommitTimeline {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for RetainedCommitTimeline {}

impl RetainedCommitTimeline {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: RwLock::new(RetainedTimelineState {
                entries: Vec::new(),
                timestamps_monotonic: true,
                complete: false,
            }),
        })
    }

    /// Whether the index currently claims complete coverage (W3.1c: the
    /// recovery invariant checks this before deciding to scan-seed).
    pub(crate) fn is_complete(&self) -> bool {
        self.inner.read().complete
    }

    /// W3.1b oracle alias.
    #[cfg(any(test, feature = "testkit"))]
    pub(crate) fn is_complete_for_test(&self) -> bool {
        self.is_complete()
    }

    /// W3.1b: a branch created in-process starts with provably complete
    /// (empty) coverage — every later commit flows through the observation
    /// hook, so completeness holds by construction and checkpoints can
    /// persist the index without a scan ever having run. No-op unless the
    /// index is empty and unseeded (a recovered branch must NOT be marked).
    pub(crate) fn mark_complete_from_birth(&self) {
        let mut state = self.inner.write();
        if state.entries.is_empty() {
            state.complete = true;
        }
    }

    /// #2521/#2522: a forked branch restored from the catalog manifest must
    /// not keep `create_branch`'s complete-from-birth marking — "empty is
    /// exact" only holds for branches created from birth, and a false
    /// complete-empty index permanently erases the fork's inherited pre-fork
    /// coverage (`snapshot_entries` would return `Some([])`, defeating every
    /// downstream guard). Recovery re-completes it from the parent chain
    /// after WAL replay.
    pub(crate) fn mark_incomplete_for_fork_recovery(&self) {
        let mut state = self.inner.write();
        state.complete = false;
    }

    /// W3.1b: the persistable form — entries with version ≤ `bound`, in
    /// version order. `None` unless the index is complete (persisting a
    /// partial index would fabricate coverage at restore).
    pub(crate) fn snapshot_entries(
        &self,
        bound: CommitVersion,
    ) -> Option<Vec<RetainedTimelineEntry>> {
        let state = self.inner.read();
        if !state.complete {
            return None;
        }
        let end = state
            .entries
            .partition_point(|entry| entry.commit_version().as_u64() <= bound.as_u64());
        Some(state.entries[..end].to_vec())
    }

    /// Record one applied commit. Called once per timeline row that reaches
    /// the branch append funnel — the second row of a commit's pair (same
    /// version, same timestamp) is deduplicated here. Anything inconsistent
    /// (version regression, same version with a different timestamp) marks
    /// the index incomplete: lookups then fall back to the scan forever,
    /// which stays correct by construction.
    pub(crate) fn observe(&self, commit_version: CommitVersion, commit_timestamp: Timestamp) {
        let mut state = self.inner.write();
        if let Some(last) = state.entries.last().copied() {
            if commit_version.as_u64() <= last.commit_version().as_u64() {
                // Not an append: either a legitimate re-observation (the
                // pair's second row, or WAL replay re-applying commits a
                // restored checkpoint already covers — replay idempotence) or
                // an inconsistency. An exact match of an existing entry is a
                // no-op; anything else poisons completeness.
                let found = state
                    .entries
                    .binary_search_by_key(&commit_version.as_u64(), |entry| {
                        entry.commit_version().as_u64()
                    });
                match found {
                    Ok(at) if state.entries[at].commit_timestamp() == commit_timestamp => {}
                    Ok(_) | Err(_) => state.complete = false,
                }
                return;
            }
            if commit_timestamp < last.commit_timestamp() {
                state.timestamps_monotonic = false;
            }
        }
        state
            .entries
            .push(RetainedTimelineEntry::new(commit_version, commit_timestamp));
    }

    /// Seed from a timeline-space scan: `scanned` must be ALL retained
    /// entries of some view, in version order. Merges with in-process
    /// observations (which may extend past the scanned view) and marks the
    /// index complete. A version present in both with disagreeing timestamps
    /// keeps the index incomplete (corruption guard — the scan stays the
    /// source of truth).
    /// Attaches the wall-clock instant to an already-observed commit (#3112
    /// S2b). `committed_at` is commit-scoped and deliberately absent from rows
    /// (storage-format spec §10 req 13), so the row-driven apply funnel cannot
    /// carry it; the commit runtime — which holds the stamp — upgrades the
    /// entry here after the apply succeeds, and WAL replay does the same from
    /// the record.
    ///
    /// The rule is deliberately fail-soft and order-independent, because an
    /// unknown instant is a legal state while a wrong one is not:
    ///
    /// - unknown -> known upgrades (the stamp/WAL outranks an older checkpoint
    ///   section that predates the field),
    /// - known -> the same value is a no-op (replay idempotence),
    /// - known -> a DIFFERENT value is an inconsistency and poisons
    ///   completeness, exactly as a conflicting `commit_timestamp` does,
    /// - known -> unknown never downgrades,
    /// - an unobserved version is a no-op: the commit either never applied or
    ///   was rolled back, and inventing an entry here would forge a commit.
    pub(crate) fn observe_committed_at(&self, commit_version: CommitVersion, instant: Timestamp) {
        let mut state = self.inner.write();
        let Ok(at) = state
            .entries
            .binary_search_by_key(&commit_version.as_u64(), |entry| {
                entry.commit_version().as_u64()
            })
        else {
            return;
        };
        match state.entries[at].committed_at() {
            None => {
                state.entries[at] = state.entries[at].with_committed_at(Some(instant));
            }
            Some(existing) if existing == instant => {}
            Some(_) => state.complete = false,
        }
    }

    pub(crate) fn seed_from_scan(&self, scanned: &[RetainedTimelineEntry]) {
        let mut state = self.inner.write();
        let mut merged = Vec::with_capacity(scanned.len().saturating_add(state.entries.len()));
        let mut observed = state.entries.iter().copied().peekable();
        for entry in scanned {
            while let Some(next) = observed.peek().copied() {
                if next.commit_version().as_u64() < entry.commit_version().as_u64() {
                    merged.push(next);
                    observed.next();
                } else {
                    break;
                }
            }
            let mut entry = *entry;
            if let Some(next) = observed.peek().copied() {
                if next.commit_version() == entry.commit_version() {
                    if next.commit_timestamp() != entry.commit_timestamp() {
                        return; // corruption guard: stay incomplete
                    }
                    // #3112 S2b: a scan can never supply `committed_at` (legacy
                    // timeline rows never carried it), so an instant already
                    // observed from the commit stamp or a WAL record must
                    // survive the merge — otherwise seeding would silently
                    // downgrade a known instant to unknown.
                    entry = entry.with_committed_at(entry.committed_at().or(next.committed_at()));
                    observed.next();
                }
            }
            merged.push(entry);
        }
        merged.extend(observed);
        let timestamps_monotonic = merged
            .windows(2)
            .all(|pair| pair[0].commit_timestamp() <= pair[1].commit_timestamp());
        let versions_ascending = merged
            .windows(2)
            .all(|pair| pair[0].commit_version().as_u64() < pair[1].commit_version().as_u64());
        if !versions_ascending {
            return; // scan not in strict version order: stay incomplete
        }
        state.entries = merged;
        state.timestamps_monotonic = timestamps_monotonic;
        state.complete = true;
    }

    /// W3.1c: the version-bounded entries when the index is complete — the
    /// cold timeline surfaces materialize a full view from these. A bound
    /// above the tip serves the clamped provable prefix (#2853): those
    /// entries ARE the retained timeline; the shed suffix is not retained.
    pub(crate) fn materialized_entries(
        &self,
        version_bound: Option<CommitVersion>,
    ) -> Option<Vec<RetainedTimelineEntry>> {
        let state = self.inner.read();
        if !state.complete {
            return None;
        }
        Some(bounded_prefix(&state.entries, version_bound).to_vec())
    }

    /// The largest-timestamp entry at or before `query_timestamp`, among
    /// entries with version ≤ `version_bound` (`None` = no bound: serve the
    /// index tip, valid for live-memtable views where the scan itself races
    /// forward). Returns `None` when exactness cannot be proven — caller
    /// falls back to the scan.
    pub(crate) fn lookup_at_or_before(
        &self,
        query_timestamp: Timestamp,
        version_bound: Option<CommitVersion>,
    ) -> Option<RetainedTimelineLookup> {
        let state = self.inner.read();
        if !state.complete || !state.timestamps_monotonic {
            return None;
        }
        let prefix = bounded_prefix(&state.entries, version_bound);
        let Some(first) = prefix.first() else {
            return Some(RetainedTimelineLookup::Empty);
        };
        if query_timestamp < first.commit_timestamp() {
            return Some(RetainedTimelineLookup::BeforeRetainedHistory);
        }
        let upper = prefix.partition_point(|entry| entry.commit_timestamp() <= query_timestamp);
        let matched = prefix[upper.saturating_sub(1)];
        let latest = *prefix.last().expect("non-empty prefix has a latest entry");
        Some(if query_timestamp > latest.commit_timestamp() {
            RetainedTimelineLookup::AfterLatestRetained(matched)
        } else {
            RetainedTimelineLookup::Matched(matched)
        })
    }

    /// The timestamp recorded for `version` among entries with version ≤
    /// `version_bound`. Outer `None` = fall back to the scan; inner `None` =
    /// proven absent from retained history.
    pub(crate) fn timestamp_for_version(
        &self,
        version: CommitVersion,
        version_bound: Option<CommitVersion>,
    ) -> RetainedVersionLookup {
        let state = self.inner.read();
        if !state.complete {
            return RetainedVersionLookup::Unproven;
        }
        // #2853: a version above the index tip may be a legally-shed mapping
        // (its rows exist; only the timeline fact is gone) — that is
        // unavailability, never proven absence.
        let tip = state
            .entries
            .last()
            .map_or(0, |entry| entry.commit_version().as_u64());
        if version.as_u64() > tip {
            return RetainedVersionLookup::Unproven;
        }
        let prefix = bounded_prefix(&state.entries, version_bound);
        match prefix
            .binary_search_by_key(&version.as_u64(), |entry| entry.commit_version().as_u64())
        {
            Ok(at) => RetainedVersionLookup::Found(prefix[at].commit_timestamp()),
            Err(_) => RetainedVersionLookup::Absent,
        }
    }
}

/// The version-bounded prefix of the version-ordered entries, CLAMPED to the
/// index tip. A view bound above the tip means the view holds content whose
/// (version→timestamp) facts were legally shed (flush-published rows outlive
/// WAL'd timeline facts after a lossy crash — DUR-011): retained history then
/// SHRINKS to the provable prefix, it does not vanish (#2853). Lookups past
/// the tip stay unavailable through each surface's own arm.
fn bounded_prefix(
    entries: &[RetainedTimelineEntry],
    version_bound: Option<CommitVersion>,
) -> &[RetainedTimelineEntry] {
    match version_bound {
        None => entries,
        Some(bound) => {
            let end =
                entries.partition_point(|entry| entry.commit_version().as_u64() <= bound.as_u64());
            &entries[..end]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #3112 S2b: the `observe_committed_at` upgrade rule, arm by arm. The rule
    /// is deliberately fail-soft — an unknown instant is legal, a wrong one is
    /// not — so each arm is pinned separately.
    #[test]
    fn observe_committed_at_upgrades_unknown_to_known() {
        let index = RetainedCommitTimeline::new();
        index.mark_complete_from_birth();
        index.observe(CommitVersion::new(1), Timestamp::from_micros(10));
        assert_eq!(
            index.materialized_entries(None).expect("complete")[0].committed_at(),
            None,
            "the row-driven funnel cannot supply the instant"
        );

        index.observe_committed_at(CommitVersion::new(1), Timestamp::from_micros(1_788_000));
        let entries = index.materialized_entries(None).expect("still complete");
        assert_eq!(
            entries[0].committed_at(),
            Some(Timestamp::from_micros(1_788_000))
        );
        // The upgrade must not disturb the identity fields or completeness.
        assert_eq!(entries[0].commit_version(), CommitVersion::new(1));
        assert_eq!(entries[0].commit_timestamp(), Timestamp::from_micros(10));
    }

    #[test]
    fn observe_committed_at_is_idempotent_for_the_same_instant() {
        // Replay re-applies commits a restored checkpoint already covers, so
        // re-observing the SAME instant must be a no-op, not a conflict.
        let index = RetainedCommitTimeline::new();
        index.mark_complete_from_birth();
        index.observe(CommitVersion::new(1), Timestamp::from_micros(10));
        index.observe_committed_at(CommitVersion::new(1), Timestamp::from_micros(1_788_000));
        index.observe_committed_at(CommitVersion::new(1), Timestamp::from_micros(1_788_000));

        let entries = index.materialized_entries(None).expect("still complete");
        assert_eq!(
            entries[0].committed_at(),
            Some(Timestamp::from_micros(1_788_000))
        );
    }

    #[test]
    fn observe_committed_at_poisons_completeness_on_a_conflicting_instant() {
        // Two different instants for one commit is an inconsistency, handled
        // exactly like a conflicting commit_timestamp: stop claiming exactness.
        let index = RetainedCommitTimeline::new();
        index.mark_complete_from_birth();
        index.observe(CommitVersion::new(1), Timestamp::from_micros(10));
        index.observe_committed_at(CommitVersion::new(1), Timestamp::from_micros(1_788_000));
        index.observe_committed_at(CommitVersion::new(1), Timestamp::from_micros(1_999_000));

        assert!(
            !index.is_complete_for_test(),
            "a conflicting instant must poison completeness"
        );
    }

    #[test]
    fn observe_committed_at_never_forges_an_unobserved_commit() {
        // The commit either never applied or was rolled back; inventing an
        // entry here would fabricate a commit the timeline never saw.
        let index = RetainedCommitTimeline::new();
        index.mark_complete_from_birth();
        index.observe(CommitVersion::new(1), Timestamp::from_micros(10));
        index.observe_committed_at(CommitVersion::new(7), Timestamp::from_micros(1_788_000));

        let entries = index.materialized_entries(None).expect("still complete");
        assert_eq!(entries.len(), 1, "no entry may be forged");
        assert_eq!(entries[0].commit_version(), CommitVersion::new(1));
    }

    #[test]
    fn seeding_from_a_scan_preserves_an_already_known_instant() {
        // A scan reads legacy timeline rows, which never carried `committed_at`.
        // Merging one over a live-observed commit must NOT downgrade a known
        // instant to unknown — the seed is a completeness repair, not a source
        // of truth for the wall clock.
        let index = RetainedCommitTimeline::new();
        index.mark_complete_from_birth();
        index.observe(CommitVersion::new(1), Timestamp::from_micros(10));
        index.observe_committed_at(CommitVersion::new(1), Timestamp::from_micros(1_788_000));

        index.seed_from_scan(&[entry(1, 10), entry(2, 20)]);

        let entries = index
            .materialized_entries(None)
            .expect("complete after seed");
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0].committed_at(),
            Some(Timestamp::from_micros(1_788_000)),
            "the scan must not erase an instant it cannot supply"
        );
        assert_eq!(
            entries[1].committed_at(),
            None,
            "a scan-only commit stays unknown"
        );
    }

    fn entry(version: u64, timestamp: u64) -> RetainedTimelineEntry {
        RetainedTimelineEntry::new(
            CommitVersion::new(version),
            Timestamp::from_micros(timestamp),
        )
    }

    fn seeded(entries: &[RetainedTimelineEntry]) -> Arc<RetainedCommitTimeline> {
        let index = RetainedCommitTimeline::new();
        index.seed_from_scan(entries);
        index
    }

    #[test]
    fn unseeded_index_always_falls_back() {
        let index = RetainedCommitTimeline::new();
        index.observe(CommitVersion::new(1), Timestamp::from_micros(10));
        assert_eq!(
            index.lookup_at_or_before(Timestamp::from_micros(10), None),
            None
        );
        assert_eq!(
            index.timestamp_for_version(CommitVersion::new(1), None),
            RetainedVersionLookup::Unproven
        );
    }

    #[test]
    fn seeded_lookup_matches_scan_semantics() {
        let index = seeded(&[entry(1, 10), entry(2, 20), entry(3, 30)]);
        assert_eq!(
            index.lookup_at_or_before(Timestamp::from_micros(5), None),
            Some(RetainedTimelineLookup::BeforeRetainedHistory)
        );
        assert_eq!(
            index.lookup_at_or_before(Timestamp::from_micros(20), None),
            Some(RetainedTimelineLookup::Matched(entry(2, 20)))
        );
        assert_eq!(
            index.lookup_at_or_before(Timestamp::from_micros(25), None),
            Some(RetainedTimelineLookup::Matched(entry(2, 20)))
        );
        assert_eq!(
            index.lookup_at_or_before(Timestamp::from_micros(31), None),
            Some(RetainedTimelineLookup::AfterLatestRetained(entry(3, 30)))
        );
    }

    #[test]
    fn version_bound_excludes_newer_entries_and_unproven_bounds() {
        let index = seeded(&[entry(1, 10), entry(2, 20), entry(3, 30)]);
        // Bounded at version 2: entry 3 is invisible.
        assert_eq!(
            index.lookup_at_or_before(Timestamp::from_micros(30), Some(CommitVersion::new(2))),
            Some(RetainedTimelineLookup::AfterLatestRetained(entry(2, 20)))
        );
        assert_eq!(
            index.timestamp_for_version(CommitVersion::new(3), Some(CommitVersion::new(2))),
            RetainedVersionLookup::Absent
        );
        // #2853: a bound beyond the tip serves the clamped provable prefix —
        // retained history SHRINKS past the tip (shed facts are not retained),
        // it does not vanish. The query at the tip's own timestamp matches it.
        assert_eq!(
            index.lookup_at_or_before(Timestamp::from_micros(30), Some(CommitVersion::new(9))),
            Some(RetainedTimelineLookup::Matched(entry(3, 30)))
        );
    }

    /// #2853 truth table: a view bound above the index tip (content outlived
    /// the shed timeline facts) serves the clamped prefix on every surface,
    /// and a shed version's mapping is unavailability, never proven absence.
    #[test]
    fn bound_above_tip_serves_the_clamped_prefix() {
        let index = seeded(&[entry(1, 10), entry(3, 30)]);
        let bound = Some(CommitVersion::new(9));

        assert_eq!(
            index.materialized_entries(bound),
            Some(vec![entry(1, 10), entry(3, 30)]),
            "the retained timeline is the provable prefix, not empty",
        );
        assert_eq!(
            index.lookup_at_or_before(Timestamp::from_micros(15), bound),
            Some(RetainedTimelineLookup::Matched(entry(1, 10))),
        );
        assert_eq!(
            index.lookup_at_or_before(Timestamp::from_micros(45), bound),
            Some(RetainedTimelineLookup::AfterLatestRetained(entry(3, 30))),
            "past the tip's timestamp stays the after-latest refusal shape",
        );
        assert_eq!(
            index.lookup_at_or_before(Timestamp::from_micros(5), bound),
            Some(RetainedTimelineLookup::BeforeRetainedHistory),
        );

        // Version→timestamp: within the tip, Found/Absent stay proven; above
        // the tip the mapping may be legally shed — Unproven, never Absent.
        assert_eq!(
            index.timestamp_for_version(CommitVersion::new(3), bound),
            RetainedVersionLookup::Found(Timestamp::from_micros(30)),
        );
        assert_eq!(
            index.timestamp_for_version(CommitVersion::new(2), bound),
            RetainedVersionLookup::Absent,
            "a gap inside the covered prefix is proven absent",
        );
        assert_eq!(
            index.timestamp_for_version(CommitVersion::new(5), bound),
            RetainedVersionLookup::Unproven,
            "a shed version above the tip is unavailable, not absent",
        );
    }

    #[test]
    fn observation_extends_a_seeded_index_and_pairs_deduplicate() {
        let index = seeded(&[entry(1, 10)]);
        index.observe(CommitVersion::new(2), Timestamp::from_micros(20));
        index.observe(CommitVersion::new(2), Timestamp::from_micros(20));
        assert_eq!(
            index.lookup_at_or_before(Timestamp::from_micros(20), None),
            Some(RetainedTimelineLookup::Matched(entry(2, 20)))
        );
        assert_eq!(
            index.timestamp_for_version(CommitVersion::new(2), None),
            RetainedVersionLookup::Found(Timestamp::from_micros(20))
        );
    }

    #[test]
    fn non_monotonic_timestamps_fall_back_but_version_lookup_survives() {
        let index = seeded(&[entry(1, 100)]);
        index.observe(CommitVersion::new(2), Timestamp::from_micros(50));
        assert_eq!(
            index.lookup_at_or_before(Timestamp::from_micros(60), None),
            None
        );
        assert_eq!(
            index.timestamp_for_version(CommitVersion::new(2), None),
            RetainedVersionLookup::Found(Timestamp::from_micros(50))
        );
    }

    #[test]
    fn version_regression_and_timestamp_disagreement_poison_completeness() {
        let index = seeded(&[entry(5, 50)]);
        index.observe(CommitVersion::new(4), Timestamp::from_micros(40));
        assert_eq!(
            index.lookup_at_or_before(Timestamp::from_micros(50), None),
            None
        );

        let index = seeded(&[entry(5, 50)]);
        index.observe(CommitVersion::new(5), Timestamp::from_micros(51));
        assert_eq!(
            index.lookup_at_or_before(Timestamp::from_micros(50), None),
            None
        );
    }

    /// WAL replay re-applies commits a restored checkpoint already covers:
    /// exact re-observations must be no-ops, not poison.
    #[test]
    fn replay_reobservation_of_covered_commits_keeps_completeness() {
        let index = seeded(&[entry(1, 10), entry(2, 20), entry(3, 30)]);
        index.observe(CommitVersion::new(1), Timestamp::from_micros(10));
        index.observe(CommitVersion::new(3), Timestamp::from_micros(30));
        index.observe(CommitVersion::new(4), Timestamp::from_micros(40));
        assert_eq!(
            index.lookup_at_or_before(Timestamp::from_micros(40), None),
            Some(RetainedTimelineLookup::Matched(entry(4, 40)))
        );
        // A non-matching historical observation still poisons.
        index.observe(CommitVersion::new(2), Timestamp::from_micros(99));
        assert_eq!(
            index.lookup_at_or_before(Timestamp::from_micros(40), None),
            None
        );
    }

    #[test]
    fn seed_merges_scan_with_newer_observations() {
        let index = RetainedCommitTimeline::new();
        index.observe(CommitVersion::new(3), Timestamp::from_micros(30));
        index.seed_from_scan(&[entry(1, 10), entry(2, 20)]);
        assert_eq!(
            index.lookup_at_or_before(Timestamp::from_micros(30), None),
            Some(RetainedTimelineLookup::Matched(entry(3, 30)))
        );
    }
}
