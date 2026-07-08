//! Recovery replay for already-durable commit records.

use super::{
    CommitBranchApplyTarget, CommitDurabilityClass, CommitFactAllocator, CommitLowerLayer,
    CommitMutationCounts, CommitOutcome, CommitOutcomeKind, CommitPhase, CommitRuntimeConfig,
    CommitRuntimeError, CommitRuntimeResult, CommitStamp, CommitTimelineEntry, CommitTimelineFact,
    CommitTimelineRowKind, CommitUnresolvedDurable, CommitUnresolvedDurableGate,
    CommitVisibilityFacts, CommitVisiblePublisher,
};
use crate::branch::read::{BranchOwnRowMatch, BranchReadView};
use crate::format::WalRecord;
use crate::observability::perf_trace;
use crate::row::{StorageRow, StorageSpaceId};

#[derive(Debug)]
pub(crate) struct CommitReplayRuntime<'a, S, B, V> {
    config: &'a CommitRuntimeConfig,
    allocator: &'a mut CommitFactAllocator<S>,
    branch: &'a mut B,
    visible: &'a mut V,
    durable_gate: &'a CommitUnresolvedDurableGate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommitReplayRequest {
    record: WalRecord,
    durability: CommitDurabilityClass,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitReplayAction {
    Applied,
    AlreadyApplied,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommitReplayReport {
    action: CommitReplayAction,
    outcome: CommitOutcome,
    rows_checked: usize,
    rows_applied: usize,
    gate_cleared: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReplayDuplicateState {
    Absent,
    Exact,
    Mismatch,
    Partial,
}

impl<'a, S, B, V> CommitReplayRuntime<'a, S, B, V> {
    pub(crate) fn new(
        config: &'a CommitRuntimeConfig,
        allocator: &'a mut CommitFactAllocator<S>,
        branch: &'a mut B,
        visible: &'a mut V,
        durable_gate: &'a CommitUnresolvedDurableGate,
    ) -> Self {
        Self {
            config,
            allocator,
            branch,
            visible,
            durable_gate,
        }
    }
}

impl<S, B, V> CommitReplayRuntime<'_, S, B, V>
where
    B: CommitBranchApplyTarget,
    V: CommitVisiblePublisher,
{
    pub(crate) fn replay(
        &mut self,
        request: &CommitReplayRequest,
    ) -> CommitRuntimeResult<CommitReplayReport> {
        let read_view = self.branch.capture_read_view()?;
        self.replay_with_view(request, &read_view)
    }

    /// [`replay`](Self::replay) with a caller-provided read view. Capturing a
    /// view clones every owned-table reader handle plus construction
    /// validation — O(tables) per call — so a caller replaying a long WAL
    /// tail captures ONCE per branch and reuses it (replay-probe slice:
    /// per-record captures were ~169ms each at 1M scale with a table-heavy
    /// close state). A pre-replay view stays valid for every later record's
    /// duplicate classification: commit versions are unique and replayed in
    /// ascending order, so a record's (key, version) row can only already
    /// exist from a PRE-CRASH apply — never from an earlier record of this
    /// replay pass — and pre-crash state is exactly what the first capture
    /// observed.
    pub(crate) fn replay_with_view(
        &mut self,
        request: &CommitReplayRequest,
        read_view: &BranchReadView,
    ) -> CommitRuntimeResult<CommitReplayReport> {
        // Replay installs already-durable rows selected by the recovery phase.
        // It does not run normal mutating admission; callers that need
        // process-wide exclusion should quiesce normal commits before invoking
        // replay.
        self.config.validate()?;
        request.validate()?;
        if request.branch_id() != self.branch.branch_id() {
            return Err(CommitRuntimeError::BranchMismatch {
                expected: request.branch_id(),
                actual: self.branch.branch_id(),
            });
        }

        let matching_gate = self.matching_unresolved_gate(request)?;
        let rows = request.rows().to_vec();
        let duplicate_state = classify_replay_rows(read_view, &rows)?;
        let stamp = request.stamp()?;
        let counts = replay_mutation_counts(&rows)?;
        let facts = replay_visibility_facts(stamp)?;

        let action = match duplicate_state {
            ReplayDuplicateState::Absent => {
                self.apply_rows_or_record_gate(
                    rows.clone(),
                    stamp,
                    request.durability(),
                    matching_gate,
                )?;
                CommitReplayAction::Applied
            }
            ReplayDuplicateState::Exact => CommitReplayAction::AlreadyApplied,
            ReplayDuplicateState::Mismatch => {
                return Err(CommitRuntimeError::InvalidCommitState {
                    reason: "replay row conflicts with installed row facts",
                });
            }
            ReplayDuplicateState::Partial => {
                return Err(CommitRuntimeError::InvalidCommitState {
                    reason: "replay rows are only partially installed",
                });
            }
        };

        self.allocator
            .catch_up_to_recovered_version(stamp.commit_version());
        self.allocator
            .catch_up_to_recovered_timestamp(stamp.commit_timestamp());

        if let Err(source) = self.visible.publish_from_facts(facts) {
            self.record_applied_not_visible_if_needed(matching_gate, stamp, request.durability())?;
            return Err(CommitRuntimeError::durable_but_not_visible_with(
                request.branch_id(),
                request.commit_version(),
                "visible publication failed after replay apply",
                source,
            ));
        }

        let gate_cleared = if let Some(unresolved) = matching_gate {
            self.durable_gate.clear_exact(unresolved)?;
            true
        } else {
            false
        };

        let outcome = CommitOutcome::new(
            request.branch_id(),
            CommitOutcomeKind::Replay,
            CommitPhase::Replay,
            request.durability(),
            Some(stamp),
            counts,
            facts,
            None,
        )?;
        Ok(CommitReplayReport {
            action,
            outcome,
            rows_checked: rows.len(),
            rows_applied: if action == CommitReplayAction::Applied {
                rows.len()
            } else {
                0
            },
            gate_cleared,
        })
    }

    fn matching_unresolved_gate(
        &self,
        request: &CommitReplayRequest,
    ) -> CommitRuntimeResult<Option<CommitUnresolvedDurable>> {
        let Some(unresolved) = self.durable_gate.unresolved()? else {
            return Ok(None);
        };
        if unresolved.branch_id() == request.branch_id()
            && unresolved.commit_version() == request.commit_version()
            && unresolved.commit_timestamp() == request.commit_timestamp()
            && unresolved.durability() == request.durability()
        {
            return Ok(Some(unresolved));
        }
        // A write group's range fact (BS5.1 D1) covers first..=last: replays of the EARLIER
        // members in the range must proceed (without clearing the gate — only the range end's
        // replay clears it above), or recovery could never work through the group in version
        // order.
        if unresolved.covers_version(request.commit_version()) {
            return Ok(None);
        }
        Err(CommitRuntimeError::UnresolvedDurableCommit {
            branch_id: unresolved.branch_id(),
            commit_version: unresolved.commit_version(),
            reason: "different unresolved durable commit blocks replay",
        })
    }

    fn apply_rows_or_record_gate(
        &mut self,
        rows: Vec<StorageRow>,
        stamp: CommitStamp,
        durability: CommitDurabilityClass,
        matching_gate: Option<CommitUnresolvedDurable>,
    ) -> CommitRuntimeResult<()> {
        if let Err(source) = self.branch.append_committed_rows_atomically(rows) {
            let reason = "branch state rejected replay rows";
            if matching_gate.is_none() {
                self.durable_gate.record_unresolved(
                    CommitUnresolvedDurable::durable_not_applied_with_facts(
                        stamp, durability, reason,
                    )?,
                )?;
            }
            return Err(CommitRuntimeError::durable_but_not_visible_with(
                stamp.branch_id(),
                stamp.commit_version(),
                reason,
                source,
            ));
        }
        Ok(())
    }

    fn record_applied_not_visible_if_needed(
        &self,
        matching_gate: Option<CommitUnresolvedDurable>,
        stamp: CommitStamp,
        durability: CommitDurabilityClass,
    ) -> CommitRuntimeResult<()> {
        if matching_gate.is_none() {
            self.durable_gate
                .record_unresolved(CommitUnresolvedDurable::applied_not_visible(
                    stamp,
                    durability,
                    "visible publication failed after replay apply",
                )?)?;
        } else if let Some(unresolved) = matching_gate {
            let replacement = CommitUnresolvedDurable::applied_not_visible(
                stamp,
                durability,
                "visible publication failed after replay apply",
            )?;
            if unresolved != replacement {
                self.durable_gate.replace_exact(unresolved, replacement)?;
            }
        }
        Ok(())
    }
}

impl CommitReplayRequest {
    pub(crate) const fn new(record: WalRecord, durability: CommitDurabilityClass) -> Self {
        Self { record, durability }
    }

    pub(crate) const fn durability(&self) -> CommitDurabilityClass {
        self.durability
    }

    pub(crate) fn rows(&self) -> &[StorageRow] {
        self.record.commit_payload().rows()
    }

    pub(crate) fn branch_id(&self) -> strata_core_next::BranchId {
        self.record.branch_id()
    }

    pub(crate) fn commit_version(&self) -> strata_core_next::CommitVersion {
        self.record.commit_version()
    }

    pub(crate) fn commit_timestamp(&self) -> strata_core_next::Timestamp {
        self.record.commit_timestamp()
    }

    fn stamp(&self) -> CommitRuntimeResult<CommitStamp> {
        CommitStamp::new(
            self.branch_id(),
            self.commit_version(),
            self.commit_timestamp(),
        )
    }

    fn validate(&self) -> CommitRuntimeResult<()> {
        if !matches!(
            self.durability,
            CommitDurabilityClass::Standard | CommitDurabilityClass::Always
        ) {
            return Err(CommitRuntimeError::DurabilityUnavailable {
                reason: "replay requires a durable WAL record",
            });
        }
        self.stamp()?;
        validate_replay_rows(self.rows(), self.branch_id())?;
        validate_replay_timeline_rows(self.rows(), self.stamp()?)
    }
}

impl CommitReplayReport {
    pub(crate) const fn action(&self) -> CommitReplayAction {
        self.action
    }

    pub(crate) const fn outcome(&self) -> &CommitOutcome {
        &self.outcome
    }

    pub(crate) const fn rows_checked(&self) -> usize {
        self.rows_checked
    }

    pub(crate) const fn rows_applied(&self) -> usize {
        self.rows_applied
    }

    pub(crate) const fn gate_cleared(&self) -> bool {
        self.gate_cleared
    }
}

fn validate_replay_rows(
    rows: &[StorageRow],
    branch_id: strata_core_next::BranchId,
) -> CommitRuntimeResult<()> {
    let mut has_user_mutation = false;
    for row in rows {
        if row.physical_key().branch_id() != branch_id {
            return Err(CommitRuntimeError::BranchMismatch {
                expected: branch_id,
                actual: row.physical_key().branch_id(),
            });
        }
        let storage_space_id = row.physical_key().storage_space_id();
        if storage_space_id == StorageSpaceId::COMMIT_TIMELINE {
            continue;
        }
        has_user_mutation = true;
        if storage_space_id.is_storage_owned() {
            return Err(CommitRuntimeError::StorageOwnedMutationSpace {
                space_id: storage_space_id,
            });
        }
    }
    if !has_user_mutation {
        return Err(CommitRuntimeError::InvalidCommitState {
            reason: "replay payload is missing user mutation rows",
        });
    }
    validate_no_duplicate_internal_rows(rows)
}

fn same_internal_row(left: &StorageRow, right: &StorageRow) -> bool {
    left.physical_key() == right.physical_key() && left.commit_version() == right.commit_version()
}

fn validate_no_duplicate_internal_rows(rows: &[StorageRow]) -> CommitRuntimeResult<()> {
    for (index, left) in rows.iter().enumerate() {
        if rows
            .iter()
            .skip(index + 1)
            .any(|right| same_internal_row(left, right))
        {
            return Err(CommitRuntimeError::InvalidCommitState {
                reason: "replay payload contains duplicate internal rows",
            });
        }
    }
    Ok(())
}

fn validate_replay_timeline_rows(
    rows: &[StorageRow],
    stamp: CommitStamp,
) -> CommitRuntimeResult<()> {
    let expected = CommitTimelineEntry::from_stamp(stamp)?;
    let mut has_timestamp_to_version = false;
    let mut has_version_to_timestamp = false;

    for row in rows
        .iter()
        .filter(|row| row.physical_key().storage_space_id() == StorageSpaceId::COMMIT_TIMELINE)
    {
        let fact = CommitTimelineFact::validate(row)?;
        if fact.entry() != expected {
            return Err(CommitRuntimeError::TimelineConflict {
                reason: "replay timeline row does not match WAL commit facts",
            });
        }
        match fact.kind() {
            CommitTimelineRowKind::TimestampToVersion => {
                if has_timestamp_to_version {
                    return Err(CommitRuntimeError::TimelineConflict {
                        reason: "replay contains duplicate timestamp timeline row",
                    });
                }
                has_timestamp_to_version = true;
            }
            CommitTimelineRowKind::VersionToTimestamp => {
                if has_version_to_timestamp {
                    return Err(CommitRuntimeError::TimelineConflict {
                        reason: "replay contains duplicate version timeline row",
                    });
                }
                has_version_to_timestamp = true;
            }
        }
    }

    // W3.1c: commits no longer materialize timeline rows, so their absence
    // is the normal case. A LONE row of the pair (only possible in a corrupt
    // pre-elision record — pairs were written atomically) still fails closed.
    if has_timestamp_to_version != has_version_to_timestamp {
        return Err(CommitRuntimeError::InvalidTimelineFact {
            reason: "replay payload has a partial commit timeline row pair",
        });
    }
    Ok(())
}

fn classify_replay_rows(
    read_view: &BranchReadView,
    rows: &[StorageRow],
) -> CommitRuntimeResult<ReplayDuplicateState> {
    let mut exact = 0usize;
    let mut absent = 0usize;
    let mut rows_classified = 0usize;
    let mut history_calls = 0usize;
    let mut source_probes = 0usize;
    for row in rows {
        rows_classified = rows_classified.saturating_add(1);
        history_calls = history_calls.saturating_add(1);
        let row_state = match classify_replay_row(read_view, row) {
            Ok((state, row_source_probes)) => {
                source_probes = source_probes.saturating_add(row_source_probes);
                state
            }
            Err(error) => {
                perf_trace::record_commit_replay_classification(
                    rows_classified,
                    history_calls,
                    source_probes,
                );
                return Err(error);
            }
        };
        match row_state {
            ReplayDuplicateState::Exact => exact += 1,
            ReplayDuplicateState::Absent => absent += 1,
            ReplayDuplicateState::Mismatch => {
                perf_trace::record_commit_replay_classification(
                    rows_classified,
                    history_calls,
                    source_probes,
                );
                return Ok(ReplayDuplicateState::Mismatch);
            }
            ReplayDuplicateState::Partial => unreachable!("single row cannot be partial"),
        }
    }
    perf_trace::record_commit_replay_classification(rows_classified, history_calls, source_probes);
    match (exact, absent) {
        (0, _) => Ok(ReplayDuplicateState::Absent),
        (_, 0) => Ok(ReplayDuplicateState::Exact),
        _ => Ok(ReplayDuplicateState::Partial),
    }
}

fn classify_replay_row(
    read_view: &BranchReadView,
    row: &StorageRow,
) -> CommitRuntimeResult<(ReplayDuplicateState, usize)> {
    // Bounded idempotence probe (replay-probe slice): an exact
    // (physical key, commit version) lookup over the branch's OWN sources
    // with a first-equal early exit, replacing the full history walk that
    // made replay O(rows × sources × key versions). Inherited layers are
    // excluded by the probe itself — replay reconciles only this branch's
    // own WAL appends, and the walk it replaces filtered them out too.
    let (matched, source_probes) = read_view.classify_own_internal_row(row).map_err(|source| {
        CommitRuntimeError::lower_layer_with(
            CommitLowerLayer::BranchRuntime,
            "branch read view failed during replay duplicate classification",
            source,
        )
    })?;
    let state = match matched {
        BranchOwnRowMatch::Equal => ReplayDuplicateState::Exact,
        BranchOwnRowMatch::SameVersionDiffers => ReplayDuplicateState::Mismatch,
        BranchOwnRowMatch::Absent => ReplayDuplicateState::Absent,
    };
    Ok((state, source_probes))
}

fn replay_mutation_counts(rows: &[StorageRow]) -> CommitRuntimeResult<CommitMutationCounts> {
    let mut puts = 0usize;
    let mut deletes = 0usize;
    let mut timeline_rows = 0usize;
    for row in rows {
        if row.physical_key().storage_space_id() == StorageSpaceId::COMMIT_TIMELINE {
            timeline_rows =
                timeline_rows
                    .checked_add(1)
                    .ok_or(CommitRuntimeError::InvalidCommitState {
                        reason: "timeline row count overflow",
                    })?;
        } else if row.is_tombstone() {
            deletes = deletes
                .checked_add(1)
                .ok_or(CommitRuntimeError::InvalidCommitState {
                    reason: "delete count overflow",
                })?;
        } else {
            puts = puts
                .checked_add(1)
                .ok_or(CommitRuntimeError::InvalidCommitState {
                    reason: "put count overflow",
                })?;
        }
    }
    Ok(CommitMutationCounts::from_replayed_rows(
        puts,
        deletes,
        timeline_rows,
    ))
}

fn replay_visibility_facts(stamp: CommitStamp) -> CommitRuntimeResult<CommitVisibilityFacts> {
    CommitVisibilityFacts::new(
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
    )
}
