//! Recovery-oracle prefix verifier.
//!
//! `classify_recovered` is pure (model + a reconstructed recovered map → a typed
//! violation) so it is exhaustively unit-testable; `scan_recovered` performs the
//! reopened-runtime enumeration the driver feeds into it.

use std::collections::BTreeMap;

use strata_core::{BranchId, CommitVersion};

use super::model::{ExpectedState, RecordedMutation, StateKey};
use crate::api::{
    PrefixScanReadRequest, ReadBound, ReadLimit, StorageKey, StorageRuntime, StorageSpaceId,
    StorageValue,
};
use crate::testkit::TestkitError;

/// Which durability guarantee the crash mechanism preserves. A zero-loss crash
/// (clean `drop`, backend op fault) must recover every acknowledged commit; an
/// on-disk-damage crash may lose a contiguous un-fsynced *suffix*.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CrashFamily {
    ZeroLoss,
    OnDiskDamage,
}

/// A way the recovered database failed to be a prefix of acknowledged history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryOracleViolation {
    /// An acknowledged commit was lost although the crash guarantees zero loss.
    LostAck {
        branch: BranchId,
        expected_through: CommitVersion,
        recovered_watermark: CommitVersion,
    },
    /// A recovered value was never produced, or a deleted key was resurrected.
    Phantom {
        branch: BranchId,
        key: StorageKey,
        recovered_version: CommitVersion,
    },
    /// A commit was only partially applied (some mutations present, some absent).
    TornBatch {
        branch: BranchId,
        version: CommitVersion,
    },
    /// A commit below the recovered watermark left no trace (a hole in history).
    Gap {
        branch: BranchId,
        missing_version: CommitVersion,
    },
}

/// The live `(space, key) -> (value, version)` state read back after recovery.
pub(crate) type RecoveredState = BTreeMap<StateKey, (StorageValue, CommitVersion)>;

/// Enumerate the recovered live state under `prefix` (the shared workload key
/// prefix) at `Latest`. Fails loudly if the scan is truncated by `limit`.
pub(crate) fn scan_recovered(
    runtime: &StorageRuntime<'_>,
    branch: BranchId,
    space: &StorageSpaceId,
    prefix: &StorageKey,
    limit: usize,
) -> Result<RecoveredState, TestkitError> {
    let read_limit =
        ReadLimit::new(limit).map_err(|err| TestkitError::new(format!("read limit: {err:?}")))?;
    let outcome = runtime
        .scan_prefix(&PrefixScanReadRequest::new(
            branch,
            space.clone(),
            prefix.clone(),
            ReadBound::Latest,
            Some(read_limit),
        ))
        .map_err(|err| TestkitError::new(format!("recovered scan: {err:?}")))?;
    let rows = outcome.rows();
    if rows.len() >= limit {
        return Err(TestkitError::new(format!(
            "recovered scan hit the {limit}-row limit; widen it so enumeration is complete"
        )));
    }
    let mut recovered = RecoveredState::new();
    for row in rows {
        if row.is_tombstone() {
            continue;
        }
        if let Some(value) = row.value() {
            recovered.insert(
                (row.storage_space().clone(), row.key().clone()),
                (value.clone(), row.commit_version()),
            );
        }
    }
    Ok(recovered)
}

fn produced_value_at(
    model: &ExpectedState,
    branch: BranchId,
    version: CommitVersion,
    state_key: &StateKey,
    value: &StorageValue,
) -> bool {
    model
        .mutations_at(branch, version)
        .is_some_and(|mutations| {
            mutations.iter().any(|mutation| match mutation {
                RecordedMutation::Put {
                    space,
                    key,
                    value: put,
                } => space == &state_key.0 && key == &state_key.1 && put == value,
                RecordedMutation::Delete { .. } => false,
            })
        })
}

/// The core prefix check: recovered state must equal the model's live state at
/// some acknowledged watermark `W`, and `W` must reach the last acknowledged
/// version under a zero-loss crash. Any deviation is a typed violation.
pub(crate) fn classify_recovered(
    model: &ExpectedState,
    branch: BranchId,
    recovered: &RecoveredState,
    family: CrashFamily,
) -> Result<(), RecoveryOracleViolation> {
    let upper = model.max_version(branch).unwrap_or(CommitVersion::ZERO);

    // Find the greatest watermark whose live state exactly equals recovered.
    if let Some(watermark) = model
        .candidate_watermarks(branch, upper)
        .into_iter()
        .find(|&candidate| &model.live_state_at(branch, candidate) == recovered)
    {
        let acked = model
            .last_acked_version(branch)
            .unwrap_or(CommitVersion::ZERO);
        if family == CrashFamily::ZeroLoss && watermark < acked {
            return Err(RecoveryOracleViolation::LostAck {
                branch,
                expected_through: acked,
                recovered_watermark: watermark,
            });
        }
        return Ok(());
    }

    // No clean prefix matches — diagnose the deviation against the top-of-history
    // expectation (everything up to the highest recovered version).
    let recovered_max = recovered
        .values()
        .map(|(_, version)| *version)
        .max()
        .unwrap_or(CommitVersion::ZERO);
    let full = model.state_at(branch, recovered_max);

    // Phantom: a value never produced at its version, or a resurrected delete.
    for (state_key, (value, version)) in recovered {
        match full.get(state_key) {
            Some((Some(expected), expected_version))
                if expected == value && *expected_version == *version => {}
            Some((None, _)) => {
                return Err(RecoveryOracleViolation::Phantom {
                    branch,
                    key: state_key.1.clone(),
                    recovered_version: *version,
                });
            }
            _ => {
                if !produced_value_at(model, branch, *version, state_key, value) {
                    return Err(RecoveryOracleViolation::Phantom {
                        branch,
                        key: state_key.1.clone(),
                        recovered_version: *version,
                    });
                }
            }
        }
    }

    // Gap vs torn: the lowest version whose top-of-history effect is missing.
    let expected_live = model.live_state_at(branch, recovered_max);
    let mut culprits: Vec<CommitVersion> = expected_live
        .iter()
        .filter(|(state_key, expected)| recovered.get(*state_key) != Some(*expected))
        .map(|(_, (_, version))| *version)
        .collect();
    culprits.sort_unstable();
    culprits.dedup();
    let culprit = culprits.first().copied().unwrap_or(recovered_max);

    let boundary_partially_present = expected_live.iter().any(|(state_key, (_, version))| {
        *version == recovered_max && recovered.contains_key(state_key)
    });
    if culprit == recovered_max
        && boundary_partially_present
        && model.ack_state_at(branch, recovered_max).is_some()
    {
        Err(RecoveryOracleViolation::TornBatch {
            branch,
            version: recovered_max,
        })
    } else {
        Err(RecoveryOracleViolation::Gap {
            branch,
            missing_version: culprit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{classify_recovered, CrashFamily, RecoveredState, RecoveryOracleViolation};
    use crate::api::{StorageKey, StorageSpaceId, StorageValue};
    use crate::testkit::recovery_oracle::model::{
        ExpectedState, OracleDurability, RecordedMutation,
    };
    use strata_core::{BranchId, CommitVersion};

    fn space() -> StorageSpaceId {
        StorageSpaceId::new(vec![0x20]).expect("space")
    }
    fn key(byte: u8) -> StorageKey {
        StorageKey::new(vec![0x6b, byte]).expect("key")
    }
    fn value(byte: u8) -> StorageValue {
        StorageValue::new(vec![byte])
    }
    fn branch() -> BranchId {
        BranchId::from_bytes([1; BranchId::BYTE_LEN])
    }
    fn put(k: u8, v: u8) -> RecordedMutation {
        RecordedMutation::Put {
            space: space(),
            key: key(k),
            value: value(v),
        }
    }
    fn del(k: u8) -> RecordedMutation {
        RecordedMutation::Delete {
            space: space(),
            key: key(k),
        }
    }
    fn recovered(entries: &[(u8, u8, u64)]) -> RecoveredState {
        entries
            .iter()
            .map(|(k, v, ver)| ((space(), key(*k)), (value(*v), CommitVersion::new(*ver))))
            .collect()
    }

    fn model_v1_v2() -> ExpectedState {
        let mut model = ExpectedState::new(OracleDurability::Standard);
        model.record_ack(branch(), CommitVersion::new(1), vec![put(0, 10)]);
        model.record_ack(branch(), CommitVersion::new(2), vec![put(1, 11)]);
        model
    }

    #[test]
    fn clean_prefix_passes_under_both_families() {
        let model = model_v1_v2();
        let full = recovered(&[(0, 10, 1), (1, 11, 2)]);
        assert_eq!(
            classify_recovered(&model, branch(), &full, CrashFamily::ZeroLoss),
            Ok(())
        );
        // a prefix at v1 is acceptable for the damage family (lost suffix)
        let prefix = recovered(&[(0, 10, 1)]);
        assert_eq!(
            classify_recovered(&model, branch(), &prefix, CrashFamily::OnDiskDamage),
            Ok(())
        );
    }

    #[test]
    fn lost_acknowledged_commit_under_zero_loss_is_lost_ack() {
        let model = model_v1_v2();
        let prefix = recovered(&[(0, 10, 1)]); // missing acked v2
        assert_eq!(
            classify_recovered(&model, branch(), &prefix, CrashFamily::ZeroLoss),
            Err(RecoveryOracleViolation::LostAck {
                branch: branch(),
                expected_through: CommitVersion::new(2),
                recovered_watermark: CommitVersion::new(1),
            })
        );
    }

    #[test]
    fn never_written_value_is_phantom() {
        let model = model_v1_v2();
        let bogus = recovered(&[(0, 10, 1), (1, 11, 2), (2, 99, 2)]); // key 2 never written
        assert!(matches!(
            classify_recovered(&model, branch(), &bogus, CrashFamily::ZeroLoss),
            Err(RecoveryOracleViolation::Phantom { .. })
        ));
    }

    #[test]
    fn resurrected_delete_is_phantom() {
        let mut model = ExpectedState::new(OracleDurability::Standard);
        model.record_ack(branch(), CommitVersion::new(1), vec![put(0, 10)]);
        model.record_ack(branch(), CommitVersion::new(2), vec![del(0), put(1, 11)]);
        // recovered reflects v2 (key1 present) but key0 was resurrected
        let resurrected = recovered(&[(0, 10, 1), (1, 11, 2)]);
        assert!(matches!(
            classify_recovered(&model, branch(), &resurrected, CrashFamily::ZeroLoss),
            Err(RecoveryOracleViolation::Phantom { .. })
        ));
    }

    #[test]
    fn partial_commit_is_torn_batch() {
        let mut model = ExpectedState::new(OracleDurability::Standard);
        model.record_ack(
            branch(),
            CommitVersion::new(1),
            vec![put(0, 10), put(1, 11)],
        );
        let half = recovered(&[(0, 10, 1)]); // key1 from the same batch missing
        assert_eq!(
            classify_recovered(&model, branch(), &half, CrashFamily::ZeroLoss),
            Err(RecoveryOracleViolation::TornBatch {
                branch: branch(),
                version: CommitVersion::new(1),
            })
        );
    }

    #[test]
    fn hole_in_history_is_gap() {
        let mut model = ExpectedState::new(OracleDurability::Standard);
        model.record_ack(branch(), CommitVersion::new(1), vec![put(0, 10)]);
        model.record_ack(branch(), CommitVersion::new(2), vec![put(1, 11)]);
        model.record_ack(branch(), CommitVersion::new(3), vec![put(2, 12)]);
        let holed = recovered(&[(0, 10, 1), (2, 12, 3)]); // v2 missing, v3 present
        assert_eq!(
            classify_recovered(&model, branch(), &holed, CrashFamily::ZeroLoss),
            Err(RecoveryOracleViolation::Gap {
                branch: branch(),
                missing_version: CommitVersion::new(2),
            })
        );
    }
}
